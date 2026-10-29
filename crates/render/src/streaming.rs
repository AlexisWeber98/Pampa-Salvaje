//! Qué chunks de terreno deben existir alrededor de un punto de foco: pura
//! contabilidad, que se puede probar sin GPU.
//!
//! Se quieren los chunks que están a `radius` chunks o menos del chunk de foco,
//! primero los más cercanos. Un chunk cargado se descarta solo cuando está a
//! más de `radius + 1` (histéresis), así que cabalgar de ida y vuelta por el
//! borde de un chunk no reconstruye los mismos chunks. Se agregan como máximo
//! `budget` chunks por actualización, así que la generación nunca traba un
//! frame.

use std::collections::BTreeSet;

use pampa_core::{ChunkCoord, Vec2 as GroundVec2, WorldConfig};

/// Lo que pide una actualización.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StreamingPlan {
    /// Chunks a construir ahora, primero los más cercanos.
    pub spawn: Vec<ChunkCoord>,
    /// Chunks a descartar ahora.
    pub despawn: Vec<ChunkCoord>,
}

/// Los chunks cargados actualmente y las reglas para cambiarlos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamingSet {
    /// Radio de carga, en chunks.
    pub radius: i32,
    /// Cantidad máxima de chunks construidos por actualización.
    pub budget: usize,
    loaded: BTreeSet<ChunkCoord>,
}

impl StreamingSet {
    /// Un conjunto vacío que carga `radius` chunks alrededor del foco, como
    /// máximo `budget` por actualización (al menos uno).
    pub fn new(radius: i32, budget: usize) -> Self {
        Self {
            radius: radius.max(0),
            budget: budget.max(1),
            loaded: BTreeSet::new(),
        }
    }

    /// Planifica el siguiente paso para tener cargado en `world` todo chunk
    /// cercano a `focus`, y registra el plan como hecho.
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

    /// Si el chunk `c` está cargado.
    pub fn is_loaded(&self, c: ChunkCoord) -> bool {
        self.loaded.contains(&c)
    }

    /// Todos los chunks cargados, de sur a norte y luego de oeste a este.
    pub fn loaded(&self) -> impl Iterator<Item = ChunkCoord> + '_ {
        self.loaded.iter().copied()
    }

    /// Cuántos chunks están cargados.
    pub fn len(&self) -> usize {
        self.loaded.len()
    }

    /// Si no hay ningún chunk cargado.
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

    /// Actualiza hasta que no se crea nada más; devuelve cuántas
    /// actualizaciones hicieron falta.
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
        // 29 chunks en un disco de radio 3, 4 por actualización: 7
        // actualizaciones más construyen los otros 25, y la 8.ª no encuentra
        // nada por construir.
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
        // Un chunk al este: (-2, 0) queda ahora a 3 (= radius + 1): se
        // conserva.
        let plan = set.update(&world(), GroundVec2::new(300.0, 100.0));
        assert!(!plan.despawn.contains(&ChunkCoord::new(-2, 0)));
        assert!(plan.spawn.contains(&ChunkCoord::new(3, 0)));
        // De vuelta: no hay nada que reconstruir.
        let plan = set.update(&world(), GroundVec2::new(100.0, 100.0));
        assert!(plan.spawn.is_empty(), "{plan:?}");
        assert!(plan.despawn.is_empty(), "{plan:?}");
        // Dos chunks al este: (-2, 0) queda a 4 y se descarta.
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
        // Nunca más que el disco de radius + 1 (45 chunks).
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
