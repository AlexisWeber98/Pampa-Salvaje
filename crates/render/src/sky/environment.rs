//! Image-based ambient light from the sky: a small cubemap painted on the
//! CPU from the same [`Sky`] the dome shows (blue from above, warm at the
//! horizon at dusk, dark below), filtered on the GPU by Bevy's
//! [`GeneratedEnvironmentMapLight`] into a diffuse irradiance map and a
//! prefiltered specular chain, so shaded faces pick up the sky's colour and
//! glossy surfaces reflect it.
//!
//! Cost control: the cubemap is repainted, and the GPU filter run, only when
//! the sky has moved meaningfully ([`needs_refresh`]), never every frame.
//! Bevy re-filters every frame while a [`GeneratedEnvironmentMapLight`] is on
//! an entity, so the component is added to each camera and removed
//! [`FILTER_FRAMES`] frames after that camera has its filtered
//! [`EnvironmentMapLight`] (each camera counts its own [`FilterFrames`]); the
//! filtered light stays.

use crate::atmosphere::{EnvironmentShare, Sky, WorldClock, clock_days, sky_with_moon};
use crate::sky::astro;
use bevy::asset::RenderAssetUsages;
use bevy::light::{EnvironmentMapLight, GeneratedEnvironmentMapLight};
use bevy::prelude::*;
use bevy::render::render_resource::{
    Extent3d, TextureDimension, TextureFormat, TextureUsages, TextureViewDescriptor,
    TextureViewDimension,
};
use bevy::tasks::futures::check_ready;
use bevy::tasks::{AsyncComputeTaskPool, Task};

/// Cubemap face size in texels (power of two). 128 keeps the specular chain
/// cheap and is plenty for a smooth sky.
pub const FACE_SIZE: u32 = 128;
/// How much of the ambient budget ([`Sky::ambient_brightness`]) the sky's
/// image-based light carries; the rest stays a flat [`GlobalAmbientLight`].
pub const ENVIRONMENT_SHARE: f32 = 0.6;
/// Frames the GPU filter stays on after a repaint: the new texture upload and
/// the downsample and filter passes need a couple of frames to settle.
pub const FILTER_FRAMES: u8 = 3;
/// Repaint once the sun or the moon has moved this many degrees.
pub const ANGLE_STEP_DEGREES: f32 = 2.0;
/// Repaint once daylight or twilight has changed by this much.
pub const LIGHT_STEP: f32 = 0.03;

/// Paints the sky into a cubemap, lights the cameras with it and keeps it
/// current. Needs a renderer; headless apps use [`crate::sky::SkyLogic`].
pub struct SkyEnvironmentPlugin;

impl Plugin for SkyEnvironmentPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(EnvironmentShare(ENVIRONMENT_SHARE))
            .add_systems(Startup, create_environment)
            .add_systems(
                Update,
                (
                    attach_environment,
                    repaint_environment,
                    settle_filter,
                    follow_exposure,
                )
                    .chain(),
            );
    }
}

/// Frames the GPU filter still has to run for this camera, counted down once
/// the camera has its filtered light; at zero the generator is removed.
#[derive(Component, Debug, Clone, Copy)]
pub struct FilterFrames(u8);

/// Marks a camera lit by the sky's environment map.
#[derive(Component, Debug, Clone, Copy)]
pub struct SkyEnvironmentLight;

/// The sky cubemap, what it was painted from and the filter's state.
#[derive(Resource)]
pub struct SkyEnvironment {
    pub image: Handle<Image>,
    /// The sky the cubemap shows now.
    painted: Snapshot,
    /// The cubemap's mean luminance, to normalize its exposure.
    mean_luma: f32,
    /// The paint in flight, if any.
    pending: Option<Task<Painted>>,
    /// How many paints have been started.
    pub repaints: u32,
}

/// The part of a [`Sky`] that decides what the cubemap looks like.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Snapshot {
    sun_dir: Vec3,
    moon_dir: Vec3,
    daylight: f32,
    twilight: f32,
}

impl Snapshot {
    pub fn of(sky: &Sky) -> Self {
        Self {
            sun_dir: sky.sun_dir,
            moon_dir: sky.moon_dir,
            daylight: sky.daylight,
            twilight: sky.twilight,
        }
    }
}

/// Whether the sky has moved enough since `painted` to be worth a repaint.
pub fn needs_refresh(painted: &Snapshot, sky: &Sky) -> bool {
    let limit = ANGLE_STEP_DEGREES.to_radians().cos();
    painted.sun_dir.dot(sky.sun_dir) < limit
        || painted.moon_dir.dot(sky.moon_dir) < limit
        || (painted.daylight - sky.daylight).abs() > LIGHT_STEP
        || (painted.twilight - sky.twilight).abs() > LIGHT_STEP
}

fn linear(color: Color) -> Vec3 {
    let c = color.to_linear();
    Vec3::new(c.red, c.green, c.blue)
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The sky's radiance toward `dir` (unit, Bevy world space), linear and
/// relative: overhead zenith colour, haze at the horizon, a dim ground below,
/// the sun's glow and its dusk stain, a faint moon glow at night.
pub fn sky_radiance(sky: &Sky, dir: Vec3) -> Vec3 {
    let haze = linear(sky.haze);
    let zenith = linear(sky.zenith);
    let up = smoothstep(0.0, 0.7, dir.y.max(0.0)).sqrt();
    let mut color = haze.lerp(zenith, up);

    // Dusk: the horizon glows with the sun's colour toward the sun.
    let sun_cos = dir.dot(sky.sun_dir).max(0.0);
    let sun_up = smoothstep(-0.12, 0.06, sky.elevation);
    let sun_color = linear(sky.light_color);
    let horizon = (1.0 - dir.y.abs()).powi(4);
    color += sun_color * sun_cos.powi(4) * horizon * sky.twilight * 1.2;
    // A soft halo round the sun itself; the disc is the directional light's.
    color += sun_color * sun_cos.powi(24) * sun_up * 0.6;

    // The moon, only when it is dark.
    let moon_cos = dir.dot(sky.moon_dir).max(0.0);
    color += Vec3::new(0.55, 0.62, 0.8) * moon_cos.powi(48) * (1.0 - sky.daylight) * 0.08;

    // Below the horizon: the dry ground, a warm dim version of the haze.
    let ground = haze * Vec3::new(0.62, 0.5, 0.38) * 0.45;
    color.lerp(ground, smoothstep(0.0, -0.18, dir.y))
}

/// The world direction of texel `(x, y)` on cubemap `face` of a `size`-texel
/// face, in wgpu's cube layer order (+X, -X, +Y, -Y, +Z, -Z).
pub fn texel_direction(face: u32, x: u32, y: u32, size: u32) -> Vec3 {
    let u = (x as f32 + 0.5) / size as f32 * 2.0 - 1.0;
    let v = (y as f32 + 0.5) / size as f32 * 2.0 - 1.0;
    match face {
        0 => Vec3::new(1.0, -v, -u),
        1 => Vec3::new(-1.0, -v, u),
        2 => Vec3::new(u, 1.0, v),
        3 => Vec3::new(u, -1.0, -v),
        4 => Vec3::new(u, -v, 1.0),
        _ => Vec3::new(-u, -v, -1.0),
    }
    .normalize()
}

/// `value` as IEEE half-float bits: positive-range values are enough here,
/// tiny ones flush to zero and big ones clamp to the largest finite half.
pub fn f16_bits(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    let mantissa = bits & 0x007f_ffff;
    if value.is_nan() {
        return sign | 0x7e00;
    }
    if exp >= 31 {
        return sign | 0x7bff;
    }
    if exp <= 0 {
        return sign;
    }
    // Round to nearest by adding half the dropped bits.
    let rounded = (((exp as u32) << 10) | (mantissa >> 13)) + ((mantissa >> 12) & 1);
    sign | rounded.min(0x7bff) as u16
}

/// The cubemap's six faces for `sky` as `Rgba16Float` bytes, and its mean
/// luminance.
pub fn paint_cubemap(sky: &Sky, size: u32) -> (Vec<u8>, f32) {
    let mut bytes = Vec::with_capacity((6 * size * size * 8) as usize);
    let mut luma_sum = 0.0;
    for face in 0..6 {
        for y in 0..size {
            for x in 0..size {
                let c = sky_radiance(sky, texel_direction(face, x, y, size));
                luma_sum += c.dot(Vec3::new(0.2126, 0.7152, 0.0722));
                for channel in [c.x, c.y, c.z, 1.0] {
                    bytes.extend_from_slice(&f16_bits(channel).to_le_bytes());
                }
            }
        }
    }
    (bytes, luma_sum / (6 * size * size) as f32)
}

/// The environment light's intensity for `sky`: the share of the ambient
/// budget it carries, over the cubemap's mean luminance so the brightness
/// stays the sky's budget however bright the painted colours are.
pub fn environment_intensity(sky: &Sky, share: f32, mean_luma: f32) -> f32 {
    sky.ambient_brightness * share / mean_luma.max(1e-3)
}

fn current_sky(clock: &WorldClock) -> Sky {
    sky_with_moon(clock.0.hour(), astro::moon_age(clock_days(&clock.0)))
}

/// The environment light's intensity right now, from the clock (the default
/// hour without one), the share and the cubemap's mean luminance.
fn current_intensity(clock: Option<&WorldClock>, share: f32, mean_luma: f32) -> f32 {
    let sky = clock.map_or_else(
        || crate::atmosphere::sky_at(crate::atmosphere::DEFAULT_HOUR),
        current_sky,
    );
    environment_intensity(&sky, share, mean_luma)
}

/// The generator for a camera, carrying the current intensity from the first
/// frame (Bevy copies it into the filtered light when it creates it).
fn generator(env: &SkyEnvironment, intensity: f32) -> impl Bundle {
    (
        GeneratedEnvironmentMapLight {
            environment_map: env.image.clone(),
            intensity,
            ..default()
        },
        FilterFrames(FILTER_FRAMES),
    )
}

fn create_environment(
    mut commands: Commands,
    clock: Option<Res<WorldClock>>,
    mut images: ResMut<Assets<Image>>,
) {
    let sky = clock.as_deref().map_or_else(
        || crate::atmosphere::sky_at(crate::atmosphere::DEFAULT_HOUR),
        current_sky,
    );
    let (bytes, mean_luma) = paint_cubemap(&sky, FACE_SIZE);
    let mut image = Image::new(
        Extent3d {
            width: FACE_SIZE,
            height: FACE_SIZE,
            depth_or_array_layers: 6,
        },
        TextureDimension::D2,
        bytes,
        TextureFormat::Rgba16Float,
        RenderAssetUsages::all(),
    );
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });
    image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::STORAGE_BINDING | TextureUsages::COPY_SRC;
    commands.insert_resource(SkyEnvironment {
        image: images.add(image),
        painted: Snapshot::of(&sky),
        mean_luma,
        pending: None,
        repaints: 1,
    });
}

/// Lights every 3D camera with the sky's environment map.
fn attach_environment(
    mut commands: Commands,
    clock: Option<Res<WorldClock>>,
    share: Res<EnvironmentShare>,
    env: Option<Res<SkyEnvironment>>,
    cameras: Query<Entity, (With<Camera3d>, Without<SkyEnvironmentLight>)>,
) {
    let Some(env) = env else {
        return;
    };
    let intensity = current_intensity(clock.as_deref(), share.0, env.mean_luma);
    for camera in &cameras {
        commands
            .entity(camera)
            .insert((SkyEnvironmentLight, generator(&env, intensity)));
    }
}

/// A finished paint: the cubemap bytes and their mean luminance.
pub struct Painted {
    bytes: Vec<u8>,
    mean_luma: f32,
}

/// Repaints the cubemap when the sky has moved enough, and turns the GPU
/// filter back on for the cameras. The paint runs on the async compute pool,
/// one at a time: while one is pending a further change just waits, and is
/// picked up (the sky is compared again) the frame the paint lands.
fn repaint_environment(
    mut commands: Commands,
    clock: Option<Res<WorldClock>>,
    share: Res<EnvironmentShare>,
    env: Option<ResMut<SkyEnvironment>>,
    mut images: ResMut<Assets<Image>>,
    cameras: Query<
        Entity,
        (
            With<SkyEnvironmentLight>,
            Without<GeneratedEnvironmentMapLight>,
        ),
    >,
) {
    let (Some(clock), Some(mut env)) = (clock, env) else {
        return;
    };
    if let Some(task) = env.pending.as_mut() {
        let Some(done) = check_ready(task) else {
            return;
        };
        env.pending = None;
        if let Some(mut image) = images.get_mut(&env.image) {
            image.data = Some(done.bytes);
        }
        env.mean_luma = done.mean_luma;
        let intensity = current_intensity(Some(&clock), share.0, env.mean_luma);
        for camera in &cameras {
            commands.entity(camera).insert(generator(&env, intensity));
        }
    }
    let sky = current_sky(&clock);
    if !needs_refresh(&env.painted, &sky) {
        return;
    }
    env.pending = Some(AsyncComputeTaskPool::get().spawn(async move {
        let (bytes, mean_luma) = paint_cubemap(&sky, FACE_SIZE);
        Painted { bytes, mean_luma }
    }));
    env.painted = Snapshot::of(&sky);
    env.repaints += 1;
}

/// Counts each camera's filter frames down once it has its filtered light,
/// then removes its generator so Bevy stops re-filtering every frame.
fn settle_filter(
    mut commands: Commands,
    mut cameras: Query<(Entity, &mut FilterFrames), With<EnvironmentMapLight>>,
) {
    for (camera, mut frames) in &mut cameras {
        frames.0 = frames.0.saturating_sub(1);
        if frames.0 == 0 {
            commands
                .entity(camera)
                .remove::<(GeneratedEnvironmentMapLight, FilterFrames)>();
        }
    }
}

/// Keeps the environment light's intensity on the clock's ambient budget.
fn follow_exposure(
    clock: Option<Res<WorldClock>>,
    env: Option<Res<SkyEnvironment>>,
    share: Res<EnvironmentShare>,
    mut lights: Query<&mut EnvironmentMapLight, With<SkyEnvironmentLight>>,
) {
    let (Some(clock), Some(env)) = (clock, env) else {
        return;
    };
    let intensity = environment_intensity(&current_sky(&clock), share.0, env.mean_luma);
    for mut light in &mut lights {
        if light.intensity != intensity {
            light.intensity = intensity;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atmosphere::sky_at;
    use pampa_core::{GameClock, TimeOfDay};

    fn luma(c: Vec3) -> f32 {
        c.dot(Vec3::new(0.2126, 0.7152, 0.0722))
    }

    #[test]
    fn a_still_sky_does_not_repaint_but_a_moved_one_does() {
        let sky = sky_at(13.0);
        let painted = Snapshot::of(&sky);
        assert!(!needs_refresh(&painted, &sky));
        // Seconds of game time: the sun has hardly moved.
        assert!(!needs_refresh(&painted, &sky_at(13.0 + 1.0 / 120.0)));
        // Two hours: the sun has crossed tens of degrees.
        assert!(needs_refresh(&painted, &sky_at(15.0)));
        // Dusk changes daylight and twilight fast.
        assert!(needs_refresh(&Snapshot::of(&sky_at(19.0)), &sky_at(19.5)));
        // Light alone, with every direction still: each of its terms counts.
        let dimmer = Sky {
            daylight: sky.daylight - 0.1,
            ..sky
        };
        assert!(needs_refresh(&painted, &dimmer));
        let stained = Sky {
            twilight: sky.twilight + 0.1,
            ..sky
        };
        assert!(needs_refresh(&painted, &stained));
        let moved_moon = Sky {
            moon_dir: Vec3::new(sky.moon_dir.x, -sky.moon_dir.y, sky.moon_dir.z),
            ..sky
        };
        assert!(needs_refresh(&painted, &moved_moon));
    }

    #[test]
    fn the_sky_is_blue_above_warm_at_dusk_and_dark_below() {
        let noon = sky_at(13.0);
        let up = sky_radiance(&noon, Vec3::Y);
        let down = sky_radiance(&noon, Vec3::NEG_Y);
        assert!(up.z > up.x, "blue overhead: {up:?}");
        assert!(luma(up) > luma(down) * 1.5, "{up:?} vs {down:?}");

        // Sunset: the horizon toward the sun is warmer than the opposite one.
        let dusk = sky_at(19.0);
        let toward = Vec3::new(dusk.sun_dir.x, 0.0, dusk.sun_dir.z).normalize();
        let warm = sky_radiance(&dusk, toward);
        let cool = sky_radiance(&dusk, -toward);
        assert!(warm.x / warm.z > cool.x / cool.z, "{warm:?} vs {cool:?}");
        // The stain is the twilight's doing, not only the sun's halo.
        let plain = Sky {
            twilight: 0.0,
            ..dusk
        };
        assert!(warm.x > sky_radiance(&plain, toward).x, "{warm:?}");
    }

    #[test]
    fn the_cubemap_has_six_faces_and_follows_daylight() {
        let (day, day_mean) = paint_cubemap(&sky_at(13.0), 8);
        let (_, night_mean) = paint_cubemap(&sky_at(1.0), 8);
        assert_eq!(day.len(), 6 * 8 * 8 * 8);
        assert!(day_mean > 2.0 * night_mean, "{day_mean} vs {night_mean}");
        // Every direction lands on its own face's side.
        for face in 0..6 {
            let d = texel_direction(face, 4, 4, 8);
            let axis = [
                Vec3::X,
                Vec3::NEG_X,
                Vec3::Y,
                Vec3::NEG_Y,
                Vec3::Z,
                Vec3::NEG_Z,
            ];
            assert!(d.dot(axis[face as usize]) > 0.8, "face {face}: {d:?}");
        }
    }

    #[test]
    fn a_whole_day_repaints_a_bounded_number_of_times() {
        let mut painted = Snapshot::of(&sky_at(0.0));
        let mut repaints = 0;
        for minute in 0..24 * 60 {
            let sky = sky_at(minute as f32 / 60.0);
            if needs_refresh(&painted, &sky) {
                painted = Snapshot::of(&sky);
                repaints += 1;
            }
        }
        eprintln!("repaints per game day: {repaints}");
        // About one repaint per 5 game minutes (~10 s of play at 30x), nowhere near every frame.
        assert!((20..=300).contains(&repaints), "{repaints}");
    }

    #[test]
    fn half_floats_round_trip() {
        assert_eq!(f16_bits(1.0), 0x3c00);
        assert_eq!(f16_bits(0.5), 0x3800);
        assert_eq!(f16_bits(0.0), 0);
        assert_eq!(f16_bits(1e9), 0x7bff);
        assert_eq!(f16_bits(1e-9), 0);
    }

    #[test]
    fn the_intensity_is_the_budget_share_over_the_cubemaps_mean_luminance() {
        // Known numbers, not the formula run on its own output.
        let sky = Sky {
            ambient_brightness: 200.0,
            ..sky_at(13.0)
        };
        assert!((environment_intensity(&sky, 0.6, 0.5) - 240.0).abs() < 1e-3);
        assert!((environment_intensity(&sky, 0.5, 2.0) - 50.0).abs() < 1e-3);
        // A black cubemap must not blow the intensity up to infinity.
        assert!(environment_intensity(&sky, 0.6, 0.0).is_finite());

        // A sky of one known grey (no sun halo, no dusk, no moon): the painted
        // mean sits between the ground's luminance and the sky's 0.5.
        let grey = Color::linear_rgb(0.5, 0.5, 0.5);
        let flat = Sky {
            haze: grey,
            zenith: grey,
            twilight: 0.0,
            daylight: 1.0,
            elevation: -1.0,
            ..sky
        };
        let (_, mean) = paint_cubemap(&flat, 16);
        let ground = 0.5 * luma(Vec3::new(0.62, 0.5, 0.38)) * 0.45;
        assert!(mean < 0.5 && mean > ground, "{mean} vs {ground}");
        let intensity = environment_intensity(&flat, 0.6, mean);
        assert!(
            (intensity - 120.0 / mean).abs() < 1e-2 && intensity > 240.0,
            "{intensity}"
        );
    }

    fn app_at(hour: u32) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(AssetPlugin::default())
            .init_asset::<Image>()
            .insert_resource(WorldClock(GameClock::at(TimeOfDay::hm(hour, 0))))
            .add_plugins(SkyEnvironmentPlugin);
        app
    }

    #[test]
    fn cameras_get_the_environment_light_and_it_is_repainted_only_on_change() {
        let mut app = app_at(13);
        let camera = app.world_mut().spawn(Camera3d::default()).id();
        app.update();
        app.update();
        let world = app.world();
        assert!(world.get::<SkyEnvironmentLight>(camera).is_some());
        let generated = world.get::<GeneratedEnvironmentMapLight>(camera).unwrap();
        assert_eq!(
            generated.environment_map,
            world.resource::<SkyEnvironment>().image
        );
        assert_eq!(world.resource::<SkyEnvironment>().repaints, 1);

        // Many frames at the same hour: still one paint.
        for _ in 0..10 {
            app.update();
        }
        assert_eq!(app.world().resource::<SkyEnvironment>().repaints, 1);

        // The clock jumps: one repaint, and the generator is back on.
        app.world_mut().resource_mut::<WorldClock>().0 = GameClock::at(TimeOfDay::hm(19, 0));
        app.update();
        assert_eq!(app.world().resource::<SkyEnvironment>().repaints, 2);
    }

    #[test]
    fn the_generator_is_removed_once_the_filtered_light_exists() {
        let mut app = app_at(13);
        let camera = app.world_mut().spawn(Camera3d::default()).id();
        app.update();
        // What Bevy's generator does on the render side.
        app.world_mut()
            .entity_mut(camera)
            .insert(EnvironmentMapLight::default());
        for _ in 0..(FILTER_FRAMES as usize + 2) {
            app.update();
        }
        let world = app.world();
        assert!(world.get::<GeneratedEnvironmentMapLight>(camera).is_none());
        assert!(world.get::<EnvironmentMapLight>(camera).is_some());
    }

    /// Runs frames until the paint in flight lands (at most about 5 s).
    fn until_landed(app: &mut App) {
        for _ in 0..500 {
            app.update();
            if app.world().resource::<SkyEnvironment>().pending.is_none() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("the paint never landed");
    }

    fn image_bytes(app: &App) -> Vec<u8> {
        let env = app.world().resource::<SkyEnvironment>();
        app.world()
            .resource::<Assets<Image>>()
            .get(&env.image)
            .unwrap()
            .data
            .clone()
            .unwrap()
    }

    #[test]
    fn a_refresh_paints_off_the_main_thread_and_lands_later() {
        let mut app = app_at(13);
        let camera = app.world_mut().spawn(Camera3d::default()).id();
        app.update();
        // The first filter has finished: the generator is off.
        app.world_mut()
            .entity_mut(camera)
            .insert(EnvironmentMapLight::default());
        for _ in 0..(FILTER_FRAMES as usize + 2) {
            app.update();
        }
        assert!(
            app.world()
                .get::<GeneratedEnvironmentMapLight>(camera)
                .is_none()
        );
        let before = image_bytes(&app);

        app.world_mut().resource_mut::<WorldClock>().0 = GameClock::at(TimeOfDay::hm(19, 0));
        app.update();
        let env = app.world().resource::<SkyEnvironment>();
        assert_eq!(env.repaints, 2, "one paint task spawned");
        // The spawning frame changed nothing: the paint is in flight, or at
        // most landed on a later frame.
        assert!(env.pending.is_some());
        assert_eq!(image_bytes(&app), before);

        until_landed(&mut app);
        assert_ne!(image_bytes(&app), before, "the new sky is in the image");
        assert!(
            app.world()
                .get::<GeneratedEnvironmentMapLight>(camera)
                .is_some(),
            "the filter is back on once the paint lands"
        );
        assert_eq!(app.world().resource::<SkyEnvironment>().repaints, 2);
    }

    #[test]
    fn a_change_during_a_paint_waits_for_it_and_repaints_once_after() {
        let mut app = app_at(13);
        app.world_mut().spawn(Camera3d::default());
        app.update();
        // A paint that cannot land yet.
        app.world_mut().resource_mut::<SkyEnvironment>().pending =
            Some(AsyncComputeTaskPool::get().spawn(std::future::pending()));
        app.world_mut().resource_mut::<WorldClock>().0 = GameClock::at(TimeOfDay::hm(19, 0));
        for _ in 0..5 {
            app.update();
        }
        assert_eq!(
            app.world().resource::<SkyEnvironment>().repaints,
            1,
            "no second task while one is pending"
        );

        // It lands: the sky has moved since, so one more paint starts.
        app.world_mut().resource_mut::<SkyEnvironment>().pending =
            Some(AsyncComputeTaskPool::get().spawn(async {
                Painted {
                    bytes: vec![0; (6 * FACE_SIZE * FACE_SIZE * 8) as usize],
                    mean_luma: 1.0,
                }
            }));
        until_landed(&mut app);
        assert_eq!(app.world().resource::<SkyEnvironment>().repaints, 2);
    }

    /// What Bevy's generator does on the render side once it has the cubemap.
    fn give_filtered_light(app: &mut App, camera: Entity) {
        app.world_mut()
            .entity_mut(camera)
            .insert(EnvironmentMapLight::default());
    }

    #[test]
    fn a_camera_spawned_after_settling_loses_its_generator_too() {
        let mut app = app_at(13);
        let first = app.world_mut().spawn(Camera3d::default()).id();
        app.update();
        give_filtered_light(&mut app, first);
        for _ in 0..(FILTER_FRAMES as usize + 2) {
            app.update();
        }
        assert!(
            app.world()
                .get::<GeneratedEnvironmentMapLight>(first)
                .is_none()
        );

        // A camera that shows up later, with the sky frozen (no repaint).
        let late = app.world_mut().spawn(Camera3d::default()).id();
        app.update();
        assert!(
            app.world()
                .get::<GeneratedEnvironmentMapLight>(late)
                .is_some()
        );
        give_filtered_light(&mut app, late);
        for _ in 0..(FILTER_FRAMES as usize + 2) {
            app.update();
        }
        assert!(
            app.world()
                .get::<GeneratedEnvironmentMapLight>(late)
                .is_none(),
            "the late camera keeps the generator, so Bevy filters every frame"
        );
    }

    #[test]
    fn a_filtered_light_that_arrives_late_still_ends_the_generator() {
        let mut app = app_at(13);
        let first = app.world_mut().spawn(Camera3d::default()).id();
        let slow = app.world_mut().spawn(Camera3d::default()).id();
        app.update();
        give_filtered_light(&mut app, first);
        for _ in 0..(FILTER_FRAMES as usize + 2) {
            app.update();
        }
        // The second camera's light only shows up now.
        give_filtered_light(&mut app, slow);
        for _ in 0..(FILTER_FRAMES as usize + 2) {
            app.update();
        }
        assert!(
            app.world()
                .get::<GeneratedEnvironmentMapLight>(slow)
                .is_none()
        );
    }

    #[test]
    fn a_landed_paint_never_shows_a_zero_intensity() {
        let mut app = app_at(13);
        let camera = app.world_mut().spawn(Camera3d::default()).id();
        app.update();
        let expected = |app: &App| {
            let env = app.world().resource::<SkyEnvironment>();
            let clock = app.world().resource::<WorldClock>();
            environment_intensity(&current_sky(clock), ENVIRONMENT_SHARE, env.mean_luma)
        };
        // From the first frame the generator carries the real intensity.
        let generated = app
            .world()
            .get::<GeneratedEnvironmentMapLight>(camera)
            .unwrap();
        assert!(generated.intensity > 0.0);
        assert!((generated.intensity - expected(&app)).abs() < 1.0);

        give_filtered_light(&mut app, camera);
        for _ in 0..(FILTER_FRAMES as usize + 2) {
            app.update();
        }
        app.world_mut().resource_mut::<WorldClock>().0 = GameClock::at(TimeOfDay::hm(19, 0));
        app.update();
        until_landed(&mut app);
        let want = expected(&app);
        let generated = app
            .world()
            .get::<GeneratedEnvironmentMapLight>(camera)
            .unwrap();
        let light = app.world().get::<EnvironmentMapLight>(camera).unwrap();
        assert!(want > 0.0);
        assert!(
            (generated.intensity - want).abs() < 1.0,
            "{}",
            generated.intensity
        );
        assert!((light.intensity - want).abs() < 1.0, "{}", light.intensity);
    }
}
