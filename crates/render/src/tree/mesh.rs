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
    /// How far the trunk leans, a fraction of the height (a low fork leans
    /// less, see [`grow`]).
    pub lean: f32,
    /// How much a prevailing wind shapes the crown: 0 is even all round;
    /// 1 has the lee limbs well longer and flatter than the windward ones,
    /// the trunk leaning with them.
    pub wind: f32,
    /// Share of the leaf cards hung in dense clumps at the branch tips
    /// instead of scattered along the branches.
    pub clump: f32,
    /// A shrub: its stems taper to fine tips and carry fine twigs, where a
    /// tree's limbs end at a twig's girth whatever the trunk's radius.
    pub shrub: bool,
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
                lean: 0.04,
                wind: 0.0,
                clump: 0.0,
                shrub: false,
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
                lean: 0.04,
                wind: 0.0,
                clump: 0.0,
                shrub: false,
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
                lean: 0.04,
                wind: 0.0,
                clump: 0.0,
                shrub: false,
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
                lean: 0.04,
                wind: 0.0,
                clump: 0.0,
                shrub: false,
            },
            // The caldén: a short thick trunk forking low into near
            // horizontal limbs, a broad flat umbrella wider than it is
            // tall, of small fine leaves, hung in dense clumps. Its trunk
            // stays within what `pampa_core::Tree::trunk_radius` plants
            // (0.035 of the height). This is the classic form; see
            // [`Self::variant_form`] for the rest.
            Self::Calden => Form {
                height: (6.0, 7.0),
                fork: 0.25,
                trunk_radius: 0.14,
                limbs: (6, 7),
                tilt: (45.0, 60.0),
                reach: 0.42,
                rise: 0.1,
                twigs: 4,
                leader: false,
                cards: 170,
                card: 1.3,
                hang: 0.2,
                sag: 0.2,
                scatter: 0.3,
                // Coarse, so the furrows survive the mipmaps at a distance.
                bark_tile: Vec2::new(1.2, 1.8),
                lean: 0.04,
                wind: 0.0,
                clump: 0.6,
                shrub: false,
            },
            // The chañar: several slim stems fanning from one low stool,
            // twiggy, with an irregular crown of small leaves in clumps.
            Self::Chanar => Form {
                height: (3.4, 4.2),
                fork: 0.08,
                trunk_radius: 0.09,
                limbs: (5, 7),
                tilt: (15.0, 32.0),
                reach: 0.75,
                rise: 0.12,
                twigs: 5,
                leader: false,
                cards: 110,
                card: 0.6,
                hang: 0.2,
                sag: 0.2,
                scatter: 0.5,
                bark_tile: Vec2::new(0.8, 1.6),
                lean: 0.04,
                wind: 0.0,
                clump: 0.5,
                shrub: false,
            },
            // The piquillín: a dense thorny shrub, many fine stems from the
            // ground under a mass of tiny leaves.
            Self::Piquillin => Form {
                height: (1.4, 1.7),
                fork: 0.05,
                trunk_radius: 0.035,
                limbs: (5, 6),
                tilt: (28.0, 48.0),
                reach: 0.8,
                rise: 0.1,
                twigs: 2,
                leader: false,
                cards: 50,
                card: 0.24,
                hang: 0.15,
                sag: 0.15,
                scatter: 0.4,
                bark_tile: Vec2::new(0.3, 0.5),
                lean: 0.02,
                wind: 0.0,
                clump: 0.5,
                shrub: true,
            },
            // The jarilla: many upright, slender stems, sparse and resinous.
            Self::Jarilla => Form {
                height: (1.2, 1.5),
                fork: 0.04,
                trunk_radius: 0.03,
                limbs: (7, 8),
                tilt: (6.0, 16.0),
                reach: 0.95,
                rise: 0.05,
                twigs: 1,
                leader: false,
                cards: 55,
                card: 0.2,
                hang: 0.1,
                sag: 0.1,
                scatter: 0.35,
                bark_tile: Vec2::new(0.3, 0.6),
                lean: 0.01,
                wind: 0.0,
                clump: 0.3,
                shrub: true,
            },
            // The fachinal: chañar suckers, a thicket of slim stems.
            Self::Fachinal => Form {
                height: (1.8, 2.1),
                fork: 0.06,
                trunk_radius: 0.05,
                limbs: (5, 6),
                tilt: (10.0, 26.0),
                reach: 0.8,
                rise: 0.1,
                twigs: 2,
                leader: false,
                cards: 50,
                card: 0.4,
                hang: 0.2,
                sag: 0.2,
                scatter: 0.5,
                bark_tile: Vec2::new(0.5, 1.0),
                lean: 0.03,
                wind: 0.0,
                clump: 0.5,
                shrub: true,
            },
        }
    }

    /// Its shape for variant `seed`. The caldén has [`crate::tree::CALDEN_LIVE`]
    /// live forms (a classic umbrella, a low multi-stem, a leaning tree, a
    /// lopsided wind-shaped crown, a tall narrow one, a broad flat one, a
    /// dense round one, a high fork and a windswept leaning one), then three
    /// snags (sparse, dead, and a broken stub); the chañar has
    /// [`crate::tree::CHANAR_VARIANTS`] forms. Every other species has its
    /// [`Self::form`] for every seed.
    pub(crate) fn variant_form(self, seed: u32) -> Form {
        let form = self.form();
        let v = seed as usize;
        match self {
            Self::Calden => match v {
                // Classic: single trunk, an even umbrella.
                0 => form,
                // Low-forked multi-stem: many stout limbs from near the ground.
                1 => Form {
                    fork: 0.12,
                    limbs: (7, 8),
                    tilt: (22.0, 42.0),
                    reach: 0.5,
                    rise: 0.15,
                    cards: 180,
                    clump: 0.7,
                    ..form
                },
                // Leaning: the whole tree tilts, the crown hangs to one side.
                2 => Form {
                    fork: 0.3,
                    lean: 0.17,
                    tilt: (40.0, 58.0),
                    reach: 0.4,
                    ..form
                },
                // Lopsided: wind-shaped, long low limbs to the lee side.
                3 => Form {
                    fork: 0.22,
                    tilt: (55.0, 72.0),
                    reach: 0.46,
                    wind: 0.9,
                    lean: 0.09,
                    ..form
                },
                // Tall and narrow: a high fork under steep limbs.
                4 => Form {
                    height: (7.5, 8.5),
                    fork: 0.42,
                    limbs: (5, 6),
                    tilt: (14.0, 28.0),
                    reach: 0.28,
                    rise: 0.2,
                    cards: 160,
                    card: 1.2,
                    ..form
                },
                // Broad and flat: long near-horizontal limbs, a low crown.
                5 => Form {
                    height: (4.8, 5.6),
                    fork: 0.4,
                    limbs: (7, 8),
                    tilt: (68.0, 80.0),
                    reach: 0.62,
                    rise: 0.12,
                    cards: 190,
                    card: 1.4,
                    clump: 0.6,
                    ..form
                },
                // Dense and round: a compact crown packed with leaves.
                6 => Form {
                    fork: 0.4,
                    limbs: (7, 8),
                    tilt: (32.0, 48.0),
                    reach: 0.34,
                    twigs: 3,
                    cards: 190,
                    card: 1.5,
                    scatter: 0.25,
                    clump: 0.85,
                    ..form
                },
                // High fork: few thick limbs far up a tall trunk.
                7 => Form {
                    height: (6.8, 7.6),
                    fork: 0.46,
                    limbs: (3, 4),
                    tilt: (35.0, 50.0),
                    reach: 0.5,
                    twigs: 5,
                    cards: 170,
                    ..form
                },
                // Windswept: leaning hard, the crown streaming downwind.
                8 => Form {
                    fork: 0.36,
                    tilt: (50.0, 70.0),
                    reach: 0.5,
                    rise: 0.05,
                    wind: 0.8,
                    lean: 0.14,
                    ..form
                },
                // Sparse: a thinned crown.
                9 => Form {
                    limbs: (5, 6),
                    cards: 60,
                    clump: 0.3,
                    ..form
                },
                // Dead: bare, twisted limbs reaching up, a last few leaves.
                10 => Form {
                    limbs: (4, 5),
                    tilt: (35.0, 60.0),
                    reach: 0.7,
                    rise: 0.3,
                    twigs: 5,
                    cards: 16,
                    clump: 0.0,
                    ..form
                },
                // A broken stub: the top gone, short limbs, a trace of leaves.
                _ => Form {
                    height: (3.8, 4.6),
                    trunk_radius: 0.12,
                    fork: 0.35,
                    limbs: (3, 4),
                    tilt: (30.0, 50.0),
                    reach: 0.4,
                    rise: 0.25,
                    twigs: 4,
                    cards: 12,
                    clump: 0.0,
                    ..form
                },
            },
            Self::Chanar => match v {
                // The classic fan of stems.
                0 => form,
                // Tall and slender.
                1 => Form {
                    height: (4.2, 4.8),
                    tilt: (8.0, 18.0),
                    reach: 0.55,
                    ..form
                },
                // Low and spreading.
                2 => Form {
                    height: (2.8, 3.2),
                    tilt: (35.0, 55.0),
                    reach: 0.95,
                    ..form
                },
                // Leaning.
                3 => Form {
                    lean: 0.12,
                    fork: 0.22,
                    ..form
                },
                // A dense thicket.
                4 => Form {
                    limbs: (7, 8),
                    cards: 150,
                    card: 0.65,
                    clump: 0.8,
                    scatter: 0.35,
                    ..form
                },
                // Wind-shaped.
                _ => Form {
                    wind: 0.8,
                    lean: 0.1,
                    tilt: (25.0, 45.0),
                    ..form
                },
            },
            // The shrubs: the plain form, a low spreading one, a tall dense
            // one, the seed turning the stems of each.
            Self::Piquillin | Self::Jarilla | Self::Fachinal => match v % 3 {
                0 => form,
                1 => Form {
                    height: (form.height.0 * 0.7, form.height.1 * 0.7),
                    tilt: (form.tilt.0 + 14.0, form.tilt.1 + 16.0),
                    reach: form.reach * 1.2,
                    ..form
                },
                _ => Form {
                    height: (form.height.0 * 1.25, form.height.1 * 1.25),
                    tilt: (form.tilt.0 * 0.7, form.tilt.1 * 0.7),
                    clump: (form.clump + 0.2).min(1.0),
                    ..form
                },
            },
            _ => form,
        }
    }
}

/// Dense clumps of leaves a crown hangs ([`Form::clump`]).
const CLUMPS: usize = 9;

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
    grow(species.variant_form(seed), Rng::new(species, seed))
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
    // A low fork leans less: the trunk is short, and a lean of 4% of the
    // height would tilt it far over (every town tree forks above 0.2).
    let lean_angle = rng.range((0.0, std::f32::consts::TAU));
    // A windswept tree leans, and its limbs stream, the same way.
    let wind_angle = if form.wind > 0.0 {
        rng.range((0.0, std::f32::consts::TAU))
    } else {
        lean_angle
    };
    let lean = heading(wind_angle) * form.lean * height * (form.fork * 5.0).min(1.0);
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
        // Downwind (1) the limbs run long and low, upwind (-1) short and
        // steep.
        let lee = (around - wind_angle).cos() * form.wind;
        let tilt = (rng.range(form.tilt) + 12.0 * lee).to_radians();
        let length = form.reach * height * rng.range((0.85, 1.1)) * (1.0 + 0.55 * lee);
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
        // A shrub's stems taper to a fine tip, a tree's limbs to a twig's
        // girth (by [`Form::shrub`], not by how thin the trunk is).
        let (tip, twig_radii) = if form.shrub {
            (width * 0.3, [0.012, 0.005])
        } else {
            (0.035, [0.03, 0.012])
        };
        tube(
            &mut bark,
            &[start - dir * width, mid, end],
            &[width, width * 0.55, tip],
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
                &twig_radii,
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
    // A stretch by its length.
    let pick_stretch = |rng: &mut Rng| {
        let mut pick = rng.unit() * total;
        leafy
            .iter()
            .find(|l| {
                pick -= l.from.distance(l.to);
                pick <= 0.0
            })
            .unwrap_or(&leafy[leafy.len() - 1])
    };
    // Where the dense clumps hang: toward the ends of the stretches.
    let clumps: Vec<Vec3> = if form.clump > 0.0 {
        (0..CLUMPS)
            .map(|_| {
                let l = pick_stretch(rng);
                l.from.lerp(l.to, 0.6 + 0.4 * rng.unit())
            })
            .collect()
    } else {
        Vec::new()
    };
    for _ in 0..form.cards {
        let (at, dir) = if !clumps.is_empty() && rng.unit() < form.clump {
            // A clump's card: within a card's reach of its centre, so the
            // leaves overlap into one mass.
            let c = clumps[(rng.unit() * CLUMPS as f32) as usize % CLUMPS];
            (
                c + rng.ball() * side * 1.3,
                (c - centre).normalize_or(Vec3::Y),
            )
        } else {
            // A spot on a stretch, mostly toward its end.
            let stretch = pick_stretch(rng);
            let t = 0.2 + 0.8 * rng.unit().sqrt();
            (
                stretch.from.lerp(stretch.to, t),
                (stretch.to - stretch.from).normalize_or(Vec3::Y),
            )
        };
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

    fn field() -> Vec<(Species, u32, TreeMesh)> {
        Species::FIELD
            .iter()
            .flat_map(|&s| (0..s.variants() as u32).map(move |v| (s, v, tree(s, v))))
            .collect()
    }

    fn live(species: Species, v: u32) -> bool {
        species != Species::Calden || (v as usize) < crate::tree::CALDEN_LIVE
    }

    /// What tells a form's silhouette apart: its height, its crown's width,
    /// how far the crown's middle sits off the trunk, how low it hangs
    /// (a fraction of the height), how much leaf it carries and how far
    /// the bark spreads in the lower half, a fraction of the height
    /// (stems fanning from the ground against a single trunk).
    fn outline(t: &TreeMesh) -> [f32; 6] {
        let p = &t.leaves.positions;
        let middle = p.iter().map(|p| Vec2::new(p.x, p.z)).sum::<Vec2>() / p.len() as f32;
        let spread = t
            .bark
            .positions
            .iter()
            .filter(|p| (0.15..0.5).contains(&(p.y / t.height)))
            .map(|p| Vec2::new(p.x, p.z).distance(middle))
            .fold(0.0, f32::max);
        [
            t.height,
            2.0 * t.crown_radius,
            middle.length(),
            t.crown_base / t.height,
            t.leaves.triangles() as f32,
            spread / t.height,
        ]
    }

    #[test]
    fn field_trees_keep_to_the_town_trees_triangle_budget() {
        for (species, v, t) in field() {
            let (bark, leaves) = (t.bark.triangles(), t.leaves.triangles());
            println!("{species:?} {v}: {bark} bark + {leaves} leaves");
            assert!(bark + leaves <= 1200, "{species:?} {v}: {bark} + {leaves}");
            if live(species, v) {
                assert!(bark + leaves >= 650, "{species:?} {v}: {bark} + {leaves}");
            }
            assert!(bark >= 100 && leaves >= 16, "{species:?} {v}");
            for b in [&t.bark, &t.leaves] {
                let n = b.positions.len();
                assert_eq!(b.normals.len(), n);
                assert_eq!(b.uvs.len(), n);
                assert!(b.indices.iter().all(|&i| (i as usize) < n));
            }
        }
    }

    #[test]
    fn field_variants_are_seeded_and_all_differ() {
        assert_eq!(Species::Calden.variants(), 12);
        assert_eq!(Species::Chanar.variants(), 6);
        for species in Species::FIELD {
            let n = species.variants() as u32;
            for v in 0..n {
                assert_eq!(tree(species, v), tree(species, v));
                for w in v + 1..n {
                    assert_ne!(tree(species, v).bark, tree(species, w).bark);
                    assert_ne!(tree(species, v).leaves, tree(species, w).leaves);
                }
            }
        }
    }

    #[test]
    fn the_forms_of_a_field_species_differ_in_outline_pairwise() {
        for species in Species::FIELD {
            let forms: Vec<(u32, [f32; 6])> = (0..species.variants() as u32)
                .filter(|&v| live(species, v))
                .map(|v| (v, outline(&tree(species, v))))
                .collect();
            for (v, o) in &forms {
                println!("{species:?} {v}: {o:.2?}");
            }
            // At least 8 live caldén forms and 5 chañar ones.
            assert!(forms.len() >= if species == Species::Calden { 8 } else { 5 });
            for (i, (a, oa)) in forms.iter().enumerate() {
                for (b, ob) in &forms[i + 1..] {
                    // Clearly apart in some trait: over a fifth of it
                    // (the lean over a tenth of the height).
                    let apart = (0..6).any(|k| {
                        // The lean and the bark's spread, a tenth of the
                        // height or less; the rest, a fifth of the larger.
                        let scale = match k {
                            2 => 0.1 * oa[0],
                            3 | 5 => 0.04,
                            _ => 0.2 * oa[k].max(ob[k]),
                        };
                        (oa[k] - ob[k]).abs() > scale
                    });
                    assert!(apart, "{species:?} forms {a} and {b}: {oa:?} {ob:?}");
                }
            }
        }
        // Some are leaning or lopsided (the crown's middle well off the
        // trunk), some tall and narrow, some broad and flat.
        let calden: Vec<[f32; 6]> = (0..crate::tree::CALDEN_LIVE as u32)
            .map(|v| outline(&tree(Species::Calden, v)))
            .collect();
        let ratio = |o: &[f32; 6]| o[1] / o[0];
        assert!(calden.iter().filter(|o| o[2] > 0.1 * o[0]).count() >= 3);
        assert!(calden.iter().any(|o| ratio(o) < 1.0), "a tall narrow form");
        assert!(calden.iter().any(|o| ratio(o) > 2.2), "a broad flat form");
        // The wind-shaped forms (3 and 8) stand lopsided, well off their
        // trunks; the classic umbrella sits over its own.
        for v in [3, 8] {
            assert!(
                calden[v][2] > 0.25 * calden[v][0],
                "form {v}: {:?}",
                calden[v]
            );
        }
        assert!(calden[0][2] < 0.15 * calden[0][0], "{:?}", calden[0]);
        // No live crown dips into the ground.
        assert!(calden.iter().all(|o| o[3] > -0.06), "{calden:?}");
        // The snags are low on leaves, and a stub stands short.
        let stub = tree(Species::Calden, 11);
        let live_height = calden.iter().map(|o| o[0]).fold(f32::MAX, f32::min);
        assert!(stub.height < live_height);
    }

    #[test]
    fn field_trunks_stand_within_the_collision_core_plants_for_them() {
        use pampa_core::flora::Age;
        use pampa_core::{Species as Core, Tree, Vec2 as Ground};
        for (species, v, t) in field() {
            let core = if species == Species::Calden {
                Core::Calden
            } else {
                Core::Chanar
            };
            for age in Age::ALL {
                // Only a mature caldén is ever a snag.
                if !live(species, v) && age != Age::Mature {
                    continue;
                }
                let (lo, hi) = age.height_range(core);
                // The widest the age is planted.
                let width = if age == Age::Old { 1.4 } else { 1.18 };
                for h in [lo, 0.5 * (lo + hi), hi] {
                    let planted = Tree {
                        at: Ground::ZERO,
                        species: core,
                        age,
                        height: h,
                        width,
                        yaw: 0.0,
                        variant: 0,
                    };
                    let scale = h / t.height;
                    // Every bark vertex in the first fifth of a meter above
                    // the ground (a sapling's limbs part half a meter up).
                    for p in t.bark.positions.iter().filter(|p| p.y * scale < 0.2) {
                        let r = Vec2::new(p.x, p.z).length() * scale * width;
                        assert!(
                            r <= planted.trunk_radius(),
                            "{species:?} {v} {age:?} at {h} m: {r} over {}",
                            planted.trunk_radius()
                        );
                    }
                }
            }
            let foot = t.bark.positions.iter().map(|p| p.y).fold(1.0, f32::min);
            // A leaning trunk tilts its first ring a little under the ground.
            assert!(foot.abs() < 0.06, "{species:?} {v}: {foot}");
        }
    }

    #[test]
    fn the_calden_is_a_broad_low_forking_crown_and_the_chanar_a_small_stand_of_stems() {
        let mean = |s: Species, f: fn(&TreeMesh) -> f32| {
            let n = (0..s.variants() as u32).filter(|&v| live(s, v));
            let all: Vec<f32> = n.map(|v| f(&tree(s, v))).collect();
            all.iter().sum::<f32>() / all.len() as f32
        };
        let calden = Species::Calden;
        // Wider than tall on the whole, and its leaves hang from low branches.
        let (h, w) = (
            mean(calden, |t| t.height),
            mean(calden, |t| 2.0 * t.crown_radius),
        );
        println!("caldén {h:.1} x {w:.1}");
        assert!(w > 1.15 * h, "{w} wide, {h} tall");
        assert!(mean(calden, |t| t.crown_base / t.height) < 0.45);
        // Small leaves, no coarser than the aromo's feathery ones, and
        // smaller still on the chañar.
        for s in Species::FIELD {
            assert!(s.form().card <= Species::Aromo.form().card, "{s:?}");
        }
        assert!(Species::Chanar.form().card < 0.6 * Species::Calden.form().card);
        let leaves = |s| mean(s, |t| t.leaves.triangles() as f32);
        assert!(leaves(Species::Chanar) < 0.8 * leaves(calden));
        // The chañar is a small tree of several stems: they fan out
        // low, well past the width of the stool they rise from.
        for v in 0..Species::Chanar.variants() as u32 {
            let t = tree(Species::Chanar, v);
            let spread = |lo: f32, hi: f32| {
                t.bark
                    .positions
                    .iter()
                    .filter(|p| p.y >= lo * t.height && p.y < hi * t.height)
                    .map(|p| Vec2::new(p.x, p.z).length())
                    .fold(0.0, f32::max)
            };
            assert!(spread(0.35, 0.6) > 2.0 * spread(0.0, 0.05).max(0.05), "{v}");
            // Several stems already stand apart a third of the way up:
            // bark well off the axis, round at least three of eight sectors.
            let mut sectors = [false; 8];
            for p in t.bark.positions.iter() {
                let off = Vec2::new(p.x, p.z);
                if (0.25 * t.height..0.55 * t.height).contains(&p.y)
                    && off.length() > 0.04 * t.height
                {
                    let turn = off.y.atan2(off.x) / std::f32::consts::TAU;
                    sectors[((turn + 1.0).fract() * 8.0) as usize % 8] = true;
                }
            }
            let stems = sectors.iter().filter(|&&s| s).count();
            assert!(stems >= 3, "{v}: bark on {stems} sectors");
        }
        // The last caldén variants are dead or sparse: few leaves.
        let full = leaves(calden);
        for v in crate::tree::CALDEN_LIVE as u32..calden.variants() as u32 {
            let t = tree(calden, v);
            assert!((t.leaves.triangles() as f32) < 0.35 * full, "{v}");
            assert!(t.leaves.triangles() >= 16, "{v}");
        }
    }

    #[test]
    fn field_leaf_normals_lean_out_and_bark_tiles_whole() {
        for (species, v, t) in field() {
            let p = &t.leaves.positions;
            let centre = p.iter().sum::<Vec3>() / p.len() as f32;
            let out = p
                .iter()
                .zip(&t.leaves.normals)
                .filter(|(p, n)| n.dot(**p - centre) > 0.0)
                .count();
            assert!(
                out * 10 >= p.len() * 8,
                "{species:?} {v}: {out} of {}",
                p.len()
            );
            let u_max = t.bark.uvs.iter().map(|uv| uv[0]).fold(0.0, f32::max);
            assert_eq!(u_max, u_max.round(), "{species:?} {v}");
        }
    }

    #[test]
    fn field_bark_repeats_coarsely_so_its_furrows_survive_the_mipmaps() {
        // The first field forms tiled the bark every 0.5 x 0.9 m (the
        // caldén) and 0.5 x 0.8 m (the chañar): too fine to read from a
        // distance. A repeat now spans over a meter round and about
        // 1.5 m along, and the town trees keep theirs.
        for species in Species::FIELD {
            for v in 0..species.variants() as u32 {
                let tile = species.variant_form(v).bark_tile;
                assert!(tile.x >= 0.8 && tile.y >= 1.5, "{species:?} {v}: {tile}");
            }
        }
        assert_eq!(Species::Paraiso.form().bark_tile, Vec2::ONE);
        assert_eq!(Species::Olmo.form().bark_tile, Vec2::new(0.8, 1.6));
        // The whole repeat wraps once round a caldén's trunk, and a
        // trunk's bark stretches over less than a repeat and a half along.
        let t = tree(Species::Calden, 0);
        let foot: Vec<[f32; 2]> = t
            .bark
            .positions
            .iter()
            .zip(&t.bark.uvs)
            .filter(|(p, _)| p.y < 0.01)
            .map(|(_, uv)| *uv)
            .collect();
        assert!(!foot.is_empty());
        assert_eq!(foot.iter().map(|uv| uv[0]).fold(0.0, f32::max), 1.0);
        let along = t.bark.uvs.iter().map(|uv| uv[1]).fold(0.0, f32::max);
        assert!(along < 2.5, "{along}");
    }

    /// Share of the crown's area (seen from the side, over cells within
    /// reach of a card) that a leaf covers.
    fn crown_cover(t: &TreeMesh) -> f32 {
        let cell = 0.12;
        let tris: Vec<[Vec2; 3]> = t
            .leaves
            .indices
            .chunks(3)
            .map(|i| {
                [i[0], i[1], i[2]].map(|i| {
                    let p = t.leaves.positions[i as usize];
                    Vec2::new(p.x, p.y)
                })
            })
            .collect();
        let inside = |q: Vec2, [a, b, c]: [Vec2; 3]| {
            let side = |u: Vec2, v: Vec2| (v - u).perp_dot(q - u);
            let (d1, d2, d3) = (side(a, b), side(b, c), side(c, a));
            !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
        };
        let centres: Vec<Vec2> = tris.iter().map(|t| (t[0] + t[1] + t[2]) / 3.0).collect();
        let (mut zone, mut covered) = (0, 0);
        let (lo, hi) = (
            tris.iter().flatten().fold(Vec2::MAX, |m, p| m.min(*p)),
            tris.iter().flatten().fold(Vec2::MIN, |m, p| m.max(*p)),
        );
        let mut y = lo.y;
        while y < hi.y {
            let mut x = lo.x;
            while x < hi.x {
                let q = Vec2::new(x, y);
                // The zone: near the leaves, not out in the sky.
                if centres.iter().any(|c| c.distance(q) < 0.9) {
                    zone += 1;
                    covered += usize::from(tris.iter().any(|t| inside(q, *t)));
                }
                x += cell;
            }
            y += cell;
        }
        covered as f32 / zone as f32
    }

    #[test]
    fn live_crowns_are_denser_than_the_first_field_crowns_with_fewer_holes() {
        // The first F1 caldén: 150 cards of 0.9 m scattered along the
        // branches.
        let old = Form {
            cards: 150,
            card: 0.9,
            hang: 0.25,
            scatter: 0.35,
            clump: 0.0,
            ..Species::Calden.form()
        };
        let old = (0..4u32)
            .map(|v| crown_cover(&grow(old, Rng::new(Species::Calden, v))))
            .sum::<f32>()
            / 4.0;
        let new: Vec<f32> = (0..crate::tree::CALDEN_LIVE as u32)
            .map(|v| crown_cover(&tree(Species::Calden, v)))
            .collect();
        let mean = new.iter().sum::<f32>() / new.len() as f32;
        println!("crown cover: old {old:.2}, new {mean:.2} {new:.2?}");
        assert!(mean > old + 0.08, "{mean} against {old}");
        assert!(new.iter().all(|&c| c > old), "{new:?}");
        // Most of a live crown is leaf.
        assert!(new.iter().filter(|&&c| c > 0.6).count() >= 5, "{new:?}");
    }

    #[test]
    fn the_towns_trees_keep_the_neutral_form_fields() {
        for species in Species::ALL {
            let f = species.form();
            assert_eq!((f.lean, f.wind, f.clump), (0.04, 0.0, 0.0), "{species:?}");
            // Every variant of a town species is its one form.
            assert_eq!(species.variant_form(2).height, f.height);
        }
    }

    /// Total area of a bark mesh: a thinner limb has less of it.
    fn bark_area(t: &TreeMesh) -> f32 {
        let b = &t.bark;
        b.indices
            .chunks(3)
            .map(|c| {
                let [p, q, r] = [c[0], c[1], c[2]].map(|i| b.positions[i as usize]);
                (q - p).cross(r - p).length() / 2.0
            })
            .sum()
    }

    #[test]
    fn only_a_shrub_form_gets_fine_stem_tips_whatever_its_trunk() {
        // A thin-trunked tree (a chañar's form at 4 cm) and the same form
        // marked a shrub, grown from the same seed: the shrub's stems end
        // finer, the tree's limbs keep a twig's girth.
        let thin = Form {
            trunk_radius: 0.04,
            ..Species::Chanar.form()
        };
        let tree_area = bark_area(&grow(thin, Rng::new(Species::Chanar, 0)));
        let shrub = Form {
            shrub: true,
            ..thin
        };
        let shrub_area = bark_area(&grow(shrub, Rng::new(Species::Chanar, 0)));
        assert!(
            tree_area > shrub_area * 1.05,
            "tree {tree_area} vs shrub {shrub_area}"
        );
        // A real shrub keeps its fine tips: unmarked, it would grow thicker.
        let piquillin = Species::Piquillin.form();
        let fine = bark_area(&grow(piquillin, Rng::new(Species::Piquillin, 0)));
        let plain = Form {
            shrub: false,
            ..piquillin
        };
        let thick = bark_area(&grow(plain, Rng::new(Species::Piquillin, 0)));
        assert!(thick > fine * 1.05, "unmarked {thick} vs shrub {fine}");
    }

    #[test]
    fn the_shrub_species_are_the_shrub_forms_and_no_tree_is() {
        for species in Species::EVERY {
            let shrub = Species::SHRUBS.contains(&species);
            for v in 0..species.variants() as u32 {
                assert_eq!(species.variant_form(v).shrub, shrub, "{species:?} {v}");
            }
            assert_eq!(species.form().shrub, shrub, "{species:?}");
            // A street form keeps its species' shrubness.
            assert_eq!(species.street_form().shrub, shrub, "{species:?}");
        }
    }

    fn shrubs() -> Vec<(Species, u32, TreeMesh)> {
        Species::SHRUBS
            .iter()
            .flat_map(|&s| (0..s.variants() as u32).map(move |v| (s, v, tree(s, v))))
            .collect()
    }

    #[test]
    fn shrubs_are_low_multi_stem_forms_within_their_triangle_budget() {
        for (species, v, t) in shrubs() {
            let tris = t.bark.triangles() + t.leaves.triangles();
            println!(
                "{species:?} {v}: {tris} triangles, {:.2} m high, {:.2} m reach",
                t.height, t.crown_radius
            );
            // Cheap, so a thicket of them stays within the frame: 200-500.
            assert!((200..=500).contains(&tris), "{species:?} {v}: {tris}");
            // Low: far under any tree, and wider than the stems' foot.
            assert!(
                (0.9..3.0).contains(&t.height),
                "{species:?} {v}: {}",
                t.height
            );
            assert!(
                t.crown_radius > 0.4 && t.crown_radius < 2.2,
                "{species:?} {v}"
            );
            // Many stems from the ground, not one trunk: the leaves hang
            // low.
            assert!(t.crown_base < 0.7 * t.height, "{species:?} {v}");
            assert!(!t.leaves.positions.is_empty() && !t.bark.positions.is_empty());
        }
    }

    #[test]
    fn shrub_variants_are_seeded_and_differ_in_shape() {
        for species in Species::SHRUBS {
            let built: Vec<TreeMesh> = (0..species.variants() as u32)
                .map(|v| tree(species, v))
                .collect();
            // The same seed builds the same shrub.
            assert_eq!(built[2], tree(species, 2), "{species:?}");
            for (i, a) in built.iter().enumerate() {
                for b in &built[i + 1..] {
                    assert_ne!(a.bark.positions, b.bark.positions, "{species:?}");
                }
            }
            // The three shapes differ in height: low and spreading, plain,
            // and tall and dense.
            let (plain, low, tall) = (&built[0], &built[1], &built[2]);
            assert!(
                low.height < plain.height && plain.height < tall.height,
                "{species:?}"
            );
        }
    }

    #[test]
    fn each_shrub_species_stands_apart_from_the_others() {
        let mean = |s: Species, f: fn(&TreeMesh) -> f32| {
            let v: Vec<f32> = (0..s.variants() as u32).map(|v| f(&tree(s, v))).collect();
            v.iter().sum::<f32>() / v.len() as f32
        };
        // Jarilla: upright stems, so narrow for its height; piquillín a
        // broad dense mass of tiny leaves; fachinal the tallest.
        let narrow = |s| mean(s, |t| t.crown_radius / t.height);
        assert!(narrow(Species::Jarilla) < narrow(Species::Piquillin));
        assert!(mean(Species::Fachinal, |t| t.height) > mean(Species::Piquillin, |t| t.height));
        assert!(mean(Species::Piquillin, |t| t.leaves.triangles() as f32) > 0.0);
    }
}
