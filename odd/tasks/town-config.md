# Feature: town-config

Locator: `odd/tasks/town-config.md` · Engram mirror: `odd/town-config/tasks` (pending, project not registered)

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
| T1 | core: `TownConfig`, validation errors, parametric `generate` with plaza placement rule (test-first, property-style over configs) | delegated (writer trigger: 2+ non-trivial files) | [ ] | | |
| T2 | core: places, street graph, spawn/arrival and other derived data follow the config; fix hard-coded tests | delegated | [ ] | | |
| T3 | content: `assets/world/towns.ron` loading + fallback + README; Castex 4×5 / 10 m streets / plaza facing station | delegated | [ ] | | |
| T4 | game/render: build world from loaded towns; `.gitignore` `.claude/`; runs | delegated | [ ] | | |

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

## Next step

T1.
