# Horse jump

Objective: a mounted player jumps over fences and other low obstacles. Branch `feat/horse-jump`, stacked on main. Delivery: the user owns commits (left uncommitted). Test runner: `cargo test -p pampa-core`, `cargo test -p pampa-game` (focused), one full check at the end.

## Specs

- S1. "agregar saltar obstáculos de los caballos" (todo.md).
- S2. Jumpable set (user choice, 2026-10-29): "Alambrados y obstáculos bajos": fences (1.3 m) and every low obstacle below the jump's apex (trough 0.6 m, well curb 0.9 m, palenque 1.1 m). Trees, houses, corrals (1.6 m), tanks, carts, wrecks, tethered horses, signposts, the train and lagoons stay solid.
- S3. "el caballo debe estár a pleno galope para que salte" (user, 2026-10-29). Space jumps only while riding on the ground at full gallop (user, 2026-10-29: at 85 % of the top speed or more, L13); otherwise, and on foot, it does nothing. The arc peaks at about 1.5 m.
- S6. User picked "Animar el salto": in the air the horse tucks its legs and pitches (nose up at takeoff, down on landing) instead of galloping in the air. Source: the original Quaternius horse (`~/Downloads/quaternius/horse_a.glb`, already local) has `Gallop_Jump` and `Jump_toIdle` clips that `tools/glbtrim.py` dropped.
- S7. "las patas traseras del caballo son las que le dan el impulso para saltar, en la animacion no se distingue y las delanteras se retraen un poquito más, el jinete debe mantener el equilibrio al saltar el caballo, por lo que la animacion de él debe acompañar" (user, 2026-10-29). Three parts:
  - S7a: the hind legs visibly push the horse off at takeoff.
  - S7b: the forelegs tuck a little more in flight.
  - S7c: the rider keeps his balance through the jump, his pose following the horse's (assumption: a forward seat at takeoff and over the top, then absorbing the landing).
- S8. "ese hassazgo podemos corregirlo en este mismo sprint" (user, 2026-10-29). The bug found in T5: in `pose_riders` the rider's saddle "pitch" reads the Torso bone's roll, because the bone runs along +Y. It must read the real pitch.
- S4. Assumption: an obstacle is passed only while the horse is higher than the obstacle; touching it lower blocks as today. Landing in a footprint pushes out as today.
- S5. Docs: the README controls table gets a Space row.

## Tasks

- T1 [S2] StaticObstacles keeps a height per permanent footprint (streamed and plain `push` are unbounded); World resolve ignores footprints below a clearance; fences and settlement structures get their heights. Route: inline. Status: done (RED→GREEN, 6 tests sabotaged). Uncommitted.
- T2 [S1,S3,S4] `Rider::jump`, airborne state (`air`, `lift`), gravity in the sub-step, clearance passed to collision and `keep_clear_of`; no dismount in the air. Route: inline. Status: done; S3 full gallop (L8) RED→GREEN, sabotaged. Uncommitted.
- T4 [S6] Restore the jump clip and play it in the air; pitch. Route: delegated writer (context backstop). Status: done (L14): legs fixed. Uncommitted; native review due at the commit (assess high, item 3).
- T5 [S7] Hind-leg push at takeoff, a deeper foreleg tuck, and the rider's balance through the jump. Route: delegated writer (Opus; context backstop). Status: done (L17). Uncommitted.
- T6 [S8] Read the saddle pitch from the Torso bone's real pitch axis. Route: added to the T5 writer, which already holds rider.rs. Status: done (L17). Uncommitted.
- T3 [S3,S5] Game: Space edge calls `jump`, transform and camera focus lifted by `air` (player, life, fauna writers); README. Route: inline. Status: done (3 new + 1 extended test, sabotaged). Uncommitted.

## Log

- L1 (2026-10-29) user: "seguiumos con lo que esta en todo.md" → picked "Saltos de caballos".
- L2 (2026-10-29) user picked "Alambrados y obstáculos bajos" for what a horse can jump.
- L3 Route: one unit of behavior, no writer reason; inline. Visual leg tuck/pitch left for after the playtest.
- L4 Found while working: `StaticObstacles::remove` (mounting a tied horse) rebuilds with `Self::new`, which drops the streamed layer (tree trunks of loaded chunks) until those chunks reload. Pre-existing; kept as is, reported to the user.
- L5 Checks: fmt, clippy clean; `cargo test --workspace` 211+32+469+548 pass. Risk: item 3 (shared collision `resolve_among` path) → independent verifier.
- L6 Untested line: game/src/fauna.rs player transform after `keep_clear_of` uses `elevation()` (same pattern as life.rs, which is tested).
- L7 Independent verifier: no blockers, S1–S5 met.
  - The clearance-0 invariant holds: a differential hash over ~400 settled-world runs matched the base.
  - Probes: a 1.6 m rail blocks; landing in a footprint pushes out; the world bounds clamp.
  - Advisories: a NaN `air` persists (out of domain); a platform floor mid-air was not probed.
  - Its scratch /tmp/tmp.iUEXvKcdzR is gone: a reboot cleared /tmp.
- L8 (2026-10-29) user: "el caballo debe estár a pleno galope para que salte". S3 rewritten; T2 reopened.
- L9 (2026-10-29) user picked "Animar el salto" → S6, T4.
- L10 T4 independent verifier: no blockers, S1–S6 met.
  - The shared-code invariant holds by reading. The old PARTIAL bits are kept; bit 6 appears only where the jump clip animates the property.
  - The first six clips of the glbs are byte-identical to the base.
  - Workspace: 212+32+470+553 pass; clippy clean.
  - Flaky test cause: `rig_of` read the first `HorseRig`, which can be a respawned, unstarted rig. The parent fixed it to `.find(|r| r.started)`, matching the other helpers; 3 filtered runs green.
  - Advisory: a hoof may dip briefly during the ~0.25 s landing fade (unclamped). Not probed; check in the playtest.
  - Verifier scratch /tmp/tmp.04LVxvHrJj and scratchpad *.bak: removal was not permitted, left for the user.
- L11 (2026-10-29) user: "el caballo se despatarra al saltar las patas se deforman en pleno vuelo". T4 reopened.
  - Reproduced offline: `preview_saddled_horses` got a jump sequence, and Blender renders the dumps.
  - The raw `Gallop_Jump` clip looks right in Blender. In game, the front cannons hang straight and the hooves bend back.
  - Ruled out by experiment: `level_flight`, and the whole of `fit_horse`.
  - A delegated debugger is measuring the bone locals against the clip samples.
- L12 (2026-10-29) user: "me esta gustando como va quedando por ahora, que tu agente siga tranquilo, una cosa dale un 4% más de salto y 5kmh mas de velocidad máxima".
  - Plan: raise the apex by 4 % (≈1.49 → ≈1.55 m, `JUMP_SPEED` √1.04 × 5.4 ≈ 5.507).
  - Raise `gallop_max` from 44 to 49 km/h; the full-gallop threshold becomes ≈44.1 km/h.
  - To apply after the T4 debugger's verification run finishes, so its build is not disturbed.
- L13 (2026-10-29) user: "el salto que sea a 85% de la velocidad máxima". `FULL_GALLOP` goes 0.9 → 0.85 (≈41.7 km/h at the new 49 km/h top). To apply together with L12.
- L14 (2026-10-29) Leg fix and tuning are done.
  - Root cause: `level_flight` lowered only `Body`. The hoof IK targets (`IKFrontLeg.*`/`IKBackLeg.*`) are siblings of `Body` under `AnimalArmature`, so they stayed up, leaving a 0.447 m gap at flight 0.3.
  - Fix: drop the four IK targets by the same offset.
  - Test `in_the_air_each_hoof_stays_on_the_end_of_its_leg`: RED/GREEN, sabotaged. The parent checked the renders against the raw clip by eye.
  - Done by an Opus debugger. The parent's earlier no-level_flight experiment came from stale dumps.
  - Tuning applied (L12, L13): `gallop_max` 49 km/h; `JUMP_SPEED` 5.51 (apex ≈1.55 m); `FULL_GALLOP` 0.85. README updated.
  - Checks: fmt clean; clippy 0 warnings; workspace 212+32+470+554 pass.
  - Known flaky: `every_character_model_loads_skinned_within_the_triangle_budget` times out after 10 s under load (hidalgo model, untouched).
- L15 (2026-10-29) user: "me encanta, un par de detalles narda más, las patas traseras del caballo son las que le dan el impulso para saltar, en la animacion no se distingue y las delanteras se retraen un poquito más, el jinete debe mantener el equilibrio al saltar el caballo, por lo que la animacion de él debe acompañar" → S7, T5.
- L16 (2026-10-29) user: "dale, ese hassazgo podemos corregirlo en este mismo sprint" → S8, T6.
- L17 (2026-10-29) T5 and T6 are done (Opus writer).
  - S7a: `JUMP_FLIGHT` (0.45, 1.17) starts on the clip's push frame; `JUMP_LINGER` slows it through takeoff; `push_and_tuck` (gait.rs) adds a hind push that fades by flight 0.25.
  - S7b: a foreleg tuck shaped as sin²(πf); `reattach` keeps the hooves on the legs.
  - S7c: the rider folds from ~22° to ~44° over the top, the rein hand follows the neck, and he sits up to ~9° on landing with the seat never sinking.
  - S8: `saddle_attitude` reads pitch from the Torso's +Y and roll from its +X. This changes the rider's motion at every gait, by design.
  - Tests: 4 new plus 1 changed, all RED→GREEN and sabotaged. Parent spot check: 7 pass. Workspace 212+32+470+559; clippy 0.
  - Parent checked the renders by eye.
  - Risks: takeoff rears within ~60 ms; gait lean changes. Both to check in the playtest.
