# Feature: signs-minimap

Locator: `odd/tasks/signs-minimap.md` · Engram mirror: `odd/signs-minimap/tasks` (pending, project not registered)

## Objective

Make the town easy to navigate: painted signs on the landmark buildings and a small minimap in the top-right corner.

## Problem / Why

Playtest feedback: it is hard to find your way around. Landmarks need readable names in the world, and the player needs an overview of where they are.

## Scope

- Core: named places (points of interest) derived from `TownLayout`:
  - The general store, police station, church, railway station and plaza, each with a display name, an anchor position, and a facing that matches the building's street side.
  - Diegetic sign text is in Spanish (period world content), e.g. "ALMACÉN DE RAMOS GENERALES", "COMISARÍA", "IGLESIA", "ESTACIÓN EDUARDO CASTEX", "PLAZA". Code and identifiers stay in English.
- Render: in-world signboards.
  - A procedural bitmap pixel font, with no third-party font. It covers A–Z, 0–9, basic punctuation and Spanish accented capitals (Á É Í Ó Ú Ñ Ü).
  - Text is painted onto a weathered wooden or painted-facade board texture and mounted on the building facade, plus a station name board on the platform.
- Render/game: minimap in the top-right corner.
  - Drawn from core data, not a second 3D camera: streets, blocks, buildings, plaza, railway, places as icons or initials, walkers as dots, and the train when present.
  - The player is shown as an arrow with its heading.
  - North-up by default; `M` toggles an enlarged map view.
  - Lightweight: the static layer is rasterized once to a texture, and only the markers update each frame.
- The minimap needs the Bevy UI feature (`bevy_ui`). Add only what is required.

Out of scope: a world map beyond the patch, quest markers, settings menus.

## Constraints

- Same architecture rules: logic never knows how things are drawn, and `pampa-core` stays dependency-free.
- Bevy stays at `=0.20.0`. No third-party art or fonts. Code is in English; diegetic sign text is in Spanish.

## Delivery strategy

`exception-ok`: fast-forward `main` after review, under the user's decision. Branch `feat/signs-minimap` from `main` (`fa635fe`).
Forecast: ~900 authored changed lines.
Review base: `fa635fe`.

## Tasks

| ID | Task | Route | Status | Commit | Review |
|----|------|-------|--------|--------|--------|
| T1 | core: named places from the town layout (test-first) | delegated (writer trigger: 2+ non-trivial files) | [x] | `af0e16a` | medium · granted · approved |
| T2 | render: bitmap pixel font + signboard textures + facade-mounted signs | delegated | [x] | `3f6f153` | medium · granted · approved |
| T3 | render/game: minimap (static raster + live markers, M to enlarge) + headless tests | delegated | [x] | `d71d73d` | medium · granted · approved |

## Acceptance criteria

- Each landmark shows a readable sign facing its street.
- The minimap shows the town, the railway, the places, walkers, the train and the player arrow, and it stays in sync with the world.
- `M` toggles the enlarged map.
- All checks pass; `pampa-core` has no Bevy dependency.

## Checks

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo build -p game`
- `timeout 15 cargo run -p game` (runs without panic)

## Progress / Evidence

- Branch `feat/signs-minimap` created from `fa635fe`.
- T1 (`af0e16a`, +389/-3): `pampa_core::places` — `PlaceKind` (Spanish sign names), `Place { kind, name, footprint,
  signs }`, `Sign { anchor, facing, mount: Facade | Posts, elevation, width }`, `TownLayout::places()`. Facade signs
  are centered 0.1 m outside the street facade (widths: store 13 m, police 7 m, church 3.4 m on the bell tower,
  station 14 m); the station adds a 7 m post board on the platform (track side, west of the canopy, elevation =
  platform height); the plaza has a 3 m post sign just inside its south edge, beside the central path. Ground post
  signs are collision obstacles (`TownLayout::obstacles` includes them). RED: 82 passed, 7 failed; GREEN: 89.
- T2 (`3f6f153`, +954/-2): `render::font` — 5x7 capitals in a 10-row cell (3 accent rows), A–Z, 0–9, space,
  `.,-:'`, Á É Í Ó Ú Ñ Ü; `text_width`, `ink_rows`, `for_each_pixel`. `render::signs` — `layout_board` (square
  pixels; the largest pixel that fits the text across and the height limit), `paint_board` (frame, padding,
  speckle wear, plank grain for timber, ~7% flaked letter pixels, nearest sampling), per-place `BoardStyle`
  (store green/gold, police white/blue, church lime/brown, station white/black, plaza timber/cream) and
  `SignPlugin` (facade boards on a backing; post boards double-sided on two posts). RED: font 0/7, signs 0/5;
  GREEN: font 7, signs 6 (incl. a headless spawn test: one board per sign, face normals match facings).
  Visual check by screenshots: store parapet, police above the door, church bell tower, station facade and
  platform board all read correctly (not mirrored, accents visible).
- T3 (`d71d73d`, +1500/-5, of which ~583 are `Cargo.lock`): Bevy features `bevy_ui` + `bevy_ui_render` (no
  `default_font`; `bevy_text` comes in transitively via `bevy_ui`). `render::minimap::raster` (pure):
  `MapFrame::for_town` (town extent + 30 m margin, squared; north-up), `to_map`, `contains`, `to_map_clamped`
  (6 px edge inset), `rasterize_town` (paper, streets, plaza with trees, railway with ticks, buildings by kind,
  place initials A/C/I/E/P at 2x with a paper halo, N marker), `draw_markers` (train cars on the rails, walker
  dots, player arrow along its heading with an ink outline). `render::minimap` plugin: 320 px base layer
  rasterized once and a 320 px marker layer rewritten each frame (PostUpdate), stacked UI `ImageNode`s in a
  framed panel (220 px top-right; `M` toggles 80 vmin centered). `MapIcon::{Player, Walker}` attached by the game.
  RED: raster 0/6; GREEN: 6. Plugin/headless tests written with the wiring (M toggles size, marker layer
  rewritten, no town no map, game: arrow follows the rider over 40 frames). Screenshots: corner map and enlarged map.
- Checks after T3: fmt ok; clippy `-D warnings` ok; tests core 89, render 46, game 29; `cargo build -p game` ok;
  `cargo tree -p pampa-core | rg -c bevy` no match; `timeout 15 cargo run -p game` exit 124.
- Advisory size heuristic exceeded in T2 and T3 (font tables and pixel tests are long; each task is one coherent change).

- Review: native RDD over `fa635fe..8685770` (medium, slice budget reached). Consent was granted; one lens (reliability) ran and the review was approved and acknowledged under lineage `review-f59534df11071319`.
  Advisory follow-ups (non-blocking):
  - No test proves that walker spawns, walker targets and the player spawn stay clear of the new plaza sign obstacle (`crates/core/src/town.rs:177`).
  - Sign placement hard-codes the terrain resolution (`crates/render/src/signs.rs:180`).

## Next step

Parent: native review of the work-unit commits (RDD), then the user's delivery decision.
