//! Lo que cambia en una pulpería mientras se juega, sobre la sala que dibuja
//! el kit de fachadas (`facade::town_buildings::interior`).
//!
//! - **Las hojas de la puerta del medio** ([`PulperiaLeaf`]): las dos hojas de
//!   tableros, cada una su propia entidad, se abren de día un cuarto de vuelta
//!   hacia adentro, de canto contra el derrame de su jamba, y se cierran de
//!   noche. No deciden nada: siguen [`PulperiaShut`], que escribe el juego con
//!   su propia regla (no se cierra con alguien adentro o en el umbral).
//! - **El farol** ([`PulperiaLamp`]): el tubo de vidrio brilla con la llama,
//!   de noche a pleno y de día tenue, porque en una sala oscura el querosén
//!   ardía todo el día ([`flame_level`]). Una sola luz puntual cálida
//!   ([`PulperiaLight`], con sombra, para que no atraviese las paredes) se
//!   pone en el farol de la pulpería más cercana a la cámara, si está a
//!   menos de [`LIGHT_REACH`]; si no, se oculta.
//! - **La penumbra de adentro** ([`IndoorDim`]): mientras la cámara está
//!   dentro de una sala (sobre su piso y bajo su cielorraso), la luz ambiente
//!   y la del entorno del cielo bajan a [`INDOOR_LIGHT`], entrando y saliendo
//!   de a poco en [`DIM_SECONDS`]. Afuera quedan exactamente como estaban.

use std::collections::BTreeSet;

use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use pampa_core::Aabb;

use crate::atmosphere::WorldClock;
use crate::character::viewer::world_layers;
use crate::facade::Surface;
use crate::facade::town_buildings::{Hinge, lamp_chimney};
use crate::lamps::{KEROSENE, lamp_glow};
use crate::terrain::WorldTerrain;
use crate::town::WorldTowns;

/// Los pueblos (por su índice en [`WorldTowns`]) cuya pulpería tiene la puerta
/// del medio cerrada. Lo escribe el juego; render solo mueve las hojas.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct PulperiaShut(pub BTreeSet<usize>);

/// Una hoja de la puerta del medio de la pulpería del pueblo `town`: un
/// pivote con sus mallas de hijas, cerradas en el espacio propio del
/// edificio, que gira sobre su bisagra.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct PulperiaLeaf {
    pub town: usize,
    /// Dónde está el edificio en el mundo.
    building: Transform,
    hinge: Hinge,
    /// Cuán abierta está, de 0 (cerrada) a 1 (abierta).
    open: f32,
}

impl PulperiaLeaf {
    /// Una hoja abierta, como está de día.
    pub(crate) fn new(town: usize, building: Transform, hinge: Hinge) -> Self {
        Self {
            town,
            building,
            hinge,
            open: 1.0,
        }
    }

    /// Cuán abierta está, de 0 (cerrada) a 1 (abierta).
    pub fn open(&self) -> f32 {
        self.open
    }

    /// Dónde queda el pivote, con la hoja tan abierta como está: el giro
    /// acelera y frena como una hoja empujada.
    pub fn transform(&self) -> Transform {
        let t = self.open;
        self.building
            .mul_transform(self.hinge.pose(t * t * (3.0 - 2.0 * t)))
    }
}

/// El farol de la pulpería del pueblo `town`, con el lugar del mundo donde
/// arde su mecha. Lleva su tubo de vidrio luminoso.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct PulperiaLamp {
    pub town: usize,
    pub flame: Vec3,
}

/// La luz del farol de la pulpería más cercana.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct PulperiaLight;

/// Cuánto tarda una hoja en abrirse o cerrarse del todo, en segundos.
pub const SWING_SECONDS: f32 = 1.2;
/// Hasta qué distancia de la cámara se enciende la luz de un farol, en metros.
pub const LIGHT_REACH: f32 = 40.0;
/// Hasta dónde llega la luz del farol, en metros: toda la sala.
pub const PULPERIA_LIGHT_RANGE: f32 = 14.0;
/// La luz del farol a pleno, de noche, en lúmenes.
pub const PULPERIA_LUMENS: f32 = 50_000.0;
/// La parte de su luz de noche que da el farol de día.
pub const DAY_FLAME: f32 = 0.35;
/// Cuánto sobre la mecha queda el centro de la llama, en metros: ahí va la
/// luz, dentro del tubo.
pub const FLAME_BODY: f32 = 0.05;
/// El emisivo del tubo de vidrio a pleno (lineal).
const CHIMNEY_GLOW: LinearRgba = LinearRgba::rgb(8.0, 4.8, 1.8);
/// La parte de la luz ambiente y de la del entorno que queda dentro de la
/// sala.
pub const INDOOR_LIGHT: f32 = 0.25;
/// Cuánto tarda la penumbra en llegar o en irse, en segundos.
pub const DIM_SECONDS: f32 = 0.5;

/// Qué tan encendido está el farol a la hora `hour`: [`DAY_FLAME`] de día, 1
/// de noche, siguiendo a los faroles de la calle ([`lamp_glow`]).
pub fn flame_level(hour: f32) -> f32 {
    DAY_FLAME + (1.0 - DAY_FLAME) * lamp_glow(hour)
}

/// Cuánto está la cámara dentro de una sala, de 0 (afuera) a 1 (adentro), ya
/// suavizado en el tiempo.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct IndoorDim {
    pub inside: f32,
}

impl IndoorDim {
    /// Por cuánto se multiplican la luz ambiente y la del entorno: 1 afuera,
    /// [`INDOOR_LIGHT`] adentro.
    pub fn factor(&self) -> f32 {
        let t = self.inside.clamp(0.0, 1.0);
        1.0 - (1.0 - INDOOR_LIGHT) * (t * t * (3.0 - 2.0 * t))
    }
}

/// Lo que comparten los faroles de las pulperías: la malla del tubo y su
/// material luminoso.
#[derive(Resource, Debug, Clone)]
struct PulperiaKit {
    chimney: Handle<Mesh>,
    glow: Handle<StandardMaterial>,
}

/// Mueve las hojas de las pulperías, enciende sus faroles y oscurece la luz
/// mientras la cámara está adentro. Lo agrega el [`crate::TownPlugin`].
pub struct PulperiaRenderPlugin;

impl Plugin for PulperiaRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PulperiaShut>()
            .init_resource::<IndoorDim>()
            .add_systems(Startup, setup_kit)
            .add_systems(PreUpdate, dim_indoors)
            .add_systems(Update, (dress_lamps, swing_leaves))
            .add_systems(
                PostUpdate,
                (glow_chimneys, light_nearest).before(TransformSystems::Propagate),
            );
    }
}

/// Agrega el [`PulperiaRenderPlugin`] si todavía no está.
pub(crate) fn ensure_plugin(app: &mut App) {
    if !app.is_plugin_added::<PulperiaRenderPlugin>() {
        app.add_plugins(PulperiaRenderPlugin);
    }
}

fn setup_kit(
    mut commands: Commands,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    let (Some(mut meshes), Some(mut materials)) = (meshes, materials) else {
        return;
    };
    let tube = lamp_chimney()
        .into_meshes()
        .into_iter()
        .find(|(s, _)| *s == Surface::Glass)
        .map(|(_, mesh)| mesh)
        .expect("the chimney is glass");
    commands.insert_resource(PulperiaKit {
        chimney: meshes.add(tube),
        glow: materials.add(StandardMaterial {
            base_color: Color::srgb(0.85, 0.8, 0.66),
            perceptual_roughness: 0.15,
            emissive: LinearRgba::BLACK,
            ..default()
        }),
    });
    // Oculta hasta que la cámara se acerca a una pulpería. En cada capa del
    // mundo, como los faroles de la calle.
    commands.spawn((
        Name::new("Pulperia light"),
        PulperiaLight,
        PointLight {
            color: KEROSENE,
            intensity: 0.0,
            range: PULPERIA_LIGHT_RANGE,
            radius: 0.03,
            shadow_maps_enabled: true,
            contact_shadows_enabled: false,
            ..default()
        },
        world_layers(),
        Transform::default(),
        Visibility::Hidden,
    ));
}

/// Le pone a cada farol nuevo su tubo luminoso, que no hace sombra a su
/// propia luz.
fn dress_lamps(
    mut commands: Commands,
    kit: Option<Res<PulperiaKit>>,
    lamps: Query<Entity, Added<PulperiaLamp>>,
) {
    let Some(kit) = kit else {
        return;
    };
    for lamp in &lamps {
        commands.entity(lamp).insert((
            Mesh3d(kit.chimney.clone()),
            MeshMaterial3d(kit.glow.clone()),
            NotShadowCaster,
        ));
    }
}

/// Abre de día y cierra de noche las hojas, según [`PulperiaShut`]. Una hoja
/// recién creada toma su lugar de una vez; después gira en
/// [`SWING_SECONDS`].
fn swing_leaves(
    time: Res<Time>,
    shut: Res<PulperiaShut>,
    mut leaves: Query<(&mut PulperiaLeaf, &mut Transform)>,
) {
    let step = time.delta_secs() / SWING_SECONDS;
    for (mut leaf, mut transform) in &mut leaves {
        let target = if shut.0.contains(&leaf.town) {
            0.0
        } else {
            1.0
        };
        if leaf.open == target && !leaf.is_added() {
            continue;
        }
        let open = if leaf.is_added() {
            target
        } else {
            leaf.open + (target - leaf.open).clamp(-step, step)
        };
        leaf.open = open;
        *transform = leaf.transform();
    }
}

/// Fija el brillo de los tubos según el reloj, tocando el material compartido
/// solo cuando cambió.
fn glow_chimneys(
    clock: Option<Res<WorldClock>>,
    kit: Option<Res<PulperiaKit>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut shown: Local<Option<f32>>,
) {
    let (Some(clock), Some(kit)) = (clock, kit) else {
        return;
    };
    let level = flame_level(clock.0.hour());
    if shown.is_some_and(|s| (s - level).abs() < 0.005) {
        return;
    }
    if let Some(mut glass) = materials.get_mut(&kit.glow) {
        glass.emissive = CHIMNEY_GLOW * level;
        *shown = Some(level);
    }
}

/// Pone la luz en el farol de la pulpería más cercana a la cámara, si está a
/// menos de [`LIGHT_REACH`]; si no, la oculta.
fn light_nearest(
    clock: Option<Res<WorldClock>>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    lamps: Query<&PulperiaLamp>,
    mut lights: Query<(&mut Transform, &mut PointLight, &mut Visibility), With<PulperiaLight>>,
) {
    let near = camera.iter().next().and_then(|eye| {
        let from = eye.translation();
        lamps
            .iter()
            .map(|lamp| lamp.flame)
            .filter(|flame| flame.distance(from) <= LIGHT_REACH)
            .min_by(|a, b| {
                a.distance_squared(from)
                    .total_cmp(&b.distance_squared(from))
            })
    });
    let level = clock.map_or(DAY_FLAME, |c| flame_level(c.0.hour()));
    for (mut transform, mut light, mut visibility) in &mut lights {
        match near {
            Some(flame) => {
                transform.translation = flame + Vec3::Y * FLAME_BODY;
                light.intensity = PULPERIA_LUMENS * level;
                visibility.set_if_neq(Visibility::Inherited);
            }
            None => {
                visibility.set_if_neq(Visibility::Hidden);
            }
        }
    }
}

/// Una sala de pulpería en el mundo: su piso sobre el suelo y las alturas de
/// su base y de su cielorraso.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Room {
    floor: Aabb,
    base: f32,
    ceiling: f32,
}

/// Las salas de los pueblos de `towns`, cada una a la altura del suelo bajo
/// el medio de su edificio, como las levanta el pueblo.
fn rooms(towns: &WorldTowns, terrain: &WorldTerrain) -> Vec<Room> {
    towns
        .0
        .iter()
        .filter_map(|town| town.pulperia())
        .map(|(building, inside)| {
            let c = building.footprint.center();
            let base = terrain.0.height(c.x, c.y);
            Room {
                floor: inside.floor,
                base,
                ceiling: base + inside.ceiling,
            }
        })
        .collect()
}

/// Sigue si la cámara está dentro de una sala y suaviza [`IndoorDim`] hacia
/// eso en [`DIM_SECONDS`]. Afuera y quieto no toca el recurso.
fn dim_indoors(
    time: Res<Time>,
    towns: Option<Res<WorldTowns>>,
    terrain: Option<Res<WorldTerrain>>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    mut dim: ResMut<IndoorDim>,
    mut cached: Local<Vec<Room>>,
) {
    let (Some(towns), Some(terrain)) = (towns, terrain) else {
        return;
    };
    if towns.is_changed() || terrain.is_changed() {
        *cached = rooms(&towns, &terrain);
    }
    let inside = camera.iter().next().is_some_and(|eye| {
        let p = eye.translation();
        // `world_from_ground` es `(x, h, -y)`.
        let ground = pampa_core::Vec2::new(p.x, -p.z);
        cached
            .iter()
            .any(|room| room.floor.contains_point(ground) && p.y > room.base && p.y < room.ceiling)
    });
    let target = if inside { 1.0 } else { 0.0 };
    if dim.inside == target {
        return;
    }
    let step = time.delta_secs() / DIM_SECONDS;
    dim.inside += (target - dim.inside).clamp(-step, step);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atmosphere::AtmospherePlugin;
    use crate::town::TownPlugin;
    use crate::{StreamFocus, sky_at};
    use bevy::time::TimeUpdateStrategy;
    use pampa_core::{GameClock, TimeOfDay, TownConfig, TownLayout};
    use std::time::Duration;

    const FRAME: f32 = 1.0 / 60.0;

    fn castex() -> TownLayout {
        TownLayout::generate(&TownConfig::castex()).unwrap()
    }

    /// Castex dibujado, a la hora `time`, con el tiempo de a un frame.
    fn app(time: TimeOfDay) -> App {
        let town = castex();
        let at = town.pulperia().unwrap().0.footprint.center();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
                FRAME,
            )))
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .insert_resource(WorldClock(GameClock::at(time)))
            .insert_resource(StreamFocus(at))
            .insert_resource(WorldTowns(vec![town]))
            .add_plugins(TownPlugin);
        app.update();
        app
    }

    fn frames(app: &mut App, seconds: f32) {
        for _ in 0..(seconds / FRAME).ceil() as usize + 1 {
            app.update();
        }
    }

    fn leaves(app: &mut App) -> Vec<(PulperiaLeaf, Transform)> {
        let mut q = app.world_mut().query::<(&PulperiaLeaf, &Transform)>();
        q.iter(app.world()).map(|(l, t)| (*l, *t)).collect()
    }

    fn lamp(app: &mut App) -> PulperiaLamp {
        let mut q = app.world_mut().query::<&PulperiaLamp>();
        *q.single(app.world()).unwrap()
    }

    /// Una cámara en `at`, en el mundo.
    fn camera(app: &mut App, at: Vec3) -> Entity {
        let t = Transform::from_translation(at);
        app.world_mut()
            .spawn((Camera3d::default(), t, GlobalTransform::from(t)))
            .id()
    }

    fn move_camera(app: &mut App, camera: Entity, at: Vec3) {
        let t = Transform::from_translation(at);
        app.world_mut()
            .entity_mut(camera)
            .insert((t, GlobalTransform::from(t)));
    }

    fn same(a: Transform, b: Transform) -> bool {
        a.translation.distance(b.translation) < 1e-4
            && a.rotation.dot(b.rotation).abs() > 1.0 - 1e-6
    }

    #[test]
    fn a_pulperia_has_its_two_leaves_and_its_lamp() {
        let mut app = app(TimeOfDay::hm(12, 0));
        let leaves = leaves(&mut app);
        assert_eq!(leaves.len(), 2);
        assert!(leaves.iter().all(|(l, _)| l.town == 0));
        assert_eq!(lamp(&mut app).town, 0);
        // Cada hoja, sus mallas gastadas debajo del pivote.
        let mut q = app
            .world_mut()
            .query_filtered::<&ChildOf, With<MeshMaterial3d<crate::facade::BuildingMaterial>>>();
        let mut p = app
            .world_mut()
            .query_filtered::<Entity, With<PulperiaLeaf>>();
        let pivots: Vec<Entity> = p.iter(app.world()).collect();
        for pivot in pivots {
            assert!(q.iter(app.world()).any(|c| c.parent() == pivot));
        }
    }

    #[test]
    fn the_leaves_shut_and_open_as_the_game_says() {
        let mut app = app(TimeOfDay::hm(12, 0));
        for (leaf, transform) in leaves(&mut app) {
            let want = leaf.building.mul_transform(leaf.hinge.pose(1.0));
            assert!(same(transform, want), "{transform:?} vs {want:?}");
            assert!(same(
                transform,
                leaf.building.mul_transform(leaf.hinge.pose(1.0))
            ));
        }
        app.world_mut().resource_mut::<PulperiaShut>().0.insert(0);
        app.update();
        // Gira de a poco.
        for (leaf, _) in leaves(&mut app) {
            assert!(leaf.open() > 0.0 && leaf.open() < 1.0, "{}", leaf.open());
        }
        frames(&mut app, SWING_SECONDS);
        for (leaf, transform) in leaves(&mut app) {
            assert_eq!(leaf.open(), 0.0);
            assert!(same(transform, leaf.building), "shut in its door");
        }
        app.world_mut().resource_mut::<PulperiaShut>().0.clear();
        frames(&mut app, SWING_SECONDS);
        for (leaf, transform) in leaves(&mut app) {
            assert_eq!(leaf.open(), 1.0);
            assert!(same(
                transform,
                leaf.building.mul_transform(leaf.hinge.pose(1.0))
            ));
        }
    }

    fn light(app: &mut App) -> (Transform, PointLight, Visibility) {
        let mut q = app
            .world_mut()
            .query_filtered::<(&Transform, &PointLight, &Visibility), With<PulperiaLight>>();
        let (t, l, v) = q.single(app.world()).unwrap();
        (*t, *l, *v)
    }

    #[test]
    fn the_nearest_lamp_lights_warm_at_its_flame_brighter_at_night() {
        let mut app = app(TimeOfDay::hm(23, 0));
        let flame = lamp(&mut app).flame;
        let eye = camera(&mut app, flame + Vec3::new(2.0, -1.0, 1.0));
        app.update();
        let (t, night, shown) = light(&mut app);
        assert_eq!(shown, Visibility::Inherited);
        assert!(t.translation.distance(flame) < 0.1);
        assert_eq!(night.color, KEROSENE);
        assert!(night.shadow_maps_enabled);
        app.insert_resource(WorldClock(GameClock::at(TimeOfDay::hm(12, 0))));
        app.update();
        let (_, day, shown) = light(&mut app);
        assert_eq!(shown, Visibility::Inherited, "dim but lit by day");
        assert!(day.intensity > 0.0 && day.intensity < night.intensity);
        // Lejos de toda pulpería, apagada.
        move_camera(&mut app, eye, flame + Vec3::X * (LIGHT_REACH + 5.0));
        app.update();
        assert_eq!(light(&mut app).2, Visibility::Hidden);
    }

    #[test]
    fn inside_the_room_the_ambient_light_dims_and_outside_it_is_untouched() {
        let mut app = app(TimeOfDay::hm(1, 0));
        app.add_plugins(AtmospherePlugin);
        let flame = lamp(&mut app).flame;
        let street = flame + Vec3::new(0.0, 0.0, 30.0);
        let eye = camera(&mut app, street);
        let budget = sky_at(1.0).ambient_brightness;
        let ambient = |app: &App| app.world().resource::<GlobalAmbientLight>().brightness;
        app.update();
        assert_eq!(ambient(&app), budget, "outside, as ever");
        move_camera(&mut app, eye, flame - Vec3::Y);
        frames(&mut app, DIM_SECONDS / 2.0);
        let half = ambient(&app);
        assert!(half < budget && half > budget * 0.25, "eases in: {half}");
        frames(&mut app, DIM_SECONDS);
        assert!(
            (ambient(&app) - budget * 0.25).abs() < 1e-3,
            "a quarter of the light"
        );
        // Sobre el techo ya no es adentro.
        move_camera(&mut app, eye, flame + Vec3::Y * 4.0);
        frames(&mut app, DIM_SECONDS);
        assert_eq!(ambient(&app), budget, "back to the very same light");
    }
}
