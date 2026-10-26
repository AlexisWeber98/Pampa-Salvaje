# Turning in place (short steps instead of a turntable)

## Objective

A player who turns while standing still, on foot or riding, takes short
steps in place instead of spinning on the idle pose.

## Problem

A/D turn the heading (`crates/core/src/rider.rs:128`). With no ground speed
the figure and the horse play their idle while the body rotates, like a
turntable. `VisualPose` carries no yaw rate and no turn clips exist.

## Scope

- `VisualPose::turn`: signed yaw rate, rad/s; the player writes it from
  the turn input, everyone else leaves it 0.
- `clips::stepping_speed(speed, turn)`: the ground speed the stride clips
  are weighed and paced by, never below the turn's own small pace
  (`TURN_STEP_RADIUS` times the rate); the sign of `speed` is kept.
- `rig.rs` (people) and `horse.rs` (horses) use it, also while logic says
  `Idle` (it does at zero speed). The existing weight easing smooths it.
- Moving normally is unchanged; standing without turning stays idle.

Out of scope: dedicated turn clips; NPC turn rates.

## Tasks

- [x] T1 Feature doc (this file).
- [x] T2 RED: tests for the pure pace, the rig's and the horse's walk weight
  and rate while turning, idle without turning, the player's pose turn.
- [x] T3 GREEN: `turn` through `VisualPose`, `player.rs`, `rig.rs`, `horse.rs`.
- [x] T4 Sabotage every new or changed test.
- [x] T5 Docs: `VisualPose` doc, `rig.rs` and `horse.rs` notes.
- [x] T6 Full checks.

Route: one delegated writer (trigger: non-trivial files across game, render
and core). Uncommitted; the user commits.

## Acceptance criteria

- Holding A or D standing still plays the walk clip at a small weight and
  a slow rate; releasing it eases back to the idle.
- No change to moving, backing, NPCs, or the gait choice.

## Checks

- `cargo test -p pampa-render <filter>` and `cargo test -p pampa-game <filter>` while iterating.
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`

## Evidence

- RED: pure pace, rig, horse and player tests failed on a stub (`stepping_speed` returned the speed; pose `turn` 0).
- GREEN: all four pass.
- Sabotage (each failed, then restored): negative-sign branch of `stepping_speed`; `moving()` ignoring the turn; the horse's Idle branch returning 0; the player writing `turn: 0.0`.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean. `cargo test --workspace`: all pass (138, 32, 352, 346 + 4 ignored).
- Constant: `TURN_STEP_RADIUS` = 0.2 m, 0.36 m/s at 1.8 rad/s (about half walk weight, walk rate floor).

## Playtest follow-up

- The user found the horse's steps a little slow. At 0.2 m the horse's pace (0.36 m/s / 1.25 m/s) fell below the walk's rate floor `RATE.0` = 0.3. `stepping_speed` now takes the radius. The horse uses `HORSE_TURN_STEP_RADIUS` = 0.3 m: 0.54 m/s, rate about 0.43, weight under full. The person keeps 0.2 m.
- The horse test now requires the rate to be above `RATE.0 + 0.05`. Sabotage: with the radius back at 0.2 m, it failed, then I restored 0.3 m.
- Clippy is clean, and `cargo test --workspace` passes (138, 32, 352, 346 + 4 ignored).
- Mouse look stays as it is: it turns the view and the head, never the steps. The user said so on 2026-10-26.
