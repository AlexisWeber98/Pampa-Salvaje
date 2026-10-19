# World

The world and its towns live in this folder as plain text files in
[RON](https://github.com/ron-rs/ron) (Rusty Object Notation). You do not need
to know Rust to edit them: change a file, save it, and start the game again.
No recompiling.

## `towns.ron`: the world, its towns and how they are laid out

```ron
(
    world: (
        half_extent: 4000.0,
        chunk_size: 200.0,
    ),
    towns: [
        (
            name: "Eduardo Castex",
            origin: (0.0, 0.0),
            blocks: (4, 5),
            block_size: 40.0,
            street_width: 10.0,
            railway: Some((side: North, offset: 24.0)),
            plaza: Auto,
            seed: 7,
        ),
    ],
)
```

### The world

| Field | Meaning | Rules |
|-------|---------|-------|
| `half_extent` | Distance from the world center to each edge, in meters. The world is a square of side `2 * half_extent`. | Above 0, at most 8000 (positions stay precise to about a millimeter within 8 km). Optional, `4000.0` by default. |
| `chunk_size` | Side of the square chunks the ground is built in around the rider, in meters. | At least 50, at most the world's side. Optional, `200.0` by default. |

The whole `world` section is optional: a file with only `towns: [...]` gets
the default 8 km x 8 km world. An invalid section is reported and the default
world is used.

### Towns

The game builds **every town** in the list, each at its own `origin`. The
player starts at the first one, on the road into town; the others appear
(with their signs, people and train) as you ride towards them. Roads and
railways between towns will come later.

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
| `railway` | `Some((side: North, offset: 24.0))`: the side of the grid the track runs along (`North`, `South`, `East`, `West`) and its distance from the grid edge, in meters. `None` for a town without a railway (no station, no train). | Optional, `None` by default. Offset at least 24 (room for the station yard). |
| `plaza` | Which block is the plaza (see below). | Optional, `Auto` by default. |
| `seed` | Varies which lots have houses or sheds. | Optional, `0` by default. |

The whole town, plus its 60 m band of leveled ground and its railway, must fit
inside the world (see above).

### Adding a second town

Add another entry to `towns`, with an `origin` far enough from the others.
For example, a town 3 km east and 2 km south of Eduardo Castex, with the
railway on its south side:

```ron
        (
            name: "Florencio Varela",
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

Around the plaza, the general store stands west of it, the church north and
the police station east, each facing the plaza across the street. When a
neighbor block does not exist (a plaza on the edge), the building takes the
next free neighbor (store: west, east, south, north; church: north, south,
east, west; police: east, west, south, north), or the nearest free block
for a plaza in a corner.

### When something is wrong

If the file is missing or cannot be read, or a town breaks a rule above, the
game still starts: it logs a warning naming the town and the problem, skips
that town, and uses the built-in Eduardo Castex when no valid town is left.
Run `cargo test -p pampa-content` to check the shipped file before playing.

Fields the game does not know yet are ignored, so they can be added ahead of
the code that uses them.
