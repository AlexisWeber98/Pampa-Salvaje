//! Which towns are near enough to the player to be drawn and simulated.
//!
//! Every town's footprints stay in the logic [`crate::World`] (they are
//! cheap); only drawing and the crowd simulation follow the player. A town
//! becomes active within `margin` meters of its extent and goes inactive
//! only past `margin + hysteresis`, so riding along the edge does not
//! rebuild it every frame.

use crate::{TownLayout, Vec2};

/// What changed in one update, as town indices in ascending order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActivationChange {
    pub activated: Vec<usize>,
    pub deactivated: Vec<usize>,
}

/// The active towns, and the rule for changing them.
#[derive(Debug, Clone, PartialEq)]
pub struct TownActivation {
    /// Distance from a town's extent within which it activates, in meters.
    pub margin: f32,
    /// Extra distance before an active town deactivates, in meters.
    pub hysteresis: f32,
    active: Vec<bool>,
}

impl Default for TownActivation {
    fn default() -> Self {
        Self::new(Self::DEFAULT_MARGIN, Self::DEFAULT_HYSTERESIS)
    }
}

impl TownActivation {
    /// Activate within 600 m of a town: well past the fog, so a town is
    /// built before it can be seen, railway run-out included.
    pub const DEFAULT_MARGIN: f32 = 600.0;
    /// Keep a town until 150 m past the margin.
    pub const DEFAULT_HYSTERESIS: f32 = 150.0;

    /// No town active yet.
    pub fn new(margin: f32, hysteresis: f32) -> Self {
        Self {
            margin,
            hysteresis: hysteresis.max(0.0),
            active: Vec::new(),
        }
    }

    /// Activates the towns near `focus` and deactivates the far ones.
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

    /// Whether town `index` is active.
    pub fn is_active(&self, index: usize) -> bool {
        self.active.get(index).copied().unwrap_or(false)
    }

    /// Indices of the active towns, ascending.
    pub fn active(&self) -> impl Iterator<Item = usize> + '_ {
        self.active
            .iter()
            .enumerate()
            .filter_map(|(i, &on)| on.then_some(i))
    }
}

/// Distance from `p` to the nearest point of `town`'s extent (0 inside).
pub fn distance_to_town(town: &TownLayout, p: Vec2) -> f32 {
    town.extent().closest_point(p).distance(p)
}

/// Index of the town whose extent is nearest `p`; ties go to the earlier
/// town. `None` without towns.
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
        // Nothing changes while standing still.
        assert_eq!(a.update(&towns, Vec2::ZERO), ActivationChange::default());

        let east = towns[0].extent().max.x;
        // Past the margin but inside the hysteresis: still active.
        let change = a.update(&towns, Vec2::new(east + 700.0, 0.0));
        assert!(change.deactivated.is_empty() && a.is_active(0));
        // Past both: gone.
        let change = a.update(&towns, Vec2::new(east + 800.0, 0.0));
        assert_eq!(change.deactivated, vec![0]);
        assert_eq!(a.active().count(), 0);
        // Back inside the margin: active again.
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
