# Credits: sky textures

| Folder | Asset | Author / source | Licence | Used for |
|---|---|---|---|---|
| `stars_milky_way/` | Stars + Milky Way (8k) | Solar System Scope (solarsystemscope.com), https://www.solarsystemscope.com/textures/ (`download/8k_stars_milky_way.jpg`) | **Creative Commons Attribution 4.0 International (CC BY 4.0)**, https://creativecommons.org/licenses/by/4.0/ | The night sky: stars, the Milky Way and the Magellanic Clouds |

This licence **requires attribution**: "Stars + Milky Way" texture by Solar System Scope (solarsystemscope.com), CC BY 4.0.

The project owner downloaded the map on 2026-10-27.

## Changes

- `stars_milky_way.jpg` is the unmodified download (8192x4096, an equirectangular map in galactic coordinates). It stays as the source.
- `stars_milky_way.ktx2` is what the game loads: the same image as sRGB UASTC LDR 4x4 with its mip chain and Zstandard supercompression, made with `basisu -ktx2 -uastc -uastc_level 2 -mipmap -srgb -file stars_milky_way.jpg -output_file stars_milky_way.ktx2` (the options of `tools/textures_to_ktx2.sh` for colour maps). Bevy transcodes it at load to the GPU's block format (BC7 on desktop GPUs, about 45 MB with mips).
- The sky shader (`crates/render/src/sky/sky.wesl`) brightens it, rotates it for the pampa's latitude (36.5 S) and the game's sidereal time, and fades it with the sun's height.
