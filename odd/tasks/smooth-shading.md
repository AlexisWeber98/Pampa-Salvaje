# Smooth shading pass

## Objective (user, 2026-10-24)

"Ya el low-poly no va más": move the look from faceted low-poly to smooth,
more realistic shading, within the budget of the Renoir iGPU. This first
graphics batch costs no GPU and needs no download.

## Evidence (read-only audit, 2026-10-24)

- Vegetation: the Quaternius Ultimate Nature OBJs carry one normal per face
  (`s off`). `flora.rs` passes them straight through (`flora.rs:582-588`),
  and the OBJ loader does no welding (`obj.rs:140`).
- Terrain: the chunk mesh is a non-indexed triangle list "so normals come
  out flat: the low-poly look" (`crates/render/src/terrain.rs:454`).
- Procedural people: skin and limbs are smooth, but the clothing has flat
  normals (`character/figure.rs:323, 493, 600`). The ridden horse has flat
  normals too (`character/horse.rs:449, 509, 596`).
- Shadows: three cascades with the first ending at 8 m, out to 100 m, on
  the default 2048 shadow map (`atmosphere.rs:159-171`).

## Tasks

- [x] T1 Smooth normals on imported OBJ vegetation: weld by position and
  average with an angle threshold, so trunks, crowns, bushes and grass read
  soft while hard creases stay. Proof: `flora::tests::baked_vegetation_has_smooth_normals_with_creases_kept`
  (RED observed: bark blend share 0.098 with flat normals; GREEN after).
- [x] T2 Smooth terrain normals, from the height field or an indexed grid.
  The ground textures, world UVs and tangents, the roads laid on the ground
  and the async chunk build must all stay correct. Proof:
  `terrain::tests::ground_normals_are_smooth_and_meet_exactly_across_chunk_borders`,
  `terrain::tests::ground_normals_follow_the_slope_of_the_height_field`
  (both RED against a stub, GREEN after), plus the existing UV, colour,
  async-build and border-height tests.
- [~] T3 DROPPED (user, 2026-10-24): the procedural figures and the ridden
  horse will be replaced by 3D models (MakeHuman people, the Quaternius
  horse) in the next PR, so smoothing them now would be thrown away.
  `character/figure.rs` and `character/horse.rs` were never touched.
- [x] T4 Sharper near shadows: tune the cascades (first split about
  10–12 m) and the biases; keep the map size unless the cost is justified.
  Proof: `atmosphere::tests::sun_shadows_are_crisp_near_the_rider` (RED: the
  new constants did not exist; GREEN after).
- [x] T5 Tests, docs and the "low-poly" wording in module docs. Proof:
  `smooth::tests::*` (5 tests, RED with an empty stub, GREEN after); module
  docs of `terrain`, `flora`, `ground`, `atmosphere`; README graphics notes.

Route: one delegated writer, for 2+ non-trivial files.

## Decisions

- Shared helper `crates/render/src/smooth.rs` (`smooth_normals`), a port of
  `tools/glbtrim.py`: weld by exact position for the normals only, join the
  faces around each welded point through shared edges bending less than the
  threshold (union-find), average the group's face normals by corner angle.
  One normal per input vertex; deterministic; zero-area faces fall back.
- Vegetation thresholds per role (`flora::smoothing`): caldén crowns,
  thickets and weeds 60°; snags and dead wood 50°; tosca stones 40°; grass
  80° plus normals leaned 50% toward +Y (thin cards light like the ground).
  Smoothing runs on the stood-up, widened shape; the few faces wound against
  the file's normal keep the file's normal. Vertex count, colours and baked
  AO unchanged; meshes are still built once and shared.
- Terrain: the chunk mesh is now an indexed grid, `(res+1)²` vertices
  instead of `6·res²`, same triangle split, so `mesh_height`, roads and
  platforms are unchanged. Normals are analytic: central differences of
  `Terrain::height` over one quad, on a lattice numbered across the whole
  world (`n · quad` meters), so shared border corners get bit-identical
  normals. Tangents follow world U on the slope (`slope_frame`), matching
  `face_tangent`'s convention. Cost: one extra height sample per corner, in
  the async build.
- Shadows: `FIRST_CASCADE` 12 m (was 8; the orbit camera sits 9.5 m back),
  `CASCADE_OVERLAP` 0.15 (Bevy 0.2), bounds about 12/35/100 m, map 2048,
  biases at Bevy's defaults (0.02, 1.8).

## Checks

- `cargo test -p pampa-core`, `-p pampa-content`, `-p pampa-render`, `-p game`
- `cargo build -p game`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`

## Progress

- 2026-10-24: branch `feat/smooth-shading` from main 9d040e7.
- 2026-10-24: T3 dropped by the user (figures and horse go to 3D models
  next PR). T1, T2, T4, T5 implemented, uncommitted for the user's review.

## Verification (2026-10-24)

- `cargo test -p pampa-core`: 350 passed
- `cargo test -p pampa-content`: 32 passed
- `cargo test -p pampa-render`: 247 passed
- `cargo test -p game`: 117 passed
- `cargo build -p game`: ok
- `cargo fmt --all -- --check`: clean
- `cargo clippy --workspace --all-targets -- -D warnings`: clean
- Not verified: in-game look (no visual run from the writer).

## Next step

Look in game: trees and bushes (`goto pico`, `goto ranch`), the ground at a
low sun (`time 18`), the rider's shadow at the default camera distance,
acne or floating shadows at the hooves. Follow-ups out of this scope: the
settlement tree crowns (`settlement/crown.rs`) and the procedural stand-ins
are still flat-shaded; native review of the work-unit commit.
