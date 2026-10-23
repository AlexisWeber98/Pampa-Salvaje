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

A standard-library `unittest` suite builds tiny COLLADA documents and PNGs in a temporary folder and covers the axis and unit conversion, triangles and polylists, the rotor split, the normal-map convention, the refusals and `--check`:

```sh
python3 -I -B -m unittest discover -s tools -v
```

The conversions done so far are listed in `assets/models/third-party/3td-starter-pack/CREDITS.md`.
