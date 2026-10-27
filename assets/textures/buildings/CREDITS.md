# Credits: building and track textures

These are PBR texture sets for the rural settlements and the towns: walls, roofs, wood, sidewalks and the dirt of the huellas. All except `thatch/` come from **Poly Haven** (https://polyhaven.com) or **ambientCG** (https://ambientcg.com). Both publish every asset under the **Creative Commons CC0 1.0 Universal** public domain dedication (https://polyhaven.com/license, https://docs.ambientcg.com/license/). The licence doesn't require credit, but we give it anyway.

| Folder | Asset | Author / source | Used for |
|---|---|---|---|
| `clay_plaster/` | Clay Plaster | Amal Kumar, Poly Haven (https://polyhaven.com/a/clay_plaster) | Adobe (mud-plastered) walls of ranchos and puestos |
| `plaster_003/` | Plaster 003 | ambientCG (https://ambientcg.com/view?id=Plaster003) | Whitewashed lime plaster of chacras, cascos and worker houses; the towns' whitewashed, lime-washed and stucco surfaces |
| `brick_wall_02/` | Brick Wall 02 | Dimitrios Savva, Poly Haven (https://polyhaven.com/a/brick_wall_02) | Exposed old brick, in settlements and town fronts |
| `corrugated_iron_02/` | Corrugated Iron 02 | Jenelle van Heerden, Sergej Majboroda, Poly Haven (https://polyhaven.com/a/corrugated_iron_02) | Weathered corrugated-iron roofs and galpones |
| `corrugated_iron/` | Corrugated Iron | Dimitrios Savva, Jenelle van Heerden, Poly Haven (https://polyhaven.com/a/corrugated_iron) | Newer galvanised iron: water tanks, newer roofs |
| `brown_planks_05/` | Brown Planks 05 | Rob Tuytel, Poly Haven (https://polyhaven.com/a/brown_planks_05) | Weathered boards: galpón walls, doors, corral rails |
| `brown_mud_dry/` | Brown Mud Dry | Rob Tuytel, Poly Haven (https://polyhaven.com/a/brown_mud_dry) | The worn earth of the huellas (inner tracks) |
| `dry_ground_01/` | Dry Ground 01 | Rob Tuytel, Poly Haven (https://polyhaven.com/a/dry_ground_01) | Trodden, cracked yards around the houses and corrals |
| `painted_plaster_wall/` | Painted Plaster Wall | Amal Kumar, Poly Haven (https://polyhaven.com/a/painted_plaster_wall) | Painted town fronts in five muted 1919 tints (lime white, ochre, pale rose, grey-blue, light terracotta) |
| `whitewashed_brick/` | Whitewashed Brick | Charlotte Baglioni, Poly Haven (https://polyhaven.com/a/whitewashed_brick) | Town houses, stores and outskirts ranchos of brick under a worn lime wash |
| `red_brick_plaster_patch_02/` | Red Brick Plaster Patch 02 | Rob Tuytel, Poly Haven (https://polyhaven.com/a/red_brick_plaster_patch_02) | Older town fronts and pulperías with their plaster flaking off the brick |
| `beige_wall_002/` | Beige Wall 002 | Dimitrios Savva, Rico Cilliers, Poly Haven (https://polyhaven.com/a/beige_wall_002) | The beige rendered plinths of painted town fronts (a 1024x256 strip, tiled 4 m by 1 m) |
| `brick_floor_003/` | Brick Floor 003 | Dimitrios Savva, Rob Tuytel, Poly Haven (https://polyhaven.com/a/brick_floor_003) | Flat brick sidewalks (veredas) of General Pico and round small towns' plazas |
| `dirt_path/` | Dirt | Charlotte Baglioni, Poly Haven (https://polyhaven.com/a/dirt) | Beaten earth of the plaza paths of small towns and villages, and of the earth sidewalks |
| `bronze/` | Metal 017 | ambientCG (https://ambientcg.com/view?id=Metal017), CC0 1.0 | The bronze plaques of the plaza monuments |
| `thatch/` | Thatch | Pampa Seca (own work, generated with ImageMagick from seeded noise by `thatch/generate.sh`) | Thatched (paja) roofs of ranchos. CC0, like the rest of the project's own assets |
| `gravel_road/` | Gravel Road | Amal Kumar, Poly Haven (https://polyhaven.com/a/gravel_road) | The ballast bed of the railway track |
| `rough_wood/` | Rough Wood | Rob Tuytel, Poly Haven (https://polyhaven.com/a/rough_wood) | The railway's timber sleepers |
| `rust_coarse_01/` | Rust Coarse 01 | Dimitrios Savva, Rico Cilliers, Poly Haven (https://polyhaven.com/a/rust_coarse_01) | The rails, with a smoother, brighter running head |
| `metal_plate/` | Metal Plate | Rob Tuytel, Poly Haven (https://polyhaven.com/a/metal_plate) | The train's diamond tread plate: running plates, footplates, steps and platforms |

Each folder holds `color.jpg`, `normal_gl.jpg` (OpenGL normal map) and `roughness.jpg`, plus `ao.jpg` where the source has one. The files are the sources' 1K JPG maps, renamed, with no other changes. They were downloaded on 2026-10-22, once, with the project owner's permission; the four sets `painted_plaster_wall/`, `whitewashed_brick/`, `red_brick_plaster_patch_02/` and `beige_wall_002/` were downloaded by the project owner on 2026-10-25 (1K JPG, `https://dl.polyhaven.org/file/ph-assets/Textures/jpg/1k/<id>/<id>_{diff,nor_gl,rough,ao}_1k.jpg`). The project owner downloaded `brick_floor_003/` (its colour map is `<id>_diffuse_1k.jpg`) and `dirt_path/` (Poly Haven id `dirt`) on 2026-10-26 the same way. The project owner downloaded `bronze/` (ambientCG `Metal017_1K-JPG.zip`: Color, NormalGL, Roughness; it has no ao map) on 2026-10-26. The project owner downloaded `gravel_road/`, `rough_wood/` and `rust_coarse_01/` on 2026-10-27 the same way, and `metal_plate/` (1K JPG: diffuse, normal GL, roughness and ao) on 2026-10-27; their authors were checked against the Poly Haven API (`metal_plate`'s author is Rob Tuytel, as downloaded). The game lifts their colour to muted 1919 tints in the material, not in the files; the train's black, dark red, maroon, varnish and oxide paints are tints of `rust_coarse_01` and `brown_planks_05` in the material too.

The game loads `color.ktx2`, `normal_gl.ktx2` and `orm.ktx2` instead: GPU-compressed copies (UASTC, mipmaps baked, ambient occlusion, roughness and metallic packed into one map) converted from those JPGs with `tools/textures_to_ktx2.sh`. The JPG sources are kept unchanged as the editable originals; re-run the script after editing one.

Suggested credit line: "Textures from Poly Haven (polyhaven.com) by Amal Kumar, Dimitrios Savva, Jenelle van Heerden, Sergej Majboroda, Rob Tuytel, Charlotte Baglioni and Rico Cilliers, and from ambientCG.com, all licensed under CC0 1.0."
