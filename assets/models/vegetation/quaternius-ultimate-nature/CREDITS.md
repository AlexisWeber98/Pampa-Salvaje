# Credits: Ultimate Nature Pack by Quaternius

Every model in this folder comes from the **Ultimate Nature Pack** by **Quaternius** (https://quaternius.com/packs/ultimatenature.html). It is licensed under the **Creative Commons CC0 1.0 Universal** public domain dedication (https://creativecommons.org/publicdomain/zero/1.0/). See `License.txt`, copied unchanged from the pack. The licence does not require credit, but we give it anyway. The author asks for support on Patreon: https://www.patreon.com/quaternius

- **Downloaded:** on 2026-10-22, by the project owner, from the OBJ package. Only the subset below was copied: low-poly OBJ meshes with flat MTL colours and no textures.
- **Left out:** palms, pines, birches, cactus, snow and autumn variants, willows, corn and lilypads, because they don't fit the dry 1919 pampa.

| Model files | Used in the game as | Changes |
|---|---|---|
| `CommonTree_1`–`5` | Caldén, the umbrella-crowned *Prosopis caldenia* (a stand-in) | Recoloured to dull olive and dark bark, rescaled to 6–9 m, crown widened by up to 25% |
| `CommonTree_Dead_1`–`5` | Dead caldén snags (3 caldén variants in 16, about 10% of all trees) | Bark recoloured weathered grey, rescaled to 6–9 m |
| `TreeStump`, `WoodLog` | Stumps and fallen trunks beside a few caldenes | Recoloured (bark, moss, muted mushrooms) |
| `Bush_1`, `Bush_2` | Chañar thickets (3–5 m), and small jarilla shrubs on the open ground | Recoloured to grey-green, rescaled, the thickets narrowed to 70% of their width |
| `Grass`, `Grass_2`, `Grass_Short` | Grass tufts | Retinted dry yellow to grey-green |
| `Wheat` | Tall dry *paja brava* clumps | Retinted, rescaled |
| `Plant_1`–`5`, `Flowers` | Weeds and sparse wildflowers | Retinted |
| `Rock_1`–`4` | Scattered tosca stones (rare on the pampa) | Recoloured to tosca beige |

The colour and scale changes are applied in code at load time (`crates/render/src/flora.rs`, read by the OBJ reader in `crates/render/src/obj.rs`), so the files themselves stay as shipped by Quaternius.
