//! Los árboles colisionan solo donde está el jinete: los troncos de los árboles
//! de los chunks alrededor del jugador son obstáculos, que entran por streaming
//! a medida que el jugador se acerca a un chunk y salen cuando lo deja. Plantar
//! todo el mundo de 24 km al inicio (cientos de miles de troncos) tardaba
//! segundos, para árboles que nadie puede alcanzar. Los arbustos y el pasto
//! nunca son obstáculos.
//!
//! Los árboles de cada chunk vienen de [`FloraSite`], determinista por chunk,
//! así que un chunk que se va y vuelve tiene exactamente los mismos árboles.
//! Este anillo es independiente del streaming de terreno del crate de render,
//! así que los juegos sin interfaz colisionan igual. Lo que se mueve lejos del
//! jugador (el ganado en sus campos alambrados) no ve árboles allí; los trenes
//! nunca los necesitan, la flora se mantiene fuera de toda la vía.

use bevy::prelude::*;
use pampa_core::{ChunkCoord, FloraSite, Vec2, World};
use pampa_render::WorldFlora;

use crate::player::{DrivePlayer, GameWorld, Player};

/// Chunks (distancia de Chebyshev desde el chunk del jugador) cuyos árboles
/// están cargados. Un chunk mide 200 m: con un chunk cargado todo alrededor,
/// cada árbol a menos de 200 m del jugador colisiona, mucho más allá de lo que
/// el jinete alcanza en un frame (unos 12 m/s al galope), el ganado y los
/// vecinos más cercanos, y los pocos metros de la cámara de seguimiento.
pub const LOAD_RADIUS: i32 = 1;
/// Los chunks más allá de esta distancia se descartan. Uno más que
/// [`LOAD_RADIUS`], para que cabalgar a lo largo del borde de un chunk no
/// plante y descarte el mismo chunk.
pub const KEEP_RADIUS: i32 = 2;
/// Máximo de chunks plantados en un frame además del propio del jugador, que
/// siempre se planta de inmediato. Calcular las plantas de un chunk lleva unos
/// milisegundos.
pub const BUDGET: usize = 1;

/// Los chunks cargados más allá de [`KEEP_RADIUS`] de `center`, y los chunks
/// dentro de [`LOAD_RADIUS`] que faltan plantar: primero el del jugador, luego
/// los más cercanos.
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

/// Planta los chunks alrededor de `center` y descarta los lejanos, calculando
/// como máximo `budget` chunks además del que contiene `center`, los más
/// cercanos primero. Devuelve cuántos chunks se plantaron y se descartaron.
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

/// Mantiene plantadas las plantas alrededor del jugador, antes de que el
/// jugador se mueva.
fn stream_plants_round_player(
    flora: Option<Res<WorldFlora>>,
    mut world: ResMut<GameWorld>,
    players: Query<&Player>,
) {
    let (Some(flora), Ok(player)) = (flora, players.single()) else {
        return;
    };
    let center = player.rider.position;
    // Primero mirar: un frame sin nada que hacer no debe marcar el mundo como
    // cambiado.
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
    use pampa_core::{Aabb, Circle, Heading, Rider};
    use std::time::Duration;

    /// Las huellas que `near` encuentra en los tres por tres chunks alrededor
    /// de `center` (menos un metro en el borde, para que un chunk cargado más
    /// allá no lo toque), en el orden de la consulta.
    fn seen(world: &World, center: Vec2) -> Vec<Aabb> {
        let side = 3.0 * world.config.chunk_size - 2.0;
        let area = Aabb::from_center_size(center, Vec2::new(side, side));
        world.obstacles.near(area).into_iter().copied().collect()
    }

    /// El más denso de los primeros chunks del monte, con plantas todo
    /// alrededor.
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
        let laid = crate::shipped_world::laid();
        let mut world = laid.world.clone();
        let coord = planted_chunk(&laid.flora, &world);
        let trees = laid.flora.trees(coord);
        assert!(trees.len() > 20, "a chunk with trees");
        // No se planta todo el mundo: ningún chunk entró por streaming, ningún
        // árbol entre las huellas permanentes.
        assert_eq!(world.obstacles.streamed_len(), 0);
        assert!(world.obstacles.len() < 200_000);
        for p in trees.iter().map(|t| t.obstacle()) {
            assert!(!world.obstacles.contains(&p), "{p:?} planted at startup");
        }
        // El chunk propio del jugador se planta de inmediato, el resto dentro
        // del radio de carga en los frames siguientes, los más cercanos
        // primero.
        let size = world.config.chunk_size;
        let center = coord.center(size);
        let (planted, dropped) = stream_plants(&mut world, &laid.flora, center, 0);
        assert_eq!((planted, dropped), (1, 0));
        assert!(world.obstacles.has_chunk(coord));
        let mut frames = 0;
        while stream_plants(&mut world, &laid.flora, center, BUDGET).0 > 0 {
            frames += 1;
        }
        assert!(frames <= 8, "{frames} frames to fill the ring");
        let loaded_chunks: Vec<_> = world.obstacles.loaded_chunks().collect();
        assert_eq!(loaded_chunks.len(), 9);
        for c in loaded_chunks {
            assert!((c.x - coord.x).abs() <= LOAD_RADIUS && (c.y - coord.y).abs() <= LOAD_RADIUS);
        }
        // Las plantas colisionan: cada una bloquea a un jinete desde unos
        // metros.
        let collider = Circle::new(Rider::COLLIDER_RADIUS);
        let tree = trees[0];
        let inside = world.resolve(collider, tree.at, |_| ());
        assert!(inside.distance(tree.at) > 0.0, "a tree pushes a body out");
    }

    #[test]
    fn plants_come_and_go_with_the_player_and_come_back_the_same() {
        let laid = crate::shipped_world::laid();
        let mut world = laid.world.clone();
        let coord = planted_chunk(&laid.flora, &world);
        let size = world.config.chunk_size;
        let here = coord.center(size);
        stream_plants(&mut world, &laid.flora, here, 8);
        let first = seen(&world, here);
        assert!(first.len() > 100, "{}", first.len());
        let streamed = world.obstacles.streamed_len();
        // Cuatro chunks al este, más allá del radio de conservación: el primer
        // chunk desapareció, y sus huellas también.
        let away = here + Vec2::new(4.0 * size, 0.0);
        let (_, dropped) = stream_plants(&mut world, &laid.flora, away, 8);
        assert!(dropped >= 3, "{dropped}");
        assert!(!world.obstacles.has_chunk(coord));
        // Las huellas permanentes (alambrados) quedan; no queda ninguna planta.
        let left: Vec<Aabb> = world
            .obstacles
            .near(coord.bounds(size))
            .into_iter()
            .filter(|o| !world.obstacles.contains(o))
            .copied()
            .collect();
        assert!(left.is_empty(), "{} plants left behind", left.len());
        // Del tamaño del anillo: las plantas cargadas nunca superan los chunks
        // del anillo.
        assert!(world.obstacles.loaded_chunks().count() <= 25);
        // Cabalgar a lo largo del borde de un chunk ya no planta ni descarta
        // nada.
        let edge = here + Vec2::new(size / 2.0, 0.0);
        stream_plants(&mut world, &laid.flora, edge - Vec2::new(0.1, 0.0), 8);
        stream_plants(&mut world, &laid.flora, edge + Vec2::new(0.1, 0.0), 8);
        let held = world.obstacles.streamed_len();
        for dx in [-0.1, 0.1, -0.1, 0.1] {
            let moved = stream_plants(&mut world, &laid.flora, edge + Vec2::new(dx, 0.0), 8);
            assert_eq!(moved, (0, 0), "no churn at a border");
        }
        assert_eq!(world.obstacles.streamed_len(), held);
        // De vuelta: las mismas plantas, hasta la huella.
        stream_plants(&mut world, &laid.flora, here, 8);
        assert_eq!(seen(&world, here), first);
        assert!(world.obstacles.streamed_len() >= streamed);
    }

    /// Un juego sin interfaz con el jugador en `spawn` y el anillo de plantas.
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
        let laid = crate::shipped_world::laid();
        let coord = planted_chunk(&laid.flora, &laid.world);
        let trees = laid.flora.trees(coord);
        let shrubs = laid.flora.shrubs(coord);
        let radius = Rider::COLLIDER_RADIUS;
        let mut full = laid.world.clone();
        full.plant_chunk(&laid.flora, coord);
        // Plantas sin ningún árbol del chunk dentro del carril de galope.
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
            // El jugador empieza 10 m al este y galopa hacia el oeste: su chunk
            // todavía no está plantado.
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
                // Detenido contra él, no arrastrado más allá.
                assert!(!through, "inside the tree");
                assert!(rider.position.x > at.x, "{:?}", rider.position);
                assert!(rider.position.x < at.x + 3.0, "{:?}", rider.position);
            } else {
                assert!(through, "the shrub stopped the rider");
                assert!(rider.position.x < at.x - 1.0, "{:?}", rider.position);
            }
        }
    }
}
