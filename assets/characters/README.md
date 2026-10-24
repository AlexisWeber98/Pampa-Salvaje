# Characters

Skinned MakeHuman people. Each folder holds a character's source `.mhm` (MakeHuman's model file, written by hand), any recoloured materials (`.mhmat`), and the glTF built from them: `model.gltf`, `model.bin` and `textures/`. Credits and substitutions are in [CREDITS.md](CREDITS.md).

The game loads `hidalgo`, `farach` and `bairoletto` (`crates/render/src/character/model.rs`) and draws them by default; the debug console's `models off` brings back the old rigid figures. The protagonist's model is built but not used yet: the player keeps the rigid figure until walking and riding are animated.

## Rebuilding a model

You need MakeHuman 1.x's data folder (only the data is read; the GUI is never started), the community asset packs the characters still use (see [CREDITS.md](CREDITS.md)) installed in it or in MakeHuman's user data folder, and Python 3 with numpy:

```sh
python3 -I tools/mh2gltf.py assets/characters/farach/farach.mhm -o assets/characters/farach
python3 -I tools/mh2gltf.py --check assets/characters/farach/model.gltf
```

The output is deterministic. A texture that a change no longer uses stays in `textures/` until you delete it: every file there should be named by its `model.gltf`.

## Writing an .mhm

These are MakeHuman's own lines (see `tools/README.md` for what the converter reads):

- `modifier macrodetails/Age 0.538462`: the age is `0.5` at 25 years and `1.0` at 90. Years above 25 map to `0.5 + (years - 25) / 130`.
- `proxy`, `eyes`, `eyebrows`, `hair`, `clothes`: a name and the asset's UUID, which is the one that counts.
- `material <name> <uuid> <file>`: a material for that proxy. The file is looked up beside the `.mhm` first, then in the data folders.
- `skeleton game_engine.mhskel`, `skinMaterial skins/.../x.mhmat`, `clothesHideFaces True`.

A recoloured `.mhmat` points at the asset's own texture with a `data/` path (for example `data/clothes/grinsegold_moustache/Moustache_black_diff.png`). Its `diffuseColor` multiplies that texture. It can darken or tint an asset, but it cannot lighten one, unless `greyscaleTexture True` turns the texture grey and white first (Don Hidalgo's white hair). Without a `diffuseTexture` the garment is a plain colour (Bairoletto's bombacha).

The converter also reads lines of its own. MakeHuman warns about them and skips them, so the files still load there:

| Line | Meaning |
|---|---|
| `decimate <name> <uuid> <triangles>` | Simplify the garment to at most that many triangles (UV seams and edges are kept, so it may stop above). |
| `piece <kind> key=value ...` | A garment cut from the body (`shirt`, `waistcoat`, `jacket`, `trousers`, `bombacha`, `boots`) or a generated piece (`hat`, `kepi`, `neckerchief`, `sash` (faja), `rastra`, `buttons`, `chain`, `strap`, `poncho`, `lapels`, `collar`). Their settings are in `tools/README.md`. |

Most clothes are cut from the body itself rather than taken from asset packs, which read modern: each garment is a copy of the body's skin over the part it covers, pushed out a little and skinned exactly like it, so it bends with the body. Only Bairoletto wears pack clothes: the harem pants as his bombacha and the riding boots.
