use std::collections::HashSet;

use pampa_core::{HouseStyle, SettlementKind, WorldConfig};

use super::*;
use crate::{FloraPlugin, StreamFocus, TerrainPlugin, flora::triangles};

/// Una estructura de tipo `kind` centrada en `(x, y)`, de `w` de ancho a lo
/// largo de su frente y `d` de profundidad, mirando hacia `front`.
fn structure(
    kind: StructureKind,
    (x, y): (f32, f32),
    (w, d): (f32, f32),
    h: f32,
    front: Side,
) -> Structure {
    let size = match front {
        Side::North | Side::South => GroundVec2::new(w, d),
        _ => GroundVec2::new(d, w),
    };
    Structure {
        kind,
        footprint: Aabb::from_center_size(GroundVec2::new(x, y), size),
        height: h,
        front,
        variant: (x * 7.0 + y * 13.0) as u32,
    }
}

/// Una de cada tipo de estructura alrededor de un centro en `hub`, en su tamaño
/// más grande.
fn every_kind(hub: (f32, f32)) -> Settlement {
    use StructureKind::*;
    let (hx, hy) = hub;
    let at = |dx: f32, dy: f32| (hx + dx, hy + dy);
    let structures = vec![
        structure(
            MainHouse(HouseStyle::GalleryHouse),
            at(0.0, 30.0),
            (30.0, 18.0),
            4.5,
            Side::South,
        ),
        structure(
            MainHouse(HouseStyle::AdobeThatch),
            at(-40.0, 0.0),
            (10.0, 5.5),
            2.6,
            Side::East,
        ),
        structure(
            MainHouse(HouseStyle::AdobeIron),
            at(40.0, 0.0),
            (10.0, 5.5),
            2.6,
            Side::West,
        ),
        structure(
            WorkerHouse(HouseStyle::BrickParapet),
            at(0.0, -30.0),
            (16.0, 9.0),
            3.4,
            Side::North,
        ),
        structure(Barn, at(-50.0, 40.0), (30.0, 14.0), 5.5, Side::South),
        structure(Corral, at(50.0, 40.0), (34.0, 34.0), 1.6, Side::South),
        structure(Windpump, at(-30.0, -40.0), (2.4, 2.4), 10.0, Side::North),
        structure(Tank, at(-20.0, -45.0), (10.0, 10.0), 1.6, Side::North),
        structure(Trough, at(-30.0, -50.0), (8.0, 0.9), 0.6, Side::North),
        structure(Well, at(20.0, -15.0), (1.6, 1.6), 0.9, Side::West),
        structure(Haystack, at(60.0, -40.0), (7.0, 7.0), 4.0, Side::West),
        structure(HitchingPost, at(10.0, 12.0), (3.0, 0.3), 1.1, Side::South),
        structure(Cart, at(30.0, -40.0), (5.0, 2.2), 2.5, Side::North),
        structure(WreckedWagon, at(70.0, 10.0), (4.8, 3.6), 2.2, Side::West),
        structure(Eucalyptus, at(-60.0, -10.0), (1.0, 1.0), 26.0, Side::East),
        structure(Poplar, at(-5.0, -60.0), (1.0, 1.0), 18.0, Side::North),
        // El resto de una hilera corta de álamos, separados 8 m, y un segundo
        // eucalipto.
        structure(Poplar, at(-5.0, -68.0), (1.0, 1.0), 16.0, Side::North),
        structure(Poplar, at(-5.0, -76.0), (1.0, 1.0), 19.0, Side::North),
        structure(Poplar, at(-5.0, -84.0), (1.0, 1.0), 17.0, Side::North),
        structure(Eucalyptus, at(-66.0, -14.0), (1.0, 1.0), 22.0, Side::East),
    ];
    let hub = GroundVec2::new(hx, hy);
    let mut tracks = vec![Track {
        points: vec![GroundVec2::new(hx, hy - 90.0), hub],
    }];
    tracks.extend(
        structures
            .iter()
            .filter(|s| s.kind.is_reached())
            .map(|s| Track {
                points: vec![hub, s.door()],
            }),
    );
    Settlement {
        kind: SettlementKind::Ranch,
        gate: 0,
        hub,
        patio: Aabb::from_center_size(hub, GroundVec2::new(24.0, 24.0)),
        bounds: Aabb::from_center_size(hub, GroundVec2::new(200.0, 200.0)),
        structures,
        tracks,
        seed: 1,
    }
}

/// Dos asentamientos: uno en el chunk (0, 0) y otro en el chunk (1, 0).
fn layout() -> SettlementLayout {
    let mut layout = SettlementLayout::default();
    layout.settlements = vec![every_kind((100.0, 100.0)), every_kind((300.0, 100.0))];
    layout
}

fn app(focus: GroundVec2) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        .init_resource::<Assets<Image>>()
        .insert_resource(WorldChunks(WorldConfig::default()))
        .insert_resource(WorldSettlements(layout()))
        .insert_resource(StreamFocus(focus))
        .add_plugins((
            TerrainPlugin {
                radius: 1,
                budget: 16,
                resolution: 4,
            },
            FloraPlugin,
            SettlementPlugin,
        ));
    app
}

fn parts(app: &mut App) -> Vec<SettlementPart> {
    app.world_mut()
        .query::<&SettlementPart>()
        .iter(app.world())
        .copied()
        .collect()
}

fn count<T: Component>(app: &mut App) -> usize {
    app.world_mut()
        .query_filtered::<(), With<T>>()
        .iter(app.world())
        .count()
}

#[test]
fn chunks_draw_each_owned_settlement_once_and_drop_it() {
    // El foco en el chunk (0, 0): los chunks de ambos asentamientos están
    // cargados.
    let mut app = app(GroundVec2::new(100.0, 100.0));
    app.update();
    app.update();
    let drawn = parts(&mut app);
    let layout = layout();
    for (k, s) in layout.settlements.iter().enumerate() {
        // Cada estructura se dibuja, con tantas piezas como superficies tiene:
        // ninguna dos veces.
        for (j, st) in s.structures.iter().enumerate() {
            let pieces = drawn
                .iter()
                .filter(|p| p.settlement == k as u32 && p.structure == j as u32)
                .count();
            assert!(pieces > 0, "{:?} not drawn", st.kind);
            let surfaces: Vec<Surface> = match st.kind {
                StructureKind::MainHouse(_)
                | StructureKind::WorkerHouse(_)
                | StructureKind::Barn => buildings::building(st, 0.3).surfaces().collect(),
                _ => Vec::new(),
            };
            if !surfaces.is_empty() {
                assert_eq!(pieces, surfaces.len(), "{:?}", st.kind);
            }
        }
    }
    assert_eq!(count::<TrackSurface>(&mut app), 2);
    assert_eq!(count::<YardSurface>(&mut app), 2);
    // Las ruedas de los molinos giran.
    assert_eq!(
        count::<WindmillRotor>(&mut app),
        2 * props::rotor().surfaces().count()
    );
    // Volver a colocar no cambia nada: cada chunk coloca sus asentamientos una
    // sola vez.
    let before = drawn.len();
    app.update();
    assert_eq!(parts(&mut app).len(), before);

    // Lejos, los chunks se van y sus asentamientos con ellos.
    app.insert_resource(StreamFocus(GroundVec2::new(-3500.0, 3500.0)));
    for _ in 0..10 {
        app.update();
    }
    assert_eq!(parts(&mut app).len(), 0);
    assert_eq!(count::<TrackSurface>(&mut app), 0);
}

/// Avanza `app` hasta que haya terminado cada construcción en segundo plano del
/// suelo y de los asentamientos (como mucho 20 actualizaciones).
fn land_all(app: &mut App) {
    for _ in 0..20 {
        let pending =
            count::<crate::terrain::PendingGround>(app) + count::<PendingSettlements>(app);
        if pending == 0 {
            return;
        }
        crate::terrain::finish_builds(app, crate::terrain::PendingGround::is_finished);
        crate::terrain::finish_builds(app, PendingSettlements::is_finished);
        app.update();
    }
    panic!("the background builds never landed in 20 updates");
}

#[test]
fn a_streamed_chunk_lays_its_settlements_off_the_main_schedule() {
    // Lo que el primer frame coloca en línea, para comparar.
    let mut inline = app(GroundVec2::new(100.0, 100.0));
    inline.update();
    inline.update();
    let mut expected = parts(&mut inline);
    expected.sort_by_key(|p| (p.settlement, p.structure));
    let tracks = count::<TrackSurface>(&mut inline);
    // Empezando lejos y después acercándose a caballo: los chunks se cargan por
    // streaming.
    let mut app = app(GroundVec2::new(-3500.0, 3500.0));
    app.update();
    app.insert_resource(StreamFocus(GroundVec2::new(100.0, 100.0)));
    app.update();
    // La actualización que cargó los chunks por streaming no construyó sus
    // asentamientos.
    assert_eq!(parts(&mut app).len(), 0);
    assert!(count::<PendingSettlements>(&mut app) > 0);
    // Aparecen una vez construidos: las mismas piezas, una vez cada una.
    land_all(&mut app);
    app.update();
    let mut landed = parts(&mut app);
    landed.sort_by_key(|p| (p.settlement, p.structure));
    assert_eq!(landed, expected);
    assert_eq!(count::<TrackSurface>(&mut app), tracks);
}

#[test]
fn a_chunk_gone_before_its_settlements_land_drops_them() {
    let mut app = app(GroundVec2::new(-3500.0, 3500.0));
    app.update();
    app.insert_resource(StreamFocus(GroundVec2::new(100.0, 100.0)));
    app.update();
    assert!(count::<PendingSettlements>(&mut app) > 0);
    // Otra vez lejos en el frame siguiente, antes de que aparezca nada.
    app.insert_resource(StreamFocus(GroundVec2::new(-3500.0, 3500.0)));
    for _ in 0..10 {
        app.update();
        assert_eq!(parts(&mut app).len(), 0, "a settlement landed");
    }
    assert_eq!(count::<PendingSettlements>(&mut app), 0, "a build stayed");
    assert_eq!(count::<TrackSurface>(&mut app), 0);
}

#[test]
fn settlements_share_a_few_ranges_and_one_material_per_surface() {
    let mut app = app(GroundVec2::new(100.0, 100.0));
    app.update();
    app.update();
    let mut q = app
        .world_mut()
        .query_filtered::<&VisibilityRange, With<SettlementPart>>();
    let distinct: HashSet<VisibilityRange> = q.iter(app.world()).cloned().collect();
    // Bevy nunca libera el slot de GPU de un rango: un puñado, compartido.
    assert!(distinct.len() <= 3, "{} distinct ranges", distinct.len());
    assert!(distinct.iter().all(|r| ranges().contains(r)));
    let mut q = app.world_mut().query_filtered::<
        &MeshMaterial3d<StandardMaterial>,
        Or<(With<SettlementPart>, With<TrackSurface>, With<YardSurface>)>,
    >();
    let materials: HashSet<AssetId<StandardMaterial>> =
        q.iter(app.world()).map(|m| m.id()).collect();
    // Uno por superficie, y la corteza y las hojas de los dos árboles.
    assert!(
        materials.len() <= Surface::ALL.len() + 4,
        "{} materials",
        materials.len()
    );
    // Lo delgado se desvanece al estorbar a la cámara; las casas y los árboles
    // no.
    let mut q = app
        .world_mut()
        .query::<(&SettlementPart, Has<ThinOccluder>)>();
    for (part, fades) in q.iter(app.world()) {
        let thin = matches!(
            part.kind,
            StructureKind::Corral
                | StructureKind::Windpump
                | StructureKind::Well
                | StructureKind::Cart
                | StructureKind::WreckedWagon
                | StructureKind::HitchingPost
                | StructureKind::Haystack
        );
        assert_eq!(fades, thin, "{:?}", part.kind);
    }
}

#[test]
fn every_structure_keeps_to_its_triangle_budget() {
    let s = every_kind((0.0, 0.0));
    for st in &s.structures {
        let tris = structure_triangles(st);
        println!("{:?}: {tris} triangles", st.kind);
        match st.kind {
            k if k.is_tree() => {}
            StructureKind::MainHouse(_) | StructureKind::WorkerHouse(_) | StructureKind::Barn => {
                assert!((150..=20_000).contains(&tris), "{:?}: {tris}", st.kind);
            }
            _ => assert!((30..=5_000).contains(&tris), "{:?}: {tris}", st.kind),
        }
    }
}

#[test]
fn the_drawn_triangles_of_a_settlement_stay_sane() {
    let mut app = app(GroundVec2::new(100.0, 100.0));
    app.update();
    app.update();
    let mut q = app
        .world_mut()
        .query_filtered::<&Mesh3d, Or<(With<SettlementPart>, With<TrackSurface>, With<YardSurface>)>>();
    let meshes = app.world().resource::<Assets<Mesh>>();
    let total: usize = q
        .iter(app.world())
        .map(|m| meshes.get(&m.0).map_or(0, triangles))
        .sum();
    let pieces = q.iter(app.world()).count();
    println!("two settlements of every kind: {total} triangles in {pieces} draws");
    assert!(total < 200_000, "{total}");
}

#[test]
fn tracks_drape_over_the_drawn_ground_with_two_ruts() {
    let terrain = Terrain::default();
    let (size, resolution) = (WorldConfig::default().chunk_size, 8);
    let ground = |p: GroundVec2| mesh_height(&terrain, p, size, resolution);
    let track = Track {
        points: vec![
            GroundVec2::new(-150.0, 20.0),
            GroundVec2::new(30.0, 40.0),
            GroundVec2::new(60.0, 210.0),
        ],
    };
    let mesh = track::tracks_mesh(&[&track], &ground).expect("a mesh");
    let (
        Some(bevy::mesh::VertexAttributeValues::Float32x3(points)),
        Some(bevy::mesh::VertexAttributeValues::Float32x4(colors)),
        Some(bevy::mesh::VertexAttributeValues::Float32x3(normals)),
    ) = (
        mesh.attribute(Mesh::ATTRIBUTE_POSITION),
        mesh.attribute(Mesh::ATTRIBUTE_COLOR),
        mesh.attribute(Mesh::ATTRIBUTE_NORMAL),
    )
    else {
        panic!()
    };
    let half = Track::HALF_WIDTH;
    let (mut ruts, mut middle) = (0, 0);
    for ((p, c), n) in points.iter().zip(colors).zip(normals) {
        let g = GroundVec2::new(p[0], -p[2]);
        let lift = p[1] - ground(g);
        assert!((lift - track::LIFT).abs() < 1e-3, "{lift} at {g:?}");
        assert!(n[1] > 0.0, "faces up");
        // Distancia desde la línea de la huella.
        let d = track
            .segments()
            .map(|(a, b)| {
                let ab = b - a;
                let t = ((g - a).dot(ab) / ab.dot(ab)).clamp(0.0, 1.0);
                g.distance(a + ab * t)
            })
            .fold(f32::MAX, f32::min);
        assert!(d <= half * 1.3, "{d}");
        if d < 0.25 * half {
            middle += 1;
            assert!(c[3] < 0.3, "grass between the ruts");
        }
        if (0.4 * half..=0.9 * half).contains(&d) {
            ruts += 1;
            assert!(c[3] > 0.5, "a rut at {d}");
        }
        if d > 1.2 * half {
            assert_eq!(c[3], 0.0, "feathered edge");
        }
    }
    assert!(ruts > 10 && middle > 10, "{ruts} {middle}");
    // Colocada cada un par de metros: sigue las subidas del suelo.
    assert!(points.len() / 42 >= 180 / 2, "{}", points.len());
}

/// Una pieza dibujada de un árbol: su estructura, mesh, material y transform.
struct TreePiece {
    structure: Structure,
    mesh: AssetId<Mesh>,
    material: AssetId<StandardMaterial>,
    scale: Vec3,
}

/// La estructura de tipo `kind` que está en `at` (traslación en el mundo) en la
/// disposición de prueba.
fn spot(kind: StructureKind, at: Vec3) -> Structure {
    layout()
        .settlements
        .iter()
        .flat_map(|s| s.structures.iter())
        .find(|s| {
            s.kind == kind && s.footprint.center().distance(GroundVec2::new(at.x, -at.z)) < 1e-3
        })
        .cloned()
        .unwrap_or_else(|| panic!("no {kind:?} at {at:?}"))
}

/// Todas las piezas dibujadas de un árbol de estancia, corteza y hojas.
fn tree_pieces(app: &mut App) -> Vec<TreePiece> {
    let mut q = app.world_mut().query::<(
        &SettlementPart,
        &Mesh3d,
        &MeshMaterial3d<StandardMaterial>,
        &Transform,
    )>();
    q.iter(app.world())
        .filter(|(p, ..)| p.kind.is_tree())
        .map(|(p, mesh, material, t)| TreePiece {
            structure: spot(p.kind, t.translation),
            mesh: mesh.id(),
            material: material.id(),
            scale: t.scale,
        })
        .collect()
}

/// La especie del kit de árboles para un tipo de estructura de árbol.
fn kit_species(kind: StructureKind) -> crate::tree::Species {
    if kind == StructureKind::Poplar {
        crate::tree::Species::Alamo
    } else {
        crate::tree::Species::Eucalipto
    }
}

fn settled_app() -> App {
    let mut app = app(GroundVec2::new(100.0, 100.0));
    app.update();
    app.update();
    app
}

#[test]
fn estancia_trees_are_tree_kit_trees_of_their_species() {
    let mut app = settled_app();
    let kit = app.world().resource::<crate::tree::TreeKit>().clone();
    let pieces = tree_pieces(&mut app);
    for kind in [StructureKind::Poplar, StructureKind::Eucalyptus] {
        let species = kit.species(kit_species(kind));
        let of_kind: Vec<_> = pieces.iter().filter(|p| p.structure.kind == kind).collect();
        assert!(!of_kind.is_empty(), "{kind:?}");
        let (mut barks, mut leaves) = (0, 0);
        for p in of_kind {
            if p.material == species.bark.id() {
                assert!(species.variants.iter().any(|v| v.bark.id() == p.mesh));
                barks += 1;
            } else {
                assert_eq!(p.material, species.leaves.id(), "{kind:?}: wrong paint");
                assert!(species.variants.iter().any(|v| v.leaves.id() == p.mesh));
                leaves += 1;
            }
        }
        // Cada árbol dibuja su tronco y sus hojas, una pieza cada uno.
        assert!(barks >= 1 && barks == leaves, "{kind:?}: {barks} {leaves}");
    }
}

#[test]
fn an_estancia_tree_stands_as_tall_as_its_structure() {
    let mut app = settled_app();
    let kit = app.world().resource::<crate::tree::TreeKit>().clone();
    for p in tree_pieces(&mut app) {
        let species = kit.species(kit_species(p.structure.kind));
        let variant = species
            .variants
            .iter()
            .find(|v| v.bark.id() == p.mesh || v.leaves.id() == p.mesh)
            .expect("a kit variant");
        // Nunca más ancho que alto, y redondo.
        assert!(p.scale.x <= p.scale.y + 1e-4 && (p.scale.x - p.scale.z).abs() < 1e-4);
        let top = variant.height * p.scale.y;
        assert!(
            (top - p.structure.height).abs() < 0.02 * p.structure.height,
            "{:?}: drawn {top} m, structure {} m",
            p.structure.kind,
            p.structure.height
        );
    }
}

#[test]
fn a_poplar_is_a_narrow_column_and_a_eucalyptus_a_broader_crown() {
    let mut app = settled_app();
    let pieces = tree_pieces(&mut app);
    let meshes = app.world().resource::<Assets<Mesh>>();
    let kit = app.world().resource::<crate::tree::TreeKit>();
    let mut seen = HashSet::new();
    for p in &pieces {
        let species = kit.species(kit_species(p.structure.kind));
        if !species.variants.iter().any(|v| v.leaves.id() == p.mesh) {
            continue;
        }
        let size = extent(meshes.get(p.mesh).unwrap()) * p.scale;
        let wide = size.x.max(size.z);
        seen.insert(p.structure.kind);
        if p.structure.kind == StructureKind::Poplar {
            assert!(p.structure.height >= 3.5 * wide, "{size:?}");
        } else {
            // Un eucalipto se extiende: más ancho que la columna de un álamo,
            // pero sigue siendo un árbol alto.
            assert!(wide > 0.25 * p.structure.height, "{size:?}");
            assert!(p.structure.height > 1.5 * wide, "{size:?}");
        }
    }
    assert_eq!(seen.len(), 2, "both species measured");
}

#[test]
fn an_estancia_tree_costs_a_few_hundred_triangles() {
    let mut app = settled_app();
    let pieces = tree_pieces(&mut app);
    let meshes = app.world().resource::<Assets<Mesh>>();
    // La corteza y las hojas de un árbol se suman, indexadas por dónde está.
    let mut costs: std::collections::HashMap<(u32, u32), usize> = Default::default();
    for p in &pieces {
        let at = p.structure.footprint.center();
        *costs.entry((at.x.to_bits(), at.y.to_bits())).or_default() +=
            triangles(meshes.get(p.mesh).unwrap());
    }
    assert!(!costs.is_empty());
    for tris in costs.into_values() {
        assert!((200..=1200).contains(&tris), "{tris}");
    }
}

#[test]
fn an_estancia_trunk_stands_within_the_collider_core_plants_for_it() {
    let mut app = settled_app();
    let kit = app.world().resource::<crate::tree::TreeKit>().clone();
    let pieces = tree_pieces(&mut app);
    let meshes = app.world().resource::<Assets<Mesh>>();
    let mut checked = 0;
    for p in pieces {
        let species = kit.species(kit_species(p.structure.kind));
        if !species.variants.iter().any(|v| v.bark.id() == p.mesh) {
            continue;
        }
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(points)) = meshes
            .get(p.mesh)
            .unwrap()
            .attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!()
        };
        let foot = points
            .iter()
            .filter(|q| q[1] < 0.5)
            .map(|q| Vec2::new(q[0], q[2]).length())
            .fold(0.0, f32::max);
        let drawn = foot * p.scale.x;
        let core = p.structure.trunk_radius();
        // Un jinete se detiene en la corteza, nunca dentro de ella.
        assert!(
            drawn <= core * 1.02,
            "{:?}: bark {drawn} m, core {core} m",
            p.structure.kind
        );
        checked += 1;
    }
    assert!(checked >= 2);
}

#[test]
fn a_row_of_estancia_trees_is_not_cloned_from_one_variant() {
    let mut app = settled_app();
    let kit = app.world().resource::<crate::tree::TreeKit>().clone();
    let pieces = tree_pieces(&mut app);
    for kind in [StructureKind::Poplar, StructureKind::Eucalyptus] {
        let barks: HashSet<AssetId<Mesh>> = pieces
            .iter()
            .filter(|p| {
                p.structure.kind == kind && p.material == kit.species(kit_species(kind)).bark.id()
            })
            .map(|p| p.mesh)
            .collect();
        assert!(barks.len() >= 2, "{kind:?}: one tree for all");
        assert!(barks.len() <= crate::tree::VARIANTS, "{kind:?}");
    }
}

/// La extensión de `mesh` a lo largo de cada eje.
fn extent(mesh: &Mesh) -> Vec3 {
    let Some(bevy::mesh::VertexAttributeValues::Float32x3(points)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        panic!()
    };
    let (lo, hi) = points.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), p| {
        let p = Vec3::from_array(*p);
        (lo.min(p), hi.max(p))
    });
    hi - lo
}

#[test]
fn estancia_trees_need_the_tree_kit_and_not_the_vegetation_models() {
    // Sin plugin de flora: ni los modelos de vegetación ni sus sustitutos.
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        .init_resource::<Assets<Image>>()
        .insert_resource(WorldChunks(WorldConfig::default()))
        .insert_resource(WorldSettlements(layout()))
        .insert_resource(StreamFocus(GroundVec2::new(100.0, 100.0)))
        .add_plugins((
            TerrainPlugin {
                radius: 1,
                budget: 16,
                resolution: 4,
            },
            SettlementPlugin,
        ));
    app.update();
    app.update();
    let drawn = parts(&mut app);
    assert!(drawn.iter().any(|p| p.kind == StructureKind::Barn), "drawn");
    for kind in [StructureKind::Poplar, StructureKind::Eucalyptus] {
        assert!(drawn.iter().any(|p| p.kind == kind), "{kind:?} not drawn");
    }
}

#[test]
fn a_settlement_tree_is_solid_and_never_fades() {
    let mut app = app(GroundVec2::new(100.0, 100.0));
    app.update();
    app.update();
    let mut q = app
        .world_mut()
        .query::<(&SettlementPart, Has<ThinOccluder>)>();
    let trees: Vec<(StructureKind, bool)> = q
        .iter(app.world())
        .filter(|(p, _)| p.kind.is_tree())
        .map(|(p, fades)| (p.kind, fades))
        .collect();
    for kind in [StructureKind::Poplar, StructureKind::Eucalyptus] {
        assert!(trees.iter().any(|t| t.0 == kind), "{kind:?}");
    }
    assert!(trees.iter().all(|t| !t.1), "{trees:?}");
}

/// Una app que carga modelos glTF desde `root` y dibuja los dos asentamientos
/// de prueba con los ajustes de slot indicados.
fn model_app(root: &crate::AssetRoot, fits: ModelSlots) -> App {
    let mut app = crate::gltf::tests::unfinished_app(root);
    app.insert_resource(root.clone())
        .insert_resource(fits)
        .insert_resource(WorldChunks(WorldConfig::default()))
        .insert_resource(WorldSettlements(layout()))
        .insert_resource(StreamFocus(GroundVec2::new(100.0, 100.0)))
        .add_plugins((
            TerrainPlugin {
                radius: 1,
                budget: 16,
                resolution: 4,
            },
            FloraPlugin,
            SettlementPlugin,
        ));
    app.finish();
    app.cleanup();
    app
}

/// Actualiza `app` hasta que ningún modelo de slot esté esperando o sustituido;
/// hace panic después de 10 s.
fn settle_models(app: &mut App) {
    for _ in 0..2000 {
        app.update();
        let waiting = count::<slots::SlotPending>(app) + count::<StandIn>(app);
        let laid = !parts(app).is_empty();
        if laid && waiting == 0 {
            // Un frame más para que las últimas escenas se creen y se preparen.
            app.update();
            app.update();
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("timed out after 10 s waiting for the slot models to load");
}

/// Los tipos de estructura dibujados con una mesh procedural y con un modelo.
fn drawn_kinds(app: &mut App) -> (Vec<StructureKind>, Vec<StructureKind>) {
    let mut q = app
        .world_mut()
        .query::<(&SettlementPart, Has<Mesh3d>, Has<SceneDress>)>();
    let (mut meshes, mut scenes) = (Vec::new(), Vec::new());
    for (part, mesh, scene) in q.iter(app.world()) {
        if mesh {
            meshes.push(part.kind);
        }
        if scene {
            scenes.push(part.kind);
        }
    }
    (meshes, scenes)
}

/// Todas las entidades bajo `root`, en profundidad primero.
fn descendants(world: &World, root: Entity) -> Vec<Entity> {
    let mut found = Vec::new();
    let mut stack = vec![root];
    while let Some(e) = stack.pop() {
        if let Some(children) = world.entity(e).get::<Children>() {
            for &child in children {
                found.push(child);
                stack.push(child);
            }
        }
    }
    found
}

/// Las meshes en espacio de mundo creadas bajo cada escena de modelo, con el
/// rango y el fundido con los que se prepararon.
fn dressed_meshes(app: &mut App) -> Vec<(Entity, Option<VisibilityRange>, Option<ThinOccluder>)> {
    let roots: Vec<Entity> = app
        .world_mut()
        .query_filtered::<Entity, With<SceneDress>>()
        .iter(app.world())
        .collect();
    let mut found = Vec::new();
    let world = app.world_mut();
    let mut meshes =
        world.query::<(Has<Mesh3d>, Option<&VisibilityRange>, Option<&ThinOccluder>)>();
    for root in roots {
        for e in descendants(world, root) {
            if let Ok((true, range, fade)) = meshes.get(world, e) {
                found.push((e, range.cloned(), fade.copied()));
            }
        }
    }
    found
}

#[test]
fn a_slot_model_replaces_the_mesh_and_a_broken_or_missing_one_does_not() {
    use crate::gltf::tests::Scratch;
    let scratch = Scratch::new("slot-models");
    // El slot del pozo contiene el triángulo de prueba; el de la carreta, un
    // archivo roto; el del molino, nada en absoluto.
    scratch.write(
        "models/third-party/well/model.gltf",
        include_str!("../../../../assets/models/test/triangle.gltf"),
    );
    scratch.write("models/third-party/cart/model.gltf", "{ \"asset\": ");
    let root = crate::AssetRoot(scratch.0.clone());
    let fit = SlotFit {
        scale: 2.0,
        ..SlotFit::default()
    };
    let mut app = model_app(&root, ModelSlots([("well".to_owned(), fit)].into()));
    settle_models(&mut app);
    let (meshes, scenes) = drawn_kinds(&mut app);
    // Dos asentamientos, un pozo cada uno: dibujados por el modelo, nunca por
    // la mesh.
    assert_eq!(
        scenes.iter().filter(|k| **k == StructureKind::Well).count(),
        2
    );
    assert!(
        !meshes.contains(&StructureKind::Well),
        "the well's mesh stayed"
    );
    // La carreta rota y el molino que falta siguen siendo procedurales.
    assert!(!scenes.contains(&StructureKind::Cart));
    assert!(meshes.contains(&StructureKind::Cart) && meshes.contains(&StructureKind::Windpump));
    assert_eq!(count::<StandIn>(&mut app), 0);
    // El triángulo del modelo cuelga bajo cada pozo, ajustado, preparado con el
    // rango compartido de los accesorios y el fundido del pozo.
    let dressed = dressed_meshes(&mut app);
    assert_eq!(dressed.len(), 2);
    for (_, range, fade) in &dressed {
        assert!(range.as_ref() == Some(&ranges()[1]));
        assert!(fade.is_some());
    }
    let mut q = app
        .world_mut()
        .query_filtered::<(&SettlementPart, &Transform), With<SceneDress>>();
    for (part, transform) in q.iter(app.world()) {
        assert_eq!(part.kind, StructureKind::Well);
        assert!((transform.scale - Vec3::splat(2.0)).length() < 1e-5);
    }
    // Todavía unos pocos rangos compartidos en total, modelos incluidos.
    let mut q = app
        .world_mut()
        .query_filtered::<&VisibilityRange, With<SettlementPart>>();
    let ours: HashSet<VisibilityRange> = q
        .iter(app.world())
        .cloned()
        .chain(dressed.iter().filter_map(|d| d.1.clone()))
        .collect();
    assert!(ours.len() <= 3 && ours.iter().all(|r| ranges().contains(r)));
}

#[test]
fn the_shipped_windpump_model_spins_its_rotor_and_replaces_the_mesh() {
    let root = crate::gltf::tests::root();
    let fits = ModelSlots(
        [(
            "windpump".to_owned(),
            SlotFit {
                rotor: Some(SlotRotor {
                    node: "rotor".to_owned(),
                    axis: Dir3::NEG_Z,
                }),
                ..SlotFit::default()
            },
        )]
        .into(),
    );
    let mut app = model_app(&root, fits);
    settle_models(&mut app);
    let (meshes, scenes) = drawn_kinds(&mut app);
    assert_eq!(
        scenes
            .iter()
            .filter(|k| **k == StructureKind::Windpump)
            .count(),
        2
    );
    assert!(!meshes.contains(&StructureKind::Windpump));
    // Las ruedas procedurales se fueron con sus sustitutos.
    assert_eq!(count::<WindmillRotor>(&mut app), 0);
    let mut q = app.world_mut().query::<(&ModelRotor, &Name, &Transform)>();
    let before: Vec<(Quat, f32)> = q
        .iter(app.world())
        .map(|(r, name, t)| {
            assert_eq!(name.as_str(), "rotor");
            assert_eq!(r.axis, Dir3::NEG_Z);
            (t.rotation, r.speed)
        })
        .collect();
    assert_eq!(before.len(), 2, "one spinning rotor per windpump");
    for _ in 0..5 {
        std::thread::sleep(std::time::Duration::from_millis(20));
        app.update();
    }
    let after: Vec<Quat> = q.iter(app.world()).map(|(_, _, t)| t.rotation).collect();
    for ((was, speed), now) in before.iter().zip(&after) {
        assert!(*speed > 1.0);
        assert!(was.angle_between(*now) > 1e-3, "the rotor did not turn");
        // Solo alrededor de su eje: el eje se queda en su lugar.
        assert!((*was * Vec3::NEG_Z).distance(*now * Vec3::NEG_Z) < 1e-4);
    }
}

#[test]
fn every_slot_name_is_a_model_folder_and_every_folder_a_slot() {
    use std::collections::BTreeSet;
    let root = crate::gltf::tests::root();
    for slot in StructureKind::SLOTS {
        let path = format!("{}/{slot}/model.glb", slots::SLOT_DIR);
        assert!(crate::gltf::is_inside(&path), "{slot}");
    }
    // Cada carpeta junto a los paquetes de origen es un slot conocido.
    let folders: BTreeSet<String> = std::fs::read_dir(root.0.join(slots::SLOT_DIR))
        .unwrap()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name != "3td-starter-pack")
        .collect();
    for folder in &folders {
        assert!(
            StructureKind::SLOTS.contains(&folder.as_str()),
            "unknown slot {folder}"
        );
        assert!(
            slots::slot_path(&root, slots::SLOT_DIR, folder).is_some(),
            "{folder} has no model.glb or model.gltf"
        );
    }
    assert!(folders.contains("windpump") && folders.contains("wrecked_wagon"));
    // La carreta sigue siendo procedural: el carro de 3TD es una ruina, no una
    // carreta.
    assert!(slots::slot_path(&root, slots::SLOT_DIR, "cart").is_none());
}

#[test]
fn yard_props_keep_off_tracks_and_structures_and_barrels_stay_rare() {
    let mut ranch = every_kind((0.0, 0.0));
    let spots = yard::yard_props(&ranch);
    assert_eq!(spots, yard::yard_props(&ranch), "the same every time");
    let has = |p: YardProp| spots.iter().any(|s| s.prop == p);
    assert!(has(YardProp::Lantern) && has(YardProp::Bucket) && has(YardProp::Barrels));
    assert!(spots.iter().filter(|s| s.prop == YardProp::Barrels).count() <= 1);
    for spot in spots.iter().filter(|s| !s.hung) {
        for (a, b) in ranch.tracks.iter().flat_map(Track::segments) {
            let ab = b - a;
            let t = ((spot.at - a).dot(ab) / ab.dot(ab)).clamp(0.0, 1.0);
            let d = spot.at.distance(a + ab * t);
            assert!(d > Track::HALF_WIDTH, "{spot:?} on a track");
        }
        for s in &ranch.structures {
            assert!(!s.footprint.contains_point(spot.at), "{spot:?} in {s:?}");
        }
    }
    // Los faroles colgados cuelgan junto a la pared del frente de un galpón,
    // por encima de la cabeza de un hombre.
    for spot in spots.iter().filter(|s| s.hung) {
        assert_eq!(spot.prop, YardProp::Lantern);
        assert!(spot.lift > 2.0);
    }
    // Solo una estancia recibe los barriles pesados.
    ranch.kind = pampa_core::SettlementKind::Farm;
    assert!(
        yard::yard_props(&ranch)
            .iter()
            .all(|s| s.prop != YardProp::Barrels)
    );
}

#[test]
fn yard_props_spawn_as_scenes_and_the_lantern_glass_is_see_through() {
    let mut app = model_app(&crate::gltf::tests::root(), ModelSlots::default());
    settle_models(&mut app);
    // Esperar a que se cree la escena de cada accesorio.
    let spawned = |app: &mut App| {
        let roots: Vec<Entity> = app
            .world_mut()
            .query_filtered::<Entity, With<YardPart>>()
            .iter(app.world())
            .collect();
        !roots.is_empty()
            && roots
                .iter()
                .all(|&r| !descendants(app.world(), r).is_empty())
    };
    for k in 0..=2000 {
        if spawned(&mut app) {
            app.update();
            break;
        }
        assert!(k < 2000, "timed out after 10 s waiting for the yard props");
        app.update();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let props: Vec<YardPart> = app
        .world_mut()
        .query::<&YardPart>()
        .iter(app.world())
        .copied()
        .collect();
    let expected: usize = layout()
        .settlements
        .iter()
        .map(|s| yard::yard_props(s).len())
        .sum();
    assert_eq!(props.len(), expected);
    for (_, range, _) in dressed_meshes(&mut app) {
        assert!(range.is_some_and(|r| ranges().contains(&r)));
    }
    // Los vidrios del farol: con mezcla, y ahora translúcidos.
    let lanterns: Vec<Entity> = app
        .world_mut()
        .query::<(Entity, &YardPart)>()
        .iter(app.world())
        .filter(|(_, p)| p.prop == YardProp::Lantern)
        .map(|(e, _)| e)
        .collect();
    assert!(!lanterns.is_empty());
    let world = app.world_mut();
    let mut looks = world.query::<&MeshMaterial3d<StandardMaterial>>();
    let mut panes = 0;
    for lantern in lanterns {
        for part in descendants(world, lantern) {
            let Ok(look) = looks.get(world, part) else {
                continue;
            };
            let material = world
                .resource::<Assets<StandardMaterial>>()
                .get(&look.0)
                .unwrap();
            if material.alpha_mode == AlphaMode::Blend {
                panes += 1;
                assert!(material.base_color.alpha() <= slots::GLASS_ALPHA + 1e-6);
            }
        }
    }
    assert!(panes > 0, "the lantern has its glass");
}

#[test]
fn a_slot_model_that_fails_after_laying_leaves_its_stand_in_for_good() {
    use crate::gltf::tests::{Scratch, app_at};
    let scratch = Scratch::new("late-failure");
    scratch.write("models/third-party/cart/model.gltf", "not json");
    let root = crate::AssetRoot(scratch.0.clone());
    let mut app = app_at(&root);
    app.add_systems(Update, slots::swap_in_models);
    let server = app.world().resource::<AssetServer>().clone();
    let scene = crate::gltf::load_scene(&server, Some(&root), "models/third-party/cart/model.gltf")
        .expect("the file exists");
    // Colocado mientras el modelo todavía cargaba: una escena en espera y su
    // sustituto bajo un chunk.
    let part = SettlementPart {
        settlement: 0,
        structure: 3,
        kind: StructureKind::Cart,
    };
    let chunk = app.world_mut().spawn(Transform::default()).id();
    let waiting = app
        .world_mut()
        .spawn((slots::SlotPending(scene), part, ChildOf(chunk)))
        .id();
    let stand_in = app
        .world_mut()
        .spawn((StandIn, part, ChildOf(chunk), Transform::default()))
        .id();
    for k in 0..=2000 {
        app.update();
        if app.world().get_entity(waiting).is_err() {
            break;
        }
        assert!(
            k < 2000,
            "timed out after 10 s waiting for the model to fail"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    app.update();
    let kept = app.world().entity(stand_in);
    assert!(!kept.contains::<StandIn>(), "still waiting to be replaced");
    assert_eq!(kept.get::<SettlementPart>(), Some(&part));
}

/// Carga `paths` (con sus indicadores sRGB) desde los assets incluidos en una
/// app headless cuyo cargador, como el de una GPU de escritorio, acepta
/// formatos BC, y espera hasta que estén todos; hace panic ante una carga
/// fallida o después de 10 s.
///
/// Bevy 0.20 no puede transcodificar UASTC a RGBA8 simple (divide los niveles
/// UASTC según el tamaño de bloque de la salida y hace panic), así que estos
/// tests, igual que el juego, necesitan un formato de GPU: BC7, ASTC o ETC2.
fn load_shipped(paths: &[(String, bool)]) -> (App, Vec<Handle<Image>>) {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        crate::gltf::asset_plugin(&crate::gltf::tests::root()),
    ))
    .init_asset::<Image>()
    .register_asset_loader(bevy::image::ImageLoader::new(
        bevy::image::CompressedImageFormats::BC,
    ));
    let server = app.world().resource::<AssetServer>().clone();
    let handles: Vec<Handle<Image>> = paths
        .iter()
        .map(|(path, srgb)| materials::load_map(&server, path, *srgb))
        .collect();
    for k in 0..=2000 {
        app.update();
        for (handle, (path, _)) in handles.iter().zip(paths) {
            let state = server.load_state(handle);
            assert!(!state.is_failed(), "{path}: {state:?}");
        }
        if handles.iter().all(|h| server.is_loaded(h)) {
            break;
        }
        assert!(k < 2000, "timed out after 10 s loading the textures");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    (app, handles)
}

/// Cuántas superficies reciben una textura de color cuando la GPU admite
/// `formats`.
fn textured_surfaces(formats: bevy::image::CompressedImageFormats) -> usize {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        crate::gltf::asset_plugin(&crate::gltf::tests::root()),
    ))
    .init_asset::<Image>()
    .init_asset::<StandardMaterial>()
    .register_asset_loader(bevy::image::ImageLoader::new(formats))
    .insert_resource(crate::gltf::tests::root())
    .insert_resource(bevy::image::CompressedImageFormatSupport(formats))
    .add_systems(Startup, materials::setup_materials);
    app.update();
    let built = app.world().resource::<materials::FacadeMaterials>();
    let assets = app.world().resource::<Assets<StandardMaterial>>();
    Surface::ALL
        .iter()
        .filter(|&&s| {
            assets
                .get(&built.get(s))
                .is_some_and(|m| m.base_color_texture.is_some())
        })
        .count()
}

#[test]
fn a_gpu_without_compressed_formats_draws_plain_colour() {
    // UASTC necesita BC, ASTC o ETC2; sin ninguno, el cargador de Bevy 0.20
    // hace panic, así que ningún mapa puede siquiera empezar a cargarse.
    assert_eq!(
        textured_surfaces(bevy::image::CompressedImageFormats::NONE),
        0
    );
    assert!(textured_surfaces(bevy::image::CompressedImageFormats::BC) > 0);
}

#[test]
fn every_set_ships_its_three_ktx2_maps_as_bc7_with_baked_mips() {
    use bevy::render::render_resource::TextureFormat;
    // Un conjunto que se usa con varios tintes se carga una sola vez.
    let sets: std::collections::BTreeSet<&str> =
        Surface::ALL.iter().filter_map(|s| s.set()).collect();
    let paths: Vec<(String, bool)> = sets
        .into_iter()
        .flat_map(|set| materials::MAPS.map(|(name, srgb)| (materials::map_path(set, name), srgb)))
        .collect();
    assert_eq!(paths.len(), 20 * 3);
    let (app, handles) = load_shipped(&paths);
    let images = app.world().resource::<Assets<Image>>();
    for (handle, (path, srgb)) in handles.iter().zip(&paths) {
        let image = images.get(handle).expect("loaded");
        let format = if *srgb {
            TextureFormat::Bc7RgbaUnormSrgb
        } else {
            TextureFormat::Bc7RgbaUnorm
        };
        assert_eq!(image.texture_descriptor.format, format, "{path}");
        assert_eq!(image.width(), 1024, "{path}");
        assert_eq!(
            image.texture_descriptor.mip_level_count, 11,
            "{path}: mips down to 1×1"
        );
        assert!(
            matches!(image.sampler, bevy::image::ImageSampler::Descriptor(ref d)
                if d.anisotropy_clamp == 8),
            "{path}: the repeating anisotropic sampler"
        );
    }
}

#[test]
fn yard_plants_stand_by_the_houses_off_tracks_structures_and_the_patio() {
    let ranch = every_kind((100.0, 100.0));
    let spots = yard::yard_plants(&ranch);
    assert_eq!(spots, yard::yard_plants(&ranch), "the same every time");
    // Cuatro casas: arbustos en todas, un segundo en las casas principales y
    // canteros en algunas.
    let shrubs = spots.iter().filter(|s| s.kind == PlantKind::Shrub).count();
    let beds = spots
        .iter()
        .filter(|s| s.kind == PlantKind::Flowers)
        .count();
    assert!((4..=8).contains(&shrubs), "{shrubs} shrubs");
    assert!((1..=4).contains(&beds), "{beds} flower beds");
    for spot in &spots {
        let reach = spot.kind.radius();
        // No sobre una huella (de 2.6 m de ancho), una estructura, el patio de
        // 24 m, y dentro.
        for (a, b) in ranch.tracks.iter().flat_map(Track::segments) {
            let ab = b - a;
            let t = ((spot.at - a).dot(ab) / ab.dot(ab)).clamp(0.0, 1.0);
            assert!(
                spot.at.distance(a + ab * t) > 1.3 + reach,
                "{spot:?} on a huella"
            );
        }
        assert!(
            ranch
                .structures
                .iter()
                .all(|s| !s.footprint.expanded(reach).contains_point(spot.at)),
            "{spot:?} on a structure"
        );
        assert!(
            !ranch.patio.contains_point(spot.at),
            "{spot:?} on the patio"
        );
        assert!(ranch.bounds.contains_point(spot.at));
        // Cerca de una casa: a menos de 4 m de su pared.
        let by_house = ranch
            .structures
            .iter()
            .filter(|s| s.kind.is_house())
            .any(|s| s.footprint.closest_point(spot.at).distance(spot.at) < 4.0);
        assert!(by_house, "{spot:?} far from every house");
    }
}

#[test]
fn settlements_draw_their_yard_plants_from_the_shared_kit() {
    let mut app = app(GroundVec2::new(100.0, 100.0));
    for _ in 0..3 {
        app.update();
    }
    land_all(&mut app);
    let want: usize = layout()
        .settlements
        .iter()
        .map(|s| yard::yard_plants(s).len())
        .sum();
    assert!(want > 8, "{want}");
    let kit = app
        .world()
        .resource::<crate::tree::TreeKit>()
        .plants
        .clone();
    let mut q = app
        .world_mut()
        .query::<(&Mesh3d, &MeshMaterial3d<StandardMaterial>, &YardPlantPart)>();
    let mut drawn = 0;
    for (mesh, material, _) in q.iter(app.world()) {
        drawn += 1;
        assert!(
            kit.shrubs
                .iter()
                .chain(&kit.flowers)
                .any(|h| h.id() == mesh.id())
        );
        assert_eq!(material.id(), kit.material.id());
    }
    // Solo se coloca el chunk en foco; ambos asentamientos cuando el foco está
    // cerca.
    assert!(drawn > 0 && drawn <= want, "{drawn} of {want}");
}

#[test]
fn a_yard_plant_that_would_land_on_a_track_a_structure_the_patio_or_outside_is_left_out() {
    let ranch = every_kind((100.0, 100.0));
    let spots = yard::yard_plants(&ranch);
    let spot = spots[0].at;
    let left_out =
        |settlement: &Settlement| !yard::yard_plants(settlement).iter().any(|s| s.at == spot);
    assert!(!left_out(&ranch));
    // Una huella que lo atraviesa.
    let mut track = ranch.clone();
    track.tracks.push(Track {
        points: vec![
            spot - GroundVec2::new(5.0, 0.0),
            spot + GroundVec2::new(5.0, 0.0),
        ],
    });
    assert!(left_out(&track), "a plant on a track");
    // Un accesorio apoyado sobre él.
    let mut built = ranch.clone();
    built.structures.push(structure(
        StructureKind::Cart,
        (spot.x, spot.y),
        (2.0, 2.0),
        1.0,
        Side::North,
    ));
    assert!(left_out(&built), "a plant under a structure");
    // El patio crecido sobre él, y el asentamiento achicado lejos de él.
    let mut patio = ranch.clone();
    patio.patio = Aabb::from_center_size(spot, GroundVec2::new(8.0, 8.0));
    assert!(left_out(&patio), "a plant on the patio");
    let mut outside = ranch.clone();
    outside.bounds = Aabb::from_center_size(
        spot + GroundVec2::new(500.0, 0.0),
        GroundVec2::new(10.0, 10.0),
    );
    assert!(left_out(&outside), "a plant outside the settlement");
}
