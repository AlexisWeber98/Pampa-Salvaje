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
themselves stay crossable. The follow camera pulls in when a building would
block its view of the rider.

## Architecture

A Cargo workspace with three members:

| Crate | Path | Role |
|-------|------|------|
| `pampa-core` | `crates/core` | Pure game logic. No Bevy, fully unit-tested. |
| `pampa-render` | `crates/render` | Bevy plugins that draw the world. |
| `game` | `game` | Binary that wires input, core and render together. |

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
| `Esc` | Quit |
