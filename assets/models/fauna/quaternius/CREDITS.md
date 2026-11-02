# Credits: Quaternius animals

The game's raptors are
animated low-poly models by Quaternius, released under CC0 1.0 (public
domain dedication, https://creativecommons.org/publicdomain/zero/1.0/).
The horse is no longer used: since 2026-10-30 the game uses its own
Blender horse (`tools/horse.py`, `assets/models/fauna/own/horse.glb`).
`horse.glb` stays here only as the reference for the rig replica.
The Quaternius cow and bull were removed on 2026-11-01: the game uses its own
Blender cattle (`tools/cattle.py`, `assets/models/fauna/own/cow.glb` and
`bull.glb`), whose rig follows the horse replica.
The Quaternius fox was removed on 2026-11-01 as well: the game uses its own
Blender gray pampas fox (`tools/fox.py`, `assets/models/fauna/own/fox.glb`).
The Quaternius cat is not drawn any more since 2026-11-02: the gato montés,
the puma and the yaguareté are the game's own Blender felids (`tools/felid.py`,
`assets/models/fauna/own/gato.glb`, `puma.glb` and `yagua.glb`). `cat.glb`
stays here.
The horse was downloaded from poly.pizza on 2026-10-23;
the Animal Pack Vol.2 from OpenGameArt on
2026-10-29.

| File | Model | Source | Licence |
|---|---|---|---|
| `horse.glb` | Horse (`horse_a.glb`) | https://poly.pizza/m/qvTrSG9pZF | CC0 1.0 |
| `cat.glb` | Cat, Animal Pack Vol.2 (`FBX/Cat.fbx`) | https://opengameart.org/content/animated-animales-low-poly | CC0 1.0 |
| `eagle.glb` | Eagle, Animal Pack Vol.2 (`FBX/Eagle.fbx`) | https://opengameart.org/content/animated-animales-low-poly | CC0 1.0 |

## Changes

The horse was trimmed with `tools/glbtrim.py`, keeping the clips `Idle`,
`Idle_2`, `Idle_Headlow`, `Eating`, `Walk`, `Gallop` and `Gallop_Jump` in
that order:

- the duplicate `AnimalArmature|…` clips and the unused Attack, Death,
  hit-react and `Jump_toIdle` clips were dropped, with the accessors and binary data
  only they used, and the unused texture samplers;
- the vertex normals were recomputed smooth (60° angle, never across
  different skinning; the exports are flat-shaded);
- otherwise the meshes, skins, nodes and materials are unchanged.

| File | Before | After | Triangles | Joints |
|---|---|---|---|---|
| `horse.glb` | 1,108,124 bytes | 496,060 bytes | 2,182 | 50 |

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
recoloured: the eagle as a slim brown chimango and a larger dark carancho
with a pale neck and a red-orange face. The cat was drawn as the gato
montés, the puma and the yaguareté until 2026-11-02.
