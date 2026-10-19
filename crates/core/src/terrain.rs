//! Procedural heightmap shared by logic (ground snapping) and render (mesh).

/// Gently rolling pampa: a few summed sine octaves. Deterministic per seed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Terrain {
    /// Peak deviation from zero height, in meters.
    pub amplitude: f32,
    /// Variation seed; changes phases of every octave.
    pub seed: u32,
}

impl Default for Terrain {
    fn default() -> Self {
        Self {
            amplitude: 2.5,
            seed: 1927,
        }
    }
}

impl Terrain {
    /// Ground height at ground-plane coordinates `(x, y)`.
    pub fn height(&self, x: f32, y: f32) -> f32 {
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
}
