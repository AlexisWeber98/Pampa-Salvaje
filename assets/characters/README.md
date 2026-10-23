# Characters

Skinned MakeHuman people. Each folder holds a character's source `.mhm` (MakeHuman's model file, written by hand), any recoloured materials (`.mhmat`), and the glTF built from them: `model.gltf`, `model.bin` and `textures/`. Credits and substitutions are in [CREDITS.md](CREDITS.md).

The game loads `hidalgo`, `farach` and `bairoletto` (`crates/render/src/character/model.rs`). They are drawn only when the debug console's `modelos on` turns them on; the rigid figures are the default. The protagonist's model is built but not used yet: the player keeps the rigid figure until walking and riding are animated.

## Rebuilding a model

You need MakeHuman 1.x's data folder (only the data is read; the GUI is never started) and Python 3 with numpy:

```sh
python3 -I tools/mh2gltf.py assets/characters/farach/farach.mhm -o assets/characters/farach
python3 -I tools/mh2gltf.py --check assets/characters/farach/model.gltf
```

The output is deterministic. A texture that a change no longer uses stays in `textures/` until you delete it.

## Writing an .mhm

These are MakeHuman's own lines (see `tools/README.md` for what the converter reads):

- `modifier macrodetails/Age 0.538462`: the age is `0.5` at 25 years and `1.0` at 90. Years above 25 map to `0.5 + (years - 25) / 130`.
- `proxy`, `eyes`, `eyebrows`, `hair`, `clothes`: a name and the asset's UUID, which is the one that counts.
- `material <name> <uuid> <file>`: a material for that proxy. The file is looked up beside the `.mhm` first, then in the data folder.
- `skeleton game_engine.mhskel`, `skinMaterial skins/.../x.mhmat`, `clothesHideFaces True`.

A recoloured `.mhmat` points at the garment's own texture with a `data/` path (for example `data/clothes/male_casualsuit03/male_casualsuit03_diffuse.png`). Its `diffuseColor` multiplies that texture. It can darken or tint a garment, but it cannot lighten one.
