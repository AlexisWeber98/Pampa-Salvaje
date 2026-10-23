# Dev console and unlockable abilities

## Objective (user, 2026-10-23)

1. A cheat console in debug builds only, opened with a hidden key
   combination, for moving around the 1:7 world quickly while developing:
   `goto` named places or points, `caballo`, `unlock all`, `help`.
2. Unlockable abilities as a general game rule: riding unlocks the first time
   the protagonist mounts Don Hidalgo's horse; from then on every horse is
   mountable. Room left for the lasso, weapons and cars.

## Decisions

- **Key combination**: `Ctrl`+`Shift`+`F12`. No game control uses `F12` or
  `Ctrl`; `Esc` (which quits the game) closes the console instead while it is
  open.
- **Debug only**: `game/src/console.rs` is declared and its plugin added under
  `#[cfg(debug_assertions)]` in `main.rs`; a release build does not compile
  it. Checked by building `--release` and finding none of the console's
  strings in the binary.
- **Input suspension**: while open, the console reads `KeyboardInput`
  messages in `PreUpdate` (after Bevy's input systems) and resets
  `ButtonInput<KeyCode>`, so no game system sees a key. The mouse still
  orbits the camera.
- **goto aboard the train**: refused with a line in the console. The ride
  owns the seated, hidden player until they get off; tearing that down from
  the console would be the riskier path.
- **Places come from world data**: a town or lagoon matches when any word of
  its name equals the typed word; towns aim at their arrival on the main
  road, the lagoon at the end of its road nearest the water (or its dry bank
  nearest the player), estancias at their patio (`Settlement::hub`), nearest
  the player or the named town. The spot is then the nearest clear, dry
  ground inside the world for a horse and rider (`World::clear_spot`, new in
  core), and `StreamFocus` moves there at once.
- **Abilities**: `pampa_core::Abilities` (a small bit set of `Ability::Ride`,
  `Lasso`, `Weapons`, `Cars`) held by the game in `PlayerAbilities`. The
  errand's `MountedHorse` event unlocks `Ride`; each unlock shows its notice
  in the HUD announcement panel ("Ahora podés montar cualquier caballo").
- **Any horse**: every standing horse carries `Mountable` (tied horses, the
  loose one, console horses). With `Ride`, `F` on foot within 3 m mounts the
  nearest: a tied one is led away for the session, the ridden coat is kept in
  `RiddenCoat` for the rider's visual and the horse he next leaves. The
  previously loose horse stays standing as an ordinary mountable horse. The
  errand flow is unchanged: Don Hidalgo's horse still answers "not mine"
  before the talk.

## Tasks

- [x] T1 Core: `Abilities`/`Ability`, `World::clear_spot` (inline).
- [x] T2 Game: `PlayerAbilities`, unlock on Hidalgo's horse with HUD notice,
  any horse mountable after unlock (delegated writer).
- [x] T3 Game: debug-only console, commands, panel in the HUD style
  (delegated writer).
- [x] T4 README sections and this document.

## Acceptance and checks

- Parsing of every command and error; `goto` resolves every named place to
  clear, dry ground inside the world; teleport moves rider and horse and the
  streaming focus; `caballo` spawns a mountable horse beside the player;
  abilities before and after the unlock; `unlock all`.
- `cargo test -p pampa-core`, `-p pampa-render`, `-p game`; `cargo build -p
  game`; `cargo build --release -p game` without the console; `cargo fmt
  --all -- --check`; `cargo clippy --workspace --all-targets -- -D warnings`.

## Progress

Implemented uncommitted on `feat/dev-console`, for the user's review. The
console and abilities tests were written together with the code: no RED was
observed separately.

- 2026-10-23 (`feat/makehuman-characters`): `modelos on|off` (also
  `personajes nuevos|viejos`) swaps Don Hidalgo, Farach and Bairoletto
  between the rigid 1919 figures (the default) and the MakeHuman models.
  Debug builds only.
- Ctrl+Shift+F12 now also closes the console (modifiers tracked from KeyboardInput events across reset_all); test `the_same_keys_close_it_pressed_one_frame_after_another`.
- `hora HH[:MM]` (added with the street lamps, 2026-10-23) moves the clock on
  to that time, to see the towns at night; refused aboard the train; test
  `hora_moves_the_clock_on_to_nightfall`.
