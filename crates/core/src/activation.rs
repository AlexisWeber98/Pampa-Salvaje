//! Qué pueblos están lo bastante cerca del jugador como para dibujarlos y
//! simularlos.
//!
//! Las huellas de todos los pueblos se quedan en el [`crate::World`] lógico
//! (son baratas); solo el dibujo y la simulación de la multitud siguen al
//! jugador. Un pueblo se activa a menos de `margin` metros de su extensión y se
//! desactiva recién más allá de `margin + hysteresis`, para que andar a caballo
//! por el borde no lo reconstruya en cada frame.

use crate::{TownLayout, Vec2};

/// Lo que cambió en una actualización, como índices de pueblos en orden
/// ascendente.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActivationChange {
    pub activated: Vec<usize>,
    pub deactivated: Vec<usize>,
}

/// Los pueblos activos y la regla para cambiarlos.
#[derive(Debug, Clone, PartialEq)]
pub struct TownActivation {
    /// Distancia desde la extensión de un pueblo dentro de la cual se activa,
    /// en metros.
    pub margin: f32,
    /// Distancia extra antes de que un pueblo activo se desactive, en metros.
    pub hysteresis: f32,
    active: Vec<bool>,
}

impl Default for TownActivation {
    fn default() -> Self {
        Self::new(Self::DEFAULT_MARGIN, Self::DEFAULT_HYSTERESIS)
    }
}

impl TownActivation {
    /// Se activa a menos de 600 m de un pueblo: bastante más allá de la niebla,
    /// así que un pueblo se construye antes de que se pueda ver, incluido el
    /// tramo final de la vía férrea.
    pub const DEFAULT_MARGIN: f32 = 600.0;
    /// Conserva un pueblo hasta 150 m más allá del margen.
    pub const DEFAULT_HYSTERESIS: f32 = 150.0;

    /// Todavía no hay ningún pueblo activo.
    pub fn new(margin: f32, hysteresis: f32) -> Self {
        Self {
            margin,
            hysteresis: hysteresis.max(0.0),
            active: Vec::new(),
        }
    }

    /// Activa los pueblos cercanos a `focus` y desactiva los lejanos.
    pub fn update(&mut self, towns: &[TownLayout], focus: Vec2) -> ActivationChange {
        let mut change = ActivationChange::default();
        for index in towns.len()..self.active.len() {
            if self.active[index] {
                change.deactivated.push(index);
            }
        }
        self.active.resize(towns.len(), false);
        for (index, town) in towns.iter().enumerate() {
            let distance = distance_to_town(town, focus);
            let active = &mut self.active[index];
            if !*active && distance <= self.margin {
                *active = true;
                change.activated.push(index);
            } else if *active && distance > self.margin + self.hysteresis {
                *active = false;
                change.deactivated.push(index);
            }
        }
        change.deactivated.sort_unstable();
        change
    }

    /// Si el pueblo `index` está activo.
    pub fn is_active(&self, index: usize) -> bool {
        self.active.get(index).copied().unwrap_or(false)
    }

    /// Índices de los pueblos activos, en orden ascendente.
    pub fn active(&self) -> impl Iterator<Item = usize> + '_ {
        self.active
            .iter()
            .enumerate()
            .filter_map(|(i, &on)| on.then_some(i))
    }
}

/// Distancia desde `p` hasta el punto más cercano de la extensión de `town` (0
/// si está adentro).
pub fn distance_to_town(town: &TownLayout, p: Vec2) -> f32 {
    town.extent().closest_point(p).distance(p)
}

/// Índice del pueblo cuya extensión está más cerca de `p`; en caso de empate
/// gana el pueblo anterior. `None` si no hay pueblos.
pub fn nearest_town(towns: &[TownLayout], p: Vec2) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (index, town) in towns.iter().enumerate() {
        let distance = distance_to_town(town, p);
        if best.is_none_or(|(_, d)| distance < d) {
            best = Some((index, distance));
        }
    }
    best.map(|(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TownConfig;

    fn towns() -> Vec<TownLayout> {
        [Vec2::ZERO, Vec2::new(3000.0, -2000.0)]
            .into_iter()
            .enumerate()
            .map(|(k, origin)| {
                TownLayout::generate(&TownConfig {
                    name: format!("Town {k}"),
                    origin,
                    ..TownConfig::castex()
                })
                .unwrap()
            })
            .collect()
    }

    #[test]
    fn towns_activate_near_and_deactivate_past_the_hysteresis() {
        let towns = towns();
        let mut a = TownActivation::default();
        let change = a.update(&towns, Vec2::ZERO);
        assert_eq!(change.activated, vec![0]);
        assert!(change.deactivated.is_empty());
        assert!(a.is_active(0) && !a.is_active(1));
        // Nada cambia mientras se está quieto.
        assert_eq!(a.update(&towns, Vec2::ZERO), ActivationChange::default());

        let east = towns[0].extent().max.x;
        // Pasado el margen pero dentro de la histéresis: sigue activo.
        let change = a.update(&towns, Vec2::new(east + 700.0, 0.0));
        assert!(change.deactivated.is_empty() && a.is_active(0));
        // Pasado ambos: desaparece.
        let change = a.update(&towns, Vec2::new(east + 800.0, 0.0));
        assert_eq!(change.deactivated, vec![0]);
        assert_eq!(a.active().count(), 0);
        // De nuevo dentro del margen: activo otra vez.
        let change = a.update(&towns, Vec2::new(east + 550.0, 0.0));
        assert_eq!(change.activated, vec![0]);
    }

    #[test]
    fn riding_between_towns_hands_over_activation() {
        let towns = towns();
        let mut a = TownActivation::default();
        a.update(&towns, Vec2::ZERO);
        let target = towns[1].grid().center();
        let mut seen_both_off = false;
        for k in 0..=100 {
            let p = target * (k as f32 / 100.0);
            a.update(&towns, p);
            seen_both_off |= a.active().count() == 0;
            assert!(a.active().count() <= 2);
        }
        assert!(seen_both_off, "3.6 km apart: nothing active halfway");
        assert_eq!(a.active().collect::<Vec<_>>(), vec![1]);
    }

    #[test]
    fn the_nearest_town_is_measured_to_its_extent() {
        let towns = towns();
        assert_eq!(nearest_town(&[], Vec2::ZERO), None);
        assert_eq!(nearest_town(&towns, Vec2::new(10.0, 10.0)), Some(0));
        assert_eq!(nearest_town(&towns, Vec2::new(2900.0, -1900.0)), Some(1));
        assert_eq!(nearest_town(&towns, Vec2::new(1700.0, -1200.0)), Some(1));
        assert_eq!(distance_to_town(&towns[0], Vec2::ZERO), 0.0);
        let east = towns[0].extent().max.x;
        assert!((distance_to_town(&towns[0], Vec2::new(east + 50.0, 0.0)) - 50.0).abs() < 1e-3);
    }
}
