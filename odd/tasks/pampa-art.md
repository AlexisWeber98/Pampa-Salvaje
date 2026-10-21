# Feature: pampa-art

Locator: `odd/tasks/pampa-art.md` · Engram mirror: `odd/pampa-art/tasks`

## Objective

Make the Pampa line feel alive. Players start standing on the station platform. Characters and horses become 3D low-poly figures that match the buildings and the train. The open land between towns gets grassland, native trees, wire fences and a direct dirt road with level crossings.

## Problem / Why

- The game starts beside the Monte Nievas platform, not on it, because the platform is a solid 0.9 m obstacle.
- Characters and horses are simple low-resolution billboards with few frames, and every horse looks alike.
- The land between towns is bare faceted ground with scattered rocks and shrubs, so the 2.3 km ride to Castex feels empty.

## Rules chosen (user-approved 2026-10-20)

- **Platform:** the platform is a walkable raised floor. Its top height is part of the ground height where it stands, not a collision box. The player starts on top of it, beside the passenger coaches of the dwelling morning train. People can walk off its ends or its town-side edge (pick one, and keep it consistent). The track side stays blocked by the train when one is present.
- **Characters and horses (style: 3D low-poly, user decision 2026-10-21; it replaces the richer-pixel-art plan, which is kept as a patch in the scratchpad):**
  - Faceted meshes generated in code, like the buildings and the train, with flat colours and no large textures. They look right from any camera angle and use little memory.
  - Animation rotates rigid parts (head, torso, arms, legs; the horse's neck, legs and tail). Gaits: idle (breathing), walk and jog for people; idle, walk, trot (new) and gallop for horses. The cadence follows speed.
  - **The protagonist is not a gaucho.** He is a former hardware-shop clerk from General Pico, adapting to life in a small town. The story starts in 1919 (user correction 2026-10-21; see the setting guide `odd/setting-1919.md`), and he is humble, not wealthy (user correction 2026-10-21). He wears a townsman's working clothes: a plain work shirt (collarless or soft collar), a worn, scuffed brown leather jacket, faded trousers, plain used boots and a gacho (a soft wide-brimmed felt hat, brim turned slightly down). No suit, stiff collar, tie or bowler (those are for the few town gentlemen), and no poncho, bombacha or faja: he is from town, not the countryside.
  - Every look follows `odd/setting-1919.md` (earthy, muted palette; bright colour only in accents). Townsfolk wear period rural clothes (with faja, pañuelo and immigrant variety): boina, bombacha, alpargatas and poncho, and the women wear ankle-length skirts, high-necked long-sleeved blouses, and a shawl or headscarf. Don Hidalgo is an older estanciero (white moustache, hat, waistcoat). Comisario Elías Farach wears a police uniform (kepi, dark tunic with brass buttons, dark trousers, boots).
  - Horse coats: zaino, tordillo, overo, alazán and bayo (bayo added by the user 2026-10-21), chosen deterministically by seed. Hidalgo's horse, the one the player rides, is bayo.
  - Everything stays procedural.
- **Terrain between towns:**
  - Grassland with tufts (paja, flechilla) and colour variation from dry yellow to grey-green.
  - Caldén and chañar trees, scattered and in small groves. Trees are obstacles; they cast shadows.
  - A dirt road: a direct track between neighbouring towns, not only along the rails. It crosses the railway at level crossings where needed, and it is visible and slightly flattened.
  - Wire fences (alambrados) dividing fields, with gates (tranqueras) where the road crosses them. Fences are obstacles; gates are open.
  - All of this is streamed by chunk, deterministic and seeded, and kept clear of towns' leveled bands, the track and the road.

## Scope and delivery

Three PRs. The user commits and pushes; Claude never commits. Each PR is reviewed on its own commits.

1. **PR-A `feat/platform-start`:** walkable platform and the start on it.
2. **PR-B:** richer character and horse sprites.
3. **PR-C:** grassland, trees, dirt road with level crossings, fences and gates.

The line-train boarding PR comes after these three.

## Constraints

- `pampa-core` stays dependency-free. Placement rules (road, fences, trees) are pure and tested in core; render only draws.
- Everything is deterministic.
- About 400 lines per task is advisory only.
- Keep each PR under about 1500 changed lines so the native review fits.

## Tasks

- [x] **T1 (PR-A)** Walkable raised platform, a ground height that includes the platform top, the start spot on the platform, and the camera and townsfolk handling it. Route: delegated writer (trigger: 2+ non-trivial files across core and game). Uncommitted; the user commits.
- [x] **T1b (PR-A2 `feat/camera-collision`, user-approved 2026-10-20)** Camera collision. The line train's cars become moving camera occluders, and so does the platform canopy roof. Pull-in is smoothed: fast in, slow out. Fading thin occluders (trees, fences) is deferred to PR-C. Route: delegated writer (trigger: 2+ non-trivial files across core, render and game). Uncommitted; the user commits.
- [x] **T2 (PR-B1)** 3D low-poly people: a part-based mesh builder and rigid-part animation (idle, walk, jog). Covers the city protagonist (on foot and mounted), the townsfolk looks, Don Hidalgo and Farach. The billboard path is retired for people. Route: delegated writer (trigger: 2+ non-trivial files across render, core and game). Uncommitted; the user commits. Known interim: the mounted player still shows the old gaucho rider billboard until T3.
- [x] **T3 (PR-B2)** 3D low-poly horses at the same PS1/PS2-like fidelity as the people. Covers the five coats (zaino, tordillo, overo, alazán, bayo), the gaits (idle, walk, the new Trot, gallop) and the mounted pose with the clerk in the saddle. The billboard path is removed completely. Route: delegated writer (trigger: 2+ non-trivial files across core, render and game). Uncommitted; the user commits (commit A).
- [x] **T3b (PR-B2 or its own PR if it is too large; user, 2026-10-21)** Visual polish without extra triangles, for **people and horses**:
  - **PBR materials per material:** leather slightly glossy, brass buttons and rastra coins metallic, wool and cloth matte, skin satin, horse coat with a slight sheen, hooves dark and glossy.
  - **Baked vertex ambient occlusion:** armpits, crotch, under the hat brim, under the jaw, between the horse's legs, under the saddle.
  - **Small procedural textures (64–128 px):** faces (eyes, brows, mouth), cloth weave on the bombacha and poncho, leather grain, felt on hats. Horse faces get eyes and nostrils, and the lucero or estrella markings.
  - **Secondary motion:** the hat brim follows the step, the poncho and skirt sway, idle figures glance around, the horse swishes its tail, flicks its ears and nods its head with the gait.
  - **Sun shadows:** if the cost is acceptable, raise the sun shadow-map resolution and tune the cascades so figures cast crisper shadows. Report the measured cost.
  - Route: delegated writer (trigger: 2+ non-trivial files across core and render). Uncommitted; the user commits (commit B).
- [x] **T4a/T5a (PR-C1a `feat/pampa-grassland`)**
  - Core `pampa_core::flora`:
    - per-chunk deterministic scatter of tufts, caldén and chañar
    - a low-frequency grove mask
    - keep-outs for the towns' leveled bands, the rail corridor and the world edge
    - every trunk as a static obstacle, planted once for the whole world at load (113,279 trees in about 117 ms)
  - Render `flora.rs`:
    - simple faceted trees: caldén about 525 triangles, chañar about 300
    - shared tuft meshes
    - per-vertex ground colours, continuous across chunk borders
  - Route: delegated writer (trigger: 2+ non-trivial files across core and render). The parent applied the final clippy fix (`as_chunks`) and wrote the docs.
- [x] **T4b/T5b (PR-C1b, re-planned by the user 2026-10-22)** The detail pass for trees, grass and ground uses **free third-party models and textures (CC0)** instead of more procedural geometry. Candidate sources: Quaternius or Kenney low-poly nature (trees, grass, rocks), and ambientCG ground textures. Acacia-type models stand in for the caldén (a *Prosopis* with an umbrella crown). Keep `flora`'s scatter, groves, keep-outs, collision and streaming, and only swap the drawn meshes, imported as glTF. Credit every asset where it is used and in a root `CREDITS.md` (author, URL, licence, what changed). This changes the "all procedural" rule for vegetation. The procedural detail pass that was started was stopped and discarded; it is backed up in the session scratchpad as `pr-c1-full-backup.patch`. Route: delegated writer (trigger: 2+ non-trivial files across render, game and assets). The pack ships OBJ/MTL, not glTF, so our own small OBJ reader replaced the glTF import; Bevy's glTF loader would also have needed new features. Uncommitted; the user commits.
- [ ] **T4c/T5c (PR-C2)** Core: the direct road network between neighbouring towns, with level crossings, a fence layout with gates. Render: road surface, fences, gates and crossings, streamed by chunk. Minimap: draw the road. Camera: fading for thin occluders.

## Checks

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build -p game
```

## Progress / Evidence

- Branch `feat/platform-start` from `main` at 7d2304b. PR4 (errand) is merged.
- **T1 done (uncommitted, for the user to review and commit).**
  - Design: `pampa_core::Floor` (one per platform, `TownLayout::floors`) with `rise_at(p)`: the platform's top on its footprint, a 3 m linear ramp past each end (along the track), and plain steps on the long sides. `World::ground_height(p)` = terrain + highest floor rise; it is the one function for where bodies stand. A small `Ground` trait (impl for `World` and bare `Terrain`) lets `Rider::new`/`on_foot`/`mount` and `FollowCamera::desired` take the world.
  - On and off: ramps at both ends, plus free stepping up or down the town side (and the track side, where a dwelling train's cars, already moving obstacles, block the way). The platform is no longer in `TownLayout::obstacles` or the camera occluders.
  - Start: `TownLayout::platform_spot` is on the platform, half way along it (where the coaches' middle stops, the station being centered on it), 1 m in from the track edge (about 2.5 m from the cars' side, 4 m from the rails), facing town.
  - Render: unchanged platform box, plus one sloping concrete slab per ramp. Game: player spawn, townsfolk, the tethered horse, errand NPCs and the camera floor use `ground_height`.
  - Tests (core RED observed, 6 failing, then GREEN): `world::the_ground_on_a_platform_is_its_top_and_its_ramps_lead_down`, `town::the_platform_is_a_floor_with_clear_ramps_not_an_obstacle`, `town::obstacles_cover_buildings_and_trees` (updated), `places::the_platform_spot_is_on_the_platform_by_the_track_facing_town` (replaces the past-the-end test), `places::the_game_starts_on_the_platform_beside_the_dwelling_coaches`, `walker::a_person_walks_off_the_platform_into_town_down_a_ramp_or_its_edge`. Game: `camera::on_the_platform_the_camera_stays_above_it_and_is_not_blocked_by_it` (RED confirmed with the occluder filter removed); the headless errand test now also checks the player stands on the platform's top.
  - Checks: `cargo fmt --all -- --check` ok; `cargo clippy --workspace --all-targets -- -D warnings` ok; `cargo test --workspace` ok (core 192, game 59, render 99, content 25); `cargo build -p game` ok.

- **T1b done (uncommitted on `feat/camera-collision` from `main` at ef569f2, for the user to review and commit).**
  - Occluder model: `Occluder { footprint, bottom, top }` in `game/src/camera.rs`. `Occluder::column` is a building or car from the ground; the canopy is a slab. `clear_reach` intersects the line's footprint range with the range where its height is strictly between bottom and top, so a line under the canopy is clear and one crossing its roof is not. `pull_in` (tests only) lerps to that reach.
  - Train: `PlacedCar::boxes()` in core (the short-box split `LineTrain::obstacles` already used, now shared); `CarKind::height()` (top above the rails: locomotive 4.55, coach 4.16, boxcar 4.06, hopper 3.56) and `LineTrain::RAIL_TOP` (0.5, now also used by render). `car_occluders` rebuilds them each frame from `WorldTrain`, only for cars within 30 m plus half their length of the rider; top = higher end's terrain + rail top + car height.
  - Canopy: `pampa_core::Canopy` with `TownLayout::canopies()` (each station's back wall, `DEPTH` 5.5 m, `HEIGHT` 3.6 m, `THICKNESS` 0.12 m). Render's station veranda uses these constants; `CameraOccluders::from_town` adds the roof slab over the terrain. Posts stay out.
  - Smoothing: `CameraReach` component (fraction of the arm) and `ease_reach`. Pull-in is instant, which the never-inside rule requires (a 0.05 s ease-in would leave the camera inside the wall for a few frames). Ease-out is exponential with `EASE_OUT` = 0.35 s, so it is frame-rate independent. Easing works on the fraction, so zooming stays immediate. Fading thin occluders is deferred to PR-C.
  - Tests (RED observed, then GREEN): core (3 RED) `town::the_station_canopy_hangs_over_the_platform_from_the_track_side_wall`, `train::a_car_on_a_slanted_link_is_covered_by_short_boxes_along_it`, `train::cars_stand_a_little_over_four_meters_above_the_rails`. Game (7 RED with stubs, including the updated platform test's occluder count) `camera::pulls_in_in_front_of_a_train_car_beside_the_player` (axis-aligned and slanted), `far_cars_are_left_out`, `looks_under_a_canopy_but_not_through_its_roof`, `the_platform_canopy_is_a_roof_over_the_platform`, `pulls_in_at_once_and_eases_out_slowly`, `eases_back_out_when_the_view_clears`, `on_the_platform_the_camera_stays_in_front_of_the_dwelling_train` (RED: the camera ended 3.3 m beyond the cars' centerline). Render: `train::each_car_is_drawn_as_tall_as_core_says` (a consistency check, written after the heights, so no RED).
  - Checks: `cargo fmt --all -- --check` ok; `cargo clippy --workspace --all-targets -- -D warnings` ok; `cargo test --workspace` ok (core 195, game 66, render 100, content 25); `cargo build -p game` ok.

- **T2 done (uncommitted on `feat/lowpoly-characters` from `main` at 47ea5fa, for the user to review and commit).**
  - Style: the user found a first all-box pass "too Minecraft" and asked for PS1/PS2-era detail (2026-10-21). Parts are lofted rings (tapered limbs with rounded ends, a torso wider at the chest with shoulders, a coat that flares at the hem, shoe-shaped feet, rounded hands), an icosphere head (2 subdivisions) with jaw and nose bumps, round 16-segment brims and crowns, a lumpy boina, a flared skirt and a hard-folded poncho. Smooth normals on skin and rounds; hard edges only on brims, discs and the poncho. Flat colours with per-vertex shades (darker hems, chin underside and limb ends). Limb segments overlap past each joint. About 7.5 heads tall.
  - 1919 looks (user corrections 2026-10-21, per `odd/setting-1919.md`): the protagonist as a humble townsman in a work shirt, a scuffed leather jacket (collar, lapels, pockets), faded trousers, used boots and a gacho (the bowler, stiff collar and tie stay built for later town gentlemen but nobody wears them); paisano with boina, bombacha, faja, pañuelo and alpargatas; peón with chaleco, soft hat, boots and a white pañuelo; a Volga German farmhand in a poncho; two women with ankle-length skirts (hem at about 0.12 m), high-necked blouses and shawls, one headscarf and one bun; Hidalgo with a good jacket, waistcoat, bombacha, tall boots, rastra and light wide-brimmed hat; Farach with kepi, buttoned dark blue tunic, leather belt, dark trousers and tall boots. Muted palette with accents only.
  - Model: `render::character::figure`. `FigureKind` (Protagonist, `Townsfolk(0..5)`, Hidalgo, Farach) picks a `Look`; `figure_parts(kind)` lists `PartSpec { piece, joint, shape, color, center, size }`. Eleven joints (pelvis, waist, neck, shoulders, elbows, hips, knees) form a tree under a body entity turned by `Quat::from_rotation_y(heading)` (front is local +X). Height 1.74 m to the top of the head, about 1.8 m with a hat.
  - Animation: `pampa_core::pose` (pure): `pose(animation, phase) -> JointAngles`, `cadence`, `advance` (frame-rate independent). Idle breathes, walk swings legs in opposite phase with counter-swinging arms and a head bob, jog (`Gallop` on foot) swings wider and leans forward.
  - Caching: `FigureKit` holds one unit mesh per `Shape` (19 shapes) and one `StandardMaterial` per colour, shared by every figure, so Bevy batches them; nothing is rebuilt per figure or per frame.
  - Detail (two passes of +20% and +30%, user requests 2026-10-21): `LIMB_SIDES` 14 (limbs, neck, hands, feet) and `BODY_SIDES` 16 (torso, hips, coats); eight loft rings per limb so knees, calves and elbows read; an 18 x 12 UV-sphere head (cheekbones, chin) with jaw, brow, ears, nose, darker eye sockets and a moustache with volume; thumbs; sleeve or shirt cuffs, the leather jacket's hem seam, the kepi's crown seam; soles under shoes and boots; trouser creases toward the knee; a wavy gacho brim; brims at 24 segments; 12 deep folds in skirts and 10 in ponchos.
  - Triangles: 4,208–5,000 per figure (Protagonist 5,000, Hidalgo 4,644, Farach 4,680, townsfolk 4,208 / 4,256 / 4,472 / 4,716 / 4,868). Earlier passes: 3,024–3,692 (+20% pass), 2,464–2,860 (first PS1/PS2 pass), 192–252 (all boxes).
  - Swap: `CharacterVisual::Figure(FigureKind)` alongside `Billboard(SheetKind)`. Each visual's drawing child carries `VisualPart`; when `CharacterVisual` changes, render despawns those children (recursively) and draws the new one, so the errand only inserts `player_visual(Travel::Riding)`.
  - Sheets: `SheetKind` keeps only `Rider` and `Horse`; the seven person sheets (Gaucho, Hidalgo, Comisario, four Person looks; 640 x 400 RGBA8 each) are gone, saving about 7.2 MB of textures (10.8 MB down to 3.7 MB).
  - Shadows: the sun already casts shadow maps, so the figures cast real shadows; no ground disc.
  - Tests (RED observed): core `pose::tests` (4, RED against stubbed `pose`/`cadence`); render `figure::tests` (looks build at 1.6–1.9 m and deterministic, the protagonist as a humble townsman (gacho, leather jacket with lapels, boots; no bowler, tie, stiff collar, poncho, bombacha or faja; RED as compile errors against the suit version, then as an assertion failure with the suit-style protagonist restored), women's ankle-length skirts (RED with a shortened skirt: hem at 0.34 m), errand people's own looks, heading faces the nose along the ground heading, 3,500–5,500 triangles, smooth normals on head and limbs) and `visual::tests` (figure-to-rider swap leaves no parts, a 20-person crowd shares meshes and materials, a walking figure turns and swings its legs); 7 RED against stubbed parts, rotation and despawn, caching RED with the material cache disabled. Game: `errand::tests::mounting_swaps_the_clerk_figure_for_one_rider_and_nothing_else` with `CharacterVisualPlugin` present (RED with the despawn disabled: `[1, 1, 11, 23]` left under the player); player, life and errand assertions updated to `Figure` (RED as compile errors once the person sheets were gone).
  - Checks: `cargo fmt --all -- --check` ok; `cargo clippy --workspace --all-targets -- -D warnings` ok; `cargo test --workspace` ok (core 199, game 67, render 107, content 25); `cargo build -p game` ok. Headless snapshots not rendered (no GPU path in the tests).

- **T3 and T3b done (uncommitted on `feat/lowpoly-horses` from `main` at 34b39b8, for the user to review and commit as two commits).**
  - Horse (`render::character::horse`): a body entity with 18 joints (trunk, neck, poll, two ears, tail, and upper, knee or hock, and fetlock per leg). Lofted parts in the people's style: barrel, chest, croup, withers, a neck with a short upright mane, a head with muzzle, jowls, eyes, nostrils, ears and forelock, a dock and full tail, and per leg a shoulder or thigh, forearm or gaskin, knee or hock, cannon, fetlock, pastern and hoof. A criollo of 1.45 m at the withers, about 1.5 m from chest to buttock. **5,240 triangles**, plus 700 for the tack (people 4,208–5,000). Meshes are cached in `HorseKit`: unit meshes, plus painted meshes per (coat, part) built the first time a coat is drawn.
  - Coats (`Coat`): painted per vertex from rest-pose positions, so patterns cross parts. Zaino solid dark brown; tordillo grey with dapples; overo bay with eight white blobs and two stockings, so 13–17% of the visible coat is white from every 3/4 view; alazán chestnut; bayo dun with black mane, tail and points and a dorsal stripe. `Coat::HIDALGO` is bayo. In `game/src/life.rs`, `tethered_coat` gives Hidalgo's town the bayo; every other town gets `Coat::seeded(town seed)`.
  - Tack (1919 recado): a woven sudadera, a leather carona, two bastos, a shaggy sheepskin pellón (the seat), a rawhide cincha with iron rings, stirrup leathers and iron stirrups, and a rawhide headstall (crownpiece, noseband, cheekpieces) with bit rings and reins. When mounted, the stirrups sit at the rider's feet and the reins run to his hands; when tied, the stirrups hang straight and the reins lie over the pommel.
  - Gaits (`pampa_core::pose`): `HorseGait::of(animation, speed)`. Logic keeps Idle, Walk and Gallop; the horse trots from the walk limit up to `TROT_MAX` = 6.5 m/s. Each `horse_pose(gait, phase)` leg pushes back in stance and swings forward folded. Walk is four-beat (offsets 0, 0.25, 0.5, 0.75 in lateral sequence, duty 0.62). The trot moves diagonal pairs (duty 0.42). The gallop is hind then fore, with a suspension, body pitch and a big neck swing. Idle breathes and rests one hind hoof. `horse_cadence` follows speed (strides of 1.7, 2.6 and 4.4 m) and `horse_advance` is frame-rate independent.
  - Mounted: `CharacterVisual::{Figure, Horse(Coat), Mounted { rider, coat }}`. The rider's figure hangs from the horse's trunk joint at `rider_offset()`, with its hips on `SEAT`, the pellón's top (1.49 m). It uses `riding_pose`: thighs forward and open (`JointAngles::spread`), shins hanging, hands at the reins following the nod, and a lean at the gallop. `player_visual(Riding)` is the clerk on Hidalgo's bayo.
  - Billboards retired: `CharacterVisual::Billboard`, `SheetKind`, the sprite materials, `ViewDirection` and `ground_yaw` are gone from the module tree. `sheet.rs`, `paint.rs` and `direction.rs` are no longer compiled, but deleting them was refused by the permission system: **the user must `git rm crates/render/src/character/{sheet,paint,direction}.rs`**. The camera test that used `ViewDirection` now checks the orbit bearing directly.
  - Polish (T3b): `render::character::finish`.
    - `Finish` per material, cached by (colour, finish) in one `Palette` (roughness / metallic / reflectance): cloth 0.92/0/0.3, wool and fleece 0.97/0/0.25, felt 0.9/0/0.3, leather 0.5/0/0.45, rawhide 0.62/0/0.4, skin and face 0.55/0/0.4, hair 0.75/0/0.35, brass 0.32/0.92/0.5, silver 0.25/0.95/0.5, iron 0.45/0.85/0.5, coat 0.62/0/0.45, hoof 0.3/0/0.5, eye 0.15/0/0.6.
    - Textures: nine 64 x 64 RGBA8 textures (face, weave, leather grain, felt, five horse faces), **147,456 bytes** in total, against the 3.7 MB of sprite sheets removed.
      - Faces map onto the front of the head through `face_uvs`; the back of the head maps onto a blank row.
      - Lofts now carry UVs with their seam at the back.
      - Horse faces wrap round the head's loft and carry eyes, nostrils, an estrella (alazán) or a blaze (overo).
    - Baked AO: people (armpits, crotch, top of the thighs and arms, under the jaw and under brims) and horses (belly, the inner legs, under the saddle's edge, the throat).
    - Secondary motion, pure and tested in `pose`: `drape` (hat tilt, and the fore-and-aft and side sway of poncho, shawl and skirt, which hang from the new `Brim`, `Drape` and `Hem` joints; figures now have 14 joints), `glance` (idle head turns, eased in render), `ear_flick` and `tail_swish`. The head nod is part of `horse_pose`.
    - Shadows: the sun uses 3 cascades, the first ending at 8 m and the last at 100 m. The shadow map stays at Bevy's default 2048, so cascade memory is a quarter lower than the default 4 cascades over 150 m. The cost was not measured: the tests run headless, with no GPU.
  - Tests (RED observed):
    - Core: 8 horse-pose tests RED against stubs (trot range, four-beat walk, diagonal trot, gallop suspension, folding legs, idle, cadence, riding pose), then GREEN. 3 secondary-motion tests RED against stubs, then GREEN.
    - Render horse tests: withers 1.35–1.6 m, hooves on the ground, compact length; triangles 4,000–6,000; distinct coats; bayo points and stripe; overo white from all four 3/4 views; tordillo dapples; seeding; rider's hips on the seat and stirrups by the feet. Three of these failed against the first model and were fixed by tuning (the back test's range, the bayo flank filter, the overo patches).
    - Render visual tests: mounting and dismounting swap (RED with the despawn disabled: 2 riders), the rider moving with the gallop, and the tied horse with tack.
    - Polish tests: finish, face, horse-face, texture size, occlusion, sway joints and loft UVs. 6 were RED in a probe that broke the leather roughness, face painting, occlusion and the head finish, then GREEN once restored.
    - Atmosphere: `sun_shadows_are_crisp_near_the_rider`.
    - Game: the errand mounting test (RED with the despawn disabled: `(1, 2, 22, 0)`), `don_hidalgos_horse_is_the_bayo_and_other_towns_seed_theirs`, and the player and life assertions updated from billboards.
  - Commit split: **A** (T3: horses, tack, mounted pose, billboard removal) and **B** (T3b polish plus docs). A touches `crates/core/src/{animation,lib,pose}.rs`, `crates/render/src/lib.rs`, `crates/render/src/character/{figure,horse,mod,visual}.rs` and `game/src/{camera,errand,life,player}.rs`; it is +2,087 / −443 lines, plus about 1,100 deleted lines from the three dead files. B touches `crates/core/src/pose.rs`, `crates/render/src/atmosphere.rs`, `crates/render/src/character/{figure,finish,horse,mod,visual}.rs`, `game/src/errand.rs`, `README.md` and this document; it is about +1,325 / −108. A is over the 1,800-line review target. If that is too big, review core `pose.rs` (+430) on its own first.
  - Checks: `cargo fmt --all -- --check` ok; `cargo clippy --workspace --all-targets -- -D warnings` ok; `cargo test --workspace` ok (core 211, game 68, render 107, content 25); `cargo build -p game` ok. Headless snapshots not rendered (no GPU path in the tests).
  - Leftover outside this task's edit surfaces: `crates/render/src/town.rs:321` still says the tethered horse "is a character billboard".

- **T4b/T5b done (uncommitted on `feat/vegetation-assets`, built on `feat/pampa-grassland`, for the user to review and commit).**
  - Loader: `render::obj` parses `v`, `vn`, `vt`, `f` (triangles, quads and n-gons fan-triangulated; `v`, `v/vt`, `v//vn`, `v/vt/vn`; negative indices; flat face normals when `vn` is missing), `usemtl`, `mtllib`, and the MTL `newmtl`/`Kd`. Errors are values, never panics. No new crate.
  - Batching: each model is baked into **one mesh per finish** (bark, leaves, rock), with the recoloured `Kd` in vertex colours, normalized to 1 unit tall on the origin. Every model then shares 5 materials, plus 4 grass tints. This keeps the PBR finishes per material (bark 0.96, leaves 0.88, rock 0.93 roughness; new `Finish::Rock`) while Bevy batches every tree of a variant with its two meshes. A single vertex-coloured mesh per model would have lost the per-finish roughness; one mesh per MTL material would have doubled entities for Green/DarkGreen.
  - Loading: `flora::load_vegetation` in Startup (`VegetationLoad` set; the terrain's chunk fill runs after it), from `AssetRoot` (the game inserts `pampa_content::asset_path("")`). It caches handles in the `VegetationModels` resource. 28 models, 24,024 source triangles, in **27 ms** (dev build). A missing file logs a warning and that model's procedural stand-in is drawn.
  - Mapping and recolour (`tree_model`, `tuft_model`, `recolour`):
    - CommonTree_1–5 → caldén: dull olive leaves, dark bark, height 6–9 m, crown widened up to 25%.
    - CommonTree_Dead_1–5 → snags: caldén variants 0–2 of 16, which is **10.4% of all trees**.
    - Bush_1–2 → chañar thickets at 70% width, and small jarilla shrubs among the terrain props.
    - Grass, Grass_2, Grass_Short, Wheat → tufts (paja: Wheat/Grass_2; flechilla: Grass/Grass_Short), pale straw × 4 tints from grey-green to dry yellow, chosen by the ground's dryness plus a seeded jitter.
    - Plant_1–5 and Flowers → about 6% of the tufts.
    - Rock_1–4 → the terrain's rock props, in tosca beige.
    - TreeStump and WoodLog → beside 6% of the caldenes.
    - Yaw is seeded; scale variation comes from core's seeded height and size ranges, plus ±15% on accents.
  - Collision: the caldén model's trunk is 0.029–0.041 × height, which matches core's 0.035 × height, so caldenes are unchanged. **Chañar changed**: the bush thicket is about 0.4 × height wide, so `Tree::trunk_radius` for chañar went from 0.03 to 0.09 × height (its stem core). Placement and spacing are unchanged; tufts keep a little more clearance from chañares.
  - Culling (`VisibilityRange`, solid from the camera, one 15 m dithered fade-out at the far end, no LOD swap): every tree (caldén, snag, chañar) is its own model out to 200 m and fades out over 185–200 m, inside the 180 m dust fog. Stumps and logs go to 120 m, and model stones and shrubs to 160 m. Tufts are solid out to 65 m from the camera (40 m from the rider at the orbit's 25 m longest zoom); past that each starts fading at its own seeded distance, up to 50 m farther, so the grass thins out smoothly.
  - **Bug fix (tree popping, reported in play: "trees disappear and reappear, or change into a different tree")**:
    - Cause 1: the caldén model was drawn only to 70 m, then the old procedural stand-in, a different shape, out to 210 m.
    - Cause 2: `visible_between` faded the model out over 62–70 m but faded the stand-in in over 70–78 m. The two margins did not line up, so neither was solid across 62–78 m, and at 70 m neither was drawn. Orbiting the camera swept trees through that band.
    - Cause 3: tufts began fading 22 m from the camera, which is next to the rider at long zoom.
    - Not a cause: placement and every model pick, yaw and scale are seeded by chunk and spot, and chunks stream out to 400 m, past the fog.
    - Fix: the stand-in is only a fallback for a missing model file. `visible_within(far, fade)` replaces `visible_between`, with no near cut-off. `TREE_DETAIL` and the shared 8 m `FADE` are gone. The new visible-triangle estimate was well under 1.2M, so no simplified far mesh was needed.
  - Triangles per model: CommonTree 1,352–2,952; Dead 944–2,360; Bush 268–364; Grass ×3 192; Wheat 408; Plant 218–620; Flowers 408; Rock 70–128; Stump 232; Log 464. Estimate at radius 3 over the densest grove (90 trees), after the popping fix: **about 416k drawn** (93k ground on 29 in-world chunks, 324k plants) of 3.56M loaded. Before the fix it was 259k. A full 49-chunk ring adds about 64k ground, so it stays well under the 1.2M budget the test now asserts.
  - Ground (`render::ground`):
    - The ambientCG maps are read as 512×512 binary PNM, converted from the JPGs by the parent with ImageMagick. Bevy's `jpeg` feature needs zune-jpeg 0.5, which is not in the offline registry.
    - A PNM P5/P6 reader handles comments and maxval scaling and returns errors.
    - Ground109 and Grass004 are blended once on the CPU: soil shows through the dark gaps of the grass. The result is one near-grey, low-contrast colour map (sRGB), a normal map (relief ×0.6), and an AO-in-R, roughness-in-G map used as both the occlusion and the metallic-roughness texture. All are with box-filtered mips and 8× anisotropic repeat sampling.
    - One `StandardMaterial` tinted by the existing vertex colour, chosen over a custom two-texture material: it needs no shader and keeps one material for every chunk.
    - World-space UVs on a 6 m tile, with flat per-face tangents.
    - Blend time: **54 ms** (dev build). Missing files log a warning, and the plain vertex-painted ground is drawn.
    - Towns keep their plain earth colour. The detail is quiet: mean 0.8–0.97, mean chroma under 0.08.
  - Tests:
    - obj: 5 (3 RED against stubs, then GREEN), including all 28 shipped models.
    - ground: 5 (PNM parse RED against a stub; blend neutrality, mips, tangents).
    - terrain: 2 (the material wears the 4 textures with 10 mips or falls back; UVs continuous at chunk borders, with tangents). Both RED, then GREEN.
    - flora: 4 (kind → model set and snag share; recolour away from the source green, trunk thickness and widened crown, tosca rocks; models replace the stand-ins, and missing ones fall back; visible triangles under budget), written alongside the code, so no RED.
    - flora, popping fix: 4 more. A tree draws the same meshes at 20, 69, 71 and 150 m. Ranges leave no gap before the fog: trees are solid to 180 m, and tufts, stones and accents are solid to 65 m. A reloaded chunk shows the same meshes, materials and transforms, bit for bit. Stand-ins are drawn only for missing models. Three were RED against the old code: the same model at every distance, no gap, and stand-ins only for missing models. The reload test was green from the start; it guards determinism.
    - core: the chañar obstacle (RED, then GREEN).
    - Existing flora, terrain, streaming and camera tests are unchanged and green.
  - Commit split (over the 1,800-line target: +1,959 / −96 in code and README, plus the CREDITS files):
    - **(a) loader and models**: `crates/render/src/{obj,flora,lib}.rs`, `character/finish.rs`, the stone and shrub part of `terrain.rs`, `crates/core/src/flora.rs`, `game/src/main.rs`.
    - **(b) ground textures and docs**: `crates/render/src/ground.rs`, the UV, tangent and material part of `terrain.rs` (`git add -p`), the `ground` module line in `lib.rs`, `README.md`, the CREDITS files and this document.
  - Checks: `cargo fmt --all -- --check` ok; `cargo clippy --workspace --all-targets -- -D warnings` ok; `cargo test --workspace` ok (core 217, game 69, render 127, content 25); `cargo build -p game` ok. A 25 s live run logged no errors. After the popping fix, all four checks pass again, with render at 131 tests.
  - For the user: delete the ground JPGs once the PNMs are accepted (the game does not read them), and update `CREDITS.md` then.

## Next step

The user reviews and commits PR-C1a on `feat/pampa-grassland` and PR-C1b on `feat/vegetation-assets`. Next is PR-C2: the road, fences and gates, level crossings, Ground102 on the road surface, and camera fading for thin occluders.
