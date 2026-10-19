# Feature: polish-art-camera

Locator: `odd/tasks/polish-art-camera.md` · Engram mirror: `odd/polish-art-camera/tasks` (pending, project not registered)

## Objective

Close the town-life review follow-ups, make the rider collide with townsfolk, give the characters rounder and better animated sprites, and add a free 360° orbit camera.

## Problem / Why

- Town-life review follow-ups:
  - A circle pinned between a train car and a static obstacle may not converge in 4 resolve passes.
  - `Townsperson::spawn` underflows on an empty or isolated street graph.
- The rider passes through townsfolk.
- Playtest feedback: the rider and horse look too boxy, and the rider's back view shows a face.
- Playtest request: the player wants to move the camera focus around the rider in 360°.

## Scope

- Robustness:
  - The pinch between a moving and a static obstacle resolves to a valid, non-overlapping position. Static obstacles win; if no free spot exists, the circle is moved to the nearest free side along the track normal.
  - Townsfolk spawning handles degenerate graphs (no nodes, isolated node) without panicking.
  - Rider ↔ townsfolk collision: circle vs circle. The rider pushes walkers aside, and walkers also avoid the rider.
- Art:
  - Rounder silhouettes for the horse, the mounted gaucho and the townsfolk (anti-aliased curves, shading, outline).
  - More gait frames: walk and gallop cycles driven by the logical animation state and the speed.
  - A correct back view, with no face.
  - Still procedurally generated, with no third-party art.
- Camera orbit:
  - Mouse motion orbits the camera 360° around the rider (yaw unbounded, pitch clamped), with the cursor locked while playing.
  - Q/E keys orbit as a keyboard alternative.
  - The mouse wheel zooms within limits.
  - After ~2 s without camera input while the rider moves, the camera eases back behind the rider (RDR-style).
  - Occlusion pull-in and snap keep working.
  - The 8-direction billboards already select views relative to the camera, so they must stay correct at any orbit angle.

Out of scope: missions, interaction, gamepad, audio.

## Constraints

- Same architecture rules: logic never knows how things are drawn, and `pampa-core` stays dependency-free.
- Bevy stays at `=0.20.0`. Only add the Bevy features required (e.g. `mouse`).
- No third-party art. All artifacts are in English.

## Delivery strategy

`exception-ok`: remote exists (`origin` on GitHub) but work is delivered by fast-forwarding `main` after review, under the user's decision. Branch `feat/polish-art-camera` from `main` (`cc19b02`).
Forecast: ~1200 authored changed lines.
Review base: `cc19b02`.

## Tasks

| ID | Task | Route | Status | Commit | Review |
|----|------|-------|--------|--------|--------|
| T1 | core: pinch resolution + degenerate graph guard (test-first) | delegated (writer trigger: 2+ non-trivial files) | [x] | `fbd493d` | medium · granted · approved |
| T2 | core + game: rider ↔ townsfolk circle collision (test-first) | delegated | [x] | `0b81e5b` | medium · granted · approved |
| T3 | game: 360° orbit camera (mouse, Q/E, wheel zoom, recenter, cursor lock) + headless tests | delegated | [x] | `5350739` | medium · granted · approved |
| T4 | render: rounder sprite sheets, gait cycles, correct back view | delegated | [x] | `ec2b066` | medium · granted · approved |

## Acceptance criteria

- No circle ends a step overlapping a static or moving obstacle in the pinch test.
- `Townsfolk::new` never panics on degenerate layouts.
- The rider cannot pass through walkers.
- The camera orbits a full 360° around the rider, zooms within limits, recenters after idle, and never ends inside a building.
- Sprites look rounder, gaits animate, and the back view shows no face.
- All checks pass; `pampa-core` has no Bevy dependency.

## Checks

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo build -p game`
- `timeout 15 cargo run -p game` (runs without panic)

## Progress / Evidence

- Branch `feat/polish-art-camera` created from `cc19b02`.

- T1 (`fbd493d`, +311/-15): `World::resolve` runs up to 8 push passes; if the circle still overlaps (static and
  moving pushes fighting), it moves to the nearest strictly clear spot found by sampling (0.25 m steps, bisection
  refine, 60 m range) across and along each touching moving obstacle and along the axes; if none, static-only
  passes (static wins). `Townsfolk::from_graph`: empty graph spawns nobody; isolated intersections keep people
  standing idle. Finding: the real platform leaves a 2.5 m gap to a standing car (wider than a rider), so the
  town pinch test stacks freight on the platform edge to narrow it. RED: 67 passed, 10 failed (8 from the
  `from_graph` stub, 2 pinch tests); intermediate 75/2 (search accepted 1 mm overlaps); GREEN: 77.
- T2 (`0b81e5b`, +335/-18): core `Body` + `Circle::push_out_of_body`, `World::resolve_among` (bodies pushed first
  each pass, world wins pinches), `Townsfolk::step_around` (walkers avoid the rider), `Townsfolk::make_way`
  (rider shoves walkers, resolved against the world) and `Rider::keep_clear_of` (walkers pinned against a wall
  stop the rider). Game: crowd steps around the rider before the player; `jostle_crowd` + `place_walkers` run in
  `DrivePlayer` after `drive_player`. Walker-vs-walker collision not added. RED: core 77 passed, 5 failed;
  game 1 failed (rider spawned on a walker); GREEN: core 82, game 13.
- T3 (`5350739`, +713/-96): `game/src/orbit.rs` (Bevy-free): yaw wrapped to [0, TAU) with unbounded rotation,
  pitch 5°–60°, distance 6–25 m (default 9.5 m, ~15°), 0.003 rad/px, Q/E 2 rad/s, recenter after 2 s idle while
  |speed| > 0.5 m/s via shortest-angle exponential ease. Camera: smoothed look-at point (rate 10), orbit offset,
  terrain clearance, `pull_in` from the rider's true focus, snap behind the rider when > 50 m. Cursor locked and
  hidden while the primary window is focused (bevy_winit falls back to confined), released when unfocused; mouse
  input only while grabbed. Bevy `mouse` feature added explicitly (it was already enabled transitively).
  RED: orbit 10 failed; camera 9 failed / 19 passed; GREEN: game 28. Tests: 360° wrap through all quadrants,
  mouse-right goes clockwise, Q/E rate, pitch and zoom clamps, recenter only after idle and only while moving,
  never while input is active, short way across the seam, bad input ignored, turning in place leaves the orbit,
  billboard view index correct at 8 orbit angles, no frame inside a building in a 2400-frame weave with E
  orbiting, and none at 72 orbit angles beside the general store.
- T4 (`ec2b066`, +1176/-260): new `render::character::paint` (SDF balls, tapered capsules, discs; analytic
  anti-aliased coverage; three-tone toon shading lit from the upper left; contour where parts overlap; one-pixel
  outline; alpha snapped for the existing alpha mask). Figures are built from 3D-ish parts projected per view
  with a slight top-down tilt. Frames 80 px; rows: idle, 4 walk, 4 gallop (horses) — horse sheets 640x720
  (1.8 MB), person sheets 640x400 (1.0 MB), ~7.8 MB total. `VisualPose.speed` added; render owns the gait clock
  (`cadence`: cycles per second from speed and stride) and picks `SheetKind::row`. Away views (k = 0, 1, 7)
  draw hair instead of a face. Visual test-first exception: headless sheet tests (sizes and rows, all frames
  distinct per row and per view, gait rows differ, outline present, facing marker sides, no skin or eye pixels
  above the chin in away views, faces in front views, idle and walk feet in the bottom 3 rows, saddled horse
  without rider) plus a headless gait test (rows follow state, faster at gallop, material = cell). Verified by
  sheet dumps and an in-game screenshot (rounded outlined sprites, rider back view without a face).
- Advisory size heuristic exceeded in T3 and T4 (camera rewrite and sheet rework are each one coherent change).

- Review: native RDD over `cc19b02..c47e461` (medium, slice budget reached). Consent was granted; one lens (reliability) ran and the review was approved and acknowledged under lineage `review-6dff6ef05ecffbd5`.
  Advisory follow-ups (non-blocking):
  - The `resolve_among` fallback paths (no clear spot within range; clear of the world but still overlapping a body) have no test (`crates/core/src/world.rs:136-144`).
  - The cursor grab wiring has no test (`game/src/camera.rs:162-174`).

## Next step

Parent: native review of the work-unit commits (RDD), then the user's delivery decision.
