//! Procedural heightmap shared by logic (ground snapping) and render (mesh).

use std::sync::Arc;

use crate::{RoadNetwork, Vec2};

/// Gently rolling pampa: a few summed sine octaves. Deterministic per seed.
///
/// [`Terrain::height`] is the single source of truth for the natural ground
/// height; both logic and render sample it, including any [`FlatArea`]
/// leveled for a town. Where bodies stand, [`crate::World::ground_height`]
/// adds the raised floors (station platforms) on top of it.
#[derive(Debug, Clone, PartialEq)]
pub struct Terrain {
    /// Peak deviation from zero height, in meters.
    pub amplitude: f32,
    /// Variation seed; changes phases of every octave.
    pub seed: u32,
    /// Ground leveled for settlements, one area per town.
    pub flat_areas: Vec<FlatArea>,
    /// The roads cut into the ground, shared (cloning the terrain is cheap).
    pub roads: Option<Arc<RoadNetwork>>,
}

impl Default for Terrain {
    fn default() -> Self {
        Self {
            amplitude: 2.5,
            seed: 1927,
            flat_areas: Vec::new(),
            roads: None,
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
    /// This terrain with `area` leveled too.
    pub fn with_flat_area(mut self, area: FlatArea) -> Self {
        self.flat_areas.push(area);
        self
    }

    /// This terrain with `roads` cut into it (see [`RoadNetwork::cut`]).
    pub fn with_roads(mut self, roads: Arc<RoadNetwork>) -> Self {
        self.roads = Some(roads);
        self
    }

    /// Ground height at `(x, y)`, including any leveled area.
    ///
    /// Where bands overlap, the leveled heights are averaged by weight and
    /// eased in by the strongest weight, so the ground stays continuous
    /// everywhere (world validation keeps town bands apart, so in practice
    /// at most one area touches any point). Roads are cut into that ground:
    /// a shallow bed, level across, easing back into the land beside it.
    pub fn height(&self, x: f32, y: f32) -> f32 {
        let ground = self.leveled_height(x, y);
        let Some(roads) = &self.roads else {
            return ground;
        };
        match roads.cut(Vec2::new(x, y), |p| self.leveled_height(p.x, p.y)) {
            Some((strength, bed)) => ground + (bed - ground) * strength,
            None => ground,
        }
    }

    /// Ground height at `(x, y)` with the towns leveled, before roads.
    fn leveled_height(&self, x: f32, y: f32) -> f32 {
        let natural = self.natural_height(x, y);
        let p = Vec2::new(x, y);
        let (mut strongest, mut total, mut weighted) = (0.0_f32, 0.0, 0.0);
        for area in &self.flat_areas {
            let w = area.weight(p);
            if w <= 0.0 {
                continue;
            }
            strongest = strongest.max(w);
            total += w;
            weighted += w * area.height;
        }
        if strongest <= 0.0 {
            return natural;
        }
        let leveled = weighted / total;
        if strongest >= 1.0 {
            return leveled;
        }
        natural + (leveled - natural) * strongest
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
        let b = Terrain {
            seed: 7,
            ..a.clone()
        };
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

    fn two_towns() -> (Terrain, FlatArea, FlatArea) {
        let a = FlatArea {
            center: Vec2::new(0.0, 0.0),
            half_extents: Vec2::new(100.0, 120.0),
            falloff: 60.0,
            height: 1.5,
        };
        let b = FlatArea {
            center: Vec2::new(2500.0, -1800.0),
            half_extents: Vec2::new(80.0, 80.0),
            falloff: 60.0,
            height: -2.0,
        };
        let t = Terrain::default().with_flat_area(a).with_flat_area(b);
        (t, a, b)
    }

    #[test]
    fn every_flat_area_levels_its_own_ground() {
        let (t, a, b) = two_towns();
        assert_eq!(t.flat_areas, vec![a, b]);
        assert_eq!(t.height(0.0, 0.0), a.height);
        assert_eq!(t.height(-99.0, 119.0), a.height);
        assert_eq!(t.height(2500.0, -1800.0), b.height);
        assert_eq!(t.height(2579.0, -1879.0), b.height);
        // Between them the pampa is untouched.
        for (x, y) in [(1200.0, -900.0), (2500.0, -1650.0), (300.0, 0.0)] {
            assert_eq!(t.height(x, y), t.natural_height(x, y), "({x}, {y})");
        }
    }

    #[test]
    fn overlapping_bands_blend_continuously() {
        // Two areas at different heights whose bands overlap: no step.
        let a = FlatArea {
            center: Vec2::new(-60.0, 0.0),
            half_extents: Vec2::new(30.0, 30.0),
            falloff: 60.0,
            height: 2.0,
        };
        let b = FlatArea {
            center: Vec2::new(60.0, 10.0),
            height: -2.0,
            ..a
        };
        let t = Terrain::default().with_flat_area(a).with_flat_area(b);
        assert_eq!(t.height(-60.0, 0.0), 2.0);
        assert_eq!(t.height(60.0, 10.0), -2.0);
        let step = 0.05;
        for j in [-40.0, 0.0, 5.0, 25.0] {
            let mut last = t.height(-120.0, j);
            for i in 1..4800 {
                let h = t.height(-120.0 + i as f32 * step, j);
                assert!((h - last).abs() < 0.05, "step at x={}", i as f32 * step);
                assert!(h.abs() <= t.amplitude + 1e-4);
                last = h;
            }
        }
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

    fn with_shipped_roads() -> Terrain {
        let (_, _, roads) = crate::road::tests::shipped_roads();
        Terrain::default().with_roads(Arc::new(roads))
    }

    #[test]
    fn a_road_is_cut_level_across_and_continuous() {
        let t = with_shipped_roads();
        let roads = t.roads.clone().unwrap();
        let plain = Terrain::default();
        let (mut flatter, mut checked) = (0, 0);
        for (a, b) in roads.segments().step_by(23) {
            let mid = (a + b) * 0.5;
            let d = b - a;
            let left = Vec2::new(-d.y, d.x) * (1.0 / d.length());
            // Mid-road (away from its ends), the bed is a little below the
            // land at the centerline, and level across the road.
            let center = t.height(mid.x, mid.y);
            let depth = plain.height(mid.x, mid.y) - center;
            let across = |t: &Terrain, s: f32| {
                let p = mid + left * s;
                t.height(p.x, p.y)
            };
            let spread = |t: &Terrain| {
                let hs: Vec<f32> = (-4..=4)
                    .map(|k| across(t, k as f32 * RoadNetwork::HALF_WIDTH / 4.0))
                    .collect();
                hs.iter().fold(f32::MIN, |m, h| m.max(*h))
                    - hs.iter().fold(f32::MAX, |m, h| m.min(*h))
            };
            if roads.near(mid, RoadNetwork::CUT_REACH * 3.0).is_some() && depth > 0.0 {
                assert!(spread(&t) < 0.02, "{mid:?}: {}", spread(&t));
                assert!(depth <= RoadNetwork::CUT_DEPTH + 0.01, "{depth}");
                flatter += usize::from(spread(&t) < spread(&plain));
                checked += 1;
            }
            // Continuous: no step walking across the road and its banks.
            let mut last = across(&t, -15.0);
            for k in 1..=600 {
                let h = across(&t, -15.0 + k as f32 * 0.05);
                assert!((h - last).abs() < 0.02, "step at {mid:?}");
                last = h;
            }
            // Past the banks the land is untouched.
            for s in [-RoadNetwork::CUT_REACH - 1.0, RoadNetwork::CUT_REACH + 1.0] {
                let p = mid + left * s;
                if roads.distance(p) > RoadNetwork::CUT_REACH {
                    assert_eq!(t.height(p.x, p.y), plain.height(p.x, p.y));
                }
            }
        }
        assert!(checked > 100, "{checked}");
        assert!(flatter * 10 > checked * 9, "{flatter} of {checked}");
        // Far from every road the terrain is unchanged.
        assert_eq!(t.height(7000.0, -7000.0), plain.height(7000.0, -7000.0));
    }

    #[test]
    fn the_road_cut_leaves_the_ground_under_the_track_untouched_at_every_crossing() {
        let (towns, line, roads) = crate::road::tests::shipped_roads();
        let crossings: Vec<Vec2> = roads.crossings().map(|c| c.at).collect();
        assert!(!crossings.is_empty());
        let t = Terrain::default().with_roads(Arc::new(roads));
        let plain = Terrain::default();
        let path = line.full_path(&towns);
        let mut checked = 0;
        for w in path.points().windows(2) {
            let (a, b) = (w[0], w[1]);
            let length = a.distance(b);
            let dir = (b - a) * (1.0 / length);
            let left = Vec2::new(-dir.y, dir.x);
            let steps = (length / 0.5).ceil() as usize;
            for k in 0..=steps {
                let on = a + (b - a) * (k as f32 / steps as f32);
                if crossings.iter().all(|c| c.distance(on) > 60.0) {
                    continue;
                }
                // The track bed: sleepers and ballast either side.
                for s in [-2.5, -1.5, -0.5, 0.0, 0.5, 1.5, 2.5] {
                    let p = on + left * s;
                    assert_eq!(t.height(p.x, p.y), plain.height(p.x, p.y), "at {p:?}");
                    checked += 1;
                }
            }
        }
        assert!(checked > 1000, "{checked}");
        // The road is still cut just past the track bed and continuous
        // walking along the road across the track.
        for c in t.roads.clone().unwrap().crossings() {
            let mut last = t.height(c.at.x - c.road.x * 30.0, c.at.y - c.road.y * 30.0);
            for k in 1..=1200 {
                let p = c.at + c.road * (-30.0 + k as f32 * 0.05);
                let h = t.height(p.x, p.y);
                assert!((h - last).abs() < 0.02, "step at {p:?}");
                last = h;
            }
            let p = c.at + c.road * 20.0;
            assert!(
                t.height(p.x, p.y) < plain.height(p.x, p.y),
                "no cut at {p:?}"
            );
        }
    }
}
