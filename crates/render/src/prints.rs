//! Huellas efímeras en el barro: cada pisada que salpica barro en la orilla de
//! una laguna (las [`Kick`] de [`crate::dust`]) deja además su marca: el vaso
//! descalzo de un caballo criollo, la suela de una alpargata o el surco de una
//! rueda. La marca se queda un rato y se borra sola. La tierra seca, el pasto,
//! lo firme y el agua no guardan huellas.
//!
//! Como el polvo, es un grupo fijo de [`PRINTS`] entidades que comparten una
//! malla (un cuadrado acostado sobre el suelo); con el grupo lleno se recicla
//! la huella más gastada. Cada clase de huella tiene su relieve hecho al
//! arrancar, horneado en cuánto oscurece (lo hondo y sus paredes, donde la
//! huella se hace sombra), y se multiplica sobre el suelo: oscurece la tierra
//! que haya debajo sin cambiarle el color. Su desvanecimiento va en [`LEVELS`]
//! escalones de materiales compartidos, para no tocar un material por huella
//! en cada frame.

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Extent3d, TextureDimension, TextureFormat};
use bevy::shader::ShaderRef;
use bevy::transform::TransformSystems;
use pampa_core::{Terrain, TownLayout, Vec2 as GroundVec2};

use crate::dust::{Footing, Kick, Kicks, Source, StirDust, WHEEL_SPACING, footing, recycle};
use crate::flora::jitter;
use crate::terrain::{WorldTerrain, hash2};
use crate::town::WorldTowns;

/// Cuántas huellas hay en total: más de las que deja un minuto de galope por
/// la orilla de una laguna.
pub const PRINTS: usize = 256;

/// En cuántos escalones de opacidad se desvanece una huella.
pub const LEVELS: usize = 12;

/// Cuántos segundos dura una huella.
const LIFE: f32 = 60.0;

/// Qué fracción de su vida se ve entera antes de empezar a borrarse.
const HOLD: f32 = 0.45;

/// A qué altura sobre el suelo va la huella, en metros: lo justo para no
/// pelearse con la tierra de abajo.
const RAISE: f32 = 0.012;

/// Cuánto más largo que lo rodado entre dos pisadas es el surco de una rueda:
/// los tramos se solapan un poco y no se ven cortes.
const RUT_OVERLAP: f32 = 1.12;

/// Cuánto deja pasar del suelo de abajo el fondo de una huella (sRGB,
/// multiplicado sobre el suelo, bajo el sombreado de su relieve): el barro
/// pisado junta agua y se oscurece.
const MUD_TINT: Vec3 = Vec3::new(0.62, 0.58, 0.54);

/// Si una pisada sobre `footing` deja huella: solo en el barro.
pub fn keeps_print(footing: Footing) -> bool {
    matches!(footing, Footing::Mud(_))
}

/// Cuánto se ve una huella de `age` segundos que dura `life`, de 1 (recién
/// hecha) a 0 (borrada): entera hasta [`HOLD`] de su vida, después se borra
/// suave.
pub fn fade(age: f32, life: f32) -> f32 {
    let u = age / life;
    if u >= 1.0 {
        0.0
    } else if u <= HOLD {
        1.0
    } else {
        let t = (u - HOLD) / (1.0 - HOLD);
        1.0 - t * t * (3.0 - 2.0 * t)
    }
}

/// El escalón de opacidad de una huella que se ve `seen` (de 0 a 1): 0 es
/// borrada, [`LEVELS`] es entera.
pub fn level(seen: f32) -> usize {
    ((seen.clamp(0.0, 1.0) * LEVELS as f32).ceil() as usize).min(LEVELS)
}

/// El ancho (de lado a lado) y el largo (en el sentido de la marcha) de la
/// huella de `source`, en metros.
fn footprint(source: Source) -> Vec2 {
    match source {
        Source::Hoof => Vec2::new(0.13, 0.15),
        Source::Foot => Vec2::new(0.11, 0.27),
        Source::Wheel => Vec2::new(0.09, WHEEL_SPACING * RUT_OVERLAP),
    }
}

/// Dónde va la huella de `kick` sobre el suelo de altura `height(x, y)` (en
/// coordenadas de suelo), girada `yaw` radianes: apoyada en el suelo y
/// acostada sobre su pendiente, con la punta hacia donde iba la pisada. El
/// surco de una rueda cubre lo que rodó desde su pisada anterior, detrás de
/// ella.
pub fn lay(kick: &Kick, yaw: f32, height: impl Fn(f32, f32) -> f32) -> Transform {
    let size = footprint(kick.source);
    let mut at = kick.at;
    if kick.source == Source::Wheel {
        at -= kick.travel * WHEEL_SPACING / 2.0;
    }
    let h = |x: f32, z: f32| height(x, -z);
    let e = 0.2;
    let up = Vec3::new(
        h(at.x - e, at.z) - h(at.x + e, at.z),
        2.0 * e,
        h(at.x, at.z - e) - h(at.x, at.z + e),
    )
    .normalize();
    let travel = Quat::from_rotation_y(yaw) * kick.travel;
    let ahead = (travel - up * travel.dot(up))
        .try_normalize()
        .unwrap_or(Vec3::Z);
    let side = up.cross(ahead);
    Transform {
        translation: Vec3::new(at.x, h(at.x, at.z) + RAISE, at.z),
        rotation: Quat::from_mat3(&Mat3::from_cols(side, up, ahead)),
        scale: Vec3::new(size.x, 1.0, size.y),
    }
}

/// Las huellas del grupo, en el orden en que se crearon.
#[derive(Resource, Debug, Default)]
struct PrintPool(Vec<Entity>);

/// Los materiales de las huellas: por clase y por escalón de opacidad (el
/// primero es el más tenue).
#[derive(Resource, Debug, Default)]
struct PrintMaterials(Vec<Handle<PrintMaterial>>);

/// Lo que el shader de las huellas (`assets/shaders/print.wesl`) cambia del
/// material estándar: en vez de su color, cuánto más oscuro que el suelo
/// llano queda cada punto bajo la misma luz, para multiplicarlo sobre el
/// suelo.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct PrintExt {}

impl MaterialExtension for PrintExt {
    fn fragment_shader() -> ShaderRef {
        "shaders/print.wesl".into()
    }
}

/// El material de una huella: [`StandardMaterial`] (su sombreado horneado,
/// su opacidad y su relieve) más [`PrintExt`].
pub type PrintMaterial = ExtendedMaterial<StandardMaterial, PrintExt>;

impl PrintMaterials {
    /// El material de `source` en el escalón `level` (de 1 a [`LEVELS`]).
    fn get(&self, source: Source, level: usize) -> Handle<PrintMaterial> {
        self.0[kind(source) * LEVELS + level - 1].clone()
    }
}

/// El índice de la clase de huella de `source`.
fn kind(source: Source) -> usize {
    match source {
        Source::Hoof => 0,
        Source::Foot => 1,
        Source::Wheel => 2,
    }
}

/// Las pisadas contadas, la semilla de la suerte de cada huella.
#[derive(Resource, Debug, Default)]
struct Pressed(u32);

/// Una huella del grupo: qué la hizo, cuán honda (de 0 a 1), sus segundos y
/// su escalón de opacidad ahora.
#[derive(Component, Debug, Clone, Copy)]
struct Print {
    source: Source,
    depth: f32,
    age: f32,
    level: usize,
}

impl Print {
    /// Una huella ya borrada, para llenar el grupo.
    const GONE: Self = Self {
        source: Source::Hoof,
        depth: 0.0,
        age: 1.0,
        level: 0,
    };

    /// Qué fracción de su vida gastó: 1 o más si ya se borró.
    fn spent(&self) -> f32 {
        if self.level == 0 {
            1.0
        } else {
            self.age / LIFE
        }
    }
}

/// Las huellas de las pisadas (ver el módulo).
pub struct PrintsPlugin;

impl Plugin for PrintsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<PrintMaterial>::default())
            .init_resource::<PrintPool>()
            .init_resource::<PrintMaterials>()
            .init_resource::<Pressed>()
            .add_systems(Startup, spawn_prints)
            .add_systems(
                PostUpdate,
                press_prints
                    .before(StirDust)
                    .before(TransformSystems::Propagate),
            );
    }
}

/// Crea el grupo de huellas, ocultas, con su malla, sus relieves y sus
/// materiales compartidos.
fn spawn_prints(
    mut commands: Commands,
    mut pool: ResMut<PrintPool>,
    mut shared: ResMut<PrintMaterials>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<PrintMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let mesh = meshes.add(lying_quad());
    for source in [Source::Hoof, Source::Foot, Source::Wheel] {
        let (color, normal) = relief(source);
        let (color, normal) = (images.add(color), images.add(normal));
        for level in 1..=LEVELS {
            let alpha = level as f32 / LEVELS as f32;
            shared.0.push(materials.add(PrintMaterial {
                base: StandardMaterial {
                    base_color: Color::WHITE.with_alpha(alpha),
                    base_color_texture: Some(color.clone()),
                    normal_map_texture: Some(normal.clone()),
                    alpha_mode: AlphaMode::Multiply,
                    perceptual_roughness: 1.0,
                    depth_bias: 2.0,
                    ..default()
                },
                extension: PrintExt {},
            }));
        }
    }
    let hidden = shared.get(Source::Hoof, 1);
    pool.0 = (0..PRINTS)
        .map(|_| {
            commands
                .spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(hidden.clone()),
                    Print::GONE,
                    Transform::default(),
                    Visibility::Hidden,
                    NotShadowCaster,
                ))
                .id()
        })
        .collect();
}

/// Un cuadrado de lado 1 acostado, con la normal hacia arriba: `u` va de lado
/// a lado (+X) y `v` hacia adelante (+Z), con sus tangentes para el mapa de
/// normales.
fn lying_quad() -> Mesh {
    let corners = [[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]];
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        corners.map(|[x, z]| [x, 0.0f32, z]).to_vec(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; 4])
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_UV_0,
        corners.map(|[x, z]| [x + 0.5f32, z + 0.5]).to_vec(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_TANGENT, vec![[1.0f32, 0.0, 0.0, 1.0]; 4])
    .with_inserted_indices(Indices::U16(vec![0, 2, 1, 0, 3, 2]))
}

/// Un ruido de valor suave en `(x, y)` con celdas de `cell`, de 0 a 1.
fn noise(x: f32, y: f32, cell: f32, salt: f32) -> f32 {
    let (gx, gy) = (x / cell, y / cell);
    let (ix, iy) = (gx.floor(), gy.floor());
    let (fx, fy) = (gx - ix, gy - iy);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let h = |a: f32, b: f32| hash2(ix + a + salt, iy + b - salt);
    let top = h(0.0, 0.0) + (h(1.0, 0.0) - h(0.0, 0.0)) * sx;
    let bottom = h(0.0, 1.0) + (h(1.0, 1.0) - h(0.0, 1.0)) * sx;
    top + (bottom - top) * sy
}

/// 1 dentro de `d < 0` y 0 fuera, con un borde suave de `soft`.
fn inside(d: f32, soft: f32) -> f32 {
    let t = (0.5 - d / soft).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// La forma de la huella de `source` en `(x, y)`, del centro a los bordes de
/// -1 a 1 (`y` hacia adelante): su hondura (0 a 1) y el lomo de tierra que
/// levanta alrededor (0 a 1). Sus bordes se desmoronan: la tierra nunca
/// guarda un contorno limpio.
fn shape(source: Source, x: f32, y: f32) -> (f32, f32) {
    let crumble = 0.16 * (noise(x * 6.0, y * 6.0, 1.0, 2.3) - 0.5);
    let (x, y) = (x * (1.0 + crumble), y * (1.0 - crumble));
    match source {
        Source::Hoof => {
            // Un vaso descalzo: la pared (más honda, carga el peso), la palma
            // cóncava, la ranilla en V con la punta hacia adelante y los
            // talones partidos atrás.
            let width = 0.86 + 0.1 * y;
            let r = ((x / width).powi(2) + (y / 0.92).powi(2)).sqrt();
            let heels =
                inside(x.abs() * 3.0 - 0.15 + (y + 1.0) * 4.0, 0.4) * inside(-y - 0.75, 0.2);
            let outline = inside(r - 1.0, 0.12) * (1.0 - heels);
            let wall = inside((r - 0.86).abs() - 0.1, 0.1);
            let frog = inside(x.abs() - (0.12 - y) * 0.5, 0.1) * inside(y - 0.12, 0.2);
            let depth = outline * (0.8 + 0.2 * wall).max(frog);
            let lip = inside((r - 1.1).abs() - 0.1, 0.18) * (0.6 + 0.4 * y.abs());
            (depth, lip * (1.0 - outline))
        }
        Source::Foot => {
            // La suela de una alpargata, de una pieza: la planta ancha, la
            // cintura y el talón redondo, con el yute trenzado en espiral.
            let ball = (-((y - 0.35) / 0.35).powi(2)).exp();
            let heel = (-((y + 0.65) / 0.25).powi(2)).exp();
            let width = 0.5 + 0.38 * ball + 0.12 * heel;
            let r = ((x / width).powi(2) + (y / 0.95).powi(6)).sqrt();
            let outline = inside(r - 1.0, 0.1);
            let jute = 0.85 + 0.15 * (r * 44.0).sin();
            let depth = outline * jute;
            let lip = inside((r - 1.12).abs() - 0.08, 0.16);
            (depth, lip * 0.5 * (1.0 - outline))
        }
        Source::Wheel => {
            // El surco de una llanta: un fondo chato con los bordes
            // levantados, y las puntas que se funden con el tramo de al lado.
            let ends = inside(y.abs() - 0.92, 0.12);
            let depth = inside(x.abs() - 0.62, 0.3) * ends;
            let lip = inside((x.abs() - 0.86).abs() - 0.1, 0.16) * ends;
            (depth, lip)
        }
    }
}

/// El relieve de la huella de `source`: su mapa de normales, que la luz del
/// sol sombrea en el shader, y lo que ninguna luz explica, horneado en cuánto
/// deja pasar del suelo de abajo (gris) y dónde (opacidad): el fondo hondo y
/// el pie de sus paredes, adonde llega menos cielo, y el fondo mojado
/// ([`MUD_TINT`]).
fn relief(source: Source) -> (Image, Image) {
    let size = footprint(source);
    let (w, h) = match source {
        Source::Wheel => (48, 384),
        _ => (128, 128),
    };
    // Cuántos metros se hunde lo más hondo y sube el lomo.
    let (sink, rise) = match source {
        Source::Hoof => (0.025, 0.008),
        Source::Foot => (0.012, 0.004),
        Source::Wheel => (0.02, 0.008),
    };
    let salt = kind(source) as f32 * 13.7;
    let at = |px: f32, py: f32| {
        let (x, y) = (px / w as f32 * 2.0 - 1.0, py / h as f32 * 2.0 - 1.0);
        let (depth, lip) = shape(source, x, y);
        // Los grumos de la tierra, en milímetros: el mismo grano para
        // cualquier clase de huella.
        let (mx, my) = (x * size.x * 500.0, y * size.y * 500.0);
        let grit = noise(mx, my, 9.0, salt) * 0.6 + noise(mx, my, 3.0, salt + 5.1) * 0.4;
        let height = -sink * depth * (0.94 + 0.12 * grit) + rise * lip * (0.5 + grit);
        (depth, lip, grit, height)
    };
    let (dx, dy) = (size.x / w as f32, size.y / h as f32);
    let mut color = Vec::with_capacity(w * h * 4);
    let mut normal = Vec::with_capacity(w * h * 4);
    for row in 0..h {
        for col in 0..w {
            let (px, py) = (col as f32 + 0.5, row as f32 + 0.5);
            let (depth, lip, grit, _) = at(px, py);
            let slope = Vec2::new(
                (at(px + 1.0, py).3 - at(px - 1.0, py).3) / (2.0 * dx),
                (at(px, py + 1.0).3 - at(px, py - 1.0).3) / (2.0 * dy),
            );
            // La tangente apunta a +u y la bitangente a -v (la convención de
            // glTF): el verde sube con la pendiente hacia adelante.
            let n = Vec3::new(-slope.x, slope.y, 1.0).normalize() * 0.5 + 0.5;
            normal.extend(n.to_array().map(|c| (c * 255.0) as u8));
            normal.push(255);
            let wall = slope.length().min(1.0);
            let shade = 1.0 - (0.08 * depth * (0.5 + grit) + 0.08 * wall * depth);
            let floor = Vec3::ONE + (MUD_TINT - Vec3::ONE) * depth;
            let rgb = (floor * shade * 255.0).to_array().map(|c| c as u8);
            let alpha = (1.5 * (depth + lip) + 2.0 * wall).min(1.0);
            color.extend_from_slice(&rgb);
            color.push((alpha * 255.0) as u8);
        }
    }
    let image = |data, format| {
        let mut image = Image::new(
            Extent3d {
                width: w as u32,
                height: h as u32,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            data,
            format,
            RenderAssetUsages::RENDER_WORLD,
        );
        image.sampler = ImageSampler::linear();
        image
    };
    (
        image(color, TextureFormat::Rgba8UnormSrgb),
        image(normal, TextureFormat::Rgba8Unorm),
    )
}

/// Deja la huella de cada pisada de este frame en el suelo que la guarda (antes
/// de que [`StirDust`] las gaste en polvo), y envejece y borra las que hay.
#[allow(clippy::too_many_arguments)]
fn press_prints(
    time: Res<Time>,
    terrain: Option<Res<WorldTerrain>>,
    towns: Option<Res<WorldTowns>>,
    kicks: Res<Kicks>,
    pool: Res<PrintPool>,
    shared: Res<PrintMaterials>,
    mut pressed: ResMut<Pressed>,
    mut prints: Query<(
        &mut Print,
        &mut Transform,
        &mut MeshMaterial3d<PrintMaterial>,
        &mut Visibility,
    )>,
) {
    let dt = time.delta_secs();
    if let Some(world) = terrain.as_deref() {
        let world: &Terrain = &world.0;
        let towns: &[TownLayout] = towns.as_deref().map_or(&[], |t| &t.0);
        let mut spent: Vec<f32> = pool
            .0
            .iter()
            .map(|&e| prints.get(e).map_or(1.0, |(p, ..)| p.spent()))
            .collect();
        for kick in kicks.pending() {
            let g = GroundVec2::new(kick.at.x, -kick.at.z);
            if !keeps_print(footing(world, towns, g)) {
                continue;
            }
            pressed.0 = pressed.0.wrapping_add(1);
            let slot = recycle(&spent);
            spent[slot] = 0.0;
            let Ok((mut print, mut transform, ..)) = prints.get_mut(pool.0[slot]) else {
                continue;
            };
            // Un pie o un vaso nunca cae del todo derecho; un surco sí.
            let yaw = match kick.source {
                Source::Wheel => 0.0,
                _ => (jitter(pressed.0, 0) - 0.5) * 0.3,
            };
            *transform = lay(kick, yaw, |x, y| world.height(x, y));
            *print = Print {
                source: kick.source,
                depth: 0.65 + 0.35 * kick.strength.clamp(0.0, 1.0),
                // Recién hecha: su paso de este frame la envejece.
                age: -dt,
                level: LEVELS,
            };
        }
    }
    for (mut print, _, mut material, mut visibility) in &mut prints {
        if print.level == 0 {
            continue;
        }
        print.age += dt;
        let now = level(print.depth * fade(print.age, LIFE));
        if now == 0 {
            print.level = 0;
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        }
        if now != print.level || *visibility == Visibility::Hidden {
            print.level = now;
            material.0 = shared.get(print.source, now);
            visibility.set_if_neq(Visibility::Inherited);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kick(source: Source) -> Kick {
        Kick {
            source,
            at: Vec3::new(10.0, 0.3, -4.0),
            travel: Vec3::X,
            strength: 1.0,
        }
    }

    #[test]
    fn only_mud_keeps_prints() {
        assert!(keeps_print(Footing::Mud(0.5)));
        assert!(!keeps_print(Footing::Dust(1.0)));
        assert!(!keeps_print(Footing::Firm));
        assert!(!keeps_print(Footing::Water));
    }

    #[test]
    fn a_print_holds_then_fades_out() {
        assert_eq!(level(fade(0.0, LIFE)), LEVELS);
        assert_eq!(level(fade(LIFE * HOLD, LIFE)), LEVELS);
        let mid = level(fade(LIFE * 0.75, LIFE));
        assert!(0 < mid && mid < LEVELS, "{mid}");
        assert_eq!(level(fade(LIFE, LIFE)), 0);
    }

    #[test]
    fn a_print_lies_on_the_slope_toe_ahead_and_a_rut_trails_its_wheel() {
        // Un suelo que sube hacia el este: 10 cm por metro.
        let slope = |x: f32, _: f32| 0.1 * x;
        let hoof = lay(&kick(Source::Hoof), 0.0, slope);
        assert!((hoof.translation.y - (1.0 + RAISE)).abs() < 1e-4);
        let up = hoof.rotation * Vec3::Y;
        assert!(up.x < -0.09 && up.y > 0.99, "{up}");
        assert!((hoof.rotation * Vec3::Z).x > 0.99);
        let rut = lay(&kick(Source::Wheel), 0.0, slope);
        assert!((rut.translation.x - (10.0 - WHEEL_SPACING / 2.0)).abs() < 1e-4);
        assert!(rut.scale.z > WHEEL_SPACING);
    }
}
