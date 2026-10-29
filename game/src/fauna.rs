//! The animals round the player: `pampa-core` seeds them
//! ([`FaunaSite`]) and steps them; this module streams them with the chunks
//! within [`RADIUS`] of the player, nearest first and up to the
//! [`caps`] (re-planned each time the player enters another chunk, so
//! nearer chunks take the room first), steps them with the player as the threat they shy from, and
//! copies their state onto the entities render draws ([`Animal`]).
//!
//! Cattle are soft bodies: the player is kept clear of them, and walks
//! them off. The horses standing in the corrals are [`Mountable`] (once
//! riding is unlocked, any horse is) and obstacles while they stand; one
//! taken stays the player's and is not seeded again this session.

use std::collections::{BTreeMap, HashSet};

use bevy::prelude::*;
use pampa_core::fauna::caps;
use pampa_core::{
    ChunkCoord, CowState, Duck, FaunaSite, Flock, Hare, HareState, Herd, MAX_STEP, Raptor,
    TetheredHorse, Vec2 as GroundVec2,
};
use pampa_render::{Animal, AnimalPose, AnimalSeed, Coat, Gait, world_from_ground};

use crate::errand::Mountable;
use crate::player::{DrivePlayer, GameWorld, Player, drive_player};
use crate::ride::OnTrain;

/// Chunks out from the player's that hold live animals.
pub const RADIUS: i32 = 2;
/// Herds farther than this from the player are not checked against the
/// rider, meters.
const BODY_REACH: f32 = 40.0;

/// Where the world's animals live.
#[derive(Resource, Debug, Clone)]
pub struct WorldFauna(pub FaunaSite);

/// The animals of one streamed chunk, each with the entities drawing it.
#[derive(Debug, Default)]
struct ChunkFauna {
    herds: Vec<(Herd, Vec<Entity>)>,
    horses: Vec<(TetheredHorse, Entity)>,
    hares: Vec<(Hare, Entity)>,
    flocks: Vec<(Flock, Vec<Entity>)>,
    raptors: Vec<(Raptor, Entity)>,
    ducks: Vec<(Duck, Entity)>,
    /// Which of the chunk's seeded animals it holds (see [`Picks`]).
    picks: Picks,
}

/// Which of a chunk's seeded animals are drawn, by kind in
/// [`Fauna::counts`] order: herds by id, corral horses by their
/// footprint ([`horse_key`]), the rest by their place in the chunk's
/// seeded list. Two plans with equal picks hold the very same animals.
type Picks = [Vec<u64>; 6];

impl ChunkFauna {
    /// Cows, horses, hares, teros, raptors and ducks.
    fn counts(&self) -> [usize; 6] {
        [
            self.herds.iter().map(|h| h.0.cows.len()).sum(),
            self.horses.len(),
            self.hares.len(),
            self.flocks.iter().map(|f| f.1.len()).sum(),
            self.raptors.len(),
            self.ducks.len(),
        ]
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
    }
}

/// The animals alive round the player now.
#[derive(Resource, Debug, Default)]
pub struct Fauna {
    center: Option<ChunkCoord>,
    chunks: BTreeMap<ChunkCoord, ChunkFauna>,
    /// Corral horses the player took, by their footprint's corner.
    taken: HashSet<(u32, u32)>,
}

impl Fauna {
    /// Cows, horses, hares, teros, raptors and ducks alive now.
    pub fn counts(&self) -> [usize; 6] {
        self.chunks.values().fold([0; 6], |mut sum, c| {
            for (s, n) in sum.iter_mut().zip(c.counts()) {
                *s += n;
            }
            sum
        })
    }
}

/// The most of each kind, in [`Fauna::counts`] order.
pub const CAPS: [usize; 6] = [
    caps::COWS,
    caps::HORSES,
    caps::HARES,
    caps::TEROS,
    caps::RAPTORS,
    caps::DUCKS,
];

/// The caps in force: [`CAPS`] unless a test lowers them.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct FaunaCaps(pub [usize; 6]);

impl Default for FaunaCaps {
    fn default() -> Self {
        Self(CAPS)
    }
}

fn horse_key(h: &TetheredHorse) -> (u32, u32) {
    (h.footprint.min.x.to_bits(), h.footprint.min.y.to_bits())
}

/// A horse's [`horse_key`] as one pick.
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

/// Where the player stands, unless aboard the train.
type Players<'w, 's> =
    Query<'w, 's, (&'static mut Player, &'static mut Transform), Without<OnTrain>>;

/// Re-plans the animals round the player each time they enter another
/// chunk: the chunks within [`RADIUS`], nearest first, each take what of
/// theirs still fits under the [`CAPS`]. A loaded chunk whose share is
/// unchanged keeps its animals as they are; one whose share changed (a
/// nearer chunk now takes the room, or room freed up) is drawn again with
/// its new share; chunks out of range go.
fn stream_fauna(
    mut commands: Commands,
    site: Option<Res<WorldFauna>>,
    mut fauna: ResMut<Fauna>,
    mut world: ResMut<GameWorld>,
    players: Query<&Player>,
    (standing, caps): (Query<(), With<Mountable>>, Res<FaunaCaps>),
) {
    let (Some(site), Ok(player)) = (site, players.single()) else {
        return;
    };
    // A horse gone from its corral was taken: never seed it again.
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
    let mut room = caps.0;
    let mut plan = BTreeMap::new();
    for &coord in &wanted {
        plan.insert(coord, select(&site.0, coord, &mut room, &fauna.taken));
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

/// What of a chunk's animals fits under the caps, before it is drawn.
#[derive(Debug, Default)]
struct Share {
    herds: Vec<Herd>,
    horses: Vec<TetheredHorse>,
    hares: Vec<Hare>,
    flocks: Vec<Flock>,
    raptors: Vec<Raptor>,
    ducks: Vec<Duck>,
    picks: Picks,
}

/// The animals of `coord` that fit in the `room` left under the caps
/// (whole herds and flocks only), leaving out the horses `taken`.
fn select(
    site: &FaunaSite,
    coord: ChunkCoord,
    room: &mut [usize; 6],
    taken: &HashSet<(u32, u32)>,
) -> Share {
    /// The `items` that fit in `room`, in order, with their picks.
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
    /// The pick of a kind without an id: its place in the seeded list.
    fn index<T>(k: usize, _: &T) -> u64 {
        k as u64
    }
    let free = site
        .horses_in(coord)
        .into_iter()
        .filter(|h| !taken.contains(&horse_key(h)))
        .collect();
    let mut picks = Picks::default();
    let [cows, horses, hares, teros, raptors, ducks] = &mut picks;
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
        picks,
    }
}

/// Spawns the entities drawing a chunk's `share`; its corral horses
/// stand as obstacles.
fn spawn_share(commands: &mut Commands, share: Share, world: &mut GameWorld) -> ChunkFauna {
    // Only the livestock models read the seed (their idles and offsets).
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
        chunk.raptors.push((raptor, spawn(Animal::Raptor, 0)));
    }
    for duck in share.ducks {
        chunk.ducks.push((duck, spawn(Animal::Duck, 0)));
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

/// Steps every live animal with the player as the threat, and keeps the
/// player clear of the cattle.
fn step_fauna(
    time: Res<Time>,
    site: Option<Res<WorldFauna>>,
    mut fauna: ResMut<Fauna>,
    world: Res<GameWorld>,
    mut players: Players,
) {
    let Some(site) = site else { return };
    let dt = time.delta_secs().min(MAX_STEP);
    let mut player = players.single_mut().ok();
    let threat = player.as_ref().map(|(p, _)| p.rider.position);
    let mut bodies = Vec::new();
    for chunk in fauna.chunks.values_mut() {
        for (herd, _) in &mut chunk.herds {
            herd.step(dt, threat, &world.0.obstacles);
            if threat.is_some_and(|t| herd.center().distance(t) < BODY_REACH) {
                bodies.extend(herd.bodies());
            }
        }
        // Hares run and teros land only on open grass (see
        // `FaunaSite::grazeable`), inside their own chunk.
        let open = |p| site.0.grazeable(p);
        chunk
            .hares
            .iter_mut()
            .for_each(|h| h.0.step(dt, threat, open));
        chunk
            .flocks
            .iter_mut()
            .for_each(|f| f.0.step(dt, threat, open));
        chunk.raptors.iter_mut().for_each(|r| r.0.step(dt));
        for (duck, _) in &mut chunk.ducks {
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

/// Copies each animal's state onto the entity drawing it.
fn place_fauna(
    site: Option<Res<WorldFauna>>,
    fauna: Res<Fauna>,
    world: Res<GameWorld>,
    mut drawn: Query<(&mut Transform, &mut AnimalPose)>,
) {
    let Some(site) = site else { return };
    let ground = |p: GroundVec2| world.0.ground_height(p);
    let mut put = |e: Entity, at: Vec3, pose: AnimalPose| {
        if let Ok((mut transform, mut drawn)) = drawn.get_mut(e) {
            transform.translation = at;
            *drawn = pose;
        }
    };
    for chunk in fauna.chunks.values() {
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
            };
            put(*e, world_from_ground(p, ground(p) + raptor.altitude), pose);
        }
        for (duck, e) in &chunk.ducks {
            let level = site.0.lagoons().get(duck.lagoon).map_or(0.0, |l| l.level);
            let pose = AnimalPose {
                heading: duck.heading,
                gait: Gait::Swim,
                speed: 0.0,
                bank: 0.0,
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

    /// The shipped world's fauna, headless, with the player at `at`.
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
            .add_plugins(FaunaPlugin);
        let rider = Rider::on_foot(at, pampa_core::Heading::default(), &laid.world);
        app.world_mut()
            .spawn((Player { rider }, Transform::default()));
        app.update();
        (app, site)
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
    fn fauna_streams_round_the_player_under_the_caps() {
        let (mut app, site) = app(GroundVec2::ZERO);
        let head: usize = site.herds().iter().map(|h| h.cows.len()).sum();
        println!("shipped world: {} herds, {head} head", site.herds().len());
        // Where the most cattle graze, then by a lagoon and a corral.
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
            let counts = app.world().resource::<Fauna>().counts();
            println!(
                "fauna near {name} {at:?}: {counts:?} (cows, horses, hares, teros, raptors, ducks)"
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
            assert_eq!(horses, counts[1], "corral horses are fauna");
            match name {
                "herds" => assert!(counts[0] > 0),
                "lagoon" => assert!(counts[5] > 0, "ducks on the lagoon"),
                _ => assert!(counts[1] > 0, "horses in the corral"),
            }
        }
        // Far out, nothing is left of what was loaded before.
        moved(&mut app, GroundVec2::new(8000.0, 8000.0));
        let counts = app.world().resource::<Fauna>().counts();
        assert_eq!(counts[0..2], [0, 0], "{counts:?}");
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
        // Two tero chunks within the load radius of each other.
        let (near, edge) = chunks
            .iter()
            .flat_map(|&a| chunks.iter().map(move |&b| (a, b)))
            .find(|(a, b)| {
                a != b && (1..=i64::from(RADIUS).pow(2)).contains(&a.distance_squared(*b))
            })
            .expect("two tero chunks near each other");
        // Room for exactly the edge chunk's flock, planned afresh from far
        // away.
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
        // Same totals, other content: as if a different herd of the same
        // size had been picked.
        {
            let mut fauna = app.world_mut().resource_mut::<Fauna>();
            let chunk = fauna.chunks.get_mut(&home).unwrap();
            chunk.picks[0] = chunk.picks[0].iter().map(|id| id + 1_000_000).collect();
        }
        // One chunk over (home still within reach), and the plan runs.
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
        // An unchanged chunk keeps its animals as they are.
        let kept = cows(&mut app);
        moved(&mut app, home.center(size));
        assert_eq!(cows(&mut app), kept);
    }

    #[test]
    fn a_taken_corral_horse_is_not_seeded_again() {
        let (_, site) = app(GroundVec2::ZERO);
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
        // Taken (as the errand's mount does): it goes with its footprint.
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
