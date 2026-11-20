use super::*;
use crate::character::CharacterVisualPlugin;
use bevy::time::TimeUpdateStrategy;
use pampa_core::AnimationState;
use std::f32::consts::FRAC_PI_2;
use std::time::Duration;

/// Los plugins de personajes y de vehículos, headless, con almacenes de assets
/// sustitutos.
fn plugin_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            50,
        )))
        .init_resource::<Assets<Image>>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        .add_plugins((CharacterVisualPlugin, VehiclePlugin));
    app.update();
    app
}

fn count<F: bevy::ecs::query::QueryFilter>(app: &mut App) -> usize {
    app.world_mut()
        .query_filtered::<(), F>()
        .iter(app.world())
        .count()
}

fn horses(app: &mut App) -> Vec<CharacterVisual> {
    let mut q = app
        .world_mut()
        .query_filtered::<&CharacterVisual, With<VehicleHorse>>();
    q.iter(app.world()).copied().collect()
}

#[test]
fn a_cart_has_two_horses_a_sulky_one_a_carreta_two_oxen_and_the_ford_none() {
    let mut app = plugin_app();
    let cart = VehicleVisual::seeded(Vehicle::Cart, 3, 77);
    let me = app.world_mut().spawn(cart).id();
    app.update();
    assert_eq!(count::<With<VehicleBody>>(&mut app), 1);
    assert_eq!(count::<With<VehicleModel>>(&mut app), 1);
    assert_eq!(count::<With<VehicleSeat>>(&mut app), 1);
    let drawn = horses(&mut app);
    assert_eq!(drawn.len(), 2);
    for coat in cart.coats {
        assert!(drawn.contains(&CharacterVisual::Draught(coat)));
    }
    // Otro visual reemplaza al anterior, sin que quede nada de él.
    let sulky = VehicleVisual::seeded(Vehicle::Sulky, 3, 77);
    app.world_mut().entity_mut(me).insert(sulky);
    app.update();
    assert_eq!(count::<With<VehicleBody>>(&mut app), 1);
    assert_eq!(horses(&mut app), [CharacterVisual::Draught(sulky.coats[0])]);
    app.world_mut()
        .entity_mut(me)
        .insert(VehicleVisual::seeded(Vehicle::Carreta, 3, 77));
    app.update();
    assert!(horses(&mut app).is_empty());
    assert_eq!(count::<With<VehicleOx>>(&mut app), 2);
    app.world_mut()
        .entity_mut(me)
        .insert(VehicleVisual::seeded(Vehicle::FordT, 3, 77));
    app.update();
    assert!(horses(&mut app).is_empty());
    assert_eq!(count::<With<VehicleOx>>(&mut app), 0);
    assert_eq!(count::<With<VehicleSeat>>(&mut app), 1);
}

#[test]
fn a_wheel_turns_its_rolled_distance_over_its_radius_top_forward() {
    // Un cuarto de vuelta: la parte de arriba (+Y) va al frente (+Z del glTF).
    let r = 0.7;
    let top = wheel_turn(std::f32::consts::FRAC_PI_2 * r, r) * Vec3::Y;
    assert!(top.abs_diff_eq(Vec3::Z, 1e-5), "{top}");
    assert!(
        wheel_turn(2.0 * std::f32::consts::PI * r, r).abs_diff_eq(Quat::IDENTITY, 1e-4)
            || wheel_turn(2.0 * std::f32::consts::PI * r, r).abs_diff_eq(-Quat::IDENTITY, 1e-4)
    );
}

#[test]
fn the_body_turns_to_the_heading_and_the_horses_walk_with_the_pose() {
    let mut app = plugin_app();
    let pose = VisualPose {
        heading: Heading::from_radians(FRAC_PI_2),
        animation: AnimationState::Walk,
        speed: 1.6,
        turn: 0.0,
        flight: None,
    };
    let me = app
        .world_mut()
        .spawn((
            VehicleVisual::seeded(Vehicle::Cart, 1, 5),
            pose,
            Rolled(0.0),
        ))
        .id();
    app.update();
    app.world_mut().entity_mut(me).insert(Rolled(1.1));
    app.update();
    let mut bodies = app
        .world_mut()
        .query_filtered::<&Transform, With<VehicleBody>>();
    let body = *bodies.single(app.world()).unwrap();
    assert!(
        body.rotation.abs_diff_eq(body_rotation(pose.heading), 1e-5),
        "{:?}",
        body.rotation
    );
    let mut horses = app
        .world_mut()
        .query_filtered::<&VisualPose, With<VehicleHorse>>();
    for horse in horses.iter(app.world()) {
        assert_eq!(horse.animation, AnimationState::Walk);
        assert_eq!(horse.speed, 1.6);
        assert_eq!(horse.heading, Heading::default(), "turned by the body");
    }
}

#[test]
fn a_townsperson_sits_on_the_seat_once_posed() {
    use crate::character::rider::{PELVIS_RISE, Seated};
    use crate::character::{Rig, model::CharacterModels};
    use crate::gltf::tests::{root, unfinished_app};
    let mut app = unfinished_app(&root());
    app.init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>()
        .add_plugins(bevy::animation::AnimationPlugin)
        .insert_resource(root())
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            50,
        )))
        .add_plugins((
            crate::fauna::FaunaPlugin,
            CharacterVisualPlugin,
            VehiclePlugin,
        ));
    app.finish();
    app.cleanup();
    // Una cámara mirando hacia otro lado: el conductor, oculto hasta su
    // primera pose, no se congela por estar fuera de la vista.
    app.world_mut().spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 2.0, 30.0).looking_at(Vec3::new(0.0, 2.0, 60.0), Vec3::Y),
    ));
    app.update();
    assert!(app.world().get_resource::<CharacterModels>().is_some());
    let sulky = VehicleVisual::seeded(Vehicle::Sulky, 4, 9);
    app.world_mut().spawn(sulky);
    // El Ford T que espera solo no lleva a nadie.
    app.world_mut()
        .spawn(VehicleVisual::seeded(Vehicle::FordT, 4, 9).driverless());
    let mut posed = None;
    for _ in 0..3000 {
        app.update();
        let world = app.world_mut();
        posed = world
            .query::<(Entity, &Rig, &Seated, &Visibility)>()
            .iter(world)
            .find(|(_, r, _, v)| r.started && **v == Visibility::Inherited)
            .map(|(e, ..)| e);
        if posed.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let model = posed.expect("the driver never sat");
    app.update();
    let world = app.world_mut();
    let riding = *world.get::<Riding>(model).unwrap();
    assert_eq!(Some(riding.kind), sulky.driver);
    assert!(world.get::<VehicleSeat>(riding.horse).is_some());
    let seat = world
        .get::<GlobalTransform>(riding.horse)
        .unwrap()
        .translation();
    let pelvis = crate::character::horse::tests::named(world, model, "pelvis");
    let at = world.get::<GlobalTransform>(pelvis).unwrap().translation();
    assert!(
        at.distance(seat + Vec3::Y * PELVIS_RISE) < 0.02,
        "pelvis at {at} over the seat at {seat}"
    );
    let drivers = world.query::<&Riding>().iter(world).count();
    assert_eq!(drivers, 1, "only the sulky has a driver");
}

#[test]
fn a_vehicle_model_spins_its_wheels_with_the_distance_rolled() {
    use crate::gltf::tests::{root, unfinished_app};
    let mut app = unfinished_app(&root());
    app.init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>()
        .add_plugins(bevy::animation::AnimationPlugin)
        .insert_resource(root())
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            50,
        )))
        .add_plugins((
            crate::fauna::FaunaPlugin,
            CharacterVisualPlugin,
            VehiclePlugin,
        ));
    app.finish();
    app.cleanup();
    app.update();
    let me = app
        .world_mut()
        .spawn((VehicleVisual::seeded(Vehicle::Cart, 1, 5), Rolled(0.0)))
        .id();
    let mut found = 0;
    for _ in 0..3000 {
        app.update();
        found = count::<With<VehicleWheel>>(&mut app);
        if found > 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(found, 4, "the carretón's four wheels");
    app.world_mut().entity_mut(me).insert(Rolled(1.1));
    app.update();
    let mut wheels = app.world_mut().query::<(&VehicleWheel, &Transform)>();
    let radii: Vec<f32> = wheels.iter(app.world()).map(|(w, _)| w.radius).collect();
    assert!(radii.contains(&0.5) && radii.contains(&0.66), "{radii:?}");
    for (wheel, t) in wheels.iter(app.world()) {
        assert!(
            t.rotation.abs_diff_eq(wheel_turn(1.1, wheel.radius), 1e-5),
            "{:?}",
            t.rotation
        );
    }
}

#[test]
fn a_draught_horse_wears_its_harness_on_its_own_joints_through_its_clips() {
    use crate::character::bones::HorseJoint;
    use crate::character::cpu;
    use crate::character::horse::tests::try_named;
    use crate::gltf::tests::{root, unfinished_app};
    use bevy::mesh::skinning::SkinnedMesh;
    let mut app = unfinished_app(&root());
    app.init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>()
        .add_plugins(bevy::animation::AnimationPlugin)
        .insert_resource(root())
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            20,
        )))
        .add_plugins((
            crate::fauna::FaunaPlugin,
            CharacterVisualPlugin,
            VehiclePlugin,
        ));
    app.finish();
    app.cleanup();
    app.update();
    let me = app
        .world_mut()
        .spawn((
            VehicleVisual::seeded(Vehicle::Sulky, 2, 7).driverless(),
            Rolled(0.0),
        ))
        .id();
    let harness = |app: &mut App| {
        let world = app.world_mut();
        world
            .query::<(Entity, &Name, &SkinnedMesh)>()
            .iter(world)
            .filter(|(_, n, _)| n.as_str().starts_with("Harness "))
            .map(|(e, n, s)| (e, n.to_string(), s.joints.clone()))
            .collect::<Vec<_>>()
    };
    let mut pieces = Vec::new();
    for _ in 0..3000 {
        app.update();
        pieces = harness(&mut app);
        if !pieces.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let names: Vec<&str> = pieces.iter().map(|(_, n, _)| n.as_str()).collect();
    for piece in [
        "Collar",
        "Saddle",
        "Bands",
        "BackStrap",
        "Crupper",
        "Breeching",
        "Bridle",
        "Blinkers",
        "Bit",
    ] {
        assert!(
            names.contains(&format!("Harness {piece}").as_str()),
            "{piece} in {names:?}"
        );
    }
    let horse = {
        let world = app.world_mut();
        world
            .query_filtered::<Entity, With<VehicleHorse>>()
            .single(world)
            .unwrap()
    };
    // Cada pieza sobre las articulaciones de este caballo.
    for (_, name, joints) in &pieces {
        for &j in joints {
            let mut at = j;
            while at != horse {
                at = app
                    .world()
                    .get::<ChildOf>(at)
                    .unwrap_or_else(|| panic!("{name}: a joint off the horse"))
                    .parent();
            }
        }
    }
    let mut measured = 0;
    for (animation, speed) in [
        (AnimationState::Idle, 0.0),
        (AnimationState::Walk, 1.2),
        (AnimationState::Gallop, 6.0),
    ] {
        app.world_mut().entity_mut(me).insert(VisualPose {
            animation,
            speed,
            ..default()
        });
        for frame in 0..60 {
            app.update();
            if frame % 6 != 0 {
                continue;
            }
            // Bajo carga la escena del caballo a veces se crea de nuevo: un
            // cuadro sin ella (ni sus arreos) no se mide.
            let now = harness(&mut app);
            let Some(lip) = try_named(app.world(), horse, HorseJoint::UpperLip.name()) else {
                continue;
            };
            let lip = app
                .world()
                .get::<GlobalTransform>(lip)
                .unwrap()
                .translation();
            let drawn = cpu::drawn(app.world_mut(), Some(horse));
            let Some(bit) = now
                .iter()
                .find(|(_, n, _)| n == "Harness Bit")
                .and_then(|(e, ..)| drawn.iter().find(|d| d.entity == *e))
            else {
                continue;
            };
            let mid = bit.positions.iter().sum::<Vec3>() / bit.positions.len() as f32;
            // Las argollas en las comisuras, a una mano del labio de arriba.
            assert!(
                mid.distance(lip) < 0.12,
                "{animation:?} frame {frame}: the bit {mid} is {} m from the upper lip {lip}",
                mid.distance(lip)
            );
            measured += 1;
        }
    }
    assert!(measured > 20, "only {measured} of 30 frames measured");
}

#[test]
fn the_left_ox_wears_the_yoke_on_its_head_once() {
    use crate::gltf::tests::{root, unfinished_app};
    use models::DressedOn;
    let mut app = unfinished_app(&root());
    app.init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>()
        .add_plugins(bevy::animation::AnimationPlugin)
        .insert_resource(root())
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            20,
        )))
        .add_plugins((
            crate::fauna::FaunaPlugin,
            CharacterVisualPlugin,
            VehiclePlugin,
        ));
    app.finish();
    app.cleanup();
    app.update();
    app.world_mut().spawn((
        VehicleVisual::seeded(Vehicle::Carreta, 2, 7).driverless(),
        Rolled(0.0),
    ));
    let yoke = |app: &mut App| {
        let world = app.world_mut();
        world
            .query::<(&Name, &ChildOf)>()
            .iter(world)
            .filter(|(n, _)| n.as_str() == "Harness on Head")
            .map(|(_, p)| p.parent())
            .collect::<Vec<_>>()
    };
    // Each ox, its side and the node it was dressed on, once both are: the
    // yoke's pieces then, read in the same frame (Bevy may create the ox's
    // scene again mid-test, and dresses it again).
    let dressed = |app: &mut App| {
        let on = yoke(app);
        let world = app.world_mut();
        let oxen: Vec<(Entity, VehicleOx, DressedOn)> = world
            .query::<(Entity, &VehicleOx, Option<&DressedOn>)>()
            .iter(world)
            .filter_map(|(e, o, d)| d.map(|d| (e, *o, *d)))
            .collect();
        let marked = oxen.len() == 2 && oxen.iter().all(|o| world.get::<Dressed>(o.2.0).is_some());
        (!on.is_empty() && marked).then_some((on, oxen))
    };
    let wait = |app: &mut App| {
        for _ in 0..3000 {
            app.update();
            if let Some(found) = dressed(app) {
                return found;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("the oxen were never dressed");
    };
    wait(&mut app);
    // A few more frames: dressed once, never again while its scene lasts (a
    // scene created again takes its old yoke with it).
    for _ in 0..20 {
        app.update();
        let now = yoke(&mut app);
        assert!(now.len() <= 2, "dressed again: {now:?}");
    }
    let (on, oxen) = wait(&mut app);
    // Its two materials (the wood and the hide straps), both on the left ox's
    // `Head` joint.
    assert_eq!(on.len(), 2, "{on:?}");
    // Both remember the node they were dressed on (the right one with
    // nothing), so neither is walked again every frame (checked in `wait`).
    for (ox, VehicleOx { left }, _) in oxen {
        let head = crate::character::horse::tests::named(app.world(), ox, "Head");
        assert_eq!(on.iter().all(|&j| j == head), left, "left: {left}");
    }
}

#[test]
fn a_draught_horses_reins_stay_on_its_bit_as_it_turns_its_head_walks_and_gallops() {
    // S10: the driver's reins end at the bit's rings however the horse holds
    // its head (in `Idle_1` henry turns it up to 60 degrees), not where the
    // bit was standing still.
    use crate::character::cpu;
    use crate::gltf::tests::{root, unfinished_app};
    let mut app = unfinished_app(&root());
    app.init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>()
        .add_plugins(bevy::animation::AnimationPlugin)
        .insert_resource(root())
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            20,
        )))
        .add_plugins((
            crate::fauna::FaunaPlugin,
            CharacterVisualPlugin,
            VehiclePlugin,
        ));
    app.finish();
    app.cleanup();
    app.update();
    let me = app
        .world_mut()
        .spawn((
            VehicleVisual::seeded(Vehicle::Sulky, 2, 7).driverless(),
            Rolled(0.0),
        ))
        .id();
    let named = |app: &mut App, name: &str| {
        let world = app.world_mut();
        world
            .query::<(Entity, &Name)>()
            .iter(world)
            .find(|(_, n)| n.as_str() == name)
            .map(|(e, _)| e)
    };
    let mut ready = false;
    for _ in 0..3000 {
        app.update();
        let model = {
            let world = app.world_mut();
            world
                .query_filtered::<(), With<VehicleWheel>>()
                .iter(world)
                .count()
        };
        if model > 0 && named(&mut app, "Harness Bit").is_some() {
            ready = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(ready, "the sulky and its horse's harness never came");
    // The nearest rein vertex (on the sulky or on the harness) to the middle
    // of the bit, whose rings are 5.5 cm to either side of it.
    let off_the_bit = |app: &mut App| -> Option<f32> {
        let bit = named(app, "Harness Bit")?;
        let reins = named(app, "Harness Reins");
        let model = {
            let world = app.world_mut();
            world
                .query_filtered::<Entity, With<VehicleModel>>()
                .iter(world)
                .next()?
        };
        let all = cpu::drawn(app.world_mut(), Some(me));
        let under_model = cpu::drawn(app.world_mut(), Some(model));
        let bit = all.iter().find(|d| d.entity == bit)?;
        let mid = bit.positions.iter().sum::<Vec3>() / bit.positions.len() as f32;
        let rein_points = under_model
            .iter()
            .chain(all.iter().filter(|d| Some(d.entity) == reins))
            .flat_map(|d| d.positions.iter());
        Some(
            rein_points
                .map(|p| p.distance(mid))
                .fold(f32::MAX, f32::min),
        )
    };
    let mut worst: (f32, String) = (0.0, String::new());
    let mut measured = 0;
    // A whole `Idle_1` (6 s), then the walk and the gallop.
    for (animation, speed, frames) in [
        (AnimationState::Idle, 0.0, 320),
        (AnimationState::Walk, 1.2, 60),
        (AnimationState::Gallop, 6.0, 60),
    ] {
        app.world_mut().entity_mut(me).insert(VisualPose {
            animation,
            speed,
            ..default()
        });
        for frame in 0..frames {
            app.update();
            if frame % 8 != 0 {
                continue;
            }
            // A frame while the horse's scene is created again is skipped.
            let Some(off) = off_the_bit(&mut app) else {
                continue;
            };
            measured += 1;
            if off > worst.0 {
                worst = (off, format!("{animation:?} frame {frame}"));
            }
        }
    }
    println!(
        "the reins at most {:.3} m from the bit's middle ({})",
        worst.0, worst.1
    );
    assert!(measured > 40, "only {measured} frames measured");
    assert!(
        worst.0 < 0.09,
        "{}: the reins end {} m from the bit",
        worst.1,
        worst.0
    );
}

/// A headless app with the fauna, characters and vehicles, and `visual`
/// spawned and played until its model, its horses' harness and (if it has
/// one) its driver are all there: the app, the vehicle and the driver.
fn vehicle_ready(visual: VehicleVisual) -> (App, Entity, Option<Entity>) {
    use crate::character::Rig;
    use crate::character::rider::Seated;
    use crate::gltf::tests::{root, unfinished_app};
    let mut app = unfinished_app(&root());
    app.init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>()
        .add_plugins(bevy::animation::AnimationPlugin)
        .insert_resource(root())
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            20,
        )))
        .add_plugins((
            crate::fauna::FaunaPlugin,
            CharacterVisualPlugin,
            VehiclePlugin,
        ));
    app.finish();
    app.cleanup();
    // No camera: with one, whatever is out of its view keeps its last pose.
    app.update();
    let me = app.world_mut().spawn((visual, Rolled(0.0))).id();
    for _ in 0..3000 {
        app.update();
        let world = app.world_mut();
        let driver = world
            .query::<(Entity, &Rig, &Seated, &Visibility)>()
            .iter(world)
            .find(|(_, r, _, v)| r.started && **v == Visibility::Inherited)
            .map(|(e, ..)| e);
        let wheels = world
            .query_filtered::<(), With<VehicleWheel>>()
            .iter(world)
            .count();
        let bits = world
            .query::<&Name>()
            .iter(world)
            .filter(|n| n.as_str() == "Harness Bit")
            .count();
        if wheels > 0
            && bits == visual.vehicle.horses()
            && (driver.is_some() || visual.driver.is_none())
        {
            return (app, me, driver);
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!(
        "{:?} never came with its harness and driver",
        visual.vehicle
    );
}

/// Every vertex drawn for the vehicle `me` but not on its horses nor its
/// driver: the vehicle's own model and whatever hangs from it, the reins
/// included.
fn vehicle_points(app: &mut App, me: Entity, driver: Option<Entity>) -> Vec<(Vec3, String)> {
    use crate::character::cpu;
    let horses: Vec<Entity> = {
        let world = app.world_mut();
        world
            .query_filtered::<Entity, With<VehicleHorse>>()
            .iter(world)
            .collect()
    };
    let mut not_mine = std::collections::HashSet::new();
    for e in horses.into_iter().chain(driver) {
        not_mine.extend(
            cpu::drawn(app.world_mut(), Some(e))
                .into_iter()
                .map(|d| d.entity),
        );
    }
    cpu::drawn(app.world_mut(), Some(me))
        .into_iter()
        .filter(|d| !not_mine.contains(&d.entity))
        .flat_map(|d| {
            let name = d.name.clone();
            d.positions.into_iter().map(move |p| (p, name.clone()))
        })
        .collect()
}

/// How many meshes drawn for the vehicle `me` (not its horses nor its
/// driver) reach within `reach` of `point`: by a vertex, or by the axis of a
/// strap (a rein's stretch is a unit strap with a ring of vertices at each
/// end, its line overlapping the hand).
fn reaching(app: &mut App, me: Entity, driver: Entity, point: Vec3, reach: f32) -> usize {
    use crate::character::cpu;
    let horses: Vec<Entity> = {
        let world = app.world_mut();
        world
            .query_filtered::<Entity, With<VehicleHorse>>()
            .iter(world)
            .collect()
    };
    let mut not_mine = std::collections::HashSet::new();
    for e in horses.into_iter().chain([driver]) {
        not_mine.extend(
            cpu::drawn(app.world_mut(), Some(e))
                .into_iter()
                .map(|d| d.entity),
        );
    }
    cpu::drawn(app.world_mut(), Some(me))
        .into_iter()
        .filter(|d| !not_mine.contains(&d.entity) && !d.positions.is_empty())
        .filter(|d| {
            let by_vertex = d.positions.iter().any(|p| p.distance(point) < reach);
            let half = d.positions.len() / 2;
            let centre = |v: &[Vec3]| v.iter().sum::<Vec3>() / v.len().max(1) as f32;
            let (a, b) = (centre(&d.positions[..half]), centre(&d.positions[half..]));
            let ab = b - a;
            let t = ((point - a).dot(ab) / ab.length_squared().max(1e-9)).clamp(0.0, 1.0);
            by_vertex || (a + ab * t).distance(point) < reach
        })
        .count()
}

#[test]
fn the_drivers_reins_end_in_his_left_hand_on_the_sulky_and_the_carreton() {
    // S10 (L52): "en ningun caso las riendas llegan a las manos del
    // conductor". Both reins of each horse reach the driver's left palm,
    // whatever the driver's look (their hands lie up to 10 cm apart),
    // standing and on the move.
    use crate::character::horse::tests::try_named;
    use crate::character::rider::PALM;
    for (vehicle, look) in [(Vehicle::Sulky, 0), (Vehicle::Sulky, 3), (Vehicle::Cart, 1)] {
        let (mut app, me, driver) = vehicle_ready(VehicleVisual::seeded(vehicle, look, 7));
        let driver = driver.unwrap();
        let want = 2 * vehicle.horses();
        let mut fewest: (usize, String) = (usize::MAX, String::new());
        let mut measured = 0;
        for (animation, speed) in [(AnimationState::Idle, 0.0), (AnimationState::Walk, 1.2)] {
            app.world_mut().entity_mut(me).insert(VisualPose {
                animation,
                speed,
                ..default()
            });
            for frame in 0..40 {
                app.update();
                if frame % 10 != 9 {
                    continue;
                }
                let Some(hand) = try_named(app.world(), driver, "hand_l") else {
                    continue;
                };
                let palm = app
                    .world()
                    .get::<GlobalTransform>(hand)
                    .unwrap()
                    .transform_point(PALM);
                let n = reaching(&mut app, me, driver, palm, 0.02);
                measured += 1;
                if n < fewest.0 {
                    fewest = (
                        n,
                        format!("{vehicle:?} look {look} {animation:?} frame {frame}"),
                    );
                }
            }
        }
        println!(
            "{}: {} reins at the left palm (want {want})",
            fewest.1, fewest.0
        );
        assert!(measured > 4, "only {measured} frames measured");
        assert!(
            fewest.0 >= want,
            "{}: {} of {want} reins in the hand",
            fewest.1,
            fewest.0
        );
    }
}

/// How deep the vehicle `me` (its model, its reins, anything but its horses
/// and its driver) goes into any of its horses' skin within `radius` of the
/// horse's joint `joint` (only its meshes of material `only`, if given), at
/// the least (negative: inside), standing a whole
/// `Idle_1`, walking and galloping; and where. Measured along the normal of
/// the nearest skin vertex, every tenth frame.
fn deepest_near(
    app: &mut App,
    me: Entity,
    driver: Option<Entity>,
    joint: crate::character::bones::HorseJoint,
    radius: f32,
    only: Option<&str>,
) -> (f32, String) {
    use crate::character::cpu;
    use crate::character::horse::tests::try_named;
    let horses: Vec<Entity> = {
        let world = app.world_mut();
        world
            .query_filtered::<Entity, With<VehicleHorse>>()
            .iter(world)
            .collect()
    };
    let mut worst: (f32, String) = (f32::MAX, String::new());
    let mut measured = 0;
    for (animation, speed, frames) in [
        (AnimationState::Idle, 0.0, 300),
        (AnimationState::Walk, 1.2, 60),
        (AnimationState::Gallop, 6.0, 60),
    ] {
        app.world_mut().entity_mut(me).insert(VisualPose {
            animation,
            speed,
            ..default()
        });
        for frame in 0..frames {
            app.update();
            if frame % 10 != 9 {
                continue;
            }
            let mine = vehicle_points(app, me, driver);
            for &horse in &horses {
                // A frame while the horse's scene is created again is skipped.
                let Some(centre) = try_named(app.world(), horse, joint.name()) else {
                    continue;
                };
                let centre = app
                    .world()
                    .get::<GlobalTransform>(centre)
                    .unwrap()
                    .translation();
                let drawn = cpu::drawn(app.world_mut(), Some(horse));
                let Some(skin) = drawn.iter().find(|d| d.name == "Horse") else {
                    continue;
                };
                let mut normals = vec![Vec3::ZERO; skin.positions.len()];
                for t in &skin.triangles {
                    let [a, b, c] = t.map(|i| skin.positions[i as usize]);
                    for &i in t {
                        normals[i as usize] += (b - a).cross(c - a);
                    }
                }
                let near: Vec<(Vec3, Vec3)> = skin
                    .positions
                    .iter()
                    .zip(&normals)
                    .filter(|(p, _)| p.distance(centre) < radius + 0.15)
                    .map(|(p, n)| (*p, n.normalize_or_zero()))
                    .collect();
                let wanted = |name: &String| only.is_none_or(|m| m == name);
                for (p, name) in mine
                    .iter()
                    .filter(|(p, name)| p.distance(centre) < radius && wanted(name))
                {
                    let p = *p;
                    let (s, n) = near
                        .iter()
                        .min_by(|a, b| a.0.distance_squared(p).total_cmp(&b.0.distance_squared(p)))
                        .unwrap();
                    let off = (p - *s).dot(*n);
                    if off < worst.0 {
                        worst = (
                            off,
                            format!("{animation:?} frame {frame}, {name} at {:.2}", p - centre),
                        );
                    }
                }
                measured += 1;
            }
        }
    }
    assert!(measured > 30, "only {measured} frames measured");
    worst
}

#[test]
fn nothing_of_the_sulky_goes_through_its_horses_croup() {
    // S10 (L52): "en el sulky las riendas o frenos atraviezan la nalga o
    // cadera del caballo". No vertex of the sulky (its reins and the
    // breeching straps the game draws included) lies more than a centimetre
    // inside the horse's quarters, within 45 cm of its croup.
    use crate::character::bones::HorseJoint;
    let (mut app, me, driver) = vehicle_ready(VehicleVisual::seeded(Vehicle::Sulky, 0, 7));
    let (deep, at) = deepest_near(&mut app, me, driver, HorseJoint::Croup, 0.45, None);
    println!("deepest sulky vertex in the croup: {deep:.3} m ({at})");
    assert!(deep > -0.01, "{at}: {deep} m inside the croup");
}

#[test]
fn the_traces_run_along_the_flank_outside_the_skin_on_both_vehicles() {
    // S10 (L52, the user's choice): the traces from the breast collar to the
    // singletrees went 4.7-5.3 cm into the barrel by the elbow. Within 55 cm
    // of the back under the harness pad no leather of the vehicle's model
    // (the traces, the breeching straps' ends round the shafts) is more than
    // a centimetre inside the skin (the traces start on the collar's end, on
    // the skin). The breeching straps the game draws are the croup test's.
    use crate::character::bones::HorseJoint;
    for vehicle in [Vehicle::Sulky, Vehicle::Cart] {
        let (mut app, me, driver) = vehicle_ready(VehicleVisual::seeded(vehicle, 0, 7));
        let (deep, at) = deepest_near(
            &mut app,
            me,
            driver,
            HorseJoint::Back,
            0.55,
            Some("Leather"),
        );
        println!("{vehicle:?}: deepest vertex in the barrel: {deep:.3} m ({at})");
        assert!(deep > -0.01, "{vehicle:?} {at}: {deep} m inside the barrel");
    }
}
