# spanish-docs

Translate every code comment and the developer docs into Spanish, so a new developer can read and understand them.
Branch: docs/spanish-comments (base 6b5dc41). Delivery: one commit per crate (core, render after T2+T3, content, game + READMEs), made by the agent once each crate's checks pass, per L3. Runner: cargo (fmt --check, clippy -D warnings, test --workspace incl. doctests, build -p game).

## Specs

- S1: "poneme toda la docu y los ocmentarios en español (puede que venga otro dev y necesita leerlo y ententderlo)". Scope chosen by the user: option 1: "Lo que lee un dev nuevo: los comentarios de todo el código, más README, tools/README y los README de assets. Dejo afuera los CREDITS (son atribuciones de licencias, conviene no tocarlos) y `odd/tasks/` (son bitácoras históricas internas)."
- S2: Every `//`, `///`, `//!` and `/* */` comment in `crates/*` and `game/src`, plus `//` comments in tracked `.ron` data files, is in neutral, professional Spanish (not regional slang). Identifiers, string literals, log/UI text, and code inside doc examples stay unchanged.
- S3: README.md, tools/README.md, assets/characters/README.md, assets/dialogue/README.md, assets/models/third-party/README.md and assets/world/README.md are in Spanish. Commands, paths, code blocks and proper names stay as they are. CREDITS*.md, odd/ and todo.md are out of scope.
- S4: Behaviour is unchanged. Only comment and Markdown text changes. fmt, clippy, tests (incl. doctests) and the game build still pass.

## Tasks

| ID | Specs | Scope | Route | Status |
|----|-------|-------|-------|--------|
| T1 | S2, S4 | crates/core (~4.7k comment lines) | delegated (context backstop) | done, committed by the user as a5a111a |
| T2 | S2, S4 | crates/render subdirectories (character, facade, fauna, minimap, settlement, sky, train, tree) | delegated (context backstop) | done, committed by the user as 037eddf |
| T3 | S2, S4 | crates/render/src/*.rs top-level files | delegated (context backstop) | done, committed by the user as 92de66a |
| T4 | S2, S3, S4 | game/src, crates/content, tracked .ron comments, the six READMEs | delegated (context backstop) | done, staged (34 files; workspace fmt/clippy OK, 1240 tests pass, game builds) |

Route note: ~13.6k comment lines cannot be read inline without blowing the parent context, so each unit goes to one writer. They run one at a time, by the user's standing preference.

## Log

- L1 (2026-10-29, user): "poneme toda la docu y los ocmentarios en español (puede que venga otro dev y necesita leerlo y ententderlo)"
- L2 (2026-10-29, user): picked scope option "1".
- L3 (2026-10-29, user): "vamos commiteado por crates"
- L4 (2026-10-29, user): "cuando termine staggea por favor". T1 gets staged when it finishes, and the commit waits for the user's go-ahead.
- L5 (2026-10-29): T4 closing fixes made by the parent:
  - The heading "### Adding another town" in assets/world/README.md is now "### Agregar otro pueblo". The README test in crates/content/src/towns.rs:513 looks for that literal, so it was updated too. A sabotage of the heading made the test fail, and it passes again after restoring.
  - The `//` comments in the ```ron examples of the dialogue, world and third-party READMEs now match the translated .ron files.
  - "saltear" (valid Spanish, used across T1–T3) was left as is.
