# Feature: ground-realism

Objective: maximum realism on every ground surface (pampa ground, caminos, streets, sidewalks, plazas, grass, earth) plus dust and splashes at footsteps. Branch `feat/ground-realism`; the user owns commits (changes stay uncommitted). Tests: `systemd-run --user --scope -p MemoryMax=5G cargo test --workspace -- --test-threads=2`; python unittest for tools/.

## Specs

- S1. todo.md (user, verbatim): "máximo realismo y detalle atmósfera, en caminos, calles, veredas, plazas, pastos y gramillas, tierra, texturas y relieves, irregularidades, polvo y/o salpicaduras en pizadas donde (o si) corresponda".
- S2. todo.md standing instruction (user, verbatim): "descargar modelos renderizados de terceros de libre uso siempre que sea posible, con el mayor detalle posible mientras sea soportado por el hardware actual, de no ser posible utilartexturas (descargables o ya disponibles)". Free CC0/CC-BY assets only, credited in the root CREDITS.md and next to the asset; never Sketchfab.
- S3. Quality floor (standing user rule, 2026-10-30): the v13 rhea level of detail is the minimum for everything visual. Must hold on the Renoir iGPU; report frame cost.

## Tasks

Order follows the weakest elements found in the map (L2). Each task is one work unit of about 400 lines.

- T1 [S1, S2, S3] Pampa ground and earth: 2K PBR layers (grass, bare soil) blended by height in the ground shader instead of one CPU-blended 512² tile; anti-tiling (noise-picked offsets plus macro variation); micro-relief through the normal. Route: inline (no writer reason). Status: done, uncommitted; optimised to about +2–3 ms on the Renoir (L6, L7). Evidence: L3, L7.
- T2 [S1, S3] Grass and gramilla: denser tufts near the camera, a short gramilla carpet, wind sway in the vertex stage. Route: inline. Status: done, uncommitted. Near paja tufts and wind are in; the gramilla cards were dropped by the user's choice (L9), so the carpet stays in the ground shader. Evidence: L8, L9.
- T3 [S1, S3] Caminos: wheel ruts, crown and verge irregularity, dry cracked mud and hoof-churned patches in the road shader and strip mesh. Route: inline. Status: done, uncommitted (the user stages and commits). Evidence: L10.
- T4 [S1, S3] Town streets: earth streets with ruts, worn centre, irregular edges, shallow dry hollows; replace the flat 3 cm boxes with ground-following surfaces. Route: inline. Status: done, uncommitted. The towns are levelled, so the boxes stay as the base and a worn layer goes on top (L12). Evidence: L12.
- T5 [S1, S3] Sidewalks, curbs and plazas: real grass on the plaza lawn, worn and broken brick, chipped curbs, plaza paths with edge wear. Route: inline. Status: pending.
- T6 [S1, S3] Dust and splashes at footsteps: footfall events from people, horses, carts and cars; own lightweight particle system (no new dependency); dust on dry earth, splashes on mud and lagoon edges; short-lived prints. Route: inline. Status: pending.

## Log

- L1 (2026-11-03) user: "vamos con la tarea de todo.md, esta: - máximo realismo y detalle atmósfera, en caminos, calles, veredas, plazas, pastos y gramillas, tierra, texturas y relieves, irregularidades, polvo y/o salpicaduras en pizadas donde (o si) corresponda".
- L2 (2026-11-03) Map of the current ground (explorer):
  - Terrain: 200 m chunks with 5 m quads and no relief under 30 m. Colour is per vertex. One 512² PNM detail set (Ground109 + Grass004) is blended on the CPU, deliberately washed out, and repeats every 6 m with no anti-tiling. The only shader is `assets/shaders/ground_litter.wesl`.
  - Caminos: a flat floating alpha strip (Ground102), cut 0.15 m, with no ruts.
  - Streets: flat 3 cm boxes of Poly Haven dirt, with no ruts. Sidewalks are brick or earth decals; curbs are stucco boxes.
  - Plaza lawn: a flat-colour cube.
  - Grass: about 1 tuft per 130 m², no wind.
  - No particles, dust or footprints anywhere. The hoof IK in `crates/render/src/character/gait.rs` is the nearest footfall hook.
  - Downloads are covered by the todo.md standing instruction (S2), as in building-realism L2.
- L3 (2026-11-03) T1 done:
  - Layers: Poly Haven `grass_ground` (2.5 m) and `dry_mud_field_001` (3 m), 2K KTX2. Their height is packed in the ORM's blue channel through a new optional `height.jpg` in `tools/textures_to_ktx2.sh`; `--check-orm` covers it and was proven by sabotage.
  - `assets/shaders/ground.wesl` replaces `ground_litter.wesl`, and `GroundExt` replaces `LitterExt`. Two noise-picked offsets per layer with a seam blend (IQ), via `textureSampleGrad`. Bare blots grow with dryness (read from the vertex red/green). The layers meet by height. There is a slow tone variation and green/straw tussocks, plus centimetre hummocks that tilt the normal and shade their hollows. Contrast is ×1.35 for grass and ×1.2 for soil.
  - The mesh's second UV is (cleared, wet) from the new `ground_cover` (towns, road verges, lagoon mud).
  - Ground109 and Grass004 and the CPU blend are removed. Credits are updated in both CREDITS.md files.
  - Checks: suite green (core 217, content 32, game 507, render 604), clippy clean. Two new tests (second UV; shipped layers load) were each proven by sabotage. Visual check with PAMPA_SHOTS: pampa near/mid, lagoon, Metileo edge, two rounds; in round 1 soil looked red and grass flat.
  - Pending: Renoir frame cost; the lagoon mud shot landed in the water. The user asked whether the JPG sources could be lighter (L4).
- L4 (2026-11-03) user: "pregunta de aparte de ignorante, sería mas economico pasar esos JEPG a SVG o otro formato más liviano sin perdida de calidad?". Answered: SVG is vector and cannot hold photos, and the game loads only the KTX2. Options were offered to cut repo weight: 1K sources like the building sets, or no sources plus the download link. Decision pending.
- L5 (2026-11-03) user: "que se recomendable?" then "si". Decision: drop the 2K JPG sources from the repo; keep only the KTX2. The download and rebuild steps are in assets/textures/ground/CREDITS.md. The agent's `rm` was denied by permissions, so the user deletes the JPGs.
- L6 (2026-11-04) user: "arrancams con lo que quedó pendiente (primero en todo.md)". The T1 leftovers before T2:
  - The JPG sources are already deleted.
  - Renoir frame cost, measured with a dev build, no vsync, 1428×834, PAMPA_SHOTS with 900-frame holds, and temporary diagnostics (since reverted). Layers on: median 16.6 ms on every view (pampa near, pampa mid, lagoon south, lagoon east), p90 about 17.2 ms. Layers off (`layers` = 0): median 6.6 ms in the pampa and 8.3 ms at the lagoon. So the new ground costs about +8 to +10 ms per frame and drops the frame rate from about 150 to 60 fps. The flat 16.6 ms may be quantised to the 60 Hz refresh, so the true cost could be somewhat lower.
  - Lagoon mud: the shots from the bank (3404,735 and 3545,841) show the bank's bare blots, but no visible mud band. The water starts right at the edge, and the 3 m band reads as nothing at eye height.
- L7 (2026-11-04) user: "si, optimicemos antes". T1b, a cheaper ground shader, inline in `assets/shaders/ground.wesl`:
  - Each layer reads its second anti-tiling offset only across the seam (`at.f` from 0.25 to 0.75, crossing from 0.3 to 0.7). Elsewhere each map is read once.
  - The bare-blot mask is computed before the layers. The height push between the layers is clamped to `HEIGHT_PUSH` (0.5), so a layer that cannot show is not read. Grass alone covers most of the open pampa.
  - The tufts fbm runs only on cleared ground.
  - The hummocks use analytic gradient noise (`noise_d`), which needs 2 noise evaluations instead of 10.
  - The normal maps and hummocks fade out between 30 and 45 m from the eye (`DETAIL_NEAR`/`DETAIL_FAR`, `view.world_position`). No ring was visible in the shots.
  - Frame cost on the Renoir: borderless fullscreen 1440×900, the same tour, temporary diagnostics (reverted). Optimised: median 11.5 ms in the pampa and 12.8 to 13.7 ms at the lagoon. Layers off: 9.7 and 10.1 to 10.7 ms. So the ground now costs about +2 to +3 ms, against about +10 ms before (L6, windowed 1428×834).
  - Shots look the same as before (pampa near, lagoon south).
  - Checks: render suite 602/604. The two failures (`rider::…half_seat`, `tack::…a_tied_horse_keeps_its_tack_and_reins_clear_through_every_idle`) pass alone and are flaky. Clippy is clean. No RED test: WGSL has no runner here; the evidence is the A/B and the shots.
  - Still open from L6: the lagoon's 3 m mud band is invisible from the bank. That belongs to T3/T6 (mud), not here.
- L8 (2026-11-04) user: "arrrrrque" (go with T2). T2 in progress, inline:
  - core: `FloraSite::grassland(p)`, the tufts' open-ground test now shared through `grass_grows`. Test: every tuft is on grassland; not on the track, in town or at the world's edge. Proven by sabotage.
  - Wind: `GrassWind` (`assets/shaders/grass_wind.wesl`) is the standard vertex shader with the tip leaned downwind. Gusts roll at 6 m/s, under a slow swell and a small flutter. The pampero heading comes from `WIND`.
    - The second UV (`SWAY`) carries the sway weight and the height.
    - `wind.w` makes the near grass sink to the ground over its last 8 m, with no dithered edge.
    - The tuft cards now use `GrassMaterial`; flora parts carry a `Paint` (standard or grass).
    - The sway attribute test was proven by sabotage.
  - Near grass (`crates/render/src/meadow.rs`, `MeadowPlugin`): 4 m cells within 22 m of the camera, nearest first, 6 per frame.
    - Each cell holds merged meshes: gramilla (2 crossed cards, 6 to 13 cm, 3/m², the ground's paint colour) and pasto (3-card tufts, 22 to 48 cm, 0.45/m², dryness tint).
    - Placement: on `mesh_height`, with the ground normal, only on `grassland`, thinned under the monte. No shadows.
    - 3 tests (dense and standing on the drawn ground with the ground normal; none off the grassland; cells nearest first and covering the radius), each proven by sabotage.
  - Frame cost on the Renoir (fullscreen 1440×900, against the L7 optimised ground): pampa near +1.7 ms, pampa mid +1.1 ms, lagoon bank (open, densest) +3.1 ms.
  - Visual check over 6 rounds of shots: the paja near the camera and the wind read well, and the end is invisible.
    - The gramilla does not blend yet: brown dots on the pale lagoon bank, blue-grey sprigs on the green pampa.
    - Near grass also grows on the shader's bare soil blots, which the CPU does not know about.
  - Checks: workspace suite green except the known flaky rider tests (`at_the_gallop…half_seat`, `over_a_jump…`), which pass alone. Clippy is clean.
- L9 (2026-11-04) user: "que recomendáns?" then "bueno mandale". Decision: drop the gramilla cards and keep the near paja tufts with wind. Reason: colour-matching cards to the shader-made ground cannot hold everywhere, and the cost is too high for what they add. If gramilla returns, it should take its colour from the ground shader itself.
  - `meadow.rs` now has pasto only: `cell_grass` returns `Option<Mesh>`, `DENSITY` is 0.45/m², `HEIGHT` is 22 to 48 cm, and each tuft has 3 cards with the ground normal. `terrain::ground_color` is private again.
  - Tests: an open cell grows tufts on the drawn ground, lit like it; none grow off the grassland; cells come nearest first. Each was re-proven by sabotage (floating base, no grassland check, vertical normal).
  - Frame cost on the Renoir (fullscreen 1440×900, against L7): pampa near +0.5 ms, pampa mid ±0, lagoon bank about +1 ms.
  - Shots: lagoon bank and low pampa read clean, with no dots and no visible edge.
  - Checks: workspace suite fully green (core, content, game, render 607), clippy clean. README updated (ground layers, near grass, wind).
  - Open: near tufts can stand on the shader's bare soil blots, which the CPU does not know about.
  - Next: T3 (caminos).
- L10 (2026-11-04) user: "si" (go with T3). The user committed T1+T2 as c5544d3. Native review of that range failed with `lens_context_budget_exceeded` (30 files, 1954 lines from 9ef9d03); STATUS keeps offering that same candidate, and the user owns the next step (split the commits, or disable review for the clone). T3, inline:
  - `RoadExt`/`RoadMaterial` (`ground.rs`) extend the road's StandardMaterial with `assets/shaders/road.wesl` and the ground's dry_mud_field_001 KTX2 maps. `road_material` now takes the asset server. `GroundMaterialPlugin` also registers `RoadMaterial`.
  - `surface_mesh` adds UV_1 = (signed distance across, positive to the road's normal; distance along the road).
  - Shader:
    - two wandering cart lanes of wheel ruts (gauge 1.56 m), deeper in some stretches, compacted and tilting the normal through Mikkelsen's surface gradient of `across`;
    - churned clods on the hoof path;
    - a crown and crown or verge grass;
    - cracked mud in low patches and puddled ruts;
    - a wandering alpha edge;
    - fine detail fading at 30 to 50 m.
  - Tests:
    - the surface's UV_1 matches the distance and side of the axis; proven by sabotage (flipped sign);
    - with a server the road loads the mud layer; proven by sabotage (wrong layer);
    - without files it draws without mud.
  - Shots on the road at (-3053,-2197) and (-1336,-1499): the ruts and the edge read well, and so does the mud grain.
  - Open: the crown grass is subtle; a hard dark band at the road's foot in the side view looks like the cut's shading and is probably pre-existing (not compared against the base).
  - Frame cost not measured: the strip is a small part of the screen.
  - Checks: workspace suite green, clippy clean, README updated.
- L11 (2026-11-04) The user split c5544d3 into 80b2489 (T1 ground) and c4ec058 (T2 grass). T3 got a native review: lenses readability and reliability, granted by the user, approved, acknowledged (lineage review-b1c3172fc5b52182). The review raised 6 advisory findings. The user said "si corrija ahora", so these were fixed:
  - `pow(x, 2.0)` with a negative base, undefined in WGSL (possible NaN on other GPUs), is now `bell(x)` = exp(-x*x).
  - The shader's wandering edge alpha is now multiplied by the mesh's feather instead of replacing it.
  - The half width is a uniform (`RoadExt::shape.x` = `RoadNetwork::HALF_WIDTH`, test proven by sabotage) instead of a hard-coded 3.25.
  - `LAYERS[1]` is now the named `SOIL_LAYER`.
  - Not changed: `Assets<RoadMaterial>` is still created with `init_resource` in RoadPlugin, the same pattern as GroundMaterial in TerrainPlugin.
  - Checks: suite green, clippy clean, road shots unchanged.
- L12 (2026-11-04) user: "bueno segumos T4" (go on with T4). Before that, the native review per commit used detached worktrees: 80b2489 and c4ec058 were approved and acknowledged; their findings are advisory and left for a later polish task. T4, inline:
  - `crates/render/src/street.rs` (`StreetWearPlugin`, added by TownPlugin): on each new `TownRoot`, every street gets an opaque `RoadMaterial` quad 3 mm (E-W) or 6 mm (N-S) above the 3 cm earth box, so crossings never z-fight.
    - UV_1 = (across from the axis, along).
    - The half width is the carriageway's: the street minus two `Sidewalk::WIDTH` 3.5 m walks, at least 1.5 m. Before this fix the lanes fell under the sidewalks.
    - One material per half width, cached.
  - `road.wesl` lanes now sit at 0.477 × the half width (1.55 m on the road, unchanged).
  - Tests, each proven by sabotage:
    - the quad covers the street, sits on the levelled height, and measures across from the axis with the carriageway half width (sabotage: no sidewalk subtraction);
    - it faces up (this caught a CW winding bug);
    - every street of the active towns is worn once, with one material per width (sabotages: no `Added` filter, no cache).
  - Shots in Monte Nievas and General Pico: the wheel ruts run in the carriageway between the curbs.
  - Also asked by the user: test windows open on Hyprland workspace 5. The game sets app_id `pampa-shots` when PAMPA_SHOTS is set (`shots::APP_ID`), and `~/.config/hypr/windows.lua` has `w("^pampa-shots$", { workspace = "5 silent", render_unfocused = true })`. Without `render_unfocused`, half the captures came out black.
