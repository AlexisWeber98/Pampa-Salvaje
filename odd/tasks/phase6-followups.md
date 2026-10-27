# Phase 6 follow-ups (town facades)

## Objective
Four small robustness follow-ups left by the reviews of town facades phase 6
(see `odd/tasks/town-facades.md`, "Delivery (phase 6)"). The only behavior
change is T2: a shade tree with no street direction used to get a NaN
rotation and now stands upright.

## Tasks (route: delegated writer, Sonnet)
- [x] T1 `game/src/towns.rs`: the carriageway check compares literal expected
      carriageways per town with the gap the generated sidewalks actually
      leave, measured per cross-section across the middle half of every
      street (`assert_sidewalks_line_every_cross_section`, intersections
      skipped), after review R3-001: each street edge must carry exactly
      `Sidewalk::WIDTH` of sidewalk where block or plaza land lies beyond it,
      and none where it faces the pampa or the rail band (sabotage: ring strips
      0.5 m wider than `Sidewalk::WIDTH` fail it; so do one block's strips
      dropped, and one block's east strip 1 m narrow).
- [x] T2 `crates/render/src/town.rs` `shade_transform`: `try_normalize` on the
      lean axis; no valid axis leaves the tree upright. New test with a zero
      `toward` (sabotage: back to `normalize()` fails it). A zero
      `crown_radius` needs no guard: `x / 0.0` is infinity, which `min` drops.
- [x] T3 `crates/render/src/settlement.rs` `setup_kit`: already ordered
      `.after(setup_trees)`, so the root cause was fine; it now `warn!`s when
      `TreeKit` is missing instead of returning silently.
- [x] T4 `crates/core/src/town.rs`: test pinning that a village has no brick
      and its plaza ring is four earth strips (sabotage: village plaza walk
      brick, and village walk brick, both fail it).

## Checks
`cargo test -p pampa-core -p pampa-render -p game`: 380+393+142 passed. `cargo clippy --all-targets -- -D warnings`: clean. `cargo fmt --check`: clean.

## Next step
User review; nothing committed.

## Assets (route: inline)
- [x] A1 Flat brick sidewalks: `Surface::Paving` uses Poly Haven
      `brick_floor_003` instead of the cobbled `brick_pavement` (removed).
      `PAVING_SCAN` re-measured; credits updated. Sabotage: a missing set
      name fails `every_textured_surface_names_a_shipped_set`.
- [x] A2 Beaten-earth plaza paths: `Surface::Earth` uses Poly Haven `dirt`
      (`dirt_path/`, 2 m tiles), lifted to the old dusty tone
      `[0.50, 0.43, 0.33]`; yards keep `dry_ground_01`. Sabotage: a
      saturated target fails `the_new_scans_are_lifted_to_muted_1919_colours`.
- [x] A3 Bronze for the plaques: new `Surface::Bronze` (ambientCG Metal 017,
      `bronze/`, 0.5 m tiles, `Look::Tint([0.88, 0.85, 0.80])`, metallic 1.0
      with the map's roughness); the plaza monuments' plaques use it, every
      other Steel use stays. Sabotage: plaques back to Steel, or one face
      dropped, fail `the_monument_plaques_are_bronze_on_every_face`; Steel
      also fails the plaza test in `town.rs`; a wrong set name fails
      `every_textured_surface_names_a_shipped_set`.
- Checks: render 394 passed, clippy and fmt clean. Not yet seen in game.
