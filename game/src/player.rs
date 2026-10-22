//! The player: keyboard -> `RiderInput` -> core step -> visuals.
//!
//! The player starts aboard the train ([`StartAboard`]), gets off on foot
//! and may later mount a horse; on foot and on horseback they move with the
//! same core model ([`Rider`]), the body and the tuning tell them apart
//! ([`Rider::travel`]). Aboard the train ([`OnTrain`]) the ride moves them
//! instead.

use bevy::prelude::*;
use pampa_core::{
    Heading, MAX_STEP, Rider, RiderInput, RiderParams, Travel, Vec2 as GroundVec2, World,
};
use pampa_render::{
    CharacterVisual, Coat, FigureKind, MapIcon, StreamFocus, VisualPose, world_from_ground,
};

use crate::ride::{OnTrain, StartAboard};

/// The logic body this entity represents, on foot or on horseback. Logic
/// state lives here; the entity's `Transform` and `VisualPose` are derived
/// from it every frame.
#[derive(Component, Debug)]
pub struct Player {
    pub rider: Rider,
}

/// The logic world the player moves through. Render draws from the same
/// value (see `main`), so what collides is what is drawn.
#[derive(Resource, Debug, Clone, Default)]
pub struct GameWorld(pub World);

/// Where, facing which way and how the player starts.
#[derive(Resource, Debug, Clone, Copy)]
pub struct PlayerSpawn {
    pub position: GroundVec2,
    pub heading: Heading,
    pub travel: Travel,
}

impl Default for PlayerSpawn {
    /// The origin, facing north (where the camera initially looks), on
    /// horseback. The game itself starts on foot (see `main`).
    fn default() -> Self {
        Self {
            position: GroundVec2::ZERO,
            heading: Heading::from_radians(std::f32::consts::FRAC_PI_2),
            travel: Travel::Riding,
        }
    }
}

/// Movement tuning for the player's horse.
#[derive(Resource, Debug, Default)]
pub struct PlayerTuning(pub RiderParams);

/// Movement tuning for the player on foot.
#[derive(Resource, Debug)]
pub struct FootTuning(pub RiderParams);

impl Default for FootTuning {
    fn default() -> Self {
        Self(RiderParams::on_foot())
    }
}

/// How the player getting around by `travel` is drawn: the clerk on foot,
/// or the clerk in the saddle of Don Hidalgo's bayo, the only horse he
/// rides.
pub fn player_visual(travel: Travel) -> CharacterVisual {
    match travel {
        Travel::OnFoot => CharacterVisual::Figure(FigureKind::Protagonist),
        Travel::Riding => CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::HIDALGO,
        },
    }
}

/// Systems that advance the player's logic; run cameras after this.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct DrivePlayer;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerTuning>()
            .init_resource::<FootTuning>()
            .init_resource::<GameWorld>()
            .init_resource::<PlayerSpawn>()
            .init_resource::<StreamFocus>()
            .add_systems(Startup, spawn_player)
            .add_systems(
                Update,
                (drive_player, follow_player).chain().in_set(DrivePlayer),
            );
    }
}

fn spawn_player(
    mut commands: Commands,
    world: Res<GameWorld>,
    spawn: Res<PlayerSpawn>,
    aboard: Option<Res<StartAboard>>,
) {
    // On the world's ground, so a spawn on the platform stands on its top.
    let ground = &world.0;
    let rider = match spawn.travel {
        Travel::OnFoot => Rider::on_foot(spawn.position, spawn.heading, ground),
        Travel::Riding => Rider::new(spawn.position, spawn.heading, ground),
    };
    let mut player = commands.spawn((
        Name::new("Player"),
        MapIcon::Player,
        player_visual(spawn.travel),
        VisualPose {
            heading: rider.heading,
            ..default()
        },
        Transform::from_translation(world_from_ground(rider.position, rider.height)),
        Player { rider },
    ));
    // Seated in a coach (the ride places them), out of sight.
    if let Some(ride) = aboard.and_then(|a| a.0) {
        player.insert((OnTrain(ride), Visibility::Hidden));
    }
}

/// The world streams (ground chunks, towns) around the player.
fn follow_player(players: Query<&Player>, mut focus: ResMut<StreamFocus>) {
    if let Some(player) = players.iter().next() {
        focus.0 = player.rider.position;
    }
}

/// Maps held keys to logic intent: W/S throttle, A/D turn, Shift gallop.
pub fn read_input(keys: &ButtonInput<KeyCode>) -> RiderInput {
    let axis = |positive: KeyCode, negative: KeyCode| {
        f32::from(u8::from(keys.pressed(positive))) - f32::from(u8::from(keys.pressed(negative)))
    };
    RiderInput {
        throttle: axis(KeyCode::KeyW, KeyCode::KeyS),
        turn: axis(KeyCode::KeyA, KeyCode::KeyD),
        gallop: keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]),
    }
}

pub(crate) fn drive_player(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    (tuning, foot): (Res<PlayerTuning>, Res<FootTuning>),
    world: Res<GameWorld>,
    mut players: Query<(&mut Player, &mut Transform, &mut VisualPose), Without<OnTrain>>,
) {
    let input = read_input(&keys);
    let dt = time.delta_secs().min(MAX_STEP);
    for (mut player, mut transform, mut pose) in &mut players {
        let rider = &mut player.rider;
        let params = match rider.travel() {
            Travel::OnFoot => &foot.0,
            Travel::Riding => &tuning.0,
        };
        rider.step(input, params, &world.0, dt);
        transform.translation = world_from_ground(rider.position, rider.height);
        *pose = VisualPose {
            heading: rider.heading,
            animation: rider.animation(params),
            speed: rider.speed.abs(),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    /// Headless app: no window, no renderer, time advanced by hand.
    fn headless_app(frame: Duration) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(frame))
            .init_resource::<ButtonInput<KeyCode>>()
            .add_plugins(PlayerPlugin);
        app
    }

    fn player_rider(app: &mut App) -> Rider {
        let mut query = app.world_mut().query::<&Player>();
        query.single(app.world()).expect("one player").rider
    }

    #[test]
    fn long_frames_are_capped_to_max_step() {
        // Bevy's virtual clock already clamps at 0.25 s; stay below that so
        // this exercises our own cap.
        let mut app = headless_app(Duration::from_millis(200));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ShiftLeft);
        app.update();
        app.update();

        let before = player_rider(&mut app);
        app.update();
        assert!((app.world().resource::<Time>().delta_secs() - 0.2).abs() < 1e-6);
        let after = player_rider(&mut app);

        let accel = app.world().resource::<PlayerTuning>().0.acceleration;
        let expected_speed = before.speed + accel * MAX_STEP;
        assert!(
            (after.speed - expected_speed).abs() < 1e-5,
            "a 0.2 s hitch must advance only {MAX_STEP} s: {} vs {expected_speed}",
            after.speed
        );
        let moved = after.position.distance(before.position);
        assert!((moved - after.speed * MAX_STEP).abs() < 1e-5);
    }

    #[test]
    fn spawns_at_the_configured_spawn_point() {
        let spawn = PlayerSpawn {
            position: GroundVec2::new(26.0, -161.0),
            heading: Heading::from_radians(1.0),
            ..PlayerSpawn::default()
        };
        let mut app = headless_app(Duration::from_millis(16));
        app.insert_resource(spawn);
        app.update();
        let rider = player_rider(&mut app);
        assert_eq!(
            (rider.position, rider.heading),
            (spawn.position, spawn.heading)
        );
    }

    #[test]
    fn player_transform_follows_logic() {
        let mut app = headless_app(Duration::from_millis(16));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        for _ in 0..30 {
            app.update();
        }
        let rider = player_rider(&mut app);
        assert!(rider.speed > 0.0);
        let mut query = app.world_mut().query_filtered::<&Transform, With<Player>>();
        let transform = query.single(app.world()).unwrap();
        assert_eq!(
            transform.translation,
            world_from_ground(rider.position, rider.height)
        );
        // The world streams around the rider.
        assert_eq!(app.world().resource::<StreamFocus>().0, rider.position);
    }

    #[test]
    fn minimap_arrow_follows_the_rider() {
        use pampa_core::TownLayout;
        use pampa_render::WorldTowns;
        use pampa_render::minimap::Minimap;

        let mut app = headless_app(Duration::from_millis(16));
        app.init_resource::<Assets<Image>>()
            .insert_resource(WorldTowns(vec![TownLayout::castex_inspired()]))
            .add_plugins(pampa_render::MinimapPlugin);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        let mut marks = Vec::new();
        for _ in 0..40 {
            app.update();
            let rider = player_rider(&mut app);
            let minimap = app.world().resource::<Minimap>();
            let (expected, _) = minimap.frame.to_map_clamped(rider.position);
            let at = minimap.player_px.expect("player marked");
            assert!(at.distance(expected) < 1e-3, "{at:?} vs {expected:?}");
            marks.push(at);
        }
        // Riding north moves the arrow up the map.
        assert!(marks.last().unwrap().y < marks[0].y);
    }

    #[test]
    fn a_player_spawned_on_foot_walks_and_jogs_at_a_persons_pace() {
        let mut app = headless_app(Duration::from_millis(50));
        app.insert_resource(PlayerSpawn {
            travel: Travel::OnFoot,
            ..PlayerSpawn::default()
        });
        app.update();
        let rider = player_rider(&mut app);
        assert_eq!(rider.travel(), Travel::OnFoot);
        assert_eq!(rider.collider.radius, Rider::PERSON_RADIUS);
        let mut q = app
            .world_mut()
            .query_filtered::<&CharacterVisual, With<Player>>();
        assert_eq!(
            *q.single(app.world()).unwrap(),
            CharacterVisual::Figure(FigureKind::Protagonist)
        );
        let foot = app.world().resource::<FootTuning>().0;
        assert_eq!(foot, RiderParams::on_foot());
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        for _ in 0..80 {
            app.update();
        }
        assert!((player_rider(&mut app).speed - foot.walk_max).abs() < 1e-4);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ShiftLeft);
        for _ in 0..80 {
            app.update();
        }
        assert!((player_rider(&mut app).speed - foot.gallop_max).abs() < 1e-4);
    }

    #[test]
    fn the_default_spawn_is_on_horseback() {
        let mut app = headless_app(Duration::from_millis(16));
        app.update();
        assert_eq!(player_rider(&mut app).travel(), Travel::Riding);
        let mut q = app
            .world_mut()
            .query_filtered::<&CharacterVisual, With<Player>>();
        assert_eq!(
            *q.single(app.world()).unwrap(),
            CharacterVisual::Mounted {
                rider: FigureKind::Protagonist,
                coat: Coat::Bayo,
            }
        );
    }

    #[test]
    fn maps_keys_to_rider_input() {
        let mut keys = ButtonInput::<KeyCode>::default();
        assert_eq!(read_input(&keys), RiderInput::default());

        keys.press(KeyCode::KeyW);
        keys.press(KeyCode::KeyD);
        keys.press(KeyCode::ShiftLeft);
        let input = read_input(&keys);
        assert_eq!(input.throttle, 1.0);
        assert_eq!(input.turn, -1.0);
        assert!(input.gallop);

        keys.press(KeyCode::KeyS);
        keys.press(KeyCode::KeyA);
        let opposed = read_input(&keys);
        assert_eq!((opposed.throttle, opposed.turn), (0.0, 0.0));
    }
}
