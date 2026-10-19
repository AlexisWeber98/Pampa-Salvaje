# Feature: vertical-slice

Locator: `odd/tasks/vertical-slice.md` · Engram mirror: `odd/vertical-slice/tasks`

## Objective

Build the smallest playable slice of Pampa Seca: a heightmap pampa patch with dust fog, a rider rendered as a billboard sprite with continuous heading, and a follow camera.

## Problem / Why

Validate that the chosen visual direction (low-poly 3D world + HD-2D style sprites) looks and feels right before investing in missions, NPCs or content.

## Scope

- Cargo workspace with three members:
  - `crates/core` — pure game logic, no Bevy dependency, unit-tested.
  - `crates/render` — Bevy plugins that draw the world (sprites today, meshes later).
  - `game` — binary that wires core + render together.
- Heightmap terrain (procedural, no external assets), distance/dust fog.
- Rider entity: continuous heading angle, kinematic movement driven by input.
- 8-direction billboard sprite selected from the continuous heading (procedurally generated placeholder texture, no third-party art).
- Third-person follow camera.

Out of scope: missions, NPCs, combat, audio, real art, license choice.

## Constraints

- Bevy pinned to an exact version (`=0.20.0`); upgrade only between milestones.
- Architecture rule: game logic never knows how things are drawn.
  - Heading is a continuous angle, never an 8-way enum, in `core`.
  - Collision shapes are owned by logic, never derived from sprite pixels.
  - Animation is a logical state (`Idle`, `Walk`, `Gallop`), never a frame index, in `core`.
  - Visual representation is a swappable per-entity component in `render`.
- Lightweight: no heavy optional Bevy features beyond what the slice needs.
- All artifacts (code, comments, docs) in English.

## Delivery strategy

`exception-ok` — local repository with no remote yet; PR slicing is not applicable. Revisit when a remote exists.
Forecast: ~700 authored changed lines across all tasks.

## Tasks

| ID | Task | Route | Status | Commit | Review |
|----|------|-------|--------|--------|--------|
| T1 | Workspace scaffold (workspace manifest, three crates, README) | delegated (writer trigger: 2+ non-trivial files) | [x] | `941c956` | not run (RDD owned by parent) |
| T2 | `core`: heading, rider kinematics, animation state + unit tests (test-first) | delegated | [x] | `d15ef85` | not run (RDD owned by parent) |
| T3 | `render`: heightmap terrain, lighting, dust fog | delegated | [x] | `e238a2f` | not run (RDD owned by parent) |
| T4 | `render`: billboard rider sprite with 8-direction selection from heading (+ tests for direction mapping) | delegated | [x] | `db03644` | not run (RDD owned by parent) |
| T5 | `game`: input -> core movement, follow camera, wire plugins; runs | delegated | [x] | `e79fa43` | not run (RDD owned by parent) |

## Acceptance criteria

- `cargo test --workspace` passes.
- `cargo clippy --workspace --all-targets -- -D warnings` passes.
- `cargo build -p game` succeeds; the game window opens and shows terrain, fog and a controllable rider.
- `core` has no Bevy dependency (`cargo tree -p pampa-core` contains no `bevy`).

## Checks

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo build -p game`

## Progress / Evidence

- Repository initialized at `089a28f` (main); feature branch `feat/vertical-slice`.
- T1: workspace builds with Bevy `=0.20.0`, `default-features = false` and a lean feature set
  (`std`, `multi_threaded`, `async_executor`, `bevy_asset`, `bevy_log`, `bevy_winit`, `wayland`, `x11`,
  `keyboard`, `bevy_render`, `bevy_core_pipeline`, `bevy_pbr`, `bevy_color`, `bevy_mesh`, `bevy_image`,
  `tonemapping_luts`, `zstd_rust`). No audio, UI, text, glTF, scenes, picking or gamepad.
  Checks: fmt ok, clippy ok, test ok (0 tests), `cargo tree -p pampa-core | rg -c bevy` -> no match.

- T2: `pampa-core` adds `Vec2`, `Heading` (continuous, `[0, TAU)`), `AnimationState`, `Circle` collider,
  procedural `Terrain` (3 sine-product octaves, amplitude 2.5 m) and kinematic `Rider`.
  RED: `cargo test -p pampa-core` -> 0 passed; 19 failed (`todo!()` stubs). GREEN: 19 passed.
  Checks: fmt ok, clippy ok, `cargo tree -p pampa-core | rg -c bevy` -> no match.

- T3: `TerrainPlugin` (flat-shaded 600 m patch built from `pampa_core::Terrain`, vertex-colored, 400 rock/shrub props),
  `AtmospherePlugin` (dusty `ClearColor`, warm `GlobalAmbientLight`, shadowed low sun, atmospheric `DistanceFog`
  auto-added to every `Camera3d`), `coords::world_from_ground` (core north -> Bevy -Z).
  Test-first exception: pure rendering wiring, no meaningful deterministic test; verified by running the game
  (temporary static camera) and a screenshot: rolling faceted ground fading into dust haze.

- T4: `render::rider` adds `ViewDirection::pick(heading, camera_yaw)` (nearest of 8 views relative to the camera),
  `ground_yaw` (Bevy dir -> core yaw), procedural 512x64 sprite sheet (`sheet::rider_sheet_pixels`, horse + gaucho,
  red facing marker), `RiderVisual::Billboard` swappable component (+ `VisualPose { heading, animation }` copied from
  core), `RiderVisualPlugin` (cylindrical billboard child, one material per view via `uv_transform`, gait bob).
  RED: `cargo test -p pampa-render` -> 0 passed; 10 failed (`todo!()` stubs). GREEN: 10 passed.
  Visual wiring itself has no deterministic test (exception); verified by screenshot of an 8-heading lineup.

- T5: `game` wires `PlayerPlugin` (W/S throttle, A/D turn, Shift gallop -> `RiderInput` -> `Rider::step`, dt capped
  at 1/20 s, syncs `Transform` + `VisualPose`), `FollowCameraPlugin` (behind/above along heading, `smooth_nudge`,
  terrain clearance), Esc quits, window titled "Pampa Seca". Unit test for key mapping (1 test).
  Sprite tweak: neckerchief hidden in away-facing views so back and front views read differently.
  `timeout 15 cargo run -p game` -> exit 124 (ran until killed), window created, no panic.
  Not verified interactively: holding keys (no input-injection tool available); logic covered by core tests.

## Next step

Human play-test of feel (speeds, turn rate, camera smoothing) and visual direction; then decide next milestone.
