//! Per-entity rider visuals. Logic hands over a [`VisualPose`]; render decides
//! how it looks.

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::math::Affine2;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::transform::TransformSystems;
use pampa_core::{AnimationState, Heading};

use super::direction::{DIRECTIONS, ViewDirection, ground_yaw};
use super::sheet::{FRAME_SIZE, rider_sheet_pixels};

/// How an entity is drawn. Swappable per entity without touching logic.
///
/// Only `Billboard` exists today; a `Model` variant (low-poly 3D) is planned.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[require(VisualPose, Transform, Visibility)]
pub enum RiderVisual {
    /// Camera-facing sprite picked from eight directional frames.
    #[default]
    Billboard,
}

/// Logic state the visual needs, copied from `pampa-core` each frame by the game.
///
/// The entity's `Transform` must carry translation only (no rotation): the
/// continuous heading lives here, not in the transform.
#[derive(Component, Debug, Clone, Copy, PartialEq, Default)]
pub struct VisualPose {
    pub heading: Heading,
    pub animation: AnimationState,
}

/// Child entity that draws a [`RiderVisual::Billboard`].
#[derive(Component, Debug, Clone, Copy)]
pub struct Billboard {
    pub view: ViewDirection,
}

/// Sprite height above the entity origin (the ground contact point), in meters.
const SPRITE_SIZE: f32 = 2.8;

#[derive(Resource)]
struct RiderSprites {
    mesh: Handle<Mesh>,
    /// One material per view, all sharing the sheet but offset to a column.
    materials: [Handle<StandardMaterial>; DIRECTIONS],
}

/// Draws entities carrying a [`RiderVisual`].
pub struct RiderVisualPlugin;

impl Plugin for RiderVisualPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_rider_sprites).add_systems(
            PostUpdate,
            (attach_billboards, update_billboards)
                .chain()
                .before(TransformSystems::Propagate),
        );
    }
}

fn load_rider_sprites(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut sheet = Image::new(
        Extent3d {
            width: (FRAME_SIZE * DIRECTIONS) as u32,
            height: FRAME_SIZE as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rider_sheet_pixels(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    // Crisp pixel art, HD-2D style.
    sheet.sampler = ImageSampler::nearest();
    let sheet = images.add(sheet);

    let column = 1.0 / DIRECTIONS as f32;
    let materials = std::array::from_fn(|k| {
        materials.add(StandardMaterial {
            base_color_texture: Some(sheet.clone()),
            alpha_mode: AlphaMode::Mask(0.5),
            // Sprites keep their painted shading; fog still applies.
            unlit: true,
            uv_transform: Affine2::from_scale_angle_translation(
                Vec2::new(column, 1.0),
                0.0,
                Vec2::new(k as f32 * column, 0.0),
            ),
            ..default()
        })
    });

    commands.insert_resource(RiderSprites {
        mesh: meshes.add(Rectangle::new(SPRITE_SIZE, SPRITE_SIZE)),
        materials,
    });
}

fn attach_billboards(
    mut commands: Commands,
    sprites: Res<RiderSprites>,
    added: Query<(Entity, &RiderVisual), Added<RiderVisual>>,
) {
    for (entity, visual) in &added {
        match visual {
            RiderVisual::Billboard => {
                commands.entity(entity).with_child((
                    Name::new("Rider billboard"),
                    Billboard {
                        view: ViewDirection::AWAY,
                    },
                    Mesh3d(sprites.mesh.clone()),
                    MeshMaterial3d(sprites.materials[0].clone()),
                    Transform::from_xyz(0.0, SPRITE_SIZE / 2.0, 0.0),
                ));
            }
        }
    }
}

/// Vertical bob (amplitude in meters, frequency in Hz) per logical gait.
fn bob(animation: AnimationState) -> (f32, f32) {
    match animation {
        AnimationState::Idle => (0.0, 0.0),
        AnimationState::Walk => (0.04, 2.0),
        AnimationState::Gallop => (0.14, 3.2),
    }
}

/// Rider roots, disjoint from their billboard children.
type RiderRoots = (With<RiderVisual>, Without<Billboard>);

fn update_billboards(
    time: Res<Time>,
    sprites: Res<RiderSprites>,
    camera: Single<&Transform, (With<Camera3d>, Without<Billboard>)>,
    riders: Query<(&Transform, &VisualPose), RiderRoots>,
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

    for (child_of, mut billboard, mut transform, mut material) in &mut billboards {
        let Ok((rider_transform, pose)) = riders.get(child_of.parent()) else {
            continue;
        };

        let view = ViewDirection::pick(pose.heading, camera_yaw);
        if billboard.view != view {
            billboard.view = view;
            material.0 = sprites.materials[view.index()].clone();
        }

        // Cylindrical billboard: turn around Y to face the camera, stay upright.
        let to_camera = camera.translation - rider_transform.translation;
        let yaw = to_camera.x.atan2(to_camera.z);
        transform.rotation = Quat::from_rotation_y(yaw);

        let (amplitude, frequency) = bob(pose.animation);
        let phase = time.elapsed_secs() * frequency * std::f32::consts::TAU;
        transform.translation.y = SPRITE_SIZE / 2.0 + amplitude * phase.sin().abs();
    }
}
