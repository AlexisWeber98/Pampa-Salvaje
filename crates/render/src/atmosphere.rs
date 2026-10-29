//! Color del cielo, sol, luz ambiente y niebla de polvo para la pampa seca,
//! según el reloj del juego: un sol bajo y cálido al amanecer y al atardecer,
//! plena luz de día, y una noche de luna tenue y azulada que sigue siendo
//! legible.
//!
//! Las cascadas de sombra del sol se adaptan a la cámara orbital; una cámara en
//! primera persona pide una primera cascada más corta con [`CloseShadows`]. La
//! niebla de polvo se mide desde la cámara que esté dibujando, así que allí no
//! necesita ningún cambio.

use crate::character::viewer::world_layers;
use crate::sky::astro;
use bevy::light::{CascadeShadowConfig, CascadeShadowConfigBuilder, EnvironmentMapLight};
use bevy::prelude::*;
use pampa_core::GameClock;

/// Color de bruma cálida y polvorienta que comparten el cielo y la niebla de
/// distancia durante el día.
pub const DUST_COLOR: Color = Color::srgb(0.82, 0.70, 0.52);
/// Color de la bruma en una noche de luna.
const NIGHT_HAZE: Color = Color::srgb(0.16, 0.18, 0.25);
/// Color de la bruma en el horizonte cuando el sol está sobre él: polvo
/// iluminado de rosa anaranjado.
const TWILIGHT_HAZE: Color = Color::srgb(0.88, 0.50, 0.36);
/// Color del cenit de día: azul profundo.
const DAY_ZENITH: Color = Color::srgb(0.05, 0.40, 0.78);
/// Color del cenit con el sol bajo: un azul más pálido, con un dejo violeta.
const LOW_SUN_ZENITH: Color = Color::srgb(0.16, 0.40, 0.72);
/// Color del cenit en la hora azul después de la puesta y antes del amanecer.
const BLUE_HOUR_ZENITH: Color = Color::srgb(0.06, 0.10, 0.27);
/// Color del cenit de noche.
const NIGHT_ZENITH: Color = Color::srgb(0.012, 0.022, 0.07);

/// El reloj del juego. Estado lógico: el juego lo hace correr y render solo lo
/// lee.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct WorldClock(pub GameClock);

/// Si la cámara mira desde los ojos del jugador (`true`), de modo que la
/// primera cascada de sombra del sol cubra [`FIRST_PERSON_CASCADE`] en lugar de
/// [`FIRST_CASCADE`]: lo que queda nítido entonces es lo que está cerca de los
/// ojos.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CloseShadows(pub bool);

/// Marca la luz direccional que hace de sol (y de luna).
#[derive(Component, Debug, Clone, Copy)]
pub struct Sun;

/// Prepara el sol, la luz ambiente y el cielo, agrega niebla de polvo a cada
/// cámara 3D y mueve la luz con el [`WorldClock`] en cada frame. Sin reloj, la
/// luz queda en [`DEFAULT_HOUR`].
pub struct AtmospherePlugin;

/// La hora que se ilumina cuando el mundo no tiene reloj: la tarde avanzada.
pub const DEFAULT_HOUR: f32 = 16.5;

impl Plugin for AtmospherePlugin {
    fn build(&self, app: &mut App) {
        let sky = sky_at(DEFAULT_HOUR);
        app.insert_resource(ClearColor(sky.haze))
            .insert_resource(GlobalAmbientLight {
                color: sky.ambient_color,
                brightness: sky.ambient_brightness,
                ..default()
            })
            .init_resource::<CloseShadows>()
            .add_systems(Startup, spawn_sun)
            .add_systems(
                PostUpdate,
                (add_dust_fog, follow_the_clock, fit_cascades).chain(),
            );
    }
}

/// Salida del sol, en horas desde la medianoche.
pub const SUNRISE: f32 = 6.5;
/// Puesta del sol, en horas desde la medianoche.
pub const SUNSET: f32 = 19.5;
/// La altura del sol al mediodía, en radianes (un verano pampeano: unos 65°).
const NOON_ELEVATION: f32 = 1.13;
/// Iluminancia del sol pleno, en lux.
const SUN_LUX: f32 = 10_000.0;
/// Iluminancia de la luna, en lux: tenue, pero suficiente para distinguir el
/// suelo.
const MOON_LUX: f32 = 900.0;

/// La parte de [`Sky::ambient_brightness`] que aporta la luz basada en imagen
/// del environment map del cielo, cuando un renderer lo provee (ver
/// `sky::environment`); el [`GlobalAmbientLight`] plano conserva el resto, así
/// que la exposición ambiente total se mantiene dentro del presupuesto del
/// cielo. Si no está, el ambiente plano aporta todo.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct EnvironmentShare(pub f32);

/// Cómo ilumina el cielo el mundo a una hora dada.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sky {
    /// Dirección unitaria desde el suelo hacia la luz, en espacio del mundo de
    /// Bevy.
    pub toward_light: Vec3,
    pub light_color: Color,
    pub illuminance: f32,
    pub ambient_color: Color,
    pub ambient_brightness: f32,
    /// Color de la niebla y de fondo.
    pub haze: Color,
    /// La altura del sol sobre el horizonte, en radianes (negativa de noche).
    pub elevation: f32,
    /// `0` de noche, `1` con luz de día.
    pub daylight: f32,
    /// Dirección unitaria hacia el sol mismo, que se hunde bajo el horizonte de
    /// noche mientras `toward_light` pasa a la luna.
    pub sun_dir: Vec3,
    /// Dirección unitaria hacia el disco de la luna.
    pub moon_dir: Vec3,
    /// La edad de la luna, en días.
    pub moon_age: f32,
    /// Color en lo alto; el horizonte es `haze`.
    pub zenith: Color,
    /// Cuánto tiñe el resplandor del sol al cielo: `0` de día y de noche, `1`
    /// con el sol sobre el horizonte.
    pub twilight: f32,
    /// Cuánto del campo de estrellas se ve: `0` de día, `1` una vez que el
    /// cielo está oscuro.
    pub star_fade: f32,
}

/// El cielo a la hora `hour` (horas desde la medianoche, acotadas a un día).
///
/// El sol sale por el este a las [`SUNRISE`], cruza el cielo del norte (la
/// pampa está al sur del ecuador) y se pone por el oeste a las [`SUNSET`]. Una
/// vez que está bien por debajo del horizonte, la luz pasa a una luna fija y
/// azulada, alta en el noreste; el cambio ocurre mientras ambas están en cero.
pub fn sky_at(hour: f32) -> Sky {
    sky_with_moon(hour, astro::FIRST_MOON_AGE)
}

/// [`sky_at`] con la luna a `moon_age` días: la luz nocturna sigue a la luna
/// (ver [`astro::moonlight_dir`]).
pub fn sky_with_moon(hour: f32, moon_age: f32) -> Sky {
    let hour = hour.rem_euclid(24.0);
    let day = (hour - SUNRISE) / (SUNSET - SUNRISE);
    // Pasada la puesta, el sol sigue hundiéndose por el mismo arco y luego
    // vuelve a subir por debajo del horizonte hacia la salida.
    let night_length = 24.0 - (SUNSET - SUNRISE);
    let elevation = if (0.0..=1.0).contains(&day) {
        NOON_ELEVATION * (std::f32::consts::PI * day).sin()
    } else {
        let into = (hour - SUNSET).rem_euclid(24.0) / night_length;
        -0.6 * (std::f32::consts::PI * into).sin()
    };
    // Del este (acimut 0) pasando por el norte (π/2) hasta el oeste (π), sobre
    // el plano del suelo.
    let azimuth = std::f32::consts::PI * day.clamp(0.0, 1.0);
    let daylight = smoothstep(-0.05, 0.12, elevation);
    let low = 1.0 - smoothstep(0.05, 0.6, elevation);
    let night = 1.0 - smoothstep(-0.2, -0.04, elevation);

    let sun_dir = Vec3::new(
        azimuth.cos() * elevation.cos(),
        elevation.sin(),
        -azimuth.sin() * elevation.cos(),
    );
    let moon_dir = astro::moon_dir(hour, moon_age);
    let (toward_light, light_color, illuminance) = if elevation > -0.04 {
        let ground = sun_dir;
        let warm = mix(
            Color::srgb(1.0, 0.95, 0.86),
            Color::srgb(1.0, 0.58, 0.32),
            low,
        );
        let lux = SUN_LUX * smoothstep(-0.04, 0.5, elevation);
        (ground, warm, lux)
    } else {
        let moon = astro::moonlight_dir(moon_dir);
        (moon, Color::srgb(0.62, 0.70, 0.95), MOON_LUX * night)
    };
    let ambient_color = mix(
        Color::srgb(0.55, 0.62, 0.85),
        mix(
            Color::srgb(1.0, 0.88, 0.72),
            Color::srgb(1.0, 0.72, 0.55),
            low,
        ),
        daylight,
    );
    // El resplandor del sol tiñe más el cielo con el sol sobre el horizonte, y
    // persiste un poco después de la puesta.
    let twilight = (-((elevation + 0.02) / 0.15).powi(2)).exp();
    let haze = mix(
        NIGHT_HAZE,
        mix(DUST_COLOR, Color::srgb(0.86, 0.60, 0.42), low * 0.3),
        daylight,
    );
    let haze = mix(haze, TWILIGHT_HAZE, twilight * 0.6);
    let blue_hour = (-((elevation + 0.15) / 0.08).powi(2)).exp();
    let dark = mix(NIGHT_ZENITH, BLUE_HOUR_ZENITH, blue_hour);
    let lit = mix(LOW_SUN_ZENITH, DAY_ZENITH, smoothstep(0.0, 0.5, elevation));
    let zenith = mix(dark, lit, daylight);
    Sky {
        toward_light: toward_light.normalize_or(Vec3::Y),
        light_color,
        illuminance,
        ambient_color,
        ambient_brightness: 180.0 + 220.0 * daylight,
        haze,
        elevation,
        daylight,
        sun_dir,
        moon_dir,
        moon_age,
        zenith,
        twilight,
        star_fade: star_fade_at(elevation),
    }
}

/// Cuánto del mapa de estrellas se ve con el sol a `elevation` radianes: nada
/// de día, apareciendo de a poco desde la puesta del sol hasta el final del
/// crepúsculo náutico (unos 18 grados bajo el horizonte), y completo una vez
/// que el cielo está oscuro.
pub fn star_fade_at(elevation: f32) -> f32 {
    1.0 - smoothstep(-0.32, -0.06, elevation)
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    let (a, b) = (a.to_srgba(), b.to_srgba());
    Color::srgb(
        a.red + (b.red - a.red) * t,
        a.green + (b.green - a.green) * t,
        a.blue + (b.blue - a.blue) * t,
    )
}

/// Cómo cubren la vista las cascadas de sombra del sol: tres cascadas sobre los
/// [`SHADOW_DISTANCE`] metros más cercanos (unos 12, 35 y 100 m), la primera
/// terminando en [`FIRST_CASCADE`]. Con la primera en 8 m, el jinete a los
/// 9.5 m por defecto de la cámara orbital caía en la segunda cascada, de unos 8
/// a 28 m de profundidad y por eso cerca de un tercio de nítida; a 12 m, el
/// jinete, el caballo y su sombra en el suelo junto a ellos quedan en la
/// nítida. Lo que está a menos de 8 m pierde algo de nitidez, pero la cámara
/// rara vez lo mira. Las cascadas vecinas se mezclan a lo largo de
/// [`CASCADE_OVERLAP`] en lugar del 0.2 de Bevy, lo que le deja a cada cascada
/// un poco más de su mapa.
///
/// El shadow map conserva los 2048 texels por cascada que usa Bevy por defecto
/// (unos 48 MB de profundidad para tres). 4096 lo haría más nítido, pero
/// costaría cuatro veces la memoria y la tasa de relleno de la pasada de
/// sombras, que la memoria compartida de la iGPU Renoir no puede ceder. Los
/// biases de profundidad y de normal se mantienen en los valores por defecto de
/// Bevy (0.02 y 1.8 texels); el bias de normal escala con el tamaño del texel,
/// así que acompaña por sí solo el cambio de cascada.
pub fn sun_cascades() -> CascadeShadowConfig {
    cascades_from(FIRST_CASCADE)
}

/// Las cascadas del sol para una cámara en primera persona: las mismas tres
/// sobre la misma [`SHADOW_DISTANCE`], la primera terminando en
/// [`FIRST_PERSON_CASCADE`], para que las manos, las riendas, el cuello del
/// caballo y el suelo un paso adelante reciban sus texels en lugar de los 12 m
/// de la cámara orbital.
pub fn first_person_cascades() -> CascadeShadowConfig {
    cascades_from(FIRST_PERSON_CASCADE)
}

fn cascades_from(first: f32) -> CascadeShadowConfig {
    CascadeShadowConfigBuilder {
        num_cascades: 3,
        first_cascade_far_bound: first,
        maximum_distance: SHADOW_DISTANCE,
        overlap_proportion: CASCADE_OVERLAP,
        ..default()
    }
    .build()
}

/// Límite lejano de la cascada de sombra más cercana, en metros.
pub const FIRST_CASCADE: f32 = 12.0;

/// Límite lejano de la cascada de sombra más cercana vista desde los ojos del
/// jugador, en metros.
pub const FIRST_PERSON_CASCADE: f32 = 6.0;

/// Cuánto de cada cascada se superpone con la anterior, para mezclar la
/// costura.
pub const CASCADE_OVERLAP: f32 = 0.15;

/// Hasta qué distancia de la cámara proyectan sombra las cosas, en metros (la
/// niebla de polvo oculta casi todo el detalle más allá).
pub const SHADOW_DISTANCE: f32 = 100.0;

fn spawn_sun(mut commands: Commands) {
    let sky = sky_at(DEFAULT_HOUR);
    commands.spawn((
        Name::new("Sun"),
        Sun,
        DirectionalLight {
            color: sky.light_color,
            illuminance: sky.illuminance,
            shadow_maps_enabled: true,
            ..default()
        },
        sun_cascades(),
        // Ambas capas: el propio sombrero del jugador proyecta su sombra aun
        // cuando una cámara en primera persona no lo dibuja.
        world_layers(),
        Transform::from_translation(sky.toward_light * 100.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

/// Ilumina el mundo según la hora del reloj: el sol (o la luna), la luz
/// ambiente, el color de fondo y el color de la niebla de cada cámara.
fn follow_the_clock(
    clock: Option<Res<WorldClock>>,
    mut sun: Query<(&mut DirectionalLight, &mut Transform), With<Sun>>,
    mut ambient: ResMut<GlobalAmbientLight>,
    environment: Option<Res<EnvironmentShare>>,
    lit: Query<&EnvironmentMapLight, With<Camera3d>>,
    mut clear: ResMut<ClearColor>,
    mut fogs: Query<&mut DistanceFog>,
) {
    let Some(clock) = clock else {
        return;
    };
    let sky = sky_with_moon(clock.0.hour(), astro::moon_age(clock_days(&clock.0)));
    for (mut light, mut transform) in &mut sun {
        light.color = sky.light_color;
        light.illuminance = sky.illuminance;
        *transform =
            Transform::from_translation(sky.toward_light * 100.0).looking_at(Vec3::ZERO, Vec3::Y);
    }
    ambient.color = sky.ambient_color;
    // El ambiente plano cede su parte solo mientras una cámara tiene realmente
    // la luz del entorno (es decir, no antes de que llegue, ni donde el filtro
    // de la GPU no está disponible y nunca llega).
    let in_effect = lit.iter().any(|light| light.intensity > 0.0);
    let share = environment
        .filter(|_| in_effect)
        .map_or(0.0, |share| share.0);
    ambient.brightness = sky.ambient_brightness * (1.0 - share);
    clear.0 = sky.haze;
    for mut fog in &mut fogs {
        *fog = haze_fog(&sky);
    }
}

/// Días de juego transcurridos desde que empezó el día 0, con fracciones.
pub fn clock_days(clock: &GameClock) -> f64 {
    clock.elapsed() / pampa_core::DAY_SECONDS
}

/// Le da al sol las cascadas que pide [`CloseShadows`], cuando cambia.
fn fit_cascades(close: Res<CloseShadows>, mut sun: Query<&mut CascadeShadowConfig, With<Sun>>) {
    if !close.is_changed() {
        return;
    }
    let cascades = if close.0 {
        first_person_cascades()
    } else {
        sun_cascades()
    };
    for mut config in &mut sun {
        *config = cascades.clone();
    }
}

/// Distancia a la que la niebla de polvo deja cerca de un 5% de contraste, en
/// metros.
///
/// El terreno se carga por streaming en chunks: más allá de
/// [`crate::TerrainPlugin::guaranteed_reach`] (400 m con el radio por defecto
/// de 3 chunks y chunks de 200 m) puede que el suelo todavía no exista, así que
/// la niebla debe tragarse todo bastante antes. El color de fondo es el color
/// de la niebla, así que el suelo que falta más allá se lee como bruma.
pub const FOG_VISIBILITY: f32 = 180.0;

/// Niebla de polvo: tenue cerca del jinete, se traga el horizonte. Esta es la
/// niebla diurna; el reloj la tiñe según la hora (ver [`sky_at`]).
pub fn dust_fog() -> DistanceFog {
    haze_fog(&sky_at(DEFAULT_HOUR))
}

/// La niebla para `sky`: siempre de [`FOG_VISIBILITY`] de profundidad, con el
/// color de su bruma.
fn haze_fog(sky: &Sky) -> DistanceFog {
    let haze = sky.haze.to_srgba();
    let shade = |k: f32| Color::srgb(haze.red * k, haze.green * k, haze.blue * k);
    DistanceFog {
        color: sky.haze,
        directional_light_color: sky.light_color.with_alpha(0.35 * sky.daylight),
        directional_light_exponent: 20.0,
        falloff: FogFalloff::from_visibility_colors(FOG_VISIBILITY, shade(0.91), shade(1.07)),
    }
}

fn add_dust_fog(
    mut commands: Commands,
    cameras: Query<Entity, (With<Camera3d>, Without<DistanceFog>)>,
) {
    for camera in &cameras {
        commands.entity(camera).insert(dust_fog());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pampa_core::TimeOfDay;

    fn luminance(c: Color) -> f32 {
        let c = c.to_srgba();
        0.2126 * c.red + 0.7152 * c.green + 0.0722 * c.blue
    }

    fn warmth(c: Color) -> f32 {
        let c = c.to_srgba();
        c.red / c.blue.max(1e-3)
    }

    #[test]
    fn the_sun_rises_east_crosses_north_and_sets_west() {
        let morning = sky_at(8.0);
        assert!(morning.toward_light.x > 0.5, "{morning:?}");
        let noon = sky_at(13.0);
        assert!(noon.toward_light.y > 0.85);
        // El norte en el suelo es -Z en Bevy.
        assert!(noon.toward_light.z < 0.0);
        let evening = sky_at(18.0);
        assert!(evening.toward_light.x < -0.5);
        assert!(sky_at(SUNRISE).elevation.abs() < 1e-4);
        assert!(sky_at(SUNSET).elevation.abs() < 1e-4);
    }

    #[test]
    fn full_light_by_day_and_warm_at_dawn_and_dusk() {
        let noon = sky_at(13.0);
        assert!((noon.illuminance - SUN_LUX).abs() < 1.0);
        assert_eq!(noon.daylight, 1.0);
        for low in [7.0, 19.0] {
            let sky = sky_at(low);
            assert!(sky.illuminance < noon.illuminance, "{low}");
            assert!(
                warmth(sky.light_color) > warmth(noon.light_color) + 0.3,
                "{low}"
            );
        }
    }

    #[test]
    fn night_is_dim_but_never_pitch_black() {
        let noon = sky_at(13.0);
        for hour in [22.0, 0.0, 3.0, 5.0] {
            let night = sky_at(hour);
            assert!(night.elevation < 0.0, "{hour}");
            assert!(night.illuminance < noon.illuminance / 5.0, "{hour}");
            assert!(night.ambient_brightness >= 180.0, "{hour}: moonlit ambient");
            assert!(night.toward_light.y > 0.0, "{hour}: the moon is up");
            // Luz de luna azulada.
            assert!(warmth(night.ambient_color) < 1.0, "{hour}");
        }
        let midnight = sky_at(0.0);
        assert!(midnight.illuminance > 500.0);
        assert!(luminance(midnight.haze) > 0.1, "the haze is never black");
    }

    #[test]
    fn the_light_changes_smoothly_through_the_day() {
        let mut last = sky_at(0.0);
        for k in 1..=24 * 60 {
            let sky = sky_at(k as f32 / 60.0);
            assert!((sky.illuminance - last.illuminance).abs() < 200.0, "{k}");
            assert!((sky.ambient_brightness - last.ambient_brightness).abs() < 10.0);
            assert!((luminance(sky.haze) - luminance(last.haze)).abs() < 0.03);
            last = sky;
        }
    }

    #[test]
    fn the_fog_keeps_its_depth_at_every_hour() {
        for hour in [0.0, 6.5, 13.0, 19.5, 22.0] {
            let fog = haze_fog(&sky_at(hour));
            assert_eq!(fog.color, sky_at(hour).haze);
            let expected =
                FogFalloff::from_visibility_colors(FOG_VISIBILITY, Color::BLACK, Color::BLACK);
            // La misma profundidad sea cual sea el tinte: se compara el tipo de
            // caída.
            assert_eq!(
                std::mem::discriminant(&fog.falloff),
                std::mem::discriminant(&expected)
            );
        }
    }

    #[test]
    fn the_sun_follows_the_world_clock() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(WorldClock(GameClock::at(TimeOfDay::hm(13, 0))))
            .add_plugins(AtmospherePlugin);
        app.update();
        let noon = sky_at(13.0);
        let mut q = app
            .world_mut()
            .query_filtered::<(&DirectionalLight, &Transform), With<Sun>>();
        let (light, transform) = q.single(app.world()).unwrap();
        assert_eq!(light.illuminance, noon.illuminance);
        assert!(light.shadow_maps_enabled);
        assert!(transform.translation.normalize().dot(noon.toward_light) > 0.999);
        assert_eq!(app.world().resource::<ClearColor>().0, noon.haze);

        app.world_mut().resource_mut::<WorldClock>().0 = GameClock::at(TimeOfDay::hm(1, 0));
        app.update();
        let ambient = app.world().resource::<GlobalAmbientLight>().brightness;
        assert_eq!(ambient, sky_at(1.0).ambient_brightness);
    }

    #[test]
    fn an_environment_map_takes_its_share_of_the_ambient_budget() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .insert_resource(WorldClock(GameClock::at(TimeOfDay::hm(1, 0))))
            .insert_resource(EnvironmentShare(0.6))
            .add_plugins(AtmospherePlugin);
        app.world_mut().spawn((
            Camera3d::default(),
            bevy::light::EnvironmentMapLight {
                intensity: 50.0,
                ..default()
            },
        ));
        app.update();
        let night = sky_at(1.0);
        let flat = app.world().resource::<GlobalAmbientLight>().brightness;
        // La parte plana más la del entorno es el presupuesto, así que el piso
        // a la luz de la luna se mantiene.
        assert!((flat - night.ambient_brightness * 0.4).abs() < 1e-3);
        assert!(flat + night.ambient_brightness * 0.6 >= 180.0);
    }

    fn ambient_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .insert_resource(WorldClock(GameClock::at(TimeOfDay::hm(1, 0))))
            .insert_resource(EnvironmentShare(0.6))
            .add_plugins(AtmospherePlugin);
        app
    }

    #[test]
    fn the_flat_ambient_keeps_the_whole_budget_until_an_environment_light_is_in_effect() {
        use bevy::light::EnvironmentMapLight;
        let budget = sky_at(1.0).ambient_brightness;
        let flat = |app: &App| app.world().resource::<GlobalAmbientLight>().brightness;

        // Sin cámara, una cámara sin la luz filtrada, una cuya luz es oscura.
        let mut app = ambient_app();
        app.update();
        assert!((flat(&app) - budget).abs() < 1e-3, "no camera");
        let camera = app.world_mut().spawn(Camera3d::default()).id();
        app.update();
        assert!((flat(&app) - budget).abs() < 1e-3, "no environment light");
        app.world_mut()
            .entity_mut(camera)
            .insert(EnvironmentMapLight {
                intensity: 0.0,
                ..default()
            });
        app.update();
        assert!((flat(&app) - budget).abs() < 1e-3, "intensity 0");

        // La luz está en efecto: el ambiente plano cede su parte.
        app.world_mut()
            .entity_mut(camera)
            .insert(EnvironmentMapLight {
                intensity: 50.0,
                ..default()
            });
        app.update();
        assert!((flat(&app) - budget * 0.4).abs() < 1e-3, "light in effect");
    }

    #[test]
    fn sun_shadows_are_crisp_near_the_rider() {
        let cascades = sun_cascades();
        assert_eq!(cascades.bounds.len(), 3);
        // La primera cascada llega más allá del jinete a los 9.5 m por defecto
        // de la cámara orbital, con margen para su sombra con el sol bajo.
        assert!(
            (10.0..=12.0).contains(&cascades.bounds[0]),
            "{:?}",
            cascades.bounds
        );
        assert_eq!(cascades.bounds[0], FIRST_CASCADE);
        assert!((cascades.bounds[2] - SHADOW_DISTANCE).abs() < 1e-3);
        assert!(cascades.bounds[1] > cascades.bounds[0] && cascades.bounds[1] < cascades.bounds[2]);
        assert_eq!(cascades.overlap_proportion, CASCADE_OVERLAP);
        assert!(
            CASCADE_OVERLAP < CascadeShadowConfigBuilder::default().overlap_proportion,
            "tighter than Bevy's default overlap"
        );
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(AtmospherePlugin);
        app.update();
        let mut q = app
            .world_mut()
            .query_filtered::<&CascadeShadowConfig, With<Sun>>();
        assert_eq!(q.single(app.world()).unwrap().bounds, cascades.bounds);
    }

    #[test]
    fn first_person_shortens_the_first_cascade_and_back() {
        let close = first_person_cascades();
        assert_eq!(close.bounds[0], FIRST_PERSON_CASCADE);
        const { assert!(FIRST_PERSON_CASCADE < FIRST_CASCADE) };
        assert!((close.bounds[2] - SHADOW_DISTANCE).abs() < 1e-3);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(AtmospherePlugin);
        app.update();
        let first = |app: &mut App| {
            let mut q = app
                .world_mut()
                .query_filtered::<&CascadeShadowConfig, With<Sun>>();
            q.single(app.world()).unwrap().bounds[0]
        };
        assert_eq!(first(&mut app), FIRST_CASCADE);
        app.insert_resource(CloseShadows(true));
        app.update();
        assert_eq!(first(&mut app), FIRST_PERSON_CASCADE);
        app.insert_resource(CloseShadows(false));
        app.update();
        assert_eq!(first(&mut app), FIRST_CASCADE);
        // El sol ilumina también la propia cabeza del jugador, para su sombra.
        let mut q = app
            .world_mut()
            .query_filtered::<&bevy::camera::visibility::RenderLayers, With<Sun>>();
        let layers = q.single(app.world()).unwrap();
        use crate::character::viewer::{OWN_BODY_LAYER, OWN_HEAD_LAYER};
        use bevy::camera::visibility::RenderLayers;
        for own in [OWN_HEAD_LAYER, OWN_BODY_LAYER] {
            assert!(layers.intersects(&RenderLayers::layer(own)), "layer {own}");
        }
    }
}
