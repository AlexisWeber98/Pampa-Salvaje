# Glorious sky

## Objective (user, 2026-10-27)

"Agregá cielo: nubes, sol, luna, estrellas, crepúsculo, atardeceres, aves —
glorioso." A sky that makes the open pampa breathtaking at every hour,
following the game clock.

## Decisions

- **One dome, one shader** (`crates/render/src/sky/`): an inward-facing UV
  sphere (24x16, 720 triangles) of radius 800 m, recentred on the drawing
  camera every frame (camera far plane is 1000 m), with a custom `Material`
  (`sky.wesl`, embedded). One draw call paints the gradient, sun disc and
  glow, moon with phases, stars, Milky Way, Southern Cross, cumulus and
  cirrus. No fog on the dome: its horizon colour IS the fog/clear colour
  (`Sky::haze`), so ground fogged into the distance meets the sky seamlessly.
- **Why not Bevy's `Atmosphere`**: it needs compute LUTs and an HDR camera,
  gives a Rayleigh/Mie blue that fights the dusty look, and draws no stars,
  moon, clouds or birds. The game has no bloom/post-process feature enabled.
- **Colour ramps live in `atmosphere.rs`** (`sky_with_moon`): zenith (day,
  dusk, blue hour, night), twilight strength (gaussian on the sun's
  elevation), star fade, haze tinted orange/pink at twilight. The fog and
  clear colour follow it. `sky_at(hour)` keeps its signature.
- **Astronomy** (`sky/astro.rs`, pure): the sun's great-circle path, lunar
  age from the continuous game clock (29.53-day synodic month), the moon's
  position (lagging the sun by age/29.53 of a day), its lit direction for
  phases, and the star frame (celestial pole at the pampa's 36.5 S, sidereal
  rotation; Acrux transits at 21:00 on day 0). The directional light at night
  follows the moon, with its elevation lifted to at least 0.3 rad so the
  night stays readable even when the disc is below the horizon (lux
  unchanged).
- **Clouds**: procedural noise on a plane projection in the shader, drifting
  with real time along a fixed wind; coverage varies by game day. Lit/shade
  colours come from the sun colour, so sunset undersides glow.
- **Birds** (`sky/birds.rs`): at most 48 entities, one draw call each, a
  mesh swapped between 6 flap frames plus a glide frame; unlit dark silhouettes
  with the standard fog. Soaring chimangos by day, crossing flocks by day,
  roost flocks at dusk, none at night. Positions are analytic, wrapped on a
  440 m torus round the camera, always at least 30 m above it. Visibility
  changes only while a bird is out of fog range.
- **Stars: a real map** (user, 2026-10-27): the procedural stars and Milky Way
  were dropped for Solar System Scope's "Stars + Milky Way" 8k map (CC BY 4.0,
  downloaded by the owner; credited in `CREDITS.md` and
  `assets/textures/sky/CREDITS.md`). It is in galactic coordinates, baked to
  UASTC KTX2 with mips (5.4 MB on disk, about 45 MB of BC7 on the GPU), sampled
  with a 16x anisotropic linear sampler that wraps in longitude. The shader
  takes a world direction to galactic coordinates through `galactic_frame` (the
  sidereal turn at 36.5 S plus the fixed equatorial to galactic rotation; the
  map's top row is latitude -90 and longitude grows leftward, checked against
  the Magellanic Clouds). It fades with `star_fade_at(sun elevation)`, and the
  shader also reads the sun's height from its own `sun` uniform, so day and
  night cannot disagree.
- **Clear skies** (user): `CLOUD_COVER` (0.12) is the one knob; the day-to-day
  variation is +-40%. Cirrus is thin wisps.
- A playtest sighting of daytime stars at 10:53 coincided with a sabotage proof
  build (`star_fade: 1.0`); the current code gives zero stars by day at 08:00,
  10:53, 12:00 and 17:00 (tests).
- Visual check is left to the user's playtest (no screenshot mode on main).

## Tasks

- [x] T1 Doc, astro module and sky ramps in `atmosphere.rs`, with tests.
- [x] T2 Dome, material, shader (gradient, sun, moon, stars), plugin, tests.
- [x] T3 Clouds (cumulus, cirrus, sunset glow, drift), tests.
- [x] T4 Birds, tests.
- [x] T5 Wiring in `game`, README line, verification.

## Checks

`cargo test -p pampa-render`, `cargo test -p game`, `cargo build -p game`,
`cargo clippy --workspace --all-targets -- -D warnings`,
`cargo fmt --all -- --check`.
