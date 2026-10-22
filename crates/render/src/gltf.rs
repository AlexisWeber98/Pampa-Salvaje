//! glTF models: third-party scenes that keep their PBR materials.
//!
//! Unlike the OBJ vegetation, which is parsed and baked by hand at startup,
//! a glTF file goes through Bevy's `AssetServer`: it loads in the background
//! and its first scene is spawned under an entity with [`scene_at`]. The
//! asset server must read from the game's [`AssetRoot`]; build its plugin
//! with [`asset_plugin`]. A missing file logs a warning and loads nothing,
//! so the caller can draw a procedural stand-in instead.

use bevy::asset::RecursiveDependencyLoadState;
use bevy::gltf::GltfAssetLabel;
use bevy::prelude::*;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot};

use crate::AssetRoot;

/// Where the glTF models live, under the [`AssetRoot`].
pub const MODEL_DIR: &str = "models";

/// The asset plugin reading from `root`, so glTF paths are relative to the
/// same `assets/` folder the hand-loaded models and textures use.
pub fn asset_plugin(root: &AssetRoot) -> AssetPlugin {
    AssetPlugin {
        file_path: root.0.to_string_lossy().into_owned(),
        ..default()
    }
}

/// Starts loading the first scene of the glTF (or GLB) file at `path`,
/// relative to the [`AssetRoot`] (for example `models/test/triangle.gltf`).
///
/// Without a root, or when the file is missing, it logs a warning and
/// returns `None`, so the caller draws its stand-in.
pub fn load_scene(
    server: &AssetServer,
    root: Option<&AssetRoot>,
    path: &str,
) -> Option<Handle<WorldAsset>> {
    let Some(root) = root else {
        warn!("glTF model {path} not loaded (no asset root); drawing its stand-in");
        return None;
    };
    let file = root.0.join(path);
    if !file.is_file() {
        warn!(
            "glTF model {} not loaded (file not found); drawing its stand-in",
            file.display()
        );
        return None;
    }
    Some(server.load(GltfAssetLabel::Scene(0).from_asset(path.to_owned())))
}

/// How far a model's scene, with its meshes, materials and textures, has
/// loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelState {
    Loading,
    Ready,
    Failed,
}

/// The load state of `scene` and everything it depends on. A failed model
/// logs its error once from the asset server; callers keep its stand-in.
pub fn model_state(server: &AssetServer, scene: &Handle<WorldAsset>) -> ModelState {
    match server.recursive_dependency_load_state(scene) {
        RecursiveDependencyLoadState::Loaded => ModelState::Ready,
        RecursiveDependencyLoadState::Failed(_) => ModelState::Failed,
        RecursiveDependencyLoadState::NotLoaded | RecursiveDependencyLoadState::Loading => {
            ModelState::Loading
        }
    }
}

/// The components that spawn `scene` at `transform`. The scene's entities
/// appear as children once it has loaded.
pub fn scene_at(scene: Handle<WorldAsset>, transform: Transform) -> impl Bundle {
    (WorldAssetRoot(scene), transform)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::gltf::{Gltf, GltfMesh, GltfPlugin};
    use bevy::world_serialization::WorldSerializationPlugin;

    const TRIANGLE: &str = "models/test/triangle.gltf";

    fn root() -> AssetRoot {
        AssetRoot(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"))
    }

    /// A headless app with just enough to load glTF files and spawn scenes.
    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            asset_plugin(&root()),
            TransformPlugin,
            WorldSerializationPlugin,
        ))
        .init_asset::<Mesh>()
        .init_asset::<Image>()
        .add_plugins(GltfPlugin::default());
        app.finish();
        app.cleanup();
        app
    }

    /// Updates `app` until `scene` stops loading, or gives up after a while.
    fn settle(app: &mut App, scene: &Handle<WorldAsset>) -> ModelState {
        for _ in 0..500 {
            app.update();
            let state = model_state(app.world().resource::<AssetServer>(), scene);
            if state != ModelState::Loading {
                return state;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        ModelState::Loading
    }

    #[test]
    fn the_shipped_triangle_loads_and_spawns() {
        let mut app = app();
        let server = app.world().resource::<AssetServer>().clone();
        let scene = load_scene(&server, Some(&root()), TRIANGLE).expect("triangle file exists");
        assert_eq!(settle(&mut app, &scene), ModelState::Ready);

        let gltf: Handle<Gltf> = server.load(TRIANGLE);
        settle_gltf(&mut app, &gltf);
        let world = app.world();
        let gltf = world.resource::<Assets<Gltf>>().get(&gltf).unwrap();
        assert_eq!(gltf.scenes.len(), 1);
        let mesh = world
            .resource::<Assets<GltfMesh>>()
            .get(&gltf.meshes[0])
            .unwrap();
        let mesh = world
            .resource::<Assets<Mesh>>()
            .get(&mesh.primitives[0].mesh)
            .unwrap();
        assert_eq!(mesh.count_vertices(), 3);

        // Spawned, the scene hangs its triangle under the placed entity.
        let placed = app
            .world_mut()
            .spawn(scene_at(scene, Transform::from_xyz(1.0, 2.0, 3.0)))
            .id();
        app.update();
        let world = app.world_mut();
        let mut meshes = world.query::<(&Mesh3d, &ChildOf)>();
        assert_eq!(meshes.iter(world).count(), 1);
        assert!(world.entity(placed).get::<Children>().is_some());
    }

    fn settle_gltf(app: &mut App, gltf: &Handle<Gltf>) {
        for _ in 0..500 {
            app.update();
            if app.world().resource::<Assets<Gltf>>().contains(gltf) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    #[test]
    fn a_missing_model_loads_nothing() {
        let app = app();
        let server = app.world().resource::<AssetServer>();
        assert!(load_scene(server, Some(&root()), "models/test/missing.glb").is_none());
        assert!(load_scene(server, None, TRIANGLE).is_none());
    }
}
