# Feature: line-train

Locator: `odd/tasks/line-train.md` · Engram mirror: `odd/line-train/tasks`

## Objective

Add a game day and one passenger train that runs the whole Pampa line (Castex, Monte Nievas, General Pico) on a timetable. The player can board it and travel to the next town with a free camera, skip the trip, or wait for the train on a platform. The minimap gets a compass.

## Problem / Why

- Today each town has its own short-lived train. It crosses only that town's track every 180 s at 10 m/s, and nothing travels between towns.
- There is no game clock, so "the train passes twice a day" cannot exist.
- The player has no sense of heading on the minimap.

## Rules chosen (user-approved 2026-10-20)

- **Clock.** One game day lasts 48 real minutes, so the base rate is 30× (1 game minute = 2 real seconds), as in RDR2 and GTA V.
- **Clock on board.** While the player rides the train, the clock runs at about 12×, so the full Castex to Pico ride (about 11 real minutes) reads as about 2 game hours.
- **Timetable.** One train runs Castex → Monte Nievas → General Pico, lays over in Pico, and returns Pico → Monte Nievas → Castex, then spends the night in Castex. Each town sees the train twice a day. Default times: depart Castex 07:00, depart Pico 16:00. These are tunable.
- **Train.** The top speed is 65 km/h (65 / 3.6 m/s), with acceleration and braking at stations and about a 25 s dwell at Monte Nievas. The consist is a rural mixed train: the locomotive, 2 passenger coaches and 5 freight cars (8 cars). Its cars stay moving obstacles.
- **Train motion.** The train moves physically, in real seconds. The clock only triggers departures. Clock jumps (waiting or skipping) fast-forward the train to match.
- **Riding (PR4).** Riding is not playable. The camera is free to orbit, and the player can skip the trip: a fade, the clock adds the remaining nominal time, and the player gets off at the next station.
- **Waiting (PR4).** On a platform, the player can wait for the train. The clock fast-forwards until the next train is at that station.
- **Compass (PR4).** The minimap shows N/S/E/W, so the player knows their heading.
- **View range.** It stays as is. Terrain streams 3 chunks (about 700 m), towns activate at 600 m, and fog is at 180 m. 1 km would raise the load, not lower it.

## Scope and delivery

Three PRs. The user commits and pushes; Claude never commits.

1. **PR3 `feat/line-train`:** the clock, the sun following the clock, and the one line train with its timetable, which replaces the per-town trains.
2. **PR4 `feat/errand`:** the errand (see `odd/tasks/pampa-line.md`, "Errand story"). The intro becomes getting off the morning train at Monte Nievas, on foot. It also brings the HUD clock and the 2-coach rural consist.
3. **PR5:** boarding, riding with a free camera, skipping the trip, waiting on the platform, the minimap compass, and the terminal-turnaround fix.

**Order change (user-approved, 2026-10-20):** the errand comes before train boarding. Boarding, waiting, skipping, the compass and the terminal-turnaround fix move to the next PR.

## Constraints

- `pampa-core` stays dependency-free. Rules are pure and tested.
- Everything is deterministic.
- About 400 lines per task is advisory only.

## Tasks

- [x] **T1 (PR3)** core: `GameClock`. It has a day length, a base rate, a rate multiplier for riding, `advance_by` and `advance_until`, and formatting as HH:MM. Render: the sun direction, colour and intensity follow the hour, and nights stay readable. Route: delegated writer (trigger: 2+ non-trivial files across core, game and render).
- [x] **T2 (PR3)** core: `LineTrain`. It runs the full line path (town tracks plus curved links), with a timetable, a 65 km/h cruise speed, station stops, a terminal layover, extra cars, and a pure `state_at` so time jumps are exact. Game and render: replace the per-town trains with the line train, and stream its cars by proximity. Route: delegated writer (trigger: 2+ non-trivial files across core, game and render).
- [ ] **T3 (PR5)** boarding, riding, skipping and waiting, and the terminal-turnaround fix.
- [ ] **T4 (PR5)** minimap compass.
- [x] **T5 (PR4)** the errand, done on `feat/errand` as `odd/tasks/pampa-line.md` T5/T6, uncommitted (the user commits). Route: delegated writer. It also adds the HUD clock and the 2-coach consist.

## Acceptance criteria (PR3)

- The train leaves Castex at 07:00, stops at Monte Nievas, reaches Pico, waits, leaves at 16:00 and returns. Tests assert the order and the dwells.
- The top speed never exceeds 65 km/h.
- A clock jump puts the train exactly where continuous simulation would.
- fmt, clippy and the whole workspace's tests are green.

## Checks

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Progress / Evidence

- Branch `feat/line-train` from `main` at f462613. PR1 and PR2 are merged.
- T1 and T2 are done in the working tree, uncommitted (the user commits).
- **Clock** (`crates/core/src/clock.rs`): `GameClock` (day, time of day in game seconds, `multiplier`), `TimeOfDay`, `BASE_RATE` = 30, `DAY_REAL_SECONDS` = 2880. `tick`, `advance_by`, `advance_until` (next occurrence, zero if already there), `hour`, `hh_mm`.
- **Sun** (`crates/render/src/atmosphere.rs`): `sky_at(hour)` is pure and tested. Sunrise is 06:30 and sunset 19:30, rising in the east, crossing north (Bevy -Z) and setting in the west. The light is warm when low and 10 000 lux at full day. At night a bluish moon (up to 900 lux) and an ambient of at least 180 take over. The fog and clear color are tinted by the hour, and `FOG_VISIBILITY` (180 m) is unchanged. The `WorldClock` resource drives it.
- **Line path** (`RailLine::full_path`, `LinePath`): Castex's west tip, the links, then Pico's east tip, 12 805.5 m in all, measured by arc length.
- **Line train** (`LineTrain` in `crates/core/src/train.rs`):
  - Consist (PR3): the locomotive, 3 passenger coaches, then the old 4 freight cars, 8 cars and 95 m. Superseded in PR4 by the 2-coach rural consist below.
  - It stops with the coaches' middle at each station's projection.
  - Its motion is a trapezoid profile: 65 km/h top speed, 0.5 m/s², 25 s dwell at Monte Nievas.
  - `state_at(run, real_s)` is pure. `advance(from, game_s, rate)` drives it: departures come from the clock, running is in real seconds, and a jump uses `BASE_RATE`.
  - It turns around half way through each layover.
  - Helpers for PR4: `next_departure_after`, `arrival_time_at`, `first_stop_at`, `time_until_at`, `next_stop`.
- **Timetable numbers** (real seconds; clock times at the base rate):
  - (At the 1:5 scale of PR3; see the 1:6 rescale below for today's numbers.) Down: Castex 07:00 → Monte Nievas arrives at 195.9 s (08:37) and leaves at 220.9 s → Pico at 743.6 s (13:11).
  - Up: Pico 16:00 → Monte Nievas at 522.7 s (20:21) → Castex at 743.6 s (22:11).
  - Each run lasts about 12.4 real minutes.
- **Start of day:** the clock starts at `first_stop_at(Monte Nievas)` (08:44:10 at 1:5, 08:33:02 at 1:6). The train is jumped there from its overnight layover at midnight, so it stands at Monte Nievas, where the player starts.
- **Game:** `run_clock_and_train` ticks `WorldClock` and advances `WorldTrain`. The 8 cars are always moving obstacles; on a slant, each car is a row of short boxes. The per-town `WorldTrains` is gone.
- **Render:** one persistent set of car entities, not parented to any town. A car is visible only within 400 m of `StreamFocus`. Yaw is `Quat::from_rotation_y(heading)`, pitched with the ground. There is a new coach mesh. The minimap draws the line train's cars inside the shown town's frame.
- **Legacy removed (coordinator-approved):** the per-town `Train`, `TrainSchedule`, `TrainPhase` and `TrainState` are gone from core, along with their re-exports and tests. The `game/src/towns.rs` test now asserts that the station lies within its railway's span. A new core test checks that a rider never ends inside a car on a slanted link.
- **Checks:** RED was observed for the 11 `LineTrain` tests (they panicked on `todo!()` stubs). The clock, line-path and sky tests were written alongside their code. fmt, clippy (`-D warnings`), `cargo test --workspace` (core 168, render 89, game 46, content 18) and `cargo build -p game` are green.
- **Rider tuning (user, 2026-10-20):** `walk_max` is 3.0 m/s and `gallop_max` is 44 km/h (`44.0 / 3.6`). The Gallop band starts above `walk_max + 0.1`, so top speed still counts as a gallop.
- **PR3 review** (lineage `review-99c315384c06a404`): approved and acknowledged.
  - `R3-001` was fixed afterwards. `run_clock_and_train` now caps the frame delta at `MAX_STEP`, as the rider and the old per-town trains did, so a frame hitch cannot jump the train onto a rider.
    - RED: `a_frame_hitch_advances_the_clock_and_train_by_one_capped_step` failed with "advanced 7.5 game s, expected 1.5". Bevy already clamps a frame to 0.25 s.
    - GREEN: game tests 47.
  - `R3-002` is a follow-up. A station within one consist length (95 m then, 93 m now) of a line end would stop with its coaches off the platform, and stations very close together could give zero-length legs. The shipped line is unaffected. Add a `LinePath` test with a station near a tip.

- **2-coach rural consist (PR4, `feat/errand`).** The consist is now the locomotive, 2 coaches and 5 freight cars (the third coach became a boxcar): 8 cars and 93 m. The coaches' middle (the stop offset) is 27.5 m from the front, and it still stops at each station's projection (`stops_put_the_coaches_exactly_at_each_station_projection`). Every stop moves by the same offset, so the timetable did not change with the consist (at 1:5 it stayed Monte Nievas at 195.9 s and 547.7 s, Pico at 743.6 s, start 08:44:10). RED: `stations_are_in_line_order_with_the_coaches_alongside` failed with `left: 3, right: 2`.
- **World rescaled 1:5 → 1:6 (PR4, user-approved 2026-10-20).** Origins: Monte Nievas (-3050, -1950) unchanged, Castex (-5150, -2850), General Pico (2917, 1783). The line path is now 11 008.0 m (was 12 805.5 m). Re-measured timetable (real seconds after departure, clock at 30×):
  - Down: Castex 07:00 → Monte Nievas arrives at 173.6 s (08:26:47), leaves at 198.6 s (08:39:17) → Pico at 644.0 s (12:22:01).
  - Up: Pico 16:00 → Monte Nievas arrives at 445.5 s (19:42:44), leaves at 470.5 s (19:55:14) → Castex at 644.0 s (21:22:01).
  - Each run lasts about 10.7 real minutes. The day starts at 08:33:02 with the train `Dwelling` at Monte Nievas (station 1), checked by `the_day_starts_with_the_morning_train_at_the_first_town`.
- **HUD clock (PR4).** The game time `HH:MM` from `WorldClock` is shown top left, in the pixel font on a paper panel, with the errand objective under it.

## Next step

T3/T4 (PR5): boarding, riding with a free camera (clock `multiplier` about 0.4), skipping the trip (`next_stop`), waiting on the platform (`time_until_at`), the minimap compass, and the terminal-turnaround fix.
