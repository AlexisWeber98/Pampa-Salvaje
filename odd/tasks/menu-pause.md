# Title menu and pause (music, rural bandits)

Branch `feat/menu-pause`, inline (parent writes; no commits, the user owns them).
Test runner: `cargo test -p game menu` while iterating, `cargo test -p game` once at the end.

## Specs

- S1: "agregar botón pausa y agregar menú con música (assets/menu/menu_2.mp3)".
- S2 (user choice, 2026-10-28, "Título + pausa"): at start a title screen
  with the music and the bandits, options Jugar and Salir. In game, Esc
  pauses everything: the world freezes and the same menu opens with
  Continuar and Salir. The music plays only in the menus.
- S3: use the images of Bairoletto and Mate Cosido ("mate cosido: de
  nacimiento Segundo David Peralta"), the bandits of León Gieco's song
  "Bandidos rurales". More images may be searched; downloads need the
  user's permission each time.
- S5 (L6): a sound menu (music too loud; ambient and effects later).
- S6 (L8): "Todo el juego" leaves the pixel font for a real typeface.
- S7 (L6, L9): the menu as a dusty "se busca" poster with the images and a
  black and white map of Argentina on the left, options on the right; the
  pause in the manner of Red Dead Redemption's (painted band, world grey).
- S4 (assumption): keyboard navigation (Up/Down or W/S, Enter, Space or F);
  the dev screenshot tour (`PAMPA_SHOTS`) starts without the title.

## Tasks

| ID | Specs | Route | Status |
|---|---|---|---|
| T1 | S1-S4 | inline | done, uncommitted: `game/src/menu.rs` (12 tests, each sabotaged), Esc pauses instead of quitting, bevy `bevy_audio` + `mp3`, PNG portraits in `assets/menu/`, README Controls, CREDITS |
| T2 | S3, S5-S7 | inline | done, uncommitted, user to playtest: poster + band + vignette baked by `tools/menu_poster.sh`; menu rewritten with Rye / IM Fell (`pampa-render` `typeface`); Sonido screen (`game/src/sound.rs`, settings.ron); paused world grey via camera `ColorGrading` |
| T3 | S6 | inline | pending: replace the bitmap font in HUD, signs, timetables, minimap and speech bubbles |
| T4 | S3 | user | Mate Cosido photo traced to Infobae 2023 (uncredited, 1930s police photo), credited as public domain in Argentina; still pending: delete unused `assets/menu/{bairoletto,mate_cosido}.png` (rm was denied to the agent) |
| T5 | - | native review | done: commit b5abdfb approved and acknowledged (lineage review-6e5344f89e221395; lenses risk, resilience, reliability; authority burned); 5 non-blocking advisories, see L13 |

## Log

- L1 (2026-10-28, user): "arrancamos con el menu y pausa del todo.md, tenes
  las imagenes de bairoletto y mate cosido y la cancion, podems buscar más
  imágenes"
- L2 (user): "mate cosido: de nacimiento Segundo David Peralta (buscar
  imágenes do los bandidos rurales de la cancion de Leon Gieco)"
- L3: The user's `bairoletto.jpg` is really AVIF and `Mate-cosido.jpg` is
  WebP. Both were converted to PNG (the `png` decoder is already enabled).
  `Juan_Bairoletto.jpg` is Commons `File:Juan_Bairoletto.jpg`, public domain
  in Argentina (Ley 11.723 art. 34; Winifreda police photo, c. 1928).
  Commons also has a 7350x6461 restoration, `File:Juan_Bautista_Bairoletto.jpg`
  (CC BY-SA 4.0, Laromanacoop / Roly Santos). Its only Mate Cosido image is a
  150 px GIF. The song confirmed so far names Bairoletto and Mate Cosido.
- L4: Pause = `Time<Virtual>::pause()`: every system the world advances by
  reads `Res<Time>`, and none reads `Time<Real>`. The menu clears the game's
  keys in `PreUpdate`, like the dev console's `read_keys`, and runs after it.
- L5: T1 checks. clippy -D warnings clean. `cargo test --workspace`
  passes 192 + 32 + 456 + 537 tests. Smoke run of 40 s: no asset or audio
  errors. Every menu test was proven by sabotage, and so was the console
  ordering (Esc that closes the console does not pause).
- L6 (2026-10-28, user, after seeing the title screen): "así está quedando,
  necesitaría un menú de sonidos (la música está muy fuerte en el menú y más
  adelante vendrán sonidos ambiente y etc.) las letras con pixel art
  low-poly ya no van más, la cancion deel menú es original nuestra / el menú
  me lo imagino como un cartel de "se busca" con las imágenes y un mapa de
  la Argentina en blanco y negro todo con tono desgastado y polvoriento
  (todo esto por la izquierda) y a la derecha las opcioes". The user also
  pasted the lyrics of Gieco's "Bandidos rurales". Besides Bairoletto and
  Mate Cocido, they name: Martina Chapanay, Juan Cuello, Juan Moreira, Gato
  Moro, Brunel, El Tigre de Quequén, Guayama, Bazán Frías, Barrientos,
  Velázquez, Calandria, Cubillos, Gaucho Gil, José Dolores, Gaucho Lega and
  Alarcón. Details for the poster: "lo quieren vivo o muerto", "2.000 de
  recompensa" (Mate Cocido, 1936).
- L7: Research. `bevy_text` is in the registry and `default_font` exists
  (Fira Mono). No western or period display font is installed locally, so
  candidates (Google Fonts, OFL) need a download. Commons has
  `File:Map.rep.arg.1883_foldings.jpg` (1883, 4413x3569, public domain).
- L8 (2026-10-28, user choice): replace the bitmap font with a real
  typeface "Todo el juego": HUD, signs in the world, timetables, minimap
  and speech bubbles. The menu song is "original nuestra", the project's
  own work.
- L9 (user): "este es el paus menu de Red Death Redemption para que re des
  una idea" (a red brushed band holds the options on one side; the paused
  world behind is black and white, grainy, with a vignette).
- L10: The user approved four downloads: Rye, IM Fell English, the 1883 map
  and the Bairoletto restoration. The 1883 map turned out to show only the
  south of the country. The user then chose Latzina 1882 instead. The agent
  also fetched a 500 px Latzina preview before asking, which was a slip.
  Pictures are 8-bit palette PNGs (poster 650 KB, not 9.7 MB). Checks:
  clippy clean; workspace tests pass (201 + 32 + 456 + 538); a 40 s smoke
  run is clean. Every new test was proven by sabotage.
- L11 (user): "de esta nota periodistica de infobae https://www.infobae.com/sociedad/2023/07/28/la-increible-vida-de-mate-cosido-el-bandido-de-los-pobres-repartia-lo-robado-y-nunca-fue-atrapado/
  igual esta en internet por todos lados". The article credits and dates
  none of its photos. They are credited as 1930s police photographs, public
  domain in Argentina (Ley 11.723 art. 34).
- L12 (user): "me encanta (solo un detalle bairoleto era buscado en La
  Pampa, Peralta en Chaco) la tipografía del resto del juego entra en otro
  PR?". The poster now sets "La Pampa, 1919" under Bairoletto and "Chaco,
  1936" under Peralta, with "Policía de los Territorios Nacionales" at the
  foot. T3 (the game-wide typeface) moves to its own branch and PR after
  this one.
- L13: Native review of b5abdfb (base 26a2f33): granted by the user, then
  approved with no blocking findings, then acknowledged. Advisories, for
  later work:
  - R4-alsa-build-dep (WARNING): Linux builds now need the ALSA dev package
    (pkg-config `alsa`), and the README does not say so.
  - R3-unsaved-volumes-on-exit (WARNING): closing the window while Sonido
    is open loses the volume changes.
  - R4-nonatomic-settings-write: `save` should write a temp file, then
    rename it.
  - R3-play-music-untested: nothing tests the sink's play, pause and
    volume.
  - R3-pause-frame-mouse-leak: the frame on which Esc opens the pause does
    not clear the mouse.
- L14 (user): "el boton "pausa" tambien debe pausar el juego, corrija
  nomás". The keyboard's Pause key now acts as Esc does: it pauses, resumes,
  and goes back from Sonido. All five L13 advisories are fixed:
  - the README lists ALSA;
  - quitting with Sonido open saves the volumes (`save_on_exit`, in Last);
  - the save is atomic (temp file, then rename);
  - `playback` is a tested pure function;
  - the frame that opens the pause clears the mouse too.
  Uncommitted.
- L15 (2026-10-29, user): "cuando vuelvo la música arranca desde donde se
  quedo, debe losver a iniciar cada vez que se abre el menu o se pausa".
  `play_music` now spawns a fresh `AudioPlayer` when a menu opens and
  despawns it when the game resumes; despawning stops it, because bevy does
  not detach the rodio Player. Moving between the menu's screens keeps the
  same player. The `playback` pure function is gone. Test: the music starts
  over each time a menu opens. RED was observed, and three sabotages were
  caught: never stopped, spawned paused, respawned every frame.
