# First-person camera

Branch: `feat/first-person-camera` (from `origin/main` daecd9f). Uncommitted; the user commits.

## Objective

The user asked for this on 2026-10-25. Add a first-person camera, toggled with `V` against the third-person orbit, so the details we keep adding can be seen up close.

## Scope

- **Game:**
  - `game/src/first_person.rs` (new);
  - `camera.rs`: wiring, no orbiting and no fade line from the eyes;
  - `player.rs`: the player is the `ViewerBody`;
  - `console.rs`: `view fp|tp`;
  - `main.rs`.
- **Render:**
  - `crates/render/src/character/viewer.rs` (new): head bone marker, own-head render layer;
  - `atmosphere.rs`: `CloseShadows` and the first-person cascades, and the sun lights both layers;
  - `character/{mod,visual}.rs` and `lib.rs`: exports and the observer.
- **Docs:** `README.md` (camera text, controls, console table).

## Tasks

- [x] T1 Explore the camera, player, ride, console, character model, atmosphere and README. Route: inline (single writer).
- [x] T2 Render: `ViewerBody`/`ViewerHead`, `OWN_HEAD_LAYER`, the `mark_viewer_head` observer on `Insert<Rig>`, the sun on both layers, and `CloseShadows` with the 6 m first cascade.
- [x] T3 Game: `ViewMode`, `FirstPersonLook`, `EyeFollow`, `ViewBlend`, and the systems `toggle_view`, `turn_to_view` and `place_first_person`.
- [x] T4 Train interplay and the console `view fp|tp`.
- [x] T5 Tests and the sabotage pass.
- [x] T6 README and doc comments.
- [x] T7 Full checks (see Verification).
- [x] T8 (review fix, after the user played it) At the gallop, pieces of the face passed over the camera: the eased eye fell up to 0.19 m behind the head. Fix: cut the head out of the skin onto layer 1 by skin weights, once per skin asset, and clamp the eye lag to `MAX_LAG` = 3 cm.
- [x] T9 (coordinator follow-up) An orbit camera pulled in by a wall to within 0.6 m of the head stops drawing the player's model (`OWN_BODY_LAYER`), and draws it again past 0.8 m.

## Decisions

- **Key:** `V` was already bound, but only aboard (`RideView` cycling). The contexts are disjoint, so `V` toggles first person off the train and keeps cycling the ride views aboard. No new key is needed.
- **Train:** this was the simpler, robust option. Aboard, first person is disabled, because the existing window-seat ride view already is the seated passenger view. The `ViewMode` choice is kept; on getting off, the eyes come back at once, with no glide from the coach. The blend is forced to 0 aboard, so the seat keeps the orbit's lens and layers.
- **Eye position:** `EYE_IN_HEAD = (0, 0.04, 0.11)` m in the `head` bone frame, measured offline from `assets/characters/protagonist/model.gltf`. The eye centroid is (0, 0.0385, 0.0905) from the bone; the bone's rest frame is the model frame.
  - The offset from the player's feet is low-pass smoothed: rate 14/s on foot and 8/s in the saddle, jumping past 1.5 m. It is clamped to `MAX_LAG` = 0.03 m behind the eye point.
  - Measured on the shipped rider at the gallop (the render test prints it): the eye surges 0.56 m fore and aft, 0.09 m up and down and 0.11 m across, at up to 5.95 m/s. Unclamped, an eye eased at 8/s fell up to 0.188 m behind. With the clamp, the simulated gallop keeps the camera within 0.0300 m.
  - The view rotation comes only from the look, never from the head, so it stays level.
  - Without a model, the eyes fall back to 1.65 m on foot and 2.45 m riding.
- **Placement after propagation:** `place_first_person` runs in `PostUpdate`, after `TransformSystems::Propagate` (so the head bone has this frame's pose) and before frusta and light cascades. It writes both `Transform` and `GlobalTransform`.
- **Look:**
  - Pitch is clamped to ±80°.
  - Riding: the yaw is clamped to ±120° about the horse's heading, and the horse steers as before.
  - On foot: the yaw is free while standing; while walking (above 0.3 m/s) the body turns to the view at 6/s, and the view stays put.
  - Q/E and the mouse turn the head; WASD and Shift are unchanged.
  - Entering first person looks where the orbit looked. Leaving swings the orbit behind where the eyes looked.
- **Head cut (T8):**
  - `split_head` cuts the skin mesh (`is_skin`: the MakeHuman skin material) by skin weights. A vertex's head share is its weight on `head` and its descendants, plus `NECK_SHARE` = 0.75 times its weight on `neck_01`. A triangle whose vertices' share averages 0.5 or more goes to the head.
  - With the head's weight alone, the neck stayed up to 1.61 m (8 cm over the collar). At 0.75 the cut runs at 1.50–1.53 m, inside the shirt collar (1.53 m) and the neckerchief (1.56 m), so no hole or stub shows from the eye. On the protagonist, 1,010 of the skin's 2,088 triangles go to the head.
  - Both halves keep every vertex and share the same `SkinnedMesh`, so animation is unchanged. The head is a sibling entity (`OwnHead`) on layer 1 only, with the same material, transform, AABB and visibility range.
  - It is done once per skin asset in the `Insert<Rig>` observer, and the halves are cached for later dressings. A scene dressed again is not cut twice. There is no per-frame work.
- **Close orbit (T9):** every other mesh of the player's model goes on `OWN_BODY_LAYER` (2).
  - Layers per view: world = 0, 1, 2; first person = 0, 2; close orbit = 0. The sun keeps 0, 1, 2, so the shadow stays.
  - `BodyTooClose` hides the model under 0.6 m from the head's eye point and shows it again past 0.8 m.
  - Only the person is hidden; the horse and tack stay drawn.
- **Own body:**
  - Render puts the player's (`ViewerBody`) meshes for the hat, band, hair, eyes and eyebrows (`is_head_piece` by glTF material name) on render layer 1 only.
  - The follow camera sees layers 0 and 1, and drops layer 1 from the eyes.
  - The sun sees layers 0 and 1, so the shadow stays (Bevy 0.20 `check_dir_light_mesh_visibility` honours light layers).
  - The face is part of the body mesh, which is single-sided (not `doubleSided`), so it is culled from inside the head. The arms, hands, reins and the horse's neck and ears stay drawn.
- **Lens:**
  - The near plane is 0.05 m as soon as the blend starts.
  - The FOV widens from 45° to 75° vertical with the blend weight. The brief said "about 75–80°" and named no axis; Bevy's FOV is vertical. Tunable as `FOV`.
  - The layers and the 6 m first shadow cascade switch at the blend's halfway point.
  - The fog is camera-relative and unchanged.
- **Transition:** a 0.25 s smoothstep glide (position lerp, rotation slerp) from the orbit transform, which keeps being computed underneath without input.
- **Session memory:** the `ViewMode` resource.
- **Console:** `view fp` / `view tp`; it works aboard too and takes effect on getting off.

## Verification

### Sabotage log

Each sabotage was applied by a script that copies the file, replaces the text, runs the test and restores. Every restore was byte-identical (`filecmp`).

| Test | Sabotage | Result |
|---|---|---|
| render `viewer::only_the_viewers_head_and_hat_leave_the_world_layer` | skip the ancestor `ViewerBody` check | FAILED, restored |
| render `viewer::the_layers_hide_the_head_from_first_person_only` | `first_person_layers()` returns `world_layers()` | FAILED, restored |
| render `atmosphere::first_person_shortens_the_first_cascade_and_back` | `fit_cascades` always returns early | FAILED, restored |
| game `first_person::v_toggles_between_the_orbit_and_the_eyes_and_glides` | `toggle_view` never toggles | FAILED, restored |
| game `first_person::the_eyes_follow_the_head_bone` | ignore the `ViewerHead` (always the fallback) | FAILED, restored |
| game `first_person::pitch_is_clamped` | drop the pitch clamp | FAILED, restored |
| game `first_person::in_the_saddle_the_head_turns_within_reach_of_the_horses_heading` | drop the riding yaw clamp | FAILED, restored |
| game `first_person::on_foot_the_body_turns_to_the_view_only_while_walking` | the body never turns | FAILED, restored |
| game `first_person::aboard_v_keeps_cycling_the_ride_views_and_the_eyes_come_back_off_the_train` | `V` toggles aboard | FAILED, restored |
| same | no snap back to the eyes after getting off | FAILED, restored |
| game `first_person::a_frame_from_the_eyes_allocates_nothing` | `vec![0u8; 1]` in `EyeFollow::follow` | FAILED, restored |
| game `console::view_switches_between_the_eyes_and_the_orbit` and `console::parses_every_command` | `view fp` parsed as `View(false)` | both FAILED, restored |
| game `console::everything_the_console_prints_is_in_the_font` | `{}` in the new help line | FAILED, restored |
| render `viewer::the_players_head_is_cut_from_his_skin_onto_the_hidden_layer` (shipped model) | body keeps every triangle (head ones too) | FAILED, restored |
| same | the cut head also on layer 0 | FAILED, restored |
| same | `NECK_SHARE` = 0 (neck stub over the collar) | FAILED, restored |
| same | `is_cut` never true (cut twice) | FAILED, restored |
| render `viewer::only_the_viewers_head_and_hat_leave_the_world_layer` (changed) | the body meshes on layer 0, not `OWN_BODY_LAYER` | FAILED, restored |
| render `viewer::the_layers_hide_the_head_from_first_person_only` (changed) | `close_layers()` returns `world_layers()` | FAILED, restored |
| render `atmosphere::first_person_shortens_the_first_cascade_and_back` (changed) | the sun on `first_person_layers()` | FAILED, restored |
| game `first_person::at_the_gallop_the_eyes_never_fall_behind_the_head` and `the_eyes_follow_the_head_bone` (changed) | no lag clamp | both FAILED, restored |
| game `first_person::an_orbit_camera_against_the_player_stops_drawing_him` | no hysteresis (show at ≥ 0.6 m) | FAILED, restored |
| same | never hide (world layers when close) | FAILED, restored |

The per-frame allocation test uses a test-only counting `#[global_allocator]` with a thread-local counter. It covers the frame's math (look, eye follow, blend, lens, transform); the system around it only adds queries.

### Checks

Observed on 2026-10-25:

- `cargo test -p pampa-core`: 345 passed.
- `cargo test -p pampa-content`: 32 passed.
- `cargo test -p pampa-render`: 324 passed, 3 ignored. The ignored ones were already there.
- `cargo test -p game`: 128 passed.
- `cargo build -p game`: ok.
- `cargo fmt --all -- --check`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.

The diff is about 1,890 changed lines, new files included (after T8 and T9).

## Next step

- Try it in game: `cargo run -p game`. Get off the train, press `V`, walk and look round, mount a horse, gallop, and look back over either shoulder.
- Tune by eye if needed: `EYE_IN_HEAD` (z, so the nose never shows), `FOV`, the eye smoothing rates, `FIRST_PERSON_CASCADE`.
- Not covered by headless tests: the real model's mesh names on a live scene (the observer is unit-tested with the protagonist's material names), and how the shirt collar or poncho look when looking straight down.
