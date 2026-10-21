//! Low-poly 3D people, built from rigid faceted parts like the buildings and
//! the train: flat colours, no textures.
//!
//! A figure is a body entity (turned to the heading) holding a tree of joint
//! entities: pelvis, waist, neck, shoulders, elbows, hips and knees. Each
//! part (head, torso, a forearm, a hat brim...) is a scaled unit mesh hung
//! off its joint, so animation only turns joints (see
//! [`pampa_core::pose`]). Every figure shares the same four unit meshes and
//! one material per colour, so nothing is built per figure or per frame.
//!
//! The figure faces local +X, its right is +Z and its feet stand on the
//! body's origin, about 1.75 m below the top of its head.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use pampa_core::{Heading, JointAngles, pose};

use super::paint::Rgb;

/// How many townsfolk looks there are; logic's `look` picks one.
pub const TOWNSFOLK_LOOKS: u8 = 5;

/// Who a figure is drawn as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FigureKind {
    /// The player on foot: a hardware-shop clerk from General Pico in his
    /// city clothes.
    Protagonist,
    /// A townsperson, in look `0..TOWNSFOLK_LOOKS`.
    Townsfolk(u8),
    /// Don Hidalgo, the old estanciero who gives the first errand.
    Hidalgo,
    /// Comisario Elías Farach, in uniform.
    Farach,
}

impl FigureKind {
    /// The townsfolk look for a logic appearance value.
    pub fn townsfolk(look: u8) -> Self {
        Self::Townsfolk(look % TOWNSFOLK_LOOKS)
    }

    /// Every figure.
    pub fn all() -> impl Iterator<Item = Self> {
        [Self::Protagonist, Self::Hidalgo, Self::Farach]
            .into_iter()
            .chain((0..TOWNSFOLK_LOOKS).map(Self::Townsfolk))
    }
}

/// A pivot that animation turns. Left is -Z, right +Z.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Joint {
    Pelvis,
    Waist,
    Neck,
    Shoulder(Side),
    Elbow(Side),
    Hip(Side),
    Knee(Side),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    const BOTH: [Self; 2] = [Self::Left, Self::Right];

    fn index(self) -> usize {
        self as usize
    }

    /// Sign along the body's Z axis.
    fn z(self) -> f32 {
        match self {
            Self::Left => -1.0,
            Self::Right => 1.0,
        }
    }
}

impl Joint {
    /// Every joint, each after its parent.
    pub const ALL: [Self; 11] = [
        Self::Pelvis,
        Self::Waist,
        Self::Neck,
        Self::Shoulder(Side::Left),
        Self::Shoulder(Side::Right),
        Self::Elbow(Side::Left),
        Self::Elbow(Side::Right),
        Self::Hip(Side::Left),
        Self::Hip(Side::Right),
        Self::Knee(Side::Left),
        Self::Knee(Side::Right),
    ];

    /// The joint this one hangs from (`None`: the body).
    pub fn parent(self) -> Option<Self> {
        match self {
            Self::Pelvis => None,
            Self::Waist | Self::Hip(_) => Some(Self::Pelvis),
            Self::Neck | Self::Shoulder(_) => Some(Self::Waist),
            Self::Elbow(side) => Some(Self::Shoulder(side)),
            Self::Knee(side) => Some(Self::Hip(side)),
        }
    }

    /// Offset from the parent joint (or the feet) when standing.
    pub fn rest(self) -> Vec3 {
        match self {
            Self::Pelvis => Vec3::new(0.0, 0.92, 0.0),
            Self::Waist => Vec3::new(0.0, 0.08, 0.0),
            Self::Neck => Vec3::new(0.0, 0.46, 0.0),
            Self::Shoulder(s) => Vec3::new(0.0, 0.42, 0.21 * s.z()),
            Self::Elbow(_) => Vec3::new(0.0, -0.29, 0.0),
            Self::Hip(s) => Vec3::new(0.0, 0.0, 0.09 * s.z()),
            Self::Knee(_) => Vec3::new(0.0, -0.42, 0.0),
        }
    }

    /// Standing position above the feet.
    pub fn rest_position(self) -> Vec3 {
        self.rest() + self.parent().map_or(Vec3::ZERO, Self::rest_position)
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|j| *j == self).unwrap_or(0)
    }

    /// This joint's turn in `angles`, about the body's side axis.
    fn rotation(self, a: &JointAngles) -> Quat {
        // Positive Z turns a hanging limb toward +X (forward).
        let angle = match self {
            Self::Pelvis => 0.0,
            Self::Waist => -a.lean,
            // The head stays nearly level while the torso leans.
            Self::Neck => 0.6 * a.lean - a.nod,
            Self::Shoulder(s) => a.shoulder[s.index()],
            Self::Elbow(s) => a.elbow[s.index()],
            Self::Hip(s) => a.hip[s.index()],
            Self::Knee(s) => a.knee[s.index()],
        };
        Quat::from_rotation_z(angle)
    }
}

/// The shared unit meshes parts are scaled copies of. Each fits a 1 m box
/// centered on the origin, so a part's size is its bounding box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Shape {
    /// Tapered limbs with rounded ends: thigh, shin, upper arm, forearm.
    Thigh,
    Shin,
    /// A bombacha's lower leg: baggy, gathered at the ankle.
    BaggyShin,
    UpperArm,
    Forearm,
    Neck,
    /// Rounded block of a hand.
    Hand,
    /// A shoe along +X: heel, instep and a lower toe.
    Shoe,
    /// A plain work boot along +X, its shaft rising at the heel.
    Boot,
    /// Rounded pelvis.
    Hips,
    /// Chest wider than the waist, with shoulders.
    Torso,
    /// The torso of a jacket or tunic, flaring at the hem.
    Coat,
    /// A worn leather jacket: the coat's cut with scuffed shading.
    Jacket,
    /// A thin front panel (waistcoat, shirt front), flaring at the hem.
    Panel,
    /// The head: a smooth sphere of 18 sides and 12 rings.
    Head,
    /// A small rounded lump (jaw, hair, boina, knot, buckle).
    Lump,
    /// A tiny bump (nose, moustache, buttons).
    Bump,
    /// A flat round disc with hard edges (hat brims, bands, the kepi visor).
    Disc,
    /// A lighter ring for cuffs, seams and soles.
    Band,
    /// A soft brim turned slightly down (the gacho).
    Brim,
    /// A hat crown, a little narrower at the top.
    Crown,
    /// A long skirt flaring to the hem.
    Skirt,
    /// A poncho draped over the shoulders, with hard folds.
    Poncho,
}

/// One ring of a loft: height, half extents along X and Z, X offset and a
/// shade multiplied into the colour (for cheap darkening at hems and joints).
#[derive(Debug, Clone, Copy)]
struct Ring(f32, f32, f32, f32, f32);

/// A round ring of radius `r` at `y`.
const fn round(y: f32, r: f32) -> Ring {
    Ring(y, r, r, 0.0, 1.0)
}

/// Sides around limbs, neck, hands and feet.
const LIMB_SIDES: usize = 14;
/// Sides around the torso, hips and coats.
const BODY_SIDES: usize = 16;

/// A limb from `top` radius down to `bottom` radius, with rounded ends, darker
/// toward its lower end. Its muscle swells a little above the middle and
/// narrows before the lower joint, so knees, calves and elbows read.
fn limb(top: f32, bottom: f32) -> Vec<Ring> {
    let at = |t: f32| bottom + (top - bottom) * t;
    let shaded = |ring: Ring, shade: f32| Ring(ring.0, ring.1, ring.2, 0.0, shade);
    vec![
        shaded(round(-0.5, bottom * 0.45), 0.86),
        shaded(round(-0.45, bottom * 0.86), 0.88),
        shaded(round(-0.36, bottom), 0.92),
        shaded(round(-0.12, at(0.3) * 0.96), 0.96),
        round(0.12, at(0.8)),
        round(0.36, top),
        round(0.45, top * 0.86),
        round(0.5, top * 0.45),
    ]
}

impl Shape {
    pub const ALL: [Self; 23] = [
        Self::Band,
        Self::Boot,
        Self::Jacket,
        Self::Brim,
        Self::Thigh,
        Self::Shin,
        Self::BaggyShin,
        Self::UpperArm,
        Self::Forearm,
        Self::Neck,
        Self::Hand,
        Self::Shoe,
        Self::Hips,
        Self::Torso,
        Self::Coat,
        Self::Panel,
        Self::Head,
        Self::Lump,
        Self::Bump,
        Self::Disc,
        Self::Crown,
        Self::Skirt,
        Self::Poncho,
    ];

    /// Builds this unit mesh. Skin and body rounds get smooth normals; brims
    /// and the poncho keep hard edges.
    pub fn mesh(self) -> Mesh {
        let sphere = Sphere::new(0.5);
        match self {
            // Trousers crease toward the knee.
            Self::Thigh => loft_cloth(
                &limb(0.5, 0.36),
                LIMB_SIDES,
                Cloth {
                    folds: 3,
                    depth: 0.08,
                    ..Cloth::PLAIN
                },
            ),
            Self::Shin => loft(&limb(0.5, 0.34), LIMB_SIDES),
            Self::BaggyShin => loft(
                &[
                    Ring(-0.5, 0.18, 0.18, 0.0, 0.85),
                    Ring(-0.45, 0.3, 0.3, 0.0, 0.85),
                    round(-0.3, 0.5),
                    round(0.05, 0.48),
                    round(0.35, 0.44),
                    round(0.46, 0.38),
                    round(0.5, 0.2),
                ],
                LIMB_SIDES,
            ),
            Self::UpperArm => loft(&limb(0.5, 0.4), LIMB_SIDES),
            Self::Forearm => loft(&limb(0.5, 0.34), LIMB_SIDES),
            Self::Neck => loft(
                &[
                    round(-0.5, 0.4),
                    round(-0.3, 0.5),
                    Ring(0.3, 0.45, 0.45, 0.0, 0.72),
                    Ring(0.5, 0.3, 0.3, 0.0, 0.7),
                ],
                LIMB_SIDES,
            ),
            Self::Hand => loft(
                &[
                    Ring(-0.5, 0.3, 0.24, 0.0, 0.95),
                    Ring(-0.35, 0.45, 0.42, 0.0, 1.0),
                    Ring(0.1, 0.5, 0.5, 0.0, 1.0),
                    Ring(0.4, 0.42, 0.45, 0.0, 0.9),
                    Ring(0.5, 0.28, 0.3, 0.0, 0.9),
                ],
                8,
            ),
            // Lofted heel to toe along Y, then laid along +X: a ring's X
            // half extent becomes the height and its offset lowers the toe.
            Self::Shoe => loft(
                &[
                    Ring(-0.5, 0.36, 0.3, 0.0, 0.9),
                    Ring(-0.42, 0.5, 0.42, 0.0, 1.0),
                    Ring(-0.1, 0.5, 0.45, 0.0, 1.0),
                    Ring(0.2, 0.4, 0.5, 0.1, 1.0),
                    Ring(0.4, 0.3, 0.42, 0.18, 1.0),
                    Ring(0.5, 0.15, 0.24, 0.25, 1.0),
                ],
                LIMB_SIDES,
            )
            .rotated_by(Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2)),
            // A work boot: like the shoe, with a shaft rising at the heel.
            Self::Boot => loft(
                &[
                    Ring(-0.5, 0.5, 0.3, -0.0, 0.85),
                    Ring(-0.42, 0.5, 0.42, 0.0, 0.95),
                    Ring(-0.1, 0.5, 0.45, 0.0, 1.0),
                    Ring(0.2, 0.3, 0.5, 0.2, 0.95),
                    Ring(0.4, 0.22, 0.42, 0.28, 0.9),
                    Ring(0.5, 0.12, 0.24, 0.33, 0.9),
                ],
                LIMB_SIDES,
            )
            .rotated_by(Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2)),
            Self::Hips => loft(
                &[
                    Ring(-0.5, 0.3, 0.36, 0.0, 0.8),
                    Ring(-0.3, 0.45, 0.48, 0.0, 0.9),
                    round(0.1, 0.5),
                    Ring(0.5, 0.45, 0.46, 0.0, 1.0),
                ],
                BODY_SIDES,
            ),
            Self::Torso => loft(&torso(0.4, 0.36), BODY_SIDES),
            Self::Coat => loft(&torso(0.5, 0.48), BODY_SIDES),
            // Worn leather: the coat's cut with scuffed, uneven shading.
            Self::Jacket => loft_cloth(
                &torso(0.5, 0.48),
                BODY_SIDES,
                Cloth {
                    scuff: 0.16,
                    ..Cloth::PLAIN
                },
            ),
            Self::Panel => loft(
                &[
                    Ring(-0.5, 0.5, 0.5, 0.0, 0.85),
                    Ring(-0.3, 0.5, 0.42, 0.0, 1.0),
                    Ring(0.2, 0.5, 0.38, 0.0, 1.0),
                    Ring(0.5, 0.5, 0.2, 0.0, 1.0),
                ],
                8,
            ),
            // Twelve rings give the cheekbones and chin their curve.
            Self::Head => sphere.mesh().uv(18, 12),
            Self::Lump => sphere
                .mesh()
                .ico(1)
                .unwrap_or_else(|_| sphere.mesh().uv(10, 6)),
            Self::Bump => sphere.mesh().uv(8, 5),
            Self::Disc => Cylinder::new(0.5, 1.0).mesh().resolution(24).build(),
            // A light ring for cuffs, seams and soles.
            Self::Band => Cylinder::new(0.5, 1.0).mesh().resolution(14).build(),
            // A soft wide brim turned slightly down all round.
            Self::Brim => loft_cloth(
                &[
                    Ring(-0.5, 0.5, 0.5, 0.0, 0.85),
                    Ring(-0.1, 0.44, 0.44, 0.0, 0.95),
                    round(0.3, 0.3),
                    round(0.5, 0.24),
                ],
                24,
                Cloth {
                    folds: 5,
                    depth: 0.06,
                    ..Cloth::PLAIN
                },
            ),
            Self::Crown => loft(
                &[
                    round(-0.5, 0.5),
                    round(0.1, 0.48),
                    Ring(0.38, 0.42, 0.4, 0.0, 1.0),
                    Ring(0.5, 0.3, 0.26, 0.0, 1.0),
                ],
                14,
            ),
            Self::Skirt => loft_cloth(
                &[
                    Ring(-0.5, 0.5, 0.5, 0.0, 0.78),
                    Ring(-0.42, 0.48, 0.48, 0.0, 0.9),
                    round(-0.1, 0.4),
                    round(0.3, 0.32),
                    round(0.5, 0.28),
                ],
                24,
                Cloth {
                    folds: 12,
                    depth: 0.2,
                    ..Cloth::PLAIN
                },
            ),
            Self::Poncho => loft_cloth(
                &[
                    Ring(-0.5, 0.5, 0.5, 0.0, 0.8),
                    Ring(-0.38, 0.48, 0.49, 0.0, 0.92),
                    round(0.1, 0.36),
                    round(0.38, 0.26),
                    round(0.5, 0.14),
                ],
                20,
                Cloth {
                    folds: 10,
                    depth: 0.22,
                    ..Cloth::PLAIN
                },
            )
            .with_duplicated_vertices()
            .with_computed_flat_normals(),
        }
    }
}

/// A torso from the waist (or hem) up to the shoulders and the base of the
/// neck: `hem` is the half width at the bottom, `waist` a little above it.
fn torso(hem: f32, waist: f32) -> Vec<Ring> {
    vec![
        Ring(-0.5, hem * 0.9, hem, 0.0, 0.82),
        Ring(-0.36, waist * 0.95, waist, 0.0, 0.92),
        Ring(-0.12, 0.44, 0.42, 0.0, 1.0),
        Ring(0.15, 0.5, 0.47, 0.0, 1.0),
        Ring(0.32, 0.47, 0.5, 0.0, 1.0),
        Ring(0.42, 0.38, 0.46, 0.0, 0.95),
        Ring(0.48, 0.25, 0.3, 0.0, 0.9),
        Ring(0.5, 0.12, 0.12, 0.0, 0.9),
    ]
}

/// How a loft's cloth hangs: `folds` vertical folds, deepening toward the
/// hem to `depth` (a fraction of the radius), and `scuff`, how unevenly worn
/// its shading is.
#[derive(Debug, Clone, Copy)]
struct Cloth {
    folds: u32,
    depth: f32,
    scuff: f32,
}

impl Cloth {
    const PLAIN: Self = Self {
        folds: 0,
        depth: 0.0,
        scuff: 0.0,
    };
}

/// A closed surface through `rings` (bottom to top), `segments` around, with
/// smooth normals and per-vertex shades.
fn loft(rings: &[Ring], segments: usize) -> Mesh {
    loft_cloth(rings, segments, Cloth::PLAIN)
}

/// [`loft`] with folds and wear (see [`Cloth`]).
fn loft_cloth(rings: &[Ring], segments: usize, cloth: Cloth) -> Mesh {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{Indices, PrimitiveTopology};
    use std::f32::consts::TAU;
    let n = segments;
    let mut positions = Vec::with_capacity(rings.len() * n + 2);
    let mut colors = Vec::with_capacity(rings.len() * n + 2);
    for (r, &Ring(y, rx, rz, dx, shade)) in rings.iter().enumerate() {
        for k in 0..n {
            let angle = k as f32 / n as f32 * TAU;
            let (s, c) = angle.sin_cos();
            // Folds pull in between their ridges, most at the hem (y = -0.5).
            let fold = (1.0 - (cloth.folds as f32 * angle).cos()) / 2.0;
            let pull = 1.0 - cloth.depth * (0.5 - y) * fold;
            positions.push([dx + rx * pull * c, y, rz * pull * s]);
            // Deterministic wear: a cheap hash of the vertex.
            let wear = ((r * 31 + k * 17) % 7) as f32 / 6.0;
            let shade = shade * (1.0 - cloth.scuff * wear) * (1.0 - 0.25 * cloth.depth * fold);
            colors.push([shade, shade, shade, 1.0]);
        }
    }
    let mut indices: Vec<u32> = Vec::new();
    for r in 0..rings.len().saturating_sub(1) {
        for k in 0..n {
            let at = |ring: usize, k: usize| (ring * n + k % n) as u32;
            let (a, b, c, d) = (at(r, k), at(r, k + 1), at(r + 1, k), at(r + 1, k + 1));
            indices.extend([a, c, b, b, c, d]);
        }
    }
    // Close both ends with a fan round a center point.
    let last = rings.len() - 1;
    for (ring, bottom) in [(0, true), (last, false)] {
        let Ring(y, _, _, dx, shade) = rings[ring];
        let center = positions.len() as u32;
        positions.push([dx, y, 0.0]);
        colors.push([shade, shade, shade, 1.0]);
        for k in 0..n {
            let a = (ring * n + k) as u32;
            let b = (ring * n + (k + 1) % n) as u32;
            indices.extend(if bottom {
                [center, a, b]
            } else {
                [center, b, a]
            });
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices))
    .with_computed_smooth_normals()
}

/// What a part is; tests and accessories go by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Piece {
    Head,
    Jaw,
    /// The front marker: it sits on the face, along +X.
    Nose,
    Hair,
    Neck,
    Torso,
    Hips,
    UpperArm,
    Forearm,
    Hand,
    UpperLeg,
    LowerLeg,
    Foot,
    /// Waistcoat or chaleco front.
    Vest,
    /// Shirt front showing in an open jacket.
    Shirt,
    /// A high collar: the city shirt's stiff one, or a blouse's high neck.
    Collar,
    Tie,
    /// Pañuelo knotted at the neck.
    Neckerchief,
    Button,
    /// Rastra or leather belt, and the rastra's silver buckle.
    Belt,
    Buckle,
    /// A coloured sash (faja) round the waist.
    Faja,
    Moustache,
    Poncho,
    Shawl,
    Skirt,
    /// A wide-brimmed felt hat: brim and crown.
    Hat,
    /// The city man's bowler (bombín): a round crown and a narrow brim.
    Bowler,
    /// The gacho: a soft wide-brimmed felt hat, brim turned slightly down.
    Gacho,
    /// The leather jacket's collar and lapels.
    Lapel,
    Pocket,
    /// Small face details: brow ridge, ears and the shadowed eye sockets.
    Brow,
    Ear,
    Eye,
    Thumb,
    /// Sleeve or shirt cuffs, the jacket's hem seam, the kepi's crown seam.
    Cuff,
    Seam,
    /// Boot and shoe soles with their heels.
    Sole,
    /// Hair worn up in a bun.
    Bun,
    Boina,
    Kepi,
    Headscarf,
}

/// One part of a figure, at rest: a scaled unit [`Shape`] centered at
/// `center` relative to its joint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PartSpec {
    pub piece: Piece,
    pub joint: Joint,
    pub shape: Shape,
    pub color: Rgb,
    pub center: Vec3,
    pub size: Vec3,
}

/// Headwear of a look.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Headwear {
    /// The city bowler (bombín) with a dark band, kept for the town
    /// gentlemen of the setting guide; nobody wears it yet.
    #[expect(dead_code, reason = "for the later town-gentleman looks")]
    Bowler {
        felt: Rgb,
        band: Rgb,
    },
    /// A soft wide-brimmed country hat.
    FeltHat(Rgb),
    /// The gacho: a soft wide-brimmed felt hat, its brim turned slightly
    /// down, a creased crown and a band.
    Gacho {
        felt: Rgb,
        band: Rgb,
    },
    /// No hat: hair worn up in a bun.
    Bun,
    Boina(Rgb),
    Kepi {
        cloth: Rgb,
        visor: Rgb,
    },
    Headscarf(Rgb),
}

/// Clothes and colouring of a figure.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Look {
    skin: Rgb,
    hair: Rgb,
    /// Jacket, shirt, blouse or tunic: torso and sleeves.
    coat: Rgb,
    /// A jacket or tunic, flaring over the hips.
    jacket: bool,
    /// Waistcoat: a front panel in an open jacket, else a chaleco.
    vest: Option<Rgb>,
    /// Shirt front showing in an open jacket.
    shirt: Option<Rgb>,
    /// A high collar (stiff shirt collar or high-necked blouse) and a tie.
    collar: Option<Rgb>,
    tie: Option<Rgb>,
    neckerchief: Option<Rgb>,
    /// A sash, or a plain leather belt.
    faja: Option<Rgb>,
    shawl: Option<Rgb>,
    /// Tall boots up to the knee, in the shoes' colour.
    tall_boots: bool,
    /// Plain work boots instead of shoes or alpargatas.
    boots: bool,
    /// A worn leather jacket with collar, lapels and pockets.
    leather: bool,
    /// Trousers, bombacha or skirt.
    legs: Rgb,
    /// Baggy bombacha gathered at the ankle.
    bombacha: bool,
    skirt: bool,
    shoes: Rgb,
    head: Headwear,
    poncho: Option<Rgb>,
    moustache: Option<Rgb>,
    rastra: bool,
    buttons: Option<Rgb>,
}

// Earthy, muted 1919 palette (see `odd/setting-1919.md`): dust, ochre,
// faded blue, brown, grey and black wool, off-white shirts. Bright colour
// only in accents: pañuelos, a faja, the police's brass buttons.
const SKIN: Rgb = [206, 162, 124];
const HAIR: Rgb = [58, 42, 32];
const OFF_WHITE: Rgb = [214, 206, 186];
const ALPARGATAS: Rgb = [190, 178, 150];
const BOOTS: Rgb = [52, 38, 30];
const PANUELO_RED: Rgb = [164, 44, 36];

/// A plain man's look, for the townsfolk to vary: a Basque-style paisano in
/// a collarless shirt, boina, bombacha, faja, alpargatas and a pañuelo.
const PAISANO: Look = Look {
    skin: SKIN,
    hair: HAIR,
    coat: OFF_WHITE,
    jacket: false,
    vest: None,
    shirt: None,
    collar: None,
    tie: None,
    neckerchief: Some(PANUELO_RED),
    faja: Some([132, 44, 40]),
    shawl: None,
    tall_boots: false,
    boots: false,
    leather: false,
    legs: [50, 48, 50],
    bombacha: true,
    skirt: false,
    shoes: ALPARGATAS,
    head: Headwear::Boina([38, 42, 56]),
    poncho: None,
    moustache: None,
    rastra: false,
    buttons: None,
};

/// The protagonist's worn brown leather.
pub const LEATHER: Rgb = [108, 72, 46];

/// The protagonist, a humble hardware-shop clerk from General Pico: a plain
/// work shirt, a worn leather jacket, faded trousers, used boots and a gacho.
/// Townsman's working clothes, not a gaucho's.
const PROTAGONIST: Look = Look {
    skin: [216, 174, 136],
    hair: [70, 50, 36],
    coat: LEATHER,
    jacket: true,
    leather: true,
    vest: None,
    shirt: Some([190, 184, 164]),
    collar: None,
    tie: None,
    neckerchief: None,
    faja: None,
    legs: [98, 92, 82],
    bombacha: false,
    shoes: [62, 46, 34],
    boots: true,
    head: Headwear::Gacho {
        felt: [84, 72, 58],
        band: [46, 38, 30],
    },
    moustache: Some([70, 50, 36]),
    ..PAISANO
};

const TOWNSFOLK: [Look; TOWNSFOLK_LOOKS as usize] = [
    PAISANO,
    // Peón: chaleco over a collarless shirt, soft wide-brimmed hat, worn
    // boots and a white pañuelo.
    Look {
        skin: [190, 142, 104],
        coat: [170, 158, 132],
        vest: Some([88, 66, 46]),
        legs: [96, 86, 70],
        shoes: BOOTS,
        head: Headwear::FeltHat([92, 76, 58]),
        neckerchief: Some([226, 222, 210]),
        faja: Some([64, 46, 34]),
        moustache: Some(HAIR),
        ..PAISANO
    },
    // A Volga German farmhand in a poncho against the cold.
    Look {
        skin: [212, 172, 140],
        hair: [150, 120, 80],
        coat: [148, 132, 108],
        legs: [74, 68, 60],
        head: Headwear::FeltHat([66, 56, 46]),
        poncho: Some([124, 76, 52]),
        neckerchief: None,
        faja: None,
        shoes: BOOTS,
        ..PAISANO
    },
    // Women: ankle-length skirts, high-necked long-sleeved blouses, a
    // shawl, and a headscarf or hair worn up.
    Look {
        skin: [212, 168, 132],
        coat: [116, 128, 142],
        collar: Some([116, 128, 142]),
        legs: [72, 56, 44],
        bombacha: false,
        skirt: true,
        shoes: [56, 42, 34],
        head: Headwear::Headscarf([196, 176, 128]),
        shawl: Some([102, 70, 54]),
        neckerchief: None,
        faja: None,
        ..PAISANO
    },
    Look {
        hair: [40, 30, 26],
        coat: OFF_WHITE,
        collar: Some(OFF_WHITE),
        legs: [44, 50, 62],
        bombacha: false,
        skirt: true,
        shoes: BOOTS,
        head: Headwear::Bun,
        shawl: Some([60, 58, 62]),
        neckerchief: None,
        faja: None,
        ..PAISANO
    },
];

/// Don Hidalgo, an older estanciero: a good brown jacket over a dark
/// waistcoat, bombacha, tall boots, a rastra, a light wide-brimmed hat and a
/// white moustache.
const HIDALGO: Look = Look {
    skin: [212, 168, 132],
    hair: [226, 222, 214],
    coat: [96, 74, 54],
    jacket: true,
    vest: Some([46, 42, 44]),
    shirt: Some(OFF_WHITE),
    legs: [62, 58, 56],
    shoes: BOOTS,
    tall_boots: true,
    head: Headwear::FeltHat([188, 176, 144]),
    moustache: Some([236, 234, 228]),
    rastra: true,
    neckerchief: None,
    faja: None,
    ..PAISANO
};

/// The police uniform's dark blue.
pub const POLICE_BLUE: Rgb = [38, 50, 86];

/// Comisario Farach: kepi, dark blue tunic with brass buttons, a leather
/// belt, dark trousers and boots.
const FARACH: Look = Look {
    skin: [196, 150, 112],
    hair: [36, 28, 24],
    coat: POLICE_BLUE,
    jacket: true,
    legs: [30, 32, 42],
    bombacha: false,
    shoes: [26, 24, 24],
    tall_boots: true,
    head: Headwear::Kepi {
        cloth: [32, 40, 64],
        visor: [20, 20, 22],
    },
    moustache: Some([36, 28, 24]),
    buttons: Some([204, 166, 70]),
    neckerchief: None,
    faja: Some([36, 28, 24]),
    ..PAISANO
};

/// `color` a shade darker: seams, cuffs, soles and eye sockets.
fn darker(color: Rgb) -> Rgb {
    color.map(|c| (c as f32 * 0.78) as u8)
}

fn look(kind: FigureKind) -> &'static Look {
    match kind {
        FigureKind::Protagonist => &PROTAGONIST,
        FigureKind::Townsfolk(n) => &TOWNSFOLK[(n % TOWNSFOLK_LOOKS) as usize],
        FigureKind::Hidalgo => &HIDALGO,
        FigureKind::Farach => &FARACH,
    }
}

/// Every part of `kind`, standing. Deterministic.
///
/// Kept as a table, one part per line.
#[rustfmt::skip]
pub fn figure_parts(kind: FigureKind) -> Vec<PartSpec> {
    let l = look(kind);
    let mut parts = Vec::with_capacity(32);
    let mut add = |piece, joint, shape, color, center: [f32; 3], size: [f32; 3]| {
        parts.push(PartSpec {
            piece,
            joint,
            shape,
            color,
            center: Vec3::from(center),
            size: Vec3::from(size),
        });
    };
    use Joint::*;
    use Piece::*;

    // Legs, from the hip down to the sole at the feet's origin. Segments
    // overlap past each joint so no gap opens when it turns. Trousers are
    // wider than a bare leg; a bombacha is baggier still.
    let (hips, thigh, shin, shin_shape) = if l.bombacha {
        ([0.27, 0.22, 0.4], [0.22, 0.5, 0.22], [0.19, 0.45, 0.19], Shape::BaggyShin)
    } else {
        ([0.24, 0.2, 0.36], [0.17, 0.5, 0.17], [0.14, 0.46, 0.14], Shape::Shin)
    };
    add(Hips, Pelvis, Shape::Hips, l.legs, [0.0, -0.01, 0.0], hips);
    for s in Side::BOTH {
        add(UpperLeg, Hip(s), Shape::Thigh, l.legs, [0.0, -0.2, 0.0], thigh);
        if l.tall_boots {
            add(LowerLeg, Knee(s), Shape::Shin, l.shoes, [0.0, -0.2, 0.0], [0.15, 0.46, 0.15]);
        } else {
            add(LowerLeg, Knee(s), shin_shape, l.legs, [0.0, -0.2, 0.0], shin);
        }
        if l.boots {
            add(Foot, Knee(s), Shape::Boot, l.shoes, [0.045, -0.43, 0.0], [0.27, 0.14, 0.12]);
            add(Sole, Knee(s), Shape::Band, darker(l.shoes), [0.04, -0.49, 0.0], [0.29, 0.02, 0.12]);
        } else {
            add(Foot, Knee(s), Shape::Shoe, l.shoes, [0.05, -0.45, 0.0], [0.27, 0.1, 0.115]);
            add(Sole, Knee(s), Shape::Band, darker(l.shoes), [0.045, -0.49, 0.0], [0.28, 0.02, 0.115]);
        }
    }
    if l.skirt {
        add(Skirt, Pelvis, Shape::Skirt, l.legs, [0.0, -0.36, 0.0], [0.58, 0.88, 0.62]);
    }

    // Torso, arms and what is worn over them. A jacket or tunic reaches
    // over the hips and flares at the hem.
    if l.leather {
        // Open at the front over the shirt: a turned-down collar, lapels and
        // two flapped pockets, all in the same worn leather, a shade darker.
        let edge = [l.coat[0] - 18, l.coat[1] - 14, l.coat[2] - 10];
        add(Torso, Waist, Shape::Jacket, l.coat, [0.0, 0.2, 0.0], [0.27, 0.6, 0.43]);
        add(Seam, Waist, Shape::Band, edge, [0.0, -0.075, 0.0], [0.27, 0.02, 0.43]);
        if let Some(shirt) = l.shirt {
            add(Shirt, Waist, Shape::Panel, shirt, [0.126, 0.3, 0.0], [0.02, 0.34, 0.1]);
        }
        add(Lapel, Waist, Shape::Disc, edge, [-0.005, 0.47, 0.0], [0.17, 0.05, 0.25]);
        for z in [-0.06, 0.06] {
            add(Lapel, Waist, Shape::Panel, edge, [0.133, 0.36, z], [0.02, 0.2, 0.06]);
            add(Pocket, Waist, Shape::Panel, edge, [0.128, 0.04, 2.0 * z], [0.02, 0.07, 0.09]);
        }
    } else if l.jacket {
        add(Torso, Waist, Shape::Coat, l.coat, [0.0, 0.2, 0.0], [0.27, 0.6, 0.43]);
    } else {
        add(Torso, Waist, Shape::Torso, l.coat, [0.0, 0.24, 0.0], [0.25, 0.5, 0.42]);
    }
    for s in Side::BOTH {
        add(UpperArm, Shoulder(s), Shape::UpperArm, l.coat, [0.0, -0.14, 0.0], [0.11, 0.33, 0.11]);
        add(Forearm, Elbow(s), Shape::Forearm, l.coat, [0.0, -0.13, 0.0], [0.095, 0.29, 0.095]);
        add(Hand, Elbow(s), Shape::Hand, l.skin, [0.0, -0.31, 0.0], [0.085, 0.1, 0.05]);
        add(Thumb, Elbow(s), Shape::Bump, l.skin, [0.03, -0.295, -0.02 * s.z()], [0.03, 0.06, 0.03]);
        add(Cuff, Elbow(s), Shape::Band, darker(l.coat), [0.0, -0.255, 0.0], [0.1, 0.025, 0.1]);
    }
    match (l.vest, l.jacket) {
        // A waistcoat and shirt front showing in the open jacket.
        (Some(vest), true) => {
            add(Vest, Waist, Shape::Panel, vest, [0.125, 0.2, 0.0], [0.03, 0.42, 0.2]);
            if let Some(shirt) = l.shirt {
                add(Shirt, Waist, Shape::Panel, shirt, [0.137, 0.34, 0.0], [0.02, 0.2, 0.09]);
            }
        }
        // A chaleco over the shirt, its sleeves showing.
        (Some(vest), false) => {
            add(Vest, Waist, Shape::Torso, vest, [0.0, 0.2, 0.0], [0.27, 0.42, 0.44]);
        }
        (None, _) => {}
    }
    if let Some(brass) = l.buttons {
        for y in [0.1, 0.2, 0.3] {
            add(Button, Waist, Shape::Bump, brass, [0.135, y, 0.0], [0.02, 0.025, 0.025]);
        }
    }
    if let Some(tie) = l.tie {
        add(Tie, Waist, Shape::Panel, tie, [0.15, 0.36, 0.0], [0.015, 0.17, 0.035]);
    }
    if let Some(faja) = l.faja {
        add(Faja, Waist, Shape::Disc, faja, [0.0, 0.03, 0.0], [0.27, 0.08, 0.43]);
    }
    if l.rastra {
        add(Belt, Waist, Shape::Disc, [42, 32, 26], [0.0, 0.02, 0.0], [0.28, 0.07, 0.44]);
        add(Buckle, Waist, Shape::Lump, [196, 198, 196], [0.135, 0.02, 0.0], [0.025, 0.08, 0.13]);
    }
    if let Some(poncho) = l.poncho {
        add(Poncho, Waist, Shape::Poncho, poncho, [0.0, 0.15, 0.0], [0.6, 0.72, 0.66]);
    }
    if let Some(shawl) = l.shawl {
        add(Shawl, Waist, Shape::Poncho, shawl, [0.0, 0.3, 0.0], [0.36, 0.42, 0.52]);
    }

    // Head: neck, skull, jaw, the nose as the front marker, hair, headwear.
    let (neck, skin) = (Joint::Neck, l.skin);
    add(Piece::Neck, neck, Shape::Neck, skin, [0.0, 0.04, 0.0], [0.1, 0.12, 0.1]);
    add(Head, neck, Shape::Head, skin, [0.0, 0.17, 0.0], [0.2, 0.22, 0.18]);
    add(Jaw, neck, Shape::Lump, skin, [0.025, 0.105, 0.0], [0.15, 0.1, 0.14]);
    add(Nose, neck, Shape::Bump, skin, [0.1, 0.16, 0.0], [0.04, 0.05, 0.035]);
    add(Brow, neck, Shape::Bump, skin, [0.085, 0.2, 0.0], [0.03, 0.03, 0.15]);
    for z in [-0.045, 0.045] {
        add(Eye, neck, Shape::Bump, darker(skin), [0.082, 0.18, z], [0.02, 0.025, 0.04]);
    }
    for z in [-0.092, 0.092] {
        add(Ear, neck, Shape::Bump, skin, [-0.005, 0.165, z], [0.035, 0.06, 0.025]);
    }
    add(Hair, neck, Shape::Lump, l.hair, [-0.02, 0.21, 0.0], [0.2, 0.16, 0.19]);
    if let Some(collar) = l.collar {
        add(Collar, neck, Shape::Disc, collar, [0.0, 0.05, 0.0], [0.12, 0.06, 0.12]);
    }
    if let Some(scarf) = l.neckerchief {
        add(Neckerchief, neck, Shape::Lump, scarf, [0.045, 0.02, 0.0], [0.08, 0.06, 0.13]);
    }
    if let Some(moustache) = l.moustache {
        add(Moustache, neck, Shape::Lump, moustache, [0.095, 0.125, 0.0], [0.03, 0.025, 0.09]);
    }
    match l.head {
        Headwear::Bowler { felt, band } => {
            add(Bowler, neck, Shape::Disc, felt, [0.0, 0.255, 0.0], [0.27, 0.015, 0.25]);
            add(Bowler, neck, Shape::Lump, felt, [0.0, 0.28, 0.0], [0.21, 0.16, 0.2]);
            add(Bowler, neck, Shape::Disc, band, [0.0, 0.272, 0.0], [0.212, 0.025, 0.202]);
        }
        Headwear::Bun => {
            add(Bun, neck, Shape::Lump, l.hair, [-0.09, 0.25, 0.0], [0.08, 0.08, 0.09]);
        }
        Headwear::Gacho { felt, band } => {
            add(Gacho, neck, Shape::Brim, felt, [0.0, 0.25, 0.0], [0.46, 0.05, 0.44]);
            add(Gacho, neck, Shape::Crown, felt, [0.0, 0.31, 0.0], [0.21, 0.12, 0.2]);
            add(Gacho, neck, Shape::Disc, band, [0.0, 0.275, 0.0], [0.214, 0.03, 0.204]);
        }
        Headwear::FeltHat(felt) => {
            add(Hat, neck, Shape::Disc, felt, [0.0, 0.255, 0.0], [0.44, 0.015, 0.42]);
            add(Hat, neck, Shape::Crown, felt, [0.0, 0.3, 0.0], [0.21, 0.1, 0.2]);
        }
        Headwear::Boina(cloth) => {
            add(Boina, neck, Shape::Lump, cloth, [-0.01, 0.275, 0.0], [0.25, 0.06, 0.24]);
        }
        Headwear::Kepi { cloth, visor } => {
            add(Kepi, neck, Shape::Crown, cloth, [0.0, 0.3, 0.0], [0.19, 0.14, 0.18]);
            add(Kepi, neck, Shape::Disc, visor, [0.1, 0.245, 0.0], [0.11, 0.015, 0.15]);
            add(Seam, neck, Shape::Band, visor, [0.0, 0.335, 0.0], [0.188, 0.012, 0.178]);
        }
        Headwear::Headscarf(cloth) => {
            add(Headscarf, neck, Shape::Head, cloth, [-0.005, 0.19, 0.0], [0.215, 0.2, 0.2]);
            add(Headscarf, neck, Shape::Lump, cloth, [-0.11, 0.1, 0.0], [0.08, 0.1, 0.08]);
        }
    }
    parts
}

/// Height of the top of `kind`'s head or headwear, standing.
pub fn figure_height(kind: FigureKind) -> f32 {
    figure_parts(kind)
        .iter()
        .map(|p| p.joint.rest_position().y + p.center.y + p.size.y / 2.0)
        .fold(0.0, f32::max)
}

/// The body's turn for a ground `heading`: its front (+X) points along the
/// heading on the ground (ground north is Bevy -Z).
pub fn body_rotation(heading: Heading) -> Quat {
    Quat::from_rotation_y(heading.radians())
}

/// The shared unit meshes and one material per colour.
#[derive(Resource, Default)]
pub struct FigureKit {
    meshes: HashMap<Shape, Handle<Mesh>>,
    materials: HashMap<Rgb, Handle<StandardMaterial>>,
}

impl FigureKit {
    pub fn new(meshes: &mut Assets<Mesh>) -> Self {
        Self {
            meshes: Shape::ALL.map(|s| (s, meshes.add(s.mesh()))).into(),
            materials: HashMap::default(),
        }
    }

    fn material(
        &mut self,
        color: Rgb,
        materials: &mut Assets<StandardMaterial>,
    ) -> Handle<StandardMaterial> {
        self.materials
            .entry(color)
            .or_insert_with(|| {
                materials.add(StandardMaterial {
                    base_color: Color::srgb_u8(color[0], color[1], color[2]),
                    perceptual_roughness: 0.95,
                    reflectance: 0.15,
                    ..default()
                })
            })
            .clone()
    }
}

/// The turning body of a figure, child of the character entity.
#[derive(Component, Debug, Clone)]
pub struct FigureBody {
    pub kind: FigureKind,
    /// How far through the gait (or breath) cycle, in cycles (`0..1`).
    pub phase: f32,
    /// Joint entities in [`Joint::ALL`] order.
    joints: Vec<Entity>,
}

/// A joint entity of a figure.
#[derive(Component, Debug, Clone, Copy)]
pub struct FigureJoint(pub Joint);

/// A part mesh of a figure.
#[derive(Component, Debug, Clone, Copy)]
pub struct FigurePart(pub Piece);

/// Spawns `kind`'s body, joints and parts under `parent`; returns the body.
pub(super) fn spawn_figure(
    commands: &mut Commands,
    kit: &mut FigureKit,
    materials: &mut Assets<StandardMaterial>,
    parent: Entity,
    kind: FigureKind,
    phase: f32,
) -> Entity {
    let body = commands
        .spawn((
            Name::new("Figure"),
            Transform::default(),
            Visibility::default(),
            ChildOf(parent),
        ))
        .id();
    let mut joints: Vec<Entity> = Vec::with_capacity(Joint::ALL.len());
    for joint in Joint::ALL {
        let holder = joint.parent().map_or(body, |p| joints[p.index()]);
        joints.push(
            commands
                .spawn((
                    FigureJoint(joint),
                    Transform::from_translation(joint.rest()),
                    Visibility::default(),
                    ChildOf(holder),
                ))
                .id(),
        );
    }
    for part in figure_parts(kind) {
        let Some(mesh) = kit.meshes.get(&part.shape).cloned() else {
            continue;
        };
        commands.spawn((
            FigurePart(part.piece),
            Mesh3d(mesh),
            MeshMaterial3d(kit.material(part.color, materials)),
            Transform::from_translation(part.center).with_scale(part.size),
            ChildOf(joints[part.joint.index()]),
        ));
    }
    commands.entity(body).insert(FigureBody {
        kind,
        phase,
        joints,
    });
    body
}

/// Turns, bobs and poses each figure from its character's pose, its gait
/// paced by speed.
pub(super) fn update_figures(
    time: Res<Time>,
    poses: Query<&super::VisualPose>,
    mut bodies: Query<(&ChildOf, &mut FigureBody, &mut Transform), Without<FigureJoint>>,
    mut joints: Query<(&FigureJoint, &mut Transform), Without<FigureBody>>,
) {
    let dt = time.delta_secs();
    for (child_of, mut body, mut transform) in &mut bodies {
        let Ok(visual) = poses.get(child_of.parent()) else {
            continue;
        };
        body.phase = pose::advance(body.phase, visual.animation, visual.speed, dt);
        let angles = pose::pose(visual.animation, body.phase);
        transform.rotation = body_rotation(visual.heading);
        transform.translation.y = angles.bob;
        for &entity in &body.joints {
            let Ok((joint, mut transform)) = joints.get_mut(entity) else {
                continue;
            };
            let breath = if joint.0 == Joint::Waist {
                angles.breath
            } else {
                0.0
            };
            transform.translation = joint.0.rest() + Vec3::Y * breath;
            transform.rotation = joint.0.rotation(&angles);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world_from_ground;

    fn has(kind: FigureKind, piece: Piece) -> bool {
        figure_parts(kind).iter().any(|p| p.piece == piece)
    }

    fn color_of(kind: FigureKind, piece: Piece) -> Option<Rgb> {
        figure_parts(kind)
            .iter()
            .find(|p| p.piece == piece)
            .map(|p| p.color)
    }

    #[test]
    fn every_look_builds_about_as_tall_as_a_person_and_the_same_each_time() {
        for kind in FigureKind::all() {
            let parts = figure_parts(kind);
            assert!(parts.len() >= 18, "{kind:?}: {} parts", parts.len());
            assert_eq!(parts, figure_parts(kind), "{kind:?} is deterministic");
            let height = figure_height(kind);
            assert!((1.6..=1.9).contains(&height), "{kind:?}: {height} m");
            // Feet on the ground.
            let sole = parts
                .iter()
                .map(|p| p.joint.rest_position().y + p.center.y - p.size.y / 2.0)
                .fold(f32::MAX, f32::min);
            assert!(sole.abs() < 1e-4, "{kind:?}: sole at {sole}");
        }
        assert_eq!(FigureKind::townsfolk(7), FigureKind::Townsfolk(2));
        let looks: Vec<_> = (0..TOWNSFOLK_LOOKS)
            .map(|n| figure_parts(FigureKind::Townsfolk(n)))
            .collect();
        for (i, a) in looks.iter().enumerate() {
            for b in &looks[i + 1..] {
                assert_ne!(a, b, "townsfolk looks differ");
            }
        }
    }

    #[test]
    fn the_protagonist_wears_a_humble_townsmans_clothes() {
        let me = FigureKind::Protagonist;
        let shape_of = |piece| {
            figure_parts(me)
                .iter()
                .find(|p| p.piece == piece)
                .map(|p| p.shape)
        };
        // A gacho, a worn leather jacket with lapels, a work shirt, boots.
        assert!(has(me, Piece::Gacho) && has(me, Piece::Shirt));
        assert_eq!(shape_of(Piece::Torso), Some(Shape::Jacket));
        assert_eq!(color_of(me, Piece::Torso), Some(LEATHER));
        assert!(has(me, Piece::Lapel) && has(me, Piece::Pocket));
        assert_eq!(shape_of(Piece::Foot), Some(Shape::Boot));
        // Not a town gentleman: no bowler, tie or stiff collar.
        assert!(!has(me, Piece::Bowler) && !has(me, Piece::Tie) && !has(me, Piece::Collar));
        // Not a gaucho: no poncho, bombacha or faja.
        assert!(!has(me, Piece::Poncho) && !has(me, Piece::Faja) && !has(me, Piece::Boina));
        assert!(!has(me, Piece::Skirt) && !has(me, Piece::Neckerchief));
        // Nobody else wears a gacho or his jacket.
        for kind in FigureKind::all().filter(|k| *k != me) {
            assert!(
                !has(kind, Piece::Gacho) && !has(kind, Piece::Lapel),
                "{kind:?}"
            );
        }
        // Trousers, not a bombacha: as slim as the comisario's.
        let thigh = |k| {
            figure_parts(k)
                .iter()
                .find(|p| p.piece == Piece::UpperLeg)
                .map(|p| p.size.x)
        };
        assert_eq!(thigh(me), thigh(FigureKind::Farach));
        assert!(thigh(me) < thigh(FigureKind::Townsfolk(0)));
        // His jacket is a colour nobody in town wears.
        let jacket = color_of(me, Piece::Torso);
        for n in 0..TOWNSFOLK_LOOKS {
            assert_ne!(jacket, color_of(FigureKind::Townsfolk(n), Piece::Torso));
        }
        // The townsfolk wear the country clothes.
        assert!((0..TOWNSFOLK_LOOKS).any(|n| has(FigureKind::Townsfolk(n), Piece::Poncho)));
        assert!((0..TOWNSFOLK_LOOKS).any(|n| has(FigureKind::Townsfolk(n), Piece::Boina)));
        assert!((0..TOWNSFOLK_LOOKS).any(|n| has(FigureKind::Townsfolk(n), Piece::Skirt)));
    }

    #[test]
    fn women_wear_ankle_length_skirts_and_high_necked_blouses() {
        let women: Vec<_> = (0..TOWNSFOLK_LOOKS)
            .map(FigureKind::Townsfolk)
            .filter(|k| has(*k, Piece::Skirt))
            .collect();
        assert_eq!(women.len(), 2);
        for kind in women {
            let skirt = figure_parts(kind)
                .into_iter()
                .find(|p| p.piece == Piece::Skirt)
                .unwrap();
            let hem = skirt.joint.rest_position().y + skirt.center.y - skirt.size.y / 2.0;
            // Down to the ankle, just above the shoes' tops (0.1 m).
            assert!((0.05..=0.15).contains(&hem), "{kind:?}: hem at {hem} m");
            assert!(
                has(kind, Piece::Collar) && has(kind, Piece::Shawl),
                "{kind:?}"
            );
            assert!(
                has(kind, Piece::Headscarf) || has(kind, Piece::Bun),
                "{kind:?}"
            );
        }
        // Peones wear the faja and the pañuelo.
        assert!(has(FigureKind::Townsfolk(0), Piece::Faja));
        assert!(has(FigureKind::Townsfolk(1), Piece::Neckerchief));
    }

    #[test]
    fn the_errand_people_have_their_own_looks() {
        let hidalgo = FigureKind::Hidalgo;
        assert!(has(hidalgo, Piece::Moustache) && has(hidalgo, Piece::Buckle));
        assert!(has(hidalgo, Piece::Hat) && has(hidalgo, Piece::Vest));
        assert_eq!(color_of(hidalgo, Piece::Moustache), Some([236, 234, 228]));
        let farach = FigureKind::Farach;
        assert!(has(farach, Piece::Kepi) && has(farach, Piece::Button));
        assert_eq!(color_of(farach, Piece::Torso), Some(POLICE_BLUE));
        let others: Vec<_> = FigureKind::all()
            .filter(|k| *k != hidalgo && *k != farach)
            .collect();
        for kind in others {
            assert_ne!(figure_parts(kind), figure_parts(hidalgo));
            assert_ne!(figure_parts(kind), figure_parts(farach));
            assert!(!has(kind, Piece::Kepi) && !has(kind, Piece::Buckle));
        }
    }

    #[test]
    fn the_body_faces_its_ground_heading() {
        let nose = figure_parts(FigureKind::Protagonist)
            .into_iter()
            .find(|p| p.piece == Piece::Nose)
            .unwrap();
        let point = nose.joint.rest_position() + nose.center;
        assert!(
            point.x > 0.08 && point.z.abs() < 1e-6,
            "the nose is in front"
        );
        for degrees in [0.0_f32, 45.0, 90.0, 180.0, 250.0, 300.0] {
            let heading = Heading::from_radians(degrees.to_radians());
            let turned = body_rotation(heading) * point;
            let along = Vec3::new(turned.x, 0.0, turned.z).normalize();
            let want = world_from_ground(heading.forward(), 0.0);
            assert!(along.distance(want) < 1e-4, "{degrees}°: {along} vs {want}");
        }
    }

    fn triangles(shape: Shape) -> usize {
        shape.mesh().triangles().map(|t| t.count()).unwrap_or(0)
    }

    #[test]
    fn a_figure_has_ps1_to_ps2_era_detail() {
        for kind in FigureKind::all() {
            let total: usize = figure_parts(kind).iter().map(|p| triangles(p.shape)).sum();
            println!("{kind:?}: {total} triangles");
            assert!((3500..=5500).contains(&total), "{kind:?}: {total}");
        }
    }

    #[test]
    fn heads_and_limbs_are_smooth_rounds_not_boxes() {
        use bevy::mesh::VertexAttributeValues;
        let rounded = [
            Shape::Head,
            Shape::Thigh,
            Shape::Shin,
            Shape::UpperArm,
            Shape::Forearm,
            Shape::Torso,
            Shape::Hand,
        ];
        for shape in rounded {
            let mesh = shape.mesh();
            let Some(VertexAttributeValues::Float32x3(normals)) =
                mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
            else {
                panic!("{shape:?} has normals");
            };
            let slanted = normals
                .iter()
                .filter(|n| n.iter().filter(|c| c.abs() > 0.2).count() >= 2)
                .count();
            assert!(
                slanted * 3 > normals.len(),
                "{shape:?}: mostly axis-aligned normals"
            );
            // Smooth: faces share vertices, so there are fewer vertices than
            // three per triangle.
            assert!(
                mesh.count_vertices() < triangles(shape) * 2,
                "{shape:?} is faceted"
            );
            // It fits its unit box, so a part's size is its bounding box.
            let Some(VertexAttributeValues::Float32x3(points)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("{shape:?} has positions");
            };
            assert!(points.iter().flatten().all(|c| c.abs() <= 0.5 + 1e-4));
        }
        // Brims keep their hard edge; the poncho its folds.
        assert!(
            Shape::Poncho.mesh().indices().is_none(),
            "unshared vertices"
        );
    }
}
