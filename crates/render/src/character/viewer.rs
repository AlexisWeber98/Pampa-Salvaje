//! El propio cuerpo del jugador, visto desde adentro por una cámara en primera
//! persona.
//!
//! El juego marca la entidad del jugador con [`ViewerBody`]. Cuando se prepara
//! un modelo de personaje bajo ella (entra su [`Rig`], a pie o en la montura),
//! [`mark_viewer_head`] le hace tres cosas:
//!
//! - marca el hueso `head` con [`ViewerHead`]: la cámara se ubica en
//!   [`EYE_IN_HEAD`] en su marco;
//! - mueve las meshes alrededor de los ojos (el sombrero y su cinta, el pelo,
//!   los ojos y las cejas; ver [`is_head_piece`]) solo a la capa de render
//!   [`OWN_HEAD_LAYER`];
//! - recorta la cabeza de la piel ([`is_skin`]) según sus pesos de skinning
//!   ([`split_head`]): los triángulos con peso mayoritario en el hueso `head` y
//!   la parte superior del cuello ([`head_share`]) pasan a una mesh propia
//!   ([`OwnHead`]), con skinning de las mismas articulaciones, también en
//!   [`OWN_HEAD_LAYER`]. El corte rodea el cuello por dentro del cuello de la
//!   camisa y del pañuelo. Las dos mitades se hacen una vez por asset de piel y
//!   las comparten todas las preparaciones posteriores; no se hace nada por
//!   frame.
//!
//! Todas las demás meshes del modelo van en [`OWN_BODY_LAYER`].
//!
//! Una cámara que ve [`world_layers`] lo dibuja todo; una cámara en primera
//! persona descarta [`OWN_HEAD_LAYER`] ([`first_person_layers`]), así que
//! ninguna parte de la cara o del cráneo puede pasar por delante de ella, se
//! mueva como se mueva la cabeza. Una cámara orbital acercada hasta pegarse al
//! jugador descarta ambas ([`close_layers`]), así que su ropa nunca llena la
//! pantalla. Cada luz (el sol y los faroles de la calle) está también en ambas
//! capas, así que el cuerpo entero sigue en sus shadow maps y proyecta la
//! sombra del sol. Los brazos, las manos, el cuello de la camisa, las riendas y
//! el cuello y las orejas del caballo siguen a la vista.

use std::collections::HashMap;

use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::{RenderLayers, VisibilityRange};
use bevy::gltf::GltfMaterialName;
use bevy::mesh::skinning::SkinnedMesh;
use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::prelude::*;

use super::rig::Rig;

/// La capa de render de la propia cabeza del jugador (la cara y el cráneo, el
/// sombrero, el pelo y los ojos): oculta para una cámara en primera persona,
/// dibujada por todas las demás cámaras e iluminada por el sol.
pub const OWN_HEAD_LAYER: usize = 1;

/// La capa de render del resto del propio modelo del jugador (el cuerpo, la
/// ropa, las manos): dibujada desde los ojos, oculta para una cámara orbital
/// acercada demasiado, iluminada por el sol.
pub const OWN_BODY_LAYER: usize = 2;

/// Los ojos en el marco del hueso `head`, en metros: entre ellos y apenas por
/// delante, medido sobre el modelo del protagonista (el centro de sus ojos está
/// 0.04 m por encima y 0.09 m por delante del hueso, y el frente de los ojos a
/// 0.10 m). El marco de reposo del hueso es el del modelo (+Z hacia adelante,
/// +Y hacia arriba).
pub const EYE_IN_HEAD: Vec3 = Vec3::new(0.0, 0.04, 0.11);

/// Cuánto cuenta para la cabeza el peso de un vertex en `neck_01` cuando se
/// corta la piel ([`head_share`]). Con solo el peso de la cabeza el corte pasa
/// por debajo de la mandíbula y deja el cuello levantado hasta 1.61 m, 8 cm por
/// encima del cuello de la camisa; con 0.75 rodea el cuello a entre 1.50 y
/// 1.53 m en el protagonista, por dentro del cuello de su camisa (hasta 1.53 m)
/// y de su pañuelo (hasta 1.56 m).
pub const NECK_SHARE: f32 = 0.75;

/// Un triángulo cuyos vertices tienen un [`head_share`] promedio igual o mayor
/// que este va con la cabeza.
pub const HEAD_DOMINANT: f32 = 0.5;

/// El personaje en el que se ubica la cámara en primera persona: el jugador.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct ViewerBody;

/// El hueso `head` del modelo de un [`ViewerBody`].
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct ViewerHead;

/// La cara y el cráneo recortados de la piel de un [`ViewerBody`]
/// ([`split_head`]).
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct OwnHead;

/// Las capas que ve una cámara para dibujar todo, también la cabeza del
/// jugador.
pub fn world_layers() -> RenderLayers {
    RenderLayers::from_layers(&[0, OWN_HEAD_LAYER, OWN_BODY_LAYER])
}

/// Las capas que ve una cámara en primera persona: todo menos la propia cabeza
/// del jugador.
pub fn first_person_layers() -> RenderLayers {
    RenderLayers::from_layers(&[0, OWN_BODY_LAYER])
}

/// Las capas que ve una cámara orbital acercada contra el jugador: todo menos
/// el propio modelo del jugador.
pub fn close_layers() -> RenderLayers {
    RenderLayers::layer(0)
}

/// Si la mesh del modelo con `material` está alrededor de los ojos: un
/// sombrero, su cinta, el pelo, los ojos, las cejas y las pestañas.
pub fn is_head_piece(material: &str) -> bool {
    let lower = material.to_ascii_lowercase();
    ["hat", "eye", "hair", "short0", "kepi", "boina"]
        .iter()
        .any(|p| lower.starts_with(p))
}

/// Las edades con las que se nombran los materiales de piel de MakeHuman.
const SKIN_AGES: [&str; 3] = ["young", "middleage", "old"];

/// Las pieles étnicas con las que MakeHuman nombra sus materiales de piel,
/// incluida la errata `cauasian` con la que vienen sus pieles de mediana edad
/// (nuestros modelos `middleage_cauasian_male`).
const SKIN_KINDS: [&str; 4] = ["caucasian", "cauasian", "african", "asian"];

/// Si la mesh del modelo con `material` es la piel: la única mesh de la que se
/// recorta la cabeza. MakeHuman la nombra según la piel, exactamente
/// `<age>_<kind>_<male|female>`, quizás con `_detailed` (como
/// `old_caucasian_male_detailed`; ver [`SKIN_AGES`] y [`SKIN_KINDS`]). La ropa
/// conserva enteros sus cuellos, se llame como se llame.
pub fn is_skin(material: &str) -> bool {
    let mut parts = material.split('_');
    let (Some(age), Some(kind), Some(sex)) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    let detail = parts.next();
    parts.next().is_none()
        && matches!(detail, None | Some("detailed"))
        && SKIN_AGES.contains(&age)
        && SKIN_KINDS.contains(&kind)
        && matches!(sex, "male" | "female")
}

/// Cuánto de un vertex va con la cabeza: sus pesos en las articulaciones de la
/// cabeza, y [`NECK_SHARE`] de los del cuello, según la proporción de cada una
/// de las articulaciones de la piel (`shares`, en el orden de sus
/// articulaciones).
pub fn head_share(joints: [u16; 4], weights: [f32; 4], shares: &[f32]) -> f32 {
    joints
        .iter()
        .zip(weights)
        .map(|(&j, w)| shares.get(usize::from(j)).copied().unwrap_or(0.0) * w)
        .sum()
}

/// Si el triángulo `triangle` de `mesh` va con la cabeza: el [`head_share`] de
/// sus vertices según las proporciones `shares` de las articulaciones promedia
/// [`HEAD_DOMINANT`] o más.
pub fn head_dominant(mesh: &Mesh, triangle: [u32; 3], shares: &[f32]) -> bool {
    let (
        Some(VertexAttributeValues::Uint16x4(joints)),
        Some(VertexAttributeValues::Float32x4(weights)),
    ) = (
        mesh.attribute(Mesh::ATTRIBUTE_JOINT_INDEX),
        mesh.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT),
    )
    else {
        return false;
    };
    let share = |i: u32| {
        let i = i as usize;
        match (joints.get(i), weights.get(i)) {
            (Some(j), Some(w)) => head_share(*j, *w, shares),
            _ => 0.0,
        }
    };
    triangle.into_iter().map(share).sum::<f32>() / 3.0 >= HEAD_DOMINANT
}

/// Los triángulos de `mesh`, por índice de vertex.
pub fn triangles(mesh: &Mesh) -> Vec<[u32; 3]> {
    let indices: Vec<u32> = match mesh.indices() {
        Some(indices) => indices.iter().map(|i| i as u32).collect(),
        None => (0..mesh.count_vertices() as u32).collect(),
    };
    indices.as_chunks::<3>().0.to_vec()
}

/// Corta `mesh` (una piel con skinning) en dos según [`head_dominant`] sobre
/// las proporciones `shares` de las articulaciones de su piel: el resto del
/// cuerpo y la cabeza, cada una con una copia de cada vertex y sus propios
/// triángulos, así que ambas conservan el mismo skinning. `None` si nada va con
/// la cabeza.
pub fn split_head(mesh: &Mesh, shares: &[f32]) -> Option<(Mesh, Mesh)> {
    let (mut body, mut head) = (Vec::new(), Vec::new());
    for triangle in triangles(mesh) {
        let to = if head_dominant(mesh, triangle, shares) {
            &mut head
        } else {
            &mut body
        };
        to.extend_from_slice(&triangle);
    }
    if head.is_empty() {
        return None;
    }
    let with = |triangles: Vec<u32>| {
        let mut part = mesh.clone();
        part.insert_indices(Indices::U32(triangles));
        part
    };
    Some((with(body), with(head)))
}

/// Una entidad de mesh de un modelo, tal como la lee [`mark_viewer_head`].
type ModelMesh<'a> = (
    &'a Mesh3d,
    &'a GltfMaterialName,
    Option<&'a SkinnedMesh>,
    Option<&'a MeshMaterial3d<StandardMaterial>>,
    Option<&'a Transform>,
    Option<&'a ChildOf>,
    Option<&'a Aabb>,
    Option<&'a VisibilityRange>,
);

/// Las dos mitades hechas hasta ahora de cada asset de piel, por el asset de la
/// piel.
#[derive(Default)]
pub(super) struct Cuts(HashMap<AssetId<Mesh>, (Handle<Mesh>, Handle<Mesh>)>);

impl Cuts {
    /// Si `id` es una mitad ya cortada (una escena preparada de nuevo).
    fn is_cut(&self, id: AssetId<Mesh>) -> bool {
        self.0
            .values()
            .any(|(body, head)| body.id() == id || head.id() == id)
    }
}

/// Marca el hueso de la cabeza, mueve las piezas de la cabeza (y el resto a
/// [`OWN_BODY_LAYER`]) y recorta la cabeza de la piel del modelo de un
/// [`ViewerBody`] en el momento en que entra su [`Rig`] (cada vez que se
/// prepara su escena).
#[allow(clippy::too_many_arguments)]
pub(super) fn mark_viewer_head(
    rigged: On<Insert<Rig>>,
    mut commands: Commands,
    parents: Query<&ChildOf>,
    viewers: Query<(), With<ViewerBody>>,
    children: Query<&Children>,
    names: Query<&Name>,
    marked: Query<Entity, With<ViewerHead>>,
    model_meshes: Query<ModelMesh>,
    mut meshes: Option<ResMut<Assets<Mesh>>>,
    mut cuts: Local<Cuts>,
) {
    let root = rigged.entity;
    let Some(viewer) = parents.iter_ancestors(root).find(|e| viewers.contains(*e)) else {
        return;
    };
    let named = |name: &str| {
        children
            .iter_descendants(root)
            .find(|e| names.get(*e).is_ok_and(|n| n.as_str() == name))
    };
    let (head, neck) = (named("head"), named("neck_01"));
    if let Some(head) = head {
        commands.entity(head).try_insert(ViewerHead);
        // Una cabeza por cuerpo: el último modelo preparado. A otro que todavía
        // esté debajo (un modelo viejo mientras se reemplaza) se le quita la
        // marca.
        for old in &marked {
            if old != head && parents.iter_ancestors(old).any(|a| a == viewer) {
                commands.entity(old).try_remove::<ViewerHead>();
            }
        }
    }
    // La proporción de cada articulación en la cabeza: la cabeza y todo lo que
    // cuelga de ella por completo, el cuello en parte.
    let share = |joint: Entity| {
        let under_head =
            head.is_some_and(|h| joint == h || parents.iter_ancestors(joint).any(|a| a == h));
        if under_head {
            1.0
        } else if Some(joint) == neck {
            NECK_SHARE
        } else {
            0.0
        }
    };
    for entity in children.iter_descendants(root) {
        let Ok((mesh, material, skin, look, place, parent, aabb, range)) = model_meshes.get(entity)
        else {
            continue;
        };
        if is_head_piece(&material.0) {
            commands
                .entity(entity)
                .try_insert(RenderLayers::layer(OWN_HEAD_LAYER));
            continue;
        }
        commands
            .entity(entity)
            .try_insert(RenderLayers::layer(OWN_BODY_LAYER));
        let (Some(skin), Some(meshes), true) = (skin, meshes.as_deref_mut(), is_skin(&material.0))
        else {
            continue;
        };
        if cuts.is_cut(mesh.id()) {
            continue;
        }
        let made = match cuts.0.get(&mesh.id()) {
            Some(made) => made.clone(),
            None => {
                let shares: Vec<f32> = skin.joints.iter().map(|&j| share(j)).collect();
                let Some((body, face)) = meshes.get(&mesh.0).and_then(|m| split_head(m, &shares))
                else {
                    continue;
                };
                let made = (meshes.add(body), meshes.add(face));
                cuts.0.insert(mesh.id(), made.clone());
                made
            }
        };
        commands.entity(entity).try_insert(Mesh3d(made.0));
        let mut own = commands.spawn((
            Name::new("Own head"),
            OwnHead,
            Mesh3d(made.1),
            material.clone(),
            skin.clone(),
            place.copied().unwrap_or_default(),
            RenderLayers::layer(OWN_HEAD_LAYER),
        ));
        if let Some(look) = look {
            own.insert(look.clone());
        }
        if let Some(aabb) = aabb {
            own.insert(*aabb);
        }
        if let Some(range) = range {
            own.insert(range.clone());
        }
        if let Some(parent) = parent {
            own.insert(ChildOf(parent.parent()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un modelo con aspecto de preparado bajo `owner`: un hueso `head` y un
    /// sombrero, una cara y un poncho, y después se le pone su rig.
    fn dress(app: &mut App, owner: Entity) -> (Entity, Entity, Entity, Entity) {
        let world = app.world_mut();
        let root = world.spawn(ChildOf(owner)).id();
        let head = world.spawn((Name::new("head"), ChildOf(root))).id();
        let mesh = |world: &mut World, material: &str| {
            world
                .spawn((
                    Mesh3d::default(),
                    GltfMaterialName(material.to_string()),
                    ChildOf(root),
                ))
                .id()
        };
        let hat = mesh(world, "hat_felt");
        let face = mesh(world, "old_caucasian_male_detailed");
        let poncho = mesh(world, "poncho");
        world.entity_mut(root).insert(Rig::new(root));
        app.update();
        (head, hat, face, poncho)
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_observer(mark_viewer_head);
        app
    }

    #[test]
    fn only_the_viewers_head_and_hat_leave_the_world_layer() {
        let mut app = app();
        // El jugador, llevado por un caballo: el modelo está unos niveles más
        // abajo.
        let player = app.world_mut().spawn(ViewerBody).id();
        let horse = app.world_mut().spawn(ChildOf(player)).id();
        let (head, hat, face, poncho) = dress(&mut app, horse);
        let world = app.world();
        assert!(world.get::<ViewerHead>(head).is_some());
        assert_eq!(
            world.get::<RenderLayers>(hat),
            Some(&RenderLayers::layer(OWN_HEAD_LAYER))
        );
        for kept in [face, poncho] {
            assert_eq!(
                world.get::<RenderLayers>(kept),
                Some(&RenderLayers::layer(OWN_BODY_LAYER)),
                "the body's layer"
            );
        }
        // El modelo de otro conserva todo en la capa del mundo.
        let stranger = app.world_mut().spawn_empty().id();
        let (head, hat, _, _) = dress(&mut app, stranger);
        assert!(app.world().get::<ViewerHead>(head).is_none());
        assert_eq!(app.world().get::<RenderLayers>(hat), None);
    }

    #[test]
    fn the_layers_hide_the_head_from_first_person_only() {
        let head = RenderLayers::layer(OWN_HEAD_LAYER);
        let body = RenderLayers::layer(OWN_BODY_LAYER);
        assert!(world_layers().intersects(&head) && world_layers().intersects(&body));
        assert!(!first_person_layers().intersects(&head));
        assert!(
            first_person_layers().intersects(&body),
            "own hands from the eyes"
        );
        assert!(!close_layers().intersects(&head) && !close_layers().intersects(&body));
        for layers in [world_layers(), first_person_layers(), close_layers()] {
            assert!(layers.intersects(&RenderLayers::default()), "the world");
        }
        for piece in [
            "hat_felt",
            "hat_band",
            "Eye_brownlight",
            "eyebrow001",
            "short04",
        ] {
            assert!(is_head_piece(piece), "{piece}");
        }
        for kept in [
            "old_caucasian_male_detailed",
            "poncho",
            "neckerchief",
            "shirt",
        ] {
            assert!(!is_head_piece(kept), "{kept}");
        }
    }

    #[test]
    fn a_model_swapped_in_takes_the_head_mark_from_the_old_one() {
        let mut app = app();
        let player = app.world_mut().spawn(ViewerBody).id();
        let (old, ..) = dress(&mut app, player);
        // El modelo nuevo se prepara mientras el viejo todavía está.
        let horse = app.world_mut().spawn(ChildOf(player)).id();
        let (new, ..) = dress(&mut app, horse);
        let world = app.world();
        assert!(
            world.get::<ViewerHead>(new).is_some(),
            "the new model's head"
        );
        assert!(world.get::<ViewerHead>(old).is_none(), "one head per body");
        // El modelo viejo preparado de nuevo la recupera: el último preparado.
        let old_model = world.get::<ChildOf>(old).unwrap().parent();
        let rig = world.get::<Rig>(old_model).unwrap().clone();
        app.world_mut().entity_mut(old_model).insert(rig);
        app.update();
        let world = app.world();
        assert!(world.get::<ViewerHead>(old).is_some());
        assert!(world.get::<ViewerHead>(new).is_none());
        // La cabeza de otro cuerpo no se toca.
        let other = app.world_mut().spawn(ViewerBody).id();
        let (theirs, ..) = dress(&mut app, other);
        let world = app.world();
        assert!(world.get::<ViewerHead>(theirs).is_some());
        assert!(world.get::<ViewerHead>(old).is_some());
    }

    /// Los nombres de material de un modelo de personaje incluido, a partir de
    /// su glTF.
    fn material_names(gltf: &str) -> Vec<String> {
        let start = gltf.find("\"materials\"").expect("a materials list");
        let open = start + gltf[start..].find('[').unwrap();
        let mut depth = 0;
        let mut end = open;
        for (k, c) in gltf[open..].char_indices() {
            match c {
                '[' | '{' => depth += 1,
                ']' | '}' => depth -= 1,
                _ => {}
            }
            if depth == 0 {
                end = open + k;
                break;
            }
        }
        gltf[open..end]
            .split("\"name\"")
            .skip(1)
            .map(|rest| {
                let rest = &rest[rest.find('"').unwrap() + 1..];
                rest[..rest.find('"').unwrap()].to_string()
            })
            .collect()
    }

    #[test]
    fn the_skin_is_the_one_makehuman_skin_material_of_each_shipped_model() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/characters");
        let mut models = 0;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let file = entry.unwrap().path().join("model.gltf");
            let Ok(gltf) = std::fs::read_to_string(&file) else {
                continue;
            };
            let names = material_names(&gltf);
            assert!(names.len() > 5, "{}: {names:?}", file.display());
            let skins: Vec<&String> = names.iter().filter(|n| is_skin(n)).collect();
            assert_eq!(skins.len(), 1, "{}: {names:?}", file.display());
            models += 1;
        }
        assert!(models >= 9, "{models} models");
        for skin in [
            "old_caucasian_male_detailed",
            "middleage_cauasian_male",
            "young_caucasian_female",
            "middleage_caucasian_female",
        ] {
            assert!(is_skin(skin), "{skin}");
        }
        // Solo el nombre exacto: una prenda nombrada según un sexo o la piel no
        // es la piel.
        for kept in [
            "male_shirt",
            "female_shawl",
            "skin",
            "skin_overlay",
            "old_caucasian_male_detailed_hat",
            "old_caucasian_male_rough",
            "caucasian_male",
            "townsfolk_peon_moustache",
            "shirt",
        ] {
            assert!(!is_skin(kept), "{kept}");
        }
    }

    // --- con el protagonista, el caballo y los clips incluidos ---------------

    use crate::character::horse::tests::named;
    use crate::character::rider::tests::seated_every;
    use crate::character::{CharacterVisual, Coat, FigureKind};

    /// Todo lo que está bajo `under`, incluido él mismo.
    fn under(world: &World, under: Entity) -> Vec<Entity> {
        let mut all = Vec::new();
        let mut stack = vec![under];
        while let Some(e) = stack.pop() {
            all.push(e);
            stack.extend(world.get::<Children>(e).into_iter().flatten().copied());
        }
        all
    }

    /// La proporción en la cabeza de cada una de las articulaciones de `skin`,
    /// calculada de nuevo.
    fn shares(world: &World, model: Entity, skin: &SkinnedMesh) -> Vec<f32> {
        let head = under(world, named(world, model, "head"));
        let neck = named(world, model, "neck_01");
        skin.joints
            .iter()
            .map(|j| {
                if head.contains(j) {
                    1.0
                } else if *j == neck {
                    NECK_SHARE
                } else {
                    0.0
                }
            })
            .collect()
    }

    /// Vuelve a ponerle el rig al modelo del jinete, como cuando se prepara su
    /// escena; devuelve el modelo.
    fn dress_again(app: &mut App, me: Entity) -> Entity {
        let world = app.world_mut();
        let carrier = world.get::<Children>(me).unwrap()[0];
        let model = named(world, carrier, "Rider model");
        let rig = world.get::<Rig>(model).unwrap().clone();
        world.entity_mut(model).insert(rig);
        app.update();
        model
    }

    /// Una mesh de piel de un modelo: su entidad y sus capas, sus triángulos,
    /// cuántos de ellos son de dominio de la cabeza y su vertex usado más alto,
    /// en reposo.
    #[derive(Debug, Clone)]
    struct Skin {
        entity: Entity,
        layers: RenderLayers,
        triangles: usize,
        heads: usize,
        top: f32,
    }

    /// Las meshes de piel del modelo (el cuerpo y su cabeza recortada).
    fn skins(app: &App, model: Entity) -> Vec<Skin> {
        let world = app.world();
        let meshes = world.resource::<Assets<Mesh>>();
        under(world, model)
            .into_iter()
            .filter_map(|entity| {
                let material = world.get::<GltfMaterialName>(entity)?;
                let skin = world.get::<SkinnedMesh>(entity)?;
                if !is_skin(&material.0) {
                    return None;
                }
                let mesh = meshes.get(&world.get::<Mesh3d>(entity)?.0)?;
                let shares = shares(world, model, skin);
                let all = triangles(mesh);
                let heads = all
                    .iter()
                    .filter(|t| head_dominant(mesh, **t, &shares))
                    .count();
                let Some(VertexAttributeValues::Float32x3(at)) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                else {
                    return None;
                };
                let top = all
                    .iter()
                    .flatten()
                    .map(|&i| at[i as usize][1])
                    .fold(f32::MIN, f32::max);
                Some(Skin {
                    entity,
                    layers: world
                        .get::<RenderLayers>(entity)
                        .cloned()
                        .unwrap_or_default(),
                    triangles: all.len(),
                    heads,
                    top,
                })
            })
            .collect()
    }

    /// El punto más alto de la mesh del modelo con `material`, en reposo.
    fn top_of(app: &App, model: Entity, material: &str) -> f32 {
        let world = app.world();
        let meshes = world.resource::<Assets<Mesh>>();
        let entity = under(world, model)
            .into_iter()
            .find(|e| {
                world
                    .get::<GltfMaterialName>(*e)
                    .is_some_and(|m| m.0 == material)
            })
            .unwrap();
        let mesh = meshes.get(&world.get::<Mesh3d>(entity).unwrap().0).unwrap();
        let Some(VertexAttributeValues::Float32x3(at)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("{material} has no positions")
        };
        at.iter().map(|p| p[1]).fold(f32::MIN, f32::max)
    }

    #[test]
    fn the_players_head_is_cut_from_his_skin_onto_the_hidden_layer() {
        let rider = CharacterVisual::Mounted {
            rider: FigureKind::Protagonist,
            coat: Coat::Bayo,
        };
        let (mut app, me) = seated_every(rider, 12.2, 20);
        app.world_mut().entity_mut(me).insert(ViewerBody);
        let model = dress_again(&mut app, me);
        let cut = skins(&app, model);
        let first = first_person_layers();
        let (seen, hidden): (Vec<&Skin>, Vec<&Skin>) =
            cut.iter().partition(|s| s.layers.intersects(&first));
        assert_eq!((seen.len(), hidden.len()), (1, 1), "{cut:?}");
        let (body, head) = (seen[0], hidden[0]);
        assert_eq!(
            body.heads, 0,
            "no head-dominant triangle seen from the eyes"
        );
        assert!(body.triangles > 1000, "the rest of the body: {body:?}");
        assert_eq!(
            head.layers,
            RenderLayers::layer(OWN_HEAD_LAYER),
            "hidden only"
        );
        assert_eq!(head.triangles, head.heads, "only head-dominant triangles");
        assert!(head.triangles > 500, "the face and the skull: {head:?}");
        assert!(app.world().get::<OwnHead>(head.entity).is_some());
        // El cuello que queda en el cuerpo termina dentro del cuello de la
        // camisa y del pañuelo.
        let collar = top_of(&app, model, "shirt").max(top_of(&app, model, "neckerchief"));
        assert!(
            body.top <= collar + 0.005,
            "neck to {:.3} m, collar to {collar:.3} m",
            body.top
        );
        // Preparado de nuevo: se corta una vez, no dos.
        let model = dress_again(&mut app, me);
        let again = skins(&app, model);
        assert_eq!(again.len(), 2, "{again:?}");
        let total: usize = again.iter().map(|s| s.triangles).sum();
        assert_eq!(total, body.triangles + head.triangles);
    }
}
