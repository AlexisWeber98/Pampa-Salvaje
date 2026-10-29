# Lean tests

Objective: drop or slim the unnecessary tests across the workspace, and stop the test suite from exceeding about 5 GB of RAM. Branch: `chore/lean-tests`, stacked on feat/horse-jump (0d44a16). Delivery: the user owns commits. Test runner: `cargo test`, capped with `systemd-run --user --scope -p MemoryMax=5G` and `--test-threads=2`.

## Specs

- S1. "que los test no tengan demasiada sobreingeniería innecesaria, si algo no puede ocurrir de ningun modo que no se testee, no puede ser que un rtest me cuelgue la PC y me usa más de 5GB de RAM" (user, 2026-10-29).
- S2. "se deben eliminar todas las puerabs innecesarias, no solo de esta feature" (user, 2026-10-29).
- S3. Keep every test that guards real, reachable behaviour or a recorded regression. NaN tests on RON-loaded configs (`TownConfig`, `WorldConfig`, `LagoonConfig`) are reachable: keep them.
- S4. Nothing is deleted until the user approves the list.

## Candidates (read-only audit, 2026-10-29, three Opus explorers)

Categories:
- a: cannot happen
- b: duplicate
- c: over-engineered
- d: heavy
- e: asserts nothing

### Root cause of the RAM peak (corrected by T1's measurement, L3)
- Running the tests is not the cause. Peak RSS per test binary is at most 1.9 GB (render, 12 threads).
- The peak came from compiling: 12 rustc jobs of over 1 GB each, plus about 3 GB of rust-analyzer. `.cargo/config.toml` now caps cargo at `jobs = 4`.
- The shared fixture caches (T1) still help:
  - game tests run 3× faster (16 s → 5 s);
  - peak RSS at 12 threads drops by about 40 % in core and game.

### core (59)
- **Delete:**
  - `estancias_keep_half_and_chacras_seven_tenths_of_their_trees` (b; covered by settlement.rs:2617)
  - `planting_the_whole_world_stays_quick` (timing; b with flora.rs:2724)
  - `non_finite_input_counts_as_no_input` (a)
  - `a_non_finite_streamed_footprint_is_dropped_and_the_rest_kept` (a)
- **Merge:**
  - fence.rs:1631 determinism into :1213
  - rider.rs:851 into :858
  - town.rs:4264 into :4216
  - settlement.rs:2161 and flora.rs:2946 onto the shared `shipped()` cache
- **Shrink to small fixtures (d):**
  - fence.rs: :1134, :1433, :1459, :1547, :1594, :1753
  - settlement.rs: :2218, :2248
  - flora.rs: :3392, :3406
  - fauna.rs: :425 (reuse a single world)
  - world.rs: :834, :893, :931, :960 (roads only, no flora)
  - road.rs: :1894
  - lamps.rs: :952
- **Shrink loops (c):**
  - fence.rs: :1391 (use `near`), :1710 (11.5M-cell raster window)
  - settlement.rs: :1838
  - fauna.rs: :543 (radius 2–3), :587 (300 steps)
  - flora.rs: :2577, :2670, :2990, :2708, :1954, :2028, :2683
  - terrain.rs: :312, :341
  - rider.rs: :858 (use `near`)
  - obstacles.rs: :398 (10k boxes, no timing)
  - train.rs: :1644 (dt 0.2)
  - pose.rs: :327
- **Drop impossible NaN or infinity lines (a):**
  - road.rs: :1779
  - rider.rs: :663
  - train.rs: :1761, :1807, :1888
  - bumps.rs: :229, :145
  - clock.rs: :224
  - townsfolk.rs: :485
  - pose.rs: :414, :396, :454
- Also drop the timing asserts inside `fenced()` (fence.rs:1057–1058) and `settled()` (settlement.rs:1796).

### render (29)
- **Delete:**
  - rig.rs:651 (b; the pure `view_eye` test covers it)
- **Merge:**
  - model.rs:354 and :387 into townsfolk.rs:332
  - rig.rs:858 into :842
  - rider.rs:1515 into :1563
  - rider.rs:1495 with tack.rs:1685 (one seated app)
  - horse.rs jump group :1160, :1221, :1265, :1306, :1367, :1398 into one jump test with one app
- **Shrink:**
  - rig.rs: :423 (drop the sweep), :993 (300 frames), :1042 (1 kind, 80 frames), :1121, :1169 (22 models → 5 looks + 1 duplicate; the heaviest), :1375
  - model.rs: :401 (read triangle counts from the glTF JSON, no asset server)
  - townsfolk.rs: :374 (parse JSON)
  - horse.rs: :937 (10 speeds), :1582, :1662, :1766
  - rider.rs: :1880 (60 idle frames), :1980
  - tack.rs: :1708 (force each idle; up to 6000 frames now)
  - viewer.rs: :651 (drop the print-only tail)
- **Check (low confidence):** gltf.rs:339 vs settlement/tests.rs:930 and :814 (props loaded twice).

### game (34 rows)
- **Delete:**
  - towns.rs: :644 (a; whole-world planting the game never does), :1794 (a), :1418 (b; covered by core rail.rs:1967), :1628 (b)
  - camera.rs: :1069 (b), :1094 (b)
  - first_person.rs: :926 (allocator benchmark; its `#[global_allocator]` taxes every game test)
  - orbit.rs: :362 (a)
  - errand.rs: :1116 (e)
- **Merge:**
  - towns.rs: :587 + :961, :1290 with :1134, :1670 with :1214
  - settlement_tests.rs:32 into main.rs:396
  - plant_collision.rs:311 with main.rs:447
  - menu.rs:876 into :812
- **Shrink:**
  - towns.rs: :689 (crossing counts only; the biggest single peak), :1214
  - main.rs: :643 (one `lay_out`)
  - plant_collision.rs: :240 (hand-placed tree)
  - fauna.rs: :679 (reuse the site)
  - console.rs: :892, :906, :1085, :1135, :1178, :1230 (MinimalPlugins, no world)
  - timetable.rs: :455, :474, :489
  - life.rs: :839
  - ride.rs: :1100, :1064
  - camera.rs: :1555 (300 frames), :1585 (12 angles), :2046 (drop the app half)
  - first_person.rs: :754, :843
- **Kept on purpose:**
  - the errand and ride flows
  - `goto_finds_clear_dry_ground_at_every_named_place`
  - main.rs:494 and main.rs:574
  - towns.rs:1402 (regression)
  - towns.rs:1516
  - menu.rs:883

### Not candidates
- `println!` next to real asserts is a repo convention.
- Ignored preview and bench tests are developer tools.

## Tasks

- T1 [S1] Cache the shared shipped-world fixtures (core `fenced`, `settled`; game `lay_out`, `load_world`). Measure the suite's peak RSS before and after. Status: done (L4). Uncommitted.
- T2 [S1,S2] core: deletes, merges and shrinks above. Status: done (L5). Uncommitted.
- T3 [S1,S2] render: deletes, merges and shrinks above. Status: in progress (Opus writer).
- T4 [S1,S2] game: deletes, merges and shrinks above. Status: approved, open.

Route: one Opus writer at a time, in order T1 → T4. After each task the writer runs the crate's tests capped at 5 GB. A deleted test needs no sabotage; a shrunk test is sabotaged once.

## Log

- L1 (2026-10-29) user: "por cierto, se deben eliminar todas las puerabs innecesarias, no solo de esta feature". Three read-only Opus audits produced the lists above.
- L2 (2026-10-29) The user picked "Todo el plan" (all of T1–T4) and committed the horse-jump follow-ups as b740590 and 0d44a16.
- L3 (2026-10-29) The T1 baseline disproved the audits' hypothesis: no test binary nears 5 GB. Peak RSS at 2 / 12 threads:
  - core: 92 / 338 MB
  - render: 870 MB / 1.9 GB
  - game: 135 / 362 MB

  The peak more likely comes from compiling: 12 rustc jobs of over 1 GB each, plus about 3 GB of rust-analyzer. The user approved ("dale") capping cargo at `jobs = 4` in `.cargo/config.toml`. The fixture caches stay, for test time.
- L4 (2026-10-29) T1 done.
  - `LazyLock` fixtures: core `fenced()`, `settled()` and fauna `site()` return `&'static`; game has `shipped_world::{loaded, laid}`. Tests that mutate the world clone it (≈9 ms, against 675 ms for a fresh `lay_out`).
  - Peak RSS at 12 threads: core 338 → 186 MB, game 362 → 219 MB. Game wall time 6.0 → 3.2 s at 12 threads.
  - Checks: fmt and clippy clean; core 470, render 558, game 212 pass. Parent spot check: game 212 pass in 5.2 s.
- L5 (2026-10-29) T2 done.
  - core: 470 → 464 tests, 57.4 → 42.6 s at 2 threads, RSS 109 → 108 MB.
  - Deleted 4 tests, merged 4, shrank 7. Dropped the impossible NaN/inf lines (plus pose `cadence(NaN)`).
  - Kept, with measured reasons: most "shrink" entries now cost ≤0.3 s thanks to T1. flora :2670 is the only `all_plants` coverage.
  - 9 sabotages, all failed as expected. fmt and clippy clean. Parent spot check: 464 pass in 42.5 s.
  - Heaviest left, not on the list, would need approval: fence `no_fence_crosses_a_road…` 15.6 s, flora `escaped_town_trees…` 11.7 s, fence `fences_keep_out…` 9.6 s, flora `shrubs_keep_off…` 7.8 s, lamps `plaza_furniture…` 6.3 s, terrain `the_cut_never_reaches…` 4.9 s.
