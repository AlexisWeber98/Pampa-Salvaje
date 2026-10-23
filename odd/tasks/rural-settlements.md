# Feature: rural settlements (PR-C3)

Branch: `feat/rural-settlements` (from `main` at e9463a2).

## Objective

Fill the fields behind some field gates with 1919 rural settlements:

- ranchos;
- puestos;
- chacras;
- estancia cascos, each with worker houses, a galpón, a corral, and a molino with its aguada.

Dirt huellas run from the gate to the houses. The look should be as realistic as the hardware allows: PBR textures, and free third-party glTF models where a good one exists.

## Problem and why

Today the fields between the towns are empty grass. The pampa of 1919 was settled land, and the settlements give the countryside life, landmarks and future errand targets.

## Decisions (user, 2026-10-22)

**Scale and content**
- Some chacras and estancias span several hectares. They have the main house plus separate houses for caretakers and peones, spread out and joined by huellas. Fields are 300–800 m survey cells (9–64 ha).
- Inner tracks are **huellas**: about 2.5–3 m wide, much narrower than the 6.5 m roads. They are two wheel ruts with grass between, worn by workers, carts and horses. They have no fences, no signposts and no terrain cut, and they live outside `RoadNetwork`.
- Settlements sit behind **open** field gates only, and only behind some of them.

**Realism and assets**
- Realism is the priority, within what an integrated GPU (Renoir) can run.
- **glTF is enabled** (`bevy_gltf`) so third-party models keep their PBR textures. The crates are fetched once from the network, with permission.
- No free model fits the Pampa buildings: the adobe rancho, the Italianate casco, the post-and-rail corral and the corrugated "australian" tank. These are built procedurally with CC0 PBR textures from ambientCG and Poly Haven.
- Third-party CC-BY models from Sketchfab need a login, so **the user downloads them**, later. Login was blocked on 2026-10-22, so for now everything is procedural, with glTF model slots ready (see T4). CC0 textures and props from ambientCG and Poly Haven are downloaded **once** by the agent, with permission.
- Every asset is credited in its folder's CREDITS.md and in the root CREDITS.md.

## Constraints

- Follow `odd/setting-1919.md`; this feature extends its building list with the rancho, puesto, casco and worker houses.
- Use deterministic seeded layout, and test that the layout is rebuilt identically.
- Settlements stream per terrain chunk, like fences and roads.
- Use a few shared `VisibilityRange` constants only. Every distinct range permanently takes a GPU slot, and past 65,535 Bevy culls unranged meshes.
- Settlements must not slow `Terrain::height`: no linear scan of a `FlatArea` per settlement.
- Triangle budgets (estimates): props 0.5–5k, trees 3–10k, windpump 8–15k, buildings 5–20k. Anything over ~50k faces must be decimated.

## Tasks

- [x] **T0 — glTF pipeline.** Enable `bevy_gltf` (plus the features it needs) and fetch the crates once. Add a small loader path for `assets/models/**.glb`, and a test that loads one shipped glTF.
  - Progress: route delegated writer. Features `bevy_gltf` (pulls `bevy_world_serialization`), `png`, `jpeg`, `reflect_auto_register` (scene spawning copies components by reflection); 21 new crates locked. `crates/render/src/gltf.rs` (`load_scene`, `model_state`, `scene_at`, `asset_plugin`) plus `assets/models/test/triangle.gltf`; render 168, game 83 tests pass, clippy and fmt clean. Pending for T3/T4: `game/src/main.rs` must set `DefaultPlugins.set(gltf::asset_plugin(&root))`, or the asset server reads `game/assets`.
- [x] **T1 — CC0 assets.** Download once from ambientCG and Poly Haven:
  - PBR textures for adobe/plaster, brick, corrugated iron, thatch, weathered wood and track dirt;
  - props: barrels, bucket, ladder, lantern and crate.

  Use 1k textures for props and up to 2k for buildings. Add credits.
  - Progress: route inline (parent download).
    - Licences verified: polyhaven.com/license says all assets are CC0; ambientCG was already verified.
    - `assets/textures/buildings/` holds 9 sets, each with `color`/`normal_gl`/`roughness`(/`ao`).jpg at 1K: clay_plaster, plaster_003, brick_wall_02, corrugated_iron(_02), brown_planks_05, brown_mud_dry, dry_ground_01, plus our own `thatch/` generated with ImageMagick.
    - `assets/models/props/polyhaven/` holds 5 glTF props. Barrels are ~33k triangles, so use them sparingly.
    - Credits are next to the files and in the root CREDITS.md. About 37 MB added.
    - No Poly Haven or ambientCG thatch exists.
- [x] **T2 — Core layout** (`crates/core/src/settlement.rs`):
  - kinds (rancho, puesto, chacra, estancia);
  - compounds with worker houses;
  - huellas from an open gate, staying clear of roads, fences and cell edges;
  - obstacles and a flora keep-out;
  - `FenceLayout::field_of`.

  Add tests for determinism, clearance and reachability.
  - Progress: route delegated writer (one new module plus small edits to fence, flora, world and lib).
    - `SettlementLayout::build(terrain, towns, line, fences)`, seeded per gate. Settlements sit only behind open roadside gates, one per survey cell, with hubs at least 300 m apart. Each is clear of roads (25 m), the whole track (40 m), town bands (60 m) and grid fences (12 m).
    - Each settlement has typed structures (`StructureKind`, with model `slot()` names), huellas (a trunk from the gate to the hub, then one branch to each door), corrals as rail spans with a 4 m opening, and `World::settle`.
    - `FloraSite::with_settlements` and `off_settlements` use an indexed `KeepOut` (32 m grid). `FenceLayout::field_of` is added.
    - Real map (fence fixture, half extent 8000): 15 settlements behind 56 open gates (4 ranchos, 4 puestos, 6 chacras, 1 estancia), 158 obstacles, laid in about 3 ms.
    - RED observed with an empty-layout stub, then GREEN: 10 settlement tests. Core 291, render 168, game 83; fmt and clippy clean.
    - For T3: render props (stones and shrubs) should call `FloraSite::off_settlements`, and `game/src/main.rs` must build the layout and call `world.settle` and `with_settlements`.
- [x] **T3 — Procedural buildings** (render):
  - the rancho (adobe with a thatch or corrugated roof), puesto and chacra (brick with a parapet);
  - the casco with its gallery, and the worker houses;
  - the galpón, the corral, the australian tank and the trough;
  - the huella mesh with ruts.

  Use PBR materials, per-chunk streaming and camera occluders.
  - Progress: route delegated writer (multi-file: new render module plus wiring in core, terrain, flora, camera and main). About 3,700 authored lines, well over the ~400 heuristic: the module needs a mesh builder, materials, 4 house styles, 9 prop kinds and the track decals to be usable at all; split into 3 commits for review (below).
    - English identifiers (user, 2026-10-22): `Huella`→`Track` (`huella.rs`→`track.rs`), `Rancho/Puesto/Chacra/Estancia`→`Hut/Outpost/Farm/Ranch`, `CascoGallery`→`GalleryHouse`, casco→homestead, `Galpon`→`Barn`, `Molino`→`Windpump`, `Carreta`→`Cart`, aguada→`water_point`, alameda→`poplar_row`. The Spanish words stay as glosses in doc comments.
    - Wiring: `game/src/main.rs` sets the asset server to the workspace `assets/` (`gltf::asset_plugin`), lays roads → fences → settlements (`world.settle`) → flora `.with_settlements`, logs counts per kind and build time, and inserts `WorldSettlements`. `lay_out` is a function now, with a game test. Terrain stones and shrubs keep 2 m off settlements (`off_settlements`).
    - `crates/render/src/settlement.rs` + `settlement/{shape,materials,buildings,props,track,tests}.rs`: `SettlementPlugin` lays each settlement on the chunk owning its hub (PostUpdate after `ChunkStreaming`), children despawn with the chunk.
    - Materials: 13 surfaces, one shared `StandardMaterial` each (9 texture sets + recess, steel, sheet, water), plus 4 tree paints. Textures load through the `AssetServer` with a repeating anisotropic sampler; `finish_textures` repacks `roughness.jpg` into green (blue = full metallic, scaled per surface by `metallic`) and builds CPU mips once, in the frame the image arrives. Without an asset server or files: plain colours.
    - World-scaled UVs (2–3 m per repeat) and tangents on every face.
    - Triangles (largest sizes): homestead 2,552, brick house 1,122, adobe huts ~400, barn 202, corral 2,892, windpump 2,214 (tower + wheel), cart 1,196, tank 600, well 408, haystack 240, trough 150, hitching post 36. Two settlements of every kind: ~23.9k triangles in 103 draws.
    - Ranges: 3 shared `VisibilityRange`s (buildings 400 m, props 220 m, trees = the wild trees' range), tested ≤ 3.
    - Trees: eucalyptus and poplar from the vegetation models (stand-ins without them), slender, recoloured; core `Structure::obstacles` now gives every tree spot a trunk box (`trunk_radius`, clamped).
    - Camera: `CameraOccluders::with_settlements` adds every house and barn; corral sides, windpumps (and wheels), wells, carts, hitching posts, haystacks and trees are `ThinOccluder`s. The occluder list grows by ~100 boxes, so no distance filter was needed.
    - Tests: RED observed for tree trunk obstacles (core) and for stones on settlements (terrain), then GREEN. Render settlement tests (12) were written alongside the code: chunk streaming once/despawn, budgets, ranges, shared materials, tracks drape with ruts, trees slender on their spots, wall openings, UVs, lathe, mips/repack. Core 292, render 181, game 84 pass; fmt and clippy clean.
    - Deferred: visual check on the Renoir GPU (a manual ride); whether a modified image rebinds already-prepared materials (textures are finished in the frame they load, before the first extract, so it should not matter); mips are box-filtered in gamma space; a settlement's tracks are drawn only while its hub's chunk is loaded; glTF model slots are T4.
- [x] **T4 — Procedural props with model slots.** Sketchfab login is blocked for now (user, 2026-10-22), so every object is built procedurally:
  - the windpump, reusing and improving the town windmill;
  - eucalyptus and poplar around the cascos;
  - the well, trough, carreta, haystack and hitching post.

  Each object kind has a model slot at `assets/models/third-party/<slot>/model.glb` (or `.gltf`). When the file exists, it replaces the procedural mesh at load time, with a per-slot scale, offset and yaw from RON. When it is missing, the procedural fallback is drawn, so a model downloaded later drops in without code changes. Document the slot names and the credit steps in `assets/models/third-party/README.md`.
  - T0 review follow-ups, to do here:
    - Test that a malformed glTF reaches `ModelState::Failed` and keeps the stand-in.
    - Make the test settle helpers panic with an explicit timeout message.
    - Reject absolute or `..` slot paths in `load_scene`.
  - 3TD Starter Pack #1 (CC0, Ron Kapaun, OpenGameArt) was downloaded on 2026-10-22 under a second one-time permission, which is now revoked. It brings an Aermotor-style windmill on a lattice tower, an old wagon and barbed-wire fence segments, as COLLADA sources in `assets/models/third-party/3td-starter-pack/source/`.
    - Convert them to glTF for the `windpump` (and maybe `carreta`) slots with our own stdlib-only Python script (`tools/dae2gltf.py`). Run it with `python -I`, with no new downloads.
    - Spin the wheel in code. Note the conversion in its CREDITS.md.
  - T2 review follow-ups, to do here or in T3:
    - Settlement tree spots (eucalyptus, poplar) have no collision yet. Their trunks must block once render or flora plants them. Done in T3.
    - Add direct unit tests of `FenceLayout::field_of` on a survey line, outside the grid, and at the outermost cell.
  - T3 review follow-ups, to do here:
    - The core test of `build_with_ranches` doesn't prove that a forced ranch was placed. Assert an in-range town is placed and that the extra ranch comes from the forced pass. The game test already checks both shipped towns.
    - The forced-ranch test asserts reach ≤ `RANCH_REACH[2]`, but the last step is unbounded. Assert the step that actually succeeded.
    - Make the exact f32 trunk-radius assert approximate.
    - Slot names are English now (`cart`, not `carreta`). T4 creates the slot folders with these names, so no asset lookup breaks today. Add a test that every slot name resolves.
    - The game test checks wild trees only on the hub's chunk. Iterate over every chunk the settlement's bounds cover, as the terrain props test does.
  - T1 review follow-ups, to do here:
    - Add a smoke test that loads every shipped prop glTF to `Ready`.
    - The lantern glass is `alphaMode BLEND` with a JPEG base colour (no alpha), so it renders opaque. Override the material, or check how it looks.
    - Commit the thatch generation command and seed as `assets/textures/buildings/thatch/generate.sh`.
  - Progress: route delegated writer (multi-file: converter, render slots and yard props, core wrecks, content RON, game wiring, docs). About 2,300 authored Rust/doc lines plus the 960-line converter and generated glTF, well over the ~400 heuristic: the slot pipeline, the converter, the props and the wreck request each need their own code and tests to be usable; split into commits below.
    - Model slots (`crates/render/src/settlement/slots.rs`): at startup every `StructureKind::SLOTS` folder with `model.glb`/`model.gltf` is loaded. While it loads, the procedural mesh stands in (`StandIn`) and the scene waits (`SlotPending`). On `Ready` the scene spawns, and on `WorldInstanceReady` its meshes get the kind's shared range and fade, its rotor node gets `ModelRotor` and the stand-ins go. On `Failed` the stand-in stays for good. Obstacles and occluders are unchanged, and no new ranges are added. Fits come from `assets/models/third-party/slots.ron` (scale, offset, yaw in degrees, rotor node and axis), parsed and validated in `pampa-content` (`load_model_slots`), and the game converts them (`model_slots`). Tree slots scale per meter of height. Documented in `assets/models/third-party/README.md`.
    - `tools/dae2gltf.py` (stdlib only, `python3 -I`): triangles/polylist/polygons, baked node transforms (negative scale flips winding), Z-up→Y-up, `<unit>`, diffuse→baseColor, `_NRM`→normal (a curl test found all 3TD maps DirectX, so green is flipped), SPEC ignored (roughness from shininess), `--rotor` splits a node off at its pivot with its animated axis, and `--check` validates the output. Hostile input is refused: DTD/entities, sizes, indices, non-finite numbers, and texture paths reduced to a base name in the model's folder.
    - Converted: windmill → `windpump` slot, 1,920 triangles (body 852 + `rotor` 1,068, axis -Z, hub at 11.4 m; fitted at scale 0.875 so the hub is at ~10 m). OldWagon → 3,046 triangles. It is a derelict four-wheeled wreck with a wheel off, so it is **not** the cart (the procedural carreta stays). By the user's idea it is the new `wrecked_wagon` slot (`StructureKind::WreckedWagon`): one at some chacras (30%) and estancias (50%), placed last so it keeps clear of every track, door and patio; and `SettlementLayout::roadside` (`settlement::RoadsideWreck`), at most 5 map-wide, 1.5 km apart, along the road on its verge (corners between the road's edge and the fence, any road angle), clear of gates, openings, signposts, rails, towns and settlements, with 3 small obstacle boxes and a flora keep-out. Shipped world: 3 in yards, 5 on verges. Without its model, the carreta's mesh stands in, sunk and tilted.
    - Poly Haven props (`settlement/yard.rs`), seeded from the settlement: a lantern hung by each barn's big door, a stepladder by a side wall and crates (some stacked) at a back corner of most barns, the barrels (~33k triangles) only behind an estancia's first barn, and a bucket by the well or the trough. Standing props that would land on a track, a structure or the patio are dropped. They are scenery with no obstacles, and use the props' range. Lantern glass: `BLEND` over an opaque JPEG multiplies to alpha 1, so its base colour alpha is set to 0.3 and it is made smooth when the scene spawns (the glTF's own shared material, an idempotent change).
    - Follow-ups closed: malformed glTF → `Failed` → stand-in kept (a gltf test and a slot test, plus a late-failure test); settle helpers panic with a timeout message; `load_scene` rejects absolute, `.`, `..` and backslash paths (`gltf::is_inside`); every shipped prop/slot glTF loads to `Ready` with its textures (the test app now registers an image loader and the PBR glTF material bridge); `FenceLayout::field_of` unit test (survey line, outside, outermost cell); forced-ranch test asserts the forced estancias lead the layout, come from the forced pass and were found in a bounded reach step; approximate trunk-radius assert; every slot name is a plain folder name, and every folder is a slot with a model (core and render tests); the game test checks wild trees on every chunk a settlement's bounds cover; trees: no trunk → no crown, ≥1 trunk per species, and the fade radius now covers the crown's real reach (RED: eucalyptus faded within 5.2 m of a 7.0 m crown; the radius is now each species' widest variant, measured once); thatch `generate.sh`.
    - `RENDER_WORLD`-only meshes: no change needed. In Bevy 0.20, extraction keeps the `Assets<Mesh>` entry and `Mesh::take_gpu_data` stores `final_aabb` before taking the attributes. `MeshAabb::get_aabb` returns it, so `calculate_bounds` still gives late-spawned entities an `Aabb` (bevy_mesh `mesh.rs` `take_gpu_data`, bevy_camera `primitives.rs`, bevy_render `render_asset.rs`).
    - Thatch: `generate.sh` holds the exact commands and seeds (1919, 7) from T1, taken from the session that made the files. It reproduces `color.jpg` and `roughness.jpg` byte for byte (checked with `cmp`).
      - Its review (4 lenses, approved) asked to prove the normal map's convention. A ramp test showed the T1 red channel was inverted: a surface rising to the right gave R 0.516 instead of below 0.5. The script now uses Sobel:180 for red (R 0.484 on that ramp; green was already right), and the shipped `normal_gl.jpg` was regenerated.
      - It also requires an HDRI ImageMagick, keeps the height map in a `mktemp` file removed by a `trap`, and refuses to overwrite an existing set unless `FORCE=1`.
    - Tests: RED observed for the crown fade radius, for slots disabled (no well scene), and for the late-failure stand-in; the other new tests were written alongside the code. Core 296, render 193, content 28, game 87 pass; fmt and clippy clean.
    - Deferred: a visual check on the Renoir GPU (wreck tilt and fit, lantern glass, windpump fit and spin direction, the prop placement); `pampa_core` does not re-export `RoadsideWreck` (`crates/core/src/lib.rs` was outside the allowed surfaces); the windpump's per-instance wind yaw (the model faces its structure's front); the minimap does not show wrecks.
- [ ] **T5 — Minimap.** Add a field frame mode that shows settlements and huellas when no town is near.
- [ ] **T6 — Docs.** Update the setting guide entries, README and this document.

## Acceptance criteria

- Settlements appear behind some open gates. A huella leads from the gate to the houses, and the rider can ride in and around them without clipping.
- No trees, tufts or stones grow on buildings or huellas.
- Frame budget holds on the Renoir GPU, and the triangle and range budgets are covered by tests.
- Every third-party asset is credited, with a licence verified on its source page.

## Checks

- `cargo test -p pampa-core`, `-p pampa-render`, `-p game`, `-p pampa-content`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets`
- A manual ride through the countryside.

## Delivery

- Forecast: well over 400 authored lines, so it is split into work-unit commits that the user commits and that get native review one commit at a time.
- Route per task: delegated writer for T0 and T2–T5, which are multi-file. T1 is a parent download step plus credits.

## Pending, outside this feature

- Verify the licence actually shipped with the Quaternius vegetation pack. quaternius.com now shows the "Quaternius Asset License v1.0", which forbids redistribution, while the old itch.io page said CC0.

## Progress

- 2026-10-22: code mapped, free-model survey done, decisions recorded. Branch created.
- 2026-10-22: T3 done (delegated writer): settlements drawn, solid and seen; English identifiers. Next: T4.
- 2026-10-22: forced estancias (user): `SettlementLayout::build_with_ranches` lays one estancia near each town named in `game/src/main.rs` `RANCH_TOWNS` (Eduardo Castex, General Pico) before the seeded mix. Open gates are tried nearest first within 3 km of the town's leveled band, 64 hub tries each; the reach widens to 6 km, 12 km, then anywhere; a town still without one is in `unplaced_ranches` and logged as a warning. RED on the shipped world (no estancia within 3 km of Castex), then GREEN. Shipped world now: 5 ranchos, 3 puestos, 6 chacras, 2 estancias (16). Castex's: hub (-4598, -2936), 561 m ESE of the town centre, gate (-4629, -2790) 331 m from its band, track 149 m south. Pico's: hub (2460, 1863), 462 m W of the town centre, gate (2450, 1668) 202 m from its band, track 197 m north. Core 293, render 181, game 85; fmt and clippy clean.
- 2026-10-22: tree crowns (playtest: the poplar crowns looked flat). Squeezed to 0.22 of their height, the Quaternius CommonTree crowns came out about 0.08–0.25 h wide and 0.43–0.57 h tall: a few big flat facets, a slab. At 0.5 the eucalyptus was not a slab (its plan ratio was 0.57–0.88) but was stretched up to 3:1. Both now keep the model trunk (or its stand-in) under a procedural crown (`crates/render/src/settlement/crown.rs`): lumpy, flat-shaded, once-subdivided icosphere leaf masses in the models' leaf colours, tinted by the species paint.
  - Poplar: a column of 7 masses, about 0.2 h wide, 560 triangles.
  - Eucalyptus: 6 clumps round the top, about 0.5 h wide, 480 triangles.
  - 4 seeded variants per species, built once and shared; the range, fade and occluder are as before.
  - RED: an app test that crowns have depth on both ground axes, the poplar's is a column, there are 300–800 triangles, and 2–4 distinct shared crowns per species. It failed on the squeezed crown (7.3 × 6.0 × 7.2 m: not columnar), then passed. Render 183, game 85; fmt and clippy clean.
- 2026-10-22: T4 done (delegated writer): model slots live (the windpump from the 3TD windmill, its wheel spinning), the 3TD wagon as abandoned wrecks in yards and on road verges (user idea), the Poly Haven yard props, `tools/dae2gltf.py`, and the T0–T3 review follow-ups. Next: T5.
