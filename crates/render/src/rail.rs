//! Vía férrea: la línea ferroviaria entre pueblos, cargada por streaming junto
//! con los chunks de terreno, y las piezas que la rodean (topes de fin de vía,
//! andenes de la vía muerta). El balasto, los durmientes y los rieles
//! propiamente dichos se construyen a lo largo del recorrido de cada vía en
//! [`crate::railbed`].
//!
//! Cada pueblo dibuja su propia vía recta bajo su [`crate::TownRoot`]. Los
//! enlaces de la [`RailLine`] entre pueblos recorren kilómetros de pampa
//! abierta, así que en cambio se dibujan chunk por chunk: cuando se construye
//! un chunk de terreno, las partes de los enlaces que caen en él se tienden
//! como hijas suyas y se van cuando se va el chunk. Los chunks se construyen de
//! a unos pocos por frame, así que la vía respeta el presupuesto por frame del
//! terreno.

use bevy::prelude::*;
use bevy::transform::TransformSystems;
use pampa_core::{Aabb, Floor, RailBay, RailLine, Terrain, Vec2 as GroundVec2};

use crate::coords::world_from_ground;
use crate::facade::{FacadeMaterials, Surface};
use crate::railbed::TrackNet;
use crate::terrain::{ChunkStreaming, TerrainChunk, WorldChunks, WorldTerrain};
use crate::town::WorldTowns;

/// La línea ferroviaria entre los pueblos. Se inserta antes de que corra el
/// plugin; sin ella no se tiende vía entre pueblos.
#[derive(Resource, Debug, Clone, Default)]
pub struct WorldRail(pub RailLine);

/// Una pieza de la línea ferroviaria entre pueblos, hija del chunk de terreno
/// en el que se encuentra.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineTrack;

/// Marca un chunk de terreno cuya parte de la línea ferroviaria ya se tendió
/// (aunque la línea no pase por él), para que se revise una sola vez.
#[derive(Component, Debug, Clone, Copy)]
struct TrackLaid;

/// Los senderos de tierra apisonada desde el andén de una vía muerta hasta el
/// camino, hijos del chunk de terreno en el que cae su punto medio: una malla
/// de decal por chunk.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct BayWalk;

/// Malla y materiales compartidos por las piezas de madera y de hormigón de la
/// vía, dentro y entre los pueblos (el balasto, los durmientes y los rieles son
/// mallas propias: ver [`crate::railbed`]). Lo inserta el kit del pueblo (ver
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

/// De qué está hecha una pieza del cubo del kit de vía.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrackPart {
    /// Las vigas del tope de fin de vía.
    Timber,
    /// Un andén de la vía muerta, como el de las estaciones (ver
    /// `crate::town`).
    Concrete,
}

/// Una viga desde el punto del suelo `from` hasta `to`, elevada `lift` sobre el
/// suelo, con `section` (ancho, alto) como sección transversal; `None` cuando
/// no tiene largo.
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

/// La madera del tope de fin de vía en el extremo cerrado de `bay`: una viga
/// atravesada sobre los rieles, apenas por encima de su cara superior, sobre
/// dos puntales inclinados hacia atrás a lo largo de la vía. Cada uno es la
/// transformación de un cubo unitario, para la madera del kit de vía.
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

/// Espesor de la losa de la rampa de un andén, en metros.
const RAMP_THICKNESS: f32 = 0.2;
/// Una losa que baja desde el borde de la parte superior de `floor` hasta el
/// suelo en cada extremo, a lo largo de las rampas por las que se hace caminar
/// a la gente ([`Floor::rise_at`]), cada una como la transformación de un cubo
/// unitario.
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

/// El andén de la vía muerta `bay`: una losa de hormigón con sus dos rampas,
/// dibujado como los andenes de las estaciones (`crate::town`).
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

/// Los andenes de las vías muertas de `line` cuyo punto medio está dentro de
/// `area`.
fn bay_works_in(line: &RailLine, area: Aabb, terrain: &Terrain) -> Vec<(TrackPart, Transform)> {
    line.bays
        .iter()
        .filter(|bay| area.contains_point(bay.platform.footprint.center()))
        .flat_map(|bay| bay_platform(bay, terrain))
        .collect()
}

/// Los senderos de las vías muertas de `line` cuyo punto medio está dentro de
/// `area`.
fn bay_walks_in(line: &RailLine, area: Aabb) -> Vec<Aabb> {
    line.bays
        .iter()
        .flat_map(|bay| &bay.walks)
        .filter(|walk| area.contains_point(walk.center()))
        .copied()
        .collect()
}

/// Los topes de fin de vía de las vías muertas de `line` que están dentro de
/// `area`.
fn buffer_stops_in(line: &RailLine, area: Aabb, terrain: &Terrain) -> Vec<Transform> {
    line.bays
        .iter()
        .filter(|bay| area.contains_point(bay.buffer()))
        .flat_map(|bay| buffer_stop(bay, terrain))
        .collect()
}

/// Tiende la vía de la línea ferroviaria sobre los chunks de terreno a medida
/// que se construyen. Necesita el [`crate::TownPlugin`] para el kit de vía
/// compartido.
pub struct RailPlugin;

impl Plugin for RailPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldRail>()
            .init_resource::<WorldTowns>()
            .init_resource::<WorldTerrain>()
            .init_resource::<WorldChunks>()
            // Justo después del streaming de chunks, en el mismo schedule: los
            // chunks que construyó en este frame ya están creados (Bevy aplica
            // sus comandos antes de que corra un sucesor ordenado), así que
            // reciben su vía en el mismo frame, y los chunks que descartó ya no
            // están, así que ninguno se decora después de eliminarse. Antes de
            // la propagación de transformaciones, para que las piezas nuevas
            // nunca se dibujen un frame en el origen.
            .add_systems(
                PostUpdate,
                lay_track
                    .after(ChunkStreaming)
                    .before(TransformSystems::Propagate),
            );
    }
}

/// Tiende como hijas de cada chunk recién construido las partes de la línea que
/// caen en él. Espera el kit de vía antes de marcar los chunks como hechos.
///
/// Si un chunk llegara a desaparecer antes de que se apliquen estos comandos
/// (porque otro sistema lo elimina), nada entra en pánico y no queda nada
/// suelto: el marcador es un `try_insert`, y las piezas se crean como hijas
/// solo mientras el chunk existe.
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
    // Cada vía que puede quedar cerca de otra, en orden de precedencia: los
    // lechos se recortan a partir de ella chunk por chunk.
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
        // El lecho, los rieles y los durmientes de la vía dentro del chunk: una
        // malla cada uno.
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
        // La tierra en la que se pisan los senderos de los andenes.
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
        // Silenciado: un chunk que ya no está simplemente no recibe vía.
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

    /// Dos pueblos ferroviarios a 3 km de distancia en una misma fila de este a
    /// oeste, y su línea. Dos pueblos a 3 km sobre un mismo enlace ferroviario.
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

    /// Piezas de vía de la línea bajo cada chunk.
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
        // A mitad de camino entre los pueblos, justo sobre el enlace.
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
                // Una malla para el lecho, otra para los durmientes, otra para
                // los rieles y otra para sus cabezas.
                assert_eq!(pieces, 4, "{coord:?}: {pieces} pieces");
            } else {
                assert_eq!(pieces, 0, "{coord:?}");
            }
        }
        assert!(by_chunk.iter().any(|&(c, _)| on_line(c)));
        assert!(by_chunk.iter().any(|&(c, _)| !on_line(c)));
        // No hay ningún pueblo cerca: cada pieza pertenece a la línea.
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
        // Se cabalga lejos hacia el sur, fuera de la línea: los chunks viejos
        // se van, con su vía.
        app.world_mut().resource_mut::<StreamFocus>().0 = GroundVec2::new(0.0, -3000.0);
        for _ in 0..12 {
            app.update();
        }
        assert_eq!(count(&mut app), 0);
    }

    /// Piezas de vía de la línea que no son hijas de un chunk de terreno vivo.
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
        // Se cabalga hacia el este a lo largo del enlace: se construye un chunk
        // nuevo por frame, y cada actualización debe mostrarlo con su vía ya
        // tendida.
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
        // Salto lejos de la línea: el frame siguiente descarta cada chunk
        // recién construido.
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
        // Un chunk nuevo sobre la línea, eliminado después de que `lay_track`
        // encoló su vía pero antes de que se apliquen esos comandos.
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

    /// Tres pueblos en una fila con un ramal que sale del pueblo del medio, y
    /// la línea con su vía muerta en el empalme.
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
        // Una viga y dos puntales, todos en el tope.
        assert_eq!(pieces.len(), 3);
        let flat = |t: &Transform| GroundVec2::new(t.translation.x, -t.translation.z);
        for piece in &pieces {
            assert!(flat(piece).distance(bay.buffer()) < 1.5, "{piece:?}");
        }
        // La viga sobrepasa ambos rieles y queda por encima de su cara
        // superior.
        let beam = &pieces[0];
        assert!(
            beam.scale.x > bay.link.track.gauge + 0.9,
            "{:?}",
            beam.scale
        );
        let ground = terrain.height(bay.buffer().x, bay.buffer().y);
        assert!(beam.translation.y > ground + 0.5, "{beam:?}");
        // Está cerrada solo en el extremo: nada se levanta más adentro de la
        // vía muerta.
        let inside = bay.buffer() + bay.heading() * 2.0;
        assert!(pieces.iter().all(|p| flat(p).distance(inside) > 0.9));
        // Solo lo recibe el chunk que contiene el extremo cerrado.
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
        // La vía muerta agrega su lecho, sus durmientes y sus rieles al chunk
        // (una malla cada uno; donde el chunk ya los tenía, ninguna), y las
        // tres maderas del andén donde cae su punto medio.
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
        // La lanzadera está en este chunk; el tope de fin de vía, en el
        // siguiente.
        assert!(area.contains_point(bay.stop));
        assert!(!area.contains_point(bay.buffer()));
        // Y el andén, cuyo punto medio cae en él.
        let works = bay_works_in(&line, area, &terrain).len();
        assert_eq!(works, 3);
        assert_eq!(laid(&line) - laid(&bare), bay_meshes + works);
    }

    /// Terreno nivelado alrededor de `bay`, como el suelo del pueblo.
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
        // La losa: la huella del andén, con su parte superior a la altura del
        // piso sobre el que core hace caminar a la gente.
        let slab = &pieces[0].1;
        let (size, c) = (f.size(), f.center());
        assert!((slab.scale.x - size.x).abs() < 1e-3 && (slab.scale.z - size.y).abs() < 1e-3);
        let ground = terrain.height(c.x, c.y);
        let top = slab.translation.y + slab.scale.y / 2.0 - ground;
        assert!((top - bay.platform.height).abs() < 1e-3, "{top}");
        assert!((slab.translation.x - c.x).abs() < 1e-3 && (slab.translation.z + c.y).abs() < 1e-3);
        // Cada rampa empieza en el borde del andén, arriba, y termina en el
        // suelo a un largo de rampa de distancia (el punto medio de una losa
        // está a mitad de camino).
        let along = bay.platform.along;
        for (ramp, out) in pieces[1..].iter().map(|p| &p.1).zip([-1.0_f32, 1.0]) {
            let mid = GroundVec2::new(ramp.translation.x, -ramp.translation.z);
            let end = if out < 0.0 { f.min } else { f.max }.dot(along);
            let reach = (mid.dot(along) - end) * out;
            assert!((reach - Floor::RAMP_LENGTH / 2.0).abs() < 0.2, "{reach}");
            // A mitad de la bajada: por debajo de la parte superior, por encima
            // del suelo.
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
        // Cada franja está en el área en la que cae su punto medio, y la otra
        // no.
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
        // Cuelga de un chunk de terreno y se va con él.
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
