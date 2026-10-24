//! Railway track: the pieces every track is drawn with, and the railway
//! line between towns, streamed with the terrain chunks.
//!
//! A town draws its own straight track under its [`crate::TownRoot`]. The
//! links of the [`RailLine`] between towns run for kilometers across open
//! pampa, so they are drawn chunk by chunk instead: when a terrain chunk is
//! built, the parts of the links inside it are laid as its children, and
//! they go when the chunk goes. Chunks are built a few per frame, so the
//! track keeps the terrain's per-frame budget.

use bevy::prelude::*;
use bevy::transform::TransformSystems;
use pampa_core::{RailLine, RailSpan, Terrain, Vec2 as GroundVec2};

use crate::coords::world_from_ground;
use crate::terrain::{ChunkStreaming, TerrainChunk, WorldChunks, WorldTerrain};

/// The railway line between the towns. Insert before the plugin runs;
/// without it no track is laid between towns.
#[derive(Resource, Debug, Clone, Default)]
pub struct WorldRail(pub RailLine);

/// One piece of the railway line between towns, child of the terrain chunk
/// it lies in.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineTrack;

/// Marks a terrain chunk whose share of the railway line was laid (even
/// when the line misses it), so it is only looked at once.
#[derive(Component, Debug, Clone, Copy)]
struct TrackLaid;

/// Mesh and materials shared by every track, in town and between towns.
/// The town kit inserts it (see `crate::town`).
#[derive(Resource, Debug, Clone)]
pub(crate) struct TrackKit {
    pub cube: Handle<Mesh>,
    pub ballast: Handle<StandardMaterial>,
    pub timber: Handle<StandardMaterial>,
    pub steel: Handle<StandardMaterial>,
}

impl TrackKit {
    pub(crate) fn material(&self, part: TrackPart) -> &Handle<StandardMaterial> {
        match part {
            TrackPart::Ballast => &self.ballast,
            TrackPart::Rail => &self.steel,
            TrackPart::Sleeper => &self.timber,
        }
    }
}

/// What a track piece is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrackPart {
    Ballast,
    Rail,
    Sleeper,
}

/// Length of one ballast and rail piece, in meters.
const PIECE: f32 = 6.0;
/// Distance between sleepers, in meters.
const SLEEPER_SPACING: f32 = 1.2;

/// Ballast bed, two rails and sleepers of the part of a track `span`
/// covers, following the ground: each piece of the unit cube, with its
/// transform.
///
/// Distances are measured along the whole track the span belongs to (a
/// town's straight track, or a curved link drawn as many straight pieces),
/// so sleepers sit where they would on the whole track: stretches drawn
/// apart, chunk by chunk or piece by piece, keep one even spacing.
pub(crate) fn track_pieces(span: RailSpan, terrain: &Terrain) -> Vec<(TrackPart, Transform)> {
    let track = span.track;
    let length = track.start.distance(track.end);
    let mut pieces = Vec::new();
    if length <= 0.0 {
        return pieces;
    }
    // Ballast and rails are cut along this straight piece.
    let enter = (span.enter - span.from).max(0.0);
    let exit = (span.exit - span.from).min(length);
    let along = (track.end - track.start) * (1.0 / length);
    // Left of the heading: `along` turned a quarter counter-clockwise.
    let across = GroundVec2::new(-along.y, along.x);
    let at = |s: f32, offset: f32| track.start + along * s + across * offset;

    let first = (enter / PIECE).floor() as usize;
    let last = (exit / PIECE).ceil() as usize;
    for k in first..last {
        let s0 = (k as f32 * PIECE).max(enter);
        let s1 = ((k + 1) as f32 * PIECE).min(exit);
        if s1 <= s0 {
            continue;
        }
        let ballast = stretched(
            terrain,
            at(s0, 0.0),
            at(s1, 0.0),
            -0.05,
            Vec2::new(3.4, 0.4),
        );
        pieces.extend(ballast.map(|t| (TrackPart::Ballast, t)));
        for side in [-0.5, 0.5] {
            let offset = side * track.gauge;
            let rail = stretched(
                terrain,
                at(s0, offset),
                at(s1, offset),
                0.36,
                Vec2::new(0.09, 0.14),
            );
            pieces.extend(rail.map(|t| (TrackPart::Rail, t)));
        }
    }

    // Sleepers lie every `SLEEPER_SPACING` along the whole track, half a
    // spacing in from its start, and stop half a spacing short of its end.
    let sleepers = (span.total / SLEEPER_SPACING) as usize;
    let first = ((span.enter - SLEEPER_SPACING / 2.0) / SLEEPER_SPACING)
        .ceil()
        .max(0.0) as usize;
    // A sleeper's long side is the cube's local Z. Ground north is Bevy -Z
    // (see `world_from_ground`), so a track heading `θ` counter-clockwise
    // from east runs along world (cos θ, 0, -sin θ), and turning local Z
    // about +Y by the same `θ` gives (sin θ, 0, cos θ): square to the track.
    // (Turning by `-θ` instead only looks right on east-west and
    // north-south track.)
    let yaw = along.y.atan2(along.x);
    for k in first..sleepers {
        let s = k as f32 * SLEEPER_SPACING + SLEEPER_SPACING / 2.0;
        if s >= span.exit {
            break;
        }
        let p = at(s - span.from, 0.0);
        let y = terrain.height(p.x, p.y) + 0.3;
        pieces.push((
            TrackPart::Sleeper,
            Transform::from_translation(world_from_ground(p, y))
                .with_rotation(Quat::from_rotation_y(yaw))
                .with_scale(Vec3::new(0.25, 0.12, track.gauge + 0.9)),
        ));
    }
    pieces
}

/// A beam from ground point `from` to `to`, raised `lift` above the ground,
/// with `section` (width, height) across it; `None` when it has no length.
pub(crate) fn stretched(
    terrain: &Terrain,
    from: GroundVec2,
    to: GroundVec2,
    lift: f32,
    section: Vec2,
) -> Option<Transform> {
    let wa = world_from_ground(from, terrain.height(from.x, from.y) + lift);
    let wb = world_from_ground(to, terrain.height(to.x, to.y) + lift);
    let d = wb - wa;
    let len = d.length();
    if len <= 0.0 {
        return None;
    }
    Some(
        Transform::from_translation((wa + wb) / 2.0 + Vec3::Y * section.y / 2.0)
            .with_rotation(Quat::from_rotation_arc(Vec3::X, d / len))
            .with_scale(Vec3::new(len + 0.02, section.y, section.x)),
    )
}

/// Lays the railway line's track on terrain chunks as they are built.
/// Needs the [`crate::TownPlugin`] for the shared track kit.
pub struct RailPlugin;

impl Plugin for RailPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldRail>()
            .init_resource::<WorldTerrain>()
            .init_resource::<WorldChunks>()
            // Right after chunk streaming, in the same schedule: the chunks
            // it built this frame are already spawned (Bevy applies its
            // commands before an ordered successor runs), so they get their
            // track the same frame, and the chunks it dropped are already
            // gone, so none is decorated after its despawn. Before transform
            // propagation, so new pieces are never drawn a frame at the
            // origin.
            .add_systems(
                PostUpdate,
                lay_track
                    .after(ChunkStreaming)
                    .before(TransformSystems::Propagate),
            );
    }
}

/// Lays the parts of the line inside each newly built chunk as its
/// children. Waits for the track kit before marking chunks done.
///
/// Should a chunk still vanish before these commands apply (another system
/// despawning it), nothing panics and nothing is left behind: the marker is
/// a `try_insert`, and the pieces are spawned as children only while the
/// chunk exists.
fn lay_track(
    mut commands: Commands,
    (rail, terrain, world): (Res<WorldRail>, Res<WorldTerrain>, Res<WorldChunks>),
    kit: Option<Res<TrackKit>>,
    chunks: Query<(Entity, &TerrainChunk), Without<TrackLaid>>,
) {
    let Some(kit) = kit else {
        return;
    };
    for (chunk, coord) in &chunks {
        let area = coord.0.bounds(world.0.chunk_size);
        let pieces: Vec<_> = rail
            .0
            .segments_in(area)
            .into_iter()
            .flat_map(|span| track_pieces(span, &terrain.0))
            .map(|(part, transform)| {
                (
                    LineTrack,
                    Mesh3d(kit.cube.clone()),
                    MeshMaterial3d(kit.material(part).clone()),
                    transform,
                )
            })
            .collect();
        let mut laid = commands.entity(chunk);
        laid.try_insert(TrackLaid);
        if pieces.is_empty() {
            continue;
        }
        // Silenced: a chunk that is gone by now simply gets no track.
        laid.queue_silenced(move |mut chunk: EntityWorldMut| {
            chunk.with_children(|track| {
                for piece in pieces {
                    track.spawn(piece);
                }
            });
        });
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{StreamFocus, TerrainPlugin, TownPlugin, WorldTowns};
    use pampa_core::{
        ChunkCoord, PlazaPlacement, Railway, RailwayConfig, Side, TownConfig, TownLayout,
        WorldConfig, place_towns,
    };

    /// Two railway towns 3 km apart on one east-west row, and their line.
    /// Two towns 3 km apart on one railway link.
    pub(crate) fn two_towns() -> (WorldConfig, Vec<TownLayout>, RailLine) {
        let world = WorldConfig::default();
        let town = |name: &str, x: f32| TownConfig {
            name: name.into(),
            origin: GroundVec2::new(x, 0.0),
            blocks: (4, 4),
            block_size: 40.0,
            street_width: 10.0,
            railway: Some(RailwayConfig {
                side: Side::North,
                offset: 24.0,
                through: false,
            }),
            plaza: PlazaPlacement::Auto,
            seed: 1,
            police_stations: None,
            density: None,
            townsfolk: None,
        };
        let (towns, errors) = place_towns(&world, &[town("Oeste", -1500.0), town("Este", 1500.0)]);
        assert!(errors.is_empty(), "{errors:?}");
        let (line, errors) = RailLine::build(None, &towns, world.bounds());
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(line.links.len(), 1);
        (world, towns, line)
    }

    fn app(focus: GroundVec2) -> App {
        let (world, towns, line) = two_towns();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .insert_resource(WorldChunks(world))
            .insert_resource(StreamFocus(focus))
            .insert_resource(WorldTowns(towns))
            .insert_resource(WorldRail(line))
            .add_plugins((
                TerrainPlugin {
                    radius: 1,
                    budget: 1,
                    resolution: 2,
                },
                TownPlugin,
                RailPlugin,
            ));
        app
    }

    /// Track pieces of the line under each chunk.
    fn track_by_chunk(app: &mut App) -> Vec<(ChunkCoord, usize)> {
        let mut chunks = app
            .world_mut()
            .query::<(&TerrainChunk, Option<&Children>)>();
        let mut pieces = app.world_mut().query_filtered::<(), With<LineTrack>>();
        let found: Vec<(ChunkCoord, Vec<Entity>)> = chunks
            .iter(app.world())
            .map(|(c, kids)| (c.0, kids.map(|k| k.to_vec()).unwrap_or_default()))
            .collect();
        found
            .into_iter()
            .map(|(coord, kids)| {
                let n = kids
                    .into_iter()
                    .filter(|&k| pieces.get(app.world(), k).is_ok())
                    .count();
                (coord, n)
            })
            .collect()
    }

    #[test]
    fn chunks_on_the_line_get_track_and_others_none() {
        // Midway between the towns, right on the link.
        let (world, _, line) = two_towns();
        let link = line.links[0].track;
        let middle = (link.start + link.end) * 0.5;
        let mut app = app(middle);
        app.update();
        app.update();
        let by_chunk = track_by_chunk(&mut app);
        assert!(!by_chunk.is_empty());
        let on_line = |c: ChunkCoord| !line.segments_in(c.bounds(world.chunk_size)).is_empty();
        for &(coord, pieces) in &by_chunk {
            if on_line(coord) {
                assert!(pieces > 100, "{coord:?}: {pieces} pieces");
            } else {
                assert_eq!(pieces, 0, "{coord:?}");
            }
        }
        assert!(by_chunk.iter().any(|&(c, _)| on_line(c)));
        assert!(by_chunk.iter().any(|&(c, _)| !on_line(c)));
        // No town is near: every piece belongs to the line.
        let all = app
            .world_mut()
            .query_filtered::<(), With<LineTrack>>()
            .iter(app.world())
            .count();
        assert_eq!(all, by_chunk.iter().map(|&(_, n)| n).sum::<usize>());
    }

    #[test]
    fn track_goes_with_its_chunk() {
        let (_, _, line) = two_towns();
        let link = line.links[0].track;
        let mut app = app((link.start + link.end) * 0.5);
        app.update();
        app.update();
        let count = |app: &mut App| {
            app.world_mut()
                .query_filtered::<(), With<LineTrack>>()
                .iter(app.world())
                .count()
        };
        assert!(count(&mut app) > 0);
        // Ride far south, off the line: the old chunks go, with their track.
        app.world_mut().resource_mut::<StreamFocus>().0 = GroundVec2::new(0.0, -3000.0);
        for _ in 0..12 {
            app.update();
        }
        assert_eq!(count(&mut app), 0);
    }

    /// Line track pieces that are not the child of a live terrain chunk.
    fn orphans(app: &mut App) -> usize {
        let parents: Vec<Option<Entity>> = app
            .world_mut()
            .query_filtered::<Option<&ChildOf>, With<LineTrack>>()
            .iter(app.world())
            .map(|c| c.map(ChildOf::parent))
            .collect();
        parents
            .into_iter()
            .filter(|parent| !parent.is_some_and(|p| app.world().get::<TerrainChunk>(p).is_some()))
            .count()
    }

    #[test]
    fn a_chunk_gets_its_track_the_same_frame_it_is_built() {
        let (world, _, line) = two_towns();
        let link = line.links[0].track;
        let middle = (link.start + link.end) * 0.5;
        let mut app = app(middle);
        app.update();
        let built = |app: &mut App| {
            app.world_mut()
                .query::<&TerrainChunk>()
                .iter(app.world())
                .count()
        };
        // Ride east along the link: one new chunk is built per frame, and
        // each update must show it with its track already laid.
        app.world_mut().resource_mut::<StreamFocus>().0 =
            middle + GroundVec2::new(world.chunk_size, 0.0);
        let mut new_on_line = 0;
        for frame in 0..3 {
            let before = built(&mut app);
            app.update();
            assert!(built(&mut app) > before, "frame {frame} builds a chunk");
            for (coord, pieces) in track_by_chunk(&mut app) {
                if !line.segments_in(coord.bounds(world.chunk_size)).is_empty() {
                    assert!(pieces > 0, "frame {frame}: {coord:?} has no track yet");
                    new_on_line += 1;
                }
            }
        }
        assert!(new_on_line > 0);
    }

    #[test]
    fn a_chunk_dropped_the_frame_after_it_is_built_leaves_no_track_behind() {
        let (_, _, line) = two_towns();
        let link = line.links[0].track;
        let mut app = app((link.start + link.end) * 0.5);
        app.update();
        assert_eq!(orphans(&mut app), 0);
        // Jump far off the line: the next frame drops every chunk just built.
        app.world_mut().resource_mut::<StreamFocus>().0 = GroundVec2::new(0.0, -3000.0);
        for _ in 0..3 {
            app.update();
            assert_eq!(orphans(&mut app), 0);
        }
    }

    #[test]
    fn a_chunk_despawned_before_its_track_lands_neither_panics_nor_orphans_pieces() {
        let (world, _, line) = two_towns();
        let link = line.links[0].track;
        let middle = (link.start + link.end) * 0.5;
        let mut app = app(middle);
        app.update();
        let count = |app: &mut App| {
            app.world_mut()
                .query_filtered::<(), With<LineTrack>>()
                .iter(app.world())
                .count()
        };
        let laid = count(&mut app);
        // A fresh chunk on the line, despawned after `lay_track` has queued
        // its track but before those commands are applied.
        let coord = ChunkCoord {
            x: (middle.x / world.chunk_size).floor() as i32,
            y: (middle.y / world.chunk_size).floor() as i32,
        };
        assert!(!line.segments_in(coord.bounds(world.chunk_size)).is_empty());
        let chunk = app
            .world_mut()
            .spawn((TerrainChunk(coord), Transform::default()))
            .id();
        let mut system = IntoSystem::into_system(lay_track);
        system.initialize(app.world_mut());
        system
            .run_without_applying_deferred((), app.world_mut())
            .expect("lay_track runs");
        assert!(app.world_mut().despawn(chunk));
        system.apply_deferred(app.world_mut());
        assert_eq!(count(&mut app), laid, "no pieces for the gone chunk");
        assert_eq!(orphans(&mut app), 0);
    }

    #[test]
    fn pieces_of_neighboring_stretches_add_up_to_the_whole_track() {
        let terrain = Terrain::default();
        let track = Railway {
            start: GroundVec2::new(-100.0, -40.0),
            end: GroundVec2::new(150.0, 60.0),
            gauge: 1.676,
        };
        let length = track.start.distance(track.end);
        let whole = track_pieces(RailSpan::whole(track), &terrain);
        let cut = 97.3;
        let part = |enter: f32, exit: f32| RailSpan {
            enter,
            exit,
            ..RailSpan::whole(track)
        };
        let mut halves = track_pieces(part(0.0, cut), &terrain);
        halves.extend(track_pieces(part(cut, length), &terrain));
        let sleepers =
            |v: &[(TrackPart, Transform)]| v.iter().filter(|p| p.0 == TrackPart::Sleeper).count();
        assert_eq!(sleepers(&whole), sleepers(&halves));
        assert_eq!(sleepers(&whole), (length / SLEEPER_SPACING) as usize);
        // The halves add one ballast piece (and its two rails) at the cut.
        assert_eq!(halves.len(), whole.len() + 3);
    }

    /// A 60 m track at `degrees` counter-clockwise from east on the ground.
    fn track_at(degrees: f32) -> Railway {
        let (sin, cos) = degrees.to_radians().sin_cos();
        let start = GroundVec2::new(20.0, -10.0);
        Railway {
            start,
            end: start + GroundVec2::new(cos, sin) * 60.0,
            gauge: 1.676,
        }
    }

    #[test]
    fn sleepers_cross_the_track_square_and_centered_between_the_rails() {
        let terrain = Terrain::default();
        for degrees in [0.0, 30.0, 45.0, 90.0, 135.0, 210.0, 300.0] {
            let track = track_at(degrees);
            let pieces = track_pieces(RailSpan::whole(track), &terrain);
            let dir = world_from_ground(track.end - track.start, 0.0).normalize();
            let rails: Vec<Transform> = pieces
                .iter()
                .filter(|p| p.0 == TrackPart::Rail)
                .map(|p| p.1)
                .collect();
            let sleepers = pieces.iter().filter(|p| p.0 == TrackPart::Sleeper);
            let mut seen = 0;
            for (_, sleeper) in sleepers {
                seen += 1;
                // The sleeper's long side is its local Z.
                let long = sleeper.rotation * Vec3::Z;
                assert!(
                    long.dot(dir).abs() < 1e-3,
                    "{degrees}°: sleeper {long:?} not square to track {dir:?}"
                );
                // The rails through this sleeper sit at equal distances on
                // either side of its center, along its long side.
                let c = sleeper.translation;
                let mut offsets: Vec<f32> = rails
                    .iter()
                    .filter(|r| {
                        let along = (c - r.translation).dot(dir).abs();
                        along <= r.scale.x / 2.0
                    })
                    .map(|r| {
                        // Distance from the rail's axis to the sleeper center,
                        // measured along the sleeper.
                        let to_rail = r.translation - c;
                        let flat = to_rail - dir * to_rail.dot(dir);
                        flat.dot(long)
                    })
                    .collect();
                offsets.sort_by(f32::total_cmp);
                assert_eq!(offsets.len(), 2, "{degrees}°: {offsets:?}");
                let half = track.gauge / 2.0;
                assert!((offsets[0] + half).abs() < 1e-2, "{degrees}°: {offsets:?}");
                assert!((offsets[1] - half).abs() < 1e-2, "{degrees}°: {offsets:?}");
                assert!(sleeper.scale.z > track.gauge, "sleeper spans both rails");
            }
            assert!(seen > 40, "{degrees}°: {seen} sleepers");
        }
    }

    #[test]
    fn sleepers_keep_one_spacing_around_a_curved_link_and_across_chunks() {
        // Two towns offset north-east, so the link curves twice.
        let world = WorldConfig::default();
        let town = |name: &str, x: f32, y: f32| TownConfig {
            name: name.into(),
            origin: GroundVec2::new(x, y),
            blocks: (4, 4),
            block_size: 40.0,
            street_width: 10.0,
            railway: Some(RailwayConfig {
                side: Side::North,
                offset: 24.0,
                through: false,
            }),
            plaza: PlazaPlacement::Auto,
            seed: 1,
            police_stations: None,
            density: None,
            townsfolk: None,
        };
        let (towns, errors) = place_towns(
            &world,
            &[town("Oeste", -1500.0, -800.0), town("Este", 1500.0, 800.0)],
        );
        assert!(errors.is_empty(), "{errors:?}");
        let (line, errors) = RailLine::build(None, &towns, world.bounds());
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(line.links[0].curves.len(), 2);
        let terrain = Terrain::default();
        // Lay the line chunk by chunk, as the terrain streams it in.
        let size = world.chunk_size;
        let link = line.links[0].track;
        let (lo, hi) = (
            GroundVec2::new(link.start.x.min(link.end.x), link.start.y.min(link.end.y)),
            GroundVec2::new(link.start.x.max(link.end.x), link.start.y.max(link.end.y)),
        );
        let cell = |v: f32| (v / size).floor() as i32;
        let mut sleepers: Vec<Vec3> = Vec::new();
        for y in cell(lo.y) - 2..=cell(hi.y) + 2 {
            for x in cell(lo.x) - 2..=cell(hi.x) + 2 {
                let area = ChunkCoord { x, y }.bounds(size);
                for span in line.segments_in(area) {
                    sleepers.extend(
                        track_pieces(span, &terrain)
                            .into_iter()
                            .filter(|p| p.0 == TrackPart::Sleeper)
                            .map(|p| p.1.translation),
                    );
                }
            }
        }
        let total = line.links[0].length();
        assert_eq!(sleepers.len(), (total / SLEEPER_SPACING) as usize);
        // Walk them in order along the link: each is one spacing from the
        // last (measured on the ground, the curve's chords barely shorten it).
        let flat = |v: Vec3| GroundVec2::new(v.x, -v.z);
        let mut left = sleepers.clone();
        let mut at = flat(
            left.remove(
                left.iter()
                    .enumerate()
                    .min_by(|a, b| {
                        let d = |v: &Vec3| flat(*v).distance(link.start);
                        d(a.1).total_cmp(&d(b.1))
                    })
                    .map(|(i, _)| i)
                    .expect("sleepers"),
            ),
        );
        assert!((at.distance(link.start) - SLEEPER_SPACING / 2.0).abs() < 1e-2);
        while !left.is_empty() {
            let (i, d) = left
                .iter()
                .enumerate()
                .map(|(i, v)| (i, flat(*v).distance(at)))
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .expect("sleepers");
            assert!((d - SLEEPER_SPACING).abs() < 1e-2, "gap of {d} m at {at:?}");
            at = flat(left.remove(i));
        }
    }
}
