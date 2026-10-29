# Mundo

El mundo y sus pueblos viven en esta carpeta como archivos de texto plano en
[RON](https://github.com/ron-rs/ron) (Rusty Object Notation). No hace falta
saber Rust para editarlos: se cambia un archivo, se guarda, y se vuelve a
iniciar el juego. Sin recompilar.

## `towns.ron`: el mundo, sus pueblos y cómo están dispuestos

```ron
(
    world: (
        half_extent: 8400.0,
        chunk_size: 200.0,
    ),
    towns: [
        (
            name: "Monte Nievas",
            origin: (-2450.0, -1950.0),
            blocks: (4, 3),
            block_size: 40.0,
            street_width: 10.0,
            railway: Some((side: North, offset: 24.0)),
            plaza: Auto,
            seed: 7,
        ),
        // ... siguen Eduardo Castex, Metileo, General Pico y Caleufú.
    ],
    line: Some(["Eduardo Castex", "Monte Nievas", "Metileo", "General Pico"]),
    branches: [["Metileo", "Caleufú"]],
    lagoons: [
        (
            name: "Laguna La Arocena",
            center: (3401.0, 841.0),
            size: (215.0, 145.0),
            seed: 5,
        ),
    ],
)
```

### La geografía incluida

El archivo dispone un primer tramo de La Pampa: cinco pueblos ferroviarios en
sus posiciones relativas reales, con las distancias a escala 1:7 para que
entren en el mundo más grande (`half_extent: 8400`, un cuadrado de
16.8 km x 16.8 km). Solo se escalan las distancias entre pueblos; cada pueblo
conserva su tamaño real (manzanas, calles, edificios y andenes).

| Pueblo | `origin` | `blocks` | Tamaño |
|------|----------|----------|------|
| Monte Nievas | `(-2450, -1950)` | 4 x 3 | pequeño, y el jugador empieza aquí |
| Eduardo Castex | `(-4250, -2721)` | 5 x 4 | pequeño |
| Metileo | `(288, -544)` | 3 x 3 | un apeadero: un tercio de sus lotes construidos, sin comisaría |
| General Pico | `(2664, 1250)` | 8 x 8 | muy grande (ver "Tamaño del pueblo"); la línea pasa por su centro |
| Caleufú | `(-7626, 2100)` | 4 x 4 | pequeño, construido en unos dos tercios (`density: Some(0.6)`); el final del ramal desde Metileo |

De Castex a Monte Nievas hay unos 12.6 km al este y 5.4 km al norte en la
pampa real; de Monte Nievas a General Pico, unos 35.8 km al este y
22.4 km al norte. Monte Nievas es el ancla, en `(-2450, -1950)` (600 m al este
de donde estaba a escala 1:6, para dejar lugar a pueblos más al oeste), y los
demás se ubican a partir de él con los desplazamientos reales divididos por 7
y redondeados al metro:

- Castex: `(-2450 - 12600 / 7, -1950 - 5400 / 7)` = `(-2450 - 1800, -1950 - 771.4)` = `(-4250, -2721)`,
  1.8 km al oeste y 0.77 km al sur de Monte Nievas (1.96 km entre sus centros).
- Metileo, entre ambos sobre la línea (estación de 1906), se ubica a partir de
  Pico: su estación (-35.774, -63.941) está unos 16.63 km al oeste y 12.56 km
  al sur de la plaza de Pico (-35.659, -63.758; OpenStreetMap y es.wikipedia),
  así que
  `(2664 - 16630 / 7, 1250 - 12560 / 7)` = `(2664 - 2375.7, 1250 - 1794.3)` = `(288, -544)`,
  a unos 260 m de la cuerda recta Monte Nievas–Pico (la línea se curva para
  pasar por su estación).
- Pico: `(-2450 + 35800 / 7, -1950 + 22400 / 7)` = `(-2450 + 5114.3, -1950 + 3200)` = `(2664, 1250)`,
  5.11 km al este y 3.2 km al norte de él (6.03 km entre sus centros).
- Caleufú (fundado el 29 de octubre de 1911 sobre el ramal Metileo–Arizona,
  no sobre la línea principal) queda unos 41 km al noroeste de Castex, así que
  se ubica a partir de Castex: `(-4250 - 3376, -2721 + 4821)` = `(-7626, 2100)`,
  a 5.89 km a escala 1:7 y 774 m hacia adentro del borde oeste del mundo; la
  prolongación oeste de su vía termina 269 m hacia adentro de ese borde.

Monte Nievas, Castex, Metileo y Caleufú tienen el ferrocarril en su lado
norte; General Pico creció a ambos lados de él, así que su línea pasa por el
centro del pueblo (`through: true`). La cuadrícula del Pico real está girada
45° y su línea corre a lo largo de ella, paralela a las calles impares; el
juego mantiene la cuadrícula alineada con el mapa y hace pasar la línea de
este a oeste a través de ella. Monte Nievas y Metileo no tienen comisaría
(`police_stations: Some(0)`).

### El mundo

| Campo | Significado | Reglas |
|-------|---------|-------|
| `half_extent` | Distancia del centro del mundo a cada borde, en metros. El mundo es un cuadrado de lado `2 * half_extent`. | Mayor que 0, como máximo 8400 (las posiciones son `f32`, con precisión de alrededor de un milímetro en cualquier punto por debajo de 16 km). Opcional, `4000.0` por defecto. |
| `chunk_size` | Lado de los chunks cuadrados en los que se construye el suelo alrededor del jinete, en metros. | Como mínimo 50, como máximo el lado del mundo. Opcional, `200.0` por defecto. |

Toda la sección `world` es opcional: un archivo con solo `towns: [...]`
recibe el mundo por defecto de 8 km x 8 km. Una sección no válida se informa
y se usa el mundo por defecto.

### Pueblos

El juego construye **todos los pueblos** de la lista, cada uno en su propio
`origin`. El jugador empieza en el primero, sobre el andén de su estación (en
el camino de entrada al pueblo cuando no tiene ferrocarril); los demás
aparecen (con sus carteles y su gente) a medida que se cabalga hacia ellos.
Los caminos entre pueblos llegarán más adelante; la línea del ferrocarril ya
los une (ver "La línea del ferrocarril").

Los pueblos deben mantenerse separados: un pueblo, su franja nivelada (60 m
alrededor de todo lo construido) y su ferrocarril (que se extiende 400 m más
allá de cada extremo de la cuadrícula) no deben tocar otro pueblo ni su franja
nivelada, y deben quedar dentro del mundo. Un pueblo que no cumple esto se
omite con un aviso que lo nombra a él y al pueblo con el que se superpone;
ganan los pueblos listados antes.

| Campo | Significado | Reglas |
|-------|---------|-------|
| `name` | El nombre del pueblo. Se pinta en el cartel de la estación, en mayúsculas. | No puede estar en blanco. Usar solo letras que tenga la fuente pixelada (ver `assets/dialogue/README.md`). |
| `origin` | Centro de la cuadrícula de calles, en metros: `(east, north)` desde el centro del mundo. | Opcional, `(0.0, 0.0)` por defecto. |
| `blocks` | Manzanas `(east-west, north-south)`: columnas y filas de la cuadrícula. | Como mínimo 3 en cada sentido. |
| `block_size` | Lado de una manzana cuadrada, en metros. | Como mínimo 30. Los edificios y los lotes crecen con la manzana. |
| `street_width` | Ancho de cada calle, incluida la calle de circunvalación, en metros. | Como mínimo 10. Las calles angostas tienen una galería del almacén menos profunda. |
| `railway` | `Some((side: North, offset: 24.0))`: el lado de la cuadrícula a lo largo del cual corre la vía (`North`, `South`, `East`, `West`) y su distancia al borde de la cuadrícula, en metros. `None` para un pueblo sin ferrocarril (sin estación, y el tren de la línea nunca para allí). Agregar `through: true` para un pueblo que creció a ambos lados de su línea (ver "Una línea a través del pueblo"). | Opcional, `None` por defecto. Desplazamiento de como mínimo 24 (lugar para el playón de la estación). |
| `density` | `Some(0.3)`: la proporción de lotes de casas y galpones que están construidos, de 0 a 1, para una aldea o un apeadero. | Opcional, `None` (cada lote como de costumbre) por defecto. |
| `plaza` | Qué manzana es la plaza (ver más abajo). | Opcional, `Auto` por defecto. |
| `seed` | Varía qué lotes tienen casas o galpones. | Opcional, `0` por defecto. |
| `police_stations` | Cuántas comisarías ("COMISARÍA") tiene el pueblo, en lugar de la cantidad de su categoría de tamaño (ver las categorías de tamaño más abajo). `Some(0)` deja el pueblo sin ninguna; su manzana recibe casas en su lugar. Monte Nievas viene con `Some(0)`: el encargo manda al jugador a Castex. | Opcional, `None` (la cantidad de la categoría) por defecto. Como máximo la cantidad de la categoría: 1 para un pueblo pequeño o grande, 2 para uno muy grande. Más se rechaza. |
| `townsfolk` | Cuántas personas caminan por las calles del pueblo, en lugar de la cantidad de gente de su categoría de tamaño (ver las categorías de tamaño más abajo). `Some(0)` deja las calles vacías. Cada pueblo incluido fija unos seis décimos de la cantidad de su categoría, para cuidar el presupuesto del frame: 10 en cada pueblo pequeño (en lugar de 16), 31 en General Pico (en lugar de 51). | Opcional, `None` (la cantidad de la categoría) por defecto. Como máximo 200: más se ignora con un aviso, y se usa la cantidad de la categoría. |

Todo el pueblo, más su franja de 60 m de terreno nivelado y su ferrocarril,
debe entrar dentro del mundo (ver arriba).

### Agregar otro pueblo

Agregar otra entrada a `towns`, con un `origin` lo bastante lejos de los
demás. Por ejemplo, un pueblo 3 km al este y 2 km al sur del centro del mundo,
con el ferrocarril en su lado sur:

```ron
        (
            name: "Winifreda",
            origin: (3000.0, -2000.0),
            blocks: (3, 4),
            block_size: 36.0,
            street_width: 11.0,
            railway: Some((side: South, offset: 26.0)),
            plaza: Auto,
            seed: 11,
        ),
```

Dejar lugar para la prolongación de 400 m de cada ferrocarril: los pueblos
alineados a lo largo de sus vías necesitan más espacio entre ellos que los
pueblos ubicados uno al lado del otro en sentido transversal. Si dos pueblos
están demasiado cerca, el aviso nombra a ambos.

Un pueblo con ferrocarril también debe agregarse a `line` o a un ramal (ver
más abajo), por ejemplo
después de General Pico: `line: Some(["Eduardo Castex", "Monte Nievas", "Metileo",
"General Pico", "Winifreda"])`.

### La línea del ferrocarril

`line` enumera los pueblos ferroviarios en el orden en que una línea de
ferrocarril pasa por ellos. Dentro de un pueblo, la vía corre recta junto a
la estación; entre dos pueblos listados uno junto al otro, un tramo (en
cualquier ángulo) une el extremo de la vía de un pueblo más cercano al otro
pueblo con el extremo de la vía del otro pueblo más cercano al primero. El
tramo sale de la vía de cada pueblo en su misma dirección y se dobla con
curvas amplias de 500 m, que solo se cierran cuando los pueblos están
demasiado cerca para ellas.

| Campo | Significado | Reglas |
|-------|---------|-------|
| `line` | `Some(["Eduardo Castex", "Monte Nievas", "Metileo", "General Pico"])`: nombres de pueblos en el orden de la línea. | Opcional. Sin él, la línea pasa por todos los pueblos ferroviarios de oeste a este, según la coordenada este del centro de cada pueblo, que es su `origin` (en caso de empate, ganan los pueblos listados primero). |

La línea es una regla del mundo:

- Todo pueblo con ferrocarril debe estar en ella, y cada nombre debe ser un
  pueblo del archivo con ferrocarril, listado una sola vez.
- Un tramo no debe cruzar ni tocar la franja nivelada de otro pueblo (60 m
  alrededor de todo lo construido), y debe quedar dentro del mundo.

Un nombre que no cumple esto queda fuera de la línea, y un tramo que no lo
cumple no se construye; cada uno recibe un aviso que nombra los pueblos. Los
pueblos en sí se construyen igual.

Un tren recorre toda la línea, del primer pueblo listado al último y de
vuelta, parando en cada estación del camino. Es un tren mixto rural de 8
vagones (93 m): la locomotora, dos coches de pasajeros y cinco vagones de
carga, y se detiene con el centro de sus coches en cada estación. Su horario
está en el código (`pampa_core::Timetable`), no en este archivo. Desde un
cambio del usuario del 2026-10-21, pasa la noche en el último pueblo (General
Pico), sale de allí a las 07:00, hace su espera en la terminal en el primer
pueblo (Eduardo Castex), sale de allí a las 16:00 y vuelve al último pueblo
para pasar la noche. (Antes dormía en el primer pueblo y salía de allí a las
07:00). Alcanza como máximo 65 km/h y se detiene 25 s en cada estación
intermedia de un recorrido. El juego empieza a media mañana con el tren de la
mañana, desde el último pueblo en su viaje ascendente, detenido en el primer
pueblo de la lista `towns`, donde el jugador empieza a pie sobre el andén, a
mitad de su largo, junto a los coches. El andén es un piso elevado con una
rampa corta en cada extremo, no un obstáculo. El tren llega hasta donde la
línea no está cortada: un tramo dejado afuera termina su recorrido en el
pueblo anterior, y una línea con menos de dos estaciones no tiene tren.

### Ramales

`branches` enumera los ramales que salen de la línea. Cada uno es una lista de
nombres de pueblos: un pueblo de `line` (el empalme), y luego los pueblos
hasta los que llega el ramal, en orden. El primer tramo sale de la vía del
empalme por su extremo más cercano al pueblo siguiente, que puede ser el mismo
extremo por el que sale la línea: las dos vías se separan allí, cada una en su
propia curva amplia, como un desvío. El archivo incluido tiene uno, el ramal
Metileo–Arizona desde el extremo oeste de Metileo hasta Caleufú.

| Campo | Significado | Reglas |
|-------|---------|-------|
| `branches` | `[["Metileo", "Caleufú"]]`: un pueblo de empalme sobre la línea, y luego los pueblos del ramal. | Opcional, vacío por defecto. |

- El empalme debe estar sobre la línea, y cada uno de los demás nombres debe
  ser un pueblo con ferrocarril que no esté ni en la línea ni en otro ramal.
  Un ramal sin un empalme válido o sin ningún pueblo más allá de él queda
  afuera, con un aviso.
- Sus tramos siguen las reglas de la línea (sin tocar la franja de otro
  pueblo, dentro del mundo).
- Cada pueblo de un ramal recibe un camino hasta su pueblo más cercano (el de
  Caleufú va a Eduardo Castex), con pasos a nivel dondequiera que se encuentre
  con una vía.

Todavía no circula ningún tren por un ramal: el tren de la línea se mantiene
en la línea.

### Una línea a través del pueblo

`railway: Some((side: North, offset: 24.0, through: true))` hace pasar la vía
de este a oeste por el centro del pueblo en lugar de a lo largo de un lado,
como los grandes pueblos ferroviarios que crecieron a ambos lados de su
línea:

- La cuadrícula se divide entre sus dos filas centrales de manzanas (la mitad
  sur recibe menos filas si la cantidad es impar). Cada mitad conserva su
  propia calle de circunvalación, y entre ellas queda una franja de
  `offset + 16` metros de ancho: la vía corre a `offset` metros del borde de
  la mitad indicada por `side` (`North` o `South`), y a 16 metros de la otra.
- La estación, su andén, los depósitos de granos, el molino y el galpón están
  en la franja, en el centro del pueblo, con la estación de frente a su mitad
  al otro lado de la calle de circunvalación.
- Cada calle norte-sur que se mantiene a 2 m del playón de la estación cruza
  la franja por un paso a nivel, con una plataforma de tablones y una cruz de
  San Andrés a cada lado, y los peatones también cruzan por allí. Las calles
  que entrarían al playón terminan en la franja a cada lado.
- `plaza: Auto` toma la manzana una columna al oeste del centro y dos filas
  hacia atrás desde la vía en la mitad de la estación (la plaza de 2 x 2 de un
  pueblo muy grande la cubre): unas cuadras al noroeste de la estación cuando
  `side` es `North`. Una plaza que abarcaría la vía es un error.
- El pueblo no tiene huella rural: sus calles cruzan la línea. Un camino entre
  pueblos nunca sale de él hacia el este ni hacia el oeste, a lo largo de la
  línea.
- Necesita al menos 4 filas de manzanas, y `side` `North` o `South`.

### Lagunas

`lagoons` enumera las lagunas de agua dulce poco profundas de la llanura:

| Campo | Significado | Reglas |
|-------|---------|-------|
| `name` | Su nombre, para los avisos. | |
| `center` | Centro del agua, `(east, north)` en metros. | |
| `size` | Tamaño del rectángulo redondeado que sigue su orilla, `(east-west, north-south)` en metros. | Cada uno entre 40 y 1500. |
| `seed` | Varía la orilla. | Opcional, `0` por defecto. |

La orilla es un rectángulo redondeado (una superelipse, casi cuadrada con
esquinas suaves), irregularizado hasta un 5%. El agua queda 0.6 m por debajo
del punto más bajo de su ribera de 30 m, y el fondo desciende hasta 1.6 m. Las
personas y los caballos vadean los primeros 5 m de agua y se los hace volver
donde se vuelve más profunda. Los alambrados y los asentamientos se mantienen
a 15 m más allá de la ribera; los árboles quedan a 12 m del agua, y el pasto,
las piedras y los arbustos crecen hasta los 3 m de barro húmedo a lo largo de
la línea del agua. Una laguna cuya ribera (más ese margen) toca la franja
nivelada de un pueblo, el ferrocarril o un camino, o sale del mundo, queda
afuera con un aviso. Toda la lista es opcional.

Cada laguna recibe un camino público desde el pueblo más cercano a menos de
2.5 km de ella: un camino de tierra sin salida entre dos alambrados, como los
caminos entre pueblos. Sale del pueblo por la calle que atraviesa el lado que
da a la laguna y que se mantiene a 40 m de la vía, sigue una línea de curvas
suaves, y termina en la ribera en un retorno de 6 m de diámetro cuyo borde
llega hasta el barro húmedo y no más allá. Un cartel indicador en su inicio
nombra la laguna. El camino es lo único que se coloca sobre la ribera y su
margen, y solo en su extremo.

La Laguna La Arocena incluida está realmente a 5.9 km con rumbo de 119° desde
General Pico (5.16 km al este, 2.86 km al sur), a escala 1:7: `(2664 + 5160 / 7, 1250 -
2860 / 7)` = `(3401, 841)`. En la realidad cubre unas 150 ha; el juego conserva
sus proporciones con 215 x 145 m.

### Ubicación de la plaza

- `Auto`: la manzana central cuando ambas cantidades de manzanas son impares.
  Si no, la manzana **frente a la estación**, como en muchos pueblos
  ferroviarios de la pampa. Un pueblo sin ferrocarril y sin manzana central
  toma la manzana más cercana al centro.
- `Center`: la manzana central. Es un error en una cuadrícula con una
  cantidad par.
- `FacingStation`: la manzana de la fila (o columna) junto al ferrocarril más
  cercana al centro (los empates van al oeste o al sur). La estación se mueve
  a lo largo de la vía para quedar frente a la plaza, al otro lado de la
  calle. Necesita un ferrocarril.
- `Block(i, j)`: la manzana de la columna `i`, fila `j`, contadas desde 0 en
  la esquina suroeste.

La iglesia da a la plaza desde la manzana al norte de ella (o desde la
siguiente vecina libre: sur, este, oeste; la manzana libre más cercana para
una plaza en una esquina). Los almacenes de ramos generales y las comisarías
se reparten por el pueblo para que tenga vida en todas partes (usuario,
2026-10-23): ningún par de estos edificios (incluida la iglesia) está en
manzanas vecinas, dos del mismo tipo están separados por alrededor de la
mitad del pueblo (la mitad de su lado más largo en manzanas, al menos dos), y
dentro de eso cada uno toma la manzana libre más cercana a la plaza (luego la
columna más baja, luego la fila), mirando hacia ella. En un pueblo que creció
a ambos lados de su línea, cada uno va a la mitad con menos edificios de su
tipo, para que cada mitad tenga su propio almacén y su propia comisaría.
Cuando ninguna manzana mantiene esa separación (una cuadrícula diminuta), el
edificio toma la más alejada de los demás.

El almacén de un pueblo pequeño es una pulpería: su cartel dice "PULPERÍA". El
de un pueblo grande o muy grande dice "ALMACÉN DE RAMOS GENERALES". (En el
minimapa ambos se marcan con "A", y la plaza con "P").

### Tamaño del pueblo

El tamaño de un pueblo sale de su cantidad de manzanas, `n` = columnas por
filas:

| | Pequeño (`n` hasta 36) | Grande (37 a 63) | Muy grande (64 o más) |
|---|---|---|---|
| Plaza principal | 1 manzana, ubicada según `plaza` | 2 manzanas unidas, centrada | 2 x 2 manzanas unidas, centrada |
| Plaza de la estación | no | sí | sí |
| Almacenes de ramos generales | 1 | 2 | 2 |
| Comisarías | 1 | 1 | 2 |
| Personas que caminan (salvo que `townsfolk` lo fije) | 16 | `0.8 n`, redondeado, al menos 16 | igual |

- Una plaza unida incluye las calles entre sus manzanas: no hay camino que la
  atraviese y los peatones la rodean. Un pueblo grande une dos manzanas a lo
  largo de su lado más largo (de este a oeste en una cuadrícula cuadrada).
  `Auto` y `Center` centran la plaza en la cuadrícula (los empates van al
  oeste o al sur); `Block` y `FacingStation` mantienen la manzana elegida
  dentro de ella.
- La plaza de la estación es media manzana. Con ferrocarril, toma la mitad
  más cercana a la vía de la manzana, en la fila junto al ferrocarril, que da
  a la estación; cuando esa manzana está ocupada, la manzana libre más
  cercana a lo largo de esa fila. La otra mitad de su manzana conserva casas.
  Sin ferrocarril, toma la mitad de la manzana libre más alejada de la plaza
  principal.
- Un almacén de ramos generales da a la plaza de la estación; el otro se
  reparte por el pueblo junto con las comisarías (ver arriba). El caballo
  atado queda en ese otro, el primer almacén de ramos generales del pueblo.
- `police_stations` puede bajar la cantidad de comisarías de la categoría
  (hasta ninguna), nunca subirla. Un pueblo sin el campo conserva la cantidad
  de la categoría y se dispone exactamente como antes.
- `townsfolk` reemplaza la cantidad de gente de la categoría, hacia arriba o
  hacia abajo. Un pueblo sin el campo conserva la cantidad de gente de la
  categoría.

### Cuando algo está mal

Si el archivo falta o no se puede leer, o un pueblo no cumple una regla de
arriba, el juego arranca igual: registra un aviso que nombra el pueblo y el
problema, omite ese pueblo, y usa el Eduardo Castex incorporado cuando no
queda ningún pueblo válido.
Un `townsfolk` mayor que 200 no omite el pueblo: se registra y en su lugar
camina por allí la cantidad de gente de la categoría.
Ejecutar `cargo test -p pampa-content` para comprobar el archivo incluido
antes de jugar.

Los campos que el juego todavía no conoce se ignoran, así que se pueden
agregar antes que el código que los usa.
