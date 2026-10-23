# Tools

Small scripts for preparing assets. They are not part of the game build.

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
| `--rotor-name NAME` | The rotor's glTF node name (default `rotor`). |
| `--scale S` | An extra uniform scale. |
| `--metallic M` | Metallic factor of every material (default 0). |
| `--normal auto\|gl\|dx` | Green convention of the normal maps. `auto` (the default) decides per map: a real height field's normals are curl-free for one sign of green only. DirectX maps get their green flipped, since glTF wants OpenGL. |
| `--single-sided` | Cull back faces (materials are double-sided by default: game models often have single planes). |
| `--check FILE` | Only validate a written glTF: buffers, views, accessors, indices and images in range, every URI inside the model's folder. |

What it reads:

- `<triangles>`, `<polylist>` and `<polygons>`, fanned into triangles; lines and holes are skipped;
- positions, normals (or face normals when missing) and the first bound texture set;
- node transforms (`matrix`, `translate`, `rotate`, `scale`), baked into the vertices; a negative scale flips the winding;
- `<up_axis>` and `<unit meter>`, to Y-up meters;
- per material: the diffuse texture or colour, the normal map (a `<bump>` texture, else a `<diffuse>_NRM.png` beside it), and a roughness from the shininess. Specular maps are ignored.

The input is treated as untrusted. A file over 64 MB, a document with a DTD or entities, a non-finite number, an out-of-range index, or an oversized array is refused. Texture paths are reduced to their base name and looked up only in the model's own folder. Nothing is evaluated.

The conversions done so far are listed in `assets/models/third-party/3td-starter-pack/CREDITS.md`.
