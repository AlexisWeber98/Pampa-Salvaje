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
| T1 | core: chunk math, world config, multi flat areas, world bounds (test-first) | delegated (writer trigger: 2+ non-trivial files) | [ ] | | |
| T2 | core: multiple towns at origins, overlap/outside validation, spatial index for obstacles (test-first) | delegated | [ ] | | |
| T3 | content: world section + multi-town loading, compat, tests, README | delegated | [ ] | | |
| T4 | render: chunk streaming for terrain + props with hysteresis and per-frame budget (headless tests for the streaming set logic) | delegated | [ ] | | |
| T5 | game/render: per-town spawn/despawn (meshes, signs, train), per-town crowds with activation radius, minimap nearest town; runs | delegated | [ ] | | |

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

## Next step

T1.
