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
| T1 | core: bump detection + cooldown + seeded line pick (test-first) | delegated (writer trigger: 2+ non-trivial files) | [ ] | | |
| T2 | pampa-content crate: RON dialogue loading, fallback, validation tests; `assets/dialogue/bumps.ron` + README | delegated | [ ] | | |
| T3 | render: lowercase + Spanish punctuation glyphs; speech bubble billboard with wrap and fade | delegated | [ ] | | |
| T4 | game: wire bump events → lines → bubbles; headless test | delegated | [ ] | | |

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

## Next step

T1.
