# Credits: grass textures

| Folder | Asset | Author / source | Licence | Used for |
|---|---|---|---|---|
| `grass_medium_01/` | Grass Medium 01 | Poly Haven, https://polyhaven.com/a/grass_medium_01 | CC0 1.0 | The alpha-cut grass cards of the field tufts (`crates/render/src/grass.rs`): paja wears the dry colour map, flechilla the green one |

The project owner downloaded the 1K diffuse, dry diffuse, alpha and normal maps on 2026-10-28. Only the diffuse and alpha maps are used.

## Changes

- The sheet is a UV layout of a whole grass model. Four of the clumps drawn in its bottom rows were cut out and fitted into the cells of a 1024x512 atlas (2x2 cells of 512x256, each clump standing on its cell's bottom edge), a few stray marks of the neighbouring islands were cleared from the opacity, and the colour outside the clumps was filled with their mean colour so the mips do not darken the edges.
- `dry_color.jpg`, `green_color.jpg` (from `grass_medium_01_dry_diff_1k.jpg` and `grass_medium_01_diff_1k.jpg`) and `opacity.jpg` (from `grass_medium_01_alpha_1k.jpg`) are the editable sources.
- `dry_color.ktx2` and `green_color.ktx2` are what the game loads: the colour with the opacity in its alpha, with the coverage-preserving mips of `tools/leaf_atlas.py --mips`, encoded with the UASTC options of `tools/textures_to_ktx2.sh` (`basisu -ktx2 -uastc -uastc_level 2 -srgb`). They were made by those same steps by hand, because the script wants a whole bark-style set (normal and roughness maps) and the cards use no other map.
- The normal map is not shipped.
