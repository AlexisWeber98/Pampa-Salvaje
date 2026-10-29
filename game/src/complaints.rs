//! Los vecinos se quejan cuando el jinete los choca.
//!
//! [`crate::life`] informa a quién empujó el jinete y a qué velocidad iba
//! ([`Shoved`]); `pampa-core` decide qué empujones merecen una queja
//! ([`BumpTracker`]) y qué línea decir ([`LinePicker`]); las líneas vienen de
//! `assets/dialogue/bumps.ron` a través de `pampa-content`; render las muestra
//! como globos de diálogo ([`Say`]).

use bevy::prelude::*;
use pampa_content::{BUMPS_FILE, BumpLines, asset_path, load_bumps_or_builtin};
use pampa_core::{BumpSeverity, BumpTracker, LinePicker, MAX_STEP};
use pampa_render::Say;
use pampa_render::lettering::{self, Face};

use crate::life::{Shoved, Walker, jostle_crowd};
use crate::player::{DrivePlayer, PlayerTuning};

/// Semilla para las líneas que eligen los vecinos.
const LINE_SEED: u32 = 1931;

/// Las líneas de diálogo que dicen los vecinos.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct Dialogue {
    pub bumps: BumpLines,
}

/// Quién puede volver a quejarse (por pueblo, según el índice del pueblo), y
/// qué línea se dijo por última vez.
#[derive(Resource, Debug, Clone)]
pub struct Complaints {
    pub trackers: Vec<BumpTracker>,
    pub picker: LinePicker,
}

impl Default for Complaints {
    fn default() -> Self {
        Self {
            trackers: Vec::new(),
            picker: LinePicker::new(LINE_SEED),
        }
    }
}

pub struct ComplaintsPlugin;

impl Plugin for ComplaintsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Shoved>()
            .add_message::<Say>()
            .init_resource::<Complaints>()
            .add_systems(Startup, load_dialogue)
            .add_systems(Update, complain.in_set(DrivePlayer).after(jostle_crowd));
    }
}

/// Lee `assets/dialogue/bumps.ron` salvo que el diálogo ya se haya
/// proporcionado, y recurre a las líneas incorporadas con un aviso.
fn load_dialogue(mut commands: Commands, existing: Option<Res<Dialogue>>) {
    if existing.is_some() {
        return;
    }
    let path = asset_path(BUMPS_FILE);
    let (bumps, error) = load_bumps_or_builtin(&path);
    match error {
        Some(error) => warn!("bump dialogue: {error}; using the built-in lines"),
        None => info!("bump dialogue loaded from {}", path.display()),
    }
    let unsupported = bumps.unsupported_chars(|c| lettering::supports(Face::Body, c));
    if !unsupported.is_empty() {
        warn!("bump dialogue uses characters the font cannot draw: {unsupported:?}");
    }
    commands.insert_resource(Dialogue { bumps });
}

/// Convierte los empujones de este frame en quejas sobre la cabeza de los
/// peatones.
fn complain(
    time: Res<Time>,
    mut shoved: MessageReader<Shoved>,
    dialogue: Option<Res<Dialogue>>,
    tuning: Res<PlayerTuning>,
    mut complaints: ResMut<Complaints>,
    walkers: Query<(Entity, &Walker)>,
    mut say: MessageWriter<Say>,
) {
    let complaints = &mut *complaints;
    let dt = time.delta_secs().min(MAX_STEP);
    for tracker in &mut complaints.trackers {
        tracker.tick(dt);
    }
    for shove in shoved.read() {
        if complaints.trackers.len() <= shove.town {
            complaints
                .trackers
                .resize_with(shove.town + 1, BumpTracker::default);
        }
        let events =
            complaints.trackers[shove.town].register(&shove.walkers, shove.speed, &tuning.0);
        let Some(dialogue) = dialogue.as_deref() else {
            continue;
        };
        for event in events {
            let speaker = Walker {
                town: shove.town,
                index: event.walker,
            };
            let Some((speaker, _)) = walkers.iter().find(|(_, w)| **w == speaker) else {
                continue;
            };
            let pool = dialogue.bumps.pool(event.severity);
            if let Some(index) = complaints.picker.pick(pool_id(event.severity), pool.len()) {
                say.write(Say {
                    speaker,
                    line: pool[index].clone(),
                });
            }
        }
    }
}

/// Índice del grupo de una gravedad para el [`LinePicker`].
fn pool_id(severity: BumpSeverity) -> usize {
    match severity {
        BumpSeverity::Walk => 0,
        BumpSeverity::Gallop => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::life::{Crowds, TownLifePlugin};
    use crate::player::{GameWorld, Player, PlayerPlugin};
    use bevy::time::TimeUpdateStrategy;
    use pampa_core::{RiderParams, StreetGraph, Townsfolk, Vec2 as GroundVec2, World};
    use pampa_render::{SpeechBubble, SpeechBubblePlugin};
    use std::time::Duration;

    const WALK: [&str; 2] = ["¡Ojo, che!", "¡Despacito!"];
    const GALLOP: [&str; 2] = ["¡Bruto!", "¡Casi me pisa!"];

    /// Cada línea dicha, en orden.
    #[derive(Resource, Default)]
    struct Said(Vec<String>);

    /// Cuántas veces el jinete empujó a alguien.
    #[derive(Resource, Default)]
    struct Shoves(usize);

    fn log_says(mut says: MessageReader<Say>, mut said: ResMut<Said>) {
        said.0.extend(says.read().map(|s| s.line.clone()));
    }

    fn log_shoves(mut shoved: MessageReader<Shoved>, mut shoves: ResMut<Shoves>) {
        shoves.0 += shoved.read().count();
    }

    /// Una persona sola de pie en campo abierto, y el jinete. Los frames duran
    /// 50 ms.
    fn app(tuning: RiderParams) -> App {
        let graph = StreetGraph {
            nodes: vec![GroundVec2::ZERO],
            links: vec![vec![]],
        };
        let own = |lines: [&str; 2]| lines.map(String::from).to_vec();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                50,
            )))
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Assets<Image>>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .insert_resource(Crowds(vec![Townsfolk::from_graph(graph, 1, 1)]))
            .insert_resource(GameWorld(World::default()))
            .insert_resource(PlayerTuning(tuning))
            .insert_resource(Dialogue {
                bumps: BumpLines {
                    walk: own(WALK),
                    gallop: own(GALLOP),
                },
            })
            .init_resource::<Said>()
            .init_resource::<Shoves>()
            .insert_resource(crate::player::PlayerSpawn {
                position: GroundVec2::new(50.0, 50.0),
                heading: pampa_core::Heading::from_radians(0.0),
                ..default()
            })
            .add_plugins((
                PlayerPlugin,
                TownLifePlugin,
                ComplaintsPlugin,
                SpeechBubblePlugin,
            ))
            .add_systems(PostUpdate, (log_says, log_shoves));
        app.update();
        app
    }

    fn person(app: &App) -> GroundVec2 {
        app.world().resource::<Crowds>().0[0].people[0].position
    }

    /// Suelta las teclas y espera hasta que la persona se quede quieta.
    fn settle(app: &mut App) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release_all();
        let mut still = 0;
        for _ in 0..400 {
            app.update();
            let moving = app.world().resource::<Crowds>().0[0].people[0].speed > 0.0;
            still = if moving { 0 } else { still + 1 };
            if still >= 10 {
                return;
            }
        }
        panic!("the person never stood still");
    }

    /// Pone al jinete `distance` m al sur de la persona (ya quieta), un poco al
    /// costado, detenido y mirando al norte.
    fn aim(app: &mut App, distance: f32) {
        settle(app);
        let target = person(app);
        let mut q = app.world_mut().query::<&mut Player>();
        let mut player = q.single_mut(app.world_mut()).unwrap();
        player.rider.position = target + GroundVec2::new(0.25, -distance);
        player.rider.heading = pampa_core::Heading::from_radians(std::f32::consts::FRAC_PI_2);
        player.rider.speed = 0.0;
    }

    fn ride(app: &mut App, gallop: bool, seconds: f32) {
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::KeyW);
            if gallop {
                keys.press(KeyCode::ShiftLeft);
            } else {
                keys.release(KeyCode::ShiftLeft);
            }
        }
        for _ in 0..(seconds / 0.05).round() as usize {
            app.update();
        }
    }

    fn said(app: &App) -> Vec<String> {
        app.world().resource::<Said>().0.clone()
    }

    fn bubbles(app: &mut App) -> Vec<(String, Entity)> {
        let mut q = app.world_mut().query::<(&SpeechBubble, &ChildOf)>();
        q.iter(app.world())
            .map(|(b, c)| (b.line.clone(), c.parent()))
            .collect()
    }

    #[test]
    fn galloping_into_a_walker_shows_one_gallop_complaint() {
        let mut app = app(RiderParams::default());
        aim(&mut app, 15.0);
        ride(&mut app, true, 3.5);
        let said = said(&app);
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(GALLOP.contains(&said[0].as_str()), "{said:?}");
        // Sobre la cabeza del peatón.
        let walker = {
            let mut q = app.world_mut().query_filtered::<Entity, With<Walker>>();
            q.single(app.world()).unwrap()
        };
        assert_eq!(bubbles(&mut app), [(said[0].clone(), walker)]);
    }

    #[test]
    fn the_same_walker_does_not_complain_again_within_the_cooldown() {
        let mut app = app(RiderParams::default());
        aim(&mut app, 15.0);
        ride(&mut app, true, 3.5);
        assert_eq!(said(&app).len(), 1);
        // Embestir otra vez enseguida: todavía en enfriamiento.
        aim(&mut app, 4.0);
        ride(&mut app, true, 1.5);
        assert_eq!(said(&app).len(), 1, "{:?}", said(&app));
        // Una vez terminado el enfriamiento, una queja nueva (distinta).
        ride(&mut app, true, 4.0);
        aim(&mut app, 4.0);
        ride(&mut app, true, 1.5);
        let said = said(&app);
        assert_eq!(said.len(), 2, "{said:?}");
        assert_ne!(said[0], said[1], "never the same line twice in a row");
        assert!(GALLOP.contains(&said[1].as_str()));
    }

    #[test]
    fn walking_into_a_walker_shows_a_walk_complaint() {
        let mut app = app(RiderParams::default());
        aim(&mut app, 3.0);
        ride(&mut app, false, 2.5);
        let said = said(&app);
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(WALK.contains(&said[0].as_str()), "{said:?}");
    }

    #[test]
    fn brushing_past_at_a_crawl_says_nothing() {
        let crawl = RiderParams {
            walk_max: 1.0,
            ..default()
        };
        let mut app = app(crawl);
        aim(&mut app, 3.0);
        ride(&mut app, false, 5.0);
        assert!(app.world().resource::<Shoves>().0 > 0, "the rider did push");
        assert!(said(&app).is_empty(), "{:?}", said(&app));
        assert!(bubbles(&mut app).is_empty());
    }

    #[test]
    fn shipped_and_builtin_lines_only_use_characters_the_font_draws() {
        let (shipped, error) = load_bumps_or_builtin(&asset_path(BUMPS_FILE));
        assert!(error.is_none(), "{error:?}");
        for lines in [shipped, BumpLines::builtin()] {
            assert_eq!(
                lines.unsupported_chars(|c| lettering::supports(Face::Body, c)),
                Vec::<char>::new()
            );
        }
    }

    #[test]
    fn missing_dialogue_is_loaded_at_startup() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(PlayerTuning::default())
            .add_plugins(ComplaintsPlugin);
        app.update();
        let dialogue = app.world().resource::<Dialogue>();
        assert!(!dialogue.bumps.gallop.is_empty());
    }
}
