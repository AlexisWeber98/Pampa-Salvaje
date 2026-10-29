# Credits: Quaternius animals

The game's cattle and corral horses are animated low-poly models by
Quaternius, released under CC0 1.0 (public domain dedication,
https://creativecommons.org/publicdomain/zero/1.0/). They were downloaded
from poly.pizza on 2026-10-23.

| File | Model | Source | Licence |
|---|---|---|---|
| `cow.glb` | Cow (`cow_a.glb`) | https://poly.pizza/m/26zM1outCr | CC0 1.0 |
| `bull.glb` | Bull | https://poly.pizza/m/a8PIIYwF7r | CC0 1.0 |
| `horse.glb` | Horse (`horse_a.glb`) | https://poly.pizza/m/qvTrSG9pZF | CC0 1.0 |

## Changes

Each file was trimmed with `tools/glbtrim.py`, keeping the clips `Idle`,
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

In the game (`crates/render/src/fauna/livestock.rs`) they are scaled from
the export's units to life size, turned to the animals' headings and
recoloured per coat: the cows as Hereford, Aberdeen Angus or overo, and
the horses in the criollo coats of the procedural horses.
