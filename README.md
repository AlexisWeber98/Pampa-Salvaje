# Pampa Seca

An Argentine rural-outlaw western set on the dry pampa of the 1920s–1940s,
inspired by Red Dead Redemption. The visual direction pairs a low-poly 3D world
with HD-2D style billboard sprites for characters.

This repository currently holds a procedural pampa patch with dust fog, a
controllable rider, a follow camera and a first settlement: a small railway
town loosely inspired by 1920s–1930s La Pampa towns such as Eduardo Castex
(not a historical reconstruction). The rider starts on the road south of town,
collides with its buildings and cannot leave the 600 m patch.

The town is alive: townsfolk wander the streets between intersections, a
saddled horse stands tied at a hitching rail in front of the general store, and
a steam freight train rolls in from the west every 150 s, stops at the station
for 25 s and pulls out east. The train shoves riders off its track; the rails
themselves stay crossable. The rider barges through the crowd, shoving
walkers aside, and walkers step around the rider.

Landmarks carry painted signs in period Spanish: the general store
("ALMACÉN DE RAMOS GENERALES"), the police station ("COMISARÍA"), the church
("IGLESIA"), the station ("ESTACIÓN EDUARDO CASTEX", on its facade and on a
name board on the platform) and the plaza. `pampa-core` names the places and
says where each sign stands (`TownLayout::places`); render letters them in a
procedural bitmap pixel font (no third-party font) on weathered boards.

A paper-style minimap in the top-right corner shows the town north-up: streets,
blocks and buildings by kind, the plaza, the railway, each place by its
initial, walkers as dots, the train when it is in town and the player as an
arrow along its heading. It frames the town plus a 30 m margin (not the whole
patch) so streets stay legible; out on the pampa the arrow sticks to the map
edge. The town layer is rasterized once; only the markers redraw each frame.
`M` toggles an enlarged, centered map.

The camera orbits the rider a full 360° with the mouse (or Q/E), zooms with
the wheel, eases back behind the rider after a moment without camera input
while riding, and pulls in when a building would block its view.

Characters are procedurally painted billboard sprites (no third-party art):
rounded, shaded and outlined figures in eight view directions, with a walk
cycle and, for horses, a gallop cycle paced by their speed.

Townsfolk talk back: bump into somebody with the horse and they complain in a
speech bubble above their head ("¡Eh, mirá por dónde andás!" at a walk,
"¡Casi me pisa el pingo, che!" at a gallop). Brushing past at a crawl says
nothing, the same person waits a few seconds before complaining again, and the
same line never repeats twice in a row. Bubbles are paper with an ink outline,
lettered in the bitmap font (now with lowercase, accents and `¡ ¿`), face the
camera and fade out after about 3 s.

### Dialogue is data

What people say lives in editable [RON](https://github.com/ron-rs/ron) files
under `assets/dialogue/`, not in code: `bumps.ron` holds one pool of lines per
bump severity (`walk`, `gallop`). Edit it and restart the game; no recompiling.
The `pampa-content` crate parses and validates it (non-empty pools, at most 40
characters per line); the game checks the characters against the font, and
falls back to a small built-in set of lines with a warning if the file is
missing or invalid. See [`assets/dialogue/README.md`](assets/dialogue/README.md)
for how to write lines.

### Towns are data

Towns are generated from editable configs in
[`assets/world/towns.ron`](assets/world/towns.ron), not hard-coded: grid size
in blocks, block and street widths, the railway's side and distance, plaza
placement and a lot seed. `pampa-core` turns a `TownConfig` into a layout
(`TownLayout::generate`), checking it and explaining what is wrong; buildings,
signs, the street graph, the hitching post, the station stop and the arrival
point all follow the generated grid. Eduardo Castex is 4 x 5 blocks of 40 m
with 10 m streets, and since it has no central block its plaza faces the
station across the street, as in many pampa railway towns. The game builds the
first town in the file and falls back to the built-in Castex with a warning
when the file is missing or invalid. See
[`assets/world/README.md`](assets/world/README.md) for every field and rule.

## Architecture

A Cargo workspace with four members:

| Crate | Path | Role |
|-------|------|------|
| `pampa-core` | `crates/core` | Pure game logic. No Bevy, fully unit-tested. |
| `pampa-content` | `crates/content` | Editable content (dialogue, towns) loaded from RON files. No Bevy. |
| `pampa-render` | `crates/render` | Bevy plugins that draw the world. |
| `game` | `game` | Binary that wires input, core, content and render together. |

The rule: **game logic never knows how things are drawn.**

- Heading is a continuous angle in `core`, never an 8-way enum.
- Collision shapes are owned by logic, never derived from sprite pixels. The town
  layout (`pampa_core::TownLayout`) owns every footprint; render only picks a look
  per building kind.
- Ground height has one source of truth, `Terrain::height`, which also levels the
  ground under the town.
- Animation is a logical state (`Idle`, `Walk`, `Gallop`) in `core`, never a frame index.
- Townsfolk and the train are deterministic `core` simulations (`Townsfolk`,
  `Train`); the game steps them and render only reads them.
- Bumps are decided in `core` (`BumpTracker`: speed threshold, gait severity,
  per-walker cooldown; `LinePicker`: seeded, no immediate repeats); the words come
  from `pampa-content`; render only draws the bubble (`Say` → `SpeechBubble`).
- The visual representation is a swappable per-entity component in `render`
  (`CharacterVisual`: a billboard sprite sheet today, a 3D model later).

Bevy is pinned to an exact version (`=0.20.0`) and only upgraded between milestones.

## Running

```sh
cargo run -p game
```

Checks:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Controls

| Key | Action |
|-----|--------|
| `W` / `S` | Throttle forward / brake and back up |
| `A` / `D` | Turn left / right |
| `Shift` (hold) | Gallop |
| Mouse | Orbit the camera (the cursor is locked while the window has focus) |
| `Q` / `E` | Orbit the camera left / right |
| Mouse wheel | Zoom in / out (6–25 m) |
| `M` | Toggle the enlarged minimap |
| `Esc` | Quit |

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
