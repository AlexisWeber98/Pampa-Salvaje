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
