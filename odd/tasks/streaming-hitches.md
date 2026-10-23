# Streaming hitches and late minimap rails

Branch: `fix/streaming-hitches`.

## Problem

1. Short stalls while riding or seated in the train. An earlier
   investigation measured 50–70 ms frames about every 18.5 s, on both
   `main` and the KTX2 branch.
2. The minimap sometimes shows the railway only seconds late, even after
   the rider is standing on the rails.

## Evidence

Harness: a temporary ignored test in `game` (removed after use). It runs
a headless app with the shipped world and the streaming plugins (terrain,
flora, rail, roads, fences, settlements, towns, thin fade, minimap). It
moves the focus and the player along the line's full path at 65 km/h with
a 1/60 s step for 13292 frames, and times each `app.update()` (dev
profile).

| | frames > 25 ms | > 50 ms | max | chunk-frame mean |
|---|---|---|---|---|
| before (HEAD worktree) | 23 | 7 | 55.2 ms | 16.1 ms |
| after | 0 | 0 | 12.6 ms | 2.2 ms |

- All spikes fell on frames that spawned a terrain chunk. Frames without
  a chunk spawn stayed at or under 2.8 ms.
- Ablation: the terrain alone gave a 48.5 ms max. The decorators added
  only about 1.5 ms on average.
- Per chunk, before: `build_chunk_mesh` took 13.8 ms on average (median
  8.6, p90 30.3, max 49.8 ms). `chunk_props` took 0.2 ms.
- Rhythm: chunks are 200 m. One chunk border every 18.5 s is a ride at
  about 10.8 m/s, roughly the 12.2 m/s gallop.

## Cause

- **Hitches.** Each streamed chunk's ground mesh was built on the main
  thread. Every grid corner was sampled once per quad touching it (four
  times). Each sample costs six `Terrain::height` calls plus the road
  distance, and the road cut makes `height` costly near roads.
- **Minimap.** A town's frame shows the town's extent plus `MARGIN`
  (30 m), but the map stayed on it until the rider was 150 m from the
  town. Between 30 m and 150 m the rider was off the map, pinned to its
  edge, and the rails under them were not drawn. The rails only appeared
  when the field frame took over at 150 m.

## Fix

- [x] T1: `build_chunk_mesh` samples each corner once (a 41x41 grid) and
  builds the same mesh. Per chunk: 4.1 ms mean, 13.3 ms max.
  Route: inline.
- [x] T2: a chunk streamed in builds its ground on the
  `AsyncComputeTaskPool`. It spawns at once with its props and
  decorations, and `land_ground` gives it its `Mesh3d` when the task is
  done. A despawned chunk drops its task. The startup fill stays inline.
  Route: inline.
- [x] T3: the minimap's `FIELD_ENTER` is now `MARGIN - 10` (20 m) and
  `TOWN_ENTER` is 5 m. The rider never rides off a town map, and the
  field frame, with the railway, takes over as they leave the town.
  Route: inline.

## Checks

- RED → GREEN: `terrain::tests::a_streamed_chunk_builds_its_ground_off_the_main_schedule`
  (a streamed chunk has no ground mesh the frame it spawns, then gets the
  same mesh the main thread would build).
- RED → GREEN: `minimap::tests::a_rider_on_the_rails_just_outside_a_town_sees_them_under_the_arrow`.
- The minimap hysteresis tests now read the thresholds from the
  constants.

## Playtest follow-ups (Renoir iGPU, dev build)

Four reports after the first fix. Route: inline, one writer.

### T4: the lean-to roof (rancho/chacra `AdobeIron`)

- Evidence: `lean_to` closed the gap under the sloped sheet with one
  zero-thickness triangle per side, on the side walls' outer faces, and
  nothing over the front wall: a 0.7 m open band between the front wall top
  and the sheet. Seen through that band, the far gable is a back face and is
  culled, so the gables read as open triangles. The sheet was high over the
  front, so from the patio (and the train) only its plain underside showed;
  the ridges (`corrugated_iron_02` runs them along `v`, one every ~5.7 cm)
  were on the top face, out of sight. The overhang was 0.35 m (not oversized
  in the geometry).
- Fix: the sheet now falls to the front (high over the back), and every
  wall is built up to its underside as a solid `wedge`: the side walls as
  full-depth gables, the back and front as sloped bands. Overhang stays
  0.35 m at eaves and verges.
- `gable_roof` (paja, galpón): its sheet meets the front/back walls' outer
  top edges and its gables are on the side walls' outer faces; the only gap
  is the wedge inside the wall thickness, hidden from outside. Left as is.
  `parapet_roof` is flat behind a parapet: no slope, no gap.
- Checks: RED → GREEN `a_lean_to_rests_on_all_four_walls` (wall tops within
  3 cm of the underside along every wall; before: 3.40 m vs 3.44 m at the
  first sample) and `the_lean_to_corrugation_runs_down_its_slope` (top face
  `v` runs down the slope, `u` along it, facing the front; RED on the
  facing). `a_lean_to_overhangs_its_walls_a_little` (0.3–0.6 m) passed
  before and after.

### T5: the field minimap's railway looked dashed

- Evidence: `track_line` drew a 2 px line and a 4 px sleeper tick every
  third pixel. The 320 px map is shown at 220 px with linear sampling, so
  the period-3 ticks beat against the sampling into dashes. No pixel gaps
  were found along the line at any angle.
- Fix: the field frame draws the railway without ticks, one even 2 px
  line, stepped every half pixel. The town maps keep their ticks.
- Checks: RED → GREEN `the_field_draws_the_railway_as_one_even_unbroken_line`
  (8 angles, sub-pixel offsets: every point on the line is a rail pixel and
  the cross width stays 2–3 px; before: widths {2, 4}).

### T6: small hitches riding the train

- Evidence: a temporary harness (an ignored test in `game`, removed after
  use) ran the shipped opening and then a full ride from Monte Nievas to the
  end of the line (5032 frames at the test step of 0.05 s), with the ride,
  player and camera plugins and every streaming plugin (terrain, flora,
  rail, roads, fences, settlements, towns, thin fade, minimap, train).
  Ablation by plugin: only the settlement frames stood out (9 frames, mean
  4.9 ms, max 9.2 ms with settlements alone); flora, fences and roads add
  1–2 ms to a chunk frame.

| train ride, dev profile | > 16.7 ms | > 25 ms | max | settlement frames mean / max |
|---|---|---|---|---|
| before | 0 | 0 | 11.0 ms | 6.2 / 11.0 ms |
| after | 0 | 0 | 7.5 ms | 2.6 / 5.2 ms |

- Fix: a chunk streamed in (ground still building) builds its settlements
  on the `AsyncComputeTaskPool`: `PaintKit` owns what the painter borrows
  (handles, a terrain that shares its roads), and `land_settlements` adds
  the meshes and spawns the pieces when the task is done. The first fill is
  still laid inline. A chunk despawned first drops the task.
- Checks: RED → GREEN
  `a_streamed_chunk_lays_its_settlements_off_the_main_schedule` (nothing
  the frame the chunk spawns, then the same parts the inline path lays).
  `a_chunk_gone_before_its_settlements_land_drops_them` and
  `terrain::tests::a_chunk_gone_before_its_ground_lands_drops_the_build`
  cover a chunk despawned the frame after it streams in. The async tests
  wait on the tasks themselves (`finish_builds`, then one update lands
  them) instead of sleeping for wall-clock time.
- On this machine no frame of the ride goes over 16.7 ms on the CPU. What
  the iGPU may still show is GPU work the headless harness cannot see:
  uploading new meshes and textures, and first-use pipeline compilation.

### T7: seated facing forward, looking round with the mouse

- Evidence: `seat_view` faced the window (a quarter turn from the
  direction of travel), and the look was reset only on boarding.
- Fix: the seat faces the coach's +X (`PlacedCar::direction`, the
  direction of travel); the mouse (or Q/E) turns the head up to 120° either
  way and 40° up or down. The head turns forward again whenever the seat is
  taken: boarding (the opening too), a skipped ride coming in (the trip's
  jump at black, `Fading` → `Arriving`), and `V` back to the window view.
  An ordinary arrival keeps the head where the player turned it. `V` still cycles window → exterior → train.
- Checks: RED → GREEN
  `every_ride_starts_seated_facing_forward_and_the_mouse_looks_round`
  (opening, mouse look and its clamp, `V` back, a normal boarding at Monte
  Nievas, a skip), `an_ordinary_arrival_keeps_the_head_where_it_was_turned`
  and the updated
  `aboard_the_camera_sits_by_a_coach_window_and_rides_with_the_train`.

## Deferred

- GPU upload of new meshes and first-use pipeline compilation on the
  integrated GPU are not measured by the headless harness.
