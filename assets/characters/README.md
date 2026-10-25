# Characters

Skinned MakeHuman people. Each folder holds a character's source `.mhm` (MakeHuman's model file, written by hand), any recoloured materials (`.mhmat`), and the glTF built from them: `model.gltf`, `model.bin` and `textures/`. Credits and substitutions are in [CREDITS.md](CREDITS.md).

The game loads `protagonist`, `hidalgo`, `farach` and `bairoletto` (`crates/render/src/character/model.rs`) and draws them by default; the debug console's `models off` brings back the old rigid figures. The protagonist wears his model on foot and in the saddle: there he sits on the horse's `Torso` bone and plays the seated `drive` clip, his legs bent to the stirrups and his hand to the reins by a small IK, with springs for the give in his seat, spine, head and hands (`crates/render/src/character/rider.rs`). `models off` only applies to people on foot. The protagonist's poncho is kept off his left upper arm by the converter (`clear_arms` in `tools/mh2gltf.py`), so it rides on the shoulder when the arm moves.

They move with animation clips, all in one shared file, `animations/humanoid.glb`: Quaternius' CC0 Universal Animation Library 1 and 2, retargeted to the `game_engine` skeleton by `tools/retarget.py`. The clip driver (`crates/render/src/character/rig.rs`, with `clips.rs`) works like this:

- the idle fades into the walk, the jog and the sprint by ground speed;
- each walking clip is paced to the ground speed, so the feet keep still on the ground;
- Farach waits with folded arms, and whoever is speaking talks with their hands;
- each person starts their clips at their own point;
- the head glances around on top of the clip;
- a rider plays only the seated clip: a clip at no weight still sets the bones only it animates.

The clips find the bones by their name path from `Root` (`Root/pelvis/spine_01/...`), so a rebuilt model must keep the `game_engine` skeleton's bone names and hierarchy. The clips were made on the protagonist. Another model has its hips' moves scaled to its own legs when it plays them.

## Rebuilding the clips

You need the two packs (see [CREDITS.md](CREDITS.md); they are not in the repository). Run, from the repository root:

```sh
python3 -I tools/retarget.py \
  --ual1 "PATH/Animation Library[Standard]/Godot/AnimationLibrary_Godot_Standard.glb" \
  --ual2 "PATH/Universal Animation Library 2 [Standard]/Unreal-Godot/UAL2_Standard.glb" \
  --target assets/characters/protagonist/model.gltf \
  --model assets/characters/protagonist/model.gltf --model assets/characters/hidalgo/model.gltf \
  --model assets/characters/farach/model.gltf --model assets/characters/bairoletto/model.gltf \
  -o assets/characters/animations/humanoid.glb
```

The output is deterministic. It prints each clip with its length and, for the walks and runs, the ground speed and stride it measured. The game's `Clip::ground_speed` (`crates/render/src/character/clips.rs`) must match them; a test checks it. The `--model` lines give the men whose chests the folded arms and talking hands must clear. The standing clips are also calmed out of the packs' boxer's stance (see `tools/README.md`). Rebuild the clips after rebuilding any model.

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
