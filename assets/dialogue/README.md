# Dialogue

Everything people say lives in this folder as plain text files in
[RON](https://github.com/ron-rs/ron) (Rusty Object Notation). You do not need
to know Rust to edit them: change a file, save it, and start the game again.
No recompiling.

## `bumps.ron`: complaints when the rider bumps into somebody

```ron
(
    // Shoved at a walking pace: annoyed, but mild.
    walk: [
        "¡Eh, mirá por dónde andás!",
        "¡Despacito, paisano!",
    ],
    // Run into at a gallop: scared and angry.
    gallop: [
        "¡Casi me pisa el pingo, che!",
        "¡Más despacio, que no es carrera!",
    ],
)
```

- `walk` is used when the rider shoves somebody at a walking pace.
- `gallop` is used when the rider runs into somebody at a gallop.
- Brushing past at a crawl says nothing, and the same person does not
  complain again for a few seconds.
- The game picks a line at random from the right pool, never the same line
  twice in a row.

### Adding a line

Add a new quoted line inside the brackets, followed by a comma:

```ron
    walk: [
        "¡Eh, mirá por dónde andás!",
        "¡Mi nueva línea!",
    ],
```

Lines starting with `//` are comments and are ignored.

### Rules

These hold for every file in this folder.

- **Voice.** Spanish, Rioplatense rural voice of the 1920s–1930s (`vos`/`usted`,
  `che`, `pingo`, `paisano`...). Keep it respectful: no slurs.
- **Length.** At most **40 characters** per line (accents count as one
  character). Lines are drawn in a small speech bubble.
- **Characters.** Lines are drawn with the game's own pixel font, which has:
  - letters `A`–`Z` and `a`–`z`, digits `0`–`9`;
  - `Á É Í Ó Ú Ñ Ü` and `á é í ó ú ñ ü`;
  - space and `. , - : ' ! ? ¡ ¿`.

  Anything else (quotes, `#`, emoji...) is not drawn. The tests reject such
  characters in the shipped file, and the game logs a warning at startup.
- **No empty pools, no blank lines.** Every pool needs at least one line.

## `errand.ron`: the first errand

```ron
(
    // Don Hidalgo, when the player talks to him in the plaza.
    hidalgo: [
        "Acá en Monte Nievas no hay comisaría.",
        "Preséntese en la comisaría de Castex.",
    ],
    // The player, at Don Hidalgo's horse before talking to him.
    not_your_horse: [
        "Este caballo no es mío.",
    ],
    // Comisario Farach, when the player reports at his door.
    farach: [
        "¡Buenas! ¿Lo manda Don Hidalgo?",
    ],
)
```

- Each list is a short sequence, said **in order**, one line after the other
  in a speech bubble, a few seconds each (`F` starts it; it plays by itself).
- `hidalgo` is said once, when the player first talks to Don Hidalgo. It
  should tell the player the story: Monte Nievas has no comisaría, report to
  comisario Elías Farach at the Castex comisaría, and take the horse tied at
  the general store (the tests check that it names the comisaría, Castex,
  Farach, the horse and the almacén).
- `not_your_horse` is said by the player above their own head when they try
  the horse before talking to Don Hidalgo.
- `farach` is said once, when the player reports; the errand is then done.
- The rules above (voice, length, characters, no empty lists, no blank lines)
  apply here too. Note the font has no double quotes, `;` or parentheses.

## When something is wrong

If a file is missing or cannot be read (a missing comma or quote, a line that
is too long, an empty pool), the game still starts: it logs a warning naming
the problem and uses a small built-in set of lines instead. Run
`cargo test -p pampa-content -p game` to check the files (format, length and
characters) before playing.

Fields the game does not know yet (for example a future per-look pool) are
ignored, so they can be added ahead of the code that uses them.
