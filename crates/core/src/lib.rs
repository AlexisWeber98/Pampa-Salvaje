//! Pure game logic for Pampa Seca.
//!
//! This crate must never depend on Bevy or any renderer: game logic never
//! knows how things are drawn.
//!
//! # Coordinate convention
//!
//! Logic lives on a 2D ground plane: `x` points east, `y` points north.
//! Height above the ground plane is a separate scalar sampled from
//! [`World::ground_height`]: the [`Terrain`], raised by station platforms
//! ([`Floor`]). Angles are radians, counter-clockwise from east, so a
//! positive turn is a turn to the left.

pub mod activation;
pub mod animation;
pub mod bumps;
pub mod chunk;
pub mod clock;
pub mod collision;
pub mod errand;
pub mod fence;
pub mod flora;
pub mod heading;
pub mod math;
pub mod obstacles;
pub mod placement;
pub mod places;
pub mod pose;
pub mod rail;
pub mod ride;
pub mod rider;
mod rng;
pub mod road;
pub mod terrain;
pub mod town;
pub mod townsfolk;
pub mod train;
pub mod walker;
pub mod world;

pub use activation::{ActivationChange, TownActivation, distance_to_town, nearest_town};
pub use animation::AnimationState;
pub use bumps::{BumpEvent, BumpSeverity, BumpTracker, LinePicker};
pub use chunk::{ChunkCoord, WorldConfig, WorldConfigError};
pub use clock::{BASE_RATE, DAY_REAL_SECONDS, DAY_SECONDS, GameClock, TimeOfDay};
pub use collision::{Aabb, Body, Circle, SideObstacle};
pub use errand::{Errand, ErrandEvent, ErrandStage, Interaction, Npc, Standing};
pub use fence::{FenceLayout, FenceRun, Frontage, Gate, Opening, OpeningKind};
pub use flora::{ChunkFlora, FloraSite, Species, Tree, Tuft, TuftKind};
pub use heading::Heading;
pub use math::Vec2;
pub use obstacles::StaticObstacles;
pub use placement::{PlacementError, place_towns};
pub use places::{Place, PlaceKind, Sign, SignMount};
pub use pose::{HorseAngles, HorseGait, JointAngles, Leg, MOTION_CLOCK_PERIOD, tick_clock};
pub use rail::{LinePath, RailCurve, RailLine, RailLineError, RailLink, RailSpan};
pub use ride::{
    APPROACH_LEAD, BOARD_RANGE, Boarding, ONBOARD_MULTIPLIER, OPENING_LEAD, Ride, SEAT_HEIGHT,
    WAIT_RANGE, alight_spot, boarding, coach_doors, depart_now, next_arrival, opening, skip_ride,
    skip_to_approach, stands_at, wait_for_train, waiting_station,
};
pub use rider::{MAX_STEP, Rider, RiderInput, RiderParams};
pub use road::{
    LevelCrossing, Road, RoadEnd, RoadError, RoadKind, RoadNetwork, RoadPoint, SignBoard, Signpost,
};
pub use terrain::{FlatArea, Terrain};
pub use town::{
    Building, BuildingKind, Canopy, Floor, HitchingPost, PLAZA_MONUMENT, PLAZA_PATH_CLEARANCE,
    PLAZA_PATH_WIDTH, PlazaPlacement, Railway, RailwayConfig, Side, TetheredHorse, TownConfig,
    TownConfigError, TownLayout, TownSize, plaza_monument, plaza_paths,
};
pub use townsfolk::{StreetGraph, Townsfolk, Townsperson};
pub use train::{
    Car, CarKind, LinePhase, LineTrain, LineTrainState, MAX_STOP_MISS, PlacedCar, Run, Station,
    Timetable, TimetableError,
};
pub use walker::Travel;
pub use world::{Ground, World, WorldBounds};
