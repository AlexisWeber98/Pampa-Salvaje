//! Third-person orbit camera around the player.
//!
//! The mouse (or Q/E) swings the camera all the way around the rider, the
//! wheel zooms, and after a moment without camera input the camera eases
//! back behind a moving rider. Buildings, the platform canopy and the line
//! train's cars never come between the camera and the rider: it pulls in in
//! front of them at once, and eases back out slowly once the view clears.
//! Thin things (trees, fences, signposts) do not move the camera: the line
//! of sight is handed to render each frame ([`SightLine`]), which fades
//! whichever of them stand in it.
//! The orbit math lives in [`crate::orbit`].

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use pampa_core::{
    Aabb, BuildingKind, Ground, LineTrain, PlacedCar, Terrain, TownLayout, Travel,
    Vec2 as GroundVec2,
};
use pampa_render::{SightLine, WorldTrain, world_from_ground};

use crate::orbit::{Orbit, OrbitInput, OrbitParams};
use crate::player::{DrivePlayer, GameWorld, Player};

/// How the camera follows the rider.
#[derive(Component, Debug, Clone, Copy)]
pub struct FollowCamera {
    /// Point above the rider's feet the camera looks at.
    pub look_height: f32,
    /// Point above the player's feet the camera looks at on foot: a
    /// person's chest is lower than a rider's.
    pub on_foot_look_height: f32,
    /// Exponential catch-up rate of the look-at point (higher = snappier).
    pub smoothing: f32,
    /// Minimum clearance above the ground (or platform) under the camera.
    pub ground_clearance: f32,
    /// Farther than this from the rider, the camera jumps behind it instead
    /// of gliding (first frame, respawns).
    pub snap_distance: f32,
}

impl Default for FollowCamera {
    fn default() -> Self {
        Self {
            look_height: 1.6,
            on_foot_look_height: 1.1,
            smoothing: 10.0,
            ground_clearance: 1.0,
            snap_distance: 50.0,
        }
    }
}

/// The camera's orbit and its smoothed look-at point; `None` until the
/// camera is first placed behind the rider.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CameraRig(pub Option<(Orbit, Vec3)>);

/// How far out along the arm from the rider to its orbit spot the camera
/// sits, as a fraction of the arm (1 = all the way out): the smoothed
/// pull-in, see [`ease_reach`].
#[derive(Component, Debug, Clone, Copy)]
pub struct CameraReach(pub f32);

impl Default for CameraReach {
    fn default() -> Self {
        Self(1.0)
    }
}

/// Orbit tuning for the player's camera.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct CameraTuning(pub OrbitParams);

/// Wheel scroll reported in pixels (touchpads) per line of zoom.
const PIXELS_PER_LINE: f32 = 100.0;

/// Something the camera cannot see or pass through: its footprint between
/// the world heights `bottom` and `top`. A building or a train car is a
/// column standing on the ground; the platform canopy is a slab overhead,
/// so the camera may still look under it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Occluder {
    pub footprint: Aabb,
    pub bottom: f32,
    pub top: f32,
}

impl Occluder {
    /// A column standing on the ground at height `ground`, up to `top`.
    pub fn column(footprint: Aabb, ground: f32, top: f32) -> Self {
        Self {
            footprint,
            bottom: ground,
            top,
        }
    }

    /// Whether the world point `p` is inside.
    #[cfg(test)]
    pub fn contains(&self, p: Vec3) -> bool {
        self.footprint.contains_point(GroundVec2::new(p.x, -p.z))
            && p.y > self.bottom
            && p.y < self.top
    }
}

/// Everything static that blocks the camera's line of sight to the rider
/// (the line train's cars are added each frame, see [`car_occluders`]).
///
/// Buildings count as columns up to their roofs, so the camera may still
/// look over a low shed, and the platform canopy as a roof slab. Tree trunks,
/// canopy posts and other thin things are left out on purpose: a 0.7 m trunk
/// sweeping across the view would make the camera jump in and out. Trees,
/// fences and signposts are faded instead (see [`SightLine`]).
#[derive(Resource, Debug, Clone, Default)]
pub struct CameraOccluders(pub Vec<Occluder>);

/// Extra height over a building's walls for roofs, parapets and towers.
const ROOF_ALLOWANCE: f32 = 3.0;
/// How far in front of a blocking wall the camera stops, in meters.
const WALL_CLEARANCE: f32 = 0.4;
/// Cars farther than this (plus half their length) from the rider cannot
/// come between it and the camera, whose arm is at most 25 m long.
const CAR_REACH: f32 = 30.0;
/// Time constant of the camera easing back out once the view clears, in
/// seconds. Pulling in has none: it is instant (see [`ease_reach`]).
pub const EASE_OUT: f32 = 0.35;

impl CameraOccluders {
    /// Every building of every town, standing on `terrain`.
    pub fn from_towns(towns: &[TownLayout], terrain: &Terrain) -> Self {
        Self(
            towns
                .iter()
                .flat_map(|town| Self::from_town(town, terrain).0)
                .collect(),
        )
    }

    /// Every building of `town`, standing on `terrain`, and the platform
    /// canopy's roof. The platform is left out: it is a floor the player
    /// stands on, and the camera's ground clearance keeps it above it.
    pub fn from_town(town: &TownLayout, terrain: &Terrain) -> Self {
        let ground = |p: GroundVec2| terrain.height(p.x, p.y);
        let buildings = town
            .buildings
            .iter()
            .filter(|b| b.kind != BuildingKind::Platform)
            .map(|b| {
                let g = ground(b.footprint.center());
                Occluder::column(b.footprint, g, g + b.height + ROOF_ALLOWANCE)
            });
        // The canopy hangs at its height over the leveled station ground.
        let canopies = town.canopies().into_iter().map(|c| {
            let g = ground(c.footprint.center());
            Occluder {
                footprint: c.footprint,
                bottom: g + c.bottom(),
                top: g + c.top(),
            }
        });
        Self(buildings.chain(canopies).collect())
    }
}

/// The line train's cars near `near` (the rider) as columns from the ground
/// to their roofs, a row of short boxes for a car on a slant.
pub fn car_occluders(cars: &[PlacedCar], terrain: &Terrain, near: GroundVec2) -> Vec<Occluder> {
    let ground = |p: GroundVec2| terrain.height(p.x, p.y);
    cars.iter()
        .filter(|car| car.center.distance(near) <= CAR_REACH + car.length / 2.0)
        .flat_map(|car| {
            // The roof over the higher of the car's two ends.
            let half = car.direction * (car.length / 2.0);
            let rails = ground(car.center - half).max(ground(car.center + half));
            let top = rails + LineTrain::RAIL_TOP + car.kind.height();
            car.boxes()
                .into_iter()
                .map(move |b| Occluder::column(b, ground(b.center()), top))
        })
        .collect()
}

/// The next frame's [`CameraReach`]: `allowed` (the clear part of the arm)
/// at once if the camera is out farther, so it is never inside an occluder,
/// even while easing; otherwise easing out toward it with the time constant
/// [`EASE_OUT`]. An exponential approach, so the same time passed in more,
/// shorter frames ends at the same spot.
///
/// There is no ease-in: any easing in would leave the camera behind the wall
/// it is pulling in front of for a few frames.
pub fn ease_reach(current: f32, allowed: f32, dt: f32) -> f32 {
    if allowed <= current {
        return allowed;
    }
    allowed - (allowed - current) * (-dt / EASE_OUT).exp()
}

/// The camera position on the line from `focus` to `wanted` that keeps the
/// line of sight clear: `wanted` itself if nothing blocks it, otherwise a
/// point just in front of the first occluder.
#[cfg(test)]
pub fn pull_in<'a>(
    focus: Vec3,
    wanted: Vec3,
    occluders: impl IntoIterator<Item = &'a Occluder>,
) -> Vec3 {
    let reach = clear_reach(focus, wanted, occluders);
    if reach >= 1.0 {
        wanted
    } else {
        focus.lerp(wanted, reach)
    }
}

/// How far along the line from `focus` to `wanted` the line of sight stays
/// clear, as a fraction of it: 1 if nothing blocks it, otherwise just in
/// front of the first occluder.
pub fn clear_reach<'a>(
    focus: Vec3,
    wanted: Vec3,
    occluders: impl IntoIterator<Item = &'a Occluder>,
) -> f32 {
    let (from, to) = (
        GroundVec2::new(focus.x, -focus.z),
        GroundVec2::new(wanted.x, -wanted.z),
    );
    let mut first = 1.0_f32;
    for o in occluders {
        let Some((enter, exit)) = o.footprint.segment_overlap(from, to) else {
            continue;
        };
        // The line is inside the occluder where it is over its footprint
        // and between its bottom and top heights.
        let Some((low, high)) = height_band(focus.y, wanted.y, o.bottom, o.top) else {
            continue;
        };
        let (start, end) = (enter.max(low), exit.min(high));
        if start <= end {
            first = first.min(start);
        }
    }
    if first >= 1.0 {
        return 1.0;
    }
    let length = from.distance(to);
    let back_off = if length > 0.0 {
        WALL_CLEARANCE / length
    } else {
        0.0
    };
    (first - back_off).max(0.0)
}

/// Where a line whose height goes linearly from `from` (at 0) to `to` (at 1)
/// is strictly between `bottom` and `top`, as a parameter range (unbounded
/// for a level line); `None` if it never is.
fn height_band(from: f32, to: f32, bottom: f32, top: f32) -> Option<(f32, f32)> {
    let rise = to - from;
    if rise == 0.0 {
        return (from > bottom && from < top).then_some((f32::NEG_INFINITY, f32::INFINITY));
    }
    let (a, b) = ((bottom - from) / rise, (top - from) / rise);
    Some((a.min(b), a.max(b)))
}

pub struct FollowCameraPlugin;

impl Plugin for FollowCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraOccluders>()
            .init_resource::<CameraTuning>()
            .init_resource::<CameraInput>()
            .add_systems(Startup, spawn_camera)
            .add_systems(Update, grab_cursor)
            .add_systems(
                Update,
                (gather_camera_input, follow_player)
                    .chain()
                    .after(DrivePlayer),
            );
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Name::new("Follow camera"),
        Camera3d::default(),
        Transform::from_xyz(0.0, 4.0, 9.0).looking_at(Vec3::Y, Vec3::Y),
        FollowCamera::default(),
        CameraRig::default(),
        CameraReach::default(),
    ));
}

/// Locks and hides the cursor while the window has focus, so the mouse
/// orbits the camera; frees it when the player switches away.
fn grab_cursor(mut windows: Query<(&Window, &mut CursorOptions), With<PrimaryWindow>>) {
    for (window, mut cursor) in &mut windows {
        let (grab_mode, visible) = if window.focused {
            (CursorGrabMode::Locked, false)
        } else {
            (CursorGrabMode::None, true)
        };
        if cursor.grab_mode != grab_mode || cursor.visible != visible {
            cursor.grab_mode = grab_mode;
            cursor.visible = visible;
        }
    }
}

/// Maps this frame's devices to camera input: mouse motion only while the
/// cursor is grabbed, Q/E to orbit, the wheel to zoom.
pub fn read_camera_input(
    keys: &ButtonInput<KeyCode>,
    mouse_motion: Vec2,
    scroll: Option<&AccumulatedMouseScroll>,
    grabbed: bool,
) -> OrbitInput {
    let look = if grabbed {
        (mouse_motion.x, mouse_motion.y)
    } else {
        (0.0, 0.0)
    };
    let zoom = scroll.map_or(0.0, |s| match s.unit {
        MouseScrollUnit::Line => s.delta.y,
        MouseScrollUnit::Pixel => s.delta.y / PIXELS_PER_LINE,
    });
    OrbitInput {
        look,
        keys: f32::from(u8::from(keys.pressed(KeyCode::KeyE)))
            - f32::from(u8::from(keys.pressed(KeyCode::KeyQ))),
        zoom,
    }
}

/// This frame's camera input, gathered from the devices.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct CameraInput(pub OrbitInput);

fn gather_camera_input(
    keys: Res<ButtonInput<KeyCode>>,
    motion: Option<Res<AccumulatedMouseMotion>>,
    scroll: Option<Res<AccumulatedMouseScroll>>,
    cursors: Query<&CursorOptions, With<PrimaryWindow>>,
    mut input: ResMut<CameraInput>,
) {
    let grabbed = cursors.iter().any(|c| c.grab_mode != CursorGrabMode::None);
    input.0 = read_camera_input(
        &keys,
        motion.map_or(Vec2::ZERO, |m| m.delta),
        scroll.as_deref(),
        grabbed,
    );
}

#[allow(clippy::too_many_arguments)]
fn follow_player(
    time: Res<Time>,
    world: Res<GameWorld>,
    occluders: Res<CameraOccluders>,
    train: Option<Res<WorldTrain>>,
    tuning: Res<CameraTuning>,
    input: Res<CameraInput>,
    sight: Option<ResMut<SightLine>>,
    player: Single<&Player>,
    camera: Single<(
        &mut Transform,
        &FollowCamera,
        &mut CameraRig,
        &mut CameraReach,
    )>,
) {
    let rider = &player.rider;
    let (mut transform, follow, mut rig, mut reach) = camera.into_inner();
    let feet = world_from_ground(rider.position, rider.height);
    let look_height = match rider.travel() {
        Travel::OnFoot => follow.on_foot_look_height,
        Travel::Riding => follow.look_height,
    };
    let target = feet + Vec3::Y * look_height;

    let dt = time.delta_secs();
    let (orbit, focus, snapped) = match rig.0.as_mut() {
        Some((orbit, focus)) if focus.distance(target) <= follow.snap_distance => {
            orbit.update(input.0, rider.heading.radians(), rider.speed, &tuning.0, dt);
            focus.smooth_nudge(&target, follow.smoothing, dt);
            (*orbit, *focus, false)
        }
        // First frame or a respawn far away: jump right behind the rider.
        _ => {
            let (orbit, focus) = *rig
                .0
                .insert((Orbit::behind(rider.heading.radians()), target));
            (orbit, focus, true)
        }
    };
    let desired = follow.desired(focus, &orbit, &world.0);
    // Keep the line of sight to the rider itself clear, of buildings and of
    // the train's cars near it.
    let cars = train
        .as_deref()
        .and_then(|t| t.0.as_ref())
        .map(|t| car_occluders(&t.placed_cars(), &world.0.terrain, rider.position))
        .unwrap_or_default();
    let allowed = clear_reach(target, desired, occluders.0.iter().chain(&cars));
    reach.0 = if snapped {
        allowed
    } else {
        ease_reach(reach.0, allowed, dt)
    };
    transform.translation = if reach.0 >= 1.0 {
        desired
    } else {
        target.lerp(desired, reach.0)
    };
    transform.look_at(focus, Vec3::Y);
    // Trees, fences and signs in the way fade instead (render).
    if let Some(mut sight) = sight {
        sight.0 = Some((target, transform.translation));
    }
}

impl FollowCamera {
    /// Unobstructed camera spot on `orbit` around the look-at point
    /// `focus`, kept above the ground (pass the world, so platforms count).
    pub fn desired(&self, focus: Vec3, orbit: &Orbit, ground: &(impl Ground + ?Sized)) -> Vec3 {
        let (offset, height) = orbit.offset();
        let mut desired = focus + world_from_ground(offset, height);
        // Keep the camera from sinking into a rise or a platform behind
        // the rider.
        let below = GroundVec2::new(desired.x, -desired.z);
        let floor = ground.ground_height(below) + self.ground_clearance;
        desired.y = desired.y.max(floor);
        desired
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orbit::angle_between;
    use crate::player::{GameWorld, PlayerPlugin, PlayerSpawn};
    use bevy::time::TimeUpdateStrategy;
    use pampa_core::{Heading, World};
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};
    use std::time::Duration;

    const FRAME: f32 = 0.016;

    fn ground(p: Vec3) -> GroundVec2 {
        GroundVec2::new(p.x, -p.z)
    }

    fn headless_app(world: World, spawn: PlayerSpawn, occluders: CameraOccluders) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
                FRAME,
            )))
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(GameWorld(world))
            .insert_resource(spawn)
            .insert_resource(occluders)
            .add_plugins((PlayerPlugin, FollowCameraPlugin));
        app
    }

    fn camera(app: &mut App) -> Transform {
        let mut query = app
            .world_mut()
            .query_filtered::<&Transform, With<FollowCamera>>();
        *query.single(app.world()).unwrap()
    }

    fn rig(app: &mut App) -> (Orbit, Vec3) {
        let mut query = app.world_mut().query::<&CameraRig>();
        query.single(app.world()).unwrap().0.expect("placed")
    }

    fn set_yaw(app: &mut App, yaw: f32) {
        let mut query = app.world_mut().query::<&mut CameraRig>();
        let mut rig = query.single_mut(app.world_mut()).unwrap();
        if let Some((orbit, _)) = rig.0.as_mut() {
            orbit.yaw = yaw;
        }
    }

    fn rider(app: &mut App) -> pampa_core::Rider {
        let mut query = app.world_mut().query::<&Player>();
        query.single(app.world()).unwrap().rider
    }

    fn keys(app: &mut App) -> Mut<'_, ButtonInput<KeyCode>> {
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>()
    }

    fn north_at(x: f32, y: f32) -> PlayerSpawn {
        PlayerSpawn {
            position: GroundVec2::new(x, y),
            heading: Heading::from_radians(FRAC_PI_2),
            ..PlayerSpawn::default()
        }
    }

    /// Ground angle from the rider to the camera.
    fn camera_bearing(app: &mut App) -> f32 {
        let r = rider(app);
        let d = ground(camera(app).translation) - r.position;
        d.y.atan2(d.x)
    }

    #[test]
    fn snaps_behind_the_rider_when_far_away() {
        // The camera starts near the origin; the rider is 160 m away.
        let mut app = headless_app(
            World::default(),
            north_at(26.0, -161.0),
            CameraOccluders::default(),
        );
        app.update();
        let r = rider(&mut app);
        let follow = FollowCamera::default();
        let focus = world_from_ground(r.position, r.height) + Vec3::Y * follow.look_height;
        let world = app.world().resource::<GameWorld>().0.clone();
        let behind = Orbit::behind(r.heading.radians());
        let expected = follow.desired(focus, &behind, &world);
        let at = camera(&mut app).translation;
        assert!(at.distance(expected) < 1e-4, "{at} vs {expected}");
        assert!(at.z > focus.z, "south of a rider facing north");
    }

    #[test]
    fn turning_in_place_leaves_the_orbit_alone() {
        let mut app = headless_app(
            World::default(),
            north_at(0.0, 0.0),
            CameraOccluders::default(),
        );
        app.update();
        let before = camera(&mut app).translation;
        keys(&mut app).press(KeyCode::KeyA);
        for _ in 0..100 {
            app.update();
        }
        assert!(rider(&mut app).heading.radians() > FRAC_PI_2 + 1.0);
        let after = camera(&mut app).translation;
        assert!(after.distance(before) < 1e-3, "{before} -> {after}");
    }

    #[test]
    fn q_and_e_orbit_around_the_rider() {
        let mut app = headless_app(
            World::default(),
            north_at(0.0, 0.0),
            CameraOccluders::default(),
        );
        app.update();
        let start = camera_bearing(&mut app);
        keys(&mut app).press(KeyCode::KeyQ);
        for _ in 0..30 {
            app.update();
        }
        let q = angle_between(start, camera_bearing(&mut app));
        let rate = OrbitParams::default().key_rate;
        assert!((q - rate * 30.0 * FRAME).abs() < 0.05, "Q swung {q}");
        keys(&mut app).release(KeyCode::KeyQ);
        keys(&mut app).press(KeyCode::KeyE);
        for _ in 0..30 {
            app.update();
        }
        let e = angle_between(start, camera_bearing(&mut app));
        assert!(e.abs() < 0.05, "E swings back: {e}");
    }

    #[test]
    fn eases_back_behind_a_moving_rider_after_idling() {
        let mut app = headless_app(
            World::default(),
            north_at(0.0, 0.0),
            CameraOccluders::default(),
        );
        app.update();
        set_yaw(&mut app, 0.0); // East of a rider facing north.
        keys(&mut app).press(KeyCode::KeyW);
        for _ in 0..60 {
            app.update();
        }
        let early = camera_bearing(&mut app);
        assert!(early.abs() < 0.05, "still to the side after 1 s: {early}");
        for _ in 0..300 {
            app.update();
        }
        let r = rider(&mut app);
        let behind = r.heading.radians() + PI;
        let late = angle_between(behind, camera_bearing(&mut app));
        assert!(late.abs() < 0.1, "behind the rider after 6 s: {late}");
    }

    #[test]
    fn mouse_moves_only_a_grabbed_cursor_and_keys_and_wheel_always_work() {
        let mut keys = ButtonInput::<KeyCode>::default();
        let motion = Vec2::new(12.0, -3.0);
        assert_eq!(
            read_camera_input(&keys, motion, None, false),
            OrbitInput::default()
        );
        let input = read_camera_input(&keys, motion, None, true);
        assert_eq!(input.look, (12.0, -3.0));
        keys.press(KeyCode::KeyE);
        assert_eq!(read_camera_input(&keys, Vec2::ZERO, None, false).keys, 1.0);
        keys.press(KeyCode::KeyQ);
        assert_eq!(read_camera_input(&keys, Vec2::ZERO, None, false).keys, 0.0);
        let lines = AccumulatedMouseScroll {
            unit: MouseScrollUnit::Line,
            delta: Vec2::new(0.0, 2.0),
        };
        assert_eq!(
            read_camera_input(&keys, Vec2::ZERO, Some(&lines), false).zoom,
            2.0
        );
        let pixels = AccumulatedMouseScroll {
            unit: MouseScrollUnit::Pixel,
            delta: Vec2::new(0.0, -50.0),
        };
        let zoom = read_camera_input(&keys, Vec2::ZERO, Some(&pixels), false).zoom;
        assert!((zoom + 50.0 / PIXELS_PER_LINE).abs() < 1e-6);
    }

    #[test]
    fn the_orbit_yaw_turns_the_camera_round_the_rider() {
        let mut app = headless_app(
            World::default(),
            north_at(0.0, 0.0),
            CameraOccluders::default(),
        );
        app.update();
        let heading = rider(&mut app).heading;
        for k in 0..8 {
            // Camera bearing such that the rider faces k * 45 degrees
            // counter-clockwise from the camera's view direction.
            let yaw = heading.radians() + PI - k as f32 * FRAC_PI_4;
            set_yaw(&mut app, yaw);
            app.update();
            // The camera looks along the ground bearing `yaw + PI`: the
            // rider's heading turned by `k` eighths of a turn.
            let forward = camera(&mut app).forward();
            let camera_yaw = (-forward.z).atan2(forward.x);
            let relative = (heading.radians() - camera_yaw).rem_euclid(std::f32::consts::TAU);
            let eighths = (relative / FRAC_PI_4).round() as usize % 8;
            assert_eq!(eighths, k, "{relative}");
        }
    }

    #[test]
    fn pulls_in_in_front_of_a_building_behind_the_rider() {
        // A tall wall 4 to 7 m south of a rider facing north.
        let wall = Occluder::column(
            Aabb::new(GroundVec2::new(-5.0, -7.0), GroundVec2::new(5.0, -4.0)),
            -10.0,
            50.0,
        );
        let mut app = headless_app(
            World::default(),
            north_at(0.0, 0.0),
            CameraOccluders(vec![wall]),
        );
        app.update();
        let at = ground(camera(&mut app).translation);
        assert!(
            !wall.footprint.contains_point(at),
            "inside the wall at {at:?}"
        );
        assert!(at.y > -4.0 && at.y < 0.0, "between wall and rider: {at:?}");
    }

    /// A caldén 6 m south of a rider at the origin facing north, planted,
    /// and drawn as one fading part.
    fn tree_app(walls: CameraOccluders) -> (App, Entity) {
        let tree = pampa_core::Tree {
            at: GroundVec2::new(0.0, -6.0),
            species: pampa_core::Species::Calden,
            height: 7.0,
            yaw: 0.0,
            variant: 0,
        };
        let mut world = World::default();
        world.obstacles.push(tree.obstacle());
        let mut app = headless_app(world.clone(), north_at(0.0, 0.0), walls);
        app.init_resource::<Assets<StandardMaterial>>()
            .add_plugins(pampa_render::ThinFadePlugin);
        let material = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let ground = world.terrain.height(0.0, -6.0);
        let part = app
            .world_mut()
            .spawn((
                pampa_render::ThinOccluder {
                    a: tree.at,
                    b: tree.at,
                    radius: 0.8 * tree.crown_radius(),
                    bottom: ground - 0.5,
                    top: ground + tree.height,
                },
                MeshMaterial3d(material),
            ))
            .id();
        (app, part)
    }

    #[test]
    fn a_tree_behind_the_rider_fades_instead_of_pulling_the_camera_in() {
        let (mut app, tree) = tree_app(CameraOccluders::default());
        for _ in 0..3 {
            app.update();
        }
        let r = rider(&mut app);
        let follow = FollowCamera::default();
        let (orbit, focus) = rig(&mut app);
        let world = app.world().resource::<GameWorld>().0.clone();
        let full = follow.desired(focus, &orbit, &world);
        let at = camera(&mut app).translation;
        assert!(at.distance(full) < 1e-3, "pulled in: {at} vs {full}");
        // The camera is south of the tree, which stands in the view.
        assert!(ground(at).y < -6.0, "{at}");
        assert!(r.position.y > -6.0);
        assert!(app.world().get::<pampa_render::Faded>(tree).is_some());
    }

    #[test]
    fn a_building_still_pulls_the_camera_in_past_a_faded_tree() {
        let wall = Occluder::column(
            Aabb::new(GroundVec2::new(-5.0, -11.0), GroundVec2::new(5.0, -8.0)),
            -10.0,
            50.0,
        );
        let (mut app, tree) = tree_app(CameraOccluders(vec![wall]));
        for _ in 0..3 {
            app.update();
        }
        let at = ground(camera(&mut app).translation);
        assert!(at.y > -8.0 && at.y < 0.0, "in front of the wall: {at:?}");
        assert!(!wall.footprint.contains_point(at));
        // Between the wall and the rider, the tree is still in the way.
        assert!(at.y < -6.0, "{at:?}");
        assert!(app.world().get::<pampa_render::Faded>(tree).is_some());
    }

    #[test]
    fn looks_over_a_low_building() {
        let shed = Occluder::column(
            Aabb::new(GroundVec2::new(-5.0, -7.0), GroundVec2::new(5.0, -4.0)),
            -20.0,
            -10.0,
        );
        let focus = Vec3::new(0.0, 1.6, 0.0);
        let wanted = Vec3::new(0.0, 4.0, 9.0);
        assert_eq!(pull_in(focus, wanted, &[shed]), wanted);
    }

    fn assert_outside<'a>(
        occluders: impl IntoIterator<Item = &'a Occluder>,
        cam: Vec3,
        when: &str,
    ) {
        for o in occluders {
            assert!(!o.contains(cam), "{when}: camera {cam} inside {o:?}");
        }
    }

    fn town_app() -> (App, CameraOccluders, Aabb) {
        let town = TownLayout::castex_inspired();
        let world = World::with_town(Terrain::default(), &town);
        let occluders = CameraOccluders::from_town(&town, &world.terrain);
        // In the street, back to the general store's front, facing the plaza.
        let store = town
            .buildings
            .iter()
            .find(|b| b.kind == pampa_core::BuildingKind::GeneralStore)
            .unwrap()
            .footprint;
        let spawn = PlayerSpawn {
            position: GroundVec2::new(store.max.x + 2.5, store.center().y),
            heading: Heading::from_radians(0.0),
            ..PlayerSpawn::default()
        };
        (
            headless_app(world, spawn, occluders.clone()),
            occluders,
            store,
        )
    }

    #[test]
    fn never_ends_a_frame_inside_a_town_building() {
        let (mut app, occluders, _) = town_app();
        keys(&mut app).press(KeyCode::KeyW);
        keys(&mut app).press(KeyCode::ShiftLeft);
        for frame in 0..2400 {
            // Weave: turn left and right in long arcs through the streets,
            // and orbit the camera now and then.
            let mut k = keys(&mut app);
            let (on, off) = if (frame / 40) % 2 == 0 {
                (KeyCode::KeyA, KeyCode::KeyD)
            } else {
                (KeyCode::KeyD, KeyCode::KeyA)
            };
            k.release(off);
            k.press(on);
            if (frame / 300) % 3 == 1 {
                k.press(KeyCode::KeyE);
            } else {
                k.release(KeyCode::KeyE);
            }
            app.update();
            assert_outside(
                &occluders.0,
                camera(&mut app).translation,
                &format!("frame {frame}"),
            );
        }
    }

    #[test]
    fn never_ends_inside_a_building_at_any_orbit_angle() {
        let (mut app, occluders, _) = town_app();
        app.update();
        let mut pulled = 0;
        for step in 0..72 {
            let yaw = step as f32 * 5f32.to_radians();
            set_yaw(&mut app, yaw);
            app.update();
            let cam = camera(&mut app).translation;
            assert_outside(&occluders.0, cam, &format!("yaw {yaw}"));
            let (orbit, focus) = rig(&mut app);
            let world = app.world().resource::<GameWorld>().0.clone();
            let desired = FollowCamera::default().desired(focus, &orbit, &world);
            if pull_in(focus, desired, &occluders.0) != desired {
                pulled += 1;
            }
        }
        assert!(pulled > 0, "some angles look through the store");
    }

    #[test]
    fn on_foot_the_camera_looks_at_a_lower_point() {
        let mut heights = Vec::new();
        for travel in [pampa_core::Travel::Riding, pampa_core::Travel::OnFoot] {
            let spawn = PlayerSpawn {
                travel,
                ..north_at(0.0, 0.0)
            };
            let mut app = headless_app(World::default(), spawn, CameraOccluders::default());
            app.update();
            let r = rider(&mut app);
            let feet = world_from_ground(r.position, r.height);
            heights.push(rig(&mut app).1.y - feet.y);
        }
        let follow = FollowCamera::default();
        assert!(
            (heights[0] - follow.look_height).abs() < 1e-4,
            "{heights:?}"
        );
        assert!(
            (heights[1] - follow.on_foot_look_height).abs() < 1e-4,
            "{heights:?}"
        );
        assert!(follow.on_foot_look_height < follow.look_height);
    }

    #[test]
    fn on_the_platform_the_camera_stays_above_it_and_is_not_blocked_by_it() {
        let town = TownLayout::castex_inspired();
        let world = World::with_town(Terrain::default(), &town);
        let occluders = CameraOccluders::from_town(&town, &world.terrain);
        let platform = town
            .buildings
            .iter()
            .find(|b| b.kind == BuildingKind::Platform)
            .unwrap();
        // A floor, not a wall: the camera may look across it.
        assert!(
            occluders
                .0
                .iter()
                .all(|o| o.footprint != platform.footprint)
        );
        assert_eq!(
            occluders.0.len(),
            town.buildings.len() - 1 + town.canopies().len()
        );
        let (position, heading) = town.platform_spot().unwrap();
        let top = world.ground_height(position);
        let spawn = PlayerSpawn {
            position,
            heading,
            travel: pampa_core::Travel::OnFoot,
        };
        let mut app = headless_app(world.clone(), spawn, occluders.clone());
        app.update();
        // The player stands on the platform's top.
        assert_eq!(rider(&mut app).height, top);
        let follow = FollowCamera::default();
        for step in 0..72 {
            let yaw = step as f32 * 5f32.to_radians();
            set_yaw(&mut app, yaw);
            app.update();
            let cam = camera(&mut app).translation;
            assert_outside(&occluders.0, cam, &format!("yaw {yaw}"));
            // Never down in the platform or the ground beside it.
            let floor = world.ground_height(ground(cam));
            assert!(cam.y > floor + 0.5, "yaw {yaw}: {cam} over {floor}");
            let (orbit, focus) = rig(&mut app);
            let desired = follow.desired(focus, &orbit, &world);
            let under = world.ground_height(ground(desired));
            assert!(
                desired.y >= under + follow.ground_clearance - 1e-4,
                "{desired}"
            );
        }
    }

    fn flat() -> Terrain {
        Terrain {
            amplitude: 0.0,
            ..Terrain::default()
        }
    }

    /// A coach standing on flat ground at `center`, along `direction`.
    fn coach(center: GroundVec2, direction: GroundVec2) -> PlacedCar {
        PlacedCar {
            kind: pampa_core::CarKind::Coach,
            center,
            direction: direction * (1.0 / direction.length()),
            length: 15.0,
            on_rail: true,
        }
    }

    /// Where `p` is relative to `car`: along its length and across it.
    fn car_frame(car: &PlacedCar, p: GroundVec2) -> (f32, f32) {
        let d = p - car.center;
        let across = GroundVec2::new(-car.direction.y, car.direction.x);
        (d.dot(car.direction), d.dot(across))
    }

    #[test]
    fn pulls_in_in_front_of_a_train_car_beside_the_player() {
        let focus = Vec3::new(0.0, 1.6, 0.0);
        // A car running east 5 m north of the player, and one on a slant.
        for direction in [GroundVec2::new(1.0, 0.0), GroundVec2::new(1.0, 0.6)] {
            let car = coach(GroundVec2::new(0.0, 5.0), direction);
            let occluders = car_occluders(&[car], &flat(), GroundVec2::ZERO);
            assert!(!occluders.is_empty(), "{direction:?}");
            let top = LineTrain::RAIL_TOP + car.kind.height();
            assert!(occluders.iter().all(|o| (o.top - top).abs() < 1e-4));
            // The camera wants to be 12 m north, behind the car, at 3 m.
            let wanted = Vec3::new(0.0, 3.0, -12.0);
            let at = pull_in(focus, wanted, &occluders);
            assert_ne!(at, wanted, "{direction:?}: blocked by the car");
            assert_outside(&occluders, at, &format!("{direction:?}"));
            // In front of the car, on the player's side, clear of its body.
            let (along, across) = car_frame(&car, ground(at));
            assert!(along.abs() < car.length / 2.0, "{direction:?}");
            assert!(
                across < -LineTrain::WIDTH / 2.0,
                "{direction:?}: {across} across the car"
            );
        }
    }

    #[test]
    fn far_cars_are_left_out() {
        let car = coach(GroundVec2::new(0.0, 200.0), GroundVec2::new(1.0, 0.0));
        assert!(car_occluders(&[car], &flat(), GroundVec2::ZERO).is_empty());
    }

    /// A roof slab from 3.5 to 3.7 m over the 6 m north of the focus.
    fn roof() -> Occluder {
        Occluder {
            footprint: Aabb::new(GroundVec2::new(-10.0, 0.0), GroundVec2::new(10.0, 6.0)),
            bottom: 3.5,
            top: 3.7,
        }
    }

    #[test]
    fn looks_under_a_canopy_but_not_through_its_roof() {
        let focus = Vec3::new(0.0, 1.6, 0.0);
        // Under the roof and out past it, low: clear.
        let under = Vec3::new(0.0, 3.0, -9.0);
        assert_eq!(pull_in(focus, under, &[roof()]), under);
        // Up through the roof: stops under it.
        let over = Vec3::new(0.0, 7.0, -9.0);
        let at = pull_in(focus, over, &[roof()]);
        assert_ne!(at, over);
        assert!(at.y < roof().bottom, "{at}");
        assert!(!roof().contains(at));
        // From above the roof down to a camera under it, too.
        let high = Vec3::new(0.0, 6.0, 0.0);
        let low = Vec3::new(0.0, 1.0, -4.0);
        let at = pull_in(high, low, &[roof()]);
        assert!(at.y > roof().top, "{at}");
    }

    #[test]
    fn the_platform_canopy_is_a_roof_over_the_platform() {
        let town = TownLayout::castex_inspired();
        let terrain = Terrain::default();
        let occluders = CameraOccluders::from_town(&town, &terrain);
        let canopy = town.canopies()[0];
        let c = canopy.footprint.center();
        let roof = occluders
            .0
            .iter()
            .find(|o| o.footprint == canopy.footprint)
            .expect("the canopy's roof");
        let ground = terrain.height(c.x, c.y);
        assert!((roof.bottom - ground - canopy.bottom()).abs() < 1e-4);
        assert!((roof.top - ground - canopy.top()).abs() < 1e-4);
    }

    #[test]
    fn pulls_in_at_once_and_eases_out_slowly() {
        // Blocked: straight to the allowed reach, never beyond it.
        assert_eq!(ease_reach(1.0, 0.3, 0.001), 0.3);
        assert_eq!(ease_reach(0.5, 0.3, 0.1), 0.3);
        // Cleared: on the way out, still short of it after one frame.
        let one = ease_reach(0.3, 1.0, FRAME);
        assert!(one > 0.3 && one < 0.35, "{one}");
        let mut r = 0.3;
        let mut t = 0.0;
        while r < 0.95 {
            r = ease_reach(r, 1.0, FRAME);
            t += FRAME;
        }
        assert!(t > 0.5 && t < 1.5, "out in {t} s");
        // Frame-rate independent.
        let coarse = ease_reach(0.3, 1.0, 0.1);
        let fine = (0..10).fold(0.3, |r, _| ease_reach(r, 1.0, 0.01));
        assert!((coarse - fine).abs() < 1e-5, "{coarse} vs {fine}");
        // Never past the allowed reach while easing.
        assert!((0..100).fold(0.3, |r, _| ease_reach(r, 0.8, 0.05)) <= 0.8);
    }

    #[test]
    fn eases_back_out_when_the_view_clears() {
        // A wall behind the rider pulls the camera in; once it is gone, the
        // camera glides back out over a few frames instead of jumping.
        let wall = Occluder::column(
            Aabb::new(GroundVec2::new(-5.0, -7.0), GroundVec2::new(5.0, -4.0)),
            -10.0,
            50.0,
        );
        let mut app = headless_app(
            World::default(),
            north_at(0.0, 0.0),
            CameraOccluders(vec![wall]),
        );
        app.update();
        app.update();
        let pulled = ground(camera(&mut app).translation).distance(GroundVec2::ZERO);
        app.insert_resource(CameraOccluders::default());
        app.update();
        let next = ground(camera(&mut app).translation).distance(GroundVec2::ZERO);
        for _ in 0..200 {
            app.update();
        }
        let out = ground(camera(&mut app).translation).distance(GroundVec2::ZERO);
        assert!(pulled < 4.0, "{pulled}");
        assert!(
            next > pulled && next < out - 1.0,
            "{pulled} -> {next} -> {out}"
        );
    }

    #[test]
    fn on_the_platform_the_camera_stays_in_front_of_the_dwelling_train() {
        let loaded =
            crate::towns::load_world(&pampa_content::asset_path(pampa_content::TOWNS_FILE));
        let (_, train) = crate::life::start_of_day(&loaded.line, &loaded.towns);
        let train = train.expect("a line train");
        let world = World::with_towns(loaded.world, Terrain::default(), &loaded.towns);
        let occluders = CameraOccluders::from_towns(&loaded.towns, &world.terrain);
        let (position, heading) = loaded.towns[0].platform_spot().unwrap();
        let cars = train.placed_cars();
        // The car beside the player, and the bearing across the track to it.
        let beside = *cars
            .iter()
            .min_by(|a, b| {
                let d = |c: &PlacedCar| c.center.distance(position);
                d(a).total_cmp(&d(b))
            })
            .unwrap();
        let (_, across) = car_frame(&beside, position);
        let normal = GroundVec2::new(-beside.direction.y, beside.direction.x);
        let to_track = normal * -across.signum();
        let spawn = PlayerSpawn {
            position,
            heading,
            travel: pampa_core::Travel::OnFoot,
        };
        let mut app = headless_app(world.clone(), spawn, occluders.clone());
        app.insert_resource(WorldTrain(Some(train)));
        app.update();
        let yaw = to_track.y.atan2(to_track.x);
        let mut pulled = 0;
        for k in -3..=3 {
            set_yaw(&mut app, yaw + k as f32 * 0.15);
            app.update();
            let cam = camera(&mut app).translation;
            let cars = car_occluders(&cars, &world.terrain, position);
            assert_outside(&cars, cam, &format!("yaw step {k}"));
            // On the player's side of the train, with a clear view.
            let (along, side) = car_frame(&beside, ground(cam));
            assert!(
                side * across > 0.0 && side.abs() > LineTrain::WIDTH / 2.0,
                "step {k}: {along}, {side} vs the player's {across}"
            );
            let r = rider(&mut app);
            let follow = FollowCamera::default();
            let target =
                world_from_ground(r.position, r.height) + Vec3::Y * follow.on_foot_look_height;
            assert_eq!(pull_in(target, cam, &cars), cam, "step {k}");
            let (orbit, focus) = rig(&mut app);
            if cam.distance(follow.desired(focus, &orbit, &world)) > 1.0 {
                pulled += 1;
            }
        }
        assert!(pulled >= 5, "the cars pull the camera in: {pulled}");
    }
}
