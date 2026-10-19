# Feature: town-life

Locator: `odd/tasks/town-life.md` · Engram mirror: `odd/town-life/tasks` (pending, project not registered)

## Objective

Close the technical follow-ups from the Castex town review, then make the town feel alive with simple townsfolk, a tethered horse and a passing train.

## Problem / Why

- Review follow-ups:
  - The rider can tunnel through thin obstacles (0.7 m tree trunks) at `dt = MAX_STEP`.
  - The follow camera clips into buildings.
  - The camera snap branch has no test.
- The town is static and empty; life is what makes it read as a place.

## Scope

- Polish:
  - Sub-stepped rider movement so no single sub-step moves farther than a safe fraction of the thinnest obstacle, with a head-on test at `MAX_STEP`.
  - Camera occlusion: the camera pulls in when buildings block the line of sight. This uses a core segment-vs-AABB query.
  - A headless test for the camera snap.
- Life:
  - Townsfolk: simple agents in core that wander between street waypoints, collide with the same static obstacles, and expose a logical animation state.
  - A tethered horse in front of the general store: a static obstacle with an idle visual.
  - A train in core that runs along the railway on a deterministic schedule, stops at the station, and is a moving obstacle for the rider.
  - Render: person billboard sprites (reusing the 8-direction billboard system), a low-poly train (locomotive and a few wagons), and the tethered horse.
  - Game: wire everything.

Rails stay crossable on purpose (horses cross tracks); only the train blocks.
Out of scope: dialogue, interaction, combat, audio, rounder sprite art.

## Constraints

- Same rules as the previous features: logic never knows how things are drawn, collision shapes are owned by core, and `pampa-core` stays dependency-free.
- Bevy stays at `=0.20.0`. Everything is deterministic and testable in core.
- No third-party art. All artifacts are in English.

## Delivery strategy

`exception-ok`: local repository, no remote. Branch `feat/town-life` is stacked on `feat/castex-town` (`7891de3`).
Forecast: ~1400 authored changed lines.
Review base: `7891de3`.

## Tasks

| ID | Task | Route | Status | Commit | Review |
|----|------|-------|--------|--------|--------|
| T1 | core: sub-stepped rider movement + head-on thin-obstacle test at MAX_STEP (test-first) | delegated (writer trigger: 2+ non-trivial files) | [x] | `bfc3341` | pending (RDD owned by parent) |
| T2 | core segment-vs-AABB query; game camera occlusion pull-in; headless camera snap + occlusion tests | delegated | [x] | `b9be93c` | pending (RDD owned by parent) |
| T3 | core: townsfolk agents (street waypoints, wandering, collisions, animation state) (test-first) | delegated | [x] | `12f9829` | pending (RDD owned by parent) |
| T4 | core: train on the railway with schedule, station stop, moving obstacle for the rider (test-first) | delegated | [x] | `5ba4d56` | pending (RDD owned by parent) |
| T5 | render: person billboards, low-poly train, tethered horse | delegated | [x] | `a7fc24c` | pending (RDD owned by parent) |
| T6 | game: wire townsfolk, train, tethered horse; runs | delegated | [x] | `5ee9ba9` | pending (RDD owned by parent) |

## Acceptance criteria

- A rider galloping head-on into a tree trunk at `dt = MAX_STEP` stays on the near side.
- The camera never ends a frame inside a building footprint.
- Townsfolk walk the streets without entering buildings.
- The train passes periodically, stops at the station, and blocks the rider.
- All checks pass; `pampa-core` has no Bevy dependency.

## Checks

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo build -p game`
- `timeout 15 cargo run -p game` (runs without panic)

## Progress / Evidence

- Branch `feat/town-life` created from `7891de3`.

- T1 (`bfc3341`, +110/-11): `Rider::step` splits each step into sub-steps so none moves farther than
  `MAX_SUBSTEP_DISTANCE` (half the rider radius, 0.45 m; capped at 32 sub-steps). `MAX_STEP` (1/20 s) moved to
  core and is reused by the game. Finding: with default tuning the review's tunneling risk was not real
  (11 m/s x 0.05 s = 0.55 m per step, tunneling needs more than radius + half-width = 1.25 m), so the default
  head-on plaza-tree test passed before the change; a 40 m/s horse did tunnel. RED: 49 passed, 2 failed (fast
  horse tunneled; `substeps` stub). GREEN: 51 passed.

- T2 (`b9be93c`, +379/-8): core `Aabb::segment_overlap` (slab method, entry/exit fractions clipped to the
  segment). Game `CameraOccluders` built from the town's buildings as columns up to wall height + 3 m roof
  allowance (trees excluded so trunks do not make the camera jump; low buildings can be looked over).
  `pull_in` stops the camera 0.4 m in front of the first blocking column, applied to the desired spot and again
  after smoothing so a glide never ends inside a wall. RED (core): 51 passed, 3 failed; GREEN: 54. RED (game):
  4 passed, 5 failed (`pull_in` stub); GREEN: 9 passed. Headless tests: snap when > 50 m away, glide when close,
  pull-in in front of a wall behind the rider, look over a low building, and a 2400-frame galloping weave
  starting with the rider's back to the general store asserting the camera never ends a frame inside a
  footprint column (the snap test documents existing behavior; it could not be RED on its own).

- T3 (`12f9829`, +534/-23): `World::resolve` is the shared push-out (rider refactored onto it, behavior
  unchanged). `TownLayout.hitching_post`: a rail 3.2 m out from the general store's front (past its 3 m
  veranda) and a saddled horse tied alongside it facing along the street; both are static obstacles.
  `Townsfolk` (16 people, seeded xorshift): `StreetGraph` of the 6 x 6 street intersections; each person walks
  to a neighboring intersection at 1.1-1.6 m/s with a fixed lane offset (at most 0.9 m), pauses 0.5-4 s, picks
  the next one (no U-turn unless a dead end), collides through `World::resolve`, turns back after 2 s stuck,
  and exposes `AnimationState` (Idle/Walk) and a `look` value. RED: 42 passed, 19 failed; GREEN: 61. Tests:
  graph links stay on streets clear of obstacles, deterministic per seed, 10 simulated minutes never inside a
  footprint and always on a street, every person visits at least 4 intersections in 5 minutes, heading faces
  the walk.

- T4 (`5ba4d56`, +595/-11): `SideObstacle` (moving box that pushes circles across, to the side of its center
  line they are on); `World.moving` resolved alongside static obstacles. `Train`: locomotive (14 m) + boxcar,
  two hoppers, boxcar (56 m with couplings), kinematic and a pure function of the schedule clock. Default
  `TrainSchedule`: enters at the west patch edge at 10 m/s, brakes at 0.5 m/s² to stop with its middle at the
  station, dwells 25 s, pulls away and runs until the last car leaves the patch (110.6 s active), period 150 s
  (never shorter than one pass). RED: 61 passed, 9 failed; GREEN: 70. Tests: phase order and dwell length,
  entry at the edge, stop alongside the platform, continuous non-decreasing motion, periodicity, game-step
  integration matches the schedule, cars coupled in order, no obstacles while away, the rider is blocked by the
  train and shoved off the track on its own side.

- T5 (`a7fc24c`, +1034/-279, mostly the module rename `render::rider` -> `render::character`):
  `CharacterVisual::Billboard(SheetKind)` replaces `RiderVisual` (same 8-direction billboard system, one
  material set per sheet, per-entity bob phase). Sheets: mounted gaucho, saddled horse with no rider, and
  people on foot in 4 palettes (red-poncho gaucho, townsman, woman with headscarf and long skirt, farmhand in a
  beret). `TrainPlugin` draws `WorldTrain`: steam locomotive (boiler, smokebox, chimney, domes, cab, coal
  tender, red buffer beams, turning wheels), boxcars and open grain hoppers on the rails, pitched with the
  ground, hidden while off the railway. Town draws the hitching rail. Test-first exception (visual): headless
  tests for sheets (size, distinct non-empty frames, mirrored facing marker, feet on the bottom row, which
  caught boots one pixel short, palettes differ, horse sheet has no rider) and train placement/visibility.
  Screenshots with temporary spawns, reverted by targeted edits: townsfolk in four palettes walking by the
  store with the horse at the rail; the train standing at the platform seen along the track; hoppers and
  boxcars from the north side.

- T6 (`5ee9ba9`, +222/-2): `life::TownLifePlugin` (`SimulateTown` set before `DrivePlayer`): steps the train
  and writes its car obstacles into `GameWorld.moving`, steps the crowd, syncs walker transforms and poses,
  spawns the tethered horse billboard from `WorldTown`. Headless tests: walkers follow logic, the train
  advances and becomes an obstacle, the horse stands at the rail and is an obstacle.

- Checks (final): `cargo fmt --all -- --check` ok; `cargo clippy --workspace --all-targets -- -D warnings` ok;
  `cargo test --workspace` ok (core 70, render 14, game 12); `cargo build -p game` ok;
  `cargo tree -p pampa-core | rg -c bevy` no match; `timeout 15 cargo run -p game` exit 124 (ran until killed).
- Known issues / follow-ups: the rider does not collide with townsfolk and townsfolk ignore the rider; the train
  is not a camera occluder; the train only runs west to east; the mounted rider's back view still shows the
  face (pre-existing sheet art).

## Next step

Native review of the work-unit commits (owned by the parent), then delivery under ordinary repository policy.
