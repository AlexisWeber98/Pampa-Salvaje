# Fauna, first pass

## Objective (user, 2026-10-23)

Bring the pampa to life with animals: cattle grazing inside the fenced
fields, a few loose horses in the estancias' and chacras' corrals, hares in
the open grass, teros that take off when approached, chimangos soaring
overhead and ducks on Laguna La Arocena. Simulation in core (pure,
deterministic, tested), rigid-part drawing in render, streamed with the
chunks around the player under hard caps, without hurting the Renoir iGPU.

## Scope

- Allowed edit surfaces: `crates/core/src/**`, `crates/render/src/**`,
  `game/src/**`, `README.md`, `odd/setting-1919.md`, this file.
- Out of scope: skeletal animation (bevy_animation is not available),
  sound, cattle drives, birds landing on fences or water.

## Tasks

- [x] T1 core `fauna` module: `FaunaSite` seeding (herds per fenced field,
  corral horses per settlement, hares, tero flocks, raptors, duck rafts per
  chunk or lagoon), `Herd`/`Cow`, `Hare`, `Flock`, `Raptor`, `Duck`
  simulation, caps. Route: delegated direct (single bounded writer).
- [x] T2 render `fauna` module: `FaunaKit` (shared unit meshes, a few shared
  materials), rigid-part animals, animation, shared `VisibilityRange`s;
  `CharacterVisual::Bare` for untacked corral horses.
- [x] T3 game `fauna` plugin: streaming by chunk around the player, caps,
  flee threat from the player, cattle as soft bodies for the rider, corral
  horses as `Mountable` obstacles, `fauna` console command.
- [x] T4 docs: README note, this file.

## Decisions

- **Fields**: a herd lives in a *pasture*, an axis-aligned box inside one
  fenced survey cell (never a descampado), [`FaunaSite::FENCE_MARGIN`] off
  its fences, sampled on a grid of [`FaunaSite::SAMPLE`] m and accepted only
  when every sample is clear of roads (roadside fences included), towns,
  lagoons, the railway and settlements. A pasture is convex, so a cow
  wandering anywhere inside it never crosses a road or a fence.
- **Density**: [`FaunaSite::HERD_CHANCE`] of the fenced fields hold a herd
  of 6–20 head. Coats: Hereford (red, white face), Aberdeen Angus (black),
  overo (white with black patches), mostly one breed per herd.
- **Behaviour**: cows graze (head down), stand, walk slowly to a spot near
  the herd; a rider closer than `Herd::SHY` walks them off. (They no
  longer lie down: see `odd/tasks/animated-fauna.md`.) They
  keep apart, are pushed out of static obstacles (trees) and clamped to the
  pasture. They are soft bodies the rider is blocked by.
- **Horses**: 1–3 untacked horses stand in the corrals of chacras and
  estancias (a puesto's corral sometimes holds one). They are `Mountable`
  (the abilities rule: once Ride is unlocked any horse is), drawn with the
  existing criollo horse model without tack. A horse taken is not respawned
  for the session.
- **Hares** sit and hop; within `Hare::ALARM` of the rider they bolt away in
  a zigzag and calm down once `Hare::SAFE` away.
- **Teros** peck on the ground; within `Flock::ALARM` they take off on an arc
  away from the rider and land 40–70 m on. **Chimangos** circle slowly 30–55
  m up. **Ducks** paddle on the lagoon's water and paddle away from riders.
- **Caps** (`fauna::caps`): cows 160, horses 16, hares 16, teros 36,
  raptors 4, ducks 32, nearest first.
- **Rendering**: one shared unit mesh set and a handful of shared
  materials; triangle budgets per species are tested. Animation (legs swing,
  head bob, wings flap) is skipped past `ANIMATE_FAR`; the meshes use the
  settlements' existing shared ranges (ground animals the props' 220 m,
  soaring birds the buildings' 400 m): no new `VisibilityRange`.

## Review and playtest fixes (round 2)

- **Streaming re-plans nearest first.** Each time the player enters
  another chunk, the plan is made again: the chunks within `RADIUS` are
  taken nearest first, and each gets the animals that still fit under the
  caps (`FaunaCaps`). A chunk is drawn again only when its picks change.
  Picks are the herd ids, the horse footprints and each other animal's
  place in the seeded list; per-kind totals are not enough. So a chunk
  first loaded short at the edge fills up as the player nears it.
- **Hares and teros stay on open grass at home.** A hare only moves onto
  ground `FaunaSite::grazeable` allows inside its home chunk. Otherwise it
  swerves (wider turns, then back), or crouches if cornered. Teros land
  only on such ground. If no spot in 8 tries will do, they circle back to
  where they rose.
- **Cows and edge trees.** A cow is clamped into its pasture, then pushed
  out of obstacles, over a few passes. If both cannot hold, it stays where
  it was.
- **Console showed an empty panel** (the cause was older than fauna).
  `run_entered` mutated `Console` every frame, so the panel was relettered
  every frame. Its lines were despawned before the UI could lay them out.
  It now touches the console only when there are lines to run. A headless
  test checks that the prompt and `help` lines are lettered and that the
  lines stay put while idle.
- **Cow looks.** The barrel and neck are now smooth capsules (`Shape::Barrel`).
  The chest is deeper and the legs shorter (0.78 m). There is a dewlap and
  an udder (a new `Udder` paint, 12 in all). A lying cow rested its belly
  on the ground (the lying state was removed later, with these rigid
  cows, in `odd/tasks/animated-fauna.md`). Triangles: Hereford 824, Angus 724,
  overo 904, within the budget of 1,000.

## Follow-ups

- Skeletal (skinned) versions of the animals: done for the cattle and
  the corral horses in `odd/tasks/animated-fauna.md` (the rigid cows and
  the untacked `CharacterVisual::Bare` horse are gone).
- Horses grazing loose in the paddocks round the homesteads (they stand in
  the corrals only).
- Calves, teros landing on fence posts, flamencos (the bull per herd is
  done in `odd/tasks/animated-fauna.md`).

## Progress

T1–T4 implemented and checked: core, render and game tests, build, fmt and clippy pass. Tests were written alongside the code, not observed RED first. Visuals have not been checked in game yet.
