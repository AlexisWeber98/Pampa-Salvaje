//! Vista en primera persona desde los ojos del jugador, que se alterna con `V`
//! frente a la órbita en tercera persona ([`ViewMode`]), para ver de cerca los
//! detalles del mundo.
//!
//! - **Ojos.** La cámara se ubica en [`EYE_IN_HEAD`] dentro del marco del hueso
//!   `head` del modelo del jugador ([`ViewerHead`], marcado por render): a pie
//!   sigue la caminata y en la montura sigue al jinete sentado. Su
//!   desplazamiento respecto de los pies del jugador se suaviza
//!   ([`EyeFollow`]), lo que quita el temblor de la caminata y del galope, pero
//!   nunca deja que los ojos queden más de [`MAX_LAG`] detrás de la cabeza; el
//!   giro de la vista viene solo de la mirada, nunca de la cabeza. A pie se
//!   mantiene nivelada; en la montura toma [`RIDING_SWAY`] del pitch y del roll
//!   de la cabeza, así que la caminata y el galope la mecen un poco. Hasta que
//!   el modelo está cargado (o sin modelo), los ojos quedan a una altura fija.
//! - **Mirada.** El mouse (o Q/E) gira la cabeza ([`FirstPersonLook`]), hacia
//!   arriba y hacia abajo dentro de [`PITCH_LIMIT`]. A pie, el cuerpo gira para
//!   seguir la vista mientras camina; en la montura el caballo se dirige como
//!   antes y la cabeza gira como máximo [`RIDING_YAW_LIMIT`] hacia cada lado de
//!   su rumbo. El teclado mueve al jugador como siempre.
//! - **Cuerpo propio.** Render recorta la cara y el cráneo del jugador de su
//!   piel y los pone, con el sombrero, el pelo y los ojos, en una capa que la
//!   cámara en primera persona no dibuja ([`first_person_layers`]); el sol
//!   igual la ilumina, así que la sombra queda entera. Los brazos, las manos,
//!   el cuello de la camisa, las riendas y el cuello y las orejas del caballo
//!   quedan a la vista. Una cámara de órbita empujada por una pared a menos de
//!   [`HIDE_BODY_WITHIN`] de su cabeza deja de dibujar su modelo, hasta que
//!   vuelve a pasar de [`SHOW_BODY_PAST`] ([`BodyTooClose`]).
//! - **Lente.** Un plano cercano de [`NEAR`] evita que las cosas cercanas
//!   desaparezcan, el campo de visión se amplía a [`FOV`] y la primera cascada
//!   de sombras del sol se acorta ([`CloseShadows`]).
//! - **Alternar.** La cámara se desliza entre la órbita y los ojos durante
//!   [`BLEND_SECONDS`] ([`ViewBlend`]); la lente y las capas cambian a mitad de
//!   camino. La elección se conserva durante la sesión.
//! - **Tren.** A bordo ([`OnTrain`]), `V` sigue recorriendo las vistas del
//!   viaje, cuyo asiento junto a la ventanilla ya es la vista del pasajero
//!   sentado; la elección de primera persona espera, y vuelve cuando el jugador
//!   se baja.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use pampa_core::{Heading, Travel};
use pampa_render::character::viewer::{close_layers, first_person_layers, world_layers};
use pampa_render::{CloseShadows, EYE_IN_HEAD, ViewerHead, VisualPose};

use crate::camera::{CameraInput, CameraRig, CameraTuning, FollowCamera, VIEW_KEY};
use crate::orbit::{OrbitInput, OrbitParams, angle_between};
use crate::player::Player;
use crate::ride::OnTrain;

/// Lo máximo que el jugador mira hacia arriba o hacia abajo, en radianes (80°).
pub const PITCH_LIMIT: f32 = 80.0 * PI / 180.0;
/// Lo máximo que el jinete gira la cabeza respecto del rumbo del caballo, hacia
/// cada lado, en radianes (120°).
pub const RIDING_YAW_LIMIT: f32 = 120.0 * PI / 180.0;
/// Tasa exponencial a la que el cuerpo a pie gira hacia la vista mientras
/// camina (más alta = más rápida).
pub const TURN_RATE: f32 = 6.0;
/// Velocidad sobre el suelo (m/s, absoluta) por encima de la cual el jugador
/// cuenta como caminando.
pub const WALKING: f32 = 0.3;
/// Segundos que tarda la cámara en deslizarse entre la órbita y los ojos.
pub const BLEND_SECONDS: f32 = 0.25;
/// Plano cercano desde los ojos, en metros.
pub const NEAR: f32 = 0.05;
/// Campo de visión vertical desde los ojos, en radianes (75°).
pub const FOV: f32 = 75.0 * PI / 180.0;
/// Tasa exponencial con la que los ojos alcanzan a la cabeza a pie.
pub const FOOT_EYE_RATE: f32 = 14.0;
/// Lo mismo en la montura. Era 8 (el galope apenas movía la vista); una prueba
/// de juego pidió que se sintiera más el andar del caballo.
pub const RIDING_EYE_RATE: f32 = 14.0;
/// Proporción del pitch y del roll de la cabeza del jinete que toma la vista en
/// la montura, para que la caminata y el galope la mezan un poco. A pie, la
/// vista se mantiene nivelada.
pub const RIDING_SWAY: f32 = 0.4;
/// Más lejos que esto de donde estaban, los ojos saltan (metros).
pub const EYE_SNAP: f32 = 1.5;
/// Lo máximo que los ojos suavizados pueden quedar detrás del punto de los ojos
/// de la cabeza, en metros. Al galope los ojos oscilan unos 0.56 m adelante y
/// atrás, 0.09 m arriba y abajo y 0.11 m de lado, a hasta 6 m/s (medido en el
/// jinete incluido:
/// `viewer::tests::the_players_head_is_cut_from_his_skin_onto_the_hidden_layer`),
/// así que un ojo suavizado solo a [`RIDING_EYE_RATE`] quedaba hasta 0.19 m
/// atrás, fuera de la cabeza y más allá del cuello. Dentro de este margen, el
/// suavizado sigue quitando el temblor.
pub const MAX_LAG: f32 = 0.03;
/// Una cámara de órbita empujada (por una pared) más cerca que esto de la
/// cabeza del jugador deja de dibujar el modelo propio del jugador, en metros.
pub const HIDE_BODY_WITHIN: f32 = 0.6;
/// ... y lo vuelve a dibujar una vez más lejos que esto (un margen, para que no
/// parpadee en el borde).
pub const SHOW_BODY_PAST: f32 = 0.8;
/// Altura de los ojos sobre los pies, a pie, mientras el modelo todavía no
/// tiene cabeza.
pub const FOOT_EYE_HEIGHT: f32 = 1.65;
/// Altura de los ojos sobre el suelo bajo el caballo, en la montura, del mismo
/// modo.
pub const RIDING_EYE_HEIGHT: f32 = 2.45;

/// Si la cámara orbita al jugador o mira desde sus ojos. Se conserva durante la
/// sesión; `V` la alterna fuera del tren.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ViewMode {
    #[default]
    ThirdPerson,
    FirstPerson,
}

impl ViewMode {
    /// La otra vista.
    pub fn toggled(self) -> Self {
        match self {
            Self::ThirdPerson => Self::FirstPerson,
            Self::FirstPerson => Self::ThirdPerson,
        }
    }
}

/// Hacia dónde mira el jugador desde sus ojos: `yaw` gira la cabeza respecto
/// del rumbo del cuerpo (positivo hacia la izquierda), `pitch` mira hacia
/// arriba (positivo) o hacia abajo.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct FirstPersonLook {
    pub yaw: f32,
    pub pitch: f32,
}

impl FirstPersonLook {
    /// Gira la cabeza con la entrada de cámara de este frame (el mouse, Q/E).
    /// En la montura, la cabeza se mantiene dentro de [`RIDING_YAW_LIMIT`] del
    /// rumbo del caballo. A pie, caminando (`speed`), el cuerpo gira hacia la
    /// vista; el giro se devuelve, para sumarlo al rumbo, y se le resta a la
    /// cabeza para que la vista no se mueva. Una entrada no finita cuenta como
    /// ninguna.
    pub fn update(
        &mut self,
        input: OrbitInput,
        travel: Travel,
        speed: f32,
        params: &OrbitParams,
        dt: f32,
    ) -> f32 {
        let finite = |v: f32| if v.is_finite() { v } else { 0.0 };
        let dt = finite(dt).max(0.0);
        let turn = finite(input.look.0) * params.mouse_sensitivity
            + finite(input.keys).clamp(-1.0, 1.0) * params.key_rate * dt;
        self.yaw -= turn;
        self.pitch = (self.pitch - finite(input.look.1) * params.mouse_sensitivity)
            .clamp(-PITCH_LIMIT, PITCH_LIMIT);
        match travel {
            Travel::Riding => {
                self.yaw = self.yaw.clamp(-RIDING_YAW_LIMIT, RIDING_YAW_LIMIT);
                0.0
            }
            Travel::OnFoot => {
                self.yaw = angle_between(0.0, self.yaw);
                if finite(speed).abs() <= WALKING {
                    return 0.0;
                }
                let body = self.yaw * (1.0 - (-TURN_RATE * dt).exp());
                self.yaw -= body;
                body
            }
        }
    }

    /// El giro de la cámara para un cuerpo que mira hacia `heading` (radianes,
    /// en sentido antihorario desde el este).
    pub fn rotation(&self, heading: f32) -> Quat {
        // Una cámara mira a lo largo de su -Z: un cuarto de vuelta a la derecha
        // mira al este.
        Quat::from_rotation_y(heading + self.yaw - FRAC_PI_2) * Quat::from_rotation_x(self.pitch)
    }

    /// La mirada que ve lo que ve una cámara de órbita en `orbit_yaw` (su
    /// ángulo en el suelo desde el jugador), nivelada, para un cuerpo que mira
    /// hacia `heading`.
    pub fn from_orbit(orbit_yaw: f32, heading: f32, travel: Travel) -> Self {
        let mut yaw = angle_between(heading, orbit_yaw + PI);
        if travel == Travel::Riding {
            yaw = yaw.clamp(-RIDING_YAW_LIMIT, RIDING_YAW_LIMIT);
        }
        Self { yaw, pitch: 0.0 }
    }
}

/// El desplazamiento suavizado de los ojos respecto de los pies del jugador;
/// `None` hasta que se ubican.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct EyeFollow(pub Option<Vec3>);

impl EyeFollow {
    /// Dónde se ubica la cámara: los pies en `anchor` más el desplazamiento del
    /// `eye` suavizado a `rate`, nunca más de [`MAX_LAG`] detrás de él, o
    /// saltando a él cuando está lejos.
    pub fn follow(&mut self, anchor: Vec3, eye: Vec3, rate: f32, dt: f32) -> Vec3 {
        let offset = eye - anchor;
        let smoothed = match self.0 {
            Some(was) if was.distance(offset) <= EYE_SNAP && dt.is_finite() => {
                let eased = was + (offset - was) * (1.0 - (-rate * dt.max(0.0)).exp());
                let behind = eased - offset;
                if behind.length() > MAX_LAG {
                    offset + behind.normalize() * MAX_LAG
                } else {
                    eased
                }
            }
            _ => offset,
        };
        self.0 = Some(smoothed);
        anchor + smoothed
    }
}

/// Si la cámara de órbita está demasiado cerca de la cabeza del jugador para
/// dibujar su modelo propio ([`HIDE_BODY_WITHIN`], [`SHOW_BODY_PAST`]).
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BodyTooClose(pub bool);

impl BodyTooClose {
    /// Sigue a la cámara a `distance` de la cabeza, con el margen.
    pub fn update(&mut self, distance: f32) {
        if distance < HIDE_BODY_WITHIN {
            self.0 = true;
        } else if distance > SHOW_BODY_PAST {
            self.0 = false;
        }
    }
}

/// Qué tan lejos está la cámara entre la órbita (0) y los ojos (1).
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct ViewBlend(pub f32);

impl ViewBlend {
    /// Se mueve hacia los ojos (`first`) o hacia la órbita durante
    /// [`BLEND_SECONDS`].
    pub fn step(&mut self, first: bool, dt: f32) {
        let step = if dt.is_finite() { dt.max(0.0) } else { 0.0 } / BLEND_SECONDS;
        self.0 = if first { self.0 + step } else { self.0 - step }.clamp(0.0, 1.0);
    }

    /// El peso suavizado de los ojos.
    pub fn weight(self) -> f32 {
        let t = self.0.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    /// Si la lente y las capas son las de los ojos (pasada la mitad).
    pub fn close(self) -> bool {
        self.0 >= 0.5
    }
}

/// La cámara a una fracción `weight` del camino desde la de la `orbit` hasta
/// los ojos en `eye`, girados por `rotation`.
pub fn blend_transform(orbit: &Transform, eye: Vec3, rotation: Quat, weight: f32) -> Transform {
    Transform::from_translation(orbit.translation.lerp(eye, weight))
        .with_rotation(orbit.rotation.slerp(rotation, weight))
}

/// La lente para la transición: el plano cercano de los ojos en cuanto la
/// cámara se acerca, y el campo de visión que se amplía con el peso.
pub fn lens(blend: ViewBlend) -> PerspectiveProjection {
    let orbit = PerspectiveProjection::default();
    if blend.0 <= 0.0 {
        return orbit;
    }
    PerspectiveProjection {
        fov: orbit.fov + (FOV - orbit.fov) * blend.weight(),
        near: NEAR,
        ..orbit
    }
}

/// Da a `projection` el campo de visión y el plano cercano de `wanted`, en el
/// lugar: su relación de aspecto (que Bevy mantiene igual a la del viewport) y
/// su plano lejano quedan como están. Solo la toca cuando cambian, para que la
/// proyección no quede marcada como cambiada en cada frame.
pub fn set_lens(projection: &mut Mut<Projection>, wanted: PerspectiveProjection) {
    match &**projection {
        Projection::Perspective(p) if p.fov == wanted.fov && p.near == wanted.near => {}
        Projection::Perspective(_) => {
            if let Projection::Perspective(p) = &mut **projection {
                p.fov = wanted.fov;
                p.near = wanted.near;
            }
        }
        _ => **projection = Projection::Perspective(wanted),
    }
}

/// `V` fuera del tren alterna la vista. Al entrar en los ojos, la cabeza gira
/// hacia donde miraba la cámara de órbita; al salir, la órbita gira detrás de
/// donde miraban los ojos.
pub(crate) fn toggle_view(
    keys: Res<ButtonInput<KeyCode>>,
    player: Single<(&Player, Has<OnTrain>)>,
    (mut mode, mut look): (ResMut<ViewMode>, ResMut<FirstPersonLook>),
    mut rigs: Query<&mut CameraRig>,
) {
    let (player, aboard) = player.into_inner();
    if aboard || !keys.just_pressed(VIEW_KEY) {
        return;
    }
    *mode = mode.toggled();
    let rider = &player.rider;
    let heading = rider.heading.radians();
    for mut rig in &mut rigs {
        let Some((orbit, _)) = rig.0.as_mut() else {
            continue;
        };
        match *mode {
            ViewMode::FirstPerson => {
                *look = FirstPersonLook::from_orbit(orbit.yaw, heading, rider.travel());
            }
            ViewMode::ThirdPerson => {
                orbit.yaw = (heading + look.yaw + PI).rem_euclid(TAU);
                orbit.idle = 0.0;
            }
        }
    }
}

/// Desde los ojos, la entrada de cámara gira la cabeza, y a pie el cuerpo la
/// sigue mientras camina.
pub(crate) fn turn_to_view(
    time: Res<Time>,
    (mode, input, tuning): (Res<ViewMode>, Res<CameraInput>, Res<CameraTuning>),
    mut look: ResMut<FirstPersonLook>,
    mut players: Query<(&mut Player, &mut VisualPose), Without<OnTrain>>,
) {
    if *mode != ViewMode::FirstPerson {
        return;
    }
    for (mut player, mut pose) in &mut players {
        let rider = &mut player.rider;
        let turn = look.update(
            input.0,
            rider.travel(),
            rider.speed,
            &tuning.0,
            time.delta_secs(),
        );
        if turn != 0.0 {
            rider.heading = Heading::from_radians(rider.heading.radians() + turn);
            pose.heading = rider.heading;
        }
    }
}

/// Lo que [`place_first_person`] conserva entre frames.
type FirstPersonState<'w> = (
    ResMut<'w, ViewBlend>,
    ResMut<'w, EyeFollow>,
    ResMut<'w, CloseShadows>,
    ResMut<'w, BodyTooClose>,
);

/// Los huesos de cabeza marcados y los padres para distinguir el del jugador.
type Heads<'w, 's> = (
    Query<'w, 's, (Entity, &'static GlobalTransform), (With<ViewerHead>, Without<FollowCamera>)>,
    Query<'w, 's, &'static ChildOf>,
);

/// Mueve la cámara (después de la propagación de transforms, para que el hueso
/// de la cabeza tenga la pose de este frame) desde donde la puso la órbita
/// hacia los ojos, y ajusta la lente, las capas y las sombras para la vista. A
/// bordo, las vistas del viaje conservan la cámara y la lente de la órbita;
/// fuera del tren en primera persona, los ojos vuelven de inmediato. Una cámara
/// de órbita empujada contra la cabeza del jugador deja de dibujar su modelo
/// ([`BodyTooClose`]). No reserva memoria.
pub(crate) fn place_first_person(
    time: Res<Time>,
    (mode, look): (Res<ViewMode>, Res<FirstPersonLook>),
    (mut blend, mut eye, mut shadows, mut too_close): FirstPersonState,
    player: Single<(Entity, &Player, &Transform, Has<OnTrain>), Without<FollowCamera>>,
    (heads, parents): Heads,
    camera: Single<
        (
            &mut Transform,
            &mut GlobalTransform,
            &mut Projection,
            &mut RenderLayers,
        ),
        With<FollowCamera>,
    >,
    mut was_aboard: Local<bool>,
) {
    let (me, player, feet, aboard) = player.into_inner();
    let (mut transform, mut global, mut projection, mut layers) = camera.into_inner();
    let dt = time.delta_secs();
    let first = *mode == ViewMode::FirstPerson && !aboard;
    if aboard {
        blend.0 = 0.0;
    } else if *was_aboard && first {
        blend.0 = 1.0;
        eye.0 = None;
    } else {
        blend.step(first, dt);
    }
    *was_aboard = aboard;
    set_lens(&mut projection, lens(*blend));
    let rider = &player.rider;
    let anchor = feet.translation;
    let (height, rate) = match rider.travel() {
        Travel::OnFoot => (FOOT_EYE_HEIGHT, FOOT_EYE_RATE),
        Travel::Riding => (RIDING_EYE_HEIGHT, RIDING_EYE_RATE),
    };
    // La cabeza propia del jugador: render conserva una por cuerpo (el modelo
    // vestido en último lugar), y la de cualquier otro modelo se ignora.
    let own = heads
        .iter()
        .find(|(head, _)| parents.iter_ancestors(*head).any(|a| a == me))
        .map(|(_, head)| head);
    let raw = own.map_or(anchor + Vec3::Y * height, |head| {
        head.transform_point(EYE_IN_HEAD)
    });
    let close = blend.close();
    if aboard {
        too_close.set_if_neq(BodyTooClose(false));
    } else {
        too_close.update(transform.translation.distance(raw));
    }
    let wanted = if close {
        first_person_layers()
    } else if too_close.0 {
        close_layers()
    } else {
        world_layers()
    };
    layers.set_if_neq(wanted);
    shadows.set_if_neq(CloseShadows(close));
    if blend.0 <= 0.0 {
        eye.0 = None;
        return;
    }
    let at = eye.follow(anchor, raw, rate, dt);
    let mut facing = look.rotation(rider.heading.radians());
    if rider.travel() == Travel::Riding
        && let Some(head) = own
    {
        facing = ride_sway(facing, head.rotation(), RIDING_SWAY);
    }
    let placed = blend_transform(&transform, at, facing, blend.weight());
    *transform = placed;
    *global = GlobalTransform::from(placed);
}

/// `facing` mecido por la fracción `share` del pitch y del roll de la cabeza.
/// El hueso de la cabeza mira hacia +Z con +Y hacia arriba, como el modelo.
/// Ambos ángulos se miden en el marco nivelado propio de la cabeza, así que una
/// mirada apartada de la cabeza toma su cabeceo solo como pitch, nunca como
/// roll.
fn ride_sway(facing: Quat, head: Quat, share: f32) -> Quat {
    let forward = head * Vec3::Z;
    let up = head * Vec3::Y;
    let pitch = forward.y.clamp(-1.0, 1.0).asin();
    let level = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    // La mano derecha de un modelo que mira hacia +Z está en su lado -X.
    let right = level.cross(Vec3::Y);
    let roll = up.dot(right).clamp(-1.0, 1.0).asin();
    facing * Quat::from_rotation_x(pitch * share) * Quat::from_rotation_z(-roll * share)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::FollowCameraPlugin;

    #[test]
    fn in_the_saddle_the_view_takes_a_share_of_the_heads_pitch_and_roll() {
        // La vista mira girada 0.7 rad; una cabeza que mira hacia el mismo lado
        // mira hacia +Z donde la cámara mira hacia -Z, de ahí la media vuelta.
        let facing = Quat::from_rotation_y(0.7);
        let head_facing = |yaw: f32| Quat::from_rotation_y(0.7 + PI + yaw);
        // El pitch de la vista y la inclinación de su vector arriba hacia su
        // derecha, respecto de `facing`.
        let rocked = |head: Quat| {
            let view = facing.inverse() * ride_sway(facing, head, RIDING_SWAY);
            ((view * Vec3::NEG_Z).y.asin(), (view * Vec3::Y).x)
        };
        // Una cabeza nivelada deja la vista como está.
        let level = ride_sway(facing, head_facing(0.0), RIDING_SWAY);
        assert!(level.angle_between(facing) < 1e-5);
        // Una cabeza que cabecea 0.2 rad hacia abajo inclina la vista hacia
        // abajo según la fracción.
        let (pitch, lean) = rocked(head_facing(0.0) * Quat::from_rotation_x(0.2));
        assert!((pitch + 0.2 * RIDING_SWAY).abs() < 0.01, "pitch {pitch}");
        assert!(lean.abs() < 1e-3, "lean {lean}");
        // Una cabeza inclinada hacia su derecha (su vector arriba hacia su -X)
        // rota la vista hacia la derecha, según la fracción.
        let (pitch, lean) = rocked(head_facing(0.0) * Quat::from_rotation_z(0.2));
        assert!(
            (lean - (0.2 * RIDING_SWAY).sin()).abs() < 0.01,
            "lean {lean}"
        );
        assert!(pitch.abs() < 1e-3, "pitch {pitch}");
        // Con la mirada girada 0.5 rad respecto de la cabeza, el cabeceo sigue
        // siendo solo pitch: nada se filtra al roll.
        let (pitch, lean) = rocked(head_facing(0.5) * Quat::from_rotation_x(0.2));
        assert!((pitch + 0.2 * RIDING_SWAY).abs() < 0.01, "pitch {pitch}");
        assert!(lean.abs() < 1e-3, "lean {lean}");
    }

    #[test]
    fn on_foot_the_heads_tilt_leaves_the_view_level() {
        let mut app = app(Travel::OnFoot);
        tap(&mut app, VIEW_KEY);
        frames(&mut app, 20);
        let head = GlobalTransform::from(
            Transform::from_translation(feet(&mut app) + Vec3::Y * 1.6)
                .with_rotation(Quat::from_rotation_x(-0.3) * Quat::from_rotation_z(0.2)),
        );
        put_head(&mut app, head);
        frames(&mut app, 60);
        let at = camera(&mut app).0;
        assert!(at.forward().y.abs() < 1e-4, "forward {}", at.forward());
        assert!(at.right().y.abs() < 1e-4, "right {}", at.right());
    }
    use crate::player::{GameWorld, PlayerPlugin, PlayerSpawn};
    use bevy::input::mouse::AccumulatedMouseMotion;
    use bevy::time::TimeUpdateStrategy;
    use bevy::window::{CursorOptions, PrimaryWindow};
    use pampa_core::{Vec2 as GroundVec2, World};
    use pampa_render::{OWN_HEAD_LAYER, world_from_ground};
    use std::time::Duration;

    const FRAME: f32 = 1.0 / 60.0;

    fn app(travel: Travel) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
                FRAME,
            )))
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(GameWorld(World::default()))
            .insert_resource(PlayerSpawn {
                position: GroundVec2::new(3.0, -4.0),
                heading: Heading::from_radians(FRAC_PI_2),
                travel,
            })
            .add_plugins((PlayerPlugin, FollowCameraPlugin));
        app.update();
        app
    }

    fn tap(app: &mut App, key: KeyCode) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(key);
        app.update();
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(key);
        keys.clear();
    }

    fn frames(app: &mut App, n: usize) {
        for _ in 0..n {
            app.update();
        }
    }

    fn camera(app: &mut App) -> (Transform, RenderLayers, PerspectiveProjection) {
        let mut q = app
            .world_mut()
            .query_filtered::<(&Transform, &RenderLayers, &Projection), With<FollowCamera>>();
        let (t, l, p) = q.single(app.world()).unwrap();
        let Projection::Perspective(p) = p else {
            panic!("a perspective camera")
        };
        (*t, l.clone(), p.clone())
    }

    fn rider(app: &mut App) -> pampa_core::Rider {
        let mut q = app.world_mut().query::<&Player>();
        q.single(app.world()).unwrap().rider
    }

    fn feet(app: &mut App) -> Vec3 {
        let r = rider(app);
        world_from_ground(r.position, r.height)
    }

    fn fov_near(lens: &PerspectiveProjection) -> (f32, f32) {
        (lens.fov, lens.near)
    }

    fn sees_own_head(layers: &RenderLayers) -> bool {
        layers.intersects(&RenderLayers::layer(OWN_HEAD_LAYER))
    }

    /// Mueve el mouse durante un frame, tal como lo transmite una ventana con
    /// foco y el cursor capturado.
    fn mouse(app: &mut App, delta: Vec2) {
        let world = app.world_mut();
        let mut windows = world.query_filtered::<(), With<PrimaryWindow>>();
        if windows.iter(world).next().is_none() {
            world.spawn((Window::default(), CursorOptions::default(), PrimaryWindow));
        }
        app.insert_resource(AccumulatedMouseMotion { delta });
        app.update();
        app.update();
        app.insert_resource(AccumulatedMouseMotion::default());
    }

    #[test]
    fn v_toggles_between_the_orbit_and_the_eyes_and_glides() {
        let mut app = app(Travel::OnFoot);
        assert_eq!(*app.world().resource::<ViewMode>(), ViewMode::ThirdPerson);
        let orbit = camera(&mut app).0.translation;
        assert!(orbit.distance(feet(&mut app)) > 5.0, "out on the orbit");
        tap(&mut app, VIEW_KEY);
        assert_eq!(*app.world().resource::<ViewMode>(), ViewMode::FirstPerson);
        // En el camino de entrada, entre la órbita y los ojos.
        frames(&mut app, 5);
        let eyes = feet(&mut app) + Vec3::Y * FOOT_EYE_HEIGHT;
        let midway = camera(&mut app).0.translation;
        assert!(midway.distance(eyes) > 0.1 && midway.distance(eyes) < orbit.distance(eyes));
        frames(&mut app, 20);
        let (at, layers, lens) = camera(&mut app);
        assert!(at.translation.distance(eyes) < 1e-3, "{}", at.translation);
        assert_eq!(lens.near, NEAR);
        assert!((lens.fov - FOV).abs() < 1e-5);
        assert!(!sees_own_head(&layers), "no own head from the eyes");
        assert!(app.world().resource::<CloseShadows>().0);
        // Mirando hacia donde mira el jugador, nivelada.
        let forward = at.forward();
        assert!(forward.distance(Vec3::NEG_Z) < 1e-3, "{forward}");
        // Y de vuelta afuera, detrás del jugador otra vez, con la cabeza
        // dibujada.
        tap(&mut app, VIEW_KEY);
        frames(&mut app, 20);
        assert_eq!(*app.world().resource::<ViewMode>(), ViewMode::ThirdPerson);
        let (at, layers, lens) = camera(&mut app);
        assert!(at.translation.distance(feet(&mut app)) > 5.0);
        assert!(
            at.translation.z > feet(&mut app).z,
            "behind a player facing north"
        );
        assert!(sees_own_head(&layers), "the head drawn from the orbit");
        assert_eq!(fov_near(&lens), fov_near(&PerspectiveProjection::default()));
        assert!(!app.world().resource::<CloseShadows>().0);
    }

    #[test]
    fn the_glide_keeps_the_viewports_aspect_ratio_and_the_far_plane() {
        let mut app = app(Travel::OnFoot);
        // Como los fija Bevy a partir de la ventana y como puede fijarlos una
        // cámara.
        let (aspect, far) = (2.25, 1234.0);
        {
            let world = app.world_mut();
            let mut q = world.query_filtered::<&mut Projection, With<FollowCamera>>();
            let mut projection = q.single_mut(world).unwrap();
            let Projection::Perspective(p) = &mut *projection else {
                panic!("a perspective camera")
            };
            p.aspect_ratio = aspect;
            p.far = far;
        }
        let kept = |app: &mut App, when: &str| {
            let lens = camera(app).2;
            assert_eq!((lens.aspect_ratio, lens.far), (aspect, far), "{when}");
        };
        tap(&mut app, VIEW_KEY);
        frames(&mut app, 5);
        kept(&mut app, "gliding in");
        frames(&mut app, 20);
        let lens = camera(&mut app).2;
        assert_eq!(lens.near, NEAR);
        assert!((lens.fov - FOV).abs() < 1e-5);
        kept(&mut app, "at the eyes");
        tap(&mut app, VIEW_KEY);
        frames(&mut app, 5);
        kept(&mut app, "gliding out");
        frames(&mut app, 20);
        assert_eq!(
            fov_near(&camera(&mut app).2),
            fov_near(&PerspectiveProjection::default())
        );
        kept(&mut app, "back on the orbit");
    }

    #[test]
    fn the_eyes_follow_the_head_bone() {
        let mut app = app(Travel::Riding);
        tap(&mut app, VIEW_KEY);
        frames(&mut app, 20);
        // La cabeza del modelo, un poco inclinada hacia adelante, sobre la
        // montura.
        let head = GlobalTransform::from(
            Transform::from_translation(feet(&mut app) + Vec3::new(0.2, 2.3, -0.1))
                .with_rotation(Quat::from_rotation_x(-0.3) * Quat::from_rotation_y(0.4)),
        );
        let bone = put_head(&mut app, head);
        frames(&mut app, 60);
        let eye = head.transform_point(EYE_IN_HEAD);
        let at = camera(&mut app).0;
        assert!(
            at.translation.distance(eye) < 1e-3,
            "{} vs {eye}",
            at.translation
        );
        // En la montura, la vista toma RIDING_SWAY del pitch de la cabeza.
        let head_pitch = (head.rotation() * Vec3::Z).y.asin();
        let view_pitch = at.forward().y.asin();
        assert!(
            (view_pitch - head_pitch * RIDING_SWAY).abs() < 1e-3,
            "view {view_pitch} vs head {head_pitch}"
        );
        // Una pequeña sacudida de la cabeza se suaviza; una grande se sigue de
        // inmediato hasta quedar dentro de MAX_LAG.
        let nudge = GlobalTransform::from(
            head.compute_transform()
                .with_translation(head.translation() + Vec3::Y * 0.02),
        );
        assert_eq!(put_head(&mut app, nudge), bone);
        app.update();
        let moved = camera(&mut app).0.translation.y - at.translation.y;
        assert!(moved > 0.0 && moved < 0.01, "eased: {moved}");
        frames(&mut app, 60);
        let at = camera(&mut app).0;
        let jolt = GlobalTransform::from(
            head.compute_transform()
                .with_translation(head.translation() + Vec3::Y * 0.12),
        );
        assert_eq!(put_head(&mut app, jolt), bone);
        app.update();
        let moved = camera(&mut app).0.translation.y - at.translation.y;
        assert!(
            (moved - (0.1 - MAX_LAG)).abs() < 1e-4,
            "at most MAX_LAG behind: {moved}"
        );
        frames(&mut app, 60);
        let eye = jolt.transform_point(EYE_IN_HEAD);
        assert!(camera(&mut app).0.translation.distance(eye) < 1e-3);
    }

    #[test]
    fn pitch_is_clamped() {
        let params = OrbitParams::default();
        let mut look = FirstPersonLook::default();
        let down = OrbitInput {
            look: (0.0, 1e6),
            ..default()
        };
        look.update(down, Travel::OnFoot, 0.0, &params, FRAME);
        assert_eq!(look.pitch, -PITCH_LIMIT, "mouse down looks down");
        let up = OrbitInput {
            look: (0.0, -1e6),
            ..default()
        };
        look.update(up, Travel::Riding, 0.0, &params, FRAME);
        assert_eq!(look.pitch, PITCH_LIMIT);
        // También en la app, la cámara no se inclina más allá del límite.
        let mut app = app(Travel::OnFoot);
        tap(&mut app, VIEW_KEY);
        mouse(&mut app, Vec2::new(0.0, -1e6));
        frames(&mut app, 20);
        assert_eq!(app.world().resource::<FirstPersonLook>().pitch, PITCH_LIMIT);
        let up = camera(&mut app).0.forward().y.asin();
        assert!((up - PITCH_LIMIT).abs() < 1e-3, "{up}");
    }

    #[test]
    fn in_the_saddle_the_head_turns_within_reach_of_the_horses_heading() {
        let params = OrbitParams::default();
        let mut look = FirstPersonLook::default();
        let right = OrbitInput {
            look: (1e6, 0.0),
            ..default()
        };
        let turn = look.update(right, Travel::Riding, 8.0, &params, FRAME);
        assert_eq!((look.yaw, turn), (-RIDING_YAW_LIMIT, 0.0));
        let left = OrbitInput {
            keys: -1.0,
            ..default()
        };
        for _ in 0..1000 {
            look.update(left, Travel::Riding, 8.0, &params, FRAME);
        }
        assert_eq!(look.yaw, RIDING_YAW_LIMIT);
        // Hacia los ojos desde una órbita apartada del frente del caballo:
        // también limitada.
        let front = FirstPersonLook::from_orbit(FRAC_PI_2, FRAC_PI_2, Travel::Riding);
        assert_eq!(front.yaw.abs(), RIDING_YAW_LIMIT);
        // En la app, E gira la cabeza hasta su límite; el caballo conserva su
        // rumbo.
        let mut app = app(Travel::Riding);
        tap(&mut app, VIEW_KEY);
        let heading = rider(&mut app).heading;
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyE);
        frames(&mut app, 200);
        assert_eq!(
            app.world().resource::<FirstPersonLook>().yaw,
            -RIDING_YAW_LIMIT
        );
        assert_eq!(rider(&mut app).heading, heading);
        let forward = camera(&mut app).0.forward();
        let bearing = (-forward.z).atan2(forward.x);
        let off = angle_between(heading.radians(), bearing);
        assert!((off + RIDING_YAW_LIMIT).abs() < 1e-3, "{off}");
    }

    #[test]
    fn on_foot_the_body_turns_to_the_view_only_while_walking() {
        let params = OrbitParams::default();
        let mut look = FirstPersonLook {
            yaw: 1.0,
            pitch: 0.0,
        };
        assert_eq!(
            look.update(OrbitInput::default(), Travel::OnFoot, 0.0, &params, FRAME),
            0.0
        );
        assert_eq!(look.yaw, 1.0, "standing: free to look round");
        let mut turned = 0.0;
        for _ in 0..120 {
            turned += look.update(OrbitInput::default(), Travel::OnFoot, 1.4, &params, FRAME);
        }
        assert!(
            (turned - 1.0).abs() < 1e-3 && look.yaw.abs() < 1e-3,
            "{turned}"
        );
        // En la app: mirar a la izquierda parado y luego caminar; el cuerpo
        // gira.
        let mut app = app(Travel::OnFoot);
        tap(&mut app, VIEW_KEY);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyQ);
        frames(&mut app, 30);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::KeyQ);
        let view =
            rider(&mut app).heading.radians() + app.world().resource::<FirstPersonLook>().yaw;
        assert_eq!(
            rider(&mut app).heading.radians(),
            FRAC_PI_2,
            "still standing"
        );
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        frames(&mut app, 120);
        let heading = rider(&mut app).heading.radians();
        assert!(
            angle_between(view, heading).abs() < 0.02,
            "{heading} vs {view}"
        );
    }

    #[test]
    fn aboard_v_keeps_cycling_the_ride_views_and_the_eyes_come_back_off_the_train() {
        use crate::camera::RideView;
        let mut app = crate::ride::testing::shipped_app();
        app.add_plugins(FollowCameraPlugin);
        app.update();
        assert!(
            crate::ride::testing::passenger(&mut app).1.is_some(),
            "aboard"
        );
        // A bordo, V es del viaje: el modo de vista no cambia.
        tap(&mut app, VIEW_KEY);
        app.update();
        assert_eq!(*app.world().resource::<RideView>(), RideView::Coach);
        assert_eq!(*app.world().resource::<ViewMode>(), ViewMode::ThirdPerson);
        tap(&mut app, VIEW_KEY);
        tap(&mut app, VIEW_KEY);
        app.update();
        assert_eq!(*app.world().resource::<RideView>(), RideView::Window);
        // Primera persona elegida antes de subir: el asiento conserva la
        // cámara, la lente de la órbita y el mundo entero dibujado.
        app.insert_resource(ViewMode::FirstPerson);
        frames(&mut app, 5);
        let (_, layers, lens) = camera(&mut app);
        assert_eq!(app.world().resource::<ViewBlend>().0, 0.0);
        assert!(sees_own_head(&layers));
        assert_eq!(fov_near(&lens), fov_near(&PerspectiveProjection::default()));
        // Fuera del tren: directo a los ojos, a pie.
        crate::ride::testing::arrive_from_anywhere(&mut app);
        app.update();
        assert_eq!(app.world().resource::<ViewBlend>().0, 1.0);
        let (at, layers, lens) = camera(&mut app);
        let eyes = feet(&mut app) + Vec3::Y * FOOT_EYE_HEIGHT;
        assert!(
            at.translation.distance(eyes) < 0.05,
            "{} vs {eyes}",
            at.translation
        );
        assert!(!sees_own_head(&layers));
        assert_eq!(lens.near, NEAR);
        // Y V vuelve a alternar en el suelo.
        tap(&mut app, VIEW_KEY);
        assert_eq!(*app.world().resource::<ViewMode>(), ViewMode::ThirdPerson);
    }

    /// Pone el hueso de la cabeza del jugador en `head` en el mundo, bajo el
    /// jugador como está el hueso de su modelo (creándolo la primera vez);
    /// devuelve el hueso. Estas apps no propagan transforms: el transform
    /// global del hueso se fija como lo dejaría la propagación.
    fn put_head(app: &mut App, head: GlobalTransform) -> Entity {
        let world = app.world_mut();
        let me = {
            let mut q = world.query_filtered::<Entity, With<Player>>();
            q.single(world).unwrap()
        };
        let mut bones = world.query_filtered::<(Entity, &ChildOf), With<ViewerHead>>();
        let found = bones
            .iter(world)
            .find(|(_, parent)| parent.parent() == me)
            .map(|(bone, _)| bone);
        match found {
            Some(bone) => {
                world.entity_mut(bone).insert(head);
                bone
            }
            None => world.spawn((ViewerHead, head, ChildOf(me))).id(),
        }
    }

    /// Pone la cabeza del jugador de modo que el punto de sus ojos quede en
    /// `eye`, erguida.
    fn head_at(app: &mut App, eye: Vec3) {
        put_head(app, GlobalTransform::from_translation(eye - EYE_IN_HEAD));
    }

    #[test]
    fn the_eyes_sit_in_the_players_own_head_not_anothers() {
        let mut app = app(Travel::OnFoot);
        tap(&mut app, VIEW_KEY);
        frames(&mut app, 20);
        // El hueso de la cabeza de otro modelo, marcado pero no del jugador (un
        // modelo que se está reemplazando, o uno perdido), primero allí.
        let stray = feet(&mut app) + Vec3::new(4.0, 1.6, 0.0);
        let other = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((
            ViewerHead,
            GlobalTransform::from_translation(stray - EYE_IN_HEAD),
            ChildOf(other),
        ));
        let eye = feet(&mut app) + Vec3::new(0.0, 1.6, 0.05);
        head_at(&mut app, eye);
        frames(&mut app, 60);
        let at = camera(&mut app).0.translation;
        assert!(at.distance(eye) < 1e-3, "{at} vs {eye}, stray at {stray}");
    }

    #[test]
    fn at_the_gallop_the_eyes_never_fall_behind_the_head() {
        let mut app = app(Travel::Riding);
        tap(&mut app, VIEW_KEY);
        frames(&mut app, 20);
        // El punto de los ojos oscilando como se midió en el jinete incluido al
        // galope: 0.56 m adelante y atrás, 0.09 m arriba y abajo, 0.11 m de
        // lado, a hasta 6 m/s.
        let seat = feet(&mut app) + Vec3::new(0.0, 2.3, 0.0);
        let stride = std::f32::consts::TAU * 3.4;
        let eye_at = |t: f32| {
            seat + Vec3::new(
                0.28 * (stride * t).sin(),
                0.045 * (2.0 * stride * t).sin(),
                0.055 * (stride * t).cos(),
            )
        };
        let mut worst = 0.0_f32;
        for k in 0..240 {
            let eye = eye_at(k as f32 * FRAME);
            head_at(&mut app, eye);
            app.update();
            let at = camera(&mut app).0.translation;
            worst = worst.max(at.distance(eye));
        }
        println!("gallop: the camera at most {worst:.4} m from the eye point");
        assert!(worst <= MAX_LAG + 1e-4, "{worst}");
        assert!(worst > 0.5 * MAX_LAG, "still eased: {worst}");
    }

    #[test]
    fn an_orbit_camera_against_the_player_stops_drawing_him() {
        let mut app = app(Travel::OnFoot);
        let layers = |app: &mut App| camera(app).1;
        assert_eq!(layers(&mut app), world_layers());
        let at = camera(&mut app).0.translation;
        // Una pared empuja la cámara a 0.3 m de su cabeza: sin modelo propio.
        head_at(&mut app, at + Vec3::X * 0.3);
        app.update();
        assert_eq!(layers(&mut app), close_layers());
        // Al volver a alejarse: todavía oculto dentro del margen, dibujado más
        // allá.
        let at = camera(&mut app).0.translation;
        head_at(&mut app, at + Vec3::X * 0.7);
        app.update();
        assert_eq!(layers(&mut app), close_layers(), "inside the gap");
        let at = camera(&mut app).0.translation;
        head_at(&mut app, at + Vec3::X * 1.0);
        app.update();
        assert_eq!(layers(&mut app), world_layers());
        let at = camera(&mut app).0.translation;
        head_at(&mut app, at + Vec3::X * 0.7);
        app.update();
        assert_eq!(layers(&mut app), world_layers(), "inside the gap");
        // El sol todavía ve el cuerpo: solo la cámara lo descarta.
        assert!(!app.world().resource::<CloseShadows>().0);
    }
}
