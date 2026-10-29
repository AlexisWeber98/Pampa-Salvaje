//! Faroles de kerosén: un poste de hierro fundido con un farol de vidrio
//! arriba, en los lugares donde los pone `pampa-core` ([`StreetLamps`]).
//!
//! - **Dibujo**: dos mallas compartidas, el hierro (poste, barra de escalera,
//!   asiento del farol, techo y remate) y el vidrio, con dos materiales
//!   compartidos, así que todos los faroles del mundo son dos draws agrupados.
//!   Los faroles de un pueblo cuelgan de su [`TownRoot`] y se cargan por
//!   streaming con él; los de una estancia, del chunk de terreno dueño de su
//!   centro, dentro de los [`VisibilityRange`] compartidos de los asentamientos
//!   (ningún rango nuevo: Bevy nunca libera uno). El poste se desvanece cuando
//!   estorba a la cámara, como un poste de alambrado ([`ThinOccluder`]).
//! - **Brillo**: el emisivo del vidrio sigue el reloj del juego: apagado de
//!   día, se enciende de a poco al anochecer y se apaga al amanecer
//!   ([`lamp_glow`]). Es un único material compartido, así que un solo cambio
//!   de asset enciende todos los faroles.
//! - **Luz**: ningún farol tiene luz propia. Un conjunto de [`LAMP_LIGHTS`]
//!   luces puntuales sin sombra se mueve en cada frame a los faroles más
//!   cercanos a la cámara mientras brillan; de día el conjunto se oculta, así
//!   que no se dibuja ninguna luz.

use bevy::camera::visibility::VisibilityRange;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use pampa_core::{ChunkCoord, StreetLamp, StreetLamps, Terrain, Vec2 as GroundVec2};

use crate::atmosphere::{WorldClock, sky_at};
use crate::character::viewer::world_layers;
use crate::coords::world_from_ground;
use crate::flora::ThinOccluder;
use crate::settlement::shape::Shape;
use crate::settlement::{Surface, WorldSettlements, ranges};
use crate::terrain::{
    ChunkDetail, ChunkStreaming, StreamFocus, TerrainChunk, WorldChunks, WorldTerrain, mesh_height,
};
use crate::town::TownRoot;

/// Todos los faroles del mundo. Se insertan antes de que corra el plugin; sin
/// ellos no se dibuja ninguno.
#[derive(Resource, Debug, Clone, Default)]
pub struct WorldLamps(pub StreetLamps);

/// El poste de un farol (la malla de hierro).
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct LampPost;

/// El vidrio de un farol, con el lugar del espacio del mundo donde arde su
/// llama: adonde va una luz del conjunto cuando este farol está entre los más
/// cercanos.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct LampGlass {
    pub flame: Vec3,
}

/// Una luz del conjunto que ilumina los faroles más cercanos.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct LampLight;

/// Cuántas luces puntuales reales comparten los faroles: esta cantidad de los
/// más cercanos a la cámara se iluminan de noche; el resto solo brilla.
pub const LAMP_LIGHTS: usize = 8;
/// Hasta dónde llega la luz de un farol, en metros.
pub const LAMP_LIGHT_RANGE: f32 = 12.0;
/// La luz de un farol encendido a pleno brillo, en lúmenes (subida desde 30.000
/// después de que una prueba de juego nocturna encontró las calles demasiado
/// oscuras).
pub const LAMP_LUMENS: f32 = 75_000.0;
/// La luz cálida, amarillo anaranjada, de una llama de kerosén.
pub const KEROSENE: Color = Color::srgb(1.0, 0.72, 0.40);
/// El emisivo del vidrio a pleno brillo (lineal): lo bastante intenso para
/// distinguirse desde lejos en una calle oscura.
const GLASS_GLOW: LinearRgba = LinearRgba::rgb(6.0, 3.6, 1.3);
/// Cantidad máxima de triángulos que puede dibujar un farol, hierro y vidrio
/// juntos.
pub const LAMP_TRIANGLE_BUDGET: usize = 96;

/// Qué tan encendidos están los faroles a la hora `hour` (horas desde la
/// medianoche): 0 de día, 1 de noche, encendiéndose a medida que se pone el sol
/// y apagándose cuando sale.
pub fn lamp_glow(hour: f32) -> f32 {
    let elevation = sky_at(hour).elevation;
    let t = ((elevation + 0.06) / (0.10 + 0.06)).clamp(0.0, 1.0);
    1.0 - t * t * (3.0 - 2.0 * t)
}

/// Índices de los (como máximo) `n` puntos de `flames` más cercanos a `from`,
/// primero los más cercanos.
pub fn nearest(from: Vec3, flames: &[Vec3], n: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..flames.len()).collect();
    order.sort_by(|&a, &b| {
        flames[a]
            .distance_squared(from)
            .total_cmp(&flames[b].distance_squared(from))
    });
    order.truncate(n);
    order
}

/// Las dos mallas del farol en su propio espacio (el pie del poste en el
/// origen): el hierro y luego el vidrio.
fn lamp_shapes() -> (Shape, Shape) {
    let mut iron = Shape::default();
    let s = Surface::Steel;
    // Una base acanalada, el fuste, una barra de escalera para el farolero y el
    // asiento sobre el que se apoya el farol.
    iron.block(s, Vec3::new(-0.13, 0.0, -0.13), Vec3::new(0.13, 0.45, 0.13));
    iron.block(
        s,
        Vec3::new(-0.06, 0.45, -0.06),
        Vec3::new(0.06, 2.55, 0.06),
    );
    iron.beam(
        s,
        Vec3::new(-0.28, 2.35, 0.0),
        Vec3::new(0.28, 2.35, 0.0),
        0.04,
    );
    iron.cuboid(
        s,
        Transform::from_xyz(0.0, 2.575, 0.0).with_scale(Vec3::new(0.34, 0.05, 0.34)),
        true,
    );
    // Un techo piramidal sobre el vidrio y un remate.
    let (eave, apex) = (3.0, 3.22);
    let r = 0.22;
    let corners = [
        Vec3::new(-r, eave, r),
        Vec3::new(r, eave, r),
        Vec3::new(r, eave, -r),
        Vec3::new(-r, eave, -r),
    ];
    let top = Vec3::new(0.0, apex, 0.0);
    for k in 0..4 {
        iron.face(s, &[corners[k], corners[(k + 1) % 4], top]);
    }
    iron.face(s, &[corners[3], corners[2], corners[1], corners[0]]);
    iron.block(
        s,
        Vec3::new(-0.03, apex - 0.02, -0.03),
        Vec3::new(0.03, StreetLamp::HEIGHT, 0.03),
    );
    // La caja de vidrio entre el asiento y el techo. La superficie de su búfer
    // solo escala sus UV: el vidrio lleva el propio material luminoso de los
    // faroles.
    let mut glass = Shape::default();
    glass.block(
        Surface::Recess,
        Vec3::new(-0.15, 2.6, -0.15),
        Vec3::new(0.15, 3.0, 0.15),
    );
    (iron, glass)
}

/// Triángulos que dibuja un farol, hierro y vidrio.
pub fn lamp_triangles() -> usize {
    let (iron, glass) = lamp_shapes();
    iron.triangles() + glass.triangles()
}

/// Las mallas y materiales que comparten todos los faroles.
#[derive(Resource, Debug, Clone)]
pub struct LampKit {
    pub iron_mesh: Handle<Mesh>,
    pub glass_mesh: Handle<Mesh>,
    /// Hierro fundido pintado de oscuro.
    pub iron: Handle<StandardMaterial>,
    /// El vidrio del farol, que brilla según el reloj.
    pub glass: Handle<StandardMaterial>,
}

fn single_mesh(shape: Shape) -> Mesh {
    shape
        .into_meshes()
        .into_iter()
        .next()
        .map(|(_, mesh)| mesh)
        .expect("a lamp part has faces")
}

fn setup_kit(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let (iron, glass) = lamp_shapes();
    commands.insert_resource(LampKit {
        iron_mesh: meshes.add(single_mesh(iron)),
        glass_mesh: meshes.add(single_mesh(glass)),
        iron: materials.add(StandardMaterial {
            base_color: Color::srgb(0.11, 0.12, 0.11),
            metallic: 0.4,
            perceptual_roughness: 0.6,
            ..default()
        }),
        glass: materials.add(StandardMaterial {
            base_color: Color::srgb(0.62, 0.58, 0.46),
            perceptual_roughness: 0.25,
            emissive: LinearRgba::BLACK,
            ..default()
        }),
    });
    // El conjunto de luces, oculto hasta que los faroles brillan. En cada capa
    // en la que se dibuja el propio modelo del jugador, como el sol: las capas
    // de una luz eligen las cámaras para las que brilla y las mallas de sus
    // mapas de sombra, así que el jugador sigue en ellas aunque una cámara
    // oculte su cabeza o su cuerpo.
    for _ in 0..LAMP_LIGHTS {
        commands.spawn((
            Name::new("Lamp light"),
            LampLight,
            PointLight {
                color: KEROSENE,
                intensity: 0.0,
                range: LAMP_LIGHT_RANGE,
                radius: 0.1,
                shadow_maps_enabled: false,
                contact_shadows_enabled: false,
                ..default()
            },
            world_layers(),
            Transform::default(),
            Visibility::Hidden,
        ));
    }
}

/// Crea un farol en el punto del suelo `at`, apoyado sobre `ground`, como hijo
/// de `parent`; `range` es el del poste y el del vidrio, cuando se dibujan
/// dentro de rangos compartidos.
fn spawn_lamp(
    commands: &mut Commands,
    kit: &LampKit,
    parent: Entity,
    (at, ground): (GroundVec2, f32),
    terrain: &Terrain,
    range: Option<[VisibilityRange; 2]>,
) {
    let foot = Transform::from_translation(world_from_ground(at, ground));
    let mut post = commands.spawn((
        LampPost,
        Mesh3d(kit.iron_mesh.clone()),
        MeshMaterial3d(kit.iron.clone()),
        foot,
        ThinOccluder::post(at, 0.3, terrain, StreetLamp::HEIGHT),
        ChildOf(parent),
    ));
    if let Some([r, _]) = &range {
        post.insert(r.clone());
    }
    let mut glass = commands.spawn((
        LampGlass {
            flame: world_from_ground(at, ground + StreetLamp::FLAME),
        },
        Mesh3d(kit.glass_mesh.clone()),
        MeshMaterial3d(kit.glass.clone()),
        foot,
        ChildOf(parent),
    ));
    if let Some([_, r]) = range {
        glass.insert(r);
    }
}

/// Dibuja los [`WorldLamps`] con los pueblos y asentamientos a los que
/// pertenecen, brillando e iluminados de noche. Necesita el
/// [`crate::TownPlugin`] para los faroles de pueblo y el
/// [`crate::TerrainPlugin`] para los de estancia.
pub struct LampPlugin;

impl Plugin for LampPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldLamps>()
            .init_resource::<WorldSettlements>()
            .init_resource::<WorldTerrain>()
            .init_resource::<WorldChunks>()
            .init_resource::<StreamFocus>()
            .add_systems(Startup, setup_kit)
            .add_systems(Update, light_towns)
            .add_systems(
                PostUpdate,
                (
                    light_estancias.after(ChunkStreaming),
                    glow_lamps,
                    light_nearest,
                )
                    .before(TransformSystems::Propagate),
            );
    }
}

/// Levanta los faroles de cada pueblo dibujado en este frame, bajo su raíz.
fn light_towns(
    mut commands: Commands,
    kit: Option<Res<LampKit>>,
    (lamps, terrain): (Res<WorldLamps>, Res<WorldTerrain>),
    roots: Query<(Entity, &TownRoot), Added<TownRoot>>,
) {
    let Some(kit) = kit else {
        return;
    };
    for (root, town) in &roots {
        let Some(placed) = lamps.0.towns.get(town.0) else {
            continue;
        };
        for lamp in placed {
            let ground = terrain.0.height(lamp.at.x, lamp.at.y);
            spawn_lamp(
                &mut commands,
                &kit,
                root,
                (lamp.at, ground),
                &terrain.0,
                None,
            );
        }
    }
}

/// Marca un chunk cuyos faroles de estancia ya están levantados.
#[derive(Component)]
struct LampsLaid;

/// Levanta los faroles de los asentamientos de los que es dueño cada chunk
/// nuevo (su centro cae en él), sobre el suelo dibujado y dentro de los rangos
/// compartidos de los asentamientos: el poste en el de los objetos, el vidrio
/// en el de los edificios, para que brille desde lejos.
fn light_estancias(
    mut commands: Commands,
    kit: Option<Res<LampKit>>,
    (lamps, layout, terrain, world): (
        Res<WorldLamps>,
        Res<WorldSettlements>,
        Res<WorldTerrain>,
        Res<WorldChunks>,
    ),
    detail: Option<Res<ChunkDetail>>,
    chunks: Query<(Entity, &TerrainChunk), Without<LampsLaid>>,
) {
    let Some(kit) = kit else {
        return;
    };
    let size = world.0.chunk_size;
    let [building, prop, _] = ranges();
    for (chunk, coord) in &chunks {
        commands.entity(chunk).try_insert(LampsLaid);
        let owned = layout
            .0
            .settlements
            .iter()
            .zip(&lamps.0.settlements)
            .filter(|(s, _)| ChunkCoord::containing(s.hub, size) == coord.0);
        for (_, placed) in owned {
            for lamp in placed {
                let ground = match &detail {
                    Some(d) => mesh_height(&terrain.0, lamp.at, size, d.resolution),
                    None => terrain.0.height(lamp.at.x, lamp.at.y),
                };
                spawn_lamp(
                    &mut commands,
                    &kit,
                    chunk,
                    (lamp.at, ground),
                    &terrain.0,
                    Some([prop.clone(), building.clone()]),
                );
            }
        }
    }
}

/// Fija el brillo del vidrio según el reloj, tocando el material compartido
/// solo cuando el brillo cambió.
fn glow_lamps(
    clock: Option<Res<WorldClock>>,
    kit: Option<Res<LampKit>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut shown: Local<Option<f32>>,
) {
    let (Some(clock), Some(kit)) = (clock, kit) else {
        return;
    };
    let glow = lamp_glow(clock.0.hour());
    if shown.is_some_and(|s| (s - glow).abs() < 0.005 && (glow > 0.0 || s == 0.0)) {
        return;
    }
    if let Some(mut glass) = materials.get_mut(&kit.glass) {
        glass.emissive = LinearRgba::rgb(GLASS_GLOW.red, GLASS_GLOW.green, GLASS_GLOW.blue) * glow;
        *shown = Some(glow);
    }
}

/// Mueve el conjunto de luces a los faroles más cercanos a la cámara (al foco
/// del streaming si no hay cámara) mientras brillan; lo oculta de día.
fn light_nearest(
    clock: Option<Res<WorldClock>>,
    focus: Res<StreamFocus>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    glasses: Query<&LampGlass>,
    mut lights: Query<(&mut Transform, &mut PointLight, &mut Visibility), With<LampLight>>,
) {
    let glow = clock.map_or(0.0, |c| lamp_glow(c.0.hour()));
    let flames: Vec<Vec3> = if glow > 0.0 {
        glasses.iter().map(|g| g.flame).collect()
    } else {
        Vec::new()
    };
    let from = camera
        .iter()
        .next()
        .map_or_else(|| world_from_ground(focus.0, 0.0), |t| t.translation());
    let lit = nearest(from, &flames, LAMP_LIGHTS);
    let mut lit = lit.into_iter();
    for (mut transform, mut light, mut visibility) in &mut lights {
        match lit.next() {
            Some(k) => {
                transform.translation = flames[k];
                light.intensity = LAMP_LUMENS * glow;
                visibility.set_if_neq(Visibility::Inherited);
            }
            None => {
                visibility.set_if_neq(Visibility::Hidden);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::viewer::{OWN_BODY_LAYER, OWN_HEAD_LAYER};
    use bevy::camera::visibility::RenderLayers;
    use pampa_core::{
        Aabb, GameClock, HouseStyle, LampSite, RoadNetwork, Settlement, SettlementKind,
        SettlementLayout, Side, Structure, StructureKind, TimeOfDay, TownLayout, Track,
    };

    #[test]
    fn a_lamp_stays_within_its_triangle_budget() {
        let n = lamp_triangles();
        println!("a lamp draws {n} triangles");
        assert!(n <= LAMP_TRIANGLE_BUDGET, "{n}");
        let (iron, glass) = lamp_shapes();
        // Una malla cada uno.
        assert_eq!(iron.into_meshes().len(), 1);
        assert_eq!(glass.into_meshes().len(), 1);
    }

    #[test]
    fn the_glass_is_dark_at_noon_and_lit_at_night_fading_between() {
        assert_eq!(lamp_glow(12.0), 0.0);
        assert_eq!(lamp_glow(8.0), 0.0);
        assert_eq!(lamp_glow(23.0), 1.0);
        assert_eq!(lamp_glow(3.0), 1.0);
        // El anochecer pasa por todos los matices: no hay un cambio brusco.
        let dusk: Vec<f32> = (0..=60)
            .map(|m| lamp_glow(19.0 + m as f32 / 60.0))
            .collect();
        assert!(dusk.iter().any(|&g| g > 0.05 && g < 0.95), "{dusk:?}");
        assert!(dusk.windows(2).all(|w| w[1] >= w[0] && w[1] - w[0] < 0.25));
    }

    #[test]
    fn nearest_picks_the_closest_flames_in_order() {
        let flames: Vec<Vec3> = (0..20)
            .map(|k| Vec3::new(k as f32 * 7.0, 2.8, 0.0))
            .collect();
        let picked = nearest(Vec3::new(70.0, 0.0, 0.0), &flames, 3);
        assert_eq!(picked, vec![10, 9, 11]);
        assert_eq!(nearest(Vec3::ZERO, &flames[..2], 8).len(), 2);
    }

    fn app(hour: u32) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .insert_resource(WorldClock(GameClock::at(TimeOfDay::hm(hour, 0))))
            .add_plugins(LampPlugin);
        app
    }

    fn lit(app: &mut App) -> Vec<Vec3> {
        let mut q = app
            .world_mut()
            .query::<(&Transform, &Visibility, &PointLight)>();
        q.iter(app.world())
            .filter(|(_, v, l)| **v != Visibility::Hidden && l.intensity > 0.0)
            .map(|(t, _, _)| t.translation)
            .collect()
    }

    fn glass_glow(app: &App) -> LinearRgba {
        let kit = app.world().resource::<LampKit>();
        let materials = app.world().resource::<Assets<StandardMaterial>>();
        materials.get(&kit.glass).unwrap().emissive
    }

    /// Una fila de 20 vidrios sueltos a lo largo de x, separados 7 m.
    fn row(app: &mut App) -> Vec<Vec3> {
        let flames: Vec<Vec3> = (0..20)
            .map(|k| Vec3::new(k as f32 * 7.0, 2.8, 0.0))
            .collect();
        for &flame in &flames {
            app.world_mut().spawn(LampGlass { flame });
        }
        flames
    }

    #[test]
    fn by_night_the_pool_lights_only_the_nearest_lamps() {
        let mut app = app(23);
        let flames = row(&mut app);
        app.world_mut().resource_mut::<StreamFocus>().0 = GroundVec2::new(70.0, 0.0);
        app.update();
        let lights = lit(&mut app);
        assert_eq!(lights.len(), LAMP_LIGHTS);
        let want: Vec<Vec3> = nearest(Vec3::new(70.0, 0.0, 0.0), &flames, LAMP_LIGHTS)
            .into_iter()
            .map(|k| flames[k])
            .collect();
        for l in &lights {
            assert!(want.contains(l), "{l} lit, not among the nearest");
        }
        let mut q = app
            .world_mut()
            .query::<(&PointLight, Option<&RenderLayers>)>();
        for (light, layers) in q.iter(app.world()) {
            assert!(!light.shadow_maps_enabled && !light.contact_shadows_enabled);
            assert_eq!(light.range, LAMP_LIGHT_RANGE);
            // Cada capa en la que se dibuja el modelo del jugador, también la
            // del mundo.
            let layers = layers.cloned().unwrap_or_default();
            for layer in [0, OWN_HEAD_LAYER, OWN_BODY_LAYER] {
                assert!(
                    layers.intersects(&RenderLayers::layer(layer)),
                    "a lamp light off layer {layer}"
                );
            }
        }
        assert!(glass_glow(&app).red > 1.0);

        // Al seguir cabalgando, el conjunto lo acompaña.
        app.world_mut().resource_mut::<StreamFocus>().0 = GroundVec2::new(0.0, 0.0);
        app.update();
        let lights = lit(&mut app);
        assert_eq!(lights.len(), LAMP_LIGHTS);
        assert!(lights.iter().all(|l| l.x < 7.0 * LAMP_LIGHTS as f32));
    }

    #[test]
    fn by_day_no_light_is_on_and_the_glass_is_dark() {
        let mut app = app(12);
        row(&mut app);
        app.update();
        assert!(lit(&mut app).is_empty());
        let glow = glass_glow(&app);
        assert_eq!((glow.red, glow.green, glow.blue), (0.0, 0.0, 0.0));
        let mut q = app.world_mut().query::<&LampLight>();
        assert_eq!(q.iter(app.world()).count(), LAMP_LIGHTS);
    }

    #[test]
    fn a_drawn_town_puts_up_its_lamps_sharing_two_meshes_and_two_materials() {
        let town = TownLayout::castex_inspired();
        let placed = pampa_core::lamps::town_lamps(&town, 0, &RoadNetwork::new(Vec::new(), &[]));
        assert!(placed.len() > 10);
        let mut app = app(23);
        app.insert_resource(WorldLamps(StreetLamps {
            towns: vec![placed.clone()],
            settlements: Vec::new(),
        }));
        app.update();
        app.world_mut()
            .spawn((TownRoot(0), Transform::default(), Visibility::default()));
        app.update();
        let mut posts = app.world_mut().query_filtered::<(
            &Mesh3d,
            &MeshMaterial3d<StandardMaterial>,
            Option<&ThinOccluder>,
        ), With<LampPost>>();
        let posts: Vec<_> = posts
            .iter(app.world())
            .map(|(m, mat, fade)| (m.0.id(), mat.0.id(), fade.is_some()))
            .collect();
        assert_eq!(posts.len(), placed.len());
        assert!(
            posts
                .iter()
                .all(|p| p.0 == posts[0].0 && p.1 == posts[0].1 && p.2)
        );
        let mut glasses = app.world_mut().query::<(
            &LampGlass,
            &MeshMaterial3d<StandardMaterial>,
            Has<VisibilityRange>,
        )>();
        let glasses: Vec<_> = glasses
            .iter(app.world())
            .map(|(g, mat, ranged)| (g.flame, mat.0.id(), ranged))
            .collect();
        assert_eq!(glasses.len(), placed.len());
        assert!(glasses.iter().all(|g| g.1 == glasses[0].1 && !g.2));
        assert_ne!(posts[0].1, glasses[0].1);
        // La llama de cada farol arde sobre su pie, arriba en el farol.
        let terrain = &app.world().resource::<WorldTerrain>().0;
        for lamp in &placed {
            let foot = world_from_ground(lamp.at, terrain.height(lamp.at.x, lamp.at.y));
            let flame = foot + Vec3::Y * StreetLamp::FLAME;
            assert!(
                glasses.iter().any(|g| g.0.distance(flame) < 1e-3),
                "{lamp:?}"
            );
        }
    }

    /// Una estancia alrededor de `hub`: su casco al norte, mirando hacia el
    /// centro, y su huella de entrada desde el oeste.
    fn estancia(hub: GroundVec2) -> Settlement {
        let casco = Structure {
            kind: StructureKind::MainHouse(HouseStyle::GalleryHouse),
            footprint: Aabb::from_center_size(
                hub + GroundVec2::new(0.0, 30.0),
                GroundVec2::new(20.0, 12.0),
            ),
            height: 4.5,
            front: Side::South,
            variant: 0,
        };
        Settlement {
            kind: SettlementKind::Ranch,
            gate: 0,
            hub,
            patio: Aabb::from_center_size(hub, GroundVec2::new(20.0, 20.0)),
            structures: vec![casco],
            tracks: vec![Track {
                points: vec![hub - GroundVec2::new(120.0, 0.0), hub],
            }],
            bounds: Aabb::from_center_size(hub, GroundVec2::new(260.0, 120.0)),
            seed: 1,
        }
    }

    #[test]
    fn an_estancia_s_lamps_come_with_its_chunk_within_the_shared_ranges() {
        let s = estancia(GroundVec2::new(40.0, 40.0));
        let placed = pampa_core::lamps::settlement_lamps(&s);
        assert!(placed.iter().any(|l| l.site == LampSite::Homestead));
        assert!(placed.iter().any(|l| l.site == LampSite::Gate));
        let mut app = app(23);
        let mut layout = SettlementLayout::default();
        layout.settlements.push(s.clone());
        app.insert_resource(WorldSettlements(layout))
            .insert_resource(WorldLamps(StreetLamps {
                towns: Vec::new(),
                settlements: vec![placed.clone()],
            }));
        app.update();
        let size = app.world().resource::<WorldChunks>().0.chunk_size;
        let coord = ChunkCoord::containing(s.hub, size);
        let chunk = app
            .world_mut()
            .spawn((
                TerrainChunk(coord),
                Transform::default(),
                Visibility::default(),
            ))
            .id();
        app.update();
        app.update();
        let shared = ranges();
        let mut q = app
            .world_mut()
            .query::<(&ChildOf, &VisibilityRange, Has<LampPost>, Has<LampGlass>)>();
        let parts: Vec<_> = q.iter(app.world()).collect();
        assert_eq!(parts.len(), 2 * placed.len());
        for (parent, range, post, glass) in parts {
            assert_eq!(parent.parent(), chunk);
            assert!(post ^ glass);
            assert!(
                shared.contains(range),
                "a new range ending {:?}",
                range.end_margin
            );
        }
    }
}
