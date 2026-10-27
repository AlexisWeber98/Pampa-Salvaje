# Todo follow-ups

Branch `feat/todo-followups`, from `main` 0c707c0. The user owns commits:
every task is left uncommitted for the user's review.

## Objective (user, 2026-10-27)

"Hagamos lo que está en el todo.md": close the open items listed in the
root `todo.md`. Field flora comes after, as its own feature.

## Scope

`feat/branch-shuttle` is already merged into `main` (PR #45), so the
todo's merge step and its per-commit review of that branch no longer
apply. What is left:

- the T6 docs of `feat/branch-shuttle`;
- the three review follow-ups of the sky birds;
- boarding at the line's terminals;
- the follow camera stopping in a coupling gap;
- the three `pampa-content` tests failing on `main`;
- leftover files;
- two optional items that need the user's decision.

## Tasks

- [x] T1 Leftovers: delete the per-set `CREDITS.md` files of
  `gravel_road`, `rough_wood` and `rust_coarse_01` once the shared
  `assets/textures/buildings/CREDITS.md` is confirmed to cover them.
  `assets/models/third-party/locomotive_1/` is already gone.
  Route: inline (mechanical deletions).
- [x] T2 `pampa-content` tests: update the General Alvear and Carmensa
  fixtures, the README's Winifreda example and that expectation to the
  10 m minimum street width. Route: delegated writer.
- [x] T3 Sky birds: a soaring bird must not teleport in view on a wrap
  (`birds.rs:198`); birds must hide after dusk even in range
  (`birds.rs:236`); a test exercises the sky material (`sky/mod.rs`).
  Route: delegated writer.
- [x] T4 Boarding at the terminals: at Castex and Monte Nievas, F from the
  platform's stop spot must reach a door during the layover.
  Route: delegated writer.
- [x] T5 Follow camera: on a platform beside a standing train, the camera
  must not settle in the gap between two cars; restore the strict camera
  test relaxed in T5b. Route: delegated writer.
- [x] T6 Docs: tick T6 in `odd/tasks/branch-shuttle.md`, close the "Next
  step" of `odd/tasks/caleufu.md`, update `README.md` (the shuttle, the
  timetable boards, the new train, the "PS2-era" line), doc comments of
  this feature's changes, and `todo.md`. Last, before the native review.
  Route: delegated writer.
- [x] T7 A tank engine (2-6-2T) for the Caleufú shuttle (user,
  2026-10-27: "solo la tanque"; the station canopy stays as it is).
  Route: delegated writer.

## Checks

- Focused tests per task, each new or changed test proven by breaking the
  code, seeing it fail, then restoring it.
- At the end: tests of all four crates, build, clippy, fmt.

## Progress

Created 2026-10-27.

- T1 done: the three per-set credit files are removed (staged). Both
  shared CREDITS files already credit those sets, and nothing referenced
  the removed files.
- T2 done, uncommitted: all three failures were stale data, not bugs.
  - `towns.rs`: the fixtures use 11 and 10 m streets. The shipped-file
    test expects each town's own width (Nievas 12, Castex 12, Pico 13,
    Metileo 12) and the lagoon 3 m east, as `towns.ron` documents.
  - `assets/world/README.md`: the Winifreda example uses 11 m, and the
    documented minimum is 10.
  - Checks: `cargo test -p pampa-content` gives 32 passed (spot-checked by
    the parent); clippy and fmt are clean.
  - Sabotage: Pico's width, the lagoon x, the width mapping and the
    README width each made a test fail. The 12 m expectations for
    Metileo, Nievas and Castex were not sabotaged one by one.

- T3 done, uncommitted (`sky/birds.rs`, `sky/mod.rs`, `odd/tasks/sky.md`).
  - Soarers wrap their own position, not their circle centre.
  - Once the gate closes, a bird in range is hidden after `GATE_GRACE`
    (45 s). It disappears at once; there is no fade.
  - A new test encodes `SkyParams` with encase and checks the bytes
    against the shader's layout.
  - RED: the old code made a bird in view jump 440 m, and the gate test
    failed. The sabotage of each fix and of two uniform fields made the
    tests fail again.
  - Checks: render 467 passed, clippy and fmt clean. The parent
    spot-checked `sky` (37 passed).
- T5 done, uncommitted: `car_occluders` (`game/src/camera.rs`) now makes
  each car cover half of the coupling gap at both ends, so the sight line
  cannot slip between cars. The strict
  `on_the_platform_the_camera_stays_in_front_of_the_dwelling_train`
  assertion is restored (RED at step 0: along -6.5, side 1.30; GREEN after
  the fix; fails again with the fix removed).

- T4 decision (user, 2026-10-27): "Alcance en punta".
  - At both terminals the consist is centred on the station, so the
    nearest door is 7.19 m from the platform's middle spot. Half way
    through the layover the consist turns round, and the doors mirror to
    the other side. No stop position or stop mark can keep a door within
    `BOARD_RANGE` in both halves.
  - The fix: while the train stands at the rider's platform, a rider
    waiting at the stop spot boards by the nearest door within
    `WAIT_RANGE` (8 m). This changes nothing visible.
- T4 done, uncommitted: new `boarding_at` (`crates/core/src/ride.rs`)
  takes the towns; a rider within `STOP_SPOT_RADIUS` (1.5 m) of the
  station platform's stop spot, with a train standing there, reaches a
  door up to `WAIT_RANGE`. `boarding` delegates to it with no towns, so
  `BOARD_RANGE` holds everywhere else (bay, shuttle, mid-platform).
  `game/src/ride.rs` calls `boarding_at`.
  - RED: `the_line_train_is_boarded_from_each_terminals_middle_spot_all_layover`
    failed with the longer reach stubbed out. GREEN with it.
  - Sabotage: reach back to `BOARD_RANGE` fails that test; a 6 m radius
    fails `away_from_the_stop_spot_the_terminal_door_stays_beyond_board_range`.
- T7 done, uncommitted: the shuttle is hauled by a 2-6-2T tank engine
  (`CarKind::TankEngine`, 12 m, top 4.2 m; the line train keeps its
  14 m 2-6-0 with tender).
  - Core: `branch_consist` uses it. The engine is 12 m, not 11: at 11 m
    the coach stood centered on Norte's middle spot and its nearest door
    was 5.03 m away, over `BOARD_RANGE`. Bay and stop tests are green.
  - Render: `train/tank.rs` (boiler, side tanks, cab, rear coal bunker,
    10 wheels, four rods, 12048 triangles) reuses the loco's wheels, rods
    and materials; no new assets.
  - Tests: core `the_shuttle_is_hauled_by_a_tank_engine_...` (RED: no
    variant; sabotage: Locomotive back in the consist), render
    `the_tank_engine_is_a_2_6_2_...` (sabotage: trailing axle, chimney
    height, bunker coal, tanks each made it fail). The look is not yet
    checked in the game.

- T6 done, uncommitted: docs only, no behaviour change. `branch-shuttle.md`
  and `caleufu.md` are closed (shuttle merged as PR #45, now hauled by the
  tank engine). `README.md` describes the branch shuttle, the timetable
  boards, the procedural British-style train (2-6-0 with tender, 2-6-2T on
  the shuttle) and the terminal boarding rule, and its "PS2-era" train line
  is reworded. Doc comments were checked and none were stale. `todo.md` is
  trimmed to what is left.

- Full checks (parent, 2026-10-27): game 169, content 32, core 416,
  render 468 (5 ignored); clippy and fmt clean.
- Native review: granted by the user (medium, 21 files, 1252 lines, the
  reliability lens). Approved and acknowledged; lineage
  `review-3f61be22c76213ae`. Two non-blocking suggestions, left as
  follow-ups:
  - `crates/core/src/ride.rs:671-674`: the negative stop-spot test claims
    the positive case in its closing block but never calls `boarding_at`.
    Another test covers that case.
  - `README.md:137-138` says the 8 m reach is for a terminal's stop spot.
    The code gives it at the stop spot of any non-bay station where a
    train stands. Narrow the code or the README; no test pins it down.

## Next step

The full checks, the native review of the uncommitted work, then the field
flora feature.
