# 3D characters (MakeHuman people, Quaternius ridden horse)

## Objective (user, 2026-10-24)

Replace the procedural people and the ridden horse with the 3D models, so
nothing procedural is refined further. Animation source "option A": drive
the MakeHuman `game_engine` bones from pampa_core's existing `JointAngles`.
CC0 humanoid clips ("option B") come in a later PR, so keep the animation
source swappable.

## Delivery: three chained PRs (user, 2026-10-24)

1. **PR 1, protagonist on foot** (branch `feat/3d-protagonist`, stacked on
   `feat/smooth-shading` until that merges):
   - T1 the bone driver;
   - T2 the protagonist's model on foot;
   - T10 the flora `bake` fix.
2. **PR 2, the ridden horse**:
   - T3 the Quaternius horse driven by `VisualPose`, `HorseGait` and clips;
     there is no trot clip, so stretch Walk up to about 4.5 m/s, then
     Gallop;
   - T4 the recado and bridle on the horse's bones;
   - T5 the rider seated on the `Back` bone;
   - T6 mount, dismount and tied horses.
3. **PR 3, townsfolk**:
   - T7 five MakeHuman outfits, with converter pieces for the skirt, shawl
     and headscarf;
   - T8 performance: at most about 8k triangles, freezing beyond 120 m, the
     shared range.

## Map (read-only, 2026-10-24)

- **Visuals and frames.** `CharacterVisual` and `attach_visuals` are in
  `crates/render/src/character/visual.rs:21-156`. The rigid figure works
  as the "carrier":
  - the MakeHuman model is a child of `FigureBody` (`model.rs:128-145`);
  - the rigid joints are hidden;
  - `breathe` writes `spine_02` (`model.rs:254-260`).
- **JointAngles** (`crates/core/src/pose.rs:14-40`):
  - fields: hip, knee, shoulder, elbow (left and right), lean, bob,
    breath, nod, spread, hat, drape;
  - axes: body frame +X front, +Y up, left −Z; positive Z swings a limb
    forward (`figure.rs:162-186`);
  - rest pose: standing, arms hanging down (`figure.rs:136-149`).
- **game_engine rig**: 53 joints. Bones point along local +Y, and every bone
  has a non-trivial rest rotation, so deltas must be conjugated into model
  space. Chain: Root, pelvis, spine_01..03, clavicle, upperarm, lowerarm,
  hand, neck_01, head, thigh, calf, foot, ball. The model faces +Z, turned
  `Ry(π/2)` to +X. The arms are already lowered offline by mh2gltf.
- **Flora bug** (from the review of 6c960ee): `flora.rs:599-639` `bake()`
  advances one shared cursor by `group.normals.len()`. A group with
  positions ≠ normals, or a count that is not a multiple of 3, desyncs the
  later groups.

## Tasks (PR 1)

- [x] T1 Bone driver: `crates/render/src/character/rig.rs`.
  - Map the figure joints to bones: pelvis, spine_01..03, neck_01 and
    head, clavicles, upperarm, lowerarm, thigh, calf, foot (ball_l/r read
    only).
  - Cache each bone's rest pose, and apply the `JointAngles` deltas
    conjugated into model space.
  - Fold breathing in.
  - Keep the animation source behind one seam.
  - Proof: `character::rig::tests::*` (17 tests), sabotage table below.
- [x] T2 Protagonist on foot uses `assets/characters/protagonist/model.gltf`.
  The rider keeps the rigid figure until PR 2.
  - Proof: `model::tests::the_protagonist_and_the_errand_people_have_models_and_each_is_shipped`,
    `model::tests::the_protagonist_wears_his_model_on_foot_but_not_in_the_saddle`,
    walk and jog contact tests, previews.
- [x] T2b (added 2026-10-24, user feedback "super rigid") Relaxed idle and
  walk posture: contrapposto, loose arms, curled fingers, slow sway, spine S,
  level head; walking pelvis turn and drop with torso counter-rotation.
- [x] T10 Flora `bake`: per-group alignment, with validation and a test.
- [x] T9 (PR 1 part) Tests: sign tests (a positive hip swings the foot
  forward, the knee bends backward), and update the tests that pin "no
  model for the protagonist".

Route: one delegated writer (2+ non-trivial files).

## Decisions (PR 1)

- **Carrier.** The rigid figure stays as the hidden carrier: its body turns
  and bobs and `update_figures` still poses its 14 joints, so `models off`
  and the riders keep working untouched. `FigureBody` now keeps the
  `JointAngles` it computed (`angles`) and an eased `stillness` (0 walking,
  1 standing), so the pose maths is not duplicated.
- **Conjugation.** Figure turns are body-frame deltas (+X front, +Y up, left
  −Z; +Z swings a hanging limb forward). For a bone with parent rest
  orientation `f` in the body frame: `local = f⁻¹ · d · f · rest`. Down the
  chain each bone ends up turned in the body frame by its own and its
  ancestors' turns. The thighs are the one exception: their turns hold in
  the body frame whatever the pelvis does.
- **Bone mapping.** The bob is not copied: the model's pelvis undoes the
  carrier's bob and sinks (up to 8 cm) until the lower ankle is at its rest
  height. The lean is split a third per spine bone, plus breath on
  spine_02. The neck's nod and glance are split half and half over
  neck_01 and head. Shoulder, elbow, hip (with spread) and knee map one to
  one. The feet are kept level, with toes out 0.12.
- **Seam.** `rig_pose(&FigureBody) -> RigPose` is the only place the pose
  comes from. It calls `joint_pose(angles, glance, bearing)`, the figure's
  angles plus `posture`. `Rig::solve` turns a `RigPose` into local
  transforms: it sets the support-leg IK, the plant sink and the toe touch.
  A clip player (PR 3, option B) only has to yield a `RigPose`.
- **Gait (pampa-core `pose.rs`).** The old gait made the stance foot float
  about 10 cm and slide up to 1.6 times the ground speed when jogging. The
  fix:
  - the walk's hip follows `foot_sweep`: the foot moves back at an even
    pace for half a cycle, then eases forward;
  - walk cadence is speed/1.4;
  - the jog's hip swing is 0.5 rad, with cadence speed/2.7.
  Measured slip: walk 0.03 m/s at 1.67 m/s, jog 0.4 m/s at 5 m/s. The rigid
  figures get the same cadences.
- **Posture.** It is on foot only; riders get none. All values are in
  `rig::relaxed`:
  - the free leg comes from the seed (`Stance::of`);
  - the pelvis drops 0.06 toward the free leg and shifts 2.5 cm over the
    support foot;
  - the free knee bends 0.22, the support knee 0.05;
  - the support foot is held by a small leg IK, and the free toe touches
    down;
  - the spine has an S (0.035 back, 0.045 forward) and counter-rolls, and
    the head is kept level;
  - the clavicles go down 0.05 and forward 0.06, the arms out 0.09 and
    forward 0.06 with per-seed differences, and the elbows bend 0.13 more;
  - the fingers are curled once when the model spawns
    (`FINGER_CURL = [0.2, 0.35, 0.25]`), the thumbs are left as they are;
  - the sway lasts 6–9 whole seconds, so it runs seamlessly across the
    27,720 s clock wrap;
  - walking, the pelvis turns 0.1 × the hips' spread and drops 0.05 × the
    knees' difference, and the spine turns back 1.6× against it.
- **Freeze.** It reuses `fauna::livestock::frozen`: a model out of view or
  beyond 120 m (resuming within 110 m) keeps its last pose. The protagonist
  never freezes; he is only ever the player. There is no new
  VisibilityRange: the models keep `settlement::ranges()[1]`.
- **Breath.** The `Breath` component and the `breathe` system are gone.
  spine_02 rocks by `BREATH_PITCH × angles.breath` inside the driver, so
  nothing else writes that bone.

## Sabotage log (user's standing rule)

Each test was failed on purpose by the sabotage listed, then the code was
restored. Every sabotage below made its test fail, and `git diff --stat`
was checked after each batch.

| Test | Sabotage → observed failure |
|---|---|
| flora `a_malformed_group_is_skipped_and_the_others_keep_their_own_vertices` | old cursor code → 8 vertices instead of 3 (first version). The first version survived "offset never advances", so the test was strengthened; now it fails on that, and on "validation skipped" (`left == right`) |
| `rig::every_bone_has_its_own_index_and_name` | Spine01 indexed as 1 → index assert |
| `rig::the_lean_and_the_nod_add_up_over_their_chains` | lean split in halves → spine angle assert |
| `rig::a_turn_in_the_body_frame_reads_the_same_on_a_turned_bone` | conjugation flipped → angle assert |
| `rig::the_protagonists_bones_turn_the_way_the_figures_joints_do` | conjugation flipped; no conjugation → hip/foot sign assert |
| `rig::the_drawn_pose_is_worked_out_afresh_each_frame` (replaces the breath test) | rotation accumulated each frame → drawn ≠ solved; the earlier breath test failed the same way |
| `rig::the_protagonist_walks_with_his_legs_and_arms_in_opposition` | upper arms take the hip turn → opposition assert |
| `rig::a_model_far_off_or_out_of_view_keeps_its_pose_but_the_protagonists_never_does` | never freeze → "Hidalgo"; player freezes → "Protagonist"; visibility ignored → "Hidalgo" |
| `rig::each_person_stands_their_own_way` | everyone rests the left leg; free knee not bent |
| `rig::the_weight_sways_slowly_within_bounds_and_comes_back` | period 10% off; sway grows with the clock; no sway |
| `rig::a_rider_keeps_the_figures_pose` | posture on riders too |
| `rig::fingers_curl_and_nothing_else_does` | thumbs curled too → `thumb_02_l` |
| `rig::the_fingers_curl_toward_the_palm` | curl backward; curl never applied at dress time. The first version survived "curl backward" because it compared against its own inverse; it now reads the straight rest from the glTF |
| `rig::a_raised_free_foot_points_its_toe_to_the_ground` | toe never points. Added because `standing_…` survived "no toe touch" |
| `rig::walking_a_foot_stays_on_the_ground_and_keeps_still_on_it` | hips never sink; carrier bob not undone |
| `rig::jogging_the_feet_never_go_through_the_ground` | plants the higher foot → through ground; jog cadence back to speed/2 → slip assert |
| `rig::standing_the_supporting_foot_stays_put_and_the_free_one_touches_its_toe_down` | no support-leg IK |
| `rig::walking_the_feet_stay_level` | no foot counter-turn |
| `model::the_protagonist_and_the_errand_people_have_models_and_each_is_shipped` | Protagonist → None |
| `model::the_protagonist_wears_his_model_on_foot_but_not_in_the_saddle` | toggle no longer skips riders → count 2 |
| core `pose::a_foot_on_the_ground_keeps_pace_with_it` | walk /1.3; jog /2.0 |
| core `pose::a_walking_foot_sweeps_back_evenly_while_its_leg_is_straight` | walk hip back to a sinusoid |
| core `pose::walking_legs_swing_in_opposite_phase_and_arms_against_them` (changed) | right leg in phase with the left |
| core `pose::jogging_swings_wider_and_leans_forward` (threshold changed) | jog hip 0.42 |
| `visual::a_walking_figure_turns_to_its_heading_and_swings_its_legs` (changed) | right leg in phase with the left |

RED before code was seen only for the flora test and the `model_name` test.
The driver and posture tests were written together with the code, so they
have sabotage evidence instead.

Tests changed:
- `model::only_the_errand_people_have_models_and_each_is_shipped` was
  renamed to `the_protagonist_and_the_errand_people_have_models_and_each_is_shipped`;
- `model::a_breath_rocks_around_the_rest_and_back` was removed along with
  `Breath`;
- the core walk opposite-phase test and the visual walk test now compare
  phases;
- the core jog-wider threshold went from 0.1 to 0.05.

The game tests that pin the player's visual still pass unchanged:
- `player::a_player_spawned_on_foot_walks_and_jogs_at_a_persons_pace`;
- `player::the_default_spawn_is_on_horseback`;
- `errand::mounting_swaps_the_clerk_figure_for_one_horse_and_rider_and_nothing_else`.
They check `CharacterVisual`, which did not change.

## Checks

- `cargo test -p pampa-core`, `-p pampa-content`, `-p pampa-render`, `-p game`
- `cargo build -p game`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`

## Verification (2026-10-24, PR 1)

- `cargo test -p pampa-core`: 352 passed
- `cargo test -p pampa-content`: 32 passed
- `cargo test -p pampa-render`: 265 passed
- `cargo test -p game`: 117 passed
- `cargo build -p game`: ok
- `cargo fmt --all -- --check`: ok
- `cargo clippy --workspace --all-targets -- -D warnings`: ok

Previews were rendered offline (CPU-skinned, flat colours) in the session
scratchpad: walk, jog, idle and Hidalgo idle, from the side and the front.
Nothing was checked in game yet.

## Progress

- 2026-10-24: `feat/3d-protagonist` branched from `feat/smooth-shading`
  (6c960ee, not merged yet).
- 2026-10-24: PR 1 implemented (T1, T2, T2b, T10, T9). It is uncommitted:
  the user commits. Native review is pending, under the user's RDD switch.

## Next step

- Look at it in game: `goto nievas` (Don Hidalgo standing), the plaza (the
  protagonist idle), then walk and jog. Check the free foot, the sway, and
  the foot contact on slopes. The driver plants on the body's flat ground
  only; there is no terrain IK.
- PR 2: the ridden horse (T3–T6). The rider needs its own `RigPose` source
  (`Bearing.on_foot = false` already skips the posture and the plant).
