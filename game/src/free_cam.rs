//! La cámara libre de la consola de depuración ([`FreeCam`]).
//!
//! `view free` suelta la cámara del jugador y la deja volar: WASD la mueve
//! hacia donde mira, el mouse mira, Espacio y Ctrl suben y bajan, y Shift va
//! [`FAST`] veces más rápido. La rueda cambia el campo de visión, no la
//! distancia: x2 o x4 sin acercarse (entre [`MIN_ZOOM`] y [`MAX_ZOOM`]); `zoom
//! N` en la consola lo fija exacto. El jugador se queda donde estaba: mientras
//! la cámara vuela, el juego no ve las teclas del jinete ([`hide_keys`]), y los
//! animales reaccionan a la posición del jinete, no a la de la cámara, así que
//! se los puede rodear sin espantarlos. `view tp` o `view fp` vuelven al
//! jugador: la cámara de órbita salta de nuevo detrás de él, con la lente
//! normal.

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use pampa_core::Vec2 as GroundVec2;
use pampa_render::character::viewer::world_layers;
use pampa_render::{CloseShadows, SightLine};

use crate::camera::{CameraInput, CameraTuning, FollowCamera};
use crate::first_person::{BodyTooClose, set_lens};
use crate::player::GameWorld;

/// Velocidad de vuelo, en m/s.
pub const FLY_SPEED: f32 = 12.0;
/// Cuántas veces más rápido vuela con Shift.
pub const FAST: f32 = 5.0;
/// El zoom más abierto: el campo de visión normal.
pub const MIN_ZOOM: f32 = 1.0;
/// El zoom más cerrado.
pub const MAX_ZOOM: f32 = 8.0;
/// Cuánto multiplica el zoom cada línea de rueda.
const ZOOM_PER_LINE: f32 = 1.25;
/// Despeje mínimo sobre el suelo bajo la cámara, en metros.
const CLEARANCE: f32 = 0.3;
/// Lo máximo que la cámara mira hacia arriba o hacia abajo, en radianes.
const PITCH_LIMIT: f32 = 1.55;
/// Las teclas que mueven la cámara en lugar del jinete.
const FLY_KEYS: [KeyCode; 7] = [
    KeyCode::KeyW,
    KeyCode::KeyA,
    KeyCode::KeyS,
    KeyCode::KeyD,
    KeyCode::Space,
    KeyCode::ShiftLeft,
    KeyCode::ShiftRight,
];

/// La cámara libre: si vuela, y su zoom (1 = el campo de visión normal).
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct FreeCam {
    pub on: bool,
    pub zoom: f32,
}

impl Default for FreeCam {
    fn default() -> Self {
        Self {
            on: false,
            zoom: MIN_ZOOM,
        }
    }
}

impl FreeCam {
    /// `zoom` dentro de [`MIN_ZOOM`]..=[`MAX_ZOOM`]; uno no finito es el
    /// normal.
    pub fn clamp_zoom(zoom: f32) -> f32 {
        if zoom.is_finite() {
            zoom.clamp(MIN_ZOOM, MAX_ZOOM)
        } else {
            MIN_ZOOM
        }
    }

    /// La lente con este zoom: el campo de visión normal dividido por él.
    pub fn lens(&self) -> PerspectiveProjection {
        let normal = PerspectiveProjection::default();
        PerspectiveProjection {
            fov: normal.fov / Self::clamp_zoom(self.zoom),
            ..normal
        }
    }
}

/// Condición de ejecución: la cámara sigue al jugador (no vuela).
pub(crate) fn following(free: Res<FreeCam>) -> bool {
    !free.on
}

/// Las teclas de [`FLY_KEYS`] presionadas este frame, que la cámara libre le
/// saca al juego.
#[derive(Resource, Debug, Default)]
pub(crate) struct HiddenKeys(Vec<KeyCode>);

/// Mientras la cámara vuela, saca de [`ButtonInput`] (después de que la
/// consola y el menú leyeron el teclado) las teclas que movería el jinete, para
/// que el resto del frame no las vea; [`restore_keys`] las devuelve al final
/// del frame, porque `ButtonInput` solo vuelve a enterarse de una tecla
/// mantenida cuando se suelta.
pub(crate) fn hide_keys(
    free: Res<FreeCam>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut hidden: ResMut<HiddenKeys>,
) {
    hidden.0.clear();
    if !free.on {
        return;
    }
    for key in FLY_KEYS {
        if keys.pressed(key) {
            hidden.0.push(key);
            keys.reset(key);
        }
    }
}

/// Devuelve las teclas que [`hide_keys`] sacó, como mantenidas (no recién
/// presionadas).
pub(crate) fn restore_keys(mut keys: ResMut<ButtonInput<KeyCode>>, hidden: Res<HiddenKeys>) {
    for &key in &hidden.0 {
        keys.press(key);
        keys.clear_just_pressed(key);
    }
}

/// Hacia dónde mira la cámara libre: `yaw` alrededor de la vertical, `pitch`
/// hacia arriba (positivo) o hacia abajo.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Aim {
    yaw: f32,
    pitch: f32,
}

/// La cámara del jugador, que la cámara libre mueve en lugar de la órbita.
type Flown<'w, 's> = Single<
    'w,
    's,
    (
        &'static mut Transform,
        &'static mut Projection,
        &'static mut RenderLayers,
    ),
    With<FollowCamera>,
>;

/// Vuela la cámara mientras [`FreeCam`] está encendida. Al soltarse, parte
/// desde donde estaba la cámara, mirando hacia el mismo lado, con el cuerpo
/// entero del jugador a la vista; al volver, la órbita retoma la cámara ese
/// mismo frame, y el zoom vuelve a x1.
#[allow(clippy::too_many_arguments)]
pub(crate) fn fly(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    (hidden, input, tuning): (Res<HiddenKeys>, Res<CameraInput>, Res<CameraTuning>),
    world: Res<GameWorld>,
    mut free: ResMut<FreeCam>,
    (sight, mut shadows, mut too_close): (
        Option<ResMut<SightLine>>,
        ResMut<CloseShadows>,
        ResMut<BodyTooClose>,
    ),
    camera: Flown,
    mut aim: Local<Option<Aim>>,
) {
    let (mut transform, mut projection, mut layers) = camera.into_inner();
    if !free.on {
        // La órbita, que no se movió mientras la cámara volaba, vuelve a
        // ubicar la cámara detrás del jugador, y la primera persona devuelve
        // la lente normal.
        if aim.take().is_some() {
            free.zoom = MIN_ZOOM;
        }
        return;
    }
    let aim = aim.get_or_insert_with(|| {
        let (yaw, pitch, _) = transform.rotation.to_euler(EulerRot::YXZ);
        layers.set_if_neq(world_layers());
        shadows.set_if_neq(CloseShadows(false));
        too_close.set_if_neq(BodyTooClose(false));
        if let Some(mut sight) = sight {
            sight.0 = None;
        }
        Aim { yaw, pitch }
    });
    // La rueda acerca la vista; con zoom, el mouse gira más despacio.
    free.zoom = FreeCam::clamp_zoom(free.zoom * ZOOM_PER_LINE.powf(input.0.zoom));
    let turn = tuning.0.mouse_sensitivity / free.zoom;
    aim.yaw -= input.0.look.0 * turn;
    aim.pitch = (aim.pitch - input.0.look.1 * turn).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    transform.rotation = Quat::from_euler(EulerRot::YXZ, aim.yaw, aim.pitch, 0.0);
    set_lens(&mut projection, free.lens());

    let held = |key| f32::from(u8::from(hidden.0.contains(&key)));
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let shift = hidden.0.contains(&KeyCode::ShiftLeft) || hidden.0.contains(&KeyCode::ShiftRight);
    let way = *transform.forward() * (held(KeyCode::KeyW) - held(KeyCode::KeyS))
        + *transform.right() * (held(KeyCode::KeyD) - held(KeyCode::KeyA))
        + Vec3::Y * (held(KeyCode::Space) - f32::from(u8::from(ctrl)));
    let speed = if shift { FLY_SPEED * FAST } else { FLY_SPEED };
    let mut at = transform.translation + way.normalize_or_zero() * speed * time.delta_secs();
    // Nunca bajo el suelo.
    let floor = world.0.ground_height(GroundVec2::new(at.x, -at.z)) + CLEARANCE;
    at.y = at.y.max(floor);
    transform.translation = at;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::{CameraOccluders, FollowCameraPlugin};
    use crate::first_person::ViewMode;
    use crate::player::{Player, PlayerPlugin, PlayerSpawn};
    use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
    use bevy::time::TimeUpdateStrategy;
    use pampa_core::World;
    use std::time::Duration;

    const FRAME: f32 = 0.016;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
                FRAME,
            )))
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(GameWorld(World::default()))
            .insert_resource(PlayerSpawn::default())
            .insert_resource(CameraOccluders::default())
            .add_plugins((PlayerPlugin, FollowCameraPlugin));
        app.update();
        app.update();
        app
    }

    fn camera(app: &mut App) -> (Transform, f32) {
        let mut q = app
            .world_mut()
            .query_filtered::<(&Transform, &Projection), With<FollowCamera>>();
        let (transform, projection) = q.single(app.world()).unwrap();
        let Projection::Perspective(p) = projection else {
            panic!("perspective");
        };
        (*transform, p.fov)
    }

    fn rider_at(app: &mut App) -> GroundVec2 {
        let mut q = app.world_mut().query::<&Player>();
        q.single(app.world()).unwrap().rider.position
    }

    fn fly(app: &mut App) {
        app.world_mut().resource_mut::<FreeCam>().on = true;
        app.update();
    }

    fn scroll(app: &mut App, lines: f32) {
        app.insert_resource(AccumulatedMouseScroll {
            unit: MouseScrollUnit::Line,
            delta: Vec2::new(0.0, lines),
        });
        app.update();
        app.insert_resource(AccumulatedMouseScroll::default());
    }

    #[test]
    fn in_free_view_wasd_flies_the_camera_and_the_player_stays() {
        let mut app = app();
        fly(&mut app);
        let (before, _) = camera(&mut app);
        let rider = rider_at(&mut app);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        for _ in 0..30 {
            app.update();
        }
        let (after, _) = camera(&mut app);
        let ahead = (after.translation - before.translation).dot(*before.forward());
        assert!(
            (ahead - FLY_SPEED * FRAME * 30.0).abs() < 0.5,
            "flew {ahead} m"
        );
        assert_eq!(rider_at(&mut app), rider, "the player stays put");
        // La tecla sigue presionada para el juego en cuanto se vuelve al
        // jugador.
        app.world_mut().resource_mut::<FreeCam>().on = false;
        for _ in 0..30 {
            app.update();
        }
        assert_ne!(rider_at(&mut app), rider, "W drives the player again");
    }

    #[test]
    fn the_wheel_narrows_the_field_of_view_up_to_the_limit() {
        let mut app = app();
        fly(&mut app);
        let (_, normal) = camera(&mut app);
        assert_eq!(normal, PerspectiveProjection::default().fov);
        scroll(&mut app, 3.0);
        let (_, fov) = camera(&mut app);
        assert!((fov - normal / ZOOM_PER_LINE.powi(3)).abs() < 1e-5, "{fov}");
        scroll(&mut app, 100.0);
        assert!((camera(&mut app).1 - normal / MAX_ZOOM).abs() < 1e-5);
        scroll(&mut app, -100.0);
        assert!((camera(&mut app).1 - normal).abs() < 1e-5);
    }

    #[test]
    fn view_tp_returns_to_the_follow_camera_and_the_normal_lens() {
        let mut app = app();
        let (orbit, normal) = camera(&mut app);
        fly(&mut app);
        scroll(&mut app, 3.0);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        for _ in 0..60 {
            app.update();
        }
        assert!(camera(&mut app).0.translation.distance(orbit.translation) > 5.0);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::KeyW);
        // Lo que hace `view tp`.
        app.world_mut().resource_mut::<FreeCam>().on = false;
        *app.world_mut().resource_mut::<ViewMode>() = ViewMode::ThirdPerson;
        app.update();
        let (back, fov) = camera(&mut app);
        assert_eq!(fov, normal);
        assert!(
            back.translation.distance(orbit.translation) < 0.01,
            "{back:?} {orbit:?}"
        );
        assert_eq!(app.world().resource::<FreeCam>().zoom, MIN_ZOOM);
    }
}
