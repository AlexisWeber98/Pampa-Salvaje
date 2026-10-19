//! Third-person camera that trails behind and above the player, smoothly.

use bevy::prelude::*;
use pampa_core::{Aabb, Terrain, TownLayout, Vec2 as GroundVec2};
use pampa_render::{WorldTerrain, world_from_ground};

use crate::player::{DrivePlayer, Player};

/// Where the camera sits relative to the rider, and how quickly it catches up.
#[derive(Component, Debug, Clone, Copy)]
pub struct FollowCamera {
    pub distance: f32,
    pub height: f32,
    /// Point above the rider's feet the camera looks at.
    pub look_height: f32,
    /// Exponential catch-up rate (higher = snappier).
    pub smoothing: f32,
    /// Minimum clearance above the terrain under the camera.
    pub ground_clearance: f32,
    /// Farther than this from where it should be, the camera jumps instead
    /// of gliding (first frame, respawns).
    pub snap_distance: f32,
}

impl Default for FollowCamera {
    fn default() -> Self {
        Self {
            distance: 9.0,
            height: 4.0,
            look_height: 1.6,
            smoothing: 4.0,
            ground_clearance: 1.0,
            snap_distance: 50.0,
        }
    }
}

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
            .add_systems(Startup, spawn_camera)
            .add_systems(Update, follow_player.after(DrivePlayer));
    }
}

fn spawn_camera(mut commands: Commands) {
    let follow = FollowCamera::default();
    commands.spawn((
        Name::new("Follow camera"),
        Camera3d::default(),
        Transform::from_xyz(0.0, follow.height, follow.distance)
            .looking_at(Vec3::Y * follow.look_height, Vec3::Y),
        follow,
    ));
}

fn follow_player(
    time: Res<Time>,
    terrain: Res<WorldTerrain>,
    occluders: Res<CameraOccluders>,
    player: Single<&Player>,
    camera: Single<(&mut Transform, &FollowCamera)>,
) {
    let rider = &player.rider;
    let (mut transform, follow) = camera.into_inner();

    let feet = world_from_ground(rider.position, rider.height);
    let focus = feet + Vec3::Y * follow.look_height;
    let desired = follow.desired(feet, rider.heading.forward(), &terrain.0);
    let desired = pull_in(focus, desired, &occluders.0);

    if transform.translation.distance(desired) > follow.snap_distance {
        transform.translation = desired;
    } else {
        let dt = time.delta_secs();
        transform
            .translation
            .smooth_nudge(&desired, follow.smoothing, dt);
        // Gliding from the old spot can cut through a wall: keep the final
        // position on the clear side too.
        transform.translation = pull_in(focus, transform.translation, &occluders.0);
    }
    transform.look_at(focus, Vec3::Y);
}

impl FollowCamera {
    /// Unobstructed spot behind and above a rider standing at `feet` and
    /// facing ground direction `forward`, kept above the terrain.
    pub fn desired(&self, feet: Vec3, forward: GroundVec2, terrain: &Terrain) -> Vec3 {
        let back = world_from_ground(forward, 0.0) * -self.distance;
        let mut desired = feet + back + Vec3::Y * self.height;
        // Keep the camera from sinking into a rise behind the rider.
        let ground = terrain.height(desired.x, -desired.z) + self.ground_clearance;
        desired.y = desired.y.max(ground);
        desired
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::{GameWorld, PlayerPlugin, PlayerSpawn};
    use bevy::time::TimeUpdateStrategy;
    use pampa_core::{Heading, World};
    use std::time::Duration;

    fn ground(p: Vec3) -> GroundVec2 {
        GroundVec2::new(p.x, -p.z)
    }

    fn headless_app(world: World, spawn: PlayerSpawn, occluders: CameraOccluders) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                16,
            )))
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(WorldTerrain(world.terrain))
            .insert_resource(GameWorld(world))
            .insert_resource(spawn)
            .insert_resource(occluders)
            .add_plugins((PlayerPlugin, FollowCameraPlugin));
        app
    }

    fn camera(app: &mut App) -> Vec3 {
        let mut query = app
            .world_mut()
            .query_filtered::<&Transform, With<FollowCamera>>();
        query.single(app.world()).unwrap().translation
    }

    fn rider(app: &mut App) -> pampa_core::Rider {
        let mut query = app.world_mut().query::<&Player>();
        query.single(app.world()).unwrap().rider
    }

    fn north_at(x: f32, y: f32) -> PlayerSpawn {
        PlayerSpawn {
            position: GroundVec2::new(x, y),
            heading: Heading::from_radians(std::f32::consts::FRAC_PI_2),
        }
    }

    #[test]
    fn snaps_when_far_from_its_target() {
        // The camera starts near the origin; the rider is 160 m away.
        let mut app = headless_app(
            World::default(),
            north_at(26.0, -161.0),
            CameraOccluders::default(),
        );
        app.update();
        let r = rider(&mut app);
        let feet = world_from_ground(r.position, r.height);
        let follow = FollowCamera::default();
        let terrain = app.world().resource::<WorldTerrain>().0;
        let expected = follow.desired(feet, r.heading.forward(), &terrain);
        assert!(
            camera(&mut app).distance(expected) < 1e-4,
            "{} vs {expected}",
            camera(&mut app)
        );
    }

    #[test]
    fn glides_when_close_to_its_target() {
        let mut app = headless_app(
            World::default(),
            north_at(0.0, 0.0),
            CameraOccluders::default(),
        );
        app.update();
        let before = camera(&mut app);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyA);
        for _ in 0..10 {
            app.update();
        }
        let r = rider(&mut app);
        let feet = world_from_ground(r.position, r.height);
        let terrain = app.world().resource::<WorldTerrain>().0;
        let target = FollowCamera::default().desired(feet, r.heading.forward(), &terrain);
        let now = camera(&mut app);
        assert!(now.distance(before) > 0.01, "moves toward the new target");
        assert!(now.distance(target) > 0.01, "but does not jump onto it");
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
        let at = ground(camera(&mut app));
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

    #[test]
    fn never_ends_a_frame_inside_a_town_building() {
        let town = TownLayout::castex_inspired();
        let world = World::with_town(Terrain::default(), &town);
        let occluders = CameraOccluders::from_town(&town, &world.terrain);
        let town_terrain = world.terrain;
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
        };
        let mut app = headless_app(world, spawn, occluders.clone());
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::KeyW);
            keys.press(KeyCode::ShiftLeft);
        }
        let mut pulled = 0;
        for frame in 0..2400 {
            // Weave: turn left and right in long arcs through the streets.
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            let (on, off) = if (frame / 40) % 2 == 0 {
                (KeyCode::KeyA, KeyCode::KeyD)
            } else {
                (KeyCode::KeyD, KeyCode::KeyA)
            };
            keys.release(off);
            keys.press(on);
            app.update();
            let cam = camera(&mut app);
            let at = ground(cam);
            for o in &occluders.0 {
                assert!(
                    !(o.footprint.contains_point(at) && cam.y < o.top),
                    "frame {frame}: camera {cam} inside {o:?}"
                );
            }
            let r = rider(&mut app);
            let follow = FollowCamera::default();
            let feet = world_from_ground(r.position, r.height);
            let focus = feet + Vec3::Y * follow.look_height;
            let desired = follow.desired(feet, r.heading.forward(), &town_terrain);
            if pull_in(focus, desired, &occluders.0) != desired {
                pulled += 1;
            }
        }
        assert!(
            pulled > 0,
            "the weave must actually bring buildings behind the rider"
        );
    }
}
