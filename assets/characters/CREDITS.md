# Character credits

Every asset here comes from MakeHuman 1.x's data folder by the MakeHuman Team (http://www.makehumancommunity.org/). They are CC0 1.0, as stated in MakeHuman's `LICENSE.ASSETS.md`. The `.mhm` and `.mhmat` files are our own. Each `model.gltf` (with its `model.bin` and `textures/`) is built from them by `tools/mh2gltf.py`: the base mesh is morphed by the `.mhm`'s modifiers, every proxy is fitted to it, faces under the clothes and hair above a hat's brim are left out, the mesh is skinned to the `game_engine` skeleton with the default weights, and the textures are downscaled to PNG (1024 px for the skin and outfit, 512 px for the rest).

Shared by all four:

| Asset | Licence |
|---|---|
| Base mesh `3dobjs/base` and the targets in `targets.npz` | CC0 1.0 |
| Body proxy `proxymeshes/male1591` (1,591 vertices; 3,140 triangles before faces are hidden) | CC0 1.0 |
| Skeleton `rigs/game_engine.mhskel` (53 joints), bone orientation and weights from `rigs/default.mhskel` and `rigs/default_weights.mhw` | CC0 1.0 |
| Eyes `eyes/low-poly` | CC0 1.0 |

## Per character

| Character | Skin | Eyes | Eyebrows | Hair | Clothes | Hat |
|---|---|---|---|---|---|---|
| `protagonist` | `young_caucasian_male` | `brownlight` | `eyebrow001` | `short04` | `male_casualsuit05`, recoloured brown (`jacket.mhmat`); `shoes01` | `fedora_cocked` |
| `bairoletto` | `young_caucasian_male2` (stubble) | `bluegreen` | `eyebrow002` | `short02` (light brown) | `male_casualsuit02`, darkened to near black (`black_shirt.mhmat`); `shoes04` | `fedora` |
| `farach` | `middleage_caucasian_male` | `brown` | `eyebrow005` | `short04` | `male_casualsuit03`, darkened to navy (`tunic.mhmat`); `shoes03` | `fedora` with `black_fedora.mhmat` |
| `hidalgo` | `middleage_caucasian_male` | `grey` | `eyebrow003` | `short04` | `male_casualsuit05` as shipped; `shoes04` | `fedora` with `grey_fedora.mhmat` |

## Substitutions

MakeHuman's CC0 wardrobe is modern casual wear, so each 1919 garment stands in as the closest available piece. These models are therefore off by default in the game: the console's `modelos on` shows them.

| Character | Wanted (1919) | Used instead |
|---|---|---|
| all | bombacha or period trousers | the suits' jeans (darkened for Bairoletto and Farach) |
| all | boots | leather or black shoes (`shoes01`, `shoes03`, `shoes04`) |
| protagonist | work shirt and worn leather jacket | `male_casualsuit05`'s shirt and field jacket, tinted leathery brown |
| protagonist | soft felt gacho | `fedora_cocked` |
| bairoletto | black shirt | `male_casualsuit02`'s long-sleeved shirt, darkened (its print still shows faintly) |
| bairoletto | white neckerchief, short beard, centre parting | none (no asset); the stubble skin stands in for the beard |
| bairoletto | chambergo | `fedora` |
| farach | kepi | the black `fedora` |
| farach | dark tunic with brass buttons | `male_casualsuit03`'s striped shirt, darkened to navy; no brass |
| hidalgo | good jacket and waistcoat | `male_casualsuit05` (field jacket over a shirt); no waistcoat |
| hidalgo | white moustache and white hair | none (no facial-hair asset; hair textures can only be darkened); mostly under the hat |
| hidalgo | wide-brimmed felt hat, tall boots | grey `fedora`, `shoes04` |

The eyelashes were left out: 368 triangles each that cannot be seen at the game's distance. Period garments and accessories are planned as procedural pieces bound to the skeleton (see `odd/tasks/makehuman-characters.md`).
