# Field flora

The user owns commits: every task is left uncommitted for the user's
review.

## Objective (user, 2026-10-27)

"Aprovechando los árboles y texturas que tenemos mejoremos la flora del
campo." Make the open country's trees, grass and estancia plantings look
as good as the town trees, using the bark and leaf textures already in the
repo. Scope chosen by the user: items 1 to 4 below.

## Problem (mapped 2026-10-27)

- Field trees and tufts are flat-coloured Quaternius OBJ models.
  - Recolouring happens in code (`crates/render/src/flora.rs`
    `recolour`/`bake`).
  - There are no UVs and no textures.
  - Leaves are not alpha-cut.
  - Every caldén is one of 5 shapes.
- Placement lives in `crates/core/src/flora.rs`.
  - It has only `Calden` and `Chanar` trees and `Paja` and `Flechilla`
    tufts.
  - Each chunk is seeded.
  - Keep-outs and collision radii come from `trunk_radius`.
- Towns use a much better kit in `crates/render/src/tree.rs`,
  `tree/mesh.rs` and `tree/materials.rs` (the `TreeKit`).
  - It has photographic bark (ambientCG KTX2) and alpha-masked leaf cards
    from `tools/leaf_atlas.py`.
  - It makes 3 seeded variants per species.
  - The fields do not use it.
- Estancia poplars and eucalyptus (`crates/render/src/settlement.rs:735`,
  `settlement/crown.rs`) are a Quaternius trunk under a procedural
  icosphere-blob crown.
- Constraint: tufts fade at one of 8 seeded steps because of a Bevy GPU
  slot leak past 65,535 distinct visibility ranges. Do not add
  per-entity ranges.

## Licence finding

The shipped `assets/models/vegetation/quaternius-ultimate-nature/License.txt`
reads CC0 1.0. A CC0 dedication cannot be revoked for copies already
received, so the newer "Quaternius Asset License v1.0" on quaternius.com
does not apply to this download. This closes the note at
`odd/tasks/rural-settlements.md:189`.

## Tasks

- [x] F0 Docs first:
  - Record the licence finding in `odd/tasks/rural-settlements.md`.
  - Extend `odd/setting-1919.md` with the field species for F4, and with
    the grasses and the estancia plantings.
  - Route: delegated writer (needs sourced research).
- [x] F1 Textured caldén and chañar:
  - Add field forms to the `TreeKit`: the caldén's umbrella crown and dark
    rough bark, and the twiggy chañar.
  - Use the existing bark sets and leaf atlases.
  - Field trees use them, and Quaternius stays as the fallback.
  - Trunk radii stay within core's collision constants.
  - Route: delegated writer.
- [x] F1b Tree variety (user playtest 2026-10-27, screenshot: "me gusta,
  pero veo todos los árboles iguales"):
  - In the playtest, the caldenes all looked the same height and shape.
    They stood in near-even rows, with sparse foliage full of holes, few
    chañares, and smooth orange trunks.
  - Find out why they look identical.
  - Add size and age classes: saplings, young, mature and old giants.
  - Add more distinct crown forms, multi-stem trunks, lean, and denser
    foliage options.
  - Add per-instance yaw and non-uniform scale.
  - Add a few material tint variants that keep batching.
  - Check that the bark texture is visible.
  - Route: delegated writer.
- [x] F1c Monte and brush:
  - Denser caldenal patches (monte and bosque) with an understory.
  - Shrubs and thickets: piquillín, jarilla, chañar thickets (fachinal).
  - Add the shrubs to `odd/setting-1919.md`.
  - New textures can be downloaded by the user if needed (CC0, named
    exactly).
  - Route: delegated writer.
- [x] F1e Solid trees and fewer trees (user, 2026-10-27):
  - "Saquemosle el difuminado transparente a los árboles, ya que estos en
    su momento tendrán su protagonismo". Trees no longer fade when they
    stand between the camera and the rider. This covers field, town and
    settlement trees. The follow camera pulls in to clear a trunk, as it
    already does for train cars. Other thin occluders (fences, lamps,
    roads, buildings) keep their fade.
  - "Ahora tiene mucho árbol, sacale un 20% al total". Remove 20% of all
    field trees (monte, groves and lone trees) evenly. Shrubs stay as
    they are.
  - Route: delegated writer.
- [ ] F1d Leaf litter under the monte: ambientCG `ScatteredLeaves009`
  (1K JPG, downloaded by the user to `~/Downloads`), blended into the
  ground under monte patches. Route: delegated writer.
- [ ] F2 Estancia monte and alameda: replace the blob crowns with textured
  `TreeKit` eucalyptus and poplar. Route: delegated writer.
- [ ] F3 Grass cards: alpha-cut textured cards for paja and flechilla
  replace the flat tufts, within the visibility-range constraint.
  Route: delegated writer.
- [ ] F4 New species: algarrobo, tala, espinillo and a rare ombú. The
  town species (paraíso, olmo, eucalipto, aromo) also appear in the field
  (user, 2026-10-27).
  - The town species are less frequent than the native trees.
  - They are rare in open field and more frequent near roads, puestos and
    taperas, as escaped plantings. This placement is the parent's
    proposal and the user may change it.
  - Every species' bark must be clearly distinguishable (user). That
    means one bark set per species, so new CC0 sets are needed. The user
    downloads them.
  - Add the algarrobo (*Prosopis flexuosa*) to `odd/setting-1919.md`. It
    is a caldén companion.
  - Per `odd/setting-1919.md` (Field flora): tala and espinillo are rare edge trees (a small share, well below caldén). The ombú is planted only beside puestos, estancias and settlements, never in open field.
  - Core: species, sizes and the species roll.
  - Render: the forms.
  - Minimap: draw them.
  - Route: delegated writer.

## Checks

- Test-first where a deterministic test applies (RED, then GREEN). Prove
  each new or changed test by sabotage.
- Focused tests per task.
- At the end: tests of all four crates, clippy and fmt.
- Look and frame time in the game are the user's playtest; tests cannot
  judge them.
- Triangle budgets as in `odd/tasks/town-facades.md` Phase 4.

## Progress

Created 2026-10-27.

- F0 done (2026-10-27): route delegated writer. Licence finding recorded in `odd/tasks/rural-settlements.md`. `odd/setting-1919.md` has a new "Field flora" section with sourced entries and unverified claims marked. Verdicts: tala and espinillo marginal and rare; ombú planted near settlements only; paja, flechilla, cardos and the estancia plantings covered.
- F1 done (2026-10-27, uncommitted): route delegated writer (2+ non-trivial files).
  - Tree kit: `Species::Calden` and `Species::Chanar` (`Species::FIELD`; the town species stay `Species::ALL`, so plazas and yards never plant them), 6 seeded variants each (`FIELD_VARIANTS`).
    - Caldén: Bark014 darkened by a tint, `aromo_leaves` tinted dull grey-green; fork at 0.25 of the height, limbs 45-60 degrees, crown about 1.75 times wider than tall.
    - Variants 4 (sparse) and 5 (dead) are the snags, picked with the old `DEAD_VARIANTS` share (3 of 16 seeds).
    - Chañar: Bark009 tinted green-ochre, `olmo_leaves` tinted yellow-green, 5-7 stems from a low stool, small sparse cards.
    - No new atlas, no new texture files; `assets/textures/trees/CREDITS.md` notes the reuse.
  - `Painter::tree` draws the kit variant at the core height (`kit_variant`), keeping `ThinOccluder`, the single `TREE_FAR`/`TREE_FADE` range and the stump/log accents; Quaternius stays as the fallback when no `TreeKit` exists.
  - A lower fork leans the trunk less (`fork * 5` clamped to 1), which leaves every town tree unchanged. `flora::triangles` now counts indexed meshes by their indices.
  - Triangles: caldén 688-968 live, 480 sparse, 312 dead; chañar 662-770. Densest chunk (90 trees) 73,326, the radius-3 scene 316,520 against 412,780 before. The 1.2M budget stays.
  - Look in game unverified (playtest).
- F1b done (2026-10-27, uncommitted): route delegated writer.
  - Diagnosis: no spread bug. `kit_variant`, the core height and the textured bark were all applied. The trees looked alike because the 4 live caldén variants shared one `Form` (the seed only turned the limbs), core gave one size band and no width, groves were more even than random (nearest-neighbour cv 0.43), and the bark tile (0.5 x 0.9 m) was too fine to survive the mipmaps.
  - Core: `Age` (sapling 10%, young 20%, mature 67%, old giant 2.5%; caldén 2-3.5, 3.5-6, 6-9 and 10-12 m; chañar 1.5-2.8 up to 5-6.5 m) and a per-tree `width` (0.82-1.18, giants 1.15-1.4); `Age::girth` scales the caldén's collision radius; spacing grows with height; `variant` is now 32 seeded bits. Placement clumps (`FloraSite::clump` noise plus a boost beside an existing tree; quadrat dispersion 0.97 to 2.07, nearest-neighbour cv 0.43 to 0.54) and chañares are the larger share at grove edges (0.47 to 0.63).
  - Render: 12 caldén forms (9 live: classic, low multi-stem, leaning, lopsided, tall narrow, broad flat, dense round, high fork, windswept; 3 snags: sparse, dead, stub) and 6 chañar forms, picked by weight from the seed; per-instance yaw and width scale; denser, clumped leaf cards (crown cover 0.58 to 0.67); 3 bark and 3 leaf tints per field species as shared materials; field bark tile 1.2 x 1.8 m (caldén) and 0.8 x 1.6 m (chañar); `Species::bark_set` is the one line to swap when a species gets its own set. Snags are mature trees only (about 7% of caldenes); giants draw a broad live form.
  - Triangles: caldén live 888-1136, sparse 512, dead 312, stub 272; chañar 742-1064. Densest chunk (90 trees) 88,020; the radius-3 scene 348,430 (316,520 before). The 1.2M budget stays.
  - `game/src/camera.rs` (a test builds a `pampa_core::Tree`) needs `age: pampa_core::flora::Age::Mature, width: 1.0` to compile: outside this task's surface, left for the user.
  - Look in game unverified (playtest).

- F1b parent follow-up: `game/src/camera.rs`'s tree test literal gains
  `age` and `width`. Game tests pass (169) and game clippy is clean.
- Bark downloads proposed to the user (2026-10-27). All are CC0 1K JPG,
  and every URL was checked with a 200 response:

  | Species | Bark set |
  |---|---|
  | caldén | Poly Haven `bark_willow` |
  | chañar | Poly Haven `bark_platanus` (peeling) |
  | algarrobo | Poly Haven `chinese_cedar_bark` (fibrous, fissured) |
  | tala | Poly Haven `chinese_hackberry_bark` (same genus, *Celtis*) |
  | espinillo | Poly Haven `bark_willow_02` |
  | ombú | ambientCG `Bark005` (light, smooth) |
  | eucalipto (optional upgrade) | Poly Haven `bark_bluegum` |

- F1 and F1b review: the user played the game on 2026-10-27 and said "lo
  que se ve en el juego está bien". The user granted the native review of
  the staged F1+F1b code (medium, 7 files, 2030 lines, the reliability
  lens). It was approved and acknowledged, lineage
  `review-4cf32dfddc4af951`. Two follow-ups, neither blocking:
  - R3-001 WARNING (`crates/render/src/flora.rs:1852`): the dense-chunk
    budget test requires at least 300 triangles for every kit tree. The
    stub snag form has 272, and nothing at the form level enforces that
    floor. The test passes on its current seed. Fix it in F1c: derive the
    floor from the lightest form, or lower it.
  - R3-002 SUGGESTION (`crates/core/src/flora.rs:347-349`): the clumping
    boost only sees trees already placed in the same chunk, so clumps reset
    at chunk seams. No test checks continuity across borders. Look at it in
    F1c with the monte patches.

- F1c done (2026-10-27, uncommitted): route delegated writer. Look in game unverified (playtest).
  - Core: `FloraSite::monte` (world-space noise, about 20% of the world, patches about 325 m across), `knot` (replaces the per-chunk "company" boost, so clumping runs across chunk seams: R3-002), `Shrub`/`ShrubKind` (piquillín, jarilla, fachinal), `chunk_shrubs`, `shrubs`, `all_shrubs`, `all_plants`. Chunks are 200 m. `TREE_BUDGET` is 160 for the monte; groves keep `GROVE_BUDGET` 90 (they already filled 90). Monte chunks hold about 100 trees (up to 146) against about 1 in open pampa; they are a young stand (60% sapling or young) with about 37% chañar. Shrubs: about 229 per chunk in monte, 133 on fringes, 16 in open (budget 300). Shares: monte piquillín 0.81 / fachinal 0.19; fringe fachinal 0.62; open dry ground jarilla 0.65.
  - Collision: superseded by the F1c follow-up below. Shrubs do not block (user, 2026-10-27). Shrub keep-outs: rail 6 m, road margin, town band, lagoon, fences, settlements, trunks (neighbour chunks included), 1.9 m between shrubs.
  - Render: kit species Piquillin, Jarilla, Fachinal, 6 seeded forms each (412-492 triangles), tinted; barks `Bark009` (piquillín), `bark_willow_02` (jarilla), chañar's `bark_platanus` (fachinal). The F4 espinillo needs another set now. Single tree visibility range. No minimap dots: the field trees are not on the minimap either.
  - R3-001: the dense-chunk test derives its floor from the lightest and heaviest field form. R3-002: seam continuity test (strip correlation across a border 0.48 against 0.46 within a chunk; the old code gave 0.41 against 0.51).
  - Triangles: densest tree chunk 145 trees, 139,772; densest shrub chunk 300 shrubs, 127,476; radius-3 scene 887,754 (795k plants, was 349k) under the 1.2M budget. Frame-time risk for the playtest: 2.3 times the plant triangles.
  - Cost: planting the 24 km world takes 5.4 s against 2.6 s trees only (dev profile, `all_plants`), and 342k obstacles become about 985k. Obstacle queries stay about 0.2 to 0.4 microseconds (grid index). Over the 1.5x / 1 s guideline: options reported to the parent.

F2 (estancia monte and alameda), on `feat/field-flora`.

- F1b bark follow-up (2026-10-27, uncommitted): the user downloaded six Poly Haven CC0 bark sets (1K JPG, converted to KTX2) into `assets/textures/trees/`.
  - `Species::bark_set`: caldén to `bark_willow`, chañar to `bark_platanus`, eucalipto to `bark_bluegum` (was Bark009). Every species now has its own set, so the bark tints only shade the real texture (caldén dull grey-brown, chañar greenish ochre, eucalipto untinted); 3 tint variants per field species kept. Bark tile sizes unchanged (the sets are 1.0-1.8 m wide).
  - `Bark009` is now used by no species (its files are kept). `Bark014` is the paraíso's alone.
  - `chinese_cedar_bark` (algarrobo), `chinese_hackberry_bark` (tala) and `bark_willow_02` (espinillo) are shipped but unused, for F4. Credits updated.
  - `Bark005` (ombú) did not arrive and is still pending for F4.
  - Tests updated: `no_two_species_wear_the_same_bark` now asserts distinct sets; the caldén/chañar bark test pins the new sets.

- F1c follow-up, plant collision streamed per chunk (2026-10-27, uncommitted): route delegated writer.
  - Decision (user, 2026-10-27): "los árboles deben ser obstáculos, los arbustos y pastizales no". Shrubs and tufts are never obstacles. A rider rides through a shrub and is stopped by a tree.
  - Problem: `World::plant` at startup took 2.6 s with trees only and 5.4 to 6.9 s with the F1c shrubs (dev profile, 24 km world).
  - Core: `StaticObstacles` has a streamed layer of footprints per chunk (`set_chunk`, `clear_chunk`, `has_chunk`, `loaded_chunks`, `streamed_len`). Only `near` sees it; the slice view, iteration, `len` and equality read the permanent footprints. `World::plant_chunk` and `unplant_chunk` stream one chunk's trees in and out, the same trees every time (`FloraSite::trees`). `World::plant` stays for tests and tools and now plants trees only.
  - Game: `game/src/plant_collision.rs` (`PlantCollisionPlugin`, runs before `DrivePlayer`). The player's own chunk is planted at once, the other chunks within 1 chunk (Chebyshev) one per frame, nearest first. Chunks past 2 chunks are dropped (1 chunk of hysteresis, so riding along a border does not churn). Radius 1 = every tree within 200 m of the player collides, far past a gallop frame (about 12 m/s), the nearest cattle and townsfolk and the follow camera. Not tied to the render crate's terrain streaming. `lay_out` no longer plants.
  - Consumers (all query `near`, so none needs the whole map): rider and `World::resolve`/`clear_spot` (player, console `goto`), herds (cows in fenced fields, `herd.step`), townsfolk (towns are keep-out of flora), errand, ride, camera, train (the flora keeps off the whole track), fences and settlements (flora keeps off them). Street lamps are placed at startup against the permanent obstacles only: a test shows no lamp stands on a tree. Cows far from the player see no trees (they graze inside fences that the flora keeps off). A console `goto` finds its clear spot before the destination chunk is planted; the rider is pushed out of any trunk the next frame.
  - Tests: `a_planted_chunk_stops_a_rider_at_its_tree_not_its_shrub_and_unplanting_frees_it`, `planting_a_chunk_again_gives_the_same_trees_and_every_chunk_adds_up_to_the_world`, `planting_the_world_makes_no_shrub_an_obstacle_and_a_rider_rides_through_one` (was: shrubs block), `a_streamed_chunk_is_found_by_queries_and_gone_when_cleared`, and four in `plant_collision` (startup plants nothing, chunks come and go and return the same, tree stops a rider and a shrub does not, lamps clear of trees). The road test `a_public_road_leads_from_general_pico_to_the_lagoon_and_rides_clear` now plants the chunks along the road first. Each proven by sabotage.
  - Measured (dev profile): `lay_out` 0.70 s now; before it was about 0.70 s plus 6.9 s of planting with shrubs (3.1 s with trees only). Streaming costs about 0.5 ms per chunk (trees only, worst frame 1.8 ms: own chunk plus one other); a border crossing plants 3 chunks over 3 frames. `near` about 0.26 microseconds.

- F1e done (2026-10-27, uncommitted): route delegated writer. Camera feel in game unverified (playtest).
  - Solid trees: no tree carries a `ThinOccluder` any more (field `Painter::tree`, town plaza and shade trees, estancia poplar and eucalyptus; the unused `crown_reach` is gone). Fences, signposts, lamps, roads and the settlement props keep their fade; houses, barns and cars still stop the camera.
  - Camera (`game/src/camera.rs`): `trunk_reach` pulls the camera in to clear a trunk. The streamed obstacle layer only holds footprints and mixes in signposts and lamps (which must fade), so the camera reads the field's own trees (`TrunkCache`, `FloraSite::trees` per chunk, kept within 2 chunks of the rider, `WorldFlora`). Town and estancia trees are not in it: only field trees pull the camera.
  - Trunk only, not the crown (decision, a product choice to confirm in the playtest): a caldén crown is 4 to 6 m across, and a monte chunk holds about 80 trees, so clearing crowns would keep the camera on the rider's shoulder through every monte and jitter at every crown edge. The leaves are alpha-cut cards the rider shows through. A trunk blocks up to 0.6 of the tree's height (trunk and main limbs).
  - No jitter: unlike a wall, a trunk's pull is continuous. It grows with how close the line passes (0.8 m soft band), fades out above the trunk's top (1.2 m) and within 1.5 to 3 m of the rider. A test sweeps a rider past a row of trunks in 5 cm steps: the reach never changes by more than 0.1 in a step. The existing instant pull-in and 0.35 s ease-out are unchanged.
  - Thinning: `FloraSite::TREE_KEEP` 0.8. Each planted tree stays by a roll of its own spot (`kept`), after the seam logic, so the same trees go whichever chunk asks. Thinned trees still held their ground while planting, so clumps and clearings stay. Shrubs and tufts still keep clear of the unthinned trees, so they are unchanged. `World::plant_chunk`, render and the camera all read the thinned trees. The budgets (`TREE_BUDGET` 160, `GROVE_BUDGET` 90) count planted trees.
  - Counts, whole world (24 km): 80,547 trees planted before, 64,571 kept (0.802). Monte 16,710 to 13,397, groves 1,436 to 1,140, open-pampa lone trees 78 to 62 (all 0.80). Monte chunks now hold about 81 trees on average (up to 117), groves 71.
  - Tests: render (`a_field_tree_never_fades_out_of_the_cameras_way`, town plaza and shade trees solid, `a_settlement_tree_is_solid_and_never_fades`), camera (`a_tree_behind_the_rider_pulls_the_camera_in_to_clear_its_trunk`, `a_trunk_on_the_line_stops_the_camera_short_of_it_and_a_crown_does_not`, `riding_past_a_row_of_trees_never_makes_the_camera_jump`, fence and building tests keep the fade), core (`a_fifth_of_the_trees_are_thinned_evenly_over_monte_groves_and_lone_trees`, `the_same_trees_go_whichever_chunk_asks_and_shrubs_are_not_thinned`). Each proven by sabotage.
- F1e follow-up (2026-10-27, uncommitted): route delegated writer. The user played the build ("todavia me parece mucho, sacale otro 30%, y subile otro 15% a pueblos y aldeas") and then corrected the brief three times; the final state is below.
  - Field trees: `FloraSite::TREE_KEEP` 0.8 to 0.56 (0.8 x 0.7), same per-spot `kept` roll; shrubs and tufts unchanged. Whole world, 2-town `site()` fixture: 80,547 planted, 45,187 kept (0.561). Share test renamed `about_half_the_trees_stay_evenly_over_monte_groves_and_lone_trees`; the monte density test now compares the monte peak with a grove's kept budget (`GROVE_BUDGET` x `TREE_KEEP`).
  - Towns +15% (`town.rs`): street tree `SPACING` (17, 21.2) to (14.8, 18.4), `GROVE_STEP` 10 to 8.8, plaza ring steps x1.08 rounded (`more_steps`; whole steps, a bigger factor overshoots), front trees 55% to 63% of houses, city yards a fifth tree in 40% of blocks, town yards 80% to 92%. Sample towns 3,681 to 4,217 trees (1.146x; rings 340 to 380, street 2,039 to 2,340, front 189 to 223, groves 100 to 128, yards 1,013 to 1,146). Shipped towns 740 to 826 (1.116x). Not grown (fixed layouts): the trees by the church, comisaria and station (2, 1 and 2 per building) and the very big town's 36% grove cut, which scales with its ring. Test `towns_plant_fifteen_percent_more_trees`.
  - Settlements, correction chain: first +15% (reverted), then "estancias sacale tambien", "chacras y estancias sacale del total que tiene ahora, un 30%", then puestos and ranchos too ("se desmonta para huerto, animales, plantaciones"). Final keeps (`Settlement::tree_keep`, applied after `plant_more` by `thin_trees`, evenly spread through the planting order, deterministic, layout otherwise unmoved): estancias 0.49, chacras 0.7, puestos 0.7, ranchos 0.7. `MORE_TREES` stays 0.35. Sample world: estancias 96 to 47, chacras 28 to 19. Shipped world: estancias 173 to 84 (0.486), chacras 25 to 19. Puestos and ranchos plant no trees at all in the sample or the shipped world (their extra count rounds to 0), so their 0.7 is pinned on a compound fixture only and changes nothing visible today. Tests: `estancias_keep_half_and_chacras_seven_tenths_of_their_trees`, `a_compound_keeps_its_kinds_share_of_its_trees_spread_evenly`.
  - Redistribution of the field monte ("cerca de las ciudades hay mucho y a campo traviesa hay poco"): `FloraSite::rural` scales the monte and grove chance by the distance to the nearest town's leveled core (0.12 within 1.5 km, 1 at 3 km, 1.6 at 9 km) and to any settlement hub (0.3 at the hub, 1 at 500 m; all four kinds, as cleared land; the monte's own settlement trees are separate). Past 1 the boost widens the monte and grove masks (`monte_wide`, `grove_wide`, edges lowered by up to 0.07) instead of raising the chance, because the stands were already spacing-limited (a plain chance boost of 1.6 added only 3% trees). Lone trees scale by the square root. Smooth, a function of the spot, so chunk seams are unchanged; peak planted per chunk 145 of 160 (was 146). The old `monte()` and `grove()` masks are unchanged public APIs.
  - Densities (shipped towns, 25 km world, kept trees per chunk): before, 28.4 within 1.5 km of a town, 25.9 at 1.5 to 3 km, 26.3 beyond 3 km (1.08x); after, 5.3, 18.3 and 31.3 (0.17x). World total 190,919 to 193,290 (+1.2%). Settlement ring (120 to 500 m from a hub) against the field at 800 to 1,500 m, sample world: 0.244 against 0.367 trees per sample point (0.66x). The 2-town `site()` fixture is a 12 km world where 2 towns cover about 60% of the ground within 3 km, so it cannot keep its 45,187 total (29,878 at 1.4x boost, 32,100 with a short ramp); the +-5% total is pinned on the shipped towns instead. `site()` now turns the redistribution off so the stand-shape tests keep measuring the stands themselves; `redistributed()` turns it on. Tests: `the_monte_keeps_back_from_the_towns_and_the_world_total_holds`, `the_land_round_a_settlement_is_thinner_than_the_open_field_further_out`, `the_rural_share_is_smooth_across_chunk_borders_and_follows_the_distance`, `redistributed_trees_keep_their_spacing_across_chunk_borders_and_are_the_same_whichever_chunk_asks`.
  - Proof: each new or changed test saw RED on the old values (sabotage restores the old constant) and was proven by breaking the code and restoring it (rural always 1, stepped distance, boost 1.0, worked share 1.0, near share 0.6, spacing check removed, ranch 0.7, chacra 1.0, puesto 1.0, rancho 1.0, only-ranch worked hubs, towns back to the old spacing).
  - Playtest to confirm: the far monte is denser in a ring 3 to 9 km out; trees near towns are few (the street and plaza trees are the town's own).
- F1e final tuning (2026-10-27, uncommitted): route delegated writer. User, after a playtest: "redujimos mucho, dame un 10% más de árboles grandes y zonas boscosas y cerramos acá". All in `crates/core/src/flora.rs`.
  - Constants: `TREE_KEEP` 0.56 to 0.616; `AGE_SHARES` (open ground) [0.10, 0.30, 0.975] to [0.07, 0.21, 0.965]; `MONTE_AGE_SHARES` [0.30, 0.60, 0.985] to [0.22, 0.50, 0.97]; monte mask `smoothstep(0.6, 0.7)` to `(0.585, 0.685)`. Budgets unchanged (`TREE_BUDGET` 160, `GROVE_BUDGET` 90); the redistribution (`rural`) is untouched.
  - Counts, 2-town `site()` world: 80,547 planted, 45,187 kept (0.561) to 83,394 planted, 51,499 kept (0.617). Open-ground ages (caldén): 0.103 sapling, 0.211 young, 0.668 mature, 0.018 old to 0.069, 0.149, 0.754, 0.028. Monte heart: 0.60 sapling or young, 0.014 giants to 0.50, 0.030. Monte coverage 0.199 to 0.221 of the world (mean crossing 325 to 336 m, widest 1040 to 1660 m). Giants stay under 4%.
  - Shipped towns, 25 km world (new baseline): 193,290 to 218,995 trees (+13.3%: 10% from the keep share, the rest from the wider monte); near a town (under 1.5 km) 5.3 to 6.0 a chunk, far (over 3 km) 31.3 to 35.4 (near/far 0.17x kept). Peak planted per chunk 145 to 144 (budget 160); densest kept monte chunk 84 to 93.
  - Render: densest chunk 96 trees, 90,920 triangles; radius-3 scene 776,240 drawn triangles (92,800 ground, 683,440 plants) against the 1.2M budget.
  - Tests: monte share pinned to 0.21..0.25, kept share to 0.616 (slack from 0.616 x 0.384), age bands (sapling 0.04-0.09, young 0.10-0.20, mature 0.70-0.85, old 0.025-0.04), monte heart (young over 0.45, mature over 0.42, giants over 0.02), shipped-towns total pinned to 218,995 +-5%. Each saw RED on the old values and was proven by sabotage (old `TREE_KEEP`, old age shares, old monte shares, old monte mask each fail exactly the tests that pin them).
  - Closes the F1e tuning ("cerramos acá"). Look in game unverified (playtest).

Next step: commit the F1c–F1e work, run the native review, then F1d (leaf litter), F2, F3 and F4 in a later session.
