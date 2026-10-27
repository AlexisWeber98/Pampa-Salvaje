//! Pure minimap drawing: the world-to-map mapping, the static town layer and
//! the live markers, all as plain RGBA8 pixel buffers (no Bevy assets).
//!
//! The map is north-up and frames the town plus a margin rather than the
//! whole 600 m patch, so streets stay legible at a small size; anything
//! outside (the player out on the pampa, the train far down the line) is
//! clamped to the edge or left off.

use bevy::math::Vec2;
use pampa_core::{
    Aabb, BuildingKind, Heading, Lagoon, PlaceKind, PlacedCar, Railway, RoadsideWreck, Settlement,
    StructureKind, TownLayout, Track, Vec2 as GroundVec2,
};

use crate::font::{self, GLYPH_HEIGHT};
use crate::terrain::hash2;

/// Meters of open ground shown around the town's extent.
pub const MARGIN: f32 = 30.0;
/// Half the side of the field frame, in meters: 640 m across, 2 m a pixel
/// on the 320-pixel map, about the scale of a big town's frame, so an
/// estancia's compound reads at a glance.
pub const FIELD_HALF_EXTENT: f32 = 320.0;
/// Inset, in pixels, that clamped markers keep from the map edge.
pub const EDGE: f32 = 6.0;

/// A square, north-up window onto the ground plane, `size_px` pixels wide.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapFrame {
    pub center: GroundVec2,
    /// Half the side of the framed square, in meters.
    pub half_extent: f32,
    pub size_px: usize,
}

impl MapFrame {
    /// The town's extent plus [`MARGIN`], squared up around its center.
    pub fn for_town(town: &TownLayout, size_px: usize) -> Self {
        let extent = town.extent();
        let size = extent.size();
        Self {
            center: extent.center(),
            half_extent: size.x.max(size.y) / 2.0 + MARGIN,
            size_px,
        }
    }

    /// The field frame: [`FIELD_HALF_EXTENT`] around `center` (the rider).
    pub fn field(center: GroundVec2, size_px: usize) -> Self {
        Self {
            center,
            half_extent: FIELD_HALF_EXTENT,
            size_px,
        }
    }

    /// The ground square it shows.
    pub fn shown(&self) -> Aabb {
        let side = 2.0 * self.half_extent;
        Aabb::from_center_size(self.center, GroundVec2::new(side, side))
    }

    pub fn meters_per_px(&self) -> f32 {
        2.0 * self.half_extent / self.size_px as f32
    }

    /// Map pixel position of a ground point: `(0, 0)` is the north-west
    /// corner, `x` grows east and `y` grows south.
    pub fn to_map(&self, p: GroundVec2) -> Vec2 {
        let scale = self.size_px as f32 / (2.0 * self.half_extent);
        Vec2::new(
            (p.x - (self.center.x - self.half_extent)) * scale,
            (self.center.y + self.half_extent - p.y) * scale,
        )
    }

    /// Whether a ground point falls on the map.
    pub fn contains(&self, p: GroundVec2) -> bool {
        (p.x - self.center.x).abs() <= self.half_extent
            && (p.y - self.center.y).abs() <= self.half_extent
    }

    /// [`Self::to_map`], pulled in to [`EDGE`] pixels from the border when it
    /// falls outside. Also says whether the point was on the map.
    pub fn to_map_clamped(&self, p: GroundVec2) -> (Vec2, bool) {
        let m = self.to_map(p);
        if self.contains(p) {
            return (m, true);
        }
        let far = self.size_px as f32 - EDGE;
        (Vec2::new(m.x.clamp(EDGE, far), m.y.clamp(EDGE, far)), false)
    }

    /// Map-pixel rectangle `(min, max)` covering a ground box.
    pub fn rect(&self, area: &pampa_core::Aabb) -> (Vec2, Vec2) {
        let a = self.to_map(GroundVec2::new(area.min.x, area.max.y));
        let b = self.to_map(GroundVec2::new(area.max.x, area.min.y));
        (a, b)
    }
}

/// An RGBA8 sRGB pixel buffer, row by row from the top.
#[derive(Debug, Clone, PartialEq)]
pub struct Canvas {
    pub size: usize,
    pub pixels: Vec<u8>,
}

impl Canvas {
    /// A transparent square canvas.
    pub fn new(size: usize) -> Self {
        Self {
            size,
            pixels: vec![0; size * size * 4],
        }
    }

    pub fn get(&self, x: usize, y: usize) -> [u8; 4] {
        let i = (y * self.size + x) * 4;
        [
            self.pixels[i],
            self.pixels[i + 1],
            self.pixels[i + 2],
            self.pixels[i + 3],
        ]
    }

    /// Sets one pixel; out-of-range coordinates are ignored.
    pub fn set(&mut self, x: i32, y: i32, color: [u8; 4]) {
        if x < 0 || y < 0 || x as usize >= self.size || y as usize >= self.size {
            return;
        }
        let i = (y as usize * self.size + x as usize) * 4;
        self.pixels[i..i + 4].copy_from_slice(&color);
    }

    /// Fills the pixels whose centers fall inside `[min, max)` (map pixels).
    pub fn fill_rect(&mut self, min: Vec2, max: Vec2, color: [u8; 4]) {
        let (x0, y0) = ((min.x - 0.5).ceil() as i32, (min.y - 0.5).ceil() as i32);
        let (x1, y1) = ((max.x - 0.5).ceil() as i32, (max.y - 0.5).ceil() as i32);
        for y in y0..y1.max(y0 + 1) {
            for x in x0..x1.max(x0 + 1) {
                self.set(x, y, color);
            }
        }
    }

    /// Makes every pixel transparent.
    pub fn clear(&mut self) {
        self.pixels.fill(0);
    }
}

/// Paper-map palette.
pub mod palette {
    pub const PAPER: [u8; 4] = [226, 210, 172, 255];
    pub const PAPER_EDGE: [u8; 4] = [196, 174, 130, 255];
    pub const STREET: [u8; 4] = [200, 176, 130, 255];
    pub const PLAZA: [u8; 4] = [150, 158, 98, 255];
    pub const HOUSE: [u8; 4] = [160, 98, 74, 255];
    pub const LANDMARK: [u8; 4] = [116, 52, 40, 255];
    pub const SHED: [u8; 4] = [128, 124, 116, 255];
    pub const WATER: [u8; 4] = [92, 124, 140, 255];
    pub const RAIL: [u8; 4] = [62, 50, 42, 255];
    /// Dirt roads: a warm brown, darker than the streets, lighter than the
    /// rail.
    pub const ROAD: [u8; 4] = [168, 124, 76, 255];
    pub const INK: [u8; 4] = [40, 30, 24, 255];
    pub const PLAYER: [u8; 4] = [196, 40, 28, 255];
    pub const WALKER: [u8; 4] = [48, 40, 96, 255];
    pub const TRAIN: [u8; 4] = [28, 26, 26, 255];
    pub const TARGET: [u8; 4] = [232, 176, 32, 255];
    /// Trees: one dark green dot each.
    pub const TREE: [u8; 4] = [96, 110, 60, 255];
    /// A settlement's huellas: a pale dirt line, lighter than the roads.
    pub const TRACK: [u8; 4] = [186, 152, 104, 255];
    /// Corral rails: a dark wood outline.
    pub const CORRAL: [u8; 4] = [104, 78, 50, 255];
    /// Haystacks.
    pub const HAY: [u8; 4] = [194, 172, 92, 255];
}

/// What a layer shows beyond its town: the railway line's links, the
/// roads' straight pieces, and the settlements and roadside wrecks, each
/// painted where it crosses the frame.
#[derive(Debug, Clone, Copy, Default)]
pub struct Overlay<'a> {
    pub links: &'a [Railway],
    pub roads: &'a [(GroundVec2, GroundVec2)],
    pub settlements: &'a [Settlement],
    pub wrecks: &'a [RoadsideWreck],
    /// Lagoons, painted as water under everything else.
    pub lagoons: &'a [Lagoon],
    /// Platforms that belong to no town's buildings (the bays'), painted
    /// like the towns' platforms.
    pub platforms: &'a [Aabb],
}

/// The color of a platform.
const PLATFORM: [u8; 4] = [184, 174, 152, 255];

/// Pixel scale of place initials on the map.
pub const LABEL_SCALE: usize = 2;

/// Paints the town once: paper, streets, plaza, railway, the railway line's
/// `links` where they cross the frame, buildings colored by kind and each
/// named place's initial over it.
pub fn rasterize_town(town: &TownLayout, links: &[Railway], frame: &MapFrame) -> Canvas {
    rasterize_town_with_roads(town, links, &[], frame)
}

/// [`rasterize_town`], with the dirt `roads` (straight pieces of their
/// centerlines) painted under the railway where they cross the frame.
pub fn rasterize_town_with_roads(
    town: &TownLayout,
    links: &[Railway],
    roads: &[(GroundVec2, GroundVec2)],
    frame: &MapFrame,
) -> Canvas {
    let overlay = Overlay {
        links,
        roads,
        ..Overlay::default()
    };
    rasterize_town_over(town, &overlay, frame)
}

/// [`rasterize_town_with_roads`], with everything of `overlay` that
/// crosses the frame: settlements just outside the town show on its map.
pub fn rasterize_town_over(town: &TownLayout, overlay: &Overlay, frame: &MapFrame) -> Canvas {
    let mut map = paper(frame.size_px);
    paint_town_ground(&mut map, town, frame);
    paint_overlay_under(&mut map, overlay, frame);
    paint_rails(
        &mut map,
        std::slice::from_ref(town),
        overlay.links,
        frame,
        true,
    );
    paint_settlements(&mut map, overlay, frame);
    paint_town_buildings(&mut map, town, frame);
    paint_platforms(&mut map, overlay, frame);
    for place in town.places() {
        let text = place_initial(place.kind).to_string();
        label(&mut map, &text, frame.to_map(place.center()));
    }
    // North marker in the north-west corner, the town's name along the top.
    label(&mut map, "N", Vec2::new(16.0, 18.0));
    title(&mut map, &town.name);
    map
}

/// The field layer: open country round the rider, with the roads, the
/// railway, the settlements and any of `towns` reaching into the frame
/// (streets and buildings, without labels or a title).
pub fn rasterize_field(towns: &[TownLayout], overlay: &Overlay, frame: &MapFrame) -> Canvas {
    let mut map = paper(frame.size_px);
    let shown = frame.shown();
    let near: Vec<&TownLayout> = towns
        .iter()
        .filter(|t| t.extent().intersects(&shown))
        .collect();
    for town in &near {
        paint_town_ground(&mut map, town, frame);
    }
    paint_overlay_under(&mut map, overlay, frame);
    for town in &near {
        if let Some(rail) = town.railway {
            track_line(
                &mut map,
                frame.to_map(rail.start),
                frame.to_map(rail.end),
                false,
            );
        }
    }
    // No sleeper ticks out here: at 2 m a pixel, shrunk onto the panel,
    // they read as a dashed line.
    paint_rails(&mut map, &[], overlay.links, frame, false);
    paint_settlements(&mut map, overlay, frame);
    for town in &near {
        paint_town_buildings(&mut map, town, frame);
    }
    paint_platforms(&mut map, overlay, frame);
    label(&mut map, "N", Vec2::new(16.0, 18.0));
    map
}

/// Aged paper, darker toward the edge.
fn paper(size: usize) -> Canvas {
    let mut map = Canvas::new(size);
    let border = 5;
    for y in 0..size {
        for x in 0..size {
            let edge = x.min(y).min(size - 1 - x).min(size - 1 - y) < border;
            let base = if edge {
                palette::PAPER_EDGE
            } else {
                palette::PAPER
            };
            let f = 0.97 + 0.04 * hash2(x as f32, y as f32);
            let c = std::array::from_fn(|i| {
                if i == 3 {
                    255
                } else {
                    (f32::from(base[i]) * f).min(255.0) as u8
                }
            });
            map.set(x as i32, y as i32, c);
        }
    }
    map
}

/// A town's streets, plazas and trees.
fn paint_town_ground(map: &mut Canvas, town: &TownLayout, frame: &MapFrame) {
    for street in &town.streets {
        let (a, b) = frame.rect(street);
        map.fill_rect(a, b, palette::STREET);
    }
    for plaza in town.plazas() {
        let (a, b) = frame.rect(&plaza);
        map.fill_rect(a, b, palette::PLAZA);
    }
    for &tree in &town.trees {
        let m = frame.to_map(tree);
        map.set(m.x as i32, m.y as i32, palette::TREE);
    }
}

/// A town's buildings, colored by kind.
fn paint_town_buildings(map: &mut Canvas, town: &TownLayout, frame: &MapFrame) {
    for building in &town.buildings {
        let color = match building.kind {
            BuildingKind::House => palette::HOUSE,
            BuildingKind::GeneralStore
            | BuildingKind::PoliceStation
            | BuildingKind::Church
            | BuildingKind::Station => palette::LANDMARK,
            BuildingKind::Platform => PLATFORM,
            BuildingKind::Shed | BuildingKind::GrainStore => palette::SHED,
            BuildingKind::WaterTank => palette::WATER,
            BuildingKind::Windmill => palette::INK,
        };
        let (a, b) = frame.rect(&building.footprint);
        map.fill_rect(a, b, color);
    }
}

/// The overlay's own platforms (the bays'), where they cross the frame.
fn paint_platforms(map: &mut Canvas, overlay: &Overlay, frame: &MapFrame) {
    for platform in overlay.platforms {
        let (a, b) = frame.rect(platform);
        map.fill_rect(a, b, PLATFORM);
    }
}

/// The roads, then the settlements' huellas, cut to the frame: under the
/// railway and every building.
fn paint_overlay_under(map: &mut Canvas, overlay: &Overlay, frame: &MapFrame) {
    let shown = frame.shown();
    let px = frame.meters_per_px();
    for lagoon in overlay.lagoons {
        paint_lagoon(map, lagoon, frame);
    }
    let half = (pampa_core::RoadNetwork::HALF_WIDTH / px).max(1.0);
    for &(a, b) in overlay.roads {
        ground_line(map, frame, (a, b), half, palette::ROAD);
    }
    // Thin: a pixel wide on the field frame, at most two on a town's.
    let half = (Track::HALF_WIDTH / px).clamp(0.5, 1.0);
    for settlement in overlay.settlements {
        if !settlement.bounds.intersects(&shown) {
            continue;
        }
        for track in &settlement.tracks {
            for piece in track.segments() {
                ground_line(map, frame, piece, half, palette::TRACK);
            }
        }
    }
}

/// A lagoon's water, pixel by pixel over its box where it crosses the
/// frame: a pixel is water when its center is.
fn paint_lagoon(map: &mut Canvas, lagoon: &Lagoon, frame: &MapFrame) {
    let shown = frame.shown();
    let water = lagoon.bounds();
    if !water.intersects(&shown) {
        return;
    }
    let (a, b) = frame.rect(&water);
    let px = frame.meters_per_px();
    let size = map.size as i32;
    let (x0, x1) = ((a.x.floor() as i32).max(0), (b.x.ceil() as i32).min(size));
    let (y0, y1) = ((a.y.floor() as i32).max(0), (b.y.ceil() as i32).min(size));
    let corner = GroundVec2::new(
        frame.center.x - frame.half_extent,
        frame.center.y + frame.half_extent,
    );
    for y in y0..y1 {
        for x in x0..x1 {
            let g = corner + GroundVec2::new((x as f32 + 0.5) * px, -(y as f32 + 0.5) * px);
            if lagoon.is_water(g) {
                map.set(x, y, palette::WATER);
            }
        }
    }
}

/// A straight ground line from `a` to `b`, cut to the frame, `half` map
/// pixels to either side.
fn ground_line(
    map: &mut Canvas,
    frame: &MapFrame,
    (a, b): (GroundVec2, GroundVec2),
    half: f32,
    color: [u8; 4],
) {
    let Some((enter, exit)) = frame.shown().segment_overlap(a, b) else {
        return;
    };
    let (s, e) = (
        frame.to_map(a + (b - a) * enter),
        frame.to_map(a + (b - a) * exit),
    );
    let steps = s.distance(e).ceil() as usize;
    for k in 0..=steps {
        let p = s.lerp(e, k as f32 / steps.max(1) as f32);
        map.fill_rect(p - Vec2::splat(half), p + Vec2::splat(half), color);
    }
}

/// Railway: each town's track, then the links of the railway line, cut to
/// the frame (they run for kilometers); with sleeper ticks when `ties`.
fn paint_rails(
    map: &mut Canvas,
    towns: &[TownLayout],
    links: &[Railway],
    frame: &MapFrame,
    ties: bool,
) {
    let shown = frame.shown();
    for rail in towns.iter().filter_map(|t| t.railway) {
        track_line(map, frame.to_map(rail.start), frame.to_map(rail.end), ties);
    }
    for link in links {
        if let Some((enter, exit)) = shown.segment_overlap(link.start, link.end) {
            let along = link.end - link.start;
            let (from, to) = (link.start + along * enter, link.start + along * exit);
            track_line(map, frame.to_map(from), frame.to_map(to), ties);
        }
    }
}

/// The map color of a settlement structure filled over its footprint;
/// `None` for corrals (drawn as an outline) and trees (a dot).
pub fn structure_color(kind: StructureKind) -> Option<[u8; 4]> {
    Some(match kind {
        StructureKind::MainHouse(_) | StructureKind::WorkerHouse(_) => palette::HOUSE,
        StructureKind::Barn
        | StructureKind::Cart
        | StructureKind::WreckedWagon
        | StructureKind::HitchingPost => palette::SHED,
        StructureKind::Tank | StructureKind::Trough | StructureKind::Well => palette::WATER,
        StructureKind::Windpump => palette::INK,
        StructureKind::Haystack => palette::HAY,
        StructureKind::Corral | StructureKind::Eucalyptus | StructureKind::Poplar => return None,
    })
}

/// The settlements reaching into the frame: footprints by kind, corrals
/// as rail outlines, trees as dots; then the roadside wrecks.
fn paint_settlements(map: &mut Canvas, overlay: &Overlay, frame: &MapFrame) {
    let shown = frame.shown();
    for settlement in overlay.settlements {
        if !settlement.bounds.intersects(&shown) {
            continue;
        }
        for s in &settlement.structures {
            let (a, b) = frame.rect(&s.footprint);
            match structure_color(s.kind) {
                Some(color) => map.fill_rect(a, b, color),
                None if s.kind == StructureKind::Corral => outline(map, a, b, palette::CORRAL),
                None => {
                    let m = frame.to_map(s.footprint.center());
                    map.set(m.x as i32, m.y as i32, palette::TREE);
                }
            }
        }
    }
    for wreck in overlay.wrecks {
        let footprint = wreck.footprint();
        if footprint.intersects(&shown) {
            let (a, b) = frame.rect(&footprint);
            map.fill_rect(a, b, palette::SHED);
        }
    }
}

/// A one-pixel outline of the map rectangle `[min, max)`.
fn outline(map: &mut Canvas, min: Vec2, max: Vec2, color: [u8; 4]) {
    map.fill_rect(min, Vec2::new(max.x, min.y + 1.0), color);
    map.fill_rect(Vec2::new(min.x, max.y - 1.0), max, color);
    map.fill_rect(min, Vec2::new(min.x + 1.0, max.y), color);
    map.fill_rect(Vec2::new(max.x - 1.0, min.y), max, color);
}

/// A track from map pixel `s` to `e`: an unbroken dark line two pixels
/// wide, with sleeper ticks across it every third pixel when `ties`, both
/// oriented to the track's main direction.
fn track_line(map: &mut Canvas, s: Vec2, e: Vec2, ties: bool) {
    // Half sizes of a line dot and a tick, oriented to the track.
    let east_west = (e.x - s.x).abs() >= (e.y - s.y).abs();
    let (line, tick) = if east_west {
        (Vec2::new(0.5, 1.0), Vec2::new(0.5, 2.0))
    } else {
        (Vec2::new(1.0, 0.5), Vec2::new(2.0, 0.5))
    };
    // Half-pixel steps: no column or row is ever skipped.
    let steps = (s.distance(e) * 2.0).ceil() as usize;
    for k in 0..=steps {
        let p = s.lerp(e, k as f32 / steps.max(1) as f32);
        map.fill_rect(p - line, p + line, palette::RAIL);
        if ties && k % 6 == 0 {
            map.fill_rect(p - tick, p + tick, palette::RAIL);
        }
    }
}

/// Writes the town's name, in capitals, centered along the top edge; at
/// half size when it is too long for the map.
fn title(map: &mut Canvas, name: &str) {
    let text = name.to_uppercase();
    let size = map.size as f32;
    let fits = (font::text_width(&text) * LABEL_SCALE) as f32 <= size - 48.0;
    let scale = if fits { LABEL_SCALE } else { 1 };
    label_scaled(map, &text, Vec2::new(size / 2.0, 18.0), scale);
}

/// Writes `text` in ink at [`LABEL_SCALE`], centered on `center`, over a
/// one-pixel paper halo so it reads on top of buildings.
fn label(map: &mut Canvas, text: &str, center: Vec2) {
    label_scaled(map, text, center, LABEL_SCALE);
}

/// [`label`] at pixel scale `scale`.
fn label_scaled(map: &mut Canvas, text: &str, center: Vec2, scale: usize) {
    let s = scale as i32;
    let (first, last) = font::ink_rows(text).unwrap_or((0, GLYPH_HEIGHT - 1));
    let w = font::text_width(text) as i32 * s;
    let h = (last - first + 1) as i32 * s;
    let (left, top) = (center.x as i32 - w / 2, center.y as i32 - h / 2);
    let mut pixels = Vec::new();
    font::for_each_pixel(text, |x, y| {
        for dy in 0..s {
            for dx in 0..s {
                pixels.push((left + x as i32 * s + dx, top + (y - first) as i32 * s + dy));
            }
        }
    });
    for &(x, y) in &pixels {
        for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            map.set(x + dx, y + dy, palette::PAPER);
        }
    }
    for (x, y) in pixels {
        map.set(x, y, palette::INK);
    }
}

/// Radius of the compass rose's ring, in map pixels.
pub const COMPASS_RADIUS: f32 = 17.0;

/// Where the compass rose stands: the map's bottom-left corner, clear of
/// the title along the top.
pub fn compass_center(size_px: usize) -> Vec2 {
    let inset = COMPASS_RADIUS + 8.0;
    Vec2::new(inset, size_px as f32 - inset)
}

/// The compass rose's four letters, N, E, S and O (oeste), each with the
/// unit direction it points in, in map pixels, taken from `frame` itself:
/// the map's own way of showing world north (ground `+y`), east, south and
/// west, so a map that turned would turn its rose with it.
pub fn compass_marks(frame: &MapFrame) -> [(char, Vec2); 4] {
    let toward = |d: GroundVec2| {
        let step = d * frame.half_extent.max(1.0);
        (frame.to_map(frame.center + step) - frame.to_map(frame.center)).normalize_or_zero()
    };
    [
        ('N', toward(GroundVec2::new(0.0, 1.0))),
        ('E', toward(GroundVec2::new(1.0, 0.0))),
        ('S', toward(GroundVec2::new(0.0, -1.0))),
        ('O', toward(GroundVec2::new(-1.0, 0.0))),
    ]
}

/// Draws the compass rose at [`compass_center`]: a paper disc in an ink
/// ring, a red needle toward north, and the letters of
/// [`compass_marks`] inside the ring.
pub fn draw_compass(canvas: &mut Canvas, frame: &MapFrame) {
    let c = compass_center(canvas.size);
    let r = COMPASS_RADIUS;
    let (cx, cy) = (c.x.floor() as i32, c.y.floor() as i32);
    let reach = r as i32 + 1;
    for y in cy - reach..=cy + reach {
        for x in cx - reach..=cx + reach {
            let d = Vec2::new(x as f32 + 0.5, y as f32 + 0.5).distance(c);
            if d <= r - 1.0 {
                canvas.set(x, y, palette::PAPER);
            } else if d <= r + 0.5 {
                canvas.set(x, y, palette::INK);
            }
        }
    }
    let marks = compass_marks(frame);
    // A short red needle from the middle toward north.
    let north = marks[0].1;
    for k in 1..=5 {
        let p = c + north * k as f32;
        canvas.set(p.x.floor() as i32, p.y.floor() as i32, palette::PLAYER);
    }
    for (letter, d) in marks {
        label_scaled(canvas, &letter.to_string(), c + d * (r * 0.6), 1);
    }
}

/// The map letter of a place: the initial of its sign name.
pub fn place_initial(kind: PlaceKind) -> char {
    kind.name().chars().next().unwrap_or(' ')
}

/// Live things to mark on the map this frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Markers {
    pub player: Option<(GroundVec2, Heading)>,
    pub walkers: Vec<GroundVec2>,
    pub cars: Vec<PlacedCar>,
    /// Where the errand sends the player, if anywhere.
    pub target: Option<GroundVec2>,
}

/// Clears `canvas` and draws the markers: train cars as dark bars, walkers
/// as dots (skipped off the map), the errand target as a gold diamond and
/// the player as an arrow along its heading. The target and the player are
/// clamped to the edge when outside, so they point the way. Returns the
/// player's marker position, if any.
pub fn draw_markers(canvas: &mut Canvas, frame: &MapFrame, markers: &Markers) -> Option<Vec2> {
    canvas.clear();
    draw_compass(canvas, frame);
    let px = frame.meters_per_px();
    for car in markers
        .cars
        .iter()
        .filter(|c| c.on_rail && frame.contains(c.center))
    {
        let c = frame.to_map(car.center);
        let dir = Vec2::new(car.direction.x, -car.direction.y);
        let half = car.length / px / 2.0;
        let steps = (2.0 * half).ceil() as i32;
        for k in 0..=steps {
            let p = c + dir * (k as f32 - steps as f32 / 2.0) * (2.0 * half / steps.max(1) as f32);
            square(canvas, p, 1, palette::TRAIN);
        }
    }
    for &walker in markers.walkers.iter().filter(|&&w| frame.contains(w)) {
        square(canvas, frame.to_map(walker), 1, palette::WALKER);
    }
    if let Some(target) = markers.target {
        let (at, _) = frame.to_map_clamped(target);
        diamond(canvas, at);
    }
    let (position, heading) = markers.player?;
    let (at, _) = frame.to_map_clamped(position);
    arrow(canvas, at, heading);
    Some(at)
}

/// A filled square of `2 * radius + 1` pixels around the pixel holding `p`.
fn square(canvas: &mut Canvas, p: Vec2, radius: i32, color: [u8; 4]) {
    let (cx, cy) = (p.x.floor() as i32, p.y.floor() as i32);
    for y in cy - radius..=cy + radius {
        for x in cx - radius..=cx + radius {
            canvas.set(x, y, color);
        }
    }
}

/// The errand target: a gold diamond outlined in ink, centered on `at`.
fn diamond(canvas: &mut Canvas, at: Vec2) {
    let (cx, cy) = (at.x.floor() as i32, at.y.floor() as i32);
    let r = 5;
    for y in cy - r - 1..=cy + r + 1 {
        for x in cx - r - 1..=cx + r + 1 {
            let d = (x - cx).abs() + (y - cy).abs();
            if d < r {
                canvas.set(x, y, palette::TARGET);
            } else if d <= r {
                canvas.set(x, y, palette::INK);
            }
        }
    }
}

/// The player's arrow: a red triangle outlined in ink, tip along `heading`.
fn arrow(canvas: &mut Canvas, at: Vec2, heading: Heading) {
    let f = heading.forward();
    // Map y grows south.
    let ahead = Vec2::new(f.x, -f.y);
    let side = Vec2::new(-ahead.y, ahead.x);
    let tip = at + ahead * 8.0;
    let (left, right) = (at - ahead * 4.0 + side * 5.0, at - ahead * 4.0 - side * 5.0);
    let inside = |x: i32, y: i32| {
        let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
        let edge = |a: Vec2, b: Vec2| (b - a).perp_dot(p - a);
        let (e0, e1, e2) = (edge(tip, left), edge(left, right), edge(right, tip));
        (e0 >= 0.0 && e1 >= 0.0 && e2 >= 0.0) || (e0 <= 0.0 && e1 <= 0.0 && e2 <= 0.0)
    };
    let (cx, cy) = (at.x as i32, at.y as i32);
    let r = 10;
    for y in cy - r..=cy + r {
        for x in cx - r..=cx + r {
            if inside(x, y) {
                canvas.set(x, y, palette::PLAYER);
            } else if [(-1, 0), (1, 0), (0, -1), (0, 1)]
                .iter()
                .any(|&(dx, dy)| inside(x + dx, y + dy))
            {
                canvas.set(x, y, palette::INK);
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use pampa_core::CarKind;

    fn town() -> TownLayout {
        TownLayout::castex_inspired()
    }

    fn close(a: Vec2, b: Vec2) -> bool {
        a.distance(b) < 1e-3
    }

    fn opaque_near(canvas: &Canvas, at: Vec2, radius: i32) -> usize {
        let (cx, cy) = (at.x as i32, at.y as i32);
        let mut n = 0;
        for y in cy - radius..=cy + radius {
            for x in cx - radius..=cx + radius {
                if x >= 0 && y >= 0 && (x as usize) < canvas.size && (y as usize) < canvas.size {
                    n += usize::from(canvas.get(x as usize, y as usize)[3] > 0);
                }
            }
        }
        n
    }

    #[test]
    fn frames_the_town_with_a_margin_in_a_square() {
        let t = town();
        let frame = MapFrame::for_town(&t, 320);
        let extent = t.extent();
        for corner in [extent.min, extent.max] {
            assert!(frame.contains(corner));
            let p = frame.to_map(corner);
            let margin_px = MARGIN / frame.meters_per_px();
            assert!(
                p.x >= margin_px - 1e-3 && p.x <= 320.0 - margin_px + 1e-3,
                "{p:?}"
            );
            assert!(
                p.y >= margin_px - 1e-3 && p.y <= 320.0 - margin_px + 1e-3,
                "{p:?}"
            );
        }
        assert!(close(frame.to_map(extent.center()), Vec2::splat(160.0)));
        assert!(frame.half_extent >= extent.size().x.max(extent.size().y) / 2.0 + MARGIN);
    }

    #[test]
    fn maps_corners_and_center_north_up() {
        let frame = MapFrame {
            center: GroundVec2::new(10.0, 20.0),
            half_extent: 100.0,
            size_px: 200,
        };
        assert_eq!(frame.meters_per_px(), 1.0);
        assert!(close(
            frame.to_map(GroundVec2::new(10.0, 20.0)),
            Vec2::new(100.0, 100.0)
        ));
        // North-west corner at the top left, south-east at the bottom right.
        assert!(close(
            frame.to_map(GroundVec2::new(-90.0, 120.0)),
            Vec2::ZERO
        ));
        assert!(close(
            frame.to_map(GroundVec2::new(110.0, -80.0)),
            Vec2::new(200.0, 200.0)
        ));
        // East is right, north is up.
        assert!(close(
            frame.to_map(GroundVec2::new(15.0, 30.0)),
            Vec2::new(105.0, 90.0)
        ));
        assert!(frame.contains(GroundVec2::new(109.0, -79.0)));
        assert!(!frame.contains(GroundVec2::new(111.0, 0.0)));
        assert!(!frame.contains(GroundVec2::new(0.0, 121.0)));
    }

    #[test]
    fn clamps_outside_points_to_the_edge() {
        let frame = MapFrame {
            center: GroundVec2::ZERO,
            half_extent: 100.0,
            size_px: 200,
        };
        let inside = GroundVec2::new(30.0, -40.0);
        assert_eq!(frame.to_map_clamped(inside), (frame.to_map(inside), true));
        let (east, on_map) = frame.to_map_clamped(GroundVec2::new(250.0, 10.0));
        assert!(!on_map);
        assert!(close(east, Vec2::new(200.0 - EDGE, 90.0)));
        let (south_west, _) = frame.to_map_clamped(GroundVec2::new(-300.0, -300.0));
        assert!(close(south_west, Vec2::new(EDGE, 200.0 - EDGE)));
    }

    #[test]
    fn a_very_big_town_paints_its_merged_and_station_plazas() {
        let t = TownLayout::generate(&pampa_core::TownConfig {
            blocks: (8, 8),
            ..pampa_core::TownConfig::castex()
        })
        .unwrap();
        let frame = MapFrame::for_town(&t, 320);
        let map = rasterize_town(&t, &[], &frame);
        let at = |p: GroundVec2| {
            let m = frame.to_map(p);
            map.get(m.x as usize, m.y as usize)
        };
        let station = t.station_plaza.expect("a station plaza");
        let spot = GroundVec2::new(station.min.x + 2.0, station.min.y + 6.0);
        assert_eq!(at(spot)[..3], palette::PLAZA[..3]);
        // Where the inner streets crossed, the merged plaza is lawn, not road;
        // off the central path and monument.
        let c = t.plaza.center();
        let former_street = GroundVec2::new(c.x + 12.0, c.y + 3.5);
        assert_eq!(at(former_street)[..3], palette::PLAZA[..3]);
    }

    #[test]
    fn the_railway_line_is_painted_where_it_crosses_the_frame() {
        let t = town();
        let frame = MapFrame::for_town(&t, 320);
        let tip = t.railway.unwrap().end;
        // A link leaving the east tip westward, slowly climbing north over
        // the open ground beyond the town's track, and one far away.
        let link = Railway {
            start: tip,
            end: GroundVec2::new(-600.0, tip.y + 36.0),
            gauge: 1.676,
        };
        let far = Railway {
            start: GroundVec2::new(3000.0, 3000.0),
            end: GroundVec2::new(3500.0, 3600.0),
            gauge: 1.676,
        };
        let on_link = |x: f32| {
            let f = (x - link.start.x) / (link.end.x - link.start.x);
            link.start + (link.end - link.start) * f
        };
        // Both east of the title along the top edge.
        let (middle, east) = (on_link(100.0), on_link(150.0));
        assert!(frame.contains(middle) && frame.contains(east));
        assert!(middle.y > t.extent().max.y + 5.0, "{middle:?}");
        let at = |map: &Canvas, p: GroundVec2| {
            let m = frame.to_map(p);
            map.get(m.x as usize, m.y as usize)
        };
        let without = rasterize_town(&t, &[], &frame);
        assert_ne!(at(&without, middle)[..3], palette::RAIL[..3]);
        let with = rasterize_town(&t, &[link, far], &frame);
        assert_eq!(at(&with, middle)[..3], palette::RAIL[..3]);
        assert_eq!(at(&with, east)[..3], palette::RAIL[..3]);
        // The far link leaves the map as it was.
        assert_eq!(rasterize_town(&t, &[far], &frame), without);
    }

    #[test]
    fn roads_are_painted_in_dirt_cut_to_the_frame_under_the_rail() {
        let t = town();
        let frame = MapFrame::for_town(&t, 320);
        // A road leaving the grid's south-west corner diagonally, out of
        // the frame, and one far away.
        let grid = t.grid();
        let from = grid.min - GroundVec2::new(5.0, 5.0);
        let road = (from, from - GroundVec2::new(400.0, 300.0));
        let far = (
            GroundVec2::new(5000.0, 5000.0),
            GroundVec2::new(5200.0, 5000.0),
        );
        let without = rasterize_town_with_roads(&t, &[], &[], &frame);
        assert_eq!(without, rasterize_town(&t, &[], &frame));
        let with = rasterize_town_with_roads(&t, &[], &[road, far], &frame);
        let inside = road.0 + (road.1 - road.0) * 0.04;
        assert!(frame.contains(inside));
        let m = frame.to_map(inside);
        assert_eq!(
            with.get(m.x as usize, m.y as usize)[..3],
            palette::ROAD[..3]
        );
        assert_ne!(palette::ROAD[..3], palette::RAIL[..3]);
        assert_ne!(palette::ROAD[..3], palette::STREET[..3]);
        let painted = with
            .pixels
            .chunks(4)
            .filter(|p| p[..3] == palette::ROAD[..3])
            .count();
        assert!(painted > 30, "{painted}");
        assert_eq!(rasterize_town_with_roads(&t, &[], &[far], &frame), without);
    }

    #[test]
    fn town_layer_shows_streets_plaza_railway_and_place_letters() {
        let t = town();
        let frame = MapFrame::for_town(&t, 320);
        let map = rasterize_town(&t, &[], &frame);
        assert_eq!(map.pixels.len(), 320 * 320 * 4);
        assert_eq!(map, rasterize_town(&t, &[], &frame), "deterministic");
        assert!(map.pixels.chunks(4).all(|p| p[3] == 255), "opaque paper");

        let at = |p: GroundVec2| {
            let m = frame.to_map(p);
            map.get(m.x as usize, m.y as usize)
        };
        // A point on a street but off the plaza and buildings.
        let street = t.streets[0];
        let on_street = GroundVec2::new(street.center().x, t.grid().min.y + 3.0);
        assert_eq!(at(on_street)[..3], palette::STREET[..3]);
        // Plaza: a spot between its trees and the central label.
        let plaza = t.plaza;
        assert_eq!(
            at(GroundVec2::new(plaza.min.x + 2.0, plaza.min.y + 6.0))[..3],
            palette::PLAZA[..3]
        );
        // The railway line is drawn where it crosses the frame.
        let rail = GroundVec2::new(frame.center.x + 100.0, t.railway.unwrap().start.y);
        assert_eq!(at(rail)[..3], palette::RAIL[..3]);
        // A house is painted as a house.
        let house = t
            .buildings
            .iter()
            .find(|b| b.kind == BuildingKind::House)
            .unwrap();
        let corner = GroundVec2::new(house.footprint.min.x + 1.0, house.footprint.min.y + 1.0);
        assert_eq!(at(corner)[..3], palette::HOUSE[..3]);
        // Every place carries its initial in ink, inside the map.
        for place in t.places() {
            let c = frame.to_map(place.center());
            assert!(c.x > 0.0 && c.y > 0.0 && c.x < 320.0 && c.y < 320.0);
            let mut ink = 0;
            for y in (c.y as usize - 8)..(c.y as usize + 8) {
                for x in (c.x as usize - 8)..(c.x as usize + 8) {
                    ink += usize::from(map.get(x, y) == palette::INK);
                }
            }
            assert!(ink >= 8, "{:?} label has {ink} ink pixels", place.kind);
        }
        assert_eq!(place_initial(PlaceKind::GeneralStore), 'A');
        assert_eq!(place_initial(PlaceKind::Plaza), 'P');
    }

    #[test]
    fn draws_the_player_arrow_pointing_along_its_heading() {
        let frame = MapFrame {
            center: GroundVec2::ZERO,
            half_extent: 100.0,
            size_px: 200,
        };
        let mut canvas = Canvas::new(200);
        let position = GroundVec2::new(20.0, 10.0);
        let north = Heading::from_radians(std::f32::consts::FRAC_PI_2);
        let markers = Markers {
            player: Some((position, north)),
            ..Markers::default()
        };
        let at = draw_markers(&mut canvas, &frame, &markers).unwrap();
        assert!(close(at, frame.to_map(position)));
        assert!(opaque_near(&canvas, at, 2) > 0);
        // More of the arrow lies ahead (up, for north) than behind.
        let (above, below) = (
            opaque_near(&canvas, at - Vec2::new(0.0, 5.0), 1),
            opaque_near(&canvas, at + Vec2::new(0.0, 5.0), 1),
        );
        assert!(above > below, "above {above} below {below}");
        // Turned east, the tip points right.
        let east = Markers {
            player: Some((position, Heading::from_radians(0.0))),
            ..Markers::default()
        };
        draw_markers(&mut canvas, &frame, &east);
        let right = opaque_near(&canvas, at + Vec2::new(5.0, 0.0), 1);
        let left = opaque_near(&canvas, at - Vec2::new(5.0, 0.0), 1);
        assert!(right > left, "right {right} left {left}");
        // Redrawing clears the previous frame.
        assert_eq!(opaque_near(&canvas, at - Vec2::new(0.0, 7.0), 0), 0);
    }

    #[test]
    fn marks_walkers_and_train_cars_and_clamps_the_player() {
        let frame = MapFrame {
            center: GroundVec2::ZERO,
            half_extent: 100.0,
            size_px: 200,
        };
        let mut canvas = Canvas::new(200);
        let walker = GroundVec2::new(-50.0, -50.0);
        let car = PlacedCar {
            kind: CarKind::Boxcar,
            center: GroundVec2::new(0.0, 60.0),
            direction: GroundVec2::new(1.0, 0.0),
            length: 14.0,
            on_rail: true,
        };
        let away = PlacedCar {
            center: GroundVec2::new(-60.0, 80.0),
            on_rail: false,
            ..car
        };
        let markers = Markers {
            player: Some((GroundVec2::new(500.0, 0.0), Heading::from_radians(0.0))),
            walkers: vec![walker, GroundVec2::new(400.0, 400.0)],
            cars: vec![car, away],
            target: None,
        };
        let player = draw_markers(&mut canvas, &frame, &markers).unwrap();
        assert!(close(player, Vec2::new(200.0 - EDGE, 100.0)));
        let w = frame.to_map(walker);
        assert_eq!(canvas.get(w.x as usize, w.y as usize), palette::WALKER);
        let c = frame.to_map(car.center);
        assert_eq!(canvas.get(c.x as usize, c.y as usize), palette::TRAIN);
        assert_eq!(
            canvas.get(c.x as usize + 5, c.y as usize),
            palette::TRAIN,
            "spans its length"
        );
        let a = frame.to_map(away.center);
        assert_eq!(
            opaque_near(&canvas, a, 2),
            0,
            "cars off the rails are not drawn"
        );
        // Off-map walkers leave the corner alone.
        assert_eq!(canvas.get(199, 0)[3], 0);
    }

    #[test]
    fn the_compass_n_points_to_world_north_and_every_letter_is_drawn() {
        let frame = MapFrame {
            center: GroundVec2::new(-3050.0, -1950.0),
            half_extent: 160.0,
            size_px: 320,
        };
        let marks = compass_marks(&frame);
        let letters: Vec<char> = marks.iter().map(|m| m.0).collect();
        assert_eq!(letters, ['N', 'E', 'S', 'O']);
        assert!(letters.iter().all(|&c| font::supports(c)));
        // N points where ground +y lands on the map: up (map y grows south).
        let north =
            frame.to_map(frame.center + GroundVec2::new(0.0, 10.0)) - frame.to_map(frame.center);
        assert!(close(marks[0].1, north.normalize()), "{:?}", marks[0]);
        assert!(close(marks[0].1, Vec2::new(0.0, -1.0)));
        assert!(close(marks[1].1, Vec2::new(1.0, 0.0)), "east is right");
        assert!(close(marks[2].1, Vec2::new(0.0, 1.0)));
        assert!(close(marks[3].1, Vec2::new(-1.0, 0.0)), "oeste is left");

        // Drawn into the marker layer, every frame, in its corner.
        let mut canvas = Canvas::new(320);
        draw_markers(&mut canvas, &frame, &Markers::default());
        let c = compass_center(320);
        assert_eq!(
            canvas.get(c.x as usize, (c.y - COMPASS_RADIUS) as usize),
            palette::INK,
            "the ring"
        );
        // Each letter has ink inside the ring on its own side, and the
        // north needle is red, above the middle.
        for (_, d) in marks {
            let at = c + d * (COMPASS_RADIUS * 0.6);
            let ink = (-3..=3)
                .flat_map(|dy| (-3..=3).map(move |dx| (dx, dy)))
                .filter(|(dx, dy)| {
                    canvas.get((at.x as i32 + dx) as usize, (at.y as i32 + dy) as usize)
                        == palette::INK
                })
                .count();
            assert!(ink >= 3, "{d:?}: {ink}");
        }
        assert_eq!(
            canvas.get(c.x as usize, (c.y - 3.0) as usize),
            palette::PLAYER
        );
        assert_eq!(
            canvas.get(c.x as usize, (c.y + 3.0) as usize),
            palette::PAPER
        );
        // Away from the corner the layer stays clear.
        assert_eq!(canvas.get(160, 160)[3], 0);
    }

    /// A small farm round `hub`: a house to the north, a galpón to the
    /// east, a tank to the west, a corral to the south, a eucalyptus to the
    /// north-west, and a huella coming in from the west.
    pub(crate) fn farm(hub: GroundVec2) -> Settlement {
        use pampa_core::{HouseStyle, SettlementKind, Side, Structure};
        let at = |dx: f32, dy: f32, size: (f32, f32), kind| Structure {
            kind,
            footprint: Aabb::from_center_size(
                hub + GroundVec2::new(dx, dy),
                GroundVec2::new(size.0, size.1),
            ),
            height: 3.0,
            front: Side::South,
            variant: 0,
        };
        let reach = GroundVec2::new(130.0, 130.0);
        Settlement {
            kind: SettlementKind::Farm,
            gate: 0,
            hub,
            patio: Aabb::from_center_size(hub, GroundVec2::new(20.0, 20.0)),
            structures: vec![
                at(
                    0.0,
                    30.0,
                    (12.0, 8.0),
                    StructureKind::MainHouse(HouseStyle::AdobeIron),
                ),
                at(40.0, 0.0, (14.0, 10.0), StructureKind::Barn),
                at(-30.0, 0.0, (6.0, 6.0), StructureKind::Tank),
                at(0.0, -40.0, (24.0, 20.0), StructureKind::Corral),
                at(-40.0, 40.0, (1.0, 1.0), StructureKind::Eucalyptus),
            ],
            tracks: vec![Track {
                points: vec![
                    hub + GroundVec2::new(-120.0, -10.0),
                    hub + GroundVec2::new(-60.0, -10.0),
                    hub,
                ],
            }],
            bounds: Aabb::new(hub - reach, hub + reach),
            seed: 1,
        }
    }

    #[test]
    fn the_field_paints_settlements_by_kind_with_huellas_and_corral_outlines() {
        let hub = GroundVec2::new(-4600.0, -2900.0);
        let frame = MapFrame::field(hub + GroundVec2::new(20.0, 10.0), 320);
        assert_eq!(frame.meters_per_px(), 2.0);
        let settlements = [farm(hub)];
        let wrecks = [RoadsideWreck {
            at: hub + GroundVec2::new(150.0, 100.0),
            along: GroundVec2::new(1.0, 0.0),
            variant: 0,
        }];
        let overlay = Overlay {
            settlements: &settlements,
            wrecks: &wrecks,
            ..Overlay::default()
        };
        let empty = rasterize_field(&[], &Overlay::default(), &frame);
        let map = rasterize_field(&[], &overlay, &frame);
        let at = |p: GroundVec2| {
            let m = frame.to_map(p);
            map.get(m.x as usize, m.y as usize)
        };
        let s = &settlements[0];
        let of = |kind: StructureKind| s.structures.iter().find(|x| x.kind == kind).unwrap();
        let house = of(StructureKind::MainHouse(pampa_core::HouseStyle::AdobeIron));
        assert_eq!(at(house.footprint.center()), palette::HOUSE);
        assert_eq!(
            at(of(StructureKind::Barn).footprint.center()),
            palette::SHED
        );
        assert_eq!(
            at(of(StructureKind::Tank).footprint.center()),
            palette::WATER
        );
        assert_eq!(
            at(of(StructureKind::Eucalyptus).footprint.center()),
            palette::TREE
        );
        assert_eq!(at(wrecks[0].at), palette::SHED, "roadside wreck");
        // The huella, a pale dirt line, thin: a pixel to either side of it
        // is paper.
        let on_track = hub + GroundVec2::new(-90.0, -10.0);
        assert_eq!(at(on_track), palette::TRACK);
        assert_ne!(at(on_track + GroundVec2::new(0.0, 4.0)), palette::TRACK);
        // The corral: its rails outlined, its pen left open ground.
        let corral = of(StructureKind::Corral).footprint;
        let (a, b) = frame.rect(&corral);
        let (x0, y0) = ((a.x - 0.5).ceil() as usize, (a.y - 0.5).ceil() as usize);
        assert_eq!(map.get(x0 + 4, y0), palette::CORRAL);
        assert_eq!(map.get(x0, y0 + 4), palette::CORRAL);
        let middle = ((a + b) / 2.0).as_uvec2();
        assert_ne!(
            map.get(middle.x as usize, middle.y as usize),
            palette::CORRAL
        );
        // A settlement off the frame leaves the map as it was.
        let far = [farm(hub + GroundVec2::new(5000.0, 0.0))];
        let away = Overlay {
            settlements: &far,
            ..Overlay::default()
        };
        assert_eq!(rasterize_field(&[], &away, &frame), empty);
    }

    #[test]
    fn a_town_map_shows_a_settlement_inside_its_frame_and_is_unchanged_without() {
        let t = town();
        let frame = MapFrame::for_town(&t, 320);
        let plain = rasterize_town_with_roads(&t, &[], &[], &frame);
        assert_eq!(
            rasterize_town_over(&t, &Overlay::default(), &frame),
            plain,
            "the town frame is as before"
        );
        // A farm whose house stands in the frame's south-east corner, off
        // the town's extent.
        let h = frame.half_extent - 12.0;
        let house = frame.center + GroundVec2::new(h, -h);
        assert!(!t.extent().contains_point(house));
        let settlements = [farm(house - GroundVec2::new(0.0, 30.0))];
        let overlay = Overlay {
            settlements: &settlements,
            ..Overlay::default()
        };
        let with = rasterize_town_over(&t, &overlay, &frame);
        let m = frame.to_map(house);
        assert_eq!(with.get(m.x as usize, m.y as usize), palette::HOUSE);
        assert_ne!(plain.get(m.x as usize, m.y as usize), palette::HOUSE);
    }

    #[test]
    fn the_field_frame_shows_a_town_reaching_into_it() {
        let t = town();
        let edge = t.extent().max;
        let frame = MapFrame::field(edge + GroundVec2::new(200.0, 0.0), 320);
        let map = rasterize_field(std::slice::from_ref(&t), &Overlay::default(), &frame);
        let house = t
            .buildings
            .iter()
            .filter(|b| b.kind == BuildingKind::House)
            .find(|b| frame.shown().contains(&b.footprint))
            .expect("a house in the frame");
        let m = frame.to_map(house.footprint.center());
        assert_eq!(map.get(m.x as usize, m.y as usize), palette::HOUSE);
    }

    #[test]
    fn the_field_draws_the_railway_as_one_even_unbroken_line() {
        let frame = MapFrame::field(GroundVec2::ZERO, 320);
        for degrees in [0.0_f32, 17.0, 30.0, 45.0, 52.0, 63.0, 90.0, 133.0] {
            let dir = GroundVec2::new(degrees.to_radians().cos(), degrees.to_radians().sin());
            let c = GroundVec2::new(0.37, 0.61);
            let link = Railway {
                start: c - dir * 250.0,
                end: c + dir * 250.0,
                gauge: 1.676,
            };
            let overlay = Overlay {
                links: std::slice::from_ref(&link),
                ..Overlay::default()
            };
            let map = rasterize_field(&[], &overlay, &frame);
            let (s, e) = (frame.to_map(link.start), frame.to_map(link.end));
            let rail = |x: i32, y: i32| {
                x >= 0
                    && y >= 0
                    && (x as usize) < map.size
                    && (y as usize) < map.size
                    && map.get(x as usize, y as usize) == palette::RAIL
            };
            // No gap: every point along the line is on a rail pixel.
            let steps = (s.distance(e) * 4.0) as usize;
            for k in steps / 20..steps * 19 / 20 {
                let p = s.lerp(e, k as f32 / steps as f32);
                assert!(
                    rail(p.x.floor() as i32, p.y.floor() as i32),
                    "{degrees}°: a gap at {p:?}"
                );
            }
            // Even: across its main direction the line is as wide in every
            // row or column, no tick thickening it into dashes.
            let east_west = (e.x - s.x).abs() >= (e.y - s.y).abs();
            let mut widths = std::collections::BTreeSet::new();
            for k in 1..20 {
                let p = s.lerp(e, 0.05 + 0.9 * k as f32 / 20.0);
                let (x, y) = (p.x.floor() as i32, p.y.floor() as i32);
                let width = (-6..=6)
                    .filter(|d| {
                        if east_west {
                            rail(x, y + d)
                        } else {
                            rail(x + d, y)
                        }
                    })
                    .count();
                widths.insert(width);
            }
            let (min, max) = (*widths.first().unwrap(), *widths.last().unwrap());
            assert!(min >= 2 && max <= 3, "{degrees}°: cross widths {widths:?}");
        }
    }

    fn count(canvas: &Canvas, color: [u8; 4]) -> usize {
        canvas.pixels.chunks(4).filter(|p| *p == color).count()
    }

    #[test]
    fn marks_the_errand_target_and_pins_it_to_the_edge_when_off_the_map() {
        let frame = MapFrame {
            center: GroundVec2::ZERO,
            half_extent: 100.0,
            size_px: 200,
        };
        let mut canvas = Canvas::new(200);
        // No target, no marker.
        draw_markers(&mut canvas, &frame, &Markers::default());
        assert_eq!(count(&canvas, palette::TARGET), 0);

        // On the map: right where it is.
        let near = GroundVec2::new(30.0, -40.0);
        let markers = Markers {
            target: Some(near),
            ..Markers::default()
        };
        draw_markers(&mut canvas, &frame, &markers);
        let at = frame.to_map(near);
        assert_eq!(canvas.get(at.x as usize, at.y as usize), palette::TARGET);
        assert!(count(&canvas, palette::TARGET) > 8);
        assert!(count(&canvas, palette::INK) > 8, "outlined in ink");

        // Far off to the north-west (another town): pinned to the edge on
        // that side, like the player's arrow.
        let far = GroundVec2::new(-2500.0, 900.0);
        let markers = Markers {
            target: Some(far),
            ..Markers::default()
        };
        draw_markers(&mut canvas, &frame, &markers);
        let (pin, on_map) = frame.to_map_clamped(far);
        assert!(!on_map);
        assert!(close(pin, Vec2::new(EDGE, EDGE)), "{pin:?}");
        assert_eq!(canvas.get(pin.x as usize, pin.y as usize), palette::TARGET);
        assert_eq!(canvas.get(100, 100)[3], 0, "nothing left in the middle");

        // The player's arrow is drawn over the target.
        let markers = Markers {
            target: Some(near),
            player: Some((near, Heading::from_radians(0.0))),
            ..Markers::default()
        };
        draw_markers(&mut canvas, &frame, &markers);
        assert_eq!(canvas.get(at.x as usize, at.y as usize), palette::PLAYER);
    }

    #[test]
    fn a_town_without_a_comisaria_is_mapped_without_one() {
        use pampa_core::TownConfig;
        let with = town();
        let without = TownLayout::generate(&TownConfig {
            police_stations: Some(0),
            density: None,
            ..TownConfig::castex()
        })
        .unwrap();
        let frame = MapFrame::for_town(&with, 320);
        let police = with
            .buildings
            .iter()
            .find(|b| b.kind == BuildingKind::PoliceStation)
            .unwrap()
            .footprint;
        // Landmark-colored pixels over the police station's footprint: the
        // building before, ordinary lots (or open ground) now.
        let (lo, hi) = frame.rect(&police);
        let landmark = |t: &TownLayout| {
            let map = rasterize_town(t, &[], &frame);
            let mut n = 0;
            for y in lo.y.ceil() as usize..hi.y.floor() as usize {
                for x in lo.x.ceil() as usize..hi.x.floor() as usize {
                    n += usize::from(map.get(x, y) == palette::LANDMARK);
                }
            }
            n
        };
        assert!(landmark(&with) > 20, "{}", landmark(&with));
        assert_eq!(landmark(&without), 0);
    }

    #[test]
    fn the_field_map_paints_a_lagoon_as_water_and_only_inside_its_shore() {
        let config = pampa_core::LagoonConfig {
            name: "Laguna".into(),
            center: GroundVec2::new(300.0, -200.0),
            size: GroundVec2::new(215.0, 145.0),
            seed: 5,
        };
        let lagoon = Lagoon::new(&config, &pampa_core::Terrain::default());
        let frame = MapFrame::field(lagoon.center + GroundVec2::new(60.0, 0.0), 256);
        let lagoons = [lagoon.clone()];
        let overlay = Overlay {
            lagoons: &lagoons,
            ..Overlay::default()
        };
        let map = rasterize_field(&[], &overlay, &frame);
        let pixel = |p: GroundVec2| {
            let m = frame.to_map(p);
            map.get(m.x as usize, m.y as usize)
        };
        assert_eq!(pixel(lagoon.center), palette::WATER);
        for p in lagoon.ring(32, -4.0) {
            assert_eq!(pixel(p), palette::WATER, "{p:?}");
        }
        for p in lagoon.ring(32, 6.0) {
            assert_ne!(pixel(p), palette::WATER, "{p:?}");
        }
        // Off the frame it leaves the map as it was.
        let mut far = lagoon;
        far.center += GroundVec2::new(3000.0, 0.0);
        let away = [far];
        let empty = rasterize_field(&[], &Overlay::default(), &frame);
        let overlay = Overlay {
            lagoons: &away,
            ..Overlay::default()
        };
        assert_eq!(rasterize_field(&[], &overlay, &frame), empty);
    }

    #[test]
    fn a_bay_platform_is_painted_like_a_towns_where_it_crosses_the_frame() {
        let frame = MapFrame::field(GroundVec2::new(0.0, 0.0), 256);
        let platform =
            Aabb::from_center_size(GroundVec2::new(10.0, 10.0), GroundVec2::new(30.0, 8.0));
        let platforms = [platform];
        let overlay = Overlay {
            platforms: &platforms,
            ..Overlay::default()
        };
        let map = rasterize_field(&[], &overlay, &frame);
        let m = frame.to_map(platform.center());
        assert_eq!(map.get(m.x as usize, m.y as usize), [184, 174, 152, 255]);
        // Off the frame it leaves the map as it was.
        let far = [Aabb::from_center_size(
            GroundVec2::new(9000.0, 0.0),
            GroundVec2::new(30.0, 8.0),
        )];
        let away = Overlay {
            platforms: &far,
            ..Overlay::default()
        };
        assert_eq!(
            rasterize_field(&[], &away, &frame),
            rasterize_field(&[], &Overlay::default(), &frame)
        );
    }
}
