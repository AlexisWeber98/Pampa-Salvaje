//! The sky: a dome that follows the camera and a shader that paints the
//! gradient, twilight, sun, moon with phases, stars, Milky Way, Southern
//! Cross and clouds from the game clock; see [`SkyPlugin`]. Birds fly in it
//! ([`birds`]).
//!
//! The dome is one inside-out sphere (720 triangles, one draw call) of
//! [`DOME_RADIUS`] meters, inside the cameras' 1000 m far plane, drawn
//! without fog: its horizon color is the fog and clear color
//! ([`crate::Sky::haze`]), so distant fogged ground meets it seamlessly. It
//! is recentred on the drawing camera every frame, so it never gets closer.

pub mod astro;
pub mod birds;

use crate::atmosphere::{DEFAULT_HOUR, Sky, WorldClock, clock_days, sky_with_moon};
use crate::character::viewer::world_layers;
use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::camera::visibility::NoFrustumCulling;
use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

/// Radius of the sky dome, in meters.
pub const DOME_RADIUS: f32 = 800.0;
/// The star map, under `assets/`: the "Stars + Milky Way" 8k map of Solar
/// System Scope (https://www.solarsystemscope.com/textures/, CC BY 4.0, see
/// `assets/textures/sky/CREDITS.md`), an equirectangular map in galactic
/// coordinates, baked to UASTC KTX2 with mips from the JPG next to it. It is
/// sampled sharply so the stars stay pin-points.
pub const STAR_MAP: &str = "textures/sky/stars_milky_way/stars_milky_way.ktx2";
/// How much of the sky the cumulus cover on an average day, about: a clear
/// dry-pampa sky with a few scattered clouds. The one knob for cloudiness
/// (see [`cloud_cover`] and `sky.wesl`).
pub const CLOUD_COVER: f32 = 0.12;
/// How fast the clouds drift, in noise units per real second.
pub const CLOUD_SPEED: f32 = 0.0015;
/// Where the clouds' wind blows toward on the ground plane (north-east, a
/// pampero's opposite): `(x, z)`, unit length.
pub const WIND: Vec2 = Vec2::new(0.8, -0.6);

/// Draws the sky dome with the clock and lets birds fly in it. Needs a
/// renderer; [`SkyLogic`] is the part that runs without one.
pub struct SkyPlugin;

impl Plugin for SkyPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "sky.wesl");
        app.add_plugins((MaterialPlugin::<SkyMaterial>::default(), SkyLogic));
    }
}

/// The dome's and the birds' systems, without the material renderer.
pub struct SkyLogic;

impl Plugin for SkyLogic {
    fn build(&self, app: &mut App) {
        app.add_plugins(birds::BirdsPlugin)
            .add_systems(Startup, spawn_dome)
            .add_systems(Update, follow_the_clock);
    }
}

/// Marks the sky dome.
#[derive(Component, Debug, Clone, Copy)]
pub struct SkyDome;

/// What the sky shader reads: see `sky.wesl`.
#[derive(ShaderType, Debug, Clone, Copy, Default, PartialEq)]
pub struct SkyParams {
    pub sun: Vec4,
    pub moon: Vec4,
    pub moon_light: Vec4,
    pub zenith: Vec4,
    pub horizon: Vec4,
    pub sun_color: Vec4,
    pub cloud_lit: Vec4,
    pub cloud_shade: Vec4,
    pub drift: Vec4,
    pub star_x: Vec4,
    pub star_y: Vec4,
    pub star_z: Vec4,
    pub center: Vec4,
}

/// The sky dome's material.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, Default)]
pub struct SkyMaterial {
    #[uniform(0)]
    pub params: SkyParams,
    /// The star map ([`STAR_MAP`]); black until it loads.
    #[texture(1)]
    #[sampler(2)]
    pub star_map: Option<Handle<Image>>,
}

impl Material for SkyMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://pampa_render/sky/sky.wesl".into()
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }
}

fn linear(color: Color) -> Vec3 {
    let c = color.to_linear();
    Vec3::new(c.red, c.green, c.blue)
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How much of the sky the cumulus cover on game day `days`: [`CLOUD_COVER`]
/// give or take 40%, a few more or fewer scattered clouds from day to day,
/// mostly open blue.
pub fn cloud_cover(days: f64) -> f32 {
    CLOUD_COVER * (1.0 + 0.4 * (days as f32 * 2.1 + 1.0).sin())
}

/// Where the clouds have drifted to after `seconds` of real time, in noise
/// units: they move along [`WIND`].
pub fn cloud_drift(seconds: f32) -> Vec2 {
    -WIND * CLOUD_SPEED * seconds
}

/// The sunlit and shaded colors of the clouds (linear): lit by the sun's
/// color, so they glow orange at sunset, and a dim moonlit grey at night.
pub fn cloud_colors(sky: &Sky) -> (Vec3, Vec3) {
    let sun = linear(sky.light_color);
    let sun_up = smoothstep(-0.12, 0.06, sky.elevation);
    let day = Vec3::new(0.92, 0.90, 0.88) * 0.5 + sun * 0.55;
    let night = Vec3::new(0.035, 0.045, 0.085);
    let lit = night.lerp(day * 1.05, sun_up);
    let day_shade = Vec3::new(0.30, 0.34, 0.42);
    let dusk_shade = Vec3::new(0.30, 0.17, 0.24);
    let shade_day = day_shade.lerp(dusk_shade, sky.twilight);
    let shade = (night * 0.5).lerp(shade_day, sun_up);
    (lit, shade)
}

/// Everything the shader needs for `sky`, the game clock at `days`, the
/// real time `seconds` and a camera at `camera`.
pub fn sky_params(sky: &Sky, days: f64, seconds: f32, camera: Vec3) -> SkyParams {
    let frame = astro::galactic_frame(days);
    let (lit, shade) = cloud_colors(sky);
    let drift = cloud_drift(seconds);
    let moon_by_day = 0.35 + 0.65 * (1.0 - sky.daylight);
    SkyParams {
        sun: sky.sun_dir.extend(smoothstep(-0.25, -0.02, sky.elevation)),
        moon: sky.moon_dir.extend(moon_by_day),
        moon_light: astro::moon_light_dir(sky.moon_dir, sky.moon_age)
            .extend(0.04 + 0.08 * (1.0 - sky.daylight)),
        zenith: linear(sky.zenith).extend(sky.star_fade),
        horizon: linear(sky.haze).extend(sky.twilight),
        sun_color: linear(sky.light_color).extend(sky.daylight),
        cloud_lit: lit.extend(cloud_cover(days)),
        cloud_shade: shade.extend(seconds),
        drift: Vec4::new(drift.x, drift.y, WIND.x, WIND.y),
        star_x: frame[0],
        star_y: frame[1],
        star_z: frame[2],
        center: camera.extend(0.0),
    }
}

/// The inside-out unit sphere of the dome.
fn dome_mesh() -> Mesh {
    Sphere::new(1.0)
        .mesh()
        .uv(24, 16)
        .with_inverted_winding()
        .expect("a UV sphere is a triangle list")
}

/// Loads [`STAR_MAP`]: longitude wraps, latitude does not, filtered
/// linearly through its mips. Without an asset server (tests) the sky has
/// no stars.
fn load_star_map(server: Option<&AssetServer>) -> Option<Handle<Image>> {
    Some(
        server?
            .load_builder()
            .with_settings(|settings: &mut ImageLoaderSettings| {
                settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                    address_mode_u: ImageAddressMode::Repeat,
                    address_mode_v: ImageAddressMode::ClampToEdge,
                    // Near the galactic poles a pixel covers a long strip of
                    // the map: filter along it rather than smear the stars.
                    anisotropy_clamp: 16,
                    ..ImageSamplerDescriptor::linear()
                });
                // Only the GPU reads it: drop the CPU copy.
                settings.asset_usage = RenderAssetUsages::RENDER_WORLD;
            })
            .load(STAR_MAP),
    )
}

fn spawn_dome(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SkyMaterial>>,
    server: Option<Res<AssetServer>>,
) {
    commands.spawn((
        Name::new("Sky dome"),
        SkyDome,
        Mesh3d(meshes.add(dome_mesh())),
        MeshMaterial3d(materials.add(SkyMaterial {
            star_map: load_star_map(server.as_deref()),
            ..default()
        })),
        Transform::from_scale(Vec3::splat(DOME_RADIUS)),
        // The dome always surrounds the camera, so never cull it.
        NoFrustumCulling,
        NotShadowCaster,
        world_layers(),
    ));
}

/// The position of the camera drawn last: the game's, and the one the sky
/// and the birds are built round.
pub(crate) fn main_camera(cameras: &Query<(&GlobalTransform, &Camera), With<Camera3d>>) -> Vec3 {
    cameras
        .iter()
        .filter(|(_, camera)| camera.is_active)
        .max_by_key(|(_, camera)| camera.order)
        .map_or(Vec3::ZERO, |(transform, _)| transform.translation())
}

/// Moves the dome to the camera and paints it for the clock's hour.
fn follow_the_clock(
    clock: Option<Res<WorldClock>>,
    time: Res<Time<Real>>,
    cameras: Query<(&GlobalTransform, &Camera), With<Camera3d>>,
    mut dome: Query<(&mut Transform, &MeshMaterial3d<SkyMaterial>), With<SkyDome>>,
    mut materials: ResMut<Assets<SkyMaterial>>,
) {
    let camera = main_camera(&cameras);
    let (hour, days) = clock.map_or((DEFAULT_HOUR, 0.0), |c| (c.0.hour(), clock_days(&c.0)));
    let sky = sky_with_moon(hour, astro::moon_age(days));
    let params = sky_params(&sky, days, time.elapsed_secs(), camera);
    for (mut transform, material) in &mut dome {
        transform.translation = camera;
        if let Some(mut material) = materials.get_mut(&material.0) {
            material.params = params;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atmosphere::sky_at;
    use pampa_core::{GameClock, TimeOfDay};

    fn app_at(hour: u32) -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TransformPlugin))
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<SkyMaterial>>()
            .init_resource::<Assets<StandardMaterial>>()
            .insert_resource(WorldClock(GameClock::at(TimeOfDay::hm(hour, 0))))
            .add_plugins(SkyLogic);
        app
    }

    fn params(app: &mut App) -> SkyParams {
        let mut q = app
            .world_mut()
            .query_filtered::<&MeshMaterial3d<SkyMaterial>, With<SkyDome>>();
        let handle = q.single(app.world()).unwrap().0.clone();
        app.world()
            .resource::<Assets<SkyMaterial>>()
            .get(&handle)
            .unwrap()
            .params
    }

    #[test]
    fn the_dome_is_one_inside_out_sphere_round_the_camera() {
        let mut app = app_at(13);
        app.world_mut()
            .spawn((Camera3d::default(), Transform::from_xyz(40.0, 5.0, -70.0)));
        app.update();
        app.update();
        let mut q = app
            .world_mut()
            .query_filtered::<&Transform, With<SkyDome>>();
        let dome = q.single(app.world()).unwrap();
        assert_eq!(dome.scale, Vec3::splat(800.0));
        assert!(dome.translation.distance(Vec3::new(40.0, 5.0, -70.0)) < 1e-3);
        let mesh = dome_mesh();
        assert_eq!(mesh.count_vertices(), 25 * 17);
        // Inside out: every triangle's front face looks at the centre.
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap();
        let idx: Vec<usize> = mesh.indices().unwrap().iter().collect();
        assert_eq!(idx.len() / 3, 24 * 15 * 2);
        for tri in idx.chunks(3) {
            let p = |i: usize| Vec3::from(positions[tri[i]]);
            let normal = (p(1) - p(0)).cross(p(2) - p(0));
            if normal.length() > 1e-6 {
                assert!(normal.dot(p(0)) < 0.0, "{tri:?}");
            }
        }
    }

    #[test]
    fn the_sky_follows_the_clock() {
        let mut app = app_at(13);
        app.update();
        let noon = params(&mut app);
        let sky = sky_at(13.0);
        assert!(noon.sun.truncate().distance(sky.sun_dir) < 1e-4);
        assert_eq!(noon.zenith.w, 0.0, "no stars at noon");
        app.world_mut().resource_mut::<WorldClock>().0 = GameClock::at(TimeOfDay::hm(1, 0));
        app.update();
        let night = params(&mut app);
        assert_eq!(night.zenith.w, 1.0, "full stars at one in the morning");
        assert!(night.sun.y < 0.0 && night.moon.y > 0.0);
        assert!(night.zenith.truncate().length() < noon.zenith.truncate().length());
    }

    #[test]
    fn the_horizon_is_the_fog_color() {
        for hour in [0.0, 6.5, 13.0, 19.5, 22.0] {
            let sky = sky_at(hour);
            let p = sky_params(&sky, 0.0, 0.0, Vec3::ZERO);
            let fog = sky.haze.to_linear();
            assert_eq!(
                p.horizon.truncate(),
                Vec3::new(fog.red, fog.green, fog.blue)
            );
        }
    }

    #[test]
    fn clouds_drift_downwind_slowly() {
        assert_eq!(cloud_drift(0.0), Vec2::ZERO);
        let d = cloud_drift(100.0);
        assert!((d.length() - 0.15).abs() < 1e-5, "{d:?}");
        assert!(d.dot(WIND) < 0.0, "noise coordinates move against the wind");
        assert!((WIND.length() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn the_cover_is_a_few_scattered_clouds_in_open_blue() {
        assert_eq!(CLOUD_COVER, 0.12);
        let mut seen = (f32::MAX, f32::MIN);
        for k in 0..400 {
            let cover = cloud_cover(f64::from(k) * 0.1);
            assert!((0.07..=0.17).contains(&cover), "{cover}");
            seen = (seen.0.min(cover), seen.1.max(cover));
        }
        assert!(
            seen.1 - seen.0 > 0.05,
            "it varies from day to day: {seen:?}"
        );
        assert!(cloud_cover(0.0) != cloud_cover(1.5));
    }

    #[test]
    fn by_day_there_are_no_stars_and_the_zenith_is_a_bright_blue() {
        let lum = |c: Vec4| 0.2126 * c.x + 0.7152 * c.y + 0.0722 * c.z;
        for (h, m) in [(8, 0), (10, 53), (12, 0), (17, 0)] {
            let hour = h as f32 + m as f32 / 60.0;
            let mut app = app_at(h);
            app.world_mut().resource_mut::<WorldClock>().0 = GameClock::at(TimeOfDay::hm(h, m));
            app.update();
            app.update();
            let p = params(&mut app);
            assert_eq!(p.zenith.w, 0.0, "no stars at {hour}");
            assert_eq!(crate::atmosphere::star_fade_at(sky_at(hour).elevation), 0.0);
            assert!(p.sun.w > 0.99, "the sun is up at {hour}");
            assert!(
                p.zenith.z > p.zenith.x * 5.0,
                "blue overhead at {hour}: {:?}",
                p.zenith
            );
            assert!(lum(p.zenith) > 0.05, "bright at {hour}: {:?}", p.zenith);
            assert!(p.horizon.w < 0.2, "no twilight at {hour}");
        }
    }

    #[test]
    fn the_stars_come_out_after_dusk_and_go_before_dawn() {
        let fade = |hour: f32| crate::atmosphere::star_fade_at(sky_at(hour).elevation);
        for hour in [21.5, 23.0, 0.0, 3.0, 4.5] {
            assert_eq!(fade(hour), 1.0, "{hour}");
        }
        assert_eq!(fade(19.5 - 0.5), 0.0, "still day light at 19:00");
        assert!(fade(20.2) > 0.0 && fade(20.2) < 1.0, "fading in at dusk");
        assert!(fade(6.0) > 0.0 && fade(6.0) < 1.0, "fading out at dawn");
        let mut app = app_at(23);
        app.update();
        assert_eq!(params(&mut app).zenith.w, 1.0);
    }

    #[test]
    fn the_shader_sun_is_the_light_the_world_is_lit_by() {
        for hour in [8_u32, 11, 13, 17] {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, TransformPlugin))
                .init_resource::<Assets<Mesh>>()
                .init_resource::<Assets<SkyMaterial>>()
                .init_resource::<Assets<StandardMaterial>>()
                .insert_resource(WorldClock(GameClock::at(TimeOfDay::hm(hour, 0))))
                .add_plugins((crate::AtmospherePlugin, SkyLogic));
            app.update();
            app.update();
            let mut q = app
                .world_mut()
                .query_filtered::<&Transform, With<crate::Sun>>();
            let light = q.single(app.world()).unwrap().translation.normalize();
            let p = params(&mut app);
            assert!(
                p.sun.truncate().dot(light) > 0.999,
                "{hour}: {:?} vs {light:?}",
                p.sun
            );
        }
    }

    #[test]
    fn the_star_map_turns_with_the_game_clock() {
        let rows = |hour: u32| {
            let mut app = app_at(hour);
            app.update();
            let p = params(&mut app);
            (p.star_x, p.star_y, p.star_z)
        };
        let (a, b) = (rows(22), rows(23));
        assert_ne!(a.0, b.0);
        assert_ne!(a.1, b.1);
        assert_ne!(a.2, b.2);
        assert_eq!(rows(22), rows(22));
        let frame = astro::galactic_frame(22.0 / 24.0);
        assert_eq!((a.0, a.1, a.2), (frame[0], frame[1], frame[2]));
    }

    #[test]
    fn the_star_map_is_the_baked_texture_and_missing_assets_leave_the_sky_dark() {
        assert_eq!(
            STAR_MAP,
            "textures/sky/stars_milky_way/stars_milky_way.ktx2"
        );
        assert!(load_star_map(None).is_none());
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        assert!(root.join(STAR_MAP).is_file());
        assert!(
            root.join("textures/sky/stars_milky_way/stars_milky_way.jpg")
                .is_file()
        );
    }

    #[test]
    fn clouds_glow_at_sunset_and_go_dark_at_night() {
        let lum = |c: Vec3| 0.2126 * c.x + 0.7152 * c.y + 0.0722 * c.z;
        let (noon, _) = cloud_colors(&sky_at(13.0));
        let (dusk, dusk_shade) = cloud_colors(&sky_at(19.4));
        let (night, _) = cloud_colors(&sky_at(0.0));
        assert!(dusk.x > dusk.z * 1.4, "orange tops: {dusk:?}");
        assert!(dusk_shade.x > dusk_shade.z * 0.9, "warm undersides");
        assert!(lum(noon) > 0.5);
        assert!(lum(night) < 0.06, "{night:?}");
    }

    /// The bytes the GPU reads for binding 0, as floats.
    fn uniform_floats(params: &SkyParams) -> Vec<f32> {
        use bevy::render::render_resource::encase::UniformBuffer;
        let mut buffer = UniformBuffer::new(Vec::<u8>::new());
        buffer.write(params).unwrap();
        buffer
            .as_ref()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect()
    }

    #[test]
    fn the_material_uniform_holds_the_clocks_sky_in_the_shaders_layout() {
        // The shader declares the same thirteen vec4s, in this order.
        let shader = include_str!("sky.wesl");
        let fields = [
            "sun",
            "moon",
            "moon_light",
            "zenith",
            "horizon",
            "sun_color",
            "cloud_lit",
            "cloud_shade",
            "drift",
            "star_x",
            "star_y",
            "star_z",
            "center",
        ];
        let at = |f: &str| shader.find(&format!("{f}: vec4")).unwrap_or(usize::MAX);
        assert!(fields.windows(2).all(|w| at(w[0]) < at(w[1])), "{fields:?}");
        let (days, seconds, camera) = (3.0_f64 + 1.0 / 24.0, 12.5, Vec3::new(7.0, 3.0, -9.0));
        let sky = sky_with_moon(1.0, astro::moon_age(days));
        let material = SkyMaterial {
            params: sky_params(&sky, days, seconds, camera),
            star_map: None,
        };
        let floats = uniform_floats(&material.params);
        assert_eq!(floats.len(), fields.len() * 4);
        let row = |k: usize| Vec4::from_slice(&floats[k * 4..k * 4 + 4]);
        assert_eq!(row(0).truncate(), sky.sun_dir, "sun");
        assert_eq!(row(1).truncate(), sky.moon_dir, "moon");
        assert_eq!(row(3).w, 1.0, "full stars at one in the morning");
        assert_eq!(row(7).w, seconds, "the clock the clouds animate by");
        assert_eq!(row(12), camera.extend(0.0), "centre");
        assert_eq!(row(9), astro::galactic_frame(days)[0]);
        assert_eq!(row(2).truncate().length().round(), 1.0, "unit moon light");
    }
}
