//! The people's animation clips: Quaternius' Universal Animation Library
//! (CC0) retargeted to the `game_engine` skeleton by `tools/retarget.py`,
//! in one shared file, `assets/characters/animations/humanoid.glb` (see
//! `assets/characters/README.md`; credited in `assets/characters/CREDITS.md`
//! and the root `CREDITS.md`).
//!
//! The file holds the protagonist's bone hierarchy and the clips, no mesh.
//! Bevy binds a clip to a bone by its name path from the scene root
//! (`Root/pelvis/spine_01/...`), which every character model shares, so
//! the clips play on any of them. It is loaded once at startup, with one
//! [`AnimationGraph`] of all its clips that every model's player shares
//! ([`CharacterClips`]), as the livestock's are.
//!
//! What a person plays is worked out here, as plain functions the driver
//! ([`rig`](super::rig)) calls each frame:
//!
//! - locomotion blends an idle, the walk, the jog and the sprint by ground
//!   speed ([`locomotion`]);
//! - each walking clip plays at the ground speed over the speed its feet
//!   show ([`playback_rate`]), so they do not slide;
//! - each person has their own idle ([`idle_for`]) and starts every clip
//!   at their own seeded point ([`phase_offset`]).

use bevy::animation::graph::AnimationNodeIndex;
use bevy::gltf::{Gltf, GltfAssetLabel, GltfNode};
use bevy::prelude::*;

use super::figure::FigureKind;
use crate::AssetRoot;
use crate::gltf;

/// The shared animation file, under the [`AssetRoot`].
pub const CLIPS_PATH: &str = "characters/animations/humanoid.glb";

/// The clips in the file, in its order (`tools/retarget.py`'s `CLIPS`; the
/// game loads them by index).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Clip {
    Idle,
    IdleTalk,
    Walk,
    WalkFormal,
    Jog,
    Sprint,
    /// Seated; for the riders (PR 2).
    SitIdle,
    /// Holding reins; for the riders and the carts (PR 2).
    Drive,
    Interact,
    IdleFoldArms,
    Yes,
    No,
    Harvest,
    Plant,
    Water,
    Chop,
    WalkCarry,
}

impl Clip {
    pub const COUNT: usize = 17;

    /// Every clip, in file order.
    pub const ALL: [Self; Self::COUNT] = [
        Self::Idle,
        Self::IdleTalk,
        Self::Walk,
        Self::WalkFormal,
        Self::Jog,
        Self::Sprint,
        Self::SitIdle,
        Self::Drive,
        Self::Interact,
        Self::IdleFoldArms,
        Self::Yes,
        Self::No,
        Self::Harvest,
        Self::Plant,
        Self::Water,
        Self::Chop,
        Self::WalkCarry,
    ];

    /// The clip's name in the file.
    pub fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::IdleTalk => "idle_talk",
            Self::Walk => "walk",
            Self::WalkFormal => "walk_formal",
            Self::Jog => "jog",
            Self::Sprint => "sprint",
            Self::SitIdle => "sit_idle",
            Self::Drive => "drive",
            Self::Interact => "interact",
            Self::IdleFoldArms => "idle_fold_arms",
            Self::Yes => "yes",
            Self::No => "no",
            Self::Harvest => "harvest",
            Self::Plant => "plant",
            Self::Water => "water",
            Self::Chop => "chop",
            Self::WalkCarry => "walk_carry",
        }
    }

    /// Its index in the file (and in [`CharacterClips::nodes`]).
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|c| *c == self).unwrap_or(0)
    }

    /// The ground speed its feet show, m/s, at the protagonist's size: what
    /// `tools/retarget.py` measured and wrote into the file's `extras`.
    /// `None` for a clip that does not travel.
    pub fn ground_speed(self) -> Option<f32> {
        match self {
            Self::Walk => Some(1.2709),
            Self::WalkFormal => Some(1.0964),
            Self::Jog => Some(6.1488),
            Self::Sprint => Some(9.2842),
            Self::WalkCarry => Some(0.6968),
            _ => None,
        }
    }
}

/// The ground speeds (m/s) between which one locomotion clip fades into the
/// next: standing to walking, walking to jogging, jogging to sprinting. The
/// player walks at 1.6 m/s and jogs at 5; townsfolk walk at 1.25 to 1.5.
pub const WALK_BAND: (f32, f32) = (0.05, 0.6);
pub const JOG_BAND: (f32, f32) = (2.4, 3.4);
pub const SPRINT_BAND: (f32, f32) = (6.5, 8.0);

/// How much slower or faster than recorded a walking clip may play.
pub const RATE: (f32, f32) = (0.5, 2.0);

/// The weights of the idle, the walk, the jog and the sprint at ground
/// speed `speed`, m/s: each fades into the next across its band. They
/// add up to one.
pub fn locomotion(speed: f32) -> [f32; 4] {
    let ramp = |(from, full): (f32, f32)| ((speed - from) / (full - from)).clamp(0.0, 1.0);
    let (walk, jog, sprint) = (ramp(WALK_BAND), ramp(JOG_BAND), ramp(SPRINT_BAND));
    [1.0 - walk, walk * (1.0 - jog), jog * (1.0 - sprint), sprint]
}

/// The locomotion clips [`locomotion`] weighs, after the idle.
pub const STRIDES: [Clip; 3] = [Clip::Walk, Clip::Jog, Clip::Sprint];

/// How fast `clip` plays for a person moving at `speed`, m/s: the ground
/// speed over the speed its feet show, within [`RATE`], so the feet keep
/// still on the ground. A clip that does not travel plays as recorded.
pub fn playback_rate(clip: Clip, speed: f32) -> f32 {
    clip.ground_speed()
        .map_or(1.0, |natural| (speed / natural).clamp(RATE.0, RATE.1))
}

/// What a person of `kind` plays standing: Farach waits with his arms
/// folded, the others stand easy; anyone talks with their hands while
/// `talking` (their lines are on screen).
pub fn idle_for(kind: FigureKind, talking: bool) -> Clip {
    match kind {
        _ if talking => Clip::IdleTalk,
        FigureKind::Farach => Clip::IdleFoldArms,
        _ => Clip::Idle,
    }
}

/// The share (`0..1`) of every clip already played when a person of
/// `seed` starts it, so two people never move in step.
pub fn phase_offset(seed: u32) -> f32 {
    // A small integer hash (from splitmix32).
    let mut h = seed.wrapping_add(0x9E37_79B9);
    h = (h ^ (h >> 16)).wrapping_mul(0x21F0_AAAD);
    h = (h ^ (h >> 15)).wrapping_mul(0x735A_2D97);
    h ^= h >> 15;
    (h % 1000) as f32 / 1000.0
}

/// The skeleton the clips were made on: where its pelvis rests under
/// `Root`, and its leg length (hip to ankle), meters. A model built
/// differently has its pelvis's moves scaled by its own legs over these.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reference {
    pub pelvis: Vec3,
    pub leg: f32,
}

/// The loaded clips and their shared graph.
#[derive(Resource, Debug, Clone)]
pub struct CharacterClips {
    pub file: Handle<Gltf>,
    pub clips: Vec<Handle<AnimationClip>>,
    pub graph: Handle<AnimationGraph>,
    /// The graph node of each [`Clip`], in [`Clip::ALL`] order.
    pub nodes: Vec<AnimationNodeIndex>,
    /// Read from the file once it has loaded.
    pub reference: Option<Reference>,
}

impl CharacterClips {
    pub fn node(&self, clip: Clip) -> AnimationNodeIndex {
        self.nodes[clip.index()]
    }
}

/// Starts loading the clips and builds their graph. Without an asset
/// server, a root or the animation plugin (tests, a bare app) there are
/// none, and the models stand at rest.
pub(super) fn load_character_clips(
    mut commands: Commands,
    server: Option<Res<AssetServer>>,
    root: Option<Res<AssetRoot>>,
    graphs: Option<ResMut<Assets<AnimationGraph>>>,
) {
    let (Some(server), Some(root), Some(mut graphs)) = (server, root, graphs) else {
        return;
    };
    if !gltf::is_inside(CLIPS_PATH) || !root.0.join(CLIPS_PATH).is_file() {
        warn!("no character clips at {CLIPS_PATH}: the models stand still");
        return;
    }
    let clips: Vec<Handle<AnimationClip>> = Clip::ALL
        .iter()
        .map(|c| server.load(GltfAssetLabel::Animation(c.index()).from_asset(CLIPS_PATH)))
        .collect();
    let (graph, nodes) = AnimationGraph::from_clips(clips.iter().cloned());
    commands.insert_resource(CharacterClips {
        file: server.load(CLIPS_PATH),
        clips,
        graph: graphs.add(graph),
        nodes,
        reference: None,
    });
}

/// Reads the [`Reference`] skeleton from the file once it has loaded.
pub(super) fn read_reference(
    clips: Option<ResMut<CharacterClips>>,
    files: Option<Res<Assets<Gltf>>>,
    nodes: Option<Res<Assets<GltfNode>>>,
) {
    let (Some(mut clips), Some(files), Some(nodes)) = (clips, files, nodes) else {
        return;
    };
    if clips.reference.is_some() {
        return;
    }
    let Some(file) = files.get(&clips.file) else {
        return;
    };
    let at = |name: &str| {
        file.named_nodes
            .get(name)
            .and_then(|n| nodes.get(n))
            .map(|n| n.transform.translation)
    };
    if let (Some(pelvis), Some(knee), Some(ankle)) = (at("pelvis"), at("calf_l"), at("foot_l")) {
        clips.reference = Some(Reference {
            pelvis,
            leg: knee.length() + ankle.length(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gltf::tests::{root, unfinished_app};

    #[test]
    fn the_weights_fade_from_idle_to_walk_to_jog_to_sprint_and_add_up_to_one() {
        assert_eq!(locomotion(0.0), [1.0, 0.0, 0.0, 0.0]);
        // The game's walk (1.6 m/s) and jog (18 km/h) are pure.
        assert_eq!(locomotion(1.6), [0.0, 1.0, 0.0, 0.0]);
        assert_eq!(locomotion(5.0), [0.0, 0.0, 1.0, 0.0]);
        assert_eq!(locomotion(9.0), [0.0, 0.0, 0.0, 1.0]);
        let mid = locomotion((JOG_BAND.0 + JOG_BAND.1) / 2.0);
        assert!(
            (mid[1] - 0.5).abs() < 1e-5 && (mid[2] - 0.5).abs() < 1e-5,
            "{mid:?}"
        );
        let mut last = locomotion(0.0);
        for k in 0..1000 {
            let w = locomotion(k as f32 * 0.01);
            assert!((w.iter().sum::<f32>() - 1.0).abs() < 1e-5, "{k}: {w:?}");
            assert!(w.iter().all(|w| (0.0..=1.0).contains(w)));
            // Faster never brings back a slower clip.
            assert!(w[0] <= last[0] && w[3] >= last[3], "{k}");
            last = w;
        }
    }

    #[test]
    fn a_walking_clip_plays_at_the_ground_speed_over_its_own() {
        let walk = Clip::Walk.ground_speed().unwrap();
        assert!((playback_rate(Clip::Walk, walk) - 1.0).abs() < 1e-6);
        assert!((playback_rate(Clip::Walk, 1.6) - 1.6 / walk).abs() < 1e-5);
        assert!((playback_rate(Clip::Jog, 5.0) - 5.0 / 6.1488).abs() < 1e-5);
        assert_eq!(playback_rate(Clip::Walk, 0.0), RATE.0, "clamped");
        assert_eq!(playback_rate(Clip::Walk, 40.0), RATE.1, "clamped");
        assert_eq!(
            playback_rate(Clip::Idle, 3.0),
            1.0,
            "an idle plays as recorded"
        );
    }

    #[test]
    fn the_walk_steps_at_an_easy_pace_never_hurried() {
        use pampa_core::Townsperson;
        // The stood-up walk steps naturally at about 1.27 m/s: the
        // townsfolk's paces (1.25 to 1.5) play it at 1.0 to 1.2 times, so
        // it never reads as a hurried scurry; the pack's walk, at 1.10 m/s,
        // played up to 1.45 times.
        let walk = Clip::Walk.ground_speed().unwrap();
        assert!((1.2..1.35).contains(&walk), "{walk}");
        for pace in [1.25, Townsperson::WALK_MAX] {
            let rate = playback_rate(Clip::Walk, pace);
            assert!((0.95..=1.2).contains(&rate), "{pace} m/s: {rate}");
        }
        // The player's 1.6 m/s is a brisk walk: a little faster, 1.26
        // times (the feet still keep still: the rate is the speed's).
        assert_eq!(pampa_core::RiderParams::on_foot().walk_max, 1.6);
        assert!(playback_rate(Clip::Walk, 1.6) <= 1.27);
    }

    #[test]
    fn a_person_with_shorter_legs_plays_the_strides_faster_by_their_length() {
        use crate::character::rig::model_stride;
        let reference = Reference {
            pelvis: Vec3::ZERO,
            leg: 0.8,
        };
        let pelvis = |leg: f32| Some((Entity::PLACEHOLDER, Vec3::ZERO, leg));
        assert_eq!(model_stride(pelvis(0.8), Some(reference)), 1.0);
        assert!((model_stride(pelvis(0.72), Some(reference)) - 0.9).abs() < 1e-6);
        assert_eq!(model_stride(None, Some(reference)), 1.0);
        assert_eq!(model_stride(pelvis(0.72), None), 1.0);
    }

    #[test]
    fn each_person_has_their_own_idle() {
        assert_eq!(idle_for(FigureKind::Farach, false), Clip::IdleFoldArms);
        assert_eq!(idle_for(FigureKind::Hidalgo, false), Clip::Idle);
        assert_eq!(idle_for(FigureKind::Bairoletto, false), Clip::Idle);
        assert_eq!(idle_for(FigureKind::Protagonist, false), Clip::Idle);
        for kind in FigureKind::all() {
            assert_eq!(idle_for(kind, true), Clip::IdleTalk, "{kind:?}");
        }
    }

    #[test]
    fn people_start_their_clips_apart_but_each_the_same_way_every_time() {
        let offsets: Vec<f32> = (0..32).map(phase_offset).collect();
        assert!(offsets.iter().all(|o| (0.0..1.0).contains(o)));
        assert_eq!(phase_offset(7), phase_offset(7));
        let distinct: std::collections::HashSet<u32> =
            offsets.iter().map(|o| (o * 1000.0) as u32).collect();
        assert!(distinct.len() > 28, "{distinct:?}");
    }

    /// The JSON chunk of the shipped file.
    fn shipped_json() -> String {
        let data = std::fs::read(root().0.join(CLIPS_PATH)).unwrap();
        let len = u32::from_le_bytes(data[12..16].try_into().unwrap()) as usize;
        String::from_utf8(data[20..20 + len].to_vec()).unwrap()
    }

    /// The number after `"key":` in `json` from `from` on.
    fn number_after(json: &str, from: usize, key: &str) -> Option<f32> {
        let at = json[from..].find(&format!("\"{key}\":"))? + from + key.len() + 3;
        let end = json[at..].find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))?;
        json[at..at + end].parse().ok()
    }

    #[test]
    fn the_clips_are_in_the_files_order_and_their_speeds_are_the_measured_ones() {
        let json = shipped_json();
        let mut from = 0;
        for clip in Clip::ALL {
            let at = json[from..]
                .find(&format!("{{\"name\":\"{}\",\"channels\"", clip.name()))
                .unwrap_or_else(|| panic!("{} not next in the file", clip.name()))
                + from;
            // The next animation's start, or the end.
            let next = json[at + 1..]
                .find("{\"name\":")
                .map_or(json.len(), |n| n + at + 1);
            let speed = number_after(&json[..next], at, "ground_speed");
            match (clip.ground_speed(), speed) {
                (Some(mine), Some(file)) => assert!((mine - file).abs() < 1e-3, "{clip:?}"),
                (None, None) => {}
                other => panic!("{clip:?}: {other:?}"),
            }
            from = at;
        }
    }

    #[test]
    fn the_file_loads_with_every_clip_and_its_reference_skeleton() {
        let mut app = unfinished_app(&root());
        app.add_plugins(bevy::animation::AnimationPlugin)
            .insert_resource(root())
            .add_systems(Startup, load_character_clips)
            .add_systems(Update, read_reference);
        app.finish();
        app.cleanup();
        for _ in 0..2000 {
            app.update();
            let clips = app.world().resource::<CharacterClips>();
            let loaded = app.world().resource::<Assets<AnimationClip>>();
            if let Some(reference) = clips.reference
                && clips.clips.iter().all(|c| loaded.contains(c))
            {
                // The protagonist's pelvis and legs.
                assert!((reference.pelvis.x - 0.9391).abs() < 1e-3, "{reference:?}");
                assert!((reference.leg - 0.865).abs() < 2e-3, "{reference:?}");
                let gltf = app
                    .world()
                    .resource::<Assets<Gltf>>()
                    .get(&clips.file)
                    .unwrap();
                for clip in Clip::ALL {
                    assert_eq!(
                        gltf.named_animations.get(clip.name()),
                        Some(&clips.clips[clip.index()]),
                        "{clip:?}"
                    );
                }
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("the clips never loaded");
    }
}
