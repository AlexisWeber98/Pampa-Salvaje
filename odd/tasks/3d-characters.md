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

## Option B now (user, 2026-10-24)

In game, the user found PR 1's procedural animation still very rigid, and
the walk and jog worse. It also looks "square": the `male1591` proxy has
too few loops at the joints. Option A's ceiling is reached, so we move to
clips now.

- PR 1 is committed as f0bac73, local, as the base.
- Next, on the same branch: CC0 clips from the Quaternius Universal
  Animation Library 1 and 2, retargeted to `game_engine`. The user
  downloaded them on 2026-10-24 to `~/Downloads/quaternius/ual/`:
  - UAL1 `AnimationLibrary_Godot_Standard.glb`: Rigify `DEF-*` names,
    53 joints, 46 clips;
  - UAL2 `UAL2_Standard.glb`: UE names that match game_engine almost 1:1,
    65 joints, 43 clips.
- After that, a separate PR: a higher-resolution body proxy and clothing
  refinement. Both regenerate the models.

### Tasks (clips)

Route: one delegated writer (2+ non-trivial files: tool, runtime, game hook,
docs).

- [x] C1 `tools/retarget.py` (stdlib Python, reuses `glbtrim`'s bounded GLB
  reader) with `tools/test_retarget.py` (21 tests) and sabotages:
  - bone maps UAL1 (Rigify) and UAL2 (Unreal) to `game_engine`'s 52 bones,
    fingers included;
  - rest alignment in the shared world frame (details under Decisions);
  - pelvis moves scaled by leg length (1.043 for the protagonist), loops held
    in place, feet kept above the ground;
  - 17 clips in a fixed order, the walks' and runs' measured ground speed and
    stride in each animation's `extras`;
  - one shared `assets/characters/animations/humanoid.glb` (1.1 MB): `Root`
    and its bones at rest, no mesh;
  - credits in `CREDITS.md` and `assets/characters/CREDITS.md`.
- [x] C2 Runtime (`crates/render/src/character/{clips,rig,model}.rs`):
  - one shared `AnimationGraph`, loaded once at startup;
  - idle, walk, jog and sprint weighed by speed, eased, each paced to
    `speed / natural speed` (clamped 0.5–2), with a seeded phase per person;
  - per-person idles: Farach folds his arms; anyone speaking plays `idle_talk`
    (`Talking`, set by the game's `errand::mark_speakers`);
  - the glance stays as an overlay; PR 1's posture, gait, foot IK, sink,
    finger curl and breath code is deleted, along with `FigureBody.angles` and
    `.stillness`;
  - riders unchanged (rigid figure until PR 2);
  - freezing as before, plus the PR 1 review findings: "any mesh in view",
    and the camera chosen deterministically (livestock too).
- [x] C3 Tests with sabotages (log below), previews, docs (this file,
  `tools/README.md`, `assets/characters/README.md`, `README.md`).

## Decisions (option B, 2026-10-24)

- **Retarget maths.** Every bone works in the shared world frame: both rigs
  are Y-up, face +Z, and have their left at +X. Each bone's world turn away
  from the source's *reference* pose, `D = W(t) · Ref⁻¹`, is applied to the
  target's world rest (`D · Rest`), then made local under the target
  parent's posed rotation. Bone roll in either rig never enters, so no limb
  twists and no sign flips.
- **Reference (rest alignment).** UAL rests in a T-pose; MakeHuman rests
  with the arms down. Down the hierarchy, each source bone keeps its rest
  turn under its parent, then swings by the shortest arc until it points
  where the target bone points. The pelvis is not swung: its axis is only a
  convention (the source's leans forward, the target's back).
  - Deviation from the brief: a source pose equal to this *reference* yields
    the target rest. A source pose equal to the raw T-pose rest yields a
    T-posed target, which is physically right. Mapping the raw rest to the
    target rest would push the idle's lowered arms into the body. The tests
    pin both cases.
- **Aim at the next joint, not along +Y** (fixed after the coordinator's
  review of the first previews). A `game_engine` bone's child can sit off
  its +Y: `spine_03`'s neck is 16° forward of it. Aiming the axes pitched
  the torso about 7.5° further forward than the source. Each bone now aims
  at the next joint of its chain; chain ends (head, balls of the feet,
  finger tips) use +Y.
  - The pelvis-to-neck line now matches the source within 3.5° through the
    walk, idle and folded arms, and the neck-to-head line within 1°. The
    remaining few degrees are the bodies' shapes: at rest the source's torso
    leans 4°, the target's 1°, and the spines split their length
    differently.
- **Pelvis.**
  - Its world move is scaled by the leg ratio (hip to ankle).
  - Loops lose their horizontal drift, keeping the bob and the sway.
  - The pelvis is raised wherever the differently built target would put an
    ankle or a ball of the foot below its rest height. Folded arms sank the
    toes 3 cm without it; the source itself sinks 1.3 cm there.
- **Speeds** (measured at the protagonist's scale; the median slide of a
  foot point within 1 cm of its lowest height, plus the drift):
  | Clip | Speed (m/s) | Stride (m/cycle) |
  |---|---|---|
  | walk | 1.0965 | 1.46 |
  | walk_formal | 1.0964 | 1.46 |
  | jog | 6.1488 | 5.74 |
  | sprint | 9.2842 | 6.19 |
  | walk_carry | 0.6968 | 1.39 |
  - The game walks at 1.67 m/s, so the walk plays 1.52× faster than
    recorded. It jogs at 5 m/s, so the jog plays at 0.81×.
  - Quaternius' "jog" is a long-stride run with strong arm pumping.
  - `Clip::ground_speed` copies the speeds, and a test reads them back from
    the GLB's JSON.
- **Binding.** The GLB's scene root is `Root`, as in every model, so clip
  target ids are `from_names(Root, pelvis, …)`. `dress_character` puts
  `AnimationPlayer`, the shared `AnimationGraphHandle` and `AnimatedBy(Root)`
  on the model's `Root`, and `AnimationTargetId` (its name path from `Root`)
  on every node under it. The headless test checks that all 52 animated
  targets are bones of the model and that they move.
- **Per-model fit.** The clips are made on the protagonist. `fit_rigs`
  (after `AnimationSystems`) sets each model's pelvis to
  `own rest + (clip − reference rest) × own leg / reference leg`. The
  reference rest is read from the GLB's own `pelvis`, `calf_l` and `foot_l`
  nodes. Without the fit, Don Hidalgo would float 2.5 cm. The model root
  undoes the carrier's stride bob, since the clips bob the pelvis.
- **Blending.** The bands are walk 0.05–0.6 m/s, jog 2.4–3.4 m/s and sprint
  6.5–8 m/s; weights ease at 6/s. The six playable clips (three idles and
  three strides) start once their assets load, each seeked to the person's
  seeded phase, and keep running; only weights and rates change.
- **Glance.** `FigureBody.glance` turns `neck_01` and `head` half each,
  about the body's up, conjugated by each one's parent rest frame. No
  breathing overlay: the Quaternius idles breathe.
- **Freeze (review findings 1–2).**
  - A model is in view while any of its meshes is (`any_in_view`), so a
    culled hat cannot freeze a visible body.
  - The eye is the active `Camera3d` with the highest `order`, the lowest
    entity among equals (`view_eye`), for the people and the livestock.
  - The tests use synthetic stand-in rigs (review finding 3). Only 3 rig
    tests and 1 clips test load the real assets.
- **Fingers.** The clips animate them, so the dress-time curl is gone.

## Playtest fixes (user, 2026-10-24)

The user's first in-game look raised three problems:
- the men stood in a wide, crouched "ready" stance;
- Farach's folded arms went through his chest;
- the protagonist's poncho clipped through his shoulder and waist.

- [x] P1 Idle stance. Front views and numbers showed the retarget is
  faithful: the target's knee and arm angles equal the source's, and its
  stance ratio is 1.90 against the source's 2.06. Its hips are wider, so
  the feet stand 0.41 m apart against 0.36. The source is the problem:
  every UAL idle (Idle_Loop, Idle_Talking, Idle_FoldArms, Idle_Rail,
  Idle_Lantern, Idle_No) stands like a boxer. The feet are twice the hip
  joints' spacing apart and one 42 cm ahead, the knees bent 21–26°, the
  arms 16–18° out.
  - Fix: the tool calms the standing clips (`CALM`: idle, idle_talk,
    idle_fold_arms, yes, no).
    - The feet are drawn in to ≤ 1.3× the hip spacing and ≤ 12 cm one
      ahead of the other, at the same heights and with their own turn.
    - The legs are solved again through the knees, keeping 35% of the
      bend. The hips go as high as the straighter leg allows, so one leg
      bears the weight nearly straight and the other may rest bent.
    - The upper arms are lowered to ≤ 10° in idle, yes and no (the
      MakeHuman rest is 11°).
  - Result for idle: stance 1.30 (0.28 m), stagger 0.12 m, knees 7–9.5°,
    arms 10°.
  - The walk and jog are unchanged: their front-view numbers match the
    source's.
- [x] P2 Folded arms through the chest. All four men were affected, not
  only Farach: the mannequin's chest is slim, so its forearms sank 8–12 cm
  into the MakeHuman chests and clothes.
  - Chosen fix: one shared clip cleared for all four models. Only Farach
    plays it in game, so this needs no per-model runtime table and no
    per-model clips.
  - `retarget.py --model` CPU-skins each model as the game poses it (the
    clip's rotations, the hips fitted to its legs). The chest is sampled
    along its normal between spine_02 and the neck, within 10 cm of its
    middle. The arms are brought forward as they are: each clavicle swings
    forward, rounding the shoulders, by the least multiple of 5 mm that
    keeps every forearm and hand ≥ 1.5 cm in front of the chest.
  - Result: fold arms 12 cm, idle_talk 3 cm, recorded as `reach` in the
    clip's extras.
  - Tried and rejected:
    - shoulder flexion: needed 0.84 rad, and the folded arms rose to the
      chin;
    - pushing only the wrists, or only the elbows: the forearm near the
      elbow stayed inside;
    - weighing the elbow blend: linear skinning pinches the inside of the
      elbow into the body at that bend whatever the arm's place, so
      vertices under 75% to one bone are not counted.
  - The other three men's arms clear by more than they need.
- [x] P3 Poncho clipping. `mh2gltf.py`'s generated pieces that span several
  bones now take their skin from the dressed body under them (`inherit`):
  - each vertex gets the inverse-distance mean of its 8 nearest body
    points' weights, kept to the trunk joints it may follow (`SKIN_FROM`);
  - this covers the poncho and strap (pelvis, spine, clavicles), the faja
    and rastra (pelvis, spine_01), and the buttons, chain and lapels;
  - hats, kepi, neckerchief and collar stay rigid on their one bone.
  - The four models were reconverted. Only the `model.bin` of the
    protagonist, Hidalgo and Farach changed. Bairoletto has no such piece,
    so his is byte-identical. No rest bone moved, so `humanoid.glb`'s
    skeleton is unchanged.
  - `--check` passes for all four.

Route: the same delegated writer (2+ non-trivial files).

## Sabotage log, option B (user's standing rule)

Each test was failed on purpose by the sabotage listed, then the code was
restored (checked with `cmp` against a saved copy for Python and with
`git diff --stat` for Rust).

| Test | Sabotage → observed failure |
|---|---|
| py `test_every_game_engine_bone_has_a_source_bone_in_both_libraries` | finger joints 03 dropped from the map → 10 bones missing |
| py `test_the_source_t_pose_gives_a_t_posed_target_not_its_rest` | no rest arc → arm 90° off |
| py `test_every_bone_points_where_its_source_bone_points` | no rest arc → `upperarm_l` 90° off; delta taken in the source bone's frame → `spine_02` off |
| py `test_a_source_posed_as_the_reference_gives_the_target_rest` | pelvis placed from the source's rest → 3.2 cm off (first version survived: synthetic rigs had equal pelvises; the target is now shifted) |
| py `test_a_known_turn_maps_to_the_same_turn_on_the_target_rest` | turn applied on the wrong side → 0.89 rad |
| py `test_an_elbow_bend_bends_without_twisting_the_forearm` | source local rotations copied → 1.5 rad twist (the "source-frame delta" sabotage only bends in the wrong plane; it is caught by the points test) |
| py `test_the_pelvis_moves_by_the_leg_length_ratio` | ratio 1 |
| py `test_a_loop_stays_in_place_and_keeps_its_bob` | drift kept → 0.125 m off |
| py `test_the_ground_speed_is_the_drift_or_the_planted_foots_slide` | drift left out of the speed → 0 |
| py `test_no_foot_goes_through_the_ground` | no clamp → pelvis 5 cm low |
| py `test_the_output_is_deterministic` | timestamp in the generator |
| py `test_the_clips_are_the_games_in_its_order` | walk and walk_formal swapped |
| py `test_the_torso_and_head_lean_as_the_sources_do` | aim along +Y → torso 16.4° vs 9.9° |
| `clips::the_weights_fade_…` | no fade between strides → sum ≠ 1 |
| `clips::a_walking_clip_plays_at_the_ground_speed_over_its_own` | rate always 1 |
| `clips::each_person_has_their_own_idle` | Farach plain idle; talking ignored |
| `clips::the_clips_are_in_the_files_order_and_their_speeds_are_the_measured_ones` | walk speed 1.2; Walk and WalkFormal swapped in `Clip::ALL` |
| `rig::each_person_plays_their_idle_standing_and_the_strides_moving` | strides never weighed |
| `rig::a_models_pelvis_moves_by_its_own_legs_about_its_own_rest` / `rig::every_model_stands_on_its_own_feet_and_farach_folds_his_arms` | no pelvis fit; Farach plain idle (real-asset test) |
| `rig::a_glance_turns_the_bone_about_the_bodys_up` | conjugation reversed |
| `rig::the_clips_bind_to_the_models_bones_and_move_them` | target path without `Root` → clip ids not on the model |
| `rig::a_planted_foot_slides_back_at_the_ground_speed_walking_and_jogging` | rate always 1 |
| `rig::a_model_far_off_or_out_of_view_keeps_its_pose_but_the_protagonists_never_does` | never freeze; protagonist freezes; first probe only (culled hat) |
| `rig::freezing_goes_by_the_camera_drawn_last_not_the_first_one_found` | first camera found |
| `rig::the_model_undoes_the_carriers_stride_bob` | bob kept |
| `livestock::the_view_is_the_active_camera_drawn_last_whatever_the_query_order` | first camera found; ties by query order |
| `livestock::a_model_is_in_view_while_any_of_its_meshes_is` | first probe only |
| game `errand::whoever_speaks_talks_with_their_hands_until_their_lines_end` | speaker never marked; never unmarked |
| py `test_the_standing_clips_stand_at_ease` (real packs) | calm skipped → stance 1.90; arms not lowered → 18.2° |
| py `test_rebuilding_gives_the_shipped_file_byte_for_byte` | calm not wired into `build`; reach removed (no reach clears the chests) |
| py `test_seen_from_the_front_the_legs_and_arms_spread_as_the_sources_do` | thighs not rest-aligned → stance off by 0.21 |
| py `Calm.test_a_wide_stance_is_drawn_in_and_straightened_on_the_same_ground` | stagger kept; stance kept; knees not straightened; arms not lowered; feet turned with the legs |
| py `Calm.test_reaching_out_brings_the_arms_forward_as_they_are` | no reach |
| py `Shipped.test_folded_arms_and_talking_hands_stay_out_of_every_chest` | elbow blend counted → 1 mm in; GLB rebuilt without `--model` → Bairoletto 9.3 cm in (rebuilt with them after) |
| py mh2gltf `test_a_piece_over_several_bones_takes_the_skin_of_the_body_under_it` | pieces left rigid; arms not left out |
| py mh2gltf `test_the_pieces_on_one_bone_are_rigid_and_the_others_follow_the_body` (Farach converted) | pieces left rigid → buttons on 1 joint |

RED before code: none observed; the tests were written with the code, so
they carry sabotage evidence instead.

Tests removed with PR 1's driver: every `rig::tests::*` of the sabotage log
below (posture, gait, foot IK, finger curl, breath) except the freeze test,
which was rewritten on stand-in rigs.

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

## Sabotage log, PR 1 (user's standing rule)

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
- 2026-10-24: PR 1 (f0bac73) reviewed, approved and merged into main. Option B
  continues on `feat/character-clips`, from origin/main. The review's three
  findings (probe, camera, synthetic tests) are folded into C2/C3.
- 2026-10-24: option B (C1–C3) implemented, uncommitted: the user commits.

## Verification (2026-10-24, option B)

- `python3 -I -B -m unittest discover -s tools`: 106 passed (28 retarget,
  real-pack tests included; after the playtest fixes)
- `python3 -I tools/mh2gltf.py --check` on the four models: ok
- `cargo test -p pampa-core`: 352 passed
- `cargo test -p pampa-content`: 32 passed
- `cargo test -p pampa-render`: 266 passed
- `cargo test -p game`: 118 passed
- `cargo build -p game`: ok
- `cargo fmt --all -- --check`: ok
- `cargo clippy --workspace --all-targets -- -D warnings`: ok

(All re-run after the playtest fixes, with the same Rust counts.)

Offline previews (CPU-skinned from `humanoid.glb`, side and front) are in
the session scratchpad: the protagonist's walk, jog and idle, and Farach's
folded arms. There are also side-by-sides with the source mannequin.
Nothing was checked in game yet.

## Next step (option B)

- 2026-10-25, user playtest of the fixes: "no me cierra del todo, pero
  después lo arreglamos". The standing, fold-arms and poncho fixes are
  accepted for now, and a later polish pass on people's poses and motion is
  pending. Next up is PR 2 (the ridden horse).

- In game, after the playtest fixes: the calmer standing (feet in, knees
  straight, arms down), Farach's folded arms in front of his chest (the
  shoulders a little rounded), and the poncho following the torso when
  walking and jogging.
- In game:
  - the plaza and `goto nievas` (Don Hidalgo idle, then `idle_talk` while he
    speaks);
  - `goto castex` (Farach's folded arms, Bairoletto idle);
  - walk and jog on foot: feet sliding, the walk at 1.52× pace, and the jog
    reading as a run;
  - the head glance.
- Possible follow-ups:
  - a slower jog (blend walk and jog at 5 m/s, or another clip);
  - no terrain foot IK yet;
  - riders on `sit_idle`/`drive` in PR 2;
  - the farm clips (`harvest`, `plant`, `water`, `chop`) for townsfolk (PR 3).

## Next step (PR 1, superseded)

- Look at it in game: `goto nievas` (Don Hidalgo standing), the plaza (the
  protagonist idle), then walk and jog. Check the free foot, the sway, and
  the foot contact on slopes. The driver plants on the body's flat ground
  only; there is no terrain IK.
- PR 2: the ridden horse (T3–T6). The rider needs its own `RigPose` source
  (`Bearing.on_foot = false` already skips the posture and the plant).
