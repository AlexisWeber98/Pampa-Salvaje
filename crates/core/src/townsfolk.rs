//! Townsfolk: simple walkers that wander the streets between intersections.
//!
//! Deterministic for a given seed. Each person walks from one street
//! intersection to a neighboring one, pauses there a moment, then picks the
//! next one at random (avoiding turning straight back when it can). People
//! collide with the same static obstacles as the rider.

use crate::rng::Rng;
use crate::{Aabb, AnimationState, Body, Circle, Heading, TownLayout, Vec2, World};

/// Street intersections and the street segments joining them.
#[derive(Debug, Clone, PartialEq)]
pub struct StreetGraph {
    /// Intersection centers.
    pub nodes: Vec<Vec2>,
    /// For each node, the indices of the nodes one block away along a street.
    pub links: Vec<Vec<usize>>,
}

impl StreetGraph {
    /// The intersections of `town`'s north-south and east-west streets, row
    /// by row from the south-west, each linked to its neighbors one block
    /// away. A merged plaza has no street through it: intersections inside
    /// it are left out, and no link crosses it.
    pub fn from_town(town: &TownLayout) -> Self {
        // Centerlines of the streets of one direction, in order; a street
        // cut by a plaza counts once.
        let lines = |key: fn(&Aabb) -> f32, keep: fn(&Aabb) -> bool| {
            let mut lines: Vec<f32> = town.streets.iter().filter(|s| keep(s)).map(key).collect();
            lines.sort_by(f32::total_cmp);
            lines.dedup();
            lines
        };
        let xs = lines(|s| s.center().x, |s| s.size().y > s.size().x);
        let ys = lines(|s| s.center().y, |s| s.size().x > s.size().y);
        let on_street = |p: Vec2| town.streets.iter().any(|s| s.contains_point(p));

        // Node index of each intersection on a street, row by row.
        let mut nodes = Vec::with_capacity(xs.len() * ys.len());
        let mut index = vec![None; xs.len() * ys.len()];
        for (j, &y) in ys.iter().enumerate() {
            for (i, &x) in xs.iter().enumerate() {
                let p = Vec2::new(x, y);
                if on_street(p) {
                    index[j * xs.len() + i] = Some(nodes.len());
                    nodes.push(p);
                }
            }
        }
        let columns = xs.len();
        let mut links = vec![Vec::with_capacity(4); nodes.len()];
        for (cell, node) in index.iter().enumerate() {
            let Some(node) = *node else {
                continue;
            };
            let (i, j) = (cell % columns, cell / columns);
            let neighbors = [
                (i > 0).then(|| cell - 1),
                (i + 1 < columns).then(|| cell + 1),
                (j > 0).then(|| cell - columns),
                (j + 1 < ys.len()).then(|| cell + columns),
            ];
            for other in neighbors.into_iter().flatten() {
                if let Some(other) = index[other]
                    && on_street((nodes[node] + nodes[other]) * 0.5)
                {
                    links[node].push(other);
                }
            }
        }
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
    /// Fastest walking pace, in m/s: an easy street walk is 1.25 to 1.5
    /// (the animation's walk, stood up for 1919, steps naturally at about
    /// 1.27 m/s).
    pub const WALK_MAX: f32 = 1.5;

    /// What the person is doing, independent of how it is drawn.
    pub fn animation(&self) -> AnimationState {
        AnimationState::from_speed(self.speed, Self::WALK_MAX)
    }

    /// Where this person stands, as an obstacle for others.
    pub fn body(&self) -> Body {
        Body {
            position: self.position,
            collider: self.collider,
        }
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

/// How many people walk around a small town, such as the Castex-inspired
/// one. Bigger towns get more ([`TownLayout::townsfolk_count`]).
pub const TOWNSFOLK_COUNT: usize = 16;

impl Townsfolk {
    /// `count` people spread over `town`'s streets, placed from `seed`.
    pub fn new(town: &TownLayout, count: usize, seed: u32) -> Self {
        Self::from_graph(StreetGraph::from_town(town), count, seed)
    }

    /// `count` people spread over `graph`, placed from `seed`.
    ///
    /// Degenerate graphs never panic: with no intersections nobody spawns,
    /// and people placed on an intersection with no streets leading away
    /// stand there idle.
    pub fn from_graph(graph: StreetGraph, count: usize, seed: u32) -> Self {
        let people = if graph.nodes.is_empty() {
            Vec::new()
        } else {
            (0..count)
                .map(|i| Townsperson::spawn(&graph, Rng::new(seed, i as u32)))
                .collect()
        };
        Self { graph, people }
    }

    /// Advances everyone by `dt` seconds inside `world`. Non-finite or
    /// non-positive `dt` changes nothing.
    pub fn step(&mut self, world: &World, dt: f32) {
        self.step_around(world, &[], dt);
    }

    /// Like [`Self::step`], with `obstacles` (the rider) that people walk
    /// around instead of through.
    pub fn step_around(&mut self, world: &World, obstacles: &[Body], dt: f32) {
        if !(dt.is_finite() && dt > 0.0) {
            return;
        }
        for person in &mut self.people {
            person.step(&self.graph, world, obstacles, dt);
        }
    }

    /// Shoves everyone overlapping `body` (a rider barging through) out of
    /// its way, without pushing anyone into the world's obstacles. Returns
    /// the indices of the people it touched, in order (the contacts that
    /// [`crate::BumpTracker`] turns into complaints).
    pub fn make_way(&mut self, world: &World, body: Body) -> Vec<usize> {
        let mut touched = Vec::new();
        for (index, person) in self.people.iter_mut().enumerate() {
            if let Some(push) = person.collider.push_out_of_body(person.position, &body) {
                person.position = world.resolve(person.collider, person.position + push, |_| ());
                touched.push(index);
            }
        }
        touched
    }

    /// Where everyone stands, as obstacles for others.
    pub fn bodies(&self) -> Vec<Body> {
        self.people.iter().map(Townsperson::body).collect()
    }
}

/// Largest offset from a street centerline, in meters: keeps people clear of
/// the hitching rail and anything else at the street's edge.
const MAX_LANE: f32 = 0.7;
/// Range of the pause at each intersection, in seconds.
const PAUSE: (f32, f32) = (0.5, 4.0);
/// Range of walking paces, in m/s.
const PACE: (f32, f32) = (1.25, Townsperson::WALK_MAX);
/// Seconds of almost no progress before a person gives up and turns back.
const GIVE_UP: f32 = 2.0;

impl Townsperson {
    /// A person somewhere along a street of `graph` (which has nodes). On an
    /// isolated intersection the person starts, and stays, right there.
    fn spawn(graph: &StreetGraph, mut rng: Rng) -> Self {
        let from = rng.below(graph.nodes.len());
        let links = &graph.links[from];
        let to = if links.is_empty() {
            from
        } else {
            links[rng.below(links.len())]
        };
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

    fn step(&mut self, graph: &StreetGraph, world: &World, obstacles: &[Body], dt: f32) {
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
        self.position = world.resolve_among(self.collider, self.position, obstacles, |_| ());

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
        // Nowhere to go from an isolated intersection: stay put.
        self.to = if choices.is_empty() {
            self.from
        } else {
            choices[self.rng.below(choices.len())]
        };
        self.pause = self.rng.range(PAUSE.0, PAUSE.1);
        self.speed = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::town::tests::{sample_configs, sample_towns};
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
        // A walker strays at most this far from the centerline.
        let reach = MAX_LANE + Townsperson::COLLIDER_RADIUS;
        for (c, town) in sample_configs().iter().zip(sample_towns()) {
            let world = World::with_town(Terrain::default(), &town);
            let graph = StreetGraph::from_town(&town);
            let (cols, rows) = c.blocks;
            // Intersections inside a merged plaza are gone.
            let (w, h) = town.size.plaza_span(c.blocks);
            let inside = (w - 1) * (h - 1);
            // A line through the town adds a row of crossings by its band.
            let band = if town.rail_band().is_some() {
                cols + 1
            } else {
                0
            };
            assert_eq!(
                graph.nodes.len(),
                (cols + 1) * (rows + 1) - inside + band,
                "{}",
                c.name
            );
            assert_eq!(graph.links.len(), graph.nodes.len());
            let on_street = |p: Vec2| town.streets.iter().any(|s| s.contains_point(p));
            for (a, links) in graph.links.iter().enumerate() {
                assert!((2..=4).contains(&links.len()), "node {a}: {links:?}");
                for &b in links {
                    assert!(graph.links[b].contains(&a), "links are symmetric");
                    let (pa, pb) = (graph.nodes[a], graph.nodes[b]);
                    assert!(pa.x == pb.x || pa.y == pb.y, "straight along a street");
                    // Every point of the link is on a street and clear of
                    // obstacles by more than a walker's reach.
                    for k in 0..=20 {
                        let p = pa + (pb - pa) * (k as f32 / 20.0);
                        assert!(on_street(p), "{p:?} off the street");
                        for o in &world.obstacles {
                            assert!(
                                !o.expanded(reach + 0.05).contains_point(p),
                                "{}: {p:?} near {o:?}",
                                c.name
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn walkers_stay_on_the_streets_of_every_town() {
        for town in sample_towns() {
            let world = World::with_town(Terrain::default(), &town);
            let mut folk = Townsfolk::new(&town, 8, 5);
            run(&mut folk, &world, 90.0, |folk| {
                for p in &folk.people {
                    for o in &world.obstacles {
                        let gap = p.position.distance(o.closest_point(p.position));
                        assert!(
                            gap >= p.collider.radius - 1e-3,
                            "{}: {p:?} inside {o:?}",
                            town.name
                        );
                    }
                    let on_street = town
                        .streets
                        .iter()
                        .any(|s| s.expanded(0.01).contains_point(p.position));
                    assert!(
                        on_street,
                        "{}: {:?} left the streets",
                        town.name, p.position
                    );
                }
            });
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
    fn people_walk_at_an_easy_street_pace() {
        // 1.25 to 1.5 m/s, each their own: a 1919 street, nobody hurrying.
        let (town, _) = town();
        let folk = Townsfolk::new(&town, 200, 3);
        let paces: Vec<f32> = folk.people.iter().map(|p| p.walk_speed).collect();
        assert!(paces.iter().all(|v| (1.25..=1.5).contains(v)), "{paces:?}");
        let (lo, hi) = paces
            .iter()
            .fold((f32::MAX, f32::MIN), |(l, h), v| (l.min(*v), h.max(*v)));
        assert!(lo < 1.3 && hi > 1.45, "varied: {lo} to {hi}");
        assert_eq!(Townsperson::WALK_MAX, 1.5);
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
        for dt in [-1.0, 0.0] {
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

    #[test]
    fn a_town_without_intersections_has_nobody_walking() {
        let (mut town, world) = town();
        town.streets.clear();
        let mut folk = Townsfolk::new(&town, TOWNSFOLK_COUNT, 7);
        assert!(folk.graph.nodes.is_empty());
        assert!(folk.people.is_empty());
        folk.step(&world, MAX_STEP);
    }

    #[test]
    fn people_on_an_isolated_intersection_stand_there() {
        let (mut town, world) = town();
        // Keep one north-south and one east-west street: a single crossing.
        let ns = *town
            .streets
            .iter()
            .find(|s| s.size().y > s.size().x)
            .unwrap();
        let ew = *town
            .streets
            .iter()
            .find(|s| s.size().x > s.size().y)
            .unwrap();
        town.streets = vec![ns, ew];
        let mut folk = Townsfolk::new(&town, 3, 7);
        assert_eq!(folk.graph.nodes.len(), 1);
        assert!(folk.graph.links[0].is_empty());
        assert_eq!(folk.people.len(), 3);
        let node = folk.graph.nodes[0];
        run(&mut folk, &world, 30.0, |folk| {
            for p in &folk.people {
                assert!(p.position.distance(node) <= MAX_LANE * 1.5, "{p:?}");
                assert_eq!(p.animation(), AnimationState::Idle);
                assert_eq!((p.last_waypoint(), p.next_waypoint()), (0, 0));
            }
        });
    }

    #[test]
    fn isolated_nodes_in_a_larger_graph_never_panic() {
        let graph = StreetGraph {
            nodes: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(20.0, 0.0),
                Vec2::new(500.0, 500.0),
            ],
            links: vec![vec![1], vec![0], vec![]],
        };
        let world = World::default();
        let mut folk = Townsfolk::from_graph(graph, 24, 3);
        assert_eq!(folk.people.len(), 24);
        run(&mut folk, &world, 60.0, |folk| {
            for p in &folk.people {
                assert!(p.position.x.is_finite() && p.position.y.is_finite());
            }
        });
        assert!(folk.people.iter().any(|p| p.last_waypoint() == 2));
    }

    /// No person overlaps `body` (beyond rounding).
    fn assert_clear_of(folk: &Townsfolk, body: Body, when: &str) {
        for p in &folk.people {
            let gap = p.position.distance(body.position);
            let needed = p.collider.radius + body.collider.radius;
            assert!(gap >= needed - 1e-3, "{when}: {p:?} overlaps {body:?}");
        }
    }

    #[test]
    fn people_walk_around_a_standing_rider() {
        let (town, world) = town();
        let mut folk = Townsfolk::new(&town, TOWNSFOLK_COUNT, 3);
        // A rider standing still on each person's path, one at a time.
        for i in 0..4 {
            let p = &folk.people[i];
            let ahead = folk.graph.nodes[p.next_waypoint()] - p.position;
            let rider = Body {
                position: p.position + ahead * 0.5,
                collider: Circle::new(0.9),
            };
            let mut met = false;
            for _ in 0..(60.0 / MAX_STEP) as usize {
                folk.step_around(&world, &[rider], MAX_STEP);
                assert_clear_of(&folk, rider, "walking");
                met |= folk.people[i].position.distance(rider.position) < 2.0;
            }
            assert!(met, "person {i} came by the rider");
        }
    }

    #[test]
    fn a_rider_shoves_people_aside_and_cannot_pass_through_them() {
        let (town, world) = town();
        let mut folk = Townsfolk::new(&town, TOWNSFOLK_COUNT, 3);
        let params = crate::RiderParams::default();
        let gallop = crate::RiderInput {
            throttle: 1.0,
            gallop: true,
            turn: 0.3,
        };
        // Start right on top of the first person.
        let at = folk.people[0].position;
        let mut rider = crate::Rider::new(at, Heading::from_radians(0.0), &world);
        let mut shoved = 0;
        for frame in 0..(120.0 / MAX_STEP) as usize {
            folk.step_around(&world, &[rider.body()], MAX_STEP);
            let input = crate::RiderInput {
                turn: if (frame / 100) % 2 == 0 { 0.3 } else { -0.3 },
                ..gallop
            };
            rider.step(input, &params, &world, MAX_STEP);
            let before = folk.clone();
            folk.make_way(&world, rider.body());
            if folk != before {
                shoved += 1;
            }
            rider.keep_clear_of(&world, &folk.bodies());
            assert_clear_of(&folk, rider.body(), &format!("frame {frame}"));
            for p in &folk.people {
                for o in &world.obstacles {
                    let gap = p.position.distance(o.closest_point(p.position));
                    assert!(gap >= p.collider.radius - 1e-3, "{p:?} shoved into {o:?}");
                }
            }
        }
        assert!(shoved > 0, "the rider met somebody");
    }

    #[test]
    fn make_way_reports_exactly_the_people_it_touched() {
        let graph = StreetGraph {
            nodes: vec![Vec2::ZERO],
            links: vec![vec![]],
        };
        let world = World::default();
        let mut folk = Townsfolk::from_graph(graph, 3, 1);
        folk.people[0].position = Vec2::new(0.5, 0.0);
        folk.people[1].position = Vec2::new(10.0, 0.0);
        folk.people[2].position = Vec2::new(-0.2, 0.9);
        let rider = Body {
            position: Vec2::ZERO,
            collider: Circle::new(0.9),
        };
        assert_eq!(folk.make_way(&world, rider), [0, 2]);
        assert_clear_of(&folk, rider, "shoved");
        // Nobody left in the way: no contacts.
        assert!(folk.make_way(&world, rider).is_empty());
    }

    #[test]
    fn a_person_pinned_against_a_wall_stops_the_rider() {
        let wall = Aabb::new(Vec2::new(-10.0, 0.0), Vec2::new(10.0, 4.0));
        let mut world = World::default();
        world.obstacles.push(wall);
        let graph = StreetGraph {
            nodes: vec![Vec2::new(0.0, -0.3)],
            links: vec![vec![]],
        };
        let mut folk = Townsfolk::from_graph(graph, 1, 1);
        folk.people[0].position = Vec2::new(0.0, -0.3);
        let north = Heading::from_radians(std::f32::consts::FRAC_PI_2);
        let mut rider = crate::Rider::new(Vec2::new(0.0, -6.0), north, &world);
        let params = crate::RiderParams::default();
        let walk = crate::RiderInput {
            throttle: 1.0,
            ..Default::default()
        };
        for _ in 0..(10.0 / MAX_STEP) as usize {
            rider.step(walk, &params, &world, MAX_STEP);
            folk.make_way(&world, rider.body());
            rider.keep_clear_of(&world, &folk.bodies());
            assert_clear_of(&folk, rider.body(), "pinned");
        }
        let person = folk.people[0].position;
        assert!(
            person.y <= -0.3 + 1e-3,
            "stays against the wall: {person:?}"
        );
        assert!(rider.position.y < person.y, "the rider stops short");
        assert!(rider.speed.abs() < 1e-3, "and stops pushing");
    }

    #[test]
    fn walkers_keep_inside_the_lane_the_palenques_leave_free() {
        // 2.5 m sidewalks leave an 8 m street a 3 m carriageway: its walkers'
        // reach stays within 1 m of the middle, where rails and horses begin.
        const { assert!(MAX_LANE + Townsperson::COLLIDER_RADIUS <= 1.0 + 1e-4) };
        const { assert!(MAX_LANE + Townsperson::COLLIDER_RADIUS <= crate::town::Palenque::LANE + 1e-4) };
    }
}
