//! El recado criollo y la brida de cuero crudo de un peón de estancia de La
//! Pampa en 1919, sobre el caballo modelo (ver [`horse`](super::horse)).
//!
//! **Las capas**, de abajo hacia arriba, como las describen los viajeros del
//! siglo XIX que cita *Tradición Gaucha* (Hutchinson 1862, Zeballos 1878,
//! Seymour, Mac Cann, Skogman, Hinchliff): la bajera (una jerga tejida), la
//! matra (la manta gruesa de telar indio, con su guarda escalonada y sus
//! flecos), la carona (suela labrada a fuego), los dos bastos (rollos de suela
//! rellenos de junco), la cincha de tientos de cuero crudo con su panel
//! tejido, sus argollas de hierro y el látigo que sube desde cada una, el
//! cojinillo (cuero de oveja de lana larga y rizada), el sobrepuesto de
//! carpincho y el cinchón que envuelve todo. La brida: la cabezada trenzada
//! (testera, frontalera, quijeras, muserola y fiador), el freno de piernas
//! curvas de hierro con su bocado y su barbada, y las riendas de tiento
//! trenzado. Los estribos: pequeños, triangulares, de hierro forjado.
//!
//! Hay dos clases de piezas, y todas se doblan con el caballo en cada clip
//! con los pesos de la piel que tienen debajo:
//!
//! - **El recado modelado** ([`Recado`], [`RECADO_PATH`]): los bastos, el
//!   cojinillo con sus mechones, el sobrepuesto, la cabezada y el freno,
//!   modelados en Blender sobre el caballo en reposo por `tools/recado.py`,
//!   con skinning a sus huesos por nombre: las piezas del lomo solo a los
//!   huesos del tronco (nunca al cuello ni a las patas, que las levantarían
//!   al bajar la cabeza o al galopar), la cabezada y el freno a la cabeza y
//!   al cuello como la piel que tienen debajo. Se llevan al marco del cuerpo del
//!   caballo ([`rest_mesh`]) con el reposo de sus articulaciones, tomado
//!   cuando su escena se prepara ([`PendingTack`]), y se dibujan con las
//!   articulaciones de la piel del caballo. Sus marcas dicen dónde salen las
//!   riendas y dónde se sienta el jinete.
//! - **Carcasas** ([`Shell`]): la bajera, la matra, la carona, la cincha, el
//!   látigo y el cinchón. Cada una es una cuadrícula de rayos lanzados desde
//!   un eje dentro del cuerpo ([`Axis`]); donde un rayo sale por primera vez
//!   del cuerpo (o, para el cinchón, por última vez del recado modelado), la
//!   cara interior de la capa queda un poco por fuera según el ángulo
//!   ([`Offset`]), y su cara exterior, un grosor más allá, adelgazando hacia
//!   los bordes redondeados; las mantas caen con las esquinas redondeadas.
//!   Los vertices toman los pesos mezclados del triángulo que tocó cada rayo
//!   ([`RestHorse::cast`]).
//! - **Hierros**: las argollas de la cincha y el estribo ([`stirrup`]).
//! - **Correas** ([`Strap`]): las riendas (desde el freno, por detrás de la
//!   quijada y a lo largo del cuello hasta las manos del jinete, o hasta la
//!   montura en un caballo sin jinete) y las aciones (desde la montura hasta
//!   los estribos), estiradas en cada frame después de que se calculan los
//!   transforms ([`stretch_straps`]). Las aciones son planas y rectas; el
//!   último tramo de las riendas cuelga en una curva dibujada en tramos
//!   ([`StrapPart`], [`strap_curve`]).
//! - **Estribos**: en un caballo sin jinete cuelgan de la montura; los pies de
//!   un jinete sentado los llevan (ver [`rider`](super::rider)).
//!
//! **Los mapas**: seis conjuntos KTX2 (color, normal map y ORM) de
//! [`TACK_DIR`] ([`TackSet`]), pintados por `tools/tack.py` y
//! `tools/recado.py`; sin ellos, cada pieza se dibuja con su color liso.
//! **El costo** para la iGPU Renoir: a lo sumo [`TACK_BUDGET`] triángulos por
//! caballo ensillado (medidos: 11.002) y unos 11,8 MB de video entre los seis
//! conjuntos con sus mips (dos de 1024², tres de 512² y uno de 256², a un
//! byte por texel en BC7), compartidos por todos los caballos.
//!
//! Los aperos se construyen una sola vez, a partir del primer caballo
//! preparado ([`TackBuilt`]), y los comparten todos los caballos ([`TackKit`]);
//! cada caballo solo crea entidades.

use std::f32::consts::{FRAC_PI_2, PI, TAU};
use std::path::Path;

use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::SystemParam;
use bevy::gltf::{Gltf, GltfMaterialName, GltfMesh, GltfNode, GltfSkin};
use bevy::image::CompressedImageFormatSupport;
use bevy::math::Affine3A;
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;

use super::figure::Rgb;
use super::finish::{Finish, Palette};
use super::horse::{HorseModel, transform_of};
use super::model::model_range;
use crate::AssetRoot;
use crate::facade::materials::{MAPS, load_map};

/// Una pieza del recado o de la brida, de abajo hacia arriba.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HorsePiece {
    /// La jerga tejida de abajo, directo sobre el lomo.
    Bajera,
    /// La manta gruesa de telar que va encima de la bajera.
    Matra,
    /// La suela grande que protege a las mantas de los bastos.
    Carona,
    /// Los dos rollos de suela rellenos de junco a cada lado de la columna.
    Basto,
    /// La cincha de tientos de cuero crudo bajo la panza.
    Cincha,
    /// Las argollas de hierro en los extremos de la cincha.
    Argolla,
    /// El cojinillo: el cuero de oveja sobre los bastos.
    Pellon,
    /// La lana larga del pellón: capas con alpha test sobre sus costados y el
    /// flequillo que cuelga de sus bordes.
    Vellon,
    /// El cuero de carpincho sobre el pellón, donde se sienta el jinete.
    Sobrepuesto,
    /// La tira de cuero crudo que envuelve todo.
    Cinchon,
    /// La cabezada de cuero crudo trenzado: testera, frontalera, muserola y
    /// quijeras.
    Headstall,
    /// El freno: una pierna curva de hierro de cada lado, con sus argollas.
    Bit,
    Rein,
    StirrupLeather,
    Stirrup,
}

/// Una jerga tejida, de un rojo óxido por el uso.
const JERGA: Rgb = [124, 56, 42];
const MATRA: Rgb = [168, 150, 128];
const CARONA: Rgb = [88, 60, 40];
const BASTO: Rgb = [64, 44, 30];
const PELLON: Rgb = [208, 196, 170];
const CARPINCHO: Rgb = [170, 136, 98];
const RAWHIDE: Rgb = [166, 140, 104];
const IRON: Rgb = [70, 68, 66];

impl HorsePiece {
    pub const ALL: [Self; 15] = [
        Self::Bajera,
        Self::Matra,
        Self::Carona,
        Self::Basto,
        Self::Cincha,
        Self::Argolla,
        Self::Pellon,
        Self::Vellon,
        Self::Sobrepuesto,
        Self::Cinchon,
        Self::Headstall,
        Self::Bit,
        Self::Rein,
        Self::StirrupLeather,
        Self::Stirrup,
    ];

    /// Su color liso, de qué está hecha y el conjunto de mapas que la viste.
    pub fn look(self) -> (Rgb, Finish, TackSet) {
        match self {
            Self::Bajera => (JERGA, Finish::Wool, TackSet::Blankets),
            Self::Matra => (MATRA, Finish::Wool, TackSet::Blankets),
            Self::Carona => (CARONA, Finish::Leather, TackSet::Leather),
            Self::Basto => (BASTO, Finish::Leather, TackSet::Recado),
            Self::StirrupLeather => (BASTO, Finish::Leather, TackSet::Leather),
            Self::Pellon => (PELLON, Finish::Fleece, TackSet::Recado),
            Self::Vellon => (PELLON, Finish::Fleece, TackSet::Wool),
            Self::Sobrepuesto => (CARPINCHO, Finish::Leather, TackSet::Recado),
            Self::Cincha | Self::Cinchon | Self::Headstall | Self::Rein => {
                (RAWHIDE, Finish::Rawhide, TackSet::Rawhide)
            }
            Self::Bit | Self::Stirrup | Self::Argolla => (IRON, Finish::Iron, TackSet::Iron),
        }
    }
}

/// Un conjunto de mapas KTX2 de los aperos (color, normal map y ORM), hecho
/// por `tools/tack.py` y `tools/textures_to_ktx2.sh` en [`TACK_DIR`] y
/// compartido por todos los caballos ensillados.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TackSet {
    /// La bajera (la mitad izquierda) y la matra (la derecha), 1024².
    Blankets,
    /// La suela de la carona y las aciones, 512².
    Leather,
    /// El atlas del recado modelado (`tools/recado.py`): el cojinillo de lana
    /// rizada, los bastos con su costura y sus puntas labradas y el
    /// sobrepuesto de carpincho, 1024².
    Recado,
    /// Los mechones de lana que cuelgan del cojinillo, con su opacidad en el
    /// alfa, 512².
    Wool,
    /// El cuero crudo: la trenza (la mitad izquierda) y los tientos planos (la
    /// derecha), 512².
    Rawhide,
    /// El hierro forjado del freno, las argollas y los estribos, 256².
    Iron,
}

/// Dónde viven los conjuntos de los aperos, bajo el [`AssetRoot`].
pub const TACK_DIR: &str = "models/tack";

impl TackSet {
    pub const ALL: [Self; 6] = [
        Self::Blankets,
        Self::Leather,
        Self::Recado,
        Self::Wool,
        Self::Rawhide,
        Self::Iron,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Blankets => "tack_blankets",
            Self::Leather => "tack_leather",
            Self::Recado => "tack_recado",
            Self::Wool => "tack_wool",
            Self::Rawhide => "tack_rawhide",
            Self::Iron => "tack_iron",
        }
    }

    /// El lado de sus mapas, en texels.
    pub fn size(self) -> u32 {
        match self {
            Self::Blankets | Self::Recado => 1024,
            Self::Iron => 256,
            _ => 512,
        }
    }

    /// La ruta de su mapa `name` (`color`, `normal_gl` u `orm`), bajo el
    /// [`AssetRoot`].
    pub fn path(self, name: &str) -> String {
        format!("{TACK_DIR}/{}/{name}.ktx2", self.name())
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
        self.cast_by(origin, dir, false)
    }

    /// Como [`RestHorse::cast`], pero donde el rayo sale por última vez: la
    /// cara de arriba de una pieza con volumen (el recado), no la de abajo.
    pub fn cast_far(&self, origin: Vec3, dir: Vec3) -> Option<Hit> {
        self.cast_by(origin, dir, true)
    }

    fn cast_by(&self, origin: Vec3, dir: Vec3, far: bool) -> Option<Hit> {
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
            if d > 1e-5 && best.is_none_or(|(b, ..)| if far { d > b } else { d < b }) {
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

/// Cuánto se separa del cuerpo la cara interior de una capa, en metros, según
/// el ángulo alrededor del eje: los `knots` (ángulo desde el 0, separación),
/// con el ángulo creciente, pasando suavemente de uno al otro y constante más
/// allá de los extremos. El mismo de los dos lados, con el ángulo llevado a
/// ±π.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Offset {
    pub knots: [(f32, f32); 6],
}

impl Offset {
    pub const fn even(at: f32) -> Self {
        Self {
            knots: [(0.0, at); 6],
        }
    }

    /// `top` dentro de `flat` radianes del ángulo 0, pasando suavemente a
    /// `edge` hasta `flat + fade`.
    pub const fn ramp(top: f32, flat: f32, fade: f32, edge: f32) -> Self {
        Self::knots([(0.0, top), (flat, top), (flat + fade, edge)])
    }

    /// Una separación por cada ángulo de `knots`.
    pub const fn knots<const N: usize>(knots: [(f32, f32); N]) -> Self {
        let mut all = [knots[N - 1]; 6];
        let mut k = 0;
        while k < N {
            all[k] = knots[k];
            k += 1;
        }
        Self { knots: all }
    }

    pub fn at(&self, theta: f32) -> f32 {
        let theta = (theta + PI).rem_euclid(TAU) - PI;
        let a = theta.abs();
        let [first, ..] = self.knots;
        if a <= first.0 {
            return first.1;
        }
        for pair in self.knots.windows(2) {
            let ((a0, o0), (a1, o1)) = (pair[0], pair[1]);
            if a <= a1 {
                let s = ((a - a0) / (a1 - a0).max(1e-6)).clamp(0.0, 1.0);
                return o0 + (o1 - o0) * s * s * (3.0 - 2.0 * s);
            }
        }
        self.knots[5].1
    }
}

/// Cómo se reparten las UV de una capa sobre su conjunto de mapas: `u` a lo
/// largo del eje y `v` alrededor de él (al revés con `swap`), de un extremo
/// del rango al otro. Más allá de 1, los mapas se repiten.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Uv {
    pub u: (f32, f32),
    pub v: (f32, f32),
    pub swap: bool,
}

impl Uv {
    pub const fn new(u: (f32, f32), v: (f32, f32)) -> Self {
        Self { u, v, swap: false }
    }

    /// Las UV de la fracción `a` a lo largo y `b` alrededor.
    pub fn at(&self, a: f32, b: f32) -> [f32; 2] {
        let (a, b) = if self.swap { (b, a) } else { (a, b) };
        [
            self.u.0 + (self.u.1 - self.u.0) * a,
            self.v.0 + (self.v.1 - self.v.0) * b,
        ]
    }
}

/// Una capa envuelta alrededor del cuerpo: `along` a lo largo del eje (en
/// metros) y `around` alrededor de él (en radianes), separada por `offset`, de
/// `thickness` de espesor, con la cara exterior abultada según `shag` (una
/// fracción del espesor) como un cuero de oveja. Sus bordes se redondean a lo
/// largo de `round` metros (adelgaza hasta un tercio de su espesor en el
/// borde mismo); una `sheet` es solo su cara exterior, sin espesor ni bordes
/// (la lana con alpha test).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shell {
    pub piece: HorsePiece,
    pub axis: Axis,
    pub along: (f32, f32),
    pub around: (f32, f32),
    pub offset: Offset,
    pub thickness: f32,
    pub shag: f32,
    pub round: f32,
    pub sheet: bool,
    /// Cuánto baja su cara interior hacia sus extremos a lo largo del eje, en
    /// metros: la capa cae sobre el lomo adelante y atrás.
    pub drape: f32,
    /// El radio de sus esquinas, en metros: una manta cae con las esquinas
    /// redondeadas.
    pub corner: f32,
    /// Cuánto varía, de una columna a otra, hasta dónde llega alrededor (una
    /// fracción): el borde desparejo de la lana.
    pub ragged: f32,
    /// Se ajusta por encima del recado: donde un rayo lo cruza, la capa se
    /// separa de su cara de arriba, no de la piel (el cinchón).
    pub over: bool,
    pub uv: Uv,
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

impl TackMesh {
    /// Agrega `other` a esta mesh.
    pub fn extend(&mut self, other: TackMesh) {
        let base = self.positions.len() as u32;
        self.positions.extend(other.positions);
        self.skins.extend(other.skins);
        self.uvs.extend(other.uvs);
        self.triangles
            .extend(other.triangles.into_iter().map(|t| t.map(|i| i + base)));
    }
}

/// `mesh` (una primitiva con skinning de otro archivo) llevada al marco del
/// cuerpo en reposo con `skin` (por cada una de sus articulaciones, el reposo
/// de la del caballo del mismo nombre por su pose de bind inversa), con sus
/// articulaciones cambiadas por las de la piel del caballo según `remap`.
/// `None` si le faltan posiciones, UV, articulaciones, pesos o índices.
pub fn rest_mesh(mesh: &Mesh, skin: &[Affine3A], remap: &[u16]) -> Option<TackMesh> {
    let (
        Some(VertexAttributeValues::Float32x3(positions)),
        Some(VertexAttributeValues::Float32x2(uvs)),
        Some(VertexAttributeValues::Uint16x4(joints)),
        Some(VertexAttributeValues::Float32x4(weights)),
        Some(indices),
    ) = (
        mesh.attribute(Mesh::ATTRIBUTE_POSITION),
        mesh.attribute(Mesh::ATTRIBUTE_UV_0),
        mesh.attribute(Mesh::ATTRIBUTE_JOINT_INDEX),
        mesh.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT),
        mesh.indices(),
    )
    else {
        return None;
    };
    let mut out = TackMesh::default();
    for (((p, j), w), uv) in positions.iter().zip(joints).zip(weights).zip(uvs) {
        let mut at = Vec3::ZERO;
        for k in 0..4 {
            if let Some(m) = skin.get(j[k] as usize) {
                at += w[k] * m.transform_point3(Vec3::from(*p));
            }
        }
        out.positions.push(at);
        let pairs = [0, 1, 2, 3].map(|k| (remap.get(j[k] as usize).copied().unwrap_or(0), w[k]));
        out.skins.push(Skin::heaviest(&pairs));
        out.uvs.push(*uv);
    }
    let indices: Vec<u32> = indices.iter().map(|i| i as u32).collect();
    out.triangles.extend(indices.as_chunks::<3>().0);
    Some(out)
}

impl From<&TackMesh> for RestHorse {
    fn from(mesh: &TackMesh) -> Self {
        Self::new(
            mesh.positions.clone(),
            mesh.skins.clone(),
            mesh.triangles.clone(),
        )
    }
}

/// Un bulto suave en 0..1 para el vellón del pellón: siempre el mismo en el
/// mismo lugar.
fn lump(t: f32, theta: f32) -> f32 {
    0.5 + 0.25 * ((t * 53.0 + theta * 9.1).sin() + (t * 23.0 - theta * 17.3).sin())
}

impl Shell {
    /// Qué fracción de su espesor tiene la capa a `t` metros a lo largo y con
    /// el ángulo `theta`, a `reach` metros del eje: entera lejos de los bordes,
    /// bajando en un cuarto de círculo hasta un tercio en el borde mismo (una
    /// capa que da toda la vuelta no tiene bordes alrededor).
    pub fn rim(&self, t: f32, theta: f32, reach: f32) -> f32 {
        if self.round <= 0.0 {
            return 1.0;
        }
        let mut d = (t - self.along.0).min(self.along.1 - t);
        let (lo, hi) = (
            self.around.0.min(self.around.1),
            self.around.0.max(self.around.1),
        );
        if hi - lo < TAU - 1e-3 {
            d = d.min((theta - lo).min(hi - theta) * reach);
        }
        let x = (d / self.round).clamp(0.0, 1.0);
        1.0 / 3.0 + 2.0 / 3.0 * (1.0 - (1.0 - x) * (1.0 - x)).sqrt()
    }

    /// Dónde va el vertex `i` a lo largo y `k` alrededor: a lo largo del eje y
    /// el ángulo, con las esquinas redondeadas y el borde de la lana
    /// desparejo.
    pub fn place(&self, i: usize, k: usize) -> (f32, f32) {
        let (along, around) = self.cells;
        let (a, mut b) = (i as f32 / along as f32, k as f32 / around as f32);
        if self.ragged > 0.0 {
            // Cada columna llega un poco más o un poco menos, siempre igual: en
            // mechones de largo desparejo, no en una fila.
            let hash = ((i as f32 * 12.9898 + 4.1).sin() * 43758.547).fract().abs() * 2.0 - 1.0;
            let h = 0.55 * (i as f32 * 1.37 + 0.4).sin() + 0.45 * hash;
            b *= 1.0 + self.ragged * h;
        }
        let mut t = self.along.0 + (self.along.1 - self.along.0) * a;
        let mut theta = self.around.0 + (self.around.1 - self.around.0) * b;
        if self.corner > 0.0 {
            // Alrededor, en metros sobre un costillar de unos 30 cm de radio.
            const REACH: f32 = 0.3;
            let (t0, t1) = (
                self.along.0.min(self.along.1),
                self.along.0.max(self.along.1),
            );
            let (v0, v1) = (
                self.around.0.min(self.around.1) * REACH,
                self.around.0.max(self.around.1) * REACH,
            );
            let r = self.corner;
            let p = Vec2::new(t, theta * REACH);
            let c = Vec2::new(p.x.clamp(t0 + r, t1 - r), p.y.clamp(v0 + r, v1 - r));
            let d = p - c;
            if d.x != 0.0 && d.y != 0.0 && d.length() > r {
                let q = c + d * (r / d.length());
                t = q.x;
                theta = q.y / REACH;
            }
        }
        (t, theta)
    }

    /// Cuánto baja la capa a `t` metros a lo largo: [`Shell::drape`] en sus
    /// extremos, nada a más de 10 cm de ellos.
    pub fn drop(&self, t: f32) -> f32 {
        if self.drape <= 0.0 {
            return 0.0;
        }
        let d = (t - self.along.0).abs().min((self.along.1 - t).abs());
        let s = (d / 0.1).clamp(0.0, 1.0);
        self.drape * (1.0 - s * s * (3.0 - 2.0 * s))
    }

    /// La capa ajustada a `horse`, y cuántos de sus rayos no lo tocaron (esos
    /// vertices se separan del eje a la distancia del impacto más cercano).
    /// Su cara exterior primero y después la interior (que solo cierra los
    /// bordes: contra el cuerpo o la capa de abajo nunca se ve).
    pub fn fit(&self, horse: &RestHorse) -> (TackMesh, usize) {
        self.fit_over(horse, None)
    }

    /// [`Shell::fit`], sobre `recado` si la capa va por encima de él
    /// ([`Shell::over`]).
    pub fn fit_over(&self, horse: &RestHorse, recado: Option<&RestHorse>) -> (TackMesh, usize) {
        let (along, around) = self.cells;
        let row = around + 1;
        let mut inner = Vec::with_capacity((along + 1) * row);
        let mut misses = 0;
        let mut last: Option<Hit> = None;
        for i in 0..=along {
            for k in 0..=around {
                let (t, theta) = self.place(i, k);
                let (origin, dir) = self.axis.ray(t, theta);
                let on_recado = recado
                    .filter(|_| self.over)
                    .and_then(|r| r.cast_far(origin, dir));
                let hit = match horse.cast(origin, dir) {
                    Some(hit) => match on_recado {
                        Some(top) if top.distance > hit.distance => top,
                        _ => hit,
                    },
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
                inner.push((hit, dir, t, theta));
            }
        }
        let mut mesh = TackMesh::default();
        let faces: &[(usize, f32)] = if self.sheet {
            &[(0, 1.0)]
        } else {
            &[(0, 1.0), (1, 0.0)]
        };
        for &(face, depth) in faces {
            for (n, (hit, dir, t, theta)) in inner.iter().enumerate() {
                let (i, k) = (n / row, n % row);
                let off = self.offset.at(*theta) - self.drop(*t);
                let bulge = if face == 0 {
                    self.shag * lump(*t, *theta)
                } else {
                    0.0
                };
                let deep = self.thickness * self.rim(*t, *theta, hit.distance + off);
                let out = off + depth * deep * (1.0 - bulge);
                mesh.positions.push(hit.point + *dir * out);
                mesh.skins.push(hit.skin);
                mesh.uvs.push(
                    self.uv
                        .at(i as f32 / along as f32, k as f32 / around as f32),
                );
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
            }
        }
        if !self.sheet {
            self.close(&mut mesh.triangles, at);
        }
        // Una capa que va hacia atrás o hacia el ángulo negativo (las del
        // lado izquierdo) da vuelta su cuadrícula: sus caras miran hacia afuera
        // igual.
        if (self.along.1 - self.along.0) * (self.around.1 - self.around.0) < 0.0 {
            for t in &mut mesh.triangles {
                t.swap(1, 2);
            }
        }
        (mesh, misses)
    }

    /// Los bordes de la capa: entre su cara exterior y la interior, alrededor
    /// de toda la cuadrícula.
    fn close(&self, tris: &mut Vec<[u32; 3]>, at: impl Fn(usize, usize, usize) -> u32) {
        let (along, around) = self.cells;
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
    }
}

/// Una argolla apoyada sobre el cuerpo con su centro en `center`, mirando
/// hacia afuera según `dir`: de `radius` de radio, con su barra de `bar` de
/// grosor. Con skinning como la piel `skin` que tiene debajo.
fn ring_at(center: Vec3, dir: Vec3, radius: f32, bar: f32, skin: Skin) -> TackMesh {
    let rotation = Quat::from_rotation_arc(Vec3::Z, dir);
    // Más lados para las argollas grandes.
    let (major, minor) = if radius > 0.012 { (12, 5) } else { (8, 4) };
    let mut mesh = TackMesh::default();
    for i in 0..=major {
        let a = i as f32 / major as f32 * TAU;
        let (sa, ca) = a.sin_cos();
        for j in 0..=minor {
            let b = j as f32 / minor as f32 * TAU;
            let (sb, cb) = b.sin_cos();
            let r = radius + bar / 2.0 * cb;
            let local = Vec3::new(r * ca, r * sa, bar / 2.0 * sb);
            mesh.positions.push(center + rotation * local);
            mesh.skins.push(skin);
            mesh.uvs
                .push([j as f32 / minor as f32, i as f32 / major as f32 * 2.0]);
        }
    }
    let row = (minor + 1) as u32;
    for i in 0..major as u32 {
        for j in 0..minor as u32 {
            let (a, b, c, d) = (
                i * row + j,
                i * row + j + 1,
                (i + 1) * row + j,
                (i + 1) * row + j + 1,
            );
            mesh.triangles.extend([[a, c, b], [b, c, d]]);
        }
    }
    mesh
}

/// Un tubo a lo largo de `path` (una curva de Catmull-Rom por sus puntos,
/// `steps` tramos entre cada par), de sección elíptica con radios `radii`:
/// el primero sobre `side` (proyectado en ángulo recto con la curva) y el
/// segundo sobre la curva × `side`. `closed`: la curva vuelve a su primer
/// punto; si no, sus extremos se tapan. Con skinning `skin`; UV: `u`
/// alrededor de la sección (de 0 a 0.5), `v` a lo largo, una vez cada 5 cm.
pub fn tube(
    path: &[Vec3],
    radii: (f32, f32),
    side: Vec3,
    sides: usize,
    steps: usize,
    closed: bool,
    skin: Skin,
) -> TackMesh {
    let n = path.len();
    let get = |i: isize| {
        if closed {
            path[i.rem_euclid(n as isize) as usize]
        } else {
            path[i.clamp(0, n as isize - 1) as usize]
        }
    };
    let segments = if closed { n } else { n - 1 };
    let mut points = Vec::new();
    for seg in 0..segments as isize {
        let (p0, p1, p2, p3) = (get(seg - 1), get(seg), get(seg + 1), get(seg + 2));
        for s in 0..steps {
            let t = s as f32 / steps as f32;
            let (t2, t3) = (t * t, t * t * t);
            points.push(
                0.5 * (2.0 * p1
                    + (p2 - p0) * t
                    + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
                    + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t3),
            );
        }
    }
    if !closed {
        points.push(path[n - 1]);
    }
    let count = points.len();
    let mut mesh = TackMesh::default();
    let mut v = 0.0;
    let rings = if closed { count + 1 } else { count };
    for r in 0..rings {
        let i = r % count;
        let (prev, next) = if closed {
            (points[(i + count - 1) % count], points[(i + 1) % count])
        } else {
            (points[i.saturating_sub(1)], points[(i + 1).min(count - 1)])
        };
        let tangent = (next - prev).normalize_or(Vec3::Y);
        let a = (side - tangent * side.dot(tangent)).normalize_or(tangent.any_orthonormal_vector());
        let b = tangent.cross(a);
        if r > 0 {
            v += points[i].distance(points[(r - 1) % count]);
        }
        for j in 0..=sides {
            let angle = j as f32 / sides as f32 * TAU;
            let (sn, cs) = angle.sin_cos();
            mesh.positions
                .push(points[i] + a * (radii.0 * cs) + b * (radii.1 * sn));
            mesh.skins.push(skin);
            mesh.uvs.push([j as f32 / sides as f32 * 0.5, v / 0.05]);
        }
    }
    let row = (sides + 1) as u32;
    for r in 0..(rings - 1) as u32 {
        for j in 0..sides as u32 {
            let (a, b, c, d) = (
                r * row + j,
                r * row + j + 1,
                (r + 1) * row + j,
                (r + 1) * row + j + 1,
            );
            mesh.triangles.extend([[a, c, b], [b, c, d]]);
        }
    }
    if !closed {
        // Una tapa en abanico en cada extremo.
        for (ring, flip) in [(0u32, true), ((rings - 1) as u32, false)] {
            let center = mesh.positions.len() as u32;
            let at = if ring == 0 {
                points[0]
            } else {
                points[count - 1]
            };
            mesh.positions.push(at);
            mesh.skins.push(skin);
            mesh.uvs.push([0.25, 0.0]);
            for j in 0..sides as u32 {
                let (a, b) = (ring * row + j, ring * row + j + 1);
                mesh.triangles
                    .push(if flip { [center, a, b] } else { [center, b, a] });
            }
        }
    }
    mesh
}

/// La unidad de la que se escala un estribo ([`STIRRUP`]).
pub const STIRRUP: Vec3 = Vec3::new(0.125, 0.15, 0.075);

/// Un estribo criollo de hierro forjado, pequeño y triangular, como los que
/// describe Hutchinson (1862): un arco de barra redonda que sube de los dos
/// extremos de la pisa hasta el ojo por donde pasa la ación, y la pisa, una
/// planchuela ovalada para la punta del pie. En una caja unitaria centrada:
/// el ancho sobre X, el alto sobre Y (el ojo arriba, en +0.5, y la pisa
/// abajo, en -0.5) y el pie pasa a lo largo de Z; [`STIRRUP`] la lleva a su
/// tamaño en metros.
pub fn stirrup() -> Mesh {
    let (w, h) = (STIRRUP.x / 2.0, STIRRUP.y / 2.0);
    let skin = Skin::default();
    let tread_y = -h + 0.006;
    let arch: Vec<Vec3> = [
        (-w + 0.0065, tread_y),
        (-w * 0.86, -h * 0.55),
        (-w * 0.74, 0.0),
        (-w * 0.46, h * 0.55),
        (-w * 0.16, h * 0.82),
        (w * 0.16, h * 0.82),
        (w * 0.46, h * 0.55),
        (w * 0.74, 0.0),
        (w * 0.86, -h * 0.55),
        (w - 0.0065, tread_y),
    ]
    .iter()
    .map(|(x, y)| Vec3::new(*x, *y, 0.0))
    .collect();
    let mut mesh = tube(&arch, (0.0055, 0.0055), Vec3::Z, 6, 2, false, skin);
    // El ojo: una presilla chata por la que pasa la ación de canto.
    let eye: Vec<Vec3> = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
        .iter()
        .map(|(x, z)| Vec3::new(x * 0.011, h - 0.006, z * 0.021))
        .collect();
    mesh.extend(tube(&eye, (0.0035, 0.0035), Vec3::Y, 4, 3, true, skin));
    // La pisa: ovalada, más ancha atrás y adelante que el arco.
    let tread: Vec<Vec3> = (0..=4)
        .map(|k| {
            Vec3::new(
                -w + 0.002 + (STIRRUP.x - 0.004) * k as f32 / 4.0,
                tread_y,
                0.0,
            )
        })
        .collect();
    mesh.extend(tube(
        &tread,
        (STIRRUP.z / 2.0, 0.0045),
        Vec3::Z,
        8,
        1,
        false,
        skin,
    ));
    let positions: Vec<[f32; 3]> = mesh
        .positions
        .iter()
        .map(|p| (*p / STIRRUP).to_array())
        .collect();
    finish_mesh(
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, mesh.uvs.clone())
        .with_inserted_indices(Indices::U32(mesh.triangles.concat())),
    )
}

/// La argolla de hierro unitaria: de 1 m de diámetro en su plano XY (mirando
/// hacia +Z), con su barra de 0.15 m de grosor.
pub fn ring() -> Mesh {
    let mesh = ring_at(Vec3::ZERO, Vec3::Z, 0.5 - 0.075, 0.15, Skin::default());
    finish_mesh(
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
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, mesh.uvs)
        .with_inserted_indices(Indices::U32(mesh.triangles.concat())),
    )
}

/// La correa unitaria: de 1 m de largo a lo largo de +Y, centrada, de 1 m de
/// ancho sobre X y `depth` de grosor sobre Z, de sección elíptica con `sides`
/// lados. Sus UV: `u` alrededor de la sección dentro de `u`, `v` a lo largo,
/// repitiendo el mapa `repeats` veces.
pub fn strap_mesh(sides: usize, depth: f32, u: (f32, f32), repeats: f32) -> Mesh {
    let mut positions = Vec::new();
    let mut uvs = Vec::new();
    for (end, y) in [(0.0, -0.5), (1.0, 0.5)] {
        for j in 0..=sides {
            let a = j as f32 / sides as f32 * TAU;
            positions.push([0.5 * a.cos(), y, 0.5 * depth * a.sin()]);
            uvs.push([u.0 + (u.1 - u.0) * j as f32 / sides as f32, end * repeats]);
        }
    }
    let row = (sides + 1) as u32;
    let mut indices = Vec::new();
    for j in 0..sides as u32 {
        indices.extend([j, j + 1, row + j, j + 1, row + j + 1, row + j]);
    }
    finish_mesh(
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(indices)),
    )
}

/// `mesh` con normales suaves y las tangentes de sus UV (para los normal
/// maps).
fn finish_mesh(mesh: Mesh) -> Mesh {
    let mut mesh = mesh.with_computed_smooth_normals();
    // Un vertex que solo tocan triángulos sin área (la punta de un mechón)
    // no tiene normal: hacia arriba, nunca NaN (se dibujaría negro).
    if let Some(VertexAttributeValues::Float32x3(normals)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_NORMAL)
    {
        for n in normals.iter_mut() {
            if !Vec3::from(*n).is_finite() || Vec3::from(*n).length_squared() < 1e-12 {
                *n = [0.0, 1.0, 0.0];
            }
        }
    }
    let tangents = tangents(&mesh);
    mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tangents);
    mesh
}

/// Las tangentes de cada vertex de `mesh` (hacia donde crece su `u`, con el
/// signo de la bitangente en `w`), sumadas de sus triángulos y hechas
/// perpendiculares a la normal.
pub fn tangents(mesh: &Mesh) -> Vec<[f32; 4]> {
    let (
        Some(VertexAttributeValues::Float32x3(p)),
        Some(VertexAttributeValues::Float32x3(n)),
        Some(VertexAttributeValues::Float32x2(uv)),
        Some(indices),
    ) = (
        mesh.attribute(Mesh::ATTRIBUTE_POSITION),
        mesh.attribute(Mesh::ATTRIBUTE_NORMAL),
        mesh.attribute(Mesh::ATTRIBUTE_UV_0),
        mesh.indices(),
    )
    else {
        return Vec::new();
    };
    let mut tan = vec![Vec3::ZERO; p.len()];
    let mut bit = vec![Vec3::ZERO; p.len()];
    let indices: Vec<usize> = indices.iter().collect();
    for t in indices.as_chunks::<3>().0 {
        let [a, b, c] = t.map(|i| Vec3::from(p[i]));
        let [ta, tb, tc] = t.map(|i| Vec2::from(uv[i]));
        let (e1, e2) = (b - a, c - a);
        let (d1, d2) = (tb - ta, tc - ta);
        let det = d1.x * d2.y - d2.x * d1.y;
        if det.abs() < 1e-12 {
            continue;
        }
        let r = 1.0 / det;
        let sdir = (e1 * d2.y - e2 * d1.y) * r;
        let tdir = (e2 * d1.x - e1 * d2.x) * r;
        for &i in t {
            tan[i] += sdir;
            bit[i] += tdir;
        }
    }
    (0..p.len())
        .map(|i| {
            let normal = Vec3::from(n[i]);
            let t = (tan[i] - normal * normal.dot(tan[i]))
                .normalize_or(normal.any_orthonormal_vector());
            let w = if normal.cross(t).dot(bit[i]) < 0.0 {
                -1.0
            } else {
                1.0
            };
            [t.x, t.y, t.z, w]
        })
        .collect()
}

/// `mesh` como una [`Mesh`] con skinning, normales suaves y tangentes: sus
/// índices de articulación apuntan a las articulaciones de la piel del
/// caballo.
pub fn skinned_mesh(mesh: &TackMesh) -> Mesh {
    finish_mesh(
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
        .with_inserted_indices(Indices::U32(mesh.triangles.concat())),
    )
}

/// Las capas del recado alrededor del costillar que se ajustan como
/// carcasas, de abajo hacia arriba, cada una más chica que la de abajo para
/// que se vea su borde: la bajera, la matra, la carona, la cincha bajo la
/// panza con el látigo de cada lado, y el cinchón, que envuelve todo por
/// encima del recado modelado (los bastos, el cojinillo y el sobrepuesto:
/// [`Recado`]).
pub fn saddle() -> Vec<Shell> {
    let layer = |piece, along, span: f32, offset, thickness, cells, uv| Shell {
        piece,
        axis: Axis::BARREL,
        along,
        around: (-span, span),
        offset,
        thickness,
        shag: 0.0,
        round: 0.0,
        sheet: false,
        drape: 0.0,
        corner: 0.0,
        ragged: 0.0,
        over: false,
        uv,
        cells,
    };
    vec![
        // Las mantas: de canto grueso (una manta de lana doblada), apenas
        // redondeado, con las esquinas caídas en curva.
        Shell {
            round: 0.004,
            corner: 0.12,
            drape: 0.004,
            ..layer(
                HorsePiece::Bajera,
                (-0.3, 0.33),
                1.75,
                Offset::ramp(0.014, 1.2, 0.55, 0.016),
                0.012,
                (14, 26),
                Uv::new((0.0, 0.5), (0.0, 1.0)),
            )
        },
        Shell {
            round: 0.004,
            corner: 0.1,
            ..layer(
                HorsePiece::Matra,
                (-0.27, 0.3),
                1.66,
                Offset::ramp(0.03, 1.2, 0.46, 0.03),
                0.016,
                (14, 24),
                Uv::new((0.5, 1.0), (0.0, 1.0)),
            )
        },
        Shell {
            round: 0.012,
            corner: 0.07,
            ..layer(
                HorsePiece::Carona,
                (-0.23, 0.27),
                1.5,
                Offset::ramp(0.048, 1.2, 0.3, 0.05),
                0.008,
                (10, 22),
                Uv::new((0.0, 1.0), (0.0, 1.0)),
            )
        },
        Shell {
            around: (ARGOLLA.1, TAU - ARGOLLA.1),
            round: 0.004,
            ..layer(
                HorsePiece::Cincha,
                (-0.045, 0.065),
                0.0,
                Offset::knots([(0.0, 0.064), (1.55, 0.064), (1.72, 0.05), (1.95, 0.016)]),
                0.006,
                (3, 36),
                Uv::new((0.75, 1.0), (0.0, 1.0)),
            )
        },
        latigo(1.0),
        latigo(-1.0),
        // Sobre el recado, apenas por encima de su cara de arriba; por
        // debajo del cojinillo, por encima de la cincha.
        Shell {
            around: (-PI, PI),
            round: 0.003,
            over: true,
            ..layer(
                HorsePiece::Cinchon,
                (-0.008, 0.028),
                0.0,
                Offset::knots([
                    (0.0, 0.003),
                    (1.36, 0.003),
                    (1.42, 0.076),
                    (1.6, 0.072),
                    (2.05, 0.025),
                ]),
                0.005,
                (1, 48),
                Uv::new((0.5, 0.75), (0.0, 1.5)),
            )
        },
    ]
}

/// El látigo de un lado: la tira de cuero crudo que sube desde la argolla
/// de la cincha, por debajo del pellón, hasta los bastos.
fn latigo(side: f32) -> Shell {
    Shell {
        around: (side * 1.0, side * (ARGOLLA.1 + 0.03)),
        thickness: 0.005,
        round: 0.003,
        ..layer_shell(
            HorsePiece::Cincha,
            (-0.005, 0.025),
            Offset::knots([(0.0, 0.058), (1.4, 0.058), (1.5, 0.064)]),
            (1, 6),
            Uv::new((0.5, 0.75), (0.0, 0.3)),
        )
    }
}

/// Una capa del costillar de `piece` sin bordes redondeados, para armar las
/// del vellón.
fn layer_shell(
    piece: HorsePiece,
    along: (f32, f32),
    offset: Offset,
    cells: (usize, usize),
    uv: Uv,
) -> Shell {
    Shell {
        piece,
        axis: Axis::BARREL,
        along,
        around: (0.0, 0.0),
        offset,
        thickness: 0.0,
        shag: 0.0,
        round: 0.0,
        sheet: false,
        drape: 0.0,
        corner: 0.0,
        ragged: 0.0,
        over: false,
        uv,
        cells,
    }
}

/// Dónde están, a lo largo del costillar y alrededor de él, las argollas de
/// la cincha: en sus extremos, a cada lado.
pub const ARGOLLA: (f32, f32) = (0.01, 1.45);

/// El rayo desde dentro del hocico hacia la comisura de la boca del lado `s`
/// (-1 izquierda, 1 derecha), donde va una argolla del freno. La cabeza está
/// centrada en la columna (la de Quaternius estaba 1.6 cm a la derecha).
pub fn bit_ray(s: f32) -> (Vec3, Vec3) {
    (Vec3::new(1.33, 1.48, 0.0), Vec3::Z * s)
}

/// El rayo desde dentro del cuello hacia su costado, un palmo por debajo de la
/// crin, del lado `s`: ahí las riendas se apoyan a lo largo del cuello. Bastante
/// de costado para que la rienda desde la quijada no atraviese la parte alta
/// del cuello, más llena en el caballo propio que en el de Quaternius.
pub fn neck_ray(s: f32) -> (Vec3, Vec3) {
    (
        Vec3::new(0.7, 1.45, 0.0),
        Vec3::new(0.0, 1.0, 0.75 * s).normalize(),
    )
}

/// El rayo desde dentro de la cabeza hacia la quijada del lado `s`, donde la
/// rienda vuelve desde el freno hacia las manos del jinete, sin tocar la
/// mejilla.
pub fn jowl_ray(s: f32) -> (Vec3, Vec3) {
    (
        Vec3::new(1.12, 1.62, 0.0),
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
    /// Con el `recado` modelado puesto, las riendas salen de las argollas de
    /// su freno, el jinete se sienta sobre su sobrepuesto y las riendas
    /// descansan y las aciones salen sobre su cojinillo; sin él, a las
    /// alturas de [`SEAT_RISE`] sobre el lomo.
    pub fn find(horse: &RestHorse, recado: Option<&Recado>) -> Option<Self> {
        let barrel = Axis::BARREL;
        let side = |f: &dyn Fn(f32) -> Option<Vec3>| Some([f(-1.0)?, f(1.0)?]);
        let out = |(origin, dir): (Vec3, Vec3), by: f32| {
            horse.cast(origin, dir).map(|h| h.point + dir * by)
        };
        // Un poco separadas de la piel: el cuello se arquea en las esperas.
        let jowl = side(&|s| out(jowl_ray(s), 0.04))?;
        let neck = side(&|s| out(neck_ray(s), 0.05))?;
        // Sobre la cara de arriba del recado, o a la altura del asiento sobre
        // el lomo sin él.
        let top = |x: f32, theta: f32, extra: f32| {
            let (origin, dir) = barrel.ray(x, theta);
            let skin = horse.cast(origin, dir)?;
            match recado.and_then(|r| r.surface.cast_far(origin, dir)) {
                Some(hit) if hit.distance > skin.distance => Some(hit.point + dir * extra),
                _ => Some(skin.point + dir * (SEAT_RISE + extra)),
            }
        };
        let pommel = side(&|s| top(0.2, s * 0.3, 0.01))?;
        let leather = side(&|s| top(0.08, s * 1.0, 0.012))?;
        let (bit, seat) = match recado {
            Some(r) => (r.reins, r.seat),
            None => (side(&|s| out(bit_ray(s), 0.03))?, top(SEAT_X, 0.0, 0.0)?),
        };
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

/// Cuánto más arriba que el lomo se sienta el jinete sin el recado
/// modelado: lo que mide su asiento (`tools/recado.py`).
pub const SEAT_RISE: f32 = 0.112;

/// Dónde vive el recado modelado, bajo el [`AssetRoot`].
pub const RECADO_PATH: &str = "models/tack/recado.glb";

/// El recado modelado en Blender (`tools/recado.py`), en el marco del cuerpo
/// del caballo en reposo: sus piezas con skinning a las articulaciones de su
/// piel (cada una con su nombre), su cara de arriba (los bastos, el
/// cojinillo y el sobrepuesto) para ajustar el cinchón y los accesorios
/// encima, y sus marcas: dónde salen las riendas y dónde se sienta el jinete.
#[derive(Debug, Clone)]
pub struct Recado {
    pub pieces: Vec<(HorsePiece, String, TackMesh)>,
    pub surface: RestHorse,
    /// Las argollas de las riendas del freno, primero la izquierda.
    pub reins: [Vec3; 2],
    pub seat: Vec3,
}

/// Qué pieza es cada mesh del recado, por su nombre (`None`: no se dibuja).
pub fn recado_piece(name: &str) -> Option<HorsePiece> {
    Some(match name {
        "Bastos" => HorsePiece::Basto,
        "Cojinillo" => HorsePiece::Pellon,
        "Sobrepuesto" => HorsePiece::Sobrepuesto,
        "Vellon" => HorsePiece::Vellon,
        "Freno" | "Bocado" => HorsePiece::Bit,
        "Testera" | "Frontalera" | "Quijera_L" | "Quijera_R" | "Muserola" | "Fiador"
        | "Barbada" => HorsePiece::Headstall,
        _ => return None,
    })
}

impl Recado {
    /// El recado a partir de sus meshes `(nombre, mesh en el marco del
    /// cuerpo)`, o `None` si le falta alguna de sus marcas.
    pub fn new(meshes: Vec<(String, TackMesh)>) -> Option<Self> {
        let mark = |name: &str| {
            let (_, m) = meshes.iter().find(|(n, _)| n == name)?;
            (!m.positions.is_empty())
                .then(|| m.positions.iter().sum::<Vec3>() / m.positions.len() as f32)
        };
        let reins = [mark("Mark_ReinL")?, mark("Mark_ReinR")?];
        let seat = mark("Mark_Seat")?;
        let mut surface = TackMesh::default();
        let mut pieces = Vec::new();
        for (name, mesh) in meshes {
            let Some(piece) = recado_piece(&name) else {
                continue;
            };
            if matches!(
                piece,
                HorsePiece::Basto | HorsePiece::Pellon | HorsePiece::Sobrepuesto
            ) {
                surface.extend(mesh.clone());
            }
            pieces.push((piece, name, mesh));
        }
        Some(Self {
            pieces,
            surface: RestHorse::from(&surface),
            reins,
            seat,
        })
    }
}

/// Cada carcasa y argolla de los aperos ajustada a `horse` (el cinchón, por
/// encima del `recado`), y los rayos que no lo tocaron.
pub fn fit_tack(
    horse: &RestHorse,
    recado: Option<&Recado>,
) -> (Vec<(HorsePiece, TackMesh)>, usize) {
    let mut out = Vec::new();
    let mut misses = 0;
    for shell in saddle() {
        let (mesh, missed) = shell.fit_over(horse, recado.map(|r| &r.surface));
        misses += missed;
        out.push((shell.piece, mesh));
    }
    for s in [-1.0, 1.0] {
        let (along, theta) = ARGOLLA;
        let (origin, dir) = Axis::BARREL.ray(along, s * theta);
        match horse.cast(origin, dir) {
            Some(hit) => out.push((
                HorsePiece::Argolla,
                ring_at(hit.point + dir * 0.074, dir, 0.026, 0.007, hit.skin),
            )),
            None => misses += 1,
        }
    }
    (out, misses)
}

/// Cuántos triángulos pueden dibujar los aperos de un caballo ensillado:
/// las carcasas, el recado modelado, las correas y los estribos.
pub const TACK_BUDGET: usize = 13_000;

/// Los materiales de los aperos, uno por pieza, compartidos.
#[derive(Debug, Clone, Default)]
pub struct TackPaints(pub HashMap<HorsePiece, Handle<StandardMaterial>>);

impl TackPaints {
    pub fn of(&self, piece: HorsePiece) -> Handle<StandardMaterial> {
        self.0.get(&piece).cloned().unwrap_or_default()
    }
}

/// Las correas de los aperos: un tramo de rienda por cada tramo (del freno a
/// la quijada, de la quijada al cuello y del cuello a la mano o la montura) y
/// la ación, cada una con su trenza o su suela repetidas según su largo.
#[derive(Debug, Clone)]
pub struct TackStraps {
    pub reins: [Handle<Mesh>; 3],
    pub leather: Handle<Mesh>,
}

/// Los aperos construidos una vez y compartidos por todos los caballos.
#[derive(Debug, Clone)]
pub struct TackKit {
    /// Cada carcasa y argolla y cada pieza del recado modelado, con
    /// skinning, con su nombre y su material.
    pub pieces: Vec<(HorsePiece, String, Handle<Mesh>, Handle<StandardMaterial>)>,
    /// Las poses de bind inversas propias de los aperos: el reposo en el marco
    /// del cuerpo de cada articulación del caballo, invertido (sus vertices
    /// están en el marco del cuerpo).
    pub bindposes: Handle<SkinnedMeshInverseBindposes>,
    pub straps: TackStraps,
    pub stirrup: Handle<Mesh>,
    pub paints: TackPaints,
    pub fittings: Fittings,
}

/// Qué tan gruesas son las riendas (redondas, trenzadas) y qué tan anchas y
/// gruesas las aciones (planas), en metros, y los lados de sus secciones.
pub const REIN: f32 = 0.012;
pub const LEATHER: f32 = 0.032;
pub const LEATHER_DEPTH: f32 = 0.2;
const REIN_SIDES: usize = 6;
const LEATHER_SIDES: usize = 8;

/// Una pieza recta de los aperos entre dos puntos que se separan: cada extremo
/// es un punto en el espacio propio de una entidad. La estira
/// [`stretch_straps`]; una correa `flat` pone su ancho a lo largo del frente
/// del caballo.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Strap {
    pub ends: [(Entity, Vec3); 2],
    /// Ancho, en metros.
    pub width: f32,
    pub flat: bool,
    /// Cuánto cuelga en el medio, como fracción de su largo: una rienda de
    /// cuero crudo floja. Una correa que cuelga no se dibuja ella misma sino
    /// en sus [`StrapPart`] a lo largo de la curva ([`strap_curve`]).
    pub sag: f32,
}

/// Un tramo recto `k` de los `n` de una correa que cuelga (su padre).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct StrapPart {
    pub k: usize,
    pub n: usize,
}

/// En cuántos tramos rectos se dibuja una correa que cuelga.
pub const SAG_PARTS: usize = 5;

/// Cuánto cuelgan las riendas: de la quijada al cuello, y del cuello a la
/// mano del jinete o a la montura (del freno a la quijada van tirantes).
pub const REIN_SAG: [f32; 3] = [0.0, 0.0, 0.07];

/// Los `n + 1` puntos de una correa de `a` a `b` que cuelga `sag` (de su
/// largo) en el medio: una parábola hacia abajo, como una catenaria poco
/// profunda.
pub fn strap_curve(a: Vec3, b: Vec3, sag: f32, n: usize) -> Vec<Vec3> {
    let drop = sag * a.distance(b);
    (0..=n)
        .map(|k| {
            let t = k as f32 / n as f32;
            a.lerp(b, t) + Vec3::NEG_Y * (drop * 4.0 * t * (1.0 - t))
        })
        .collect()
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
    server: Option<Res<'w, AssetServer>>,
    root: Option<Res<'w, AssetRoot>>,
    support: Option<Res<'w, CompressedImageFormatSupport>>,
    gltf: GltfStores<'w>,
    recado: Option<Res<'w, RecadoModel>>,
}

/// Los assets de glTF de los que se lee el recado modelado.
type GltfStores<'w> = (
    Option<Res<'w, Assets<Gltf>>>,
    Option<Res<'w, Assets<GltfMesh>>>,
    Option<Res<'w, Assets<GltfSkin>>>,
    Option<Res<'w, Assets<GltfNode>>>,
);

/// El archivo del recado modelado, en carga ([`RECADO_PATH`]); `None` sin un
/// servidor de assets o sin el archivo.
#[derive(Resource, Debug, Clone, Default)]
pub struct RecadoModel(pub Option<Handle<Gltf>>);

/// Empieza a cargar el recado modelado.
pub(super) fn load_recado(
    mut commands: Commands,
    server: Option<Res<AssetServer>>,
    root: Option<Res<AssetRoot>>,
) {
    let handle = match (server, root) {
        (Some(server), Some(root)) if root.0.join(RECADO_PATH).is_file() => {
            Some(server.load(RECADO_PATH))
        }
        (Some(_), Some(_)) => {
            warn!("{RECADO_PATH} not found: saddled horses without bastos, cojinillo nor bridle");
            None
        }
        _ => None,
    };
    commands.insert_resource(RecadoModel(handle));
}

/// Las escenas de caballos preparadas que esperan sus aperos, cada una con
/// los transforms de sus nodos en reposo tal como estaban al prepararse (sus
/// clips ya los mueven cuando se visten): se visten cuando el recado
/// modelado terminó de cargar (o no está).
#[derive(Resource, Debug, Clone, Default)]
pub struct PendingTack(pub Vec<(Entity, HashMap<Entity, Transform>)>);

/// La ubicación de `entity` en el espacio de `root` según los transforms en
/// reposo de `rest` (como [`rest_affine`], sin la pose de este frame).
fn rest_in(
    entity: Entity,
    root: Entity,
    parents: &Query<&ChildOf>,
    rest: &HashMap<Entity, Transform>,
) -> Affine3A {
    let mut affine = Affine3A::IDENTITY;
    let mut at = entity;
    loop {
        if let Some(t) = rest.get(&at) {
            affine = t.compute_affine() * affine;
        }
        if at == root {
            break;
        }
        let Ok(child_of) = parents.get(at) else {
            break;
        };
        at = child_of.parent();
    }
    affine
}

/// El recado modelado, en el marco del cuerpo del caballo en reposo, a
/// partir de su archivo: cada mesh llevada a las articulaciones del caballo
/// del mismo nombre (`names`, con su reposo `rest` en el marco del cuerpo).
/// `None` si el archivo no cargó o no encaja con el caballo.
fn recado_on(stores: &TackStores, names: &[String], rest: &[Affine3A]) -> Option<Recado> {
    let handle = stores.recado.as_ref()?.0.as_ref()?;
    let (gltfs, meshes, skins, nodes) = &stores.gltf;
    let (gltfs, meshes, skins, nodes) = (
        gltfs.as_ref()?,
        meshes.as_ref()?,
        skins.as_ref()?,
        nodes.as_ref()?,
    );
    let gltf = gltfs.get(handle)?;
    let skin = skins.get(gltf.skins.first()?)?;
    let binds = stores
        .bindposes
        .as_ref()?
        .get(&skin.inverse_bind_matrices)?;
    let mut matrices = Vec::new();
    let mut remap = Vec::new();
    for (joint, ibm) in skin.joints.iter().zip(binds.iter()) {
        let name = &nodes.get(joint)?.name;
        let Some(k) = names.iter().position(|n| n == name) else {
            warn!("the recado's joint {name} is not the horse's");
            return None;
        };
        matrices.push(rest[k] * Affine3A::from_mat4(*ibm));
        remap.push(k as u16);
    }
    let mut out = Vec::new();
    for handle in &gltf.meshes {
        let gm = meshes.get(handle)?;
        for primitive in &gm.primitives {
            let mesh = stores.meshes.get(&primitive.mesh)?;
            out.push((gm.name.clone(), rest_mesh(mesh, &matrices, &remap)?));
        }
    }
    Recado::new(out)
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
    /// El caballo en reposo sobre el que se ajustaron (para medirlos).
    pub rest: TackRest,
}

/// El caballo en reposo, en el marco del cuerpo: su piel (sin crin ni ojos),
/// el blanco de sus ojos y las puntas de sus orejas.
#[derive(Debug, Clone, Default)]
pub struct TackRest {
    pub skin: RestHorse,
    pub eyes: Vec<Vec3>,
    pub ear_tips: Vec<Vec3>,
}

/// Los aperos construidos (una vez, a partir del primer caballo preparado).
#[derive(Resource, Debug, Clone, Default)]
pub struct TackBuilt(pub Option<HorseTack>);

/// Construye el kit de aperos a partir de la escena de un caballo bajo `root`:
/// su piel en reposo en el marco del cuerpo, las carcasas ajustadas a ella y
/// las poses de bind inversas que llevan los vertices del marco del cuerpo a
/// sus articulaciones. `None` si al modelo le falta su piel o sus huesos.
fn build_tack(
    scene: &TackScene,
    stores: &mut TackStores,
    root: Entity,
    at_rest: &HashMap<Entity, Transform>,
    joints: &[Entity],
    bindposes: &Handle<SkinnedMeshInverseBindposes>,
) -> Option<HorseTack> {
    let rest: Vec<Affine3A> = joints
        .iter()
        .map(|j| rest_in(*j, root, &scene.parents, at_rest))
        .collect();
    let inverse_binds = stores.bindposes.as_ref()?.get(bindposes)?;
    let skin: Vec<Affine3A> = rest
        .iter()
        .zip(inverse_binds.iter())
        .map(|(r, ibm)| *r * Affine3A::from_mat4(*ibm))
        .collect();
    let mut horse = RestHorse::default();
    let mut measured = TackRest::default();
    for entity in scene.children.iter_descendants(root) {
        let Ok((_, mesh, name)) = scene.skins.get(entity) else {
            continue;
        };
        let Some(mesh) = stores.meshes.get(&mesh.0) else {
            continue;
        };
        let name = name.map_or("", |n| n.0.as_str());
        // Los ojos sobresalen de la cara; los aperos los rodean.
        if name.starts_with("Eye") {
            if name == "Eye_White" {
                let mut eye = RestHorse::default();
                eye.add(mesh, &skin);
                measured.eyes.extend(eye.positions);
            }
            continue;
        }
        horse.add(mesh, &skin);
        if ["Main", "Main_Dark", "Main_Light", "Muzzle", "Hooves"].contains(&name) {
            measured.skin.add(mesh, &skin);
        }
    }
    for ear in ["Ear4.L", "Ear4.R"] {
        if let Some(e) = scene
            .children
            .iter_descendants(root)
            .find(|e| scene.names.get(*e).is_ok_and(|n| n.as_str() == ear))
        {
            let tip = rest_in(e, root, &scene.parents, at_rest);
            measured.ear_tips.push(tip.translation.into());
        }
    }
    let names: Vec<String> = joints
        .iter()
        .map(|j| {
            scene
                .names
                .get(*j)
                .map(|n| n.to_string())
                .unwrap_or_default()
        })
        .collect();
    let recado = recado_on(stores, &names, &rest);
    if recado.is_none() {
        warn!("the horse's tack has no modelled recado");
    }
    let (fitted, misses) = fit_tack(&horse, recado.as_ref());
    if misses > 0 {
        warn!("the horse's tack missed its body {misses} times");
    }
    let fittings = Fittings::find(&horse, recado.as_ref())?;
    let bone = |b: HorseBone| {
        scene
            .children
            .iter_descendants(root)
            .find(|e| scene.names.get(*e).is_ok_and(|n| n.as_str() == b.name()))
            .map(|e| rest_in(e, root, &scene.parents, at_rest))
    };
    let bones = [
        bone(HorseBone::Torso)?,
        bone(HorseBone::Neck2)?,
        bone(HorseBone::Head)?,
    ];
    let inverse: Vec<Mat4> = rest.iter().map(|r| Mat4::from(r.inverse())).collect();
    let paints = tack_paints(stores);
    let modelled = recado.into_iter().flat_map(|r| r.pieces);
    let pieces = fitted
        .into_iter()
        .map(|(piece, mesh)| (piece, format!("{piece:?}"), mesh))
        .chain(modelled)
        .map(|(piece, name, mesh)| {
            let handle = stores.meshes.add(skinned_mesh(&mesh));
            (piece, name, handle, paints.of(piece))
        })
        .collect();
    let f = &fittings;
    let length = |a: [Vec3; 2], b: [Vec3; 2]| a[0].distance(b[0]);
    // La trenza de las riendas: un chevrón cada ~1.4 cm (16 por vuelta del
    // mapa); la suela de las aciones, sin repetirse a lo largo.
    // Una rienda que cuelga se dibuja en tramos: cada uno, su parte.
    let rein = |metres: f32, sag: f32| {
        let parts = if sag > 0.0 { SAG_PARTS } else { 1 };
        strap_mesh(REIN_SIDES, 1.0, (0.0, 0.5), metres / 0.22 / parts as f32)
    };
    let straps = TackStraps {
        reins: [
            stores.meshes.add(rein(length(f.bit, f.jowl), REIN_SAG[0])),
            stores.meshes.add(rein(length(f.jowl, f.neck), REIN_SAG[1])),
            stores
                .meshes
                .add(rein(length(f.neck, f.pommel), REIN_SAG[2])),
        ],
        leather: stores
            .meshes
            .add(strap_mesh(LEATHER_SIDES, LEATHER_DEPTH, (0.2, 0.8), 0.6)),
    };
    let kit = TackKit {
        pieces,
        bindposes: stores.bindposes.as_mut()?.add(inverse),
        straps,
        stirrup: stores.meshes.add(stirrup()),
        paints,
        fittings,
    };
    Some(HorseTack {
        local: local_fittings(&kit.fittings, bones),
        torso: bones[0],
        kit,
        rest: measured,
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
    roots: Query<(), With<HorseModel>>,
    children: Query<&Children>,
    transforms: Query<&Transform>,
    mut pending: ResMut<PendingTack>,
) {
    let root = ready.entity;
    if !roots.contains(root) {
        return;
    }
    let rest = std::iter::once(root)
        .chain(children.iter_descendants(root))
        .filter_map(|e| transforms.get(e).ok().map(|t| (e, *t)))
        .collect();
    pending.0.retain(|(r, _)| *r != root);
    pending.0.push((root, rest));
}

/// Viste las escenas de caballos que esperan sus aperos ([`PendingTack`]),
/// una vez que el recado modelado terminó de cargar o falló.
pub(super) fn dress_pending(
    mut commands: Commands,
    mut stores: TackStores,
    scene: TackScene,
    roots: Query<&ChildOf, With<HorseModel>>,
    (pieces, worn): (Query<(), With<HorsePiece>>, Query<&Tack>),
    mut pending: ResMut<PendingTack>,
    mut built: ResMut<TackBuilt>,
) {
    if pending.0.is_empty() {
        return;
    }
    let loading = match (
        &stores.server,
        stores.recado.as_ref().and_then(|r| r.0.as_ref()),
    ) {
        (Some(server), Some(handle)) => {
            !server.is_loaded_with_dependencies(handle) && !server.load_state(handle).is_failed()
        }
        _ => false,
    };
    if loading && built.0.is_none() {
        return;
    }
    for (root, rest) in std::mem::take(&mut pending.0) {
        dress(
            &mut commands,
            &mut stores,
            &scene,
            &roots,
            (&pieces, &worn),
            &mut built.0,
            root,
            &rest,
        );
    }
}

/// Pone los aperos (construidos la primera vez) en la escena del caballo
/// `root`; ver [`dress_tack`].
#[allow(clippy::too_many_arguments)]
fn dress(
    commands: &mut Commands,
    stores: &mut TackStores,
    scene: &TackScene,
    roots: &Query<&ChildOf, With<HorseModel>>,
    (pieces, worn): (&Query<(), With<HorsePiece>>, &Query<&Tack>),
    tack: &mut Option<HorseTack>,
    root: Entity,
    rest: &HashMap<Entity, Transform>,
) {
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
            scene,
            stores,
            root,
            rest,
            &skin.joints,
            &skin.inverse_bindposes,
        );
    }
    let Some(tack) = tack.as_ref() else {
        warn!("the horse's tack could not be fitted");
        commands.entity(carrier).try_remove::<Tack>();
        return;
    };
    put_on_tack(commands, tack, carrier, &skin, [torso, neck, head]);
}

/// Pone los aperos compartidos en el caballo `carrier`: las carcasas con
/// skinning en las articulaciones de su `skin`, los estribos colgando del torso
/// y las correas (en el orden torso, cuello, cabeza: `bones`).
fn put_on_tack(
    commands: &mut Commands,
    tack: &HorseTack,
    carrier: Entity,
    skin: &SkinnedMesh,
    [torso, neck, head]: [Entity; 3],
) {
    let range = model_range();
    let kit = &tack.kit;
    let local = &tack.local;
    for (piece, name, mesh, paint) in &kit.pieces {
        let mut entity = commands.spawn((
            Name::new(name.clone()),
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
        // Los mechones sueltos no echan sombra: sobre la lana se verían como
        // agujeros negros.
        if *piece == HorsePiece::Vellon {
            entity.insert(bevy::light::NotShadowCaster);
        }
    }
    let paint = |piece| kit.paints.of(piece);
    let spawn_strap =
        |commands: &mut Commands, piece, ends, width, sag: f32, mesh: &Handle<Mesh>| {
            let strap = Strap {
                ends,
                width,
                flat: piece == HorsePiece::StirrupLeather,
                sag,
            };
            let mut entity = commands.spawn((
                Name::new(format!("{piece:?}")),
                piece,
                strap,
                Transform::default(),
                Visibility::default(),
                range.clone(),
                ChildOf(carrier),
            ));
            if sag > 0.0 {
                for k in 0..SAG_PARTS {
                    entity.with_child((
                        Name::new(format!("{piece:?}")),
                        StrapPart { k, n: SAG_PARTS },
                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(paint(piece)),
                        Transform::default(),
                        range.clone(),
                    ));
                }
            } else {
                entity.insert((Mesh3d(mesh.clone()), MeshMaterial3d(paint(piece))));
            }
            entity.id()
        };
    let mut reins = [Entity::PLACEHOLDER; 2];
    let mut stirrups = [Entity::PLACEHOLDER; 2];
    let [bit, jowl, neck_rein] = &kit.straps.reins;
    for s in 0..2 {
        spawn_strap(
            commands,
            HorsePiece::Rein,
            [(head, local.bit[s]), (head, local.jowl[s])],
            REIN,
            REIN_SAG[0],
            bit,
        );
        spawn_strap(
            commands,
            HorsePiece::Rein,
            [(head, local.jowl[s]), (neck, local.neck[s])],
            REIN,
            REIN_SAG[1],
            jowl,
        );
        reins[s] = spawn_strap(
            commands,
            HorsePiece::Rein,
            [(neck, local.neck[s]), (torso, local.pommel[s])],
            REIN,
            REIN_SAG[2],
            neck_rein,
        );
        stirrups[s] = commands
            .spawn((
                Name::new("Stirrup"),
                HorsePiece::Stirrup,
                Mesh3d(kit.stirrup.clone()),
                MeshMaterial3d(paint(HorsePiece::Stirrup)),
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
            0.0,
            &kit.straps.leather,
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

/// La ubicación en el mundo de una correa plana desde `a` hasta `b`, de
/// `width` de ancho: su ancho (la X de la correa unitaria) tan cerca de
/// `forward` como lo deja el largo, así que su cara (la Z) mira de costado.
pub fn flat_strap_between(a: Vec3, b: Vec3, width: f32, forward: Vec3) -> GlobalTransform {
    let along = b - a;
    let y = along.normalize_or(Vec3::Y);
    let x = (forward - y * forward.dot(y)).normalize_or(y.any_orthonormal_vector());
    let rotation = Quat::from_mat3(&Mat3::from_cols(x, y, x.cross(y)));
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
    mut straps: Query<
        (
            &Strap,
            &ChildOf,
            &mut Transform,
            &mut GlobalTransform,
            &mut Visibility,
        ),
        Without<StrapPart>,
    >,
    mut parts: Query<(&StrapPart, &ChildOf, &mut Transform, &mut GlobalTransform), Without<Strap>>,
    places: Query<&GlobalTransform, (Without<Strap>, Without<StrapPart>)>,
) {
    let ends = |strap: &Strap, parent: Entity| {
        let [(a, at_a), (b, at_b)] = strap.ends;
        let (a, b, parent) = (
            places.get(a).ok()?,
            places.get(b).ok()?,
            places.get(parent).ok()?,
        );
        Some((a.transform_point(at_a), b.transform_point(at_b), *parent))
    };
    for (part, child_of, mut transform, mut global) in &mut parts {
        let Ok((strap, strap_of, ..)) = straps.get(child_of.parent()) else {
            continue;
        };
        let Some((a, b, parent)) = ends(strap, strap_of.parent()) else {
            continue;
        };
        let curve = strap_curve(a, b, strap.sag, part.n);
        // Un poco más largo que su tramo, para que los tramos se solapen en
        // los codos.
        let (p, q) = (curve[part.k], curve[part.k + 1]);
        let over = (q - p) * 0.08;
        let placed = strap_between(p - over, q + over, strap.width);
        *global = placed;
        *transform =
            Transform::from_matrix(Mat4::from(parent.affine().inverse() * placed.affine()));
    }
    for (strap, child_of, mut transform, mut global, mut shown) in &mut straps {
        if strap.sag > 0.0 {
            // La correa misma se queda en su padre; la dibujan sus tramos.
            let Some((_, _, parent)) = ends(strap, child_of.parent()) else {
                shown.set_if_neq(Visibility::Hidden);
                continue;
            };
            shown.set_if_neq(Visibility::Inherited);
            *transform = Transform::IDENTITY;
            *global = parent;
            continue;
        }
        let [(a, at_a), (b, at_b)] = strap.ends;
        let (Ok(a), Ok(b), Ok(parent)) =
            (places.get(a), places.get(b), places.get(child_of.parent()))
        else {
            shown.set_if_neq(Visibility::Hidden);
            continue;
        };
        shown.set_if_neq(Visibility::Inherited);
        let (a, b) = (a.transform_point(at_a), b.transform_point(at_b));
        let placed = if strap.flat {
            flat_strap_between(a, b, strap.width, *parent.right())
        } else {
            strap_between(a, b, strap.width)
        };
        *global = placed;
        *transform =
            Transform::from_matrix(Mat4::from(parent.affine().inverse() * placed.affine()));
    }
}

/// El material liso de `piece`, sin mapas.
pub fn material(
    piece: HorsePiece,
    palette: &mut Palette,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) -> Handle<StandardMaterial> {
    let (color, finish, _) = piece.look();
    palette.material(color, finish, materials, images)
}

/// Los mapas en carga de un conjunto.
#[derive(Debug, Clone)]
pub struct TackMaps {
    pub color: Handle<Image>,
    pub normal: Handle<Image>,
    pub orm: Handle<Image>,
}

/// `piece` vestida con los mapas de su conjunto: su color, su relieve, su
/// oclusión y su rugosidad salen de los mapas (y su metalicidad, del hierro).
/// La lana larga, con alpha test y por ambos lados; el cuero del pellón, sin
/// los huecos entre sus mechones; las mantas, con alpha test por los flecos
/// de la matra.
pub fn textured(piece: HorsePiece, maps: &TackMaps) -> StandardMaterial {
    let (_, finish, set) = piece.look();
    let (_, metallic, reflectance) = finish.pbr();
    let wool = piece == HorsePiece::Vellon;
    let masked = wool || set == TackSet::Blankets;
    StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(maps.color.clone()),
        normal_map_texture: Some(maps.normal.clone()),
        metallic_roughness_texture: Some(maps.orm.clone()),
        occlusion_texture: Some(maps.orm.clone()),
        perceptual_roughness: 1.0,
        metallic,
        reflectance,
        alpha_mode: if masked {
            AlphaMode::Mask(0.5)
        } else {
            AlphaMode::Opaque
        },
        double_sided: wool,
        cull_mode: if wool {
            None
        } else {
            Some(bevy::render::render_resource::Face::Back)
        },
        ..default()
    }
}

/// Empieza a cargar los conjuntos de los aperos que tienen todos sus archivos
/// (sin un servidor de assets, una raíz o una GPU con formatos comprimidos, a
/// los que se transcodifican los KTX2, ninguno).
fn load_sets(stores: &TackStores) -> HashMap<TackSet, TackMaps> {
    let mut sets = HashMap::default();
    let (Some(server), Some(root)) = (&stores.server, &stores.root) else {
        return sets;
    };
    if stores.support.as_ref().is_some_and(|s| s.0.is_empty()) {
        return sets;
    }
    for set in TackSet::ALL {
        let paths = MAPS.map(|(name, srgb)| (set.path(name), srgb));
        if let Some((missing, _)) = paths
            .iter()
            .find(|(p, _)| !Path::new(&root.0).join(p).is_file())
        {
            warn!("tack maps {missing} not found; drawing plain colour");
            continue;
        }
        let [color, normal, orm] = paths.map(|(path, srgb)| load_map(server, &path, srgb));
        sets.insert(set, TackMaps { color, normal, orm });
    }
    sets
}

/// El material de cada pieza: con los mapas de su conjunto, o liso si no se
/// cargaron.
fn tack_paints(stores: &mut TackStores) -> TackPaints {
    let sets = load_sets(stores);
    let mut paints = TackPaints::default();
    for piece in HorsePiece::ALL {
        let (_, _, set) = piece.look();
        let paint = match sets.get(&set) {
            Some(maps) => stores.materials.add(textured(piece, maps)),
            None => material(
                piece,
                &mut stores.palette,
                &mut stores.materials,
                &mut stores.images,
            ),
        };
        paints.0.insert(piece, paint);
    }
    paints
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
    fn a_shell_lies_its_offset_off_the_body_its_thickness_deep_with_rounded_edges() {
        let horse = tube_horse();
        let shell = saddle()[0];
        let (mesh, misses) = shell.fit(&horse);
        assert_eq!(misses, 0);
        let (along, around) = shell.cells;
        let row = around + 1;
        let face = (along + 1) * row;
        assert_eq!(mesh.positions.len(), 2 * face);
        let (mut middle, mut rim) = (0, 0);
        for k in 0..face {
            let (outer, inner) = (mesh.positions[k], mesh.positions[face + k]);
            let off = |p: Vec3| Vec2::new(p.y - 1.14, p.z).length() - 0.28;
            assert!(
                (off(inner) - shell.offset.at(0.0)).abs() < 0.012,
                "{} at {inner}",
                off(inner)
            );
            let deep = outer.distance(inner);
            let (i, j) = (k / row, k % row);
            let plain = (
                shell.along.0 + (shell.along.1 - shell.along.0) * i as f32 / along as f32,
                shell.around.0 + (shell.around.1 - shell.around.0) * j as f32 / around as f32,
            );
            let moved = shell.place(i, j);
            let rounded = (moved.0 - plain.0).abs() + (moved.1 - plain.1).abs() > 1e-4;
            let corner = (i == 0 || i == along) && (j == 0 || j == around);
            assert!(!corner || rounded, "every corner is rounded");
            if corner {
                // Las esquinas, redondeadas: más adentro que los dos bordes.
                let (t, theta) = moved;
                let inside = (t - shell.along.0).abs().min((shell.along.1 - t).abs());
                let up = (theta.abs() - shell.around.1.abs()).abs() * 0.3;
                assert!(inside > 0.01 && up > 0.01, "corner {i} {j}: {t} {theta}");
            } else if rounded {
                // Junto a una esquina, sobre su arco: algo más gruesa que el
                // borde.
                assert!(deep >= shell.thickness / 3.0 - 1e-4, "near a corner {deep}");
            } else if i == 0 || i == along || j == 0 || j == around {
                // En el borde mismo, un tercio: el canto redondeado.
                assert!((deep - shell.thickness / 3.0).abs() < 1e-4, "rim {deep}");
                rim += 1;
            } else if (3..=along - 3).contains(&i) && (5..=around - 5).contains(&j) {
                assert!((deep - shell.thickness).abs() < 1e-4, "middle {deep}");
                middle += 1;
            }
            // Adentro y afuera comparten la piel que tienen debajo.
            assert_eq!(mesh.skins[k], mesh.skins[face + k]);
        }
        assert!(middle > 0 && rim > 0);
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

    /// La capa de `piece` (la primera, si hay varias) ajustada al caballo
    /// sustituto: su mesh y su forma.
    fn fitted(piece: HorsePiece) -> (TackMesh, Shell) {
        let shell = saddle().into_iter().find(|s| s.piece == piece).unwrap();
        (shell.fit(&tube_horse()).0, shell)
    }

    /// Lo más alto de la cara exterior y de la cara interior de `piece` sobre
    /// la columna.
    fn on_the_spine(piece: HorsePiece) -> (f32, f32) {
        let (mesh, shell) = fitted(piece);
        let (along, around) = shell.cells;
        let face = (along + 1) * (around + 1);
        let top = |range: std::ops::Range<usize>| {
            mesh.positions[range]
                .iter()
                .filter(|p| p.z.abs() < 0.03)
                .map(|p| p.y)
                .fold(f32::MIN, f32::max)
        };
        (top(face..2 * face), top(0..face))
    }

    #[test]
    fn the_blankets_stack_each_showing_under_the_next_and_the_cinchon_wraps_the_cincha() {
        use HorsePiece::*;
        // Sobre la columna, cada capa empieza por encima de donde termina la
        // de abajo.
        let stack = [Bajera, Matra, Carona];
        for pair in stack.windows(2) {
            let ((_, below), (above, _)) = (on_the_spine(pair[0]), on_the_spine(pair[1]));
            assert!(below <= above + 1e-4, "{pair:?}: {below} over {above}");
        }
        // Cada manta asoma por debajo de la de arriba: más larga y más baja
        // por los costados.
        for pair in stack.windows(2) {
            let (lower, upper) = (fitted(pair[0]).1, fitted(pair[1]).1);
            assert!(
                lower.along.0 < upper.along.0 && lower.along.1 > upper.along.1,
                "{pair:?}"
            );
            assert!(lower.around.1 > upper.around.1, "{pair:?}");
        }
        // La cincha pasa bajo la panza y el cinchón la envuelve.
        let lowest = |piece| {
            let (mesh, _) = fitted(piece);
            mesh.positions.iter().map(|p| p.y).fold(f32::MAX, f32::min)
        };
        assert!(
            lowest(Cinchon) < lowest(Cincha) && lowest(Cincha) < 1.14 - 0.28,
            "under the belly"
        );
        let below = |p: Vec3| Vec2::new(p.y - 1.14, p.z).length() - 0.28;
        let reach = |piece, fold: fn(f32, f32) -> f32, start| {
            let (mesh, _) = fitted(piece);
            mesh.positions
                .iter()
                .filter(|p| p.y < 0.9)
                .map(|p| below(*p))
                .fold(start, fold)
        };
        assert!(
            reach(Cinchon, f32::min, f32::MAX) > reach(Cincha, f32::max, f32::MIN),
            "the cinchón over the cincha"
        );
    }

    /// Un recado sustituto sobre el caballo sustituto: un tubo alrededor del
    /// costillar a 12 cm de su piel, sobre la columna, con sus marcas.
    fn tube_recado() -> Recado {
        let mut mesh = TackMesh::default();
        let (rings, around) = (8, 16);
        for i in 0..=rings {
            let t = -0.2 + 0.45 * i as f32 / rings as f32;
            for k in 0..=around {
                let theta = -1.2 + 2.4 * k as f32 / around as f32;
                let (o, d) = Axis::BARREL.ray(t, theta);
                mesh.positions.push(o + d * 0.4);
                mesh.skins.push(Skin::default());
                mesh.uvs.push([0.0, 0.0]);
            }
        }
        let row = around as u32 + 1;
        for i in 0..rings as u32 {
            for k in 0..around as u32 {
                let a = i * row + k;
                mesh.triangles
                    .extend([[a, a + 1, a + row], [a + 1, a + row + 1, a + row]]);
            }
        }
        let mark = |p: Vec3| TackMesh {
            positions: vec![p],
            skins: vec![Skin::default()],
            uvs: vec![[0.0, 0.0]],
            triangles: vec![],
        };
        Recado::new(vec![
            ("Cojinillo".into(), mesh),
            ("Mark_ReinL".into(), mark(Vec3::new(1.3, 1.4, -0.06))),
            ("Mark_ReinR".into(), mark(Vec3::new(1.3, 1.4, 0.06))),
            ("Mark_Seat".into(), mark(Vec3::new(SEAT_X, 1.54, 0.0))),
        ])
        .unwrap()
    }

    #[test]
    fn the_cinchon_and_the_fittings_go_over_the_recado() {
        let horse = tube_horse();
        let recado = tube_recado();
        let (fitted, misses) = fit_tack(&horse, Some(&recado));
        assert_eq!(misses, 0);
        let cinchon = &fitted
            .iter()
            .find(|(p, _)| *p == HorsePiece::Cinchon)
            .unwrap()
            .1;
        // Sobre la columna, apenas encima del recado (a 40 cm del eje).
        let top = cinchon
            .positions
            .iter()
            .map(|p| p.y)
            .fold(f32::MIN, f32::max);
        assert!((top - (1.14 + 0.4 + 0.003 + 0.005)).abs() < 0.004, "{top}");
        let f = Fittings::find(&horse, Some(&recado)).unwrap();
        assert_eq!(f.seat, Vec3::new(SEAT_X, 1.54, 0.0));
        assert_eq!(
            f.bit,
            [Vec3::new(1.3, 1.4, -0.06), Vec3::new(1.3, 1.4, 0.06)]
        );
        for s in 0..2 {
            let off = Vec2::new(f.pommel[s].y - 1.14, f.pommel[s].z).length();
            assert!(
                (off - 0.41).abs() < 0.003,
                "the reins rest on the recado: {off}"
            );
        }
        // Sin el recado, las riendas salen de la boca y el jinete se sienta a
        // la altura del recado sobre el lomo.
        let bare = Fittings::find(&horse, None).unwrap();
        let seat = Vec2::new(bare.seat.y - 1.14, bare.seat.z).length() - 0.28;
        assert!((seat - SEAT_RISE).abs() < 0.012, "{seat}");
    }

    #[test]
    fn the_stirrup_is_a_small_criollo_iron_with_its_tread_arch_and_eye() {
        let mesh = stirrup();
        let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("no positions");
        };
        let p: Vec<Vec3> = p.iter().map(|v| Vec3::from(*v)).collect();
        // En su caja unitaria.
        assert!(p.iter().all(|v| v.abs().max_element() <= 0.5 + 1e-3));
        // La pisa: abajo, de lado a lado y de atrás hacia adelante.
        let tread: Vec<&Vec3> = p.iter().filter(|v| v.y < -0.4).collect();
        let span = |f: fn(&Vec3) -> f32| {
            let (lo, hi) = tread
                .iter()
                .map(|v| f(v))
                .fold((f32::MAX, f32::MIN), |(a, b), x| (a.min(x), b.max(x)));
            hi - lo
        };
        assert!(span(|v| v.x) > 0.9 && span(|v| v.z) > 0.9, "a full tread");
        // El arco sube hasta el ojo, angosto arriba: un triángulo.
        let top = p.iter().map(|v| v.y).fold(f32::MIN, f32::max);
        assert!(top > 0.45);
        let high: Vec<&Vec3> = p.iter().filter(|v| v.y > 0.3).collect();
        assert!(high.iter().all(|v| v.x.abs() < 0.3), "narrow at the eye");
        let middle = p
            .iter()
            .filter(|v| v.y.abs() < 0.05)
            .map(|v| v.x.abs())
            .fold(f32::MIN, f32::max);
        assert!(middle > 0.3, "the arch spreads to the tread: {middle}");
        // Normales y tangentes para sus mapas.
        assert!(mesh.attribute(Mesh::ATTRIBUTE_NORMAL).is_some());
        assert!(mesh.attribute(Mesh::ATTRIBUTE_TANGENT).is_some());
    }

    #[test]
    fn the_fittings_are_found_on_the_body() {
        let horse = tube_horse();
        let f = Fittings::find(&horse, None).unwrap();
        for s in 0..2 {
            // La izquierda es -Z.
            let sign = if s == 0 { -1.0 } else { 1.0 };
            for p in [f.pommel[s], f.leather[s], f.hanging[s]] {
                assert!(p.z * sign > 0.0, "{s}: {p}");
            }
            assert!(f.hanging[s].z.abs() > 0.28, "clear of the barrel");
        }
    }

    /// Una taza abierta hacia +Z (un fondo y cuatro paredes de 10 cm, sin
    /// tapa, con las caras hacia afuera), como una oreja hueca, y una caja
    /// cerrada al lado.
    fn cup_and_box() -> (Vec<Vec3>, Vec<[u32; 3]>) {
        let mut positions = Vec::new();
        let mut triangles = Vec::new();
        let mut quad = |a: Vec3, b: Vec3, c: Vec3, d: Vec3| {
            let base = positions.len() as u32;
            positions.extend([a, b, c, d]);
            triangles.extend([[base, base + 1, base + 2], [base, base + 2, base + 3]]);
        };
        let v = |x: f32, y: f32, z: f32| Vec3::new(x, y, z);
        for (dx, open) in [(0.0, true), (0.3, false)] {
            let (lo, hi) = (v(dx, 0.0, 0.0), v(dx + 0.1, 0.1, 0.1));
            // Las seis caras de la caja, hacia afuera; la taza no tiene la de +Z.
            quad(
                v(lo.x, lo.y, lo.z),
                v(lo.x, hi.y, lo.z),
                v(hi.x, hi.y, lo.z),
                v(hi.x, lo.y, lo.z),
            );
            if !open {
                quad(
                    v(lo.x, lo.y, hi.z),
                    v(hi.x, lo.y, hi.z),
                    v(hi.x, hi.y, hi.z),
                    v(lo.x, hi.y, hi.z),
                );
            }
            quad(
                v(lo.x, lo.y, lo.z),
                v(hi.x, lo.y, lo.z),
                v(hi.x, lo.y, hi.z),
                v(lo.x, lo.y, hi.z),
            );
            quad(
                v(lo.x, hi.y, lo.z),
                v(lo.x, hi.y, hi.z),
                v(hi.x, hi.y, hi.z),
                v(hi.x, hi.y, lo.z),
            );
            quad(
                v(lo.x, lo.y, lo.z),
                v(lo.x, lo.y, hi.z),
                v(lo.x, hi.y, hi.z),
                v(lo.x, hi.y, lo.z),
            );
            quad(
                v(hi.x, lo.y, lo.z),
                v(hi.x, hi.y, lo.z),
                v(hi.x, hi.y, hi.z),
                v(hi.x, lo.y, hi.z),
            );
        }
        (positions, triangles)
    }

    #[test]
    fn a_probe_in_an_open_cup_is_outside_and_one_in_a_closed_box_inside() {
        let (positions, triangles) = cup_and_box();
        let near = Near::new(&positions, &triangles);
        // En la boca de la taza, a 5 mm del fondo: el triángulo más cercano
        // es el fondo, de espaldas, pero es aire (como frente a una oreja).
        let cup = Vec3::new(0.05, 0.05, 0.005);
        assert!(
            signed_distance(cup, &positions, &triangles) < 0.0,
            "the naive sign"
        );
        let s = near.signed_distance(cup).unwrap();
        assert!((s - 0.005).abs() < 1e-4, "outside, 5 mm off: {s}");
        // Dentro de la caja cerrada, de verdad adentro.
        let s = near.signed_distance(Vec3::new(0.35, 0.05, 0.005)).unwrap();
        assert!((s + 0.005).abs() < 1e-4, "inside, 5 mm deep: {s}");
    }

    #[test]
    fn a_strap_runs_from_one_end_to_the_other() {
        let (a, b) = (Vec3::new(1.0, 2.0, 0.5), Vec3::new(-0.5, 1.0, 0.2));
        let placed = strap_between(a, b, 0.02);
        assert!(placed.transform_point(Vec3::Y * 0.5).distance(b) < 1e-5);
        assert!(placed.transform_point(Vec3::Y * -0.5).distance(a) < 1e-5);
        let (scale, _, _) = placed.to_scale_rotation_translation();
        assert!((scale.x - 0.02).abs() < 1e-6 && (scale.z - 0.02).abs() < 1e-6);
        // Una plana, colgando: su ancho hacia adelante y su cara de costado.
        let (a, b) = (Vec3::new(0.1, 1.2, 0.3), Vec3::new(0.05, 0.9, 0.35));
        let placed = flat_strap_between(a, b, 0.03, Vec3::X);
        assert!(placed.transform_point(Vec3::Y * 0.5).distance(b) < 1e-5);
        assert!(placed.transform_point(Vec3::Y * -0.5).distance(a) < 1e-5);
        let width = placed.affine().transform_vector3(Vec3::X);
        assert!(width.normalize().dot(Vec3::X) > 0.98, "{width}");
        assert!((width.length() - 0.03).abs() < 1e-5);
        assert!(
            placed
                .affine()
                .transform_vector3(Vec3::Z)
                .normalize()
                .z
                .abs()
                > 0.98
        );
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
    pub(crate) struct Near<'a> {
        positions: &'a [Vec3],
        triangles: &'a [[u32; 3]],
        cells: HashMap<IVec3, Vec<u32>>,
    }

    impl<'a> Near<'a> {
        const CELL: f32 = 0.1;

        pub(crate) fn new(positions: &'a [Vec3], triangles: &'a [[u32; 3]]) -> Self {
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
        pub(crate) fn signed_distance(&self, p: Vec3) -> Option<f32> {
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
            // El lado del triángulo más cercano no basta frente a una cavidad
            // abierta (la oreja hueca: su fondo da la espalda a lo que está en
            // su boca, que es aire); adentro es adentro solo si lo dicen los
            // rayos en las tres direcciones (cruzan la piel un número impar de
            // veces).
            let s = if s < 0.0 && !self.inside(p) { -s } else { s };
            (!near.is_empty() && s.abs() <= Self::CELL).then_some(s)
        }

        /// Si `p` está dentro de la superficie: un rayo hacia +X, +Y y +Z
        /// desde él la cruza un número impar de veces en cada caso.
        fn inside(&self, p: Vec3) -> bool {
            [Vec3::X, Vec3::Y, Vec3::Z].iter().all(|dir| {
                let far = p + *dir * 10.0;
                self.triangles
                    .iter()
                    .filter(|t| crosses(p, far, self.positions, std::slice::from_ref(*t)))
                    .count()
                    % 2
                    == 1
            })
        }
    }

    /// Las peores holguras a lo largo de `frames` frames del caballo ensillado
    /// de `app` bajo `me`, a menos de 10 cm de su piel: cuánto se separan de
    /// ella las carcasas de los aperos (negativo: dentro de ella), cuánto se
    /// separa el modelo del jinete, y las correas que atraviesan la piel.
    /// Las piezas de los aperos que se dibujan sobre la piel, por su nombre.
    pub(crate) const TACK_SHELLS: [&str; 18] = [
        "Bajera",
        "Matra",
        "Carona",
        "Cincha",
        "Argolla",
        "Cinchon",
        "Bastos",
        "Cojinillo",
        "Sobrepuesto",
        "Vellon",
        "Testera",
        "Frontalera",
        "Quijera_L",
        "Quijera_R",
        "Muserola",
        "Fiador",
        "Barbada",
        "Freno",
    ];

    /// Cuánto puede hundirse una pierna del jinete en la lana del cojinillo, a
    /// cualquier andar: lo que cede la lana larga, 4 cm de alto sobre el
    /// asiento, bajo un muslo o una rodilla que la aprieta (medido: 1.6 cm
    /// sentado, 0.4 cm en medio asiento al galope).
    const FLEECE_GIVE: f32 = 0.02;

    /// Los materiales de las piernas de los jinetes.
    const LEGS: [&str; 2] = ["bombacha", "boots"];

    /// Las piezas del recado modelado que llevan el peso del jinete.
    const SEAT: [&str; 3] = ["Bastos", "Cojinillo", "Sobrepuesto"];

    /// A lo largo de `frames` frames: cuánto se separan de la piel, como
    /// mínimo, los bastos, el cojinillo y el sobrepuesto (la piel nunca los
    /// atraviesa), y cuánto se separan las piernas del jinete, como mínimo, de
    /// su cara de arriba (negativo: metidas en la lana).
    pub(crate) fn seat_clearances(app: &mut App, me: Entity, frames: usize) -> (f32, f32) {
        let (mut wool, mut rider) = (f32::MAX, f32::MAX);
        for _ in 0..frames {
            app.update();
            let drawn = cpu::drawn(app.world_mut(), Some(me));
            let (skin, tris) = horse_skin(&drawn);
            let near_skin = Near::new(&skin, &tris);
            let (mut seat, mut seat_tris) = (Vec::new(), Vec::new());
            for d in drawn.iter().filter(|d| SEAT.contains(&d.name.as_str())) {
                let base = seat.len() as u32;
                seat.extend(&d.positions);
                // Solo su cara de arriba, la que mira hacia afuera del
                // costillar (sus cantos y su cara de abajo no la dan vuelta).
                seat_tris.extend(d.triangles.iter().filter_map(|t| {
                    let [a, b, c] = t.map(|i| d.positions[i as usize]);
                    let normal = (b - a).cross(c - a).normalize_or_zero();
                    let centre = (a + b + c) / 3.0;
                    let out = Vec3::new(0.0, centre.y - 1.14, centre.z).normalize_or_zero();
                    (normal.dot(out) > 0.3).then(|| t.map(|i| i + base))
                }));
                for p in &d.positions {
                    if let Some(s) = near_skin.signed_distance(*p) {
                        wool = wool.min(s);
                    }
                }
            }
            let near_seat = Near::new(&seat, &seat_tris);
            // Las piernas del jinete: la bombacha y las botas (el poncho y la
            // mano libre pueden apoyarse sobre el recado).
            for d in drawn.iter().filter(|d| LEGS.contains(&d.name.as_str())) {
                for p in &d.positions {
                    if let Some(s) = near_seat.signed_distance(*p) {
                        rider = rider.min(s);
                    }
                }
            }
        }
        (wool, rider)
    }

    pub(crate) fn clearances(app: &mut App, me: Entity, frames: usize) -> (f32, f32, Vec<String>) {
        let shells = TACK_SHELLS;
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
            // Cruza la boca, por dentro.
            "Bocado",
        ];
        let (mut tack, mut rider, mut cuts) = (f32::MAX, f32::MAX, Vec::new());
        let mut closest = String::new();
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
                    if is_tack && s < tack {
                        closest = d.name.clone();
                    }
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
                // Por cada tramo de la curva en la que cuelga.
                let curve = strap_curve(at(strap.ends[0]), at(strap.ends[1]), strap.sag, SAG_PARTS);
                if curve.windows(2).any(|w| crosses(w[0], w[1], &skin, &tris)) {
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
        println!("the tack comes closest to the skin with its {closest} ({tack:.4} m)");
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
        for piece in HorsePiece::ALL {
            let skinned = !matches!(
                piece,
                HorsePiece::Rein | HorsePiece::StirrupLeather | HorsePiece::Stirrup
            );
            assert!(
                found.contains(&(piece, skinned)),
                "{piece:?} skinned: {skinned}"
            );
        }
        let count = |p| found.iter().filter(|(f, _)| *f == p).count();
        // Del recado modelado: los bastos, el cojinillo con su lana, el
        // sobrepuesto, las siete tiras de la cabezada y el freno con su
        // bocado.
        assert_eq!(count(HorsePiece::Basto), 1);
        assert_eq!(count(HorsePiece::Pellon), 1);
        assert_eq!(count(HorsePiece::Vellon), 1);
        assert_eq!(count(HorsePiece::Sobrepuesto), 1);
        assert_eq!(count(HorsePiece::Headstall), 7);
        assert_eq!(count(HorsePiece::Bit), 2);
        assert_eq!(count(HorsePiece::Argolla), 2);
        assert_eq!(count(HorsePiece::Stirrup), 2);
        assert_eq!(count(HorsePiece::StirrupLeather), 2);
        let reins = world
            .query::<(&HorsePiece, &Strap)>()
            .iter(world)
            .filter(|(p, _)| **p == HorsePiece::Rein)
            .count();
        assert_eq!(reins, 6, "three stretches a side");
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

    #[test]
    fn every_piece_of_the_tack_wears_its_maps() {
        let (mut app, _) = saddled(CharacterVisual::Horse(Coat::Bayo), 0.0);
        let world = app.world_mut();
        // Cada pieza dibujada: las carcasas, los estribos, las aciones y los
        // tramos de las riendas que cuelgan (de la pieza de su padre).
        let mut drawn = world.query::<(
            Option<&HorsePiece>,
            Option<&ChildOf>,
            &Mesh3d,
            &MeshMaterial3d<StandardMaterial>,
        )>();
        let found: Vec<_> = drawn
            .iter(world)
            .filter_map(|(p, of, m, s)| {
                let piece = p
                    .copied()
                    .or_else(|| of.and_then(|of| world.get::<HorsePiece>(of.parent()).copied()))?;
                Some((piece, m.0.clone(), s.0.clone()))
            })
            .collect();
        let (materials, meshes) = (
            world.resource::<Assets<StandardMaterial>>(),
            world.resource::<Assets<Mesh>>(),
        );
        let mut seen = Vec::new();
        for (piece, mesh, paint) in found {
            let (_, _, set) = piece.look();
            let m = materials.get(&paint).unwrap();
            let path = |h: &Option<Handle<Image>>| {
                h.as_ref()
                    .and_then(|h| h.path())
                    .map(|p| p.path().to_string_lossy().into_owned())
            };
            // Ningún color liso: el color, el relieve y el ORM de su conjunto.
            assert_eq!(
                path(&m.base_color_texture),
                Some(set.path("color")),
                "{piece:?}"
            );
            assert_eq!(
                path(&m.normal_map_texture),
                Some(set.path("normal_gl")),
                "{piece:?}"
            );
            assert_eq!(
                path(&m.metallic_roughness_texture),
                Some(set.path("orm")),
                "{piece:?}"
            );
            assert_eq!(m.base_color, Color::WHITE);
            // Con alpha test, la lana larga y los flecos de las mantas.
            assert_eq!(
                matches!(m.alpha_mode, AlphaMode::Mask(_)),
                matches!(
                    piece,
                    HorsePiece::Vellon | HorsePiece::Bajera | HorsePiece::Matra
                ),
                "{piece:?}"
            );
            // Con UV y tangentes para sus mapas.
            let mesh = meshes.get(&mesh).unwrap();
            assert!(mesh.attribute(Mesh::ATTRIBUTE_UV_0).is_some(), "{piece:?}");
            assert!(
                mesh.attribute(Mesh::ATTRIBUTE_TANGENT).is_some(),
                "{piece:?}"
            );
            seen.push(piece);
        }
        for piece in HorsePiece::ALL {
            assert!(seen.contains(&piece), "{piece:?}");
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
                    flat: false,
                    sag: 0.0,
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
    fn a_slack_strap_hangs_in_a_curve_of_parts_end_to_end() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        let at = |x: f32| GlobalTransform::from_xyz(x, 1.0, 0.0);
        let parent = world.spawn(at(0.0)).id();
        let (a, b) = (world.spawn(at(0.0)).id(), world.spawn(at(1.0)).id());
        let strap = world
            .spawn((
                Strap {
                    ends: [(a, Vec3::ZERO), (b, Vec3::ZERO)],
                    width: 0.012,
                    flat: false,
                    sag: 0.1,
                },
                Transform::default(),
                GlobalTransform::default(),
                Visibility::default(),
                ChildOf(parent),
            ))
            .id();
        let parts: Vec<Entity> = (0..4)
            .map(|k| {
                world
                    .spawn((
                        StrapPart { k, n: 4 },
                        Transform::default(),
                        GlobalTransform::default(),
                        ChildOf(strap),
                    ))
                    .id()
            })
            .collect();
        world.run_system_once(stretch_straps).unwrap();
        let ends = |e: Entity| {
            let g = world.get::<GlobalTransform>(e).unwrap();
            (
                g.transform_point(Vec3::Y * -0.5),
                g.transform_point(Vec3::Y * 0.5),
            )
        };
        // Cada tramo empieza donde terminó el anterior (con su solape) y el
        // del medio cuelga un décimo del largo, 10 cm, por debajo de la recta.
        let curve = strap_curve(Vec3::new(0.0, 1.0, 0.0), Vec3::new(1.0, 1.0, 0.0), 0.1, 4);
        assert!((curve[2].y - 0.9).abs() < 1e-5);
        for (k, part) in parts.iter().enumerate() {
            let (p, q) = ends(*part);
            let mid = (p + q) / 2.0;
            assert!(mid.distance((curve[k] + curve[k + 1]) / 2.0) < 1e-4, "{k}");
            // Solapados un 8 % de su largo en cada punta.
            let long = curve[k].distance(curve[k + 1]);
            assert!((p.distance(curve[k]) - 0.08 * long).abs() < 1e-4, "{k}");
            assert!((q.distance(curve[k + 1]) - 0.08 * long).abs() < 1e-4, "{k}");
            assert!(mid.y < 1.0, "below the straight line");
        }
        // La correa misma no se estira: se queda en su padre.
        assert_eq!(
            world.get::<GlobalTransform>(strap).unwrap(),
            world.get::<GlobalTransform>(parent).unwrap()
        );
        world.despawn(b);
        world.run_system_once(stretch_straps).unwrap();
        assert_eq!(*world.get::<Visibility>(strap).unwrap(), Visibility::Hidden);
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
            // La piel nunca atraviesa el recado, y el jinete se sienta sobre la
            // lana sin meterse en ella.
            let (wool, fleece) = seat_clearances(&mut app, me, frames);
            println!(
                "{speed} m/s: the recado {wool:.4} m off the skin, the rider {fleece:.4} m off the fleece"
            );
            assert!(
                wool > 0.002,
                "{speed} m/s: the skin pokes through the recado ({wool})"
            );
            assert!(
                fleece > -FLEECE_GIVE,
                "{speed} m/s: the rider cuts into the fleece ({fleece})"
            );
        }
    }

    /// Las piezas del lomo: el recado modelado y el cinchón, que nunca
    /// siguen al cuello.
    const BACK: [&str; 5] = ["Bastos", "Cojinillo", "Sobrepuesto", "Vellon", "Cinchon"];

    /// Dónde estaban, en el espacio del torso, los vertices de las piezas del
    /// lomo (cada uno por su mesh y su índice) y la piel más cercana a cada
    /// uno, al empezar.
    struct Drift {
        torso: Entity,
        back: Vec<((String, usize), Vec3, usize, Vec3)>,
    }

    /// Las piezas del lomo de `drawn` por su nombre, y la piel, en el espacio
    /// del torso.
    fn back_and_skin(
        world: &World,
        torso: Entity,
        drawn: &[cpu::Drawn],
    ) -> (HashMap<String, Vec<Vec3>>, Vec<Vec3>) {
        let to_torso = world
            .get::<GlobalTransform>(torso)
            .unwrap()
            .affine()
            .inverse();
        let (skin, _) = horse_skin(drawn);
        let back = drawn
            .iter()
            .filter(|d| BACK.contains(&d.name.as_str()))
            .map(|d| {
                let p = d
                    .positions
                    .iter()
                    .map(|p| to_torso.transform_point3(*p))
                    .collect();
                (d.name.clone(), p)
            })
            .collect();
        (
            back,
            skin.iter().map(|p| to_torso.transform_point3(*p)).collect(),
        )
    }

    impl Drift {
        fn start(app: &mut App, me: Entity) -> Self {
            let carrier = app.world().get::<Children>(me).unwrap()[0];
            let torso = app.world().get::<Tack>(carrier).unwrap().torso;
            let drawn = cpu::drawn(app.world_mut(), Some(me));
            let (back, skin) = back_and_skin(app.world(), torso, &drawn);
            let world_of: HashMap<String, Vec<Vec3>> = drawn
                .iter()
                .map(|d| (d.name.clone(), d.positions.clone()))
                .collect();
            let mut out = Vec::new();
            for (name, points) in &back {
                for (i, p) in points.iter().enumerate() {
                    // Del cinchón, la parte de arriba, sobre el recado: bajo la
                    // panza sigue la piel, que las manos mueven al galope.
                    if name == "Cinchon" && world_of[name][i].y < 1.2 {
                        continue;
                    }
                    let (k, q) = skin
                        .iter()
                        .enumerate()
                        .min_by(|a, b| {
                            a.1.distance_squared(*p)
                                .total_cmp(&b.1.distance_squared(*p))
                        })
                        .unwrap();
                    out.push(((name.clone(), i), *p, k, *q));
                }
            }
            Self { torso, back: out }
        }

        /// Cuánto más que la piel de debajo se movió, como máximo, un vertex
        /// de las piezas del lomo desde el comienzo, en el espacio del torso,
        /// a lo largo de `frames` frames, y de qué pieza.
        fn worst(&self, app: &mut App, me: Entity, frames: usize) -> (f32, String) {
            let mut worst = (0.0f32, String::new());
            for _ in 0..frames {
                app.update();
                let drawn = cpu::drawn(app.world_mut(), Some(me));
                let (back, skin) = back_and_skin(app.world(), self.torso, &drawn);
                for ((name, i), was, k, skin_was) in &self.back {
                    let moved = back[name][*i].distance(*was) - skin[*k].distance(*skin_was);
                    if moved > worst.0 {
                        worst = (moved, name.clone());
                    }
                }
            }
            worst
        }
    }

    /// Cuánto puede moverse una pieza del lomo, respecto del torso, más que la
    /// piel que tiene debajo: nada atado ni al paso (medido: 0 mm, cabeza
    /// baja incluida); al galope la punta de los bastos, rígidos sobre un lomo
    /// que se dobla, se mece 2.2 cm.
    const BACK_DRIFT: f32 = 0.025;

    #[test]
    fn the_back_pieces_ride_the_trunk_when_the_head_goes_down_tied_or_ridden() {
        // Atado: cada espera, cabeza baja incluida.
        let (mut app, me) = saddled(CharacterVisual::Horse(Coat::Zaino), 0.0);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        let drift = Drift::start(&mut app, me);
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
            app.world_mut().entity_mut(carrier).insert(body);
            let (worst, piece) = drift.worst(&mut app, me, 40);
            println!("{idle:?}: the back pieces drift {worst:.3} m beyond the skin ({piece})");
            assert!(worst < BACK_DRIFT, "{idle:?}: {piece} {worst}");
        }
        // Montado: a cada andar.
        use crate::character::horse::tests::ride;
        use crate::character::rider::tests::seated;
        let rider = CharacterVisual::Mounted {
            rider: crate::character::FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = seated(rider, 0.0);
        let drift = Drift::start(&mut app, me);
        for speed in [0.0, 1.5, 4.5, 12.2] {
            for _ in 0..30 {
                ride(&mut app, me, speed);
            }
            let (worst, piece) = drift.worst(&mut app, me, 12);
            println!("{speed} m/s: the back pieces drift {worst:.3} m beyond the skin ({piece})");
            assert!(worst < BACK_DRIFT, "{speed} m/s: {piece} {worst}");
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

    /// Cada mesh dibujada del caballo ensillado de `app` bajo `me`, por su
    /// nombre, en el primer frame.
    fn drawn_by_name(app: &mut App, me: Entity) -> HashMap<String, Vec<cpu::Drawn>> {
        app.update();
        let mut by: HashMap<String, Vec<cpu::Drawn>> = HashMap::default();
        for d in cpu::drawn(app.world_mut(), Some(me)) {
            by.entry(d.name.clone()).or_default().push(d);
        }
        by
    }

    #[test]
    fn the_whole_tack_stays_within_its_triangle_budget() {
        let (mut app, me) = saddled(CharacterVisual::Horse(Coat::Bayo), 0.0);
        let by = drawn_by_name(&mut app, me);
        let mut tack = 0;
        let mut rows = Vec::new();
        for (name, ds) in &by {
            if TACK_SHELLS.contains(&name.as_str())
                || ["Rein", "StirrupLeather", "Stirrup", "Bocado"].contains(&name.as_str())
            {
                let n: usize = ds.iter().map(|d| d.triangles.len()).sum();
                rows.push(format!("{name} {n}"));
                tack += n;
            }
        }
        rows.sort();
        println!("the tack draws {tack} triangles: {}", rows.join(", "));
        assert!(by.contains_key("Cojinillo"), "the modelled recado is on");
        assert!(tack <= TACK_BUDGET, "{tack}");
    }

    /// Las piezas de los aperos en reposo, en el marco del cuerpo, por su
    /// nombre, y el caballo en reposo sobre el que se ajustaron.
    fn at_rest(app: &App) -> (HashMap<String, Vec<Vec3>>, TackRest) {
        let built = app
            .world()
            .resource::<TackBuilt>()
            .0
            .clone()
            .expect("the tack is built");
        let meshes = app.world().resource::<Assets<Mesh>>();
        let mut by: HashMap<String, Vec<Vec3>> = HashMap::default();
        for (_, name, mesh, _) in &built.kit.pieces {
            let Some(VertexAttributeValues::Float32x3(p)) = meshes
                .get(mesh)
                .and_then(|m| m.attribute(Mesh::ATTRIBUTE_POSITION))
            else {
                continue;
            };
            by.entry(name.clone())
                .or_default()
                .extend(p.iter().map(|v| Vec3::from(*v)));
        }
        (by, built.rest)
    }

    #[test]
    fn the_cabezada_lies_on_the_head_above_and_behind_the_eyes_and_below_the_ears() {
        let (app, _) = saddled(CharacterVisual::Horse(Coat::Zaino), 0.0);
        let (by, rest) = at_rest(&app);
        let skin = &rest.skin;
        let near = Near::new(&skin.positions, &skin.triangles);
        // Los ojos: el centro y el radio del blanco de cada uno.
        let eyes: Vec<(Vec3, f32)> = [-1.0f32, 1.0]
            .iter()
            .map(|s| {
                let side: Vec<Vec3> = rest
                    .eyes
                    .iter()
                    .copied()
                    .filter(|p| p.z * s > 0.0)
                    .collect();
                let c = side.iter().sum::<Vec3>() / side.len() as f32;
                (c, side.iter().map(|p| p.distance(c)).fold(0.0, f32::max))
            })
            .collect();
        assert_eq!(rest.ear_tips.len(), 2);
        let straps = [
            "Testera",
            "Frontalera",
            "Quijera_L",
            "Quijera_R",
            "Muserola",
            "Fiador",
            "Barbada",
        ];
        let mut worst = Vec::new();
        for strap in straps {
            let mut most = 0.0f32;
            for p in by.get(strap).unwrap_or_else(|| panic!("{strap}")) {
                let off = near.signed_distance(*p).unwrap_or(f32::MAX);
                assert!(off > 0.0, "{strap} inside the head at {p}");
                most = most.max(off);
            }
            worst.push((strap, most));
        }
        println!("the cabezada stands off the head at most: {worst:?}");
        // Sobre la piel: ningún vertex a más de STRAP_GAP de ella (salvo
        // donde la cabeza baja aparta una tira: ver HEAD_DOWN).
        for (strap, most) in worst {
            let limit = HEAD_DOWN
                .iter()
                .find(|(s, _)| *s == strap)
                .map_or(STRAP_GAP, |(_, l)| *l);
            assert!(most <= limit, "{strap} stands {most} m off the head");
        }
        // La frontalera, por encima de los dos ojos y por debajo de las puntas
        // de las orejas.
        for p in &by["Frontalera"] {
            for (eye, _) in &eyes {
                assert!(p.y > eye.y + 0.04, "the browband above the eyes: {p}");
            }
            assert!(
                rest.ear_tips.iter().all(|e| p.y < e.y),
                "below the ears: {p}"
            );
        }
        // Las quijeras nunca cruzan un ojo.
        for q in ["Quijera_L", "Quijera_R"] {
            for p in &by[q] {
                for (eye, r) in &eyes {
                    assert!(p.distance(*eye) > r + 0.01, "{q} over an eye at {p}");
                }
            }
        }
    }

    /// Cuánto se separa de la piel, como máximo, una tira de la cabezada en
    /// reposo: la luz que le deja `tools/recado.py` (3 mm, y algo más donde
    /// la piel se mueve bajo ella en los clips de un caballo ensillado) y su
    /// espesor (3–5 mm), sobre una cabeza curva.
    const STRAP_GAP: f32 = 0.008;

    /// Las dos tiras que la cabeza baja (`Idle_Headlow`) aparta un poco más:
    /// la testera en un costado de la nuca (medido 9.8 mm) y el fiador bajo
    /// la garganta (8.7 mm), donde la piel se pliega sobre ellas.
    const HEAD_DOWN: [(&str, f32); 2] = [("Testera", 0.01), ("Fiador", 0.009)];

    #[test]
    fn the_bit_crosses_the_mouth_at_the_lip_corners_and_its_shanks_lie_on_the_lips() {
        let (app, _) = saddled(CharacterVisual::Horse(Coat::Zaino), 0.0);
        let (by, rest) = at_rest(&app);
        let skin = &rest.skin;
        let near = Near::new(&skin.positions, &skin.triangles);
        // El bocado: por dentro de la boca, de una comisura a la otra.
        let bar = &by["Bocado"];
        let centre = bar.iter().sum::<Vec3>() / bar.len() as f32;
        assert!(
            signed_distance(centre, &skin.positions, &skin.triangles) < 0.0,
            "inside the mouth: {centre}"
        );
        for s in [-1.0f32, 1.0] {
            let end = bar
                .iter()
                .copied()
                .max_by(|a, b| (a.z * s).total_cmp(&(b.z * s)))
                .unwrap();
            let at = signed_distance(end, &skin.positions, &skin.triangles);
            assert!(
                at.abs() < 0.012,
                "the bar comes out at the lip corner: {at} at {end}"
            );
        }
        // Las piernas, contra los labios por fuera: cerca de la piel, nunca
        // dentro de ella.
        let iron = &by["Freno"];
        let mut offs: Vec<f32> = iron
            .iter()
            .filter_map(|p| near.signed_distance(*p))
            .collect();
        offs.sort_by(f32::total_cmp);
        let median = offs[offs.len() / 2];
        println!(
            "the bit's iron: {median:.4} m off the lips (median), {} of {} vertices near the face",
            offs.len(),
            iron.len()
        );
        assert!(offs.len() * 2 > iron.len(), "most of it near the face");
        assert!(median < 0.012, "{median}");
        assert!(offs.iter().all(|o| *o > 0.0), "never inside the lips");
    }
}
