# Credits: Quaternius animals

The game's raptors are
animated low-poly models by Quaternius, released under CC0 1.0 (public
domain dedication, https://creativecommons.org/publicdomain/zero/1.0/).
The Quaternius horse was removed on 2026-11-20: the game's horse is
henrysteve973's (`assets/models/fauna/henry/`). Before it, the game's own
Blender horse (`tools/horse.py`, removed from `assets/models/fauna/own/` the
same day) replicated its rig.
The Quaternius cow and bull were removed on 2026-11-01: the game uses its own
Blender cattle (`tools/cattle.py`, `assets/models/fauna/own/cow.glb` and
`bull.glb`), whose rig follows the horse replica.
The Quaternius fox was removed on 2026-11-01 as well: the game uses its own
Blender gray pampas fox (`tools/fox.py`, `assets/models/fauna/own/fox.glb`).
The Quaternius cat is not drawn any more since 2026-11-02: the gato montés,
the puma and the yaguareté are the game's own Blender felids (`tools/felid.py`,
`assets/models/fauna/own/gato.glb`, `puma.glb` and `yagua.glb`). `cat.glb`
stays here.
The Animal Pack Vol.2 was downloaded from OpenGameArt on 2026-10-29.

| File | Model | Source | Licence |
|---|---|---|---|
| `cat.glb` | Cat, Animal Pack Vol.2 (`FBX/Cat.fbx`) | https://opengameart.org/content/animated-animales-low-poly | CC0 1.0 |
| `eagle.glb` | Eagle, Animal Pack Vol.2 (`FBX/Eagle.fbx`) | https://opengameart.org/content/animated-animales-low-poly | CC0 1.0 |

## Changes

The cat and the eagle were converted from the pack's FBX files with
`tools/fbx2glb.py` (Blender 5.2) and then trimmed with
`tools/glbtrim.py` (smooth normals):

- `cat.glb`: turned to face glTF +Z; the clips `Idle` and `Walking`
  (renamed `Walk`); 807 triangles, 34 joints, 156,100 bytes;
- `eagle.glb`: turned to face glTF +Z, decimated from 24,800 to 2,400
  triangles, a still `Glide` clip of the rest pose (wings spread) added,
  and the `Flying` clip kept as `Flap` (the perched `Idle` dropped); 30
  joints, 150,856 bytes.

The pack's other models (Dog, Piranha, Wolf), its Blend and OBJ files and
the downloaded archives are not in the repository.

In the game (`crates/render/src/fauna/livestock.rs`) they are scaled from
the export's units to life size, turned to the animals' headings and
recoloured. The eagle was drawn as the chimango and the carancho (a slim
brown chimango, and a larger dark carancho with a pale neck and a red-orange
face) until 2026-11-03, when the own models in `assets/models/fauna/own/`
replaced it. The cat was drawn as the gato montés, the puma and the
yaguareté until 2026-11-02.
