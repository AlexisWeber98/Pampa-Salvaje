//! El primer encargo, en juego: el jugador se baja a pie del tren de la mañana
//! en el primer pueblo (la apertura, ver [`crate::ride`]), habla con Don
//! Hidalgo en la plaza, toma su caballo del almacén de ramos generales y
//! cabalga para presentarse ante el comisario Farach en el pueblo que nombra el
//! archivo de diálogo (`police_town`).
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
//! `pampa-core` es dueño de las reglas ([`Errand`]): quién está dónde, las
//! etapas y qué está al alcance. Este módulo lee la tecla de interacción
//! ([`INTERACT_KEY`]), reproduce las líneas de `assets/dialogue/errand.ron`
//! como globos de diálogo ([`Say`]), una por cada duración de globo, pone al
//! jugador en la montura, y mantiene el objetivo del HUD ([`HudObjective`]) y
//! el objetivo del minimapa ([`MapTarget`]) al ritmo del encargo.

use bevy::prelude::*;
use pampa_content::{ERRAND_FILE, ErrandLines, asset_path, load_errand_or_builtin};
use pampa_core::{
    Ability, Errand, ErrandEvent, Interaction, MAX_STEP, Npc, Rider, TownLayout, Travel, World,
};
use pampa_render::bubble::LIFETIME;
use pampa_render::lettering::{self, Face};
use pampa_render::{
    Animal, CharacterVisual, Coat, FigureKind, HudObjective, MapTarget, Say, Talking, VisualPose,
    world_from_ground,
};

use crate::abilities::{PlayerAbilities, Unlocked, announce_unlocks, unlock_on_mount};
use crate::life::{Cast, LedAway, StandingNpc, TetheredHorse};
use crate::player::{DrivePlayer, GameWorld, Player, RiddenCoat, player_visual_on};

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

/// Quién dice una línea.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speaker {
    Npc(Npc),
    Player,
}

/// Líneas que se dicen una tras otra, cada una mostrada durante la duración de
/// un globo.
#[derive(Debug, Clone, PartialEq)]
pub struct Talk {
    pub speaker: Speaker,
    pub lines: Vec<String>,
    /// Índice de la próxima línea a decir.
    pub next: usize,
    /// Segundos hasta la próxima línea.
    pub wait: f32,
}

impl Talk {
    fn new(speaker: Speaker, lines: Vec<String>) -> Self {
        Self {
            speaker,
            lines,
            next: 0,
            wait: 0.0,
        }
    }
}

/// La conversación en curso, si la hay. Una nueva la reemplaza.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct Conversation(pub Option<Talk>);

/// El encargo de un mundo de `towns`, dado el primer pueblo: Don Hidalgo en su
/// plaza, Farach (con Bairoletto a su lado) en el pueblo llamado `police_town`
/// (el [`ErrandLines::police_town`] del diálogo; el otro pueblo más cercano con
/// comisaría cuando ese no tiene una). Sus huellas en planta se agregan a
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
    let cast = Cast(
        [Npc::Hidalgo, Npc::Farach]
            .map(|npc| (npc, errand.standing(npc)))
            .to_vec(),
        vec![(FigureKind::Bairoletto, errand.bairoletto())],
    );
    Some((ErrandRun(errand), cast))
}

pub struct ErrandPlugin;

impl Plugin for ErrandPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Say>()
            .add_message::<ErrandProgress>()
            .add_message::<Unlocked>()
            .init_resource::<Conversation>()
            .init_resource::<HudObjective>()
            .init_resource::<MapTarget>()
            .init_resource::<LedAway>()
            .init_resource::<LooseHorse>()
            .init_resource::<GameWorld>()
            .init_resource::<PlayerAbilities>()
            .init_resource::<RiddenCoat>()
            .init_resource::<pampa_render::HudAnnouncement>()
            .init_resource::<crate::ride::AnnounceTimer>()
            .add_systems(Startup, load_dialogue)
            .add_systems(
                Update,
                (
                    interact,
                    converse,
                    mark_speakers,
                    show_progress,
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
    if !keys.is_some_and(|k| k.just_pressed(INTERACT_KEY)) {
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
            conversation.0 = Some(Talk::new(Speaker::Player, lines.not_your_horse.clone()));
            return;
        }
        Interaction::Event(event) => event,
    };
    progress.write(ErrandProgress(event));
    match event {
        ErrandEvent::TalkedToHidalgo => {
            conversation.0 = Some(Talk::new(Speaker::Npc(Npc::Hidalgo), lines.hidalgo.clone()));
        }
        ErrandEvent::ReportedToFarach => {
            conversation.0 = Some(Talk::new(Speaker::Npc(Npc::Farach), lines.farach.clone()));
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

/// Dice las líneas de la conversación una tras otra, cada una durante la
/// duración de un globo, sobre quien habla (si está cerca).
fn converse(
    time: Res<Time>,
    mut conversation: ResMut<Conversation>,
    players: Query<Entity, With<Player>>,
    npcs: Query<(Entity, &StandingNpc)>,
    mut say: MessageWriter<Say>,
) {
    let Some(talk) = conversation.0.as_mut() else {
        return;
    };
    talk.wait -= time.delta_secs().min(MAX_STEP);
    if talk.wait > 0.0 {
        return;
    }
    let Some(line) = talk.lines.get(talk.next).cloned() else {
        conversation.0 = None;
        return;
    };
    talk.next += 1;
    talk.wait = LIFETIME;
    let speaker = match talk.speaker {
        Speaker::Player => players.iter().next(),
        Speaker::Npc(npc) => npcs.iter().find(|(_, n)| n.npc == npc).map(|(e, _)| e),
    };
    if let Some(speaker) = speaker {
        say.write(Say { speaker, line });
    }
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
    let speaker = conversation.0.as_ref().and_then(|talk| match talk.speaker {
        Speaker::Player => players.iter().next(),
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
    use pampa_core::{ErrandStage, Terrain, Travel};
    use pampa_render::character::{FigureBody, FigureJoint, FigurePart, HorseBody, HorseModel};
    use pampa_render::{CharacterVisual, CharacterVisualPlugin, Coat, FigureKind, WorldTowns};

    /// Cada línea dicha, y por quién.
    #[derive(Resource, Default)]
    struct Said(Vec<(Entity, String)>);

    #[derive(Resource, Default)]
    struct Progress(Vec<ErrandEvent>);

    fn log(
        mut says: MessageReader<Say>,
        mut progress: MessageReader<ErrandProgress>,
        mut said: ResMut<Said>,
        mut events: ResMut<Progress>,
    ) {
        said.0
            .extend(says.read().map(|s| (s.speaker, s.line.clone())));
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
        assert_eq!(horse.coat, Coat::Bayo, "Don Hidalgo's bayo");
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

    fn said_by(app: &App, speaker: Entity) -> Vec<String> {
        let said = &app.world().resource::<Said>().0;
        said.iter()
            .filter(|(e, _)| *e == speaker)
            .map(|(_, l)| l.clone())
            .collect()
    }

    /// Deja que se reproduzcan todas las líneas de una conversación.
    fn wait_for_lines(app: &mut App, lines: usize) {
        let frames = ((lines as f32 + 1.0) * LIFETIME / 0.05).ceil() as usize;
        for _ in 0..frames {
            app.update();
        }
        assert_eq!(app.world().resource::<Conversation>().0, None);
    }

    #[test]
    fn whoever_speaks_talks_with_their_hands_until_their_lines_end() {
        let mut app = shipped_app();
        let lines = shipped_lines();
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
        wait_for_lines(&mut app, lines.hidalgo.len());
        app.update();
        assert_eq!(talking(&mut app), vec![]);
    }

    #[test]
    fn the_whole_errand_from_the_train_to_the_comisaria() {
        let mut app = shipped_app();
        let lines = shipped_lines();
        // Bajado del tren de la mañana en Monte Nievas.
        arrive(&mut app);
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
        assert_eq!(said_by(&app, me), lines.not_your_horse[..1]);
        assert_eq!(player(&mut app).1.rider.travel(), Travel::OnFoot);

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
        wait_for_lines(&mut app, lines.hidalgo.len());
        let don = npc_entity(&mut app, Npc::Hidalgo);
        assert_eq!(said_by(&app, don), lines.hidalgo);
        press_f(&mut app);
        assert_eq!(said_by(&app, don).len(), lines.hidalgo.len(), "said once");

        // Montar su caballo: el jugador pasa a ser el jinete; el caballo atado
        // y su huella en planta desaparecen.
        teleport(&mut app, beside(&horse));
        press_f(&mut app);
        assert_eq!(errand(&app).stage(), ErrandStage::ReportToFarach);
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
                coat: Coat::Bayo,
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
        assert_eq!(errand(&app).stage(), ErrandStage::Done);
        wait_for_lines(&mut app, lines.farach.len());
        let comisario = npc_entity(&mut app, Npc::Farach);
        assert_eq!(said_by(&app, comisario), lines.farach);
        assert_eq!(objective(&app), None);
        assert_eq!(app.world().resource::<MapTarget>().0, None);

        // Cada paso ocurrió una sola vez.
        press_f(&mut app);
        assert_eq!(
            app.world().resource::<Progress>().0,
            [
                ErrandEvent::TalkedToHidalgo,
                ErrandEvent::MountedHorse,
                ErrandEvent::ReportedToFarach
            ]
        );
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
        assert_eq!(drawn, vec![CharacterVisual::Horse(Coat::Bayo)]);
        assert!(
            app.world()
                .resource::<GameWorld>()
                .0
                .obstacles
                .contains(&horse.footprint)
        );
        assert!(p.rider.position.distance(horse.position) <= Errand::MOUNT_RANGE);
        // El encargo no avanzó: falta presentarse, nada que tomar.
        assert_eq!(errand(&app).stage(), ErrandStage::ReportToFarach);
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
        assert_eq!(errand(&app).stage(), ErrandStage::Done);
        let (horse, _) = loose(&mut app);
        let horse = horse.unwrap();
        teleport(&mut app, horse.position + horse.heading.forward() * 2.0);
        press_f(&mut app);
        assert_eq!(player(&mut app).1.rider.travel(), Travel::Riding);
        assert_eq!(
            app.world().resource::<Progress>().0,
            [
                ErrandEvent::TalkedToHidalgo,
                ErrandEvent::MountedHorse,
                ErrandEvent::ReportedToFarach
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

        // Se baja de este y vuelve al bayo de Don Hidalgo (el antiguo caballo
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
        // Hidalgo, Farach y Bairoletto a su lado, apartados de Castex.
        assert_eq!(world.obstacles.len(), before + 3);
        assert_eq!(cast.0, vec![(Npc::Hidalgo, hidalgo), (Npc::Farach, farach)]);
        let bairoletto = run.0.bairoletto();
        assert_eq!(cast.1, vec![(FigureKind::Bairoletto, bairoletto)]);
        for o in loaded.towns[farach.town].obstacles() {
            assert!(
                !o.intersects(&bairoletto.footprint()),
                "Bairoletto inside {o:?}"
            );
        }
    }

    #[test]
    fn missing_dialogue_is_loaded_at_startup() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(ErrandPlugin);
        app.update();
        let dialogue = app.world().resource::<ErrandDialogue>();
        assert!(!dialogue.0.hidalgo.is_empty());
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
