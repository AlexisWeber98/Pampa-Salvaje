# Credits: building and track textures

These are PBR texture sets for the rural settlements: walls, roofs, wood and the dirt of the huellas. All except `thatch/` come from **Poly Haven** (https://polyhaven.com) or **ambientCG** (https://ambientcg.com). Both publish every asset under the **Creative Commons CC0 1.0 Universal** public domain dedication (https://polyhaven.com/license, https://docs.ambientcg.com/license/). The licence doesn't require credit, but we give it anyway.

| Folder | Asset | Author / source | Used for |
|---|---|---|---|
| `clay_plaster/` | Clay Plaster | Amal Kumar, Poly Haven (https://polyhaven.com/a/clay_plaster) | Adobe (mud-plastered) walls of ranchos and puestos |
| `plaster_003/` | Plaster 003 | ambientCG (https://ambientcg.com/view?id=Plaster003) | Whitewashed lime plaster of chacras, cascos and worker houses |
| `brick_wall_02/` | Brick Wall 02 | Dimitrios Savva, Poly Haven (https://polyhaven.com/a/brick_wall_02) | Exposed old brick |
| `corrugated_iron_02/` | Corrugated Iron 02 | Jenelle van Heerden, Sergej Majboroda, Poly Haven (https://polyhaven.com/a/corrugated_iron_02) | Weathered corrugated-iron roofs and galpones |
| `corrugated_iron/` | Corrugated Iron | Dimitrios Savva, Jenelle van Heerden, Poly Haven (https://polyhaven.com/a/corrugated_iron) | Newer galvanised iron: water tanks, newer roofs |
| `brown_planks_05/` | Brown Planks 05 | Rob Tuytel, Poly Haven (https://polyhaven.com/a/brown_planks_05) | Weathered boards: galpón walls, doors, corral rails |
| `brown_mud_dry/` | Brown Mud Dry | Rob Tuytel, Poly Haven (https://polyhaven.com/a/brown_mud_dry) | The worn earth of the huellas (inner tracks) |
| `dry_ground_01/` | Dry Ground 01 | Rob Tuytel, Poly Haven (https://polyhaven.com/a/dry_ground_01) | Trodden, cracked yards around the houses and corrals |
| `thatch/` | Thatch | Pampa Seca (own work, generated with ImageMagick from seeded noise by `thatch/generate.sh`) | Thatched (paja) roofs of ranchos. CC0, like the rest of the project's own assets |

Each folder holds `color.jpg`, `normal_gl.jpg` (OpenGL normal map) and `roughness.jpg`, plus `ao.jpg` where the source has one. The files are the sources' 1K JPG maps, renamed, with no other changes. They were downloaded on 2026-10-22, once, with the project owner's permission.

The game loads `color.ktx2`, `normal_gl.ktx2` and `orm.ktx2` instead: GPU-compressed copies (UASTC, mipmaps baked, ambient occlusion, roughness and metallic packed into one map) converted from those JPGs with `tools/textures_to_ktx2.sh`. The JPG sources are kept unchanged as the editable originals; re-run the script after editing one.

Suggested credit line: "Textures from Poly Haven (polyhaven.com) by Amal Kumar, Dimitrios Savva, Jenelle van Heerden, Sergej Majboroda and Rob Tuytel, and from ambientCG.com, all licensed under CC0 1.0."
