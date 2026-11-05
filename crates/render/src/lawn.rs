//! El césped de las plazas: sobre la losa de color del césped de cada plaza
//! ([`crate::town`]) va una capa con el shader del suelo
//! (`assets/shaders/ground.wesl`, [`GroundMaterial`]), así que la plaza muestra
//! el mismo pasto fotografiado de 2K que la pampa, con su relieve, sus matas y
//! algún manchón pelado, en un verde más vivo: el de un césped regado frente a
//! la municipalidad.
//!
//! La capa es una grilla plana apenas por encima de la losa (el pueblo está
//! nivelado); los senderos, el monumento y los bancos de la plaza quedan por
//! encima de ella. Junto a los senderos el pasto está pisoteado: una franja de
//! tierra gastada de ancho irregular, así que el borde entre el sendero y el
//! césped no es neto. Se agrega a cada pueblo cuando aparece ([`TownRoot`]), como
//! hija suya, y se va con él.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use pampa_core::town::plaza_crossing;
use pampa_core::{Aabb, plaza_paths};

use crate::AssetRoot;
use crate::coords::world_from_ground;
use crate::facade::plaza::LAWN_TOP;
use crate::ground::{GroundMaterial, face_tangent, ground_material, ground_uv};
use crate::settlement::ranges;
use crate::terrain::{ROAD_EARTH, WorldTerrain};
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

/// Hasta dónde llega, como mucho, el pasto pisoteado más allá del borde de un
/// sendero de la plaza o de su cruce, en metros: la gente camina por el borde
/// y corta camino por las esquinas.
pub const PATH_WEAR: f32 = 2.0;
/// Lado de las celdas de la capa de pasto cerca de los senderos y lejos de
/// ellos, en metros: finas donde el pasto se gasta, para que el borde del
/// desgaste no sea una línea recta.
const FINE_CELL: f32 = 0.25;
const COARSE_CELL: f32 = 1.0;

/// Cuánto gastado está el césped de `plaza` en el punto del suelo `g`, de 0 a
/// 1: 1 en el borde de sus senderos y de su cruce ([`plaza_paths`],
/// [`plaza_crossing`]) y en la mitad de la franja más cerca de ellos (tierra
/// pelada), y 0 a una distancia que va y viene a lo largo del borde, de un
/// tercio de [`PATH_WEAR`] a todo él.
pub fn path_wear(plaza: Aabb, g: pampa_core::Vec2) -> f32 {
    let [across, along] = plaza_paths(plaza);
    let d = [across, along, plaza_crossing(plaza)]
        .iter()
        .map(|a| a.closest_point(g).distance(g))
        .fold(f32::INFINITY, f32::min);
    // A lo largo de cualquiera de los dos senderos, `s` avanza.
    let s = g.x + g.y;
    let reach = PATH_WEAR * (0.65 + 0.2 * (1.3 * s).sin() + 0.15 * (3.7 * s + 1.9).sin());
    let t = (2.0 * (1.0 - d / reach)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Los cortes de la capa a lo largo de un eje, de `lo` a `hi`: cada
/// [`FINE_CELL`] a menos de `band` de `c` y cada [`COARSE_CELL`] lejos.
fn stops(lo: f32, hi: f32, c: f32, band: f32) -> Vec<f32> {
    let mut out = vec![lo];
    let mut t = lo;
    while t < hi {
        let step = if (t - c).abs() < band {
            FINE_CELL
        } else {
            COARSE_CELL
        };
        t = (t + step).min(hi);
        out.push(t);
    }
    out
}

/// La capa de pasto del césped `plaza`: una grilla plana a [`LAWN_LIFT`] sobre
/// la losa, con sus posiciones relativas a `origin`, el punto del suelo en el
/// medio de la plaza (el pueblo está nivelado), pintada de [`LAWN`] como
/// pintura de suelo (sin hojarasca), sin suelo húmedo, y despejada y de tierra
/// gastada donde la gente pisa junto a los senderos ([`path_wear`]).
pub fn lawn_mesh(plaza: Aabb, origin: Vec3) -> Mesh {
    let top = origin.y + LAWN_TOP + LAWN_LIFT;
    let c = plaza.center();
    let band = plaza_crossing(plaza).size().x / 2.0 + PATH_WEAR + FINE_CELL;
    let xs = stops(plaza.min.x, plaza.max.x, c.x, band);
    let ys = stops(plaza.min.y, plaza.max.y, c.y, band);
    let (mut positions, mut uvs, mut cover, mut colours) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for &y in &ys {
        for &x in &xs {
            let g = pampa_core::Vec2::new(x, y);
            let world = world_from_ground(g, top);
            let wear = path_wear(plaza, g);
            let srgb = Vec3::from_array(LAWN).lerp(ROAD_EARTH, wear);
            let paint = LinearRgba::from(Color::srgb(srgb.x, srgb.y, srgb.z));
            positions.push((world - origin).to_array());
            uvs.push(ground_uv(world));
            cover.push([wear, 0.0]);
            colours.push([paint.red, paint.green, paint.blue, 0.0]);
        }
    }
    let corner = |g: pampa_core::Vec2| world_from_ground(g, top);
    let world = [
        corner(plaza.min),
        corner(pampa_core::Vec2::new(plaza.max.x, plaza.min.y)),
        corner(plaza.max),
    ];
    let tangent = face_tangent(world, world.map(ground_uv));
    let nx = xs.len() as u32;
    let mut indices = Vec::new();
    for j in 0..ys.len() as u32 - 1 {
        for i in 0..nx - 1 {
            let a = j * nx + i;
            let (b, c, d) = (a + 1, a + nx + 1, a + nx);
            indices.extend([a, b, c, a, c, d]);
        }
    }
    let n = positions.len();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; n])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, cover)
    .with_inserted_attribute(Mesh::ATTRIBUTE_TANGENT, vec![tangent; n])
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colours)
    .with_inserted_indices(Indices::U32(indices))
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
        let Some(VertexAttributeValues::Float32x2(cover)) = mesh.attribute(Mesh::ATTRIBUTE_UV_1)
        else {
            panic!("cover")
        };
        for ((p, colour), cover) in points.iter().zip(colours).zip(cover) {
            let world = origin + Vec3::from_array(*p);
            let wear = path_wear(plaza, GroundVec2::new(world.x, -world.z));
            // Sin hojarasca ni suelo húmedo; despejado lo que está gastado.
            assert_eq!(colour[3], 0.0);
            assert_eq!(*cover, [wear, 0.0]);
            // Verde (más verde que rojo) lejos de los senderos; tierra (más
            // roja que verde) en su borde.
            if wear == 0.0 {
                assert!(colour[1] > colour[0], "{colour:?}");
            } else if wear == 1.0 {
                assert!(colour[0] > colour[1], "{colour:?}");
            }
        }
        assert!(cover.iter().any(|c| c[0] == 1.0), "worn by the paths");
        assert!(cover.iter().any(|c| c[0] == 0.0), "green in the beds");
        mesh.compute_normals();
        let Some(VertexAttributeValues::Float32x3(normals)) =
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
        else {
            panic!("normals")
        };
        assert!(normals.iter().all(|n| n[1] > 0.99), "faces up");
    }

    #[test]
    fn the_grass_is_worn_beside_the_paths_to_a_ragged_edge() {
        let plaza = Aabb {
            min: GroundVec2::new(-40.0, -40.0),
            max: GroundVec2::new(40.0, 40.0),
        };
        // El borde norte del sendero este–oeste, a lo largo de 20 m.
        let edge = pampa_core::PLAZA_PATH_WIDTH / 2.0;
        let reach: Vec<f32> = (0..80)
            .map(|i| {
                let x = 10.0 + 0.25 * i as f32;
                assert_eq!(path_wear(plaza, GroundVec2::new(x, edge)), 1.0);
                assert_eq!(path_wear(plaza, GroundVec2::new(x, edge + PATH_WEAR)), 0.0);
                (0..=80)
                    .map(|k| PATH_WEAR * k as f32 / 80.0)
                    .find(|&d| path_wear(plaza, GroundVec2::new(x, edge + d)) == 0.0)
                    .unwrap()
            })
            .collect();
        let (lo, hi) = reach.iter().fold((f32::INFINITY, 0.0f32), |(lo, hi), &r| {
            (lo.min(r), hi.max(r))
        });
        assert!(hi - lo > 0.5 * PATH_WEAR, "{lo}..{hi}");
    }
}
