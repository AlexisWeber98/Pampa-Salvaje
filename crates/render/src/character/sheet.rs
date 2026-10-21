//! Procedural sprite sheets for characters, painted from rounded 3D-ish
//! parts (see [`paint`](super::paint)) so every view and gait frame comes
//! from the same figure.
//!
//! A sheet is a grid of square frames: one column per view direction (see
//! [`ViewDirection`](super::ViewDirection) for the order) and one row per
//! animation frame. Row 0 is standing still, the next [`GAIT_FRAMES`] rows
//! are a walk cycle and, for horses, the last [`GAIT_FRAMES`] rows are a
//! gallop cycle. Render picks the row from the logical animation state and
//! its own gait clock (see [`SheetKind::row`]).
//!
//! Only horses and the mounted rider are still sheets; people on foot are
//! low-poly 3D figures (see [`figure`](super::figure)).
//!
//! Sizes: frames are [`FRAME_SIZE`] pixels square, so a sheet is 640 x 720
//! RGBA8 (1.8 MB); the two sheets (rider and horse) take about 3.7 MB.

use pampa_core::AnimationState;

use super::direction::DIRECTIONS;
use super::paint::{Frame, P2, Part, Rgb, Shape};

/// Width and height of one frame, in pixels.
pub const FRAME_SIZE: usize = 80;

/// Frames in one walk or gallop cycle.
pub const GAIT_FRAMES: usize = 4;

/// Which sprite sheet a billboard draws from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SheetKind {
    /// Gaucho on horseback.
    Rider,
    /// Saddled horse with no rider.
    Horse,
}

impl SheetKind {
    /// Every sheet, so they can all be built up front.
    pub fn all() -> impl Iterator<Item = Self> {
        [Self::Rider, Self::Horse].into_iter()
    }

    /// Side of the square sprite in meters (frames are square).
    pub fn sprite_size(self) -> f32 {
        2.8
    }

    /// Rows of frames in this sheet: standing, walking and galloping.
    pub fn rows(self) -> usize {
        1 + 2 * GAIT_FRAMES
    }

    /// Sheet row for `animation` at gait frame `frame` (wrapped to the
    /// cycle).
    pub fn row(self, animation: AnimationState, frame: usize) -> usize {
        let frame = frame % GAIT_FRAMES;
        match animation {
            AnimationState::Idle => 0,
            AnimationState::Walk => 1 + frame,
            AnimationState::Gallop => 1 + GAIT_FRAMES + frame,
        }
    }

    /// Pixel width and height of the whole sheet.
    pub fn sheet_size(self) -> (usize, usize) {
        (FRAME_SIZE * DIRECTIONS, FRAME_SIZE * self.rows())
    }
}

/// RGBA8 sRGB pixels of a full sheet (see [`SheetKind::sheet_size`]),
/// row-major from the top-left.
pub fn sheet_pixels(kind: SheetKind) -> Vec<u8> {
    let (width, height) = kind.sheet_size();
    let mut pixels = vec![0; width * height * 4];
    for row in 0..kind.rows() {
        let pose = Pose::for_row(row);
        for k in 0..DIRECTIONS {
            let view = View::new(k);
            let mut parts = match kind {
                SheetKind::Rider => {
                    let mut parts = horse(&view, pose, false);
                    parts.extend(mounted_gaucho(&view, pose));
                    parts
                }
                SheetKind::Horse => horse(&view, pose, true),
            };
            let mut frame = Frame::new(FRAME_SIZE);
            frame.paint(&mut parts);
            frame.outline();
            frame.blit(&mut pixels, width, k * FRAME_SIZE, row * FRAME_SIZE);
        }
    }
    pixels
}

const HORSE: Rgb = [128, 84, 52];
const HORSE_DARK: Rgb = [62, 42, 30];
const HOOF: Rgb = [44, 36, 32];
const SADDLE: Rgb = [70, 44, 28];
const SADDLE_BLANKET: Rgb = [156, 70, 46];
const PONCHO: Rgb = [164, 116, 68];
const PONCHO_STRIPE: Rgb = [94, 60, 40];
const TROUSERS: Rgb = [84, 78, 70];
pub(super) const SKIN: Rgb = [204, 158, 116];
const HAIR: Rgb = [52, 38, 30];
const HAT: Rgb = [58, 44, 34];
const BOOTS: Rgb = [46, 34, 26];
const EYES: Rgb = [30, 22, 18];
/// Bright facing marker (the horse's blaze and the neckerchiefs).
pub(super) const MARKER: Rgb = [200, 40, 30];

/// Height (pixels above the ground) and radius of the mounted gaucho's head.
pub(super) const RIDER_HEAD: (f32, f32) = (63.5, 4.6);

/// How much higher (in pixels per pixel of depth) farther points are drawn:
/// the camera looks slightly down on the characters.
const TILT: f32 = 0.12;

/// A gait and how far through its cycle (`0..1`).
#[derive(Debug, Clone, Copy, PartialEq)]
enum Pose {
    Idle,
    Walk(f32),
    Gallop(f32),
}

impl Pose {
    fn for_row(row: usize) -> Self {
        let t = |first: usize| (row - first) as f32 / GAIT_FRAMES as f32;
        match row {
            0 => Self::Idle,
            r if r <= GAIT_FRAMES => Self::Walk(t(1)),
            _ => Self::Gallop(t(1 + GAIT_FRAMES)),
        }
    }

    /// Swing angle (radians, forward positive) and hoof or foot lift
    /// (pixels) of a leg whose cycle starts at `offset`.
    fn leg(self, offset: f32, walk: (f32, f32), gallop: (f32, f32)) -> (f32, f32) {
        let (t, (swing, lift)) = match self {
            Self::Idle => return (0.0, 0.0),
            Self::Walk(t) => (t, walk),
            Self::Gallop(t) => (t, gallop),
        };
        let phase = (t + offset).fract() * std::f32::consts::TAU;
        // Lifted while swinging forward, planted while pushing back.
        (swing * phase.sin(), lift * phase.cos().max(0.0))
    }
}

/// A point on a figure: `fwd` along its facing, `right` to its right and
/// `up` above the ground, in pixels.
#[derive(Debug, Clone, Copy)]
struct L3 {
    fwd: f32,
    right: f32,
    up: f32,
}

const fn l3(fwd: f32, right: f32, up: f32) -> L3 {
    L3 { fwd, right, up }
}

/// Projects figure points for frame `k`: the figure faces `k * 45°` CCW
/// from "away".
struct View {
    /// Screen-space facing: `.0` > 0 faces right, `.1` > 0 faces away.
    forward: (f32, f32),
    right: (f32, f32),
    /// Screen row of the ground under the figure's origin.
    ground: f32,
}

impl View {
    fn new(k: usize) -> Self {
        let theta = k as f32 * std::f32::consts::FRAC_PI_4;
        let (side, depth) = (-theta.sin(), theta.cos());
        Self {
            forward: (side, depth),
            right: (depth, -side),
            ground: FRAME_SIZE as f32 - 1.0,
        }
    }

    /// Whether the figure faces away from the camera, so no face shows.
    fn away(&self) -> bool {
        self.forward.1 > 0.3
    }

    /// Screen position and depth (into the screen) of `p`.
    fn at(&self, p: L3) -> (P2, f32) {
        let x = FRAME_SIZE as f32 / 2.0 + p.fwd * self.forward.0 + p.right * self.right.0;
        let depth = p.fwd * self.forward.1 + p.right * self.right.1;
        (P2::new(x, self.ground - p.up - depth * TILT), depth)
    }

    fn ball(&self, c: L3, r: f32, color: Rgb) -> Part {
        let (c, depth) = self.at(c);
        Part::new(Shape::Ball { c, r }, color, depth)
    }

    fn capsule(&self, a: L3, b: L3, ra: f32, rb: f32, color: Rgb) -> Part {
        let ((a, da), (b, db)) = (self.at(a), self.at(b));
        Part::new(Shape::Capsule { a, b, ra, rb }, color, (da + db) / 2.0)
    }

    /// A flat disc seen from slightly above (a hat brim).
    fn disc(&self, c: L3, rx: f32, ry: f32, color: Rgb) -> Part {
        let (c, depth) = self.at(c);
        Part::new(Shape::Ellipse { c, rx, ry }, color, depth)
    }

    /// Screen row of height `up` above the figure's origin.
    fn row_of(&self, up: f32) -> f32 {
        self.ground - up
    }

    /// Whether a point on the surface of a ball centered at `center` is on
    /// the half facing the camera.
    fn visible(&self, point: L3, center: L3) -> bool {
        self.at(point).1 < self.at(center).1 - 0.3
    }
}

/// Body rise in pixels for the pose (horse and walker bob).
fn rise(pose: Pose, walk: f32, gallop: f32) -> f32 {
    use std::f32::consts::TAU;
    match pose {
        Pose::Idle => 0.0,
        Pose::Walk(t) => walk * (2.0 * TAU * t).sin(),
        Pose::Gallop(t) => gallop * (TAU * t + 0.6).sin(),
    }
}

/// A horse; `saddled` puts an empty saddle on it (the tethered horse).
fn horse(v: &View, pose: Pose, saddled: bool) -> Vec<Part> {
    let body = 33.0 + rise(pose, 0.6, 2.4);
    let nod = match pose {
        Pose::Gallop(t) => 2.5 * (std::f32::consts::TAU * t).cos(),
        _ => 0.0,
    };
    let mut parts = vec![
        v.capsule(
            l3(-11.0, 0.0, body),
            l3(9.0, 0.0, body + 1.0),
            9.5,
            9.5,
            HORSE,
        ),
        v.ball(l3(-13.0, 0.0, body + 2.0), 10.0, HORSE),
        v.ball(l3(11.0, 0.0, body), 9.0, HORSE),
        v.capsule(
            l3(13.0, 0.0, body + 5.0),
            l3(20.0, 0.0, body + 15.0 + nod),
            6.5,
            4.5,
            HORSE,
        ),
        // Mane along the crest of the neck.
        v.capsule(
            l3(10.5, 0.0, body + 12.0),
            l3(18.5, 0.0, body + 21.0 + nod),
            2.6,
            2.0,
            HORSE_DARK,
        )
        .nudge(0.4),
    ];
    let head = l3(21.0, 0.0, body + 18.0 + nod);
    let muzzle = l3(28.0, 0.0, body + 9.0 + nod);
    parts.push(v.capsule(head, muzzle, 4.8, 3.3, HORSE).nudge(-0.5));
    for side in [-1.8, 1.8] {
        parts.push(v.capsule(
            l3(19.5, side, body + 21.0 + nod),
            l3(18.5, side * 1.2, body + 26.0 + nod),
            1.3,
            0.8,
            HORSE,
        ));
        let eye = l3(24.0, side * 1.8, body + 15.5 + nod);
        if v.visible(eye, l3(24.0, 0.0, body + 15.5 + nod)) {
            parts.push(v.ball(eye, 0.8, EYES).flat().nudge(-8.0));
        }
    }
    if !v.away() {
        // A blaze would vanish against the dust: use the facing marker.
        let blaze = l3(28.0, 0.0, body + 10.5 + nod);
        parts.push(v.ball(blaze, 1.9, MARKER).flat().nudge(-6.0));
    }

    // Tail, swinging with the gait.
    let swish = match pose {
        Pose::Idle => 0.0,
        Pose::Walk(t) | Pose::Gallop(t) => 2.0 * (std::f32::consts::TAU * t).sin(),
    };
    let tail_end = match pose {
        Pose::Gallop(_) => l3(-31.0, swish, body),
        _ => l3(-25.0, swish, body - 14.0),
    };
    parts.push(v.capsule(l3(-22.0, 0.0, body + 6.0), tail_end, 2.8, 1.5, HORSE_DARK));

    // Legs: walk is a four-beat gait, gallop a transverse gallop.
    let legs = [
        // (fwd, right, walk offset, gallop offset, front)
        (10.0, -4.5, 0.25, 0.5, true),
        (10.0, 4.5, 0.75, 0.62, true),
        (-12.0, -4.5, 0.0, 0.0, false),
        (-12.0, 4.5, 0.5, 0.12, false),
    ];
    for (fwd, right, walk_offset, gallop_offset, front) in legs {
        let offset = match pose {
            Pose::Gallop(_) => gallop_offset,
            _ => walk_offset,
        };
        let (swing, lift) = pose.leg(offset, (0.3, 4.0), (0.62, 8.0));
        let hip = l3(fwd, right, body - 3.0);
        let hoof = l3(fwd + swing.sin() * 28.0, right, lift);
        let bend = if front { 0.5 * lift } else { -0.5 * lift };
        let knee = l3(
            fwd + swing.sin() * 14.0 + bend,
            right,
            (hip.up + hoof.up) / 2.0 - 1.0,
        );
        parts.push(v.capsule(hip, knee, 3.8, 2.5, HORSE));
        parts.push(v.capsule(knee, hoof, 2.3, 1.8, HORSE));
        parts.push(
            v.ball(l3(hoof.fwd, right, hoof.up + 1.2), 2.0, HOOF)
                .nudge(-0.2),
        );
    }

    if saddled {
        parts.extend(saddle(v, body));
    }
    parts
}

/// Saddle blanket and saddle on a horse whose barrel is at `body`.
fn saddle(v: &View, body: f32) -> [Part; 3] {
    let blanket = v
        .capsule(
            l3(-6.0, 0.0, body + 9.5),
            l3(3.0, 0.0, body + 9.5),
            4.0,
            4.0,
            SADDLE_BLANKET,
        )
        .nudge(-12.0);
    let stripe_row = v.row_of(body + 8.0);
    let stripe = blanket
        .rows(stripe_row - 0.8, stripe_row + 0.8)
        .nudge(-0.01);
    let stripe = Part {
        color: PONCHO_STRIPE,
        ..stripe
    };
    let seat = v
        .capsule(
            l3(-5.0, 0.0, body + 11.5),
            l3(1.0, 0.0, body + 11.5),
            2.6,
            2.6,
            SADDLE,
        )
        .nudge(-12.1);
    [blanket, stripe, seat]
}

/// A head at `center` with radius `r`: a face toward the camera unless the
/// figure faces away, then only hair.
fn head(v: &View, center: L3, r: f32, hair: Rgb) -> Vec<Part> {
    if v.away() {
        return vec![v.ball(center, r, hair)];
    }
    let mut parts = vec![
        // Hair cap on the back of the head shows round the face's edge.
        v.ball(
            l3(center.fwd - 1.3, center.right, center.up + 0.6),
            r * 0.95,
            hair,
        )
        .nudge(0.6),
        v.ball(center, r, SKIN),
    ];
    for side in [-1.0, 1.0] {
        let eye = l3(center.fwd + r * 0.82, side * r * 0.38, center.up + 0.4);
        if v.visible(eye, center) {
            parts.push(v.ball(eye, 0.7, EYES).flat().nudge(-r * 2.0));
        }
    }
    parts
}

/// Gaucho sitting on the horse: legs astride, poncho, head and hat.
fn mounted_gaucho(v: &View, pose: Pose) -> Vec<Part> {
    // The rider rides the horse's rise, a little softened.
    let lift = rise(pose, 0.6, 2.4) * 0.8;
    let mut parts = Vec::new();
    for side in [-1.0, 1.0] {
        let hip = l3(-2.0, side * 5.0, 46.0 + lift);
        let knee = l3(4.0, side * 8.5, 39.0 + lift);
        let ankle = l3(2.0, side * 8.5, 30.0 + lift);
        parts.push(v.capsule(hip, knee, 3.2, 2.8, TROUSERS).nudge(-10.0));
        parts.push(v.capsule(knee, ankle, 2.6, 2.3, TROUSERS).nudge(-10.0));
        parts.push(
            v.capsule(ankle, l3(4.5, side * 8.5, 28.0 + lift), 2.4, 2.2, BOOTS)
                .nudge(-10.1),
        );
    }
    let shoulders = l3(-2.5, 0.0, 56.5 + lift);
    let hem = l3(-2.0, 0.0, 47.0 + lift);
    let poncho = v.capsule(shoulders, hem, 6.5, 9.0, PONCHO).nudge(-14.0);
    let stripe_row = v.row_of(51.5 + lift);
    parts.push(poncho);
    parts.push(Part {
        color: PONCHO_STRIPE,
        ..poncho.rows(stripe_row - 1.0, stripe_row + 1.0).nudge(-0.01)
    });

    let (up, r) = RIDER_HEAD;
    let center = l3(-1.5, 0.0, up + lift);
    for part in head(v, center, r, HAIR) {
        parts.push(part.nudge(-15.0));
    }
    if !v.away() {
        let kerchief = v.ball(l3(2.0, 0.0, 58.8 + lift), 2.3, MARKER).flat();
        parts.push(kerchief.nudge(-16.0));
        let lip = l3(center.fwd + r * 0.9, 0.0, up - 1.6 + lift);
        if v.visible(lip, center) {
            let (a, b) = (l3(lip.fwd, -1.5, lip.up), l3(lip.fwd, 1.5, lip.up));
            parts.push(v.capsule(a, b, 0.7, 0.7, HAT).flat().nudge(-25.0));
        }
    }
    // Wide-brimmed hat.
    parts.push(
        v.disc(l3(-1.5, 0.0, up + 3.8 + lift), 9.0, 1.8, HAT)
            .nudge(-24.0),
    );
    parts.push(
        v.capsule(
            l3(-1.5, 0.0, up + 6.8 + lift),
            l3(-1.5, 0.0, up + 8.6 + lift),
            3.6,
            3.1,
            HAT,
        )
        .nudge(-24.1),
    );
    parts
}

#[cfg(test)]
mod tests {
    use super::super::paint::tones;
    use super::*;

    /// RGBA bytes of frame `(k, row)` of `pixels`, row-major.
    fn frame(kind: SheetKind, pixels: &[u8], k: usize, row: usize) -> Vec<u8> {
        let row_bytes = kind.sheet_size().0 * 4;
        (0..FRAME_SIZE)
            .flat_map(|y| {
                let start = (row * FRAME_SIZE + y) * row_bytes + k * FRAME_SIZE * 4;
                pixels[start..start + FRAME_SIZE * 4].to_vec()
            })
            .collect()
    }

    fn opaque(f: &[u8]) -> usize {
        f.chunks(4).filter(|p| p[3] == 255).count()
    }

    #[test]
    fn has_expected_size_and_rows() {
        for kind in SheetKind::all() {
            let (w, h) = kind.sheet_size();
            assert_eq!((w, h), (FRAME_SIZE * DIRECTIONS, FRAME_SIZE * kind.rows()));
            assert_eq!(sheet_pixels(kind).len(), w * h * 4, "{kind:?}");
        }
        assert_eq!(SheetKind::Rider.rows(), 9);
        assert_eq!(SheetKind::Horse.rows(), 9);
    }

    #[test]
    fn rows_follow_the_logical_animation_state() {
        use AnimationState::*;
        let rider = SheetKind::Rider;
        assert_eq!(rider.row(Idle, 3), 0);
        assert_eq!(
            (0..4).map(|f| rider.row(Walk, f)).collect::<Vec<_>>(),
            [1, 2, 3, 4]
        );
        assert_eq!(
            (0..4).map(|f| rider.row(Gallop, f)).collect::<Vec<_>>(),
            [5, 6, 7, 8]
        );
        assert_eq!(rider.row(Gallop, 6), 7, "wraps around the cycle");
    }

    #[test]
    fn every_frame_is_distinct_and_not_empty() {
        for kind in SheetKind::all() {
            let pixels = sheet_pixels(kind);
            for row in 0..kind.rows() {
                let frames: Vec<_> = (0..DIRECTIONS)
                    .map(|k| frame(kind, &pixels, k, row))
                    .collect();
                for (i, a) in frames.iter().enumerate() {
                    let n = opaque(a);
                    assert!(n > 400, "{kind:?} row {row} view {i} nearly empty ({n} px)");
                    for (j, b) in frames.iter().enumerate().skip(i + 1) {
                        assert!(a != b, "{kind:?} row {row}: views {i} and {j} match");
                    }
                }
            }
        }
    }

    #[test]
    fn gait_cycles_animate() {
        for kind in SheetKind::all() {
            let pixels = sheet_pixels(kind);
            for k in 0..DIRECTIONS {
                let frames: Vec<_> = (0..kind.rows())
                    .map(|row| frame(kind, &pixels, k, row))
                    .collect();
                for (i, a) in frames.iter().enumerate() {
                    for (j, b) in frames.iter().enumerate().skip(i + 1) {
                        assert!(a != b, "{kind:?} view {k}: rows {i} and {j} match");
                    }
                }
            }
        }
    }

    #[test]
    fn edges_are_outlined() {
        for kind in SheetKind::all() {
            let pixels = sheet_pixels(kind);
            let f = frame(kind, &pixels, 2, 0);
            let outline = f
                .chunks(4)
                .filter(|p| p[3] == 255 && p[..3] == super::super::paint::OUTLINE)
                .count();
            assert!(outline > 60, "{kind:?}: {outline} outline px");
            assert!(
                f.chunks(4).all(|p| p[3] == 0 || p[3] == 255),
                "masked alpha"
            );
        }
    }

    #[test]
    fn mirrored_side_views_point_opposite_ways() {
        // The facing marker sits left of center when facing left, right when facing right.
        for kind in SheetKind::all() {
            let pixels = sheet_pixels(kind);
            let centroid_x = |k: usize| {
                let f = frame(kind, &pixels, k, 0);
                let (mut sum, mut n) = (0usize, 0usize);
                for (i, p) in f.chunks(4).enumerate() {
                    if p[..3] == MARKER {
                        sum += i % FRAME_SIZE;
                        n += 1;
                    }
                }
                assert!(n > 0, "{kind:?} frame {k} has no facing marker");
                sum as f32 / n as f32
            };
            let center = FRAME_SIZE as f32 / 2.0;
            assert!(centroid_x(2) < center - 1.5, "{kind:?}");
            assert!(centroid_x(6) > center + 1.5, "{kind:?}");
        }
    }

    #[test]
    fn faces_show_from_the_front_and_never_from_behind() {
        let skin = tones(SKIN);
        for kind in SheetKind::all().filter(|k| *k != SheetKind::Horse) {
            let (up, r) = RIDER_HEAD;
            // Every row from the top of the frame down to the chin.
            let chin = (FRAME_SIZE as f32 - up + r + 2.0) as usize;
            let pixels = sheet_pixels(kind);
            let face = |k: usize, row: usize| {
                frame(kind, &pixels, k, row)[..chin * FRAME_SIZE * 4]
                    .chunks(4)
                    .filter(|p| {
                        p[3] == 255 && (skin.contains(&[p[0], p[1], p[2]]) || p[..3] == EYES)
                    })
                    .count()
            };
            for row in 0..kind.rows() {
                for k in [0, 1, 7] {
                    assert_eq!(
                        face(k, row),
                        0,
                        "{kind:?} back view {k} row {row} shows a face"
                    );
                }
                for k in [3, 4, 5] {
                    let n = face(k, row);
                    assert!(n > 6, "{kind:?} front view {k} row {row} has no face ({n})");
                }
            }
        }
    }

    #[test]
    fn standing_and_walking_feet_reach_the_bottom_rows() {
        for kind in SheetKind::all() {
            let pixels = sheet_pixels(kind);
            for row in 0..=GAIT_FRAMES {
                for k in 0..DIRECTIONS {
                    let f = frame(kind, &pixels, k, row);
                    let bottom = &f[(FRAME_SIZE - 3) * FRAME_SIZE * 4..];
                    assert!(opaque(bottom) > 0, "{kind:?} row {row} view {k} floats");
                }
            }
        }
    }

    #[test]
    fn the_saddled_horse_has_no_rider() {
        // Nothing above the horse's ears: the rider's hat is gone.
        let horse = sheet_pixels(SheetKind::Horse);
        let rider = sheet_pixels(SheetKind::Rider);
        let opaque_above = |kind, pixels: &[u8], rows: usize| {
            opaque(&frame(kind, pixels, 0, 0)[..rows * FRAME_SIZE * 4])
        };
        assert_eq!(opaque_above(SheetKind::Horse, &horse, 10), 0);
        assert!(opaque_above(SheetKind::Rider, &rider, 10) > 0);
    }
}
