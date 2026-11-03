//! Los animales alrededor del jugador: `pampa-core` los siembra ([`FaunaSite`])
//! y los hace avanzar; este módulo los trae por streaming con los chunks dentro
//! de [`RADIUS`] del jugador, los más cercanos primero y hasta los [`caps`] (se
//! replanifica cada vez que el jugador entra en otro chunk, para que los chunks
//! más cercanos ocupen el lugar primero; un animal de suelo cerca del jugador
//! sobrevive a su chunk, ver `Fauna::strays`), los hace avanzar con el jugador como
//! la amenaza que esquivan, y copia su estado en las entidades que dibuja
//! render ([`Animal`]). Los felinos se siembran solo en el monte de la flora
//! del mundo ([`WorldFlora`]) y, si salen de él, vuelven antes de dos horas de
//! juego (`Critter::step_in`); sin flora no hay felinos. Los animales de
//! suelo pisan el pasto abierto del mundo, con sus caminos y los alambrados
//! que el zorro y los felinos saltan ([`Wilds`]).
//!
//! El ganado es un cuerpo blando: el jugador se mantiene apartado de él y lo
//! aparta al caminar. Los caballos que están en los corrales son [`Mountable`]
//! (una vez desbloqueada la monta, cualquier caballo lo es) y obstáculos
//! mientras están quietos; uno que se toma queda para el jugador y no se vuelve
//! a sembrar en esta sesión.

use std::collections::{BTreeMap, HashSet};

use bevy::prelude::*;
use pampa_core::fauna::Land;
use pampa_core::fauna::caps;
use pampa_core::{
    Aabb, ChunkCoord, CowState, Critter, CritterKind, CritterState, Duck, FaunaSite, FenceLayout,
    Flock, Hare, HareState, Heading, Herd, MAX_STEP, Raptor, RaptorKind, TetheredHorse,
    Vec2 as GroundVec2, World,
};
use pampa_render::{Animal, AnimalPose, AnimalSeed, Coat, Gait, WorldFlora, world_from_ground};

use crate::errand::Mountable;
use crate::player::{DrivePlayer, GameWorld, Player, drive_player};
use crate::ride::OnTrain;

/// Chunks, contados desde el del jugador, que tienen animales vivos.
pub const RADIUS: i32 = 2;

/// Hasta cuántos chunks del jugador sigue vivo un animal de suelo cuyo chunk se
/// fue (ver `Fauna::strays`).
pub const STRAY_REACH: f32 = RADIUS as f32 + 0.5;
/// Las manadas a más de esta distancia del jugador no se comprueban contra el
/// jinete, en metros.
const BODY_REACH: f32 = 40.0;

/// Dónde viven los animales del mundo.
#[derive(Resource, Debug, Clone)]
pub struct WorldFauna(pub FaunaSite);

/// Los animales de un chunk cargado por streaming, cada uno con las entidades
/// que lo dibujan.
#[derive(Debug, Default)]
struct ChunkFauna {
    herds: Vec<(Herd, Vec<Entity>)>,
    horses: Vec<(TetheredHorse, Entity)>,
    hares: Vec<(Hare, Entity)>,
    flocks: Vec<(Flock, Vec<Entity>)>,
    raptors: Vec<(Raptor, Entity)>,
    ducks: Vec<(Duck, Entity)>,
    critters: Vec<(Critter, Entity)>,
    /// El lugar de cada animal de `critters` en la lista sembrada de su chunk.
    critter_ids: Vec<u64>,
    nutrias: Vec<(Duck, Entity)>,
    /// Cuáles de los animales sembrados del chunk contiene (ver [`Picks`]).
    picks: Picks,
}

/// Cuáles de los animales sembrados de un chunk se dibujan, por tipo en el
/// orden de [`Fauna::all_counts`]: las manadas por id, los caballos de corral por
/// su huella en planta ([`horse_key`]), el resto por su lugar en la lista
/// sembrada del chunk. Dos planes con las mismas selecciones contienen
/// exactamente los mismos animales.
type Picks = [Vec<u64>; KINDS];

/// Cuántos tipos de animales se cuentan (ver [`Fauna::all_counts`]).
pub const KINDS: usize = 12;

/// El lugar de los animales de suelo de `kind` en [`Fauna::all_counts`]: zorros,
/// ñandúes, armadillos, vizcachas y felinos.
fn critter_slot(kind: CritterKind) -> usize {
    match kind {
        CritterKind::Zorro => 6,
        CritterKind::Nandu => 7,
        CritterKind::Mulita | CritterKind::Peludo => 8,
        CritterKind::Vizcacha => 9,
        CritterKind::GatoMontes | CritterKind::Puma | CritterKind::Yaguarete => 10,
    }
}

/// Cómo se dibuja un animal de suelo de `kind`.
fn critter_animal(kind: CritterKind) -> Animal {
    match kind {
        CritterKind::Zorro => Animal::Zorro,
        CritterKind::Nandu => Animal::Nandu,
        CritterKind::Mulita => Animal::Mulita,
        CritterKind::Peludo => Animal::Peludo,
        CritterKind::Vizcacha => Animal::Vizcacha,
        CritterKind::GatoMontes => Animal::GatoMontes,
        CritterKind::Puma => Animal::Puma,
        CritterKind::Yaguarete => Animal::Yaguarete,
    }
}

/// Una semilla estable a partir de un lugar sembrado, para desincronizar los
/// clips de los animales de modelo.
fn spot_seed(p: GroundVec2) -> u32 {
    p.x.to_bits() ^ p.y.to_bits().rotate_left(16)
}

impl ChunkFauna {
    /// Vacas, caballos, liebres, teros, rapaces, patos, zorros, ñandúes,
    /// armadillos, vizcachas, felinos y nutrias.
    fn counts(&self) -> [usize; KINDS] {
        let mut counts = [
            self.herds.iter().map(|h| h.0.cows.len()).sum(),
            self.horses.len(),
            self.hares.len(),
            self.flocks.iter().map(|f| f.1.len()).sum(),
            self.raptors.len(),
            self.ducks.len(),
            0,
            0,
            0,
            0,
            0,
            self.nutrias.len(),
        ];
        for (critter, _) in &self.critters {
            counts[critter_slot(critter.kind)] += 1;
        }
        counts
    }

    fn entities(&self) -> impl Iterator<Item = Entity> + '_ {
        let herds = self.herds.iter().flat_map(|h| h.1.iter().copied());
        let flocks = self.flocks.iter().flat_map(|f| f.1.iter().copied());
        herds
            .chain(flocks)
            .chain(self.horses.iter().map(|h| h.1))
            .chain(self.hares.iter().map(|h| h.1))
            .chain(self.raptors.iter().map(|r| r.1))
            .chain(self.ducks.iter().map(|d| d.1))
            .chain(self.critters.iter().map(|c| c.1))
            .chain(self.nutrias.iter().map(|n| n.1))
    }
}

/// Los animales vivos alrededor del jugador ahora.
#[derive(Resource, Debug, Default)]
pub struct Fauna {
    center: Option<ChunkCoord>,
    chunks: BTreeMap<ChunkCoord, ChunkFauna>,
    /// Los animales de suelo que andan cerca del jugador (ver [`STRAY_REACH`]),
    /// con el chunk del que vienen en `stray_from`: sobreviven a su chunk, su
    /// lugar bajo los límites va primero y no se vuelven a sembrar mientras
    /// tanto; lejos, vuelven a su chunk si sigue cargado.
    strays: ChunkFauna,
    stray_from: Vec<ChunkCoord>,
    /// Caballos de corral que tomó el jugador, por la esquina de su huella en
    /// planta.
    taken: HashSet<(u32, u32)>,
}

impl Fauna {
    /// Los animales silvestres de suelo que se dibujan ahora, con su especie y
    /// dónde están.
    pub fn critters(&self) -> impl Iterator<Item = (CritterKind, GroundVec2)> + '_ {
        self.chunks
            .values()
            .chain([&self.strays])
            .flat_map(|c| &c.critters)
            .map(|(k, _)| (k.kind, k.position))
    }

    /// Vacas, caballos, liebres, teros, rapaces, patos, zorros, ñandúes,
    /// armadillos (mulitas y peludos), vizcachas, felinos (gatos monteses,
    /// pumas y yaguaretés) y nutrias vivos ahora (lo que muestra el comando
    /// `fauna` de la consola).
    pub fn all_counts(&self) -> [usize; KINDS] {
        let all = self.chunks.values().chain([&self.strays]);
        all.fold([0; KINDS], |mut sum, c| {
            for (s, n) in sum.iter_mut().zip(c.counts()) {
                *s += n;
            }
            sum
        })
    }
}

/// El máximo de cada tipo, en el orden de [`Fauna::all_counts`].
pub const CAPS: [usize; KINDS] = [
    caps::COWS,
    caps::HORSES,
    caps::HARES,
    caps::TEROS,
    caps::RAPTORS,
    caps::DUCKS,
    caps::ZORROS,
    caps::NANDUS,
    caps::ARMADILLOS,
    caps::VIZCACHAS,
    caps::FELINES,
    caps::NUTRIAS,
];

/// Los límites vigentes: [`CAPS`] salvo que un test los baje.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct FaunaCaps(pub [usize; KINDS]);

impl Default for FaunaCaps {
    fn default() -> Self {
        Self(CAPS)
    }
}

fn horse_key(h: &TetheredHorse) -> (u32, u32) {
    (h.footprint.min.x.to_bits(), h.footprint.min.y.to_bits())
}

/// La [`horse_key`] de un caballo como una selección.
fn horse_pick(h: &TetheredHorse) -> u64 {
    let (x, y) = horse_key(h);
    (u64::from(x) << 32) | u64::from(y)
}

pub struct FaunaPlugin;

impl Plugin for FaunaPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Fauna>()
            .init_resource::<FaunaCaps>()
            .add_systems(
                Update,
                (stream_fauna, step_fauna, place_fauna)
                    .chain()
                    .in_set(DrivePlayer)
                    .after(drive_player),
            );
    }
}

/// Dónde está el jugador, salvo que esté a bordo del tren.
type Players<'w, 's> =
    Query<'w, 's, (&'static mut Player, &'static mut Transform), Without<OnTrain>>;

/// Lo que limita el plan de [`stream_fauna`]: los caballos todavía parados en
/// sus corrales, los topes y el monte de la flora.
type Limits<'w, 's> = (
    Query<'w, 's, (), With<Mountable>>,
    Res<'w, FaunaCaps>,
    Option<Res<'w, WorldFlora>>,
);

/// Replanifica los animales alrededor del jugador cada vez que entra en otro
/// chunk: los chunks dentro de [`RADIUS`], los más cercanos primero, toman cada
/// uno lo que de lo suyo todavía entra bajo los [`CAPS`]. Un chunk cargado cuya
/// parte no cambió conserva sus animales tal como están; uno cuya parte cambió
/// (un chunk más cercano ahora ocupa el lugar, o se liberó lugar) se vuelve a
/// dibujar con su nueva parte; los chunks fuera de alcance se van.
fn stream_fauna(
    mut commands: Commands,
    site: Option<Res<WorldFauna>>,
    mut fauna: ResMut<Fauna>,
    mut world: ResMut<GameWorld>,
    players: Query<&Player>,
    (standing, caps, flora): Limits,
) {
    let (Some(site), Ok(player)) = (site, players.single()) else {
        return;
    };
    // Un caballo que falta de su corral fue tomado: no se vuelve a sembrar
    // nunca.
    let fauna = &mut *fauna;
    for chunk in fauna.chunks.values_mut() {
        chunk.horses.retain(|(horse, e)| {
            let gone = standing.get(*e).is_err();
            if gone {
                fauna.taken.insert(horse_key(horse));
            }
            !gone
        });
        chunk.picks[1] = chunk.horses.iter().map(|(h, _)| horse_pick(h)).collect();
    }
    let config = site.0.config();
    let center = config.chunk_of(player.rider.position);
    if fauna.center == Some(center) {
        return;
    }
    fauna.center = Some(center);
    let mut wanted = config.chunks_near(player.rider.position, RADIUS);
    wanted.sort_by_key(|c| c.distance_squared(center));
    // Todo animal de suelo que anda cerca del jugador queda suelto: sobrevive
    // a su chunk y su lugar bajo los límites va antes que el de nadie. Uno
    // suelto que quedó lejos vuelve a su chunk si sigue cargado, si no se va.
    let here = player.rider.position;
    let reach = STRAY_REACH * config.chunk_size;
    let strays = std::mem::take(&mut fauna.strays.critters);
    let from = std::mem::take(&mut fauna.stray_from);
    let ids = std::mem::take(&mut fauna.strays.critter_ids);
    for ((stray, e), (from, id)) in strays.into_iter().zip(from.into_iter().zip(ids)) {
        if stray.position.distance(here) <= reach {
            fauna.strays.critters.push((stray, e));
            fauna.strays.critter_ids.push(id);
            fauna.stray_from.push(from);
        } else if let Some(chunk) = fauna.chunks.get_mut(&from) {
            chunk.critters.push((stray, e));
            chunk.critter_ids.push(id);
        } else {
            commands.entity(e).despawn();
        }
    }
    for (&coord, chunk) in fauna.chunks.iter_mut() {
        let critters = std::mem::take(&mut chunk.critters);
        let ids = std::mem::take(&mut chunk.critter_ids);
        for ((critter, e), id) in critters.into_iter().zip(ids) {
            if critter.position.distance(here) <= reach {
                fauna.strays.critters.push((critter, e));
                fauna.strays.critter_ids.push(id);
                fauna.stray_from.push(coord);
            } else {
                chunk.critters.push((critter, e));
                chunk.critter_ids.push(id);
            }
        }
    }
    let mut room = caps.0;
    for (stray, _) in &fauna.strays.critters {
        let slot = critter_slot(stray.kind);
        room[slot] = room[slot].saturating_sub(1);
    }
    let loose: HashSet<(ChunkCoord, u64)> = fauna
        .stray_from
        .iter()
        .copied()
        .zip(fauna.strays.critter_ids.iter().copied())
        .collect();
    let mut plan = BTreeMap::new();
    let monte = |p| flora.as_ref().map_or(0.0, |f| f.0.monte(p));
    for &coord in &wanted {
        plan.insert(
            coord,
            select(&site.0, coord, &mut room, &fauna.taken, &loose, monte),
        );
    }
    let stale: Vec<ChunkCoord> = fauna
        .chunks
        .iter()
        .filter(|(c, chunk)| plan.get(*c).is_none_or(|p| p.picks != chunk.picks))
        .map(|(c, _)| *c)
        .collect();
    for coord in stale {
        let chunk = fauna.chunks.remove(&coord).unwrap_or_default();
        for (horse, _) in &chunk.horses {
            world.0.obstacles.remove(&horse.footprint);
        }
        for e in chunk.entities() {
            commands.entity(e).despawn();
        }
    }
    for (coord, share) in plan {
        if let std::collections::btree_map::Entry::Vacant(slot) = fauna.chunks.entry(coord) {
            slot.insert(spawn_share(&mut commands, share, &mut world));
        }
    }
}

/// Lo que de los animales de un chunk entra bajo los límites, antes de
/// dibujarlo.
#[derive(Debug, Default)]
struct Share {
    herds: Vec<Herd>,
    horses: Vec<TetheredHorse>,
    hares: Vec<Hare>,
    flocks: Vec<Flock>,
    raptors: Vec<Raptor>,
    ducks: Vec<Duck>,
    critters: Vec<Critter>,
    critter_ids: Vec<u64>,
    nutrias: Vec<Duck>,
    picks: Picks,
}

/// Los animales de `coord` que entran en el lugar `room` que queda bajo los
/// límites (solo manadas y bandadas enteras), sin los caballos `taken` ni los
/// animales de suelo sueltos `loose` (ver `Fauna::strays`), con
/// `monte` la parte de monte de cada lugar (`FloraSite::monte`).
fn select(
    site: &FaunaSite,
    coord: ChunkCoord,
    room: &mut [usize; KINDS],
    taken: &HashSet<(u32, u32)>,
    loose: &HashSet<(ChunkCoord, u64)>,
    monte: impl Fn(GroundVec2) -> f32,
) -> Share {
    /// Los `items` que entran en `room`, en orden, con sus selecciones.
    fn fits<T>(
        items: Vec<T>,
        size: impl Fn(&T) -> usize,
        pick: impl Fn(usize, &T) -> u64,
        room: &mut usize,
        picks: &mut Vec<u64>,
    ) -> Vec<T> {
        let mut kept = Vec::new();
        for (k, item) in items.into_iter().enumerate() {
            let n = size(&item);
            if n <= *room {
                *room -= n;
                picks.push(pick(k, &item));
                kept.push(item);
            }
        }
        kept
    }
    /// La selección de un tipo sin id: su lugar en la lista sembrada.
    fn index<T>(k: usize, _: &T) -> u64 {
        k as u64
    }
    let free = site
        .horses_in(coord)
        .into_iter()
        .filter(|h| !taken.contains(&horse_key(h)))
        .collect();
    let mut picks = Picks::default();
    // Cada animal de suelo entra si queda lugar para su tipo; uno que anda
    // suelto (`loose`, ya contado) sigue en la selección pero no se vuelve a
    // sembrar.
    let (mut critters, mut critter_ids) = (Vec::new(), Vec::new());
    for (k, critter) in site.critters_in(coord, monte).into_iter().enumerate() {
        let slot = critter_slot(critter.kind);
        if loose.contains(&(coord, k as u64)) {
            picks[slot].push(k as u64);
        } else if room[slot] > 0 {
            room[slot] -= 1;
            picks[slot].push(k as u64);
            critters.push(critter);
            critter_ids.push(k as u64);
        }
    }
    let [
        cows,
        horses,
        hares,
        teros,
        raptors,
        ducks,
        _,
        _,
        _,
        _,
        _,
        nutrias,
    ] = &mut picks;
    let herd_id = |_: usize, h: &Herd| u64::from(h.id);
    let herds = fits(
        site.herds_in(coord),
        |h| h.cows.len(),
        herd_id,
        &mut room[0],
        cows,
    );
    let horse = |_: usize, h: &TetheredHorse| horse_pick(h);
    let flock_size = |f: &Flock| f.birds().count();
    Share {
        herds,
        horses: fits(free, |_| 1, horse, &mut room[1], horses),
        hares: fits(site.hares_in(coord), |_| 1, index, &mut room[2], hares),
        flocks: fits(
            site.flocks_in(coord),
            flock_size,
            index,
            &mut room[3],
            teros,
        ),
        raptors: fits(site.raptors_in(coord), |_| 1, index, &mut room[4], raptors),
        ducks: fits(site.ducks_in(coord), |_| 1, index, &mut room[5], ducks),
        critters,
        critter_ids,
        nutrias: fits(site.nutrias_in(coord), |_| 1, index, &mut room[11], nutrias),
        picks,
    }
}

/// Crea las entidades que dibujan la parte `share` de un chunk; sus caballos de
/// corral quedan como obstáculos.
fn spawn_share(commands: &mut Commands, share: Share, world: &mut GameWorld) -> ChunkFauna {
    // Solo los animales de modelo leen la semilla (sus esperas y
    // desplazamientos).
    let mut spawn = |animal: Animal, seed: u32| {
        commands
            .spawn((Name::new("Animal"), animal, AnimalSeed(seed)))
            .id()
    };
    let mut chunk = ChunkFauna {
        picks: share.picks,
        ..default()
    };
    for herd in share.herds {
        let entities = herd
            .cows
            .iter()
            .enumerate()
            .map(|(k, c)| {
                let animal = if c.bull {
                    Animal::Bull(c.coat)
                } else {
                    Animal::Cow(c.coat)
                };
                spawn(animal, herd.id.wrapping_mul(64) ^ k as u32)
            })
            .collect();
        chunk.herds.push((herd, entities));
    }
    for hare in share.hares {
        chunk.hares.push((hare, spawn(Animal::Hare, 0)));
    }
    for flock in share.flocks {
        let birds = flock.birds().map(|_| spawn(Animal::Tero, 0)).collect();
        chunk.flocks.push((flock, birds));
    }
    for raptor in share.raptors {
        let animal = match raptor.kind {
            RaptorKind::Chimango => Animal::Chimango,
            RaptorKind::Carancho => Animal::Carancho,
        };
        let seed = spot_seed(raptor.center);
        chunk.raptors.push((raptor, spawn(animal, seed)));
    }
    for duck in share.ducks {
        chunk.ducks.push((duck, spawn(Animal::Duck, 0)));
    }
    for (critter, id) in share.critters.into_iter().zip(share.critter_ids) {
        let seed = spot_seed(critter.position);
        let entity = spawn(critter_animal(critter.kind), seed);
        chunk.critters.push((critter, entity));
        chunk.critter_ids.push(id);
    }
    for nutria in share.nutrias {
        chunk.nutrias.push((nutria, spawn(Animal::Nutria, 0)));
    }
    for horse in share.horses {
        world.0.obstacles.push(horse.footprint);
        let key = horse.footprint.min.x.to_bits() ^ horse.footprint.min.y.to_bits();
        let height = world.0.ground_height(horse.position);
        let entity = commands
            .spawn((
                Name::new("Corral horse"),
                Mountable(horse),
                Animal::Horse(Coat::seeded(key)),
                AnimalPose {
                    heading: horse.heading,
                    pitch: slope_pitch(&world.0, horse.position, horse.heading),
                    ..default()
                },
                AnimalSeed(key),
                Transform::from_translation(world_from_ground(horse.position, height)),
            ))
            .id();
        chunk.horses.push((horse, entity));
    }
    chunk
}

/// Hace avanzar a cada animal vivo con el jugador como amenaza, y mantiene al
/// jugador apartado del ganado.
/// El terreno de los animales de suelo: el pasto abierto de la fauna, la
/// franja de sus caminos y los alambrados del mundo (sus obstáculos bajos, de
/// [`FenceLayout::HEIGHT`] o menos, incluidas las tranqueras cerradas).
struct Wilds<'a> {
    site: &'a FaunaSite,
    world: &'a World,
}

impl Land for Wilds<'_> {
    fn open(&self, p: GroundVec2) -> bool {
        self.site.grazeable(p)
    }

    fn road(&self, p: GroundVec2) -> bool {
        self.site.road_strip(p)
    }

    fn wire(&self, from: GroundVec2, to: GroundVec2) -> Option<f32> {
        let area = Aabb::new(
            GroundVec2::new(from.x.min(to.x), from.y.min(to.y)),
            GroundVec2::new(from.x.max(to.x), from.y.max(to.y)),
        );
        let obstacles = &self.world.obstacles;
        let tall = obstacles.near_above(area, FenceLayout::HEIGHT);
        obstacles
            .near(area)
            .into_iter()
            .filter(|o| !tall.iter().any(|t| std::ptr::eq(*t, *o)))
            .filter_map(|o| o.segment_overlap(from, to).map(|(enter, _)| enter))
            .min_by(f32::total_cmp)
    }
}

/// Cómo se dibuja lo que está haciendo un animal de suelo.
fn critter_gait(critter: &Critter) -> Gait {
    match critter.state {
        _ if critter.airborne() => Gait::Leap,
        CritterState::Rest if critter.kind == CritterKind::Nandu => Gait::Graze,
        CritterState::Rest | CritterState::Den => Gait::Stand,
        CritterState::Walk => Gait::Walk,
        CritterState::Flee => Gait::Run,
    }
}

/// Cuánto se mide adelante y atrás de un animal la pendiente con la que
/// cabecea, en metros.
const SLOPE_REACH: f32 = 0.5;

/// El cabeceo de un animal parado en `p` mirando hacia `heading`: la pendiente
/// del suelo a lo largo de su rumbo, en radianes (positivo: cuesta arriba). Con
/// el cuerpo inclinado así, las patas de adelante y de atrás pisan el suelo en
/// vez de hundirse cuesta arriba y flotar cuesta abajo.
fn slope_pitch(world: &World, p: GroundVec2, heading: Heading) -> f32 {
    let f = heading.forward() * SLOPE_REACH;
    let rise = world.ground_height(p + f) - world.ground_height(p - f);
    (rise / (2.0 * SLOPE_REACH)).atan()
}

fn step_fauna(
    time: Res<Time>,
    site: Option<Res<WorldFauna>>,
    mut fauna: ResMut<Fauna>,
    world: Res<GameWorld>,
    mut players: Players,
    flora: Option<Res<WorldFlora>>,
) {
    let Some(site) = site else { return };
    let dt = time.delta_secs().min(MAX_STEP);
    let mut player = players.single_mut().ok();
    let threat = player.as_ref().map(|(p, _)| p.rider.position);
    let mut bodies = Vec::new();
    let fauna = &mut *fauna;
    for chunk in fauna.chunks.values_mut().chain([&mut fauna.strays]) {
        for (herd, _) in &mut chunk.herds {
            herd.step(dt, threat, &world.0.obstacles);
            if threat.is_some_and(|t| herd.center().distance(t) < BODY_REACH) {
                bodies.extend(herd.bodies());
            }
        }
        // Las liebres y los animales de suelo corren y los teros se posan solo
        // sobre pasto abierto (ver `FaunaSite::grazeable`), dentro de su propio
        // chunk.
        let open = |p| site.0.grazeable(p);
        chunk
            .hares
            .iter_mut()
            .for_each(|h| h.0.step(dt, threat, open));
        chunk
            .flocks
            .iter_mut()
            .for_each(|f| f.0.step(dt, threat, open));
        // Los felinos pueden salir del monte, pero vuelven a él (ver
        // `Critter::step_in`); el zorro y los felinos saltan los alambrados y,
        // huyendo, cruzan los caminos (`Critter::step_on`).
        let land = Wilds {
            site: &site.0,
            world: &world.0,
        };
        chunk.critters.iter_mut().for_each(|c| {
            let feline = c.0.kind.feline();
            let monte = |p| {
                !feline
                    || flora
                        .as_ref()
                        .is_none_or(|f| f.0.monte(p) >= FaunaSite::MONTE)
            };
            c.0.step_on(dt, threat, &land, monte)
        });
        chunk.raptors.iter_mut().for_each(|r| r.0.step(dt));
        for (duck, _) in chunk.ducks.iter_mut().chain(&mut chunk.nutrias) {
            if let Some(lagoon) = site.0.lagoons().get(duck.lagoon) {
                duck.step(dt, lagoon, threat);
            }
        }
    }
    if let Some((player, transform)) = player.as_mut().filter(|_| !bodies.is_empty()) {
        player.rider.keep_clear_of(&world.0, &bodies);
        transform.translation = world_from_ground(player.rider.position, player.rider.elevation());
    }
}

/// Copia el estado de cada animal en la entidad que lo dibuja.
fn place_fauna(
    site: Option<Res<WorldFauna>>,
    fauna: Res<Fauna>,
    world: Res<GameWorld>,
    mut drawn: Query<(&mut Transform, &mut AnimalPose)>,
) {
    let Some(site) = site else { return };
    let ground = |p: GroundVec2| world.0.ground_height(p);
    let lean = |p: GroundVec2, heading: Heading| slope_pitch(&world.0, p, heading);
    let mut put = |e: Entity, at: Vec3, pose: AnimalPose| {
        if let Ok((mut transform, mut drawn)) = drawn.get_mut(e) {
            transform.translation = at;
            *drawn = pose;
        }
    };
    for chunk in fauna.chunks.values().chain([&fauna.strays]) {
        for (herd, entities) in &chunk.herds {
            for (cow, &e) in herd.cows.iter().zip(entities) {
                let gait = match cow.state {
                    CowState::Graze => Gait::Graze,
                    CowState::Stand => Gait::Stand,
                    CowState::Walk => Gait::Walk,
                };
                let pose = AnimalPose {
                    heading: cow.heading,
                    gait,
                    speed: cow.speed,
                    bank: 0.0,
                    pitch: lean(cow.position, cow.heading),
                };
                put(
                    e,
                    world_from_ground(cow.position, ground(cow.position)),
                    pose,
                );
            }
        }
        for (hare, e) in &chunk.hares {
            let gait = match hare.state {
                HareState::Sit => Gait::Stand,
                HareState::Hop => Gait::Walk,
                HareState::Flee => Gait::Run,
            };
            let pose = AnimalPose {
                heading: hare.heading,
                gait,
                speed: hare.speed,
                bank: 0.0,
                pitch: lean(hare.position, hare.heading),
            };
            let at = world_from_ground(hare.position, ground(hare.position) + hare.lift());
            put(*e, at, pose);
        }
        for (flock, entities) in &chunk.flocks {
            let gait = if flock.flying() {
                Gait::Fly
            } else {
                Gait::Graze
            };
            for ((p, lift), &e) in flock.birds().zip(entities) {
                let pose = AnimalPose {
                    heading: flock.heading(),
                    gait,
                    speed: 0.0,
                    bank: 0.0,
                    pitch: 0.0,
                };
                put(e, world_from_ground(p, ground(p) + lift), pose);
            }
        }
        for (raptor, e) in &chunk.raptors {
            let p = raptor.position();
            let pose = AnimalPose {
                heading: raptor.heading(),
                gait: Gait::Glide,
                speed: 0.0,
                bank: raptor.bank(),
                pitch: 0.0,
            };
            put(*e, world_from_ground(p, ground(p) + raptor.altitude), pose);
        }
        for (critter, e) in &chunk.critters {
            let pose = AnimalPose {
                heading: critter.heading,
                gait: critter_gait(critter),
                speed: critter.speed,
                bank: 0.0,
                pitch: lean(critter.position, critter.heading),
            };
            let p = critter.position;
            put(*e, world_from_ground(p, ground(p) + critter.lift()), pose);
        }
        for (duck, e) in chunk.ducks.iter().chain(&chunk.nutrias) {
            let level = site.0.lagoons().get(duck.lagoon).map_or(0.0, |l| l.level);
            let pose = AnimalPose {
                heading: duck.heading,
                gait: Gait::Swim,
                speed: 0.0,
                bank: 0.0,
                pitch: 0.0,
            };
            put(*e, world_from_ground(duck.position, level), pose);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use pampa_core::{Rider, World};
    use std::time::Duration;

    /// La fauna del mundo incluido, sin interfaz, con el jugador en `at`.
    fn app(at: GroundVec2) -> (App, FaunaSite) {
        let laid = crate::shipped_world::laid();
        let site = laid.fauna.clone();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                50,
            )))
            .insert_resource(GameWorld(laid.world.clone()))
            .insert_resource(WorldFauna(laid.fauna.clone()))
            .insert_resource(WorldFlora(laid.flora.clone()))
            .add_plugins(FaunaPlugin);
        let rider = Rider::on_foot(at, pampa_core::Heading::default(), &laid.world);
        app.world_mut()
            .spawn((Player { rider }, Transform::default()));
        app.update();
        (app, site)
    }

    /// El primer felino sembrado en `den`, en la lista de su chunk o suelto.
    fn feline_from(fauna: &mut Fauna, den: ChunkCoord) -> &mut (Critter, Entity) {
        let Fauna {
            chunks,
            strays,
            stray_from,
            ..
        } = fauna;
        let loose = strays.critters.iter_mut().zip(stray_from.iter());
        chunks
            .get_mut(&den)
            .into_iter()
            .flat_map(|c| c.critters.iter_mut())
            .chain(loose.filter(|(_, f)| **f == den).map(|(c, _)| c))
            .find(|(c, _)| c.kind.feline())
            .expect("a feline from that chunk")
    }

    fn moved(app: &mut App, to: GroundVec2) {
        let world: World = app.world().resource::<GameWorld>().0.clone();
        let mut q = app.world_mut().query::<&mut Player>();
        for mut p in q.iter_mut(app.world_mut()) {
            p.rider = Rider::on_foot(to, p.rider.heading, &world);
        }
        app.update();
        app.update();
    }

    #[test]
    fn a_chased_puma_jumps_a_field_fence_of_the_shipped_world_and_is_drawn_leaping() {
        let laid = crate::shipped_world::laid();
        let land = Wilds {
            site: &laid.fauna,
            world: &laid.world,
        };
        // Un tramo de alambrado de la cuadrícula con pasto abierto a los dos
        // lados.
        let (mid, across) = laid
            .fences
            .runs
            .iter()
            .filter(|r| !r.roadside)
            .flat_map(|r| r.spans())
            .find_map(|(a, b)| {
                let along = (b - a) * (1.0 / a.distance(b));
                let across = GroundVec2::new(-along.y, along.x);
                let mid = (a + b) * 0.5;
                (-30..=30)
                    .all(|k| land.open(mid + across * k as f32))
                    .then_some((mid, across))
            })
            .expect("a field fence in open country");
        // Lo ve a través, no a lo largo.
        let at = land.wire(mid - across * 2.0, mid + across * 2.0);
        assert!(at.is_some_and(|f| (f - 0.5).abs() < 0.1), "{at:?}");
        assert_eq!(land.wire(mid + across, mid + across * 3.0), None);
        // Un jinete corre al puma contra el alambrado.
        let home = Aabb::new(
            mid - GroundVec2::new(500.0, 500.0),
            mid + GroundVec2::new(500.0, 500.0),
        );
        let mut puma = Critter::new(CritterKind::Puma, mid - across * 25.0, home, None, 1);
        let side = |c: &Critter| (c.position - mid).dot(across);
        let mut leaped = false;
        for _ in 0..200 {
            let rider = puma.position - across * 15.0;
            let before = side(&puma);
            puma.step_on(0.05, Some(rider), &land, |_| true);
            if before < 0.0 && side(&puma) >= 0.0 {
                // Lo más bajo de su cuerpo, con las patas recogidas.
                let tuck = CritterKind::Puma.leap().unwrap().tuck;
                assert!(puma.lift() + tuck > FenceLayout::HEIGHT, "over the wire");
            }
            leaped |= critter_gait(&puma) == Gait::Leap;
            assert_eq!(critter_gait(&puma) == Gait::Leap, puma.airborne());
        }
        assert!(leaped && side(&puma) > 10.0, "{}", side(&puma));
    }

    #[test]
    fn a_feline_on_a_slope_leans_with_the_ground_along_its_heading() {
        let laid = crate::shipped_world::laid();
        let (mut app, site) = app(GroundVec2::ZERO);
        let world = &laid.world;
        let config = site.config();
        let size = config.chunk_size;
        let reach = (config.half_extent / size) as i32;
        let den = config
            .chunks_near(GroundVec2::ZERO, reach)
            .into_iter()
            .find(|&c| {
                site.critters_in(c, |p| laid.flora.monte(p))
                    .iter()
                    .any(|k| k.kind.feline())
            })
            .expect("a feline in some monte");
        moved(&mut app, den.center(size));
        // La pendiente del suelo a lo largo de `heading`, de medio metro atrás
        // a medio metro adelante.
        let rise = |p: GroundVec2, heading: pampa_core::Heading| {
            let f = heading.forward() * 0.5;
            world.ground_height(p + f) - world.ground_height(p - f)
        };
        let east = pampa_core::Heading::default();
        let slope = (0..4000)
            .map(|k| {
                den.center(size) + GroundVec2::new((k % 63) as f32 * 1.3, (k / 63) as f32 * 1.1)
            })
            .find(|&p| rise(p, east) > 0.06)
            .expect("a slope up to the east");
        let entity = {
            let mut fauna = app.world_mut().resource_mut::<Fauna>();
            let (cat, e) = feline_from(&mut fauna, den);
            cat.position = slope;
            cat.heading = east;
            cat.speed = 0.0;
            *e
        };
        app.update();
        let fauna = app.world().resource::<Fauna>();
        let cat = fauna
            .chunks
            .get(&den)
            .into_iter()
            .flat_map(|c| c.critters.iter())
            .chain(fauna.strays.critters.iter())
            .find(|(_, e)| *e == entity)
            .map(|(c, _)| c.clone())
            .unwrap();
        let pose = *app.world().get::<AnimalPose>(entity).unwrap();
        let want = rise(cat.position, cat.heading).atan();
        assert!(want.abs() > 0.02, "still on the slope: {want}");
        assert!(
            (pose.pitch - want).abs() < 1e-4,
            "leans {} with a slope of {want}",
            pose.pitch
        );
    }

    #[test]
    fn fauna_streams_round_the_player_under_the_caps() {
        let (mut app, site) = app(GroundVec2::ZERO);
        let head: usize = site.herds().iter().map(|h| h.cows.len()).sum();
        println!("shipped world: {} herds, {head} head", site.herds().len());
        // Donde pasta más ganado, y luego junto a una laguna y a un corral.
        let busiest = site
            .herds()
            .iter()
            .max_by_key(|h| {
                site.herds()
                    .iter()
                    .filter(|o| o.pasture.center().distance(h.pasture.center()) < 500.0)
                    .map(|o| o.cows.len())
                    .sum::<usize>()
            })
            .unwrap()
            .pasture
            .center();
        let mut spots = vec![("herds", busiest)];
        if let Some(lagoon) = site.lagoons().first() {
            spots.push(("lagoon", lagoon.center));
        }
        if let Some(horse) = site.all_horses().next() {
            spots.push(("corral", horse.position));
        }
        for (name, at) in spots {
            moved(&mut app, at);
            let counts = app.world().resource::<Fauna>().all_counts();
            println!(
                "fauna near {name} {at:?}: {counts:?} (cows, horses, hares, teros, raptors, ducks, \
                 zorros, ñandúes, armadillos, vizcachas, felines, nutrias)"
            );
            for (n, cap) in counts.iter().zip(CAPS) {
                assert!(*n <= cap, "{counts:?} over {CAPS:?}");
            }
            let drawn = app.world_mut().query::<&Animal>().iter(app.world()).count();
            let horses = app
                .world_mut()
                .query_filtered::<&Animal, With<Mountable>>()
                .iter(app.world())
                .filter(|a| matches!(a, Animal::Horse(_)))
                .count();
            assert_eq!(drawn, counts.iter().sum::<usize>());
            // Cada tipo vivo se dibuja como su especie.
            let animals: Vec<Animal> = app
                .world_mut()
                .query::<&Animal>()
                .iter(app.world())
                .copied()
                .collect();
            let of = |kinds: &[Animal]| animals.iter().filter(|a| kinds.contains(a)).count();
            assert_eq!(of(&[Animal::Zorro]), counts[6]);
            assert_eq!(of(&[Animal::Nandu]), counts[7]);
            assert_eq!(of(&[Animal::Mulita, Animal::Peludo]), counts[8]);
            assert_eq!(of(&[Animal::Vizcacha]), counts[9]);
            assert_eq!(of(&[Animal::Nutria]), counts[11]);
            assert_eq!(of(&[Animal::Chimango, Animal::Carancho]), counts[4]);
            assert_eq!(horses, counts[1], "corral horses are fauna");
            match name {
                "herds" => assert!(counts[0] > 0),
                "lagoon" => assert!(counts[5] > 0 && counts[11] > 0, "ducks and nutrias"),
                _ => assert!(counts[1] > 0, "horses in the corral"),
            }
        }
        // Lejos, no queda nada de lo que estaba cargado antes.
        moved(&mut app, GroundVec2::new(8000.0, 8000.0));
        let counts = app.world().resource::<Fauna>().all_counts();
        assert_eq!(counts[0..2], [0, 0], "{counts:?}");
    }

    #[test]
    fn a_wild_animal_led_out_of_its_chunk_range_stays_with_the_player_and_is_not_reseeded() {
        let laid = crate::shipped_world::laid();
        let (mut app, site) = app(GroundVec2::ZERO);
        let config = site.config();
        let size = config.chunk_size;
        let reach = (config.half_extent / size) as i32;
        let monte = |p| laid.flora.monte(p);
        let den = config
            .chunks_near(GroundVec2::ZERO, reach)
            .into_iter()
            .find(|&c| {
                config.has_chunk(ChunkCoord::new(c.x + RADIUS + 1, c.y))
                    && site.critters_in(c, monte).iter().any(|k| k.kind.feline())
            })
            .expect("a feline in some monte");
        moved(&mut app, den.center(size));
        // El jugador lo persigue: el felino va adelante, lejos de su chunk.
        let far = ChunkCoord::new(den.x + RADIUS + 1, den.y).center(size);
        let ahead = far + GroundVec2::new(15.0, 0.0);
        let (kind, seed, entity) = {
            let mut fauna = app.world_mut().resource_mut::<Fauna>();
            let (cat, e) = feline_from(&mut fauna, den);
            let seed = cat.position;
            cat.position = ahead;
            (cat.kind, seed, *e)
        };
        let near = |app: &App, at: GroundVec2| {
            let fauna = app.world().resource::<Fauna>();
            fauna
                .critters()
                .filter(|&(k, p)| k == kind && p.distance(at) < 1.0)
                .count()
        };
        moved(&mut app, far);
        assert!(!app.world().resource::<Fauna>().chunks.contains_key(&den));
        assert_eq!(near(&app, ahead), 1, "still with the player");
        assert!(app.world().get_entity(entity).is_ok(), "still drawn");
        // De vuelta a un chunk desde el que su chunk se carga otra vez: no
        // aparece un segundo igual en su lugar de nacimiento.
        moved(
            &mut app,
            ChunkCoord::new(den.x + RADIUS, den.y).center(size),
        );
        assert!(app.world().resource::<Fauna>().chunks.contains_key(&den));
        assert_eq!(near(&app, ahead), 1, "still with the player");
        assert_eq!(near(&app, seed), 0, "not reseeded while alive");
    }

    #[test]
    fn a_wild_animal_near_the_player_keeps_its_place_when_a_nearer_chunk_wants_it() {
        let laid = crate::shipped_world::laid();
        let (mut app, site) = app(GroundVec2::ZERO);
        let config = site.config();
        let size = config.chunk_size;
        let reach = (config.half_extent / size) as i32;
        let monte = |p| laid.flora.monte(p);
        // Dos chunks cercanos con un felino cada uno, y lugar para uno solo.
        let dens: Vec<ChunkCoord> = config
            .chunks_near(GroundVec2::ZERO, reach)
            .into_iter()
            .filter(|&c| site.critters_in(c, monte).iter().any(|k| k.kind.feline()))
            .collect();
        let (den, other) = dens
            .iter()
            .flat_map(|&a| dens.iter().map(move |&b| (a, b)))
            .find(|(a, b)| a != b && a.distance_squared(*b) <= i64::from(RADIUS).pow(2))
            .expect("two felines near each other");
        let mut caps = CAPS;
        caps[10] = 1;
        app.insert_resource(FaunaCaps(caps));
        moved(&mut app, den.center(size));
        // El jugador lo persigue hasta el otro chunk, que ahora queda más
        // cerca y se quedaría con el único lugar.
        let there = other.center(size);
        let ahead = there + GroundVec2::new(15.0, 0.0);
        let kind = {
            let mut fauna = app.world_mut().resource_mut::<Fauna>();
            let (cat, _) = feline_from(&mut fauna, den);
            cat.position = ahead;
            cat.kind
        };
        moved(&mut app, there);
        let fauna = app.world().resource::<Fauna>();
        let with_player = fauna
            .critters()
            .filter(|&(k, p)| k == kind && p.distance(ahead) < 1.0)
            .count();
        assert_eq!(with_player, 1, "still with the player");
        assert_eq!(fauna.all_counts()[10], 1, "under the cap");
    }

    #[test]
    fn wild_ground_animals_stream_in_their_habitat_and_felines_only_in_the_monte() {
        let laid = crate::shipped_world::laid();
        let (mut app, site) = app(GroundVec2::ZERO);
        let config = site.config();
        let monte = |p| laid.flora.monte(p);
        let reach = (config.half_extent / config.chunk_size) as i32;
        // El chunk con un felino más cercano al centro del mundo.
        let den = config
            .chunks_near(GroundVec2::ZERO, reach)
            .into_iter()
            .find(|&c| site.critters_in(c, monte).iter().any(|k| k.kind.feline()))
            .expect("a feline in some monte");
        moved(&mut app, den.center(config.chunk_size));
        let counts = app.world().resource::<Fauna>().all_counts();
        println!("fauna near the monte {den:?}: {counts:?}");
        for (n, cap) in counts.iter().zip(CAPS) {
            assert!(*n <= cap, "{counts:?} over {CAPS:?}");
        }
        assert!(counts[10] > 0, "a feline");
        let drawn: Vec<Animal> = app
            .world_mut()
            .query::<&Animal>()
            .iter(app.world())
            .copied()
            .collect();
        let of = |kinds: &[Animal]| drawn.iter().filter(|a| kinds.contains(a)).count();
        let felines = [Animal::GatoMontes, Animal::Puma, Animal::Yaguarete];
        assert_eq!(of(&felines), counts[10]);
        // Cada animal de suelo vivo está sobre pasto abierto, y los felinos en
        // el monte.
        let fauna = app.world().resource::<Fauna>();
        for chunk in fauna.chunks.values() {
            for (critter, _) in &chunk.critters {
                assert!(site.grazeable(critter.position));
                if critter.kind.feline() {
                    assert!(monte(critter.position) >= FaunaSite::MONTE);
                }
            }
        }
        // Sin flora no hay monte, ni felinos.
        app.world_mut().remove_resource::<WorldFlora>();
        moved(&mut app, GroundVec2::new(8000.0, 8000.0));
        moved(&mut app, den.center(config.chunk_size));
        assert_eq!(app.world().resource::<Fauna>().all_counts()[10], 0);
    }

    #[test]
    fn a_chunk_loaded_short_under_a_full_cap_fills_up_as_the_player_nears() {
        let (mut app, site) = app(GroundVec2::ZERO);
        let size = site.config().chunk_size;
        let teros =
            |c: ChunkCoord| -> usize { site.flocks_in(c).iter().map(|f| f.birds().count()).sum() };
        let chunks: Vec<ChunkCoord> = ChunkCoord::new(0, 0)
            .within(10)
            .into_iter()
            .filter(|&c| teros(c) > 0)
            .collect();
        // Dos chunks de teros dentro del radio de carga uno del otro.
        let (near, edge) = chunks
            .iter()
            .flat_map(|&a| chunks.iter().map(move |&b| (a, b)))
            .find(|(a, b)| {
                a != b && (1..=i64::from(RADIUS).pow(2)).contains(&a.distance_squared(*b))
            })
            .expect("two tero chunks near each other");
        // Lugar exactamente para la bandada del chunk del borde, planificado de
        // nuevo desde lejos.
        moved(&mut app, GroundVec2::new(8000.0, 8000.0));
        app.world_mut().resource_mut::<FaunaCaps>().0[3] = teros(edge);
        let loaded = |app: &App, c: ChunkCoord| {
            let fauna = app.world().resource::<Fauna>();
            fauna.chunks.get(&c).map_or(0, |f| f.counts()[3])
        };
        moved(&mut app, near.center(size));
        println!(
            "teros: near {near:?} has {}, edge {edge:?} has {}: loaded {}/{}",
            teros(near),
            teros(edge),
            loaded(&app, near),
            loaded(&app, edge)
        );
        assert!(
            loaded(&app, edge) < teros(edge),
            "the nearer chunk took the room"
        );
        moved(&mut app, edge.center(size));
        assert_eq!(loaded(&app, edge), teros(edge), "now nearest, it fills up");
        let drawn = app
            .world_mut()
            .query::<&Animal>()
            .iter(app.world())
            .filter(|a| **a == Animal::Tero)
            .count();
        assert_eq!(drawn, teros(edge), "nothing left over past the cap");
    }

    #[test]
    fn a_chunk_holding_other_animals_than_planned_is_drawn_again() {
        let (mut app, site) = app(GroundVec2::ZERO);
        let size = site.config().chunk_size;
        let herd = site.herds()[0].clone();
        let home = ChunkCoord::containing(herd.pasture.center(), size);
        moved(&mut app, home.center(size));
        let cows = |app: &mut App| -> Vec<Entity> {
            let fauna = app.world().resource::<Fauna>();
            fauna.chunks[&home]
                .herds
                .iter()
                .flat_map(|h| h.1.clone())
                .collect()
        };
        let before = cows(&mut app);
        assert!(!before.is_empty());
        // Los mismos totales, otro contenido: como si se hubiera elegido otra
        // manada del mismo tamaño.
        {
            let mut fauna = app.world_mut().resource_mut::<Fauna>();
            let chunk = fauna.chunks.get_mut(&home).unwrap();
            chunk.picks[0] = chunk.picks[0].iter().map(|id| id + 1_000_000).collect();
        }
        // Un chunk más allá (el de origen todavía al alcance), y el plan se
        // ejecuta.
        moved(&mut app, ChunkCoord::new(home.x + 1, home.y).center(size));
        let after = cows(&mut app);
        assert_eq!(after.len(), before.len());
        assert!(after.iter().all(|e| !before.contains(e)), "drawn again");
        let fauna = app.world().resource::<Fauna>();
        let ids: Vec<u64> = fauna.chunks[&home]
            .herds
            .iter()
            .map(|h| u64::from(h.0.id))
            .collect();
        assert_eq!(fauna.chunks[&home].picks[0], ids);
        // Un chunk sin cambios conserva sus animales tal como están.
        let kept = cows(&mut app);
        moved(&mut app, home.center(size));
        assert_eq!(cows(&mut app), kept);
    }

    #[test]
    fn a_taken_corral_horse_is_not_seeded_again() {
        let site = &crate::shipped_world::laid().fauna;
        let horse = *site.all_horses().next().expect("a corral horse");
        let (mut app, _) = app(horse.position + GroundVec2::new(0.0, 30.0));
        app.update();
        let mut q = app.world_mut().query::<(Entity, &Mountable)>();
        let mine: Vec<Entity> = q
            .iter(app.world())
            .filter(|(_, m)| m.0 == horse)
            .map(|(e, _)| e)
            .collect();
        assert_eq!(mine.len(), 1);
        assert!(
            app.world()
                .resource::<GameWorld>()
                .0
                .obstacles
                .contains(&horse.footprint)
        );
        // Tomado (como lo hace la montura del encargo): se va con su huella en
        // planta.
        app.world_mut()
            .resource_mut::<GameWorld>()
            .0
            .obstacles
            .remove(&horse.footprint);
        app.world_mut().despawn(mine[0]);
        app.update();
        moved(&mut app, GroundVec2::new(8000.0, 8000.0));
        moved(&mut app, horse.position + GroundVec2::new(0.0, 30.0));
        let mut q = app.world_mut().query::<&Mountable>();
        assert!(q.iter(app.world()).all(|m| m.0 != horse));
    }
}
