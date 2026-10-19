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
| T1 | Workspace scaffold (workspace manifest, three crates, README) | delegated (writer trigger: 2+ non-trivial files) | [x] | | |
| T2 | `core`: heading, rider kinematics, animation state + unit tests (test-first) | delegated | [ ] | | |
| T3 | `render`: heightmap terrain, lighting, dust fog | delegated | [ ] | | |
| T4 | `render`: billboard rider sprite with 8-direction selection from heading (+ tests for direction mapping) | delegated | [ ] | | |
| T5 | `game`: input -> core movement, follow camera, wire plugins; runs | delegated | [ ] | | |

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

## Next step

T2.
