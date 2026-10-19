# Feature: town-config

Locator: `odd/tasks/town-config.md` · Engram mirror: `odd/town-config/tasks` (pending, owned by parent)

## Objective

Make towns data-driven: a generator builds any town from a `TownConfig`, and towns are declared in an editable `assets/world/towns.ron` file. Castex becomes 4×5 blocks with 10 m streets, and its plaza faces the railway station.

## Problem / Why

- The user tried 3×3 by editing constants in `town.rs`, and at least 8 tests failed. Building sizes, the hitching post, signs and spawn points assume a 5×5 grid of 40 m blocks, so the town only looks configurable.
- The group will add more towns and cities (Carmensa, General Alvear…). Their configuration must live in a data file, not in Rust.
- In Argentine pampa towns the plaza often faces the railway station. That gives the generator a natural placement rule when no central block exists.

## Scope

- Core (dependency-free): `TownConfig` and `TownLayout::generate(&TownConfig) -> Result<TownLayout, TownConfigError>`.
  - Config fields:
    - `name`
    - `origin` (the town center in world coordinates; Castex stays at the origin)
    - `blocks: (east_west, north_south)`
    - `block_size`
    - `street_width`
    - `railway: Option<RailwayConfig { side, offset }>`
    - `plaza: PlazaPlacement` (`Auto` | `Center` | `FacingStation` | `Block(i, j)`)
    - `seed` (for house/shed lots)
  - Placement rule (`Auto`):
    - If there is a central block (odd count on both axes), the plaza goes there.
    - Otherwise it goes in the block facing the station: the row adjacent to the railway, with the column nearest the grid center.
    - The station is then aligned to face the plaza across the street.
    - `Center` on a grid without a central block is a config error.
  - Landmark buildings (general store, police station, church, station, grain stores, windmills) are sized relative to `block_size`, with sensible minimums. Lots, hitching post, signs/places, the street graph, the arrival point and the flat area all derive from the config.
  - Validation returns clear errors: too few blocks, block too small for the landmarks, the town does not fit inside the world patch, or the plaza block is out of range.
  - `TownLayout::castex_inspired()` remains as a convenience that builds the Castex config: 4×5 blocks, 40 m blocks, 10 m streets, railway north at 24 m, plaza `Auto` (so it faces the station).
- Tests: the invariants are property-style over several configs (3×3, 4×5, 5×5, 6×4, block 32–50 m, street 8–14 m).
  - Nothing overlaps streets, the plaza or other buildings, and everything sits inside the extent on level ground.
  - Walkers stay on streets, and the rider never ends inside buildings.
  - The station faces the plaza when `Auto` picks `FacingStation`.
  - Invalid configs return the right errors.
  - Fix the tests that hard-code 5×5 values (e.g. "36 intersections").
- Content (`pampa-content`): loads `assets/world/towns.ron` (a list of `TownConfig`s, serde-mapped from content-side DTOs so core stays dependency-free).
  - Validates through `TownLayout::generate`.
  - Falls back to the built-in Castex config with a warning.
  - Documented in `assets/world/README.md`.
- Game/render: build the world from the loaded towns. One town is rendered for now, and everything else (signs, minimap, townsfolk, train, horse) follows the generated layout.
- Chore: add `.claude/` to `.gitignore`.

Out of scope: multiple simultaneous towns in the world, roads between towns, a bigger world patch.

## Constraints

- Same architecture rules: logic never knows how things are drawn, and `pampa-core` stays dependency-free.
- Bevy stays at `=0.20.0`. All artifacts are in English.

## Delivery strategy

`exception-ok`: fast-forward `main` after review, under the user's decision. Branch `feat/town-config` is stacked on `feat/bump-complaints` (`56bd69f`).
Forecast: ~1100 authored changed lines.
Review base: `56bd69f`.

## Tasks

| ID | Task | Route | Status | Commit | Review |
|----|------|-------|--------|--------|--------|
| T1 | core: `TownConfig`, validation errors, parametric `generate` with plaza placement rule (test-first, property-style over configs) | delegated (writer trigger: 2+ non-trivial files) | [x] | `ec5003d` | medium · granted · approved |
| T2 | core: places, street graph, spawn/arrival and other derived data follow the config; fix hard-coded tests | delegated | [x] | `416d812` | medium · granted · approved |
| T3 | content: `assets/world/towns.ron` loading + fallback + README; Castex 4×5 / 10 m streets / plaza facing station | delegated | [x] | `3e2ad1c` | medium · granted · approved |
| T4 | game/render: build world from loaded towns; `.gitignore` `.claude/`; runs | delegated | [x] | `896d6de` | medium · granted · approved |

## Acceptance criteria

- Castex renders as 4×5 blocks with 10 m streets, and the plaza faces the station across the street.
- Changing `towns.ron` (blocks, sizes, plaza) changes the town without recompiling; invalid configs log a clear error and fall back.
- All invariant tests pass for every tested config.
- All checks pass; `pampa-core` has no dependencies.

## Checks

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo build -p game`
- Game runs ~15 s without panic.

## Progress / Evidence

- Branch `feat/town-config` created from `56bd69f`.
- T1 `ec5003d`: `TownConfig`, `RailwayConfig`, `PlazaPlacement`, `TownConfigError` (Display + Error) and `TownLayout::generate`. RED: 17 of 18 new town tests failed on a `todo!()` generator; GREEN after implementation. Invariants run over 9 sample configs (3x3, 4x4, 4x5, 5x5, 6x4, 3x5, 5x3, 4x6; blocks 32–50 m; streets 8–14 m; railway N/S/E/W/none; Auto, FacingStation, Block). `TownLayout.railway` is now `Option<Railway>` and `Train::for_town` returns `Option<Train>`.
- T2 `416d812`: station sign reads "ESTACIÓN <TOWN NAME>", facade boards narrow to fit small facades, platform board faces the track on any side. RED: 3 places tests failed; GREEN after. Street-graph, walker, rider, train and world tests now run over every sample config (they passed on first run because T1 already generalized the geometry). The "36 intersections" assumption became `(cols + 1) * (rows + 1)`.
- T3 `3e2ad1c`: `pampa-content::towns` (DTOs, `parse_towns`, `load_towns`, `load_towns_or_builtin`), `assets/world/towns.ron`, `assets/world/README.md`; Castex becomes 4x5, 40 m blocks, 10 m streets, plaza facing the station. RED: 5 content tests and the Castex-shape core test failed; GREEN after.
- T4 `896d6de`: game loads the first town (`game/src/towns.rs`), logs config and glyph warnings at startup, runs without a train for railway-less towns; render veranda depth follows `HitchingPost::veranda`; minimap draws a railway on any side; `.claude/` ignored. Headless tests cover the shipped file, a custom town and the fallback (written together with the code, no separate RED).
- Checks: `cargo fmt --all -- --check` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo test --workspace`: core 110, content 13, render 60, game 38, all passing; `cargo build -p game` ok; `cargo tree` shows no Bevy under `pampa-core` or `pampa-content`, and `pampa-core` has no dependencies. Game ran ~16–20 s with no panic and no town warning; screenshots show the 4x5 minimap with the plaza in the top row and the station right above it, and the store veranda with the tied horse.

### Rules chosen

- Minimums: 3 blocks per axis, 30 m blocks, 8 m streets, railway offset 24 m; the town plus its 60 m leveled band must fit the ±300 m patch.
- Landmarks: store prefers west, church north, police east of the plaza (then the other neighbors in a fixed order, then the nearest free block for corner plazas); each faces the plaza. Block-relative proportions scale with `block_size / 40`.
- Station yard is anchored to the track and centered on the plaza along it (clamped so the 60 m platform stays within the grid's length); grain stores, shed and windmill appear only where the grid is long enough.
- Store veranda depth is `clamp(street/2 - 2.7, 1.2, 3.0)` so the tied horse stays clear of walkers on narrow streets.

- Review: native RDD over `56bd69f..61a1582` (medium, slice budget reached). Consent was granted; one lens (reliability) ran and the review was approved and acknowledged under lineage `review-17d16d5b61efd7e1`.
  Advisory follow-ups (non-blocking):
  - No app-level test starts the plugins without `WorldTrain` (railway-less town). Parent spot check: every `WorldTrain` system parameter is `Option<…>` (`game/src/life.rs:96`, `crates/render/src/train.rs:335,365,399`), so no startup panic is expected, but it stays unproven by a test.
  - Minimap railway drawing on other sides (`crates/render/src/minimap/raster.rs:194-199`) and `crates/core/src/town.rs:451` are suggestions only.

## Next step

Parent: native review (RDD) of the feature commits from base `56bd69f`, then the user's delivery decision.
