//! Cómo reciben la luz las superficies de los personajes: un acabado PBR por
//! material, pequeñas texturas pintadas en código y funciones auxiliares que
//! hornean la oclusión ambiental en los tonos de los vertices. Nada de esto
//! agrega triángulos.
//!
//! Los materiales se guardan en caché por color y [`Finish`] en una
//! [`Palette`], y cada [`Texture`] se pinta una sola vez, así que una multitud
//! sigue compartiendo un puñado de materiales y unos pocos kilobytes de
//! texturas.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Affine2;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use super::figure::Rgb;

/// De qué está hecha una superficie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Finish {
    /// Camisas, pantalones, bombachas, polleras y túnicas: mate, tejido.
    Cloth,
    /// Ponchos, chales y la sudadera tejida de la montura: lo más mate.
    Wool,
    /// Sombreros y boinas.
    Felt,
    /// Cuero curtido: chaquetas, botas, cinturones, la carona y los bastos.
    Leather,
    /// Cuero crudo: la brida, las riendas y la cincha.
    Rawhide,
    /// Piel satinada.
    Skin,
    /// La cabeza de una persona: piel con ojos, cejas y boca pintados.
    Face,
    Hair,
    /// Botones de policía.
    Brass,
    /// Las monedas y la hebilla de la rastra.
    Silver,
    /// Estribos, freno y argollas.
    Iron,
    /// El cuero de oveja del pellón.
    Fleece,
    /// Corteza de árbol: la superficie más rugosa.
    Bark,
    /// Racimos de hojas: mate, un poco más suave que la corteza.
    Leaves,
    /// Piedras en la pampa: rugosas, un poco menos que la corteza.
    Rock,
}

impl Finish {
    /// Rugosidad perceptual, metalicidad y reflectancia.
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
            Self::Bark => (0.96, 0.0, 0.2),
            Self::Leaves => (0.88, 0.0, 0.3),
            Self::Rock => (0.93, 0.0, 0.25),
        }
    }

    /// La textura que lleva este acabado y cuántas veces se repite a lo largo
    /// de una pieza.
    pub fn texture(self) -> Option<(Texture, f32)> {
        match self {
            Self::Cloth => Some((Texture::Weave, 4.0)),
            Self::Wool => Some((Texture::Weave, 3.0)),
            Self::Felt | Self::Fleece => Some((Texture::Felt, 2.0)),
            Self::Leather | Self::Rawhide => Some((Texture::Grain, 3.0)),
            Self::Face => Some((Texture::Face, 1.0)),
            _ => None,
        }
    }
}

/// Una textura pequeña pintada en código.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Texture {
    /// Ojos, cejas y boca, blanco en el resto (ver [`face_uvs`]).
    Face,
    /// Un tejido de tela fino.
    Weave,
    /// Grano de cuero.
    Grain,
    /// Fieltro suave.
    Felt,
}

impl Texture {
    /// Todas las texturas.
    pub fn all() -> impl Iterator<Item = Self> {
        [Self::Face, Self::Weave, Self::Grain, Self::Felt].into_iter()
    }

    /// Ancho, alto y píxeles RGBA8 sRGB.
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
        // Un hash barato por texel para el grano y el fieltro.
        let noise = (x.wrapping_mul(73_856_093) ^ y.wrapping_mul(19_349_663)) % 97;
        let noise = noise as f32 / 96.0;
        match self {
            Self::Weave => {
                // Hilos que pasan por encima y por debajo: se oscurecen filas y
                // columnas alternadas.
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
        }
    }
}

/// Dentro de la elipse en `(cu, cv)` con radios `(ru, rv)`.
fn within(u: f32, v: f32, (cu, cv): (f32, f32), (ru, rv): (f32, f32)) -> bool {
    let (du, dv) = ((u - cu) / ru, (v - cv) / rv);
    du * du + dv * dv < 1.0
}

/// El frente de la cabeza de una persona (ver [`face_uvs`]): `u` va desde la
/// oreja derecha (0.25) pasando por la nariz (0.5) hasta la izquierda (0.75),
/// `v` desde la coronilla (0) hasta el mentón (1). Blanco fuera de los rasgos,
/// así que solo oscurece la piel.
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

/// Todos los materiales y texturas con los que se dibujan los personajes.
#[derive(Resource, Default)]
pub struct Palette {
    materials: HashMap<(Rgb, Finish), Handle<StandardMaterial>>,
    textures: HashMap<Texture, Handle<Image>>,
}

impl Palette {
    /// El material para `color` con `finish`, creado una sola vez.
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
                    Texture::Face => ImageAddressMode::ClampToEdge,
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

/// Los bytes de todas las texturas que pueden usar los personajes, pintadas una
/// vez cada una.
pub fn texture_bytes() -> usize {
    Texture::all().map(|t| t.paint().2.len()).sum()
}

/// `mesh` con el tono de cada vertex multiplicado por `occlusion(position,
/// normal)`: oclusión ambiental horneada, sin triángulos extra.
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

/// Más oscuro hacia el extremo lejano de `edge`: `0` por debajo de `from`, `1`
/// más allá de `to`.
pub(super) fn ramp(x: f32, from: f32, to: f32) -> f32 {
    ((x - from) / (to - from)).clamp(0.0, 1.0)
}

/// UVs que colocan [`Texture::Face`] en el frente de una esfera de cabeza
/// (frente +X): alrededor de la cabeza desde la oreja derecha (`u` 0.25) hasta
/// la izquierda (0.75), hacia abajo desde la coronilla (`v` 0) hasta el mentón
/// (1). La nuca se mapea a la fila superior vacía de la textura, así que la
/// cara solo se ve por delante.
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
        for matte in [Finish::Cloth, Finish::Wool, Finish::Felt, Finish::Skin] {
            assert_eq!(matte.pbr().1, 0.0, "{matte:?}");
        }
        let (skin, ..) = Finish::Skin.pbr();
        assert!(skin < cloth, "satin skin");

        // La paleta crea un material por color y acabado.
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
        // Dos acabados texturizados pintaron dos texturas; el cuero otra vez,
        // ninguna.
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
        // Vacío donde se mapea la nuca, y entre los ojos.
        assert_eq!(at(0.5, 0.01), [255; 3]);
        assert_eq!(at(0.1, 0.5), [255; 3]);
        assert_eq!(at(0.5, 0.47), [255; 3]);
        // Los vertices de la nuca caen en la fila superior vacía.
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
