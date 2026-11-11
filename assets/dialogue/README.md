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
- **Longitud.** Las quejas se dibujan en un pequeño globo de diálogo: como
  máximo **40 caracteres** por línea (cada letra acentuada cuenta como un
  carácter). Las líneas de la historia se muestran como subtítulos: como
  máximo **100 caracteres**, así cada una queda en pantalla todo el tiempo de
  leerla.
- **Caracteres.** Las líneas se dibujan con la tipografía propia del juego (IM
  Fell English), que tiene:
  - las letras `A`–`Z` y `a`–`z`, los dígitos `0`–`9`;
  - `Á É Í Ó Ú Ñ Ü` y `á é í ó ú ñ ü`;
  - `à è ì ò ù` y otras letras latinas acentuadas;
  - el espacio y `. , : ; - ' " ! ? ¡ ¿ ( ) % $ /`, y también `’` y `…`
    como en `historia`.

  Lo que le falta a la tipografía (emoji, otros alfabetos...) no se dibuja.
  Los tests rechazan esos caracteres en el archivo incluido, y el juego
  registra un aviso al inicio.
- **Sin grupos vacíos ni líneas en blanco.** Cada grupo necesita al menos una
  línea.

## `errand.ron`: la historia del capítulo 1

La historia, paso a paso (actos 2 a 4 de `historia`), y el encuentro del acto 5:

```ron
(
    police_town: "Eduardo Castex",
    not_your_horse: [
        "Este caballo no es mío.",
    ],
    steps: [
        (
            goal: Talk(Hidalgo),
            objective: "Buscá a Don Hidalgo en la plaza",
            scene: [
                Say(Hidalgo, "Tome, le escribí una carta pa’l comisario."),
                Receive(Letter),
                Earn(pesos: 2),
            ],
        ),
        (
            goal: TakeHorse,
            objective: "Tomá el caballo de Don Hidalgo",
        ),
        (
            goal: Talk(Farach),
            objective: "Presentate al comisario Farach en {town}",
            scene: [
                Say(Lisandro, "Buenas, Comisario, le traigo unos papeles que me envió mi tío pa’ usté’"),
                Say(Farach, "¿y vos quien sos?"),
                HandOver(Letter),
            ],
        ),
    ],
    amadeo: [
        Say(Amadeo, "No se haga problema joven, tome, llévese un pedazo,"),
        Receive(Cheese),
    ],
)
```

- `steps` son los pasos, **en orden**; hay uno pendiente a la vez y cada uno se
  cumple una sola vez. Los actos siguientes se agregan como pasos nuevos al
  final.
- `goal` es lo que pide el paso: `Talk(Hidalgo)`, `Talk(Farach)` o
  `Talk(Bairoletto)` (hablarle con `F`, al alcance; la escena de Bairoletto
  empieza sola cuando el jugador llega a pie por la vereda a la puerta abierta
  de La Colonia: entra tímido, mira el lugar, Bairoletto lo llama y va hasta su
  mesa), o `TakeHorse` (montar el caballo de Don Hidalgo, que desde
  entonces es del jugador y le enseña a montar).
- `objective` es la línea del objetivo del HUD mientras el paso está pendiente
  (como máximo 48 caracteres); `{town}` se reemplaza por `police_town`. El
  minimapa marca a quien hay que hablarle o al caballo. Puede faltar: un paso
  sin `objective` es un paso sin aviso, sin línea en el HUD ni marca en el
  minimapa. Escrito, no puede estar en blanco.
- `scene` es lo que sucede al cumplir el paso (puede faltar). Mientras dura, el
  jugador no maneja y la cámara encuadra a los dos que hablan:
  - `Say(quién, "texto")`: una línea como subtítulo, con el nombre de quien la
    dice, el tiempo de leerla. `quién` es `Lisandro` (el jugador), `Hidalgo`,
    `Farach`, `Bairoletto` o `Amadeo`.
  - `Earn(pesos: 2, centavos: 50)`: le dan plata (los centavos pueden faltar);
    el HUD la muestra.
  - `Receive(…)` y `HandOver(…)`: recibe o entrega algo: `Letter`, la carta
    de presentación de Don Hidalgo, o `Cheese`, el pedazo de queso del Viejo
    Amadeo. Lo entregado no vuelve. No se ve en la mano: solo se lleva.
  Las acciones suceden al instante, entre una línea y la siguiente.
- Lo que `historia` cita se copia palabra por palabra; lo narrado se vuelve
  acciones. Una línea larga se corta en varias `Say` seguidas del mismo
  personaje, en un punto o una coma, sin cambiar ninguna palabra.
- `amadeo` es la escena del encuentro con el Viejo Amadeo (puede faltar: sin
  ella no hay encuentro). No es un paso: sucede una sola vez, la primera vez que
  el jugador pasa a menos de 8 m de su puesto, a pie o montado, en cualquier
  momento del juego, y no mueve la historia; Lisandro se le acerca solo antes
  de la primera línea. Sus líneas se validan como las de los pasos. Ningún paso
  puede pedir `Talk(Amadeo)`.
- `not_your_horse` lo dice el jugador cuando intenta usar el caballo antes de
  que Don Hidalgo se lo preste (no detiene al jugador).
- `police_town` es el pueblo de la comisaría, por su nombre en
  `world/towns.ron`.
- Las reglas de arriba (voz, longitud, caracteres, sin listas vacías ni líneas
  en blanco) también valen aquí.

## Cuando algo está mal

Si un archivo falta o no se puede leer (una coma o una comilla que faltan, una
línea demasiado larga, un grupo vacío), el juego arranca igual: registra un
aviso que nombra el problema y usa en su lugar un pequeño conjunto de líneas
incorporadas. Ejecutar `cargo test -p pampa-content -p game` para comprobar
los archivos (formato, longitud y caracteres) antes de jugar.

Los textos de `errand.ron` se pueden cambiar sin tocar el código ni los tests:
los tests leen las líneas de este archivo y comprueban que el juego las muestre
tal como están escritas. Lo que sí comprueban es la forma de la historia: el
orden de los pasos, quién habla en cada escena y qué se da o se entrega (la
carta, la plata, el queso). Si se cambia eso, hay que cambiar también el test
`the_shipped_file_tells_acts_two_to_four_in_order_and_amadeo_apart` de
`crates/content/src/errand.rs`.

Los campos que el juego todavía no conoce (por ejemplo, un futuro grupo por
aspecto) se ignoran, así que se pueden agregar antes que el código que los
usa.
