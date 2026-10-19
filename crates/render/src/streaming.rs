//! Which terrain chunks should exist around a focus point: pure bookkeeping,
//! testable without a GPU.
//!
//! Chunks within `radius` chunks of the focus chunk are wanted, nearest
//! first. A loaded chunk is dropped only once it is farther than
//! `radius + 1` (hysteresis), so riding back and forth across a chunk border
//! does not rebuild the same chunks. At most `budget` chunks are added per
//! update, so generation never hitches a frame.

use std::collections::BTreeSet;

use pampa_core::{ChunkCoord, Vec2 as GroundVec2, WorldConfig};

/// What one update asks for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StreamingPlan {
    /// Chunks to build now, nearest first.
    pub spawn: Vec<ChunkCoord>,
    /// Chunks to drop now.
    pub despawn: Vec<ChunkCoord>,
}

/// The chunks currently loaded, and the rules for changing them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamingSet {
    /// Load radius, in chunks.
    pub radius: i32,
    /// Most chunks built per update.
    pub budget: usize,
    loaded: BTreeSet<ChunkCoord>,
}

impl StreamingSet {
    /// An empty set loading `radius` chunks around the focus, at most
    /// `budget` per update (at least one).
    pub fn new(radius: i32, budget: usize) -> Self {
        Self {
            radius: radius.max(0),
            budget: budget.max(1),
            loaded: BTreeSet::new(),
        }
    }

    /// Plans the next step towards having every chunk near `focus` loaded
    /// in `world`, and records the plan as done.
    pub fn update(&mut self, world: &WorldConfig, focus: GroundVec2) -> StreamingPlan {
        let center = world.chunk_of(focus);
        let keep = i64::from(self.radius + 1).pow(2);
        let despawn: Vec<ChunkCoord> = self
            .loaded
            .iter()
            .copied()
            .filter(|&c| c.distance_squared(center) > keep || !world.has_chunk(c))
            .collect();
        for c in &despawn {
            self.loaded.remove(c);
        }
        let spawn: Vec<ChunkCoord> = world
            .chunks_near(focus, self.radius)
            .into_iter()
            .filter(|c| !self.loaded.contains(c))
            .take(self.budget)
            .collect();
        self.loaded.extend(spawn.iter().copied());
        StreamingPlan { spawn, despawn }
    }

    /// Whether chunk `c` is loaded.
    pub fn is_loaded(&self, c: ChunkCoord) -> bool {
        self.loaded.contains(&c)
    }

    /// Every loaded chunk, south to north, then west to east.
    pub fn loaded(&self) -> impl Iterator<Item = ChunkCoord> + '_ {
        self.loaded.iter().copied()
    }

    /// How many chunks are loaded.
    pub fn len(&self) -> usize {
        self.loaded.len()
    }

    /// Whether no chunk is loaded.
    pub fn is_empty(&self) -> bool {
        self.loaded.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> WorldConfig {
        WorldConfig::default()
    }

    /// Updates until nothing more is spawned; returns how many updates it took.
    fn settle(set: &mut StreamingSet, focus: GroundVec2) -> usize {
        for n in 1..1000 {
            if set.update(&world(), focus).spawn.is_empty() {
                return n;
            }
        }
        panic!("never settled");
    }

    #[test]
    fn loads_the_disc_around_the_focus_nearest_first_within_budget() {
        let mut set = StreamingSet::new(3, 4);
        let focus = GroundVec2::new(50.0, 50.0);
        let first = set.update(&world(), focus);
        assert_eq!(first.spawn.len(), 4);
        assert_eq!(first.spawn[0], ChunkCoord::new(0, 0));
        assert!(first.despawn.is_empty());
        let updates = settle(&mut set, focus);
        // 29 chunks in a radius-3 disc, 4 per update: 7 more updates build
        // the other 25, and the 8th finds nothing left to build.
        assert_eq!(updates, 8);
        let expected: Vec<ChunkCoord> = {
            let mut v = world().chunks_near(focus, 3);
            v.sort();
            v
        };
        let mut loaded: Vec<ChunkCoord> = set.loaded().collect();
        loaded.sort();
        assert_eq!(loaded, expected);
    }

    #[test]
    fn hysteresis_keeps_chunks_until_past_radius_plus_one() {
        let mut set = StreamingSet::new(2, 100);
        settle(&mut set, GroundVec2::new(100.0, 100.0));
        assert!(set.is_loaded(ChunkCoord::new(-2, 0)));
        // One chunk east: (-2, 0) is now 3 away (= radius + 1): kept.
        let plan = set.update(&world(), GroundVec2::new(300.0, 100.0));
        assert!(!plan.despawn.contains(&ChunkCoord::new(-2, 0)));
        assert!(plan.spawn.contains(&ChunkCoord::new(3, 0)));
        // Back again: nothing to rebuild.
        let plan = set.update(&world(), GroundVec2::new(100.0, 100.0));
        assert!(plan.spawn.is_empty(), "{plan:?}");
        assert!(plan.despawn.is_empty(), "{plan:?}");
        // Two chunks east: (-2, 0) is 4 away and goes.
        let plan = set.update(&world(), GroundVec2::new(500.0, 100.0));
        assert!(plan.despawn.contains(&ChunkCoord::new(-2, 0)));
        assert!(!set.is_loaded(ChunkCoord::new(-2, 0)));
    }

    #[test]
    fn memory_stays_bounded_on_a_long_ride() {
        let mut set = StreamingSet::new(3, 3);
        let mut most = 0;
        for step in 0..2000 {
            let focus = GroundVec2::new(
                -3500.0 + step as f32 * 3.5,
                (step as f32 * 0.01).sin() * 900.0,
            );
            let plan = set.update(&world(), focus);
            assert!(plan.spawn.len() <= 3);
            for c in &plan.despawn {
                assert!(!set.is_loaded(*c));
            }
            most = most.max(set.len());
        }
        // Never more than the radius + 1 disc (45 chunks).
        assert!(most <= ChunkCoord::new(0, 0).within(4).len(), "{most}");
    }

    #[test]
    fn chunks_outside_the_world_are_never_loaded() {
        let mut set = StreamingSet::new(3, 100);
        settle(&mut set, GroundVec2::new(3990.0, -3990.0));
        assert!(!set.is_empty());
        assert!(set.loaded().all(|c| world().has_chunk(c)));
    }
}
