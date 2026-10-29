//! Static obstacles indexed on a uniform grid, so collision only looks at
//! the footprints near a point instead of every footprint in the world.

use std::collections::HashMap;
use std::ops::Deref;

use crate::{Aabb, ChunkCoord, Vec2, WorldConfig};

/// Every static footprint of the world, with a uniform-grid index.
///
/// Reads like a slice of [`Aabb`] (in insertion order); [`Self::near`]
/// returns only the footprints overlapping an area.
///
/// Besides the permanent footprints it holds a streamed layer: footprints
/// grouped by the chunk they belong to, added and dropped a chunk at a time
/// (see [`Self::set_chunk`]). Only [`Self::near`] sees that layer; the slice
/// view, iteration, `len` and equality cover the permanent footprints.
///
/// A permanent footprint may be low (see [`Self::push_low`]): something a
/// jumping horse clears, such as a fence. [`Self::near_above`] leaves out
/// the low footprints a body is higher than. Everything else, streamed
/// footprints included, is unbounded.
#[derive(Debug, Clone, Default)]
pub struct StaticObstacles {
    items: Vec<Aabb>,
    /// The height of each of `items`, in meters; infinite unless low.
    tops: Vec<f32>,
    /// Footprint indices per grid cell.
    cells: HashMap<(i32, i32), Vec<u32>>,
    /// The streamed footprints of each loaded chunk.
    chunks: HashMap<ChunkCoord, Vec<Aabb>>,
    /// Streamed footprints per grid cell: the chunk and index in it.
    chunk_cells: HashMap<(i32, i32), Vec<(ChunkCoord, u32)>>,
}

impl StaticObstacles {
    /// Side of one index cell, in meters: about a house lot.
    pub const CELL: f32 = 16.0;

    /// The clearance of a body on the ground ([`Self::near_above`]): every
    /// footprint blocks it, however low.
    pub const ON_GROUND: f32 = f32::NEG_INFINITY;

    /// The obstacles in `items`, indexed.
    pub fn new(items: Vec<Aabb>) -> Self {
        let mut obstacles = Self::default();
        for item in items {
            obstacles.push(item);
        }
        obstacles
    }

    /// Adds one footprint, unbounded upward.
    pub fn push(&mut self, item: Aabb) {
        self.push_low(item, f32::INFINITY);
    }

    /// Adds one footprint `height` meters tall, which a body higher than
    /// that clears ([`Self::near_above`]).
    pub fn push_low(&mut self, item: Aabb, height: f32) {
        let index = self.items.len() as u32;
        for cell in Self::cells_of(item) {
            self.cells.entry(cell).or_default().push(index);
        }
        self.items.push(item);
        self.tops.push(height);
    }

    /// Adds every footprint of `items`.
    pub fn extend(&mut self, items: impl IntoIterator<Item = Aabb>) {
        for item in items {
            self.push(item);
        }
    }

    /// Removes the first footprint equal to `item` and rebuilds the index,
    /// keeping the others in order. Returns whether one was removed. Rare
    /// (a tied horse led away), so a full rebuild is fine.
    pub fn remove(&mut self, item: &Aabb) -> bool {
        let Some(index) = self.items.iter().position(|o| o == item) else {
            return false;
        };
        let mut items = std::mem::take(&mut self.items);
        let mut tops = std::mem::take(&mut self.tops);
        items.remove(index);
        tops.remove(index);
        *self = Self::default();
        for (item, top) in items.into_iter().zip(tops) {
            self.push_low(item, top);
        }
        true
    }

    /// Makes `items` the streamed footprints of `coord`, replacing any it
    /// held. An empty `items` still marks the chunk as loaded. A footprint
    /// with a corner past the largest possible world
    /// ([`WorldConfig::MAX_HALF_EXTENT`]), or not finite, is dropped:
    /// indexing it could take effectively forever, and no real footprint
    /// reaches there ([`Self::near`] likewise finds nothing for a
    /// non-finite area).
    pub fn set_chunk(&mut self, coord: ChunkCoord, mut items: Vec<Aabb>) {
        self.clear_chunk(coord);
        // `abs() <= max` is false for NaN and infinities too.
        items.retain(|i| {
            [i.min.x, i.min.y, i.max.x, i.max.y]
                .iter()
                .all(|v| v.abs() <= WorldConfig::MAX_HALF_EXTENT)
        });
        for (index, item) in items.iter().enumerate() {
            for cell in Self::cells_of(*item) {
                self.chunk_cells
                    .entry(cell)
                    .or_default()
                    .push((coord, index as u32));
            }
        }
        self.chunks.insert(coord, items);
    }

    /// Drops the streamed footprints of `coord`. Returns whether the chunk
    /// was loaded.
    pub fn clear_chunk(&mut self, coord: ChunkCoord) -> bool {
        let Some(items) = self.chunks.remove(&coord) else {
            return false;
        };
        for item in items {
            for cell in Self::cells_of(item) {
                if let Some(entries) = self.chunk_cells.get_mut(&cell) {
                    entries.retain(|(c, _)| *c != coord);
                    if entries.is_empty() {
                        self.chunk_cells.remove(&cell);
                    }
                }
            }
        }
        true
    }

    /// Whether `coord` holds streamed footprints (possibly none).
    pub fn has_chunk(&self, coord: ChunkCoord) -> bool {
        self.chunks.contains_key(&coord)
    }

    /// The chunks that hold streamed footprints.
    pub fn loaded_chunks(&self) -> impl Iterator<Item = ChunkCoord> + '_ {
        self.chunks.keys().copied()
    }

    /// How many streamed footprints are loaded, over every chunk.
    pub fn streamed_len(&self) -> usize {
        self.chunks.values().map(Vec::len).sum()
    }

    /// Every footprint overlapping or touching `area`, once each: the
    /// permanent ones in insertion order, then the streamed ones by chunk
    /// and index. A non-finite `area` finds nothing.
    pub fn near(&self, area: Aabb) -> Vec<&Aabb> {
        self.near_above(area, Self::ON_GROUND)
    }

    /// Like [`Self::near`], leaving out the low footprints no taller than
    /// `clearance` meters: what a body that high above the ground runs
    /// into.
    pub fn near_above(&self, area: Aabb, clearance: f32) -> Vec<&Aabb> {
        let finite = [area.min.x, area.min.y, area.max.x, area.max.y]
            .iter()
            .all(|v| v.is_finite());
        if !finite {
            return Vec::new();
        }
        let mut found: Vec<u32> = Self::cells_of(area)
            .filter_map(|cell| self.cells.get(&cell))
            .flatten()
            .copied()
            .collect();
        found.sort_unstable();
        found.dedup();
        let mut streamed: Vec<(ChunkCoord, u32)> = Self::cells_of(area)
            .filter_map(|cell| self.chunk_cells.get(&cell))
            .flatten()
            .copied()
            .collect();
        streamed.sort_unstable();
        streamed.dedup();
        found
            .into_iter()
            .filter(|&i| self.tops[i as usize] > clearance)
            .map(|i| &self.items[i as usize])
            .chain(
                streamed
                    .into_iter()
                    .map(|(coord, i)| &self.chunks[&coord][i as usize]),
            )
            .filter(|o| touches(o, &area))
            .collect()
    }

    /// How many indexed footprints a query of `area` looks at before
    /// keeping the overlapping ones: what [`Self::near`] costs.
    #[cfg(test)]
    fn scanned(&self, area: Aabb) -> usize {
        Self::cells_of(area)
            .filter_map(|cell| self.cells.get(&cell))
            .map(Vec::len)
            .sum()
    }

    /// The grid cells `area` covers.
    fn cells_of(area: Aabb) -> impl Iterator<Item = (i32, i32)> {
        let cell = |v: f32| (v / Self::CELL).floor() as i32;
        let (x0, y0) = (cell(area.min.x), cell(area.min.y));
        let (x1, y1) = (cell(area.max.x), cell(area.max.y));
        (y0..=y1).flat_map(move |y| (x0..=x1).map(move |x| (x, y)))
    }
}

impl Deref for StaticObstacles {
    type Target = [Aabb];

    fn deref(&self) -> &[Aabb] {
        &self.items
    }
}

impl<'a> IntoIterator for &'a StaticObstacles {
    type Item = &'a Aabb;
    type IntoIter = std::slice::Iter<'a, Aabb>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

impl PartialEq for StaticObstacles {
    fn eq(&self, other: &Self) -> bool {
        self.items == other.items
    }
}

impl PartialEq<Vec<Aabb>> for StaticObstacles {
    fn eq(&self, other: &Vec<Aabb>) -> bool {
        &self.items == other
    }
}

impl From<Vec<Aabb>> for StaticObstacles {
    fn from(items: Vec<Aabb>) -> Self {
        Self::new(items)
    }
}

/// Whether two boxes overlap or share an edge.
fn touches(a: &Aabb, b: &Aabb) -> bool {
    a.min.x <= b.max.x && b.min.x <= a.max.x && a.min.y <= b.max.y && b.min.y <= a.max.y
}

/// A square query area of half side `reach` around `center`.
pub(crate) fn around(center: Vec2, reach: f32) -> Aabb {
    Aabb::from_center_size(center, Vec2::new(2.0 * reach, 2.0 * reach))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Rng;

    fn scattered(n: u32) -> Vec<Aabb> {
        let mut rng = Rng::new(5, 0);
        (0..n)
            .map(|_| {
                let c = Vec2::new(rng.range(-300.0, 300.0), rng.range(-300.0, 300.0));
                let s = Vec2::new(rng.range(0.2, 60.0), rng.range(0.2, 25.0));
                Aabb::from_center_size(c, s)
            })
            .collect()
    }

    #[test]
    fn a_low_footprint_is_cleared_only_above_its_top() {
        let wall = Aabb::from_center_size(Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0));
        let fence = Aabb::from_center_size(Vec2::new(2.0, 0.0), Vec2::new(4.0, 0.15));
        let mut obstacles = StaticObstacles::default();
        obstacles.push(wall);
        obstacles.push_low(fence, 1.3);
        let area = around(Vec2::ZERO, 10.0);
        let found = |clearance| -> Vec<Aabb> {
            obstacles
                .near_above(area, clearance)
                .into_iter()
                .copied()
                .collect()
        };
        assert_eq!(found(0.0), vec![wall, fence]);
        assert_eq!(found(1.29), vec![wall, fence]);
        // At its top or higher, the fence is cleared; the wall never is.
        assert_eq!(found(1.3), vec![wall]);
        assert_eq!(found(50.0), vec![wall]);
        // Plain queries and the slice view still see every footprint.
        assert_eq!(obstacles.near(area).len(), 2);
        assert_eq!(obstacles, vec![wall, fence]);
    }

    #[test]
    fn streamed_footprints_are_never_cleared() {
        let mut obstacles = StaticObstacles::default();
        let trunk = Aabb::from_center_size(Vec2::new(10.0, 10.0), Vec2::new(0.6, 0.6));
        obstacles.set_chunk(ChunkCoord::new(0, 0), vec![trunk]);
        let found = obstacles.near_above(around(Vec2::new(10.0, 10.0), 2.0), 100.0);
        assert_eq!(found, vec![&trunk]);
    }

    #[test]
    fn removing_a_footprint_keeps_the_others_heights() {
        let a = Aabb::from_center_size(Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0));
        let b = Aabb::from_center_size(Vec2::new(3.0, 0.0), Vec2::new(1.0, 1.0));
        let c = Aabb::from_center_size(Vec2::new(6.0, 0.0), Vec2::new(1.0, 1.0));
        let mut obstacles = StaticObstacles::default();
        obstacles.push_low(a, 1.3);
        obstacles.push_low(b, 0.6);
        obstacles.push(c);
        assert!(obstacles.remove(&a));
        let found = obstacles.near_above(around(Vec2::new(3.0, 0.0), 10.0), 1.0);
        assert_eq!(found, vec![&c]);
    }

    #[test]
    fn a_non_finite_streamed_footprint_is_dropped_and_the_rest_kept() {
        let mut obstacles = StaticObstacles::default();
        let coord = ChunkCoord::new(0, 0);
        let good = Aabb::from_center_size(Vec2::new(10.0, 10.0), Vec2::new(1.0, 1.0));
        let nan = Aabb::from_center_size(Vec2::new(f32::NAN, 10.0), Vec2::new(1.0, 1.0));
        let inf = Aabb::from_center_size(Vec2::new(10.0, 10.0), Vec2::new(f32::INFINITY, 1.0));
        obstacles.set_chunk(coord, vec![nan, good, inf]);
        // The chunk is loaded; only the finite footprint is held and found.
        assert!(obstacles.has_chunk(coord));
        assert_eq!(obstacles.streamed_len(), 1);
        let found: Vec<Aabb> = obstacles
            .near(around(Vec2::new(10.0, 10.0), 5.0))
            .into_iter()
            .copied()
            .collect();
        assert_eq!(found, vec![good]);
        // Clearing leaves no stale index entry behind.
        assert!(obstacles.clear_chunk(coord));
        assert!(
            obstacles
                .near(around(Vec2::new(10.0, 10.0), 5.0))
                .is_empty()
        );
    }

    #[test]
    fn a_streamed_footprint_reaching_past_any_world_is_dropped() {
        let mut obstacles = StaticObstacles::default();
        let coord = ChunkCoord::new(0, 0);
        let good = Aabb::from_center_size(Vec2::new(10.0, 10.0), Vec2::new(1.0, 1.0));
        // Finite, but spanning about 1e30 m: indexing it would never end.
        let huge = Aabb::from_center_size(Vec2::new(10.0, 10.0), Vec2::new(1e30, 1.0));
        obstacles.set_chunk(coord, vec![huge, good]);
        assert_eq!(obstacles.streamed_len(), 1);
        let found: Vec<Aabb> = obstacles
            .near(around(Vec2::new(10.0, 10.0), 5.0))
            .into_iter()
            .copied()
            .collect();
        assert_eq!(found, vec![good]);
    }

    #[test]
    fn a_streamed_chunk_is_found_by_queries_and_gone_when_cleared() {
        let mut obstacles = StaticObstacles::new(scattered(50));
        let permanent: Vec<Aabb> = obstacles.to_vec();
        let (a, b) = (ChunkCoord::new(0, 0), ChunkCoord::new(1, 0));
        // One box spans several grid cells and must be reported once.
        let wide = Aabb::from_center_size(Vec2::new(500.0, 500.0), Vec2::new(40.0, 3.0));
        let small = Aabb::from_center_size(Vec2::new(510.0, 520.0), Vec2::new(1.0, 1.0));
        obstacles.set_chunk(a, vec![wide, small]);
        obstacles.set_chunk(b, Vec::new());
        assert!(obstacles.has_chunk(a) && obstacles.has_chunk(b));
        assert_eq!(obstacles.streamed_len(), 2);
        let area = around(Vec2::new(505.0, 505.0), 30.0);
        let found: Vec<Aabb> = obstacles.near(area).into_iter().copied().collect();
        assert_eq!(found, vec![wide, small]);
        // The slice view and equality still read the permanent ones only.
        assert_eq!(obstacles.len(), permanent.len());
        assert_eq!(obstacles, permanent);
        // Replacing a chunk drops what it held.
        obstacles.set_chunk(a, vec![small]);
        assert_eq!(obstacles.near(area).len(), 1);
        assert!(obstacles.clear_chunk(a));
        assert!(!obstacles.clear_chunk(a));
        assert!(obstacles.near(area).is_empty());
        assert_eq!(obstacles.streamed_len(), 0);
        assert_eq!(obstacles.loaded_chunks().collect::<Vec<_>>(), vec![b]);
    }

    #[test]
    fn queries_stay_local_with_a_hundred_thousand_obstacles() {
        // Fence-like boxes every 4 m over about 1.3 km square.
        let side = 316;
        let items: Vec<Aabb> = (0..side * side)
            .map(|k| {
                let p = Vec2::new((k % side) as f32 * 4.0, (k / side) as f32 * 4.0);
                Aabb::from_center_size(p, Vec2::new(1.0, 0.15))
            })
            .collect();
        assert!(items.len() >= 99_856);
        let started = std::time::Instant::now();
        let obstacles = StaticObstacles::new(items.clone());
        assert!(
            started.elapsed().as_secs_f32() < 5.0,
            "{:?}",
            started.elapsed()
        );
        let mut rng = Rng::new(20, 0);
        for _ in 0..200 {
            let at = Vec2::new(rng.range(0.0, 1264.0), rng.range(0.0, 1264.0));
            let area = around(at, 2.0);
            // A rider's query looks at a few cells' worth, not the world.
            let scanned = obstacles.scanned(area);
            assert!(scanned <= 200, "{scanned}");
            let found = obstacles.near(area);
            let expected: Vec<&Aabb> = items.iter().filter(|o| touches(o, &area)).collect();
            assert_eq!(found, expected);
        }
    }

    #[test]
    fn near_finds_exactly_the_overlapping_footprints_once_in_order() {
        let items = scattered(400);
        let obstacles = StaticObstacles::new(items.clone());
        assert_eq!(obstacles, items);
        assert_eq!(obstacles.len(), 400);
        let mut rng = Rng::new(8, 1);
        for _ in 0..300 {
            let c = Vec2::new(rng.range(-350.0, 350.0), rng.range(-350.0, 350.0));
            let area = around(c, rng.range(0.1, 40.0));
            let expected: Vec<&Aabb> = items.iter().filter(|o| touches(o, &area)).collect();
            assert_eq!(obstacles.near(area), expected, "{area:?}");
        }
    }

    #[test]
    fn pushing_keeps_the_index_in_step() {
        let mut obstacles = StaticObstacles::default();
        assert!(obstacles.near(around(Vec2::ZERO, 1000.0)).is_empty());
        let wall = Aabb::new(Vec2::new(-20.0, 2.0), Vec2::new(20.0, 6.0));
        obstacles.push(wall);
        obstacles.extend([Aabb::from_center_size(
            Vec2::new(5000.0, -3000.0),
            Vec2::new(2.0, 2.0),
        )]);
        assert_eq!(
            obstacles.near(around(Vec2::new(0.0, 4.0), 0.5)),
            vec![&wall]
        );
        assert_eq!(
            obstacles
                .near(around(Vec2::new(4999.0, -3000.0), 0.5))
                .len(),
            1
        );
        assert!(
            obstacles
                .near(around(Vec2::new(100.0, 100.0), 5.0))
                .is_empty()
        );
        assert_eq!((&obstacles).into_iter().count(), 2);
    }

    #[test]
    fn removing_a_footprint_drops_it_from_the_list_and_the_index() {
        let items = scattered(200);
        let mut obstacles = StaticObstacles::new(items.clone());
        let gone = items[57];
        assert!(obstacles.remove(&gone));
        assert_eq!(obstacles.len(), 199);
        let mut rest = items.clone();
        rest.remove(57);
        assert_eq!(obstacles, rest, "the others keep their order");
        let mut rng = Rng::new(9, 2);
        for _ in 0..200 {
            let c = Vec2::new(rng.range(-350.0, 350.0), rng.range(-350.0, 350.0));
            let area = around(c, rng.range(0.1, 40.0));
            let expected: Vec<&Aabb> = rest.iter().filter(|o| touches(o, &area)).collect();
            assert_eq!(obstacles.near(area), expected, "{area:?}");
        }
        assert!(!obstacles.near(gone).contains(&&gone));
        // Removing what is not there changes nothing.
        assert!(!obstacles.remove(&gone));
        let missing = Aabb::from_center_size(Vec2::new(9000.0, 9000.0), Vec2::new(1.0, 1.0));
        assert!(!obstacles.remove(&missing));
        assert_eq!(obstacles, rest);
    }
}
