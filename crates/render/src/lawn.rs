//! El césped de las plazas: sobre la losa de color del césped de cada plaza
//! ([`crate::town`]) va una capa con el shader del suelo
//! (`assets/shaders/ground.wesl`, [`GroundMaterial`]), así que la plaza muestra
//! el mismo pasto fotografiado de 2K que la pampa, con su relieve, sus matas y
//! algún manchón pelado, en un verde más vivo: el de un césped regado frente a
//! la municipalidad.
//!
//! La capa es un quad plano apenas por encima de la losa (el pueblo está
//! nivelado); los senderos, el monumento y los bancos de la plaza quedan por
//! encima de ella. Se agrega a cada pueblo cuando aparece ([`TownRoot`]), como
//! hija suya, y se va con él.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use pampa_core::Aabb;

use crate::AssetRoot;
use crate::coords::world_from_ground;
use crate::facade::plaza::LAWN_TOP;
use crate::ground::{GroundMaterial, face_tangent, ground_material, ground_uv};
use crate::settlement::ranges;
use crate::terrain::WorldTerrain;
use crate::town::{StreamTowns, TownRoot, WorldTowns};

/// Cuánto sube la capa de pasto sobre la losa del césped, en metros.
pub const LAWN_LIFT: f32 = 0.002;
/// El verde del césped (sRGB): la pintura que el shader del suelo lee como
/// pasto verde, así que los manchones pelados quedan raros (los de la pampa
/// crecen con la sequedad).
pub const LAWN: [f32; 3] = [0.42, 0.48, 0.25];

/// La capa de pasto del césped de una plaza, hija de la raíz de su pueblo.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlazaLawn;

/// Siembra el césped de las plazas de cada pueblo que aparece.
pub struct LawnPlugin;

impl Plugin for LawnPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Assets<GroundMaterial>>()
            .add_systems(Startup, setup_lawn)
            .add_systems(PreUpdate, sow_lawns.after(StreamTowns));
    }
}

/// El material del césped, compartido por todas las plazas.
#[derive(Resource)]
struct LawnKit(Handle<GroundMaterial>);

fn setup_lawn(
    mut commands: Commands,
    root: Option<Res<AssetRoot>>,
    server: Option<Res<AssetServer>>,
    mut materials: ResMut<Assets<GroundMaterial>>,
) {
    // Sin imágenes: sin la baldosa de hojarasca del monte, que el césped no
    // lleva.
    let material = ground_material(
        root.as_deref().map(|r| r.0.as_path()),
        None,
        server.as_deref(),
    );
    commands.insert_resource(LawnKit(materials.add(material)));
}

/// La capa de pasto del césped `plaza`: un quad a [`LAWN_LIFT`] sobre la losa,
/// con sus posiciones relativas a `origin`, el punto del suelo en el medio de
/// la plaza (el pueblo está nivelado), pintado de [`LAWN`] como pintura de
/// suelo (sin hojarasca) y sin suelo despejado ni húmedo.
pub fn lawn_mesh(plaza: Aabb, origin: Vec3) -> Mesh {
    let top = origin.y + LAWN_TOP + LAWN_LIFT;
    let corners = [
        plaza.min,
        pampa_core::Vec2::new(plaza.max.x, plaza.min.y),
        plaza.max,
        pampa_core::Vec2::new(plaza.min.x, plaza.max.y),
    ];
    let world = corners.map(|g| world_from_ground(g, top));
    let uv = world.map(ground_uv);
    let tangent = face_tangent([world[0], world[1], world[2]], [uv[0], uv[1], uv[2]]);
    let paint = LinearRgba::from(Color::srgb(LAWN[0], LAWN[1], LAWN[2]));
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        world.map(|p| (p - origin).to_array()).to_vec(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; 4])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv.to_vec())
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, vec![[0.0, 0.0]; 4])
    .with_inserted_attribute(Mesh::ATTRIBUTE_TANGENT, vec![tangent; 4])
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_COLOR,
        vec![[paint.red, paint.green, paint.blue, 0.0]; 4],
    )
    .with_inserted_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]))
}

/// Siembra el césped de cada plaza de los pueblos recién aparecidos.
fn sow_lawns(
    mut commands: Commands,
    towns: Res<WorldTowns>,
    terrain: Res<WorldTerrain>,
    kit: Option<Res<LawnKit>>,
    roots: Query<(Entity, &TownRoot), Added<TownRoot>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Some(kit) = kit else {
        return;
    };
    for (root, town) in &roots {
        let Some(town) = towns.0.get(town.0) else {
            continue;
        };
        for plaza in town.plazas() {
            let centre = plaza.center();
            let origin = world_from_ground(centre, terrain.0.height(centre.x, centre.y));
            commands.spawn((
                Mesh3d(meshes.add(lawn_mesh(plaza, origin))),
                MeshMaterial3d(kit.0.clone()),
                Transform::from_translation(origin),
                // El rango de los edificios del pueblo, como la losa del césped.
                ranges()[0].clone(),
                PlazaLawn,
                ChildOf(root),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::mesh::VertexAttributeValues;
    use pampa_core::{Terrain, Vec2 as GroundVec2};

    use super::*;

    #[test]
    fn a_lawn_covers_its_plaza_just_above_the_slab_painted_green_with_no_litter() {
        let terrain = Terrain::default();
        let plaza = Aabb {
            min: GroundVec2::new(-20.0, 30.0),
            max: GroundVec2::new(20.0, 70.0),
        };
        let c = plaza.center();
        let origin = world_from_ground(c, terrain.height(c.x, c.y));
        let mut mesh = lawn_mesh(plaza, origin);
        let Some(VertexAttributeValues::Float32x3(points)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION).cloned()
        else {
            panic!("positions")
        };
        for p in &points {
            let world = origin + Vec3::from_array(*p);
            let g = GroundVec2::new(world.x, -world.z);
            assert!(plaza.expanded(1e-3).contains_point(g), "{g:?}");
            // Plana, a la altura del suelo en su medio: el pueblo está nivelado.
            let above = world.y - terrain.height(c.x, c.y);
            assert!((above - LAWN_TOP - LAWN_LIFT).abs() < 1e-4, "{above}");
        }
        let Some(VertexAttributeValues::Float32x4(colours)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("colours")
        };
        // Verde (más verde que rojo), sin hojarasca.
        assert!(
            colours.iter().all(|c| c[1] > c[0] && c[3] == 0.0),
            "{colours:?}"
        );
        let Some(VertexAttributeValues::Float32x2(cover)) = mesh.attribute(Mesh::ATTRIBUTE_UV_1)
        else {
            panic!("cover")
        };
        assert!(
            cover.iter().all(|c| *c == [0.0, 0.0]),
            "neither cleared nor wet"
        );
        mesh.compute_normals();
        let Some(VertexAttributeValues::Float32x3(normals)) =
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!("normals")
        };
        assert!(normals.iter().all(|n| n[1] > 0.99), "faces up");
    }
}
