//! Lagunas: una lámina plana de agua en cada cuenca, con totoras a lo largo de
//! sus orillas.
//!
//! Cada [`Lagoon`] del [`WorldTerrain`] recibe dos mallas al inicio, cada una
//! en una entidad ubicada en el medio de la laguna (los rangos de visibilidad
//! se miden desde allí): su agua, una lámina al nivel del agua que se extiende
//! un poco por debajo de la barranca para que el suelo oculte su borde, y su
//! franja de juncos, matas de hojas finas en manchones a lo largo de la línea
//! del agua, fusionadas en una sola malla. Todas las lagunas comparten dos
//! materiales.
//!
//! El agua es la de una laguna de llanura, de un pardo oliva oscuro: lisa, con
//! la reflectancia del agua, más oscura y opaca hacia el medio y translúcida
//! sobre el barro cerca de la orilla (colores de vértice). Unas ondas finas y
//! tenues salen de un pequeño normal map repetible pintado en código (varias
//! capas de ondas pequeñas, con su propia cadena de mips para que se calme en
//! una superficie lisa a lo lejos), que se desplaza lentamente mediante su
//! transformación de UV: sin pasadas extra y sin reflejos en espacio de
//! pantalla, así que la iGPU Renoir paga un solo draw por ella. Los juncos se
//! dibujan dentro del rango compartido de objetos de los asentamientos
//! ([`crate::settlement::ranges`]), así que no agregan ningún rango de
//! visibilidad propio.

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::VisibilityRange;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Affine2;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use pampa_core::{Lagoon, Vec2 as GroundVec2};

use crate::coords::world_from_ground;
use crate::terrain::WorldTerrain;

/// Dibuja el agua y los juncos de cada laguna de [`WorldTerrain`].
pub struct LagoonPlugin;

impl Plugin for LagoonPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldTerrain>()
            .add_systems(Startup, spawn_lagoons)
            .add_systems(Update, ripple);
    }
}

/// La superficie de agua de una laguna.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct LagoonWater;

/// La franja de juncos de una laguna.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct LagoonReeds;

/// El material compartido del agua, desplazado por [`ripple`].
#[derive(Resource, Debug, Clone)]
struct WaterMaterial(Handle<StandardMaterial>);

/// Puntos alrededor del borde del agua.
const SHORE_POINTS: usize = 160;
/// Cuánto se extiende la lámina de agua más allá de la orilla, por debajo de la
/// barranca, en metros.
const UNDER_BANK: f32 = 3.0;
/// Lado de una baldosa del normal map de ondas sobre el agua, en metros:
/// pequeñas ondas de viento, no oleaje.
const RIPPLE_TILE: f32 = 2.5;
/// Qué tan rápido se desplazan las ondas, en baldosas por segundo.
const RIPPLE_DRIFT: Vec2 = Vec2::new(0.03, 0.017);
/// Lado del normal map de ondas pintado, en píxeles.
const RIPPLE_PIXELS: u32 = 64;
/// Inclinación máxima que las ondas dan a la superficie, como pendiente: tenue.
pub const RIPPLE_MAX_SLOPE: f32 = 0.08;
/// Cantidad máxima de triángulos de juncos que puede dibujar una laguna.
pub const REED_TRIANGLE_BUDGET: usize = 6000;
/// Hojas en una mata de juncos.
const BLADES: usize = 6;
/// Separación entre matas de juncos a lo largo de la orilla, en metros.
const CLUMP_SPACING: f32 = 1.1;
/// Altura de un junco, del más alto al más bajo, en metros.
pub const REED_HEIGHT: (f32, f32) = (1.5, 2.5);
/// El color (sRGB) y la opacidad del agua: sobre el medio profundo, sobre los
/// bajíos y en su borde bajo la barranca, donde se transparenta el barro.
const WATER_DEEP: [f32; 4] = [0.10, 0.13, 0.09, 0.94];
const WATER_SHALLOW: [f32; 4] = [0.16, 0.19, 0.12, 0.86];
const WATER_RIM: [f32; 4] = [0.24, 0.25, 0.17, 0.5];
/// Anillos de la lámina de agua: metros más allá de la orilla (negativo: dentro
/// del agua) y su color.
const WATER_RINGS: [(f32, [f32; 4]); 3] = [
    (-20.0, WATER_DEEP),
    (-6.0, WATER_SHALLOW),
    (UNDER_BANK, WATER_RIM),
];

fn spawn_lagoons(
    mut commands: Commands,
    terrain: Res<WorldTerrain>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    images: Option<ResMut<Assets<Image>>>,
) {
    let lagoons = &terrain.0.lagoons;
    if lagoons.is_empty() {
        return;
    }
    let ripples = images.map(|mut images| images.add(ripple_image()));
    let water = materials.add(water_material(ripples));
    let reeds = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.85,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    commands.insert_resource(WaterMaterial(water.clone()));
    let range = reed_range();
    for lagoon in lagoons {
        // En el medio de la laguna, para que el rango de los juncos se cuente
        // desde allí.
        let at = Transform::from_translation(world_from_ground(lagoon.center, lagoon.level));
        commands.spawn((
            Name::new(format!("{} water", lagoon.name)),
            LagoonWater,
            Mesh3d(meshes.add(water_mesh(lagoon))),
            MeshMaterial3d(water.clone()),
            at,
        ));
        commands.spawn((
            Name::new(format!("{} reeds", lagoon.name)),
            LagoonReeds,
            Mesh3d(meshes.add(reed_mesh(lagoon, &terrain.0))),
            MeshMaterial3d(reeds.clone()),
            at,
            range.clone(),
        ));
    }
}

/// El material del agua: blanco, para que lo pinten los colores de vértice;
/// liso, con la reflectancia del agua (cerca del 2% de frente, subiendo hacia
/// los ángulos rasantes), nunca metálico, y mezclado para que los bajíos dejen
/// ver el barro.
pub(crate) fn water_material(ripples: Option<Handle<Image>>) -> StandardMaterial {
    StandardMaterial {
        base_color: Color::WHITE,
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.06,
        metallic: 0.0,
        reflectance: 0.35,
        normal_map_texture: ripples,
        uv_transform: Affine2::from_scale(Vec2::splat(1.0 / RIPPLE_TILE)),
        ..default()
    }
}

/// El rango de los juncos: el compartido de los asentamientos para objetos.
fn reed_range() -> VisibilityRange {
    crate::settlement::ranges()[1].clone()
}

/// Desplaza las ondas sobre todas las lagunas.
fn ripple(
    time: Res<Time>,
    water: Option<Res<WaterMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(water) = water else { return };
    let Some(mut material) = materials.get_mut(&water.0) else {
        return;
    };
    let drift = RIPPLE_DRIFT * time.elapsed_secs();
    material.uv_transform = Affine2::from_scale_angle_translation(
        Vec2::splat(1.0 / RIPPLE_TILE),
        0.0,
        drift - drift.floor(),
    );
}

/// La lámina de agua, relativa al medio de la laguna al nivel del agua: un
/// abanico desde el medio hasta el anillo profundo, y luego tiras hacia afuera
/// a través de los bajíos hasta [`UNDER_BANK`] metros más allá de la orilla,
/// cada anillo pintado con su color de [`WATER_RINGS`]; plana, mirando hacia
/// arriba, con sus UV en metros del mundo (el material las escala a baldosas de
/// ondas).
pub(crate) fn water_mesh(lagoon: &Lagoon) -> Mesh {
    let n = SHORE_POINTS;
    let srgba = |c: [f32; 4]| LinearRgba::from(Color::srgba(c[0], c[1], c[2], c[3])).to_f32_array();
    let mut points = vec![lagoon.center];
    let mut colors = vec![srgba(WATER_DEEP)];
    for (offset, color) in WATER_RINGS {
        points.extend(lagoon.ring(n, offset));
        colors.extend(std::iter::repeat_n(srgba(color), n));
    }
    let positions: Vec<[f32; 3]> = points
        .iter()
        .map(|&p| world_from_ground(p - lagoon.center, 0.0).to_array())
        .collect();
    let uvs: Vec<[f32; 2]> = points.iter().map(|p| [p.x, -p.y]).collect();
    let count = points.len();
    let ring = |r: usize, k: usize| (1 + r * n + k % n) as u32;
    let mut indices = Vec::with_capacity(n * 3 * (1 + 2 * (WATER_RINGS.len() - 1)));
    for k in 0..n {
        // El sentido antihorario sobre el suelo es horario visto desde arriba
        // con el norte en -Z de Bevy, así que cada triángulo va primero hacia
        // afuera y luego a lo largo, para mirar hacia arriba.
        indices.extend([0, ring(0, k), ring(0, k + 1)]);
        for r in 0..WATER_RINGS.len() - 1 {
            let (a, b) = (ring(r, k), ring(r, k + 1));
            let (c, d) = (ring(r + 1, k), ring(r + 1, k + 1));
            indices.extend([a, c, d, a, d, b]);
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; count])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_attribute(Mesh::ATTRIBUTE_TANGENT, vec![[1.0, 0.0, 0.0, 1.0]; count])
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices))
}

/// Pequeño hash determinista en `[0, 1)`.
fn unit(seed: u32) -> f32 {
    let mut v = seed.wrapping_mul(0x9e37_79b9) ^ 0x7a3c_51e9;
    v ^= v >> 16;
    v = v.wrapping_mul(0x7feb_352d);
    v ^= v >> 15;
    v = v.wrapping_mul(0x846c_a68b);
    v ^= v >> 16;
    (v >> 8) as f32 / (1u32 << 24) as f32
}

/// Colores de los juncos, sRGB: verde fresco, oliva y el ocre seco de los
/// tallos viejos.
const REED_COLORS: [[f32; 3]; 3] = [[0.36, 0.45, 0.22], [0.47, 0.49, 0.26], [0.62, 0.55, 0.36]];

/// Las totoras de `lagoon`, relativas a su medio al nivel del agua: matas de
/// hojas finas en dos hileras sueltas, una en los bajíos y otra en la línea del
/// agua, agrupadas en manchones a lo largo de partes de la orilla, con trechos
/// abiertos entre ellos por donde baja el ganado a beber. Cada hoja es un
/// triángulo de doble cara parado sobre `terrain`, de [`REED_HEIGHT`] de alto,
/// algo inclinado.
pub(crate) fn reed_mesh(lagoon: &Lagoon, terrain: &pampa_core::Terrain) -> Mesh {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let perimeter: f32 = {
        let ring = lagoon.shore(SHORE_POINTS);
        (0..SHORE_POINTS)
            .map(|k| ring[k].distance(ring[(k + 1) % SHORE_POINTS]))
            .sum()
    };
    let clumps = (perimeter / CLUMP_SPACING) as usize;
    let mut seed = 0u32;
    let mut next = || {
        seed = seed.wrapping_add(1);
        unit(seed)
    };
    for row in 0..2 {
        // En los bajíos (cerca de un metro de franja de juncos dentro del
        // agua), y luego en la orilla, justo por encima del agua.
        let (lo, hi) = if row == 0 { (-3.5, -1.0) } else { (-1.0, 1.2) };
        for k in 0..clumps {
            let angle = (k as f32 + next()) / clumps as f32 * std::f32::consts::TAU;
            // Manchones: dos ondas lentas alrededor de la orilla; donde su suma
            // es alta los juncos crecen tupidos, y en el resto la orilla queda
            // abierta.
            let belt = (5.0 * angle + 0.7).sin() + 0.6 * (11.0 * angle + 2.1).sin();
            if belt < 0.2 || next() > 0.9 {
                continue;
            }
            let offset = lo + (hi - lo) * next();
            let r = lagoon.shore_radius(angle) + offset;
            let dir = GroundVec2::new(angle.cos(), angle.sin());
            let at = lagoon.center + dir * r;
            let base = terrain.height(at.x, at.y) - lagoon.level;
            for _ in 0..BLADES {
                let yaw = next() * std::f32::consts::TAU;
                let side = GroundVec2::new(yaw.cos(), yaw.sin()) * 0.035;
                let foot = at + GroundVec2::new(next() - 0.5, next() - 0.5) * 0.7;
                let lean = GroundVec2::new(next() - 0.5, next() - 0.5) * 0.5;
                let height = REED_HEIGHT.0 + (REED_HEIGHT.1 - REED_HEIGHT.0) * next();
                let tip = foot + lean - lagoon.center;
                let foot = foot - lagoon.center;
                let corners = [
                    world_from_ground(foot - side, base - 0.1),
                    world_from_ground(foot + side, base - 0.1),
                    world_from_ground(tip, base + height - 0.1),
                ];
                let normal = (corners[1] - corners[0])
                    .cross(corners[2] - corners[0])
                    .normalize_or(Vec3::Y);
                let rgb = REED_COLORS[(next() * 3.0) as usize % 3];
                let color = LinearRgba::from(Color::srgb(rgb[0], rgb[1], rgb[2])).to_f32_array();
                positions.extend(corners.map(|c| c.to_array()));
                normals.extend([normal.to_array(); 3]);
                colors.extend([color; 3]);
            }
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
}

/// Capas de ondas del mapa de ondas: (ondas a lo largo de x, ondas a lo largo
/// de y, altura, fase), con cantidades enteras de ondas a lo ancho de la
/// baldosa para que se repita sin costuras; las capas más finas son más bajas.
const RIPPLE_WAVES: [(f32, f32, f32, f32); 6] = [
    (3.0, 1.0, 0.0018, 0.0),
    (-2.0, 4.0, 0.0014, 1.7),
    (5.0, -3.0, 0.0010, 4.1),
    (7.0, 6.0, 0.0006, 2.3),
    (-9.0, 5.0, 0.0004, 5.2),
    (11.0, -10.0, 0.0003, 0.9),
];

/// El normal map de ondas: las [`RIPPLE_WAVES`] sumadas en una baldosa tenue y
/// sin costuras de pequeñas ondas de viento, con una cadena de mips filtrada
/// por caja hasta un píxel (la superficie se calma como un espejo a lo lejos en
/// lugar de titilar); guardado en lineal (un normal map es un dato, no un
/// color).
pub(crate) fn ripple_image() -> Image {
    let n = RIPPLE_PIXELS as usize;
    let tau = std::f32::consts::TAU;
    let mut level: Vec<Vec3> = Vec::with_capacity(n * n);
    for y in 0..n {
        for x in 0..n {
            let (u, v) = (x as f32 / n as f32, y as f32 / n as f32);
            // Pendiente de las capas sumadas a lo largo de u y v, por metro de
            // agua.
            let (mut du, mut dv) = (0.0, 0.0);
            for (kx, ky, h, phase) in RIPPLE_WAVES {
                let c = (tau * (kx * u + ky * v) + phase).cos() * h * tau / RIPPLE_TILE;
                du += c * kx;
                dv += c * ky;
            }
            level.push(Vec3::new(-du, -dv, 1.0).normalize());
        }
    }
    let byte = |c: f32| ((c * 0.5 + 0.5) * 255.0).round() as u8;
    let mut data = Vec::new();
    let mut side = n;
    let mut mips = 0;
    loop {
        for normal in &level {
            data.extend([byte(normal.x), byte(normal.y), byte(normal.z), 255]);
        }
        mips += 1;
        if side == 1 {
            break;
        }
        let half = side / 2;
        level = (0..half * half)
            .map(|i| {
                let (x, y) = (2 * (i % half), 2 * (i / half));
                (level[y * side + x]
                    + level[y * side + x + 1]
                    + level[(y + 1) * side + x]
                    + level[(y + 1) * side + x + 1])
                    .normalize()
            })
            .collect();
        side = half;
    }
    let mut image = Image::new(
        Extent3d {
            width: RIPPLE_PIXELS,
            height: RIPPLE_PIXELS,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data[..n * n * 4].to_vec(),
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = mips;
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mipmap_filter: ImageFilterMode::Linear,
        ..ImageSamplerDescriptor::linear()
    });
    image
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;
    use pampa_core::{LagoonConfig, Terrain};

    fn arocena() -> (Terrain, Lagoon) {
        let config = LagoonConfig {
            name: "Laguna La Arocena".into(),
            center: GroundVec2::new(3401.0, 841.0),
            size: GroundVec2::new(215.0, 145.0),
            seed: 5,
        };
        let lagoon = Lagoon::new(&config, &Terrain::default());
        (Terrain::default().with_lagoon(lagoon.clone()), lagoon)
    }

    fn positions(mesh: &Mesh) -> Vec<[f32; 3]> {
        match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(VertexAttributeValues::Float32x3(v)) => v.clone(),
            _ => panic!("positions"),
        }
    }

    /// Punto del suelo de un vértice de malla ubicado en relación con el medio
    /// de `lagoon`.
    fn ground(lagoon: &Lagoon, p: [f32; 3]) -> GroundVec2 {
        lagoon.center + GroundVec2::new(p[0], -p[2])
    }

    #[test]
    fn the_water_is_one_flat_upward_sheet_reaching_under_the_bank() {
        let (terrain, lagoon) = arocena();
        let mesh = water_mesh(&lagoon);
        let points = positions(&mesh);
        assert!(points.iter().all(|p| p[1] == 0.0));
        // Su borde queda bajo la barranca, fuera del agua.
        let rim = &points[points.len() - SHORE_POINTS..];
        for &p in rim {
            let g = ground(&lagoon, p);
            assert!(!lagoon.is_water(g));
            assert!(terrain.height(g.x, g.y) > lagoon.level, "{g:?}");
        }
        // Todos los triángulos miran hacia arriba.
        let Some(Indices::U32(indices)) = mesh.indices() else {
            panic!("indices")
        };
        assert!(indices.len() / 3 < 2000);
        for t in indices.chunks(3) {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| Vec3::from(points[i as usize]));
            assert!((b - a).cross(c - a).y > 0.0);
        }
        // Oscura y casi opaca en el medio, translúcida sobre el barro en su
        // borde.
        let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("colors")
        };
        let (middle, edge) = (colors[0], colors[colors.len() - 1]);
        assert!(middle[3] > 0.9 && edge[3] < 0.6);
        assert!(middle[0] + middle[1] + middle[2] < edge[0] + edge[1] + edge[2]);
        // Pardo oliva, nunca arenoso: el verde al menos tan intenso como el
        // rojo.
        assert!(middle[1] >= middle[0] && middle[1] > middle[2]);
    }

    #[test]
    fn the_reeds_stand_in_patches_at_the_waterline_within_their_budget() {
        let (terrain, lagoon) = arocena();
        let mesh = reed_mesh(&lagoon, &terrain);
        let points = positions(&mesh);
        let triangles = points.len() / 3;
        assert!(
            (1000..=REED_TRIANGLE_BUDGET).contains(&triangles),
            "{triangles}"
        );
        // Cada hoja se levanta en el borde del agua, con 1.5 a 2.5 m de alto.
        let mut angles = Vec::new();
        for t in points.chunks(3) {
            let foot = ground(&lagoon, t[0]);
            let off = lagoon.offshore(foot);
            assert!((-5.0..=2.5).contains(&off), "{foot:?}: {off}");
            let height = t[2][1] - t[0][1];
            assert!(
                (REED_HEIGHT.0 - 0.01..=REED_HEIGHT.1 + 0.01).contains(&height),
                "{height}"
            );
            let d = foot - lagoon.center;
            angles.push(d.y.atan2(d.x));
        }
        // En manchones: algunos trechos de orilla están tupidos de juncos y
        // otros abiertos.
        let mut sectors = [0usize; 36];
        for a in angles {
            let k = ((a + std::f32::consts::PI) / std::f32::consts::TAU * 36.0) as usize;
            sectors[k.min(35)] += 1;
        }
        let open = sectors.iter().filter(|&&c| c == 0).count();
        let thick = sectors.iter().filter(|&&c| c > 60).count();
        assert!(open >= 4 && thick >= 8, "{sectors:?}");
        // Los mismos juncos cada vez.
        assert_eq!(positions(&reed_mesh(&lagoon, &terrain)), points);
    }

    #[test]
    fn the_ripple_map_is_faint_fine_and_seamless_with_mips() {
        let image = ripple_image();
        let data = image.data.as_ref().unwrap();
        let n = RIPPLE_PIXELS as usize;
        let normal = |x: usize, y: usize| {
            let i = (y * n + x) * 4;
            Vec3::new(
                data[i] as f32 / 127.5 - 1.0,
                data[i + 1] as f32 / 127.5 - 1.0,
                data[i + 2] as f32 / 127.5 - 1.0,
            )
        };
        for k in 0..n {
            // De un lado a otro de cada borde, el salto no es mayor que entre
            // vecinos.
            assert!(normal(0, k).distance(normal(n - 1, k)) < 0.05);
            assert!(normal(k, 0).distance(normal(k, n - 1)) < 0.05);
        }
        // Tenue: ninguna onda inclina el agua más de unos pocos grados.
        let mut steepest: f32 = 0.0;
        for y in 0..n {
            for x in 0..n {
                let m = normal(x, y);
                steepest = steepest.max(m.x.hypot(m.y) / m.z);
            }
        }
        assert!(steepest < RIPPLE_MAX_SLOPE, "{steepest}");
        assert!(steepest > 0.01, "{steepest}");
        // Fina: una baldosa equivale a un par de metros de agua.
        const { assert!(RIPPLE_TILE <= 3.0) };
        // Una cadena de mips completa, para que se calme con la distancia.
        assert_eq!(image.texture_descriptor.mip_level_count, 7);
        assert_eq!(
            data.len(),
            (64 * 64 + 32 * 32 + 16 * 16 + 64 + 16 + 4 + 1) * 4
        );
        assert_eq!(image.texture_descriptor.format, TextureFormat::Rgba8Unorm);
    }

    #[test]
    fn every_lagoon_shares_two_materials_and_the_props_range_from_its_middle() {
        let (terrain, lagoon) = arocena();
        let mut second = lagoon.clone();
        second.center = lagoon.center + GroundVec2::new(-1000.0, 0.0);
        let terrain = terrain.with_lagoon(second.clone());
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .init_resource::<Assets<Image>>()
            .insert_resource(WorldTerrain(terrain))
            .add_plugins(LagoonPlugin);
        app.update();
        app.update();
        let world = app.world_mut();
        let water: Vec<_> = world
            .query_filtered::<&MeshMaterial3d<StandardMaterial>, With<LagoonWater>>()
            .iter(world)
            .map(|m| m.0.id())
            .collect();
        let reeds: Vec<(_, VisibilityRange, Transform)> = world
            .query_filtered::<(
                &MeshMaterial3d<StandardMaterial>,
                &VisibilityRange,
                &Transform,
            ), With<LagoonReeds>>()
            .iter(world)
            .map(|(m, r, t)| (m.0.id(), r.clone(), *t))
            .collect();
        assert_eq!((water.len(), reeds.len()), (2, 2));
        assert_eq!(water[0], water[1]);
        assert_eq!(reeds[0].0, reeds[1].0);
        assert!(
            reeds
                .iter()
                .all(|(_, r, _)| *r == crate::settlement::ranges()[1])
        );
        // El rango se cuenta desde el medio de cada laguna, así que un jinete
        // en su orilla ve sus juncos.
        for (_, _, t) in &reeds {
            let at = GroundVec2::new(t.translation.x, -t.translation.z);
            assert!(
                at.distance(lagoon.center) < 1e-3 || at.distance(second.center) < 1e-3,
                "{at:?}"
            );
            assert!(lagoon.radii.x * 1.1 < crate::settlement::PROP_FAR);
        }
        assert_eq!(world.resource::<Assets<StandardMaterial>>().len(), 2);
        let id = water[0];
        let material = world
            .resource::<Assets<StandardMaterial>>()
            .get(id)
            .unwrap();
        assert!(material.normal_map_texture.is_some());
        assert_eq!(material.alpha_mode, AlphaMode::Blend);
        assert_eq!(material.metallic, 0.0);
        assert!(material.perceptual_roughness < 0.1);
    }
}
