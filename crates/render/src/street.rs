//! El desgaste de las calles de tierra de los pueblos: sobre la calzada de
//! cada calle (la losa de tierra de [`crate::town`]) va una capa con el shader
//! del camino (`assets/shaders/road.wesl`, [`RoadMaterial`]), así que una calle
//! de 1919 muestra lo mismo que un camino: dos carriles de huellas de ruedas
//! que serpentean, el pisoteo de los cascos, bajos de barro seco agrietado y
//! pasto que se mete por los bordes. El shader ubica los carriles en proporción
//! al medio ancho de la calzada de cada calle (la calle menos sus dos veredas
//! de [`Sidewalk::WIDTH`]), así que hay un material por ancho.
//!
//! La capa es opaca y apenas más alta que la calzada; las calles norte-sur van
//! un poco más arriba que las este-oeste, así que en un cruce gana una sola y
//! nunca parpadean. Se agrega a cada pueblo cuando aparece ([`TownRoot`]), como
//! hija suya, y se va con él.

use bevy::asset::RenderAssetUsages;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use pampa_core::town::Sidewalk;
use pampa_core::{Aabb, TownConfig, Vec2 as GroundVec2};

use crate::AssetRoot;
use crate::coords::world_from_ground;
use crate::ground::{RoadMaterial, face_tangent, ground_uv, road_material};
use crate::road::DIRT;
use crate::settlement::ranges;
use crate::terrain::WorldTerrain;
use crate::town::{STREET_TOP, StreamTowns, TownRoot, WorldTowns};

/// Cuánto sube la capa gastada sobre la calzada, en metros: una calle
/// este-oeste y una norte-sur.
pub const WEAR_LIFT: [f32; 2] = [0.003, 0.006];
// Las norte-sur ganan en los cruces.
const _: () = assert!(WEAR_LIFT[1] > WEAR_LIFT[0]);

/// La calzada más angosta que se gasta, de medio ancho en metros: la de la
/// calle más angosta ([`TownConfig::MIN_STREET_WIDTH`]) entre sus veredas.
const MIN_HALF: f32 = 0.5 * TownConfig::MIN_STREET_WIDTH - Sidewalk::WIDTH;
// Una calzada de 3 m entre dos veredas, como pide core.
const _: () = assert!(MIN_HALF == 1.5);

/// Si `street` corre de norte a sur (más larga en `y`); si no, de este a
/// oeste.
fn runs_north_south(street: Aabb) -> bool {
    street.max.y - street.min.y > street.max.x - street.min.x
}

/// El medio ancho de la calzada de `street`, entre sus dos veredas de
/// [`Sidewalk::WIDTH`], al menos [`MIN_HALF`]: de ahí mide el shader sus
/// carriles, y hay un material por cada uno.
pub fn carriageway_half(street: Aabb) -> f32 {
    let size = street.max - street.min;
    (0.5 * size.x.min(size.y) - Sidewalk::WIDTH).max(MIN_HALF)
}

/// Una capa gastada de calle, hija de la raíz de su pueblo.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreetWear;

/// Gasta las calles de cada pueblo que aparece.
pub struct StreetWearPlugin;

impl Plugin for StreetWearPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Assets<RoadMaterial>>()
            .add_systems(Startup, setup_wear)
            .add_systems(PreUpdate, wear_streets.after(StreamTowns));
    }
}

/// El material de calle de base y uno por medio ancho (en centímetros).
#[derive(Resource)]
struct WearKit {
    base: RoadMaterial,
    by_half: HashMap<u32, Handle<RoadMaterial>>,
}

fn setup_wear(
    mut commands: Commands,
    root: Option<Res<AssetRoot>>,
    server: Option<Res<AssetServer>>,
    mut images: Option<ResMut<Assets<Image>>>,
) {
    let mut base = road_material(
        root.as_deref().map(|r| r.0.as_path()),
        images.as_deref_mut(),
        server.as_deref(),
    );
    // La calle se apoya sobre su propia calzada, no se mezcla sobre el suelo.
    base.base.alpha_mode = AlphaMode::Opaque;
    base.base.depth_bias = 0.0;
    commands.insert_resource(WearKit {
        base,
        by_half: HashMap::new(),
    });
}

/// La capa gastada de `street`: un quad a [`WEAR_LIFT`] sobre la calzada (más
/// alto si corre de norte a sur), con sus posiciones relativas a `origin`, el
/// punto del suelo en el medio de la calle (el pueblo está nivelado). Su segundo UV es la distancia de costado
/// al eje de la calle y la distancia a lo largo de ella, en metros, como el
/// de la superficie de un camino. Devuelve la malla y el medio ancho de la
/// calzada, entre las veredas (que cubren la capa a cada lado).
pub fn wear_mesh(street: Aabb, origin: Vec3) -> (Mesh, f32) {
    let centre = street.center();
    let north_south = runs_north_south(street);
    // A lo largo del lado largo; de costado, hacia el norte o el oeste.
    let (along, across) = if north_south {
        (GroundVec2::new(0.0, 1.0), GroundVec2::new(-1.0, 0.0))
    } else {
        (GroundVec2::new(1.0, 0.0), GroundVec2::new(0.0, 1.0))
    };
    let half = carriageway_half(street);
    let top = origin.y + STREET_TOP + WEAR_LIFT[usize::from(north_south)];
    let corners = [
        street.min,
        GroundVec2::new(street.max.x, street.min.y),
        street.max,
        GroundVec2::new(street.min.x, street.max.y),
    ];
    let world = corners.map(|g| world_from_ground(g, top));
    let uv = world.map(ground_uv);
    let lanes = corners.map(|g| [(g - centre).dot(across), g.dot(along)]);
    // Visto desde arriba, en sentido antihorario.
    let tangent = face_tangent([world[0], world[1], world[2]], [uv[0], uv[1], uv[2]]);
    let tint = LinearRgba::from(Color::srgb(DIRT[0], DIRT[1], DIRT[2]));
    let mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        world.map(|p| (p - origin).to_array()).to_vec(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; 4])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv.to_vec())
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, lanes.to_vec())
    .with_inserted_attribute(Mesh::ATTRIBUTE_TANGENT, vec![tangent; 4])
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_COLOR,
        vec![[tint.red, tint.green, tint.blue, 1.0]; 4],
    )
    .with_inserted_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]));
    (mesh, half)
}

/// Agrega la capa gastada a las calles de cada pueblo recién aparecido.
fn wear_streets(
    mut commands: Commands,
    towns: Res<WorldTowns>,
    terrain: Res<WorldTerrain>,
    kit: Option<ResMut<WearKit>>,
    roots: Query<(Entity, &TownRoot), Added<TownRoot>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<RoadMaterial>>,
) {
    let Some(mut kit) = kit else {
        return;
    };
    for (root, town) in &roots {
        let Some(town) = towns.0.get(town.0) else {
            continue;
        };
        for street in &town.streets {
            let centre = street.center();
            let origin = world_from_ground(centre, terrain.0.height(centre.x, centre.y));
            let (mesh, half) = wear_mesh(*street, origin);
            let key = (half * 100.0).round() as u32;
            let kit = &mut *kit;
            let material = kit
                .by_half
                .entry(key)
                .or_insert_with(|| {
                    let mut material = kit.base.clone();
                    material.extension.shape.x = half;
                    materials.add(material)
                })
                .clone();
            commands.spawn((
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(material),
                Transform::from_translation(origin),
                // El rango de los edificios del pueblo, como su calzada.
                ranges()[0].clone(),
                NotShadowCaster,
                StreetWear,
                ChildOf(root),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::mesh::VertexAttributeValues;
    use pampa_core::Terrain;

    use super::*;

    fn floats2(mesh: &Mesh, attribute: bevy::mesh::MeshVertexAttribute) -> Vec<[f32; 2]> {
        match mesh.attribute(attribute) {
            Some(VertexAttributeValues::Float32x2(v)) => v.clone(),
            _ => Vec::new(),
        }
    }

    fn points(mesh: &Mesh) -> Vec<[f32; 3]> {
        match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(VertexAttributeValues::Float32x3(v)) => v.clone(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn a_street_wears_its_whole_top_just_above_the_dirt_measured_from_its_axis() {
        let terrain = Terrain::default();
        // Una calle este-oeste de 80 por 16 m, y una norte-sur de 12 por 60.
        let cases = [
            (
                Aabb {
                    min: GroundVec2::new(-40.0, 10.0),
                    max: GroundVec2::new(40.0, 26.0),
                },
                4.5,
                0,
            ),
            (
                Aabb {
                    min: GroundVec2::new(5.0, -30.0),
                    max: GroundVec2::new(17.0, 30.0),
                },
                2.5,
                1,
            ),
        ];
        for (street, expected_half, lift) in cases {
            let c = street.center();
            let origin = world_from_ground(c, terrain.height(c.x, c.y));
            let (mesh, half) = wear_mesh(street, origin);
            assert_eq!(half, expected_half);
            let at = points(&mesh);
            let lanes = floats2(&mesh, Mesh::ATTRIBUTE_UV_1);
            assert_eq!((at.len(), lanes.len()), (4, 4));
            for (p, lane) in at.iter().zip(&lanes) {
                let world = origin + Vec3::from_array(*p);
                let g = GroundVec2::new(world.x, -world.z);
                // Cubre la calle entera, a ras de sus bordes.
                assert!(street.expanded(1e-3).contains_point(g), "{g:?}");
                // Plana, a la altura del suelo en su medio: el pueblo está
                // nivelado.
                let above = world.y - terrain.height(c.x, c.y) - STREET_TOP;
                assert!((above - WEAR_LIFT[lift]).abs() < 1e-4, "{above}");
                // Los bordes de la calle (más allá de la calzada, bajo las
                // veredas) están a medio ancho de la calle del eje.
                let street_half = half + Sidewalk::WIDTH;
                assert!((lane[0].abs() - street_half).abs() < 1e-3, "{lane:?}");
            }
            assert!(lanes.iter().any(|l| l[0] > 0.0) && lanes.iter().any(|l| l[0] < 0.0));
            // A lo largo del lado largo.
            let long = (street.max.x - street.min.x).max(street.max.y - street.min.y);
            let along: Vec<f32> = lanes.iter().map(|l| l[1]).collect();
            let span = along.iter().copied().fold(f32::MIN, f32::max)
                - along.iter().copied().fold(f32::MAX, f32::min);
            assert!((span - long).abs() < 1e-3, "{span}");
        }
    }

    #[test]
    fn the_wear_faces_up() {
        let street = Aabb {
            min: GroundVec2::new(0.0, 0.0),
            max: GroundVec2::new(50.0, 14.0),
        };
        let (mut mesh, _) = wear_mesh(street, Vec3::ZERO);
        mesh.compute_normals();
        let Some(VertexAttributeValues::Float32x3(normals)) =
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!("normals")
        };
        assert!(normals.iter().all(|n| n[1] > 0.99), "{normals:?}");
    }
}
