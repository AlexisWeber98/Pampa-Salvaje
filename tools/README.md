# Tools

Small scripts for preparing assets. They are not part of the game build.

## `textures_to_ktx2.sh`: building textures to KTX2

Converts the building texture sets in `assets/textures/buildings/<set>/` to the GPU-compressed KTX2 files the game loads. It needs `basisu` (Basis Universal 2.x) and ImageMagick 7 (`magick`), and is POSIX `sh`:

```sh
FORCE=1 sh tools/textures_to_ktx2.sh assets/textures/buildings/*/
sh tools/textures_to_ktx2.sh --check-orm
```

From each set's `color.jpg`, `normal_gl.jpg`, `roughness.jpg` and optional `ao.jpg` it writes:

| File | Contents |
|---|---|
| `color.ktx2` | sRGB colour |
| `normal_gl.ktx2` | OpenGL (+Y) normal map, linear, basisu's normal-map mode |
| `orm.ktx2` | linear; red = ambient occlusion (white without `ao.jpg`), green = roughness, blue = metallic (white; the material's `metallic` scales it) |

All three are UASTC LDR 4x4 with mipmaps baked in and Zstandard supercompression. Bevy transcodes UASTC at load to the GPU's block format (BC7 on desktop GPUs, ASTC or ETC2 elsewhere): 1 byte a texel in video memory instead of RGBA8's 4. No RDO is used: on these sets it saved about 3% of disk for about 1 dB of PSNR.

Without `FORCE=1` it refuses to overwrite existing `.ktx2` files. The JPGs stay the editable sources; re-run it after changing one. `--check-orm` converts a synthetic set with and without `ao.jpg`, unpacks its `orm.ktx2` to BC7 and fails unless each channel holds what it should.

The ground textures are not converted: the game blends the PNM maps into one detail texture on the CPU at startup, and has no encoder to compress the result.

## `dae2gltf.py`: COLLADA to glTF

Converts a COLLADA (`.dae`) model into a glTF 2.0 model slot (see `assets/models/third-party/README.md`). It uses the Python 3 standard library only. Run it isolated, so nothing next to the input or in the current folder is imported:

```sh
python3 -I tools/dae2gltf.py INPUT.dae -o OUTPUT_FOLDER [options]
python3 -I tools/dae2gltf.py --check OUTPUT_FOLDER/model.gltf
```

It writes `model.gltf`, `model.bin` and a `textures/` folder into the output folder, then prints the triangle counts, the bounds in meters, the rotor's pivot and axis, and what it did with each texture.

| Option | Meaning |
|---|---|
| `--rotor NODE` | Keep the geometry under this COLLADA node (name or id) apart, as a `rotor` node with its origin at the node's pivot. Its spin axis comes from the node's animated rotation; put it in `slots.ron`. |
| `--rotor-axis X,Y,Z` | The spin axis (glTF space) when the rotor is not animated. |
| `--rotor-name NAME` | The rotor's glTF node name (default `rotor`; `body` is refused, it names the static node). |
| `--scale S` | An extra uniform scale. |
| `--metallic M` | Metallic factor of every material (default 0). |
| `--normal auto\|gl\|dx` | Green convention of the normal maps. `auto` (the default) decides per map: a real height field's normals are curl-free for one sign of green only. DirectX maps get their green flipped, since glTF wants OpenGL. `gl` copies the maps without decoding them; `dx` flips every map (PNG only). The report names the convention applied to each map and whether it was detected, forced or assumed (a JPEG normal map is not inspected and is taken as GL). |
| `--single-sided` | Cull back faces (materials are double-sided by default: game models often have single planes). |
| `--check FILE` | Only validate a written glTF: buffers, views, accessors, indices and images in range (a negative or missing index is a fault), every URI inside the model's folder, and every image a PNG or JPEG by type, extension and content. |

What it reads:

- `<triangles>`, `<polylist>` and `<polygons>`, fanned into triangles; lines and holes are skipped;
- positions, normals (or face normals when missing) and the first bound texture set;
- node transforms (`matrix`, `translate`, `rotate`, `scale`), baked into the vertices; a negative scale flips the winding;
- `<up_axis>` and `<unit meter>`, to Y-up meters;
- per material: the diffuse texture or colour, the normal map (a `<bump>` texture, else a `<diffuse>_NRM.png` beside it), and a roughness from the shininess. Specular maps are ignored.

The input is treated as untrusted. A file over 64 MB, a document with a DTD or entities, a non-finite number, an out-of-range index, or an oversized array is refused. The DTD refusal is a byte search for `<!DOCTYPE` and `<!ENTITY`, so a document that is not UTF-8 (a UTF-16 or UTF-32 file, or any other declared encoding) is refused first: those bytes could hide from the search. Texture paths are reduced to their base name and looked up only in the model's own folder: the exact name first, else one case-insensitive match (several are refused as ambiguous). Textures must be PNG or JPEG, the only formats core glTF allows. Nothing is evaluated. Invalid input gets a one-line error; any other exception is a bug in the script and keeps its traceback.

Accessor `min`/`max` bounds are the float32 values actually stored, so validators see them match the buffer exactly.

### Tests

A standard-library `unittest` suite builds tiny COLLADA documents and PNGs in a temporary folder and covers the axis and unit conversion, triangles and polylists, the rotor split, the normal-map convention, the refusals and `--check`. `test_mh2gltf.py` does the same for `mh2gltf.py` (`.obj` and `.mhclo` reading, data folders, materials, decimation, the garments cut from the body, hats and the other generated pieces, cloth textures, the arm pose) and converts Farach when MakeHuman's data is installed:

```sh
python3 -I -B -m unittest discover -s tools -v
```

The conversions done so far are listed in `assets/models/third-party/3td-starter-pack/CREDITS.md`.

## `mh2gltf.py`: MakeHuman character to skinned glTF

Converts a MakeHuman character (`.mhm`) into a skinned glTF 2.0 model (see `assets/characters/README.md`). It needs Python 3 with numpy and MakeHuman 1.x's *data* folder: `--data`, else `$MAKEHUMAN_DATA`, else `~/.local/opt/makehuman-1.3.0/makehuman/data`. Assets are also looked up in MakeHuman's user data folder, where downloaded asset packs go, and there first, as MakeHuman does: `--user-data`, else `$MAKEHUMAN_USER_DATA`, else `~/Documents/makehuman/v1py3/data`, when it exists (`--no-user-data` skips it). Only the data is read; none of MakeHuman's code runs and its GUI is never started. Run it isolated:

```sh
python3 -I tools/mh2gltf.py INPUT.mhm -o OUTPUT_FOLDER [options]
python3 -I tools/mh2gltf.py --check OUTPUT_FOLDER/model.gltf
```

It writes `model.gltf`, `model.bin` and `textures/` (PNG). Units are meters, Y is up, the model faces +Z and its lowest point is on y = 0. There is one skinned mesh node with one primitive per proxy, and the skeleton's joint hierarchy. It prints the triangles and bounds of each primitive, the textures it wrote, and anything it had to work around.

| Option | Meaning |
|---|---|
| `--data DIR` | MakeHuman's data folder |
| `--user-data DIR` | MakeHuman's user data folder, searched first |
| `--no-user-data` | Search the data folder only |
| `--max-texture N` | Largest side of the skin and the garments that cover the body (default 1024) |
| `--max-small-texture N` | Largest side of the other textures and of the normal maps (default 512) |
| `--no-normal-maps` | Leave the normal maps out |
| `--keep-hair-under-hats` | Keep the hair above a hat's brim |
| `--check FILE` | Only validate a written glTF. It checks what `dae2gltf.py --check` does, and also the skin: every joint is a node, there is one invertible inverse bind matrix per joint, and every vertex has joints in range and non-negative weights that sum to 1 |

What it reproduces from MakeHuman's sources (`apps/humanmodifier.py`, `apps/human.py`, `lib/targets.py`, `shared/proxy.py`, `shared/skeleton.py`, `shared/animation.py`):

- **Morphing.** The base mesh is morphed by every modifier: the `.mhm`'s values and the defaults of the rest. Macro modifiers (gender, age, ethnics, muscle, weight, height, proportions, breast) go through the macro variables' interpolation and each target's factors. The others use their decr/incr targets.
- **Proxies.** The body proxy (one `proxy` line is required, since the base mesh is too dense for a game) and the eyes, eyebrows, eyelashes, hair and clothes are fitted to the morphed mesh by their reference vertices, weights, offsets and scale matrix. A proxy is read from MakeHuman's compiled `.mhpxy` or, for an asset MakeHuman has not loaded yet, from its `.mhclo` text; its mesh from the compiled `.npz` or else its `.obj` (a triangle repeats its first corner, as in MakeHuman; larger polygons are fanned). Shear fitting is not supported.
- **Face hiding.** With `clothesHideFaces`, the clothes' `deleteVerts` hide what is under them. The outermost layer goes first, by `z_depth`. Hidden faces are not written.
- **Skeleton.** The skeleton comes from the `skeleton` line (`game_engine.mhskel`). Joints are placed at their reference vertices. Bone orientation is copied from the default skeleton's plane normals. Weights come from `default_weights.mhw`, through the bones' weight references (completed as `autoBuildWeightReferences` does), then through each proxy's reference vertices, or from a proxy's own `.mhw` (the shoes). Each vertex keeps at most 4 influences, normalized.
- **Materials.** From each `.mhmat` it takes the diffuse texture and `diffuseColor`, the normal map, `transparent` (alpha mask at 0.5), `backfaceCull` (double-sided), and a roughness from MakeHuman's Phong `shininess` weighted by the strength of its `specularColor`: `1 - shininess * min(1, 4 * specular)`, clamped to 0.35–0.95. Cloth and felt have a faint specular (the fedora's 0.04), so they read matte instead of like polished metal. An explicit `roughness` or `metallic` line wins. Colours (`diffuseColor`, and the `.mhm`'s `piece` colours) are display (sRGB) colours, as MakeHuman shows them; they are written to glTF's `baseColorFactor`, which is linear, through the sRGB-to-linear curve, or a dark suit would render as light grey.

What is not MakeHuman's, each set by a line of the `.mhm` or `.mhmat` that MakeHuman skips (see `assets/characters/README.md`):

- **Hair under hats.** The hair above a generated hat's or kepi's band is left out, so it does not poke through the crown. `--keep-hair-under-hats` turns this off.
- **Decimation.** `decimate` simplifies a dense garment after it is fitted and skinned, by quadric edge collapse (Garland and Heckbert). A vertex merges into a neighbour and keeps that neighbour's position and UV, so no UV is interpolated. Vertices on an open edge or a UV seam never move, which keeps the outline and the texture's seams exactly (and is what stops a garment above its target). A collapse that would turn a triangle over or pinch the surface is refused. The surviving vertex takes both vertices' weights, cut to the strongest 4 and renormalized. Cheapest first, ties by index: the output is deterministic.
- **Arms at the sides.** MakeHuman's rest pose holds the arms out in an A. The converter lowers them: each upper arm turns about its shoulder to hang down and a little out (`ARM_HANG`), each forearm about its elbow to follow it, a little forward. The bones below follow and every vertex moves by its weights; that pose is the rest pose written, so the game needs no correction.
- **Greyscale textures.** `greyscaleTexture True` in an `.mhmat` turns the diffuse texture grey, stretched so its brighter texels are white, before `diffuseColor` tints it: dark hair can be made white.
- **Garments cut from the body.** `piece <garment> key=value ...` makes a garment from the posed body proxy itself, so it reads 1919 instead of like a modern pack garment and bends exactly with the body:
  - *Regions.* Each skin vertex gets a limb from its strongest joint (the trunk, neck and legs; an arm; the head, never dressed) and a place on it: on the trunk and legs a *station*, its height on a scale through the joints (0 at the soles, 1 the ankles, 2 the knees, 3 the hips, 4 the waist at `spine_02`, 5 the shoulders, 6 the neck, 7 the head), and on an arm a *reach* (0 at the shoulder, 1 at the elbow, 2 at the wrist). So the cuts follow each character's own proportions.
  - *Shell.* A garment covers the skin faces whose three vertices fall in its range. It copies them, pushed out along the smooth normals by an offset that runs along the station or reach (`GARMENT_CUTS`: a shirt close, a waistcoat over it, a coat outermost; trousers close below the knee; a bombacha wide from the calf to the hip; boots snug with a flared top and a sole). Each vertex keeps its skin vertex's weights and UV (the UVs tiled for the cloth texture).
  - *Rims.* Wherever a garment ends (a hem, a cuff, a collar, an armhole), a rim goes from its edge down to just above the skin, so the end is never see-through.
  - *Layers.* Where garments overlap, each face goes to the one that sits furthest out, and the others leave it out (the shirt under the waistcoat, the trousers tucked into the boots). The skin's faces under any garment are left out too.
  - *Skirts.* A coat with `skirt` gets a tube from its waist to that station's height, round the dressed hips and legs, flared at the hem and open at the front by `open` sectors; it goes with the pelvis, its lower part partly with the thighs.

  Their settings, all optional: `color`; `top` and `bottom` (stations); `cuff` (a reach; 0, no sleeves); `loose` (a factor on the offsets); `vneck` and `vwidth` (a V opening at the front: its point's station and its half width at the neck); `skirt`, `open` and `flare` (the coat's skirt); `pad` (squared, padded shoulders); `sole` (boots: the sole's thickness):

  | Garment | Texture | Defaults |
  |---|---|---|
  | `shirt` (sleeves to the wrist, a small collar) | `cotton` | `top` 6.15, `bottom` 3.2, `cuff` 2.08 |
  | `waistcoat` (no sleeves, a V) | `wool` | `top` 6.0, `bottom` 3.6, `vneck` 5.0 |
  | `jacket` (sleeves, a V, a skirt); with `vneck=0` and a high `top`, a closed guerrera with a stiff high collar | `wool` | `top` 6.1, `bottom` 3.9, `cuff` 2.07, `vneck` 4.7, `skirt` 2.8, `open` 1 |
  | `trousers` | `wool` | `top` 4.3, `bottom` 1.2 |
  | `bombacha` | `cotton` | `top` 4.3, `bottom` 1.8 |
  | `boots` | `leather` | `top` 2.0, `sole` 0.1 |

- **Cloth textures.** The garments, the faja and the poncho carry small (64 px) tiling textures made in code, the same every run: `cotton` (a fine plain weave), `wool` (a twill), `stripe` (the twill with stripes), `leather` (soft mottling), all grey for the colour to tint, and `poncho` (dark red with cream woven bands).
- **Generated pieces.** `piece <kind> key=value ...` adds a small mesh fitted to the dressed body. A piece on one bone (the hats and kepi on the head, the neckerchief and collar on the neck) is rigidly skinned to it (weight 1). A piece that lies over several takes its skin from the dressed body under it, the way MakeHuman's garments take the body's: the poncho and strap from the shoulder to the hip, the faja and rastra round the waist, and the buttons, watch chain and lapels down the chest. Each vertex gets the inverse-distance mean of the weights of its 8 nearest body points, kept to the trunk joints the piece may follow (`SKIN_FROM`: pelvis, spine and clavicles, never the arms). So the poncho turns with the clavicle at the shoulder and with the pelvis at the hip. Each piece is fitted to an outline of the points around it (for each of 20 directions round a vertical axis, the farthest point at that height, plus a margin) or laid on the frontmost surface found along z. Lengths are in MakeHuman's decimeters, colours are `r,g,b`:

  | Kind | Joint | Settings (defaults) |
  |---|---|---|
  | `hat`: a felt crown with a flat top, a brim with a thickness and a curled edge, and a ribbon (gacho, chambergo, wide brim) | `head` | `color`, `band` (the ribbon's colour); `crown` 0.25 (top above the head's top); `taper` 0.97 (top width / band width); `brim` 0.75; `droop`, `back` and `sides` 0.05, 0, 0 (how far the brim's front, back and side edges drop; negative raises them); `dent` 0 (a crease down the top, front to back); `pinch` 0 (narrows the crown's front); `round` 0 (softens the top's edge, up to 1); `thick` 0.07; `curl` 0.04 (the edge turned up); `tilt` 0.15 (band lower at the back); `fit` 1.05 (band below the head's top). The band (a hat's or a kepi's) hugs the skull's skin outline, not the hair's |
  | `kepi`: a cloth crown with a flat top a little wider than its band, sloping down to the front; a leather band, a stiff visor and a chin strap with two buttons | `head` | `color`, `visor` (the leather's colour), `button`, `crown` 0.02, `taper` 1.06, `slope` 0.3, `width` 0.6 (visor), `droop` 0.3, `tilt` 0, `fit` 0.62 |
  | `neckerchief` (pañuelo): a band round the neck, a point on the chest, a knot to one side with two short ends | `neck_01` | `color`, `drop` 0.9, `knot` 0.11 |
  | `collar`: a stiff band collar with small wing tips | `neck_01` | `color`, `height` 0.28, `wings` 0.12 |
  | `sash` (faja): a striped band round the waist | `pelvis` | `color`, `width` 1.1, `height` 0 |
  | `rastra`: a leather belt over the sash, a plate and coins at the front (`coins=0`: a plain belt with a buckle) | `pelvis` | `color`, `coin` (the coins' colour; metallic), `coins` 8, `height` 0 |
  | `buttons`: brass buttons down the front | `spine_02` | `color` (metallic), `count` 6, `spread` 0 (column spacing; above 0, two columns) |
  | `chain`: a watch chain across the waistcoat | `spine_02` | `color` (metallic), `height` 0.5 (from `spine_01` to `spine_02`), `reach` 0.55, `sag` 0.18 |
  | `lapels`: a coat's lapels either side of its opening | `spine_03` | `color`, `point` 0.3 (from `spine_02` to the neck), `open` 0.6, `width` 0.4 |
  | `strap`: a leather strap from one shoulder across the chest and back to the other hip | `spine_03` | `color`, `side` -1 (1 the left shoulder), `width` 0.5 |
  | `poncho`: a poncho folded long over one shoulder, its ends hanging below the hip | `spine_03` | `color` (tints the woven texture), `side` 1, `width` 1.8, `thick` 0.16, `hang` 1.4 |

The data is local, but it is read as untrusted input:

- Text and JSON files have size limits, and non-finite numbers are refused.
- Numpy archives are loaded with `allow_pickle=False`. Every array's type, shape and size is checked, and so is every index.
- Paths are kept inside the data folder or the `.mhm`'s own folder.
- PNGs are decoded by the script itself: 8- or 16-bit, not interlaced (an interlaced PNG is refused). It undoes their filters along anti-diagonals with numpy.

Bad input gets a one-line error. Any other exception is a bug and keeps its traceback.

## `glbtrim.py`: trimming animated GLBs

Keeps only the named animation clips of a self-contained GLB and drops the accessors, buffer views and binary data nothing else uses. Nodes, meshes, skins and materials keep their indices, so the scene is unchanged. It uses the Python 3 standard library only, treats the input as untrusted data (bounded sizes, every index checked, no external URIs) and writes the same bytes for the same input. Run it isolated:

```sh
for pair in "cow_a cow" "bull bull" "horse_a horse"; do
  set -- $pair
  python3 -I tools/glbtrim.py ~/Downloads/quaternius/$1.glb \
    assets/models/fauna/quaternius/$2.glb \
    --keep Idle Idle_2 Idle_Headlow Eating Walk Gallop
done
```

The clips are written in the `--keep` order, which the game relies on (`pampa_render::fauna::Clip`). It prints the size before and after, the triangles, the joints and the clips kept.

It also recomputes smooth vertex normals, because the exports are flat-shaded and the bodies read faceted. This is on by default; `--smooth-angle DEGREES` sets the angle (60 by default) and `--smooth-angle 0` keeps the file's normals.

- For the normals only, vertices are welded by exact position and equal `JOINTS_0`/`WEIGHTS_0`, so nothing is smoothed across different skinning. The weld spans all primitives of a mesh, so no seam shows where two materials meet.
- Around each welded point, faces that share an edge bending less than the angle form one smooth group, like auto smooth. Each vertex gets its group's face normals, weighted by corner angle, so hoof rims and ear and horn tips stay sharp.
- Only the NORMAL bytes change: the vertex count, positions, UVs, colours and file size stay the same.

## `retarget.py`: animation clips for the MakeHuman characters

Retargets Quaternius' CC0 Universal Animation Library 1 and 2 (their Godot GLBs) to MakeHuman's `game_engine` skeleton, and writes the clips the game plays into one shared animation GLB. It uses the Python 3 standard library, plus numpy for the chest-clearance step (`--model`), as `mh2gltf.py` does. It reads the GLBs through `glbtrim.py`'s bounded reader, treats every input as untrusted data (no external URIs, every index, name and number checked, only LINEAR samplers and unscaled bones) and writes the same bytes for the same inputs. Run it isolated:

```sh
python3 -I tools/retarget.py --ual1 UAL1.glb --ual2 UAL2.glb \
  --target assets/characters/protagonist/model.gltf \
  --model assets/characters/protagonist/model.gltf --model assets/characters/hidalgo/model.gltf \
  --model assets/characters/farach/model.gltf --model assets/characters/bairoletto/model.gltf \
  -o assets/characters/animations/humanoid.glb
```

The output holds the target's `Root` node and the 52 bones under it, at rest, plus the clips, and no mesh. Each clip has one rotation channel per bone and one translation channel on the pelvis. Bevy binds a clip to a bone by the bone's name path from the scene root, so the clips play on any character built with the same skeleton. Each clip is renamed to its game name and written in a fixed order, which the game relies on (`pampa_render::character::clips::Clip`; see `CLIPS` in the script). Walks and runs carry the `ground_speed` (m/s) and `stride` (m per cycle) the script measured, in the animation's `extras`. The script also prints them.

How a clip is retargeted, each frame:

- **Bone names.** UAL1's Rigify `DEF-*` bones and UAL2's Unreal bones are mapped to `game_engine`'s 52, including the fingers. UAL2's `*_leaf` and `*_04` tips have no counterpart.
- **Rest alignment.** UAL rests in a T-pose; the MakeHuman rest has the arms down. The source is first posed like the target's rest (its *reference*). Down the hierarchy, each bone keeps its rest turn under its parent, then swings by the shortest arc until it points where the target bone points. That is at the next joint of its chain (the spine at the neck, the upper arm at the elbow), or along its +Y at a chain's end. The pelvis is not swung.
- **Rotations.** Each bone's world turn away from the reference is applied to the target bone's world rest, then made local under the target's posed parent. All of this happens in the shared world frame (Y up, facing +Z, left at +X), so neither rig's bone roll matters: no limb twists and no sign flips.
- **Pelvis.** Its world move from rest is scaled by the target's leg length over the source's (hip to ankle, 1.043 for the protagonist). Loops have their horizontal drift taken out, so they stay in place (the game moves the entity); the bob and the sway stay. Wherever the target, built differently, would put an ankle or a ball of the foot below its rest height, the pelvis is raised until it is not.
- **Calm standing.** Quaternius' idles stand like a boxer: the feet twice the hips apart, one 42 cm ahead, the knees bent about 20°, the arms 17° out. For `idle`, `idle_talk`, `idle_fold_arms`, `yes` and `no` (`CALM`), the feet are drawn in to at most 1.3 times the hip joints' spacing and 12 cm one ahead of the other, at the same heights and with their own turn. The legs are solved again through the knees, which keep 35% of their bend, and the hips go as high as the straighter leg allows (or lower, when the feet come in under them). In `idle`, `yes` and `no`, the upper arms are lowered to at most 10° out (the MakeHuman rest's own is 11°). The walks and runs are left as the source has them.
- **Chest clearance** (with `--model`). The source mannequin's chest is slimmer than the MakeHuman men's in their clothes, so its folded arms sank 8 to 12 cm into theirs. For `idle_fold_arms` and `idle_talk` (`CLEAR`), the arms are brought forward as they are: each clavicle swings forward, the shoulders rounding, by the least multiple of 5 mm that keeps every given model's forearms and hands at least 1.5 cm in front of its chest. Each model is posed the way the game poses it (the clip's rotations, the hips fitted to its legs) and CPU-skinned. The chest is sampled along its normal (+z) between `spine_02` and the neck, within 10 cm of its middle. The inside of the elbow (vertices weighted under 75% to one bone) is not counted, since linear skinning pinches it into the body at that bend. Folded arms come 12 cm forward and the talking hands 3 cm. Both are written as `reach` in the clip's `extras`.
- **Speed.** A loop's ground speed is how fast a foot point on the ground (within 1 cm of its lowest height) slides back once the drift is out, plus the drift: the median over the clip, scaled to the target. The game plays a walk at ground speed over this speed, so the feet do not slide.

`test_retarget.py` covers these on small synthetic rigs: the name maps against the shipped skeleton, a source posed as the reference giving the target rest, a known turn, every bone pointing where its source bone points, no forearm twist, the leg-length scaling, the loops held in place, the ground clamp, the calm stance, the arms brought forward, the speed measure, and deterministic output. It also checks the shipped GLB's clip order, bone paths and calmed clips. It checks that no forearm or hand of any of the four men comes within 1.5 cm of his chest in the folded arms and the talking hands. When the packs are in `~/Downloads/quaternius/ual/`, it also rebuilds the GLB and compares it byte for byte with the shipped one. With the packs it also checks that the torso and the head lean as the source's do (walk, idle, folded arms), that seen from the front the legs and arms spread as the source's do before calming, and that the calmed idles stand easy.
