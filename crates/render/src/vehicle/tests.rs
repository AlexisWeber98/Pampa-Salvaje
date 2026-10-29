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
fn a_cart_has_four_wheels_and_two_horses_and_a_sulky_two_and_one() {
    let mut app = plugin_app();
    let cart = VehicleVisual::seeded(Vehicle::Cart, 3, 77);
    let me = app.world_mut().spawn(cart).id();
    app.update();
    assert_eq!(count::<With<VehicleBody>>(&mut app), 1);
    assert_eq!(count::<With<VehicleWheel>>(&mut app), 4);
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
    assert_eq!(count::<With<VehicleWheel>>(&mut app), 2);
    assert_eq!(horses(&mut app), [CharacterVisual::Draught(sulky.coats[0])]);
}

#[test]
fn the_wheels_turn_with_the_distance_rolled_and_the_horses_walk_with_the_pose() {
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
    let mut wheels = app.world_mut().query::<(&VehicleWheel, &Transform)>();
    for (wheel, t) in wheels.iter(app.world()) {
        // Una rueda que avanza 1,1 m gira 1,1 / radio radianes, hacia
        // adelante.
        let want = Quat::from_rotation_z(-1.1 / wheel.radius);
        assert!(t.rotation.abs_diff_eq(want, 1e-5), "{:?}", t.rotation);
    }
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
    assert_eq!(riding.kind, sulky.driver);
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
}
