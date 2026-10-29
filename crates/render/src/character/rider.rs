//! The rider on the model horse: a MakeHuman model (see
//! [`model`](super::model)) seated on the horse's `Torso` bone (see
//! [`horse`](super::horse)), so it rises, pitches and turns with the
//! horse's own clip and keeps in step with it for free.
//!
//! It plays the shared `drive` clip (seated, hands forward on the reins:
//! read better than `sit_idle`, whose hands rest in the lap), and
//! [`pose_riders`] fits it to the horse after the animation systems, every
//! overlay worked out afresh from the clip's pose, with no allocation:
//!
//! - **walking** it sits deep in the recado, upright and relaxed, its seat
//!   going with the horse's back (it hangs from it) while the spine counters
//!   the saddle's pitch and roll ([`saddle_attitude`]), late
//!   ([`COUNTER`]), so the upper body stays quiet;
//! - **galloping** it rides the half-seat ([`HALF_SEAT`]): the seat hovers
//!   over the saddle, forward over the knees, and does not ride the
//!   saddle's bounce (it only just touches at the top of a stride, never
//!   below); the legs take it, the knees and hips opening and closing and
//!   the heels sinking ([`HEEL`]); the rider leans about 20° from the hips,
//!   the back flat ([`LEAN`], [`HIP_SHARE`]), and back when braking;
//! - **over a jump** it keeps its balance: it holds the half-seat, folds
//!   forward from the hips as the horse takes off and over the top
//!   ([`JUMP_FOLD`], [`jump_fold`]), taking the horse's pitch since it
//!   left the ground at the hips rather than the spine ([`JUMP_BALANCE`]),
//!   the rein hand forward and down along the neck
//!   ([`JUMP_HAND`]); landing, it sits up and the knees and hips give, the
//!   seat coming down to the saddle and never into it ([`landing`],
//!   [`LANDING_GIVE`]);
//! - the head stays level and looks ahead, steadied against the lean and
//!   the saddle's pitch;
//! - each leg is bent round the barrel by a two-bone IK ([`two_bone`]): the
//!   ankle to its stirrup ([`ANKLES`]), the knee out and forward
//!   ([`KNEE_POLE`]);
//! - the left hand holds both reins (the gaucho's way, the right free for
//!   the rebenque or the lazo) in front of the pommel walking
//!   ([`REINS_HAND`]), over the withers galloping ([`GALLOP_HAND`]), and
//!   follows the horse's mouth so the contact stays even ([`FOLLOW`]): the
//!   elbow opens as the head goes forward and down. The right hand rests by
//!   its thigh ([`THIGH_HAND`]), loose, swinging late with the horse (a
//!   little walking, more and lifting at the gallop), forward braking, and
//!   now and then up to the hat's brim standing still ([`free_hand`]); both
//!   shoulders shrug late with the saddle ([`SHRUG`]);
//! - standing still a few seconds ([`STILL`]), the rein hand settles on the
//!   pommel, the weight shifts now and then and the head glances around.
//!
//! The lags are critically damped springs ([`Spring`]), the same at any
//! frame rate.
//!
//! The targets are in the rider model's own space: +Z its front, +X its
//! left, +Y up, from the pelvis; they were measured on the horse's barrel
//! (0.47 m across at the seat, the belly at 0.86 m) and the protagonist's
//! legs (0.865 m hip to ankle). Once the rider is seated
//! ([`seat_riders`]), the reins run to its hands and the stirrups go on
//! its feet ([`carry_stirrups`]: they stay under the horse's torso, so a
//! respawned rider scene leaves the tack whole).

use std::f32::consts::FRAC_PI_2;

use bevy::math::Affine3A;
use bevy::prelude::*;

use super::figure::FigureKind;
use super::horse::{HorseBody, HorseModel, transform_of};
use super::model::{CharacterModel, CharacterModels, ModelPending};
use super::rig::Rig;
use super::tack::{STIRRUP, Strap, Tack};

/// A rider's model, seated on a horse: on the model's root, child of the
/// horse's `Torso` bone.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Riding {
    /// The horse's body.
    pub horse: Entity,
    pub kind: FigureKind,
    pub seed: u32,
}

/// How far above the top of the pellón the rider's pelvis joint sits: the
/// seat of the protagonist's breeches is 0.147 m below it, pressed a
/// little into the sheepskin.
pub const PELVIS_RISE: f32 = 0.13;

/// Where a rider's model is placed in the horse's body frame for a seat on
/// top of the pellón at `seat`: its pelvis held at its origin
/// ([`pose_riders`]) just above it, facing the horse's front (the glTF
/// faces +Z).
pub fn seat_transform(seat: Vec3) -> Affine3A {
    Affine3A::from_rotation_translation(
        Quat::from_rotation_y(FRAC_PI_2),
        seat + Vec3::Y * PELVIS_RISE,
    )
}

/// Where the ankles go, left then right, from the pelvis (rider space):
/// in the stirrups, 0.3 m out either side (the barrel is 0.23 m from the
/// spine at its widest, and the boots must clear the flaps), 10 cm ahead,
/// the boot soles about 12 cm below the belly.
pub const ANKLES: [Vec3; 2] = [Vec3::new(0.32, -0.69, 0.1), Vec3::new(-0.32, -0.69, 0.1)];

/// Where the ankles go in the half-seat: the stirrups taken up a hand
/// (the leathers follow the feet) and the heels under the hips, so the
/// knees bend about 110°.
pub const GALLOP_ANKLES: [Vec3; 2] = [Vec3::new(0.31, -0.6, 0.02), Vec3::new(-0.31, -0.6, 0.02)];

/// Where the rein hand's wrist goes (the left: a gaucho rides one-handed,
/// both reins in it): in front of the pommel, a little to its side, so the
/// arm stays clear of the poncho folded over that shoulder.
pub const REINS_HAND: Vec3 = Vec3::new(0.12, 0.18, 0.32);

/// Where it rests after a few seconds standing still: on the pommel.
pub const POMMEL_HAND: Vec3 = Vec3::new(0.06, -0.02, 0.22);

/// Where the free (right) hand rests: on its thigh, as if loosely holding
/// a rebenque, the elbow bent about 50°.
pub const THIGH_HAND: Vec3 = Vec3::new(-0.2, 0.08, 0.1);

/// Where it goes, now and then, standing still: to the hat's brim.
pub const HAT_HAND: Vec3 = Vec3::new(-0.1, 0.62, 0.16);

/// How far the free hand swings from its rest at most, meters.
pub const FREE_REACH: f32 = 0.12;

/// How far the shoulders shrug with the saddle's motion, radians a radian.
pub const SHRUG: f32 = 0.6;

/// How long a horse stands still before its rider's hands settle, s.
pub const STILL: f32 = 3.0;

// How fast each of the rider's springs ([`Spring::step`]) follows, rad/s:
// the higher, the sooner it catches up (it closes most of a step in about
// 4 / ω seconds).

/// The saddle's smoothed path, whose remainder (its bounce) the rider
/// gives with.
pub const SADDLE_EASE: f32 = 4.0;
/// The braking lean coming in and going.
pub const BRAKING_EASE: f32 = 6.0;
/// The hands settling on the pommel standing still: slow.
pub const REST_EASE: f32 = 2.5;
/// The half-seat coming in with the gallop, late.
pub const HALF_SEAT_EASE: f32 = 3.0;
/// The lean of the upper body.
pub const LEAN_EASE: f32 = 4.0;
/// The spine countering the saddle's pitch and roll. Fast on purpose: the
/// counter only has to lag the saddle by a frame or two to keep the upper
/// body steadier than the hips; a slow one would counter the last stride
/// and rock the rider instead.
pub const COUNTER_EASE: f32 = 40.0;
/// The bit's smoothed path, whose remainder the rein hand gives with.
pub const BIT_EASE: f32 = 3.0;
/// The head's late part of the levelling.
pub const HEAD_EASE: f32 = 8.0;
/// The shoulders' shrug.
pub const SHRUG_EASE: f32 = 8.0;
/// The free hand: a loose arm lagging off the torso.
pub const FREE_EASE: f32 = 7.0;

/// The deceleration that is full braking, m/s²: the leaning back is the
/// horse's deceleration over it. Coupled to the logic: it must match
/// `pampa_core::RiderParams::braking` (the player's braking).
pub const BRAKING: f32 = 8.0;

/// Below this ground speed a horse stands still, m/s.
pub const STANDING: f32 = 0.05;

/// The speed that is a full walk, m/s, for the walking seat and the free
/// arm's sway. Coupled to the logic: `pampa_core::RiderParams::walk_max`.
pub const WALK_TOP: f32 = 3.0;

/// The weight shift standing still: its period, in seconds, is the first
/// plus the rider's seed modulo the second (7–9 s), and its amplitude, in
/// radians, the third.
pub const SHIFT: (f32, u32, f32) = (7.0, 3, 0.035);

/// How far the rein hand gives with the bit at most, meters.
pub const GIVE_MAX: f32 = 0.12;

/// How far the back rounds or hollows at most, radians.
pub const FLEX_MAX: f32 = 0.12;

/// How the free hand swings with the saddle's motion beyond its smoothed
/// path: each row (the hand's x, y, z) weighs the saddle's rise, pitch and
/// roll, meters a meter or a radian.
pub const FREE_SWING: [[f32; 3]; 3] = [[0.15, 0.0, -0.8], [-1.6, 0.3, 0.0], [0.8, 1.2, 0.0]];

/// How much more the free arm swings walking than galloping (it swings
/// with less motion walking), and how far toward the pommel it comes
/// braking (a share of the way).
pub const FREE_WALK: f32 = 2.0;
pub const FREE_BRAKE: f32 = 0.45;

/// The hat-brim touch: its seed salt (so it does not glance with the head)
/// and its clock rate (much rarer than a glance).
pub const HAT_SALT: u32 = 0x5eed;
pub const HAT_PACE: f32 = 0.4;

/// The widest glance `pampa_core::pose::glance` gives, radians
/// (0.3 + 0.25): a glance this wide is the whole way to the hat's brim.
/// Coupled to that function.
pub const GLANCE_MAX: f32 = 0.55;

/// Which way the knees bend, left then right: forward and out round the
/// barrel.
pub const KNEE_POLE: [Vec3; 2] = [Vec3::new(0.6, -0.1, 1.0), Vec3::new(-0.6, -0.1, 1.0)];

/// Which way the elbows bend: down, out and back (the rein elbow well out,
/// over the poncho's hanging end).
pub const ELBOW_POLE: [Vec3; 2] = [Vec3::new(1.2, -1.0, -0.6), Vec3::new(-0.5, -1.0, -0.6)];

/// How far the rider leans forward in the half-seat, at the walk, and back
/// braking as hard as logic brakes (8 m/s²), radians: on top of the seated
/// clip's own lean back, about 20° from the vertical in the half-seat.
pub const LEAN: (f32, f32, f32) = (0.5, 0.03, 0.12);

/// How much of the lean is at the hips: the back stays flat.
pub const HIP_SHARE: f32 = 0.7;

/// The half-seat at the gallop: how far the seat hovers over the saddle
/// (before the saddle's bounce is taken out: at the top of a stride it
/// just touches) and how far forward over the knees it goes, meters.
pub const HALF_SEAT: Vec3 = Vec3::new(0.0, 0.06, 0.1);

/// How far the heels sink in the half-seat, radians (more as the seat
/// comes down on the stirrups).
pub const HEEL: f32 = 0.2;

/// Where the rein hand goes in the half-seat: forward over the withers and
/// a little above, clear of the horse's neck.
pub const GALLOP_HAND: Vec3 = Vec3::new(0.04, 0.22, 0.4);

/// How much of the bit's motion the rein hand follows, walking and in the
/// half-seat: the contact stays even.
pub const FOLLOW: (f32, f32) = (0.35, 0.85);

/// How much of the saddle's pitch and roll (beyond their smoothed path)
/// the spine counters, so the upper body stays steadier than the hips.
pub const COUNTER: (f32, f32) = (0.95, 0.6);
/// How far the back rounds as the saddle comes up under it at the walk,
/// radians a meter.
pub const FLEX: f32 = 1.2;

/// How far the rider folds forward from the hips over a jump, radians, on
/// top of the half-seat's lean ([`jump_fold`]).
pub const JUMP_FOLD: f32 = 0.4;

/// Where the rein hand goes over a jump, on top of where it is: forward
/// and down along the horse's neck, giving it its head (a crest release),
/// meters at the full fold.
pub const JUMP_HAND: Vec3 = Vec3::new(0.0, -0.06, 0.16);

/// How much of the horse's pitch over a jump (since it left the ground)
/// the rider takes at the hips: his upper body keeps its balance.
pub const JUMP_BALANCE: f32 = 0.9;

/// The fold coming in and going: quick, but never a snap.
pub const JUMP_EASE: f32 = 14.0;

/// How long the landing takes the rider, seconds ([`landing`]): he sits up
/// and sinks into the saddle through the knees and hips, then rides on.
pub const LANDING: f32 = 0.5;

/// How far he sits up on landing, radians against the half-seat's lean,
/// and how much of the half-seat's hover his knees and hips give at the
/// bottom of it.
pub const LANDING_GIVE: (f32, f32) = (0.25, 1.0);

/// How far forward the rider is folded (`0..1`) `flight` (`0..=1`) of the
/// way through his horse's jump: folding as the horse takes off, folded
/// over the top, and sitting up again for the landing.
pub fn jump_fold(flight: f32) -> f32 {
    let smooth = |u: f32| {
        let u = u.clamp(0.0, 1.0);
        u * u * (3.0 - 2.0 * u)
    };
    smooth(flight / 0.1) * (1.0 - smooth((flight - 0.7) / 0.3))
}

/// How much of the landing (`0..1`) the rider takes `since` seconds after
/// his horse lands: none at the touchdown, all of it a fifth of a second
/// on, none again after [`LANDING`].
pub fn landing(since: f32) -> f32 {
    if !(0.0..LANDING).contains(&since) {
        return 0.0;
    }
    (std::f32::consts::PI * since / LANDING).sin().powi(2)
}

/// Where on the hand bone the reins are held (the palm), its own space.
pub const PALM: Vec3 = Vec3::new(0.0, 0.05, 0.0);

/// The rider's bones the overlays move, in the model's hierarchy order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bone {
    Root,
    Pelvis,
    Spine1,
    Spine2,
    Spine3,
    Neck,
    Head,
    Clavicle(usize),
    UpperArm(usize),
    LowerArm(usize),
    Hand(usize),
    Thigh(usize),
    Calf(usize),
    Foot(usize),
    Ball(usize),
}

impl Bone {
    pub const COUNT: usize = 23;

    /// Every bone, each after its parent.
    pub const ALL: [Self; Self::COUNT] = [
        Self::Root,
        Self::Pelvis,
        Self::Spine1,
        Self::Spine2,
        Self::Spine3,
        Self::Neck,
        Self::Head,
        Self::Clavicle(0),
        Self::Clavicle(1),
        Self::UpperArm(0),
        Self::UpperArm(1),
        Self::LowerArm(0),
        Self::LowerArm(1),
        Self::Hand(0),
        Self::Hand(1),
        Self::Thigh(0),
        Self::Thigh(1),
        Self::Calf(0),
        Self::Calf(1),
        Self::Foot(0),
        Self::Foot(1),
        Self::Ball(0),
        Self::Ball(1),
    ];

    /// Its node's name in a `game_engine` model (0 is the left).
    pub fn name(self) -> String {
        let side = |s: usize| if s == 0 { "l" } else { "r" };
        match self {
            Self::Root => "Root".into(),
            Self::Pelvis => "pelvis".into(),
            Self::Spine1 => "spine_01".into(),
            Self::Spine2 => "spine_02".into(),
            Self::Spine3 => "spine_03".into(),
            Self::Neck => "neck_01".into(),
            Self::Head => "head".into(),
            Self::Clavicle(s) => format!("clavicle_{}", side(s)),
            Self::UpperArm(s) => format!("upperarm_{}", side(s)),
            Self::LowerArm(s) => format!("lowerarm_{}", side(s)),
            Self::Hand(s) => format!("hand_{}", side(s)),
            Self::Thigh(s) => format!("thigh_{}", side(s)),
            Self::Calf(s) => format!("calf_{}", side(s)),
            Self::Foot(s) => format!("foot_{}", side(s)),
            Self::Ball(s) => format!("ball_{}", side(s)),
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|b| *b == self).unwrap_or(0)
    }
}

/// A seated rider's bones, found when its scene spawns; on the model's
/// root next to its [`Rig`].
#[derive(Component, Debug, Clone)]
pub struct Seated {
    /// In [`Bone::ALL`] order.
    pub bones: [Entity; Bone::COUNT],
    /// Each stirrup's placement on its foot bone, left then right: its
    /// tread under the ball of the foot, its ring standing across it.
    pub stirrups: [Transform; 2],
}

impl Seated {
    fn bone(&self, bone: Bone) -> Entity {
        self.bones[bone.index()]
    }

    /// The bones of the model under `root` (`named` finds a node by name),
    /// with the stirrups placed from its rest pose. `None` without the
    /// whole skeleton.
    pub fn find(
        root: Entity,
        named: impl Fn(&str) -> Option<Entity>,
        rest: impl Fn(Entity) -> Affine3A,
    ) -> Option<Self> {
        let mut bones = [Entity::PLACEHOLDER; Bone::COUNT];
        for (k, bone) in Bone::ALL.into_iter().enumerate() {
            bones[k] = named(&bone.name())?;
        }
        let seated = Self {
            bones,
            stirrups: [Transform::IDENTITY; 2],
        };
        // In the model's own space, standing at rest.
        let to_model = rest(root).inverse();
        let at = |b: Bone| to_model * rest(seated.bone(b));
        let stirrups = [0, 1].map(|s| {
            let (foot, ball) = (at(Bone::Foot(s)), at(Bone::Ball(s)));
            let tread = Vec3::new(
                ball.translation.x,
                STIRRUP.y / 2.0 - 0.01,
                (foot.translation.z + ball.translation.z) / 2.0,
            );
            let ring = Affine3A::from_scale_rotation_translation(STIRRUP, Quat::IDENTITY, tread);
            transform_of(foot.inverse() * ring)
        });
        Some(Self { stirrups, ..seated })
    }
}

/// A bone's placement in the model's space: no scale (the `game_engine`
/// bones have none).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Place {
    pub rotation: Quat,
    pub translation: Vec3,
}

impl Place {
    pub const IDENTITY: Self = Self {
        rotation: Quat::IDENTITY,
        translation: Vec3::ZERO,
    };

    /// The placement of a child whose local transform is `local`.
    pub fn then(&self, local: &Transform) -> Self {
        Self {
            rotation: (self.rotation * local.rotation).normalize(),
            translation: self.translation + self.rotation * local.translation,
        }
    }
}

/// A two-bone chain (`a` the root joint, `b` the middle, `c` the end)
/// brought to reach `target`, bending toward `pole`: the turns, in the
/// chain's space, that take the upper bone (`upper`) and then the lower one
/// (`lower`, after the upper's) to their new places. Each bone keeps its
/// length; the lower bone only hinges about the bend's axis, so the knee or
/// elbow never twists. A target out of reach is reached for as far as the
/// chain goes.
pub fn two_bone(a: Vec3, b: Vec3, c: Vec3, target: Vec3, pole: Vec3) -> (Quat, Quat) {
    let (l1, l2) = (a.distance(b), b.distance(c));
    let to = target - a;
    let reach = to.length().clamp((l1 - l2).abs() + 1e-4, l1 + l2 - 1e-4);
    let dir = to.normalize_or(c - a);
    let side = pole - dir * pole.dot(dir);
    let side = side.normalize_or(dir.any_orthonormal_vector());
    let cos = ((l1 * l1 + reach * reach - l2 * l2) / (2.0 * l1 * reach)).clamp(-1.0, 1.0);
    let sin = (1.0 - cos * cos).max(0.0).sqrt();
    let knee = a + (dir * cos + side * sin) * l1;
    let end = a + dir * reach;
    // The upper bone turns so its line and its bend's plane both match.
    let frame = |u: Vec3, n: Vec3| Mat3::from_cols(u, n, u.cross(n));
    let (u0, u1) = ((b - a).normalize(), (knee - a).normalize());
    let (n0, n1) = ((b - a).cross(c - b), (knee - a).cross(end - knee));
    let upper = match (n0.try_normalize(), n1.try_normalize()) {
        (Some(n0), Some(n1)) => {
            let (n0, n1) = (
                (n0 - u0 * n0.dot(u0)).normalize(),
                (n1 - u1 * n1.dot(u1)).normalize(),
            );
            Quat::from_mat3(&(frame(u1, n1) * frame(u0, n0).transpose())).normalize()
        }
        _ => Quat::from_rotation_arc(u0, u1),
    };
    let lower = Quat::from_rotation_arc((upper * (c - b)).normalize(), (end - knee).normalize());
    (upper, lower)
}

/// The rider's bones as [`pose_riders`] reads and writes them.
type Bones<'w, 's> = Query<'w, 's, &'static mut Transform>;

/// Bends `upper`, `lower` and `end` (children of a bone placed `parent`)
/// so the end reaches `target`, bending toward `pole`; the end keeps its
/// placement's turn from the clip. Returns nothing: it writes the bones.
fn reach(
    transforms: &mut Bones,
    parent: Place,
    [upper, lower, end]: [Entity; 3],
    target: Vec3,
    pole: Vec3,
) {
    let (Ok(u), Ok(l), Ok(e)) = (
        transforms.get(upper).copied(),
        transforms.get(lower).copied(),
        transforms.get(end).copied(),
    ) else {
        return;
    };
    let pu = parent.then(&u);
    let pl = pu.then(&l);
    let pe = pl.then(&e);
    let (d1, d2) = two_bone(pu.translation, pl.translation, pe.translation, target, pole);
    let upper_turn = (d1 * pu.rotation).normalize();
    let lower_turn = (d2 * d1 * pl.rotation).normalize();
    if let Ok(mut t) = transforms.get_mut(upper) {
        t.rotation = (parent.rotation.inverse() * upper_turn).normalize();
    }
    if let Ok(mut t) = transforms.get_mut(lower) {
        t.rotation = (upper_turn.inverse() * lower_turn).normalize();
    }
    if let Ok(mut t) = transforms.get_mut(end) {
        t.rotation = (lower_turn.inverse() * pe.rotation).normalize();
    }
}

/// A critically damped spring: a value easing toward a target with no
/// overshoot, the same however the time is cut into frames.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Spring {
    pub value: f32,
    pub velocity: f32,
}

impl Spring {
    /// Moves `dt` seconds toward `target`, at angular frequency `omega`
    /// (about 4.7/`omega` seconds to settle); returns the value.
    pub fn step(&mut self, target: f32, omega: f32, dt: f32) -> f32 {
        let dt = dt.max(0.0);
        let y = self.value - target;
        let c = self.velocity + omega * y;
        let e = (-omega * dt).exp();
        self.value = target + (y + c * dt) * e;
        self.velocity = (self.velocity - omega * c * dt) * e;
        self.value
    }

    /// At `value`, still.
    pub fn at(value: f32) -> Self {
        Self {
            value,
            velocity: 0.0,
        }
    }
}

/// A rider's secondary motion: the springs and clocks of its overlays, on
/// the model's root.
#[derive(Component, Debug, Clone, Default)]
pub struct RiderMotion {
    /// The saddle's height, pitch and roll, smoothed.
    pub saddle: [Spring; 3],
    /// The spine's counter-pitch and roll, and the head's, lagging.
    pub counter: [Spring; 2],
    pub head: Spring,
    pub lean: Spring,
    pub braking: Spring,
    /// How far into the half-seat (`0..1`), easing with the gallop.
    pub half: Spring,
    /// How the head was held when the rider sat down (its pitch in the
    /// horse's body frame): it keeps it.
    pub head0: f32,
    pub head_primed: bool,
    /// How settled the hands are (`0..1`), after standing still.
    pub rest: Spring,
    /// The bit, smoothed, in the rider's space.
    pub bit: [Spring; 3],
    /// The free hand, lagging, in the rider's space.
    pub free: [Spring; 3],
    /// The shoulders' shrug.
    pub shrug: Spring,
    /// How far folded forward over a jump (`0..1`), easing.
    pub fold: Spring,
    /// Whether the horse was in the air last frame, and how long since it
    /// landed, seconds (none once the landing is over).
    pub flying: bool,
    pub landed: Option<f32>,
    /// The saddle's pitch along the horse, smoothed on the ground and held
    /// over a jump, and how far into a jump the rider is (`0..1`), easing.
    pub level: Spring,
    pub air: Spring,
    /// The rider's seed, for its own moments.
    pub seed: u32,
    pub speed: f32,
    pub still: f32,
    pub clock: f32,
    pub primed: bool,
}

/// What a rider reads of its horse: the chains from the model's root down
/// to its `Torso` and its `Head` (the body frame's placements of the
/// saddle and the bit), and the bit's middle on the head. On the horse's
/// body.
#[derive(Component, Debug, Clone)]
pub struct Saddle {
    pub torso: Vec<Entity>,
    pub head: Vec<Entity>,
    pub bit: Vec3,
}

/// The placement of the last of `chain` (from the horse model's root
/// down), in the horse's body frame.
fn chain_affine(chain: &[Entity], transforms: &Bones) -> Affine3A {
    chain.iter().fold(Affine3A::IDENTITY, |at, e| {
        at * transforms
            .get(*e)
            .map_or(Affine3A::IDENTITY, |t| t.compute_affine())
    })
}

/// A rotation `r` given in the horse's body frame, as a rotation in the
/// rider's space (`frame`: the rider's turn in the body frame).
fn into_rider(r: Quat, frame: Quat) -> Quat {
    frame.inverse() * r * frame
}

/// Turns `bone` (child of a bone placed `parent`) by `turn`, a rotation in
/// the rider's space.
fn turn_bone(transforms: &mut Bones, parent: Place, bone: Entity, turn: Quat) {
    if let Ok(mut t) = transforms.get_mut(bone) {
        t.rotation = (parent.rotation.inverse() * turn * parent.rotation * t.rotation).normalize();
    }
}

/// The saddle's pitch (nose up positive) and roll (its right side up
/// positive), radians, from the `Torso` bone's turn in the horse's body
/// frame: the bone runs along its own +Y to the horse's front, its +X to
/// the horse's right.
pub fn saddle_attitude(tilt: Quat) -> (f32, f32) {
    (
        (tilt * Vec3::Y).y.clamp(-1.0, 1.0).asin(),
        (tilt * Vec3::X).y.clamp(-1.0, 1.0).asin(),
    )
}

/// Fits every playing rider to its horse (see the module notes), after the
/// clips have posed it (and the horse) and before the transforms are worked
/// out.
pub(super) fn pose_riders(
    time: Res<Time>,
    horses: Query<(&HorseBody, &Saddle)>,
    mut riders: Query<(
        Entity,
        &Rig,
        &Riding,
        &Seated,
        &mut RiderMotion,
        &mut Visibility,
    )>,
    mut transforms: Bones,
) {
    let dt = time.delta_secs();
    for (me, rig, riding, seated, mut m, mut shown) in &mut riders {
        if rig.frozen || !rig.started {
            continue;
        }
        // Posed from here on: drawn.
        shown.set_if_neq(Visibility::Inherited);
        let Ok((horse, saddle)) = horses.get(riding.horse) else {
            continue;
        };
        let (Some(seat), Some(root)) = (
            transforms.get(me).ok().copied(),
            transforms.get(seated.bone(Bone::Root)).ok().copied(),
        ) else {
            continue;
        };
        // The saddle in the body frame, and the rider in it.
        let torso = chain_affine(&saddle.torso, &transforms);
        let rider = torso * seat.compute_affine();
        let (_, frame, _) = rider.to_scale_rotation_translation();
        let (_, tilt, _) = torso.to_scale_rotation_translation();
        // The seat's height (where the pelvis sits), and the saddle's
        // pitch and roll.
        let (saddle_pitch, saddle_roll) = saddle_attitude(tilt);
        let now = [rider.translation.y, saddle_pitch, saddle_roll];
        let head = chain_affine(&saddle.head, &transforms);
        let bit = rider
            .inverse()
            .transform_point3(head.transform_point3(saddle.bit));
        if !m.primed {
            m.saddle = now.map(Spring::at);
            m.level = Spring::at((tilt * Vec3::Y).y.clamp(-1.0, 1.0).asin());
            m.bit = bit.to_array().map(Spring::at);
            m.free = THIGH_HAND.to_array().map(Spring::at);
            m.seed = riding.seed;
            m.speed = horse.speed;
            m.primed = true;
        }
        m.clock = pampa_core::tick_clock(m.clock, dt);
        // A jump: folded forward from the takeoff over the top, then sitting
        // up and taking the landing in the knees and hips.
        let fold = m
            .fold
            .step(horse.flight.map_or(0.0, jump_fold), JUMP_EASE, dt)
            .clamp(0.0, 1.0);
        m.landed = match (m.flying, horse.flight) {
            (_, Some(_)) => None,
            (true, None) => Some(0.0),
            (false, None) => m.landed.map(|t| t + dt).filter(|t| *t < LANDING),
        };
        m.flying = horse.flight.is_some();
        let give = m.landed.map_or(0.0, landing);
        // The gait's clips stop in the air: the half-seat is held through
        // the jump and its landing.
        let jumping = f32::from(m.flying || m.landed.is_some());
        // What the saddle does beyond its smoothed path.
        let mut off = [0.0; 3];
        for k in 0..3 {
            let smooth = m.saddle[k].step(now[k], SADDLE_EASE, dt);
            off[k] = now[k] - smooth;
        }
        // Over a jump he keeps his balance however the horse pitches: its
        // pitch since it left the ground (the `Torso` bone runs along its
        // own +Y, forward) is taken at the hips, eased in and out.
        let heave = (tilt * Vec3::Y).y.clamp(-1.0, 1.0).asin();
        let level = if jumping > 0.0 {
            m.level.value
        } else {
            m.level.step(heave, SADDLE_EASE, dt)
        };
        // At once on takeoff (the horse has not pitched yet), easing out
        // once landed.
        if jumping > 0.0 {
            m.air = Spring::at(1.0);
        }
        let air = m.air.step(0.0, JUMP_EASE, dt).clamp(0.0, 1.0);
        let balance = JUMP_BALANCE * air * (heave - level);
        let [rise, pitch, roll] = off;
        // Speed, braking and standing still.
        let accel = if dt > 0.0 {
            (horse.speed - m.speed) / dt
        } else {
            0.0
        };
        m.speed = horse.speed;
        let braking = m
            .braking
            .step((-accel / BRAKING).clamp(0.0, 1.0), BRAKING_EASE, dt);
        m.still = if horse.speed < STANDING {
            m.still + dt
        } else {
            0.0
        };
        let settled = f32::from(m.still > STILL);
        let rest = m.rest.step(settled, REST_EASE, dt).clamp(0.0, 1.0);
        let gallop = horse.gallop.max(jumping).clamp(0.0, 1.0);
        // The half-seat comes in with the gallop, late.
        let half = m.half.step(gallop, HALF_SEAT_EASE, dt).clamp(0.0, 1.0);
        let walking = (horse.speed / WALK_TOP).min(1.0) * (1.0 - gallop);
        let lean = m.lean.step(
            LEAN.0 * half + LEAN.1 * walking - LEAN.2 * braking,
            LEAN_EASE,
            dt,
        ) + JUMP_FOLD * fold
            - LANDING_GIVE.0 * give
            + balance;
        // Over a jump the hips take the pitch (`balance`), not the spine.
        let counter_pitch = m.counter[0].step(COUNTER.0 * pitch * (1.0 - air), COUNTER_EASE, dt);
        let counter_roll = m.counter[1].step(COUNTER.1 * roll, COUNTER_EASE, dt);
        let slack = Vec3::new(
            bit.x - m.bit[0].step(bit.x, BIT_EASE, dt),
            bit.y - m.bit[1].step(bit.y, BIT_EASE, dt),
            bit.z - m.bit[2].step(bit.z, BIT_EASE, dt),
        );
        let get = |t: &Bones, b: Bone| t.get(seated.bone(b)).copied().ok();
        let root = Place::IDENTITY.then(&root);
        // The seat. Walking it sits deep in the recado; in the half-seat
        // it hovers above it, forward over the knees, not riding the
        // saddle's bounce: the legs take it. Never below the seat.
        // On landing the knees and hips give, the seat coming down to the
        // saddle (never into it).
        let hover = (HALF_SEAT.y - rise).clamp(0.0, HALF_SEAT.y * 2.0)
            * (1.0 - LANDING_GIVE.1 * give).max(0.0);
        let seat = Vec3::new(0.0, half * hover, half * HALF_SEAT.z);
        let period = SHIFT.0 + (riding.seed % SHIFT.1) as f32;
        let shift = Quat::from_rotation_z(
            SHIFT.2 * rest * (std::f32::consts::TAU * m.clock / period).sin(),
        );
        // A flat back: the lean is mostly at the hips.
        let tip = Quat::from_rotation_x(HIP_SHARE * lean) * shift;
        if let Ok(mut pelvis) = transforms.get_mut(seated.bone(Bone::Pelvis)) {
            pelvis.translation = root.rotation.inverse() * (seat - root.translation);
            pelvis.rotation =
                (root.rotation.inverse() * tip * root.rotation * pelvis.rotation).normalize();
        }
        let Some(pelvis) = get(&transforms, Bone::Pelvis) else {
            continue;
        };
        let pelvis = root.then(&pelvis);
        // The spine: the rest of the lean, countering the saddle late.
        let counter = |share: f32| {
            into_rider(
                Quat::from_rotation_z(-share * counter_pitch)
                    * Quat::from_rotation_x(share * counter_roll),
                frame,
            )
        };
        turn_bone(
            &mut transforms,
            pelvis,
            seated.bone(Bone::Spine1),
            Quat::from_rotation_x((1.0 - HIP_SHARE) * lean) * counter(0.6),
        );
        let Some(spine1) = get(&transforms, Bone::Spine1) else {
            continue;
        };
        let spine1 = pelvis.then(&spine1);
        let flex = (FLEX * rise * (1.0 - half)).clamp(-FLEX_MAX, FLEX_MAX);
        turn_bone(
            &mut transforms,
            spine1,
            seated.bone(Bone::Spine2),
            Quat::from_rotation_x(flex) * counter(0.4),
        );
        let mut chest = spine1;
        for b in [Bone::Spine2, Bone::Spine3] {
            if let Some(t) = get(&transforms, b) {
                chest = chest.then(&t);
            }
        }
        // The head level and looking ahead (as it held it sitting down),
        // steadied a touch late, and glancing around standing still.
        let (Some(neck), Some(skull)) =
            (get(&transforms, Bone::Neck), get(&transforms, Bone::Head))
        else {
            continue;
        };
        let up = frame * chest.then(&neck).then(&skull).rotation * Vec3::Y;
        let nodding = up.x.atan2(up.y);
        if !m.head_primed {
            m.head0 = nodding;
            m.head_primed = true;
        }
        let off_level = nodding - m.head0;
        // Most of it at once, the rest a touch late: steady, not locked.
        let level = 0.85 * off_level + 0.15 * m.head.step(off_level, HEAD_EASE, dt);
        let glance = 0.5 * rest * pampa_core::pose::glance(riding.seed, m.clock);
        let nod = |share: f32| {
            Quat::from_rotation_y(glance) * into_rider(Quat::from_rotation_z(share * level), frame)
        };
        turn_bone(&mut transforms, chest, seated.bone(Bone::Neck), nod(0.5));
        if let Some(neck) = get(&transforms, Bone::Neck) {
            turn_bone(
                &mut transforms,
                chest.then(&neck),
                seated.bone(Bone::Head),
                nod(0.5),
            );
        }
        // The legs to the stirrups, the heels down in the half-seat and
        // more as the seat comes down on them.
        let heel = Quat::from_rotation_x(-half * (HEEL + 2.0 * (HALF_SEAT.y - hover).max(0.0)));
        for s in 0..2 {
            let leg = [Bone::Thigh(s), Bone::Calf(s), Bone::Foot(s)].map(|b| seated.bone(b));
            let ankle = ANKLES[s].lerp(GALLOP_ANKLES[s], half);
            reach(&mut transforms, pelvis, leg, ankle, KNEE_POLE[s]);
            if let (Some(thigh), Some(calf)) = (
                get(&transforms, Bone::Thigh(s)),
                get(&transforms, Bone::Calf(s)),
            ) {
                let shin = pelvis.then(&thigh).then(&calf);
                turn_bone(&mut transforms, shin, seated.bone(Bone::Foot(s)), heel);
            }
        }
        // The rein hand follows the horse's mouth (more in the half-seat:
        // the elbows open as the head goes forward and down); the free hand
        // rests by the thigh, going with the seat.
        let follow = FOLLOW.0 + (FOLLOW.1 - FOLLOW.0) * half;
        let slack = slack.clamp_length_max(GIVE_MAX) * follow * (1.0 - rest);
        let reins = REINS_HAND.lerp(GALLOP_HAND, half).lerp(POMMEL_HAND, rest) + JUMP_HAND * fold;
        let free = free_hand(
            &mut m,
            (rise, pitch, roll),
            (walking, gallop, braking, rest),
            dt,
        );
        let hands = [reins + slack, seat + free];
        // The shoulders go with the torso, late: up as the saddle drops
        // away, forward as it pitches down.
        let shrug = m.shrug.step(SHRUG * (pitch - 2.0 * rise), SHRUG_EASE, dt);
        for s in 0..2 {
            let side = if s == 0 { 1.0 } else { -1.0 };
            turn_bone(
                &mut transforms,
                chest,
                seated.bone(Bone::Clavicle(s)),
                Quat::from_rotation_x(0.5 * shrug) * Quat::from_rotation_z(side * shrug),
            );
            let Some(clavicle) = get(&transforms, Bone::Clavicle(s)) else {
                continue;
            };
            let shoulder = chest.then(&clavicle);
            let arm = [Bone::UpperArm(s), Bone::LowerArm(s), Bone::Hand(s)].map(|b| seated.bone(b));
            reach(&mut transforms, shoulder, arm, hands[s], ELBOW_POLE[s]);
        }
    }
}

/// Where the free (right) hand goes, from where the seat is, in the
/// rider's space: on its thigh, swinging late with the horse (a little
/// walking, more and lifting at the gallop), forward toward the pommel
/// braking, and, standing still, now and then up to the hat's brim.
/// `motion` is the saddle's rise, pitch and roll beyond their smoothed
/// paths;
/// `gaits` how much it walks, gallops, brakes and has settled.
pub fn free_hand(
    m: &mut RiderMotion,
    (rise, pitch, roll): (f32, f32, f32),
    (walking, gallop, braking, rest): (f32, f32, f32, f32),
    dt: f32,
) -> Vec3 {
    let [x, y, z] = FREE_SWING;
    let stride = Vec3::new(
        x[0] * rise + x[2] * roll,
        y[0] * rise + y[1] * pitch,
        z[1] * pitch + z[0] * rise,
    );
    let swing = stride * (FREE_WALK * walking + gallop);
    let brake = (POMMEL_HAND - THIGH_HAND) * FREE_BRAKE * braking;
    let brim = (pampa_core::pose::glance(m.seed ^ HAT_SALT, m.clock * HAT_PACE).abs() / GLANCE_MAX)
        .min(1.0);
    let hat = (HAT_HAND - THIGH_HAND) * brim * rest;
    let target = THIGH_HAND + (swing + brake).clamp_length_max(FREE_REACH) + hat;
    // Lagging off the torso: a loose arm, never stiff, never flailing.
    let at = [0, 1, 2].map(|k| m.free[k].step(target[k], FREE_EASE, dt));
    Vec3::from_array(at)
}

/// Seats each saddled horse's rider once its tack is on: the rider's model
/// on the `Torso` bone at the seat, waiting for its scene, and what the
/// rider reads of the horse ([`Saddle`]). Riders without a model ride an
/// empty saddle.
pub(super) fn spawn_riders(
    mut commands: Commands,
    characters: Option<Res<CharacterModels>>,
    horses: Query<(Entity, &HorseBody, &Tack), Changed<Tack>>,
    parents: Query<&ChildOf>,
    roots: Query<(), With<HorseModel>>,
) {
    for (carrier, body, tack) in &horses {
        // From the horse model's root down to `bone`.
        let chain = |bone: Entity| {
            let mut chain = vec![bone];
            let mut at = bone;
            while !roots.contains(at) {
                let Ok(child_of) = parents.get(at) else {
                    break;
                };
                at = child_of.parent();
                chain.push(at);
            }
            chain.reverse();
            chain
        };
        commands.entity(carrier).try_insert(Saddle {
            torso: chain(tack.torso),
            head: chain(tack.head),
            bit: tack.bit,
        });
        let model = body
            .rider
            .and_then(|kind| Some((kind, characters.as_ref()?.of(kind)?.clone())));
        if let Some((kind, scene)) = model {
            commands.spawn((
                Name::new("Rider model"),
                CharacterModel,
                ModelPending(scene),
                Riding {
                    horse: carrier,
                    kind,
                    seed: body.seed,
                },
                transform_of(tack.torso_rest.inverse() * seat_transform(tack.seat)),
                // Drawn once seated: its scene spawns standing at rest.
                Visibility::Hidden,
                ChildOf(tack.torso),
            ));
        }
    }
}

/// A stirrup a seated rider's foot carries. It stays under the horse's
/// torso, with the rest of the tack, since the rider's scene (and its foot
/// bones) may be spawned again; [`carry_stirrups`] puts it on the foot
/// each frame.
#[derive(Component, Debug, Clone, Copy)]
pub struct OnFoot {
    /// The rider's model root: while it is hidden (not yet posed) the
    /// stirrup stays where it was.
    pub rider: Entity,
    pub foot: Entity,
    /// Its placement on the foot bone ([`Seated::stirrups`]).
    pub at: Transform,
}

/// Once a rider is seated, and again whenever its bones are found anew
/// (its scene spawned again), its horse's reins run to its left hand and
/// its stirrups go on its feet.
pub(super) fn seat_riders(
    mut commands: Commands,
    seated: Query<(Entity, &Riding, &Seated), Changed<Seated>>,
    tacks: Query<&Tack>,
    mut straps: Query<&mut Strap>,
) {
    for (rider, riding, seated) in &seated {
        let Ok(tack) = tacks.get(riding.horse) else {
            continue;
        };
        for s in 0..2 {
            // Both reins in the left hand, a finger's width apart.
            if let Ok(mut rein) = straps.get_mut(tack.reins[s]) {
                let apart = Vec3::X * (if s == 0 { -0.012 } else { 0.012 });
                rein.ends[1] = (seated.bone(Bone::Hand(0)), PALM + apart);
            }
            commands.entity(tack.stirrups[s]).try_insert(OnFoot {
                rider,
                foot: seated.bone(Bone::Foot(s)),
                at: seated.stirrups[s],
            });
        }
    }
}

/// Puts each carried stirrup on its rider's foot, once the transforms are
/// worked out (before the straps are stretched to it).
pub(super) fn carry_stirrups(
    mut stirrups: Query<(&OnFoot, &ChildOf, &mut Transform, &mut GlobalTransform)>,
    places: Query<&GlobalTransform, Without<OnFoot>>,
    shown: Query<&Visibility>,
) {
    for (on, child_of, mut transform, mut global) in &mut stirrups {
        if shown.get(on.rider).is_ok_and(|v| *v == Visibility::Hidden) {
            continue;
        }
        let (Ok(foot), Ok(parent)) = (places.get(on.foot), places.get(child_of.parent())) else {
            continue;
        };
        let placed = foot.affine() * on.at.compute_affine();
        *global = GlobalTransform::from(placed);
        *transform = transform_of(parent.affine().inverse() * placed);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn two_bones_reach_the_target_bending_toward_the_pole_and_keep_their_lengths() {
        // A leg hanging bent: hip, knee forward, ankle under it.
        let (a, b, c) = (
            Vec3::ZERO,
            Vec3::new(0.0, -0.3, 0.3),
            Vec3::new(0.0, -0.7, 0.25),
        );
        for (target, pole) in [
            (Vec3::new(0.3, -0.69, 0.1), Vec3::new(0.45, -0.1, 1.0)),
            (Vec3::new(-0.2, -0.5, 0.3), Vec3::new(-1.0, 0.0, 0.2)),
            (Vec3::new(0.0, -0.3, -0.4), Vec3::new(0.0, 1.0, 0.0)),
        ] {
            let (d1, d2) = two_bone(a, b, c, target, pole);
            let knee = a + d1 * (b - a);
            let end = knee + d2 * d1 * (c - b);
            assert!(end.distance(target) < 1e-4, "{end} {target}");
            assert!((knee.distance(a) - b.distance(a)).abs() < 1e-5);
            assert!((end.distance(knee) - c.distance(b)).abs() < 1e-5);
            // The knee is on the pole's side of the hip-to-ankle line.
            let line = (target - a).normalize();
            let off = (knee - a) - line * (knee - a).dot(line);
            assert!(off.dot(pole) > 0.0, "{knee}");
            // The lower bone only hinges: about the bend's axis.
            let (axis, _) = d2.to_axis_angle();
            let bend = (knee - a).cross(end - knee).normalize();
            assert!(axis.cross(bend).length() < 1e-3, "{axis} {bend}");
        }
        // Out of reach: straight toward it, as far as the chain goes.
        let far = Vec3::new(0.0, -5.0, 0.0);
        let (d1, d2) = two_bone(a, b, c, far, Vec3::Z);
        let end = a + d1 * (b - a) + d2 * d1 * (c - b);
        let reach = b.distance(a) + c.distance(b);
        assert!(
            (end.length() - reach).abs() < 1e-3 && end.y < -reach + 1e-3,
            "{end}"
        );
    }

    #[test]
    fn the_upper_bone_keeps_its_twist_round_the_bend() {
        // The bend's axis turns with the bone: a frame on the upper bone
        // with its hinge along the bend's axis keeps it there.
        let (a, b, c) = (
            Vec3::ZERO,
            Vec3::new(0.0, -0.4, 0.1),
            Vec3::new(0.0, -0.8, -0.1),
        );
        let target = Vec3::new(0.25, -0.6, 0.2);
        let (d1, _) = two_bone(a, b, c, target, Vec3::new(1.0, 0.0, 1.0));
        let hinge = (b - a).cross(c - b).normalize();
        let knee = d1 * (b - a);
        let (d1b, d2) = two_bone(a, b, c, target, Vec3::new(1.0, 0.0, 1.0));
        assert_eq!(d1, d1b, "deterministic");
        let end = knee + d2 * d1 * (c - b);
        let new_hinge = knee.cross(end - knee).normalize();
        assert!(
            (d1 * hinge).distance(new_hinge) < 1e-4,
            "{} {new_hinge}",
            d1 * hinge
        );
    }

    #[test]
    fn a_placement_composes_like_the_transforms() {
        let parent = Place {
            rotation: Quat::from_euler(EulerRot::XYZ, 0.3, -0.5, 0.9),
            translation: Vec3::new(0.1, 0.9, -0.2),
        };
        let local = Transform::from_xyz(0.0, 0.4, 0.05).with_rotation(Quat::from_euler(
            EulerRot::XYZ,
            -0.2,
            0.1,
            0.4,
        ));
        let child = parent.then(&local);
        let want = Transform::from_translation(parent.translation)
            .with_rotation(parent.rotation)
            .compute_affine()
            * local.compute_affine();
        assert!(child.translation.distance(want.translation.into()) < 1e-6);
        assert!(
            child
                .rotation
                .angle_between(Quat::from_mat3a(&want.matrix3))
                < 1e-5
        );
    }

    #[test]
    fn the_rider_faces_the_horses_front_with_the_pelvis_above_the_seat() {
        let seat = Vec3::new(-0.02, 1.55, 0.0);
        let at = seat_transform(seat);
        assert!(
            at.transform_vector3(Vec3::Z).distance(Vec3::X) < 1e-6,
            "front"
        );
        // The model's left (+X) is the horse's left (-Z).
        assert!(at.transform_vector3(Vec3::X).distance(Vec3::NEG_Z) < 1e-6);
        assert!(
            at.transform_point3(Vec3::ZERO)
                .distance(seat + Vec3::Y * PELVIS_RISE)
                < 1e-6
        );
        // The feet go down either side, the rein hand forward and up.
        const { assert!(REINS_HAND.z > 0.2 && REINS_HAND.y > 0.0 && THIGH_HAND.x < 0.0) };
        for s in 0..2 {
            let side = if s == 0 { 1.0 } else { -1.0 };
            assert!(ANKLES[s].x * side > 0.25 && ANKLES[s].y < -0.6);
            assert!(KNEE_POLE[s].x * side > 0.0 && ELBOW_POLE[s].x * side > 0.0);
        }
    }

    #[test]
    fn the_saddle_pitches_with_the_horses_nose_and_rolls_with_its_sides() {
        // The `Torso` bone at rest: it runs along its own +Y to the horse's
        // front (+X), its +X to the horse's right (+Z), its +Z up.
        let rest = Quat::from_mat3(&Mat3::from_cols(Vec3::Z, Vec3::X, Vec3::Y));
        for angle in [-0.4f32, -0.1, 0.1, 0.4] {
            // Nose up (about the body's side axis, +Z) is a pitch alone.
            let (pitch, roll) = saddle_attitude(Quat::from_rotation_z(angle) * rest);
            assert!((pitch - angle).abs() < 1e-4, "{angle}: pitch {pitch}");
            assert!(roll.abs() < 1e-4, "{angle}: roll {roll}");
            // The right side up (about the body's long axis, +X) a roll
            // alone.
            let (pitch, roll) = saddle_attitude(Quat::from_rotation_x(-angle) * rest);
            assert!(pitch.abs() < 1e-4, "{angle}: pitch {pitch}");
            assert!((roll - angle).abs() < 1e-4, "{angle}: roll {roll}");
        }
    }

    #[test]
    fn a_spring_eases_without_overshoot_the_same_at_any_frame_rate() {
        let mut fine = Spring::at(1.0);
        let mut coarse = Spring::at(1.0);
        let mut last = 1.0;
        for _ in 0..100 {
            let v = fine.step(0.0, 6.0, 0.01);
            assert!(v <= last + 1e-6 && v >= -1e-6, "{v}");
            last = v;
        }
        for _ in 0..10 {
            coarse.step(0.0, 6.0, 0.1);
        }
        assert!(
            (fine.value - coarse.value).abs() < 1e-5,
            "{fine:?} {coarse:?}"
        );
        assert!((fine.velocity - coarse.velocity).abs() < 1e-4);
        // Critically damped from rest: (1 + omega t) e^(-omega t), setting
        // off gently, settled after about 4.7/omega seconds.
        let want = (1.0 + 6.0) * (-6.0f32).exp();
        assert!((fine.value - want).abs() < 1e-4, "{fine:?} {want}");
        let mut start = Spring::at(1.0);
        start.step(0.0, 6.0, 0.01);
        assert!(1.0 - start.value < 0.005, "it sets off gently: {start:?}");
        assert_eq!(Spring::at(2.0).step(2.0, 6.0, 0.1), 2.0, "at rest stays");
    }

    #[test]
    fn a_stirrup_stays_put_while_its_rider_is_hidden_and_goes_back_on_the_foot_once_shown() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        let torso = world
            .spawn((
                Transform::default(),
                GlobalTransform::from_xyz(0.0, 1.2, 0.0),
            ))
            .id();
        let foot = world.spawn(GlobalTransform::from_xyz(0.3, 0.5, 0.1)).id();
        let rider = world.spawn(Visibility::Hidden).id();
        let hanging = Transform::from_xyz(0.25, -0.6, 0.0);
        let on = OnFoot {
            rider,
            foot,
            at: Transform::from_xyz(0.0, -0.05, 0.02),
        };
        let stirrup = world
            .spawn((
                on,
                hanging,
                GlobalTransform::from_xyz(0.25, 0.6, 0.0),
                ChildOf(torso),
            ))
            .id();
        let place = |world: &World| {
            (
                *world.get::<Transform>(stirrup).unwrap(),
                world.get::<GlobalTransform>(stirrup).unwrap().translation(),
            )
        };
        // Hidden (its scene just spawned, not posed yet): left where it hung,
        // not put on a foot standing at rest.
        world.run_system_once(carry_stirrups).unwrap();
        assert_eq!(place(&world), (hanging, Vec3::new(0.25, 0.6, 0.0)));
        // Shown: on the foot, its local place under the torso to match.
        world.entity_mut(rider).insert(Visibility::Inherited);
        world.run_system_once(carry_stirrups).unwrap();
        let (local, global) = place(&world);
        assert!(
            global.distance(Vec3::new(0.3, 0.45, 0.12)) < 1e-5,
            "{global}"
        );
        assert!(
            (local.translation - Vec3::new(0.3, -0.75, 0.12)).length() < 1e-5,
            "{local:?}"
        );
        // And it follows the foot.
        *world.get_mut::<GlobalTransform>(foot).unwrap() = GlobalTransform::from_xyz(0.2, 0.7, 0.0);
        world.run_system_once(carry_stirrups).unwrap();
        assert!(place(&world).1.distance(Vec3::new(0.2, 0.65, 0.02)) < 1e-5);
        // Hidden again: it keeps its last place.
        world.entity_mut(rider).insert(Visibility::Hidden);
        *world.get_mut::<GlobalTransform>(foot).unwrap() = GlobalTransform::from_xyz(9.0, 9.0, 9.0);
        world.run_system_once(carry_stirrups).unwrap();
        assert!(place(&world).1.distance(Vec3::new(0.2, 0.65, 0.02)) < 1e-5);
    }

    // --- with the shipped horse and rider ------------------------------------

    use crate::character::cpu;
    use crate::character::horse::tests::{named, saddled_every};
    use crate::character::tack::tests::{clearances, horse_skin, signed_distance};
    use crate::character::tack::{HorsePiece, Strap, Tack};
    use crate::character::{CharacterVisual, Coat, FigureKind, HorseModel, Rig};

    /// [`saddled_every`] a rider, returning once the rider plays its clips
    /// too, seated.
    pub(crate) fn seated_every(visual: CharacterVisual, speed: f32, ms: u64) -> (App, Entity) {
        let (mut app, me) = saddled_every(visual, speed, ms);
        for _ in 0..3000 {
            app.update();
            let world = app.world_mut();
            if world
                .query::<(&Rig, &Seated)>()
                .iter(world)
                .any(|(r, _)| r.started)
            {
                // One more frame: the seat's straps and stirrups move over.
                app.update();
                return (app, me);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("the rider never sat");
    }

    pub(crate) fn seated(visual: CharacterVisual, speed: f32) -> (App, Entity) {
        seated_every(visual, speed, 50)
    }

    /// How far the poncho of the model under `model` sinks into its upper arms
    /// (or shoulders), meters: for each arm, its upper arm is a capsule round
    /// the bone from the shoulder joint to the elbow, as thick as the sleeve
    /// round its middle (the median distance of the shirt's points there), and
    /// the deepest poncho point inside either is the answer (0 when none is).
    /// Only what lies out from the armpit counts (no more than 4 cm in from
    /// the shoulder joint, across the model): nearer the spine the capsule
    /// would take in the chest and the back.
    pub(crate) fn poncho_in_arms(world: &mut World, model: Entity) -> f32 {
        let drawn = cpu::drawn(world, Some(model));
        let find = |name: &str| {
            let mut stack = vec![model];
            while let Some(e) = stack.pop() {
                if world.get::<Name>(e).is_some_and(|n| n.as_str() == name) {
                    return world.get::<GlobalTransform>(e).map(|g| g.translation());
                }
                stack.extend(world.get::<Children>(e).into_iter().flatten().copied());
            }
            None
        };
        let mesh = |name: &'static str| {
            drawn
                .iter()
                .filter(move |d| d.name == name)
                .flat_map(|d| d.positions.iter().copied())
        };
        let to_model = world
            .get::<GlobalTransform>(model)
            .map_or(bevy::math::Affine3A::IDENTITY, |g| g.affine().inverse());
        let mut deepest = 0.0f32;
        for side in ["l", "r"] {
            let (Some(a), Some(b)) = (
                find(&format!("upperarm_{side}")),
                find(&format!("lowerarm_{side}")),
            ) else {
                continue;
            };
            let along = b - a;
            let place = |p: Vec3| {
                let t = (p - a).dot(along) / along.length_squared();
                (t, p.distance(a + along * t.clamp(0.0, 1.0)))
            };
            let mut sleeve: Vec<f32> = mesh("shirt")
                .map(place)
                .filter(|(t, d)| (0.3..0.7).contains(t) && *d < 0.15)
                .map(|(_, d)| d)
                .collect();
            if sleeve.is_empty() {
                continue;
            }
            sleeve.sort_by(f32::total_cmp);
            let radius = sleeve[sleeve.len() / 2];
            let shoulder = to_model.transform_point3(a).x.abs();
            for p in mesh("poncho") {
                if to_model.transform_point3(p).x.abs() < shoulder - 0.04 {
                    continue;
                }
                let (t, d) = place(p);
                if (0.0..=1.0).contains(&t) && d < radius {
                    deepest = deepest.max(radius - d);
                }
            }
        }
        deepest
    }

    /// The interior angle at `b` between `a` and `c`, degrees.
    fn angle_at(a: Vec3, b: Vec3, c: Vec3) -> f32 {
        (a - b).angle_between(c - b).to_degrees()
    }

    fn spread(v: &[f32]) -> f32 {
        v.iter().fold(f32::MIN, |a, b| a.max(*b)) - v.iter().fold(f32::MAX, |a, b| a.min(*b))
    }

    #[test]
    fn a_rider_sits_on_the_horses_torso_with_the_reins_and_the_stirrups() {
        let (mut app, me) = seated(
            CharacterVisual::Mounted {
                rider: FigureKind::Protagonist,
                coat: Coat::Bayo,
            },
            0.0,
        );
        let world = app.world_mut();
        let carrier = world.get::<Children>(me).unwrap()[0];
        // The rider's model sits on the horse's torso bone.
        let mut riders = world.query::<(Entity, &Riding, &Seated, &ChildOf)>();
        let (rider, riding, seated, seat) = riders.single(world).unwrap();
        let seated = seated.clone();
        assert_eq!(
            (riding.horse, riding.kind),
            (carrier, FigureKind::Protagonist)
        );
        assert_eq!(world.get::<Name>(seat.parent()).unwrap().as_str(), "Torso");
        // The reins run to its hands, the stirrups are on its feet.
        let tack = *world.get::<Tack>(carrier).unwrap();
        for s in 0..2 {
            let rein = world.get::<Strap>(tack.reins[s]).unwrap();
            // Both in the left hand.
            assert_eq!(
                rein.ends[1].0,
                seated.bones[Bone::ALL.iter().position(|b| *b == Bone::Hand(0)).unwrap()]
            );
            // Carried by the foot, kept under the horse's torso.
            let on = world.get::<OnFoot>(tack.stirrups[s]).unwrap();
            assert_eq!(
                world.get::<Name>(on.foot).unwrap().as_str(),
                ["foot_l", "foot_r"][s]
            );
            assert_eq!((on.rider, on.at), (rider, seated.stirrups[s]));
            let hung = world.get::<ChildOf>(tack.stirrups[s]).unwrap().parent();
            assert_eq!(hung, tack.torso);
            let (stirrup, foot) = (
                world.get::<GlobalTransform>(tack.stirrups[s]).unwrap(),
                world.get::<GlobalTransform>(on.foot).unwrap(),
            );
            let want = foot.affine() * on.at.compute_affine();
            assert!(
                stirrup.translation().distance(Vec3::from(want.translation)) < 1e-4,
                "on the foot"
            );
        }
    }

    /// The frames a rider's scene, already loaded, may take to spawn again
    /// and be seated and shown.
    const RESPAWN_FRAMES: usize = 3;

    #[test]
    fn a_respawned_rider_keeps_the_stirrups_on_its_new_feet_and_the_reins_in_its_new_hand() {
        use bevy::world_serialization::WorldAssetRoot;
        let (mut app, me) = seated(
            CharacterVisual::Mounted {
                rider: FigureKind::Protagonist,
                coat: Coat::Bayo,
            },
            1.5,
        );
        let world = app.world_mut();
        let carrier = world.get::<Children>(me).unwrap()[0];
        let tack = *world.get::<Tack>(carrier).unwrap();
        let (rider, old) = world
            .query_filtered::<(Entity, &Seated), With<Riding>>()
            .single(world)
            .map(|(e, s)| (e, s.clone()))
            .unwrap();
        // Its scene spawned again: new bones.
        world
            .get_mut::<WorldAssetRoot>(rider)
            .unwrap()
            .set_changed();
        // The scene is loaded already: it spawns again, is dressed and
        // posed within a few frames, with no waiting on the asset server.
        let mut new = None;
        for frame in 0..RESPAWN_FRAMES {
            app.update();
            let world = app.world();
            let seated = world.get::<Seated>(rider).unwrap();
            let posed = world.get::<Visibility>(rider) == Some(&Visibility::Inherited);
            if seated.bones[0] != old.bones[0] && posed {
                new = Some((frame, seated.clone()));
                break;
            }
        }
        let (frame, new) = new.expect("the rider's scene spawned again and was posed");
        println!("respawned and seated after {} frames", frame + 1);
        let bone = |b: Bone| new.bones[Bone::ALL.iter().position(|x| *x == b).unwrap()];
        // Over a few frames of the walk: the stirrups ride the new feet,
        // the reins end in the new hand.
        for _ in 0..10 {
            app.update();
            let world = app.world();
            for s in 0..2 {
                let on = world
                    .get::<OnFoot>(tack.stirrups[s])
                    .expect("the stirrup is kept");
                assert_eq!(on.foot, bone(Bone::Foot(s)));
                let foot = world.get::<GlobalTransform>(on.foot).unwrap();
                let at = world.get::<GlobalTransform>(tack.stirrups[s]).unwrap();
                let want = foot.affine() * on.at.compute_affine();
                assert!(at.translation().distance(Vec3::from(want.translation)) < 1e-4);
                let rein = world.get::<Strap>(tack.reins[s]).unwrap();
                assert_eq!(rein.ends[1].0, bone(Bone::Hand(0)));
            }
        }
    }

    #[test]
    fn the_rider_clears_the_horse_through_every_gait() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        for (speed, frames) in [(0.0, 8), (1.5, 24), (4.5, 12), (12.2, 12)] {
            let (mut app, me) = seated(rider, speed);
            for _ in 0..30 {
                app.update();
            }
            let (_, seat, _) = clearances(&mut app, me, frames);
            println!("{speed} m/s: the rider {seat:.4} m off the horse's skin");
            assert!(
                seat > 0.0,
                "{speed} m/s: the rider sinks into the horse ({seat})"
            );
        }
    }

    #[test]
    fn through_a_jump_the_rider_stays_on_the_saddle() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = seated_every(rider, 12.2, 20);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        // The pelvis in the saddle's (`Torso` bone's) frame.
        let seat = |app: &App| {
            let world = app.world();
            let model = named(world, carrier, "Rider model");
            let [torso, pelvis] = [
                named(world, carrier, "Torso"),
                named(world, model, "pelvis"),
            ];
            let g = |e: Entity| world.get::<GlobalTransform>(e).unwrap().affine();
            g(torso)
                .inverse()
                .transform_point3(Vec3::from(g(pelvis).translation))
        };
        let fly = |app: &mut App, flight: Option<f32>| {
            app.world_mut()
                .entity_mut(me)
                .insert(crate::character::VisualPose {
                    animation: pampa_core::AnimationState::Gallop,
                    speed: 12.2,
                    flight,
                    ..default()
                });
            app.update();
        };
        for _ in 0..80 {
            fly(&mut app, None);
        }
        let ground = seat(&app);
        // A jump's whole flight, at its real pace (1.1 s of 20 ms frames).
        for k in 0..55 {
            fly(&mut app, Some(k as f32 / 54.0));
            let off = seat(&app).distance(ground);
            assert!(off < 0.15, "frame {k}: the rider is {off} m off the saddle");
        }
        for _ in 0..60 {
            fly(&mut app, None);
        }
        assert!(seat(&app).distance(ground) < 0.15, "and back on it");
    }

    #[test]
    fn over_a_jump_the_rider_folds_forward_follows_the_neck_and_sits_up_to_land() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = seated_every(rider, 12.2, 20);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        // How far the rider leans forward from the vertical (his neck over
        // his pelvis), degrees; how far his seat is over the saddle, in his
        // own (seat) frame, meters; and his rein hand ahead of the saddle,
        // along the horse, meters.
        let measure = |app: &App| {
            let world = app.world();
            let model = named(world, carrier, "Rider model");
            let [pelvis, neck, hand] =
                ["pelvis", "neck_01", "hand_l"].map(|n| named(world, model, n));
            let torso = named(world, carrier, "Torso");
            let g = |e: Entity| *world.get::<GlobalTransform>(e).unwrap();
            let body = g(carrier).affine().inverse();
            let back = body.transform_vector3(g(neck).translation() - g(pelvis).translation());
            let gap = g(model)
                .affine()
                .inverse()
                .transform_point3(g(pelvis).translation())
                .y;
            let reach = body.transform_point3(g(hand).translation()).x
                - body.transform_point3(g(torso).translation()).x;
            (back.x.atan2(back.y).to_degrees(), gap, reach)
        };
        let fly = |app: &mut App, flight: Option<f32>| {
            app.world_mut()
                .entity_mut(me)
                .insert(crate::character::VisualPose {
                    animation: pampa_core::AnimationState::Gallop,
                    speed: 12.2,
                    flight,
                    ..default()
                });
            app.update();
        };
        let mean = |v: &[(f32, f32, f32)], k: usize| {
            v.iter().map(|m| [m.0, m.1, m.2][k]).sum::<f32>() / v.len() as f32
        };
        for _ in 0..80 {
            fly(&mut app, None);
        }
        let mut ground = Vec::new();
        for _ in 0..40 {
            fly(&mut app, None);
            ground.push(measure(&app));
        }
        // A jump's whole flight, at its real pace (1.1 s of 20 ms frames).
        let mut flight = Vec::new();
        for k in 0..55 {
            fly(&mut app, Some(k as f32 / 54.0));
            flight.push(measure(&app));
        }
        let mut landing = Vec::new();
        for _ in 0..100 {
            fly(&mut app, None);
            landing.push(measure(&app));
        }
        let (lean, reach) = (mean(&ground, 0), mean(&ground, 2));
        let all: Vec<_> = flight.iter().chain(&landing).collect();
        let jolt = all
            .windows(2)
            .map(|w| (w[1].0 - w[0].0).abs())
            .fold(0.0, f32::max);
        let top = &flight[15..45];
        let sitting = landing[..25].iter().map(|m| m.0).fold(f32::MAX, f32::min);
        println!(
            "gallop {lean:.1}° {reach:.2} m, over the top {:.1}° {:.2} m, landing {sitting:.1}°, after {:.1}°, jolt {jolt:.1}°",
            mean(top, 0),
            mean(top, 2),
            mean(&landing[60..], 0),
        );
        // Folded forward from the hips over the top, his hand forward
        // along the neck.
        let leans: Vec<f32> = top.iter().map(|m| m.0).collect();
        assert!(
            leans.iter().all(|l| (lean + 12.0..lean + 35.0).contains(l)),
            "{leans:?} vs {lean}"
        );
        assert!(mean(top, 2) > reach + 0.1, "{} vs {reach}", mean(top, 2));
        // Sitting up to land, then riding on in the half-seat.
        assert!(sitting < lean - 6.0, "{sitting} vs {lean}");
        assert!((mean(&landing[60..], 0) - lean).abs() < 4.0);
        // Never a snap, and never into the saddle.
        assert!(jolt < 5.0, "{jolt}° in a frame");
        let lowest = all.iter().map(|m| m.1).fold(f32::MAX, f32::min);
        assert!(lowest > -0.002, "the seat {lowest} m into the saddle");
    }

    #[test]
    fn at_the_gallop_the_rider_rides_the_half_seat() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = seated_every(rider, 12.2, 20);
        for _ in 0..80 {
            app.update();
        }
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        let (mut gap, mut knee, mut elbow, mut reach, mut mouth) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let (mut saddle, mut skull, mut nods, mut pitches) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let mut closest = f32::MAX;
        let mut leans = Vec::new();
        // Two strides and more.
        for k in 0..40 {
            app.update();
            let world = app.world();
            // Found afresh each frame: a scene spawned again has new bones.
            let model = named(world, carrier, "Rider model");
            let [torso, horse_head] = ["Torso", "Head"].map(|n| named(world, carrier, n));
            let [pelvis, thigh, calf, foot, head, upper, lower, hand, neck] = [
                "pelvis",
                "thigh_l",
                "calf_l",
                "foot_l",
                "head",
                "upperarm_l",
                "lowerarm_l",
                "hand_l",
                "neck_01",
            ]
            .map(|n| named(world, model, n));
            let bit = world.get::<Saddle>(carrier).unwrap().bit;
            let g = |e: Entity| *world.get::<GlobalTransform>(e).unwrap();
            let at = |e: Entity| g(e).translation();
            let turn = |e: Entity| g(e).to_scale_rotation_translation().1;
            let body = g(carrier).affine().inverse();
            // The seat over the saddle, in the rider's own (seat) frame.
            gap.push(g(model).affine().inverse().transform_point3(at(pelvis)).y);
            knee.push(angle_at(at(thigh), at(calf), at(foot)));
            elbow.push(angle_at(at(upper), at(lower), at(hand)));
            let bit_at = g(horse_head).transform_point(bit);
            reach.push(at(hand).distance(bit_at));
            mouth.push(body.transform_point3(bit_at).x);
            saddle.push(at(torso).y);
            skull.push(at(head).y);
            let pitch = |q: Quat| {
                let up = body.transform_vector3(q * Vec3::Y);
                up.x.atan2(up.y)
            };
            nods.push(pitch(turn(head)));
            // The saddle's pitch along the horse (the bone runs along +Y).
            pitches.push((body.transform_vector3(turn(torso) * Vec3::Y)).y.asin());
            let back = body.transform_vector3(at(neck) - at(pelvis));
            leans.push(back.x.atan2(back.y).to_degrees());
            let head_at = at(head);
            if k % 4 == 0 {
                // The rider's head never near the horse's head or neck.
                let drawn = cpu::drawn(app.world_mut(), Some(carrier));
                let (skin, _) = horse_skin(&drawn);
                let to_body = |p: Vec3| body.transform_point3(p);
                for p in skin.iter().filter(|p| to_body(**p).x > 0.45) {
                    closest = closest.min(p.distance(head_at));
                }
            }
        }
        println!(
            "gap {:.3}..{:.3} m, knee {:.0}..{:.0}°, elbow {:.0}..{:.0}°, hand to bit {:.3}..{:.3} m, saddle {:.3} m, head {:.3} m, head pitch {:.3}, saddle pitch {:.3}, closest {closest:.2} m",
            gap.iter().fold(f32::MAX, |a, b| a.min(*b)),
            gap.iter().fold(f32::MIN, |a, b| a.max(*b)),
            knee.iter().fold(f32::MAX, |a, b| a.min(*b)),
            knee.iter().fold(f32::MIN, |a, b| a.max(*b)),
            elbow.iter().fold(f32::MAX, |a, b| a.min(*b)),
            elbow.iter().fold(f32::MIN, |a, b| a.max(*b)),
            reach.iter().fold(f32::MAX, |a, b| a.min(*b)),
            reach.iter().fold(f32::MIN, |a, b| a.max(*b)),
            spread(&saddle),
            spread(&skull),
            spread(&nods),
            spread(&pitches),
        );
        // Leaning forward about 15–25° from the vertical.
        let lean = leans.iter().sum::<f32>() / leans.len() as f32;
        println!(
            "lean {lean:.1}°, spread {:.1}° against the saddle's {:.1}°",
            spread(&leans),
            spread(&pitches).to_degrees()
        );
        assert!((15.0..25.0).contains(&lean), "lean {lean}°");
        // The spine counters the saddle's pitch: the upper body pitches
        // less than the horse under it.
        assert!(
            spread(&leans) < 0.8 * spread(&pitches).to_degrees(),
            "the rider pitches {}° on a saddle pitching {}°",
            spread(&leans),
            spread(&pitches).to_degrees()
        );
        // The seat hovers and gives, never below the saddle.
        assert!(gap.iter().all(|g| *g > -0.002), "{gap:?}");
        assert!(spread(&gap) > 0.02, "the seat gives {}", spread(&gap));
        // Its head far from the horse's head and neck, whatever they do.
        assert!(
            closest > 0.3,
            "the rider's head {closest} m from the horse's"
        );
        // The knees take it: about 110°, opening and closing.
        let mean = knee.iter().sum::<f32>() / knee.len() as f32;
        assert!((100.0..125.0).contains(&mean), "knee {mean}°");
        assert!(spread(&knee) > 4.0, "the knee flexes {}°", spread(&knee));
        // The head rides steadier than the saddle, in height and in pitch.
        assert!(
            spread(&skull) < 0.8 * spread(&saddle),
            "{} {}",
            spread(&skull),
            spread(&saddle)
        );
        assert!(
            spread(&nods) < spread(&pitches),
            "{} {}",
            spread(&nods),
            spread(&pitches)
        );
        // The rein hand keeps an even contact: it follows the mouth, the
        // elbow opening as the head goes forward and closing as it comes
        // back.
        assert!(spread(&reach) < 0.08, "hand to bit {}", spread(&reach));
        let n = elbow.len() as f32;
        let (me_, mm) = (elbow.iter().sum::<f32>() / n, mouth.iter().sum::<f32>() / n);
        let cov: f32 = elbow
            .iter()
            .zip(&mouth)
            .map(|(e, m)| (e - me_) * (m - mm))
            .sum();
        let var = |v: &[f32], m: f32| v.iter().map(|x| (x - m) * (x - m)).sum::<f32>().sqrt();
        let corr = cov / (var(&elbow, me_) * var(&mouth, mm)).max(1e-9);
        assert!(corr > 0.3, "the elbow follows the mouth: {corr}");
    }

    #[test]
    fn walking_the_rider_sits_deep_and_upright() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = seated_every(rider, 1.5, 20);
        for _ in 0..80 {
            app.update();
        }
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        let mut gaps = Vec::new();
        let mut leans = Vec::new();
        for _ in 0..30 {
            app.update();
            let world = app.world();
            let model = named(world, carrier, "Rider model");
            let (pelvis, neck) = (
                named(world, model, "pelvis"),
                named(world, model, "neck_01"),
            );
            let g = |e: Entity| *world.get::<GlobalTransform>(e).unwrap();
            let seat = g(model).affine().inverse();
            let (p, n) = (
                seat.transform_point3(g(pelvis).translation()),
                seat.transform_point3(g(neck).translation()),
            );
            gaps.push(p.y);
            leans.push((n - p).z.atan2((n - p).y).to_degrees());
        }
        assert!(
            gaps.iter().all(|g| g.abs() < 0.005),
            "deep in the saddle: {gaps:?}"
        );
        let lean = leans.iter().sum::<f32>() / leans.len() as f32;
        // Upright: the shoulders over the hips (the seated clip leans back
        // about 5°).
        assert!(lean.abs() < 8.0, "upright: {lean}°");
    }

    /// Over `frames` frames of the rider on `app`'s horse: the free (right)
    /// elbow's angles (degrees), the free hand's places in the rider's seat
    /// frame, and how near the hand comes to the horse's skin (signed: below
    /// zero inside it) and to the saddle.
    fn free_arm(app: &mut App, me: Entity, frames: usize) -> (Vec<f32>, Vec<Vec3>, f32, f32) {
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        let saddle = ["Sudadera", "Carona", "Basto", "Pellon"];
        let (mut elbow, mut places) = (Vec::new(), Vec::new());
        let (mut skin_gap, mut saddle_gap) = (f32::MAX, f32::MAX);
        for k in 0..frames {
            app.update();
            let world = app.world();
            let model = named(world, carrier, "Rider model");
            let [upper, lower, hand, finger] =
                ["upperarm_r", "lowerarm_r", "hand_r", "middle_02_r"]
                    .map(|n| named(world, model, n));
            let at = |e: Entity| world.get::<GlobalTransform>(e).unwrap().translation();
            elbow.push(angle_at(at(upper), at(lower), at(hand)));
            let seat = world
                .get::<GlobalTransform>(model)
                .unwrap()
                .affine()
                .inverse();
            places.push(seat.transform_point3(at(hand)));
            let points = [at(hand), at(finger)];
            if k % 3 == 0 {
                let drawn = cpu::drawn(app.world_mut(), Some(carrier));
                let (skin, tris) = horse_skin(&drawn);
                for p in points {
                    skin_gap = skin_gap.min(signed_distance(p, &skin, &tris));
                    for d in drawn.iter().filter(|d| saddle.contains(&d.name.as_str())) {
                        for q in &d.positions {
                            saddle_gap = saddle_gap.min(p.distance(*q));
                        }
                    }
                }
            }
        }
        (elbow, places, skin_gap, saddle_gap)
    }

    #[test]
    fn the_free_arm_swings_with_the_gallop_less_at_the_walk_and_keeps_off_the_horse() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let mut seen = Vec::new();
        for (speed, frames) in [(12.2, 40), (1.5, 60), (0.0, 600)] {
            let (mut app, me) = seated_every(rider, speed, 20);
            for _ in 0..80 {
                app.update();
            }
            let (elbow, places, skin, saddle) = free_arm(&mut app, me, frames);
            let moved = places
                .iter()
                .map(|p| p.distance(places[0]))
                .fold(0.0, f32::max);
            let mean = elbow.iter().sum::<f32>() / elbow.len() as f32;
            println!(
                "{speed} m/s: elbow {:.0}° ({:.0}° range), hand moves {moved:.3} m, {skin:.3} m off the horse, {saddle:.3} m off the saddle",
                mean,
                spread(&elbow)
            );
            // Never in the horse or the saddle.
            assert!(skin > 0.02, "{speed} m/s: the hand {skin} m off the horse");
            assert!(
                saddle > 0.02,
                "{speed} m/s: the hand {saddle} m off the saddle"
            );
            seen.push((spread(&elbow), moved, mean));
        }
        let [
            (gallop_bend, gallop_moves, _),
            (walk_bend, walk_moves, walk_mean),
            _,
        ] = seen[..]
        else {
            unreachable!()
        };
        // The gallop swings it, the elbow flexing; the walk less, but some.
        assert!(
            gallop_bend > 6.0 && gallop_moves > 0.03,
            "{gallop_bend}° {gallop_moves} m"
        );
        assert!(
            gallop_moves < FREE_REACH + 0.05,
            "never flailing: {gallop_moves}"
        );
        assert!(walk_moves < gallop_moves && walk_bend < gallop_bend);
        assert!(walk_moves > 0.003, "the walk sways it: {walk_moves}");
        // At rest the elbow is bent about 30–60° (an angle of 120–150°).
        assert!((120.0..150.0).contains(&walk_mean), "{walk_mean}°");
    }

    #[test]
    fn getting_off_leaves_nothing_of_the_horse_and_getting_on_again_seats_the_rider() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = seated(rider, 0.0);
        let count = |app: &mut App| {
            let world = app.world_mut();
            (
                world.query::<&HorsePiece>().iter(world).count(),
                world.query::<&Riding>().iter(world).count(),
                world.query::<&HorseModel>().iter(world).count(),
            )
        };
        assert_eq!(count(&mut app), (22, 1, 1));
        app.world_mut()
            .entity_mut(me)
            .insert(CharacterVisual::Figure(FigureKind::Protagonist));
        app.update();
        assert_eq!(count(&mut app), (0, 0, 0), "nothing of the horse left");
        // Back in the saddle: the new horse dresses and seats its rider.
        app.world_mut().entity_mut(me).insert(rider);
        for _ in 0..200 {
            app.update();
            let world = app.world_mut();
            let seated = world
                .query::<(&Rig, &Seated)>()
                .iter(world)
                .any(|(r, _)| r.started);
            if seated {
                break;
            }
        }
        app.update();
        assert_eq!(count(&mut app), (22, 1, 1));
        let world = app.world_mut();
        let (riding, seated) = world.query::<(&Riding, &Seated)>().single(world).unwrap();
        let (horse, hand) = (
            riding.horse,
            seated.bones[Bone::ALL.iter().position(|b| *b == Bone::Hand(0)).unwrap()],
        );
        let tack = *world.get::<Tack>(horse).unwrap();
        assert_eq!(world.get::<Strap>(tack.reins[0]).unwrap().ends[1].0, hand);
    }

    #[test]
    fn the_riders_poncho_stays_over_his_shoulder_and_arm() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        for (speed, frames) in [(0.0, 10), (1.5, 30), (12.2, 30)] {
            let (mut app, me) = seated_every(rider, speed, 20);
            for _ in 0..80 {
                app.update();
            }
            let carrier = app.world().get::<Children>(me).unwrap()[0];
            let mut deepest = 0.0f32;
            for _ in 0..frames {
                app.update();
                let model = named(app.world(), carrier, "Rider model");
                deepest = deepest.max(poncho_in_arms(app.world_mut(), model));
            }
            println!("{speed} m/s: the poncho {deepest:.3} m into an arm");
            assert!(
                deepest < 0.005,
                "{speed} m/s: the poncho {deepest} m into an arm"
            );
        }
    }
}
