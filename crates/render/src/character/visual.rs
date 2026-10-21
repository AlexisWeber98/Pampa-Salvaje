//! Per-entity character visuals. Logic hands over a [`VisualPose`]; render
//! decides how it looks: low-poly 3D people (see [`figure`](super::figure))
//! and horses (see [`horse`](super::horse)), and a rider in the saddle.

use bevy::prelude::*;
use bevy::transform::TransformSystems;
use pampa_core::{AnimationState, Heading};

use super::figure::{FigureKind, FigureKit, spawn_figure, update_figures};
use super::finish::Palette;
use super::horse::{Coat, HorseKit, spawn_horse, update_horses};

/// How a character is drawn. Swappable per entity without touching logic:
/// inserting a new value replaces whatever was drawn before.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[require(VisualPose, Transform, Visibility)]
pub enum CharacterVisual {
    /// A low-poly 3D person on foot.
    Figure(FigureKind),
    /// A saddled horse with nobody on it, like the one tied at a store.
    Horse(Coat),
    /// A person riding a saddled horse; the pose is the horse's.
    Mounted { rider: FigureKind, coat: Coat },
}

/// The child entity a [`CharacterVisual`] draws with (a figure's or a
/// horse's body). It and everything under it go when the visual changes.
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
    /// Ground speed in m/s; render paces the gait with it.
    pub speed: f32,
}

/// Draws entities carrying a [`CharacterVisual`].
pub struct CharacterVisualPlugin;

impl Plugin for CharacterVisualPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_kits).add_systems(
            PostUpdate,
            (attach_visuals, update_horses, update_figures)
                .chain()
                .before(TransformSystems::Propagate),
        );
    }
}

fn load_kits(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(FigureKit::new(&mut meshes));
    commands.insert_resource(HorseKit::new(&mut meshes));
    commands.insert_resource(Palette::default());
}

/// The kits and asset stores [`attach_visuals`] draws with.
type Kits<'a> = (
    ResMut<'a, FigureKit>,
    ResMut<'a, HorseKit>,
    ResMut<'a, Palette>,
    ResMut<'a, Assets<Mesh>>,
    ResMut<'a, Assets<StandardMaterial>>,
    ResMut<'a, Assets<Image>>,
);

/// Draws new and changed visuals, dropping whatever drew the old one.
fn attach_visuals(
    mut commands: Commands,
    (mut people, mut horses, mut palette, mut meshes, mut materials, mut images): Kits,
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
        let body = match *visual {
            CharacterVisual::Figure(kind) => spawn_figure(
                &mut commands,
                &mut people,
                (&mut palette, &mut materials, &mut images),
                entity,
                kind,
                phase,
                None,
            ),
            CharacterVisual::Horse(coat) => {
                spawn_horse(
                    &mut commands,
                    (&mut horses, &mut meshes),
                    (&mut palette, &mut materials, &mut images),
                    entity,
                    coat,
                    phase,
                    false,
                )
                .0
            }
            CharacterVisual::Mounted { rider, coat } => {
                let (horse, trunk) = spawn_horse(
                    &mut commands,
                    (&mut horses, &mut meshes),
                    (&mut palette, &mut materials, &mut images),
                    entity,
                    coat,
                    phase,
                    true,
                );
                // The rider sits on the trunk, so it rises, rocks and turns
                // with the horse.
                spawn_figure(
                    &mut commands,
                    &mut people,
                    (&mut palette, &mut materials, &mut images),
                    trunk,
                    rider,
                    phase,
                    Some(horse),
                );
                horse
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
    fn mounting_swaps_the_figure_for_one_horse_and_rider_and_back() {
        use super::super::figure::{FigureBody, FigureJoint, FigurePart};
        use super::super::horse::{HorseBody, HorseJoint, HorseJointPart, HorsePartMesh};
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

        // Mount: the new visual replaces the figure the same frame.
        app.world_mut()
            .entity_mut(me)
            .insert(CharacterVisual::Mounted {
                rider: FigureKind::Protagonist,
                coat: Coat::HIDALGO,
            });
        app.update();
        assert_eq!(count::<With<HorseBody>>(&mut app), 1);
        assert_eq!(
            count::<With<HorseJointPart>>(&mut app),
            HorseJoint::ALL.len()
        );
        assert_eq!(count::<With<FigureBody>>(&mut app), 1, "one rider");
        assert_eq!(
            count::<With<FigureJoint>>(&mut app),
            super::super::figure::Joint::ALL.len()
        );
        let children = app.world().get::<Children>(me).unwrap();
        assert_eq!(children.len(), 1, "one horse-and-rider visual");
        let horse = children[0];
        assert_eq!(
            app.world().get::<HorseBody>(horse).unwrap().coat,
            Coat::Bayo
        );
        // The rider hangs off the horse's trunk.
        let mut riders = app.world_mut().query::<(&FigureBody, &ChildOf)>();
        let (rider, seat) = riders.single(app.world()).unwrap();
        assert_eq!(rider.seat, Some(horse));
        assert_eq!(
            app.world().get::<HorseJointPart>(seat.parent()).unwrap().0,
            HorseJoint::Trunk
        );
        // Every mesh drawn is a figure or horse part: nothing else is left.
        let meshes = count::<With<Mesh3d>>(&mut app);
        let parts = count::<With<FigurePart>>(&mut app) + count::<With<HorsePartMesh>>(&mut app);
        assert_eq!(meshes, parts);

        // Dismount: back to the figure, nothing of the horse left.
        app.world_mut()
            .entity_mut(me)
            .insert(CharacterVisual::Figure(FigureKind::Protagonist));
        app.update();
        assert_eq!(count::<With<HorseBody>>(&mut app), 0);
        assert_eq!(count::<With<HorseJointPart>>(&mut app), 0);
        assert_eq!(count::<With<HorsePartMesh>>(&mut app), 0);
        assert_eq!(count::<With<FigureBody>>(&mut app), 1);
        assert_eq!(app.world().get::<Children>(me).unwrap().len(), 1);
    }

    #[test]
    fn the_rider_moves_with_the_horses_gait() {
        use super::super::figure::{FigureBody, FigureJoint, Joint, Side};
        use super::super::horse::{HorseBody, HorseJoint, HorseJointPart};
        use pampa_core::HorseGait;
        let mut app = plugin_app();
        app.world_mut().spawn((
            CharacterVisual::Mounted {
                rider: FigureKind::Protagonist,
                coat: Coat::Bayo,
            },
            VisualPose {
                heading: Heading::from_radians(PI),
                animation: AnimationState::Gallop,
                speed: 11.0,
            },
        ));
        let mut trunk = Vec::new();
        let mut legs = Vec::new();
        for _ in 0..30 {
            app.update();
            let mut bodies = app.world_mut().query::<(&HorseBody, &Transform)>();
            let (horse, body) = bodies.single(app.world()).unwrap();
            let (gait, body) = (horse.gait, *body);
            assert_eq!(gait, HorseGait::Gallop);
            assert!(
                (body.rotation * Vec3::X).distance(Vec3::NEG_X) < 1e-4,
                "turned west"
            );
            let mut joints = app.world_mut().query::<(&HorseJointPart, &Transform)>();
            let mut joint = |j: HorseJoint| {
                joints
                    .iter(app.world())
                    .find(|(p, _)| p.0 == j)
                    .map(|(_, t)| *t)
                    .unwrap()
            };
            trunk.push(
                body.translation.y + joint(HorseJoint::Trunk).rotation.to_euler(EulerRot::ZYX).0,
            );
            legs.push(joint(HorseJoint::Upper(pampa_core::Leg::FrontLeft)).rotation);
            // The rider sits astride: thighs forward, never turned by the
            // heading again.
            let mut riders = app.world_mut().query::<(&FigureBody, &Transform)>();
            let (_, rider) = riders.single(app.world()).unwrap();
            assert_eq!(rider.rotation, Quat::IDENTITY);
            let mut fj = app.world_mut().query::<(&FigureJoint, &Transform)>();
            let hip = fj
                .iter(app.world())
                .find(|(j, _)| j.0 == Joint::Hip(Side::Left))
                .map(|(_, t)| t.rotation)
                .unwrap();
            assert!((hip * Vec3::NEG_Y).x > 0.5, "the thigh points forward");
        }
        let spread = |v: &[f32]| {
            v.iter().fold(f32::MIN, |a, b| a.max(*b)) - v.iter().fold(f32::MAX, |a, b| a.min(*b))
        };
        assert!(spread(&trunk) > 0.05, "the body rises and rocks: {trunk:?}");
        assert!(legs.windows(2).any(|w| w[0] != w[1]), "the legs move");
    }

    #[test]
    fn a_tied_horse_stands_saddled_and_breathes() {
        use super::super::figure::FigureBody;
        use super::super::horse::{HorseBody, HorsePartMesh, HorsePiece};
        let mut app = plugin_app();
        app.world_mut()
            .spawn(CharacterVisual::Horse(Coat::Tordillo));
        app.update();
        assert_eq!(count::<With<HorseBody>>(&mut app), 1);
        assert_eq!(count::<With<FigureBody>>(&mut app), 0, "nobody on it");
        let mut pieces = app.world_mut().query::<&HorsePartMesh>();
        let all: Vec<_> = pieces.iter(app.world()).map(|p| p.0).collect();
        for piece in [
            HorsePiece::Pellon,
            HorsePiece::Stirrup,
            HorsePiece::Rein,
            HorsePiece::Hoof,
        ] {
            assert!(all.contains(&piece), "{piece:?}");
        }
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
            assert!((left + right).abs() < 1e-4, "opposite phase");
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
