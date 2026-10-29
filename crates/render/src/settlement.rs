//! Asentamientos rurales (ranchos, puestos, chacras y estancias), cargados por
//! streaming junto con los chunks de terreno, como los alambrados y los
//! caminos.
//!
//! `pampa-core` los dispone ([`SettlementLayout`]). Cuando se construye un
//! chunk de terreno, cada asentamiento cuyo centro cae en él se dibuja como
//! hijo del chunk y se va con él. Un chunk que llega por streaming mientras se
//! cabalga los construye en el pool de cómputo asíncrono, como su suelo, y
//! aparecen unos pocos frames después:
//!
//! - cada casa, galpón y objeto, construidos de forma procedural en su propio
//!   espacio (ver [`buildings`] y [`props`]) como una malla por cada
//!   [`Surface`] que llevan, todas compartiendo un material PBR por superficie
//!   ([`FacadeMaterials`]), construidos con el kit de fachadas compartido con
//!   los pueblos ([`crate::facade`]);
//! - la rueda del molino de viento aparte, girando como un [`WindmillRotor`]
//!   (que hace girar el [`crate::TownPlugin`]);
//! - la isleta de eucaliptos y la hilera de álamos: árboles texturizados del
//!   [`crate::tree::TreeKit`] compartido (el eucalipto de los pueblos y el
//!   álamo del campo, la columna de un álamo de Lombardía), escalados a la
//!   altura de cada lugar;
//! - las huellas y los patios pisoteados, como decals sobre el suelo dibujado
//!   ([`track`]);
//! - los carros abandonados, en algunos patios y en unas pocas banquinas;
//! - los objetos de patio de Poly Haven alrededor de los galpones y los pozos
//!   ([`yard`]);
//! - arbustos de patio y canteros de flores junto a las casas
//!   ([`yard::yard_plants`]), mallas compartidas del kit de árboles
//!   ([`crate::tree::plants`]).
//!
//! Un tipo de estructura con una ranura de modelo dibuja un modelo glTF de
//! terceros en lugar de su malla procedural cuando
//! `assets/models/third-party/<slot>/` contiene uno; la malla procedural lo
//! sustituye mientras carga y se queda si falla ([`slots`]).
//!
//! Los lados de los corrales, los molinos de viento, los pozos, los carros y
//! las parvas se desvanecen cuando estorban a la cámara ([`ThinOccluder`]); las
//! casas y los galpones, en cambio, detienen la cámara (los oclusores de cámara
//! del juego), y lo mismo hacen los árboles, sólidos alrededor de sus troncos.
//! Todo se dibuja dentro de unos pocos [`VisibilityRange`] compartidos: Bevy
//! conserva para siempre un lugar en la GPU por cada rango distinto (ver
//! [`crate::flora::TUFT_FADE_STEPS`]).

pub mod buildings;
pub mod props;
pub mod slots;
pub mod track;
pub mod yard;

/// El constructor de mallas y los materiales del kit de fachadas compartido,
/// con los que se construyeron originalmente los asentamientos.
pub use crate::facade::{materials, shape};

use bevy::camera::visibility::VisibilityRange;
use std::sync::Arc;

use bevy::ecs::system::EntityCommands;
use bevy::prelude::*;
use bevy::tasks::futures::check_ready;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use bevy::transform::TransformSystems;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot};
use pampa_core::settlement::RoadsideWreck;
use pampa_core::town::PlantKind;
use pampa_core::{
    Aabb, ChunkCoord, Settlement, SettlementLayout, Side, Structure, StructureKind, Terrain, Track,
    Vec2 as GroundVec2,
};

pub use materials::{FacadeMaterials, Livery, Surface};
pub use slots::{ModelRotor, ModelSlots, SlotFit, SlotFolder, SlotModels, SlotRotor, StandIn};
pub use yard::{YardModels, YardProp};

use crate::coords::world_from_ground;
use crate::flora::{TREE_FADE, TREE_FAR, ThinOccluder, visible_within};
use crate::gltf::{self, ModelState};
use crate::terrain::{
    ChunkDetail, ChunkStreaming, PendingGround, TerrainChunk, WorldChunks, WorldTerrain,
    mesh_height,
};
use crate::town::WindmillRotor;
use crate::tree::Species as TreeSpecies;
use shape::Shape;
use slots::SceneDress;

/// Los asentamientos del mundo. Se insertan antes de que corra el plugin; sin
/// ellos no se dibuja ninguno.
#[derive(Resource, Debug, Clone, Default)]
pub struct WorldSettlements(pub SettlementLayout);

/// Una pieza dibujada de la estructura de un asentamiento, hija de su chunk:
/// qué asentamiento (un índice en [`SettlementLayout::settlements`]), cuál de
/// sus estructuras y qué es.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettlementPart {
    pub settlement: u32,
    pub structure: u32,
    pub kind: StructureKind,
}

impl SettlementPart {
    /// El `settlement` de un carro abandonado junto al camino, cuyo `structure`
    /// es un índice en [`SettlementLayout::roadside`].
    pub const ROADSIDE: u32 = u32::MAX;
}

/// La escena de un objeto de patio, hija de su chunk: qué asentamiento y qué
/// objeto.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct YardPart {
    pub settlement: u32,
    pub prop: YardProp,
}

/// Un arbusto de patio o cantero de flores, hijo de su chunk: su asentamiento.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct YardPlantPart(pub u32);

/// Las huellas de un asentamiento, una malla de decal, hija de su chunk.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackSurface(pub u32);

/// Los pisos del patio y del corral de un asentamiento, una malla de decal.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct YardSurface(pub u32);

#[derive(Component)]
struct SettlementsLaid;

/// Dónde dejan de dibujarse las casas, galpones, tanques y parvas, en metros:
/// bastante más allá de la niebla de polvo, cerca del borde del streaming.
pub const BUILDING_FAR: f32 = 400.0;
const BUILDING_FADE: f32 = 25.0;
/// Dónde dejan de dibujarse los corrales, los molinos de viento y los objetos
/// pequeños.
pub const PROP_FAR: f32 = 220.0;
const PROP_FADE: f32 = 20.0;

/// Los pocos rangos compartidos dentro de los cuales se dibujan los
/// asentamientos: edificios, objetos y el de los árboles (el mismo que el de
/// los árboles silvestres).
pub fn ranges() -> [VisibilityRange; 3] {
    [
        visible_within(BUILDING_FAR, BUILDING_FADE),
        visible_within(PROP_FAR, PROP_FADE),
        visible_within(TREE_FAR, TREE_FADE),
    ]
}

/// Las mallas y materiales compartidos por todos los asentamientos, además de
/// los materiales de superficie: la rueda del molino de viento y el kit de los
/// árboles.
#[derive(Resource, Debug, Clone)]
struct SettlementKit {
    rotor: Vec<(Surface, Handle<Mesh>)>,
    /// El eucalipto del kit de árboles, y luego su álamo.
    trees: [crate::tree::SpeciesKit; 2],
    /// Los arbustos de patio y canteros de flores, los de los pueblos
    /// ([`crate::tree`]).
    plants: crate::tree::plants::PlantKit,
}

/// Dibuja los [`WorldSettlements`] chunk por chunk. Necesita el
/// [`crate::TerrainPlugin`]; las ruedas de los molinos de viento giran con el
/// [`crate::TownPlugin`].
pub struct SettlementPlugin;

impl Plugin for SettlementPlugin {
    fn build(&self, app: &mut App) {
        crate::facade::ensure_plugin(app);
        crate::tree::ensure_plugin(app);
        app.init_resource::<WorldSettlements>()
            .init_resource::<WorldTerrain>()
            .init_resource::<WorldChunks>()
            .init_resource::<ModelSlots>()
            .init_resource::<SlotFolder>()
            .add_systems(
                Startup,
                (
                    setup_kit.after(crate::tree::setup_trees),
                    slots::load_slot_models,
                    yard::load_yard_models,
                ),
            )
            .add_systems(Update, (slots::swap_in_models, slots::spin_model_rotors))
            .add_observer(slots::dress_scene)
            .add_systems(
                PostUpdate,
                (lay_settlements, land_settlements)
                    .chain()
                    .after(ChunkStreaming)
                    .before(TransformSystems::Propagate),
            );
    }
}

fn setup_kit(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    tree_kit: Option<Res<crate::tree::TreeKit>>,
) {
    // El plugin lo ordena después de `setup_trees`, así que si falta el kit es
    // que el plugin de árboles no está: se avisa en lugar de no dibujar ningún
    // asentamiento.
    let Some(tree_kit) = tree_kit else {
        warn!("tree kit missing; settlement kit not built, settlements will not be drawn");
        return;
    };
    let rotor = props::rotor()
        .into_meshes()
        .into_iter()
        .map(|(surface, mesh)| (surface, meshes.add(mesh)))
        .collect();
    commands.insert_resource(SettlementKit {
        rotor,
        trees: [
            tree_kit.species(TreeSpecies::Eucalipto).clone(),
            tree_kit.species(TreeSpecies::Alamo).clone(),
        ],
        plants: tree_kit.plants.clone(),
    });
}

/// Qué es un hijo de chunk.
#[derive(Clone, Copy, Debug)]
enum Tag {
    Part(SettlementPart),
    Tracks(u32),
    Yards(u32),
    Prop(YardPart),
    Plant(u32),
}

/// Lo que dibuja una pieza: una malla compartida, una malla procedural recién
/// construida (que se agrega a las mallas cuando se crea), o la escena de un
/// modelo glTF (que se crea ahora cuando está `ready`, o si no una vez que su
/// modelo se ha cargado).
enum Look {
    Mesh(Handle<Mesh>, Handle<StandardMaterial>),
    Built(Mesh, Handle<StandardMaterial>),
    Scene {
        scene: Handle<WorldAsset>,
        ready: bool,
        dress: SceneDress,
    },
}

/// Una entidad a crear bajo un chunk.
struct Piece {
    look: Look,
    transform: Transform,
    range: Option<VisibilityRange>,
    fade: Option<ThinOccluder>,
    rotor: Option<WindmillRotor>,
    tag: Tag,
    /// Se dibuja solo hasta que el modelo de la ranura de su estructura esté
    /// listo.
    stand_in: bool,
}

/// Si se usa el modelo de ranura de un tipo de estructura, y cuánto se ha
/// cargado.
#[derive(Clone, Debug)]
enum SlotUse {
    /// Sin modelo, o falló: solo la malla procedural.
    Procedural,
    /// Cargando: la malla procedural lo sustituye hasta que esté listo.
    Loading(Handle<WorldAsset>),
    Ready(Handle<WorldAsset>),
}

/// Lo que necesita el dibujo de un asentamiento.
struct Painter<'a> {
    terrain: &'a Terrain,
    /// La altura del suelo dibujado (la de la malla del chunk, no la del
    /// terreno lógico, más fino), para que nada flote ni se hunda.
    ground: &'a dyn Fn(GroundVec2) -> f32,
    surfaces: &'a FacadeMaterials,
    kit: &'a SettlementKit,
    server: Option<&'a AssetServer>,
    slots: Option<&'a SlotModels>,
    fits: &'a ModelSlots,
    yard: Option<&'a YardModels>,
}

/// El giro que lleva el `+Z` propio de una estructura hacia su `front`.
fn yaw(front: Side) -> Quat {
    use std::f32::consts::{FRAC_PI_2, PI};
    Quat::from_rotation_y(match front {
        Side::South => 0.0,
        Side::North => PI,
        Side::East => FRAC_PI_2,
        Side::West => -FRAC_PI_2,
    })
}

impl Painter<'_> {
    /// Si se usa el modelo de ranura de `kind`, y cuánto se ha cargado.
    fn slot_use(&self, kind: StructureKind) -> SlotUse {
        let (Some(slot), Some(models), Some(server)) = (kind.slot(), self.slots, self.server)
        else {
            return SlotUse::Procedural;
        };
        let Some(scene) = models.0.get(slot) else {
            return SlotUse::Procedural;
        };
        match gltf::model_state(server, scene) {
            ModelState::Ready => SlotUse::Ready(scene.clone()),
            ModelState::Loading => SlotUse::Loading(scene.clone()),
            ModelState::Failed => SlotUse::Procedural,
        }
    }

    /// La pieza de escena del modelo de ranura de `kind` en `transform` (el
    /// espacio propio de la estructura), cuando tiene uno.
    fn slot_scene(
        &self,
        kind: StructureKind,
        transform: Transform,
        range: VisibilityRange,
        fade: Option<ThinOccluder>,
        speed: f32,
        tag: Tag,
    ) -> Option<Piece> {
        let (scene, ready) = match self.slot_use(kind) {
            SlotUse::Procedural => return None,
            SlotUse::Loading(scene) => (scene, false),
            SlotUse::Ready(scene) => (scene, true),
        };
        let fit = self.fits.fit(kind.slot()?);
        Some(Piece {
            look: Look::Scene {
                scene,
                ready,
                dress: SceneDress {
                    range,
                    fade,
                    rotor: fit.rotor.clone().map(|r| (r, speed)),
                    glass: false,
                },
            },
            transform: transform * fit.transform(),
            range: None,
            fade: None,
            rotor: None,
            tag,
            stand_in: false,
        })
    }

    /// Dónde se ubica `structure`: su medio sobre el suelo, girada hacia su
    /// frente; y cuánto queda su esquina más baja por debajo de eso, más un
    /// margen, hasta donde deben bajar sus paredes y cimientos.
    fn place(&self, structure: &Structure) -> (Transform, f32) {
        let fp = structure.footprint;
        let base = (self.ground)(fp.center());
        let low = [
            fp.min,
            fp.max,
            GroundVec2::new(fp.min.x, fp.max.y),
            GroundVec2::new(fp.max.x, fp.min.y),
        ]
        .into_iter()
        .map(|p| (self.ground)(p))
        .fold(base, f32::min);
        let transform = Transform::from_translation(world_from_ground(fp.center(), base))
            .with_rotation(yaw(structure.front));
        (transform, base - low + 0.3)
    }

    /// Todas las piezas del asentamiento `index`.
    fn settlement(&self, index: u32, settlement: &Settlement) -> Vec<Piece> {
        let mut pieces = Vec::new();
        for (k, structure) in settlement.structures.iter().enumerate() {
            let tag = SettlementPart {
                settlement: index,
                structure: k as u32,
                kind: structure.kind,
            };
            self.structure(structure, tag, &mut pieces);
        }
        let tracks: Vec<&Track> = settlement.tracks.iter().collect();
        if let Some(mesh) = track::tracks_mesh(&tracks, &self.ground) {
            pieces.push(self.decal(mesh, Surface::Mud, Tag::Tracks(index)));
        }
        let yards: Vec<Aabb> = std::iter::once(settlement.patio)
            .chain(settlement.of(StructureKind::Corral).map(|c| c.footprint))
            .collect();
        if let Some(mesh) = track::yards_mesh(&yards, &self.ground) {
            pieces.push(self.decal(mesh, Surface::Yard, Tag::Yards(index)));
        }
        for spot in yard::yard_plants(settlement) {
            let plants = &self.kit.plants;
            let meshes = match spot.kind {
                PlantKind::Shrub => &plants.shrubs,
                PlantKind::Flowers => &plants.flowers,
            };
            let pick = (spot.at.x.abs() * 7.0 + spot.at.y.abs() * 13.0) as usize % meshes.len();
            pieces.push(Piece {
                look: Look::Mesh(meshes[pick].clone(), plants.material.clone()),
                transform: Transform::from_translation(world_from_ground(
                    spot.at,
                    (self.ground)(spot.at),
                ))
                .with_rotation(spot.turn),
                range: Some(ranges()[1].clone()),
                fade: None,
                rotor: None,
                tag: Tag::Plant(index),
                stand_in: false,
            });
        }
        if let Some(models) = self.yard {
            for spot in yard::yard_props(settlement) {
                let Some(scene) = models.0.get(&spot.prop) else {
                    continue;
                };
                let base = (self.ground)(spot.at) + spot.lift;
                pieces.push(Piece {
                    look: Look::Scene {
                        scene: scene.clone(),
                        ready: true,
                        dress: SceneDress {
                            range: ranges()[1].clone(),
                            fade: None,
                            rotor: None,
                            glass: spot.prop == YardProp::Lantern,
                        },
                    },
                    transform: Transform::from_translation(world_from_ground(spot.at, base))
                        .with_rotation(spot.turn),
                    range: None,
                    fade: None,
                    rotor: None,
                    tag: Tag::Prop(YardPart {
                        settlement: index,
                        prop: spot.prop,
                    }),
                    stand_in: false,
                });
            }
        }
        pieces
    }

    /// El carro abandonado junto al camino `index`: tendido a lo largo del
    /// camino, algo inclinado.
    fn roadside(&self, index: u32, wreck: &RoadsideWreck) -> Vec<Piece> {
        let (wide, deep) = Settlement::WRECK_SIZE;
        let structure = Structure {
            kind: StructureKind::WreckedWagon,
            footprint: Aabb::from_center_size(wreck.at, GroundVec2::new(wide, deep)),
            height: 2.2,
            front: Side::North,
            variant: wreck.variant,
        };
        let tag = SettlementPart {
            settlement: SettlementPart::ROADSIDE,
            structure: index,
            kind: StructureKind::WreckedWagon,
        };
        // Su propio `x` a lo largo del camino.
        let heading = Quat::from_rotation_y(wreck.along.y.atan2(wreck.along.x));
        let mut pieces = Vec::new();
        self.structure_turned(&structure, tag, Some(heading), &mut pieces);
        pieces
    }

    /// Un decal en el espacio del mundo: sin rango, como los caminos.
    fn decal(&self, mesh: Mesh, surface: Surface, tag: Tag) -> Piece {
        Piece {
            look: Look::Built(mesh, self.surfaces.get(surface)),
            transform: Transform::default(),
            range: None,
            fade: None,
            rotor: None,
            tag,
            stand_in: false,
        }
    }

    fn structure(&self, structure: &Structure, tag: SettlementPart, out: &mut Vec<Piece>) {
        self.structure_turned(structure, tag, None, out);
    }

    /// Todas las piezas de `structure`, giradas según `heading` en lugar de
    /// hacia su frente cuando se indica (un carro abandonado a lo largo de su
    /// camino). Un tipo cuyo modelo de ranura está listo dibuja solo ese
    /// modelo; mientras carga, lo sustituyen las piezas procedurales.
    fn structure_turned(
        &self,
        structure: &Structure,
        tag: SettlementPart,
        heading: Option<Quat>,
        out: &mut Vec<Piece>,
    ) {
        if structure.kind.is_tree() {
            self.tree(structure, tag, out);
            return;
        }
        let (mut transform, drop) = self.place(structure);
        if let Some(heading) = heading {
            transform.rotation = heading;
        }
        if structure.kind == StructureKind::WreckedWagon {
            // Asentado torcido donde se rompió.
            let v = structure.variant;
            let (roll, pitch) = ((v % 7) as f32 - 3.0, (v / 7 % 5) as f32 - 2.0);
            transform.rotation *= Quat::from_rotation_x(roll * 0.012)
                * Quat::from_rotation_z(pitch * 0.015)
                * Quat::from_rotation_y((v % 2) as f32 * std::f32::consts::PI);
        }
        let at = structure.footprint.center();
        let size = structure.footprint.size();
        let reach = size.x.max(size.y) / 2.0;
        let slot = self.slot_use(structure.kind);
        if matches!(slot, SlotUse::Ready(_)) {
            let (range, fade, speed) = self.slot_look(structure, at, reach);
            out.extend(self.slot_scene(
                structure.kind,
                transform,
                range,
                fade,
                speed,
                Tag::Part(tag),
            ));
            return;
        }
        if matches!(slot, SlotUse::Loading(_)) {
            let (range, fade, speed) = self.slot_look(structure, at, reach);
            out.extend(self.slot_scene(
                structure.kind,
                transform,
                range,
                fade,
                speed,
                Tag::Part(tag),
            ));
        }
        let stand_in = matches!(slot, SlotUse::Loading(_));
        let mut add = |shape: Shape,
                       transform: Transform,
                       range: usize,
                       fade: Option<ThinOccluder>,
                       rotor: Option<WindmillRotor>| {
            for (surface, mesh) in shape.into_meshes() {
                out.push(Piece {
                    look: Look::Built(mesh, self.surfaces.get(surface)),
                    transform,
                    range: Some(ranges()[range].clone()),
                    fade,
                    rotor,
                    tag: Tag::Part(tag),
                    stand_in,
                });
            }
        };
        match structure.kind {
            StructureKind::MainHouse(_) | StructureKind::WorkerHouse(_) | StructureKind::Barn => {
                add(
                    buildings::building(structure, drop),
                    transform,
                    0,
                    None,
                    None,
                );
            }
            StructureKind::Corral => {
                let ground = |p: Vec3| {
                    let w = transform.transform_point(p.with_y(0.0));
                    (self.ground)(GroundVec2::new(w.x, -w.z)) - transform.translation.y
                };
                for side in props::corral(structure, &ground) {
                    let (a, b) = (
                        transform.transform_point(side.ends.0),
                        transform.transform_point(side.ends.1),
                    );
                    let (a, b) = (GroundVec2::new(a.x, -a.z), GroundVec2::new(b.x, -b.z));
                    let fade = ThinOccluder {
                        a,
                        b,
                        radius: 0.5,
                        bottom: transform.translation.y - drop - 0.5,
                        top: transform.translation.y + structure.height + 0.6,
                    };
                    add(side.shape, transform, 1, Some(fade), None);
                }
            }
            StructureKind::Windpump => {
                let fade = ThinOccluder::post(
                    at,
                    props::WHEEL_RADIUS,
                    self.terrain,
                    structure.height + 2.5,
                );
                add(props::prop(structure, drop), transform, 1, Some(fade), None);
                let (hub, turn) = props::rotor_place(structure);
                let wheel = transform * Transform::from_translation(hub).with_rotation(turn);
                let speed = rotor_speed(structure);
                for (surface, mesh) in &self.kit.rotor {
                    out.push(Piece {
                        look: Look::Mesh(mesh.clone(), self.surfaces.get(*surface)),
                        transform: wheel,
                        range: Some(ranges()[1].clone()),
                        fade: Some(fade),
                        rotor: Some(WindmillRotor { speed }),
                        tag: Tag::Part(tag),
                        stand_in,
                    });
                }
            }
            StructureKind::Tank => {
                add(props::prop(structure, drop), transform, 0, None, None);
            }
            StructureKind::Haystack => {
                let fade = ThinOccluder::post(at, reach * 0.8, self.terrain, structure.height);
                add(props::prop(structure, drop), transform, 0, Some(fade), None);
            }
            StructureKind::Trough => {
                add(props::prop(structure, drop), transform, 1, None, None);
            }
            StructureKind::WreckedWagon => {
                // El bastidor de la carreta lo sustituye, hundido hasta los
                // ejes.
                let fade = ThinOccluder::post(at, reach, self.terrain, structure.height + 1.0);
                let sunk = transform * Transform::from_xyz(0.0, -0.25, 0.0);
                add(props::prop(structure, drop), sunk, 1, Some(fade), None);
            }
            _ => {
                // El pozo, el palenque y el carro.
                let fade = ThinOccluder::post(at, reach, self.terrain, structure.height + 1.0);
                add(props::prop(structure, drop), transform, 1, Some(fade), None);
            }
        }
    }

    /// El rango compartido, la forma de fundido y la velocidad del rotor con
    /// los que se dibuja el modelo de ranura de `structure`: los mismos que los
    /// de sus piezas procedurales.
    fn slot_look(
        &self,
        structure: &Structure,
        at: GroundVec2,
        reach: f32,
    ) -> (VisibilityRange, Option<ThinOccluder>, f32) {
        let terrain = self.terrain;
        let h = structure.height;
        match structure.kind {
            StructureKind::Windpump => (
                ranges()[1].clone(),
                Some(ThinOccluder::post(
                    at,
                    props::WHEEL_RADIUS,
                    terrain,
                    h + 2.5,
                )),
                rotor_speed(structure),
            ),
            StructureKind::Haystack => (
                ranges()[0].clone(),
                Some(ThinOccluder::post(at, reach * 0.8, terrain, h)),
                0.0,
            ),
            StructureKind::Trough => (ranges()[1].clone(), None, 0.0),
            _ => (
                ranges()[1].clone(),
                Some(ThinOccluder::post(at, reach, terrain, h + 1.0)),
                0.0,
            ),
        }
    }

    /// Un árbol plantado en un lugar para árbol: un árbol del kit de árboles
    /// compartido (el eucalipto, o el álamo para una hilera de álamos), elegido
    /// según la variante del lugar y escalado de forma uniforme para que tenga
    /// la altura del lugar.
    fn tree(&self, structure: &Structure, tag: SettlementPart, out: &mut Vec<Piece>) {
        let at = structure.footprint.center();
        let h = structure.height;
        let poplar = structure.kind == StructureKind::Poplar;
        let species = &self.kit.trees[poplar as usize];
        let species_kind = if poplar {
            TreeSpecies::Alamo
        } else {
            TreeSpecies::Eucalipto
        };
        let variant = &species.variants[(structure.variant / 7) as usize % species.variants.len()];
        let turned = Transform::from_translation(world_from_ground(at, (self.ground)(at) - 0.1))
            .with_rotation(Quat::from_rotation_y(
                (structure.variant % 360) as f32 / 57.3,
            ));
        let range = visible_within(TREE_FAR, TREE_FADE);
        // El modelo de una ranura de árbol se ajusta por metro de altura del
        // lugar.
        let slot = self.slot_use(structure.kind);
        if !matches!(slot, SlotUse::Procedural) {
            let per_meter = Transform::from_scale(Vec3::splat(h));
            out.extend(self.slot_scene(
                structure.kind,
                turned * per_meter,
                range.clone(),
                None,
                0.0,
                Tag::Part(tag),
            ));
            if matches!(slot, SlotUse::Ready(_)) {
                return;
            }
        }
        let stand_in = matches!(slot, SlotUse::Loading(_));
        // Llevado a la altura del lugar; un tronco que saldría más grueso que
        // el collider que core planta para él (el de un eucalipto, de más de
        // 18 m) se adelgaza, con copa y todo, para que un jinete nunca quede
        // dentro de la corteza.
        let k = h / variant.height;
        let girth = k.min(structure.trunk_radius() / species_kind.form().trunk_radius);
        let scaled = turned.with_scale(Vec3::new(girth, k, girth));
        for (mesh, paint) in [
            (&variant.bark, &species.bark),
            (&variant.leaves, &species.leaves),
        ] {
            out.push(Piece {
                look: Look::Mesh(mesh.clone(), paint.clone()),
                transform: scaled,
                range: Some(range.clone()),
                fade: None,
                rotor: None,
                tag: Tag::Part(tag),
                stand_in,
            });
        }
    }
}

/// Si el chunk `coord` es dueño de `settlement`: su centro cae en él (bordes
/// oeste y sur incluidos), según cuenta [`SettlementLayout::in_area`].
fn owns(coord: ChunkCoord, size: f32, settlement: &Settlement) -> bool {
    ChunkCoord::containing(settlement.hub, size) == coord
}

/// Lo que tiende el chunk `coord`: los asentamientos de los que es dueño y los
/// carros abandonados junto al camino que caen en él, cada uno con su índice en
/// la disposición.
#[derive(Debug, Clone, Default)]
struct ChunkShare {
    settlements: Vec<(u32, Settlement)>,
    wrecks: Vec<(u32, RoadsideWreck)>,
}

impl ChunkShare {
    fn of(layout: &SettlementLayout, coord: ChunkCoord, size: f32) -> Self {
        Self {
            settlements: layout
                .settlements
                .iter()
                .enumerate()
                .filter(|(_, s)| owns(coord, size, s))
                .map(|(k, s)| (k as u32, s.clone()))
                .collect(),
            wrecks: layout
                .roadside
                .iter()
                .enumerate()
                .filter(|(_, w)| ChunkCoord::containing(w.at, size) == coord)
                .map(|(k, w)| (k as u32, *w))
                .collect(),
        }
    }

    fn is_empty(&self) -> bool {
        self.settlements.is_empty() && self.wrecks.is_empty()
    }
}

/// Todas las piezas del `share` de un chunk.
fn chunk_pieces(painter: &Painter, share: &ChunkShare) -> Vec<Piece> {
    let mut pieces: Vec<Piece> = share
        .settlements
        .iter()
        .flat_map(|(k, s)| painter.settlement(*k, s))
        .collect();
    for (k, wreck) in &share.wrecks {
        pieces.extend(painter.roadside(*k, wreck));
    }
    pieces
}

/// Qué tan rápido gira la rueda de un molino de viento, en radianes por
/// segundo: un poco distinto en cada uno.
fn rotor_speed(structure: &Structure) -> f32 {
    1.4 + (structure.variant % 89) as f32 / 89.0
}

/// Lo que lee el tendido de los asentamientos.
type LayInputs<'a> = (
    Res<'a, WorldSettlements>,
    Res<'a, WorldTerrain>,
    Res<'a, WorldChunks>,
    Option<Res<'a, ChunkDetail>>,
);

/// Los modelos que puede dibujar el tendido de los asentamientos.
type LayModels<'a> = (
    Option<Res<'a, AssetServer>>,
    Option<Res<'a, SlotModels>>,
    Res<'a, ModelSlots>,
    Option<Res<'a, YardModels>>,
);

/// Todo lo que toma prestado un [`Painter`], en propiedad, para que los
/// asentamientos de un chunk se puedan construir en el pool de cómputo
/// asíncrono. Barato de clonar: handles, y un terreno que comparte sus caminos.
#[derive(Clone)]
struct PaintKit {
    terrain: Terrain,
    size: f32,
    resolution: u32,
    surfaces: FacadeMaterials,
    kit: SettlementKit,
    server: Option<AssetServer>,
    slots: Option<SlotModels>,
    fits: ModelSlots,
    yard: Option<YardModels>,
}

impl PaintKit {
    /// Las piezas del `share` de un chunk.
    fn pieces(&self, share: &ChunkShare) -> Vec<Piece> {
        let (terrain, size, resolution) = (&self.terrain, self.size, self.resolution);
        let ground = |p: GroundVec2| mesh_height(terrain, p, size, resolution);
        let painter = Painter {
            terrain,
            ground: &ground,
            surfaces: &self.surfaces,
            kit: &self.kit,
            server: self.server.as_ref(),
            slots: self.slots.as_ref(),
            fits: &self.fits,
            yard: self.yard.as_ref(),
        };
        chunk_pieces(&painter, share)
    }
}

/// Los asentamientos de un chunk cargado por streaming, mientras se construyen
/// en el pool de cómputo asíncrono. [`land_settlements`] los crea cuando están
/// listos; un chunk eliminado antes lo descarta, lo que cancela la
/// construcción.
#[derive(Component)]
struct PendingSettlements(Task<Vec<Piece>>);

#[cfg(test)]
impl PendingSettlements {
    fn is_finished(&self) -> bool {
        self.0.is_finished()
    }
}

/// Dibuja los asentamientos de cada chunk recién construido como hijos suyos.
/// Los chunks del primer llenado se tienden ahora, para que el primer frame los
/// muestre; un chunk que llega por streaming mientras se cabalga (con su suelo
/// todavía en construcción) los construye en el pool de cómputo asíncrono, así
/// que tender una estancia nunca traba un frame. Está en el borde del radio de
/// carga, más allá del rango de los edificios, así que sus asentamientos, que
/// aparecen unos pocos frames después, nunca se ven. Un chunk que desaparece
/// antes de que se apliquen los comandos simplemente no recibe ninguno.
#[allow(clippy::too_many_arguments)]
fn lay_settlements(
    mut commands: Commands,
    (layout, terrain, world, detail): LayInputs,
    (surfaces, kit): (Option<Res<FacadeMaterials>>, Option<Res<SettlementKit>>),
    (server, slots, fits, yard): LayModels,
    chunks: Query<(Entity, &TerrainChunk, Has<PendingGround>), Without<SettlementsLaid>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut paint: Local<Option<Arc<PaintKit>>>,
) {
    let (Some(surfaces), Some(kit), Some(detail)) = (surfaces, kit, detail) else {
        return;
    };
    let size = world.0.chunk_size;
    // Se construye de nuevo en cada frame en que hay trabajo: los modelos se
    // cargan con el tiempo.
    *paint = None;
    for (chunk, coord, streamed) in &chunks {
        let mut laid = commands.entity(chunk);
        laid.try_insert(SettlementsLaid);
        let share = ChunkShare::of(&layout.0, coord.0, size);
        if share.is_empty() {
            continue;
        }
        let paint = paint
            .get_or_insert_with(|| {
                Arc::new(PaintKit {
                    terrain: terrain.0.clone(),
                    size,
                    resolution: detail.resolution,
                    surfaces: surfaces.clone(),
                    kit: kit.clone(),
                    server: server.as_deref().cloned(),
                    slots: slots.as_deref().cloned(),
                    fits: fits.clone(),
                    yard: yard.as_deref().cloned(),
                })
            })
            .clone();
        if streamed {
            let task = AsyncComputeTaskPool::get().spawn(async move { paint.pieces(&share) });
            laid.try_insert(PendingSettlements(task));
        } else {
            let pieces = paint.pieces(&share);
            spawn_pieces(&mut laid, pieces, &mut meshes);
        }
    }
}

/// Crea los asentamientos de cada chunk cuya construcción terminó.
fn land_settlements(
    mut commands: Commands,
    mut pending: Query<(Entity, &mut PendingSettlements)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for (chunk, mut build) in &mut pending {
        if let Some(pieces) = check_ready(&mut build.0) {
            let mut chunk = commands.entity(chunk);
            chunk.remove::<PendingSettlements>();
            spawn_pieces(&mut chunk, pieces, &mut meshes);
        }
    }
}

/// Encola `pieces` para crearlas como hijas de `chunk`, agregando ahora sus
/// mallas recién construidas.
fn spawn_pieces(chunk: &mut EntityCommands, pieces: Vec<Piece>, meshes: &mut Assets<Mesh>) {
    if pieces.is_empty() {
        return;
    }
    let pieces: Vec<Piece> = pieces
        .into_iter()
        .map(|p| match p.look {
            Look::Built(mesh, material) => Piece {
                look: Look::Mesh(meshes.add(mesh), material),
                ..p
            },
            _ => p,
        })
        .collect();
    chunk.queue_silenced(move |mut chunk: EntityWorldMut| {
        chunk.with_children(|children| {
            for p in pieces {
                let mut part = children.spawn(p.transform);
                match p.look {
                    Look::Mesh(mesh, material) => {
                        part.insert((Mesh3d(mesh), MeshMaterial3d(material)));
                    }
                    Look::Built(..) => unreachable!("added above"),
                    Look::Scene {
                        scene,
                        ready,
                        dress,
                    } => {
                        part.insert((dress, Visibility::default()));
                        if ready {
                            part.insert(WorldAssetRoot(scene));
                        } else {
                            part.insert(slots::SlotPending(scene));
                        }
                    }
                }
                if p.stand_in {
                    part.insert(StandIn);
                }
                if let Some(range) = p.range {
                    part.insert(range);
                }
                if let Some(fade) = p.fade {
                    part.insert(fade);
                }
                if let Some(rotor) = p.rotor {
                    part.insert(rotor);
                }
                match p.tag {
                    Tag::Part(tag) => part.insert(tag),
                    Tag::Tracks(k) => part.insert(TrackSurface(k)),
                    Tag::Yards(k) => part.insert(YardSurface(k)),
                    Tag::Prop(prop) => part.insert(prop),
                    Tag::Plant(k) => part.insert(YardPlantPart(k)),
                };
            }
        });
    });
}

/// Triángulos que dibuja `structure`, sin contar la rueda ni los árboles: lo
/// que cuentan los tests de presupuesto y el log de inicio.
pub fn structure_triangles(structure: &Structure) -> usize {
    match structure.kind {
        StructureKind::MainHouse(_) | StructureKind::WorkerHouse(_) | StructureKind::Barn => {
            buildings::building(structure, 0.3).triangles()
        }
        StructureKind::Corral => props::corral(structure, &|_| 0.0)
            .iter()
            .map(|s| s.shape.triangles())
            .sum(),
        StructureKind::Windpump => {
            props::prop(structure, 0.3).triangles() + props::rotor().triangles()
        }
        k if k.is_tree() => 0,
        _ => props::prop(structure, 0.3).triangles(),
    }
}

#[cfg(test)]
mod tests;
