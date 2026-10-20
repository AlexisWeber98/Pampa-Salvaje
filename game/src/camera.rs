//! Third-person orbit camera around the player.
//!
//! The mouse (or Q/E) swings the camera all the way around the rider, the
//! wheel zooms, and after a moment without camera input the camera eases
//! back behind a moving rider. Buildings never come between the camera and
//! the rider: it pulls in in front of them. The orbit math lives in
//! [`crate::orbit`].

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use pampa_core::{Aabb, Terrain, TownLayout, Travel, Vec2 as GroundVec2};
use pampa_render::{WorldTerrain, world_from_ground};

use crate::orbit::{Orbit, OrbitInput, OrbitParams};
use crate::player::{DrivePlayer, Player};

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
    /// Minimum clearance above the terrain under the camera.
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

/// Orbit tuning for the player's camera.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct CameraTuning(pub OrbitParams);

/// Wheel scroll reported in pixels (touchpads) per line of zoom.
const PIXELS_PER_LINE: f32 = 100.0;

/// A building the camera cannot see or pass through: its footprint as a
/// column from the ground up to `top` (world height, roof included).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Occluder {
    pub footprint: Aabb,
    pub top: f32,
}

/// Everything that blocks the camera's line of sight to the rider.
///
/// Buildings count as columns up to their roofs, so the camera may still
/// look over a low shed. Tree trunks are left out on purpose: a 0.7 m trunk
/// sweeping across the view would make the camera jump in and out.
#[derive(Resource, Debug, Clone, Default)]
pub struct CameraOccluders(pub Vec<Occluder>);

/// Extra height over a building's walls for roofs, parapets and towers.
const ROOF_ALLOWANCE: f32 = 3.0;
/// How far in front of a blocking wall the camera stops, in meters.
const WALL_CLEARANCE: f32 = 0.4;

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

    /// Every building of `town`, standing on `terrain`.
    pub fn from_town(town: &TownLayout, terrain: &Terrain) -> Self {
        Self(
            town.buildings
                .iter()
                .map(|b| {
                    let c = b.footprint.center();
                    Occluder {
                        footprint: b.footprint,
                        top: terrain.height(c.x, c.y) + b.height + ROOF_ALLOWANCE,
                    }
                })
                .collect(),
        )
    }
}

/// The camera position on the line from `focus` to `wanted` that keeps the
/// line of sight clear: `wanted` itself if nothing blocks it, otherwise a
/// point just in front of the first occluder.
pub fn pull_in(focus: Vec3, wanted: Vec3, occluders: &[Occluder]) -> Vec3 {
    let (from, to) = (
        GroundVec2::new(focus.x, -focus.z),
        GroundVec2::new(wanted.x, -wanted.z),
    );
    let height = |t: f32| focus.y + (wanted.y - focus.y) * t;
    let mut first = 1.0_f32;
    for o in occluders {
        let Some((enter, exit)) = o.footprint.segment_overlap(from, to) else {
            continue;
        };
        // Height is linear along the line, so it is lowest at an end of the
        // part inside the footprint; above the top there, it is above it all.
        let hit = if height(enter) < o.top {
            enter
        } else if height(exit) < o.top {
            // Descending into the column over its roof edge.
            enter + (exit - enter) * (height(enter) - o.top) / (height(enter) - height(exit))
        } else {
            continue;
        };
        first = first.min(hit);
    }
    if first >= 1.0 {
        return wanted;
    }
    let length = from.distance(to);
    let back_off = if length > 0.0 {
        WALL_CLEARANCE / length
    } else {
        0.0
    };
    focus.lerp(wanted, (first - back_off).max(0.0))
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

fn follow_player(
    time: Res<Time>,
    terrain: Res<WorldTerrain>,
    occluders: Res<CameraOccluders>,
    tuning: Res<CameraTuning>,
    input: Res<CameraInput>,
    player: Single<&Player>,
    camera: Single<(&mut Transform, &FollowCamera, &mut CameraRig)>,
) {
    let rider = &player.rider;
    let (mut transform, follow, mut rig) = camera.into_inner();
    let feet = world_from_ground(rider.position, rider.height);
    let look_height = match rider.travel() {
        Travel::OnFoot => follow.on_foot_look_height,
        Travel::Riding => follow.look_height,
    };
    let target = feet + Vec3::Y * look_height;

    let (orbit, focus) = match rig.0.as_mut() {
        Some((orbit, focus)) if focus.distance(target) <= follow.snap_distance => {
            let dt = time.delta_secs();
            orbit.update(input.0, rider.heading.radians(), rider.speed, &tuning.0, dt);
            focus.smooth_nudge(&target, follow.smoothing, dt);
            (*orbit, *focus)
        }
        // First frame or a respawn far away: jump right behind the rider.
        _ => *rig
            .0
            .insert((Orbit::behind(rider.heading.radians()), target)),
    };
    let desired = follow.desired(focus, &orbit, &terrain.0);
    // Keep the line of sight to the rider itself clear.
    transform.translation = pull_in(target, desired, &occluders.0);
    transform.look_at(focus, Vec3::Y);
}

impl FollowCamera {
    /// Unobstructed camera spot on `orbit` around the look-at point
    /// `focus`, kept above the terrain.
    pub fn desired(&self, focus: Vec3, orbit: &Orbit, terrain: &Terrain) -> Vec3 {
        let (ground, height) = orbit.offset();
        let mut desired = focus + world_from_ground(ground, height);
        // Keep the camera from sinking into a rise behind the rider.
        let floor = terrain.height(desired.x, -desired.z) + self.ground_clearance;
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
    use pampa_render::ViewDirection;
    use pampa_render::character::ground_yaw;
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
            .insert_resource(WorldTerrain(world.terrain.clone()))
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
        let terrain = app.world().resource::<WorldTerrain>().0.clone();
        let behind = Orbit::behind(r.heading.radians());
        let expected = follow.desired(focus, &behind, &terrain);
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
    fn billboards_face_correctly_at_any_orbit_angle() {
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
            let forward = camera(&mut app).forward();
            let camera_yaw = ground_yaw(forward.x, forward.z).unwrap();
            assert_eq!(ViewDirection::pick(heading, camera_yaw).index(), k);
        }
    }

    #[test]
    fn pulls_in_in_front_of_a_building_behind_the_rider() {
        // A tall wall 4 to 7 m south of a rider facing north.
        let wall = Occluder {
            footprint: Aabb::new(GroundVec2::new(-5.0, -7.0), GroundVec2::new(5.0, -4.0)),
            top: 50.0,
        };
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

    #[test]
    fn looks_over_a_low_building() {
        let shed = Occluder {
            footprint: Aabb::new(GroundVec2::new(-5.0, -7.0), GroundVec2::new(5.0, -4.0)),
            top: -10.0,
        };
        let focus = Vec3::new(0.0, 1.6, 0.0);
        let wanted = Vec3::new(0.0, 4.0, 9.0);
        assert_eq!(pull_in(focus, wanted, &[shed]), wanted);
    }

    fn assert_outside(occluders: &CameraOccluders, cam: Vec3, when: &str) {
        let at = ground(cam);
        for o in &occluders.0 {
            assert!(
                !(o.footprint.contains_point(at) && cam.y < o.top),
                "{when}: camera {cam} inside {o:?}"
            );
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
                &occluders,
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
            assert_outside(&occluders, cam, &format!("yaw {yaw}"));
            let (orbit, focus) = rig(&mut app);
            let terrain = app.world().resource::<WorldTerrain>().0.clone();
            let desired = FollowCamera::default().desired(focus, &orbit, &terrain);
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
}
