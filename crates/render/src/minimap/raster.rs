//! El dibujo puro del minimapa: la correspondencia de mundo a mapa, la capa
//! estática del pueblo y los marcadores en vivo, todo como buffers de píxeles
//! RGBA8 simples (sin assets de Bevy).
//!
//! El mapa tiene el norte arriba y encuadra el pueblo más un margen en lugar de
//! toda la parcela de 600 m, para que las calles sigan siendo legibles a un
//! tamaño pequeño; lo que queda afuera (el jugador en plena pampa, el tren
//! lejos sobre la línea) se fija en el borde o se deja afuera.

use bevy::math::Vec2;
use pampa_core::{
    Aabb, BuildingKind, Heading, Lagoon, PlaceKind, PlacedCar, Railway, RoadsideWreck, Settlement,
    StructureKind, TownLayout, Track, Vec2 as GroundVec2,
};

use crate::lettering::{self, Face};
use crate::terrain::hash2;

/// Metros de campo abierto que se muestran alrededor de la extensión del
/// pueblo.
pub const MARGIN: f32 = 30.0;
/// La mitad del lado del encuadre de campo, en metros: 640 m de lado, 2 m por
/// píxel en el mapa de 320 píxeles, más o menos la escala del encuadre de un
/// pueblo grande, para que el casco de una estancia se entienda de un vistazo.
pub const FIELD_HALF_EXTENT: f32 = 320.0;
/// Margen, en píxeles, que los marcadores fijados mantienen respecto del borde
/// del mapa.
pub const EDGE: f32 = 6.0;

/// Una ventana cuadrada, con el norte arriba, sobre el plano del suelo, de
/// `size_px` píxeles de ancho.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapFrame {
    pub center: GroundVec2,
    /// La mitad del lado del cuadrado encuadrado, en metros.
    pub half_extent: f32,
    pub size_px: usize,
}

impl MapFrame {
    /// La extensión del pueblo más [`MARGIN`], cuadrada alrededor de su centro.
    pub fn for_town(town: &TownLayout, size_px: usize) -> Self {
        let extent = town.extent();
        let size = extent.size();
        Self {
            center: extent.center(),
            half_extent: size.x.max(size.y) / 2.0 + MARGIN,
            size_px,
        }
    }

    /// El encuadre de campo: [`FIELD_HALF_EXTENT`] alrededor de `center` (el
    /// jinete).
    pub fn field(center: GroundVec2, size_px: usize) -> Self {
        Self {
            center,
            half_extent: FIELD_HALF_EXTENT,
            size_px,
        }
    }

    /// El cuadrado del suelo que muestra.
    pub fn shown(&self) -> Aabb {
        let side = 2.0 * self.half_extent;
        Aabb::from_center_size(self.center, GroundVec2::new(side, side))
    }

    pub fn meters_per_px(&self) -> f32 {
        2.0 * self.half_extent / self.size_px as f32
    }

    /// La posición en píxeles del mapa de un punto del suelo: `(0, 0)` es la
    /// esquina noroeste, `x` crece hacia el este e `y` crece hacia el sur.
    pub fn to_map(&self, p: GroundVec2) -> Vec2 {
        let scale = self.size_px as f32 / (2.0 * self.half_extent);
        Vec2::new(
            (p.x - (self.center.x - self.half_extent)) * scale,
            (self.center.y + self.half_extent - p.y) * scale,
        )
    }

    /// Si un punto del suelo cae dentro del mapa.
    pub fn contains(&self, p: GroundVec2) -> bool {
        (p.x - self.center.x).abs() <= self.half_extent
            && (p.y - self.center.y).abs() <= self.half_extent
    }

    /// [`Self::to_map`], llevado hasta [`EDGE`] píxeles del borde cuando cae
    /// afuera. También indica si el punto estaba en el mapa.
    pub fn to_map_clamped(&self, p: GroundVec2) -> (Vec2, bool) {
        let m = self.to_map(p);
        if self.contains(p) {
            return (m, true);
        }
        let far = self.size_px as f32 - EDGE;
        (Vec2::new(m.x.clamp(EDGE, far), m.y.clamp(EDGE, far)), false)
    }

    /// El rectángulo en píxeles del mapa `(min, max)` que cubre una caja del
    /// suelo.
    pub fn rect(&self, area: &pampa_core::Aabb) -> (Vec2, Vec2) {
        let a = self.to_map(GroundVec2::new(area.min.x, area.max.y));
        let b = self.to_map(GroundVec2::new(area.max.x, area.min.y));
        (a, b)
    }
}

/// Un buffer de píxeles RGBA8 sRGB, fila por fila desde arriba.
#[derive(Debug, Clone, PartialEq)]
pub struct Canvas {
    pub size: usize,
    pub pixels: Vec<u8>,
}

impl Canvas {
    /// Un lienzo cuadrado transparente.
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

    /// Fija un píxel; las coordenadas fuera de rango se ignoran.
    pub fn set(&mut self, x: i32, y: i32, color: [u8; 4]) {
        if x < 0 || y < 0 || x as usize >= self.size || y as usize >= self.size {
            return;
        }
        let i = (y as usize * self.size + x as usize) * 4;
        self.pixels[i..i + 4].copy_from_slice(&color);
    }

    /// Mezcla `color` sobre el píxel en `(x, y)` según `alpha` (0..=255); fuera
    /// del lienzo no hace nada.
    pub fn blend(&mut self, x: i32, y: i32, color: [u8; 4], alpha: u8) {
        if x < 0 || y < 0 || x as usize >= self.size || y as usize >= self.size {
            return;
        }
        let i = (y as usize * self.size + x as usize) * 4;
        let a = u32::from(alpha);
        for (dst, &src) in self.pixels[i..i + 3].iter_mut().zip(&color) {
            *dst = ((u32::from(*dst) * (255 - a) + u32::from(src) * a + 127) / 255) as u8;
        }
        let dst_a = u32::from(self.pixels[i + 3]);
        self.pixels[i + 3] =
            (dst_a + (u32::from(color[3]) - dst_a.min(u32::from(color[3]))) * a / 255) as u8;
    }

    /// Rellena los píxeles cuyos centros caen dentro de `[min, max)` (píxeles
    /// del mapa).
    pub fn fill_rect(&mut self, min: Vec2, max: Vec2, color: [u8; 4]) {
        let (x0, y0) = ((min.x - 0.5).ceil() as i32, (min.y - 0.5).ceil() as i32);
        let (x1, y1) = ((max.x - 0.5).ceil() as i32, (max.y - 0.5).ceil() as i32);
        for y in y0..y1.max(y0 + 1) {
            for x in x0..x1.max(x0 + 1) {
                self.set(x, y, color);
            }
        }
    }

    /// Vuelve transparentes todos los píxeles.
    pub fn clear(&mut self) {
        self.pixels.fill(0);
    }
}

/// Paleta de mapa de papel.
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
    /// Caminos de tierra: un marrón cálido, más oscuro que las calles y más
    /// claro que el riel.
    pub const ROAD: [u8; 4] = [168, 124, 76, 255];
    pub const INK: [u8; 4] = [40, 30, 24, 255];
    pub const PLAYER: [u8; 4] = [196, 40, 28, 255];
    pub const WALKER: [u8; 4] = [48, 40, 96, 255];
    pub const TRAIN: [u8; 4] = [28, 26, 26, 255];
    pub const TARGET: [u8; 4] = [232, 176, 32, 255];
    /// Árboles: un punto verde oscuro cada uno.
    pub const TREE: [u8; 4] = [96, 110, 60, 255];
    /// Las huellas de un asentamiento: una línea de tierra pálida, más clara
    /// que los caminos.
    pub const TRACK: [u8; 4] = [186, 152, 104, 255];
    /// Travesaños de corral: un contorno de madera oscura.
    pub const CORRAL: [u8; 4] = [104, 78, 50, 255];
    /// Parvas.
    pub const HAY: [u8; 4] = [194, 172, 92, 255];
}

/// Lo que muestra una capa más allá de su pueblo: los tramos de la línea del
/// ferrocarril, las piezas rectas de los caminos, y los asentamientos y los
/// restos abandonados al costado del camino, cada uno pintado donde cruza el
/// encuadre.
#[derive(Debug, Clone, Copy, Default)]
pub struct Overlay<'a> {
    pub links: &'a [Railway],
    pub roads: &'a [(GroundVec2, GroundVec2)],
    pub settlements: &'a [Settlement],
    pub wrecks: &'a [RoadsideWreck],
    /// Lagunas, pintadas como agua por debajo de todo lo demás.
    pub lagoons: &'a [Lagoon],
    /// Andenes que no pertenecen a los edificios de ningún pueblo (los de las
    /// vías muertas), pintados como los andenes de los pueblos.
    pub platforms: &'a [Aabb],
}

/// El color de un andén.
const PLATFORM: [u8; 4] = [184, 174, 152, 255];

/// El tamaño en píxeles de las iniciales de los lugares y del título en el mapa
/// (una mayúscula mide unos 14 px de alto, como lo hacía el viejo bitmap de 7
/// filas a 2x).
pub const LABEL_PX: f32 = 21.0;
/// El tamaño en píxeles de las letras pequeñas: las letras de la rosa de los
/// vientos y un título demasiado largo para [`LABEL_PX`].
pub const SMALL_LABEL_PX: f32 = 11.0;

/// Pinta el pueblo una vez: el papel, las calles, la plaza, el ferrocarril, los
/// tramos `links` de la línea del ferrocarril donde cruzan el encuadre, los
/// edificios coloreados por tipo y la inicial de cada lugar con nombre encima.
pub fn rasterize_town(town: &TownLayout, links: &[Railway], frame: &MapFrame) -> Canvas {
    rasterize_town_with_roads(town, links, &[], frame)
}

/// [`rasterize_town`], con los caminos de tierra `roads` (piezas rectas de sus
/// ejes) pintados bajo el ferrocarril donde cruzan el encuadre.
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

/// [`rasterize_town_with_roads`], con todo lo de `overlay` que cruza el
/// encuadre: los asentamientos que quedan justo fuera del pueblo se ven en su
/// mapa.
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
    // La marca del norte en la esquina noroeste, y el nombre del pueblo a lo
    // largo del borde superior.
    label(&mut map, "N", Vec2::new(16.0, 18.0));
    title(&mut map, &town.name);
    map
}

/// La capa de campo: campo abierto alrededor del jinete, con los caminos, el
/// ferrocarril, los asentamientos y cualquiera de los `towns` que entre en el
/// encuadre (calles y edificios, sin etiquetas ni título).
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
    // Aquí no hay marcas de durmientes: a 2 m por píxel, reducidas sobre el
    // panel, parecen una línea discontinua.
    paint_rails(&mut map, &[], overlay.links, frame, false);
    paint_settlements(&mut map, overlay, frame);
    for town in &near {
        paint_town_buildings(&mut map, town, frame);
    }
    paint_platforms(&mut map, overlay, frame);
    label(&mut map, "N", Vec2::new(16.0, 18.0));
    map
}

/// Papel envejecido, más oscuro hacia el borde.
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

/// Las calles, las plazas y los árboles de un pueblo.
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

/// Los edificios de un pueblo, coloreados por tipo.
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

/// Los andenes propios de la superposición (los de las vías muertas), donde
/// cruzan el encuadre.
fn paint_platforms(map: &mut Canvas, overlay: &Overlay, frame: &MapFrame) {
    for platform in overlay.platforms {
        let (a, b) = frame.rect(platform);
        map.fill_rect(a, b, PLATFORM);
    }
}

/// Los caminos, y después las huellas de los asentamientos, recortados al
/// encuadre: por debajo del ferrocarril y de todos los edificios.
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
    // Delgada: un píxel de ancho en el encuadre de campo, como mucho dos en el
    // de un pueblo.
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

/// El agua de una laguna, píxel por píxel sobre su caja donde cruza el
/// encuadre: un píxel es agua cuando su centro lo es.
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

/// Una línea recta del suelo desde `a` hasta `b`, recortada al encuadre, de
/// `half` píxeles del mapa a cada lado.
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

/// El ferrocarril: la vía de cada pueblo, y después los tramos de la línea del
/// ferrocarril, recortados al encuadre (recorren kilómetros); con marcas de
/// durmientes cuando `ties`.
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

/// El color en el mapa de una estructura de asentamiento rellena sobre su
/// huella de edificio; `None` para los corrales (dibujados como contorno) y los
/// árboles (un punto).
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

/// Los asentamientos que entran en el encuadre: huellas de edificio por tipo,
/// corrales como contornos de travesaños, árboles como puntos; y después los
/// restos abandonados al costado del camino.
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

/// Un contorno de un píxel del rectángulo del mapa `[min, max)`.
fn outline(map: &mut Canvas, min: Vec2, max: Vec2, color: [u8; 4]) {
    map.fill_rect(min, Vec2::new(max.x, min.y + 1.0), color);
    map.fill_rect(Vec2::new(min.x, max.y - 1.0), max, color);
    map.fill_rect(min, Vec2::new(min.x + 1.0, max.y), color);
    map.fill_rect(Vec2::new(max.x - 1.0, min.y), max, color);
}

/// Una vía desde el píxel del mapa `s` hasta `e`: una línea oscura continua de
/// dos píxeles de ancho, con marcas de durmientes que la cruzan cada tres
/// píxeles cuando `ties`, ambas orientadas según la dirección principal de la
/// vía.
fn track_line(map: &mut Canvas, s: Vec2, e: Vec2, ties: bool) {
    // Semitamaños de un punto de la línea y de una marca, orientados según la
    // vía.
    let east_west = (e.x - s.x).abs() >= (e.y - s.y).abs();
    let (line, tick) = if east_west {
        (Vec2::new(0.5, 1.0), Vec2::new(0.5, 2.0))
    } else {
        (Vec2::new(1.0, 0.5), Vec2::new(2.0, 0.5))
    };
    // Pasos de medio píxel: nunca se saltea ninguna columna ni fila.
    let steps = (s.distance(e) * 2.0).ceil() as usize;
    for k in 0..=steps {
        let p = s.lerp(e, k as f32 / steps.max(1) as f32);
        map.fill_rect(p - line, p + line, palette::RAIL);
        if ties && k % 6 == 0 {
            map.fill_rect(p - tick, p + tick, palette::RAIL);
        }
    }
}

/// Escribe el nombre del pueblo, en mayúsculas, centrado a lo largo del borde
/// superior; a [`SMALL_LABEL_PX`] cuando es demasiado largo para el mapa.
fn title(map: &mut Canvas, name: &str) {
    let text = name.to_uppercase();
    let size = map.size as f32;
    let fits = lettering::measure(&text, Face::Body, LABEL_PX).width as f32 <= size - 48.0;
    let px = if fits { LABEL_PX } else { SMALL_LABEL_PX };
    label_scaled(map, &text, Vec2::new(size / 2.0, 18.0), px);
}

/// Escribe `text` con tinta a [`LABEL_PX`], centrado en `center`, sobre un halo
/// de papel de un píxel para que se lea encima de los edificios.
fn label(map: &mut Canvas, text: &str, center: Vec2) {
    label_scaled(map, text, center, LABEL_PX);
}

/// La cobertura a partir de la cual un píxel recibe un halo de papel alrededor.
const HALO_COVERAGE: u8 = 96;

/// [`label`] a `px` píxeles: los límites de la tinta de las letras se centran
/// en `center`, un halo de papel va debajo de cada píxel bien cubierto, y la
/// tinta se mezcla encima según la cobertura.
fn label_scaled(map: &mut Canvas, text: &str, center: Vec2, px: f32) {
    let extent = lettering::measure(text, Face::Body, px);
    let Some(ink) = extent.ink else { return };
    let coverage = lettering::rasterize(text, Face::Body, px);
    // El origen de la cobertura es el origen de la pluma; los límites de la
    // tinta son relativos a él.
    let left = (center.x - (ink.min_x + ink.max_x) / 2.0).round() as i32;
    let top = (center.y - (ink.min_y + ink.max_y) / 2.0).round() as i32;
    coverage.for_each(|x, y, a| {
        if a < HALO_COVERAGE {
            return;
        }
        let (x, y) = (left + x, top + y);
        for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            map.set(x + dx, y + dy, palette::PAPER);
        }
    });
    coverage.for_each(|x, y, a| {
        let (x, y) = (left + x, top + y);
        map.blend(x, y, palette::INK, a);
    });
}

/// Radio del anillo de la rosa de los vientos, en píxeles del mapa.
pub const COMPASS_RADIUS: f32 = 17.0;

/// Dónde está la rosa de los vientos: la esquina inferior izquierda del mapa,
/// lejos del título a lo largo del borde superior.
pub fn compass_center(size_px: usize) -> Vec2 {
    let inset = COMPASS_RADIUS + 8.0;
    Vec2::new(inset, size_px as f32 - inset)
}

/// Las cuatro letras de la rosa de los vientos, N, E, S y O (oeste), cada una
/// con la dirección unitaria hacia la que apunta, en píxeles del mapa, tomada
/// del propio `frame`: la forma propia del mapa de mostrar el norte del mundo
/// (`+y` del suelo), el este, el sur y el oeste, así que un mapa que girara
/// giraría su rosa con él.
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

/// Dibuja la rosa de los vientos en [`compass_center`]: un disco de papel en un
/// anillo de tinta, una aguja roja hacia el norte y las letras de
/// [`compass_marks`] dentro del anillo.
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
    // Una aguja roja corta desde el centro hacia el norte.
    let north = marks[0].1;
    for k in 1..=5 {
        let p = c + north * k as f32;
        canvas.set(p.x.floor() as i32, p.y.floor() as i32, palette::PLAYER);
    }
    for (letter, d) in marks {
        label_scaled(
            canvas,
            &letter.to_string(),
            c + d * (r * 0.6),
            SMALL_LABEL_PX,
        );
    }
}

/// La letra en el mapa de un lugar: la inicial del nombre de su cartel.
pub fn place_initial(kind: PlaceKind) -> char {
    kind.name().chars().next().unwrap_or(' ')
}

/// Las cosas en vivo que se marcan en el mapa en este frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Markers {
    pub player: Option<(GroundVec2, Heading)>,
    pub walkers: Vec<GroundVec2>,
    pub cars: Vec<PlacedCar>,
    /// Adónde envía el mandado al jugador, si a algún lado.
    pub target: Option<GroundVec2>,
}

/// Limpia `canvas` y dibuja los marcadores: los vagones del tren como barras
/// oscuras, los peatones como puntos (omitidos fuera del mapa), el destino del
/// mandado como un rombo dorado y el jugador como una flecha según su rumbo. El
/// destino y el jugador se fijan en el borde cuando están afuera, así que
/// señalan el camino. Devuelve la posición del marcador del jugador, si la hay.
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

/// Un cuadrado relleno de `2 * radius + 1` píxeles alrededor del píxel que
/// contiene `p`.
fn square(canvas: &mut Canvas, p: Vec2, radius: i32, color: [u8; 4]) {
    let (cx, cy) = (p.x.floor() as i32, p.y.floor() as i32);
    for y in cy - radius..=cy + radius {
        for x in cx - radius..=cx + radius {
            canvas.set(x, y, color);
        }
    }
}

/// El destino del mandado: un rombo dorado con contorno de tinta, centrado en
/// `at`.
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

/// La flecha del jugador: un triángulo rojo con contorno de tinta, con la punta
/// según `heading`.
fn arrow(canvas: &mut Canvas, at: Vec2, heading: Heading) {
    let f = heading.forward();
    // La y del mapa crece hacia el sur.
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
        // La esquina noroeste arriba a la izquierda, la sudeste abajo a la
        // derecha.
        assert!(close(
            frame.to_map(GroundVec2::new(-90.0, 120.0)),
            Vec2::ZERO
        ));
        assert!(close(
            frame.to_map(GroundVec2::new(110.0, -80.0)),
            Vec2::new(200.0, 200.0)
        ));
        // El este está a la derecha, el norte arriba.
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
        // Donde se cruzaban las calles interiores, la plaza fusionada es
        // césped, no calle; fuera del sendero central y del monumento.
        let c = t.plaza.center();
        let former_street = GroundVec2::new(c.x + 12.0, c.y + 3.5);
        assert_eq!(at(former_street)[..3], palette::PLAZA[..3]);
    }

    #[test]
    fn the_railway_line_is_painted_where_it_crosses_the_frame() {
        let t = town();
        let frame = MapFrame::for_town(&t, 320);
        let tip = t.railway.unwrap().end;
        // Un tramo que sale de la punta este hacia el oeste, subiendo
        // lentamente hacia el norte sobre el campo abierto más allá de la vía
        // del pueblo, y uno muy lejos.
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
        // Ambos al este del título, a lo largo del borde superior.
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
        // El tramo lejano deja el mapa como estaba.
        assert_eq!(rasterize_town(&t, &[far], &frame), without);
    }

    #[test]
    fn roads_are_painted_in_dirt_cut_to_the_frame_under_the_rail() {
        let t = town();
        let frame = MapFrame::for_town(&t, 320);
        // Un camino que sale en diagonal de la esquina sudoeste de la
        // cuadrícula, fuera del encuadre, y uno muy lejos.
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
        // Un punto sobre una calle pero fuera de la plaza y de los edificios.
        let street = t.streets[0];
        let on_street = GroundVec2::new(street.center().x, t.grid().min.y + 3.0);
        assert_eq!(at(on_street)[..3], palette::STREET[..3]);
        // Plaza: un lugar entre sus árboles y la etiqueta central.
        let plaza = t.plaza;
        assert_eq!(
            at(GroundVec2::new(plaza.min.x + 2.0, plaza.min.y + 6.0))[..3],
            palette::PLAZA[..3]
        );
        // La línea del ferrocarril se dibuja donde cruza el encuadre.
        let rail = GroundVec2::new(frame.center.x + 100.0, t.railway.unwrap().start.y);
        assert_eq!(at(rail)[..3], palette::RAIL[..3]);
        // Una casa se pinta como una casa.
        let house = t
            .buildings
            .iter()
            .find(|b| b.kind == BuildingKind::House)
            .unwrap();
        let corner = GroundVec2::new(house.footprint.min.x + 1.0, house.footprint.min.y + 1.0);
        assert_eq!(at(corner)[..3], palette::HOUSE[..3]);
        // Cada lugar lleva su inicial con tinta, dentro del mapa.
        for place in t.places() {
            let c = frame.to_map(place.center());
            assert!(c.x > 0.0 && c.y > 0.0 && c.x < 320.0 && c.y < 320.0);
            let mut ink = 0;
            for y in (c.y as usize - 12)..(c.y as usize + 12) {
                for x in (c.x as usize - 12)..(c.x as usize + 12) {
                    ink += usize::from(dark(map.get(x, y)));
                }
            }
            assert!(ink >= 8, "{:?} label has {ink} ink pixels", place.kind);
        }
        assert_eq!(place_initial(PlaceKind::GeneralStore), 'A');
        assert_eq!(place_initial(PlaceKind::Plaza), 'P');
    }

    /// Verdadero para un píxel que es mayormente tinta: los trazos de las
    /// letras son delgados, así que pocos llegan a la cobertura completa.
    fn dark(px: [u8; 4]) -> bool {
        px[0] < 110 && px[1] < 100 && px[2] < 100
    }

    /// Los píxeles de tinta, de halo y mezclados de `canvas`, que se rellenó
    /// con `ground`.
    fn tally(canvas: &Canvas, ground: [u8; 4]) -> (usize, usize, usize) {
        let (mut ink, mut halo, mut blended) = (0, 0, 0);
        for y in 0..canvas.size {
            for x in 0..canvas.size {
                let px = canvas.get(x, y);
                if dark(px) {
                    ink += 1;
                } else if px == palette::PAPER {
                    halo += 1;
                } else if px != ground {
                    blended += 1;
                }
            }
        }
        (ink, halo, blended)
    }

    /// Los límites `(min_x, min_y, max_x, max_y)` de los píxeles que no son
    /// `ground`.
    fn bounds(canvas: &Canvas, ground: [u8; 4]) -> (usize, usize, usize, usize) {
        let mut b = (usize::MAX, usize::MAX, 0, 0);
        for y in 0..canvas.size {
            for x in 0..canvas.size {
                if canvas.get(x, y) != ground {
                    b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
                }
            }
        }
        b
    }

    fn filled(size: usize, color: [u8; 4]) -> Canvas {
        let mut c = Canvas::new(size);
        c.fill_rect(Vec2::ZERO, Vec2::splat(size as f32), color);
        c
    }

    #[test]
    fn a_label_is_antialiased_ink_over_a_paper_halo_centered_on_its_spot() {
        let mut canvas = filled(200, palette::HOUSE);
        label(&mut canvas, "ALMACÉN", Vec2::new(100.0, 100.0));
        let (ink, halo, blended) = tally(&canvas, palette::HOUSE);
        assert!(ink >= 30, "ink {ink}");
        assert!(halo >= 30, "paper halo {halo}");
        assert!(blended >= 10, "antialiased edges {blended}");
        let (x0, y0, x1, y1) = bounds(&canvas, palette::HOUSE);
        let (cx, cy) = ((x0 + x1) as f32 / 2.0, (y0 + y1) as f32 / 2.0);
        assert!((cx - 100.0).abs() <= 3.0, "centered x {cx}");
        assert!((cy - 100.0).abs() <= 3.0, "centered y {cy}");
        // El halo se extiende a cada lado de las letras, más allá de la tinta
        // bien cubierta. Solo el borde tenue con antialiasing de una serifa,
        // que no recibe halo, puede quedar hasta dos píxeles más allá.
        let row_has_paper = |y: usize| (x0..=x1).any(|x| canvas.get(x, y) == palette::PAPER);
        let col_has_paper = |x: usize| (y0..=y1).any(|y| canvas.get(x, y) == palette::PAPER);
        let near = |edge: usize, inward: isize, has: &dyn Fn(usize) -> bool| {
            (0..=2).any(|d| has((edge as isize + inward * d) as usize))
        };
        assert!(
            near(y0, 1, &row_has_paper) && near(y1, -1, &row_has_paper),
            "halo above and below"
        );
        assert!(
            near(x0, 1, &col_has_paper) && near(x1, -1, &col_has_paper),
            "halo left and right"
        );
        // Más o menos la altura del viejo bitmap: 7 filas a 2x, más el halo.
        let h = y1 - y0 + 1;
        assert!((12..=24).contains(&h), "height {h}");
    }

    #[test]
    fn blending_mixes_the_color_over_the_pixel_by_alpha() {
        let mut canvas = filled(4, [200, 100, 0, 255]);
        canvas.blend(1, 1, [0, 0, 100, 255], 0);
        assert_eq!(canvas.get(1, 1), [200, 100, 0, 255], "alpha 0 leaves it");
        canvas.blend(1, 1, [0, 0, 100, 255], 255);
        assert_eq!(canvas.get(1, 1), [0, 0, 100, 255], "alpha 255 replaces it");
        canvas.blend(2, 2, [0, 100, 100, 255], 128);
        assert_eq!(canvas.get(2, 2), [100, 100, 50, 255], "halfway");
        canvas.blend(-1, 0, palette::INK, 255);
        canvas.blend(0, 4, palette::INK, 255);
        let mut clear = Canvas::new(2);
        clear.blend(0, 0, palette::INK, 255);
        assert_eq!(clear.get(0, 0)[3], 255, "ink over nothing is opaque");
    }

    #[test]
    fn a_title_that_does_not_fit_falls_back_to_the_smaller_size() {
        let mut short = filled(320, palette::PAPER);
        title(&mut short, "Pico");
        let mut long = filled(320, palette::PAPER);
        title(&mut long, "Coronel Francisco de Paula Rodriguez Larreta");
        let (a0, a1, a2, a3) = bounds(&short, palette::PAPER);
        let (b0, b1, b2, b3) = bounds(&long, palette::PAPER);
        assert!(a2 - a0 > 20 && b2 - b0 > 20);
        // El nombre largo es más angosto que el mapa menos un margen, y sus
        // letras son más pequeñas que las del nombre corto.
        assert!(b2 - b0 < 320 - 48, "long title width {}", b2 - b0 + 1);
        assert!(b3 - b1 < a3 - a1, "long {} short {}", b3 - b1, a3 - a1);
        // Ambos quedan centrados a lo largo del borde superior.
        assert!((((a0 + a2) as f32 / 2.0) - 160.0).abs() <= 3.0);
        assert!((((b0 + b2) as f32 / 2.0) - 160.0).abs() <= 3.0);
        assert!(a1 < 30 && b1 < 30, "along the top edge");
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
        // Hay más flecha por delante (arriba, para el norte) que por detrás.
        let (above, below) = (
            opaque_near(&canvas, at - Vec2::new(0.0, 5.0), 1),
            opaque_near(&canvas, at + Vec2::new(0.0, 5.0), 1),
        );
        assert!(above > below, "above {above} below {below}");
        // Girada hacia el este, la punta apunta a la derecha.
        let east = Markers {
            player: Some((position, Heading::from_radians(0.0))),
            ..Markers::default()
        };
        draw_markers(&mut canvas, &frame, &east);
        let right = opaque_near(&canvas, at + Vec2::new(5.0, 0.0), 1);
        let left = opaque_near(&canvas, at - Vec2::new(5.0, 0.0), 1);
        assert!(right > left, "right {right} left {left}");
        // Volver a dibujar limpia el frame anterior.
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
        // Los peatones fuera del mapa dejan la esquina sin tocar.
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
        assert!(
            letters
                .iter()
                .all(|&c| crate::lettering::supports(crate::lettering::Face::Body, c))
        );
        // La N apunta adonde cae el +y del suelo en el mapa: arriba (la y del
        // mapa crece hacia el sur).
        let north =
            frame.to_map(frame.center + GroundVec2::new(0.0, 10.0)) - frame.to_map(frame.center);
        assert!(close(marks[0].1, north.normalize()), "{:?}", marks[0]);
        assert!(close(marks[0].1, Vec2::new(0.0, -1.0)));
        assert!(close(marks[1].1, Vec2::new(1.0, 0.0)), "east is right");
        assert!(close(marks[2].1, Vec2::new(0.0, 1.0)));
        assert!(close(marks[3].1, Vec2::new(-1.0, 0.0)), "oeste is left");

        // Se dibuja en la capa de marcadores, en cada frame, en su esquina.
        let mut canvas = Canvas::new(320);
        draw_markers(&mut canvas, &frame, &Markers::default());
        let c = compass_center(320);
        assert_eq!(
            canvas.get(c.x as usize, (c.y - COMPASS_RADIUS) as usize),
            palette::INK,
            "the ring"
        );
        // Cada letra tiene tinta dentro del anillo de su propio lado, y la
        // aguja del norte es roja, por encima del centro.
        for (_, d) in marks {
            let at = c + d * (COMPASS_RADIUS * 0.6);
            let ink = (-3..=3)
                .flat_map(|dy| (-3..=3).map(move |dx| (dx, dy)))
                .filter(|(dx, dy)| {
                    dark(canvas.get((at.x as i32 + dx) as usize, (at.y as i32 + dy) as usize))
                })
                .count();
            assert!(ink >= 3, "{d:?}: {ink}");
            let halo = (-6..=6)
                .flat_map(|dy| (-6..=6).map(move |dx| (dx, dy)))
                .filter(|(dx, dy)| {
                    canvas.get((at.x as i32 + dx) as usize, (at.y as i32 + dy) as usize)
                        == palette::PAPER
                })
                .count();
            assert!(halo >= 3, "{d:?} letter sits on paper: {halo}");
        }
        assert_eq!(
            canvas.get(c.x as usize, (c.y - 3.0) as usize),
            palette::PLAYER
        );
        assert_eq!(
            canvas.get(c.x as usize, (c.y + 3.0) as usize),
            palette::PAPER
        );
        // Lejos de la esquina la capa queda limpia.
        assert_eq!(canvas.get(160, 160)[3], 0);
    }

    /// Una pequeña chacra alrededor de `hub`: una casa al norte, un galpón al
    /// este, un tanque al oeste, un corral al sur, un eucalipto al noroeste y
    /// una huella que llega desde el oeste.
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
        // La huella, una línea de tierra pálida, delgada: un píxel a cada lado
        // de ella es papel.
        let on_track = hub + GroundVec2::new(-90.0, -10.0);
        assert_eq!(at(on_track), palette::TRACK);
        assert_ne!(at(on_track + GroundVec2::new(0.0, 4.0)), palette::TRACK);
        // El corral: sus travesaños en contorno, y su encierro como campo
        // abierto.
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
        // Un asentamiento fuera del encuadre deja el mapa como estaba.
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
        // Una chacra cuya casa está en la esquina sudeste del encuadre, fuera
        // de la extensión del pueblo.
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
            // Sin huecos: cada punto a lo largo de la línea está sobre un píxel
            // de riel.
            let steps = (s.distance(e) * 4.0) as usize;
            for k in steps / 20..steps * 19 / 20 {
                let p = s.lerp(e, k as f32 / steps as f32);
                assert!(
                    rail(p.x.floor() as i32, p.y.floor() as i32),
                    "{degrees}°: a gap at {p:?}"
                );
            }
            // Pareja: a lo ancho de su dirección principal la línea tiene el
            // mismo ancho en cada fila o columna, sin marcas que la engrosen en
            // guiones.
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
        // Sin destino, sin marcador.
        draw_markers(&mut canvas, &frame, &Markers::default());
        assert_eq!(count(&canvas, palette::TARGET), 0);

        // En el mapa: justo donde está.
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

        // Muy lejos hacia el noroeste (otro pueblo): fijado en el borde de ese
        // lado, como la flecha del jugador.
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

        // La flecha del jugador se dibuja encima del destino.
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
        // Píxeles del color de los hitos sobre la huella de edificio de la
        // comisaría: el edificio antes, ahora lotes comunes (o campo abierto).
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
        // Fuera del encuadre deja el mapa como estaba.
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
        // Fuera del encuadre deja el mapa como estaba.
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
