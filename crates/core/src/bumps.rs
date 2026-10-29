//! Empujones: el jinete que se lleva por delante a los vecinos, y quién se
//! queja por eso.
//!
//! [`crate::Townsfolk::make_way`] informa a qué peatones empujó el jinete;
//! [`BumpTracker`] convierte esos contactos en [`BumpEvent`]s cuando el jinete
//! iba lo bastante rápido, como mucho una vez por peatón cada
//! [`BumpTracker::COOLDOWN`] segundos. Después, [`LinePicker`] elige qué frase
//! de un conjunto dice el peatón, de forma determinista y sin repetir la frase
//! anterior de ese conjunto.

use crate::rng::Rng;
use crate::{AnimationState, RiderParams};

/// Qué tan fuerte empujó el jinete a alguien.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BumpSeverity {
    /// Empujado al paso.
    Walk,
    /// Atropellado al galope.
    Gallop,
}

impl BumpSeverity {
    /// Gravedad de un contacto a velocidad `speed` (m/s, en cualquier sentido),
    /// o `None` por debajo de [`BumpTracker::MIN_SPEED`] (un roce casi
    /// detenido). Usa las mismas franjas de marcha que la animación del jinete.
    pub fn from_speed(speed: f32, params: &RiderParams) -> Option<Self> {
        let speed = speed.abs();
        if speed.is_nan() || speed < BumpTracker::MIN_SPEED {
            return None;
        }
        match AnimationState::from_speed(speed, params.walk_max) {
            AnimationState::Gallop => Some(Self::Gallop),
            _ => Some(Self::Walk),
        }
    }
}

/// Un peatón se queja porque lo empujaron.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BumpEvent {
    /// Índice del peatón en [`crate::Townsfolk::people`].
    pub walker: usize,
    pub severity: BumpSeverity,
}

/// Convierte los contactos del jinete en eventos de empujón, con un tiempo de
/// espera por peatón.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BumpTracker {
    /// Segundos hasta que cada peatón pueda volver a quejarse (indexado por
    /// peatón).
    cooldown: Vec<f32>,
}

impl BumpTracker {
    /// Velocidad mínima del jinete que cuenta como empujón, en m/s.
    pub const MIN_SPEED: f32 = 1.5;
    /// Segundos antes de que el mismo peatón vuelva a quejarse.
    pub const COOLDOWN: f32 = 6.0;

    /// Deja pasar `dt` segundos de tiempo de espera. Un `dt` no finito o no
    /// positivo no cambia nada.
    pub fn tick(&mut self, dt: f32) {
        if !(dt.is_finite() && dt > 0.0) {
            return;
        }
        for left in &mut self.cooldown {
            *left = (*left - dt).max(0.0);
        }
    }

    /// Eventos para los peatones de `contacts` que el jinete acaba de empujar
    /// mientras se movía a `rider_speed`. Los peatones que todavía están en
    /// tiempo de espera se quedan callados.
    pub fn register(
        &mut self,
        contacts: &[usize],
        rider_speed: f32,
        params: &RiderParams,
    ) -> Vec<BumpEvent> {
        let Some(severity) = BumpSeverity::from_speed(rider_speed, params) else {
            return Vec::new();
        };
        let mut events = Vec::new();
        for &walker in contacts {
            if walker >= self.cooldown.len() {
                self.cooldown.resize(walker + 1, 0.0);
            }
            if self.cooldown[walker] > 0.0 {
                continue;
            }
            self.cooldown[walker] = Self::COOLDOWN;
            events.push(BumpEvent { walker, severity });
        }
        events
    }
}

/// Elige frases de conjuntos de frases, de forma determinista para una semilla,
/// sin elegir nunca el mismo índice de un conjunto dos veces seguidas cuando el
/// conjunto tiene más de una frase.
#[derive(Debug, Clone, PartialEq)]
pub struct LinePicker {
    rng: Rng,
    /// Último índice elegido por id de conjunto.
    last: Vec<(usize, usize)>,
}

impl LinePicker {
    pub fn new(seed: u32) -> Self {
        Self {
            rng: Rng::new(seed, 0),
            last: Vec::new(),
        }
    }

    /// Un índice dentro del conjunto `pool`, que tiene `n` frases, o `None`
    /// cuando está vacío.
    pub fn pick(&mut self, pool: usize, n: usize) -> Option<usize> {
        if n == 0 {
            return None;
        }
        let slot = match self.last.iter().position(|&(p, _)| p == pool) {
            Some(slot) => slot,
            None => {
                self.last.push((pool, usize::MAX));
                self.last.len() - 1
            }
        };
        let last = self.last[slot].1;
        let index = if n > 1 && last < n {
            // Elige entre las otras n - 1 frases, salteando la última.
            let i = self.rng.below(n - 1);
            if i >= last { i + 1 } else { i }
        } else {
            self.rng.below(n)
        };
        self.last[slot].1 = index;
        Some(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_follows_the_gait() {
        let p = RiderParams::default();
        assert_eq!(BumpSeverity::from_speed(0.0, &p), None);
        assert_eq!(BumpSeverity::from_speed(1.0, &p), None, "a crawl");
        assert_eq!(BumpSeverity::from_speed(1.49, &p), None);
        assert_eq!(
            BumpSeverity::from_speed(BumpTracker::MIN_SPEED, &p),
            Some(BumpSeverity::Walk)
        );
        assert_eq!(
            BumpSeverity::from_speed(p.walk_max, &p),
            Some(BumpSeverity::Walk)
        );
        assert_eq!(
            BumpSeverity::from_speed(p.gallop_max, &p),
            Some(BumpSeverity::Gallop)
        );
        // Chocar a alguien marcha atrás también cuenta según la velocidad.
        assert_eq!(BumpSeverity::from_speed(-2.0, &p), Some(BumpSeverity::Walk));
        for speed in [2.0, 5.0, 11.0] {
            let animation = AnimationState::from_speed(speed, p.walk_max);
            let gallop = BumpSeverity::from_speed(speed, &p) == Some(BumpSeverity::Gallop);
            assert_eq!(gallop, animation == AnimationState::Gallop, "{speed}");
        }
    }

    #[test]
    fn a_fast_contact_fires_once_then_cools_down() {
        let p = RiderParams::default();
        let mut t = BumpTracker::default();
        assert_eq!(
            t.register(&[3], 9.0, &p),
            [BumpEvent {
                walker: 3,
                severity: BumpSeverity::Gallop
            }]
        );
        // Sigue empujando a la misma persona: silencio durante el tiempo de
        // espera.
        for _ in 0..119 {
            t.tick(0.05);
            assert!(t.register(&[3], 9.0, &p).is_empty());
        }
        // Otra persona todavía puede quejarse.
        assert_eq!(t.register(&[1], 2.0, &p).len(), 1);
        t.tick(0.06);
        assert_eq!(
            t.register(&[3], 2.0, &p),
            [BumpEvent {
                walker: 3,
                severity: BumpSeverity::Walk
            }]
        );
    }

    #[test]
    fn slow_contacts_never_fire_nor_start_a_cooldown() {
        let p = RiderParams::default();
        let mut t = BumpTracker::default();
        assert!(t.register(&[0, 2], 0.8, &p).is_empty());
        assert_eq!(t.register(&[0, 2], 9.0, &p).len(), 2);
    }

    #[test]
    fn several_walkers_and_duplicates_in_one_contact_list() {
        let p = RiderParams::default();
        let mut t = BumpTracker::default();
        let events = t.register(&[5, 0, 5], 3.5, &p);
        assert_eq!(
            events,
            [
                BumpEvent {
                    walker: 5,
                    severity: BumpSeverity::Gallop
                },
                BumpEvent {
                    walker: 0,
                    severity: BumpSeverity::Gallop
                },
            ]
        );
    }

    #[test]
    fn bad_dt_does_not_shorten_the_cooldown() {
        let p = RiderParams::default();
        let mut t = BumpTracker::default();
        t.register(&[0], 9.0, &p);
        for dt in [-100.0, 0.0] {
            t.tick(dt);
        }
        assert!(t.register(&[0], 9.0, &p).is_empty());
        t.tick(BumpTracker::COOLDOWN);
        assert_eq!(t.register(&[0], 9.0, &p).len(), 1);
    }

    #[test]
    fn picker_never_repeats_and_covers_the_pool() {
        let mut picker = LinePicker::new(7);
        for n in [2usize, 3, 10] {
            let mut seen = vec![0usize; n];
            let mut last = None;
            for _ in 0..400 {
                let i = picker.pick(n, n).unwrap();
                assert!(i < n);
                assert_ne!(Some(i), last, "pool of {n} repeated {i}");
                last = Some(i);
                seen[i] += 1;
            }
            assert!(seen.iter().all(|&k| k > 0), "{seen:?}");
        }
    }

    #[test]
    fn picker_tracks_pools_separately_and_handles_tiny_pools() {
        let mut picker = LinePicker::new(1);
        assert_eq!(picker.pick(0, 0), None);
        assert_eq!(picker.pick(0, 1), Some(0));
        assert_eq!(picker.pick(0, 1), Some(0), "one line may repeat");
        // Conjuntos intercalados: cada conjunto sigue sin repetirse nunca.
        let (mut a, mut b) = (None, None);
        for _ in 0..200 {
            let x = picker.pick(1, 4);
            let y = picker.pick(2, 4);
            assert_ne!(x, a);
            assert_ne!(y, b);
            (a, b) = (x, y);
        }
        // Un conjunto que se achicó (contenido recargado) nunca devuelve un
        // índice viejo.
        picker.pick(3, 10);
        for _ in 0..50 {
            assert!(picker.pick(3, 2).unwrap() < 2);
        }
    }

    #[test]
    fn picker_is_deterministic_per_seed() {
        let run = |seed| {
            let mut p = LinePicker::new(seed);
            (0..50).map(|_| p.pick(0, 8).unwrap()).collect::<Vec<_>>()
        };
        assert_eq!(run(4), run(4));
        assert_ne!(run(4), run(5));
    }
}
