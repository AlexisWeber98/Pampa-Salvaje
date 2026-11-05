# Credits: ground textures

Every texture in this folder is licensed under the **Creative Commons CC0 1.0 Universal** public domain dedication. The licence doesn't require credit, but we give it anyway.

## Poly Haven ground layers

From **Poly Haven** (https://polyhaven.com, licence https://polyhaven.com/license). Downloaded on 2026-11-03 as the 2K JPG maps.

| Folder | Asset | Authors | Used for | Changes |
|---|---|---|---|---|
| `grass_ground/` | Grass Ground (https://polyhaven.com/a/grass_ground) | Charlotte Baglioni | The pampa's grass layer, 2.5 m a tile | The diffuse, OpenGL normal, roughness, AO and displacement maps are renamed `color`, `normal_gl`, `roughness`, `ao` and `height.jpg`. `tools/textures_to_ktx2.sh` turns them into `color`, `normal_gl` and `orm.ktx2`, with the height in the ORM's blue channel. The ground shader (`assets/shaders/ground.wesl`) desaturates the colour and lets the vertex colour set the grass tone. |
| `dry_mud_field_001/` | Dry Mud Field 001 (https://polyhaven.com/a/dry_mud_field_001) | Rob Tuytel (photography), Rico Cilliers (processing) | Bare loess: blots in the open pampa, town ground, road verges and lagoon mud, 3 m a tile | Same maps and conversion. The shader keeps part of its own pale brown hue and darkens it where the ground is wet. |
| `dirt/` | Dirt (https://polyhaven.com/a/dirt) | Charlotte Baglioni | The compacted earth and grit of dirt roads and town streets, 2 m a tile (downloaded 2026-11-04) | Same maps and conversion. The road shader (`assets/shaders/road.wesl`) keeps half its hue and 70 % of its contrast under the road's vertex tint. |

The two layers meet by their heights in the ground shader, which also hides their tiling.

Only the `.ktx2` files are kept in the repository; the 2K JPG sources (about 35 MB) are not. To rebuild a layer, download its maps into the layer's folder under the names the script expects, then convert:

```sh
cd assets/textures/ground
for set in grass_ground dry_mud_field_001 dirt; do
  base="https://dl.polyhaven.org/file/ph-assets/Textures/jpg/2k/$set/${set}"
  curl -fo "$set/color.jpg"     "${base}_diff_2k.jpg"
  curl -fo "$set/normal_gl.jpg" "${base}_nor_gl_2k.jpg"
  curl -fo "$set/roughness.jpg" "${base}_rough_2k.jpg"
  curl -fo "$set/ao.jpg"        "${base}_ao_2k.jpg"
  curl -fo "$set/height.jpg"    "${base}_disp_2k.jpg"
done
cd -
FORCE=1 sh tools/textures_to_ktx2.sh assets/textures/ground/grass_ground assets/textures/ground/dry_mud_field_001 assets/textures/ground/dirt
```

Delete the JPG files again afterwards.

## ambientCG

From **ambientCG** (https://ambientcg.com, licence https://docs.ambientcg.com/license/).

| Folder | Asset | Source | Used for | Changes |
|---|---|---|---|---|
| `Ground102/` | Ground 102 (compressed dirt) | https://ambientcg.com/view?id=Ground102 | The dirt roads' surface when `dirt/` is missing | Only the Color, NormalGL, Roughness and AmbientOcclusion maps are kept. Used alone, a little greyer and flatter than the photo, on the road surface; the vertex colour tints it dirt brown, and its alpha feathers the road's edges into the grass. |
| `ScatteredLeaves009/` | Scattered Leaves 009 | https://ambientcg.com/view?id=ScatteredLeaves009 | Leaf litter under the monte | Only the Color map is kept, as a 512×512 PPM (`*_Color.ppm`). The ground shader blends it in where the monte grows, toned darker and browner, and feathers it out at the patch edges. Downloaded on 2026-10-28. |

Suggested credit line, from ambientCG: "Created using Ground 102 and Scattered Leaves 009 from ambientCG.com, licensed under the Creative Commons CC0 1.0 Universal License."

Downloaded on 2026-10-22 as the `_1K-JPG` packages. Ground 109 and Grass 004, from the same download, drew the pampa until 2026-11-03, when the Poly Haven layers above replaced them.

The game loads these as uncompressed PNM files, with no JPEG decoder and no extra dependency. Each map was converted with ImageMagick and **downscaled to 512×512**:
- Color and NormalGL maps became binary PPM (P6, `*_Color.ppm`, `*_NormalGL.ppm`).
- Roughness and AmbientOcclusion maps became greyscale PGM (P5, `*_Roughness.pgm`, `*_AmbientOcclusion.pgm`).

The original `_1K-JPG_*.jpg` files are only the source of the conversion. They can be deleted from the repository.
