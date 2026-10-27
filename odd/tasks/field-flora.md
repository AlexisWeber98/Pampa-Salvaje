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
- [ ] F1c Monte and brush:
  - Denser caldenal patches (monte and bosque) with an understory.
  - Shrubs and thickets: piquillín, jarilla, chañar thickets (fachinal).
  - Add the shrubs to `odd/setting-1919.md`.
  - New textures can be downloaded by the user if needed (CC0, named
    exactly).
  - Route: delegated writer.
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

## Next step

F2 (estancia monte and alameda), on `feat/field-flora`.

- F1b bark follow-up (2026-10-27, uncommitted): the user downloaded six Poly Haven CC0 bark sets (1K JPG, converted to KTX2) into `assets/textures/trees/`.
  - `Species::bark_set`: caldén to `bark_willow`, chañar to `bark_platanus`, eucalipto to `bark_bluegum` (was Bark009). Every species now has its own set, so the bark tints only shade the real texture (caldén dull grey-brown, chañar greenish ochre, eucalipto untinted); 3 tint variants per field species kept. Bark tile sizes unchanged (the sets are 1.0-1.8 m wide).
  - `Bark009` is now used by no species (its files are kept). `Bark014` is the paraíso's alone.
  - `chinese_cedar_bark` (algarrobo), `chinese_hackberry_bark` (tala) and `bark_willow_02` (espinillo) are shipped but unused, for F4. Credits updated.
  - `Bark005` (ombú) did not arrive and is still pending for F4.
  - Tests updated: `no_two_species_wear_the_same_bark` now asserts distinct sets; the caldén/chañar bark test pins the new sets.
