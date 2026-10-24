# Town population in towns.ron

## Objective (user, 2026-10-24)

Set how many townsfolk walk each town from `assets/world/towns.ron`, and use
it to cut the crowds by about 40%. That frees budget for replacing the
procedural people with third-party 3D models later.

## Today

`TownSize::townsfolk` (`crates/core/src/town.rs:386`) decides the count:
- `TOWNSFOLK_COUNT` (16) for small towns;
- `max(16, 0.8 × blocks)` for big and very big ones (General Pico: 51).

## Tasks

- [x] T1 An optional `townsfolk: Option<usize>` per town:
  - in the RON and the content parser, then in `TownConfig` and the layout;
  - when missing, the current formula applies, so old files keep working;
  - validation: refuse an unreasonable value with a content warning
    (e.g. above a sane cap).
- [x] T2 `assets/world/towns.ron`: set each town to about 60% of today's
  count, rounded, never below a small floor that keeps a town alive. List
  the numbers before and after in this document.
- [x] T3 Tests and docs:
  - the field is read and optional;
  - the override wins and the default formula is unchanged;
  - the shipped file has the new counts;
  - `assets/world/README.md` and the README.

Route: one delegated writer (2+ non-trivial files). The new `TownConfig`
field also needed `townsfolk: None` in seven exhaustive test literals
(core `road.rs`, `rail.rs`, `lamps.rs`; render `train.rs`, `road.rs`,
`rail.rs`) and a `ContentError::Townsfolk` warning in content `lib.rs`;
the user approved those surfaces (2026-10-24).

## Design

- `TownConfig::townsfolk: Option<usize>`, `TownConfig::MAX_TOWNSFOLK = 200`
  and `TownConfig::townsfolk_count()` (override, else the tier formula);
  `TownLayout` stores that count. 0 is allowed (an empty halt).
- Core rejects a value above the cap (`TownConfigError::TooManyTownsfolk`),
  like `police_stations`. The content parser catches it first: it logs
  `ContentError::Townsfolk` and drops the override, so the town is kept
  with its tier's crowd, as a bad `world` section falls back to defaults.

## Before and after

Floor of 6 people; no town hits it.

| Town | Blocks | Before (formula) | After (`townsfolk`) |
|---|---|---|---|
| Monte Nievas | 4 x 3, small | 16 | 10 |
| Eduardo Castex | 5 x 4, small | 16 | 10 |
| Metileo | 3 x 3, small | 16 | 10 |
| General Pico | 8 x 8, very big | 51 | 31 |
| Caleufú | 4 x 4, small | 16 | 10 |

## Checks

- `cargo test -p pampa-core`, `-p pampa-content`, `-p pampa-render`, `-p game`
- `cargo build -p game`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`

## Progress

- 2026-10-24: branch `feat/town-population` from main d4e3b07.
- T1–T3 done (uncommitted; the user commits). Test-first: RED observed as
  compile failures for the new core and content tests, and as a real
  assertion failure for the shipped-file test before `towns.ron` changed.
  New tests: core `a_townsfolk_override_wins_over_the_size_formula`,
  `too_many_townsfolk_are_rejected`; content
  `the_shipped_towns_walk_six_tenths_of_their_old_crowds`,
  `the_townsfolk_count_is_read_and_too_many_fall_back_to_the_formula`.
  Re-pinned on purpose: game `towns.rs` shipped Pico 51 -> 31, plus small
  shipped towns now pinned at 10. Formula tests (core `town.rs`, game
  `life.rs`) unchanged and green.
- Verification (2026-10-24): `cargo test -p pampa-core` 350 passed;
  `-p pampa-content` 32 passed; `-p pampa-render` 239 passed; `-p game`
  117 passed; `cargo build -p game` ok; `cargo fmt --all -- --check` ok;
  `cargo clippy --workspace --all-targets -- -D warnings` ok.

## Next step

User reviews the diff and commits; then native review per RDD.
