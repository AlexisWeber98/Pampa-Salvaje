# World

The world and its towns live in this folder as plain text files in
[RON](https://github.com/ron-rs/ron) (Rusty Object Notation). You do not need
to know Rust to edit them: change a file, save it, and start the game again.
No recompiling.

## `towns.ron`: the world, its towns and how they are laid out

```ron
(
    world: (
        half_extent: 8400.0,
        chunk_size: 200.0,
    ),
    towns: [
        (
            name: "Monte Nievas",
            origin: (-2450.0, -1950.0),
            blocks: (4, 3),
            block_size: 40.0,
            street_width: 10.0,
            railway: Some((side: North, offset: 24.0)),
            plaza: Auto,
            seed: 7,
        ),
        // ... Eduardo Castex, Metileo and General Pico follow.
    ],
    line: Some(["Eduardo Castex", "Monte Nievas", "Metileo", "General Pico"]),
    lagoons: [
        (
            name: "Laguna La Arocena",
            center: (3401.0, 841.0),
            size: (215.0, 145.0),
            seed: 5,
        ),
    ],
)
```

### The shipped geography

The file lays out a first stretch of La Pampa: three railway towns at their
real relative positions, with distances scaled 1:7 to fit the largest world
(`half_extent: 8400`, a 16.8 km x 16.8 km square). Only the distances between
towns are scaled; each town keeps its real size (blocks, streets, buildings
and platforms).

| Town | `origin` | `blocks` | Size |
|------|----------|----------|------|
| Monte Nievas | `(-2450, -1950)` | 4 x 3 | small, and the player starts here |
| Eduardo Castex | `(-4250, -2721)` | 5 x 4 | small |
| Metileo | `(288, -544)` | 3 x 3 | a railway halt: a third of its lots built, no police station |
| General Pico | `(2664, 1250)` | 8 x 8 | very big (see "Town size"); the line runs through its middle |

From Castex to Monte Nievas is about 12.6 km east and 5.4 km north in the
real pampa; from Monte Nievas to General Pico, about 35.8 km east and
22.4 km north. Monte Nievas is the anchor, at `(-2450, -1950)` (600 m east of
where it stood at 1:6, to leave room for towns further west), and the others
are placed from it with the real offsets divided by 7 and rounded to the
meter:

- Castex: `(-2450 - 12600 / 7, -1950 - 5400 / 7)` = `(-2450 - 1800, -1950 - 771.4)` = `(-4250, -2721)`,
  1.8 km west and 0.77 km south of Monte Nievas (1.96 km between their centers).
- Metileo, between them on the line (station 1906), is placed from Pico:
  its station (-35.774, -63.941) is about 16.63 km west and 12.56 km south
  of Pico's plaza (-35.659, -63.758; OpenStreetMap and es.wikipedia), so
  `(2664 - 16630 / 7, 1250 - 12560 / 7)` = `(2664 - 2375.7, 1250 - 1794.3)` = `(288, -544)`,
  about 260 m off the straight Monte Nievas–Pico chord (the line bends
  through its station).
- Pico: `(-2450 + 35800 / 7, -1950 + 22400 / 7)` = `(-2450 + 5114.3, -1950 + 3200)` = `(2664, 1250)`,
  5.11 km east and 3.2 km north of it (6.03 km between their centers).

Monte Nievas, Castex and Metileo have the railway on their north side;
General Pico grew on both sides of it, so its line runs through the middle
of the town (`through: true`). The real Pico's grid is turned 45° and its
line runs along it, parallel to the odd-numbered streets; the game keeps
the grid square to the map and runs the line east-west through it. Monte
Nievas and Metileo have no police station (`police_stations: Some(0)`).

### The world

| Field | Meaning | Rules |
|-------|---------|-------|
| `half_extent` | Distance from the world center to each edge, in meters. The world is a square of side `2 * half_extent`. | Above 0, at most 8400 (positions are `f32`, precise to about a millimeter anywhere below 16 km). Optional, `4000.0` by default. |
| `chunk_size` | Side of the square chunks the ground is built in around the rider, in meters. | At least 50, at most the world's side. Optional, `200.0` by default. |

The whole `world` section is optional: a file with only `towns: [...]` gets
the default 8 km x 8 km world. An invalid section is reported and the default
world is used.

### Towns

The game builds **every town** in the list, each at its own `origin`. The
player starts at the first one, on its station platform (on the road into
town when it has no railway); the others appear
(with their signs and people) as you ride towards them. Roads between
towns will come later; the railway line already joins them (see "The railway
line").

Towns must keep apart: a town, its leveled band (60 m around everything
built) and its railway (which runs 400 m past either end of the grid) must not
touch another town or its leveled band, and must stay inside the world. A
town that breaks this is skipped with a warning naming it and the town it
overlaps; the towns listed before it win.

| Field | Meaning | Rules |
|-------|---------|-------|
| `name` | The town's name. It is painted on the station sign, in capitals. | Not blank. Use only letters the pixel font has (see `assets/dialogue/README.md`). |
| `origin` | Center of the street grid, in meters: `(east, north)` from the world center. | Optional, `(0.0, 0.0)` by default. |
| `blocks` | Blocks `(east-west, north-south)`: columns and rows of the grid. | At least 3 each way. |
| `block_size` | Side of one square block, in meters. | At least 30. Buildings and lots grow with the block. |
| `street_width` | Width of every street, ring street included, in meters. | At least 8. Narrow streets get a shallower store veranda. |
| `railway` | `Some((side: North, offset: 24.0))`: the side of the grid the track runs along (`North`, `South`, `East`, `West`) and its distance from the grid edge, in meters. `None` for a town without a railway (no station, and the line train never stops there). Add `through: true` for a town grown on both sides of its line (see "A line through the town"). | Optional, `None` by default. Offset at least 24 (room for the station yard). |
| `density` | `Some(0.3)`: the share of house and shed lots that are built, from 0 to 1, for a village or a halt. | Optional, `None` (every lot as usual) by default. |
| `plaza` | Which block is the plaza (see below). | Optional, `Auto` by default. |
| `seed` | Varies which lots have houses or sheds. | Optional, `0` by default. |
| `police_stations` | How many police stations ("COMISARÍA") the town has, instead of its size tier's count (see the size tiers below). `Some(0)` leaves the town without one; its block gets houses instead. Monte Nievas ships with `Some(0)`: the errand sends the player to Castex. | Optional, `None` (the tier's count) by default. At most the tier's count: 1 for a small or big town, 2 for a very big one. More is rejected. |

The whole town, plus its 60 m band of leveled ground and its railway, must fit
inside the world (see above).

### Adding another town

Add another entry to `towns`, with an `origin` far enough from the others.
For example, a town 3 km east and 2 km south of the world center, with the
railway on its south side:

```ron
        (
            name: "Winifreda",
            origin: (3000.0, -2000.0),
            blocks: (3, 4),
            block_size: 36.0,
            street_width: 9.0,
            railway: Some((side: South, offset: 26.0)),
            plaza: Auto,
            seed: 11,
        ),
```

Leave room for each railway's 400 m run-out: towns lined up along their
tracks need more space between them than towns side by side across them. If
two towns are too close, the warning names both.

A town with a railway must also be added to `line` (see below), for example
after General Pico: `line: Some(["Eduardo Castex", "Monte Nievas", "Metileo",
"General Pico", "Winifreda"])`.

### The railway line

`line` lists the railway towns in the order one railway line runs through
them. Inside a town the track runs straight along the station; between two
towns listed next to each other, a link (at any angle) joins the end of one
town's track nearest the other town to the end of the other town's track
nearest the first. The link leaves each town's track along it and bends
with wide 500 m curves, tightened only when the towns are too close for
them.

| Field | Meaning | Rules |
|-------|---------|-------|
| `line` | `Some(["Eduardo Castex", "Monte Nievas", "Metileo", "General Pico"])`: town names in line order. | Optional. Without it, the line runs through every railway town from west to east, by the east coordinate of each town's center, which is its `origin` (towns listed first win ties). |

The line is a rule of the world:

- Every town with a railway must be on it, and every name must be a town of
  the file with a railway, listed once.
- A link must not cross or touch another town's leveled band (60 m around
  everything built), and must stay inside the world.

A name that breaks this is left out of the line, and a link that breaks it
is not built; each gets a warning naming the towns. The towns themselves are
still built.

One train runs the whole line, from the first town listed to the last and
back, stopping at every station on the way. It is a rural mixed train of 8
cars (93 m): the locomotive, two passenger coaches and five freight cars, and
it stops with the middle of its coaches at each station. Its timetable is in
code (`pampa_core::Timetable`), not in this file. Since a user change on
2026-10-21 it spends the night at the last town (General Pico), leaves it at
07:00, lays over at the first town (Eduardo Castex), leaves it at 16:00 and
runs back to the last town for the night. (Before, it slept at the first town
and left it at 07:00.) It tops out at 65 km/h and stands 25 s at each station
partway through a run. The game begins in the mid-morning with the morning train,
from the last town on its up run, standing at
the first town of the `towns` list, where the player starts on foot on top
of the platform, half way along it, beside the coaches. The platform is a
raised floor with a short ramp at each end, not an obstacle. The train runs
as far as the line is unbroken: a link left out ends its run at the town
before it, and a line with fewer than two stations has no train.

### A line through the town

`railway: Some((side: North, offset: 24.0, through: true))` runs the track
east-west through the middle of the town instead of along one side, like
the big railway towns that grew on both sides of their line:

- The grid splits between its two middle rows of blocks (the south half gets
  the fewer rows on an odd count). Each half keeps its own ring street, and
  between them lies a band `offset + 16` meters wide: the track runs
  `offset` meters from the edge of the half named by `side` (`North` or
  `South`), and 16 meters from the other.
- The station, its platform, grain stores, windmill and shed stand in the
  band, in the middle of the town, the station facing its half across the
  ring street.
- Every north-south street that keeps 2 m clear of the station yard
  crosses the band at a level crossing, with a plank deck and a St Andrew's
  cross on each side, and walkers cross there too. The streets that would
  run into the yard stop at the band on either side.
- `plaza: Auto` takes the block one column west of the middle and two rows
  back from the track in the station's half (a very big town's 2 x 2 plaza
  covers it): a few blocks north-west of the station when `side` is
  `North`. A plaza that would span the track is an error.
- The town has no rural track: its streets cross the line. A road between
  towns never leaves it eastward or westward, along the line.
- It needs at least 4 rows of blocks, and `side` `North` or `South`.

### Lagoons

`lagoons` lists the shallow freshwater lagoons of the plains:

| Field | Meaning | Rules |
|-------|---------|-------|
| `name` | Its name, for warnings. | |
| `center` | Middle of the water, `(east, north)` in meters. | |
| `size` | Size of the rounded rectangle its shore follows, `(east-west, north-south)` in meters. | Each between 40 and 1500. |
| `seed` | Varies the shore. | Optional, `0` by default. |

The shore is a rounded rectangle (a superellipse, squarish with soft
corners), roughened by up to 5%. The water stands 0.6 m below the lowest
ground of its 30 m bank, and the bottom shelves down to 1.6 m. People and
horses wade the first 5 m of water and are turned back where it gets
deeper. Fences and settlements keep 15 m past the bank; trees stay 12 m
from the water, and grass, stones and shrubs grow right down to the 3 m of
wet mud along the waterline. A lagoon whose bank (plus that margin) touches a town's leveled
band, the railway or a road, or leaves the world, is left out with a
warning. The whole list is optional.

Each lagoon gets a public road from the nearest town within 2.5 km of it: a
dead-end dirt road between two fences, like the roads between towns. It
leaves the town by the street through the side facing the lagoon that keeps
40 m from the track, runs out on a gently curving line, and ends on the bank
in a turnaround loop 6 m across whose edge comes to the wet mud and no
farther. A signpost at its start names the lagoon. The road is the only
thing laid on the bank and its margin, and only at its end.

The shipped Laguna La Arocena is real 5.9 km at bearing 119° from General
Pico (5.16 km east, 2.86 km south), scaled 1:7: `(2664 + 5160 / 7, 1250 -
2860 / 7)` = `(3401, 841)`. It covers about 150 ha in reality; the game keeps
its proportions at 215 x 145 m.

### Plaza placement

- `Auto`: the central block when both block counts are odd. Otherwise the
  block **facing the station**, like many pampa railway towns. A town with no
  railway and no central block takes the block nearest the center.
- `Center`: the central block. It is an error on a grid with an even count.
- `FacingStation`: the block in the row (or column) next to the railway,
  nearest the middle (ties go west or south). The station is moved along the
  track to face the plaza across the street. Needs a railway.
- `Block(i, j)`: block column `i`, row `j`, counted from 0 at the south-west
  corner.

The church faces the plaza from the block north of it (or the next free
neighbor: south, east, west; the nearest free block for a plaza in a
corner). The general stores and police stations are spread over the town so
it has life all over (user, 2026-10-23): no two of these buildings (church
included) stand on neighboring blocks, two of the same kind stand about
half the town apart (half its longer side in blocks, at least two), and
within that each takes the free block nearest the plaza (then the lowest
column, then row), facing toward it. In a town grown on both sides of its
line, each goes to the half with fewer of its kind, so each half has a
store and a police station of its own. When no block keeps that spacing (a
tiny grid), the building takes the one farthest from the others.

A small town's store is a pulpería: its sign reads "PULPERÍA". A big or
very big town's reads "ALMACÉN DE RAMOS GENERALES". (On the minimap both are
marked "A", the plaza being "P".)

### Town size

A town's size comes from its block count, `n` = columns times rows:

| | Small (`n` up to 36) | Big (37 to 63) | Very big (64 or more) |
|---|---|---|---|
| Main plaza | 1 block, placed by `plaza` | 2 blocks merged, centered | 2 x 2 blocks merged, centered |
| Station plaza | no | yes | yes |
| General stores | 1 | 2 | 2 |
| Police stations | 1 | 1 | 2 |
| People walking | 16 | `0.8 n`, rounded, at least 16 | same |

- A merged plaza includes the streets between its blocks: there is no road
  through it and walkers go around it. A big town merges two blocks along
  its longer side (east-west on a square grid). `Auto` and `Center` center
  the plaza on the grid (ties go west or south); `Block` and
  `FacingStation` keep the picked block inside it.
- The station plaza is half a block. With a railway, it takes the half
  nearest the track of the block, in the row next to the railway, that
  faces the station; when that block is taken, the nearest free block
  along that row. The other half of its block keeps houses. Without a
  railway, it takes half of the free block farthest from the main plaza.
- One general store faces the station plaza; the other is spread over the
  town with the police stations (see above). The tied horse stays at that
  other one, the town's first general store.
- `police_stations` can lower the tier's police station count (down to
  none), never raise it. A town without the field keeps the tier's count
  and is laid out exactly as before.

### When something is wrong

If the file is missing or cannot be read, or a town breaks a rule above, the
game still starts: it logs a warning naming the town and the problem, skips
that town, and uses the built-in Eduardo Castex when no valid town is left.
Run `cargo test -p pampa-content` to check the shipped file before playing.

Fields the game does not know yet are ignored, so they can be added ahead of
the code that uses them.
