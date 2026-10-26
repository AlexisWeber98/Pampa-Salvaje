# Metileo blocks to 40 m, plus small review follow-ups

## Objective (user, 2026-10-25)

Metileo's 30 m blocks give 6–7.5 m house lots: only 2 bays, and a church
front too narrow for windows beside the tower. The facades writer
recommended at least 8.5 m of frontage (3 bays), i.e. 40 m blocks as in the
other towns. The user approved changing Metileo to 40 m.

## Tasks

- [x] M1 `assets/world/towns.ron`: Metileo `block_size` 30 → 40. Keep the
  town's position. Check that the whole layout still fits and that nothing
  overlaps: the station and halt on the main line, the plaza, roads, level
  crossings, settlements, the Caleufú branch junction at Metileo's west
  run-out tip, the lagoon road. Update the tests that pin Metileo's
  layout, deliberately, and name them.
- [x] M2 Review follow-ups left from the last reviews:
  - `tools/mh2gltf.py` `make_long_skirt`: count the points at the capped
    reach (clamp to the figure's height) before raising ConvertError, plus a
    test for sparse points that only the final reach finds;
  - `tools/test_retarget.py:1126`: fix the comment that contradicts the sign
    of the speed assertion;
  - `crates/render/src/facade/town_buildings.rs:768-775`: keep the station's
    gable eaves inside the footprint, or document the exception and keep the
    signs and colliders true. Extend the footprint test to the station,
    sheds and grain stores;
  - stabilise the flaky render test
    `the_long_gallop_keeps_its_hooves_still_up_to_its_fastest_unhurried_pace`,
    which failed once under full-workspace load.
- [x] M3 Tests (each with its sabotage), docs.

Route: one delegated writer.

## Checks

- `python3 -I -B -m unittest discover -s tools`
- `cargo test -p pampa-core`, `-p pampa-content`, `-p pampa-render`, `-p game`
- `cargo build -p game`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`

## Progress

- 2026-10-25: branch `feat/metileo-blocks` from main daecd9f.
- 2026-10-25 (delegated writer, uncommitted for the user's review):
  - M1: Metileo `block_size` 30 → 40 at the same origin; the grid grows
    from 122 to 152 m, the track 15 m north and 30 m longer. The shipped
    world loads with no warnings; no grid tweak or nudge was needed.
    Its houses are now 8.06 m or wider and 4 of 6 fit three bays (before: 6.0–7.2 m, none);
    church front 12 m (9 m before). Line 10 013.8 → 10 025.1 m, so the
    timetable moved: Metileo up 08:51:31–09:04:01, Monte Nievas
    10:53:59–11:06:29, Castex 12:25:21; down Metileo 19:20:17–19:32:47,
    Pico 21:25:21; the opening starts at 10:43:59 (was 10:43:40).
    README timetable and opening updated; `game/src/life.rs` comment too.
  - Pinned tests: `pampa-content` `the_shipped_file_holds_the_line_its_branch_and_a_lagoon_in_the_largest_world`
    now pins Metileo's `(block_size, street_width) == (40, 8)`; new game
    tests `metileo_has_lots_wide_enough_for_three_bay_houses_and_a_windowed_church`
    and `metileo_on_40_m_blocks_keeps_clear_of_its_neighbours_roads_and_settlements`
    (grid 152 m at its origin; no building on another, the plaza or a
    street; 500 m clear of other towns and the lagoon; no other town's
    road within 100 m of its leveled ground; no settlement on it). The
    existing junction, crossing, platform and lamp tests pass unchanged.
  - M2 skirt: `make_long_skirt` caps the reach at the figure's height and
    counts there before refusing. Test
    `test_a_skirt_counts_the_points_at_its_capped_reach_before_refusing`.
  - M2 retarget: the `test_the_target_side_speed_is_a_planted_foots_slide`
    comment now matches the sign (a foot carried forward slides back at
    -0.9 m/s). Comment only.
  - M2 eaves: the station's gable eaves overhang 0.05 m (was 0.5 m), ending
    just inside the footprint (the 0.08 m slab tilts ~3.6 cm out). Sheds and
    grain stores stand their iron walls `BARN_REACH` (0.45 m) in from the
    lot so the barn's 0.4 m eaves end on it; settlement barns unchanged.
    `masonry_fronts_stay_inside_their_footprints` now also covers the
    station (its platform canopy exempted to core's canopy box), yard and
    outskirts sheds, and both grain stores.
  - M2 gallop: the flake was a load-dependent start phase. `saddled_every`
    waits in real time (5 ms sleeps) for assets, so the number of frames
    the clips had played before the measure varied with load, and the
    300-frame median slide with it (24 concurrent runs: 8 m/s slide
    -0.26 to -0.36, natural 7.40–7.63). The test now restarts the clips at
    their seeded points (`from_the_seeded_start`) before its 100 warm-up
    frames; 24 concurrent runs then give identical numbers (8: -0.27,
    12.2: -0.34, 19.4: +4.45; natural 7.55/7.50/7.48). Assertions unchanged.

## Sabotage log (2026-10-25)

| Test | Sabotage | Seen | Restored |
| --- | --- | --- | --- |
| `metileo_has_lots_wide_enough_…` | `block_size` back to 30 | FAILED: houses 6.0–7.2 m | yes, `git diff` shows 40 |
| `metileo_on_40_m_blocks_keeps_clear_…` | `block_size` back to 30 | FAILED: grid 122 ≠ 152 | yes |
| content shipped-file test | `block_size` back to 30 | FAILED: `(30.0, 8.0)` | yes |
| `test_a_skirt_counts_…capped_reach…` | old loop (raise before counting at the cap) | ERROR: fewer than 8 body points (RED before the fix) | fix applied |
| `masonry_fronts_stay_inside_…` (station) | old 0.5 m overhang; then `PROUD` | FAILED: eave at z 4.94, then 4.54 > 4.5 (RED) | fix at 0.05 |
| `masonry_fronts_stay_inside_…` (sheds) | barn at the full lot size | FAILED: Shed OldIron at z 5.42 > 5 (RED) | `BARN_REACH` inset |
| `the_long_gallop_…` | without `from_the_seeded_start` | 24 concurrent runs: 4 distinct results per speed | helper restored |
| `the_long_gallop_…` | `GALLOP_SPEED` 7.3 → 6.5 | FAILED: natural 7.41 | yes, no diff on the line |

## Verification (2026-10-25)

- `python3 -I -B -m unittest discover -s tools`: 140 tests OK
- `cargo test -p pampa-core`: 345 passed
- `cargo test -p pampa-content`: 32 passed
- `cargo test -p pampa-render`: 320 passed, 3 ignored
- `cargo test -p game`: 120 passed
- `cargo build -p game`: ok, no warnings
- `cargo fmt --all -- --check`: clean
- `cargo clippy --workspace --all-targets -- -D warnings`: clean

## Open

None. The first two items were closed before phase 2 of the town facades,
the third in it (checked 2026-10-25):

- `crates/core/src/lamps.rs`: the `metileo()` fixture now uses
  `block_size: 40.0`, as `assets/world/towns.ron` ships it.
- README lamp counts now read Eduardo Castex 25 and Caleufú 21, matching
  `every_town_and_estancia_is_lit_by_lamps_standing_clear_and_solid`.
- The church-front assertion in `game/src/towns.rs` now checks the 12 m its
  comment states (it checked 11 m).
