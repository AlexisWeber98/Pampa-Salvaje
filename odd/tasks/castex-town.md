# Feature: castex-town

Locator: `odd/tasks/castex-town.md` · Engram mirror: `odd/castex-town/tasks` (pending, project not registered)

## Objective

Harden the vertical slice base, then add the first settlement: a small 1920s–1930s pampa railway town inspired by Eduardo Castex (La Pampa), where the rider can ride the streets and collides with buildings.

## Problem / Why

- The vertical slice review left non-blocking follow-ups: there is no world boundary, `Rider::step` does not guard against non-finite input, and the game wiring has no headless tests.
- The slice is a tech demo. A town is the first content that makes it feel like the game.

## Scope

- Polish:
  - World bounds owned by core, so the rider cannot leave the drawn patch.
  - Non-finite/negative input and `dt` guards in `Rider::step`.
  - Headless Bevy tests for the dt cap and the billboard view selection.
- Town (inspired by the period, not a historically exact reconstruction):
  - Data in core: a street grid around a central plaza, a railway line with a station, and building footprints (general store, police station, church, houses, sheds, a windmill with its water tank).
  - Terrain flattened under the town.
- Collision: rider (circle) against static obstacles (axis-aligned boxes) in core, resolved by pushing the rider out.
- Render: procedural low-poly buildings (brick or whitewash walls, false-front facades, pitched sheet-metal roofs), rails, plaza trees, windmill.
- Game: town wired in, rider spawns at the town edge.

Out of scope: NPCs, interiors, missions, rounder sprite art (follow-up), real art assets.

## Constraints

- Same architecture rules as `vertical-slice.md`: the logic never knows how things are drawn, and collision shapes are owned by core.
- Bevy stays at `=0.20.0`. `pampa-core` stays dependency-free.
- No third-party art. All artifacts are in English.

## Delivery strategy

`exception-ok`: local repository with no remote yet. Branch `feat/castex-town` is stacked on `feat/vertical-slice` (`ad90d03`).
Forecast: ~800 authored changed lines.
Review base: `ad90d03` (the vertical slice is reviewed; `ad90d03` is a passive doc commit).

## Tasks

| ID | Task | Route | Status | Commit | Review |
|----|------|-------|--------|--------|--------|
| T1 | Polish: core `WorldBounds` + finite guards in `Rider::step` (test-first); headless App tests for dt cap and billboard view | delegated (writer trigger: 2+ non-trivial files) | [x] | `ee71080` | pending (RDD owned by parent) |
| T2 | core: town layout data (streets, plaza, rail, station, building footprints) + terrain flattening (test-first) | delegated | [x] | `f3c5722` | pending (RDD owned by parent) |
| T3 | core: circle vs AABB collision resolution for the rider (test-first) | delegated | [x] | `6dc3b0f` | pending (RDD owned by parent) |
| T4 | render: procedural low-poly town (buildings, facades, roofs, rails, trees, windmill) | delegated | [x] | `f7dccc8` | pending (RDD owned by parent) |
| T5 | game: wire town, spawn at town edge, collisions active; runs | delegated | [x] | `f98a9d2` | pending (RDD owned by parent) |

## Acceptance criteria

- The rider cannot leave the world bounds or pass through buildings.
- `Rider::step` stays finite for NaN, infinite or negative inputs and `dt`.
- The town renders and reads as a small pampa railway town in the fog.
- All checks pass; `pampa-core` has no Bevy dependency.

## Checks

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo build -p game`
- `timeout 15 cargo run -p game` (runs without panic)

## Progress / Evidence

- Branch `feat/castex-town` created from `ad90d03`.

- T1 (`ee71080`, +423/-23): core `world` module with `PATCH_HALF_EXTENT` (300 m, also the default `TerrainPlugin`
  size), `WorldBounds::contain` (circle kept inside, reports push directions) and `World { terrain, bounds }`.
  `Rider::step` now takes `&World`: non-finite turn/throttle count as 0, non-finite or non-positive `dt` is a no-op,
  the rider is clamped to the bounds and loses only the velocity component into the edge (slides).
  RED: `cargo test -p pampa-core` -> 11 passed, 15 failed (`todo!()` stubs). GREEN: 26 passed (after fixing a
  push-direction sign in `contain`). Headless Bevy tests (`MinimalPlugins`, `TimeUpdateStrategy::ManualDuration`):
  `game` dt cap with a 0.2 s hitch (Bevy's virtual clock already clamps at 0.25 s), transform sync;
  `render` billboard view + material index follow heading relative to the camera (stand-in material handles,
  `update_billboards` only; no GPU needed).

- T2 (`f3c5722`, +736/-3): `Aabb` shape; `Terrain.flat_area: Option<FlatArea>` levels a rectangle with a
  smoothstep falloff inside `Terrain::height` (single source of truth; `natural_height` is the raw pampa).
  `TownLayout::castex_inspired()` (doc says "inspired by", not a reconstruction): 5x5 grid of 40 m blocks and
  12 m streets around a plaza with 16 trees; railway (broad gauge) 24 m north of the grid spanning the patch;
  station, platform, two grain stores, shed and windmill + tank by the track; church north of the plaza,
  police station east, general store west; houses/sheds on lots by deterministic hash; three back-yard
  windmills with tanks. `flat_area`, `obstacles`, `arrival` (main road south of town, facing north).
  RED: 9 passed, 31 failed. GREEN: 40 passed. Larger than the ~400-line heuristic: layout data plus 13 tests.

- T3 (`6dc3b0f`, +189/-1): `Circle::push_out` (face, corner, center-inside cases); `World.obstacles` and
  `World::with_town`; `Rider::step` resolves obstacles (up to 4 passes, linear scan) before the bounds, removing
  the velocity component into each wall. A 240 s galloping wander through the town asserts contact happens and
  the rider never ends a step inside a footprint. RED: 40 passed, 8 failed. GREEN: 48 passed.

- T4 (`f7dccc8`, +751/-1): `render::town::TownPlugin` draws a `WorldTown`: dirt streets, plaza (paths,
  monument, trees), ballast/sleepers/rails following the ground, whitewash/lime/brick walls with false-front
  parapets or pitched sheet-metal roofs (unit gable prism mesh), church with bell tower, store veranda,
  station with platform canopy, metal sheds and grain stores, round tanks, lattice windmills with turning
  wheels (`WindmillRotor`). Terrain props skip leveled ground. Test-first exception: visual only; verified
  by screenshots from the arrival point (town skyline with church tower, false fronts, gable roofs in the
  dust haze) and from a temporary in-town spawn (plaza trees, paths, monument; temp change reverted).

- T5 (`f98a9d2`, +60/-15): `main` builds `World::with_town` once and shares its terrain with render
  (`WorldTerrain`, `WorldTown`, `GameWorld`, `PlayerSpawn` from `town.arrival()`). Follow camera snaps when
  more than 50 m from its target (first frame). Headless test: player spawns at `PlayerSpawn`.

- Checks (final): `cargo fmt --all -- --check` ok; `cargo clippy --workspace --all-targets -- -D warnings` ok;
  `cargo test --workspace` ok (core 48, render 11, game 4); `cargo build -p game` ok;
  `cargo tree -p pampa-core | rg -c bevy` -> no match; `timeout 15 cargo run -p game` -> exit 124, no panic.
- Not verified interactively: riding into walls by keyboard (no input injection); covered by core tests.
- Known issues: the follow camera can clip into buildings (no camera collision); rails are not obstacles.
- Engram mirror `odd/castex-town/tasks`: pending (the project is not registered in Engram).

## Next step

Native review of `ad90d03..HEAD` (parent-owned), then a human play-test of riding the town.
