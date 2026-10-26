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

## Checks

- `cargo test -p pampa-core`, `-p pampa-content`, `-p pampa-render`, `-p game`
- `cargo build -p game`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
