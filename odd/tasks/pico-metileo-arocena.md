# General Pico through its line, Metileo, Laguna La Arocena

## Objective (user, 2026-10-23)

1. General Pico: the railway passes through the town centre and the town
   grows on both sides of the tracks.
2. Metileo: a railway halt on the main line between Monte Nievas and General
   Pico, where the train stops; station, platform, store and a few houses.
3. Laguna La Arocena: a shallow plains lagoon east-south-east of Pico, with
   water, reeds, a basin, keep-outs, no wading into deep water, and the
   minimap.
4. Playtest follow-up (same PR): civic buildings spread over every town (no
   cluster round one corner); General Pico has a store and a comisaría on
   each side of its line; small towns' store is a "Pulpería".

## Decisions

- **Pico, option B** (user, final, after A and C): the grid stays square to
  the map and the line runs east-west through its middle, along a grid axis.
  The real Pico's grid is turned 45° with the Ferrocarril Oeste parallel to
  its odd-numbered streets (OpenStreetMap, verified by the coordinator), so
  this keeps the real topology without the whole-town rotation. Option A
  (rotating the town) was estimated at 2–3k lines over ~25 files because
  every town consumer is axis-aligned; option C rested on a wrong premise.
- `RailwayConfig::through`: the grid splits between its two middle rows; the
  band between the halves' ring streets is `offset + 16` m wide; the station
  yard stands in the station's half of the band (`side`), centred on the
  town. North-south streets clear of the yard cross at level crossings,
  modelled as `RoadKind::Street` roads so the deck, ramps, signs and render
  are reused. No rural track for a through town; inter-town roads never leave
  along the line's axis.
- `plaza: Auto` on a through town: one column west of the middle, two rows
  back from the track in the station's half (Pico: north-west of the
  station, a few blocks off).
- **Metileo** at its real place (station -35.774, -63.941; Pico's plaza
  -35.659, -63.758; coordinator, from OpenStreetMap/es.wikipedia): 16.63 km
  west and 12.56 km south of Pico, `(288, -544)` at 1:7. 3 x 3 blocks of 30 m,
  8 m streets, no comisaría, `density: Some(0.3)` (new `TownConfig::density`,
  the share of lots built). The train stops there because every line town is
  a station.
- **Laguna La Arocena** at `(3401, 841)`, 215 x 145 m: a new
  `pampa_core::Lagoon` in `Terrain::lagoons` (seeded star-shaped shore, water
  0.6 m under the lowest bank, 1.6 m deep, 30 m bank, 5 m of wadeable
  shallows, `World::resolve` pushes bodies out of deeper water). Fences,
  settlements, flora and terrain props keep out of its keep-out box (water,
  bank and 15 m); `place_lagoons` rejects lagoons on towns, rails, roads or
  off the world. Render: one blended water fan with a code-painted 64 px
  normal map drifted by `uv_transform` (no file, no SSR), one reed mesh per
  lagoon (≤ 6000 triangles) on the settlements' shared props
  `VisibilityRange`, two shared materials; the minimap paints it.

## Tasks

- [x] T1 Laguna La Arocena: core lagoon, terrain basin, collision,
  keep-outs, content/game wiring, render water and reeds, minimap. Route:
  delegated writer (this worker).
- [x] T2 Metileo: towns.ron entry, `density`, line order, timetable stop.
  Route: delegated writer.
- [x] T3 General Pico through its line: `RailwayConfig::through`, split grid,
  street crossings, plaza, roads. Route: delegated writer.
- [x] T4 Approved 1:7 review follow-ups: afternoon-wait test derived from
  `arrival_time_at` with a tolerance; road-cut test keeps the depth bound by
  the crossings; determinism of plain `SettlementLayout::build`.
- [x] T5 Playtest follow-up: dispersed civic buildings, a store and a
  comisaría on each side of Pico's line, "Pulpería" in small towns. Route:
  delegated writer.
- [x] T6 Lagoon playtest (screenshot at 18:26): water read as sand dunes,
  a huge bare mud bank, no visible reeds, an oval shore. Route: delegated
  writer.
- [x] T7 A public road from General Pico to Laguna La Arocena. Route:
  delegated writer.

- **Civic dispersal** (playtest, 2026-10-23): the church still faces the
  plaza; a bigger town's station plaza and its store come next; every other
  store and police station is dispersed (`disperse` in `core::town`): at
  least 2 blocks (Chebyshev) from every civic building, the same kind at
  least half the longer side apart, nearest the plaza within that; in a
  through town each goes to the half with fewer of its kind. The very big
  tier has 2 stores (was `n / 16`, 4 in Pico). Small-town layout
  fingerprints were re-pinned on purpose.
- **Pulpería**: `StoreStyle::of(size)`; small towns' store sign reads
  "PULPERÍA", big and very big "ALMACÉN DE RAMOS GENERALES". The minimap
  letter stays "A" (kind-based) so "P" stays the plaza's. The errand target
  is unchanged (Castex's only comisaría; checked by the errand tests).

- **Lagoon look (T6)**: the ripple map is a 2.5 m tile of six fine layers
  (slope at most 0.08) with its own mip chain, drifting slowly. The water
  is white-based and painted by vertex colours: dark olive-brown and nearly
  opaque in the middle, see-through over the mud at the rim. It has
  roughness 0.06, reflectance 0.35 and is not metallic. The bare mud is
  only `Lagoon::MUD_BAND` (3 m) up the bank: grass, stones and shrubs grow
  right past it, and trees stay 12 m from the water. The reeds were never
  drawn: their entity stood at the world origin, so the shared 220 m range
  culled them from 3.5 km away; water and reeds now stand at the lagoon's
  middle, and reeds are 1.5–2.5 m tall, in patches. The shore is a
  superellipse (exponent 5, a rounded rectangle) roughened by up to 5%,
  axis-aligned (the real orientation is unknown here).
- **Lagoon road (T7)**: `RoadKind::Lagoon`, laid by
  `RoadNetwork::with_lagoons` after `place_lagoons` (which still rejects any
  other road on a bank), from the nearest town within 2.5 km. It leaves by
  the side facing the lagoon on the street nearest it that keeps 40 m from
  the track (Pico: east), curves out like a road between towns, and ends in
  a 6 m turnaround loop whose edge stops at the mud. A signpost at its start
  names the lagoon. Fences, settlements and flora treat it like any road.

## Timetable (base rate)

| Run | Before | After |
|-----|--------|-------|
| Up | Pico 07:00, MN 10:14:45–10:27:15, Castex 11:46:07 | Pico 07:00, Metileo 08:51:38–09:04:08, MN 10:53:40–11:06:10, Castex 12:25:02 |
| Down | Castex 16:00, MN 17:17:49–17:30:19, Pico 20:46:07 | Castex 16:00, MN 17:17:49–17:30:19, Metileo 19:19:51–19:32:21, Pico 21:25:02 |
| Run | 572.2 s over 9 712 m | 650.1 s over 10 014 m |

Opening: aboard from 10:43:40 (was 10:04:45), in at Monte Nievas 10:53:40,
off at 10:54:04; the 50 real second lead is unchanged.

## Startup counts (shipped world)

| | 1:7 shifted (before) | T1–T5 | with the lagoon road (T7) |
|-|-----|-----|-----|
| Fence spans | 130 226 | 126 016 | 127 095 |
| Ranchos / puestos / chacras / estancias | 3 / 3 / 5 / 2 | 3 / 2 / 7 / 3 | 4 / 2 / 6 / 3 |
| Settlement obstacles | 255 | 318 | 298 |
| Warnings | none | none | none |

## Checks

- RED first: the shipped-file tests in content and game failed on the new
  towns.ron (four towns, the Metileo stop, Pico's crossings) before they were
  updated; the opening tests failed on the moved timetable before being
  derived from it. The lagoon, through-line and density tests were written
  with their code (no meaningful RED for brand-new types).
- Final run (2026-10-23): pampa-core 307 passed, pampa-content 29,
  pampa-render 216, game 92; `cargo build -p game` ok; `cargo fmt --all --
  --check` clean; `cargo clippy --workspace --all-targets -- -D warnings`
  clean. Visuals not checked in a window.

## Commit split (cumulative patches for the native review)

Built by the delegated writer under the session scratchpad
(`map-split/final/1-lagoon-core.patch` … `6-lagoon-road.patch`), each
applying on the previous ones from the branch point and passing the
crates' tests; applied in order they reproduce the working tree exactly.

1. `feat(world): sink Laguna La Arocena into the pampa` (lagoon core,
   terrain, collision, keep-outs, content and game wiring).
2. `feat(render): draw Laguna La Arocena's water, reeds and mud` (render
   lagoon, ground colour, props, minimap, T6 look).
3. `feat(world): add the Metileo halt to the line` (`density`, towns.ron,
   timetable-derived tests, the 1:7 review follow-ups).
4. `feat(town): run General Pico's line through its middle`.
5. `feat(town): spread civic buildings and name small-town stores
   pulperías` (and this feature document).
6. `feat(road): lay a public road from General Pico to Laguna La Arocena`.

## Next step

Native review of the work-unit commits once the user commits them.
