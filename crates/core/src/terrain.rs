//! Procedural heightmap shared by logic (ground snapping) and render (mesh).

use crate::Vec2;

/// Gently rolling pampa: a few summed sine octaves. Deterministic per seed.
///
/// [`Terrain::height`] is the single source of truth for ground height; both
/// logic and render sample it, including any [`FlatArea`] leveled for a town.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Terrain {
    /// Peak deviation from zero height, in meters.
    pub amplitude: f32,
    /// Variation seed; changes phases of every octave.
    pub seed: u32,
    /// Ground leveled for a settlement, if any.
    pub flat_area: Option<FlatArea>,
}

impl Default for Terrain {
    fn default() -> Self {
        Self {
            amplitude: 2.5,
            seed: 1927,
            flat_area: None,
        }
    }
}

/// A rectangle of ground leveled to one height, blending smoothly back into
/// the natural terrain over `falloff` meters around it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlatArea {
    pub center: Vec2,
    /// Half width (`x`) and half depth (`y`) of the fully flat core.
    pub half_extents: Vec2,
    /// Width of the blend band around the core, in meters.
    pub falloff: f32,
    /// Height of the leveled ground.
    pub height: f32,
}

impl FlatArea {
    /// How strongly this area levels the ground at `p`: `1` on the flat core,
    /// easing to `0` at `falloff` meters outside it.
    pub fn weight(&self, p: Vec2) -> f32 {
        let dx = ((p.x - self.center.x).abs() - self.half_extents.x).max(0.0);
        let dy = ((p.y - self.center.y).abs() - self.half_extents.y).max(0.0);
        let outside = dx.hypot(dy);
        if outside <= 0.0 {
            return 1.0;
        }
        if self.falloff <= 0.0 {
            return 0.0;
        }
        let t = (1.0 - outside / self.falloff).clamp(0.0, 1.0);
        // Smoothstep: no crease where the flat core meets the slope.
        t * t * (3.0 - 2.0 * t)
    }
}

impl Terrain {
    /// This terrain with `area` leveled.
    pub fn with_flat_area(self, area: FlatArea) -> Self {
        Self {
            flat_area: Some(area),
            ..self
        }
    }

    /// Ground height at `(x, y)`, including any leveled area.
    pub fn height(&self, x: f32, y: f32) -> f32 {
        let natural = self.natural_height(x, y);
        match self.flat_area {
            Some(area) => {
                let w = area.weight(Vec2::new(x, y));
                if w >= 1.0 {
                    area.height
                } else {
                    natural + (area.height - natural) * w
                }
            }
            None => natural,
        }
    }

    /// Height of the untouched rolling pampa at `(x, y)`.
    pub fn natural_height(&self, x: f32, y: f32) -> f32 {
        let mut sum = 0.0;
        let mut weights = 0.0;
        for (i, &(wavelength, weight)) in OCTAVES.iter().enumerate() {
            let k = std::f32::consts::TAU / wavelength;
            let r = hash(self.seed.wrapping_mul(31).wrapping_add(i as u32));
            let (a1, a2) = (unit(r) * TAU_F, unit(r >> 8) * TAU_F);
            let (p1, p2) = (unit(r >> 16) * TAU_F, unit(r.rotate_left(13)) * TAU_F);
            let u = x * a1.cos() + y * a1.sin();
            let v = x * a2.cos() + y * a2.sin();
            sum += weight * (u * k + p1).sin() * (v * k * 0.7 + p2).cos();
            weights += weight;
        }
        self.amplitude * sum / weights
    }
}

const TAU_F: f32 = std::f32::consts::TAU;

/// `(wavelength in meters, relative weight)`, from broad swells to small bumps.
/// Kept long and shallow so slopes stay gentle.
const OCTAVES: [(f32, f32); 3] = [(180.0, 0.6), (70.0, 0.3), (30.0, 0.1)];

/// Small integer hash (lowbias32) so the seed scrambles every octave.
fn hash(mut v: u32) -> u32 {
    v ^= v >> 16;
    v = v.wrapping_mul(0x7feb_352d);
    v ^= v >> 15;
    v = v.wrapping_mul(0x846c_a68b);
    v ^ (v >> 16)
}

/// Low 8 bits of `v` mapped to `[0, 1)`.
fn unit(v: u32) -> f32 {
    (v & 0xff) as f32 / 256.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_deterministic() {
        let t = Terrain::default();
        assert_eq!(t.height(12.3, -45.6), t.height(12.3, -45.6));
    }

    #[test]
    fn stays_within_amplitude_and_is_not_flat() {
        let t = Terrain::default();
        let (mut min, mut max) = (f32::MAX, f32::MIN);
        for i in -50..50 {
            for j in -50..50 {
                let h = t.height(i as f32 * 4.0, j as f32 * 4.0);
                assert!(h.abs() <= t.amplitude + 1e-4, "height {h} out of range");
                min = min.min(h);
                max = max.max(h);
            }
        }
        assert!(
            max - min > 0.5,
            "terrain should roll, got range {}",
            max - min
        );
    }

    #[test]
    fn is_gentle() {
        // The pampa is mostly flat: no slope steeper than ~20 degrees.
        let t = Terrain::default();
        let step = 0.5;
        for i in -100..100 {
            let x = i as f32 * 1.7;
            let y = i as f32 * -2.3;
            let dx = (t.height(x + step, y) - t.height(x, y)) / step;
            let dy = (t.height(x, y + step) - t.height(x, y)) / step;
            assert!(dx.hypot(dy) < 0.36, "slope too steep at ({x}, {y})");
        }
    }

    #[test]
    fn seed_changes_shape() {
        let a = Terrain::default();
        let b = Terrain { seed: 7, ..a };
        assert_ne!(a.height(10.0, 20.0), b.height(10.0, 20.0));
    }

    fn leveled() -> (Terrain, FlatArea) {
        let area = FlatArea {
            center: Vec2::new(10.0, -5.0),
            half_extents: Vec2::new(40.0, 30.0),
            falloff: 50.0,
            height: 1.0,
        };
        (Terrain::default().with_flat_area(area), area)
    }

    #[test]
    fn unleveled_height_is_natural() {
        let t = Terrain::default();
        assert_eq!(t.height(12.0, 34.0), t.natural_height(12.0, 34.0));
    }

    #[test]
    fn flat_area_core_is_level() {
        let (t, area) = leveled();
        for (x, y) in [(10.0, -5.0), (-29.0, 24.0), (49.9, -34.9), (0.0, 0.0)] {
            assert_eq!(t.height(x, y), area.height, "at ({x}, {y})");
        }
    }

    #[test]
    fn flat_area_blends_back_into_natural_terrain() {
        let (t, area) = leveled();
        // Beyond the falloff band the pampa is untouched.
        for (x, y) in [(120.0, -5.0), (10.0, 100.0), (-200.0, -200.0)] {
            assert_eq!(t.height(x, y), t.natural_height(x, y));
        }
        // Weight eases monotonically from 1 to 0 across the band.
        let mut last = 1.0;
        for i in 0..=50 {
            let w = area.weight(Vec2::new(50.0 + i as f32, -5.0));
            assert!(w <= last + 1e-6 && (0.0..=1.0).contains(&w));
            last = w;
        }
        assert_eq!(last, 0.0);
    }

    #[test]
    fn leveled_terrain_stays_gentle_and_in_range() {
        let (t, _) = leveled();
        let step = 0.5;
        for i in -150..150 {
            for j in [-80.0, -40.0, -5.0, 30.0, 70.0] {
                let (x, y) = (i as f32, j);
                let h = t.height(x, y);
                assert!(h.abs() <= t.amplitude + 1e-4);
                let dx = (t.height(x + step, y) - h) / step;
                let dy = (t.height(x, y + step) - h) / step;
                assert!(dx.hypot(dy) < 0.36, "slope too steep at ({x}, {y})");
            }
        }
    }
}
