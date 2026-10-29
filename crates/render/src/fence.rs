//! Alambrados y sus tranqueras abiertas, cargados por streaming junto con los
//! chunks de terreno.
//!
//! `pampa-core` traza los alambrados ([`FenceLayout`]). Cuando se construye un
//! chunk de terreno, los tramos de alambrado cuyo punto medio cae en él se
//! dibujan como hijos suyos: piezas de unos pocos tramos cada una, cada pieza
//! con una malla de postes de madera y otra de alambre galvanizado, para que
//! una pieza pueda desvanecerse por sí sola cuando se interpone entre la cámara
//! y el jinete ([`crate::flora::ThinOccluder`]). Los caminos corren entre dos
//! alambrados que siguen sus curvas. Una tranquera (del camino a un campo, o
//! entre potreros) recibe dos postes robustos y una o dos hojas de madera
//! abiertas hacia atrás a lo largo del alambrado; un tramo roto recibe un poste
//! inclinado, uno caído y un alambre combado.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use pampa_core::{FenceLayout, Gate, Opening, OpeningKind, Terrain, Vec2 as GroundVec2};

use crate::coords::world_from_ground;
use crate::flora::ThinOccluder;
use crate::terrain::{ChunkStreaming, TerrainChunk, WorldChunks, WorldTerrain};

/// Los alambrados del mundo. Se insertan antes de que corra el plugin; sin
/// ellos no se dibuja ningún alambrado.
#[derive(Resource, Debug, Clone, Default)]
pub struct WorldFences(pub FenceLayout);

/// Una pieza dibujada de alambrado (postes o alambre), hija de su chunk.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FencePiece;

/// Un poste u hoja de una tranquera, o una pieza de un tramo roto, hijo de su
/// chunk.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GatePart;

#[derive(Component)]
struct FencesLaid;

#[derive(Resource)]
struct FenceKit {
    timber: Handle<StandardMaterial>,
    wire: Handle<StandardMaterial>,
    cube: Handle<Mesh>,
}

/// Dibuja los [`WorldFences`] chunk por chunk. Necesita el
/// [`crate::TerrainPlugin`].
pub struct FencePlugin;

impl Plugin for FencePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldFences>()
            .init_resource::<WorldTerrain>()
            .init_resource::<WorldChunks>()
            .add_systems(Startup, setup_kit)
            .add_systems(
                PostUpdate,
                lay_fences
                    .after(ChunkStreaming)
                    .before(TransformSystems::Propagate),
            );
    }
}

fn setup_kit(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(FenceKit {
        timber: materials.add(StandardMaterial {
            base_color: Color::srgb(0.42, 0.34, 0.25),
            perceptual_roughness: 0.95,
            ..default()
        }),
        wire: materials.add(StandardMaterial {
            base_color: Color::srgb(0.62, 0.63, 0.62),
            metallic: 0.8,
            perceptual_roughness: 0.45,
            ..default()
        }),
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
    });
}

/// Tramos por pieza dibujada: lo bastante cortos para que solo se desvanezca el
/// trecho que está delante de la cámara.
pub const SPANS_PER_PIECE: usize = 5;
/// Alturas de los alambres sobre el suelo, en metros.
const WIRES: [f32; FenceLayout::WIRES] = [0.3, 0.52, 0.74, 0.96, 1.18];
/// Grosor de un alambre tal como se dibuja, en metros (un poco más que los 4 mm
/// reales, para que no desaparezca a la distancia).
const WIRE: f32 = 0.014;
/// Lado de un poste, en metros.
const POST: f32 = 0.12;

type FenceBundle = (
    Mesh3d,
    MeshMaterial3d<StandardMaterial>,
    Transform,
    ThinOccluder,
    bool,
);

fn lay_fences(
    mut commands: Commands,
    (fences, terrain, world): (Res<WorldFences>, Res<WorldTerrain>, Res<WorldChunks>),
    kit: Option<Res<FenceKit>>,
    chunks: Query<(Entity, &TerrainChunk), Without<FencesLaid>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Some(kit) = kit else {
        return;
    };
    for (chunk, coord) in &chunks {
        let area = coord.0.bounds(world.0.chunk_size);
        let mut parts: Vec<FenceBundle> = Vec::new();
        for run in fences.0.runs_in(area) {
            let spans: Vec<(GroundVec2, GroundVec2)> = run.spans().collect();
            for piece in spans.chunks(SPANS_PER_PIECE) {
                let (posts, wires) = piece_meshes(piece, &terrain.0);
                let fade = piece_occluder(piece, &terrain.0);
                parts.push((
                    Mesh3d(meshes.add(posts)),
                    MeshMaterial3d(kit.timber.clone()),
                    Transform::default(),
                    fade,
                    false,
                ));
                parts.push((
                    Mesh3d(meshes.add(wires)),
                    MeshMaterial3d(kit.wire.clone()),
                    Transform::default(),
                    fade,
                    false,
                ));
            }
        }
        for gate in fences.0.gates.iter().filter(|g| {
            let p = g.at;
            p.x >= area.min.x && p.x < area.max.x && p.y >= area.min.y && p.y < area.max.y
        }) {
            for transform in gate_pieces(gate, &terrain.0) {
                let ground = GroundVec2::new(transform.translation.x, -transform.translation.z);
                let fade = ThinOccluder::post(ground, 0.6, &terrain.0, 1.6);
                parts.push((
                    Mesh3d(kit.cube.clone()),
                    MeshMaterial3d(kit.timber.clone()),
                    transform,
                    fade,
                    true,
                ));
            }
        }
        let in_area = |p: GroundVec2| {
            p.x >= area.min.x && p.x < area.max.x && p.y >= area.min.y && p.y < area.max.y
        };
        for opening in fences.0.openings.iter().filter(|o| in_area(o.at)) {
            let fade = ThinOccluder::post(opening.at, opening.width / 2.0, &terrain.0, 1.6);
            let pieces: Vec<(Transform, bool)> = match opening.kind {
                OpeningKind::FieldGate { double, open } => {
                    let gate = Gate {
                        at: opening.at,
                        along: opening.along,
                        width: opening.width,
                        double,
                        open,
                        frontage: u32::MAX,
                    };
                    gate_pieces(&gate, &terrain.0)
                        .into_iter()
                        .map(|t| (t, false))
                        .collect()
                }
                OpeningKind::Broken => {
                    let (timber, wire) = broken_pieces(opening, &terrain.0);
                    timber
                        .into_iter()
                        .map(|t| (t, false))
                        .chain(wire.into_iter().map(|t| (t, true)))
                        .collect()
                }
            };
            for (transform, wire) in pieces {
                let material = if wire { &kit.wire } else { &kit.timber };
                parts.push((
                    Mesh3d(kit.cube.clone()),
                    MeshMaterial3d(material.clone()),
                    transform,
                    fade,
                    true,
                ));
            }
        }
        let mut laid = commands.entity(chunk);
        laid.try_insert(FencesLaid);
        if parts.is_empty() {
            continue;
        }
        laid.queue_silenced(move |mut chunk: EntityWorldMut| {
            chunk.with_children(|fence| {
                for (mesh, material, transform, fade, gate) in parts {
                    let mut part = fence.spawn((mesh, material, transform, fade));
                    if gate {
                        part.insert(GatePart);
                    } else {
                        part.insert(FencePiece);
                    }
                }
            });
        });
    }
}

/// Los postes y los alambres de unos pocos tramos consecutivos, como dos mallas
/// en el espacio del mundo: un poste al inicio de cada tramo y otro al final
/// del último, y cada alambre de poste a poste.
pub(crate) fn piece_meshes(spans: &[(GroundVec2, GroundVec2)], terrain: &Terrain) -> (Mesh, Mesh) {
    let ground = |p: GroundVec2| terrain.height(p.x, p.y);
    let mut posts = Boxes::default();
    let mut wires = Boxes::default();
    let mut corners: Vec<GroundVec2> = spans.iter().map(|s| s.0).collect();
    corners.extend(spans.last().map(|s| s.1));
    for &p in &corners {
        let h = FenceLayout::HEIGHT + 0.1;
        posts.push(
            Transform::from_translation(world_from_ground(p, ground(p) - 0.1 + h / 2.0))
                .with_scale(Vec3::new(POST, h, POST)),
        );
    }
    for &(a, b) in spans {
        for height in WIRES {
            let (wa, wb) = (
                world_from_ground(a, ground(a) + height),
                world_from_ground(b, ground(b) + height),
            );
            let d = wb - wa;
            let len = d.length();
            if len <= 0.0 {
                continue;
            }
            wires.push(
                Transform::from_translation((wa + wb) / 2.0)
                    .with_rotation(Quat::from_rotation_arc(Vec3::X, d / len))
                    .with_scale(Vec3::new(len, WIRE, WIRE)),
            );
        }
    }
    (posts.into_mesh(), wires.into_mesh())
}

/// La forma de fundido de una pieza: la línea de su primer a su último poste,
/// hasta la punta de los postes.
fn piece_occluder(spans: &[(GroundVec2, GroundVec2)], terrain: &Terrain) -> ThinOccluder {
    let (a, b) = (spans[0].0, spans[spans.len() - 1].1);
    let ground = |p: GroundVec2| terrain.height(p.x, p.y);
    let low = ground(a).min(ground(b)) - 0.5;
    let high = ground(a).max(ground(b)) + FenceLayout::HEIGHT + 0.2;
    ThinOccluder {
        a,
        b,
        radius: 0.4,
        bottom: low,
        top: high,
    }
}

/// Los dos postes robustos de la tranquera y su hoja o sus dos hojas, como
/// transformaciones de cubo unitario: abiertas hacia atrás contra el alambrado
/// junto a la abertura, o cerradas a través de ella.
pub(crate) fn gate_pieces(gate: &Gate, terrain: &Terrain) -> Vec<Transform> {
    let ground = |p: GroundVec2| terrain.height(p.x, p.y);
    let along = gate.along;
    let side = GroundVec2::new(-along.y, along.x);
    let leaves = if gate.double { 2.0 } else { 1.0 };
    let leaf = (gate.width / leaves - 0.6).clamp(1.5, 4.5);
    let mut pieces = Vec::new();
    for s in [-1.0, 1.0] {
        let hinge = gate.at + along * (s * gate.width / 2.0);
        let h = FenceLayout::HEIGHT + 0.35;
        pieces.push(
            Transform::from_translation(world_from_ground(hinge, ground(hinge) - 0.1 + h / 2.0))
                .with_scale(Vec3::new(0.2, h, 0.2)),
        );
        // Una tranquera simple cuelga de un solo poste.
        if !gate.double && s > 0.0 {
            continue;
        }
        // Abierta: girada 170°, la hoja queda hacia atrás a lo largo del
        // alambrado, apenas fuera de su línea y lejos de la abertura. Cerrada:
        // a través de la abertura.
        let (base, dir, leaf) = if gate.open {
            (hinge + side * 0.25, s, leaf)
        } else {
            (hinge, -s, gate.width / leaves - 0.15)
        };
        let tip = base + along * (dir * leaf);
        let mid = (base + tip) * 0.5;
        let yaw = Quat::from_rotation_y(along.y.atan2(along.x));
        for (k, height) in [0.35, 0.7, 1.05, 1.3].into_iter().enumerate() {
            let bar = if k == 3 { 0.1 } else { 0.07 };
            pieces.push(
                Transform::from_translation(world_from_ground(mid, ground(mid) + height))
                    .with_rotation(yaw)
                    .with_scale(Vec3::new(leaf, bar, 0.05)),
            );
        }
        // Una riostra diagonal y el montante en el extremo libre de la hoja.
        let rise = 0.95_f32;
        let brace = Quat::from_rotation_z(rise.atan2(leaf) * if dir > 0.0 { 1.0 } else { -1.0 });
        pieces.push(
            Transform::from_translation(world_from_ground(mid, ground(mid) + 0.82))
                .with_rotation(yaw * brace)
                .with_scale(Vec3::new(leaf.hypot(rise), 0.07, 0.05)),
        );
        pieces.push(
            Transform::from_translation(world_from_ground(tip, ground(tip) + 0.75))
                .with_scale(Vec3::new(0.08, 1.2, 0.08)),
        );
    }
    pieces
}

/// Un trecho roto de alambrado a través de `opening`: postes inclinados o
/// caídos en él, y un alambre que se comba hasta el pasto entre los postes de
/// sus extremos, como transformaciones de cubo unitario (madera, alambre). Nada
/// de esto es un obstáculo.
pub(crate) fn broken_pieces(
    opening: &Opening,
    terrain: &Terrain,
) -> (Vec<Transform>, Vec<Transform>) {
    let ground = |p: GroundVec2| terrain.height(p.x, p.y);
    let along = opening.along;
    let side = GroundVec2::new(-along.y, along.x);
    let half = opening.width / 2.0;
    let (a, b) = (opening.at - along * half, opening.at + along * half);
    let yaw = Quat::from_rotation_y(along.y.atan2(along.x));
    let mut timber = Vec::new();
    // Un poste muy inclinado en el medio, y uno caído en el pasto.
    let lean = opening.at + side * 0.3;
    let h = FenceLayout::HEIGHT + 0.1;
    timber.push(
        Transform::from_translation(world_from_ground(lean, ground(lean) + h * 0.4))
            .with_rotation(yaw * Quat::from_rotation_x(0.55))
            .with_scale(Vec3::new(POST, h, POST)),
    );
    let fallen = opening.at + along * (0.3 * half) - side * 0.9;
    timber.push(
        Transform::from_translation(world_from_ground(fallen, ground(fallen) + POST / 2.0))
            .with_rotation(yaw)
            .with_scale(Vec3::new(h, POST, POST)),
    );
    // El alambre superior sigue colgado de ambos postes extremos y se comba
    // hacia el medio.
    let mut wire = Vec::new();
    let sag = opening.at + side * 0.2;
    let low = world_from_ground(sag, ground(sag) + 0.15);
    for end in [a, b] {
        let high = world_from_ground(end, ground(end) + WIRES[WIRES.len() - 1]);
        let d = low - high;
        let len = d.length();
        if len > 0.0 {
            wire.push(
                Transform::from_translation((high + low) / 2.0)
                    .with_rotation(Quat::from_rotation_arc(Vec3::X, d / len))
                    .with_scale(Vec3::new(len, WIRE, WIRE)),
            );
        }
    }
    (timber, wire)
}

/// Cajas unitarias fusionadas en una malla con sombreado plano.
#[derive(Default)]
struct Boxes {
    positions: Vec<[f32; 3]>,
}

impl Boxes {
    /// Las caras del cubo unitario como cuadriláteros, con las esquinas en
    /// sentido antihorario vistas desde afuera (se omite la base: queda dentro
    /// del suelo o nunca se ve).
    const FACES: [[[f32; 3]; 4]; 5] = [
        [
            [0.5, -0.5, -0.5],
            [0.5, 0.5, -0.5],
            [0.5, 0.5, 0.5],
            [0.5, -0.5, 0.5],
        ],
        [
            [-0.5, -0.5, 0.5],
            [-0.5, 0.5, 0.5],
            [-0.5, 0.5, -0.5],
            [-0.5, -0.5, -0.5],
        ],
        [
            [-0.5, 0.5, -0.5],
            [-0.5, 0.5, 0.5],
            [0.5, 0.5, 0.5],
            [0.5, 0.5, -0.5],
        ],
        [
            [-0.5, -0.5, 0.5],
            [0.5, -0.5, 0.5],
            [0.5, 0.5, 0.5],
            [-0.5, 0.5, 0.5],
        ],
        [
            [0.5, -0.5, -0.5],
            [-0.5, -0.5, -0.5],
            [-0.5, 0.5, -0.5],
            [0.5, 0.5, -0.5],
        ],
    ];

    fn push(&mut self, transform: Transform) {
        let m = transform.to_matrix();
        for face in Self::FACES {
            let c = face.map(|p| m.transform_point3(Vec3::from_array(p)).to_array());
            self.positions.extend([c[0], c[1], c[2], c[0], c[2], c[3]]);
        }
    }

    fn into_mesh(self) -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.compute_normals();
        mesh
    }
}

/// Triángulos de las mallas que dibuja una pieza de alambrado de `spans`
/// tramos.
pub fn piece_triangles(spans: usize) -> usize {
    2 * Boxes::FACES.len() * ((spans + 1) + spans * FenceLayout::WIRES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{StreamFocus, TerrainPlugin};
    use pampa_core::{ChunkCoord, RailLine, WorldConfig};

    fn layout() -> FenceLayout {
        let config = WorldConfig::default();
        FenceLayout::build(config, &Terrain::default(), &[], &RailLine::default())
    }

    fn app(focus: GroundVec2) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .insert_resource(WorldChunks(WorldConfig::default()))
            .insert_resource(WorldFences(layout()))
            .insert_resource(StreamFocus(focus))
            .add_plugins((
                TerrainPlugin {
                    radius: 1,
                    budget: 1,
                    resolution: 2,
                },
                FencePlugin,
            ));
        app
    }

    fn count<T: Component>(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<(), With<T>>()
            .iter(app.world())
            .count()
    }

    #[test]
    fn chunks_draw_their_fence_spans_once_and_drop_them() {
        let fences = layout();
        let x = fences.surveyed.0[5];
        let mut app = app(GroundVec2::new(x, 0.0));
        app.update();
        app.update();
        // Cada tramo de los chunks cargados se dibuja en exactamente una pieza.
        let mut chunks = app.world_mut().query::<&TerrainChunk>();
        let loaded: Vec<ChunkCoord> = chunks.iter(app.world()).map(|c| c.0).collect();
        let size = WorldConfig::default().chunk_size;
        let spans: usize = loaded
            .iter()
            .flat_map(|c| fences.runs_in(c.bounds(size)))
            .map(|r| r.spans().count())
            .sum();
        assert!(spans > 30, "{spans}");
        let pieces: usize = loaded
            .iter()
            .flat_map(|c| fences.runs_in(c.bounds(size)))
            .map(|r| r.spans().count().div_ceil(SPANS_PER_PIECE))
            .sum();
        assert_eq!(count::<FencePiece>(&mut app), 2 * pieces);
        // Una pieza es postes más alambres, y cada uno se desvanece por su
        // cuenta.
        assert_eq!(
            count::<FencePiece>(&mut app),
            app.world_mut()
                .query_filtered::<(), (With<FencePiece>, With<ThinOccluder>)>()
                .iter(app.world())
                .count()
        );
        app.insert_resource(StreamFocus(GroundVec2::new(-3500.0, 3500.0)));
        for _ in 0..40 {
            app.update();
        }
        let far: usize = {
            let mut chunks = app.world_mut().query::<&TerrainChunk>();
            let loaded: Vec<ChunkCoord> = chunks.iter(app.world()).map(|c| c.0).collect();
            loaded
                .iter()
                .flat_map(|c| fences.runs_in(c.bounds(size)))
                .map(|r| r.spans().count().div_ceil(SPANS_PER_PIECE))
                .sum()
        };
        assert_eq!(count::<FencePiece>(&mut app), 2 * far);
    }

    #[test]
    fn chunk_parts_cover_every_span_exactly_once() {
        let fences = layout();
        let size = WorldConfig::default().chunk_size;
        let total: usize = fences.runs.iter().map(|r| r.spans().count()).sum();
        let mut drawn = 0;
        for y in -20..20 {
            for x in -20..20 {
                for run in fences.runs_in(ChunkCoord::new(x, y).bounds(size)) {
                    drawn += run.spans().count();
                }
            }
        }
        assert_eq!(drawn, total);
    }

    #[test]
    fn posts_stand_on_the_ground_with_wires_strung_between() {
        let terrain = Terrain::default();
        let spans = [
            (GroundVec2::new(0.0, 0.0), GroundVec2::new(10.0, 0.0)),
            (GroundVec2::new(10.0, 0.0), GroundVec2::new(20.0, 0.0)),
        ];
        let (posts, wires) = piece_meshes(&spans, &terrain);
        let tris = |m: &Mesh| m.count_vertices() / 3;
        assert_eq!(tris(&posts) + tris(&wires), piece_triangles(2));
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(points)) =
            posts.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!()
        };
        let top = points.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
        let ground = terrain.height(0.0, 0.0).max(terrain.height(20.0, 0.0));
        assert!(top > ground + 1.2 && top < ground + 1.6, "{top}");
        assert!(piece_triangles(SPANS_PER_PIECE) <= 400);
    }

    #[test]
    fn a_broken_span_leans_and_sags_inside_its_opening() {
        let terrain = Terrain::default();
        let opening = Opening {
            at: GroundVec2::new(0.0, 100.0),
            along: GroundVec2::new(1.0, 0.0),
            width: 20.0,
            kind: OpeningKind::Broken,
            stretch: 0,
        };
        let (timber, wire) = broken_pieces(&opening, &terrain);
        assert_eq!((timber.len(), wire.len()), (2, 2));
        for t in timber.iter().chain(&wire) {
            let g = GroundVec2::new(t.translation.x, -t.translation.z);
            assert!(g.distance(opening.at) <= opening.width / 2.0 + 0.1, "{g:?}");
            // Nada queda en pie a toda su altura: está roto.
            let top = t.translation.y - terrain.height(g.x, g.y);
            assert!(top < FenceLayout::HEIGHT, "{top}");
        }
    }

    #[test]
    fn chunks_draw_the_openings_in_them() {
        let fences = layout();
        let o = fences.openings[0];
        let mut app = app(o.at);
        app.update();
        app.update();
        assert!(count::<GatePart>(&mut app) > 0);
    }

    #[test]
    fn open_gate_leaves_stay_off_the_opening() {
        let gate = Gate {
            at: GroundVec2::new(50.0, 0.0),
            along: GroundVec2::new(0.0, 1.0),
            width: 10.0,
            double: true,
            open: true,
            frontage: 0,
        };
        let pieces = gate_pieces(&gate, &Terrain::default());
        assert_eq!(pieces.len(), 2 * (1 + 4 + 2));
        let single = gate_pieces(
            &Gate {
                double: false,
                ..gate
            },
            &Terrain::default(),
        );
        assert_eq!(single.len(), 2 + 4 + 2);
        for t in pieces {
            let g = GroundVec2::new(t.translation.x, -t.translation.z);
            // Nada queda en pie dentro de la abertura entre los postes de la
            // tranquera.
            assert!(g.y.abs() >= gate.width / 2.0 - 0.15, "{g:?}");
        }
        // Cerradas, las hojas se juntan a través de la abertura, en la línea
        // del alambrado.
        let shut = gate_pieces(
            &Gate {
                open: false,
                ..gate
            },
            &Terrain::default(),
        );
        let bars: Vec<GroundVec2> = shut
            .iter()
            .map(|t| GroundVec2::new(t.translation.x, -t.translation.z))
            .filter(|g| g.y.abs() < gate.width / 2.0 - 0.5)
            .collect();
        assert!(bars.len() >= 8, "{bars:?}");
        assert!(bars.iter().all(|g| (g.x - gate.at.x).abs() < 0.05));
    }
}
