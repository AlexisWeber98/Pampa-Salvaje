//! Per-entity rider visuals. Logic hands over a [`VisualPose`]; render decides
//! how it looks.

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::math::Affine2;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::transform::TransformSystems;
use pampa_core::{AnimationState, Heading};

use super::direction::{DIRECTIONS, ViewDirection, ground_yaw};
use super::sheet::{GAIT_FRAMES, SheetKind, sheet_pixels};

/// How a character is drawn. Swappable per entity without touching logic.
///
/// Only billboards exist today; a `Model` variant (low-poly 3D) is planned.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[require(VisualPose, Transform, Visibility)]
pub enum CharacterVisual {
    /// Camera-facing sprite picked from eight directional frames of a sheet.
    Billboard(SheetKind),
}

/// Logic state the visual needs, copied from `pampa-core` each frame by the game.
///
/// The entity's `Transform` must carry translation only (no rotation): the
/// continuous heading lives here, not in the transform.
#[derive(Component, Debug, Clone, Copy, PartialEq, Default)]
pub struct VisualPose {
    pub heading: Heading,
    pub animation: AnimationState,
    /// Ground speed in m/s; render paces the gait frames with it.
    pub speed: f32,
}

/// Child entity that draws a [`CharacterVisual::Billboard`].
#[derive(Component, Debug, Clone, Copy)]
pub struct Billboard {
    pub sheet: SheetKind,
    pub view: ViewDirection,
    /// Sheet row shown (see [`SheetKind::row`]).
    pub row: usize,
    /// How far through the current gait cycle, in cycles (`0..1`).
    pub phase: f32,
}

impl Billboard {
    pub fn new(sheet: SheetKind) -> Self {
        Self {
            sheet,
            view: ViewDirection::AWAY,
            row: 0,
            phase: 0.0,
        }
    }
}

/// Quad and per-frame materials of one sheet.
struct SheetSprites {
    mesh: Handle<Mesh>,
    /// One material per frame, `row * DIRECTIONS + view`, all sharing the
    /// sheet but offset to their cell.
    materials: Vec<Handle<StandardMaterial>>,
}

impl SheetSprites {
    fn material(&self, row: usize, view: ViewDirection) -> Option<&Handle<StandardMaterial>> {
        self.materials.get(row * DIRECTIONS + view.index())
    }
}

#[derive(Resource)]
struct BillboardSprites(HashMap<SheetKind, SheetSprites>);

/// Draws entities carrying a [`CharacterVisual`].
pub struct CharacterVisualPlugin;

impl Plugin for CharacterVisualPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_sprites).add_systems(
            PostUpdate,
            (attach_billboards, update_billboards)
                .chain()
                .before(TransformSystems::Propagate),
        );
    }
}

fn load_sprites(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let column = 1.0 / DIRECTIONS as f32;
    let sheets = SheetKind::all()
        .map(|kind| {
            let (width, height) = kind.sheet_size();
            let mut sheet = Image::new(
                Extent3d {
                    width: width as u32,
                    height: height as u32,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                sheet_pixels(kind),
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::RENDER_WORLD,
            );
            // Crisp pixel art, HD-2D style.
            sheet.sampler = ImageSampler::nearest();
            let sheet = images.add(sheet);
            let rows = kind.rows();
            let line = 1.0 / rows as f32;
            let materials = (0..rows * DIRECTIONS)
                .map(|cell| {
                    let (row, k) = (cell / DIRECTIONS, cell % DIRECTIONS);
                    materials.add(StandardMaterial {
                        base_color_texture: Some(sheet.clone()),
                        alpha_mode: AlphaMode::Mask(0.5),
                        // Sprites keep their painted shading; fog still applies.
                        unlit: true,
                        uv_transform: Affine2::from_scale_angle_translation(
                            Vec2::new(column, line),
                            0.0,
                            Vec2::new(k as f32 * column, row as f32 * line),
                        ),
                        ..default()
                    })
                })
                .collect();
            let size = kind.sprite_size();
            let mesh = meshes.add(Rectangle::new(size, size));
            (kind, SheetSprites { mesh, materials })
        })
        .collect();
    commands.insert_resource(BillboardSprites(sheets));
}

fn attach_billboards(
    mut commands: Commands,
    sprites: Res<BillboardSprites>,
    added: Query<(Entity, &CharacterVisual), Added<CharacterVisual>>,
) {
    for (entity, visual) in &added {
        match *visual {
            CharacterVisual::Billboard(sheet) => {
                let Some(sprites) = sprites.0.get(&sheet) else {
                    continue;
                };
                // Per-entity start so a crowd does not step in unison.
                let phase = (entity.index_u32() as f32 * 0.37).fract();
                commands.entity(entity).with_child((
                    Name::new("Billboard"),
                    Billboard {
                        phase,
                        ..Billboard::new(sheet)
                    },
                    Mesh3d(sprites.mesh.clone()),
                    MeshMaterial3d(sprites.materials[0].clone()),
                    Transform::from_xyz(0.0, sheet.sprite_size() / 2.0, 0.0),
                ));
            }
        }
    }
}

/// Gait cycles per second for `animation` at ground `speed` (m/s): one
/// cycle covers a fixed stride, so feet keep pace with the ground.
pub fn cadence(sheet: SheetKind, animation: AnimationState, speed: f32) -> f32 {
    let speed = if speed.is_finite() { speed.abs() } else { 0.0 };
    match (sheet, animation) {
        (_, AnimationState::Idle) => 0.0,
        // Two steps of about 0.65 m each.
        (SheetKind::Person(_), _) => speed.max(0.8) / 1.3,
        (_, AnimationState::Walk) => speed.max(1.0) / 1.8,
        (_, AnimationState::Gallop) => speed.max(5.0) / 4.2,
    }
}

/// Vertical bob amplitude in meters per logical gait, on top of the rise
/// already painted into the frames.
fn bob(sheet: SheetKind, animation: AnimationState) -> f32 {
    match (sheet, animation) {
        (_, AnimationState::Idle) => 0.0,
        (SheetKind::Person(_), _) => 0.03,
        (_, AnimationState::Walk) => 0.02,
        (_, AnimationState::Gallop) => 0.08,
    }
}

/// Character roots, disjoint from their billboard children.
type CharacterRoots = (With<CharacterVisual>, Without<Billboard>);

fn update_billboards(
    time: Res<Time>,
    sprites: Res<BillboardSprites>,
    camera: Single<&Transform, (With<Camera3d>, Without<Billboard>)>,
    riders: Query<(&Transform, &VisualPose), CharacterRoots>,
    mut billboards: Query<(
        &ChildOf,
        &mut Billboard,
        &mut Transform,
        &mut MeshMaterial3d<StandardMaterial>,
    )>,
) {
    let camera = *camera;
    let forward = camera.forward();
    let Some(camera_yaw) = ground_yaw(forward.x, forward.z) else {
        return;
    };

    for (child_of, mut billboard, mut transform, mut material_handle) in &mut billboards {
        let Ok((rider_transform, pose)) = riders.get(child_of.parent()) else {
            continue;
        };

        let Some(sheet) = sprites.0.get(&billboard.sheet) else {
            continue;
        };
        let cycles = cadence(billboard.sheet, pose.animation, pose.speed) * time.delta_secs();
        billboard.phase = (billboard.phase + cycles).fract();
        let frame = (billboard.phase * GAIT_FRAMES as f32) as usize;
        let row = billboard.sheet.row(pose.animation, frame);
        let view = ViewDirection::pick(pose.heading, camera_yaw);
        if billboard.view != view || billboard.row != row {
            billboard.view = view;
            billboard.row = row;
            if let Some(material) = sheet.material(row, view) {
                material_handle.0 = material.clone();
            }
        }

        // Cylindrical billboard: turn around Y to face the camera, stay upright.
        let to_camera = camera.translation - rider_transform.translation;
        let yaw = to_camera.x.atan2(to_camera.z);
        transform.rotation = Quat::from_rotation_y(yaw);

        // Two bumps per cycle, one per stride.
        let bump = (billboard.phase * std::f32::consts::TAU).sin().abs();
        transform.translation.y =
            billboard.sheet.sprite_size() / 2.0 + bob(billboard.sheet, pose.animation) * bump;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use std::f32::consts::{FRAC_PI_2, PI};
    use std::time::Duration;

    /// Headless app running only `update_billboards`, with stand-in sprite
    /// handles: no GPU, window or asset server is needed to pick a view.
    /// Frames last 50 ms.
    fn headless_app() -> (App, Vec<Handle<StandardMaterial>>) {
        let mut assets = Assets::<StandardMaterial>::default();
        let materials: Vec<Handle<StandardMaterial>> = (0..SheetKind::Rider.rows() * DIRECTIONS)
            .map(|_| assets.add(StandardMaterial::default()))
            .collect();

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                50,
            )))
            .insert_resource(BillboardSprites(HashMap::from([(
                SheetKind::Rider,
                SheetSprites {
                    mesh: Handle::default(),
                    materials: materials.clone(),
                },
            )])))
            .add_systems(Update, update_billboards);
        // Camera south of the origin looking north (core +y, Bevy -Z).
        app.world_mut().spawn((
            Camera3d::default(),
            Transform::from_xyz(0.0, 4.0, 9.0).looking_at(Vec3::ZERO, Vec3::Y),
        ));
        (app, materials)
    }

    fn spawn_rider(app: &mut App, heading: f32, first: &Handle<StandardMaterial>) -> Entity {
        let rider = app
            .world_mut()
            .spawn((
                VisualPose {
                    heading: Heading::from_radians(heading),
                    animation: AnimationState::Idle,
                    speed: 0.0,
                },
                Transform::default(),
            ))
            .id();
        // Insert the visual tag without triggering `attach_billboards`, which
        // is not scheduled here; the child mirrors what it would spawn.
        app.world_mut()
            .entity_mut(rider)
            .insert(CharacterVisual::Billboard(SheetKind::Rider));
        app.world_mut().spawn((
            Billboard::new(SheetKind::Rider),
            MeshMaterial3d(first.clone()),
            Transform::default(),
            ChildOf(rider),
        ));
        rider
    }

    fn billboard(app: &mut App) -> (ViewDirection, Handle<StandardMaterial>) {
        let mut query = app
            .world_mut()
            .query::<(&Billboard, &MeshMaterial3d<StandardMaterial>)>();
        let (billboard, material) = query.single(app.world()).expect("one billboard");
        (billboard.view, material.0.clone())
    }

    #[test]
    fn view_and_material_follow_heading_relative_to_camera() {
        let (mut app, materials) = headless_app();
        let rider = spawn_rider(&mut app, FRAC_PI_2, &materials[0]);

        app.update();
        assert_eq!(
            billboard(&mut app),
            (ViewDirection::AWAY, materials[0].clone())
        );

        // Turn to face south, toward the camera: front view.
        app.world_mut()
            .get_mut::<VisualPose>(rider)
            .unwrap()
            .heading = Heading::from_radians(3.0 * FRAC_PI_2);
        app.update();
        assert_eq!(
            billboard(&mut app),
            (ViewDirection::TOWARD, materials[4].clone())
        );

        // Facing west is screen-left for a camera looking north.
        app.world_mut()
            .get_mut::<VisualPose>(rider)
            .unwrap()
            .heading = Heading::from_radians(PI);
        app.update();
        assert_eq!(
            billboard(&mut app),
            (ViewDirection::LEFT, materials[2].clone())
        );
    }

    fn set_pose(app: &mut App, rider: Entity, animation: AnimationState, speed: f32) {
        let mut pose = app.world_mut().get_mut::<VisualPose>(rider).unwrap();
        pose.animation = animation;
        pose.speed = speed;
    }

    fn rows_seen(app: &mut App, frames: usize) -> Vec<usize> {
        let mut rows = Vec::new();
        for _ in 0..frames {
            app.update();
            let mut query = app.world_mut().query::<&Billboard>();
            rows.push(query.single(app.world()).unwrap().row);
        }
        rows
    }

    #[test]
    fn gait_frames_follow_the_animation_state_paced_by_speed() {
        let (mut app, materials) = headless_app();
        let rider = spawn_rider(&mut app, FRAC_PI_2, &materials[0]);
        assert!(
            rows_seen(&mut app, 20).iter().all(|&r| r == 0),
            "idle stands"
        );

        set_pose(&mut app, rider, AnimationState::Walk, 2.5);
        let walk = rows_seen(&mut app, 40);
        for row in 1..=GAIT_FRAMES {
            assert!(walk.contains(&row), "walk shows row {row}: {walk:?}");
        }
        assert!(walk.iter().all(|r| (1..=GAIT_FRAMES).contains(r)));
        // The material shows the cell of the current row and view.
        let (view, material) = billboard(&mut app);
        let row = *walk.last().unwrap();
        assert_eq!(material, materials[row * DIRECTIONS + view.index()]);

        set_pose(&mut app, rider, AnimationState::Gallop, 11.0);
        let gallop = rows_seen(&mut app, 40);
        assert!(
            gallop
                .iter()
                .all(|r| (GAIT_FRAMES + 1..=2 * GAIT_FRAMES).contains(r))
        );
        // Faster strides change frames more often than walking.
        let changes = |rows: &[usize]| rows.windows(2).filter(|w| w[0] != w[1]).count();
        assert!(changes(&gallop) > changes(&walk), "{gallop:?} vs {walk:?}");
    }

    #[test]
    fn cadence_scales_with_speed_and_stops_when_idle() {
        let rider = SheetKind::Rider;
        assert_eq!(cadence(rider, AnimationState::Idle, 3.0), 0.0);
        assert!(
            cadence(rider, AnimationState::Walk, 2.5) > cadence(rider, AnimationState::Walk, 1.5)
        );
        assert!(
            cadence(rider, AnimationState::Gallop, 11.0)
                > cadence(rider, AnimationState::Walk, 2.5)
        );
        assert!(
            cadence(rider, AnimationState::Walk, f32::NAN) > 0.0,
            "never stalls a gait"
        );
    }
}
