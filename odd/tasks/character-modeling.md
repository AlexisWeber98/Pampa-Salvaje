# Character modeling

Raise the characters' 3D modeling toward realism; first two prototypes of the protagonist to compare.
Branch: decided once a prototype is chosen (the run's work is uncommitted on `feat/realistic-run`); delivery `ask-on-risk`; runner `cargo test -p pampa-render --lib character::` and the tools' unittests.

## Specs

- S1 (L1): "mejoremos el modelado 3d de los personajes"; the standing goal in todo.md: "Máximo realismo posible en personajes, más animaciones, low-poly no va más".
- S2 (L2): "haceme uno con la primera opcion y uno con la segunda, y comparamos". Option one: "Subir el pipeline MakeHuman": the same skeleton and animations, a full-resolution body (no proxy) with subdivision, generated clothing with more faces and smoothing, Poly Haven 2K fabrics (CC0) with normal maps, hair and beard with volume. Option two: "Modelos de terceros realistas": realistic characters under a free license, adapted to the `game_engine` skeleton.
- S4 (L9): "modelalo, yo no se modelar, apenas estoy viendo como hacerlo correr": I model the 1919 clothing for the new protagonist (Sports_Male_01, in game in swim trunks): the outfit of `protagonist.mhm` (shirt, waistcoat, bombacha, boots, poncho, hat, neckerchief, sash, rastra).
- S6 (L17): user, in-game screenshot of the dressed Rocketbox protagonist: "fijate el ponche atravezando el hombro". The poncho folded over the shoulder must lie over the shoulder and chest, not through them.
- S7 (L17): the dressing is a reproducible tool in `tools/` (the scratchpad copy was lost), with Poly Haven 2K fabrics (CC0; the user allowed re-downloading rough_linen, poly_wool_herringbone and brown_leather, in `~/Downloads/polyhaven/`).
- S3: the comparison is renders of both prototypes beside the current protagonist, from the same cameras and light (full body front, three-quarter, face), plus triangle and texture counts. Assumption: the prototypes do not replace the shipped models until the user chooses.

## Tasks

- T1 S2 inline: prototype B, Rocketbox `Male_Adult_05` (MIT): 7,836 triangles, 80 bones (Bip01, with face bones), 2K color, normal and specular maps, modern clothes, T-pose. Rendered. Done.
- T2 S2 explorer, then inline: prototype A, built in the scratchpad with an unmodified `.mhm` copy (proxy `male_generic`, `HighPolyEyes`, 2K textures) and a patched copy of `mh2gltf.py` (Poly Haven rough_linen, poly_wool_herringbone and brown_leather 2K, greyscale and tinted, with normal maps): 32,198 triangles. The repo is untouched. Done.
- T3 S3 inline: comparison sheet `~/Documents/pampa-prototipos/comparacion.png`, plus the three models for Blender in that folder. Done. User chose B.
- T4 S4 explorer: map what `mh2gltf`'s garment code needs from the body, to dress a Rocketbox body with it. Pending.
- T5 S4, S6, S7 delegated (long-session backstop): `tools/dress.py` + `test_dress.py` dress the Rocketbox body in `protagonist.mhm` with `tools/mh2gltf.py` (`--fabrics DIR`, `FABRIC_TILE` 3; poncho probed across its width against the trunk and upper arms, lifted out of the arm capsule as whole sections, the arm's weight share from 8 cm in from the shoulder). Installed: 9,486 triangles, 10 maps at 2048². Skeleton unchanged, `humanoid.glb` untouched. Checks: mh2gltf tests 69 OK, dress 3 OK (RED/GREEN, each sabotaged), parent re-ran both OK; character 130/134 with the 4 known failures; all 3 poncho tests pass (the rider's from 0.0105 m to 0.000). Without `--fabrics`, Farach rebuilds byte-identical; Volga's poncho changes on its next rebuild (880 to 1,104 triangles). Renders reviewed: poncho over the shoulder; small dark shards remain on the shoulder top. Rocketbox MIT credit added by the parent. Done, uncommitted.
- T6 S4 pending: clean garment edges (clip the triangles at the cut, L16).

## Log

- L1 2026-11-06: user: "cambio de planes, mejoremos el modelado 3d de los personajes, yo me estoy viendo unos tiutoriales de blender para animar el run". Measured protagonist: 5,882 triangles (body proxy 2,088; garments about 300 each), clothing textures 64×64, skin 1024, no normal maps. The render shows a faceted shirt and waistcoat and painted hair.
- L2 2026-11-06: user, offered "Subir el pipeline MakeHuman (Recomendado)", "Modelos de terceros realistas" and "Vos en Blender, yo el pipeline": "haceme uno con la primera opcion y uno con la segunda, y comparamos".
- L3 2026-11-06: user: "despues, cuando estén, dame el path de los archivos,los modelos para importarlos en blender y ver si puedo animar o mejorar la animación de carrera". Delivered `~/Documents/pampa-prototipos/`:
  - `protagonista_actual/protagonista.gltf`: the game model with all 17 clips; Blender import checked: 53 bones, 17 actions;
  - `prototipo_A/model.gltf`;
  - `prototipo_B/Male_Adult_05.fbx` with `Textures/`.
- L4 2026-11-06: finding. A's density adds little: the face and the garments look the same. Its fabrics barely show because `CLOTH_TILE = 6` makes the weave tiny. The garments cut from the body keep jagged necklines along the quads. B's realism comes from its photographic 2K textures and normal maps. Using B needs retargeting from Bip01 to `game_engine` (or the reverse), 1919 clothing and rescaling (1.81 m).
- L5 2026-11-06: user: "ahy alguno para probar en el juego?". Installed A as the protagonist for a trial (same skeleton: 53 joints, rest within 0.7 mm). Only the triangle-budget test failed, by design (32,198 triangles).
- L6 2026-11-06: user: "instalame el otro para ver (B)". Built B on the game skeleton with scratchpad `rocketbox/extract.py` (Blender) and `build_b.py`:
  - each Bip01 bone is turned onto its game_engine bone, parents first; the chain ends (head, balls, finger tips) and the face bones follow their parent;
  - the joints move to the avatar's, keeping the template's rest rotations;
  - the weights are renamed and limited to 4;
  - the textures are converted from TGA to PNG.
  Rendered with the game's idle and jog: correct. Installed for a trial (uncommitted). Failing character tests:
  - the rider's free arm hits the horse;
  - two viewer tests (the first-person head cut needs MakeHuman's skin material);
  - the known rider flake.
- L7 2026-11-06: user: "me gusta más el B, hay otros persnoajes descargables? ente me sirve para un NPC o para otro personaje pero para el pretagonista necesitnto uno más jeven". Male_Adult_05 is kept for an NPC. Screened the 66 Rocketbox men (Adults and Professions) from their thumbnails. Rendered four young candidates on the game skeleton: Business_Male_06, Male_Adult_02, Male_Adult_08 and Sports_Male_01. Their models and the sheet are in `~/Documents/pampa-prototipos/candidatos/`.
- L8 2026-11-06: user chose "Sport_Male_01". Installed Sports_Male_01 (m021, 7,358 triangles, 1.80 m, swim trunks) as the protagonist for a trial, uncommitted. Next: 1919 clothing (hat, shirt, bombacha, poncho, faja, boots) over its body.
- L9 2026-11-06: user (screenshot: Sports_Male_01 riding in game, in swim trunks): "modelalo, yo no se modelar, apenas estoy viendo como hacerlo correr".
- L10 2026-11-06: built scratchpad `rocketbox/dress_b.py`. It feeds the Rocketbox body, already on the game skeleton, to mh2gltf's own garment code:
  - the input body is welded by position, converted to decimetres with V flipped, and given dense weights in rig order;
  - `dress`, `generate` and `write` run with `protagonist.mhm`, using the fabric hook of `protoA/mh2gltf_proto.py`;
  - the result is 9,270 triangles: shirt 998, waistcoat 510, bombacha 806, boots 798, poncho 848, hat 460, plus the small pieces;
  - the game's clips play on it in render.
  Installed as the protagonist (uncommitted trial). Character tests: 127 passed, 6 failed:
  - the two viewer tests (first-person head) and the known rider flake;
  - balls of the feet 4.6 cm up instead of 2.4 (`every_model_stands_on_its_own_feet`);
  - the rider's free arm hits the horse;
  - `a_tied_horse_keeps_its_tack_and_reins_clear_through_every_idle`.
  Visible: necklines still jagged where garments are cut from the body.
- L11 2026-11-06: user: "si, el brazo izq atravieza el pecho en el caballo, el derecho está muy rígido, los hobrom muy atrás en todo momento (parece que tiene una deformidad en los huesos) dejalo en malla, acemedamos los huesos, los movimientos y despues lo vestimos, vamos arreglando las bases antes de retocar". Back to swim trunks (S5: the bones and movement first, clothing after).
- L12 2026-11-06: three fixes.
  - Hunch (build_b.py): the spine, neck and clavicles had been aimed at MakeHuman's forward-curved spine, which hunched the straight Rocketbox back and pushed the head forward. Now only the arms and legs are aimed; the legs must be, or the planted foot slides and the balls sit wrong. The torso pivots (spine_01..03 and the clavicles) also moved to the template's proportional places, without moving the mesh.
  - Arms on horseback (crates/render/src/character/rider.rs): the hand targets were fixed offsets from the seat, tuned on MakeHuman; Rocketbox's shoulders sit 6 cm higher and its arms are 4 cm longer, so the reins arm crossed the chest and the free arm went straight (165°). New `fit_hand` with `REF_SHOULDERS` and `REF_ARM`, measured on the MakeHuman protagonist seated: each rider's hand target is carried to its own seated shoulder and scaled by its arm length (`RiderMotion::arms`). The MakeHuman protagonist is unchanged: all character tests pass but the two known flakes, which pass alone. Other MakeHuman riders now scale to their own proportions.
  - Results: with Rocketbox, 130 of 133 pass; the failures are the known rider flake and the two first-person viewer tests. The free-arm and stand-on-feet tests pass.
- L13 2026-11-06: user (in-game screenshots): "los brazos muy atrs, todo desde los hobros, la caminata esta bien aunque los brazos muy rígidos, deben estar mas "flojos" los codos, al correr mantiene los hobros "dislocados" hacia atras (la espalda) y hasta se encoraba hacia uno de los lados, como si tuviera escoliosis". Measured in the shared clips, so it applies to every character; on MakeHuman the clothes hid it:
  - idle: upper arm 8–29° back, elbows 28–44°, clavicles 25–37° off rest;
  - walk elbows: 10–25°;
  - jog: trunk leaning to one side all cycle (mean −6.4°), arm always behind (mean −26°).
  Fixes in `tools/retarget.py`:
  - `HANG` / `hang_arms` for idle, yes and no: clavicles at rest, arm pitch mean −2°, elbow mean 15°, half of the sway kept, the outward angle kept;
  - `ELBOW` raised to (18, 35);
  - `centered` for jog: lean and twist removed about their means, iterated; `RUN_ARM` −8;
  - `RUN_ELBOW` (85, 0.6) through `bend_elbows`, replacing the rib rule.
  The clips are rebuilt against the MakeHuman protagonist (the shared reference). Speeds unchanged (walk 1.271, jog 4.093). Tests:
  - retarget: 53 OK, and every new test was proven by sabotage;
  - character: green but for the two listed flakes, which pass alone.
  Rocketbox is reinstalled for the user's trial.
- L14 2026-11-06: user: "la caminata me gusta un poco mas, aun tiene los brazos muy abietros / al correr aun tiene escoliosis, la columno tiene un ligero quiebre hacia la derecha lo que hace que el brazo izq quede "junto al cuerpo" yel derecho más separado". Measured the jog spine segments' side means (+7, +5.7, −1.1, +8.2°: an S that the whole-line centring hid) and the arms' mean outward angle (left −3.6°, right +9.3°); the walk hung its arms 11.4° out (the MakeHuman rest). Fixes:
  - `centered` straightens each spine/neck segment about its mean, bottom up, in two rounds;
  - `RUN_OUT` 6° for both arms;
  - `WALK_OUT` 5° in `_swing_arm`.
  Result: segments at 0.0°, arms 6.0/6.0 running and 5.1/5.1 walking. Tests: 53 retarget OK, new asserts proven by sabotage; character tests green but for a listed flake. Rocketbox reinstalled.
- L15 2026-11-06: user: "ya tengo el Rocketbox, igual aun no me convence la carrera, vestilo y vemos como queda". Redressed the corrected body (game_sm4) with scratchpad `dress_b.py` and Poly Haven 2K fabrics (`PROTO_FABRICS`); installed for a trial (body backed up in the scratchpad `rb_body_keep/`). The jog is regenerated on this model (`run_cycle.py --target` the dressed protagonist): 5.436 m/s at the MakeHuman reference scale, 5.0 on his legs; `Clip::Jog` 5.436. Video `~/Videos/jog_vestido.mp4`. Character tests 127/133: the planted-foot slide now passes; failing: half-seat (known), two viewer tests (known), stands-on-feet (boots), the tied horse tack, and the rider poncho over the shoulder. Visible: jagged waistcoat neckline and poncho edges.
- L16 2026-11-06: the scratchpads were emptied (dress_b.py, mh2gltf_proto.py, fabrics, the undressed body backup and the preview tools are gone). Mapped the jagged edges in `tools/mh2gltf.py`: `dress()` keeps a skin face only when all three corners pass `cut()` (per vertex: station band, strongest-weight limb for armholes, normal.z > 0.3 for the V-neck), and `shell()` copies whole triangles and adds rims, so the edges step along the triangles; outer-layer holes come from single vertices flipping out of the selection. Fix: `cut()` returns a signed field per garment, `shell()` clips the crossing triangles at its zero, and `dress()` uses the same field for ownership and hidden skin. The poncho is generated (`make_poncho()`, `diagonal()`: 24 samples on a straight line), and its border meets the faceted shells; sample across the band too, as `make_shawl_ends` does. Next (pending the user): rebuild the Rocketbox dressing as a `tools/` script (T5) with these fixes.
- L17 2026-11-06: user (screenshot): "fijate el ponche atravezando el hombro, en el sprint cada tanto se traba"; allowed re-downloading the three Poly Haven fabrics ("Sí, bajalas"). Downloaded to `~/Downloads/polyhaven/` (rough_linen diff/nor_gl, poly_wool_herringbone diff/nor_gl, brown_leather albedo/nor_gl, 2K).
- L18 2026-11-06: T5 done. Previous install backed up to `~/Documents/pampa-prototipos/respaldo_protagonista_vestido/`. Writer scratch `/tmp/dress-scratch.xGyT` (its renders still to delete: rm denied).
