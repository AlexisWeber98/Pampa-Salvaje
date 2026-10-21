//! Per-entity character visuals. Logic hands over a [`VisualPose`]; render
//! decides how it looks: people are low-poly 3D figures (see
//! [`figure`](super::figure)), horses and the mounted rider are still
//! billboards.

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::math::Affine2;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::transform::TransformSystems;
use pampa_core::{AnimationState, Heading};

use super::direction::{DIRECTIONS, ViewDirection, ground_yaw};
use super::figure::{FigureKind, FigureKit, spawn_figure, update_figures};
use super::sheet::{GAIT_FRAMES, SheetKind, sheet_pixels};

/// How a character is drawn. Swappable per entity without touching logic:
/// inserting a new value replaces whatever was drawn before.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[require(VisualPose, Transform, Visibility)]
pub enum CharacterVisual {
    /// Camera-facing sprite picked from eight directional frames of a sheet
    /// (horses and the mounted rider).
    Billboard(SheetKind),
    /// A low-poly 3D person on foot.
    Figure(FigureKind),
}

/// The child entity a [`CharacterVisual`] draws with (a billboard or a
/// figure's body). It and everything under it go when the visual changes.
#[derive(Component, Debug, Clone, Copy)]
pub struct VisualPart;

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
        app.add_systems(Startup, (load_sprites, load_figure_kit))
            .add_systems(
                PostUpdate,
                (attach_visuals, (update_billboards, update_figures))
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

fn load_figure_kit(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(FigureKit::new(&mut meshes));
}

/// Draws new and changed visuals, dropping whatever drew the old one.
fn attach_visuals(
    mut commands: Commands,
    sprites: Res<BillboardSprites>,
    mut kit: ResMut<FigureKit>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    changed: Query<(Entity, &CharacterVisual, Option<&Children>), Changed<CharacterVisual>>,
    parts: Query<(), With<VisualPart>>,
) {
    for (entity, visual, children) in &changed {
        for &child in children.into_iter().flatten() {
            if parts.contains(child) {
                commands.entity(child).despawn();
            }
        }
        // Per-entity start so a crowd does not step in unison.
        let phase = (entity.index_u32() as f32 * 0.37).fract();
        match *visual {
            CharacterVisual::Figure(kind) => {
                let body =
                    spawn_figure(&mut commands, &mut kit, &mut materials, entity, kind, phase);
                commands.entity(body).insert(VisualPart);
            }
            CharacterVisual::Billboard(sheet) => {
                let Some(sprites) = sprites.0.get(&sheet) else {
                    continue;
                };
                commands.entity(entity).with_child((
                    Name::new("Billboard"),
                    VisualPart,
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

/// Gait cycles per second of a horse's `animation` at ground `speed`
/// (m/s): one cycle covers a fixed stride, so hooves keep pace with the
/// ground.
pub fn cadence(animation: AnimationState, speed: f32) -> f32 {
    let speed = if speed.is_finite() { speed.abs() } else { 0.0 };
    match animation {
        AnimationState::Idle => 0.0,
        AnimationState::Walk => speed.max(1.0) / 1.8,
        AnimationState::Gallop => speed.max(5.0) / 4.2,
    }
}

/// Vertical bob amplitude in meters per logical gait, on top of the rise
/// already painted into the frames.
fn bob(animation: AnimationState) -> f32 {
    match animation {
        AnimationState::Idle => 0.0,
        AnimationState::Walk => 0.02,
        AnimationState::Gallop => 0.08,
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
        let cycles = cadence(pose.animation, pose.speed) * time.delta_secs();
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
        transform.translation.y = billboard.sheet.sprite_size() / 2.0 + bob(pose.animation) * bump;
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
        assert_eq!(cadence(AnimationState::Idle, 3.0), 0.0);
        assert!(cadence(AnimationState::Walk, 2.5) > cadence(AnimationState::Walk, 1.5));
        assert!(cadence(AnimationState::Gallop, 11.0) > cadence(AnimationState::Walk, 2.5));
        assert!(
            cadence(AnimationState::Walk, f32::NAN) > 0.0,
            "never stalls a gait"
        );
    }

    /// The whole plugin, headless, with stand-in asset stores and a camera.
    fn plugin_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                50,
            )))
            .init_resource::<Assets<Image>>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .add_plugins(CharacterVisualPlugin);
        app.world_mut().spawn((
            Camera3d::default(),
            Transform::from_xyz(0.0, 4.0, 9.0).looking_at(Vec3::ZERO, Vec3::Y),
        ));
        app.update();
        app
    }

    fn count<F: bevy::ecs::query::QueryFilter>(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<(), F>()
            .iter(app.world())
            .count()
    }

    #[test]
    fn swapping_a_figure_for_the_rider_leaves_no_parts_behind() {
        use super::super::figure::{FigureBody, FigureJoint, FigurePart};
        let mut app = plugin_app();
        let me = app
            .world_mut()
            .spawn(CharacterVisual::Figure(FigureKind::Protagonist))
            .id();
        app.update();
        assert_eq!(count::<With<FigureBody>>(&mut app), 1);
        assert_eq!(count::<With<FigureJoint>>(&mut app), 11);
        assert!(count::<With<FigurePart>>(&mut app) >= 18);

        // Mount: the new visual replaces the figure the same frame.
        app.world_mut()
            .entity_mut(me)
            .insert(CharacterVisual::Billboard(SheetKind::Rider));
        app.update();
        assert_eq!(count::<With<FigureBody>>(&mut app), 0);
        assert_eq!(count::<With<FigureJoint>>(&mut app), 0);
        assert_eq!(count::<With<FigurePart>>(&mut app), 0);
        let mut billboards = app.world_mut().query::<(&Billboard, &ChildOf)>();
        let all: Vec<_> = billboards.iter(app.world()).collect();
        assert_eq!(all.len(), 1, "exactly one rider visual");
        assert_eq!((all[0].0.sheet, all[0].1.parent()), (SheetKind::Rider, me));
        let children = app.world().get::<Children>(me).unwrap();
        assert_eq!(children.len(), 1);
    }

    #[test]
    fn a_crowd_of_one_look_shares_meshes_and_materials() {
        let mut app = plugin_app();
        let one = |app: &mut App| {
            app.world_mut()
                .spawn(CharacterVisual::Figure(FigureKind::Townsfolk(2)));
            app.update();
            let meshes = app.world().resource::<Assets<Mesh>>().len();
            let materials = app.world().resource::<Assets<StandardMaterial>>().len();
            (meshes, materials)
        };
        let first = one(&mut app);
        for _ in 0..19 {
            one(&mut app);
        }
        assert_eq!(one(&mut app), first, "no new meshes or materials");
        let mut handles = app.world_mut().query::<&Mesh3d>();
        let distinct: std::collections::HashSet<_> =
            handles.iter(app.world()).map(|m| m.0.id()).collect();
        // Twenty people drawn with no more meshes than one person's parts.
        let one_person: std::collections::HashSet<_> =
            super::super::figure::figure_parts(FigureKind::Townsfolk(2))
                .iter()
                .map(|p| p.shape)
                .collect();
        assert_eq!(distinct.len(), one_person.len());
    }

    #[test]
    fn a_walking_figure_turns_to_its_heading_and_swings_its_legs() {
        use super::super::figure::{FigureBody, FigureJoint, Joint};
        let mut app = plugin_app();
        let me = app
            .world_mut()
            .spawn((
                CharacterVisual::Figure(FigureKind::Hidalgo),
                VisualPose {
                    heading: Heading::from_radians(PI),
                    animation: AnimationState::Walk,
                    speed: 1.4,
                },
            ))
            .id();
        let mut swings = Vec::new();
        for _ in 0..12 {
            app.update();
            let mut joints = app.world_mut().query::<(&FigureJoint, &Transform)>();
            let mut hip = |j: Joint| {
                joints
                    .iter(app.world())
                    .find(|(f, _)| f.0 == j)
                    .map(|(_, t)| t.rotation.to_euler(EulerRot::ZYX).0)
                    .unwrap()
            };
            let left = hip(Joint::Hip(super::super::figure::Side::Left));
            let right = hip(Joint::Hip(super::super::figure::Side::Right));
            assert!((left + right).abs() < 1e-4, "opposite phase");
            swings.push(left);
        }
        assert!(swings.iter().any(|a| *a > 0.2) && swings.iter().any(|a| *a < -0.2));
        // Facing west: the body turned half round.
        let mut bodies = app
            .world_mut()
            .query::<(&FigureBody, &Transform, &ChildOf)>();
        let (_, body, parent) = bodies.single(app.world()).unwrap();
        assert_eq!(parent.parent(), me);
        assert!((body.rotation * Vec3::X).distance(Vec3::NEG_X) < 1e-4);
    }
}
