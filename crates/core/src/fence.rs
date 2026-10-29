//! Alambrados que delimitan la tierra privada entre los pueblos: estancias,
//! tambos, campos y parcelas, con tranqueras de madera para entrar.
//!
//! Los caminos son públicos (usuario, 2026-10-22): ningún alambrado ni
//! tranquera cruza nunca uno. Todo camino corre entre dos alambrados, uno a
//! cada lado, a [`FenceLayout::ROADSIDE`] metros de su línea central, siguiendo
//! sus curvas: la clásica "calle entre alambrados". Terminan en el terreno
//! nivelado de los pueblos, junto a la vía férrea y, por lo tanto, en los pasos
//! a nivel.
//!
//! Lejos de los caminos, la tierra está relevada en una cuadrícula norte–sur,
//! como lo estaba la pampa: las líneas de alambrado corren de este a oeste y de
//! norte a sur, separadas por una distancia con semilla de 300–800 m, así que
//! los potreros tienen unos cientos de metros de lado. Una línea de la
//! cuadrícula se detiene en un alambrado al costado del camino y se une a él en
//! un poste esquinero. Cada tramo de alambrado al costado del camino entre dos
//! esquinas es el frente de un campo sobre el camino, y tiene una o dos
//! tranqueras abiertas hacia el campo. Las líneas de la cuadrícula tienen
//! aberturas cada 300–500 m (usuario, "Alambrados con huecos", 2026-10-22): una
//! tranquera de campo abierta o un tramo roto.
//!
//! Los postes están a [`FenceLayout::POST_SPACING`] metros como mucho entre sí,
//! con [`FenceLayout::WIRES`] alambres entre ellos. Cada tramo es un obstáculo
//! estático delgado (rectángulos cortos a lo largo de él, para que un tramo
//! inclinado siga siendo delgado), así que el alambre detiene a jinetes y
//! peatones, que pasan por las tranqueras, por las aberturas o rodeando el
//! extremo de un alambrado. Las tranqueras y las aberturas no son obstáculos.

use crate::rng::Rng;
use crate::{Aabb, RailLine, RoadNetwork, Terrain, TownLayout, Vec2, WorldConfig};

/// Un tramo continuo de alambrado: sus postes en orden, a
/// [`FenceLayout::POST_SPACING`] metros como mucho entre sí.
#[derive(Debug, Clone, PartialEq)]
pub struct FenceRun {
    pub posts: Vec<Vec2>,
    /// A lo largo de un camino, y no en la cuadrícula del relevamiento.
    pub roadside: bool,
}

/// Una tranquera abierta en un alambrado al costado del camino, del camino
/// hacia un campo: dos postes de tranquera separados por `width`, centrados en
/// `at`, a lo largo del alambrado en ese punto.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gate {
    pub at: Vec2,
    /// Dirección unitaria del alambrado en la tranquera.
    pub along: Vec2,
    pub width: f32,
    /// Dos hojas en lugar de una.
    pub double: bool,
    /// Abierta; una tranquera cerrada bloquea a los jinetes igual que el
    /// alambre.
    pub open: bool,
    /// El frente de campo desde el que se abre (un índice dentro de
    /// [`FenceLayout::frontages`]).
    pub frontage: u32,
}

/// El frente de un campo sobre un camino: un tramo de alambrado al costado del
/// camino entre dos esquinas donde se unen las líneas de la cuadrícula, o los
/// extremos del alambrado.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frontage {
    pub start: Vec2,
    pub end: Vec2,
    /// Longitud a lo largo del alambrado, en metros.
    pub length: f32,
    /// Alambrado, con tranqueras; `false` a lo largo de un descampado.
    pub fenced: bool,
}

/// Una abertura transitable en un alambrado de la cuadrícula (usuario,
/// "Alambrados con huecos", 2026-10-22): uno o dos tramos, centrados en `at`,
/// de `width` metros a lo largo del alambrado, sin obstáculo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Opening {
    pub at: Vec2,
    /// Dirección unitaria de la línea de alambrado.
    pub along: Vec2,
    pub width: f32,
    pub kind: OpeningKind,
    /// El tramo alambrado en el que se abrió (ver [`FenceLayout::stretches`]).
    pub stretch: u32,
}

/// Cómo se ve una abertura en un alambrado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpeningKind {
    /// Una tranquera de campo con una hoja o dos, abierta o cerrada.
    FieldGate { double: bool, open: bool },
    /// Alambre caído o roto: postes inclinados y un hilo combado.
    Broken,
}

/// Todos los alambrados, tranqueras y aberturas del mundo.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FenceLayout {
    pub runs: Vec<FenceRun>,
    /// Tranqueras abiertas desde los caminos hacia los campos.
    pub gates: Vec<Gate>,
    /// Todos los frentes de campo sobre un camino.
    pub frontages: Vec<Frontage>,
    /// Tranqueras de campo y tramos rotos a lo largo de los alambrados de la
    /// cuadrícula.
    pub openings: Vec<Opening>,
    /// Cada tramo alambrado de la cuadrícula antes de que se abrieran sus
    /// aberturas: su id y su longitud en metros.
    pub stretches: Vec<(u32, f32)>,
    /// Por dónde corren las líneas norte–sur (su `x`) y las este–oeste (su
    /// `y`), estén alambradas ahí o no.
    pub surveyed: (Vec<f32>, Vec<f32>),
    /// Los potreros que quedaron sin alambrar (descampados, campos
    /// abandonados), como `(i, j)`: entre la línea norte–sur número `i` y la
    /// siguiente, y entre la línea este–oeste número `j` y la siguiente.
    pub descampados: Vec<(usize, usize)>,
}

/// Qué es un tramo de una línea de la cuadrícula.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Span {
    Fenced,
    Opening,
    Road,
    Blocked,
}

/// Un alambrado al costado del camino antes de dividirlo en frentes y
/// tranqueras: una línea densa al costado de un camino, y las esquinas donde se
/// le unen las líneas de la cuadrícula (como distancias a lo largo de ella).
struct Roadside {
    line: Vec<Vec2>,
    arcs: Vec<f32>,
    corners: Vec<f32>,
}

impl Roadside {
    fn length(&self) -> f32 {
        self.arcs.last().copied().unwrap_or(0.0)
    }

    /// El punto a `s` metros del recorrido, y la dirección unitaria en ese
    /// punto.
    fn at(&self, s: f32) -> (Vec2, Vec2) {
        let i = self
            .arcs
            .partition_point(|&a| a <= s)
            .saturating_sub(1)
            .min(self.line.len() - 2);
        let (a, b) = (self.line[i], self.line[i + 1]);
        let span = (self.arcs[i + 1] - self.arcs[i]).max(1e-6);
        let t = ((s - self.arcs[i]) / span).clamp(0.0, 1.0);
        (a + (b - a) * t, (b - a) * (1.0 / span))
    }

    /// Postes desde `from` hasta `to` metros del recorrido, separados como
    /// mucho por un espaciado de poste, y más juntos en las curvas cerradas
    /// para que ningún tramo corte la curva por más de unos pocos centímetros.
    fn posts(&self, from: f32, to: f32) -> Vec<Vec2> {
        let n = ((to - from) / FenceLayout::POST_SPACING).ceil().max(1.0) as usize;
        let mut posts = vec![self.at(from).0];
        for k in 0..n {
            let (a, b) = (
                from + (to - from) * (k as f32 / n as f32),
                from + (to - from) * ((k + 1) as f32 / n as f32),
            );
            self.subdivide(a, b, &mut posts);
        }
        posts
    }

    /// Agrega el poste en `b`, después de los que hagan falta entre `a` y `b`.
    fn subdivide(&self, a: f32, b: f32, posts: &mut Vec<Vec2>) {
        let (pa, pb) = (self.at(a).0, self.at(b).0);
        let mid = self.at((a + b) / 2.0).0;
        if b - a > 1.0 && mid.distance((pa + pb) * 0.5) > 0.05 {
            self.subdivide(a, (a + b) / 2.0, posts);
            self.subdivide((a + b) / 2.0, b, posts);
        } else {
            posts.push(pb);
        }
    }
}

impl FenceLayout {
    /// Distancia máxima entre postes, en metros.
    pub const POST_SPACING: f32 = 10.0;
    /// Alambres tendidos entre los postes.
    pub const WIRES: usize = 5;
    /// Altura de los postes sobre el suelo, en metros.
    pub const HEIGHT: f32 = 1.3;
    /// Espesor del obstáculo de un tramo, en metros: más o menos el de un
    /// poste.
    pub const THICKNESS: f32 = 0.15;
    /// Distancia mínima y máxima entre dos líneas paralelas de la cuadrícula,
    /// en metros.
    pub const FIELD: (f32, f32) = (300.0, 800.0);
    /// Distancia desde la línea central de un camino hasta los alambrados que
    /// lo bordean, en metros (8,5 m): su borde más una banquina de 5,25 m (para
    /// que un tramo que corta una curva cerrada todavía deje 5 m), de modo que
    /// la calle pública entre los alambrados tenga lugar para carros y, más
    /// adelante, una ruta.
    pub const ROADSIDE: f32 = RoadNetwork::HALF_WIDTH + 5.25;
    /// Distancia mínima desde un alambrado hasta la línea central de la vía, en
    /// metros.
    pub const RAIL_MARGIN: f32 = 15.0;
    /// Distancia mínima desde un alambrado hasta la franja nivelada de un
    /// pueblo, en metros.
    pub const TOWN_MARGIN: f32 = 10.0;
    /// Distancia mínima y máxima entre dos aberturas de un tramo de la
    /// cuadrícula, en metros, y a qué distancia de su comienzo está la primera.
    pub const OPENING_SPACING: (f32, f32) = (300.0, 500.0);
    const FIRST_OPENING: (f32, f32) = (100.0, 350.0);
    /// Anchos de una tranquera simple y de una doble hacia un campo, en metros.
    pub const GATE_WIDTH: (f32, f32) = (4.5, 8.0);
    /// El frente más corto que tiene una tranquera, en metros.
    pub const MIN_FRONTAGE: f32 = 40.0;
    /// Distancia mínima desde una tranquera hasta una esquina, en metros.
    const GATE_CORNER: f32 = 12.0;
    /// Proporción de tranqueras de campo que están abiertas; el resto están
    /// cerradas.
    pub const OPEN_GATES: f32 = 0.6;
    /// Proporción de potreros que quedan sin alambrar (descampados).
    pub const DESCAMPADOS: f32 = 0.1;
    /// Distancia mínima desde una línea de alambrado hasta el borde del mundo,
    /// en metros.
    pub const EDGE_MARGIN: f32 = 10.0;
    /// Separación de la línea densa sobre la que se traza un alambrado al
    /// costado del camino, en metros.
    const TRACE: f32 = 2.0;

    /// Los alambrados del mundo de `config` sobre `terrain` (con sus pueblos
    /// nivelados y sus caminos trazados), fuera de los pueblos de `towns`, de
    /// la vía de `line` y de los caminos.
    pub fn build(
        config: WorldConfig,
        terrain: &Terrain,
        towns: &[TownLayout],
        line: &RailLine,
    ) -> Self {
        let bounds = config.bounds();
        let lo = bounds.min + Vec2::new(Self::EDGE_MARGIN, Self::EDGE_MARGIN);
        let hi = bounds.max - Vec2::new(Self::EDGE_MARGIN, Self::EDGE_MARGIN);
        let mut rng = Rng::new(terrain.seed ^ FENCE_SEED, 0);
        let mut survey = |from: f32, to: f32| {
            let mut at = from + rng.range(0.0, Self::FIELD.1);
            let mut lines = Vec::new();
            while at < to {
                lines.push(at);
                at += rng.range(Self::FIELD.0, Self::FIELD.1);
            }
            lines
        };
        let xs = survey(lo.x, hi.x);
        let ys = survey(lo.y, hi.y);
        let rails: Vec<(Vec2, Vec2)> = line
            .segments()
            .map(|s| (s.start, s.end))
            .chain(
                towns
                    .iter()
                    .filter_map(|t| t.railway)
                    .map(|r| (r.start, r.end)),
            )
            .collect();
        // Pueblos y lagunas, con sus barrancas.
        let bands: Vec<Aabb> = towns
            .iter()
            .map(|t| t.leveled().expanded(Self::TOWN_MARGIN))
            .chain(terrain.lagoons.iter().map(|l| l.keep_out()))
            .collect();
        let descampados = (0..xs.len().saturating_sub(1))
            .flat_map(|i| (0..ys.len().saturating_sub(1)).map(move |j| (i, j)))
            .filter(|&(i, j)| {
                Rng::new(terrain.seed ^ FENCE_SEED ^ 0xdeca, (i * 4099 + j) as u32).unit()
                    < Self::DESCAMPADOS
            })
            .collect();
        let mut layout = Self {
            surveyed: (xs.clone(), ys.clone()),
            descampados,
            ..Self::default()
        };
        let site = Site {
            terrain,
            rails: &rails,
            bands: &bands,
            lo,
            hi,
        };
        let mut sides = terrain
            .roads
            .as_deref()
            .map(|roads| roadsides(roads, &site))
            .unwrap_or_default();
        let lines = xs
            .iter()
            .map(|&x| (Vec2::new(x, lo.y), Vec2::new(x, hi.y)))
            .chain(ys.iter().map(|&y| (Vec2::new(lo.x, y), Vec2::new(hi.x, y))));
        for (start, end) in lines {
            layout.lay_line(start, end, &site, &mut sides);
        }
        for side in &mut sides {
            layout.cut_frontages(side);
        }
        layout.keep_fields_reachable(&site);
        layout
    }

    /// Cuántos potreros forma el relevamiento (entre líneas por todos los
    /// lados).
    pub fn cells(&self) -> usize {
        self.surveyed.0.len().saturating_sub(1) * self.surveyed.1.len().saturating_sub(1)
    }

    /// El potrero en el que está `p`, si está entre líneas del relevamiento por
    /// todos los lados.
    fn cell_of(&self, p: Vec2) -> Option<(usize, usize)> {
        let index = |lines: &[f32], v: f32| {
            let k = lines.partition_point(|&l| l <= v);
            (k > 0 && k < lines.len()).then(|| k - 1)
        };
        Some((index(&self.surveyed.0, p.x)?, index(&self.surveyed.1, p.y)?))
    }

    /// El potrero en el que está `p`, entre líneas del relevamiento por todos
    /// los lados, como rectángulo.
    pub fn field_of(&self, p: Vec2) -> Option<Aabb> {
        let (i, j) = self.cell_of(p)?;
        let (xs, ys) = &self.surveyed;
        Some(Aabb::new(
            Vec2::new(xs[i], ys[j]),
            Vec2::new(xs[i + 1], ys[j + 1]),
        ))
    }

    /// Si `p` está en un potrero sin alambrar.
    pub fn is_descampado(&self, p: Vec2) -> bool {
        self.cell_of(p)
            .is_some_and(|c| self.descampados.binary_search(&c).is_ok())
    }

    /// Si el tramo de la cuadrícula de `a` a `b` está entre dos potreros sin
    /// alambrar, de modo que ninguno de los dos tiene alambrado ahí.
    fn between_descampados(&self, a: Vec2, b: Vec2) -> bool {
        let mid = (a + b) * 0.5;
        let across = Vec2::new(-(b - a).y, (b - a).x);
        let across = across * (1.0 / across.length().max(1e-6));
        self.is_descampado(mid + across) && self.is_descampado(mid - across)
    }

    /// Coloca los postes de la línea de la cuadrícula de `start` a `end`,
    /// deteniéndola en los alambrados al costado del camino que encuentre.
    fn lay_line(&mut self, start: Vec2, end: Vec2, site: &Site, sides: &mut [Roadside]) {
        let length = start.distance(end);
        let along = (end - start) * (1.0 / length);
        let spans = (length / Self::POST_SPACING).floor() as usize;
        let post = |k: usize| start + along * (k as f32 * Self::POST_SPACING);
        let whole = Aabb::new(
            Vec2::new(start.x.min(end.x), start.y.min(end.y)),
            Vec2::new(start.x.max(end.x), start.y.max(end.y)),
        )
        .expanded(Self::RAIL_MARGIN);
        let near_rails: Vec<(Vec2, Vec2)> = site
            .rails
            .iter()
            .copied()
            .filter(|&(a, b)| whole.segment_overlap(a, b).is_some())
            .collect();
        // Dentro de los alambrados al costado del camino, más un poco de lugar
        // para la esquina.
        let road_reach = Self::ROADSIDE + 0.5;
        let mut kind: Vec<Span> = (0..spans)
            .map(|k| {
                let (a, b) = (post(k), post(k + 1));
                let blocked = site
                    .bands
                    .iter()
                    .any(|band| band.segment_overlap(a, b).is_some())
                    || near_rails
                        .iter()
                        .any(|&(r0, r1)| segments_distance(a, b, r0, r1) < Self::RAIL_MARGIN);
                let on_road = site.terrain.roads.as_ref().is_some_and(|roads| {
                    [a, (a + b) * 0.5, b]
                        .iter()
                        .any(|&p| roads.near(p, road_reach).is_some())
                });
                if blocked || self.between_descampados(a, b) {
                    Span::Blocked
                } else if on_road {
                    Span::Road
                } else {
                    Span::Fenced
                }
            })
            .collect();
        self.cut_openings(&mut kind, start, along);
        let mut k = 0;
        while k < spans {
            let first = k;
            while k < spans && kind[k] == kind[first] {
                k += 1;
            }
            if kind[first] != Span::Fenced {
                continue;
            }
            let mut posts: Vec<Vec2> = (first..=k).map(post).collect();
            // Donde la línea seguiría hasta un camino, termina en el alambrado
            // al costado del camino, en un poste esquinero.
            if first > 0 && kind[first - 1] == Span::Road {
                let from = post(first);
                if let Some(corner) =
                    join(sides, from, from - along * (3.0 * Self::POST_SPACING), site)
                {
                    posts.insert(0, corner);
                }
            }
            if k < spans && kind[k] == Span::Road {
                let from = post(k);
                if let Some(corner) =
                    join(sides, from, from + along * (3.0 * Self::POST_SPACING), site)
                {
                    posts.push(corner);
                }
            }
            self.runs.push(FenceRun {
                posts: densify(&posts),
                roadside: false,
            });
        }
    }

    /// Divide un alambrado al costado del camino en frentes de campo en sus
    /// esquinas, con una o dos tranqueras abiertas en cada uno, y coloca el
    /// alambrado entre ellas.
    fn cut_frontages(&mut self, side: &mut Roadside) {
        let total = side.length();
        side.corners.sort_by(f32::total_cmp);
        let mut bounds = vec![0.0];
        bounds.extend(
            side.corners
                .iter()
                .copied()
                .filter(|&c| c > 1.0 && c < total - 1.0),
        );
        bounds.push(total);
        let mut fenced: Vec<(f32, f32)> = Vec::new();
        for w in bounds.windows(2) {
            let (from, to) = (w[0], w[1]);
            if to - from < 0.5 {
                continue;
            }
            let id = self.frontages.len() as u32;
            // A lo largo de un descampado el camino no tiene alambrado.
            let has_fence = !self.is_descampado(side.at((from + to) / 2.0).0);
            self.frontages.push(Frontage {
                start: side.at(from).0,
                end: side.at(to).0,
                length: to - from,
                fenced: has_fence,
            });
            if !has_fence {
                continue;
            }
            let mut rng = Rng::new(
                FENCE_SEED ^ (side.at(from).0.x as i32 as u32).wrapping_mul(0x9e37_79b9),
                (side.at(from).0.y as i32 as u32) ^ id,
            );
            let mut cuts: Vec<(f32, f32, bool, bool)> = Vec::new();
            if to - from >= Self::MIN_FRONTAGE {
                let two = to - from > 300.0 && rng.unit() < 0.5;
                let count = if two { 2 } else { 1 };
                for g in 0..count {
                    let double = rng.unit() < 0.4;
                    let width = if double {
                        Self::GATE_WIDTH.1
                    } else {
                        Self::GATE_WIDTH.0
                    };
                    // Cada tranquera en su propia parte del frente.
                    let share = (to - from) / count as f32;
                    let lo = from + share * g as f32 + Self::GATE_CORNER;
                    let hi = from + share * (g + 1) as f32 - Self::GATE_CORNER - width;
                    if hi <= lo {
                        continue;
                    }
                    let start = rng.range(lo, hi);
                    let open = rng.unit() < Self::OPEN_GATES;
                    cuts.push((start, start + width, double, open));
                }
                // Cada campo conserva al menos una entrada desde el camino.
                if !cuts.iter().any(|c| c.3)
                    && let Some(first) = cuts.first_mut()
                {
                    first.3 = true;
                }
            }
            let mut at = from;
            for (g0, g1, double, open) in cuts {
                fenced.push((at, g0));
                let (center, along) = side.at((g0 + g1) / 2.0);
                self.gates.push(Gate {
                    at: center,
                    along,
                    width: g1 - g0,
                    double,
                    open,
                    frontage: id,
                });
                at = g1;
            }
            fenced.push((at, to));
        }
        // Las esquinas unen en un solo tramo los tramos que tienen a cada lado.
        let mut run: Vec<(f32, f32)> = Vec::new();
        for (from, to) in fenced {
            if let Some(last) = run.last()
                && (last.1 - from).abs() > 1e-3
            {
                self.push_roadside(side, &run);
                run.clear();
            }
            run.push((from, to));
        }
        self.push_roadside(side, &run);
    }

    fn push_roadside(&mut self, side: &Roadside, stretches: &[(f32, f32)]) {
        let mut posts: Vec<Vec2> = Vec::new();
        for &(from, to) in stretches {
            for p in side.posts(from, to) {
                // Un poste justo al lado del último es el mismo poste.
                if posts.last().is_none_or(|l: &Vec2| l.distance(p) > 0.5) {
                    posts.push(p);
                }
            }
        }
        if posts.len() > 1 {
            self.runs.push(FenceRun {
                posts,
                roadside: true,
            });
        }
    }

    /// Abre aberturas con semilla en cada tramo alambrado de una línea de la
    /// cuadrícula cuyos tramos son de tipo `kind`, con postes desde `start` a
    /// lo largo de `along`.
    fn cut_openings(&mut self, kind: &mut [Span], start: Vec2, along: Vec2) {
        let post = |k: usize| start + along * (k as f32 * Self::POST_SPACING);
        let spacing = Self::POST_SPACING;
        let mut k = 0;
        while k < kind.len() {
            if kind[k] != Span::Fenced {
                k += 1;
                continue;
            }
            let first = k;
            while k < kind.len() && kind[k] == Span::Fenced {
                k += 1;
            }
            let id = self.stretches.len() as u32;
            let length = (k - first) as f32 * spacing;
            self.stretches.push((id, length));
            let corner = post(first);
            let seed = (corner.x as i32 as u32).wrapping_mul(0x9e37_79b9)
                ^ (corner.y as i32 as u32).rotate_left(16)
                ^ FENCE_SEED;
            let mut rng = Rng::new(seed, id);
            let mut at = rng.range(Self::FIRST_OPENING.0, Self::FIRST_OPENING.1);
            loop {
                let wide = if rng.unit() < 0.5 { 1 } else { 2 };
                let span = first + (at / spacing) as usize;
                if span + wide > k {
                    break;
                }
                // No donde una línea de alambrado que cruza se une con esta.
                let center = (post(span) + post(span + wide)) * 0.5;
                if self.near_crossing_line(center, along, wide as f32 * spacing / 2.0 + spacing) {
                    at += 2.0 * spacing;
                    continue;
                }
                let opening = if rng.unit() < 0.5 {
                    OpeningKind::FieldGate {
                        double: rng.unit() < 0.5,
                        open: rng.unit() < Self::OPEN_GATES,
                    }
                } else {
                    OpeningKind::Broken
                };
                for s in &mut kind[span..span + wide] {
                    *s = Span::Opening;
                }
                self.openings.push(Opening {
                    at: center,
                    along,
                    width: wide as f32 * spacing,
                    kind: opening,
                    stretch: id,
                });
                at += rng.range(Self::OPENING_SPACING.0, Self::OPENING_SPACING.1);
            }
        }
    }

    /// Si una línea relevada perpendicular a `along` pasa a menos de `reach` de
    /// `p`.
    fn near_crossing_line(&self, p: Vec2, along: Vec2, reach: f32) -> bool {
        let (lines, v) = if along.x.abs() > along.y.abs() {
            (&self.surveyed.0, p.x)
        } else {
            (&self.surveyed.1, p.y)
        };
        lines.iter().any(|l| (l - v).abs() < reach)
    }

    /// Rectángulos delgados a lo largo de cada tramo de cada recorrido y a
    /// través de cada tranquera cerrada: los obstáculos estáticos de los
    /// alambrados.
    pub fn obstacles(&self) -> Vec<Aabb> {
        let line = |at: Vec2, along: Vec2, width: f32| {
            (at - along * (width / 2.0), at + along * (width / 2.0))
        };
        let closed_road = self
            .gates
            .iter()
            .filter(|g| !g.open)
            .map(|g| line(g.at, g.along, g.width));
        let closed_grid = self.openings.iter().filter_map(|o| match o.kind {
            OpeningKind::FieldGate { open: false, .. } => Some(line(o.at, o.along, o.width)),
            _ => None,
        });
        self.runs
            .iter()
            .flat_map(FenceRun::spans)
            .chain(closed_road)
            .chain(closed_grid)
            .flat_map(span_boxes)
            .collect()
    }

    /// Se asegura de que cada potrero tenga una entrada: una tranquera abierta,
    /// un tramo roto o un lado sin alambrar. Donde un potrero no tiene ninguna,
    /// se abre una tranquera cerrada de su borde o, si no, se deja roto un
    /// tramo de su lado más largo.
    fn keep_fields_reachable(&mut self, site: &Site) {
        let (xs, ys) = self.surveyed.clone();
        for i in 0..xs.len().saturating_sub(1) {
            for j in 0..ys.len().saturating_sub(1) {
                let cell = Aabb::new(Vec2::new(xs[i], ys[j]), Vec2::new(xs[i + 1], ys[j + 1]));
                if self.reachable(cell, site) {
                    continue;
                }
                if let Some(o) = self.openings.iter_mut().find(|o| {
                    matches!(o.kind, OpeningKind::FieldGate { open: false, .. })
                        && on_edge(cell, o.at)
                }) {
                    if let OpeningKind::FieldGate { open, .. } = &mut o.kind {
                        *open = true;
                    }
                    continue;
                }
                self.break_edge(cell, site);
            }
        }
    }

    /// Si `cell` tiene una entrada.
    fn reachable(&self, cell: Aabb, site: &Site) -> bool {
        let passable = self.openings.iter().any(|o| {
            !matches!(o.kind, OpeningKind::FieldGate { open: false, .. }) && on_edge(cell, o.at)
        });
        let gate = self
            .gates
            .iter()
            .any(|g| g.open && cell.contains_point(g.at));
        if passable || gate {
            return true;
        }
        // Un tramo sin alambrar de un borde, lejos de los caminos.
        let near_road = |p: Vec2| {
            site.terrain
                .roads
                .as_ref()
                .is_some_and(|r| r.near(p, Self::ROADSIDE + 1.0).is_some())
        };
        let edges = [
            (cell.min, Vec2::new(cell.max.x, cell.min.y)),
            (Vec2::new(cell.max.x, cell.min.y), cell.max),
            (cell.max, Vec2::new(cell.min.x, cell.max.y)),
            (Vec2::new(cell.min.x, cell.max.y), cell.min),
        ];
        edges.iter().any(|&(a, b)| {
            let length = a.distance(b);
            let mut free = 0;
            let mut k = 2.0;
            while k < length - 2.0 {
                let p = a + (b - a) * (k / length);
                let fenced = self.fenced_at(p);
                if !fenced && !near_road(p) {
                    free += 1;
                    if free >= 4 {
                        return true;
                    }
                } else {
                    free = 0;
                }
                k += 1.0;
            }
            false
        })
    }

    /// Si un recorrido de la cuadrícula pasa por `p` (sobre una línea del
    /// relevamiento), o si una tranquera de campo cerrada cierra el hueco ahí.
    fn fenced_at(&self, p: Vec2) -> bool {
        let run = self.runs.iter().filter(|r| !r.roadside).any(|r| {
            let (a, b) = (r.start(), r.end());
            let lo = Vec2::new(a.x.min(b.x) - 0.5, a.y.min(b.y) - 0.5);
            let hi = Vec2::new(a.x.max(b.x) + 0.5, a.y.max(b.y) + 0.5);
            p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y
        });
        run || self.openings.iter().any(|o| {
            let rel = p - o.at;
            matches!(o.kind, OpeningKind::FieldGate { open: false, .. })
                && rel.dot(o.along).abs() <= o.width / 2.0 + 0.5
                && (rel.x * o.along.y - rel.y * o.along.x).abs() <= 0.5
        })
    }

    /// Deja roto un tramo en el borde más largo de `cell` que tenga un tramo
    /// entero bien lejos de sus esquinas y de los caminos.
    fn break_edge(&mut self, cell: Aabb, site: &Site) {
        let middle = cell.center();
        let off_road = |p: Vec2| {
            site.terrain
                .roads
                .as_ref()
                .is_none_or(|r| r.near(p, Self::ROADSIDE + 10.0).is_none())
        };
        let mut best: Option<(f32, usize, usize)> = None;
        for (r, run) in self.runs.iter().enumerate().filter(|(_, r)| !r.roadside) {
            for (k, (a, b)) in run.spans().enumerate() {
                let mid = (a + b) * 0.5;
                let whole = (a.distance(b) - Self::POST_SPACING).abs() < 1e-2;
                let corners = [
                    cell.min,
                    cell.max,
                    Vec2::new(cell.min.x, cell.max.y),
                    Vec2::new(cell.max.x, cell.min.y),
                ];
                let off_corners = corners.iter().all(|c| c.distance(mid) >= BREAK_CORNER);
                if whole && on_edge(cell, a) && on_edge(cell, b) && off_road(mid) && off_corners {
                    let d = mid.distance(middle);
                    if best.is_none_or(|(bd, _, _)| d < bd) {
                        best = Some((d, r, k));
                    }
                }
            }
        }
        let Some((_, r, k)) = best else {
            return;
        };
        let run = self.runs[r].clone();
        let (a, b) = (run.posts[k], run.posts[k + 1]);
        self.runs[r].posts.truncate(k + 1);
        let rest: Vec<Vec2> = run.posts[k + 1..].to_vec();
        if rest.len() > 1 {
            self.runs.push(FenceRun {
                posts: rest,
                roadside: false,
            });
        }
        if self.runs[r].posts.len() < 2 {
            self.runs.swap_remove(r);
        }
        self.openings.push(Opening {
            at: (a + b) * 0.5,
            along: (b - a) * (1.0 / a.distance(b)),
            width: a.distance(b),
            kind: OpeningKind::Broken,
            stretch: u32::MAX,
        });
    }

    /// Las partes de cada recorrido dentro de `area` (un chunk del terreno):
    /// tramos enteros cuyo centro está en ella, para que los chunks vecinos no
    /// compartan ninguno.
    pub fn runs_in(&self, area: Aabb) -> Vec<FenceRun> {
        let mut parts = Vec::new();
        let reach = area.expanded(Self::POST_SPACING);
        for run in &self.runs {
            if !run.posts.iter().any(|p| reach.contains_point(*p))
                && reach.segment_overlap(run.start(), run.end()).is_none()
            {
                continue;
            }
            let mut current: Vec<Vec2> = Vec::new();
            for (a, b) in run.spans() {
                if area_holds(area, (a + b) * 0.5) {
                    if current.is_empty() {
                        current.push(a);
                    }
                    current.push(b);
                } else if !current.is_empty() {
                    parts.push(FenceRun {
                        posts: std::mem::take(&mut current),
                        roadside: run.roadside,
                    });
                }
            }
            if !current.is_empty() {
                parts.push(FenceRun {
                    posts: current,
                    roadside: run.roadside,
                });
            }
        }
        parts
    }

    /// Distancia desde `p` hasta la línea relevada de la cuadrícula más
    /// cercana, esté alambrada ahí o no, en metros.
    pub fn line_distance(&self, p: Vec2) -> f32 {
        let nearest = |lines: &[f32], v: f32| {
            let i = lines.partition_point(|&l| l < v);
            [i.checked_sub(1), Some(i)]
                .into_iter()
                .flatten()
                .filter_map(|i| lines.get(i))
                .map(|l| (l - v).abs())
                .fold(f32::INFINITY, f32::min)
        };
        nearest(&self.surveyed.0, p.x).min(nearest(&self.surveyed.1, p.y))
    }
}

impl FenceRun {
    /// Los tramos entre postes consecutivos, en orden.
    pub fn spans(&self) -> impl Iterator<Item = (Vec2, Vec2)> + '_ {
        self.posts.windows(2).map(|w| (w[0], w[1]))
    }

    /// El primer poste.
    pub fn start(&self) -> Vec2 {
        self.posts[0]
    }

    /// El último poste.
    pub fn end(&self) -> Vec2 {
        self.posts[self.posts.len() - 1]
    }
}

/// Aquello de lo que se mantienen fuera los alambrados.
struct Site<'a> {
    terrain: &'a Terrain,
    rails: &'a [(Vec2, Vec2)],
    bands: &'a [Aabb],
    lo: Vec2,
    hi: Vec2,
}

/// Los alambrados a ambos lados de cada camino, trazados a
/// [`FenceLayout::ROADSIDE`] de la línea central, e interrumpidos donde
/// quedarían sobre la franja de un pueblo, junto a la vía férrea (por lo tanto,
/// en cada paso a nivel), fuera del mundo o cerca de otro camino.
fn roadsides(roads: &RoadNetwork, site: &Site) -> Vec<Roadside> {
    let mut sides = Vec::new();
    let near_rail = |p: Vec2| {
        site.rails
            .iter()
            .any(|&(a, b)| crate::road::segment_distance(p, a, b) < FenceLayout::RAIL_MARGIN)
    };
    for road in &roads.roads {
        let traced = trace(&road.path, FenceLayout::TRACE);
        for side in [-1.0_f32, 1.0] {
            let mut line: Vec<Vec2> = Vec::new();
            let mut flush = |line: &mut Vec<Vec2>| {
                if line.len() > 2 {
                    sides.push(Roadside::new(std::mem::take(line)));
                }
                line.clear();
            };
            for &(p, normal) in &traced {
                let q = p + normal * (side * FenceLayout::ROADSIDE);
                let ok = q.x >= site.lo.x
                    && q.x <= site.hi.x
                    && q.y >= site.lo.y
                    && q.y <= site.hi.y
                    && !site.bands.iter().any(|b| b.contains_point(q))
                    && !near_rail(q)
                    && roads.near(q, FenceLayout::ROADSIDE - 0.05).is_none();
                if ok {
                    line.push(q);
                } else {
                    flush(&mut line);
                }
            }
            flush(&mut line);
        }
    }
    sides
}

impl Roadside {
    fn new(line: Vec<Vec2>) -> Self {
        let mut arcs = Vec::with_capacity(line.len());
        let mut total = 0.0;
        for (i, p) in line.iter().enumerate() {
            if i > 0 {
                total += line[i - 1].distance(*p);
            }
            arcs.push(total);
        }
        Self {
            line,
            arcs,
            corners: Vec::new(),
        }
    }
}

/// Puntos a lo largo de `path` separados unos `step` metros, con la normal
/// unitaria izquierda del recorrido en cada uno (con inglete en las curvas).
fn trace(path: &[Vec2], step: f32) -> Vec<(Vec2, Vec2)> {
    let mut out = Vec::new();
    let unit = |v: Vec2| v * (1.0 / v.length().max(1e-9));
    for (i, w) in path.windows(2).enumerate() {
        let d = unit(w[1] - w[0]);
        let left = Vec2::new(-d.y, d.x);
        let length = w[0].distance(w[1]);
        let n = (length / step).ceil().max(1.0) as usize;
        for k in 0..n {
            let normal = if k == 0 && i > 0 {
                let before = unit(path[i] - path[i - 1]);
                unit(Vec2::new(-before.y, before.x) + left)
            } else {
                left
            };
            out.push((w[0] + d * (length * k as f32 / n as f32), normal));
        }
    }
    if let (Some(&last), Some(&(_, normal))) = (path.last(), out.last()) {
        out.push((last, normal));
    }
    out
}

/// Dónde la línea de la cuadrícula que va de `from` hacia `to` se encuentra por
/// primera vez con un alambrado al costado del camino: ese punto, registrado
/// como una esquina del alambrado que encuentra.
fn join(sides: &mut [Roadside], from: Vec2, to: Vec2, site: &Site) -> Option<Vec2> {
    let mut best: Option<(f32, usize, f32)> = None;
    for (n, side) in sides.iter().enumerate() {
        for (i, w) in side.line.windows(2).enumerate() {
            if let Some((t, u)) = crate::road::intersect(from, to, w[0], w[1])
                && best.is_none_or(|(bt, _, _)| t < bt)
            {
                let arc = side.arcs[i] + (side.arcs[i + 1] - side.arcs[i]) * u;
                best = Some((t, n, arc));
            }
        }
    }
    let (t, n, arc) = best?;
    let corner = from + (to - from) * t;
    // Solo el alambrado del lado cercano: nunca cruzando el camino hasta el del
    // otro lado.
    let clear = site.terrain.roads.as_ref().is_none_or(|roads| {
        (0..=10).all(|k| {
            let p = from + (corner - from) * (k as f32 / 10.0);
            roads.near(p, FenceLayout::ROADSIDE - 0.05).is_none()
        })
    });
    if !clear {
        return None;
    }
    sides[n].corners.push(arc);
    Some(corner)
}

/// `posts` con postes agregados donde dos estén más separados que
/// [`FenceLayout::POST_SPACING`].
fn densify(posts: &[Vec2]) -> Vec<Vec2> {
    let mut out = vec![posts[0]];
    for w in posts.windows(2) {
        let n = (w[0].distance(w[1]) / FenceLayout::POST_SPACING - 1e-3)
            .ceil()
            .max(1.0) as usize;
        for k in 1..=n {
            out.push(w[0] + (w[1] - w[0]) * (k as f32 / n as f32));
        }
    }
    out
}

/// Si `p` está en el borde de `cell` (con una tolerancia de unos pocos
/// centímetros).
fn on_edge(cell: Aabb, p: Vec2) -> bool {
    let near = |a: f32, b: f32| (a - b).abs() < 0.05;
    let within = |v: f32, lo: f32, hi: f32| v >= lo - 0.05 && v <= hi + 0.05;
    ((near(p.x, cell.min.x) || near(p.x, cell.max.x)) && within(p.y, cell.min.y, cell.max.y))
        || ((near(p.y, cell.min.y) || near(p.y, cell.max.y)) && within(p.x, cell.min.x, cell.max.x))
}

/// Si `p` está dentro de `area`, con sus bordes oeste y sur incluidos y sus
/// bordes este y norte excluidos, para que un punto sobre un borde compartido
/// pertenezca a un solo chunk.
fn area_holds(area: Aabb, p: Vec2) -> bool {
    p.x >= area.min.x && p.x < area.max.x && p.y >= area.min.y && p.y < area.max.y
}

/// Rectángulos delgados a lo largo de un tramo: uno para un tramo alineado con
/// los ejes, y varios cortos a lo largo de un tramo inclinado para que ninguno
/// sobresalga más de unos pocos centímetros.
fn span_boxes((a, b): (Vec2, Vec2)) -> Vec<Aabb> {
    let t = FenceLayout::THICKNESS / 2.0;
    let d = b - a;
    let slant = d.x.abs().min(d.y.abs());
    let pieces = (slant / SLANT_BULGE).ceil().max(1.0) as usize;
    (0..pieces)
        .map(|k| {
            let (p, q) = (
                a + d * (k as f32 / pieces as f32),
                a + d * ((k + 1) as f32 / pieces as f32),
            );
            Aabb::new(
                Vec2::new(p.x.min(q.x) - t, p.y.min(q.y) - t),
                Vec2::new(p.x.max(q.x) + t, p.y.max(q.y) + t),
            )
        })
        .collect()
}

/// Distancia mínima desde un tramo roto para abrir un potrero cerrado hasta las
/// esquinas del potrero, en metros.
const BREAK_CORNER: f32 = 30.0;

/// Lo máximo que un rectángulo de un tramo inclinado se extiende a lo ancho de
/// él, en metros.
const SLANT_BULGE: f32 = 0.3;

/// Distancia mínima entre los segmentos `a`–`b` y `c`–`d`.
fn segments_distance(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> f32 {
    if crate::road::intersect(a, b, c, d).is_some() {
        return 0.0;
    }
    use crate::road::segment_distance as sd;
    sd(a, c, d)
        .min(sd(b, c, d))
        .min(sd(c, a, b))
        .min(sd(d, a, b))
}

/// Se mezcla con la semilla del terreno para el relevamiento.
const FENCE_SEED: u32 = 0xa1a3_b4ad;

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{Circle, FloraSite, Heading, Rider, RiderInput, RiderParams, World};

    fn config() -> WorldConfig {
        WorldConfig {
            half_extent: WorldConfig::MAX_HALF_EXTENT,
            ..WorldConfig::default()
        }
    }

    /// Los pueblos distribuidos con los caminos trazados, sus alambrados y el
    /// mundo. Se construyen una sola vez y se comparten: las pruebas que
    /// cambian el mundo primero lo clonan.
    pub(crate) fn fenced() -> &'static (World, Vec<TownLayout>, RailLine, FenceLayout) {
        static FENCED: std::sync::LazyLock<(World, Vec<TownLayout>, RailLine, FenceLayout)> =
            std::sync::LazyLock::new(|| {
                let (towns, line, roads) = crate::road::tests::shipped_roads();
                let mut world = World::with_towns(config(), Terrain::default(), &towns);
                world.lay_roads(roads);
                let fences = FenceLayout::build(config(), &world.terrain, &towns, &line);
                let count = world.fence(&fences);
                println!(
                    "fences: {} runs, {} gates, {count} obstacles",
                    fences.runs.len(),
                    fences.gates.len(),
                );
                (world, towns, line, fences)
            });
        &FENCED
    }

    /// Un potrero de 400 m alambrado por todos lados, con su lado sur
    /// interrumpido alrededor de `gaps` (como `(x, width)`), cada uno una
    /// tranquera de campo cerrada; después, la pasada de accesibilidad.
    fn sealed_paddock(gaps: &[(f32, f32)]) -> FenceLayout {
        let side = 400.0;
        let posts = |a: Vec2, b: Vec2| -> Vec<Vec2> {
            let n = (a.distance(b) / FenceLayout::POST_SPACING).round() as usize;
            (0..=n)
                .map(|k| a + (b - a) * (k as f32 / n as f32))
                .collect()
        };
        let run = |posts: Vec<Vec2>| FenceRun {
            posts,
            roadside: false,
        };
        let mut layout = FenceLayout {
            surveyed: (vec![0.0, side], vec![0.0, side]),
            ..FenceLayout::default()
        };
        let (c0, c1, c2, c3) = (
            Vec2::new(0.0, 0.0),
            Vec2::new(side, 0.0),
            Vec2::new(side, side),
            Vec2::new(0.0, side),
        );
        layout.runs.push(run(posts(c1, c2)));
        layout.runs.push(run(posts(c2, c3)));
        layout.runs.push(run(posts(c3, c0)));
        // El lado sur, en partes entre las tranqueras.
        let mut from = 0.0;
        for &(x, width) in gaps {
            layout.runs.push(run(posts(
                Vec2::new(from, 0.0),
                Vec2::new(x - width / 2.0, 0.0),
            )));
            layout.openings.push(Opening {
                at: Vec2::new(x, 0.0),
                along: Vec2::new(1.0, 0.0),
                width,
                kind: OpeningKind::FieldGate {
                    double: width > 6.0,
                    open: false,
                },
                stretch: 0,
            });
            from = x + width / 2.0;
        }
        layout.runs.push(run(posts(Vec2::new(from, 0.0), c1)));
        let terrain = Terrain::default();
        let site = Site {
            terrain: &terrain,
            rails: &[],
            bands: &[],
            lo: Vec2::new(-10.0, -10.0),
            hi: Vec2::new(side + 10.0, side + 10.0),
        };
        layout.keep_fields_reachable(&site);
        layout
    }

    /// Las aberturas transitables en el borde del potrero.
    fn ways_in(layout: &FenceLayout) -> usize {
        let cell = Aabb::new(Vec2::new(0.0, 0.0), Vec2::new(400.0, 400.0));
        layout
            .openings
            .iter()
            .filter(|o| !matches!(o.kind, OpeningKind::FieldGate { open: false, .. }))
            .filter(|o| on_edge(cell, o.at))
            .count()
    }

    #[test]
    fn a_fence_blocks_up_to_its_wires_only() {
        let (world, _, _, fences) = fenced();
        let spans = fences.obstacles();
        assert!(!spans.is_empty());
        for span in spans.iter().take(500) {
            let found = |clearance| world.obstacles.near_above(*span, clearance).contains(&span);
            assert!(found(FenceLayout::HEIGHT - 0.01), "{span:?}");
            assert!(!found(FenceLayout::HEIGHT), "{span:?}");
        }
    }

    #[test]
    fn field_of_finds_the_paddock_between_its_survey_lines() {
        let layout = FenceLayout {
            surveyed: (vec![0.0, 300.0, 800.0], vec![-400.0, 0.0, 500.0]),
            ..FenceLayout::default()
        };
        let cell = |x0, y0, x1, y1| Some(Aabb::new(Vec2::new(x0, y0), Vec2::new(x1, y1)));
        assert_eq!(
            layout.field_of(Vec2::new(10.0, -10.0)),
            cell(0.0, -400.0, 300.0, 0.0)
        );
        // Sobre una línea del relevamiento, el potrero al este (al norte) de
        // ella: sus bordes oeste y sur están incluidos.
        assert_eq!(
            layout.field_of(Vec2::new(300.0, 0.0)),
            cell(300.0, 0.0, 800.0, 500.0)
        );
        assert_eq!(
            layout.field_of(Vec2::new(0.0, -400.0)),
            cell(0.0, -400.0, 300.0, 0.0)
        );
        // La celda más externa, hasta justo antes de sus líneas lejanas.
        assert_eq!(
            layout.field_of(Vec2::new(799.9, 499.9)),
            cell(300.0, 0.0, 800.0, 500.0)
        );
        // Fuera de la cuadrícula, y en sus bordes lejanos, no hay potrero.
        for p in [
            Vec2::new(-0.1, 10.0),
            Vec2::new(10.0, 500.0),
            Vec2::new(800.0, 10.0),
            Vec2::new(5000.0, 5000.0),
        ] {
            assert_eq!(layout.field_of(p), None, "{p:?}");
        }
        assert_eq!(FenceLayout::default().field_of(Vec2::new(0.0, 0.0)), None);
    }

    #[test]
    fn a_paddock_ringed_only_by_closed_gates_gets_a_way_in() {
        let layout = sealed_paddock(&[(120.0, 8.0), (300.0, 4.5)]);
        assert!(ways_in(&layout) >= 1, "{:?}", layout.openings);
    }

    #[test]
    fn a_paddock_with_no_opening_at_all_gets_a_broken_span() {
        let layout = sealed_paddock(&[]);
        assert_eq!(ways_in(&layout), 1, "{:?}", layout.openings);
        let broken = layout.openings[0];
        assert_eq!(broken.kind, OpeningKind::Broken);
        // Un tramo entero, bien lejos de las esquinas, que ya no está
        // alambrado.
        assert!((broken.width - FenceLayout::POST_SPACING).abs() < 1e-3);
        let corners = [0.0, 400.0];
        assert!(
            corners
                .iter()
                .flat_map(|&x| corners.iter().map(move |&y| Vec2::new(x, y)))
                .all(|c| c.distance(broken.at) >= 30.0)
        );
        let spans: Vec<(Vec2, Vec2)> = layout.runs.iter().flat_map(FenceRun::spans).collect();
        assert!(
            spans.iter().all(|&(a, b)| (a + b) * 0.5 != broken.at),
            "the broken span is still strung"
        );
        assert_eq!(spans.len(), 4 * 40 - 1);
    }

    #[test]
    fn the_land_is_surveyed_into_large_fields_the_same_way_every_time() {
        let (world, towns, line, fences) = fenced();
        assert_eq!(
            *fences,
            FenceLayout::build(config(), &world.terrain, towns, line)
        );
        for lines in [&fences.surveyed.0, &fences.surveyed.1] {
            assert!(lines.len() > 15, "{}", lines.len());
            for w in lines.windows(2) {
                let field = w[1] - w[0];
                assert!((300.0..=800.0).contains(&field), "{field}");
            }
        }
        for run in &fences.runs {
            let (start, end) = (run.start(), run.end());
            assert!(
                run.roadside || start.x == end.x || start.y == end.y,
                "{run:?}"
            );
            for (a, b) in run.spans() {
                assert!(a.distance(b) <= FenceLayout::POST_SPACING + 1e-2);
                assert!(a.distance(b) > 0.1);
            }
        }
        let other = Terrain {
            seed: 11,
            ..world.terrain.clone()
        };
        assert_ne!(
            FenceLayout::build(config(), &other, towns, line).surveyed,
            fences.surveyed
        );
    }

    #[test]
    fn fences_keep_out_of_towns_the_rail_corridor_and_the_roads() {
        let (world, towns, line, fences) = fenced();
        let roads = world.terrain.roads.clone().unwrap();
        let rails: Vec<(Vec2, Vec2)> = line
            .segments()
            .map(|s| (s.start, s.end))
            .chain(
                towns
                    .iter()
                    .filter_map(|t| t.railway)
                    .map(|r| (r.start, r.end)),
            )
            .collect();
        let mut spans = 0;
        for run in &fences.runs {
            for (a, b) in run.spans() {
                for p in [a, (a + b) * 0.5, b] {
                    for town in towns {
                        assert!(!town.leveled().contains_point(p), "{p:?} in {}", town.name);
                    }
                    let rail = rails
                        .iter()
                        .map(|&(r0, r1)| crate::road::segment_distance(p, r0, r1))
                        .fold(f32::INFINITY, f32::min);
                    assert!(rail >= FenceLayout::RAIL_MARGIN, "{p:?}: {rail}");
                    assert!(
                        roads.distance(p) > crate::RoadNetwork::HALF_WIDTH + 1.0,
                        "{p:?}"
                    );
                }
                spans += 1;
            }
        }
        assert!(spans > 20_000, "{spans}");
    }

    /// Puntos a lo largo de cada camino cada `step` metros, con la normal
    /// unitaria izquierda del camino en cada uno.
    fn along_roads(roads: &crate::RoadNetwork, step: f32) -> Vec<(usize, Vec2, Vec2)> {
        let mut out = Vec::new();
        for (i, road) in roads.roads.iter().enumerate() {
            let mut next = 0.0;
            let mut along = 0.0;
            for w in road.path.windows(2) {
                let length = w[0].distance(w[1]);
                let d = (w[1] - w[0]) * (1.0 / length);
                while next <= along + length {
                    let p = w[0] + d * (next - along);
                    out.push((i, p, Vec2::new(-d.y, d.x)));
                    next += step;
                }
                along += length;
            }
        }
        out
    }

    #[test]
    fn no_fence_crosses_a_road_and_a_rider_rides_every_road_end_to_end() {
        let (world, _, _, fences) = fenced();
        let roads = world.terrain.roads.clone().unwrap();
        for run in &fences.runs {
            for (a, b) in run.spans() {
                for k in 0..=4 {
                    let p = a + (b - a) * (k as f32 / 4.0);
                    // Una calle pública: al menos 5 m desde el borde del
                    // camino.
                    let gap = roads.distance(p) - crate::RoadNetwork::HALF_WIDTH;
                    assert!(gap >= 5.0 - 1e-3, "{p:?}: {gap}");
                }
                for (r0, r1) in roads.segments() {
                    assert!(
                        crate::road::intersect(a, b, r0, r1).is_none(),
                        "{a:?}–{b:?}"
                    );
                }
            }
        }
        let c = Circle::new(Rider::COLLIDER_RADIUS);
        for (_, p, _) in along_roads(&roads, 1.0) {
            assert_eq!(world.resolve(c, p, |_| ()), p, "blocked at {p:?}");
        }
        // Los carteles indicadores y las señales de los pasos a nivel quedan
        // lejos de todo alambrado.
        for post in roads.obstacles() {
            let c = post.center();
            for run in &fences.runs {
                for (a, b) in run.spans() {
                    assert!(crate::road::segment_distance(c, a, b) > 1.0, "{c:?}");
                }
            }
        }
    }

    #[test]
    fn roads_run_between_fences_on_both_sides_except_by_towns_and_the_rail() {
        let (world, towns, line, fences) = fenced();
        let roads = world.terrain.roads.clone().unwrap();
        let rails: Vec<(Vec2, Vec2)> = line
            .segments()
            .map(|s| (s.start, s.end))
            .chain(
                towns
                    .iter()
                    .filter_map(|t| t.railway)
                    .map(|r| (r.start, r.end)),
            )
            .collect();
        let sides: Vec<(Vec2, Vec2)> = fences
            .runs
            .iter()
            .filter(|r| r.roadside)
            .flat_map(|r| r.spans().collect::<Vec<_>>())
            .collect();
        let (mut checked, mut fenced) = (0, 0);
        for (_, p, normal) in along_roads(&roads, 20.0) {
            for side in [-1.0, 1.0] {
                let q = p + normal * (side * FenceLayout::ROADSIDE);
                let free = towns.iter().any(|t| {
                    t.leveled()
                        .expanded(FenceLayout::TOWN_MARGIN + 15.0)
                        .contains_point(q)
                }) || rails.iter().any(|&(a, b)| {
                    crate::road::segment_distance(q, a, b) < FenceLayout::RAIL_MARGIN + 15.0
                });
                if free || fences.is_descampado(q) {
                    continue;
                }
                checked += 1;
                let on_fence = sides
                    .iter()
                    .any(|&(a, b)| crate::road::segment_distance(q, a, b) < 0.6);
                let at_gate = fences
                    .gates
                    .iter()
                    .any(|g| g.at.distance(q) < g.width / 2.0 + 1.0);
                assert!(on_fence || at_gate, "no fence beside the road at {q:?}");
                fenced += 1;
            }
        }
        assert!(checked > 500, "{checked}");
        assert_eq!(fenced, checked);
    }

    #[test]
    fn every_field_on_a_road_has_an_open_gate_into_it() {
        let (world, _, _, fences) = fenced();
        let roads = world.terrain.roads.clone().unwrap();
        assert!(fences.frontages.len() > 20, "{}", fences.frontages.len());
        for (id, frontage) in fences.frontages.iter().enumerate() {
            let gates = fences
                .gates
                .iter()
                .filter(|g| g.frontage == id as u32)
                .count();
            let open = fences
                .gates
                .iter()
                .filter(|g| g.frontage == id as u32 && g.open)
                .count();
            if !frontage.fenced {
                assert_eq!(gates, 0);
            } else if frontage.length >= FenceLayout::MIN_FRONTAGE {
                assert!((1..=2).contains(&gates), "{frontage:?}: {gates}");
                assert!(open >= 1, "{frontage:?}: every gate closed");
            }
        }
        let obstacles = fences.obstacles();
        for gate in fences.gates.iter().filter(|g| g.open) {
            // En el alambrado al costado del camino, paralela al camino,
            // abierta.
            let gap = roads.distance(gate.at);
            assert!((gap - FenceLayout::ROADSIDE).abs() < 0.5, "{gate:?}: {gap}");
            // Libre de lado a lado en su abertura, salvo por los rectángulos de
            // los propios postes en sus extremos.
            for k in 2..=8 {
                let p = gate.at + gate.along * (gate.width * (k as f32 / 10.0 - 0.5));
                assert!(
                    !obstacles.iter().any(|o| o.expanded(0.1).contains_point(p)),
                    "{gate:?}"
                );
            }
        }
        let doubles = fences.gates.iter().filter(|g| g.double).count();
        println!("road gates {}, double {doubles}", fences.gates.len());
    }

    #[test]
    fn a_rider_turns_off_the_road_through_a_gate_into_the_field() {
        let (world, _, _, fences) = fenced();
        let roads = world.terrain.roads.clone().unwrap();
        for gate in fences.gates.iter().filter(|g| g.open).step_by(5).take(6) {
            // Desde la línea central del camino, perpendicular, a través de la
            // tranquera.
            let out = Vec2::new(-gate.along.y, gate.along.x);
            let out = if roads.distance(gate.at + out) > roads.distance(gate.at) {
                out
            } else {
                out * -1.0
            };
            let start = gate.at - out * FenceLayout::ROADSIDE;
            let mut rider = Rider::new(start, Heading::from_radians(out.y.atan2(out.x)), world);
            let input = RiderInput {
                throttle: 1.0,
                ..Default::default()
            };
            for _ in 0..300 {
                rider.step(input, &RiderParams::default(), world, 1.0 / 60.0);
            }
            let into = (rider.position - gate.at).dot(out);
            assert!(into > 3.0, "stopped at {:?} by {gate:?}", rider.position);
        }
    }

    #[test]
    fn a_rider_is_stopped_by_the_wire() {
        let (world, _, line, fences) = fenced();
        // Un recorrido alambrado largo sobre y, lejos de los caminos: cabalgar
        // hacia él desde 15 m.
        let run = fences
            .runs
            .iter()
            .find(|r| {
                !r.roadside && r.start().x == r.end().x && r.start().distance(r.end()) > 200.0
            })
            .expect("a long north-south run");
        let mid = (run.start() + run.end()) * 0.5;
        let start = mid + Vec2::new(15.0, 0.0);
        // La tierra se planta solo donde va el jinete.
        let mut world = world.clone();
        let flora = FloraSite::new(world.config, &world.terrain, line).with_fences(fences);
        let size = world.config.chunk_size;
        for p in [mid, start] {
            world.plant_chunk(&flora, crate::ChunkCoord::containing(p, size));
        }
        let mut rider = Rider::new(start, Heading::from_radians(std::f32::consts::PI), &world);
        let input = RiderInput {
            throttle: 1.0,
            gallop: true,
            ..Default::default()
        };
        for _ in 0..180 {
            rider.step(input, &RiderParams::default(), &world, 1.0 / 60.0);
        }
        assert!(
            rider.position.x > mid.x + FenceLayout::THICKNESS / 2.0,
            "through the fence: {:?}",
            rider.position
        );
        assert!(rider.position.x < mid.x + 2.0, "{:?}", rider.position);
    }

    #[test]
    fn every_long_stretch_has_openings_every_300_to_500_m_off_the_rail_and_towns() {
        let (world, towns, line, fences) = fenced();
        // Un tramo: una línea alambrada entre dos huecos que no son aberturas.
        let mut by_stretch: std::collections::BTreeMap<u32, Vec<&Opening>> = Default::default();
        for o in fences.openings.iter().filter(|o| o.stretch != u32::MAX) {
            by_stretch.entry(o.stretch).or_default().push(o);
        }
        for (stretch, length) in &fences.stretches {
            let openings = by_stretch.get(stretch).map_or(0, Vec::len);
            if *length > 500.0 {
                assert!(openings >= 1, "stretch {stretch} of {length} m has none");
            }
        }
        for list in by_stretch.values() {
            for w in list.windows(2) {
                let gap = w[0].at.distance(w[1].at);
                // 300–500 m, corridos hasta 40 m donde una línea que cruza estorba.
                assert!((290.0..=550.0).contains(&gap), "{gap}");
            }
        }
        let rails: Vec<(Vec2, Vec2)> = line
            .segments()
            .map(|s| (s.start, s.end))
            .chain(
                towns
                    .iter()
                    .filter_map(|t| t.railway)
                    .map(|r| (r.start, r.end)),
            )
            .collect();
        for o in &fences.openings {
            for town in towns {
                assert!(!town.leveled().contains_point(o.at));
            }
            let rail = rails
                .iter()
                .map(|&(a, b)| crate::road::segment_distance(o.at, a, b))
                .fold(f32::INFINITY, f32::min);
            assert!(rail >= FenceLayout::RAIL_MARGIN);
            assert!(o.width == 10.0 || o.width == 20.0, "{o:?}");
        }
        // Más o menos la mitad tranqueras de campo y la mitad tramos rotos.
        let gates = fences
            .openings
            .iter()
            .filter(|o| matches!(o.kind, OpeningKind::FieldGate { .. }))
            .count();
        let n = fences.openings.len();
        println!("openings: {n}, field gates {gates}, broken {}", n - gates);
        assert!(n > 500, "{n}");
        assert!(gates * 3 > n && gates * 3 < 2 * n, "{gates} of {n}");
        let _ = world;
    }

    #[test]
    fn no_opening_span_is_an_obstacle_and_a_rider_rides_through() {
        let (world, _, _, fences) = fenced();
        let c = Circle::new(Rider::COLLIDER_RADIUS);
        let obstacles = fences.obstacles();
        let passable = |o: &&Opening| !matches!(o.kind, OpeningKind::FieldGate { open: false, .. });
        for o in fences.openings.iter().filter(passable) {
            let half = o.along * (o.width / 2.0 - 0.2);
            let gap = Aabb::new(
                Vec2::new(
                    (o.at - half).x.min((o.at + half).x),
                    (o.at - half).y.min((o.at + half).y),
                ),
                Vec2::new(
                    (o.at - half).x.max((o.at + half).x),
                    (o.at - half).y.max((o.at + half).y),
                ),
            );
            assert!(!obstacles.iter().any(|b| b.intersects(&gap)), "{o:?}");
        }
        // Cabalgar en perpendicular a través de una abertura de cada tipo.
        for kind in [true, false] {
            let o = fences
                .openings
                .iter()
                .find(|o| match o.kind {
                    OpeningKind::FieldGate { open, .. } => kind && open,
                    OpeningKind::Broken => !kind,
                })
                .unwrap();
            let across = Vec2::new(-o.along.y, o.along.x);
            let start = o.at - across * 12.0;
            let heading = Heading::from_radians(across.y.atan2(across.x));
            let mut rider = Rider::new(start, heading, world);
            let input = RiderInput {
                throttle: 1.0,
                ..Default::default()
            };
            for _ in 0..600 {
                rider.step(input, &RiderParams::default(), world, 1.0 / 60.0);
            }
            let through = (rider.position - o.at).dot(across);
            assert!(through > 5.0, "stopped at {:?} by {o:?}", rider.position);
            let _ = c;
        }
    }

    #[test]
    fn about_four_in_ten_field_gates_are_closed_and_a_closed_gate_blocks() {
        let (world, _, _, fences) = fenced();
        let road_gates = fences.gates.iter().map(|g| g.open);
        let grid_gates = fences.openings.iter().filter_map(|o| match o.kind {
            OpeningKind::FieldGate { open, .. } => Some(open),
            OpeningKind::Broken => None,
        });
        let all: Vec<bool> = road_gates.chain(grid_gates).collect();
        let closed = all.iter().filter(|o| !**o).count();
        println!("field gates {}, closed {closed}", all.len());
        let share = closed as f32 / all.len() as f32;
        assert!((0.3..0.48).contains(&share), "{share}");
        // Cabalgar en perpendicular hacia una tranquera cerrada de la
        // cuadrícula: se detiene en la línea de la tranquera.
        let o = fences
            .openings
            .iter()
            .find(|o| matches!(o.kind, OpeningKind::FieldGate { open: false, .. }))
            .unwrap();
        let across = Vec2::new(-o.along.y, o.along.x);
        let start = o.at - across * 12.0;
        let mut rider = Rider::new(
            start,
            Heading::from_radians(across.y.atan2(across.x)),
            world,
        );
        let input = RiderInput {
            throttle: 1.0,
            ..Default::default()
        };
        for _ in 0..600 {
            rider.step(input, &RiderParams::default(), world, 1.0 / 60.0);
        }
        let through = (rider.position - o.at).dot(across);
        assert!(through < 0.0, "through a closed gate: {:?}", rider.position);
    }

    #[test]
    fn about_one_paddock_in_ten_is_unfenced() {
        // Siempre igual: `the_land_is_surveyed_...` reconstruye toda la
        // distribución, descampados incluidos.
        let (_, _, _, fences) = fenced();
        let cells = fences.cells();
        let open = fences.descampados.len();
        let share = open as f32 / cells as f32;
        println!("descampados {open} of {cells}");
        assert!((0.07..0.13).contains(&share), "{share}");
        // Ningún alambrado al costado del camino corre a lo largo de un
        // descampado.
        for run in fences.runs.iter().filter(|r| r.roadside) {
            for (a, b) in run.spans() {
                assert!(!fences.is_descampado((a + b) * 0.5), "{a:?}");
            }
        }
    }

    /// Celdas de una grilla de 1 m sobre `window` en las que puede pararse un
    /// jinete, alcanzadas desde `from` a través de los huecos de `obstacles`.
    pub(crate) fn reached(world: &World, window: Aabb, from: &[Vec2]) -> (Vec<bool>, usize) {
        let w = window.size().x as usize;
        let h = window.size().y as usize;
        let mut blocked = vec![false; w * h];
        let r = Rider::COLLIDER_RADIUS * 0.8;
        for o in world.obstacles.near(window) {
            let o = o.expanded(r);
            let x0 = ((o.min.x - window.min.x).floor().max(0.0)) as usize;
            let y0 = ((o.min.y - window.min.y).floor().max(0.0)) as usize;
            let x1 = ((o.max.x - window.min.x).ceil() as usize).min(w);
            let y1 = ((o.max.y - window.min.y).ceil() as usize).min(h);
            for y in y0..y1 {
                for x in x0..x1 {
                    let c = window.min + Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                    if o.contains_point(c) {
                        blocked[y * w + x] = true;
                    }
                }
            }
        }
        let mut seen = vec![false; w * h];
        let mut stack: Vec<usize> = from
            .iter()
            .filter(|p| window.contains_point(**p))
            .map(|p| {
                let q = *p - window.min;
                q.y as usize * w + q.x as usize
            })
            .filter(|&i| !blocked[i])
            .collect();
        for &i in &stack {
            seen[i] = true;
        }
        while let Some(i) = stack.pop() {
            let (x, y) = (i % w, i / w);
            let mut go = |j: usize| {
                if !seen[j] && !blocked[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if x > 0 {
                go(i - 1);
            }
            if x + 1 < w {
                go(i + 1);
            }
            if y > 0 {
                go(i - w);
            }
            if y + 1 < h {
                go(i + w);
            }
        }
        (seen, w)
    }

    #[test]
    fn every_field_can_be_reached_from_the_road() {
        let (world, towns, _, fences) = fenced();
        let roads = world.terrain.roads.clone().unwrap();
        // Alrededor de Monte Nievas y del primer tramo del camino a Pico.
        let window = Aabb::new(Vec2::new(-4600.0, -3600.0), Vec2::new(-1000.0, -400.0));
        let from: Vec<Vec2> = along_roads(&roads, 5.0)
            .into_iter()
            .map(|(_, p, _)| p)
            .collect();
        let (seen, w) = reached(world, window, &from);
        let at = |p: Vec2| {
            let q = p - window.min;
            seen[q.y as usize * w + q.x as usize]
        };
        let (xs, ys) = &fences.surveyed;
        let mut checked = 0;
        for i in 0..xs.len() - 1 {
            for j in 0..ys.len() - 1 {
                let cell = Aabb::new(Vec2::new(xs[i], ys[j]), Vec2::new(xs[i + 1], ys[j + 1]));
                if !window.contains(&cell) {
                    continue;
                }
                // Se muestrea el centro de cada campo, lejos de caminos,
                // pueblos y la vía.
                let c = cell.center();
                let clear = roads.distance(c) > 30.0
                    && towns
                        .iter()
                        .all(|t| !t.leveled().expanded(30.0).contains_point(c))
                    && world
                        .obstacles
                        .near(Aabb::from_center_size(c, Vec2::new(4.0, 4.0)))
                        .is_empty();
                if !clear {
                    continue;
                }
                assert!(at(c), "the field around {c:?} cannot be reached");
                checked += 1;
            }
        }
        assert!(checked >= 10, "{checked}");
    }

    #[test]
    fn trees_keep_off_the_fence_lines() {
        let (world, _, line, fences) = fenced();
        let flora = FloraSite::new(world.config, &world.terrain, line).with_fences(fences);
        // Un cuadrado de campos de 2 km alrededor de Monte Nievas, no el mundo
        // entero.
        let size = world.config.chunk_size;
        let lo = crate::ChunkCoord::containing(Vec2::new(-4000.0, -3000.0), size);
        let hi = crate::ChunkCoord::containing(Vec2::new(-2000.0, -1000.0), size);
        let trees: Vec<_> = (lo.y..=hi.y)
            .flat_map(|y| (lo.x..=hi.x).map(move |x| crate::ChunkCoord::new(x, y)))
            .flat_map(|c| flora.trees(c))
            .collect();
        assert!(trees.len() > 200, "{}", trees.len());
        for t in &trees {
            assert!(
                fences.line_distance(t.at) >= FloraSite::TREE_FENCE_MARGIN,
                "{t:?}"
            );
        }
    }
}
