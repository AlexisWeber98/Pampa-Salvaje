# Period wardrobe for the MakeHuman characters

## Objective (user, 2026-10-23)

Step 2 of `odd/tasks/makehuman-characters.md`. Dress the MakeHuman characters
(protagonist, Bairoletto, Farach, Hidalgo) for La Pampa in 1919 and make them
the default instead of the rigid figures. The user finds the procedural
figures and the current models too basic ("medio básico").

## Problem

- The CC0 base wardrobe reads modern: jeans, a field jacket and a fedora.
- The arms are still half raised, and the hats render like metal pots.
- Hidalgo does not read as about 60, and Bairoletto is not slight enough.
- Some 1919 pieces have no pack asset: the gacho and chambergo brims, the
  neckerchief (pañuelo), the faja and rastra, and the kepi with a visor.

## Scope

- Choose garments from the 10 packs installed on 2026-10-24 (licences in the
  session scratchpad `mh-packs/assets-licences.tsv`; installed under
  `~/Documents/makehuman/v1py3/data/`). Candidates:
  - `toigo_harem_pants` for the bombacha;
  - `toigo_wool_pants`;
  - `punkduck_riding_boots`, `maciekg_leather_boots`, `culturalibre_male_boots`;
  - `elvs_male_shirt_untucked_bd1`;
  - `toigo_male_suit_3` or `toigo_male_double-breasted_suit` for Hidalgo;
  - `mindfront_patrol_cap` as the base of the kepi;
  - `culturalibre_cl_don_quixote_hat`, `culturalibre_cl_bowler_hat`,
    `grinsegold_uncle_joshis_hat`;
  - beards and moustaches from `bodyparts05` and `bodyparts06`.
- Procedural pieces made by the converter, rigidly skinned to one
  `game_engine` joint each:
  - the neckerchief;
  - the faja and rastra;
  - the kepi visor and the tunic's brass buttons;
  - hat brims where a pack hat is missing or wrong.
- Fix the arm rest pose, and the hat material (no metallic look).
- Stronger macro differences between the characters.
- Make the MakeHuman NPCs the default. `modelos off` keeps the old figures
  in debug builds.
- Credit every CC-BY asset: its author, in `CREDITS.md`,
  `assets/characters/CREDITS.md` and next to the files.

## Constraints

- No network. The packs are already installed.
- Never launch the MakeHuman GUI.
- Keep the triangle budget: about 14k triangles per character today, and
  avoid the 15–17k-triangle suits.
- The user commits. Leave the changes uncommitted.

## Tasks

- [x] T1 Converter: pack garment selection, procedural pieces, arm pose,
  hat material, macros; `.mhm`/`.mhmat` updates; reconvert the four
  characters; tests in `tools/test_mh2gltf.py`. Route: delegated writer
  (2+ non-trivial files). Added mid-task by the user: QEM decimation of
  heavy pack garments (the harem pants kept as the bombacha).
- [x] T2 Game: MakeHuman NPCs by default, `modelos off` for the old figures;
  game tests. Route: same writer. The game tests are blocked by unrelated
  console renames (see Verification).
- [x] T3 Credits and READMEs. Route: same writer.

## Acceptance and checks

- `python3 -I -B -m unittest discover -s tools`
- `python3 -I -B tools/mh2gltf.py --check assets/characters/<name>/model.gltf`
  for each character
- `cargo test -p pampa-render`, `-p game`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`

## Decisions

- **Where the packs are.** `~/Documents/makehuman/v1py3/data` turned out
  empty. The 10 packs sit in the system data folder as `.mhpxy` + `.obj`,
  without the compiled `.npz` mesh. The converter now reads `.obj` meshes
  and `.mhclo` text proxies. It also searches the user data folder first,
  as MakeHuman does (`--user-data`, `$MAKEHUMAN_USER_DATA`,
  `--no-user-data`).
- **Decimation.** QEM half-edge collapse, numpy and heapq only. It runs
  after fitting and skinning, with boundaries and UV seams locked. Weights
  are merged and cut to the top 4, and the output is deterministic. With
  1,490 seam vertices the harem pants stop at 3,142 triangles (target
  3,000).
- **Converter lines in the `.mhm`** (MakeHuman warns and skips them):
  `zDepth`, `decimate`, `hideUV` (the suits' jeans under the bombacha) and
  `piece`.
- **Generated pieces.** Each is rigid (weight 1) on one joint and fitted
  to a 20-sector outline of the dressed, posed body:
  - `hat` on `head`: gacho, chambergo, the wide brim;
  - `kepi` with its visor, on `head`;
  - `neckerchief` on `neck_01`;
  - `sash` (faja) and `rastra` with its coins, on `pelvis`;
  - `buttons` (brass) on `spine_02`.

  All hats are generated, so no pack hat is used. The patrol cap and
  Don Quixote hat were not needed.
- **Boots.** `punkduck_riding_boots` stay under the trousers: only the feet
  show. Over the baggy bombacha the boot tops poked through, and over the
  suits the boots do not hide the trouser legs.
- **Material fix.** Roughness is now `1 - shininess * min(1, 4 * specular)`.
  The black fedora went from 0.35 to 0.86, and the felt and the kepi are
  0.9. `roughness`/`metallic` lines are honoured. Only the brass and the
  coins are metallic.
- **Arms.** Baked into the rest pose by linear blend skinning: the upper
  arm hangs along (0.2, -1, 0.05), the forearm a little forward. The
  in-game `lower_arm` was removed.
- **White hair.** A `greyscaleTexture` `.mhmat` key stretches a dark
  texture to grey before the tint. Hidalgo gets white hair, grey eyebrows
  and a white moustache.
- **Macros.**
  - Hidalgo: age 0.8, weight 0.7, muscle 0.3, height 0.46. The old skin
    read too dark, so he keeps the middle-aged one.
  - Bairoletto: weight 0.2, muscle 0.4, height 0.42.
  - Farach: weight 0.75, height 0.52.
- **Outfits.**
  - Protagonist: `male_casualsuit05` (brown, jeans hidden), harem bombacha
    grey-brown, boots, gacho, red neckerchief, faja, rastra.
  - Bairoletto: `male_casualsuit02` black (jeans hidden), charcoal
    bombacha, boots, moustache (light brown), chambergo, white
    neckerchief.
  - Farach: double-breasted suit dyed navy (6,500), boots, moustache,
    kepi, two columns of brass buttons.
  - Hidalgo: `toigo_male_suit_3` warmed (7,000), boots, white moustache,
    wide-brimmed hat.

## Results

| Character | Triangles before | after | Model size before | after (used files) |
|---|---|---|---|---|
| protagonist | 13,796 | 12,337 | 4.2 MB | 3.5 MB |
| bairoletto | 13,408 | 11,092 | 4.1 MB | 3.6 MB |
| farach | 13,434 | 11,272 | 3.9 MB | 3.4 MB |
| hidalgo | 13,824 | 11,712 | 3.9 MB | 3.3 MB |

19 textures of the old wardrobe stay in the `textures/` folders, unused:
the fedoras, `shoes01`/`03`/`04`, `male_casualsuit03` and Hidalgo's
`male_casualsuit05`. The writer was not allowed to delete files; they need
a `git rm`.

## Verification (2026-10-23)

- `python3 -I -B -m unittest discover -s tools`: 57 tests OK. RED was
  observed for the `.obj`/`.mhclo`, data-folder, material, greyscale and
  `.mhm` line tests. The decimation, piece and arm-pose tests were written
  with or after their code, so no separate RED.
- `--check` on each of the four models: exit 0.
- Re-converting Bairoletto gives byte-identical output.
- `cargo test -p pampa-render`: 232 passed. RED was observed for
  `the_models_are_shown_by_default_and_off_restores_the_figures`.
- `cargo test -p game`: 108 passed, 3 failed. The failing tests are
  `parses_every_command`, `modelos_turns_the_makehuman_people_on_and_off`
  and `caballo_stands_a_horse_beside_the_player_mountable_once_riding_is_unlocked`.
  The cause is edits to `game/src/console.rs` made outside this task
  (`caballo`→`horse`, `modelos on`→`models on`, `hora`→`time`,
  `laguna`→`lake`). They are left in place, pending the user's decision.
- `cargo fmt --all -- --check`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- Visual check: offline software renders only, front, side and back; not
  checked in game yet.

## Progress

- 2026-10-23: branch `feat/period-wardrobe` created from `feat/fauna`.
- 2026-10-23: T1–T3 done by the writer, uncommitted.

## Round 2 (user, 2026-10-24)

The user checked the models in game. The pack garments still read modern:
- a pinstripe double-breasted suit;
- ties;
- a print on Bairoletto's shirt;
- bucket-like hats and a pale top-hat kepi.

Only Bairoletto's bombacha (harem pants) and his boots pass. The user approved body-derived garments instead of pack clothes. The user also asked that everything no longer used be deleted, with no orphan files.

- [x] T4 Body-derived 1919 garments in the converter. Regions of the fitted, skinned body are inflated outward into each garment:
  - shirt with a small or no collar, and a waistcoat;
  - Farach's high-collared guerrera;
  - Hidalgo's three-piece suit with a stiff collar;
  - trousers tucked into knee-high boots.

  They inherit the body's skin weights. Modelled pieces add the period silhouette:
  - flat-crowned, flat wide-brimmed hats with a band;
  - a real kepi (flat top, visor);
  - the poncho over the shoulder, high collars and a watch chain.

  Materials are simple textures made in code, and there are no ties. Bairoletto's harem bombacha and riding boots stay. Route: delegated writer (2+ non-trivial files, broad preparation reading). Proof: see Round 2 verification.
- [x] T5 Delete everything unused:
  - pack garments and their textures that no character uses;
  - the 19 old textures;
  - converter options and code left with no user, such as `hideUV` if no suit remains;
  - their credits.

  Route: same writer. Done except 12 untracked files: the sandbox denied plain `rm` (see Deleted files).

### Round 2 decisions

- **Garments cut from the body** (`piece shirt|waistcoat|jacket|trousers|bombacha|boots`):
  - Every vertex of the posed `male1591` proxy gets a limb from its strongest joint: trunk, neck and legs; arm; or head, never dressed.
  - It also gets a place on that limb. On the trunk and legs this is a *station*: a height scale through the joints, 0 at the soles, 1 ankles, 2 knees, 3 hips, 4 `spine_02`, 5 shoulders, 6 neck, 7 head. On the arms it is a *reach*: 0 at the shoulder, 1 elbow, 2 wrist.
  - A garment covers the faces whose three vertices fall in its range, pushed out along smooth normals by a per-garment offset profile (`GARMENT_CUTS`).
  - Every vertex keeps its source vertex's weights. Rims go from each open edge down to 0.005 dm above the skin.
  - An overlapped face goes to the garment that sits furthest out. The skin under any garment is left out.
  - Coats get a skirt tube with front opening, flare and pelvis/thigh weights. Optional: a V-neck, padded shoulders, a boot sole and a flared top.
- **Pack clothes left:** only Bairoletto's (harem pants, riding boots) and the moustaches. Garment faces are not dropped under pack clothes. That left a skin hole at Bairoletto's back, so his shirt is cut down to station 2.9 under the harem pants.
- **Rigid pieces** fitted to the dressed body (a ray cast along z finds the frontmost surface):
  - hats with a flat top, an optional crease (`dent`), `round`, `pinch`, a thick brim with a curled lip, and a ribbon;
  - a kepi with a planar top sloping forward and wider than the band, a leather band, a half-disc visor, a chin strap and buttons;
  - a neckerchief with a knot to one side and two splayed ends;
  - a band collar with wing tips, lapels, a strap, a watch chain;
  - a poncho folded over one shoulder, its ends hanging below the hip.
- **Cloth textures:** 64-px textures made in code and the same every run: `cotton`, `wool` (twill), `stripe`, `leather`, all grey for tinting, and `poncho` (dark red with cream bands, in colour).
- **Removed converter code:** `zDepth` (the harem pants' own z_depth 50 already sits over the boots' 45), `hideUV`, pack-hat bands and proxy tags. Their tests and docs went with them.
- **Outfits:**
  - Protagonist: cream shirt, dark waistcoat, bombacha loose to the calf in calf-high brown boots, gacho, red neckerchief, faja, rastra, red and cream poncho over the left shoulder.
  - Bairoletto: dark shirt, brown waistcoat, harem bombacha and riding boots (packs), chambergo, white knotted neckerchief.
  - Farach: navy guerrera with a high collar and skirt, navy trousers in black knee boots, black belt with a brass buckle, shoulder strap, two columns of brass buttons, kepi.
  - Hidalgo: white shirt with a band collar and wing tips, black waistcoat with a gold watch chain, open hip-length jacket with lapels and padded shoulders, trousers in brown boots, felt hat with the widest brim and a tall creased crown.
- **Render test floor:** `every_character_model_loads_skinned_within_the_triangle_budget` now accepts 3,000–15,000 triangles. It was 6,000; the lightest model now has about 4,900.

### Round 2 results

| Character | Triangles (round 1 → 2) | Model size (gltf + bin + used textures) |
|---|---|---|
| protagonist | 12,337 → 5,838 | 1.9 MB |
| bairoletto | 11,092 → 9,754 | 2.6 MB |
| farach | 11,272 → 4,900 | 1.9 MB |
| hidalgo | 11,712 → 4,902 | 1.9 MB |

Offline previews (front, side, back; software renderer in the session scratchpad) were reviewed by the coordinator and the user, and one round of feedback was applied: distinct hats, poncho, bombacha profile, Bairoletto's knot, Hidalgo's lapels and collar.

### Deleted files

With `git rm` (26):
- `bairoletto/`: `black_shirt.mhmat`, `textures/fedora_normal.png`, `fedora_texture.png`, `male_casualsuit02_diffuse.png`, `male_casualsuit02_normal.png`, `shoes04_diffuse.png`, `shoes04_normal.png`;
- `farach/`: `tunic.mhmat`, `textures/fedora_normal.png`, `fedora_texture_black.png`, `male_casualsuit03_diffuse.png`, `male_casualsuit03_normal.png`, `shoes03_diffuse.png`;
- `hidalgo/`: `textures/fedora_normal.png`, `fedora_texture_grey.png`, `male_casualsuit05_diffuse.png`, `male_casualsuit05_normal.png`, `shoes04_diffuse.png`, `shoes04_normal.png`;
- `protagonist/`: `jacket.mhmat`, `textures/fedora_normal.png`, `fedora_texture.png`, `male_casualsuit05_diffuse.png`, `male_casualsuit05_normal.png`, `shoes01_diffuse.png`, `shoes01_normal.png`.

Still to delete (untracked; the sandbox denied `rm`), all under `assets/characters/`:
- `protagonist/bombacha.mhmat`, `protagonist/textures/riding_boots.png`, `protagonist/textures/riding_boots_norm.png`;
- `farach/textures/DB-suit.png`, `DB-suit-norm.png`, `riding_boots.png`, `riding_boots_norm.png`;
- `hidalgo/suit.mhmat`, `hidalgo/textures/NewSuit3.png`, `NewSuit3-norm.png`, `riding_boots.png`, `riding_boots_norm.png`.

Credits: the `suits01` row, the `male_casualsuit02`/`05` mentions and the old substitutions table are gone. `toigo_harem_pants` and `punkduck_riding_boots` now credit Bairoletto only.

### Round 2 verification (2026-10-24)

- `python3 -I -B -m unittest discover -s tools`: 68 tests OK.
  - The garment, hat, kepi, cloth, poncho and skirt tests were written with or after their code, so no separate RED was observed for them.
  - The kepi's planar top failed first (genuine RED), and the code was fixed after.
- `--check` on each of the four models: exit 0.
- Reconverting Hidalgo gives byte-identical `model.gltf` and `model.bin`.
- `cargo test -p pampa-render`: 232 passed, after lowering the triangle floor (its first run failed on the 6,000 floor).
- `cargo test -p game`: 111 passed, 0 failed (the console renames are now in).
- `cargo fmt --all -- --check`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `rg` check: every file in `textures/` is named by its `model.gltf`, except the 12 untracked files above, and no doc names a deleted asset.

### Round 2 follow-up (user, in game, 2026-10-24)

The user checked the round-2 models in game. Hidalgo's hat was too big, with a crown like a top hat; Bairoletto's was a bit big. The colours looked washed out: Hidalgo's dark suit showed as light taupe, Bairoletto's dark clothes as mid grey, and Farach's navy as grey-blue.

- **Cause of the pale colours.**
  - glTF's `baseColorFactor` is linear, but the converter wrote the `.mhm`/`.mhmat` display (sRGB) colours into it raw. Hidalgo's jacket, colour 0.21, showed as about 0.50.
  - The generated textures were not the cause: they are near-white sRGB, the colour is applied once, roughness is 0.85–0.92, and there are no normal maps.
  - The offline previews hid it, because the renderer treated the factor as a display colour.
- **Fix.**
  - `mh2gltf.linear()` converts every material colour to linear when it is written. This covers piece colours and `diffuseColor`; white is unchanged.
  - Tests: `test_colours_are_written_linear_as_gltf_wants` and Conversion `test_the_uniform_colour_is_written_linear`. RED was observed: both errored before `linear()` existed.
  - The preview renderer now works like Bevy: sRGB textures are decoded, the factor is linear, and the result is encoded back to sRGB.
- **Hats.**
  - Hidalgo: crown 0.35 → 0.1 (the gacho's height plus about 2.5 cm, with the crease kept at dent 0.07), brim 1.25 → 1.0.
  - Bairoletto: crown 0.18 → 0.05, brim 1.05 → 0.9 (about 10% smaller).
  - Every hat band and the kepi's now hug the skull's skin outline plus 0.04 dm, instead of the hair's plus 0.08.
- **Checks (2026-10-24):**
  - tools tests: 70 OK;
  - `--check` on each of the four models: exit 0;
  - Bairoletto reconverted byte-identical;
  - `cargo test -p pampa-render`: 232 passed.
  - The triangle counts are unchanged.

### Hat follow-up (user, 2026-10-24)

The user called Farach perfect ("muy fachero"); he was left untouched.

The other hats were made smaller and raised so the eyes and brows show under the brim. The bands still rest on the skull, because they follow its outline at the new height. Lower `fit` raises a hat.

| Hat | `fit` | `brim` | `crown` |
|---|---|---|---|
| Hidalgo's | 1.05 → 0.74 | 1.0 → 0.9 | 0.1, crease kept |
| Bairoletto's chambergo | 1.05 → 0.9 | 0.9 → 0.85 | 0.05 |
| Protagonist's gacho | 0.85 → 0.78 | unchanged | unchanged |

The gacho had sat at the brow line, so it was raised only slightly.

Checks:
- tools tests: 70 OK;
- `--check` on each of the four models: exit 0;
- `cargo test -p pampa-render`: 232 passed.

## Next step

1. Delete the 12 untracked files listed under Deleted files.
2. In game, check `goto castex` (Farach and Bairoletto) and `goto nievas` (Don Hidalgo): the hats, the kepi, the poncho and the collars at game distance and in the game's lighting.
3. Commit (the user's decision).
