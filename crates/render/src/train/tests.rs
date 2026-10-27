use super::kit::Mat;
use super::shape::Shape;
use super::*;
use crate::facade::{Livery, Surface};
use bevy::time::TimeUpdateStrategy;
use pampa_core::CoachBody;
use pampa_core::{
    BASE_RATE, PlazaPlacement, RailLine, RailwayConfig, Side, Terrain, Timetable, TownConfig,
    TownLayout, Vec2 as GroundVec2, WorldConfig, place_towns,
};
use std::f32::consts::{PI, TAU};
use std::time::Duration;

/// Triangles of each car, as built (see the budget test).
const LOCO_TRIANGLES: usize = 14844;
const COACH_TRIANGLES: usize = 14328;
const BOXCAR_TRIANGLES: usize = 6588;
const HOPPER_TRIANGLES: usize = 6432;

/// Castex, Monte Nievas and Pico as `assets/world/towns.ron` lays them
/// out, and the line train through them.
pub(crate) fn line_train() -> (Vec<TownLayout>, LineTrain) {
    let world = WorldConfig {
        half_extent: WorldConfig::MAX_HALF_EXTENT,
        ..WorldConfig::default()
    };
    let town = |name: &str, x: f32, y: f32, blocks| TownConfig {
        name: name.into(),
        origin: GroundVec2::new(x, y),
        blocks,
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
    // Monte Nievas, and the others at their real offsets from it
    // (east, north, in meters) scaled 1:7 and rounded to the meter.
    let nievas = (-2450.0, -1950.0);
    let at = |east: f32, north: f32| {
        (
            (nievas.0 + east / 7.0).round(),
            (nievas.1 + north / 7.0).round(),
        )
    };
    let (castex, pico) = (at(-12_600.0, -5_400.0), at(35_800.0, 22_400.0));
    let (towns, errors) = place_towns(
        &world,
        &[
            town("Monte Nievas", nievas.0, nievas.1, (4, 3)),
            town("Eduardo Castex", castex.0, castex.1, (5, 4)),
            town("General Pico", pico.0, pico.1, (8, 8)),
        ],
    );
    assert!(errors.is_empty(), "{errors:?}");
    let names: Vec<String> = ["Eduardo Castex", "Monte Nievas", "General Pico"]
        .map(String::from)
        .to_vec();
    let (line, _) = RailLine::build(Some(&names), &towns, world.bounds());
    let train = LineTrain::new(&line, &towns, Timetable::default()).expect("a train");
    (towns, train)
}

fn app_with_train(game_seconds: f64, focus: GroundVec2) -> App {
    let (_, mut train) = line_train();
    train.advance(0.0, game_seconds, BASE_RATE);
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        .insert_resource(WorldTerrain(Terrain::default()))
        .insert_resource(StreamFocus(focus))
        .insert_resource(WorldTrains(vec![train]))
        .add_plugins(TrainPlugin);
    app
}

/// A second, shorter train (a shuttle's loco, coach and boxcar) over the
/// same line, leaving `down` and `up` (hours of the day), so it keeps to
/// its own timetable apart from the line train.
pub(crate) fn shuttle(line: &LineTrain, towns: &[TownLayout], down: u32, up: u32) -> LineTrain {
    let cars = [
        (CarKind::Locomotive, 14.0),
        (CarKind::Coach, 12.0),
        (CarKind::Boxcar, 10.0),
    ]
    .map(|(kind, length)| pampa_core::Car { kind, length })
    .to_vec();
    let served: Vec<usize> = line.stations().iter().map(|s| s.town).collect();
    let timetable = Timetable {
        down: vec![pampa_core::TimeOfDay::hm(down, 0)],
        up: vec![pampa_core::TimeOfDay::hm(up, 0)],
        ..Timetable::default()
    };
    LineTrain::on_path(line.path().clone(), &served, towns, cars, timetable)
        .expect("a shuttle on the line")
}

/// An app with the line train and the shuttle (leaving Castex at
/// 22:00, Pico at 23:00, so parked at Castex all day) at `game_seconds`.
fn app_with_two_trains(game_seconds: f64, focus: GroundVec2) -> App {
    let (towns, mut line) = line_train();
    let mut shuttle = shuttle(&line, &towns, 22, 23);
    line.advance(0.0, game_seconds, BASE_RATE);
    shuttle.advance(0.0, game_seconds, BASE_RATE);
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        .insert_resource(WorldTerrain(Terrain::default()))
        .insert_resource(StreamFocus(focus))
        .insert_resource(WorldTrains(vec![line, shuttle]))
        .add_plugins(TrainPlugin);
    app
}

/// Every car entity's (train, car) pair with its transform and visibility.
fn all_cars(app: &mut App) -> Vec<(usize, usize, Transform, Visibility)> {
    let mut query = app
        .world_mut()
        .query::<(&TrainCar, &Transform, &Visibility)>();
    let mut out: Vec<_> = query
        .iter(app.world())
        .map(|(c, t, v)| (c.train, c.index, *t, *v))
        .collect();
    out.sort_by_key(|c| (c.0, c.1));
    out
}

fn cars(app: &mut App) -> Vec<(usize, Transform, Visibility)> {
    let mut query = app
        .world_mut()
        .query::<(&TrainCar, &Transform, &Visibility)>();
    let mut out: Vec<_> = query
        .iter(app.world())
        .map(|(c, t, v)| (c.index, *t, *v))
        .collect();
    out.sort_by_key(|c| c.0);
    out
}

#[test]
fn cars_near_the_focus_follow_logic_along_the_curves() {
    // Two minutes out of Monte Nievas, on the way to Pico.
    let (_, probe) = line_train();
    let (_, depart) = probe.stop_times(pampa_core::Run::Down, 1).unwrap();
    let at = 7.0 * 3600.0 + (depart + 120.0) * BASE_RATE;
    let mut app = app_with_train(at, GroundVec2::ZERO);
    let placed = app.world().resource::<WorldTrains>().0[0]
        .clone()
        .placed_cars();
    app.insert_resource(StreamFocus(placed[0].center));
    app.update();
    app.update();
    let drawn = cars(&mut app);
    assert_eq!(drawn.len(), placed.len());
    assert_eq!(
        drawn.len(),
        8,
        "the locomotive, two coaches and five freight cars"
    );
    for ((_, t, visibility), p) in drawn.iter().zip(&placed) {
        assert_eq!(*visibility, Visibility::Inherited);
        let at = t.translation;
        assert!((at.x - p.center.x).abs() < 1e-3 && (at.z + p.center.y).abs() < 1e-3);
        // Local +X points along the car's ground heading.
        let forward = t.rotation * Vec3::X;
        let flat = GroundVec2::new(forward.x, -forward.z);
        let flat = flat * (1.0 / flat.length());
        assert!(
            flat.dot(p.direction) > 0.9999,
            "{flat:?} vs {:?}",
            p.direction
        );
    }
}

/// One drawn mesh of a car, in the car's frame.
struct Drawn {
    material: Handle<StandardMaterial>,
    /// Inside a coach's shell ([`CoachInterior`]).
    interior: bool,
    points: Vec<Vec3>,
    normals: Vec<Vec3>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

impl Drawn {
    fn triangles(&self) -> impl Iterator<Item = [Vec3; 3]> + '_ {
        self.indices
            .chunks(3)
            .map(|t| [t[0], t[1], t[2]].map(|i| self.points[i as usize]))
    }
}

/// Every mesh under car `index` (the smoke puffs apart), the transforms of
/// the parts between it and the car applied.
fn drawn(app: &mut App, index: usize) -> Vec<Drawn> {
    let mut roots = app.world_mut().query::<(Entity, &TrainCar)>();
    let root = roots
        .iter(app.world())
        .find(|(_, c)| c.index == index && c.train == 0)
        .unwrap()
        .0;
    let meshes = app.world().resource::<Assets<Mesh>>();
    let mut out = Vec::new();
    let mut stack = vec![(root, Transform::IDENTITY)];
    while let Some((e, above)) = stack.pop() {
        let world = app.world();
        if world.get::<SmokePuff>(e).is_some() {
            continue;
        }
        let here = if e == root {
            above
        } else {
            above.mul_transform(world.get::<Transform>(e).copied().unwrap_or_default())
        };
        if let Some(children) = world.get::<Children>(e) {
            stack.extend(children.iter().map(|c| (c, here)));
        }
        let (Some(mesh), Some(material)) = (
            world.get::<Mesh3d>(e),
            world.get::<MeshMaterial3d<StandardMaterial>>(e),
        ) else {
            continue;
        };
        let mesh = meshes.get(&mesh.0).unwrap();
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|a| a.as_float3())
            .unwrap();
        let normals = mesh
            .attribute(Mesh::ATTRIBUTE_NORMAL)
            .and_then(|a| a.as_float3())
            .unwrap();
        let uvs = match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
            Some(bevy::mesh::VertexAttributeValues::Float32x2(uv)) => uv.clone(),
            _ => panic!("every part has UVs"),
        };
        out.push(Drawn {
            material: material.0.clone(),
            interior: world.get::<CoachInterior>(e).is_some(),
            points: positions
                .iter()
                .map(|p| here.transform_point(Vec3::from(*p)))
                .collect(),
            normals: normals
                .iter()
                .map(|n| here.rotation * Vec3::from(*n))
                .collect(),
            uvs,
            indices: mesh.indices().unwrap().iter().map(|i| i as u32).collect(),
        });
    }
    out
}

/// The kit's handle of `mat`, of the app that spawned a train.
fn material(app: &App, mat: Mat) -> Handle<StandardMaterial> {
    app.world().resource::<CarKit>().0.material(mat)
}

/// Where the ray from `from` along `dir` (unit) meets any triangle of
/// `parts`, as the materials of what it meets.
fn ray_hits(parts: &[Drawn], from: Vec3, dir: Vec3, reach: f32) -> Vec<Handle<StandardMaterial>> {
    let mut hits = Vec::new();
    for part in parts {
        for [a, b, c] in part.triangles() {
            // Moeller-Trumbore, both faces.
            let (e1, e2) = (b - a, c - a);
            let p = dir.cross(e2);
            let det = e1.dot(p);
            if det.abs() < 1e-9 {
                continue;
            }
            let t = from - a;
            let u = t.dot(p) / det;
            let q = t.cross(e1);
            let v = dir.dot(q) / det;
            let at = e2.dot(q) / det;
            if u >= 0.0 && v >= 0.0 && u + v <= 1.0 && at > 1e-4 && at < reach {
                hits.push(part.material.clone());
            }
        }
    }
    hits
}

fn app_with_cars() -> App {
    let mut app = app_with_train(3600.0, GroundVec2::ZERO);
    app.update();
    app
}

fn first(app: &App, kind: CarKind) -> (usize, f32) {
    let train = &app.world().resource::<WorldTrains>().0[0];
    let index = train.cars().iter().position(|c| c.kind == kind).unwrap();
    (index, train.cars()[index].length)
}

#[test]
fn each_car_is_drawn_as_tall_as_core_says() {
    let mut app = app_with_cars();
    let train = app.world().resource::<WorldTrains>().0[0].clone();
    for (index, car) in train.cars().iter().enumerate() {
        let top = drawn(&mut app, index)
            .iter()
            .flat_map(|d| d.points.iter().map(|p| p.y))
            .fold(f32::MIN, f32::max);
        assert!(
            (top - car.kind.height()).abs() < 0.02,
            "{:?}: drawn {top}, core {}",
            car.kind,
            car.kind.height()
        );
    }
}

#[test]
fn coaches_are_hollow_with_glazed_windows_and_benches_inside() {
    let mut app = app_with_cars();
    let (index, length) = first(&app, CarKind::Coach);
    let parts = drawn(&mut app, index);
    let glass = material(&app, Mat::Glass);
    let (windows, open) = CoachBody::windows(length);
    assert_eq!(windows.len(), 5);
    let mid_y = (CoachBody::SILL + CoachBody::LINTEL) / 2.0;
    for &x in &windows {
        for side in [-1.0f32, 1.0] {
            // From the aisle straight out through the window: only the
            // clear pane is in the way (it has two faces).
            let hits = ray_hits(
                &parts,
                Vec3::new(x, mid_y, 0.0),
                Vec3::new(0.0, 0.0, side),
                3.0,
            );
            assert!(!hits.is_empty(), "no pane at {x}, side {side}");
            assert!(
                hits.iter().all(|m| *m == glass),
                "window at {x}, side {side}"
            );
        }
    }
    // The wall beside a window, below the sill, is boarded, not open.
    let hits = ray_hits(
        &parts,
        Vec3::new(windows[0], CoachBody::SILL - 0.3, 0.0),
        Vec3::Z,
        3.0,
    );
    assert!(hits.contains(&material(&app, Mat::MAROON)));
    // Benches: leather cushions, two seats and two backs per bench, four
    // benches to a bay.
    let leather = material(&app, Mat::Leather);
    let cushions = parts
        .iter()
        .filter(|d| d.material == leather)
        .map(|d| d.triangles().count())
        .sum::<usize>();
    assert!(cushions > 0);
    assert_eq!(cushions, 4 * windows.len() * bench_cushion_triangles());
    // From a window seat the way out of the window is clear, glass only.
    for side in [-1.0, 1.0] {
        let (along, across, eye) = CoachBody::window_seat(length, side);
        assert!(windows.iter().any(|&x| (along - x).abs() < open / 2.0));
        assert!(eye > CoachBody::SILL && eye < CoachBody::LINTEL);
        let hits = ray_hits(
            &parts,
            Vec3::new(along, eye, across),
            Vec3::new(0.0, 0.0, side),
            3.0,
        );
        assert!(
            !hits.is_empty() && hits.iter().all(|m| *m == glass),
            "{side}"
        );
        // And nothing hangs within arm's reach of the seated eye.
        let eye = Vec3::new(along, eye, across);
        for part in parts.iter().filter(|d| d.interior) {
            assert!(part.points.iter().all(|p| p.distance(eye) > 0.15));
        }
    }
}

/// The triangles of one seat cushion or back cushion, as built.
fn bench_cushion_triangles() -> usize {
    let (seat, back, ..) = super::coach::bench_shapes();
    seat.triangles() + back.triangles()
}

#[test]
fn the_ridden_coachs_inside_keeps_within_the_coach_body() {
    let mut app = app_with_cars();
    let (index, length) = first(&app, CarKind::Coach);
    let parts = drawn(&mut app, index);
    let inner = length - 2.0 * CoachBody::END_INSET;
    let reach = CoachBody::HALF_WIDTH - CoachBody::WALL;
    let mut materials = std::collections::HashSet::new();
    let mut count = 0;
    for part in parts.iter().filter(|d| d.interior) {
        materials.insert(part.material.id());
        for p in &part.points {
            count += 1;
            assert!(p.x.abs() <= inner / 2.0 + 1e-3, "{p}");
            assert!(p.z.abs() <= reach + 1e-3, "{p}");
            // Floor slab under the floor, ceiling slab over the ceiling.
            assert!(p.y >= CoachBody::FLOOR - 0.1 - 1e-3, "{p}");
            assert!(p.y <= CoachBody::CEILING + 0.08 + 1e-3, "{p}");
        }
    }
    assert!(count > 500);
    // Panelling and floor, ceiling and cream beading, brass racks and
    // lamps, glowing globes, leather seats, the runner.
    for mat in [
        Mat::VARNISH,
        Mat::Cream,
        Mat::BRASS,
        Mat::Lamp,
        Mat::Leather,
        Mat::Runner,
    ] {
        assert!(materials.contains(&material(&app, mat).id()), "{mat:?}");
    }
    // The shell is not inside: the outer skin reaches the car's full width.
    let skin = material(&app, Mat::MAROON);
    let widest = parts
        .iter()
        .filter(|d| d.material == skin)
        .flat_map(|d| d.points.iter().map(|p| p.z.abs()))
        .fold(0.0, f32::max);
    assert!((widest - CoachBody::HALF_WIDTH).abs() < 0.02, "{widest}");
    // The floor top is CoachBody::FLOOR and the ceiling's underside its
    // CEILING.
    let varnish = material(&app, Mat::VARNISH);
    let (mut floor, mut ceiling) = (f32::MIN, f32::MAX);
    for part in parts.iter().filter(|d| d.interior && d.material == varnish) {
        for (p, n) in part.points.iter().zip(&part.normals) {
            if n.y > 0.99 && p.y < 1.4 {
                floor = floor.max(p.y);
            }
            if n.y < -0.99 && p.y > 3.5 {
                ceiling = ceiling.min(p.y);
            }
        }
    }
    assert!((floor - 1.3).abs() < 1e-3, "floor {floor}");
    assert!((ceiling - 3.8).abs() < 1e-3, "ceiling {ceiling}");
}

#[test]
fn wheels_stand_on_the_rails_a_gauge_apart() {
    let mut app = app_with_cars();
    let train = app.world().resource::<WorldTrains>().0[0].clone();
    let mut q = app
        .world_mut()
        .query::<(&TrainWheel, &Transform, &Mesh3d, &ChildOf)>();
    let mut checked = 0;
    for (wheel, t, mesh, parent) in q.iter(app.world()) {
        let car = app.world().get::<TrainCar>(parent.parent()).unwrap();
        if car.train != 0 {
            continue;
        }
        let mesh = app.world().resource::<Assets<Mesh>>().get(&mesh.0).unwrap();
        let low = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|a| a.as_float3())
            .unwrap()
            .iter()
            .map(|p| t.transform_point(Vec3::from(*p)).y)
            .fold(f32::MAX, f32::min);
        // The flange reaches 3 cm under the rail top; the tread is on it.
        assert!((low + 0.03).abs() < 2e-3, "{:?}: lowest {low}", car.index);
        assert!((t.translation.y - wheel.radius).abs() < 1e-5);
        // 0.86 m either side of the middle: the rails' heads at 0.838.
        assert!((t.translation.z.abs() - 0.86).abs() < 1e-5);
        checked += 1;
    }
    // Per car: a locomotive and tender's 12, coaches 8 each, freight 8.
    let expected: usize = train
        .cars()
        .iter()
        .map(|c| if c.kind == CarKind::Locomotive { 14 } else { 8 })
        .sum();
    assert_eq!(checked, expected);
}

#[test]
fn materials_go_where_the_period_train_wears_them() {
    let mut app = app_with_cars();
    let on = |app: &mut App, kind: CarKind| {
        let (index, _) = first(app, kind);
        let parts = drawn(app, index);
        let kit = app.world().resource::<CarKit>().0.clone();
        move |mat: Mat| parts.iter().any(|d| d.material == kit.material(mat))
    };
    let loco = on(&mut app, CarKind::Locomotive);
    for mat in [
        Mat::BLACK,
        Mat::TREAD,
        Mat::BRASS,
        Mat::BUFFER_RED,
        Mat::Coal,
        Mat::Lamp,
    ] {
        assert!(loco(mat), "locomotive {mat:?}");
    }
    let coach = on(&mut app, CarKind::Coach);
    for mat in [
        Mat::MAROON,
        Mat::VARNISH,
        Mat::Glass,
        Mat::Canvas,
        Mat::BRASS,
        Mat::TREAD,
    ] {
        assert!(coach(mat), "coach {mat:?}");
    }
    let boxcar = on(&mut app, CarKind::Boxcar);
    for mat in [Mat::OXIDE, Mat::OLD_IRON, Mat::BLACK] {
        assert!(boxcar(mat), "boxcar {mat:?}");
    }
    let hopper = on(&mut app, CarKind::Hopper);
    assert!(hopper(Mat::STEEL) && hopper(Mat::BLACK));
    // The painted steel is the towns' own surface, wearing a scan.
    assert_eq!(Mat::BLACK, Mat::Surface(Surface::Livery(Livery::Black)));
}

#[test]
fn textures_tile_in_meters_whatever_the_part() {
    let mut app = app_with_cars();
    let (index, length) = first(&app, CarKind::Coach);
    let parts = drawn(&mut app, index);
    let maroon = material(&app, Mat::MAROON);
    let varnish = material(&app, Mat::VARNISH);
    let mut sides = 0;
    for part in &parts {
        for (i, ((p, n), uv)) in part
            .points
            .iter()
            .zip(&part.normals)
            .zip(&part.uvs)
            .enumerate()
        {
            let _ = i;
            if part.material == maroon && n.z.abs() > 0.99 {
                // A side wall: (u, v) = (x, y) in meters.
                assert!((uv[0] - p.x).abs() < 1e-4 && (uv[1] - p.y).abs() < 1e-4);
                sides += 1;
            }
            if part.interior && part.material == varnish && n.y > 0.99 && (p.y - 1.3).abs() < 1e-3 {
                // The floor: (x, z).
                assert!((uv[0] - p.x).abs() < 1e-4 && (uv[1] - p.z).abs() < 1e-4);
            }
        }
    }
    assert!(sides > 100);
    // The boxcar's boards run upright: (u, v) = (y, x).
    let (index, _) = first(&app, CarKind::Boxcar);
    let oxide = material(&app, Mat::OXIDE);
    let mut upright = 0;
    for part in drawn(&mut app, index)
        .iter()
        .filter(|d| d.material == oxide)
    {
        for ((p, n), uv) in part.points.iter().zip(&part.normals).zip(&part.uvs) {
            if n.z.abs() > 0.99 {
                assert!((uv[0] - p.y).abs() < 1e-4 && (uv[1] - p.x).abs() < 1e-4);
                upright += 1;
            }
        }
    }
    assert!(upright > 0);
    // A lathe turning's `u` runs round it in meters: 0.2 m radius, 1.26 m.
    let rim = Shape::lathe(&[(0.0, 0.2), (1.0, 0.2)], 16);
    let u = rim.uvs.iter().map(|uv| uv[0]).fold(f32::MIN, f32::max);
    assert!((u - std::f32::consts::TAU * 0.2).abs() < 1e-4);
    let _ = length;
}

/// Triangles drawn for car `index`, over every mesh under it.
fn triangles(app: &mut App, index: usize) -> usize {
    let mut q = app.world_mut().query::<(Entity, &TrainCar)>();
    let root = q
        .iter(app.world())
        .find(|(_, c)| c.index == index && c.train == 0)
        .unwrap()
        .0;
    let meshes = app.world().resource::<Assets<Mesh>>();
    let mut stack = vec![root];
    let mut total = 0;
    while let Some(e) = stack.pop() {
        if let Some(children) = app.world().get::<Children>(e) {
            stack.extend(children.iter());
        }
        if let Some(mesh) = app.world().get::<Mesh3d>(e) {
            let mesh = meshes.get(&mesh.0).unwrap();
            total += mesh.indices().map_or(mesh.count_vertices(), |i| i.len()) / 3;
        }
    }
    total
}

#[test]
fn every_car_stays_within_its_triangle_budget() {
    let mut app = app_with_cars();
    let train = app.world().resource::<WorldTrains>().0[0].clone();
    for (index, car) in train.cars().iter().enumerate() {
        let tris = triangles(&mut app, index);
        println!("{:?}: {tris} triangles", car.kind);
        // The counts of the rounded, spoked, textured train (T5, 2026-10-27;
        // the boxy train's were 2156, 2348, 1124 and 1100): a locomotive and
        // tender of lathe turnings and 20 spoked wheels, a coach with its
        // furnished inside and eight plate wheels, freight cars of framed
        // boards. A quarter's slack either way.
        let documented = match car.kind {
            CarKind::Locomotive => LOCO_TRIANGLES,
            CarKind::Coach => COACH_TRIANGLES,
            CarKind::Boxcar => BOXCAR_TRIANGLES,
            CarKind::Hopper => HOPPER_TRIANGLES,
        } as f32;
        let range = (documented * 0.75) as usize..=(documented * 1.25) as usize;
        assert!(
            range.contains(&tris),
            "{:?}: {tris} not in {range:?}",
            car.kind
        );
    }
}

#[test]
fn the_rods_swing_round_with_the_cranks() {
    let rod = TrainRod {
        train: 0,
        base: Vec3::new(2.9, 0.75, 1.12),
        crank: 0.3,
        radius: 0.75,
        phase: 0.0,
        length: 0.0,
    };
    let at = |phase: f32| rod_pose(&TrainRod { phase, ..rod }, Vec3::ONE).translation;
    assert!((at(0.0) - rod.base - Vec3::new(0.3, 0.0, 0.0)).length() < 1e-5);
    assert!(
        (at(std::f32::consts::FRAC_PI_2) - rod.base - Vec3::new(0.0, 0.3, 0.0)).length() < 1e-5
    );
    // A side rod stays level; a main rod tilts with the crank.
    assert_eq!(
        rod_pose(&TrainRod { phase: 1.0, ..rod }, Vec3::ONE).rotation,
        Quat::IDENTITY
    );
    let main = TrainRod {
        length: 3.0,
        phase: 1.0,
        ..rod
    };
    assert!(
        rod_pose(&main, Vec3::ONE)
            .rotation
            .angle_between(Quat::IDENTITY)
            > 0.01
    );
    // In the app, the rods turn with the drivers as the train runs: by
    // the distance rolled over the drivers' radius, in their sense.
    let mut app = running_app();
    let speed = app
        .world()
        .resource::<WorldTrains>()
        .line()
        .unwrap()
        .state()
        .speed;
    assert!(speed > 1.0, "the train is under way: {speed}");
    app.update();
    let rods = |app: &mut App| {
        let mut q = app.world_mut().query::<&TrainRod>();
        q.iter(app.world()).copied().collect::<Vec<_>>()
    };
    // A driver's crank angle in the car's side plane: where its local
    // +X (a point on the rim) has turned to about the car's Z axis.
    let drivers = |app: &mut App| {
        let mut q = app.world_mut().query::<(&TrainWheel, &Transform)>();
        q.iter(app.world())
            .filter(|(w, _)| w.radius == DRIVER_RADIUS)
            .map(|(_, t)| {
                let rim = t.rotation * Vec3::X;
                rim.y.atan2(rim.x)
            })
            .collect::<Vec<_>>()
    };
    let (rods_before, drivers_before) = (rods(&mut app), drivers(&mut app));
    assert_eq!(rods_before.len(), 4, "two side rods and two main rods");
    assert_eq!(drivers_before.len(), 6, "three coupled drivers a side");
    app.update();
    let turned = speed * app.world().resource::<Time>().delta_secs() / DRIVER_RADIUS;
    assert!(turned > 0.05, "a measurable turn: {turned}");
    let wrap = |a: f32| (a + PI).rem_euclid(TAU) - PI;
    for (before, after) in drivers_before.iter().zip(drivers(&mut app)) {
        // Rolling toward +X, a driver's front goes down: clockwise.
        assert!(
            (wrap(after - before) + turned).abs() < 1e-3,
            "driver {before} -> {after}, turned {turned}"
        );
    }
    for (before, after) in rods_before.iter().zip(rods(&mut app)) {
        assert_eq!(before.radius, DRIVER_RADIUS);
        assert!(
            (wrap(after.phase - before.phase) + turned).abs() < 1e-3,
            "rod turned {}, drivers {}",
            after.phase - before.phase,
            -turned
        );
    }
}

#[test]
fn smoke_puffs_from_the_chimney_only_while_the_train_moves() {
    let puffs = |app: &mut App| {
        let mut q = app
            .world_mut()
            .query::<(&SmokePuff, &Transform, &Visibility)>();
        q.iter(app.world())
            .map(|(p, t, v)| (*p, *t, *v))
            .collect::<Vec<_>>()
    };
    // Parked overnight: no smoke.
    let mut app = app_with_train(3600.0, GroundVec2::ZERO);
    app.update();
    app.update();
    let parked = puffs(&mut app);
    assert_eq!(parked.len(), 4);
    assert!(parked.iter().all(|p| p.2 == Visibility::Hidden));
    // Every puff leaves from the top of the chimney's cap.
    let chimney = parked[0].0.chimney;
    assert!(parked.iter().all(|p| p.0.chimney == chimney));
    assert_eq!((chimney.x, chimney.y, chimney.z), (5.7, 4.55, 0.0));
    // The chimney's brass cap ends exactly there: its highest point over
    // the chimney, and nothing of the engine is higher.
    let parts = drawn(&mut app, 0);
    let over: Vec<f32> = parts
        .iter()
        .flat_map(|d| d.points.iter())
        .filter(|p| (p.x - chimney.x).hypot(p.z - chimney.z) < 0.35)
        .map(|p| p.y)
        .collect();
    let top = over.iter().copied().fold(f32::MIN, f32::max);
    assert!((top - chimney.y).abs() < 1e-4, "{top}");
    // Under way: shown, rising from the chimney, cycling round.
    let mut app = running_app();
    app.update();
    let mut wrapped = false;
    let mut last = puffs(&mut app);
    for _ in 0..40 {
        app.update();
        let now = puffs(&mut app);
        for ((before, _, _), (puff, t, v)) in last.iter().zip(&now) {
            assert_eq!(*v, Visibility::Inherited);
            assert!((0.0..1.0).contains(&puff.phase));
            let rise = t.translation.y - puff.chimney.y;
            assert!((rise - (0.3 + puff.phase * 2.0)).abs() < 1e-4);
            assert!(t.translation.x <= puff.chimney.x);
            wrapped |= puff.phase < before.phase;
        }
        last = now;
    }
    assert!(wrapped, "puffs start again from the chimney");
}

/// The train under way out of Monte Nievas toward Castex, near the
/// focus, with each frame a fixed twentieth of a second.
fn running_app() -> App {
    let (_, probe) = line_train();
    let (_, depart) = probe.stop_times(pampa_core::Run::Up, 1).unwrap();
    let mut app = app_with_train(7.0 * 3600.0 + (depart + 30.0) * BASE_RATE, GroundVec2::ZERO);
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        50,
    )));
    app
}

#[test]
fn cars_far_from_the_focus_are_hidden() {
    // Overnight at Pico, with the focus at Castex.
    let mut app = app_with_train(3600.0, GroundVec2::new(-4250.0, -2721.0));
    app.update();
    app.update();
    let drawn = cars(&mut app);
    assert_eq!(drawn.len(), 8);
    assert!(drawn.iter().all(|c| c.2 == Visibility::Hidden));
    // No train, no cars.
    let mut empty = App::new();
    empty
        .add_plugins(MinimalPlugins)
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        .insert_resource(WorldTerrain(Terrain::default()))
        .add_plugins(TrainPlugin);
    empty.update();
    assert!(cars(&mut empty).is_empty());
}

#[test]
fn every_train_gets_its_own_cars() {
    let mut app = app_with_two_trains(3600.0, GroundVec2::ZERO);
    app.update();
    let drawn = all_cars(&mut app);
    let of = |train: usize| drawn.iter().filter(|c| c.0 == train).count();
    assert_eq!((of(0), of(1)), (8, 3), "the line train and the shuttle");
    assert_eq!(drawn.len(), 11);
    // The shuttle's cars are its own consist, locomotive first.
    let trains = app.world().resource::<WorldTrains>().clone();
    assert_eq!(trains.0[1].cars().len(), 3);
    let mut roots = app.world_mut().query::<(Entity, &TrainCar)>();
    let loco = roots
        .iter(app.world())
        .find(|(_, c)| (c.train, c.index) == (1, 0))
        .unwrap()
        .0;
    let wheels = app.world().get::<Children>(loco).unwrap();
    assert!(
        wheels
            .iter()
            .any(|e| app.world().get::<TrainRod>(e).is_some_and(|r| r.train == 1)),
        "the shuttle's locomotive has its rods"
    );
    // Parts of the second train carry its index.
    let mut parts = app.world_mut().query::<&TrainWheel>();
    let mut mine = |t: usize| parts.iter(app.world()).filter(|w| w.train == t).count();
    let (line, shuttle) = (mine(0), mine(1));
    assert!(shuttle > 0 && shuttle < line, "{shuttle} wheels of {line}");
}

#[test]
fn each_car_follows_its_own_train() {
    // Overnight: the line train at Pico, the shuttle at Castex; the
    // focus at Castex, so only the shuttle's cars are near.
    let mut app = app_with_two_trains(3600.0, GroundVec2::ZERO);
    let castex = app.world().resource::<WorldTrains>().0[1].placed_cars()[1].center;
    app.insert_resource(StreamFocus(castex));
    app.update();
    app.update();
    let trains = app.world().resource::<WorldTrains>().clone();
    let drawn = all_cars(&mut app);
    for (train, index, t, visibility) in &drawn {
        let p = trains.0[*train].placed_cars()[*index];
        if *train == 1 {
            assert_eq!(*visibility, Visibility::Inherited, "shuttle car {index}");
            let at = t.translation;
            assert!((at.x - p.center.x).abs() < 1e-3 && (at.z + p.center.y).abs() < 1e-3);
        } else {
            assert_eq!(*visibility, Visibility::Hidden, "line car {index} far away");
        }
    }
    // And the other way round: at Pico only the line train is near.
    let pico = trains.0[0].placed_cars()[0].center;
    app.insert_resource(StreamFocus(pico));
    app.update();
    for (train, index, _, visibility) in all_cars(&mut app) {
        let near = train == 0;
        assert_eq!(
            visibility == Visibility::Inherited,
            near,
            "train {train} car {index}"
        );
    }
}

#[test]
fn each_train_rolls_its_own_wheels_and_smokes_for_itself() {
    // The line train runs out of Monte Nievas; the shuttle, leaving at
    // 22:00, is parked at Castex.
    let (_, probe) = line_train();
    let (_, depart) = probe.stop_times(pampa_core::Run::Up, 1).unwrap();
    let at = 7.0 * 3600.0 + (depart + 30.0) * BASE_RATE;
    let mut app = app_with_two_trains(at, GroundVec2::ZERO);
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        50,
    )));
    let speeds: Vec<f32> = app
        .world()
        .resource::<WorldTrains>()
        .0
        .iter()
        .map(|t| t.state().speed)
        .collect();
    assert!(speeds[0] > 1.0 && speeds[1] == 0.0, "{speeds:?}");
    app.update();
    let spin = |app: &mut App| {
        let mut q = app.world_mut().query::<(&TrainWheel, &Transform)>();
        q.iter(app.world())
            .map(|(w, t)| (w.train, t.rotation))
            .collect::<Vec<_>>()
    };
    let before = spin(&mut app);
    let puffs = |app: &mut App, train: usize| {
        let mut q = app.world_mut().query::<(&SmokePuff, &Visibility)>();
        q.iter(app.world())
            .filter(|(p, _)| p.train == train)
            .map(|(_, v)| *v)
            .collect::<Vec<_>>()
    };
    app.update();
    let after = spin(&mut app);
    for ((train, a), (_, b)) in before.iter().zip(&after) {
        assert_eq!(a != b, *train == 0, "train {train}'s wheels");
    }
    assert!(after.iter().any(|w| w.0 == 1), "the shuttle has wheels");
    // The rods swing with their own train's drivers only.
    let phases = |app: &mut App| {
        let mut q = app.world_mut().query::<&TrainRod>();
        q.iter(app.world())
            .map(|r| (r.train, r.phase))
            .collect::<Vec<_>>()
    };
    let rods = phases(&mut app);
    app.update();
    let swung = phases(&mut app);
    assert!(rods.iter().any(|r| r.0 == 1), "the shuttle has rods");
    for ((train, a), (_, b)) in rods.iter().zip(&swung) {
        assert_eq!(a != b, *train == 0, "train {train}'s rods");
    }
    // Smoke: the line train's puffs show, the parked shuttle's do not.
    let (line, shuttle) = (puffs(&mut app, 0), puffs(&mut app, 1));
    assert_eq!((line.len(), shuttle.len()), (4, 4));
    assert!(line.iter().all(|v| *v == Visibility::Inherited));
    assert!(shuttle.iter().all(|v| *v == Visibility::Hidden));
}
