# Pampa Salvaje

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

The town is alive: townsfolk wander the streets between intersections (how
many walks each town is set by `townsfolk` in `assets/world/towns.ron`), a
saddled horse stands tied at a hitching rail in front of the general store
(and a city ties a few more at the palenques of its comisaría, church and
station), and the player barges through the crowd, shoving walkers aside, while walkers
step around the player.

The streets are shaded as in the humid pampa. A town's streets are 12 m wide
(General Pico's 13 m, the village Metileo's too) with 3.5 m sidewalks
leaving a 5 m carriageway (Pico's 6 m). All have curbs: brick in a city and
round a town's plaza, and a beaten-earth walk, feathered into the grass,
along every other block of a town and everywhere in a village. A city lines its sidewalks with trees in cazuelas
(a small earth square rimmed in curb stucco, about 0.4 m in from the curb),
one every 17 to 21 m, standing nearly straight under a low, full crown and
kept off doors, corners, lamps, galerías, palenques and signs. General Pico's
plaza has groves between its paths as well as its ring of edge trees; the
sample very big towns have over a third fewer plaza trees than the first
planting (the groves thinned, the rings kept). A town or village has sparse trees on the earth walk in front of some
houses, never a row, and every settlement has yard trees behind its houses,
30% more than first planted, with shrubs and flower beds by the walls
(low-poly, shared meshes). The rural estancias, chacras and puestos plant a
third more yard trees and have shrubs and flower beds by their houses too.

The day has a clock: one game day lasts 48 real minutes (30×, one game minute
every two real seconds). The sun follows it, rising in the east, crossing the
northern sky and setting in the west, warm and low at dawn and dusk and full
by day; nights are dim and bluish under the moon but stay readable, and the
dust fog takes the color of the hour without getting any thicker. The sky
(`crates/render/src/sky/`, one dome and one shader) goes from deep blue to
dusty haze at the horizon, burns orange, pink and violet at dusk and dawn
and turns indigo in the blue hour. It has a visible sun with its halo, a moon
that goes through its phases (a 29.5-day cycle that follows the game
days), a real star map (Milky Way, Magellanic Clouds, the Southern Cross)
wheeling over the south, drifting cumulus and high cirrus that glow at sunset, and birds:
chimangos soaring by day, flocks of palomas and teros crossing, and flocks
flying low to roost at dusk, none at night. The game begins in the mid-morning, in full daylight, aboard the morning train from
General Pico: the protagonist, a former hardware-shop clerk from Pico, rides
it into Monte Nievas. The
time of day (`HH:MM`) is shown in the top-left corner, lettered in the game's
typeface (IM Fell English) on a paper panel, with the current objective under it and, while
riding the train, the next station and its arrival time
(for example "Próxima: Eduardo Castex, 11:46").

One steam train runs the whole railway line on a timetable: a rural mixed
train of 8 cars (93 m), the locomotive, two passenger coaches and five freight
cars, drawn in a British-built style of about 1919, built procedurally from
rounded shapes (lathed boilers, swept roofs) with PBR textures, from shared
unit meshes and materials. The engine, a 2-6-0 with a tender, has a banded boiler, a
smokebox door, a headlamp, cylinders, side and main rods that turn with the
drivers, a cowcatcher, a cab with spectacle windows, a coaled tender and a
few puffs of chimney smoke while moving. The coaches have boarded sides,
framed windows, a clerestory roof and open end platforms with railings; the
freight cars are boarded and iron-strapped; every car rides on bogies with
buffers and couplers. It spends the night at General Pico, leaves Pico at 07:00, stops 25 s at
the Metileo halt and at Monte Nievas, reaches Eduardo Castex, lays over, leaves
Castex at 16:00, stops at Monte Nievas and Metileo again and is back at Pico
for the night (a user change on
2026-10-21; until then it slept at Castex and left it at 07:00). It runs at up to 65 km/h, braking to stop with the
middle of its two coaches alongside each station on the way. At the two
terminals the whole train stands centered on the station, so it turns around in
place during the layover. It takes about 11 real minutes
over the 10.0 km of line (Pico 07:00, Metileo 08:52–09:04, Monte Nievas
10:54–11:06, Castex 12:25; Castex 16:00, Monte Nievas 17:18–17:30, Metileo
19:20–19:33, Pico 21:25, with the clock at its base rate; the Metileo stop
since 2026-10-23, its 40 m blocks since 2026-10-25). The clock
only starts each run: once moving, the train runs in real seconds, and a
jump of the clock fast-forwards it exactly where it would have been. Its
cars follow the curves between towns and shove riders off the track; the
rails themselves stay crossable.

Landmarks carry painted signs in period Spanish: the general store
("PULPERÍA" in a small town, "ALMACÉN DE RAMOS GENERALES" in General Pico),
the police station ("COMISARÍA"), the church
("IGLESIA"), the station ("ESTACIÓN EDUARDO CASTEX", on its facade and on two
name boards at the track edge, one near each end of the platform, past the
coaches, facing the train windows) and the plaza. `pampa-core` names the places and
says where each sign stands (`TownLayout::places`); render letters them in a
Rye wood-type face (embedded TrueType, antialiased) on weathered boards.
The station is an FCO-style brick building with stucco pilasters, hooded
openings and a pedimented central pavilion on both its street and track
fronts, and a canopy over the platform on cast-iron columns standing on it,
with a fascia and a cut timber valance. The
stores and police stations are spread over each town rather than round its
plaza, and General Pico has a store and a comisaría on each side of its line.

A paper-style minimap in the top-right corner shows the nearest town north-up,
titled with its name: streets, blocks and buildings by kind, the plaza, the
railway, each place by its initial, walkers as dots, the train's cars when
they are in the map, the errand's target as a gold diamond and the player as an
arrow along its heading. Dirt roads are drawn in a warm brown under the
railway. A small compass rose in its bottom-left corner marks N, E, S and O
(oeste); the map is north-up, so the rose stays put and the arrow turns. It frames the town plus a 30 m margin (not the whole
world) so streets stay legible, with any settlement reaching into that frame.
More than 20 m from the nearest town (before the rider rides off the town's
frame), or within 20 m of a settlement, it switches to a field frame: 640 m
round the rider (2 m a pixel) with the roads, the railway (one unbroken line,
without the town map's sleeper ticks), settlements
(buildings by kind, corral outlines, huellas as thin dirt lines, trees as dots)
and roadside wrecks. It goes back to the town within 5 m of it and 50 m clear
of every settlement, so it does not flicker. A
target off the map is pinned to the edge on its side, so it points the way.
Each town's layer is rasterized once, the first time it is shown; the field
layer is repainted in place only after the rider rides 80 m from its center;
only the markers redraw each frame. `M` toggles an enlarged, centered map.

### Riding the train

**The opening.** The game starts at 10:43:59 with the protagonist seated in
the first coach of the morning train from Pico, 50 real seconds before it
stops at Monte Nievas. The camera starts behind the coach, framing it and the
town ahead ("Próxima estación: Monte Nievas"). The train brakes into the
station at 10:53:59 ("Estación Monte Nievas"), and two real seconds later,
at 10:54:23, he gets off onto the platform beside the coach door, on foot.
(The opening follows the timetable: the Metileo stop moved it 39 game
minutes later on 2026-10-23, and Metileo's wider blocks, a longer
track through it, 19 game seconds more on 2026-10-25.)
`Space` skips the approach, as a short trip (see Skipping).

**Boarding.** While the train stands at a station (or lays over at a
terminal), `F` within about 4 m of a coach door, on foot on the platform,
boards it. At a terminal's stop spot, the middle of the platform, `F` reaches
the nearest door up to 8 m away for the whole layover. Each station has
timetable boards; `F` beside one reads the day's trains. On horseback the clerk only says "Primero bajate del caballo": the
horse stays where it is, it does not travel by train.

**Aboard.** The player sits in the coach and goes wherever the train goes.
`V` cycles three views, named on the HUD ("V: vista - ventana"): seated by
a window inside the coach, the default, on the side that faces the next
station's platform (so the arrival is seen through the window), with the
window from 0.8 m to 1.8 m above the floor around the seated eyes at
1.2 m, facing forward the way the train runs; the mouse (or `Q`/`E`)
turns the head up to about 120° either way and 40° up or down, so either
window, the pampa scrolling past and the platform on arrival are in reach.
Every time the seat is taken (the opening, boarding, a skipped ride coming
in, `V` back to this view) the head turns forward again; the orbit around
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

Taking Don Hidalgo's horse for the first time also teaches the protagonist to
ride: "Ahora podés montar cualquier caballo" shows at the top of the screen,
and from then on `F` within about 3 m of any standing horse (one tied in
another town, or a free one) gets on it. A town's tied horse is then led away
for the session, and the horse ridden before stays where it was left. Before
that, only Don Hidalgo's horse lets him on. Riding is the first of the
player's abilities (`pampa_core::Abilities`); the lasso, weapons and cars are
kept for later.

On foot the player walks at 1.6 m/s (about 5.8 km/h) and jogs with `Shift` at 18 km/h, with the same keys, the same collision and a person-sized body.
People make way for somebody on foot without complaint; only the horse gets
them talking.

### An open world in chunks

The ground is built in 200 m square chunks around the rider: chunks within 3
chunks are built (at most one per frame, all of them on the first frame), and
chunks past 4 are despawned with their rocks, shrubs, grass and trees, so
memory stays bounded wherever you ride. A chunk streamed in while riding builds
its ground mesh, and lays out any settlement in it, on a background thread
and gets them a few frames later, out in the fog, so crossing a chunk border
never stalls a frame. The ground is smooth-shaded: each corner of a chunk's
grid takes its normal from the slope of the terrain's height field, sampled
on one world-wide lattice, so the ground rolls without facets and chunk
borders shade without seams.

General Pico grew on both sides of the Ferrocarril Oeste, which runs
through the middle of the town (east-west in the game; the real town's grid
and line are turned 45°): the station, its platform, grain stores and yard
stand on the line in the centre, the main plaza a few blocks north-west of
them, and every north-south street clear of the yard crosses the track at a
level crossing with a plank deck and St Andrew's crosses (`RailwayConfig::through`).
Between Monte Nievas and Pico, the train stops at the Metileo halt (station
1906): a store and a handful of houses round a small grid (`density` in the
towns file). At Metileo's west end a branch leaves the line, as the real
Metileo–Arizona branch did, and curves away north-west on 500 m curves to
**Caleufú** (founded 1911), 7.5 km of track ending at its station near the
world's west edge (`RailBranch`, `branches` in the towns file). Caleufú is a
small railway town of 4 x 4 blocks, about two thirds built, with its plaza
and church, a comisaría and a pulpería standing blocks apart, and its own
level crossing and rural track by the station. Track, curves, crossings and
the minimap treat the branch like the line. A **branch shuttle**, a 2-6-2T
tank engine with a coach, runs between Caleufú and a bay track at Metileo and
meets the line train there both ways, so the change is made at Metileo. The
line train keeps to the main line. East-south-east of Pico lies **Laguna La Arocena**, a shallow
freshwater lagoon (`pampa_core::Lagoon`): a squarish, softly rounded shore
of about 215 x 145 m, a little rough at the edges. Its water is a flat,
dark olive-brown sheet 0.6 m under the lowest bank, smooth and reflective
like water, darker in the middle and see-through over the mud near the
shore, with faint, fine wind ripples from a small normal map painted in code
and drifted by its UV transform (one draw, no reflection pass). The ground
is sunk into a basin under it (1.6 m deep in the middle); only a 3 m band of
wet mud rings the waterline, and the grass, stones and shrubs grow again
right past it. Totora reeds, 1.5 to 2.5 m tall, stand in patches along the
water's edge, in one mesh. Riders and walkers wade the shallows but are
turned back where the water gets deep. Fences, settlements and roads keep
off it and its bank, trees stay 12 m back from the water, and the minimap
paints it as water.

**Kerosene street lamps (faroles)** light the towns, as in 1919, with no
electric street lighting: a cast-iron post with a glass lantern, about 3.3 m
tall. How many depends on the town's size (`pampa_core::LampTier`):
- **Big towns** (General Pico, 105 lamps): one by each path's mouth in both
  plazas, four more round the main monument, and one at the curb of every
  street corner.
- **Small towns** (Monte Nievas 20, Eduardo Castex 25, Caleufú 21): four in the plaza, one
  by each path's mouth, and one at every other street corner.
- **Villages** of at most 3 x 3 blocks (Metileo, 14): two in the plaza, at
  the mouths of the path toward the station, and corner lamps only round the
  plaza and by the station.

Every road leaves a town between a pair of lamps. Each estancia has one at either end of
its gallery and one by its track inside the gate. Placement is deterministic
and clear of lanes, doors, verandas, plaza paths, the railway, roads, tracks
and every obstacle (`pampa_core::StreetLamps`); each post is a thin obstacle
that fades out of the camera's way. The glass glows from dusk to dawn with
the game clock, fading in and out, and only the 8 lamps nearest the camera
cast real light (a shared pool of shadowless warm point lights, 12 m reach);
by day there are no lamp lights at all (`pampa_render::lamps`).

The open land between towns is pampa grassland:
- The ground's colour drifts from dry yellow to grey-green, through vertex colours that are continuous across chunk borders. Over that paint lies a tiled soil-and-grass detail texture (normal, roughness and ambient-occlusion maps too), in world-space UVs on a 6 m tile, so it runs on across chunk borders.
- Grass tufts (paja, flechilla) are scattered densely, tinted from grey-green to dry yellow with the ground, with a few weeds and wildflowers among them. They thin out with distance.
- Caldén and chañar trees grow alone or in small groves (isletas). About one tree in ten is a dead caldén snag; a few have a stump or a fallen log beside them. Chañares grow as dense thickets.
- Tosca stones and small jarilla shrubs are scattered on the open ground.

The plants and stones are low-poly CC0 models from Quaternius' Ultimate Nature Pack, and the ground textures are CC0 from ambientCG. They are read from `assets/` at startup by our own small OBJ and PNM readers (no extra crates), and they are recoloured and rescaled in code for the dry 1919 pampa. The pack's faceted normals are smoothed once when a model is baked (`pampa_render::smooth`, the same auto-smooth as `tools/glbtrim.py`): crowns, bushes and weeds blend facets up to 60° apart, dead wood 50°, while tosca stones keep the edges sharper than 40°. Grass blends up to 80° and leans its normals up, so a tuft lights like the ground under it. A missing file logs a warning, and a procedural stand-in is drawn instead. There is no level-of-detail swap: each tree is the same model out to 200 m and fades out inside the 180 m dust fog. Tufts stay solid within about 40 m of the rider and thin out smoothly farther away. Every model choice, turn and size is seeded by its placement, so a reloaded chunk looks exactly the same. Every asset is credited, with what was changed, in [CREDITS.md](CREDITS.md).

Roads, fences and gates cross that land:
- **Dirt roads** for carts and riders join neighbouring towns on the line (Eduardo Castex–Monte Nievas, Monte Nievas–Metileo, Metileo–General Pico), and each town on a branch to its nearest town (Caleufú–Eduardo Castex, 5.8 km, crossing the line's run-out west of Castex). Each leaves a town by the street facing the other town and runs on a gently curving, seeded line. It is more direct than the railway, which loses distance on its run-outs and wide curves. A road keeps 40 m clear of the track and crosses it only where its ends lie on opposite sides, and then squarely. A road never leaves General Pico along its line, which runs out of both ends of the town. A public dead-end road also leaves General Pico by its east side for Laguna La Arocena, with a signpost naming the lagoon at its start; it ends on the lagoon's bank in a small turnaround loop that comes to the wet mud and no farther.
- **Level crossings and rural tracks:** every railway town has one street that crosses its track by the station, square, at a level crossing. That is the street through the station yard nearest the station, or the ring street carried just past the grid when the yard fills the town's track side. The crossing has timber planks flush under the rail heads, between the rails and in a strip of about half a meter outside each, exactly the road's width along the track. The dirt road itself rises to them as packed earth over 7 m on each side (about 1 in 10 at the steepest), on its own axis and as wide as the road, its edges sloping down to the grass. Riders, people and the camera climb the same rise; the track bed itself is never lowered or raised by the road. A St Andrew's cross stands on a post on each approach. Past it, a short seeded rural track of 300–800 m runs out toward the chacras and estancias and ends in the open land. A rider can always cross; a train car standing on the crossing still blocks it.
- **Road surface:** each road is cut a shallow 15 cm into the ground, level across and easing back into the land beside it, so `Terrain::height` stays continuous. Nothing farther than the cut's reach from the road is touched, and the track bed at a level crossing is left as it is. It wears Ground 102 compressed dirt from ambientCG, with its edges feathered into the grass, which is worn to earth along it. No tree, tuft, stone or shrub grows on a road or its verges.
- **Signposts:** a wooden signpost stands at each town's road exit. Its arrow board names the far town and the distance in kilometres, to a tenth, in the same Rye face, for example "GENERAL PICO 6,9 KM".
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

The towns' buildings are built with the same kit (`pampa_render::facade`: the mesh builder, the shared textured materials, walls, roofs and the Italianate dressing) as 1919 Italianate pampa fronts. Houses are whitewashed, lime-washed, painted in a muted 1919 colour (lime white, ochre, pale rose, grey-blue or light terracotta) over a beige plinth, bare brick, brick under a worn wash, or now and then old brick losing its plaster, with chamfered corner piers, pilasters between the bays, a moulded cornice and a parapet (solid or balustraded, some with urns or a small pediment) hiding a flat azotea or a corrugated roof; on the outskirts most are plainer adobe or whitewashed cottages under iron. The general store has a tall parapet behind its sign and a galería (veranda) over the sidewalk on cast-iron posts, a small town's pulpería one on timber posts; the comisaría has barred windows, a balustrade and its flagpole; the church a pedimented nave and a bell tower with a belfry of round arches under a zinc spire. Sheds and grain stores are corrugated-iron galpones, which collide at their walls (and stop the camera there) while their eaves reach their lots' edges, and the windmills and water tanks are the settlements' molino and australian tank. Brick sidewalks (veredas) with a stucco curb follow the town's size (`pampa_core::town::Paving`): along every block front in General Pico, only along the plaza's own edge in the small towns (Eduardo Castex, Monte Nievas, Caleufú), and none in a village of 3 x 3 blocks or fewer (Metileo), whose streets stay dirt. They lie a few centimeters proud on the street, like the plaza paths, and change nothing in how anyone walks. Every masonry front keeps inside its core footprint, so colliders and facade signs stay true. Each building is one mesh per surface; a town streamed in builds them on a background thread, and they are drawn within the settlements' shared visibility ranges.

The streets are dirt: one mesh of compacted earth per town (`Surface::Earth`, `dry_ground_01`) with world-space UVs, so the strips meet at the crossings without a seam, topped 3 cm over the ground as before. The streets wear no wheel ruts, since people walk and ride them. Cart tracks (two ruts of `brown_mud_dry`, a few millimeters over the dirt) mark only the entrances carts use: the estancias' huellas from their gate, and a short track from the street to the door of each of the station yard's goods buildings (the 25 m shed and the grain stores), never at the station, the houses, the stores or the plazas.

Besides the general store's hitching post, `pampa_core::town::Palenque` puts a hitching rail in front of each other general store, comisaría, church and station, beside the door and clear of signs, lamps, plaza furniture, the platform and the line; each rail, and each horse, is an obstacle. In a city (a Big town or bigger, General Pico) a seeded half of them, at most three, have a horse tied (a game entity, `PalenqueHorse`); the posts of towns and villages stay empty. `pampa_render::facade::palenque` builds them (chamfered posts and a round bar in weathered boards, one mesh per town, drawn within the props' range).

Each plaza is laid out by core (`pampa_core::town::PlazaFurniture`) and dressed by its town's size, like the sidewalks (`pampa_render::facade::plaza`):
- **General Pico**: brick paths with stucco curbs and an iron railing over a stucco plinth between piers.
- **Small towns** (Eduardo Castex, Monte Nievas, Caleufú): earth paths with brick curbs and no railing.
- **Villages** (Metileo): earth paths, no curbs and no railing.

The two paths still cross the plaza, 3 m wide, and meet on a paved square round the monument. The monument is in the manner of the Pirámide de Mayo: stucco steps, a whitewashed pedestal with bronze plaques under a cornice, a tapering obelisk and an iron finial. It is about 4.5 m tall in a village, 6.4 m in a town and 8.8 m in General Pico, where urns stand on its cornice. Slatted wooden benches on cast-iron ends stand in the beds beside the paths, facing them: three on each side of each path in General Pico, but only one by each path in a small town and two in all in a village, so the small plazas stay sparse. General Pico's railing opens at every path's mouth.

The monument's lowest step, the benches and the railing are obstacles. The paths, the crossing round the monument, the lamps and the townsfolk's streets stay clear, and tests walk a person and a rider through every plaza. Each plaza is two meshes per surface: paths and monument within the buildings' range, benches and railing within the props'. They are built on the background thread with the buildings. Pico's main plaza has about 22k triangles.

The plaza trees are the trees of 1919 pampa plazas: the paraíso's round umbrella of pinnate leaves, the olmo's tall vase, the tall eucalipto with its hanging leaves and the low, feathery aromo. Each plaza plants one species, picked from the town's seed, and a station plaza plants another. A tree is a tapered bark trunk with limbs and twigs, carrying alpha-tested leaf cards (between about 720 and 960 triangles). Every species has three seeded variants, built once and shared by every plaza tree, turned and scaled a little per tree, so the towns add four bark and four leaf materials in all. The trees fade out of the camera's way like the settlements' trees. The bark and leaf textures are ambientCG CC0 sets: `tools/leaf_atlas.py` lays the scanned leaves into sprig atlases (the paraíso's pinnate leaf is composed from single leaflets), and `tools/textures_to_ktx2.sh` packs each atlas's opacity into its colour's alpha (see `assets/textures/trees/CREDITS.md`).

**Fauna** (first pass, `pampa_core::fauna`, drawn by `pampa_render::fauna`, streamed by the game's `fauna` module). Herds of Hereford, Aberdeen Angus and overo cattle graze inside some fenced fields. Each herd keeps to a pasture that is clear of roads, towns, lagoons, the railway and settlements. Cows walk slowly, graze head down and stand about; they do not lie down. A herd of 10 head or more runs a bull of its breed. They walk off when a rider comes close, and they block the rider like soft bodies. Untacked horses stand in the corrals of the chacras and estancias, in the criollo coats, now idling, now cropping the ground. Once riding is unlocked, any of them can be mounted. Hares sit and hop in the open grass and bolt from a rider. Teros take off on a low arc and land farther on. Chimangos circle overhead, and ducks paddle on Laguna La Arocena. The cows, bulls and corral horses are Quaternius' animated CC0 models (`assets/models/fauna/quaternius/`, trimmed by `tools/glbtrim.py`). They are tinted per coat from shared material copies, and their clips follow the simulation: eating, idling, walking paced to the ground speed, galloping. Past 120 m, or out of view, an animal freezes on its pose. The smaller animals are a few rigid low-poly parts that share 4 meshes and 10 materials, animated near the camera only. Animals are loaded only in the chunks within two of the player's chunk, nearest first, under hard caps per species (`pampa_core::fauna::caps`). In debug builds the console's `fauna` command counts what is nearby.

A structure kind with a model slot (the windpump, the wrecked wagon, the cart, the well and others) draws a glTF model from `assets/models/third-party/<slot>/` when one is there, fitted by `slots.ron`, and its procedural mesh otherwise; see `assets/models/third-party/README.md`. The windpump and the wrecked wagon are 3TD Starter Pack models (CC0) converted from COLLADA with `tools/dae2gltf.py`, a standard-library Python script with its own tests (`python3 -I -B -m unittest discover -s tools`; see `tools/README.md`). The thatch textures are made by `assets/textures/buildings/thatch/generate.sh`, whose `--check-signs` proves the normal map's OpenGL convention on known ramps. The building textures ship as GPU-compressed KTX2 next to their JPG sources: `tools/textures_to_ktx2.sh` encodes each set to UASTC with baked mipmaps and packs occlusion, roughness and metallic into one `orm` map, and Bevy transcodes them at load to the GPU's block format (BC7 on desktop), a quarter of the video memory RGBA8 took (see `tools/README.md`).

Everything is placed by `pampa_core::flora`, seeded by the chunk, and kept clear of towns' leveled ground, the railway, the roads, the fence lines and the world edge. Every tree trunk in the world is a static obstacle: about 113,000 of them, planted once at load in about 0.1 s, so a horse hits a trunk even in a chunk that is not drawn yet. The dust fog swallows everything well before the
400 m that is always loaded. Towns stream the same way: a town's buildings,
signs, people and tied horse appear within 600 m of it and go once you are
750 m away. Every town's footprints stay in the logic world at all times
(collision looks them up in a uniform grid). The line train always runs; its
cars are drawn only within 400 m of the rider. Positions are `f32`, so the world is limited to 8.4 km from its center.

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

Off the train, `V` toggles a first-person view from the player's eyes, to
see the details up close, and back; the camera glides between the two in a
quarter of a second, and the choice holds for the session. The eyes follow
the head bone of the protagonist's model, on foot and in the saddle, with
the bob of the walk and the gallop damped and the view kept level. The
mouse (or `Q`/`E`) looks round, up to 80° up or down; on foot the body
turns to the view while walking, and in the saddle the head turns up to
120° either way from the horse's heading while the horse steers as always.
The player's own head is not drawn from the eyes: his face and skull are
cut out of his skin by their skin weights when the model loads, the cut
running round the neck inside the collar, and they go with the hat, hair
and eyes onto a render layer that camera leaves out, while the sun still
casts the whole shadow. The eyes never fall more than 3 cm behind the head,
even at the gallop. The arms, the hands, the collar, the reins and the
horse's neck and ears stay in view. When a wall pulls the orbit camera in
to within 0.6 m of the player's head, it stops drawing his model (the
shadow stays) until it is back past 0.8 m. The eyes see from 5 cm (the near plane), with a 75° vertical field
of view, and the sun's first shadow cascade shortens from 12 m to 6 m.
Aboard, `V` keeps cycling the ride views, whose window seat is the
passenger's own view; a first-person choice comes back on getting off.

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
- **Townsfolk** come in five looks, each a MakeHuman model of its own age
  and build: a young paisano in boina, bombacha, faja, pañuelo and
  alpargatas; a lean criollo peón with a moustache, in a chaleco, soft
  wide-brimmed hat and worn boots; a big, bearded Volga German farmhand in a
  poncho; and two women in ankle-length skirts, high-necked long-sleeved
  blouses, crossed shawls and high laced boots, an older countrywoman in a
  headscarf and an apron and a young townswoman with her hair in a bun. Each
  person's clothes come in their own colours, from a muted 1919 palette.
- **Don Hidalgo** is an old estanciero with a white moustache, a light
  wide-brimmed hat, a good jacket over a dark waistcoat, bombacha, tall boots
  and a rastra. **Comisario Elías Farach** wears a kepi, a dark blue tunic with
  brass buttons, a leather belt, dark trousers and boots.

Saddled horses are Quaternius' animated CC0 horse, the corral horses' model,
about 1.5 m at the withers, in a 1919 recado and bridle, tied up or ridden
(`pampa_render::character::{horse, tack, gait, rider}`):

- **Coats** are painted per vertex from where each vertex sits on the horse,
  so patterns run across the body: **zaino** (solid dark brown), **tordillo**
  (grey with lighter dapples), **overo** (bay with large white patches and
  stockings, some white from every side), **alazán** (chestnut) and **bayo**
  (sandy dun with black mane, tail and lower legs and a dark dorsal stripe).
  Don Hidalgo's horse, the one the player rides, is bayo; the horse tied at
  every other store gets a coat seeded from its town.
- **Tack** fits 1919 rural Argentina: a recado (a woven sudadera, a leather
  carona, the two bastos and a sheepskin pellón on top, a rawhide cincha),
  stirrup leathers and iron stirrups, and a rawhide bridle (crownpiece,
  noseband, cheekpieces, bit rings) with reins. Each layer is wrapped round
  the horse's own skin at rest and skinned to its bones with the weights of
  the skin under it, so it bends with the barrel through every clip and
  never cuts into it. A tied horse's stirrups hang and its reins rest on the
  pommel; a rider's feet carry the stirrups and the reins run along the neck
  to the rider's left hand.
- **Gaits:** the clips' idle, then the walk paced to the ground speed and
  stretched up to about 4.5 m/s (there is no trot clip), then the gallop.
  The walk and the gallop change outright, the new one started in step with
  the old (a fast fade, never half and half); a clip fading out never leaves
  a bone it alone moves behind, and the hooves stay on the legs and above the
  ground. The gallop's stride is stretched to a real gallop's (its hooves keep
  still on the ground up to about 14.6 m/s), and the body flexes with it:
  the croup drops as the hind legs reach under and rises at the push-off, the
  withers pitch the other way, the neck pumps and the tail follows. A tied
  horse changes idle now and then; one out of view or far off keeps its last
  pose, but never the player's.
- **Mounted**, the clerk's MakeHuman model sits on the horse's back bone, so
  he rises, pitches and turns with the horse, playing the seated `drive` clip:
  his legs bent round the barrel to the stirrups and his left hand at the
  reins by a small IK. On top, critically damped springs: his seat gives back
  some of the gallop's bounce, his spine counters the saddle's pitch and roll
  late, his head stays steadier than the saddle, he leans forward with speed
  and back braking, his rein hand gives with the horse's mouth and his right
  hand rests on his thigh. Standing still a few seconds, his hand settles on
  the pommel, his weight shifts and he glances around.

**Polish, without extra triangles.** Every part has a PBR finish (leather
slightly glossy, brass buttons and the rastra's silver metallic, wool and
cloth matte, satin skin), cached by colour and finish. Ambient occlusion is
baked into the vertex shades: armpits, the crotch, under the jaw and hat
brims. Small textures
painted in code (64 x 64 each) add faces (eyes, brows, a mouth, mapped onto
the front of the head only), a cloth weave on bombachas, ponchos and shirts,
leather grain and felt: four textures, 64 KB in all, against the 3.7 MB of
sprite sheets they replace. Hats tip with the step, ponchos, shawls and
skirts sway, and standing people glance around now and then. The sun's shadows use
three cascades over the nearest 100 m, the first ending 12 m out, just past
the rider at the default camera distance, for crisper figure shadows on the
2048-texel map.

Townsfolk talk back: bump into somebody with the horse and they complain in a
speech bubble above their head ("¡Eh, mirá por dónde andás!" at a walk,
"¡Casi me pisa el pingo, che!" at a gallop). Brushing past at a crawl says
nothing, the same person waits a few seconds before complaining again, and the
same line never repeats twice in a row. Bubbles are paper with an ink outline,
lettered in IM Fell English (antialiased, with lowercase, accents and `¡ ¿`), face the
camera and fade out after about 3 s.

### Dialogue is data

What people say lives in editable [RON](https://github.com/ron-rs/ron) files
under `assets/dialogue/`, not in code: `bumps.ron` holds one pool of lines per
bump severity (`walk`, `gallop`), and `errand.ron` holds what Don Hidalgo and
comisario Farach say during the errand. Edit them and restart the game; no
recompiling. The `pampa-content` crate parses and validates them (non-empty pools, at most 40
characters per line); the game checks the characters against the typeface, and
falls back to a small built-in set of lines with a warning if the file is
missing or invalid. See [`assets/dialogue/README.md`](assets/dialogue/README.md)
for how to write lines.

### Towns are data

Towns are generated from editable configs in
[`assets/world/towns.ron`](assets/world/towns.ron), not hard-coded: grid size
in blocks, block and street widths, the railway's side and distance, plaza
placement and a lot seed. `pampa-core` turns a `TownConfig` into a layout
(`TownLayout::generate`), checking it and explaining what is wrong; buildings,
signs, the street graph, the hitching posts, the station stop and the arrival
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
  drawn; the townsfolk's pure gait poses and secondary motion, and the
  horse's gait, live in `core` (`pampa_core::pose`).

Bevy is pinned to an exact version (`=0.20.0`) and only upgraded between milestones.

## Running

```sh
cargo run -p game
```

On Linux the build links with LLVM's `lld` through `clang` (`.cargo/config.toml`), so both must be installed (Arch: `pacman -S clang lld`). The menu music plays through ALSA, so its development files and `pkg-config` are needed too (Arch: `pacman -S alsa-lib pkgconf`; Debian/Ubuntu: `apt install libasound2-dev pkg-config`). Dev builds keep only line tables for our crates and no debug info for dependencies. On this machine that makes an incremental rebuild after touching `crates/render` take about 12 s instead of 47 s, and the dev binary 172 MB instead of 1.6 GB.

Checks:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Controls

The game opens on a title screen. On the left, a "Se busca" poster of Juan Bautista Bairoletto and Mate Cosido (born Segundo David Peralta), two of the rural bandits of León Gieco's "Bandidos rurales", is pinned over an 1882 map of Argentina. On the right, a band of red paint carries "Pampa Salvaje" and the choices Jugar, Sonido and Salir. `Esc` (or the keyboard's `Pause` key) in game opens the same screen over the world. The world stays visible, turned grey with dark corners, and it stands still: virtual time is paused, so the clock, the trains, the animals and the people wait. Sonido sets the general volume and the music's, from 0 to 100 % in steps of 10. They are saved, when the player leaves Sonido or quits with it open, in `settings.ron` under the user's config directory (`~/.config/pampa-salvaje/` on Linux, `%APPDATA%\pampa-salvaje\` on Windows). The menu music (`assets/menu/menu_2.mp3`, the project's own song) loops only while a menu is open. The dev screenshot tour (`PAMPA_SHOTS`) skips the title. `tools/menu_poster.sh` bakes the poster and the band. See `game/src/menu.rs` and `game/src/sound.rs`.

| Key | Action |
|-----|--------|
| `W` / `S` | Walk or ride forward / brake and back up |
| `A` / `D` | Turn left / right |
| `Shift` (hold) | Jog on foot, gallop on horseback |
| `F` | Interact: talk to somebody, mount or get off a horse, board or get off the train at a coach door, or wait for the train near a platform's middle |
| `Space` | Aboard the train: skip to the next stop |
| `V` | Off the train: toggle the first-person view from the player's eyes. Aboard the train: cycle the view (window seat, around the coach, the whole train) |
| Mouse | Orbit the camera, or look round in first person (the cursor is locked while the window has focus) |
| `Q` / `E` | Orbit the camera left / right, or turn the head in first person |
| Mouse wheel | Zoom in / out (6–25 m; not in first person) |
| `M` | Toggle the enlarged minimap |
| `Esc` or `Pause` | Pause: the world stands still and the menu opens (Continuar, Sonido, Salir); `Esc` or `Pause` again resumes, or goes back from Sonido |
| `↑` / `↓` or `W` / `S`, then `Enter`, `Space` or `F` | Choose in the title screen and the pause menu |
| `←` / `→` or `A` / `D` | In Sonido: turn the highlighted volume down / up |

### Dev console (debug builds only)

`Ctrl`+`Shift`+`F12` opens a small cheat console at the bottom left in debug
builds (`cargo run -p game`). It is compiled out of `--release` builds: the
module is declared under `cfg(debug_assertions)`. `Enter` runs the line, `Esc`
or the same keys close it, and while it is open the game gets no keys at all.

| Command | Effect |
|---------|--------|
| `goto lake` | Go to the landing of the nearest lagoon (Laguna La Arocena) |
| `goto pico` / `castex` / `nievas` / `metileo` / `caleufu` / `arocena` | Go to a town's main road in, or a lagoon's landing; any word of a town's or lagoon's name works, accents optional |
| `goto ranch` | Go to the patio of the nearest estancia |
| `goto ranch castex` / `goto ranch pico` | Go to the estancia nearest that town |
| `goto herd` | Go to the pasture of the nearest herd of cattle |
| `goto X Y` | Go to a point, in game meters |
| `horse` | Stand a saddled horse beside the player (rideable once riding is unlocked) |
| `turbo on` / `turbo off` | Let the player's horse gallop at up to 70 km/h, to get round the map fast, or back to normal |
| `unlock all` | Unlock every ability |
| `models on` / `models off` | Draw the people on foot (the protagonist, Don Hidalgo, Farach, Bairoletto and the townsfolk) as the skinned MakeHuman models (the default), or back as the old rigid figures |
| `time 21` / `time 21:30` | Move the clock on to that time (later today, or tomorrow if it has passed) |
| `view fp` / `view tp` | Look from the player's eyes, or from the orbit camera (like `V` off the train) |
| `help` | List the commands |

The MakeHuman models (`assets/characters/`, built by `tools/mh2gltf.py`) wear
a 1919 wardrobe, mostly cut from the body by the converter: shirts and
waistcoats, bombachas, knee boots, a police guerrera with brass buttons, a
three-piece suit, felt hats, a kepi, a boina, neckerchiefs, a poncho, the faja
and the rastra, and for the women blouses, long bell skirts, aprons, crossed
shawls, a headscarf and hair worn up in a bun. They move with Quaternius' CC0
animation clips, retargeted to their skeleton by `tools/retarget.py`: they
stand, walk, jog and run, the clips paced to the ground speed (and to each
person's legs) so the feet do not slide. The pack's crouched game walk is
stood up into an easy upright street walk. The townsfolk's tinted clothes
are shared material copies, and the women's arms hang a little out over
their skirts. Farach waits with his arms
folded, and whoever is speaking talks with their hands. In the saddle the
protagonist plays the seated `drive` clip on the horse's back. They are
the default, in release builds too; `models off` brings back the old rigid
figures of the people on foot in a debug build (riders and saddled horses
have no rigid figures any more). Try `goto castex` (Farach with Bairoletto at his
side) or `goto nievas` (Don Hidalgo).

`goto` puts the player, and the horse when riding, on the nearest clear, dry
ground to the place, and the world streams in around them at once. Aboard the
train it refuses ("not on the train: get off first"): get off first.

## Credits

Third-party assets (CC0 ground textures from ambientCG, CC0 vegetation models from Quaternius' Ultimate Nature Pack, the CC0 MakeHuman assets of the characters, and their CC0 animation clips from Quaternius' Universal Animation Library) are listed with their author, source, licence and changes in [CREDITS.md](CREDITS.md), and again next to their files.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
