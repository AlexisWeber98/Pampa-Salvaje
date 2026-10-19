# Feature: bump-complaints

Locator: `odd/tasks/bump-complaints.md` · Engram mirror: `odd/bump-complaints/tasks` (pending, project not registered)

## Objective

First interaction: when the rider bumps into a townsperson with the horse, that person complains in a small speech bubble. The lines come from an editable data file, so non-programmers can write dialogue.

## Problem / Why

- The town has life but no interaction yet.
- Dialogue must be data-driven (user decision) so anyone in the group can write it without knowing Rust. This feature lays the foundation for future dialogue and missions.

## Scope

- Core (dependency-free): a bump event.
  - Fires when the rider's contact with a walker happens above a speed threshold.
  - Has a per-walker cooldown, so it does not spam.
  - Carries the severity (walk vs gallop) and the walker id.
  - Line selection is deterministic (seeded), avoids repeating the last line, and picks from a pool by severity.
- Content: a new crate `pampa-content`.
  - Uses `serde` + `ron` and loads `assets/dialogue/bumps.ron`.
  - Pools: walk bump, gallop bump; optionally per walker look.
  - Spanish lines in Rioplatense period voice, e.g. "¡Eh, mirá por dónde andás!" and "¡Casi me pisa el pingo, che!".
  - Validated by tests: the file parses, the pools are non-empty, and every character is supported by the bitmap font.
  - If the file is missing or invalid, the game falls back to a small built-in pool and logs a warning.
- Render: a speech bubble.
  - A world-space billboard above the walker's head, using our bitmap font. Extend the font with lowercase letters and `¡ ¿ ! ? á é í ó ú ñ ü ,`.
  - A rounded paper bubble with a tail, word-wrapped to a max width.
  - Shows for ~3 s with a fade.
  - One bubble per walker; a new line replaces the old one.
- Game: wire the event to content and bubble.
- Docs: a short `assets/dialogue/README.md` explaining how to write lines.

Out of scope: conversation trees, choices, the talk key (E), missions.

## Constraints

- `pampa-core` stays dependency-free. Parsing lives in `pampa-content`.
- Bevy stays at `=0.20.0` with lean features, and no third-party fonts.
- Code, comments, identifiers and docs are in English; dialogue lines are in Spanish (diegetic content).

## Delivery strategy

`exception-ok`: fast-forward `main` after review, under the user's decision. Branch `feat/bump-complaints` from `main` (`e7be342`).
Forecast: ~900 authored changed lines.
Review base: `e7be342`.

## Tasks

| ID | Task | Route | Status | Commit | Review |
|----|------|-------|--------|--------|--------|
| T1 | core: bump detection + cooldown + seeded line pick (test-first) | delegated (writer trigger: 2+ non-trivial files) | [x] | `d0fb9aa` | medium · granted · approved |
| T2 | pampa-content crate: RON dialogue loading, fallback, validation tests; `assets/dialogue/bumps.ron` + README | delegated | [x] | `077bbd7` | medium · granted · approved |
| T3 | render: lowercase + Spanish punctuation glyphs; speech bubble billboard with wrap and fade | delegated | [x] | `6578b2b` | medium · granted · approved |
| T4 | game: wire bump events → lines → bubbles; headless test | delegated | [x] | `01f3cb2` | medium · granted · approved |

## Acceptance criteria

- Galloping into a walker shows a gallop complaint above them. A slow bump shows a mild one, and brushing past at a crawl shows nothing.
- No bubble spam: a per-walker cooldown applies, and the same line does not repeat twice in a row.
- Editing `bumps.ron` changes the lines without recompiling.
- All checks pass; `pampa-core` has no new dependencies.

## Checks

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo build -p game`
- `timeout 15 cargo run -p game` (runs without panic)

## Progress / Evidence

- Branch `feat/bump-complaints` created from `e7be342`.
- T1 (`d0fb9aa`, +364/-44): `pampa_core::bumps` — `BumpSeverity::from_speed` (nothing below
  `BumpTracker::MIN_SPEED` = 1.5 m/s; `Gallop` exactly when the rider animation gallops, else `Walk`),
  `BumpEvent { walker, severity }`, `BumpTracker` (`tick`, `register`; 6 s per-walker cooldown started only by
  an event; duplicates in one contact list fire once), `LinePicker` (seeded xorshift; never the same index
  twice in a row per pool id when n > 1; empty pool → `None`; stale index after a pool shrinks is ignored).
  `Townsfolk::make_way` now returns the indices it shoved (contact detected where the shove happens). The
  private xorshift `Rng` moved from `townsfolk` to a crate-private `rng` module. RED: 89 passed, 9 failed;
  GREEN: 98.
- T2 (`077bbd7`, +430/-1, ~9 lines `Cargo.lock`): new crate `crates/content` (`pampa-content`; deps `serde`
  1.0.229 with derive, `ron` 0.12.2, `pampa-core`; no Bevy). `BumpLines { walk, gallop }` (unknown fields
  ignored for forward compatibility), `parse`/`load`/`validate` (non-empty pools, no blank lines,
  `MAX_LINE_CHARS` = 40 characters), `pool(severity)`, `builtin()`, `unsupported_chars(predicate)`,
  `load_bumps_or_builtin` (returns the error for the caller to log), `asset_path` (workspace `assets/`
  beside the sources via `CARGO_MANIFEST_DIR`, then `assets/` under the CWD). `assets/dialogue/bumps.ron`
  (11 walk + 10 gallop lines, Rioplatense 1920s–30s voice) and `assets/dialogue/README.md`. Decision: the
  font-charset check lives in the game crate (it sees both content and render), so content stays engine-free.
  RED: 0/8; GREEN: 8.
- T3 (`6578b2b`, +868/-26): `render::font` — cell grows to 12 rows (`BASELINE` = 9, `ASCENT` = 10, `X_TOP`
  = 5, 2 descender rows); 5x5 lowercase a–z with ascenders/descenders (g j p q y), á é í (dotless base) ó ú
  ñ ü with marks above the x-height, `! ?` and `¡ ¿` (upside down, descending); lowercase no longer maps to
  capitals; `supports`, `line_rows` (signs keep `ASCENT` rows, so boards are unchanged). `render::bubble` —
  `wrap` (16 chars/line, splits over-long words), `layout_bubble`, `paint_bubble` (rounded paper body, 1 px
  ink outline, 4-row tail opening the bottom edge, centered lines), `bubble_alpha` (3 s life, 0.6 s fade),
  `SpeechBubblePlugin` (`Say` message → one `SpeechBubble` child per speaker, replaced by a new line;
  textures cached by line; unlit, alpha-blended, no shadow; Y-axis yaw toward the camera; PostUpdate before
  transform propagation). RED: 38 passed, 22 failed (font, bubble, and signs via the `line_rows` stub);
  GREEN: 60.
- T4 (`01f3cb2`, +352/-5): `life` writes `Shoved { walkers, speed }` from `jostle_crowd` (speed before a
  pinned walker stops the rider); `complaints::ComplaintsPlugin` loads `Dialogue` at startup (warns and
  uses the built-in lines on error; warns on characters the font cannot draw), ticks the tracker, picks a
  line per event and writes `Say` for the walker entity. `main` adds `SpeechBubblePlugin` and
  `ComplaintsPlugin`. Headless tests: gallop bump → exactly one gallop line bubble on that walker; repeat
  charge within the cooldown → nothing, after it → a different gallop line; walk bump → walk line; crawl
  (walk_max 1.0) shoves but says nothing; shipped and built-in lines only use font characters; dialogue
  loads at startup when not provided. RED: 30 passed, 5 failed; GREEN: 35.
  Visual check by screenshots (temporary spawn/chatter edits, reverted): bubbles float above walkers'
  heads, face the camera, wrap to two lines, accents, `¡`, descenders and the tail read correctly.
- Checks after T4: fmt ok; clippy `-D warnings` ok; tests core 98, content 8, render 60, game 35;
  `cargo build -p game` ok; `cargo tree -p pampa-core | rg -c bevy` and `cargo tree -p pampa-content | rg -c
  bevy` no match; game binary run 15 s: exit 124 (timeout), no panic, no warnings.
- Advisory size heuristic exceeded in T3 (font table and pixel tests are long; one coherent change).

- Review: native RDD over `e7be342..c684935` (medium, slice budget reached). Consent was granted; one lens (reliability) ran and the review was approved and acknowledged under lineage `review-70476fa7ab4f4f96`.
  Advisory follow-ups (non-blocking):
  - Two `Say` messages for the same speaker in one frame can leave two overlapping bubbles (`crates/render/src/bubble.rs:295-306`).
  - The complaint cooldown may be consumed without a line being spoken (`game/src/complaints.rs:86-95`).

## Next step

Native review of the slice (owned by the parent), then the user's delivery decision.
