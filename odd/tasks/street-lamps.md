# Kerosene street lamps (faroles)

## Objective (user, 2026-10-22/23)

Light the 1919 towns and estancias with kerosene street lamps: placed in
core (deterministic, data-driven), drawn low-poly, glowing from dusk to dawn,
with real light only near the camera, without hurting the Renoir iGPU.

## Decisions

- **Placement** (`crates/core/src/lamps.rs`, `StreetLamps::place`, laid in
  `game::lay_out` after the flora, so it keeps off every world obstacle):
  - lighting scales with `LampTier` (user playtest, 2026-10-23: small plazas
    had too many lamps). `VerySmall` is a grid of at most 3 x 3 blocks
    (Metileo), `Small` is `TownSize::Small`, and `Big` covers both the big and
    very big sizes;
  - plazas: one by each path's mouth (right-hand side walking in). Big towns
    have them in both plazas, plus four round the main plaza's monument in
    the bed corners. Small towns have only the four mouths. Villages have
    only the two mouths on the path toward the station (north-south when
    there is no station);
  - entrances: a pair, 4.25 m to either side of the road, 1.5 m out of every
    grid exit a road (not a street across the line) leaves by;
  - corners: one lamp per intersection, at the curb (0.7 m in from both street
    edges) of a seeded corner of the four, the next tried when it does not
    fit. Big and very big towns (General Pico): every intersection; small
    towns: every other one, in a checkerboard; villages: only the
    intersections round the plaza and the two nearest the station. "Every corner" was read as
    every intersection (esquina), not four lamps per crossing;
  - estancias: one at either end of the casco's gallery (1.2 m out of its
    front) and one beside the trunk track just inside the gate.
  - Keep-outs: door fronts (4 m out, 1.8 m to either side), store verandas,
    platforms, canopies, the rail corridor and band, plaza paths and monuments,
    roads (3.75 m from the centerline), settlement tracks, 3 m between lamps,
    0.5 m clearance round the post. Each post is a 0.24 m square obstacle.
- **Model** (`crates/render/src/lamps.rs`): cast-iron base, shaft, ladder bar,
  lantern seat, pyramid roof and finial (one mesh) and a glass box (one mesh):
  70 triangles, budget 96. Two shared meshes, two shared materials; the post
  carries a `ThinOccluder`, the glass does not (fading copies a material once,
  which would freeze its glow).
- **Streaming**: town lamps hang off the `TownRoot` (spawned when it appears);
  estancia lamps off the terrain chunk owning the settlement's hub, with the
  settlements' existing ranges (post: props', glass: buildings'). No new
  `VisibilityRange`.
- **Glow**: `lamp_glow(hour)` is 1 minus a smoothstep of the sun's elevation
  from -0.06 to 0.10 rad: off by day, full at night, fading over about half an
  hour at dusk and dawn. One shared glass material, updated only when the glow
  changes.
- **Light**: a pool of `LAMP_LIGHTS` = 8 point lights, warm kerosene colour,
  75,000 lm at full glow (30,000 until the 2026-10-26 playtest), 12 m range, shadow maps and contact shadows off,
  moved each frame to the lamps nearest the 3D camera (the stream focus
  without one); hidden by day. Intensity is a first guess, to tune in game.
- **Minimap**: skipped.
- **Night quickly**: debug console `hora HH[:MM]` moves the clock on.

## Tasks

- [x] T1 Core placement and tests (inline; RED observed: 5 of 7 failing on the
  stub, then green).
- [x] T2 Render model, streaming, glow and light pool, with tests (inline;
  tests written with the code, no separate RED).
- [x] T3 Game wiring, startup log, shipped-world test; console `hora`.
- [x] T4 README, setting guide, this document.
- [x] T5 Playtest follow-up: plaza lamps (and a village's corners) scaled by
  `LampTier`; tests `plaza_lamps_scale_with_the_town` and
  `a_village_gets_two_plaza_lamps_and_corners_only_by_the_plaza_and_station`
  (inline; written with the change, no separate RED).

## Counts (shipped world)

Monte Nievas 20 (4 plaza), Eduardo Castex 23 (4 plaza), Metileo 14 (2 plaza,
6 corners), General Pico 105 (12 plaza: 8 main, 4 station); each of the 3
estancias 3.

## Checks

`cargo test -p pampa-core`, `-p pampa-render`, `-p game`; `cargo build -p
game`; `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets
-- -D warnings`. Visuals (glow from afar, light intensity, fade) are not
checked: to be looked at in game (`hora 21`, `goto pico`).

## Progress

Implemented uncommitted on `feat/street-lamps`, for the user's review.
