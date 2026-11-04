//! El pasto cercano: matas bajas alrededor de la cámara, donde las matas
//! sueltas de [`crate::flora`] (una cada cien y pico de metros cuadrados)
//! dejan el suelo pelado. La alfombra la dibuja el suelo (`ground.wesl`);
//! esto le da volumen.
//!
//! El suelo cercano se divide en celdas de [`CELL`] metros. Cada celda dentro
//! de [`NEAR`] metros de la cámara es una entidad con una malla fusionada de
//! matas de tres tarjetas en abanico, del mapa seco de `grass_medium_01`,
//! teñidas de verde a pajizo con la sequedad del lugar. Se mecen con el viento
//! ([`crate::grass::GrassWind`]) y se apoyan en el suelo dibujado
//! (`mesh_height`), no en el lógico, para no hundirse en la malla de 5 m, con
//! la normal del suelo, para iluminarse como él en una barranca. El pasto
//! crece solo donde crecen las matas ([`FloraSite::grassland`]), y más ralo
//! bajo el monte.
//!
//! Las celdas se arman al acercarse la cámara (las más cercanas primero, unas
//! pocas por frame) y se borran al alejarse. En los últimos metros antes de
//! [`NEAR`] las puntas se hunden en el suelo (en el vertex shader, sin el
//! punteado de un fundido por dithering). Cada celda sale siempre igual: su
//! contenido depende solo de su coordenada.

use bevy::asset::RenderAssetUsages;
use bevy::image::CompressedImageFormatSupport;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use pampa_core::{FloraSite, TuftKind, Vec2 as GroundVec2};

use crate::AssetRoot;
use crate::coords::world_from_ground;
use crate::flora::{WorldFlora, jitter};
use crate::grass::{CARD_VARIANTS, GrassMaterial, SWAY, load_maps, swaying, tint_rgb};
use crate::terrain::{ChunkDetail, WorldChunks, WorldTerrain, mesh_height};

/// Lado de una celda, en metros.
pub const CELL: f32 = 4.0;
/// Hasta dónde llega el pasto cercano desde la cámara, en metros: ahí ya se
/// hundió en el suelo. Lejos, para que no se lo vea crecer ni hundirse cerca
/// del jinete; las matas son tan livianas (seis triángulos) que alcanzan
/// pocos miles.
pub const NEAR: f32 = 50.0;
/// Matas por metro cuadrado en pampa abierta.
pub const DENSITY: f32 = 0.45;
/// La altura más baja y la más alta de una mata, en metros.
pub const HEIGHT: (f32, f32) = (0.22, 0.48);
/// Tarjetas por mata.
const CARDS: usize = 3;
/// Celdas nuevas por frame: la cámara al galope no traba el frame.
const NEW_PER_FRAME: usize = 6;

/// Una celda de pasto cercano, por su coordenada en la grilla de [`CELL`].
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeadowCell(pub IVec2);

/// Arma y borra las celdas alrededor de la cámara.
pub struct MeadowPlugin;

impl Plugin for MeadowPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Assets<GrassMaterial>>()
            .init_resource::<Meadow>()
            .add_systems(Startup, setup_meadow)
            .add_systems(Update, tend_meadow);
    }
}

/// Las celdas armadas.
#[derive(Resource, Default)]
struct Meadow {
    cells: HashMap<IVec2, Entity>,
}

/// El material del pasto cercano.
#[derive(Resource)]
struct MeadowKit(Handle<GrassMaterial>);

fn setup_meadow(
    mut commands: Commands,
    root: Option<Res<AssetRoot>>,
    server: Option<Res<AssetServer>>,
    support: Option<Res<CompressedImageFormatSupport>>,
    mut materials: ResMut<Assets<GrassMaterial>>,
) {
    let [dry, _] = load_maps(root.as_deref(), server.as_deref(), support.as_deref());
    // Blanco: el color de vértice lleva el tinte. Las tarjetas se hunden
    // hacia NEAR.
    let mut material = swaying(Color::WHITE, dry);
    material.extension.wind.w = NEAR;
    commands.insert_resource(MeadowKit(materials.add(material)));
}

/// La celda que contiene el punto del suelo `p`.
pub fn cell_of(p: GroundVec2) -> IVec2 {
    IVec2::new((p.x / CELL).floor() as i32, (p.y / CELL).floor() as i32)
}

/// El centro de la celda `cell`, en el suelo.
pub fn cell_centre(cell: IVec2) -> GroundVec2 {
    GroundVec2::new((cell.x as f32 + 0.5) * CELL, (cell.y as f32 + 0.5) * CELL)
}

/// Las celdas dentro de `radius` metros de `p`, de la más cercana a la más
/// lejana.
pub fn cells_near(p: GroundVec2, radius: f32) -> Vec<IVec2> {
    let reach = (radius / CELL).ceil() as i32 + 1;
    let home = cell_of(p);
    let mut cells: Vec<(f32, IVec2)> = (-reach..=reach)
        .flat_map(|dx| (-reach..=reach).map(move |dy| home + IVec2::new(dx, dy)))
        .map(|c| (cell_centre(c).distance(p), c))
        .filter(|(d, _)| *d <= radius + CELL * std::f32::consts::FRAC_1_SQRT_2)
        .collect();
    cells.sort_by(|a, b| a.0.total_cmp(&b.0));
    cells.into_iter().map(|(_, c)| c).collect()
}

/// Una malla de tarjetas en construcción, con posiciones relativas a `origin`.
struct Cards {
    origin: Vec3,
    positions: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    sway: Vec<[f32; 2]>,
    colors: Vec<[f32; 4]>,
    normals: Vec<[f32; 3]>,
    indices: Vec<u32>,
}

impl Cards {
    fn new(origin: Vec3) -> Self {
        Self {
            origin,
            positions: Vec::new(),
            uvs: Vec::new(),
            sway: Vec::new(),
            colors: Vec::new(),
            normals: Vec::new(),
            indices: Vec::new(),
        }
    }

    /// Una mata de [`CARDS`] tarjetas de `height` metros abiertas en abanico
    /// sobre `base` desde el rumbo `yaw`, con la macolla `variant` del atlas
    /// y el color lineal `color`, iluminadas con la normal `normal` del suelo
    /// donde están.
    fn tuft(
        &mut self,
        base: Vec3,
        height: f32,
        yaw: f32,
        variant: usize,
        color: [f32; 4],
        normal: Vec3,
    ) {
        let (count, width) = (CARDS, 1.2 * height);
        // La celda de la macolla en la grilla de 2x2 del atlas
        // ([`CARD_VARIANTS`]).
        let (u0, v0) = ((variant % 2) as f32 * 0.5, (variant / 2) as f32 * 0.5);
        let at = base - self.origin;
        for card in 0..count {
            let angle = yaw + card as f32 / count as f32 * std::f32::consts::PI;
            let side = Vec3::new(angle.cos(), 0.0, angle.sin()) * (0.5 * width);
            let up = Vec3::Y * height;
            let first = self.positions.len() as u32;
            self.positions.extend(
                [at - side, at + side, at + side + up, at - side + up].map(|p| p.to_array()),
            );
            self.uvs.extend([
                [u0, v0 + 0.5],
                [u0 + 0.5, v0 + 0.5],
                [u0 + 0.5, v0],
                [u0, v0],
            ]);
            self.sway
                .extend([[0.0, 0.0], [0.0, 0.0], [1.0, height], [1.0, height]]);
            self.colors.extend([color; 4]);
            self.normals.extend([normal.to_array(); 4]);
            self.indices.extend([0, 1, 2, 0, 2, 3].map(|i| first + i));
        }
    }

    fn mesh(self) -> Option<Mesh> {
        if self.indices.is_empty() {
            return None;
        }
        Some(
            Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::RENDER_WORLD,
            )
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs)
            .with_inserted_attribute(SWAY, self.sway)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
            .with_inserted_indices(Indices::U32(self.indices)),
        )
    }
}

/// El color lineal de una mata en `p`: el tinte de su sequedad, corrido por
/// `tint`, y su brillo, por `light` (dos tiradas independientes en `[0, 1)`).
fn card_color(site: &FloraSite, p: GroundVec2, tint: f32, light: f32) -> [f32; 4] {
    let dry = site.dryness(p) + (tint - 0.5) * 0.35;
    let c = tint_rgb(TuftKind::Paja, dry) * (0.88 + 0.24 * light);
    Color::srgb(c.x, c.y, c.z).to_linear().to_f32_array()
}

/// El pasto de la celda `cell`, con sus posiciones relativas a `origin` (un
/// punto del mundo) y apoyado en `ground` (la altura del suelo dibujado en un
/// punto del suelo); `None` cuando no crece nada.
pub fn cell_grass(
    cell: IVec2,
    site: &FloraSite,
    ground: &dyn Fn(GroundVec2) -> f32,
    origin: Vec3,
) -> Option<Mesh> {
    let seed =
        (cell.x as u32).wrapping_mul(0x9e37_79b9) ^ (cell.y as u32).wrapping_mul(0x85eb_ca6b);
    let corner = GroundVec2::new(cell.x as f32 * CELL, cell.y as f32 * CELL);
    let mut i = 0u32;
    let mut roll = || {
        i += 1;
        jitter(seed, i)
    };
    let mut cards = Cards::new(origin);
    for _ in 0..(DENSITY * CELL * CELL).round() as usize {
        let p = corner + GroundVec2::new(roll() * CELL, roll() * CELL);
        let (keep, size, yaw, variant, tint, light) =
            (roll(), roll(), roll(), roll(), roll(), roll());
        // Bajo el monte la hojarasca le gana al pasto.
        if keep > 1.0 - 0.7 * site.monte(p) || !site.grassland(p) {
            continue;
        }
        let base = world_from_ground(p, ground(p));
        // La normal del suelo, no la vertical: en una barranca al sol el pasto
        // se ilumina como la tierra que lo rodea.
        let e = 0.5;
        let dx = ground(p + GroundVec2::new(e, 0.0)) - ground(p - GroundVec2::new(e, 0.0));
        let dy = ground(p + GroundVec2::new(0.0, e)) - ground(p - GroundVec2::new(0.0, e));
        let normal = Vec3::new(-dx, 2.0 * e, dy).normalize();
        let height = HEIGHT.0 + (HEIGHT.1 - HEIGHT.0) * size;
        let color = card_color(site, p, tint, light);
        cards.tuft(
            base,
            height,
            yaw * std::f32::consts::TAU,
            (variant * CARD_VARIANTS as f32) as usize,
            color,
            normal,
        );
    }
    cards.mesh()
}

/// Dónde está la cámara que se ve, la misma que mide el hundimiento del
/// shader: la activa de mayor orden. `None` sin ninguna activa.
fn viewing_eye<'a>(
    cameras: impl Iterator<Item = (&'a GlobalTransform, &'a Camera)>,
) -> Option<Vec3> {
    cameras
        .filter(|(_, c)| c.is_active)
        .max_by_key(|(_, c)| c.order)
        .map(|(t, _)| t.translation())
}

/// Arma las celdas que entraron en [`NEAR`] (las más cercanas primero, hasta
/// [`NEW_PER_FRAME`]) y borra las que salieron.
#[allow(clippy::too_many_arguments)]
fn tend_meadow(
    mut commands: Commands,
    mut meadow: ResMut<Meadow>,
    kit: Option<Res<MeadowKit>>,
    flora: Option<Res<WorldFlora>>,
    terrain: Res<WorldTerrain>,
    world: Res<WorldChunks>,
    detail: Option<Res<ChunkDetail>>,
    camera: Query<(&GlobalTransform, &Camera), With<Camera3d>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let (Some(kit), Some(flora), Some(detail)) = (kit, flora, detail) else {
        return;
    };
    let Some(eye) = viewing_eye(camera.iter()) else {
        return;
    };
    let eye = GroundVec2::new(eye.x, -eye.z);
    let keep = NEAR + 2.0 * CELL;
    meadow.cells.retain(|&cell, &mut entity| {
        let near = cell_centre(cell).distance(eye) <= keep;
        if !near {
            commands.entity(entity).despawn();
        }
        near
    });
    let (size, resolution) = (world.0.chunk_size, detail.resolution);
    let ground = |p: GroundVec2| mesh_height(&terrain.0, p, size, resolution);
    let new: Vec<IVec2> = cells_near(eye, NEAR)
        .into_iter()
        .filter(|c| !meadow.cells.contains_key(c))
        .take(NEW_PER_FRAME)
        .collect();
    for cell in new {
        let centre = cell_centre(cell);
        let origin = world_from_ground(centre, ground(centre));
        let grass = cell_grass(cell, &flora.0, &ground, origin);
        let mut entity = commands.spawn((
            MeadowCell(cell),
            Transform::from_translation(origin),
            Visibility::default(),
        ));
        entity.with_children(|parts| {
            if let Some(mesh) = grass {
                parts.spawn((
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(kit.0.clone()),
                    Transform::default(),
                    NotShadowCaster,
                ));
            }
        });
        meadow.cells.insert(cell, entity.id());
    }
}

#[cfg(test)]
mod tests {
    use bevy::mesh::VertexAttributeValues;
    use pampa_core::{RailLine, Terrain, WorldConfig};

    use super::*;

    fn site() -> FloraSite {
        FloraSite::new(
            WorldConfig::default(),
            &Terrain::default(),
            &RailLine::default(),
        )
    }

    /// Un suelo en pendiente, para ver que cada mata se apoya donde está.
    fn ground(p: GroundVec2) -> f32 {
        0.3 + 0.01 * p.x - 0.02 * p.y
    }

    fn pairs(mesh: &Mesh, attribute: bevy::mesh::MeshVertexAttribute) -> Vec<[f32; 2]> {
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

    /// Una celda de pampa abierta, lejos del monte.
    fn open_cell(site: &FloraSite) -> IVec2 {
        (0..200)
            .map(|i| IVec2::new(i * 7, -i * 3))
            .find(|&c| site.monte(cell_centre(c)) < 0.05 && site.grassland(cell_centre(c)))
            .expect("an open cell")
    }

    #[test]
    fn an_open_cell_grows_tufts_standing_on_the_drawn_ground_lit_like_it() {
        let site = site();
        let cell = open_cell(&site);
        let origin = world_from_ground(cell_centre(cell), 1.0);
        let mesh = cell_grass(cell, &site, &ground, origin).expect("grass");
        let at = points(&mesh);
        let sway = pairs(&mesh, SWAY);
        assert_eq!(sway.len(), at.len());
        let tufts = at.len() / (4 * CARDS);
        assert!(tufts as f32 >= 0.6 * DENSITY * CELL * CELL, "{tufts} tufts");
        for (p, s) in at.iter().zip(&sway) {
            let world = origin + Vec3::from_array(*p);
            let above = world.y - ground(GroundVec2::new(world.x, -world.z));
            // Una mata se apoya en su centro: en la pendiente, las esquinas de
            // sus tarjetas quedan a unos milímetros del suelo.
            if s[0] == 0.0 {
                assert!(above.abs() < 0.01, "a base floats {above} m");
            } else {
                assert_eq!(s[0], 1.0);
                assert!(
                    (above - s[1]).abs() < 0.01,
                    "a tip {above} m up, sways as {}",
                    s[1]
                );
                assert!((HEIGHT.0..=HEIGHT.1).contains(&s[1]), "{}", s[1]);
            }
        }
        // Iluminadas como el suelo en pendiente que tienen debajo.
        let slope = Vec3::new(-0.01, 1.0, -0.02).normalize();
        let normals = match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
            Some(VertexAttributeValues::Float32x3(v)) => v.clone(),
            _ => Vec::new(),
        };
        assert_eq!(normals.len(), at.len());
        assert!(
            normals
                .iter()
                .all(|n| Vec3::from_array(*n).distance(slope) < 1e-3)
        );
        // Siempre la misma celda.
        let again = cell_grass(cell, &site, &ground, origin).unwrap();
        assert_eq!(at, points(&again));
    }

    #[test]
    fn no_grass_grows_off_the_grassland() {
        let site = site();
        let outside = cell_of(WorldConfig::default().bounds().min) - IVec2::ONE;
        assert!(cell_grass(outside, &site, &ground, Vec3::ZERO).is_none());
    }

    #[test]
    fn the_grass_follows_the_active_camera_of_highest_order() {
        let at = |x: f32| GlobalTransform::from_translation(Vec3::new(x, 2.0, 0.0));
        let camera = |order: isize, is_active: bool| Camera {
            order,
            is_active,
            ..default()
        };
        let (a, b, c) = (at(1.0), at(2.0), at(3.0));
        let (ca, cb, cc) = (camera(0, true), camera(2, true), camera(5, false));
        let eye = viewing_eye([(&a, &ca), (&b, &cb), (&c, &cc)].into_iter());
        assert_eq!(eye, Some(b.translation()), "the inactive one is skipped");
        assert_eq!(viewing_eye([(&c, &cc)].into_iter()), None);
    }

    #[test]
    fn cells_near_come_nearest_first_and_cover_the_radius() {
        let eye = GroundVec2::new(13.0, -7.5);
        let cells = cells_near(eye, NEAR);
        let distances: Vec<f32> = cells
            .iter()
            .map(|&c| cell_centre(c).distance(eye))
            .collect();
        assert!(distances.windows(2).all(|d| d[0] <= d[1]));
        assert_eq!(cells[0], cell_of(eye));
        // Todo punto dentro de NEAR cae en una celda listada.
        for k in 0..64 {
            let angle = k as f32 * 0.37;
            let p = eye + GroundVec2::new(angle.cos(), angle.sin()) * NEAR * (k as f32 / 64.0);
            assert!(cells.contains(&cell_of(p)), "{p:?}");
        }
    }
}
