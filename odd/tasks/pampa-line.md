# Feature: pampa-line

Locator: `odd/tasks/pampa-line.md` · Engram mirror: `odd/pampa-line/tasks`

## Objective

Lay out the first real stretch of La Pampa: three towns at their real relative positions (Eduardo Castex, Monte Nievas, General Pico), joined by one railway line, plus a first errand. In the errand, Don Hidalgo in the Monte Nievas plaza sends the player to comisario Farachelías at the Castex police station, and the minimap marks the destination.

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
   - Comisario Farachelías stands at the Castex police station.
   - Talking to Hidalgo starts the errand; reaching Farachelías completes it.
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
- **Geography:** real offsets scaled 1:4 and fit inside the ±8000 m world (`half_extent: 8000`).
  - Castex to Monte Nievas: about 12.6 km E and 5.4 km N.
  - Monte Nievas to General Pico: about 35.8 km E and 22.4 km N.
  - Origins: Castex (-6200, -3300), Monte Nievas (-3050, -1950), General Pico (5900, 3650).
- **Start town:** Monte Nievas is first in `towns.ron`, so the player arrives there, where the action starts.
- **Rail geometry:** the user chose "diagonal real". Inside a town the track stays axis-aligned along the station. Between towns, straight diagonal segments join the end of one town's track to the start of the next town's track, in the order the line lists them. Line order: Castex, Monte Nievas, General Pico.

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
- [ ] **T3 (PR2)** core: `RailLine` network. Covers the order of towns, diagonal links between town tracks, segment-vs-leveled-box placement validation (the line must not cross another town), rail collision for diagonal segments, and a RON `line` section with validation. Route: delegated.
- [ ] **T4 (PR2)** render: stream inter-town track by chunk (ballast, sleepers, rails at any angle), and draw the line on the minimap. Route: delegated.
- [ ] **T5 (PR3)** core: `Errand` state machine, giver spot in the plaza, police-station entrance spot, and static NPC obstacles. Route: delegated.
- [ ] **T6 (PR3)** game and render: spawn Don Hidalgo and Farachelías, talk with F, `Say` lines, minimap target marker pinned to the edge. Route: delegated.

## Acceptance criteria

- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` all pass at each commit.
- The shipped `towns.ron` builds all three towns with no placement warnings, and General Pico has two plazas and two general stores.
- (PR2) Every railway town is on the line, and no link crosses another town's leveled band.
- (PR3) Talk near Hidalgo, then reach Farachelías: the errand completes once, and the marker points the way.

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
- The acceptance criterion "General Pico has two plazas and two general stores" predates the tiered rule. Under it, Pico has two plazas, four general stores and two police stations, and the tests assert that.

## Next step

T3 (PR2): the `RailLine` network on `feat/rail-line`, branched from `feat/town-rules`.

## Follow-ups (out of scope)

- Trains running the whole line between towns. Trains stay per town for now.
- A curved transition where the town track meets the diagonal link.
