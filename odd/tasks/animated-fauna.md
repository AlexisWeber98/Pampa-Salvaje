# Animated fauna: Quaternius cattle and horses

## Objective (user, 2026-10-24)

The procedural cattle look like blocks ("las vacas son feas"). Replace the
cows, add a bull, and replace the corral horses with the CC0 animated models
from Quaternius. Drive their clips from the existing fauna simulation.

## Inputs

- Downloaded by the user on 2026-10-23 from poly.pizza (CC0, Quaternius) to
  `~/Downloads/quaternius/`:
  - `cow_a.glb`: Cow, https://poly.pizza/m/26zM1outCr
  - `bull.glb`: Bull, https://poly.pizza/m/a8PIIYwF7r
  - `horse_a.glb`: Horse, https://poly.pizza/m/qvTrSG9pZF
- All three come from the same rig family:
  - 42–50 joints and about 2.2–2.5k triangles;
  - flat colour materials (`Main`, `Main_Light`, `Muzzle`, `Hooves`,
    `Eye_*`, `Horns`, `Hair`);
  - 13 clips (Walk, Gallop, Eating, Idle, Idle_Headlow, Idle_2, Death,
    Attack_*, Jump_*), each present twice: once bare and once as
    `AnimalArmature|…`;
  - exported by FBX2glTF in centimetres, scaled through the node tree.
- The `bevy_animation` 0.20.0 crates are already in the cargo cache, so the
  feature builds offline.

## Tasks

- [x] T1 Assets:
  - `tools/glbtrim.py` (standard library, deterministic, untrusted input
    bounded and index-checked, smooth normals by default) with
    `tools/test_glbtrim.py`;
  - the trimmed models in `assets/models/fauna/quaternius/` (`cow.glb`,
    `bull.glb`, `horse.glb`), six clips each in a fixed order;
  - credits in `CREDITS.md` and `assets/models/fauna/quaternius/CREDITS.md`.
- [x] T2 Bevy features `bevy_animation` and `gltf_animation` (the glTF
  loader reads clips only with the latter), built offline.
- [x] T3 Render and game:
  - `crates/render/src/fauna/livestock.rs`: cows, bulls and corral horses
    as skinned glTF scenes, coats tinted per material, one shared graph per
    species, clips from the fauna states, freezing far off or out of view;
  - core: one bull per herd of `Herd::BULL_HERD` head or more;
  - corral horses are fauna (`Animal::Horse`), still `Mountable`;
  - the procedural cows, `CharacterVisual::Bare` and the untacked
    `spawn_horse` path deleted.
- [x] T4 Tests, README, tools README, this document.

Route: one delegated writer, for 2+ non-trivial files.

## Checks

- `cargo test -p pampa-core`, `-p pampa-render`, `-p game`
- `cargo build -p game`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- Tool tests, if the trimming tool is Python:
  `python3 -I -B -m unittest discover -s tools`

## Progress

- 2026-10-24: branch `feat/animated-fauna` from main 4147f3e.
- 2026-10-24: T1–T4 done by one delegated writer (route: delegated
  direct; 2+ non-trivial files). Uncommitted: the user commits.

## Decisions

- **Clips kept**: `Idle`, `Idle_2`, `Idle_Headlow`, `Eating`, `Walk` and
  `Gallop`, in that order (the game loads them by index; a test checks the
  names). Sizes: cow 1,028,584 → 493,248 bytes (2,450 triangles, 42
  joints), bull 1,038,360 → 495,012 (2,418, 42), horse 1,108,124 →
  496,060 (2,182, 50).
- **Scale and heading**: the export is about 4.4 units to a cow's withers
  (3.6 to a horse's). The models are scaled to withers of 1.4 m (cow),
  1.55 m (bull) and 1.5 m (horse), measured in the load test (1.43, 1.58
  and 1.50 m), and turned from +Z to the body's +X like the MakeHuman models.
- **State to clip**: Graze → Eating; Stand → one of the three idles by the
  animal's seed (`AnimalSeed`, from the herd id and index, or the horse's
  footprint); Walk → Walk, played at ground speed / 1.0 m/s (horse 1.5),
  clamped 0.4–2.0; Walk faster than 2.5 m/s or Run → Gallop (no cow runs
  yet). Corral horses never move: they change idle, or crop the ground,
  every 14 s, deterministically. Clips start at a seeded offset and blend
  over 0.3 s.
- **No lying cows** (user decision, 2026-10-24). The user found every
  lying pose unnatural, so cows no longer lie down. The attempts were:
  the model sunk 0.55 m ("sinking in quicksand"), then four skeleton
  poses, posed after the animation systems with the IK feet moved to the
  folded leg tips. They read as a sphinx, then as crawling chin-down, and
  last as a tucked sternal pose with the head up. A hind leg stretched
  out sideways read as a kick on this rig, which hinges the leg at the
  high hip. So `CowState::Lie` and `Herd::ROUSE` are gone from core: a
  cow now grazes, walks or stands (58 / 28 / 14 % of its choices), and
  starts grazing or standing. `Gait::Lie`, the lying module and its
  CPU-skinning preview tests are gone from render, so no dead state is
  left.
- **Smooth normals** (after playtest: "too faceted"; the art direction is
  now smoother and more realistic within the iGPU budget): `glbtrim.py`
  recomputes the normals. It welds by exact position and equal skin
  joints and weights, across a mesh's primitives, and groups faces by
  edges under 60°, weighted by corner angle. Only the NORMAL bytes
  change: the same sizes, vertex and triangle counts. On the cow, 4,860 of
  4,970 normals turned (mean 18.5°), and points with split normals went
  from 1,237 to 206, the sharp edges kept. The vertex-colour coats come
  from positions, so they are unchanged (only the bayo's dorsal stripe
  reads the normal).
- **Bull**: one per herd of 10 or more head, picked from the herd's seed,
  of the herd's breed; still counted in the cow cap.
- **Coats**: material copies per (model material, coat), made once and
  shared, metallic 0 and roughness 0.9 (the export's 0.4 metallic looked
  plastic). Hereford: red `Main`, white `Main_Light` (the model's face and
  lower legs). Angus: black, horns hidden (polled). Overo: approximated
  with vertex colours on one copy of the body mesh per coat: white with
  black blotches from crossed sine waves (about a third black), black face
  and socks. Horses: the procedural horse's `Coat::color_at` baked into
  vertex colours (dapples, overo patches, dorsal stripe and points), mane,
  head and hooves from the same coat.
- **Throttle**: an animal past 120 m from the camera, or not in view,
  freezes on its pose: its player loses its `AnimationGraphHandle`, so the
  animation systems skip its bones. It plays again within 110 m. The
  model's transform is written only when it changes. Meshes use the props'
  shared range (220 m): no new `VisibilityRange`.
- **Spawning**: each species' scene, clips and graph load once at startup;
  each animal spawns the shared scene. A missing or failed model draws
  nothing for that species (logged), since the procedural cattle are gone.

## Verification (2026-10-24)

- `python3 -I -B -m unittest discover -s tools`: 77 tests OK (after
  smooth normals).
- `cargo test -p pampa-core`: 348 passed.
- `cargo test -p pampa-render`: 239 passed after the lying removal (one
  run failed `terrain::tests::a_chunk_gone_before_its_ground_lands_drops_the_build`,
  a timing test outside this feature; it passed on three reruns and in the
  full rerun).
- `cargo test -p game`: 117 passed.
- `cargo build -p game`, `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- Smooth normals: tests written alongside the code, no separate RED
  observed.
- Test-first: the bull test was seen failing (it did not compile) before
  the bull existed. The tool, render and mount tests were written alongside
  the code and passed when first run, so no separate RED was observed.
- Not checked: the look in game (tints, overo patches,
  playback speeds) and the frame time on the Renoir iGPU.

## Next step

Play-check near a herd and a corral (`goto herd`, `fauna` counts): the
smooth shading, the colours and walk speeds. Then commit.

