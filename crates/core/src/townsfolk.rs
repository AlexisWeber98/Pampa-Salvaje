//! Vecinos: peatones simples que deambulan por las calles entre intersecciones.
//!
//! Deterministas para una semilla dada. Cada persona camina desde una
//! intersección de calles hasta una vecina, se detiene ahí un momento y después
//! elige la siguiente al azar (evitando dar media vuelta cuando puede). Las
//! personas chocan con los mismos obstáculos estáticos que el jinete.
//!
//! En un pueblo con veredas ([`Sidewalks`]) la gente camina por ellas, de
//! esquina a esquina de cada cuadra, y cruza la calle solo en las esquinas: la
//! calzada queda para los caballos. Sin veredas camina cerca de la línea
//! central de la calle.

use crate::rng::Rng;
use crate::{Aabb, AnimationState, Body, Circle, Heading, TownLayout, Vec2, World};

/// Intersecciones de calles y los tramos de calle que las unen.
#[derive(Debug, Clone, PartialEq)]
pub struct StreetGraph {
    /// Centros de las intersecciones.
    pub nodes: Vec<Vec2>,
    /// Para cada nodo, los índices de los nodos que están a una cuadra por una
    /// calle.
    pub links: Vec<Vec<usize>>,
    /// Las veredas por las que camina la gente, si el pueblo las tiene.
    pub sidewalks: Option<Sidewalks>,
}

/// Las veredas de un pueblo, como las recorre la gente.
#[derive(Debug, Clone, PartialEq)]
pub struct Sidewalks {
    /// Distancia de la línea por la que camina la gente en una vereda a la
    /// línea central de su calle, en metros.
    pub offset: f32,
    /// Las franjas de vereda ([`TownLayout::sidewalks`]).
    pub areas: Vec<Aabb>,
}

impl Sidewalks {
    /// Si `p` está sobre alguna vereda.
    pub fn contains(&self, p: Vec2) -> bool {
        self.areas.iter().any(|a| a.contains_point(p))
    }
}

impl StreetGraph {
    /// Las intersecciones de las calles norte-sur y este-oeste de `town`, fila
    /// por fila desde el sudoeste, cada una unida a sus vecinas a una cuadra de
    /// distancia. Una plaza fusionada no tiene ninguna calle que la atraviese:
    /// las intersecciones que quedan adentro se omiten, y ningún enlace la
    /// cruza.
    pub fn from_town(town: &TownLayout) -> Self {
        // Líneas centrales de las calles de una dirección, en orden; una calle
        // cortada por una plaza cuenta una sola vez.
        let lines = |key: fn(&Aabb) -> f32, keep: fn(&Aabb) -> bool| {
            let mut lines: Vec<f32> = town.streets.iter().filter(|s| keep(s)).map(key).collect();
            lines.sort_by(f32::total_cmp);
            lines.dedup();
            lines
        };
        let xs = lines(|s| s.center().x, |s| s.size().y > s.size().x);
        let ys = lines(|s| s.center().y, |s| s.size().x > s.size().y);
        let on_street = |p: Vec2| town.streets.iter().any(|s| s.contains_point(p));

        // Índice de nodo de cada intersección de una calle, fila por fila.
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
        // Todas las calles de un pueblo tienen el mismo ancho.
        let width = town
            .streets
            .iter()
            .map(|s| s.size().x.min(s.size().y))
            .min_by(f32::total_cmp);
        let sidewalks = match width {
            Some(width) if !town.sidewalks().is_empty() => Some(Sidewalks {
                offset: width / 2.0 - FROM_FACADE,
                areas: town.sidewalks().iter().map(|w| w.area).collect(),
            }),
            _ => None,
        };
        Self {
            nodes,
            links,
            sidewalks,
        }
    }
}

/// Un peatón.
#[derive(Debug, Clone, PartialEq)]
pub struct Townsperson {
    pub position: Vec2,
    pub heading: Heading,
    /// Velocidad actual sobre el suelo, en m/s.
    pub speed: f32,
    pub collider: Circle,
    /// Variación de apariencia; el render decide cómo se ve cada valor.
    pub look: u8,
    walk_speed: f32,
    /// Desplazamiento fijo respecto de la línea central de las calles (o del
    /// medio de la vereda), para que la gente no camine en fila.
    lane: Vec2,
    from: usize,
    to: usize,
    /// De qué lado de la cuadra camina por la vereda: `1.0` a la izquierda de
    /// `from`→`to`, `-1.0` a la derecha.
    side: f32,
    /// Si va hacia la esquina donde empieza su cuadra (cruzando la calle, o
    /// doblando la esquina) en lugar de hacia la del final.
    to_corner: bool,
    /// Segundos que le quedan parado en el punto de paso actual.
    pause: f32,
    /// Segundos que lleva casi sin avanzar hacia el objetivo.
    stuck: f32,
    rng: Rng,
}

impl Townsperson {
    /// Radio de colisión de una persona vista desde arriba, en metros.
    pub const COLLIDER_RADIUS: f32 = 0.3;
    /// Ritmo de caminata más rápido, en m/s: un paseo tranquilo por la calle va
    /// de 1,25 a 1,5 (la caminata de la animación, erguida para 1919, avanza
    /// naturalmente a alrededor de 1,27 m/s).
    pub const WALK_MAX: f32 = 1.5;

    /// Lo que está haciendo la persona, sin importar cómo se dibuja.
    pub fn animation(&self) -> AnimationState {
        AnimationState::from_speed(self.speed, Self::WALK_MAX)
    }

    /// Dónde está parada esta persona, como obstáculo para los demás.
    pub fn body(&self) -> Body {
        Body {
            position: self.position,
            collider: self.collider,
        }
    }

    /// Índice de la última intersección a la que llegó (o de la que se alejó).
    pub fn last_waypoint(&self) -> usize {
        self.from
    }

    /// Índice de la intersección hacia la que camina.
    pub fn next_waypoint(&self) -> usize {
        self.to
    }
}

/// Todos los que caminan por el pueblo, más el grafo de calles que siguen.
#[derive(Debug, Clone, PartialEq)]
pub struct Townsfolk {
    pub graph: StreetGraph,
    pub people: Vec<Townsperson>,
}

/// Cuántas personas caminan por un pueblo chico, como el inspirado en Castex.
/// Los pueblos más grandes tienen más ([`TownLayout::townsfolk_count`]).
pub const TOWNSFOLK_COUNT: usize = 16;

impl Townsfolk {
    /// `count` personas repartidas por las calles de `town`, ubicadas a partir
    /// de `seed`.
    pub fn new(town: &TownLayout, count: usize, seed: u32) -> Self {
        Self::from_graph(StreetGraph::from_town(town), count, seed)
    }

    /// `count` personas repartidas por `graph`, ubicadas a partir de `seed`.
    ///
    /// Los grafos degenerados nunca provocan un panic: sin intersecciones no
    /// aparece nadie, y las personas ubicadas en una intersección de la que no
    /// sale ninguna calle se quedan ahí paradas.
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

    /// Hace avanzar a todos `dt` segundos dentro de `world`. Un `dt` no finito
    /// o no positivo no cambia nada.
    pub fn step(&mut self, world: &World, dt: f32) {
        self.step_around(world, &[], dt);
    }

    /// Como [`Self::step`], con `obstacles` (el jinete) que la gente rodea en
    /// lugar de atravesar.
    pub fn step_around(&mut self, world: &World, obstacles: &[Body], dt: f32) {
        if !(dt.is_finite() && dt > 0.0) {
            return;
        }
        for person in &mut self.people {
            person.step(&self.graph, world, obstacles, dt);
        }
    }

    /// Aparta del camino de `body` (un jinete que se abre paso) a todos los que
    /// se superponen con él, sin empujar a nadie contra los obstáculos del
    /// mundo. Devuelve los índices de las personas que tocó, en orden (los
    /// contactos que [`crate::BumpTracker`] convierte en quejas).
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

    /// Dónde está parado cada uno, como obstáculo para los demás.
    pub fn bodies(&self) -> Vec<Body> {
        self.people.iter().map(Townsperson::body).collect()
    }
}

/// Desplazamiento máximo respecto de la línea central de una calle, en metros:
/// mantiene a la gente lejos del palenque y de cualquier otra cosa que haya en
/// el borde de la calle.
const MAX_LANE: f32 = 0.7;
/// A qué distancia de la fachada camina la gente por la vereda, en metros: en
/// la franja libre entre los faroles de esquina ([`crate::StreetLamps::CURB`])
/// y la fila de árboles de frente y faroles
/// ([`crate::town::ShadeTree::FRONT_OUT`]).
pub const FROM_FACADE: f32 = 1.35;
/// Desplazamiento máximo respecto de [`FROM_FACADE`], en metros.
const MAX_WALK_LANE: f32 = 0.15;
/// Aire que la gente deja entre ella y un tronco o un poste, en metros.
const CLEAR: f32 = 0.05;
// Con su radio, la gente pasa lejos de los troncos de 0,7 m de la fila de
// árboles...
const _: () = assert!(
    FROM_FACADE + MAX_WALK_LANE + Townsperson::COLLIDER_RADIUS
        <= crate::town::ShadeTree::FRONT_OUT - 0.35 - CLEAR
);
// ...y de los postes de los faroles de esquina, también al doblar en ellas.
const _: () = assert!(
    FROM_FACADE - MAX_WALK_LANE - Townsperson::COLLIDER_RADIUS
        >= crate::StreetLamps::CURB + crate::StreetLamp::POST / 2.0 + CLEAR
);
/// Rango de la pausa en cada intersección, en segundos.
const PAUSE: (f32, f32) = (0.5, 4.0);
/// Rango de ritmos de caminata, en m/s.
const PACE: (f32, f32) = (1.25, Townsperson::WALK_MAX);
/// Segundos casi sin avanzar antes de que una persona se rinda y dé media
/// vuelta.
const GIVE_UP: f32 = 2.0;

impl Townsperson {
    /// Una persona en algún punto de una calle de `graph` (que tiene nodos). En
    /// una intersección aislada la persona empieza ahí mismo y se queda ahí.
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
        let mut side = if rng.unit() < 0.5 { 1.0 } else { -1.0 };
        let along = rng.range(0.1, 0.9);
        let mut position = a + (b - a) * along + lane;
        if let Some(walks) = &graph.sidewalks
            && from != to
        {
            let lane = Vec2::new(rng.range(-MAX_WALK_LANE, MAX_WALK_LANE), 0.0);
            let at = |side: f32| {
                let (start, end) = corners(graph, walks, from, to, side, lane.x);
                start + (end - start) * along
            };
            // La cuadra de un lado del borde del pueblo no tiene vereda.
            if !walks.contains(at(side)) {
                side = -side;
            }
            position = at(side);
            return Self::new(position, (a, b), lane, (from, to, side), rng);
        }
        Self::new(position, (a, b), lane, (from, to, side), rng)
    }

    fn new(
        position: Vec2,
        (a, b): (Vec2, Vec2),
        lane: Vec2,
        (from, to, side): (usize, usize, f32),
        mut rng: Rng,
    ) -> Self {
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
            side,
            to_corner: false,
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
        let target = self.target(graph);
        let offset = target - self.position;
        let distance = offset.length();
        let stride = self.walk_speed * dt;
        let start = self.position;
        if distance <= stride {
            self.position = target;
            if self.to_corner {
                // En la esquina: mira ya hacia el final de la cuadra.
                self.to_corner = false;
                let ahead = self.target(graph) - self.position;
                if ahead.length() > 1e-4 {
                    self.heading = Heading::from_radians(ahead.y.atan2(ahead.x));
                }
            } else {
                self.arrive(graph);
            }
        } else {
            let direction = offset * (1.0 / distance);
            self.heading = Heading::from_radians(direction.y.atan2(direction.x));
            self.position += direction * stride;
            self.speed = self.walk_speed;
        }
        self.position = world.resolve_among(self.collider, self.position, obstacles, |_| ());

        // Algo en el camino (una pared, el caballo atado): dar media vuelta.
        if self.speed > 0.0 && self.position.distance(start) < 0.25 * stride {
            self.stuck += dt;
            if self.stuck > GIVE_UP {
                std::mem::swap(&mut self.from, &mut self.to);
                // La misma vereda, vista en el otro sentido.
                self.side = -self.side;
                self.to_corner = false;
                self.stuck = 0.0;
                self.heading = self.heading.rotated(std::f32::consts::PI);
            }
        } else {
            self.stuck = 0.0;
        }
    }

    /// Hacia dónde camina: la intersección `to` (sin veredas), o la esquina
    /// de su cuadra por la vereda.
    fn target(&self, graph: &StreetGraph) -> Vec2 {
        match &graph.sidewalks {
            Some(walks) if self.from != self.to => {
                let (start, end) =
                    corners(graph, walks, self.from, self.to, self.side, self.lane.x);
                if self.to_corner { start } else { end }
            }
            _ => graph.nodes[self.to] + self.lane,
        }
    }

    /// Llegó a `to`: elige la siguiente intersección y se queda quieta un
    /// momento. Por la vereda, sigue desde la esquina de la cuadra siguiente
    /// más cercana, sin cruzar nunca en diagonal: dobla la esquina o cruza una
    /// sola calle.
    fn arrive(&mut self, graph: &StreetGraph) {
        let came_from = self.from;
        self.from = self.to;
        let links = &graph.links[self.from];
        let onward: Vec<usize> = links.iter().copied().filter(|&n| n != came_from).collect();
        let choices = if onward.is_empty() { links } else { &onward };
        // No hay adónde ir desde una intersección aislada: se queda donde está.
        self.to = if choices.is_empty() {
            self.from
        } else {
            choices[self.rng.below(choices.len())]
        };
        if let Some(walks) = &graph.sidewalks
            && self.from != self.to
        {
            let start = |side| corners(graph, walks, self.from, self.to, side, self.lane.x).0;
            let rank = |side: f32| {
                let at = start(side);
                (!walks.contains(at), at.distance(self.position))
            };
            let (left, right) = (rank(1.0), rank(-1.0));
            // Primero una esquina sobre la vereda, después la más cercana.
            self.side = if right < left { -1.0 } else { 1.0 };
            self.to_corner = true;
        }
        self.pause = self.rng.range(PAUSE.0, PAUSE.1);
        self.speed = 0.0;
    }
}

/// Las esquinas de la cuadra `from`→`to` sobre la vereda del lado `side`: la
/// del principio y la del final, a `lane` metros del medio de la vereda.
fn corners(
    graph: &StreetGraph,
    walks: &Sidewalks,
    from: usize,
    to: usize,
    side: f32,
    lane: f32,
) -> (Vec2, Vec2) {
    let (a, b) = (graph.nodes[from], graph.nodes[to]);
    let along = b - a;
    let dir = along * (1.0 / along.length().max(1e-6));
    let left = Vec2::new(-dir.y, dir.x);
    let d = walks.offset + lane;
    let out = left * (side * d);
    (a + dir * d + out, b - dir * d + out)
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
        // Un peatón se aleja como mucho esta distancia de la línea central.
        let reach = MAX_LANE + Townsperson::COLLIDER_RADIUS;
        for (c, town) in sample_configs().iter().zip(sample_towns()) {
            let world = World::with_town(Terrain::default(), &town);
            let graph = StreetGraph::from_town(&town);
            let (cols, rows) = c.blocks;
            // Las intersecciones dentro de una plaza fusionada desaparecen.
            let (w, h) = town.size.plaza_span(c.blocks);
            let inside = (w - 1) * (h - 1);
            // Una línea que atraviesa el pueblo agrega una fila de cruces junto
            // a su franja.
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
                    // Cada punto del enlace está sobre una calle y lejos de los
                    // obstáculos más que el alcance de un peatón.
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
        // De 1,25 a 1,5 m/s, cada uno el suyo: una calle de 1919, nadie
        // apurado.
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
    fn people_walk_the_sidewalks_and_cross_only_at_the_corners() {
        for town in sample_towns() {
            let mut world = World::with_town(Terrain::default(), &town);
            // Con los faroles que pone el juego, los de esquina incluidos.
            let lamps = crate::StreetLamps::place(
                std::slice::from_ref(&town),
                &crate::RoadNetwork::default(),
                &crate::SettlementLayout::default(),
                &world.obstacles,
            );
            world.obstacles.extend(lamps.obstacles());
            let mut folk = Townsfolk::new(&town, 30, 3);
            let walks = folk.graph.sidewalks.clone().expect("a paved town");
            let half = town.streets[0].size().x.min(town.streets[0].size().y) / 2.0;
            let (mut on, mut all) = (0, 0);
            run(&mut folk, &world, 120.0, |folk| {
                for p in &folk.people {
                    all += 1;
                    if walks.contains(p.position) {
                        on += 1;
                        // Por la franja libre: no roza los árboles ni los
                        // faroles de la vereda.
                        let around = Aabb::new(
                            p.position - Vec2::new(1.0, 1.0),
                            p.position + Vec2::new(1.0, 1.0),
                        );
                        for o in world.obstacles.near(around) {
                            let gap = p.position.distance(o.closest_point(p.position));
                            assert!(
                                gap > p.collider.radius + 0.05,
                                "{}: {p:?} brushes {o:?}",
                                town.name
                            );
                        }
                        continue;
                    }
                    // Fuera de la vereda solo para cruzar, en una esquina, o en
                    // una cuadra sin vereda de ningún lado (la que cruza la
                    // franja de la vía).
                    let corner = folk.graph.nodes.iter().any(|n| {
                        (p.position.x - n.x).abs() <= half && (p.position.y - n.y).abs() <= half
                    });
                    let bare = [1.0, -1.0].into_iter().all(|side| {
                        let (a, b) = corners(&folk.graph, &walks, p.from, p.to, side, 0.0);
                        !walks.contains((a + b) * 0.5)
                    });
                    assert!(
                        corner || bare,
                        "{}: {p:?} in the roadway mid-block",
                        town.name
                    );
                    // Nadie se traba contra los árboles ni los faroles.
                    assert!(p.stuck < 1.0, "{}: {p:?} stuck", town.name);
                }
            });
            let share = on as f32 / all as f32;
            assert!(share > 0.85, "{}: only {share} on the sidewalks", town.name);
        }
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
                let to = p.target(&folk.graph) - p.position;
                let distance = to.length();
                if p.animation() == AnimationState::Walk && distance > 0.5 {
                    // Mirando hacia la siguiente esquina (continuo, sin
                    // ajustar a pasos fijos).
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
        // Se conserva una calle norte-sur y una este-oeste: un único cruce.
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
            sidewalks: None,
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

    /// Ninguna persona se superpone con `body` (más allá del redondeo).
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
        // Un jinete quieto en el camino de cada persona, de a una por vez.
        for i in 0..4 {
            let p = &folk.people[i];
            let ahead = p.target(&folk.graph) - p.position;
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
            sprint: false,
        };
        // Empieza justo encima de la primera persona.
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
            sidewalks: None,
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
        // Ya no queda nadie en el camino: no hay contactos.
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
            sidewalks: None,
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
        // Veredas de 2,5 m dejan a una calle de 8 m una calzada de 3 m: el
        // alcance de sus peatones queda a menos de 1 m del medio, donde
        // empiezan los palenques y los caballos.
        const { assert!(MAX_LANE + Townsperson::COLLIDER_RADIUS <= 1.0 + 1e-4) };
        const { assert!(MAX_LANE + Townsperson::COLLIDER_RADIUS <= crate::town::Palenque::LANE + 1e-4) };
    }
}
