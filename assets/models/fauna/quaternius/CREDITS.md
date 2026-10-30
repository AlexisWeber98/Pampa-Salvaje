# Credits: Quaternius animals

The game's cattle, foxes, wild cats and raptors are
animated low-poly models by Quaternius, released under CC0 1.0 (public
domain dedication, https://creativecommons.org/publicdomain/zero/1.0/).
The horse is no longer used: since 2026-10-30 the game uses its own
Blender horse (`tools/horse.py`, `assets/models/fauna/own/horse.glb`).
`horse.glb` stays here only as the reference for the rig replica.
The cow, bull and horse were downloaded from poly.pizza on 2026-10-23;
the fox from poly.pizza and the Animal Pack Vol.2 from OpenGameArt on
2026-10-29.

| File | Model | Source | Licence |
|---|---|---|---|
| `cow.glb` | Cow (`cow_a.glb`) | https://poly.pizza/m/26zM1outCr | CC0 1.0 |
| `bull.glb` | Bull | https://poly.pizza/m/a8PIIYwF7r | CC0 1.0 |
| `horse.glb` | Horse (`horse_a.glb`) | https://poly.pizza/m/qvTrSG9pZF | CC0 1.0 |
| `fox.glb` | Fox | https://poly.pizza/m/Bc97C66HKi | CC0 1.0 |
| `cat.glb` | Cat, Animal Pack Vol.2 (`FBX/Cat.fbx`) | https://opengameart.org/content/animated-animales-low-poly | CC0 1.0 |
| `eagle.glb` | Eagle, Animal Pack Vol.2 (`FBX/Eagle.fbx`) | https://opengameart.org/content/animated-animales-low-poly | CC0 1.0 |

## Changes

The cow, bull and horse were trimmed with `tools/glbtrim.py`, keeping the clips `Idle`,
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
| `cow.glb` | 1,028,584 bytes | 493,248 bytes | 2,450 | 42 |
| `bull.glb` | 1,038,360 bytes | 495,012 bytes | 2,418 | 42 |
| `horse.glb` | 1,108,124 bytes | 496,060 bytes | 2,182 | 50 |

The fox was trimmed the same way, keeping `Idle`, `Idle_2`,
`Idle_2_HeadLow`, `Eating`, `Walk`, `Gallop` and `Gallop_Jump` (1,848
triangles, 51 joints; 960,228 -> 452,620 bytes).

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
recoloured: the cows as Hereford, Aberdeen Angus or overo, the horses in
the criollo coats of the procedural horses, the fox as a grey pampas fox,
the cat as a small grey spotted gato montés, a large tawny puma and a
large spotted yaguareté, and the eagle as a slim brown chimango and a
larger dark carancho with a pale neck and a red-orange face.
