# Credits: plaza tree textures

These are the bark and leaf sets of the plaza trees (paraíso, olmo, eucalipto and aromo) and the open country's trees (caldén and chañar). The ambientCG sets (`Bark…`, `LeafSet…`) come from **ambientCG** (https://ambientcg.com), which publishes every asset under the **Creative Commons CC0 1.0 Universal** public domain dedication (https://docs.ambientcg.com/license/). The licence doesn't require credit, but we give it anyway.

| Folder | Asset | Source | Used for | Changes |
|---|---|---|---|---|
| `Bark014/` | Bark 014 | ambientCG (https://ambientcg.com/view?id=Bark014) | Paraíso bark | 1K JPG maps renamed |
| `Bark001/` | Bark 001 | ambientCG (https://ambientcg.com/view?id=Bark001) | Olmo and álamo bark (the álamo shades it cooler grey) | 1K JPG maps renamed |
| `Bark009/` | Bark 009 | ambientCG (https://ambientcg.com/view?id=Bark009) | Not used now (the eucalipto and chañar moved to Poly Haven sets) | 1K JPG maps renamed |
| `Bark002/` | Bark 002 | ambientCG (https://ambientcg.com/view?id=Bark002) | Aromo bark | 1K JPG maps renamed |
| `Bark005/` | Bark 005 | ambientCG (https://ambientcg.com/view?id=Bark005) | For F4 (ombú); not used yet | 1K JPG maps (Color, NormalGL, Roughness, AmbientOcclusion) renamed; downloaded by the project owner on 2026-10-27 |
| `LeafSet013/` | Leaf Set 013 | ambientCG (https://ambientcg.com/view?id=LeafSet013) | Leaflets of the paraíso atlas | 1K JPG maps renamed; source only |
| `LeafSet001/` | Leaf Set 001 | ambientCG (https://ambientcg.com/view?id=LeafSet001) | Leaves of the olmo atlas (and, through it, the chañar's) | 1K JPG maps renamed; source only |
| `LeafSet003/` | Leaf Set 003 | ambientCG (https://ambientcg.com/view?id=LeafSet003) | Leaves of the eucalipto atlas | 1K JPG maps renamed; source only |
| `LeafSet019/` | Leaf Set 019 | ambientCG (https://ambientcg.com/view?id=LeafSet019) | Sprays of the aromo atlas (and, through it, the caldén's) | 1K JPG maps renamed; source only; tinted silver-green in the game |
| `paraiso_leaves/` | Derived from Leaf Set 013 | ambientCG (https://ambientcg.com/view?id=LeafSet013) | Paraíso leaf cards | Leaflets cut out and laid in opposite pairs along a drawn rachis, as the paraíso's pinnate leaf (no scanned set has one), by `tools/leaf_atlas.py` |
| `olmo_leaves/` | Derived from Leaf Set 001 | ambientCG (https://ambientcg.com/view?id=LeafSet001) | Olmo, chañar and álamo leaf cards | Leaves cut out and laid alternately along drawn twigs by `tools/leaf_atlas.py` |
| `eucalipto_leaves/` | Derived from Leaf Set 003 | ambientCG (https://ambientcg.com/view?id=LeafSet003) | Eucalipto leaf cards | Leaves cut out and hung close along drawn twigs by `tools/leaf_atlas.py` |
| `aromo_leaves/` | Derived from Leaf Set 019 | ambientCG (https://ambientcg.com/view?id=LeafSet019) | Aromo and caldén leaf cards | Sprays cut out and stood upright by `tools/leaf_atlas.py` |

The barks below come from **Poly Haven** (https://polyhaven.com), which publishes every asset under **CC0 1.0** (https://polyhaven.com/license). They were downloaded by the project owner on 2026-10-27 as 1K JPG maps and renamed; the game loads their KTX2 copies like the others.

| Folder | Asset | Author | Source | Used for | Changes |
|---|---|---|---|---|---|
| `bark_willow/` | Bark Willow | Dario Barresi (processing), Dimitrios Savva (photography) | Poly Haven (https://polyhaven.com/a/bark_willow) | Caldén bark, shaded grey-brown in the game | 1K JPG maps renamed |
| `bark_platanus/` | Bark Platanus | Dimitrios Savva | Poly Haven (https://polyhaven.com/a/bark_platanus) | Chañar bark, tinted greenish ochre in the game | 1K JPG maps renamed |
| `bark_bluegum/` | Bark Bluegum | Dimitrios Savva | Poly Haven (https://polyhaven.com/a/bark_bluegum) | Eucalipto bark (replaces Bark 009) | 1K JPG maps renamed |
| `chinese_cedar_bark/` | Chinese Cedar Bark | Charlotte Baglioni | Poly Haven (https://polyhaven.com/a/chinese_cedar_bark) | For F4 (algarrobo); not used yet | 1K JPG maps renamed |
| `chinese_hackberry_bark/` | Chinese Hackberry Bark | Charlotte Baglioni | Poly Haven (https://polyhaven.com/a/chinese_hackberry_bark) | For F4 (tala); not used yet | 1K JPG maps renamed |
| `bark_willow_02/` | Bark Willow 02 | Charlotte Baglioni | Poly Haven (https://polyhaven.com/a/bark_willow_02) | For F4 (espinillo); not used yet | 1K JPG maps renamed |

The source folders hold the sets' 1K JPG maps, renamed, with no other changes: `color.jpg`, `normal_gl.jpg` (OpenGL normal map) and `roughness.jpg`, plus `ao.jpg` for the barks that have one and `opacity.jpg` for the leaf sets. The ambientCG sets were downloaded by the project owner on 2026-10-26 (1K JPG). The zips' other files (Blender, MaterialX, Godot and USD scenes, previews, DirectX normal and displacement maps) are not used.

Each `*_leaves/` folder is a derived work, made from its leaf set by `python3 -I tools/leaf_atlas.py assets/textures/trees` (seeded, reproducible): four sprigs on a 1024×1024 atlas, one to a quarter, with the same four maps. It is CC0 like its source.

The game loads `color.ktx2`, `normal_gl.ktx2` and `orm.ktx2` of the bark and `*_leaves/` folders: GPU-compressed copies (UASTC, mipmaps baked; ambient occlusion, roughness and metallic packed into one map; the leaves' opacity in the colour's alpha) converted from the JPGs with `tools/textures_to_ktx2.sh`. The JPG sources are kept as the editable originals; re-run the scripts after editing one.

Suggested credit line: "Bark and leaf textures from ambientCG.com, licensed under CC0 1.0."
