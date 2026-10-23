//! Lagoons: the shallow freshwater "lagunas" of the plains.
//!
//! A [`Lagoon`] is a basin lowered into the [`Terrain`] around an irregular
//! shore, holding a flat sheet of water. Its shore is a rounded rectangle (a
//! superellipse, like the squarish real Laguna La Arocena), roughened by a
//! few small seeded harmonics of the radius, so it stays star-shaped around
//! the center; its bottom shelves down from the shore and its bank eases back
//! into the pampa. The water stands a little below the lowest ground of the
//! bank, so it never spills over. People and horses wade the shallows and
//! stop where the water gets deep ([`Lagoon::push_out`]); fences, settlements,
//! roads and trees keep off its [`Lagoon::keep_out`] box.

use std::f32::consts::TAU;
use std::fmt;

use crate::rng::Rng;
use crate::{Aabb, RailLine, RoadNetwork, Terrain, TownLayout, Vec2, WorldConfig};

/// How many harmonics roughen the shore (the third to the sixth).
const HARMONICS: usize = 4;

/// A lagoon as configured: where it lies and how big it is.
#[derive(Debug, Clone, PartialEq)]
pub struct LagoonConfig {
    /// Its name, for warnings and the map.
    pub name: String,
    /// Center of the water, in meters.
    pub center: Vec2,
    /// Full size of the rounded rectangle the shore follows: east-west, then
    /// north-south, in meters.
    pub size: Vec2,
    /// Varies the shore's wobble.
    pub seed: u32,
}

/// Why a lagoon cannot be laid.
#[derive(Debug, Clone, PartialEq)]
pub enum LagoonError {
    /// A size outside [`Lagoon::MIN_SIZE`] to [`Lagoon::MAX_SIZE`], or a
    /// number that is not finite.
    BadSize { name: String },
    /// Its bank reaches past the world's edge.
    OutsideWorld { name: String },
    /// Its bank touches a town's leveled band.
    Town { name: String, town: String },
    /// The railway runs across its bank.
    Railway { name: String },
    /// A road runs across its bank.
    Road { name: String },
}

impl fmt::Display for LagoonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LagoonError::BadSize { name } => write!(
                f,
                "lagoon {name}: size must be between {} and {} m each way",
                Lagoon::MIN_SIZE,
                Lagoon::MAX_SIZE
            ),
            LagoonError::OutsideWorld { name } => {
                write!(f, "lagoon {name}: its bank reaches past the world's edge")
            }
            LagoonError::Town { name, town } => {
                write!(f, "lagoon {name}: its bank touches the town {town}")
            }
            LagoonError::Railway { name } => {
                write!(f, "lagoon {name}: the railway runs across its bank")
            }
            LagoonError::Road { name } => write!(f, "lagoon {name}: a road runs across its bank"),
        }
    }
}

impl std::error::Error for LagoonError {}

/// A shallow lagoon sunk into the terrain (see the module docs).
#[derive(Debug, Clone, PartialEq)]
pub struct Lagoon {
    pub name: String,
    pub center: Vec2,
    /// Half sides of the rounded rectangle the shore follows, in meters.
    pub radii: Vec2,
    /// Amplitude (a share of the radius) and phase of each harmonic.
    wobble: [(f32, f32); HARMONICS],
    /// Height of the water surface, in meters.
    pub level: f32,
}

impl Lagoon {
    /// Depth of the water in the middle, in meters: a plains lagoon is
    /// shallow.
    pub const DEPTH: f32 = 1.6;
    /// How far in from the shore the bottom reaches its full depth, meters.
    pub const SHELF: f32 = 30.0;
    /// Width of the bank where the ground eases from the water's edge back
    /// into the pampa, in meters.
    pub const BANK: f32 = 30.0;
    /// How far in from the shore people and horses can wade, in meters
    /// (about half a meter of water); past it the water is too deep.
    pub const WADE: f32 = 5.0;
    /// Least height of the bank above the water, in meters.
    pub const FREEBOARD: f32 = 0.6;
    /// Margin past the bank that fences, settlements, roads and trees keep
    /// clear of, in meters.
    pub const MARGIN: f32 = 15.0;
    /// Smallest and largest size each way, in meters.
    pub const MIN_SIZE: f32 = 40.0;
    pub const MAX_SIZE: f32 = 1500.0;
    /// Largest sum of the harmonics' amplitudes: the shore stays within
    /// this share of its rounded rectangle.
    const MAX_WOBBLE: f32 = 0.05;
    /// Exponent of the shore's superellipse: 2 is an ellipse; higher is
    /// squarer, with softly rounded corners.
    const SQUARENESS: f32 = 5.0;
    /// Width of the wet mud along the waterline, in meters: past it the
    /// grass grows again ([`Self::is_bare`]).
    pub const MUD_BAND: f32 = 3.0;

    /// The lagoon `config` describes, its water set just below the lowest
    /// natural ground of its bank on `terrain`.
    pub fn new(config: &LagoonConfig, terrain: &Terrain) -> Self {
        let mut rng = Rng::new(config.seed ^ LAGOON_SEED, 0);
        let mut wobble = [(0.0, 0.0); HARMONICS];
        for (k, w) in wobble.iter_mut().enumerate() {
            // Lower harmonics bend the shore more: a shallow bay, then coves.
            let share = Self::MAX_WOBBLE * [0.34, 0.28, 0.22, 0.16][k];
            *w = (share * rng.range(0.55, 1.0), rng.unit() * TAU);
        }
        let mut lagoon = Self {
            name: config.name.clone(),
            center: config.center,
            radii: config.size * 0.5,
            wobble,
            level: 0.0,
        };
        let lowest = lagoon
            .bank_samples()
            .map(|p| terrain.natural_height(p.x, p.y))
            .fold(f32::MAX, f32::min);
        lagoon.level = lowest - Self::FREEBOARD;
        lagoon
    }

    /// Points around the bank, from the shore to its outer edge: where the
    /// water level is measured against the land.
    fn bank_samples(&self) -> impl Iterator<Item = Vec2> + '_ {
        const ANGLES: usize = 96;
        (0..ANGLES).flat_map(move |k| {
            let angle = k as f32 * TAU / ANGLES as f32;
            let dir = Vec2::new(angle.cos(), angle.sin());
            let r = self.shore_radius(angle);
            (0..=6).map(move |s| self.center + dir * (r + Self::BANK * s as f32 / 6.0))
        })
    }

    /// Distance from the center to the shore at `angle` (radians from east).
    pub fn shore_radius(&self, angle: f32) -> f32 {
        let rectangle = self.rounded_rectangle(angle);
        let wobble: f32 = self
            .wobble
            .iter()
            .enumerate()
            .map(|(k, &(amp, phase))| amp * ((k + 3) as f32 * angle + phase).sin())
            .sum();
        rectangle * (1.0 + wobble)
    }

    /// Distance from the center to the smooth rounded rectangle (the
    /// superellipse `|x/a|^n + |y/b|^n = 1`) at `angle`, before the
    /// harmonics roughen it.
    pub fn rounded_rectangle(&self, angle: f32) -> f32 {
        let n = Self::SQUARENESS;
        let (c, s) = (angle.cos().abs(), angle.sin().abs());
        ((c / self.radii.x).powf(n) + (s / self.radii.y).powf(n)).powf(-1.0 / n)
    }

    /// Whether `p` is in the water or on the bare wet mud just above it,
    /// where no grass grows.
    pub fn is_bare(&self, p: Vec2) -> bool {
        self.offshore(p) < Self::MUD_BAND
    }

    /// How far `p` lies past the shore, in meters, along the line from the
    /// center: negative in the water.
    pub fn offshore(&self, p: Vec2) -> f32 {
        let d = p - self.center;
        d.length() - self.shore_radius(d.y.atan2(d.x))
    }

    /// Whether `p` is in the water.
    pub fn is_water(&self, p: Vec2) -> bool {
        self.offshore(p) < 0.0
    }

    /// Whether `p` is in water too deep to wade.
    pub fn is_deep(&self, p: Vec2) -> bool {
        self.offshore(p) < -Self::WADE
    }

    /// `n` points along the shore, counter-clockwise from east.
    pub fn shore(&self, n: usize) -> Vec<Vec2> {
        self.ring(n, 0.0)
    }

    /// `n` points `offset` meters past the shore (negative: in the water),
    /// counter-clockwise from east.
    pub fn ring(&self, n: usize, offset: f32) -> Vec<Vec2> {
        (0..n)
            .map(|k| {
                let angle = k as f32 * TAU / n as f32;
                let r = (self.shore_radius(angle) + offset).max(0.0);
                self.center + Vec2::new(angle.cos(), angle.sin()) * r
            })
            .collect()
    }

    /// Box around the whole water.
    pub fn bounds(&self) -> Aabb {
        let reach = 1.0 + Self::MAX_WOBBLE;
        Aabb::from_center_size(self.center, self.radii * (2.0 * reach))
    }

    /// Box around the water, its bank and [`Self::MARGIN`]: what fences,
    /// settlements, roads and trees keep out of.
    pub fn keep_out(&self) -> Aabb {
        self.bounds().expanded(Self::BANK + Self::MARGIN)
    }

    /// Depth of the water at `p`, in meters: zero on dry land.
    pub fn depth_at(&self, p: Vec2) -> f32 {
        let u = self.offshore(p);
        if u >= 0.0 {
            return 0.0;
        }
        let t = (-u / Self::SHELF).min(1.0);
        // Steepest at the shore, flat in the middle.
        Self::DEPTH * (1.0 - (1.0 - t) * (1.0 - t))
    }

    /// The ground at `p`, given the `ground` around it without the lagoon:
    /// its bottom under the water, its bank easing back into `ground`
    /// over [`Self::BANK`] meters, and `ground` itself past the bank.
    pub fn shape(&self, p: Vec2, ground: f32) -> f32 {
        let u = self.offshore(p);
        if u < 0.0 {
            return self.level - self.depth_at(p);
        }
        if u >= Self::BANK {
            return ground;
        }
        let s = u / Self::BANK;
        // Rises from the water's edge, easing into the land at the top.
        let ease = 1.0 - (1.0 - s) * (1.0 - s);
        self.level + (ground - self.level) * ease
    }

    /// Whether the lagoon may change the ground at `p` at all.
    pub fn touches(&self, p: Vec2) -> bool {
        self.bounds().expanded(Self::BANK).contains_point(p)
    }

    /// How far to move a body standing at `center` to take it out of the
    /// deep water, back to where it can still wade: `None` when it is not
    /// in deep water.
    pub fn push_out(&self, center: Vec2) -> Option<Vec2> {
        let d = center - self.center;
        let r = d.length();
        let angle = d.y.atan2(d.x);
        let wade = self.shore_radius(angle) - Self::WADE;
        if r >= wade {
            return None;
        }
        let dir = if r > 1e-4 {
            d * (1.0 / r)
        } else {
            Vec2::new(1.0, 0.0)
        };
        Some(dir * (wade - r + 1e-3))
    }
}

/// Seed salt of every lagoon's shore.
const LAGOON_SEED: u32 = 0x1a6_0a5e;

/// Lays every lagoon of `configs` on `terrain`, leaving out (with an
/// error each) those too small or too big, past the world's edge, or whose
/// bank touches a town's leveled band, the railway `line` or a road.
pub fn place_lagoons(
    config: &WorldConfig,
    terrain: &Terrain,
    towns: &[TownLayout],
    line: &RailLine,
    roads: &RoadNetwork,
    configs: &[LagoonConfig],
) -> (Vec<Lagoon>, Vec<LagoonError>) {
    let bounds = config.bounds();
    let (mut laid, mut errors) = (Vec::new(), Vec::new());
    for c in configs {
        let name = c.name.clone();
        let sized = [c.center.x, c.center.y, c.size.x, c.size.y]
            .iter()
            .all(|v| v.is_finite())
            && [c.size.x, c.size.y]
                .iter()
                .all(|s| (Lagoon::MIN_SIZE..=Lagoon::MAX_SIZE).contains(s));
        if !sized {
            errors.push(LagoonError::BadSize { name });
            continue;
        }
        let lagoon = Lagoon::new(c, terrain);
        let keep = lagoon.keep_out();
        let rails = line
            .segments()
            .chain(towns.iter().filter_map(|t| t.railway))
            .any(|r| keep.segment_overlap(r.start, r.end).is_some());
        let error = if !Aabb::new(bounds.min, bounds.max).contains(&keep) {
            Some(LagoonError::OutsideWorld { name })
        } else if let Some(town) = towns.iter().find(|t| t.leveled().intersects(&keep)) {
            Some(LagoonError::Town {
                name,
                town: town.name.clone(),
            })
        } else if rails {
            Some(LagoonError::Railway { name })
        } else if roads
            .segments()
            .any(|(a, b)| keep.segment_overlap(a, b).is_some())
        {
            Some(LagoonError::Road { name })
        } else {
            None
        };
        match error {
            Some(error) => errors.push(error),
            None => laid.push(lagoon),
        }
    }
    (laid, errors)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn arocena_config() -> LagoonConfig {
        LagoonConfig {
            name: "Laguna La Arocena".into(),
            center: Vec2::new(3401.0, 841.0),
            size: Vec2::new(215.0, 145.0),
            seed: 5,
        }
    }

    fn arocena() -> (Terrain, Lagoon) {
        let terrain = Terrain::default();
        let lagoon = Lagoon::new(&arocena_config(), &terrain);
        let terrain = terrain.with_lagoon(lagoon.clone());
        (terrain, lagoon)
    }

    #[test]
    fn the_shore_is_a_slightly_rough_rounded_rectangle_and_is_seeded() {
        let (_, lagoon) = arocena();
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for k in 0..360 {
            let angle = (k as f32).to_radians();
            let share = lagoon.shore_radius(angle) / lagoon.rounded_rectangle(angle);
            lo = lo.min(share);
            hi = hi.max(share);
            assert!(lagoon.bounds().contains_point(lagoon.shore(360)[k]));
        }
        // A little irregular, still the rectangle it was configured as.
        assert!(hi - lo > 0.02, "{lo}..{hi}");
        assert!(lo > 0.94 && hi < 1.06, "{lo}..{hi}");
        // Squarish: it fills most of the box around its shore (an ellipse
        // fills 79%).
        let shore = lagoon.shore(720);
        let (min, max) = shore.iter().fold(
            (Vec2::new(f32::MAX, f32::MAX), Vec2::new(f32::MIN, f32::MIN)),
            |(lo, hi), p| {
                (
                    Vec2::new(lo.x.min(p.x), lo.y.min(p.y)),
                    Vec2::new(hi.x.max(p.x), hi.y.max(p.y)),
                )
            },
        );
        let (mut inside, mut total) = (0, 0);
        for i in 0..200 {
            for j in 0..200 {
                let p = Vec2::new(
                    min.x + (max.x - min.x) * (i as f32 + 0.5) / 200.0,
                    min.y + (max.y - min.y) * (j as f32 + 0.5) / 200.0,
                );
                total += 1;
                inside += usize::from(lagoon.is_water(p));
            }
        }
        let fill = inside as f32 / total as f32;
        assert!(fill > 0.85, "{fill}");
        // About 1.5 to 1, longer east-west.
        let aspect = (max.x - min.x) / (max.y - min.y);
        assert!((1.35..1.65).contains(&aspect), "{aspect}");
        assert_eq!(Lagoon::new(&arocena_config(), &Terrain::default()), lagoon);
        let other = LagoonConfig {
            seed: 6,
            ..arocena_config()
        };
        assert_ne!(
            Lagoon::new(&other, &Terrain::default()).shore(8),
            lagoon.shore(8)
        );
    }

    #[test]
    fn only_a_narrow_band_of_mud_is_bare() {
        let (_, lagoon) = arocena();
        for p in lagoon.ring(64, Lagoon::MUD_BAND - 0.5) {
            assert!(lagoon.is_bare(p));
        }
        for p in lagoon.ring(64, Lagoon::MUD_BAND + 0.5) {
            assert!(!lagoon.is_bare(p));
        }
        const { assert!(Lagoon::MUD_BAND <= 5.0) };
    }

    #[test]
    fn the_water_is_flat_below_its_whole_bank_and_deepest_inside() {
        let (terrain, lagoon) = arocena();
        // The bank stands above the water all round: it never spills.
        for p in lagoon.ring(128, 0.5) {
            assert!(terrain.height(p.x, p.y) > lagoon.level, "{p:?}");
        }
        for p in lagoon.ring(128, Lagoon::BANK * 0.5) {
            assert!(terrain.height(p.x, p.y) >= lagoon.level + Lagoon::FREEBOARD * 0.5);
        }
        // The bottom is under the water everywhere inside the shore, and
        // deepest in the middle.
        for p in lagoon.ring(128, -0.5).into_iter().chain([lagoon.center]) {
            let h = terrain.height(p.x, p.y);
            assert!(h < lagoon.level, "{p:?}: {h} vs {}", lagoon.level);
        }
        let middle = terrain.height(lagoon.center.x, lagoon.center.y);
        assert!((lagoon.level - middle - Lagoon::DEPTH).abs() < 1e-3);
        // Past the bank the pampa is untouched.
        for p in lagoon.ring(64, Lagoon::BANK + 1.0) {
            assert_eq!(terrain.height(p.x, p.y), terrain.natural_height(p.x, p.y));
        }
    }

    #[test]
    fn the_basin_is_continuous_and_gentle() {
        let (terrain, lagoon) = arocena();
        let step = 0.25;
        for k in 0..24 {
            let angle = k as f32 * TAU / 24.0;
            let dir = Vec2::new(angle.cos(), angle.sin());
            let reach = lagoon.shore_radius(angle) + Lagoon::BANK + 10.0;
            let mut last = terrain.height(lagoon.center.x, lagoon.center.y);
            let mut r = step;
            while r < reach {
                let p = lagoon.center + dir * r;
                let h = terrain.height(p.x, p.y);
                assert!((h - last).abs() / step < 0.36, "too steep at {p:?}");
                last = h;
                r += step;
            }
        }
    }

    #[test]
    fn deep_water_pushes_a_body_back_to_the_shallows() {
        let (_, lagoon) = arocena();
        assert_eq!(lagoon.push_out(lagoon.center + Vec2::new(500.0, 0.0)), None);
        // Wading the shallows is fine.
        for p in lagoon.ring(64, -Lagoon::WADE + 0.5) {
            assert_eq!(lagoon.push_out(p), None, "{p:?}");
            assert!(lagoon.is_water(p) && !lagoon.is_deep(p));
        }
        for p in lagoon.ring(64, -20.0).into_iter().chain([lagoon.center]) {
            assert!(lagoon.is_deep(p));
            let out = p + lagoon.push_out(p).expect("deep water");
            assert!(!lagoon.is_deep(out), "{out:?}");
            assert!(lagoon.is_water(out));
        }
    }

    #[test]
    fn a_lagoon_on_a_town_off_the_world_or_too_small_is_left_out() {
        use crate::{PlazaPlacement, TownConfig, place_towns};
        let world = WorldConfig::default();
        let town = TownConfig {
            origin: Vec2::new(-1500.0, 0.0),
            plaza: PlazaPlacement::Auto,
            railway: None,
            ..TownConfig::castex()
        };
        let (towns, _) = place_towns(&world, &[town]);
        let (line, _) = RailLine::build(None, &towns, world.bounds());
        let roads = RoadNetwork::new(Vec::new(), &[]);
        let at = |name: &str, x: f32, y: f32, size: f32| LagoonConfig {
            name: name.into(),
            center: Vec2::new(x, y),
            size: Vec2::new(size, size * 0.7),
            seed: 1,
        };
        let configs = [
            at("ok", 1500.0, 0.0, 200.0),
            at("town", -1500.0, 0.0, 200.0),
            at("edge", 3990.0, 0.0, 200.0),
            at("small", 0.0, 1500.0, 10.0),
            at("nan", f32::NAN, 0.0, 200.0),
        ];
        let (laid, errors) =
            place_lagoons(&world, &Terrain::default(), &towns, &line, &roads, &configs);
        assert_eq!(laid.len(), 1);
        assert_eq!(laid[0].name, "ok");
        assert!(matches!(errors[0], LagoonError::Town { .. }), "{errors:?}");
        assert!(matches!(errors[1], LagoonError::OutsideWorld { .. }));
        assert!(matches!(errors[2], LagoonError::BadSize { .. }));
        assert!(matches!(errors[3], LagoonError::BadSize { .. }));
        for e in &errors {
            assert!(e.to_string().starts_with("lagoon "), "{e}");
        }
    }
}
