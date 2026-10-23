# Pampa Seca

An Argentine rural-outlaw western set on the dry pampa in 1919,
inspired by Red Dead Redemption. The visual direction is a low-poly 3D world
with PS1/PS2-era low-poly 3D people.

This repository currently holds an open procedural pampa (8 km x 8 km by
default) streamed in chunks under dust fog, a player who goes on foot or on
horseback, a follow camera, settlements and a first errand. The towns are
small railway towns loosely inspired by 1910s La Pampa towns such as
Eduardo Castex (not a historical reconstruction), each at its own place in the
world. The player starts aboard the morning train from General Pico as it pulls
into Monte Nievas, gets off on foot onto its platform, collides with buildings
and cannot leave the world. The platform is a raised floor, not a wall: short ramps lead down from
both ends, and its long sides are steps to walk up or off.

The town is alive: townsfolk wander the streets between intersections, a
saddled horse stands tied at a hitching rail in front of the general store,
and the player barges through the crowd, shoving walkers aside, while walkers
step around the player.

The day has a clock: one game day lasts 48 real minutes (30×, one game minute
every two real seconds). The sun follows it, rising in the east, crossing the
northern sky and setting in the west, warm and low at dawn and dusk and full
by day; nights are dim and bluish under the moon but stay readable, and the
dust fog takes the color of the hour without getting any thicker. The game
begins in the late morning, in full daylight, aboard the morning train from
General Pico: the protagonist, a former hardware-shop clerk from Pico, rides
it into Monte Nievas. The
time of day (`HH:MM`) is shown in the top-left corner, lettered in the game's
pixel font on a paper panel, with the current objective under it and, while
riding the train, the next station and its arrival time
(for example "Próxima: Eduardo Castex, 11:22").

One steam train runs the whole railway line on a timetable: a rural mixed
train of 8 cars (93 m), the locomotive, two passenger coaches and five freight
cars, drawn in a British-built style of about 1919 at roughly PS2-era detail
from shared unit meshes and materials (about 2 200 triangles for the engine
and each coach, 1 100 for a freight car). The engine has a banded boiler, a
smokebox door, a headlamp, cylinders, side and main rods that turn with the
drivers, a cowcatcher, a cab with spectacle windows, a coaled tender and a
few puffs of chimney smoke while moving. The coaches have boarded sides,
framed windows, a clerestory roof and open end platforms with railings; the
freight cars are boarded and iron-strapped; every car rides on bogies with
buffers and couplers. It spends the night at General Pico, leaves Pico at 07:00, stops 25 s at
Monte Nievas, reaches Eduardo Castex, lays over, leaves Castex at 16:00, stops
at Monte Nievas again and is back at Pico for the night (a user change on
2026-10-21; until then it slept at Castex and left it at 07:00). It runs at up to 65 km/h, braking to stop with the
middle of its two coaches alongside each station on the way. At the two
terminals the whole train stands centered on the station, so it turns around in
place during the layover. It takes about 10.7 real minutes
over the 11.0 km of line (Pico 07:00, Monte Nievas 10:42–10:54, Castex 12:22; Castex 16:00, Monte
Nievas 17:26–17:38, Pico 21:22, with the clock at its base rate). The clock
only starts each run: once moving, the train runs in real seconds, and a
jump of the clock fast-forwards it exactly where it would have been. Its
cars follow the curves between towns and shove riders off the track; the
rails themselves stay crossable.

Landmarks carry painted signs in period Spanish: the general store
("ALMACÉN DE RAMOS GENERALES"), the police station ("COMISARÍA"), the church
("IGLESIA"), the station ("ESTACIÓN EDUARDO CASTEX", on its facade and on a
name board on the platform) and the plaza. `pampa-core` names the places and
says where each sign stands (`TownLayout::places`); render letters them in a
procedural bitmap pixel font (no third-party font) on weathered boards.

A paper-style minimap in the top-right corner shows the nearest town north-up,
titled with its name: streets, blocks and buildings by kind, the plaza, the
railway, each place by its initial, walkers as dots, the train's cars when
they are in the map, the errand's target as a gold diamond and the player as an
arrow along its heading. Dirt roads are drawn in a warm brown under the
railway. A small compass rose in its bottom-left corner marks N, E, S and O
(oeste); the map is north-up, so the rose stays put and the arrow turns. It frames the town plus a 30 m margin (not the whole
world) so streets stay legible; out on the pampa the arrow sticks to the map
edge, and a target in another town is pinned to the edge on its side, so it
points the way. Each town's layer is rasterized once, the first
time it is shown; only the markers redraw each frame. `M` toggles an enlarged,
centered map.

### Riding the train

**The opening.** The game starts at 10:32:13 with the protagonist seated in
the first coach of the morning train from Pico, 50 real seconds before it
stops at Monte Nievas. The camera starts behind the coach, framing it and the
town ahead ("Próxima estación: Monte Nievas"). The train brakes into the
station at 10:42:13 ("Estación Monte Nievas"), and two real seconds later,
at 10:42:37, he gets off onto the platform beside the coach door, on foot.
`Space` skips the approach, as a short trip (see Skipping).

**Boarding.** While the train stands at a station (or lays over at a
terminal), `F` within about 4 m of a coach door, on foot on the platform,
boards it. On horseback the clerk only says "Primero bajate del caballo": the
horse stays where it is, it does not travel by train.

**Aboard.** The player sits in the coach and goes wherever the train goes.
`V` cycles three views, named on the HUD ("V: vista - ventana"): seated by
a window inside the coach, the default, on the side that faces the next
station's platform (so the arrival is seen through the window), with the
window from 0.8 m to 1.8 m above the floor around the seated eyes at
1.2 m, looking out with the mouse (or
`Q`/`E`) up to about 120° either way and 40° up or down, so the pampa
scrolls past the window and the coach can be looked into; the orbit around
the coach from 16 m out, which the train's own cars never pull in; and the
whole train from the side, slowly following it. The coaches are hollow
second-class coaches of about 1919: wooden walls with glazed window
openings (the same windows from outside), a planked floor, wooden benches
facing each other in bays, oil lamps under the ceiling and a door at each
end. The world keeps streaming around the moving train. The clock slows to 12× while
riding, so the whole line (about 11 real minutes) reads as about two game
hours. `F` while the train stands at a station gets off onto its platform
beside the coach door; `F` while it runs gets off at the next stop. At the
end of the line the player is put off when the train lays over. Every stop
is announced in a panel at the top of the screen ("Estación Monte Nievas"),
and so is the next one as the train pulls out ("Próxima estación: Eduardo
Castex").

**Skipping.** `Space` aboard skips to the next stop as a short trip: the
train pulls out of the station for real for 3 seconds ("Próxima estación:
…"), the screen fades to black, and the train comes in for the last 3
seconds and brakes to a stop ("Estación …"). The jump happens in the black:
the clock moves by the ride time at 12×, and the train lands exactly where
riding on would have brought it. `Space` again during the trip cuts it short.
The player stays aboard, to get off or ride on, unless the ride ends there.

**Waiting.** On a platform with no train, `F` near its middle fast-forwards
the clock (at the base rate, behind the same fade) until the next train stands
there, the next day's if none comes before midnight.

Trees, stones, shrubs and grass tufts stay off the whole train path, including each
town's own track and its run-outs past the town, not only the links between
towns.

The rules (who may board, where a passenger gets off, the skip and wait
jumps, the opening) are pure `core` logic (`pampa_core::ride`), tested
without the game.

### The first errand

The player arrives on foot. On top of the Monte Nievas platform, beside the
morning train's coaches, the objective reads "Buscá a Don Hidalgo en la plaza". Don Hidalgo
stands in the plaza; press `F` within about 3 m to talk. Monte Nievas has no
comisaría, so he sends the player to report to comisario Elías Farach at the
Castex comisaría, and lends his horse, the one tied at the general store.
Farach's town is not hard-coded: `police_town` in
`assets/dialogue/errand.ron` names it (Eduardo Castex; without it, or when that
town has no police station, the nearest other town that has one), and the
objective line names it too. His
lines play one after the other in speech bubbles, a few seconds each. The horse
is not the player's before that talk ("Este caballo no es mío."); after it,
`F` within about 3 m of the horse mounts it: the tied horse is led away for the
session and the player rides from where it stood. Farach stands in the street
in front of the Castex comisaría's door; `F` within about 4 m, mounted or not,
reports to him and ends the errand. The objective line and the minimap's target
follow each step (Hidalgo, the horse, then Farach) and disappear once the
errand is done.

Once taken, the horse is the player's. `F` while stopped or at a slow walk (up
to 1 m/s) gets off: the player stands beside the horse, on its left (its right
if the left is blocked), and the horse stays there, tied loosely, standing
still and in the way like any tied horse. `F` within about 3 m of it gets back
on. Neither moves the errand on: the horse is taken once, and Farach can be
reported to on foot or mounted.

On foot the player walks at 6 km/h and jogs with `Shift` at 18 km/h, with the same keys, the same collision and a person-sized body.
People make way for somebody on foot without complaint; only the horse gets
them talking.

### An open world in chunks

The ground is built in 200 m square chunks around the rider: chunks within 3
chunks are built (at most one per frame, all of them on the first frame), and
chunks past 4 are despawned with their rocks, shrubs, grass and trees, so
memory stays bounded wherever you ride.

The open land between towns is pampa grassland:
- The ground's colour drifts from dry yellow to grey-green, through vertex colours that are continuous across chunk borders. Over that paint lies a tiled soil-and-grass detail texture (normal, roughness and ambient-occlusion maps too), in world-space UVs on a 6 m tile, so it runs on across chunk borders.
- Grass tufts (paja, flechilla) are scattered densely, tinted from grey-green to dry yellow with the ground, with a few weeds and wildflowers among them. They thin out with distance.
- Caldén and chañar trees grow alone or in small groves (isletas). About one tree in ten is a dead caldén snag; a few have a stump or a fallen log beside them. Chañares grow as dense thickets.
- Tosca stones and small jarilla shrubs are scattered on the open ground.

The plants and stones are low-poly CC0 models from Quaternius' Ultimate Nature Pack, and the ground textures are CC0 from ambientCG. They are read from `assets/` at startup by our own small OBJ and PNM readers (no extra crates), and they are recoloured and rescaled in code for the dry 1919 pampa. A missing file logs a warning, and a procedural stand-in is drawn instead. There is no level-of-detail swap: each tree is the same model out to 200 m and fades out inside the 180 m dust fog. Tufts stay solid within about 40 m of the rider and thin out smoothly farther away. Every model choice, turn and size is seeded by its placement, so a reloaded chunk looks exactly the same. Every asset is credited, with what was changed, in [CREDITS.md](CREDITS.md).

Roads, fences and gates cross that land:
- **Dirt roads** for carts and riders join neighbouring towns (Eduardo Castex–Monte Nievas, 2.1 km; Monte Nievas–General Pico, 6.9 km). Each leaves a town by the street facing the other town and runs on a gently curving, seeded line. It is more direct than the railway, which loses distance on its run-outs and wide curves. A road keeps 40 m clear of the track and crosses it only where its ends lie on opposite sides, and then squarely. In the shipped map, both direct roads stay south of the line.
- **Level crossings and rural tracks:** every railway town has one street that crosses its track by the station, square, at a level crossing. That is the street through the station yard nearest the station, or the ring street carried just past the grid when the yard fills the town's track side. The crossing has timber planks flush under the rail heads, between the rails and in a strip of about half a meter outside each, exactly the road's width along the track. The dirt road itself rises to them as packed earth over 7 m on each side (about 1 in 10 at the steepest), on its own axis and as wide as the road, its edges sloping down to the grass. Riders, people and the camera climb the same rise; the track bed itself is never lowered or raised by the road. A St Andrew's cross stands on a post on each approach. Past it, a short seeded rural track of 300–800 m runs out toward the chacras and estancias and ends in the open land. A rider can always cross; a train car standing on the crossing still blocks it.
- **Road surface:** each road is cut a shallow 15 cm into the ground, level across and easing back into the land beside it, so `Terrain::height` stays continuous. Nothing farther than the cut's reach from the road is touched, and the track bed at a level crossing is left as it is. It wears Ground 102 compressed dirt from ambientCG, with its edges feathered into the grass, which is worn to earth along it. No tree, tuft, stone or shrub grows on a road or its verges.
- **Signposts:** a wooden signpost stands at each town's road exit. Its arrow board names the far town and the distance in kilometres, to a tenth, in the pixel font, for example "GENERAL PICO 6,9 KM".
- **Wire fences (alambrados)** mark out the private land: estancias, tambos, fields and plots. Roads are public, and no fence or gate ever crosses one.
  - Every road is 6.5 m wide and runs between two fences 8.5 m from its centerline, at least 5 m past its edge on each side: a wide "calle entre alambrados" for carts and, later, routes. The fences follow the road's curves and stop at the towns' leveled ground and by the railway, so there is no fence at a level crossing.
  - The land away from the roads is surveyed on a north–south grid, with lines a seeded 300–800 m apart. A grid line stops at the roadside fence and joins it at a corner post.
  - Each field's frontage on a road gets one or two gates (tranqueras, single or double). The grid fences have an opening every 300–500 m: a field gate between paddocks, or a broken stretch with a leaning post, a fallen one and a sagging wire.
  - About 60% of the field gates stand open. The rest are shut across their opening and block riders like the wire. Broken stretches are always passable.
  - Every field keeps a way in: an open gate, a broken span or an unfenced side. Each road frontage keeps at least one open gate.
  - About one paddock in ten is a descampado, an abandoned field with no fence of its own. Its road frontage is unfenced, and only its neighbours' fences border it.
  - Posts stand at most 10 m apart with five wires, closer round tight bends. Every span and every shut gate is a thin static obstacle, split into short boxes along slanted spans: about 125,000 boxes, laid in about 0.1 s. The roads stay open end to end.
  - Opening and closing gates is not in the game yet.

Roads, crossings, signposts, fences and gates stream with the chunks, like the track.

Behind some open gates stand rural settlements (`pampa_core::settlement`, drawn by `pampa_render::settlement`): adobe ranchos under paja or corrugated iron, puestos with their corral and aguada, whitewashed or brick chacras with a galpón and a molino, and estancia cascos with a front gallery, worker houses, corrals, a eucalyptus grove and a poplar row. Dirt huellas (two wheel ruts) lead from the gate to every door. The buildings and props are procedural, with CC0 PBR textures shared one material per surface; every building, prop, corral rail and tree trunk is a static obstacle, and houses and galpones stop the camera. Abandoned wagons rot at some yards and on a few road verges, and Poly Haven barrels, crates, a ladder, a bucket and lanterns sit about the barns and wells.

A structure kind with a model slot (the windpump, the wrecked wagon, the cart, the well and others) draws a glTF model from `assets/models/third-party/<slot>/` when one is there, fitted by `slots.ron`, and its procedural mesh otherwise; see `assets/models/third-party/README.md`. The windpump and the wrecked wagon are 3TD Starter Pack models (CC0) converted from COLLADA with `tools/dae2gltf.py`, a standard-library Python script (see `tools/README.md`).

Everything is placed by `pampa_core::flora`, seeded by the chunk, and kept clear of towns' leveled ground, the railway, the roads, the fence lines and the world edge. Every tree trunk in the world is a static obstacle: about 113,000 of them, planted once at load in about 0.1 s, so a horse hits a trunk even in a chunk that is not drawn yet. The dust fog swallows everything well before the
400 m that is always loaded. Towns stream the same way: a town's buildings,
signs, people and tied horse appear within 600 m of it and go once you are
750 m away. Every town's footprints stay in the logic world at all times
(collision looks them up in a uniform grid). The line train always runs; its
cars are drawn only within 400 m of the rider. Positions are `f32`, so the world is limited to 8 km from its center.

The camera orbits the player a full 360° with the mouse (or Q/E), zooms with
the wheel, eases back behind the player after a moment without camera input
while moving, and looks at a lower point on foot than on horseback. It pulls
in at once when a building, the line train's cars or the platform canopy's
roof would block its view (it may still look under the canopy), and eases
back out over about a second once the view clears. Thin things do not pull
it in. A tree, a stretch of fence, a signpost or a crossing sign that stands
between the camera and the rider fades to see-through instead, and comes back
once the view clears. Only the things indexed near that line of sight are
checked, so this stays cheap.

People are PS1/PS2-style low-poly 3D figures generated in code (no
third-party art), about 4,200–5,000 triangles each, in flat, earthy colours with smooth
shading on skin and rounded limbs and hard edges only on hat brims and the
poncho's folds. Each is built from rigid parts on joints (tapered limbs, a
torso wider at the chest, knees, calves and elbows shaped along the limbs,
a rounded head with cheekbones, chin, jaw, brow, ears, nose and shadowed eye
sockets, hands with thumbs, soled shoes and boots, cuffs and seams, creased
trousers, a wavy gacho brim and deep folds in skirts and ponchos), and animation turns the joints: breathing when idle, a walk with
counter-swinging arms and a forward-leaning jog, paced by speed. Hems, the
underside of the chin and the lower ends of limbs are slightly darker.

The story opens in 1919, and every look follows the setting guide
[`odd/setting-1919.md`](odd/setting-1919.md): a muted palette of dust, ochre,
faded blue, brown, grey and black wool, with bright colour only in accents
(a red pañuelo, a faja, brass buttons).

- **The protagonist** is not a gaucho: he is a humble former hardware-shop
  clerk from General Pico, a small provincial town, starting over in another
  one. Like most people in this world he is not wealthy. He wears a townsman's
  working clothes: a plain work shirt, a worn, scuffed brown leather jacket
  with a turned-down collar, lapels and pockets, faded trousers, plain used
  boots and a gacho (a soft wide-brimmed felt hat, its brim turned slightly
  down). No poncho, bombacha or faja: he is from town, not the countryside.
  Suits, stiff collars, ties and bowlers belong to the few town gentlemen.
- **Townsfolk** come in five looks: a paisano in boina, bombacha, faja,
  pañuelo and alpargatas; a peón in a chaleco, soft wide-brimmed hat and worn
  boots; a Volga German farmhand in a poncho; and two women in ankle-length
  skirts, high-necked long-sleeved blouses and shawls, one in a headscarf and
  one with her hair up.
- **Don Hidalgo** is an old estanciero with a white moustache, a light
  wide-brimmed hat, a good jacket over a dark waistcoat, bombacha, tall boots
  and a rastra. **Comisario Elías Farach** wears a kepi, a dark blue tunic with
  brass buttons, a leather belt, dark trousers and boots.

Horses are 3D low-poly too, at the same fidelity: compact Argentine criollos
about 1.45 m at the withers (about 5,200 triangles each, plus about 700 for
the tack), built from lofted parts like the people: barrel, chest, croup and
withers, a neck with a short mane, a head with muzzle, jowls, ears and
forelock, a full tail, and legs with forearm or gaskin, knee or hock, cannon,
fetlock, pastern and hoof.

- **Coats** are painted per vertex from where each vertex sits on the horse,
  so patterns run across parts: **zaino** (solid dark brown), **tordillo**
  (grey with lighter dapples), **overo** (bay with large white patches and
  stockings, some white from every side), **alazán** (chestnut) and **bayo**
  (sandy dun with black mane, tail and lower legs and a dark dorsal stripe).
  Don Hidalgo's horse, the one the player rides, is bayo; the horse tied at
  every other store gets a coat seeded from its town.
- **Tack** fits 1919 rural Argentina: a recado (a woven sudadera, a leather
  carona, the two bastos and a sheepskin pellón on top, a rawhide cincha with
  iron rings), stirrup leathers and iron stirrups, and a rawhide bridle
  (crownpiece, noseband, cheekpieces, bit rings) with reins. A tied horse
  wears the same tack, its stirrups hanging and the reins over the pommel.
- **Gaits** come from `pampa_core::pose`: idle (breathing, one hind hoof
  resting), the four-beat walk, the trot with diagonal pairs (the slower part
  of the gallop range, up to 6.5 m/s) and the gallop with a moment of
  suspension, a rocking body and a swinging neck. Strides are paced by speed,
  frame-rate independent.
- **Mounted**, the clerk sits on the pellón with his thighs open round the
  barrel, his shins hanging to the stirrups and his hands forward at the
  reins; he rises, rocks and turns with the horse and leans into the gallop.

**Polish, without extra triangles.** Every part has a PBR finish (leather
slightly glossy, brass buttons and the rastra's silver metallic, wool and
cloth matte, satin skin, a slight sheen on the horse's coat, dark glossy
hooves), cached by colour and finish. Ambient occlusion is baked into the
vertex shades: armpits, the crotch, under the jaw and hat brims, under the
horse's belly, between its legs and under the saddle's edge. Small textures
painted in code (64 x 64 each) add faces (eyes, brows, a mouth, mapped onto
the front of the head only), a cloth weave on bombachas, ponchos and shirts,
leather grain and felt, and horse faces with eyes, nostrils and an estrella
(alazán) or a blaze (overo): nine textures, 147 KB in all, against the 3.7 MB
of sprite sheets they replace. Hats tip with the step, ponchos, shawls and
skirts sway, standing people glance around now and then, and horses flick
their ears, swish their tails and nod with the gait. The sun's shadows use
three cascades over the nearest 100 m, the first ending 8 m out, for crisper
figure shadows.

Townsfolk talk back: bump into somebody with the horse and they complain in a
speech bubble above their head ("¡Eh, mirá por dónde andás!" at a walk,
"¡Casi me pisa el pingo, che!" at a gallop). Brushing past at a crawl says
nothing, the same person waits a few seconds before complaining again, and the
same line never repeats twice in a row. Bubbles are paper with an ink outline,
lettered in the bitmap font (now with lowercase, accents and `¡ ¿`), face the
camera and fade out after about 3 s.

### Dialogue is data

What people say lives in editable [RON](https://github.com/ron-rs/ron) files
under `assets/dialogue/`, not in code: `bumps.ron` holds one pool of lines per
bump severity (`walk`, `gallop`), and `errand.ron` holds what Don Hidalgo and
comisario Farach say during the errand. Edit them and restart the game; no
recompiling. The `pampa-content` crate parses and validates them (non-empty pools, at most 40
characters per line); the game checks the characters against the font, and
falls back to a small built-in set of lines with a warning if the file is
missing or invalid. See [`assets/dialogue/README.md`](assets/dialogue/README.md)
for how to write lines.

### Towns are data

Towns are generated from editable configs in
[`assets/world/towns.ron`](assets/world/towns.ron), not hard-coded: grid size
in blocks, block and street widths, the railway's side and distance, plaza
placement and a lot seed. `pampa-core` turns a `TownConfig` into a layout
(`TownLayout::generate`), checking it and explaining what is wrong; buildings,
signs, the street graph, the hitching post, the station stop and the arrival
point all follow the generated grid. Eduardo Castex is 4 x 5 blocks of 40 m
with 10 m streets, and since it has no central block its plaza faces the
station across the street, as in many pampa railway towns. The game builds
every town in the file at its `origin`, skips (with a warning naming them)
towns that overlap an earlier one or reach past the world edge, and falls back
to the built-in Castex when no town is left. The file also sets the world's
size and chunk size. See
[`assets/world/README.md`](assets/world/README.md) for every field and rule.

## Architecture

A Cargo workspace with four members:

| Crate | Path | Role |
|-------|------|------|
| `pampa-core` | `crates/core` | Pure game logic. No Bevy, fully unit-tested. |
| `pampa-content` | `crates/content` | Editable content (dialogue, towns) loaded from RON files. No Bevy. |
| `pampa-render` | `crates/render` | Bevy plugins that draw the world. |
| `game` | `game` | Binary that wires input, core, content and render together. |

The rule: **game logic never knows how things are drawn.**

- Heading is a continuous angle in `core`, never an 8-way enum.
- Collision shapes are owned by logic, never derived from sprite pixels. The town
  layout (`pampa_core::TownLayout`) owns every footprint; render only picks a look
  per building kind.
- Ground height has one source of truth, `Terrain::height`. It also levels the
  ground under every town and cuts the roads in, so streamed chunks always meet
  without seams.
- Roads, level crossings, signposts (`pampa_core::road`) and fences and gates
  (`pampa_core::fence`) are laid out in `core`, pure and seeded. `core` owns
  their obstacles. `render` only draws them and fades thin occluders along the
  camera's line of sight.
- Which chunks and towns exist is pure bookkeeping (`StreamingSet` in `render`,
  `TownActivation` in `core`), tested without a GPU.
- Animation is a logical state (`Idle`, `Walk`, `Gallop`) in `core`, never a frame index.
- Townsfolk, the clock and the train are deterministic `core` simulations
  (`Townsfolk`, one per town; `GameClock`; `LineTrain`, one for the whole
  line, whose run is a pure function of the real seconds since it departed);
  the game steps them and render only reads them. Riding it is `core` too
  (`Ride`, `boarding`, `alight_spot`, `skip_ride`, `wait_for_train`,
  `opening`); the game reads `F` and `Space` and plays the fade
  (`ScreenFade` in `render`).
- The errand is a `core` state machine (`Errand`: `MeetHidalgo`, `TakeHorse`,
  `ReportToFarach`, `Done`), one enum variant per stage, each transition
  guarded by range and stage so it fires once. `core` also says where people
  stand (`TownLayout::plaza_spot`, `police_door`, `platform_spot`), and how
  high the ground is (`World::ground_height`: the terrain, raised by the
  platforms' `Floor`s and their ramps); the game
  reads the `F` key and plays the lines; render draws the HUD and the target.
- Bumps are decided in `core` (`BumpTracker`: speed threshold, gait severity,
  per-walker cooldown; `LinePicker`: seeded, no immediate repeats); the words come
  from `pampa-content`; render only draws the bubble (`Say` → `SpeechBubble`).
- The visual representation is a swappable per-entity component in `render`
  (`CharacterVisual`: `Figure` for a person on foot, `Horse` for a saddled
  horse, `Mounted` for a rider on one). Inserting a new value swaps what is
  drawn; the pure gait poses and secondary motion live in `core`
  (`pampa_core::pose`).

Bevy is pinned to an exact version (`=0.20.0`) and only upgraded between milestones.

## Running

```sh
cargo run -p game
```

Checks:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Controls

| Key | Action |
|-----|--------|
| `W` / `S` | Walk or ride forward / brake and back up |
| `A` / `D` | Turn left / right |
| `Shift` (hold) | Jog on foot, gallop on horseback |
| `F` | Interact: talk to somebody, mount or get off a horse, board or get off the train at a coach door, or wait for the train near a platform's middle |
| `Space` | Aboard the train: skip to the next stop |
| `V` | Aboard the train: cycle the view (window seat, around the coach, the whole train) |
| Mouse | Orbit the camera (the cursor is locked while the window has focus) |
| `Q` / `E` | Orbit the camera left / right |
| Mouse wheel | Zoom in / out (6–25 m) |
| `M` | Toggle the enlarged minimap |
| `Esc` | Quit |

## Credits

Third-party assets (CC0 ground textures from ambientCG and CC0 vegetation models from Quaternius' Ultimate Nature Pack) are listed with their author, source, licence and changes in [CREDITS.md](CREDITS.md), and again next to their files.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
