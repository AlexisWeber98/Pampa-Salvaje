# Feature: pampa-line

Locator: `odd/tasks/pampa-line.md` · Engram mirror: `odd/pampa-line/tasks`

## Objective

Lay out the first real stretch of La Pampa: three towns at their real relative positions (Eduardo Castex, Monte Nievas, General Pico), joined by one railway line, plus a first errand. In the errand, Don Hidalgo in the Monte Nievas plaza sends the player to comisario Elías Farach at the Castex police station, and the minimap marks the destination.

## Problem / Why

- `towns.ron` today holds Monte Nieves (misspelled), a Castex that overlaps it and gets rejected by `place_towns`, and Florencio Varela, which is not on this line.
- Big towns look like small ones: there is one plaza and one general store per town, whatever the size.
- Each town's railway is an isolated axis-aligned segment, and nothing connects towns.
- There is no quest system or quest giver.

## Scope

Three chained PRs, as the user requested:

1. **Town rules (`feat/town-rules`)**
   - Big towns get two plazas and two general stores.
   - `towns.ron` holds the three towns at geographic positions.
2. **Railway rule (`feat/rail-line`)**
   - Railway towns are connected by a line of straight segments at any angle.
   - Validation and placement treat the line as a rule.
   - The line is rendered and streamed by chunk, not by town.
3. **Errand (`feat/errand`)**
   - A static NPC, Don Hidalgo, stands in the Monte Nievas plaza.
   - Comisario Elías Farach stands at the Castex police station.
   - Talking to Hidalgo starts the errand; reaching Elías Farach completes it.
   - A minimap target marker shows the destination, pinned to the map edge while the target is off-map.

## Rules chosen

- **Town size tiers** (user-approved 2026-10-20; this replaces the first ">= 36 means two plazas" rule). `n = blocks.0 * blocks.1`:

  | | Small (n ≤ 36) | Big (37–63) | Very big (n ≥ 64) |
  |---|---|---|---|
  | Central plaza | 1 block (as today) | 2 adjacent blocks merged, including the street between them | 2×2 blocks, centered |
  | Station plaza (half block, railway side, facing the station) | no | yes | yes |
  | General stores | 1 | 2 | `n / 16` (Pico: 4) |
  | Police stations | 1 | 1 | 2 |
  | Townsfolk | 16 | `max(16, round(0.8 n))` | same formula (Pico: 51) |

  - Small towns stay byte-for-byte identical; the layout hash test guards this.
  - The tethered horse and hitching post stay at the first general store.
  - Monte Nievas (4×3) and Castex (5×4) are small, General Pico (8×8) is very big.
- **Geography** (scale changed from 1:4 to 1:5, user-approved 2026-10-20): real offsets scaled 1:5 and fit inside the ±8000 m world (`half_extent: 8000`). Monte Nievas keeps its origin.
  - Castex to Monte Nievas: about 12.6 km E and 5.4 km N.
  - Monte Nievas to General Pico: about 35.8 km E and 22.4 km N.
  - Origins: Castex (-5570, -3030), Monte Nievas (-3050, -1950), General Pico (4110, 2530).
- **Start town:** Monte Nievas is first in `towns.ron`, so the player arrives there, where the action starts.
- **Rail geometry:** the user chose "diagonal real". Inside a town the track stays axis-aligned along the station. Between towns, straight diagonal segments join the end of one town's track to the start of the next town's track, in the order the line lists them. Line order: Castex, Monte Nievas, General Pico. Each link leaves a town's tip straight on along its track and bends onto the diagonal (and back onto the next town's track) on circular curves of radius 500 m. A curve's tangent length `R·tan(Δ/2)` is capped at a quarter of the tip-to-tip distance, which keeps it within half of the middle straight; a capped curve gets a smaller radius. Curves are drawn as straight pieces of at most 8 m that turn at most 2° each. The curves belong to the link, so the town tracks are never cut short.

## Errand story (user, 2026-10-20; supersedes the errand bullets above where they differ)

- The player arrives **on foot** at Monte Nievas by train. The game starts with the player standing on the platform next to the stopped train.
- A few seconds later the train leaves and does not come back this session ("the train passes once a day").
- The player walks to the plaza and meets **Don Hidalgo**, a static NPC.
- Monte Nievas has no police station, so Hidalgo tells the player to report to **comisario Elías Farach** at the Castex comisaría.
- The tethered horse at the general store belongs to Hidalgo. After the talk, the player may **mount it** (it is released only after talking to Hidalgo) and ride to Castex.
- The minimap marks the destination, pinned to the map edge while the target is off-map.
- New mechanics this needs in PR3:
  - an on-foot player mode with its own speed and sprite
  - mounting: on-foot to rider, and the tethered horse becomes the ridden horse
  - the start-on-platform intro with a one-shot train departure
  - the errand state machine
- PR3 will likely exceed the ~400-line heuristic. Split it into commits (or PRs) along those mechanics.

## Constraints

- `pampa-core` stays dependency-free. Rules live in core with unit tests. Render only draws.
- Everything stays deterministic (seeded). Bad data falls back and warns; it never crashes.
- Every shipped string must pass `font::supports` and the 40-character line limit.
- About 400 authored changed lines per task is a planning heuristic only.

## Delivery strategy

- Three chained PRs, as the user requested: `feat/town-rules` → `feat/rail-line` (branched from town-rules) → `feat/errand` (branched from rail-line).
- Work-unit commits on each branch.
- Push, PR creation and merge are the user's decision.

## Tasks

- [x] **T1 (PR1)** core: big-town rule. Covers two plazas (`TownLayout` plaza becomes primary plus optional secondary), two general stores, `places()` returning both plazas and stores, render plaza drawing and the minimap raster for both. Tests first. Route: delegated (writer trigger: 2+ non-trivial files across core, render and minimap).
- [x] **T2 (PR1)** content: `towns.ron` with the three towns at their geographic origins, `half_extent: 8000`, Monte Nievas first. Update the shipped-file and fallback tests and the `assets/world/README`. Route: delegated, same writer as T1.
- [x] **T3 (PR2)** core: `RailLine` network. Covers the order of towns, diagonal links between town tracks, segment-vs-leveled-box placement validation (the line must not cross another town), and a RON `line` section. Rail collision is not in scope: no track is an obstacle today, in town or not, so riders cross the rails freely. Review finding `R3-diagonal-rail-collision-missing` corrected this scope line. with validation. Route: delegated writer (writer trigger: core, content, game and assets).
- [x] **T4 (PR2)** render: stream inter-town track by chunk (ballast, sleepers, rails at any angle), and draw the line on the minimap. Route: delegated writer, same as T3.
- [ ] **T5 (PR3)** core: `Errand` state machine, giver spot in the plaza, police-station entrance spot, and static NPC obstacles. Route: delegated.
- [ ] **T6 (PR3)** game and render: spawn Don Hidalgo and Elías Farach, talk with F, `Say` lines, minimap target marker pinned to the edge. Route: delegated.

## Acceptance criteria

- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` all pass at each commit.
- The shipped `towns.ron` builds all three towns with no placement warnings, and General Pico (very big, 8×8) has a centered 2×2 central plaza, a station plaza, 4 general stores, 2 police stations and 51 townsfolk.
- (PR2) Every railway town is on the line, and no link crosses another town's leveled band.
- (PR3) Talk near Hidalgo, then reach Elías Farach: the errand completes once, and the marker points the way.

## Checks

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Progress / Evidence

- Branch `feat/town-rules` created from `main` at 37ed1ac.
- RDD is on (default).
- **T1** done in `4926186` `feat(town): scale plazas, stores, police and crowds with town size` (amended from the first two-plaza rule `8a0d3f6`, so the branch holds one T1 commit). Route: delegated writer.
  - Adds `TownSize` (Small, Big, VeryBig), merged central plazas with cut streets, `TownLayout::station_plaza`, `plazas()` and `townsfolk_count()`.
  - `StreetGraph` leaves out intersections and links inside a merged plaza. `places()` lists every plaza, general store and police station. Render and the minimap draw every plaza.
  - Layout hashes of the small towns were pinned before the change, in `small_towns_keep_their_layout_byte_for_byte`. They still match, which covers Monte Nievas 4x3 seed 7 and Castex 5x4 seed 2.
  - RED for the first two-plaza rule: `big_towns_have_two_plazas_and_two_general_stores` panicked with "a big town has a second plaza". The minimap test failed with `left: [219, 204, 167] right: [150, 158, 98]` while only the main plaza was drawn.
  - The tier rework tests were written together with the code, so no separate RED run was observed for them. The pinned small-town hashes guarded the rework throughout.
  - Checks on T1, run with the `towns.ron` then committed: fmt OK, clippy clean, `cargo test --workspace` OK (core 142, render 72, game 43, content 17).
- **T2** done in `c9edb4b` `feat(world): lay out Castex, Monte Nievas and General Pico`. Route: delegated writer.
  - `towns.ron` holds Monte Nievas, Eduardo Castex and General Pico (8x8, seed 13) in the 8000 m world, with no placement warnings. The README documents the geography and the size tiers.
  - Checks: fmt OK, clippy clean, `cargo test --workspace` OK (core 142, render 72, game 43, content 17).
- The acceptance criterion now matches the size tiers (`38df1e4`).
- **PR1 native review:**
  - Assessment: medium risk, `slice_budget_reached`, 13 files, 1655 lines.
  - Consent granted. One lens (reliability). Approved, then acknowledged. Lineage `review-d88fa1eab3068f87`.
  - `.atl/` is excluded locally through `.git/info/exclude`, at the user's choice.
  - Advisory findings (non-blocking):
    - `R3-plaza-span-underflow` (town.rs:802-811): `(n - len) / 2` underflows on a grid thinner than the plaza span. This cannot happen today, because `TownConfig::MIN_BLOCKS = 3` rejects every grid under 3×3 and the largest span is 2. It is worth a guard if that minimum ever drops.
    - `R3-noncentered-big-plaza-untested`: there is no test for big towns with `Block` or `FacingStation` plaza placement.

- **T3 and T4** done on `feat/rail-line`, **uncommitted** at the user's request (the user reviews and commits them). Route: delegated writer.
  - T3: `crates/core/src/rail.rs` adds `RailLine::build(names, towns, bounds)`, `RailLink`, `RailSpan` and `RailLineError` (unknown, duplicate, no railway, not on the line, crosses a town, outside the world; each names the towns). Links join the run-out tip of each town nearest the other town's grid center. A link is checked against every other town's leveled box grown by the rail corridor half width, with the existing `Aabb::segment_overlap`. Bad names or links are left out and reported through `TownReport` as `towns: railway line: ...`. The optional `line` is read in `crates/content/src/towns.rs`; without it the line runs west to east. `towns.ron` ships the explicit line; the README documents it.
  - T4: `crates/render/src/rail.rs` adds `WorldRail`, `RailPlugin`, `LineTrack` and a shared `track_pieces` helper. The town's `railway()` now uses the same helper and the same mesh and materials (`TrackKit`, inserted by the town kit). New terrain chunks get their part of each link as children (ballast, two rails and sleepers placed as on the whole link, so chunks meet without seams), and lose them with the chunk; chunk building keeps its per-frame budget. The minimap paints the links clipped to the shown frame in the track color. `game/src/main.rs` inserts `WorldRail` and adds `RailPlugin`.
  - RED: the 8 core `rail::tests` failed against a stub (`left: [] right: ["Eduardo Castex", "Monte Nievas", "General Pico"]`, and so on). The minimap test failed with link drawing disabled (`left: [227, 211, 173] right: [62, 50, 42]`). The chunk tests failed with track laying disabled (`ChunkCoord { x: 0, y: 0 }: 0 pieces`). The content and game line tests were written together with the code.
  - Checks: fmt OK, clippy clean, `cargo test --workspace` OK (core 150, render 76, game 44, content 18).
- **Rail visuals fix** (uncommitted, part of PR2). Route: delegated writer.
  - Sleepers: `track_pieces` turned them by `-θ` about +Y, which is only square to east-west and north-south track. They now turn by `+θ`, because ground north is Bevy -Z. RED: `sleepers_cross_the_track_square_and_centered_between_the_rails` failed at 30° (`sleeper (-0.5, 0, 0.866) not square to track (0.866, 0, -0.5)`).
  - Curves: `RailLink` now carries `path` (the smoothed centerline) and `curves`. Placement checks the path against the world bounds and the other towns' leveled ground. `RailSpan` measures along the whole path, so sleepers keep one spacing across pieces and chunks. The town track, the chunks and the minimap all draw from it. RED: with a straight-line stub, 5 core tests failed (for example `bends 32.07° at once`). The render continuity test was written with the code.
  - Checks: fmt OK, clippy clean, `cargo test --workspace` OK (core 156, render 78, game 44, content 18).
- **Speed, scale, docs and race fix** (2026-10-20, uncommitted, part of PR2). Route: delegated writer.
  - Speed: `RiderParams::default` gallops at 44 km/h (`44.0 / 3.6` m/s) and walks at 3.0 m/s. The user tuned these values on 2026-10-20, after 40 and then 42 km/h. The gait, bump and cadence thresholds still put top speed in the Gallop band.
  - Scale: 1:5 (see Geography). The shipped line validates with no warnings. Both links keep 500 m curves: tangents of 204 m (Castex–Monte Nievas, 44.4° turn, chord 1822 m) and 177 m (Monte Nievas–General Pico, 39.0° turn, chord 7603 m).
  - Docs: `assets/world/README.md` says the default order sorts by the town center (its `origin`), as `west_to_east` does. The `towns.ron` line comment no longer calls the links straight. Review finding `R3-default-line-order-doc-mismatch` is resolved.
  - Race: `lay_track` runs in `PostUpdate` after the new `ChunkStreaming` set and before transform propagation, so a chunk gets its track the frame it is built. It uses `try_insert`, and spawns pieces only while the chunk still exists (`queue_silenced`). Review finding `R3-lay-track-unordered-vs-chunk-despawn` is resolved.
  - RED with the old `lay_track`: `a_chunk_gets_its_track_the_same_frame_it_is_built` failed (`frame 1: ChunkCoord { x: 2, y: 0 } has no track yet`). `a_chunk_despawned_before_its_track_lands_neither_panics_nor_orphans_pieces` panicked (`Entity despawned`). `a_chunk_dropped_the_frame_after_it_is_built_leaves_no_track_behind` passed before and after; it guards against regressions.
  - Checks: fmt OK, clippy clean, `cargo test --workspace` OK (core 156, render 81, game 44, content 18), `cargo build -p game` OK.

## Next step

T5 (PR3): the `Errand` state machine on `feat/errand`, branched from `feat/rail-line` once the user commits PR2.

## Follow-ups (out of scope)

- PR2 review (lineage `review-292e75db8cb4f043`, approved and acknowledged) left two advisory findings:
  - `R3-line-track-parent-relative-transform`: line track pieces use world coordinates under `ChildOf(chunk)`. This is correct today because chunks spawn with `Transform::default()` (render/terrain.rs:220), but it would break if chunks ever get an offset. Add a `GlobalTransform` test.
  - `R3-diagonal-rail-collision-missing`: the scope line was corrected. Rails as obstacles would be a separate decision.

- Trains running the whole line between towns. Trains stay per town for now.
- A curved transition where the town track meets the diagonal link.
- A middle town whose two neighbors are both nearest the same end of its track gets both links on that tip; the shipped line does not hit this.
- Tests for big and very big towns with `Block` and `FacingStation` plaza placement (review finding `R3-noncentered-big-plaza-untested`).
