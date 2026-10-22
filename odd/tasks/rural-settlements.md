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
- [ ] **T1 — CC0 assets.** Download once from ambientCG and Poly Haven:
  - PBR textures for adobe/plaster, brick, corrugated iron, thatch, weathered wood and track dirt;
  - props: barrels, bucket, ladder, lantern and crate.

  Use 1k textures for props and up to 2k for buildings. Add credits.
- [ ] **T2 — Core layout** (`crates/core/src/settlement.rs`):
  - kinds (rancho, puesto, chacra, estancia);
  - compounds with worker houses;
  - huellas from an open gate, staying clear of roads, fences and cell edges;
  - obstacles and a flora keep-out;
  - `FenceLayout::field_of`.

  Add tests for determinism, clearance and reachability.
- [ ] **T3 — Procedural buildings** (render):
  - the rancho (adobe with a thatch or corrugated roof), puesto and chacra (brick with a parapet);
  - the casco with its gallery, and the worker houses;
  - the galpón, the corral, the australian tank and the trough;
  - the huella mesh with ruts.

  Use PBR materials, per-chunk streaming and camera occluders.
- [ ] **T4 — Procedural props with model slots.** Sketchfab login is blocked for now (user, 2026-10-22), so every object is built procedurally:
  - the windpump, reusing and improving the town windmill;
  - eucalyptus and poplar around the cascos;
  - the well, trough, carreta, haystack and hitching post.

  Each object kind has a model slot at `assets/models/third-party/<slot>/model.glb` (or `.gltf`). When the file exists, it replaces the procedural mesh at load time, with a per-slot scale, offset and yaw from RON. When it is missing, the procedural fallback is drawn, so a model downloaded later drops in without code changes. Document the slot names and the credit steps in `assets/models/third-party/README.md`.
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
