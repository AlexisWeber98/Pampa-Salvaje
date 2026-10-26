//! The plaza trees' meshes: a tapered trunk and limbs of bark, and leaf
//! cards hung at the branch tips, seeded per species and variant.
//!
//! A tree is two meshes, bark and leaves, built once per variant at
//! startup and shared by every tree of that variant (each placed with its
//! own yaw and scale). The trunk, the limbs and the twigs are open tubes
//! of a few sides with smooth radial normals and UVs that repeat the bark
//! every [`Form::bark_tile`] meters, a whole number of times round each
//! tube so its seam does not show. A card is one sprig of the species'
//! leaf atlas (a quarter of it, see `tools/leaf_atlas.py`), hung on a
//! tip by the sprig's base and bent down a little under its own weight.
//! Its normals lean out from the crown's middle, so the crown shades as
//! one mass rather than as scattered flat cards.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use super::Species;
use crate::ground::face_tangent;

/// Where a sprig grows from in its quarter of the leaf atlas, as a
/// fraction of the quarter's height up from its bottom edge
/// (`leaf_atlas.py`'s `BASE`: 32 of 512 texels).
pub const SPRIG_BASE: f32 = 32.0 / 512.0;

/// The shape of a species: its trunk, limbs and leaf cards, laid out from
/// [`Form::height`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct Form {
    /// Height the limbs are laid out for, meters (min, max): a scale for
    /// the fork, the limbs' length and the leader, not the built tree's
    /// height. That is measured from the geometry into
    /// [`TreeMesh::height`] and comes out lower where the limbs lean out
    /// (the aromo's is about 4.6 m against a layout height of 6.5–7.5).
    pub height: (f32, f32),
    /// Where the limbs part from the trunk, a fraction of the height.
    pub fork: f32,
    /// The trunk's radius at the ground, meters.
    pub trunk_radius: f32,
    /// Limbs (min, max).
    pub limbs: (u32, u32),
    /// A limb's lean from upright, degrees (min, max).
    pub tilt: (f32, f32),
    /// A limb's length, a fraction of the height.
    pub reach: f32,
    /// How far a limb's outer half bends up (or down, below zero), a
    /// fraction of its length.
    pub rise: f32,
    /// Twigs off each limb.
    pub twigs: u32,
    /// The trunk goes on up through the limbs to the top (eucalipto).
    pub leader: bool,
    /// Leaf cards on the whole tree.
    pub cards: u32,
    /// A card's side, meters.
    pub card: f32,
    /// 0: cards reach out and up from their branch; 1: they hang
    /// straight down.
    pub hang: f32,
    /// How far a card's tip bends down, a fraction of its side.
    pub sag: f32,
    /// How far the cards scatter round their branch, a fraction of a card.
    pub scatter: f32,
    /// Meters per bark repeat, round the tube and along it.
    pub bark_tile: Vec2,
}

impl Species {
    /// Its shape trimmed for a street: a trunk bare to about 3 m, then a
    /// fuller crown held close to the trunk (limbs part about 3.3 m up and
    /// lean little), with the species' own bark, leaves and bearing.
    pub(crate) fn street_form(self) -> Form {
        let form = self.form();
        let (tilt, reach) = match self {
            Self::Paraiso => ((24.0, 34.0), 0.15),
            Self::Olmo => ((12.0, 20.0), 0.13),
            _ => ((10.0, 16.0), 0.12),
        };
        Form {
            height: (8.5, 10.0),
            fork: 0.4,
            tilt,
            reach,
            rise: 0.1,
            twigs: 3,
            leader: false,
            cards: form.cards,
            card: form.card * 0.9,
            hang: form.hang.min(0.4),
            scatter: 0.2,
            ..form
        }
    }

    /// Its shape: the paraíso's round umbrella, the olmo's tall vase, the
    /// eucalipto's tall open crown of hanging leaves, the aromo's low
    /// feathery one.
    pub(crate) fn form(self) -> Form {
        match self {
            Self::Paraiso => Form {
                height: (7.5, 8.5),
                fork: 0.45,
                trunk_radius: 0.2,
                limbs: (5, 6),
                tilt: (55.0, 70.0),
                reach: 0.3,
                rise: 0.15,
                twigs: 3,
                leader: false,
                cards: 120,
                card: 1.5,
                hang: 0.1,
                sag: 0.12,
                scatter: 0.3,
                bark_tile: Vec2::new(1.0, 1.0),
            },
            Self::Olmo => Form {
                height: (11.0, 13.0),
                fork: 0.33,
                trunk_radius: 0.28,
                limbs: (4, 5),
                tilt: (15.0, 25.0),
                reach: 0.5,
                rise: 0.08,
                twigs: 4,
                leader: false,
                cards: 140,
                card: 1.6,
                hang: 0.15,
                sag: 0.15,
                scatter: 0.3,
                // Bark001 is twice as tall as it is wide.
                bark_tile: Vec2::new(0.8, 1.6),
            },
            Self::Eucalipto => Form {
                height: (14.0, 17.0),
                fork: 0.45,
                trunk_radius: 0.3,
                limbs: (6, 7),
                tilt: (30.0, 50.0),
                reach: 0.2,
                rise: -0.12,
                twigs: 3,
                leader: true,
                cards: 150,
                card: 1.6,
                hang: 0.7,
                sag: 0.1,
                scatter: 0.3,
                bark_tile: Vec2::new(1.2, 1.2),
            },
            Self::Aromo => Form {
                height: (6.5, 7.5),
                fork: 0.24,
                trunk_radius: 0.15,
                limbs: (5, 6),
                tilt: (40.0, 60.0),
                reach: 0.32,
                rise: 0.12,
                twigs: 3,
                leader: false,
                cards: 130,
                card: 1.3,
                hang: 0.2,
                sag: 0.2,
                scatter: 0.3,
                bark_tile: Vec2::new(0.8, 0.8),
            },
        }
    }
}

/// Sides of the trunk, of a limb and of a twig.
const TRUNK_SIDES: u32 = 8;
const LIMB_SIDES: u32 = 6;
const TWIG_SIDES: u32 = 3;

/// One mesh's vertices, before it becomes a [`Mesh`].
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Buffers {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

impl Buffers {
    #[cfg(test)]
    pub fn triangles(&self) -> usize {
        self.indices.len() / 3
    }

    fn vertex(&mut self, p: Vec3, n: Vec3, uv: [f32; 2]) -> u32 {
        self.positions.push(p);
        self.normals.push(n);
        self.uvs.push(uv);
        (self.positions.len() - 1) as u32
    }

    /// Two triangles over corners `a b c d`, counter-clockwise.
    fn quad(&mut self, [a, b, c, d]: [u32; 4]) {
        self.indices.extend([a, b, c, a, c, d]);
    }

    /// Per-vertex tangents for the normal maps: each triangle's UV tangent,
    /// summed round its vertices and squared against their normals.
    pub fn tangents(&self) -> Vec<[f32; 4]> {
        let mut sum = vec![Vec3::ZERO; self.positions.len()];
        let mut sign = vec![0.0f32; self.positions.len()];
        for t in self.indices.chunks(3) {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| i as usize);
            let f = face_tangent(
                [self.positions[a], self.positions[b], self.positions[c]],
                [self.uvs[a], self.uvs[b], self.uvs[c]],
            );
            for v in [a, b, c] {
                sum[v] += Vec3::new(f[0], f[1], f[2]);
                sign[v] += f[3];
            }
        }
        sum.iter()
            .zip(&sign)
            .zip(&self.normals)
            .map(|((t, w), n)| {
                let t = (*t - *n * n.dot(*t)).normalize_or(n.any_orthonormal_vector());
                [t.x, t.y, t.z, if *w < 0.0 { -1.0 } else { 1.0 }]
            })
            .collect()
    }

    pub fn into_mesh(self) -> Mesh {
        let tangents = self.tangents();
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_POSITION,
            self.positions
                .iter()
                .map(|p| p.to_array())
                .collect::<Vec<_>>(),
        )
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_NORMAL,
            self.normals
                .iter()
                .map(|n| n.to_array())
                .collect::<Vec<_>>(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_TANGENT, tangents)
        .with_inserted_indices(Indices::U32(self.indices))
    }
}

/// One built tree: its bark and its leaves, standing at the origin.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TreeMesh {
    pub bark: Buffers,
    pub leaves: Buffers,
    /// Its top, meters over the ground.
    pub height: f32,
    /// How far its leaves reach from the trunk's axis, meters.
    pub crown_radius: f32,
    /// How low its lowest leaf hangs over the ground, meters.
    pub crown_base: f32,
}

/// A small seeded generator (SplitMix64): the same seed, the same tree.
struct Rng(u64);

impl Rng {
    fn new(species: Species, seed: u32) -> Self {
        Self(u64::from(seed) << 8 | species as u64)
    }

    fn unit(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, (lo, hi): (f32, f32)) -> f32 {
        lo + (hi - lo) * self.unit()
    }

    /// A point in the unit ball.
    fn ball(&mut self) -> Vec3 {
        loop {
            let p = Vec3::new(self.unit(), self.unit(), self.unit()) * 2.0 - 1.0;
            if p.length_squared() <= 1.0 {
                return p;
            }
        }
    }
}

/// A horizontal unit vector `angle` radians round from +X.
fn heading(angle: f32) -> Vec3 {
    Vec3::new(angle.cos(), 0.0, angle.sin())
}

/// An open tube through `points` with `radii`, `sides` round, its bark
/// repeating every `tile` meters (round, along).
fn tube(out: &mut Buffers, points: &[Vec3], radii: &[f32], sides: u32, tile: Vec2) {
    let wraps = (std::f32::consts::TAU * radii[0] / tile.x).round().max(1.0);
    let mut side = Vec3::ZERO;
    let mut along = 0.0;
    let mut rings = Vec::new();
    for (i, (&p, &r)) in points.iter().zip(radii).enumerate() {
        let ahead = points[(i + 1).min(points.len() - 1)] - points[i.saturating_sub(1)];
        let dir = ahead.normalize_or(Vec3::Y);
        // Carry the ring's start round the bends, so the tube never twists.
        side = (side - dir * dir.dot(side)).normalize_or(dir.any_orthonormal_vector());
        let up = dir.cross(side);
        if i > 0 {
            along += p.distance(points[i - 1]);
        }
        let ring: Vec<u32> = (0..=sides)
            .map(|k| {
                let a = std::f32::consts::TAU * k as f32 / sides as f32;
                let n = side * a.cos() + up * a.sin();
                out.vertex(
                    p + n * r,
                    n,
                    [wraps * k as f32 / sides as f32, along / tile.y],
                )
            })
            .collect();
        rings.push(ring);
    }
    for pair in rings.windows(2) {
        for k in 0..sides as usize {
            // Counter-clockwise seen from outside: up the tube, then round.
            out.quad([pair[0][k], pair[0][k + 1], pair[1][k + 1], pair[1][k]]);
        }
    }
}

/// A leafy stretch of branch: the outer half of a limb, a twig, the top
/// of a leader. Cards hang along it, more of them toward its end.
struct Leafy {
    from: Vec3,
    to: Vec3,
}

/// Builds variant `seed` of `species`.
pub(crate) fn tree(species: Species, seed: u32) -> TreeMesh {
    grow(species.form(), Rng::new(species, seed))
}

/// Builds variant `seed` of `species` trimmed for a street ([`Species::street_form`]).
pub(crate) fn street_tree(species: Species, seed: u32) -> TreeMesh {
    grow(species.street_form(), Rng::new(species, seed ^ 0x57ee))
}

/// Grows a tree of `form`, drawing its randomness from `rng`.
fn grow(form: Form, mut rng: Rng) -> TreeMesh {
    let height = rng.range(form.height);
    let mut bark = Buffers::default();
    let mut leafy = Vec::new();
    let r0 = form.trunk_radius;
    let lean = heading(rng.range((0.0, std::f32::consts::TAU))) * 0.04 * height;
    let fork = form.fork * height;
    // The trunk, flared a little at the foot, leaning a little.
    let top = if form.leader { 0.92 * height } else { fork };
    let steps = if form.leader { 4 } else { 2 };
    let trunk: Vec<Vec3> = (0..=steps)
        .map(|k| {
            let t = k as f32 / steps as f32;
            Vec3::Y * top * t + lean * t * t * (fork / top)
        })
        .collect();
    let radii: Vec<f32> = (0..=steps)
        .map(|k| {
            let t = k as f32 / steps as f32;
            if form.leader {
                r0 * (1.0 - 0.85 * t)
            } else {
                r0 * (1.0 - 0.3 * t)
            }
        })
        .collect();
    tube(&mut bark, &trunk, &radii, TRUNK_SIDES, form.bark_tile);
    if form.leader {
        leafy.push(Leafy {
            from: trunk[steps - 1],
            to: trunk[steps],
        });
    }
    let on_trunk = |h: f32| {
        let t = (h / top).clamp(0.0, 1.0);
        Vec3::Y * h + lean * t * t * (fork / top)
    };
    let limbs = form.limbs.0 + (rng.unit() * (form.limbs.1 - form.limbs.0 + 1) as f32) as u32;
    let limbs = limbs.min(form.limbs.1);
    let turn0 = rng.range((0.0, std::f32::consts::TAU));
    for i in 0..limbs {
        let around =
            turn0 + std::f32::consts::TAU * (i as f32 + rng.range((-0.2, 0.2))) / limbs as f32;
        let out = heading(around);
        let tilt = rng.range(form.tilt).to_radians();
        let length = form.reach * height * rng.range((0.85, 1.1));
        // Limbs part round the fork; a leader's climb its upper half.
        let h = if form.leader {
            fork + (top - fork) * 0.8 * (i as f32 + rng.unit()) / limbs as f32
        } else {
            fork * rng.range((0.9, 1.05))
        };
        let start = on_trunk(h);
        let dir = out * tilt.sin() + Vec3::Y * tilt.cos();
        let mid = start + dir * length * 0.5;
        let end_dir = (dir + Vec3::Y * form.rise * 2.0).normalize();
        let end = mid + end_dir * length * 0.5;
        let width = (r0 * 0.55).min(r0 * 0.5 + 0.03);
        tube(
            &mut bark,
            &[start - dir * width, mid, end],
            &[width, width * 0.55, 0.035],
            LIMB_SIDES,
            form.bark_tile,
        );
        leafy.push(Leafy { from: mid, to: end });
        for j in 0..form.twigs {
            let t = 0.35 + 0.6 * (j as f32 + rng.unit()) / form.twigs as f32;
            let from = if t < 0.5 {
                start.lerp(mid, t * 2.0)
            } else {
                mid.lerp(end, t * 2.0 - 1.0)
            };
            let side = if j % 2 == 0 { 1.0 } else { -1.0 };
            let twig_dir = (end_dir
                + heading(around + side * rng.range((0.7, 1.1))) * 0.9
                + Vec3::Y * rng.range((-0.1, 0.4)))
            .normalize();
            let twig = length * rng.range((0.32, 0.45));
            let to = from + twig_dir * twig;
            tube(
                &mut bark,
                &[from, to],
                &[0.03, 0.012],
                TWIG_SIDES,
                form.bark_tile,
            );
            leafy.push(Leafy { from, to });
        }
    }
    let leaves = cards(&form, &leafy, &mut rng);
    let height = bark
        .positions
        .iter()
        .chain(&leaves.positions)
        .map(|p| p.y)
        .fold(0.0, f32::max);
    let crown_radius = leaves
        .positions
        .iter()
        .map(|p| Vec2::new(p.x, p.z).length())
        .fold(0.0, f32::max);
    let crown_base = leaves
        .positions
        .iter()
        .map(|p| p.y)
        .fold(f32::INFINITY, f32::min);
    TreeMesh {
        bark,
        leaves,
        height,
        crown_radius,
        crown_base,
    }
}

/// The leaf cards: [`Form::cards`] of them along the `leafy` stretches,
/// each one sprig of the atlas.
fn cards(form: &Form, leafy: &[Leafy], rng: &mut Rng) -> Buffers {
    let mut out = Buffers::default();
    let ends = leafy.iter().map(|l| l.to).sum::<Vec3>() / leafy.len() as f32;
    // The crown's middle: under its ends, at its own height.
    let centre = Vec3::new(0.0, ends.y * 0.9, 0.0);
    let total: f32 = leafy.iter().map(|l| l.from.distance(l.to)).sum();
    let side = form.card;
    for _ in 0..form.cards {
        // A stretch by its length, then a spot on it, mostly toward its end.
        let mut pick = rng.unit() * total;
        let stretch = leafy
            .iter()
            .find(|l| {
                pick -= l.from.distance(l.to);
                pick <= 0.0
            })
            .unwrap_or(&leafy[leafy.len() - 1]);
        let t = 0.2 + 0.8 * rng.unit().sqrt();
        let at = stretch.from.lerp(stretch.to, t);
        let dir = (stretch.to - stretch.from).normalize_or(Vec3::Y);
        let outward = (at - centre).normalize_or(dir);
        {
            // Which way the sprig grows from its base: out along the
            // branch and away from the crown, or hanging.
            let reach = (dir + outward + rng.ball() * 0.9 + Vec3::Y * 0.3).normalize_or(Vec3::Y);
            let up = reach.lerp(Vec3::NEG_Y, form.hang).normalize_or(Vec3::NEG_Y);
            let right = up
                .cross(rng.ball() + Vec3::new(0.01, 0.0, 0.0))
                .normalize_or(up.any_orthonormal_vector());
            let face = right.cross(up);
            let base = at + rng.ball() * form.scatter * side + outward * side * 0.1;
            let bottom = base - up * SPRIG_BASE * side;
            let quarter = (rng.unit() * 4.0) as u32 % 4;
            let (u0, v0) = ((quarter % 2) as f32 * 0.5, (quarter / 2) as f32 * 0.5);
            let mut rows = [[0u32; 2]; 3];
            for (r, row) in rows.iter_mut().enumerate() {
                let t = r as f32 / 2.0;
                for (s, slot) in row.iter_mut().enumerate() {
                    let across = s as f32 - 0.5;
                    let p = bottom + right * across * side + up * t * side
                        - Vec3::Y * form.sag * side * t * t;
                    // Lean the normal out from the crown's middle.
                    let facing = if face.dot(p - centre) < 0.0 {
                        -face
                    } else {
                        face
                    };
                    let n = (facing * 0.4 + (p - centre).normalize_or(Vec3::Y)).normalize();
                    *slot = out.vertex(p, n, [u0 + s as f32 * 0.5, v0 + (1.0 - t) * 0.5]);
                }
            }
            for k in 0..2 {
                let ([a, b], [d, c]) = (rows[k], rows[k + 1]);
                out.quad([a, b, c, d]);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::VARIANTS;

    fn all() -> Vec<(Species, u32, TreeMesh)> {
        Species::ALL
            .iter()
            .flat_map(|&s| (0..VARIANTS as u32).map(move |v| (s, v, tree(s, v))))
            .collect()
    }

    #[test]
    fn every_tree_keeps_to_the_triangle_budget() {
        for (species, v, t) in all() {
            let (bark, leaves) = (t.bark.triangles(), t.leaves.triangles());
            println!("{species:?} {v}: {bark} bark + {leaves} leaves");
            assert!(bark + leaves <= 1000, "{species:?} {v}: {bark} + {leaves}");
            assert!(bark + leaves >= 500, "{species:?} {v}: {bark} + {leaves}");
            assert!(bark >= 100 && leaves >= 300, "{species:?} {v}");
            for b in [&t.bark, &t.leaves] {
                let n = b.positions.len();
                assert_eq!(b.normals.len(), n);
                assert_eq!(b.uvs.len(), n);
                assert!(b.indices.iter().all(|&i| (i as usize) < n));
            }
        }
    }

    #[test]
    fn a_variant_is_its_seed_and_the_seeds_differ() {
        for &species in &Species::ALL {
            assert_eq!(tree(species, 1), tree(species, 1));
            assert_ne!(tree(species, 1).leaves, tree(species, 2).leaves);
        }
    }

    #[test]
    fn the_trunk_stands_within_its_collider() {
        for (species, v, t) in all() {
            for p in t.bark.positions.iter().filter(|p| p.y < 0.5) {
                let r = Vec2::new(p.x, p.z).length();
                // As wide as a plaza ever draws it.
                assert!(
                    r * crate::town::TREE_SCALE_MAX <= pampa_core::town::TREE_TRUNK_RADIUS,
                    "{species:?} {v}: {r}"
                );
            }
            // And the bark reaches the ground.
            let foot = t.bark.positions.iter().map(|p| p.y).fold(1.0, f32::min);
            assert!(foot.abs() < 0.02, "{species:?} {v}: {foot}");
        }
    }

    #[test]
    fn each_species_keeps_its_shape() {
        let mean = |s: Species, f: fn(&TreeMesh) -> f32| {
            (0..VARIANTS as u32).map(|v| f(&tree(s, v))).sum::<f32>() / VARIANTS as f32
        };
        let h = |s| mean(s, |t| t.height);
        let w = |s| mean(s, |t| 2.0 * t.crown_radius);
        for s in Species::ALL {
            println!("{s:?}: {:.1} m tall, {:.1} m wide", h(s), w(s));
        }
        // The eucalipto towers over the rest; the aromo is the lowest.
        for s in [Species::Paraiso, Species::Olmo, Species::Aromo] {
            assert!(h(Species::Eucalipto) > h(s) + 2.0, "{s:?}");
            if s != Species::Aromo {
                assert!(h(s) > h(Species::Aromo) + 1.0, "{s:?}");
            }
        }
        // The paraíso's umbrella is wider than it is tall; the olmo's vase
        // and the eucalipto are taller than wide.
        assert!(w(Species::Paraiso) > h(Species::Paraiso));
        assert!(h(Species::Olmo) > w(Species::Olmo));
        assert!(h(Species::Eucalipto) > 1.5 * w(Species::Eucalipto));
        // The olmo is a vase: its leaves spread wider up high than low.
        let t = tree(Species::Olmo, 0);
        let spread = |lo: f32, hi: f32| {
            t.leaves
                .positions
                .iter()
                .filter(|p| p.y >= lo * t.height && p.y < hi * t.height)
                .map(|p| Vec2::new(p.x, p.z).length())
                .fold(0.0, f32::max)
        };
        assert!(spread(0.7, 1.0) > spread(0.3, 0.55), "a vase");
        // The eucalipto's cards hang: most sprigs point down.
        let t = tree(Species::Eucalipto, 0);
        let down = t
            .leaves
            .positions
            .chunks(6)
            .filter(|c| c[4].y < c[0].y)
            .count();
        assert!(down * 4 > t.leaves.positions.len() / 6 * 3, "{down}");
    }

    #[test]
    fn leaf_normals_lean_out_of_the_crown() {
        for (species, v, t) in all() {
            let p = &t.leaves.positions;
            let centre = p.iter().sum::<Vec3>() / p.len() as f32;
            let out = p
                .iter()
                .zip(&t.leaves.normals)
                .filter(|(p, n)| n.dot(**p - centre) > 0.0)
                .count();
            assert!(
                out * 10 >= p.len() * 9,
                "{species:?} {v}: {out} of {}",
                p.len()
            );
            assert!(
                t.leaves
                    .normals
                    .iter()
                    .all(|n| (n.length() - 1.0).abs() < 1e-4)
            );
        }
    }

    #[test]
    fn each_card_shows_one_sprig_hung_by_its_base() {
        let t = tree(Species::Paraiso, 0);
        for card in t.leaves.uvs.chunks(6) {
            let (u0, v0) = (card[0][0], card[0][1] - 0.5);
            assert!(u0 == 0.0 || u0 == 0.5, "{card:?}");
            assert!(v0 == 0.0 || v0 == 0.5, "{card:?}");
            for uv in card {
                assert!((u0..=u0 + 0.5).contains(&uv[0]), "{card:?}");
                assert!((v0..=v0 + 0.5).contains(&uv[1]), "{card:?}");
            }
            // The tip row samples the top of the quarter.
            assert_eq!(card[4][1], v0);
        }
        // Every quarter is used.
        let quarters: std::collections::HashSet<_> = t
            .leaves
            .uvs
            .chunks(6)
            .map(|c| (c[0][0].to_bits(), c[0][1].to_bits()))
            .collect();
        assert_eq!(quarters.len(), 4);
    }

    #[test]
    fn bark_repeats_whole_times_round_and_tangents_follow_it() {
        for (species, v, t) in all() {
            let b = &t.bark;
            for (tangent, n) in b.tangents().iter().zip(&b.normals) {
                let t3 = Vec3::new(tangent[0], tangent[1], tangent[2]);
                assert!((t3.length() - 1.0).abs() < 1e-3, "{species:?} {v}");
                assert!(t3.dot(*n).abs() < 1e-3, "{species:?} {v}");
            }
            // A tube's seam: the last vertex of each ring sits on the
            // first with a whole number of repeats between.
            let u_max = b.uvs.iter().map(|uv| uv[0]).fold(0.0, f32::max);
            assert_eq!(u_max, u_max.round(), "{species:?} {v}");
        }
    }
}
