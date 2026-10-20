//! Pure minimap drawing: the world-to-map mapping, the static town layer and
//! the live markers, all as plain RGBA8 pixel buffers (no Bevy assets).
//!
//! The map is north-up and frames the town plus a margin rather than the
//! whole 600 m patch, so streets stay legible at a small size; anything
//! outside (the player out on the pampa, the train far down the line) is
//! clamped to the edge or left off.

use bevy::math::Vec2;
use pampa_core::{BuildingKind, Heading, PlaceKind, PlacedCar, TownLayout, Vec2 as GroundVec2};

use crate::font::{self, GLYPH_HEIGHT};
use crate::terrain::hash2;

/// Meters of open ground shown around the town's extent.
pub const MARGIN: f32 = 30.0;
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
    pub const INK: [u8; 4] = [40, 30, 24, 255];
    pub const PLAYER: [u8; 4] = [196, 40, 28, 255];
    pub const WALKER: [u8; 4] = [48, 40, 96, 255];
    pub const TRAIN: [u8; 4] = [28, 26, 26, 255];
}

/// Pixel scale of place initials on the map.
pub const LABEL_SCALE: usize = 2;

/// Paints the town once: paper, streets, plaza, railway, buildings colored by
/// kind and each named place's initial over it.
pub fn rasterize_town(town: &TownLayout, frame: &MapFrame) -> Canvas {
    let size = frame.size_px;
    let mut map = Canvas::new(size);
    // Aged paper, darker toward the edge.
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
        map.set(m.x as i32, m.y as i32, [96, 110, 60, 255]);
    }

    // Railway: a dark line with sleeper ticks across it.
    if let Some(rail) = town.railway {
        let (s, e) = (frame.to_map(rail.start), frame.to_map(rail.end));
        // Half sizes of a line dot and a tick, oriented to the track.
        let east_west = (e.x - s.x).abs() >= (e.y - s.y).abs();
        let (line, tick) = if east_west {
            (Vec2::new(0.5, 1.0), Vec2::new(0.5, 2.0))
        } else {
            (Vec2::new(1.0, 0.5), Vec2::new(2.0, 0.5))
        };
        let steps = s.distance(e).ceil() as usize;
        for k in 0..=steps {
            let p = s.lerp(e, k as f32 / steps.max(1) as f32);
            map.fill_rect(p - line, p + line, palette::RAIL);
            if k % 3 == 0 {
                map.fill_rect(p - tick, p + tick, palette::RAIL);
            }
        }
    }

    for building in &town.buildings {
        let color = match building.kind {
            BuildingKind::House => palette::HOUSE,
            BuildingKind::GeneralStore
            | BuildingKind::PoliceStation
            | BuildingKind::Church
            | BuildingKind::Station => palette::LANDMARK,
            BuildingKind::Platform => [184, 174, 152, 255],
            BuildingKind::Shed | BuildingKind::GrainStore => palette::SHED,
            BuildingKind::WaterTank => palette::WATER,
            BuildingKind::Windmill => palette::INK,
        };
        let (a, b) = frame.rect(&building.footprint);
        map.fill_rect(a, b, color);
    }

    for place in town.places() {
        let text = place_initial(place.kind).to_string();
        label(&mut map, &text, frame.to_map(place.center()));
    }
    // North marker in the north-west corner, the town's name along the top.
    label(&mut map, "N", Vec2::new(16.0, 18.0));
    title(&mut map, &town.name);
    map
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
}

/// Clears `canvas` and draws the markers: train cars as dark bars, walkers
/// as dots (skipped off the map) and the player as an arrow along its
/// heading, clamped to the edge when outside. Returns the player's marker
/// position, if any.
pub fn draw_markers(canvas: &mut Canvas, frame: &MapFrame, markers: &Markers) -> Option<Vec2> {
    canvas.clear();
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
mod tests {
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
        let map = rasterize_town(&t, &frame);
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
    fn town_layer_shows_streets_plaza_railway_and_place_letters() {
        let t = town();
        let frame = MapFrame::for_town(&t, 320);
        let map = rasterize_town(&t, &frame);
        assert_eq!(map.pixels.len(), 320 * 320 * 4);
        assert_eq!(map, rasterize_town(&t, &frame), "deterministic");
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
}
