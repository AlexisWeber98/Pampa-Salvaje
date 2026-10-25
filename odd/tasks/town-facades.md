# Town facades: textured, bevelled 1919 buildings

## Objective (user, 2026-10-25)

The towns still look procedural and basic. Replace the plain-colour box
buildings with 1919 Italianate pampa architecture. This is phase 1 of 6.

## Findings (read-only audit, 2026-10-25)

- `crates/render/src/town.rs`:
  - `Palette::new` (`:106-150`) is flat-colour materials only, with no
    textures;
  - every box is its own entity (`Builder::spawn/cube :178-195`);
  - the gable mesh has no UVs or tangents (`:818-836`);
  - buildings are built in `spawn_building :601-731`;
  - `false_front :525-555`, `openings :557+`;
  - there is no `VisibilityRange` on towns.
- The model to copy is `crates/render/src/settlement/`:
  - `shape.rs` is a builder with world-scaled UVs and tangents;
  - `materials.rs` holds 13 shared KTX2 PBR `Surface` materials;
  - `buildings.rs` already builds Italianate fronts (parapet, cornice,
    coping, pilasters, galerías on iron posts, doors, shutters), with one
    mesh per Surface per structure;
  - `settlement.rs:119-131` defines the shared ranges.
- No free 3D building fits 1919 Argentina. Phase 1 needs no downloads: it
  uses the 9 existing KTX2 sets.

## Phases (each ≤ about 3.5k changed lines)

1. Textured, bevelled town buildings (this document).
2. New Poly Haven plaster, brick and paving textures (needs a download).
3. The plaza: pirámide, paths with curbs, benches, faroles, railing.
4. Better trees: LeafSet cards, paraíso, olmo, eucalipto, aromo.
5. The station: FCO brick, platform canopy on cast-iron columns.
6. Streets: textured dirt, sidewalks, curbs, palenques.

## Tasks (phase 1)

- [x] F1 Shared facade kit `crates/render/src/facade/`. Settlements are
  unchanged, byte for byte (see Verification).
  - `shape.rs` and `materials.rs` moved from `settlement/` (`git mv`).
    `Shape` gained `chamfered` (bevelled vertical edges), `merge` (a shape
    moved rigidly into another) and a test-only `indices`.
  - `SettlementMaterials` was renamed `FacadeMaterials`. Two tints over
    `plaster_003` were added: `Lime` (ochre wash) and `Stucco` (grey
    plinths). The asset server loads a shared set once.
  - `walls.rs` holds `Opening`, `Wall`, `Walls`, `spread`, `slab`,
    `gable_roof`, `wedge`, `lean_to`, `ring`, `parapet_roof`, `flat_roof`,
    `chimney` and `barn`, moved from `settlement/buildings.rs`.
    `buildings.rs` keeps the settlement houses and calls the kit.
  - `italianate.rs` holds the dressing: `plinth` (broken at doors),
    chamfered `pilaster` and `corner_piers`, the moulded `cornice`,
    `parapet` (solid or balustraded, pedestals, all round or front only),
    `baluster`, `urn`, `pediment`, `surround` (with hood), `door_leaves`
    (with transom), `grille` (reja), `arch` (round head with archivolt),
    `gallery` (galería, on iron or timber posts) and `spire`.
  - `FacadePlugin` builds the materials once. Both `SettlementPlugin` and
    `TownPlugin` add it when it is missing (`facade::ensure_plugin`).
- [x] F2 Town buildings on the kit: `facade/town_buildings.rs`. `town.rs`
  only streams and spawns.
  - Houses: whitewash, lime or bare brick, with:
    - a Stucco plinth;
    - chamfered corner piers and pilasters between 2–5 bays;
    - the zaguán door at one end, with a transom;
    - framed windows with shutters, some with hoods or rejas;
    - a cornice;
    - a parapet, solid or balustraded, with urns or a pediment on some;
    - a flat azotea (3 in 5) or iron behind the front parapet.
  - Outskirts houses (within 50 m of the grid edge): 13 in 20 are plain
    adobe or whitewashed cottages under a lean-to or gable of old iron.
  - General store (almacén): brick or lime, about 5 bays of doors and shop
    windows, a solid 2.3 m parapet behind the sign, a pediment and urns,
    and a galvanised galería on cast-iron posts over the veranda depth.
  - Pulpería: whitewash or lime, rejas, and an old-iron galería on timber
    posts.
  - Comisaría: whitewash, rejas, a 2.5 m door under the sign, a
    balustrade, a pediment, urns, and a steel flagpole with a finial on
    the azotea.
  - Church: a nave set back 0.7 m behind its tower, with:
    - arched, unshuttered windows and buttress pilasters;
    - a pedimented front gable and an old-iron gable roof along the nave;
    - a tower with an arched door, a cornice and a belfry of four round
      arches, then a drum, a zinc (Sheet) spire and an iron cross.
  - Station (light, until phase 5): brick with stucco dressings, hooded
    openings both sides, a galvanised gable roof and chimneys. Its
    platform canopy follows core's `Canopy` exactly (DEPTH, HEIGHT,
    THICKNESS), on iron posts.
  - Shed and grain store: the kit's `barn` (old or galvanised iron, 1–4
    door pairs).
  - Water tank and windmill: the settlements' `props::tank`,
    `props::windpump` and `props::rotor`. The windmill wheel still spins
    as a `WindmillRotor`.
  - The platform and its ramps stay concrete boxes.
  - One entity per building and Surface (`TownBuilding(index)`), all
    using the shared `FacadeMaterials`.
- [x] F3 Ranges: buildings, the platform and the ramps use `ranges()[0]`
  (BUILDING_FAR). Windmills, tanks, the hitching rail and the plaza
  monument use `ranges()[1]` (PROP_FAR), and plaza trees `ranges()[2]`.
  Streets, plaza lawns, paths and the town railway have no range. No new
  VisibilityRange.
- [x] F4 Tests with sabotages, previews and docs (this file, README, module
  docs).

Route: one delegated writer, in an isolated worktree on
`feat/town-facades` (trigger: 2+ non-trivial files). Uncommitted; the user
commits.

## Decisions

- **Inside the footprint.** Masonry walls stand `PROUD` = 0.1 m in from
  the footprint, and every pilaster, cornice, sill, shutter and coping
  stays within it. So colliders (the footprints) and facade signs, which
  hang 0.1 m outside, stay true. Only the store's galería reaches over the
  sidewalk: below the cornice, and short of the hitching rail.
- **Signs unchanged.** The store parapet is a solid 2.3 m (the sign sits
  0.2–2.0 m over the wall). The police door is 2.5 m (the sign starts at
  2.75 m). The church door arch tops out at 3.4 m (the sign starts at
  3.6 m).
- **Streaming.** The first fill builds its buildings synchronously, so the
  first frame shows them. Later activations build on the
  `AsyncComputeTaskPool` (`PendingBuildings` on the root) and land a few
  frames later. Activation is 600 m out, past BUILDING_FAR. A root
  despawned first cancels its task. A `goto` teleport shows a town's
  buildings about 30 frames after arrival.
- **Draw calls.** No per-block merge. Bevy 0.20 bins opaque items by
  pipeline + material bind group + mesh slabs (`Opaque3dBatchSetKey`),
  not by mesh, and draws each batch set with one
  `multi_draw_indexed_indirect(_count)` call on Vulkan. Every building
  mesh shares one vertex layout and the default slabs grow to 512 MiB, so
  the town's buildings are a handful of draws. This is an estimate from
  the source; no GPU capture was taken.
- **Settlement reuse.** Town windmills and tanks are built from
  `pampa_core::Structure` values, so they match the settlements' molino
  and tank exactly.
- **Core untouched.** The store style comes from `StoreStyle::of(size)`,
  outskirts from the grid bounds, and variants from a hash of index and
  footprint.
- **English identifiers.** Spanish terms appear only as glosses in docs
  (coordinator, 2026-10-25): `barn`, `gallery`, `grille`, `cottage`,
  `flat_roof`, `town_buildings`.

## Perf (headless, dev profile, `scratchpad/townperf`)

| Town | Entities | Mesh entities | Distinct meshes | Mesh+material pairs | Triangles | Ranged |
|---|---|---|---|---|---|---|
| General Pico before | 3428 | 3424 | 5 | 19 | 44,370 | 0 |
| General Pico after | 3378 | 3374 | 1644 | 1651 | 490,418 | 1729 (3 ranges) |
| Monte Nievas before | 1775 | 1772 | 5 | 19 | 22,644 | 0 |
| Monte Nievas after | 1680 | 1677 | 284 | 291 | 76,286 | 317 (3 ranges) |

Spawn cost (main thread):

| Town | Before | First fill (sync) | Streamed activation | Landing | Worst later frame |
|---|---|---|---|---|---|
| General Pico | 2.8 ms | 77–87 ms | 1.1–1.4 ms | ~30 frames | 1.4 ms |
| Monte Nievas | 1.7 ms | 7–14 ms | 0.9–1.8 ms | ~5 frames | — |

- About 1.5k triangles per building.
- Shadows reach only 100 m (3 cascades), so far buildings cast nothing.

## Sabotage log (each broken, seen failing, restored; source hash checked equal)

| Sabotage | Test seen failing |
|---|---|
| `chamfered` bevel set to 0 | `shape::a_chamfered_block_bevels_its_vertical_edges_and_faces_out` |
| `merge` leaves normals unrotated | `shape::a_merged_shape_moves_rigidly_into_its_place` |
| wall UV `u` halved | `shape::uvs_repeat_with_the_world_not_the_piece` (moved) |
| flat-face UVs swapped | `walls::the_lean_to_corrugation_runs_down_its_slope` (moved) |
| lathe normals flipped | `shape::a_lathe_closes_round_and_faces_out` (moved) |
| ORM not used for occlusion | `materials::the_orm_map_serves_occlusion_and_metallic_roughness` (moved) |
| Lime names a missing set | `materials::every_textured_surface_names_a_shipped_set` (moved), `settlement::every_set_ships_its_three_ktx2_maps_as_bc7_with_baked_mips` (changed: dedupes sets) |
| door leaf always built | `walls::a_wall_leaves_its_openings_open` (moved) |
| lean-to side gables raised 0.3 | `walls::a_lean_to_rests_on_all_four_walls` (moved) |
| lean-to overhang tripled | `walls::a_lean_to_overhangs_its_walls_a_little` (moved) |
| masonry not inset | `town_buildings::masonry_fronts_stay_inside_their_footprints` |
| store parapet 1.4 m | `town_buildings::a_store_parapet_stands_solid_behind_its_sign` |
| canopy 0.5 m short | `town_buildings::the_station_canopy_is_the_roof_core_gives_the_camera` |
| balusters 40 segments | `town_buildings::town_buildings_keep_to_their_triangle_budgets` |
| every house whitewashed | `town_buildings::town_houses_vary_their_wash_roof_and_dressing` |
| first fill built async | `town::only_towns_near_the_focus_are_built_and_far_ones_are_dropped` |
| every building in Brick's material | `town::each_building_is_one_mesh_per_surface_with_the_shared_materials` |
| building range `abrupt(0, 450)` | `town::towns_draw_within_the_shared_ranges_only` |
| deactivation despawns only children | `town::a_town_dropped_while_building_lands_nothing` (and the streaming test) |

Settlements unchanged: a temporary mesh fingerprint of the settlement test
layout gave 119 parts, hash `5823756589482928305`, both on `main` (a
`git archive` copy) and on the branch, before and after all changes. The
fingerprint test was then removed.

## Previews

The ignored test `facade::town_buildings::tests::preview_town_street`
(`PAMPA_PREVIEW=<dir> cargo test -p pampa-render -- --ignored preview_town`)
dumps three streets. They are rendered offline with textures (numpy +
ImageMagick, session scratchpad `render_preview.py`):

- `a_store.png`: almacén with galería;
- `a_police.png`: comisaría, lime house;
- `b_church.png`: church, brick house, pulpería, cottages;
- `c_station.png`: station, shed, grain store.

## Verification (2026-10-25, worktree `feat/town-facades`)

- `cargo test -p pampa-core`: 344 passed.
- `cargo test -p pampa-content`: 32 passed.
- `cargo test -p pampa-render`: 306 passed, 2 ignored (the previews).
- `cargo test -p game`: 118 passed.
- `cargo build -p game`: ok.
- `cargo fmt --all -- --check`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.

## Diff size

About 4.2k changed lines raw, of which about 1.3k are moved code (`walls.rs`
from `buildings.rs`, plus the `git mv`s). If one commit is too big, split it
into two that compile on their own:

1. The kit extraction: `facade/{mod,shape,materials,walls}.rs`, plus the
   settlement and `lib.rs` changes, with `mod.rs` not yet naming
   `italianate`/`town_buildings`.
2. The towns: `italianate.rs`, `town_buildings.rs` and its tests,
   `town.rs`, README and this doc.

## Follow-ups and next step

- `assets/textures/buildings/CREDITS.md` "Used for" could mention the
  towns. It is outside this task's surfaces; no new assets were added.
- Block size: lots are 8–10 m of frontage on 40 m blocks, enough for 3
  bays. Metileo's 30 m blocks give 6–7.5 m lots (2 bays) and too narrow a
  church front for its side windows. 40 m everywhere would do; 50 m+ is
  not needed.
- Next: phase 2, the Poly Haven plaster, brick and paving textures. It
  needs a download, with permission. Phase 5 replaces the light station.

## Checks

- `cargo test -p pampa-core`, `-p pampa-content`, `-p pampa-render`, `-p game`
- `cargo build -p game`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
