# The moonwalk (backing up plays the walk forward)

## Objective

A player who backs up, on foot or riding, steps backward: the travelling
clips play in reverse while the ground speed is negative.

## Problem

Backing up plays the walk clip forward while the body moves back. The sign
of the speed is dropped three times:

- `game/src/player.rs:188` writes `rider.speed.abs()` into `VisualPose`;
- `crates/render/src/character/rig.rs:206` clamps it (`pose.speed.max(0.0)`),
  and the clips' rate comes from `playback_rate`, never negative;
- `crates/render/src/character/horse.rs:653` takes `pose.speed.abs()`, and
  `gait_rate` is never negative.

## Scope

- `VisualPose::speed` carries the sign; negative means backing up.
- Clip choice and weights (people's walk/jog/sprint, the horse's walk and
  gallop, `HorseBody::speed` for the rider's seat) stay on the magnitude.
- Travelling clips (walk/jog/sprint; the horse's walk/gallop) play at a
  negative rate when backing; idles and the seated clip stay positive.
  Bevy 0.20 wraps a repeating clip played backward
  (`bevy_animation` `lib.rs:585-607`).
- Every other `VisualPose` reader keeps its behaviour: the figure carrier's
  cadence already takes `abs()`, `HorseGait::of` compares `abs()`, and the
  minimap, first-person and life code read only the heading or write NPC
  poses (never negative).

Out of scope: NPC backing (they never back up).

## Tasks

- [x] M1 Feature doc (this file).
- [x] M2 RED: tests for the signed pose (player), a backing figure's
  planted foot sliding forward and its stride rate negative (rig), a
  backing horse's hooves sliding forward and its walk/gallop rate negative
  (horse).
- [x] M3 GREEN: signed speed through `VisualPose`, `rig.rs`, `horse.rs`.
- [x] M4 Sabotage every new or changed test.
- [x] M5 Docs: `VisualPose` doc, `rig.rs` and `horse.rs` notes.
- [x] M6 Full checks.

Route: one delegated writer (trigger: 3+ non-trivial files across game and
render). Uncommitted; the user commits.

## Acceptance criteria

- Holding S on foot or riding plays the stride clips in reverse at the
  same pace as forward; standing still plays the idle as before.
- No change to forward motion, NPCs, or the gait choice.

## Checks

- `cargo test -p pampa-render <filter>` and `cargo test -p pampa-game <filter>` while iterating.
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`

## Outcome

- `VisualPose::speed` is signed: `drive_player` copies `rider.speed`.
- `rig.rs`: `moving` keeps the sign; `targets` weighs by `abs()`; the new
  `stride_rate` paces by the size and negates it for travelling clips
  (`Clip::ground_speed`), idles and the seated clip forward.
- `horse.rs`: `drive_horses` reads the signed `velocity`; gallop choice,
  weights and `HorseBody::speed` (the rider's seat) use its size;
  `gait_rate` returns a negative rate for the walk and gallop backing.
- Other readers unchanged: the carrier's `cadence` and `HorseGait::of`
  already take `abs()`; the rest read only the heading or write NPC poses.

### Evidence

- RED (before the fix): all five new tests failed. The backing figure's
  planted foot slid 0.0002 m/s at -1.6 m/s; the backing horse's hooves
  slid back at +1.25 m/s, as ahead.
- GREEN: the figure's foot slides forward at the ground speed (within
  12 %); the horse's hooves -1.25 m/s backing vs +1.25 m/s ahead.
- Sabotage, each restored after: `.abs()` back in `drive_player` fails the
  player test; dropping the negation in `stride_rate` fails both rig tests;
  `moving` clamped to `max(0.0)` fails the planted-foot test; a constant
  sign in `gait_rate` fails both horse tests; `pose.speed.abs()` in
  `drive_horses` fails the hoof-slide test.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo test --workspace`: all pass.

Next step: the user reviews and commits.
