//! Trees collide only where the rider is: the trunks of the trees of the
//! chunks round the player are obstacles, streamed in as the player nears a
//! chunk and out as they leave it. Planting the whole 24 km world at startup
//! (hundreds of thousands of trunks) took seconds, for trees nobody can
//! reach. Shrubs and grass are never obstacles.
//!
//! Each chunk's trees come from [`FloraSite`], deterministic per chunk, so
//! a chunk that leaves and returns has exactly the same trees. This ring is
//! separate from the render crate's terrain streaming, so headless games
//! collide the same way. What moves far from the player (cattle in their
//! fenced fields) does not see trees there; trains never need them, the
//! flora keeps off the whole track.

use bevy::prelude::*;
use pampa_core::{ChunkCoord, FloraSite, Vec2, World};
use pampa_render::WorldFlora;

use crate::player::{DrivePlayer, GameWorld, Player};

/// Chunks (Chebyshev distance from the player's chunk) whose trees are
/// loaded. A chunk is 200 m: with one chunk loaded all round, every tree
/// within 200 m of the player collides, far past the rider's reach in a
/// frame (about 12 m/s at a gallop), the nearest cattle and townsfolk, and
/// the follow camera's few meters.
pub const LOAD_RADIUS: i32 = 1;
/// Chunks past this distance are dropped. One more than [`LOAD_RADIUS`], so
/// riding along a chunk border does not plant and drop the same chunk.
pub const KEEP_RADIUS: i32 = 2;
/// Most chunks planted in a frame besides the player's own, which is always
/// planted at once. A chunk's plants take a few milliseconds to work out.
pub const BUDGET: usize = 1;

/// The loaded chunks past [`KEEP_RADIUS`] of `center`, and the chunks within
/// [`LOAD_RADIUS`] still to plant: the player's own first, then the nearest.
fn pending(world: &World, center: Vec2) -> (Vec<ChunkCoord>, Vec<ChunkCoord>) {
    let config = world.config;
    let here = config.chunk_of(center);
    let far = world
        .obstacles
        .loaded_chunks()
        .filter(|c| (c.x - here.x).abs().max((c.y - here.y).abs()) > KEEP_RADIUS)
        .collect();
    let mut wanted: Vec<ChunkCoord> = (-LOAD_RADIUS..=LOAD_RADIUS)
        .flat_map(|dy| (-LOAD_RADIUS..=LOAD_RADIUS).map(move |dx| (dx, dy)))
        .map(|(dx, dy)| ChunkCoord::new(here.x + dx, here.y + dy))
        .filter(|&c| config.has_chunk(c) && !world.obstacles.has_chunk(c))
        .collect();
    let size = config.chunk_size;
    wanted.sort_by(|a, b| {
        let key = |c: &ChunkCoord| (*c != here, c.center(size).distance(center));
        key(a).partial_cmp(&key(b)).expect("finite distances")
    });
    (far, wanted)
}

/// Plants the chunks round `center` and drops the far ones, working out at
/// most `budget` chunks besides the one holding `center`, nearest first.
/// Returns how many chunks were planted and dropped.
pub fn stream_plants(
    world: &mut World,
    flora: &FloraSite,
    center: Vec2,
    budget: usize,
) -> (usize, usize) {
    let here = world.config.chunk_of(center);
    let (far, wanted) = pending(world, center);
    for &c in &far {
        world.unplant_chunk(c);
    }
    let mut planted = 0;
    let mut others = 0;
    for c in wanted {
        if c != here {
            if others == budget {
                break;
            }
            others += 1;
        }
        world.plant_chunk(flora, c);
        planted += 1;
    }
    (planted, far.len())
}

/// Keeps the plants round the player planted, before the player moves.
fn stream_plants_round_player(
    flora: Option<Res<WorldFlora>>,
    mut world: ResMut<GameWorld>,
    players: Query<&Player>,
) {
    let (Some(flora), Ok(player)) = (flora, players.single()) else {
        return;
    };
    let center = player.rider.position;
    // Look first: a frame with nothing to do must not mark the world changed.
    let (far, wanted) = pending(&world.0, center);
    if !far.is_empty() || !wanted.is_empty() {
        stream_plants(&mut world.0, &flora.0, center, BUDGET);
    }
}

pub struct PlantCollisionPlugin;

impl Plugin for PlantCollisionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, stream_plants_round_player.before(DrivePlayer));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::{PlayerPlugin, PlayerSpawn};
    use bevy::time::TimeUpdateStrategy;
    use pampa_content::{TOWNS_FILE, asset_path};
    use pampa_core::{Aabb, Circle, Heading, Rider, StreetLamps};
    use std::time::Duration;

    /// The footprints `near` finds in the three-by-three chunks round (less a
    /// meter at the edge, so a loaded chunk beyond does not touch it)
    /// `center`, in query order.
    fn seen(world: &World, center: Vec2) -> Vec<Aabb> {
        let side = 3.0 * world.config.chunk_size - 2.0;
        let area = Aabb::from_center_size(center, Vec2::new(side, side));
        world.obstacles.near(area).into_iter().copied().collect()
    }

    /// The densest of the first chunks of the monte, with plants all round.
    fn planted_chunk(flora: &FloraSite, world: &World) -> ChunkCoord {
        let size = world.config.chunk_size;
        (-5..=5)
            .flat_map(|y| (-5..=5).map(move |x| ChunkCoord::new(x, y)))
            .filter(|&c| world.config.has_chunk(c))
            .max_by_key(|&c| flora.trees(c).len() + usize::from(c.center(size).x > 0.0))
            .expect("a chunk")
    }

    #[test]
    fn startup_plants_nothing_and_a_chunk_is_planted_only_when_the_player_is_near() {
        let loaded = crate::towns::load_world(&asset_path(TOWNS_FILE));
        let mut laid = crate::lay_out(&loaded);
        let coord = planted_chunk(&laid.flora, &laid.world);
        let trees = laid.flora.trees(coord);
        assert!(trees.len() > 20, "a chunk with trees");
        // The whole world is not planted: no chunk streamed, no tree among
        // the permanent footprints.
        assert_eq!(laid.world.obstacles.streamed_len(), 0);
        assert!(laid.world.obstacles.len() < 200_000);
        for p in trees.iter().map(|t| t.obstacle()) {
            assert!(
                !laid.world.obstacles.contains(&p),
                "{p:?} planted at startup"
            );
        }
        // The player's own chunk is planted at once, the rest within the
        // load radius over the next frames, nearest first.
        let size = laid.world.config.chunk_size;
        let center = coord.center(size);
        let (planted, dropped) = stream_plants(&mut laid.world, &laid.flora, center, 0);
        assert_eq!((planted, dropped), (1, 0));
        assert!(laid.world.obstacles.has_chunk(coord));
        let mut frames = 0;
        while stream_plants(&mut laid.world, &laid.flora, center, BUDGET).0 > 0 {
            frames += 1;
        }
        assert!(frames <= 8, "{frames} frames to fill the ring");
        let loaded_chunks: Vec<_> = laid.world.obstacles.loaded_chunks().collect();
        assert_eq!(loaded_chunks.len(), 9);
        for c in loaded_chunks {
            assert!((c.x - coord.x).abs() <= LOAD_RADIUS && (c.y - coord.y).abs() <= LOAD_RADIUS);
        }
        // The plants collide: each one blocks a rider from a few meters off.
        let collider = Circle::new(Rider::COLLIDER_RADIUS);
        let tree = trees[0];
        let inside = laid.world.resolve(collider, tree.at, |_| ());
        assert!(inside.distance(tree.at) > 0.0, "a tree pushes a body out");
    }

    #[test]
    fn plants_come_and_go_with_the_player_and_come_back_the_same() {
        let loaded = crate::towns::load_world(&asset_path(TOWNS_FILE));
        let mut laid = crate::lay_out(&loaded);
        let coord = planted_chunk(&laid.flora, &laid.world);
        let size = laid.world.config.chunk_size;
        let here = coord.center(size);
        stream_plants(&mut laid.world, &laid.flora, here, 8);
        let first = seen(&laid.world, here);
        assert!(first.len() > 100, "{}", first.len());
        let streamed = laid.world.obstacles.streamed_len();
        // Four chunks east, past the keep radius: the first chunk is gone,
        // and so are its footprints.
        let away = here + Vec2::new(4.0 * size, 0.0);
        let (_, dropped) = stream_plants(&mut laid.world, &laid.flora, away, 8);
        assert!(dropped >= 3, "{dropped}");
        assert!(!laid.world.obstacles.has_chunk(coord));
        // Permanent footprints (fences) stay; no plant is left behind.
        let left: Vec<Aabb> = laid
            .world
            .obstacles
            .near(coord.bounds(size))
            .into_iter()
            .filter(|o| !laid.world.obstacles.contains(o))
            .copied()
            .collect();
        assert!(left.is_empty(), "{} plants left behind", left.len());
        // Ring-sized: the plants held never exceed the ring's chunks.
        assert!(laid.world.obstacles.loaded_chunks().count() <= 25);
        // Riding along a chunk border plants and drops nothing again.
        let edge = here + Vec2::new(size / 2.0, 0.0);
        stream_plants(&mut laid.world, &laid.flora, edge - Vec2::new(0.1, 0.0), 8);
        stream_plants(&mut laid.world, &laid.flora, edge + Vec2::new(0.1, 0.0), 8);
        let held = laid.world.obstacles.streamed_len();
        for dx in [-0.1, 0.1, -0.1, 0.1] {
            let moved = stream_plants(&mut laid.world, &laid.flora, edge + Vec2::new(dx, 0.0), 8);
            assert_eq!(moved, (0, 0), "no churn at a border");
        }
        assert_eq!(laid.world.obstacles.streamed_len(), held);
        // Back again: the same plants, to the footprint.
        stream_plants(&mut laid.world, &laid.flora, here, 8);
        assert_eq!(seen(&laid.world, here), first);
        assert!(laid.world.obstacles.streamed_len() >= streamed);
    }

    /// A headless game with the player at `spawn` and the plant ring.
    fn headless_game(world: World, flora: FloraSite, spawn: PlayerSpawn) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                16,
            )))
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(GameWorld(world))
            .insert_resource(WorldFlora(flora))
            .insert_resource(spawn)
            .add_plugins((PlayerPlugin, PlantCollisionPlugin));
        app
    }

    #[test]
    fn a_rider_cannot_ride_through_a_tree_in_a_chunk_near_them_but_rides_through_a_shrub() {
        let loaded = crate::towns::load_world(&asset_path(TOWNS_FILE));
        let laid = crate::lay_out(&loaded);
        let coord = planted_chunk(&laid.flora, &laid.world);
        let trees = laid.flora.trees(coord);
        let shrubs = laid.flora.shrubs(coord);
        let radius = Rider::COLLIDER_RADIUS;
        let mut full = laid.world.clone();
        full.plant_chunk(&laid.flora, coord);
        // Plants with no tree of the chunk within the gallop lane.
        let lane = |at: Vec2| Aabb::new(at - Vec2::new(1.0, 3.0), at + Vec2::new(11.0, 3.0));
        let lone = |core: Aabb, at: Vec2| {
            full.obstacles
                .near(lane(at))
                .iter()
                .filter(|o| ***o != core)
                .count()
                == 0
        };
        let tree = trees.iter().find(|t| lone(t.obstacle(), t.at)).copied();
        let shrub = shrubs.iter().find(|s| lone(s.obstacle(), s.at)).copied();
        let (tree, shrub) = (tree.expect("a lone tree"), shrub.expect("a lone shrub"));
        for (core, at, blocks) in [
            (tree.obstacle(), tree.at, true),
            (shrub.obstacle(), shrub.at, false),
        ] {
            // The player starts 10 m east of it and gallops west: its chunk
            // is not planted yet.
            let start = at + Vec2::new(10.0, 0.0);
            let spawn = PlayerSpawn {
                position: start,
                heading: Heading::from_radians(std::f32::consts::PI),
                travel: pampa_core::Travel::Riding,
            };
            let mut game = headless_game(laid.world.clone(), laid.flora.clone(), spawn);
            assert_eq!(
                game.world()
                    .resource::<GameWorld>()
                    .0
                    .obstacles
                    .streamed_len(),
                0
            );
            {
                let mut keys = game.world_mut().resource_mut::<ButtonInput<KeyCode>>();
                keys.press(KeyCode::KeyW);
                keys.press(KeyCode::ShiftLeft);
            }
            let mut through = false;
            for _ in 0..240 {
                game.update();
                let mut query = game.world_mut().query::<&Player>();
                let rider = query.single(game.world()).unwrap().rider;
                let gap = core.closest_point(rider.position).distance(rider.position);
                through |= gap < radius - 1e-3;
            }
            let mut query = game.world_mut().query::<&Player>();
            let rider = query.single(game.world()).unwrap().rider;
            if blocks {
                // Stopped against it, not carried past it.
                assert!(!through, "inside the tree");
                assert!(rider.position.x > at.x, "{:?}", rider.position);
                assert!(rider.position.x < at.x + 3.0, "{:?}", rider.position);
            } else {
                assert!(through, "the shrub stopped the rider");
                assert!(rider.position.x < at.x - 1.0, "{:?}", rider.position);
            }
        }
    }

    #[test]
    fn street_lamps_stand_clear_of_every_tree_so_they_need_not_see_the_trees_at_startup() {
        let loaded = crate::towns::load_world(&asset_path(TOWNS_FILE));
        let laid = crate::lay_out(&loaded);
        let mut checked = 0;
        for lamp in laid.lamps.all() {
            let reach = lamp.obstacle().expanded(StreetLamps::CLEARANCE);
            let here = laid.world.config.chunk_of(lamp.obstacle().center());
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let c = ChunkCoord::new(here.x + dx, here.y + dy);
                    for t in laid.flora.trees(c) {
                        assert!(!t.obstacle().intersects(&reach), "{lamp:?} on {t:?}");
                    }
                }
            }
            checked += 1;
        }
        assert!(checked > 50, "{checked} lamps");
    }
}
