//! The player's own body, seen from inside it by a first-person camera.
//!
//! The game marks the player's entity [`ViewerBody`]. When a character
//! model under it is dressed (its [`Rig`] goes in, on foot or in the
//! saddle), [`mark_viewer_head`] does three things to it:
//!
//! - it marks the `head` bone [`ViewerHead`]: the camera sits at
//!   [`EYE_IN_HEAD`] in its frame;
//! - it moves the meshes round the eyes (the hat and its band, the hair,
//!   the eyes and eyebrows; see [`is_head_piece`]) to the render layer
//!   [`OWN_HEAD_LAYER`] alone;
//! - it cuts the head out of the skin ([`is_skin`]) by its skin weights
//!   ([`split_head`]): the triangles mostly weighted to the `head` bone
//!   and the top of the neck ([`head_share`]) go to a mesh of their own
//!   ([`OwnHead`]), skinned by the same joints, on [`OWN_HEAD_LAYER`] too.
//!   The cut runs round the neck inside the collar and the neckerchief.
//!   The two halves are made once per skin asset and shared by every later
//!   dressing; nothing is done per frame.
//!
//! Every other mesh of the model goes on [`OWN_BODY_LAYER`].
//!
//! A camera that sees [`world_layers`] draws all of it; a first-person
//! camera drops [`OWN_HEAD_LAYER`] ([`first_person_layers`]), so no piece
//! of the face or skull can pass in front of it, however the head moves.
//! An orbit camera pulled in right against the player drops both
//! ([`close_layers`]), so his clothes never fill the screen. Every light
//! (the sun and the street lamps) is on both layers too, so the whole body
//! stays in its shadow maps and casts the sun's shadow. The arms, the hands,
//! the collar, the reins and the horse's neck and ears stay in view.

use std::collections::HashMap;

use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::{RenderLayers, VisibilityRange};
use bevy::gltf::GltfMaterialName;
use bevy::mesh::skinning::SkinnedMesh;
use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::prelude::*;

use super::rig::Rig;

/// The render layer of the player's own head (the face and skull, the hat,
/// hair and eyes): hidden from a first-person camera, drawn by every other
/// camera and lit by the sun.
pub const OWN_HEAD_LAYER: usize = 1;

/// The render layer of the rest of the player's own model (the body, the
/// clothes, the hands): drawn from the eyes, hidden from an orbit camera
/// pulled in too close, lit by the sun.
pub const OWN_BODY_LAYER: usize = 2;

/// The eyes in the `head` bone's frame, meters: between them and just in
/// front, measured from the protagonist's model (its eyes' middle is
/// 0.04 m above and 0.09 m in front of the bone, the front of the eyes
/// 0.10 m). The bone's rest frame is the model's (+Z forward, +Y up).
pub const EYE_IN_HEAD: Vec3 = Vec3::new(0.0, 0.04, 0.11);

/// How much a vertex's weight on `neck_01` counts toward the head when the
/// skin is cut ([`head_share`]). With the head's weight alone the cut runs
/// under the jaw and leaves the neck standing up to 1.61 m, 8 cm over the
/// collar; at 0.75 it runs round the neck at 1.50 to 1.53 m on the
/// protagonist, inside his shirt collar (to 1.53 m) and neckerchief (to
/// 1.56 m).
pub const NECK_SHARE: f32 = 0.75;

/// A triangle whose vertices' [`head_share`] averages this or more goes
/// with the head.
pub const HEAD_DOMINANT: f32 = 0.5;

/// The character the first-person camera sits in: the player.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct ViewerBody;

/// The `head` bone of a [`ViewerBody`]'s model.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct ViewerHead;

/// The face and skull cut out of a [`ViewerBody`]'s skin ([`split_head`]).
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct OwnHead;

/// The layers a camera sees to draw everything, the player's head too.
pub fn world_layers() -> RenderLayers {
    RenderLayers::from_layers(&[0, OWN_HEAD_LAYER, OWN_BODY_LAYER])
}

/// The layers a first-person camera sees: everything but the player's own
/// head.
pub fn first_person_layers() -> RenderLayers {
    RenderLayers::from_layers(&[0, OWN_BODY_LAYER])
}

/// The layers an orbit camera pulled in against the player sees:
/// everything but the player's own model.
pub fn close_layers() -> RenderLayers {
    RenderLayers::layer(0)
}

/// Whether the model mesh of `material` sits round the eyes: a hat, its
/// band, the hair, the eyes, eyebrows and eyelashes.
pub fn is_head_piece(material: &str) -> bool {
    let lower = material.to_ascii_lowercase();
    ["hat", "eye", "hair", "short0", "kepi", "boina"]
        .iter()
        .any(|p| lower.starts_with(p))
}

/// The ages a MakeHuman skin material is named after.
const SKIN_AGES: [&str; 3] = ["young", "middleage", "old"];

/// The ethnic skins MakeHuman names its skin materials after, with the
/// `cauasian` misspelling its middle-aged skins ship with (our
/// `middleage_cauasian_male` models).
const SKIN_KINDS: [&str; 4] = ["caucasian", "cauasian", "african", "asian"];

/// Whether the model mesh of `material` is the skin: the one mesh the head
/// is cut out of. MakeHuman names it after the skin, exactly
/// `<age>_<kind>_<male|female>`, maybe with `_detailed` (like
/// `old_caucasian_male_detailed`; see [`SKIN_AGES`] and [`SKIN_KINDS`]). The
/// clothes keep their collars whole, whatever they are called.
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

/// How much of a vertex goes with the head: its weights on the head's
/// joints, and [`NECK_SHARE`] of those on the neck's, by the share of each
/// of the skin's joints (`shares`, in its joint order).
pub fn head_share(joints: [u16; 4], weights: [f32; 4], shares: &[f32]) -> f32 {
    joints
        .iter()
        .zip(weights)
        .map(|(&j, w)| shares.get(usize::from(j)).copied().unwrap_or(0.0) * w)
        .sum()
}

/// Whether the triangle `triangle` of `mesh` goes with the head: its
/// vertices' [`head_share`] by the joint `shares` averages
/// [`HEAD_DOMINANT`] or more.
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

/// The triangles of `mesh`, by vertex index.
pub fn triangles(mesh: &Mesh) -> Vec<[u32; 3]> {
    let indices: Vec<u32> = match mesh.indices() {
        Some(indices) => indices.iter().map(|i| i as u32).collect(),
        None => (0..mesh.count_vertices() as u32).collect(),
    };
    indices.as_chunks::<3>().0.to_vec()
}

/// Cuts `mesh` (a skinned skin) in two by [`head_dominant`] over its skin's
/// joint `shares`: the rest of the body and the head, each a copy of every
/// vertex with its own triangles, so both keep the same skinning. `None`
/// if nothing goes with the head.
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

/// A mesh entity of a model, as [`mark_viewer_head`] reads it.
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

/// The two halves made of each skin asset so far, by the skin's asset.
#[derive(Default)]
pub(super) struct Cuts(HashMap<AssetId<Mesh>, (Handle<Mesh>, Handle<Mesh>)>);

impl Cuts {
    /// Whether `id` is a half already cut (a scene dressed again).
    fn is_cut(&self, id: AssetId<Mesh>) -> bool {
        self.0
            .values()
            .any(|(body, head)| body.id() == id || head.id() == id)
    }
}

/// Marks the head bone, moves the head pieces (and the rest to
/// [`OWN_BODY_LAYER`]) and cuts the head out of the
/// skin of a [`ViewerBody`]'s model the moment its [`Rig`] goes in (every
/// time its scene is dressed).
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
        // One head per body: the model dressed last. Another one still
        // under it (an old model while it is swapped out) is unmarked.
        for old in &marked {
            if old != head && parents.iter_ancestors(old).any(|a| a == viewer) {
                commands.entity(old).try_remove::<ViewerHead>();
            }
        }
    }
    // Each joint's share in the head: the head and anything under it
    // wholly, the neck in part.
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

    /// A dressed-looking model under `owner`: a `head` bone and a hat, a
    /// face and a poncho, then its rig put in.
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
        // The player, carried by a horse: the model is a few levels down.
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
        // Somebody else's model keeps everything on the world layer.
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
        // The new model dressed while the old one is still there.
        let horse = app.world_mut().spawn(ChildOf(player)).id();
        let (new, ..) = dress(&mut app, horse);
        let world = app.world();
        assert!(
            world.get::<ViewerHead>(new).is_some(),
            "the new model's head"
        );
        assert!(world.get::<ViewerHead>(old).is_none(), "one head per body");
        // The old model dressed again takes it back: the last one dressed.
        let old_model = world.get::<ChildOf>(old).unwrap().parent();
        let rig = world.get::<Rig>(old_model).unwrap().clone();
        app.world_mut().entity_mut(old_model).insert(rig);
        app.update();
        let world = app.world();
        assert!(world.get::<ViewerHead>(old).is_some());
        assert!(world.get::<ViewerHead>(new).is_none());
        // Another body's head is not touched.
        let other = app.world_mut().spawn(ViewerBody).id();
        let (theirs, ..) = dress(&mut app, other);
        let world = app.world();
        assert!(world.get::<ViewerHead>(theirs).is_some());
        assert!(world.get::<ViewerHead>(old).is_some());
    }

    /// The material names of a shipped character model, from its glTF.
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
        // Only the exact name: a garment named after a sex or the skin is
        // not the skin.
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

    // --- with the shipped protagonist, horse and clips -----------------------

    use crate::character::horse::tests::named;
    use crate::character::rider::tests::seated_every;
    use crate::character::{CharacterVisual, Coat, FigureKind};

    /// Everything under `under`, itself included.
    fn under(world: &World, under: Entity) -> Vec<Entity> {
        let mut all = Vec::new();
        let mut stack = vec![under];
        while let Some(e) = stack.pop() {
            all.push(e);
            stack.extend(world.get::<Children>(e).into_iter().flatten().copied());
        }
        all
    }

    /// Each of `skin`'s joints' share in the head, worked out afresh.
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

    /// Puts the rider model's rig in again, as when its scene is dressed;
    /// returns the model.
    fn dress_again(app: &mut App, me: Entity) -> Entity {
        let world = app.world_mut();
        let carrier = world.get::<Children>(me).unwrap()[0];
        let model = named(world, carrier, "Rider model");
        let rig = world.get::<Rig>(model).unwrap().clone();
        world.entity_mut(model).insert(rig);
        app.update();
        model
    }

    /// One skin mesh of a model: its entity and layers, its triangles, how
    /// many of them are head-dominant, and its highest vertex used, at rest.
    #[derive(Debug, Clone)]
    struct Skin {
        entity: Entity,
        layers: RenderLayers,
        triangles: usize,
        heads: usize,
        top: f32,
    }

    /// The model's skin meshes (the body and its cut head).
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

    /// The highest point of the model's mesh of `material`, at rest.
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
        // The neck left on the body ends inside the collar and neckerchief.
        let collar = top_of(&app, model, "shirt").max(top_of(&app, model, "neckerchief"));
        assert!(
            body.top <= collar + 0.005,
            "neck to {:.3} m, collar to {collar:.3} m",
            body.top
        );
        // Dressed again: cut once, not twice.
        let model = dress_again(&mut app, me);
        let again = skins(&app, model);
        assert_eq!(again.len(), 2, "{again:?}");
        let total: usize = again.iter().map(|s| s.triangles).sum();
        assert_eq!(total, body.triangles + head.triangles);

        // The eyes at the gallop, in the horse's frame (it stays put): how
        // far they bob, how fast, and how far behind them an eye eased at
        // the game's riding rate (8/s, unclamped) would fall.
        let dt = 0.02;
        let mut eyes = Vec::new();
        for _ in 0..100 {
            app.update();
            let world = app.world_mut();
            let head = *world
                .query_filtered::<&GlobalTransform, With<ViewerHead>>()
                .single(world)
                .unwrap();
            eyes.push(head.transform_point(EYE_IN_HEAD));
        }
        let mut eased = eyes[0];
        let (mut lag, mut speed) = (0.0_f32, 0.0_f32);
        for pair in eyes.windows(2) {
            speed = speed.max(pair[0].distance(pair[1]) / dt);
            eased += (pair[1] - eased) * (1.0 - (-8.0_f32 * dt).exp());
            lag = lag.max(eased.distance(pair[1]));
        }
        let low = eyes.iter().copied().fold(Vec3::MAX, Vec3::min);
        let high = eyes.iter().copied().fold(Vec3::MIN, Vec3::max);
        println!(
            "gallop eyes: bob {:.3} m, fastest {speed:.2} m/s, unclamped lag up to {lag:.3} m",
            high - low
        );
        assert!(lag > 0.0 && speed < 10.0, "a seated rider's head: {speed}");
    }
}
