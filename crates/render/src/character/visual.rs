//! Los visuales de personaje por entidad. La lógica entrega una [`VisualPose`];
//! render decide cómo se ve: personas a pie (ver [`figure`](super::figure) y
//! [`model`](super::model)), caballos ensillados (ver [`horse`](super::horse))
//! y un jinete en la montura (ver [`rider`](super::rider)).

use bevy::app::AnimationSystems;
use bevy::camera::visibility::VisibilitySystems;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use pampa_core::pulperia::Pastime;
use pampa_core::{AnimationState, Heading};

use super::clips::{load_character_clips, read_reference};
use super::fade::fade_people;
use super::figure::{FigureKind, FigureKit, body_rotation, spawn_figure, update_figures};
use super::finish::Palette;
use super::horse::{Coat, dress_horse, drive_horses, fit_horses, spawn_horse, swap_in_horses};
use super::model::{
    CharacterModel, CharacterModels, CharacterModelsShown, ModelPending, attach_model,
    dress_character, load_character_models, swap_in_character_models, toggle_character_models,
};
use super::rider::{
    OnBench, Riding, carry_stirrups, pose_riders, seat_riders, seat_transform, spawn_riders,
};
use super::rig::{drive_rigs, fit_rigs};
use super::tack::{PendingTack, TackBuilt, dress_pending, dress_tack, load_recado, stretch_straps};
use super::townsfolk::TintCache;
use super::viewer::mark_viewer_head;

/// Cómo se dibuja un personaje. Se puede cambiar por entidad sin tocar la
/// lógica: insertar un valor nuevo reemplaza lo que se dibujaba antes.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[require(VisualPose, Transform, Visibility)]
pub enum CharacterVisual {
    /// Una persona 3D low-poly a pie.
    Figure(FigureKind),
    /// Un caballo ensillado sin nadie encima, como el que está atado en un
    /// almacén. (Los caballos sin aperos sueltos en los corrales son fauna: ver
    /// [`Animal::Horse`](crate::fauna::Animal::Horse).)
    Horse(Coat),
    /// Un caballo de tiro, sin aperos de montar: el que tira de un carro o de
    /// un sulky (ver [`vehicle`](crate::vehicle)).
    Draught(Coat),
    /// Una persona montada en un caballo ensillado; la pose es la del caballo.
    /// Solo se dibuja encima un jinete con modelo
    /// ([`model_name`](super::model::model_name)).
    Mounted { rider: FigureKind, coat: Coat },
    /// Una persona sentada en un banco, quieta, con las manos en el regazo: el
    /// `Transform` de la entidad es la parte de arriba del asiento, con el piso
    /// [`BENCH_HEIGHT`](super::rider::BENCH_HEIGHT) más abajo, y el rumbo de su
    /// [`VisualPose`], hacia dónde mira. Solo se dibuja con modelo
    /// ([`model_name`](super::model::model_name)), como un conductor. Pasa el
    /// rato con su [`Pastime`]: quieto, jugando al truco, fumando, tomando o
    /// conversando.
    Seated(FigureKind, Pastime),
}

/// La entidad hija con la que dibuja un [`CharacterVisual`] (el cuerpo de una
/// figura o de un caballo, y un jinete sobre el caballo). Ella y todo lo que
/// cuelga de ella se eliminan cuando cambia el visual.
#[derive(Component, Debug, Clone, Copy)]
pub struct VisualPart;

/// El estado de la lógica que necesita el visual, copiado de `pampa-core` en
/// cada frame por el juego.
///
/// El `Transform` de la entidad debe llevar solo traslación (sin rotación): el
/// rumbo continuo vive aquí, no en el transform.
#[derive(Component, Debug, Clone, Copy, PartialEq, Default)]
pub struct VisualPose {
    pub heading: Heading,
    pub animation: AnimationState,
    /// Velocidad sobre el suelo en m/s, negativa al retroceder; render marca el
    /// paso del andar según su magnitud y reproduce la zancada hacia atrás
    /// según su signo.
    pub speed: f32,
    /// Velocidad de giro en rad/s (positiva hacia la izquierda) que el cuerpo
    /// giró realmente en este frame, 0 cuando no gira; parado, render da pasos
    /// en el lugar según su magnitud.
    pub turn: f32,
    /// Qué tan avanzado está el vuelo de un salto de un jinete montado, `0..=1`
    /// (nada en el suelo, y siempre nada para las personas a pie): el caballo
    /// ensillado reproduce su clip de salto a lo largo de este valor, recogido
    /// arriba y estirándose para aterrizar.
    pub flight: Option<f32>,
}

/// Dibuja las entidades que llevan un [`CharacterVisual`].
pub struct CharacterVisualPlugin;

impl Plugin for CharacterVisualPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CharacterModelsShown>()
            .init_resource::<TintCache>()
            .init_resource::<PendingTack>()
            .init_resource::<TackBuilt>()
            .add_systems(
                Startup,
                (
                    load_kits,
                    load_character_models,
                    load_character_clips,
                    load_recado,
                    super::pastime::make_kit,
                ),
            )
            .add_systems(
                Update,
                (
                    toggle_character_models,
                    swap_in_character_models,
                    swap_in_horses,
                    dress_pending,
                    spawn_riders,
                    seat_riders,
                    read_reference,
                )
                    .chain(),
            )
            .add_systems(Update, super::pastime::hold_things)
            .add_systems(
                PostUpdate,
                (super::pastime::drift_smoke, super::pastime::tilt_glasses)
                    .after(TransformSystems::Propagate),
            )
            .add_systems(
                PostUpdate,
                (attach_visuals, update_figures, drive_rigs, drive_horses)
                    .chain()
                    .before(AnimationSystems)
                    .before(TransformSystems::Propagate),
            )
            .add_systems(PostUpdate, fade_people.after(attach_visuals))
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

/// El kit y los almacenes de assets con los que dibuja [`attach_visuals`].
type Kits<'a> = (
    ResMut<'a, FigureKit>,
    ResMut<'a, Palette>,
    ResMut<'a, Assets<StandardMaterial>>,
    ResMut<'a, Assets<Image>>,
);

/// Dibuja los visuales nuevos y los cambiados, descartando lo que dibujaba el
/// anterior.
fn attach_visuals(
    mut commands: Commands,
    (mut people, mut palette, mut materials, mut images): Kits,
    changed: Query<
        (Entity, &CharacterVisual, &VisualPose, Option<&Children>),
        Changed<CharacterVisual>,
    >,
    parts: Query<(), With<VisualPart>>,
    (models, shown): (Option<Res<CharacterModels>>, Res<CharacterModelsShown>),
) {
    for (entity, visual, pose, children) in &changed {
        for &child in children.into_iter().flatten() {
            if parts.contains(child) {
                commands.entity(child).despawn();
            }
        }
        // Inicio por entidad para que una multitud no camine al unísono.
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
            CharacterVisual::Horse(coat) => spawn_horse(&mut commands, entity, coat, None, true),
            CharacterVisual::Draught(coat) => spawn_horse(&mut commands, entity, coat, None, false),
            // El jinete se sienta en cuanto aparece la escena del caballo.
            CharacterVisual::Mounted { rider, coat } => {
                spawn_horse(&mut commands, entity, coat, Some(rider), true)
            }
            CharacterVisual::Seated(kind, pastime) => spawn_sitter(
                &mut commands,
                models.as_deref(),
                entity,
                kind,
                pastime,
                pose.heading,
            ),
        };
        commands.entity(body).insert(VisualPart);
    }
}

/// El asiento de una persona sentada, girado hacia su rumbo, y su modelo sobre
/// él, como el conductor de un vehículo (ver [`vehicle`](crate::vehicle)): un
/// [`Riding`] sin caballo debajo, que [`pose_riders`] sienta con la pelvis
/// sobre el asiento.
fn spawn_sitter(
    commands: &mut Commands,
    models: Option<&CharacterModels>,
    entity: Entity,
    kind: FigureKind,
    pastime: Pastime,
    heading: Heading,
) -> Entity {
    let seat = commands
        .spawn((
            Name::new("Bench seat"),
            Transform::from_rotation(body_rotation(heading)),
            Visibility::default(),
            ChildOf(entity),
        ))
        .id();
    if let Some(scene) = models.and_then(|m| m.of(kind)).cloned() {
        commands.spawn((
            Name::new("Seated model"),
            CharacterModel,
            ModelPending(scene),
            Riding {
                horse: seat,
                kind,
                seed: entity.index_u32(),
            },
            OnBench(pastime),
            Transform::from_matrix(Mat4::from(seat_transform(Vec3::ZERO))),
            // Se dibuja una vez sentado: su escena se crea parada en reposo.
            Visibility::Hidden,
            ChildOf(seat),
        ));
    }
    seat
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use std::f32::consts::PI;
    use std::time::Duration;

    /// El plugin completo, headless, con almacenes de assets sustitutos y una
    /// cámara.
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

        // Montar: el nuevo visual reemplaza a la figura en el mismo frame, con
        // un cuerpo de caballo esperando su modelo y nada rígido.
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
        assert_eq!(body.coat, Coat::HIDALGO);
        assert_eq!(body.rider, Some(FigureKind::Protagonist));
        assert!(body.never_freezes(), "the player's horse");
        let model = app.world().get::<Children>(horse).unwrap()[0];
        assert!(app.world().get::<HorseModel>(model).is_some());
        // No hay archivo de caballo en esta app: espera, sin dibujar nada.
        assert!(app.world().get::<HorsePending>(model).is_some());
        assert_eq!(count::<With<FigureBody>>(&mut app), 0, "no rigid rider");
        assert_eq!(count::<With<FigureJoint>>(&mut app), 0);
        assert_eq!(count::<With<Mesh3d>>(&mut app), 0);

        // Desmontar: vuelve a la figura, sin que quede nada del caballo.
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
        // Veinte personas dibujadas con no más meshes que las partes de una
        // sola persona.
        let one_person: std::collections::HashSet<_> =
            super::super::figure::figure_parts(FigureKind::Townsfolk(2))
                .iter()
                .map(|p| p.shape)
                .collect();
        assert_eq!(distinct.len(), one_person.len());
    }

    /// Cada material dibujado bajo `who`: su id, su alfa y su modo de alfa.
    fn looks(app: &mut App, who: Entity) -> Vec<(AssetId<StandardMaterial>, f32, AlphaMode)> {
        let mut under = Vec::new();
        let mut stack = vec![who];
        while let Some(e) = stack.pop() {
            if let Some(children) = app.world().get::<Children>(e) {
                stack.extend(children.iter());
                under.extend(children.iter());
            }
        }
        let materials = app.world().resource::<Assets<StandardMaterial>>();
        let mut found: Vec<_> = under
            .iter()
            .filter_map(|e| app.world().get::<MeshMaterial3d<StandardMaterial>>(*e))
            .map(|m| {
                let material = materials.get(&m.0).unwrap();
                (m.0.id(), material.base_color.alpha(), material.alpha_mode)
            })
            .collect();
        found.sort_by_key(|(id, ..)| format!("{id:?}"));
        found
    }

    #[test]
    fn a_fading_person_turns_see_through_alone_and_the_others_of_its_look_stay_opaque() {
        use super::super::fade::Fading;
        let mut app = plugin_app();
        let kind = CharacterVisual::Figure(FigureKind::Bairoletto);
        let leaving = app.world_mut().spawn(kind).id();
        let other = app.world_mut().spawn(kind).id();
        app.update();
        let shared = looks(&mut app, other);
        assert!(!shared.is_empty());
        assert_eq!(looks(&mut app, leaving), shared, "one look, shared");
        assert!(
            shared
                .iter()
                .all(|(_, a, mode)| *a == 1.0 && *mode == AlphaMode::Opaque)
        );

        // Se desvanece con sus propios materiales; el otro, igual que antes.
        app.world_mut().entity_mut(leaving).insert(Fading(0.6));
        app.update();
        let faded = looks(&mut app, leaving);
        assert_eq!(faded.len(), shared.len());
        for (id, alpha, mode) in &faded {
            assert!(shared.iter().all(|(s, ..)| s != id), "its own copy");
            assert!((alpha - 0.6).abs() < 1e-4, "{alpha}");
            assert_eq!(*mode, AlphaMode::Blend);
        }
        assert_eq!(looks(&mut app, other), shared);
        // Cada vez menos visible, con las mismas copias.
        app.world_mut().entity_mut(leaving).insert(Fading(0.2));
        app.update();
        let fainter = looks(&mut app, leaving);
        assert!(fainter.iter().all(|(_, a, _)| (a - 0.2).abs() < 1e-4));
        let ids = |l: &[(AssetId<StandardMaterial>, f32, AlphaMode)]| {
            l.iter().map(|(id, ..)| *id).collect::<Vec<_>>()
        };
        assert_eq!(ids(&fainter), ids(&faded));
        assert_eq!(looks(&mut app, other), shared);
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
        // La fase inicial depende de la entidad, así que se muestrea más de un
        // ciclo completo de andar: 24 frames de 50 ms a unos 1.08 ciclos/s.
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
        // A lo largo de un ciclo completo cada pierna llega cerca de su
        // recorrido completo en ambos sentidos, empiece donde empiece el ciclo.
        let full = pampa_core::pose::pose(AnimationState::Walk, 0.25).hip[0];
        let (high, low) = swings
            .iter()
            .fold((f32::MIN, f32::MAX), |(h, l), a| (h.max(*a), l.min(*a)));
        assert!(
            high > 0.9 * full && low < -0.9 * full,
            "{high} {low} of {full}"
        );
        // Mirando al oeste: el cuerpo giró media vuelta.
        let mut bodies = app
            .world_mut()
            .query::<(&FigureBody, &Transform, &ChildOf)>();
        let (_, body, parent) = bodies.single(app.world()).unwrap();
        assert_eq!(parent.parent(), me);
        assert!((body.rotation * Vec3::X).distance(Vec3::NEG_X) < 1e-4);
    }

    #[test]
    fn a_standing_figure_turns_its_head_toward_what_it_looks_at_never_too_far() {
        use super::super::figure::FigureBody;
        use super::super::rig::{GAZE_MAX, Gaze};
        let mut app = plugin_app();
        let me = app
            .world_mut()
            .spawn((
                CharacterVisual::Figure(FigureKind::Hidalgo),
                VisualPose::default(),
                Gaze(0.8),
            ))
            .id();
        let glance = |app: &mut App| {
            let mut q = app.world_mut().query::<&FigureBody>();
            q.single(app.world()).unwrap().glance
        };
        for _ in 0..30 {
            app.update();
        }
        assert!(
            (glance(&mut app) - 0.8).abs() < 0.02,
            "{}",
            glance(&mut app)
        );
        // Hacia atrás no: gira lo más que es natural.
        app.world_mut().entity_mut(me).insert(Gaze(-2.5));
        for _ in 0..30 {
            app.update();
        }
        assert!(
            (glance(&mut app) + GAZE_MAX).abs() < 0.02,
            "{}",
            glance(&mut app)
        );
    }

    /// Una app con modelos y una persona del pueblo sentada en un banco cuya
    /// parte de arriba está en `bench`, mirando hacia `heading`, ya en pose:
    /// la app, la entidad de la persona y la de su modelo.
    fn seat_townsperson(heading: Heading, bench: Vec3) -> (App, Entity, Entity) {
        use super::super::rider::Seated;
        use super::super::{Rig, model::CharacterModels};
        use crate::gltf::tests::{root, unfinished_app};
        let mut app = unfinished_app(&root());
        app.init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>()
            .add_plugins(bevy::animation::AnimationPlugin)
            .insert_resource(root())
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                50,
            )))
            .add_plugins((crate::fauna::FaunaPlugin, CharacterVisualPlugin));
        app.finish();
        app.cleanup();
        // Una cámara mirando hacia otro lado: quien se sienta, oculto hasta su
        // primera pose, no se congela por estar fuera de la vista.
        app.world_mut().spawn((
            Camera3d::default(),
            Transform::from_xyz(0.0, 2.0, 30.0).looking_at(Vec3::new(0.0, 2.0, 60.0), Vec3::Y),
        ));
        app.update();
        assert!(app.world().get_resource::<CharacterModels>().is_some());
        let me = app
            .world_mut()
            .spawn((
                CharacterVisual::Seated(FigureKind::townsfolk(3), Pastime::Rest),
                VisualPose {
                    heading,
                    ..default()
                },
                Transform::from_translation(bench),
            ))
            .id();
        let mut posed = None;
        for _ in 0..3000 {
            app.update();
            let world = app.world_mut();
            posed = world
                .query::<(Entity, &Rig, &Seated, &Visibility)>()
                .iter(world)
                .find(|(_, r, _, v)| r.started && **v == Visibility::Inherited)
                .map(|(e, ..)| e);
            if posed.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let model = posed.expect("the townsperson never sat");
        for _ in 0..10 {
            app.update();
        }
        (app, me, model)
    }

    #[test]
    fn a_seated_townsperson_sits_on_the_bench_facing_ahead_with_hands_in_the_lap() {
        use super::super::horse::tests::named;
        use super::super::rider::PELVIS_RISE;
        // La parte de arriba de un banco a 0.45 m del piso, mirando al noroeste.
        let heading = Heading::from_radians(2.2);
        let bench = Vec3::new(3.0, 0.5, -2.0);
        let (mut app, me, model) = seat_townsperson(heading, bench);
        let world = app.world_mut();
        let riding = *world.get::<Riding>(model).unwrap();
        assert_eq!(riding.kind, FigureKind::townsfolk(3));
        let seat = world.get::<ChildOf>(riding.horse).unwrap().parent();
        assert_eq!(seat, me, "the seat hangs from the sitter's entity");
        let at = |world: &World, bone: &str| {
            let e = named(world, model, bone);
            world.get::<GlobalTransform>(e).unwrap().translation()
        };
        let pelvis = at(world, "pelvis");
        assert!(
            pelvis.distance(bench + Vec3::Y * PELVIS_RISE) < 0.02,
            "pelvis at {pelvis} over the bench at {bench}"
        );
        let ahead = body_rotation(heading) * Vec3::X;
        let floor = bench.y - super::super::rider::BENCH_HEIGHT;
        let flat = |v: Vec3| Vec3::new(v.x, 0.0, v.z);
        for side in ["l", "r"] {
            // Las suelas en el piso, delante del banco, sin atravesarlo.
            let (foot, ball) = (
                at(world, &format!("foot_{side}")),
                at(world, &format!("ball_{side}")),
            );
            assert!(
                (0.0..0.05).contains(&(ball.y - floor)) && foot.y - floor > 0.04,
                "{side} foot at {:.3}, ball at {:.3} over the floor",
                foot.y - floor,
                ball.y - floor
            );
            assert!(flat(ball - pelvis).dot(ahead) > 0.3, "{side} foot ahead");
            // Las manos descansan sobre los muslos, no adelante en las riendas.
            let hand = at(world, &format!("hand_{side}"));
            let (up, out) = (hand.y - pelvis.y, flat(hand - pelvis).dot(ahead));
            assert!(
                (0.0..0.3).contains(&up) && (-0.1..0.25).contains(&out),
                "{side} hand {up:.3} up and {out:.3} ahead of the pelvis"
            );
        }
    }

    #[test]
    fn seated_on_a_bench_he_turns_his_head_toward_what_he_looks_at_up_to_seventy_degrees() {
        use super::super::horse::tests::named;
        use super::super::rig::{GAZE_MAX, Gaze};
        let (mut app, me, model) =
            seat_townsperson(Heading::from_radians(2.2), Vec3::new(3.0, 0.5, -2.0));
        let world = app.world_mut();
        let camera = world
            .query_filtered::<Entity, With<Camera3d>>()
            .single(world)
            .unwrap();
        world.despawn(camera);
        let head = |app: &mut App| {
            let e = named(app.world(), model, "head");
            app.world().get::<GlobalTransform>(e).unwrap().rotation()
        };
        // Cuánto giró la cabeza alrededor de la vertical, positivo a la
        // izquierda, respecto de como la tenía mirando al frente.
        let ahead = head(&mut app);
        let turned = |app: &mut App| {
            let now = head(app) * ahead.inverse();
            let f = now * Vec3::X;
            let (x, z) = (f.x, f.z);
            (-z).atan2(x)
        };
        for (gaze, want) in [(0.9, 0.9), (2.8, GAZE_MAX), (-1.0, -1.0)] {
            app.world_mut().entity_mut(me).insert(Gaze(gaze));
            for _ in 0..40 {
                app.update();
            }
            let got = turned(&mut app);
            assert!((got - want).abs() < 0.15, "gaze {gaze}: turned {got}");
        }
        // Sin nada que mirar, vuelve al frente.
        app.world_mut().entity_mut(me).remove::<Gaze>();
        for _ in 0..40 {
            app.update();
        }
        assert!(turned(&mut app).abs() < 0.15, "{}", turned(&mut app));
    }

    #[test]
    fn standing_up_from_the_bench_the_pelvis_rises_and_the_feet_stay_on_the_floor() {
        use super::super::horse::tests::named;
        use super::super::rider::{BENCH_HEIGHT, PELVIS_RISE, Rising};
        let bench = Vec3::new(3.0, 0.5, -2.0);
        let floor = bench.y - BENCH_HEIGHT;
        let (mut app, me, model) = seat_townsperson(Heading::from_radians(2.2), bench);
        // Sin la cámara que mira hacia otro lado, nadie queda fuera de la vista:
        // su modelo sigue animado.
        let world = app.world_mut();
        let camera = world
            .query_filtered::<Entity, With<Camera3d>>()
            .single(world)
            .unwrap();
        world.despawn(camera);
        let mut last = bench.y + PELVIS_RISE;
        for rise in [0.5, 1.0] {
            app.world_mut().entity_mut(me).insert(Rising(rise));
            for _ in 0..10 {
                app.update();
            }
            let world = app.world_mut();
            let at = |world: &World, bone: &str| {
                let e = named(world, model, bone);
                world.get::<GlobalTransform>(e).unwrap().translation()
            };
            let pelvis = at(world, "pelvis").y;
            assert!(
                pelvis > last + 0.02,
                "at {rise} the pelvis at {pelvis}, before at {last}"
            );
            last = pelvis;
            for side in ["l", "r"] {
                let ball = at(world, &format!("ball_{side}")).y - floor;
                assert!(
                    (0.0..0.05).contains(&ball),
                    "at {rise} the {side} ball {ball:.3} over the floor"
                );
            }
        }
        // De pie: la pelvis a la altura de una persona parada, no la de una
        // sentada en el banco (0.58 m).
        assert!(
            last - floor > 0.7,
            "standing pelvis {:.3} over the floor",
            last - floor
        );
    }
}
