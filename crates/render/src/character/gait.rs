//! The saddled horse's clips, fitted after the animation systems (see
//! [`horse`](super::horse)): whatever the clips leave undone or too short.
//!
//! - **Blending** ([`complete_blend`]). Bevy weighs each bone's property
//!   only among the clips that animate it: a property that only the walk
//!   animates (the back's sway, the hooves' IK targets) would keep the walk
//!   at full strength while the walk fades out, folding the legs. Each such
//!   property is eased back toward its rest by the weight of the clips that
//!   do not animate it ([`PARTIAL`]).
//! - **A longer gallop.** Quaternius' gallop has a short stride (about 2.3
//!   m a loop): played fast enough not to slide at a full gallop it reads
//!   frantic. At the gallop each leg's swing about its own mean
//!   ([`GALLOP_MEANS`]) is stretched by [`STRIDE`], so the clip covers more
//!   ground a loop and plays slower.
//! - **A flexing body** ([`articulation`]), phase-locked to the gallop's
//!   loop: the croup drops as the hind legs reach under the belly and rises
//!   at the push-off, the back rounds and extends against it, so the
//!   withers pitch the other way, the neck pumps against the forehand and
//!   the head steadies, the hips roll and yaw a little with the lead, and
//!   the tail follows late. The walk is left as it is.
//! - **The hooves stay on the legs, above the ground.** A leg whose end a
//!   blend or the stretch puts below the ground is folded at the knee or
//!   hock just enough ([`lift_legs`]). The model's hooves hang from IK
//!   targets, not from the legs, so each hoof is then put on its leg's end
//!   ([`reattach`]), never below where it would stand.
//! - **A jump in the air.** While the jump clip plays ([`level_flight`]) the
//!   gallop's flexing is muted by its weight and the hooves are left as the
//!   clip tucks them ([`JUMP_FREE`]); the clip's own rise of the `Body`
//!   bone (and of the hooves' IK targets with it) is taken out, since the
//!   entity already flies the physics arc. On top of the clip the hind legs
//!   push the horse off, reaching back and down as it leaves the ground
//!   ([`push`], [`PUSH`]), and the forelegs tuck a little more over the top
//!   ([`tuck`], [`TUCK`]), each hoof put back on its leg's end.
//!
//! All of it is deterministic, worked out afresh each frame from the clips'
//! pose, and allocates nothing.

use bevy::math::Affine3A;
use bevy::prelude::*;

use crate::fauna::livestock::{CLIP_COUNT, Clip};

/// Which part of a bone's transform a clip animates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Property {
    Rotation,
    Translation,
}

/// The properties not every clip animates, with the clips that do (bit
/// `k` for [`Clip::ALL`]`[k]`), as `horse.glb` has them; every other
/// animated property is in every clip.
pub const PARTIAL: [(&str, Property, u8); 26] = [
    ("Back", Property::Rotation, 0b0010000),
    ("Back", Property::Translation, 0b1110000),
    ("BackShoulder.L", Property::Translation, 0b1100010),
    ("BackShoulder.R", Property::Translation, 0b1100000),
    ("Ear2.L", Property::Rotation, 0b1101111),
    ("Ear2.R", Property::Rotation, 0b1101111),
    ("Ear3.L", Property::Rotation, 0b1100100),
    ("Ear3.R", Property::Rotation, 0b1100100),
    ("Ear4.L", Property::Rotation, 0b1110000),
    ("Ear4.R", Property::Rotation, 0b1110000),
    ("FrontShoulder.L", Property::Translation, 0b1111101),
    ("FrontShoulder.R", Property::Translation, 0b1110000),
    ("IKBackLeg.L", Property::Rotation, 0b1110000),
    ("IKBackLeg.L", Property::Translation, 0b1110010),
    ("IKBackLeg.R", Property::Rotation, 0b1110100),
    ("IKBackLeg.R", Property::Translation, 0b1110100),
    ("IKFrontLeg.L", Property::Rotation, 0b1110100),
    ("IKFrontLeg.L", Property::Translation, 0b1111101),
    ("IKFrontLeg.R", Property::Rotation, 0b1110000),
    ("IKFrontLeg.R", Property::Translation, 0b1110000),
    ("Neck3", Property::Rotation, 0b1110011),
    ("PoleTarget.L", Property::Translation, 0b1000000),
    ("PoleTarget.R", Property::Translation, 0b1000000),
    ("Tail2", Property::Rotation, 0b1100000),
    ("Torso2", Property::Rotation, 0b0010000),
    ("Torso3", Property::Rotation, 0b0001000),
];

/// The bones no clip turns that [`fit_horse`] turns: they are put back at
/// rest first each frame, or the turns would add up.
pub const UNANIMATED: [&str; 2] = ["Torso", "Tail1"];

/// The share of a property `mask` (see [`PARTIAL`]) the clips playing with
/// `weights` animate: the rest of it stays at rest.
pub fn animated_share(mask: u8, weights: &[f32; CLIP_COUNT]) -> f32 {
    Clip::ALL
        .iter()
        .filter(|c| mask & (1 << c.index()) != 0)
        .map(|c| weights[c.index()])
        .sum::<f32>()
        .clamp(0.0, 1.0)
}

/// `value` (what the clips set) eased toward `rest` by the share the clips
/// that set it do not have.
pub fn complete_blend(property: Property, rest: &Transform, value: &mut Transform, share: f32) {
    match property {
        Property::Rotation => value.rotation = rest.rotation.slerp(value.rotation, share),
        Property::Translation => {
            value.translation = rest.translation.lerp(value.translation, share);
        }
    }
}

/// How much longer the gallop's stride is than the clip's.
pub const STRIDE: f32 = 1.4;

/// Each swinging upper leg bone's mean turn over the gallop's loop (its
/// own local rotation, measured on `horse.glb`): the stride is stretched
/// about it. The lower legs fold as the clip folds them.
pub const GALLOP_MEANS: [(&str, [f32; 4]); 8] = [
    ("FrontUpperLeg.L", [0.72688, 0.63446, 0.17364, 0.1974]),
    ("FrontUpperLeg.R", [0.72373, -0.63646, -0.17442, 0.20176]),
    ("BackShoulder.L", [-0.68568, -0.56806, -0.27013, 0.3663]),
    ("BackShoulder.R", [0.69222, -0.5585, -0.25075, -0.38215]),
    ("BackLeg.L", [0.73969, 0.63967, 0.12762, 0.16553]),
    ("BackLeg.R", [0.73885, -0.64136, -0.12789, 0.16247]),
    ("BackUpperLeg.L", [0.65989, -0.00073, 0.04062, 0.75027]),
    ("BackUpperLeg.R", [0.64587, 0.00303, -0.0341, 0.76268]),
];

/// `rotation` swung `k` times as far from `mean`.
pub fn stretch(rotation: Quat, mean: Quat, k: f32) -> Quat {
    let mut delta = mean.inverse() * rotation;
    if delta.w < 0.0 {
        delta = -delta;
    }
    let (axis, angle) = delta.to_axis_angle();
    (mean * Quat::from_axis_angle(axis, angle * k)).normalize()
}

/// Where in the gallop's loop (`0..1`) the hind legs reach furthest under
/// the belly and the left hind strikes: the croup is lowest there.
pub const GATHER: f32 = 0.57;

/// The body's own motion at the gallop, radians, at `phase` of its loop
/// (`0..1`): pitches about the body's side axis (nose or front end up
/// positive) of the hindquarters (`Back`), each back bone, the neck and the
/// head, the hips' roll and yaw, and the tail's lift.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Articulation {
    pub back: f32,
    pub torso: [f32; 3],
    pub neck: f32,
    pub head: f32,
    pub roll: f32,
    pub yaw: f32,
    pub tail: f32,
}

/// The croup's pitch either way at the gallop, radians (about 7°).
pub const CROUP: f32 = 0.12;

/// The [`Articulation`] at `phase` of the gallop. The hindquarters pitch
/// most; the back bones bend against them, a little more each toward the
/// withers, so the forehand pitches the other way; the neck pumps against
/// the forehand and the head undoes half of it; the tail lags a fifth of a
/// loop.
pub fn articulation(phase: f32) -> Articulation {
    use std::f32::consts::TAU;
    let gather = (TAU * (phase - GATHER)).cos();
    let lead = (TAU * (phase - GATHER)).sin();
    let back = CROUP * gather;
    let neck = 0.6 * CROUP * gather;
    Articulation {
        back,
        torso: [-0.7 * back, -0.5 * back, -0.3 * back],
        neck,
        head: -0.5 * neck,
        roll: 0.04 * lead,
        yaw: 0.03 * lead,
        tail: 0.25 * (TAU * (phase - GATHER - 0.2)).cos(),
    }
}

/// The horse's bones this module moves, by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bone {
    Armature,
    Body,
    Back,
    Torso,
    Torso2,
    Torso3,
    Neck1,
    Neck2,
    Neck3,
    Head,
    Tail1,
    /// Upper bone of each leg: the front shoulder or the hind hip.
    Shoulder(usize),
    UpperLeg(usize),
    /// The hind leg's extra bone between the hip and the thigh.
    Leg(usize),
    LowerLeg(usize),
    End(usize),
    Hoof(usize),
}

/// The legs, in [`Bone`]'s leg index order.
pub const LEGS: [&str; 4] = ["Front?.L", "Front?.R", "Back?.L", "Back?.R"];

impl Bone {
    pub const COUNT: usize = 11 + 6 * 4;

    pub fn index(self) -> usize {
        match self {
            Self::Armature => 0,
            Self::Body => 1,
            Self::Back => 2,
            Self::Torso => 3,
            Self::Torso2 => 4,
            Self::Torso3 => 5,
            Self::Neck1 => 6,
            Self::Neck2 => 7,
            Self::Neck3 => 8,
            Self::Head => 9,
            Self::Tail1 => 10,
            Self::Shoulder(l) => 11 + l,
            Self::UpperLeg(l) => 15 + l,
            Self::Leg(l) => 19 + l,
            Self::LowerLeg(l) => 23 + l,
            Self::End(l) => 27 + l,
            Self::Hoof(l) => 31 + l,
        }
    }

    /// Its node's name in `horse.glb`.
    pub fn name(self) -> String {
        let leg = |l: usize, part: &str| LEGS[l].replace('?', part);
        match self {
            Self::Armature => "AnimalArmature".into(),
            Self::Body => "Body".into(),
            Self::Back => "Back".into(),
            Self::Torso => "Torso".into(),
            Self::Torso2 => "Torso2".into(),
            Self::Torso3 => "Torso3".into(),
            Self::Neck1 => "Neck1".into(),
            Self::Neck2 => "Neck2".into(),
            Self::Neck3 => "Neck3".into(),
            Self::Head => "Head".into(),
            Self::Tail1 => "Tail1".into(),
            Self::Shoulder(l) => leg(l, "Shoulder"),
            Self::UpperLeg(l) if l < 2 => leg(l, "UpperLeg"),
            Self::UpperLeg(l) => leg(l, "UpperLeg"),
            // The front legs have no such bone: their shoulder stands in.
            Self::Leg(l) if l < 2 => leg(l, "Shoulder"),
            Self::Leg(l) => leg(l, "Leg"),
            Self::LowerLeg(l) => leg(l, "LowerLeg"),
            Self::End(l) => format!("{}_end", leg(l, "LowerLeg")),
            Self::Hoof(l) if l < 2 => format!("IKFrontLeg.{}", ["L", "R"][l]),
            Self::Hoof(l) => format!("IKBackLeg.{}", ["L", "R"][l - 2]),
        }
    }

    /// Every bone, by [`Self::index`].
    pub fn all() -> [Self; Self::COUNT] {
        let mut out = [Self::Armature; Self::COUNT];
        let fixed = [
            Self::Armature,
            Self::Body,
            Self::Back,
            Self::Torso,
            Self::Torso2,
            Self::Torso3,
            Self::Neck1,
            Self::Neck2,
            Self::Neck3,
            Self::Head,
            Self::Tail1,
        ];
        for b in fixed {
            out[b.index()] = b;
        }
        for l in 0..4 {
            for b in [
                Self::Shoulder(l),
                Self::UpperLeg(l),
                Self::Leg(l),
                Self::LowerLeg(l),
                Self::End(l),
                Self::Hoof(l),
            ] {
                out[b.index()] = b;
            }
        }
        out
    }

    /// The leg's chain from the body down to its end (the front legs hang
    /// from the withers, the hind ones from the hindquarters).
    fn chain(leg: usize) -> [Self; 8] {
        if leg < 2 {
            [
                Self::Body,
                Self::Back,
                Self::Torso,
                Self::Torso2,
                Self::Shoulder(leg),
                Self::UpperLeg(leg),
                Self::LowerLeg(leg),
                Self::End(leg),
            ]
        } else {
            [
                Self::Body,
                Self::Back,
                Self::Shoulder(leg),
                Self::Leg(leg),
                Self::UpperLeg(leg),
                Self::LowerLeg(leg),
                Self::End(leg),
                Self::End(leg),
            ]
        }
    }
}

/// A horse model's bones and rests, found when its scene spawns; on its
/// root next to its [`HorseRig`](super::horse::HorseRig).
#[derive(Component, Debug, Clone)]
pub struct HorseBones {
    /// By [`Bone::index`].
    pub bones: [Entity; Bone::COUNT],
    /// The armature's placement in the body frame (the model's root and
    /// the scene's root node above it).
    pub armature: Affine3A,
    /// Each partly animated property's bone, its rest and mask (the
    /// [`UNANIMATED`] bones' turns too, with no clip in theirs).
    pub partial: Vec<(Entity, Property, u8, Transform)>,
    /// Each stretched leg bone, its leg (by [`LEGS`]) and its mean turn
    /// at the gallop.
    pub means: Vec<(usize, Entity, Quat)>,
    /// How high each hoof's target stands at rest in the body frame.
    pub standing: [f32; 4],
    /// The `Body` bone's rest translation (in the armature's space): the
    /// jump clip's rise is measured from it.
    pub body_rest: Vec3,
}

/// The placement of `bone` in the armature's space, from `transforms`.
fn placed(bones: &HorseBones, chain: &[Bone], transforms: &Query<&mut Transform>) -> Affine3A {
    let mut at = Affine3A::IDENTITY;
    let mut last = None;
    for &b in chain {
        if last == Some(b) {
            continue;
        }
        last = Some(b);
        if let Ok(t) = transforms.get(bones.bones[b.index()]) {
            at *= t.compute_affine();
        }
    }
    at
}

/// The rotation part of a placement with uniform scale.
fn turn(a: &Affine3A) -> Quat {
    let (_, rotation, _) = a.to_scale_rotation_translation();
    rotation
}

/// Turns `bone` by `delta`, a turn in the body frame (taken into the
/// armature's space by `armature`), about itself.
fn turn_in_body(
    bones: &HorseBones,
    transforms: &mut Query<&mut Transform>,
    parent: &[Bone],
    bone: Bone,
    delta: Quat,
) {
    let frame = turn(&bones.armature).inverse() * delta * turn(&bones.armature);
    let p = turn(&placed(bones, parent, transforms));
    if let Ok(mut t) = transforms.get_mut(bones.bones[bone.index()]) {
        t.rotation = (p.inverse() * frame * p * t.rotation).normalize();
    }
}

/// How far below where it stands a leg's end may reach before its stride
/// is stretched less, meters.
pub const PLANTED: f32 = 0.015;

/// Folds each leg whose end the blend or the stretch has put below the
/// ground at its lower joint (the knee or the hock), just enough to bring
/// it back up: two clips half and half can reach further down than
/// either.
fn lift_legs(bones: &HorseBones, transforms: &mut Query<&mut Transform>) {
    for leg in 0..4 {
        let chain = Bone::chain(leg);
        let floor = bones.standing[leg] - PLANTED;
        let height = |transforms: &Query<&mut Transform>| {
            let end = placed(bones, &chain, transforms);
            bones
                .armature
                .transform_point3(Vec3::from(end.translation))
                .y
        };
        let low = height(transforms);
        if low >= floor {
            continue;
        }
        let lower = bones.bones[Bone::LowerLeg(leg).index()];
        let Ok(start) = transforms.get(lower).map(|t| t.rotation) else {
            continue;
        };
        // The fold about the body's side axis, in the lower bone's parent's
        // space (its rotation is applied there): the chain down to it.
        let Some(at) = chain.iter().position(|b| *b == Bone::LowerLeg(leg)) else {
            continue;
        };
        let parent = turn(&placed(bones, &chain[..at], transforms));
        let side = parent.inverse() * turn(&bones.armature).inverse() * Vec3::Z;
        let mut best = (low, start);
        for step in 1..=12 {
            for sign in [1.0, -1.0] {
                let turned =
                    (Quat::from_axis_angle(side, sign * 0.06 * step as f32) * start).normalize();
                if let Ok(mut t) = transforms.get_mut(lower) {
                    t.rotation = turned;
                }
                let h = height(transforms);
                if h > best.0 {
                    best = (h, turned);
                }
            }
            if best.0 >= floor {
                break;
            }
        }
        if let Ok(mut t) = transforms.get_mut(lower) {
            t.rotation = best.1;
        }
    }
}

/// Puts each hoof's IK target on its leg's end, turned as far as the
/// lower leg turned since `before` (the lower legs' turns as the clips left
/// them), and never below where it would stand (or the clip's own place,
/// if the clip has it lower). Blending clips weighs the legs' turns and the
/// hoofs' targets apart: without this a hoof drifts off its leg between
/// two clips.
fn reattach(bones: &HorseBones, transforms: &mut Query<&mut Transform>, before: &[Quat; 4]) {
    for (leg, was_turned) in before.iter().enumerate() {
        let chain = Bone::chain(leg);
        let end = placed(bones, &chain, transforms);
        let lower = turn(&placed(bones, &chain[..chain.len() - 1], transforms));
        let entity = bones.bones[Bone::Hoof(leg).index()];
        let Ok(mut hoof) = transforms.get_mut(entity) else {
            continue;
        };
        let spin = lower * was_turned.inverse();
        let to_body = bones.armature;
        let was = to_body.transform_point3(hoof.translation).y;
        let floor = was.min(bones.standing[leg]);
        let mut body = to_body.transform_point3(Vec3::from(end.translation));
        body.y = body.y.max(floor);
        hoof.translation = to_body.inverse().transform_point3(body);
        hoof.rotation = (spin * hoof.rotation).normalize();
    }
}

/// The share of the jump clip's weight from which the hooves are no longer
/// clamped to the ground (see [`fit_horse`]).
pub const JUMP_FREE: f32 = 0.05;

/// Takes the jump clip's own rise out of the horse: `Body` carries the clip's
/// crouch and leap, but the horse's height in the air is the physics arc
/// (the game lifts the whole entity by it), so the clip's rise would add
/// to it. The bone's vertical offset from rest (in the body frame) is
/// removed, `jump` (the clip's weight) of it, after the clips have set it
/// and before anything hangs off it; the tuck and pitch stay.
///
/// The hooves' IK targets are the body's siblings under the armature, not
/// its children, and the clip lifts them with it: they are lowered by the
/// same offset, or the hooves would stay up where the clip put them while
/// the legs come down, off their legs.
///
/// Reading the bone after the animation (rather than sampling the curve)
/// follows whatever the blend really produced, in the model's own scale.
fn level_flight(bones: &HorseBones, transforms: &mut Query<&mut Transform>, jump: f32) {
    let Ok(body) = transforms.get(bones.bones[Bone::Body.index()]) else {
        return;
    };
    let rise = bones
        .armature
        .transform_vector3(body.translation - bones.body_rest)
        .y;
    let drop = bones
        .armature
        .inverse()
        .transform_vector3(Vec3::Y * rise * jump);
    let hooves = (0..4).map(|leg| bones.bones[Bone::Hoof(leg).index()]);
    for entity in std::iter::once(bones.bones[Bone::Body.index()]).chain(hooves) {
        if let Ok(mut t) = transforms.get_mut(entity) {
            t.translation -= drop;
        }
    }
}

/// The hind legs' push at takeoff, radians: each thigh swung back at the
/// hip and each hock opened, so the hooves reach back and down as they
/// leave the ground (the clip has the push, but too short to read).
pub const PUSH: (f32, f32) = (0.3, 0.45);

/// The flight (`0..=1`) by which the push is gone: the clip folds the hind
/// legs up from there.
pub const PUSH_END: f32 = 0.25;

/// The forelegs' extra tuck at the top of the jump, radians: each forearm
/// lifted forward at the elbow and each knee folded, a little more than
/// the clip does.
pub const TUCK: (f32, f32) = (0.15, 0.4);

/// How much of the hind legs' push (`0..1`) there is `flight` (`0..=1`)
/// of the way through a jump: all of it as the horse leaves the ground,
/// easing out by [`PUSH_END`].
pub fn push(flight: f32) -> f32 {
    let u = (flight / PUSH_END).clamp(0.0, 1.0);
    1.0 - u * u * (3.0 - 2.0 * u)
}

/// How much of the forelegs' extra tuck (`0..1`) there is `flight` of the
/// way through a jump: none at either end, all of it at the top, smooth
/// throughout.
pub fn tuck(flight: f32) -> f32 {
    (std::f32::consts::PI * flight.clamp(0.0, 1.0))
        .sin()
        .powi(2)
}

/// Adds the hind legs' push and the forelegs' tuck ([`PUSH`], [`TUCK`])
/// to the jump clip's pose, `flight` of the way through the jump and
/// weighed by `jump` (the clip's weight), then puts each hoof back on its
/// leg's end ([`reattach`]: never below where it stands, so the push does
/// not sink the hooves at takeoff).
fn push_and_tuck(
    bones: &HorseBones,
    transforms: &mut Query<&mut Transform>,
    flight: f32,
    jump: f32,
    before: &[Quat; 4],
) {
    let (push, tuck) = (push(flight) * jump, tuck(flight) * jump);
    if push < 1e-4 && tuck < 1e-4 {
        return;
    }
    // About the body's side axis: positive swings a hanging leg forward.
    let pitch = Quat::from_rotation_z;
    for leg in 0..4 {
        let chain = Bone::chain(leg);
        let parent = |bone: Bone| {
            let at = chain.iter().position(|b| *b == bone).unwrap_or(0);
            &chain[..at]
        };
        if leg < 2 {
            let (forearm, knee) = (Bone::UpperLeg(leg), Bone::LowerLeg(leg));
            turn_in_body(
                bones,
                transforms,
                parent(forearm),
                forearm,
                pitch(TUCK.0 * tuck),
            );
            turn_in_body(bones, transforms, parent(knee), knee, pitch(-TUCK.1 * tuck));
        } else {
            let (thigh, hock) = (Bone::Leg(leg), Bone::LowerLeg(leg));
            turn_in_body(
                bones,
                transforms,
                parent(thigh),
                thigh,
                pitch(-PUSH.0 * push),
            );
            turn_in_body(bones, transforms, parent(hock), hock, pitch(-PUSH.1 * push));
        }
    }
    reattach(bones, transforms, before);
}

/// Fits one horse's pose for `weights` (in [`Clip::ALL`] order) at
/// `phase` of the gallop's loop, `flight` of the way through a jump (none
/// on the ground): see the module notes.
pub fn fit_horse(
    bones: &HorseBones,
    transforms: &mut Query<&mut Transform>,
    weights: &[f32; CLIP_COUNT],
    phase: f32,
    flight: Option<f32>,
) {
    for (entity, property, mask, rest) in &bones.partial {
        if let Ok(mut t) = transforms.get_mut(*entity) {
            complete_blend(*property, rest, &mut t, animated_share(*mask, weights));
        }
    }
    let jump = weights[Clip::GallopJump.index()];
    if jump > 0.0 {
        level_flight(bones, transforms, jump);
    }
    let before: [Quat; 4] = std::array::from_fn(|leg| {
        let chain = Bone::chain(leg);
        turn(&placed(bones, &chain[..chain.len() - 1], transforms))
    });
    // The jump's own tuck and pitch replace the gallop's: its legs are meant
    // to leave the ground, so they are not clamped back to it either.
    if jump >= JUMP_FREE {
        if let Some(flight) = flight {
            push_and_tuck(bones, transforms, flight, jump, &before);
        }
        return;
    }
    let gallop = weights[Clip::Gallop.index()] * (1.0 - jump);
    if gallop < 1e-3 {
        lift_legs(bones, transforms);
        reattach(bones, transforms, &before);
        return;
    }
    let a = articulation(phase);
    let pitch = |angle: f32| Quat::from_rotation_z(angle * gallop);
    use Bone as B;
    let spine = [
        B::Body,
        B::Back,
        B::Torso,
        B::Torso2,
        B::Torso3,
        B::Neck1,
        B::Neck2,
        B::Neck3,
    ];
    let hips = Quat::from_rotation_y(a.yaw * gallop)
        * Quat::from_rotation_x(a.roll * gallop)
        * pitch(a.back);
    turn_in_body(bones, transforms, &spine[..1], B::Back, hips);
    for (k, bone) in [B::Torso, B::Torso2, B::Torso3].into_iter().enumerate() {
        turn_in_body(bones, transforms, &spine[..2 + k], bone, pitch(a.torso[k]));
    }
    turn_in_body(bones, transforms, &spine[..5], B::Neck1, pitch(a.neck));
    turn_in_body(bones, transforms, &spine, B::Head, pitch(a.head));
    turn_in_body(bones, transforms, &spine[..2], B::Tail1, pitch(-a.tail));
    // Each leg's swing stretched, as far as its end stays above the ground.
    let k = 1.0 + (STRIDE - 1.0) * gallop;
    for leg in 0..4 {
        let mut clip = [(Entity::PLACEHOLDER, Quat::IDENTITY, Quat::IDENTITY); 3];
        let mut n = 0;
        for (l, entity, mean) in &bones.means {
            if *l == leg
                && n < clip.len()
                && let Ok(t) = transforms.get(*entity)
            {
                clip[n] = (*entity, t.rotation, *mean);
                n += 1;
            }
        }
        let floor = bones.standing[leg] - PLANTED;
        for share in [1.0, 0.75, 0.5, 0.25, 0.0] {
            let kk = 1.0 + (k - 1.0) * share;
            for (entity, rotation, mean) in &clip[..n] {
                if let Ok(mut t) = transforms.get_mut(*entity) {
                    t.rotation = stretch(*rotation, *mean, kk);
                }
            }
            let end = placed(bones, &Bone::chain(leg), transforms);
            if bones
                .armature
                .transform_point3(Vec3::from(end.translation))
                .y
                >= floor
            {
                break;
            }
        }
    }
    lift_legs(bones, transforms);
    reattach(bones, transforms, &before);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_push_is_whole_at_takeoff_and_gone_by_its_end_and_the_tuck_peaks_mid_flight() {
        assert_eq!(push(0.0), 1.0);
        assert!(push(0.1) > 0.3 && push(0.1) < 1.0);
        for f in [PUSH_END, 0.5, 1.0] {
            assert_eq!(push(f), 0.0, "{f}");
        }
        assert!(tuck(0.0).abs() < 1e-6 && tuck(1.0).abs() < 1e-6);
        assert!((tuck(0.5) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_property_the_fading_clips_do_not_animate_eases_back_to_rest() {
        // Only the walk and the gallop sway the back.
        let mask = 0b110000;
        let walking = [0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        assert_eq!(animated_share(mask, &walking), 1.0);
        let halfway = [0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0];
        assert!((animated_share(mask, &halfway) - 0.5).abs() < 1e-6);
        let rest = Transform::from_xyz(0.0, 1.0, 0.0);
        let mut walked =
            Transform::from_xyz(0.0, 1.2, 0.0).with_rotation(Quat::from_rotation_x(0.6));
        complete_blend(Property::Translation, &rest, &mut walked, 0.5);
        complete_blend(Property::Rotation, &rest, &mut walked, 0.5);
        assert!((walked.translation.y - 1.1).abs() < 1e-6);
        assert!((walked.rotation.angle_between(Quat::IDENTITY) - 0.3).abs() < 1e-5);
        // Standing: wholly at rest.
        complete_blend(
            Property::Rotation,
            &rest,
            &mut walked,
            animated_share(mask, &[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        );
        assert!(walked.rotation.angle_between(Quat::IDENTITY) < 1e-6);
    }

    #[test]
    fn the_partial_table_is_the_files() {
        use crate::gltf::tests::root;
        let data = std::fs::read(root().0.join("models/fauna/quaternius/horse.glb")).unwrap();
        let len = u32::from_le_bytes(data[12..16].try_into().unwrap()) as usize;
        let json = String::from_utf8(data[20..20 + len].to_vec()).unwrap();
        // Node names in file order, and each animation's (node, path)s.
        let names: Vec<&str> = {
            // The top-level node list is the file's last key.
            let nodes = &json[json.rfind("\"nodes\":[").unwrap()..];
            nodes
                .match_indices("\"name\":\"")
                .map(|(i, m)| {
                    let s = &nodes[i + m.len()..];
                    &s[..s.find('"').unwrap()]
                })
                .collect()
        };
        let animations = &json[json.find("\"animations\":[").unwrap()..];
        let mut found: std::collections::BTreeMap<(String, &str), u8> = Default::default();
        for (k, clip) in Clip::ALL.iter().enumerate() {
            let start = animations
                .find(&format!("{{\"name\":\"{}\",\"channels\":[", clip.name()))
                .unwrap();
            let end = animations[start..].find("],\"samplers\"").unwrap() + start;
            let channels = &animations[start..end];
            for (i, _) in channels.match_indices("\"node\":") {
                let rest = &channels[i + 7..];
                let node: usize = rest[..rest.find(|c: char| !c.is_ascii_digit()).unwrap()]
                    .parse()
                    .unwrap();
                let path = &rest[..rest.find('}').unwrap()];
                let path = if path.contains("rotation") {
                    "rotation"
                } else if path.contains("translation") {
                    "translation"
                } else {
                    continue;
                };
                *found.entry((names[node].to_string(), path)).or_default() |= 1 << k;
            }
        }
        let partial: Vec<(String, &str, u8)> = found
            .into_iter()
            .filter(|(_, m)| *m != (1 << Clip::ALL.len()) - 1)
            .map(|((n, p), m)| (n, p, m))
            .collect();
        let table: Vec<(String, &str, u8)> = PARTIAL
            .iter()
            .map(|(n, p, m)| {
                let p = if *p == Property::Rotation {
                    "rotation"
                } else {
                    "translation"
                };
                (n.to_string(), p, *m)
            })
            .collect();
        assert_eq!(partial, table);
    }

    #[test]
    fn a_stretched_swing_reaches_further_about_its_mean() {
        let mean = Quat::from_euler(EulerRot::XYZ, 0.3, -0.2, 0.9);
        let swung = mean * Quat::from_rotation_z(0.4);
        let stretched = stretch(swung, mean, 1.5);
        assert!((stretched.angle_between(mean) - 0.6).abs() < 1e-4);
        assert!(
            (mean.inverse() * stretched)
                .to_axis_angle()
                .0
                .distance(Vec3::Z)
                < 1e-4
        );
        assert!(stretch(mean, mean, 1.5).angle_between(mean) < 1e-4);
        assert!(stretch(swung, mean, 1.0).angle_between(swung) < 1e-4);
    }

    /// Raises the ground 12 cm under `legs` of a standing saddled horse, so
    /// [`lift_legs`] must fold them, and checks each folds at its lower
    /// joint (the knee or the hock) about the body's side axis: its end
    /// comes up, the joint stays, and the end keeps its side offset.
    fn folds_in_the_sagittal_plane(legs: [usize; 2]) {
        use crate::character::horse::tests::saddled;
        use crate::character::{CharacterVisual, Coat};
        use bevy::ecs::system::RunSystemOnce;
        let (mut app, _) = saddled(CharacterVisual::Horse(Coat::Bayo), 0.0);
        let world = app.world_mut();
        let mut bones = world.query::<&HorseBones>().single(world).unwrap().clone();
        // Body-frame places of each leg's lower joint and end.
        let at = |world: &mut World, bones: &HorseBones, leg: usize| {
            let bones = bones.clone();
            world
                .run_system_once(move |transforms: Query<&mut Transform>| {
                    let chain = Bone::chain(leg);
                    let joint = chain
                        .iter()
                        .position(|b| *b == Bone::LowerLeg(leg))
                        .unwrap();
                    let body = |c: &[Bone]| {
                        let p = placed(&bones, c, &transforms);
                        bones.armature.transform_point3(Vec3::from(p.translation))
                    };
                    (body(&chain[..=joint]), body(&chain))
                })
                .unwrap()
        };
        let before = legs.map(|leg| at(world, &bones, leg));
        for (k, &leg) in legs.iter().enumerate() {
            bones.standing[leg] = before[k].1.y + PLANTED + 0.12;
        }
        let folding = bones.clone();
        world
            .run_system_once(move |mut transforms: Query<&mut Transform>| {
                lift_legs(&folding, &mut transforms)
            })
            .unwrap();
        for (k, &leg) in legs.iter().enumerate() {
            let (joint, end) = before[k];
            let (joint2, end2) = at(world, &bones, leg);
            assert!(end2.y > end.y + 0.03, "leg {leg} folded: {end} -> {end2}");
            assert!(
                joint2.distance(joint) < 1e-4,
                "leg {leg}'s joint stays: {joint} {joint2}"
            );
            // A fold about the side axis keeps the end's side offset
            // exactly (about the bone's own axis instead it drifts ~1 mm).
            assert!(
                (end2.z - end.z).abs() < 2e-4,
                "leg {leg} twisted sideways: {end} -> {end2}"
            );
        }
    }

    #[test]
    fn a_folded_hind_leg_stays_in_the_bodys_sagittal_plane() {
        folds_in_the_sagittal_plane([2, 3]);
    }

    #[test]
    fn a_folded_front_leg_stays_in_the_bodys_sagittal_plane() {
        // The front legs fold at the knee: their chain ends in a single
        // `End`, so the lower bone's parent sits elsewhere in it than a
        // hind leg's.
        folds_in_the_sagittal_plane([0, 1]);
    }

    #[test]
    fn the_croup_and_the_withers_pitch_against_each_other_through_the_loop() {
        let mut low = (f32::MIN, 0.0);
        for k in 0..100 {
            let phase = k as f32 / 100.0;
            let a = articulation(phase);
            // The withers turn by the croup's and every back bone's.
            let withers = a.back + a.torso.iter().sum::<f32>();
            assert!(a.back * withers <= 0.0, "{phase}: {a:?}");
            assert!(a.back.abs() <= CROUP + 1e-6 && a.roll.abs() < 0.05);
            if a.back > low.0 {
                low = (a.back, phase);
            }
            let again = articulation(phase + 1.0);
            assert!((a.back - again.back).abs() < 1e-5 && (a.tail - again.tail).abs() < 1e-5);
        }
        // The croup is lowest (the hindquarters' front end up most) as the
        // hind legs gather under the belly.
        assert!((low.1 - GATHER).abs() < 0.02, "{low:?}");
    }
}
