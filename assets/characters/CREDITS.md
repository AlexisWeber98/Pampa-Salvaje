# Character credits

The `.mhm` and `.mhmat` files are our own. Each `model.gltf` (with its `model.bin` and `textures/`) is built from them by `tools/mh2gltf.py`: the base mesh is morphed by the `.mhm`'s modifiers, every proxy is fitted to it, faces under the clothes and hair above a hat's band are left out, heavy pack garments are decimated, the mesh is skinned to the `game_engine` skeleton with the default weights, the arms are lowered into the rest pose, and the textures are downscaled to PNG (1024 px for the skin and outfit, 512 px for the rest).

Most of the clothes are made by the converter from the `.mhm`'s `piece` lines, not taken from third-party assets: the shirts and blouses, waistcoats, Farach's guerrera, Don Hidalgo's suit, the trousers, the bombachas, the boots, the alpargatas, the shawls, the headscarf and the women's hair worn up are cut from the body proxy (`male1591`, or `female1605` for the women) and keep its skin weights; the hats, the kepi, the boina, the bun, the neckerchiefs, the faja, the rastra, the poncho, the strap, the lapels, the collar, the watch chain, the brass buttons, the shawls' crossed ends, the headscarf's knot and point, the long skirts and the apron are generated meshes. Their small cloth and leather textures (`cotton.png`, `wool.png`, `stripe.png`, `leather.png`, `poncho.png`) are made in code too.

## Sources

| Asset | Author | Licence | Source |
|---|---|---|---|
| Base mesh `3dobjs/base` and the targets in `targets.npz`; body proxies `proxymeshes/male1591` (3,140 triangles before faces are hidden) and `proxymeshes/female1605`; skeleton `rigs/game_engine.mhskel` (53 joints), bone orientation and weights from `rigs/default.mhskel` and `rigs/default_weights.mhw`; eyes `eyes/low-poly`; skins, eyebrows and hair | The MakeHuman Team | CC0 1.0 (MakeHuman's `LICENSE.ASSETS.md`) | MakeHuman 1.x's data folder, http://www.makehumancommunity.org/ |
| `toigo_harem_pants` | MargaretToigo | CC0 1.0 | asset pack `pants01`, https://static.makehumancommunity.org/assets/assetpacks/pants01.html |
| `punkduck_riding_boots` | punkduck | CC-BY (attribution; the pack states no version) | asset pack `shoes03`, https://static.makehumancommunity.org/assets/assetpacks/shoes03.html |
| `grinsegold_moustache` | grinsegold | CC-BY (attribution; the pack states no version; its `.mhclo` header carries MakeClothes' default AGPL3 line, but the pack lists it as CC-BY) | asset pack `bodyparts06`, https://static.makehumancommunity.org/assets/assetpacks/bodyparts06.html |
| `culturalibre_faun_beard` | culturalibre | CC0 1.0 (its `.mhclo` header: `license CC-0`) | MakeHuman community assets, http://www.makehumancommunity.org/ (installed in MakeHuman's data folder, `clothes/culturalibre_faun_beard`) |

## Per character

| Character | Skin | Eyes, eyebrows, hair | Pack clothes | Made by the converter |
|---|---|---|---|---|
| `protagonist` | `young_caucasian_male` | `brownlight`, `eyebrow001`, `short04` | none | cream shirt, dark waistcoat, baggy bombacha, calf-high brown boots; gacho (low crown, brim dipping at the front), red neckerchief, dark faja, rastra with coins, dark red poncho with cream bands folded over the left shoulder |
| `bairoletto` | `young_caucasian_male2` (stubble) | `bluegreen`, `eyebrow002`, `short02` (light brown) | `toigo_harem_pants` as the bombacha, charcoal (`bombacha.mhmat`), decimated; `punkduck_riding_boots`, decimated; `grinsegold_moustache`, light brown (`moustache.mhmat`) | dark shirt, brown waistcoat; chambergo (round pinched crown, wide soft brim), white neckerchief knotted at the front |
| `farach` | `middleage_caucasian_male` | `brown`, `eyebrow005`, `short04` | `grinsegold_moustache` as shipped (`moustache.mhmat`) | dark navy guerrera with a stiff high collar and a skirt, navy trousers tucked into black knee boots; kepi with a black band, visor and chin strap; black belt with a brass buckle, shoulder strap, two columns of brass buttons |
| `townsfolk_paisano` | `young_caucasian_male` | `brown`, `eyebrow004`, `short02` (under the boina) | none | collarless off-white shirt, dark bombacha gathered at the ankle, red faja, red neckerchief, dark blue boina pulled down to one side, canvas alpargatas |
| `townsfolk_peon` | `middleage_caucasian_male` (a criollo build: some Asian and African in the macros) | `brown`, `eyebrow006`, `short04` | `grinsegold_moustache` as shipped (`moustache.mhmat`) | dun shirt, brown chaleco, dun bombacha into worn boots, dark faja, white neckerchief, soft felt hat with a drooping brim |
| `townsfolk_volga` | `middleage_caucasian_male` | `lightblue`, `eyebrow003`, `short04` light brown (`hair.mhmat`) | `culturalibre_faun_beard`, light brown from a greyscale copy of its texture (`beard.mhmat`), decimated from 10,432 to 3,408 triangles | dun shirt, dark waistcoat, wool trousers into boots, round-crowned felt hat, a brown woven poncho over the left shoulder |
| `townsfolk_headscarf` | `middleage_caucasian_female` | `brown`, `eyebrow012`; no hair (under the headscarf) | none | faded blue high-necked blouse, brown wool skirt to the ankle, cotton apron, dark shawl with its ends crossed over the chest, high laced boots, dark headscarf knotted under the chin |
| `townsfolk_bun` | `young_caucasian_female` | `green`, `eyebrow001`; hair worn up in a bun (made by the converter) | none | off-white blouse with a high band collar, dark blue wool skirt to the ankle, grey shawl with its ends crossed, high laced boots |
| `hidalgo` | `middleage_caucasian_male` | `grey`, `eyebrow003` grey (`eyebrows.mhmat`), `short04` white (`hair.mhmat`) | `grinsegold_moustache`, white (`moustache.mhmat`) | white shirt, black waistcoat with a gold watch chain, dark open jacket with lapels and padded shoulders, trousers tucked into brown boots, stiff band collar with wing tips; felt hat with the widest flat brim and a tall creased crown |

The eyelashes are left out: 368 triangles each that cannot be seen at the game's distance.

## Animations

`animations/humanoid.glb` holds every character's clips. It is made by `tools/retarget.py` (see `tools/README.md`) from two CC0 packs by Quaternius, downloaded on 2026-10-24. Neither pack is in the repository. Get them from https://quaternius.com/packs/universalanimationlibrary.html (also published on OpenGameArt), unzip them, and keep the `License.txt` next to each.

| Pack | File used | Licence | Clips kept (game name ← pack name) |
|---|---|---|---|
| Universal Animation Library (Standard) | `Godot/AnimationLibrary_Godot_Standard.glb` (Rigify `DEF-*` bones) | CC0 1.0 | `idle` ← Idle_Loop, `idle_talk` ← Idle_Talking_Loop, `walk` ← Walk_Loop, `walk_formal` ← Walk_Formal_Loop, `jog` ← Jog_Fwd_Loop, `sprint` ← Sprint_Loop, `sit_idle` ← Sitting_Idle_Loop, `drive` ← Driving_Loop, `interact` ← Interact |
| Universal Animation Library 2 (Standard) | `Unreal-Godot/UAL2_Standard.glb` (Unreal bone names) | CC0 1.0 | `idle_fold_arms` ← Idle_FoldArms_Loop, `yes` ← Yes, `no` ← Idle_No_Loop, `harvest` ← Farm_Harvest, `plant` ← Farm_PlantSeed, `water` ← Farm_Watering, `chop` ← TreeChopping_Loop, `walk_carry` ← Walk_Carry_Loop |

What was changed: the clips were retargeted from the packs' mannequin to the protagonist's `game_engine` skeleton. The T-pose rest is aligned to the arms-down rest, and the hips' moves are scaled to his legs (1.043 times the mannequin's). Looping clips are held in place, and the hips are raised wherever a foot would go through the ground. The standing clips are calmed from the packs' wide, crouched stance: feet drawn in, knees straightened, arms lowered. The folded arms (12 cm) are brought forward to clear the four men's chests, and the talking hands (6 cm) to clear theirs and the five townsfolk's. The walk is stood up from the pack's crouched game walk: the knees straightened, the torso upright, the shoulders down, the arms swinging at the sides, the stride 1.15 times as long. The packs' other 72 clips and their mannequin mesh are left out.
