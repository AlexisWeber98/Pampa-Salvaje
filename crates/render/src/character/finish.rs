//! How character surfaces take the light: a PBR finish per material, small
//! textures painted in code, and helpers that bake ambient occlusion into
//! vertex shades. Nothing here adds triangles.
//!
//! Materials are cached by colour and [`Finish`] in a [`Palette`], and each
//! [`Texture`] is painted once, so a crowd still shares a handful of
//! materials and a few kilobytes of textures.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Affine2;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use super::figure::Rgb;
use super::horse::Coat;

/// What a surface is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Finish {
    /// Shirts, trousers, bombachas, skirts and tunics: matte, woven.
    Cloth,
    /// Ponchos, shawls and the saddle's woven sudadera: the most matte.
    Wool,
    /// Hats and boinas.
    Felt,
    /// Tanned leather: jackets, boots, belts, the carona and bastos.
    Leather,
    /// Raw hide: the bridle, the reins and the cincha.
    Rawhide,
    /// Satin skin.
    Skin,
    /// A person's head: skin with eyes, brows and a mouth painted on.
    Face,
    Hair,
    /// Police buttons.
    Brass,
    /// The rastra's coins and buckle.
    Silver,
    /// Stirrups, bit and rings.
    Iron,
    /// A horse's coat, with a slight sheen.
    Coat,
    /// A horse's head in its coat, with eyes, nostrils and any marking.
    HorseFace(Coat),
    /// Dark, glossy hooves.
    Hoof,
    /// The pellón's sheepskin.
    Fleece,
    /// Wet eyes.
    Eye,
    /// Tree bark: the roughest surface.
    Bark,
    /// Leaf clusters: matte, a little softer than bark.
    Leaves,
    /// Stones on the pampa: rough, a touch less than bark.
    Rock,
}

impl Finish {
    /// Perceptual roughness, metallic and reflectance.
    pub fn pbr(self) -> (f32, f32, f32) {
        match self {
            Self::Cloth => (0.92, 0.0, 0.3),
            Self::Wool | Self::Fleece => (0.97, 0.0, 0.25),
            Self::Felt => (0.9, 0.0, 0.3),
            Self::Leather => (0.5, 0.0, 0.45),
            Self::Rawhide => (0.62, 0.0, 0.4),
            Self::Skin | Self::Face => (0.55, 0.0, 0.4),
            Self::Hair => (0.75, 0.0, 0.35),
            Self::Brass => (0.32, 0.92, 0.5),
            Self::Silver => (0.25, 0.95, 0.5),
            Self::Iron => (0.45, 0.85, 0.5),
            Self::Coat | Self::HorseFace(_) => (0.62, 0.0, 0.45),
            Self::Hoof => (0.3, 0.0, 0.5),
            Self::Eye => (0.15, 0.0, 0.6),
            Self::Bark => (0.96, 0.0, 0.2),
            Self::Leaves => (0.88, 0.0, 0.3),
            Self::Rock => (0.93, 0.0, 0.25),
        }
    }

    /// The texture this finish wears, and how many times it repeats across
    /// a part.
    pub fn texture(self) -> Option<(Texture, f32)> {
        match self {
            Self::Cloth => Some((Texture::Weave, 4.0)),
            Self::Wool => Some((Texture::Weave, 3.0)),
            Self::Felt | Self::Fleece => Some((Texture::Felt, 2.0)),
            Self::Leather | Self::Rawhide => Some((Texture::Grain, 3.0)),
            Self::Face => Some((Texture::Face, 1.0)),
            Self::HorseFace(coat) => Some((Texture::HorseFace(coat), 1.0)),
            _ => None,
        }
    }
}

/// A small texture painted in code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Texture {
    /// Eyes, brows and a mouth, white elsewhere (see [`face_uvs`]).
    Face,
    /// A fine cloth weave.
    Weave,
    /// Leather grain.
    Grain,
    /// Soft felt.
    Felt,
    /// A horse's head in its coat, wrapped round the head's loft: eyes,
    /// nostrils, and a star (alazán) or blaze (overo).
    HorseFace(Coat),
}

impl Texture {
    /// Every texture.
    pub fn all() -> impl Iterator<Item = Self> {
        [Self::Face, Self::Weave, Self::Grain, Self::Felt]
            .into_iter()
            .chain(Coat::ALL.map(Self::HorseFace))
    }

    /// Width, height and RGBA8 sRGB pixels.
    pub fn paint(self) -> (u32, u32, Vec<u8>) {
        const N: u32 = 64;
        let mut pixels = Vec::with_capacity((N * N * 4) as usize);
        for y in 0..N {
            for x in 0..N {
                let (u, v) = ((x as f32 + 0.5) / N as f32, (y as f32 + 0.5) / N as f32);
                let [r, g, b] = self.texel(u, v, x, y);
                pixels.extend([r, g, b, 255]);
            }
        }
        (N, N, pixels)
    }

    fn texel(self, u: f32, v: f32, x: u32, y: u32) -> Rgb {
        let grey = |k: f32| [(255.0 * k.clamp(0.0, 1.0)) as u8; 3];
        // A cheap hash per texel for grain and felt.
        let noise = (x.wrapping_mul(73_856_093) ^ y.wrapping_mul(19_349_663)) % 97;
        let noise = noise as f32 / 96.0;
        match self {
            Self::Weave => {
                // Over-under threads: alternate rows and columns darken.
                let warp = if (x / 2 + y / 4).is_multiple_of(2) {
                    0.0
                } else {
                    0.06
                };
                let weft = if (y / 2 + x / 4).is_multiple_of(2) {
                    0.0
                } else {
                    0.05
                };
                grey(1.0 - warp - weft - 0.04 * noise)
            }
            Self::Grain => {
                let crease = ((u * 23.0 + (v * 9.0).sin() * 2.0).sin() * 0.5 + 0.5).powi(8);
                grey(1.0 - 0.12 * noise - 0.1 * crease)
            }
            Self::Felt => grey(0.92 + 0.08 * noise),
            Self::Face => face_texel(u, v),
            Self::HorseFace(coat) => horse_face_texel(coat, u, v),
        }
    }
}

/// Inside the ellipse at `(cu, cv)` with radii `(ru, rv)`.
fn within(u: f32, v: f32, (cu, cv): (f32, f32), (ru, rv): (f32, f32)) -> bool {
    let (du, dv) = ((u - cu) / ru, (v - cv) / rv);
    du * du + dv * dv < 1.0
}

/// The front of a person's head (see [`face_uvs`]): `u` from the right ear
/// (0.25) through the nose (0.5) to the left (0.75), `v` from the crown (0)
/// to the chin (1). White outside the features, so it only darkens skin.
fn face_texel(u: f32, v: f32) -> Rgb {
    for side in [-1.0, 1.0] {
        let eye = (0.5 + 0.085 * side, 0.47);
        if within(u, v, eye, (0.018, 0.022)) {
            return [44, 32, 26];
        }
        if within(u, v, eye, (0.034, 0.024)) {
            return [236, 232, 222];
        }
        if within(u, v, (0.5 + 0.088 * side, 0.415), (0.04, 0.012)) {
            return [96, 70, 56];
        }
    }
    if within(u, v, (0.5, 0.6), (0.04, 0.012)) {
        return [150, 88, 80];
    }
    [255; 3]
}

/// A horse's head round its loft: `u` 0.5 is the face, 0 and 1 the jaw;
/// `v` runs from the muzzle (0) to the poll (1).
fn horse_face_texel(coat: Coat, u: f32, v: f32) -> Rgb {
    const WHITE: Rgb = [234, 230, 222];
    for side in [-1.0, 1.0] {
        if within(u, v, (0.5 + 0.177 * side, 0.776), (0.03, 0.045)) {
            return if within(u, v, (0.5 + 0.17 * side, 0.79), (0.008, 0.012)) {
                [200, 196, 190]
            } else {
                [26, 18, 14]
            };
        }
        if within(u, v, (0.5 + 0.108 * side, 0.14), (0.022, 0.05)) {
            return [26, 20, 18];
        }
    }
    let marked = match coat {
        // Estrella: a star on the forehead.
        Coat::Alazan => within(u, v, (0.5, 0.82), (0.035, 0.05)),
        // Lucero corrido: a blaze down the face.
        Coat::Overo => (u - 0.5).abs() < 0.03 + 0.02 * v && (0.12..0.88).contains(&v),
        _ => false,
    };
    if marked {
        return WHITE;
    }
    // The muzzle darkens toward the lips.
    let muzzle = if v < 0.18 {
        0.72 + 0.28 * v / 0.18
    } else {
        1.0
    };
    coat.head().map(|c| (c as f32 * muzzle) as u8)
}

/// Every material and texture characters are drawn with.
#[derive(Resource, Default)]
pub struct Palette {
    materials: HashMap<(Rgb, Finish), Handle<StandardMaterial>>,
    textures: HashMap<Texture, Handle<Image>>,
}

impl Palette {
    /// The material for `color` with `finish`, made once.
    pub fn material(
        &mut self,
        color: Rgb,
        finish: Finish,
        materials: &mut Assets<StandardMaterial>,
        images: &mut Assets<Image>,
    ) -> Handle<StandardMaterial> {
        if let Some(handle) = self.materials.get(&(color, finish)) {
            return handle.clone();
        }
        let (perceptual_roughness, metallic, reflectance) = finish.pbr();
        let (texture, repeat) = match finish.texture() {
            Some((texture, repeat)) => (Some(self.texture(texture, images)), repeat),
            None => (None, 1.0),
        };
        let handle = materials.add(StandardMaterial {
            base_color: Color::srgb_u8(color[0], color[1], color[2]),
            base_color_texture: texture,
            uv_transform: Affine2::from_scale(Vec2::splat(repeat)),
            perceptual_roughness,
            metallic,
            reflectance,
            ..default()
        });
        self.materials.insert((color, finish), handle.clone());
        handle
    }

    fn texture(&mut self, texture: Texture, images: &mut Assets<Image>) -> Handle<Image> {
        self.textures
            .entry(texture)
            .or_insert_with(|| {
                let (width, height, pixels) = texture.paint();
                let mut image = Image::new(
                    Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    TextureDimension::D2,
                    pixels,
                    TextureFormat::Rgba8UnormSrgb,
                    RenderAssetUsages::RENDER_WORLD,
                );
                let wrap = match texture {
                    Texture::Face | Texture::HorseFace(_) => ImageAddressMode::ClampToEdge,
                    _ => ImageAddressMode::Repeat,
                };
                image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                    address_mode_u: wrap,
                    address_mode_v: wrap,
                    ..ImageSamplerDescriptor::linear()
                });
                images.add(image)
            })
            .clone()
    }
}

/// Bytes of every texture characters can use, painted once each.
pub fn texture_bytes() -> usize {
    Texture::all().map(|t| t.paint().2.len()).sum()
}

/// `mesh` with each vertex shade multiplied by `occlusion(position,
/// normal)`: baked ambient occlusion, no extra triangles.
pub(super) fn occlude(mut mesh: Mesh, occlusion: impl Fn(Vec3, Vec3) -> f32) -> Mesh {
    use bevy::mesh::VertexAttributeValues as V;
    let (Some(V::Float32x3(points)), Some(V::Float32x3(normals))) = (
        mesh.attribute(Mesh::ATTRIBUTE_POSITION),
        mesh.attribute(Mesh::ATTRIBUTE_NORMAL),
    ) else {
        return mesh;
    };
    let shades: Vec<f32> = points
        .iter()
        .zip(normals)
        .map(|(p, n)| occlusion(Vec3::from(*p), Vec3::from(*n)).clamp(0.0, 1.0))
        .collect();
    let mut colors = match mesh.attribute(Mesh::ATTRIBUTE_COLOR) {
        Some(V::Float32x4(c)) => c.clone(),
        _ => vec![[1.0; 4]; shades.len()],
    };
    for (color, shade) in colors.iter_mut().zip(shades) {
        for channel in &mut color[..3] {
            *channel *= shade;
        }
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh
}

/// Darker toward `edge`'s far end: `0` below `from`, `1` past `to`.
pub(super) fn ramp(x: f32, from: f32, to: f32) -> f32 {
    ((x - from) / (to - from)).clamp(0.0, 1.0)
}

/// UVs that lay [`Texture::Face`] on the front of a head sphere (front +X):
/// around the head from the right ear (`u` 0.25) to the left (0.75), down
/// from the crown (`v` 0) to the chin (1). The back of the head maps onto
/// the texture's blank top row, so the face shows only in front.
pub(super) fn face_uvs(mut mesh: Mesh) -> Mesh {
    use bevy::mesh::VertexAttributeValues as V;
    let Some(V::Float32x3(points)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
        return mesh;
    };
    let uvs: Vec<[f32; 2]> = points
        .iter()
        .map(|&[x, y, z]| {
            let around = z.atan2(x.max(0.0)).clamp(-1.6, 1.6);
            let u = 0.5 + around / std::f32::consts::TAU;
            if x < 0.0 {
                [u, 0.0]
            } else {
                [
                    u,
                    0.5 - (2.0 * y).clamp(-1.0, 1.0).asin() / std::f32::consts::PI,
                ]
            }
        })
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materials_differ_by_kind() {
        let (leather, ..) = Finish::Leather.pbr();
        let (wool, ..) = Finish::Wool.pbr();
        let (cloth, ..) = Finish::Cloth.pbr();
        assert!(leather < wool && leather < cloth, "leather is glossier");
        for metal in [Finish::Brass, Finish::Silver, Finish::Iron] {
            assert!(metal.pbr().1 > 0.5, "{metal:?} is metallic");
        }
        for matte in [
            Finish::Cloth,
            Finish::Wool,
            Finish::Felt,
            Finish::Skin,
            Finish::Coat,
        ] {
            assert_eq!(matte.pbr().1, 0.0, "{matte:?}");
        }
        let (skin, ..) = Finish::Skin.pbr();
        let (coat, ..) = Finish::Coat.pbr();
        let (hoof, ..) = Finish::Hoof.pbr();
        assert!(
            skin < cloth && coat < wool && hoof < coat,
            "satin skin, sheen, glossy hooves"
        );

        // The palette makes one material per colour and finish.
        let mut palette = Palette::default();
        let mut materials = Assets::<StandardMaterial>::default();
        let mut images = Assets::<Image>::default();
        let mut get = |c, f| palette.material(c, f, &mut materials, &mut images);
        let brown = [100, 70, 40];
        let a = get(brown, Finish::Leather);
        assert_eq!(a, get(brown, Finish::Leather));
        assert_ne!(a, get(brown, Finish::Wool));
        assert_eq!(materials.len(), 2);
        let leather = materials.get(&a).unwrap();
        assert!(leather.base_color_texture.is_some(), "leather grain");
        // Two textured finishes painted two textures; leather again none.
        assert_eq!(images.len(), 2);
    }

    #[test]
    fn faces_have_eyes_brows_and_a_mouth_only_in_front() {
        let (w, h, pixels) = Texture::Face.paint();
        let at = |u: f32, v: f32| {
            let (x, y) = ((u * w as f32) as usize, (v * h as f32) as usize);
            let i = (y * w as usize + x) * 4;
            [pixels[i], pixels[i + 1], pixels[i + 2]]
        };
        let dark = |c: [u8; 3]| c.iter().map(|&c| c as u32).sum::<u32>() < 300;
        for side in [-1.0, 1.0] {
            assert!(dark(at(0.5 + 0.085 * side, 0.47)), "an eye");
            assert!(dark(at(0.5 + 0.088 * side, 0.415)), "a brow");
        }
        assert!(at(0.5, 0.6)[0] < 200, "a mouth");
        // Blank where the back of the head maps, and between the eyes.
        assert_eq!(at(0.5, 0.01), [255; 3]);
        assert_eq!(at(0.1, 0.5), [255; 3]);
        assert_eq!(at(0.5, 0.47), [255; 3]);
        // The head's back vertices land on the blank top row.
        let head = face_uvs(Sphere::new(0.5).mesh().uv(18, 12));
        let Some(bevy::mesh::VertexAttributeValues::Float32x2(uvs)) =
            head.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("uvs");
        };
        let points = head
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap();
        for (p, uv) in points.iter().zip(uvs) {
            if p[0] < 0.0 {
                assert_eq!(uv[1], 0.0);
            } else {
                assert!((0.24..=0.76).contains(&uv[0]), "{uv:?}");
            }
        }
    }

    #[test]
    fn horse_faces_have_eyes_nostrils_and_fitting_markings() {
        let white = |coat: Coat| {
            let (_, _, p) = Texture::HorseFace(coat).paint();
            p.chunks(4).filter(|c| c[0] > 225 && c[1] > 225).count()
        };
        assert!(white(Coat::Alazan) > 10, "estrella");
        assert!(white(Coat::Overo) > white(Coat::Alazan), "a blaze");
        assert_eq!(white(Coat::Zaino), 0, "the zaino has no white");
        assert_eq!(white(Coat::Bayo), 0);
        for coat in Coat::ALL {
            let (_, _, p) = Texture::HorseFace(coat).paint();
            let eyes = p.chunks(4).filter(|c| c[0] < 30 && c[1] < 25).count();
            assert!(eyes > 20, "{coat:?}: eyes and nostrils");
        }
    }

    #[test]
    fn textures_stay_small() {
        let bytes = texture_bytes();
        println!("character textures: {bytes} bytes");
        assert!(bytes <= 160 * 1024, "{bytes}");
        for t in Texture::all() {
            let (w, h, p) = t.paint();
            assert!((64..=128).contains(&w) && (64..=128).contains(&h));
            assert_eq!(p.len(), (w * h * 4) as usize);
            assert_eq!(t.paint().2, p, "{t:?} is deterministic");
        }
    }

    #[test]
    fn occlusion_darkens_only_where_asked() {
        let mesh = occlude(Sphere::new(0.5).mesh().uv(8, 6), |p, _| {
            if p.y < 0.0 { 0.5 } else { 1.0 }
        });
        let points = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap();
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(colors)) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("colours");
        };
        for (p, c) in points.iter().zip(colors) {
            let want = if p[1] < 0.0 { 0.5 } else { 1.0 };
            assert_eq!(c[0], want);
        }
        assert_eq!(ramp(0.5, 0.0, 1.0), 0.5);
        assert_eq!(ramp(-1.0, 0.0, 1.0), 0.0);
    }
}
