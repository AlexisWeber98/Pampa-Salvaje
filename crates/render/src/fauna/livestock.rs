//! Vacunos y caballos de corral: los modelos low-poly animados de Quaternius
//! (CC0, de poly.pizza; con crédito en
//! `assets/models/fauna/quaternius/CREDITS.md` y en el `CREDITS.md` de la
//! raíz), recortados a siete clips por `tools/glbtrim.py`.
//!
//! Cada [`Species`] se carga una vez al iniciar ([`LivestockModels`]): su
//! escena glTF, sus clips y un [`AnimationGraph`] compartido. Una vaca, un toro
//! o un caballo [`Animal`] recibe un hijo [`LivestockModel`] que crea la escena
//! compartida una vez que terminó de cargar. El hijo lleva la escala de la
//! especie (la exportación está a unas 3 unidades por metro) y gira el modelo
//! (que mira hacia +Z) hacia el rumbo del animal (+X es su frente). Cuando la
//! escena está lista:
//!
//! - sus meshes reciben el rango compartido de los accesorios de los
//!   asentamientos (sin un `VisibilityRange` nuevo);
//! - sus materiales se cambian por copias teñidas con el pelaje del animal,
//!   creadas una vez por pelaje y material y compartidas por todos los animales
//!   que lo llevan ([`paint`]); las manchas de las vacas overas y los pelajes
//!   de los caballos son colores por vertex en una copia de la mesh del cuerpo
//!   por pelaje, compartida de la misma forma;
//! - su reproductor de animación recibe el grafo de la especie.
//!
//! En cada frame [`clip_for`] elige el clip a partir del estado de la fauna
//! ([`AnimalPose`]), los clips de caminata se ajustan a la velocidad sobre el
//! suelo ([`clip_speed`]) y cada animal empieza sus clips en su propio desfase
//! con semilla. Un reproductor más lejos que [`FREEZE_FAR`] de la cámara, o
//! fuera de la vista, pierde su grafo: sus huesos conservan su última pose y
//! los sistemas de animación lo saltean. Los vacunos nunca se echan: la
//! simulación no tiene estado de echado (toda pose echada que se probó se veía
//! poco natural en este rig).

use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;
use std::time::Duration;

use bevy::animation::{graph::AnimationNodeIndex, transition::AnimationTransitions};
use bevy::gltf::{GltfAssetLabel, GltfMaterialName};
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot, WorldInstanceReady};
use pampa_core::{CowCoat, Heading};

use super::{Animal, AnimalPose, AnimalSeed, Gait};

use crate::AssetRoot;
use crate::gltf::{self, ModelState};

/// Dónde viven los modelos de animales, bajo el [`AssetRoot`].
pub const LIVESTOCK_DIR: &str = "models/fauna/quaternius";

/// Más allá de esta distancia de la cámara la animación de un animal se congela
/// en su pose actual, en metros; vuelve a reproducirse una vez que regresa
/// dentro de [`RESUME_NEAR`].
pub const FREEZE_FAR: f32 = 120.0;
/// Dentro de esta distancia un animal congelado vuelve a reproducirse, en
/// metros: un poco más cerca que [`FREEZE_FAR`], para que uno en el borde no
/// parpadee.
pub const RESUME_NEAR: f32 = 110.0;

/// Por encima de esta velocidad sobre el suelo un animal que camina galopa, en
/// m/s.
pub const GALLOP_SPEED: f32 = 2.5;

/// Fundido entre dos clips.
const BLEND: Duration = Duration::from_millis(300);

/// La velocidad sobre el suelo que muestra el clip `Walk` del caballo a tamaño
/// real, en m/s: el deslizamiento medio hacia atrás de cada vaso mientras está
/// a menos de 1.5 cm de su punto más bajo, a lo largo de todo el ciclo
/// (1.17 s). El clip es irregular: los vasos traseros se deslizan hacia atrás a
/// unos 1.4 m/s y los delanteros a unos 0.85.
pub const HORSE_WALK_SPEED: f32 = 1.25;
/// Lo mismo para su clip `Gallop` (0.58 s, unos 2.3 m por zancada): los vasos
/// traseros a unos 4.4 m/s y los delanteros a entre 2.6 y 4.1.
pub const HORSE_GALLOP_SPEED: f32 = 3.94;

/// Los animales que se dibujan a partir de un modelo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Species {
    Cow,
    Bull,
    Horse,
}

impl Species {
    pub const ALL: [Self; 3] = [Self::Cow, Self::Bull, Self::Horse];

    /// La especie con la que se dibuja `animal`, si es ganado.
    pub fn of(animal: Animal) -> Option<Self> {
        match animal {
            Animal::Cow(_) => Some(Self::Cow),
            Animal::Bull(_) => Some(Self::Bull),
            Animal::Horse(_) => Some(Self::Horse),
            _ => None,
        }
    }

    /// El archivo del modelo, bajo el [`AssetRoot`].
    pub fn path(self) -> String {
        let name = match self {
            Self::Cow => "cow",
            Self::Bull => "bull",
            Self::Horse => "horse",
        };
        format!("{LIVESTOCK_DIR}/{name}.glb")
    }

    /// Altura a la cruz en las unidades propias del modelo: la parte superior
    /// del lomo por encima de las patas delanteras, medida sobre las meshes
    /// exportadas.
    pub const fn model_withers(self) -> f32 {
        match self {
            Self::Cow | Self::Bull => 4.4,
            Self::Horse => 3.6,
        }
    }

    /// Altura a la cruz en el juego, en metros: como los vacunos y los caballos
    /// criollos procedurales a los que reemplazan (una vaca 1.4 m, un caballo
    /// 1.5 m), con el toro un poco más grande.
    pub const fn withers(self) -> f32 {
        match self {
            Self::Cow => 1.4,
            Self::Bull => 1.55,
            Self::Horse => 1.5,
        }
    }

    /// Metros por unidad del modelo.
    pub fn scale(self) -> f32 {
        self.withers() / self.model_withers()
    }

    /// La velocidad sobre el suelo que muestra su clip `Walk`, en m/s.
    pub fn walk_speed(self) -> f32 {
        match self {
            Self::Cow | Self::Bull => 1.0,
            Self::Horse => HORSE_WALK_SPEED,
        }
    }

    /// La velocidad sobre el suelo que muestra su clip `Gallop`, en m/s.
    pub fn gallop_speed(self) -> f32 {
        match self {
            Self::Cow | Self::Bull => 6.0,
            Self::Horse => HORSE_GALLOP_SPEED,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// Los clips que se conservan en cada modelo, en el orden en que los escribió
/// `tools/glbtrim.py` (sus índices de animación glTF).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Clip {
    Idle,
    Idle2,
    IdleHeadlow,
    Eating,
    Walk,
    Gallop,
    /// El salto en el aire (patas recogidas, cuerpo cabeceando): solo lo
    /// reproduce un caballo montado en vuelo, así que todos los demás animales
    /// lo mantienen con peso cero.
    GallopJump,
}

/// Cuántos clips contiene cada modelo (la longitud de [`Clip::ALL`]).
pub const CLIP_COUNT: usize = 7;

impl Clip {
    pub const ALL: [Self; CLIP_COUNT] = [
        Self::Idle,
        Self::Idle2,
        Self::IdleHeadlow,
        Self::Eating,
        Self::Walk,
        Self::Gallop,
        Self::GallopJump,
    ];

    /// El nombre del clip en el archivo glTF.
    pub fn name(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Idle2 => "Idle_2",
            Self::IdleHeadlow => "Idle_Headlow",
            Self::Eating => "Eating",
            Self::Walk => "Walk",
            Self::Gallop => "Gallop",
            Self::GallopJump => "Gallop_Jump",
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

/// Una mezcla con semilla de `seed` y `salt`, para una variedad determinista.
pub(crate) fn mix(seed: u32, salt: u32) -> u32 {
    let mut h = seed.wrapping_mul(0x9e37_79b9) ^ salt.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^ (h >> 12)
}

/// Segundos que un caballo de corral mantiene una espera antes de la siguiente.
pub(crate) const HORSE_IDLE: f32 = 14.0;

/// El clip que reproduce `animal` para `pose`. Los animales parados eligen una
/// de las esperas según su `seed`; los caballos de corral, que nunca se mueven,
/// cambian de espera (o pastan) cada [`HORSE_IDLE`] segundos de `clock`.
pub fn clip_for(animal: Animal, pose: &AnimalPose, seed: u32, clock: f32) -> Clip {
    let idles = [Clip::Idle, Clip::Idle2, Clip::IdleHeadlow];
    match pose.gait {
        Gait::Graze => Clip::Eating,
        Gait::Walk if pose.speed > GALLOP_SPEED => Clip::Gallop,
        Gait::Walk => Clip::Walk,
        Gait::Run => Clip::Gallop,
        _ if matches!(animal, Animal::Horse(_)) => {
            let turn = ((clock.max(0.0) + (seed % 97) as f32) / HORSE_IDLE) as u32;
            [Clip::Idle, Clip::Idle2, Clip::IdleHeadlow, Clip::Eating][mix(seed, turn) as usize % 4]
        }
        _ => idles[mix(seed, 1) as usize % 3],
    }
}

/// La velocidad de reproducción de `clip` para un animal de `species` que se
/// mueve a `ground` m/s: la caminata y el galope ajustados a ella, el resto tal
/// como se creó.
pub fn clip_speed(clip: Clip, species: Species, ground: f32) -> f32 {
    match clip {
        Clip::Walk => (ground / species.walk_speed()).clamp(0.4, 2.0),
        Clip::Gallop => (ground / species.gallop_speed()).clamp(0.6, 1.6),
        _ => 1.0,
    }
}

/// La fracción con semilla de un clip ya reproducida cuando un animal lo
/// empieza, en `[0, 1)`: los vecinos nunca se mueven al mismo paso.
pub fn start_offset(seed: u32, clip: Clip) -> f32 {
    (mix(seed, 7 + clip.index() as u32) % 1000) as f32 / 1000.0
}

/// Si la animación de un animal está congelada: fuera de la vista, o más allá
/// de [`FREEZE_FAR`] (de vuelta dentro de [`RESUME_NEAR`] una vez que `was`
/// congelado).
pub fn frozen(was: bool, distance: f32, in_view: bool) -> bool {
    !in_view || distance > if was { RESUME_NEAR } else { FREEZE_FAR }
}

/// Desde dónde se ve el mundo: la cámara 3D activa dibujada en último lugar (el
/// orden más alto), la entidad más baja entre iguales; `None` si no hay
/// ninguna. La misma cámara sea cual sea el orden en que las liste una
/// consulta.
pub fn view_eye<'a>(
    cameras: impl IntoIterator<Item = (Entity, &'a GlobalTransform, &'a Camera)>,
) -> Option<Vec3> {
    cameras
        .into_iter()
        .filter(|(_, _, camera)| camera.is_active)
        .max_by_key(|(entity, _, camera)| (camera.order, std::cmp::Reverse(*entity)))
        .map(|(_, at, _)| at.translation())
}

/// Si un modelo está a la vista: alguna de sus meshes (`probes`) lo está, según
/// `visible`; una pieza pequeña descartada por culling (un sombrero, un botón)
/// no oculta el resto. Si no hay ninguna mesh a la que preguntar, lo está.
pub fn any_in_view(probes: &[Entity], visible: impl Fn(Entity) -> bool) -> bool {
    probes.is_empty() || probes.iter().any(|&p| visible(p))
}

/// El transform del modelo bajo su animal: girado de mirar hacia +Z al
/// `heading` (+X es el frente del cuerpo) y escalado a tamaño real.
pub fn model_transform(species: Species, heading: Heading) -> Transform {
    Transform::from_rotation(Quat::from_rotation_y(heading.radians() + FRAC_PI_2))
        .with_scale(Vec3::splat(species.scale()))
}

/// Un vertex de la mesh exportada (Z hacia arriba, alrededor de 1/100 de una
/// unidad del modelo) en el marco del cuerpo del animal parado, en metros: +X
/// su frente, +Y hacia arriba, +Z su lado derecho.
pub fn body_point(species: Species, raw: Vec3) -> Vec3 {
    // El nodo de la mesh gira -90° alrededor de X y escala por 100: de Z hacia
    // arriba a Y hacia arriba.
    let scene = Vec3::new(raw.x, raw.z, -raw.y) * 100.0;
    // Después el modelo gira para mirar hacia +X.
    Vec3::new(scene.z, scene.y, -scene.x) * species.scale()
}

/// Cómo pinta un pelaje uno de los materiales del modelo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Paint {
    /// Su propio color, sin cambios.
    Keep,
    /// Un color liso.
    Flat(Color),
    /// Blanco, con el color en los vertices de la copia de la mesh del cuerpo
    /// para este pelaje ([`coat_color_at`]).
    Vertex,
    /// No se dibuja (los cuernos del Angus mocho).
    Hidden,
}

/// La pintura que el pelaje de `animal` le da al material llamado `material`.
///
/// Vacunos: `Main` es el cuero y `Main_Light` la cara y la parte inferior de
/// las patas. Un Hereford es colorado con la cara y las medias blancas; un
/// Angus, todo negro y mocho; un overo, blanco con manchas negras, con la cara
/// y las patas negras. Caballos: `Main` es el cuerpo (colores por vertex, de
/// los pelajes criollos, [`Coat`](crate::character::Coat)), `Hair` la crin y la
/// cola, y `Main_Dark` y `Main_Light` partes de la cabeza.
pub fn paint(animal: Animal, material: &str) -> Paint {
    let rgb = |c: [u8; 3]| Paint::Flat(Color::srgb_u8(c[0], c[1], c[2]));
    match animal {
        Animal::Cow(coat) | Animal::Bull(coat) => match (coat, material) {
            (CowCoat::Hereford, "Main") => rgb([118, 44, 22]),
            (CowCoat::Hereford, "Main_Light") => rgb([226, 220, 206]),
            (CowCoat::Hereford, "Muzzle") => rgb([200, 150, 132]),
            (CowCoat::Angus, "Main" | "Main_Light") => rgb([22, 20, 19]),
            (CowCoat::Angus, "Muzzle") => rgb([34, 30, 28]),
            (CowCoat::Angus, "Horns") => Paint::Hidden,
            (CowCoat::Overo, "Main") => Paint::Vertex,
            (CowCoat::Overo, "Main_Light") => rgb([24, 22, 20]),
            (CowCoat::Overo, "Muzzle") => rgb([44, 38, 34]),
            _ => Paint::Keep,
        },
        Animal::Horse(coat) => match material {
            "Main" => Paint::Vertex,
            "Hair" => rgb(coat.mane()),
            "Main_Dark" => rgb(coat.points()),
            "Main_Light" => rgb(coat.head()),
            "Hooves" => rgb(coat.hoof()),
            _ => Paint::Keep,
        },
        _ => Paint::Keep,
    }
}

/// Si el punto `p` del cuerpo de un overo (en metros, marco del cuerpo) cae en
/// una mancha negra: unas pocas manchas grandes a partir de ondas cruzadas. Una
/// aproximación del patrón real, lo bastante barata para hornearla una vez por
/// vertex.
pub fn overo_black(p: Vec3) -> bool {
    let wave = (p.x * 3.1 + 0.7).sin() * (p.y * 2.3 + p.z * 1.9 + 1.1).sin()
        + 0.6 * (p.x * 5.3 - p.z * 3.7 + 2.0).sin() * (p.y * 4.1 + 0.3).cos();
    wave > 0.3
}

/// El color por vertex del cuerpo de `animal` en `p` (en metros, marco del
/// cuerpo), mirando hacia `normal`, como RGBA lineal. Solo para los materiales
/// [`Paint::Vertex`].
pub fn coat_color_at(animal: Animal, p: Vec3, normal: Vec3) -> [f32; 4] {
    let srgb = match animal {
        Animal::Horse(coat) => coat.color_at(p, normal),
        _ if overo_black(p) => [26, 24, 22],
        _ => [230, 226, 216],
    };
    Color::srgb_u8(srgb[0], srgb[1], srgb[2])
        .to_linear()
        .to_f32_array()
}

/// Una especie cargada: su escena, sus clips y su grafo compartido.
#[derive(Debug, Clone)]
pub struct SpeciesModel {
    /// El archivo completo, retenido para que se cargue una sola vez: si solo
    /// se retienen partes etiquetadas de él, una parte pedida más tarde vuelve
    /// a cargar el archivo, y la escena creada a partir de la primera carga se
    /// crea otra vez.
    pub file: Handle<bevy::gltf::Gltf>,
    pub scene: Handle<WorldAsset>,
    pub clips: Vec<Handle<AnimationClip>>,
    pub graph: Handle<AnimationGraph>,
    /// El nodo del grafo de cada [`Clip`], en el orden de [`Clip::ALL`].
    pub nodes: Vec<AnimationNodeIndex>,
}

/// Los modelos de ganado encontrados al iniciar, por [`Species`] (`None` cuando
/// falta el archivo: esa especie no se dibuja).
#[derive(Resource, Debug, Clone, Default)]
pub struct LivestockModels(pub [Option<SpeciesModel>; 3]);

impl LivestockModels {
    pub fn get(&self, species: Species) -> Option<&SpeciesModel> {
        self.0[species.index()].as_ref()
    }
}

/// Las copias por pelaje de los materiales y las meshes del cuerpo de los
/// modelos, creadas una vez y compartidas por todos los animales de ese pelaje.
#[derive(Resource, Debug, Default)]
pub struct CoatCache {
    materials: HashMap<(AssetId<StandardMaterial>, Animal), Handle<StandardMaterial>>,
    meshes: HashMap<(AssetId<Mesh>, Animal), Handle<Mesh>>,
}

impl CoatCache {
    /// Las copias por pelaje creadas hasta ahora: materiales, y después meshes.
    pub fn len(&self) -> (usize, usize) {
        (self.materials.len(), self.meshes.len())
    }
}

/// La raíz de un modelo de ganado, hija de su animal.
#[derive(Component, Debug, Clone, Copy)]
pub struct LivestockModel {
    pub species: Species,
}

/// Un modelo que todavía carga: su escena se crea cuando esté listo.
#[derive(Component, Debug, Clone)]
pub struct LivestockPending(pub Handle<WorldAsset>);

/// El estado de animación de un modelo creado.
#[derive(Component, Debug, Clone)]
pub struct LivestockRig {
    /// La entidad con su `AnimationPlayer`.
    pub player: Entity,
    /// Sus meshes dibujadas: está a la vista mientras alguna lo esté.
    pub probes: Vec<Entity>,
    pub clip: Option<Clip>,
    pub frozen: bool,
}

/// Empieza a cargar los tres modelos y construye el grafo de cada uno. Sin un
/// servidor de assets, una raíz o el plugin de animación (tests, una app vacía)
/// no carga ninguno.
pub(super) fn load_livestock(
    mut commands: Commands,
    server: Option<Res<AssetServer>>,
    root: Option<Res<AssetRoot>>,
    graphs: Option<ResMut<Assets<AnimationGraph>>>,
) {
    commands.init_resource::<CoatCache>();
    let (Some(server), Some(root), Some(mut graphs)) = (server, root, graphs) else {
        commands.insert_resource(LivestockModels::default());
        return;
    };
    let mut models = LivestockModels::default();
    for species in Species::ALL {
        let path = species.path();
        let Some(scene) = gltf::load_scene(&server, Some(&root), &path) else {
            continue;
        };
        let file = server.load(path.clone());
        let clips: Vec<Handle<AnimationClip>> = Clip::ALL
            .iter()
            .map(|c| server.load(GltfAssetLabel::Animation(c.index()).from_asset(path.clone())))
            .collect();
        let (graph, nodes) = AnimationGraph::from_clips(clips.iter().cloned());
        models.0[species.index()] = Some(SpeciesModel {
            file,
            scene,
            clips,
            graph: graphs.add(graph),
            nodes,
        });
    }
    commands.insert_resource(models);
}

/// Cuelga un modelo bajo cada vaca, toro y caballo de corral nuevo.
pub(super) fn attach_livestock(
    mut commands: Commands,
    models: Option<Res<LivestockModels>>,
    added: Query<(Entity, &Animal, &AnimalPose), Added<Animal>>,
) {
    for (entity, &animal, pose) in &added {
        let Some(species) = Species::of(animal) else {
            continue;
        };
        let mut model = commands.spawn((
            Name::new("Livestock model"),
            LivestockModel { species },
            model_transform(species, pose.heading),
            Visibility::default(),
            ChildOf(entity),
        ));
        if let Some(m) = models.as_ref().and_then(|m| m.get(species)) {
            model.insert(LivestockPending(m.scene.clone()));
        }
    }
}

/// Crea la escena de cada modelo en espera cuando está listo; descarta uno que
/// falló.
pub(super) fn swap_in_livestock(
    mut commands: Commands,
    server: Option<Res<AssetServer>>,
    pending: Query<(Entity, &LivestockPending)>,
) {
    let Some(server) = server else { return };
    for (entity, wait) in &pending {
        match gltf::model_state(&server, &wait.0) {
            ModelState::Ready => {
                commands
                    .entity(entity)
                    .remove::<LivestockPending>()
                    .try_insert(WorldAssetRoot(wait.0.clone()));
            }
            ModelState::Failed => {
                commands.entity(entity).remove::<LivestockPending>();
            }
            ModelState::Loading => {}
        }
    }
}

/// Con qué tiñe [`dress_livestock`].
type Stores<'w> = (
    ResMut<'w, CoatCache>,
    ResMut<'w, Assets<StandardMaterial>>,
    ResMut<'w, Assets<Mesh>>,
);

/// Prepara un modelo en el momento en que se crea su escena: el rango
/// compartido en sus meshes, su pelaje y el grafo de la especie en su
/// reproductor.
#[allow(clippy::too_many_arguments)]
pub(super) fn dress_livestock(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    (mut cache, mut materials, mut meshes): Stores,
    models: Option<Res<LivestockModels>>,
    roots: Query<(&ChildOf, &LivestockModel)>,
    animals: Query<&Animal>,
    children: Query<&Children>,
    parts: Query<ModelPart>,
) {
    let root = ready.entity;
    let Ok((parent, model)) = roots.get(root) else {
        return;
    };
    let (Ok(&animal), Some(species)) = (
        animals.get(parent.parent()),
        models.as_ref().and_then(|m| m.get(model.species)),
    ) else {
        return;
    };
    let (player, probes) = dress_meshes(
        &mut commands,
        (&mut cache, &mut materials, &mut meshes),
        root,
        (animal, model.species),
        &children,
        &parts,
    );
    if let Some(player) = player {
        commands.entity(player).try_insert((
            AnimationGraphHandle(species.graph.clone()),
            AnimationTransitions::new(),
        ));
        commands.entity(root).try_insert(LivestockRig {
            player,
            probes,
            clip: None,
            frozen: false,
        });
    }
}

/// Lo que [`dress_meshes`] lee de los nodos de un modelo: su mesh, su material
/// y el nombre del material, y si es el reproductor de animación.
pub(crate) type ModelPart = (
    Option<&'static Mesh3d>,
    Option<&'static MeshMaterial3d<StandardMaterial>>,
    Option<&'static GltfMaterialName>,
    Has<AnimationPlayer>,
);

/// Prepara las meshes del modelo creado bajo `root` como `animal` (de
/// `species`): el rango compartido de los accesorios, los materiales teñidos
/// del pelaje y el cuerpo con colores por vertex (copias creadas una vez por
/// pelaje, en `cache`), y las partes ocultas ocultas. Devuelve la entidad con
/// su reproductor de animación y las meshes dibujadas (sus sondas de
/// visibilidad). Los modelos de los caballos ensillados se preparan igual (ver
/// [`horse`](crate::character::horse)).
pub(crate) fn dress_meshes(
    commands: &mut Commands,
    (cache, materials, meshes): (
        &mut CoatCache,
        &mut Assets<StandardMaterial>,
        &mut Assets<Mesh>,
    ),
    root: Entity,
    (animal, species): (Animal, Species),
    children: &Query<&Children>,
    parts: &Query<ModelPart>,
) -> (Option<Entity>, Vec<Entity>) {
    let range = crate::settlement::ranges()[1].clone();
    let mut player = None;
    let mut probes = Vec::new();
    for entity in children.iter_descendants(root) {
        let Ok((mesh, material, name, has_player)) = parts.get(entity) else {
            continue;
        };
        if has_player {
            player = Some(entity);
        }
        let Some(mesh) = mesh else { continue };
        let mut part = commands.entity(entity);
        part.try_insert(range.clone());
        let paint = name.map_or(Paint::Keep, |n| paint(animal, &n.0));
        if paint == Paint::Hidden {
            part.try_insert(Visibility::Hidden);
            continue;
        }
        probes.push(entity);
        if let Some(material) = material {
            let tinted = cache
                .materials
                .entry((material.0.id(), animal))
                .or_insert_with(|| {
                    let mut m = materials.get(&material.0).cloned().unwrap_or_default();
                    match paint {
                        Paint::Flat(color) => m.base_color = color,
                        Paint::Vertex => m.base_color = Color::WHITE,
                        Paint::Keep | Paint::Hidden => {}
                    }
                    // El brillo metálico de la exportación parece plástico.
                    m.metallic = 0.0;
                    m.perceptual_roughness = 0.9;
                    materials.add(m)
                })
                .clone();
            part.try_insert(MeshMaterial3d(tinted));
        }
        if paint == Paint::Vertex {
            let key = (mesh.0.id(), animal);
            let coloured = match cache.meshes.get(&key) {
                Some(h) => Some(h.clone()),
                None => meshes
                    .get(&mesh.0)
                    .map(|m| coat_mesh(m, animal, species))
                    .map(|m| {
                        let h = meshes.add(m);
                        cache.meshes.insert(key, h.clone());
                        h
                    }),
            };
            if let Some(coloured) = coloured {
                part.try_insert(Mesh3d(coloured));
            }
        }
    }
    (player, probes)
}

/// `mesh` con cada vertex coloreado para el pelaje de `animal`.
fn coat_mesh(mesh: &Mesh, animal: Animal, species: Species) -> Mesh {
    let mut out = mesh.clone();
    let positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(VertexAttributeValues::Float32x3(p)) => p.clone(),
        _ => Vec::new(),
    };
    let normals = match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
        Some(VertexAttributeValues::Float32x3(n)) => n.clone(),
        _ => vec![[0.0, 0.0, 1.0]; positions.len()],
    };
    let colors: Vec<[f32; 4]> = positions
        .iter()
        .zip(normals.iter().chain(std::iter::repeat(&[0.0, 0.0, 1.0])))
        .map(|(p, n)| {
            let at = body_point(species, Vec3::from(*p));
            let normal = body_point(species, Vec3::from(*n)).normalize_or_zero();
            coat_color_at(animal, at, normal)
        })
        .collect();
    out.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    out
}

/// Un modelo tal como lo ve [`drive_livestock`].
type ModelRig = (
    &'static ChildOf,
    &'static LivestockModel,
    &'static mut Transform,
    &'static GlobalTransform,
    Option<&'static mut LivestockRig>,
);

/// Gira cada modelo y reproduce su clip: elegido a partir del estado de la
/// fauna, ajustado a su velocidad, empezado en su desfase con semilla, y
/// congelado lejos o fuera de la vista.
#[allow(clippy::too_many_arguments)]
pub(super) fn drive_livestock(
    mut commands: Commands,
    time: Res<Time>,
    models: Option<Res<LivestockModels>>,
    clips: Option<Res<Assets<AnimationClip>>>,
    cameras: Query<(Entity, &GlobalTransform, &Camera), With<Camera3d>>,
    animals: Query<(&Animal, &AnimalPose, Option<&AnimalSeed>)>,
    mut rigs: Query<ModelRig>,
    mut players: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
    seen: Query<&ViewVisibility>,
) {
    let clock = time.elapsed_secs();
    let eye = view_eye(cameras);
    for (child_of, model, mut transform, at, rig) in &mut rigs {
        let Ok((&animal, pose, seed)) = animals.get(child_of.parent()) else {
            continue;
        };
        // Solo cuando se movió: un animal quieto deja intacto su subárbol.
        transform.set_if_neq(model_transform(model.species, pose.heading));
        let (Some(mut rig), Some(species)) =
            (rig, models.as_ref().and_then(|m| m.get(model.species)))
        else {
            continue;
        };
        let distance = eye.map_or(0.0, |e| e.distance(at.translation()));
        // La visibilidad solo se conoce con una cámara dibujando.
        let in_view =
            eye.is_none() || any_in_view(&rig.probes, |p| seen.get(p).is_ok_and(|v| v.get()));
        let freeze = frozen(rig.frozen, distance, in_view);
        if freeze != rig.frozen {
            rig.frozen = freeze;
            let mut player = commands.entity(rig.player);
            if freeze {
                player.try_remove::<AnimationGraphHandle>();
            } else {
                player.try_insert(AnimationGraphHandle(species.graph.clone()));
            }
        }
        if rig.frozen {
            continue;
        }
        let Ok((mut player, mut transitions)) = players.get_mut(rig.player) else {
            continue;
        };
        let seed = seed.map_or(child_of.parent().index_u32(), |s| s.0);
        let clip = clip_for(animal, pose, seed, clock);
        let node = species.nodes[clip.index()];
        if rig.clip != Some(clip) {
            let blend = if rig.clip.is_some() {
                BLEND
            } else {
                Duration::ZERO
            };
            let duration = clips
                .as_ref()
                .and_then(|c| c.get(&species.clips[clip.index()]))
                .map_or(0.0, AnimationClip::duration);
            transitions
                .play(&mut player, node, blend)
                .repeat()
                .seek_to(start_offset(seed, clip) * duration);
            rig.clip = Some(clip);
        }
        if let Some(active) = player.animation_mut(node) {
            active.set_speed(clip_speed(clip, model.species, pose.speed));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::Coat;
    use crate::gltf::tests::{root, settle, unfinished_app};
    use bevy::camera::visibility::VisibilityRange;

    fn standing(gait: Gait, speed: f32) -> AnimalPose {
        AnimalPose {
            gait,
            speed,
            ..default()
        }
    }

    #[test]
    fn fauna_states_map_to_clips() {
        let cow = Animal::Cow(CowCoat::Hereford);
        let clip = |animal, gait, speed| clip_for(animal, &standing(gait, speed), 3, 0.0);
        assert_eq!(clip(cow, Gait::Graze, 0.0), Clip::Eating);
        assert_eq!(clip(cow, Gait::Walk, 0.5), Clip::Walk);
        assert_eq!(
            clip(cow, Gait::Walk, 1.3),
            Clip::Walk,
            "walking off a rider"
        );
        assert_eq!(clip(cow, Gait::Walk, 4.0), Clip::Gallop);
        assert_eq!(clip(cow, Gait::Run, 4.0), Clip::Gallop);
        // Parado: una de las esperas, la misma para el mismo animal, y no la
        // misma para toda la tropa.
        let idles: std::collections::HashSet<Clip> = (0..40)
            .map(|seed| {
                let c = clip_for(cow, &standing(Gait::Stand, 0.0), seed, 0.0);
                assert_eq!(c, clip_for(cow, &standing(Gait::Stand, 0.0), seed, 99.0));
                c
            })
            .collect();
        assert_eq!(
            idles,
            [Clip::Idle, Clip::Idle2, Clip::IdleHeadlow].into(),
            "{idles:?}"
        );
        // Un caballo de corral cambia lo que hace cada tanto, de forma
        // determinista.
        let horse = Animal::Horse(Coat::Bayo);
        let seen: std::collections::HashSet<Clip> = (0..40)
            .map(|k| {
                clip_for(
                    horse,
                    &standing(Gait::Stand, 0.0),
                    11,
                    k as f32 * HORSE_IDLE,
                )
            })
            .collect();
        assert!(seen.len() >= 3 && seen.contains(&Clip::Eating), "{seen:?}");
        assert_eq!(
            clip_for(horse, &standing(Gait::Stand, 0.0), 11, 30.0),
            clip_for(horse, &standing(Gait::Stand, 0.0), 11, 30.0)
        );
    }

    #[test]
    fn walking_clips_are_paced_to_the_ground_speed_and_seeded_apart() {
        let slow = clip_speed(Clip::Walk, Species::Cow, 0.5);
        let shy = clip_speed(Clip::Walk, Species::Cow, 1.3);
        assert!(slow < 1.0 && shy > 1.0 && shy > slow, "{slow} {shy}");
        assert_eq!(clip_speed(Clip::Walk, Species::Cow, 0.0), 0.4, "clamped");
        assert_eq!(clip_speed(Clip::Eating, Species::Cow, 0.0), 1.0);
        let offsets: Vec<f32> = (0..20).map(|s| start_offset(s, Clip::Eating)).collect();
        assert!(offsets.iter().all(|o| (0.0..1.0).contains(o)));
        let distinct: std::collections::HashSet<u32> =
            offsets.iter().map(|o| (o * 1000.0) as u32).collect();
        assert!(distinct.len() >= 15, "{offsets:?}");
    }

    #[test]
    fn the_view_is_the_active_camera_drawn_last_whatever_the_query_order() {
        let mut world = World::new();
        let [a, b, c] = [(); 3].map(|_| world.spawn_empty().id());
        let at = |x: f32| GlobalTransform::from_translation(Vec3::X * x);
        let (near, far, off) = (at(1.0), at(500.0), at(9.0));
        let main = Camera {
            order: 1,
            ..default()
        };
        let other = Camera::default();
        let inactive = Camera {
            order: 5,
            is_active: false,
            ..default()
        };
        // Gana el orden activo más alto, listado primero o último.
        let cams = [(a, &far, &other), (b, &near, &main), (c, &off, &inactive)];
        assert_eq!(view_eye(cams), Some(Vec3::X));
        let mut reversed = cams;
        reversed.reverse();
        assert_eq!(view_eye(reversed), Some(Vec3::X));
        // Órdenes iguales: la entidad más baja, en cualquiera de los dos
        // listados.
        let ties = [(b, &far, &other), (a, &near, &other)];
        let lowest = if a < b { Vec3::X } else { Vec3::X * 500.0 };
        assert_eq!(view_eye(ties), Some(lowest));
        let mut reversed = ties;
        reversed.reverse();
        assert_eq!(view_eye(reversed), Some(lowest));
        assert_eq!(view_eye([(c, &off, &inactive)]), None);
    }

    #[test]
    fn a_model_is_in_view_while_any_of_its_meshes_is() {
        let mut world = World::new();
        let [hat, body] = [(); 2].map(|_| world.spawn_empty().id());
        // El sombrero descartado por culling, el cuerpo dibujado: a la vista.
        assert!(any_in_view(&[hat, body], |e| e == body));
        assert!(!any_in_view(&[hat, body], |_| false));
        assert!(any_in_view(&[], |_| false), "nothing to ask");
    }

    #[test]
    fn animation_freezes_far_off_or_out_of_view() {
        assert!(!frozen(false, 50.0, true));
        assert!(frozen(false, 50.0, false), "culled");
        assert!(!frozen(false, FREEZE_FAR - 1.0, true));
        assert!(frozen(false, FREEZE_FAR + 1.0, true));
        // Entre los dos sigue haciendo lo que estaba haciendo.
        let edge = (FREEZE_FAR + RESUME_NEAR) / 2.0;
        assert!(frozen(true, edge, true));
        assert!(!frozen(false, edge, true));
        assert!(!frozen(true, RESUME_NEAR - 1.0, true));
        // Cada animal congelado se sigue dibujando: el rango de los accesorios
        // llega más lejos.
        let props = crate::settlement::ranges()[1].clone();
        assert!(props.end_margin.start > FREEZE_FAR);
    }

    #[test]
    fn models_are_scaled_to_life_and_turned_to_the_heading() {
        let heading = Heading::from_radians(0.7);
        for species in Species::ALL {
            let t = model_transform(species, heading);
            // El frente del modelo (+Z) apunta a lo largo del rumbo del cuerpo.
            let front = (t.rotation * Vec3::Z).normalize();
            let along = Quat::from_rotation_y(heading.radians()) * Vec3::X;
            assert!(front.distance(along) < 1e-5, "{species:?}: {front}");
            let withers = t.scale.y * species.model_withers();
            assert!((withers - species.withers()).abs() < 1e-4);
        }
        assert!(Species::Bull.withers() > Species::Cow.withers());
        assert!((1.3..1.5).contains(&Species::Cow.withers()));
        assert!((1.4..1.6).contains(&Species::Horse.withers()));
        // Un vertex crudo hacia arriba en la Z de la mesh está arriba en el
        // cuerpo; uno hacia abajo en su -Y (el lado de la cabeza) está en el
        // frente del cuerpo.
        let up = body_point(Species::Cow, Vec3::new(0.0, 0.0, 0.01));
        let head = body_point(Species::Cow, Vec3::new(0.0, -0.01, 0.0));
        assert!(up.y > 0.0 && up.x.abs() < 1e-6 && head.x > 0.0 && head.y.abs() < 1e-6);
    }

    #[test]
    fn coats_tint_per_breed_and_overos_are_patched() {
        let tint = |animal, material| paint(animal, material);
        let red = tint(Animal::Cow(CowCoat::Hereford), "Main");
        let Paint::Flat(red) = red else {
            panic!("{red:?}")
        };
        let red = red.to_srgba();
        assert!(
            red.red > red.green * 1.8 && red.red > red.blue * 2.0,
            "{red:?}"
        );
        let Paint::Flat(face) = tint(Animal::Cow(CowCoat::Hereford), "Main_Light") else {
            panic!()
        };
        assert!(face.to_srgba().green > 0.8, "a white face");
        for material in ["Main", "Main_Light"] {
            let Paint::Flat(black) = tint(Animal::Cow(CowCoat::Angus), material) else {
                panic!()
            };
            assert!(black.to_srgba().red < 0.1, "black Angus");
        }
        assert_eq!(tint(Animal::Cow(CowCoat::Angus), "Horns"), Paint::Hidden);
        assert_eq!(tint(Animal::Cow(CowCoat::Overo), "Main"), Paint::Vertex);
        assert_eq!(
            tint(Animal::Cow(CowCoat::Hereford), "Eye_Black"),
            Paint::Keep
        );
        // El toro lleva el pelaje de su raza como las vacas.
        for material in ["Main", "Main_Light", "Muzzle", "Horns"] {
            for coat in CowCoat::ALL {
                assert_eq!(
                    tint(Animal::Bull(coat), material),
                    tint(Animal::Cow(coat), material)
                );
            }
        }
        // Entre un tercio y la mitad del costillar de un overo es negro.
        let mut black = 0;
        let mut all = 0;
        for i in 0..30 {
            for j in 0..10 {
                for k in 0..8 {
                    let p = Vec3::new(
                        -1.0 + i as f32 * 0.07,
                        0.7 + j as f32 * 0.07,
                        -0.4 + k as f32 * 0.1,
                    );
                    all += 1;
                    black += usize::from(overo_black(p));
                }
            }
        }
        let share = black as f32 / all as f32;
        assert!((0.2..0.55).contains(&share), "{share}");
        // Los caballos toman los pelajes criollos: el cuerpo en colores por
        // vertex, la crin en su color.
        for coat in Coat::ALL {
            assert_eq!(tint(Animal::Horse(coat), "Main"), Paint::Vertex);
            let m = coat.mane();
            assert_eq!(
                tint(Animal::Horse(coat), "Hair"),
                Paint::Flat(Color::srgb_u8(m[0], m[1], m[2]))
            );
            let p = Vec3::new(0.0, 1.1, 0.4);
            let c = coat.color_at(p, Vec3::Z);
            let want = Color::srgb_u8(c[0], c[1], c[2]).to_linear().to_f32_array();
            assert_eq!(coat_color_at(Animal::Horse(coat), p, Vec3::Z), want);
        }
    }

    /// Los tres modelos incluidos, cargados en modo headless.
    fn loaded() -> (App, Vec<Handle<bevy::gltf::Gltf>>) {
        let mut app = unfinished_app(&root());
        app.init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>()
            .add_plugins(bevy::animation::AnimationPlugin);
        app.finish();
        app.cleanup();
        let server = app.world().resource::<AssetServer>().clone();
        let mut handles = Vec::new();
        for species in Species::ALL {
            let path = species.path();
            let scene = gltf::load_scene(&server, Some(&root()), &path).expect(&path);
            assert_eq!(settle(&mut app, &scene), ModelState::Ready, "{path}");
            let handle: Handle<bevy::gltf::Gltf> = server.load(path.clone());
            while !app
                .world()
                .resource::<Assets<bevy::gltf::Gltf>>()
                .contains(&handle)
            {
                app.update();
            }
            handles.push(handle);
        }
        (app, handles)
    }

    #[test]
    fn the_three_models_load_skinned_with_their_clips_at_life_size() {
        let (app, handles) = loaded();
        let world = app.world();
        for (species, handle) in Species::ALL.into_iter().zip(handles) {
            let gltf = world
                .resource::<Assets<bevy::gltf::Gltf>>()
                .get(&handle)
                .unwrap();
            // Los clips que reproduce el juego, en el orden en que los
            // reproduce.
            assert_eq!(gltf.animations.len(), Clip::ALL.len(), "{species:?}");
            for clip in Clip::ALL {
                let named = &gltf.named_animations[clip.name()];
                assert_eq!(
                    *named,
                    gltf.animations[clip.index()],
                    "{species:?} {clip:?}"
                );
                let clip = world
                    .resource::<Assets<AnimationClip>>()
                    .get(named)
                    .unwrap();
                assert!(clip.duration() > 0.3, "{species:?}");
            }
            assert_eq!(gltf.skins.len(), 1);
            let skin = world
                .resource::<Assets<bevy::gltf::GltfSkin>>()
                .get(&gltf.skins[0])
                .unwrap();
            let joints = match species {
                Species::Horse => 50,
                _ => 42,
            };
            assert_eq!(skin.joints.len(), joints, "{species:?}");
            // Su cuerpo está a tamaño real en el marco del cuerpo.
            let mut top = 0.0f32;
            let mut triangles = 0;
            let mut names: Vec<&str> = gltf.named_materials.keys().map(|k| &**k).collect();
            names.sort();
            let mesh = world
                .resource::<Assets<bevy::gltf::GltfMesh>>()
                .get(&gltf.meshes[0])
                .unwrap();
            for primitive in &mesh.primitives {
                let m = world
                    .resource::<Assets<Mesh>>()
                    .get(&primitive.mesh)
                    .unwrap();
                triangles += m.indices().map_or(0, |i| i.len() / 3);
                let Some(VertexAttributeValues::Float32x3(p)) =
                    m.attribute(Mesh::ATTRIBUTE_POSITION)
                else {
                    panic!("positions")
                };
                for p in p {
                    let b = body_point(species, Vec3::from(*p));
                    // El lomo, detrás de la cabeza y el cuello.
                    if b.x < 0.2 {
                        top = top.max(b.y);
                    }
                }
            }
            println!("{species:?}: {triangles} triangles, back {top:.2} m, materials {names:?}");
            assert!(triangles <= 2_500, "{species:?}: {triangles}");
            for material in ["Main", "Main_Light", "Muzzle", "Hooves"] {
                assert!(names.contains(&material), "{species:?}: {names:?}");
            }
            let want = species.withers();
            assert!(
                (want * 0.9..want * 1.1).contains(&top),
                "{species:?}: back at {top}"
            );
        }
    }

    /// El plugin de fauna con los modelos incluidos, headless.
    fn fauna_app() -> App {
        let mut app = unfinished_app(&root());
        app.init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>()
            .add_plugins(bevy::animation::AnimationPlugin)
            .insert_resource(root())
            .add_plugins(super::super::FaunaPlugin);
        app.finish();
        app.cleanup();
        app
    }

    /// Actualiza `app` hasta que cada modelo tenga su rig; hace panic después
    /// de 10 s.
    fn until_rigged(app: &mut App, count: usize) {
        for _ in 0..2000 {
            app.update();
            let mut q = app.world_mut().query::<&LivestockRig>();
            if q.iter(app.world()).count() == count {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("the models never spawned");
    }

    #[test]
    fn animals_spawn_tinted_and_animated_and_freeze_far_off() {
        let mut app = fauna_app();
        let grazing = AnimalPose {
            gait: Gait::Graze,
            ..default()
        };
        let herd = [
            Animal::Cow(CowCoat::Angus),
            Animal::Cow(CowCoat::Angus),
            Animal::Cow(CowCoat::Overo),
            Animal::Bull(CowCoat::Overo),
            Animal::Horse(Coat::Tordillo),
        ];
        for (k, animal) in herd.into_iter().enumerate() {
            app.world_mut().spawn((
                animal,
                grazing,
                AnimalSeed(k as u32),
                Transform::from_xyz(k as f32 * 4.0, 0.0, 0.0),
            ));
        }
        until_rigged(&mut app, herd.len());
        app.update();
        let world = app.world_mut();
        // Cada reproductor tiene el grafo de su especie y reproduce su clip.
        let mut rigs = world.query::<(&LivestockRig, &LivestockModel)>();
        let rigs: Vec<(LivestockRig, Species)> = rigs
            .iter(world)
            .map(|(r, m)| (r.clone(), m.species))
            .collect();
        for (rig, _) in &rigs {
            assert!(!rig.frozen);
            assert!(rig.clip.is_some());
            let player = world.get::<AnimationPlayer>(rig.player).unwrap();
            assert_eq!(player.playing_animations().count(), 1);
            assert!(world.get::<AnimationGraphHandle>(rig.player).is_some());
        }
        let cows = rigs.iter().filter(|r| r.1 == Species::Cow).count();
        assert_eq!(cows, 3);
        // Las meshes comparten el rango de los accesorios, y las copias por
        // pelaje se crean una vez por pelaje: los dos Angus comparten las
        // suyas.
        let props = crate::settlement::ranges()[1].clone();
        let mut parts = world.query::<(
            &Mesh3d,
            &MeshMaterial3d<StandardMaterial>,
            &GltfMaterialName,
            &VisibilityRange,
            Option<&Visibility>,
        )>();
        let mut angus_main = std::collections::HashSet::new();
        let mut horns_hidden = 0;
        for (mesh, material, name, range, visibility) in parts.iter(world) {
            assert!(*range == props, "a new range");
            let m = world
                .resource::<Assets<StandardMaterial>>()
                .get(&material.0)
                .unwrap();
            if name.0 == "Horns" && visibility == Some(&Visibility::Hidden) {
                horns_hidden += 1;
                continue;
            }
            assert_eq!(m.metallic, 0.0, "{}", name.0);
            if name.0 == "Main" && m.base_color.to_srgba().red < 0.1 {
                angus_main.insert(material.0.id());
            }
            let coloured = world
                .resource::<Assets<Mesh>>()
                .get(&mesh.0)
                .unwrap()
                .attribute(Mesh::ATTRIBUTE_COLOR)
                .is_some();
            assert_eq!(coloured, m.base_color == Color::WHITE && name.0 == "Main");
        }
        assert_eq!(angus_main.len(), 1, "one shared Angus hide");
        assert_eq!(horns_hidden, 2, "the Angus are polled");
        let (materials, meshes) = world.resource::<CoatCache>().len();
        // Dos pelajes de vaca, un toro overo y un caballo, unos pocos
        // materiales cada uno; las meshes del cuerpo de la vaca overa, del toro
        // overo y del caballo.
        assert!(materials <= 4 * 8, "{materials}");
        assert_eq!(meshes, 3);

        // Una cámara lejos: cada animal se congela y se le quita su grafo.
        world.spawn((Camera3d::default(), Transform::from_xyz(0.0, 2.0, 500.0)));
        app.update();
        app.update();
        let world = app.world_mut();
        let mut rigs = world.query::<&LivestockRig>();
        let rigs: Vec<LivestockRig> = rigs.iter(world).cloned().collect();
        for rig in rigs {
            assert!(rig.frozen);
            assert!(world.get::<AnimationGraphHandle>(rig.player).is_none());
        }
    }
}
