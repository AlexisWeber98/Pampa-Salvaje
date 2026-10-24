//! The bone driver: poses a MakeHuman model's `game_engine` skeleton like
//! the rigid figure it rides on (see [`model`](super::model)), with a
//! relaxed, living posture on top.
//!
//! The figure stays the carrier. Its body turns to the heading and bobs
//! with the stride, carrying the model, and [`update_figures`] still poses
//! its hidden joints and keeps the [`JointAngles`] it computed on the
//! [`FigureBody`]. Every frame [`drive_rigs`] turns the model's bones by
//! the same angles:
//!
//! | figure joint | bones |
//! |---|---|
//! | pelvis: the bob | `pelvis`: it skips the bob and sinks to plant a foot instead |
//! | waist: the lean, and the breath | `spine_01..03`, a third each; `spine_02` also rocks with the breath |
//! | neck: the nod, the glance | `neck_01` and `head`, half each |
//! | shoulder, elbow | `upperarm_l/r`, `lowerarm_l/r` |
//! | hip (with the rider's spread), knee | `thigh_l/r`, `calf_l/r` |
//! | none | `foot_l/r`: kept level, or pointed to touch the ground |
//!
//! A figure's turns are in the body frame (+X front, +Y up, left −Z;
//! positive Z swings a hanging limb forward), and its joints all rest
//! unturned. The bones rest turned every which way (each points along its
//! local +Y), so a turn `d` is conjugated into the bone's parent space:
//! with `f` the parent's rest orientation in the body frame,
//! `local = f⁻¹ · d · f · rest`. Applied down the chain, each bone ends up
//! turned in the body frame by its own and its ancestors' turns, as the
//! figure's parts are, about its own pivot. The thighs are the exception:
//! their turns hold in the body frame whatever the pelvis does.
//!
//! On foot the posture ([`posture`]) loosens the figure's soldierly
//! stance: standing, the weight rests on one leg (contrapposto: the pelvis
//! drops toward the free leg, whose knee bends, and the spine answers with
//! a slight S), the shoulders sit a touch down and forward, the arms hang a
//! little out with the elbows bent, and the weight sways slowly; walking,
//! the pelvis turns with the stride and drops on the swing side while the
//! torso turns against it. Each person stands their own way, from their
//! seed. Then [`Rig::solve`] keeps a foot on the ground: the supporting
//! foot stays where it stands, the hips sink until the lower foot is down,
//! and a free foot points its toe to the ground.
//!
//! Where the pose comes from is one seam, [`rig_pose`]: today the figure's
//! joint angles ([`joint_pose`]); a later clip player only has to yield a
//! [`RigPose`] too. The driver allocates nothing per frame and freezes a
//! model out of view or past [`FREEZE_FAR`](crate::fauna::FREEZE_FAR) like
//! the livestock (the protagonist, only ever the player, never freezes).

use std::f32::consts::TAU;

use bevy::prelude::*;
use pampa_core::JointAngles;

use super::figure::{FigureBody, FigureKind, Joint, Side};
use crate::fauna::livestock::frozen;

/// How far the chest rocks forward per meter of the figure's breath rise,
/// radians: the figure's 12 mm breath rocks it 0.012 rad.
pub const BREATH_PITCH: f32 = 1.0;

/// How far the hips may sink to put a foot on the ground, meters: all the
/// way through a walking step, while a jog leaves the ground near the ends
/// of its stride.
pub const PLANT_DEPTH: f32 = 0.08;

/// How far a free foot may point its toe down to touch the ground, radians.
pub const TOE_POINT: f32 = 0.6;

/// A bone the driver turns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bone {
    /// It turns and sways with the posture, and rises and sinks to plant a
    /// foot.
    Pelvis,
    Spine01,
    Spine02,
    Spine03,
    Neck,
    Head,
    Clavicle(Side),
    UpperArm(Side),
    LowerArm(Side),
    Thigh(Side),
    Calf(Side),
    Foot(Side),
    /// Never turned: where the toes bend, for pointing a foot.
    Ball(Side),
}

impl Bone {
    /// How many bones are driven.
    pub const COUNT: usize = 20;

    /// Every driven bone, in [`Bone::index`] order.
    pub const ALL: [Self; Self::COUNT] = [
        Self::Pelvis,
        Self::Spine01,
        Self::Spine02,
        Self::Spine03,
        Self::Neck,
        Self::Head,
        Self::Clavicle(Side::Left),
        Self::Clavicle(Side::Right),
        Self::UpperArm(Side::Left),
        Self::UpperArm(Side::Right),
        Self::LowerArm(Side::Left),
        Self::LowerArm(Side::Right),
        Self::Thigh(Side::Left),
        Self::Thigh(Side::Right),
        Self::Calf(Side::Left),
        Self::Calf(Side::Right),
        Self::Foot(Side::Left),
        Self::Foot(Side::Right),
        Self::Ball(Side::Left),
        Self::Ball(Side::Right),
    ];

    /// Index into [`Bone::ALL`] and a [`RigPose`].
    pub fn index(self) -> usize {
        let side = |s: Side| s.index();
        match self {
            Self::Pelvis => 0,
            Self::Spine01 => 1,
            Self::Spine02 => 2,
            Self::Spine03 => 3,
            Self::Neck => 4,
            Self::Head => 5,
            Self::Clavicle(s) => 6 + side(s),
            Self::UpperArm(s) => 8 + side(s),
            Self::LowerArm(s) => 10 + side(s),
            Self::Thigh(s) => 12 + side(s),
            Self::Calf(s) => 14 + side(s),
            Self::Foot(s) => 16 + side(s),
            Self::Ball(s) => 18 + side(s),
        }
    }

    /// The bone's name in the `game_engine` skeleton.
    pub fn name(self) -> &'static str {
        let lr = |s: Side, l: &'static str, r: &'static str| match s {
            Side::Left => l,
            Side::Right => r,
        };
        match self {
            Self::Pelvis => "pelvis",
            Self::Spine01 => "spine_01",
            Self::Spine02 => "spine_02",
            Self::Spine03 => "spine_03",
            Self::Neck => "neck_01",
            Self::Head => "head",
            Self::Clavicle(s) => lr(s, "clavicle_l", "clavicle_r"),
            Self::UpperArm(s) => lr(s, "upperarm_l", "upperarm_r"),
            Self::LowerArm(s) => lr(s, "lowerarm_l", "lowerarm_r"),
            Self::Thigh(s) => lr(s, "thigh_l", "thigh_r"),
            Self::Calf(s) => lr(s, "calf_l", "calf_r"),
            Self::Foot(s) => lr(s, "foot_l", "foot_r"),
            Self::Ball(s) => lr(s, "ball_l", "ball_r"),
        }
    }

    /// The driven bone called `name`, if any.
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|b| b.name() == name)
    }
}

/// One frame's pose.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RigPose {
    /// Each driven bone's turn in the body frame, in [`Bone::ALL`] order:
    /// relative to its parent bone's turn, except the thighs', which hold
    /// whatever the pelvis does. A foot's turn is from level.
    pub turns: [Quat; Bone::COUNT],
    /// How far the hips move from their rest place on the body, meters,
    /// before they sink to plant a foot.
    pub shift: Vec3,
    /// How far the hips rise besides, meters: the body's own rise, undone.
    pub lift: f32,
    /// Whether the hips also sink, up to [`PLANT_DEPTH`], until the lower
    /// foot is on the ground (see [`Rig::solve`]).
    pub plant: bool,
    /// The leg the weight is on, and how firmly (`0..1`): its foot stays
    /// where it rests on the ground whatever the hips do.
    pub support: Option<(Side, f32)>,
    /// How far (`0..1`) a foot above the ground points its toe to touch it.
    pub touch: f32,
}

impl RigPose {
    /// Standing at rest: nothing turned.
    pub const REST: Self = Self {
        turns: [Quat::IDENTITY; Bone::COUNT],
        shift: Vec3::ZERO,
        lift: 0.0,
        plant: false,
        support: None,
        touch: 0.0,
    };

    pub fn get(&self, bone: Bone) -> Quat {
        self.turns[bone.index()]
    }

    fn set(&mut self, bone: Bone, turn: Quat) {
        self.turns[bone.index()] = turn;
    }

    /// Turns `bone` further by `turn`, after what it has.
    fn add(&mut self, bone: Bone, turn: Quat) {
        let i = bone.index();
        self.turns[i] *= turn;
    }
}

/// What the posture needs to know of a figure besides its joint angles.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Bearing {
    /// Its own seed: how it stands.
    pub seed: u32,
    /// Its secondary-motion clock, seconds (see [`pampa_core::tick_clock`]).
    pub clock: f32,
    /// How still it is, from walking (`0`) to standing (`1`).
    pub stillness: f32,
    /// On its own feet (not riding): only then has it a posture.
    pub on_foot: bool,
}

/// How one person stands, from their seed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stance {
    /// The leg that rests while the other bears the weight.
    pub free: Side,
    /// Seconds per slow sway of the weight: whole seconds from 6 to 9, so
    /// the sway runs on seamlessly as the motion clock wraps.
    pub sway_period: f32,
    /// Where in its sway it starts, radians.
    pub sway_start: f32,
    /// How each arm differs from the plain posture, radians: left, right.
    pub arms: [f32; 2],
}

impl Stance {
    /// The stance of `seed`.
    pub fn of(seed: u32) -> Self {
        // A small integer hash (from splitmix32).
        let mut h = seed.wrapping_add(0x9E37_79B9);
        h = (h ^ (h >> 16)).wrapping_mul(0x21F0_AAAD);
        h = (h ^ (h >> 15)).wrapping_mul(0x735A_2D97);
        h ^= h >> 15;
        let unit = |shift: u32| ((h >> shift) & 0xFF) as f32 / 255.0;
        Self {
            free: if h & 1 == 0 { Side::Left } else { Side::Right },
            sway_period: 6.0 + ((h >> 1) % 4) as f32,
            sway_start: unit(3) * TAU,
            arms: [0.04 * (unit(11) - 0.5), 0.04 * (unit(19) - 0.5)],
        }
    }
}

/// The standing posture's angles, radians (and meters for the shift).
mod relaxed {
    /// The pelvis drops this far toward the free leg, and sways by this
    /// much more or less.
    pub const TILT: f32 = 0.06;
    pub const TILT_SWAY: f32 = 0.015;
    /// The hips move this far over the supporting foot, and sway by this
    /// much more or less, meters.
    pub const SHIFT: f32 = 0.025;
    pub const SHIFT_SWAY: f32 = 0.01;
    /// The free leg: its thigh forward a little, its knee bent.
    pub const FREE_HIP: f32 = 0.14;
    pub const FREE_KNEE: f32 = 0.22;
    /// The toes turned a little out.
    pub const TOE_OUT: f32 = 0.12;
    /// The supporting knee, not quite locked.
    pub const SUPPORT_KNEE: f32 = 0.05;
    /// The spine's S: the small of the back curves back, the chest forward.
    pub const LUMBAR: f32 = 0.035;
    pub const THORAX: f32 = 0.045;
    /// The shoulders a touch down and forward.
    pub const SHOULDER_DOWN: f32 = 0.05;
    pub const SHOULDER_FORWARD: f32 = 0.06;
    /// The upper arms a little out from the body and forward.
    pub const ARM_OUT: f32 = 0.09;
    pub const ARM_FORWARD: f32 = 0.06;
    /// More bend at the elbows than the rigid figure's.
    pub const ELBOW: f32 = 0.13;
    /// Walking: the pelvis turns with the forward leg, by this much of the
    /// hips' spread, and drops on the swing side by this much of the knees'
    /// difference.
    pub const WALK_TURN: f32 = 0.1;
    pub const WALK_DROP: f32 = 0.05;
    /// How much of the pelvis's turn the spine turns back against.
    pub const COUNTER: f32 = 1.6;
}

/// The rigid figure's joint angles, and its head's `glance`, as bone turns,
/// with the posture ([`posture`]) when on foot.
pub fn joint_pose(a: &JointAngles, glance: f32, bearing: &Bearing) -> RigPose {
    // On foot the model skips the stride's bob the body carries the figure
    // by: its hips sink to plant a foot instead.
    let mut pose = RigPose {
        plant: bearing.on_foot,
        lift: if bearing.on_foot { -a.bob } else { 0.0 },
        ..RigPose::REST
    };
    // The waist's lean spread over the spine; turns about one axis add up.
    let lean = part(Joint::Waist.rotation(a, glance), 1.0 / 3.0);
    let chest = Quat::from_rotation_z(-BREATH_PITCH * a.breath);
    pose.set(Bone::Spine01, lean);
    pose.set(Bone::Spine02, lean * chest);
    pose.set(Bone::Spine03, lean);
    // The neck's nod and glance, half at the neck and half at the head.
    let neck = part(Joint::Neck.rotation(a, glance), 0.5);
    pose.set(Bone::Neck, neck);
    pose.set(Bone::Head, neck);
    for side in Side::BOTH {
        pose.set(Bone::Thigh(side), Joint::Hip(side).rotation(a, glance));
        pose.set(Bone::Calf(side), Joint::Knee(side).rotation(a, glance));
        pose.set(
            Bone::UpperArm(side),
            Joint::Shoulder(side).rotation(a, glance),
        );
        pose.set(Bone::LowerArm(side), Joint::Elbow(side).rotation(a, glance));
    }
    if bearing.on_foot {
        posture(&mut pose, a, bearing);
    }
    pose
}

/// Loosens `pose`, a figure's on foot, into a person's: see the module
/// notes. Standing (`bearing.stillness` 1) it rests on one leg and sways;
/// walking (0) its pelvis and torso turn against each other.
pub fn posture(pose: &mut RigPose, a: &JointAngles, bearing: &Bearing) {
    use relaxed::*;
    let still = bearing.stillness.clamp(0.0, 1.0);
    let stance = Stance::of(bearing.seed);
    let sway = (TAU * bearing.clock / stance.sway_period + stance.sway_start).sin();
    let (free, support) = match stance.free {
        Side::Left => (Side::Left, Side::Right),
        Side::Right => (Side::Right, Side::Left),
    };
    // Turning about X by `z * angle` drops the side at `z`.
    let drop = free.z() * (TILT + TILT_SWAY * sway);
    // A bent knee is negative: the pelvis drops on the leg that swings.
    let swing = WALK_DROP * (a.knee[0] - a.knee[1]);
    let roll = still * drop + (1.0 - still) * swing;
    // The left hip forward (its leg ahead) turns the pelvis to the right.
    let yaw = -(1.0 - still) * WALK_TURN * (a.hip[0] - a.hip[1]) / 2.0;
    let pelvis = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(roll);
    pose.set(Bone::Pelvis, pelvis);
    pose.shift += Vec3::Z * -free.z() * still * (SHIFT + SHIFT_SWAY * sway);

    // The spine turns back against the pelvis, past it at the shoulders,
    // and curves in a slight S.
    let back = |share: f32| {
        Quat::from_rotation_y(-COUNTER * yaw * share)
            * Quat::from_rotation_x(-COUNTER * roll * share)
    };
    let curve = [LUMBAR, 0.0, -THORAX].map(Quat::from_rotation_z);
    let spine = [Bone::Spine01, Bone::Spine02, Bone::Spine03];
    for (bone, curve) in spine.into_iter().zip(curve) {
        pose.add(bone, back(1.0 / 3.0) * curve);
    }
    // The head stays level: the neck undoes the pelvis, the spine's turn
    // back and its S.
    let level = (pelvis * back(1.0) * curve[0] * curve[2]).inverse();
    let level = part(level, 0.5);
    pose.add(Bone::Neck, level);
    pose.add(Bone::Head, level);

    for side in Side::BOTH {
        let z = side.z();
        let k = side.index();
        // Turning about Y by `-z * angle` turns the toes out.
        pose.set(Bone::Foot(side), Quat::from_rotation_y(-z * TOE_OUT));
        pose.set(
            Bone::Clavicle(side),
            Quat::from_rotation_y(z * SHOULDER_FORWARD) * Quat::from_rotation_x(z * SHOULDER_DOWN),
        );
        // Out (away from the body) and forward, each arm its own way.
        let arm = Quat::from_rotation_z(ARM_FORWARD + stance.arms[k])
            * Quat::from_rotation_x(-z * (ARM_OUT + stance.arms[k]));
        pose.set(Bone::UpperArm(side), pose.get(Bone::UpperArm(side)) * arm);
        pose.add(
            Bone::LowerArm(side),
            Quat::from_rotation_z(ELBOW + stance.arms[1 - k]),
        );
    }
    pose.add(Bone::Thigh(free), Quat::from_rotation_z(still * FREE_HIP));
    pose.add(Bone::Calf(free), Quat::from_rotation_z(-still * FREE_KNEE));
    pose.add(
        Bone::Thigh(support),
        Quat::from_rotation_z(still * SUPPORT_KNEE / 2.0),
    );
    pose.add(
        Bone::Calf(support),
        Quat::from_rotation_z(-still * SUPPORT_KNEE),
    );
    pose.support = Some((support, still));
    pose.touch = still;
}

/// The fraction `t` of `turn`, about the same axis.
fn part(turn: Quat, t: f32) -> Quat {
    let (axis, angle) = turn.to_axis_angle();
    Quat::from_axis_angle(axis, angle * t)
}

/// Where a figure's model takes its pose from this frame: the one seam an
/// animation source plugs into. Today it is the figure's own joint angles.
pub fn rig_pose(body: &FigureBody) -> RigPose {
    let bearing = Bearing {
        seed: body.seed,
        clock: body.clock,
        stillness: body.stillness,
        on_foot: body.seat.is_none(),
    };
    joint_pose(&body.angles, body.glance, &bearing)
}

/// A pose worked out on a rig: each bone's local rotation, and the
/// pelvis's local place.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Solved {
    pub local: [Quat; Bone::COUNT],
    pub pelvis: Vec3,
    /// Each ankle's height above its rest height on the body once the
    /// hips have sunk, left and right, meters.
    pub ankles: [f32; 2],
}

/// A character model's skeleton, found when its scene spawns; on the
/// model's root.
#[derive(Component, Debug, Clone)]
pub struct Rig {
    /// The bone entities, in [`Bone::ALL`] order (`None`: not in the model).
    bones: [Option<Entity>; Bone::COUNT],
    /// Each bone's rest rotation, in its parent's space.
    rest: [Quat; Bone::COUNT],
    /// Each bone's parent's rest orientation in the body frame.
    frame: [Quat; Bone::COUNT],
    /// Each bone's rest place, in its parent's space.
    at: [Vec3; Bone::COUNT],
    /// A mesh of the model, whose visibility tells whether it is in view.
    pub probe: Option<Entity>,
    /// Whether the bones keep their last pose (see [`frozen`]).
    pub frozen: bool,
}

impl Default for Rig {
    fn default() -> Self {
        Self {
            bones: [None; Bone::COUNT],
            rest: [Quat::IDENTITY; Bone::COUNT],
            frame: [Quat::IDENTITY; Bone::COUNT],
            at: [Vec3::ZERO; Bone::COUNT],
            probe: None,
            frozen: false,
        }
    }
}

impl Rig {
    /// Records `bone`: its entity, its `rest` transform, and its parent's
    /// rest orientation in the body frame.
    pub fn bind(&mut self, bone: Bone, entity: Entity, rest: &Transform, frame: Quat) {
        let i = bone.index();
        self.bones[i] = Some(entity);
        self.rest[i] = rest.rotation;
        self.at[i] = rest.translation;
        self.frame[i] = frame;
    }

    /// The entity of `bone`, if the model has it.
    pub fn bone(&self, bone: Bone) -> Option<Entity> {
        self.bones[bone.index()]
    }

    /// Whether every driven bone was found.
    pub fn is_complete(&self) -> bool {
        self.bones.iter().all(Option::is_some)
    }

    /// `bone`'s local rotation once turned by `turn` in the body frame.
    pub fn local(&self, bone: Bone, turn: Quat) -> Quat {
        let i = bone.index();
        let frame = self.frame[i];
        (frame.inverse() * turn * frame * self.rest[i]).normalize()
    }

    /// `bone`'s rest offset from its parent's joint, in the body frame.
    fn offset(&self, bone: Bone) -> Vec3 {
        let i = bone.index();
        self.frame[i] * self.at[i]
    }

    /// Works `pose` out on this skeleton. The supporting foot stays where
    /// it rests whatever the hips do; then, when the pose plants, the hips
    /// sink, by up to [`PLANT_DEPTH`], until the lower ankle is back at its
    /// rest height; the feet stay level, and one above the ground points
    /// its toe down to touch it, as far as the pose's `touch`.
    pub fn solve(&self, pose: &RigPose) -> Solved {
        let pelvis = pose.get(Bone::Pelvis);
        let mut thighs = [Quat::IDENTITY; 2];
        let mut ankles = [0.0; 2];
        for side in Side::BOTH {
            let k = side.index();
            let (knee, ankle) = (self.offset(Bone::Calf(side)), self.offset(Bone::Foot(side)));
            let rest = self.offset(Bone::Thigh(side)) + knee + ankle;
            let hip = pose.shift + pelvis * self.offset(Bone::Thigh(side));
            let calf = pose.get(Bone::Calf(side));
            let mut thigh = pose.get(Bone::Thigh(side));
            if let Some((support, firm)) = pose.support
                && support == side
                && self.is_complete()
            {
                // The leg turned to reach where its ankle rests.
                let reach = thigh * knee + thigh * calf * ankle;
                let aim = Quat::from_rotation_arc(reach.normalize(), (rest - hip).normalize());
                thigh = part(aim, firm) * thigh;
            }
            thighs[k] = thigh;
            ankles[k] = (hip + thigh * knee + thigh * calf * ankle - rest).y;
        }
        let low = ankles[0].min(ankles[1]);
        let sink = if pose.plant && self.is_complete() {
            -low.min(PLANT_DEPTH)
        } else {
            0.0
        };
        let mut turns = pose.turns;
        for side in Side::BOTH {
            let k = side.index();
            let calf = pose.get(Bone::Calf(side));
            // Pointed so the ball comes down by its height above the ground.
            let ball = self.offset(Bone::Ball(side));
            let above = (ankles[k] - low).max(0.0);
            let point = pose.touch * (above / ball.x.max(0.05)).min(1.0).asin().min(TOE_POINT);
            let sole = Quat::from_rotation_z(-point) * pose.get(Bone::Foot(side));
            turns[Bone::Thigh(side).index()] = pelvis.inverse() * thighs[k];
            turns[Bone::Foot(side).index()] = (thighs[k] * calf).inverse() * sole;
            ankles[k] += pose.lift + sink;
        }
        let mut local = [Quat::IDENTITY; Bone::COUNT];
        for bone in Bone::ALL {
            local[bone.index()] = self.local(bone, turns[bone.index()]);
        }
        let i = Bone::Pelvis.index();
        let lift = pose.shift + Vec3::Y * (pose.lift + sink);
        Solved {
            local,
            pelvis: self.at[i] + self.frame[i].inverse() * lift,
            ankles,
        }
    }

    /// Poses every bone found by [`Rig::solve`], from its rest: nothing
    /// accumulates.
    pub fn apply(&self, pose: &RigPose, transforms: &mut Query<&mut Transform>) {
        let solved = self.solve(pose);
        for bone in Bone::ALL {
            let Some(entity) = self.bone(bone) else {
                continue;
            };
            if let Ok(mut transform) = transforms.get_mut(entity) {
                transform.rotation = solved.local[bone.index()];
                if bone == Bone::Pelvis {
                    transform.translation = solved.pelvis;
                }
            }
        }
    }
}

/// How far each finger joint curls, knuckle to tip, radians: a hand at
/// rest is never flat.
pub const FINGER_CURL: [f32; 3] = [0.2, 0.35, 0.25];

/// The rest rotation of a finger bone called `name`, curled toward the palm
/// from `rest`; `None` for any other bone. (In the `game_engine` skeleton
/// both hands' fingers curl about their own +X.)
pub fn curled(name: &str, rest: Quat) -> Option<Quat> {
    let finger = ["index_", "middle_", "ring_", "pinky_"]
        .iter()
        .any(|f| name.starts_with(f));
    let joint = name.get(name.len().checked_sub(4)?..name.len().checked_sub(2)?)?;
    let k = ["01", "02", "03"].iter().position(|j| *j == joint)?;
    finger.then(|| rest * Quat::from_rotation_x(FINGER_CURL[k]))
}

/// The rest orientation in the body frame of the space `entity` hangs in:
/// its ancestors' rotations, up to and including the model's `root`
/// (turned to the body's front).
pub(super) fn rest_frame(
    entity: Entity,
    root: Entity,
    parents: &Query<&ChildOf>,
    transforms: &Query<&Transform>,
) -> Quat {
    let mut frame = Quat::IDENTITY;
    let mut at = entity;
    while at != root {
        let Ok(child_of) = parents.get(at) else {
            break;
        };
        at = child_of.parent();
        if let Ok(t) = transforms.get(at) {
            frame = t.rotation * frame;
        }
    }
    frame
}

/// Poses each model's bones from its figure ([`rig_pose`]); a model out of
/// view or far off keeps its last pose, except the protagonist's.
pub(super) fn drive_rigs(
    cameras: Query<&GlobalTransform, With<Camera3d>>,
    bodies: Query<&FigureBody>,
    mut rigs: Query<(&ChildOf, &mut Rig, &GlobalTransform)>,
    seen: Query<&ViewVisibility>,
    mut transforms: Query<&mut Transform>,
) {
    let eye = cameras.iter().next().map(GlobalTransform::translation);
    for (child_of, mut rig, at) in &mut rigs {
        let Ok(body) = bodies.get(child_of.parent()) else {
            continue;
        };
        let distance = eye.map_or(0.0, |e| e.distance(at.translation()));
        // Visibility is only known with a camera drawing.
        let in_view = eye.is_none() || rig.probe.is_none_or(|p| seen.get(p).is_ok_and(|v| v.get()));
        let freeze = body.kind != FigureKind::Protagonist && frozen(rig.frozen, distance, in_view);
        rig.frozen = freeze;
        if !freeze {
            rig.apply(&rig_pose(body), &mut transforms);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pampa_core::{AnimationState, MOTION_CLOCK_PERIOD, pose};

    /// A figure's pose with no posture, as a rider's.
    fn rigid(a: &JointAngles) -> RigPose {
        joint_pose(a, 0.0, &Bearing::default())
    }

    /// Standing still, on foot, `seed`'s posture at `clock`.
    fn standing(seed: u32, clock: f32) -> RigPose {
        let bearing = Bearing {
            seed,
            clock,
            stillness: 1.0,
            on_foot: true,
        };
        joint_pose(&pose::pose(AnimationState::Idle, 0.0), 0.0, &bearing)
    }

    #[test]
    fn every_bone_has_its_own_index_and_name() {
        for (k, bone) in Bone::ALL.into_iter().enumerate() {
            assert_eq!(bone.index(), k);
            assert_eq!(Bone::named(bone.name()), Some(bone));
        }
        assert_eq!(Bone::named("hand_l"), None);
    }

    #[test]
    fn the_lean_and_the_nod_add_up_over_their_chains() {
        let a = JointAngles {
            lean: 0.3,
            nod: 0.1,
            ..JointAngles::default()
        };
        let pose = joint_pose(&a, 0.4, &Bearing::default());
        let spine = pose.get(Bone::Spine01) * pose.get(Bone::Spine02) * pose.get(Bone::Spine03);
        assert!(spine.angle_between(Joint::Waist.rotation(&a, 0.4)) < 1e-3);
        let neck = pose.get(Bone::Neck) * pose.get(Bone::Head);
        assert!(neck.angle_between(Joint::Neck.rotation(&a, 0.4)) < 1e-3);
    }

    #[test]
    fn a_turn_in_the_body_frame_reads_the_same_on_a_turned_bone() {
        // A bone whose parent rests turned every which way: turning it by
        // `d` in the body frame turns its whole frame by `d`.
        let frame = Quat::from_euler(EulerRot::XYZ, 0.7, -1.2, 2.1);
        let rest = Quat::from_euler(EulerRot::XYZ, -0.4, 0.3, 0.9);
        let mut rig = Rig::default();
        rig.bind(
            Bone::Thigh(Side::Left),
            Entity::PLACEHOLDER,
            &Transform::from_rotation(rest),
            frame,
        );
        let d = Quat::from_rotation_z(0.5);
        let turned = frame * rig.local(Bone::Thigh(Side::Left), d);
        assert!(turned.angle_between(d * frame * rest) < 1e-3);
        let still = rig.local(Bone::Thigh(Side::Left), Quat::IDENTITY);
        assert!(still.angle_between(rest) < 1e-3);
    }

    #[test]
    fn each_person_stands_their_own_way() {
        let stances: Vec<Stance> = (0..64).map(Stance::of).collect();
        assert!(stances.iter().any(|s| s.free == Side::Left));
        assert!(stances.iter().any(|s| s.free == Side::Right));
        let periods: std::collections::HashSet<u32> =
            stances.iter().map(|s| s.sway_period as u32).collect();
        assert_eq!(periods.len(), 4, "{periods:?}");
        for s in &stances {
            assert!((6.0..=9.0).contains(&s.sway_period) && s.sway_period.fract() == 0.0);
            assert!(s.arms.iter().all(|a| a.abs() <= 0.02));
        }
        assert!(stances.iter().filter(|s| s.arms[0] != s.arms[1]).count() > 56);
        assert_eq!(Stance::of(7), Stance::of(7), "deterministic");
        // Standing, the sides differ: one knee bends, the other barely.
        for seed in 0..8 {
            let pose = standing(seed, 0.0);
            let free = Stance::of(seed).free;
            let other = if free == Side::Left {
                Side::Right
            } else {
                Side::Left
            };
            let bend = |s: Side| pose.get(Bone::Calf(s)).angle_between(Quat::IDENTITY);
            assert!(bend(free) > 0.15 && bend(other) < 0.06, "{seed}");
            // The pelvis drops toward the free leg; the arms hang a little out.
            let dropped = pose.get(Bone::Pelvis) * (Vec3::Z * free.z());
            assert!(dropped.y < -0.02, "{seed}");
            assert_eq!(pose.support.map(|s| s.0), Some(other));
            for side in Side::BOTH {
                let hand = pose.get(Bone::UpperArm(side)) * Vec3::NEG_Y;
                assert!(hand.z * side.z() > 0.05 && hand.x > 0.0, "{seed} {side:?}");
                let elbow = pose.get(Bone::LowerArm(side)) * Vec3::NEG_Y;
                assert!(elbow.x > 0.15_f32.sin(), "{seed} {side:?}");
            }
        }
    }

    #[test]
    fn the_weight_sways_slowly_within_bounds_and_comes_back() {
        for seed in 0..8 {
            let period = Stance::of(seed).sway_period;
            let mut drops = Vec::new();
            for k in 0..200 {
                let t = k as f32 * 0.1;
                let pose = standing(seed, t);
                let again = standing(seed, t + period);
                for (a, b) in pose.turns.iter().zip(again.turns) {
                    assert!(a.angle_between(b) < 2e-3, "{seed} {t}");
                }
                assert!((pose.shift - again.shift).length() < 1e-3);
                let roll = pose.get(Bone::Pelvis).angle_between(Quat::IDENTITY);
                assert!(roll <= relaxed::TILT + relaxed::TILT_SWAY + 1e-4);
                assert!(pose.shift.length() <= relaxed::SHIFT + relaxed::SHIFT_SWAY + 1e-4);
                drops.push(roll);
            }
            let (lo, hi) = drops
                .iter()
                .fold((f32::MAX, f32::MIN), |(l, h), d| (l.min(*d), h.max(*d)));
            assert!(hi - lo > relaxed::TILT_SWAY, "it sways: {lo} {hi}");
            // Seamless as the motion clock wraps.
            let end = standing(seed, MOTION_CLOCK_PERIOD - 0.001);
            let start = standing(seed, 0.0);
            assert!(end.get(Bone::Pelvis).angle_between(start.get(Bone::Pelvis)) < 2e-3);
        }
    }

    #[test]
    fn a_rider_keeps_the_figures_pose() {
        let a = pose::riding_pose(pampa_core::HorseGait::Trot, 0.3);
        let bearing = Bearing {
            stillness: 1.0,
            ..Bearing::default()
        };
        let pose = joint_pose(&a, 0.0, &bearing);
        assert_eq!(pose.get(Bone::Pelvis), Quat::IDENTITY);
        assert_eq!(pose.support, None);
        assert!(!pose.plant && pose.touch == 0.0 && pose.shift == Vec3::ZERO && pose.lift == 0.0);
    }

    #[test]
    fn fingers_curl_and_nothing_else_does() {
        let rest = Quat::from_rotation_y(0.3);
        for name in ["index_01_l", "middle_02_r", "ring_03_l", "pinky_01_r"] {
            let curl = curled(name, rest).unwrap();
            assert!(curl.angle_between(rest) > 0.15, "{name}");
        }
        for name in ["thumb_02_l", "hand_l", "head", "index", "_01_l"] {
            assert_eq!(curled(name, rest), None, "{name}");
        }
    }

    /// The character plugin with the asset server, and `people` standing
    /// a meter apart along +X, posed `animation`; returns once every one's
    /// model has its skeleton, each rig with its figure's kind.
    fn rigged(
        people: &[FigureKind],
        animation: AnimationState,
        speed: f32,
    ) -> (App, Vec<(FigureKind, Entity)>) {
        use crate::character::{CharacterVisual, CharacterVisualPlugin, VisualPose};
        use crate::gltf::tests::{root, unfinished_app};
        let mut app = unfinished_app(&root());
        app.init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>()
            .insert_resource(root())
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_millis(50),
            ))
            .add_plugins(CharacterVisualPlugin);
        app.finish();
        app.cleanup();
        for (k, &kind) in people.iter().enumerate() {
            app.world_mut().spawn((
                CharacterVisual::Figure(kind),
                VisualPose {
                    animation,
                    speed,
                    ..VisualPose::default()
                },
                Transform::from_xyz(k as f32, 0.0, 0.0),
            ));
        }
        for _ in 0..2000 {
            app.update();
            let mut rigs = app.world_mut().query::<(Entity, &Rig, &ChildOf)>();
            let found: Vec<(Entity, Entity)> = rigs
                .iter(app.world())
                .map(|(e, _, c)| (e, c.parent()))
                .collect();
            if found.len() == people.len() {
                let world = app.world();
                let kinds = found
                    .into_iter()
                    .map(|(rig, body)| (world.get::<FigureBody>(body).unwrap().kind, rig))
                    .collect();
                return (app, kinds);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("the models never spawned");
    }

    /// The protagonist alone, posed `animation`, and his model's rig.
    fn protagonist(animation: AnimationState, speed: f32) -> (App, Entity) {
        let (app, rigs) = rigged(&[FigureKind::Protagonist], animation, speed);
        (app, rigs[0].1)
    }

    /// Where the origin of `entity` is in the frame of the figure body
    /// above it.
    fn in_body(world: &World, entity: Entity) -> Vec3 {
        let mut point = Vec3::ZERO;
        let mut at = entity;
        while world.get::<FigureBody>(at).is_none() {
            point = world.get::<Transform>(at).unwrap().transform_point(point);
            at = world.get::<ChildOf>(at).unwrap().parent();
        }
        point
    }

    /// The entity named `name` under the model.
    fn named(world: &mut World, name: &str) -> Entity {
        let mut names = world.query::<(Entity, &Name)>();
        names
            .iter(world)
            .find(|(_, n)| n.as_str() == name)
            .map(|(e, _)| e)
            .unwrap()
    }

    /// Poses the model's bones by `pose`, as the driver does.
    fn pose_by(app: &mut App, rig: Entity, pose: &RigPose) {
        let world = app.world_mut();
        let rig = world.get::<Rig>(rig).unwrap().clone();
        let solved = rig.solve(pose);
        for bone in Bone::ALL {
            let entity = rig.bone(bone).unwrap();
            let mut transform = world.get_mut::<Transform>(entity).unwrap();
            transform.rotation = solved.local[bone.index()];
            if bone == Bone::Pelvis {
                transform.translation = solved.pelvis;
            }
        }
    }

    /// The turn of `entity` in the frame of the figure body above it.
    fn turn_in_body(world: &World, entity: Entity) -> Quat {
        let mut turn = Quat::IDENTITY;
        let mut at = entity;
        while world.get::<FigureBody>(at).is_none() {
            turn = world.get::<Transform>(at).unwrap().rotation * turn;
            at = world.get::<ChildOf>(at).unwrap().parent();
        }
        turn
    }

    #[test]
    fn the_protagonists_bones_turn_the_way_the_figures_joints_do() {
        let (mut app, rig) = protagonist(AnimationState::Idle, 0.0);
        assert!(app.world().get::<Rig>(rig).unwrap().is_complete());
        let rest = JointAngles::default();
        let at = |app: &mut App, angles: &JointAngles, name: &str| {
            pose_by(app, rig, &rigid(angles));
            let entity = named(app.world_mut(), name);
            in_body(app.world(), entity)
        };
        // The left side is the body's -Z.
        assert!(at(&mut app, &rest, "foot_l").z < -0.1);
        assert!(at(&mut app, &rest, "hand_r").z > 0.1);
        for (k, side) in Side::BOTH.into_iter().enumerate() {
            let (foot, calf, hand, lowerarm) = match side {
                Side::Left => ("foot_l", "calf_l", "hand_l", "lowerarm_l"),
                Side::Right => ("foot_r", "calf_r", "hand_r", "lowerarm_r"),
            };
            // A positive hip swings the foot forward and up.
            let mut a = rest;
            a.hip[k] = 0.4;
            let (still, swung) = (at(&mut app, &rest, foot), at(&mut app, &a, foot));
            assert!(swung.x > still.x + 0.25 && swung.y > still.y, "{side:?}");
            // A knee folds the shin back: the foot goes behind the knee.
            let mut a = rest;
            a.knee[k] = -0.8;
            let behind = at(&mut app, &a, foot).x - at(&mut app, &a, calf).x;
            let below = at(&mut app, &rest, foot).x - at(&mut app, &rest, calf).x;
            assert!(behind < below - 0.25, "{side:?} {behind} {below}");
            // A positive shoulder swings the hand forward; an elbow folds
            // the forearm forward.
            let mut a = rest;
            a.shoulder[k] = 0.4;
            assert!(at(&mut app, &a, hand).x > at(&mut app, &rest, hand).x + 0.15);
            let mut a = rest;
            a.elbow[k] = 0.8;
            let ahead = at(&mut app, &a, hand).x - at(&mut app, &a, lowerarm).x;
            let down = at(&mut app, &rest, hand).x - at(&mut app, &rest, lowerarm).x;
            assert!(ahead > down + 0.1, "{side:?}");
        }
        // The lean tips the head forward.
        let lean = JointAngles { lean: 0.2, ..rest };
        let head = at(&mut app, &rest, "head");
        assert!(at(&mut app, &lean, "head").x > head.x + 0.06);
        // The legs stay put while the torso leans.
        assert!(at(&mut app, &lean, "foot_l").distance(at(&mut app, &rest, "foot_l")) < 1e-4);
    }

    #[test]
    fn the_drawn_pose_is_worked_out_afresh_each_frame() {
        let (mut app, rig) = protagonist(AnimationState::Idle, 0.0);
        let chest = app
            .world()
            .get::<Rig>(rig)
            .unwrap()
            .bone(Bone::Spine02)
            .unwrap();
        let rest = app.world().get::<Rig>(rig).unwrap().rest[Bone::Spine02.index()];
        let mut turns = Vec::new();
        for _ in 0..160 {
            app.update();
            let world = app.world_mut();
            let rig = world.get::<Rig>(rig).unwrap().clone();
            let mut bodies = world.query::<&FigureBody>();
            let solved = rig.solve(&rig_pose(bodies.single(world).unwrap()));
            for bone in Bone::ALL {
                let drawn = world.get::<Transform>(rig.bone(bone).unwrap()).unwrap();
                assert!(drawn.rotation.angle_between(solved.local[bone.index()]) < 2e-3);
            }
            turns.push(
                world
                    .get::<Transform>(chest)
                    .unwrap()
                    .rotation
                    .angle_between(rest),
            );
        }
        // The chest breathes and sways a little, never far.
        assert!(turns.iter().all(|t| *t < 0.1), "{turns:?}");
        let (lo, hi) = turns
            .iter()
            .fold((f32::MAX, f32::MIN), |(l, h), t| (l.min(*t), h.max(*t)));
        assert!(hi - lo > 0.005, "{lo} {hi}");
    }

    #[test]
    fn the_protagonist_walks_with_his_legs_and_arms_in_opposition() {
        let (mut app, _) = protagonist(AnimationState::Walk, 1.4);
        let mut swings = Vec::new();
        for _ in 0..30 {
            app.update();
            let world = app.world_mut();
            // Knees and elbows: the feet and hands lag a little as the
            // knees and elbows fold.
            let [fl, fr, hl, hr] =
                ["calf_l", "calf_r", "lowerarm_l", "lowerarm_r"].map(|n| named(world, n));
            let x = |e| in_body(world, e).x;
            swings.push((x(fl) - x(fr), x(hl) - x(hr)));
        }
        // Over a cycle each knee leads by turns, and the elbow on its side
        // swings the other way.
        assert!(swings.iter().any(|s| s.0 > 0.2) && swings.iter().any(|s| s.0 < -0.2));
        let opposed = swings
            .iter()
            .filter(|s| s.0.abs() > 0.02)
            .all(|s| s.0 * s.1 < 0.0);
        assert!(opposed, "{swings:?}");
    }

    /// Each frame's ankles and balls of the feet, left and right, in the
    /// body frame plus the body's own rise, for `frames` of 50 ms; and the
    /// same at rest.
    fn feet(animation: AnimationState, speed: f32, frames: usize) -> (Vec<[Vec3; 4]>, [Vec3; 4]) {
        let (mut app, rig) = protagonist(animation, speed);
        let names = ["foot_l", "foot_r", "ball_l", "ball_r"];
        let measure = |world: &mut World| {
            let ends = names.map(|n| named(world, n));
            let mut bodies = world.query::<(&FigureBody, &Transform)>();
            let bob = bodies.single(world).unwrap().1.translation.y;
            ends.map(|e| in_body(world, e) + Vec3::Y * bob)
        };
        let mut out = Vec::new();
        for _ in 0..frames {
            app.update();
            out.push(measure(app.world_mut()));
        }
        pose_by(&mut app, rig, &RigPose::REST);
        let mut figure = app
            .world_mut()
            .query_filtered::<&mut Transform, With<FigureBody>>();
        figure.single_mut(app.world_mut()).unwrap().translation.y = 0.0;
        let rest = measure(app.world_mut());
        (out, rest)
    }

    #[test]
    fn walking_a_foot_stays_on_the_ground_and_keeps_still_on_it() {
        let speed = 6.0 / 3.6;
        let (frames, rest) = feet(AnimationState::Walk, speed, 60);
        let ground = rest[0].y;
        let mut slip = Vec::new();
        for pair in frames.windows(2) {
            let low = |f: &[Vec3; 4]| usize::from(f[1].y < f[0].y);
            let k = low(&pair[1]);
            // The lower foot is on the ground, the other never under it.
            assert!((pair[1][k].y - ground).abs() < 3e-3, "{pair:?}");
            assert!(pair[1][1 - k].y > ground - 3e-3);
            if low(&pair[0]) == k {
                slip.push((speed + (pair[1][k].x - pair[0][k].x) / 0.05).abs());
            }
        }
        let mean = slip.iter().sum::<f32>() / slip.len() as f32;
        assert!(mean < 0.1, "{mean} m/s {slip:?}");
    }

    #[test]
    fn jogging_the_feet_never_go_through_the_ground() {
        let (frames, rest) = feet(AnimationState::Gallop, 5.0, 40);
        let ground = rest[0].y;
        assert!(frames.iter().all(|f| f[0].y.min(f[1].y) > ground - 3e-3));
        assert!(
            frames.iter().any(|f| f[0].y.min(f[1].y) < ground + 3e-3),
            "it lands"
        );
        // A foot on the ground keeps nearly still on it.
        let mut slip = Vec::new();
        for pair in frames.windows(2) {
            for k in 0..2 {
                if pair.iter().all(|f| f[k].y < ground + 3e-3) {
                    slip.push((5.0 + (pair[1][k].x - pair[0][k].x) / 0.05).abs());
                }
            }
        }
        let mean = slip.iter().sum::<f32>() / slip.len().max(1) as f32;
        assert!(!slip.is_empty() && mean < 0.5, "{mean} m/s {slip:?}");
    }

    #[test]
    fn standing_the_supporting_foot_stays_put_and_the_free_one_touches_its_toe_down() {
        let (frames, rest) = feet(AnimationState::Idle, 0.0, 200);
        // The seed is the character entity's: the free foot is the one
        // that moved.
        let last = frames.last().unwrap();
        let moved = |k: usize| last[k].distance(rest[k]);
        let (support, free) = if moved(0) < moved(1) { (0, 1) } else { (1, 0) };
        let ground = rest[0].y;
        for f in &frames[4..] {
            assert!(f[support].distance(rest[support]) < 4e-3, "{f:?} {rest:?}");
            // The free foot rests on the ground: its ball, or its heel too.
            assert!(f[free].y > ground - 3e-3, "{f:?}");
            assert!(
                (f[2 + free].y - rest[2 + free].y).abs() < 6e-3,
                "{f:?} {rest:?}"
            );
        }
    }

    #[test]
    fn walking_the_feet_stay_level() {
        let (mut app, rig) = protagonist(AnimationState::Walk, 1.5);
        let rig = app.world().get::<Rig>(rig).unwrap().clone();
        for _ in 0..30 {
            app.update();
            for side in Side::BOTH {
                let foot = rig.bone(Bone::Foot(side)).unwrap();
                let i = Bone::Foot(side).index();
                let rest = rig.frame[i] * rig.rest[i];
                // Level, the toes only turned a little out.
                let out = Quat::from_rotation_y(-side.z() * relaxed::TOE_OUT);
                let turn = turn_in_body(app.world(), foot);
                assert!(turn.angle_between(out * rest) < 2e-3, "{side:?}");
            }
        }
    }

    #[test]
    fn the_fingers_curl_toward_the_palm() {
        use bevy::gltf::{Gltf, GltfNode};
        let (mut app, _) = protagonist(AnimationState::Idle, 0.0);
        // The fingers as the file has them, straight.
        let gltf: Handle<Gltf> = app
            .world()
            .resource::<AssetServer>()
            .load(super::super::model::model_path("protagonist"));
        while !app.world().resource::<Assets<Gltf>>().contains(&gltf) {
            app.update();
        }
        let world = app.world_mut();
        let file = |world: &World, name: &str| {
            let gltf = world.resource::<Assets<Gltf>>().get(&gltf).unwrap();
            let node = &gltf.named_nodes[name];
            world
                .resource::<Assets<GltfNode>>()
                .get(node)
                .unwrap()
                .transform
                .rotation
        };
        for (side, z) in [("l", 1.0), ("r", -1.0)] {
            let tip = named(world, &format!("index_03_{side}"));
            let curled = in_body(world, tip);
            for joint in ["01", "02", "03"] {
                let name = format!("index_{joint}_{side}");
                let straight = file(world, &name);
                let e = named(world, &name);
                world.get_mut::<Transform>(e).unwrap().rotation = straight;
            }
            let straight = in_body(world, tip);
            // The left palm faces +Z (the body), the right one -Z.
            assert!(
                (curled.z - straight.z) * z > 0.005,
                "{side} {curled} {straight}"
            );
        }
    }

    #[test]
    fn a_raised_free_foot_points_its_toe_to_the_ground() {
        let (mut app, rig) = protagonist(AnimationState::Idle, 0.0);
        let ball = |app: &mut App, pose: &RigPose| {
            pose_by(app, rig, pose);
            let ball = named(app.world_mut(), "ball_l");
            in_body(app.world(), ball).y
        };
        let ground = ball(&mut app, &RigPose::REST);
        let mut raised = RigPose {
            plant: true,
            support: Some((Side::Right, 1.0)),
            ..RigPose::REST
        };
        raised.set(Bone::Thigh(Side::Left), Quat::from_rotation_z(0.2));
        raised.set(Bone::Calf(Side::Left), Quat::from_rotation_z(-0.5));
        let flat = ball(&mut app, &raised);
        assert!(flat > ground + 0.02, "{flat} {ground}");
        raised.touch = 1.0;
        let pointed = ball(&mut app, &raised);
        assert!((pointed - ground).abs() < 0.01, "{pointed} {ground}");
    }

    #[test]
    fn a_model_far_off_or_out_of_view_keeps_its_pose_but_the_protagonists_never_does() {
        let (mut app, rigs) = rigged(
            &[FigureKind::Hidalgo, FigureKind::Protagonist],
            AnimationState::Idle,
            0.0,
        );
        let camera = app
            .world_mut()
            .spawn((Camera3d::default(), Transform::from_xyz(0.0, 1.6, 4.0)))
            .id();
        // Headless, nothing computes visibility: every model is in view.
        for &(_, rig) in &rigs {
            let probe = app.world().get::<Rig>(rig).unwrap().probe.unwrap();
            app.world_mut()
                .entity_mut(probe)
                .insert(ViewVisibility::VISIBLE);
        }
        let chest = |app: &App, rig: Entity| {
            let bone = app
                .world()
                .get::<Rig>(rig)
                .unwrap()
                .bone(Bone::Spine02)
                .unwrap();
            app.world().get::<Transform>(bone).unwrap().rotation
        };
        // Breathing moves the chest a little every frame, unless frozen.
        let breathes = |app: &mut App, rig: Entity| {
            let before = chest(app, rig);
            app.update();
            app.update();
            chest(app, rig) != before
        };
        let frozen = |app: &App, rig: Entity| app.world().get::<Rig>(rig).unwrap().frozen;
        // The camera's place reaches the driver a frame late.
        let settle = |app: &mut App| app.update();
        for &(_, rig) in &rigs {
            assert!(breathes(&mut app, rig) && !frozen(&app, rig));
        }
        // The camera far off: Don Hidalgo stops; the clerk carries on.
        app.world_mut()
            .get_mut::<Transform>(camera)
            .unwrap()
            .translation
            .z = 500.0;
        settle(&mut app);
        for &(kind, rig) in &rigs {
            let player = kind == FigureKind::Protagonist;
            assert_eq!(breathes(&mut app, rig), player, "{kind:?}");
            assert_eq!(frozen(&app, rig), !player, "{kind:?}");
        }
        // Near again, but out of view: the same.
        app.world_mut()
            .get_mut::<Transform>(camera)
            .unwrap()
            .translation
            .z = 4.0;
        for &(_, rig) in &rigs {
            let probe = app.world().get::<Rig>(rig).unwrap().probe.unwrap();
            app.world_mut()
                .entity_mut(probe)
                .insert(ViewVisibility::HIDDEN);
        }
        for &(kind, rig) in &rigs {
            let player = kind == FigureKind::Protagonist;
            assert_eq!(breathes(&mut app, rig), player, "{kind:?}");
        }
    }
}
