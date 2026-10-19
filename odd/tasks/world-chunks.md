# Feature: world-chunks

Locator: `odd/tasks/world-chunks.md` · Engram mirror: `odd/world-chunks/tasks` (pending, project not registered)

## Objective

Turn the fixed 600 m patch into a large open world streamed in chunks, with several towns at their configured `origin`s. This is the foundation for an RDR-style world with roads and railways between towns.

## Problem / Why

- Today the world is one ±300 m patch, and the game only builds the first town in `towns.ron`. The user added a second town ("Florencio Varela") and it never appears; at the same `origin` it would overlap Castex.
- Real towns are kilometers apart. The world must be bigger than what fits in memory or on screen at once.

## Scope

- World config in `assets/world/towns.ron`, renamed or extended to a world file:
  - A `world` section with the world half-extent (default 4 km per side, ±4000 m) and the chunk size (default 200 m).
  - The `towns` list.
  - Backwards compatible with the current `(towns: [...])` shape.
- Core (dependency-free):
  - `ChunkCoord` and the chunk math: world ↔ chunk, the chunks within a radius, deterministic iteration order.
  - `Terrain` supports many flat areas, one per town. `Terrain::height` stays the single source of truth and is continuous across chunk borders.
  - `WorldBounds` comes from the world config.
  - Multiple towns, each generated at its `origin`. Validation rejects towns that overlap each other (extents plus leveled band) and towns outside the world, with clear errors naming both towns.
  - Static obstacles are indexed per chunk (or a uniform grid), so collision only scans nearby obstacles.
  - The bounds clamp uses the world bounds.
- Render: chunk streaming.
  - Terrain meshes and props are generated per chunk (props seeded by the chunk coordinate) within a view radius around the camera/rider, e.g. 3 chunks.
  - Chunks are despawned beyond radius + 1 (hysteresis).
  - Generation work per frame is bounded (at most N chunks per frame) to avoid hitches.
  - Fog distance hides the streaming edge.
  - Town meshes, signs and the train for each town spawn and despawn with town proximity, with the same hysteresis idea.
- Life: townsfolk are simulated only for towns near the player (activation radius). Each town has its own crowd, seeded from the town seed. Each town's train runs on its own rail segment (the town extent plus a margin); a full rail network is out of scope.
- Minimap: shows the nearest town (or the one the player is in), and its title shows that town's name.
- Spawn: at the arrival point of the first town.
- Content: the shipped file keeps Castex 4×5 first.
  - The content test that pins the shipped file should check that the file parses and validates, and that Castex is present with its config. It should not require exactly one town.
  - Add an example second town far from Castex as a commented example in the README, not in the shipped file (the user's local edits stay theirs).

Out of scope: roads and railways between towns, map-wide minimap, save/load, LOD for distant towns (only fog cut-off).

## Constraints

- Same architecture rules: logic never knows how things are drawn, and `pampa-core` stays dependency-free.
- Bevy stays at `=0.20.0`. Keep the game lightweight: bounded per-frame generation, despawn far chunks, and no unbounded caches.
- f32 precision: keep the world within ±8 km; document the limit.
- All artifacts are in English.

## Delivery strategy

`exception-ok`: fast-forward `main` after review, under the user's decision. Branch `feat/world-chunks` from `main` (`aeb1385`).
Forecast: ~1400 authored changed lines.
Review base: `aeb1385`.

## Tasks

| ID | Task | Route | Status | Commit | Review |
|----|------|-------|--------|--------|--------|
| T1 | core: chunk math, world config, multi flat areas, world bounds (test-first) | delegated (writer trigger: 2+ non-trivial files) | [x] | `b3f0f48` | pending (RDD owned by parent) |
| T2 | core: multiple towns at origins, overlap/outside validation, spatial index for obstacles (test-first) | delegated | [x] | `2bb10d4` | pending (RDD owned by parent) |
| T3 | content: world section + multi-town loading, compat, tests, README | delegated | [x] | `277a26b` | pending (RDD owned by parent) |
| T4 | render: chunk streaming for terrain + props with hysteresis and per-frame budget (headless tests for the streaming set logic) | delegated | [x] | `c2d018d` | pending (RDD owned by parent) |
| T5 | game/render: per-town spawn/despawn (meshes, signs, train), per-town crowds with activation radius, minimap nearest town; runs | delegated | [x] | `0ae25a9` | pending (RDD owned by parent) |

## Acceptance criteria

- Riding from Castex in any direction streams terrain continuously, with no seams or holes and no frame hitches from generation.
- A second town declared at a distant `origin` appears when you ride there, with its own signs, crowd and train.
- Overlapping or out-of-world towns are rejected with a clear message.
- Memory stays bounded: chunk entities are despawned beyond the radius.
- All checks pass; `pampa-core` has no dependencies.

## Checks

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo build -p game`
- Game runs ~15 s without panic.

## Progress / Evidence

- Branch `feat/world-chunks` created from `aeb1385`.
- T1 `b3f0f48`: `ChunkCoord` (containing/min/center/bounds, `within(radius)` disc nearest first then south-north/west-east, per-chunk seed), `WorldConfig` (default ±4000 m, 200 m chunks, max ±8000 m, min chunk 50 m) with `WorldConfigError`; `Terrain.flat_areas: Vec<FlatArea>` (Terrain is no longer `Copy`); overlapping bands blend by weight-averaged height eased in by the strongest weight (continuous); `World::new(config, terrain)` takes its bounds from the config. RED: 53 core tests failed (7 chunk, 2 terrain, the rest through `todo!()` bounds); GREEN 119.
- T2 `2bb10d4`: `place_towns(world, configs)` with `PlacementError::{Invalid, OutsideWorld, OverlapsTown}` naming both towns; `TownLayout::{leveled, rail_corridor, seed}`; rails now span the grid ± `RAIL_RUN_OUT` (400 m) instead of the patch; `World::with_towns`; `StaticObstacles` uniform-grid index (16 m cells) used by `World::resolve`; `TownActivation` and `nearest_town` (pure, used by T5). The town-level `OutsideWorld` error moved to placement. RED: 44 core tests failed; GREEN 132. Default train period went 150 → 180 s so the longer line still has an `Away` phase.
- T3 `277a26b`: optional `world: (half_extent, chunk_size)` section (each field optional; bad section → default world + `ContentError::World`); towns validated with `place_towns`; `ContentError::Town(PlacementError)`; `WorldContent`; shipped-file test checks parse + validity + Castex present (not "only town"); README documents the world section, spacing rules and a commented second-town example, which a test splices into the shipped file and places. RED: 8 content tests failed; GREEN 17.
- T4 `c2d018d`: pure `StreamingSet` (radius R load, R+1 unload, per-update budget) — RED 4/4 failed, GREEN; terrain streamed per chunk (radius 3, 40×40 quads, 1 chunk per frame, full fill on the first frame), props per chunk seeded by the coordinate skipping leveled ground, despawned with their chunk; `StreamFocus` follows the rider; `FOG_VISIBILITY` 180 m ≤ guaranteed reach (R−1)×chunk = 400 m (test). `PATCH_HALF_EXTENT` removed.
- T5 `0ae25a9`: `WorldTowns`, `ActiveTowns` (activate within 600 m of a town's extent, drop past 750 m), `TownRoot` per town with meshes, signs and train cars as children; `WorldTrains` per town (all step, all are moving obstacles); per-town `Crowds` seeded from the town seed, walkers/horse spawned only for active towns (all crowds when no `ActiveTowns`, e.g. headless tests); complaints track cooldowns per town; minimap shows the nearest town with its name as title and caches one raster per town; camera occluders from every town. Wiring tests were written with the code (headless), no separate RED.
- Checks (final, worktree root): `cargo fmt --all -- --check` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo test --workspace`: core 132, content 17, render 71, game 42, all passing; `cargo build -p game` ok; `cargo tree -p pampa-core | rg -c bevy` and `cargo tree -p pampa-content | rg -c bevy`: no match; `pampa-core` has no dependencies. Game ran ~15 s twice with no panic: shipped file logs `towns: 1 built: Eduardo Castex` and no warning; with a temporary second town listed first (reverted) it logs `towns: 2 built: Florencio Varela, Eduardo Castex`, spawns there with its buildings, walkers and minimap titled "FLORENCIO VARELA".

### Rules chosen

- Chunk `(0, 0)` covers `[0, size)²`; streaming disc is Euclidean in chunk units; border vertices are computed from the same chunk corners so neighbors share them exactly.
- Towns keep apart by leveled extent (extent + 60 m) and by railway corridor (track ± 4 m, 400 m past the grid) against the other's leveled extent; earlier towns win.
- Train cars are hidden whenever they are off the town's line, so the train appears and leaves at the ends of the line, out in the fog.

### Known issues / follow-ups

- The minimap title shares the top edge with the clamped player arrow when the rider is north of the framed town.
- The first frame builds all 29 chunks at once (a short startup cost by design); afterwards at most one chunk per frame.

## Next step

Parent: native review (RDD) of the feature commits from base `aeb1385`, then the user's delivery decision.
