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
- [ ] **T3 (PR-B2)** 3D low-poly horses at the same PS1/PS2-like fidelity as the people. Covers the five coats (zaino, tordillo, overo, alazán, bayo), the gaits (idle, walk, the new Trot, gallop) and the mounted pose with the clerk in the saddle. The billboard path is removed completely.
- [ ] **T3b (PR-B2 or its own PR if it is too large; user, 2026-10-21)** Visual polish without extra triangles, for **people and horses**:
  - **PBR materials per material:** leather slightly glossy, brass buttons and rastra coins metallic, wool and cloth matte, skin satin, horse coat with a slight sheen, hooves dark and glossy.
  - **Baked vertex ambient occlusion:** armpits, crotch, under the hat brim, under the jaw, between the horse's legs, under the saddle.
  - **Small procedural textures (64–128 px):** faces (eyes, brows, mouth), cloth weave on the bombacha and poncho, leather grain, felt on hats. Horse faces get eyes and nostrils, and the lucero or estrella markings.
  - **Secondary motion:** the hat brim follows the step, the poncho and skirt sway, idle figures glance around, the horse swishes its tail, flicks its ears and nods its head with the gait.
  - **Sun shadows:** if the cost is acceptable, raise the sun shadow-map resolution and tune the cascades so figures cast crisper shadows. Report the measured cost.
- [ ] **T4 (PR-C)** Core: direct road network between neighbouring towns, with level crossings, fence layout with gates, and tree and tuft scatter rules.
- [ ] **T5 (PR-C)** Render: grass colour variation, tufts, trees, road surface, fences, gates and crossings, streamed by chunk. Minimap: draw the road.

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

## Next step

The user reviews and commits T2 (PR-B1). Next is T3 (PR-B2): 3D low-poly horses (five coats, trot, the mounted pose with the clerk in the saddle), which retires the last billboards.
