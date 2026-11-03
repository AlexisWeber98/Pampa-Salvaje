//! Los viajeros de los caminos: jinetes y carros que van de pueblo a pueblo.
//!
//! La lógica vive en `pampa-core` ([`Travellers`]). Son pocos, así que se
//! simulan todos en cada frame, antes que el jugador; solo los que andan cerca
//! del jugador ([`DRAW_RADIUS`]) tienen una entidad que dibuja render (un
//! jinete montado o un [`VehicleVisual`]). El jugador no puede atravesar sus
//! caballos ni sus carros.

use bevy::prelude::*;
use pampa_core::{MAX_STEP, Travellers};
use pampa_render::{
    CharacterVisual, Coat, FigureKind, Rolled, VehicleVisual, VisualPose, world_from_ground,
};

use crate::life::{SimulateTown, jostle_crowd, run_clock_and_train};
use crate::player::{DrivePlayer, GameWorld, Player, drive_player};
use crate::ride::OnTrain;

/// La semilla de todos los viajeros.
const TRAVELLERS_SEED: u32 = 0x7669_616a;

/// Hasta qué distancia del jugador se dibuja un viajero, en metros.
pub const DRAW_RADIUS: f32 = 600.0;
/// Margen más allá de [`DRAW_RADIUS`] antes de dejar de dibujarlo, en metros,
/// para que no aparezca y desaparezca en el borde.
const HYSTERESIS: f32 = 100.0;

/// Todos los viajeros de los caminos del mundo.
#[derive(Resource, Debug, Clone, Default)]
pub struct RoadTravellers(pub Travellers);

impl RoadTravellers {
    /// Los viajeros de los caminos de `world` (ninguno sin caminos), con las
    /// entradas de `estancias` como paradas de los vehículos.
    pub fn of(world: &pampa_core::World, estancias: &[pampa_core::Vec2]) -> Self {
        Self(
            world
                .terrain
                .roads
                .as_deref()
                .map(|roads| Travellers::on(roads, estancias, TRAVELLERS_SEED))
                .unwrap_or_default(),
        )
    }
}

/// La entrada de cada estancia de `settlements`: el principio de su huella,
/// justo dentro de la tranquera.
pub fn estancias(settlements: &pampa_core::SettlementLayout) -> Vec<pampa_core::Vec2> {
    settlements
        .settlements
        .iter()
        .filter(|s| s.kind == pampa_core::SettlementKind::Ranch)
        .filter_map(|s| s.trunk().segments().next().map(|(a, _)| a))
        .collect()
}

/// Entidad que dibuja al viajero `.0` de [`RoadTravellers`].
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct OnTheRoad(pub usize);

pub struct TravellersPlugin;

impl Plugin for TravellersPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (ride_travellers, show_travellers)
                .chain()
                .in_set(SimulateTown)
                .after(run_clock_and_train),
        )
        .add_systems(
            Update,
            (keep_player_clear, place_travellers)
                .chain()
                .in_set(DrivePlayer)
                .after(drive_player)
                .after(jostle_crowd),
        );
    }
}

/// Hace avanzar a todos los viajeros, que frenan ante el jugador donde estaba
/// en el frame anterior y esperan el paso de los trenes ya movidos en este
/// frame.
fn ride_travellers(
    time: Res<Time>,
    travellers: Option<ResMut<RoadTravellers>>,
    world: Res<GameWorld>,
    players: Query<&Player>,
) {
    let (Some(mut travellers), Some(roads)) = (travellers, world.0.terrain.roads.as_deref()) else {
        return;
    };
    let player: Vec<_> = players.iter().map(|p| p.rider.body()).collect();
    let dt = time.delta_secs().min(MAX_STEP);
    travellers.0.step(roads, &player, &world.0.moving, dt);
}

/// Crea las entidades de los viajeros que se acercaron al jugador y elimina las
/// de los que quedaron lejos.
fn show_travellers(
    mut commands: Commands,
    travellers: Option<Res<RoadTravellers>>,
    world: Res<GameWorld>,
    players: Query<&Player>,
    shown: Query<(Entity, &OnTheRoad)>,
) {
    let Some(travellers) = travellers else {
        return;
    };
    let Some(player) = players.iter().next().map(|p| p.rider.position) else {
        return;
    };
    let mut drawn = vec![false; travellers.0.riders.len()];
    for (entity, on) in &shown {
        let near = travellers
            .0
            .riders
            .get(on.0)
            .is_some_and(|t| t.position.distance(player) < DRAW_RADIUS + HYSTERESIS);
        if near {
            drawn[on.0] = true;
        } else {
            commands.entity(entity).despawn();
        }
    }
    for (index, t) in travellers.0.riders.iter().enumerate() {
        if drawn[index] || t.position.distance(player) >= DRAW_RADIUS {
            continue;
        }
        let height = world.0.ground_height(t.position);
        let mut traveller = commands.spawn((
            Name::new(format!("Traveller {index}")),
            OnTheRoad(index),
            pose_of(t),
            Transform::from_translation(world_from_ground(t.position, height)),
        ));
        match t.vehicle {
            Some(vehicle) => traveller.insert((
                VehicleVisual::seeded(vehicle, t.look, t.coat),
                Rolled(t.travelled),
            )),
            None => traveller.insert(CharacterVisual::Mounted {
                rider: FigureKind::townsfolk(t.look),
                coat: Coat::seeded(t.coat),
            }),
        };
    }
}

fn pose_of(t: &pampa_core::Traveller) -> VisualPose {
    VisualPose {
        heading: t.heading,
        animation: t.animation(),
        speed: t.speed,
        turn: 0.0,
        flight: None,
    }
}

/// El jugador se detiene contra los caballos y los carros de los viajeros: no
/// se apartan.
fn keep_player_clear(
    travellers: Option<Res<RoadTravellers>>,
    world: Res<GameWorld>,
    mut players: Query<(&mut Player, &mut Transform), Without<OnTrain>>,
) {
    let Some(travellers) = travellers else {
        return;
    };
    let bodies = travellers.0.bodies();
    for (mut player, mut transform) in &mut players {
        let rider = &mut player.rider;
        rider.keep_clear_of(&world.0, &bodies);
        transform.translation = world_from_ground(rider.position, rider.elevation());
    }
}

/// Copia el estado lógico de los viajeros en sus entidades.
fn place_travellers(
    travellers: Option<Res<RoadTravellers>>,
    world: Res<GameWorld>,
    mut shown: Query<
        (
            &OnTheRoad,
            &mut Transform,
            &mut VisualPose,
            Option<&mut Rolled>,
        ),
        Without<Player>,
    >,
) {
    let Some(travellers) = travellers else {
        return;
    };
    for (on, mut transform, mut pose, rolled) in &mut shown {
        let Some(t) = travellers.0.riders.get(on.0) else {
            continue;
        };
        let height = world.0.ground_height(t.position);
        transform.translation = world_from_ground(t.position, height);
        *pose = pose_of(t);
        if let Some(mut rolled) = rolled {
            rolled.0 = t.travelled;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::life::TownLifePlugin;
    use crate::player::{PlayerPlugin, PlayerSpawn};
    use bevy::time::TimeUpdateStrategy;
    use pampa_core::{Heading, Travel, Vec2, Vehicle};
    use pampa_render::{Rolled, VehicleVisual};
    use std::time::Duration;

    fn app_near_traveller() -> (App, Vec2) {
        let world = crate::shipped_world::laid().world.clone();
        let travellers = RoadTravellers::of(&world, &[]);
        // Parado en el campo, a 20 m del primer viajero.
        let near = travellers.0.riders[0].position + Vec2::new(0.0, 20.0);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                50,
            )))
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(GameWorld(world))
            .insert_resource(travellers)
            .insert_resource(PlayerSpawn {
                position: near,
                heading: Heading::default(),
                travel: Travel::OnFoot,
            })
            .add_plugins((PlayerPlugin, TownLifePlugin, TravellersPlugin));
        (app, near)
    }

    fn shown(app: &mut App) -> Vec<(OnTheRoad, Transform, CharacterVisual)> {
        let mut q = app
            .world_mut()
            .query::<(&OnTheRoad, &Transform, &CharacterVisual)>();
        q.iter(app.world()).map(|(o, t, v)| (*o, *t, *v)).collect()
    }

    fn player(app: &mut App) -> Vec2 {
        let mut q = app.world_mut().query::<&Player>();
        q.single(app.world()).unwrap().rider.position
    }

    #[test]
    fn only_travellers_near_the_player_are_drawn_where_they_ride() {
        let (mut app, _) = app_near_traveller();
        for _ in 0..10 {
            app.update();
        }
        let travellers = app.world().resource::<RoadTravellers>().0.clone();
        let world = app.world().resource::<GameWorld>().0.clone();
        let at = player(&mut app);
        let shown = shown(&mut app);
        assert!(shown.iter().any(|(o, ..)| o.0 == 0));
        assert!(
            shown.len() < travellers.riders.len(),
            "far ones are drawn too"
        );
        for (on, transform, visual) in shown {
            let t = &travellers.riders[on.0];
            assert!(t.position.distance(at) < DRAW_RADIUS + HYSTERESIS);
            let height = world.ground_height(t.position);
            assert_eq!(transform.translation, world_from_ground(t.position, height));
            assert_eq!(
                visual,
                CharacterVisual::Mounted {
                    rider: FigureKind::townsfolk(t.look),
                    coat: Coat::seeded(t.coat),
                }
            );
        }
        // Lejos de todos los caminos, no se dibuja ninguno.
        let mut q = app.world_mut().query::<&mut Player>();
        q.single_mut(app.world_mut()).unwrap().rider.position = Vec2::new(1.0e5, 1.0e5);
        app.update();
        assert!(self::shown(&mut app).is_empty());
    }

    /// Una app con el jugador parado a 20 m del primer carro, y el índice del
    /// carro.
    fn app_near_cart() -> (App, usize) {
        let (mut app, _) = app_near_traveller();
        let travellers = app.world().resource::<RoadTravellers>().0.clone();
        let cart = travellers
            .riders
            .iter()
            .position(|t| t.vehicle.is_some())
            .expect("a cart");
        let near = travellers.riders[cart].position + Vec2::new(0.0, 20.0);
        app.world_mut()
            .resource_mut::<RoadTravellers>()
            .0
            .riders
            .truncate(cart + 1);
        app.update();
        let mut q = app.world_mut().query::<&mut Player>();
        q.single_mut(app.world_mut()).unwrap().rider.position = near;
        (app, cart)
    }

    #[test]
    fn a_cart_near_the_player_is_drawn_rolling_where_it_goes() {
        let (mut app, cart) = app_near_cart();
        for _ in 0..20 {
            app.update();
        }
        let t = app.world().resource::<RoadTravellers>().0.riders[cart].clone();
        let mut q = app
            .world_mut()
            .query::<(&OnTheRoad, &VehicleVisual, &Rolled, &VisualPose)>();
        let (_, visual, rolled, pose) = q
            .iter(app.world())
            .find(|(o, ..)| o.0 == cart)
            .expect("the cart is drawn");
        assert_eq!(
            *visual,
            VehicleVisual::seeded(Vehicle::Cart, t.look, t.coat)
        );
        assert!(t.travelled > 0.0);
        assert_eq!(rolled.0, t.travelled);
        assert_eq!(pose.speed, t.speed);
    }

    #[test]
    fn the_player_cannot_walk_through_a_cart() {
        let (mut app, cart) = app_near_cart();
        app.update();
        let t = app.world().resource::<RoadTravellers>().0.riders[cart].clone();
        // Contra la caja, detrás de los caballos.
        let tail = t.position - t.heading.forward() * 3.0;
        let mut q = app.world_mut().query::<&mut Player>();
        q.single_mut(app.world_mut()).unwrap().rider.position = tail;
        app.update();
        let bodies = app.world().resource::<RoadTravellers>().0.riders[cart].bodies();
        let mut q = app.world_mut().query::<&Player>();
        let rider = q.single(app.world()).unwrap().rider;
        for b in bodies {
            assert!(
                rider.position.distance(b.position)
                    >= rider.collider.radius + b.collider.radius - 1e-3
            );
        }
    }

    #[test]
    fn the_player_cannot_walk_through_a_travellers_horse() {
        let (mut app, _) = app_near_traveller();
        app.update();
        let horse = app.world().resource::<RoadTravellers>().0.riders[0].position;
        let mut q = app.world_mut().query::<&mut Player>();
        q.single_mut(app.world_mut()).unwrap().rider.position = horse;
        app.update();
        let t = app.world().resource::<RoadTravellers>().0.riders[0].clone();
        let mut q = app.world_mut().query::<&Player>();
        let rider = q.single(app.world()).unwrap().rider;
        assert!(
            rider.position.distance(t.position) >= rider.collider.radius + t.collider.radius - 1e-3
        );
    }
}
