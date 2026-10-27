# Branch shuttle, a better train and better track

## Objective (user, 2026-10-27)

The next step declared in `odd/tasks/caleufu.md`: a shuttle that runs back
and forth on the Metileo–Caleufú branch, with a bay track at Metileo, and
a timetable that meets the line train there in both directions. While at
it, make the train look good (it still looks boxy and procedural) and make
the track match the rest of the world (PBR textures, real shapes). The user
allows downloading packages, models and textures (each download needs the
user's permission; see the asset rules in `CREDITS.md`).

## Problem

- Every train system is written for one train, the render resource
  `WorldTrain(Option<LineTrain>)` (render `train.rs:41`): spawning, placing
  and animating the cars, the minimap, the clock and obstacles (game
  `life.rs`), boarding and riding (game `ride.rs`, core `ride.rs`), the
  onboard camera.
- `LineTrain::new` hardwires `RailLine::full_path`, and `Timetable` holds
  only `depart_castex` / `depart_pico`, one run each way per day.
- Metileo has one station track, where the line train stops. Without a
  second track there, the shuttle and the line train would overlap.
- The train is built from cubes and cylinders with flat colours
  (render `train.rs`, about 1030 production lines). The track is stretched
  unit cubes with flat colours and no UVs (render `rail.rs`).

## Decisions

- **Train model**: a downloaded glTF (user choice, 2026-10-27), CC0 or
  CC-BY only, credited in the root `CREDITS.md` and next to its files. It
  must keep what the procedural train does: spinning wheels, moving side
  rods, the chimney smoke anchor, and the hollow coach the player rides in
  (core `CoachBody`, `WIDTH` 3.0, `RAIL_TOP` 0.5). Candidates are being
  researched; the choice goes to the user before any download.
- **Order**: shuttle first (T1–T3), then the track look (T4), then the
  train model (T5), then docs (T6).
- **Commits**: the user makes every commit (memory `user-owns-commits`);
  workers leave changes uncommitted. Delivery slicing is the user's choice.
- **T1 API (core `train.rs`)**: `Timetable { down, up: Vec<TimeOfDay>,
  dwell, top_speed, acceleration }` (no longer `Copy`; `down` leaves the
  path's first station, `up` its last; `departures(run)`, `departure(run)`
  is the earliest, `morning_run()`); `TimetableError::{SameDeparture,
  NoDeparture}`. `LineTrain::on_path(path, served, towns, cars, timetable)`
  builds a train on any path (`new` delegates, same behaviour);
  `LineTrain::branch_shuttle(&line_train, path, &[junction, end], towns)`;
  `shuttle_timetable(&line_train, junction_town, up_run_secs)` and
  `SHUTTLE_MARGIN` (10 min since T2) derive the departures (up from Caleufu, down
  from Metileo, one pair per line stop at the junction, so two round trips
  by default); `schedule_fits()`; `branch_consist()` (loco, coach, boxcar).
  `arrival_time_at` / `stop_at` still use the earliest departure of a run.
  The shuttle stops on Metileo's one station track until T2.
- **T1 tests (all in `line_tests`)**: `a_shuttle_on_the_branch_stops_at_both_ends_and_runs_both_ways`,
  `the_shuttle_meets_the_line_train_at_the_junction_both_ways`,
  `the_shuttle_still_meets_the_line_train_when_the_line_timetable_shifts`,
  `no_shuttle_is_derived_for_a_town_the_line_does_not_stop_at`,
  `a_train_with_two_round_trips_is_where_the_timetable_puts_it`,
  `a_timetable_needs_a_departure_from_each_end_and_none_at_the_same_time`,
  `a_schedule_that_leaves_before_the_run_is_over_does_not_fit`,
  `on_path_refuses_a_bad_timetable_where_new_replaces_it`. Written after
  the implementation (no RED run); each proven by sabotage instead.

- **T2 bay (core `rail.rs`)**: `RailLine::bays: Vec<RailBay>`, derived
  per branch from its first link, not in `towns.ron`; `bay_of(branch)`,
  `shuttle_paths(&towns)` (bay buffer stop, turnout, the station track's
  run-out, the branch), `track_paths` now ends with each bay's path.
  Geometry at Metileo: track at y = -436 (tips x -196 and 772), platform
  x 258..318, so the bay lies on the NORTH side (away from the town: the
  south side holds the platform, windmill and shed), y = -431.5, 4.5 m
  centre to centre (cars 3 m wide, 1.5 m gap). Buffer stop at x = 258
  (the platform's west end), 50 m straight (2 m gap, 38 m consist, 10 m
  clear), then an S-turnout of two 500 m arcs (about 5.4 each, 95 m long)
  onto the straight run-out at x = 113, 309 m short of the junction tip.
  The shuttle's coaches stop at x = 237, level with the platform's west
  end, a step across the line track from the platform; the line train's
  tail (93 m long, coaches' middle at the station) stands up to 65.5 m
  from the station centre on the side it came from, level with the bay,
  1.5 m off the shuttle's side. The bay is a `RailLink` (from = to = junction), so
  `segments`/`segments_in` (render chunks, minimap, flora, fences,
  lagoons, settlements) see it with no change. `SHUTTLE_MARGIN` went from
  5 to 10 game minutes: at 5 the shuttle followed the line train 2 m
  behind its tail on the shared run-out. Metileo's rural road now also
  crosses the bay (a second level crossing). `LineTrain::on_path_stopping`
  and `branch_shuttle(.., stops, ..)` take the bay's `stop`. Render draws
  a three-timber buffer stop (`buffer_stop`) with the existing kit.
  Reachability: the bay is across the line track from the platform, so on
  foot the player crosses the track west of the platform end (or around a
  standing line train's tail); T3 decides boarding.
- **T3 trains (render, game, core `ride.rs`)**: the resource is
  `WorldTrains(pub Vec<LineTrain>)` (renamed: a Vec is no longer "the
  train"; index 0 is the line train, then one shuttle per branch with a
  bay; `.line()`, `.obstacles()`), built by `LineTrain::all_of(line,
  towns, timetable)` (startup and `start_of_day`, which now returns
  `Vec<LineTrain>`; the shuttle is advanced from midnight to the opening's
  time). `TrainCar`, `TrainWheel`, `TrainRod`, `SmokePuff` carry `train`;
  the minimap marks every train; the clock advances all and `moving`
  holds all their cars; the camera occludes with every train's cars.
  `Ride.train` is the ridden train's index (`Ride::new` says 0,
  `on_train(i)`); seat, camera, coach occlusion, announcements and the HUD
  follow it. Boarding rule (`boarding(&[LineTrain], ..)`): the standing
  train with the nearest coach door within `BOARD_RANGE`. A rider must be
  on a platform floor, except for a train standing in a bay
  (`Station::bay`, `LineTrain::stands_in_bay`), boarded from the ground
  beside it (kept simple: a bay platform is a follow-up). Waiting is by
  town: `waiting_town` (no train standing beside the platform; a shuttle in
  its bay does not count) and `wait_for_train(&mut [LineTrain], clock,
  town)` skips to the first arrival of any train there, a bay train
  standing now being waited out (else the line train, due after the
  shuttle, could never be waited for). `depart_now`, `skip_ride`,
  `skip_to_approach` act on the ridden train; `follow_jump` brings the
  others along with the clock. `arrivals_at` / `stops_at` give every
  departure (`arrival_time_at` / `stop_at` stay the earliest). At Caleufu
  the 12 m coach's doors are 4.9 m from the platform's middle spot, just
  out of `BOARD_RANGE`: the player steps along the platform.
- **T3 tests**: core `ride`: `boarding_takes_the_standing_train_beside_the_rider_among_several`,
  `only_a_train_in_a_bay_is_boarded_from_off_the_platform`,
  `waiting_at_medio_takes_whichever_train_comes_first`,
  `waiting_at_norte_takes_the_shuttle_which_alone_serves_it`,
  `a_jump_of_the_ridden_train_leaves_the_others_following_the_clock`,
  `the_shuttles_second_round_trip_is_reached_by_waiting`; core `train`:
  `arrival_times_cover_every_departure_of_a_train_with_several`; render:
  `every_train_gets_its_own_cars`, `each_car_follows_its_own_train`,
  `each_train_rolls_its_own_wheels_and_smokes_for_itself`, minimap
  `marks_the_cars_of_every_train_in_the_shown_town`; game: life
  `the_clock_advances_every_train_and_all_their_cars_are_obstacles`, ride
  `waiting_at_caleufu_boards_the_shuttle_and_it_rides_to_metileo`,
  `the_shuttle_in_metileos_bay_is_boarded_from_beside_it`. No RED run
  (written after the code); each proven by sabotage (boarding ignoring
  the train index, farthest door, bay waiver removed or always on, bay
  train not waited out, bay counted as standing, only the first train
  advanced or waited for, `follow_jump` off, `arrivals_at` first
  departure only, cars/wheels/rods/smoke/spawn/minimap using train 0 only,
  ridden train fixed at 0, `moving` of the line train only).
  Updated on purpose: core `waiting_on_a_platform_lands_on_the_next_train_there`
  (towns, not stations; slices), core ride `boarding` calls (slices),
  game `the_game_starts_aboard_...` (two trains), life
  `moving` expectations (all trains), render/game fixtures
  (`WorldTrains`).

- **T3b bay platform and foot crossing (core `rail.rs`, `ride.rs`,
  `world.rs`, render `rail.rs`, game `ride.rs`, `main.rs`)**: `RailBay`
  now has `platform: Floor` and `walkway: Floor`. Platform at Metileo:
  x 208..258 (buffer stop to the end of the straight), y -427.5..-420.5,
  0.9 m (`Floor::PLATFORM_HEIGHT`), its track edge 4 m from the bay's
  centre, as the station's; clear of the 3 m cars (2.5 m gap), the rural
  road's crossing (x 189.7) and everything built or planted (tested).
  Foot crossing: a boarded timber deck at rail level
  (`RoadNetwork::CROSSING_RISE`), x 211..214, y -438..-427.5, 3 m wide,
  from south of the station track over it and the bay, landing on the
  platform's edge; ramps down at both ends. **It is west of the shuttle
  (4 m past its front), not at the station platform's west end**: the
  line train's 93 m (tail up to 65.5 m out from the station middle on the
  side it came from) stands across any crossing from the platform's west
  end, and the cars are walk obstacles, so such a crossing would be shut
  exactly when changing trains. The walk from the station platform is
  about 45 m along the track side (open ground, y -441) to the deck.
  `World.walkways` (new) holds the deck: part of `ground_height` but not
  a floor, so nobody boards from it; `World::lay_rail(&RailLine)` adds
  the bays' platforms to `floors` and decks to `walkways` (game `lay_out`,
  tests). Boarding: the waiver is gone, a platform floor is required, and
  the bay platform is one. Waiting: `WaitingPlace { town, bay }`;
  `waiting_place(trains, towns, bays, rider)` is the station platform
  (within `WAIT_RANGE` of its middle) or the bay platform (on it); a
  place waits for the trains whose station at that town has the same
  `bay` flag, so Metileo's station platform waits for the line train
  (even with the shuttle in its bay), its bay platform for the shuttle,
  Caleufu's (non-bay) platform for the shuttle; `wait_for_train(.., place)`.
  The T3 bay special case (`wait_until_arrival`) is removed. Alighting
  from a bay train puts the player on the bay platform
  (`Floor::spot_beside`), and `facing_platform(train, towns, floors)`
  seats them by the bay platform's side. Caleufu doors:
  the flip of the consist at the terminal (at half the layover) puts a
  lone coach's doors 3.4 m along from the middle spot in both halves, so
  no stop shift can put a door at the spot in both; `BOARD_RANGE` is now
  5 m (the door is 4.9 m away) and the line train's stops are untouched.
  Render draws the platform (concrete slab and ramps) and the crossing
  (timber planks over two stringers, ramps) per terrain chunk in
  `rail.rs` (`TrackPart::Concrete`; the walkway uses the track timber).
- **T3b tests**: core `rail`: `the_bay_has_a_station_built_platform_along_the_standing_shuttle`,
  `a_boarded_foot_crossing_joins_the_station_side_to_the_bay_platform_clear_of_the_cars`;
  core `world`: `a_foot_crossing_raises_the_ground_to_rail_level_and_is_no_platform`;
  core `ride`: `boarding_takes_the_standing_train_beside_the_rider_among_several`
  (bay platform), `nothing_is_boarded_from_the_ground_not_even_beside_the_bay`,
  `each_platform_at_medio_waits_for_its_own_train`,
  `the_shuttle_is_boarded_from_the_middle_spot_of_its_terminal_platform_all_layover`,
  `a_rider_alights_from_the_shuttle_on_the_bay_platform_at_medios_bay`,
  `a_passenger_on_the_shuttle_faces_the_bay_platform_it_comes_in_to`; render
  `rail`: `the_bay_platform_is_a_concrete_slab_at_station_height_with_a_ramp_at_each_end`,
  `the_foot_crossing_is_boarded_across_the_tracks_at_rail_level`,
  `a_chunk_gets_the_bay_platform_and_crossing_only_where_their_middles_stand`;
  game `towns::caleufu`: `the_bay_platform_and_foot_crossing_keep_clear_of_everything_built_planted_or_driven`,
  `a_walker_crosses_from_the_station_platform_to_the_bay_platform_with_both_trains_in`,
  `the_walker_is_stopped_on_the_track_where_a_standing_train_is_but_not_on_the_crossing`,
  `the_shuttle_is_boarded_from_caleufus_middle_spot_all_through_its_layover`; game
  `ride`: `the_shuttle_in_metileos_bay_is_boarded_from_its_platform_not_from_the_ground`,
  `each_of_metileos_platforms_waits_for_its_own_train`. No RED run (written
  after the code). Each proven by sabotage (platform edge 3 m, crossing
  clear 0 or moved to the station platform's end, deck height, ground
  ignoring walkways, `lay_rail` without floors or walkways, `lay_out`
  without `lay_rail`, boarding waiver back, waiting ignoring the bay flag,
  alighting or facing ignoring the bay, `BOARD_RANGE` 4, ride keys with no
  bays, platform stretched over the road crossing, render chunk without
  the crossing, platform slab low, planks flat on the ground, no
  stringers). Updated on purpose: core `ride` `waiting_*` and
  `facing_platform` calls (new signatures), `branched()` (now also lays the
  bay), game `waiting_at_caleufu...` (boards from the middle spot, no step
  along the platform), render `the_chunk_with_the_bay_gets_its_track_and_buffer_stop`
  (adds the platform and crossing pieces in the chunk). Replaced:
  `only_a_train_in_a_bay_is_boarded_from_off_the_platform`,
  `waiting_at_medio_takes_whichever_train_comes_first`,
  `the_shuttle_in_metileos_bay_is_boarded_from_beside_it` (the waiver is gone).
- **T3c timetable boards (core `board.rs`, `places.rs`, render `signs.rs`,
  `facade/town_buildings.rs`, game `timetable.rs`)**: core
  `station_board(trains, towns, town) -> Vec<BoardRow>` reads every stop
  of every train at a town from the trains (`stop_times` plus each listed
  departure), sorted by time; a row has `service` (line or shuttle),
  `bound` (`Towards(terminal)` or `From(origin)` where the run ends), and
  `arrive`/`depart` (`None` where the run starts or ends). Metileo reads
  (default timetable): `08:41 Ramal de Caleufú  Llega 08:41`, `09:04 Tren a
  Eduardo Castex  Llega 08:51  Sale 09:04`, `09:14 Ramal a Caleufú  Sale
  09:14`, `19:10 Ramal de Caleufú  Llega 19:10`, `19:33 Tren a General
  Pico  Llega 19:20  Sale 19:33`, `19:43 Ramal a Caleufú  Sale 19:43`.
  Boards: `TownLayout::timetable_boards()` gives three facade `Sign`s per
  station, 1.4 m wide, 1.7 m over the floor (platform 0.9 m on the track
  side), each on a pier (the edge between the middle bay and its
  neighbour, `station_bays` shared with render): one on the track-side
  wall facing the platform, two on the street front flanking the entrance.
  Nothing ties them to the bay. Render paints "HORARIOS" (station sign
  style) and tests clearance against the real openings
  (`station_openings`). Game: stand within 3.5 m, in front, looking at a
  board (cos 0.64): prompt "F: horarios"; F opens the panel ("Horarios -
  Estación X", then the rows; the font has no em dash, so a hyphen),
  F again or walking more than 5.5 m off closes it. F rule: the board
  system runs before `ride_keys` and the errand's `interact` and eats the
  press only when it acts; it yields when the errand's person or horse is
  in reach or a horse can be got on, and it wins over boarding or waiting
  for the train because the player faces the wall. `ride_keys` and
  `interact` are now `pub(crate)` for the ordering.
- **T3c tests**: core `board`: `medio_lists_both_line_runs_and_both_shuttle_round_trips`,
  `the_branch_end_lists_only_the_shuttle_and_a_line_station_only_the_line`,
  `rows_are_sorted_and_follow_the_timetables_when_they_shift`; core
  `places`: `every_station_has_three_timetable_boards_one_on_each_wall_side_of_the_entrance`,
  `station_bays_are_odd_between_three_and_nine`; render `signs`:
  `every_station_gets_three_boards_each_clear_of_doors_and_windows`; game
  `timetable`: `a_row_reads_in_spanish_with_what_it_has`,
  `metileos_panel_lists_the_line_train_and_the_shuttle_from_the_trains`,
  `near_the_board_f_opens_the_panel_and_f_again_closes_it`,
  `f_at_a_street_side_board_opens_the_same_panel`,
  `far_away_or_looking_away_f_opens_nothing`, `behind_the_wall_nothing_is_read`,
  `walking_away_closes_the_panel`, `f_is_left_to_a_horse_in_reach_and_to_the_errand`,
  `f_goes_to_the_errand_when_its_person_stands_at_the_board`,
  `the_panel_and_prompt_are_lettered_when_shown`. No RED run (written after
  the code). Each proven by sabotage (bound origin swapped, no sort, line
  train only, departure = arrival, service kind, arrive/depart always
  present, one departure only, run time unscaled; pier 0, wrong facing,
  elevation 0, board height, bay count, a board dropped, street boards
  swapped or on the track side; range, in-front, facing, errand and horse
  checks, keep range, F-again close, panel town, ordering after
  `ride_keys`, display toggle, Spanish strings). Updated on purpose: render
  `spawns_one_board_per_sign_facing_its_street` (three more images).
  Lesson: an out-of-reach F starts waiting for the train and fades the
  screen, so each such game test needs a fresh app.

## Tasks

- [x] T1 Core: a train on any path. `LineTrain` built from a given path and
  its stations (the line keeps `full_path`; the shuttle uses the branch
  path), a `Timetable` with several departures per terminal (the line keeps
  Pico 07:00 and Castex 16:00), a short shuttle consist, and a shuttle
  timetable derived from the line train's Metileo stops so it arrives
  before and leaves after each one, both ways. Route: delegated writer
  (2+ non-trivial core files).
- [x] T2 Core: a bay track at Metileo, a dead-end track beside the station
  track at the junction end, joined to the branch by a turnout. The shuttle
  stops there. Roads, flora, settlements and the render see it. The shuttle
  and the line train never overlap at Metileo at any time of day. Route:
  delegated writer.
- [x] T3 Render and game: several trains. The resource holds every train;
  the cars, wheels, rods and smoke carry their train; the minimap shows
  both; the clock advances every train and joins their obstacles; the
  player boards whichever train stops at their station; clock jumps and
  the camera follow the ridden train. Route: delegated writer.
- [x] T3b The bay is usable: a platform of its own on the bay's far side,
  and a foot crossing over the main track from the station platform, so
  the player changes trains at Metileo without walking on the ballast.
  User decision, 2026-10-27: keep the Metileo–Caleufú shuttle (every
  trip to Caleufú changes trains at Metileo); no through Pico–Caleufú
  train. Route: delegated writer.
- [x] T3c Timetable board: at every station, a board on the station wall
  shows the departures of every train that stops there (at Metileo the
  line train both ways and the shuttle), read with F (`INTERACT_KEY`,
  game `errand.rs:39`). User request, 2026-10-27: also one board either
  side of the street-side entrance (three per station). Route: delegated
  writer.
- [ ] T3d The Caleufú platform faces the Castex platform (user playtest,
  2026-10-27). The bay runs alongside the station platform on the north
  side, with its buffer stop past the platform's east end and its turnout
  west of the line train's tail; the bay platform stands opposite the
  station platform. No standing train may block the way between the
  platforms (user, 2026-10-27): the separate foot crossing goes, and
  people (player and townsfolk) cross at Metileo's rural road level
  crossing (x 189.7), about 33 m beyond the line train's tail (x 222.5),
  with a walked path from each platform's west ramp to the road. The
  standing shuttle must keep clear of that crossing too. Route: delegated
  writer.
- [ ] T4 Track look: profiled rails, textured sleepers and ballast
  (`gravel_road`, `rough_wood`, `rust_coarse_01`, downloaded), laid as
  continuous meshes that follow curves, turnouts and the terrain. Today the
  stretched-cube ballast breaks into tilted, overlapping slabs on the
  turnout and where the branch parts from the line (user playtest,
  2026-10-27). Two crossing signs per foot crossing, one each side, not
  four. Route: delegated writer.
- [ ] T5 Train model: glTF locomotive (and coaches if a fitting model
  exists), downloaded by the user, converted, credited, wired to the
  wheel, rod and smoke animation, and fitted to the ridden coach. Route:
  delegated writer.
- [ ] T6 Docs: this file, `odd/tasks/caleufu.md`, README files, doc
  comments. Last step before the native review.

## T2 tests

- Core `rail`: `a_junction_town_gets_a_bay_beside_its_station_track_at_the_junction_end`,
  `the_bay_joins_the_station_track_by_a_smooth_turnout_within_the_run_out`,
  `the_shuttle_path_runs_unbroken_from_the_buffer_stop_to_the_branch_end`,
  `the_bay_is_track_everything_that_keeps_off_track_sees`; core `train`:
  `the_shuttle_stands_in_medios_bay_beside_the_line_trains_track`.
- Render `rail`: `a_buffer_stop_closes_the_dead_end_of_the_bay_across_both_rails`,
  `the_chunk_with_the_bay_gets_its_track_and_buffer_stop`.
- Game `towns::tests::caleufu`: `metileo_has_a_bay_on_the_far_side_of_its_track_clear_of_the_town`,
  `the_shuttle_stands_in_the_bay_and_never_overlaps_the_line_train_all_day`
  (two game days every 15 s, footprints expanded 0.4 m).
- Updated on purpose: core `the_branch_path_runs_outward_unbroken_and_the_train_keeps_to_the_line`
  (three track paths), core `branched_trains` helper (shuttle on
  `shuttle_paths` with the bay stop), game
  `the_shipped_world_has_its_roads_and_one_level_crossing_by_each_station`
  (one crossing per track at the station: two at Metileo), the
  `branch_shuttle` call in core `train` (new `stops` argument).
- No RED run: tests were written after the code. Each was proven by
  sabotage (break, see it fail, restore): side flipped, buffer moved, bay
  straight shortened, spacing changed, a bay for a town with no branch,
  one-piece turnout, turnout off the track, shuttle path keeping points
  before the join or ignoring the bay, `segments`/`track_paths` without
  bays (core and game tests), stop ignored or wrong, `SHUTTLE_MARGIN` back
  to 5 min, no bay stop (tail on the buffer), buffer stop without its beam
  or in every chunk, buffer stops not laid.

## Checks

- Focused while iterating: `cargo test -p pampa-core`, `-p pampa-render`,
  `-p game` for the touched crate.
- Every new or changed test is proven by breaking the code, seeing it
  fail, and restoring it.
- Once at the end: all four crates' tests, `cargo build -p game`,
  `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, and a playtest
  by the user.

## Progress

- 2026-10-27: exploration done; branch `feat/branch-shuttle` created;
  model research running.

## Next step

T2, delegated writer.
