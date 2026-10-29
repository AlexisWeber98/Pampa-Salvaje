# Diálogo

Todo lo que dice la gente vive en esta carpeta como archivos de texto plano en
[RON](https://github.com/ron-rs/ron) (Rusty Object Notation). No hace falta
saber Rust para editarlos: se cambia un archivo, se guarda, y se vuelve a
iniciar el juego. Sin recompilar.

## `bumps.ron`: quejas cuando el jinete choca a alguien

```ron
(
    // Empujado al paso: molesto, pero sin exagerar.
    walk: [
        "¡Eh, mirá por dónde andás!",
        "¡Despacito, paisano!",
    ],
    // Atropellado al galope: asustado y enojado.
    gallop: [
        "¡Casi me pisa el pingo, che!",
        "¡Más despacio, que no es carrera!",
    ],
)
```

- `walk` se usa cuando el jinete empuja a alguien al paso.
- `gallop` se usa cuando el jinete atropella a alguien al galope.
- Rozar a alguien muy despacio no provoca ninguna línea, y la misma persona no
  vuelve a quejarse durante unos segundos.
- El juego elige una línea al azar del grupo que corresponde, nunca la misma
  línea dos veces seguidas.

### Agregar una línea

Agregar una nueva línea entre comillas dentro de los corchetes, seguida de una
coma:

```ron
    walk: [
        "¡Eh, mirá por dónde andás!",
        "¡Mi nueva línea!",
    ],
```

Las líneas que empiezan con `//` son comentarios y se ignoran.

### Reglas

Valen para todos los archivos de esta carpeta.

- **Voz.** Español, voz rural rioplatense de 1920–1930 (`vos`/`usted`,
  `che`, `pingo`, `paisano`...). Mantenerla respetuosa: sin insultos
  ofensivos.
- **Longitud.** Como máximo **40 caracteres** por línea (cada letra acentuada
  cuenta como un carácter). Las líneas se dibujan en un pequeño globo de
  diálogo.
- **Caracteres.** Las líneas se dibujan con la tipografía propia del juego (IM
  Fell English), que tiene:
  - las letras `A`–`Z` y `a`–`z`, los dígitos `0`–`9`;
  - `Á É Í Ó Ú Ñ Ü` y `á é í ó ú ñ ü`;
  - el espacio y `. , : ; - ' " ! ? ¡ ¿ ( ) % $ /`.

  Lo que le falta a la tipografía (emoji, otros alfabetos...) no se dibuja.
  Los tests rechazan esos caracteres en el archivo incluido, y el juego
  registra un aviso al inicio.
- **Sin grupos vacíos ni líneas en blanco.** Cada grupo necesita al menos una
  línea.

## `errand.ron`: el primer encargo

```ron
(
    // Don Hidalgo, cuando el jugador le habla en la plaza.
    hidalgo: [
        "Acá en Monte Nievas no hay comisaría.",
        "Preséntese en la comisaría de Castex.",
    ],
    // El jugador, junto al caballo de Don Hidalgo antes de hablar con él.
    not_your_horse: [
        "Este caballo no es mío.",
    ],
    // El comisario Farach, cuando el jugador se presenta a su puerta.
    farach: [
        "¡Buenas! ¿Lo manda Don Hidalgo?",
    ],
)
```

- Cada lista es una secuencia breve que se dice **en orden**, una línea tras
  otra en un globo de diálogo, unos segundos cada una (`F` la inicia; se
  reproduce sola).
- `hidalgo` se dice una vez, cuando el jugador habla por primera vez con Don
  Hidalgo. Debe contarle la historia al jugador: Monte Nievas no tiene
  comisaría, hay que presentarse ante el comisario Elías Farach en la
  comisaría de Castex, y tomar el caballo atado en el almacén de ramos
  generales (los tests comprueban que nombre la comisaría, Castex, Farach, el
  caballo y el almacén).
- `not_your_horse` lo dice el jugador sobre su propia cabeza cuando intenta
  usar el caballo antes de hablar con Don Hidalgo.
- `farach` se dice una vez, cuando el jugador se presenta; con eso el encargo
  termina.
- Las reglas de arriba (voz, longitud, caracteres, sin listas vacías ni líneas
  en blanco) también valen aquí. La tipografía tiene comillas dobles, `;` y
  paréntesis, así que se pueden usar.

## Cuando algo está mal

Si un archivo falta o no se puede leer (una coma o una comilla que faltan, una
línea demasiado larga, un grupo vacío), el juego arranca igual: registra un
aviso que nombra el problema y usa en su lugar un pequeño conjunto de líneas
incorporadas. Ejecutar `cargo test -p pampa-content -p game` para comprobar
los archivos (formato, longitud y caracteres) antes de jugar.

Los campos que el juego todavía no conoce (por ejemplo, un futuro grupo por
aspecto) se ignoran, así que se pueden agregar antes que el código que los
usa.
