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
3. **PR 3, townsfolk** (user, 2026-10-25: triangle cap per townsperson
   **11,000**, raised from the planned ~8,000):
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

## Tasks (PR 3: townsfolk), branch feat/townsfolk-models from main 0386485

Replace the procedural townsfolk with MakeHuman models. The cap is at most
**11,000 triangles** per townsperson (user, 2026-10-25). After this PR no
procedural person is drawn by default.

Route: one delegated writer. The work spans 2+ non-trivial files: the
converter, the retarget tool, the render runtime, core pace, assets and
docs. Mid-task the user added `crates/core/src/walker.rs` to the allowed
surfaces, only for the player's walk speed.

- [x] P1 Converter pieces for the women (`tools/mh2gltf.py`):
  - `skirt` (pollera), a generated bell with a weighted skin;
  - `apron` over it;
  - `shawl`, cut from the body, with generated ends crossed over the chest;
  - `headscarf`, cut from the head, with a knot and a point down the nape;
  - `hair` worn up and a `bun`.

  Plus `boina` and `alpargatas` for the paisano, and a `collar` with no wing
  tips. CPU-skinned clearance through walk, idle and idle_talk (Rust,
  real assets).
- [x] P2 Five outfits in `assets/characters/townsfolk_{paisano,peon,volga,headscarf,bun}`:
  `.mhm` and `.mhmat` sources, converted. Measured triangles:

  | Outfit | Triangles |
  |---|---|
  | paisano | 6,312 |
  | peón | 4,958 |
  | Volga German | 9,004 |
  | headscarf woman | 4,746 |
  | bun woman | 4,644 |

  The four existing men reconvert byte for byte with the new converter.
- [x] P3 Runtime (`crates/render/src/character/townsfolk.rs`, new):
  - the model comes from the look's outfit;
  - per-garment tints come from shared, cached copies (`TintCache`);
  - the clips go through the shared graph;
  - freeze and range are as for the other models;
  - the skirt-wearers' arms are held 0.14 rad out.

  `models off` keeps the rigid figures (see Decisions).
- [x] P4 Headless CPU measurement of a full plaza (release); numbers under
  Decisions.
- [x] P5 PR 2 review follow-ups: the frame-bounded respawn test, the
  hidden-rider stirrup test and the front-leg fold-axis test.
- [x] P6 Tests with sabotages (log below), previews, credits, docs (this
  file, `README.md`, `CREDITS.md`, `assets/characters/{README,CREDITS}.md`,
  `tools/README.md`, module docs).
- [x] W1 (user playtest, 2026-10-25: everyone "walks as if stalking") The
  walk is stood up in the retarget tool (`UPRIGHT`). Townsfolk pace is now
  1.25–1.5 m/s. The player walks at 1.6 m/s (user's choice). Strides are
  scaled per model by leg length. The skirt was re-fitted to the longer
  stride.
- [x] W2 (user playtest of W1: "the knee seems to dislocate") Two causes:
  the hips switched standing leg, and the knees locked straight and then
  flicked. Fixed with a blended, loop-smoothed hip rise, a 4° minimum knee
  bend and smoothed swinging knees. Measured at 60 fps through the runtime.

## Decisions (PR 3, 2026-10-25)

- **The skirt** (`make_long_skirt`):
  - Shape: 32 sides × 10 rows, from a waistband at station 3.9 to an
    ankle hem at 1.15. It never narrows below the hips, rounds into a
    bell, flares 1.2 dm at the hem, has 8 soft folds and a turned-in hem.
  - Skin: the waistband takes the body's pelvis/spine_01 weights. Lower
    rows take up to `follow` 0.8 of the thighs' turn, growing toward the
    hem and split left/right by side. The back of the hem also takes up to
    `kick` 0.9 of the shins' turn.
  - The leg skin under it is left out from station 1.6 up.
  - Tried first: 0.6 follow and no kick. The trailing heel came 13 cm out
    of the back hem. Then, with the longer upright stride, the forward
    knee came 8 cm out of the front: hence 0.8 follow and the wider hem.
- **The women's hands.** The clips were made on a man. The women's wider
  hips, with the skirt over them, swallowed the idle hands by up to 4.4 cm.
  - Chosen fix: a runtime per-model fit, like the pelvis fit. A model
    wearing a material named `skirt` gets its upper arms turned out by
    `SKIRT_ARMS_OUT` = 0.14 rad about the body's front, on top of the clip
    (`arm_out`).
  - Measured (`the_women_keep_their_legs_in_their_skirts_and_their_hands_out_of_them`,
    real assets, the game's own poses): legs ≥ 1.25 cm inside, hands
    ≥ 1.8 cm outside, in idle, walk and talking.
- **Shawl and headscarf.**
  - The shawl is a body shell: shoulders above station 5, the upper arms
    to reach 0.85, and a point down the back. So it bends exactly with the
    body. Its ends are generated bands crossed over the chest, laid point
    by point on the frontmost surface (smoothed, never into it). They are
    skinned from the trunk and clavicles.
  - The headscarf is a head shell round an open face oval. Its knot is
    rigid on the head; its point is skinned from the head, neck and
    spine_03.
  - The bun woman's hair is a head shell plus a generated bun.
  - The headscarf woman has no hair proxy.
- **Women's boots** reach mid-calf (station 1.9), so no bare calf shows
  when the hem kicks.
- **Outfits.** Each look keeps its 1919 look:
  - paisano: young, medium build;
  - peón: lean criollo, about 40, moustache;
  - Volga German: about 50, heavy, light-brown CC0 beard decimated to
    3,408 triangles;
  - headscarf woman: about 45, sturdy, with an apron;
  - bun woman: about 23, slim.

  Bombachas reach the ankle inside the boots or alpargatas: the
  `male1591` shin has one long face that otherwise showed bare skin.
- **Tints.** A look's low part (`look % 5`) picks the outfit, so
  `FigureKind::townsfolk` now keeps the whole look. For each garment, a
  hash of `look / 5` picks one of 4 sRGB colours, the first the model's
  own. Copies are cached by (material, colour) in `TintCache`, so the
  count is bounded by garments × colours. A 22-person crowd test checks
  that dressed-alike people share the very same copies.
- **Models load once.** `CharacterModels` is keyed by model folder.
- **`models off`: kept.** Retiring the rigid figures would need
  `game/src/console.rs`, which is outside the surface. They are also the
  fallback when a model fails to load, and the carrier body stays either
  way. The bench shows the 1,100 hidden rigid parts of a plaza cost
  0.01–0.06 ms a frame.
- **Clips.**
  - `retarget.py --talker` adds models whose chests only the talking
    hands must clear. The townsfolk never fold their arms.
  - idle_talk's reach went from 3 to 6 cm: the shawl-covered busts. The
    folded arms are unchanged at 12 cm.
  - Then the upright walk (below).
  - `humanoid.glb` changes in `idle_talk` and `walk` only; the nodes and
    the other clips are unchanged. It rebuilds byte for byte.
- **The upright walk (W1).**
  - Measured on the protagonist, the pack's `Walk_Loop`: stance knee 25°
    on average (36° at most), torso lean 7–9°, shoulders 6 cm back of
    rest, elbows 32–41°. The game played it at 1.52×.
  - `Walk_Formal_Loop` keeps the hands behind the hips the whole cycle,
    so it was not used.
  - `Retargeter.upright`, deterministic, does all of this:
    - stride ×1.15;
    - knees eased by 19° (never below 30% of the bend, nor `KNEE_MIN` 4°);
    - the hips' rise blended from both legs' asks, by which ankle is lower
      (`STAND_BLEND` 1.2 cm), then smoothed round the loop over ±2 keys
      (`RISE_SMOOTH`);
    - no leg pulled straighter than its own eased bend: its foot lifts a
      touch instead;
    - each swinging knee's bend smoothed round the loop (1-2-1, twice,
      fading in as the foot lifts: `SWING_SMOOTH`, `CONTACT_LIFT`);
    - torso turned back at spine_01 to a mean lean of 2°;
    - clavicles at rest;
    - upper arms hanging as at rest, swinging −10° to +14° in phase with
      the source;
    - elbows 10–25°;
    - head back to its rest pitch.
  - Result: stance knee 11° on average (19.3° at most), never under 4°;
    a knee changes by at most 16° a key (the source's 13°); lean
    1.7–4.1°; hands −0.5 to +30 cm ahead of the hip line (mean +13 cm). The
    natural speed is 1.271 m/s, measured on the target.
- **The knee "dislocation" (W2), measured on the walk the user played:**
  - the trailing knee locked at 0.0° for 4 keys, then flicked 27° in one
    key;
  - at the stance hand-off the knee jumped by 14° and the pelvis stepped
    2.6 cm, as the hips switched standing leg;
  - no knee ever bent backwards: every bend had the same sign;
  - the shin stayed within 2.1° of the thigh's plane, as in the source;
  - key 0 equals the last key, and the runtime rate is constant at a
    given speed.

  After the fix, through the runtime at 60 fps
  (`rig::walking_knees_bend_smoothly_never_lock_or_twist_round_the_loop`):

  | Who | Speed | Least bend | Most change per frame | Shin off plane | Pelvis per frame |
  |---|---|---|---|---|---|
  | protagonist | 1.6 m/s | 4.0° | 9.8° | 2.1° | 8.6 mm |
  | peón | 1.4 m/s | 4.0° | 9.3° | 2.1° | 7.5 mm |
  | headscarf woman | 1.4 m/s | 4.0° | 10.8° | 2.1° | 7.5 mm |
- **Pace.**
  - Townsfolk walk at 1.25–1.5 m/s (`WALK_MAX` 1.5), playing the walk at
    0.98–1.18×.
  - The player walks at 1.6 m/s (user), so 1.26× for the protagonist.
    Playback is speed over natural speed, so his feet stay planted; no
    extra stride stretch was needed. The foot-slide test is green.
  - Each model's playback is divided by its legs over the protagonist's
    (`model_stride`): shorter people take shorter, quicker steps without
    sliding.
- **P4, plaza bench** (`rig::tests::plaza_bench`, ignored; release, Ryzen 5
  4600G, headless). 31 townsfolk (looks 0–30) and the protagonist, two in
  three walking, none frozen:
  - the 32 people add 0.63–0.65 ms a frame to the update (0.84–0.85 ms
    against 0.20–0.21 ms without them): clip weighing, Bevy's animation,
    the fits, the carriers and the transforms;
  - the 1,100 hidden rigid parts cost −0.01 to 0.05 ms of that (noise);
  - skinning preparation is 0.28 ms a frame for 20,405 joint matrices;
  - 190,178 triangles are drawn.
  - Skinning preparation means each skinned mesh's joint matrices, as the
    render extraction computes them (385 skinned meshes, about 12 a
    person).
  - Freezing past 120 m and out of view (unchanged) and the shared range
    need no tuning at this cost.

## Sabotage log, PR 3

Each sabotage replaced one snippet. The test was run, then the file was
restored and checked with `cmp` by a script (`sab/` in the session
scratchpad). Asset-level sabotages rebuilt an asset, then restored it from
a saved copy, checked with `cmp` or `diff -r`. Every one failed:

| Test | Sabotage → result |
|---|---|
| py `test_a_long_skirt_hangs_as_a_bell…` | narrows below the hips → survived (uniform stand-in legs), strengthened with narrow legs, then failed; no flare → fails |
| py `test_the_skirt_follows_the_thighs…` | thighs not split by side → fails; no shin share → fails |
| py `test_an_apron_covers_the_skirts_front…` | apron in the skirt's surface → fails |
| py `test_a_shawl_covers_the_shoulders…` | no point down the back → fails |
| py `test_the_shawls_ends_cross…` | ends at one depth → fails; laid into the chest → fails |
| py `test_a_headscarf_frames_the_face…` | face covered → fails |
| py `test_a_headscarf_is_knotted…` | point on the throat → fails |
| py `test_a_boina_covers_the_crown…` | no lean → fails |
| py `test_a_bun_sits_behind_the_head…` | bun on the face → fails |
| py `test_alpargatas_are_canvas_shoes…` | leather → fails |
| py `TownswomanConversion` | skirt follow 0 → fails |
| py `test_a_talker_is_cleared_for_the_talk_only` | talkers checked for folded arms → fails |
| py `test_the_packs_crouched_walk_is_stood_up` | knees not eased; lean 8°; clavicles kept; arms swing back; pack elbows; head not levelled → each fails; stride ×1.0 → survived (compared against the sabotaged constant), fixed to 1.15, then failed |
| py `Shipped.test_the_walk_is_an_easy_upright_street_walk` | GLB rebuilt without `UPRIGHT` → fails |
| py `Shipped.test_folded_arms_and_talking_hands…` (changed) | GLB rebuilt without `--talker` → fails |
| py `test_rebuilding_gives_the_shipped_file_byte_for_byte` (changed) | `TALK = ()` → fails |
| `townsfolk::each_look_wears_its_own_outfit…` | `look % 4` → fails |
| `townsfolk::every_tinted_garment_is_a_material…` | garment `aprons` → fails (first run: stale snippet after fmt; tables now `rustfmt::skip`) |
| `townsfolk::a_look_picks_each_garments_colour…` | 2 colours → fails |
| `townsfolk::the_cache_makes_one_copy…` | keyed by material only → fails |
| `townsfolk::an_arm_turned_out…` | about Z → fails |
| `townsfolk::a_level_ray_meets…` | ray hits outside the triangle → fails |
| `model::every_person_has_a_model…` | townsfolk without models → fails |
| `model::every_character_model_loads…within the triangle budget` (changed) | Volga beard not decimated (16,028) → fails |
| `rig::the_women_keep_their_legs_in_their_skirts…` | arms not held out → fails; the old skirt (follow 0.6, flare 1.0) reconverted → fails (leg 7.9 cm out) |
| `rig::townsfolk_wear_their_outfits_in_tinted_cloth…` | no tint → fails; one copy per person → fails |
| `clips::the_walk_steps_at_an_easy_pace…` | the pack's speed → fails |
| `clips::a_person_with_shorter_legs…` | stride not scaled → fails |
| `gait::a_folded_front_leg_stays_in_the_bodys_sagittal_plane` | fold about the raw Z axis, not the body's side in the parent's frame → fails |
| `rider::a_respawned_rider_keeps_the_stirrups…` (frame-bounded) | `Added<Seated>` → fails |
| `rider::a_stirrup_stays_put_while_its_rider_is_hidden…` | carried while hidden → fails |
| `figure::every_look_builds…` (changed) | `townsfolk()` drops the look's high part → fails |
| py `check_knees` (both walk tests) | knees pulled straight → fails; no rise smoothing → fails; no swing smoothing → fails; the hips switched by the standing leg instead of blended → survived (the loop smoothing alone removes the step: the blend is kept as a second guard, see W2) |
| `rig::walking_knees_bend_smoothly…` | the walk played twice as fast → fails |
| core `townsfolk::people_walk_at_an_easy_street_pace` | the old 1.1–1.6 → fails |
| core `walker::walks_at_1_6_m_s_and_jogs_at_18_kmh` (renamed) | 6 km/h → fails |

RED before code: none observed. The tests were written with the code, so
they carry sabotage evidence instead.

## Verification (2026-10-25, PR 3)

- `python3 -I -B -m unittest discover -s tools`: 127 passed (packs present;
  humanoid.glb rebuilds byte for byte)
- `python3 -I tools/mh2gltf.py --check` on the nine models: ok (exit 0)
- `cargo test -p pampa-core`: 345 passed
- `cargo test -p pampa-content`: 32 passed
- `cargo test -p pampa-render`: 309 passed, 2 ignored (the horse preview
  dump and the plaza bench)
- `cargo test -p game`: 118 passed
- `cargo build -p game`: ok
- `cargo fmt --all -- --check`: ok
- `cargo clippy --workspace --all-targets -- -D warnings`: ok

Previews: offline, CPU-skinned in Python from the shipped clips with the
game's pelvis fit and arm-out, flat-shaded. They are in the session
scratchpad `prev3/`:
- `standing_{front,side,q34}.png` and `walking_{front,side,q34}.png` (the
  five outfits);
- `skirt_{headscarf,bun}_{side,q34}.png` (six phases of the walk);
- `crowd_q34.png` and `crowd_front.png` (31 tinted townsfolk and the
  protagonist);
- `walk_{old,new}_{side,front}.png` (the protagonist, the peón and the
  headscarf woman, the pack's walk against the stood-up one);
- `knees_{old,new}_{protagonist,peon}_{side,front}.png` (knee close-ups
  over a cycle, the first upright walk against the smoothed one).

Nothing was checked in game by the writer.

## Next step (PR 3)

- The user commits (split and commands in the writer's report). The native
  review runs per commit, under the user's RDD switch.
- In game:
  - the plaza in General Pico (31 people): outfits, colours and the
    women's skirts while walking;
  - the walk of everyone, the player at 1.6 m/s included;
  - `models off` and back.
- Possible follow-ups:
  - farm clips for the townsfolk;
  - women's own idles;
  - a mounted townsperson would now ride with their model, untested in
    game.

## Tasks (PR 2: the ridden horse), branch feat/character-clips after c008f59

Route: one delegated writer (2+ non-trivial files: render horse, tack, gait,
rider, rig, model, visual, livestock; core pose; game tests; the retarget
tool; docs). `crates/core/src/lib.rs` was added to the allowed surfaces by the
user, only to drop the re-exports of the deleted horse pose code.

- [x] H1 Saddled horses are the Quaternius model (`character/horse.rs`),
  driven from `VisualPose` and `HorseGait`:
  - `CharacterVisual::Horse` and `::Mounted` spawn a `HorseBody` (turned to
    the heading) holding a `HorseModel` that waits for the scene
    (`HorsePending`), dressed by `dress_horse` with the corral horses' coat
    code (`livestock::dress_meshes`, extracted);
  - gaits: the idle, then the walk paced to the ground speed (stretched to
    4.8 m/s), then the gallop; tied horses change idle (Idle, Idle_2,
    Idle_Headlow), ridden ones stand still; the player's horse never freezes.
  - Proof: `horse::tests::{the_idle_fades_into_the_stride_and_the_gallop_takes_over_outright,
    the_walk_and_the_gallop_play_at_the_ground_speed_over_their_own,
    a_ridden_horse_stands_still_and_a_tied_one_changes_idle,
    the_long_gallop_keeps_its_hooves_still_up_to_its_fastest_unhurried_pace,
    slowing_down_from_a_gallop_or_a_walk_never_folds_a_leg_or_sinks_a_hoof,
    the_galloping_horse_bends_its_back_through_the_stride}`,
    `gait::tests::a_folded_hind_leg_stays_in_the_bodys_sagittal_plane`,
    the coat tests, `visual::tests::a_saddled_horse_turns_to_its_heading_and_takes_its_gait_from_the_speed`.
- [x] H2 Tack (`character/tack.rs`): sudadera, carona, two bastos, pellón,
  cincha, crownpiece, noseband, cheekpieces and bit rings fitted to the
  horse's own rest skin and skinned to its joints; reins (three stretches a
  side: bit, jowl, neck, then the rider's left hand or the pommel) and
  stirrup leathers stretched each frame; stirrups hanging from `Torso` or on
  the rider's feet.
  - Proof: the `tack::tests` on the stand-in tube horse, and on the
    shipped horse (CPU skinning through the clips)
    `tack::tests::{a_saddled_horse_wears_its_tack_on_its_own_joints,
    the_tack_clears_the_horse_through_every_gait,
    a_tied_horse_keeps_its_tack_and_reins_clear_through_every_idle,
    a_scene_spawned_again_is_checked_before_the_old_tack_comes_off,
    a_strap_with_an_end_gone_is_hidden_until_it_has_both_again}`.
- [x] H3 The rider (`character/rider.rs`): the protagonist's model on the
  `Torso` bone, playing `drive` (only), with two-bone IK legs to the stirrups
  and arms to the reins, and spring-damped secondary motion (added after the
  user's playtest, below).
  - Proof: `rider::tests::*`, among them
    `{a_rider_sits_on_the_horses_torso_with_the_reins_and_the_stirrups,
    a_respawned_rider_keeps_the_stirrups_on_its_new_feet_and_the_reins_in_its_new_hand,
    the_rider_clears_the_horse_through_every_gait,
    at_the_gallop_the_rider_rides_the_half_seat,
    walking_the_rider_sits_deep_and_upright,
    the_free_arm_swings_with_the_gallop_less_at_the_walk_and_keeps_off_the_horse,
    the_riders_poncho_stays_over_his_shoulder_and_arm}`. The seated clip
    only (`rig::SEATED`) has no test of its own in `rig::tests`; every
    real-asset rider test plays it.
- [x] H4 Mount, dismount and tied horses; the procedural horse
  (`HorseJoint`, `HorseShape`, `HorseKit`, painted parts, `spawn_horse` and
  `update_horses`), the rigid rider (`FigureBody.seat`, `rider_offset`, the
  rider branch of `update_figures`), the horse face texture and finishes,
  and pampa-core's horse and riding pose maths (`horse_pose`, `HorseAngles`,
  `Leg`, `leg_offsets`, `duty`, `in_stance`, `horse_cadence`,
  `horse_advance`, `riding_pose`, `ear_flick`, `tail_swish`,
  `JointAngles.spread`) are deleted. `HorseGait` stays.
  - Fallback: nothing is drawn until the horse's scene has loaded (it loads
    at startup, long before anyone mounts); no rigid stand-in.
  - `models off` now only swaps the people on foot back to rigid figures.
  - Proof: `rider::tests::getting_off_leaves_nothing_of_the_horse_and_getting_on_again_seats_the_rider`,
    `visual::tests::mounting_swaps_the_figure_for_one_saddled_horse_and_back`,
    `model::tests::the_toggle_puts_models_on_people_on_foot_and_leaves_riders_to_their_horse`,
    game `errand::mounting_swaps_the_clerk_figure_for_one_horse_and_rider_and_nothing_else`.
- [x] H5 Review follow-ups:
  - Talking: `rig::tests::whoever_is_marked_talking_talks_with_their_hands_and_stops_when_unmarked`;
  - every key checked for chest clearance (a reach is tried on every fifth
    key first, `CLEAR_EVERY`, then on all); the shipped GLB rebuilds byte
    for byte;
  - `--model` errors are one-line `RetargetError`s (no numpy, JSON, UTF-8,
    shape);
  - the measures dict is updated;
  - the docstring names numpy for `--model`;
  - the output is written atomically (`write_atomically`);
  - `leg_length(skeleton, calf, foot)` has no unused parameter and is the
    one leg measure (the clearance used two inline copies);
  - `rig::tests::the_glance_turns_the_head_within_bounds_and_never_adds_up`.
  - Proof: py `FollowUps.*` (5 tests).
- [x] H6 Tests with sabotages (log below), previews, docs (this file,
  `README.md`, `assets/characters/README.md`, `tools/README.md`, module docs).

## Playtest of the work in progress (user, 2026-10-25)

"The horse looks beautiful, the tack too, and the walk is spectacular." Three
problems, then two more:

- [x] P4 The gallop read frantic (the clip's 2.3 m stride at about 3x).
- [x] P5 Slowing down folded the legs (gallop to walk, and walk to idle).
- [x] P6 The rider was "harder than Maradona at an electronic party".
- [x] P7 The galloping body moved "all in one piece".
- [x] P8 Second playtest: "the gallop is beautiful", the rider "still a bit
  stiff" (the wrist motion liked): the gallop seat reworked to the
  half-seat (riding references: two-point / galloping position; the
  gaucho's one hand on the reins).
- [x] P9 Third playtest: one hand on the reins confirmed. The poncho folded
  over the left shoulder went through the shoulder and upper arm (on foot
  too: 4 cm deep in the idle); the free right arm was rigid.

## Decisions (PR 2, 2026-10-25)

- **Measured speeds.** Hoof (toe) slide while planted, pooled over the four
  hooves: the walk clip 1.25 m/s (the corral horses' `HORSE_WALK_SPEED`; the
  hind hooves slide back at about 1.4, the fore at 0.85), the gallop clip
  3.94 m/s by the same pooled mean (4.46 by the median toe slide the tests
  use). The stretched gallop shows 7.3 m/s (`GALLOP_SPEED`, checked by
  `the_long_gallop_…`).
- **Gaits.** The idle fades into the stride over 0.05–0.6 m/s. The walk and
  the gallop change outright with hysteresis (gallop above 4.8 m/s, back to
  the walk below 4.2), never held half and half; the weights ease at 12/s and
  the incoming clip is seeked in step (the left hind striking:
  `WALK_FROM_GALLOP`). Walk rate up to 3.9x (4.8 m/s); gallop rate 0.5–2.0x.
  Slide: none to speak of up to 14.6 m/s (12.2 m/s: about 0.1 m/s at 1.67x);
  the 19.4 m/s turbo slides about 4.6 m/s (accepted, dev only).
- **Why the legs folded (P5).** Bevy weighs a bone's property only among the
  clips that animate it, and the horse's clips animate different sets (the
  back's translation and rotation, the hooves' IK targets, the ears, the
  tail's root): a fading clip kept those at full strength. `gait::PARTIAL`
  lists the 24 such properties (checked against the GLB); each is eased to
  its rest by the weight of the clips that do not animate it. The same
  effect made a rider's zero-weight `drive` clip disturb walkers, so riders
  play only `drive` and walkers never play it.
- **The long gallop (P4, P7).** Each upper leg bone's swing about its mean
  over the gallop loop (`GALLOP_MEANS`, measured on the GLB) is stretched
  1.4x, as far as its leg's end stays above the ground. Phase-locked to the
  loop: the croup pitches ±7° (lowest as the hind legs gather, at 0.57 of the
  loop), the back bones bend against it so the withers pitch the other way,
  the neck pumps, the head undoes half, the hips roll and yaw a little and
  the tail lags a fifth of a loop. `Torso` and `Tail1` (no clip moves them)
  are put back at rest first each frame.
- **Hooves.** The model's hooves hang from IK targets, not from the legs.
  Each frame they are put on their leg's end (blends weigh legs and targets
  apart), never below where they would stand; a leg a blend reaches below
  the ground is folded at the knee or hock just enough.
- **Seat.** The rider hangs from `Torso` (it follows the saddle area like
  `Back`; `Back` is the croup). The pelvis is held 0.13 m above the
  pellón's top at x = -0.02 (the lowest point of the back). IK targets in the
  rider's space: ankles ±0.32 m out, 0.69 m down, 0.1 m ahead; knees bend
  forward and out; rein hand at (0.05, 0.18, 0.32), on the pommel (0.06,
  -0.02, 0.22) after 3 s still; right hand on the thigh.
- **Rider motion (P6, P8).** Critically damped springs (exact, frame-rate
  independent). The seat's height and the saddle's pitch and roll are
  smoothed (4 rad/s); the spine counters 95% of the pitch and 60% of the
  roll beyond that (nearly at once: a lag at the gallop's 17 rad/s put the
  torso out of step). Walking the rider sits deep and upright (the seat on
  the pellón within 5 mm, the shoulders within 8° of over the hips), the
  back rounding 1.2 rad a meter of rise. Galloping it rides the half-seat
  (`HALF_SEAT`, eased in by a spring): the seat hovers 6 cm over its
  smoothed path, which takes the saddle's bounce out (the gap runs 1.7–10
  cm, never below 0), 10 cm forward over the knees; the stirrups are taken
  up a hand (`GALLOP_ANKLES`; the leathers follow the feet) so the knees
  bend about 105° (97–114°) and flex each stride; the heels sink, more as
  the seat comes down; the lean is 0.5 rad on top of the clip's 5° lean
  back, 70% from the hips (a flat back), about 20° from the vertical; the
  head keeps the pitch it had sitting down (85% at once, 15% late), so it
  pitches 0.04 rad against the saddle's 0.08 and rises 0.077 m against the
  saddle's 0.11. The rein hand rides over the withers (`GALLOP_HAND`) and
  follows 85% of the bit (35% walking): the hand-to-bit distance stays
  within 2 cm, the elbow opening (85–127°) as the head goes forward. The
  right hand rests by the thigh, swinging a little. Braking leans back.
  Idle: after 3 s the rein hand settles on the pommel, the weight shifts
  (±0.035 rad, 7–9 s) and the head glances. The rider's head stays at least
  0.8 m from the horse's head and neck.
- **The free arm (P9).** Spring-driven (7 rad/s, so it lags off the
  torso): its hand rests on the thigh as if holding a rebenque
  (`THIGH_HAND`, the elbow bent about 50° walking), sways with the horse's
  roll and pitch walking, swings and lifts with the gallop's rise and pitch
  (up to `FREE_REACH`, 12 cm; the elbow flexing 108° ± 10°), comes forward
  toward the pommel braking, and standing still now and then goes up to the
  hat's brim (a seeded, eased moment every 15–25 s). Both shoulders shrug
  late with the saddle's motion (`SHRUG`). The rein hand sits a little to
  the left of the pommel with its elbow carried out, clear of the poncho.
  Measured: at the gallop the free elbow moves 19° and the hand 7 cm, at the
  walk 9° and 1 cm; the hand stays at least 20 cm off the horse and the
  saddle.
- **The poncho (P9).** `tools/mh2gltf.py` `clear_arms`: out from the
  shoulder joint (no more than 7 cm in from it), every poncho vertex within
  2.5 cm of the dressed upper arm (a capsule to just past the elbow) is
  pushed straight out to it and takes up to 90% of the upper arm's weight,
  fading over 15 cm, so the poncho rides on the deltoid. The chest is left
  as it was, so the clips' chest clearance and `humanoid.glb` are
  unchanged (rebuilt byte for byte). The protagonist was reconverted
  (`model.bin` and `model.gltf` changed; `--check` passes). The CPU-skinned
  check (`rider::tests::poncho_in_arms`, used by the rider and the on-foot
  `rig` tests) finds no poncho point inside an upper arm
  (beyond the armpit) on foot (idle, walk, jog) or riding (idle, walk,
  gallop); without the fix it is 4 cm.
- **Tack fitting.** Each layer is a grid of rays from an axis inside the body
  (the barrel's along X at 1.14 m; the head's from behind the poll to the
  muzzle); the inner face stands off the first hit, the vertices take the
  hit triangle's blended weights, and the tack's own inverse bind poses are
  the joints' body-frame rests inverted, so its vertices are in the body
  frame. Offsets (sudadera 16 mm … pellón 97 mm on top) and lengths were set
  by CPU-skinned clearance through the clips: the saddle's back edge was
  pulled forward off the hips (the stretched gallop moves them), the
  crownpiece stops at the cheeks (below, the head runs into the neck), and
  the reins run bit → jowl → neck so no stretch cuts the head or neck in any
  gait or idle. Tied horses do not crop the ground: the reins over the
  pommel would cut the neck.
- **Respawned scenes.** In parallel test runs the horse's scene was
  sometimes spawned again (Bevy re-adds the asset and sends
  `AssetEvent::Modified`): `dress_tack` despawns the carrier's old
  `HorsePiece`s first, so a horse is always dressed once; the rider is
  spawned hidden and shown only once `pose_riders` has posed him, so a
  respawned seat never shows a T-pose for a frame. The real-asset fixtures
  wait for the rig to keep one player for 10 frames with no `WorldAsset`
  event, and the tests resolve bones by name every frame.
- **Split-friendly modules.** The horse (`horse.rs`, `gait.rs`), its tack
  (`tack.rs`: `dress_tack` observer, `Tack`) and the rider (`rider.rs`:
  `spawn_riders` on `Changed<Tack>`, `seat_riders`, `pose_riders`) each own
  their systems and real-asset tests, so the work can be committed as the
  horse, then the tack, then the rider. The clearance metric counts only
  points within 10 cm of the nearest triangle (beyond that the sign of the
  nearest triangle is meaningless, e.g. the hat's brim over the mane).

## Sabotage log, PR 2

Each test below was failed on purpose by the sabotage listed (one source
snippet replaced, the test run, the file restored byte for byte; a script
checked each restore, and `git diff --stat` was checked after each batch).

| Test | Sabotage → result |
|---|---|
| `a_ray_from_inside_meets_the_skin…` | barycentric weights rotated → fails |
| `a_shell_lies_its_offset_off_the_body…` | shell twice as thick → fails |
| `the_saddle_layers_nest…` | pellón offset under the carona → fails |
| `the_fittings_are_found_on_the_body…` | hanging stirrup against the barrel → fails |
| `a_strap_runs_from_one_end…` | strap centred on one end → fails |
| `two_bones_reach_the_target…` | end short of the target → fails |
| `the_upper_bone_keeps_its_twist…` | upper bone by the shortest arc only → fails |
| `a_placement_composes_like_the_transforms…` | child offset not turned → fails |
| `the_rider_faces_the_horses_front…` | rider faces backward → fails |
| `a_spring_eases_without_overshoot…` | first-order lag, not the spring → survived; strengthened (the exact critically damped curve and a gentle start), then failed |
| `a_property_the_fading_clips…` | blend toward the clip, not rest → fails |
| `the_partial_table_is_the_files…` | a wrong mask → fails |
| `a_stretched_swing_reaches_further…` | stretch adds instead of scaling → fails |
| `the_croup_and_the_withers_pitch…` | withers pitch with the croup → fails |
| `every_coat_looks_different…` | bayo the alazán's colour → fails |
| `the_bayo_has_black_points…` | no dorsal stripe → fails |
| `the_overo_shows_white_patches…` | no white blobs → fails |
| `the_tordillo_is_dappled…` | no dapples → fails |
| `coats_are_seeded_deterministically…` | only two coats → fails |
| `the_idle_fades_into_the_stride…` | no hysteresis → fails |
| `the_walk_and_the_gallop_play_at_the_ground…` | gallop capped at the walk's 3.9 → fails |
| `a_ridden_horse_stands_still…` | any rider never freezes → fails |
| `a_ridden_horse_wears_its_tack…` (since split) | stirrups on the shins → fails |
| `the_tack_and_the_rider_clear…` (since split) | sudadera sunk 1 cm → fails |
| `a_tied_horse_keeps_its_tack_and_reins…` | reins on the neck's skin → fails |
| `the_long_gallop_keeps_its_hooves…` | the clip's own speed → fails |
| `slowing_down_from_a_gallop…` | blend left to Bevy → fails |
| `at_the_gallop_the_back_bends…` (since replaced) | seat gives nothing back → survived; a seat-gives assertion was added, then failed; the test was later replaced by the half-seat test |
| `at_the_gallop_the_back_bends…` | no articulation → fails |
| `getting_off_leaves_nothing…` | reins never rewired → fails |
| `mounting_swaps_the_figure_for_one_saddled…` | rider dropped → fails |
| `a_saddled_horse_turns_to_its_heading…` | never turns → fails |
| `the_toggle_puts_models_on_people…` | a model on the mounted figure too → fails |
| `materials_differ_by_kind…` | skin rougher than cloth → fails |
| `a_planted_foot_slides_back…` | rate always 1 → fails |
| `whoever_is_marked_talking…` | talking ignored → fails |
| `the_glance_turns_the_head…` | glance quadrupled → fails |
| game `mounting_swaps_the_clerk_figure…` | rider dropped (game) → fails |
| core `the_motion_clock_wraps…` | period not a multiple of the glance cycles → fails |
| py `every_key_is_checked…` | sampled keys only → fails |
| py `a_cleared_walk_keeps_its_reach…` | measures replaced → fails |
| py `a_bad_model_is_a_one_line_error…` | raw exceptions → fails |
| py `the_output_is_written_whole…` | written in place → fails |
| py `the_leg_is_the_thigh_and_the_shin…` | thigh only → fails |
| `at_the_gallop_the_back_bends…` (since replaced) | spine never counters → survived; superseded by the half-seat test's head-pitch check |
| `at_the_gallop_the_rider_rides_the_half_seat…` | seat rides the saddle's bounce → fails |
| `at_the_gallop_the_rider_rides_the_half_seat…` | seat slams 5 cm into the saddle → fails |
| `at_the_gallop_the_rider_rides_the_half_seat…` | seat over the neck → fails |
| `at_the_gallop_the_rider_rides_the_half_seat…` | long stirrups in the half-seat → fails |
| `at_the_gallop_the_rider_rides_the_half_seat…` | head not levelled → fails |
| `at_the_gallop_the_rider_rides_the_half_seat…` | hand does not follow the mouth → fails |
| `at_the_gallop_the_rider_rides_the_half_seat…` | no lean in the half-seat → fails |
| `at_the_gallop_the_rider_rides_the_half_seat…` | no articulation → fails |
| `walking_the_rider_sits_deep…` | seat floats walking → fails |
| `walking_the_rider_sits_deep…` | leaning far forward walking → survived at 12°; the bound tightened to 8°, then failed |
| `the_free_arm_swings_with_the_gallop…` | free hand held still → fails |
| `the_free_arm_swings_with_the_gallop…` | no sway walking → fails |
| `the_free_arm_swings_with_the_gallop…` | free hand down in the saddle → fails |
| py mh2gltf `test_the_poncho_is_kept_off_the_upper_arm_and_rides_on_it` | no push off the arm → fails; no arm share → fails; chest pushed too → survived (the chest strip lay out of reach), moved to the armpit, then failed |
| `the_riders_poncho_stays_over_his_shoulder_and_arm`, `rig::the_poncho_stays_over_the_shoulder_and_arm_on_foot` | the protagonist reconverted without `clear_arms` → both fail (4 cm); the converter and the model restored and checked with `cmp` |

Re-run after the tests moved into `tack.rs` and `rider.rs` (renamed,
split, and the respawn-safe fixtures), plan 8, every one failed:

| Test | Sabotage → result |
|---|---|
| `tack::a_saddled_horse_wears_its_tack_on_its_own_joints` | one bit ring only → fails |
| `tack::the_tack_clears_the_horse_through_every_gait` | sudadera sunk on top → fails |
| `tack::a_tied_horse_keeps_its_tack_and_reins_clear…` | reins on the neck's skin → fails |
| `rider::a_rider_sits_on_the_horses_torso…` | stirrups on the shins → fails; `spawn_riders` never spawns → fails |
| `rider::the_rider_clears_the_horse_through_every_gait` | stirrups 12 cm out, against the barrel → fails |
| `rider::at_the_gallop_the_rider_rides_the_half_seat` | bounce, slam, over the neck, long stirrups, head not levelled, hand not following, no lean → each fails |
| `horse::the_galloping_horse_bends_its_back_through_the_stride` | no articulation (`CROUP` 0) → fails |
| `rider::walking_the_rider_sits_deep_and_upright` | seat floats 3 cm; leaning far forward → each fails |
| `rider::the_free_arm_swings_with_the_gallop…` | hand held still; no walk sway; hand down in the saddle → each fails |
| `rider::getting_off_leaves_nothing…` | reins never rewired → fails |
| `rider::the_riders_poncho_stays…`, `rig::the_poncho_stays…on_foot` | protagonist reconverted without `clear_arms` (a scratch copy of the converter) → both fail; the model restored from a saved copy, `cmp` identical; the unpatched converter reproduces it byte for byte |

RED before code: none observed; the tests were written with the code, so
they carry sabotage evidence instead.

Tests removed with the procedural horse and rider: `horse::tests::{a_criollo_stands_about_one_forty_five_at_the_withers_on_its_hooves,
a_horse_has_ps1_to_ps2_era_detail, shade_gathers_under_the_belly_between_the_legs_and_under_the_saddle,
horse_parts_have_their_finishes, the_rider_sits_on_the_pellon_with_feet_in_the_stirrups}`,
`visual::tests::{the_rider_moves_with_the_horses_gait, a_tied_horse_stands_saddled_and_breathes}`,
`finish::tests::horse_faces_have_eyes_nostrils_and_fitting_markings`, and
core `pose::tests::{the_walk_has_four_evenly_spaced_beats, the_trot_moves_diagonal_pairs_together,
the_gallop_has_a_suspension_and_reaches_further, legs_fold_while_in_the_air_and_never_bend_the_wrong_way,
a_standing_horse_breathes_and_rests_one_hind_leg, the_horse_cadence_follows_speed_whatever_the_frame_rate,
horses_flick_their_ears_and_swish_their_tails, a_rider_sits_astride_with_hands_at_the_reins}`.
The coat tests were rewritten on stand-in surface points (the procedural
horse they sampled is gone); `rig::…a_planted_foot…` samples 120 frames
instead of 60 (its 60 depended on the seeded phase).

## Verification (2026-10-25, PR 2)

- `python3 -I -B -m unittest discover -s tools`: 112 passed (real packs present; `humanoid.glb` rebuilds byte for byte)
- `python3 -I tools/mh2gltf.py --check assets/characters/protagonist/model.gltf`: ok
- `cargo test -p pampa-core`: 344 passed (8 horse/rider pose tests removed with their code)
- `cargo test -p pampa-content`: 32 passed
- `cargo test -p pampa-render`: 292 passed, 1 ignored (the preview dump), six runs in a row
- `cargo test -p game`: 118 passed
- `cargo build -p game`: ok
- `cargo fmt --all -- --check`: ok
- `cargo clippy --workspace --all-targets -- -D warnings`: ok

Split into three commits (user, 2026-10-25), each state assembled in the
scratchpad from `git archive HEAD` and checked with a scratch
`CARGO_TARGET_DIR` (never the repo's `target/`; one target reused in turn,
every `.rs` touched first so each state rebuilt): fmt, `cargo build -p
game`, `cargo test --workspace --no-fail-fast`, clippy `-D warnings` and
the tools tests all pass.

| Commit | Content | Changed lines | Tests (core, content, render, game; py) |
|---|---|---|---|
| 1 | the horse model, gaits, the gallop, the core cleanup (no tack, no rider drawn) | 4,135 (+2,037 −2,098) | 344, 32, 269, 118; 106 |
| 2 | the tack | 1,658 | 344, 32, 277, 118; 106 |
| 3 | the rider, the free arm, the poncho fix, the H5 tools follow-ups, the docs | about 2,450 | 344, 32, 292, 118; 112 |

Commit 1 is over the ~3,500 guide because of `horse.rs`'s rewrite (2,237,
half deletions) and the core's deletions (496). With the core cleanup moved
to commit 2 (also checked) it is 3,637 and commit 2 is 2,156. The third
state equals the working tree. Steps 1 and 2 carry small variants of
`horse.rs`, `mod.rs`, `visual.rs`, `model.rs`, `tack.rs` and `rig.rs`
(docs and wiring without the later modules; `rig.rs` there is the base
with the planted-foot test's 120 frames).

Previews (CPU-skinned in Rust by the ignored test
`horse::tests::preview_saddled_horses`, rasterized offline) are in the
session scratchpad `prev/`: `gallop_side.png` (a 16-frame loop),
`gallop_q34.png`, `seat_side.png` (the half-seat over a stride and the
walk seat), `seat_q34.png`, `slowdown_side.png` (12.2 m/s braking at 8 m/s² to a
stand), `walkstop_side.png`, `walk_side.png`, `rider.png`,
`rider_front34.png`, `arms_front34.png` and `arms_side.png` (the torso and
arms at idle, settled, walking and galloping, the poncho on the left
shoulder), and the first pass `idle.png`, `zoom.png`. Nothing was
checked in game by the writer.

## Review follow-ups (PR 2, after 5a39575, 890c51f and 2932c93)

The native review approved all three commits with warnings. Fixed in one
follow-up, uncommitted (route: inline, by the PR 2 writer):

- [x] R1 `gait::lift_legs` folded the hind legs about a skewed axis: the
  parent chain dropped the last two entries, which for a hind leg (its
  chain ends in a doubled `End`) still held the lower bone. The fold axis is
  now worked out in the lower bone's parent frame, for every leg.
- [x] R2 A respawned rider scene broke the tack. The stirrups were parented
  to the rider's foot bones and went with them, and the reins kept pointing
  at the old hand. Now the stirrups stay under the horse's `Torso`, and
  `rider::carry_stirrups` places them on the feet each frame (`OnFoot`).
  `seat_riders` runs on `Changed<Seated>`, so a re-dressed rider re-wires
  the reins and the stirrups.
- [x] R3 `dress_tack` checks the new scene (the `Main` skin, `Torso`,
  `Neck2`, `Head`) first. It then takes off the old tack, stirrups
  included. When the scene is not whole, it removes `Tack` instead of
  leaving it pointing at despawned entities. `stretch_straps` hides a strap
  with a missing end and shows it again once both ends are back. The straps
  and stirrups are now ordered before `VisibilityPropagate`.
- [x] R4 The rider's tuning literals are named, documented constants:
  - the springs: `SADDLE_EASE`, `BRAKING_EASE`, `REST_EASE`,
    `HALF_SEAT_EASE`, `LEAN_EASE`, `COUNTER_EASE` (fast on purpose),
    `BIT_EASE`, `HEAD_EASE`, `SHRUG_EASE`, `FREE_EASE`;
  - the motion thresholds: `BRAKING`, `STANDING`, `WALK_TOP`, `SHIFT`,
    `GIVE_MAX`, `FLEX_MAX`;
  - the free hand: `FREE_SWING`, `FREE_WALK`, `FREE_BRAKE`;
  - the hat-brim touch: `HAT_SALT`, `HAT_PACE`, `GLANCE_MAX`.

  Three are coupled to other systems: `BRAKING` and `WALK_TOP` to
  `pampa_core::RiderParams`, and `GLANCE_MAX` to `pose::glance`. Behaviour
  is unchanged: the existing tests pass as they were.
- [x] R5 The H2–H4 proof lists now name the actual test paths.

Sabotages (plan 9, each restored byte for byte; every one failed):

| Test | Sabotage → result |
|---|---|
| `gait::a_folded_hind_leg_stays_in_the_bodys_sagittal_plane` | the old slice → the hoof drifts 0.7–1.2 mm sideways, fails (the fixed fold keeps it within 1e-7 m) |
| `rider::a_respawned_rider_keeps_the_stirrups_on_its_new_feet_and_the_reins_in_its_new_hand` | `Added<Seated>` only → fails |
| `rider::a_rider_sits_on_the_horses_torso…` (changed) | stirrups never carried → fails |
| `tack::a_scene_spawned_again_is_checked_before_the_old_tack_comes_off` | `Tack` kept on a failed dress → fails; old stirrups left on → fails |
| `tack::a_strap_with_an_end_gone_is_hidden_until_it_has_both_again` | never hidden → fails; never shown again → fails |

Verification:
- tools: 112 OK;
- core 344, content 32, render 296 (1 ignored; four runs), game 118;
- build, fmt and clippy are clean.

## Next step (PR 2)

- The user commits the three commits (patches and commands in the
  writer's report), then the native review per commit.
- In game: ride Don Hidalgo's bayo (walk, the trot band at 4–5 m/s, the
  gallop, braking), look at the tack from the side and the front, the reins
  in the left hand, the rider's give at the gallop, dismount and remount, and
  a tied horse in another town. Watch the gallop's legs at the hand-off and
  the hooves at the stretched reach.
- Possible follow-ups: textures on the preview's rider (the renderer only);
  no terrain IK for the hooves; townsfolk riders have no model, so a mounted
  townsperson would ride an empty saddle.
- Then PR 3, townsfolk (T7, T8).

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
- 2026-10-25: PR 2 (H1–H6) implemented on `feat/character-clips`, with the
  user's playtest fixes (P4–P8), uncommitted: the user commits. Native
  review pending, under the user's RDD switch.
- 2026-10-25: PR 3 (P1–P6, W1–W2) implemented on `feat/townsfolk-models`,
  uncommitted: the user commits.

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
