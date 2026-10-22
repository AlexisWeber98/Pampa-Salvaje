# Credits: 3TD Starter Pack #1

The models in `source/` come from **3TD Starter Pack #1** by **Ron Kapaun (3TD)**, published on OpenGameArt: https://opengameart.org/content/3td-starter-pack-1.

They are licensed under the **Creative Commons CC0 1.0 Universal** public domain dedication, as the OpenGameArt page states. The full licence text ships with the pack and is kept here as `3TD_CC0_License_EULA.pdf`. The licence doesn't require credit, but we give it anyway.

| Folder | Asset | Used for |
|---|---|---|
| `source/Windmill/` | Animated windmill (`WindMill-Anim01.DAE`, multi-blade wheel on a steel lattice tower, with diffuse, normal and specular maps) | The molino (windpump) of rural settlements, model slot `windpump` |
| `source/OldWagon/` | Old wagon (`OldWagon.DAE`) | A farm wagon prop by galpones (candidate for the `carreta` slot, after rescaling) |
| `source/BarbedWireFences/` | Barbed-wire fence segments (`FenceSeg_01.DAE`, `FenceSeg_02.DAE`) | Reference for rural wire fences |

Only these three folders were kept out of the pack, unchanged: the COLLADA (`.DAE`) files and their textures. The pack's Torque engine files (`.cs`, `.dts`, `.dds`) were left out. Bevy cannot read COLLADA, so the files are converted to glTF into the matching model slot (see `odd/tasks/rural-settlements.md`, T4), and the conversion is noted here once done.

Downloaded on 2026-10-22, once, with the project owner's permission.

Suggested credit line: "Windmill, wagon and fence models from 3TD Starter Pack #1 by Ron Kapaun (3TD), opengameart.org, CC0 1.0."
