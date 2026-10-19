# World

The towns of the world live in this folder as plain text files in
[RON](https://github.com/ron-rs/ron) (Rusty Object Notation). You do not need
to know Rust to edit them: change a file, save it, and start the game again.
No recompiling.

## `towns.ron`: the towns and how they are laid out

```ron
(
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

For now the game builds **only the first town** in the list. More towns (and
roads between them) will come later; they can already be listed.

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

The whole town, plus a 60 m band of leveled ground around it, must fit inside
the 600 m x 600 m world (300 m from the center each way).

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
