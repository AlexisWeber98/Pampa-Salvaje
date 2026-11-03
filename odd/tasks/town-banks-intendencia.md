# Feature: town-banks-intendencia

Objective: a bank building in cities and towns (none in villages) and the Intendencia in General Pico. Branch `feat/pampa-fauna` (shared with the fauna writer; the user owns commits). Test runner: `cargo test` (core, render, game), capped with `systemd-run --user --scope -p MemoryMax=5G` and `--test-threads=2`.

## Specs

- **S1** — "agregar banco en ciudades, uno en pueblos ninguno en aldeas"
  - Categories follow the existing `Paving::of(blocks)` tiers: `Everywhere` = ciudad (General Pico), `Plaza` = pueblo (Eduardo Castex, Monte Nievas, Caleufú), `None` = aldea (Metileo, ≤ 9 blocks).
  - S1a — a ciudad gets `2` banks. **Assumption** (the count is unspecified): two, one on each side of a line that crosses the town, named BANCO DE LA NACIÓN ARGENTINA (the state bank, with branches across the pampa railway towns since the 1890s–1910s) and BANCO ESPAÑOL DEL RÍO DE LA PLATA (the largest private bank of the period, with branches in pampa towns).
  - S1b — a pueblo gets exactly one bank (BANCO DE LA NACIÓN ARGENTINA).
  - S1c — an aldea gets none.
  - **Assumption**: "banco" means a bank building (the institution), not a plaza bench (plazas already have benches).
- **S2** — "agregar intendencia en General Pico"
  - Only the town named General Pico gets the Intendencia (sign "INTENDENCIA MUNICIPAL"), facing its main plaza from across the street.
  - **Assumption**: chosen by name in core (`TownConfig::name`); a `towns.ron` flag would need the content crate and `assets/world/towns.ron`, outside this writer's surface.
- **S3** — Quality: "con el máximo de calidad, realismo y detalle posible"; free third-party models/textures allowed, credited. Buildings use the existing italianate facade kit (academic 1919 façades: stucco, pilasters, arched openings, balustrade, pediment) and its CC0 textures; no new download.
- **S4** — Existing behaviour holds: placement is deterministic, existing landmarks (stores, comisarías, church, station) and windmills keep their blocks; banks and the Intendencia get signs, collision, palenques and the minimap landmark colour like the other civic buildings.

## Tasks

| ID | Specs | Route | Status | Evidence |
|----|-------|-------|--------|----------|
| T1 | S1, S2, S4 | inline (delegated writer for this feature) | done, uncommitted (user owns commits) | RED observed for S1a/S1b/S2; GREEN; every new test sabotage-proven |
| T2 | S3, S4 | inline | done, uncommitted | `facade/town_buildings/civic.rs`; sign/minimap/footprint/budget tests sabotage-proven |
| T3 | S3 | inline | done | shots in a scratch dir (reported to the parent) |

## Log

- **L1** (2026-11-02, user, verbatim from `todo.md`): "agregar banco en ciudades, uno en pueblos ninguno en aldeas
  agregar intendencia en General Pico"
- **L2** (2026-11-02, user): "tirame un secundo agente en parealelo con la tarea de todo.md, la los bancos y la intendencia"
- **L3** (2026-11-02, writer): Core — `BuildingKind::{Bank, TownHall}`, `Paving::banks()` (0/1/2), `TownConfig::{bank_count, has_town_hall, TOWN_HALL_TOWN}`, `PlaceKind::{Bank, TownHall}` and `BANK_NAMES`. `place_landmarks` adds the Intendencia (`place_landmark`, facing the plaza: south, east, west, north) and the banks (`disperse`, like the comisarías) after every existing landmark and off the windmill blocks; a bank takes a windmill block only when no other block keeps `CIVIC_SPACING` (sample town "Este"). Banks and the Intendencia get palenques and landmark trees. Differential check against HEAD over the five shipped towns: every pre-existing landmark, windmill, tank, grain store and place is identical. Ten small-town fingerprints re-pinned (the bank block loses its houses); "Tres" (an aldea) unchanged.
- **L4** (2026-11-02, writer): Render — `civic::bank` (grey stucco academic front, rusticated ground course, arched glazed openings with keystones and grilles, giant pilasters, plain frieze carrying the bronze-lettered name, dentils, balustrade with urns, pediment; ~4.3k triangles) and `civic::town_hall` (two-storey ochre front, central pavilion with arched door, in-plane balcony, attic carrying "INTENDENCIA MUNICIPAL", pediment with the municipal clock, flagpole; ~7.4k triangles). `italianate` now shares `back_and_sides` with them (same build order). Signs: bronze-on-stone for banks, dark-on-ochre for the Intendencia. Minimap paints both as landmarks. No third-party asset added (existing CC0 facade textures only).
- **L5** (2026-11-02, writer): Full check — `cargo fmt --check` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo test -p pampa-core` 485 ok, `-p pampa-render` 582 ok (5 ignored), `-p game` 203 ok, `-p pampa-content` 32 ok.
