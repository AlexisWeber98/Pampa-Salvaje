# Model slots

A model slot lets a downloaded glTF model replace one procedural structure of the rural settlements without code changes. Each slot is a folder here named after the slot. When the folder holds `model.glb` or `model.gltf` (the `.glb` wins), every structure of that kind draws the model instead of its procedural mesh. When it does not, the procedural mesh is drawn.

- While the model loads, the procedural mesh stands in.
- If the model fails to load (a broken file, a missing texture), the procedural mesh stays and the log says why.
- Obstacles, flora keep-outs and camera occluders come from the game's layout and do not change with the model. A model should stay inside its structure's footprint.
- Every model is drawn within the settlement props' shared visibility range, and fades out of the camera's way like the procedural mesh it replaces.

## Slots

| Slot | Structure | Size the layout gives it | Filled by |
|---|---|---|---|
| `windpump` | Molino: tower, wheel and tail vane | 2.4 m square base, wheel hub at about 10.5 m | 3TD windmill (converted) |
| `wrecked_wagon` | Abandoned wagon in a yard or on a road verge | 4.8 m long, 3.6 m deep, up to 2.2 m tall | 3TD old wagon (converted) |
| `cart` | Carreta: two-wheeled ox cart | 5 m long, 2.2 m wide, 2.5 m tall | Procedural |
| `well` | Brick well curb with its frame | 1.6 m square | Procedural |
| `trough` | Bebedero | 6–8 m long, 0.9 m wide | Procedural |
| `haystack` | Parva | 5–7 m square, 4 m tall | Procedural |
| `hitching_post` | Palenque | 3 m long | Procedural |
| `eucalyptus` | Eucalyptus of the grove | 18–28 m tall | Procedural (model trunk, procedural crown) |
| `poplar` | Lombardy poplar of the alameda | 14–20 m tall | Procedural (model trunk, procedural crown) |

The names come from `StructureKind::slot` in `crates/core/src/settlement.rs`. A test checks that every folder here (except the `3td-starter-pack` sources) is a known slot with a model in it.

## Fitting a model: `slots.ron`

`slots.ron` says how each model sits in its structure's own space. That space has `x` along the structure's front, `y` up from the ground at its middle, and `z` out of its front (toward the settlement's patio). A slot with a model and no entry is drawn as it comes.

```ron
(
    slots: {
        "windpump": (
            scale: 0.875,               // uniform scale, applied first
            offset: (0.0, 0.0, 0.0),    // meters, after scale and yaw
            yaw: 0.0,                   // degrees about the vertical
            rotor: Some((node: "rotor", axis: (0.0, 0.0, -1.0))),
        ),
    },
)
```

| Field | Default | Meaning |
|---|---|---|
| `scale` | `1.0` | Uniform scale. For the tree slots it is per meter of the tree's height: author the tree 1 m tall, or set `scale` to 1 over its height. |
| `offset` | `(0, 0, 0)` | Move in meters, in the structure's space. |
| `yaw` | `0.0` | Turn about the vertical, in degrees. |
| `rotor` | `None` | The glTF node that spins (a windpump's wheel), by name, and its axis in that node's own space. It turns 1.4–2.4 radians per second. |

An unknown slot name, a scale that is not positive, a non-finite number, or a rotor without a name or with a zero axis makes the whole file invalid: the game warns and draws every model unfitted.

## Adding a model

1. Check the licence **on the asset's source page**. CC0 and CC-BY are fine. Note the author, the page URL and the licence.
2. Put the model in `<slot>/model.glb` (or `model.gltf` with its `.bin` and textures beside it). Keep it under the triangle budgets: props 0.5–5k, the windpump 8–15k.
3. A COLLADA (`.dae`) model can be converted with `tools/dae2gltf.py` (see `tools/README.md`).
4. Fit it in `slots.ron` and check it in the game.
5. Credit it twice:
   - in a `CREDITS.md` next to its files (the folder's, or the source pack's): asset, author, source URL, licence, the date it was downloaded, and what was changed;
   - in the root `CREDITS.md`, in the models table.
   CC-BY models must keep the author's credit line exactly as the licence asks.
