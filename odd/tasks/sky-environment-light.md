# Sky environment light (image-based ambient from the custom sky)

Route: delegated writer (one bounded writer, no commits; the user owns them).
Branch: `feat/sky-environment-light`.

## Objective

Replace the flat, same-from-every-direction `GlobalAmbientLight` with
image-based light generated from the sky the game already draws: blue from
above, warm at the horizon at dusk, dark below; it follows the hour; glossy
surfaces reflect the sky. No frame-time cost worth noticing.

## Approach and why

Bevy 0.20 offers (read in `bevy_light-0.20.0/src/probe.rs` and
`bevy_pbr-0.20.0/src/{atmosphere/environment.rs,light_probe/generate.rs}`):

- `AtmosphereEnvironmentMapLight`: renders Bevy's own physical `Atmosphere`,
  which the game does not use (the sky is the custom `sky.wesl` dome). Using
  it would mean a second, different sky. Rejected.
- Rendering the dome into a cubemap with six cameras: heavy (a camera per
  face, the dome material and the star map) and awkward to throttle.
- **Chosen: paint a small cubemap on the CPU from the same `Sky` data the dome
  uses (zenith, haze, sun, twilight, moon) and hand it to
  `GeneratedEnvironmentMapLight`.** Bevy filters it on the GPU into the 32 px
  diffuse irradiance map and the prefiltered specular mip chain. No new
  Cargo feature (it is part of `bevy_pbr`, already enabled), no extra
  cameras, trivially headless-testable, and fully under our control.

The cubemap has no clouds, stars or birds: ambient light and reflections need
the broad colour field, not detail.

### Throttling (the hard requirement)

Bevy re-filters every frame while a `GeneratedEnvironmentMapLight` sits on an
entity (`downsampling_system`/`filtering_system` run for every
`RenderEnvironmentMap`). So:

- the paint itself runs on `AsyncComputeTaskPool` (follow-up, requested
  after the first measurement of an 11.5 ms main-thread hitch): the `Sky`
  is copied into a task, at most one task runs at a time, the system polls it
  with `check_ready`, and on landing swaps the bytes into the image and puts
  the generator on the cameras. A sky change while a paint is pending just
  waits; the sky is compared again the frame the paint lands, so one more
  paint follows if it moved;
- the cubemap is repainted only when `needs_refresh` says the sky moved: sun
  or moon by more than 2 degrees, or daylight/twilight by more than 0.03;
- after a repaint the generator component is on the camera for
  `FILTER_FRAMES = 3` frames, then removed; the filtered `EnvironmentMapLight`
  it created stays and keeps lighting, with no GPU filter work in between;
- exposure (`EnvironmentMapLight.intensity`) is updated every frame, which is
  a field write.

### Ambient rebalance

`Sky::ambient_brightness` (180 at night, 400 at noon) stays the total ambient
budget, so the moonlit floor tests are unchanged. A renderer-added resource
`EnvironmentShare(0.6)` splits it: `follow_the_clock` sets the flat
`GlobalAmbientLight` to 40% of the budget, and the environment light's
intensity is `budget * 0.6 / mean_luma(cubemap)`, so the sum is the old
brightness on average while the 60% now has direction and colour. Without
the plugin (headless) the resource is absent and the flat ambient keeps 100%.

## Tasks

- [x] T1 Pure logic in `crates/render/src/sky/environment.rs`: `sky_radiance`,
  `texel_direction`, `paint_cubemap`, `f16_bits`, `needs_refresh`,
  `environment_intensity`.
- [x] T2 `SkyEnvironmentPlugin` (added by `SkyPlugin`, renderer only): creates
  the cubemap, attaches `GeneratedEnvironmentMapLight` + `SkyEnvironmentLight`
  to every `Camera3d`, repaints on change, removes the generator after
  `FILTER_FRAMES`, follows exposure.
- [x] T3 `EnvironmentShare` in `atmosphere.rs`; flat ambient takes the rest.
- [x] T4 `todo.md` line ticked.
- [x] T5 Full checks and a short live run (see Verification).

Allowed edit surfaces were respected; `lib.rs`, `main.rs` and `Cargo.toml`
needed no change (the plugin rides inside `SkyPlugin`).

## Performance

See Verification for the numbers. Regeneration frequency: 260 repaints per
game day (the sun moves 2 degrees about every 5.5 game minutes), a day lasting
48 real minutes, so about one repaint every 11 s of play, each followed by 3
frames of GPU filtering.

## Sabotage evidence

Every new or changed test was proven to fail on broken code, then the code was
restored exactly (diffed against a saved copy) and the tests passed.

| Test | Sabotage | Failure seen |
|---|---|---|
| `a_still_sky_does_not_repaint_but_a_moved_one_does` | trigger always true; then each of the daylight, twilight and moon terms removed | assertion failed (still sky repainted / light-only change not detected) |
| `the_sky_is_blue_above_warm_at_dusk_and_dark_below` | ground colour set to the zenith; dusk stain multiplied by 0 | "dark below" and "stain is the twilight's doing" assertions failed |
| `the_cubemap_has_six_faces_and_follows_daylight` | +X face direction mirrored | face-direction assertion failed |
| `half_floats_round_trip` | overflow returns infinity bits | 1e9 assertion failed |
| `the_intensity_keeps_the_ambient_budget_and_follows_daylight` | intensity ignores the budget | exposure assertion failed |
| `the_generator_is_removed_once_the_filtered_light_exists` | settle step never removes | generator still present |
| `cameras_get_the_environment_light_and_it_is_repainted_only_on_change` | trigger always true | repaint count assertion failed |
| `an_environment_map_takes_its_share_of_the_ambient_budget` | flat ambient ignores the share | flat brightness assertion failed |
| `a_refresh_paints_off_the_main_thread_and_lands_later` | paint result dropped; filter not re-enabled on landing; pending task never polled | image unchanged / generator missing / assertions failed |
| `a_change_during_a_paint_waits_for_it_and_repaints_once_after` | pending task ignored (second task spawned); no repaint after landing | "no second task" and final repaint count assertions failed |
| `a_whole_day_repaints_a_bounded_number_of_times` | angle step 2.0 to 0.1 degrees | count above the bound |

Note on RED: the tests were written together with the code, so the first run
was green; the RED evidence is the sabotage above, not a failing run before
implementing. A first sabotage of the dusk stain (halo left on) did not fail,
which exposed a weak test; the stain assertion was added and re-sabotaged.

## Verification

- `cargo fmt --all -- --check`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo test --workspace --no-fail-fast -- --test-threads=8`: all green
  (180, 32, 456 and 530 passed; 5 pre-existing ignored).
- `cargo build -p game`: ok.
- `cargo run -p game` equivalent (dev binary, two runs of 40-45 s, RADV Renoir
  iGPU, Wayland): no wgpu, naga or shader errors or warnings; the compute
  check of Bevy's generator did not disable it (no "Disabling" log). Only
  unrelated `xkbcommon` noise about `~/.XCompose`. Game process killed.
- Main-thread cost, before the async change: the whole paint ran in the
  repaint frame, about 11.5 ms. After (dev profile, whole `app.update` timed
  in a temporary test, removed): the frame that spawns the task takes
  0.1-0.15 ms, and the frame the paint lands (moving ~800 KB into the image)
  0.2-0.4 ms. The 11.5 ms now runs on a pool thread.
- CPU paint cost (`paint_cubemap`, 128 px faces, 98k texels): about 11.5 ms
  with our crate at the dev profile's opt-level 1 (dependencies at 3). The
  release number was not measured: a `--release` build of the workspace did
  not finish in a reasonable time on this 14 GB machine and was cancelled.
  Expect roughly 3x less in release. It runs once per ~11 s of play, on a pool thread.
- GPU filter cost: not measured (no GPU timing tooling used). It runs for 3
  frames per repaint instead of every frame.
- Not seen by eye (no screenshots taken): colours and relative brightness of
  the new ambient need a look in the game: dusk shadows, metal reflections,
  night. If scenes look off, tune `ENVIRONMENT_SHARE` and the ground/halo
  constants in `sky_radiance`.
