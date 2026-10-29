//! Per-entity character visuals. Logic hands over a [`VisualPose`]; render
//! decides how it looks: people on foot (see [`figure`](super::figure) and
//! [`model`](super::model)), saddled horses (see [`horse`](super::horse))
//! and a rider in the saddle (see [`rider`](super::rider)).

use bevy::app::AnimationSystems;
use bevy::camera::visibility::VisibilitySystems;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use pampa_core::{AnimationState, Heading};

use super::clips::{load_character_clips, read_reference};
use super::figure::{FigureKind, FigureKit, spawn_figure, update_figures};
use super::finish::Palette;
use super::horse::{Coat, dress_horse, drive_horses, fit_horses, spawn_horse, swap_in_horses};
use super::model::{
    CharacterModels, CharacterModelsShown, attach_model, dress_character, load_character_models,
    swap_in_character_models, toggle_character_models,
};
use super::rider::{carry_stirrups, pose_riders, seat_riders, spawn_riders};
use super::rig::{drive_rigs, fit_rigs};
use super::tack::{dress_tack, stretch_straps};
use super::townsfolk::TintCache;
use super::viewer::mark_viewer_head;

/// How a character is drawn. Swappable per entity without touching logic:
/// inserting a new value replaces whatever was drawn before.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[require(VisualPose, Transform, Visibility)]
pub enum CharacterVisual {
    /// A low-poly 3D person on foot.
    Figure(FigureKind),
    /// A saddled horse with nobody on it, like the one tied at a store.
    /// (The untacked horses loose in the corrals are fauna: see
    /// [`Animal::Horse`](crate::fauna::Animal::Horse).)
    Horse(Coat),
    /// A person riding a saddled horse; the pose is the horse's. Only a
    /// rider with a model ([`model_name`](super::model::model_name)) is
    /// drawn on it.
    Mounted { rider: FigureKind, coat: Coat },
}

/// The child entity a [`CharacterVisual`] draws with (a figure's or a
/// horse's body, and a rider on the horse). It and everything under it go
/// when the visual changes.
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
    /// Ground speed in m/s, negative backing up; render paces the gait
    /// with its size and plays the stride backward by its sign.
    pub speed: f32,
    /// Yaw rate in rad/s (positive to the left) the body actually turned
    /// this frame, 0 when not turning; standing still, render steps in
    /// place by its size.
    pub turn: f32,
    /// How far through a jump's flight a mounted rider is, `0..=1` (none on
    /// the ground, and always none for people on foot): the saddled horse
    /// plays its jump clip along it, tucked at the top and stretching out
    /// to land.
    pub flight: Option<f32>,
}

/// Draws entities carrying a [`CharacterVisual`].
pub struct CharacterVisualPlugin;

impl Plugin for CharacterVisualPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CharacterModelsShown>()
            .init_resource::<TintCache>()
            .add_systems(
                Startup,
                (load_kits, load_character_models, load_character_clips),
            )
            .add_systems(
                Update,
                (
                    toggle_character_models,
                    swap_in_character_models,
                    swap_in_horses,
                    spawn_riders,
                    seat_riders,
                    read_reference,
                )
                    .chain(),
            )
            .add_systems(
                PostUpdate,
                (attach_visuals, update_figures, drive_rigs, drive_horses)
                    .chain()
                    .before(AnimationSystems)
                    .before(TransformSystems::Propagate),
            )
            .add_systems(
                PostUpdate,
                (fit_rigs, fit_horses, pose_riders)
                    .chain()
                    .after(AnimationSystems)
                    .before(TransformSystems::Propagate),
            )
            .add_systems(
                PostUpdate,
                (carry_stirrups, stretch_straps)
                    .chain()
                    .after(TransformSystems::Propagate)
                    .before(VisibilitySystems::VisibilityPropagate),
            )
            .add_observer(dress_character)
            .add_observer(dress_horse)
            .add_observer(dress_tack)
            .add_observer(mark_viewer_head);
    }
}

fn load_kits(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(FigureKit::new(&mut meshes));
    commands.insert_resource(Palette::default());
}

/// The kit and asset stores [`attach_visuals`] draws with.
type Kits<'a> = (
    ResMut<'a, FigureKit>,
    ResMut<'a, Palette>,
    ResMut<'a, Assets<StandardMaterial>>,
    ResMut<'a, Assets<Image>>,
);

/// Draws new and changed visuals, dropping whatever drew the old one.
fn attach_visuals(
    mut commands: Commands,
    (mut people, mut palette, mut materials, mut images): Kits,
    changed: Query<(Entity, &CharacterVisual, Option<&Children>), Changed<CharacterVisual>>,
    parts: Query<(), With<VisualPart>>,
    (models, shown): (Option<Res<CharacterModels>>, Res<CharacterModelsShown>),
) {
    for (entity, visual, children) in &changed {
        for &child in children.into_iter().flatten() {
            if parts.contains(child) {
                commands.entity(child).despawn();
            }
        }
        // Per-entity start so a crowd does not step in unison.
        let phase = (entity.index_u32() as f32 * 0.37).fract();
        let body = match *visual {
            CharacterVisual::Figure(kind) => {
                let body = spawn_figure(
                    &mut commands,
                    &mut people,
                    (&mut palette, &mut materials, &mut images),
                    entity,
                    kind,
                    phase,
                );
                if shown.0 {
                    attach_model(&mut commands, models.as_deref(), body, kind);
                }
                body
            }
            CharacterVisual::Horse(coat) => spawn_horse(&mut commands, entity, coat, None),
            // The rider is seated once the horse's scene spawns.
            CharacterVisual::Mounted { rider, coat } => {
                spawn_horse(&mut commands, entity, coat, Some(rider))
            }
        };
        commands.entity(body).insert(VisualPart);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use std::f32::consts::PI;
    use std::time::Duration;

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
    fn mounting_swaps_the_figure_for_one_saddled_horse_and_back() {
        use super::super::figure::{FigureBody, FigureJoint, FigurePart};
        use super::super::horse::{HorseBody, HorseModel, HorsePending};
        let mut app = plugin_app();
        let me = app
            .world_mut()
            .spawn(CharacterVisual::Figure(FigureKind::Protagonist))
            .id();
        app.update();
        assert_eq!(count::<With<FigureBody>>(&mut app), 1);
        assert_eq!(
            count::<With<FigureJoint>>(&mut app),
            super::super::figure::Joint::ALL.len()
        );
        assert!(count::<With<FigurePart>>(&mut app) >= 18);
        assert_eq!(count::<With<HorseBody>>(&mut app), 0);

        // Mount: the new visual replaces the figure the same frame, with a
        // horse body waiting for its model and nothing rigid.
        app.world_mut()
            .entity_mut(me)
            .insert(CharacterVisual::Mounted {
                rider: FigureKind::Protagonist,
                coat: Coat::HIDALGO,
            });
        app.update();
        let children = app.world().get::<Children>(me).unwrap();
        assert_eq!(children.len(), 1, "one horse-and-rider visual");
        let horse = children[0];
        let body = app.world().get::<HorseBody>(horse).unwrap();
        assert_eq!(body.coat, Coat::Bayo);
        assert_eq!(body.rider, Some(FigureKind::Protagonist));
        assert!(body.never_freezes(), "the player's horse");
        let model = app.world().get::<Children>(horse).unwrap()[0];
        assert!(app.world().get::<HorseModel>(model).is_some());
        // No horse file in this app: it waits, drawing nothing.
        assert!(app.world().get::<HorsePending>(model).is_some());
        assert_eq!(count::<With<FigureBody>>(&mut app), 0, "no rigid rider");
        assert_eq!(count::<With<FigureJoint>>(&mut app), 0);
        assert_eq!(count::<With<Mesh3d>>(&mut app), 0);

        // Dismount: back to the figure, nothing of the horse left.
        app.world_mut()
            .entity_mut(me)
            .insert(CharacterVisual::Figure(FigureKind::Protagonist));
        app.update();
        assert_eq!(count::<With<HorseBody>>(&mut app), 0);
        assert_eq!(count::<With<HorseModel>>(&mut app), 0);
        assert_eq!(count::<With<FigureBody>>(&mut app), 1);
        assert_eq!(app.world().get::<Children>(me).unwrap().len(), 1);
    }

    #[test]
    fn a_saddled_horse_turns_to_its_heading_and_takes_its_gait_from_the_speed() {
        use super::super::horse::HorseBody;
        use pampa_core::HorseGait;
        let mut app = plugin_app();
        let horse = app
            .world_mut()
            .spawn((
                CharacterVisual::Horse(Coat::Tordillo),
                VisualPose {
                    heading: Heading::from_radians(PI),
                    animation: AnimationState::Gallop,
                    speed: 11.0,
                    turn: 0.0,
                    flight: None,
                },
            ))
            .id();
        app.update();
        let body = |app: &mut App| {
            let mut bodies = app.world_mut().query::<(&HorseBody, &Transform)>();
            let (b, t) = bodies.single(app.world()).unwrap();
            (b.clone(), *t)
        };
        let (b, t) = body(&mut app);
        assert_eq!(b.gait, HorseGait::Gallop);
        assert_eq!(b.rider, None);
        assert!(!b.never_freezes(), "a tied horse may freeze");
        assert!(
            (t.rotation * Vec3::X).distance(Vec3::NEG_X) < 1e-4,
            "turned west"
        );
        app.world_mut().entity_mut(horse).insert(VisualPose {
            heading: Heading::from_radians(PI / 2.0),
            animation: AnimationState::Gallop,
            speed: 5.0,
            turn: 0.0,
            flight: None,
        });
        app.update();
        let (b, t) = body(&mut app);
        assert_eq!(b.gait, HorseGait::Trot);
        let front = Quat::from_rotation_y(PI / 2.0) * Vec3::X;
        assert!((t.rotation * Vec3::X).distance(front) < 1e-4);
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
                    turn: 0.0,
                    flight: None,
                },
            ))
            .id();
        // The starting phase depends on the entity, so sample more than one
        // whole gait cycle: 24 frames of 50 ms at about 1.08 cycles/s.
        let frames = 24;
        let cycles = frames as f32 * 0.05 * pampa_core::pose::cadence(AnimationState::Walk, 1.4);
        assert!(cycles >= 1.0, "{cycles}");
        let mut swings = Vec::new();
        for _ in 0..frames {
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
            assert!(left * right <= 1e-6, "opposite phase");
            swings.push(left);
        }
        // Over a whole cycle each leg reaches close to its full swing both
        // ways, wherever the cycle started.
        let full = pampa_core::pose::pose(AnimationState::Walk, 0.25).hip[0];
        let (high, low) = swings
            .iter()
            .fold((f32::MIN, f32::MAX), |(h, l), a| (h.max(*a), l.min(*a)));
        assert!(
            high > 0.9 * full && low < -0.9 * full,
            "{high} {low} of {full}"
        );
        // Facing west: the body turned half round.
        let mut bodies = app
            .world_mut()
            .query::<(&FigureBody, &Transform, &ChildOf)>();
        let (_, body, parent) = bodies.single(app.world()).unwrap();
        assert_eq!(parent.parent(), me);
        assert!((body.rotation * Vec3::X).distance(Vec3::NEG_X) < 1e-4);
    }
}
