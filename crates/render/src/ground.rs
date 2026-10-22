//! Ground textures for the terrain chunks.
//!
//! They come from ambientCG, CC0 1.0 (Ground 109 and Grass 004; see
//! assets/textures/ground/CREDITS.md). They are shipped as 512×512 binary
//! PNM files (P6 colour and normal maps, P5 roughness and ambient occlusion)
//! converted from ambientCG's 1K JPGs, so no image decoder is needed.
//!
//! Bevy's [`StandardMaterial`] takes one base-colour texture, so the two
//! sets are blended here, once, into one tiling detail set: soil showing
//! through the gaps of the grass. Its colour is kept low in saturation and
//! contrast, so the per-vertex ground colour (dryness, town earth, shade)
//! still decides the hue. The normal, roughness and ambient-occlusion maps
//! are blended the same way. UVs are world space ([`ground_uv`]), so the
//! tiles run on across chunk borders without a seam.
//!
//! Roads wear Ground 102 (compressed dirt, ambientCG, CC0 1.0) alone:
//! [`road_material`], on the road surface drawn by [`crate::road`].

use std::fmt;
use std::path::Path;

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// Where the ground textures live, under the [`crate::AssetRoot`].
pub const GROUND_DIR: &str = "textures/ground";
/// Meters per texture tile.
pub const TILE: f32 = 6.0;

/// A decoded binary PNM image: one (grey) or three (RGB) 8-bit channels.
#[derive(Debug, Clone, PartialEq)]
pub struct Pnm {
    pub width: u32,
    pub height: u32,
    pub channels: usize,
    pub data: Vec<u8>,
}

/// Why a PNM image could not be read.
#[derive(Debug, Clone, PartialEq)]
pub struct PnmError(pub String);

impl fmt::Display for PnmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PnmError {}

/// Parses a binary PNM file: `P5` (grey) or `P6` (RGB), with `#` comments
/// allowed in the header and any maxval up to 255.
pub fn parse_pnm(bytes: &[u8]) -> Result<Pnm, PnmError> {
    let channels = match bytes.get(..2) {
        Some(b"P5") => 1,
        Some(b"P6") => 3,
        _ => return Err(PnmError("not a binary PNM (P5 or P6)".into())),
    };
    let mut at = 2;
    let mut fields = [0u32; 3];
    for field in &mut fields {
        // Whitespace and comments up to the next number.
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
    // Exactly one whitespace byte ends the header.
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

/// Reads the PNM file at `path`.
pub fn load_pnm(path: &Path) -> Result<Pnm, PnmError> {
    let bytes = std::fs::read(path)
        .map_err(|e| PnmError(format!("cannot read {}: {e}", path.display())))?;
    parse_pnm(&bytes).map_err(|e| PnmError(format!("{}: {e}", path.display())))
}

/// One ambientCG texture set at a common size.
struct Maps {
    size: u32,
    color: Pnm,
    normal: Pnm,
    roughness: Pnm,
    occlusion: Pnm,
}

impl Maps {
    /// Reads set `id` (`Ground109`, `Grass004`) from `dir`.
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

/// The blended detail set: RGBA8 colour (sRGB), normal, and AO in red with
/// roughness in green (linear), all `size` square.
pub struct GroundDetail {
    pub size: u32,
    pub color: Vec<u8>,
    pub normal: Vec<u8>,
    pub surface: Vec<u8>,
}

/// How far the detail colour may stray from grey, and from its mean
/// brightness: low, so the vertex colour keeps the say.
const SATURATION: f32 = 0.3;
const CONTRAST: f32 = 0.45;
/// Mean brightness of the detail colour (sRGB).
const BRIGHTNESS: f32 = 0.9;
/// How much of the maps' relief is kept: the faceted ground stays readable.
const RELIEF: f32 = 0.6;

/// Blends soil and grass: grass where its blades are bright, soil showing
/// through the dark gaps.
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
        // Soften the occlusion as the relief is: no black pits.
        surface.extend([byte(0.6 + 0.4 * ao), byte(rough.max(0.7)), 0, 255]);
    }
    GroundDetail {
        size,
        color,
        normal,
        surface,
    }
}

/// The dirt of a road on its own: Ground 102's colour, a little greyer and
/// flatter than the photo so the vertex colour sets the hue, with its
/// relief, roughness and occlusion.
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

/// How much colour and contrast of Ground 102 the road keeps: more than the
/// grass ground, so wheel ruts and pebbles read.
const ROAD_SATURATION: f32 = 0.5;
const ROAD_CONTRAST: f32 = 0.7;

/// Reads Ground 102 (compressed dirt) from `dir`, for the roads.
pub fn load_road_detail(dir: &Path) -> Result<GroundDetail, PnmError> {
    Ok(dirt(&Maps::load(dir, "Ground102")?))
}

/// Reads and blends Ground 109 (soil) and Grass 004 from `dir`.
pub fn load_detail(dir: &Path) -> Result<GroundDetail, PnmError> {
    let soil = Maps::load(dir, "Ground109")?;
    let grass = Maps::load(dir, "Grass004")?;
    if soil.size != grass.size {
        return Err(PnmError("soil and grass maps differ in size".into()));
    }
    Ok(blend(&soil, &grass))
}

/// `pixels` (RGBA8, `size` square) with its full mip chain appended, each
/// level a 2×2 box filter of the last: tiled ground must not shimmer.
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

/// The terrain's material: white and matte, its colour from the vertex
/// paint. It carries the ground detail textures from ambientCG (Ground 109
/// and Grass 004, CC0 1.0; see assets/textures/ground/CREDITS.md) when
/// they can be read from `root`; otherwise a warning is logged and the
/// plain vertex-painted ground is drawn.
pub fn ground_material(
    root: Option<&Path>,
    images: Option<&mut Assets<Image>>,
) -> StandardMaterial {
    let mut material = StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 1.0,
        reflectance: 0.1,
        ..default()
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
            wear(&mut material, detail, images);
        }
        Err(error) => warn!("ground textures not loaded ({error}); drawing plain ground"),
    }
    material
}

/// Puts `detail` on `material`: colour, normal, and the occlusion and
/// roughness map.
fn wear(material: &mut StandardMaterial, detail: GroundDetail, images: &mut Assets<Image>) {
    let size = detail.size;
    let surface = images.add(image(size, detail.surface, TextureFormat::Rgba8Unorm));
    material.base_color_texture =
        Some(images.add(image(size, detail.color, TextureFormat::Rgba8UnormSrgb)));
    material.normal_map_texture =
        Some(images.add(image(size, detail.normal, TextureFormat::Rgba8Unorm)));
    material.metallic_roughness_texture = Some(surface.clone());
    // Metallic stays 0: the surface map's blue channel is 0 too.
    material.occlusion_texture = Some(surface);
}

/// The road surface's material: Ground 102 dirt from ambientCG (CC0 1.0;
/// see assets/textures/ground/CREDITS.md) when it can be read from `root`,
/// tinted by the vertex colour, whose alpha feathers the road's edges into
/// the grass. Blended over the ground, pulled toward the camera a little so
/// it never fights the terrain under it. Without the textures a warning is
/// logged and the plain vertex-painted road is drawn.
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

/// The ground UV of world point `p`: one tile every [`TILE`] meters, a
/// function of the position alone, so chunks agree on shared vertices.
pub fn ground_uv(p: Vec3) -> [f32; 2] {
    [p.x / TILE, p.z / TILE]
}

/// The tangent (with its bitangent sign in `w`) of a flat triangle with
/// world-space UVs from [`ground_uv`].
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

        // A smaller maxval is scaled up to 255.
        let scaled = parse_pnm(&pnm("P5\n2 1\n15\n", &[15, 0])).unwrap();
        assert_eq!(scaled.data, vec![255, 0]);
        // Bytes past the image are ignored; a pixel value 10 (`\n`) right
        // after the header is data, not whitespace.
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
        // Bright and nearly grey: the vertex colour keeps the hue.
        assert!(
            mean.min_element() > 0.8 && mean.max_element() < 0.97,
            "{mean:?}"
        );
        assert!(chroma / (n as f32) < 0.08, "{}", chroma / n as f32);
        // Normals point out of the surface; AO never goes black.
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
        // An empty assets folder of its own.
        let root = std::env::temp_dir().join(format!("pampa-no-textures-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let mut images = Assets::<Image>::default();
        let ground = ground_material(Some(&root), Some(&mut images));
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
        // On a slope it leans with the ground and stays unit length.
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
