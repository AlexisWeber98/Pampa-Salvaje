//! El jugador: teclado -> `RiderInput` -> paso del core -> visuales.
//!
//! El jugador empieza a bordo del tren ([`StartAboard`]), se baja a pie y más
//! tarde puede montar un caballo; a pie y a caballo se mueve con el mismo
//! modelo del core ([`Rider`]); el cuerpo y el ajuste los distinguen
//! ([`Rider::travel`]). A bordo del tren ([`OnTrain`]) lo mueve, en cambio, el
//! viaje. En una escena, con las teclas bloqueadas ([`ControlLock`]), puede
//! caminar (o cabalgar) solo por un camino y volverse hacia alguien
//! ([`WalkPath`]), con el mismo paso del core.

use bevy::prelude::*;
use pampa_core::{
    Heading, MAX_STEP, Rider, RiderInput, RiderParams, Travel, Vec2 as GroundVec2, World,
};
use pampa_render::{
    CharacterVisual, Coat, FigureKind, MapIcon, StreamFocus, ViewerBody, VisualPose,
    world_from_ground,
};

use crate::ride::{OnTrain, StartAboard};

/// El cuerpo lógico que representa esta entidad, a pie o a caballo. El estado
/// lógico vive aquí; el `Transform` y el `VisualPose` de la entidad se derivan
/// de él en cada frame.
#[derive(Component, Debug)]
pub struct Player {
    pub rider: Rider,
}

/// El mundo lógico por el que se mueve el jugador. Render dibuja a partir del
/// mismo valor (ver `main`), así que lo que colisiona es lo que se dibuja.
#[derive(Resource, Debug, Clone, Default)]
pub struct GameWorld(pub World);

/// Dónde, mirando hacia dónde y cómo empieza el jugador.
#[derive(Resource, Debug, Clone, Copy)]
pub struct PlayerSpawn {
    pub position: GroundVec2,
    pub heading: Heading,
    pub travel: Travel,
}

impl Default for PlayerSpawn {
    /// El origen, mirando al norte (hacia donde mira inicialmente la cámara), a
    /// caballo. El juego en sí empieza a pie (ver `main`).
    fn default() -> Self {
        Self {
            position: GroundVec2::ZERO,
            heading: Heading::from_radians(std::f32::consts::FRAC_PI_2),
            travel: Travel::Riding,
        }
    }
}

/// Ajuste de movimiento para el caballo del jugador.
#[derive(Resource, Debug, Default)]
pub struct PlayerTuning(pub RiderParams);

/// Ajuste de movimiento para el jugador a pie.
#[derive(Resource, Debug)]
pub struct FootTuning(pub RiderParams);

impl Default for FootTuning {
    fn default() -> Self {
        Self(RiderParams::on_foot())
    }
}

/// Cómo se dibuja al jugador según cómo se desplaza (`travel`): el empleado a
/// pie, o el empleado en la montura del alazán de Don Hidalgo, el primer caballo
/// que monta.
pub fn player_visual(travel: Travel) -> CharacterVisual {
    player_visual_on(travel, Coat::HIDALGO)
}

/// Cómo se dibuja al jugador según cómo se desplaza (`travel`), en la montura
/// de un caballo de pelaje `coat` cuando cabalga.
pub fn player_visual_on(travel: Travel, coat: Coat) -> CharacterVisual {
    match travel {
        Travel::OnFoot => CharacterVisual::Figure(FigureKind::Protagonist),
        Travel::Riding => CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat,
        },
    }
}

/// El pelaje del caballo que monta el jugador (o que montó por última vez): el
/// alazán de Don Hidalgo hasta que monte otro.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RiddenCoat(pub Coat);

impl Default for RiddenCoat {
    fn default() -> Self {
        Self(Coat::HIDALGO)
    }
}

/// Sistemas que hacen avanzar la lógica del jugador; las cámaras se ejecutan
/// después.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct DrivePlayer;

/// Si el jugador no maneja al protagonista: mientras se reproduce una escena de
/// la historia (ver [`crate::errand`]) las teclas no lo mueven, y un caballo en
/// marcha se detiene solo.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ControlLock(pub bool);

/// Un camino que el protagonista recorre solo, al paso, mientras las teclas
/// están bloqueadas ([`ControlLock`]): gira hacia el próximo punto y va hacia
/// él con el mismo paso del core que las teclas (choca, pisa y anda como
/// siempre), y lo quita al llegar. A pie camina; montado, el caballo deja la
/// marcha que traía y frena solo (sin tirones: el roce del core) hasta el
/// tranco, y frena del todo al llegar. Sin puntos, se vuelve en el lugar hacia
/// `face`, si lo hay; si no, no hace nada.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct WalkPath {
    pub points: Vec<GroundVec2>,
    /// Más despacio que el paso de la marcha, en m/s (entrar tímido a un
    /// lugar); `None`, al paso.
    pub pace: Option<f32>,
    /// Hacia dónde se vuelve cuando no camina: hacia quien le habla.
    pub face: Option<GroundVec2>,
}

impl WalkPath {
    /// El camino por `points`, al paso.
    pub fn along(points: Vec<GroundVec2>) -> Self {
        Self {
            points,
            ..default()
        }
    }

    /// Si todavía tiene puntos que recorrer.
    pub fn walking(&self) -> bool {
        !self.points.is_empty()
    }
}

/// A qué distancia de un punto de [`WalkPath`] se lo da por alcanzado, en
/// metros.
const ARRIVED: f32 = 0.2;

/// Hasta qué velocidad se da por llegado al último punto de [`WalkPath`], en
/// m/s: un caballo que pasa de largo al galope frena y vuelve.
const ARRIVAL_SPEED: f32 = 1.0;

/// Cuánto puede apartarse el rumbo de `face` en [`WalkPath`] para darse por
/// vuelto hacia él, en radianes.
const FACING: f32 = 0.05;

/// Cuánto puede apartarse el rumbo del próximo punto de [`WalkPath`] para
/// seguir caminando, en radianes; más, gira en el lugar.
const ON_COURSE: f32 = 0.6;

/// La intención que lleva a `rider` por `path`, quitando los puntos ya
/// alcanzados: girar hacia el próximo (todo lo que alcanza en `dt` con
/// `params`) y andar si ya lo tiene casi enfrente, sin pasar del paso (ni de
/// `pace`) y frenando a tiempo para detenerse en el último; sin puntos,
/// volverse en el lugar hacia `face`.
fn walk_along(path: &mut WalkPath, rider: &Rider, params: &RiderParams, dt: f32) -> RiderInput {
    let points = &mut path.points;
    while points.first().is_some_and(|to| {
        rider.position.distance(*to) <= ARRIVED
            && (points.len() > 1 || rider.speed.abs() <= ARRIVAL_SPEED)
    }) {
        points.remove(0);
    }
    let Some(&to) = points.first().or(path.face.as_ref()) else {
        return RiderInput::default();
    };
    let toward = to - rider.position;
    let goal = toward.y.atan2(toward.x);
    let delta = (goal - rider.heading.radians() + std::f32::consts::PI)
        .rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    let reach = (params.turn_rate * dt).max(f32::EPSILON);
    let turn = (delta / reach).clamp(-1.0, 1.0);
    if points.is_empty() && rider.speed.abs() <= ARRIVAL_SPEED {
        // Solo se vuelve hacia quien le habla.
        let turn = if delta.abs() < FACING { 0.0 } else { turn };
        return RiderInput { turn, ..default() };
    }
    // Lo que queda hasta el último punto, y la velocidad desde la que el roce
    // lo detiene justo ahí.
    let left: f32 = std::iter::once(rider.position)
        .chain(points.iter().copied())
        .collect::<Vec<_>>()
        .windows(2)
        .map(|w| w[0].distance(w[1]))
        .sum();
    if rider.speed > params.walk_max + ARRIVAL_SPEED / 2.0 {
        // Más rápido que el paso (al galope, corriendo): primero lo deja, sin
        // tirones y derecho, sin doblar de golpe; después vuelve.
        return RiderInput::default();
    }
    let stopping = (2.0 * params.friction * left).sqrt();
    let top = path.pace.unwrap_or(params.walk_max).min(stopping);
    let on_course = delta.abs() < ON_COURSE;
    RiderInput {
        throttle: if on_course && rider.speed < top {
            1.0
        } else {
            0.0
        },
        turn,
        ..default()
    }
}

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerTuning>()
            .init_resource::<FootTuning>()
            .init_resource::<SprintTaps>()
            .init_resource::<GameWorld>()
            .init_resource::<PlayerSpawn>()
            .init_resource::<StreamFocus>()
            .init_resource::<ControlLock>()
            .init_resource::<WalkPath>()
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
    // Sobre el suelo del mundo, para que al crearse en el andén quede parado
    // encima.
    let ground = &world.0;
    let rider = match spawn.travel {
        Travel::OnFoot => Rider::on_foot(spawn.position, spawn.heading, ground),
        Travel::Riding => Rider::new(spawn.position, spawn.heading, ground),
    };
    let mut player = commands.spawn((
        Name::new("Player"),
        MapIcon::Player,
        // La cámara en primera persona se ubica en la cabeza de este cuerpo.
        ViewerBody,
        player_visual(spawn.travel),
        VisualPose {
            heading: rider.heading,
            ..default()
        },
        Transform::from_translation(world_from_ground(rider.position, rider.height)),
        Player { rider },
    ));
    // Sentado en un coche (el viaje lo ubica), fuera de la vista.
    if let Some(ride) = aboard.and_then(|a| a.0) {
        player.insert((OnTrain(ride), Visibility::Hidden));
    }
}

/// El mundo hace streaming (chunks de suelo, pueblos) alrededor del jugador.
pub(crate) fn follow_player(players: Query<&Player>, mut focus: ResMut<StreamFocus>) {
    if let Some(player) = players.iter().next() {
        focus.0 = player.rider.position;
    }
}

/// Hace saltar al caballo a galope tendido (ver [`Rider::jump`]); más lento, o
/// a pie, no hace nada.
pub const JUMP_KEY: KeyCode = KeyCode::Space;

/// Cuánto puede tardar la segunda presión de Shift tras la primera para que
/// el doble toque empiece el sprint, en segundos.
pub const SPRINT_TAP_WINDOW: f32 = 0.35;

/// El sprint de a pie: mantener Shift corre; tocarlo y volver a apretarlo
/// enseguida (un doble toque), manteniéndolo, sprinta hasta que se suelta.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub struct SprintTaps {
    /// Cuándo se apretó Shift por última vez, en segundos desde el inicio.
    last: Option<f32>,
    /// Si el doble toque sigue apretado.
    sprinting: bool,
}

impl SprintTaps {
    /// Registra el estado de Shift en `now` (`pressed`, recién apretado en
    /// este frame; `held`, apretado) y dice si se sprinta.
    pub fn update(&mut self, pressed: bool, held: bool, now: f32) -> bool {
        if pressed {
            self.sprinting = self.last.is_some_and(|t| now - t <= SPRINT_TAP_WINDOW);
            self.last = Some(now);
        }
        self.sprinting &= held;
        self.sprinting
    }
}

/// Traduce las teclas presionadas a intención lógica: W/S acelerador, A/D giro,
/// Shift galope (el sprint lo agrega [`SprintTaps`]).
pub fn read_input(keys: &ButtonInput<KeyCode>) -> RiderInput {
    let axis = |positive: KeyCode, negative: KeyCode| {
        f32::from(u8::from(keys.pressed(positive))) - f32::from(u8::from(keys.pressed(negative)))
    };
    RiderInput {
        throttle: axis(KeyCode::KeyW, KeyCode::KeyS),
        turn: axis(KeyCode::KeyA, KeyCode::KeyD),
        gallop: keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]),
        sprint: false,
    }
}

pub(crate) fn drive_player(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut taps: ResMut<SprintTaps>,
    (tuning, foot): (Res<PlayerTuning>, Res<FootTuning>),
    world: Res<GameWorld>,
    (lock, mut path): (Res<ControlLock>, Option<ResMut<WalkPath>>),
    mut players: Query<(&mut Player, &mut Transform, &mut VisualPose), Without<OnTrain>>,
) {
    let shift = [KeyCode::ShiftLeft, KeyCode::ShiftRight];
    let sprint = taps.update(
        keys.any_just_pressed(shift),
        keys.any_pressed(shift),
        time.elapsed_secs(),
    );
    let input = if lock.0 {
        RiderInput::default()
    } else {
        RiderInput {
            sprint,
            ..read_input(&keys)
        }
    };
    let dt = time.delta_secs().min(MAX_STEP);
    for (mut player, mut transform, mut pose) in &mut players {
        let rider = &mut player.rider;
        let params = match rider.travel() {
            Travel::OnFoot => &foot.0,
            Travel::Riding => &tuning.0,
        };
        let before = rider.heading;
        if keys.just_pressed(JUMP_KEY) && !lock.0 {
            rider.jump(params);
        }
        let input = match path.as_deref_mut() {
            Some(path) if lock.0 && (path.walking() || path.face.is_some()) => {
                walk_along(path, rider, params, dt)
            }
            _ => input,
        };
        rider.step(input, params, &world.0, dt);
        transform.translation = world_from_ground(rider.position, rider.elevation());
        *pose = VisualPose {
            heading: rider.heading,
            animation: rider.animation(params),
            speed: rider.speed,
            turn: yaw_rate(before, rider.heading, dt),
            flight: rider.flight(),
        };
    }
}

/// La velocidad de giro (rad/s, positiva hacia la izquierda) que convirtió
/// `before` en `after` en `dt` segundos, por el camino más corto; ninguna si no
/// pasó tiempo.
fn yaw_rate(before: Heading, after: Heading, dt: f32) -> f32 {
    if dt <= 0.0 {
        return 0.0;
    }
    let turned = after.radians() - before.radians();
    turned.sin().atan2(turned.cos()) / dt
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    /// App sin interfaz: sin ventana, sin renderer, con el tiempo avanzado a
    /// mano.
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
        // El reloj virtual de Bevy ya limita a 0.25 s; se queda por debajo para
        // que esto ejercite nuestro propio límite.
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
        // El mundo hace streaming alrededor del jinete.
        assert_eq!(app.world().resource::<StreamFocus>().0, rider.position);
    }

    /// Cabalga con W y Shift durante cuatro segundos (un caballo llega a galope
    /// tendido) y luego toca la tecla de salto durante un frame.
    fn tap_jump_moving(app: &mut App) {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.press(KeyCode::KeyW);
        keys.press(KeyCode::ShiftLeft);
        for _ in 0..250 {
            app.update();
        }
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(JUMP_KEY);
        app.update();
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(JUMP_KEY);
        keys.clear_just_pressed(JUMP_KEY);
    }

    #[test]
    fn the_jump_key_lifts_a_moving_horse_and_its_transform() {
        let mut app = headless_app(Duration::from_millis(16));
        tap_jump_moving(&mut app);
        app.update();
        let rider = player_rider(&mut app);
        assert!(rider.airborne() && rider.air > 0.0, "{rider:?}");
        let mut query = app.world_mut().query_filtered::<&Transform, With<Player>>();
        let transform = query.single(app.world()).unwrap();
        assert_eq!(
            transform.translation,
            world_from_ground(rider.position, rider.elevation())
        );
        // Vuelve a bajar.
        for _ in 0..90 {
            app.update();
        }
        assert!(!player_rider(&mut app).airborne());
    }

    #[test]
    fn the_jump_key_does_nothing_short_of_full_gallop() {
        let mut app = headless_app(Duration::from_millis(16));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        for _ in 0..120 {
            app.update();
        }
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(JUMP_KEY);
        app.update();
        let rider = player_rider(&mut app);
        assert!(rider.speed > 0.0);
        assert!(!rider.airborne());
    }

    #[test]
    fn the_jump_key_does_nothing_on_foot() {
        let mut app = headless_app(Duration::from_millis(16));
        app.insert_resource(PlayerSpawn {
            travel: Travel::OnFoot,
            ..PlayerSpawn::default()
        });
        tap_jump_moving(&mut app);
        let rider = player_rider(&mut app);
        assert!(rider.speed > 0.0);
        assert!(!rider.airborne());
    }

    #[test]
    fn turning_in_place_puts_the_yaw_rate_on_the_pose() {
        let mut app = headless_app(Duration::from_millis(16));
        for (key, sign) in [(KeyCode::KeyA, 1.0), (KeyCode::KeyD, -1.0)] {
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(key);
            for _ in 0..5 {
                app.update();
            }
            let rate = app.world().resource::<PlayerTuning>().0.turn_rate;
            let mut query = app
                .world_mut()
                .query_filtered::<&VisualPose, With<Player>>();
            let pose = *query.single(app.world()).unwrap();
            assert_eq!(pose.speed, 0.0);
            assert!((pose.turn - sign * rate).abs() < 1e-6, "{}", pose.turn);
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .release(key);
            app.update();
            let pose = *query.single(app.world()).unwrap();
            assert_eq!(pose.turn, 0.0, "no key, no turn");
        }
    }

    #[test]
    fn the_poses_turn_is_the_heading_actually_turned() {
        // Un frame sin tiempo no gira nada, así que la pose no da pasos.
        let mut app = headless_app(Duration::ZERO);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyA);
        for _ in 0..3 {
            app.update();
        }
        let mut query = app
            .world_mut()
            .query_filtered::<&VisualPose, With<Player>>();
        assert_eq!(query.single(app.world()).unwrap().turn, 0.0);
    }

    #[test]
    fn a_turn_across_east_is_the_short_way_round() {
        // Los rumbos dan la vuelta en 0 (este): de 2π - 0.01 a 0.01 hay 0.02
        // hacia la izquierda.
        let before = Heading::from_radians(-0.01);
        let after = Heading::from_radians(0.01);
        assert!((yaw_rate(before, after, 0.1) - 0.2).abs() < 1e-4);
        assert!((yaw_rate(after, before, 0.1) + 0.2).abs() < 1e-4);
        assert_eq!(yaw_rate(before, after, 0.0), 0.0);
    }

    #[test]
    fn backing_up_keeps_the_poses_speed_negative() {
        let mut app = headless_app(Duration::from_millis(16));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyS);
        for _ in 0..30 {
            app.update();
        }
        let rider = player_rider(&mut app);
        assert!(rider.speed < 0.0, "{}", rider.speed);
        let mut query = app
            .world_mut()
            .query_filtered::<&VisualPose, With<Player>>();
        let pose = query.single(app.world()).unwrap();
        // Render reproduce el tranco hacia atrás según este signo.
        assert_eq!(pose.speed, rider.speed);
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
        // Cabalgar hacia el norte mueve la flecha hacia arriba en el mapa.
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
        // Mantenido: sin el plugin de entrada, `just_pressed` se borra a mano,
        // o cada frame contaría como otra presión (ver [`SprintTaps`]).
        for _ in 0..80 {
            app.update();
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .clear();
        }
        assert!((player_rider(&mut app).speed - foot.gallop_max).abs() < 1e-4);
    }

    /// Toca Shift una vez: presionada un frame, suelta el siguiente (sin el
    /// plugin de entrada, `just_pressed` se borra a mano).
    fn tap_shift(app: &mut App) {
        for press in [true, false] {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            if press {
                keys.press(KeyCode::ShiftLeft);
            } else {
                keys.release(KeyCode::ShiftLeft);
            }
            app.update();
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .clear();
        }
    }

    #[test]
    fn double_tapping_and_holding_shift_on_foot_sprints_until_it_is_let_go() {
        // Mantener Shift corre; tocarlo y volver a apretarlo enseguida,
        // manteniéndolo, sprinta; al soltarlo se vuelve a caminar.
        let mut app = headless_app(Duration::from_millis(50));
        app.insert_resource(PlayerSpawn {
            travel: Travel::OnFoot,
            ..PlayerSpawn::default()
        });
        app.update();
        let foot = app.world().resource::<FootTuning>().0;
        let hold = |app: &mut App, frames: usize| {
            for _ in 0..frames {
                app.update();
                app.world_mut()
                    .resource_mut::<ButtonInput<KeyCode>>()
                    .clear();
            }
        };
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        tap_shift(&mut app);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ShiftLeft);
        hold(&mut app, 80);
        assert!((player_rider(&mut app).speed - foot.sprint_max).abs() < 1e-4);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::ShiftLeft);
        hold(&mut app, 80);
        assert!((player_rider(&mut app).speed - foot.walk_max).abs() < 1e-4);
        // Apretado de nuevo, sin el doble toque, corre.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ShiftLeft);
        hold(&mut app, 80);
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
                coat: Coat::HIDALGO,
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
