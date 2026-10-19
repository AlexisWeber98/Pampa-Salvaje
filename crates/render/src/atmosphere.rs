//! Sky color, sun, ambient light and dust fog for the dry pampa.

use bevy::prelude::*;

/// Warm, dusty haze color shared by the sky and the distance fog.
pub const DUST_COLOR: Color = Color::srgb(0.82, 0.70, 0.52);

/// Sets up the sun, ambient light and sky, and adds dust fog to every 3D camera.
pub struct AtmospherePlugin;

impl Plugin for AtmospherePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(DUST_COLOR))
            .insert_resource(GlobalAmbientLight {
                color: Color::srgb(1.0, 0.88, 0.72),
                brightness: 400.0,
                ..default()
            })
            .add_systems(Startup, spawn_sun)
            .add_systems(PostUpdate, add_dust_fog);
    }
}

fn spawn_sun(mut commands: Commands) {
    // Low afternoon sun from the north-west, for long shadows and warm light.
    commands.spawn((
        Name::new("Sun"),
        DirectionalLight {
            color: Color::srgb(1.0, 0.86, 0.66),
            illuminance: 9_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(-60.0, 45.0, -40.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

/// Distance at which the dust fog leaves about 5% contrast, in meters.
///
/// Terrain is streamed in chunks: past
/// [`crate::TerrainPlugin::guaranteed_reach`] (400 m with the default
/// 3-chunk radius and 200 m chunks) the ground may not exist yet, so the fog
/// must swallow everything well before it. The clear color is the fog
/// color, so the missing ground beyond reads as haze.
pub const FOG_VISIBILITY: f32 = 180.0;

/// Dust fog: thin near the rider, swallowing the horizon.
pub fn dust_fog() -> DistanceFog {
    DistanceFog {
        color: DUST_COLOR,
        directional_light_color: Color::srgba(1.0, 0.85, 0.6, 0.35),
        directional_light_exponent: 20.0,
        falloff: FogFalloff::from_visibility_colors(
            FOG_VISIBILITY,
            Color::srgb(0.75, 0.62, 0.45),
            Color::srgb(0.88, 0.78, 0.60),
        ),
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
