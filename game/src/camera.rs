//! Cámara de órbita en tercera persona alrededor del jugador.
//!
//! El mouse (o Q/E) gira la cámara en una vuelta completa alrededor del jinete,
//! la rueda hace zoom, y tras un momento sin entrada de cámara la cámara vuelve
//! suavemente detrás de un jinete en movimiento. Los edificios, el techo del
//! andén y los vagones del tren de la línea nunca se interponen entre la cámara
//! y el jinete: la cámara se acerca de inmediato hasta quedar delante de ellos,
//! y se aleja lentamente una vez que la vista se despeja. Los troncos de los
//! árboles también la acercan, pero con suavidad ([`trunk_reach`]): la cámara
//! rodea un tronco suavemente en lugar de saltar hacia él, y mira a través de
//! las hojas de una copa. Las demás cosas delgadas (alambrados, carteles
//! indicadores, faroles) no mueven la cámara: la línea de visión se entrega a
//! render en cada frame ([`SightLine`]), que desvanece las que se interponen.
//!
//! A bordo del tren ([`OnTrain`]), `V` recorre tres vistas ([`RideView`]):
//! sentado junto a una ventanilla dentro del coche (por defecto), mirando hacia
//! adelante en el sentido de la marcha y mirando alrededor (por cualquiera de
//! las ventanillas) con el mouse dentro de [`SEAT_YAW_LIMIT`] y
//! [`SEAT_PITCH_LIMIT`]; la órbita alrededor del coche, desde más lejos
//! ([`RIDE_DISTANCE`]), a la que los vagones del propio tren no acercan; y el
//! tren entero de costado, siguiéndolo lentamente. El HUD muestra la vista
//! ([`HudHint`]). Subir y bajar ubican la cámara de nuevo; cada vez que se
//! ocupa el asiento (la apertura, al subir, cuando entra un viaje saltado, al
//! volver con `V` a la ventanilla) la cabeza vuelve a mirar hacia adelante. La
//! matemática de la órbita vive en [`crate::orbit`].
//!
//! Fuera del tren, `V` alterna en cambio la vista en primera persona desde los
//! ojos del jugador ([`crate::first_person`]): este módulo sigue ubicando la
//! cámara de órbita por debajo (sin hacerla orbitar, y sin línea de visión que
//! desvanecer), y la primera persona mueve la cámara desde allí hasta los ojos.

use bevy::camera::visibility::VisibilitySystems;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::light::SimulationLightSystems;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use pampa_core::{
    Aabb, BuildingKind, ChunkCoord, CoachBody, FloraSite, Ground, LineTrain, PlacedCar,
    SettlementLayout, StructureKind, Terrain, TownLayout, Travel, Vec2 as GroundVec2, World,
};
use pampa_render::character::viewer::world_layers;
use pampa_render::{
    CloseShadows, HudHint, SightLine, WorldFlora, WorldSettlements, WorldTowns, WorldTrains,
    car_transform, world_from_ground,
};

use crate::first_person::{
    BodyTooClose, EyeFollow, FirstPersonLook, ViewBlend, ViewMode, place_first_person, toggle_view,
    turn_to_view,
};
use crate::orbit::{Orbit, OrbitInput, OrbitParams};
use crate::player::{DrivePlayer, GameWorld, Player};
use crate::ride::{OnTrain, Trip, TripPhase};

/// Distancia de órbita que toma la cámara alrededor del coche, en metros: lo
/// bastante lejos para encuadrar el coche y el campo de adelante.
pub const RIDE_DISTANCE: f32 = 16.0;

/// Recorre las vistas del viaje a bordo; alterna la primera persona fuera del
/// tren.
pub const VIEW_KEY: KeyCode = KeyCode::KeyV;

/// Lo máximo que el jugador sentado gira la cabeza respecto de mirar hacia
/// adelante, hacia cada lado, en radianes (unos 120°): más allá de ambas
/// ventanillas, incluida la del andén.
pub const SEAT_YAW_LIMIT: f32 = 2.094;
/// Lo máximo que el jugador sentado mira hacia arriba o hacia abajo, en
/// radianes (unos 40°).
pub const SEAT_PITCH_LIMIT: f32 = 0.698;

/// Cómo muestra la cámara un viaje en tren, que se recorre con [`VIEW_KEY`].
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RideView {
    /// Sentado junto a una ventanilla dentro del coche.
    #[default]
    Window,
    /// Orbitando el coche desde afuera.
    Coach,
    /// El tren entero de costado.
    Train,
}

impl RideView {
    /// La vista siguiente, dando la vuelta.
    pub fn next(self) -> Self {
        match self {
            Self::Window => Self::Coach,
            Self::Coach => Self::Train,
            Self::Train => Self::Window,
        }
    }

    /// La ayuda del HUD para esta vista.
    pub fn hint(self) -> String {
        let name = match self {
            Self::Window => "ventana",
            Self::Coach => "exterior",
            Self::Train => "tren",
        };
        format!("V: vista - {name}")
    }
}

/// Hacia dónde mira el jugador sentado, desde mirar hacia adelante a lo largo
/// del coche: `yaw` gira la cabeza (positivo hacia la izquierda), `pitch` mira
/// hacia arriba (positivo) o hacia abajo.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct SeatLook {
    pub yaw: f32,
    pub pitch: f32,
}

impl SeatLook {
    /// Gira la cabeza con la entrada de cámara de este frame (el mouse, Q/E),
    /// dentro de los límites del asiento.
    pub fn update(&mut self, input: OrbitInput, params: &OrbitParams, dt: f32) {
        let finite = |v: f32| if v.is_finite() { v } else { 0.0 };
        let turn = finite(input.look.0) * params.mouse_sensitivity
            + finite(input.keys).clamp(-1.0, 1.0) * params.key_rate * finite(dt);
        self.yaw = (self.yaw - turn).clamp(-SEAT_YAW_LIMIT, SEAT_YAW_LIMIT);
        self.pitch = (self.pitch - finite(input.look.1) * params.mouse_sensitivity)
            .clamp(-SEAT_PITCH_LIMIT, SEAT_PITCH_LIMIT);
    }
}

/// La cámara sentada junto a la ventanilla ([`CoachBody::window_seat`]) del
/// lado `side` de un coche de longitud `length` detenido en `car`, mirando
/// hacia adelante (el +X del vagón, su sentido de marcha) girada según `look`.
pub fn seat_view(car: &Transform, length: f32, side: f32, look: SeatLook) -> Transform {
    let (along, across, eye) = CoachBody::window_seat(length, side);
    let at = car.transform_point(Vec3::new(along, eye, across));
    // Una cámara mira a lo largo de su -Z: un cuarto de vuelta a la derecha
    // mira hacia el +X del vagón.
    let facing = -std::f32::consts::FRAC_PI_2;
    let turn = Quat::from_rotation_y(facing + look.yaw) * Quat::from_rotation_x(look.pitch);
    Transform::from_translation(at).with_rotation(car.rotation * turn)
}

/// El tren entero visto de costado: un punto a la derecha del centro de la
/// formación, lo bastante alto para ver por encima de ella, y el punto al que
/// mira.
pub fn train_view(cars: &[PlacedCar], terrain: &Terrain) -> Option<(Vec3, Vec3)> {
    let first = cars.first()?;
    let last = cars.last()?;
    let middle = (first.center + last.center) * 0.5;
    let along = first.center - last.center;
    let span = along.length().max(1.0);
    let right = GroundVec2::new(along.y, -along.x) * (1.0 / span);
    let ground = |p: GroundVec2| terrain.height(p.x, p.y);
    let spot = middle + right * (span * 0.55 + 15.0);
    let look = world_from_ground(middle, ground(middle) + 2.5);
    Some((world_from_ground(spot, ground(spot) + 9.0), look))
}

/// Cómo sigue la cámara al jinete.
#[derive(Component, Debug, Clone, Copy)]
pub struct FollowCamera {
    /// Punto sobre los pies del jinete al que mira la cámara.
    pub look_height: f32,
    /// Punto sobre los pies del jugador al que mira la cámara a pie: el pecho
    /// de una persona está más bajo que el de un jinete.
    pub on_foot_look_height: f32,
    /// Tasa exponencial con que el punto observado alcanza su destino (más alta
    /// = más ágil).
    pub smoothing: f32,
    /// Despeje mínimo sobre el suelo (o el andén) bajo la cámara.
    pub ground_clearance: f32,
    /// Más lejos que esto del jinete, la cámara salta detrás de él en lugar de
    /// deslizarse (primer frame, reapariciones).
    pub snap_distance: f32,
}

impl Default for FollowCamera {
    fn default() -> Self {
        Self {
            look_height: 1.6,
            on_foot_look_height: 1.1,
            smoothing: 10.0,
            ground_clearance: 1.0,
            snap_distance: 50.0,
        }
    }
}

/// La órbita de la cámara y su punto observado suavizado; `None` hasta que la
/// cámara se ubica por primera vez detrás del jinete.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CameraRig(pub Option<(Orbit, Vec3)>);

/// Qué tan afuera, a lo largo del brazo desde el jinete hasta su punto de
/// órbita, se ubica la cámara, como fracción del brazo (1 = completamente
/// afuera): el acercamiento suavizado, ver [`ease_reach`].
#[derive(Component, Debug, Clone, Copy)]
pub struct CameraReach(pub f32);

impl Default for CameraReach {
    fn default() -> Self {
        Self(1.0)
    }
}

/// Ajuste de órbita para la cámara del jugador.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct CameraTuning(pub OrbitParams);

/// Desplazamiento de la rueda informado en píxeles (touchpads) por línea de
/// zoom.
const PIXELS_PER_LINE: f32 = 100.0;

/// Algo que la cámara no puede ver ni atravesar: su huella en planta entre las
/// alturas del mundo `bottom` y `top`. Un edificio o un vagón de tren es una
/// columna apoyada en el suelo; el techo del andén es una losa elevada, así que
/// la cámara todavía puede mirar por debajo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Occluder {
    pub footprint: Aabb,
    pub bottom: f32,
    pub top: f32,
}

impl Occluder {
    /// Una columna apoyada en el suelo a la altura `ground`, hasta `top`.
    pub fn column(footprint: Aabb, ground: f32, top: f32) -> Self {
        Self {
            footprint,
            bottom: ground,
            top,
        }
    }

    /// Si el punto del mundo `p` está adentro.
    #[cfg(test)]
    pub fn contains(&self, p: Vec3) -> bool {
        self.footprint.contains_point(GroundVec2::new(p.x, -p.z))
            && p.y > self.bottom
            && p.y < self.top
    }
}

/// Todo lo estático que bloquea la línea de visión de la cámara hacia el jinete
/// (los vagones del tren de la línea se agregan en cada frame, ver
/// [`car_occluders`]).
///
/// Los edificios cuentan como columnas hasta sus techos, así que la cámara
/// todavía puede mirar por encima de un galpón bajo, y el techo del andén como
/// una losa de techo. Los postes del techo y otras cosas delgadas se dejan
/// afuera a propósito: un poste que barre la vista haría que la cámara salte
/// hacia adentro y hacia afuera. Los alambrados y los carteles indicadores, en
/// cambio, se desvanecen (ver [`SightLine`]). Los troncos de los árboles
/// tampoco son columnas aquí: vienen y van con los chunks y acercan la cámara
/// con suavidad (ver [`trunk_reach`]).
#[derive(Resource, Debug, Clone, Default)]
pub struct CameraOccluders(pub Vec<Occluder>);

/// Altura extra sobre las paredes de un edificio para techos, parapetos y
/// torres.
const ROOF_ALLOWANCE: f32 = 3.0;
/// A qué distancia delante de una pared que bloquea se detiene la cámara, en
/// metros.
const WALL_CLEARANCE: f32 = 0.4;
/// Los vagones más lejos que esto (más la mitad de su longitud) del jinete no
/// pueden interponerse entre él y la cámara, cuyo brazo mide como máximo 25 m.
const CAR_REACH: f32 = 30.0;
/// Constante de tiempo con la que la cámara vuelve a alejarse una vez que la
/// vista se despeja, en segundos. Acercarse no tiene ninguna: es instantáneo
/// (ver [`ease_reach`]).
pub const EASE_OUT: f32 = 0.35;

impl CameraOccluders {
    /// Cada edificio de cada pueblo, apoyado sobre `terrain`.
    pub fn from_towns(towns: &[TownLayout], terrain: &Terrain) -> Self {
        Self(
            towns
                .iter()
                .flat_map(|town| Self::from_town(town, terrain).0)
                .collect(),
        )
    }

    /// Agrega cada casa y cada galpón de `settlements` como una columna hasta
    /// su techo. Los tanques, las parvas, los corrales y los objetos delgados
    /// se dejan afuera: bajos o delgados, se desvanecen en cambio (ver
    /// [`SightLine`]).
    pub fn with_settlements(mut self, settlements: &SettlementLayout, terrain: &Terrain) -> Self {
        let ground = |p: GroundVec2| terrain.height(p.x, p.y);
        self.0.extend(
            settlements
                .settlements
                .iter()
                .flat_map(|s| s.structures.iter())
                .filter(|s| s.kind.is_house() || s.kind == StructureKind::Barn)
                .map(|s| {
                    let g = ground(s.footprint.center());
                    Occluder::column(s.footprint, g, g + s.height + ROOF_ALLOWANCE)
                }),
        );
        self
    }

    /// Cada edificio de `town` como una columna sobre su collider, apoyada
    /// sobre `terrain`, y la losa del techo del andén. El andén se deja afuera:
    /// es un piso sobre el que se para el jugador, y el despeje de la cámara
    /// sobre el suelo la mantiene por encima.
    pub fn from_town(town: &TownLayout, terrain: &Terrain) -> Self {
        let ground = |p: GroundVec2| terrain.height(p.x, p.y);
        let buildings = town
            .buildings
            .iter()
            .filter(|b| b.kind != BuildingKind::Platform)
            .map(|b| {
                // Su collider: las paredes de un galpón, no sus aleros, bajo
                // los cuales puede pararse el jugador.
                let c = b.collider();
                let g = ground(c.center());
                Occluder::column(c, g, g + b.height + ROOF_ALLOWANCE)
            });
        // El techo cuelga a su altura sobre el terreno nivelado de la estación.
        let canopies = town.canopies().into_iter().map(|c| {
            let g = ground(c.footprint.center());
            Occluder {
                footprint: c.footprint,
                bottom: g + c.bottom(),
                top: g + c.top(),
            }
        });
        Self(buildings.chain(canopies).collect())
    }
}

/// Los vagones de los trenes cerca de `near` (el jinete) como columnas desde el
/// suelo hasta sus techos, una fila de cajas cortas para un vagón en pendiente.
pub fn car_occluders(cars: &[PlacedCar], terrain: &Terrain, near: GroundVec2) -> Vec<Occluder> {
    let ground = |p: GroundVec2| terrain.height(p.x, p.y);
    cars.iter()
        .filter(|car| car.center.distance(near) <= CAR_REACH + car.length / 2.0)
        .flat_map(|car| {
            // El techo sobre el más alto de los dos extremos del vagón.
            let half = car.direction * (car.length / 2.0);
            let rails = ground(car.center - half).max(ground(car.center + half));
            let top = rails + LineTrain::RAIL_TOP + car.kind.height();
            // Cada vagón cubre también la mitad del espacio de enganche en
            // ambos extremos, para que la línea de visión no pueda colarse
            // entre dos vagones y dejar la cámara parada en el hueco.
            let bridged = PlacedCar {
                length: car.length + LineTrain::COUPLING,
                ..*car
            };
            bridged
                .boxes()
                .into_iter()
                .map(move |b| Occluder::column(b, ground(b.center()), top))
        })
        .collect()
}

/// Un tronco de árbol que la cámara mantiene fuera de la línea de visión hacia
/// el jinete.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trunk {
    pub at: GroundVec2,
    pub radius: f32,
    /// Alturas que abarcan el tronco y sus ramas principales, en metros del
    /// mundo. Una línea bajo `bottom` pasa por debajo del pie del tronco (a
    /// través de una elevación), una línea sobre `top` a través de la copa:
    /// ninguna de las dos queda bloqueada.
    pub bottom: f32,
    pub top: f32,
}

/// Qué parte de la altura de un árbol trata la cámara como tronco, hasta donde
/// empieza la masa de la copa. Por encima, la línea de visión pasa a través de
/// las hojas, que son tarjetas con recorte alfa a través de las que se ve al
/// jinete.
const TRUNK_TOP: f32 = 0.6;
/// Una línea a esta distancia (en metros) más allá del costado de un tronco, o
/// por encima de su punta, no se ve afectada; desde allí, el acercamiento crece
/// suavemente hasta la detención total, para que un tronco que barre la vista
/// mueva la cámara de forma gradual en lugar de hacerla saltar.
const TRUNK_SOFT: f32 = 0.8;
const TRUNK_TOP_SOFT: f32 = 1.2;
/// Un tronco a esta distancia (en metros a lo largo de la línea) del jinete o
/// más cerca se ignora, y cuenta por completo desde
/// `TRUNK_NEAR + TRUNK_NEAR_BLEND`: un árbol junto al jinete no debe estampar
/// la cámara contra él.
const TRUNK_NEAR: f32 = 1.5;
const TRUNK_NEAR_BLEND: f32 = 1.5;

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Qué tan lejos a lo largo de la línea desde `focus` hasta `wanted` los
/// troncos dejan despejada la línea de visión, como fracción de ella: 1 si
/// ninguno estorba; si no, un punto justo delante del primer tronco.
///
/// A diferencia de una pared ([`clear_reach`]), el resultado es continuo
/// respecto de dónde están los troncos: un tronco bloquea por completo solo
/// sobre la línea misma, y su efecto se desvanece con la distancia de la línea
/// a él, con su altura por encima de su punta o por debajo de su pie, y con su
/// cercanía al jinete. Por eso, cabalgar junto a una fila de árboles nunca hace
/// saltar la cámara. Las copas no bloquean en absoluto: acercarse bajo cada
/// copa de un monte mantendría la cámara sobre el hombro del jinete.
pub fn trunk_reach(focus: Vec3, wanted: Vec3, trunks: &[Trunk]) -> f32 {
    let (from, to) = (
        GroundVec2::new(focus.x, -focus.z),
        GroundVec2::new(wanted.x, -wanted.z),
    );
    let length = from.distance(to);
    if length <= f32::EPSILON {
        return 1.0;
    }
    let along = (to - from) * (1.0 / length);
    let mut reach = 1.0_f32;
    for t in trunks {
        // El punto de la línea más cercano al tronco, y a qué distancia pasa.
        let at = (t.at - from).dot(along).clamp(0.0, length);
        let d = t.at.distance(from + along * at);
        let entry = at - (t.radius * t.radius - d * d).max(0.0).sqrt();
        let stop = ((entry - WALL_CLEARANCE) / length).max(0.0);
        if stop >= 1.0 {
            continue;
        }
        let height = focus.y + (wanted.y - focus.y) * at / length;
        let slack = smooth((d - t.radius) / TRUNK_SOFT)
            .max(smooth((height - t.top) / TRUNK_TOP_SOFT))
            .max(smooth((t.bottom - height) / TRUNK_TOP_SOFT))
            .max(1.0 - smooth((at - TRUNK_NEAR) / TRUNK_NEAR_BLEND));
        reach = reach.min(stop + (1.0 - stop) * slack);
    }
    reach
}

/// Los troncos cerca de la línea de visión de la cámara, de los propios árboles
/// del campo (los mismos que planta [`FloraSite`], para que un árbol esté donde
/// se dibuja), guardados por chunk mientras el jinete está cerca. Los
/// obstáculos cargados por streaming contienen solo huellas en planta, y
/// mezclan carteles indicadores y faroles, que en cambio se desvanecen.
#[derive(Default)]
pub struct TrunkCache(std::collections::HashMap<ChunkCoord, Vec<Trunk>>);

impl TrunkCache {
    /// Los troncos al alcance del segmento de `a` a `b`.
    pub fn along(&mut self, flora: &FloraSite, world: &World, a: Vec3, b: Vec3) -> Vec<Trunk> {
        let config = world.config;
        let (lo, hi) = segment_box(a, b);
        let (first, last) = (config.chunk_of(lo), config.chunk_of(hi));
        let here = config.chunk_of(GroundVec2::new(a.x, -a.z));
        self.0
            .retain(|c, _| (c.x - here.x).abs().max((c.y - here.y).abs()) <= 2);
        let mut found = Vec::new();
        for y in first.y..=last.y {
            for x in first.x..=last.x {
                let coord = ChunkCoord::new(x, y);
                if !config.has_chunk(coord) {
                    continue;
                }
                let trunks = self.0.entry(coord).or_insert_with(|| {
                    flora
                        .trees(coord)
                        .iter()
                        .map(|t| {
                            let ground = world.terrain.height(t.at.x, t.at.y);
                            Trunk {
                                at: t.at,
                                radius: t.trunk_radius(),
                                bottom: ground - 0.5,
                                top: ground + TRUNK_TOP * t.height,
                            }
                        })
                        .collect()
                });
                found.extend(trunks.iter().filter(|t| {
                    (lo.x..=hi.x).contains(&t.at.x) && (lo.y..=hi.y).contains(&t.at.y)
                }));
            }
        }
        found
    }
}

/// Hasta qué altura un árbol de pueblo cuenta como tronco, en metros sobre el
/// suelo. Core no conoce la altura de ningún árbol de pueblo (render los
/// escala), así que esto es un sustituto fijo: la copa de un árbol de calle
/// podado empieza por encima de 3.5 m, y una línea por encima pasa entre las
/// hojas como cualquier otra.
const TOWN_TREE_TRUNK_TOP: f32 = 3.5;

/// Los troncos de los árboles que nunca vienen y van con un chunk: los anillos,
/// las isletas y los árboles de calle de los pueblos, y los eucaliptos y álamos
/// de las estancias. Cada uno es sólido, así que render nunca lo desvanece y la
/// cámara debe rodearlo suavemente. Se construyen una sola vez a partir de los
/// trazados (sus posiciones y radios son los que usa la colisión) y se guardan
/// por chunk, para que un frame solo mire los chunks que cruza la línea de
/// visión.
#[derive(Debug, Default)]
pub struct StaticTrunks(std::collections::HashMap<ChunkCoord, Vec<Trunk>>);

impl StaticTrunks {
    /// Cada árbol de pueblo y de asentamiento, apoyado sobre `terrain`.
    pub fn build(
        config: pampa_core::WorldConfig,
        terrain: &Terrain,
        towns: &[TownLayout],
        settlements: &SettlementLayout,
    ) -> Self {
        let mut cache = Self::default();
        let mut add = |at: GroundVec2, radius: f32, height: f32| {
            let ground = terrain.height(at.x, at.y);
            cache.0.entry(config.chunk_of(at)).or_default().push(Trunk {
                at,
                radius,
                bottom: ground - 0.5,
                top: ground + height,
            });
        };
        for town in towns {
            let ring = town.trees.iter().copied();
            let shade = town.shade_trees.iter().map(|t| t.at);
            for at in ring.chain(shade) {
                add(at, pampa_core::town::TREE_TRUNK_RADIUS, TOWN_TREE_TRUNK_TOP);
            }
        }
        for tree in settlements
            .settlements
            .iter()
            .flat_map(|s| s.structures.iter())
            .filter(|s| s.kind.is_tree())
        {
            add(
                tree.footprint.center(),
                tree.trunk_radius(),
                TRUNK_TOP * tree.height,
            );
        }
        cache
    }

    /// Los troncos al alcance del segmento de `a` a `b`.
    pub fn along(&self, config: pampa_core::WorldConfig, a: Vec3, b: Vec3) -> Vec<Trunk> {
        let (lo, hi) = segment_box(a, b);
        let (first, last) = (config.chunk_of(lo), config.chunk_of(hi));
        let mut found = Vec::new();
        for y in first.y..=last.y {
            for x in first.x..=last.x {
                if let Some(trunks) = self.0.get(&ChunkCoord::new(x, y)) {
                    found.extend(trunks.iter().filter(|t| {
                        (lo.x..=hi.x).contains(&t.at.x) && (lo.y..=hi.y).contains(&t.at.y)
                    }));
                }
            }
        }
        found
    }
}

/// La caja en el suelo alrededor del segmento de `a` a `b`, ampliada según el
/// alcance del borde suave de un tronco.
fn segment_box(a: Vec3, b: Vec3) -> (GroundVec2, GroundVec2) {
    let (a, b) = (GroundVec2::new(a.x, -a.z), GroundVec2::new(b.x, -b.z));
    let pad = TRUNK_SOFT + 1.0;
    (
        GroundVec2::new(a.x.min(b.x) - pad, a.y.min(b.y) - pad),
        GroundVec2::new(a.x.max(b.x) + pad, a.y.max(b.y) + pad),
    )
}

/// El [`CameraReach`] del frame siguiente: `allowed` (la parte despejada del
/// brazo) de inmediato si la cámara está más afuera, para que nunca quede
/// dentro de un oclusor, incluso mientras se suaviza; si no, se aleja
/// suavemente hacia él con la constante de tiempo [`EASE_OUT`]. Una
/// aproximación exponencial, así que el mismo tiempo transcurrido en más
/// frames, más cortos, termina en el mismo punto.
///
/// No hay suavizado al acercarse: cualquier suavizado al acercarse dejaría la
/// cámara detrás de la pared delante de la cual se está acercando durante unos
/// frames.
pub fn ease_reach(current: f32, allowed: f32, dt: f32) -> f32 {
    if allowed <= current {
        return allowed;
    }
    allowed - (allowed - current) * (-dt / EASE_OUT).exp()
}

/// La posición de la cámara sobre la línea desde `focus` hasta `wanted` que
/// mantiene despejada la línea de visión: el propio `wanted` si nada la
/// bloquea; si no, un punto justo delante del primer oclusor.
#[cfg(test)]
pub fn pull_in<'a>(
    focus: Vec3,
    wanted: Vec3,
    occluders: impl IntoIterator<Item = &'a Occluder>,
) -> Vec3 {
    let reach = clear_reach(focus, wanted, occluders);
    if reach >= 1.0 {
        wanted
    } else {
        focus.lerp(wanted, reach)
    }
}

/// Qué tan lejos a lo largo de la línea desde `focus` hasta `wanted` la línea
/// de visión sigue despejada, como fracción de ella: 1 si nada la bloquea; si
/// no, justo delante del primer oclusor.
pub fn clear_reach<'a>(
    focus: Vec3,
    wanted: Vec3,
    occluders: impl IntoIterator<Item = &'a Occluder>,
) -> f32 {
    let (from, to) = (
        GroundVec2::new(focus.x, -focus.z),
        GroundVec2::new(wanted.x, -wanted.z),
    );
    let mut first = 1.0_f32;
    for o in occluders {
        let Some((enter, exit)) = o.footprint.segment_overlap(from, to) else {
            continue;
        };
        // La línea está dentro del oclusor donde pasa sobre su huella en planta
        // y entre sus alturas inferior y superior.
        let Some((low, high)) = height_band(focus.y, wanted.y, o.bottom, o.top) else {
            continue;
        };
        let (start, end) = (enter.max(low), exit.min(high));
        if start <= end {
            first = first.min(start);
        }
    }
    if first >= 1.0 {
        return 1.0;
    }
    let length = from.distance(to);
    let back_off = if length > 0.0 {
        WALL_CLEARANCE / length
    } else {
        0.0
    };
    (first - back_off).max(0.0)
}

/// Dónde una línea cuya altura va linealmente de `from` (en 0) a `to` (en 1)
/// está estrictamente entre `bottom` y `top`, como un rango de parámetros (sin
/// límites para una línea nivelada); `None` si nunca lo está.
fn height_band(from: f32, to: f32, bottom: f32, top: f32) -> Option<(f32, f32)> {
    let rise = to - from;
    if rise == 0.0 {
        return (from > bottom && from < top).then_some((f32::NEG_INFINITY, f32::INFINITY));
    }
    let (a, b) = ((bottom - from) / rise, (top - from) / rise);
    Some((a.min(b), a.max(b)))
}

pub struct FollowCameraPlugin;

impl Plugin for FollowCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraOccluders>()
            .init_resource::<CameraTuning>()
            .init_resource::<CameraInput>()
            .add_systems(Startup, spawn_camera)
            .add_systems(Update, grab_cursor)
            .init_resource::<RideView>()
            .init_resource::<SeatLook>()
            .init_resource::<HudHint>()
            .init_resource::<ViewMode>()
            .init_resource::<FirstPersonLook>()
            .init_resource::<ViewBlend>()
            .init_resource::<EyeFollow>()
            .init_resource::<BodyTooClose>()
            .init_resource::<CloseShadows>()
            .add_systems(
                Update,
                (
                    gather_camera_input,
                    cycle_view,
                    toggle_view,
                    turn_to_view,
                    follow_player,
                )
                    .chain()
                    .after(DrivePlayer),
            )
            .add_systems(
                PostUpdate,
                place_first_person
                    .after(TransformSystems::Propagate)
                    .before(VisibilitySystems::UpdateFrusta)
                    .before(SimulationLightSystems::AssignLightsToClusters)
                    .before(SimulationLightSystems::UpdateDirectionalLightCascades),
            );
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Name::new("Follow camera"),
        Camera3d::default(),
        Transform::from_xyz(0.0, 4.0, 9.0).looking_at(Vec3::Y, Vec3::Y),
        FollowCamera::default(),
        CameraRig::default(),
        CameraReach::default(),
        // También la cabeza propia del jugador, hasta que la primera persona la
        // descarta.
        world_layers(),
    ));
}

/// Bloquea y oculta el cursor mientras la ventana tiene el foco, para que el
/// mouse orbite la cámara; lo libera cuando el jugador cambia de ventana o hay
/// un menú abierto.
fn grab_cursor(
    mut windows: Query<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
    menu: Option<Res<crate::menu::Menu>>,
) {
    let in_menu = menu.is_some_and(|m| m.screen.is_some());
    for (window, mut cursor) in &mut windows {
        let (grab_mode, visible) = if window.focused && !in_menu {
            (CursorGrabMode::Locked, false)
        } else {
            (CursorGrabMode::None, true)
        };
        if cursor.grab_mode != grab_mode || cursor.visible != visible {
            cursor.grab_mode = grab_mode;
            cursor.visible = visible;
        }
    }
}

/// Traduce los dispositivos de este frame a entrada de cámara: el movimiento
/// del mouse solo mientras el cursor está capturado, Q/E para orbitar, la rueda
/// para el zoom.
pub fn read_camera_input(
    keys: &ButtonInput<KeyCode>,
    mouse_motion: Vec2,
    scroll: Option<&AccumulatedMouseScroll>,
    grabbed: bool,
) -> OrbitInput {
    let look = if grabbed {
        (mouse_motion.x, mouse_motion.y)
    } else {
        (0.0, 0.0)
    };
    let zoom = scroll.map_or(0.0, |s| match s.unit {
        MouseScrollUnit::Line => s.delta.y,
        MouseScrollUnit::Pixel => s.delta.y / PIXELS_PER_LINE,
    });
    OrbitInput {
        look,
        keys: f32::from(u8::from(keys.pressed(KeyCode::KeyE)))
            - f32::from(u8::from(keys.pressed(KeyCode::KeyQ))),
        zoom,
    }
}

/// La entrada de cámara de este frame, reunida a partir de los dispositivos.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct CameraInput(pub OrbitInput);

fn gather_camera_input(
    keys: Res<ButtonInput<KeyCode>>,
    motion: Option<Res<AccumulatedMouseMotion>>,
    scroll: Option<Res<AccumulatedMouseScroll>>,
    cursors: Query<&CursorOptions, With<PrimaryWindow>>,
    mut input: ResMut<CameraInput>,
) {
    let grabbed = cursors.iter().any(|c| c.grab_mode != CursorGrabMode::None);
    input.0 = read_camera_input(
        &keys,
        motion.map_or(Vec2::ZERO, |m| m.delta),
        scroll.as_deref(),
        grabbed,
    );
}

/// `V` a bordo recorre las vistas del viaje; al subir se empieza en la
/// ventanilla. La cabeza sentada vuelve a mirar hacia adelante cada vez que se
/// ocupa el asiento: al subir, cuando entra un viaje saltado, y al volver con
/// `V` a la ventanilla. La ayuda del HUD nombra la vista mientras se está a
/// bordo.
fn cycle_view(
    keys: Res<ButtonInput<KeyCode>>,
    aboard: Query<(), (With<Player>, With<OnTrain>)>,
    trip: Option<Res<Trip>>,
    (mut view, mut look): (ResMut<RideView>, ResMut<SeatLook>),
    mut hint: ResMut<HudHint>,
    mut was: Local<(bool, Option<TripPhase>)>,
) {
    let on = !aboard.is_empty();
    let phase = trip.and_then(|t| t.0);
    if on && !was.0 {
        *view = RideView::Window;
        *look = SeatLook::default();
    }
    // Solo el salto de un viaje saltado en el negro cae en `Arriving`,
    // directamente desde `Fading`; una llegada normal deja la cabeza girada
    // donde está.
    let skipped_in = phase == Some(TripPhase::Arriving) && was.1 == Some(TripPhase::Fading);
    if on && skipped_in {
        *look = SeatLook::default();
    }
    *was = (on, phase);
    if on && keys.just_pressed(VIEW_KEY) {
        *view = view.next();
        if *view == RideView::Window {
            *look = SeatLook::default();
        }
    }
    hint.set_if_neq(HudHint(on.then(|| view.hint())));
}

/// Lo que leen las vistas del viaje.
type RideCamera<'w> = (Res<'w, RideView>, ResMut<'w, SeatLook>, Res<'w, ViewMode>);

#[allow(clippy::too_many_arguments)]
fn follow_player(
    time: Res<Time>,
    world: Res<GameWorld>,
    occluders: Res<CameraOccluders>,
    trains: Option<Res<WorldTrains>>,
    tuning: Res<CameraTuning>,
    input: Res<CameraInput>,
    sight: Option<ResMut<SightLine>>,
    flora: Option<Res<WorldFlora>>,
    (towns, settlements): (Option<Res<WorldTowns>>, Option<Res<WorldSettlements>>),
    player: Single<(&Player, Option<&OnTrain>)>,
    (view, mut look, mode): RideCamera,
    (mut was, mut trunks, mut solid): (
        Local<Option<RideView>>,
        Local<TrunkCache>,
        Local<Option<StaticTrunks>>,
    ),
    camera: Single<(
        &mut Transform,
        &FollowCamera,
        &mut CameraRig,
        &mut CameraReach,
    )>,
) {
    let (player, ride) = player.into_inner();
    let aboard = ride.is_some();
    let rider = &player.rider;
    let (mut transform, follow, mut rig, mut reach) = camera.into_inner();
    // Subir, bajar o una vista nueva: empezar de nuevo detrás del jugador.
    let now = aboard.then_some(*view);
    if now != *was {
        *was = now;
        rig.0 = None;
    }
    let dt = time.delta_secs();
    // El tren en el que viaja el jugador.
    let ridden = ride.and_then(|ride| trains.as_deref()?.0.get(ride.0.train));
    if let (Some(ride), Some(train)) = (ride, ridden) {
        let cars = train.placed_cars();
        match *view {
            RideView::Window => {
                if let Some(car) = cars.get(ride.0.car) {
                    look.update(input.0, &tuning.0, dt);
                    let placed = car_transform(car, &world.0.terrain);
                    *transform = seat_view(&placed, car.length, ride.0.side, *look);
                    if let Some(mut sight) = sight {
                        sight.0 = None;
                    }
                    return;
                }
            }
            RideView::Train => {
                if let Some((spot, at)) = train_view(&cars, &world.0.terrain) {
                    // Siguiéndolo lentamente; los saltos (un salto de viaje)
                    // van directo allí.
                    if rig.0.is_none() || transform.translation.distance(spot) > 150.0 {
                        transform.translation = spot;
                        rig.0 = Some((Orbit::behind(0.0), at));
                    } else {
                        transform.translation.smooth_nudge(&spot, 1.5, dt);
                    }
                    transform.look_at(at, Vec3::Y);
                    if let Some(mut sight) = sight {
                        sight.0 = None;
                    }
                    return;
                }
            }
            RideView::Coach => {}
        }
    }
    let feet = world_from_ground(rider.position, rider.elevation());
    let look_height = match rider.travel() {
        Travel::OnFoot => follow.on_foot_look_height,
        Travel::Riding => follow.look_height,
    };
    let target = feet + Vec3::Y * look_height;
    // Desde los ojos, la entrada de cámara gira la cabeza, no la órbita.
    let first = *mode == ViewMode::FirstPerson;
    let input = if first {
        OrbitInput::default()
    } else {
        input.0
    };

    let (orbit, focus, snapped) = match rig.0.as_mut() {
        Some((orbit, focus)) if focus.distance(target) <= follow.snap_distance => {
            orbit.update(input, rider.heading.radians(), rider.speed, &tuning.0, dt);
            focus.smooth_nudge(&target, follow.smoothing, dt);
            (*orbit, *focus, false)
        }
        // Primer frame o una reaparición lejana: saltar justo detrás del
        // jinete.
        _ => {
            let mut orbit = Orbit::behind(rider.heading.radians());
            if aboard {
                orbit.distance = RIDE_DISTANCE;
            }
            let (orbit, focus) = *rig.0.insert((orbit, target));
            (orbit, focus, true)
        }
    };
    let desired = follow.desired(focus, &orbit, &world.0);
    // Mantener despejada de edificios la línea de visión hasta el propio
    // jinete, y de los vagones de todos los trenes cercanos (pero no del tren
    // en el que viaja el jugador).
    let cars = trains
        .as_deref()
        .filter(|_| !aboard)
        .map(|t| {
            let cars: Vec<_> = t.0.iter().flat_map(LineTrain::placed_cars).collect();
            car_occluders(&cars, &world.0.terrain, rider.position)
        })
        .unwrap_or_default();
    // Y de los troncos de los árboles a su alrededor: la cámara los rodea
    // suavemente.
    let mut trunks = flora
        .as_deref()
        .map(|f| trunks.along(&f.0, &world.0, target, desired))
        .unwrap_or_default();
    // Los árboles de los pueblos y de las estancias son sólidos: la cámara
    // también los rodea suavemente. Sus trazados rara vez cambian, así que los
    // troncos se reúnen una sola vez y se vuelven a reunir cada vez que se
    // agrega o se reemplaza un trazado.
    if towns.as_ref().is_some_and(|t| t.is_changed())
        || settlements.as_ref().is_some_and(|s| s.is_changed())
    {
        *solid = None;
    }
    if towns.is_some() || settlements.is_some() {
        let solid = solid.get_or_insert_with(|| {
            let none = SettlementLayout::default();
            let towns = towns.as_deref().map_or(&[][..], |t| &t.0);
            let settlements = settlements.as_deref().map_or(&none, |s| &s.0);
            StaticTrunks::build(world.0.config, &world.0.terrain, towns, settlements)
        });
        trunks.extend(solid.along(world.0.config, target, desired));
    }
    let allowed = clear_reach(target, desired, occluders.0.iter().chain(&cars))
        .min(trunk_reach(target, desired, &trunks));
    reach.0 = if snapped {
        allowed
    } else {
        ease_reach(reach.0, allowed, dt)
    };
    transform.translation = if reach.0 >= 1.0 {
        desired
    } else {
        target.lerp(desired, reach.0)
    };
    transform.look_at(focus, Vec3::Y);
    // Los alambrados y los carteles que estorban se desvanecen en cambio
    // (render); nada se interpone entre los ojos y el jugador.
    if let Some(mut sight) = sight {
        sight.0 = (!first).then_some((target, transform.translation));
    }
}

impl FollowCamera {
    /// Punto de cámara sin obstrucciones sobre `orbit` alrededor del punto
    /// observado `focus`, mantenido sobre el suelo (pasar el mundo, para que
    /// cuenten los andenes).
    pub fn desired(&self, focus: Vec3, orbit: &Orbit, ground: &(impl Ground + ?Sized)) -> Vec3 {
        let (offset, height) = orbit.offset();
        let mut desired = focus + world_from_ground(offset, height);
        // Evitar que la cámara se hunda en una elevación o en un andén detrás
        // del jinete.
        let below = GroundVec2::new(desired.x, -desired.z);
        let floor = ground.ground_height(below) + self.ground_clearance;
        desired.y = desired.y.max(floor);
        desired
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::{GameWorld, PlayerPlugin, PlayerSpawn};
    use bevy::time::TimeUpdateStrategy;
    use pampa_core::{Heading, World};
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};
    use std::time::Duration;

    const FRAME: f32 = 0.016;

    fn ground(p: Vec3) -> GroundVec2 {
        GroundVec2::new(p.x, -p.z)
    }

    fn headless_app(world: World, spawn: PlayerSpawn, occluders: CameraOccluders) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
                FRAME,
            )))
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(GameWorld(world))
            .insert_resource(spawn)
            .insert_resource(occluders)
            .add_plugins((PlayerPlugin, FollowCameraPlugin));
        app
    }

    fn camera(app: &mut App) -> Transform {
        let mut query = app
            .world_mut()
            .query_filtered::<&Transform, With<FollowCamera>>();
        *query.single(app.world()).unwrap()
    }

    fn rig(app: &mut App) -> (Orbit, Vec3) {
        let mut query = app.world_mut().query::<&CameraRig>();
        query.single(app.world()).unwrap().0.expect("placed")
    }

    fn set_yaw(app: &mut App, yaw: f32) {
        let mut query = app.world_mut().query::<&mut CameraRig>();
        let mut rig = query.single_mut(app.world_mut()).unwrap();
        if let Some((orbit, _)) = rig.0.as_mut() {
            orbit.yaw = yaw;
        }
    }

    fn rider(app: &mut App) -> pampa_core::Rider {
        let mut query = app.world_mut().query::<&Player>();
        query.single(app.world()).unwrap().rider
    }

    fn keys(app: &mut App) -> Mut<'_, ButtonInput<KeyCode>> {
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>()
    }

    fn north_at(x: f32, y: f32) -> PlayerSpawn {
        PlayerSpawn {
            position: GroundVec2::new(x, y),
            heading: Heading::from_radians(FRAC_PI_2),
            ..PlayerSpawn::default()
        }
    }

    #[test]
    fn snaps_behind_the_rider_when_far_away() {
        // La cámara empieza cerca del origen; el jinete está a 160 m.
        let mut app = headless_app(
            World::default(),
            north_at(26.0, -161.0),
            CameraOccluders::default(),
        );
        app.update();
        let r = rider(&mut app);
        let follow = FollowCamera::default();
        let focus = world_from_ground(r.position, r.height) + Vec3::Y * follow.look_height;
        let world = app.world().resource::<GameWorld>().0.clone();
        let behind = Orbit::behind(r.heading.radians());
        let expected = follow.desired(focus, &behind, &world);
        let at = camera(&mut app).translation;
        assert!(at.distance(expected) < 1e-4, "{at} vs {expected}");
        assert!(at.z > focus.z, "south of a rider facing north");
    }

    #[test]
    fn turning_in_place_leaves_the_orbit_alone() {
        let mut app = headless_app(
            World::default(),
            north_at(0.0, 0.0),
            CameraOccluders::default(),
        );
        app.update();
        let before = camera(&mut app).translation;
        keys(&mut app).press(KeyCode::KeyA);
        for _ in 0..100 {
            app.update();
        }
        assert!(rider(&mut app).heading.radians() > FRAC_PI_2 + 1.0);
        let after = camera(&mut app).translation;
        assert!(after.distance(before) < 1e-3, "{before} -> {after}");
    }

    #[test]
    fn mouse_moves_only_a_grabbed_cursor_and_keys_and_wheel_always_work() {
        let mut keys = ButtonInput::<KeyCode>::default();
        let motion = Vec2::new(12.0, -3.0);
        assert_eq!(
            read_camera_input(&keys, motion, None, false),
            OrbitInput::default()
        );
        let input = read_camera_input(&keys, motion, None, true);
        assert_eq!(input.look, (12.0, -3.0));
        keys.press(KeyCode::KeyE);
        assert_eq!(read_camera_input(&keys, Vec2::ZERO, None, false).keys, 1.0);
        keys.press(KeyCode::KeyQ);
        assert_eq!(read_camera_input(&keys, Vec2::ZERO, None, false).keys, 0.0);
        let lines = AccumulatedMouseScroll {
            unit: MouseScrollUnit::Line,
            delta: Vec2::new(0.0, 2.0),
        };
        assert_eq!(
            read_camera_input(&keys, Vec2::ZERO, Some(&lines), false).zoom,
            2.0
        );
        let pixels = AccumulatedMouseScroll {
            unit: MouseScrollUnit::Pixel,
            delta: Vec2::new(0.0, -50.0),
        };
        let zoom = read_camera_input(&keys, Vec2::ZERO, Some(&pixels), false).zoom;
        assert!((zoom + 50.0 / PIXELS_PER_LINE).abs() < 1e-6);
    }

    #[test]
    fn the_orbit_yaw_turns_the_camera_round_the_rider() {
        let mut app = headless_app(
            World::default(),
            north_at(0.0, 0.0),
            CameraOccluders::default(),
        );
        app.update();
        let heading = rider(&mut app).heading;
        for k in 0..8 {
            // Rumbo de la cámara tal que el jinete mira k * 45 grados en
            // sentido antihorario respecto de la dirección de vista de la
            // cámara.
            let yaw = heading.radians() + PI - k as f32 * FRAC_PI_4;
            set_yaw(&mut app, yaw);
            app.update();
            // La cámara mira a lo largo del rumbo en el suelo `yaw + PI`: el
            // rumbo del jinete girado `k` octavos de vuelta.
            let forward = camera(&mut app).forward();
            let camera_yaw = (-forward.z).atan2(forward.x);
            let relative = (heading.radians() - camera_yaw).rem_euclid(std::f32::consts::TAU);
            let eighths = (relative / FRAC_PI_4).round() as usize % 8;
            assert_eq!(eighths, k, "{relative}");
        }
    }

    #[test]
    fn pulls_in_in_front_of_a_building_behind_the_rider() {
        // Una pared alta entre 4 y 7 m al sur de un jinete que mira al norte.
        let wall = Occluder::column(
            Aabb::new(GroundVec2::new(-5.0, -7.0), GroundVec2::new(5.0, -4.0)),
            -10.0,
            50.0,
        );
        let mut app = headless_app(
            World::default(),
            north_at(0.0, 0.0),
            CameraOccluders(vec![wall]),
        );
        app.update();
        let at = ground(camera(&mut app).translation);
        assert!(
            !wall.footprint.contains_point(at),
            "inside the wall at {at:?}"
        );
        assert!(at.y > -4.0 && at.y < 0.0, "between wall and rider: {at:?}");
    }

    /// Un alambrado 6 m al sur de un jinete en el origen que mira al norte,
    /// dibujado como una parte que se desvanece: delgado, así que se desvanece
    /// en lugar de mover la cámara.
    fn fence_app(walls: CameraOccluders) -> (App, Entity) {
        let mut app = headless_app(World::default(), north_at(0.0, 0.0), walls);
        app.init_resource::<Assets<StandardMaterial>>()
            .add_plugins(pampa_render::ThinFadePlugin);
        let material = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let ground = World::default().terrain.height(0.0, -6.0);
        let part = app
            .world_mut()
            .spawn((
                pampa_render::ThinOccluder {
                    a: GroundVec2::new(-4.0, -6.0),
                    b: GroundVec2::new(4.0, -6.0),
                    radius: 0.4,
                    bottom: ground - 0.5,
                    top: ground + 7.0,
                },
                MeshMaterial3d(material),
            ))
            .id();
        (app, part)
    }

    #[test]
    fn a_fence_behind_the_rider_fades_instead_of_pulling_the_camera_in() {
        let (mut app, fence) = fence_app(CameraOccluders::default());
        for _ in 0..3 {
            app.update();
        }
        let r = rider(&mut app);
        let follow = FollowCamera::default();
        let (orbit, focus) = rig(&mut app);
        let world = app.world().resource::<GameWorld>().0.clone();
        let full = follow.desired(focus, &orbit, &world);
        let at = camera(&mut app).translation;
        assert!(at.distance(full) < 1e-3, "pulled in: {at} vs {full}");
        // La cámara está al sur del alambrado, que se interpone en la vista.
        assert!(ground(at).y < -6.0, "{at}");
        assert!(r.position.y > -6.0);
        assert!(app.world().get::<pampa_render::Faded>(fence).is_some());
    }

    #[test]
    fn a_building_still_pulls_the_camera_in_past_a_faded_fence() {
        let wall = Occluder::column(
            Aabb::new(GroundVec2::new(-5.0, -11.0), GroundVec2::new(5.0, -8.0)),
            -10.0,
            50.0,
        );
        let (mut app, fence) = fence_app(CameraOccluders(vec![wall]));
        for _ in 0..3 {
            app.update();
        }
        let at = ground(camera(&mut app).translation);
        assert!(at.y > -8.0 && at.y < 0.0, "in front of the wall: {at:?}");
        assert!(!wall.footprint.contains_point(at));
        // Entre la pared y el jinete, el alambrado sigue estorbando.
        assert!(at.y < -6.0, "{at:?}");
        assert!(app.world().get::<pampa_render::Faded>(fence).is_some());
    }

    /// Un caldén del campo solo: ningún otro árbol a menos de 40 m.
    fn lone_tree(flora: &FloraSite) -> pampa_core::Tree {
        let trees: Vec<pampa_core::Tree> = (-19..1)
            .flat_map(|x| (3..19).map(move |y| ChunkCoord::new(x, y)))
            .flat_map(|c| flora.trees(c))
            .collect();
        *trees
            .iter()
            .find(|t| {
                t.species == pampa_core::Species::Calden
                    && t.height > 6.0
                    && trees
                        .iter()
                        .all(|o| std::ptr::eq(o, *t) || o.at.distance(t.at) > 40.0)
            })
            .expect("a lone caldén")
    }

    fn flora_site() -> FloraSite {
        let world = World::default();
        FloraSite::new(
            world.config,
            &world.terrain,
            &pampa_core::RailLine::default(),
        )
    }

    /// Una app con los árboles del campo, el jinete 6 m al norte de `tree`
    /// mirando al norte (de espaldas a él), así que el árbol queda detrás del
    /// jinete, donde va la cámara.
    fn grove_app(tree: &pampa_core::Tree) -> App {
        let mut app = headless_app(
            World::default(),
            north_at(tree.at.x, tree.at.y + 6.0),
            CameraOccluders::default(),
        );
        app.insert_resource(pampa_render::WorldFlora(flora_site()));
        app
    }

    #[test]
    fn a_tree_behind_the_rider_pulls_the_camera_in_to_clear_its_trunk() {
        let tree = lone_tree(&flora_site());
        let mut app = grove_app(&tree);
        for _ in 0..3 {
            app.update();
        }
        let follow = FollowCamera::default();
        let (orbit, focus) = rig(&mut app);
        let world = app.world().resource::<GameWorld>().0.clone();
        let full = follow.desired(focus, &orbit, &world);
        let at = camera(&mut app).translation;
        // Acercada antes de donde quiere estar, delante del árbol.
        assert!(
            at.distance(focus) < full.distance(focus) - 1.0,
            "{at} {full}"
        );
        assert!(
            ground(at).y > tree.at.y,
            "the camera is past the tree: {at}"
        );
        // La línea desde la cabeza del jinete hasta la cámara esquiva el
        // tronco.
        let clearance = |c: GroundVec2| {
            let (a, b) = (ground(focus), ground(at));
            let t = ((c - a).dot(b - a) / (b - a).dot(b - a)).clamp(0.0, 1.0);
            c.distance(a + (b - a) * t)
        };
        assert!(
            clearance(tree.at) > tree.trunk_radius(),
            "{}",
            clearance(tree.at)
        );
    }

    fn trunk(x: f32, y: f32) -> Trunk {
        Trunk {
            at: GroundVec2::new(x, y),
            radius: 0.4,
            bottom: -0.5,
            top: 4.0,
        }
    }

    #[test]
    fn a_trunk_on_the_line_stops_the_camera_short_of_it_and_a_crown_does_not() {
        // Jinete en el origen mirando al norte, la cámara 10 m al sur de él y
        // 3 m arriba (z es el sur).
        let (head, camera) = (Vec3::new(0.0, 2.0, 0.0), Vec3::new(0.0, 5.0, 10.0));
        assert_eq!(trunk_reach(head, camera, &[]), 1.0);
        // Un tronco 6 m detrás del jinete, sobre la línea: la cámara se detiene
        // delante de él, a 6 m de los 10 m del brazo.
        let on_line = trunk_reach(head, camera, &[trunk(0.0, -6.0)]);
        assert!(on_line < 0.6, "{on_line}");
        // Por encima de la punta de las ramas del tronco, la línea pasa entre
        // las hojas.
        let high = Trunk {
            top: 1.0,
            ..trunk(0.0, -6.0)
        };
        assert_eq!(trunk_reach(head, camera, &[high]), 1.0);
        // Bien al costado, o más allá de la cámara, o detrás del jinete.
        assert_eq!(trunk_reach(head, camera, &[trunk(3.0, -6.0)]), 1.0);
        assert_eq!(trunk_reach(head, camera, &[trunk(0.0, -14.0)]), 1.0);
        assert_eq!(trunk_reach(head, camera, &[trunk(0.0, 6.0)]), 1.0);
    }

    #[test]
    fn a_line_passing_under_a_trunks_base_is_not_blocked_by_it() {
        // La línea a la altura del tronco está a unos 3.8 m de altura. Un
        // tronco sobre una elevación, con su base por encima de eso, se pasa
        // por debajo (a través de la elevación, que le corresponde detener al
        // terreno); uno cuya base está por debajo bloquea.
        let (head, camera) = (Vec3::new(0.0, 2.0, 0.0), Vec3::new(0.0, 5.0, 10.0));
        let on_rise = Trunk {
            bottom: 6.0,
            top: 12.0,
            ..trunk(0.0, -6.0)
        };
        assert_eq!(trunk_reach(head, camera, &[on_rise]), 1.0);
        let standing = Trunk {
            bottom: 1.0,
            ..on_rise
        };
        assert!(trunk_reach(head, camera, &[standing]) < 0.6);
    }

    /// Un jinete 6 m al oeste de `tree` y una cámara 4 m al este de él, 3 m
    /// arriba.
    fn behind_a_tree(tree: GroundVec2, ground: f32) -> (Vec3, Vec3) {
        (
            Vec3::new(tree.x - 6.0, ground + 1.6, -tree.y),
            Vec3::new(tree.x + 4.0, ground + 4.0, -tree.y),
        )
    }

    #[test]
    fn a_plaza_or_street_tree_pulls_the_camera_in_like_a_field_tree() {
        let town = TownLayout::castex_inspired();
        let world = World::with_town(Terrain::default(), &town);
        let solid = StaticTrunks::build(
            world.config,
            &world.terrain,
            std::slice::from_ref(&town),
            &SettlementLayout::default(),
        );
        assert!(!town.trees.is_empty() && !town.shade_trees.is_empty());
        let ring = town.trees[0];
        let shade = town.shade_trees[0].at;
        for tree in [ring, shade] {
            let ground = world.terrain.height(tree.x, tree.y);
            let (head, camera) = behind_a_tree(tree, ground);
            let trunks = solid.along(world.config, head, camera);
            let reach = trunk_reach(head, camera, &trunks);
            assert!(reach < 0.8, "{tree:?}: {reach}");
            // Lejos de todos los árboles, el conjunto está vacío y la vista
            // despejada.
            let far = Vec3::new(head.x, head.y, head.z + 400.0);
            assert!(solid.along(world.config, far, far + Vec3::X).is_empty());
        }
    }

    /// Un asentamiento de rancho cuya única estructura es un eucalipto de 18 m
    /// en `at`.
    fn eucalyptus_at(at: GroundVec2) -> SettlementLayout {
        let spot = Aabb::from_center_size(at, GroundVec2::new(1.0, 1.0));
        let mut settlements = SettlementLayout::default();
        settlements.settlements.push(pampa_core::Settlement {
            kind: pampa_core::SettlementKind::Hut,
            gate: 0,
            hub: spot.center(),
            patio: spot,
            structures: vec![pampa_core::Structure {
                kind: StructureKind::Eucalyptus,
                footprint: spot,
                height: 18.0,
                front: pampa_core::Side::South,
                variant: 0,
            }],
            tracks: Vec::new(),
            bounds: spot,
            seed: 0,
        });
        settlements
    }

    #[test]
    fn an_estancia_tree_pulls_the_camera_in() {
        let settlements = eucalyptus_at(GroundVec2::new(30.0, 40.0));
        let world = World::default();
        let solid = StaticTrunks::build(world.config, &world.terrain, &[], &settlements);
        let tree = settlements
            .settlements
            .iter()
            .flat_map(|s| s.structures.iter())
            .find(|s| s.kind.is_tree())
            .expect("an estancia tree");
        let at = tree.footprint.center();
        let ground = world.terrain.height(at.x, at.y);
        let (head, camera) = behind_a_tree(at, ground);
        let reach = trunk_reach(head, camera, &solid.along(world.config, head, camera));
        assert!(reach < 0.8, "{reach}");
    }

    #[test]
    fn estancias_laid_out_after_the_towns_still_pull_the_camera_in() {
        // Los pueblos están primero; las estancias llegan unos frames después,
        // con un eucalipto entre el jinete y la cámara.
        let tree = GroundVec2::new(30.0, 40.0);
        let mut app = headless_app(
            World::default(),
            north_at(tree.x, tree.y + 6.0),
            CameraOccluders::default(),
        );
        app.insert_resource(WorldTowns(Vec::new()));
        for _ in 0..3 {
            app.update();
        }
        app.insert_resource(WorldSettlements(eucalyptus_at(tree)));
        for _ in 0..3 {
            app.update();
        }
        let follow = FollowCamera::default();
        let (orbit, focus) = rig(&mut app);
        let world = app.world().resource::<GameWorld>().0.clone();
        let full = follow.desired(focus, &orbit, &world);
        let at = camera(&mut app).translation;
        assert!(
            at.distance(focus) < full.distance(focus) - 1.0,
            "{at} {full}"
        );
    }

    #[test]
    fn riding_past_a_row_of_trees_never_makes_the_camera_jump() {
        // Troncos cada 5 m a lo largo de una línea, el jinete cruzándolos de a
        // 0.05 m, la cámara 8 m detrás y 3 m arriba: el alcance acercado cambia
        // un poco en cada paso, nunca de golpe.
        let row: Vec<Trunk> = (0..6)
            .map(|i| trunk(-12.0 + i as f32 * 5.0, -5.0))
            .collect();
        let (mut last, mut worst, mut lowest) = (None::<f32>, 0.0_f32, 1.0_f32);
        let mut x = -15.0;
        while x < 15.0 {
            let (head, camera) = (Vec3::new(x, 2.0, 0.0), Vec3::new(x, 5.0, 8.0));
            let reach = trunk_reach(head, camera, &row);
            if let Some(last) = last {
                worst = worst.max((reach - last).abs());
            }
            (last, lowest) = (Some(reach), lowest.min(reach));
            x += 0.05;
        }
        assert!(lowest < 0.8, "the camera never pulled in: {lowest}");
        assert!(worst < 0.1, "jumped by {worst} in one step");
    }

    #[test]
    fn looks_over_a_low_building() {
        let shed = Occluder::column(
            Aabb::new(GroundVec2::new(-5.0, -7.0), GroundVec2::new(5.0, -4.0)),
            -20.0,
            -10.0,
        );
        let focus = Vec3::new(0.0, 1.6, 0.0);
        let wanted = Vec3::new(0.0, 4.0, 9.0);
        assert_eq!(pull_in(focus, wanted, &[shed]), wanted);
    }

    fn assert_outside<'a>(
        occluders: impl IntoIterator<Item = &'a Occluder>,
        cam: Vec3,
        when: &str,
    ) {
        for o in occluders {
            assert!(!o.contains(cam), "{when}: camera {cam} inside {o:?}");
        }
    }

    fn town_app() -> (App, CameraOccluders, Aabb) {
        let town = TownLayout::castex_inspired();
        let world = World::with_town(Terrain::default(), &town);
        let occluders = CameraOccluders::from_town(&town, &world.terrain);
        // En la calle, de espaldas al frente del almacén de ramos generales,
        // mirando a la plaza.
        let store = town
            .buildings
            .iter()
            .find(|b| b.kind == pampa_core::BuildingKind::GeneralStore)
            .unwrap()
            .footprint;
        let spawn = PlayerSpawn {
            position: GroundVec2::new(store.max.x + 2.5, store.center().y),
            heading: Heading::from_radians(0.0),
            ..PlayerSpawn::default()
        };
        (
            headless_app(world, spawn, occluders.clone()),
            occluders,
            store,
        )
    }

    #[test]
    fn never_ends_a_frame_inside_a_town_building() {
        let (mut app, occluders, _) = town_app();
        keys(&mut app).press(KeyCode::KeyW);
        keys(&mut app).press(KeyCode::ShiftLeft);
        for frame in 0..300 {
            // Zigzag: girar a la izquierda y a la derecha en arcos largos por
            // las calles, y orbitar la cámara durante un rato.
            let mut k = keys(&mut app);
            let (on, off) = if (frame / 40) % 2 == 0 {
                (KeyCode::KeyA, KeyCode::KeyD)
            } else {
                (KeyCode::KeyD, KeyCode::KeyA)
            };
            k.release(off);
            k.press(on);
            if (frame / 100) % 3 == 1 {
                k.press(KeyCode::KeyE);
            } else {
                k.release(KeyCode::KeyE);
            }
            app.update();
            assert_outside(
                &occluders.0,
                camera(&mut app).translation,
                &format!("frame {frame}"),
            );
        }
    }

    #[test]
    fn never_ends_inside_a_building_at_any_orbit_angle() {
        let (mut app, occluders, _) = town_app();
        app.update();
        let mut pulled = 0;
        for step in 0..72 {
            let yaw = step as f32 * 5f32.to_radians();
            set_yaw(&mut app, yaw);
            app.update();
            let cam = camera(&mut app).translation;
            assert_outside(&occluders.0, cam, &format!("yaw {yaw}"));
            let (orbit, focus) = rig(&mut app);
            let world = app.world().resource::<GameWorld>().0.clone();
            let desired = FollowCamera::default().desired(focus, &orbit, &world);
            if pull_in(focus, desired, &occluders.0) != desired {
                pulled += 1;
            }
        }
        assert!(pulled > 0, "some angles look through the store");
    }

    #[test]
    fn walking_into_a_galpon_keeps_the_camera_off_the_player() {
        // Un galpón colisiona en sus paredes, bajo sus aleros
        // (`Building::collider` del core): caminar hasta su pared pone al
        // jugador dentro de su huella en planta, donde la cámara no debe
        // plegarse sobre él.
        let town = TownLayout::castex_inspired();
        let world = World::with_town(Terrain::default(), &town);
        let occluders = CameraOccluders::from_town(&town, &world.terrain);
        let shed = town
            .buildings
            .iter()
            .find(|b| {
                matches!(
                    b.kind,
                    pampa_core::BuildingKind::Shed | pampa_core::BuildingKind::GrainStore
                )
            })
            .copied()
            .expect("Castex has a galpón");
        // Del lado de su calle, caminando directo hacia su pared frontal.
        let (f, c) = (shed.footprint, shed.footprint.center());
        let (position, heading) = match shed.front {
            pampa_core::Side::South => (GroundVec2::new(c.x, f.min.y - 4.0), FRAC_PI_2),
            pampa_core::Side::North => (GroundVec2::new(c.x, f.max.y + 4.0), -FRAC_PI_2),
            pampa_core::Side::West => (GroundVec2::new(f.min.x - 4.0, c.y), 0.0),
            pampa_core::Side::East => (GroundVec2::new(f.max.x + 4.0, c.y), PI),
        };
        let spawn = PlayerSpawn {
            position,
            heading: Heading::from_radians(heading),
            travel: pampa_core::Travel::OnFoot,
        };
        let colliders = world_colliders(&town);
        let mut app = headless_app(world, spawn, occluders.clone());
        keys(&mut app).press(KeyCode::KeyW);
        for frame in 0..400 {
            app.update();
            let cam = camera(&mut app).translation;
            let (_, head) = rig(&mut app);
            assert!(
                cam.distance(head) >= 1.2,
                "frame {frame}: camera {cam} on the player's head {head}"
            );
            assert_outside(&occluders.0, cam, &format!("frame {frame}"));
            for c in &colliders {
                assert!(
                    !c.contains_point(ground(cam)),
                    "frame {frame}: camera in {c:?}"
                );
            }
        }
        // Llegó: contra la pared, bajo los aleros.
        let r = rider(&mut app);
        assert!(
            f.contains_point(r.position),
            "{:?} outside {f:?}",
            r.position
        );
    }

    /// El collider de cada edificio de `town`.
    fn world_colliders(town: &TownLayout) -> Vec<Aabb> {
        town.buildings
            .iter()
            .filter(|b| b.kind != pampa_core::BuildingKind::Platform)
            .map(|b| b.collider())
            .collect()
    }

    #[test]
    fn on_foot_the_camera_looks_at_a_lower_point() {
        let mut heights = Vec::new();
        for travel in [pampa_core::Travel::Riding, pampa_core::Travel::OnFoot] {
            let spawn = PlayerSpawn {
                travel,
                ..north_at(0.0, 0.0)
            };
            let mut app = headless_app(World::default(), spawn, CameraOccluders::default());
            app.update();
            let r = rider(&mut app);
            let feet = world_from_ground(r.position, r.height);
            heights.push(rig(&mut app).1.y - feet.y);
        }
        let follow = FollowCamera::default();
        assert!(
            (heights[0] - follow.look_height).abs() < 1e-4,
            "{heights:?}"
        );
        assert!(
            (heights[1] - follow.on_foot_look_height).abs() < 1e-4,
            "{heights:?}"
        );
        assert!(follow.on_foot_look_height < follow.look_height);
    }

    #[test]
    fn the_camera_rises_with_a_jumping_horse() {
        let mut app = headless_app(
            World::default(),
            north_at(0.0, 0.0),
            CameraOccluders::default(),
        );
        app.update();
        {
            let mut q = app.world_mut().query::<&mut Player>();
            let mut player = q.single_mut(app.world_mut()).unwrap();
            (player.rider.air, player.rider.lift) = (1.0, 0.0);
        }
        app.update();
        let r = rider(&mut app);
        assert!(r.airborne());
        // El foco sube suavemente hacia el jinete elevado, por encima de donde
        // mira con el caballo en el suelo.
        let ground = world_from_ground(r.position, r.height);
        let above = rig(&mut app).1.y - ground.y;
        assert!(
            above > FollowCamera::default().look_height + 0.05,
            "{above}"
        );
    }

    #[test]
    fn on_the_platform_the_camera_stays_above_it_and_is_not_blocked_by_it() {
        let town = TownLayout::castex_inspired();
        let world = World::with_town(Terrain::default(), &town);
        let occluders = CameraOccluders::from_town(&town, &world.terrain);
        let platform = town
            .buildings
            .iter()
            .find(|b| b.kind == BuildingKind::Platform)
            .unwrap();
        // Un piso, no una pared: la cámara puede mirar a través de él.
        assert!(
            occluders
                .0
                .iter()
                .all(|o| o.footprint != platform.footprint)
        );
        assert_eq!(
            occluders.0.len(),
            town.buildings.len() - 1 + town.canopies().len()
        );
        let (position, heading) = town.platform_spot().unwrap();
        let top = world.ground_height(position);
        let spawn = PlayerSpawn {
            position,
            heading,
            travel: pampa_core::Travel::OnFoot,
        };
        let mut app = headless_app(world.clone(), spawn, occluders.clone());
        app.update();
        // El jugador está parado sobre el andén.
        assert_eq!(rider(&mut app).height, top);
        let follow = FollowCamera::default();
        for step in 0..72 {
            let yaw = step as f32 * 5f32.to_radians();
            set_yaw(&mut app, yaw);
            app.update();
            let cam = camera(&mut app).translation;
            assert_outside(&occluders.0, cam, &format!("yaw {yaw}"));
            // Nunca hundida en el andén ni en el suelo a su lado.
            let floor = world.ground_height(ground(cam));
            assert!(cam.y > floor + 0.5, "yaw {yaw}: {cam} over {floor}");
            let (orbit, focus) = rig(&mut app);
            let desired = follow.desired(focus, &orbit, &world);
            let under = world.ground_height(ground(desired));
            assert!(
                desired.y >= under + follow.ground_clearance - 1e-4,
                "{desired}"
            );
        }
    }

    fn flat() -> Terrain {
        Terrain {
            amplitude: 0.0,
            ..Terrain::default()
        }
    }

    /// Un coche detenido sobre suelo plano en `center`, a lo largo de
    /// `direction`.
    fn coach(center: GroundVec2, direction: GroundVec2) -> PlacedCar {
        PlacedCar {
            kind: pampa_core::CarKind::Coach,
            center,
            direction: direction * (1.0 / direction.length()),
            length: 15.0,
            on_rail: true,
        }
    }

    /// Dónde está `p` respecto de `car`: a lo largo de su longitud y a lo
    /// ancho.
    fn car_frame(car: &PlacedCar, p: GroundVec2) -> (f32, f32) {
        let d = p - car.center;
        let across = GroundVec2::new(-car.direction.y, car.direction.x);
        (d.dot(car.direction), d.dot(across))
    }

    #[test]
    fn pulls_in_in_front_of_a_train_car_beside_the_player() {
        let focus = Vec3::new(0.0, 1.6, 0.0);
        // Un vagón que va hacia el este 5 m al norte del jugador, y uno en
        // pendiente.
        for direction in [GroundVec2::new(1.0, 0.0), GroundVec2::new(1.0, 0.6)] {
            let car = coach(GroundVec2::new(0.0, 5.0), direction);
            let occluders = car_occluders(&[car], &flat(), GroundVec2::ZERO);
            assert!(!occluders.is_empty(), "{direction:?}");
            let top = LineTrain::RAIL_TOP + car.kind.height();
            assert!(occluders.iter().all(|o| (o.top - top).abs() < 1e-4));
            // La cámara quiere estar 12 m al norte, detrás del vagón, a 3 m.
            let wanted = Vec3::new(0.0, 3.0, -12.0);
            let at = pull_in(focus, wanted, &occluders);
            assert_ne!(at, wanted, "{direction:?}: blocked by the car");
            assert_outside(&occluders, at, &format!("{direction:?}"));
            // Delante del vagón, del lado del jugador, apartada de su caja.
            let (along, across) = car_frame(&car, ground(at));
            assert!(along.abs() < car.length / 2.0, "{direction:?}");
            assert!(
                across < -LineTrain::WIDTH / 2.0,
                "{direction:?}: {across} across the car"
            );
        }
    }

    #[test]
    fn far_cars_are_left_out() {
        let car = coach(GroundVec2::new(0.0, 200.0), GroundVec2::new(1.0, 0.0));
        assert!(car_occluders(&[car], &flat(), GroundVec2::ZERO).is_empty());
    }

    /// Una losa de techo de 3.5 a 3.7 m sobre los 6 m al norte del foco.
    fn roof() -> Occluder {
        Occluder {
            footprint: Aabb::new(GroundVec2::new(-10.0, 0.0), GroundVec2::new(10.0, 6.0)),
            bottom: 3.5,
            top: 3.7,
        }
    }

    #[test]
    fn looks_under_a_canopy_but_not_through_its_roof() {
        let focus = Vec3::new(0.0, 1.6, 0.0);
        // Bajo el techo y más allá de él, baja: despejada.
        let under = Vec3::new(0.0, 3.0, -9.0);
        assert_eq!(pull_in(focus, under, &[roof()]), under);
        // Hacia arriba a través del techo: se detiene debajo.
        let over = Vec3::new(0.0, 7.0, -9.0);
        let at = pull_in(focus, over, &[roof()]);
        assert_ne!(at, over);
        assert!(at.y < roof().bottom, "{at}");
        assert!(!roof().contains(at));
        // Desde arriba del techo hasta una cámara debajo de él, también.
        let high = Vec3::new(0.0, 6.0, 0.0);
        let low = Vec3::new(0.0, 1.0, -4.0);
        let at = pull_in(high, low, &[roof()]);
        assert!(at.y > roof().top, "{at}");
    }

    #[test]
    fn the_platform_canopy_is_a_roof_over_the_platform() {
        let town = TownLayout::castex_inspired();
        let terrain = Terrain::default();
        let occluders = CameraOccluders::from_town(&town, &terrain);
        let canopy = town.canopies()[0];
        let c = canopy.footprint.center();
        let roof = occluders
            .0
            .iter()
            .find(|o| o.footprint == canopy.footprint)
            .expect("the canopy's roof");
        let ground = terrain.height(c.x, c.y);
        assert!((roof.bottom - ground - canopy.bottom()).abs() < 1e-4);
        assert!((roof.top - ground - canopy.top()).abs() < 1e-4);
    }

    #[test]
    fn train_cars_are_looked_at_as_far_as_the_orbit_arm_reaches() {
        // El brazo nunca llega más allá de los vagones considerados, con un
        // poco de margen, y el alcance no es innecesariamente largo.
        let arm = OrbitParams::default().max_distance;
        assert!(
            CAR_REACH > arm && CAR_REACH <= arm + 10.0,
            "{CAR_REACH} vs {arm}"
        );
        let car = |d: f32| pampa_core::PlacedCar {
            kind: pampa_core::CarKind::Coach,
            center: GroundVec2::new(d, 0.0),
            direction: GroundVec2::new(0.0, 1.0),
            length: 12.0,
            on_rail: true,
        };
        let terrain = Terrain::default();
        // Un vagón cuyo costado está en el extremo del brazo más largo igual
        // cuenta.
        assert!(!car_occluders(&[car(arm)], &terrain, GroundVec2::ZERO).is_empty());
        assert!(car_occluders(&[car(CAR_REACH + 7.0)], &terrain, GroundVec2::ZERO).is_empty());
    }

    #[test]
    fn the_canopy_roof_stands_on_the_leveled_station_ground() {
        let town = TownLayout::castex_inspired();
        let world = World::with_town(Terrain::default(), &town);
        let occluders = CameraOccluders::from_town(&town, &world.terrain);
        let canopy = town.canopies()[0];
        let station = town
            .buildings
            .iter()
            .find(|b| b.kind == BuildingKind::Station)
            .unwrap()
            .footprint
            .center();
        let roof = occluders
            .0
            .iter()
            .find(|o| o.footprint == canopy.footprint)
            .unwrap();
        // Anclado sobre el terreno de la estación, en cualquier punto del techo
        // en que se mida: el terreno nivelado de la estación es plano.
        let ground = world.terrain.height(station.x, station.y);
        assert!((roof.bottom - ground - canopy.bottom()).abs() < 1e-4);
        for corner in [canopy.footprint.min, canopy.footprint.max] {
            assert!((world.terrain.height(corner.x, corner.y) - ground).abs() < 1e-4);
        }
    }

    #[test]
    fn pulls_in_at_once_and_eases_out_slowly() {
        // Bloqueada: directo al alcance permitido, nunca más allá.
        assert_eq!(ease_reach(1.0, 0.3, 0.001), 0.3);
        assert_eq!(ease_reach(0.5, 0.3, 0.1), 0.3);
        // Despejada: alejándose, todavía antes de él tras un frame.
        let one = ease_reach(0.3, 1.0, FRAME);
        assert!(one > 0.3 && one < 0.35, "{one}");
        let mut r = 0.3;
        let mut t = 0.0;
        while r < 0.95 {
            r = ease_reach(r, 1.0, FRAME);
            t += FRAME;
        }
        assert!(t > 0.5 && t < 1.5, "out in {t} s");
        // Independiente de la tasa de frames.
        let coarse = ease_reach(0.3, 1.0, 0.1);
        let fine = (0..10).fold(0.3, |r, _| ease_reach(r, 1.0, 0.01));
        assert!((coarse - fine).abs() < 1e-5, "{coarse} vs {fine}");
        // Nunca más allá del alcance permitido mientras se suaviza.
        assert!((0..100).fold(0.3, |r, _| ease_reach(r, 0.8, 0.05)) <= 0.8);
    }

    #[test]
    fn eases_back_out_when_the_view_clears() {
        // Una pared detrás del jinete acerca la cámara; una vez que desaparece,
        // la cámara se desliza de vuelta hacia afuera durante unos frames en
        // lugar de saltar.
        let wall = Occluder::column(
            Aabb::new(GroundVec2::new(-5.0, -7.0), GroundVec2::new(5.0, -4.0)),
            -10.0,
            50.0,
        );
        let mut app = headless_app(
            World::default(),
            north_at(0.0, 0.0),
            CameraOccluders(vec![wall]),
        );
        app.update();
        app.update();
        let pulled = ground(camera(&mut app).translation).distance(GroundVec2::ZERO);
        app.insert_resource(CameraOccluders::default());
        app.update();
        let next = ground(camera(&mut app).translation).distance(GroundVec2::ZERO);
        for _ in 0..200 {
            app.update();
        }
        let out = ground(camera(&mut app).translation).distance(GroundVec2::ZERO);
        assert!(pulled < 4.0, "{pulled}");
        assert!(
            next > pulled && next < out - 1.0,
            "{pulled} -> {next} -> {out}"
        );
    }

    /// La apertura del juego (a bordo, entrando a Monte Nievas) con la cámara.
    fn riding_app() -> App {
        let mut app = crate::ride::testing::shipped_app();
        app.add_plugins(FollowCameraPlugin);
        app.update();
        app
    }

    /// La cámara en el marco del coche del jugador, y ese coche.
    fn in_coach(app: &mut App) -> (Transform, PlacedCar) {
        let train = app.world().resource::<WorldTrains>().0[0].clone();
        let (_, ride) = crate::ride::testing::passenger(app);
        let car = train.placed_cars()[ride.expect("aboard").car];
        let terrain = app.world().resource::<GameWorld>().0.terrain.clone();
        let placed = car_transform(&car, &terrain);
        let cam = camera(app);
        let inverse = placed.compute_affine().inverse();
        let local = Transform::from_translation(inverse.transform_point3(cam.translation))
            .with_rotation(placed.rotation.inverse() * cam.rotation);
        (local, car)
    }

    #[test]
    fn aboard_the_camera_sits_by_a_coach_window_and_rides_with_the_train() {
        let mut app = riding_app();
        assert_eq!(*app.world().resource::<RideView>(), RideView::Window);
        let (local, car) = in_coach(&mut app);
        let p = local.translation;
        // Dentro de la caja del coche, apartada de sus paredes, su piso y su
        // techo.
        let inner = CoachBody::HALF_WIDTH - CoachBody::WALL;
        assert!(p.x.abs() < car.length / 2.0 - CoachBody::END_INSET, "{p}");
        assert!(p.z.abs() < inner - 0.3, "{p}");
        assert!(
            p.y > CoachBody::FLOOR + 0.5 && p.y < CoachBody::CEILING - 0.5,
            "{p}"
        );
        // Junto a la abertura de una ventanilla, mirando hacia afuera.
        let (windows, open) = CoachBody::windows(car.length);
        assert!(windows.iter().any(|&x| (p.x - x).abs() < open / 2.0));
        assert!(p.y > CoachBody::SILL && p.y < CoachBody::LINTEL);
        // Sentado mirando hacia adelante, en el sentido de la marcha, con la
        // ventanilla al lado.
        let ahead = local.rotation * Vec3::NEG_Z;
        assert!(ahead.dot(Vec3::X) > 0.99, "faces forward: {ahead}");
        let side = crate::ride::testing::passenger(&mut app).1.unwrap().side;
        assert!(p.z * side > 0.0, "on the platform side");
        // Viaja con él: el tren se mueve, el asiento queda fijo en el coche.
        let before = camera(&mut app).translation;
        for _ in 0..60 {
            app.update();
        }
        let (again, _) = in_coach(&mut app);
        assert!(camera(&mut app).translation.distance(before) > 5.0);
        assert!(again.translation.distance(p) < 1e-2);
        // El paisaje hace streaming alrededor del tren en movimiento.
        let (rider, _) = crate::ride::testing::passenger(&mut app);
        let focus = app.world().resource::<pampa_render::StreamFocus>().0;
        let train = app.world().resource::<WorldTrains>().0[0].clone();
        let (_, ride) = crate::ride::testing::passenger(&mut app);
        assert_eq!(focus, train.placed_cars()[ride.unwrap().car].center);
        assert_eq!(focus, rider.position);
    }

    #[test]
    fn the_seated_look_stays_within_the_limits() {
        let params = OrbitParams::default();
        let mut look = SeatLook::default();
        for _ in 0..200 {
            look.update(
                OrbitInput {
                    look: (900.0, 900.0),
                    keys: 1.0,
                    zoom: 0.0,
                },
                &params,
                0.1,
            );
        }
        assert_eq!((look.yaw, look.pitch), (-SEAT_YAW_LIMIT, -SEAT_PITCH_LIMIT));
        for _ in 0..200 {
            look.update(
                OrbitInput {
                    look: (-900.0, -900.0),
                    keys: -1.0,
                    zoom: 0.0,
                },
                &params,
                0.1,
            );
        }
        assert_eq!((look.yaw, look.pitch), (SEAT_YAW_LIMIT, SEAT_PITCH_LIMIT));
        // Girada hasta el tope: unos 120° respecto de mirar hacia adelante (el
        // +X del coche), más allá de cualquiera de las ventanillas, mirando 40°
        // hacia arriba.
        let view = seat_view(&Transform::IDENTITY, 12.0, 1.0, look);
        let ahead = view.rotation * Vec3::NEG_Z;
        let flat = Vec3::new(ahead.x, 0.0, ahead.z).normalize();
        assert!((flat.dot(Vec3::X).acos() - SEAT_YAW_LIMIT).abs() < 1e-3);
        assert!((ahead.y.asin() - SEAT_PITCH_LIMIT).abs() < 1e-3);
        // Q/E giran la cabeza también en la app, pero nunca más allá del
        // límite.
        let mut app = riding_app();
        keys(&mut app).press(KeyCode::KeyE);
        for _ in 0..200 {
            app.update();
        }
        let look = *app.world().resource::<SeatLook>();
        assert_eq!(look.yaw, -SEAT_YAW_LIMIT);
    }

    /// Qué tan de frente mira hacia adelante la cámara sentada a lo largo de su
    /// coche.
    fn facing_forward(app: &mut App) -> f32 {
        let (local, _) = in_coach(app);
        (local.rotation * Vec3::NEG_Z).dot(Vec3::X)
    }

    /// Gira la cabeza sentada con el mouse, tal como lo transmite una ventana
    /// con foco y el cursor capturado.
    fn mouse_look(app: &mut App, delta: Vec2) {
        let world = app.world_mut();
        let mut windows = world.query_filtered::<(), With<PrimaryWindow>>();
        if windows.iter(world).next().is_none() {
            world.spawn((Window::default(), CursorOptions::default(), PrimaryWindow));
        }
        app.insert_resource(AccumulatedMouseMotion { delta });
        app.update();
        app.update();
        app.insert_resource(AccumulatedMouseMotion::default());
        app.update();
    }

    fn tap(app: &mut App, key: KeyCode) {
        keys(app).press(key);
        app.update();
        let mut k = keys(app);
        k.release(key);
        k.clear();
        app.update();
    }

    #[test]
    fn every_ride_starts_seated_facing_forward_and_the_mouse_looks_round() {
        // La apertura.
        let mut app = riding_app();
        assert!(facing_forward(&mut app) > 0.99, "the opening");
        // El mouse gira la cabeza, y no más de lo que permite el asiento.
        mouse_look(&mut app, Vec2::new(40.0, 0.0));
        let look = *app.world().resource::<SeatLook>();
        assert!(look.yaw < -0.05, "turned right: {look:?}");
        assert!(facing_forward(&mut app) < 0.999);
        mouse_look(&mut app, Vec2::new(1.0e5, -1.0e5));
        let look = *app.world().resource::<SeatLook>();
        assert_eq!((look.yaw, look.pitch), (-SEAT_YAW_LIMIT, SEAT_PITCH_LIMIT));
        // Recorrer las vistas con V y volver a la ventanilla: de nuevo hacia
        // adelante.
        for _ in 0..3 {
            tap(&mut app, VIEW_KEY);
        }
        assert_eq!(*app.world().resource::<RideView>(), RideView::Window);
        assert!(facing_forward(&mut app) > 0.99, "back at the window");
        // Se baja en Monte Nievas con la cabeza girada, y luego vuelve a subir.
        mouse_look(&mut app, Vec2::new(-60.0, 0.0));
        crate::ride::testing::arrive(&mut app);
        crate::ride::testing::press(&mut app, crate::errand::INTERACT_KEY);
        app.update();
        assert!(
            crate::ride::testing::passenger(&mut app).1.is_some(),
            "boarded"
        );
        assert!(facing_forward(&mut app) > 0.99, "a normal boarding");
        // Un viaje saltado también entra mirando hacia adelante.
        mouse_look(&mut app, Vec2::new(-60.0, 0.0));
        crate::ride::testing::press(&mut app, crate::ride::SKIP_KEY);
        for _ in 0..2000 {
            app.update();
            if app.world().resource::<crate::ride::Trip>().0
                == Some(crate::ride::TripPhase::Arriving)
            {
                break;
            }
        }
        app.update();
        assert!(crate::ride::testing::passenger(&mut app).1.is_some());
        assert!(facing_forward(&mut app) > 0.99, "after the skip");
    }

    #[test]
    fn an_ordinary_arrival_keeps_the_head_where_it_was_turned() {
        // La apertura viajando hasta Monte Nievas sin salto, mirando hacia un
        // costado todo el camino.
        let mut app = riding_app();
        mouse_look(&mut app, Vec2::new(-60.0, 10.0));
        let turned = *app.world().resource::<SeatLook>();
        assert!(turned.yaw > 0.05, "{turned:?}");
        let mut frames = 0;
        while crate::ride::testing::passenger(&mut app).1.is_some() {
            assert_eq!(
                *app.world().resource::<SeatLook>(),
                turned,
                "frame {frames}"
            );
            app.update();
            frames += 1;
            assert!(frames < 4000, "never got off");
        }
        assert_eq!(
            app.world().resource::<crate::ride::Trip>().0,
            None,
            "an ordinary ride, no skip"
        );
    }

    #[test]
    fn v_cycles_the_riding_views_and_wraps() {
        let mut app = riding_app();
        let hint = |app: &App| app.world().resource::<HudHint>().0.clone();
        assert_eq!(hint(&app).as_deref(), Some("V: vista - ventana"));
        let mut seen = Vec::new();
        for _ in 0..4 {
            keys(&mut app).press(VIEW_KEY);
            app.update();
            {
                let mut k = keys(&mut app);
                k.release(VIEW_KEY);
                k.clear();
            }
            app.update();
            seen.push(*app.world().resource::<RideView>());
        }
        assert_eq!(
            seen,
            [
                RideView::Coach,
                RideView::Train,
                RideView::Window,
                RideView::Coach
            ]
        );
        assert_eq!(hint(&app).as_deref(), Some("V: vista - exterior"));
        // Fuera del coche en la órbita, y apartada al costado del tren entero
        // en la vista del tren.
        let (local, car) = in_coach(&mut app);
        assert!(
            local.translation.z.abs() > CoachBody::HALF_WIDTH
                || local.translation.x.abs() > car.length / 2.0
        );
        keys(&mut app).press(VIEW_KEY);
        app.update();
        keys(&mut app).release(VIEW_KEY);
        keys(&mut app).clear();
        app.update();
        assert_eq!(hint(&app).as_deref(), Some("V: vista - tren"));
        let (local, _) = in_coach(&mut app);
        assert!(local.translation.z > 30.0, "{}", local.translation);
        // Fuera del tren: sin ayuda.
        crate::ride::testing::arrive_from_anywhere(&mut app);
        assert_eq!(hint(&app), None);
    }

    #[test]
    fn on_the_platform_the_camera_stays_in_front_of_the_dwelling_train() {
        let loaded = crate::shipped_world::loaded();
        // El tren de la apertura, saltado hasta su parada en Monte Nievas.
        let (clock, trains, _) = crate::life::start_of_day(&loaded.line, &loaded.towns);
        let mut train = trains.into_iter().next().expect("a line train");
        train.skip_to_next_stop(clock.elapsed(), clock.rate());
        let world = World::with_towns(loaded.world, Terrain::default(), &loaded.towns);
        let occluders = CameraOccluders::from_towns(&loaded.towns, &world.terrain);
        let (position, heading) = loaded.towns[0].platform_spot().unwrap();
        let cars = train.placed_cars();
        // El vagón junto al jugador, y el rumbo a través de la vía hasta él.
        let beside = *cars
            .iter()
            .min_by(|a, b| {
                let d = |c: &PlacedCar| c.center.distance(position);
                d(a).total_cmp(&d(b))
            })
            .unwrap();
        let (_, across) = car_frame(&beside, position);
        let normal = GroundVec2::new(-beside.direction.y, beside.direction.x);
        let to_track = normal * -across.signum();
        let spawn = PlayerSpawn {
            position,
            heading,
            travel: pampa_core::Travel::OnFoot,
        };
        let mut app = headless_app(world.clone(), spawn, occluders.clone());
        app.insert_resource(WorldTrains(vec![train]));
        app.update();
        let yaw = to_track.y.atan2(to_track.x);
        let mut pulled = 0;
        for k in -3..=3 {
            set_yaw(&mut app, yaw + k as f32 * 0.15);
            app.update();
            let cam = camera(&mut app).translation;
            let cars = car_occluders(&cars, &world.terrain, position);
            assert_outside(&cars, cam, &format!("yaw step {k}"));
            // Del lado del jugador del tren, con una vista despejada.
            let (along, side) = car_frame(&beside, ground(cam));
            assert!(
                side * across > 0.0 && side.abs() > LineTrain::WIDTH / 2.0,
                "step {k}: {along}, {side} vs the player's {across}"
            );
            let r = rider(&mut app);
            let follow = FollowCamera::default();
            let target =
                world_from_ground(r.position, r.height) + Vec3::Y * follow.on_foot_look_height;
            assert_eq!(pull_in(target, cam, &cars), cam, "step {k}");
            let (orbit, focus) = rig(&mut app);
            if cam.distance(follow.desired(focus, &orbit, &world)) > 1.0 {
                pulled += 1;
            }
        }
        assert!(pulled >= 5, "the cars pull the camera in: {pulled}");
    }
}
