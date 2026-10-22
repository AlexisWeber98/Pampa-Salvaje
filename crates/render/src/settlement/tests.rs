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
