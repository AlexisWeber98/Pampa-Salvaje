# Credits: 3TD Starter Pack #1

The models in `source/` come from **3TD Starter Pack #1** by **Ron Kapaun (3TD)**, published on OpenGameArt: https://opengameart.org/content/3td-starter-pack-1.

They are licensed under the **Creative Commons CC0 1.0 Universal** public domain dedication, as the OpenGameArt page states. The full licence text ships with the pack and is kept here as `3TD_CC0_License_EULA.pdf`. The licence doesn't require credit, but we give it anyway.

| Folder | Asset | Used for |
|---|---|---|
| `source/Windmill/` | Animated windmill (`WindMill-Anim01.DAE`, multi-blade wheel on a steel lattice tower, with diffuse, normal and specular maps) | The molino (windpump) of rural settlements, model slot `windpump` |
| `source/OldWagon/` | Old wagon (`OldWagon.DAE`) | The abandoned wagons (a broken-down chata) at some yards and on a few road verges, model slot `wrecked_wagon`. Not the `cart` slot: it is a four-wheeled wreck with a wheel off, not a two-wheeled carreta |
| `source/BarbedWireFences/` | Barbed-wire fence segments (`FenceSeg_01.DAE`, `FenceSeg_02.DAE`) | Reference for rural wire fences |

Only these three folders were kept out of the pack, unchanged: the COLLADA (`.DAE`) files and their textures. The pack's Torque engine files (`.cs`, `.dts`, `.dds`) were left out. Bevy cannot read COLLADA, so the files are converted to glTF into the matching model slot (see `odd/tasks/rural-settlements.md`, T4).

## Conversions (2026-10-22)

Both were converted with our own standard-library script `tools/dae2gltf.py` (see `tools/README.md`), run with `python3 -I`; the sources here stay unchanged.

| Output | Command | Result |
|---|---|---|
| `../windpump/model.gltf` | `python3 -I tools/dae2gltf.py assets/models/third-party/3td-starter-pack/source/Windmill/WindMill-Anim01.DAE -o assets/models/third-party/windpump --rotor WindMillHead256` | 1,920 triangles: `body` 852 (tower, platform, tail vane), `rotor` 1,068 (the wheel, split off at its pivot so the game spins it about -Z) |
| `../wrecked_wagon/model.gltf` | `python3 -I tools/dae2gltf.py assets/models/third-party/3td-starter-pack/source/OldWagon/OldWagon.DAE -o assets/models/third-party/wrecked_wagon` | 3,046 triangles, one `body` node |

What the conversion changes: Z-up becomes Y-up, the wagon's feet become meters, every node transform is baked into the vertices, and each diffuse map becomes the base colour. The `_NRM` maps are DirectX-convention (green down; found by the script's curl test on each map), so their green channel is flipped to the OpenGL convention glTF uses. The `_SPEC` maps are not used: glTF has no Blinn specular, and the roughness comes from each material's shininess instead.

Downloaded on 2026-10-22, once, with the project owner's permission.

Suggested credit line: "Windmill, wagon and fence models from 3TD Starter Pack #1 by Ron Kapaun (3TD), opengameart.org, CC0 1.0."
