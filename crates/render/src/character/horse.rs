//! Caballos criollos ensillados: el caballo animado de Quaternius (el modelo de
//! los caballos de corral, ver [`livestock`](crate::fauna::livestock)) con un
//! recado de 1919 y una brida de cuero crudo (ver [`tack`](super::tack)), atado
//! o montado.
//!
//! Un caballo ensillado es una entidad de cuerpo ([`HorseBody`], girada hacia
//! el rumbo, +X su frente, su derecha +Z, los vasos en su origen) que contiene
//! el modelo ([`HorseModel`]). El modelo espera su escena ([`HorsePending`]):
//! hasta que termina de cargar no se dibuja nada (las escenas cargan con el
//! juego, mucho antes de que alguien se suba a un caballo). Cuando se crea,
//! [`dress_horse`] les da a sus meshes el pelaje y el rango compartido de los
//! accesorios, como a los caballos de corral, y ubica los huesos que mueven los
//! andares ([`gait`](super::gait)). El mismo evento de escena pone los aperos
//! ([`tack`](super::tack): el recado y la brida con skinning, los estribos y
//! las correas), y una vez puestos los aperos un jinete con modelo se sienta
//! sobre su hueso `Torso` ([`rider`](super::rider)), así que sube, cabecea y
//! gira con el propio clip del caballo.
//!
//! [`drive_horses`] reproduce los clips: la espera, después la caminata al
//! ritmo de la velocidad sobre el suelo hasta unos 4.5 m/s (no hay clip de
//! trote), y después el galope ([`gait_weights`], [`gait_rate`]), cada uno a la
//! velocidad sobre el suelo dividida por la velocidad que muestran sus vasos,
//! para que se queden quietos sobre el suelo; al retroceder, la zancada se
//! reproduce hacia atrás al mismo ritmo; al girar en el lugar, camina en el
//! sitio al ritmo corto de [`stepping_speed`]. Un caballo atado cambia de
//! espera de vez en cuando, como los caballos de corral; uno montado se queda
//! quieto. En el aire ([`VisualPose::flight`](super::VisualPose)) solo se
//! reproduce el clip de salto ([`Clip::GallopJump`], que gana peso a
//! [`JUMP_BLEND_RATE`]): nunca avanza con el reloj, sino que en cada frame se
//! fija en el [`jump_time`] del propio vuelo de la física, así que el recogido
//! y el cabeceo aterrizan con el arco, y los clips de andar retoman donde
//! estaban cuando termina. El vuelo empieza en el propio impulso del clip,
//! demorándose en él ([`JUMP_LINGER`]), y el ajuste agrega el empuje de las
//! patas traseras y un recogido más profundo de las manos (ver
//! [`gait`](super::gait)). Un caballo fuera de la vista o lejos conserva su
//! última pose, pero nunca el del jugador.
//!
//! Los pelajes se pintan en los colores por vertex del modelo según dónde está
//! cada vertex en el caballo parado ([`Coat::color_at`]), así que las manchas,
//! los rodados y la raya de mula del bayo recorren el cuerpo.

use bevy::animation::graph::AnimationGraphHandle;
use bevy::ecs::system::SystemParam;
use bevy::math::Affine3A;
use bevy::prelude::*;
use bevy::world_serialization::{WorldAssetRoot, WorldInstanceReady};
use pampa_core::{AnimationState, Heading, HorseGait};

use super::clips::stepping_speed;
use super::figure::{FigureKind, Rgb, body_rotation};
use super::gait::{
    Bone as GaitBone, GALLOP_MEANS, HorseBones, PARTIAL, Property, UNANIMATED, fit_horse,
};
use super::visual::VisualPose;
use crate::fauna::Animal;
use crate::fauna::livestock::{
    self, CLIP_COUNT, Clip, CoatCache, HORSE_IDLE, HORSE_WALK_SPEED, LivestockModels, ModelPart,
    Species, any_in_view, dress_meshes, frozen, mix, start_offset, view_eye,
};
use crate::gltf::{self, ModelState};

/// El pelaje de un caballo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Coat {
    /// Marrón oscuro entero, crin y cola negras, sin blanco.
    Zaino,
    /// Gris con rodados más claros.
    Tordillo,
    /// Bayo con grandes manchas blancas.
    Overo,
    /// Alazán, con la crin y la cola un poco más claras.
    Alazan,
    /// Bayo: amarillo arena con la crin, la cola y la parte baja de las patas
    /// negras, y una raya oscura a lo largo del lomo.
    Bayo,
}

impl Coat {
    pub const ALL: [Self; 5] = [
        Self::Zaino,
        Self::Tordillo,
        Self::Overo,
        Self::Alazan,
        Self::Bayo,
    ];

    /// El caballo de Don Hidalgo, el que monta el jugador.
    pub const HIDALGO: Self = Self::Bayo;

    /// Un pelaje elegido de forma determinista a partir de `seed`.
    pub fn seeded(seed: u32) -> Self {
        let mut h = seed.wrapping_mul(0x9e37_79b9) ^ 0x85eb_ca6b;
        h ^= h >> 15;
        h = h.wrapping_mul(0x2c1b_3c6d);
        h ^= h >> 12;
        Self::ALL[(h % Self::ALL.len() as u32) as usize]
    }

    fn body(self) -> Rgb {
        match self {
            Self::Zaino => [78, 50, 34],
            Self::Tordillo => [118, 118, 114],
            Self::Overo => [142, 84, 48],
            Self::Alazan => [154, 80, 42],
            Self::Bayo => [198, 162, 104],
        }
    }

    pub(crate) fn mane(self) -> Rgb {
        match self {
            Self::Zaino | Self::Bayo => [34, 28, 24],
            Self::Tordillo => [206, 202, 194],
            Self::Overo => [118, 72, 44],
            Self::Alazan => [176, 104, 58],
        }
    }

    /// La parte baja de las patas, más oscura en la mayoría de los pelajes.
    pub(crate) fn points(self) -> Rgb {
        match self {
            Self::Zaino => [44, 32, 26],
            Self::Tordillo => [92, 92, 90],
            Self::Overo => [142, 84, 48],
            Self::Alazan => [132, 70, 38],
            Self::Bayo => [40, 32, 26],
        }
    }

    /// La cabeza, un poco más clara en el tordillo.
    pub(crate) fn head(self) -> Rgb {
        match self {
            Self::Tordillo => [150, 150, 146],
            _ => self.body(),
        }
    }

    pub(crate) fn hoof(self) -> Rgb {
        match self {
            Self::Tordillo | Self::Overo => [78, 68, 58],
            _ => [38, 32, 28],
        }
    }

    /// El color de un vertex del pelaje en `p` sobre el caballo parado, con su
    /// superficie mirando hacia `normal`.
    pub fn color_at(self, p: Vec3, normal: Vec3) -> Rgb {
        const WHITE: Rgb = [232, 228, 218];
        if p.y < POINTS_HEIGHT {
            // Medias del overo en la mano izquierda y en la pata derecha.
            let stocking = (p.x > 0.0 && p.z < 0.0) || (p.x < 0.0 && p.z > 0.0);
            if self == Self::Overo && stocking {
                return WHITE;
            }
            return self.points();
        }
        match self {
            Self::Overo if overo_patch(p) => WHITE,
            Self::Tordillo if dapple(p) => [178, 176, 170],
            // La raya de mula del bayo, desde la cruz hasta la cola.
            Self::Bayo if normal.y > 0.6 && p.z.abs() < 0.05 && (-0.8..0.45).contains(&p.x) => {
                [104, 76, 46]
            }
            _ => self.body(),
        }
    }
}

/// Por debajo de esta altura (en m) las patas muestran los cabos del pelaje.
const POINTS_HEIGHT: f32 = 0.5;

/// El blanco del overo: grandes manchas en la panza, en ambos costados, en el
/// lado derecho de la grupa, en ambas paletas y a ambos lados del cuello, así
/// que algo de blanco se ve desde cualquier lado.
fn overo_patch(p: Vec3) -> bool {
    const BLOBS: [([f32; 3], [f32; 3]); 8] = [
        ([0.0, 0.8, 0.0], [0.5, 0.2, 0.4]),
        ([-0.15, 1.08, -0.26], [0.36, 0.26, 0.16]),
        ([0.12, 1.02, 0.26], [0.38, 0.32, 0.18]),
        ([-0.56, 1.18, 0.24], [0.22, 0.22, 0.16]),
        ([0.78, 1.5, -0.12], [0.2, 0.24, 0.12]),
        ([0.52, 1.1, 0.22], [0.2, 0.26, 0.14]),
        ([0.9, 1.62, 0.1], [0.18, 0.2, 0.1]),
        ([0.45, 1.0, -0.22], [0.2, 0.26, 0.14]),
    ];
    BLOBS.iter().any(|(c, r)| {
        let d = (p - Vec3::from(*c)) / Vec3::from(*r);
        d.length_squared() < 1.0
    })
}

/// Los rodados del tordillo: un patrón de interferencia 3D barato.
fn dapple(p: Vec3) -> bool {
    let a = (p.x * 23.0 + (p.z * 17.0).sin()).sin();
    let b = (p.y * 21.0 + (p.x * 13.0).cos()).sin();
    let c = (p.z * 19.0 + p.y * 7.0).sin();
    a * b * c > 0.12
}

/// A qué distancia (en m) están los vasos de un caballo del centro de un giro
/// en el lugar: más que los pies de una persona, para que sus pasos mantengan
/// un ritmo de caminata (con la velocidad de giro del juego de 1.8 rad/s,
/// 0.54 m/s).
pub const HORSE_TURN_STEP_RADIUS: f32 = 0.3;

/// Las velocidades sobre el suelo (m/s) entre las que la espera se funde con la
/// caminata.
pub const WALK_BAND: (f32, f32) = (0.05, 0.6);

/// Las velocidades sobre el suelo (m/s) a las que un caballo que camina pasa al
/// galope, y uno que galopa vuelve a la caminata. La lógica hace caminar a un
/// caballo hasta unos 3 m/s y lo "hace galopar" a partir de ahí (trotando hasta
/// [`HorseGait::TROT_MAX`]); sin clip de trote, la caminata se estira hasta
/// unos 4.5 m/s. El andar cambia de golpe (y los pesos se funden rápido,
/// [`HORSE_BLEND_RATE`]), sin mantener nunca los dos mitad y mitad: sus patas
/// no coinciden.
pub const GALLOP_FROM: (f32, f32) = (4.2, 4.8);

/// Si un caballo que se mueve a `speed` galopa, según si `was` galopando.
pub fn gallops(was: bool, speed: f32) -> bool {
    speed > if was { GALLOP_FROM.0 } else { GALLOP_FROM.1 }
}

/// Cuánto más lento o más rápido que lo grabado puede reproducirse la caminata:
/// hasta 3.8 veces a 4.8 m/s.
pub const RATE: (f32, f32) = (0.3, 3.9);

/// Cuánto más lento o más rápido que lo grabado puede reproducirse el galope
/// (estirado): más rápido parece frenético, así que por encima de unos 11.6 m/s
/// sus vasos se deslizan un poco.
pub const GALLOP_RATE: (f32, f32) = (0.5, 2.0);

/// La velocidad sobre el suelo que muestran los vasos del galope con su zancada
/// estirada ([`STRIDE`](super::gait::STRIDE)), en m/s, medida como la del
/// propio clip
/// ([`HORSE_GALLOP_SPEED`](crate::fauna::livestock::HORSE_GALLOP_SPEED)).
pub const GALLOP_SPEED: f32 = 7.3;

/// El peso de cada clip, en el orden de [`Clip::ALL`], para un caballo parado
/// con `idle` o que se mueve a `speed` m/s con `stride` (la caminata o el
/// galope): la espera se funde con la zancada. Suman uno.
pub fn gait_weights(idle: Clip, stride: Clip, speed: f32) -> [f32; CLIP_COUNT] {
    let moving = ((speed - WALK_BAND.0) / (WALK_BAND.1 - WALK_BAND.0)).clamp(0.0, 1.0);
    let mut out = [0.0; CLIP_COUNT];
    out[idle.index()] = 1.0 - moving;
    out[stride.index()] += moving;
    out
}

/// Qué tan rápido se reproduce `clip` para un caballo que se mueve a `speed`
/// m/s: la velocidad sobre el suelo dividida por la velocidad que muestran sus
/// vasos, dentro de [`RATE`], negativa al retroceder; una espera se reproduce
/// tal como se grabó.
pub fn gait_rate(clip: Clip, speed: f32) -> f32 {
    let back = if speed < 0.0 { -1.0 } else { 1.0 };
    let speed = speed.abs();
    match clip {
        Clip::Walk => back * (speed / HORSE_WALK_SPEED).clamp(RATE.0, RATE.1),
        Clip::Gallop => back * (speed / GALLOP_SPEED).clamp(GALLOP_RATE.0, GALLOP_RATE.1),
        _ => 1.0,
    }
}

/// Las esperas entre las que alterna un caballo atado. Atado corto al poste, no
/// puede pastar (y sus riendas, sobre la cabezada, le atravesarían el cuello si
/// lo hiciera).
pub const TIED_IDLES: [Clip; 3] = [Clip::Idle, Clip::Idle2, Clip::IdleHeadlow];

/// El cuerpo de un caballo ensillado, hijo de su entidad de personaje: girado
/// hacia el rumbo, contiene el modelo.
#[derive(Component, Debug, Clone)]
pub struct HorseBody {
    pub coat: Coat,
    /// Quién lo monta, si alguien lo hace.
    pub rider: Option<FigureKind>,
    /// Su andar: lo deciden la animación de la lógica y la velocidad.
    pub gait: HorseGait,
    /// Cuánto galope reproduce ahora (`0..1`): un jinete se inclina con él.
    pub gallop: f32,
    /// Su velocidad sobre el suelo actual, en m/s (ninguna estando parado).
    pub speed: f32,
    /// Qué tan avanzado está el vuelo de un salto (`0..=1`), nada en el suelo:
    /// un jinete mantiene el equilibrio durante él.
    pub flight: Option<f32>,
    /// Su propia semilla: dónde empiezan sus clips y qué espera reproduce un
    /// caballo atado.
    pub seed: u32,
}

impl HorseBody {
    /// La espera con la que está parado a los `clock` segundos: un caballo
    /// montado se queda quieto; uno atado cambia de espera de vez en cuando,
    /// como los caballos de corral ([`TIED_IDLES`]).
    pub fn idle(&self, clock: f32) -> Clip {
        match self.rider {
            Some(_) => Clip::Idle,
            None => {
                let turn = ((clock.max(0.0) + (self.seed % 97) as f32) / HORSE_IDLE) as u32;
                TIED_IDLES[mix(self.seed, turn) as usize % TIED_IDLES.len()]
            }
        }
    }

    /// Si sus clips siguen reproduciéndose fuera de la vista o lejos: el
    /// caballo del jugador siempre lo hace.
    pub fn never_freezes(&self) -> bool {
        self.rider == Some(FigureKind::Protagonist)
    }
}

/// La raíz del modelo del caballo, hija de su [`HorseBody`].
#[derive(Component, Debug, Clone, Copy)]
pub struct HorseModel;

/// Un modelo de caballo que espera que cargue su escena.
#[derive(Component, Debug, Clone, Copy)]
pub struct HorsePending;

/// El estado de animación de un modelo de caballo creado, en su raíz.
#[derive(Component, Debug, Clone)]
pub struct HorseRig {
    /// La entidad con su `AnimationPlayer`.
    pub player: Entity,
    /// Sus meshes dibujadas: está a la vista mientras alguna lo esté.
    pub probes: Vec<Entity>,
    /// El peso actual de cada clip, en el orden de [`Clip::ALL`].
    pub weights: [f32; CLIP_COUNT],
    /// Si sus clips se están reproduciendo.
    pub started: bool,
    /// Si sus huesos conservan su última pose.
    pub frozen: bool,
    /// La duración de cada clip, en segundos, una vez que terminó de cargar.
    pub durations: [f32; CLIP_COUNT],
    /// El clip de zancada que reproduce principalmente (la caminata o el
    /// galope): el otro se pone al mismo paso cuando toma el relevo.
    pub lead: Clip,
    /// Qué tan avanzado está el vuelo de un salto (`0..=1`), nada en el suelo.
    pub flight: Option<f32>,
}

impl HorseRig {
    /// El rig de un modelo, con sus clips reproducidos por `player`, visto a
    /// través de `probes`.
    pub fn new(player: Entity, probes: Vec<Entity>) -> Self {
        Self {
            player,
            probes,
            weights: [0.0; CLIP_COUNT],
            started: false,
            frozen: false,
            durations: [0.0; CLIP_COUNT],
            lead: Clip::Walk,
            flight: None,
        }
    }
}

/// Dónde está el ciclo de la caminata al mismo paso que el del galope: cada uno
/// con la trasera izquierda apoyando (el de la caminata a 0.25 de su ciclo, el
/// del galope en [`GATHER`](super::gait::GATHER)).
pub const WALK_FROM_GALLOP: f32 = 0.25 - super::gait::GATHER;

/// La fracción de su ciclo (`0..1`) en la que empieza un clip de zancada que
/// toma el relevo de `from` en la fase `phase` de su ciclo.
pub fn in_step(to: Clip, phase: f32) -> f32 {
    match to {
        Clip::Walk => (phase + WALK_FROM_GALLOP).rem_euclid(1.0),
        _ => (phase - WALK_FROM_GALLOP).rem_euclid(1.0),
    }
}

/// Qué tan rápido se acercan los pesos de los clips a sus objetivos, por
/// segundo: un fundido rápido, para que rara vez se vean dos clips mitad y
/// mitad.
pub const HORSE_BLEND_RATE: f32 = 12.0;

/// Lo mismo, al despegar: más rápido, para que las patas estén recogidas cuando
/// el caballo deja el suelo.
pub const JUMP_BLEND_RATE: f32 = 40.0;

/// La parte del clip `Gallop_Jump` que es el vuelo, en segundos del clip
/// (medida sobre las curvas de su `Body` y de las patas traseras): desde el
/// impulso de las patas traseras, con los vasos estirándose hacia atrás y hacia
/// abajo al dejar el suelo, hasta el `Body` de vuelta a su altura de reposo,
/// con los vasos abajo. Lo que viene antes es el agachado y la preparación, y
/// lo que viene después, el asentamiento, del que se encargan los clips de
/// zancada.
pub const JUMP_FLIGHT: (f32, f32) = (0.45, 1.17);

/// Cuántos segundos del clip se alarga el impulso del despegue: el clip empieza
/// lento (a alrededor de un tercio de su ritmo) en el impulso de las patas
/// traseras, para que se note cuando el caballo deja el suelo, y se pone al día
/// hacia un tercio del vuelo.
pub const JUMP_LINGER: f32 = 0.15;

/// En qué punto del clip `Gallop_Jump` está un caballo en la fracción `flight`
/// (`0..=1`) de su salto, en segundos: el clip se reparte a lo largo del propio
/// vuelo de la física, no se reproduce con el reloj, y se demora en el impulso
/// ([`JUMP_LINGER`]).
pub fn jump_time(flight: f32) -> f32 {
    let f = flight.clamp(0.0, 1.0);
    let span = JUMP_FLIGHT.1 - JUMP_FLIGHT.0 + JUMP_LINGER;
    JUMP_FLIGHT.0 + span * f - JUMP_LINGER * (1.0 - (1.0 - f).powi(4))
}

/// El transform del modelo bajo su cuerpo: la exportación del caballo girada
/// para mirar hacia +X y escalada a tamaño real (la escala de los caballos de
/// corral).
pub fn model_transform() -> Transform {
    livestock::model_transform(Species::Horse, Heading::default())
}

/// Crea un caballo ensillado con el pelaje `coat` (montado por `rider`) bajo
/// `parent`: su cuerpo y su modelo esperando la escena. Devuelve el cuerpo.
pub(super) fn spawn_horse(
    commands: &mut Commands,
    parent: Entity,
    coat: Coat,
    rider: Option<FigureKind>,
) -> Entity {
    let body = commands
        .spawn((
            Name::new("Horse"),
            HorseBody {
                coat,
                rider,
                gait: HorseGait::Idle,
                gallop: 0.0,
                speed: 0.0,
                flight: None,
                seed: parent.index_u32(),
            },
            Transform::default(),
            Visibility::default(),
            ChildOf(parent),
        ))
        .id();
    commands.spawn((
        Name::new("Horse model"),
        HorseModel,
        HorsePending,
        model_transform(),
        Visibility::default(),
        ChildOf(body),
    ));
    body
}

/// Crea la escena de cada modelo de caballo en espera una vez que terminó de
/// cargar. Sin el archivo del caballo no se dibuja nada.
pub(super) fn swap_in_horses(
    mut commands: Commands,
    server: Option<Res<AssetServer>>,
    models: Option<Res<LivestockModels>>,
    pending: Query<Entity, With<HorsePending>>,
) {
    let Some(models) = models else { return };
    for entity in &pending {
        let Some(model) = models.get(Species::Horse) else {
            commands.entity(entity).remove::<HorsePending>();
            continue;
        };
        let Some(server) = server.as_deref() else {
            return;
        };
        match gltf::model_state(server, &model.scene) {
            ModelState::Ready => {
                commands
                    .entity(entity)
                    .remove::<HorsePending>()
                    .try_insert(WorldAssetRoot(model.scene.clone()));
            }
            ModelState::Failed => {
                commands.entity(entity).remove::<HorsePending>();
            }
            ModelState::Loading => {}
        }
    }
}

/// La ubicación de reposo de `entity` en el marco del cuerpo del modelo `root`
/// (su propio transform y los de sus ancestros, hasta el de la raíz inclusive).
pub(super) fn rest_affine(
    entity: Entity,
    root: Entity,
    parents: &Query<&ChildOf>,
    transforms: &Query<&Transform>,
) -> Affine3A {
    let mut affine = Affine3A::IDENTITY;
    let mut at = entity;
    loop {
        if let Ok(t) = transforms.get(at) {
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

/// Un transform a partir de una ubicación afín.
pub(super) fn transform_of(affine: Affine3A) -> Transform {
    Transform::from_matrix(Mat4::from(affine))
}

/// A partir de qué se prepara un modelo de caballo: los nombres, los reposos y
/// las meshes de sus nodos.
#[derive(SystemParam)]
pub(super) struct HorseScene<'w, 's> {
    names: Query<'w, 's, &'static Name>,
    transforms: Query<'w, 's, &'static Transform>,
    parents: Query<'w, 's, &'static ChildOf>,
    children: Query<'w, 's, &'static Children>,
    parts: Query<'w, 's, ModelPart>,
}

/// Los almacenes a partir de los que se prepara un caballo.
#[derive(SystemParam)]
pub(super) struct HorseStores<'w> {
    cache: Option<ResMut<'w, CoatCache>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
    meshes: ResMut<'w, Assets<Mesh>>,
    models: Option<Res<'w, LivestockModels>>,
}

/// Prepara un modelo de caballo en el momento en que se crea su escena: su
/// pelaje y su rango, el grafo de sus clips y sus huesos para [`fit_horses`].
/// (Los aperos y el jinete los ponen sus propios módulos.)
pub(super) fn dress_horse(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    mut stores: HorseStores,
    scene: HorseScene,
    roots: Query<&ChildOf, With<HorseModel>>,
    bodies: Query<&HorseBody>,
) {
    let root = ready.entity;
    let Ok(child_of) = roots.get(root) else {
        return;
    };
    let carrier = child_of.parent();
    let (Ok(body), Some(species)) = (
        bodies.get(carrier),
        stores
            .models
            .as_ref()
            .and_then(|m| m.get(Species::Horse))
            .cloned(),
    ) else {
        return;
    };
    let HorseStores {
        cache: Some(cache),
        materials,
        meshes,
        ..
    } = &mut stores
    else {
        return;
    };
    let (player, probes) = dress_meshes(
        &mut commands,
        (cache, materials, meshes),
        root,
        (Animal::Horse(body.coat), Species::Horse),
        &scene.children,
        &scene.parts,
    );
    let Some(player) = player else {
        warn!("horse model without an animation player: it stands still");
        return;
    };
    commands
        .entity(player)
        .try_insert(AnimationGraphHandle(species.graph.clone()));
    commands
        .entity(root)
        .try_insert(HorseRig::new(player, probes));
    match find_bones(&scene, root) {
        Some(bones) => {
            commands.entity(root).try_insert(bones);
        }
        None => warn!("horse model without its skeleton: its gallop stays the clip's"),
    }
}

/// Los huesos que mueve [`fit_horses`] bajo el modelo `root`, y sus reposos.
fn find_bones(scene: &HorseScene, root: Entity) -> Option<HorseBones> {
    let named = |name: &str| {
        scene
            .children
            .iter_descendants(root)
            .find(|e| scene.names.get(*e).is_ok_and(|n| n.as_str() == name))
    };
    let all = GaitBone::all();
    let mut bones = [Entity::PLACEHOLDER; GaitBone::COUNT];
    for (k, bone) in all.into_iter().enumerate() {
        bones[k] = named(&bone.name())?;
    }
    let rest = |e: Entity| scene.transforms.get(e).ok().copied();
    let mut partial = Vec::with_capacity(PARTIAL.len() + UNANIMATED.len());
    let unanimated = UNANIMATED.map(|n| (n, Property::Rotation, 0));
    for (name, property, mask) in PARTIAL.into_iter().chain(unanimated) {
        let e = named(name)?;
        partial.push((e, property, mask, rest(e)?));
    }
    let mut means = Vec::with_capacity(GALLOP_MEANS.len());
    for (name, q) in GALLOP_MEANS {
        let leg = usize::from(name.ends_with(".R")) + if name.starts_with("Back") { 2 } else { 0 };
        means.push((leg, named(name)?, Quat::from_array(q).normalize()));
    }
    let armature = rest_affine(
        bones[GaitBone::Armature.index()],
        root,
        &scene.parents,
        &scene.transforms,
    );
    let standing = [0, 1, 2, 3].map(|leg| {
        let hoof = bones[GaitBone::Hoof(leg).index()];
        rest_affine(hoof, root, &scene.parents, &scene.transforms)
            .translation
            .y
    });
    Some(HorseBones {
        bones,
        armature,
        partial,
        means,
        standing,
        body_rest: rest(bones[GaitBone::Body.index()])?.translation,
    })
}

/// Ajusta la pose de cada caballo que se reproduce después de sus clips (ver
/// [`gait`](super::gait)): la mezcla completada, el galope estirado y
/// flexionado, los vasos mantenidos en las patas.
pub(super) fn fit_horses(
    models: Option<Res<LivestockModels>>,
    rigs: Query<(&HorseRig, &HorseBones)>,
    players: Query<&AnimationPlayer>,
    mut transforms: Query<&mut Transform>,
) {
    let Some(species) = models.as_ref().and_then(|m| m.get(Species::Horse)) else {
        return;
    };
    let gallop = Clip::Gallop.index();
    for (rig, bones) in &rigs {
        if rig.frozen || !rig.started {
            continue;
        }
        let phase = players
            .get(rig.player)
            .ok()
            .and_then(|p| p.animation(species.nodes[gallop]))
            .map_or(0.0, |a| a.seek_time() / rig.durations[gallop].max(1e-3));
        fit_horse(bones, &mut transforms, &rig.weights, phase, rig.flight);
    }
}

/// Gira cada caballo ensillado hacia su rumbo y reproduce sus clips (ver las
/// notas del módulo): ponderados y al ritmo de la velocidad, empezados en su
/// punto con semilla, congelados lejos o fuera de la vista salvo que sea el del
/// jugador.
#[allow(clippy::too_many_arguments)]
pub(super) fn drive_horses(
    mut commands: Commands,
    time: Res<Time>,
    models: Option<Res<LivestockModels>>,
    loaded: Option<Res<Assets<AnimationClip>>>,
    cameras: Query<(Entity, &GlobalTransform, &Camera), With<Camera3d>>,
    poses: Query<&VisualPose>,
    mut bodies: Query<(&ChildOf, &mut HorseBody, &mut Transform)>,
    mut rigs: Query<(&ChildOf, &mut HorseRig, &GlobalTransform)>,
    mut players: Query<&mut AnimationPlayer>,
    seen: Query<&ViewVisibility>,
) {
    let dt = time.delta_secs();
    let clock = time.elapsed_secs();
    for (child_of, mut body, mut transform) in &mut bodies {
        let Ok(pose) = poses.get(child_of.parent()) else {
            continue;
        };
        transform.rotation = body_rotation(pose.heading);
        body.gait = HorseGait::of(pose.animation, pose.speed);
    }
    let eye = view_eye(cameras);
    let species = models.as_ref().and_then(|m| m.get(Species::Horse));
    for (child_of, mut rig, at) in &mut rigs {
        let Ok((parent, mut body, _)) = bodies.get_mut(child_of.parent()) else {
            continue;
        };
        let distance = eye.map_or(0.0, |e| e.distance(at.translation()));
        let in_view =
            eye.is_none() || any_in_view(&rig.probes, |p| seen.get(p).is_ok_and(|v| v.get()));
        let freeze = !body.never_freezes() && frozen(rig.frozen, distance, in_view);
        let Some(species) = species else {
            rig.frozen = freeze;
            continue;
        };
        if freeze != rig.frozen {
            rig.frozen = freeze;
            let mut player = commands.entity(rig.player);
            if freeze {
                player.try_remove::<AnimationGraphHandle>();
            } else {
                player.try_insert(AnimationGraphHandle(species.graph.clone()));
            }
        }
        if rig.frozen {
            continue;
        }
        let Ok(mut player) = players.get_mut(rig.player) else {
            continue;
        };
        // Negativa al retroceder: la zancada se reproduce hacia atrás,
        // elegida y ponderada según su magnitud.
        // Girar en el lugar da pasos cortos ([`stepping_speed`]).
        let velocity = match poses.get(parent.parent()) {
            Ok(pose) if pose.animation != AnimationState::Idle => {
                stepping_speed(pose.speed, pose.turn, HORSE_TURN_STEP_RADIUS)
            }
            Ok(pose) => stepping_speed(0.0, pose.turn, HORSE_TURN_STEP_RADIUS),
            _ => 0.0,
        };
        let speed = velocity.abs();
        let stride = if gallops(rig.lead == Clip::Gallop, speed) {
            Clip::Gallop
        } else {
            Clip::Walk
        };
        // En el aire solo se reproduce el salto, a lo largo del vuelo.
        let flight = poses.get(parent.parent()).ok().and_then(|p| p.flight);
        let mut goal = gait_weights(body.idle(clock), stride, speed);
        if flight.is_some() {
            goal = [0.0; CLIP_COUNT];
            goal[Clip::GallopJump.index()] = 1.0;
        }
        if !rig.started {
            let Some(loaded) = loaded.as_deref() else {
                continue;
            };
            let durations: Option<Vec<f32>> = species
                .clips
                .iter()
                .map(|c| loaded.get(c).map(AnimationClip::duration))
                .collect();
            let Some(durations) = durations else {
                continue;
            };
            for (k, (clip, duration)) in Clip::ALL.into_iter().zip(durations).enumerate() {
                player
                    .play(species.nodes[clip.index()])
                    .repeat()
                    .seek_to(start_offset(body.seed, clip) * duration);
                rig.durations[k] = duration;
            }
            rig.weights = goal;
            rig.started = true;
        }
        // El clip de zancada que toma el relevo empieza al mismo paso que el
        // que sale.
        let lead = stride;
        if lead != rig.lead {
            let from = rig.lead;
            let phase = player
                .animation(species.nodes[from.index()])
                .map(|a| a.seek_time() / rig.durations[from.index()].max(1e-3));
            if let (Some(phase), Some(active)) =
                (phase, player.animation_mut(species.nodes[lead.index()]))
            {
                active.seek_to(in_step(lead, phase) * rig.durations[lead.index()]);
            }
            rig.lead = lead;
        }
        let rate = if flight.is_some() {
            JUMP_BLEND_RATE
        } else {
            HORSE_BLEND_RATE
        };
        let k = 1.0 - (-rate * dt).exp();
        for (w, t) in rig.weights.iter_mut().zip(goal) {
            *w += (t - *w) * k;
        }
        body.gallop = rig.weights[Clip::Gallop.index()];
        body.speed = speed;
        body.flight = flight;
        rig.flight = flight;
        for clip in Clip::ALL {
            if let Some(active) = player.animation_mut(species.nodes[clip.index()]) {
                if clip == Clip::GallopJump {
                    // Nunca se reproduce con el reloj (así que nunca hace
                    // bucle): se fija en el punto del vuelo, y se deja ahí
                    // mientras se desvanece.
                    if let Some(flight) = flight {
                        active.seek_to(jump_time(flight));
                    }
                    active.set_weight(rig.weights[clip.index()]).set_speed(0.0);
                } else {
                    active
                        .set_weight(rig.weights[clip.index()])
                        .set_speed(gait_rate(clip, velocity));
                }
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// La superficie de un caballo sustituto: puntos en su costillar, su grupa
    /// y sus paletas (un tubo a lo largo de +X alrededor de 1.14 m de altura),
    /// su cuello y sus cuatro patas, cada uno con su normal hacia afuera.
    fn coat_points() -> Vec<(Vec3, Vec3)> {
        let mut out = Vec::new();
        for i in 0..40 {
            let x = -0.85 + 1.45 * i as f32 / 39.0;
            for k in 0..32 {
                let a = k as f32 / 32.0 * std::f32::consts::TAU;
                let n = Vec3::new(0.0, a.cos(), a.sin());
                out.push((Vec3::new(x, 1.14 + 0.29 * n.y, 0.23 * n.z), n));
            }
        }
        for i in 0..20 {
            let t = i as f32 / 19.0;
            let c = Vec3::new(0.5 + 0.45 * t, 1.3 + 0.45 * t, 0.0);
            for k in 0..16 {
                let a = k as f32 / 16.0 * std::f32::consts::TAU;
                let n = Vec3::new(-0.7 * a.cos(), 0.7 * a.cos(), a.sin()).normalize();
                out.push((c + n * 0.13, n));
            }
        }
        for (x, z) in [(0.55, -0.17), (0.55, 0.17), (-0.65, -0.17), (-0.65, 0.17)] {
            for j in 0..20 {
                for k in 0..10 {
                    let a = k as f32 / 10.0 * std::f32::consts::TAU;
                    let n = Vec3::new(a.cos(), 0.0, a.sin());
                    out.push((Vec3::new(x, 0.9 * j as f32 / 19.0, z) + n * 0.06, n));
                }
            }
        }
        out
    }

    fn coat_colors(coat: Coat) -> Vec<(Vec3, Vec3, Rgb)> {
        coat_points()
            .into_iter()
            .map(|(p, n)| (p, n, coat.color_at(p, n)))
            .collect()
    }

    fn luma(c: Rgb) -> f32 {
        0.3 * c[0] as f32 + 0.59 * c[1] as f32 + 0.11 * c[2] as f32
    }

    #[test]
    fn every_coat_looks_different() {
        let mean = |coat| {
            let v: Vec<_> = coat_colors(coat)
                .into_iter()
                .filter(|(p, _, _)| p.y > POINTS_HEIGHT)
                .collect();
            let sum = v.iter().fold(Vec3::ZERO, |s, (_, _, c)| {
                s + Vec3::new(c[0] as f32, c[1] as f32, c[2] as f32)
            });
            sum / v.len() as f32
        };
        for (i, a) in Coat::ALL.iter().enumerate() {
            for b in &Coat::ALL[i + 1..] {
                let d = mean(*a).distance(mean(*b));
                assert!(d > 20.0, "{a:?} vs {b:?}: {d}");
            }
        }
    }

    #[test]
    fn the_bayo_has_black_points_and_a_dorsal_stripe() {
        let all = coat_colors(Coat::Bayo);
        let legs: Vec<_> = all.iter().filter(|(p, _, _)| p.y < 0.4).collect();
        assert!(!legs.is_empty());
        assert!(legs.iter().all(|(_, _, c)| luma(*c) < 60.0), "black legs");
        let flank = all
            .iter()
            .filter(|(p, n, _)| p.y > 0.9 && n.z.abs() > 0.8 && p.x < 0.45);
        assert!(flank.clone().count() > 20);
        assert!(flank.clone().all(|(_, _, c)| luma(*c) > 140.0), "sandy");
        assert!(luma(Coat::Bayo.mane()) < 60.0, "black mane and tail");
        let stripe: Vec<_> = all
            .iter()
            .filter(|(p, n, _)| n.y > 0.8 && p.z.abs() < 0.02 && (-0.6..0.2).contains(&p.x))
            .collect();
        assert!(stripe.len() >= 3, "{}", stripe.len());
        assert!(
            stripe.iter().all(|(_, _, c)| luma(*c) < 100.0),
            "a dark stripe"
        );
        assert!(
            coat_colors(Coat::Alazan)
                .iter()
                .all(|(_, _, c)| *c != [104, 76, 46]),
            "the other coats have none"
        );
    }

    #[test]
    fn the_overo_shows_white_patches_from_every_three_quarter_view() {
        let all = coat_colors(Coat::Overo);
        for (x, z) in [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
            let toward = Vec3::new(x, 0.6, z).normalize();
            let seen: Vec<_> = all
                .iter()
                .filter(|(p, n, _)| n.dot(toward) > 0.3 && p.y > POINTS_HEIGHT)
                .collect();
            let white = seen.iter().filter(|(_, _, c)| luma(*c) > 200.0).count();
            let share = white as f32 / seen.len() as f32;
            assert!((0.08..0.7).contains(&share), "from ({x}, {z}): {share}");
        }
        assert_ne!(Coat::Overo.hoof(), Coat::Zaino.hoof());
    }

    #[test]
    fn the_tordillo_is_dappled() {
        let barrel: Vec<_> = coat_colors(Coat::Tordillo)
            .into_iter()
            .filter(|(p, _, _)| p.y > 0.8)
            .collect();
        let light = barrel.iter().filter(|(_, _, c)| luma(*c) > 160.0).count();
        let dark = barrel.iter().filter(|(_, _, c)| luma(*c) < 130.0).count();
        let n = barrel.len();
        assert!(
            light * 7 > n && dark * 7 > n,
            "{light} light, {dark} dark of {n}"
        );
    }

    #[test]
    fn coats_are_seeded_deterministically_and_hidalgo_rides_a_bayo() {
        assert_eq!(Coat::HIDALGO, Coat::Bayo);
        for seed in 0..50 {
            assert_eq!(Coat::seeded(seed), Coat::seeded(seed));
        }
        let seen: std::collections::HashSet<_> = (0..200).map(Coat::seeded).collect();
        assert_eq!(seen.len(), Coat::ALL.len(), "every coat turns up");
    }

    #[test]
    fn the_idle_fades_into_the_stride_and_the_gallop_takes_over_outright() {
        let only = |w: [f32; CLIP_COUNT], clip: Clip| {
            w.iter()
                .enumerate()
                .all(|(k, w)| *w == if k == clip.index() { 1.0 } else { 0.0 })
        };
        let (walk, gallop) = (Clip::Walk, Clip::Gallop);
        assert!(only(gait_weights(Clip::Idle, walk, 0.0), Clip::Idle));
        assert!(only(gait_weights(Clip::Idle2, gallop, 0.0), Clip::Idle2));
        assert!(only(gait_weights(Clip::Idle, walk, 3.0), walk));
        assert!(only(gait_weights(Clip::Idle, gallop, 12.2), gallop));
        let setting_off = gait_weights(Clip::Idle, walk, 0.3);
        assert!(setting_off[0] > 0.3 && setting_off[walk.index()] > 0.3);
        for speed in [0.0, 0.1, 0.3, 0.6, 1.0, 1.5, 3.0, 4.8, 8.0, 12.2] {
            for stride in [walk, gallop] {
                let w = gait_weights(Clip::Idle, stride, speed);
                assert!((w.iter().sum::<f32>() - 1.0).abs() < 1e-5, "{speed}");
                // Nunca la caminata y el galope a la vez.
                assert!(w[walk.index()] * w[gallop.index()] == 0.0);
            }
        }
        // La caminata de la lógica camina, la mitad rápida del trote y el
        // galope galopan, con un margen hacia cada lado para que un ritmo
        // constante nunca parpadee.
        assert!(!gallops(false, 3.0) && !gallops(false, 4.5) && gallops(false, 4.9));
        assert!(gallops(true, 4.5) && !gallops(true, 4.1));
        assert!(gallops(false, 12.2) && gallops(true, 12.2));
    }

    #[test]
    fn the_walk_and_the_gallop_play_at_the_ground_speed_over_their_own() {
        assert!((gait_rate(Clip::Walk, HORSE_WALK_SPEED) - 1.0).abs() < 1e-6);
        assert!((gait_rate(Clip::Gallop, GALLOP_SPEED) - 1.0).abs() < 1e-6);
        // La caminata de la lógica y la caminata estirada hasta el tope de la
        // franja del trote.
        assert!((gait_rate(Clip::Walk, 3.0) - 3.0 / HORSE_WALK_SPEED).abs() < 1e-5);
        assert!((gait_rate(Clip::Walk, 4.8) - 4.8 / HORSE_WALK_SPEED).abs() < 1e-5);
        // El galope largo al ritmo del suelo hasta su tope, y después limitado:
        // nunca frenético.
        assert!((gait_rate(Clip::Gallop, 8.0) - 8.0 / GALLOP_SPEED).abs() < 1e-5);
        assert!((gait_rate(Clip::Gallop, 12.2) - 12.2 / GALLOP_SPEED).abs() < 1e-5);
        assert_eq!(gait_rate(Clip::Gallop, 19.4), GALLOP_RATE.1);
        assert_eq!(gait_rate(Clip::Gallop, 0.0), GALLOP_RATE.0);
        assert_eq!(gait_rate(Clip::Walk, 0.0), RATE.0, "clamped");
        assert_eq!(gait_rate(Clip::Idle, 9.0), 1.0, "an idle as recorded");
        assert_eq!(gait_rate(Clip::Eating, 9.0), 1.0);
    }

    #[test]
    fn backing_up_plays_the_walk_and_the_gallop_backward_and_the_idles_forward() {
        for clip in [Clip::Walk, Clip::Gallop] {
            for speed in [0.5, 1.5, 4.8, 12.2, 19.4] {
                let ahead = gait_rate(clip, speed);
                assert_eq!(gait_rate(clip, -speed), -ahead, "{clip:?} at {speed}");
            }
            assert!(gait_rate(clip, -0.0) > 0.0, "a stop is no reverse");
        }
        assert_eq!(gait_rate(Clip::Idle, -1.5), 1.0);
        assert_eq!(gait_rate(Clip::Eating, -1.5), 1.0);
    }

    #[test]
    fn a_ridden_horse_stands_still_and_a_tied_one_changes_idle() {
        let mut body = HorseBody {
            coat: Coat::Zaino,
            rider: Some(FigureKind::Protagonist),
            gait: HorseGait::Idle,
            gallop: 0.0,
            speed: 0.0,
            flight: None,
            seed: 11,
        };
        for k in 0..40 {
            assert_eq!(body.idle(k as f32 * 14.0), Clip::Idle);
        }
        assert!(body.never_freezes());
        body.rider = Some(FigureKind::Hidalgo);
        assert!(!body.never_freezes(), "only the player's");
        body.rider = None;
        assert!(!body.never_freezes());
        let seen: std::collections::HashSet<Clip> =
            (0..40).map(|k| body.idle(k as f32 * 14.0)).collect();
        assert_eq!(seen, TIED_IDLES.into(), "{seen:?}");
        assert_eq!(body.idle(30.0), body.idle(30.0));
    }

    // --- con el caballo, el jinete y los clips incluidos ---------------------

    use crate::character::cpu;
    use crate::character::{CharacterVisual, CharacterVisualPlugin};
    use bevy::mesh::skinning::SkinnedMeshInverseBindposes;

    /// Los plugins de fauna y de personajes con los modelos incluidos, y
    /// `visual` en el origen mirando hacia +X, moviéndose a `speed`; vuelve
    /// cuando el caballo reproduce sus clips.
    pub(crate) fn saddled(visual: CharacterVisual, speed: f32) -> (App, Entity) {
        saddled_every(visual, speed, 50)
    }

    /// [`saddled`], con frames de `ms` milisegundos.
    pub(crate) fn saddled_every(visual: CharacterVisual, speed: f32, ms: u64) -> (App, Entity) {
        use crate::gltf::tests::{root, unfinished_app};
        let mut app = unfinished_app(&root());
        app.init_asset::<SkinnedMeshInverseBindposes>()
            .add_plugins(bevy::animation::AnimationPlugin)
            .insert_resource(root())
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_millis(ms),
            ))
            .add_plugins((crate::fauna::FaunaPlugin, CharacterVisualPlugin));
        // Frames desde que cambió por última vez un asset de escena: una escena
        // creada antes de que su asset se estabilice se vuelve a crear, y los
        // tests retienen sus huesos.
        #[derive(Resource, Default)]
        struct Settled(u32);
        app.init_resource::<Settled>().add_systems(
            Update,
            |mut events: MessageReader<AssetEvent<bevy::world_serialization::WorldAsset>>,
             mut settled: ResMut<Settled>| {
                settled.0 = if events.read().count() > 0 {
                    0
                } else {
                    settled.0 + 1
                };
            },
        );
        app.finish();
        app.cleanup();
        let animation = match speed {
            s if s <= 0.0 => AnimationState::Idle,
            s if s <= 3.0 => AnimationState::Walk,
            _ => AnimationState::Gallop,
        };
        let me = app
            .world_mut()
            .spawn((
                visual,
                VisualPose {
                    animation,
                    speed,
                    ..default()
                },
            ))
            .id();
        // Hasta que el caballo reproduce, el mismo modelo durante un rato, con
        // sus assets estabilizados.
        const QUIET: u32 = 40;
        let mut held = (Entity::PLACEHOLDER, 0);
        for _ in 0..3000 {
            app.update();
            let world = app.world_mut();
            let playing = world
                .query::<(Entity, &HorseRig)>()
                .iter(world)
                .find(|(_, r)| r.started)
                .map(|(_, r)| r.player);
            match playing {
                Some(player) if player == held.0 => held.1 += 1,
                Some(player) => held = (player, 0),
                None => held = (Entity::PLACEHOLDER, 0),
            }
            // Quieto durante 40 frames (al menos 200 ms de tiempo real, sin
            // importar qué tan cargada esté la máquina): un archivo que llegara
            // más tarde volvería a crear la escena en medio de una medición.
            if held.1 >= QUIET && world.resource::<Settled>().0 >= QUIET {
                return (app, me);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("the horse never played");
    }

    #[test]
    fn the_galloping_horse_bends_its_back_through_the_stride() {
        let (mut app, me) = saddled_every(CharacterVisual::Horse(Coat::Bayo), 12.2, 20);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        for _ in 0..80 {
            ride(&mut app, me, 12.2);
        }
        let mut flex = Vec::new();
        for _ in 0..40 {
            ride(&mut app, me, 12.2);
            let [croup, withers] = ["Back", "Torso3"].map(|n| named(app.world(), carrier, n));
            let turn = |e: Entity| {
                app.world()
                    .get::<GlobalTransform>(e)
                    .unwrap()
                    .to_scale_rotation_translation()
                    .1
            };
            flex.push(turn(croup).inverse() * turn(withers));
        }
        // No es un bloque rígido: la grupa y la cruz giran por separado y
        // vuelven.
        let bend = flex
            .iter()
            .map(|q| q.angle_between(flex[0]))
            .fold(0.0, f32::max);
        assert!(bend > 0.1, "the horse's back bends over a stride: {bend}");
    }

    /// `visual` cabalgando a `speed` (un galope) en la fracción `flight` de un
    /// salto, durante un frame.
    fn fly(app: &mut App, me: Entity, flight: Option<f32>) {
        app.world_mut().entity_mut(me).insert(VisualPose {
            animation: AnimationState::Gallop,
            speed: 12.2,
            flight,
            ..default()
        });
        app.update();
    }

    /// El rig que se reproduce: una escena creada de nuevo bajo carga deja uno
    /// nuevo, sin empezar, a su lado hasta que se estabiliza.
    fn rig_of(app: &mut App) -> (Entity, [f32; CLIP_COUNT]) {
        let world = app.world_mut();
        let (_, rig) = world
            .query::<(Entity, &HorseRig)>()
            .iter(world)
            .find(|(_, r)| r.started)
            .unwrap();
        (rig.player, rig.weights)
    }

    /// El vaso de cada pata (su objetivo de IK) desde su articulación superior
    /// (la cadera trasera, el codo delantero), en el marco del cuerpo del
    /// caballo: `[front L, front R, hind L, hind R]`.
    fn hooves_from_joints(app: &App, carrier: Entity) -> [Vec3; 4] {
        let world = app.world();
        let to_body = world
            .get::<GlobalTransform>(carrier)
            .unwrap()
            .affine()
            .inverse();
        let at = |n: &str| {
            let e = named(world, carrier, n);
            to_body.transform_point3(world.get::<GlobalTransform>(e).unwrap().translation())
        };
        [
            ("FrontUpperLeg.L", "IKFrontLeg.L"),
            ("FrontUpperLeg.R", "IKFrontLeg.R"),
            ("BackLeg.L", "IKBackLeg.L"),
            ("BackLeg.R", "IKBackLeg.R"),
        ]
        .map(|(joint, hoof)| at(hoof) - at(joint))
    }

    #[test]
    fn a_jump_holds_the_clip_at_the_flights_point_tucks_the_legs_and_lands_back_in_the_gallop() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = saddled_every(rider, 12.2, 20);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        let node = app
            .world()
            .resource::<LivestockModels>()
            .get(Species::Horse)
            .unwrap()
            .nodes[Clip::GallopJump.index()];
        let jump = Clip::GallopJump.index();
        let seek = |app: &mut App| {
            let (player, _) = rig_of(app);
            let player = app.world().get::<AnimationPlayer>(player).unwrap();
            let active = player.animation(node).unwrap();
            (active.seek_time(), active.speed())
        };
        // Las puntas y el cuerpo en el marco del cuerpo del caballo.
        let in_body = |app: &App, names: &[&str]| -> Vec<Vec3> {
            let world = app.world();
            let to_body = world
                .get::<GlobalTransform>(carrier)
                .unwrap()
                .affine()
                .inverse();
            names
                .iter()
                .map(|n| {
                    let e = named(world, carrier, n);
                    to_body.transform_point3(world.get::<GlobalTransform>(e).unwrap().translation())
                })
                .collect()
        };
        let toes = ["FF.L_end", "FF.R_end", "FFB.L_end", "FFB.R_end"];
        let body_rest = {
            let world = app.world_mut();
            let (_, bones) = world
                .query::<(&HorseRig, &HorseBones)>()
                .iter(world)
                .next()
                .unwrap();
            bones.armature.transform_point3(bones.body_rest).y
        };
        // El galope: el clip de salto en cero, y el suelo de los vasos.
        let mut ground = f32::MAX;
        for _ in 0..80 {
            ride(&mut app, me, 12.2);
            ground = in_body(&app, &toes)
                .iter()
                .map(|t| t.y)
                .fold(ground, f32::min);
        }
        let (_, galloping) = rig_of(&mut app);
        assert!(galloping[jump] < 1e-3 && galloping[Clip::Gallop.index()] > 0.95);
        // El vuelo abarca el propio vuelo del clip, entero.
        assert_eq!(jump_time(0.0), JUMP_FLIGHT.0);
        assert_eq!(jump_time(1.0), JUMP_FLIGHT.1);
        let legs = [
            ("FrontLowerLeg.L_end", "IKFrontLeg.L"),
            ("FrontLowerLeg.R_end", "IKFrontLeg.R"),
            ("BackLowerLeg.L_end", "IKBackLeg.L"),
            ("BackLowerLeg.R_end", "IKBackLeg.R"),
        ];
        for flight in [
            0.0, 0.05, 0.1, 0.2, 0.25, 0.3, 0.45, 0.5, 0.7, 0.8, 0.9, 1.0,
        ] {
            for _ in 0..10 {
                fly(&mut app, me, Some(flight));
            }
            // El clip de salto se fija en el punto del vuelo, nunca se
            // reproduce con el reloj.
            let (_, weights) = rig_of(&mut app);
            assert!(weights[jump] > 0.99, "{flight}: {weights:?}");
            assert!(
                weights[Clip::Gallop.index()] < 0.01,
                "{flight}: {weights:?}"
            );
            let (at, speed) = seek(&mut app);
            assert!((at - jump_time(flight)).abs() < 1e-3, "{flight}: {at}");
            assert_eq!(speed, 0.0);
            // En el aire el cuerpo conserva su altura: se quita la elevación
            // propia del clip.
            if [0.0, 0.2, 0.5, 0.8, 1.0].contains(&flight) {
                let height = in_body(&app, &["Body"])[0].y;
                assert!(
                    (height - body_rest).abs() < 0.03,
                    "{flight}: {height} against rest {body_rest}"
                );
            }
            // Los vasos cuelgan de objetivos de IK bajo el armature, no de las
            // patas: el clip mantiene cada objetivo en el extremo de su pata (a
            // menos de 4 cm, donde su IK no llega), y quitar del cuerpo la
            // elevación del clip no debe dejar atrás los objetivos.
            if (0.1..=0.9).contains(&flight) {
                let world = app.world();
                let at = |n: &str| {
                    world
                        .get::<GlobalTransform>(named(world, carrier, n))
                        .unwrap()
                        .translation()
                };
                for (leg, (end, hoof)) in legs.iter().enumerate() {
                    let gap = at(end).distance(at(hoof));
                    assert!(gap < 0.05, "flight {flight}, leg {leg}: {gap} m");
                }
            }
            let hooves = hooves_from_joints(&app, carrier);
            // Al dejar el suelo: cada pata trasera se estira hacia atrás y
            // hacia abajo desde la cadera, cerca de su largo completo (el clip
            // solo todavía la tiene bajo la panza en ese punto, con el vaso por
            // delante de la cadera).
            if flight == 0.05 {
                for leg in &hooves[2..] {
                    assert!(leg.x < -0.3 && leg.length() > 1.0, "pushing: {leg}");
                }
            }
            if flight == 0.5 {
                // A mitad del vuelo el impulso ya terminó: plegada hacia
                // arriba, con el vaso bajo la panza.
                for leg in &hooves[2..] {
                    assert!(leg.x > 0.1 && leg.length() < 1.0, "folded: {leg}");
                }
                // En lo alto los cuatro vasos están recogidos, lejos del suelo.
                for (leg, toe) in in_body(&app, &toes).iter().enumerate() {
                    assert!(
                        toe.y > ground + 0.1,
                        "leg {leg}: {} against the ground at {ground}",
                        toe.y
                    );
                }
            }
            // En lo alto las manos se recogen cerca de los codos: el clip solo
            // deja los vasos delanteros a unos 0.72 m de ellos en ese punto, en
            // promedio.
            if flight == 0.45 || flight == 0.5 {
                let [l, r, ..] = hooves;
                let reach = (l.length() + r.length()) / 2.0;
                assert!(reach < 0.66, "flight {flight}: {l} {r}");
            }
            // Un punto fijado no sigue avanzando.
            fly(&mut app, me, Some(flight));
            assert_eq!(seek(&mut app).0, at, "{flight}");
        }
        // Aterrizado: de vuelta al galope, el salto en cero, y se queda ahí (el
        // clip no queda en bucle).
        for _ in 0..60 {
            ride(&mut app, me, 12.2);
        }
        let (_, weights) = rig_of(&mut app);
        assert!(weights[jump] < 1e-3, "{weights:?}");
        assert!(weights[Clip::Gallop.index()] > 0.95, "{weights:?}");
        assert!((seek(&mut app).0 - JUMP_FLIGHT.1).abs() < 1e-3);
        // Los pesos de los clips de zancada son los que da el andar solo.
        let goal = gait_weights(Clip::Idle, Clip::Gallop, 12.2);
        for (w, g) in weights.iter().zip(goal) {
            assert!((w - g).abs() < 0.05, "{weights:?} vs {goal:?}");
        }
    }

    /// Escribe frames con skinning en CPU en `PAMPA_PREVIEW` (una carpeta) para
    /// el renderizador de vistas previas offline: un caballo montado parado (y
    /// asentado), caminando y a lo largo de un ciclo de galope, frenando del
    /// galope hasta parar y de una caminata hasta parar, y un caballo atado. Se
    /// ejecuta a mano:
    /// `PAMPA_PREVIEW=/some/dir cargo test -p pampa-render -- --ignored preview`.
    #[test]
    #[ignore = "writes preview files"]
    fn preview_saddled_horses() {
        let Some(dir) = std::env::var_os("PAMPA_PREVIEW") else {
            return;
        };
        let dir = std::path::PathBuf::from(dir);
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let dump = |app: &mut App, me: Entity, name: String| {
            let drawn = cpu::drawn(app.world_mut(), Some(me));
            cpu::write(&dir.join(format!("{name}.txt")), &drawn);
        };
        // Ritmos constantes: frames de 20 ms, y se escribe uno de cada dos.
        for (name, speed, frames) in [("walk", 1.5, 24), ("gallop", 12.2, 16), ("trot", 4.5, 8)] {
            let (mut app, me) = saddled_every(rider, speed, 20);
            for _ in 0..60 {
                ride(&mut app, me, speed);
            }
            for k in 0..frames {
                ride(&mut app, me, speed);
                ride(&mut app, me, speed);
                dump(&mut app, me, format!("{name}_{k:02}"));
            }
        }
        // Frenando tan fuerte como lo hace la lógica (8 m/s²): cada 0.1 s.
        for (name, from) in [("slowdown", 12.2f32), ("walkstop", 1.5)] {
            let (mut app, me) = saddled_every(rider, from, 20);
            for _ in 0..60 {
                ride(&mut app, me, from);
            }
            let mut speed = from;
            for k in 0..24 {
                for _ in 0..5 {
                    speed = (speed - 8.0 * 0.02).max(0.0);
                    ride(&mut app, me, speed);
                }
                dump(&mut app, me, format!("{name}_{k:02}"));
            }
        }
        // Un salto a galope tendido, tal como lo vuela la lógica (alrededor de
        // 1.1 s): cada 0.1 s a lo largo del vuelo, y después el aterrizaje.
        let (mut app, me) = saddled_every(rider, 12.2, 20);
        for _ in 0..60 {
            ride(&mut app, me, 12.2);
        }
        let frames = 55;
        for f in 0..=frames {
            fly(&mut app, me, Some(f as f32 / frames as f32));
            if f % 5 == 0 {
                dump(&mut app, me, format!("jump_{:02}", f / 5));
            }
        }
        for k in 0..6 {
            for _ in 0..5 {
                ride(&mut app, me, 12.2);
            }
            dump(&mut app, me, format!("landing_{k:02}"));
        }
        // Parado: enseguida, y asentado después de unos segundos.
        let (mut app, me) = saddled_every(rider, 0.0, 20);
        dump(&mut app, me, "idle_00".into());
        for k in 1..4 {
            for _ in 0..100 {
                ride(&mut app, me, 0.0);
            }
            dump(&mut app, me, format!("idle_{k:02}"));
        }
        let (mut app, me) = saddled(CharacterVisual::Horse(Coat::Overo), 0.0);
        dump(&mut app, me, "tied_00".into());
    }

    pub(crate) fn named(world: &World, under: Entity, name: &str) -> Entity {
        let children = |e: Entity| world.get::<Children>(e).into_iter().flatten().copied();
        let mut stack: Vec<Entity> = children(under).collect();
        while let Some(e) = stack.pop() {
            if world.get::<Name>(e).is_some_and(|n| n.as_str() == name) {
                return e;
            }
            stack.extend(children(e));
        }
        panic!("no {name}");
    }

    /// Qué tan rápido se deslizan hacia atrás en el marco del cuerpo del
    /// caballo los vasos apoyados de cada frame (puntas a menos de 1.5 cm de la
    /// más baja), en m/s, a lo largo de `frames` frames de `dt` segundos: su
    /// mediana.
    fn hoof_slide(app: &mut App, me: Entity, frames: usize, dt: f32) -> f32 {
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        let mut tracks = Vec::new();
        for _ in 0..frames {
            app.update();
            let world = app.world();
            // Ubicados de nuevo: una escena creada otra vez tiene huesos
            // nuevos.
            let toes = ["FF.L_end", "FF.R_end", "FFB.L_end", "FFB.R_end"]
                .map(|n| named(world, carrier, n));
            let body = world
                .get::<GlobalTransform>(carrier)
                .unwrap()
                .affine()
                .inverse();
            tracks.push(toes.map(|t| {
                body.transform_point3(world.get::<GlobalTransform>(t).unwrap().translation())
            }));
        }
        // El suelo propio de cada vaso: lo más bajo que llega.
        let ground: [f32; 4] =
            std::array::from_fn(|k| tracks.iter().map(|t| t[k].y).fold(f32::MAX, f32::min));
        let mut slides: Vec<f32> = tracks
            .windows(2)
            .flat_map(|w| {
                (0..4)
                    .filter(move |&k| {
                        w[0][k].y < ground[k] + 0.015 && w[1][k].y < ground[k] + 0.015
                    })
                    .map(move |k| -(w[1][k].x - w[0][k].x) / dt)
            })
            .collect();
        assert!(slides.len() > 10, "{slides:?}");
        slides.sort_by(f32::total_cmp);
        slides[slides.len() / 2]
    }

    /// Vuelve a empezar los clips de los caballos de `app` desde sus puntos con
    /// semilla, sobre clips ya mezclados: cuánto tardaron en cargar sus
    /// archivos (y por lo tanto cuántos frames se reprodujeron mientras
    /// [`saddled_every`] esperaba) ya no decide en qué punto de su ciclo
    /// empieza una medición.
    fn from_the_seeded_start(app: &mut App) {
        let world = app.world_mut();
        let nodes = world
            .resource::<LivestockModels>()
            .get(Species::Horse)
            .expect("the horse's model")
            .nodes
            .clone();
        let rigs: Vec<(Entity, u32, [f32; CLIP_COUNT])> = world
            .query::<(&ChildOf, &HorseRig)>()
            .iter(world)
            .map(|(child_of, rig)| {
                let body = world.get::<HorseBody>(child_of.parent()).unwrap();
                (rig.player, body.seed, rig.durations)
            })
            .collect();
        assert!(!rigs.is_empty());
        for (player, seed, durations) in rigs {
            let mut player = world.get_mut::<AnimationPlayer>(player).unwrap();
            for clip in Clip::ALL {
                let active = player.animation_mut(nodes[clip.index()]).unwrap();
                active.seek_to(start_offset(seed, clip) * durations[clip.index()]);
            }
        }
    }

    #[test]
    fn the_long_gallop_keeps_its_hooves_still_up_to_its_fastest_unhurried_pace() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = saddled_every(rider, 8.0, 10);
        for speed in [8.0, 12.2, 19.4] {
            app.world_mut().entity_mut(me).insert(VisualPose {
                animation: AnimationState::Gallop,
                speed,
                ..default()
            });
            from_the_seeded_start(&mut app);
            for _ in 0..100 {
                app.update();
            }
            let back = hoof_slide(&mut app, me, 300, 0.01);
            let rate = gait_rate(Clip::Gallop, speed);
            let (natural, slide) = (back / rate, speed - back);
            println!(
                "{speed} m/s: hooves back at {back:.2} m/s (rate {rate:.2}, natural {natural:.2}), slide {slide:.2} m/s"
            );
            // El ciclo estirado muestra la velocidad con la que se marca su
            // ritmo.
            assert!((natural / GALLOP_SPEED - 1.0).abs() < 0.06, "{natural}");
            if speed <= GALLOP_SPEED * GALLOP_RATE.1 {
                assert!(slide.abs() < 0.5, "{speed} m/s: slides {slide} m/s");
            } else {
                // Pasado el tope (el turbo de desarrollo) se desliza, sin ir
                // más rápido.
                assert!(slide > 0.0 && slide < speed - GALLOP_SPEED * GALLOP_RATE.1 + 0.5);
            }
        }
    }

    #[test]
    fn a_horse_turning_in_place_takes_short_slow_steps_and_stops_when_the_turn_does() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = saddled_every(rider, 0.0, 50);
        let walk = |app: &mut App| {
            let world = app.world_mut();
            let (rig, speed) = {
                let mut q = world.query::<&HorseRig>();
                let rig = q.iter(world).find(|r| r.started).unwrap();
                (rig.weights[Clip::Walk.index()], rig.player)
            };
            let species = world
                .resource::<LivestockModels>()
                .get(Species::Horse)
                .unwrap()
                .nodes[Clip::Walk.index()];
            let rate = world
                .get::<AnimationPlayer>(speed)
                .unwrap()
                .animation(species)
                .unwrap()
                .speed();
            (rig, rate)
        };
        for _ in 0..20 {
            app.update();
        }
        assert!(walk(&mut app).0 < 1e-3, "standing still stays idle");
        app.world_mut().entity_mut(me).insert(VisualPose {
            animation: AnimationState::Idle,
            speed: 0.0,
            turn: 1.8,
            ..default()
        });
        for _ in 0..20 {
            app.update();
        }
        let (weight, rate) = walk(&mut app);
        assert!(weight > 0.2 && weight < 0.95, "{weight}");
        // Por encima del ritmo mínimo de la caminata, para que los pasos no se
        // arrastren.
        assert!(rate > RATE.0 + 0.05 && rate <= 0.5, "{rate}");
        app.world_mut().entity_mut(me).insert(VisualPose::default());
        for _ in 0..40 {
            app.update();
        }
        assert!(walk(&mut app).0 < 1e-2, "back to idle");
    }

    #[test]
    fn a_backing_horse_steps_back_its_hooves_sliding_forward_as_fast_as_ahead() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let speed = 1.5;
        let (mut app, me) = saddled_every(rider, speed, 10);
        from_the_seeded_start(&mut app);
        // A lo largo de un ciclo de caminata y algo más hacia cada lado
        // (1.17 s).
        let ahead = hoof_slide(&mut app, me, 150, 0.01);
        app.world_mut().entity_mut(me).insert(VisualPose {
            animation: AnimationState::Walk,
            speed: -speed,
            ..default()
        });
        let back = hoof_slide(&mut app, me, 150, 0.01);
        println!("{speed} m/s: hooves back at {ahead:.2} m/s ahead, {back:.2} m/s backing");
        assert!(ahead > 0.5, "{ahead}");
        assert!(
            (back / -ahead - 1.0).abs() < 0.2,
            "backing: {back} m/s, ahead {ahead} m/s"
        );
    }

    /// Por cada frame del caballo de `app` bajo `me`: la punta más baja (marco
    /// del cuerpo), lo más lejos que está cualquier vaso del extremo de su
    /// pata, y el giro de cada hueso de pata respecto de `rest` (según
    /// [`LEG_BONES`]).
    const LEG_BONES: [&str; 14] = [
        "FrontUpperLeg.L",
        "FrontUpperLeg.R",
        "FrontLowerLeg.L",
        "FrontLowerLeg.R",
        "BackShoulder.L",
        "BackShoulder.R",
        "BackLeg.L",
        "BackLeg.R",
        "BackUpperLeg.L",
        "BackUpperLeg.R",
        "BackLowerLeg.L",
        "BackLowerLeg.R",
        "Back",
        "Body",
    ];

    fn legs_now(app: &App, carrier: Entity, rest: &[Quat]) -> (f32, f32, Vec<f32>) {
        let world = app.world();
        let body = world
            .get::<GlobalTransform>(carrier)
            .unwrap()
            .affine()
            .inverse();
        let at = |n: &str| {
            body.transform_point3(
                world
                    .get::<GlobalTransform>(named(world, carrier, n))
                    .unwrap()
                    .translation(),
            )
        };
        let low = ["FF.L_end", "FF.R_end", "FFB.L_end", "FFB.R_end"]
            .map(|n| at(n).y)
            .into_iter()
            .fold(f32::MAX, f32::min);
        let apart = [
            ("IKFrontLeg.L", "FrontLowerLeg.L_end"),
            ("IKFrontLeg.R", "FrontLowerLeg.R_end"),
            ("IKBackLeg.L", "BackLowerLeg.L_end"),
            ("IKBackLeg.R", "BackLowerLeg.R_end"),
        ]
        .map(|(h, e)| at(h).distance(at(e)))
        .into_iter()
        .fold(0.0, f32::max);
        let turns = LEG_BONES
            .iter()
            .zip(rest)
            .map(|(n, r)| {
                world
                    .get::<Transform>(named(world, carrier, n))
                    .unwrap()
                    .rotation
                    .angle_between(*r)
            })
            .collect();
        (low, apart, turns)
    }

    /// Reproduce el caballo de `app` a cada velocidad de `speeds` durante un
    /// frame, con la animación de la lógica siguiendo la velocidad como lo hace
    /// la del jinete.
    pub(crate) fn ride(app: &mut App, me: Entity, speed: f32) {
        let animation = if speed > 3.0 {
            AnimationState::Gallop
        } else if speed > AnimationState::IDLE_THRESHOLD {
            AnimationState::Walk
        } else {
            AnimationState::Idle
        };
        app.world_mut().entity_mut(me).insert(VisualPose {
            animation,
            speed,
            ..default()
        });
        app.update();
    }

    #[test]
    fn slowing_down_from_a_gallop_or_a_walk_never_folds_a_leg_or_sinks_a_hoof() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = saddled_every(rider, 0.0, 20);
        let carrier = app.world().get::<Children>(me).unwrap()[0];
        let rest: Vec<Quat> = LEG_BONES
            .iter()
            .map(|n| {
                let e = named(app.world(), carrier, n);
                *app.world().get::<Transform>(e).unwrap()
            })
            .map(|t| t.rotation)
            .collect();
        // Los límites: lo más lejos que gira cada hueso a ritmos constantes.
        let mut limit = vec![0.0f32; LEG_BONES.len()];
        for speed in [0.0, 1.5, 3.0, 4.5, 8.0, 12.2] {
            for frame in 0..60 {
                ride(&mut app, me, speed);
                if frame > 30 {
                    let (_, _, turns) = legs_now(&app, carrier, &rest);
                    for (l, t) in limit.iter_mut().zip(turns) {
                        *l = l.max(t);
                    }
                }
            }
        }
        // Frenando fuerte (8 m/s², el de la lógica) y avanzando por inercia
        // (3 m/s²) desde la velocidad máxima y desde una caminata, hasta parar.
        for (from, frames) in [(12.2f32, 76), (12.2, 200), (1.5, 10), (1.5, 25)] {
            for _ in 0..40 {
                ride(&mut app, me, from);
            }
            for k in 0..=frames + 60 {
                let speed = from * (1.0 - k as f32 / frames as f32).max(0.0);
                ride(&mut app, me, speed);
                let (low, apart, turns) = legs_now(&app, carrier, &rest);
                assert!(low > -0.03, "from {from} at {speed:.2} m/s: a toe at {low}");
                assert!(
                    apart < 0.06,
                    "from {from} at {speed:.2} m/s: a hoof {apart} m off its leg"
                );
                for ((n, t), l) in LEG_BONES.iter().zip(turns).zip(&limit) {
                    assert!(
                        t <= l + 0.12,
                        "from {from} at {speed:.2} m/s: {n} turned {t} (steady {l})"
                    );
                }
            }
        }
    }
}
