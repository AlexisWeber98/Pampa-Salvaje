//! Los animales de la pampa.
//!
//! El juego los simula (ver `pampa_core::fauna`) y le da a cada entidad un
//! [`Animal`] (qué es), un [`AnimalPose`] (hacia dónde mira y qué hace) y,
//! opcionalmente, un [`AnimalSeed`] (su identidad estable); su `Transform`
//! lleva solo la traslación, sobre el suelo, el agua o en el aire.
//!
//! - El ganado (vacas y toros) y los caballos del corral son modelos glTF
//!   animados con skinning cuyos clips siguen el estado de la fauna (ver
//!   [`livestock`]).
//! - Los animales silvestres pequeños se dibujan como las figuras de los
//!   vecinos: piezas rígidas low-poly colgadas de unas pocas articulaciones,
//!   así que la animación solo gira articulaciones (las patas se balancean, las
//!   cabezas bajan para picotear o se mecen, las colas se agitan, las alas
//!   aletean), a partir de un único [`FaunaKit`] de cuatro mallas unitarias
//!   compartidas, escaladas por pieza, y un material compartido por cada
//!   [`Paint`]. Más allá de [`ANIMATE_FAR`] desde la cámara conservan su última
//!   pose (solo gira el cuerpo).
//!
//! Cada malla se dibuja dentro de los rangos compartidos de los asentamientos
//! (los animales de suelo, el de los objetos; las aves que planean, el de los
//! edificios): ningún `VisibilityRange` nuevo.

pub mod livestock;

pub use livestock::{
    Clip, FREEZE_FAR, LivestockModel, LivestockModels, LivestockRig, RESUME_NEAR, Species,
};

use bevy::app::AnimationSystems;
use bevy::camera::visibility::VisibilityRange;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use pampa_core::{CowCoat, Heading};
use std::f32::consts::{FRAC_PI_2, TAU};

use crate::character::Coat;

/// Qué es un animal.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[require(AnimalPose, Transform, Visibility)]
pub enum Animal {
    Cow(CowCoat),
    /// El toro de una tropa, con el pelaje de su raza.
    Bull(CowCoat),
    /// Un caballo suelto parado en un corral.
    Horse(Coat),
    Hare,
    /// Un tero (avefría sureña).
    Tero,
    /// Las dos rapaces que planean en lo alto (modelos, ver [`livestock`]).
    Chimango,
    Carancho,
    Duck,
    /// El zorro gris (modelo).
    Zorro,
    Nandu,
    /// Los dos armadillos: la mulita, chica y oscura, y el peludo, más grande
    /// y pardo.
    Mulita,
    Peludo,
    Vizcacha,
    /// Los felinos del monte (modelos).
    GatoMontes,
    Puma,
    Yaguarete,
    Nutria,
}

impl Animal {
    /// Si se dibuja a partir de un modelo animado (ver [`livestock`]) en lugar
    /// de piezas rígidas: el ganado, los caballos, el zorro, los felinos y las
    /// rapaces.
    pub fn is_livestock(self) -> bool {
        Species::of(self).is_some()
    }
}

/// La identidad estable de un animal, a partir de su lugar con semilla en el
/// mundo: elige su espera y desincroniza sus clips de los de sus vecinos. Sin
/// ella, el índice de la entidad la sustituye.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AnimalSeed(pub u32);

/// Qué está haciendo un animal, tal como se dibuja.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Gait {
    #[default]
    Stand,
    /// Cabeza gacha en el pasto (o picoteando).
    Graze,
    Walk,
    /// Huyendo a toda carrera o saltando rápido.
    Run,
    /// Aleteando.
    Fly,
    /// Planeando con las alas quietas.
    Glide,
    Swim,
}

/// Estado lógico que necesita el dibujo, copiado de `pampa-core` por el juego.
#[derive(Component, Debug, Clone, Copy, PartialEq, Default)]
pub struct AnimalPose {
    pub heading: Heading,
    pub gait: Gait,
    /// Velocidad sobre el suelo, en m/s: marca el ritmo de las patas.
    pub speed: f32,
    /// Alabeo al girar, en radianes (positivo: ala izquierda hacia abajo).
    pub bank: f32,
}

/// Más allá de esta distancia de la cámara, las articulaciones de un animal
/// dejan de moverse.
pub const ANIMATE_FAR: f32 = 80.0;

/// Las articulaciones de un animal: el cuerpo contiene a todas las demás.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Joint {
    Head,
    /// Patas: los pares delantero y trasero de una liebre son `Leg(0)` y
    /// `Leg(2)`; las dos de un ave, `Leg(0)` y `Leg(1)`.
    Leg(u8),
    Tail,
    /// Izquierda (`0`, -Z) y derecha (`1`, +Z).
    Wing(u8),
}

/// Una malla unitaria compartida.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Shape {
    Ball,
    Block,
    Rod,
    Cone,
}

impl Shape {
    pub const ALL: [Self; 4] = [Self::Ball, Self::Block, Self::Rod, Self::Cone];

    pub fn mesh(self) -> Mesh {
        match self {
            Self::Ball => Sphere::new(0.5).mesh().uv(8, 6),
            Self::Block => Cuboid::from_length(1.0).mesh().build(),
            Self::Rod => Cylinder::new(0.5, 1.0).mesh().resolution(6).build(),
            Self::Cone => Cone::new(0.5, 1.0).mesh().resolution(6).build(),
        }
    }
}

/// Un material compartido.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Paint {
    White,
    Black,
    Horn,
    HareBrown,
    BirdGrey,
    BirdRed,
    DuckBrown,
    DuckGreen,
    Bill,
    NanduGrey,
    MulitaShell,
    PeludoShell,
    /// La piel rosada grisácea de la cara y las patas de los armadillos.
    ArmadilloSkin,
    VizcachaGrey,
    NutriaBrown,
    /// Los incisivos anaranjados de la nutria.
    Incisor,
}

impl Paint {
    pub const ALL: [Self; 16] = [
        Self::White,
        Self::Black,
        Self::Horn,
        Self::HareBrown,
        Self::BirdGrey,
        Self::BirdRed,
        Self::DuckBrown,
        Self::DuckGreen,
        Self::Bill,
        Self::NanduGrey,
        Self::MulitaShell,
        Self::PeludoShell,
        Self::ArmadilloSkin,
        Self::VizcachaGrey,
        Self::NutriaBrown,
        Self::Incisor,
    ];

    fn color(self) -> Color {
        let [r, g, b] = match self {
            Self::White => [0.82, 0.8, 0.74],
            Self::Black => [0.05, 0.045, 0.04],
            Self::Horn => [0.72, 0.66, 0.52],
            Self::HareBrown => [0.42, 0.33, 0.22],
            Self::BirdGrey => [0.5, 0.48, 0.44],
            Self::BirdRed => [0.62, 0.18, 0.14],
            Self::DuckBrown => [0.36, 0.28, 0.18],
            Self::DuckGreen => [0.08, 0.2, 0.12],
            Self::Bill => [0.55, 0.45, 0.12],
            Self::NanduGrey => [0.5, 0.47, 0.41],
            Self::MulitaShell => [0.3, 0.26, 0.22],
            Self::PeludoShell => [0.42, 0.33, 0.23],
            Self::ArmadilloSkin => [0.62, 0.52, 0.45],
            Self::VizcachaGrey => [0.48, 0.47, 0.44],
            Self::NutriaBrown => [0.3, 0.22, 0.15],
            Self::Incisor => [0.82, 0.46, 0.12],
        };
        Color::srgb(r, g, b)
    }
}

/// Una pieza rígida: una forma unitaria escalada a `size`, colocada en `at`
/// respecto de su articulación (o del cuerpo), en `paint`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Part {
    pub joint: Option<Joint>,
    pub shape: Shape,
    pub paint: Paint,
    pub at: Vec3,
    pub size: Vec3,
}

/// Las mallas y materiales compartidos con los que se dibuja todo animal.
#[derive(Resource, Debug, Clone)]
pub struct FaunaKit {
    meshes: Vec<(Shape, Handle<Mesh>)>,
    materials: Vec<(Paint, Handle<StandardMaterial>)>,
}

impl FaunaKit {
    pub fn new(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> Self {
        Self {
            meshes: Shape::ALL.map(|s| (s, meshes.add(s.mesh()))).to_vec(),
            materials: Paint::ALL
                .map(|p| {
                    let material = StandardMaterial {
                        base_color: p.color(),
                        perceptual_roughness: 0.9,
                        ..default()
                    };
                    (p, materials.add(material))
                })
                .to_vec(),
        }
    }

    fn mesh(&self, shape: Shape) -> Handle<Mesh> {
        self.meshes.iter().find(|m| m.0 == shape).unwrap().1.clone()
    }

    fn material(&self, paint: Paint) -> Handle<StandardMaterial> {
        self.materials
            .iter()
            .find(|m| m.0 == paint)
            .unwrap()
            .1
            .clone()
    }
}

impl Animal {
    /// Dónde se ubica cada articulación en el animal parado.
    pub fn joints(self) -> Vec<(Joint, Vec3)> {
        let v = Vec3::new;
        match self {
            Self::Nandu => vec![
                (Joint::Head, v(0.28, 1.0, 0.0)),
                (Joint::Leg(0), v(0.0, 0.85, -0.1)),
                (Joint::Leg(1), v(0.0, 0.85, 0.1)),
            ],
            Self::Mulita | Self::Peludo => {
                let s = self.armadillo_scale();
                [
                    (Joint::Head, v(0.15, 0.09, 0.0)),
                    (Joint::Leg(0), v(0.08, 0.07, -0.07)),
                    (Joint::Leg(1), v(0.08, 0.07, 0.07)),
                    (Joint::Leg(2), v(-0.08, 0.07, -0.07)),
                    (Joint::Leg(3), v(-0.08, 0.07, 0.07)),
                    (Joint::Tail, v(-0.16, 0.08, 0.0)),
                ]
                .map(|(j, p)| (j, p * s))
                .to_vec()
            }
            Self::Vizcacha => vec![
                (Joint::Head, v(0.22, 0.3, 0.0)),
                (Joint::Leg(0), v(0.14, 0.15, 0.0)),
                (Joint::Leg(2), v(-0.12, 0.18, 0.0)),
                (Joint::Tail, v(-0.24, 0.2, 0.0)),
            ],
            Self::Nutria => vec![
                (Joint::Head, v(0.26, 0.05, 0.0)),
                (Joint::Tail, v(-0.26, 0.0, 0.0)),
            ],
            Self::Hare => vec![
                (Joint::Head, v(0.18, 0.27, 0.0)),
                (Joint::Leg(0), v(0.12, 0.14, 0.0)),
                (Joint::Leg(2), v(-0.12, 0.14, 0.0)),
            ],
            Self::Tero => vec![
                (Joint::Head, v(0.13, 0.3, 0.0)),
                (Joint::Leg(0), v(0.0, 0.15, -0.03)),
                (Joint::Leg(1), v(0.0, 0.15, 0.03)),
                (Joint::Wing(0), v(0.02, 0.25, -0.06)),
                (Joint::Wing(1), v(0.02, 0.25, 0.06)),
            ],
            Self::Duck => vec![(Joint::Head, v(0.15, 0.15, 0.0))],
            // Se dibuja a partir de un modelo (ver `livestock`).
            _ => Vec::new(),
        }
    }

    /// Cuánto más grande que una mulita es el animal (el peludo).
    fn armadillo_scale(self) -> f32 {
        if self == Self::Peludo { 1.35 } else { 1.0 }
    }

    /// Todas las piezas rígidas.
    pub fn parts(self) -> Vec<Part> {
        let mut parts = Vec::new();
        let mut add = |joint, shape, paint, at: [f32; 3], size: [f32; 3]| {
            parts.push(Part {
                joint,
                shape,
                paint,
                at: Vec3::from(at),
                size: Vec3::from(size),
            });
        };
        use Paint as P;
        use Shape as S;
        match self {
            Self::Hare => {
                add(
                    None,
                    S::Ball,
                    P::HareBrown,
                    [0.0, 0.2, 0.0],
                    [0.42, 0.24, 0.22],
                );
                add(
                    None,
                    S::Ball,
                    P::White,
                    [-0.21, 0.22, 0.0],
                    [0.07, 0.07, 0.07],
                );
                let head = Some(Joint::Head);
                add(
                    head,
                    S::Ball,
                    P::HareBrown,
                    [0.06, 0.0, 0.0],
                    [0.15, 0.13, 0.12],
                );
                for z in [-0.03, 0.03] {
                    add(
                        head,
                        S::Block,
                        P::HareBrown,
                        [-0.01, 0.13, z],
                        [0.04, 0.2, 0.03],
                    );
                }
                add(
                    Some(Joint::Leg(0)),
                    S::Block,
                    P::HareBrown,
                    [0.0, -0.07, 0.0],
                    [0.04, 0.14, 0.1],
                );
                let hind = Some(Joint::Leg(2));
                add(
                    hind,
                    S::Ball,
                    P::HareBrown,
                    [0.0, 0.0, 0.0],
                    [0.18, 0.15, 0.2],
                );
                add(
                    hind,
                    S::Block,
                    P::HareBrown,
                    [0.04, -0.12, 0.0],
                    [0.14, 0.04, 0.1],
                );
            }
            Self::Tero => {
                let (back, belly, wing, legs) = (P::BirdGrey, P::Black, P::BirdGrey, P::BirdRed);
                add(None, S::Ball, back, [0.0, 0.24, 0.0], [0.3, 0.14, 0.13]);
                add(None, S::Ball, belly, [0.07, 0.21, 0.0], [0.13, 0.1, 0.11]);
                add(
                    None,
                    S::Block,
                    back,
                    [-0.18, 0.25, 0.0],
                    [0.12, 0.015, 0.08],
                );
                let head = Some(Joint::Head);
                add(head, S::Ball, back, [0.0, 0.0, 0.0], [0.09, 0.08, 0.08]);
                add(
                    head,
                    S::Block,
                    legs,
                    [0.06, -0.01, 0.0],
                    [0.05, 0.015, 0.015],
                );
                for k in 0..2 {
                    add(
                        Some(Joint::Leg(k)),
                        S::Rod,
                        legs,
                        [0.0, -0.07, 0.0],
                        [0.015, 0.15, 0.015],
                    );
                    let side = if k == 0 { -1.0 } else { 1.0 };
                    let at = [0.0, 0.0, 0.15 * side];
                    add(Some(Joint::Wing(k)), S::Block, wing, at, [0.16, 0.012, 0.3]);
                }
            }
            Self::Duck => {
                add(
                    None,
                    S::Ball,
                    P::DuckBrown,
                    [0.0, 0.06, 0.0],
                    [0.36, 0.16, 0.2],
                );
                add(
                    None,
                    S::Block,
                    P::DuckBrown,
                    [-0.18, 0.1, 0.0],
                    [0.08, 0.03, 0.08],
                );
                let head = Some(Joint::Head);
                add(
                    head,
                    S::Rod,
                    P::DuckBrown,
                    [-0.02, -0.03, 0.0],
                    [0.06, 0.1, 0.06],
                );
                add(
                    head,
                    S::Ball,
                    P::DuckGreen,
                    [0.0, 0.04, 0.0],
                    [0.11, 0.1, 0.09],
                );
                add(
                    head,
                    S::Block,
                    P::Bill,
                    [0.07, 0.03, 0.0],
                    [0.07, 0.02, 0.04],
                );
            }
            Self::Nandu => {
                // Cuerpo de plumas sueltas sobre patas largas, cuello largo y
                // cabeza chica.
                add(
                    None,
                    S::Ball,
                    P::NanduGrey,
                    [0.0, 0.98, 0.0],
                    [0.7, 0.48, 0.46],
                );
                add(
                    None,
                    S::Ball,
                    P::NanduGrey,
                    [-0.3, 0.92, 0.0],
                    [0.3, 0.3, 0.36],
                );
                let head = Some(Joint::Head);
                add(
                    head,
                    S::Rod,
                    P::BirdGrey,
                    [0.04, 0.24, 0.0],
                    [0.07, 0.5, 0.07],
                );
                add(head, S::Rod, P::Black, [0.02, 0.06, 0.0], [0.1, 0.14, 0.1]);
                add(
                    head,
                    S::Ball,
                    P::BirdGrey,
                    [0.08, 0.5, 0.0],
                    [0.12, 0.08, 0.08],
                );
                add(
                    head,
                    S::Block,
                    P::Horn,
                    [0.16, 0.49, 0.0],
                    [0.07, 0.02, 0.03],
                );
                for k in 0..2 {
                    let leg = Some(Joint::Leg(k));
                    add(leg, S::Rod, P::Horn, [0.0, -0.42, 0.0], [0.05, 0.85, 0.05]);
                    add(
                        leg,
                        S::Block,
                        P::Horn,
                        [0.05, -0.84, 0.0],
                        [0.14, 0.02, 0.06],
                    );
                }
            }
            Self::Mulita | Self::Peludo => {
                let shell = if self == Self::Mulita {
                    P::MulitaShell
                } else {
                    P::PeludoShell
                };
                let s = self.armadillo_scale();
                let mut add = |joint, shape, paint, at: [f32; 3], size: [f32; 3]| {
                    add(joint, shape, paint, at.map(|c| c * s), size.map(|c| c * s));
                };
                // El caparazón, con sus bandas, y la coraza de la cabeza.
                add(None, S::Ball, shell, [0.0, 0.11, 0.0], [0.34, 0.17, 0.2]);
                for x in [-0.03, 0.03] {
                    add(
                        None,
                        S::Block,
                        P::Black,
                        [x, 0.19, 0.0],
                        [0.012, 0.02, 0.15],
                    );
                }
                let head = Some(Joint::Head);
                add(
                    head,
                    S::Ball,
                    P::ArmadilloSkin,
                    [0.05, -0.02, 0.0],
                    [0.13, 0.06, 0.06],
                );
                add(head, S::Ball, shell, [0.02, 0.01, 0.0], [0.08, 0.04, 0.06]);
                for z in [-0.02, 0.02] {
                    add(
                        head,
                        S::Block,
                        P::ArmadilloSkin,
                        [-0.01, 0.04, z],
                        [0.015, 0.04, 0.015],
                    );
                }
                add(
                    Some(Joint::Tail),
                    S::Ball,
                    shell,
                    [-0.08, -0.03, 0.0],
                    [0.18, 0.03, 0.03],
                );
                for k in 0..4 {
                    let leg = Some(Joint::Leg(k));
                    add(
                        leg,
                        S::Block,
                        P::ArmadilloSkin,
                        [0.0, -0.04, 0.0],
                        [0.03, 0.07, 0.03],
                    );
                }
            }
            Self::Vizcacha => {
                // Gris, con la cara blanca cruzada por una franja negra.
                add(
                    None,
                    S::Ball,
                    P::VizcachaGrey,
                    [0.0, 0.22, 0.0],
                    [0.42, 0.28, 0.26],
                );
                add(
                    None,
                    S::Block,
                    P::White,
                    [0.06, 0.12, 0.0],
                    [0.2, 0.04, 0.16],
                );
                let head = Some(Joint::Head);
                add(
                    head,
                    S::Ball,
                    P::VizcachaGrey,
                    [0.05, 0.0, 0.0],
                    [0.17, 0.14, 0.15],
                );
                add(
                    head,
                    S::Block,
                    P::White,
                    [0.11, -0.02, 0.0],
                    [0.05, 0.06, 0.12],
                );
                add(
                    head,
                    S::Block,
                    P::Black,
                    [0.1, 0.02, 0.0],
                    [0.05, 0.03, 0.155],
                );
                for z in [-0.04, 0.04] {
                    add(
                        head,
                        S::Block,
                        P::VizcachaGrey,
                        [0.0, 0.08, z],
                        [0.04, 0.07, 0.015],
                    );
                }
                add(
                    Some(Joint::Tail),
                    S::Ball,
                    P::Black,
                    [-0.06, 0.0, 0.0],
                    [0.14, 0.05, 0.05],
                );
                add(
                    Some(Joint::Leg(0)),
                    S::Block,
                    P::VizcachaGrey,
                    [0.0, -0.07, 0.0],
                    [0.04, 0.14, 0.1],
                );
                let hind = Some(Joint::Leg(2));
                add(
                    hind,
                    S::Ball,
                    P::VizcachaGrey,
                    [0.0, 0.0, 0.0],
                    [0.18, 0.16, 0.24],
                );
                add(
                    hind,
                    S::Block,
                    P::VizcachaGrey,
                    [0.05, -0.15, 0.0],
                    [0.15, 0.04, 0.12],
                );
            }
            Self::Nutria => {
                // Casi sumergida: el lomo, la cabeza con sus incisivos
                // anaranjados y la cola.
                add(
                    None,
                    S::Ball,
                    P::NutriaBrown,
                    [0.0, 0.02, 0.0],
                    [0.5, 0.18, 0.26],
                );
                let head = Some(Joint::Head);
                add(
                    head,
                    S::Ball,
                    P::NutriaBrown,
                    [0.04, 0.0, 0.0],
                    [0.16, 0.11, 0.12],
                );
                add(
                    head,
                    S::Block,
                    P::Incisor,
                    [0.12, -0.03, 0.0],
                    [0.015, 0.03, 0.03],
                );
                for z in [-0.04, 0.04] {
                    add(
                        head,
                        S::Block,
                        P::NutriaBrown,
                        [-0.01, 0.06, z],
                        [0.03, 0.03, 0.02],
                    );
                }
                add(
                    Some(Joint::Tail),
                    S::Block,
                    P::Black,
                    [-0.14, -0.02, 0.0],
                    [0.3, 0.04, 0.05],
                );
            }
            // Se dibuja a partir de un modelo (ver `livestock`).
            _ => {}
        }
        parts
    }

    /// El rango compartido dentro del cual se dibujan sus mallas.
    pub fn range(self) -> VisibilityRange {
        let [buildings, props, _] = crate::settlement::ranges();
        if matches!(self, Self::Chimango | Self::Carancho) {
            buildings
        } else {
            props
        }
    }
}

/// El cuerpo de un animal dibujado, hijo de su entidad: gira hacia el rumbo y
/// contiene las articulaciones.
#[derive(Component, Debug, Clone)]
pub struct AnimalRig {
    joints: Vec<(Joint, Entity)>,
    /// Ciclos de zancada (o de aleteo) hasta ahora, acotados.
    phase: f32,
    /// Segundos desde que se dibujó, acotados, para cabezas y colas.
    clock: f32,
    seed: f32,
}

/// Una entidad de articulación de un animal.
#[derive(Component, Debug, Clone, Copy)]
pub struct AnimalJoint(pub Joint, pub Vec3);

pub struct FaunaPlugin;

impl Plugin for FaunaPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (load_kit, livestock::load_livestock))
            .add_systems(
                PostUpdate,
                (
                    attach_animals,
                    livestock::attach_livestock,
                    livestock::swap_in_livestock,
                    animate_animals,
                    livestock::drive_livestock,
                )
                    .chain()
                    .before(AnimationSystems)
                    .before(TransformSystems::Propagate),
            )
            .add_observer(livestock::dress_livestock);
    }
}

fn load_kit(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(FaunaKit::new(&mut meshes, &mut materials));
}

/// Dibuja cada animal nuevo.
fn attach_animals(
    mut commands: Commands,
    kit: Option<Res<FaunaKit>>,
    added: Query<(Entity, &Animal), Added<Animal>>,
) {
    let Some(kit) = kit else { return };
    for (entity, &animal) in &added {
        if animal.is_livestock() {
            continue;
        }
        let range = animal.range();
        let body = commands
            .spawn((Transform::default(), Visibility::default(), ChildOf(entity)))
            .id();
        let joints: Vec<(Joint, Entity)> = animal
            .joints()
            .into_iter()
            .map(|(joint, rest)| {
                let e = commands
                    .spawn((
                        AnimalJoint(joint, rest),
                        Transform::from_translation(rest),
                        Visibility::default(),
                        ChildOf(body),
                    ))
                    .id();
                (joint, e)
            })
            .collect();
        for part in animal.parts() {
            let holder = part
                .joint
                .and_then(|j| joints.iter().find(|(k, _)| *k == j))
                .map_or(body, |&(_, e)| e);
            commands.spawn((
                Mesh3d(kit.mesh(part.shape)),
                MeshMaterial3d(kit.material(part.paint)),
                Transform::from_translation(part.at).with_scale(part.size),
                range.clone(),
                ChildOf(holder),
            ));
        }
        let seed = (entity.index_u32() as f32 * 0.618).fract();
        commands.entity(body).insert(AnimalRig {
            joints,
            phase: seed,
            clock: seed * 10.0,
            seed,
        });
    }
}

/// Cuántos ciclos de zancada equivalen a un metro, por animal.
fn strides_per_meter(animal: Animal) -> f32 {
    match animal {
        Animal::Hare => 0.6,
        Animal::Vizcacha => 1.0,
        Animal::Nandu => 0.8,
        Animal::Mulita | Animal::Peludo => 4.0,
        _ => 2.0,
    }
}

/// Gira cada cuerpo hacia su rumbo y, cerca de la cámara, posiciona sus
/// articulaciones.
fn animate_animals(
    time: Res<Time>,
    cameras: Query<&GlobalTransform, With<Camera3d>>,
    animals: Query<(&Animal, &AnimalPose, &GlobalTransform)>,
    mut bodies: Query<(&ChildOf, &mut AnimalRig, &mut Transform), Without<AnimalJoint>>,
    mut joints: Query<(&AnimalJoint, &mut Transform), Without<AnimalRig>>,
) {
    let dt = time.delta_secs();
    let eye = cameras.iter().next().map(GlobalTransform::translation);
    for (child_of, mut rig, mut transform) in &mut bodies {
        let Ok((&animal, pose, at)) = animals.get(child_of.parent()) else {
            continue;
        };
        transform.rotation =
            Quat::from_rotation_y(pose.heading.radians()) * Quat::from_rotation_x(-pose.bank);
        if eye.is_some_and(|e| e.distance(at.translation()) > ANIMATE_FAR) {
            continue;
        }
        let cycles = match pose.gait {
            Gait::Fly => 5.0 * dt,
            Gait::Glide | Gait::Swim => 0.3 * dt,
            _ => pose.speed * dt * strides_per_meter(animal),
        };
        rig.phase = (rig.phase + cycles).fract();
        rig.clock = pampa_core::tick_clock(rig.clock, dt);
        let (phase, clock, seed) = (rig.phase * TAU, rig.clock, rig.seed);
        for &(_, entity) in &rig.joints {
            let Ok((joint, mut transform)) = joints.get_mut(entity) else {
                continue;
            };
            let (rotation, rise) =
                joint_pose(animal, pose.gait, joint.0, phase, clock + seed * 7.0);
            transform.rotation = rotation;
            transform.translation = joint.1 + rise;
        }
    }
}

/// El giro y la elevación de una articulación para `gait`, en el ángulo de
/// zancada `phase` y a los `clock` segundos.
pub fn joint_pose(
    animal: Animal,
    gait: Gait,
    joint: Joint,
    phase: f32,
    clock: f32,
) -> (Quat, Vec3) {
    let pitch = Quat::from_rotation_z;
    let moving = matches!(gait, Gait::Walk | Gait::Run);
    match joint {
        Joint::Head => {
            let angle = match gait {
                Gait::Graze if matches!(animal, Animal::Tero | Animal::Nandu) => {
                    -0.9 * (clock * 1.7).sin().max(0.0)
                }
                Gait::Graze => -1.0 + 0.08 * (clock * 2.0).sin(),
                Gait::Walk => -0.25 + 0.08 * (2.0 * phase).sin(),
                Gait::Swim => 0.06 * (clock * 1.3).sin(),
                _ => 0.05 * (clock * 0.7).sin(),
            };
            (pitch(angle), Vec3::ZERO)
        }
        Joint::Leg(k) => {
            if matches!(gait, Gait::Fly | Gait::Glide) {
                return (pitch(1.3), Vec3::ZERO);
            }
            if !moving {
                return (Quat::IDENTITY, Vec3::ZERO);
            }
            // Las patas de un ave, por turnos; el salto de una liebre: los
            // pares juntos.
            // El trote de un armadillo: los pares en diagonal juntos.
            let offset = match (animal, k) {
                (Animal::Hare | Animal::Vizcacha, k) => (k / 2) as f32 * std::f32::consts::PI,
                (Animal::Mulita | Animal::Peludo, 0 | 3) => 0.0,
                (Animal::Mulita | Animal::Peludo, _) => std::f32::consts::PI,
                (_, 0) => 0.0,
                _ => std::f32::consts::PI,
            };
            let swing = if gait == Gait::Run { 0.7 } else { 0.35 };
            (pitch(swing * (phase + offset).sin()), Vec3::ZERO)
        }
        Joint::Tail => {
            let swish = 0.3 * (clock * 1.3).sin() * (clock * 0.31).sin();
            (Quat::from_rotation_x(swish), Vec3::ZERO)
        }
        Joint::Wing(k) => {
            let side = if k == 0 { -1.0 } else { 1.0 };
            let beat = match gait {
                Gait::Fly => 0.9 * phase.sin(),
                Gait::Glide => 0.12 + 0.04 * phase.sin(),
                // Plegadas hacia atrás a lo largo del cuerpo.
                _ => return (Quat::from_rotation_y(side * FRAC_PI_2), Vec3::ZERO),
            };
            (Quat::from_rotation_x(side * beat), Vec3::ZERO)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Las especies dibujadas a partir de piezas rígidas.
    const RIGID: [Animal; 8] = [
        Animal::Hare,
        Animal::Tero,
        Animal::Duck,
        Animal::Nandu,
        Animal::Mulita,
        Animal::Peludo,
        Animal::Vizcacha,
        Animal::Nutria,
    ];

    /// Las especies dibujadas a partir de modelos.
    const LIVESTOCK: [Animal; 13] = [
        Animal::Cow(CowCoat::Hereford),
        Animal::Cow(CowCoat::Angus),
        Animal::Cow(CowCoat::Overo),
        Animal::Bull(CowCoat::Hereford),
        Animal::Horse(Coat::Zaino),
        Animal::Horse(Coat::Overo),
        Animal::Horse(Coat::Bayo),
        Animal::Zorro,
        Animal::GatoMontes,
        Animal::Puma,
        Animal::Yaguarete,
        Animal::Chimango,
        Animal::Carancho,
    ];

    fn triangles(animal: Animal) -> usize {
        animal
            .parts()
            .iter()
            .map(|p| p.shape.mesh().triangles().map(|t| t.count()).unwrap_or(0))
            .sum()
    }

    #[test]
    fn every_rigid_species_fits_its_triangle_and_part_budget() {
        for animal in RIGID {
            let (tris, parts) = (triangles(animal), animal.parts().len());
            let budget = match animal {
                Animal::Duck | Animal::Nutria => 300,
                _ => 500,
            };
            println!("{animal:?}: {tris} triangles, {parts} parts");
            assert!(tris <= budget, "{animal:?}: {tris} > {budget}");
            assert!(parts <= 24, "{animal:?}: {parts} parts");
            for part in animal.parts() {
                let joint = part.joint;
                assert!(joint.is_none_or(|j| animal.joints().iter().any(|(k, _)| *k == j)));
            }
        }
        for animal in LIVESTOCK {
            assert!(animal.is_livestock());
            assert!(animal.parts().is_empty() && animal.joints().is_empty());
        }
        assert!(RIGID.iter().all(|a| !a.is_livestock()));
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .add_plugins(FaunaPlugin);
        app.update();
        app
    }

    #[test]
    fn animals_share_a_few_meshes_materials_and_the_settlements_ranges() {
        let mut app = app();
        for (k, animal) in RIGID.into_iter().chain(LIVESTOCK).enumerate() {
            for n in 0..3 {
                app.world_mut().spawn((
                    animal,
                    Transform::from_xyz(k as f32 * 5.0, 0.0, n as f32 * 4.0),
                ));
            }
        }
        app.update();
        let mut q = app
            .world_mut()
            .query::<(&Mesh3d, &MeshMaterial3d<StandardMaterial>, &VisibilityRange)>();
        let drawn: Vec<_> = q.iter(app.world()).collect();
        let expected: usize = RIGID.iter().map(|a| 3 * a.parts().len()).sum();
        assert_eq!(drawn.len(), expected);
        let meshes: std::collections::HashSet<_> = drawn.iter().map(|d| d.0.0.id()).collect();
        let materials: std::collections::HashSet<_> = drawn.iter().map(|d| d.1.0.id()).collect();
        assert!(meshes.len() <= Shape::ALL.len());
        assert!(materials.len() <= Paint::ALL.len());
        let shared = crate::settlement::ranges();
        let mut ranges: Vec<&VisibilityRange> = Vec::new();
        for (_, _, range) in &drawn {
            assert!(shared.contains(range), "a new range");
            if !ranges.contains(range) {
                ranges.push(range);
            }
        }
        assert_eq!(ranges.len(), 1, "props'");
        // Las rapaces, en lo alto, se ven hasta el rango de los edificios.
        for raptor in [Animal::Chimango, Animal::Carancho] {
            assert!(raptor.range() == shared[0]);
        }
        // Cada vaca, toro y caballo espera su modelo (aquí no se carga
        // ninguno).
        let mut models = app.world_mut().query::<(&LivestockModel, &ChildOf)>();
        assert_eq!(models.iter(app.world()).count(), 3 * LIVESTOCK.len());
    }

    #[test]
    fn birds_peck_and_flap_and_hares_bound() {
        let angle = |q: Quat| q.to_euler(EulerRot::ZYX).0;
        let (head, _) = joint_pose(Animal::Tero, Gait::Graze, Joint::Head, 0.0, 0.9);
        assert!(angle(head) < -0.5, "pecking");
        let a = joint_pose(Animal::Tero, Gait::Walk, Joint::Leg(0), 1.0, 0.0).0;
        let b = joint_pose(Animal::Tero, Gait::Walk, Joint::Leg(1), 1.0, 0.0).0;
        assert!(angle(a) * angle(b) < 0.0, "legs step in turn");
        let fore = joint_pose(Animal::Hare, Gait::Run, Joint::Leg(0), 1.0, 0.0).0;
        let hind = joint_pose(Animal::Hare, Gait::Run, Joint::Leg(2), 1.0, 0.0).0;
        assert!(angle(fore) * angle(hind) < 0.0, "a bound");
        let up = joint_pose(Animal::Tero, Gait::Fly, Joint::Wing(1), FRAC_PI_2, 0.0).0;
        let down = joint_pose(Animal::Tero, Gait::Fly, Joint::Wing(1), -FRAC_PI_2, 0.0).0;
        assert!(up.angle_between(down) > 1.5, "wings beat");
        // Un armadillo trota: las patas en diagonal juntas, las de un mismo
        // lado alternadas.
        let leg = |k| angle(joint_pose(Animal::Peludo, Gait::Walk, Joint::Leg(k), 1.0, 0.0).0);
        assert!((leg(0) - leg(3)).abs() < 1e-6 && (leg(1) - leg(2)).abs() < 1e-6);
        assert!(leg(0) * leg(2) < 0.0, "a trot");
    }
}
