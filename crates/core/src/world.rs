//! El mundo estático por el que se mueve el jinete: terreno, pisos elevados y
//! límites jugables.

use std::sync::Arc;

use crate::obstacles::around;
use crate::{
    Body, ChunkCoord, Circle, FenceLayout, Floor, FloraSite, RailLine, RoadNetwork,
    SettlementLayout, SideObstacle, StaticObstacles, Terrain, TownLayout, Vec2, WorldConfig,
};

/// Área jugable alineada con los ejes en el plano del suelo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldBounds {
    pub min: Vec2,
    pub max: Vec2,
}

/// Resultado de mantener un círculo dentro de [`WorldBounds`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Contained {
    pub position: Vec2,
    /// Dirección unitaria en la que se empujó de vuelta al círculo sobre `x`,
    /// si se lo empujó.
    pub push_x: Option<Vec2>,
    /// Dirección unitaria en la que se empujó de vuelta al círculo sobre `y`,
    /// si se lo empujó.
    pub push_y: Option<Vec2>,
}

impl WorldBounds {
    /// Un cuadrado de lado `2 * half_extent` centrado en el origen.
    pub const fn centered(half_extent: f32) -> Self {
        Self {
            min: Vec2::new(-half_extent, -half_extent),
            max: Vec2::new(half_extent, half_extent),
        }
    }

    /// Longitudes de los lados sobre `x` y sobre `y`.
    pub fn size(&self) -> Vec2 {
        self.max - self.min
    }

    /// Ajusta un círculo de radio `radius` ubicado en `center` para que quede
    /// completamente adentro.
    pub fn contain(&self, center: Vec2, radius: f32) -> Contained {
        let (x, push_x) = clamp_axis(center.x, self.min.x + radius, self.max.x - radius);
        let (y, push_y) = clamp_axis(center.y, self.min.y + radius, self.max.y - radius);
        Contained {
            position: Vec2::new(x, y),
            push_x: push_x.map(|sign| Vec2::new(sign, 0.0)),
            push_y: push_y.map(|sign| Vec2::new(0.0, sign)),
        }
    }
}

/// Ajusta `value` a `[lo, hi]`, informando el signo del empuje, si lo hubo.
fn clamp_axis(value: f32, lo: f32, hi: f32) -> (f32, Option<f32>) {
    if value < lo {
        (lo, Some(1.0))
    } else if value > hi {
        (hi, Some(-1.0))
    } else {
        (value, None)
    }
}

/// Todo lo estático con lo que choca el jinete o sobre lo que se apoya.
#[derive(Debug, Clone, PartialEq)]
pub struct World {
    pub config: WorldConfig,
    pub terrain: Terrain,
    pub bounds: WorldBounds,
    /// Huellas estáticas que el jinete no puede atravesar, indexadas para que
    /// la colisión solo mire las cercanas.
    pub obstacles: StaticObstacles,
    /// Huellas en movimiento (el tren), que el juego actualiza en cada frame.
    pub moving: Vec<SideObstacle>,
    /// Pisos elevados por los que camina la gente (andenes de las estaciones),
    /// parte de [`Self::ground_height`].
    pub floors: Vec<Floor>,
}

/// Algo que sabe a qué altura está el suelo donde se apoyan los cuerpos.
pub trait Ground {
    /// Altura de la superficie transitable en `p`, en metros.
    fn ground_height(&self, p: Vec2) -> f32;
}

/// Terreno desnudo: sin pisos encima.
impl Ground for Terrain {
    fn ground_height(&self, p: Vec2) -> f32 {
        self.height(p.x, p.y)
    }
}

impl Ground for World {
    fn ground_height(&self, p: Vec2) -> f32 {
        World::ground_height(self, p)
    }
}

impl World {
    /// `terrain` abierto dentro del mundo que describe `config`, sin nada
    /// construido encima.
    pub fn new(config: WorldConfig, terrain: Terrain) -> Self {
        Self {
            config,
            terrain,
            bounds: config.bounds(),
            obstacles: StaticObstacles::default(),
            moving: Vec::new(),
            floors: Vec::new(),
        }
    }

    /// Altura de la superficie sobre la que se apoyan los cuerpos en `p`: el
    /// terreno, elevado por el piso más alto que haya ahí (un andén o su
    /// rampa), o por el acceso y la plataforma de un paso a nivel
    /// ([`RoadNetwork::crossing_rise`]). Todo lo que se coloca sobre el suelo
    /// (jinetes, personas, caballos, el piso de la cámara) usa esto.
    pub fn ground_height(&self, p: Vec2) -> f32 {
        let crossing = self
            .terrain
            .roads
            .as_ref()
            .map_or(0.0, |roads| roads.crossing_rise(p));
        let rise = self
            .floors
            .iter()
            .filter_map(|f| f.rise_at(p))
            .fold(crossing, f32::max);
        self.terrain.height(p.x, p.y) + rise
    }

    /// Terreno abierto en el mundo por defecto, sin nada construido encima.
    pub fn open(terrain: Terrain) -> Self {
        Self::new(WorldConfig::default(), terrain)
    }

    /// `terrain` nivelado debajo de `town`, con las huellas del pueblo como
    /// obstáculos, en el mundo por defecto.
    pub fn with_town(terrain: Terrain, town: &TownLayout) -> Self {
        Self::with_towns(WorldConfig::default(), terrain, std::slice::from_ref(town))
    }

    /// El mundo que describe `config`, con `terrain` nivelado debajo de cada
    /// pueblo, las huellas de cada pueblo como obstáculos y sus andenes como
    /// pisos. Los pueblos deberían venir de [`crate::place_towns`], para que
    /// queden separados y adentro.
    pub fn with_towns(config: WorldConfig, terrain: Terrain, towns: &[TownLayout]) -> Self {
        let mut terrain = terrain;
        for town in towns {
            let area = town.flat_area(&terrain);
            terrain = terrain.with_flat_area(area);
        }
        let mut world = Self::new(config, terrain);
        for town in towns {
            world.obstacles.extend(town.obstacles());
            world.floors.extend(town.floors());
        }
        world
    }

    /// Agrega los andenes de las vías muertas como pisos
    /// ([`RailLine::floors`]).
    pub fn lay_rail(&mut self, line: &RailLine) {
        self.floors.extend(line.floors());
    }

    /// Corta `roads` en el terreno y agrega sus carteles indicadores y las
    /// señales de los pasos a nivel como obstáculos. Hay que trazar los caminos
    /// antes de hacer crecer la flora, que se mantiene fuera de ellos.
    pub fn lay_roads(&mut self, roads: RoadNetwork) {
        self.obstacles.extend(roads.obstacles());
        let terrain = std::mem::take(&mut self.terrain);
        self.terrain = terrain.with_roads(Arc::new(roads));
    }

    /// Agrega cada tramo y cada tranquera cerrada de `fences` como obstáculos
    /// estáticos delgados de [`FenceLayout::HEIGHT`] de alto, que un caballo
    /// supera al saltar; las tranqueras abiertas y los tramos rotos son huecos.
    /// Devuelve cuántos rectángulos se agregaron.
    pub fn fence(&mut self, fences: &FenceLayout) -> usize {
        let spans = fences.obstacles();
        let count = spans.len();
        for span in spans {
            self.obstacles.push_low(span, FenceLayout::HEIGHT);
        }
        count
    }

    /// Agrega cada casa, galpón, tanque y elemento decorativo de `settlements`
    /// como obstáculo estático, cada corral como sus tramos de baranda (abierto
    /// en su tranquera) y cada lugar de árbol como su tronco, cada uno tan alto
    /// como su estructura ([`SettlementLayout::obstacle_heights`]). Las huellas
    /// no son obstáculos. Devuelve cuántos rectángulos se agregaron.
    pub fn settle(&mut self, settlements: &SettlementLayout) -> usize {
        let before = self.obstacles.len();
        for (obstacle, height) in settlements.obstacle_heights() {
            self.obstacles.push_low(obstacle, height);
        }
        self.obstacles.len() - before
    }

    /// Agrega cada tronco de árbol que `flora` hace crecer en cualquier parte
    /// del mundo como obstáculo estático, para que la colisión funcione en los
    /// chunks que no se dibujan. Devuelve cuántos árboles se plantaron. Los
    /// arbustos y el pasto no son obstáculos: un jinete pasa a través de ellos.
    pub fn plant(&mut self, flora: &FloraSite) -> usize {
        let trees = flora.all_trees();
        self.obstacles.extend(trees.iter().map(|t| t.obstacle()));
        trees.len()
    }

    /// Carga por streaming los troncos de los árboles del chunk `coord` como
    /// obstáculos, reemplazando los que tuviera, para que el chunk tenga
    /// colisión sin plantar el mapa entero. Siempre los mismos árboles.
    /// Devuelve cuántos se plantaron. Ver [`Self::unplant_chunk`].
    pub fn plant_chunk(&mut self, flora: &FloraSite, coord: ChunkCoord) -> usize {
        let trees = flora.trees(coord);
        let count = trees.len();
        self.obstacles
            .set_chunk(coord, trees.iter().map(|t| t.obstacle()).collect());
        count
    }

    /// Descarta lo que [`Self::plant_chunk`] plantó en `coord`. Devuelve si el
    /// chunk estaba plantado.
    pub fn unplant_chunk(&mut self, coord: ChunkCoord) -> bool {
        self.obstacles.clear_chunk(coord)
    }
}

/// Superposición (en metros) por debajo de la cual un círculo se considera
/// libre de un obstáculo.
const CLEAR: f32 = 1e-3;
/// Pasadas de empuje antes de que un círculo se considere atrapado.
const PASSES: usize = 8;
/// Separación entre muestras de la búsqueda de lugar libre, en metros.
const SEARCH_STEP: f32 = 0.25;
/// Lo más lejos que busca la búsqueda de lugar libre, en metros: más allá de
/// cualquiera de los dos extremos del tren más largo.
const SEARCH_RANGE: f32 = 60.0;
/// Alcance extra de la búsqueda de obstáculos estáticos alrededor de un
/// círculo, en metros.
const LOOKUP_SLACK: f32 = 0.5;

impl World {
    /// Saca un círculo ubicado en `center` de todos los obstáculos con los que
    /// se superpone, estáticos o en movimiento, y devuelve dónde termina.
    /// `on_push` recibe la dirección unitaria de cada empuje (para anular la
    /// velocidad contra las paredes).
    ///
    /// Unas pocas pasadas resuelven las esquinas donde se juntan dos huellas.
    /// Cuando los empujes se contrarrestan entre sí (un vagón de tren que
    /// empuja el círculo contra una pared), el círculo queda atrapado: entonces
    /// se mueve al lugar más cercano libre de todo, buscando a lo ancho y a lo
    /// largo de los obstáculos en movimiento y sobre los ejes. Si no existe un
    /// lugar así cerca, ganan los obstáculos estáticos y el círculo solo se
    /// saca de esos.
    pub fn resolve(&self, collider: Circle, center: Vec2, on_push: impl FnMut(Vec2)) -> Vec2 {
        self.resolve_among(collider, center, &[], on_push)
    }

    /// Como [`Self::resolve`] para un cuerpo que está a `clearance` metros
    /// sobre el suelo (un caballo que salta): los obstáculos bajos que no
    /// superan esa altura ([`StaticObstacles::near_above`]) lo dejan pasar.
    pub fn resolve_above(
        &self,
        collider: Circle,
        center: Vec2,
        clearance: f32,
        on_push: impl FnMut(Vec2),
    ) -> Vec2 {
        self.resolve_among_above(collider, center, &[], clearance, on_push)
    }

    /// Como [`Self::resolve`], pero además aleja el círculo de los `bodies` de
    /// otros personajes. Los cuerpos ceden ante el mundo: cuando un cuerpo y
    /// una pared atrapan al círculo, gana la pared.
    pub fn resolve_among(
        &self,
        collider: Circle,
        center: Vec2,
        bodies: &[Body],
        on_push: impl FnMut(Vec2),
    ) -> Vec2 {
        self.resolve_among_above(
            collider,
            center,
            bodies,
            StaticObstacles::ON_GROUND,
            on_push,
        )
    }

    /// [`Self::resolve_among`] para un cuerpo que está a `clearance` metros
    /// sobre el suelo, como [`Self::resolve_above`].
    pub fn resolve_among_above(
        &self,
        collider: Circle,
        center: Vec2,
        bodies: &[Body],
        clearance: f32,
        mut on_push: impl FnMut(Vec2),
    ) -> Vec2 {
        let start = center;
        let resolved = self.push_passes(collider, center, bodies, clearance, true, &mut on_push);
        if self.overlap(collider, resolved, clearance) < CLEAR {
            return resolved;
        }
        let spot = match self.nearest_clear(collider, start, clearance) {
            Some(spot) => spot,
            None => self.push_passes(collider, resolved, &[], clearance, false, &mut |_| ()),
        };
        let push = spot - resolved;
        let length = push.length();
        if length > 0.0 {
            on_push(push * (1.0 / length));
        }
        spot
    }

    /// Hasta [`PASSES`] rondas en las que se saca el círculo de cada cuerpo y
    /// después de cada obstáculo por turno (los que se mueven, solo cuando
    /// `with_moving`), de modo que el mundo tiene la última palabra en cada
    /// ronda. Los obstáculos bajos que no superan `clearance` se pasan por
    /// encima.
    fn push_passes(
        &self,
        collider: Circle,
        mut center: Vec2,
        bodies: &[Body],
        clearance: f32,
        with_moving: bool,
        on_push: &mut impl FnMut(Vec2),
    ) -> Vec2 {
        for _ in 0..PASSES {
            let mut moved = false;
            let mut apply = |push: Option<Vec2>, center: &mut Vec2| {
                let Some(push) = push else { return };
                *center += push;
                let length = push.length();
                if length > 0.0 {
                    on_push(push * (1.0 / length));
                }
                moved = true;
            };
            for body in bodies {
                apply(collider.push_out_of_body(center, body), &mut center);
            }
            let nearby = self
                .obstacles
                .near_above(around(center, collider.radius + LOOKUP_SLACK), clearance);
            for obstacle in nearby {
                apply(collider.push_out(center, obstacle), &mut center);
            }
            // El agua profunda devuelve los cuerpos a las partes bajas.
            for lagoon in &self.terrain.lagoons {
                apply(lagoon.push_out(center), &mut center);
            }
            if with_moving {
                for obstacle in &self.moving {
                    apply(obstacle.push_out(collider, center), &mut center);
                }
            }
            if !moved {
                break;
            }
        }
        center
    }

    /// La mayor superposición de un círculo ubicado en `center` con cualquier
    /// obstáculo más alto que `clearance`, en metros.
    fn overlap(&self, collider: Circle, center: Vec2, clearance: f32) -> f32 {
        let footprints = self
            .obstacles
            .near_above(around(center, collider.radius + LOOKUP_SLACK), clearance)
            .into_iter()
            .chain(self.moving.iter().map(|m| &m.footprint));
        footprints
            .filter_map(|o| collider.push_out(center, o))
            .chain(
                self.terrain
                    .lagoons
                    .iter()
                    .filter_map(|l| l.push_out(center)),
            )
            .map(Vec2::length)
            .fold(0.0, f32::max)
    }

    /// El lugar más cercano a `from` donde el círculo no se superpone con nada,
    /// en las direcciones transversal y longitudinal de cada obstáculo en
    /// movimiento que toca y en los ejes del mundo; `None` si no hay ninguno
    /// dentro de [`SEARCH_RANGE`].
    fn nearest_clear(&self, collider: Circle, from: Vec2, clearance: f32) -> Option<Vec2> {
        let axes = [Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0)];
        let touching = self
            .moving
            .iter()
            .filter(|m| collider.push_out(from, &m.footprint).is_some())
            .flat_map(|m| [m.across, Vec2::new(-m.across.y, m.across.x)]);
        let directions: Vec<Vec2> = touching.chain(axes).flat_map(|d| [d, d * -1.0]).collect();
        // Estrictamente libre: el resultado no debe contar como que toca algo.
        let clear = |p: Vec2| self.overlap(collider, p, clearance) == 0.0;
        let mut best: Option<(f32, Vec2)> = None;
        for direction in directions {
            let limit = best.map_or(SEARCH_RANGE, |(t, _)| t);
            let mut blocked = 0.0;
            let mut t = SEARCH_STEP;
            while t <= limit {
                if clear(from + direction * t) {
                    // Ajustar hasta el borde entre la última muestra bloqueada
                    // y esta libre.
                    let mut free = t;
                    for _ in 0..12 {
                        let mid = (blocked + free) / 2.0;
                        if clear(from + direction * mid) {
                            free = mid;
                        } else {
                            blocked = mid;
                        }
                    }
                    if best.is_none_or(|(b, _)| free < b) {
                        best = Some((free, from + direction * free));
                    }
                    break;
                }
                blocked = t;
                t += SEARCH_STEP;
            }
        }
        best.map(|(_, spot)| spot)
    }

    /// El lugar más cercano a `near`, a como mucho `range` metros, donde puede
    /// pararse un cuerpo con `collider`: libre de todo obstáculo, dentro de los
    /// límites y en suelo seco, fuera del barro húmedo de las lagunas. Se busca
    /// en anillos separados por [`SPOT_STEP`], del más cercano al más lejano;
    /// `None` si no hay ninguno.
    pub fn clear_spot(&self, collider: Circle, near: Vec2, range: f32) -> Option<Vec2> {
        let fits = |p: Vec2| {
            self.bounds.contain(p, collider.radius).position == p
                && !self.terrain.lagoons.iter().any(|l| l.is_bare(p))
                && self.overlap(collider, p, StaticObstacles::ON_GROUND) == 0.0
        };
        if fits(near) {
            return Some(near);
        }
        let rings = (range / SPOT_STEP).floor() as usize;
        (1..=rings).find_map(|k| {
            let r = k as f32 * SPOT_STEP;
            let n = ((std::f32::consts::TAU * r / SPOT_STEP).ceil() as usize).max(8);
            (0..n)
                .map(|i| {
                    let a = std::f32::consts::TAU * i as f32 / n as f32;
                    near + Vec2::new(a.cos(), a.sin()) * r
                })
                .find(|&p| fits(p))
        })
    }
}

/// Separación de los anillos en los que busca [`World::clear_spot`], en metros.
const SPOT_STEP: f32 = 1.0;

impl Default for World {
    fn default() -> Self {
        Self::open(Terrain::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Aabb, SideObstacle};

    #[test]
    fn every_town_levels_its_ground_and_blocks_its_buildings() {
        let configs = [
            crate::TownConfig::castex(),
            crate::TownConfig {
                name: "Lejos".into(),
                origin: Vec2::new(3000.0, -2000.0),
                ..crate::TownConfig::castex()
            },
        ];
        let (towns, errors) = crate::place_towns(&WorldConfig::default(), &configs);
        assert!(errors.is_empty(), "{errors:?}");
        let terrain = Terrain::default();
        let world = World::with_towns(WorldConfig::default(), terrain.clone(), &towns);
        assert_eq!(world.terrain.flat_areas.len(), 2);
        let mut all = Vec::new();
        for t in &towns {
            let area = t.flat_area(&terrain);
            assert!(world.terrain.flat_areas.contains(&area));
            let c = t.grid().center();
            assert_eq!(world.terrain.height(c.x, c.y), area.height);
            all.extend(t.obstacles());
        }
        assert_eq!(world.obstacles, all);
        // Cabalgar hacia un edificio del pueblo lejano también queda bloqueado
        // ahí.
        let wall = towns[1].buildings[0].footprint;
        let c = Circle::new(0.9);
        let end = world.resolve(c, wall.center(), |_| ());
        assert!(deepest_overlap(&world, c, end) < 1e-3, "{end:?}");
    }

    #[test]
    fn a_body_higher_than_a_low_footprint_passes_over_it() {
        let mut world = World::default();
        let fence = Aabb::from_center_size(Vec2::ZERO, Vec2::new(10.0, 0.15));
        world.obstacles.push_low(fence, 1.3);
        let c = Circle::new(0.9);
        let on_it = Vec2::new(0.0, 0.3);
        // Por encima de los alambres: se queda donde está, sin ningún empuje.
        let mut pushes = 0;
        let end = world.resolve_above(c, on_it, 1.4, |_| pushes += 1);
        assert_eq!((end, pushes), (on_it, 0));
        // Más bajo que los alambres, o en el suelo: se lo saca como siempre.
        for clearance in [1.2, 0.0] {
            let end = world.resolve_above(c, on_it, clearance, |_| ());
            assert!(end.y >= 0.9 + 0.075 - 1e-4, "{clearance}: {end:?}");
        }
        assert_eq!(
            world.resolve(c, on_it, |_| ()),
            world.resolve_above(c, on_it, 0.0, |_| ())
        );
    }

    #[test]
    fn a_clear_spot_is_the_nearest_free_ground_inside_the_world() {
        let town = crate::TownLayout::castex_inspired();
        let world = World::with_town(Terrain::default(), &town);
        let c = Circle::new(0.9);
        // El terreno abierto es su propio lugar libre.
        let open = world.bounds.min + Vec2::new(50.0, 50.0);
        assert_eq!(world.clear_spot(c, open, 10.0), Some(open));
        // Dentro de un edificio: el lugar más cercano fuera de él, sin
        // superponerse con nada.
        let wall = town.buildings[0].footprint;
        let spot = world.clear_spot(c, wall.center(), 100.0).unwrap();
        assert_eq!(deepest_overlap(&world, c, spot), 0.0, "{spot:?}");
        assert_eq!(world.resolve(c, spot, |_| ()), spot);
        // Fuera de la esquina del mundo: se lo devuelve hacia adentro.
        let corner = world.clear_spot(c, world.bounds.max, 5.0).unwrap();
        assert_eq!(world.bounds.contain(corner, 0.9).position, corner);
        // No hay nada libre al alcance.
        assert_eq!(world.clear_spot(c, wall.center(), 0.0), None);
    }

    #[test]
    fn a_clear_spot_keeps_out_of_a_lagoon_and_off_its_mud() {
        let terrain = Terrain::default();
        let lagoon = crate::Lagoon::new(&crate::lagoon::tests::arocena_config(), &terrain);
        let world = World::open(terrain.with_lagoon(lagoon.clone()));
        let spot = world
            .clear_spot(Circle::new(0.9), lagoon.center, 2000.0)
            .unwrap();
        assert!(!lagoon.is_bare(spot), "{spot:?}");
        assert!(
            lagoon.offshore(spot) < crate::Lagoon::MUD_BAND + 2.0,
            "nearest"
        );
    }

    #[test]
    fn laying_the_rail_adds_the_bay_platform_as_a_floor_to_walk_and_board_from() {
        let (towns, line) = crate::train::line_tests::branched_line();
        let mut world = World::with_towns(WorldConfig::default(), Terrain::default(), &towns);
        let platforms = world.floors.len();
        world.lay_rail(&line);
        assert_eq!(world.floors.len(), platforms + 1);
        let bay = line.bays[0].platform;
        assert!(world.floors.contains(&bay));
        // El suelo sobre él es su parte superior, a 0,9 m de altura, y está
        // nivelado al costado.
        let on = bay.footprint.center();
        let rise = |p: Vec2| world.ground_height(p) - world.terrain.height(p.x, p.y);
        assert!((rise(on) - 0.9).abs() < 1e-5, "{}", rise(on));
        assert_eq!(rise(on + Vec2::new(0.0, 20.0)), 0.0);
    }

    #[test]
    fn the_ground_on_a_platform_is_its_top_and_its_ramps_lead_down() {
        let mut checked = 0;
        for town in crate::town::tests::sample_towns() {
            let world = World::with_town(Terrain::default(), &town);
            assert_eq!(world.floors, town.floors());
            let Some(platform) = town
                .buildings
                .iter()
                .find(|b| b.kind == crate::BuildingKind::Platform)
            else {
                assert!(world.floors.is_empty(), "{}", town.name);
                continue;
            };
            let floor = world.floors[0];
            let f = platform.footprint;
            let terrain = |p: Vec2| world.terrain.height(p.x, p.y);
            // En cualquier punto del andén, el suelo es su parte superior.
            for (i, j) in [(0.5, 0.5), (0.0, 0.0), (1.0, 1.0), (0.1, 0.9), (0.95, 0.2)] {
                let p = f.min + Vec2::new(f.size().x * i, f.size().y * j);
                assert_eq!(world.ground_height(p), terrain(p) + platform.height);
                assert_eq!(Ground::ground_height(&world, p), world.ground_height(p));
            }
            // Bajando por cada rampa, de forma pareja, hasta el terreno desnudo
            // al pie.
            let mid = f.center().dot(floor.across());
            let (lo, hi) = (f.min.dot(floor.along), f.max.dot(floor.along));
            for (end, out) in [(hi, 1.0), (lo, -1.0)] {
                let at = |d: f32| floor.along * (end + out * d) + floor.across() * mid;
                let mut last = world.ground_height(at(0.0));
                for k in 1..=30 {
                    let p = at(k as f32 * Floor::RAMP_LENGTH / 30.0);
                    let h = world.ground_height(p);
                    assert!(h < last, "{}: {k}", town.name);
                    last = h;
                }
                assert!((last - terrain(at(Floor::RAMP_LENGTH))).abs() < 1e-4);
                assert_eq!(world.ground_height(at(4.0)), terrain(at(4.0)));
            }
            // Al costado de sus lados largos, el terreno desnudo: son
            // escalones.
            for side in [
                f.min.dot(floor.across()) - 0.1,
                f.max.dot(floor.across()) + 0.1,
            ] {
                let p = floor.along * f.center().dot(floor.along) + floor.across() * side;
                assert_eq!(world.ground_height(p), terrain(p), "{}", town.name);
            }
            // El terreno desnudo no sabe nada de andenes.
            let c = f.center();
            assert_eq!(Ground::ground_height(&world.terrain, c), terrain(c));
            checked += 1;
        }
        assert!(checked > 3);
    }

    #[test]
    fn a_rider_wades_the_shallows_of_a_lagoon_but_never_its_deep_water() {
        let config = crate::lagoon::tests::arocena_config();
        let lagoon = crate::Lagoon::new(&config, &Terrain::default());
        let world = World::new(
            WorldConfig::default(),
            Terrain::default().with_lagoon(lagoon.clone()),
        );
        let collider = Circle::new(crate::Rider::COLLIDER_RADIUS);
        // Cabalgando directo hacia el medio desde todos los lados: cada paso se
        // resuelve y el jinete termina en las partes bajas, nunca más hondo.
        for k in 0..16 {
            let angle = k as f32 * std::f32::consts::TAU / 16.0;
            let dir = Vec2::new(angle.cos(), angle.sin());
            let mut at = lagoon.center + dir * (lagoon.shore_radius(angle) + 20.0);
            let mut pushed = false;
            for _ in 0..200 {
                at = world.resolve(collider, at + dir * -0.5, |_| pushed = true);
                assert!(!lagoon.is_deep(at), "{at:?}");
            }
            assert!(pushed && lagoon.is_water(at), "{at:?}");
            assert!(world.ground_height(at) < lagoon.level);
        }
        // La tierra firme queda como está.
        let dry = lagoon.center + Vec2::new(lagoon.radii.x * 2.0, 0.0);
        assert_eq!(world.resolve(collider, dry, |_| ()), dry);
    }

    #[test]
    fn world_bounds_follow_the_world_config() {
        assert_eq!(World::default().bounds, WorldConfig::default().bounds());
        let config = WorldConfig {
            half_extent: 1000.0,
            chunk_size: 100.0,
        };
        let world = World::new(config, Terrain::default());
        assert_eq!(world.bounds.min, Vec2::new(-1000.0, -1000.0));
        assert_eq!(world.bounds.size(), Vec2::new(2000.0, 2000.0));
        assert_eq!(world.config, config);
    }

    #[test]
    fn inside_points_are_untouched() {
        let c = WorldBounds::centered(300.0).contain(Vec2::new(10.0, -20.0), 1.0);
        assert_eq!(c.position, Vec2::new(10.0, -20.0));
        assert_eq!((c.push_x, c.push_y), (None, None));
    }

    #[test]
    fn keeps_the_whole_circle_inside_and_reports_push_directions() {
        let c = WorldBounds::centered(300.0).contain(Vec2::new(400.0, -299.5), 1.0);
        assert_eq!(c.position, Vec2::new(299.0, -299.0));
        assert_eq!(c.push_x, Some(Vec2::new(-1.0, 0.0)));
        assert_eq!(c.push_y, Some(Vec2::new(0.0, 1.0)));
    }

    #[test]
    fn town_world_levels_ground_and_blocks_buildings() {
        for town in crate::town::tests::sample_towns() {
            let world = World::with_town(Terrain::default(), &town);
            let area = town.flat_area(&Terrain::default());
            assert_eq!(world.terrain.flat_areas, vec![area]);
            assert_eq!(world.obstacles, town.obstacles());
            assert_eq!(world.bounds, WorldConfig::default().bounds());
        }
    }

    /// La mayor superposición de un círculo ubicado en `p` con cualquier cosa
    /// de `world`, en metros.
    pub(crate) fn deepest_overlap(world: &World, collider: Circle, p: Vec2) -> f32 {
        let statics = world
            .obstacles
            .iter()
            .filter_map(|o| collider.push_out(p, o));
        let moving = world.moving.iter().filter_map(|o| o.push_out(collider, p));
        statics.chain(moving).map(Vec2::length).fold(0.0, f32::max)
    }

    /// Un vagón de 10 m a lo largo de `x` (de 3 m de ancho) con una pared 0,5 m
    /// al norte: un hueco demasiado angosto para un círculo de 0,9 m.
    fn pinch_world() -> World {
        let mut world = World::default();
        world
            .obstacles
            .push(Aabb::new(Vec2::new(-20.0, 2.0), Vec2::new(20.0, 6.0)));
        world.moving.push(SideObstacle {
            footprint: Aabb::from_center_size(Vec2::ZERO, Vec2::new(10.0, 3.0)),
            across: Vec2::new(0.0, 1.0),
        });
        world
    }

    #[test]
    fn a_circle_pinched_between_a_car_and_a_wall_ends_clear_of_both() {
        let world = pinch_world();
        let c = Circle::new(0.9);
        for k in 0..=20 {
            // Desde dentro de la mitad norte del vagón hasta tocar la pared.
            let start = Vec2::new(-4.0 + 0.4 * k as f32, 0.1 + 0.1 * k as f32);
            let mut pushes = 0;
            let end = world.resolve(c, start, |n| {
                assert!((n.length() - 1.0).abs() < 1e-4, "unit push {n:?}");
                pushes += 1;
            });
            assert!(pushes > 0, "{start:?} needed a push");
            assert!(
                deepest_overlap(&world, c, end) < 1e-3,
                "{start:?} -> {end:?} still overlaps"
            );
            // El lugar libre más cercano: cruzando el vagón hasta su otro lado,
            // o más allá de alguno de sus extremos cuando la pared deja lugar
            // ahí y eso queda más cerca.
            let across = start.y + 1.5 + 0.9;
            let past_end = (start.x + 5.9).min(5.9 - start.x);
            let nearest = if start.y + 0.9 <= 2.0 {
                across.min(past_end)
            } else {
                across
            };
            assert!(
                end.distance(start) <= nearest + 0.01,
                "{start:?} -> {end:?}, nearest is {nearest}"
            );
        }
    }

    #[test]
    fn a_pinch_near_the_car_end_escapes_past_the_end_when_closer() {
        // La pared solo cubre el extremo este del vagón, y deja lugar más allá.
        let mut world = pinch_world();
        world.obstacles =
            StaticObstacles::new(vec![Aabb::new(Vec2::new(3.0, 2.0), Vec2::new(4.6, 6.0))]);
        let c = Circle::new(0.9);
        let start = Vec2::new(4.5, 1.6);
        let end = world.resolve(c, start, |_| ());
        assert!(deepest_overlap(&world, c, end) < 1e-3, "{end:?}");
        assert!(end.distance(start) < 3.0, "nearest free spot: {end:?}");
    }

    #[test]
    fn bodies_push_the_circle_aside_but_walls_win() {
        let world = pinch_world();
        let c = Circle::new(0.3);
        let rider = Body {
            position: Vec2::new(0.0, 10.0),
            collider: Circle::new(0.9),
        };
        // Terreno abierto: directo hacia afuera del jinete.
        let end = world.resolve_among(c, Vec2::new(0.0, 10.5), &[rider], |_| ());
        assert!(end.distance(Vec2::new(0.0, 11.2)) < 1e-4, "{end:?}");
        assert_eq!(
            world.resolve(c, Vec2::new(0.0, 10.5), |_| ()),
            Vec2::new(0.0, 10.5)
        );
        // Un jinete contra la cara norte de la pared no puede meterse en la
        // pared.
        let rider = Body {
            position: Vec2::new(0.0, 7.0),
            ..rider
        };
        let end = world.resolve_among(c, Vec2::new(0.0, 6.4), &[rider], |_| ());
        assert!(deepest_overlap(&world, c, end) < 1e-3, "{end:?}");
    }

    #[test]
    fn unpinched_pushes_are_unchanged() {
        let world = pinch_world();
        let c = Circle::new(0.9);
        // Mitad sur del vagón, terreno abierto más allá: empuje simple a lo
        // ancho.
        let end = world.resolve(c, Vec2::new(1.0, -0.5), |_| ());
        assert!(end.distance(Vec2::new(1.0, -2.4)) < 1e-4, "{end:?}");
        // Contra la pared desde el norte: empuje simple hacia afuera.
        let end = world.resolve(c, Vec2::new(0.0, 6.5), |_| ());
        assert!(end.distance(Vec2::new(0.0, 6.9)) < 1e-4, "{end:?}");
    }

    /// Los pueblos distribuidos con sus caminos trazados, sin plantar: los
    /// árboles se mantienen fuera de los caminos (`nothing_blocks_a_rider_...`
    /// lo comprueba).
    fn roaded_world() -> (World, crate::RoadNetwork, RailLine) {
        let (towns, line, roads) = crate::road::tests::shipped_roads();
        let config = WorldConfig {
            half_extent: WorldConfig::MAX_HALF_EXTENT,
            ..WorldConfig::default()
        };
        let mut world = World::with_towns(config, Terrain::default(), &towns);
        world.lay_roads(roads.clone());
        (world, roads, line)
    }

    #[test]
    fn nothing_blocks_a_rider_anywhere_on_a_road_or_its_level_crossings() {
        let (mut world, roads, line) = roaded_world();
        world.plant(&FloraSite::new(world.config, &world.terrain, &line));
        let c = Circle::new(crate::Rider::COLLIDER_RADIUS);
        for road in &roads.roads {
            for w in road.path.windows(2) {
                for k in 0..4 {
                    let p = w[0] + (w[1] - w[0]) * (k as f32 / 4.0);
                    assert_eq!(world.resolve(c, p, |_| ()), p, "blocked at {p:?}");
                }
            }
        }
        // Las señales están al costado del paso a nivel, el poste al costado
        // del camino.
        assert!(world.obstacles.len() > roads.obstacles().len());
    }

    #[test]
    fn a_rider_rides_out_of_town_across_the_level_crossing_to_the_rural_track() {
        let (world, roads, _) = roaded_world();
        let params = crate::RiderParams::default();
        for road in roads
            .roads
            .iter()
            .filter(|r| r.kind == crate::RoadKind::Rural)
        {
            let crossing = road.crossings[0];
            let start = road.path[0];
            let out = road.ends[0].out;
            let mut rider = crate::Rider::new(
                start,
                crate::Heading::from_radians(out.y.atan2(out.x)),
                &world,
            );
            let mut passed = false;
            for _ in 0..60 * 120 {
                // Apuntar a un punto un poco más adelante sobre el camino.
                let near = (0..road.path.len())
                    .min_by(|&a, &b| {
                        road.path[a]
                            .distance(rider.position)
                            .total_cmp(&road.path[b].distance(rider.position))
                    })
                    .unwrap();
                let aim = road.path[(near + 3).min(road.path.len() - 1)] - rider.position;
                let want = aim.y.atan2(aim.x);
                let mut turn = want - rider.heading.radians();
                while turn > std::f32::consts::PI {
                    turn -= std::f32::consts::TAU;
                }
                while turn < -std::f32::consts::PI {
                    turn += std::f32::consts::TAU;
                }
                let input = crate::RiderInput {
                    throttle: 1.0,
                    turn: (turn * 3.0).clamp(-1.0, 1.0),
                    gallop: true,
                };
                rider.step(input, &params, &world, 1.0 / 60.0);
                passed |= (rider.position - crossing.at).dot(crossing.road) > 10.0;
                if rider.position.distance(road.ends[1].at) < 8.0 {
                    break;
                }
            }
            assert!(passed, "never crossed at {:?}", crossing.at);
            assert!(
                rider.position.distance(road.ends[1].at) < 8.0,
                "stuck at {:?}",
                rider.position
            );
        }
    }

    /// La plataforma de un paso a nivel, por encima del suelo: la parte
    /// superior de los tablones.
    const DECK: f32 = crate::LineTrain::RAIL_TOP - 0.02;

    #[test]
    fn the_road_climbs_gently_onto_each_crossing_deck_and_down_again() {
        let (world, roads, _) = roaded_world();
        let raised = |p: Vec2| world.ground_height(p) - world.terrain.height(p.x, p.y);
        for c in roads.crossings() {
            let mut last: Option<f32> = None;
            for k in -60..=60 {
                let p = c.at + c.road * (k as f32 * 0.25);
                let h = world.ground_height(p);
                if let Some(before) = last {
                    let slope = (h - before).abs() / 0.25;
                    let land = (world.terrain.height(p.x, p.y)
                        - world
                            .terrain
                            .height(p.x - c.road.x * 0.25, p.y - c.road.y * 0.25))
                    .abs()
                        / 0.25;
                    assert!(slope - land <= 0.105, "{slope} at {p:?}");
                }
                last = Some(h);
            }
            assert!((raised(c.at) - DECK).abs() < 0.01, "{}", raised(c.at));
            for s in [-12.0, 12.0] {
                assert!(raised(c.at + c.road * s).abs() < 1e-4);
            }
            // A lo largo de la vía, más allá de los bordes del camino, el suelo
            // es el del terreno (y el terreno debajo de la vía está intacto).
            for s in [-9.0, -6.0, 6.0, 9.0] {
                let p = c.at + c.rail * s;
                assert_eq!(
                    world.ground_height(p),
                    world.terrain.height(p.x, p.y),
                    "{s}"
                );
            }
        }
    }

    #[test]
    fn a_rider_rides_up_onto_the_deck_and_over_the_rails() {
        let (world, roads, _) = roaded_world();
        let params = crate::RiderParams::default();
        for c in roads.crossings() {
            let start = c.at - c.road * 20.0;
            let heading = crate::Heading::from_radians(c.road.y.atan2(c.road.x));
            let mut rider = crate::Rider::new(start, heading, &world);
            let mut on_deck = None;
            for _ in 0..60 * 20 {
                let input = crate::RiderInput {
                    throttle: 1.0,
                    ..Default::default()
                };
                rider.step(input, &params, &world, 1.0 / 60.0);
                let ground = world.terrain.height(rider.position.x, rider.position.y);
                if rider.position.distance(c.at) < 0.5 {
                    on_deck = Some(rider.height - ground);
                }
                if (rider.position - c.at).dot(c.road) > 15.0 {
                    break;
                }
            }
            let lift = on_deck.expect("rode over the crossing");
            assert!((lift - DECK).abs() < 0.01, "{lift}");
            assert!((rider.position - c.at).dot(c.road) > 15.0, "stuck");
        }
    }

    #[test]
    fn a_train_car_on_the_level_crossing_still_blocks_a_rider() {
        let (mut world, roads, _) = roaded_world();
        let crossing = *roads.crossings().next().expect("a level crossing");
        let across = Vec2::new(-crossing.rail.y, crossing.rail.x);
        world.moving.push(SideObstacle {
            footprint: Aabb::from_center_size(crossing.at, Vec2::new(16.0, 3.0)),
            across,
        });
        let c = Circle::new(crate::Rider::COLLIDER_RADIUS);
        let end = world.resolve(c, crossing.at, |_| ());
        assert!(end.distance(crossing.at) > 1.0, "{end:?}");
        assert!(deepest_overlap(&world, c, end) < 1e-3);
    }
}
