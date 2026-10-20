# Feature: pampa-art

Locator: `odd/tasks/pampa-art.md` · Engram mirror: `odd/pampa-art/tasks`

## Objective

Make the Pampa line feel alive. Players start standing on the station platform. Characters and horses get richer pixel art. The open land between towns gets grassland, native trees, wire fences and a direct dirt road with level crossings.

## Problem / Why

- The game starts beside the Monte Nievas platform, not on it, because the platform is a solid 0.9 m obstacle.
- Characters and horses are simple low-resolution billboards with few frames, and every horse looks alike.
- The land between towns is bare faceted ground with scattered rocks and shrubs, so the 2.3 km ride to Castex feels empty.

## Rules chosen (user-approved 2026-10-20)

- **Platform:** the platform is a walkable raised floor. Its top height is part of the ground height where it stands, not a collision box. The player starts on top of it, beside the passenger coaches of the dwelling morning train. People can walk off its ends or its town-side edge (pick one, and keep it consistent). The track side stays blocked by the train when one is present.
- **Characters and horses (style: richer pixel art):**
  - Keep camera-facing billboards with 8 view directions.
  - Raise sprite resolution, and add shading and more animation frames (walk, trot and gallop for horses; walk and jog for people).
  - Period clothes: boina, bombacha, poncho, alpargatas, and a hat for the gaucho.
  - Horse coats: zaino, tordillo, overo and alazán, chosen deterministically by seed.
  - Everything stays procedural (painted in code).
- **Terrain between towns:**
  - Grassland with tufts (paja, flechilla) and colour variation from dry yellow to grey-green.
  - Caldén and chañar trees, scattered and in small groves. Trees are obstacles; they cast shadows.
  - A dirt road: a direct track between neighbouring towns, not only along the rails. It crosses the railway at level crossings where needed, and it is visible and slightly flattened.
  - Wire fences (alambrados) dividing fields, with gates (tranqueras) where the road crosses them. Fences are obstacles; gates are open.
  - All of this is streamed by chunk, deterministic and seeded, and kept clear of towns' leveled bands, the track and the road.

## Scope and delivery

Three PRs. The user commits and pushes; Claude never commits. Each PR is reviewed on its own commits.

1. **PR-A `feat/platform-start`:** walkable platform and the start on it.
2. **PR-B:** richer character and horse sprites.
3. **PR-C:** grassland, trees, dirt road with level crossings, fences and gates.

The line-train boarding PR comes after these three.

## Constraints

- `pampa-core` stays dependency-free. Placement rules (road, fences, trees) are pure and tested in core; render only draws.
- Everything is deterministic.
- About 400 lines per task is advisory only.
- Keep each PR under about 1500 changed lines so the native review fits.

## Tasks

- [x] **T1 (PR-A)** Walkable raised platform, a ground height that includes the platform top, the start spot on the platform, and the camera and townsfolk handling it. Route: delegated writer (trigger: 2+ non-trivial files across core and game). Uncommitted; the user commits.
- [ ] **T2 (PR-B)** Richer pixel-art people and gaucho (resolution, shading, frames, period clothes).
- [ ] **T3 (PR-B)** Horses: coats, gait frames, shading.
- [ ] **T4 (PR-C)** Core: direct road network between neighbouring towns, with level crossings, fence layout with gates, and tree and tuft scatter rules.
- [ ] **T5 (PR-C)** Render: grass colour variation, tufts, trees, road surface, fences, gates and crossings, streamed by chunk. Minimap: draw the road.

## Checks

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build -p game
```

## Progress / Evidence

- Branch `feat/platform-start` from `main` at 7d2304b. PR4 (errand) is merged.
- **T1 done (uncommitted, for the user to review and commit).**
  - Design: `pampa_core::Floor` (one per platform, `TownLayout::floors`) with `rise_at(p)`: the platform's top on its footprint, a 3 m linear ramp past each end (along the track), and plain steps on the long sides. `World::ground_height(p)` = terrain + highest floor rise; it is the one function for where bodies stand. A small `Ground` trait (impl for `World` and bare `Terrain`) lets `Rider::new`/`on_foot`/`mount` and `FollowCamera::desired` take the world.
  - On and off: ramps at both ends, plus free stepping up or down the town side (and the track side, where a dwelling train's cars, already moving obstacles, block the way). The platform is no longer in `TownLayout::obstacles` or the camera occluders.
  - Start: `TownLayout::platform_spot` is on the platform, half way along it (where the coaches' middle stops, the station being centered on it), 1 m in from the track edge (about 2.5 m from the cars' side, 4 m from the rails), facing town.
  - Render: unchanged platform box, plus one sloping concrete slab per ramp. Game: player spawn, townsfolk, the tethered horse, errand NPCs and the camera floor use `ground_height`.
  - Tests (core RED observed, 6 failing, then GREEN): `world::the_ground_on_a_platform_is_its_top_and_its_ramps_lead_down`, `town::the_platform_is_a_floor_with_clear_ramps_not_an_obstacle`, `town::obstacles_cover_buildings_and_trees` (updated), `places::the_platform_spot_is_on_the_platform_by_the_track_facing_town` (replaces the past-the-end test), `places::the_game_starts_on_the_platform_beside_the_dwelling_coaches`, `walker::a_person_walks_off_the_platform_into_town_down_a_ramp_or_its_edge`. Game: `camera::on_the_platform_the_camera_stays_above_it_and_is_not_blocked_by_it` (RED confirmed with the occluder filter removed); the headless errand test now also checks the player stands on the platform's top.
  - Checks: `cargo fmt --all -- --check` ok; `cargo clippy --workspace --all-targets -- -D warnings` ok; `cargo test --workspace` ok (core 192, game 59, render 99, content 25); `cargo build -p game` ok.

## Next step

The user reviews and commits T1 (PR-A). Then T2/T3 (PR-B): richer character and horse sprites.
