//! Railway track: the railway line between towns, streamed with the
//! terrain chunks, and the pieces around it (buffer stops, bay platforms).
//! The ballast, sleepers and rails themselves are built along each track's
//! path by [`crate::railbed`].
//!
//! A town draws its own straight track under its [`crate::TownRoot`]. The
//! links of the [`RailLine`] between towns run for kilometers across open
//! pampa, so they are drawn chunk by chunk instead: when a terrain chunk is
//! built, the parts of the links inside it are laid as its children, and
//! they go when the chunk goes. Chunks are built a few per frame, so the
//! track keeps the terrain's per-frame budget.

use bevy::prelude::*;
use bevy::transform::TransformSystems;
use pampa_core::{Aabb, Floor, RailBay, RailLine, Terrain, Vec2 as GroundVec2};

use crate::coords::world_from_ground;
use crate::facade::{FacadeMaterials, Surface};
use crate::railbed::TrackNet;
use crate::terrain::{ChunkStreaming, TerrainChunk, WorldChunks, WorldTerrain};
use crate::town::WorldTowns;

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

/// The beaten-earth walks from a bay's platform to the road, child of the
/// terrain chunk their middles lie in: one decal mesh per chunk.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct BayWalk;

/// Mesh and materials shared by the track's timber and concrete pieces, in
/// town and between towns (the ballast, sleepers and rails are meshes of
/// their own: see [`crate::railbed`]). The town kit inserts it (see
/// `crate::town`).
#[derive(Resource, Debug, Clone)]
pub(crate) struct TrackKit {
    pub cube: Handle<Mesh>,
    pub timber: Handle<StandardMaterial>,
    pub concrete: Handle<StandardMaterial>,
}

impl TrackKit {
    pub(crate) fn material(&self, part: TrackPart) -> &Handle<StandardMaterial> {
        match part {
            TrackPart::Timber => &self.timber,
            TrackPart::Concrete => &self.concrete,
        }
    }
}

/// What a piece of the track kit's cube is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrackPart {
    /// The buffer stop's beams.
    Timber,
    /// A bay platform, as the stations' (see `crate::town`).
    Concrete,
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

/// The timber of the buffer stop at the dead end of `bay`: a beam across
/// the rails, just above their tops, on two struts raked back along the
/// track. Each is a unit cube's transform, for the track kit's timber.
pub(crate) fn buffer_stop(bay: &RailBay, terrain: &Terrain) -> Vec<Transform> {
    let (end, back) = (bay.buffer(), bay.heading());
    let across = GroundVec2::new(-back.y, back.x);
    let half = bay.link.track.gauge / 2.0 + 0.5;
    let beam = end + back * 0.15;
    let beam_ends = (beam - across * half, beam + across * half);
    let strut = |side: f32| {
        let at = across * side;
        (end - back * 0.9 + at, beam + at)
    };
    let (left, right) = (strut(-half + 0.15), strut(half - 0.15));
    [
        stretched(terrain, beam_ends.0, beam_ends.1, 0.4, Vec2::new(0.35, 0.6)),
        stretched(terrain, left.0, left.1, 0.0, Vec2::new(0.3, 0.7)),
        stretched(terrain, right.0, right.1, 0.0, Vec2::new(0.3, 0.7)),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Thickness of a platform's ramp slab, in meters.
const RAMP_THICKNESS: f32 = 0.2;
/// A slab sloping from the edge of `floor`'s top down to the ground at
/// each end, along the ramps people are walked over ([`Floor::rise_at`]),
/// each a unit cube's transform.
fn floor_ramps(floor: &Floor, terrain: &Terrain) -> Vec<Transform> {
    let f = floor.footprint;
    let mid = floor.across() * f.center().dot(floor.across());
    let width = f.size().dot(floor.across());
    let ground = |p: GroundVec2| terrain.height(p.x, p.y);
    [
        (f.min.dot(floor.along), -1.0),
        (f.max.dot(floor.along), 1.0),
    ]
    .into_iter()
    .map(|(end, out)| {
        let top = mid + floor.along * end;
        let foot = top + floor.along * (out * Floor::RAMP_LENGTH);
        let (top3, foot3) = (
            world_from_ground(top, ground(top) + floor.height),
            world_from_ground(foot, ground(foot)),
        );
        let slope = (foot3 - top3).normalize();
        let side = slope.cross(Vec3::Y).normalize();
        let up = side.cross(slope);
        let rotation = Quat::from_mat3(&Mat3::from_cols(slope, up, side));
        Transform::from_translation((top3 + foot3) * 0.5 - up * (RAMP_THICKNESS / 2.0))
            .with_rotation(rotation)
            .with_scale(Vec3::new(top3.distance(foot3), RAMP_THICKNESS, width))
    })
    .collect()
}

/// The bay platform of `bay`: a concrete slab with its two ramps, drawn
/// as the stations' platforms are (`crate::town`).
pub(crate) fn bay_platform(bay: &RailBay, terrain: &Terrain) -> Vec<(TrackPart, Transform)> {
    let floor = &bay.platform;
    let (c, size, h) = (
        floor.footprint.center(),
        floor.footprint.size(),
        floor.height,
    );
    let y = terrain.height(c.x, c.y);
    let top = Transform::from_translation(world_from_ground(c, y + (h - 0.2) / 2.0))
        .with_scale(Vec3::new(size.x, h + 0.2, size.y));
    std::iter::once(top)
        .chain(floor_ramps(floor, terrain))
        .map(|t| (TrackPart::Concrete, t))
        .collect()
}

/// The platforms of `line`'s bays whose middle stands inside `area`.
fn bay_works_in(line: &RailLine, area: Aabb, terrain: &Terrain) -> Vec<(TrackPart, Transform)> {
    line.bays
        .iter()
        .filter(|bay| area.contains_point(bay.platform.footprint.center()))
        .flat_map(|bay| bay_platform(bay, terrain))
        .collect()
}

/// The walks of `line`'s bays whose middle stands inside `area`.
fn bay_walks_in(line: &RailLine, area: Aabb) -> Vec<Aabb> {
    line.bays
        .iter()
        .flat_map(|bay| &bay.walks)
        .filter(|walk| area.contains_point(walk.center()))
        .copied()
        .collect()
}

/// The buffer stops of `line`'s bays that stand inside `area`.
fn buffer_stops_in(line: &RailLine, area: Aabb, terrain: &Terrain) -> Vec<Transform> {
    line.bays
        .iter()
        .filter(|bay| area.contains_point(bay.buffer()))
        .flat_map(|bay| buffer_stop(bay, terrain))
        .collect()
}

/// Lays the railway line's track on terrain chunks as they are built.
/// Needs the [`crate::TownPlugin`] for the shared track kit.
pub struct RailPlugin;

impl Plugin for RailPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldRail>()
            .init_resource::<WorldTowns>()
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
    (rail, terrain, world, towns): (
        Res<WorldRail>,
        Res<WorldTerrain>,
        Res<WorldChunks>,
        Res<WorldTowns>,
    ),
    (kit, surfaces): (Option<Res<TrackKit>>, Option<Res<FacadeMaterials>>),
    mut meshes: ResMut<Assets<Mesh>>,
    chunks: Query<(Entity, &TerrainChunk), Without<TrackLaid>>,
) {
    let (Some(kit), Some(surfaces)) = (kit, surfaces) else {
        return;
    };
    if chunks.is_empty() {
        return;
    }
    // Every track that may lie near another, in the order they take
    // precedence: the beds are cut from it chunk by chunk.
    let net = TrackNet::new(&towns.0, &rail.0);
    for (chunk, coord) in &chunks {
        let area = coord.0.bounds(world.0.chunk_size);
        let timbers: Vec<_> = buffer_stops_in(&rail.0, area, &terrain.0)
            .into_iter()
            .map(|transform| (TrackPart::Timber, transform))
            .chain(bay_works_in(&rail.0, area, &terrain.0))
            .map(|(part, transform)| {
                (
                    LineTrack,
                    Mesh3d(kit.cube.clone()),
                    MeshMaterial3d(kit.material(part).clone()),
                    transform,
                )
            })
            .collect();
        // The bed, rails and sleepers of the track inside the chunk: one
        // mesh each.
        let track: Vec<_> = net
            .lay_in(area, &terrain.0)
            .into_meshes()
            .into_iter()
            .map(|(surface, mesh)| {
                (
                    LineTrack,
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(surfaces.get(surface)),
                    Transform::default(),
                )
            })
            .collect();
        // The earth the platforms' walks are trodden in.
        let ground = |p: GroundVec2| terrain.0.height(p.x, p.y);
        let walks: Vec<Vec<Aabb>> = bay_walks_in(&rail.0, area)
            .into_iter()
            .map(|walk| vec![walk])
            .collect();
        let walk = crate::settlement::track::walks_mesh(&walks, &ground).map(|mesh| {
            (
                BayWalk,
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(surfaces.get(Surface::Yard)),
                Transform::default(),
            )
        });
        let mut laid = commands.entity(chunk);
        laid.try_insert(TrackLaid);
        if timbers.is_empty() && track.is_empty() && walk.is_none() {
            continue;
        }
        // Silenced: a chunk that is gone by now simply gets no track.
        laid.queue_silenced(move |mut chunk: EntityWorldMut| {
            chunk.with_children(|children| {
                for piece in timbers {
                    children.spawn(piece);
                }
                for piece in track {
                    children.spawn(piece);
                }
                if let Some(walk) = walk {
                    children.spawn(walk);
                }
            });
        });
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{StreamFocus, TerrainPlugin, TownPlugin};
    use pampa_core::{
        ChunkCoord, PlazaPlacement, RailwayConfig, Side, TownConfig, TownLayout, WorldConfig,
        place_towns,
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
                // A mesh each for the bed, the sleepers, the rails and
                // their heads.
                assert_eq!(pieces, 4, "{coord:?}: {pieces} pieces");
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

    /// Three towns on one row with a branch off the middle one, and the
    /// line with its bay track at the junction.
    fn bayed_line() -> (WorldConfig, Vec<TownLayout>, RailLine) {
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
        let configs = [
            town("Oeste", -2000.0, 0.0),
            town("Medio", 0.0, 0.0),
            town("Este", 2000.0, 0.0),
            town("Norte", -2200.0, 1800.0),
        ];
        let (towns, errors) = place_towns(&world, &configs);
        assert!(errors.is_empty(), "{errors:?}");
        let list = ["Oeste", "Medio", "Este"].map(String::from);
        let branches = [["Medio", "Norte"].map(String::from).to_vec()];
        let (line, errors) =
            RailLine::build_with_branches(Some(&list), &branches, &towns, world.bounds());
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(line.bays.len(), 1);
        (world, towns, line)
    }

    #[test]
    fn a_buffer_stop_closes_the_dead_end_of_the_bay_across_both_rails() {
        let (_, _, line) = bayed_line();
        let bay = &line.bays[0];
        let terrain = Terrain::default();
        let pieces = buffer_stop(bay, &terrain);
        // A beam and two struts, all at the buffer.
        assert_eq!(pieces.len(), 3);
        let flat = |t: &Transform| GroundVec2::new(t.translation.x, -t.translation.z);
        for piece in &pieces {
            assert!(flat(piece).distance(bay.buffer()) < 1.5, "{piece:?}");
        }
        // The beam reaches past both rails and stands above their tops.
        let beam = &pieces[0];
        assert!(
            beam.scale.x > bay.link.track.gauge + 0.9,
            "{:?}",
            beam.scale
        );
        let ground = terrain.height(bay.buffer().x, bay.buffer().y);
        assert!(beam.translation.y > ground + 0.5, "{beam:?}");
        // It is closed at the end only: nothing stands further into the bay.
        let inside = bay.buffer() + bay.heading() * 2.0;
        assert!(pieces.iter().all(|p| flat(p).distance(inside) > 0.9));
        // Only the chunk holding the dead end gets it.
        let area = |at: GroundVec2| Aabb::from_center_size(at, GroundVec2::new(200.0, 200.0));
        assert_eq!(
            buffer_stops_in(&line, area(bay.buffer()), &terrain).len(),
            3
        );
        let away = bay.buffer() + GroundVec2::new(0.0, 1000.0);
        assert!(buffer_stops_in(&line, area(away), &terrain).is_empty());
    }

    #[test]
    fn the_chunk_with_the_bay_gets_its_track_and_buffer_stop() {
        let (world, towns, line) = bayed_line();
        let bay = line.bays[0].clone();
        let coord = ChunkCoord {
            x: (bay.stop.x / world.chunk_size).floor() as i32,
            y: (bay.stop.y / world.chunk_size).floor() as i32,
        };
        let laid = |line: &RailLine| {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .init_resource::<Assets<Mesh>>()
                .init_resource::<Assets<StandardMaterial>>()
                .insert_resource(WorldChunks(world))
                .insert_resource(StreamFocus(bay.stop))
                .insert_resource(WorldTowns(towns.clone()))
                .insert_resource(WorldRail(line.clone()))
                .add_plugins((
                    TerrainPlugin {
                        radius: 1,
                        budget: 1,
                        resolution: 2,
                    },
                    TownPlugin,
                    RailPlugin,
                ));
            app.update();
            app.update();
            track_by_chunk(&mut app)
                .into_iter()
                .find(|&(c, _)| c == coord)
                .map(|(_, n)| n)
                .expect("the chunk is built")
        };
        let mut bare = line.clone();
        bare.bays.clear();
        // The bay adds its bed, sleepers and rails to the chunk (a mesh
        // each, where the chunk had them already: none then), and the
        // platform's three timbers where its middle stands.
        let terrain = Terrain::default();
        let area = coord.bounds(world.chunk_size);
        let meshes = |line: &RailLine| {
            TrackNet::new(&towns, line)
                .lay_in(area, &terrain)
                .into_meshes()
                .len()
        };
        let bay_meshes = meshes(&line) - meshes(&bare);
        assert_eq!(meshes(&line), 4);
        // The shuttle stands in this chunk; the buffer stop is in the next.
        assert!(area.contains_point(bay.stop));
        assert!(!area.contains_point(bay.buffer()));
        // And the platform, whose middle stands in it.
        let works = bay_works_in(&line, area, &terrain).len();
        assert_eq!(works, 3);
        assert_eq!(laid(&line) - laid(&bare), bay_meshes + works);
    }

    /// Terrain leveled around `bay`, as the town's ground is.
    fn level_at(bay: &RailBay) -> Terrain {
        Terrain::default().with_flat_area(pampa_core::FlatArea {
            center: bay.stop,
            half_extents: GroundVec2::new(100.0, 60.0),
            falloff: 10.0,
            height: 3.0,
        })
    }

    #[test]
    fn the_bay_platform_is_a_concrete_slab_at_station_height_with_a_ramp_at_each_end() {
        let (_, _, line) = bayed_line();
        let bay = &line.bays[0];
        let terrain = level_at(bay);
        let pieces = bay_platform(bay, &terrain);
        assert_eq!(pieces.len(), 3);
        assert!(pieces.iter().all(|(part, _)| *part == TrackPart::Concrete));
        let f = bay.platform.footprint;
        // The slab: the platform's footprint, its top at the floor height
        // core walks people on.
        let slab = &pieces[0].1;
        let (size, c) = (f.size(), f.center());
        assert!((slab.scale.x - size.x).abs() < 1e-3 && (slab.scale.z - size.y).abs() < 1e-3);
        let ground = terrain.height(c.x, c.y);
        let top = slab.translation.y + slab.scale.y / 2.0 - ground;
        assert!((top - bay.platform.height).abs() < 1e-3, "{top}");
        assert!((slab.translation.x - c.x).abs() < 1e-3 && (slab.translation.z + c.y).abs() < 1e-3);
        // Each ramp starts at the platform's edge at its top and ends on
        // the ground a ramp's length out (a slab's middle is halfway).
        let along = bay.platform.along;
        for (ramp, out) in pieces[1..].iter().map(|p| &p.1).zip([-1.0_f32, 1.0]) {
            let mid = GroundVec2::new(ramp.translation.x, -ramp.translation.z);
            let end = if out < 0.0 { f.min } else { f.max }.dot(along);
            let reach = (mid.dot(along) - end) * out;
            assert!((reach - Floor::RAMP_LENGTH / 2.0).abs() < 0.2, "{reach}");
            // Halfway down: below the top, above the ground.
            let halfway = ramp.translation.y - ground;
            assert!(halfway > 0.0 && halfway < bay.platform.height, "{halfway}");
        }
    }

    #[test]
    fn a_chunk_gets_the_bay_platform_only_where_its_middle_stands() {
        let (_, _, line) = bayed_line();
        let bay = &line.bays[0];
        let terrain = Terrain::default();
        let at = |p: GroundVec2| Aabb::from_center_size(p, GroundVec2::new(10.0, 10.0));
        let all = bay_platform(bay, &terrain).len();
        assert_eq!(all, 3);
        assert_eq!(
            bay_works_in(&line, at(bay.platform.footprint.center()), &terrain).len(),
            all
        );
        let away = bay.platform.footprint.center() + GroundVec2::new(0.0, 2000.0);
        assert!(bay_works_in(&line, at(away), &terrain).is_empty());
    }

    #[test]
    fn a_chunk_gets_a_bay_walk_where_its_middle_stands_and_none_elsewhere() {
        let (_, _, mut line) = bayed_line();
        let strip = |x: f32, y: f32| {
            Aabb::new(
                GroundVec2::new(x, y - 1.0),
                GroundVec2::new(x + 40.0, y + 1.0),
            )
        };
        line.bays[0].walks = vec![strip(100.0, 0.0), strip(100.0, 500.0)];
        let around = |p: GroundVec2| Aabb::from_center_size(p, GroundVec2::new(30.0, 30.0));
        // Each strip is in the area its middle stands in, the other not.
        assert_eq!(
            bay_walks_in(&line, around(GroundVec2::new(120.0, 0.0))),
            [strip(100.0, 0.0)]
        );
        assert_eq!(
            bay_walks_in(&line, around(GroundVec2::new(120.0, 500.0))),
            [strip(100.0, 500.0)]
        );
        assert!(bay_walks_in(&line, around(GroundVec2::new(120.0, 250.0))).is_empty());
    }

    #[test]
    fn the_chunk_with_the_bay_walks_gets_them_as_earth_the_same_frame() {
        let (world, towns, mut line) = bayed_line();
        let bay = line.bays[0].clone();
        let at = bay.platform.footprint.center() - GroundVec2::new(70.0, 0.0);
        line.bays[0].walks = vec![Aabb::from_center_size(at, GroundVec2::new(20.0, 2.0))];
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .insert_resource(WorldChunks(world))
            .insert_resource(StreamFocus(at))
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
        for _ in 0..3 {
            app.update();
        }
        let walks = app
            .world_mut()
            .query_filtered::<&Mesh3d, With<BayWalk>>()
            .iter(app.world())
            .count();
        assert_eq!(walks, 1);
        // It hangs off a terrain chunk, and goes with it.
        let kids = app
            .world_mut()
            .query_filtered::<&ChildOf, With<BayWalk>>()
            .iter(app.world())
            .map(|c| c.parent())
            .collect::<Vec<_>>();
        assert!(
            kids.iter()
                .all(|&k| app.world().get::<TerrainChunk>(k).is_some())
        );
    }
}
