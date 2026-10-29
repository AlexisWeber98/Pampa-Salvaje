//! Los clips de animación de la gente: la Universal Animation Library de
//! Quaternius (CC0) retargeteada al esqueleto `game_engine` por
//! `tools/retarget.py`, en un único archivo compartido,
//! `assets/characters/animations/humanoid.glb` (ver
//! `assets/characters/README.md`; con crédito en `assets/characters/CREDITS.md`
//! y en el `CREDITS.md` de la raíz).
//!
//! El archivo contiene la jerarquía de huesos del protagonista y los clips, sin
//! mesh. Bevy vincula un clip a un hueso por la ruta de nombres desde la raíz
//! de la escena (`Root/pelvis/spine_01/...`), que comparten todos los modelos
//! de personaje, así que los clips se reproducen en cualquiera de ellos. Se
//! carga una vez al iniciar, con un [`AnimationGraph`] de todos sus clips que
//! comparte el reproductor de cada modelo ([`CharacterClips`]), igual que los
//! del ganado.
//!
//! Lo que reproduce una persona se calcula aquí, como funciones simples que el
//! controlador ([`rig`](super::rig)) llama en cada frame:
//!
//! - la locomoción mezcla una espera, la caminata, el trote y la carrera según
//!   la velocidad sobre el suelo ([`locomotion`]);
//! - cada clip de caminata se reproduce a la velocidad sobre el suelo dividida
//!   por la velocidad que muestran sus pies ([`playback_rate`]), para que no se
//!   deslicen;
//! - cada persona tiene su propia espera ([`idle_for`]) y empieza cada clip en
//!   su propio punto con semilla ([`phase_offset`]).

use bevy::animation::graph::AnimationNodeIndex;
use bevy::gltf::{Gltf, GltfAssetLabel, GltfNode};
use bevy::prelude::*;

use super::figure::FigureKind;
use crate::AssetRoot;
use crate::gltf;

/// El archivo de animación compartido, bajo el [`AssetRoot`].
pub const CLIPS_PATH: &str = "characters/animations/humanoid.glb";

/// Los clips del archivo, en su orden (los `CLIPS` de `tools/retarget.py`; el
/// juego los carga por índice).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Clip {
    Idle,
    IdleTalk,
    Walk,
    WalkFormal,
    Jog,
    Sprint,
    /// Sentado; para los jinetes (PR 2).
    SitIdle,
    /// Sosteniendo las riendas; para los jinetes y las carretas (PR 2).
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

    /// Todos los clips, en el orden del archivo.
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

    /// El nombre del clip en el archivo.
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

    /// Su índice en el archivo (y en [`CharacterClips::nodes`]).
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|c| *c == self).unwrap_or(0)
    }

    /// La velocidad sobre el suelo que muestran sus pies, en m/s, al tamaño del
    /// protagonista: lo que midió `tools/retarget.py` y escribió en los
    /// `extras` del archivo. `None` para un clip que no avanza.
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

/// Las velocidades sobre el suelo (m/s) entre las que un clip de locomoción se
/// funde con el siguiente: de parado a caminando, de caminando a trotando, de
/// trotando a corriendo. El jugador camina a 1.6 m/s y trota a 5; los vecinos
/// caminan a entre 1.25 y 1.5.
pub const WALK_BAND: (f32, f32) = (0.05, 0.6);
pub const JOG_BAND: (f32, f32) = (2.4, 3.4);
pub const SPRINT_BAND: (f32, f32) = (6.5, 8.0);

/// Cuánto más lento o más rápido que lo grabado puede reproducirse un clip de
/// caminata.
pub const RATE: (f32, f32) = (0.5, 2.0);

/// Los pesos de la espera, la caminata, el trote y la carrera a la velocidad
/// sobre el suelo `speed`, en m/s: cada uno se funde con el siguiente a lo
/// largo de su franja. Suman uno.
pub fn locomotion(speed: f32) -> [f32; 4] {
    let ramp = |(from, full): (f32, f32)| ((speed - from) / (full - from)).clamp(0.0, 1.0);
    let (walk, jog, sprint) = (ramp(WALK_BAND), ramp(JOG_BAND), ramp(SPRINT_BAND));
    [1.0 - walk, walk * (1.0 - jog), jog * (1.0 - sprint), sprint]
}

/// Los clips de locomoción que pondera [`locomotion`], después de la espera.
pub const STRIDES: [Clip; 3] = [Clip::Walk, Clip::Jog, Clip::Sprint];

/// Qué tan rápido se reproduce `clip` para una persona que se mueve a `speed`,
/// en m/s: la velocidad sobre el suelo dividida por la velocidad que muestran
/// sus pies, dentro de [`RATE`], para que los pies se queden quietos sobre el
/// suelo. Un clip que no avanza se reproduce tal como se grabó.
pub fn playback_rate(clip: Clip, speed: f32) -> f32 {
    clip.ground_speed()
        .map_or(1.0, |natural| (speed / natural).clamp(RATE.0, RATE.1))
}

/// A qué distancia (en m) se considera que están los pies del centro de un giro
/// en el lugar: se mantiene pequeña para que los pasos sigan siendo cortos (con
/// la velocidad de giro del juego de 1.8 rad/s, 0.36 m/s: una caminata lenta, a
/// medio peso).
pub const TURN_STEP_RADIUS: f32 = 0.2;

/// La velocidad sobre el suelo (m/s) con la que se ponderan y se marcan los
/// clips de zancada para un cuerpo que se mueve a `speed` mientras gira a
/// `turn` rad/s, con los pies a `radius` m del centro del giro
/// ([`TURN_STEP_RADIUS`] para una persona).
///
/// Girar en el lugar da pasos en el sitio al ritmo al que avanzarían los pies
/// sobre un arco pequeño (a [`TURN_STEP_RADIUS`] del centro del giro); un
/// cuerpo que ya se mueve más rápido que eso, o que retrocede, conserva su
/// propia velocidad.
pub fn stepping_speed(speed: f32, turn: f32, radius: f32) -> f32 {
    let pace = turn.abs() * radius;
    if speed.abs() >= pace {
        speed
    } else if speed < 0.0 {
        -pace
    } else {
        pace
    }
}

/// Lo que reproduce parada una persona de tipo `kind`: Farach espera con los
/// brazos cruzados, los demás están en posición de descanso; cualquiera habla
/// con las manos mientras `talking` (sus líneas están en pantalla).
pub fn idle_for(kind: FigureKind, talking: bool) -> Clip {
    match kind {
        _ if talking => Clip::IdleTalk,
        FigureKind::Farach => Clip::IdleFoldArms,
        _ => Clip::Idle,
    }
}

/// La fracción (`0..1`) de cada clip ya reproducida cuando una persona de
/// semilla `seed` lo empieza, para que dos personas nunca se muevan al mismo
/// paso.
pub fn phase_offset(seed: u32) -> f32 {
    // Un hash de enteros pequeño (de splitmix32).
    let mut h = seed.wrapping_add(0x9E37_79B9);
    h = (h ^ (h >> 16)).wrapping_mul(0x21F0_AAAD);
    h = (h ^ (h >> 15)).wrapping_mul(0x735A_2D97);
    h ^= h >> 15;
    (h % 1000) as f32 / 1000.0
}

/// El esqueleto sobre el que se hicieron los clips: dónde descansa su pelvis
/// bajo `Root`, y el largo de sus piernas (de la cadera al tobillo), en metros.
/// Un modelo construido de otra forma tiene los movimientos de su pelvis
/// escalados por sus propias piernas en relación con estas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reference {
    pub pelvis: Vec3,
    pub leg: f32,
}

/// Los clips cargados y su grafo compartido.
#[derive(Resource, Debug, Clone)]
pub struct CharacterClips {
    pub file: Handle<Gltf>,
    pub clips: Vec<Handle<AnimationClip>>,
    pub graph: Handle<AnimationGraph>,
    /// El nodo del grafo de cada [`Clip`], en el orden de [`Clip::ALL`].
    pub nodes: Vec<AnimationNodeIndex>,
    /// Se lee del archivo una vez que terminó de cargar.
    pub reference: Option<Reference>,
}

impl CharacterClips {
    pub fn node(&self, clip: Clip) -> AnimationNodeIndex {
        self.nodes[clip.index()]
    }
}

/// Empieza a cargar los clips y construye su grafo. Sin un servidor de assets,
/// una raíz o el plugin de animación (tests, una app vacía) no hay ninguno, y
/// los modelos quedan en reposo.
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

/// Lee el esqueleto [`Reference`] del archivo una vez que terminó de cargar.
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
    fn turning_in_place_steps_slowly_and_never_changes_a_real_pace() {
        // Parado y sin girar: quieto.
        assert_eq!(stepping_speed(0.0, 0.0, TURN_STEP_RADIUS), 0.0);
        // Girando a la velocidad del juego: un paso corto, dentro del fundido
        // de entrada de la caminata, gire hacia donde gire.
        let pace = stepping_speed(0.0, 1.8, TURN_STEP_RADIUS);
        assert!(pace > WALK_BAND.0 && pace < WALK_BAND.1, "{pace}");
        assert_eq!(stepping_speed(0.0, -1.8, TURN_STEP_RADIUS), pace);
        assert!(
            pace < stepping_speed(0.0, 3.6, TURN_STEP_RADIUS),
            "faster turn, faster steps"
        );
        // Moverse más rápido que eso, o retroceder, no cambia.
        assert_eq!(stepping_speed(1.6, 1.8, TURN_STEP_RADIUS), 1.6);
        assert_eq!(stepping_speed(-1.5, 1.8, TURN_STEP_RADIUS), -1.5);
        assert_eq!(stepping_speed(9.0, 1.8, TURN_STEP_RADIUS), 9.0);
        // Un avance más lento igual da pasos al menos al ritmo del giro, hacia
        // adelante o hacia atrás.
        assert_eq!(stepping_speed(0.1, 1.8, TURN_STEP_RADIUS), pace);
        assert_eq!(stepping_speed(-0.1, 1.8, TURN_STEP_RADIUS), -pace);
    }

    #[test]
    fn the_weights_fade_from_idle_to_walk_to_jog_to_sprint_and_add_up_to_one() {
        assert_eq!(locomotion(0.0), [1.0, 0.0, 0.0, 0.0]);
        // La caminata (1.6 m/s) y el trote (18 km/h) del juego son puros.
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
            // Ir más rápido nunca vuelve a traer un clip más lento.
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
        // La caminata erguida da pasos naturales a unos 1.27 m/s: los ritmos de
        // los vecinos (de 1.25 a 1.5) la reproducen a entre 1.0 y 1.2 veces,
        // así que nunca parece un trotecito apurado; la caminata del paquete, a
        // 1.10 m/s, se reproducía hasta a 1.45 veces.
        let walk = Clip::Walk.ground_speed().unwrap();
        assert!((1.2..1.35).contains(&walk), "{walk}");
        for pace in [1.25, Townsperson::WALK_MAX] {
            let rate = playback_rate(Clip::Walk, pace);
            assert!((0.95..=1.2).contains(&rate), "{pace} m/s: {rate}");
        }
        // El 1.6 m/s del jugador es una caminata enérgica: un poco más rápida,
        // 1.26 veces (los pies siguen quietos: el ritmo es el de la velocidad).
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

    /// El bloque JSON del archivo incluido.
    fn shipped_json() -> String {
        let data = std::fs::read(root().0.join(CLIPS_PATH)).unwrap();
        let len = u32::from_le_bytes(data[12..16].try_into().unwrap()) as usize;
        String::from_utf8(data[20..20 + len].to_vec()).unwrap()
    }

    /// El número que sigue a `"key":` en `json` a partir de `from`.
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
            // El comienzo de la animación siguiente, o el final.
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
                // La pelvis y las piernas del protagonista.
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
