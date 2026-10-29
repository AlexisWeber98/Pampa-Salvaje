//! El recado y la brida de cuero crudo de 1919 sobre el caballo modelo (ver
//! [`horse`](super::horse)).
//!
//! Cada capa se ajusta al caballo sobre el que se pone: se envuelve alrededor
//! de la propia superficie del caballo en reposo, y después recibe skinning a
//! los huesos del caballo con los pesos de la piel que tiene debajo, así que se
//! dobla con el costillar en todos los clips y nunca se mete en él.
//!
//! - **Carcasas** ([`Shell`]): la sudadera, la carona, los dos bastos, el
//!   pellón de cuero de oveja y la cincha alrededor del costillar, y la
//!   testera, la muserola y las quijeras de la brida alrededor de la cabeza.
//!   Cada una es una cuadrícula de rayos lanzados desde un eje dentro del
//!   cuerpo ([`Axis`]); donde un rayo sale por primera vez del cuerpo, la cara
//!   interior de la capa queda un poco por fuera ([`Offset`]), y su cara
//!   exterior, un grosor más allá. Los vertices toman los pesos mezclados del
//!   triángulo que tocó cada rayo ([`RestHorse::cast`]).
//! - **Argollas**: el freno, una pequeña argolla de hierro en cada comisura de
//!   la boca, con skinning como las carcasas.
//! - **Correas** ([`Strap`]): las riendas (desde el freno hacia atrás, por
//!   detrás de la quijada y a lo largo del cuello hasta las manos del jinete, o
//!   hasta la cabezada en un caballo sin jinete) y las aciones (desde la
//!   montura hasta los estribos) son piezas rectas entre dos puntos sobre
//!   huesos distintos, estiradas en cada frame después de que se calculan los
//!   transforms ([`stretch_straps`]).
//! - **Estribos**: en un caballo sin jinete cuelgan de la montura; los pies de
//!   un jinete sentado los llevan (ver [`rider`](super::rider)).
//!
//! Las carcasas y las argollas se construyen una sola vez, a partir del primer
//! caballo preparado, y las comparten todos los caballos ([`TackKit`]); cada
//! caballo solo crea entidades.

use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::SystemParam;
use bevy::gltf::GltfMaterialName;
use bevy::math::Affine3A;
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;
use std::f32::consts::{FRAC_PI_2, PI};

use super::figure::Rgb;
use super::finish::{Finish, Palette};
use super::horse::{HorseModel, rest_affine, transform_of};
use super::model::model_range;

/// Una pieza del recado o de la brida.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HorsePiece {
    /// La manta tejida bajo la montura.
    Sudadera,
    /// La capa de cuero que va encima.
    Carona,
    /// Los dos rollos acolchados entre los que se sienta el jinete.
    Basto,
    /// El cuero de oveja de arriba, donde se sienta el jinete.
    Pellon,
    /// La cincha de cuero crudo alrededor de la panza.
    Cincha,
    /// La cabezada de cuero crudo: testera, muserola y quijeras.
    Headstall,
    Bit,
    Rein,
    StirrupLeather,
    Stirrup,
}

/// Una jerga tejida, de un rojo óxido por el uso.
const JERGA: Rgb = [124, 56, 42];
const CARONA: Rgb = [88, 60, 40];
const BASTO: Rgb = [64, 44, 30];
const PELLON: Rgb = [208, 196, 170];
const RAWHIDE: Rgb = [166, 140, 104];
const IRON: Rgb = [70, 68, 66];

impl HorsePiece {
    /// Su color y de qué está hecha.
    pub fn look(self) -> (Rgb, Finish) {
        match self {
            Self::Sudadera => (JERGA, Finish::Wool),
            Self::Carona => (CARONA, Finish::Leather),
            Self::Basto | Self::StirrupLeather => (BASTO, Finish::Leather),
            Self::Pellon => (PELLON, Finish::Fleece),
            Self::Cincha | Self::Headstall | Self::Rein => (RAWHIDE, Finish::Rawhide),
            Self::Bit | Self::Stirrup => (IRON, Finish::Iron),
        }
    }
}

/// Las articulaciones y los pesos con los que un vertex recibe skinning
/// (cuatro, que suman uno).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Skin {
    pub joints: [u16; 4],
    pub weights: [f32; 4],
}

impl Skin {
    /// Los cuatro más pesados de `pairs` (articulación, peso), normalizados.
    fn heaviest(pairs: &[(u16, f32)]) -> Self {
        let mut merged: Vec<(u16, f32)> = Vec::with_capacity(pairs.len());
        for &(j, w) in pairs {
            match merged.iter_mut().find(|(k, _)| *k == j) {
                Some((_, sum)) => *sum += w,
                None => merged.push((j, w)),
            }
        }
        merged.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        let mut skin = Self::default();
        let total: f32 = merged.iter().take(4).map(|(_, w)| w.max(0.0)).sum();
        for (k, (j, w)) in merged.into_iter().take(4).enumerate() {
            skin.joints[k] = j;
            skin.weights[k] = if total > 0.0 { w.max(0.0) / total } else { 0.0 };
        }
        if total <= 0.0 {
            skin.weights[0] = 1.0;
        }
        skin
    }
}

/// Dónde salió por primera vez un rayo del caballo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub point: Vec3,
    pub distance: f32,
    pub skin: Skin,
}

/// La piel del caballo en reposo en el marco de su cuerpo (en metros, +X su
/// frente, +Y hacia arriba, +Z su derecha, los vasos en 0): aquello a lo que se
/// ajustan los aperos.
#[derive(Debug, Clone, Default)]
pub struct RestHorse {
    positions: Vec<Vec3>,
    triangles: Vec<[u32; 3]>,
    skins: Vec<Skin>,
}

impl RestHorse {
    /// Una superficie a partir de sus vertices (con sus skins) y triángulos.
    pub fn new(positions: Vec<Vec3>, skins: Vec<Skin>, triangles: Vec<[u32; 3]>) -> Self {
        Self {
            positions,
            triangles,
            skins,
        }
    }

    /// Agrega una primitiva con skinning del modelo, en la pose de reposo dada
    /// por `skin` (la matriz en el marco del cuerpo de cada articulación por su
    /// pose de bind inversa). `false` si le faltan posiciones, articulaciones,
    /// pesos o índices.
    pub fn add(&mut self, mesh: &Mesh, skin: &[Affine3A]) -> bool {
        let (
            Some(VertexAttributeValues::Float32x3(positions)),
            Some(VertexAttributeValues::Uint16x4(joints)),
            Some(VertexAttributeValues::Float32x4(weights)),
            Some(indices),
        ) = (
            mesh.attribute(Mesh::ATTRIBUTE_POSITION),
            mesh.attribute(Mesh::ATTRIBUTE_JOINT_INDEX),
            mesh.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT),
            mesh.indices(),
        )
        else {
            return false;
        };
        let base = self.positions.len() as u32;
        for ((p, j), w) in positions.iter().zip(joints).zip(weights) {
            let mut at = Vec3::ZERO;
            for k in 0..4 {
                if let Some(m) = skin.get(j[k] as usize) {
                    at += w[k] * m.transform_point3(Vec3::from(*p));
                }
            }
            self.positions.push(at);
            let pairs = [0, 1, 2, 3].map(|k| (j[k], w[k]));
            self.skins.push(Skin::heaviest(&pairs));
        }
        let indices: Vec<u32> = indices.iter().map(|i| i as u32).collect();
        for t in indices.as_chunks::<3>().0 {
            self.triangles.push(t.map(|i| i + base));
        }
        true
    }

    /// Dónde sale por primera vez del cuerpo un rayo desde `origin` en la
    /// dirección `dir` (unitaria), con los pesos del triángulo de ese punto,
    /// mezclados según dónde tocó. Los rayos empiezan dentro del cuerpo, así
    /// que la primera superficie que encuentran es su piel.
    pub fn cast(&self, origin: Vec3, dir: Vec3) -> Option<Hit> {
        let mut best: Option<(f32, usize, f32, f32)> = None;
        for (k, t) in self.triangles.iter().enumerate() {
            let [a, b, c] = t.map(|i| self.positions[i as usize]);
            // Möller–Trumbore.
            let (e1, e2) = (b - a, c - a);
            let p = dir.cross(e2);
            let det = e1.dot(p);
            if det.abs() < 1e-9 {
                continue;
            }
            let inv = 1.0 / det;
            let s = origin - a;
            let u = s.dot(p) * inv;
            if !(0.0..=1.0).contains(&u) {
                continue;
            }
            let q = s.cross(e1);
            let v = dir.dot(q) * inv;
            if v < 0.0 || u + v > 1.0 {
                continue;
            }
            let d = e2.dot(q) * inv;
            if d > 1e-5 && best.is_none_or(|(b, ..)| d < b) {
                best = Some((d, k, u, v));
            }
        }
        let (distance, k, u, v) = best?;
        let corners = self.triangles[k].map(|i| self.skins[i as usize]);
        let mut pairs = [(0u16, 0.0f32); 12];
        for (c, (skin, bary)) in corners.iter().zip([1.0 - u - v, u, v]).enumerate() {
            for i in 0..4 {
                pairs[c * 4 + i] = (skin.joints[i], skin.weights[i] * bary);
            }
        }
        Some(Hit {
            point: origin + dir * distance,
            distance,
            skin: Skin::heaviest(&pairs),
        })
    }
}

/// Una línea dentro del cuerpo alrededor de la cual se envuelve una pieza: los
/// rayos salen de ella en ángulo recto, con un ángulo `theta` desde `up` hacia
/// `up × along`... es decir, hacia la derecha del caballo para los ángulos
/// positivos.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Axis {
    pub origin: Vec3,
    /// Unitario, a lo largo del eje.
    pub along: Vec3,
    /// Unitario, en ángulo recto con `along`: hacia donde apunta el ángulo 0.
    pub up: Vec3,
}

impl Axis {
    /// El del costillar: a lo largo del lomo, a 1.14 m de altura (con su centro
    /// en la cinchera).
    pub const BARREL: Self = Self {
        origin: Vec3::new(0.0, 1.14, 0.0),
        along: Vec3::X,
        up: Vec3::Y,
    };

    /// El de la cabeza: desde detrás de la nuca, a lo largo de ella, hasta el
    /// hocico; el ángulo 0 es la frente.
    pub fn head() -> Self {
        let along = Vec3::new(0.755, -0.656, 0.0).normalize();
        Self {
            origin: Vec3::new(1.1, 1.78, 0.01),
            along,
            up: Vec3::new(-along.y, along.x, 0.0),
        }
    }

    /// El rayo a `t` metros a lo largo del eje, saliendo de él con el ángulo
    /// `theta`.
    pub fn ray(&self, t: f32, theta: f32) -> (Vec3, Vec3) {
        let side = self.along.cross(self.up);
        (
            self.origin + self.along * t,
            (self.up * theta.cos() + side * theta.sin()).normalize(),
        )
    }
}

/// Cuánto se separa del cuerpo la cara interior de una capa, en metros: `top`
/// dentro de `flat` radianes del ángulo 0, pasando suavemente a `edge` hasta
/// `flat + fade`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Offset {
    pub top: f32,
    pub edge: f32,
    pub flat: f32,
    pub fade: f32,
}

impl Offset {
    pub const fn even(at: f32) -> Self {
        Self {
            top: at,
            edge: at,
            flat: 0.0,
            fade: 1.0,
        }
    }

    pub fn at(&self, theta: f32) -> f32 {
        let s = ((theta.abs() - self.flat) / self.fade.max(1e-6)).clamp(0.0, 1.0);
        let s = s * s * (3.0 - 2.0 * s);
        self.top + (self.edge - self.top) * s
    }
}

/// Una capa envuelta alrededor del cuerpo: `along` a lo largo del eje (en
/// metros) y `around` alrededor de él (en radianes), separada por `offset`, de
/// `thickness` de espesor, con la cara exterior rugosa según `shag` (una
/// fracción del espesor) como un cuero de oveja.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shell {
    pub piece: HorsePiece,
    pub axis: Axis,
    pub along: (f32, f32),
    pub around: (f32, f32),
    pub offset: Offset,
    pub thickness: f32,
    pub shag: f32,
    /// Celdas de la cuadrícula a lo largo y alrededor.
    pub cells: (usize, usize),
}

/// Una mesh con skinning en el marco del cuerpo: posiciones, skins y
/// triángulos, antes de convertirse en una [`Mesh`] ([`skinned_mesh`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TackMesh {
    pub positions: Vec<Vec3>,
    pub skins: Vec<Skin>,
    pub uvs: Vec<[f32; 2]>,
    pub triangles: Vec<[u32; 3]>,
}

impl Shell {
    /// La capa ajustada a `horse`, y cuántos de sus rayos no lo tocaron (esos
    /// vertices se separan del eje a la distancia del impacto más cercano).
    pub fn fit(&self, horse: &RestHorse) -> (TackMesh, usize) {
        let (along, around) = self.cells;
        let row = around + 1;
        let mut inner = Vec::with_capacity((along + 1) * row);
        let mut misses = 0;
        let mut last: Option<Hit> = None;
        for i in 0..=along {
            let t = self.along.0 + (self.along.1 - self.along.0) * i as f32 / along as f32;
            for k in 0..=around {
                let theta =
                    self.around.0 + (self.around.1 - self.around.0) * k as f32 / around as f32;
                let (origin, dir) = self.axis.ray(t, theta);
                let hit = match horse.cast(origin, dir) {
                    Some(hit) => hit,
                    None => {
                        misses += 1;
                        let reach = last.map_or(0.2, |h| h.distance);
                        Hit {
                            point: origin + dir * reach,
                            distance: reach,
                            skin: last.map_or_else(Skin::default, |h| h.skin),
                        }
                    }
                };
                last = Some(hit);
                inner.push((hit, dir, theta));
            }
        }
        let mut mesh = TackMesh::default();
        for (face, depth) in [(0, 1.0), (1, 0.0)] {
            for (n, (hit, dir, theta)) in inner.iter().enumerate() {
                let (i, k) = (n / row, n % row);
                // Mechones solo en la cara exterior, siempre iguales.
                let tuft = if face == 0 {
                    ((i * 7 + k * 13) % 5) as f32 / 4.0
                } else {
                    0.0
                };
                let out =
                    self.offset.at(*theta) + depth * self.thickness * (1.0 - self.shag * tuft);
                mesh.positions.push(hit.point + *dir * out);
                mesh.skins.push(hit.skin);
                mesh.uvs
                    .push([i as f32 / along as f32, k as f32 / around as f32]);
            }
        }
        let at = |face: usize, i: usize, k: usize| (face * (along + 1) * row + i * row + k) as u32;
        let tris = &mut mesh.triangles;
        for i in 0..along {
            for k in 0..around {
                let (a, b, c, d) = (
                    at(0, i, k),
                    at(0, i, k + 1),
                    at(0, i + 1, k),
                    at(0, i + 1, k + 1),
                );
                tris.extend([[a, b, c], [b, d, c]]);
                let (a, b, c, d) = (
                    at(1, i, k),
                    at(1, i, k + 1),
                    at(1, i + 1, k),
                    at(1, i + 1, k + 1),
                );
                tris.extend([[a, c, b], [b, c, d]]);
            }
        }
        // Los dos extremos y los dos bordes laterales cierran la carcasa.
        for k in 0..around {
            let (o, o1, n, n1) = (at(0, 0, k), at(0, 0, k + 1), at(1, 0, k), at(1, 0, k + 1));
            tris.extend([[o, n, o1], [o1, n, n1]]);
            let (o, o1, n, n1) = (
                at(0, along, k),
                at(0, along, k + 1),
                at(1, along, k),
                at(1, along, k + 1),
            );
            tris.extend([[o, o1, n], [o1, n1, n]]);
        }
        for i in 0..along {
            let (o, o1, n, n1) = (
                at(0, i, around),
                at(0, i + 1, around),
                at(1, i, around),
                at(1, i + 1, around),
            );
            tris.extend([[o, n, o1], [o1, n, n1]]);
            let (o, o1, n, n1) = (at(0, i, 0), at(0, i + 1, 0), at(1, i, 0), at(1, i + 1, 0));
            tris.extend([[o, o1, n], [o1, n1, n]]);
        }
        (mesh, misses)
    }
}

/// Una pequeña argolla apoyada sobre el cuerpo en `hit`, mirando hacia afuera
/// según `dir`: de `radius` de diámetro, con su barra de `bar` de grosor. Con
/// skinning como la piel que tiene debajo.
fn ring_on(hit: &Hit, dir: Vec3, radius: f32, bar: f32, lift: f32) -> TackMesh {
    let unit = ring();
    let rotation = Quat::from_rotation_arc(Vec3::Z, dir);
    let center = hit.point + dir * lift;
    let mut mesh = TackMesh::default();
    let Some(VertexAttributeValues::Float32x3(p)) = unit.attribute(Mesh::ATTRIBUTE_POSITION) else {
        return mesh;
    };
    for v in p {
        let v = Vec3::from(*v) * Vec3::new(2.0 * radius, 2.0 * radius, bar / 0.15);
        mesh.positions.push(center + rotation * v);
        mesh.skins.push(hit.skin);
        mesh.uvs.push([0.0, 0.0]);
    }
    if let Some(indices) = unit.indices() {
        let i: Vec<u32> = indices.iter().map(|i| i as u32).collect();
        mesh.triangles.extend(i.as_chunks::<3>().0);
    }
    mesh
}

/// La argolla de hierro unitaria: de 1 m de diámetro en su plano XY (mirando
/// hacia +Z), con su barra de 0.15 m de grosor.
pub fn ring() -> Mesh {
    Torus::new(0.35, 0.5)
        .mesh()
        .minor_resolution(4)
        .major_resolution(10)
        .build()
        .rotated_by(Quat::from_rotation_x(FRAC_PI_2))
}

/// La correa unitaria: de 1 m de largo a lo largo de +Y, centrada, de 1 m de
/// ancho.
pub fn strap() -> Mesh {
    Cylinder::new(0.5, 1.0).mesh().resolution(5).build()
}

/// `mesh` como una [`Mesh`] con skinning y normales planas: sus índices de
/// articulación apuntan a las articulaciones de la piel del caballo.
pub fn skinned_mesh(mesh: &TackMesh) -> Mesh {
    let mut indices = Vec::with_capacity(mesh.triangles.len() * 3);
    for t in &mesh.triangles {
        indices.extend(t);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        mesh.positions
            .iter()
            .map(|p| p.to_array())
            .collect::<Vec<_>>(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, mesh.uvs.clone())
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_JOINT_INDEX,
        VertexAttributeValues::Uint16x4(mesh.skins.iter().map(|s| s.joints).collect()),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_JOINT_WEIGHT,
        mesh.skins.iter().map(|s| s.weights).collect::<Vec<_>>(),
    )
    .with_inserted_indices(Indices::U32(indices))
    .with_duplicated_vertices()
    .with_computed_flat_normals()
}

/// Las capas del costillar, de atrás hacia adelante y de adentro hacia afuera:
/// la sudadera, la carona, los bastos a cada lado de la columna, el pellón
/// encima de ellos y la cincha delante de ellos, alrededor de la cinchera.
pub fn saddle() -> [Shell; 6] {
    let barrel = Axis::BARREL;
    let layer = |piece, along, span: f32, offset, thickness, shag, cells| Shell {
        piece,
        axis: barrel,
        along,
        around: (-span, span),
        offset,
        thickness,
        shag,
        cells,
    };
    let basto = |side: f32| Shell {
        around: (side * 0.21, side * 0.45),
        ..layer(
            HorsePiece::Basto,
            (-0.19, 0.23),
            0.0,
            Offset::even(0.044),
            0.05,
            0.0,
            (8, 4),
        )
    };
    [
        layer(
            HorsePiece::Sudadera,
            (-0.19, 0.24),
            1.45,
            Offset {
                top: 0.016,
                edge: 0.024,
                flat: 1.0,
                fade: 0.45,
            },
            0.012,
            0.0,
            (12, 20),
        ),
        layer(
            HorsePiece::Carona,
            (-0.17, 0.22),
            1.3,
            Offset {
                top: 0.03,
                edge: 0.038,
                flat: 1.0,
                fade: 0.3,
            },
            0.012,
            0.0,
            (10, 18),
        ),
        basto(1.0),
        basto(-1.0),
        layer(
            HorsePiece::Pellon,
            (-0.16, 0.2),
            1.05,
            Offset {
                top: 0.097,
                edge: 0.047,
                flat: 0.5,
                fade: 0.5,
            },
            0.045,
            0.35,
            (9, 16),
        ),
        Shell {
            around: (-PI, PI),
            ..layer(
                HorsePiece::Cincha,
                (0.26, 0.33),
                0.0,
                Offset {
                    top: 0.018,
                    edge: 0.014,
                    flat: 0.4,
                    fade: 0.6,
                },
                0.008,
                0.0,
                (2, 36),
            )
        },
    ]
}

/// La cabezada: la testera sobre la nuca detrás de las orejas y bajando hasta
/// las mejillas (no alrededor del cuello: por debajo de ella la cabeza se junta
/// con el cuello), la muserola y una quijera bajando por cada lado hasta el
/// freno.
pub fn bridle() -> [Shell; 4] {
    let head = Axis::head();
    let band = |along, around| Shell {
        piece: HorsePiece::Headstall,
        axis: head,
        along,
        around,
        offset: Offset::even(0.006),
        thickness: 0.01,
        shag: 0.0,
        cells: (1, 28),
    };
    let cheek = |side: f32| Shell {
        cells: (6, 1),
        ..band(
            (0.05, 0.38),
            (side * (FRAC_PI_2 + 0.22), side * (FRAC_PI_2 + 0.36)),
        )
    };
    [
        band((0.03, 0.065), (-1.9, 1.9)),
        band((0.3, 0.33), (-PI, PI)),
        cheek(1.0),
        cheek(-1.0),
    ]
}

/// El rayo desde dentro del hocico hacia la comisura de la boca del lado `s`
/// (-1 izquierda, 1 derecha), donde va una argolla del freno. La cabeza está un
/// poco a la derecha de la columna (su centro, 1.6 cm desplazado).
pub fn bit_ray(s: f32) -> (Vec3, Vec3) {
    (Vec3::new(1.33, 1.48, 0.016), Vec3::Z * s)
}

/// El rayo desde dentro del cuello hacia su costado, un palmo por debajo de la
/// crin, del lado `s`: ahí las riendas se apoyan a lo largo del cuello.
pub fn neck_ray(s: f32) -> (Vec3, Vec3) {
    (
        Vec3::new(0.7, 1.45, 0.0),
        Vec3::new(0.0, 1.0, 0.55 * s).normalize(),
    )
}

/// El rayo desde dentro de la cabeza hacia la quijada del lado `s`, donde la
/// rienda vuelve desde el freno hacia las manos del jinete, sin tocar la
/// mejilla.
pub fn jowl_ray(s: f32) -> (Vec3, Vec3) {
    (
        Vec3::new(1.12, 1.62, 0.015),
        Vec3::new(0.0, -0.2, s).normalize(),
    )
}

/// Los puntos del caballo en reposo (marco del cuerpo) de los que cuelgan las
/// correas y el jinete, calculados a partir de su superficie.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fittings {
    /// Las argollas del freno, primero la izquierda y después la derecha (en la
    /// cabeza).
    pub bit: [Vec3; 2],
    /// Por dónde pasan las riendas junto a las quijadas, desde el freno hacia
    /// atrás (en la cabeza).
    pub jowl: [Vec3; 2],
    /// Dónde se apoyan a lo largo del cuello (en su hueso central).
    pub neck: [Vec3; 2],
    /// Dónde descansan las riendas en un caballo sin jinete: en la cabezada.
    pub pommel: [Vec3; 2],
    /// Dónde salen las aciones por debajo del pellón.
    pub leather: [Vec3; 2],
    /// Dónde cuelga un estribo sin ningún pie dentro (el centro de su argolla).
    pub hanging: [Vec3; 2],
    /// La parte superior del pellón, donde se sienta el jinete.
    pub seat: Vec3,
}

/// Cuánto detrás de la cinchera se sienta el jinete: el punto más bajo del
/// lomo, entre la cruz y la grupa.
pub const SEAT_X: f32 = -0.02;

impl Fittings {
    /// Los accesorios sobre `horse`, o `None` si no tiene forma de caballo.
    pub fn find(horse: &RestHorse) -> Option<Self> {
        let barrel = Axis::BARREL;
        let pellon = saddle()[4];
        let side = |f: &dyn Fn(f32) -> Option<Vec3>| Some([f(-1.0)?, f(1.0)?]);
        let out = |(origin, dir): (Vec3, Vec3), by: f32| {
            horse.cast(origin, dir).map(|h| h.point + dir * by)
        };
        let bit = side(&|s| out(bit_ray(s), 0.012))?;
        let jowl = side(&|s| out(jowl_ray(s), 0.035))?;
        let neck = side(&|s| out(neck_ray(s), 0.035))?;
        let top = |x: f32, theta: f32, extra: f32| {
            let (origin, dir) = barrel.ray(x, theta);
            horse
                .cast(origin, dir)
                .map(|h| h.point + dir * (pellon.offset.at(theta) + pellon.thickness + extra))
        };
        let pommel = side(&|s| top(0.2, s * 0.3, 0.01))?;
        let leather = side(&|s| top(0.08, s * 1.0, 0.012))?;
        let seat = top(SEAT_X, 0.0, 0.0)?;
        // Directo hacia abajo desde la ación, sin tocar el costillar de abajo.
        let hanging = side(&|s| {
            let (origin, dir) = barrel.ray(0.08, s * FRAC_PI_2);
            let flank = horse.cast(origin, dir)?;
            Some(Vec3::new(0.08, 0.98, s * (flank.point.z.abs() + 0.06)))
        })?;
        Some(Self {
            bit,
            jowl,
            neck,
            pommel,
            leather,
            hanging,
            seat,
        })
    }
}

/// Cada carcasa y argolla de los aperos ajustada a `horse`, y los rayos que no
/// lo tocaron.
pub fn fit_tack(horse: &RestHorse) -> (Vec<(HorsePiece, TackMesh)>, usize) {
    let mut out = Vec::new();
    let mut misses = 0;
    for shell in saddle().iter().chain(&bridle()) {
        let (mesh, missed) = shell.fit(horse);
        misses += missed;
        out.push((shell.piece, mesh));
    }
    for s in [-1.0, 1.0] {
        let (origin, dir) = bit_ray(s);
        match horse.cast(origin, dir) {
            Some(hit) => out.push((HorsePiece::Bit, ring_on(&hit, dir, 0.03, 0.008, 0.02))),
            None => misses += 1,
        }
    }
    (out, misses)
}

/// Los aperos construidos una vez y compartidos por todos los caballos.
#[derive(Debug, Clone)]
pub struct TackKit {
    /// Cada carcasa y argolla con skinning con su material.
    pub pieces: Vec<(HorsePiece, Handle<Mesh>, Handle<StandardMaterial>)>,
    /// Las poses de bind inversas propias de los aperos: el reposo en el marco
    /// del cuerpo de cada articulación del caballo, invertido (sus vertices
    /// están en el marco del cuerpo).
    pub bindposes: Handle<SkinnedMeshInverseBindposes>,
    pub strap: Handle<Mesh>,
    pub ring: Handle<Mesh>,
    pub fittings: Fittings,
}

/// El tamaño de un estribo: ancho, alto y la profundidad de su barra.
pub const STIRRUP: Vec3 = Vec3::new(0.13, 0.16, 0.012 / 0.15);

/// Qué tan gruesas son las riendas y las aciones, en metros.
pub const REIN: f32 = 0.014;
pub const LEATHER: f32 = 0.03;

/// Una pieza recta de los aperos entre dos puntos que se separan: cada extremo
/// es un punto en el espacio propio de una entidad. La estira
/// [`stretch_straps`].
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Strap {
    pub ends: [(Entity, Vec3); 2],
    /// Ancho, en metros.
    pub width: f32,
}

/// Los aperos de un caballo, tal como los toma un jinete: las correas van hasta
/// la mano del jinete y los estribos van en sus pies; se apoya sobre el hueso
/// del torso en el asiento. En el cuerpo del caballo.
#[derive(Component, Debug, Clone, Copy)]
pub struct Tack {
    /// El último tramo de las riendas (desde el cuello), primero la izquierda y
    /// después la derecha.
    pub reins: [Entity; 2],
    /// Los estribos, primero el izquierdo y después el derecho.
    pub stirrups: [Entity; 2],
    /// Los huesos `Torso` y `Head` del caballo.
    pub torso: Entity,
    pub head: Entity,
    /// La parte superior del pellón (marco del cuerpo), y la ubicación de
    /// reposo del torso.
    pub seat: Vec3,
    pub torso_rest: Affine3A,
    /// El centro de las argollas del freno, en la cabeza.
    pub bit: Vec3,
}

/// Los huesos de los que cuelgan los aperos y el jinete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HorseBone {
    /// El centro del lomo, bajo la montura.
    Torso,
    /// El centro del cuello.
    Neck2,
    Head,
}

impl HorseBone {
    pub fn name(self) -> &'static str {
        match self {
            Self::Torso => "Torso",
            Self::Neck2 => "Neck2",
            Self::Head => "Head",
        }
    }
}

/// Aquello a lo que se ajustan los aperos en la escena de un caballo: los
/// nombres y los reposos de sus nodos, y cada primitiva con skinning con su
/// material.
#[derive(SystemParam)]
pub(super) struct TackScene<'w, 's> {
    names: Query<'w, 's, &'static Name>,
    transforms: Query<'w, 's, &'static Transform>,
    parents: Query<'w, 's, &'static ChildOf>,
    children: Query<'w, 's, &'static Children>,
    skins: Query<
        'w,
        's,
        (
            &'static SkinnedMesh,
            &'static Mesh3d,
            Option<&'static GltfMaterialName>,
        ),
    >,
}

/// Los almacenes a partir de los que se construyen los aperos.
#[derive(SystemParam)]
pub(super) struct TackStores<'w> {
    materials: ResMut<'w, Assets<StandardMaterial>>,
    meshes: ResMut<'w, Assets<Mesh>>,
    images: ResMut<'w, Assets<Image>>,
    bindposes: Option<ResMut<'w, Assets<SkinnedMeshInverseBindposes>>>,
    palette: ResMut<'w, Palette>,
}

/// El kit de aperos, con las ubicaciones de reposo de sus huesos convertidas:
/// los accesorios en los espacios propios de sus huesos.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocalFittings {
    /// En la cabeza.
    pub bit: [Vec3; 2],
    pub jowl: [Vec3; 2],
    /// En `Neck2`.
    pub neck: [Vec3; 2],
    /// En el torso, como el resto.
    pub pommel: [Vec3; 2],
    pub leather: [Vec3; 2],
    pub hanging: [Transform; 2],
}

/// Los accesorios en los espacios de los huesos, a partir de sus ubicaciones de
/// reposo `rest` (la del torso, la del cuello y la de la cabeza).
pub fn local_fittings(f: &Fittings, rest: [Affine3A; 3]) -> LocalFittings {
    let [torso, neck, head] = rest.map(|a| a.inverse());
    let on = |a: Affine3A, p: [Vec3; 2]| p.map(|p| a.transform_point3(p));
    let hang = |p: Vec3| {
        transform_of(
            torso
                * Affine3A::from_scale_rotation_translation(
                    STIRRUP,
                    Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
                    p,
                ),
        )
    };
    LocalFittings {
        bit: on(head, f.bit),
        jowl: on(head, f.jowl),
        neck: on(neck, f.neck),
        pommel: on(torso, f.pommel),
        leather: on(torso, f.leather),
        hanging: f.hanging.map(hang),
    }
}

/// El kit tal como lo pone un caballo: las meshes compartidas, sus accesorios y
/// el reposo del torso (desde el que se sienta un jinete).
#[derive(Debug, Clone)]
pub struct HorseTack {
    pub kit: TackKit,
    pub local: LocalFittings,
    pub torso: Affine3A,
}

/// Construye el kit de aperos a partir de la escena de un caballo bajo `root`:
/// su piel en reposo en el marco del cuerpo, las carcasas ajustadas a ella y
/// las poses de bind inversas que llevan los vertices del marco del cuerpo a
/// sus articulaciones. `None` si al modelo le falta su piel o sus huesos.
fn build_tack(
    scene: &TackScene,
    stores: &mut TackStores,
    root: Entity,
    joints: &[Entity],
    bindposes: &Handle<SkinnedMeshInverseBindposes>,
) -> Option<HorseTack> {
    let rest: Vec<Affine3A> = joints
        .iter()
        .map(|j| rest_affine(*j, root, &scene.parents, &scene.transforms))
        .collect();
    let inverse_binds = stores.bindposes.as_ref()?.get(bindposes)?;
    let skin: Vec<Affine3A> = rest
        .iter()
        .zip(inverse_binds.iter())
        .map(|(r, ibm)| *r * Affine3A::from_mat4(*ibm))
        .collect();
    let mut horse = RestHorse::default();
    for entity in scene.children.iter_descendants(root) {
        let Ok((_, mesh, name)) = scene.skins.get(entity) else {
            continue;
        };
        // Los ojos sobresalen de la cara; los aperos los rodean.
        if name.is_some_and(|n| n.0.starts_with("Eye")) {
            continue;
        }
        if let Some(mesh) = stores.meshes.get(&mesh.0) {
            horse.add(mesh, &skin);
        }
    }
    let (fitted, misses) = fit_tack(&horse);
    if misses > 0 {
        warn!("the horse's tack missed its body {misses} times");
    }
    let fittings = Fittings::find(&horse)?;
    let bone = |b: HorseBone| {
        scene
            .children
            .iter_descendants(root)
            .find(|e| scene.names.get(*e).is_ok_and(|n| n.as_str() == b.name()))
            .map(|e| rest_affine(e, root, &scene.parents, &scene.transforms))
    };
    let bones = [
        bone(HorseBone::Torso)?,
        bone(HorseBone::Neck2)?,
        bone(HorseBone::Head)?,
    ];
    let inverse: Vec<Mat4> = rest.iter().map(|r| Mat4::from(r.inverse())).collect();
    let pieces = fitted
        .iter()
        .map(|(piece, mesh)| {
            let handle = stores.meshes.add(skinned_mesh(mesh));
            let paint = material(
                *piece,
                &mut stores.palette,
                &mut stores.materials,
                &mut stores.images,
            );
            (*piece, handle, paint)
        })
        .collect();
    let kit = TackKit {
        pieces,
        bindposes: stores.bindposes.as_mut()?.add(inverse),
        strap: stores.meshes.add(strap()),
        ring: stores.meshes.add(ring()),
        fittings,
    };
    Some(HorseTack {
        local: local_fittings(&kit.fittings, bones),
        torso: bones[0],
        kit,
    })
}

/// Pone los aperos en un modelo de caballo en el momento en que se crea su
/// escena (ver las notas del módulo). El kit se construye a partir del primer
/// caballo preparado. A una escena creada de nuevo (con su asset recargado) se
/// le quitan los aperos viejos una vez que se verificó la escena nueva
/// (cualquier jinete se fue con los huesos viejos); si no está completa, el
/// caballo queda sin [`Tack`], nunca con uno que apunte a entidades eliminadas.
pub(super) fn dress_tack(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    mut stores: TackStores,
    scene: TackScene,
    roots: Query<&ChildOf, With<HorseModel>>,
    (pieces, worn): (Query<(), With<HorsePiece>>, Query<&Tack>),
    mut tack: Local<Option<HorseTack>>,
) {
    let root = ready.entity;
    let Ok(child_of) = roots.get(root) else {
        return;
    };
    let carrier = child_of.parent();
    // La piel del cuerpo, sus articulaciones y sus poses de bind, y los huesos
    // de los que cuelgan las correas: todo verificado antes de quitar nada.
    let skin = scene.children.iter_descendants(root).find_map(|e| {
        let (skin, _, name) = scene.skins.get(e).ok()?;
        name.is_some_and(|n| n.0 == "Main").then(|| skin.clone())
    });
    let bone = |b: HorseBone| {
        scene
            .children
            .iter_descendants(root)
            .find(|e| scene.names.get(*e).is_ok_and(|n| n.as_str() == b.name()))
    };
    let bones = (
        bone(HorseBone::Torso),
        bone(HorseBone::Neck2),
        bone(HorseBone::Head),
    );
    for &child in scene.children.get(carrier).into_iter().flatten() {
        if pieces.contains(child) {
            commands.entity(child).try_despawn();
        }
    }
    // Los estribos cuelgan del torso viejo, si todavía está.
    for stirrup in worn.get(carrier).map(|t| t.stirrups).into_iter().flatten() {
        commands.entity(stirrup).try_despawn();
    }
    let (Some(skin), (Some(torso), Some(neck), Some(head))) = (skin, bones) else {
        warn!("horse model without its body's skin or its torso, neck and head: no tack");
        commands.entity(carrier).try_remove::<Tack>();
        return;
    };
    if tack.is_none() {
        *tack = build_tack(
            &scene,
            &mut stores,
            root,
            &skin.joints,
            &skin.inverse_bindposes,
        );
    }
    let Some(tack) = tack.as_ref() else {
        warn!("the horse's tack could not be fitted");
        commands.entity(carrier).try_remove::<Tack>();
        return;
    };
    put_on_tack(
        &mut commands,
        &mut stores,
        tack,
        carrier,
        &skin,
        [torso, neck, head],
    );
}

/// Pone los aperos compartidos en el caballo `carrier`: las carcasas con
/// skinning en las articulaciones de su `skin`, los estribos colgando del torso
/// y las correas (en el orden torso, cuello, cabeza: `bones`).
fn put_on_tack(
    commands: &mut Commands,
    stores: &mut TackStores,
    tack: &HorseTack,
    carrier: Entity,
    skin: &SkinnedMesh,
    [torso, neck, head]: [Entity; 3],
) {
    let range = model_range();
    let kit = &tack.kit;
    let local = &tack.local;
    for (piece, mesh, paint) in &kit.pieces {
        commands.spawn((
            Name::new(format!("{piece:?}")),
            *piece,
            Mesh3d(mesh.clone()),
            MeshMaterial3d(paint.clone()),
            SkinnedMesh {
                inverse_bindposes: kit.bindposes.clone(),
                joints: skin.joints.clone(),
            },
            Transform::default(),
            range.clone(),
            ChildOf(carrier),
        ));
    }
    let mut paint = |piece| {
        material(
            piece,
            &mut stores.palette,
            &mut stores.materials,
            &mut stores.images,
        )
    };
    let (iron, leather, rein) = (
        paint(HorsePiece::Stirrup),
        paint(HorsePiece::StirrupLeather),
        paint(HorsePiece::Rein),
    );
    let spawn_strap =
        |commands: &mut Commands, piece, ends, width, material: &Handle<StandardMaterial>| {
            commands
                .spawn((
                    Name::new(format!("{piece:?}")),
                    piece,
                    Strap { ends, width },
                    Mesh3d(kit.strap.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::default(),
                    Visibility::default(),
                    range.clone(),
                    ChildOf(carrier),
                ))
                .id()
        };
    let mut reins = [Entity::PLACEHOLDER; 2];
    let mut stirrups = [Entity::PLACEHOLDER; 2];
    for s in 0..2 {
        spawn_strap(
            commands,
            HorsePiece::Rein,
            [(head, local.bit[s]), (head, local.jowl[s])],
            REIN,
            &rein,
        );
        spawn_strap(
            commands,
            HorsePiece::Rein,
            [(head, local.jowl[s]), (neck, local.neck[s])],
            REIN,
            &rein,
        );
        reins[s] = spawn_strap(
            commands,
            HorsePiece::Rein,
            [(neck, local.neck[s]), (torso, local.pommel[s])],
            REIN,
            &rein,
        );
        stirrups[s] = commands
            .spawn((
                Name::new("Stirrup"),
                HorsePiece::Stirrup,
                Mesh3d(kit.ring.clone()),
                MeshMaterial3d(iron.clone()),
                local.hanging[s],
                range.clone(),
                ChildOf(torso),
            ))
            .id();
        spawn_strap(
            commands,
            HorsePiece::StirrupLeather,
            [(torso, local.leather[s]), (stirrups[s], Vec3::Y * 0.5)],
            LEATHER,
            &leather,
        );
    }
    commands.entity(carrier).try_insert(Tack {
        reins,
        stirrups,
        torso,
        head,
        seat: tack.kit.fittings.seat,
        torso_rest: tack.torso,
        bit: (local.bit[0] + local.bit[1]) / 2.0,
    });
}

/// La ubicación en el mundo de una correa desde `a` hasta `b`, de `width` de
/// ancho.
pub fn strap_between(a: Vec3, b: Vec3, width: f32) -> GlobalTransform {
    let along = b - a;
    let rotation = Quat::from_rotation_arc(Vec3::Y, along.normalize_or(Vec3::Y));
    GlobalTransform::from(Transform {
        translation: (a + b) / 2.0,
        rotation,
        scale: Vec3::new(width, along.length().max(1e-4), width),
    })
}

/// Estira cada correa entre sus dos extremos, una vez calculados los
/// transforms: se dibuja en este frame donde están sus extremos en este frame.
/// Una correa cuyo extremo ya no está (su hueso se eliminó) queda oculta hasta
/// que se vuelvan a encontrar ambos extremos.
pub(super) fn stretch_straps(
    mut straps: Query<(
        &Strap,
        &ChildOf,
        &mut Transform,
        &mut GlobalTransform,
        &mut Visibility,
    )>,
    places: Query<&GlobalTransform, Without<Strap>>,
) {
    for (strap, child_of, mut transform, mut global, mut shown) in &mut straps {
        let [(a, at_a), (b, at_b)] = strap.ends;
        let (Ok(a), Ok(b), Ok(parent)) =
            (places.get(a), places.get(b), places.get(child_of.parent()))
        else {
            shown.set_if_neq(Visibility::Hidden);
            continue;
        };
        shown.set_if_neq(Visibility::Inherited);
        let placed = strap_between(
            a.transform_point(at_a),
            b.transform_point(at_b),
            strap.width,
        );
        *global = placed;
        *transform =
            Transform::from_matrix(Mat4::from(parent.affine().inverse() * placed.affine()));
    }
}

/// El material de `piece`.
pub fn material(
    piece: HorsePiece,
    palette: &mut Palette,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) -> Handle<StandardMaterial> {
    let (color, finish) = piece.look();
    palette.material(color, finish, materials, images)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Un caballo sustituto: un tubo cerrado a lo largo de +X alrededor del eje
    /// del costillar (radio 0.28, de -0.8 a 0.8 m), con su piel pasando de la
    /// articulación 0 atrás a la articulación 1 adelante, un tubo con forma de
    /// cabeza alrededor del eje de la cabeza en la articulación 2 y un cuello
    /// en la articulación 3.
    pub(crate) fn tube_horse() -> RestHorse {
        let mut positions = Vec::new();
        let mut skins = Vec::new();
        let mut triangles = Vec::new();
        let mut tube = |axis: Axis, length: (f32, f32), radius: f32, joint: Option<u16>| {
            let base = positions.len() as u32;
            let (rings, around) = (16, 24);
            for i in 0..=rings {
                let t = length.0 + (length.1 - length.0) * i as f32 / rings as f32;
                for k in 0..around {
                    let theta = k as f32 / around as f32 * std::f32::consts::TAU;
                    let (o, d) = axis.ray(t, theta);
                    positions.push(o + d * radius);
                    let front = i as f32 / rings as f32;
                    skins.push(match joint {
                        Some(j) => Skin {
                            joints: [j, 0, 0, 0],
                            weights: [1.0, 0.0, 0.0, 0.0],
                        },
                        None => Skin {
                            joints: [0, 1, 0, 0],
                            weights: [1.0 - front, front, 0.0, 0.0],
                        },
                    });
                }
            }
            for i in 0..rings {
                for k in 0..around {
                    let at = |i: u32, k: u32| base + i * around as u32 + k % around as u32;
                    let (i, k) = (i as u32, k as u32);
                    triangles.push([at(i, k), at(i + 1, k), at(i, k + 1)]);
                    triangles.push([at(i, k + 1), at(i + 1, k), at(i + 1, k + 1)]);
                }
            }
        };
        tube(Axis::BARREL, (-0.8, 0.8), 0.28, None);
        tube(Axis::head(), (-0.1, 0.55), 0.13, Some(2));
        let neck = Vec3::new(0.64, 0.768, 0.0);
        tube(
            Axis {
                origin: Vec3::new(0.6, 1.2, 0.0),
                along: neck,
                up: Vec3::new(-neck.y, neck.x, 0.0),
            },
            (0.0, 0.7),
            0.12,
            Some(3),
        );
        RestHorse::new(positions, skins, triangles)
    }

    #[test]
    fn a_ray_from_inside_meets_the_skin_with_its_blended_weights() {
        let horse = tube_horse();
        let (origin, dir) = Axis::BARREL.ray(0.4, 0.0);
        let hit = horse.cast(origin, dir).unwrap();
        // El tramo plano del polígono entre esquinas queda un poco dentro del
        // círculo sobre el que se construye.
        assert!((hit.distance - 0.28).abs() < 0.01, "{hit:?}");
        assert!((hit.point.x - 0.4).abs() < 1e-4);
        // A un cuarto del camino desde adelante: 75% la articulación delantera.
        let front = hit.skin.joints.iter().position(|j| *j == 1).unwrap();
        assert!((hit.skin.weights[front] - 0.75).abs() < 0.02, "{hit:?}");
        assert!((hit.skin.weights.iter().sum::<f32>() - 1.0).abs() < 1e-5);
        // Fuera del cuerpo, un rayo que se aleja de él no encuentra nada.
        assert!(horse.cast(Vec3::new(0.0, 3.0, 0.0), Vec3::Y).is_none());
    }

    #[test]
    fn a_shell_lies_its_offset_off_the_body_and_its_thickness_deep() {
        let horse = tube_horse();
        let shell = saddle()[0];
        let (mesh, misses) = shell.fit(&horse);
        assert_eq!(misses, 0);
        let (along, around) = shell.cells;
        let face = (along + 1) * (around + 1);
        assert_eq!(mesh.positions.len(), 2 * face);
        for k in 0..face {
            let (outer, inner) = (mesh.positions[k], mesh.positions[face + k]);
            let off = |p: Vec3| Vec2::new(p.y - 1.14, p.z).length() - 0.28;
            assert!(
                (off(inner) - shell.offset.top).abs() < 0.012,
                "{} at {inner}",
                off(inner)
            );
            assert!((outer.distance(inner) - shell.thickness).abs() < 1e-4);
            // Adentro y afuera comparten la piel que tienen debajo.
            assert_eq!(mesh.skins[k], mesh.skins[face + k]);
        }
        // El borde delantero sigue más a la articulación delantera que a la
        // trasera.
        let weight_of = |s: &Skin, j: u16| {
            s.joints
                .iter()
                .zip(s.weights)
                .filter(|(k, _)| **k == j)
                .map(|(_, w)| w)
                .sum::<f32>()
        };
        assert!(weight_of(&mesh.skins[face - 1], 1) > weight_of(&mesh.skins[0], 1));
        assert!(
            mesh.triangles
                .iter()
                .flatten()
                .all(|i| (*i as usize) < mesh.positions.len())
        );
    }

    #[test]
    fn the_saddle_layers_nest_and_the_pellon_is_thick_and_shaggy() {
        let horse = tube_horse();
        let up = |piece: HorsePiece| {
            let shell = saddle().into_iter().find(|s| s.piece == piece).unwrap();
            let (mesh, _) = shell.fit(&horse);
            let (along, around) = shell.cells;
            let face = (along + 1) * (around + 1);
            // El punto más alto de la cara exterior y el de la cara interior,
            // sobre la columna.
            let top = |range: std::ops::Range<usize>| {
                mesh.positions[range]
                    .iter()
                    .filter(|p| p.z.abs() < 0.03)
                    .map(|p| p.y)
                    .fold(f32::MIN, f32::max)
            };
            (top(face..2 * face), top(0..face))
        };
        let (_, sudadera) = up(HorsePiece::Sudadera);
        let (carona_in, carona) = up(HorsePiece::Carona);
        let (pellon_in, pellon) = up(HorsePiece::Pellon);
        assert!(
            sudadera < carona_in && carona < pellon_in,
            "{sudadera} {carona_in} {carona} {pellon_in}"
        );
        assert!(pellon - pellon_in > 0.03, "a thick sheepskin");
        // Los bastos quedan entre la carona y el pellón.
        for basto in saddle().iter().filter(|s| s.piece == HorsePiece::Basto) {
            let (mesh, _) = basto.fit(&horse);
            let high = mesh.positions.iter().map(|p| p.y).fold(f32::MIN, f32::max);
            assert!(high > carona && high < pellon, "{high}");
        }
    }

    #[test]
    fn the_fittings_are_found_on_the_body() {
        let horse = tube_horse();
        let f = Fittings::find(&horse).unwrap();
        for s in 0..2 {
            // La izquierda es -Z.
            let sign = if s == 0 { -1.0 } else { 1.0 };
            for p in [f.pommel[s], f.leather[s], f.hanging[s]] {
                assert!(p.z * sign > 0.0, "{s}: {p}");
            }
            assert!(f.hanging[s].z.abs() > 0.28, "clear of the barrel");
        }
        let seat = Vec2::new(f.seat.y - 1.14, f.seat.z).length() - 0.28;
        let pellon = saddle()[4];
        assert!(
            (seat - pellon.offset.top - pellon.thickness).abs() < 0.012,
            "{seat}"
        );
    }

    #[test]
    fn a_strap_runs_from_one_end_to_the_other() {
        let (a, b) = (Vec3::new(1.0, 2.0, 0.5), Vec3::new(-0.5, 1.0, 0.2));
        let placed = strap_between(a, b, 0.02);
        assert!(placed.transform_point(Vec3::Y * 0.5).distance(b) < 1e-5);
        assert!(placed.transform_point(Vec3::Y * -0.5).distance(a) < 1e-5);
        let (scale, _, _) = placed.to_scale_rotation_translation();
        assert!((scale.x - 0.02).abs() < 1e-6 && (scale.z - 0.02).abs() < 1e-6);
    }

    // --- con el caballo incluido ---------------------------------------------

    use crate::character::cpu;
    use crate::character::horse::tests::saddled;
    use crate::character::horse::{HorseBody, TIED_IDLES};
    use crate::character::{CharacterVisual, Coat, FigureKind};
    use bevy::mesh::skinning::SkinnedMesh;
    use bevy::platform::collections::HashMap;

    /// La distancia con signo desde `p` hasta el más cercano de los `triangles`
    /// (sobre `positions`), positiva del lado hacia el que mira su orden.
    pub(crate) fn signed_distance(p: Vec3, positions: &[Vec3], triangles: &[[u32; 3]]) -> f32 {
        let mut best = (f32::MAX, 0.0);
        for t in triangles {
            let [a, b, c] = t.map(|i| positions[i as usize]);
            let q = closest_on_triangle(p, a, b, c);
            let d = p.distance_squared(q);
            if d < best.0 {
                let n = (b - a).cross(c - a).normalize_or_zero();
                best = (d, (p - q).dot(n).signum());
            }
        }
        best.0.sqrt() * best.1
    }

    /// El punto más cercano a `p` sobre el triángulo `a b c` (Ericson).
    pub(crate) fn closest_on_triangle(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
        let (ab, ac, ap) = (b - a, c - a, p - a);
        let (d1, d2) = (ab.dot(ap), ac.dot(ap));
        if d1 <= 0.0 && d2 <= 0.0 {
            return a;
        }
        let bp = p - b;
        let (d3, d4) = (ab.dot(bp), ac.dot(bp));
        if d3 >= 0.0 && d4 <= d3 {
            return b;
        }
        let vc = d1 * d4 - d3 * d2;
        if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
            return a + ab * (d1 / (d1 - d3));
        }
        let cp = p - c;
        let (d5, d6) = (ab.dot(cp), ac.dot(cp));
        if d6 >= 0.0 && d5 <= d6 {
            return c;
        }
        let vb = d5 * d2 - d1 * d6;
        if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
            return a + ac * (d2 / (d2 - d6));
        }
        let va = d3 * d6 - d5 * d4;
        if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
            return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
        }
        let denom = 1.0 / (va + vb + vc);
        a + ab * (vb * denom) + ac * (vc * denom)
    }

    /// Si el segmento `a b` atraviesa alguno de los `triangles`.
    pub(crate) fn crosses(a: Vec3, b: Vec3, positions: &[Vec3], triangles: &[[u32; 3]]) -> bool {
        let dir = b - a;
        triangles.iter().any(|t| {
            let [p, q, r] = t.map(|i| positions[i as usize]);
            let (e1, e2) = (q - p, r - p);
            let h = dir.cross(e2);
            let det = e1.dot(h);
            if det.abs() < 1e-12 {
                return false;
            }
            let s = a - p;
            let u = s.dot(h) / det;
            let qv = s.cross(e1);
            let v = dir.dot(qv) / det;
            let t = e2.dot(qv) / det;
            (0.0..=1.0).contains(&u) && v >= 0.0 && u + v <= 1.0 && (0.0..=1.0).contains(&t)
        })
    }

    /// La piel del caballo (los materiales de su cuerpo, no su crin ni sus
    /// ojos) en `drawn`: posiciones y triángulos, fusionados.
    pub(crate) fn horse_skin(drawn: &[cpu::Drawn]) -> (Vec<Vec3>, Vec<[u32; 3]>) {
        let mut positions = Vec::new();
        let mut triangles = Vec::new();
        for d in drawn {
            if ["Main", "Main_Dark", "Main_Light", "Muzzle", "Hooves"].contains(&d.name.as_str()) {
                let base = positions.len() as u32;
                positions.extend(&d.positions);
                triangles.extend(d.triangles.iter().map(|t| t.map(|i| i + base)));
            }
        }
        (positions, triangles)
    }

    /// Los `triangles` (sobre `positions`) agrupados en celdas de 10 cm, para
    /// buscar el más cercano a un punto solo entre los que están a menos de
    /// 10 cm de él.
    struct Near<'a> {
        positions: &'a [Vec3],
        triangles: &'a [[u32; 3]],
        cells: HashMap<IVec3, Vec<u32>>,
    }

    impl<'a> Near<'a> {
        const CELL: f32 = 0.1;

        fn new(positions: &'a [Vec3], triangles: &'a [[u32; 3]]) -> Self {
            let cell = |p: Vec3| (p / Self::CELL).floor().as_ivec3();
            let mut cells: HashMap<IVec3, Vec<u32>> = HashMap::default();
            for (k, t) in triangles.iter().enumerate() {
                let [a, b, c] = t.map(|i| positions[i as usize]);
                let (lo, hi) = (cell(a.min(b).min(c)), cell(a.max(b).max(c)));
                for x in lo.x..=hi.x {
                    for y in lo.y..=hi.y {
                        for z in lo.z..=hi.z {
                            cells.entry(IVec3::new(x, y, z)).or_default().push(k as u32);
                        }
                    }
                }
            }
            Self {
                positions,
                triangles,
                cells,
            }
        }

        /// [`signed_distance`] desde `p` hasta los triángulos, si el más
        /// cercano está a menos de 10 cm de él.
        fn signed_distance(&self, p: Vec3) -> Option<f32> {
            let at = (p / Self::CELL).floor().as_ivec3();
            let mut near: Vec<[u32; 3]> = Vec::new();
            for x in -1..=1 {
                for y in -1..=1 {
                    for z in -1..=1 {
                        if let Some(ks) = self.cells.get(&(at + IVec3::new(x, y, z))) {
                            near.extend(ks.iter().map(|&k| self.triangles[k as usize]));
                        }
                    }
                }
            }
            near.sort_unstable();
            near.dedup();
            let s = signed_distance(p, self.positions, &near);
            (!near.is_empty() && s.abs() <= Self::CELL).then_some(s)
        }
    }

    /// Las peores holguras a lo largo de `frames` frames del caballo ensillado
    /// de `app` bajo `me`, a menos de 10 cm de su piel: cuánto se separan de
    /// ella las carcasas de los aperos (negativo: dentro de ella), cuánto se
    /// separa el modelo del jinete, y las correas que atraviesan la piel.
    pub(crate) fn clearances(app: &mut App, me: Entity, frames: usize) -> (f32, f32, Vec<String>) {
        let shells = [
            "Sudadera",
            "Carona",
            "Basto",
            "Pellon",
            "Cincha",
            "Headstall",
            "Bit",
        ];
        let not_rider = [
            "Main",
            "Main_Dark",
            "Main_Light",
            "Muzzle",
            "Hooves",
            "Hair",
            "Eye_Black",
            "Eye_White",
            "Rein",
            "StirrupLeather",
            "Stirrup",
        ];
        let (mut tack, mut rider, mut cuts) = (f32::MAX, f32::MAX, Vec::new());
        for _ in 0..frames {
            app.update();
            let drawn = cpu::drawn(app.world_mut(), Some(me));
            let (skin, tris) = horse_skin(&drawn);
            let near = Near::new(&skin, &tris);
            for d in &drawn {
                let (is_tack, is_rider) = (
                    shells.contains(&d.name.as_str()),
                    !shells.contains(&d.name.as_str()) && !not_rider.contains(&d.name.as_str()),
                );
                if !is_tack && !is_rider {
                    continue;
                }
                for p in d.positions.iter().step_by(3) {
                    // El lado del triángulo más cercano solo distingue adentro
                    // de afuera cerca de la piel (lejos, el dorso de una oreja
                    // delgada puede ser el más cercano).
                    let Some(s) = near.signed_distance(*p) else {
                        continue;
                    };
                    let worst = if is_tack { &mut tack } else { &mut rider };
                    *worst = worst.min(s);
                }
            }
            let world = app.world_mut();
            let mut straps = world.query::<(&Strap, &HorsePiece)>();
            for (strap, piece) in straps.iter(world) {
                let at = |(e, p): (Entity, Vec3)| {
                    world.get::<GlobalTransform>(e).unwrap().transform_point(p)
                };
                if crosses(at(strap.ends[0]), at(strap.ends[1]), &skin, &tris) {
                    let n = |e: Entity| world.get::<Name>(e).map(|n| n.to_string());
                    cuts.push(format!(
                        "{piece:?} {:?}-{:?} {:.2} {:.2}",
                        n(strap.ends[0].0),
                        n(strap.ends[1].0),
                        at(strap.ends[0]),
                        at(strap.ends[1])
                    ));
                }
            }
        }
        (tack, rider, cuts)
    }

    #[test]
    fn a_saddled_horse_wears_its_tack_on_its_own_joints() {
        let (mut app, _) = saddled(CharacterVisual::Horse(Coat::Bayo), 0.0);
        let world = app.world_mut();
        // Cada pieza del recado y de la brida, cada una con skinning en las
        // propias articulaciones del caballo (las de la piel), en el rango de
        // los accesorios.
        let props = crate::settlement::ranges()[1].clone();
        let mut pieces = world.query::<(
            &HorsePiece,
            &ChildOf,
            Option<&SkinnedMesh>,
            Option<&bevy::camera::visibility::VisibilityRange>,
        )>();
        let found: Vec<(HorsePiece, bool)> = pieces
            .iter(world)
            .map(|(p, _, skin, range)| {
                assert!(range == Some(&props), "{p:?}");
                (*p, skin.is_some())
            })
            .collect();
        for piece in [
            HorsePiece::Sudadera,
            HorsePiece::Carona,
            HorsePiece::Basto,
            HorsePiece::Pellon,
            HorsePiece::Cincha,
            HorsePiece::Headstall,
            HorsePiece::Bit,
        ] {
            assert!(found.contains(&(piece, true)), "{piece:?} skinned");
        }
        let count = |p| found.iter().filter(|(f, _)| *f == p).count();
        assert_eq!(count(HorsePiece::Basto), 2);
        assert_eq!(count(HorsePiece::Bit), 2);
        assert_eq!(count(HorsePiece::Stirrup), 2);
        assert_eq!(count(HorsePiece::StirrupLeather), 2);
        assert_eq!(count(HorsePiece::Rein), 6, "three stretches a side");
        let main = world
            .query::<(&SkinnedMesh, &bevy::gltf::GltfMaterialName)>()
            .iter(world)
            .find(|(_, n)| n.0 == "Main")
            .map(|(s, _)| s.joints.clone())
            .unwrap();
        for (piece, _, skin, _) in pieces.iter(world) {
            if let Some(skin) = skin {
                assert_eq!(skin.joints, main, "{piece:?}");
            }
        }
    }

    /// Si existe cada entidad para la que el caballo de `app` bajo `me` tiene
    /// un [`Tack`]; los aperos, si los hay.
    fn whole_tack(app: &mut App, me: Entity) -> Option<Tack> {
        let world = app.world();
        let carrier = world.get::<Children>(me).unwrap()[0];
        let tack = *world.get::<Tack>(carrier)?;
        for e in [tack.reins, tack.stirrups, [tack.torso, tack.head]].concat() {
            assert!(world.get_entity(e).is_ok(), "{e:?} is gone");
        }
        Some(tack)
    }

    #[test]
    fn a_scene_spawned_again_is_checked_before_the_old_tack_comes_off() {
        use bevy::world_serialization::WorldInstance;
        let (mut app, me) = saddled(CharacterVisual::Horse(Coat::Bayo), 0.0);
        let count = |app: &mut App, piece: HorsePiece| {
            let world = app.world_mut();
            world
                .query::<&HorsePiece>()
                .iter(world)
                .filter(|p| **p == piece)
                .count()
        };
        let world = app.world_mut();
        let carrier = world.get::<Children>(me).unwrap()[0];
        let root = world.get::<Children>(carrier).unwrap()[0];
        assert!(world.get::<HorseModel>(root).is_some());
        let ready = |world: &mut World| WorldInstanceReady {
            entity: root,
            instance_id: **world.get::<WorldInstance>(root).unwrap(),
        };
        // Completo: preparado de nuevo, una vez.
        let event = ready(world);
        world.trigger(event);
        app.update();
        assert!(whole_tack(&mut app, me).is_some());
        assert_eq!(count(&mut app, HorsePiece::Stirrup), 2);
        assert_eq!(count(&mut app, HorsePiece::Rein), 6);
        // Sin su hueso del torso: sin aperos, y ninguno a medias.
        let world = app.world_mut();
        let torso = world
            .query::<(Entity, &Name)>()
            .iter(world)
            .find(|(e, n)| n.as_str() == "Torso" && world.get::<ChildOf>(*e).is_some())
            .map(|(e, _)| e)
            .unwrap();
        world.entity_mut(torso).insert(Name::new("Torso.lost"));
        let event = ready(world);
        world.trigger(event);
        app.update();
        assert!(whole_tack(&mut app, me).is_none());
        for piece in [HorsePiece::Pellon, HorsePiece::Rein, HorsePiece::Stirrup] {
            assert_eq!(count(&mut app, piece), 0, "{piece:?}");
        }
    }

    #[test]
    fn a_strap_with_an_end_gone_is_hidden_until_it_has_both_again() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        let at = |x: f32| GlobalTransform::from_xyz(x, 1.0, 0.0);
        let parent = world.spawn(at(0.0)).id();
        let (a, b, c) = (
            world.spawn(at(1.0)).id(),
            world.spawn(at(2.0)).id(),
            world.spawn(at(3.0)).id(),
        );
        let strap = world
            .spawn((
                Strap {
                    ends: [(a, Vec3::ZERO), (b, Vec3::ZERO)],
                    width: 0.02,
                },
                Transform::default(),
                GlobalTransform::default(),
                Visibility::default(),
                ChildOf(parent),
            ))
            .id();
        let stretch = |world: &mut World| {
            world.run_system_once(stretch_straps).unwrap();
            (
                *world.get::<Visibility>(strap).unwrap(),
                world.get::<GlobalTransform>(strap).unwrap().translation(),
            )
        };
        assert_eq!(
            stretch(&mut world),
            (Visibility::Inherited, Vec3::new(1.5, 1.0, 0.0))
        );
        world.despawn(b);
        assert_eq!(stretch(&mut world).0, Visibility::Hidden);
        world.get_mut::<Strap>(strap).unwrap().ends[1].0 = c;
        assert_eq!(
            stretch(&mut world),
            (Visibility::Inherited, Vec3::new(2.0, 1.0, 0.0))
        );
    }

    #[test]
    fn the_tack_and_the_rider_clear_the_horse_through_every_gait() {
        use crate::character::horse::tests::ride;
        use crate::character::rider::tests::seated;
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = seated(rider, 0.0);
        // Parado, caminando, la caminata estirada y la velocidad máxima: a lo
        // largo de una zancada completa o más cada uno.
        for (speed, frames) in [(0.0, 8), (1.5, 24), (4.5, 12), (12.2, 12)] {
            for _ in 0..30 {
                ride(&mut app, me, speed);
            }
            let (tack, seat, cuts) = clearances(&mut app, me, frames);
            println!(
                "{speed} m/s: tack {tack:.4} m and the rider {seat:.4} m off the skin, cuts {cuts:?}"
            );
            assert!(
                tack > 0.002,
                "{speed} m/s: the tack cuts into the horse ({tack})"
            );
            assert!(cuts.is_empty(), "{speed} m/s: {cuts:?}");
            assert!(
                seat > 0.0,
                "{speed} m/s: the rider sinks into the horse ({seat})"
            );
        }
    }

    #[test]
    fn a_tied_horse_keeps_its_tack_and_reins_clear_through_every_idle() {
        let (mut app, me) = saddled(CharacterVisual::Horse(Coat::Overo), 0.0);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        // Cada espera por turno: una semilla que la elige para los próximos
        // segundos, y después una vez que entró por completo.
        for idle in TIED_IDLES {
            let clock = app.world().resource::<Time>().elapsed_secs();
            let mut body = app.world().get::<HorseBody>(carrier).unwrap().clone();
            body.seed = (0..1000)
                .find(|&seed| {
                    let body = HorseBody {
                        seed,
                        ..body.clone()
                    };
                    (0..80).all(|k| body.idle(clock + k as f32 * 0.05) == idle)
                })
                .expect("a seed that picks it");
            app.world_mut().entity_mut(carrier).insert(body.clone());
            for _ in 0..30 {
                app.update();
            }
            let (tack, _, cuts) = clearances(&mut app, me, 8);
            let world = app.world_mut();
            let weight = world
                .query::<&crate::character::horse::HorseRig>()
                .iter(world)
                .find(|r| r.started)
                .unwrap()
                .weights[idle.index()];
            assert!(weight > 0.9, "{idle:?} plays: {weight}");
            assert!(tack > 0.002, "{idle:?}: {tack}");
            assert!(cuts.is_empty(), "{idle:?}: {cuts:?}");
        }
        // Nadie lo monta: los estribos cuelgan de la montura y las riendas
        // descansan en la cabezada.
        let tack = *app.world().get::<Tack>(carrier).unwrap();
        for s in 0..2 {
            let hung = app
                .world()
                .get::<ChildOf>(tack.stirrups[s])
                .unwrap()
                .parent();
            assert_eq!(app.world().get::<Name>(hung).unwrap().as_str(), "Torso");
            let rein = app.world().get::<Strap>(tack.reins[s]).unwrap();
            assert_eq!(rein.ends[1].0, hung, "to the pommel");
        }
    }
}
