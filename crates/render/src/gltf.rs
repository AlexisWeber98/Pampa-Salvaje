//! Modelos glTF: escenas de terceros que conservan sus materiales PBR.
//!
//! A diferencia de la vegetación OBJ, que se interpreta y se hornea a mano al
//! inicio, un archivo glTF pasa por el `AssetServer` de Bevy: se carga en
//! segundo plano y su primera escena se crea bajo una entidad con [`scene_at`].
//! El servidor de assets debe leer desde el [`AssetRoot`] del juego; su plugin
//! se construye con [`asset_plugin`]. Un archivo faltante registra una
//! advertencia y no carga nada, para que quien llama pueda dibujar en su lugar
//! un sustituto procedural.

use bevy::asset::RecursiveDependencyLoadState;
use bevy::gltf::GltfAssetLabel;
use bevy::prelude::*;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot};

use crate::AssetRoot;

/// Dónde están los modelos glTF, bajo el [`AssetRoot`].
pub const MODEL_DIR: &str = "models";

/// El plugin de assets que lee desde `root`, para que las rutas glTF sean
/// relativas a la misma carpeta `assets/` que usan los modelos y texturas
/// cargados a mano.
pub fn asset_plugin(root: &AssetRoot) -> AssetPlugin {
    AssetPlugin {
        file_path: root.0.to_string_lossy().into_owned(),
        ..default()
    }
}

/// Empieza a cargar la primera escena del archivo glTF (o GLB) en `path`,
/// relativa al [`AssetRoot`] (por ejemplo `models/test/triangle.gltf`).
///
/// Sin raíz, cuando falta el archivo, o cuando `path` no es una ruta relativa
/// simple dentro de la raíz (absoluta, o con `..`), registra una advertencia y
/// devuelve `None`, para que quien llama dibuje su sustituto.
pub fn load_scene(
    server: &AssetServer,
    root: Option<&AssetRoot>,
    path: &str,
) -> Option<Handle<WorldAsset>> {
    if !is_inside(path) {
        warn!("glTF model {path:?} not loaded (not a plain path under the assets folder)");
        return None;
    }
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

/// Si `path` es relativa y se mantiene dentro de la carpeta a la que se une:
/// solo nombres simples, sin raíz, prefijo, `.` ni `..`.
pub fn is_inside(path: &str) -> bool {
    use std::path::Component;
    let path = std::path::Path::new(path);
    !path.as_os_str().is_empty()
        && !path.to_string_lossy().contains('\\')
        && path.components().all(|c| matches!(c, Component::Normal(_)))
}

/// Cuánto se ha cargado de la escena de un modelo, con sus mallas, materiales y
/// texturas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelState {
    Loading,
    Ready,
    Failed,
}

/// El estado de carga de `scene` y de todo aquello de lo que depende. Un modelo
/// que falla registra su error una vez desde el servidor de assets; quienes
/// llaman conservan su sustituto.
pub fn model_state(server: &AssetServer, scene: &Handle<WorldAsset>) -> ModelState {
    match server.recursive_dependency_load_state(scene) {
        RecursiveDependencyLoadState::Loaded => ModelState::Ready,
        RecursiveDependencyLoadState::Failed(_) => ModelState::Failed,
        RecursiveDependencyLoadState::NotLoaded | RecursiveDependencyLoadState::Loading => {
            ModelState::Loading
        }
    }
}

/// Los componentes que crean `scene` en `transform`. Las entidades de la escena
/// aparecen como hijas una vez que se ha cargado.
pub fn scene_at(scene: Handle<WorldAsset>, transform: Transform) -> impl Bundle {
    (WorldAssetRoot(scene), transform)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use bevy::gltf::{Gltf, GltfMesh, GltfPlugin};
    use bevy::world_serialization::WorldSerializationPlugin;

    const TRIANGLE: &str = "models/test/triangle.gltf";

    pub(crate) fn root() -> AssetRoot {
        AssetRoot(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"))
    }

    /// Una app headless con lo justo para cargar archivos glTF y crear escenas.
    fn app() -> App {
        app_at(&root())
    }

    /// Una app glTF headless que lee desde `root`, ya finalizada.
    pub(crate) fn app_at(root: &AssetRoot) -> App {
        let mut app = unfinished_app(root);
        app.finish();
        app.cleanup();
        app
    }

    /// Una app glTF headless que lee desde `root`: carga modelos con sus
    /// materiales y texturas, y crea sus escenas. Se agregan plugins y luego se
    /// finaliza.
    pub(crate) fn unfinished_app(root: &AssetRoot) -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            asset_plugin(root),
            TransformPlugin,
            WorldSerializationPlugin,
        ))
        .init_asset::<Mesh>()
        .init_asset::<Image>()
        .init_asset::<StandardMaterial>()
        // Lo que registra el plugin de imágenes una vez que la GPU dice qué
        // formatos comprimidos acepta: aquí no hace falta ninguno.
        .register_asset_loader(bevy::image::ImageLoader::new(
            bevy::image::CompressedImageFormats::NONE,
        ))
        .register_type::<MeshMaterial3d<StandardMaterial>>()
        .add_plugins(GltfPlugin::default());
        app.world_mut()
            .resource_mut::<bevy::gltf::extensions::GltfExtensionHandlers>()
            .0
            .write_blocking()
            .push(Box::new(PbrMaterials));
        app
    }

    /// Lo que hace el `PbrPlugin` (que no está en estas apps headless) para
    /// glTF: un `StandardMaterial` por cada material glTF, en cada primitiva
    /// creada.
    #[derive(Default, Clone)]
    struct PbrMaterials;

    impl bevy::gltf::extensions::GltfExtensionHandler for PbrMaterials {
        fn dyn_clone(&self) -> Box<dyn bevy::gltf::extensions::ErasedGltfExtensionHandler> {
            Box::new(self.clone())
        }

        fn on_material(
            &mut self,
            load_context: &mut bevy::asset::LoadContext<'_>,
            _gltf_material: &bevy::gltf::gltf::Material,
            _material: Handle<bevy::gltf::GltfMaterial>,
            material_asset: &bevy::gltf::GltfMaterial,
            material_label: &str,
        ) {
            load_context.add_labeled_asset(
                format!("{material_label}/std"),
                bevy::pbr::gltf::standard_material_from_gltf_material(material_asset),
            );
        }

        fn on_spawn_mesh_and_material(
            &mut self,
            load_context: &mut bevy::asset::LoadContext<'_>,
            _primitive: &bevy::gltf::gltf::Primitive,
            _mesh: &bevy::gltf::gltf::Mesh,
            _material: &bevy::gltf::gltf::Material,
            entity: &mut EntityWorldMut,
            material_label: &str,
        ) {
            let material =
                load_context.get_label_handle::<StandardMaterial>(format!("{material_label}/std"));
            entity.insert(MeshMaterial3d(material));
        }
    }

    /// Actualiza `app` hasta que `scene` deja de cargar; entra en pánico a los
    /// 10 s.
    pub(crate) fn settle(app: &mut App, scene: &Handle<WorldAsset>) -> ModelState {
        for _ in 0..2000 {
            app.update();
            let state = model_state(app.world().resource::<AssetServer>(), scene);
            if state != ModelState::Loading {
                return state;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!(
            "timed out after 10 s waiting for {:?} to load",
            scene.path()
        );
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

        // Ya creada, la escena cuelga su triángulo bajo la entidad colocada.
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

    /// Actualiza `app` hasta que `gltf` está en sus assets; entra en pánico a
    /// los 10 s.
    fn settle_gltf(app: &mut App, gltf: &Handle<Gltf>) {
        for _ in 0..2000 {
            app.update();
            if app.world().resource::<Assets<Gltf>>().contains(gltf) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("timed out after 10 s waiting for {:?} to load", gltf.path());
    }

    #[test]
    fn a_missing_model_or_a_path_out_of_the_root_loads_nothing() {
        let app = app();
        let server = app.world().resource::<AssetServer>();
        assert!(load_scene(server, Some(&root()), "models/test/missing.glb").is_none());
        assert!(load_scene(server, None, TRIANGLE).is_none());
        // El triángulo existe, pero solo se carga una ruta simple dentro de la
        // raíz.
        for path in [
            "../assets/models/test/triangle.gltf",
            "models/../models/test/triangle.gltf",
            "./models/test/triangle.gltf",
            "/models/test/triangle.gltf",
            "models\\test\\triangle.gltf",
            "",
        ] {
            assert!(!is_inside(path), "{path:?}");
            assert!(
                load_scene(server, Some(&root()), path).is_none(),
                "{path:?}"
            );
        }
        assert!(is_inside(TRIANGLE));
    }

    /// Una carpeta de assets provisoria para un test, que se borra al soltarla.
    pub(crate) struct Scratch(pub std::path::PathBuf);

    impl Scratch {
        pub(crate) fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("pampa-salvaje-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        /// Escribe `text` en `path` dentro de la carpeta.
        pub(crate) fn write(&self, path: &str, text: &str) {
            let file = self.0.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_malformed_model_fails_and_its_caller_keeps_the_stand_in() {
        let scratch = Scratch::new("malformed-gltf");
        scratch.write("models/broken/model.gltf", "{ \"asset\": { \"version\": ");
        let root = AssetRoot(scratch.0.clone());
        let mut app = app_at(&root);
        let server = app.world().resource::<AssetServer>().clone();
        let scene = load_scene(&server, Some(&root), "models/broken/model.gltf")
            .expect("the file exists, so loading starts");
        assert_eq!(settle(&mut app, &scene), ModelState::Failed);
    }

    /// Todos los glTF que trae el juego: los objetos de Poly Haven y las
    /// ranuras de modelo ya ocupadas.
    fn shipped_models() -> Vec<String> {
        let mut found = Vec::new();
        for dir in ["models/props/polyhaven", "models/third-party"] {
            let Ok(entries) = std::fs::read_dir(root().0.join(dir)) else {
                continue;
            };
            for entry in entries.flatten() {
                let Ok(files) = std::fs::read_dir(entry.path()) else {
                    continue;
                };
                for file in files.flatten() {
                    let name = file.file_name().to_string_lossy().into_owned();
                    if name.ends_with(".gltf") || name.ends_with(".glb") {
                        let folder = entry.file_name().to_string_lossy().into_owned();
                        found.push(format!("{dir}/{folder}/{name}"));
                    }
                }
            }
        }
        found.sort();
        found
    }

    #[test]
    fn every_shipped_model_loads_with_its_textures() {
        let models = shipped_models();
        // 5 objetos de Poly Haven, el molino de viento y la carreta destrozada.
        assert!(models.len() >= 7, "{models:?}");
        let mut app = app();
        let server = app.world().resource::<AssetServer>().clone();
        for path in &models {
            let scene = load_scene(&server, Some(&root()), path).expect(path);
            assert_eq!(settle(&mut app, &scene), ModelState::Ready, "{path}");
        }
    }
}
