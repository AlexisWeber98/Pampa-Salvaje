//! Pueblo procedural low-poly dibujado a partir de un [`TownLayout`] de
//! `pampa-core`: calles de tierra (una malla de tierra con UV en espacio del
//! mundo, más las huellas de carro del patio de cargas) con veredas, de
//! ladrillo en una ciudad y alrededor de la plaza de un pueblo, y de tierra
//! apisonada que se funde con el pasto en el resto ([`TownLayout::sidewalks`],
//! [`pampa_core::town::Walk`]), la barra para atar caballos del almacén y los
//! palenques ([`crate::facade::palenque`]), una plaza arbolada (sus senderos,
//! monumento, bancos y reja de [`crate::facade::plaza`], sus paraísos, olmos,
//! eucaliptos o aromos de [`crate::tree`]), los edificios italianizantes de
//! 1919 del kit de fachadas ([`crate::facade::town_buildings`]), un ferrocarril
//! con su estación, galpones de cereal y molinos de viento sobre tanques de
//! agua redondos.
//!
//! Los árboles fuera de los anillos de la plaza ([`TownLayout::shade_trees`])
//! también son mallas compartidas del kit: los árboles de vereda de una ciudad
//! son las variantes podadas de las especies de calle en sus cazuelas
//! ([`tree_pit_shape`]), los de las isletas y los patios, las variantes
//! comunes; los árboles de frente de un pueblo están podados y se levantan
//! sobre la tierra. Los arbustos de patio y los canteros de flores
//! ([`TownLayout::yard_plants`]) son cada uno una pequeña malla compartida
//! ([`crate::tree::plants`]).
//!
//! La lógica es dueña de cada huella; este módulo solo decide cómo se ve cada
//! [`BuildingKind`].
//!
//! Los pueblos se cargan por streaming: las mallas de un pueblo se construyen
//! bajo un único [`TownRoot`] cuando el jinete se acerca a menos del margen de
//! [`TownActivation`], y se eliminan (raíz incluida) al pasar el margen más la
//! histéresis. Los carteles y el tren cuelgan sus entidades de la misma raíz.
//!
//! Cada edificio es una malla por cada [`Surface`] que lleva, compartiendo los
//! [`FacadeMaterials`] del kit. El primer pueblo que se llena se construye de
//! una vez, para que el primer frame lo muestre; un pueblo que llega más tarde
//! por streaming construye sus edificios en el pool de cómputo asíncrono y
//! estos aparecen unos pocos frames después de su raíz, todavía bastante más
//! allá de [`crate::settlement::BUILDING_FAR`] (el margen de activación es más
//! amplio). Los senderos, monumentos, bancos y rejas de las plazas se
//! construyen de la misma forma. Los edificios, las veredas, los objetos
//! pequeños y los árboles se dibujan dentro de los [`ranges`] compartidos de
//! los asentamientos: ningún [`VisibilityRange`] nuevo.
//!
//! Las calles no tienen surcos de ruedas: la gente camina y cabalga por ellas.
//! Las huellas de carro se dibujan solo por donde pasan los carros: desde la
//! calle hasta la puerta de cada edificio de cargas del patio de la estación
//! ([`goods_tracks`]; las huellas de las estancias son de los asentamientos).
//! Las calles y los palenques son una malla por superficie y por pueblo, dentro
//! de los rangos de los edificios y de los objetos.
//!
//! Una vereda de ladrillo es una franja de pavimento a [`SIDEWALK_TOP`] sobre
//! el suelo con un cordón de revoque a [`CURB_TOP`] sobre él del lado de la
//! calzada; una vereda de tierra tiene el mismo cordón y, como piso, un decal
//! difuminado de la tierra pisoteada de los patios ([`TownEarthWalk`]) unos
//! pocos centímetros por encima del suelo. Ambas se dibujan sobre el suelo como
//! las calles y los senderos de la plaza, por los que core hace caminar a la
//! gente a la altura propia del suelo.

use bevy::camera::visibility::VisibilityRange;
use bevy::prelude::*;
use bevy::tasks::futures::check_ready;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use pampa_core::town::{
    PlantKind, PlazaFurniture, ShadeKind, ShadeTree, Sidewalk, Walk, YardPlant,
};
use pampa_core::{
    Aabb, Building, BuildingKind, Floor, Side, StoreStyle, Terrain, TownActivation, TownLayout,
    Vec2 as GroundVec2,
};

use crate::coords::world_from_ground;
use crate::facade::palenque;
use crate::facade::shape::Shape;
use crate::facade::town_buildings::{self, Lot};
use crate::facade::{FacadeMaterials, Surface};
use crate::rail::TrackKit;
use crate::railbed::{Parts, Track};
use crate::settlement::{props, ranges};
use crate::terrain::{StreamFocus, WorldTerrain, hash2};
use crate::tree::{TreeKit, TreeVariant, VARIANTS, plaza_species, street_species, yard_species};

/// Todos los pueblos del mundo, en el orden en que los ubicó el juego; el
/// índice de un pueblo aquí es su id en todo render. Sin esto no se dibuja
/// nada.
#[derive(Resource, Debug, Clone, Default)]
pub struct WorldTowns(pub Vec<TownLayout>);

/// Qué pueblos están cerca del jinete (ver [`TownActivation`]). Render
/// construye los pueblos activos; el juego simula a su gente.
#[derive(Resource, Debug, Clone, Default)]
pub struct ActiveTowns(pub TownActivation);

/// Raíz de un pueblo dibujado: cada malla, cartel y vagón del pueblo `.0`
/// cuelga de ella, así que eliminarla quita todo el pueblo.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TownRoot(pub usize);

/// Actualiza [`ActiveTowns`] y crea o elimina los [`TownRoot`]; corre en
/// `PreUpdate`, para que cada sistema de `Update` vea los pueblos de este
/// frame.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct StreamTowns;

/// Dibuja los [`WorldTowns`] cercanos al jinete y hace girar los rotores de los
/// molinos de viento.
pub struct TownPlugin;

impl Plugin for TownPlugin {
    fn build(&self, app: &mut App) {
        crate::facade::ensure_plugin(app);
        crate::tree::ensure_plugin(app);
        app.init_resource::<WorldTowns>()
            .init_resource::<ActiveTowns>()
            .init_resource::<StreamFocus>()
            .init_resource::<WorldTerrain>()
            .add_systems(Startup, setup_kit)
            .add_systems(
                PreUpdate,
                (stream_towns, land_buildings).chain().in_set(StreamTowns),
            )
            .add_systems(Update, spin_rotors);
    }
}

/// Rueda del molino de viento, que gira alrededor de su eje Z local.
#[derive(Component, Debug, Clone, Copy)]
pub struct WindmillRotor {
    /// Radianes por segundo.
    pub speed: f32,
}

fn spin_rotors(time: Res<Time>, mut rotors: Query<(&WindmillRotor, &mut Transform)>) {
    for (rotor, mut transform) in &mut rotors {
        transform.rotate_local_z(rotor.speed * time.delta_secs());
    }
}

/// Una pieza dibujada del edificio `.0` de su pueblo (un índice en
/// [`TownLayout::buildings`]).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TownBuilding(pub usize);

/// Una pieza dibujada de la plaza `.0` de su pueblo (un índice en
/// [`TownLayout::plazas`]): sus senderos y su monumento, o sus bancos y su
/// reja.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TownPlaza(pub usize);

/// Una pieza de las veredas de un pueblo: el anillo de una manzana o el de la
/// plaza.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TownSidewalk;

/// Las veredas de tierra apisonada de las manzanas de un pueblo (y de una
/// aldea): un único decal difuminado
/// ([`crate::settlement::track::walks_mesh`]); sus cordones son mallas de
/// vereda como las de ladrillo.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TownEarthWalk;

/// Las cazuelas de los árboles de calle de una ciudad: una malla de borde y
/// tierra.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TownTreePit;

/// La calzada de tierra de un pueblo: la tierra de sus calles, o los surcos de
/// ruedas gastados en ella (una malla por superficie).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TownStreet;

/// Los palenques de un pueblo, entre ellos la barra para atar caballos del
/// almacén: una sola malla de madera para todos.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TownPalenque;

/// Mallas compartidas, escaladas por instancia.
#[derive(Resource)]
struct Shapes {
    cube: Handle<Mesh>,
    /// La rueda del molino de viento, una malla por superficie (la rueda del
    /// molino de los asentamientos).
    rotor: Vec<(Surface, Handle<Mesh>)>,
}

/// Paleta terrosa de las calles, la plaza, los topes de fin de vía y el andén.
#[derive(Resource)]
struct Palette {
    timber: Handle<StandardMaterial>,
    plaza: Handle<StandardMaterial>,
    concrete: Handle<StandardMaterial>,
}

impl Palette {
    fn new(materials: &mut Assets<StandardMaterial>) -> Self {
        let mut matte = |r: f32, g: f32, b: f32| {
            materials.add(StandardMaterial {
                base_color: Color::srgb(r, g, b),
                perceptual_roughness: 0.95,
                reflectance: 0.15,
                ..default()
            })
        };
        let timber = matte(0.34, 0.25, 0.18);
        let plaza = matte(0.47, 0.46, 0.29);
        let concrete = matte(0.62, 0.59, 0.54);
        Self {
            timber,
            plaza,
            concrete,
        }
    }

    /// Los materiales de la vía, con el cubo compartido, para [`TrackKit`].
    fn track_kit(&self, cube: Handle<Mesh>) -> TrackKit {
        TrackKit {
            cube,
            concrete: self.concrete.clone(),
            timber: self.timber.clone(),
        }
    }
}

/// Ayudante de creación que mantiene corto el código por elemento. Todo lo que
/// crea cuelga del `root` del pueblo, dentro de `range` cuando se fija uno.
struct Builder<'c, 'w, 's, 'a> {
    commands: &'c mut Commands<'w, 's>,
    shapes: &'a Shapes,
    palette: &'a Palette,
    trees: &'a TreeKit,
    terrain: &'a Terrain,
    root: Entity,
    range: Option<VisibilityRange>,
}

impl Builder<'_, '_, '_, '_> {
    fn spawn(&mut self, mesh: &Handle<Mesh>, material: &Handle<StandardMaterial>, t: Transform) {
        let mut e = self.commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            t,
            ChildOf(self.root),
        ));
        if let Some(range) = &self.range {
            e.insert(range.clone());
        }
    }

    /// Caja alineada con los ejes desde el `center` del mundo con el `size` del
    /// mundo.
    fn cube(&mut self, material: &Handle<StandardMaterial>, center: Vec3, size: Vec3) {
        let mesh = self.shapes.cube.clone();
        self.spawn(
            &mesh,
            material,
            Transform::from_translation(center).with_scale(size),
        );
    }

    fn ground(&self, p: GroundVec2) -> f32 {
        self.terrain.height(p.x, p.y)
    }
}

fn setup_kit(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let palette = Palette::new(&mut materials);
    // La línea ferroviaria entre pueblos se dibuja con el mismo kit de vía.
    commands.insert_resource(palette.track_kit(cube.clone()));
    let rotor = props::rotor()
        .into_meshes()
        .into_iter()
        .map(|(surface, mesh)| (surface, meshes.add(mesh)))
        .collect();
    commands.insert_resource(Shapes { cube, rotor });
    commands.insert_resource(palette);
}

/// Mallas y materiales compartidos de los pueblos, una vez que corrieron
/// [`setup_kit`], el kit de fachadas y el kit de árboles.
type TownKit<'a> = (
    Option<Res<'a, Shapes>>,
    Option<Res<'a, Palette>>,
    Option<Res<'a, FacadeMaterials>>,
    Option<Res<'a, TreeKit>>,
);

/// Los edificios de un pueblo, mientras se construyen en el pool de cómputo
/// asíncrono; [`land_buildings`] los crea cuando están listos. Eliminar antes
/// la raíz del pueblo los descarta, lo que cancela la construcción.
#[derive(Component)]
struct PendingBuildings(Task<Vec<Built>>);

/// Activa los pueblos cercanos a [`StreamFocus`]: construye los recién
/// activados bajo un [`TownRoot`] nuevo y elimina las raíces de los que
/// quedaron atrás. La primera ejecución tiende los edificios de sus pueblos de
/// una vez; las siguientes los construyen fuera del hilo principal.
fn stream_towns(
    mut commands: Commands,
    (towns, terrain, focus): (Res<WorldTowns>, Res<WorldTerrain>, Res<StreamFocus>),
    mut active: ResMut<ActiveTowns>,
    (shapes, palette, surfaces, trees): TownKit,
    roots: Query<(Entity, &TownRoot)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut first_fill_laid: Local<bool>,
) {
    let (Some(shapes), Some(palette), Some(surfaces), Some(trees)) =
        (shapes, palette, surfaces, trees)
    else {
        return;
    };
    let change = active.0.update(&towns.0, focus.0);
    for (entity, root) in &roots {
        if change.deactivated.contains(&root.0) {
            commands.entity(entity).despawn();
        }
    }
    for index in change.activated {
        let town = &towns.0[index];
        let root = commands
            .spawn((
                Name::new(format!("Town {}", town.name)),
                TownRoot(index),
                Transform::default(),
                Visibility::default(),
            ))
            .id();
        let mut b = Builder {
            commands: &mut commands,
            shapes: &shapes,
            palette: &palette,
            trees: &trees,
            terrain: &terrain.0,
            root,
            range: None,
        };
        build_town(&mut b, town);
        spawn_track(
            &mut commands,
            root,
            town,
            &terrain.0,
            &surfaces,
            &mut meshes,
        );
        spawn_streets(
            &mut commands,
            root,
            town,
            &terrain.0,
            &surfaces,
            &mut meshes,
        );
        spawn_palenques(
            &mut commands,
            root,
            town,
            &terrain.0,
            &surfaces,
            &mut meshes,
        );
        spawn_sidewalks(
            &mut commands,
            root,
            town,
            &terrain.0,
            &surfaces,
            &mut meshes,
        );
        spawn_tree_pits(
            &mut commands,
            root,
            town,
            &terrain.0,
            &surfaces,
            &mut meshes,
        );
        let lots = lots(town, &terrain.0);
        let plazas = plaza_jobs(town, &terrain.0);
        if *first_fill_laid {
            let task = AsyncComputeTaskPool::get().spawn(async move { build_lots(&lots, &plazas) });
            commands.entity(root).insert(PendingBuildings(task));
        } else {
            let built = build_lots(&lots, &plazas);
            spawn_built(&mut commands, root, built, &shapes, &surfaces, &mut meshes);
        }
    }
    *first_fill_laid = true;
}

/// Crea los edificios de cada pueblo cuya construcción terminó.
fn land_buildings(
    mut commands: Commands,
    mut pending: Query<(Entity, &mut PendingBuildings)>,
    (shapes, surfaces): (Option<Res<Shapes>>, Option<Res<FacadeMaterials>>),
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let (Some(shapes), Some(surfaces)) = (shapes, surfaces) else {
        return;
    };
    for (root, mut build) in &mut pending {
        if let Some(built) = check_ready(&mut build.0) {
            commands.entity(root).remove::<PendingBuildings>();
            spawn_built(&mut commands, root, built, &shapes, &surfaces, &mut meshes);
        }
    }
}

/// Todas las mallas de `town` salvo sus edificios, bajo la raíz del
/// constructor.
fn build_town(b: &mut Builder, town: &TownLayout) {
    plazas(b, town);
    b.range = Some(ranges()[0].clone());
    for building in &town.buildings {
        if building.kind == BuildingKind::Platform {
            platform(b, building);
        }
    }
    for floor in town.floors() {
        floor_ramps(b, &floor);
    }
    b.range = None;
}

/// Altura de la calzada de tierra sobre el suelo, en metros: por debajo del
/// [`SIDEWALK_TOP`] de las veredas. Core hace caminar a la gente sobre ella a
/// la altura propia del suelo.
pub const STREET_TOP: f32 = 0.03;
/// Cuánto se extiende la calzada por debajo del suelo, en metros.
const STREET_FOOT: f32 = -0.05;
/// Altura de las huellas de carro sobre el nivel de la tierra, en metros: unos
/// pocos milímetros, para que nunca compitan con ella.
pub const RUT_LIFT: f32 = 0.006;
/// Ancho de un surco de rueda, en metros.
const RUT_WIDTH: f32 = 0.25;
/// Distancia entre los dos surcos de la huella de un carro, en metros.
const RUT_GAUGE: f32 = 1.4;
/// El trecho de surco más corto que vale la pena dibujar, en metros.
const RUT_MIN: f32 = 1.0;
/// Dónde empieza una huella de cargas desde la fachada, en metros.
const RUT_START: f32 = 0.3;
/// Distancia que mantiene una huella de cargas respecto de cualquier cosa que
/// esté en su camino, en metros.
const RUT_GAP: f32 = 0.3;
/// Hasta qué distancia de la puerta de un edificio de cargas se busca la calle
/// hacia la que corre su huella de carro, en metros.
const GOODS_REACH: f32 = 12.0;
/// Largo de la huella que sale de un edificio de cargas sin ninguna calle al
/// alcance: la explanada de carga, en metros.
const GOODS_APPROACH: f32 = 8.0;

/// Crea la calzada de `town` bajo `root`: una malla de tierra y otra de las
/// huellas de carro del patio de cargas, en el rango compartido de los
/// edificios, con UV en espacio del mundo para que las franjas se encuentren en
/// los cruces sin costura.
fn spawn_streets(
    commands: &mut Commands,
    root: Entity,
    town: &TownLayout,
    terrain: &Terrain,
    surfaces: &FacadeMaterials,
    meshes: &mut Assets<Mesh>,
) {
    let range = ranges()[0].clone();
    let (origin, shape) = street_shape(town, terrain);
    for (surface, mesh) in shape.into_meshes() {
        commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(surfaces.get(surface)),
            Transform::from_translation(origin),
            range.clone(),
            TownStreet,
            ChildOf(root),
        ));
    }
}

/// Las calles de `town` como una sola forma construida alrededor de `origin`,
/// el medio de su grilla sobre el suelo: cada franja es una losa de
/// [`Surface::Earth`] con su parte superior en [`STREET_TOP`], y las
/// [`goods_tracks`] en [`Surface::Mud`]. Las calles mismas no tienen surcos: la
/// gente camina y cabalga por ellas.
fn street_shape(town: &TownLayout, terrain: &Terrain) -> (Vec3, Shape) {
    let middle = town.grid().center();
    let origin = world_from_ground(middle, terrain.height(middle.x, middle.y));
    let mut shape = Shape::default();
    for street in &town.streets {
        let c = street.center();
        let ground = terrain.height(c.x, c.y) - origin.y;
        let earth = (ground, STREET_FOOT, STREET_TOP);
        ground_box(
            &mut shape,
            Surface::Earth,
            (street.min, street.max),
            origin,
            earth,
        );
    }
    for rut in goods_tracks(town) {
        let c = rut.center();
        let top = terrain.height(c.x, c.y) - origin.y + STREET_TOP + RUT_LIFT;
        let (p, q) = (
            world_from_ground(rut.min, 0.0) - origin,
            world_from_ground(rut.max, 0.0) - origin,
        );
        let (lo, hi) = (p.min(q), p.max(q));
        // Visto desde arriba, en sentido antihorario.
        shape.face(
            Surface::Mud,
            &[
                Vec3::new(lo.x, top, lo.z),
                Vec3::new(lo.x, top, hi.z),
                Vec3::new(hi.x, top, hi.z),
                Vec3::new(hi.x, top, lo.z),
            ],
        );
    }
    (origin, shape)
}

/// El vector unitario del suelo que apunta hacia afuera de `side`.
fn side_vector(side: Side) -> GroundVec2 {
    match side {
        Side::North => GroundVec2::new(0.0, 1.0),
        Side::South => GroundVec2::new(0.0, -1.0),
        Side::East => GroundVec2::new(1.0, 0.0),
        Side::West => GroundVec2::new(-1.0, 0.0),
    }
}

/// Si `b` es uno de los edificios de cargas del ferrocarril: un galpón o
/// depósito de cereal del patio de la estación, junto a la línea fuera de la
/// grilla o en la franja de una línea de paso, no uno de los galpones entre las
/// casas.
fn is_goods_yard(town: &TownLayout, b: &Building) -> bool {
    matches!(b.kind, BuildingKind::Shed | BuildingKind::GrainStore)
        && (town
            .rail_band()
            .is_some_and(|band| band.contains(&b.footprint))
            || !town.grid().contains(&b.footprint))
}

/// Qué largo tiene la huella de carro que sale de la `door` de un edificio de
/// cargas que mira hacia `n`: hasta un cuarto de metro antes de la primera de
/// las `streets` dentro de [`GOODS_REACH`], o si no [`GOODS_APPROACH`]. `None`
/// cuando la calle está tan cerca (o la puerta ya está sobre ella, donde las
/// calles no tienen surcos) que no cabe ningún trecho de [`RUT_MIN`]: la calle
/// es la explanada de la puerta.
fn goods_track_length(streets: &[Aabb], door: GroundVec2, n: GroundVec2) -> Option<f32> {
    let to_street = (0..=(GOODS_REACH * 4.0) as usize)
        .map(|k| k as f32 * 0.25)
        .find(|d| streets.iter().any(|s| s.contains_point(door + n * *d)))
        .map(|d| d - 0.25);
    let length = to_street.unwrap_or(GOODS_APPROACH);
    (length - RUT_START >= RUT_MIN).then_some(length)
}

/// Las huellas de carro de `town`, sobre el suelo: desde la puerta de cada
/// edificio de cargas ([`is_goods_yard`]) en línea recta hacia afuera a lo
/// largo de su frente, dos surcos de rueda separados por [`RUT_GAUGE`], hasta
/// la calle si hay una dentro de [`GOODS_REACH`], o si no hasta
/// [`GOODS_APPROACH`] de distancia. Se detienen antes de lo que haya en el
/// camino (edificios, árboles, la línea, andenes, plazas, veredas, carteles y
/// palenques).
fn goods_tracks(town: &TownLayout) -> Vec<Aabb> {
    let mut keep_off: Vec<Aabb> = town.obstacles();
    for floor in town.floors() {
        keep_off.push(floor.footprint);
        keep_off.extend(floor.ramps());
    }
    keep_off.extend(town.canopies().iter().map(|c| c.footprint));
    keep_off.extend(town.rail_corridor());
    keep_off.extend(town.plazas());
    keep_off.extend(town.sidewalks().iter().map(|w| w.area));
    let mut out = Vec::new();
    for b in town.buildings.iter().filter(|b| is_goods_yard(town, b)) {
        let n = side_vector(b.front);
        let across = GroundVec2::new(-n.y, n.x);
        let half = b.footprint.size() * 0.5;
        let door = b.footprint.center() + n * (n.x.abs() * half.x + n.y.abs() * half.y);
        let Some(length) = goods_track_length(&town.streets, door, n) else {
            continue;
        };
        let own = b.collider();
        for wheel in [-1.0, 1.0] {
            let lane = across * (wheel * RUT_GAUGE / 2.0);
            // En qué lugar del frente corta el surco cada cosa que está en el
            // camino.
            let mut pieces = vec![(RUT_START, length)];
            for o in keep_off.iter().filter(|o| **o != own) {
                let at = |p: GroundVec2| ((p - door - lane).dot(n), (p - door - lane).dot(across));
                let (a, z) = (at(o.min), at(o.max));
                let (d_lo, d_hi) = (a.0.min(z.0) - RUT_GAP, a.0.max(z.0) + RUT_GAP);
                let (c_lo, c_hi) = (a.1.min(z.1), a.1.max(z.1));
                if c_hi <= -RUT_WIDTH / 2.0 || c_lo >= RUT_WIDTH / 2.0 {
                    continue;
                }
                pieces = pieces
                    .into_iter()
                    .flat_map(|(from, to)| {
                        if d_hi <= from || d_lo >= to {
                            vec![(from, to)]
                        } else {
                            vec![(from, d_lo), (d_hi, to)]
                        }
                    })
                    .collect();
            }
            for (from, to) in pieces.into_iter().filter(|(f, t)| t - f >= RUT_MIN) {
                let (p, q) = (door + lane + n * from, door + lane + n * to);
                let reach = GroundVec2::new(across.x.abs(), across.y.abs()) * (RUT_WIDTH / 2.0);
                out.push(Aabb::new(
                    GroundVec2::new(p.x.min(q.x), p.y.min(q.y)) - reach,
                    GroundVec2::new(p.x.max(q.x), p.y.max(q.y)) + reach,
                ));
            }
        }
    }
    out
}

/// Crea los palenques de `town` bajo `root`: una malla de madera, dentro del
/// rango de los objetos pequeños.
fn spawn_palenques(
    commands: &mut Commands,
    root: Entity,
    town: &TownLayout,
    terrain: &Terrain,
    surfaces: &FacadeMaterials,
    meshes: &mut Assets<Mesh>,
) {
    let range = ranges()[1].clone();
    let (origin, shape) = palenque_shape(town, terrain);
    for (surface, mesh) in shape.into_meshes() {
        commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(surfaces.get(surface)),
            Transform::from_translation(origin),
            range.clone(),
            TownPalenque,
            ChildOf(root),
        ));
    }
}

/// Las barras de `town` como una sola forma construida alrededor de `origin`,
/// el medio de su grilla sobre el suelo: la barra para atar caballos del
/// almacén de ramos generales y cada palenque, cada uno a lo largo del lado
/// largo de su huella.
fn palenque_shape(town: &TownLayout, terrain: &Terrain) -> (Vec3, Shape) {
    let middle = town.grid().center();
    let origin = world_from_ground(middle, terrain.height(middle.x, middle.y));
    let mut shape = Shape::default();
    let rails =
        std::iter::once(town.hitching_post.rail).chain(town.palenques.iter().map(|p| p.rail));
    for rail in rails {
        let size = rail.size();
        let along_x = size.x >= size.y;
        let c = rail.center();
        let at = world_from_ground(c, terrain.height(c.x, c.y)) - origin;
        // Una barra a lo largo de la y del suelo corre a lo largo de la z del
        // mundo.
        let turn = if along_x {
            0.0
        } else {
            std::f32::consts::FRAC_PI_2
        };
        shape.merge(
            palenque::palenque(size.x.max(size.y)),
            Transform::from_translation(at).with_rotation(Quat::from_rotation_y(turn)),
        );
    }
    (origin, shape)
}

/// Altura del pavimento de ladrillo de una vereda sobre el suelo, en metros:
/// apenas por encima de los 0.03 de la calle y por debajo de los 0.07 de los
/// senderos de la plaza.
pub const SIDEWALK_TOP: f32 = 0.06;
/// Altura de su cordón sobre el suelo, en metros: 3 cm por encima del
/// pavimento.
pub const CURB_TOP: f32 = 0.09;
/// Ancho de un cordón, en metros.
const CURB_WIDTH: f32 = 0.15;
/// Cuánto se extienden las veredas por debajo del suelo, en metros.
const SIDEWALK_FOOT: f32 = -0.05;

/// Crea las veredas de `town` bajo `root`: una entidad por cada grupo de
/// franjas que se tocan y por superficie (pavimento, cordón), dentro del rango
/// compartido de los edificios.
fn spawn_sidewalks(
    commands: &mut Commands,
    root: Entity,
    town: &TownLayout,
    terrain: &Terrain,
    surfaces: &FacadeMaterials,
    meshes: &mut Assets<Mesh>,
) {
    let range = ranges()[0].clone();
    // El piso transitable de cada anillo de veredas de tierra, dentro de sus
    // cordones.
    let rings: Vec<Vec<Aabb>> = town
        .sidewalks()
        .iter()
        .filter(|walk| walk.walk == Walk::Earth)
        .collect::<Vec<_>>()
        .chunks(4)
        .map(|ring| ring.iter().map(|walk| walk_floor(walk)).collect())
        .collect();
    let ground = |p: GroundVec2| terrain.height(p.x, p.y);
    if let Some(mesh) = crate::settlement::track::walks_mesh(&rings, &ground) {
        commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(surfaces.get(Surface::Yard)),
            Transform::default(),
            range.clone(),
            TownEarthWalk,
            ChildOf(root),
        ));
    }
    for (origin, shape) in sidewalk_shapes(town.sidewalks(), terrain) {
        for (surface, mesh) in shape.into_meshes() {
            commands.spawn((
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(surfaces.get(surface)),
                Transform::from_translation(origin),
                range.clone(),
                TownSidewalk,
                ChildOf(root),
            ));
        }
    }
}

/// Los grupos de `walks` que se tocan (las cuatro franjas de un anillo), cada
/// uno construido alrededor del medio de su primera franja sobre el suelo:
/// dónde se ubica en el mundo, y su forma allí.
fn sidewalk_shapes(walks: &[Sidewalk], terrain: &Terrain) -> Vec<(Vec3, Shape)> {
    let mut groups: Vec<(Vec<Aabb>, Vec3, Shape)> = Vec::new();
    for walk in walks {
        let touches = |areas: &Vec<Aabb>| {
            areas
                .iter()
                .any(|a| a.expanded(0.01).intersects(&walk.area))
        };
        let mut touching = (0..groups.len()).filter(|&k| touches(&groups[k].0));
        let k = match touching.next() {
            Some(k) => {
                // Una franja que une dos grupos (la franja este de un anillo,
                // entre la norte y la sur) los fusiona.
                let rest: Vec<usize> = touching.collect();
                for j in rest.into_iter().rev() {
                    let (areas, origin, shape) = groups.remove(j);
                    let into = &mut groups[k];
                    into.0.extend(areas);
                    into.2
                        .merge(shape, Transform::from_translation(origin - into.1));
                }
                k
            }
            None => {
                let c = walk.area.center();
                let origin = world_from_ground(c, terrain.height(c.x, c.y));
                groups.push((Vec::new(), origin, Shape::default()));
                groups.len() - 1
            }
        };
        let (areas, origin, shape) = &mut groups[k];
        areas.push(walk.area);
        let c = walk.area.center();
        let ground = terrain.height(c.x, c.y) - origin.y;
        sidewalk(shape, walk, *origin, ground);
    }
    groups
        .into_iter()
        .map(|(_, origin, shape)| (origin, shape))
        .collect()
}

/// Qué tan ancho es el borde de una cazuela, en metros.
const PIT_RIM: f32 = 0.1;
/// Cuánto sobresale la tierra de una cazuela por encima del pavimento, en
/// metros.
const PIT_EARTH_LIFT: f32 = 0.005;

/// Crea las cazuelas de los árboles de calle de `town` bajo `root`: una malla
/// de bordes a la altura del cordón y otra de tierra, en el rango de las
/// veredas.
fn spawn_tree_pits(
    commands: &mut Commands,
    root: Entity,
    town: &TownLayout,
    terrain: &Terrain,
    surfaces: &FacadeMaterials,
    meshes: &mut Assets<Mesh>,
) {
    let range = ranges()[0].clone();
    let Some((origin, shape)) = tree_pit_shape(town, terrain) else {
        return;
    };
    for (surface, mesh) in shape.into_meshes() {
        commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(surfaces.get(surface)),
            Transform::from_translation(origin),
            range.clone(),
            TownTreePit,
            ChildOf(root),
        ));
    }
}

/// Las cazuelas de los árboles [`ShadeKind::Pit`] de `town` como una sola forma
/// alrededor de `origin`, el medio de su grilla sobre el suelo: un cuadrado de
/// borde de revoque (tan alto como el cordón) alrededor de [`Surface::Earth`],
/// sobre el pavimento. `None` para un pueblo sin árboles de calle.
fn tree_pit_shape(town: &TownLayout, terrain: &Terrain) -> Option<(Vec3, Shape)> {
    let pits: Vec<&ShadeTree> = town
        .shade_trees
        .iter()
        .filter(|t| t.kind == ShadeKind::Pit)
        .collect();
    if pits.is_empty() {
        return None;
    }
    let middle = town.grid().center();
    let origin = world_from_ground(middle, terrain.height(middle.x, middle.y));
    let mut shape = Shape::default();
    let (outer, inner) = (
        ShadeTree::PIT_SIDE / 2.0,
        ShadeTree::PIT_SIDE / 2.0 - PIT_RIM,
    );
    for tree in pits {
        let ground = terrain.height(tree.at.x, tree.at.y) - origin.y;
        let corner = |dx: f32, dy: f32| tree.at + GroundVec2::new(dx, dy);
        let rim = (ground, SIDEWALK_FOOT, CURB_TOP);
        // Los bordes sur y norte enteros, los oeste y este entre ellos.
        for (a, b) in [
            (corner(-outer, -outer), corner(outer, -inner)),
            (corner(-outer, inner), corner(outer, outer)),
            (corner(-outer, -inner), corner(-inner, inner)),
            (corner(inner, -inner), corner(outer, inner)),
        ] {
            ground_box(&mut shape, Surface::Stucco, (a, b), origin, rim);
        }
        let earth = (ground, SIDEWALK_FOOT, SIDEWALK_TOP + PIT_EARTH_LIFT);
        let soil = (corner(-inner, -inner), corner(inner, inner));
        ground_box(&mut shape, Surface::Earth, soil, origin, earth);
    }
    Some((origin, shape))
}

/// La caja entre las esquinas del suelo `a` y `b`, desde `low` hasta `high`
/// sobre `ground`, en el espacio de un grupo ubicado en `origin`.
fn ground_box(
    shape: &mut Shape,
    surface: Surface,
    (a, b): (GroundVec2, GroundVec2),
    origin: Vec3,
    (ground, low, high): (f32, f32, f32),
) {
    let (p, q) = (
        world_from_ground(a, 0.0) - origin,
        world_from_ground(b, 0.0) - origin,
    );
    shape.block(
        surface,
        p.min(q).with_y(ground + low),
        p.max(q).with_y(ground + high),
    );
}

/// El piso de `walk`: su área menos el cordón a lo largo de cada borde con
/// cordón.
fn walk_floor(walk: &Sidewalk) -> Aabb {
    let (mut lo, mut hi) = (walk.area.min, walk.area.max);
    for side in &walk.curbs {
        match side {
            Side::South => lo.y += CURB_WIDTH,
            Side::North => hi.y -= CURB_WIDTH,
            Side::West => lo.x += CURB_WIDTH,
            Side::East => hi.x -= CURB_WIDTH,
        }
    }
    Aabb::new(lo, hi)
}

/// Una franja: pavimento de ladrillo (ninguno en una vereda de tierra), y un
/// cordón a lo largo de cada uno de sus bordes con cordón (el primero entero,
/// los demás deteniéndose antes de él).
fn sidewalk(shape: &mut Shape, walk: &Sidewalk, origin: Vec3, ground: f32) {
    let w = CURB_WIDTH;
    let a = walk.area;
    // El pavimento de ladrillo se detiene en los cordones; una vereda de tierra
    // no tiene (su piso es un decal, [`walk_floor`]).
    if walk.walk == Walk::Brick {
        let floor = walk_floor(walk);
        let paving = (ground, SIDEWALK_FOOT, SIDEWALK_TOP);
        ground_box(
            shape,
            Surface::Paving,
            (floor.min, floor.max),
            origin,
            paving,
        );
    }
    let first = walk.curbs.first().copied();
    for (k, &side) in walk.curbs.iter().enumerate() {
        let (mut lo, mut hi) = (a.min, a.max);
        match side {
            Side::South => hi.y = lo.y + w,
            Side::North => lo.y = hi.y - w,
            Side::West => hi.x = lo.x + w,
            Side::East => lo.x = hi.x - w,
        }
        if k > 0 {
            match first {
                Some(Side::South) => lo.y = lo.y.max(a.min.y + w),
                Some(Side::North) => hi.y = hi.y.min(a.max.y - w),
                Some(Side::West) => lo.x = lo.x.max(a.min.x + w),
                Some(Side::East) => hi.x = hi.x.min(a.max.x - w),
                None => {}
            }
        }
        let curb = (ground, SIDEWALK_FOOT, CURB_TOP);
        ground_box(shape, Surface::Stucco, (lo, hi), origin, curb);
    }
}

/// Espesor de la losa de una rampa, en metros.
const RAMP_THICKNESS: f32 = 0.2;

/// Una losa de hormigón que baja desde cada extremo de la parte superior de un
/// andén hasta el suelo, a lo largo de las rampas por las que la lógica hace
/// caminar a la gente ([`Floor::rise_at`]).
fn floor_ramps(b: &mut Builder, floor: &Floor) {
    let concrete = b.palette.concrete.clone();
    let f = floor.footprint;
    let mid = floor.across() * f.center().dot(floor.across());
    let width = f.size().dot(floor.across());
    let (lo, hi) = (f.min.dot(floor.along), f.max.dot(floor.along));
    for (end, out) in [(lo, -1.0), (hi, 1.0)] {
        // Desde el borde del andén (arriba) hasta el pie de la rampa (suelo).
        let top = mid + floor.along * end;
        let foot = top + floor.along * (out * Floor::RAMP_LENGTH);
        let (top3, foot3) = (
            world_from_ground(top, b.ground(top) + floor.height),
            world_from_ground(foot, b.ground(foot)),
        );
        let slope = (foot3 - top3).normalize();
        let side = slope.cross(Vec3::Y).normalize();
        let up = side.cross(slope);
        let rotation = Quat::from_mat3(&Mat3::from_cols(slope, up, side));
        let center = (top3 + foot3) * 0.5 - up * (RAMP_THICKNESS / 2.0);
        let mesh = b.shapes.cube.clone();
        b.spawn(
            &mesh,
            &concrete,
            Transform::from_translation(center)
                .with_rotation(rotation)
                .with_scale(Vec3::new(top3.distance(foot3), RAMP_THICKNESS, width)),
        );
    }
}

/// Una losa delgada apoyada sobre el suelo (nivelado) en `area`.
fn flat_patch(b: &mut Builder, material: &Handle<StandardMaterial>, area: &Aabb, lift: f32) {
    let center = area.center();
    let size = area.size();
    let y = b.ground(center);
    b.cube(
        material,
        world_from_ground(center, y + lift - 0.05),
        Vec3::new(size.x, 0.1, size.y),
    );
}

/// Cada plaza (la principal, y la segunda de un pueblo grande), luego los
/// árboles de todas ellas, y los árboles de calle, de isleta y de patio.
fn plazas(b: &mut Builder, town: &TownLayout) {
    for p in town.plazas() {
        plaza(b, p);
    }
    plaza_trees(b, town);
    shade_trees(b, town);
    yard_plants(b, town);
}

/// El césped de una plaza. Sus senderos, monumento, bancos y reja se construyen
/// con los edificios ([`plaza_jobs`], [`crate::facade::plaza`]).
fn plaza(b: &mut Builder, p: Aabb) {
    let lawn = b.palette.plaza.clone();
    flat_patch(b, &lawn, &p, crate::facade::plaza::LAWN_TOP);
}

/// Cada árbol de plaza: una variante de la especie de su plaza
/// ([`plaza_species`]) del [`TreeKit`] compartido, su corteza y sus hojas,
/// girado y escalado un poco por árbol. Los árboles son sólidos: en lugar de
/// desvanecerlos, la cámara de seguimiento se acerca alrededor de ellos.
fn plaza_trees(b: &mut Builder, town: &TownLayout) {
    for (plaza, tree) in plaza_tree_sites(town) {
        let kit = b.trees.species(plaza_species(town.seed, plaza));
        let (bark, leaves) = (kit.bark.clone(), kit.leaves.clone());
        let ground = b.ground(tree.at);
        let variant = &kit.variants[tree.variant];
        let transform = Transform::from_translation(world_from_ground(tree.at, ground - TREE_SINK))
            .with_rotation(Quat::from_rotation_y(tree.yaw))
            .with_scale(Vec3::splat(tree.scale));
        spawn_tree(b, (variant, &bark, &leaves), transform, TownTree);
    }
}

/// La corteza y las hojas de un árbol, dos entidades que comparten las mallas y
/// los materiales del kit, etiquetadas con `marker`.
fn spawn_tree(
    b: &mut Builder,
    (variant, bark, leaves): (
        &TreeVariant,
        &Handle<StandardMaterial>,
        &Handle<StandardMaterial>,
    ),
    transform: Transform,
    marker: impl Bundle + Copy,
) {
    for (mesh, material) in [(&variant.bark, bark), (&variant.leaves, leaves)] {
        b.commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            transform,
            ranges()[2].clone(),
            marker,
            ChildOf(b.root),
        ));
    }
}

/// Cada árbol de calle, de isleta y de patio ([`TownLayout::shade_trees`]) del
/// [`TreeKit`] compartido: un árbol de calle es una de las variantes podadas de
/// las especies de calle (un tronco desnudo hasta unos 4 m, una copa angosta
/// inclinada sobre la calle), un árbol de isleta es una variante de la especie
/// de su plaza, y un árbol de patio, de cualquier especie, escalado hacia abajo
/// para que entre en el espacio de su copa.
fn shade_trees(b: &mut Builder, town: &TownLayout) {
    let plazas: Vec<Aabb> = town.plazas().collect();
    for tree in &town.shade_trees {
        let look = shade_look(town.seed, &plazas, tree);
        let kit = b.trees.species(look.species);
        let (bark, leaves) = (kit.bark.clone(), kit.leaves.clone());
        let variants = if look.trimmed {
            &kit.pruned
        } else {
            &kit.variants
        };
        let variant = &variants[look.variant];
        let transform = shade_transform(tree, &look, variant.crown_radius, b.ground(tree.at));
        spawn_tree(b, (variant, &bark, &leaves), transform, TownShadeTree);
    }
}

/// Cada arbusto de patio y cantero de flores ([`TownLayout::yard_plants`]): una
/// malla de planta compartida girada un poco, en el rango de los objetos
/// pequeños.
fn yard_plants(b: &mut Builder, town: &TownLayout) {
    let plants = &b.trees.plants;
    for plant in &town.yard_plants {
        let (mesh, material) = (plant_mesh(plants, plant), plants.material.clone());
        let ground = b.ground(plant.at);
        let turn = hash2(plant.at.y + 11.0, plant.at.x) * std::f32::consts::TAU;
        b.commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::from_translation(world_from_ground(plant.at, ground))
                .with_rotation(Quat::from_rotation_y(turn)),
            ranges()[1].clone(),
            TownYardPlant,
            ChildOf(b.root),
        ));
    }
}

/// La malla compartida con la que se dibuja `plant`, una de las variantes de su
/// tipo elegida según su lugar.
fn plant_mesh(plants: &crate::tree::plants::PlantKit, plant: &YardPlant) -> Handle<Mesh> {
    let pick = |meshes: &[Handle<Mesh>]| {
        let k = (hash2(plant.at.x + 5.0, plant.at.y - 9.0) * meshes.len() as f32) as usize;
        meshes[k % meshes.len()].clone()
    };
    match plant.kind {
        PlantKind::Shrub => pick(&plants.shrubs),
        PlantKind::Flowers => pick(&plants.flowers),
    }
}

/// Uno de los arbustos de patio o canteros de flores dibujados de un pueblo.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TownYardPlant;

/// Uno de los árboles de calle, de isleta o de patio dibujados de un pueblo (su
/// corteza o sus hojas).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TownShadeTree;

/// Como qué árbol se dibuja un [`ShadeTree`].
#[derive(Debug, Clone, Copy, PartialEq)]
struct ShadeLook {
    species: crate::tree::Species,
    /// Se dibuja a partir de las variantes podadas.
    trimmed: bool,
    variant: usize,
    yaw: f32,
    /// Con semilla según su lugar, de 0 a 1: su tamaño dentro del rango.
    jitter: f32,
}

/// El aspecto de `tree` en el pueblo con semilla `seed` con `plazas`: los
/// árboles de calle de una hilera de vereda comparten especie; los demás siguen
/// la plaza o el lugar.
fn shade_look(seed: u32, plazas: &[Aabb], tree: &ShadeTree) -> ShadeLook {
    let at = tree.at;
    let (a, b) = (hash2(at.x, at.y), hash2(at.y + 7.0, at.x));
    let spot = (a * 65_536.0) as u32;
    let species = match tree.kind {
        ShadeKind::Pit => street_species(seed, tree.row),
        ShadeKind::Front => street_species(seed, spot),
        ShadeKind::Grove => plaza_species(seed, plaza_of(plazas, at)),
        ShadeKind::Yard => yard_species(seed, at),
    };
    ShadeLook {
        species,
        trimmed: matches!(tree.kind, ShadeKind::Pit | ShadeKind::Front),
        variant: (hash2(at.x + 1.0, at.y + 2.0) * VARIANTS as f32) as usize % VARIANTS,
        yaw: b * std::f32::consts::TAU,
        jitter: hash2(at.x + 3.0, at.y - 5.0),
    }
}

/// Dónde y cómo se ubica una variante de `crown_radius` para `tree` sobre
/// `ground`. Un árbol podado conserva su altura (un poco más alto, para que su
/// copa quede por encima de 3.5 m), nunca es más ancho que su copa y se inclina
/// [`ShadeTree::LEAN`] grados hacia la calle; cualquier otro se escala de forma
/// uniforme para que entre en el espacio de su copa. Un árbol sin dirección de
/// calle (cero, o vertical) se mantiene derecho.
fn shade_transform(
    tree: &ShadeTree,
    look: &ShadeLook,
    crown_radius: f32,
    ground: f32,
) -> Transform {
    let size = TREE_SCALE_MIN + (TREE_SCALE_MAX - TREE_SCALE_MIN) * look.jitter;
    let fit = tree.crown / crown_radius;
    let at = world_from_ground(tree.at, ground - TREE_SINK);
    let yaw = Quat::from_rotation_y(look.yaw);
    if !look.trimmed {
        return Transform::from_translation(at)
            .with_rotation(yaw)
            .with_scale(Vec3::splat(size.min(fit)));
    }
    let wide = size.min(fit);
    let toward = Vec3::new(tree.toward.x, 0.0, -tree.toward.y);
    let lean = Vec3::Y
        .cross(toward)
        .try_normalize()
        .map_or(Quat::IDENTITY, |axis| {
            Quat::from_axis_angle(axis, ShadeTree::LEAN.to_radians())
        });
    Transform::from_translation(at)
        .with_rotation(lean * yaw)
        .with_scale(Vec3::new(wide, 1.0 + 0.08 * look.jitter, wide))
}

/// Uno de los árboles de plaza dibujados de un pueblo (su corteza o sus hojas).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TownTree;

/// Dónde y cómo se ubica un árbol de plaza.
#[derive(Debug, Clone, Copy, PartialEq)]
struct TreeSite {
    at: GroundVec2,
    variant: usize,
    yaw: f32,
    /// Escala uniforme, de [`TREE_SCALE_MIN`] a [`TREE_SCALE_MAX`].
    scale: f32,
}

/// Lo más grande que se dibuja un árbol de plaza respecto de su variante: su
/// tronco se mantiene dentro de [`pampa_core::town::TREE_TRUNK_RADIUS`] de
/// core.
pub(crate) const TREE_SCALE_MAX: f32 = 1.08;

/// Lo más pequeño que se dibuja un árbol de plaza respecto de su variante: la
/// escala de cada árbol queda entre esto y [`TREE_SCALE_MAX`], con semilla
/// según su lugar.
const TREE_SCALE_MIN: f32 = 0.92;

/// Cuánto queda el pie de un árbol de plaza por debajo del suelo en su lugar,
/// en metros: la base plana del tronco queda enterrada donde el césped tiene un
/// poco de pendiente.
const TREE_SINK: f32 = 0.05;

/// Cada árbol de `town` con el índice de la plaza en la que está
/// ([`TownLayout::plazas`], [`plaza_of`]), su variante, giro y escala, con
/// semilla según su lugar.
fn plaza_tree_sites(town: &TownLayout) -> Vec<(usize, TreeSite)> {
    let plazas: Vec<Aabb> = town.plazas().collect();
    town.trees
        .iter()
        .map(|&at| {
            let plaza = plaza_of(&plazas, at);
            let (a, b) = (hash2(at.x, at.y), hash2(at.y + 7.0, at.x));
            let site = TreeSite {
                at,
                variant: (a * VARIANTS as f32) as usize % VARIANTS,
                yaw: b * std::f32::consts::TAU,
                scale: TREE_SCALE_MAX
                    - (TREE_SCALE_MAX - TREE_SCALE_MIN) * hash2(at.x + 3.0, at.y - 5.0),
            };
            (plaza, site)
        })
        .collect()
}

/// El índice de la plaza de `plazas` en la que está el árbol `at`. Core planta
/// cada árbol dentro de una plaza; uno que quede fuera de todas (después de un
/// cambio de disposición) se asigna a la más cercana, con una advertencia, en
/// lugar de asignarse en silencio a la plaza principal.
fn plaza_of(plazas: &[Aabb], at: GroundVec2) -> usize {
    if let Some(inside) = plazas.iter().position(|p| p.contains_point(at)) {
        return inside;
    }
    let away = |p: &Aabb| p.closest_point(at).distance(at);
    let nearest = (0..plazas.len())
        .min_by(|&a, &b| away(&plazas[a]).total_cmp(&away(&plazas[b])))
        .unwrap_or(0);
    warn!(
        "plaza tree at ({:.1}, {:.1}) stands in no plaza; drawn as plaza {nearest}'s",
        at.x, at.y
    );
    nearest
}

/// Crea la vía de la estación de `town` bajo `root`: el lecho de balasto, los
/// durmientes y los rieles con los que también se tiende la línea ferroviaria
/// entre pueblos (ver [`crate::railbed`]), una malla cada uno, sin rango: la
/// vía sale del pueblo hacia la línea.
fn spawn_track(
    commands: &mut Commands,
    root: Entity,
    town: &TownLayout,
    terrain: &Terrain,
    surfaces: &FacadeMaterials,
    meshes: &mut Assets<Mesh>,
) {
    let Some(rail) = town.railway else {
        return;
    };
    let mut parts = Parts::default();
    parts.add(&Track::straight(rail), &[], None, terrain);
    for (surface, mesh) in parts.into_meshes() {
        commands.spawn((
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(surfaces.get(surface)),
            Transform::default(),
            ChildOf(root),
        ));
    }
}

/// El andén de la estación: un piso de hormigón liso, con su parte superior a
/// la altura sobre la que core hace caminar a la gente.
fn platform(b: &mut Builder, building: &Building) {
    let concrete = b.palette.concrete.clone();
    let (c, size, h) = (
        building.footprint.center(),
        building.footprint.size(),
        building.height,
    );
    let y = b.ground(c);
    b.cube(
        &concrete,
        world_from_ground(c, y + (h - 0.2) / 2.0),
        Vec3::new(size.x, h + 0.2, size.y),
    );
}

/// Qué tan adentro del borde de la grilla de calles cuenta una casa como de las
/// afueras, en metros: más o menos una manzana.
const OUTSKIRTS: f32 = 50.0;

/// Un edificio del pueblo por construir, con el lugar donde se ubica.
#[derive(Debug, Clone, Copy)]
struct Job {
    index: usize,
    lot: Lot,
    /// El medio de su huella sobre el suelo, en el mundo.
    base: Vec3,
}

/// Cada edificio de `town` que construye el kit de fachadas (salvo el andén),
/// ubicado sobre `terrain`.
fn lots(town: &TownLayout, terrain: &Terrain) -> Vec<Job> {
    let grid = town.grid();
    let store = StoreStyle::of(town.size);
    town.buildings
        .iter()
        .enumerate()
        .filter(|(_, b)| b.kind != BuildingKind::Platform)
        .map(|(index, &building)| {
            let c = building.footprint.center();
            let edge = (c.x - grid.min.x)
                .min(grid.max.x - c.x)
                .min(c.y - grid.min.y)
                .min(grid.max.y - c.y);
            Job {
                index,
                lot: Lot {
                    building,
                    variant: variant(index, &building.footprint),
                    outskirts: edge < OUTSKIRTS,
                    store,
                    veranda: town.hitching_post.veranda,
                },
                base: world_from_ground(c, terrain.height(c.x, c.y)),
            }
        })
        .collect()
}

/// Una semilla bien mezclada a partir del índice y la huella de un edificio.
fn variant(index: usize, footprint: &Aabb) -> u32 {
    let mut v = (index as u32).wrapping_mul(0x9e37_79b9)
        ^ (footprint.min.x.to_bits()).wrapping_mul(0x85eb_ca6b)
        ^ (footprint.min.y.to_bits()).wrapping_mul(0xc2b2_ae35);
    v ^= v >> 16;
    v = v.wrapping_mul(0x7feb_352d);
    v ^= v >> 15;
    v = v.wrapping_mul(0x846c_a68b);
    v ^ (v >> 16)
}

/// Qué es una pieza construida de un pueblo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    /// El edificio `.0` de [`TownLayout::buildings`].
    Building(usize),
    /// La plaza `.0` de [`TownLayout::plazas`].
    Plaza(usize),
}

/// Una plaza del pueblo por construir: lo que core pone en ella, y su medio
/// sobre el suelo en el mundo.
#[derive(Debug, Clone)]
struct PlazaJob {
    index: usize,
    furniture: PlazaFurniture,
    base: Vec3,
}

/// Cada plaza de `town`, ubicada sobre `terrain`.
fn plaza_jobs(town: &TownLayout, terrain: &Terrain) -> Vec<PlazaJob> {
    town.plaza_furniture()
        .into_iter()
        .enumerate()
        .map(|(index, furniture)| {
            let c = furniture.area.center();
            PlazaJob {
                index,
                base: world_from_ground(c, terrain.height(c.x, c.y)),
                furniture,
            }
        })
        .collect()
}

/// Una pieza construida (un edificio o la parte de una plaza que corresponde a
/// un rango): sus mallas en su propio espacio, dónde se ubica, el índice de su
/// rango compartido y la rueda de un molino de viento.
struct Built {
    part: Part,
    transform: Transform,
    meshes: Vec<(Surface, Mesh)>,
    range: usize,
    rotor: Option<(Transform, WindmillRotor)>,
}

/// Construye las mallas de cada trabajo y luego las de cada plaza: sus
/// senderos, cordones y monumento dentro del rango de los edificios, y sus
/// bancos y reja dentro del de los objetos.
fn build_lots(jobs: &[Job], plazas: &[PlazaJob]) -> Vec<Built> {
    let plazas = plazas.iter().flat_map(|job| {
        let shapes = crate::facade::plaza::plaza(&job.furniture);
        let transform = Transform::from_translation(job.base);
        [(shapes.ground, 0), (shapes.furniture, 1)]
            .into_iter()
            .map(move |(shape, range)| Built {
                part: Part::Plaza(job.index),
                transform,
                meshes: shape.into_meshes(),
                range,
                rotor: None,
            })
    });
    jobs.iter()
        .filter_map(|job| {
            let shape = town_buildings::building(&job.lot)?;
            let kind = job.lot.building.kind;
            let transform = Transform::from_translation(job.base)
                .with_rotation(town_buildings::yaw(job.lot.building.front));
            let rotor = (kind == BuildingKind::Windmill).then(|| {
                let (hub, yaw) = town_buildings::rotor_place(&job.lot);
                let speed = 1.5 + hash2(job.index as f32, 7.7);
                (
                    Transform::from_translation(transform.transform_point(hub))
                        .with_rotation(transform.rotation * yaw),
                    WindmillRotor { speed },
                )
            });
            // Las cosas pequeñas se van antes que las casas.
            let range = match kind {
                BuildingKind::Windmill | BuildingKind::WaterTank => 1,
                _ => 0,
            };
            Some(Built {
                part: Part::Building(job.index),
                transform,
                meshes: shape.into_meshes(),
                range,
                rotor,
            })
        })
        .chain(plazas)
        .collect()
}

/// Crea `built` bajo `root`: una entidad por edificio y superficie.
fn spawn_built(
    commands: &mut Commands,
    root: Entity,
    built: Vec<Built>,
    shapes: &Shapes,
    surfaces: &FacadeMaterials,
    meshes: &mut Assets<Mesh>,
) {
    let ranges = ranges();
    for b in built {
        let range = &ranges[b.range];
        for (surface, mesh) in b.meshes {
            let mut e = commands.spawn((
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(surfaces.get(surface)),
                b.transform,
                range.clone(),
                ChildOf(root),
            ));
            match b.part {
                Part::Building(index) => e.insert(TownBuilding(index)),
                Part::Plaza(index) => e.insert(TownPlaza(index)),
            };
        }
        let (Some((transform, rotor)), Part::Building(index)) = (b.rotor, b.part) else {
            continue;
        };
        {
            let wheel = commands
                .spawn((
                    Name::new("Windmill wheel"),
                    rotor,
                    transform,
                    Visibility::default(),
                    TownBuilding(index),
                    ChildOf(root),
                ))
                .id();
            for (surface, mesh) in &shapes.rotor {
                commands.spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(surfaces.get(*surface)),
                    Transform::IDENTITY,
                    range.clone(),
                    TownBuilding(index),
                    ChildOf(wheel),
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::flora::ThinOccluder;
    use crate::tree::{STREET_SPECIES, Species};
    use pampa_core::{TownConfig, Vec2};

    fn two_towns() -> Vec<TownLayout> {
        let far = TownConfig {
            name: "Lejos".into(),
            origin: Vec2::new(3000.0, -2000.0),
            ..TownConfig::castex()
        };
        vec![
            TownLayout::castex_inspired(),
            TownLayout::generate(&far).unwrap(),
        ]
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .insert_resource(WorldTowns(two_towns()))
            .add_plugins(TownPlugin);
        app
    }

    fn roots(app: &mut App) -> Vec<usize> {
        let mut q = app.world_mut().query::<&TownRoot>();
        let mut v: Vec<usize> = q.iter(app.world()).map(|r| r.0).collect();
        v.sort_unstable();
        v
    }

    fn count<F: bevy::ecs::query::QueryFilter>(app: &mut App) -> usize {
        let mut q = app.world_mut().query_filtered::<(), F>();
        q.iter(app.world()).count()
    }

    /// Actualiza hasta que no quede ningún pueblo con edificios en
    /// construcción.
    fn settle(app: &mut App) {
        for _ in 0..2000 {
            if count::<With<PendingBuildings>>(app) == 0 {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
            app.update();
        }
        panic!("the town's buildings never landed");
    }

    #[test]
    fn only_towns_near_the_focus_are_built_and_far_ones_are_dropped() {
        let mut app = app();
        app.update();
        assert_eq!(roots(&mut app), vec![0]);
        // El primer pueblo se tiende de una vez, edificios incluidos.
        assert_eq!(count::<With<PendingBuildings>>(&mut app), 0);
        let castex = count::<With<Mesh3d>>(&mut app);
        assert!(castex > 100, "{castex} meshes");
        assert!(count::<With<TownBuilding>>(&mut app) > 100);

        // Se cabalga hasta el pueblo lejano: Castex se va, llega el pueblo
        // lejano, y sus edificios se construyen fuera del hilo principal y
        // aparecen un poco después.
        app.world_mut().resource_mut::<StreamFocus>().0 = Vec2::new(2950.0, -1950.0);
        app.update();
        assert_eq!(roots(&mut app), vec![1]);
        assert!(app.world().resource::<ActiveTowns>().0.is_active(1));
        settle(&mut app);
        // La misma disposición desplazada, con otras semillas por edificio: más
        // o menos la misma cantidad de mallas, ninguna sobrante de Castex.
        let far = count::<With<Mesh3d>>(&mut app);
        assert!(far.abs_diff(castex) < castex / 5, "{far} vs {castex}");
        assert!(count::<With<TownBuilding>>(&mut app) > 100);

        // A mitad de camino entre ellos: no se dibuja nada.
        app.world_mut().resource_mut::<StreamFocus>().0 = Vec2::new(1500.0, -1000.0);
        app.update();
        assert!(roots(&mut app).is_empty());
        assert_eq!(count::<With<Mesh3d>>(&mut app), 0);
    }

    #[test]
    fn a_town_dropped_while_building_lands_nothing() {
        let mut app = app();
        app.update();
        app.world_mut().resource_mut::<StreamFocus>().0 = Vec2::new(2950.0, -1950.0);
        app.update();
        assert_eq!(count::<With<PendingBuildings>>(&mut app), 1);
        app.world_mut().resource_mut::<StreamFocus>().0 = Vec2::new(1500.0, -1000.0);
        for _ in 0..50 {
            std::thread::sleep(std::time::Duration::from_millis(1));
            app.update();
        }
        assert_eq!(count::<With<Mesh3d>>(&mut app), 0);
    }

    #[test]
    fn each_building_is_one_mesh_per_surface_with_the_shared_materials() {
        let mut app = app();
        app.update();
        let town = app.world().resource::<WorldTowns>().0[0].clone();
        let facade = app.world().resource::<FacadeMaterials>().clone();
        let shared: HashSet<AssetId<StandardMaterial>> =
            Surface::ALL.iter().map(|&s| facade.get(s).id()).collect();
        let mut q = app
            .world_mut()
            .query::<(&TownBuilding, &MeshMaterial3d<StandardMaterial>, &ChildOf)>();
        let wheels: HashSet<Entity> = app
            .world_mut()
            .query_filtered::<Entity, With<WindmillRotor>>()
            .iter(app.world())
            .collect();
        let mut per: Vec<Vec<AssetId<StandardMaterial>>> = vec![Vec::new(); town.buildings.len()];
        for (b, m, parent) in q.iter(app.world()) {
            // La rueda de un molino de viento es su propia malla por
            // superficie.
            if !wheels.contains(&parent.parent()) {
                per[b.0].push(m.id());
            }
        }
        for (k, (building, materials)) in town.buildings.iter().zip(&per).enumerate() {
            if building.kind == BuildingKind::Platform {
                assert!(materials.is_empty());
                continue;
            }
            assert!(!materials.is_empty(), "building {k} ({:?})", building.kind);
            assert!(materials.iter().all(|m| shared.contains(m)));
            let distinct: HashSet<_> = materials.iter().collect();
            assert_eq!(
                distinct.len(),
                materials.len(),
                "building {k}: one mesh per surface"
            );
        }
    }

    #[test]
    fn each_plaza_is_one_mesh_per_surface_and_range_with_the_shared_materials() {
        let mut app = app();
        app.update();
        let facade = app.world().resource::<FacadeMaterials>().clone();
        let id = |s: Surface| facade.get(s).id();
        let mut q = app.world_mut().query::<(
            &TownPlaza,
            &MeshMaterial3d<StandardMaterial>,
            &VisibilityRange,
        )>();
        let ranges = ranges();
        let mut seen = HashSet::new();
        let (mut ground, mut furniture) = (HashSet::new(), HashSet::new());
        for (plaza, material, range) in q.iter(app.world()) {
            let r = ranges
                .iter()
                .position(|x| x == range)
                .expect("a shared range");
            assert!(
                seen.insert((plaza.0, r, material.id())),
                "one mesh per surface"
            );
            match r {
                0 => ground.insert(material.id()),
                1 => furniture.insert(material.id()),
                _ => panic!("plazas draw within the buildings' and props' ranges"),
            };
        }
        // Castex es un pueblo pequeño: senderos de tierra con cordones de
        // ladrillo y un monumento de revoque y encalado con placas de bronce
        // hasta donde llegan los edificios; bancos de hierro y madera y una
        // reja de hierro hasta donde llegan los objetos.
        let want = [
            Surface::Earth,
            Surface::Brick,
            Surface::Stucco,
            Surface::Whitewash,
            Surface::Bronze,
        ];
        assert!(want.iter().all(|&s| ground.contains(&id(s))), "{ground:?}");
        assert!(!ground.contains(&id(Surface::Paving)));
        let want = [Surface::Planks, Surface::Steel];
        assert_eq!(furniture, want.iter().map(|&s| id(s)).collect());
    }

    #[test]
    fn plaza_trees_share_the_kit_and_stay_solid() {
        let mut app = app();
        app.update();
        let town = app.world().resource::<WorldTowns>().0[0].clone();
        let kit = app.world().resource::<crate::tree::TreeKit>().clone();
        let species = plaza_species(town.seed, 0);
        let own = kit.species(species);
        let meshes: HashSet<AssetId<Mesh>> = own
            .variants
            .iter()
            .flat_map(|v| [v.bark.id(), v.leaves.id()])
            .collect();
        let mut q = app.world_mut().query_filtered::<(
            &Mesh3d,
            &MeshMaterial3d<StandardMaterial>,
            &VisibilityRange,
            &Transform,
            Has<ThinOccluder>,
        ), With<TownTree>>();
        let mut drawn = 0;
        let mut used = HashSet::new();
        for (mesh, material, range, at, fades) in q.iter(app.world()) {
            drawn += 1;
            // Castex tiene una plaza: las mallas y materiales compartidos de su
            // especie, hasta donde llegan los árboles.
            assert!(meshes.contains(&mesh.id()));
            assert!([own.bark.id(), own.leaves.id()].contains(&material.id()));
            assert!(range == &ranges()[2]);
            used.insert(mesh.id());
            // Sólido: un árbol nunca se desvanece para no estorbar a la cámara.
            assert!(!fades);
            assert!(at.scale.x <= TREE_SCALE_MAX && at.scale.x >= TREE_SCALE_MIN);
        }
        // Una malla de corteza y una de hojas por árbol, con las variantes
        // compartidas.
        assert_eq!(drawn, 2 * town.trees.len());
        assert!(town.trees.len() > 2 * VARIANTS);
        assert_eq!(
            used.len(),
            2 * VARIANTS,
            "every variant drawn, none rebuilt"
        );
    }

    #[test]
    fn a_town_with_a_station_plaza_plants_another_species_there() {
        // General Pico tal como lo dispone la vista previa: una ciudad con una
        // plaza de estación.
        let town = TownLayout::generate(&pampa_core::TownConfig {
            name: "General Pico".into(),
            origin: Vec2::ZERO,
            blocks: (8, 8),
            block_size: 40.0,
            railway: Some(pampa_core::RailwayConfig {
                side: pampa_core::Side::North,
                offset: 24.0,
                through: true,
            }),
            seed: 13,
            ..TownConfig::castex()
        })
        .unwrap();
        let sites = plaza_tree_sites(&town);
        let plazas: HashSet<usize> = sites.iter().map(|(p, _)| *p).collect();
        assert_eq!(plazas, HashSet::from([0, 1]), "trees in both plazas");
        for (plaza, site) in &sites {
            let area = town.plazas().nth(*plaza).unwrap();
            assert!(area.contains_point(site.at));
        }
        assert_ne!(plaza_species(town.seed, 0), plaza_species(town.seed, 1));
    }

    #[test]
    fn every_tree_stands_in_exactly_one_plaza_and_is_filed_under_it() {
        // Cada forma de pueblo que toman los pueblos del juego (de 3 a 8
        // manzanas por lado, con y sin ferrocarril, con una línea de paso que
        // forma una plaza de estación) con unas pocas semillas: core planta
        // cada árbol en una plaza, así que un cambio de disposición que deje
        // uno afuera falla aquí, no en silencio.
        let mut towns = 0;
        for blocks in [(3, 3), (4, 3), (4, 4), (4, 5), (5, 4), (8, 8)] {
            for seed in [1, 2, 13, 19] {
                for railway in [None, Some(false), Some(true)] {
                    let config = TownConfig {
                        blocks,
                        seed,
                        railway: railway.map(|through| pampa_core::RailwayConfig {
                            side: pampa_core::Side::North,
                            offset: 24.0,
                            through,
                        }),
                        ..TownConfig::castex()
                    };
                    let Ok(town) = TownLayout::generate(&config) else {
                        continue;
                    };
                    towns += 1;
                    let plazas: Vec<Aabb> = town.plazas().collect();
                    for (plaza, site) in plaza_tree_sites(&town) {
                        let holding: Vec<usize> = (0..plazas.len())
                            .filter(|&i| plazas[i].contains_point(site.at))
                            .collect();
                        assert_eq!(holding, [plaza], "{blocks:?} seed {seed} {:?}", site.at);
                    }
                }
            }
        }
        assert!(towns > 50, "{towns} towns generated");
    }

    #[test]
    fn a_tree_outside_every_plaza_goes_to_the_nearest() {
        let plazas = [
            Aabb::new(Vec2::new(0.0, 0.0), Vec2::new(40.0, 40.0)),
            Aabb::new(Vec2::new(100.0, 0.0), Vec2::new(140.0, 40.0)),
        ];
        assert_eq!(plaza_of(&plazas, Vec2::new(20.0, 20.0)), 0);
        assert_eq!(plaza_of(&plazas, Vec2::new(120.0, 40.0)), 1, "on the edge");
        assert_eq!(plaza_of(&plazas, Vec2::new(141.0, 20.0)), 1, "just outside");
        assert_eq!(plaza_of(&plazas, Vec2::new(60.0, -5.0)), 0);
    }

    #[test]
    fn towns_draw_within_the_shared_ranges_only() {
        let mut app = app();
        app.update();
        let mut q = app
            .world_mut()
            .query::<(&VisibilityRange, Has<TownBuilding>, Has<Mesh3d>)>();
        let mut seen = HashSet::new();
        let mut buildings = 0;
        for (range, building, mesh) in q.iter(app.world()) {
            assert!(ranges().contains(range), "a range of its own");
            seen.insert(range.clone());
            buildings += usize::from(building && mesh);
        }
        // Bevy nunca libera el lugar en la GPU de un rango: los pueblos no
        // agregan ninguno.
        assert!(seen.len() <= ranges().len());
        // Cada malla de edificio tiene rango, las casas en BUILDING_FAR.
        assert_eq!(
            buildings,
            count::<(With<TownBuilding>, With<Mesh3d>)>(&mut app)
        );
        assert!(seen.contains(&ranges()[0]));
    }

    /// Escribe tres pueblos tal como vienen en el juego (General Pico,
    /// Eduardo Castex, Metileo) en `PAMPA_PREVIEW` (una carpeta) para el
    /// renderer offline, en el formato de
    /// `facade::town_buildings::tests::preview_town_street`: edificios, las
    /// calles de tierra con las huellas de carro del patio de cargas, los
    /// palenques, las veredas y las plazas (césped, senderos, monumento,
    /// bancos, reja y árboles) sobre suelo plano. Se ejecuta con
    /// `PAMPA_PREVIEW=/some/dir cargo test -p pampa-render -- --ignored preview_town`.
    #[test]
    #[ignore = "writes preview files"]
    fn preview_town_sidewalks() {
        use std::fmt::Write as _;
        let Some(dir) = std::env::var_os("PAMPA_PREVIEW") else {
            return;
        };
        let dir = std::path::PathBuf::from(dir);
        let config = |name: &str, blocks, street_width, seed, through| pampa_core::TownConfig {
            name: name.into(),
            origin: Vec2::ZERO,
            blocks,
            block_size: 40.0,
            street_width,
            railway: Some(pampa_core::RailwayConfig {
                side: pampa_core::Side::North,
                offset: 24.0,
                through,
            }),
            plaza: pampa_core::PlazaPlacement::Auto,
            seed,
            police_stations: None,
            density: None,
            townsfolk: None,
        };
        let towns = [
            ("pico", config("General Pico", (8, 8), 13.0, 13, true)),
            ("castex", config("Eduardo Castex", (5, 4), 12.0, 2, false)),
            (
                "metileo",
                pampa_core::TownConfig {
                    density: Some(0.3),
                    ..config("Metileo", (3, 3), 10.0, 19, false)
                },
            ),
        ];
        for (name, config) in towns {
            let town = TownLayout::generate(&config).unwrap();
            let terrain = WorldTerrain::default().0;
            let terrain = terrain.clone().with_flat_area(town.flat_area(&terrain));
            let floor = terrain.height(0.0, 0.0);
            let mut text = String::new();
            let put_part = |text: &mut String, shape: &Shape, surface: Surface, at: Transform| {
                for (p, n, uv) in shape.vertices(surface) {
                    let p = at.transform_point(p) - Vec3::Y * floor;
                    let n = at.rotation * n;
                    let _ = writeln!(
                        text,
                        "v {:.3} {:.3} {:.3} {:.3} {:.3} {:.3} {:.4} {:.4}",
                        p.x, p.y, p.z, n.x, n.y, n.z, uv.x, uv.y
                    );
                }
                for t in shape.indices(surface).chunks(3) {
                    let _ = writeln!(text, "f {} {} {}", t[0], t[1], t[2]);
                }
            };
            let put = |text: &mut String, shape: &Shape, at: Transform| {
                for surface in shape.surfaces().collect::<Vec<_>>() {
                    let _ = writeln!(text, "m {surface:?}");
                    put_part(text, shape, surface, at);
                }
            };
            // Parches de suelo como quads planos con nombre.
            let patch = |text: &mut String, name: &str, a: &Aabb, lift: f32| {
                let (p, q) = (
                    world_from_ground(a.min, lift),
                    world_from_ground(a.max, lift),
                );
                let _ = writeln!(text, "m {name}");
                for (x, z) in [(p.x, p.z), (q.x, p.z), (q.x, q.z), (p.x, q.z)] {
                    let _ = writeln!(text, "v {x:.3} {lift:.3} {z:.3} 0 1 0 0 0");
                }
                let _ = writeln!(text, "f 0 2 1\nf 0 3 2");
            };
            // Las calles de tierra con las huellas de carro del patio de
            // cargas, y los palenques, tal como los construye el juego.
            let (origin, streets) = street_shape(&town, &terrain);
            put(&mut text, &streets, Transform::from_translation(origin));
            let (origin, rails) = palenque_shape(&town, &terrain);
            put(&mut text, &rails, Transform::from_translation(origin));
            for plaza in town.plazas() {
                patch(&mut text, "Plaza", &plaza, crate::facade::plaza::LAWN_TOP);
            }
            for job in plaza_jobs(&town, &terrain) {
                let shapes = crate::facade::plaza::plaza(&job.furniture);
                let at = Transform::from_translation(job.base);
                put(&mut text, &shapes.ground, at);
                put(&mut text, &shapes.furniture, at);
            }
            // Árboles tal como los dibuja el juego: una variante de la especie
            // de la plaza, girada y escalada.
            for (plaza, site) in plaza_tree_sites(&town) {
                let species = plaza_species(town.seed, plaza);
                let tree = crate::tree::mesh::tree(species, site.variant as u32);
                let at = Transform::from_translation(world_from_ground(
                    site.at,
                    terrain.height(site.at.x, site.at.y) - TREE_SINK,
                ))
                .with_rotation(Quat::from_rotation_y(site.yaw))
                .with_scale(Vec3::splat(site.scale));
                put_tree(&mut text, species, &tree, at, floor);
            }
            for (origin, shape) in sidewalk_shapes(town.sidewalks(), &terrain) {
                put(&mut text, &shape, Transform::from_translation(origin));
            }
            for job in lots(&town, &terrain) {
                let Some(shape) = town_buildings::building(&job.lot) else {
                    continue;
                };
                let at = Transform::from_translation(job.base)
                    .with_rotation(town_buildings::yaw(job.lot.building.front));
                put(&mut text, &shape, at);
            }
            std::fs::write(dir.join(format!("town_{name}.txt")), text).expect("preview written");
            // Dónde está la plaza, para encuadrar la cámara.
            let c = town.plaza.center();
            println!(
                "{name}: plaza at x {} z {}, grid {:?}",
                c.x,
                -c.y,
                town.grid()
            );
        }
    }

    /// Escribe `tree` ubicado en `at` en un `text` de vista previa: su corteza
    /// como `m Bark(<species>)`, sus hojas como `m Leaves(<species>)`, con
    /// `floor` levantado de las alturas.
    fn put_tree(
        text: &mut String,
        species: crate::tree::Species,
        tree: &crate::tree::mesh::TreeMesh,
        at: Transform,
        floor: f32,
    ) {
        use std::fmt::Write as _;
        for (part, b) in [("Bark", &tree.bark), ("Leaves", &tree.leaves)] {
            let _ = writeln!(text, "m {part}({species:?})");
            for ((p, n), uv) in b.positions.iter().zip(&b.normals).zip(&b.uvs) {
                let (p, n) = (at.transform_point(*p) - Vec3::Y * floor, at.rotation * *n);
                let _ = writeln!(
                    text,
                    "v {:.3} {:.3} {:.3} {:.3} {:.3} {:.3} {:.4} {:.4}",
                    p.x, p.y, p.z, n.x, n.y, n.z, uv[0], uv[1]
                );
            }
            for t in b.indices.chunks(3) {
                let _ = writeln!(text, "f {} {} {}", t[0], t[1], t[2]);
            }
        }
    }

    /// Escribe cada variante de las cuatro especies en una fila, separadas
    /// 14 m (paraíso, olmo, eucalipto y aromo de oeste a este, las variantes
    /// de norte a sur), en `PAMPA_PREVIEW/trees.txt` para el renderer
    /// offline. Se ejecuta con
    /// `PAMPA_PREVIEW=/some/dir cargo test -p pampa-render -- --ignored preview_tree_species`.
    #[test]
    #[ignore = "writes preview files"]
    fn preview_tree_species() {
        let Some(dir) = std::env::var_os("PAMPA_PREVIEW") else {
            return;
        };
        let mut text = String::new();
        for (i, &species) in crate::tree::Species::ALL.iter().enumerate() {
            for v in 0..VARIANTS {
                let tree = crate::tree::mesh::tree(species, v as u32);
                let at = Transform::from_xyz(i as f32 * 14.0, 0.0, v as f32 * 14.0);
                put_tree(&mut text, species, &tree, at, 0.0);
            }
        }
        let path = std::path::PathBuf::from(dir).join("trees.txt");
        std::fs::write(&path, text).expect("preview written");
        println!("wrote {}", path.display());
    }

    #[test]
    fn a_town_paves_its_plaza_in_brick_and_every_other_block_in_earth_with_curbs() {
        let mut app = app();
        app.update();
        let facade = app.world().resource::<FacadeMaterials>().clone();
        let mut q = app.world_mut().query_filtered::<
            (&MeshMaterial3d<StandardMaterial>, &VisibilityRange),
            With<TownSidewalk>,
        >();
        let (mut brick, mut curbs) = (0, 0);
        for (material, range) in q.iter(app.world()) {
            assert!(range == &ranges()[0], "a range of its own");
            if material.id() == facade.get(Surface::Paving).id() {
                brick += 1;
            } else {
                assert_eq!(material.id(), facade.get(Surface::Stucco).id());
                curbs += 1;
            }
        }
        // Castex tiene 20 manzanas: un anillo de ladrillo alrededor de la
        // manzana de la plaza, y una malla de cordón para él y para los anillos
        // de cada una de las otras 19 manzanas.
        assert_eq!(brick, 1, "brick sidewalk meshes");
        assert_eq!(curbs, 20, "curb meshes");
        // Las veredas de tierra dentro de esos 19 anillos: un único decal
        // difuminado de patio.
        let mut q = app.world_mut().query_filtered::<
            (&MeshMaterial3d<StandardMaterial>, &VisibilityRange),
            With<TownEarthWalk>,
        >();
        let earth: Vec<_> = q.iter(app.world()).collect();
        assert_eq!(earth.len(), 1, "earth walk meshes");
        assert_eq!(earth[0].0.id(), facade.get(Surface::Yard).id());
        assert!(earth[0].1 == &ranges()[0]);
    }

    #[test]
    fn the_three_tiers_pave_as_the_rule_says() {
        let terrain = |t: &TownLayout| {
            let ground = WorldTerrain::default().0;
            ground.clone().with_flat_area(t.flat_area(&ground))
        };
        let surfaces = |t: &TownLayout, walk: Walk| -> HashSet<Surface> {
            let walks: Vec<Sidewalk> = t
                .sidewalks()
                .iter()
                .filter(|w| w.walk == walk)
                .cloned()
                .collect();
            sidewalk_shapes(&walks, &terrain(t))
                .iter()
                .flat_map(|(_, shape)| shape.surfaces())
                .collect()
        };
        let drawn = towns_to_draw();
        let (village, town, city) = (&drawn[0].0, &drawn[1].0, &drawn[2].0);
        // Una ciudad: ladrillo y cordones, sin tierra. Un pueblo: ladrillo
        // alrededor de la plaza, tierra en el resto, cordones en ambos. Una
        // aldea: tierra y cordones, sin ladrillo.
        assert_eq!(
            surfaces(city, Walk::Brick),
            HashSet::from([Surface::Paving, Surface::Stucco])
        );
        assert!(surfaces(city, Walk::Earth).is_empty());
        assert_eq!(
            surfaces(town, Walk::Brick),
            HashSet::from([Surface::Paving, Surface::Stucco])
        );
        // Las veredas de tierra dibujan un cordón y nada de pavimento.
        assert_eq!(
            surfaces(town, Walk::Earth),
            HashSet::from([Surface::Stucco])
        );
        assert!(surfaces(village, Walk::Brick).is_empty());
        assert_eq!(
            surfaces(village, Walk::Earth),
            HashSet::from([Surface::Stucco])
        );
    }

    #[test]
    fn an_earth_walk_is_flush_opaque_along_its_middle_and_feathered_at_its_edges() {
        let (town, terrain) = &towns_to_draw()[1];
        let rings: Vec<Vec<Aabb>> = town
            .sidewalks()
            .iter()
            .filter(|w| w.walk == Walk::Earth)
            .collect::<Vec<_>>()
            .chunks(4)
            .map(|ring| ring.iter().map(|w| walk_floor(w)).collect())
            .collect();
        assert_eq!(rings.len(), 19);
        let areas: Vec<Aabb> = rings.iter().flatten().copied().collect();
        let ground = |p: GroundVec2| terrain.height(p.x, p.y);
        let mesh = crate::settlement::track::walks_mesh(&rings, &ground).expect("a walk");
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(points)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!()
        };
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(colors)) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!()
        };
        let (mut solid, mut edge) = (0, 0);
        for (p, c) in points.iter().zip(colors) {
            let g = GroundVec2::new(p[0], -p[2]);
            // Al ras: de 3 a 5 cm por encima del suelo (las calles están a
            // 3 cm, las veredas de ladrillo a 6 y los cordones a 9).
            let over = p[1] - terrain.height(g.x, g.y);
            assert!((0.03..=0.05).contains(&over), "{over} m over the ground");
            let near = areas
                .iter()
                .map(|a| a.closest_point(g).distance(g))
                .fold(f32::INFINITY, f32::min);
            // Nada visible más allá de un metro de difuminado (las propias
            // esquinas de la malla llegan a menos de 2 m, sin alfa); opaca
            // (0.9) solo sobre la vereda.
            assert!(near <= 2.0, "{near} m off the walk");
            assert!(
                c[3] < 1e-3 || near <= 1.0 + 1e-3,
                "{near} m off, alpha {}",
                c[3]
            );
            assert!(c[3] <= 0.9 + 1e-4);
            if c[3] > 0.89 {
                solid += 1;
                assert!(near < 1e-3, "opaque {near} m off the walk");
            }
            if c[3] < 0.05 {
                edge += 1;
            }
        }
        assert!(
            solid > 500 && edge > 500,
            "{solid} solid and {edge} feathered corners"
        );
        // Bordes con pasto: varios escalones de alfa entre el medio y el borde.
        let steps: HashSet<u32> = colors.iter().map(|c| (c[3] * 10.0) as u32).collect();
        assert!(steps.len() >= 4, "{steps:?}");
    }

    /// Cada punto de vereda de `town`, sobre el suelo que tiene debajo.
    fn sidewalk_heights(town: &TownLayout, terrain: &Terrain) -> Vec<(Surface, f32)> {
        sidewalk_shapes(town.sidewalks(), terrain)
            .into_iter()
            .flat_map(|(origin, shape)| {
                shape
                    .surfaces()
                    .flat_map(|s| {
                        shape.points(s).into_iter().map(move |p| {
                            let p = p + origin;
                            (s, p.y - terrain.height(p.x, -p.z))
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    #[test]
    fn sidewalks_stand_a_few_centimeters_proud_like_the_plaza_paths() {
        // La gente camina a la altura propia del suelo sobre las calles
        // (0.03 m), los senderos (0.07 m) y por lo tanto las veredas: su
        // pavimento y su cordón se mantienen dentro de esos pocos centímetros.
        let pico = TownLayout::generate(&pampa_core::TownConfig {
            name: "Pico".into(),
            blocks: (8, 8),
            ..TownConfig::castex()
        })
        .unwrap();
        // El juego nivela el suelo bajo un pueblo.
        let terrain = WorldTerrain::default().0;
        let terrain = terrain.clone().with_flat_area(pico.flat_area(&terrain));
        let heights = sidewalk_heights(&pico, &terrain);
        let top = |surface| {
            heights
                .iter()
                .filter(|(s, _)| *s == surface)
                .fold(f32::MIN, |m, (_, y)| m.max(*y))
        };
        assert!((top(Surface::Paving) - SIDEWALK_TOP).abs() < 1e-3);
        assert!((top(Surface::Stucco) - CURB_TOP).abs() < 1e-3);
        const { assert!(CURB_TOP <= 0.1, "a curb people step over unawares") };
        for (surface, y) in heights {
            assert!(
                (SIDEWALK_FOOT - 1e-3..=CURB_TOP + 1e-3).contains(&y),
                "{surface:?} at {y}"
            );
        }
        // Un anillo por manzana salvo las cuatro de la plaza, y el de la plaza
        // misma: una forma cada uno.
        assert_eq!(
            sidewalk_shapes(pico.sidewalks(), &terrain).len(),
            64 - 4 + 1
        );
    }

    /// Una aldea, un pueblo pequeño, una ciudad y una ciudad crecida a ambos
    /// lados de su línea, con el suelo nivelado bajo cada uno como lo hace el
    /// juego.
    fn towns_to_draw() -> Vec<(TownLayout, Terrain)> {
        let line = |through| {
            Some(pampa_core::RailwayConfig {
                side: Side::North,
                offset: 24.0,
                through,
            })
        };
        [
            ((3, 3), false),
            ((5, 4), false),
            ((8, 8), false),
            ((8, 8), true),
        ]
        .into_iter()
        .map(|(blocks, through)| {
            let town = TownLayout::generate(&pampa_core::TownConfig {
                name: "Pueblo".into(),
                blocks,
                railway: line(through),
                ..TownConfig::castex()
            })
            .unwrap();
            let terrain = WorldTerrain::default().0;
            let terrain = terrain.clone().with_flat_area(town.flat_area(&terrain));
            (town, terrain)
        })
        .collect()
    }

    /// Cada vértice de la `surface` de `shape`, como `(world position, normal,
    /// uv)` para una forma ubicada en `origin`.
    fn world_vertices(
        origin: Vec3,
        shape: &Shape,
        surface: Surface,
    ) -> Vec<(Vec3, Vec3, bevy::math::Vec2)> {
        shape
            .vertices(surface)
            .into_iter()
            .map(|(p, n, uv)| (p + origin, n, uv))
            .collect()
    }

    /// El punto del suelo bajo una posición del mundo.
    fn ground_of(p: Vec3) -> GroundVec2 {
        GroundVec2::new(p.x, -p.z)
    }

    #[test]
    fn a_town_draws_its_streets_as_one_earth_mesh_and_one_track_mesh() {
        let mut app = app();
        app.update();
        let facade = app.world().resource::<FacadeMaterials>().clone();
        let mut q = app.world_mut().query_filtered::<(
            &Mesh3d,
            &MeshMaterial3d<StandardMaterial>,
            &VisibilityRange,
            &ChildOf,
        ), With<TownStreet>>();
        let streets: Vec<_> = q.iter(app.world()).collect();
        let roots = streets
            .iter()
            .map(|(.., root)| root.parent())
            .collect::<HashSet<_>>()
            .len();
        assert_eq!(streets.len(), 2 * roots, "one mesh per surface and town");
        let meshes = app.world().resource::<Assets<Mesh>>();
        for surface in [Surface::Earth, Surface::Mud] {
            let wearing: Vec<_> = streets
                .iter()
                .filter(|(_, m, ..)| m.id() == facade.get(surface).id())
                .collect();
            assert_eq!(wearing.len(), roots, "{surface:?} once per town");
            for (mesh, _, range, _) in wearing {
                assert!(**range == ranges()[0], "a range of its own");
                let mesh = meshes.get(&mesh.0).unwrap();
                assert!(mesh.attribute(Mesh::ATTRIBUTE_UV_0).is_some());
                assert!(mesh.attribute(Mesh::ATTRIBUTE_TANGENT).is_some());
            }
        }
        // Ya no hay cubos de calle de color liso: cada pieza de calle tiene
        // textura.
        assert_eq!(
            streets
                .iter()
                .map(|(_, m, ..)| m.id())
                .collect::<HashSet<_>>()
                .len(),
            2
        );
    }

    #[test]
    fn street_strips_share_world_space_uvs_so_crossings_show_no_seam() {
        let tile = Surface::Earth.tile();
        for (town, terrain) in towns_to_draw() {
            let (origin, shape) = street_shape(&town, &terrain);
            let mut seen: std::collections::HashMap<(i32, i32), bevy::math::Vec2> =
                Default::default();
            let mut shared = 0;
            for (p, n, uv) in world_vertices(origin, &shape, Surface::Earth) {
                // Las caras superiores quedan planas sobre el suelo: la UV
                // planar del mundo.
                if n.y < 0.9 {
                    continue;
                }
                let want = bevy::math::Vec2::new((p.x - origin.x) / tile, (p.z - origin.z) / tile);
                assert!(uv.distance(want) < 1e-3, "{}: uv {uv:?} at {p}", town.name);
                // Sea cual sea la franja que dibujó un punto, recibió la misma
                // UV.
                let key = ((p.x * 100.0).round() as i32, (p.z * 100.0).round() as i32);
                if let Some(other) = seen.insert(key, uv) {
                    assert!(other.distance(uv) < 1e-3, "{}: a seam at {p}", town.name);
                    shared += 1;
                }
            }
            assert!(
                shared > 0,
                "{}: the strips overlap at the crossings",
                town.name
            );
        }
    }

    #[test]
    fn the_dirt_covers_every_street_at_the_old_height() {
        for (town, terrain) in towns_to_draw() {
            let (origin, shape) = street_shape(&town, &terrain);
            let tops: Vec<(Vec3, Vec3, bevy::math::Vec2)> =
                world_vertices(origin, &shape, Surface::Earth)
                    .into_iter()
                    .filter(|(_, n, _)| n.y > 0.9)
                    .collect();
            for (p, _, _) in &tops {
                let g = ground_of(*p);
                assert!(
                    ((p.y - terrain.height(g.x, g.y)) - 0.03).abs() < 1e-3,
                    "{}: top at {p}",
                    town.name
                );
            }
            // Las cuatro esquinas de cada franja son esquinas de la tierra.
            for street in &town.streets {
                for corner in [
                    street.min,
                    street.max,
                    GroundVec2::new(street.min.x, street.max.y),
                    GroundVec2::new(street.max.x, street.min.y),
                ] {
                    assert!(
                        tops.iter()
                            .any(|(p, _, _)| ground_of(*p).distance(corner) < 1e-3),
                        "{}: {street:?} is not covered",
                        town.name
                    );
                }
            }
        }
    }

    /// Los edificios de cargas de `town`, distinguidos de los galpones entre
    /// las casas por lo que construye el patio de la estación: depósitos de
    /// cereal y su galpón largo de 25 m.
    fn goods(town: &TownLayout) -> Vec<&Building> {
        town.buildings
            .iter()
            .filter(|b| {
                b.kind == BuildingKind::GrainStore
                    || (b.kind == BuildingKind::Shed
                        && b.footprint.size().x.max(b.footprint.size().y) >= 20.0)
            })
            .collect()
    }

    /// El edificio (de cualquier tipo) del que sale una pieza de `track`: aquel
    /// cuya huella queda más cerca de ella.
    fn owner<'t>(town: &'t TownLayout, track: &Aabb) -> &'t Building {
        let gap = |b: &Building| {
            let p = b.footprint.closest_point(track.center());
            let q = track.closest_point(p);
            p.distance(q)
        };
        town.buildings
            .iter()
            .min_by(|a, b| gap(a).total_cmp(&gap(b)))
            .unwrap()
    }

    #[test]
    fn only_the_railways_goods_yard_has_cart_tracks_in_town() {
        let mut yards = 0;
        for (town, _) in towns_to_draw() {
            let tracks = goods_tracks(&town);
            assert!(
                !tracks.is_empty(),
                "{}: the goods yard has tracks",
                town.name
            );
            for track in &tracks {
                let b = owner(&town, track);
                assert!(
                    goods(&town).contains(&b),
                    "{}: a track out of a {:?}",
                    town.name,
                    b.kind
                );
            }
            // Los galpones entre las casas, la estación, las casas, los
            // comercios y las plazas no tienen ninguna.
            for b in town.buildings.iter().filter(|b| !goods(&town).contains(b)) {
                assert!(
                    tracks.iter().all(|t| b
                        .footprint
                        .closest_point(t.center())
                        .distance(t.center())
                        > 1.0),
                    "{}: a track at a {:?}",
                    town.name,
                    b.kind
                );
            }
            // Cada depósito de cereal y el galpón del patio sacan una huella.
            for b in town
                .buildings
                .iter()
                .filter(|b| b.kind == BuildingKind::GrainStore)
            {
                assert!(
                    tracks.iter().any(|t| b
                        .footprint
                        .closest_point(t.center())
                        .distance(t.center())
                        < 14.0),
                    "{}: a grain store without a track",
                    town.name
                );
            }
            let sheds = goods(&town)
                .iter()
                .filter(|b| b.kind == BuildingKind::Shed)
                .count();
            assert_eq!(sheds, 1, "{}: one goods shed in the yard", town.name);
            // Cada edificio de cargas saca dos surcos separados por una trocha.
            for b in goods(&town) {
                let n = side_vector(b.front);
                let across = GroundVec2::new(-n.y, n.x);
                let half = b.footprint.size() * 0.5;
                let door = b.footprint.center() + n * (n.x.abs() * half.x + n.y.abs() * half.y);
                let mine: Vec<f32> = tracks
                    .iter()
                    .filter(|t| t.center().distance(door) < GOODS_REACH + 1.0)
                    .map(|t| (t.center() - door).dot(across))
                    .collect();
                assert!(!mine.is_empty(), "{}: {:?} has no track", town.name, b.kind);
                for lane in [-1.0, 1.0] {
                    let want = lane * 0.7;
                    assert!(
                        mine.iter().any(|o| (o - want).abs() < 1e-3),
                        "{}",
                        town.name
                    );
                }
                yards += 1;
            }
            for street in &town.streets {
                // Las calles comunes son de tierra, nada más: ningún surco
                // queda sobre una.
                assert!(
                    tracks.iter().all(|t| !t.intersects(street)),
                    "{}",
                    town.name
                );
            }
        }
        assert!(yards >= 8);
    }

    #[test]
    fn goods_tracks_run_to_the_street_or_a_short_apron_and_keep_clear() {
        let mut to_street = 0;
        let mut aprons = 0;
        for (town, _) in towns_to_draw() {
            let obstacles = town.obstacles();
            for track in goods_tracks(&town) {
                // Corta, a lo largo del frente, sin tocar nada.
                let size = track.size();
                assert!(
                    size.x.max(size.y) <= GOODS_REACH + RUT_WIDTH,
                    "{}",
                    town.name
                );
                assert!(size.x.min(size.y) <= RUT_WIDTH + 1e-4, "{}", town.name);
                for o in obstacles
                    .iter()
                    .filter(|o| !goods(&town).iter().any(|b| b.collider() == **o))
                {
                    assert!(!o.intersects(&track), "{}: a track on {o:?}", town.name);
                }
                for w in town.sidewalks() {
                    assert!(!w.area.intersects(&track), "{}", town.name);
                }
                if let Some(corridor) = town.rail_corridor() {
                    assert!(
                        !corridor.intersects(&track),
                        "{}: a track on the line",
                        town.name
                    );
                }
                // Termina en una calle cuando hay una cerca de su extremo; si
                // no, es una explanada.
                let touches = town
                    .streets
                    .iter()
                    .any(|s| s.expanded(RUT_GAP).intersects(&track));
                if touches {
                    to_street += 1;
                } else {
                    aprons += 1;
                }
            }
        }
        assert!(to_street > 0, "a goods building by a street");
        assert!(aprons > 0, "a goods building with an apron");
    }

    #[test]
    fn a_goods_door_on_or_next_to_a_street_gets_no_track_and_others_stop_short() {
        // Una calle a lo largo de x desde y = 0 hasta 10; las puertas miran al
        // norte (+y), hacia ella.
        let street = [Aabb::new(
            GroundVec2::new(-50.0, 0.0),
            GroundVec2::new(50.0, 10.0),
        )];
        let north = GroundVec2::new(0.0, 1.0);
        let at = |y: f32| GroundVec2::new(3.0, y);
        // Sobre el borde de la calle, o dentro de ella: la calle es la
        // explanada.
        assert_eq!(goods_track_length(&street, at(0.0), north), None);
        assert_eq!(goods_track_length(&street, at(5.0), north), None);
        // Demasiado cerca para un trecho que valga la pena dibujar (0.3 m de
        // inicio más 1 m).
        assert_eq!(goods_track_length(&street, at(-1.0), north), None);
        // Una huella llega hasta un cuarto de metro antes de la calle.
        let far = goods_track_length(&street, at(-4.0), north).unwrap();
        assert!((far - 3.75).abs() < 1e-4, "{far}");
        // Fuera del alcance de la calle: la explanada de 8 m.
        let apron = goods_track_length(&street, at(-30.0), north).unwrap();
        assert!((apron - 8.0).abs() < 1e-4, "{apron}");
        // Mirando en sentido contrario a la calle: también la explanada.
        let away = goods_track_length(&street, at(-4.0), GroundVec2::new(0.0, -1.0)).unwrap();
        assert!((away - 8.0).abs() < 1e-4, "{away}");
    }

    /// Pueblos con otros anchos de calle (10, 11, 13 y 16 m), uno pequeño y
    /// ciudades.
    fn varied_streets() -> Vec<(TownLayout, Terrain)> {
        [
            ((4, 4), 10.0),
            ((6, 6), 11.0),
            ((8, 8), 16.0),
            ((5, 5), 13.0),
        ]
        .into_iter()
        .map(|(blocks, street_width)| {
            let town = TownLayout::generate(&pampa_core::TownConfig {
                name: "Calles".into(),
                blocks,
                street_width,
                seed: 5,
                ..TownConfig::castex()
            })
            .unwrap();
            let terrain = WorldTerrain::default().0;
            let terrain = terrain.clone().with_flat_area(town.flat_area(&terrain));
            (town, terrain)
        })
        .collect()
    }

    #[test]
    fn a_town_draws_its_shade_trees_from_the_shared_kit_two_entities_each() {
        let mut app = app();
        app.update();
        let town = app.world().resource::<WorldTowns>().0[0].clone();
        let kit = app.world().resource::<crate::tree::TreeKit>().clone();
        // Castex es un pueblo pequeño: árboles podados frente a las casas,
        // libres en los patios, todos de las mallas compartidas del kit.
        let plazas: Vec<Aabb> = town.plazas().collect();
        let mut expected: HashSet<AssetId<Mesh>> = HashSet::new();
        let mut trimmed = 0;
        for tree in &town.shade_trees {
            let look = shade_look(town.seed, &plazas, tree);
            let own = kit.species(look.species);
            let variants = if look.trimmed {
                &own.pruned
            } else {
                &own.variants
            };
            let v = &variants[look.variant];
            expected.extend([v.bark.id(), v.leaves.id()]);
            trimmed += usize::from(look.trimmed);
        }
        assert!(trimmed > 0 && trimmed < town.shade_trees.len());
        let mut q = app
            .world_mut()
            .query_filtered::<(&Mesh3d, &VisibilityRange, Has<ThinOccluder>), With<TownShadeTree>>(
            );
        let mut drawn = 0;
        for (mesh, range, fades) in q.iter(app.world()) {
            drawn += 1;
            assert!(expected.contains(&mesh.id()));
            assert!(range == &ranges()[2]);
            assert!(!fades, "a shade tree fades");
        }
        assert_eq!(drawn, 2 * town.shade_trees.len());
        assert!(town.shade_trees.len() > 5);
        // Ninguna malla nueva: las del kit, tantas como construyó.
        let meshes = app.world().resource::<Assets<Mesh>>().len();
        let kit_meshes = 2 * VARIANTS * (Species::ALL.len() + STREET_SPECIES.len());
        assert!(meshes > kit_meshes);
    }

    #[test]
    fn a_town_draws_its_yard_plants_one_entity_each_from_the_shared_kit() {
        let mut app = app();
        app.update();
        let town = app.world().resource::<WorldTowns>().0[0].clone();
        let kit = app
            .world()
            .resource::<crate::tree::TreeKit>()
            .plants
            .clone();
        let mut q = app.world_mut().query_filtered::<(
            &Mesh3d,
            &MeshMaterial3d<StandardMaterial>,
            &VisibilityRange,
        ), With<TownYardPlant>>();
        let mut drawn = 0;
        for (mesh, material, range) in q.iter(app.world()) {
            drawn += 1;
            assert!(
                kit.shrubs
                    .iter()
                    .chain(&kit.flowers)
                    .any(|h| h.id() == mesh.id())
            );
            assert_eq!(material.id(), kit.material.id());
            assert!(range == &ranges()[1]);
        }
        assert_eq!(drawn, town.yard_plants.len());
        assert!(drawn > 10, "{drawn}");
        // Arbustos y canteros: más de una malla en uso.
        let used: HashSet<AssetId<Mesh>> = q.iter(app.world()).map(|(m, _, _)| m.id()).collect();
        assert!(used.len() >= 3, "{} meshes", used.len());
    }

    #[test]
    fn a_tree_with_no_street_direction_or_no_crown_still_gets_a_finite_transform() {
        let look = ShadeLook {
            species: Species::Olmo,
            trimmed: true,
            variant: 0,
            yaw: 0.7,
            jitter: 0.5,
        };
        let tree = ShadeTree {
            at: Vec2::new(3.0, 4.0),
            kind: ShadeKind::Pit,
            crown: 2.3,
            toward: Vec2::ZERO,
            row: 0,
        };
        for crown_radius in [0.0, 2.0] {
            let at = shade_transform(&tree, &look, crown_radius, 0.0);
            assert!(at.translation.is_finite() && at.scale.is_finite());
            assert!(at.rotation.is_finite(), "{crown_radius}: {:?}", at.rotation);
            // No hay dirección hacia la cual inclinarse: se mantiene derecho.
            assert!((at.rotation * Vec3::Y).angle_between(Vec3::Y) < 1e-3);
        }
        let plain = ShadeLook {
            trimmed: false,
            ..look
        };
        assert!(shade_transform(&tree, &plain, 0.0, 0.0).scale.is_finite());
    }

    #[test]
    fn trimmed_street_trees_keep_their_crown_over_1_9_m_and_off_the_facades() {
        let mut checked = 0;
        for (town, _) in towns_to_draw().into_iter().chain(varied_streets()) {
            let walls: Vec<Aabb> = town
                .buildings
                .iter()
                .filter(|b| b.kind != BuildingKind::Platform)
                .map(|b| b.footprint)
                .collect();
            let plazas: Vec<Aabb> = town.plazas().collect();
            for tree in town
                .shade_trees
                .iter()
                .filter(|t| t.kind != ShadeKind::Grove)
            {
                let look = shade_look(town.seed, &plazas, tree);
                let mesh = if look.trimmed {
                    crate::tree::mesh::street_tree(look.species, look.variant as u32)
                } else {
                    crate::tree::mesh::tree(look.species, look.variant as u32)
                };
                let at = shade_transform(tree, &look, mesh.crown_radius, 0.0);
                // Nunca más ancho que el espacio que le dejó core.
                assert!(
                    at.scale.x * mesh.crown_radius <= tree.crown + 1e-3,
                    "{:?}",
                    tree.kind
                );
                // Un árbol podado se mantiene casi derecho (usuario,
                // 2026-10-26): como mucho seis grados fuera de la vertical, e
                // inclinado hacia la calle.
                let up = at.rotation * Vec3::Y;
                let off = up.angle_between(Vec3::Y).to_degrees();
                if look.trimmed {
                    assert!(
                        off > 1.0 && off <= 6.0,
                        "{:?}: leans {off} degrees",
                        tree.kind
                    );
                    let toward = Vec3::new(tree.toward.x, 0.0, -tree.toward.y);
                    assert!(up.dot(toward) > 0.0, "leans away from the street");
                } else {
                    assert!(off < 0.01);
                }
                for leaf in &mesh.leaves.positions {
                    let p = at.transform_point(*leaf);
                    let ground = GroundVec2::new(p.x, -p.z);
                    if look.trimmed {
                        // Tronco desnudo hasta unos 2.5 m: ninguna hoja por
                        // debajo de 1.9 m.
                        assert!(p.y >= 1.9, "{:?}: a leaf at {:.2} m", look.species, p.y);
                    }
                    for wall in &walls {
                        let gap = wall.closest_point(ground).distance(ground);
                        assert!(
                            gap >= 0.5,
                            "{} {:?}: a leaf {gap:.2} m off a wall",
                            town.name,
                            tree.kind
                        );
                    }
                }
                checked += 1;
            }
        }
        assert!(checked > 500, "{checked}");
    }

    #[test]
    fn street_trees_are_trimmed_species_in_rows_and_yards_take_any() {
        let pico = &towns_to_draw()[2].0;
        let plazas: Vec<Aabb> = pico.plazas().collect();
        let mut rows: std::collections::HashMap<u32, HashSet<Species>> = Default::default();
        let mut yards = HashSet::new();
        for tree in &pico.shade_trees {
            let look = shade_look(pico.seed, &plazas, tree);
            match tree.kind {
                ShadeKind::Pit => {
                    assert!(look.trimmed);
                    // La especie Aromo se queda baja: nunca a lo largo de una
                    // calle.
                    assert_ne!(look.species, Species::Aromo);
                    rows.entry(tree.row).or_default().insert(look.species);
                }
                ShadeKind::Yard => {
                    assert!(!look.trimmed);
                    yards.insert(look.species);
                }
                ShadeKind::Grove => assert!(!look.trimmed),
                ShadeKind::Front => unreachable!("a city has no front trees"),
            }
        }
        // Una especie por hilera, especies distintas a lo largo del pueblo.
        assert!(rows.values().all(|s| s.len() == 1));
        let across: HashSet<Species> = rows.values().flatten().copied().collect();
        assert_eq!(across.len(), 3, "{across:?}");
        assert_eq!(yards.len(), 4, "{yards:?}");
    }

    #[test]
    fn a_cazuela_is_a_curb_high_rim_round_earth_on_the_walk() {
        let (city, terrain) = &towns_to_draw()[2];
        let (origin, shape) = tree_pit_shape(city, terrain).expect("a city has pits");
        let pits = city
            .shade_trees
            .iter()
            .filter(|t| t.kind == ShadeKind::Pit)
            .count();
        let rim = world_vertices(origin, &shape, Surface::Stucco);
        let soil = world_vertices(origin, &shape, Surface::Earth);
        // Un bloque de tierra (20 vértices, sin cara inferior) y cuatro losas
        // de borde por cada cazuela.
        assert_eq!(soil.len(), 20 * pits);
        assert_eq!(rim.len(), 4 * soil.len());
        let top = |v: &[(Vec3, Vec3, bevy::math::Vec2)]| {
            v.iter()
                .filter(|(_, n, _)| n.y > 0.99)
                .map(|(p, _, _)| p.y - terrain.height(p.x, -p.z))
                .fold(f32::NEG_INFINITY, f32::max)
        };
        // El borde tan alto como el cordón (9 cm) y la tierra apenas por encima
        // del pavimento (6 cm).
        assert!((top(&rim) - 0.09).abs() < 2e-3, "{}", top(&rim));
        assert!((top(&soil) - 0.065).abs() < 2e-3, "{}", top(&soil));
        // Cada vértice queda sobre una vereda, a 0.4 m o más hacia adentro del
        // borde del cordón.
        for (p, _, _) in &rim {
            let g = GroundVec2::new(p.x, -p.z);
            assert!(
                city.sidewalks()
                    .iter()
                    .any(|w| w.area.expanded(1e-3).contains_point(g)),
                "{g:?} off the walk"
            );
        }
        assert!(pits > 100);
        // Un pueblo pequeño no tiene ninguna.
        let (small, terrain) = &towns_to_draw()[1];
        assert!(tree_pit_shape(small, terrain).is_none());
    }

    #[test]
    fn cart_tracks_lie_a_few_millimeters_over_the_dirt_level() {
        for (town, terrain) in towns_to_draw() {
            let (origin, shape) = street_shape(&town, &terrain);
            let verts = world_vertices(origin, &shape, Surface::Mud);
            assert!(!verts.is_empty(), "{}", town.name);
            for (p, n, _) in &verts {
                assert!(n.y > 0.99, "{}: a rut that leans", town.name);
                let g = ground_of(*p);
                let over = p.y - terrain.height(g.x, g.y);
                // 5 mm o más por encima de la tierra de la calle (sin competir
                // con ella), y por debajo del pavimento de las veredas.
                assert!(over >= 0.03 + 0.005, "{}: {over} m", town.name);
                assert!(over < SIDEWALK_TOP, "{}: {over} m", town.name);
                // Y nunca sobre la tierra de una calle: las calles no tienen
                // surcos.
                for street in &town.streets {
                    assert!(
                        !street.expanded(-0.01).contains_point(g),
                        "{}: a rut at {g:?} on a street",
                        town.name
                    );
                }
            }
            // No hay dos surcos que se superpongan.
            let tracks = goods_tracks(&town);
            for (i, a) in tracks.iter().enumerate() {
                assert!(
                    tracks[i + 1..].iter().all(|b| !a.intersects(b)),
                    "{}",
                    town.name
                );
            }
        }
    }

    #[test]
    fn a_town_draws_all_its_rails_as_one_timber_mesh_in_the_small_props_range() {
        let mut app = app();
        app.update();
        let facade = app.world().resource::<FacadeMaterials>().clone();
        let mut q = app.world_mut().query_filtered::<(
            &MeshMaterial3d<StandardMaterial>,
            &VisibilityRange,
            &ChildOf,
        ), With<TownPalenque>>();
        let rails: Vec<_> = q.iter(app.world()).collect();
        let roots = rails
            .iter()
            .map(|(.., root)| root.parent())
            .collect::<HashSet<_>>()
            .len();
        assert_eq!(rails.len(), roots, "one mesh per town");
        assert!(roots >= 1);
        for (material, range, _) in rails {
            assert_eq!(material.id(), facade.get(Surface::Planks).id());
            assert!(*range == ranges()[1], "the small props' range");
        }
    }

    #[test]
    fn every_rail_of_a_town_gets_a_palenque_along_its_footprint() {
        for (town, terrain) in towns_to_draw() {
            let (origin, shape) = palenque_shape(&town, &terrain);
            let rails: Vec<Aabb> = std::iter::once(town.hitching_post.rail)
                .chain(town.palenques.iter().map(|p| p.rail))
                .collect();
            assert!(
                rails.len() > 1,
                "{}: palenques besides the store's",
                town.name
            );
            let verts: Vec<Vec3> = world_vertices(origin, &shape, Surface::Planks)
                .into_iter()
                .map(|(p, _, _)| p)
                .collect();
            let mut claimed = 0;
            for rail in &rails {
                let mine: Vec<&Vec3> = verts
                    .iter()
                    .filter(|p| rail.expanded(0.2).contains_point(ground_of(**p)))
                    .collect();
                assert!(!mine.is_empty(), "{}: nothing at {rail:?}", town.name);
                claimed += mine.len();
                // De punta a punta a lo largo del lado largo de la barra, con
                // el ancho de un poste a lo ancho.
                let (lo, hi) = mine.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| {
                    let g = ground_of(**p);
                    let t = if rail.size().x >= rail.size().y {
                        g.x
                    } else {
                        g.y
                    };
                    (lo.min(t), hi.max(t))
                });
                let long = rail.size().x.max(rail.size().y);
                assert!((hi - lo - long).abs() < 1e-2, "{}: {}", town.name, hi - lo);
                // Los postes se apoyan sobre el suelo allí y la barra va a su
                // altura.
                let c = rail.center();
                let ground = terrain.height(c.x, c.y);
                let top = mine.iter().fold(f32::MIN, |m, p| m.max(p.y)) - ground;
                assert!((top - 1.15).abs() < 1e-3, "{}", town.name);
                assert!(mine.iter().any(|p| ((p.y - ground) - 1.0).abs() < 0.06));
            }
            // No se dibuja nada más, y se mantiene liviano.
            assert_eq!(claimed, verts.len(), "{}: stray timber", town.name);
            assert!(shape.triangles() < 120 * rails.len(), "{}", town.name);
        }
    }
}
