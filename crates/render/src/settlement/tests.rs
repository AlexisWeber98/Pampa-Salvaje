use std::collections::HashSet;

use pampa_core::{HouseStyle, SettlementKind, WorldConfig};

use super::*;
use crate::{FloraPlugin, StreamFocus, TerrainPlugin, flora::triangles};

/// A structure of `kind` centred on `(x, y)`, `w` wide along its front and
/// `d` deep, facing `front`.
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

/// One of every structure kind round a hub at `hub`, at their largest.
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
        // The rest of a short poplar row, 8 m apart, and a second eucalyptus.
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

/// Two settlements: one in chunk (0, 0), one in chunk (1, 0).
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
    // The focus on chunk (0, 0): both settlements' chunks are loaded.
    let mut app = app(GroundVec2::new(100.0, 100.0));
    app.update();
    app.update();
    let drawn = parts(&mut app);
    let layout = layout();
    for (k, s) in layout.settlements.iter().enumerate() {
        // Every structure is drawn, with as many pieces as it has
        // surfaces: none twice.
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
    // The windpumps' wheels turn.
    assert_eq!(
        count::<WindmillRotor>(&mut app),
        2 * props::rotor().surfaces().count()
    );
    // Laying again changes nothing: each chunk lays its settlements once.
    let before = drawn.len();
    app.update();
    assert_eq!(parts(&mut app).len(), before);

    // Far away, the chunks go and their settlements with them.
    app.insert_resource(StreamFocus(GroundVec2::new(-3500.0, 3500.0)));
    for _ in 0..10 {
        app.update();
    }
    assert_eq!(parts(&mut app).len(), 0);
    assert_eq!(count::<TrackSurface>(&mut app), 0);
}

/// Steps `app` until every background ground and settlement build has
/// landed (at most 20 updates).
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
    // What the first frame lays inline, for comparison.
    let mut inline = app(GroundVec2::new(100.0, 100.0));
    inline.update();
    inline.update();
    let mut expected = parts(&mut inline);
    expected.sort_by_key(|p| (p.settlement, p.structure));
    let tracks = count::<TrackSurface>(&mut inline);
    // Starting far off, then riding in: the chunks are streamed.
    let mut app = app(GroundVec2::new(-3500.0, 3500.0));
    app.update();
    app.insert_resource(StreamFocus(GroundVec2::new(100.0, 100.0)));
    app.update();
    // The update that streamed the chunks did not build their settlements.
    assert_eq!(parts(&mut app).len(), 0);
    assert!(count::<PendingSettlements>(&mut app) > 0);
    // They land once built: the same pieces, once each.
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
    // Away again the very next frame, before anything lands.
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
    // Bevy never frees a range's GPU slot: a handful, shared.
    assert!(distinct.len() <= 3, "{} distinct ranges", distinct.len());
    assert!(distinct.iter().all(|r| ranges().contains(r)));
    let mut q = app.world_mut().query_filtered::<
        &MeshMaterial3d<StandardMaterial>,
        Or<(With<SettlementPart>, With<TrackSurface>, With<YardSurface>)>,
    >();
    let materials: HashSet<AssetId<StandardMaterial>> =
        q.iter(app.world()).map(|m| m.id()).collect();
    // One per surface, and the two trees' bark and leaves.
    assert!(
        materials.len() <= Surface::ALL.len() + 4,
        "{} materials",
        materials.len()
    );
    // Thin things fade out of the camera's way; houses do not.
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
                | StructureKind::Eucalyptus
                | StructureKind::Poplar
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
        // Distance from the track's line.
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
    // Laid every couple of meters: it follows the ground's rises.
    assert!(points.len() / 42 >= 180 / 2, "{}", points.len());
}

#[test]
fn trees_grow_slender_on_their_spots() {
    let mut app = app(GroundVec2::new(100.0, 100.0));
    app.update();
    app.update();
    let kit = app.world().resource::<SettlementKit>().clone();
    let bark = [kit.trees[0][0].id(), kit.trees[1][0].id()];
    let mut q = app.world_mut().query::<(
        &SettlementPart,
        &Transform,
        &MeshMaterial3d<StandardMaterial>,
    )>();
    let trees: Vec<(StructureKind, Vec3, Vec3, bool)> = q
        .iter(app.world())
        .filter(|(p, ..)| p.kind.is_tree())
        .map(|(p, t, m)| (p.kind, t.translation, t.scale, bark.contains(&m.id())))
        .collect();
    assert!(!trees.is_empty());
    // Every species draws its trunks, not just crowns.
    for kind in [StructureKind::Poplar, StructureKind::Eucalyptus] {
        let trunks = trees.iter().filter(|t| t.0 == kind && t.3).count();
        assert!(trunks >= 1, "no {kind:?} trunk drawn");
    }
    for (kind, at, scale, trunk) in trees {
        // Trunks taller than wide: a poplar's much more so. (Crowns are
        // built to their shape and scaled evenly; see the crown tests.)
        let slender = if trunk {
            scale.y / scale.x
        } else {
            f32::INFINITY
        };
        let least = if kind == StructureKind::Poplar {
            2.5
        } else {
            1.2
        };
        assert!(slender > least, "{kind:?}: {scale:?}");
        let spot = layout()
            .settlements
            .iter()
            .flat_map(|s| s.structures.iter())
            .any(|s| {
                s.kind == kind && s.footprint.center().distance(GroundVec2::new(at.x, -at.z)) < 1e-3
            });
        assert!(spot);
    }
}

/// The extent of `mesh` along each axis.
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
fn tree_crowns_are_round_from_every_side_varied_and_shared() {
    let mut app = app(GroundVec2::new(100.0, 100.0));
    app.update();
    app.update();
    let kit = app.world().resource::<SettlementKit>().clone();
    let mut q = app.world_mut().query::<(
        &SettlementPart,
        &Mesh3d,
        &MeshMaterial3d<StandardMaterial>,
        &Transform,
    )>();
    let crowns: Vec<(StructureKind, AssetId<Mesh>, Vec3)> = q
        .iter(app.world())
        .filter(|(p, _, m, _)| {
            p.kind.is_tree() && (m.id() == kit.trees[0][1].id() || m.id() == kit.trees[1][1].id())
        })
        .map(|(p, mesh, _, t)| (p.kind, mesh.id(), t.scale))
        .collect();
    let meshes = app.world().resource::<Assets<Mesh>>();
    for kind in [StructureKind::Poplar, StructureKind::Eucalyptus] {
        let of_kind: Vec<_> = crowns.iter().filter(|c| c.0 == kind).collect();
        assert!(of_kind.len() >= 2, "{kind:?}");
        for (_, mesh, scale) in &of_kind {
            let size = extent(meshes.get(*mesh).unwrap()) * *scale;
            // Real depth along both ground axes: not a slab.
            let (wide, deep) = (size.x.max(size.z), size.x.min(size.z));
            assert!(deep >= 0.6 * wide, "{kind:?}: {size:?}");
            if kind == StructureKind::Poplar {
                // A Lombardy poplar's crown is a tall column.
                assert!(size.y >= 3.0 * wide, "{size:?}");
                assert!(wide >= 1.5, "{size:?}");
            }
            let tris = triangles(meshes.get(*mesh).unwrap());
            assert!((300..=800).contains(&tris), "{kind:?}: {tris}");
        }
        // A row is not cloned: several crowns, from a few shared meshes.
        let distinct: HashSet<AssetId<Mesh>> = of_kind.iter().map(|c| c.1).collect();
        assert!(distinct.len() >= 2, "{kind:?}: one crown for all");
        assert!(distinct.len() <= 4, "{kind:?}: {} crowns", distinct.len());
    }
}

#[test]
fn without_a_trunk_model_or_stand_in_no_crown_floats() {
    // No flora plugin: neither the vegetation models nor their stand-ins.
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
    assert!(
        drawn.iter().all(|p| !p.kind.is_tree()),
        "a tree drawn without a trunk"
    );
}

#[test]
fn a_tree_fades_over_its_whole_crown() {
    let mut app = app(GroundVec2::new(100.0, 100.0));
    app.update();
    app.update();
    let kit = app.world().resource::<SettlementKit>().clone();
    let leaves = [kit.trees[0][1].id(), kit.trees[1][1].id()];
    let mut q = app.world_mut().query::<(
        &SettlementPart,
        &Mesh3d,
        &MeshMaterial3d<StandardMaterial>,
        &Transform,
        &ThinOccluder,
    )>();
    let crowns: Vec<(StructureKind, AssetId<Mesh>, Transform, ThinOccluder)> = q
        .iter(app.world())
        .filter(|(p, _, m, ..)| p.kind.is_tree() && leaves.contains(&m.id()))
        .map(|(p, mesh, _, t, o)| (p.kind, mesh.id(), *t, *o))
        .collect();
    let meshes = app.world().resource::<Assets<Mesh>>();
    for kind in [StructureKind::Poplar, StructureKind::Eucalyptus] {
        assert!(crowns.iter().any(|c| c.0 == kind), "{kind:?}");
    }
    for (kind, mesh, transform, fade) in crowns {
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(points)) = meshes
            .get(mesh)
            .unwrap()
            .attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!()
        };
        // The crown's reach from its trunk, as drawn (turned and scaled).
        let at = transform.translation;
        let reach = points
            .iter()
            .map(|p| {
                let w = transform.transform_point(Vec3::from_array(*p)) - at;
                w.with_y(0.0).length()
            })
            .fold(0.0, f32::max);
        assert!(
            fade.radius >= reach - 1e-3,
            "{kind:?}: fades within {} m of a crown reaching {reach} m",
            fade.radius
        );
        // The widest crown variant of its species, no wider than that.
        assert!(
            fade.radius <= 0.6 * transform.scale.y,
            "{kind:?}: {}",
            fade.radius
        );
    }
}

/// An app that loads glTF models from `root`, drawing the two test
/// settlements with the given slot fits.
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

/// Updates `app` until no slot model is waiting or standing in; panics
/// after 10 s.
fn settle_models(app: &mut App) {
    for _ in 0..2000 {
        app.update();
        let waiting = count::<slots::SlotPending>(app) + count::<StandIn>(app);
        let laid = !parts(app).is_empty();
        if laid && waiting == 0 {
            // One more frame for the last scenes to spawn and be dressed.
            app.update();
            app.update();
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("timed out after 10 s waiting for the slot models to load");
}

/// The structure kinds drawn with a procedural mesh, and with a model.
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

/// Every entity under `root`, depth first.
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

/// The world-space meshes spawned under each model scene, with the range
/// and fade they were dressed with.
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
    // The well's slot holds the test triangle; the cart's a broken file;
    // the windpump's nothing at all.
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
    // Two settlements, one well each: drawn by the model, never the mesh.
    assert_eq!(
        scenes.iter().filter(|k| **k == StructureKind::Well).count(),
        2
    );
    assert!(
        !meshes.contains(&StructureKind::Well),
        "the well's mesh stayed"
    );
    // The broken cart and the missing windpump stay procedural.
    assert!(!scenes.contains(&StructureKind::Cart));
    assert!(meshes.contains(&StructureKind::Cart) && meshes.contains(&StructureKind::Windpump));
    assert_eq!(count::<StandIn>(&mut app), 0);
    // The model's triangle hangs under each well, fitted, dressed with the
    // props' shared range and the well's fade.
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
    // Still a few shared ranges in all, models included.
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
    // The procedural wheels went with their stand-ins.
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
        // About its axis only: the axis stays put.
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
    // Every folder beside the source packs is a known slot.
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
    // The cart is still procedural: the 3TD wagon is a wreck, not a carreta.
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
    // Hung lanterns hang by a barn's front wall, above a man's head.
    for spot in spots.iter().filter(|s| s.hung) {
        assert_eq!(spot.prop, YardProp::Lantern);
        assert!(spot.lift > 2.0);
    }
    // Only an estancia gets the heavy barrels.
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
    // Wait for every prop's scene to spawn.
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
    // The lantern's panes: blended, and see-through now.
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
    // Laid while the model was still loading: a waiting scene and its
    // stand-in under one chunk.
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

/// Loads `paths` (with their sRGB flags) from the shipped assets in a
/// headless app whose loader, like a desktop GPU's, takes BC formats, and
/// waits until every one is in; panics on a failed load or after 10 s.
///
/// Bevy 0.20 cannot transcode UASTC to plain RGBA8 (it slices the UASTC
/// levels by the output's block size and panics), so these tests, like the
/// game, need a GPU format: BC7, ASTC or ETC2.
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

/// How many surfaces get a colour texture when the GPU supports `formats`.
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
    // UASTC needs BC, ASTC or ETC2; without any, Bevy 0.20's loader panics,
    // so no map may even start loading.
    assert_eq!(
        textured_surfaces(bevy::image::CompressedImageFormats::NONE),
        0
    );
    assert!(textured_surfaces(bevy::image::CompressedImageFormats::BC) > 0);
}

#[test]
fn every_set_ships_its_three_ktx2_maps_as_bc7_with_baked_mips() {
    use bevy::render::render_resource::TextureFormat;
    // A set worn under several tints is loaded once.
    let sets: std::collections::BTreeSet<&str> =
        Surface::ALL.iter().filter_map(|s| s.set()).collect();
    let paths: Vec<(String, bool)> = sets
        .into_iter()
        .flat_map(|set| materials::MAPS.map(|(name, srgb)| (materials::map_path(set, name), srgb)))
        .collect();
    assert_eq!(paths.len(), 9 * 3);
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
