//! Kerosene street lamps (faroles): a cast-iron post with a glass lantern
//! on top, at the places `pampa-core` puts them ([`StreetLamps`]).
//!
//! - **Drawing**: two shared meshes, the iron (post, ladder bar, lantern
//!   seat, roof and finial) and the glass, under two shared materials, so
//!   every lamp of the world is two batched draws. A town's lamps hang off
//!   its [`TownRoot`] and stream with it; an estancia's off the terrain
//!   chunk that owns its hub, within the settlements' shared
//!   [`VisibilityRange`]s (no new range: Bevy never frees one). The post
//!   fades out of the camera's way like a fence post ([`ThinOccluder`]).
//! - **Glow**: the glass's emissive follows the game clock: off by day,
//!   fading in at dusk and out at dawn ([`lamp_glow`]). One shared
//!   material, so one asset change lights every lamp.
//! - **Light**: no lamp owns a light. A pool of [`LAMP_LIGHTS`] shadowless
//!   point lights moves to the lamps nearest the camera each frame while
//!   they glow; by day the pool is hidden, so no light is drawn at all.

use bevy::camera::visibility::VisibilityRange;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use pampa_core::{ChunkCoord, StreetLamp, StreetLamps, Terrain, Vec2 as GroundVec2};

use crate::atmosphere::{WorldClock, sky_at};
use crate::coords::world_from_ground;
use crate::flora::ThinOccluder;
use crate::settlement::shape::Shape;
use crate::settlement::{Surface, WorldSettlements, ranges};
use crate::terrain::{
    ChunkDetail, ChunkStreaming, StreamFocus, TerrainChunk, WorldChunks, WorldTerrain, mesh_height,
};
use crate::town::TownRoot;

/// Every street lamp of the world. Insert before the plugin runs; without
/// it none is drawn.
#[derive(Resource, Debug, Clone, Default)]
pub struct WorldLamps(pub StreetLamps);

/// A lamp's post (the iron mesh).
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct LampPost;

/// A lamp's glass, with where its flame burns in world space: where a pool
/// light goes when this lamp is among the nearest.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct LampGlass {
    pub flame: Vec3,
}

/// One light of the pool lighting the nearest lamps.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct LampLight;

/// How many real point lights the lamps share: the nearest this many to
/// the camera are lit at night; the rest only glow.
pub const LAMP_LIGHTS: usize = 8;
/// How far a lamp's light reaches, in meters.
pub const LAMP_LIGHT_RANGE: f32 = 12.0;
/// A lit lamp's light at full glow, in lumens.
pub const LAMP_LUMENS: f32 = 30_000.0;
/// The warm, yellow-orange light of a kerosene flame.
pub const KEROSENE: Color = Color::srgb(1.0, 0.72, 0.40);
/// The glass's emissive at full glow (linear): bright enough to read far
/// down a dark street.
const GLASS_GLOW: LinearRgba = LinearRgba::rgb(6.0, 3.6, 1.3);
/// Most triangles one lamp may draw, iron and glass together.
pub const LAMP_TRIANGLE_BUDGET: usize = 96;

/// How lit the lamps are at `hour` (hours since midnight): 0 by day, 1 at
/// night, fading in as the sun sets and out as it rises.
pub fn lamp_glow(hour: f32) -> f32 {
    let elevation = sky_at(hour).elevation;
    let t = ((elevation + 0.06) / (0.10 + 0.06)).clamp(0.0, 1.0);
    1.0 - t * t * (3.0 - 2.0 * t)
}

/// Indices of the (at most) `n` points of `flames` nearest `from`, nearest
/// first.
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

/// The lamp's two meshes in its own space (foot of the post at the
/// origin): the iron, then the glass.
fn lamp_shapes() -> (Shape, Shape) {
    let mut iron = Shape::default();
    let s = Surface::Steel;
    // A fluted base, the shaft, a ladder bar for the lamplighter and the
    // seat the lantern stands on.
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
    // A pyramid roof over the glass and a finial.
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
    // The glass box between the seat and the roof. Its buffer's surface
    // only scales its UVs: the glass wears the lamps' own glowing material.
    let mut glass = Shape::default();
    glass.block(
        Surface::Recess,
        Vec3::new(-0.15, 2.6, -0.15),
        Vec3::new(0.15, 3.0, 0.15),
    );
    (iron, glass)
}

/// Triangles one lamp draws, iron and glass.
pub fn lamp_triangles() -> usize {
    let (iron, glass) = lamp_shapes();
    iron.triangles() + glass.triangles()
}

/// The meshes and materials every lamp shares.
#[derive(Resource, Debug, Clone)]
pub struct LampKit {
    pub iron_mesh: Handle<Mesh>,
    pub glass_mesh: Handle<Mesh>,
    /// Dark painted cast iron.
    pub iron: Handle<StandardMaterial>,
    /// The lantern's glass, glowing with the clock.
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
    // The light pool, hidden until the lamps glow.
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
            Transform::default(),
            Visibility::Hidden,
        ));
    }
}

/// Spawns one lamp at ground point `at`, standing on `ground`, as a child
/// of `parent`; `range` is the post's and the glass's, when drawn within
/// shared ranges.
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

/// Draws the [`WorldLamps`] with the towns and settlements they belong to,
/// glowing and lit by night. Needs the [`crate::TownPlugin`] for town
/// lamps and the [`crate::TerrainPlugin`] for estancia lamps.
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

/// Puts up the lamps of every town drawn this frame, under its root.
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

/// Marks a chunk whose estancia lamps are up.
#[derive(Component)]
struct LampsLaid;

/// Puts up the lamps of the settlements each new chunk owns (its hub lies
/// in it), on the drawn ground, within the settlements' shared ranges: the
/// post the props', the glass the buildings', so it shines from afar.
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

/// Sets the glass's glow from the clock, touching the shared material only
/// when the glow has changed.
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

/// Moves the light pool to the lamps nearest the camera (the stream focus
/// without one) while they glow; hides it by day.
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
        // One mesh each.
        assert_eq!(iron.into_meshes().len(), 1);
        assert_eq!(glass.into_meshes().len(), 1);
    }

    #[test]
    fn the_glass_is_dark_at_noon_and_lit_at_night_fading_between() {
        assert_eq!(lamp_glow(12.0), 0.0);
        assert_eq!(lamp_glow(8.0), 0.0);
        assert_eq!(lamp_glow(23.0), 1.0);
        assert_eq!(lamp_glow(3.0), 1.0);
        // Dusk passes through every shade: no sudden switch.
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

    /// A row of 20 bare glasses along x, 7 m apart.
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
        let mut q = app.world_mut().query::<&PointLight>();
        for light in q.iter(app.world()) {
            assert!(!light.shadow_maps_enabled && !light.contact_shadows_enabled);
            assert_eq!(light.range, LAMP_LIGHT_RANGE);
        }
        assert!(glass_glow(&app).red > 1.0);

        // Riding on, the pool follows.
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
        // Each lamp's flame burns over its foot, up in the lantern.
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

    /// An estancia round `hub`: its casco to the north, facing the hub,
    /// and its track in from the west.
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
