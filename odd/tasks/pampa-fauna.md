# Pampa fauna

Objective: the wild fauna of the 1919 pampa: zorro, ñandú, mulita, peludo, puma, yaguareté, chimango, carancho, nutria, vizcacha, gato montés. Branch `feat/pampa-fauna`, from main (a8848c2). Delivery: the user owns commits (left uncommitted). Test runner: `cargo test -p pampa-core`, `-p pampa-render`, `-p game` (focused, capped at MemoryMax=5G, --test-threads=2), one full check at the end.

## Specs

- S1. "agregar fauna: zorros, ñandú, armadillo (mulita, peludo), puma, yaguareté, Chimango, carancho, nutria, bizcacha, gatos monteses" (todo.md). Standing directive (todo.md): "descargar modelos renderizados de terceros de libre uso siempre que sea posible, con el mayor detalle posible mientras sea soportado por el hardware actual" (Renoir iGPU budget).
- S2. Models (user approved the downloads, 2026-10-29, "Sí, descargá los dos"):
  - S2a zorro: Quaternius Fox, CC0, animated glb, poly.pizza/m/Bc97C66HKi → `~/Downloads/quaternius/fox/fox.glb`.
  - S2b gato montés, puma, yaguareté: Quaternius Animal Pack Vol.2 Cat, CC0 (opengameart.org/content/animated-animales-low-poly) → `~/Downloads/quaternius/animal-pack-2/animal-pack-vol2.zip` (Blend/FBX only; convert with the installed Blender). One rig, three sizes and coats: gato montés small grey with spots, puma large tawny, yaguareté large spotted.
  - S2c chimango and carancho: today one rigid `Raptor`; split into two species with their own look. Assumption: try the pack's animated Eagle (same zip) for both, tinted (chimango slim brown-buff, carancho dark with a pale neck and red-orange face); if it does not suit, keep improved rigid models.
  - S2d ñandú, mulita, peludo, vizcacha, nutria: no free models exist; build them procedurally (rigid `FaunaKit` path like the hare and the tero).
  - Credit every third-party asset next to it and in the root `CREDITS.md`. Sketchfab is unusable; no other downloads without asking.
- S3. Behaviour (assumption): ground animals flee from the rider like the hare (zorro, ñandú running fast in a straight line, mulita/peludo slow, vizcacha to its burrow colony, gato montés, puma); the nutria swims in lagoons like the ducks; raptors circle/perch like today's raptor.
- S4. Habitat and rarity (assumption): felines near the monte (caldén woods/flora); vizcachas in colonies on open ground; ñandúes in small groups on open fields; armadillos and foxes on open ground; nutrias on lagoons. Yaguareté: "Incluilo (muy raro) avistamiento excepcional" (user, 2026-10-29) — extinct in the region around 1903–1925, so a very rare sighting in the monte.
- S5. Docs: README fauna section and the asset credits.
- S6. Second batch (user, 2026-10-29, pasted text): "guanacos, venados de las pampas (Ozotoceros bezoarticus), pumas (Puma concolor) y yaguaretés (Panthera onca), además de armadillos, vizcachas, zorros y comadrejas" and "ñandúes (Rhea americana), loicas pampeanas, tordos amarillos, flamencos y batatúes, junto con grandes herbívoros como el ciervo de los pantanos y el carpincho". New species: guanaco, venado de las pampas, comadreja, loica pampeana, tordo amarillo, flamenco, batitú, ciervo de los pantanos, carpincho. The yaguareté stays a very rare sighting (S4); the ciervo de los pantanos only rare, by the lagoons (assumption).

## Tasks

- T1 [S1–S5] Prepare the models (convert/trim like `tools/glbtrim.py`), add the species to core (behaviour, habitat, caps), render (skinned and rigid), and game streaming; tests; docs. Route: delegated writer (Opus), reason: long-session context backstop. Status: done, uncommitted (the user owns commits); checks in L6.
- T2 [S6] Second batch of species. Route: TBD after T1. Status: pending.

## Log

- L1 (2026-10-29) user: "mamos con fauna".
- L2 Map: species today are cows, bulls, corral horses, hares, teros, one rigid `Raptor` (chimango/carancho), ducks (`crates/core/src/fauna*`, `crates/render/src/fauna*`, `game/src/fauna.rs`); skinned glTF path in `crates/render/src/fauna/livestock.rs` (Quaternius cow/bull/horse, CC0); game caps arrays `CAPS: [usize; 6]` must widen; no monte habitat yet.
- L3 (2026-10-29) user approved downloading the Quaternius Fox and Animal Pack Vol.2; both downloaded to `~/Downloads/quaternius/`.
- L4 (2026-10-29) yaguareté: user chose "Incluilo (muy raro) avistamiento excepcional".
- L5 (2026-10-29) user pasted a list of 1900s pampa fauna and chose "Segunda tanda" → S6, T2 (after T1).
- L5 (2026-10-29) T1 writer: models. Fox trimmed with `tools/glbtrim.py` (7 clips, 1,848 tris). Cat and Eagle converted from the pack's FBX with the new Blender script `tools/fbx2glb.py`, then `glbtrim.py`: cat 807 tris (Idle, Walk only: felines flee with the walk sped up, up to 2.5x); eagle decimated 24,800 -> 2,400 tris, clips Glide (added rest pose, wings spread) and Flap. Eagle used for both chimango and carancho (tinted); S2c assumption holds. Core: `Critter` (`crates/core/src/fauna/critter.rs`) for the eight ground species, seeded per chunk with their own rng kinds 4..11 (existing species unchanged); habitat from `FloraSite::monte` passed in by the game (felines monte >= 0.5, ñandú and vizcacha < 0.2); vizcachas flee into their vizcachera; `Raptor.kind` (chimango 60 %, drawn last so circles did not move); nutrias per lagoon on the `Duck` simulation. Caps: zorros 3, ñandúes 10, armadillos 4, vizcachas 12, felines 3, nutrias 6 (raptors stay 4). Render: `Species` Fox, GatoMontes, Puma, Yaguarete, Chimango, Carancho with per-species clip maps, sizes, tints and vertex spots; rigid ñandú, mulita, peludo, vizcacha, nutria. Game: 12 counted kinds (`Fauna::all_counts`; `counts()` keeps the six the console prints, because `game/src/console.rs` was outside the edit surface).
- L6 (2026-10-29) T1 verification: `cargo fmt --all --check` clean; `cargo clippy -q --workspace --all-targets` no output; workspace tests (MemoryMax=5G, 2 threads) game 198, core 469, render 549 passed (5 ignored), all green; `python3 -I -B -m unittest discover -s tools` 152 OK. Every new or changed test was sabotaged and seen failing. Open: the console `fauna` command still prints six kinds (`game/src/console.rs` outside the surface); the cat's upright tail and walk-only clip make felines look domestic when fleeing; time of day is ignored (vizcachas are crepuscular).
- L7 (2026-10-29) parent: the console `fauna` command now prints all 12 kinds (`Fauna::all_counts`); the six-kind `Fauna::counts` was dropped (no other user). Spot check: clippy clean, `cargo test -p game` 198 pass.
