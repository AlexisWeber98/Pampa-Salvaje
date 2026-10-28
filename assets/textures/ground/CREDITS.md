# Credits: ground textures

Every texture in this folder comes from **ambientCG** (https://ambientcg.com) and is licensed under the **Creative Commons CC0 1.0 Universal** public domain dedication (https://docs.ambientcg.com/license/). The licence doesn't require credit, but we give it anyway.

| Folder | Asset | Source | Used for | Changes |
|---|---|---|---|---|
| `Ground109/` | Ground 109 (dry beige soil with pebbles) | https://ambientcg.com/view?id=Ground109 | Base soil of the open pampa, showing through the grass | Only the Color, NormalGL, Roughness and AmbientOcclusion maps are kept. The game blends them with Grass 004 into one tiled detail set, nearly grey, and the ground's vertex colour tints it. |
| `Ground102/` | Ground 102 (compressed dirt) | https://ambientcg.com/view?id=Ground102 | The dirt roads' surface | Same four maps. Used alone, a little greyer and flatter than the photo, on the road surface; the vertex colour tints it dirt brown, and its alpha feathers the road's edges into the grass. |
| `Grass004/` | Grass 004 | https://ambientcg.com/view?id=Grass004 | Grass cover, tinted from dry yellow to grey-green | Same four maps. Blended with Ground 109 as above; the colour is retinted in the game to fit a dry 1919 pampa. |
| `ScatteredLeaves009/` | Scattered Leaves 009 | https://ambientcg.com/view?id=ScatteredLeaves009 | Leaf litter under the monte | Only the Color map is kept, as a 512×512 PPM (`*_Color.ppm`). The ground shader blends it in where the monte grows, toned darker and browner, and feathers it out at the patch edges. Downloaded on 2026-10-28. |

Suggested credit line, from ambientCG: "Created using Ground 109, Ground 102, Grass 004 and Scattered Leaves 009 from ambientCG.com, licensed under the Creative Commons CC0 1.0 Universal License."

Downloaded on 2026-10-22 as the `_1K-JPG` packages.

The game loads these as uncompressed PNM files, with no JPEG decoder and no extra dependency. Each map was converted with ImageMagick and **downscaled to 512×512**:
- Color and NormalGL maps became binary PPM (P6, `*_Color.ppm`, `*_NormalGL.ppm`).
- Roughness and AmbientOcclusion maps became greyscale PGM (P5, `*_Roughness.pgm`, `*_AmbientOcclusion.pgm`).

The original `_1K-JPG_*.jpg` files are only the source of the conversion. They can be deleted from the repository.
