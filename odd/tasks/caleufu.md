# Caleufú and the Metileo branch

## Objective (user, 2026-10-23)

Add the town of Caleufú and the railway branch that reaches it from
Metileo: the town (station, plaza, comisaría, pulpería, houses) with its
name sign, roads and signposts; the branch track from a junction at
Metileo, with gentle curves, level crossings and the minimap; a ridable
branch shuttle if it can run at reasonable cost; `goto caleufu`; tests;
docs.

## Decisions

- **Position**: `(-7626, 2100)`, as agreed: real Caleufú lies 41 km
  north-west of Eduardo Castex, `(-4250 - 3376, -2721 + 4821)`, 5.89 km at
  1:7. It collides with nothing, so it was not moved. The grid is 774 m
  inside the west edge; the track's west run-out ends 269 m inside it.
- **Size**: small, 4 x 4 blocks of 40 m with 10 m streets, railway on the
  north side, `density: Some(0.6)` (a town eight years old in 1919). The
  existing tier rules give it one plaza with its church, one pulpería and
  one comisaría; the dispersal rule (`disperse` in `core::town`) puts the
  pulpería and the comisaría two blocks apart. 5 x 4 and 4 x 5 grids gave
  the same spread; 5 x 5 put them side by side, so 4 x 4 stayed.
- **Branch model**: `RailBranch` in `RailLine::branches`, built by
  `RailLine::build_with_branches` from `branches` in `towns.ron` (a junction
  town on the line, then the branch towns). Branch links live in
  `RailLine::links` with the line's, so the render, the chunk streaming,
  the minimap, fences, flora, settlements and lagoons lay and keep off the
  branch with no change. `full_path` and `connected_towns` still follow the
  line only, so the line train and its timetable are unchanged.
  `branch_paths` / `track_paths` give each branch as one outward path
  (junction's far tip, its station, the branch, the end town's far tip).
- **Junction**: Metileo's west run-out tip `(-173, -459)`, the same tip the
  line to Monte Nievas leaves by; each track parts on its own 500 m curve
  (the branch turns 21.7° north and back), 7.46 km to Caleufú's east tip.
  Past 400 m from the junction it stays more than 50 m from the line.
- **Roads**: the roads' `Track` now holds every track path (the line,
  then each branch, oriented outward), so side tests, square crossings and
  the 40 m clearance see the branch too. A branch town gets a road to its
  nearest town by grid centre: Caleufú–Eduardo Castex, 5.8 km (Metileo is
  8.3 km). That road crosses the line's run-out west of Castex at a square
  level crossing; it never meets the branch. Caleufú's rural track crosses
  the branch by its station. Signposts at both ends read "CALEUFÚ 5,8 KM"
  and "EDUARDO CASTEX 5,8 KM"; the station sign reads "ESTACIÓN CALEUFÚ".
- **No shuttle yet**, the next step. Two reasons. Every train system is
  written for one train, the `WorldTrain(Option<LineTrain>)` resource:
  drawing and advancing the cars (render `train.rs`), boarding, riding and
  skipping (game `ride.rs`), the onboard camera, the clock jumps (`life.rs`)
  and the minimap. And Metileo has one station track, where the line train
  stops: a shuttle meeting it needs a bay or loop track there, or the two
  trains overlap. `LineTrain::new` would already run a branch path
  (`branch_paths` gives it, through Metileo's platform), so the core is
  ready; the cost lies in the many single-train systems and the yard.
- **Console**: `goto caleufu` (or `caleufú`): place-name matching now
  ignores Spanish accents. The console in this base is still Spanish
  (`goto laguna`, `caballo`, `modelos`); it was not translated here.

## Tasks

- [x] T1 Core branch model: `RailBranch`, `build_with_branches`,
  `BadJunction`, `branch_paths`, `track_paths`; content parses `branches`;
  the game builds with them. Route: delegated writer (this worker).
- [x] T2 Caleufú in `assets/world/towns.ron` and the Metileo branch.
- [x] T3 Roads over every track path, branch-town roads to the nearest
  town; settlements and flora keep off the branch paths.
- [x] T4 `goto caleufu` with accent folding.
- [x] T5 Tests (core, content, game) and deliberate updates of existing
  expectations.
- [x] T6 Docs: this file, README.md, assets/world/README.md.
- [ ] Next: a branch shuttle (see Decisions), with a bay track at Metileo
  and a timetable meeting the line train both ways.

## Tests

- New, core: `a_branch_leaves_the_line_at_its_junction_tip_and_runs_out_to_its_town`,
  `the_branch_path_runs_outward_unbroken_and_the_train_keeps_to_the_line`
  (continuity, bends, determinism), `bad_branches_warn_and_are_left_out`,
  `a_branch_town_gets_a_road_to_its_nearest_town_and_crossings_on_the_branch`.
- New, content: `branches_are_read_and_optional`.
- New, game (`towns::tests::caleufu`): the town's places, placement inside
  the bounds and clear of towns, lagoons, other roads and settlements, the
  branch joining the line at Metileo unbroken to Caleufú's platform, the
  Castex road and every branch crossing, determinism; `goto caleufu` and
  `goto caleufú` in the console goto test.
- Expectations updated on purpose: game
  `the_shipped_file_builds_monte_nievas_first_without_warnings` (five towns,
  the branch link and branch), game
  `the_shipped_world_has_its_roads_and_one_level_crossing_by_each_station`
  (counts rural-track crossings only, since Castex's road to Caleufú adds a
  second crossing at Castex), content
  `the_shipped_file_holds_the_line_its_branch_and_a_lagoon_in_the_largest_world`
  (renamed; five towns, four links, Caleufú's origin).
- RED: observed for `goto caleufu` ("no conozco caleufu") before the
  accent folding. The branch, road and placement tests were written after
  their code; no separate RED for them.

## Verification

Final run (2026-10-24), shipped world loads with no warnings:

- `cargo test -p pampa-core`: 330 passed.
- `cargo test -p pampa-content`: 30 passed.
- `cargo test -p pampa-render`: 229 passed.
- `cargo test -p game`: 111 passed.
- `cargo build -p game`: ok.
- `cargo fmt --all -- --check`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.

Not checked in a window: the junction's look at Metileo, the branch on
the minimap, Caleufú's buildings and signs.

## Next step

The branch shuttle: generalise the single `WorldTrain` into a list of
trains (render, ride, camera, life, minimap), add a bay track at Metileo,
and a timetable meeting the up and down line trains there.
