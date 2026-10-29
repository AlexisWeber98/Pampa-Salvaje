//! La línea del ferrocarril: una vía que pasa por cada pueblo con ferrocarril
//! del mundo.
//!
//! Dentro de un pueblo la vía se mantiene alineada con los ejes a lo largo de
//! la estación (ver [`TownLayout::railway`]). Entre dos pueblos listados uno al
//! lado del otro, un enlace une la punta del tramo final de la vía de un pueblo
//! más cercana al otro pueblo con la punta del otro pueblo más cercana al
//! primero, de modo que las vías de los pueblos y los enlaces forman una sola
//! línea continua.
//!
//! Un enlace sale de su primera punta en línea recta siguiendo la vía de ese
//! pueblo, entra en la diagonal con una curva circular amplia, sigue recto y
//! vuelve a la vía del segundo pueblo justo cuando llega a la punta de ese
//! pueblo. Las curvas quedan completamente en el enlace, así que la vía recta
//! propia de un pueblo nunca se acorta, y la línea mantiene una misma
//! orientación al pasar por cada punta.
//!
//! Un ramal ([`RailBranch`]) sale de la línea en un pueblo de empalme y llega
//! hasta pueblos a los que solo sirve él: su primer enlace empieza en la punta
//! del empalme más cercana al ramal, la misma punta por la que puede salir ya
//! la línea, así que las dos vías se separan ahí con sus propias curvas (de
//! Metileo a Caleufú, como el ramal real Metileo–Arizona de 1911).
//!
//! El pueblo de empalme de un ramal también tiene una vía muerta ([`RailBay`]):
//! una vía sin salida al lado de la vía de la estación, enfrente del andén de
//! la estación, para que la lanzadera del ramal nunca se detenga sobre los
//! rieles del tren de la línea.
//!
//! La línea es una regla del mundo: todo pueblo con ferrocarril debe estar
//! sobre ella o sobre un ramal, y ningún enlace puede atravesar otro pueblo ni
//! salir del mundo. Los datos incorrectos nunca hacen fallar el juego; los
//! pueblos o enlaces problemáticos se dejan fuera y se informan.

use std::fmt;

use crate::train::{branch_consist, consist_length};
use crate::{Aabb, BuildingKind, Floor, Railway, RoadNetwork, TownLayout, Vec2, WorldBounds};

/// Un tramo de vía entre dos pueblos de la línea.
#[derive(Debug, Clone, PartialEq)]
pub struct RailLink {
    /// Índice (dentro de los pueblos del mundo) del pueblo del que sale el
    /// enlace.
    pub from: usize,
    /// Índice del pueblo al que llega el enlace.
    pub to: usize,
    /// La cuerda recta desde una punta del tramo final de `from` hasta una
    /// punta del tramo final de `to`. La vía en sí sigue [`Self::path`].
    pub track: Railway,
    /// La línea central que sigue la vía, desde `track.start` hasta
    /// `track.end`: tramos rectos cortos que se desvían como mucho
    /// [`RailLine::MAX_TURN`] radianes de uno al siguiente.
    pub path: Vec<Vec2>,
    /// Las curvas del enlace, en el orden del recorrido (ninguna en un enlace
    /// que ya sigue recto desde ambas puntas).
    pub curves: Vec<RailCurve>,
}

/// Una curva circular de la línea, encajada en la esquina entre dos rectas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RailCurve {
    /// Radio del arco, en metros: [`RailLine::CURVE_RADIUS`] salvo que las
    /// rectas que lo rodean sean demasiado cortas para eso.
    pub radius: f32,
    /// Distancia desde la esquina hasta cada extremo del arco, en metros
    /// (`radius * tan(turn / 2)`).
    pub tangent: f32,
    /// Cuánto gira la vía, en radianes, positivo en sentido antihorario.
    pub turn: f32,
}

impl RailLink {
    /// Los tramos rectos de [`Self::path`], en orden.
    pub fn segments(&self) -> impl Iterator<Item = Railway> + '_ {
        let gauge = self.track.gauge;
        self.path.windows(2).map(move |w| Railway {
            start: w[0],
            end: w[1],
            gauge,
        })
    }

    /// Longitud de la vía a lo largo de [`Self::path`], en metros.
    pub fn length(&self) -> f32 {
        self.path.windows(2).map(|w| w[0].distance(w[1])).sum()
    }
}

/// La línea del ferrocarril que pasa por los pueblos del mundo.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RailLine {
    /// Índices (dentro de los pueblos del mundo) de los pueblos de la línea, en
    /// orden.
    pub towns: Vec<usize>,
    /// Los enlaces entre pueblos consecutivos, en el orden de la línea, y
    /// después los enlaces de cada ramal en el orden de los ramales. Un enlace
    /// que rompe una regla se deja fuera, así que la línea puede tener huecos.
    pub links: Vec<RailLink>,
    /// Los ramales que salen de la línea, en el orden en que se listaron.
    pub branches: Vec<RailBranch>,
    /// La vía muerta en el pueblo de empalme de cada ramal, en el orden de los
    /// ramales (un ramal cuyo empalme no tiene lugar para una no tiene
    /// ninguna).
    pub bays: Vec<RailBay>,
}

/// Un ramal del ferrocarril: vía desde un pueblo de la línea (el empalme) que
/// llega hasta pueblos a los que no llega ninguna otra vía. Sus enlaces están
/// en [`RailLine::links`] junto con los de la línea, así que todo lo que coloca
/// o dibuja vías lo trata igual; solo el tren de la línea se mantiene en la
/// línea.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RailBranch {
    /// Índices (dentro de los pueblos del mundo) de los pueblos del ramal, en
    /// orden, primero el empalme.
    pub towns: Vec<usize>,
}

/// Una vía muerta sin salida en el pueblo de empalme de un ramal, al lado de la
/// vía de la estación, del lado opuesto al pueblo, y unida a ella por un
/// desvío, para que la lanzadera del ramal se detenga ahí mientras el tren de
/// la línea ocupa la vía de la estación.
///
/// Su [`Self::link`] es una pieza más de vía (`from` y `to` son el empalme),
/// así que todo lo que coloca, dibuja o se mantiene lejos de la vía de la línea
/// ve la vía muerta sin ningún cambio. El recorrido va desde el paragolpes, a
/// la altura del extremo del andén de la estación opuesto a la punta del
/// empalme, en línea recta a lo largo de la vía muerta pasando el andén, y
/// después sale por el desvío (dos arcos de [`RailBay::TURNOUT_RADIUS`]) hasta
/// el tramo final recto de la vía de la estación hacia la punta del empalme,
/// donde sigue el ramal. La lanzadera se detiene enfrente del andén de la
/// estación, en el andén propio de la vía muerta ([`Self::platform`]); la gente
/// cruza las vías entre los andenes por el paso a nivel del camino, a través de
/// [`Self::walks`].
#[derive(Debug, Clone, PartialEq)]
pub struct RailBay {
    /// Índice (dentro de los ramales del mundo) del ramal al que sirve la vía
    /// muerta.
    pub branch: usize,
    /// Índice (dentro de los pueblos del mundo) del pueblo de empalme.
    pub town: usize,
    /// La vía muerta y su desvío, desde el paragolpes hasta el punto donde se
    /// une con la vía de la estación.
    pub link: RailLink,
    /// Dónde queda el centro de la formación de la lanzadera, detenida en la
    /// vía muerta enfrente del centro del andén de la estación.
    pub stop: Vec2,
    /// El andén de la vía muerta, del lado opuesto a la vía de la estación, a
    /// lo largo de la lanzadera detenida y enfrente del andén de la estación:
    /// construido y tan alto como el andén de una estación.
    pub platform: Floor,
    /// Los senderos de tierra apisonada desde el extremo de cada andén hacia la
    /// punta del empalme hasta el camino que cruza ahí la vía de la estación,
    /// para que la gente cambie de andén por su paso a nivel: primero el
    /// sendero del andén de la estación y después el del andén de la vía
    /// muerta. Vacío hasta que [`RailLine::lay_walks`] haya visto los caminos.
    pub walks: Vec<Aabb>,
}

impl RailBay {
    /// Distancia desde la línea central de la vía de la estación hasta la de la
    /// vía muerta, en metros (vagones de 3 m de ancho y una separación de 1,5 m
    /// entre ellos).
    pub const SPACING: f32 = 4.5;
    /// Vía libre entre el paragolpes y la cola de una lanzadera detenida, en
    /// metros.
    pub const BUFFER_GAP: f32 = 2.0;
    /// Vía recta más allá del frente de una lanzadera detenida antes de que
    /// empiece el desvío, en metros.
    pub const FRONT_CLEAR: f32 = 10.0;
    /// Tramo final que queda entre el final del desvío y la punta del empalme,
    /// en metros.
    const RUN_OUT_LEFT: f32 = 100.0;
    /// Radio de los arcos del desvío de la vía muerta, en metros: más cerrado
    /// que el de la línea, para que la vía muerta vuelva a unirse a la vía de
    /// la estación antes del camino que la cruza junto al andén (un desvío
    /// cerrado, como el de una playa de maniobras).
    pub const TURNOUT_RADIUS: f32 = 150.0;
    /// Distancia desde la línea central de la vía muerta hasta el borde de su
    /// andén junto a la vía, en metros: como en una estación
    /// ([`Floor::PLATFORM_EDGE`]).
    pub const PLATFORM_EDGE: f32 = Floor::PLATFORM_EDGE;
    /// Ancho del andén de la vía muerta a lo ancho de la vía, en metros: su
    /// borde lejano queda a 11 m de la vía, como el del andén de la estación,
    /// así que el andén creció hacia la vía cuando su borde se acercó.
    pub const PLATFORM_DEPTH: f32 = 11.0 - Self::PLATFORM_EDGE;
    /// Ancho de los senderos desde los andenes hasta el camino, en metros.
    pub const WALK_WIDTH: f32 = 2.0;
    /// A qué distancia de la línea central del camino termina un sendero, en
    /// metros: dentro del borde de la calzada, para que los dos se unan.
    const WALK_INTO_ROAD: f32 = 2.5;
    /// Lo más lejos que llega un sendero desde su andén buscando el camino, en
    /// metros.
    const WALK_REACH: f32 = 150.0;
    /// Qué tan cerca del paso a nivel de la vía de la estación se une un
    /// sendero con el camino, en metros.
    const WALK_NEAR_CROSSING: f32 = 25.0;

    /// El extremo sin salida de la vía.
    pub fn buffer(&self) -> Vec2 {
        self.link.path[0]
    }

    /// Dirección unitaria de la vía muerta en su paragolpes, apuntando hacia
    /// afuera de la vía muerta (por donde sale la lanzadera).
    pub fn heading(&self) -> Vec2 {
        unit(self.link.path[1] - self.link.path[0]).unwrap_or(Vec2::new(1.0, 0.0))
    }
}

/// Por qué un pueblo o un enlace quedó fuera de la línea del ferrocarril.
#[derive(Debug, Clone, PartialEq)]
pub enum RailLineError {
    /// La línea nombra un pueblo que el mundo no tiene (o que no se pudo
    /// construir).
    UnknownTown { name: String },
    /// La línea nombra un pueblo dos veces.
    DuplicateTown { name: String },
    /// La línea nombra un pueblo sin ferrocarril.
    NoRailway { name: String },
    /// Un pueblo con ferrocarril que la línea deja fuera.
    NotOnLine { name: String },
    /// El enlace entre `from` y `to` atraviesa el terreno nivelado de `other`
    /// (`other` puede ser el propio `from` o `to`, cuando el enlace vuelve a
    /// cruzar su propio pueblo).
    CrossesTown {
        from: String,
        to: String,
        other: String,
    },
    /// El enlace entre `from` y `to` sale del mundo.
    OutsideWorld { from: String, to: String },
    /// Un ramal cuyo primer pueblo (`name`) no está sobre la línea, o que no
    /// nombra ningún pueblo más allá de él.
    BadJunction { name: String },
}

impl fmt::Display for RailLineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "railway line: ")?;
        match self {
            Self::UnknownTown { name } => {
                write!(f, "`{name}` is not a town of the world; left out")
            }
            Self::DuplicateTown { name } => {
                write!(
                    f,
                    "`{name}` is listed more than once; only the first is kept"
                )
            }
            Self::NoRailway { name } => write!(f, "`{name}` has no railway; left out"),
            Self::NotOnLine { name } => write!(
                f,
                "`{name}` has a railway but is not on the line; every railway town must be"
            ),
            Self::CrossesTown { from, to, other } => write!(
                f,
                "the link from `{from}` to `{to}` runs across `{other}`; link left out"
            ),
            Self::OutsideWorld { from, to } => write!(
                f,
                "the link from `{from}` to `{to}` leaves the world; link left out"
            ),
            Self::BadJunction { name } => write!(
                f,
                "the branch from `{name}` must start at a town on the line and reach a town past it; branch left out"
            ),
        }
    }
}

impl std::error::Error for RailLineError {}

impl RailLine {
    /// Radio de las curvas de la línea, en metros, donde las rectas que rodean
    /// una curva dejan lugar para él.
    pub const CURVE_RADIUS: f32 = 500.0;
    /// El tramo recto más largo con el que se dibuja una curva, en metros.
    pub const CURVE_STEP: f32 = 8.0;
    /// Mayor cambio de orientación de un tramo de una curva al siguiente, en
    /// radianes (2°).
    pub const MAX_TURN: f32 = 2.0 * std::f32::consts::PI / 180.0;

    /// Construye la línea a través de `towns` (ya ubicados, ver
    /// [`crate::place_towns`]) dentro de `bounds`.
    ///
    /// `names` lista los pueblos en el orden de la línea. Sin esa lista, todos
    /// los pueblos con ferrocarril están sobre la línea, de oeste a este según
    /// el centro de su cuadrícula. Los nombres desconocidos, duplicados y sin
    /// ferrocarril se dejan fuera, los pueblos con ferrocarril que faltan en la
    /// lista quedan fuera de la línea, y los enlaces que atraviesan otro pueblo
    /// o salen del mundo se descartan; cada caso se informa.
    pub fn build(
        names: Option<&[String]>,
        towns: &[TownLayout],
        bounds: WorldBounds,
    ) -> (Self, Vec<RailLineError>) {
        Self::build_with_branches(names, &[], towns, bounds)
    }

    /// [`Self::build`], con los `branches` que salen de la línea: cada uno
    /// lista un pueblo de la línea (el empalme) y después, en orden, los
    /// pueblos hasta los que llega el ramal. Los pueblos de un ramal siguen las
    /// reglas de nombres de la línea (nunca un pueblo que ya está en la línea o
    /// en otro ramal); un ramal cuyo empalme no está en la línea, o que no
    /// conserva ningún pueblo más allá de él, se deja fuera y se informa.
    pub fn build_with_branches(
        names: Option<&[String]>,
        branches: &[Vec<String>],
        towns: &[TownLayout],
        bounds: WorldBounds,
    ) -> (Self, Vec<RailLineError>) {
        let mut errors = Vec::new();
        let order = match names {
            Some(names) => listed(names, towns, &mut errors),
            None => west_to_east(towns),
        };
        let mut kept: Vec<RailBranch> = Vec::new();
        for list in branches {
            let junction = list
                .first()
                .and_then(|name| towns.iter().position(|t| &t.name == name))
                .filter(|i| order.contains(i));
            let taken: Vec<usize> = order
                .iter()
                .chain(kept.iter().flat_map(|b| &b.towns))
                .copied()
                .collect();
            let past = listed(list.get(1..).unwrap_or_default(), towns, &mut errors);
            let mut past = past;
            past.retain(|i| {
                let free = !taken.contains(i);
                if !free {
                    errors.push(RailLineError::DuplicateTown {
                        name: towns[*i].name.clone(),
                    });
                }
                free
            });
            match junction {
                Some(junction) if !past.is_empty() => {
                    let mut branch = vec![junction];
                    branch.extend(past);
                    kept.push(RailBranch { towns: branch });
                }
                _ => errors.push(RailLineError::BadJunction {
                    name: list.first().cloned().unwrap_or_default(),
                }),
            }
        }
        errors.extend(
            towns
                .iter()
                .enumerate()
                .filter(|(i, t)| {
                    t.railway.is_some()
                        && !order.contains(i)
                        && !kept.iter().any(|b| b.towns.contains(i))
                })
                .map(|(_, t)| RailLineError::NotOnLine {
                    name: t.name.clone(),
                }),
        );
        let mut links = Vec::new();
        let pairs = order
            .windows(2)
            .chain(kept.iter().flat_map(|b| b.towns.windows(2)));
        for pair in pairs {
            match link(towns, pair[0], pair[1], bounds) {
                Ok(link) => links.push(link),
                Err(error) => errors.push(error),
            }
        }
        let mut bays: Vec<RailBay> = Vec::new();
        for (index, branch) in kept.iter().enumerate() {
            let first = links
                .iter()
                .find(|l| l.from == branch.towns[0] && l.to == branch.towns[1]);
            let beside = bays.iter().filter(|b| b.town == branch.towns[0]).count();
            if let Some(bay) = first.and_then(|f| bay(towns, index, f, beside)) {
                bays.push(bay);
            }
        }
        (
            Self {
                towns: order,
                links,
                branches: kept,
                bays,
            },
            errors,
        )
    }

    /// Los tramos rectos de la vía de cada enlace, en el orden de la línea, y
    /// después los de cada vía muerta.
    pub fn segments(&self) -> impl Iterator<Item = Railway> + '_ {
        self.tracks().flat_map(RailLink::segments)
    }

    /// Cada tramo de vía entre pueblos y en las vías muertas de los empalmes:
    /// primero los enlaces y después las vías muertas.
    fn tracks(&self) -> impl Iterator<Item = &RailLink> + '_ {
        self.links.iter().chain(self.bays.iter().map(|b| &b.link))
    }

    /// Las partes de los enlaces dentro de `area` (un chunk del terreno, un
    /// cuadro del mapa), una por tramo recto, en el orden de la línea. Se
    /// saltean las partes que solo tocan su borde.
    pub fn segments_in(&self, area: Aabb) -> Vec<RailSpan> {
        let mut spans = Vec::new();
        for link in self.tracks() {
            let total = link.length();
            let mut from = 0.0;
            for track in link.segments() {
                let length = track.start.distance(track.end);
                if let Some((enter, exit)) = area.segment_overlap(track.start, track.end) {
                    let (enter, exit) = (from + enter * length, from + exit * length);
                    if exit - enter > CLIP_EPSILON {
                        spans.push(RailSpan {
                            track,
                            from,
                            enter,
                            exit,
                            total,
                        });
                    }
                }
                from += length;
            }
        }
        spans
    }
}

impl RailLine {
    /// La línea entera como un solo recorrido que puede hacer un tren, desde la
    /// punta lejana del tramo final del primer pueblo hasta la punta lejana del
    /// último pueblo conectado: la vía recta de cada pueblo, de punta a punta,
    /// unida por los enlaces curvos.
    ///
    /// El recorrido se detiene en el primer hueco (un enlace dejado fuera); una
    /// línea sin enlaces es solo la vía de su primer pueblo, y una línea sin
    /// pueblos está vacía.
    pub fn full_path(&self, towns: &[TownLayout]) -> LinePath {
        self.path_through(&self.connected_towns(), towns)
    }

    /// Los pueblos por los que pasa [`Self::full_path`], en el orden de la
    /// línea: el primer pueblo y cada pueblo al que se llega desde él sin un
    /// hueco.
    pub fn connected_towns(&self) -> Vec<usize> {
        self.connected(&self.towns)
    }

    /// Cada ramal como un solo recorrido, como [`Self::full_path`]: desde la
    /// punta lejana del empalme, pasando por su estación y saliendo a lo largo
    /// del ramal hasta la punta lejana del último pueblo conectado, de modo que
    /// avanza hacia afuera desde la línea. Un ramal al que le falta su primer
    /// enlace es solo la vía del empalme.
    pub fn branch_paths(&self, towns: &[TownLayout]) -> Vec<LinePath> {
        self.branches
            .iter()
            .map(|b| self.path_through(&self.connected(&b.towns), towns))
            .collect()
    }

    /// Cada recorrido de vía que podría hacer un tren: primero
    /// [`Self::full_path`], después [`Self::branch_paths`] y después cada vía
    /// muerta desde su paragolpes hasta donde se une con la vía de la estación.
    /// La vía propia de cada pueblo y sus tramos finales están en alguno de
    /// ellos.
    pub fn track_paths(&self, towns: &[TownLayout]) -> Vec<LinePath> {
        let mut paths = vec![self.full_path(towns)];
        paths.extend(self.branch_paths(towns));
        paths.extend(self.bays.iter().map(|b| LinePath::new(b.link.path.clone())));
        paths
    }

    /// Cada ramal como el recorrido que hace su lanzadera, como
    /// [`Self::branch_paths`] pero saliendo de la vía muerta del empalme: desde
    /// el paragolpes de la vía muerta, pasando por el desvío, a lo largo del
    /// tramo final de la vía de la estación y del ramal hasta la punta lejana
    /// del último pueblo conectado. Un ramal sin vía muerta hace su recorrido
    /// de [`Self::branch_paths`].
    pub fn shuttle_paths(&self, towns: &[TownLayout]) -> Vec<LinePath> {
        self.branch_paths(towns)
            .into_iter()
            .enumerate()
            .map(|(index, path)| {
                let Some(bay) = self.bay_of(index) else {
                    return path;
                };
                // La vía muerta se une al tramo final recto: se sigue desde
                // ahí.
                let joined = *bay.link.path.last().expect("a bay has a path");
                let at = path.project(joined) + CLIP_EPSILON;
                let mut points = bay.link.path.clone();
                points.extend(
                    path.points()
                        .iter()
                        .zip(&path.arcs)
                        .filter(|&(_, &arc)| arc > at)
                        .map(|(&p, _)| p),
                );
                LinePath::new(points)
            })
            .collect()
    }

    /// Los andenes de las vías muertas, como pisos por los que camina la gente
    /// y desde los que sube.
    pub fn floors(&self) -> Vec<Floor> {
        self.bays.iter().map(|b| b.platform).collect()
    }

    /// Coloca los [`RailBay::walks`] de cada vía muerta: desde el extremo del
    /// andén de la estación y del andén de la vía muerta hacia la punta del
    /// empalme, en línea recta por el centro del andén hasta el camino que
    /// cruza ahí la vía de la estación. La gente cambia de tren por ese paso a
    /// nivel: queda pasando el desvío, más allá de donde se detiene cualquier
    /// tren, así que siempre está libre. Una vía muerta sin ese camino, o cuyos
    /// andenes no llegan hasta él, no tiene senderos.
    pub fn lay_walks(&mut self, towns: &[TownLayout], roads: &RoadNetwork) {
        for bay in &mut self.bays {
            bay.walks = towns
                .get(bay.town)
                .and_then(|town| walks(bay, town, roads))
                .unwrap_or_default();
        }
    }

    /// La vía muerta del ramal `branch` (índice dentro de [`Self::branches`]).
    pub fn bay_of(&self, branch: usize) -> Option<&RailBay> {
        self.bays.iter().find(|b| b.branch == branch)
    }

    /// Los pueblos de `order` a los que se llega desde el primero sin un hueco.
    fn connected(&self, order: &[usize]) -> Vec<usize> {
        let mut served: Vec<usize> = order.iter().take(1).copied().collect();
        for pair in order.windows(2) {
            if self.link(pair[0], pair[1]).is_none() {
                break;
            }
            served.push(pair[1]);
        }
        served
    }

    /// El recorrido a través de `served` (pueblos unidos por enlaces, en
    /// orden): la punta lejana del primer pueblo, cada enlace y la punta lejana
    /// del último pueblo.
    fn path_through(&self, served: &[usize], towns: &[TownLayout]) -> LinePath {
        let Some(rail) = served.first().and_then(|&i| towns.get(i)?.railway) else {
            return LinePath::default();
        };
        let first_link = served.get(1).and_then(|&next| self.link(served[0], next));
        let mut points = vec![match first_link {
            Some(link) => far_tip(rail, link.track.start),
            None => rail.start,
        }];
        for pair in served.windows(2) {
            if let Some(link) = self.link(pair[0], pair[1]) {
                points.extend(&link.path);
            }
        }
        let last = served.last().and_then(|&i| towns.get(i)?.railway);
        if let (Some(rail), Some(&arrived)) = (last, points.last()) {
            points.push(far_tip(rail, arrived));
        }
        LinePath::new(points)
    }

    /// El enlace del pueblo `from` al pueblo `to`, si la línea lo tiene.
    fn link(&self, from: usize, to: usize) -> Option<&RailLink> {
        self.links.iter().find(|l| l.from == from && l.to == to)
    }
}

/// La punta de `rail` más alejada de `tip`.
fn far_tip(rail: Railway, tip: Vec2) -> Vec2 {
    if rail.start.distance(tip) < rail.end.distance(tip) {
        rail.end
    } else {
        rail.start
    }
}

/// Una polilínea medida por longitud de arco: por donde avanza un tren.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LinePath {
    points: Vec<Vec2>,
    /// Longitud de arco desde el primer punto hasta cada punto.
    arcs: Vec<f32>,
}

impl LinePath {
    /// El recorrido a través de `points`, descartando los puntos repetidos.
    pub fn new(points: Vec<Vec2>) -> Self {
        let mut kept: Vec<Vec2> = Vec::with_capacity(points.len());
        for p in points {
            push_point(&mut kept, p);
        }
        let mut arcs = Vec::with_capacity(kept.len());
        let mut total = 0.0;
        for (i, p) in kept.iter().enumerate() {
            if i > 0 {
                total += kept[i - 1].distance(*p);
            }
            arcs.push(total);
        }
        Self { points: kept, arcs }
    }

    /// Las esquinas del recorrido, en orden.
    pub fn points(&self) -> &[Vec2] {
        &self.points
    }

    /// Longitud del recorrido, en metros.
    pub fn length(&self) -> f32 {
        self.arcs.last().copied().unwrap_or(0.0)
    }

    /// El punto a `distance` metros a lo largo del recorrido, limitado a sus
    /// extremos.
    pub fn at(&self, distance: f32) -> Vec2 {
        let Some(i) = self.piece(distance) else {
            return self.points.first().copied().unwrap_or(Vec2::ZERO);
        };
        let (a, b) = (self.points[i], self.points[i + 1]);
        let span = self.arcs[i + 1] - self.arcs[i];
        let t = ((distance - self.arcs[i]) / span).clamp(0.0, 1.0);
        a + (b - a) * t
    }

    /// La dirección unitaria de avance a `distance` metros a lo largo del
    /// recorrido (la orientación del tramo recto en ese punto), hacia el este
    /// en un recorrido vacío.
    pub fn direction_at(&self, distance: f32) -> Vec2 {
        self.piece(distance)
            .and_then(|i| unit(self.points[i + 1] - self.points[i]))
            .unwrap_or(Vec2::new(1.0, 0.0))
    }

    /// Distancia a lo largo del recorrido hasta el punto del recorrido más
    /// cercano a `p`.
    pub fn project(&self, p: Vec2) -> f32 {
        let mut best = (f32::INFINITY, 0.0);
        for (i, w) in self.points.windows(2).enumerate() {
            let d = w[1] - w[0];
            let span = d.length();
            let t = ((p - w[0]).dot(d) / (span * span)).clamp(0.0, 1.0);
            let gap = p.distance(w[0] + d * t);
            if gap < best.0 {
                best = (gap, self.arcs[i] + span * t);
            }
        }
        best.1
    }

    /// Índice del tramo recto que contiene `distance` (limitada), o `None`
    /// cuando el recorrido no tiene tramos.
    fn piece(&self, distance: f32) -> Option<usize> {
        let pieces = self.points.len().checked_sub(1).filter(|&n| n > 0)?;
        let i = self.arcs.partition_point(|&a| a <= distance);
        Some(i.saturating_sub(1).min(pieces - 1))
    }
}

/// La parte de un tramo recto de una vía dentro de un área, medida a lo largo
/// de toda la vía a la que pertenece (el recorrido de un enlace, o la vía recta
/// de un pueblo): el tramo, a qué distancia a lo largo de la vía empieza el
/// tramo, dónde empieza y dónde termina la parte, y la longitud de toda la vía,
/// todo en metros. Medir a lo largo de toda la vía permite que cada parte
/// coloque sus durmientes donde los colocaría la vía completa, así que las
/// partes dibujadas chunk por chunk y tramo por tramo se unen sin costura.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RailSpan {
    /// El tramo recto sobre el que está la parte.
    pub track: Railway,
    /// Distancia a lo largo de toda la vía hasta `track.start`.
    pub from: f32,
    /// Distancia a lo largo de toda la vía hasta donde empieza la parte.
    pub enter: f32,
    /// Distancia a lo largo de toda la vía hasta donde termina la parte.
    pub exit: f32,
    /// Longitud de toda la vía.
    pub total: f32,
}

impl RailSpan {
    /// Toda una vía recta simple.
    pub fn whole(track: Railway) -> Self {
        let total = track.start.distance(track.end);
        Self {
            track,
            from: 0.0,
            enter: 0.0,
            exit: total,
            total,
        }
    }

    /// Dónde empieza la parte sobre el suelo.
    pub fn start(&self) -> Vec2 {
        self.at(self.enter)
    }

    /// Dónde termina la parte sobre el suelo.
    pub fn end(&self) -> Vec2 {
        self.at(self.exit)
    }

    /// El punto a `distance` metros a lo largo de toda la vía (sobre la línea
    /// de este tramo).
    fn at(&self, distance: f32) -> Vec2 {
        let along = self.track.end - self.track.start;
        let length = along.length();
        if length <= 0.0 {
            return self.track.start;
        }
        self.track.start + along * ((distance - self.from) / length)
    }
}

/// Las partes recortadas más cortas que esto (en metros) solo tocan un área.
const CLIP_EPSILON: f32 = 1e-3;

/// Los pueblos que lista `names`, como índices, conservando el primero de cada
/// nombre y dejando fuera (e informando) los pueblos desconocidos y los que no
/// tienen ferrocarril.
fn listed(names: &[String], towns: &[TownLayout], errors: &mut Vec<RailLineError>) -> Vec<usize> {
    let mut order: Vec<usize> = Vec::new();
    for name in names {
        let Some(index) = towns.iter().position(|t| &t.name == name) else {
            errors.push(RailLineError::UnknownTown { name: name.clone() });
            continue;
        };
        if order.contains(&index) {
            errors.push(RailLineError::DuplicateTown { name: name.clone() });
        } else if towns[index].railway.is_none() {
            errors.push(RailLineError::NoRailway { name: name.clone() });
        } else {
            order.push(index);
        }
    }
    order
}

/// Todos los pueblos con ferrocarril, de oeste a este según el centro de su
/// cuadrícula (el orden de la lista desempata).
fn west_to_east(towns: &[TownLayout]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..towns.len())
        .filter(|&i| towns[i].railway.is_some())
        .collect();
    order.sort_by(|&a, &b| {
        let x = |i: usize| towns[i].grid().center().x;
        x(a).total_cmp(&x(b))
    });
    order
}

/// El enlace del pueblo `from` al pueblo `to`, ambos con ferrocarril, o por qué
/// rompe las reglas: su vía curva debe mantenerse dentro de `bounds` y lejos
/// del terreno nivelado de todos los demás pueblos, balasto incluido. Solo
/// puede salir del terreno de sus dos propios pueblos, nunca volver a cruzarlo.
fn link(
    towns: &[TownLayout],
    from: usize,
    to: usize,
    bounds: WorldBounds,
) -> Result<RailLink, RailLineError> {
    let (a, b) = (&towns[from], &towns[to]);
    // Solo los pueblos con ferrocarril entran en la línea, así que esto nunca
    // se dispara.
    let (Some(rail_a), Some(rail_b)) = (a.railway, b.railway) else {
        let name = if a.railway.is_none() {
            &a.name
        } else {
            &b.name
        };
        return Err(RailLineError::NoRailway { name: name.clone() });
    };
    let (start, start_back) = nearest_tip(rail_a, b.grid().center());
    let (end, end_back) = nearest_tip(rail_b, a.grid().center());
    // Sale de `a` en la dirección de su vía; llega a `b` entrando en la
    // dirección de la suya.
    let out = unit(start - start_back).unwrap_or(Vec2::ZERO);
    let into = unit(end_back - end).unwrap_or(Vec2::ZERO);
    let (path, curves) = smooth_link(start, out, end, into, RailLine::CURVE_RADIUS);

    let names = || (a.name.clone(), b.name.clone());
    let half = TownLayout::RAIL_CORRIDOR_HALF;
    let inside = |p: &Vec2| {
        p.x - half >= bounds.min.x
            && p.y - half >= bounds.min.y
            && p.x + half <= bounds.max.x
            && p.y + half <= bounds.max.y
    };
    // Si cada esquina del recorrido está dentro del mundo (convexo), todo el
    // recorrido queda adentro.
    if !path.iter().all(inside) {
        let (from, to) = names();
        return Err(RailLineError::OutsideWorld { from, to });
    }
    let crossed = towns.iter().enumerate().find(|&(i, other)| {
        let ground = other.leveled().expanded(half);
        i != from
            && i != to
            && path
                .windows(2)
                .any(|w| ground.segment_overlap(w[0], w[1]).is_some())
    });
    // Una vez fuera del primer pueblo, nunca vuelve; en el último, solo llega
    // (leído desde ese extremo, tampoco vuelve nunca).
    let own = |town: &TownLayout| town.leveled().expanded(half);
    let crossed = crossed.map(|(_, other)| other).or_else(|| {
        let back_into_a = reenters(own(a), path.windows(2));
        let back_into_b = reenters(own(b), path.windows(2).rev());
        if back_into_a {
            Some(a)
        } else if back_into_b {
            Some(b)
        } else {
            None
        }
    });
    if let Some(other) = crossed {
        let (from, to) = names();
        return Err(RailLineError::CrossesTown {
            from,
            to,
            other: other.name.clone(),
        });
    }
    Ok(RailLink {
        from,
        to,
        track: Railway {
            start,
            end,
            gauge: rail_a.gauge,
        },
        path,
        curves,
    })
}

/// La vía muerta de `branch` en su empalme, al lado de la vía de la estación
/// del pueblo del que sale el enlace `first` del ramal, junto a las otras vías
/// muertas `beside` que ya estén ahí (cada una ocupa la siguiente vía hacia
/// afuera). La vía muerta queda del lado de la vía opuesto al pueblo, enfrente
/// del andén de la estación: su paragolpes a la altura del extremo del andén
/// alejado de la punta del empalme, la lanzadera detenida en el centro del
/// andén, y su desvío en el tramo final, pasando el otro extremo del andén.
/// `None` para un pueblo sin ferrocarril o que creció a ambos lados de su
/// línea, y cuando el tramo final es demasiado corto para contener la vía
/// muerta y su desvío.
fn bay(towns: &[TownLayout], branch: usize, first: &RailLink, beside: usize) -> Option<RailBay> {
    let town = towns.get(first.from)?;
    let rail = town.railway?;
    if town.rail_band().is_some() {
        return None;
    }
    let tip = first.track.start;
    let back = far_tip(rail, tip);
    let out = unit(tip - back)?;
    // El lado de la vía opuesto al pueblo.
    let left = Vec2::new(-out.y, out.x);
    let side = if (town.grid().center() - back).dot(left) > 0.0 {
        left * -1.0
    } else {
        left
    };
    // A lo largo de la vía desde su punta lejana, y hacia afuera de ella.
    let at = |x: f32, y: f32| back + out * x + side * y;
    // La extensión del andén de la estación a lo largo de la vía, desde su
    // punta lejana.
    let platform = town
        .buildings
        .iter()
        .filter(|b| b.kind == BuildingKind::Platform)
        .flat_map(|b| {
            let f = b.footprint;
            [
                f.min,
                f.max,
                Vec2::new(f.min.x, f.max.y),
                Vec2::new(f.max.x, f.min.y),
            ]
        })
        .map(|c| (c - back).dot(out))
        .fold(None, |span: Option<(f32, f32)>, x| {
            Some(span.map_or((x, x), |(lo, hi)| (lo.min(x), hi.max(x))))
        })
        .unwrap_or_else(|| {
            let middle = back.distance(tip) / 2.0;
            (middle, middle)
        });

    let consist = consist_length(&branch_consist());
    let gap = RailBay::SPACING * (beside + 1) as f32;
    let radius = RailBay::TURNOUT_RADIUS;
    // Dos arcos de radio `radius` llevan la vía muerta hasta la vía.
    let angle = (1.0 - gap / (2.0 * radius)).clamp(-1.0, 1.0).acos();
    let buffer = platform.0;
    let held = RailBay::BUFFER_GAP + consist;
    // El desvío empieza en el otro extremo del andén, pasando el frente de la
    // lanzadera (si el andén es demasiado corto, la lanzadera lo excede).
    let turnout = platform.1.max(buffer + held + RailBay::FRONT_CLEAR);
    let stop = ((platform.0 + platform.1) / 2.0).max(buffer + held - consist / 2.0);
    if turnout + 2.0 * radius * angle.sin() + RailBay::RUN_OUT_LEFT > back.distance(tip) {
        return None;
    }
    let pieces = (angle / RailLine::MAX_TURN)
        .max(radius * angle / RailLine::CURVE_STEP)
        .ceil()
        .max(1.0) as usize;
    let mut path = vec![at(buffer, gap), at(turnout, gap)];
    // Primer arco: sale de la vía muerta recta hacia la vía.
    let (sin, cos) = (angle.sin(), angle.cos());
    for k in 1..=pieces {
        let phi = angle * k as f32 / pieces as f32;
        path.push(at(
            turnout + radius * phi.sin(),
            gap - radius * (1.0 - phi.cos()),
        ));
    }
    // Segundo arco: vuelve a la orientación de la vía.
    let (mid_x, mid_y) = (turnout + radius * sin, gap - radius * (1.0 - cos));
    for k in 1..=pieces {
        let psi = angle * k as f32 / pieces as f32;
        let heading = angle - psi;
        path.push(at(
            mid_x + radius * (sin - heading.sin()),
            mid_y - radius * (heading.cos() - cos),
        ));
    }
    // El primer arco gira hacia la vía y el segundo, de vuelta.
    let toward = turn(out, side * -1.0).signum();
    let tangent = radius * (angle / 2.0).tan();
    let curve = |sign: f32| RailCurve {
        radius,
        tangent,
        turn: sign * toward * angle,
    };
    let end = *path.last()?;
    // Rectángulos alineados con los ejes a partir de esquinas del sistema de
    // referencia (las vías de los pueblos corren a lo largo de un eje): `x` a
    // lo largo de la vía, `y` hacia afuera de ella.
    let area = |x: (f32, f32), y: (f32, f32)| {
        let (p, q) = (at(x.0, y.0), at(x.1, y.1));
        Aabb::new(
            Vec2::new(p.x.min(q.x), p.y.min(q.y)),
            Vec2::new(p.x.max(q.x), p.y.max(q.y)),
        )
    };
    let axis = |v: Vec2| {
        if v.x.abs() > v.y.abs() {
            Vec2::new(1.0, 0.0)
        } else {
            Vec2::new(0.0, 1.0)
        }
    };
    let edge = gap + RailBay::PLATFORM_EDGE;
    Some(RailBay {
        branch,
        town: first.from,
        link: RailLink {
            from: first.from,
            to: first.from,
            track: Railway {
                start: path[0],
                end,
                gauge: rail.gauge,
            },
            path,
            curves: vec![curve(1.0), curve(-1.0)],
        },
        stop: at(stop, gap),
        platform: Floor {
            footprint: area((buffer, turnout), (edge, edge + RailBay::PLATFORM_DEPTH)),
            height: Floor::PLATFORM_HEIGHT,
            along: axis(out),
        },
        walks: Vec::new(),
    })
}

/// Los senderos de `bay` (ver [`RailLine::lay_walks`]): primero el del andén de
/// la estación y después el del andén de la vía muerta. `None` si no hay un
/// paso a nivel sobre la vía de la estación pasando el desvío, o cuando un
/// sendero no encuentra ningún camino.
fn walks(bay: &RailBay, town: &TownLayout, roads: &RoadNetwork) -> Option<Vec<Aabb>> {
    let station = *town.floors().first()?;
    let out = bay.heading();
    let join = *bay.link.path.last()?;
    // Pasando la unión de la vía muerta, hacia afuera a lo largo de la vía de
    // la estación, sobre ella.
    let crossing = roads
        .crossings()
        .filter(|c| {
            let rel = c.at - join;
            let off = rel.x * out.y - rel.y * out.x;
            off.abs() < 0.5 && rel.dot(out) > RailBay::WALK_INTO_ROAD
        })
        .min_by(|a, b| (a.at - join).dot(out).total_cmp(&(b.at - join).dot(out)))?;
    let reach = |floor: Floor, track: Vec2| {
        let (along, across) = (floor.along, floor.across());
        // El extremo del andén hacia la punta, y una rampa más allá.
        let sign = along.dot(out).signum();
        let (lo, hi) = (
            floor.footprint.min.dot(along),
            floor.footprint.max.dot(along),
        );
        let start = (if sign > 0.0 { hi } else { lo }) + sign * Floor::RAMP_LENGTH;
        // El sendero corre 3,5 m hacia adentro del borde lejano del andén
        // (donde estaba el centro de un andén de 7 m, antes de que los andenes
        // crecieran hacia la vía), así que se mantiene lejos de lo que hay
        // junto al paso a nivel.
        let (low, high) = (
            floor.footprint.min.dot(across),
            floor.footprint.max.dot(across),
        );
        let from_track = track.dot(across);
        let line = if (high - from_track).abs() > (low - from_track).abs() {
            high - Floor::WALK_LINE_IN
        } else {
            low + Floor::WALK_LINE_IN
        };
        let at = |a: f32| along * a + across * line;
        let steps = (RailBay::WALK_REACH / 0.25) as usize;
        let end = (0..=steps)
            .map(|k| start + sign * 0.25 * k as f32)
            .find(|&a| {
                roads
                    .near(at(a), RailBay::WALK_INTO_ROAD)
                    .is_some_and(|d| d <= RailBay::WALK_INTO_ROAD)
            })?;
        let (p, q) = (at(start), at(end));
        let half = across * (RailBay::WALK_WIDTH / 2.0);
        let corners = [p - half, p + half, q - half, q + half];
        let (mut min, mut max) = (corners[0], corners[0]);
        for c in corners {
            min = Vec2::new(min.x.min(c.x), min.y.min(c.y));
            max = Vec2::new(max.x.max(c.x), max.y.max(c.y));
        }
        // El camino al final del sendero está junto al paso a nivel.
        (at(end).distance(crossing.at) < RailBay::WALK_NEAR_CROSSING).then(|| Aabb::new(min, max))
    };
    Some(vec![
        reach(station, join)?,
        reach(bay.platform, bay.buffer())?,
    ])
}

/// Si un recorrido, leído como sus `pieces` en orden, vuelve a entrar en `band`
/// después de haber salido: un enlace que sale del terreno de su pueblo y lo
/// vuelve a cruzar más adelante.
fn reenters<'a>(band: Aabb, pieces: impl Iterator<Item = &'a [Vec2]>) -> bool {
    let mut left = false;
    for w in pieces {
        let on = band.segment_overlap(w[0], w[1]).is_some();
        if left && on {
            return true;
        }
        left |= !on;
    }
    false
}

/// La punta de `rail` más cercana a `toward`, y su otra punta.
fn nearest_tip(rail: Railway, toward: Vec2) -> (Vec2, Vec2) {
    if rail.start.distance(toward) <= rail.end.distance(toward) {
        (rail.start, rail.end)
    } else {
        (rail.end, rail.start)
    }
}

/// La línea central de un enlace desde `start`, saliendo en la dirección
/// unitaria `out`, hasta `end`, llegando en la dirección unitaria `into`, y sus
/// curvas.
///
/// El enlace es la polilínea `start → p1 → p2 → end`, donde `p1` está delante
/// de `start` en la dirección `out` y `p2` detrás de `end` en la dirección
/// `into`, con cada esquina redondeada por un arco de radio `radius`. Las
/// esquinas quedan exactamente a una longitud de tangente de las puntas, así
/// que el primer arco empieza en `start` y el segundo termina en `end`: el
/// enlace une ambas vías de los pueblos sin quiebres, huecos ni
/// superposiciones. Las longitudes de tangente se limitan a un cuarto de la
/// distancia entre las puntas, lo que mantiene cada una dentro de la mitad de
/// la recta del medio; una curva limitada recibe el radio que le cabe.
fn smooth_link(
    start: Vec2,
    out: Vec2,
    end: Vec2,
    into: Vec2,
    radius: f32,
) -> (Vec<Vec2>, Vec<RailCurve>) {
    let cap = start.distance(end) / 4.0;
    let corners = |(ta, tb): (f32, f32)| (start + out * ta, end - into * tb);
    // Las longitudes de tangente dependen de la orientación de la recta del
    // medio, que a su vez depende de ellas: se itera hasta un punto fijo
    // (alcanzan unas pocas rondas, ya que las tangentes son cortas comparadas
    // con el enlace).
    let mut tangents = (0.0, 0.0);
    for _ in 0..64 {
        let (p1, p2) = corners(tangents);
        let Some(middle) = unit(p2 - p1) else {
            break;
        };
        let next = (
            tangent_length(radius, turn(out, middle)).min(cap),
            tangent_length(radius, turn(middle, into)).min(cap),
        );
        let settled = (next.0 - tangents.0).abs() < 1e-4 && (next.1 - tangents.1).abs() < 1e-4;
        tangents = next;
        if settled {
            break;
        }
    }
    let (p1, p2) = corners(tangents);
    let middle = unit(p2 - p1).unwrap_or(out);
    let mut path = vec![start];
    let mut curves = Vec::new();
    curves.extend(fillet(&mut path, p1, out, middle, tangents.0));
    curves.extend(fillet(&mut path, p2, middle, into, tangents.1));
    push_point(&mut path, end);
    (path, curves)
}

/// Agrega a `path` el arco que redondea `corner` entre las orientaciones
/// unitarias `before` y `after`, a `tangent` metros a cada lado, en tramos
/// rectos de como mucho [`RailLine::CURVE_STEP`] de largo que giran como mucho
/// [`RailLine::MAX_TURN`] cada uno. Sin giro, no se agrega nada.
fn fillet(
    path: &mut Vec<Vec2>,
    corner: Vec2,
    before: Vec2,
    after: Vec2,
    tangent: f32,
) -> Option<RailCurve> {
    let delta = turn(before, after);
    if delta.abs() < 1e-5 || tangent <= 0.0 {
        return None;
    }
    let radius = tangent / (delta.abs() / 2.0).tan();
    let (from, to) = (corner - before * tangent, corner + after * tangent);
    let left = Vec2::new(-before.y, before.x);
    let center = from + left * (radius * delta.signum());
    let arc = radius * delta.abs();
    let pieces = (delta.abs() / RailLine::MAX_TURN)
        .ceil()
        .max((arc / RailLine::CURVE_STEP).ceil())
        .max(1.0) as usize;
    push_point(path, from);
    for i in 1..pieces {
        let angle = delta * i as f32 / pieces as f32;
        push_point(path, center + rotate(from - center, angle));
    }
    push_point(path, to);
    Some(RailCurve {
        radius,
        tangent,
        turn: delta,
    })
}

/// Agrega `p` salvo que repita el último punto.
fn push_point(path: &mut Vec<Vec2>, p: Vec2) {
    if path
        .last()
        .is_none_or(|&last| last.distance(p) > CLIP_EPSILON)
    {
        path.push(p);
    }
}

/// Distancia desde una esquina que gira `delta` radianes hasta donde un arco de
/// radio `radius` se une con las rectas. Cerca de un giro en U crece sin
/// límite, así que quienes la llaman la limitan.
fn tangent_length(radius: f32, delta: f32) -> f32 {
    // La mitad de un giro en U es π/2, donde tan se dispara (y cambia de signo
    // en f32).
    radius * (delta.abs() / 2.0).min(1.55).tan()
}

/// El ángulo con signo desde el vector unitario `a` hasta el vector unitario
/// `b`, positivo en sentido antihorario.
fn turn(a: Vec2, b: Vec2) -> f32 {
    (a.x * b.y - a.y * b.x).atan2(a.dot(b))
}

/// `v` rotado `angle` radianes en sentido antihorario.
fn rotate(v: Vec2, angle: f32) -> Vec2 {
    let (sin, cos) = angle.sin_cos();
    Vec2::new(v.x * cos - v.y * sin, v.x * sin + v.y * cos)
}

/// `v` escalado a longitud uno, o `None` cuando no tiene longitud.
fn unit(v: Vec2) -> Option<Vec2> {
    let length = v.length();
    (length > 0.0).then(|| v * (1.0 / length))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{
        LineTrain, PlazaPlacement, RailwayConfig, Side, TownConfig, WorldConfig, place_towns,
    };

    fn config(name: &str, x: f32, y: f32, blocks: (usize, usize), rail: bool) -> TownConfig {
        TownConfig {
            name: name.into(),
            origin: Vec2::new(x, y),
            blocks,
            block_size: 40.0,
            street_width: 10.0,
            railway: rail.then_some(RailwayConfig {
                side: Side::North,
                offset: 24.0,
                through: false,
            }),
            plaza: PlazaPlacement::Auto,
            seed: 1,
            police_stations: None,
            density: None,
            townsfolk: None,
        }
    }

    fn place(world: WorldConfig, configs: &[TownConfig]) -> Vec<TownLayout> {
        let (towns, errors) = place_towns(&world, configs);
        assert!(errors.is_empty(), "{errors:?}");
        towns
    }

    /// Los tres pueblos de la línea, tal como los distribuye
    /// `assets/world/towns.ron`, en el orden del archivo.
    fn pampa_line() -> (WorldConfig, Vec<TownLayout>) {
        let world = WorldConfig {
            half_extent: WorldConfig::MAX_HALF_EXTENT,
            ..WorldConfig::default()
        };
        // Monte Nievas, y los demás en sus desplazamientos reales respecto de
        // él (este, norte, en metros), escalados 1:7 y redondeados al metro.
        let nievas = (-2450.0, -1950.0);
        let at = |east: f32, north: f32| {
            (
                (nievas.0 + east / 7.0).round(),
                (nievas.1 + north / 7.0).round(),
            )
        };
        let (castex, pico) = (at(-12_600.0, -5_400.0), at(35_800.0, 22_400.0));
        let towns = place(
            world,
            &[
                config("Monte Nievas", nievas.0, nievas.1, (4, 3), true),
                config("Eduardo Castex", castex.0, castex.1, (5, 4), true),
                config("General Pico", pico.0, pico.1, (8, 8), true),
            ],
        );
        (world, towns)
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|&n| n.to_string()).collect()
    }

    fn line_names(line: &RailLine, towns: &[TownLayout]) -> Vec<String> {
        line.towns.iter().map(|&i| towns[i].name.clone()).collect()
    }

    /// Tres pueblos en una misma fila este-oeste, a 2 km de distancia, el del
    /// medio opcionalmente sin ferrocarril.
    fn row(middle_rail: bool) -> Vec<TownLayout> {
        place(
            WorldConfig::default(),
            &[
                config("Oeste", -2000.0, 0.0, (4, 4), true),
                config("Medio", 0.0, 0.0, (4, 4), middle_rail),
                config("Este", 2000.0, 0.0, (4, 4), true),
            ],
        )
    }

    #[test]
    fn the_pampa_line_connects_the_three_towns_without_warnings() {
        let (world, towns) = pampa_line();
        let list = names(&["Eduardo Castex", "Monte Nievas", "General Pico"]);
        let (line, errors) = RailLine::build(Some(&list), &towns, world.bounds());
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(line_names(&line, &towns), list);
        let pairs: Vec<(usize, usize)> = line.links.iter().map(|l| (l.from, l.to)).collect();
        assert_eq!(pairs, [(1, 0), (0, 2)]);
        // Cada enlace se curva dos veces, así que se dibuja con muchos tramos
        // rectos.
        assert!(line.segments().count() > 2 * 4);
    }

    #[test]
    fn links_join_the_run_out_tips_into_one_continuous_line() {
        let (world, towns) = pampa_line();
        let list = names(&["Eduardo Castex", "Monte Nievas", "General Pico"]);
        let (line, _) = RailLine::build(Some(&list), &towns, world.bounds());
        let rail = |i: usize| towns[i].railway.expect("a railway");
        for link in &line.links {
            let (from, to) = (rail(link.from), rail(link.to));
            assert!(
                [from.start, from.end].contains(&link.track.start),
                "{link:?}"
            );
            assert!([to.start, to.end].contains(&link.track.end), "{link:?}");
            assert_eq!(link.track.gauge, from.gauge);
        }
        // Los tres pueblos están de oeste a este y sus vías corren de este a
        // oeste: cada enlace sale de la punta este y llega a la punta oeste,
        // así que la vía del pueblo del medio continúa la línea.
        let (castex, nievas, pico) = (rail(1), rail(0), rail(2));
        assert_eq!(line.links[0].track.start, castex.end);
        assert_eq!(line.links[0].track.end, nievas.start);
        assert_eq!(line.links[1].track.start, nievas.end);
        assert_eq!(line.links[1].track.end, pico.start);
    }

    #[test]
    fn without_a_list_the_line_runs_west_to_east() {
        let world = WorldConfig::default();
        let towns = place(
            world,
            &[
                config("Este", 2000.0, 0.0, (4, 4), true),
                config("Sin Tren", 0.0, 2500.0, (3, 3), false),
                config("Oeste", -2000.0, 0.0, (4, 4), true),
                config("Medio", 0.0, 0.0, (4, 4), true),
            ],
        );
        let (line, errors) = RailLine::build(None, &towns, world.bounds());
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(line_names(&line, &towns), ["Oeste", "Medio", "Este"]);
        assert_eq!(line.links.len(), 2);
    }

    #[test]
    fn a_link_that_comes_back_across_its_own_towns_band_is_caught() {
        let band = Aabb::new(Vec2::new(0.0, 0.0), Vec2::new(100.0, 100.0));
        let out = [
            Vec2::new(50.0, 50.0),
            Vec2::new(150.0, 50.0),
            Vec2::new(300.0, 50.0),
        ];
        // Salir de la franja y quedarse afuera está bien.
        assert!(!reenters(band, out.windows(2)));
        assert!(!reenters(
            band,
            [Vec2::new(200.0, 0.0), Vec2::new(300.0, 0.0)].windows(2)
        ));
        // Salir y volver a entrar no.
        let back = [
            Vec2::new(50.0, 50.0),
            Vec2::new(150.0, 50.0),
            Vec2::new(150.0, 200.0),
            Vec2::new(50.0, 200.0),
            Vec2::new(50.0, 90.0),
        ];
        assert!(reenters(band, back.windows(2)));
        // La línea distribuida nunca lo hace.
        let (towns, line) = shipped();
        for link in &line.links {
            for (town, from_end) in [(link.from, false), (link.to, true)] {
                let band = towns[town]
                    .leveled()
                    .expanded(TownLayout::RAIL_CORRIDOR_HALF);
                let pieces = link.path.windows(2);
                let again = if from_end {
                    reenters(band, pieces.rev())
                } else {
                    reenters(band, pieces)
                };
                assert!(!again, "{}", towns[town].name);
            }
        }
    }

    #[test]
    fn a_link_across_a_third_town_is_left_out_naming_it() {
        let towns = row(false);
        let bounds = WorldConfig::default().bounds();
        let (line, errors) = RailLine::build(Some(&names(&["Oeste", "Este"])), &towns, bounds);
        assert_eq!(line_names(&line, &towns), ["Oeste", "Este"]);
        assert!(line.links.is_empty(), "{:?}", line.links);
        assert_eq!(
            errors,
            [RailLineError::CrossesTown {
                from: "Oeste".into(),
                to: "Este".into(),
                other: "Medio".into(),
            }]
        );
        let message = errors[0].to_string();
        for name in ["Oeste", "Este", "Medio"] {
            assert!(message.contains(name), "{message}");
        }
        // La línea por defecto cumple la misma regla.
        let (line, errors) = RailLine::build(None, &towns, bounds);
        assert!(line.links.is_empty());
        assert_eq!(errors.len(), 1, "{errors:?}");
    }

    #[test]
    fn unknown_duplicated_and_railway_less_names_warn_and_are_left_out() {
        let towns = row(false);
        let bounds = WorldConfig::default().bounds();
        let list = names(&["Oeste", "Rancul", "Oeste", "Medio"]);
        let (line, errors) = RailLine::build(Some(&list), &towns, bounds);
        assert_eq!(line_names(&line, &towns), ["Oeste"]);
        assert_eq!(
            errors,
            [
                RailLineError::UnknownTown {
                    name: "Rancul".into()
                },
                RailLineError::DuplicateTown {
                    name: "Oeste".into()
                },
                RailLineError::NoRailway {
                    name: "Medio".into()
                },
                RailLineError::NotOnLine {
                    name: "Este".into()
                },
            ]
        );
        for (error, name) in errors.iter().zip(["Rancul", "Oeste", "Medio", "Este"]) {
            assert!(error.to_string().contains(name), "{error}");
        }
    }

    #[test]
    fn a_railway_town_missing_from_the_list_warns() {
        let towns = row(true);
        let bounds = WorldConfig::default().bounds();
        let (line, errors) = RailLine::build(Some(&names(&["Oeste", "Medio"])), &towns, bounds);
        assert_eq!(line_names(&line, &towns), ["Oeste", "Medio"]);
        assert_eq!(line.links.len(), 1);
        assert_eq!(
            errors,
            [RailLineError::NotOnLine {
                name: "Este".into()
            }]
        );
    }

    #[test]
    fn a_link_leaving_the_world_is_left_out() {
        let towns = row(true);
        // Un mundo demasiado chico para los enlaces (los pueblos en sí se
        // ubicaron en uno más grande).
        let bounds = WorldBounds::centered(1200.0);
        let (line, errors) = RailLine::build(None, &towns, bounds);
        assert!(line.links.is_empty());
        assert_eq!(
            errors[0],
            RailLineError::OutsideWorld {
                from: "Oeste".into(),
                to: "Medio".into(),
            }
        );
        assert!(errors[0].to_string().contains("leaves the world"));
    }

    /// La línea tal como la distribuye `assets/world/towns.ron`.
    pub(crate) fn shipped() -> (Vec<TownLayout>, RailLine) {
        let (world, towns) = pampa_line();
        let list = names(&["Eduardo Castex", "Monte Nievas", "General Pico"]);
        let (line, errors) = RailLine::build(Some(&list), &towns, world.bounds());
        assert!(errors.is_empty(), "{errors:?}");
        (towns, line)
    }

    /// El mayor cambio de orientación entre tramos consecutivos de `points`.
    fn sharpest_bend(points: &[Vec2]) -> f32 {
        points
            .windows(3)
            .map(|w| {
                let a = unit(w[1] - w[0]).expect("no zero-length piece");
                let b = unit(w[2] - w[1]).expect("no zero-length piece");
                turn(a, b).abs()
            })
            .fold(0.0, f32::max)
    }

    /// La punta de `rail` distinta de `tip`.
    fn other_tip(rail: Railway, tip: Vec2) -> Vec2 {
        if rail.start == tip {
            rail.end
        } else {
            rail.start
        }
    }

    #[test]
    fn the_line_bends_at_most_two_degrees_from_one_piece_to_the_next() {
        let (towns, line) = shipped();
        let rail = |i: usize| towns[i].railway.expect("a railway");
        for link in &line.links {
            // Entrar a lo largo de la vía del primer pueblo y salir a lo largo
            // de la del siguiente, para que también cuenten las uniones en las
            // puntas.
            let mut ride = vec![other_tip(rail(link.from), link.track.start)];
            ride.extend(&link.path);
            ride.push(other_tip(rail(link.to), link.track.end));
            let sharpest = sharpest_bend(&ride);
            assert!(
                sharpest <= RailLine::MAX_TURN + 1e-4,
                "bends {}° at once",
                sharpest.to_degrees()
            );
            // Sí gira: los enlaces corren en diagonal entre vías este-oeste.
            let turned: f32 = link.curves.iter().map(|c| c.turn.abs()).sum();
            assert!(turned.to_degrees() > 20.0, "{:?}", link.curves);
        }
    }

    #[test]
    fn each_link_path_runs_unbroken_from_tip_to_tip() {
        let (_, line) = shipped();
        for link in &line.links {
            assert_eq!(link.path.first(), Some(&link.track.start));
            assert_eq!(link.path.last(), Some(&link.track.end));
            let pieces: Vec<Railway> = link.segments().collect();
            assert_eq!(pieces.len(), link.path.len() - 1);
            for pair in pieces.windows(2) {
                assert_eq!(pair[0].end, pair[1].start);
            }
            // Los tramos de curva se mantienen cortos; solo las rectas son
            // largas.
            let short = pieces
                .iter()
                .filter(|p| p.start.distance(p.end) <= RailLine::CURVE_STEP + 1e-2)
                .count();
            assert!(short >= pieces.len() - 1, "{short} of {}", pieces.len());
            // Curvar agrega solo un poco a la cuerda recta. A escala 1:7 el
            // enlace Castex–Monte Nievas gira 73° y es un 9,2 % más largo que
            // su cuerda, así que el límite deja un 10 %.
            let chord = link.track.start.distance(link.track.end);
            assert!(
                link.length() > chord && link.length() < chord * 1.10,
                "{} {chord}",
                link.length()
            );
        }
    }

    #[test]
    fn curves_take_the_full_radius_where_the_straights_leave_room() {
        let (_, line) = shipped();
        let mut full = 0;
        for link in &line.links {
            assert_eq!(link.curves.len(), 2, "{:?}", link.curves);
            let chord = link.track.start.distance(link.track.end);
            for curve in &link.curves {
                let expected = curve.radius * (curve.turn.abs() / 2.0).tan();
                assert!((curve.tangent - expected).abs() < 1e-2, "{curve:?}");
                assert!(curve.tangent <= chord / 4.0 + 1e-2, "{curve:?}");
                if curve.tangent < chord / 4.0 - 1e-2 {
                    // Hay lugar de sobra: el radio completo.
                    assert!(
                        (curve.radius - RailLine::CURVE_RADIUS).abs() < 1.0,
                        "{curve:?}"
                    );
                    full += 1;
                } else {
                    // Limitado a un cuarto de la cuerda: más cerrado, pero
                    // sigue siendo una curva amplia. A escala 1:7 el enlace
                    // corto Castex–Monte Nievas gira 73° en curvas de unos
                    // 362 m.
                    assert!(curve.radius < RailLine::CURVE_RADIUS, "{curve:?}");
                    assert!(curve.radius > 300.0, "{curve:?}");
                }
            }
            // Las dos vías de los pueblos corren de este a oeste: el enlace
            // vuelve a girar en sentido contrario al que giró.
            let net = link.curves[0].turn + link.curves[1].turn;
            assert!(net.abs() < 1e-3, "{:?}", link.curves);
        }
        // El enlace largo Monte Nievas–General Pico conserva el radio completo.
        assert_eq!(full, 2, "{:?}", line.links);
    }

    #[test]
    fn a_curve_between_short_straights_is_tightened_to_fit() {
        // Un giro en ángulo recto con 200 m hacia cada lado: un radio de 500 m
        // necesitaría tangentes de 207 m, pero solo cabe un cuarto de la cuerda
        // de 283 m.
        let (start, end) = (Vec2::ZERO, Vec2::new(200.0, 200.0));
        let (path, curves) = smooth_link(
            start,
            Vec2::new(1.0, 0.0),
            end,
            Vec2::new(0.0, 1.0),
            RailLine::CURVE_RADIUS,
        );
        let cap = start.distance(end) / 4.0;
        assert_eq!(curves.len(), 2, "{curves:?}");
        for curve in &curves {
            assert!((curve.tangent - cap).abs() < 1e-2, "{curve:?}");
            assert!(curve.radius < RailLine::CURVE_RADIUS, "{curve:?}");
            let fits = curve.radius * (curve.turn.abs() / 2.0).tan();
            assert!((fits - curve.tangent).abs() < 1e-2, "{curve:?}");
        }
        assert_eq!(path.first(), Some(&start));
        assert_eq!(path.last(), Some(&end));
        let mut ride = vec![Vec2::new(-10.0, 0.0)];
        ride.extend(&path);
        ride.push(Vec2::new(200.0, 210.0));
        assert!(sharpest_bend(&ride) <= RailLine::MAX_TURN + 1e-4);
    }

    #[test]
    fn a_link_already_heading_straight_on_has_no_curves() {
        let (path, curves) = smooth_link(
            Vec2::ZERO,
            Vec2::new(1.0, 0.0),
            Vec2::new(900.0, 0.0),
            Vec2::new(1.0, 0.0),
            RailLine::CURVE_RADIUS,
        );
        assert!(curves.is_empty(), "{curves:?}");
        assert_eq!(path, [Vec2::ZERO, Vec2::new(900.0, 0.0)]);
    }

    #[test]
    fn segments_in_clips_the_links_to_an_area() {
        let (world, towns) = pampa_line();
        let (line, _) = RailLine::build(None, &towns, world.bounds());
        let link = &line.links[1];
        // La recta larga en el medio del enlace, y a qué distancia a lo largo
        // del enlace empieza.
        let len = |p: &Railway| p.start.distance(p.end);
        let (from, piece) = link
            .segments()
            .scan(0.0, |along, p| {
                let at = *along;
                *along += len(&p);
                Some((at, p))
            })
            .max_by(|a, b| len(&a.1).total_cmp(&len(&b.1)))
            .expect("pieces");
        let middle = (piece.start + piece.end) * 0.5;
        let area = Aabb::from_center_size(middle, Vec2::new(200.0, 200.0));
        let parts = line.segments_in(area);
        assert_eq!(parts.len(), 1, "{parts:?}");
        let part = parts[0];
        assert_eq!(part.track, piece);
        assert_eq!(part.from, from);
        assert_eq!(part.total, link.length());
        for p in [part.start(), part.end()] {
            assert!(area.expanded(1e-2).contains_point(p), "{p:?}");
        }
        assert!(part.exit - part.enter > 100.0);
        let half = from + len(&piece) / 2.0;
        assert!(part.enter < half && half < part.exit, "{part:?}");
        // Las partes de dos áreas vecinas se unen extremo con extremo.
        let east = Aabb::new(
            Vec2::new(area.max.x, area.min.y),
            Vec2::new(area.max.x + 200.0, area.max.y),
        );
        let next = line.segments_in(east);
        if let Some(next) = next.first() {
            assert!((next.enter - part.exit).abs() < 1e-2, "{next:?} {part:?}");
        }
        // Un área fuera de la línea no contiene nada.
        let off = Aabb::from_center_size(Vec2::new(0.0, -7000.0), Vec2::new(200.0, 200.0));
        assert!(line.segments_in(off).is_empty());
    }

    #[test]
    fn segments_in_measures_along_the_whole_curved_link() {
        let (world, towns) = pampa_line();
        let (line, _) = RailLine::build(None, &towns, world.bounds());
        let bounds = world.bounds();
        let parts = line.segments_in(Aabb::new(bounds.min, bounds.max));
        let link = &line.links[0];
        let parts = &parts[..link.path.len() - 1];
        assert_eq!(parts[0].enter, 0.0);
        for (part, ends) in parts.iter().zip(link.path.windows(2)) {
            assert_eq!(part.from, part.enter);
            assert_eq!(part.total, link.length());
            assert!(part.start().distance(ends[0]) < 1e-2);
            assert!(part.end().distance(ends[1]) < 1e-2);
        }
        for pair in parts.windows(2) {
            assert_eq!(pair[0].exit, pair[1].enter);
        }
        let last = parts.last().expect("parts");
        assert!((last.exit - link.length()).abs() < 1e-2);
    }

    #[test]
    fn the_full_path_runs_castex_to_pico_through_every_town_track() {
        let (towns, line) = shipped();
        let path = line.full_path(&towns);
        let rail = |i: usize| towns[i].railway.expect("a railway");
        let (castex, nievas, pico) = (rail(1), rail(0), rail(2));
        let points = path.points();
        // Desde la punta del tramo final oeste de Castex hasta la punta del
        // tramo final este de Pico.
        assert_eq!(points[0], castex.start);
        assert_eq!(*points.last().unwrap(), pico.end);
        // Toda la vía recta de Monte Nievas es un tramo del recorrido.
        let i = points.iter().position(|&p| p == nievas.start).unwrap();
        assert_eq!(points[i + 1], nievas.end);
        assert_eq!(line.connected_towns(), [1, 0, 2]);
        // La longitud de arco es la suma de los tramos, y se curva suavemente.
        let sum: f32 = points.windows(2).map(|w| w[0].distance(w[1])).sum();
        assert!((path.length() - sum).abs() < 1e-2);
        assert!(sharpest_bend(points) <= RailLine::MAX_TURN + 1e-4);
        let links: f32 = line.links.iter().map(RailLink::length).sum();
        let tracks = [castex, nievas, pico].map(|r| r.start.distance(r.end));
        assert!((path.length() - links - tracks.iter().sum::<f32>()).abs() < 0.5);
    }

    #[test]
    fn line_paths_are_measured_by_arc_length() {
        let path = LinePath::new(vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(10.0, 5.0),
        ]);
        assert_eq!(path.points().len(), 3, "repeats dropped");
        assert_eq!(path.length(), 15.0);
        assert_eq!(path.at(4.0), Vec2::new(4.0, 0.0));
        assert_eq!(path.at(12.0), Vec2::new(10.0, 2.0));
        assert_eq!(path.at(-3.0), Vec2::new(0.0, 0.0));
        assert_eq!(path.at(99.0), Vec2::new(10.0, 5.0));
        assert_eq!(path.direction_at(3.0), Vec2::new(1.0, 0.0));
        assert_eq!(path.direction_at(11.0), Vec2::new(0.0, 1.0));
        assert_eq!(path.project(Vec2::new(6.0, -2.0)), 6.0);
        assert_eq!(path.project(Vec2::new(14.0, 3.0)), 13.0);
        assert_eq!(LinePath::default().length(), 0.0);
        assert_eq!(LinePath::default().at(1.0), Vec2::ZERO);
    }

    #[test]
    fn a_gap_ends_the_full_path_and_a_lone_town_is_its_track() {
        let towns = row(true);
        let (mut line, _) = RailLine::build(None, &towns, WorldConfig::default().bounds());
        assert_eq!(line.links.len(), 2);
        line.links.remove(1);
        assert_eq!(line.connected_towns(), [0, 1]);
        let path = line.full_path(&towns);
        let medio = towns[1].railway.unwrap();
        assert_eq!(*path.points().last().unwrap(), medio.end);
        line.links.clear();
        let oeste = towns[0].railway.unwrap();
        assert_eq!(line.full_path(&towns).points(), [oeste.start, oeste.end]);
        assert_eq!(RailLine::default().full_path(&towns).length(), 0.0);
    }

    /// La fila de tres pueblos y Norte al noroeste de Medio, para un ramal que
    /// sale de la línea en Medio.
    pub(crate) fn branched() -> (Vec<TownLayout>, Vec<String>, Vec<Vec<String>>) {
        let towns = place(
            WorldConfig::default(),
            &[
                config("Oeste", -2000.0, 0.0, (4, 4), true),
                config("Medio", 0.0, 0.0, (4, 4), true),
                config("Este", 2000.0, 0.0, (4, 4), true),
                config("Norte", -2200.0, 1800.0, (4, 4), true),
            ],
        );
        (
            towns,
            names(&["Oeste", "Medio", "Este"]),
            vec![names(&["Medio", "Norte"])],
        )
    }

    #[test]
    fn a_branch_leaves_the_line_at_its_junction_tip_and_runs_out_to_its_town() {
        let (towns, list, branches) = branched();
        let bounds = WorldConfig::default().bounds();
        let (line, errors) = RailLine::build_with_branches(Some(&list), &branches, &towns, bounds);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(line.towns, [0, 1, 2]);
        assert_eq!(line.branches, [RailBranch { towns: vec![1, 3] }]);
        let pairs: Vec<(usize, usize)> = line.links.iter().map(|l| (l.from, l.to)).collect();
        assert_eq!(pairs, [(0, 1), (1, 2), (1, 3)]);
        // El ramal empieza en la punta oeste de Medio, donde llega la línea
        // desde Oeste, y llega a la punta de Norte más cercana a Medio.
        let (into_medio, branch) = (&line.links[0], &line.links[2]);
        let (medio, norte) = (towns[1].railway.unwrap(), towns[3].railway.unwrap());
        assert_eq!(branch.track.start, medio.start);
        assert_eq!(into_medio.track.end, medio.start);
        assert_eq!(branch.track.end, norte.end);
        // Sin el ramal, Norte es un pueblo con ferrocarril fuera de la línea.
        let (_, errors) = RailLine::build(Some(&list), &towns, bounds);
        assert_eq!(
            errors,
            [RailLineError::NotOnLine {
                name: "Norte".into()
            }]
        );
    }

    #[test]
    fn the_branch_path_runs_outward_unbroken_and_the_train_keeps_to_the_line() {
        let (towns, list, branches) = branched();
        let bounds = WorldConfig::default().bounds();
        let (line, _) = RailLine::build_with_branches(Some(&list), &branches, &towns, bounds);
        let (plain, _) = RailLine::build(Some(&list), &towns, bounds);
        // El recorrido del tren de la línea es solo el de la línea.
        assert_eq!(line.full_path(&towns), plain.full_path(&towns));
        assert_eq!(line.connected_towns(), [0, 1, 2]);
        let paths = line.branch_paths(&towns);
        assert_eq!(paths.len(), 1);
        let points = paths[0].points();
        // Desde la punta lejana (este) de Medio, pasando por su estación y el
        // empalme, hasta la punta lejana (oeste) de Norte, sin curvarse nunca
        // más que la línea.
        let (medio, norte) = (towns[1].railway.unwrap(), towns[3].railway.unwrap());
        assert_eq!(points.first(), Some(&medio.end));
        assert!(points.contains(&medio.start));
        assert_eq!(points.last(), Some(&norte.start));
        assert!(sharpest_bend(points) <= RailLine::MAX_TURN + 1e-4);
        let expected = medio.start.distance(medio.end)
            + line.links[2].length()
            + norte.start.distance(norte.end);
        assert!((paths[0].length() - expected).abs() < 0.1);
        // La línea, el ramal y después la vía muerta del empalme.
        let tracks = line.track_paths(&towns);
        assert_eq!(tracks.len(), 3);
        assert_eq!(tracks[1], paths[0]);
        assert_eq!(tracks[2].points(), line.bays[0].link.path);
        // Se coloca igual siempre.
        let (again, _) = RailLine::build_with_branches(Some(&list), &branches, &towns, bounds);
        assert_eq!(again, line);
    }

    #[test]
    fn bad_branches_warn_and_are_left_out() {
        let (towns, list, _) = branched();
        let bounds = WorldConfig::default().bounds();
        let build = |branches: &[&[&str]]| {
            let branches: Vec<Vec<String>> = branches.iter().map(|b| names(b)).collect();
            RailLine::build_with_branches(Some(&list), &branches, &towns, bounds)
        };
        let bad = |name: &str| RailLineError::BadJunction { name: name.into() };
        let off = RailLineError::NotOnLine {
            name: "Norte".into(),
        };
        // El empalme tiene que estar en la línea.
        let (line, errors) = build(&[&["Norte", "Este"]]);
        assert!(line.branches.is_empty() && line.links.len() == 2);
        assert!(errors.contains(&bad("Norte")), "{errors:?}");
        // Un ramal necesita un pueblo más allá de su empalme, uno que no esté
        // en la línea.
        let (_, errors) = build(&[&["Medio"]]);
        assert_eq!(errors, [bad("Medio"), off.clone()]);
        let (_, errors) = build(&[&["Medio", "Este"]]);
        let duplicate = RailLineError::DuplicateTown {
            name: "Este".into(),
        };
        assert_eq!(errors, [duplicate, bad("Medio"), off.clone()]);
        let (_, errors) = build(&[&["Medio", "Nada"]]);
        let unknown = RailLineError::UnknownTown {
            name: "Nada".into(),
        };
        assert_eq!(errors, [unknown, bad("Medio"), off]);
        assert!(bad("Medio").to_string().contains("branch left out"));
        // Un pueblo está en un solo ramal.
        let (line, errors) = build(&[&["Medio", "Norte"], &["Oeste", "Norte"]]);
        assert_eq!(line.branches.len(), 1);
        assert_eq!(errors.len(), 2, "{errors:?}");
    }

    /// La única vía muerta del mundo con ramal, con la vía de la estación del
    /// empalme.
    fn bayed() -> (Vec<TownLayout>, RailLine, Railway) {
        let (towns, list, branches) = branched();
        let bounds = WorldConfig::default().bounds();
        let (line, errors) = RailLine::build_with_branches(Some(&list), &branches, &towns, bounds);
        assert!(errors.is_empty(), "{errors:?}");
        let track = towns[1].railway.unwrap();
        (towns, line, track)
    }

    /// Distancia desde `p` hasta el segmento `a`-`b`.
    fn gap(p: Vec2, a: Vec2, b: Vec2) -> f32 {
        let d = b - a;
        let t = ((p - a).dot(d) / d.dot(d)).clamp(0.0, 1.0);
        p.distance(a + d * t)
    }

    #[test]
    fn a_junction_town_gets_a_bay_beside_its_station_track_at_the_junction_end() {
        let (towns, line, track) = bayed();
        // Una vía muerta, para el único ramal, en su empalme (Medio).
        assert_eq!(line.bays.len(), 1);
        let bay = line.bay_of(0).expect("a bay");
        assert_eq!((bay.branch, bay.town), (0, 1));
        assert_eq!((bay.link.from, bay.link.to), (1, 1));
        assert_eq!(line.bay_of(1), None);
        let medio = &towns[1];
        // Al lado de la vía, del lado opuesto al pueblo, a 4,5 m.
        let (buffer, heading) = (bay.buffer(), bay.heading());
        // Separación entre vías de trocha ancha: 4,5 m, el ancho de un vagón y
        // medio.
        assert_eq!(RailBay::SPACING, 4.5);
        assert!((gap(buffer, track.start, track.end) - 4.5).abs() < 1e-3);
        assert!(buffer.y > track.start.y, "away from the town (north)");
        assert!(medio.grid().center().y < track.start.y);
        // Sale hacia la punta del empalme (oeste); el extremo cerrado queda al
        // este.
        let tip = line.links[2].track.start;
        assert_eq!(tip, track.start);
        assert!(
            (heading - Vec2::new(-1.0, 0.0)).length() < 1e-3,
            "{heading:?}"
        );
        assert!(buffer.x > tip.x && buffer.x < track.end.x);
        // El paragolpes está a la altura del extremo este del andén de la
        // estación (lejos de la punta del empalme), así que la vía muerta corre
        // a lo largo del andén.
        let platform = medio
            .buildings
            .iter()
            .find(|b| b.kind == BuildingKind::Platform)
            .unwrap();
        assert!((buffer.x - platform.footprint.max.x).abs() < 1e-3);
        // Lugar para la lanzadera: la formación, la separación en el paragolpes
        // y la vía libre antes del desvío.
        let straight = bay
            .link
            .path
            .iter()
            .take_while(|p| (p.y - buffer.y).abs() < 1e-3)
            .last()
            .unwrap();
        let length = buffer.distance(*straight);
        let consist = consist_length(&branch_consist());
        assert!(consist > 30.0 && consist < 50.0, "{consist}");
        assert!(
            length >= consist + 10.0,
            "{length} m for a {consist} m consist"
        );
        // El centro de la formación está en la vía muerta enfrente del centro
        // del andén de la estación, con la cola lejos del paragolpes.
        assert!((bay.stop.y - buffer.y).abs() < 1e-3);
        assert!((bay.stop.x - platform.footprint.center().x).abs() < 1e-3);
        assert!(buffer.x - bay.stop.x - consist / 2.0 >= RailBay::BUFFER_GAP);
        // Lejos de todo edificio y del andén, por el ancho de los vagones.
        let wide = 3.0 / 2.0 + 0.5;
        for b in &medio.buildings {
            let keep = b.footprint.expanded(wide);
            for w in bay.link.path.windows(2) {
                assert!(keep.segment_overlap(w[0], w[1]).is_none(), "{:?}", b.kind);
            }
        }
        // Un pueblo del que no sale ningún ramal no tiene ninguna, y tampoco
        // una línea sin ramales.
        assert!(line.bays.iter().all(|b| b.town == 1));
        let (_, list, _) = branched();
        let (plain, _) = RailLine::build(Some(&list), &towns, WorldConfig::default().bounds());
        assert!(plain.bays.is_empty());
        assert_eq!(plain.track_paths(&towns).len(), 1);
    }

    #[test]
    fn the_bay_has_a_station_built_platform_along_the_standing_shuttle() {
        let (towns, line, track) = bayed();
        let bay = line.bay_of(0).unwrap();
        let (buffer, out) = (bay.buffer(), bay.heading());
        // Del otro lado de las vías, lejos de la vía de la estación.
        let across = Vec2::new(-out.y, out.x);
        let across = across * across.dot(buffer - track.start).signum();
        let platform = bay.platform;
        let station = towns[1].floors()[0];
        // La construcción del andén de la estación: altura, profundidad, su
        // borde a 1,65 m de la vía, del lado de la vía muerta opuesto a la vía
        // de la estación.
        assert_eq!(platform.height, station.height);
        let (lo, hi) = (platform.footprint.min, platform.footprint.max);
        let corners = [lo, hi, Vec2::new(lo.x, hi.y), Vec2::new(hi.x, lo.y)];
        let off = |c: &Vec2| (*c - buffer).dot(across);
        let (near, far) = corners
            .iter()
            .map(off)
            .fold((f32::MAX, f32::MIN), |(n, f), d| (n.min(d), f.max(d)));
        assert!(
            (near - 1.65).abs() < 1e-3 && (far - 11.0).abs() < 1e-3,
            "{near} {far}"
        );
        let depth = |f: &Floor| f.footprint.size().x.min(f.footprint.size().y);
        assert!((depth(&platform) - depth(&station)).abs() < 1e-3);
        assert!((depth(&platform) - 9.35).abs() < 1e-3);
        // El costado de los vagones (a 1,5 m de la línea central) deja 0,15 m
        // hasta él.
        assert!((near - LineTrain::WIDTH / 2.0 - 0.15).abs() < 1e-3);
        // A lo largo de la vía va desde el paragolpes hasta pasar el frente de
        // la formación detenida, que queda entera al lado del andén.
        let consist = consist_length(&branch_consist());
        let along = |c: Vec2| (c - buffer).dot(out);
        let (a, b) = corners
            .iter()
            .map(|c| along(*c))
            .fold((f32::MAX, f32::MIN), |(l, h), d| (l.min(d), h.max(d)));
        assert!(a.abs() < 1e-3);
        assert!(b >= RailBay::BUFFER_GAP + consist);
        assert_eq!(platform.along.dot(out).abs(), 1.0);
        // Al lado de la vía muerta recta: antes de que el desvío la curve.
        assert!(b <= bay.link.path[1].distance(buffer) + 1e-3);
    }

    #[test]
    fn the_bay_platform_stands_opposite_the_station_platform_with_the_shuttle_between() {
        let (towns, line, track) = bayed();
        let bay = line.bay_of(0).unwrap();
        let station = towns[1].floors()[0];
        let platform = bay.platform;
        // El mismo tramo de la vía, de extremo a extremo, 60 m.
        assert_eq!(platform.footprint.min.x, station.footprint.min.x);
        assert_eq!(platform.footprint.max.x, station.footprint.max.x);
        assert_eq!(platform.footprint.size().x, 60.0);
        // Del otro lado de la vía de la estación respecto del andén de la
        // estación.
        assert!(station.footprint.max.y < track.start.y);
        assert!(platform.footprint.min.y > track.start.y);
        // Los coches de la lanzadera detenida quedan completamente enfrente,
        // con el frente y la cola dentro del largo del andén, en su centro.
        let consist = consist_length(&branch_consist());
        let (stop, half) = (bay.stop.x, consist / 2.0);
        assert!(stop - half >= platform.footprint.min.x, "{stop}");
        assert!(stop + half <= platform.footprint.max.x, "{stop}");
        assert!((stop - station.footprint.center().x).abs() < 1e-3);
        // El desvío empieza donde termina el andén, así que la vía muerta corre
        // recta a lo largo de todo el andén.
        let straight = bay.link.path[1];
        assert!((straight.x - platform.footprint.min.x).abs() < 1e-3);
        // Los pisos de la línea son los andenes de las vías muertas.
        assert_eq!(line.floors(), [platform]);
    }

    #[test]
    fn the_bay_joins_the_station_track_by_a_smooth_turnout_within_the_run_out() {
        let (towns, line, track) = bayed();
        let bay = line.bay_of(0).unwrap();
        let path = &bay.link.path;
        assert!(sharpest_bend(path) <= RailLine::MAX_TURN + 1e-4);
        // Dos arcos del radio de 150 m de la vía muerta, que se curvan en
        // sentidos opuestos, sin giro neto.
        let curves = &bay.link.curves;
        assert_eq!(curves.len(), 2);
        assert!(curves.iter().all(|c| (c.radius - 150.0).abs() < 1e-3));
        assert!((curves[0].turn + curves[1].turn).abs() < 1e-5);
        // Termina sobre la vía de la estación, orientada a lo largo de ella,
        // antes de la punta.
        let join = *path.last().unwrap();
        assert!((join.y - track.start.y).abs() < 1e-2, "{join:?}");
        assert!(join.x > track.start.x + 90.0, "within the run-out");
        let last = path[path.len() - 1] - path[path.len() - 2];
        assert!(last.y.abs() / last.length() < 0.02, "{last:?}");
        // El desvío queda más allá del andén, nunca sobre la playa.
        let medio = &towns[1];
        let grid = medio.grid();
        for p in path {
            assert!(p.y > grid.max.y, "{p:?}");
        }
        // El enlace se mide como cualquier otro.
        assert_eq!(bay.link.track.start, bay.buffer());
        assert_eq!(bay.link.track.end, join);
        let length: f32 = path.windows(2).map(|w| w[0].distance(w[1])).sum();
        assert!((bay.link.length() - length).abs() < 1e-3);
    }

    #[test]
    fn the_shuttle_path_runs_unbroken_from_the_buffer_stop_to_the_branch_end() {
        let (towns, line, track) = bayed();
        let bay = line.bay_of(0).unwrap();
        let paths = line.shuttle_paths(&towns);
        assert_eq!(paths.len(), 1);
        let points = paths[0].points();
        let norte = towns[3].railway.unwrap();
        assert_eq!(points.first(), Some(&bay.buffer()));
        assert_eq!(points.last(), Some(&norte.start));
        assert!(sharpest_bend(points) <= RailLine::MAX_TURN + 1e-4);
        // Al salir de la vía muerta toma la vía propia del recorrido del ramal:
        // el tramo final de la vía de la estación al oeste del desvío y después
        // el enlace del ramal.
        let join = *bay.link.path.last().unwrap();
        let on_branch = line.branch_paths(&towns).remove(0);
        let rest = on_branch.length() - on_branch.project(join);
        assert!((paths[0].length() - (bay.link.length() + rest)).abs() < 0.1);
        assert!(points.contains(&track.start));
        let at = paths[0].project(join);
        // Hasta el último tramo del desvío se mantiene fuera de la vía de la
        // estación, así que ningún tren sobre ella comparte los rieles del tren
        // de la línea.
        for k in 0..=200 {
            let d = at * k as f32 / 200.0;
            if d < at - 20.0 {
                let off = gap(paths[0].at(d), track.start, track.end);
                assert!(off > 0.3, "{d} {off}");
            }
        }
        // Se coloca igual siempre; un ramal sin vía muerta conserva su
        // recorrido.
        let (again, _) = RailLine::build_with_branches(
            Some(&names(&["Oeste", "Medio", "Este"])),
            &[names(&["Medio", "Norte"])],
            &towns,
            WorldConfig::default().bounds(),
        );
        assert_eq!(again.shuttle_paths(&towns), paths);
        let mut bare = line.clone();
        bare.bays.clear();
        assert_eq!(bare.shuttle_paths(&towns), bare.branch_paths(&towns));
    }

    #[test]
    fn the_bay_is_track_everything_that_keeps_off_track_sees() {
        let (towns, line, _) = bayed();
        let bay = line.bay_of(0).unwrap();
        // Los tramos de la vía muerta están entre los tramos de la línea.
        let pieces: Vec<Railway> = line.segments().collect();
        for w in bay.link.path.windows(2) {
            assert!(pieces.iter().any(|r| (r.start, r.end) == (w[0], w[1])));
        }
        // Y se recortan al área que los rodea como los de cualquier enlace.
        let area = Aabb::from_center_size(bay.stop, Vec2::new(40.0, 40.0));
        let spans = line.segments_in(area);
        assert!(spans.iter().any(|s| s.total == bay.link.length()));
        // Los caminos la ven como un recorrido de vía propio.
        assert_eq!(
            line.track_paths(&towns).last().unwrap().points(),
            bay.link.path
        );
    }
}
