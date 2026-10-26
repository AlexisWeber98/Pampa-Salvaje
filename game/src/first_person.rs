//! First-person view from the player's eyes, toggled with `V` against the
//! third-person orbit ([`ViewMode`]), to see the world's details up close.
//!
//! - **Eyes.** The camera sits at [`EYE_IN_HEAD`] in the frame of the
//!   player's model's `head` bone ([`ViewerHead`], marked by render), on
//!   foot following the walk and in the saddle following the seated rider.
//!   Its offset from the player's feet is smoothed ([`EyeFollow`]), which
//!   takes the jitter off the walk and the gallop, but never lets the eyes
//!   fall more than [`MAX_LAG`] behind the head; the view's turn comes from
//!   the look alone, never from the head, so it stays level. Until the model
//!   is in (or without one) the eyes stand at a fixed height.
//! - **Look.** The mouse (or Q/E) turns the head ([`FirstPersonLook`]),
//!   up and down within [`PITCH_LIMIT`]. On foot the body turns to follow
//!   the view while walking; in the saddle the horse steers as before and
//!   the head turns at most [`RIDING_YAW_LIMIT`] either way from its
//!   heading. The keyboard drives the player as always.
//! - **Own body.** Render cuts the player's face and skull out of his
//!   skin and puts them, with the hat, hair and eyes, on a layer the
//!   first-person camera does not draw ([`first_person_layers`]); the sun
//!   still lights it, so the whole shadow stays. The arms, the hands, the
//!   collar, the reins and the horse's neck and ears stay in view. An
//!   orbit camera pulled in by a wall to within [`HIDE_BODY_WITHIN`] of
//!   his head stops drawing his model, until it is past [`SHOW_BODY_PAST`]
//!   again ([`BodyTooClose`]).
//! - **Lens.** A near plane of [`NEAR`] keeps close things from vanishing,
//!   the field of view widens to [`FOV`] and the sun's first shadow
//!   cascade shortens ([`CloseShadows`]).
//! - **Toggle.** The camera glides between the orbit and the eyes over
//!   [`BLEND_SECONDS`] ([`ViewBlend`]); the lens and layers switch halfway.
//!   The choice is kept for the session.
//! - **Train.** Aboard ([`OnTrain`]) `V` keeps cycling the ride views,
//!   whose window seat is the seated passenger's view already; the
//!   first-person choice waits, and comes back when the player gets off.

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

/// Farthest the player looks up or down, in radians (80°).
pub const PITCH_LIMIT: f32 = 80.0 * PI / 180.0;
/// Farthest the rider turns their head from the horse's heading, either
/// way, in radians (120°).
pub const RIDING_YAW_LIMIT: f32 = 120.0 * PI / 180.0;
/// Exponential rate at which the body on foot turns to the view while
/// walking (higher = quicker).
pub const TURN_RATE: f32 = 6.0;
/// Ground speed (m/s, absolute) above which the player counts as walking.
pub const WALKING: f32 = 0.3;
/// Seconds the camera takes to glide between the orbit and the eyes.
pub const BLEND_SECONDS: f32 = 0.25;
/// Near plane from the eyes, in meters.
pub const NEAR: f32 = 0.05;
/// Vertical field of view from the eyes, in radians (75°).
pub const FOV: f32 = 75.0 * PI / 180.0;
/// Exponential rate of the eyes catching up with the head on foot.
pub const FOOT_EYE_RATE: f32 = 14.0;
/// The same in the saddle: slower, so the gallop sways the view less.
pub const RIDING_EYE_RATE: f32 = 8.0;
/// Farther than this from where they were, the eyes jump (meters).
pub const EYE_SNAP: f32 = 1.5;
/// Farthest the eased eyes may fall behind the head's eye point, in meters.
/// At the gallop the eyes surge about 0.56 m fore and aft, 0.09 m up and
/// down and 0.11 m across, at up to 6 m/s (measured on the shipped rider:
/// `viewer::tests::the_players_head_is_cut_from_his_skin_onto_the_hidden_layer`),
/// so an eye eased at [`RIDING_EYE_RATE`] alone fell up to 0.19 m behind,
/// out of the head and past the neck. Within this the easing still takes
/// the jitter off.
pub const MAX_LAG: f32 = 0.03;
/// An orbit camera pulled in nearer than this to the player's head (by a
/// wall) stops drawing the player's own model, in meters.
pub const HIDE_BODY_WITHIN: f32 = 0.6;
/// ... and draws it again once farther than this (a gap, so it does not
/// flicker at the edge).
pub const SHOW_BODY_PAST: f32 = 0.8;
/// Eye height above the feet on foot while the model has no head yet.
pub const FOOT_EYE_HEIGHT: f32 = 1.65;
/// Eye height above the ground under the horse, in the saddle, likewise.
pub const RIDING_EYE_HEIGHT: f32 = 2.45;

/// Whether the camera orbits the player or looks from their eyes. Kept
/// for the session; `V` toggles it off the train.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ViewMode {
    #[default]
    ThirdPerson,
    FirstPerson,
}

impl ViewMode {
    /// The other view.
    pub fn toggled(self) -> Self {
        match self {
            Self::ThirdPerson => Self::FirstPerson,
            Self::FirstPerson => Self::ThirdPerson,
        }
    }
}

/// Where the player looks from their eyes: `yaw` turns the head from the
/// body's heading (positive to the left), `pitch` looks up (positive) or
/// down.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct FirstPersonLook {
    pub yaw: f32,
    pub pitch: f32,
}

impl FirstPersonLook {
    /// Turns the head with this frame's camera input (the mouse, Q/E). In
    /// the saddle the head stays within [`RIDING_YAW_LIMIT`] of the
    /// horse's heading. On foot, walking (`speed`), the body turns toward
    /// the view; the turn is returned, to add to the heading, and taken off
    /// the head so the view stays put. Non-finite input counts as none.
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

    /// The camera's turn for a body facing `heading` (radians,
    /// counter-clockwise from east).
    pub fn rotation(&self, heading: f32) -> Quat {
        // A camera looks down its -Z: a quarter turn right faces east.
        Quat::from_rotation_y(heading + self.yaw - FRAC_PI_2) * Quat::from_rotation_x(self.pitch)
    }

    /// The look that sees what an orbit camera at `orbit_yaw` (its ground
    /// angle from the player) sees, level, for a body facing `heading`.
    pub fn from_orbit(orbit_yaw: f32, heading: f32, travel: Travel) -> Self {
        let mut yaw = angle_between(heading, orbit_yaw + PI);
        if travel == Travel::Riding {
            yaw = yaw.clamp(-RIDING_YAW_LIMIT, RIDING_YAW_LIMIT);
        }
        Self { yaw, pitch: 0.0 }
    }
}

/// The eyes' smoothed offset from the player's feet; `None` until placed.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct EyeFollow(pub Option<Vec3>);

impl EyeFollow {
    /// Where the camera sits: the feet at `anchor` plus the offset of the
    /// `eye` eased in at `rate`, never more than [`MAX_LAG`] behind it, or
    /// jumped to when far off.
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

/// Whether the orbit camera is too close to the player's head to draw his
/// own model ([`HIDE_BODY_WITHIN`], [`SHOW_BODY_PAST`]).
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BodyTooClose(pub bool);

impl BodyTooClose {
    /// Follows the camera at `distance` from the head, with the gap.
    pub fn update(&mut self, distance: f32) {
        if distance < HIDE_BODY_WITHIN {
            self.0 = true;
        } else if distance > SHOW_BODY_PAST {
            self.0 = false;
        }
    }
}

/// How far the camera is from the orbit (0) to the eyes (1).
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct ViewBlend(pub f32);

impl ViewBlend {
    /// Moves toward the eyes (`first`) or the orbit over [`BLEND_SECONDS`].
    pub fn step(&mut self, first: bool, dt: f32) {
        let step = if dt.is_finite() { dt.max(0.0) } else { 0.0 } / BLEND_SECONDS;
        self.0 = if first { self.0 + step } else { self.0 - step }.clamp(0.0, 1.0);
    }

    /// The eased weight of the eyes.
    pub fn weight(self) -> f32 {
        let t = self.0.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    /// Whether the lens and layers are the eyes' (past halfway).
    pub fn close(self) -> bool {
        self.0 >= 0.5
    }
}

/// The camera `weight` of the way from the `orbit`'s to the eyes at `eye`
/// turned by `rotation`.
pub fn blend_transform(orbit: &Transform, eye: Vec3, rotation: Quat, weight: f32) -> Transform {
    Transform::from_translation(orbit.translation.lerp(eye, weight))
        .with_rotation(orbit.rotation.slerp(rotation, weight))
}

/// The lens for the blend: the eyes' near plane as soon as the camera
/// moves in, the field of view widening with the weight.
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

/// `V` off the train toggles the view. Into the eyes, the head turns to
/// where the orbit camera looked; back out, the orbit swings behind where
/// the eyes looked.
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

/// From the eyes, the camera input turns the head, and on foot the body
/// follows it while walking.
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

/// What [`place_first_person`] keeps between frames.
type FirstPersonState<'w> = (
    ResMut<'w, ViewBlend>,
    ResMut<'w, EyeFollow>,
    ResMut<'w, CloseShadows>,
    ResMut<'w, BodyTooClose>,
);

/// Moves the camera (after transform propagation, so the head bone has
/// this frame's pose) from where the orbit put it toward the eyes, and
/// sets the lens, the layers and the shadows for the view. Aboard, the
/// ride views keep the camera and the orbit's lens; off the train in the
/// first person, the eyes are back at once. An orbit camera pulled in
/// against the player's head stops drawing his model ([`BodyTooClose`]).
/// Allocates nothing.
pub(crate) fn place_first_person(
    time: Res<Time>,
    (mode, look): (Res<ViewMode>, Res<FirstPersonLook>),
    (mut blend, mut eye, mut shadows, mut too_close): FirstPersonState,
    player: Single<(&Player, &Transform, Has<OnTrain>), Without<FollowCamera>>,
    heads: Query<&GlobalTransform, (With<ViewerHead>, Without<FollowCamera>)>,
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
    let (player, feet, aboard) = player.into_inner();
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
    let wanted = lens(*blend);
    if !matches!(&*projection, Projection::Perspective(p) if p.fov == wanted.fov && p.near == wanted.near)
    {
        *projection = Projection::Perspective(wanted);
    }
    let rider = &player.rider;
    let anchor = feet.translation;
    let (height, rate) = match rider.travel() {
        Travel::OnFoot => (FOOT_EYE_HEIGHT, FOOT_EYE_RATE),
        Travel::Riding => (RIDING_EYE_HEIGHT, RIDING_EYE_RATE),
    };
    let raw = heads
        .iter()
        .next()
        .map_or(anchor + Vec3::Y * height, |head| {
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
    let placed = blend_transform(
        &transform,
        at,
        look.rotation(rider.heading.radians()),
        blend.weight(),
    );
    *transform = placed;
    *global = GlobalTransform::from(placed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::FollowCameraPlugin;
    use crate::player::{GameWorld, PlayerPlugin, PlayerSpawn};
    use bevy::input::mouse::AccumulatedMouseMotion;
    use bevy::time::TimeUpdateStrategy;
    use bevy::window::{CursorOptions, PrimaryWindow};
    use pampa_core::{Vec2 as GroundVec2, World};
    use pampa_render::{OWN_HEAD_LAYER, world_from_ground};
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;
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

    /// Moves the mouse for one frame, as a focused window with the cursor
    /// grabbed passes it on.
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
        // On the way in, between the orbit and the eyes.
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
        // Facing the way the player faces, level.
        let forward = at.forward();
        assert!(forward.distance(Vec3::NEG_Z) < 1e-3, "{forward}");
        // And back out, behind the player again, the head drawn.
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
    fn the_eyes_follow_the_head_bone() {
        let mut app = app(Travel::Riding);
        tap(&mut app, VIEW_KEY);
        frames(&mut app, 20);
        // The model's head, tipped forward a little, over the saddle.
        let head = GlobalTransform::from(
            Transform::from_translation(feet(&mut app) + Vec3::new(0.2, 2.3, -0.1))
                .with_rotation(Quat::from_rotation_x(-0.3) * Quat::from_rotation_y(0.4)),
        );
        let bone = app.world_mut().spawn((ViewerHead, head)).id();
        frames(&mut app, 60);
        let eye = head.transform_point(EYE_IN_HEAD);
        let at = camera(&mut app).0;
        assert!(
            at.translation.distance(eye) < 1e-3,
            "{} vs {eye}",
            at.translation
        );
        // The head's tilt does not tip the view: it stays level.
        assert!(at.forward().y.abs() < 1e-4);
        // A small jolt of the head is eased, a big one followed to within
        // MAX_LAG at once.
        let nudge = GlobalTransform::from(
            head.compute_transform()
                .with_translation(head.translation() + Vec3::Y * 0.02),
        );
        app.world_mut().entity_mut(bone).insert(nudge);
        app.update();
        let moved = camera(&mut app).0.translation.y - at.translation.y;
        assert!(moved > 0.0 && moved < 0.01, "eased: {moved}");
        frames(&mut app, 60);
        let at = camera(&mut app).0;
        let jolt = GlobalTransform::from(
            head.compute_transform()
                .with_translation(head.translation() + Vec3::Y * 0.12),
        );
        app.world_mut().entity_mut(bone).insert(jolt);
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
        // In the app too, the camera tips no further than the limit.
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
        // Into the eyes from an orbit off the horse's front: clamped too.
        let front = FirstPersonLook::from_orbit(FRAC_PI_2, FRAC_PI_2, Travel::Riding);
        assert_eq!(front.yaw.abs(), RIDING_YAW_LIMIT);
        // In the app, E turns the head to its limit; the horse keeps its
        // heading.
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
        // In the app: look left standing, then walk; the body comes round.
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
        // Aboard, V is the ride's: the view mode stays.
        tap(&mut app, VIEW_KEY);
        app.update();
        assert_eq!(*app.world().resource::<RideView>(), RideView::Coach);
        assert_eq!(*app.world().resource::<ViewMode>(), ViewMode::ThirdPerson);
        tap(&mut app, VIEW_KEY);
        tap(&mut app, VIEW_KEY);
        app.update();
        assert_eq!(*app.world().resource::<RideView>(), RideView::Window);
        // First person chosen before boarding: the seat keeps the camera,
        // the orbit's lens and the whole world drawn.
        app.insert_resource(ViewMode::FirstPerson);
        frames(&mut app, 5);
        let (_, layers, lens) = camera(&mut app);
        assert_eq!(app.world().resource::<ViewBlend>().0, 0.0);
        assert!(sees_own_head(&layers));
        assert_eq!(fov_near(&lens), fov_near(&PerspectiveProjection::default()));
        // Off the train: straight into the eyes, on foot.
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
        // And V toggles again on the ground.
        tap(&mut app, VIEW_KEY);
        assert_eq!(*app.world().resource::<ViewMode>(), ViewMode::ThirdPerson);
    }

    /// Counts this thread's heap allocations, for
    /// [`a_frame_from_the_eyes_allocates_nothing`].
    struct Counting;

    thread_local! {
        static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
    }

    fn count() {
        let _ = ALLOCATIONS.try_with(|n| n.set(n.get() + 1));
    }

    // SAFETY: every call is passed straight on to the system allocator.
    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            count();
            // SAFETY: the caller's contract, passed on.
            unsafe { System.alloc(layout) }
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            // SAFETY: the caller's contract, passed on.
            unsafe { System.dealloc(ptr, layout) }
        }
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            count();
            // SAFETY: the caller's contract, passed on.
            unsafe { System.alloc_zeroed(layout) }
        }
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            count();
            // SAFETY: the caller's contract, passed on.
            unsafe { System.realloc(ptr, layout, size) }
        }
    }

    #[global_allocator]
    static COUNTING: Counting = Counting;

    #[test]
    fn a_frame_from_the_eyes_allocates_nothing() {
        let params = OrbitParams::default();
        let mut look = FirstPersonLook::default();
        let mut eye = EyeFollow::default();
        let mut blend = ViewBlend::default();
        let orbit = Transform::from_xyz(0.0, 3.0, 9.0).looking_at(Vec3::Y, Vec3::Y);
        let mut sum = Vec3::ZERO;
        let before = ALLOCATIONS.with(Cell::get);
        for k in 0..600 {
            let t = k as f32 * FRAME;
            let input = OrbitInput {
                look: (t.sin() * 4.0, t.cos()),
                keys: 0.0,
                zoom: 0.0,
            };
            let travel = if k < 300 {
                Travel::OnFoot
            } else {
                Travel::Riding
            };
            let turn = look.update(input, travel, 1.4, &params, FRAME);
            blend.step(true, FRAME);
            let anchor = Vec3::new(t, 0.0, -t);
            let at = eye.follow(
                anchor,
                anchor + Vec3::new(0.0, 1.6 + t.sin() * 0.03, 0.1),
                14.0,
                FRAME,
            );
            let wide = lens(blend);
            let placed = blend_transform(&orbit, at, look.rotation(turn), blend.weight());
            sum += placed.translation + Vec3::splat(wide.fov);
        }
        let after = ALLOCATIONS.with(Cell::get);
        assert!(sum.is_finite());
        assert_eq!(after - before, 0, "allocations in 600 frames");
    }

    /// Puts the player's head so its eye point is at `eye`, upright.
    fn head_at(app: &mut App, eye: Vec3) {
        let head = GlobalTransform::from_translation(eye - EYE_IN_HEAD);
        let world = app.world_mut();
        let found = world
            .query_filtered::<Entity, With<ViewerHead>>()
            .iter(world)
            .next();
        match found {
            Some(bone) => {
                world.entity_mut(bone).insert(head);
            }
            None => {
                world.spawn((ViewerHead, head));
            }
        }
    }

    #[test]
    fn at_the_gallop_the_eyes_never_fall_behind_the_head() {
        let mut app = app(Travel::Riding);
        tap(&mut app, VIEW_KEY);
        frames(&mut app, 20);
        // The eye point surging as measured on the shipped rider at the
        // gallop: 0.56 m fore and aft, 0.09 m up and down, 0.11 m across,
        // up to 6 m/s.
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
        // A wall pulls the camera in to 0.3 m of his head: no own model.
        head_at(&mut app, at + Vec3::X * 0.3);
        app.update();
        assert_eq!(layers(&mut app), close_layers());
        // Easing back out: still hidden inside the gap, drawn past it.
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
        // The sun still sees the body: only the camera drops it.
        assert!(!app.world().resource::<CloseShadows>().0);
    }
}
