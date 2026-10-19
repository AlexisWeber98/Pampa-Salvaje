//! Townsfolk: simple walkers that wander the streets between intersections.
//!
//! Deterministic for a given seed. Each person walks from one street
//! intersection to a neighboring one, pauses there a moment, then picks the
//! next one at random (avoiding turning straight back when it can). People
//! collide with the same static obstacles as the rider.

use crate::{Aabb, AnimationState, Circle, Heading, TownLayout, Vec2, World};

/// Street intersections and the street segments joining them.
#[derive(Debug, Clone, PartialEq)]
pub struct StreetGraph {
    /// Intersection centers.
    pub nodes: Vec<Vec2>,
    /// For each node, the indices of the nodes one block away along a street.
    pub links: Vec<Vec<usize>>,
}

impl StreetGraph {
    /// The intersections of `town`'s north-south and east-west streets.
    pub fn from_town(town: &TownLayout) -> Self {
        let by = |key: fn(&Aabb) -> f32, keep: fn(&Aabb) -> bool| {
            let mut streets: Vec<Aabb> = town.streets.iter().copied().filter(keep).collect();
            streets.sort_by(|a, b| key(a).total_cmp(&key(b)));
            streets
        };
        let north_south = by(|s| s.center().x, |s| s.size().y > s.size().x);
        let east_west = by(|s| s.center().y, |s| s.size().x > s.size().y);
        let (columns, rows) = (north_south.len(), east_west.len());

        let mut nodes = Vec::with_capacity(columns * rows);
        for row in &east_west {
            for column in &north_south {
                nodes.push(Vec2::new(column.center().x, row.center().y));
            }
        }
        let links = (0..nodes.len())
            .map(|n| {
                let (i, j) = (n % columns, n / columns);
                let mut out = Vec::with_capacity(4);
                if i > 0 {
                    out.push(n - 1);
                }
                if i + 1 < columns {
                    out.push(n + 1);
                }
                if j > 0 {
                    out.push(n - columns);
                }
                if j + 1 < rows {
                    out.push(n + columns);
                }
                out
            })
            .collect();
        Self { nodes, links }
    }
}

/// One walker.
#[derive(Debug, Clone, PartialEq)]
pub struct Townsperson {
    pub position: Vec2,
    pub heading: Heading,
    /// Current ground speed, in m/s.
    pub speed: f32,
    pub collider: Circle,
    /// Appearance variation; render decides what each value looks like.
    pub look: u8,
    walk_speed: f32,
    /// Fixed offset from street centerlines, so people do not walk in file.
    lane: Vec2,
    from: usize,
    to: usize,
    /// Seconds left standing at the current waypoint.
    pause: f32,
    /// Seconds spent making almost no progress toward the target.
    stuck: f32,
    rng: Rng,
}

impl Townsperson {
    /// Collision radius of a person seen from above, in meters.
    pub const COLLIDER_RADIUS: f32 = 0.3;
    /// Fastest walking pace, in m/s.
    pub const WALK_MAX: f32 = 1.6;

    /// What the person is doing, independent of how it is drawn.
    pub fn animation(&self) -> AnimationState {
        AnimationState::from_speed(self.speed, Self::WALK_MAX)
    }

    /// Index of the intersection last reached (or walked away from).
    pub fn last_waypoint(&self) -> usize {
        self.from
    }

    /// Index of the intersection being walked to.
    pub fn next_waypoint(&self) -> usize {
        self.to
    }
}

/// Everyone walking around town, plus the street graph they follow.
#[derive(Debug, Clone, PartialEq)]
pub struct Townsfolk {
    pub graph: StreetGraph,
    pub people: Vec<Townsperson>,
}

/// How many people walk around the Castex-inspired town.
pub const TOWNSFOLK_COUNT: usize = 16;

impl Townsfolk {
    /// `count` people spread over `town`'s streets, placed from `seed`.
    pub fn new(town: &TownLayout, count: usize, seed: u32) -> Self {
        let graph = StreetGraph::from_town(town);
        let people = (0..count)
            .map(|i| Townsperson::spawn(&graph, Rng::new(seed, i as u32)))
            .collect();
        Self { graph, people }
    }

    /// Advances everyone by `dt` seconds inside `world`. Non-finite or
    /// non-positive `dt` changes nothing.
    pub fn step(&mut self, world: &World, dt: f32) {
        if !(dt.is_finite() && dt > 0.0) {
            return;
        }
        for person in &mut self.people {
            person.step(&self.graph, world, dt);
        }
    }
}

/// Largest offset from a street centerline, in meters: keeps people clear of
/// the hitching rail and anything else at the street's edge.
const MAX_LANE: f32 = 0.9;
/// Range of the pause at each intersection, in seconds.
const PAUSE: (f32, f32) = (0.5, 4.0);
/// Range of walking paces, in m/s.
const PACE: (f32, f32) = (1.1, Townsperson::WALK_MAX);
/// Seconds of almost no progress before a person gives up and turns back.
const GIVE_UP: f32 = 2.0;

impl Townsperson {
    fn spawn(graph: &StreetGraph, mut rng: Rng) -> Self {
        let from = rng.below(graph.nodes.len());
        let to = graph.links[from][rng.below(graph.links[from].len())];
        let lane = Vec2::new(
            rng.range(-MAX_LANE, MAX_LANE),
            rng.range(-MAX_LANE, MAX_LANE),
        );
        let (a, b) = (graph.nodes[from], graph.nodes[to]);
        let position = a + (b - a) * rng.range(0.1, 0.9) + lane;
        Self {
            position,
            heading: Heading::from_radians((b.y - a.y).atan2(b.x - a.x)),
            speed: 0.0,
            collider: Circle::new(Self::COLLIDER_RADIUS),
            look: (rng.next() >> 24) as u8,
            walk_speed: rng.range(PACE.0, PACE.1),
            lane,
            from,
            to,
            pause: 0.0,
            stuck: 0.0,
            rng,
        }
    }

    fn step(&mut self, graph: &StreetGraph, world: &World, dt: f32) {
        if self.pause > 0.0 {
            self.pause -= dt;
            self.speed = 0.0;
            return;
        }
        let target = graph.nodes[self.to] + self.lane;
        let offset = target - self.position;
        let distance = offset.length();
        let stride = self.walk_speed * dt;
        let start = self.position;
        if distance <= stride {
            self.position = target;
            self.arrive(graph);
        } else {
            let direction = offset * (1.0 / distance);
            self.heading = Heading::from_radians(direction.y.atan2(direction.x));
            self.position += direction * stride;
            self.speed = self.walk_speed;
        }
        self.position = world.resolve(self.collider, self.position, |_| ());

        // Something in the way (a wall, the tethered horse): turn back.
        if self.speed > 0.0 && self.position.distance(start) < 0.25 * stride {
            self.stuck += dt;
            if self.stuck > GIVE_UP {
                std::mem::swap(&mut self.from, &mut self.to);
                self.stuck = 0.0;
            }
        } else {
            self.stuck = 0.0;
        }
    }

    /// Reached `to`: pick the next intersection and stand still a moment.
    fn arrive(&mut self, graph: &StreetGraph) {
        let came_from = self.from;
        self.from = self.to;
        let links = &graph.links[self.from];
        let onward: Vec<usize> = links.iter().copied().filter(|&n| n != came_from).collect();
        let choices = if onward.is_empty() { links } else { &onward };
        self.to = choices[self.rng.below(choices.len())];
        self.pause = self.rng.range(PAUSE.0, PAUSE.1);
        self.speed = 0.0;
    }
}

/// Small deterministic xorshift generator, so `core` needs no crates.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Rng(u32);

impl Rng {
    /// Generator for item `index` of a `seed`ed set.
    fn new(seed: u32, index: u32) -> Self {
        let mut v = seed ^ index.wrapping_mul(0x9e37_79b9);
        v ^= v >> 16;
        v = v.wrapping_mul(0x7feb_352d);
        v ^= v >> 15;
        v = v.wrapping_mul(0x846c_a68b);
        v ^= v >> 16;
        // Xorshift must never hold zero.
        Self(v | 1)
    }

    fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// Uniform in `[0, 1)`.
    fn unit(&mut self) -> f32 {
        (self.next() >> 8) as f32 / (1u32 << 24) as f32
    }

    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }

    /// Uniform index in `0..n` (`n > 0`).
    fn below(&mut self, n: usize) -> usize {
        ((self.unit() * n as f32) as usize).min(n - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MAX_STEP, Terrain};

    fn town() -> (TownLayout, World) {
        let town = TownLayout::castex_inspired();
        let world = World::with_town(Terrain::default(), &town);
        (town, world)
    }

    fn run(folk: &mut Townsfolk, world: &World, seconds: f32, mut each: impl FnMut(&Townsfolk)) {
        for _ in 0..(seconds / MAX_STEP) as usize {
            folk.step(world, MAX_STEP);
            each(folk);
        }
    }

    #[test]
    fn street_graph_joins_neighboring_intersections_along_streets() {
        let (town, world) = town();
        let graph = StreetGraph::from_town(&town);
        assert_eq!(graph.nodes.len(), 36, "6 x 6 intersections");
        assert_eq!(graph.links.len(), graph.nodes.len());
        let on_street = |p: Vec2| town.streets.iter().any(|s| s.contains_point(p));
        for (a, links) in graph.links.iter().enumerate() {
            assert!((2..=4).contains(&links.len()), "node {a}: {links:?}");
            for &b in links {
                assert!(graph.links[b].contains(&a), "links are symmetric");
                let (pa, pb) = (graph.nodes[a], graph.nodes[b]);
                assert!(pa.x == pb.x || pa.y == pb.y, "straight along a street");
                // Every point of the link is on a street and clear of obstacles.
                for k in 0..=20 {
                    let p = pa + (pb - pa) * (k as f32 / 20.0);
                    assert!(on_street(p), "{p:?} off the street");
                    for o in &world.obstacles {
                        assert!(!o.expanded(1.5).contains_point(p), "{p:?} near {o:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn spawns_a_crowd_on_the_streets() {
        let (town, world) = town();
        let folk = Townsfolk::new(&town, TOWNSFOLK_COUNT, 7);
        assert_eq!(folk.people.len(), TOWNSFOLK_COUNT);
        let looks: std::collections::HashSet<_> = folk.people.iter().map(|p| p.look).collect();
        assert!(looks.len() >= 3, "varied looks: {looks:?}");
        for p in &folk.people {
            assert!(town.streets.iter().any(|s| s.contains_point(p.position)));
            assert_eq!(world.resolve(p.collider, p.position, |_| ()), p.position);
        }
    }

    #[test]
    fn is_deterministic_per_seed() {
        let (town, world) = town();
        let mut a = Townsfolk::new(&town, TOWNSFOLK_COUNT, 7);
        let mut b = Townsfolk::new(&town, TOWNSFOLK_COUNT, 7);
        let mut c = Townsfolk::new(&town, TOWNSFOLK_COUNT, 8);
        for _ in 0..600 {
            a.step(&world, MAX_STEP);
            b.step(&world, MAX_STEP);
            c.step(&world, MAX_STEP);
        }
        assert_eq!(a, b);
        assert_ne!(a.people, c.people);
    }

    #[test]
    fn walks_the_streets_without_entering_buildings() {
        let (town, world) = town();
        let mut folk = Townsfolk::new(&town, TOWNSFOLK_COUNT, 3);
        let mut moving = 0usize;
        run(&mut folk, &world, 600.0, |folk| {
            for p in &folk.people {
                assert!(p.position.x.is_finite() && p.position.y.is_finite());
                for o in &world.obstacles {
                    let gap = p.position.distance(o.closest_point(p.position));
                    assert!(gap >= p.collider.radius - 1e-3, "{p:?} inside {o:?}");
                }
                let on_street = town
                    .streets
                    .iter()
                    .any(|s| s.expanded(0.01).contains_point(p.position));
                assert!(on_street, "{:?} left the streets", p.position);
                assert!(p.speed <= Townsperson::WALK_MAX + 1e-4);
                if p.animation() == AnimationState::Walk {
                    moving += 1;
                }
            }
        });
        assert!(moving > 0);
    }

    #[test]
    fn reaches_waypoints_pausing_between_walks() {
        let (town, world) = town();
        let mut folk = Townsfolk::new(&town, TOWNSFOLK_COUNT, 11);
        let mut visited: Vec<std::collections::HashSet<usize>> =
            vec![Default::default(); folk.people.len()];
        let (mut idle, mut walk) = (0usize, 0usize);
        run(&mut folk, &world, 300.0, |folk| {
            for (i, p) in folk.people.iter().enumerate() {
                visited[i].insert(p.last_waypoint());
                assert!(folk.graph.links[p.last_waypoint()].contains(&p.next_waypoint()));
                match p.animation() {
                    AnimationState::Idle => idle += 1,
                    AnimationState::Walk => walk += 1,
                    AnimationState::Gallop => panic!("people do not gallop"),
                }
            }
        });
        for (i, v) in visited.iter().enumerate() {
            assert!(v.len() >= 4, "person {i} only visited {v:?}");
        }
        assert!(idle > 0 && walk > idle, "mostly walking, sometimes pausing");
    }

    #[test]
    fn heading_follows_the_walk_and_bad_dt_is_a_no_op() {
        let (town, world) = town();
        let mut folk = Townsfolk::new(&town, 4, 5);
        let start = folk.clone();
        for dt in [f32::NAN, f32::INFINITY, -1.0, 0.0] {
            folk.step(&world, dt);
            assert_eq!(folk, start);
        }
        run(&mut folk, &world, 20.0, |folk| {
            for p in &folk.people {
                let to = folk.graph.nodes[p.next_waypoint()] + p.lane - p.position;
                let distance = to.length();
                if p.animation() == AnimationState::Walk && distance > 0.5 {
                    // Facing the next intersection (continuous, not snapped).
                    let facing = p.heading.forward().dot(to * (1.0 / distance));
                    assert!(facing > 0.99, "{p:?} faces away ({facing})");
                }
            }
        });
    }
}
