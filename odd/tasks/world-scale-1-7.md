# World scale 1:7

## Decision (user, 2026-10-23)

- The ride from Monte Nievas to General Pico felt too long: distances between
  places go from **1:6 to 1:7**. Towns keep their real size (blocks, streets,
  buildings, platforms); only the distances between them scale.
- The world grows 5%: half extent **8000 → 8400 m** (`WorldConfig::MAX_HALF_EXTENT`
  and `half_extent` in `assets/world/towns.ron`). `f32` stays about a
  millimeter precise below 16 km.
- The whole map moves **600 m east** (coordinator follow-up, same PR) so
  Caleufú fits with margin in a later PR: the anchor Monte Nievas goes from
  (-3050, -1950) to (-2450, -1950).
- Later (not in this change): Metileo, Laguna La Arocena and Caleufú.

## Positions

Monte Nievas is the anchor; the others are its origin plus the real offsets
divided by 7, rounded to the meter (arithmetic in `assets/world/README.md`).

| Town | 1:6 origin | 1:7 origin, shifted |
|------|-----------|-----------|
| Monte Nievas | (-3050, -1950) | (-2450, -1950) |
| Eduardo Castex | (-5150, -2850) | (-4250, -2721) |
| General Pico | (2917, 1783) | (2664, 1250) |

## Distances and times

Gallop is `RiderParams::default().gallop_max`, 44 km/h (12.2 m/s), straight
line between origins, in real time. The shift moves no distance, so nothing
here changed with it.

| Leg | 1:6 | 1:7 |
|-----|-----|-----|
| Castex – Monte Nievas | 2 285 m, 3.1 min | 1 958 m, 2.7 min |
| Monte Nievas – General Pico | 7 038 m, 9.6 min | 6 033 m, 8.2 min |
| Line path (train) | 11 008 m | 9 712 m |
| Train run, end to end | 644.0 s | 572.2 s |

- Rail links: Castex–Monte Nievas is now 1 167.3 m on a 1 068.5 m chord and
  its curves are capped at a quarter of the chord (radius 362 m, 72.8° turn);
  Monte Nievas–General Pico keeps the full 500 m radius (5 264.6 m link).
- Timetable (base rate): up Pico 07:00 → Monte Nievas 10:14:45–10:27:15 →
  Castex 11:46:07; down Castex 16:00 → Monte Nievas 17:17:49–17:30:19 → Pico
  20:46:07.
- Opening: aboard from 10:04:45 (was 10:32:13), in at Monte Nievas 10:14:45,
  off at 10:15:09. The 50 real second lead is unchanged.

## Future towns at 1:7 (from the shifted origins)

- Caleufú (real 41.2 km at bearing 325° from Castex: 23.63 km west, 33.75 km
  north): about **(-7626, 2100)**, 774 m inside the west edge of ±8400 at its
  origin (without the shift it was (-8226, 2100), 174 m).
- Laguna La Arocena (real 5.9 km at bearing 119° from General Pico: 5.16 km
  east, 2.86 km south): about **(3401, 841)**, 4 999 m from the east edge.

## Startup counts (shipped world)

| | 1:6 | 1:7 | 1:7 shifted |
|-|-----|-----|-----|
| Fence spans | 125 354 | 129 004 | 130 226 |
| Ranchos / puestos / chacras / estancias | 5 / 3 / 6 / 2 | 4 / 2 / 8 / 3 | 3 / 3 / 5 / 2 |
| Settlement obstacles | 265 | 383 | 255 |
| Warnings | none | none | none |

The terrain noise and the per-gate seeds depend on absolute positions, so the
shift redraws the fields and settlements even though no distance changed.

## Tasks

- [x] T1 `towns.ron` at 1:7, half extent 8400, `MAX_HALF_EXTENT` 8400; docs
  and comments say 1:7; dated notes in the older feature logs. Route:
  delegated writer.
- [x] T2 map shifted 600 m east (anchor Monte Nievas at (-2450, -1950)).
  Route: delegated writer.

## Tests changed

- `content::towns` shipped-file test derives the origins from the anchor and
  the real offsets / 7 instead of hard-coding them; the `core::rail` and
  `render::train` fixtures derive them the same way.
- `core::chunk` bad-config test reads the limit from `MAX_HALF_EXTENT`.
- `core::rail` curve test accepts a capped (tighter, still > 300 m) radius
  where the chord leaves no room and requires the full radius on the Pico
  link; the chord bound is 10%.
- `core::terrain` road-cut test skips samples within 40 m of a level
  crossing, where the cut eases off the track bed by design.
- `core::train`: the dwell-skip test computes its time from `stop_times`;
  the run-length bound is 500–650 s.
- `core::settlement`: the `settled()` fixture lays the game's estancias near
  Castex and Pico (`build_with_ranches`), as `game` does; after the shift the
  seeded mix alone drew no estancia, which failed the kind-coverage checks.
- `render::train`: the Castex focus point moved with the map.
- `game`: opening times 10:04 / 10:14 / 10:15, the dwell clock test at
  10:20, the afternoon wait 17:18.

## Checks

See the delivery report of this change (`cargo test` for core, content,
render and game, `cargo build -p game`, `cargo fmt --check`, `cargo clippy`).
