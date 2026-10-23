# MakeHuman characters

## Objective

Replace the rigid low-poly people of the errand (Don Hidalgo, comisario Farach and Juan Bautista Bairoletto), and later the protagonist, with skinned MakeHuman characters built offline from CC0 assets. They must stay within a game budget of about 6–15k triangles each, and be animated later.

## Plan

1. **This PR: converter, models, wiring behind a toggle.**
   - `tools/mh2gltf.py` (with `tools/test_mh2gltf.py`) turns a hand-written `.mhm` into a skinned glTF using MakeHuman's data only.
   - Four characters are written and converted in `assets/characters/`.
   - The game loads Hidalgo, Farach and Bairoletto (the new bystander at Farach's side) in a static pose, with arms lowered and a subtle breath.
   - **User decision:** the CC0 wardrobe reads modern, so the rigid 1919 figures stay the default. The models show only after the debug console's `modelos on` (`personajes nuevos`). `modelos off` (`personajes viejos`) restores the figures, and release builds never show them.
2. **Period wardrobe PR.**
   - Procedural 1919 garments and accessories bound to the `game_engine` joints: bombacha, waistcoat, neckerchief, faja and rastra, a kepi with a visor and a brass-buttoned tunic for Farach, distinct hats (gacho, chambergo, wide brim), and moustache and beard pieces.
   - Stronger macro differences: Hidalgo clearly about 60, Bairoletto slighter and with lighter hair.
   - Then make the MakeHuman figures the default.
3. **bevy_animation and clips.** Enable `bevy_animation` (it needs a network fetch; it is not in the cargo cache), add idle, walk and ride clips for the `game_engine` rig, and move the protagonist onto his model.
4. **The comisaría scene.** Farach and Bairoletto at the Castex police station, staged.

## Tasks (step 1)

- [x] T1 Converter `tools/mh2gltf.py`. It covers macro and universal morphs, proxy fitting, clothes face hiding, the `game_engine` skin with at most 4 normalized influences, `.mhmat` materials, downscaled PNG textures, deterministic output and `--check`. Route: delegated writer.
- [x] T2 `tools/test_mh2gltf.py`: synthetic unit tests, plus a real-data conversion of Farach when MakeHuman is installed.
- [x] T3 Four `.mhm` files with their recoloured `.mhmat`s, converted.
- [x] T4 Game wiring:
  - `FigureKind::Bairoletto` with a rigid fallback look.
  - `Errand::bairoletto()` standing 1.1 m to Farach's left, with its collision footprint.
  - `Cast` bystanders spawned and despawned with their town.
  - `character/model.rs`: models loaded at startup, a rigid stand-in while loading or on failure, the props' shared `VisibilityRange`, lowered arms and a breath on `spine_02`.
  - `CharacterModelsShown` toggle and the console's `modelos` command.
- [x] T5 Credits (`CREDITS.md`, `assets/characters/CREDITS.md`), `README.md`, `tools/README.md`, `assets/characters/README.md`.

## Results

| Character | Triangles | Folder size | Textures |
|---|---|---|---|
| protagonist | 13,796 | 4.2 MB | skin and outfit at 1024 px; hair, shoes, hat, eyes, eyebrows and normal maps at 512 px |
| bairoletto | 13,408 | 4.1 MB | same |
| farach | 13,434 | 3.9 MB | same |
| hidalgo | 13,824 | 3.9 MB | same |

The body proxy is `male1591` (3,140 triangles; 2,276 left once the faces under the clothes are hidden). The skeleton is `game_engine` with 53 joints. The bodies stand 1.73–1.77 m without the hat.

## Substitutions

MakeHuman's CC0 wardrobe has jeans, modern shirts, one fedora and modern shoes. The full table is in `assets/characters/CREDITS.md`. In short:

- jeans for bombacha and trousers, and shoes for boots;
- `male_casualsuit05` (field jacket) for the protagonist's leather jacket (tinted brown) and for Hidalgo's good jacket;
- `male_casualsuit02` darkened for Bairoletto's black shirt;
- `male_casualsuit03` darkened to navy for Farach's tunic;
- fedoras for the gacho, chambergo, kepi and wide-brimmed hat;
- no neckerchief, beard, moustache, waistcoat, brass buttons or white hair.

## Decisions

- **Body proxy:** `male1591` is the game-oriented low-poly body. `proxy741` is too coarse, and the 13k proxies alone break the budget.
- **Heavy suits:** `male_casualsuit01` and `male_elegantsuit01` (15–17k triangles alone) are not used.
- **Recolouring:** a `.mhmat` beside the `.mhm` (MakeHuman's own `diffuseColor`, multiplied into the texture).
- **Hair under hats:** dropped above the hat's brim, a converter step MakeHuman does not have. Without it the hair pokes through the fedora.
- **Eyelashes:** left out (368 triangles, invisible at game distance).
- **Orientation:** the glTF faces +Z and the figure body's front is +X, so the model root turns 90° about Y.
- **Draw range:** the models use the settlement props' shared range (`PROP_FAR`, 220 m), so no new GPU visibility-range slot is used.
- **Bairoletto:** a `Bystander`, not an errand `Npc`. He has no dialogue yet, but he blocks the way like the others.

## Checks

- `python3 -I -B -m unittest discover -s tools`
- `python3 -I -B tools/mh2gltf.py --check assets/characters/<name>/model.gltf` for each character
- `cargo test -p pampa-core`, `-p pampa-render`, `-p game`
- `cargo build -p game`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`

## Next step

In game, check `goto castex` (Farach with Bairoletto at his left) and `goto nievas` (Hidalgo), with `modelos on` and `modelos off`. Then the period wardrobe PR.
