# Game-wide typeface (replace the bitmap font)

Branch `feat/game-typeface`, stacked on `feat/menu-pause` (a040a00). Delivery: its own PR. The user owns commits.
Test runner: `cargo test -p pampa-render` / `cargo test -p game` per unit, then `cargo test --workspace` once.

## Specs

- S1 (L1): "las letras con pixel art low-poly ya no van más". User choice
  "Todo el juego" (2026-10-28). Replace the procedural bitmap font
  (`crates/render/src/font.rs`) everywhere: HUD, signs in the world,
  timetables, minimap, speech bubbles. The dev console is included as well.
- S2 (L2): "la tipografía del resto del juego entra en otro PR?". Yes: this
  branch and its PR.
- S3 (assumption): typefaces from `crates/render/src/typeface.rs`. Rye goes
  on the signs painted on wood and on the timetable board's heading. IM Fell
  English goes on everything else.
- S4 (assumption): text keeps its content, and keeps its place on screen and
  in the world. Only the lettering changes, and the sizes that follow from
  it.

## Tasks

| ID | Specs | Route | Status |
|---|---|---|---|
| T1 | S1, S3 | delegated writer (context backstop) | done (uncommitted): `crates/render/src/lettering.rs` rasterizes TTF text on the CPU with `ab_glyph` (fonts embedded with `include_bytes!`): measure, coverage, glyph support. `font::supports` users switch to it |
| T2 | S1, S3, S4 | delegated writer | done (uncommitted): the UI text (HUD lines and announcement, timetable panel, dev console) is bevy `Text` in IM Fell English (`Typefaces`); `paint_text`, `TextImage`, `text_image` and the `*_SCALE` constants are gone |
| T3 | S1, S3, S4 | delegated writer | done (uncommitted): world textures (town signboards, road signposts, timetable board, speech bubbles) paint with `lettering` |
| T4 | S1, S3, S4 | delegated writer | done (uncommitted): minimap labels, title and compass paint with `lettering`; `pub mod font;` removed and nothing uses it (the file `font.rs` itself still needs `rm`, denied to the writer); docs |

## Log

- L1 (2026-10-28, user): "las letras con pixel art low-poly ya no van más".
  Then the choice "Todo el juego" from the question "dónde reemplazo la fuente
  bitmap por una tipografía real?".
- L2 (user): "me encanta (solo un detalle bairoleto era buscado en La Pampa,
  Peralta en Chaco) la tipografía del resto del juego entra en otro PR?". Then
  the choice "Encima de menu-pause".
- L3 (explorer map). The UI consumers are the HUD (hud.rs `paint_text` /
  `letter`), the timetable panel (game/src/timetable.rs, `text_image`) and
  the console (game/src/console.rs). The world textures are signs.rs
  (`layout_board` / `paint_board`, Rye-sized boards; road signposts via
  road.rs) and bubble.rs (`layout_bubble` / `paint_bubble`, cached per line,
  nearest sampler). The raster consumer is minimap/raster.rs
  (`label_scaled`, `title`, compass). Charset-only users are towns.rs,
  errand.rs, complaints.rs, ride.rs and content's `unsupported_chars`. bevy_text
  has no string-to-Image API. `ab_glyph` 0.2.32 is already in Cargo.lock
  (transitively). Tests that pin bitmap metrics live in font.rs, hud.rs,
  bubble.rs, signs.rs, road.rs, minimap/raster.rs and game console,
  timetable and errand.
- Route: units run one at a time (the user's rule), each by a Sonnet writer.
  The reason is the long-session context backstop: the parent session is
  long.
- T1 (writer, uncommitted): `lettering.rs` added (Face, supports, measure,
  rasterize, Coverage::for_each); `ab_glyph = "=0.2.32"` direct dep (lock only
  gains the dependency edge, no new crate). 14 tests, RED observed against
  stubs, then GREEN; sabotaged (glyph check, kerning, advance, width rounding,
  baseline, descent, for_each, draw, row offset, embedded bytes, ink bounds,
  whitespace) and each was caught. Checks: `cargo test -p pampa-render` 551
  passed, clippy `-D warnings` clean, `cargo fmt --check` clean. Note: Rye has
  no kerning pairs; IM Fell italic kerns V/A. The "switch `font::supports`
  users" step is left to later tasks.
- T2 (writer, uncommitted): HUD (`hud.rs`), timetable panel and dev console
  letter with `Text` + `hud::ink_font(face, px)` (ink color). Sizes in logical
  px: clock 30 (`CLOCK_PX`), objective/ride/hint and the timetable and console
  lines 20 (`LINE_PX`), announcement 28 (`ANNOUNCEMENT_PX`). Roman body face
  everywhere, italic for the key hint. Plugins `init_resource::<Typefaces>()`.
  Empty timetable/console lines keep `min_height = LINE_PX`. Charset tests use
  `lettering::supports(Face::Body, c)`. README HUD line updated. Tests rewritten
  to assert Text string, display, font handle/size and ink color; HUD tests
  were RED against the bitmap impl (no `Text` on the lines), then GREEN;
  timetable/console tests proven by sabotage (prompt text, line order, prompt
  while open, font face, font size, prompt dash, dropped typing line, panel
  never hiding, clock/objective/ride/hint gating, ink color, hint face, sizes),
  each caught after fixing two weak tests (literal sizes, change-tick check,
  distinct font handle, console-hide assertion). Checks: `cargo test -p
  pampa-render hud` 7 passed; `cargo test -p game -- timetable console ride
  hud` 51 passed; clippy `-D warnings` clean; fmt clean; `cargo test
  --workspace` all green (one run saw `every_character_model_loads_skinned_
  within_the_triangle_budget` fail under load, passes alone and on rerun).
- T3 (writer, uncommitted): signs.rs, road signposts and bubble.rs paint with
  `lettering`. Boards: Rye (`Face::Display`) at `TEXT_PX` 56, `BORDER` 6,
  `PADDING` 20 texture px; size from the measured ink; linear sampler. Bubbles:
  IM Fell roman at `TEXT_PX` 38, `LINE_PITCH` 34 px = 0.264 m so
  `METERS_PER_PIXEL` = 0.264/34 = 0.00776 (was 0.022 with a 12 px pitch);
  outline 3, padding 11/9, corner 8, tail 11. Edges blend ink over paper or
  wood; wear keeps its hash noise. World sizes (before -> now): COMISARIA
  6x1.8 board 1.791 -> 1.793 m tall; general store 6x1.8 0.727 -> 0.701;
  HORARIOS 2.0x0.8 0.702 -> 0.581; "12 KM" 0.337 -> 0.339; bubble lines come
  out up to a third narrower (IM Fell is narrower than the 6 px cells; long
  3-line bubble 2.18 -> about 1.5 m wide), heights within 20%. Known limit: an
  accent over a descender on two stacked lines can touch by up to 3 px.
  Charset checks in errand.rs and complaints.rs use `Face::Body`; README and
  content docs reworded. Checks: `cargo test -p pampa-render -- signs bubble
  road` 37 passed; `cargo test -p game -- errand complaints` 20 passed; clippy
  clean; fmt clean; `cargo test --workspace` all green. Sabotage (each caught
  after strengthening: blended-threshold ratio, sharpness floor, tail
  narrowing): board height/min width/dx/dy/hard threshold/nondeterminism/
  frame/face/text px, bubble width/line offset/left/hard threshold/m-per-px/
  text px/block height/text_top/face/tail height/tail shape, charset
  predicate, road height limit.
- L5 (2026-10-28, user): "la iglesia no debe tener un cartel que diga
  "IGLESIA"". crates/core/src/places.rs: church places get no signboard; the
  place name stays "IGLESIA". The user's interim `" "` name was reverted.
  The small-town fingerprints were re-pinned in town.rs, because the
  places' signs are part of them. Test: only the church has no signs. RED
  was observed before the fix. Staged for the user's separate commit.
- T4 (writer, uncommitted): raster.rs paints labels with `lettering` (Face::Body):
  `LABEL_PX` 21 (capital about 14 px, as the bitmap at 2x), `SMALL_LABEL_PX` 11
  (compass letters, and a title that does not fit `size - 48` measured at 21).
  Ink bounds are centered on the spot, a paper halo (4-neighbours of pixels with
  coverage >= 96) goes under it, ink blends by coverage (`Canvas::blend`).
  `towns.rs` warns for station chars missing from Body or Display. Docs:
  dialogue README (the typeface has quotes, `;`, parentheses), CREDITS font
  rows. Tests: label/compass tests count dark pixels (strokes are thin), new
  tests for halo/antialias/centering, title fallback, `Canvas::blend`; the label
  test was RED against the bitmap. Sabotage caught: halo skipped/threshold/
  vertical/horizontal, blending off/math, centering x/y, fit forced/width,
  label and small px, compass letters, title y, place labels.
- L6 (2026-10-28, user): "el juego finalmente se llamara "Pampa Salvaje"".
  The user-facing name, the docs, the crate descriptions and comments, and
  the settings dir (`pampa-salvaje`) were renamed. Kept as they are: the repo
  folder, the `pampa-*` package names, the glTF generator strings in tools/
  and the exported models, and the historical odd/ docs.
- L7: Closing checks for T1–T4 plus the rename: fmt clean, clippy clean,
  workspace tests 207 + 32 + 456 + 544 pass, a 40 s smoke run is clean.
  Pending: the user deletes the orphaned `crates/render/src/font.rs` (rm was
  denied to the agents) and commits. The native review runs on that commit.
- L8: Native review. The whole range b5abdfb..HEAD (36 files, 2735 lines)
  stopped with `lens_context_budget_exceeded`. The user chose to review it
  in slices, using detached worktrees in the session scratchpad:
  - Slice 1, b5abdfb..e35863b: approved (review-6bd3434a2c6865d4).
    Advisories:
    - `settings.ron.tmp` is left behind when the temp write fails.
    - There is no `sync_all` before the rename.
    - Nothing tests how `play_music` applies its decision to the sink.
  - Slice 2 (old), e35863b..32fd09a: the reviewer found a CRITICAL. The user
    had committed the deletion of `font.rs` before the commits that migrate
    its users, so the commit did not build. The lineage was abandoned. With
    the user's agreement the commits were reordered by a non-interactive
    rebase: the deletion now comes after the typeface commit. Backup branch:
    `backup/game-typeface-before-reorder`. The final tree is identical.
  - Slice 2, e35863b..647e3cb: approved (review-90bcd192ac51c90e).
    Advisories:
    - `rasterize` does not validate px (overflow, NaN).
    - The notdef glyph and control chars are drawn silently.
    - No test covers the overhang clip.
    - ab_glyph has an exact pin.
  - Slice 3, 647e3cb..dfafe3a: escalated with unknown causality (R3-001).
    R3-001 claims `lettering` is missing, but it exists in the base 647e3cb
    (verified). It is a false positive from a diff-only view. Terminal
    native stop, informational.
  - Slice 4, dfafe3a..2d92730: approved (review-1cb6017a7c3d33bf).
    Advisories:
    - The settings dir rename has no migration from the old dir.
    - The font.rs deletion is unexplained in that slice (it is explained by
      the commit order).
- L9 (user): "correja nomás mientras reviso". The review advisories were
  fixed, uncommitted:
  - lettering: the canvas now grows to hold all the ink. A probe showed the
    italic f's swash, the roman Q's tail and the hooks of j, y and J were
    being clipped by tens of pixels, a visible bug in signs, bubbles and the
    minimap. `Coverage::for_each` now yields i32 line-box coordinates.
  - lettering: px that is not finite or not positive letters nothing; px is
    capped at MAX_PX = 512; control chars take no room.
  - ab_glyph uses a caret requirement (the lock is unchanged).
  - settings: the temp file is flushed with `sync_all` before the rename and
    removed on any failure. The sound.rs doc says the old pampa-seca dir is
    no longer read (renamed before any release, so there is no migration).
  - The minimap halo test allows a 2 px faint serif edge, measured: ink
    columns 56..144, paper columns 58..144.
  Not done: a play_music sink test. AudioSink needs a real audio device, and
  the decision is already the tested pure `playback`.
  Checks: fmt clean, clippy clean, and workspace tests 208 + 32 + 456 + 548
  pass. The flaky character-model test failed once in a crate run and
  passed alone. Each of the 5 new tests was sabotaged and each sabotage was
  caught. A 40 s smoke run is clean.
