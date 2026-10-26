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

## Phase 2: new textures and sidewalks (branch feat/facades-textures, stacked on feat/metileo-blocks 243aaa0)

The user downloaded five Poly Haven CC0 1K sets on 2026-10-25, untracked,
into `assets/textures/buildings/<id>/` as `color.jpg`, `normal_gl.jpg`,
`roughness.jpg` and `ao.jpg`:
- `painted_plaster_wall`
- `whitewashed_brick`
- `red_brick_plaster_patch_02`
- `beige_wall_002`: a 1024x256 strip, good for plinths or bands
- `brick_pavement`

The URL pattern is
`https://dl.polyhaven.org/file/ph-assets/Textures/jpg/1k/<id>/<id>_{diff,nor_gl,rough,ao}_1k.jpg`.

Sidewalk rule (user, 2026-10-25):
- **Brick sidewalks** only in the big city, General Pico (tier Big or
  VeryBig).
- **Towns** (Small tier: Castex, Monte Nievas, Caleufú): a brick sidewalk
  only round the plaza.
- **The smallest villages** (Metileo, 3x3 or less): dirt only.

- [x] T2.1 Convert the five sets with `tools/textures_to_ktx2.sh` into
  KTX2 (`color`, `normal_gl`, `orm`). Keep the JPG sources. Credits go in
  `CREDITS.md` and `assets/textures/buildings/CREDITS.md` (Poly Haven, CC0,
  URLs, changes), including the towns under "Used for".
- [x] T2.2 New facade surfaces from the new sets:
  - painted plaster in a few muted 1919 tints, applied to the existing
    material copies per tint and shared;
  - whitewashed brick;
  - plaster flaking over brick;
  - a beige band for plinths and cornices if it suits.

  Spread them over the town buildings deterministically, with variety,
  in a muted palette that follows `odd/setting-1919.md`.
- [x] T2.3 Sidewalks per the rule above: curb strips along the block
  fronts with the brick_pavement material, flush or at a few cm, without
  changing core walking (or a tiny, tested core step if needed), using
  the shared ranges.
- [x] T2.4 Review follow-ups:
  - sheds and grain stores: walls inset `BARN_REACH` (0.45 m) but the
    collider is still the lot. Bring the visible wall and the collider
    together (move the collider in, or the wall out with eaves allowed past
    the lot as a documented exception);
  - `odd/tasks/metileo-blocks.md` "Open": the lamps fixture and the README
    lamp counts are already fixed (40 m, Castex 25, Caleufú 21); update it;
  - `game/src/towns.rs:398-402`: the church-front comment says 12 m with
    2.4 m per side but asserts 11 m; make them agree;
  - optional: derive `BARN_REACH` from the barn's eave constant.
- [x] T2.5 Tests (each with its sabotage), previews, docs, the perf note
  (VRAM).

Route: one delegated writer on `feat/facades-textures` (trigger: 2+
non-trivial files). Uncommitted; the user commits.

### Phase 2 decisions

- **Shared tinted copies.** Nine new `Surface`s, 24 in all, one shared
  material each: `Painted(Paint)` for `LimeWhite`, `Ochre`, `PaleRose`,
  `GreyBlue` and `Terracotta` (all over `painted_plaster_wall`, whose maps
  load once), `WashedBrick`, `PatchedBrick`, `Band` and `Paving`.
- **Lifted scans.** The new scans are darker than what they stand for
  (painted plaster's mean is about 0.4 linear). Their colour is
  `Look::Lifted`: the material colour is the target's linear albedo over the
  scan's measured mean, so a mean texel lands on a muted sRGB target and the
  scan keeps its stains. The few brightest texels go a little over white.
  The means are constants in `materials.rs`, with the `magick` command that
  measured them.
- **Band strip.** `Surface::tiles()` gives `u` and `v` repeats separately.
  `beige_wall_002` (1024x256) tiles 4 m by 1 m, so its texels stay square.
- **Spread.** House finishes come from `variant % 16` (the dressing bits
  stay free):
  - 6 painted (2 ochre, one each of the other paints) over a beige `Band`
    plinth, with whitewashed mouldings (grey `Stucco` mouldings on lime white);
  - 3 lime-washed, 2 whitewashed, 2 bare brick, 2 washed brick;
  - 1 patched brick: a young town has little flaking plaster.

  Almacén: lime, brick, washed brick, ochre or grey-blue. Pulpería: lime,
  whitewash or patched brick. One whitewashed outskirts rancho in four is
  washed brick. The comisaría, church and station are unchanged (phase 5
  redoes the station).
- **Sidewalks in core.** `pampa_core::town::Paving::of(blocks)` and
  `TownLayout::sidewalks()` (strips `Sidewalk::WIDTH` = 1.8 m, each with
  its curbed edges):
  - `Everywhere` (Big or VeryBig: General Pico): a ring round every block
    and round the plaza;
  - `Plaza` (Small: Castex, Monte Nievas, Caleufú): only the plaza's own
    edge, a ring round it (user, 2026-10-25, after playing: the facing
    block-front strips of the first cut were dropped);
  - `None` (9 blocks or fewer, 3 x 3: Metileo): dirt.

  The strips lie on the street, outside the block, so building footprints,
  colliders and signs are untouched.
- **Walking floor.** Paving tops out 0.06 m over the ground (streets 0.03,
  plaza paths 0.07), and the stucco curb 0.09 m. Core walks people at the
  ground's height, as it already does over the paths, so core walking is
  unchanged. Feet sink at most 9 cm at a curb, and a test keeps it there.
- **Sidewalk entities.** Touching strips (a ring) merge into one shape
  around the first strip's middle, one entity per surface, at
  `ranges()[0]`. No new `VisibilityRange`.
- **Galpón collider.** Core's `Building::collider()` is the footprint, but
  for a shed or grain store it is shrunk by `Building::BARN_EAVES` = 0.45
  m, where its walls stand. `obstacles()` uses it. Render's `BARN_REACH` is
  derived as `BARN_OVERHANG` (0.4) + `BARN_SHEET` (0.05; it was a 0.06 m
  sheet), and a test ties it to core's.
- **Camera occluders on colliders** (bug found in play, 2026-10-25). A
  player walking up to a galpón wall stands under its eaves, inside its
  footprint. The orbit camera's occluder was still the footprint, so the
  line of sight started inside it and was pulled in to the player's head (a
  close-up of the poncho). `CameraOccluders::from_town` (`game/src/camera.rs`,
  a four-line change) now builds each building's column over
  `Building::collider()`. That is the footprint for everything but a
  galpón. The eaves stay outside the occluder, 0.45 m of thin sheet.
- **Anachronism flagged.** `brick_pavement` is modern interlocking wavy
  pavers, not 1919 flat-laid brick. It is kept until the user picks a
  replacement (`brick_floor` or `recycled_brick_floor` were offered).

### Phase 2 perf (headless probe, `build_lots` + `sidewalk_shapes`, HEAD vs now)

| Town | Building mesh entities | Building triangles | Sidewalk groups / entities / triangles |
|---|---|---|---|
| General Pico before | 1662 | 456,122 | — |
| General Pico after | 1716 (+54) | 456,122 | 61 / 122 / 7,320 |
| Monte Nievas before | 288 | 61,860 | — |
| Monte Nievas after | 292 (+4) | 61,860 | 1 / 2 / 60 |

- The extra building entities come from a few fronts now wearing four
  surfaces where they wore three (for example a painted wall, whitewashed
  mouldings and a beige band). No triangles change.
- VRAM: five sets of three BC7 maps with mips. Four 1K sets take about
  4 MiB each and the 1024x256 strip about 1 MiB: about 17 MiB in all. On
  disk the KTX2 files take about 15.5 MB.
- Draw calls: up to nine more batches, one per new material, since Bevy
  batches by material bind group. This is an estimate; no GPU capture was
  taken.

### Phase 2 sabotage log (each broken, seen failing, restored; sha256 checked equal)

| Sabotage | Test seen failing |
|---|---|
| `collider` returns the footprint | `town::a_galpon_collides_at_its_walls_not_at_its_eaves` (core), `town_buildings::a_galpon_wall_stands_where_core_collides` |
| `obstacles` maps footprints | `town::obstacles_cover_buildings_and_trees` (changed) |
| `VILLAGE_MAX` = 4 | `town::paving_follows_the_town_size` |
| south ring strip inside the block | `town::sidewalks_lie_on_the_streets_clear_of_buildings_and_plazas` |
| a small town paves a block's ring too | `town::a_big_town_paves_every_block_a_small_one_its_plaza_edge_a_village_nothing` |
| camera occluders built over footprints | `camera::walking_into_a_galpon_keeps_the_camera_off_the_player` (game; also seen failing before the fix, at frame 173 with the camera 0.15 m from the head) |
| lift multiplies instead of dividing | `materials::the_new_scans_are_lifted_to_muted_1919_colours` |
| band tiled 4 x 4 | `materials::the_band_strip_keeps_its_texels_square` |
| band wears `plaster_003` | `settlement::every_set_ships_its_three_ktx2_maps_as_bc7_with_baked_mips` (changed: 14 sets) |
| galpón walls on the lot | `town_buildings::a_galpon_wall_stands_where_core_collides` |
| no patched-brick front | `town_buildings::town_houses_vary_their_wash_roof_and_dressing` (changed) |
| sidewalks on the props range | `town::a_small_town_paves_round_its_plaza_with_the_shared_paving_and_curb` |
| curb 0.2 m higher | `town::sidewalks_stand_a_few_centimeters_proud_like_the_plaza_paths` |
| touching strips not merged | `town::sidewalks_stand_a_few_centimeters_proud_like_the_plaza_paths` |
| church front 11 m (core `landmark_shape`) | `towns::metileo_has_lots_wide_enough_for_three_bay_houses_and_a_windowed_church` (game; the old 11 m assert passed) |

### Phase 2 previews

These are offline renders made with the session scratchpad's
`render2.py`, a numpy software renderer with textures. It now subdivides
large triangles and knows the new sets and their lifts.

- `preview_town_sidewalks` (ignored, in `town.rs`) dumps General Pico,
  Eduardo Castex and Metileo as shipped, on flat ground.
- `preview_town_street` gained `street_d`: one house of each new finish.
- Images: `pico_oblique.png` (brick sidewalks round the plaza and every
  block), `castex_plaza.png` (the plaza's own ring only),
  `metileo_street.png` (dirt only), and `walls_overview.png`,
  `walls_close_a.png`, `walls_close_b.png` (the paints, washed brick and
  patched brick).

### Phase 2 verification (2026-10-25)

- `cargo test -p pampa-core`: 349 passed.
- `cargo test -p pampa-content`: 32 passed.
- `cargo test -p pampa-render`: 325 passed, 4 ignored (the previews).
- `cargo test -p game`: 121 passed.
- `cargo build -p game`: ok.
- `cargo fmt --all -- --check`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `sh tools/textures_to_ktx2.sh --check-orm`: ORM map: AO in red,
  roughness in green, white in blue.

Diff: about 1.3k changed lines, not counting the KTX2 and JPG binaries.

### Phase 2 follow-ups

- Replace `brick_pavement` with a flat-laid brick set once the user
  downloads one, and re-run `textures_to_ktx2.sh` on it.
- In game, check the lifted paints under the real sun and fog. The offline
  renderer has no PBR, so the scans' few brightest texels (lifted past
  white) are worth a look up close.

## Phase 3: the plaza (branch feat/plaza, from origin/main 71017f6)

Goal (user's roadmap: finish the towns): turn the plaza from a lawn, a
cross of 3 m paths, a concrete box and sphere trees into a 1919 pampa
plaza. Trees stay as they are; phase 4 replaces them.

- [x] P3.1 Core layout, `pampa_core::town::PlazaFurniture`, from
  `TownLayout::plaza_furniture()` per plaza, tiered by `Paving`:
  - the monument's collider (lowest step), inside the old 3x3 `plaza_monument`;
  - `plaza_crossing`: a paved 9 m square round the monument;
  - `Bench`es beside the paths, facing them;
  - railing pieces (the city only, user 2026-10-26).

  `obstacles()` now includes the monument, the benches and the railing.
- [x] P3.2 Render `facade/plaza.rs`: curbed paths and the crossing, the
  monument, benches and railing. Built in the async `build_lots` task
  (`Part::Plaza`, `TownPlaza(index)`). The old path patches and the
  monument boxes are gone; the lawn patch stays.
- [x] P3.3 `Surface::Earth`: opaque `dry_ground_01`, for earth paths.
  `Yard` and `Mud` blend by vertex alpha, so they would sort as
  transparent.
- [x] P3.4 Tests with sabotages, previews, README, module docs, this
  section.

Route: one delegated writer in an isolated worktree on `feat/plaza`
(trigger: 2+ non-trivial files). Uncommitted; the user commits.

### Phase 3 decisions

- **Tiers follow the sidewalk rule** (`Paving`):

  | Tier | Paths | Curbs | Monument | Benches | Railing |
  |---|---|---|---|---|---|
  | `Everywhere` (General Pico) | brick `Paving` | `Stucco` | 3 steps, about 8.8 m, urns on the cornice | 3 on each side of each path, 24 (1 a side, 8, in the station plaza) | iron over a 0.35 m stucco plinth between chamfered piers, 1.05 m |
  | `Plaza` (Castex, Monte Nievas, Caleufú) | `Earth` | `Brick` | 2 steps, about 6.4 m | 1 by each path, on its left: one to a quadrant, 4 | none |
  | `None` (Metileo) | `Earth` | none | 1 step, about 4.5 m | 1 by the east and 1 by the west path, 2 | none |

  User review (2026-10-26): sparse plazas and earth paths are right for
  1919 pampa towns. The first cut gave the towns a plain iron railing and
  2 benches a path side (16), and the village 1 a side (8). They were
  dropped to the above: only the city fences its plaza, and the small
  plazas stay sparse (`PlazaFurniture::bench_plan`).

  Curbs are 0.15 m wide, on the lawn side, 0.10 m tall (3 cm over the
  0.07 m path). The paths keep their 3 m width.
- **Monument** (in the manner of the Pirámide de Mayo):
  - chamfered stucco steps;
  - a whitewashed pedestal on a moulded stucco base, with a dark plaque
    on each face (towns and the city);
  - a two-part stucco cornice, then a die;
  - a tapering whitewashed obelisk with a pyramidion;
  - an iron ball finial.

  The lowest step is 2.4 m in a village and 3 m elsewhere. It never
  outgrows core's 3x3 plinth, so the four lamps round Pico's monument (at
  ±2.2 m, with a 0.5 m clearance) stay.
- **The monument collides now** (it did not before). It filled the 3 m
  crossing of the paths, so core adds the paved crossing: 3 m all round
  it, where people and riders walk round it. The plaza spot (Don
  Hidalgo, 6 m out) stays on the path, past the crossing.
- **Benches are colliders** (cheap: static boxes in `obstacles()`). Each
  is 1.8 x 0.6 m, 0.3 m off the path's edge (past the curb), facing the
  path:
  - spread evenly between 0.5 m past the crossing and 2.6 m before the
    edge, clear of the mouth lamps' reach;
  - trees stand at least 3.35 m from a path's middle, so they never meet
    a bench;
  - a bench on a tree trunk or a sign would be dropped (none is in the
    shipped or sample towns).
- **Railing collider**: 0.05–0.35 m in from the plaza's edge. The mouth
  lamps stand 1 m in, and their 0.5 m clearance starts 0.38 m in, so the
  railing never drops a lamp. It opens 1.65 m to either side of each
  path's middle (the path and its curbs). Its piers and posts stand
  inside the collider. Corners are closed; nobody enters but by a path.
- **Unchanged**: lamp sites and counts, the plaza sign, the townsfolk
  graph (streets only), streets, sidewalks and the paths' layout. No new
  `VisibilityRange`:
  - paths, curbs, crossing and monument use `ranges()[0]` (BUILDING_FAR);
  - benches and railing use `ranges()[1]` (PROP_FAR);
  - trees stay on `ranges()[2]`.

  The lawn patch keeps no range, as before.
- **Meshes**: each plaza is two merged `Shape`s (ground + monument,
  furniture), one mesh per surface each, on the shared `FacadeMaterials`.
  Castex's plaza is 7 entities (5 ground surfaces, then planks and iron for
  its 4 benches). Bench orientation is a yaw
  (`from_rotation_y`), so a half turn never flips a bench.
- **Camera**: the monument is not a camera occluder (only buildings are).
  At 0.9–3 m wide it rarely hides the player; revisit if it does.

### Phase 3 perf

Triangle budget: under 30k a plaza (tested), monument under 1.5k, bench
under 250.

| Plaza | Ground (paths, curbs, monument) | Furniture (benches, railing) |
|---|---|---|
| General Pico main (90 m, city) | 864 | 20,960 |

Most of Pico's furniture triangles are its 0.25 m pickets round a 360 m
edge. Earth reuses `dry_ground_01`, so no new texture memory is used.
Draws: one new material (`Earth`). The rest share the existing ones.
This is an estimate from the source; no GPU capture was taken.

### Phase 3 sabotage log (each broken, seen failing, restored; sha256 checked equal)

| Sabotage | Test seen failing |
|---|---|
| village monument 3.0 m | `town::plaza_furniture_follows_the_town_size` |
| benches 0.3 m inside the path's edge | `town::plaza_furniture_keeps_off_the_paths_streets_trees_and_signs` |
| railing laid round the plaza grown 1 m | `town::plaza_furniture_keeps_off_the_paths_streets_trees_and_signs` |
| `RAILING_DEPTH` 0.5 | `lamps::plaza_furniture_keeps_every_plaza_lamp_and_the_paths_walkable` (General Pico loses a mouth lamp; rerun after the review, Monte Nievas has no railing now) |
| `MOUTH_HALF` 0.6 (mouths closed) | `lamps::plaza_furniture_keeps_every_plaza_lamp_and_the_paths_walkable` (the rider's mouth blocked) |
| benches on the paths' middle | `lamps::plaza_furniture_keeps_every_plaza_lamp_and_the_paths_walkable` (the plaza spot unreachable) |
| the monument left out of `obstacles()` | `town::obstacles_cover_buildings_and_trees` (changed) |
| a box inside the plaza among the obstacles | `town::no_street_runs_through_a_merged_plaza` (changed: furniture allowed) |
| town paths in brick `Paving` | `plaza::each_tier_dresses_its_paths_as_the_sidewalk_rule_does` |
| Pico's lowest step 3.4 m | `plaza::the_monument_grows_with_the_town_and_stays_on_its_plinth` |
| benches turned a half turn | `plaza::a_bench_keeps_within_its_footprint_and_faces_its_path` |
| benches turned about `x` (flipped) | `plaza::a_bench_keeps_within_its_footprint_and_faces_its_path`, `plaza::the_railing_stands_within_its_colliders_open_at_the_mouths` |
| piers 0.4 m wide | `plaza::the_railing_stands_within_its_colliders_open_at_the_mouths` |
| pickets every 5 cm (77k triangles) | `plaza::a_plaza_keeps_to_its_triangle_budget` |
| furniture on the trees' range | `town::each_plaza_is_one_mesh_per_surface_and_range_with_the_shared_materials` |
| `Earth` names a missing set | `materials::every_textured_surface_names_a_shipped_set` |
| after the review: towns railed again | `town::plaza_furniture_follows_the_town_size`, `plaza::each_tier_dresses_its_paths_as_the_sidewalk_rule_does` |
| after the review: town benches on both sides (8) | `town::plaza_furniture_follows_the_town_size` |
| after the review: village benches by all four paths | `town::plaza_furniture_follows_the_town_size` |
| after the review: town benches by two paths twice (two to a quadrant) | `town::plaza_furniture_follows_the_town_size` (one to a quadrant) |

Not a defect: `Quat::from_rotation_arc(Z, -Z)` (the first cut) turns
about `y` in glam, so that sabotage stayed green. The yaw-only rotation
was kept for clarity.

### Phase 3 previews

`preview_town_sidewalks` (ignored, `town.rs`) now dumps each plaza's
meshes and trees. It is rendered with the session scratchpad's
`render3.py`, a numpy software rasterizer with textures (no PBR, no
shadows):
- `pico_overview.png`, `pico_eye.png`, `pico_mouth.png` (railing, piers
  and brick paths at a mouth);
- `castex_overview.png`, `castex_eye.png`;
- `metileo_overview.png`, `metileo_eye.png`.

Castex and Metileo were re-rendered after the review (no railing, fewer
benches).

### Phase 3 verification (2026-10-26)

- `cargo test -p pampa-core`: 352 passed.
- `cargo test -p pampa-content`: 32 passed.
- `cargo test -p pampa-render`: 335 passed, 4 ignored (the previews).
- `cargo test -p game`: 131 passed.
- `cargo build -p game`: ok.
- `cargo fmt --all -- --check`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.

Diff: about 1.6k changed lines (1,559 insertions, 70 deletions before this
note), with about 720 in the new `facade/plaza.rs` and its tests.

### Phase 3 follow-ups and next step

- The plaza area is big in Pico (90 m): three benches per path side
  leave about 18 m between benches. More, or benches round the crossing,
  if it feels empty in game.
- `dry_ground_01` reads as cracked mud up close. A packed-earth or tosca
  set would suit the paths better (it needs a download).
- The plaques are plain dark iron (`Steel`). A bronze material would need
  a new surface.
- Next: phase 4, better trees (LeafSet cards, paraíso, olmo, eucalipto,
  aromo).

## Checks

- `cargo test -p pampa-core`, `-p pampa-content`, `-p pampa-render`, `-p game`
- `cargo build -p game`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`

## Phase 4: better trees (branch feat/plaza-trees, from main 5bfbe2b)

Replace the plaza trees (a 16-sided cylinder under a squashed icosphere,
flat colours, `crates/render/src/town.rs:601-627`) with procedural trees:
tapered bark trunks and limbs carrying alpha-tested leaf cards, in four
species of 1919 pampa plazas: paraíso, olmo, eucalipto, aromo.

Assets: ambientCG CC0, 1K-JPG, downloaded by the user on 2026-10-26 into
`assets/textures/trees/<Id>/` (47 MB raw, the zips' extras included).

| Species | Leaves | Bark |
|---|---|---|
| olmo | LeafSet001 | Bark001 |
| eucalipto | LeafSet003 | Bark009 |
| aromo | LeafSet019 (tint silver-green) | Bark002 |
| paraíso | atlas composed from LeafSet013 leaflets (no compound leaf exists) | Bark014 |

Constraints: shared `ranges()[2]` only (no new VisibilityRange); Renoir iGPU
budget, about 0.5–1k triangles per tree, one bark and one leaf material per
species, shared meshes (a few seeded variants per species); no core layout
change (species chosen in render from the town seed); every test with its
sabotage; credits in a folder CREDITS.md and the root CREDITS.md.

### Tasks (phase 4)

- [x] T1 Textures: keep only Color, NormalGL, Roughness, Opacity (and AO)
  per set; RGBA leaf `color.ktx2` (Opacity in alpha) through
  `tools/textures_to_ktx2.sh`; the paraíso pinnate atlas built from
  LeafSet013 by a reproducible tool; credits.
- [x] T2 Leaf and bark materials: `AlphaMode::Mask` (or AlphaToCoverage if
  MSAA is on), double-sided, clamp-to-edge atlas sampler; plain-colour
  fallback without assets.
- [x] T3 Tree builder: seeded tapered trunk and limbs with bark UVs, leaf
  cards clustered at branch tips with normals bent outward; per-species
  shape (paraíso round umbrella, olmo tall vase, eucalipto tall and open,
  aromo low and feathery); smoothed normals; triangle budget test.
- [x] T4 Plazas: species per plaza from the town seed, the station plaza
  may differ; trunks stay the obstacles core already places; ThinOccluder
  like settlement trees; preview render.
- [x] T5 Tests with sabotages, docs, full checks.

Route: one delegated writer in the main checkout (trigger: 2+ non-trivial
files across tools, render and assets). Uncommitted; the user commits.

### Checks

- `cargo test -p pampa-core`, `-p pampa-render`, `-p game`
- `python3 -I -B -m unittest discover -s tools` (if a tool changes)
- `cargo build -p game`, `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`

### Phase 4 decisions

- **Every species gets a composed atlas**, not only the paraíso. The
  LeafSet sheets hold single leaves (LeafSet019: three loose sprays), so a
  card mapped to a whole sheet showed a grid of six oversized leaves.
  `tools/leaf_atlas.py` cuts each leaf out by its opacity
  (`-connected-components`) and lays them along drawn stems into four
  sprigs a 1024² atlas, one to a quarter, the base 32 texels over the
  quarter's bottom edge (`SPRIG_BASE`) and a 16-texel empty margin:
  paraíso leaflets in opposite pairs on a rachis (pinnate), olmo leaves
  alternating on a twig, eucalipto leaves hanging close to a twig, the
  aromo's sprays stood upright. Leaves are rotated, so the normal map's
  x/y are rotated with them (`-color-matrix`). Seeded and reproducible
  (re-run byte-identical, sha256 checked on the paraíso atlas).
- **Files follow the buildings' convention**: each source folder keeps its
  renamed 1K JPGs (`color`, `normal_gl`, `roughness`, plus `ao` on the
  barks, `opacity` on the leaf sets); the bark folders and the four
  `*_leaves/` atlases ship `color/normal_gl/orm.ktx2`. The LeafSet folders
  are tool inputs only (no KTX2). The zip extras (.blend, .mtlx, .tres,
  .usdc, previews, Displacement, NormalDX) are gone: the writer's `rm` was
  denied by permissions, and they were removed outside it. One unused file
  remains: `LeafSet003/LeafSet003_1K-JPG_AmbientOcclusion.jpg`.
- **Alpha path**: `textures_to_ktx2.sh` packs `opacity.jpg` into the colour's
  alpha (RGBA PNG, then UASTC `-srgb`); a set without it encodes exactly as
  before (building `brick_pavement` and `beige_wall_002` re-encoded
  byte-identical, sha256). `--check-alpha` proves the opacity lands in
  alpha after a BC7 transcode.
- **`AlphaMode::Mask(0.5)`, not AlphaToCoverage.** MSAA is Bevy's default
  (Sample4, the game sets no `Msaa`), but Mask is the safe choice for the
  shadow and prepass and for the `ThinOccluder` fade copy (which switches
  to `Blend`). The leaf material has `cull_mode: None` and
  `double_sided: false` on purpose: Bevy's double-sided flag flips a back
  face's normal, which would turn the outward-bent normals into the crown.
- **Shape**: a tapered 8-sided trunk (the eucalipto's goes on up as a
  leader), 4–7 limbs of 6 sides in two segments, 2–4 three-sided twigs per
  limb; cards spread along the limbs' outer halves and the twigs (by
  length, biased to the ends), each card two quads bent down by its sag.
  Bark UVs wrap a whole number of times round each tube (no seam). Normals
  are analytic (radial on tubes, bent out of the crown on cards), so
  `crate::smooth` was not needed.
- **Species per plaza** (`tree::plaza_species`, from the town seed): the
  station plaza always gets another species. Shipped preview towns:
  General Pico eucalipto (station: olmo), Eduardo Castex olmo, Metileo
  eucalipto.
- **Sharing**: 4 species x 3 variants, built once at startup into
  `TreeKit` (24 meshes, 8 materials for the whole game). Each plaza tree is
  2 entities (bark, leaves), as before, on `ranges()[2]`, with a
  `ThinOccluder` of its crown's reach and height, yaw and scale
  (0.92–1.08) seeded by its spot. The trunk stays inside core's 0.35 m
  collider at the largest scale (tested).

### Phase 4 perf

| Species | Bark | Leaves | Total | Height x crown width (mean, m) |
|---|---|---|---|---|
| paraíso | 242 | 480 | 722 | 6.5 x 9.6 |
| olmo | 272 | 560 | 832 | 11.8 x 10.6 |
| eucalipto | 316–358 | 600 | 916–958 | 15.4 x 9.3 |
| aromo | 242–284 | 520 | 762–804 | 4.6 x 7.8 |

(Crown width is the farthest card corner, so it overstates the mass.)
Per town: General Pico 36 trees (72 entities, ~32k triangles, 4
materials), Castex 12 (24 entities, ~10k, 2 materials), Metileo 12 (~11k,
2 materials). The old trees were about 150 triangles each. Textures: 8
sets x 3 maps at 1K UASTC, 16 MB on disk, loaded at startup (BC7 about
1.4 MB a map with mips). No GPU capture was taken.

### Phase 4 sabotage log (each broken, seen failing, restored; sha256 checked equal)

| Sabotage | Test seen failing |
|---|---|
| `--check-alpha`: opacity never packed | `textures_to_ktx2.sh --check-alpha` (alpha 40, expected 64) |
| leaf components of any area | `test_leaf_atlas.Layout.test_only_the_big_white_objects_are_leaves` |
| normal x/y rotated the wrong way (R row) | `test_leaf_atlas.Layout.test_a_turned_normal_map_turns_its_vectors_with_it` (first cut stayed green: case added, up turned right) |
| sprigs not fitted to their quarter | `test_leaf_atlas.Layout.test_every_sprig_keeps_inside_its_quarter_and_is_reproducible` |
| leaves turned counter-clockwise | `test_leaf_atlas.Atlas.test_a_leaf_turned_clockwise_lands_there_with_its_normals_turned` |
| black ground under the leaves | `test_leaf_atlas.Atlas.test_the_atlas_holds_four_sprigs_with_empty_margins` |
| `+virtual-pixel` reset dropped (a real bug of the first cut: cut-outs came out as opaque boxes) | `test_leaf_atlas.Atlas.test_each_leaf_is_cut_out_along_its_outline` (added after this sabotage stayed green) |
| olmo 300 cards | `tree::mesh::every_tree_keeps_to_the_triangle_budget` |
| seed ignored | `tree::mesh::a_variant_is_its_seed_and_the_seeds_differ` |
| eucalipto trunk 0.36 m | `tree::mesh::the_trunk_stands_within_its_collider` |
| plaza scale up to 1.3 | `tree::mesh::the_trunk_stands_within_its_collider` |
| olmo limbs lean 55–70° | `tree::mesh::each_species_keeps_its_shape` |
| eucalipto cards reach up (`hang` 0) | `tree::mesh::each_species_keeps_its_shape` |
| leaf normals into the crown | `tree::mesh::leaf_normals_lean_out_of_the_crown` |
| cards upside down in the atlas | `tree::mesh::each_card_shows_one_sprig_hung_by_its_base` |
| bark wraps not whole | `tree::mesh::bark_repeats_whole_times_round_and_tangents_follow_it` |
| leaves `Blend` | `tree::materials::leaves_are_alpha_tested_cards_seen_from_both_sides` |
| leaves `double_sided` | `tree::materials::leaves_are_alpha_tested_cards_seen_from_both_sides` |
| atlas sampler repeats | `tree::materials::the_atlas_clamps_and_the_bark_repeats` |
| aromo atlas misnamed | `tree::materials::every_species_names_shipped_bark_and_leaf_sets` |
| untextured leaves tinted, not plain | `tree::materials::missing_sets_draw_plain_colour` |
| station plaza same species | `tree::plaza_species_follow_the_seed_and_the_two_plazas_differ` |
| one variant built | `tree::the_kit_builds_each_variant_once` |
| trees on the props' range | `town::plaza_trees_share_the_kit_and_fade_out_of_the_way` |
| trees without `ThinOccluder` | `town::plaza_trees_share_the_kit_and_fade_out_of_the_way` |
| every tree variant 0 | `town::plaza_trees_share_the_kit_and_fade_out_of_the_way` |
| every tree counted in the main plaza | `town::a_town_with_a_station_plaza_plants_another_species_there` |

RED/GREEN: the shape and collider tests were first seen failing on real
tuning (crowns 11–14 m wide, olmo wider than tall, a trunk foot test off
by the lean) and went green by changing the forms, not the tests (the
foot test was loosened to 2 cm for the lean).

### Phase 4 previews

`preview_tree_species` (ignored, `town.rs`) dumps the four species x three
variants in a row; `preview_town_sidewalks` now dumps the real trees. Both
rendered with a scratch copy of the software rasterizer extended for
alpha-tested leaves (`render4.py`, session scratchpad, no PBR/shadows):
`trees/species_row.png`, `trees/paraiso_close.png`,
`trees/castex_eye.png`, `trees/castex_overview.png`, `trees/pico_eye.png`
under the session scratchpad.

### Phase 4 verification (2026-10-26)

- `cargo test -p pampa-core`: 352 passed.
- `cargo test -p pampa-render`: 354 passed, 5 ignored (the previews).
- `cargo test -p game`: 136 passed.
- `python3 -I -B -m unittest discover -s tools`: 146 passed.
- `cargo build -p game`: ok.
- `cargo fmt --all -- --check`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `tools/textures_to_ktx2.sh --check-alpha` and `--check-orm`: pass.

### Phase 4 follow-ups and next step

- Not seen in the game yet: check the leaves' tints and the paraíso's
  density. The Mask cutoff at a distance was seen (crowns thinned at
  100–150 m) and is fixed by the tree crowns follow-up below.
- `tools/README.md`: `leaf_atlas.py` section and the `--check-alpha` path
  added by the parent (done).
- Delete `LeafSet003/LeafSet003_1K-JPG_AmbientOcclusion.jpg` (unused).
- Next: run the game, then the native review of the work-unit commit.

### Tree crowns follow-up (branch fix/tree-crowns, from 2c9a773)

The native review of 2c9a773 approved with findings, and the user saw
distant crowns (100–150 m) thinning in the game. Route: one delegated
writer in the main checkout (trigger: tools, assets and render files).
Uncommitted; the user commits.

- [x] C1 Crowns thinning with distance (R4-001, R3-mask-cutoff-mips):
  coverage-preserving mips for the leaf atlases.
- [x] C2 Plaza fallback (R2-tree-plaza-fallback, R3-plaza-fallback): a
  tree outside every plaza goes to the nearest one, with a warning. A
  sweep test checks that every tree stands in exactly one plaza.
- [x] C3 Suggestions: `TREE_SCALE_MIN` and `TREE_SINK` named
  (R2-tree-scale-magic, R2-tree-sink-constant); `Form::height` documented
  as the layout height, not the built one (R2-form-height-misleading);
  `leaf_atlas.py` points at `SPRIG_BASE` in `tree/mesh.rs`
  (R2-atlas-doc-pointer).

#### Decisions

- **Measured first.** Shipped atlases unpacked to BC7 (what the GPU
  holds), share of texels with alpha ≥ 0.5 per level: paraíso 0.081 at
  L0, 0.050 at L5 (32²), 0.012 at L6, 0 at L7. Olmo 0.143 → 0.094 at L7,
  aromo 0.134 → 0.047 at L7, and every species was 0 from L8 (4²) down. A
  card about 1 m wide over a 512-texel quarter, at 125 m, 1080p, 60° fov,
  is about 67 texels a pixel: mip 6–7, where the paraíso had lost most of
  its leaves.
- **Not AlphaToCoverage.** MSAA is on: the game has one camera
  (`game/src/camera.rs:457`, `Camera3d::default()`, `Msaa::Sample4`), no
  `Msaa` override, no HDR or post-processing, no prepass components. A2C
  would not fix the thinning, though. Bevy 0.20 sharpens A2C alpha as
  `(a - cutoff) / fwidth(a) + 0.5` round a fixed 0.5
  (`bevy_pbr-0.20.0/src/render/pbr_fragment.wesl:212-226`,
  `pbr_material.rs:1189-1203`), so a low mip's smooth alpha under 0.5
  still draws nothing. Its shadow and prepass cut at 0.05, not at its
  cutoff (`pbr_prepass_functions.wesl:17-31`, `render/light.rs:2585-2591`,
  `prepass/mod.rs:1138-1142`), so shadows would come from the leaves'
  fringes. Without MSAA it falls back to a mask
  (`material.rs:697-700`).
- **Coverage-preserving mips** (after Castaño, "Computing alpha mipmaps",
  2010). `leaf_atlas.py --mips` averages each level from the plain level
  above (colour in linear light, alpha as is). It then remaps the
  level's alpha so that it draws the share of texels the top level
  draws: the threshold whose count is nearest the wanted one lands on
  128, and every texel keeps 8 steps (`GUARD`) clear of it. Without the
  guard, UASTC→BC7 moved enough texels across the cutoff to miss by up to
  35% at 8² (seen in a sabotage). It writes the chain as a mipmapped
  32-bit RGBA DDS. basisu 2.10 encodes a DDS's levels as given (tried
  first with distinct colours per level; read back per level after the
  BC7 unpack), so the leaf colour encodes without `-mipmap`.
  `LEAF_CUTOFF` and the tool's `CUTOFF = 128` are tied by a test.
- **Result** (BC7, per level L0 … L8): paraíso 0.081 0.081 0.081 0.081
  0.081 0.081 0.082 0.078 0.063; olmo 0.143 … 0.145 0.141 0.125;
  eucalipto 0.146 … 0.145 0.141 0.125; aromo 0.134 … 0.129 0.141 0.125.
  Every level of 8² and up is within 4% of L0. Only the four leaf
  `color.ktx2` changed. Their `normal_gl`/`orm`, every bark and building
  set (brick_pavement and Bark014 re-encoded in the scratchpad) stay
  byte-identical, and re-encoding the atlases is reproducible (sha256).
  File sizes are 2–4% smaller.
- **Plaza assignment**: `plaza_of` returns the plaza that holds the tree,
  or else the nearest one by distance to its box, with a `warn!`. No
  shipped-towns test: `pampa-render` cannot read
  `assets/world/towns.ron` (that needs `pampa-content`, only the game
  depends on it). The render test sweeps the shapes the shipped towns take
  instead (3–8 blocks a side, with no railway, a railway, or a through
  line, seeds 1, 2, 13, 19, more than 50 towns) and asserts each tree lies
  in exactly one plaza's Aabb and is filed under it.

#### Tree crowns follow-up sabotage log (each broken, seen failing, restored; sha256 checked equal)

| Sabotage | Test seen failing |
|---|---|
| shipped atlases as at 2c9a773 (plain basisu mips; RED before the fix) | `test_leaf_atlas.ShippedAtlases.test_distant_crowns_keep_the_coverage_of_the_near_ones` (paraíso 32²: 0.050 vs 0.081) |
| colour encoded with plain `-mipmap` again (atlases re-encoded, then restored identical) | `test_leaf_atlas.ShippedAtlases.test_distant_crowns_keep_the_coverage_of_the_near_ones` |
| `GUARD = 0` (atlases re-encoded, then restored identical) | `test_leaf_atlas.ShippedAtlases.test_distant_crowns_keep_the_coverage_of_the_near_ones` (paraíso 8²: 0.109) |
| mips not remapped | `test_leaf_atlas.Mips.test_the_mips_keep_the_cutoff_coverage_of_the_top_level` |
| threshold texels remapped under the cutoff | `test_leaf_atlas.Mips.test_rescaled_alpha_draws_the_wanted_share_and_keeps_its_order` |
| alpha 128 not drawn (`>` for `>=`) | `test_leaf_atlas.Mips.test_a_texel_is_drawn_from_half_alpha_up` |
| colour averaged in sRGB bytes | `test_leaf_atlas.Mips.test_colour_mips_average_in_linear_light` |
| DDS mip count 1 | `test_leaf_atlas.Mips.test_the_dds_holds_every_level_as_rgba` |
| `LEAF_CUTOFF` 0.6 | `tree::materials::the_atlas_mips_keep_coverage_for_this_cutoff` |
| old fallback, plaza 0 (RED before the fix) | `town::a_tree_outside_every_plaza_goes_to_the_nearest` ("just outside") |
| farthest plaza instead of nearest | `town::a_tree_outside_every_plaza_goes_to_the_nearest` |
| every tree filed among plaza 0 only | `town::every_tree_stands_in_exactly_one_plaza_and_is_filed_under_it` |
| scale spread doubled under `TREE_SCALE_MIN` | `town::plaza_trees_share_the_kit_and_fade_out_of_the_way` |

#### Verification (2026-10-26)

- `cargo test -p pampa-render`: 357 passed, 5 ignored.
- `cargo test -p game`: 136 passed.
- `python3 -I -B -m unittest discover -s tools`: 152 passed.
- `cargo build -p game`: ok.
- `cargo fmt --all -- --check`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `tools/textures_to_ktx2.sh --check-alpha` and `--check-orm`: pass.

#### Follow-ups and next step

- In the game: crowns at 100–250 m should keep their density, and the
  shadows under far trees too (shadows use the same cutoff and mips).
  Very close up nothing changes (level 0 is untouched).
- A shipped-towns plaza test belongs in `game/src/towns.rs` if wanted
  (it would need `plaza_tree_sites` public from `pampa-render`).
- Next: the user plays it and commits. The native review takes the work
  unit commit.

#### Native review of the crowns follow-up (2026-10-26)

Lineage review-0c871c7925dfd036, high risk, 4 lenses: approved and
acknowledged. The one WARNING (R4-python3-not-preflighted) was fixed after
it: `textures_to_ktx2.sh` adds `python3` to its tool preflight when an
argument is `--check-alpha` or a set with `opacity.jpg`, so a machine
without it fails before any output is written. Checked by running
`--check-alpha` with a PATH lacking python3: it stops with
"needs `python3` on PATH"; with python3, `--check-alpha` passes. The
SUGGESTIONs (encode/encode_levels duplication, help docstring slice, the
empty-plaza fallback, a hard-coded height note, the shipped-atlas test
skipping without basisu) are left as follow-ups.
