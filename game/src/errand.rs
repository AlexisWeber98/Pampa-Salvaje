//! El primer encargo, en juego: el jugador se baja a pie del tren de la mañana
//! en el primer pueblo (la apertura, ver [`crate::ride`]), habla con Don
//! Hidalgo en la plaza, toma su caballo del almacén de ramos generales y
//! cabalga para presentarse ante el comisario Farach en el pueblo que nombra el
//! archivo de diálogo (`police_town`). Enseguida, en la pulpería de ese pueblo,
//! habla con Bairoletto, que después se levanta y se va ([`see_off`]).
//!
//! Una vez tomado, el caballo es del jugador: F detenido o al paso lento lo
//! hace bajar y lo deja atado flojo donde está ([`LooseHorse`]), y F junto a él
//! lo hace volver a subir. Ninguna de las dos cosas hace avanzar el encargo.
//!
//! Tomar el caballo de Don Hidalgo desbloquea la monta ([`crate::abilities`]):
//! desde entonces F junto a cualquier otro caballo ([`Mountable`]: atado en un
//! pueblo, o parado suelto) lo hace subir, y el que montaba antes queda parado
//! donde lo dejó. Antes de eso, solo el caballo de Don Hidalgo lo deja subir.
//!
//! `pampa-core` es dueño de las reglas ([`Errand`]): quién está dónde, los
//! pasos de la historia y qué está al alcance. Los pasos son datos, en
//! `assets/dialogue/errand.ron` (actos 2 a 4 de `historia`). Este módulo lee
//! la tecla de interacción ([`INTERACT_KEY`]), reproduce la escena de cada
//! paso cumplido: sus líneas como subtítulos ([`Subtitle`]) con el nombre de
//! quien habla, cada una el tiempo de leerla, y sus acciones narradas (la
//! plata de [`PlayerWallet`], las cosas de [`PlayerItems`]). Mientras dura una
//! escena el jugador no maneja ([`ControlLock`]) y la cámara encuadra a los
//! dos que hablan ([`SceneShot`]). También pone al jugador en la montura, y
//! mantiene el objetivo del HUD ([`HudObjective`]) y el objetivo del minimapa
//! ([`MapTarget`]) al ritmo del encargo.

use bevy::prelude::*;
use pampa_content::{ERRAND_FILE, ErrandLines, asset_path, load_errand_or_builtin};
use pampa_core::{
    Ability, Beat, Errand, ErrandEvent, Interaction, MAX_STEP, Npc, Rider, Speaker, TownLayout,
    Travel, World,
};
use pampa_render::character::{GAZE_MAX, Gaze, Rising};
use pampa_render::lettering::{self, Face};
use pampa_render::subtitle;
use pampa_render::{
    Animal, CharacterVisual, Coat, HudObjective, MapTarget, Say, Subtitle, SubtitleLine, Talking,
    VisualPose, WorldTowns, world_from_ground,
};

use crate::abilities::{PlayerAbilities, Unlocked, announce_unlocks, unlock_on_mount};
use crate::camera::SceneShot;
use crate::life::{
    AtTable, Cast, Leaving, LedAway, StandingNpc, TetheredHorse, npc_figure, pulperia_floor,
};
use crate::player::{
    ControlLock, DrivePlayer, GameWorld, Player, RiddenCoat, WalkPath, player_visual_on,
};
use crate::pulperia::PulperiaDoors;
use crate::wallet::{PlayerItems, PlayerWallet};

/// La única tecla de contexto: hablar, montar o bajarse. (E orbita la cámara, M
/// alterna el mapa, Esc sale).
pub const INTERACT_KEY: KeyCode = KeyCode::KeyF;

/// El caballo del que se bajó el jugador, atado flojo donde lo dejó; `None`
/// mientras lo monta (o antes de tomarlo).
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct LooseHorse(pub Option<pampa_core::TetheredHorse>);

/// La entidad que dibuja el [`LooseHorse`].
#[derive(Component, Debug, Clone, Copy)]
pub struct LeftHorse;

/// Un caballo parado al que el jugador puede subir (una vez desbloqueada la
/// monta, ver [`PlayerAbilities`]): atado en un pueblo, dejado suelto, o parado
/// libre. Su huella en planta es un obstáculo del [`GameWorld`] mientras está
/// parado.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Mountable(pub pampa_core::TetheredHorse);

/// El encargo en curso.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct ErrandRun(pub Errand);

/// Las líneas del encargo.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct ErrandDialogue(pub ErrandLines);

/// El encargo avanzó (cada evento se escribe una sola vez).
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrandProgress(pub ErrandEvent);

/// Una escena o un comentario: líneas que se dicen una tras otra, cada una en
/// pantalla el tiempo de leerla ([`subtitle::duration`]), y acciones narradas
/// que suceden en su lugar, al instante.
#[derive(Debug, Clone, PartialEq)]
pub struct Talk {
    pub beats: Vec<Beat>,
    /// Índice del próximo momento.
    pub next: usize,
    /// Segundos hasta el próximo momento.
    pub wait: f32,
    /// Quién dice la línea en pantalla.
    pub speaker: Option<Speaker>,
    /// Con quién es la escena: mientras dura, el jugador no maneja y la cámara
    /// los encuadra. `None` para un comentario suelto del jugador.
    pub partner: Option<Npc>,
}

impl Talk {
    /// La escena `beats` con `partner`.
    pub fn scene(partner: Npc, beats: &[Beat]) -> Self {
        Self {
            partner: Some(partner),
            ..Self::remark(beats.to_vec())
        }
    }

    /// Un comentario suelto: el jugador sigue manejando.
    fn remark(beats: Vec<Beat>) -> Self {
        Self {
            beats,
            next: 0,
            wait: 0.0,
            speaker: None,
            partner: None,
        }
    }

    /// Las `lines` dichas por el protagonista, como comentario suelto.
    fn said_by_me(lines: &[String]) -> Self {
        Self::remark(
            lines
                .iter()
                .map(|text| Beat::Line {
                    speaker: Speaker::Protagonist,
                    text: text.clone(),
                })
                .collect(),
        )
    }
}

/// La conversación en curso, si la hay. Una nueva la reemplaza.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct Conversation(pub Option<Talk>);

/// El encargo de un mundo de `towns`, dado el primer pueblo: Don Hidalgo en su
/// plaza, Farach en el pueblo llamado `police_town` (el
/// [`ErrandLines::police_town`] del diálogo; el otro pueblo más cercano con
/// comisaría cuando ese no tiene una) y Bairoletto sentado en la pulpería de
/// ese pueblo. Las huellas en planta de los que están parados se agregan a
/// `world` como obstáculos. `None` (sin encargo) si no hay un segundo pueblo
/// con comisaría.
pub fn plan(
    towns: &[TownLayout],
    world: &mut World,
    police_town: &str,
) -> Option<(ErrandRun, Cast)> {
    let giver = 0;
    let police = Errand::police_town(towns, giver, police_town)?;
    let errand = Errand::new(towns, giver, police)?;
    world.obstacles.extend(errand.obstacles());
    let cast = Cast::of(&errand);
    Some((ErrandRun(errand), cast))
}

pub struct ErrandPlugin;

impl Plugin for ErrandPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Say>()
            .add_message::<ErrandProgress>()
            .add_message::<Unlocked>()
            .init_resource::<Conversation>()
            .init_resource::<Subtitle>()
            .init_resource::<HudObjective>()
            .init_resource::<pampa_render::HudMoney>()
            .init_resource::<PlayerWallet>()
            .init_resource::<PlayerItems>()
            .init_resource::<ControlLock>()
            .init_resource::<SceneShot>()
            .init_resource::<MapTarget>()
            .init_resource::<LedAway>()
            .init_resource::<LooseHorse>()
            .init_resource::<GameWorld>()
            .init_resource::<PlayerAbilities>()
            .init_resource::<RiddenCoat>()
            .init_resource::<pampa_render::HudAnnouncement>()
            .init_resource::<crate::ride::AnnounceTimer>()
            .add_systems(Startup, (load_dialogue, tell_story).chain())
            .add_systems(
                Update,
                (
                    interact,
                    converse,
                    see_off,
                    frame_scene,
                    mark_speakers,
                    show_progress,
                    crate::wallet::show_money,
                    unlock_on_mount,
                    announce_unlocks,
                )
                    .chain()
                    .after(DrivePlayer),
            );
    }
}

/// Lo que encontró la lectura del archivo de diálogo, registrado en el log al
/// inicio (el juego lo lee antes de que el log esté listo, para planificar el
/// encargo).
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct DialogueReport {
    pub info: Vec<String>,
    pub warnings: Vec<String>,
}

/// Lee `assets/dialogue/errand.ron` y recurre a las líneas incorporadas si
/// falla, con lo que hay que registrar al respecto.
pub fn read_dialogue() -> (ErrandDialogue, DialogueReport) {
    let path = asset_path(ERRAND_FILE);
    let (lines, error) = load_errand_or_builtin(&path);
    let mut report = DialogueReport::default();
    match error {
        Some(error) => report.warnings.push(format!(
            "errand dialogue: {error}; using the built-in lines"
        )),
        None => report
            .info
            .push(format!("errand dialogue loaded from {}", path.display())),
    }
    let unsupported = lines.unsupported_chars(|c| lettering::supports(Face::Body, c));
    if !unsupported.is_empty() {
        report.warnings.push(format!(
            "errand dialogue uses characters the font cannot draw: {unsupported:?}"
        ));
    }
    (ErrandDialogue(lines), report)
}

/// Registra el [`DialogueReport`] en el log, leyendo primero el diálogo salvo
/// que las líneas ya se hayan proporcionado.
fn load_dialogue(
    mut commands: Commands,
    existing: Option<Res<ErrandDialogue>>,
    report: Option<Res<DialogueReport>>,
) {
    let report = match existing {
        Some(_) => report.map(|r| r.clone()).unwrap_or_default(),
        None => {
            let (lines, report) = read_dialogue();
            commands.insert_resource(lines);
            report
        }
    };
    report.info.iter().for_each(|line| info!("{line}"));
    report.warnings.iter().for_each(|line| warn!("{line}"));
}

/// Le da al encargo los pasos de la historia del archivo de diálogo, desde el
/// primero.
fn tell_story(errand: Option<ResMut<ErrandRun>>, dialogue: Option<Res<ErrandDialogue>>) {
    if let (Some(mut errand), Some(dialogue)) = (errand, dialogue) {
        errand.0 = errand.0.clone().with_story(dialogue.0.steps.clone());
    }
}

/// Lo que [`interact`] cambia cuando el jugador monta o se baja.
type MountState<'w> = (
    ResMut<'w, GameWorld>,
    ResMut<'w, LedAway>,
    ResMut<'w, LooseHorse>,
    ResMut<'w, RiddenCoat>,
    Res<'w, PlayerAbilities>,
);

/// Un caballo parado tal como lo ve [`interact`].
type StandingHorse = (
    Entity,
    &'static Mountable,
    Option<&'static CharacterVisual>,
    Option<&'static Animal>,
    Option<&'static TetheredHorse>,
    Has<LeftHorse>,
);

/// El pelaje de un caballo parado: el visual de uno ensillado, o el de un
/// caballo de corral (dibujado como fauna).
fn standing_coat(visual: Option<&CharacterVisual>, animal: Option<&Animal>) -> Option<Coat> {
    match (visual, animal) {
        (Some(CharacterVisual::Horse(coat)), _) | (_, Some(Animal::Horse(coat))) => Some(*coat),
        _ => None,
    }
}

/// Los caballos que [`interact`] puede cambiar: los atados en los pueblos, el
/// que el jugador dejó atado flojo, y cada caballo parado.
type Horses<'w, 's> = (
    Query<'w, 's, (Entity, &'static TetheredHorse)>,
    Query<'w, 's, Entity, With<LeftHorse>>,
    Query<'w, 's, StandingHorse>,
);

/// Si la persona del encargo al alcance de `at` toma la F.
pub(crate) fn errand_wants_f(errand: &Errand, at: pampa_core::Vec2) -> bool {
    errand.clone().interact(at) != Interaction::Nothing
}

/// Lo que reclama la F antes que un cartel o una puerta: la persona del
/// encargo, o un caballo al alcance que el jugador puede montar.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct FClaims<'w, 's> {
    errand: Option<Res<'w, ErrandRun>>,
    loose: Option<Res<'w, LooseHorse>>,
    abilities: Option<Res<'w, PlayerAbilities>>,
    horses: Query<'w, 's, &'static Mountable>,
}

impl FClaims<'_, '_> {
    /// Si alguno de ellos toma la F de un jugador parado en `at`.
    pub(crate) fn taken(&self, at: pampa_core::Vec2) -> bool {
        let horse_in_reach = self
            .loose
            .as_ref()
            .and_then(|l| l.0)
            .is_some_and(|h| at.distance(h.position) <= Errand::MOUNT_RANGE)
            || self
                .abilities
                .as_ref()
                .is_some_and(|a| a.0.has(Ability::Ride))
                && self
                    .horses
                    .iter()
                    .any(|h| at.distance(h.0.position) <= Errand::MOUNT_RANGE);
        horse_in_reach
            || self
                .errand
                .as_ref()
                .is_some_and(|e| errand_wants_f(&e.0, at))
    }
}

/// La tecla de interacción: habla con quien esté al alcance y le corresponda, o
/// monta el caballo de Don Hidalgo; sin nada más que hacer, baja del caballo o
/// vuelve a subir, o (una vez desbloqueada la monta) sube a cualquier otro
/// caballo al alcance.
#[allow(clippy::too_many_arguments)]
pub(crate) fn interact(
    mut commands: Commands,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    errand: Option<ResMut<ErrandRun>>,
    dialogue: Option<Res<ErrandDialogue>>,
    mut conversation: ResMut<Conversation>,
    (mut world, mut led_away, mut loose, mut coat, abilities): MountState,
    mut players: Query<(Entity, &mut Player, &mut Transform)>,
    (horses, left, standing): Horses,
    mut progress: MessageWriter<ErrandProgress>,
) {
    // Durante una escena la tecla no hace nada.
    let in_scene = conversation.0.as_ref().is_some_and(|t| t.partner.is_some());
    if in_scene || !keys.is_some_and(|k| k.just_pressed(INTERACT_KEY)) {
        return;
    }
    let Ok((me, mut player, mut transform)) = players.single_mut() else {
        return;
    };
    let mut saddle = Saddle {
        commands: &mut commands,
        world: &mut world.0,
        loose: &mut loose,
        led_away: &mut led_away,
        coat: &mut coat.0,
        left: left.iter().collect(),
    };
    let ride_any = abilities.0.has(Ability::Ride);
    let swap_or_take = |saddle: &mut Saddle, rider: &mut Rider, transform: &mut Transform| {
        if !saddle.swap(me, rider, transform) && ride_any {
            saddle.take(me, rider, transform, standing.iter());
        }
    };
    let (Some(mut errand), Some(dialogue)) = (errand, dialogue) else {
        swap_or_take(&mut saddle, &mut player.rider, &mut transform);
        return;
    };
    let lines = &dialogue.0;
    let event = match errand.0.interact(player.rider.position) {
        Interaction::Nothing => {
            swap_or_take(&mut saddle, &mut player.rider, &mut transform);
            return;
        }
        Interaction::NotYourHorse => {
            conversation.0 = Some(Talk::said_by_me(&lines.not_your_horse));
            return;
        }
        Interaction::Event(event) => event,
    };
    progress.write(ErrandProgress(event));
    match event {
        ErrandEvent::Talked(npc) => {
            conversation.0 = Some(Talk::scene(npc, errand.0.last_scene()));
        }
        ErrandEvent::MountedHorse => {
            let (town, horse) = errand.0.horse();
            // Se lo llevan por el resto de la sesión: el caballo atado y su
            // huella en planta desaparecen, y el jugador lo monta desde donde
            // estaba.
            saddle.world.obstacles.remove(&horse.footprint);
            saddle.led_away.0.insert(town);
            for (entity, tied) in &horses {
                if tied.0 == town {
                    saddle.commands.entity(entity).despawn();
                }
            }
            *saddle.coat = Coat::HIDALGO;
            saddle.ride(me, &mut player.rider, &mut transform, &horse);
        }
    }
}

/// Lo que cambia al subir y bajar del caballo.
struct Saddle<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    world: &'a mut World,
    loose: &'a mut LooseHorse,
    led_away: &'a mut LedAway,
    /// El pelaje del caballo montado.
    coat: &'a mut Coat,
    /// Las entidades que dibujan el caballo suelto.
    left: Vec<Entity>,
}

impl Saddle<'_, '_, '_> {
    /// Pone a `rider` (la entidad `me`) en la montura de `horse`, donde está
    /// parado, con el visual de montado.
    fn ride(
        &mut self,
        me: Entity,
        rider: &mut Rider,
        transform: &mut Transform,
        horse: &pampa_core::TetheredHorse,
    ) {
        *rider = rider.mount(horse, &*self.world);
        transform.translation = world_from_ground(rider.position, rider.height);
        // Un visual nuevo: render descarta la figura y dibuja al jinete.
        self.commands
            .entity(me)
            .insert(player_visual_on(rider.travel(), *self.coat));
    }

    /// Se baja del caballo cuando cabalga lo bastante lento y lo deja atado
    /// flojo (un caballo quieto y un obstáculo), o vuelve a subir a él desde
    /// dentro de [`Errand::MOUNT_RANGE`]. `false` si no hizo ninguna de las dos
    /// cosas.
    fn swap(&mut self, me: Entity, rider: &mut Rider, transform: &mut Transform) -> bool {
        match (rider.travel(), self.loose.0) {
            (Travel::Riding, _) => {
                let Some((person, horse)) = rider.dismount(&*self.world) else {
                    return true;
                };
                *rider = person;
                transform.translation = world_from_ground(rider.position, rider.height);
                self.commands
                    .entity(me)
                    .insert(player_visual_on(Travel::OnFoot, *self.coat));
                self.world.obstacles.push(horse.footprint);
                self.loose.0 = Some(horse);
                let height = self.world.ground_height(horse.position);
                self.commands.spawn((
                    Name::new("Loose horse"),
                    LeftHorse,
                    Mountable(horse),
                    CharacterVisual::Horse(*self.coat),
                    VisualPose {
                        heading: horse.heading,
                        ..default()
                    },
                    Transform::from_translation(world_from_ground(horse.position, height)),
                ));
                true
            }
            (Travel::OnFoot, Some(horse))
                if rider.position.distance(horse.position) <= Errand::MOUNT_RANGE =>
            {
                self.world.obstacles.remove(&horse.footprint);
                self.loose.0 = None;
                for &entity in &self.left {
                    self.commands.entity(entity).despawn();
                }
                self.ride(me, rider, transform, &horse);
                true
            }
            (Travel::OnFoot, _) => false,
        }
    }

    /// Sube al más cercano de los `horses` dentro de [`Errand::MOUNT_RANGE`]
    /// del jugador a pie: su huella en planta desaparece, un caballo atado en
    /// un pueblo se lo llevan para siempre, y un caballo suelto dejado antes
    /// queda parado (ya no es el suelto).
    fn take<'h>(
        &mut self,
        me: Entity,
        rider: &mut Rider,
        transform: &mut Transform,
        horses: impl Iterator<
            Item = (
                Entity,
                &'h Mountable,
                Option<&'h CharacterVisual>,
                Option<&'h Animal>,
                Option<&'h TetheredHorse>,
                bool,
            ),
        >,
    ) {
        if rider.travel() != Travel::OnFoot {
            return;
        }
        let at = rider.position;
        let nearest = horses
            .map(|(e, m, visual, animal, tied, left)| {
                (
                    e,
                    m.0,
                    standing_coat(visual, animal),
                    tied.map(|t| t.0),
                    left,
                )
            })
            .filter(|(_, h, ..)| at.distance(h.position) <= Errand::MOUNT_RANGE)
            .min_by(|a, b| {
                at.distance(a.1.position)
                    .total_cmp(&at.distance(b.1.position))
            });
        let Some((entity, horse, coat, town, _)) = nearest else {
            return;
        };
        self.world.obstacles.remove(&horse.footprint);
        if let Some(town) = town {
            self.led_away.0.insert(town);
        }
        // El caballo que haya quedado suelto queda donde está parado.
        self.loose.0 = None;
        for &e in &self.left {
            self.commands.entity(e).remove::<LeftHorse>();
        }
        self.commands.entity(entity).despawn();
        if let Some(coat) = coat {
            *self.coat = coat;
        }
        self.ride(me, rider, transform, &horse);
    }
}

/// Reproduce la conversación: muestra sus líneas una tras otra como
/// subtítulos, cada una el tiempo de leerla, aplica sus acciones a la plata y a
/// las cosas del protagonista al llegar a ellas, y borra el subtítulo cuando
/// termina.
fn converse(
    time: Res<Time>,
    mut conversation: ResMut<Conversation>,
    mut subtitle: ResMut<Subtitle>,
    (mut wallet, mut items): (ResMut<PlayerWallet>, ResMut<PlayerItems>),
) {
    let Some(talk) = conversation.0.as_mut() else {
        return;
    };
    talk.wait -= time.delta_secs().min(MAX_STEP);
    if talk.wait > 0.0 {
        return;
    }
    loop {
        let Some(beat) = talk.beats.get(talk.next).cloned() else {
            conversation.0 = None;
            subtitle.0 = None;
            return;
        };
        talk.next += 1;
        match beat {
            Beat::Act(action) => action.apply(&mut wallet.0, &mut items.0),
            Beat::Line { speaker, text } => {
                talk.wait = subtitle::duration(&text);
                talk.speaker = Some(speaker);
                subtitle.0 = Some(SubtitleLine {
                    speaker: speaker.name().to_string(),
                    text,
                });
                return;
            }
        }
    }
}

/// Terminada su escena, quien esperaba sentado en la pulpería ([`Cast`]:
/// Bairoletto) se para del banco y se va ([`Leaving`]): sale del elenco, así que no
/// vuelve cuando su pueblo se vuelve a cargar.
fn see_off(
    mut commands: Commands,
    (errand, conversation, towns): (
        Option<Res<ErrandRun>>,
        Res<Conversation>,
        Option<Res<WorldTowns>>,
    ),
    cast: Option<ResMut<Cast>>,
    world: Res<GameWorld>,
    npcs: Query<(Entity, &StandingNpc)>,
) {
    let (Some(errand), Some(mut cast)) = (errand, cast) else {
        return;
    };
    let partner = conversation.0.as_ref().and_then(|t| t.partner);
    let (done, waiting): (Vec<AtTable>, Vec<AtTable>) = cast
        .1
        .iter()
        .partition(|a| errand.0.talked_to(a.npc) && partner != Some(a.npc));
    if done.is_empty() {
        return;
    }
    cast.1 = waiting;
    for sitter in done {
        let pulperia = towns
            .as_ref()
            .and_then(|t| t.0.get(sitter.town)?.pulperia());
        for (entity, npc) in &npcs {
            if npc.npc != sitter.npc {
                continue;
            }
            let mut him = commands.entity(entity);
            him.remove::<(StandingNpc, Gaze)>();
            match &pulperia {
                Some((store, inside)) => {
                    him.insert((
                        Leaving {
                            town: sitter.town,
                            figure: npc_figure(sitter.npc),
                            way: inside.way_out(sitter.seat),
                            next: 1,
                            room: inside.floor,
                            floor: pulperia_floor(store, &world),
                        },
                        Rising(0.0),
                    ));
                }
                None => {
                    him.despawn();
                }
            }
        }
    }
}

/// Mientras se reproduce una escena, el jugador no maneja y la cámara encuadra
/// a quien habla con él; fuera de una escena, todo vuelve a la normalidad.
fn frame_scene(
    conversation: Res<Conversation>,
    errand: Option<Res<ErrandRun>>,
    mut lock: ResMut<ControlLock>,
    mut shot: ResMut<SceneShot>,
) {
    let partner = conversation.0.as_ref().and_then(|t| t.partner);
    let at = partner
        .zip(errand)
        .map(|(npc, errand)| errand.0.standing(npc).position);
    lock.set_if_neq(ControlLock(partner.is_some()));
    shot.set_if_neq(SceneShot(at));
}

/// Pone [`Talking`] a quien tenga sus líneas en pantalla y se lo quita a todos
/// los demás: parados, hablan con las manos.
fn mark_speakers(
    mut commands: Commands,
    conversation: Res<Conversation>,
    players: Query<Entity, With<Player>>,
    npcs: Query<(Entity, &StandingNpc)>,
    talking: Query<Entity, With<Talking>>,
) {
    let speaker = conversation
        .0
        .as_ref()
        .and_then(|talk| talk.speaker)
        .and_then(|speaker| match speaker {
            Speaker::Protagonist => players.iter().next(),
            Speaker::Npc(npc) => npcs.iter().find(|(_, n)| n.npc == npc).map(|(e, _)| e),
        });
    for entity in &talking {
        if Some(entity) != speaker {
            commands.entity(entity).remove::<Talking>();
        }
    }
    if let Some(speaker) = speaker
        && !talking.contains(speaker)
    {
        commands.entity(speaker).insert(Talking);
    }
}

/// Mantiene el objetivo del HUD y el objetivo del minimapa en la etapa actual.
fn show_progress(
    errand: Option<Res<ErrandRun>>,
    mut objective: ResMut<HudObjective>,
    mut target: ResMut<MapTarget>,
) {
    let text = errand.as_ref().and_then(|e| e.0.objective());
    objective.set_if_neq(HudObjective(text));
    target.set_if_neq(MapTarget(errand.and_then(|e| e.0.target())));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ride::testing::arrive;
    use pampa_core::Vec2 as GroundVec2;
    use pampa_core::{ErrandStage, Item, Money, Terrain, Travel};
    use pampa_render::character::{FigureBody, FigureJoint, FigurePart, HorseBody, HorseModel};
    use pampa_render::{CharacterVisual, CharacterVisualPlugin, Coat, FigureKind, WorldTowns};

    /// Cada subtítulo mostrado, en orden.
    #[derive(Resource, Default)]
    struct Said(Vec<SubtitleLine>);

    #[derive(Resource, Default)]
    struct Progress(Vec<ErrandEvent>);

    fn log(
        subtitle: Res<Subtitle>,
        mut progress: MessageReader<ErrandProgress>,
        mut said: ResMut<Said>,
        mut events: ResMut<Progress>,
    ) {
        if subtitle.is_changed()
            && let Some(line) = &subtitle.0
        {
            said.0.push(line.clone());
        }
        events.0.extend(progress.read().map(|p| p.0));
    }

    fn shipped_lines() -> ErrandLines {
        let (lines, error) = load_errand_or_builtin(&asset_path(ERRAND_FILE));
        assert!(error.is_none(), "{error:?}");
        lines
    }

    /// La apertura del juego en el mundo incluido (a bordo del tren de la
    /// mañana desde Pico entrando a Monte Nievas), con cada línea y cada evento
    /// del encargo registrados. Los frames duran 50 ms.
    fn shipped_app() -> App {
        let mut app = crate::ride::testing::shipped_app();
        app.init_resource::<Said>()
            .init_resource::<Progress>()
            .add_systems(PostUpdate, log);
        app
    }

    fn player(app: &mut App) -> (Entity, Player) {
        let mut q = app.world_mut().query::<(Entity, &Player)>();
        let (e, p) = q.single(app.world()).unwrap();
        (e, Player { rider: p.rider })
    }

    fn teleport(app: &mut App, to: GroundVec2) {
        let mut q = app.world_mut().query::<&mut Player>();
        let mut p = q.single_mut(app.world_mut()).unwrap();
        p.rider.position = to;
        p.rider.speed = 0.0;
    }

    /// Cuerpos de caballo, cuerpos de figura, articulaciones de figura, partes
    /// de figura y modelos de caballo dibujados bajo `who`, y meshes que no son
    /// parte de ninguna figura.
    fn drawn_under(app: &mut App, who: Entity) -> [usize; 6] {
        let mut under = Vec::new();
        let mut stack = vec![who];
        while let Some(e) = stack.pop() {
            if let Some(children) = app.world().get::<Children>(e) {
                stack.extend(children.iter());
                under.extend(children.iter());
            }
        }
        let world = app.world();
        let n = |has: &dyn Fn(Entity) -> bool| under.iter().filter(|e| has(**e)).count();
        [
            n(&|e| world.get::<HorseBody>(e).is_some()),
            n(&|e| world.get::<FigureBody>(e).is_some()),
            n(&|e| world.get::<FigureJoint>(e).is_some()),
            n(&|e| world.get::<FigurePart>(e).is_some()),
            n(&|e| world.get::<HorseModel>(e).is_some()),
            n(&|e| world.get::<Mesh3d>(e).is_some() && world.get::<FigurePart>(e).is_none()),
        ]
    }

    #[test]
    fn mounting_swaps_the_clerk_figure_for_one_horse_and_rider_and_nothing_else() {
        let mut app = shipped_app();
        app.init_resource::<Assets<Image>>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .add_plugins(CharacterVisualPlugin);
        app.world_mut()
            .spawn((Camera3d::default(), Transform::default()));
        arrive(&mut app);
        let e = errand(&app);
        let (me, _) = player(&mut app);
        assert_eq!(
            *app.world().get::<CharacterVisual>(me).unwrap(),
            CharacterVisual::Figure(FigureKind::Protagonist)
        );
        let [horses, bodies, joints, parts, _, other] = drawn_under(&mut app, me);
        assert_eq!((horses, bodies, joints, other), (0, 1, 14, 0));
        assert!(parts >= 18, "{parts}");

        teleport(
            &mut app,
            e.standing(Npc::Hidalgo).position + e.standing(Npc::Hidalgo).heading.forward() * 2.0,
        );
        press_f(&mut app);
        wait_for_scene(&mut app, 0);
        teleport(&mut app, beside(&e.horse().1));
        press_f(&mut app);
        assert_eq!(player(&mut app).1.rider.travel(), Travel::Riding);
        // Un caballo con el empleado en su montura, y nada más: no queda ningún
        // empleado a pie, nada rígido. (Esta app no carga modelos: el caballo y
        // su jinete esperan los suyos sin dibujar nada).
        let [horses, bodies, joints, parts, models, other] = drawn_under(&mut app, me);
        assert_eq!(
            (horses, bodies, joints, parts, models, other),
            (1, 0, 0, 0, 1, 0)
        );
        let children: Vec<Entity> = app.world().get::<Children>(me).unwrap().to_vec();
        assert_eq!(children.len(), 1, "only the horse and its rider");
        let horse = app.world().get::<HorseBody>(children[0]).unwrap();
        assert_eq!(horse.coat, Coat::Alazan, "Don Hidalgo's alazán");
        assert_eq!(
            horse.rider,
            Some(FigureKind::Protagonist),
            "the clerk rides it"
        );
        let mut figures = app.world_mut().query::<&FigureBody>();
        assert!(
            figures
                .iter(app.world())
                .all(|b| b.kind != FigureKind::Protagonist),
            "no clerk on foot"
        );
        // El caballo atado que tomó tampoco está ya en el almacén.
        let mut tied = app.world_mut().query::<&TetheredHorse>();
        assert!(tied.iter(app.world()).all(|h| h.0 != e.horse().0));
    }

    /// Presiona la tecla de interacción durante exactamente un frame.
    fn press_f(app: &mut App) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(INTERACT_KEY);
        app.update();
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(INTERACT_KEY);
        keys.clear();
    }

    fn errand(app: &App) -> Errand {
        app.world().resource::<ErrandRun>().0.clone()
    }

    fn npc_entity(app: &mut App, npc: Npc) -> Entity {
        let mut q = app.world_mut().query::<(Entity, &StandingNpc)>();
        q.iter(app.world())
            .find(|(_, n)| n.npc == npc)
            .map(|(e, _)| e)
            .expect("standing in town")
    }

    fn objective(app: &App) -> Option<String> {
        app.world().resource::<HudObjective>().0.clone()
    }

    /// Las líneas mostradas como subtítulo con el nombre de `speaker`.
    fn said_by(app: &App, speaker: &str) -> Vec<String> {
        let said = &app.world().resource::<Said>().0;
        said.iter()
            .filter(|l| l.speaker == speaker)
            .map(|l| l.text.clone())
            .collect()
    }

    /// Las líneas de la escena del paso `step` de la historia incluida, con el
    /// nombre de quien las dice.
    fn scene_lines(step: usize) -> Vec<(String, String)> {
        shipped_lines().steps[step]
            .scene
            .iter()
            .filter_map(|beat| match beat {
                Beat::Line { speaker, text } => Some((speaker.name().to_string(), text.clone())),
                Beat::Act(_) => None,
            })
            .collect()
    }

    /// Cada subtítulo mostrado, como (nombre, texto).
    fn said(app: &App) -> Vec<(String, String)> {
        let said = &app.world().resource::<Said>().0;
        said.iter()
            .map(|l| (l.speaker.clone(), l.text.clone()))
            .collect()
    }

    /// Deja que se reproduzca toda la escena del paso `step` de la historia
    /// incluida, y comprueba que el subtítulo se borró al terminar.
    fn wait_for_scene(app: &mut App, step: usize) {
        let lines: Vec<String> = scene_lines(step).into_iter().map(|(_, t)| t).collect();
        wait_for_lines(app, &lines);
    }

    /// Deja que se reproduzcan todas las `lines` de una conversación, y
    /// comprueba que el subtítulo se borró al terminar.
    fn wait_for_lines(app: &mut App, lines: &[String]) {
        let seconds: f32 = lines.iter().map(|l| subtitle::duration(l)).sum();
        let frames = ((seconds + 1.0) / 0.05).ceil() as usize;
        for _ in 0..frames {
            app.update();
        }
        assert_eq!(app.world().resource::<Conversation>().0, None);
        assert_eq!(app.world().resource::<Subtitle>().0, None);
    }

    #[test]
    fn whoever_speaks_talks_with_their_hands_until_their_lines_end() {
        let mut app = shipped_app();
        arrive(&mut app);
        let e = errand(&app);
        let talking = |app: &mut App| {
            let mut q = app.world_mut().query_filtered::<Entity, With<Talking>>();
            q.iter(app.world()).collect::<Vec<_>>()
        };
        assert_eq!(talking(&mut app), vec![]);
        teleport(
            &mut app,
            e.standing(Npc::Hidalgo).position + e.standing(Npc::Hidalgo).heading.forward() * 2.0,
        );
        press_f(&mut app);
        app.update();
        let don = npc_entity(&mut app, Npc::Hidalgo);
        assert_eq!(talking(&mut app), vec![don]);
        wait_for_scene(&mut app, 0);
        app.update();
        assert_eq!(talking(&mut app), vec![]);
    }

    #[test]
    fn the_whole_errand_from_the_train_to_the_comisaria() {
        let mut app = shipped_app();
        let lines = shipped_lines();
        // Bajado del tren de la mañana en Monte Nievas, sin un peso a la vista.
        arrive(&mut app);
        assert_eq!(
            app.world()
                .resource::<pampa_render::HudMoney>()
                .0
                .as_deref(),
            Some("$ 0,00")
        );
        let e = errand(&app);
        let hidalgo = e.standing(Npc::Hidalgo).position;
        let (horse_town, horse) = e.horse();
        let farach = e.standing(Npc::Farach).position;

        // A pie, arriba del andén de Monte Nievas, junto a los coches del tren
        // detenido, mirando al pueblo; quedarse quieto deja al jugador donde
        // está.
        let (start, facing) = {
            let (_, p) = player(&mut app);
            (p.rider.position, p.rider.heading)
        };
        let town = app.world().resource::<WorldTowns>().0[0].clone();
        assert_eq!(facing, town.platform_spot().unwrap().1);
        for _ in 0..10 {
            app.update();
        }
        let (me, p) = player(&mut app);
        assert_eq!(p.rider.travel(), Travel::OnFoot);
        assert!(
            p.rider.position.distance(start) < 1e-4,
            "{:?}",
            p.rider.position
        );
        // Arriba, sobre el andén, no en el suelo junto a él.
        let world = app.world().resource::<GameWorld>().0.clone();
        assert_eq!(p.rider.height, world.ground_height(start));
        assert!(p.rider.height > world.terrain.height(start.x, start.y) + 0.5);
        assert_eq!(objective(&app), e.objective());
        assert_eq!(
            objective(&app).as_deref(),
            Some("Buscá a Don Hidalgo en la plaza")
        );
        assert_eq!(app.world().resource::<MapTarget>().0, Some(hidalgo));

        // Nada que hacer en el camino de entrada al pueblo.
        teleport(&mut app, town.arrival().0);
        press_f(&mut app);
        assert_eq!(errand(&app).stage(), ErrandStage::MeetHidalgo);
        assert!(app.world().resource::<Said>().0.is_empty());

        // Todavía no es su caballo.
        teleport(&mut app, beside(&horse));
        press_f(&mut app);
        assert_eq!(errand(&app).stage(), ErrandStage::MeetHidalgo);
        assert_eq!(said_by(&app, "Lisandro"), lines.not_your_horse[..1]);
        assert_eq!(player(&mut app).1.rider.travel(), Travel::OnFoot);
        wait_for_lines(&mut app, &lines.not_your_horse);

        // Don Hidalgo, en la plaza, cuenta toda la historia, línea por línea.
        teleport(
            &mut app,
            hidalgo + e.standing(Npc::Hidalgo).heading.forward() * 2.0,
        );
        press_f(&mut app);
        assert_eq!(errand(&app).stage(), ErrandStage::TakeHorse);
        assert_eq!(objective(&app), errand(&app).objective());
        assert!(objective(&app).unwrap().contains("caballo"));
        assert_eq!(app.world().resource::<MapTarget>().0, Some(horse.position));
        wait_for_scene(&mut app, 0);
        let hidalgos: Vec<String> = scene_lines(0).into_iter().map(|(_, t)| t).collect();
        assert_eq!(said_by(&app, "Don Hidalgo"), hidalgos);
        press_f(&mut app);
        assert_eq!(
            said_by(&app, "Don Hidalgo").len(),
            hidalgos.len(),
            "said once"
        );

        // Montar su caballo: el jugador pasa a ser el jinete; el caballo atado
        // y su huella en planta desaparecen.
        teleport(&mut app, beside(&horse));
        press_f(&mut app);
        assert_eq!(errand(&app).stage(), ErrandStage::Talk(Npc::Farach));
        let (_, p) = player(&mut app);
        assert_eq!(p.rider.travel(), Travel::Riding);
        assert_eq!(
            (p.rider.position, p.rider.heading),
            (horse.position, horse.heading)
        );
        let visual = *app.world().get::<CharacterVisual>(me).unwrap();
        assert_eq!(
            visual,
            CharacterVisual::Mounted {
                rider: FigureKind::Protagonist,
                coat: Coat::HIDALGO,
            }
        );
        assert!(
            !app.world()
                .resource::<GameWorld>()
                .0
                .obstacles
                .contains(&horse.footprint)
        );
        assert!(app.world().resource::<LedAway>().0.contains(&horse_town));
        let mut tied = app.world_mut().query::<&TetheredHorse>();
        assert!(tied.iter(app.world()).all(|h| h.0 != horse_town));
        assert_eq!(app.world().resource::<MapTarget>().0, Some(farach));
        assert_eq!(objective(&app), errand(&app).objective());
        assert!(
            objective(&app)
                .unwrap()
                .ends_with("Farach en Eduardo Castex")
        );

        // Cabalgar hasta Castex y presentarse, sin bajarse.
        // A 2.5 m frente a él, en la calle.
        teleport(
            &mut app,
            farach + e.standing(Npc::Farach).heading.forward() * 2.5,
        );
        press_f(&mut app);
        assert_eq!(errand(&app).stage(), ErrandStage::Talk(Npc::Bairoletto));
        wait_for_scene(&mut app, 2);
        assert!(said(&app).ends_with(&scene_lines(2)));
        let tano = errand(&app).standing(Npc::Bairoletto).position;
        assert_eq!(
            objective(&app).as_deref(),
            Some("Andá a la pulpería La Colonia")
        );
        assert_eq!(app.world().resource::<MapTarget>().0, Some(tano));

        // A pie, en la pulpería de enfrente: Bairoletto, y después nada más.
        press_f(&mut app);
        assert_eq!(player(&mut app).1.rider.travel(), Travel::OnFoot);
        let castex = &app.world().resource::<WorldTowns>().0[e.standing(Npc::Bairoletto).town];
        let (_, inside) = castex.pulperia().unwrap();
        let aisle = inside.way_out(e.bairoletto_seat().unwrap())[2];
        teleport(&mut app, aisle);
        press_f(&mut app);
        assert_eq!(errand(&app).stage(), ErrandStage::Done);
        wait_for_scene(&mut app, 3);
        assert!(said(&app).ends_with(&scene_lines(3)));
        assert_eq!(objective(&app), None);
        assert_eq!(app.world().resource::<MapTarget>().0, None);

        // Cada paso ocurrió una sola vez.
        press_f(&mut app);
        assert_eq!(
            app.world().resource::<Progress>().0,
            [
                ErrandEvent::Talked(Npc::Hidalgo),
                ErrandEvent::MountedHorse,
                ErrandEvent::Talked(Npc::Farach),
                ErrandEvent::Talked(Npc::Bairoletto)
            ]
        );
    }

    /// Lleva la historia hasta frente a Farach: habla con Don Hidalgo, espera
    /// su escena y monta su caballo.
    fn ride_to_farach(app: &mut App) {
        arrive(app);
        let e = errand(app);
        let hidalgo = e.standing(Npc::Hidalgo);
        teleport(app, hidalgo.position + hidalgo.heading.forward() * 2.0);
        press_f(app);
        wait_for_scene(app, 0);
        teleport(app, beside(&e.horse().1));
        press_f(app);
        let farach = e.standing(Npc::Farach);
        teleport(app, farach.position + farach.heading.forward() * 2.5);
    }

    fn wallet(app: &App) -> Money {
        app.world().resource::<PlayerWallet>().0
    }

    fn has_letter(app: &App) -> bool {
        app.world().resource::<PlayerItems>().0.has(Item::Letter)
    }

    #[test]
    fn don_hidalgo_hands_over_three_pesos_and_his_letter() {
        let mut app = shipped_app();
        arrive(&mut app);
        assert_eq!((wallet(&app), has_letter(&app)), (Money::ZERO, false));
        let hidalgo = errand(&app).standing(Npc::Hidalgo);
        teleport(&mut app, hidalgo.position + hidalgo.heading.forward() * 2.0);
        press_f(&mut app);
        wait_for_scene(&mut app, 0);
        assert_eq!(wallet(&app), Money::new(3, 0));
        assert!(has_letter(&app));
        assert_eq!(
            app.world()
                .resource::<pampa_render::HudMoney>()
                .0
                .as_deref(),
            Some("$ 3,00")
        );
    }

    #[test]
    fn at_the_comisaria_lisandro_and_farach_speak_act_three_word_for_word() {
        let mut app = shipped_app();
        ride_to_farach(&mut app);
        let before = said(&app).len();
        press_f(&mut app);
        wait_for_scene(&mut app, 2);
        let all = said(&app);
        let lines: Vec<(&str, &str)> = all[before..]
            .iter()
            .map(|(who, text)| (who.as_str(), text.as_str()))
            .collect();
        assert_eq!(
            lines,
            [
                (
                    "Lisandro",
                    "Buenas, Comisario, le traigo unos papeles que me envió mi tío pa’ uste’"
                ),
                ("Farach", "¿y vos quien sos?"),
                (
                    "Lisandro",
                    "Soy el sobrino de Don Hidalgo, llegué hoy mismo al pueblo."
                ),
                (
                    "Farach",
                    "Ah el viejo cascarrabias ese, si ya se… a ver pasame tus papeles."
                ),
                (
                    "Farach",
                    "Lisandro Benjamin Andrada, nacido en General Pico, desempleado, zoncera, zoncera…"
                ),
                (
                    "Farach",
                    "Bueno Pibe, te aviso que en esta ciudad mando yo,"
                ),
                (
                    "Farach",
                    "así que si no queres tener problema no te hagas el guapo y no andès holgazaneando,"
                ),
                ("Farach", "que a los vagos como vos los tengo cortitos."),
                (
                    "Farach",
                    "Quedate cerca que a la noche te devuelvo tus papeles firmados."
                ),
            ]
        );
        // Enseguida, el acto 4: Bairoletto espera en la pulpería.
        assert_eq!(errand(&app).stage(), ErrandStage::Talk(Npc::Bairoletto));
    }

    /// El mundo incluido con el pueblo `town` activo y el resto no, como si el
    /// jugador estuviera parado en `at`.
    fn activate_around(app: &mut App, at: GroundVec2) {
        let towns = app.world().resource::<WorldTowns>().0.clone();
        let mut activation = pampa_core::TownActivation::default();
        activation.update(&towns, at);
        app.insert_resource(pampa_render::ActiveTowns(activation));
    }

    #[test]
    fn in_la_colonia_bairoletto_speaks_act_four_word_for_word_then_gets_up_and_leaves() {
        let mut app = shipped_app();
        ride_to_farach(&mut app);
        press_f(&mut app);
        wait_for_scene(&mut app, 2);
        let e = errand(&app);
        let him = e.standing(Npc::Bairoletto);
        let towns = app.world().resource::<WorldTowns>().0.clone();
        let (store, inside) = towns[him.town].pulperia().expect("Castex has a pulpería");
        assert_eq!(him.town, e.standing(Npc::Farach).town);
        let seat = e.bairoletto_seat().unwrap();
        // Ya no está en la puerta de la comisaría: espera sentado en su mesa,
        // y el HUD y el mapa mandan ahí.
        assert_eq!(
            objective(&app).as_deref(),
            Some("Andá a la pulpería La Colonia")
        );
        assert_eq!(app.world().resource::<MapTarget>().0, Some(him.position));
        let tano = npc_entity(&mut app, Npc::Bairoletto);
        assert_eq!(
            app.world().get::<CharacterVisual>(tano),
            Some(&CharacterVisual::Seated(FigureKind::Bairoletto))
        );
        let at = app.world().get::<Transform>(tano).unwrap().translation;
        assert_eq!(GroundVec2::new(at.x, -at.z), inside.seats[seat].position);
        let mut patrons = app.world_mut().query::<&crate::life::Patron>();
        for p in patrons.iter(app.world()).filter(|p| p.town == him.town) {
            assert_ne!(inside.seats[p.seat].table, inside.seats[seat].table);
        }

        // Desde el paso que viene de la puerta, junto a su banco: la escena,
        // con su cámara sobre él.
        teleport(&mut app, inside.way_out(seat)[2]);
        let before = said(&app).len();
        press_f(&mut app);
        app.update();
        assert_eq!(app.world().resource::<SceneShot>().0, Some(him.position));
        wait_for_scene(&mut app, 3);
        let all = said(&app);
        let lines: Vec<(&str, &str)> = all[before..]
            .iter()
            .map(|(who, text)| (who.as_str(), text.as_str()))
            .collect();
        assert_eq!(
            lines,
            [
                (
                    "Bairoletto",
                    "Che pibe, ese caballo que está afuera…¿Es tuyo?"
                ),
                ("Lisandro", "Si, es de mi tìo."),
                (
                    "Bairoletto",
                    "¿Queres ganarte unos pesos? Necesito transporte para esta noche."
                ),
                ("Lisandro", "si ¿hasta donde necesita ir?"),
                ("Bairoletto", "A la noche te digo ¿Cómo te llamas?"),
                ("Lisandro", "Lisandro."),
                (
                    "Bairoletto",
                    "Yo soy Bairoletto, pero me dicen Tano. Nos vemos a la noche en esta pulpería."
                ),
            ]
        );
        // Libre hasta la noche: no queda nada que hacer.
        assert_eq!(errand(&app).stage(), ErrandStage::Done);
        assert_eq!(objective(&app), None);
        assert_eq!(app.world().resource::<MapTarget>().0, None);

        // Se levanta del banco, sin moverse de su lugar.
        app.update();
        let mut standing = app.world_mut().query::<&StandingNpc>();
        assert!(standing.iter(app.world()).all(|n| n.npc != Npc::Bairoletto));
        let on_bench = app.world().get::<Transform>(tano).unwrap().translation;
        let mut risen = 0.0;
        while let Some(rising) = app.world().get::<pampa_render::character::Rising>(tano) {
            assert!(rising.0 >= risen, "{} after {risen}", rising.0);
            risen = rising.0;
            assert_eq!(
                app.world().get::<CharacterVisual>(tano),
                Some(&CharacterVisual::Seated(FigureKind::Bairoletto))
            );
            assert_eq!(
                app.world().get::<Transform>(tano).unwrap().translation,
                on_bench
            );
            assert!(risen < 1.0, "{risen}");
            app.update();
        }
        assert!(risen > 0.9, "he stands up gradually, last at {risen}");
        // Sale caminando por la puerta, sigue por la vereda, se difumina al
        // final (S16: ya no desaparece de golpe) y se va.
        let walking = |app: &mut App| {
            let mut q = app.world_mut().query::<(
                &crate::life::Leaving,
                &CharacterVisual,
                &Transform,
                &VisualPose,
                Option<&pampa_render::character::Fading>,
            )>();
            q.iter(app.world())
                .map(|(l, v, t, p, f)| (l.town, *v, t.translation, *p, f.map(|f| f.0)))
                .collect::<Vec<_>>()
        };
        let (mut fading, mut seen) = (0, 1.0_f32);
        let world = app.world().resource::<GameWorld>().0.clone();
        let middle = store.footprint.center();
        let floor = world.terrain.height(middle.x, middle.y) + pampa_core::pulperia::FLOOR_RISE;
        let way = inside.way_out(seat);
        let length: f32 = way.windows(2).map(|w| w[0].distance(w[1])).sum();
        let (mut out_the_door, mut gone) = (false, false);
        let (mut was, mut facing, mut steps) = (None::<(GroundVec2, f32)>, 0, 0);
        let frames = ((length / crate::life::LEAVING_PACE + 2.0) / 0.05) as usize;
        for _ in 0..frames {
            match walking(&mut app)[..] {
                [] => {
                    gone = true;
                    break;
                }
                [(town, visual, feet, pose, fade)] => {
                    let at = GroundVec2::new(feet.x, -feet.z);
                    if let Some(fade) = fade {
                        assert!(fade <= seen && fade < 1.0, "{fade} after {seen}");
                        seen = fade;
                        fading += 1;
                    }
                    assert_eq!(town, him.town);
                    assert_eq!(visual, CharacterVisual::Figure(FigureKind::Bairoletto));
                    // Camina con el ciclo de caminata, con los pies en el
                    // piso: el de ladrillo adentro, el suelo afuera.
                    assert_eq!(pose.animation, pampa_core::AnimationState::Walk);
                    assert!(pose.speed > 0.5, "{}", pose.speed);
                    let ground = if inside.floor.contains_point(at) {
                        floor
                    } else {
                        world.ground_height(at)
                    };
                    assert!(
                        (feet.y - ground).abs() < 1e-4,
                        "{at:?}: {} over {ground}",
                        feet.y
                    );
                    out_the_door |= inside.doorway().contains_point(at);
                    // Gira hacia su camino sin saltos, y camina mirando
                    // hacia adelante.
                    let heading = pose.heading.radians();
                    if let Some((before, turned)) = was {
                        use std::f32::consts::{PI, TAU};
                        let turn = (heading - turned + PI).rem_euclid(TAU) - PI;
                        let max = crate::life::LEAVING_TURN_RATE * 0.05 + 1e-3;
                        assert!(turn.abs() <= max, "a turn of {turn} in one frame");
                        if at.distance(before) > 1e-3 {
                            steps += 1;
                            let step = (at - before) * (1.0 / at.distance(before));
                            let ahead = pose.heading.forward().dot(step);
                            facing += usize::from(ahead > 0.99);
                        }
                    }
                    was = Some((at, heading));
                    // Nunca atraviesa una pared de la pulpería.
                    assert!(!inside.walls.iter().any(|w| w.contains_point(at)), "{at:?}");
                }
                ref more => panic!("{more:?}"),
            }
            app.update();
        }
        assert!(out_the_door && gone, "{out_the_door} {gone}");
        // Se difumina durante el último segundo y medio, hasta casi no verse.
        let faded = fading as f32 * 0.05;
        assert!((1.3..=1.7).contains(&faded), "faded for {faded} s");
        assert!(seen < 0.1, "last seen at {seen}");
        assert!(
            facing * 10 > steps * 9,
            "facing ahead {facing} of {steps} steps"
        );
        assert!(store.footprint.contains_point(way[2]));

        // No vuelve cuando el pueblo se descarga y se vuelve a cargar.
        activate_around(&mut app, GroundVec2::new(1.0e6, 1.0e6));
        app.update();
        activate_around(&mut app, him.position);
        for _ in 0..3 {
            app.update();
        }
        let mut standing = app.world_mut().query::<&StandingNpc>();
        let back: Vec<Npc> = standing.iter(app.world()).map(|n| n.npc).collect();
        assert!(back.contains(&Npc::Farach), "the town is back: {back:?}");
        assert!(!back.contains(&Npc::Bairoletto), "{back:?}");
        assert!(walking(&mut app).is_empty());
    }

    #[test]
    fn farach_keeps_the_letter_once_it_is_handed_over() {
        let mut app = shipped_app();
        ride_to_farach(&mut app);
        assert!(has_letter(&app));
        press_f(&mut app);
        // Las dos primeras líneas: todavía la lleva.
        let lines = scene_lines(2);
        let first: Vec<String> = lines[..2].iter().map(|(_, t)| t.clone()).collect();
        let seconds: f32 = first.iter().map(|l| subtitle::duration(l)).sum();
        for _ in 0..((seconds - 0.5) / 0.05) as usize {
            app.update();
        }
        assert!(has_letter(&app), "not yet handed over");
        wait_for_scene(&mut app, 2);
        assert!(!has_letter(&app));
        assert_eq!(wallet(&app), Money::new(3, 0), "the money stays");
    }

    #[test]
    fn a_scene_holds_the_player_still_and_frames_the_speaker_until_it_ends() {
        let mut app = shipped_app();
        arrive(&mut app);
        let hidalgo = errand(&app).standing(Npc::Hidalgo);
        let shot = |app: &App| app.world().resource::<SceneShot>().0;
        assert_eq!(shot(&app), None);
        teleport(&mut app, hidalgo.position + hidalgo.heading.forward() * 2.0);
        press_f(&mut app);
        app.update();
        assert_eq!(shot(&app), Some(hidalgo.position));
        // W no lo mueve mientras habla Don Hidalgo.
        let at = player(&mut app).1.rider.position;
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        for _ in 0..20 {
            app.update();
        }
        assert!(player(&mut app).1.rider.position.distance(at) < 1e-3);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::KeyW);
        wait_for_scene(&mut app, 0);
        app.update();
        assert_eq!(shot(&app), None);
        // Terminada la escena, W vuelve a moverlo.
        let at = player(&mut app).1.rider.position;
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        for _ in 0..20 {
            app.update();
        }
        assert!(player(&mut app).1.rider.position.distance(at) > 0.5);
    }

    /// La lógica del caballo suelto y sus entidades dibujadas.
    fn loose(app: &mut App) -> (Option<pampa_core::TetheredHorse>, Vec<CharacterVisual>) {
        let horse = app.world().resource::<LooseHorse>().0;
        let mut q = app
            .world_mut()
            .query_filtered::<&CharacterVisual, With<LeftHorse>>();
        (horse, q.iter(app.world()).copied().collect())
    }

    #[test]
    fn getting_off_and_back_on_keeps_the_errand_on_track() {
        let mut app = shipped_app();
        arrive(&mut app);
        let e = errand(&app);
        let hidalgo = e.standing(Npc::Hidalgo);
        teleport(&mut app, hidalgo.position + hidalgo.heading.forward() * 2.0);
        press_f(&mut app);
        wait_for_scene(&mut app, 0);
        teleport(&mut app, beside(&e.horse().1));
        press_f(&mut app);
        let (me, p) = player(&mut app);
        assert_eq!(p.rider.travel(), Travel::Riding);
        let report = objective(&app);
        assert!(
            report
                .as_deref()
                .is_some_and(|o| o.contains("Eduardo Castex"))
        );

        // No mientras sigue cabalgando.
        app.world_mut()
            .query::<&mut Player>()
            .single_mut(app.world_mut())
            .unwrap()
            .rider
            .speed = 5.0;
        press_f(&mut app);
        assert_eq!(player(&mut app).1.rider.travel(), Travel::Riding);
        assert_eq!(loose(&mut app), (None, vec![]));

        // Detenido: se baja, a pie junto al caballo, que queda atado flojo.
        let farach = e.standing(Npc::Farach);
        let street = farach.position + farach.heading.forward() * 12.0;
        teleport(&mut app, street);
        app.update();
        let at = player(&mut app).1.rider;
        press_f(&mut app);
        let (_, p) = player(&mut app);
        assert_eq!(p.rider.travel(), Travel::OnFoot);
        assert_eq!(
            *app.world().get::<CharacterVisual>(me).unwrap(),
            CharacterVisual::Figure(FigureKind::Protagonist)
        );
        let (horse, drawn) = loose(&mut app);
        let horse = horse.expect("a loose horse");
        assert_eq!((horse.position, horse.heading), (at.position, at.heading));
        assert_eq!(drawn, vec![CharacterVisual::Horse(Coat::HIDALGO)]);
        assert!(
            app.world()
                .resource::<GameWorld>()
                .0
                .obstacles
                .contains(&horse.footprint)
        );
        assert!(p.rider.position.distance(horse.position) <= Errand::MOUNT_RANGE);
        // El encargo no avanzó: falta presentarse, nada que tomar.
        assert_eq!(errand(&app).stage(), ErrandStage::Talk(Npc::Farach));
        assert_eq!(objective(&app), report);

        // F junto a él: de vuelta en la montura, el caballo suelto desaparece.
        press_f(&mut app);
        let (_, p) = player(&mut app);
        assert_eq!(p.rider.travel(), Travel::Riding);
        assert_eq!(p.rider.position, horse.position);
        assert_eq!(loose(&mut app), (None, vec![]));
        assert!(
            !app.world()
                .resource::<GameWorld>()
                .0
                .obstacles
                .contains(&horse.footprint)
        );

        // Bajarse otra vez, presentarse a pie, y seguir cabalgando después del
        // encargo.
        press_f(&mut app);
        assert_eq!(player(&mut app).1.rider.travel(), Travel::OnFoot);
        teleport(&mut app, farach.position + farach.heading.forward() * 2.5);
        press_f(&mut app);
        assert_eq!(errand(&app).stage(), ErrandStage::Talk(Npc::Bairoletto));
        wait_for_scene(&mut app, 2);
        let (horse, _) = loose(&mut app);
        let horse = horse.unwrap();
        teleport(&mut app, horse.position + horse.heading.forward() * 2.0);
        press_f(&mut app);
        assert_eq!(player(&mut app).1.rider.travel(), Travel::Riding);
        assert_eq!(
            app.world().resource::<Progress>().0,
            [
                ErrandEvent::Talked(Npc::Hidalgo),
                ErrandEvent::MountedHorse,
                ErrandEvent::Talked(Npc::Farach)
            ]
        );
    }

    #[test]
    fn hidalgos_horse_unlocks_riding_and_then_any_horse_lets_him_on() {
        use pampa_core::{Abilities, Ability};
        let mut app = shipped_app();
        arrive(&mut app);
        let e = errand(&app);
        let ride = |app: &App| {
            app.world()
                .resource::<PlayerAbilities>()
                .0
                .has(Ability::Ride)
        };
        assert!(!ride(&app));
        // Un caballo atado en un pueblo que no es ni el de Don Hidalgo ni el de
        // Farach.
        let other = {
            let towns = &app.world().resource::<WorldTowns>().0;
            let police = e.standing(Npc::Farach).town;
            (0..towns.len())
                .find(|&t| t != e.horse().0 && t != police)
                .map(|t| (t, towns[t].hitching_post.horse))
                .expect("a third town")
        };
        teleport(&mut app, beside(&other.1));
        press_f(&mut app);
        assert_eq!(player(&mut app).1.rider.travel(), Travel::OnFoot, "not yet");

        // El caballo de Don Hidalgo: se desbloquea la monta, con un aviso.
        let hidalgo = e.standing(Npc::Hidalgo);
        teleport(&mut app, hidalgo.position + hidalgo.heading.forward() * 2.0);
        press_f(&mut app);
        wait_for_scene(&mut app, 0);
        teleport(&mut app, beside(&e.horse().1));
        press_f(&mut app);
        assert!(ride(&app));
        assert_eq!(
            app.world()
                .resource::<pampa_render::HudAnnouncement>()
                .0
                .as_deref(),
            Some(Ability::Ride.notice())
        );
        assert_ne!(
            app.world().resource::<PlayerAbilities>().0,
            Abilities::all(),
            "only riding"
        );
        // Se baja donde lo tomó: queda atado flojo.
        press_f(&mut app);
        let (left, _) = loose(&mut app);
        let left = left.expect("left loose");

        // Ahora el caballo del otro pueblo lo deja subir, con su propio pelaje,
        // y se lo llevan para siempre; el de Don Hidalgo queda parado donde lo
        // dejó.
        teleport(&mut app, beside(&other.1));
        press_f(&mut app);
        let (_, p) = player(&mut app);
        assert_eq!(p.rider.travel(), Travel::Riding);
        assert_eq!(p.rider.position, other.1.position);
        assert!(app.world().resource::<LedAway>().0.contains(&other.0));
        let mut tied = app.world_mut().query::<&TetheredHorse>();
        assert!(tied.iter(app.world()).all(|h| h.0 != other.0));
        let obstacles = &app.world().resource::<GameWorld>().0.obstacles;
        assert!(!obstacles.contains(&other.1.footprint));
        assert!(obstacles.contains(&left.footprint));
        let coat = crate::life::tethered_coat(
            &app.world().resource::<WorldTowns>().0[other.0],
            app.world().resource::<Cast>(),
            other.0,
        );
        assert_eq!(app.world().resource::<RiddenCoat>().0, coat);
        let (me, _) = player(&mut app);
        assert_eq!(
            *app.world().get::<CharacterVisual>(me).unwrap(),
            player_visual_on(Travel::Riding, coat)
        );
        assert_eq!(loose(&mut app), (None, vec![]));
        let mut standing = app.world_mut().query::<&Mountable>();
        assert!(standing.iter(app.world()).any(|m| m.0 == left));

        // Se baja de este y vuelve al alazán de Don Hidalgo (el antiguo caballo
        // suelto ahora es un caballo parado cualquiera).
        press_f(&mut app);
        teleport(&mut app, left.position + left.heading.forward() * 2.0);
        press_f(&mut app);
        assert_eq!(player(&mut app).1.rider.position, left.position);
        assert_eq!(app.world().resource::<RiddenCoat>().0, Coat::HIDALGO);
    }

    /// A 2.5 m delante del caballo atado a lo largo de la calle: apartado del
    /// caballo y de su palenque, al alcance.
    fn beside(horse: &pampa_core::TetheredHorse) -> GroundVec2 {
        horse.position + horse.heading.forward() * 2.5
    }

    #[test]
    fn the_shipped_lines_only_use_characters_the_font_draws() {
        for lines in [shipped_lines(), ErrandLines::builtin()] {
            assert_eq!(
                lines.unsupported_chars(|c| lettering::supports(Face::Body, c)),
                Vec::<char>::new()
            );
        }
    }

    #[test]
    fn there_is_an_errand_in_the_shipped_world_from_monte_nievas_to_castex() {
        let loaded = crate::shipped_world::loaded();
        let mut world = World::with_towns(loaded.world, Terrain::default(), &loaded.towns);
        let before = world.obstacles.len();
        let lines = shipped_lines();
        let (run, cast) = plan(&loaded.towns, &mut world, &lines.police_town).unwrap();
        let hidalgo = run.0.standing(Npc::Hidalgo);
        let farach = run.0.standing(Npc::Farach);
        assert_eq!(loaded.towns[hidalgo.town].name, "Monte Nievas");
        assert_eq!(loaded.towns[farach.town].name, lines.police_town);
        assert_eq!(run.0.police_name(), "Eduardo Castex");
        // Como dice la historia: no hay comisaría en Monte Nievas, hay una en
        // Castex.
        let comisarias = |t: &TownLayout| {
            t.places()
                .iter()
                .filter(|p| p.kind == pampa_core::PlaceKind::PoliceStation)
                .count()
        };
        assert_eq!(comisarias(&loaded.towns[hidalgo.town]), 0);
        assert_eq!(loaded.towns[hidalgo.town].police_door(), None);
        assert_eq!(comisarias(&loaded.towns[farach.town]), 1);
        // Hidalgo y Farach parados, apartados de Castex; Bairoletto sentado
        // en la pulpería de Castex, sobre su banco.
        assert_eq!(world.obstacles.len(), before + 2);
        assert_eq!(cast.0, vec![(Npc::Hidalgo, hidalgo), (Npc::Farach, farach)]);
        assert_eq!(
            cast.1,
            vec![crate::life::AtTable {
                npc: Npc::Bairoletto,
                town: farach.town,
                seat: run.0.bairoletto_seat().unwrap()
            }]
        );
    }

    #[test]
    fn missing_dialogue_is_loaded_at_startup() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(ErrandPlugin);
        app.update();
        let dialogue = app.world().resource::<ErrandDialogue>();
        assert!(!dialogue.0.steps.is_empty());
        // Sin encargo no hay objetivo ni destino.
        assert_eq!(app.world().resource::<HudObjective>().0, None);
        assert_eq!(app.world().resource::<MapTarget>().0, None);
    }

    #[test]
    fn a_corral_horse_drawn_as_fauna_lets_the_player_on_in_its_coat() {
        use pampa_core::{Abilities, GameClock, Heading, TimeOfDay};
        use pampa_render::{Animal, AnimalPose};
        let mut app =
            crate::ride::testing::shipped_app_at(Some(GameClock::at(TimeOfDay::hm(12, 0))));
        app.update();
        // En plena pampa, lejos del andén.
        teleport(&mut app, GroundVec2::new(100.0, -200.0));
        app.update();
        let at = player(&mut app).1.rider.position + GroundVec2::new(0.0, 2.0);
        let horse = pampa_core::TetheredHorse::standing(at, Heading::default());
        let corral = app
            .world_mut()
            .spawn((
                Mountable(horse),
                Animal::Horse(Coat::Tordillo),
                AnimalPose::default(),
            ))
            .id();
        // Todavía no le corresponde montarlo.
        press_f(&mut app);
        assert_eq!(player(&mut app).1.rider.travel(), Travel::OnFoot);
        app.world_mut().resource_mut::<PlayerAbilities>().0 = Abilities::all();
        press_f(&mut app);
        assert_eq!(player(&mut app).1.rider.travel(), Travel::Riding);
        assert_eq!(app.world().resource::<RiddenCoat>().0, Coat::Tordillo);
        assert!(
            app.world().get_entity(corral).is_err(),
            "taken from the corral"
        );
    }
}
