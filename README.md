# Pampa Seca

An Argentine rural-outlaw western set on the dry pampa of the 1920s–1940s,
inspired by Red Dead Redemption. The visual direction pairs a low-poly 3D world
with HD-2D style billboard sprites for characters.

This repository currently holds the first vertical slice: a procedural pampa
patch with dust fog, a controllable rider and a follow camera.

## Architecture

A Cargo workspace with three members:

| Crate | Path | Role |
|-------|------|------|
| `pampa-core` | `crates/core` | Pure game logic. No Bevy, fully unit-tested. |
| `pampa-render` | `crates/render` | Bevy plugins that draw the world. |
| `game` | `game` | Binary that wires input, core and render together. |

The rule: **game logic never knows how things are drawn.**

- Heading is a continuous angle in `core`, never an 8-way enum.
- Collision shapes are owned by logic, never derived from sprite pixels.
- Animation is a logical state (`Idle`, `Walk`, `Gallop`) in `core`, never a frame index.
- The visual representation is a swappable per-entity component in `render`
  (a billboard sprite today, a 3D model later).

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
