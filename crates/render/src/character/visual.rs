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
use super::sheet::{FRAME_SIZE, SheetKind, sheet_pixels};

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
}

/// Child entity that draws a [`CharacterVisual::Billboard`].
#[derive(Component, Debug, Clone, Copy)]
pub struct Billboard {
    pub sheet: SheetKind,
    pub view: ViewDirection,
}

/// Quad and per-view materials of one sheet.
struct SheetSprites {
    mesh: Handle<Mesh>,
    /// One material per view, all sharing the sheet but offset to a column.
    materials: [Handle<StandardMaterial>; DIRECTIONS],
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
            let mut sheet = Image::new(
                Extent3d {
                    width: (FRAME_SIZE * DIRECTIONS) as u32,
                    height: FRAME_SIZE as u32,
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
                commands.entity(entity).with_child((
                    Name::new("Billboard"),
                    Billboard {
                        sheet,
                        view: ViewDirection::AWAY,
                    },
                    Mesh3d(sprites.mesh.clone()),
                    MeshMaterial3d(sprites.materials[0].clone()),
                    Transform::from_xyz(0.0, sheet.sprite_size() / 2.0, 0.0),
                ));
            }
        }
    }
}

/// Vertical bob (amplitude in meters, frequency in Hz) per logical gait.
fn bob(sheet: SheetKind, animation: AnimationState) -> (f32, f32) {
    match (sheet, animation) {
        (_, AnimationState::Idle) => (0.0, 0.0),
        // People step quicker and bounce more than a walking horse.
        (SheetKind::Person(_), _) => (0.06, 2.2),
        (_, AnimationState::Walk) => (0.04, 2.0),
        (_, AnimationState::Gallop) => (0.14, 3.2),
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

    for (child_of, mut billboard, mut transform, mut material) in &mut billboards {
        let Ok((rider_transform, pose)) = riders.get(child_of.parent()) else {
            continue;
        };

        let Some(sheet) = sprites.0.get(&billboard.sheet) else {
            continue;
        };
        let view = ViewDirection::pick(pose.heading, camera_yaw);
        if billboard.view != view {
            billboard.view = view;
            material.0 = sheet.materials[view.index()].clone();
        }

        // Cylindrical billboard: turn around Y to face the camera, stay upright.
        let to_camera = camera.translation - rider_transform.translation;
        let yaw = to_camera.x.atan2(to_camera.z);
        transform.rotation = Quat::from_rotation_y(yaw);

        let (amplitude, frequency) = bob(billboard.sheet, pose.animation);
        // Per-entity offset so a crowd does not bob in step.
        let offset = child_of.parent().index_u32() as f32 * 0.37;
        let phase = (time.elapsed_secs() * frequency + offset) * std::f32::consts::TAU;
        transform.translation.y =
            billboard.sheet.sprite_size() / 2.0 + amplitude * phase.sin().abs();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::{FRAC_PI_2, PI};

    /// Headless app running only `update_billboards`, with stand-in sprite
    /// handles: no GPU, window or asset server is needed to pick a view.
    fn headless_app() -> (App, [Handle<StandardMaterial>; DIRECTIONS]) {
        let mut assets = Assets::<StandardMaterial>::default();
        let materials: [Handle<StandardMaterial>; DIRECTIONS] =
            std::array::from_fn(|_| assets.add(StandardMaterial::default()));

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
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
            Billboard {
                sheet: SheetKind::Rider,
                view: ViewDirection::AWAY,
            },
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
}
