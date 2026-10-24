//! The pampa's animals.
//!
//! The game simulates them (see `pampa_core::fauna`) and hands each entity
//! an [`Animal`] (what it is), an [`AnimalPose`] (where it faces and what
//! it does) and, optionally, an [`AnimalSeed`] (its stable identity); its
//! `Transform` carries translation only, on the ground, the water or up in
//! the air.
//!
//! - Cattle (cows and bulls) and the corral horses are skinned, animated
//!   glTF models whose clips follow the fauna state (see [`livestock`]).
//! - The small wild animals are drawn like the procedural horses: rigid
//!   low-poly parts hung off a few joints, so animation only turns joints
//!   (legs swing, heads go down to peck or bob, tails swish, wings flap),
//!   from one [`FaunaKit`] of four shared unit meshes, scaled per part, and
//!   one shared material per [`Paint`]. Past [`ANIMATE_FAR`] from the
//!   camera they keep their last pose (only the body turns).
//!
//! Every mesh is drawn within the settlements' shared ranges (ground
//! animals the props', soaring birds the buildings'): no new
//! `VisibilityRange`.

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

/// What an animal is.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[require(AnimalPose, Transform, Visibility)]
pub enum Animal {
    Cow(CowCoat),
    /// A herd's bull, in its breed's coat.
    Bull(CowCoat),
    /// A loose horse standing in a corral.
    Horse(Coat),
    Hare,
    /// A tero (southern lapwing).
    Tero,
    /// A chimango or carancho.
    Raptor,
    Duck,
}

impl Animal {
    /// Whether it is drawn from an animated model (see [`livestock`])
    /// rather than rigid parts.
    pub fn is_livestock(self) -> bool {
        Species::of(self).is_some()
    }
}

/// An animal's stable identity, from its seeded place in the world: picks
/// its idle and desyncs its clips from its neighbours'. Without it the
/// entity's index stands in.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AnimalSeed(pub u32);

/// What an animal is doing, as drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Gait {
    #[default]
    Stand,
    /// Head down in the grass (or pecking).
    Graze,
    Walk,
    /// Bolting or hopping fast.
    Run,
    /// Flapping.
    Fly,
    /// Soaring on still wings.
    Glide,
    Swim,
}

/// Logic state the drawing needs, copied from `pampa-core` by the game.
#[derive(Component, Debug, Clone, Copy, PartialEq, Default)]
pub struct AnimalPose {
    pub heading: Heading,
    pub gait: Gait,
    /// Ground speed, in m/s: paces the legs.
    pub speed: f32,
    /// Roll into a turn, radians (positive: left wing down).
    pub bank: f32,
}

/// Past this distance from the camera an animal's joints stop moving.
pub const ANIMATE_FAR: f32 = 80.0;

/// The joints of an animal: the body holds all the others.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Joint {
    Head,
    /// Legs: a hare's front and hind pairs are `Leg(0)` and `Leg(2)`; a
    /// bird's two `Leg(0)` and `Leg(1)`.
    Leg(u8),
    Tail,
    /// Left (`0`, -Z) and right (`1`, +Z).
    Wing(u8),
}

/// A shared unit mesh.
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

/// A shared material.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Paint {
    White,
    Black,
    Horn,
    HareBrown,
    BirdGrey,
    BirdRed,
    RaptorBrown,
    DuckBrown,
    DuckGreen,
    Bill,
}

impl Paint {
    pub const ALL: [Self; 10] = [
        Self::White,
        Self::Black,
        Self::Horn,
        Self::HareBrown,
        Self::BirdGrey,
        Self::BirdRed,
        Self::RaptorBrown,
        Self::DuckBrown,
        Self::DuckGreen,
        Self::Bill,
    ];

    fn color(self) -> Color {
        let [r, g, b] = match self {
            Self::White => [0.82, 0.8, 0.74],
            Self::Black => [0.05, 0.045, 0.04],
            Self::Horn => [0.72, 0.66, 0.52],
            Self::HareBrown => [0.42, 0.33, 0.22],
            Self::BirdGrey => [0.5, 0.48, 0.44],
            Self::BirdRed => [0.62, 0.18, 0.14],
            Self::RaptorBrown => [0.33, 0.24, 0.15],
            Self::DuckBrown => [0.36, 0.28, 0.18],
            Self::DuckGreen => [0.08, 0.2, 0.12],
            Self::Bill => [0.55, 0.45, 0.12],
        };
        Color::srgb(r, g, b)
    }
}

/// One rigid part: a unit shape scaled to `size`, placed at `at` from its
/// joint (or the body), in `paint`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Part {
    pub joint: Option<Joint>,
    pub shape: Shape,
    pub paint: Paint,
    pub at: Vec3,
    pub size: Vec3,
}

/// The shared meshes and materials every animal is drawn with.
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
    /// Where each joint sits on the standing animal.
    pub fn joints(self) -> Vec<(Joint, Vec3)> {
        let v = Vec3::new;
        match self {
            // Drawn from a model (see `livestock`).
            Self::Cow(_) | Self::Bull(_) | Self::Horse(_) => Vec::new(),
            Self::Hare => vec![
                (Joint::Head, v(0.18, 0.27, 0.0)),
                (Joint::Leg(0), v(0.12, 0.14, 0.0)),
                (Joint::Leg(2), v(-0.12, 0.14, 0.0)),
            ],
            Self::Tero | Self::Raptor => {
                let s = self.bird_scale();
                [
                    (Joint::Head, v(0.13, 0.3, 0.0)),
                    (Joint::Leg(0), v(0.0, 0.15, -0.03)),
                    (Joint::Leg(1), v(0.0, 0.15, 0.03)),
                    (Joint::Wing(0), v(0.02, 0.25, -0.06)),
                    (Joint::Wing(1), v(0.02, 0.25, 0.06)),
                ]
                .map(|(j, p)| (j, p * s))
                .to_vec()
            }
            Self::Duck => vec![(Joint::Head, v(0.15, 0.15, 0.0))],
        }
    }

    fn bird_scale(self) -> f32 {
        if self == Self::Raptor { 2.4 } else { 1.0 }
    }

    /// Every rigid part.
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
            Self::Cow(_) | Self::Bull(_) | Self::Horse(_) => {}
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
            Self::Tero | Self::Raptor => {
                let raptor = self == Self::Raptor;
                let (back, belly, wing, legs) = if raptor {
                    (P::RaptorBrown, P::Horn, P::RaptorBrown, P::Bill)
                } else {
                    (P::BirdGrey, P::Black, P::BirdGrey, P::BirdRed)
                };
                let s = self.bird_scale();
                let mut add = |joint, shape, paint, at: [f32; 3], size: [f32; 3]| {
                    add(joint, shape, paint, at.map(|c| c * s), size.map(|c| c * s));
                };
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
        }
        parts
    }

    /// The shared range its meshes are drawn within.
    pub fn range(self) -> VisibilityRange {
        let [buildings, props, _] = crate::settlement::ranges();
        if self == Self::Raptor {
            buildings
        } else {
            props
        }
    }
}

/// The body of a drawn animal, child of its entity: it turns to the
/// heading and holds the joints.
#[derive(Component, Debug, Clone)]
pub struct AnimalRig {
    joints: Vec<(Joint, Entity)>,
    /// Stride (or wingbeat) cycles so far, wrapped.
    phase: f32,
    /// Seconds since drawn, wrapped, for heads and tails.
    clock: f32,
    seed: f32,
}

/// A joint entity of an animal.
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

/// Draws every new animal.
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

/// How many stride cycles a meter is, by animal.
fn strides_per_meter(animal: Animal) -> f32 {
    match animal {
        Animal::Hare => 0.6,
        _ => 2.0,
    }
}

/// Turns each body to its heading and, near the camera, poses its joints.
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

/// One joint's turn and rise for `gait`, at stride angle `phase` and
/// `clock` seconds.
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
                Gait::Graze if animal == Animal::Tero => -0.9 * (clock * 1.7).sin().max(0.0),
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
            // A bird's legs in turn; a hare's bound: pairs together.
            let offset = match (animal, k) {
                (Animal::Hare, k) => (k / 2) as f32 * std::f32::consts::PI,
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
                // Folded back along the body.
                _ => return (Quat::from_rotation_y(side * FRAC_PI_2), Vec3::ZERO),
            };
            (Quat::from_rotation_x(side * beat), Vec3::ZERO)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The species drawn from rigid parts.
    const RIGID: [Animal; 4] = [Animal::Hare, Animal::Tero, Animal::Raptor, Animal::Duck];

    /// The species drawn from models.
    const LIVESTOCK: [Animal; 7] = [
        Animal::Cow(CowCoat::Hereford),
        Animal::Cow(CowCoat::Angus),
        Animal::Cow(CowCoat::Overo),
        Animal::Bull(CowCoat::Hereford),
        Animal::Horse(Coat::Zaino),
        Animal::Horse(Coat::Overo),
        Animal::Horse(Coat::Bayo),
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
                Animal::Duck => 300,
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
        assert_eq!(ranges.len(), 2, "props' and buildings'");
        // Each cow, bull and horse waits for its model (none loads here).
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
    }
}
