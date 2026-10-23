# Credits

Pampa Seca is mostly built procedurally in code. These are the third-party assets it uses. Each one is also credited next to its files.

## Textures

| Asset | Author / source | Licence | Where | Changes |
|---|---|---|---|---|
| Ground 109 | ambientCG (https://ambientcg.com/view?id=Ground109) | CC0 1.0 | `assets/textures/ground/Ground109/` | Converted to 512×512 PNM; blended with Grass 004 and tinted in the game |
| Ground 102 | ambientCG (https://ambientcg.com/view?id=Ground102) | CC0 1.0 | `assets/textures/ground/Ground102/` | Converted to 512×512 PNM; the dirt roads' surface, tinted and feathered at the edges in the game |
| Grass 004 | ambientCG (https://ambientcg.com/view?id=Grass004) | CC0 1.0 | `assets/textures/ground/Grass004/` | Converted to 512×512 PNM; blended with Ground 109 and retinted dry in the game |

All three were converted to 512×512 PPM/PGM for loading. See `assets/textures/ground/CREDITS.md` for details.

| Asset | Author / source | Licence | Where | Changes |
|---|---|---|---|---|
| Clay Plaster | Amal Kumar, Poly Haven (https://polyhaven.com/a/clay_plaster) | CC0 1.0 | `assets/textures/buildings/clay_plaster/` | 1K maps renamed; adobe walls |
| Plaster 003 | ambientCG (https://ambientcg.com/view?id=Plaster003) | CC0 1.0 | `assets/textures/buildings/plaster_003/` | 1K maps renamed; whitewashed walls |
| Brick Wall 02 | Dimitrios Savva, Poly Haven (https://polyhaven.com/a/brick_wall_02) | CC0 1.0 | `assets/textures/buildings/brick_wall_02/` | 1K maps renamed |
| Corrugated Iron 02 | Jenelle van Heerden, Sergej Majboroda, Poly Haven (https://polyhaven.com/a/corrugated_iron_02) | CC0 1.0 | `assets/textures/buildings/corrugated_iron_02/` | 1K maps renamed |
| Corrugated Iron | Dimitrios Savva, Jenelle van Heerden, Poly Haven (https://polyhaven.com/a/corrugated_iron) | CC0 1.0 | `assets/textures/buildings/corrugated_iron/` | 1K maps renamed |
| Brown Planks 05 | Rob Tuytel, Poly Haven (https://polyhaven.com/a/brown_planks_05) | CC0 1.0 | `assets/textures/buildings/brown_planks_05/` | 1K maps renamed |
| Brown Mud Dry | Rob Tuytel, Poly Haven (https://polyhaven.com/a/brown_mud_dry) | CC0 1.0 | `assets/textures/buildings/brown_mud_dry/` | 1K maps renamed; huella surface |
| Dry Ground 01 | Rob Tuytel, Poly Haven (https://polyhaven.com/a/dry_ground_01) | CC0 1.0 | `assets/textures/buildings/dry_ground_01/` | 1K maps renamed; trodden yards |

The `thatch/` set next to them is our own work. See `assets/textures/buildings/CREDITS.md` for details.

## Models

| Asset | Author / source | Licence | Where | Changes |
|---|---|---|---|---|
| Ultimate Nature Pack (28 models: trees, dead trees, stump, log, bushes, grass, wheat, plants, flowers, rocks) | Quaternius (https://quaternius.com/packs/ultimatenature.html, https://www.patreon.com/quaternius) | CC0 1.0 | `assets/models/vegetation/quaternius-ultimate-nature/` | A subset of the OBJ files; recoloured and rescaled in code for the 1919 pampa (common trees stand in for the caldén, bushes for chañar and jarilla) |

See `assets/models/vegetation/quaternius-ultimate-nature/CREDITS.md` for the full list.

| Asset | Author / source | Licence | Where | Changes |
|---|---|---|---|---|
| Wooden Barrels 01, Wooden Bucket 01, Wooden Crate 01, Wooden Lantern 01 | James Ray Cock, Poly Haven (https://polyhaven.com/models) | CC0 1.0 | `assets/models/props/polyhaven/` | None; the 1K glTF packages, used as props around rural settlements |
| Wooden Ladder | Miroslav Turura, Poly Haven (https://polyhaven.com/a/wooden_ladder) | CC0 1.0 | `assets/models/props/polyhaven/wooden_ladder/` | None |

See `assets/models/props/polyhaven/CREDITS.md` for details.

| Asset | Author / source | Licence | Where | Changes |
|---|---|---|---|---|
| 3TD Starter Pack #1 (windmill, old wagon, barbed-wire fence segments) | Ron Kapaun (3TD), OpenGameArt (https://opengameart.org/content/3td-starter-pack-1) | CC0 1.0 | `assets/models/third-party/3td-starter-pack/` | Only the COLLADA sources and their textures are kept; they are converted to glTF for the model slots |
| 3TD windmill, converted | Ron Kapaun (3TD), as above | CC0 1.0 | `assets/models/third-party/windpump/` | Converted to glTF with `tools/dae2gltf.py` (Z-up to Y-up, wheel split off as the `rotor` node, normal maps' green flipped from DirectX to OpenGL); the windpump of rural settlements |
| 3TD old wagon, converted | Ron Kapaun (3TD), as above | CC0 1.0 | `assets/models/third-party/wrecked_wagon/` | Converted the same way (feet to meters); the abandoned wagons in yards and on road verges |

See `assets/models/third-party/3td-starter-pack/CREDITS.md` for details.

## Rules

- Every free or open-source asset is credited in this file and next to its files, and the code that loads it says where it comes from in a doc comment. This applies even when the licence does not ask for credit.
- Only assets with a clear licence that allows use in a game are accepted, such as CC0 or CC-BY with attribution. Non-commercial licences are not accepted without the project owner's agreement.
