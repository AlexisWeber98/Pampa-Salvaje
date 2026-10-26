# Review follow-ups (first-person camera 41f0245, facades phase 2 5114b0c)

## Objective

Fix the warnings from the retro native reviews of 2026-10-26 before facades
phase 3.

## Tasks

- [x] R1 Lights and the player's layers. `crates/render/src/character/viewer.rs:256-264`
  moves all of the player's meshes to layers 1 (head) and 2 (body). Only the
  sun and the follow camera see those layers, so the street lamps' point
  lights (`crates/render/src/lamps.rs`, layer 0) no longer light the player
  or cast his shadow at night. Every light that should touch the player
  must include layers 0–2. Check all lights: lamps, the train lamps if any,
  any other point or spot lights. Add a test.
- [x] R2 Projection reset in the first-person glide (`game/src/first_person.rs:364-368`).
  The glide replaces `Projection` with one built from
  `PerspectiveProjection::default()` every frame, which resets `aspect_ratio`
  and `far`. Mutate `fov` and `near` in place instead. Test that the aspect
  ratio and far plane survive a toggle.
- [x] R3 Unbounded texture lift (`crates/render/src/facade/materials.rs:230-232`).
  Paving gets about 2.5× and clips. Bound the gain (e.g. ≤ 1.6), and/or fold
  the rest into the target colour. Test: no lifted material's gain passes the
  bound, and the clipped fraction of the scan stays small.
- [x] R4 Sidewalks on the railway band (`crates/core/src/town.rs:157-169`).
  In a through-railway big town (the shipped General Pico),
  `Paving::Everywhere` may lay block rings onto the track or station band.
  Test with the shipped Pico; keep strips off the band and off the platform.
- [x] R5 Suggestions:
  - `viewer.rs` `is_skin`: match the MakeHuman skin material exactly, not
    "male"/"skin" substrings;
  - more than one `ViewerHead` during a model swap: pick the current
    player's, deterministically.

Route: one delegated writer (trigger: 2+ non-trivial files, in render and
game). Uncommitted; the user commits.

## Outcome

- **R1 (lights and layers).** Fixed defensively; the visible bug as
  described does not happen in Bevy 0.20. Read in `bevy_pbr-0.20.0`
  and `bevy_light-0.20.0`: a light's `RenderLayers` decide two things.
  One is whether a camera's clusters take the light
  (`cluster/assign.rs:491`, light ∩ camera layers). The other is which
  meshes go into the light's shadow maps (`bevy_light/src/lib.rs:611`,
  `render/light.rs:2750`). Shading is never filtered by a mesh's own
  layers. Every camera keeps layer 0, so the layer-0 lamps already lit the
  player. They cast no shadows (`shadow_maps_enabled: false`). The lamp
  pool now carries `world_layers()` (0, 1, 2) like the sun, so the player
  stays in its shadow maps if lamp shadows are ever turned on. Those two
  are the only lights: no other `PointLight`, `SpotLight` or
  `DirectionalLight` is spawned, and the train's lamps are emissive glass
  only. The camera layers are unchanged (`first_person_layers` 0+2,
  `close_layers` 0, `world_layers` 0+1+2). Their tests are untouched and
  pass.
- **R2 (projection reset).** `set_lens` writes only `fov` and `near` into
  the camera's `PerspectiveProjection`, in place, and only when they
  change. `aspect_ratio` and `far` survive the glide both ways.
- **R3 (texture lift).** Measured first: the lifts reach 2.48 (paving),
  2.33 (band), 1.84 (lime white) and 1.83 (washed brick). Decoding each
  scan's `color.jpg`, no texel goes past white, except 0.014% of the
  patched brick and 0.003% of the paving. "Paving clips" did not hold, so
  the bound is `MAX_LIFT` = 2.5. That leaves every shipped look as
  approved. A bound of 1.6 would have darkened the lime-white paint, the
  washed brick, the band and the paving with no clipping to remove. Past
  the bound, `lift()` scales the three gains as one (hue kept), folding
  the rest into a darker shade of the target. The test reads the shipped
  scans and caps the clipped share at 0.1%.
- **R4 (sidewalks on the railway band).** Not reproduced: in the shipped
  General Pico, every block ring lies on a street (1.8 m into a 10 m
  street). The band starts past the halves' ring streets, so no strip
  touches the band, the track corridor, the platform or its ramps. Core
  code is unchanged. A game test on the shipped file guards it.
- **R5.**
  - `is_skin` matches the MakeHuman skin name exactly:
    `<young|middleage|old>_<caucasian|cauasian|african|asian>_<male|female>`
    with an optional `_detailed`. `cauasian` is the misspelling our four
    middle-aged men ship with. A test reads every shipped model's glTF
    materials: exactly one skin each.
  - `ViewerHead`, render side: when a model of a `ViewerBody` is dressed,
    every other `ViewerHead` under that same body is unmarked. There is
    one head per body, the model dressed last.
  - `ViewerHead`, game side: `place_first_person` takes only a head under
    the player's entity, so another model's head is never used.

## Verification (2026-10-26)

- `cargo test -p pampa-core`: 349 passed.
- `cargo test -p pampa-content`: 32 passed.
- `cargo test -p pampa-render`: 333 passed, 4 ignored (the previews).
  The first full run failed one test: the untouched
  `terrain::a_chunk_gone_before_its_ground_lands_drops_the_build`, while
  another worktree was compiling. It passed three times alone and in the
  full rerun, so it is timing-sensitive under load and unrelated to this
  change.
- `cargo test -p game`: 134 passed.
- `cargo build -p game`: ok.
- `cargo fmt --all -- --check`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.

Diff: about 430 changed lines, mostly tests.

## Next

- In game, at night (`time 21`), stand by a street lamp in first person
  and in the orbit. The player should be lit as before; this was never
  broken (see R1).
- Toggle `V` with the window at a wide aspect ratio. The view should not
  stretch while gliding in or out (R2).
- If the user wants the paving or band less lifted, `MAX_LIFT` is the one
  knob: at 1.6 they come out a darker shade of their targets.

### Sabotage log

Each sabotage was applied by a script (scratchpad `sabotage.py`): it
replaces the text, runs the test, restores the file and checks that the
sha256 is equal. Every test below failed under its sabotage, and every
restore was byte-identical.

| Sabotage | Test seen failing |
|---|---|
| lamp light spawned without `world_layers()` | `lamps::by_night_the_pool_lights_only_the_nearest_lamps` (changed) |
| glide replaces the whole `Projection` again | `first_person::the_glide_keeps_the_viewports_aspect_ratio_and_the_far_plane` |
| lift not folded at `MAX_LIFT` | `materials::no_lift_passes_the_bound_and_a_steeper_one_folds_into_its_target` |
| lift three times steeper | `materials::the_lifted_scans_hardly_clip` |
| a ring of sidewalks inside the railway band (core `sidewalks`) | `towns::general_picos_sidewalks_keep_off_the_railway_band_and_the_platform` (game) |
| `is_skin` by `male`/`skin` substring again | `viewer::the_skin_is_the_one_makehuman_skin_material_of_each_shipped_model` |
| old head keeps its `ViewerHead` | `viewer::a_model_swapped_in_takes_the_head_mark_from_the_old_one` |
| eyes in the first `ViewerHead` found | `first_person::the_eyes_sit_in_the_players_own_head_not_anothers` |

The game's head tests (`the_eyes_follow_the_head_bone`, the gallop and
close-orbit tests) now put the head bone under the player, as a real
model's is (`put_head`). Their assertions are unchanged.

## Checks

- `cargo test -p pampa-core`, `-p pampa-content`, `-p pampa-render`, `-p game`
- `cargo build -p game`, `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
