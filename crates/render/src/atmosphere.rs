//! Sky color, sun, ambient light and dust fog for the dry pampa, following
//! the game clock: a warm low sun at dawn and dusk, full light by day, and
//! a dim, bluish moonlit night that stays readable.

use bevy::light::{CascadeShadowConfig, CascadeShadowConfigBuilder};
use bevy::prelude::*;
use pampa_core::GameClock;

/// Warm, dusty haze color shared by the sky and the distance fog by day.
pub const DUST_COLOR: Color = Color::srgb(0.82, 0.70, 0.52);
/// Haze color on a moonlit night.
const NIGHT_HAZE: Color = Color::srgb(0.16, 0.18, 0.25);

/// The game clock. Logic state: the game runs it, render only reads it.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct WorldClock(pub GameClock);

/// Marks the directional light standing in for the sun (and the moon).
#[derive(Component, Debug, Clone, Copy)]
pub struct Sun;

/// Sets up the sun, ambient light and sky, adds dust fog to every 3D camera,
/// and moves the light with the [`WorldClock`] every frame. Without a clock
/// the light stays at [`DEFAULT_HOUR`].
pub struct AtmospherePlugin;

/// The hour lit when the world has no clock: late afternoon.
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
            .add_systems(Startup, spawn_sun)
            .add_systems(PostUpdate, (add_dust_fog, follow_the_clock).chain());
    }
}

/// Sunrise, in hours since midnight.
pub const SUNRISE: f32 = 6.5;
/// Sunset, in hours since midnight.
pub const SUNSET: f32 = 19.5;
/// The sun's height at noon, in radians (a pampa summer: about 65°).
const NOON_ELEVATION: f32 = 1.13;
/// Illuminance of the full sun, in lux.
const SUN_LUX: f32 = 10_000.0;
/// Illuminance of the moon, in lux: dim, but enough to read the ground.
const MOON_LUX: f32 = 900.0;

/// How the sky lights the world at one hour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sky {
    /// Unit direction from the ground toward the light, in Bevy world space.
    pub toward_light: Vec3,
    pub light_color: Color,
    pub illuminance: f32,
    pub ambient_color: Color,
    pub ambient_brightness: f32,
    /// Fog and clear color.
    pub haze: Color,
    /// The sun's height above the horizon, in radians (negative at night).
    pub elevation: f32,
    /// `0` at night, `1` in daylight.
    pub daylight: f32,
}

/// The sky at `hour` (hours since midnight, wrapped into a day).
///
/// The sun rises in the east at [`SUNRISE`], crosses the northern sky (the
/// pampa is south of the equator) and sets in the west at [`SUNSET`]. Once
/// it is well below the horizon, the light swaps to a fixed, bluish moon
/// high in the north-east; the swap happens while both are at zero.
pub fn sky_at(hour: f32) -> Sky {
    let hour = hour.rem_euclid(24.0);
    let day = (hour - SUNRISE) / (SUNSET - SUNRISE);
    // Past sunset the sun keeps sinking on the same arc, then comes back up
    // under the horizon toward sunrise.
    let night_length = 24.0 - (SUNSET - SUNRISE);
    let elevation = if (0.0..=1.0).contains(&day) {
        NOON_ELEVATION * (std::f32::consts::PI * day).sin()
    } else {
        let into = (hour - SUNSET).rem_euclid(24.0) / night_length;
        -0.6 * (std::f32::consts::PI * into).sin()
    };
    // East (azimuth 0) through north (π/2) to west (π), on the ground plane.
    let azimuth = std::f32::consts::PI * day.clamp(0.0, 1.0);
    let daylight = smoothstep(-0.05, 0.12, elevation);
    let low = 1.0 - smoothstep(0.05, 0.6, elevation);
    let night = 1.0 - smoothstep(-0.2, -0.04, elevation);

    let (toward_light, light_color, illuminance) = if elevation > -0.04 {
        let ground = Vec3::new(
            azimuth.cos() * elevation.cos(),
            elevation.sin(),
            -azimuth.sin() * elevation.cos(),
        );
        let warm = mix(
            Color::srgb(1.0, 0.95, 0.86),
            Color::srgb(1.0, 0.58, 0.32),
            low,
        );
        let lux = SUN_LUX * smoothstep(-0.04, 0.5, elevation);
        (ground, warm, lux)
    } else {
        // Moon high in the north-east.
        let moon = Vec3::new(0.35, 0.8, -0.5).normalize();
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
    let haze = mix(
        NIGHT_HAZE,
        mix(DUST_COLOR, Color::srgb(0.86, 0.60, 0.42), low * 0.6),
        daylight,
    );
    Sky {
        toward_light: toward_light.normalize_or(Vec3::Y),
        light_color,
        illuminance,
        ambient_color,
        ambient_brightness: 180.0 + 220.0 * daylight,
        haze,
        elevation,
        daylight,
    }
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

/// How the sun's shadow cascades cover the view: three cascades over the
/// nearest [`SHADOW_DISTANCE`] meters (about 12, 35 and 100 m), the first
/// ending at [`FIRST_CASCADE`]. With the first at 8 m, the rider at the
/// orbit camera's default 9.5 m fell into the second cascade, about 8 to
/// 28 m deep and so about a third as sharp; at 12 m the rider, the horse
/// and their shadow on the ground beside them are in the sharp one. Things
/// closer than 8 m lose a little sharpness, but the camera seldom looks at
/// them. Neighbouring cascades blend over [`CASCADE_OVERLAP`] rather than
/// Bevy's 0.2, which leaves each cascade a little more of its map.
///
/// The shadow map keeps Bevy's default 2048 texels per cascade (about
/// 48 MB of depth for three). 4096 would sharpen it further but cost four
/// times the memory and the shadow-pass fill rate, which the Renoir iGPU's
/// shared memory cannot spare. Depth and normal biases stay at Bevy's
/// defaults (0.02 and 1.8 texels); the normal bias scales with the texel
/// size, so it follows the cascade change by itself.
pub fn sun_cascades() -> CascadeShadowConfig {
    CascadeShadowConfigBuilder {
        num_cascades: 3,
        first_cascade_far_bound: FIRST_CASCADE,
        maximum_distance: SHADOW_DISTANCE,
        overlap_proportion: CASCADE_OVERLAP,
        ..default()
    }
    .build()
}

/// Far bound of the nearest shadow cascade, in meters.
pub const FIRST_CASCADE: f32 = 12.0;

/// How much of each cascade overlaps the one before, to blend the seam.
pub const CASCADE_OVERLAP: f32 = 0.15;

/// How far from the camera things cast shadows, in meters (the dust fog
/// hides most detail past it).
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
        Transform::from_translation(sky.toward_light * 100.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

/// Lights the world for the clock's hour: the sun (or moon), the ambient
/// light, the clear color and every camera's fog color.
fn follow_the_clock(
    clock: Option<Res<WorldClock>>,
    mut sun: Query<(&mut DirectionalLight, &mut Transform), With<Sun>>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut clear: ResMut<ClearColor>,
    mut fogs: Query<&mut DistanceFog>,
) {
    let Some(clock) = clock else {
        return;
    };
    let sky = sky_at(clock.0.hour());
    for (mut light, mut transform) in &mut sun {
        light.color = sky.light_color;
        light.illuminance = sky.illuminance;
        *transform =
            Transform::from_translation(sky.toward_light * 100.0).looking_at(Vec3::ZERO, Vec3::Y);
    }
    ambient.color = sky.ambient_color;
    ambient.brightness = sky.ambient_brightness;
    clear.0 = sky.haze;
    for mut fog in &mut fogs {
        *fog = haze_fog(&sky);
    }
}

/// Distance at which the dust fog leaves about 5% contrast, in meters.
///
/// Terrain is streamed in chunks: past
/// [`crate::TerrainPlugin::guaranteed_reach`] (400 m with the default
/// 3-chunk radius and 200 m chunks) the ground may not exist yet, so the fog
/// must swallow everything well before it. The clear color is the fog
/// color, so the missing ground beyond reads as haze.
pub const FOG_VISIBILITY: f32 = 180.0;

/// Dust fog: thin near the rider, swallowing the horizon. This is the
/// daytime fog; the clock tints it with the hour (see [`sky_at`]).
pub fn dust_fog() -> DistanceFog {
    haze_fog(&sky_at(DEFAULT_HOUR))
}

/// The fog for `sky`: always [`FOG_VISIBILITY`] deep, colored like its haze.
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
        // North on the ground is Bevy -Z.
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
            // Bluish moonlight.
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
            // Same depth whatever the tint: compare the falloff's kind.
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
    fn sun_shadows_are_crisp_near_the_rider() {
        let cascades = sun_cascades();
        assert_eq!(cascades.bounds.len(), 3);
        // The first cascade reaches past the rider at the orbit camera's
        // default 9.5 m, with room for their shadow on a low sun.
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
}
