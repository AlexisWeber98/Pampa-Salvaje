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

pub mod abilities;
pub mod activation;
pub mod animation;
pub mod board;
pub mod bumps;
pub mod chunk;
pub mod clock;
pub mod collision;
pub mod errand;
pub mod fauna;
pub mod fence;
pub mod flora;
pub mod heading;
pub mod lagoon;
pub mod lamps;
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
pub mod settlement;
pub mod terrain;
pub mod town;
pub mod townsfolk;
pub mod train;
pub mod walker;
pub mod world;

pub use abilities::{Abilities, Ability};
pub use activation::{ActivationChange, TownActivation, distance_to_town, nearest_town};
pub use animation::AnimationState;
pub use board::{BoardRow, Bound, Service, station_board};
pub use bumps::{BumpEvent, BumpSeverity, BumpTracker, LinePicker};
pub use chunk::{ChunkCoord, WorldConfig, WorldConfigError};
pub use clock::{BASE_RATE, DAY_REAL_SECONDS, DAY_SECONDS, GameClock, TimeOfDay};
pub use collision::{Aabb, Body, Circle, SideObstacle};
pub use errand::{Errand, ErrandEvent, ErrandStage, Interaction, Npc, Standing};
pub use fauna::{
    Cow, CowCoat, CowState, Duck, FaunaSite, Flock, FlockState, Hare, HareState, Herd, Raptor,
};
pub use fence::{FenceLayout, FenceRun, Frontage, Gate, Opening, OpeningKind};
pub use flora::{ChunkFlora, FloraSite, Shrub, ShrubKind, Species, Tree, Tuft, TuftKind};
pub use heading::Heading;
pub use lagoon::{Lagoon, LagoonConfig, LagoonError, place_lagoons};
pub use lamps::{LampSite, LampTier, StreetLamp, StreetLamps};
pub use math::Vec2;
pub use obstacles::StaticObstacles;
pub use placement::{PlacementError, place_towns};
pub use places::{
    Place, PlaceKind, STATION_WALL_INSET, Sign, SignMount, StoreStyle, TIMETABLE_BOARD_HEIGHT,
    TIMETABLE_BOARD_WIDTH, station_bays,
};
pub use pose::{HorseGait, JointAngles, MOTION_CLOCK_PERIOD, tick_clock};
pub use rail::{
    LinePath, RailBay, RailBranch, RailCurve, RailLine, RailLineError, RailLink, RailSpan,
};
pub use ride::{
    APPROACH_LEAD, BOARD_RANGE, Boarding, ONBOARD_MULTIPLIER, OPENING_LEAD, Ride, SEAT_HEIGHT,
    WAIT_RANGE, WaitingPlace, alight_spot, boarding, boarding_at, coach_doors, depart_now,
    follow_jump, next_arrival, opening, skip_ride, skip_to_approach, stands_at, wait_for_train,
    waiting_place,
};
pub use rider::{MAX_STEP, Rider, RiderInput, RiderParams};
pub use road::{
    LevelCrossing, Road, RoadEnd, RoadError, RoadKind, RoadNetwork, RoadPoint, SignBoard, Signpost,
};
pub use settlement::{
    HouseStyle, KeepOut, RoadsideWreck, Settlement, SettlementKind, SettlementLayout, Structure,
    StructureKind, Track,
};
pub use terrain::{FlatArea, Terrain};
pub use town::{
    Building, BuildingKind, Canopy, Floor, HitchingPost, PLAZA_MONUMENT, PLAZA_PATH_CLEARANCE,
    PLAZA_PATH_WIDTH, PlazaPlacement, Railway, RailwayConfig, Side, TetheredHorse, TownConfig,
    TownConfigError, TownLayout, TownSize, plaza_monument, plaza_paths,
};
pub use townsfolk::{StreetGraph, Townsfolk, Townsperson};
pub use train::{
    Car, CarKind, CoachBody, LinePhase, LineTrain, LineTrainState, MAX_STOP_MISS, PlacedCar, Run,
    SHUTTLE_MARGIN, Station, Timetable, TimetableError, shuttle_timetable,
};
pub use walker::Travel;
pub use world::{Ground, World, WorldBounds};
