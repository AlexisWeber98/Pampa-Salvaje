//! Third-person orbit camera around the player.
//!
//! The mouse (or Q/E) swings the camera all the way around the rider, the
//! wheel zooms, and after a moment without camera input the camera eases
//! back behind a moving rider. Buildings, the platform canopy and the line
//! train's cars never come between the camera and the rider: it pulls in in
//! front of them at once, and eases back out slowly once the view clears.
//! Tree trunks pull it in too, but softly ([`trunk_reach`]): the camera
//! eases round a trunk rather than snapping at it, and it looks through the
//! leaves of a crown. Other thin things (fences, signposts, lamps) do not
//! move the camera: the line of sight is handed to render each frame
//! ([`SightLine`]), which fades whichever of them stand in it.
//!
//! Aboard the train ([`OnTrain`]) `V` cycles three views ([`RideView`]):
//! seated by a window inside the coach (the default), facing forward the
//! way the train runs and looking round (out of either window) with the
//! mouse within [`SEAT_YAW_LIMIT`] and [`SEAT_PITCH_LIMIT`]; the orbit
//! around the coach, from farther out ([`RIDE_DISTANCE`]), which the
//! train's own cars do not pull in; and the whole train from the side,
//! slowly following it. The HUD shows the view ([`HudHint`]). Boarding and
//! getting off place the camera afresh; every time the seat is taken (the
//! opening, boarding, a skipped ride coming in, `V` back to the window) the
//! head turns forward again.
//! The orbit math lives in [`crate::orbit`].
//!
//! Off the train `V` toggles the first-person view from the player's eyes
//! instead ([`crate::first_person`]): this module keeps placing the orbit
//! camera underneath (without orbiting it, and with no line of sight to
//! fade), and the first person moves the camera from there to the eyes.

use bevy::camera::visibility::VisibilitySystems;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::light::SimulationLightSystems;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use pampa_core::{
    Aabb, BuildingKind, ChunkCoord, CoachBody, FloraSite, Ground, LineTrain, PlacedCar,
    SettlementLayout, StructureKind, Terrain, TownLayout, Travel, Vec2 as GroundVec2, World,
};
use pampa_render::character::viewer::world_layers;
use pampa_render::{
    CloseShadows, HudHint, SightLine, WorldFlora, WorldSettlements, WorldTowns, WorldTrains,
    car_transform, world_from_ground,
};

use crate::first_person::{
    BodyTooClose, EyeFollow, FirstPersonLook, ViewBlend, ViewMode, place_first_person, toggle_view,
    turn_to_view,
};
use crate::orbit::{Orbit, OrbitInput, OrbitParams};
use crate::player::{DrivePlayer, GameWorld, Player};
use crate::ride::{OnTrain, Trip, TripPhase};

/// Orbit distance the camera takes around the coach, in meters: far enough
/// out to frame the coach and the country ahead.
pub const RIDE_DISTANCE: f32 = 16.0;

/// Cycles the riding views aboard; toggles the first person off the train.
pub const VIEW_KEY: KeyCode = KeyCode::KeyV;

/// Farthest the seated player turns their head from facing forward, either
/// way, in radians (about 120°): past both windows, and the platform's.
pub const SEAT_YAW_LIMIT: f32 = 2.094;
/// Farthest the seated player looks up or down, in radians (about 40°).
pub const SEAT_PITCH_LIMIT: f32 = 0.698;

/// How the camera shows a train ride, cycled with [`VIEW_KEY`].
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RideView {
    /// Seated by a window inside the coach.
    #[default]
    Window,
    /// Orbiting the coach from outside.
    Coach,
    /// The whole train from the side.
    Train,
}

impl RideView {
    /// The next view, wrapping around.
    pub fn next(self) -> Self {
        match self {
            Self::Window => Self::Coach,
            Self::Coach => Self::Train,
            Self::Train => Self::Window,
        }
    }

    /// The HUD hint for this view.
    pub fn hint(self) -> String {
        let name = match self {
            Self::Window => "ventana",
            Self::Coach => "exterior",
            Self::Train => "tren",
        };
        format!("V: vista - {name}")
    }
}

/// Where the seated player looks, from facing forward along the coach:
/// `yaw` turns the head (positive to the left), `pitch` looks up (positive)
/// or down.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct SeatLook {
    pub yaw: f32,
    pub pitch: f32,
}

impl SeatLook {
    /// Turns the head with this frame's camera input (the mouse, Q/E),
    /// within the seat's limits.
    pub fn update(&mut self, input: OrbitInput, params: &OrbitParams, dt: f32) {
        let finite = |v: f32| if v.is_finite() { v } else { 0.0 };
        let turn = finite(input.look.0) * params.mouse_sensitivity
            + finite(input.keys).clamp(-1.0, 1.0) * params.key_rate * finite(dt);
        self.yaw = (self.yaw - turn).clamp(-SEAT_YAW_LIMIT, SEAT_YAW_LIMIT);
        self.pitch = (self.pitch - finite(input.look.1) * params.mouse_sensitivity)
            .clamp(-SEAT_PITCH_LIMIT, SEAT_PITCH_LIMIT);
    }
}

/// The camera seated by the window ([`CoachBody::window_seat`]) on `side`
/// of a coach `length` long standing at `car`, facing forward (the car's
/// +X, its direction of travel) turned by `look`.
pub fn seat_view(car: &Transform, length: f32, side: f32, look: SeatLook) -> Transform {
    let (along, across, eye) = CoachBody::window_seat(length, side);
    let at = car.transform_point(Vec3::new(along, eye, across));
    // A camera looks down its -Z: a quarter turn right faces the car's +X.
    let facing = -std::f32::consts::FRAC_PI_2;
    let turn = Quat::from_rotation_y(facing + look.yaw) * Quat::from_rotation_x(look.pitch);
    Transform::from_translation(at).with_rotation(car.rotation * turn)
}

/// The whole train seen from its side: a spot out to the right of the
/// middle of the consist, high enough to see over it, and the point it
/// looks at.
pub fn train_view(cars: &[PlacedCar], terrain: &Terrain) -> Option<(Vec3, Vec3)> {
    let first = cars.first()?;
    let last = cars.last()?;
    let middle = (first.center + last.center) * 0.5;
    let along = first.center - last.center;
    let span = along.length().max(1.0);
    let right = GroundVec2::new(along.y, -along.x) * (1.0 / span);
    let ground = |p: GroundVec2| terrain.height(p.x, p.y);
    let spot = middle + right * (span * 0.55 + 15.0);
    let look = world_from_ground(middle, ground(middle) + 2.5);
    Some((world_from_ground(spot, ground(spot) + 9.0), look))
}

/// How the camera follows the rider.
#[derive(Component, Debug, Clone, Copy)]
pub struct FollowCamera {
    /// Point above the rider's feet the camera looks at.
    pub look_height: f32,
    /// Point above the player's feet the camera looks at on foot: a
    /// person's chest is lower than a rider's.
    pub on_foot_look_height: f32,
    /// Exponential catch-up rate of the look-at point (higher = snappier).
    pub smoothing: f32,
    /// Minimum clearance above the ground (or platform) under the camera.
    pub ground_clearance: f32,
    /// Farther than this from the rider, the camera jumps behind it instead
    /// of gliding (first frame, respawns).
    pub snap_distance: f32,
}

impl Default for FollowCamera {
    fn default() -> Self {
        Self {
            look_height: 1.6,
            on_foot_look_height: 1.1,
            smoothing: 10.0,
            ground_clearance: 1.0,
            snap_distance: 50.0,
        }
    }
}

/// The camera's orbit and its smoothed look-at point; `None` until the
/// camera is first placed behind the rider.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CameraRig(pub Option<(Orbit, Vec3)>);

/// How far out along the arm from the rider to its orbit spot the camera
/// sits, as a fraction of the arm (1 = all the way out): the smoothed
/// pull-in, see [`ease_reach`].
#[derive(Component, Debug, Clone, Copy)]
pub struct CameraReach(pub f32);

impl Default for CameraReach {
    fn default() -> Self {
        Self(1.0)
    }
}

/// Orbit tuning for the player's camera.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct CameraTuning(pub OrbitParams);

/// Wheel scroll reported in pixels (touchpads) per line of zoom.
const PIXELS_PER_LINE: f32 = 100.0;

/// Something the camera cannot see or pass through: its footprint between
/// the world heights `bottom` and `top`. A building or a train car is a
/// column standing on the ground; the platform canopy is a slab overhead,
/// so the camera may still look under it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Occluder {
    pub footprint: Aabb,
    pub bottom: f32,
    pub top: f32,
}

impl Occluder {
    /// A column standing on the ground at height `ground`, up to `top`.
    pub fn column(footprint: Aabb, ground: f32, top: f32) -> Self {
        Self {
            footprint,
            bottom: ground,
            top,
        }
    }

    /// Whether the world point `p` is inside.
    #[cfg(test)]
    pub fn contains(&self, p: Vec3) -> bool {
        self.footprint.contains_point(GroundVec2::new(p.x, -p.z))
            && p.y > self.bottom
            && p.y < self.top
    }
}

/// Everything static that blocks the camera's line of sight to the rider
/// (the line train's cars are added each frame, see [`car_occluders`]).
///
/// Buildings count as columns up to their roofs, so the camera may still
/// look over a low shed, and the platform canopy as a roof slab. Canopy posts
/// and other thin things are left out on purpose: a post sweeping across the
/// view would make the camera jump in and out. Fences and signposts are
/// faded instead (see [`SightLine`]). Tree trunks are not columns here
/// either: they come and go with the chunks and pull the camera in softly
/// (see [`trunk_reach`]).
#[derive(Resource, Debug, Clone, Default)]
pub struct CameraOccluders(pub Vec<Occluder>);

/// Extra height over a building's walls for roofs, parapets and towers.
const ROOF_ALLOWANCE: f32 = 3.0;
/// How far in front of a blocking wall the camera stops, in meters.
const WALL_CLEARANCE: f32 = 0.4;
/// Cars farther than this (plus half their length) from the rider cannot
/// come between it and the camera, whose arm is at most 25 m long.
const CAR_REACH: f32 = 30.0;
/// Time constant of the camera easing back out once the view clears, in
/// seconds. Pulling in has none: it is instant (see [`ease_reach`]).
pub const EASE_OUT: f32 = 0.35;

impl CameraOccluders {
    /// Every building of every town, standing on `terrain`.
    pub fn from_towns(towns: &[TownLayout], terrain: &Terrain) -> Self {
        Self(
            towns
                .iter()
                .flat_map(|town| Self::from_town(town, terrain).0)
                .collect(),
        )
    }

    /// Adds every house and barn of `settlements` as a column up to its
    /// roof. Tanks, haystacks, corrals and the thin props are left out: low
    /// or thin, they fade instead (see [`SightLine`]).
    pub fn with_settlements(mut self, settlements: &SettlementLayout, terrain: &Terrain) -> Self {
        let ground = |p: GroundVec2| terrain.height(p.x, p.y);
        self.0.extend(
            settlements
                .settlements
                .iter()
                .flat_map(|s| s.structures.iter())
                .filter(|s| s.kind.is_house() || s.kind == StructureKind::Barn)
                .map(|s| {
                    let g = ground(s.footprint.center());
                    Occluder::column(s.footprint, g, g + s.height + ROOF_ALLOWANCE)
                }),
        );
        self
    }

    /// Every building of `town` as a column over its collider, standing on
    /// `terrain`, and the platform canopy's roof. The platform is left out: it is a floor the player
    /// stands on, and the camera's ground clearance keeps it above it.
    pub fn from_town(town: &TownLayout, terrain: &Terrain) -> Self {
        let ground = |p: GroundVec2| terrain.height(p.x, p.y);
        let buildings = town
            .buildings
            .iter()
            .filter(|b| b.kind != BuildingKind::Platform)
            .map(|b| {
                // Its collider: a galpón's walls, not its eaves, which the
                // player may stand under.
                let c = b.collider();
                let g = ground(c.center());
                Occluder::column(c, g, g + b.height + ROOF_ALLOWANCE)
            });
        // The canopy hangs at its height over the leveled station ground.
        let canopies = town.canopies().into_iter().map(|c| {
            let g = ground(c.footprint.center());
            Occluder {
                footprint: c.footprint,
                bottom: g + c.bottom(),
                top: g + c.top(),
            }
        });
        Self(buildings.chain(canopies).collect())
    }
}

/// The trains' cars near `near` (the rider) as columns from the ground
/// to their roofs, a row of short boxes for a car on a slant.
pub fn car_occluders(cars: &[PlacedCar], terrain: &Terrain, near: GroundVec2) -> Vec<Occluder> {
    let ground = |p: GroundVec2| terrain.height(p.x, p.y);
    cars.iter()
        .filter(|car| car.center.distance(near) <= CAR_REACH + car.length / 2.0)
        .flat_map(|car| {
            // The roof over the higher of the car's two ends.
            let half = car.direction * (car.length / 2.0);
            let rails = ground(car.center - half).max(ground(car.center + half));
            let top = rails + LineTrain::RAIL_TOP + car.kind.height();
            // Each car also covers half of the coupling gap at both ends, so
            // the sight line cannot slip between two cars and leave the
            // camera standing in the gap.
            let bridged = PlacedCar {
                length: car.length + LineTrain::COUPLING,
                ..*car
            };
            bridged
                .boxes()
                .into_iter()
                .map(move |b| Occluder::column(b, ground(b.center()), top))
        })
        .collect()
}

/// A tree trunk the camera keeps off the line of sight to the rider.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trunk {
    pub at: GroundVec2,
    pub radius: f32,
    /// Heights the trunk and its main limbs span, world meters. A line
    /// under `bottom` passes below the trunk's foot (through a rise), a line
    /// over `top` through the crown: neither is blocked.
    pub bottom: f32,
    pub top: f32,
}

/// How much of a tree's height the camera treats as trunk, up to where the
/// crown's mass begins. Above it the line of sight runs through leaves,
/// which are alpha-cut cards the rider shows through.
const TRUNK_TOP: f32 = 0.6;
/// A line this far (meters) past a trunk's side, or above its top, is
/// unaffected; the pull-in grows smoothly to the full stop from there, so a
/// trunk sweeping across the view moves the camera gradually instead of
/// snapping it.
const TRUNK_SOFT: f32 = 0.8;
const TRUNK_TOP_SOFT: f32 = 1.2;
/// A trunk this close (meters along the line) to the rider is ignored, and
/// counts fully from `TRUNK_NEAR + TRUNK_NEAR_BLEND`: a tree beside the
/// rider must not slam the camera onto it.
const TRUNK_NEAR: f32 = 1.5;
const TRUNK_NEAR_BLEND: f32 = 1.5;

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// How far along the line from `focus` to `wanted` the trunks leave the
/// line of sight clear, as a fraction of it: 1 if none is in the way,
/// otherwise a point just in front of the first trunk.
///
/// Unlike a wall ([`clear_reach`]), the result is continuous in where the
/// trunks stand: a trunk fully blocks only on the line itself, and its
/// effect fades out with the line's distance from it, its height above its
/// top or below its foot, and its nearness to the rider. Riding past a row of trees therefore
/// never makes the camera jump. Crowns do not block at all: pulling in under
/// every crown of a monte would keep the camera on the rider's shoulder.
pub fn trunk_reach(focus: Vec3, wanted: Vec3, trunks: &[Trunk]) -> f32 {
    let (from, to) = (
        GroundVec2::new(focus.x, -focus.z),
        GroundVec2::new(wanted.x, -wanted.z),
    );
    let length = from.distance(to);
    if length <= f32::EPSILON {
        return 1.0;
    }
    let along = (to - from) * (1.0 / length);
    let mut reach = 1.0_f32;
    for t in trunks {
        // The point of the line nearest the trunk, and how far it passes.
        let at = (t.at - from).dot(along).clamp(0.0, length);
        let d = t.at.distance(from + along * at);
        let entry = at - (t.radius * t.radius - d * d).max(0.0).sqrt();
        let stop = ((entry - WALL_CLEARANCE) / length).max(0.0);
        if stop >= 1.0 {
            continue;
        }
        let height = focus.y + (wanted.y - focus.y) * at / length;
        let slack = smooth((d - t.radius) / TRUNK_SOFT)
            .max(smooth((height - t.top) / TRUNK_TOP_SOFT))
            .max(smooth((t.bottom - height) / TRUNK_TOP_SOFT))
            .max(1.0 - smooth((at - TRUNK_NEAR) / TRUNK_NEAR_BLEND));
        reach = reach.min(stop + (1.0 - stop) * slack);
    }
    reach
}

/// The trunks near the camera's line of sight, from the field's own trees
/// (the same ones [`FloraSite`] plants, so a tree stands where it is drawn),
/// kept per chunk while the rider is near it. The streamed obstacles hold
/// only footprints, and mix in signposts and lamps, which fade instead.
#[derive(Default)]
pub struct TrunkCache(std::collections::HashMap<ChunkCoord, Vec<Trunk>>);

impl TrunkCache {
    /// The trunks within reach of the segment from `a` to `b`.
    pub fn along(&mut self, flora: &FloraSite, world: &World, a: Vec3, b: Vec3) -> Vec<Trunk> {
        let config = world.config;
        let (lo, hi) = segment_box(a, b);
        let (first, last) = (config.chunk_of(lo), config.chunk_of(hi));
        let here = config.chunk_of(GroundVec2::new(a.x, -a.z));
        self.0
            .retain(|c, _| (c.x - here.x).abs().max((c.y - here.y).abs()) <= 2);
        let mut found = Vec::new();
        for y in first.y..=last.y {
            for x in first.x..=last.x {
                let coord = ChunkCoord::new(x, y);
                if !config.has_chunk(coord) {
                    continue;
                }
                let trunks = self.0.entry(coord).or_insert_with(|| {
                    flora
                        .trees(coord)
                        .iter()
                        .map(|t| {
                            let ground = world.terrain.height(t.at.x, t.at.y);
                            Trunk {
                                at: t.at,
                                radius: t.trunk_radius(),
                                bottom: ground - 0.5,
                                top: ground + TRUNK_TOP * t.height,
                            }
                        })
                        .collect()
                });
                found.extend(trunks.iter().filter(|t| {
                    (lo.x..=hi.x).contains(&t.at.x) && (lo.y..=hi.y).contains(&t.at.y)
                }));
            }
        }
        found
    }
}

/// How far up a town tree counts as trunk, meters over the ground. Core
/// knows no town tree's height (render scales them), so this is a flat
/// stand-in: a trimmed street tree's crown starts over 3.5 m, and a line
/// above it runs through leaves like any other.
const TOWN_TREE_TRUNK_TOP: f32 = 3.5;

/// The trunks of the trees that never come and go with a chunk: the rings,
/// groves and street trees of the towns, and the estancias' eucalyptus and
/// poplars. Each is solid, so render never fades it and the camera must
/// ease round it instead. Built once from the layouts (their positions and
/// radii are the ones collision uses) and kept per chunk, so a frame
/// only looks at the chunks the sight line crosses.
#[derive(Debug, Default)]
pub struct StaticTrunks(std::collections::HashMap<ChunkCoord, Vec<Trunk>>);

impl StaticTrunks {
    /// Every town and settlement tree, standing on `terrain`.
    pub fn build(
        config: pampa_core::WorldConfig,
        terrain: &Terrain,
        towns: &[TownLayout],
        settlements: &SettlementLayout,
    ) -> Self {
        let mut cache = Self::default();
        let mut add = |at: GroundVec2, radius: f32, height: f32| {
            let ground = terrain.height(at.x, at.y);
            cache.0.entry(config.chunk_of(at)).or_default().push(Trunk {
                at,
                radius,
                bottom: ground - 0.5,
                top: ground + height,
            });
        };
        for town in towns {
            let ring = town.trees.iter().copied();
            let shade = town.shade_trees.iter().map(|t| t.at);
            for at in ring.chain(shade) {
                add(at, pampa_core::town::TREE_TRUNK_RADIUS, TOWN_TREE_TRUNK_TOP);
            }
        }
        for tree in settlements
            .settlements
            .iter()
            .flat_map(|s| s.structures.iter())
            .filter(|s| s.kind.is_tree())
        {
            add(
                tree.footprint.center(),
                tree.trunk_radius(),
                TRUNK_TOP * tree.height,
            );
        }
        cache
    }

    /// The trunks within reach of the segment from `a` to `b`.
    pub fn along(&self, config: pampa_core::WorldConfig, a: Vec3, b: Vec3) -> Vec<Trunk> {
        let (lo, hi) = segment_box(a, b);
        let (first, last) = (config.chunk_of(lo), config.chunk_of(hi));
        let mut found = Vec::new();
        for y in first.y..=last.y {
            for x in first.x..=last.x {
                if let Some(trunks) = self.0.get(&ChunkCoord::new(x, y)) {
                    found.extend(trunks.iter().filter(|t| {
                        (lo.x..=hi.x).contains(&t.at.x) && (lo.y..=hi.y).contains(&t.at.y)
                    }));
                }
            }
        }
        found
    }
}

/// The ground box round the segment from `a` to `b`, padded by the reach of
/// a trunk's soft edge.
fn segment_box(a: Vec3, b: Vec3) -> (GroundVec2, GroundVec2) {
    let (a, b) = (GroundVec2::new(a.x, -a.z), GroundVec2::new(b.x, -b.z));
    let pad = TRUNK_SOFT + 1.0;
    (
        GroundVec2::new(a.x.min(b.x) - pad, a.y.min(b.y) - pad),
        GroundVec2::new(a.x.max(b.x) + pad, a.y.max(b.y) + pad),
    )
}

/// The next frame's [`CameraReach`]: `allowed` (the clear part of the arm)
/// at once if the camera is out farther, so it is never inside an occluder,
/// even while easing; otherwise easing out toward it with the time constant
/// [`EASE_OUT`]. An exponential approach, so the same time passed in more,
/// shorter frames ends at the same spot.
///
/// There is no ease-in: any easing in would leave the camera behind the wall
/// it is pulling in front of for a few frames.
pub fn ease_reach(current: f32, allowed: f32, dt: f32) -> f32 {
    if allowed <= current {
        return allowed;
    }
    allowed - (allowed - current) * (-dt / EASE_OUT).exp()
}

/// The camera position on the line from `focus` to `wanted` that keeps the
/// line of sight clear: `wanted` itself if nothing blocks it, otherwise a
/// point just in front of the first occluder.
#[cfg(test)]
pub fn pull_in<'a>(
    focus: Vec3,
    wanted: Vec3,
    occluders: impl IntoIterator<Item = &'a Occluder>,
) -> Vec3 {
    let reach = clear_reach(focus, wanted, occluders);
    if reach >= 1.0 {
        wanted
    } else {
        focus.lerp(wanted, reach)
    }
}

/// How far along the line from `focus` to `wanted` the line of sight stays
/// clear, as a fraction of it: 1 if nothing blocks it, otherwise just in
/// front of the first occluder.
pub fn clear_reach<'a>(
    focus: Vec3,
    wanted: Vec3,
    occluders: impl IntoIterator<Item = &'a Occluder>,
) -> f32 {
    let (from, to) = (
        GroundVec2::new(focus.x, -focus.z),
        GroundVec2::new(wanted.x, -wanted.z),
    );
    let mut first = 1.0_f32;
    for o in occluders {
        let Some((enter, exit)) = o.footprint.segment_overlap(from, to) else {
            continue;
        };
        // The line is inside the occluder where it is over its footprint
        // and between its bottom and top heights.
        let Some((low, high)) = height_band(focus.y, wanted.y, o.bottom, o.top) else {
            continue;
        };
        let (start, end) = (enter.max(low), exit.min(high));
        if start <= end {
            first = first.min(start);
        }
    }
    if first >= 1.0 {
        return 1.0;
    }
    let length = from.distance(to);
    let back_off = if length > 0.0 {
        WALL_CLEARANCE / length
    } else {
        0.0
    };
    (first - back_off).max(0.0)
}

/// Where a line whose height goes linearly from `from` (at 0) to `to` (at 1)
/// is strictly between `bottom` and `top`, as a parameter range (unbounded
/// for a level line); `None` if it never is.
fn height_band(from: f32, to: f32, bottom: f32, top: f32) -> Option<(f32, f32)> {
    let rise = to - from;
    if rise == 0.0 {
        return (from > bottom && from < top).then_some((f32::NEG_INFINITY, f32::INFINITY));
    }
    let (a, b) = ((bottom - from) / rise, (top - from) / rise);
    Some((a.min(b), a.max(b)))
}

pub struct FollowCameraPlugin;

impl Plugin for FollowCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraOccluders>()
            .init_resource::<CameraTuning>()
            .init_resource::<CameraInput>()
            .add_systems(Startup, spawn_camera)
            .add_systems(Update, grab_cursor)
            .init_resource::<RideView>()
            .init_resource::<SeatLook>()
            .init_resource::<HudHint>()
            .init_resource::<ViewMode>()
            .init_resource::<FirstPersonLook>()
            .init_resource::<ViewBlend>()
            .init_resource::<EyeFollow>()
            .init_resource::<BodyTooClose>()
            .init_resource::<CloseShadows>()
            .add_systems(
                Update,
                (
                    gather_camera_input,
                    cycle_view,
                    toggle_view,
                    turn_to_view,
                    follow_player,
                )
                    .chain()
                    .after(DrivePlayer),
            )
            .add_systems(
                PostUpdate,
                place_first_person
                    .after(TransformSystems::Propagate)
                    .before(VisibilitySystems::UpdateFrusta)
                    .before(SimulationLightSystems::AssignLightsToClusters)
                    .before(SimulationLightSystems::UpdateDirectionalLightCascades),
            );
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Name::new("Follow camera"),
        Camera3d::default(),
        Transform::from_xyz(0.0, 4.0, 9.0).looking_at(Vec3::Y, Vec3::Y),
        FollowCamera::default(),
        CameraRig::default(),
        CameraReach::default(),
        // The player's own head too, until the first person drops it.
        world_layers(),
    ));
}

/// Locks and hides the cursor while the window has focus, so the mouse
/// orbits the camera; frees it when the player switches away.
fn grab_cursor(mut windows: Query<(&Window, &mut CursorOptions), With<PrimaryWindow>>) {
    for (window, mut cursor) in &mut windows {
        let (grab_mode, visible) = if window.focused {
            (CursorGrabMode::Locked, false)
        } else {
            (CursorGrabMode::None, true)
        };
        if cursor.grab_mode != grab_mode || cursor.visible != visible {
            cursor.grab_mode = grab_mode;
            cursor.visible = visible;
        }
    }
}

/// Maps this frame's devices to camera input: mouse motion only while the
/// cursor is grabbed, Q/E to orbit, the wheel to zoom.
pub fn read_camera_input(
    keys: &ButtonInput<KeyCode>,
    mouse_motion: Vec2,
    scroll: Option<&AccumulatedMouseScroll>,
    grabbed: bool,
) -> OrbitInput {
    let look = if grabbed {
        (mouse_motion.x, mouse_motion.y)
    } else {
        (0.0, 0.0)
    };
    let zoom = scroll.map_or(0.0, |s| match s.unit {
        MouseScrollUnit::Line => s.delta.y,
        MouseScrollUnit::Pixel => s.delta.y / PIXELS_PER_LINE,
    });
    OrbitInput {
        look,
        keys: f32::from(u8::from(keys.pressed(KeyCode::KeyE)))
            - f32::from(u8::from(keys.pressed(KeyCode::KeyQ))),
        zoom,
    }
}

/// This frame's camera input, gathered from the devices.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct CameraInput(pub OrbitInput);

fn gather_camera_input(
    keys: Res<ButtonInput<KeyCode>>,
    motion: Option<Res<AccumulatedMouseMotion>>,
    scroll: Option<Res<AccumulatedMouseScroll>>,
    cursors: Query<&CursorOptions, With<PrimaryWindow>>,
    mut input: ResMut<CameraInput>,
) {
    let grabbed = cursors.iter().any(|c| c.grab_mode != CursorGrabMode::None);
    input.0 = read_camera_input(
        &keys,
        motion.map_or(Vec2::ZERO, |m| m.delta),
        scroll.as_deref(),
        grabbed,
    );
}

/// `V` aboard cycles the riding views; boarding starts at the window. The
/// seated head turns forward whenever the seat is taken: on boarding, when
/// a skipped ride comes in, and on `V` back to the window. The HUD hint
/// names the view while aboard.
fn cycle_view(
    keys: Res<ButtonInput<KeyCode>>,
    aboard: Query<(), (With<Player>, With<OnTrain>)>,
    trip: Option<Res<Trip>>,
    (mut view, mut look): (ResMut<RideView>, ResMut<SeatLook>),
    mut hint: ResMut<HudHint>,
    mut was: Local<(bool, Option<TripPhase>)>,
) {
    let on = !aboard.is_empty();
    let phase = trip.and_then(|t| t.0);
    if on && !was.0 {
        *view = RideView::Window;
        *look = SeatLook::default();
    }
    // Only a skipped ride's jump at black lands in `Arriving`, straight
    // from `Fading`; an ordinary arrival keeps the head where it is turned.
    let skipped_in = phase == Some(TripPhase::Arriving) && was.1 == Some(TripPhase::Fading);
    if on && skipped_in {
        *look = SeatLook::default();
    }
    *was = (on, phase);
    if on && keys.just_pressed(VIEW_KEY) {
        *view = view.next();
        if *view == RideView::Window {
            *look = SeatLook::default();
        }
    }
    hint.set_if_neq(HudHint(on.then(|| view.hint())));
}

/// What the riding views read.
type RideCamera<'w> = (Res<'w, RideView>, ResMut<'w, SeatLook>, Res<'w, ViewMode>);

#[allow(clippy::too_many_arguments)]
fn follow_player(
    time: Res<Time>,
    world: Res<GameWorld>,
    occluders: Res<CameraOccluders>,
    trains: Option<Res<WorldTrains>>,
    tuning: Res<CameraTuning>,
    input: Res<CameraInput>,
    sight: Option<ResMut<SightLine>>,
    flora: Option<Res<WorldFlora>>,
    (towns, settlements): (Option<Res<WorldTowns>>, Option<Res<WorldSettlements>>),
    player: Single<(&Player, Option<&OnTrain>)>,
    (view, mut look, mode): RideCamera,
    (mut was, mut trunks, mut solid): (
        Local<Option<RideView>>,
        Local<TrunkCache>,
        Local<Option<StaticTrunks>>,
    ),
    camera: Single<(
        &mut Transform,
        &FollowCamera,
        &mut CameraRig,
        &mut CameraReach,
    )>,
) {
    let (player, ride) = player.into_inner();
    let aboard = ride.is_some();
    let rider = &player.rider;
    let (mut transform, follow, mut rig, mut reach) = camera.into_inner();
    // Boarding, getting off or a new view: start again behind the player.
    let now = aboard.then_some(*view);
    if now != *was {
        *was = now;
        rig.0 = None;
    }
    let dt = time.delta_secs();
    // The train the player rides.
    let ridden = ride.and_then(|ride| trains.as_deref()?.0.get(ride.0.train));
    if let (Some(ride), Some(train)) = (ride, ridden) {
        let cars = train.placed_cars();
        match *view {
            RideView::Window => {
                if let Some(car) = cars.get(ride.0.car) {
                    look.update(input.0, &tuning.0, dt);
                    let placed = car_transform(car, &world.0.terrain);
                    *transform = seat_view(&placed, car.length, ride.0.side, *look);
                    if let Some(mut sight) = sight {
                        sight.0 = None;
                    }
                    return;
                }
            }
            RideView::Train => {
                if let Some((spot, at)) = train_view(&cars, &world.0.terrain) {
                    // Slowly following; jumps (a skip) cut straight there.
                    if rig.0.is_none() || transform.translation.distance(spot) > 150.0 {
                        transform.translation = spot;
                        rig.0 = Some((Orbit::behind(0.0), at));
                    } else {
                        transform.translation.smooth_nudge(&spot, 1.5, dt);
                    }
                    transform.look_at(at, Vec3::Y);
                    if let Some(mut sight) = sight {
                        sight.0 = None;
                    }
                    return;
                }
            }
            RideView::Coach => {}
        }
    }
    let feet = world_from_ground(rider.position, rider.height);
    let look_height = match rider.travel() {
        Travel::OnFoot => follow.on_foot_look_height,
        Travel::Riding => follow.look_height,
    };
    let target = feet + Vec3::Y * look_height;
    // From the eyes, the camera input turns the head, not the orbit.
    let first = *mode == ViewMode::FirstPerson;
    let input = if first {
        OrbitInput::default()
    } else {
        input.0
    };

    let (orbit, focus, snapped) = match rig.0.as_mut() {
        Some((orbit, focus)) if focus.distance(target) <= follow.snap_distance => {
            orbit.update(input, rider.heading.radians(), rider.speed, &tuning.0, dt);
            focus.smooth_nudge(&target, follow.smoothing, dt);
            (*orbit, *focus, false)
        }
        // First frame or a respawn far away: jump right behind the rider.
        _ => {
            let mut orbit = Orbit::behind(rider.heading.radians());
            if aboard {
                orbit.distance = RIDE_DISTANCE;
            }
            let (orbit, focus) = *rig.0.insert((orbit, target));
            (orbit, focus, true)
        }
    };
    let desired = follow.desired(focus, &orbit, &world.0);
    // Keep the line of sight to the rider itself clear, of buildings and of
    // the cars of every train near it (but not the train the player rides).
    let cars = trains
        .as_deref()
        .filter(|_| !aboard)
        .map(|t| {
            let cars: Vec<_> = t.0.iter().flat_map(LineTrain::placed_cars).collect();
            car_occluders(&cars, &world.0.terrain, rider.position)
        })
        .unwrap_or_default();
    // And of the trunks of the trees round it: it eases in round them.
    let mut trunks = flora
        .as_deref()
        .map(|f| trunks.along(&f.0, &world.0, target, desired))
        .unwrap_or_default();
    // Town and estancia trees are solid: the camera eases round them too.
    // Their layouts rarely change, so the trunks are gathered once and
    // gathered again whenever a layout is added or replaced.
    if towns.as_ref().is_some_and(|t| t.is_changed())
        || settlements.as_ref().is_some_and(|s| s.is_changed())
    {
        *solid = None;
    }
    if towns.is_some() || settlements.is_some() {
        let solid = solid.get_or_insert_with(|| {
            let none = SettlementLayout::default();
            let towns = towns.as_deref().map_or(&[][..], |t| &t.0);
            let settlements = settlements.as_deref().map_or(&none, |s| &s.0);
            StaticTrunks::build(world.0.config, &world.0.terrain, towns, settlements)
        });
        trunks.extend(solid.along(world.0.config, target, desired));
    }
    let allowed = clear_reach(target, desired, occluders.0.iter().chain(&cars))
        .min(trunk_reach(target, desired, &trunks));
    reach.0 = if snapped {
        allowed
    } else {
        ease_reach(reach.0, allowed, dt)
    };
    transform.translation = if reach.0 >= 1.0 {
        desired
    } else {
        target.lerp(desired, reach.0)
    };
    transform.look_at(focus, Vec3::Y);
    // Fences and signs in the way fade instead (render); nothing stands
    // between the eyes and the player.
    if let Some(mut sight) = sight {
        sight.0 = (!first).then_some((target, transform.translation));
    }
}

impl FollowCamera {
    /// Unobstructed camera spot on `orbit` around the look-at point
    /// `focus`, kept above the ground (pass the world, so platforms count).
    pub fn desired(&self, focus: Vec3, orbit: &Orbit, ground: &(impl Ground + ?Sized)) -> Vec3 {
        let (offset, height) = orbit.offset();
        let mut desired = focus + world_from_ground(offset, height);
        // Keep the camera from sinking into a rise or a platform behind
        // the rider.
        let below = GroundVec2::new(desired.x, -desired.z);
        let floor = ground.ground_height(below) + self.ground_clearance;
        desired.y = desired.y.max(floor);
        desired
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orbit::angle_between;
    use crate::player::{GameWorld, PlayerPlugin, PlayerSpawn};
    use bevy::time::TimeUpdateStrategy;
    use pampa_core::{Heading, World};
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};
    use std::time::Duration;

    const FRAME: f32 = 0.016;

    fn ground(p: Vec3) -> GroundVec2 {
        GroundVec2::new(p.x, -p.z)
    }

    fn headless_app(world: World, spawn: PlayerSpawn, occluders: CameraOccluders) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
                FRAME,
            )))
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(GameWorld(world))
            .insert_resource(spawn)
            .insert_resource(occluders)
            .add_plugins((PlayerPlugin, FollowCameraPlugin));
        app
    }

    fn camera(app: &mut App) -> Transform {
        let mut query = app
            .world_mut()
            .query_filtered::<&Transform, With<FollowCamera>>();
        *query.single(app.world()).unwrap()
    }

    fn rig(app: &mut App) -> (Orbit, Vec3) {
        let mut query = app.world_mut().query::<&CameraRig>();
        query.single(app.world()).unwrap().0.expect("placed")
    }

    fn set_yaw(app: &mut App, yaw: f32) {
        let mut query = app.world_mut().query::<&mut CameraRig>();
        let mut rig = query.single_mut(app.world_mut()).unwrap();
        if let Some((orbit, _)) = rig.0.as_mut() {
            orbit.yaw = yaw;
        }
    }

    fn rider(app: &mut App) -> pampa_core::Rider {
        let mut query = app.world_mut().query::<&Player>();
        query.single(app.world()).unwrap().rider
    }

    fn keys(app: &mut App) -> Mut<'_, ButtonInput<KeyCode>> {
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>()
    }

    fn north_at(x: f32, y: f32) -> PlayerSpawn {
        PlayerSpawn {
            position: GroundVec2::new(x, y),
            heading: Heading::from_radians(FRAC_PI_2),
            ..PlayerSpawn::default()
        }
    }

    /// Ground angle from the rider to the camera.
    fn camera_bearing(app: &mut App) -> f32 {
        let r = rider(app);
        let d = ground(camera(app).translation) - r.position;
        d.y.atan2(d.x)
    }

    #[test]
    fn snaps_behind_the_rider_when_far_away() {
        // The camera starts near the origin; the rider is 160 m away.
        let mut app = headless_app(
            World::default(),
            north_at(26.0, -161.0),
            CameraOccluders::default(),
        );
        app.update();
        let r = rider(&mut app);
        let follow = FollowCamera::default();
        let focus = world_from_ground(r.position, r.height) + Vec3::Y * follow.look_height;
        let world = app.world().resource::<GameWorld>().0.clone();
        let behind = Orbit::behind(r.heading.radians());
        let expected = follow.desired(focus, &behind, &world);
        let at = camera(&mut app).translation;
        assert!(at.distance(expected) < 1e-4, "{at} vs {expected}");
        assert!(at.z > focus.z, "south of a rider facing north");
    }

    #[test]
    fn turning_in_place_leaves_the_orbit_alone() {
        let mut app = headless_app(
            World::default(),
            north_at(0.0, 0.0),
            CameraOccluders::default(),
        );
        app.update();
        let before = camera(&mut app).translation;
        keys(&mut app).press(KeyCode::KeyA);
        for _ in 0..100 {
            app.update();
        }
        assert!(rider(&mut app).heading.radians() > FRAC_PI_2 + 1.0);
        let after = camera(&mut app).translation;
        assert!(after.distance(before) < 1e-3, "{before} -> {after}");
    }

    #[test]
    fn q_and_e_orbit_around_the_rider() {
        let mut app = headless_app(
            World::default(),
            north_at(0.0, 0.0),
            CameraOccluders::default(),
        );
        app.update();
        let start = camera_bearing(&mut app);
        keys(&mut app).press(KeyCode::KeyQ);
        for _ in 0..30 {
            app.update();
        }
        let q = angle_between(start, camera_bearing(&mut app));
        let rate = OrbitParams::default().key_rate;
        assert!((q - rate * 30.0 * FRAME).abs() < 0.05, "Q swung {q}");
        keys(&mut app).release(KeyCode::KeyQ);
        keys(&mut app).press(KeyCode::KeyE);
        for _ in 0..30 {
            app.update();
        }
        let e = angle_between(start, camera_bearing(&mut app));
        assert!(e.abs() < 0.05, "E swings back: {e}");
    }

    #[test]
    fn eases_back_behind_a_moving_rider_after_idling() {
        let mut app = headless_app(
            World::default(),
            north_at(0.0, 0.0),
            CameraOccluders::default(),
        );
        app.update();
        set_yaw(&mut app, 0.0); // East of a rider facing north.
        keys(&mut app).press(KeyCode::KeyW);
        for _ in 0..60 {
            app.update();
        }
        let early = camera_bearing(&mut app);
        assert!(early.abs() < 0.05, "still to the side after 1 s: {early}");
        for _ in 0..300 {
            app.update();
        }
        let r = rider(&mut app);
        let behind = r.heading.radians() + PI;
        let late = angle_between(behind, camera_bearing(&mut app));
        assert!(late.abs() < 0.1, "behind the rider after 6 s: {late}");
    }

    #[test]
    fn mouse_moves_only_a_grabbed_cursor_and_keys_and_wheel_always_work() {
        let mut keys = ButtonInput::<KeyCode>::default();
        let motion = Vec2::new(12.0, -3.0);
        assert_eq!(
            read_camera_input(&keys, motion, None, false),
            OrbitInput::default()
        );
        let input = read_camera_input(&keys, motion, None, true);
        assert_eq!(input.look, (12.0, -3.0));
        keys.press(KeyCode::KeyE);
        assert_eq!(read_camera_input(&keys, Vec2::ZERO, None, false).keys, 1.0);
        keys.press(KeyCode::KeyQ);
        assert_eq!(read_camera_input(&keys, Vec2::ZERO, None, false).keys, 0.0);
        let lines = AccumulatedMouseScroll {
            unit: MouseScrollUnit::Line,
            delta: Vec2::new(0.0, 2.0),
        };
        assert_eq!(
            read_camera_input(&keys, Vec2::ZERO, Some(&lines), false).zoom,
            2.0
        );
        let pixels = AccumulatedMouseScroll {
            unit: MouseScrollUnit::Pixel,
            delta: Vec2::new(0.0, -50.0),
        };
        let zoom = read_camera_input(&keys, Vec2::ZERO, Some(&pixels), false).zoom;
        assert!((zoom + 50.0 / PIXELS_PER_LINE).abs() < 1e-6);
    }

    #[test]
    fn the_orbit_yaw_turns_the_camera_round_the_rider() {
        let mut app = headless_app(
            World::default(),
            north_at(0.0, 0.0),
            CameraOccluders::default(),
        );
        app.update();
        let heading = rider(&mut app).heading;
        for k in 0..8 {
            // Camera bearing such that the rider faces k * 45 degrees
            // counter-clockwise from the camera's view direction.
            let yaw = heading.radians() + PI - k as f32 * FRAC_PI_4;
            set_yaw(&mut app, yaw);
            app.update();
            // The camera looks along the ground bearing `yaw + PI`: the
            // rider's heading turned by `k` eighths of a turn.
            let forward = camera(&mut app).forward();
            let camera_yaw = (-forward.z).atan2(forward.x);
            let relative = (heading.radians() - camera_yaw).rem_euclid(std::f32::consts::TAU);
            let eighths = (relative / FRAC_PI_4).round() as usize % 8;
            assert_eq!(eighths, k, "{relative}");
        }
    }

    #[test]
    fn pulls_in_in_front_of_a_building_behind_the_rider() {
        // A tall wall 4 to 7 m south of a rider facing north.
        let wall = Occluder::column(
            Aabb::new(GroundVec2::new(-5.0, -7.0), GroundVec2::new(5.0, -4.0)),
            -10.0,
            50.0,
        );
        let mut app = headless_app(
            World::default(),
            north_at(0.0, 0.0),
            CameraOccluders(vec![wall]),
        );
        app.update();
        let at = ground(camera(&mut app).translation);
        assert!(
            !wall.footprint.contains_point(at),
            "inside the wall at {at:?}"
        );
        assert!(at.y > -4.0 && at.y < 0.0, "between wall and rider: {at:?}");
    }

    /// A fence 6 m south of a rider at the origin facing north, drawn as one
    /// fading part: thin, so it fades instead of moving the camera.
    fn fence_app(walls: CameraOccluders) -> (App, Entity) {
        let mut app = headless_app(World::default(), north_at(0.0, 0.0), walls);
        app.init_resource::<Assets<StandardMaterial>>()
            .add_plugins(pampa_render::ThinFadePlugin);
        let material = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let ground = World::default().terrain.height(0.0, -6.0);
        let part = app
            .world_mut()
            .spawn((
                pampa_render::ThinOccluder {
                    a: GroundVec2::new(-4.0, -6.0),
                    b: GroundVec2::new(4.0, -6.0),
                    radius: 0.4,
                    bottom: ground - 0.5,
                    top: ground + 7.0,
                },
                MeshMaterial3d(material),
            ))
            .id();
        (app, part)
    }

    #[test]
    fn a_fence_behind_the_rider_fades_instead_of_pulling_the_camera_in() {
        let (mut app, fence) = fence_app(CameraOccluders::default());
        for _ in 0..3 {
            app.update();
        }
        let r = rider(&mut app);
        let follow = FollowCamera::default();
        let (orbit, focus) = rig(&mut app);
        let world = app.world().resource::<GameWorld>().0.clone();
        let full = follow.desired(focus, &orbit, &world);
        let at = camera(&mut app).translation;
        assert!(at.distance(full) < 1e-3, "pulled in: {at} vs {full}");
        // The camera is south of the fence, which stands in the view.
        assert!(ground(at).y < -6.0, "{at}");
        assert!(r.position.y > -6.0);
        assert!(app.world().get::<pampa_render::Faded>(fence).is_some());
    }

    #[test]
    fn a_building_still_pulls_the_camera_in_past_a_faded_fence() {
        let wall = Occluder::column(
            Aabb::new(GroundVec2::new(-5.0, -11.0), GroundVec2::new(5.0, -8.0)),
            -10.0,
            50.0,
        );
        let (mut app, fence) = fence_app(CameraOccluders(vec![wall]));
        for _ in 0..3 {
            app.update();
        }
        let at = ground(camera(&mut app).translation);
        assert!(at.y > -8.0 && at.y < 0.0, "in front of the wall: {at:?}");
        assert!(!wall.footprint.contains_point(at));
        // Between the wall and the rider, the fence is still in the way.
        assert!(at.y < -6.0, "{at:?}");
        assert!(app.world().get::<pampa_render::Faded>(fence).is_some());
    }

    /// A caldén of the field standing alone: no other tree within 40 m.
    fn lone_tree(flora: &FloraSite) -> pampa_core::Tree {
        let trees: Vec<pampa_core::Tree> = (-19..1)
            .flat_map(|x| (3..19).map(move |y| ChunkCoord::new(x, y)))
            .flat_map(|c| flora.trees(c))
            .collect();
        *trees
            .iter()
            .find(|t| {
                t.species == pampa_core::Species::Calden
                    && t.height > 6.0
                    && trees
                        .iter()
                        .all(|o| std::ptr::eq(o, *t) || o.at.distance(t.at) > 40.0)
            })
            .expect("a lone caldén")
    }

    fn flora_site() -> FloraSite {
        let world = World::default();
        FloraSite::new(
            world.config,
            &world.terrain,
            &pampa_core::RailLine::default(),
        )
    }

    /// An app with the field's trees, the rider 6 m north of `tree` facing
    /// north (away from it), so the tree stands behind the rider, where the
    /// camera goes.
    fn grove_app(tree: &pampa_core::Tree) -> App {
        let mut app = headless_app(
            World::default(),
            north_at(tree.at.x, tree.at.y + 6.0),
            CameraOccluders::default(),
        );
        app.insert_resource(pampa_render::WorldFlora(flora_site()));
        app
    }

    #[test]
    fn a_tree_behind_the_rider_pulls_the_camera_in_to_clear_its_trunk() {
        let tree = lone_tree(&flora_site());
        let mut app = grove_app(&tree);
        for _ in 0..3 {
            app.update();
        }
        let follow = FollowCamera::default();
        let (orbit, focus) = rig(&mut app);
        let world = app.world().resource::<GameWorld>().0.clone();
        let full = follow.desired(focus, &orbit, &world);
        let at = camera(&mut app).translation;
        // Pulled in short of where it wants to be, in front of the tree.
        assert!(
            at.distance(focus) < full.distance(focus) - 1.0,
            "{at} {full}"
        );
        assert!(
            ground(at).y > tree.at.y,
            "the camera is past the tree: {at}"
        );
        // The line from the rider's head to the camera clears the trunk.
        let clearance = |c: GroundVec2| {
            let (a, b) = (ground(focus), ground(at));
            let t = ((c - a).dot(b - a) / (b - a).dot(b - a)).clamp(0.0, 1.0);
            c.distance(a + (b - a) * t)
        };
        assert!(
            clearance(tree.at) > tree.trunk_radius(),
            "{}",
            clearance(tree.at)
        );
    }

    fn trunk(x: f32, y: f32) -> Trunk {
        Trunk {
            at: GroundVec2::new(x, y),
            radius: 0.4,
            bottom: -0.5,
            top: 4.0,
        }
    }

    #[test]
    fn a_trunk_on_the_line_stops_the_camera_short_of_it_and_a_crown_does_not() {
        // Rider at the origin looking north, the camera 10 m south of it
        // and 3 m up (z is south).
        let (head, camera) = (Vec3::new(0.0, 2.0, 0.0), Vec3::new(0.0, 5.0, 10.0));
        assert_eq!(trunk_reach(head, camera, &[]), 1.0);
        // A trunk 6 m behind the rider, on the line: the camera stops in
        // front of it, 6 m out of the 10 m arm.
        let on_line = trunk_reach(head, camera, &[trunk(0.0, -6.0)]);
        assert!(on_line < 0.6, "{on_line}");
        // Over the top of the trunk's limbs, the line runs through leaves.
        let high = Trunk {
            top: 1.0,
            ..trunk(0.0, -6.0)
        };
        assert_eq!(trunk_reach(head, camera, &[high]), 1.0);
        // Well to the side, or beyond the camera, or behind the rider.
        assert_eq!(trunk_reach(head, camera, &[trunk(3.0, -6.0)]), 1.0);
        assert_eq!(trunk_reach(head, camera, &[trunk(0.0, -14.0)]), 1.0);
        assert_eq!(trunk_reach(head, camera, &[trunk(0.0, 6.0)]), 1.0);
    }

    #[test]
    fn a_line_passing_under_a_trunks_base_is_not_blocked_by_it() {
        // The line at the trunk is about 3.8 m up. A trunk standing on a
        // rise, its base above that, is passed under (through the rise,
        // which is the terrain's to stop); one whose base is below it blocks.
        let (head, camera) = (Vec3::new(0.0, 2.0, 0.0), Vec3::new(0.0, 5.0, 10.0));
        let on_rise = Trunk {
            bottom: 6.0,
            top: 12.0,
            ..trunk(0.0, -6.0)
        };
        assert_eq!(trunk_reach(head, camera, &[on_rise]), 1.0);
        let standing = Trunk {
            bottom: 1.0,
            ..on_rise
        };
        assert!(trunk_reach(head, camera, &[standing]) < 0.6);
    }

    /// A rider 6 m west of `tree` and a camera 4 m east of it, 3 m up.
    fn behind_a_tree(tree: GroundVec2, ground: f32) -> (Vec3, Vec3) {
        (
            Vec3::new(tree.x - 6.0, ground + 1.6, -tree.y),
            Vec3::new(tree.x + 4.0, ground + 4.0, -tree.y),
        )
    }

    #[test]
    fn a_plaza_or_street_tree_pulls_the_camera_in_like_a_field_tree() {
        let town = TownLayout::castex_inspired();
        let world = World::with_town(Terrain::default(), &town);
        let solid = StaticTrunks::build(
            world.config,
            &world.terrain,
            std::slice::from_ref(&town),
            &SettlementLayout::default(),
        );
        assert!(!town.trees.is_empty() && !town.shade_trees.is_empty());
        let ring = town.trees[0];
        let shade = town.shade_trees[0].at;
        for tree in [ring, shade] {
            let ground = world.terrain.height(tree.x, tree.y);
            let (head, camera) = behind_a_tree(tree, ground);
            let trunks = solid.along(world.config, head, camera);
            let reach = trunk_reach(head, camera, &trunks);
            assert!(reach < 0.8, "{tree:?}: {reach}");
            // Far from every tree the set is empty and the view clear.
            let far = Vec3::new(head.x, head.y, head.z + 400.0);
            assert!(solid.along(world.config, far, far + Vec3::X).is_empty());
        }
    }

    /// A hut settlement whose only structure is an 18 m eucalyptus at `at`.
    fn eucalyptus_at(at: GroundVec2) -> SettlementLayout {
        let spot = Aabb::from_center_size(at, GroundVec2::new(1.0, 1.0));
        let mut settlements = SettlementLayout::default();
        settlements.settlements.push(pampa_core::Settlement {
            kind: pampa_core::SettlementKind::Hut,
            gate: 0,
            hub: spot.center(),
            patio: spot,
            structures: vec![pampa_core::Structure {
                kind: StructureKind::Eucalyptus,
                footprint: spot,
                height: 18.0,
                front: pampa_core::Side::South,
                variant: 0,
            }],
            tracks: Vec::new(),
            bounds: spot,
            seed: 0,
        });
        settlements
    }

    #[test]
    fn an_estancia_tree_pulls_the_camera_in() {
        let settlements = eucalyptus_at(GroundVec2::new(30.0, 40.0));
        let world = World::default();
        let solid = StaticTrunks::build(world.config, &world.terrain, &[], &settlements);
        let tree = settlements
            .settlements
            .iter()
            .flat_map(|s| s.structures.iter())
            .find(|s| s.kind.is_tree())
            .expect("an estancia tree");
        let at = tree.footprint.center();
        let ground = world.terrain.height(at.x, at.y);
        let (head, camera) = behind_a_tree(at, ground);
        let reach = trunk_reach(head, camera, &solid.along(world.config, head, camera));
        assert!(reach < 0.8, "{reach}");
    }

    #[test]
    fn estancias_laid_out_after_the_towns_still_pull_the_camera_in() {
        // The towns are there first; the estancias arrive a few frames
        // later, with a eucalyptus between the rider and the camera.
        let tree = GroundVec2::new(30.0, 40.0);
        let mut app = headless_app(
            World::default(),
            north_at(tree.x, tree.y + 6.0),
            CameraOccluders::default(),
        );
        app.insert_resource(WorldTowns(Vec::new()));
        for _ in 0..3 {
            app.update();
        }
        app.insert_resource(WorldSettlements(eucalyptus_at(tree)));
        for _ in 0..3 {
            app.update();
        }
        let follow = FollowCamera::default();
        let (orbit, focus) = rig(&mut app);
        let world = app.world().resource::<GameWorld>().0.clone();
        let full = follow.desired(focus, &orbit, &world);
        let at = camera(&mut app).translation;
        assert!(
            at.distance(focus) < full.distance(focus) - 1.0,
            "{at} {full}"
        );
    }

    #[test]
    fn riding_past_a_row_of_trees_never_makes_the_camera_jump() {
        // Trunks every 5 m along a line, the rider crossing them 0.05 m at
        // a time, the camera 8 m behind and 3 m up: the pulled-in reach
        // changes a little each step, never by a jump.
        let row: Vec<Trunk> = (0..6)
            .map(|i| trunk(-12.0 + i as f32 * 5.0, -5.0))
            .collect();
        let (mut last, mut worst, mut lowest) = (None::<f32>, 0.0_f32, 1.0_f32);
        let mut x = -15.0;
        while x < 15.0 {
            let (head, camera) = (Vec3::new(x, 2.0, 0.0), Vec3::new(x, 5.0, 8.0));
            let reach = trunk_reach(head, camera, &row);
            if let Some(last) = last {
                worst = worst.max((reach - last).abs());
            }
            (last, lowest) = (Some(reach), lowest.min(reach));
            x += 0.05;
        }
        assert!(lowest < 0.8, "the camera never pulled in: {lowest}");
        assert!(worst < 0.1, "jumped by {worst} in one step");
    }

    #[test]
    fn looks_over_a_low_building() {
        let shed = Occluder::column(
            Aabb::new(GroundVec2::new(-5.0, -7.0), GroundVec2::new(5.0, -4.0)),
            -20.0,
            -10.0,
        );
        let focus = Vec3::new(0.0, 1.6, 0.0);
        let wanted = Vec3::new(0.0, 4.0, 9.0);
        assert_eq!(pull_in(focus, wanted, &[shed]), wanted);
    }

    fn assert_outside<'a>(
        occluders: impl IntoIterator<Item = &'a Occluder>,
        cam: Vec3,
        when: &str,
    ) {
        for o in occluders {
            assert!(!o.contains(cam), "{when}: camera {cam} inside {o:?}");
        }
    }

    fn town_app() -> (App, CameraOccluders, Aabb) {
        let town = TownLayout::castex_inspired();
        let world = World::with_town(Terrain::default(), &town);
        let occluders = CameraOccluders::from_town(&town, &world.terrain);
        // In the street, back to the general store's front, facing the plaza.
        let store = town
            .buildings
            .iter()
            .find(|b| b.kind == pampa_core::BuildingKind::GeneralStore)
            .unwrap()
            .footprint;
        let spawn = PlayerSpawn {
            position: GroundVec2::new(store.max.x + 2.5, store.center().y),
            heading: Heading::from_radians(0.0),
            ..PlayerSpawn::default()
        };
        (
            headless_app(world, spawn, occluders.clone()),
            occluders,
            store,
        )
    }

    #[test]
    fn never_ends_a_frame_inside_a_town_building() {
        let (mut app, occluders, _) = town_app();
        keys(&mut app).press(KeyCode::KeyW);
        keys(&mut app).press(KeyCode::ShiftLeft);
        for frame in 0..2400 {
            // Weave: turn left and right in long arcs through the streets,
            // and orbit the camera now and then.
            let mut k = keys(&mut app);
            let (on, off) = if (frame / 40) % 2 == 0 {
                (KeyCode::KeyA, KeyCode::KeyD)
            } else {
                (KeyCode::KeyD, KeyCode::KeyA)
            };
            k.release(off);
            k.press(on);
            if (frame / 300) % 3 == 1 {
                k.press(KeyCode::KeyE);
            } else {
                k.release(KeyCode::KeyE);
            }
            app.update();
            assert_outside(
                &occluders.0,
                camera(&mut app).translation,
                &format!("frame {frame}"),
            );
        }
    }

    #[test]
    fn never_ends_inside_a_building_at_any_orbit_angle() {
        let (mut app, occluders, _) = town_app();
        app.update();
        let mut pulled = 0;
        for step in 0..72 {
            let yaw = step as f32 * 5f32.to_radians();
            set_yaw(&mut app, yaw);
            app.update();
            let cam = camera(&mut app).translation;
            assert_outside(&occluders.0, cam, &format!("yaw {yaw}"));
            let (orbit, focus) = rig(&mut app);
            let world = app.world().resource::<GameWorld>().0.clone();
            let desired = FollowCamera::default().desired(focus, &orbit, &world);
            if pull_in(focus, desired, &occluders.0) != desired {
                pulled += 1;
            }
        }
        assert!(pulled > 0, "some angles look through the store");
    }

    #[test]
    fn walking_into_a_galpon_keeps_the_camera_off_the_player() {
        // A galpón collides at its walls, under its eaves (core's
        // `Building::collider`): walking up to its wall puts the player
        // inside its footprint, where the camera must not fold onto him.
        let town = TownLayout::castex_inspired();
        let world = World::with_town(Terrain::default(), &town);
        let occluders = CameraOccluders::from_town(&town, &world.terrain);
        let shed = town
            .buildings
            .iter()
            .find(|b| {
                matches!(
                    b.kind,
                    pampa_core::BuildingKind::Shed | pampa_core::BuildingKind::GrainStore
                )
            })
            .copied()
            .expect("Castex has a galpón");
        // Out on its street side, walking straight at its front wall.
        let (f, c) = (shed.footprint, shed.footprint.center());
        let (position, heading) = match shed.front {
            pampa_core::Side::South => (GroundVec2::new(c.x, f.min.y - 4.0), FRAC_PI_2),
            pampa_core::Side::North => (GroundVec2::new(c.x, f.max.y + 4.0), -FRAC_PI_2),
            pampa_core::Side::West => (GroundVec2::new(f.min.x - 4.0, c.y), 0.0),
            pampa_core::Side::East => (GroundVec2::new(f.max.x + 4.0, c.y), PI),
        };
        let spawn = PlayerSpawn {
            position,
            heading: Heading::from_radians(heading),
            travel: pampa_core::Travel::OnFoot,
        };
        let colliders = world_colliders(&town);
        let mut app = headless_app(world, spawn, occluders.clone());
        keys(&mut app).press(KeyCode::KeyW);
        for frame in 0..400 {
            app.update();
            let cam = camera(&mut app).translation;
            let (_, head) = rig(&mut app);
            assert!(
                cam.distance(head) >= 1.2,
                "frame {frame}: camera {cam} on the player's head {head}"
            );
            assert_outside(&occluders.0, cam, &format!("frame {frame}"));
            for c in &colliders {
                assert!(
                    !c.contains_point(ground(cam)),
                    "frame {frame}: camera in {c:?}"
                );
            }
        }
        // It got there: against the wall, under the eaves.
        let r = rider(&mut app);
        assert!(
            f.contains_point(r.position),
            "{:?} outside {f:?}",
            r.position
        );
    }

    /// Every building's collider in `town`.
    fn world_colliders(town: &TownLayout) -> Vec<Aabb> {
        town.buildings
            .iter()
            .filter(|b| b.kind != pampa_core::BuildingKind::Platform)
            .map(|b| b.collider())
            .collect()
    }

    #[test]
    fn on_foot_the_camera_looks_at_a_lower_point() {
        let mut heights = Vec::new();
        for travel in [pampa_core::Travel::Riding, pampa_core::Travel::OnFoot] {
            let spawn = PlayerSpawn {
                travel,
                ..north_at(0.0, 0.0)
            };
            let mut app = headless_app(World::default(), spawn, CameraOccluders::default());
            app.update();
            let r = rider(&mut app);
            let feet = world_from_ground(r.position, r.height);
            heights.push(rig(&mut app).1.y - feet.y);
        }
        let follow = FollowCamera::default();
        assert!(
            (heights[0] - follow.look_height).abs() < 1e-4,
            "{heights:?}"
        );
        assert!(
            (heights[1] - follow.on_foot_look_height).abs() < 1e-4,
            "{heights:?}"
        );
        assert!(follow.on_foot_look_height < follow.look_height);
    }

    #[test]
    fn on_the_platform_the_camera_stays_above_it_and_is_not_blocked_by_it() {
        let town = TownLayout::castex_inspired();
        let world = World::with_town(Terrain::default(), &town);
        let occluders = CameraOccluders::from_town(&town, &world.terrain);
        let platform = town
            .buildings
            .iter()
            .find(|b| b.kind == BuildingKind::Platform)
            .unwrap();
        // A floor, not a wall: the camera may look across it.
        assert!(
            occluders
                .0
                .iter()
                .all(|o| o.footprint != platform.footprint)
        );
        assert_eq!(
            occluders.0.len(),
            town.buildings.len() - 1 + town.canopies().len()
        );
        let (position, heading) = town.platform_spot().unwrap();
        let top = world.ground_height(position);
        let spawn = PlayerSpawn {
            position,
            heading,
            travel: pampa_core::Travel::OnFoot,
        };
        let mut app = headless_app(world.clone(), spawn, occluders.clone());
        app.update();
        // The player stands on the platform's top.
        assert_eq!(rider(&mut app).height, top);
        let follow = FollowCamera::default();
        for step in 0..72 {
            let yaw = step as f32 * 5f32.to_radians();
            set_yaw(&mut app, yaw);
            app.update();
            let cam = camera(&mut app).translation;
            assert_outside(&occluders.0, cam, &format!("yaw {yaw}"));
            // Never down in the platform or the ground beside it.
            let floor = world.ground_height(ground(cam));
            assert!(cam.y > floor + 0.5, "yaw {yaw}: {cam} over {floor}");
            let (orbit, focus) = rig(&mut app);
            let desired = follow.desired(focus, &orbit, &world);
            let under = world.ground_height(ground(desired));
            assert!(
                desired.y >= under + follow.ground_clearance - 1e-4,
                "{desired}"
            );
        }
    }

    fn flat() -> Terrain {
        Terrain {
            amplitude: 0.0,
            ..Terrain::default()
        }
    }

    /// A coach standing on flat ground at `center`, along `direction`.
    fn coach(center: GroundVec2, direction: GroundVec2) -> PlacedCar {
        PlacedCar {
            kind: pampa_core::CarKind::Coach,
            center,
            direction: direction * (1.0 / direction.length()),
            length: 15.0,
            on_rail: true,
        }
    }

    /// Where `p` is relative to `car`: along its length and across it.
    fn car_frame(car: &PlacedCar, p: GroundVec2) -> (f32, f32) {
        let d = p - car.center;
        let across = GroundVec2::new(-car.direction.y, car.direction.x);
        (d.dot(car.direction), d.dot(across))
    }

    #[test]
    fn pulls_in_in_front_of_a_train_car_beside_the_player() {
        let focus = Vec3::new(0.0, 1.6, 0.0);
        // A car running east 5 m north of the player, and one on a slant.
        for direction in [GroundVec2::new(1.0, 0.0), GroundVec2::new(1.0, 0.6)] {
            let car = coach(GroundVec2::new(0.0, 5.0), direction);
            let occluders = car_occluders(&[car], &flat(), GroundVec2::ZERO);
            assert!(!occluders.is_empty(), "{direction:?}");
            let top = LineTrain::RAIL_TOP + car.kind.height();
            assert!(occluders.iter().all(|o| (o.top - top).abs() < 1e-4));
            // The camera wants to be 12 m north, behind the car, at 3 m.
            let wanted = Vec3::new(0.0, 3.0, -12.0);
            let at = pull_in(focus, wanted, &occluders);
            assert_ne!(at, wanted, "{direction:?}: blocked by the car");
            assert_outside(&occluders, at, &format!("{direction:?}"));
            // In front of the car, on the player's side, clear of its body.
            let (along, across) = car_frame(&car, ground(at));
            assert!(along.abs() < car.length / 2.0, "{direction:?}");
            assert!(
                across < -LineTrain::WIDTH / 2.0,
                "{direction:?}: {across} across the car"
            );
        }
    }

    #[test]
    fn far_cars_are_left_out() {
        let car = coach(GroundVec2::new(0.0, 200.0), GroundVec2::new(1.0, 0.0));
        assert!(car_occluders(&[car], &flat(), GroundVec2::ZERO).is_empty());
    }

    /// A roof slab from 3.5 to 3.7 m over the 6 m north of the focus.
    fn roof() -> Occluder {
        Occluder {
            footprint: Aabb::new(GroundVec2::new(-10.0, 0.0), GroundVec2::new(10.0, 6.0)),
            bottom: 3.5,
            top: 3.7,
        }
    }

    #[test]
    fn looks_under_a_canopy_but_not_through_its_roof() {
        let focus = Vec3::new(0.0, 1.6, 0.0);
        // Under the roof and out past it, low: clear.
        let under = Vec3::new(0.0, 3.0, -9.0);
        assert_eq!(pull_in(focus, under, &[roof()]), under);
        // Up through the roof: stops under it.
        let over = Vec3::new(0.0, 7.0, -9.0);
        let at = pull_in(focus, over, &[roof()]);
        assert_ne!(at, over);
        assert!(at.y < roof().bottom, "{at}");
        assert!(!roof().contains(at));
        // From above the roof down to a camera under it, too.
        let high = Vec3::new(0.0, 6.0, 0.0);
        let low = Vec3::new(0.0, 1.0, -4.0);
        let at = pull_in(high, low, &[roof()]);
        assert!(at.y > roof().top, "{at}");
    }

    #[test]
    fn the_platform_canopy_is_a_roof_over_the_platform() {
        let town = TownLayout::castex_inspired();
        let terrain = Terrain::default();
        let occluders = CameraOccluders::from_town(&town, &terrain);
        let canopy = town.canopies()[0];
        let c = canopy.footprint.center();
        let roof = occluders
            .0
            .iter()
            .find(|o| o.footprint == canopy.footprint)
            .expect("the canopy's roof");
        let ground = terrain.height(c.x, c.y);
        assert!((roof.bottom - ground - canopy.bottom()).abs() < 1e-4);
        assert!((roof.top - ground - canopy.top()).abs() < 1e-4);
    }

    #[test]
    fn train_cars_are_looked_at_as_far_as_the_orbit_arm_reaches() {
        // The arm never reaches past the cars considered, with a little to
        // spare, and the reach is not wastefully long.
        let arm = OrbitParams::default().max_distance;
        assert!(
            CAR_REACH > arm && CAR_REACH <= arm + 10.0,
            "{CAR_REACH} vs {arm}"
        );
        let car = |d: f32| pampa_core::PlacedCar {
            kind: pampa_core::CarKind::Coach,
            center: GroundVec2::new(d, 0.0),
            direction: GroundVec2::new(0.0, 1.0),
            length: 12.0,
            on_rail: true,
        };
        let terrain = Terrain::default();
        // A car whose side is at the end of the longest arm still counts.
        assert!(!car_occluders(&[car(arm)], &terrain, GroundVec2::ZERO).is_empty());
        assert!(car_occluders(&[car(CAR_REACH + 7.0)], &terrain, GroundVec2::ZERO).is_empty());
    }

    #[test]
    fn the_canopy_roof_stands_on_the_leveled_station_ground() {
        let town = TownLayout::castex_inspired();
        let world = World::with_town(Terrain::default(), &town);
        let occluders = CameraOccluders::from_town(&town, &world.terrain);
        let canopy = town.canopies()[0];
        let station = town
            .buildings
            .iter()
            .find(|b| b.kind == BuildingKind::Station)
            .unwrap()
            .footprint
            .center();
        let roof = occluders
            .0
            .iter()
            .find(|o| o.footprint == canopy.footprint)
            .unwrap();
        // Anchored on the station ground, wherever along the canopy it is
        // measured: the station's leveled ground is flat.
        let ground = world.terrain.height(station.x, station.y);
        assert!((roof.bottom - ground - canopy.bottom()).abs() < 1e-4);
        for corner in [canopy.footprint.min, canopy.footprint.max] {
            assert!((world.terrain.height(corner.x, corner.y) - ground).abs() < 1e-4);
        }
    }

    #[test]
    fn pulls_in_at_once_and_eases_out_slowly() {
        // Blocked: straight to the allowed reach, never beyond it.
        assert_eq!(ease_reach(1.0, 0.3, 0.001), 0.3);
        assert_eq!(ease_reach(0.5, 0.3, 0.1), 0.3);
        // Cleared: on the way out, still short of it after one frame.
        let one = ease_reach(0.3, 1.0, FRAME);
        assert!(one > 0.3 && one < 0.35, "{one}");
        let mut r = 0.3;
        let mut t = 0.0;
        while r < 0.95 {
            r = ease_reach(r, 1.0, FRAME);
            t += FRAME;
        }
        assert!(t > 0.5 && t < 1.5, "out in {t} s");
        // Frame-rate independent.
        let coarse = ease_reach(0.3, 1.0, 0.1);
        let fine = (0..10).fold(0.3, |r, _| ease_reach(r, 1.0, 0.01));
        assert!((coarse - fine).abs() < 1e-5, "{coarse} vs {fine}");
        // Never past the allowed reach while easing.
        assert!((0..100).fold(0.3, |r, _| ease_reach(r, 0.8, 0.05)) <= 0.8);
    }

    #[test]
    fn eases_back_out_when_the_view_clears() {
        // A wall behind the rider pulls the camera in; once it is gone, the
        // camera glides back out over a few frames instead of jumping.
        let wall = Occluder::column(
            Aabb::new(GroundVec2::new(-5.0, -7.0), GroundVec2::new(5.0, -4.0)),
            -10.0,
            50.0,
        );
        let mut app = headless_app(
            World::default(),
            north_at(0.0, 0.0),
            CameraOccluders(vec![wall]),
        );
        app.update();
        app.update();
        let pulled = ground(camera(&mut app).translation).distance(GroundVec2::ZERO);
        app.insert_resource(CameraOccluders::default());
        app.update();
        let next = ground(camera(&mut app).translation).distance(GroundVec2::ZERO);
        for _ in 0..200 {
            app.update();
        }
        let out = ground(camera(&mut app).translation).distance(GroundVec2::ZERO);
        assert!(pulled < 4.0, "{pulled}");
        assert!(
            next > pulled && next < out - 1.0,
            "{pulled} -> {next} -> {out}"
        );
    }

    /// The game's opening (aboard, pulling into Monte Nievas) with the
    /// camera.
    fn riding_app() -> App {
        let mut app = crate::ride::testing::shipped_app();
        app.add_plugins(FollowCameraPlugin);
        app.update();
        app
    }

    /// The camera in the frame of the player's coach, and that coach.
    fn in_coach(app: &mut App) -> (Transform, PlacedCar) {
        let train = app.world().resource::<WorldTrains>().0[0].clone();
        let (_, ride) = crate::ride::testing::passenger(app);
        let car = train.placed_cars()[ride.expect("aboard").car];
        let terrain = app.world().resource::<GameWorld>().0.terrain.clone();
        let placed = car_transform(&car, &terrain);
        let cam = camera(app);
        let inverse = placed.compute_affine().inverse();
        let local = Transform::from_translation(inverse.transform_point3(cam.translation))
            .with_rotation(placed.rotation.inverse() * cam.rotation);
        (local, car)
    }

    #[test]
    fn aboard_the_camera_sits_by_a_coach_window_and_rides_with_the_train() {
        let mut app = riding_app();
        assert_eq!(*app.world().resource::<RideView>(), RideView::Window);
        let (local, car) = in_coach(&mut app);
        let p = local.translation;
        // Inside the coach's body, clear of its walls, floor and ceiling.
        let inner = CoachBody::HALF_WIDTH - CoachBody::WALL;
        assert!(p.x.abs() < car.length / 2.0 - CoachBody::END_INSET, "{p}");
        assert!(p.z.abs() < inner - 0.3, "{p}");
        assert!(
            p.y > CoachBody::FLOOR + 0.5 && p.y < CoachBody::CEILING - 0.5,
            "{p}"
        );
        // Beside a window opening, looking out of it.
        let (windows, open) = CoachBody::windows(car.length);
        assert!(windows.iter().any(|&x| (p.x - x).abs() < open / 2.0));
        assert!(p.y > CoachBody::SILL && p.y < CoachBody::LINTEL);
        // Seated facing forward, the way the train runs, the window beside.
        let ahead = local.rotation * Vec3::NEG_Z;
        assert!(ahead.dot(Vec3::X) > 0.99, "faces forward: {ahead}");
        let side = crate::ride::testing::passenger(&mut app).1.unwrap().side;
        assert!(p.z * side > 0.0, "on the platform side");
        // Rides along: the train moves, the seat stays put in the coach.
        let before = camera(&mut app).translation;
        for _ in 0..60 {
            app.update();
        }
        let (again, _) = in_coach(&mut app);
        assert!(camera(&mut app).translation.distance(before) > 5.0);
        assert!(again.translation.distance(p) < 1e-2);
        // The landscape streams around the moving train.
        let (rider, _) = crate::ride::testing::passenger(&mut app);
        let focus = app.world().resource::<pampa_render::StreamFocus>().0;
        let train = app.world().resource::<WorldTrains>().0[0].clone();
        let (_, ride) = crate::ride::testing::passenger(&mut app);
        assert_eq!(focus, train.placed_cars()[ride.unwrap().car].center);
        assert_eq!(focus, rider.position);
    }

    #[test]
    fn the_seated_look_stays_within_the_limits() {
        let params = OrbitParams::default();
        let mut look = SeatLook::default();
        for _ in 0..200 {
            look.update(
                OrbitInput {
                    look: (900.0, 900.0),
                    keys: 1.0,
                    zoom: 0.0,
                },
                &params,
                0.1,
            );
        }
        assert_eq!((look.yaw, look.pitch), (-SEAT_YAW_LIMIT, -SEAT_PITCH_LIMIT));
        for _ in 0..200 {
            look.update(
                OrbitInput {
                    look: (-900.0, -900.0),
                    keys: -1.0,
                    zoom: 0.0,
                },
                &params,
                0.1,
            );
        }
        assert_eq!((look.yaw, look.pitch), (SEAT_YAW_LIMIT, SEAT_PITCH_LIMIT));
        // Turned all the way: about 120° from facing forward (the coach's
        // +X), past either window, looking 40° up.
        let view = seat_view(&Transform::IDENTITY, 12.0, 1.0, look);
        let ahead = view.rotation * Vec3::NEG_Z;
        let flat = Vec3::new(ahead.x, 0.0, ahead.z).normalize();
        assert!((flat.dot(Vec3::X).acos() - SEAT_YAW_LIMIT).abs() < 1e-3);
        assert!((ahead.y.asin() - SEAT_PITCH_LIMIT).abs() < 1e-3);
        // Q/E turn the head in the app too, but never past the limit.
        let mut app = riding_app();
        keys(&mut app).press(KeyCode::KeyE);
        for _ in 0..200 {
            app.update();
        }
        let look = *app.world().resource::<SeatLook>();
        assert_eq!(look.yaw, -SEAT_YAW_LIMIT);
    }

    /// How squarely the seated camera faces forward along its coach.
    fn facing_forward(app: &mut App) -> f32 {
        let (local, _) = in_coach(app);
        (local.rotation * Vec3::NEG_Z).dot(Vec3::X)
    }

    /// Turns the seated head with the mouse, as a focused window with the
    /// cursor grabbed passes it on.
    fn mouse_look(app: &mut App, delta: Vec2) {
        let world = app.world_mut();
        let mut windows = world.query_filtered::<(), With<PrimaryWindow>>();
        if windows.iter(world).next().is_none() {
            world.spawn((Window::default(), CursorOptions::default(), PrimaryWindow));
        }
        app.insert_resource(AccumulatedMouseMotion { delta });
        app.update();
        app.update();
        app.insert_resource(AccumulatedMouseMotion::default());
        app.update();
    }

    fn tap(app: &mut App, key: KeyCode) {
        keys(app).press(key);
        app.update();
        let mut k = keys(app);
        k.release(key);
        k.clear();
        app.update();
    }

    #[test]
    fn every_ride_starts_seated_facing_forward_and_the_mouse_looks_round() {
        // The opening.
        let mut app = riding_app();
        assert!(facing_forward(&mut app) > 0.99, "the opening");
        // The mouse turns the head, and no further than the seat allows.
        mouse_look(&mut app, Vec2::new(40.0, 0.0));
        let look = *app.world().resource::<SeatLook>();
        assert!(look.yaw < -0.05, "turned right: {look:?}");
        assert!(facing_forward(&mut app) < 0.999);
        mouse_look(&mut app, Vec2::new(1.0e5, -1.0e5));
        let look = *app.world().resource::<SeatLook>();
        assert_eq!((look.yaw, look.pitch), (-SEAT_YAW_LIMIT, SEAT_PITCH_LIMIT));
        // Round the views with V and back to the window: forward again.
        for _ in 0..3 {
            tap(&mut app, VIEW_KEY);
        }
        assert_eq!(*app.world().resource::<RideView>(), RideView::Window);
        assert!(facing_forward(&mut app) > 0.99, "back at the window");
        // Off at Monte Nievas with the head turned, then aboard again.
        mouse_look(&mut app, Vec2::new(-60.0, 0.0));
        crate::ride::testing::arrive(&mut app);
        crate::ride::testing::press(&mut app, crate::errand::INTERACT_KEY);
        app.update();
        assert!(
            crate::ride::testing::passenger(&mut app).1.is_some(),
            "boarded"
        );
        assert!(facing_forward(&mut app) > 0.99, "a normal boarding");
        // A skipped ride comes in facing forward too.
        mouse_look(&mut app, Vec2::new(-60.0, 0.0));
        crate::ride::testing::press(&mut app, crate::ride::SKIP_KEY);
        for _ in 0..2000 {
            app.update();
            if app.world().resource::<crate::ride::Trip>().0
                == Some(crate::ride::TripPhase::Arriving)
            {
                break;
            }
        }
        app.update();
        assert!(crate::ride::testing::passenger(&mut app).1.is_some());
        assert!(facing_forward(&mut app) > 0.99, "after the skip");
    }

    #[test]
    fn an_ordinary_arrival_keeps_the_head_where_it_was_turned() {
        // The opening ridden into Monte Nievas without a skip, looking off
        // to one side the whole way.
        let mut app = riding_app();
        mouse_look(&mut app, Vec2::new(-60.0, 10.0));
        let turned = *app.world().resource::<SeatLook>();
        assert!(turned.yaw > 0.05, "{turned:?}");
        let mut frames = 0;
        while crate::ride::testing::passenger(&mut app).1.is_some() {
            assert_eq!(
                *app.world().resource::<SeatLook>(),
                turned,
                "frame {frames}"
            );
            app.update();
            frames += 1;
            assert!(frames < 4000, "never got off");
        }
        assert_eq!(
            app.world().resource::<crate::ride::Trip>().0,
            None,
            "an ordinary ride, no skip"
        );
    }

    #[test]
    fn v_cycles_the_riding_views_and_wraps() {
        let mut app = riding_app();
        let hint = |app: &App| app.world().resource::<HudHint>().0.clone();
        assert_eq!(hint(&app).as_deref(), Some("V: vista - ventana"));
        let mut seen = Vec::new();
        for _ in 0..4 {
            keys(&mut app).press(VIEW_KEY);
            app.update();
            {
                let mut k = keys(&mut app);
                k.release(VIEW_KEY);
                k.clear();
            }
            app.update();
            seen.push(*app.world().resource::<RideView>());
        }
        assert_eq!(
            seen,
            [
                RideView::Coach,
                RideView::Train,
                RideView::Window,
                RideView::Coach
            ]
        );
        assert_eq!(hint(&app).as_deref(), Some("V: vista - exterior"));
        // Outside the coach in the orbit, and off to the side of the whole
        // train in the train view.
        let (local, car) = in_coach(&mut app);
        assert!(
            local.translation.z.abs() > CoachBody::HALF_WIDTH
                || local.translation.x.abs() > car.length / 2.0
        );
        keys(&mut app).press(VIEW_KEY);
        app.update();
        keys(&mut app).release(VIEW_KEY);
        keys(&mut app).clear();
        app.update();
        assert_eq!(hint(&app).as_deref(), Some("V: vista - tren"));
        let (local, _) = in_coach(&mut app);
        assert!(local.translation.z > 30.0, "{}", local.translation);
        // Off the train: no hint.
        crate::ride::testing::arrive_from_anywhere(&mut app);
        assert_eq!(hint(&app), None);
    }

    #[test]
    fn on_the_platform_the_camera_stays_in_front_of_the_dwelling_train() {
        let loaded =
            crate::towns::load_world(&pampa_content::asset_path(pampa_content::TOWNS_FILE));
        // The opening's train, skipped on to its dwell at Monte Nievas.
        let (clock, trains, _) = crate::life::start_of_day(&loaded.line, &loaded.towns);
        let mut train = trains.into_iter().next().expect("a line train");
        train.skip_to_next_stop(clock.elapsed(), clock.rate());
        let world = World::with_towns(loaded.world, Terrain::default(), &loaded.towns);
        let occluders = CameraOccluders::from_towns(&loaded.towns, &world.terrain);
        let (position, heading) = loaded.towns[0].platform_spot().unwrap();
        let cars = train.placed_cars();
        // The car beside the player, and the bearing across the track to it.
        let beside = *cars
            .iter()
            .min_by(|a, b| {
                let d = |c: &PlacedCar| c.center.distance(position);
                d(a).total_cmp(&d(b))
            })
            .unwrap();
        let (_, across) = car_frame(&beside, position);
        let normal = GroundVec2::new(-beside.direction.y, beside.direction.x);
        let to_track = normal * -across.signum();
        let spawn = PlayerSpawn {
            position,
            heading,
            travel: pampa_core::Travel::OnFoot,
        };
        let mut app = headless_app(world.clone(), spawn, occluders.clone());
        app.insert_resource(WorldTrains(vec![train]));
        app.update();
        let yaw = to_track.y.atan2(to_track.x);
        let mut pulled = 0;
        for k in -3..=3 {
            set_yaw(&mut app, yaw + k as f32 * 0.15);
            app.update();
            let cam = camera(&mut app).translation;
            let cars = car_occluders(&cars, &world.terrain, position);
            assert_outside(&cars, cam, &format!("yaw step {k}"));
            // On the player's side of the train, with a clear view.
            let (along, side) = car_frame(&beside, ground(cam));
            assert!(
                side * across > 0.0 && side.abs() > LineTrain::WIDTH / 2.0,
                "step {k}: {along}, {side} vs the player's {across}"
            );
            let r = rider(&mut app);
            let follow = FollowCamera::default();
            let target =
                world_from_ground(r.position, r.height) + Vec3::Y * follow.on_foot_look_height;
            assert_eq!(pull_in(target, cam, &cars), cam, "step {k}");
            let (orbit, focus) = rig(&mut app);
            if cam.distance(follow.desired(focus, &orbit, &world)) > 1.0 {
                pulled += 1;
            }
        }
        assert!(pulled >= 5, "the cars pull the camera in: {pulled}");
    }
}
