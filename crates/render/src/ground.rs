//! Texturas del suelo para los chunks de terreno.
//!
//! Vienen de ambientCG, CC0 1.0 (Ground 109 y Grass 004; ver
//! assets/textures/ground/CREDITS.md). Se distribuyen como archivos PNM
//! binarios de 512×512 (mapas de color y de normales en P6, rugosidad y
//! oclusión ambiental en P5) convertidos a partir de los JPG de 1K de
//! ambientCG, así que no hace falta ningún decodificador de imágenes.
//!
//! El [`StandardMaterial`] de Bevy acepta una sola textura de color base, así
//! que aquí los dos conjuntos se mezclan, una sola vez, en un conjunto de
//! detalle repetible: tierra que asoma por los huecos del pasto. Su color se
//! mantiene con poca saturación y poco contraste, para que el color del suelo
//! por vértice (sequedad, tierra de pueblo, sombra) siga decidiendo el tono.
//! Los mapas de normales, rugosidad y oclusión ambiental se mezclan de la misma
//! forma. Las UV están en espacio del mundo ([`ground_uv`]), así que las
//! baldosas siguen sin costura a través de los bordes de los chunks.
//!
//! Bajo los manchones de monte, el suelo muestra hojarasca (ScatteredLeaves009,
//! CC0 1.0): [`GroundMaterial`] extiende el material estándar con la baldosa de
//! hojarasca ([`LitterExt`], `assets/shaders/ground_litter.wesl`), y el peso
//! por vértice sale de [`litter_weight`].
//!
//! Los caminos llevan solo Ground 102 (tierra compactada, ambientCG, CC0 1.0):
//! [`road_material`], sobre la superficie del camino que dibuja
//! [`crate::road`].

use std::fmt;
use std::path::Path;

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, Extent3d, TextureDimension, TextureFormat};
use bevy::shader::ShaderRef;

/// Dónde están las texturas del suelo, bajo el [`crate::AssetRoot`].
pub const GROUND_DIR: &str = "textures/ground";
/// Metros por baldosa de textura.
pub const TILE: f32 = 6.0;

/// Una imagen PNM binaria decodificada: uno (gris) o tres (RGB) canales de 8
/// bits.
#[derive(Debug, Clone, PartialEq)]
pub struct Pnm {
    pub width: u32,
    pub height: u32,
    pub channels: usize,
    pub data: Vec<u8>,
}

/// Por qué no se pudo leer una imagen PNM.
#[derive(Debug, Clone, PartialEq)]
pub struct PnmError(pub String);

impl fmt::Display for PnmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PnmError {}

/// Interpreta un archivo PNM binario: `P5` (gris) o `P6` (RGB), con comentarios
/// `#` permitidos en el encabezado y cualquier maxval de hasta 255.
pub fn parse_pnm(bytes: &[u8]) -> Result<Pnm, PnmError> {
    let channels = match bytes.get(..2) {
        Some(b"P5") => 1,
        Some(b"P6") => 3,
        _ => return Err(PnmError("not a binary PNM (P5 or P6)".into())),
    };
    let mut at = 2;
    let mut fields = [0u32; 3];
    for field in &mut fields {
        // Espacios en blanco y comentarios hasta el siguiente número.
        loop {
            match bytes.get(at) {
                Some(b) if b.is_ascii_whitespace() => at += 1,
                Some(b'#') => {
                    while bytes.get(at).is_some_and(|&b| b != b'\n') {
                        at += 1;
                    }
                }
                _ => break,
            }
        }
        let start = at;
        while bytes.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        *field = std::str::from_utf8(&bytes[start..at])
            .ok()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| PnmError("bad or missing header number".into()))?;
    }
    // Exactamente un byte de espacio en blanco termina el encabezado.
    if !bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
        return Err(PnmError("header not ended by whitespace".into()));
    }
    at += 1;
    let [width, height, maxval] = fields;
    if width == 0 || height == 0 || width > 8192 || height > 8192 {
        return Err(PnmError(format!("unsupported size {width}×{height}")));
    }
    if !(1..=255).contains(&maxval) {
        return Err(PnmError(format!("unsupported maxval {maxval}")));
    }
    let len = width as usize * height as usize * channels;
    let data = bytes
        .get(at..at + len)
        .ok_or_else(|| PnmError(format!("truncated: {len} bytes expected")))?;
    let data = if maxval == 255 {
        data.to_vec()
    } else {
        data.iter()
            .map(|&v| ((v.min(maxval as u8) as u32 * 255 + maxval / 2) / maxval) as u8)
            .collect()
    };
    Ok(Pnm {
        width,
        height,
        channels,
        data,
    })
}

/// Lee el archivo PNM en `path`.
pub fn load_pnm(path: &Path) -> Result<Pnm, PnmError> {
    let bytes = std::fs::read(path)
        .map_err(|e| PnmError(format!("cannot read {}: {e}", path.display())))?;
    parse_pnm(&bytes).map_err(|e| PnmError(format!("{}: {e}", path.display())))
}

/// Un conjunto de texturas de ambientCG a un tamaño común.
struct Maps {
    size: u32,
    color: Pnm,
    normal: Pnm,
    roughness: Pnm,
    occlusion: Pnm,
}

impl Maps {
    /// Lee el conjunto `id` (`Ground109`, `Grass004`) desde `dir`.
    fn load(dir: &Path, id: &str) -> Result<Self, PnmError> {
        let at = |name: &str| load_pnm(&dir.join(id).join(format!("{id}_{name}")));
        let maps = Self {
            color: at("Color.ppm")?,
            normal: at("NormalGL.ppm")?,
            roughness: at("Roughness.pgm")?,
            occlusion: at("AmbientOcclusion.pgm")?,
            size: 0,
        };
        let size = maps.color.width;
        let fits =
            |p: &Pnm, channels| p.width == size && p.height == size && p.channels == channels;
        if !(fits(&maps.color, 3) && fits(&maps.normal, 3))
            || !(fits(&maps.roughness, 1) && fits(&maps.occlusion, 1))
        {
            return Err(PnmError(format!("{id}: maps differ in size or kind")));
        }
        Ok(Self { size, ..maps })
    }

    fn rgb(p: &Pnm, i: usize) -> Vec3 {
        Vec3::new(
            p.data[i * 3] as f32,
            p.data[i * 3 + 1] as f32,
            p.data[i * 3 + 2] as f32,
        ) / 255.0
    }

    fn grey(p: &Pnm, i: usize) -> f32 {
        p.data[i] as f32 / 255.0
    }

    fn mean_luma(&self) -> f32 {
        let n = (self.size * self.size) as usize;
        (0..n).map(|i| luma(Self::rgb(&self.color, i))).sum::<f32>() / n as f32
    }
}

fn luma(c: Vec3) -> f32 {
    c.dot(Vec3::new(0.299, 0.587, 0.114))
}

fn smoothstep(lo: f32, hi: f32, x: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// El conjunto de detalle mezclado: color RGBA8 (sRGB), normal, y AO en el rojo
/// con la rugosidad en el verde (lineal), todos cuadrados de `size`.
pub struct GroundDetail {
    pub size: u32,
    pub color: Vec<u8>,
    pub normal: Vec<u8>,
    pub surface: Vec<u8>,
}

/// Cuánto puede apartarse el color de detalle del gris, y de su brillo medio:
/// poco, para que el color de vértice tenga la última palabra.
const SATURATION: f32 = 0.3;
const CONTRAST: f32 = 0.45;
/// Brillo medio del color de detalle (sRGB).
const BRIGHTNESS: f32 = 0.9;
/// Cuánto del relieve de los mapas se conserva: las pendientes propias del
/// suelo siguen siendo legibles.
const RELIEF: f32 = 0.6;

/// Mezcla tierra y pasto: pasto donde sus hojas son claras, y tierra que asoma
/// por los huecos oscuros.
fn blend(soil: &Maps, grass: &Maps) -> GroundDetail {
    let size = soil.size;
    let n = (size * size) as usize;
    let (soil_mean, grass_mean) = (soil.mean_luma(), grass.mean_luma());
    let (mut color, mut normal, mut surface) = (
        Vec::with_capacity(n * 4),
        Vec::with_capacity(n * 4),
        Vec::with_capacity(n * 4),
    );
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    for i in 0..n {
        let g = Maps::rgb(&grass.color, i);
        let s = Maps::rgb(&soil.color, i);
        let m = smoothstep(0.75, 1.15, luma(g) / grass_mean.max(1e-3));
        let c = (s / soil_mean.max(1e-3)).lerp(g / grass_mean.max(1e-3), m);
        let y = luma(c);
        let c = Vec3::splat(y).lerp(c, SATURATION);
        let c = Vec3::splat(BRIGHTNESS) + (c - Vec3::ONE) * CONTRAST * BRIGHTNESS;
        color.extend([byte(c.x), byte(c.y), byte(c.z), 255]);

        let decode = |p: &Pnm| Maps::rgb(p, i) * 2.0 - Vec3::ONE;
        let nv = decode(&soil.normal).lerp(decode(&grass.normal), m);
        let nv = Vec3::new(nv.x * RELIEF, nv.y * RELIEF, nv.z.max(0.05)).normalize_or(Vec3::Z);
        let nv = nv * 0.5 + Vec3::splat(0.5);
        normal.extend([byte(nv.x), byte(nv.y), byte(nv.z), 255]);

        let lerp = |a: f32, b: f32| a + (b - a) * m;
        let ao = lerp(
            Maps::grey(&soil.occlusion, i),
            Maps::grey(&grass.occlusion, i),
        );
        let rough = lerp(
            Maps::grey(&soil.roughness, i),
            Maps::grey(&grass.roughness, i),
        );
        // Se suaviza la oclusión igual que el relieve: sin pozos negros.
        surface.extend([byte(0.6 + 0.4 * ao), byte(rough.max(0.7)), 0, 255]);
    }
    GroundDetail {
        size,
        color,
        normal,
        surface,
    }
}

/// La tierra de un camino por sí sola: el color de Ground 102, un poco más gris
/// y plano que la foto para que el color de vértice fije el tono, con su
/// relieve, rugosidad y oclusión.
fn dirt(maps: &Maps) -> GroundDetail {
    let n = (maps.size * maps.size) as usize;
    let mean = maps.mean_luma();
    let (mut color, mut normal, mut surface) = (
        Vec::with_capacity(n * 4),
        Vec::with_capacity(n * 4),
        Vec::with_capacity(n * 4),
    );
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    for i in 0..n {
        let c = Maps::rgb(&maps.color, i) / mean.max(1e-3);
        let c = Vec3::splat(luma(c)).lerp(c, ROAD_SATURATION);
        let c = Vec3::splat(BRIGHTNESS) + (c - Vec3::ONE) * ROAD_CONTRAST * BRIGHTNESS;
        color.extend([byte(c.x), byte(c.y), byte(c.z), 255]);
        let nv = Maps::rgb(&maps.normal, i) * 2.0 - Vec3::ONE;
        let nv = Vec3::new(nv.x * RELIEF, nv.y * RELIEF, nv.z.max(0.05)).normalize_or(Vec3::Z);
        let nv = nv * 0.5 + Vec3::splat(0.5);
        normal.extend([byte(nv.x), byte(nv.y), byte(nv.z), 255]);
        let ao = Maps::grey(&maps.occlusion, i);
        let rough = Maps::grey(&maps.roughness, i);
        surface.extend([byte(0.6 + 0.4 * ao), byte(rough.max(0.75)), 0, 255]);
    }
    GroundDetail {
        size: maps.size,
        color,
        normal,
        surface,
    }
}

/// Cuánto color y contraste de Ground 102 conserva el camino: más que el suelo
/// de pasto, para que se distingan las huellas de las ruedas y las piedritas.
const ROAD_SATURATION: f32 = 0.5;
const ROAD_CONTRAST: f32 = 0.7;

/// Lee Ground 102 (tierra compactada) desde `dir`, para los caminos.
pub fn load_road_detail(dir: &Path) -> Result<GroundDetail, PnmError> {
    Ok(dirt(&Maps::load(dir, "Ground102")?))
}

/// Lee y mezcla Ground 109 (tierra) y Grass 004 desde `dir`.
pub fn load_detail(dir: &Path) -> Result<GroundDetail, PnmError> {
    let soil = Maps::load(dir, "Ground109")?;
    let grass = Maps::load(dir, "Grass004")?;
    if soil.size != grass.size {
        return Err(PnmError("soil and grass maps differ in size".into()));
    }
    Ok(blend(&soil, &grass))
}

/// Cobertura de monte en la que empieza la hojarasca y en la que es completa.
/// El borde propio del monte ya es una rampa suave de ruido; esto ensancha el
/// difuminado.
const LITTER_FROM: f32 = 0.1;
const LITTER_FULL: f32 = 0.8;

/// Cuánta hojarasca hay en un punto del suelo, en `[0, 1]`, dada la cobertura
/// de `monte` allí ([`pampa_core::FloraSite::monte`]) y qué tan despejado
/// (`cleared`) está el suelo (0 en la pampa abierta, 1 en el suelo nivelado de
/// un pueblo, un camino o el barro de una laguna). Se guarda por vértice, en el
/// alfa del color del suelo, y el shader del suelo (`ground_litter.wesl`)
/// mezcla las hojas.
pub fn litter_weight(monte: f32, cleared: f32) -> f32 {
    smoothstep(LITTER_FROM, LITTER_FULL, monte) * (1.0 - cleared.clamp(0.0, 1.0))
}

/// La baldosa de color de la hojarasca (RGBA8, sRGB) y su lado.
pub struct LitterTile {
    pub size: u32,
    pub color: Vec<u8>,
}

/// Cuánto del color y el contraste de las hojas se conserva: más que el suelo
/// de pasto, para que las hojas se lean como hojarasca marrón y no como pasto
/// teñido.
const LITTER_SATURATION: f32 = 0.7;
const LITTER_CONTRAST: f32 = 0.6;
/// Brillo medio de la hojarasca (sRGB): bastante por debajo del [`BRIGHTNESS`]
/// del suelo, para que el piso del monte sea más oscuro que la pampa abierta.
/// El shader multiplica el suelo por esta baldosa, así que es una proporción.
const LITTER_BRIGHTNESS: f32 = 0.6;
/// Baldosas de hojarasca por baldosa de suelo: las hojas se repiten cada 2 m,
/// no cada 6 m.
pub const LITTER_TILING: f32 = 3.0;

/// Lee ScatteredLeaves009 (solo el mapa de color) desde `dir`, normalizado por
/// la media y entonado como el resto del suelo.
pub fn load_litter(dir: &Path) -> Result<LitterTile, PnmError> {
    let id = "ScatteredLeaves009";
    let color = load_pnm(&dir.join(id).join(format!("{id}_Color.ppm")))?;
    if color.channels != 3 || color.width != color.height {
        return Err(PnmError(format!(
            "{id}: colour map is not a square RGB image"
        )));
    }
    let n = (color.width * color.height) as usize;
    let mean = (0..n).map(|i| luma(Maps::rgb(&color, i))).sum::<f32>() / n as f32;
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let mut out = Vec::with_capacity(n * 4);
    for i in 0..n {
        let c = Maps::rgb(&color, i) / mean.max(1e-3);
        let c = Vec3::splat(luma(c)).lerp(c, LITTER_SATURATION);
        let c =
            Vec3::splat(LITTER_BRIGHTNESS) + (c - Vec3::ONE) * LITTER_CONTRAST * LITTER_BRIGHTNESS;
        out.extend([byte(c.x), byte(c.y), byte(c.z), 255]);
    }
    Ok(LitterTile {
        size: color.width,
        color: out,
    })
}

/// `pixels` (RGBA8, cuadrado de `size`) con su cadena de mips completa
/// agregada, cada nivel un filtro de caja de 2×2 del anterior: el suelo en
/// baldosas no debe titilar.
fn with_mips(size: u32, pixels: Vec<u8>) -> (Vec<u8>, u32) {
    let mut data = pixels;
    let (mut level, mut side, mut levels) = (0, size as usize, 1);
    while side > 1 && side.is_multiple_of(2) {
        let half = side / 2;
        let src = data[level..].to_vec();
        level = data.len();
        for y in 0..half {
            for x in 0..half {
                for c in 0..4 {
                    let at = |dx: usize, dy: usize| {
                        src[((2 * y + dy) * side + 2 * x + dx) * 4 + c] as u32
                    };
                    data.push(((at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1) + 2) / 4) as u8);
                }
            }
        }
        side = half;
        levels += 1;
    }
    (data, levels)
}

fn image(size: u32, pixels: Vec<u8>, format: TextureFormat) -> Image {
    let (data, levels) = with_mips(size, pixels);
    let mut image = Image::new_uninit(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        format,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = levels;
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 8,
        ..default()
    });
    image
}

/// Lo que el shader del suelo agrega al material estándar: la baldosa de
/// hojarasca (ScatteredLeaves009, ambientCG, CC0 1.0). Sin ella el sampler usa
/// blanco por defecto y el suelo queda como está.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub struct LitterExt {
    #[texture(100)]
    #[sampler(101)]
    pub litter: Option<Handle<Image>>,
}

impl MaterialExtension for LitterExt {
    /// `assets/shaders/ground_litter.wesl`: el fragment shader estándar, con el
    /// color del suelo oscurecido hacia la hojarasca según el alfa del color de
    /// vértice ([`litter_weight`]).
    fn fragment_shader() -> ShaderRef {
        "shaders/ground_litter.wesl".into()
    }
}

/// El material del terreno: [`StandardMaterial`] más [`LitterExt`].
pub type GroundMaterial = ExtendedMaterial<StandardMaterial, LitterExt>;

/// Dibuja [`GroundMaterial`]. Necesita un renderer, como [`crate::SkyPlugin`];
/// sin él el terreno igual construye sus chunks, solo con los assets.
pub struct GroundMaterialPlugin;

impl Plugin for GroundMaterialPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<GroundMaterial>::default());
    }
}

/// El material del terreno: blanco y mate, con su color tomado de la pintura de
/// vértices. Lleva las texturas de detalle del suelo de ambientCG (Ground 109 y
/// Grass 004, CC0 1.0; ver assets/textures/ground/CREDITS.md) cuando se pueden
/// leer desde `root`, y la hojarasca del monte (ScatteredLeaves009,
/// [`load_litter`]) cuando también se puede leer; si no, se registra una
/// advertencia y se dibuja el suelo liso pintado por vértice.
pub fn ground_material(root: Option<&Path>, images: Option<&mut Assets<Image>>) -> GroundMaterial {
    let mut material = GroundMaterial {
        base: StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 1.0,
            reflectance: 0.1,
            ..default()
        },
        extension: LitterExt::default(),
    };
    let (Some(root), Some(images)) = (root, images) else {
        return material;
    };
    let started = std::time::Instant::now();
    match load_detail(&root.join(GROUND_DIR)) {
        Ok(detail) => {
            info!(
                "ground textures: {0}×{0} detail blended in {1:.0} ms",
                detail.size,
                started.elapsed().as_secs_f64() * 1000.0
            );
            wear(&mut material.base, detail, images);
            match load_litter(&root.join(GROUND_DIR)) {
                Ok(tile) => {
                    material.extension.litter = Some(images.add(image(
                        tile.size,
                        tile.color,
                        TextureFormat::Rgba8UnormSrgb,
                    )));
                }
                Err(error) => warn!("leaf litter not loaded ({error}); no litter in the monte"),
            }
        }
        Err(error) => warn!("ground textures not loaded ({error}); drawing plain ground"),
    }
    material
}

/// Pone `detail` en `material`: color, normal, y el mapa de oclusión y
/// rugosidad.
fn wear(material: &mut StandardMaterial, detail: GroundDetail, images: &mut Assets<Image>) {
    let size = detail.size;
    let surface = images.add(image(size, detail.surface, TextureFormat::Rgba8Unorm));
    material.base_color_texture =
        Some(images.add(image(size, detail.color, TextureFormat::Rgba8UnormSrgb)));
    material.normal_map_texture =
        Some(images.add(image(size, detail.normal, TextureFormat::Rgba8Unorm)));
    material.metallic_roughness_texture = Some(surface.clone());
    // Metallic queda en 0: el canal azul del mapa de superficie también es 0.
    material.occlusion_texture = Some(surface);
}

/// El material de la superficie del camino: tierra Ground 102 de ambientCG (CC0
/// 1.0; ver assets/textures/ground/CREDITS.md) cuando se puede leer desde
/// `root`, teñida por el color de vértice, cuyo alfa difumina los bordes del
/// camino hacia el pasto. Se mezcla sobre el suelo, acercado un poco a la
/// cámara para que nunca compita con el terreno que tiene debajo. Sin las
/// texturas se registra una advertencia y se dibuja el camino liso pintado por
/// vértice.
pub fn road_material(root: Option<&Path>, images: Option<&mut Assets<Image>>) -> StandardMaterial {
    let mut material = StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 1.0,
        reflectance: 0.1,
        alpha_mode: AlphaMode::Blend,
        depth_bias: 2.0,
        ..default()
    };
    let (Some(root), Some(images)) = (root, images) else {
        return material;
    };
    match load_road_detail(&root.join(GROUND_DIR)) {
        Ok(detail) => wear(&mut material, detail, images),
        Err(error) => warn!("road texture not loaded ({error}); drawing plain dirt"),
    }
    material
}

/// La UV del suelo del punto del mundo `p`: una baldosa cada [`TILE`] metros,
/// en función solo de la posición, para que los chunks coincidan en los
/// vértices compartidos.
pub fn ground_uv(p: Vec3) -> [f32; 2] {
    [p.x / TILE, p.z / TILE]
}

/// La tangente (con el signo de su bitangente en `w`) de un triángulo plano con
/// UV en espacio del mundo de [`ground_uv`].
pub fn face_tangent(p: [Vec3; 3], uv: [[f32; 2]; 3]) -> [f32; 4] {
    let (e1, e2) = (p[1] - p[0], p[2] - p[0]);
    let (du1, dv1) = (uv[1][0] - uv[0][0], uv[1][1] - uv[0][1]);
    let (du2, dv2) = (uv[2][0] - uv[0][0], uv[2][1] - uv[0][1]);
    let det = du1 * dv2 - du2 * dv1;
    let normal = e1.cross(e2).normalize_or(Vec3::Y);
    if det.abs() < 1e-12 {
        return [1.0, 0.0, 0.0, 1.0];
    }
    let r = 1.0 / det;
    let t = (e1 * dv2 - e2 * dv1) * r;
    let b = (e2 * du1 - e1 * du2) * r;
    let t = (t - normal * normal.dot(t)).normalize_or(Vec3::X);
    let w = if normal.cross(t).dot(b) < 0.0 {
        -1.0
    } else {
        1.0
    };
    [t.x, t.y, t.z, w]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pnm(header: &str, data: &[u8]) -> Vec<u8> {
        let mut bytes = header.as_bytes().to_vec();
        bytes.extend_from_slice(data);
        bytes
    }

    #[test]
    fn grey_and_colour_pnm_files_parse_with_comments() {
        let grey = parse_pnm(&pnm("P5\n# made by hand\n2 2\n255\n", &[0, 64, 128, 255])).unwrap();
        assert_eq!((grey.width, grey.height, grey.channels), (2, 2, 1));
        assert_eq!(grey.data, vec![0, 64, 128, 255]);

        let colour = parse_pnm(&pnm("P6 1 2 # inline\n255 ", &[1, 2, 3, 4, 5, 6])).unwrap();
        assert_eq!((colour.width, colour.height, colour.channels), (1, 2, 3));
        assert_eq!(colour.data, vec![1, 2, 3, 4, 5, 6]);

        // Un maxval menor se escala hasta 255.
        let scaled = parse_pnm(&pnm("P5\n2 1\n15\n", &[15, 0])).unwrap();
        assert_eq!(scaled.data, vec![255, 0]);
        // Se ignoran los bytes posteriores a la imagen; un valor de píxel 10
        // (`\n`) justo después del encabezado es un dato, no un espacio en
        // blanco.
        let tight = parse_pnm(&pnm("P5\n1 1\n255\n", &[10, 99])).unwrap();
        assert_eq!(tight.data, vec![10]);
    }

    #[test]
    fn the_shipped_maps_blend_into_a_quiet_near_white_detail() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets")
            .join(GROUND_DIR);
        let started = std::time::Instant::now();
        let detail = load_detail(&dir).unwrap();
        println!("ground detail in {:?}", started.elapsed());
        assert_eq!(detail.size, 512);
        let n = (detail.size * detail.size) as usize;
        assert_eq!(detail.color.len(), n * 4);
        let (mut sum, mut chroma) = (Vec3::ZERO, 0.0);
        for px in detail.color.chunks(4) {
            let c = Vec3::new(px[0] as f32, px[1] as f32, px[2] as f32) / 255.0;
            sum += c;
            chroma += c.max_element() - c.min_element();
        }
        let mean = sum / n as f32;
        // Claro y casi gris: el color de vértice conserva el tono.
        assert!(
            mean.min_element() > 0.8 && mean.max_element() < 0.97,
            "{mean:?}"
        );
        assert!(chroma / (n as f32) < 0.08, "{}", chroma / n as f32);
        // Las normales apuntan hacia afuera de la superficie; la AO nunca llega
        // al negro.
        assert!(detail.normal.chunks(4).all(|p| p[2] >= 128));
        assert!(detail.surface.chunks(4).all(|p| p[0] >= 150 && p[2] == 0));
    }

    #[test]
    fn the_road_dirt_keeps_more_grain_than_the_grass_ground_and_stays_bright() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets")
            .join(GROUND_DIR);
        let dirt = load_road_detail(&dir).unwrap();
        let ground = load_detail(&dir).unwrap();
        assert_eq!(dirt.size, 512);
        let spread = |d: &GroundDetail| {
            let lumas: Vec<f32> = d.color.chunks(4).map(|p| p[1] as f32 / 255.0).collect();
            let mean = lumas.iter().sum::<f32>() / lumas.len() as f32;
            let var = lumas.iter().map(|l| (l - mean).powi(2)).sum::<f32>() / lumas.len() as f32;
            (mean, var.sqrt())
        };
        let (mean, grain) = spread(&dirt);
        assert!((0.75..0.97).contains(&mean), "{mean}");
        assert!(grain > spread(&ground).1, "{grain}");
        assert!(dirt.normal.chunks(4).all(|p| p[2] >= 128));
        let missing = road_material(Some(Path::new("/nonexistent")), None);
        assert_eq!(missing.alpha_mode, AlphaMode::Blend);
        assert!(missing.base_color_texture.is_none());
    }

    #[test]
    fn with_images_but_no_texture_files_both_materials_fall_back_to_plain() {
        // Una carpeta de assets vacía, propia.
        let root = std::env::temp_dir().join(format!("pampa-no-textures-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let mut images = Assets::<Image>::default();
        let ground = ground_material(Some(&root), Some(&mut images));
        assert!(ground.extension.litter.is_none());
        let ground = ground.base;
        let road = road_material(Some(&root), Some(&mut images));
        std::fs::remove_dir(&root).unwrap();
        for material in [&ground, &road] {
            assert!(material.base_color_texture.is_none());
            assert!(material.normal_map_texture.is_none());
            assert!(material.occlusion_texture.is_none());
            assert_eq!(material.base_color, Color::WHITE);
        }
        assert_eq!(road.alpha_mode, AlphaMode::Blend);
        assert_eq!(images.len(), 0, "no half-made textures");
    }

    #[test]
    fn litter_is_full_in_the_monte_none_in_the_open_and_feathers_between() {
        assert_eq!(litter_weight(0.0, 0.0), 0.0);
        assert_eq!(litter_weight(1.0, 0.0), 1.0);
        // Difuminada: sube suavemente con el monte, sin saltos.
        let steps: Vec<f32> = (0..=20)
            .map(|i| litter_weight(i as f32 / 20.0, 0.0))
            .collect();
        assert!(
            steps.windows(2).all(|w| w[1] >= w[0] && w[1] - w[0] < 0.2),
            "{steps:?}"
        );
        assert!((0.2..0.8).contains(&litter_weight(0.45, 0.0)));
        // Los pueblos, los caminos y el barro la despejan, diga lo que diga el
        // monte.
        assert_eq!(litter_weight(1.0, 1.0), 0.0);
        assert!((litter_weight(1.0, 0.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn the_shipped_leaves_are_a_darker_browner_tile_than_the_ground() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets")
            .join(GROUND_DIR);
        let litter = load_litter(&dir).unwrap();
        assert_eq!(litter.size, 512);
        assert_eq!(litter.color.len(), 512 * 512 * 4);
        let mean = |c: &[u8]| {
            let n = (c.len() / 4) as f32;
            let sum = c.chunks(4).fold(Vec3::ZERO, |a, p| {
                a + Vec3::new(p[0] as f32, p[1] as f32, p[2] as f32) / 255.0
            });
            sum / n
        };
        let leaves = mean(&litter.color);
        let ground = mean(&load_detail(&dir).unwrap().color);
        // Más oscura que el detalle del suelo, para que el monte se lea como
        // hojarasca, y más cálida que el gris: rojo por encima de azul.
        assert!(luma(leaves) < luma(ground) - 0.1, "{leaves:?} {ground:?}");
        assert!(leaves.x > leaves.z + 0.03, "{leaves:?}");
        assert!(litter.color.chunks(4).all(|p| p[3] == 255));
        // Los archivos faltantes son un error, no un pánico.
        assert!(load_litter(Path::new("/nonexistent")).is_err());
    }

    #[test]
    fn mips_halve_down_to_one_texel() {
        let (data, levels) = with_mips(4, vec![200; 4 * 4 * 4]);
        assert_eq!(levels, 3);
        assert_eq!(data.len(), (16 + 4 + 1) * 4);
        assert!(data.iter().all(|&b| b == 200));
    }

    #[test]
    fn face_tangents_follow_the_world_space_uvs() {
        let flat = [
            Vec3::ZERO,
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, -1.0),
        ];
        let t = face_tangent(flat, flat.map(ground_uv));
        assert!(
            (Vec3::new(t[0], t[1], t[2]) - Vec3::X).length() < 1e-5,
            "{t:?}"
        );
        assert!(t[3] == 1.0 || t[3] == -1.0);
        // En una pendiente se inclina con el suelo y mantiene largo unitario.
        let slope = [
            Vec3::ZERO,
            Vec3::new(1.0, 0.5, 0.0),
            Vec3::new(1.0, 0.5, -1.0),
        ];
        let t = face_tangent(slope, slope.map(ground_uv));
        let v = Vec3::new(t[0], t[1], t[2]);
        assert!((v.length() - 1.0).abs() < 1e-5 && v.y > 0.3, "{t:?}");
    }

    #[test]
    fn malformed_pnm_files_are_errors_not_panics() {
        for bad in [
            pnm("", &[]),
            pnm("P3\n1 1\n255\n", b"0 0 0"),
            pnm("P5\n", &[]),
            pnm("P5\n2\n", &[]),
            pnm("P5\n2 2\n255\n", &[1, 2, 3]),
            pnm("P6\n1 1\n255\n", &[1, 2]),
            pnm("P5\n0 4\n255\n", &[]),
            pnm("P5\n1 1\n0\n", &[0]),
            pnm("P5\n1 1\n65535\n", &[0, 0]),
            pnm("P5\nx 1\n255\n", &[0]),
            pnm("P5\n99999999 99999999\n255\n", &[0]),
            pnm("P5\n1 1\n255", &[]),
        ] {
            assert!(
                parse_pnm(&bad).is_err(),
                "{:?}",
                String::from_utf8_lossy(&bad)
            );
        }
    }
}
