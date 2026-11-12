//! Lo que tiene en la mano quien pasa el rato sentado en un banco
//! ([`OnBench`]), según su [`Pastime`]:
//!
//! - el que fuma, un cigarrillo armado entre el índice y el mayor de la mano
//!   derecha, con la brasa encendida, que larga [`PUFFS`] bocanadas de humo
//!   que suben, se hinchan y se deshacen;
//! - el que toma, un vaso de caña en la mano derecha, que se mantiene derecho
//!   y se inclina hacia la boca cuando la mano llega a ella ([`tilt_glasses`]);
//! - el que juega al truco, su mano de tres naipes españoles en abanico en la
//!   mano izquierda.
//!
//! Las cosas se cuelgan de los huesos de la mano en cuanto el modelo tiene su
//! [`Rig`] (otra vez, si su escena se vuelve a crear), en el marco del hueso:
//! `+Y` hacia los dedos, los nudillos a unos 10 cm de la muñeca; la palma mira
//! hacia [`palm`] y el lado del índice queda hacia [`across`].

use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use pampa_core::pulperia::Pastime;

use super::rider::OnBench;
use super::rig::Rig;

/// Cuántas bocanadas de humo larga a la vez un cigarrillo.
pub const PUFFS: usize = 5;

/// Cuánto vive una bocanada de humo, en segundos, y cuánto sube por segundo,
/// en metros.
const PUFF_LIFE: f32 = 2.4;
const PUFF_RISE: f32 = 0.16;

/// El largo y el diámetro de un cigarrillo armado, en metros.
const CIGARETTE: (f32, f32) = (0.07, 0.008);

/// El alto y el diámetro de un vaso, en metros, y hasta qué alto lo llena la
/// caña.
const GLASS: (f32, f32) = (0.095, 0.062);
const POUR: f32 = 0.6;

/// Un naipe español, ancho por alto, en metros.
const CARD: (f32, f32) = (0.06, 0.092);

/// Cuánto se inclina un vaso al tomar, en radianes, con la mano en la boca.
const SIP_TILT: f32 = 1.1;

/// Hacia dónde mira la palma de la mano de un lado (`right`), en el marco del
/// hueso de la mano: medido en los modelos de MakeHuman, de la palma hacia
/// afuera de ella.
pub fn palm(right: bool) -> Vec3 {
    Vec3::new(if right { -0.85 } else { 0.85 }, 0.0, 0.55).normalize()
}

/// Hacia el lado del índice, a lo ancho de los nudillos, en el marco del hueso
/// de la mano de un lado (`right`).
pub fn across(right: bool) -> Vec3 {
    Vec3::new(if right { 0.55 } else { -0.55 }, 0.0, 0.85).normalize()
}

/// Las mallas y los materiales de las cosas en la mano, compartidos.
#[derive(Resource, Debug, Clone)]
pub struct PastimeKit {
    cigarette: Handle<Mesh>,
    paper: Handle<StandardMaterial>,
    ember: Handle<StandardMaterial>,
    glass: Handle<Mesh>,
    glass_skin: Handle<StandardMaterial>,
    drink: Handle<Mesh>,
    cane: Handle<StandardMaterial>,
    card: Handle<Mesh>,
    card_face: Handle<StandardMaterial>,
    puff: Handle<Mesh>,
    smoke: Handle<Image>,
}

/// Una cosa colgada de la mano de alguien sentado, por su [`Pastime`].
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Held(pub Pastime);

/// La brasa de un cigarrillo: de ahí sale el humo.
#[derive(Component, Debug, Clone, Copy)]
pub struct Ember;

/// Un vaso en la mano, que se mantiene derecho: `head` es el hueso de la
/// cabeza de quien lo tiene, hacia donde se inclina al tomar.
#[derive(Component, Debug, Clone, Copy)]
pub struct HeldGlass {
    pub head: Entity,
}

/// Una bocanada de humo de la brasa `ember`: su edad en segundos, dónde nació
/// y su material (su transparencia es suya).
#[derive(Component, Debug, Clone)]
pub struct Puff {
    pub ember: Entity,
    pub age: f32,
    pub origin: Vec3,
    material: Handle<StandardMaterial>,
}

pub(super) fn make_kit(
    mut commands: Commands,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
    images: Option<ResMut<Assets<Image>>>,
) {
    let (Some(mut meshes), Some(mut materials), Some(mut images)) = (meshes, materials, images)
    else {
        return;
    };
    // Un cuadrado de pie (en el plano XY, mirando a +Z) que se gira hacia la
    // cámara.
    let quad = Mesh::from(Rectangle::new(1.0, 1.0));
    commands.insert_resource(PastimeKit {
        cigarette: meshes.add(Cylinder::new(CIGARETTE.1 / 2.0, CIGARETTE.0)),
        // Papel de chala, apenas amarillento.
        paper: materials.add(StandardMaterial {
            base_color: Color::srgb(0.86, 0.82, 0.70),
            perceptual_roughness: 0.9,
            ..default()
        }),
        ember: materials.add(StandardMaterial {
            base_color: Color::srgb(0.35, 0.12, 0.05),
            emissive: LinearRgba::rgb(6.0, 1.4, 0.2),
            ..default()
        }),
        glass: meshes.add(Cylinder::new(GLASS.1 / 2.0, GLASS.0)),
        glass_skin: materials.add(StandardMaterial {
            base_color: Color::srgba(0.80, 0.88, 0.82, 0.28),
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 0.08,
            reflectance: 0.6,
            ..default()
        }),
        drink: meshes.add(Cylinder::new(GLASS.1 / 2.0 - 0.004, GLASS.0 * POUR)),
        // Caña quemada: ámbar claro.
        cane: materials.add(StandardMaterial {
            base_color: Color::srgba(0.78, 0.52, 0.18, 0.75),
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 0.1,
            ..default()
        }),
        card: meshes.add(Cuboid::new(CARD.0, 0.0015, CARD.1)),
        // El dorso de los naipes españoles: un tostado rojizo.
        card_face: materials.add(StandardMaterial {
            base_color: Color::srgb(0.62, 0.30, 0.22),
            perceptual_roughness: 0.8,
            ..default()
        }),
        puff: meshes.add(quad),
        smoke: images.add(smoke_image()),
    });
}

/// Una mancha de humo redonda y blanda, de borde difuso.
fn smoke_image() -> Image {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    const N: u32 = 32;
    let mut data = Vec::with_capacity((N * N * 4) as usize);
    for y in 0..N {
        for x in 0..N {
            let u = (x as f32 + 0.5) / N as f32 * 2.0 - 1.0;
            let v = (y as f32 + 0.5) / N as f32 * 2.0 - 1.0;
            let r2 = u * u + v * v;
            let a = (-(r2 * 3.5)).exp() * (1.0 - r2).max(0.0);
            data.extend_from_slice(&[215, 218, 222, (a * 255.0) as u8]);
        }
    }
    Image::new(
        Extent3d {
            width: N,
            height: N,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// Cuelga de las manos de cada modelo sentado que acaba de tener su [`Rig`] lo
/// que tiene en ellas por su [`Pastime`].
pub(super) fn hold_things(
    mut commands: Commands,
    kit: Option<Res<PastimeKit>>,
    sitters: Query<(Entity, &OnBench), Added<Rig>>,
    children: Query<&Children>,
    names: Query<&Name>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    let Some(kit) = kit else {
        return;
    };
    for (root, bench) in &sitters {
        let named = |name: &str| {
            children
                .iter_descendants(root)
                .find(|e| names.get(*e).is_ok_and(|n| n.as_str() == name))
        };
        let pastime = bench.0;
        match pastime {
            Pastime::Smoke => {
                let Some(hand) = named("hand_r") else { continue };
                let ember = hold_cigarette(&mut commands, &kit, hand);
                if let Some(materials) = materials.as_deref_mut() {
                    for k in 0..PUFFS {
                        spawn_puff(&mut commands, &kit, materials, ember, k);
                    }
                }
            }
            Pastime::Drink => {
                let (Some(hand), Some(head)) = (named("hand_r"), named("head")) else {
                    continue;
                };
                hold_glass(&mut commands, &kit, hand, head);
            }
            Pastime::Cards => {
                let Some(hand) = named("hand_l") else { continue };
                hold_cards(&mut commands, &kit, hand);
            }
            Pastime::Rest | Pastime::Chat => {}
        }
    }
}

/// El cigarrillo en la mano derecha `hand`: cruza la mano entre el índice y el
/// mayor, a la altura de los nudillos, la punta de la boca apenas del lado de
/// la palma, que el gesto de fumar vuelve hacia los labios, y la brasa del lado
/// del dorso, lejos de la cara. Devuelve la brasa.
fn hold_cigarette(commands: &mut Commands, kit: &PastimeKit, hand: Entity) -> Entity {
    let (p, a) = (palm(true), across(true));
    let at = Vec3::Y * 0.105 + a * 0.012 - p * (CIGARETTE.0 / 2.0 - 0.012);
    let along = Quat::from_rotation_arc(Vec3::Y, -p);
    let cigarette = commands
        .spawn((
            Name::new("Cigarette"),
            Held(Pastime::Smoke),
            Mesh3d(kit.cigarette.clone()),
            MeshMaterial3d(kit.paper.clone()),
            Transform::from_translation(at).with_rotation(along),
            NotShadowCaster,
            ChildOf(hand),
        ))
        .id();
    commands
        .spawn((
            Name::new("Ember"),
            Ember,
            Mesh3d(kit.cigarette.clone()),
            MeshMaterial3d(kit.ember.clone()),
            Transform::from_translation(Vec3::Y * (CIGARETTE.0 / 2.0))
                .with_scale(Vec3::new(1.05, 0.08, 1.05)),
            NotShadowCaster,
            ChildOf(cigarette),
        ))
        .id()
}

/// El vaso de caña en la mano derecha `hand`, delante de la palma, abrazado por
/// los dedos.
fn hold_glass(commands: &mut Commands, kit: &PastimeKit, hand: Entity, head: Entity) {
    let at = Vec3::Y * 0.075 + palm(true) * (GLASS.1 / 2.0 + 0.012);
    let glass = commands
        .spawn((
            Name::new("Glass"),
            Held(Pastime::Drink),
            HeldGlass { head },
            Mesh3d(kit.glass.clone()),
            MeshMaterial3d(kit.glass_skin.clone()),
            Transform::from_translation(at),
            NotShadowCaster,
            ChildOf(hand),
        ))
        .id();
    commands.spawn((
        Name::new("Cane"),
        Mesh3d(kit.drink.clone()),
        MeshMaterial3d(kit.cane.clone()),
        Transform::from_translation(Vec3::Y * (GLASS.0 * (POUR - 1.0) / 2.0 + 0.004)),
        NotShadowCaster,
        ChildOf(glass),
    ));
}

/// La mano de tres naipes en la mano izquierda `hand`: en abanico desde la
/// base de los dedos, el dorso hacia afuera de la palma.
fn hold_cards(commands: &mut Commands, kit: &PastimeKit, hand: Entity) {
    let (p, a) = (palm(false), across(false));
    // El naipe se acuesta en el plano de la palma: su cara mira hacia la
    // palma, su largo sigue a los dedos.
    let face = Quat::from_mat3(&Mat3::from_cols(a, p, a.cross(p)));
    for k in 0..3 {
        let fan = (k as f32 - 1.0) * 0.28;
        let turn = Quat::from_axis_angle(p, fan);
        let at = Vec3::Y * 0.085 + p * (0.012 + 0.002 * k as f32) + turn * (Vec3::Y * 0.02);
        commands.spawn((
            Name::new("Card"),
            Held(Pastime::Cards),
            Mesh3d(kit.card.clone()),
            MeshMaterial3d(kit.card_face.clone()),
            Transform::from_translation(at).with_rotation(turn * face),
            NotShadowCaster,
            ChildOf(hand),
        ));
    }
}

/// Una bocanada de humo, la `k`-ésima de la brasa `ember`: nace escalonada,
/// para que el humo salga parejo.
fn spawn_puff(
    commands: &mut Commands,
    kit: &PastimeKit,
    materials: &mut Assets<StandardMaterial>,
    ember: Entity,
    k: usize,
) {
    let material = materials.add(StandardMaterial {
        // Sin sombrear: en la penumbra de la sala, el humo a la luz del farol
        // se ve gris claro, no se apaga con la pared de atrás.
        base_color: Color::srgba(0.62, 0.62, 0.64, 0.0),
        base_color_texture: Some(kit.smoke.clone()),
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        unlit: true,
        ..default()
    });
    commands.spawn((
        Name::new("Smoke puff"),
        Puff {
            ember,
            age: PUFF_LIFE * k as f32 / PUFFS as f32,
            origin: Vec3::NAN,
            material: material.clone(),
        },
        Mesh3d(kit.puff.clone()),
        MeshMaterial3d(material),
        Transform::default(),
        Visibility::Hidden,
        NotShadowCaster,
        NotShadowReceiver,
    ));
}

/// Qué tan a la vista está una bocanada de `age` segundos: aparece rápido y se
/// deshace de a poco.
pub fn puff_alpha(age: f32) -> f32 {
    let t = (age / PUFF_LIFE).clamp(0.0, 1.0);
    (t / 0.12).min(1.0) * (1.0 - t).powf(1.5) * 0.6
}

/// Mueve el humo: cada bocanada sube desde donde nació, se hincha, se ladea con
/// una brisa suave y se deshace; al morir vuelve a nacer en la brasa. Se
/// borra con su brasa.
pub(super) fn drift_smoke(
    mut commands: Commands,
    time: Res<Time>,
    embers: Query<&GlobalTransform, With<Ember>>,
    cameras: Query<&GlobalTransform, With<Camera3d>>,
    mut puffs: Query<(Entity, &mut Puff, &mut Transform, &mut Visibility)>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    let dt = time.delta_secs();
    let eye = cameras.iter().next().map(GlobalTransform::translation);
    for (entity, mut puff, mut transform, mut visibility) in &mut puffs {
        let Ok(ember) = embers.get(puff.ember) else {
            commands.entity(entity).despawn();
            continue;
        };
        puff.age += dt;
        if puff.age >= PUFF_LIFE || !puff.origin.is_finite() {
            puff.age %= PUFF_LIFE;
            puff.origin = ember.translation();
        }
        let t = puff.age / PUFF_LIFE;
        let sway = Vec3::new((puff.age * 2.1).sin(), 0.0, (puff.age * 1.3).cos()) * 0.015 * t;
        let at = puff.origin + Vec3::Y * (PUFF_RISE * puff.age) + sway;
        let size = 0.04 + 0.18 * t;
        let facing = eye.map_or(Quat::IDENTITY, |e| {
            let d = e - at;
            Quat::from_rotation_y(d.x.atan2(d.z))
        });
        *transform = Transform::from_translation(at)
            .with_rotation(facing)
            .with_scale(Vec3::splat(size));
        visibility.set_if_neq(Visibility::Inherited);
        if let Some(mut material) = materials
            .as_deref_mut()
            .and_then(|m| m.get_mut(&puff.material))
        {
            material.base_color.set_alpha(puff_alpha(puff.age));
        }
    }
}

/// Endereza cada vaso en la mano: derecho mientras la mano está abajo, y
/// volcado hacia la cara hasta [`SIP_TILT`] a medida que la mano sube hasta la
/// boca. Corre después de propagar los transforms, con la pose del frame, y
/// ajusta el giro del vaso bajo la mano para el próximo.
pub(super) fn tilt_glasses(
    mut glasses: Query<(&HeldGlass, &ChildOf, &mut Transform)>,
    globals: Query<&GlobalTransform>,
) {
    for (glass, hand, mut transform) in &mut glasses {
        let (Ok(hand), Ok(head)) = (globals.get(hand.parent()), globals.get(glass.head)) else {
            continue;
        };
        let at = hand.transform_point(transform.translation);
        let head = head.translation();
        let up_to_mouth = ((at.y - (head.y - 0.45)) / 0.4).clamp(0.0, 1.0);
        let toward = (head - at).with_y(0.0).normalize_or(Vec3::Z);
        let tilt = Quat::from_axis_angle(Vec3::Y.cross(toward).normalize_or(Vec3::X), SIP_TILT * up_to_mouth);
        let (_, hand_rotation, _) = hand.to_scale_rotation_translation();
        transform.rotation = (hand_rotation.inverse() * tilt).normalize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            bevy::transform::TransformPlugin,
        ))
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .init_asset::<Image>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(50)))
        .add_systems(Startup, make_kit)
        .add_systems(Update, hold_things)
        .add_systems(
            PostUpdate,
            (drift_smoke, tilt_glasses).after(TransformSystems::Propagate),
        );
        app
    }

    /// Un modelo sentado con `pastime`, con las dos manos y la cabeza, que
    /// recibe su rig.
    fn sitter(app: &mut App, pastime: Pastime) -> Entity {
        let world = app.world_mut();
        let root = world
            .spawn((OnBench(pastime), Transform::default(), Visibility::default()))
            .id();
        for (name, at) in [
            ("hand_r", Vec3::new(-0.2, 0.7, 0.1)),
            ("hand_l", Vec3::new(0.2, 0.7, 0.1)),
            ("head", Vec3::new(0.0, 1.2, 0.0)),
        ] {
            world.spawn((Name::new(name), Transform::from_translation(at), ChildOf(root)));
        }
        world.entity_mut(root).insert(Rig::new(root));
        root
    }

    fn held(app: &mut App, under: Entity) -> Vec<Pastime> {
        let world = app.world_mut();
        let mut out = Vec::new();
        let mut q = world.query::<(&Held, &ChildOf)>();
        for (h, parent) in q.iter(world) {
            let name = world.get::<Name>(parent.parent()).map(|n| n.as_str().to_owned());
            if world.get::<ChildOf>(parent.parent()).map(ChildOf::parent) == Some(under) {
                out.push((h.0, name));
            }
        }
        out.sort_by_key(|(p, _)| format!("{p:?}"));
        out.into_iter()
            .map(|(p, name)| {
                let hand = if p == Pastime::Cards { "hand_l" } else { "hand_r" };
                assert_eq!(name.as_deref(), Some(hand), "{p:?}");
                p
            })
            .collect()
    }

    #[test]
    fn each_sitter_holds_the_things_of_what_they_do_and_the_smoke_goes_with_the_cigarette() {
        let mut app = app();
        app.update();
        let smoker = sitter(&mut app, Pastime::Smoke);
        let drinker = sitter(&mut app, Pastime::Drink);
        let player = sitter(&mut app, Pastime::Cards);
        let talker = sitter(&mut app, Pastime::Chat);
        let resting = sitter(&mut app, Pastime::Rest);
        for _ in 0..20 {
            app.update();
        }
        assert_eq!(held(&mut app, smoker), [Pastime::Smoke]);
        assert_eq!(held(&mut app, drinker), [Pastime::Drink]);
        assert_eq!(held(&mut app, player), [Pastime::Cards; 3]);
        assert!(held(&mut app, talker).is_empty());
        assert!(held(&mut app, resting).is_empty());

        // El humo sale de la brasa, sube y se ve.
        let world = app.world_mut();
        let ember = world
            .query_filtered::<&GlobalTransform, With<Ember>>()
            .single(world)
            .unwrap()
            .translation();
        let mut puffs = world.query::<(&Puff, &GlobalTransform, &Visibility)>();
        let materials = world.resource::<Assets<StandardMaterial>>();
        let mut shown = 0;
        for (puff, at, visibility) in puffs.iter(world) {
            assert_eq!(*visibility, Visibility::Inherited);
            assert!(at.translation().y >= ember.y - 0.03, "{at:?}");
            assert!(at.translation().xz().distance(ember.xz()) < 0.1);
            let alpha = materials.get(&puff.material).unwrap().base_color.alpha();
            shown += usize::from(alpha > 0.1);
        }
        assert_eq!(puffs.iter(world).count(), PUFFS);
        assert!(shown >= 2, "{shown} puffs show");

        // Al irse el que fuma, su humo se va con él.
        app.world_mut().entity_mut(smoker).despawn();
        app.update();
        let world = app.world_mut();
        assert_eq!(world.query::<&Puff>().iter(world).count(), 0);
    }
}
