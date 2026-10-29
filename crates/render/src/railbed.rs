//! La vía permanente: el lecho de balasto, los durmientes y los rieles de cada
//! vía, construidos como mallas continuas a lo largo del recorrido de la vía.
//!
//! Un [`Track`] es un eje (la vía recta de un pueblo, el recorrido curvo de un
//! enlace, o una vía muerta y su desvío) y [`Parts`] barre tres cosas a lo
//! largo de él: un lecho de balasto trapezoidal cuyos hombros siguen el
//! terreno, un perfil de riel de patín plano (patín, alma, cabeza) para cada
//! riel, y los durmientes, cada [`SLEEPER_SPACING`] metros de toda la vía. Todo
//! tiene UV a escala del mundo, así que las texturas conservan su tamaño sea
//! cual sea el largo de la pieza (los conjuntos se acreditan en
//! [`crate::facade`]).
//!
//! Cada altura es función solo de la posición en el suelo, así que las vías que
//! se encuentran coinciden sin compartir nada:
//!
//! - donde dos vías corren cerca (una vía muerta junto a la vía de la estación,
//!   el desvío, un ramal que se separa de la línea), una vía se mantiene unos
//!   milímetros por debajo de las anteriores en el [`TrackNet`] y sigue sus
//!   alturas mientras está cerca, así que los lechos se leen como una sola
//!   superficie y nada compite;
//! - los durmientes de la vía posterior se detienen donde terminan las maderas
//!   de la anterior, así que ninguno queda atravesado sobre otro, y los rieles
//!   de la vía posterior siguen apoyados en madera: más allá de las maderas
//!   anteriores, sus durmientes se acortan solo de ese lado.
//!
//! La cara superior de los rieles queda a [`RAIL_TOP`] sobre el suelo, donde
//! ruedan las ruedas de los trenes (`LineTrain::RAIL_TOP`).

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use pampa_core::{Aabb, RailLine, Railway, Terrain, TownLayout, Vec2 as GroundVec2};

use crate::coords::world_from_ground;
use crate::facade::Surface;
use crate::ground::face_tangent;

/// Cara superior de los rieles sobre el suelo, en metros.
pub(crate) const RAIL_TOP: f32 = 0.5;
/// Altura del perfil del riel, en metros (un riel de patín plano de 60 kg mide
/// unos 14 cm de alto).
const RAIL_HEIGHT: f32 = 0.14;
/// Altura del patín de un riel sobre el suelo: la cara superior de los
/// durmientes.
const RAIL_FOOT: f32 = RAIL_TOP - RAIL_HEIGHT;
/// Altura de la parte superior plana del balasto sobre el suelo, en metros.
const BALLAST_TOP: f32 = 0.30;
/// La mitad del ancho de la parte superior del balasto, en metros.
const BALLAST_TOP_HALF: f32 = 1.4;
/// La mitad del ancho del balasto a nivel del suelo, en metros.
const BALLAST_FOOT_HALF: f32 = 1.9;
/// Cuánto por debajo del suelo terminan los hombros del balasto, en metros.
const BALLAST_SINK: f32 = 0.05;
/// Distancia entre durmientes a lo largo de la vía, en metros. Los 1.2 m de
/// antes los hacían ver ralos junto a una trocha de 1.676 m; la vía de trocha
/// ancha los coloca separados de 0.6 a 0.7 m.
pub(crate) const SLEEPER_SPACING: f32 = 0.65;
/// Largo, ancho y espesor de un durmiente, en metros.
pub(crate) const SLEEPER: Vec3 = Vec3::new(2.7, 0.25, 0.12);
/// Distancia entre las muestras en las que se cortan un lecho y sus rieles a lo
/// largo de un tramo recto, en metros. Las curvas ya están hechas de piezas
/// cortas: cada unión también es una muestra.
const STEP: f32 = 2.0;
/// Cuánto más abajo que la anterior queda una vía donde se superponen, en
/// metros: el depth buffer resuelve la superposición y nada compite.
const UNDER: f32 = 0.004;
/// Las vías más cercanas que esto (de centro a centro, en metros) quedan
/// [`UNDER`] por debajo de la anterior.
const OVERLAP: f32 = 3.0;
/// Una vía sigue la altura de las vías anteriores dentro de esta distancia de
/// ellas, y la suya propia más allá de [`BLEND_FAR`], con una mezcla suave
/// entre ambas.
const BLEND_NEAR: f32 = 1.5;
const BLEND_FAR: f32 = 4.5;
/// A qué distancia del centro de una vía anterior se mantienen alejados los
/// extremos de sus durmientes, en metros: la mitad del largo de sus maderas más
/// un palmo.
const TIMBER_CLEAR: f32 = SLEEPER.x / 2.0 + 0.05;
/// El trozo de durmiente más corto que vale la pena colocar, en metros.
const STUB_MIN: f32 = 0.35;

/// El perfil del riel, en (a lo ancho, hacia arriba) desde el centro del riel y
/// la base del patín, en sentido antihorario: la parte superior del patín, el
/// alma, la parte inferior de la cabeza, su costado y su parte superior.
const RAIL_PROFILE: [(f32, f32); 10] = [
    (0.07, 0.02),
    (0.02, 0.05),
    (0.02, 0.105),
    (0.037, 0.11),
    (0.027, 0.14),
    (-0.027, 0.14),
    (-0.037, 0.11),
    (-0.02, 0.105),
    (-0.02, 0.05),
    (-0.07, 0.02),
];
/// Las aristas del perfil (índice del punto en el que empiezan) que forman la
/// cabeza del riel, su banda de rodadura: el costado, la parte superior y el
/// otro costado.
const HEAD_EDGES: [usize; 3] = [3, 4, 5];

/// `v` escalado a largo uno, o `fallback` cuando no tiene largo.
fn unit(v: GroundVec2, fallback: GroundVec2) -> GroundVec2 {
    let len = v.length();
    if len > 1e-6 {
        v * (1.0 / len)
    } else {
        fallback
    }
}

/// El eje de una vía sobre el suelo.
#[derive(Debug, Clone)]
pub(crate) struct Track {
    points: Vec<GroundVec2>,
    /// Distancia a lo largo de la vía hasta cada punto.
    arcs: Vec<f32>,
    /// Distancia entre los rieles, en metros.
    pub gauge: f32,
}

impl Track {
    /// La vía que pasa por `points` (se descartan las piezas de largo cero).
    pub fn new(points: &[GroundVec2], gauge: f32) -> Self {
        let mut kept: Vec<GroundVec2> = Vec::with_capacity(points.len());
        for &p in points {
            if kept.last().is_none_or(|q| q.distance(p) > 1e-4) {
                kept.push(p);
            }
        }
        let mut arcs = vec![0.0];
        for w in kept.windows(2) {
            arcs.push(arcs.last().copied().unwrap_or(0.0) + w[0].distance(w[1]));
        }
        Self {
            points: kept,
            arcs,
            gauge,
        }
    }

    /// La vía recta de un pueblo.
    pub fn straight(rail: Railway) -> Self {
        Self::new(&[rail.start, rail.end], rail.gauge)
    }

    /// Largo de la vía, en metros.
    pub fn length(&self) -> f32 {
        self.arcs.last().copied().unwrap_or(0.0)
    }

    fn pieces(&self) -> usize {
        self.points.len().saturating_sub(1)
    }

    /// Dirección unitaria de la pieza `i`.
    fn dir(&self, i: usize) -> GroundVec2 {
        unit(
            self.points[i + 1] - self.points[i],
            GroundVec2::new(1.0, 0.0),
        )
    }

    /// El punto a `s` metros a lo largo de la vía y la dirección de la pieza en
    /// la que cae.
    pub fn at(&self, s: f32) -> (GroundVec2, GroundVec2) {
        let i = self
            .arcs
            .partition_point(|&a| a <= s)
            .saturating_sub(1)
            .min(self.pieces().saturating_sub(1));
        let (a, b) = (self.points[i], self.points[i + 1]);
        let len = self.arcs[i + 1] - self.arcs[i];
        (a + (b - a) * ((s - self.arcs[i]) / len), self.dir(i))
    }

    /// A qué distancia está `p` de la vía, y en qué lugar de ella cae el punto
    /// más cercano.
    pub fn nearest(&self, p: GroundVec2) -> (f32, f32) {
        let mut best = (f32::MAX, 0.0);
        for i in 0..self.pieces() {
            let (a, b) = (self.points[i], self.points[i + 1]);
            let len = self.arcs[i + 1] - self.arcs[i];
            let t = ((p - a).dot(b - a) / (len * len)).clamp(0.0, 1.0);
            let d = (a + (b - a) * t).distance(p);
            if d < best.0 {
                best = (d, self.arcs[i] + t * len);
            }
        }
        best
    }

    fn bounds(&self) -> Aabb {
        let (mut lo, mut hi) = (self.points[0], self.points[0]);
        for p in &self.points {
            lo = GroundVec2::new(lo.x.min(p.x), lo.y.min(p.y));
            hi = GroundVec2::new(hi.x.max(p.x), hi.y.max(p.y));
        }
        Aabb::new(lo, hi)
    }
}

/// Cada vía que puede quedar cerca de otra, en orden de precedencia: las vías
/// rectas de los pueblos, luego los enlaces de la línea y de sus ramales, y
/// luego las vías muertas. Cada pueblo dibuja su propia vía (ver
/// `crate::town`); la red tiende el resto, chunk por chunk, y conoce las vías
/// de los pueblos solo para mantenerse alejada de ellas.
#[derive(Debug, Clone, Default)]
pub(crate) struct TrackNet {
    tracks: Vec<Track>,
    /// Cuántas de las vías (las primeras) son propias de los pueblos.
    towns: usize,
}

impl TrackNet {
    pub fn new(towns: &[TownLayout], line: &RailLine) -> Self {
        let mut tracks: Vec<Track> = towns
            .iter()
            .filter_map(|t| t.railway)
            .map(Track::straight)
            .collect();
        let count = tracks.len();
        tracks.extend(
            line.links
                .iter()
                .map(|l| Track::new(&l.path, l.track.gauge)),
        );
        tracks.extend(
            line.bays
                .iter()
                .map(|b| Track::new(&b.link.path, b.link.track.gauge)),
        );
        tracks.retain(|t| t.pieces() > 0);
        let towns = tracks.len().min(count);
        Self { tracks, towns }
    }

    /// Las vías anteriores a la vía `i` que se le acercan.
    fn earlier(&self, i: usize) -> Vec<&Track> {
        let near = self.tracks[i]
            .bounds()
            .expanded(BLEND_FAR + BALLAST_FOOT_HALF);
        self.tracks[..i]
            .iter()
            .filter(|t| t.bounds().intersects(&near))
            .collect()
    }

    /// Las mallas de los enlaces y vías muertas dentro de `area` (un chunk de
    /// terreno).
    pub fn lay_in(&self, area: Aabb, terrain: &Terrain) -> Parts {
        let mut parts = Parts::default();
        let reach = area.expanded(BALLAST_FOOT_HALF);
        for i in self.towns..self.tracks.len() {
            let track = &self.tracks[i];
            if track.bounds().expanded(1.0).intersects(&reach) {
                parts.add(track, &self.earlier(i), Some(area), terrain);
            }
        }
        parts
    }
}

/// Los vértices de una malla mientras se construye: quads con sombreado plano.
#[derive(Default, Debug, Clone)]
struct Buf {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    tangents: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

impl Buf {
    /// Un quad cuyas esquinas lo recorren, girado para mirar hacia `outward`.
    fn quad(&mut self, mut c: [Vec3; 4], mut uv: [[f32; 2]; 4], outward: Vec3) {
        let n = (c[2] - c[0]).cross(c[3] - c[1]);
        if n.length_squared() < 1e-12 {
            return;
        }
        if n.dot(outward) < 0.0 {
            c.reverse();
            uv.reverse();
        }
        let n = (c[2] - c[0]).cross(c[3] - c[1]).normalize();
        let tangent = face_tangent([c[0], c[1], c[2]], [uv[0], uv[1], uv[2]]);
        let base = self.positions.len() as u32;
        for k in 0..4 {
            self.positions.push(c[k].to_array());
            self.normals.push(n.to_array());
            self.uvs.push(uv[k]);
            self.tangents.push(tangent);
        }
        self.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    fn into_mesh(self) -> Option<Mesh> {
        (!self.indices.is_empty()).then(|| {
            Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::RENDER_WORLD,
            )
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs)
            .with_inserted_attribute(Mesh::ATTRIBUTE_TANGENT, self.tangents)
            .with_inserted_indices(Indices::U32(self.indices))
        })
    }
}

/// Un corte transversal de una vía: dónde, hacia dónde corre y cuánto ensanchar
/// lo que se coloca atravesado (en la unión de dos piezas el corte sigue la
/// bisectriz).
#[derive(Debug, Clone, Copy)]
struct Ring {
    s: f32,
    pos: GroundVec2,
    dir: GroundVec2,
    widen: f32,
}

impl Ring {
    fn new(s: f32, pos: GroundVec2, dir: GroundVec2) -> Self {
        Self {
            s,
            pos,
            dir,
            widen: 1.0,
        }
    }

    /// El anillo en una unión entre piezas con rumbo `before` y `after`.
    fn joint(s: f32, pos: GroundVec2, before: GroundVec2, after: GroundVec2) -> Self {
        let dir = unit(before + after, before);
        Self {
            s,
            pos,
            dir,
            widen: (1.0 / dir.dot(before).max(0.5_f32)).min(1.5),
        }
    }

    /// El punto del suelo a `across` metros a la izquierda del centro del
    /// anillo.
    fn at(&self, across: f32) -> GroundVec2 {
        self.pos + GroundVec2::new(-self.dir.y, self.dir.x) * (across * self.widen)
    }
}

/// Un anillo con la altura del centro de la vía debajo de él, en metros.
#[derive(Debug, Clone, Copy)]
struct Station {
    ring: Ring,
    base: f32,
}

/// Las alturas de una vía entre las vías anteriores cercanas a ella.
struct Heights<'a> {
    terrain: &'a Terrain,
    earlier: &'a [&'a Track],
}

impl Heights<'_> {
    /// La altura del suelo bajo el centro de la vía en `p`, acercada a la de
    /// las vías anteriores cerca de ellas, y cuánto por debajo de ellas queda
    /// la vía.
    fn at(&self, p: GroundVec2) -> f32 {
        let mut h = self.terrain.height(p.x, p.y);
        let mut under = 0.0;
        for track in self.earlier {
            let (d, s) = track.nearest(p);
            if d < BLEND_FAR {
                let q = track.at(s).0;
                let w = ((d - BLEND_NEAR) / (BLEND_FAR - BLEND_NEAR)).clamp(0.0, 1.0);
                let w = w * w * (3.0 - 2.0 * w);
                h = self.terrain.height(q.x, q.y) * (1.0 - w) + h * w;
            }
            if d < OVERLAP {
                under += UNDER;
            }
        }
        h - under
    }
}

/// Los cortes transversales de `track` dentro de `clip` (toda la vía si no hay
/// recorte), en tramos de piezas consecutivas: uno en cada extremo, uno en cada
/// unión y uno cada [`STEP`] metros entre medio. Un tramo se detiene donde la
/// vía sale del área.
fn runs(track: &Track, clip: Option<Aabb>) -> Vec<Vec<Ring>> {
    let mut out: Vec<Vec<Ring>> = Vec::new();
    let mut open: Vec<Ring> = Vec::new();
    // Si el tramo abierto termina en la unión desde la que empieza la pieza
    // siguiente.
    let mut at_joint = false;
    let pieces = track.pieces();
    for i in 0..pieces {
        let (a, b) = (track.points[i], track.points[i + 1]);
        let len = track.arcs[i + 1] - track.arcs[i];
        let overlap = match clip {
            None => Some((0.0, 1.0)),
            Some(area) => area.segment_overlap(a, b),
        };
        let Some((t0, t1)) = overlap.filter(|(t0, t1)| (t1 - t0) * len > 1e-3) else {
            if !open.is_empty() {
                out.push(std::mem::take(&mut open));
            }
            at_joint = false;
            continue;
        };
        let dir = track.dir(i);
        let continues = at_joint && t0 <= 0.0;
        if !continues && !open.is_empty() {
            out.push(std::mem::take(&mut open));
        }
        let n = (((t1 - t0) * len / STEP).ceil() as usize).max(1);
        for k in (continues as usize)..=n {
            let t = t0 + (t1 - t0) * k as f32 / n as f32;
            let (s, pos) = (track.arcs[i] + t * len, a + (b - a) * t);
            let ring = if k == 0 && t <= 0.0 && i > 0 {
                Ring::joint(s, pos, track.dir(i - 1), dir)
            } else if k == n && t >= 1.0 && i + 1 < pieces {
                Ring::joint(s, pos, dir, track.dir(i + 1))
            } else {
                Ring::new(s, pos, dir)
            };
            open.push(ring);
        }
        at_joint = t1 >= 1.0 && i + 1 < pieces;
        if !at_joint {
            out.push(std::mem::take(&mut open));
        }
    }
    if !open.is_empty() {
        out.push(open);
    }
    out
}

/// Las mallas de una o más vías, una por superficie.
#[derive(Default, Debug, Clone)]
pub(crate) struct Parts {
    ballast: Buf,
    sleepers: Buf,
    rail: Buf,
    head: Buf,
}

impl Parts {
    /// Tiende `track` dentro de `clip` (toda la vía si no hay recorte) entre
    /// las vías anteriores `earlier` cercanas a ella.
    pub fn add(
        &mut self,
        track: &Track,
        earlier: &[&Track],
        clip: Option<Aabb>,
        terrain: &Terrain,
    ) {
        let heights = Heights { terrain, earlier };
        for run in runs(track, clip) {
            let stations: Vec<Station> = run
                .iter()
                .map(|&ring| Station {
                    ring,
                    base: heights.at(ring.pos),
                })
                .collect();
            self.bed(&stations, terrain);
            for side in [-0.5, 0.5] {
                self.rails(&stations, side * track.gauge);
            }
            let (s0, s1) = (run[0].s, run[run.len() - 1].s);
            let last = s1 >= track.length() - 1e-3;
            self.sleepers(track, (s0, s1, last), &heights);
        }
    }

    /// El balasto: un trapecio barrido a lo largo de las estaciones de corte,
    /// con su parte superior plana a lo ancho a la altura del centro y sus
    /// hombros bajando hasta el suelo a cada lado.
    fn bed(&mut self, stations: &[Station], terrain: &Terrain) {
        let tile = Surface::Ballast.tile();
        // Pie derecho, borde superior derecho, borde superior izquierdo, pie
        // izquierdo: los puntos del perfil según cuán lejos están a lo ancho
        // (positivo a la izquierda), con el arco a lo ancho del perfil para
        // `v`.
        let shoulder = (BALLAST_FOOT_HALF - BALLAST_TOP_HALF).hypot(BALLAST_TOP + BALLAST_SINK);
        let across = [
            BALLAST_FOOT_HALF,
            BALLAST_TOP_HALF,
            -BALLAST_TOP_HALF,
            -BALLAST_FOOT_HALF,
        ];
        let arc = [
            0.0,
            shoulder,
            shoulder + 2.0 * BALLAST_TOP_HALF,
            2.0 * shoulder + 2.0 * BALLAST_TOP_HALF,
        ];
        let point = |st: &Station, k: usize| {
            let g = st.ring.at(-across[k]);
            let y = if k == 1 || k == 2 {
                st.base + BALLAST_TOP
            } else {
                terrain.height(g.x, g.y) - BALLAST_SINK
            };
            world_from_ground(g, y)
        };
        for w in stations.windows(2) {
            for k in 0..3 {
                let corner =
                    |st: &Station, j: usize| (point(st, j), [st.ring.s / tile, arc[j] / tile]);
                let (p0, p1) = (corner(&w[0], k), corner(&w[0], k + 1));
                let (p2, p3) = (corner(&w[1], k + 1), corner(&w[1], k));
                // La cara del medio mira hacia arriba; los hombros miran hacia
                // afuera y hacia arriba.
                let outward = match k {
                    0 => Vec3::new(0.0, 1.0, 0.0) + (p0.0 - p1.0).with_y(0.0).normalize_or_zero(),
                    1 => Vec3::Y,
                    _ => Vec3::new(0.0, 1.0, 0.0) + (p1.0 - p0.0).with_y(0.0).normalize_or_zero(),
                };
                self.ballast
                    .quad([p0.0, p1.0, p2.0, p3.0], [p0.1, p1.1, p2.1, p3.1], outward);
            }
        }
    }

    /// Un riel, a `across` metros a la izquierda del centro: el perfil barrido
    /// a lo largo de las estaciones de corte, con la cabeza separada del patín
    /// y el alma.
    fn rails(&mut self, stations: &[Station], across: f32) {
        let tile = Surface::RailSteel.tile();
        // El arco del perfil, para `v`, y una parte distinta del escaneo para
        // cada riel.
        let mut arc = vec![across.signum() * 0.37];
        for w in RAIL_PROFILE.windows(2) {
            let d = (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1);
            arc.push(arc.last().copied().unwrap_or(0.0) + d);
        }
        let point = |st: &Station, k: usize| {
            let (x, y) = RAIL_PROFILE[k];
            world_from_ground(st.ring.at(across + x), st.base + RAIL_FOOT + y)
        };
        for w in stations.windows(2) {
            for k in 0..RAIL_PROFILE.len() - 1 {
                let corners = [
                    point(&w[0], k),
                    point(&w[0], k + 1),
                    point(&w[1], k + 1),
                    point(&w[1], k),
                ];
                let uv = |st: &Station, j: usize| [st.ring.s / tile, arc[j] / tile];
                let uvs = [
                    uv(&w[0], k),
                    uv(&w[0], k + 1),
                    uv(&w[1], k + 1),
                    uv(&w[1], k),
                ];
                // Hacia afuera es la normal de la arista en el plano del
                // perfil.
                let (a, b) = (RAIL_PROFILE[k], RAIL_PROFILE[k + 1]);
                let (du, dl) = (b.1 - a.1, b.0 - a.0);
                let left = Vec3::new(-w[0].ring.dir.y, 0.0, -w[0].ring.dir.x);
                let outward = -left * du + Vec3::Y * -dl;
                let buf = if HEAD_EDGES.contains(&k) {
                    &mut self.head
                } else {
                    &mut self.rail
                };
                buf.quad(corners, uvs, outward);
            }
        }
    }

    /// Los durmientes de `track` desde `s0` hasta `s1` (`s1` mismo solo cuando
    /// es el final de la vía), acortados donde hay maderas de una vía anterior.
    fn sleepers(&mut self, track: &Track, (s0, s1, last): (f32, f32, bool), heights: &Heights) {
        let count = (track.length() / SLEEPER_SPACING) as usize;
        let first = (s0 / SLEEPER_SPACING - 0.5).ceil().max(0.0) as usize;
        for k in first..count {
            let s = (k as f32 + 0.5) * SLEEPER_SPACING;
            if s > s1 || (s == s1 && !last) {
                break;
            }
            if s < s0 {
                continue;
            }
            let (c, dir) = track.at(s);
            let left = GroundVec2::new(-dir.y, dir.x);
            let top = heights.at(c) + RAIL_FOOT;
            for (u0, u1) in stubs(c, left, heights.earlier) {
                let mid = c + left * ((u0 + u1) / 2.0);
                self.sleeper(mid, dir, u1 - u0, top, k as u32);
            }
        }
    }

    /// Un durmiente: una caja de `length` de largo atravesada sobre la vía en
    /// `mid`, con su parte superior en `top` y la veta a lo largo.
    fn sleeper(&mut self, mid: GroundVec2, dir: GroundVec2, length: f32, top: f32, seed: u32) {
        let tile = Surface::Sleeper.tile();
        let left = GroundVec2::new(-dir.y, dir.x);
        let (hl, hw) = (length / 2.0, SLEEPER.y / 2.0);
        let corner = |l: f32, w: f32, up: f32| {
            world_from_ground(mid + left * l + dir * w, top - SLEEPER.z * (1.0 - up))
        };
        // Cada durmiente muestra otra parte del escaneo.
        let h = seed.wrapping_mul(0x9e37_79b9) ^ 0x85eb_ca6b;
        let (ou, ov) = ((h & 0xff) as f32 / 256.0, ((h >> 8) & 0xff) as f32 / 256.0);
        let uv = |a: f32, b: f32| [a / tile + ou, b / tile + ov];
        let (wl, ww) = (length / tile, SLEEPER.y / tile);
        let _ = (wl, ww);
        let lw = |l: f32| l + hl;
        let ww = |w: f32| w + hw;
        let left3 = Vec3::new(-dir.y, 0.0, -dir.x);
        let along3 = Vec3::new(dir.x, 0.0, -dir.y);
        // La parte superior, los dos costados largos y los dos extremos.
        self.sleepers.quad(
            [
                corner(-hl, -hw, 1.0),
                corner(hl, -hw, 1.0),
                corner(hl, hw, 1.0),
                corner(-hl, hw, 1.0),
            ],
            [
                uv(ww(-hw), lw(-hl)),
                uv(ww(-hw), lw(hl)),
                uv(ww(hw), lw(hl)),
                uv(ww(hw), lw(-hl)),
            ],
            Vec3::Y,
        );
        for (w, out) in [(-hw, -along3), (hw, along3)] {
            self.sleepers.quad(
                [
                    corner(-hl, w, 0.0),
                    corner(hl, w, 0.0),
                    corner(hl, w, 1.0),
                    corner(-hl, w, 1.0),
                ],
                [
                    uv(0.0, lw(-hl)),
                    uv(0.0, lw(hl)),
                    uv(SLEEPER.z, lw(hl)),
                    uv(SLEEPER.z, lw(-hl)),
                ],
                out,
            );
        }
        for (l, out) in [(-hl, -left3), (hl, left3)] {
            self.sleepers.quad(
                [
                    corner(l, -hw, 0.0),
                    corner(l, hw, 0.0),
                    corner(l, hw, 1.0),
                    corner(l, -hw, 1.0),
                ],
                [
                    uv(ww(-hw), 0.0),
                    uv(ww(hw), 0.0),
                    uv(ww(hw), SLEEPER.z),
                    uv(ww(-hw), SLEEPER.z),
                ],
                out,
            );
        }
    }

    /// Una malla por superficie, las que tienen algo.
    pub fn into_meshes(self) -> Vec<(Surface, Mesh)> {
        [
            (Surface::Ballast, self.ballast),
            (Surface::Sleeper, self.sleepers),
            (Surface::RailSteel, self.rail),
            (Surface::RailHead, self.head),
        ]
        .into_iter()
        .filter_map(|(surface, buf)| buf.into_mesh().map(|mesh| (surface, mesh)))
        .collect()
    }

    /// Triángulos colocados hasta ahora.
    #[cfg(test)]
    pub fn triangles(&self) -> usize {
        [&self.ballast, &self.sleepers, &self.rail, &self.head]
            .iter()
            .map(|b| b.indices.len() / 3)
            .sum()
    }
}

/// Los trechos (como metros `(from, to)` a la izquierda de `c`, a lo largo del
/// eje de un durmiente) de un durmiente en `c` que se mantienen a
/// [`TIMBER_CLEAR`] del centro de cada vía anterior, solo con extremos de al
/// menos [`STUB_MIN`]. Un durmiente alejado de todas ellas es un único trecho
/// de punta a punta.
fn stubs(c: GroundVec2, left: GroundVec2, earlier: &[&Track]) -> Vec<(f32, f32)> {
    let half = SLEEPER.x / 2.0;
    let near = earlier.iter().any(|t| t.nearest(c).0 < TIMBER_CLEAR + half);
    if !near {
        return vec![(-half, half)];
    }
    let clear = |u: f32| {
        let p = c + left * u;
        earlier.iter().all(|t| t.nearest(p).0 >= TIMBER_CLEAR)
    };
    const SAMPLES: usize = 54;
    let at = |j: usize| -half + SLEEPER.x * j as f32 / SAMPLES as f32;
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for j in 0..=SAMPLES + 1 {
        let ok = j <= SAMPLES && clear(at(j));
        match (ok, start) {
            (true, None) => start = Some(j),
            (false, Some(j0)) => {
                let (u0, u1) = (at(j0), at(j - 1));
                if u1 - u0 >= STUB_MIN {
                    out.push((u0, u1));
                }
                start = None;
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Una vía a lo largo de `points`, con trocha de 1.676 m.
    fn track(points: &[(f32, f32)]) -> Track {
        let points: Vec<GroundVec2> = points.iter().map(|&(x, y)| GroundVec2::new(x, y)).collect();
        Track::new(&points, 1.676)
    }

    /// Una vía recta hacia el este desde (0, 0), de `length` metros de largo.
    fn straight(length: f32) -> Track {
        track(&[(0.0, 0.0), (length, 0.0)])
    }

    /// El giro de `track` hacia la izquierda en `turn` radianes a lo largo de
    /// `length` metros desde (0, 0) con rumbo este, en piezas de 2 grados.
    fn arc(length: f32, turn: f32) -> Track {
        let pieces = (turn.abs() / 0.035).ceil().max(1.0) as usize;
        let (mut p, mut heading) = (GroundVec2::new(0.0, 0.0), 0.0_f32);
        let mut points = vec![p];
        for _ in 0..pieces {
            let step = length / pieces as f32;
            heading += turn / pieces as f32;
            p += GroundVec2::new(heading.cos(), heading.sin()) * step;
            points.push(p);
        }
        Track::new(&points, 1.676)
    }

    /// Punto del suelo de una posición del mundo.
    fn ground(v: [f32; 3]) -> GroundVec2 {
        GroundVec2::new(v[0], -v[2])
    }

    fn flat(height: f32) -> Terrain {
        Terrain::default().with_flat_area(pampa_core::FlatArea {
            center: GroundVec2::new(0.0, 0.0),
            half_extents: GroundVec2::new(400.0, 400.0),
            falloff: 10.0,
            height,
        })
    }

    fn lay(track: &Track, terrain: &Terrain) -> Parts {
        let mut parts = Parts::default();
        parts.add(track, &[], None, terrain);
        parts
    }

    #[test]
    fn the_ballast_follows_a_curved_path_and_the_bumpy_ground() {
        // Suelo natural, una curva a la izquierda: no se aplana nada.
        let terrain = Terrain::default();
        let track = arc(120.0, 0.25);
        let parts = lay(&track, &terrain);
        assert!(!parts.ballast.positions.is_empty());
        for &v in &parts.ballast.positions {
            let g = ground(v);
            let (d, s) = track.nearest(g);
            // Un vértice está en el borde de la parte superior o en el pie del
            // hombro.
            let top = (d - 1.4).abs() < 0.02;
            let foot = (d - 1.9).abs() < 0.02;
            assert!(top || foot, "{d} m from the path");
            if foot {
                // Los hombros terminan 5 cm por debajo del suelo mismo.
                let ground = terrain.height(g.x, g.y);
                assert!(
                    (v[1] - (ground - 0.05)).abs() < 1e-3,
                    "{} vs {ground}",
                    v[1]
                );
            } else {
                // La parte superior está nivelada a lo ancho, 30 cm por encima
                // del suelo del centro.
                let c = track.at(s).0;
                let base = terrain.height(c.x, c.y);
                assert!((v[1] - (base + 0.30)).abs() < 0.03, "{} vs {base}", v[1]);
            }
        }
    }

    #[test]
    fn every_triangle_faces_the_way_its_normal_says_and_none_is_degenerate() {
        let terrain = Terrain::default();
        // Curvas a la derecha y a la izquierda, para probar el sentido de
        // enrollado en ambos casos.
        for turn in [0.3, -0.3] {
            let parts = lay(&arc(100.0, turn), &terrain);
            let all = [
                (&parts.ballast, true),
                (&parts.sleepers, false),
                (&parts.rail, false),
                (&parts.head, false),
            ];
            for (buf, is_ballast) in all {
                assert!(!buf.indices.is_empty());
                for t in buf.indices.chunks(3) {
                    let p = |i: u32| Vec3::from_array(buf.positions[i as usize]);
                    let n = (p(t[1]) - p(t[0])).cross(p(t[2]) - p(t[0]));
                    assert!(n.length() > 1e-7, "degenerate triangle");
                    let stored = Vec3::from_array(buf.normals[t[0] as usize]);
                    assert!(n.normalize().dot(stored) > 0.99, "inverted triangle");
                    if is_ballast {
                        // Cada cara del lecho mira hacia arriba, nunca hacia
                        // abajo.
                        assert!(stored.y > 0.3, "{stored:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn the_ballast_textures_scale_with_meters_not_with_the_piece() {
        let terrain = flat(3.0);
        for length in [30.0, 90.0] {
            let parts = lay(&straight(length), &terrain);
            let us: Vec<f32> = parts.ballast.uvs.iter().map(|uv| uv[0]).collect();
            let span = us.iter().copied().fold(f32::MIN, f32::max)
                - us.iter().copied().fold(f32::MAX, f32::min);
            // Una repetición cada 2 m.
            assert!((span - length / 2.0).abs() < 1e-3, "{span}");
            // A lo ancho: la parte superior (2.8 m) y los dos hombros (0.61 m
            // cada uno), cada 2 m.
            let vs: Vec<f32> = parts.ballast.uvs.iter().map(|uv| uv[1]).collect();
            let across = vs.iter().copied().fold(f32::MIN, f32::max)
                - vs.iter().copied().fold(f32::MAX, f32::min);
            assert!((across - 2.0103).abs() < 0.005, "{across}");
        }
    }

    /// Los durmientes colocados: (centro, eje largo, largo, ancho), leídos de
    /// la cara superior de cada durmiente (el primer quad de sus 20 vértices).
    fn sleepers_of(parts: &Parts) -> Vec<(GroundVec2, GroundVec2, f32, f32)> {
        parts
            .sleepers
            .positions
            .chunks(20)
            .map(|c| {
                let q: Vec<GroundVec2> = c[..4].iter().map(|&v| ground(v)).collect();
                let (e1, e2) = (q[1] - q[0], q[2] - q[1]);
                let (long, short) = if e1.length() > e2.length() {
                    (e1, e2)
                } else {
                    (e2, e1)
                };
                let centre = (q[0] + q[2]) * 0.5;
                (centre, unit(long, long), long.length(), short.length())
            })
            .collect()
    }

    #[test]
    fn sleepers_lie_every_65_cm_square_to_the_track() {
        let terrain = flat(3.0);
        for (heading, end) in [
            (0.0_f32, (60.0, 0.0)),
            (30.0, (51.96, 30.0)),
            (135.0, (-42.4, 42.4)),
        ] {
            let track = track(&[(10.0, -5.0), (10.0 + end.0, -5.0 + end.1)]);
            let sleepers = sleepers_of(&lay(&track, &terrain));
            assert_eq!(sleepers.len(), 92, "{heading}°");
            let d = unit(track.points[1] - track.points[0], GroundVec2::new(1.0, 0.0));
            let mut last: Option<GroundVec2> = None;
            for (c, axis, length, width) in sleepers {
                assert!((length - 2.7).abs() < 1e-3 && (width - 0.25).abs() < 1e-3);
                assert!(axis.dot(d).abs() < 1e-3, "{heading}°: not square");
                if let Some(l) = last {
                    assert!((c.distance(l) - 0.65).abs() < 1e-3, "{heading}°");
                }
                last = Some(c);
            }
        }
    }

    #[test]
    fn sleepers_of_neighboring_chunks_add_up_to_the_whole_track() {
        let terrain = flat(3.0);
        let track = straight(150.0);
        let whole = lay(&track, &terrain).triangles();
        let mut cut = Parts::default();
        for (x0, x1) in [(-1.0, 47.3), (47.3, 100.0), (100.0, 151.0)] {
            let area = Aabb::new(GroundVec2::new(x0, -50.0), GroundVec2::new(x1, 50.0));
            cut.add(&track, &[], Some(area), &terrain);
        }
        assert_eq!(sleepers_of(&cut).len(), 230);
        assert_eq!(sleepers_of(&cut).len(), (150.0_f32 / 0.65) as usize);
        // El balasto y los rieles se cortan en los bordes: los mismos
        // triángulos, más los de las muestras extra que hay allí.
        assert!(cut.triangles() >= whole && cut.triangles() < whole + 2 * 40);
    }

    #[test]
    fn the_rails_top_stands_at_the_trains_rail_height_a_gauge_apart() {
        let terrain = flat(3.0);
        let parts = lay(&straight(60.0), &terrain);
        let top = parts
            .head
            .positions
            .iter()
            .map(|v| v[1])
            .fold(f32::MIN, f32::max);
        assert!((top - (3.0 + 0.5)).abs() < 1e-3, "{top}");
        // El patín se apoya sobre los durmientes (3.36), debajo de la cabeza.
        let low = parts
            .rail
            .positions
            .iter()
            .map(|v| v[1])
            .fold(f32::MAX, f32::min);
        assert!(low > 3.0 + 0.36, "{low}");
        // Los centros de los rieles: la media del lado de cada cabeza que da al
        // suelo.
        let mean = |sign: f32| {
            let ys: Vec<f32> = parts
                .head
                .positions
                .iter()
                .map(|&v| ground(v).y)
                .filter(|y| y * sign > 0.0)
                .collect();
            ys.iter().sum::<f32>() / ys.len() as f32
        };
        assert!((mean(1.0) - 0.838).abs() < 2e-3, "{}", mean(1.0));
        assert!((mean(-1.0) + 0.838).abs() < 2e-3, "{}", mean(-1.0));
    }

    /// Si dos durmientes (centro, eje, largo, ancho) se tocan, según el teorema
    /// del eje separador.
    fn touch(a: (GroundVec2, GroundVec2, f32, f32), b: (GroundVec2, GroundVec2, f32, f32)) -> bool {
        let axes = [
            a.1,
            GroundVec2::new(-a.1.y, a.1.x),
            b.1,
            GroundVec2::new(-b.1.y, b.1.x),
        ];
        let reach = |s: (GroundVec2, GroundVec2, f32, f32), axis: GroundVec2| {
            let side = GroundVec2::new(-s.1.y, s.1.x);
            (s.1.dot(axis) * s.2 / 2.0).abs() + (side.dot(axis) * s.3 / 2.0).abs()
        };
        axes.into_iter().all(|axis| {
            let gap = (b.0 - a.0).dot(axis).abs();
            gap < reach(a, axis) + reach(b, axis)
        })
    }

    /// Una línea y una vía similar a una vía muerta que sale de ella en forma
    /// tangente: dos arcos de 150 m que giran 0.2 rad hacia afuera y de vuelta,
    /// y terminan a 4.5 m de ella.
    fn turnout() -> (Track, Track) {
        let line = straight(200.0);
        let mut points = vec![GroundVec2::new(0.0, 0.0)];
        let (mut p, mut heading) = (GroundVec2::new(0.0, 0.0), 0.0_f32);
        for k in 0..40 {
            heading += if k < 20 { 0.01 } else { -0.01 };
            p += GroundVec2::new(heading.cos(), heading.sin()) * 3.0;
            points.push(p);
        }
        (line, Track::new(&points, 1.676))
    }

    #[test]
    fn a_turnout_lays_no_two_sleepers_across_each_other() {
        let (line, branch) = turnout();
        let terrain = Terrain::default();
        let mut both = Parts::default();
        both.add(&line, &[], None, &terrain);
        both.add(&branch, &[&line], None, &terrain);
        let sleepers = sleepers_of(&both);
        // El ramal solo tiene más: sus primeros se cortan o se descartan.
        let alone = sleepers_of(&lay(&branch, &terrain)).len();
        let only_line = sleepers_of(&lay(&line, &terrain)).len();
        assert!(sleepers.len() < alone + only_line, "cuts nothing");
        assert!(sleepers.len() > only_line + alone / 2, "{}", sleepers.len());
        for (i, a) in sleepers.iter().enumerate() {
            for b in &sleepers[i + 1..] {
                assert!(!touch(*a, *b), "{a:?} and {b:?} cross");
            }
        }
    }

    #[test]
    fn the_branchs_rails_stay_on_timber_all_along_the_turnout() {
        let (line, branch) = turnout();
        let terrain = flat(3.0);
        let mut both = Parts::default();
        both.add(&line, &[], None, &terrain);
        both.add(&branch, &[&line], None, &terrain);
        let timbers = sleepers_of(&both);
        // Cada punto de los dos rieles del ramal, cada medio metro, cae sobre
        // la huella de algún durmiente (con un pelo de margen).
        let mut s = 1.0;
        while s < branch.length() - 1.0 {
            let (c, dir) = branch.at(s);
            let left = GroundVec2::new(-dir.y, dir.x);
            for side in [-0.838, 0.838] {
                let p = c + left * side;
                let held = timbers.iter().any(|&(mid, axis, length, width)| {
                    let rel = p - mid;
                    let side_axis = GroundVec2::new(-axis.y, axis.x);
                    rel.dot(axis).abs() <= length / 2.0 + 0.05
                        && rel.dot(side_axis).abs() <= width / 2.0 + 0.45
                });
                assert!(held, "rail at {p:?} floats");
            }
            s += 0.5;
        }
    }

    #[test]
    fn a_track_under_another_lies_a_few_millimeters_below_it() {
        let terrain = flat(3.0);
        let line = straight(100.0);
        let twin = straight(100.0);
        let mut both = Parts::default();
        both.add(&line, &[], None, &terrain);
        both.add(&twin, &[&line], None, &terrain);
        let tops: Vec<f32> = both.ballast.positions.iter().map(|v| v[1]).collect();
        let high = tops.iter().copied().fold(f32::MIN, f32::max);
        let low = tops
            .iter()
            .copied()
            .filter(|y| *y > 3.2)
            .fold(f32::MAX, f32::min);
        assert!((high - 3.30).abs() < 1e-3, "{high}");
        assert!((high - low - 0.004).abs() < 1e-4, "{high} vs {low}");
        // Todos sus durmientes se descartan: se apoya sobre los de la línea.
        assert_eq!(sleepers_of(&both).len(), 153);
    }

    #[test]
    fn a_kilometer_of_track_stays_inside_its_triangle_budget() {
        let parts = lay(&straight(1000.0), &flat(3.0));
        // Unos 3 triángulos por metro de lecho, 18 de riel y 15 de durmientes.
        let per_meter = parts.triangles() as f32 / 1000.0;
        assert!(per_meter < 40.0, "{per_meter} triangles a meter");
        println!(
            "track: {} tris/km (ballast {}, sleepers {}, rails {})",
            parts.triangles(),
            parts.ballast.indices.len() / 3,
            parts.sleepers.indices.len() / 3,
            (parts.rail.indices.len() + parts.head.indices.len()) / 3
        );
    }

    #[test]
    fn the_net_lays_links_and_bays_but_not_the_towns_own_track() {
        let town_track = Railway {
            start: GroundVec2::new(0.0, 0.0),
            end: GroundVec2::new(200.0, 0.0),
            gauge: 1.676,
        };
        let link = Track::new(
            &[GroundVec2::new(200.0, 0.0), GroundVec2::new(500.0, 0.0)],
            1.676,
        );
        let net = TrackNet {
            tracks: vec![Track::straight(town_track), link],
            towns: 1,
        };
        let area =
            |x0: f32, x1: f32| Aabb::new(GroundVec2::new(x0, -50.0), GroundVec2::new(x1, 50.0));
        let terrain = Terrain::default();
        assert!(net.lay_in(area(0.0, 100.0), &terrain).triangles() == 0);
        assert!(net.lay_in(area(300.0, 400.0), &terrain).triangles() > 0);
    }
}
