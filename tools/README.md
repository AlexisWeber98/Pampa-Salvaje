# Herramientas

Pequeños scripts para preparar assets. No forman parte de la compilación del juego.

## `textures_to_ktx2.sh`: texturas de edificios a KTX2

Convierte los conjuntos de texturas de edificios de `assets/textures/buildings/<set>/` en los archivos KTX2 comprimidos para la GPU que carga el juego. Necesita `basisu` (Basis Universal 2.x) e ImageMagick 7 (`magick`), y es `sh` POSIX:

```sh
FORCE=1 sh tools/textures_to_ktx2.sh assets/textures/buildings/*/
sh tools/textures_to_ktx2.sh --check-orm
sh tools/textures_to_ktx2.sh --check-alpha
```

A partir de `color.jpg`, `normal_gl.jpg`, `roughness.jpg` y el `ao.jpg` opcional de cada conjunto, escribe:

| Archivo | Contenido |
|---|---|
| `color.ktx2` | color sRGB; con un `opacity.jpg` (los atlas de hojas de los árboles), RGBA con la opacidad en el alfa |
| `normal_gl.ktx2` | normal map OpenGL (+Y), lineal, con el modo de normal maps de basisu |
| `orm.ktx2` | lineal; rojo = oclusión ambiental (blanco sin `ao.jpg`), verde = rugosidad, azul = metalicidad (blanco; el `metallic` del material lo escala) |

Los tres son UASTC LDR 4x4 con mipmaps incorporados y supercompresión Zstandard. El color de un atlas de hojas toma sus mips de `leaf_atlas.py --mips` (más abajo), así que el script también necesita `python3` para los conjuntos con `opacity.jpg`; todos los demás conjuntos se codifican exactamente igual que antes. Bevy transcodifica UASTC al cargar al formato de bloques de la GPU (BC7 en GPUs de escritorio, ASTC o ETC2 en otras): 1 byte por texel en memoria de video en lugar de los 4 de RGBA8. No se usa RDO: en estos conjuntos ahorraba alrededor de un 3% de disco a cambio de alrededor de 1 dB de PSNR.

Sin `FORCE=1` se niega a sobrescribir archivos `.ktx2` existentes. Los JPG siguen siendo las fuentes editables; hay que volver a ejecutarlo después de cambiar uno. `--check-orm` convierte un conjunto sintético con y sin `ao.jpg`, desempaqueta su `orm.ktx2` a BC7 y falla salvo que cada canal contenga lo que debe. `--check-alpha` hace lo mismo para un conjunto con `opacity.jpg` y falla salvo que la opacidad termine en el alfa de `color.ktx2`.

Las texturas del suelo no se convierten: el juego mezcla los mapas PNM en una única textura de detalle en la CPU al inicio, y no tiene un codificador para comprimir el resultado.

## `leaf_atlas.py`: atlas de tarjetas de hojas para los árboles de la plaza

Compone los atlas de tarjetas de hojas de las cuatro especies de árboles de la plaza a partir de las hojas de muestras de ambientCG en `assets/textures/trees/`. Una hoja de muestras contiene hojas sueltas fotografiadas planas; el script recorta cada hoja según su mapa de opacidad y dispone las hojas a lo largo de tallos dibujados en cuatro ramitas, una por cuarto de un atlas de 1024². Necesita ImageMagick 7 (`magick`) y solo la biblioteca estándar:

```sh
python3 -I tools/leaf_atlas.py assets/textures/trees
FORCE=1 sh tools/textures_to_ktx2.sh assets/textures/trees/*_leaves/
```

| Atlas | Fuente | Ramita |
|---|---|---|
| `paraiso_leaves` | LeafSet013 | folíolos en pares opuestos a lo largo de un raquis (la hoja pinnada que no tiene ningún conjunto escaneado) |
| `olmo_leaves` | LeafSet001 | hojas alternadas a lo largo de una ramita |
| `eucalipto_leaves` | LeafSet003 | hojas largas que cuelgan pegadas a una ramita |
| `aromo_leaves` | LeafSet019 | los ramilletes, puestos en posición vertical |

Cada carpeta de atlas recibe `color.jpg`, `opacity.jpg`, `normal_gl.jpg` y `roughness.jpg`; `textures_to_ktx2.sh` empaqueta la opacidad en el alfa del color. Las componentes x e y del normal map se rotan con cada hoja. La disposición usa una semilla, así que la salida es reproducible con el mismo ImageMagick.

`python3 -I tools/leaf_atlas.py --mips RGBA.png OUT.dds` (que ejecuta `textures_to_ktx2.sh`) escribe la cadena de mips de una imagen RGBA como un DDS RGBA con mipmaps, que basisu codifica nivel por nivel. Los mips comunes promedian el alfa de una hoja delgada con el fondo transparente que la rodea, por debajo del umbral de alpha test de 0.5 del juego, así que las copas lejanas se ralean. En cambio, el alfa de cada nivel se reasigna para que dibuje la misma proporción de texels que dibuja el atlas a tamaño completo (siguiendo los mips que conservan la cobertura de Castaño), manteniéndose a 8 pasos del umbral por la compresión en bloques. Un test desempaqueta los atlas incluidos a BC7 y comprueba cada nivel de 8 x 8 texels o más. Tests: `python3 -I -B -m unittest discover -s tools -p test_leaf_atlas.py`.

## `dae2gltf.py`: de COLLADA a glTF

Convierte un modelo COLLADA (`.dae`) en una ranura de modelo glTF 2.0 (ver `assets/models/third-party/README.md`). Usa solo la biblioteca estándar de Python 3. Hay que ejecutarlo aislado, para que no se importe nada que esté junto a la entrada o en la carpeta actual:

```sh
python3 -I tools/dae2gltf.py INPUT.dae -o OUTPUT_FOLDER [options]
python3 -I tools/dae2gltf.py --check OUTPUT_FOLDER/model.gltf
```

Escribe `model.gltf`, `model.bin` y una carpeta `textures/` en la carpeta de salida, y luego imprime la cantidad de triángulos, los límites en metros, el pivote y el eje del rotor, y lo que hizo con cada textura.

| Opción | Significado |
|---|---|
| `--rotor NODE` | Mantiene aparte la geometría bajo este nodo COLLADA (nombre o id), como un nodo `rotor` con su origen en el pivote del nodo. Su eje de giro sale de la rotación animada del nodo; hay que ponerlo en `slots.ron`. |
| `--rotor-axis X,Y,Z` | El eje de giro (espacio glTF) cuando el rotor no está animado. |
| `--rotor-name NAME` | El nombre del nodo glTF del rotor (por defecto `rotor`; `body` se rechaza, porque nombra al nodo estático). |
| `--scale S` | Una escala uniforme adicional. |
| `--metallic M` | Factor de metalicidad de todos los materiales (por defecto 0). |
| `--normal auto\|gl\|dx` | Convención del verde de los normal maps. `auto` (por defecto) decide por mapa: las normales de un campo de alturas real tienen rotacional nulo solo para un signo del verde. Los mapas DirectX reciben el verde invertido, ya que glTF requiere OpenGL. `gl` copia los mapas sin decodificarlos; `dx` invierte todos los mapas (solo PNG). El informe nombra la convención aplicada a cada mapa y si se detectó, se forzó o se supuso (un normal map JPEG no se inspecciona y se toma como GL). |
| `--single-sided` | Descarta las caras traseras (por defecto los materiales son de doble cara: los modelos de juegos suelen tener planos simples). |
| `--check FILE` | Solo valida un glTF escrito: buffers, vistas, accessors, índices e imágenes dentro de rango (un índice negativo o ausente es una falla), cada URI dentro de la carpeta del modelo, y cada imagen un PNG o JPEG por tipo, extensión y contenido. |

Lo que lee:

- `<triangles>`, `<polylist>` y `<polygons>`, convertidos en abanicos de triángulos; las líneas y los huecos se omiten;
- posiciones, normales (o normales de cara cuando faltan) y el primer conjunto de texturas enlazado;
- transformaciones de nodos (`matrix`, `translate`, `rotate`, `scale`), incorporadas a los vértices; una escala negativa invierte el orden de los vértices;
- `<up_axis>` y `<unit meter>`, a metros con Y hacia arriba;
- por material: la textura o el color difuso, el normal map (una textura `<bump>`, o si no un `<diffuse>_NRM.png` a su lado), y una rugosidad a partir del brillo. Los mapas especulares se ignoran.

La entrada se trata como no confiable. Se rechaza un archivo de más de 64 MB, un documento con un DTD o entidades, un número no finito, un índice fuera de rango o un arreglo demasiado grande. El rechazo del DTD es una búsqueda de bytes de `<!DOCTYPE` y `<!ENTITY`, así que primero se rechaza un documento que no sea UTF-8 (un archivo UTF-16 o UTF-32, o cualquier otra codificación declarada): esos bytes podrían esconderse de la búsqueda. Las rutas de las texturas se reducen a su nombre base y se buscan solo en la carpeta propia del modelo: primero el nombre exacto, y si no, una única coincidencia sin distinguir mayúsculas (varias se rechazan por ambiguas). Las texturas deben ser PNG o JPEG, los únicos formatos que permite el glTF básico. No se evalúa nada. Una entrada no válida recibe un error de una línea; cualquier otra excepción es un bug del script y conserva su traceback.

Los límites `min`/`max` de los accessors son los valores float32 realmente almacenados, para que los validadores los vean coincidir exactamente con el buffer.

### Tests

Una suite `unittest` de la biblioteca estándar construye pequeños documentos COLLADA y PNG en una carpeta temporal y cubre la conversión de ejes y unidades, los triángulos y las polylists, la separación del rotor, la convención de los normal maps, los rechazos y `--check`. `test_mh2gltf.py` hace lo mismo para `mh2gltf.py` (lectura de `.obj` y `.mhclo`, carpetas de datos, materiales, decimación, las prendas recortadas del cuerpo, los sombreros y las demás piezas generadas, las texturas de tela, la pose de los brazos) y convierte a Farach cuando los datos de MakeHuman están instalados:

```sh
python3 -I -B -m unittest discover -s tools -v
```

Las conversiones hechas hasta ahora están listadas en `assets/models/third-party/3td-starter-pack/CREDITS.md`.

## `mh2gltf.py`: de personaje de MakeHuman a glTF con skinning

Convierte un personaje de MakeHuman (`.mhm`) en un modelo glTF 2.0 con skinning (ver `assets/characters/README.md`). Necesita Python 3 con numpy y la carpeta *data* de MakeHuman 1.x: `--data`, si no `$MAKEHUMAN_DATA`, si no `~/.local/opt/makehuman-1.3.0/makehuman/data`. Los assets también se buscan en la carpeta de datos de usuario de MakeHuman, donde van los paquetes de assets descargados, y allí primero, como hace MakeHuman: `--user-data`, si no `$MAKEHUMAN_USER_DATA`, si no `~/Documents/makehuman/v1py3/data`, cuando existe (`--no-user-data` la omite). Solo se leen los datos; no se ejecuta nada del código de MakeHuman y su GUI nunca se inicia. Hay que ejecutarlo aislado:

```sh
python3 -I tools/mh2gltf.py INPUT.mhm -o OUTPUT_FOLDER [options]
python3 -I tools/mh2gltf.py --check OUTPUT_FOLDER/model.gltf
```

Escribe `model.gltf`, `model.bin` y `textures/` (PNG). Las unidades son metros, Y va hacia arriba, el modelo mira hacia +Z y su punto más bajo está en y = 0. Hay un único nodo de mesh con skinning, con una primitiva por proxy, y la jerarquía de articulaciones del esqueleto. Imprime los triángulos y los límites de cada primitiva, las texturas que escribió, y todo lo que tuvo que sortear.

| Opción | Significado |
|---|---|
| `--data DIR` | La carpeta de datos de MakeHuman |
| `--user-data DIR` | La carpeta de datos de usuario de MakeHuman, en la que se busca primero |
| `--no-user-data` | Busca solo en la carpeta de datos |
| `--max-texture N` | Lado más grande de la piel y de las prendas que cubren el cuerpo (por defecto 1024) |
| `--max-small-texture N` | Lado más grande de las demás texturas y de los normal maps (por defecto 512) |
| `--no-normal-maps` | Deja afuera los normal maps |
| `--keep-hair-under-hats` | Conserva el pelo por encima del ala de un sombrero |
| `--check FILE` | Solo valida un glTF escrito. Comprueba lo mismo que `dae2gltf.py --check`, y además el skin: cada articulación es un nodo, hay una matriz de bind inversa invertible por articulación, y cada vértice tiene articulaciones dentro de rango y pesos no negativos que suman 1 |

Lo que reproduce de las fuentes de MakeHuman (`apps/humanmodifier.py`, `apps/human.py`, `lib/targets.py`, `shared/proxy.py`, `shared/skeleton.py`, `shared/animation.py`):

- **Morphing.** La mesh base se deforma con cada modificador: los valores del `.mhm` y los valores por defecto del resto. Los modificadores macro (género, edad, etnias, músculo, peso, altura, proporciones, busto) pasan por la interpolación de las variables macro y los factores de cada target. Los demás usan sus targets decr/incr.
- **Proxies.** El proxy del cuerpo (se requiere una línea `proxy`, ya que la mesh base es demasiado densa para un juego) y los ojos, las cejas, las pestañas, el pelo y la ropa se ajustan a la mesh deformada mediante sus vértices de referencia, pesos, desplazamientos y matriz de escala. Un proxy se lee del `.mhpxy` compilado de MakeHuman o, para un asset que MakeHuman todavía no cargó, de su texto `.mhclo`; su mesh, del `.npz` compilado o, si no, de su `.obj` (un triángulo repite su primera esquina, como en MakeHuman; los polígonos más grandes se convierten en abanicos). El ajuste con cizallamiento no está soportado.
- **Ocultamiento de caras.** Con `clothesHideFaces`, los `deleteVerts` de la ropa ocultan lo que hay debajo. La capa más externa va primero, según `z_depth`. Las caras ocultas no se escriben.
- **Esqueleto.** El esqueleto sale de la línea `skeleton` (`game_engine.mhskel`). Las articulaciones se ubican en sus vértices de referencia. La orientación de los huesos se copia de las normales de plano del esqueleto por defecto. Los pesos salen de `default_weights.mhw`, a través de las referencias de peso de los huesos (completadas como lo hace `autoBuildWeightReferences`), y luego a través de los vértices de referencia de cada proxy, o del `.mhw` propio de un proxy (los zapatos). Cada vértice conserva como máximo 4 influencias, normalizadas.
- **Materiales.** De cada `.mhmat` toma la textura difusa y `diffuseColor`, el normal map, `transparent` (máscara de alfa en 0.5), `backfaceCull` (doble cara), y una rugosidad a partir del `shininess` Phong de MakeHuman ponderado por la intensidad de su `specularColor`: `1 - shininess * min(1, 4 * specular)`, limitada a 0.35–0.95. La tela y el fieltro tienen un especular tenue (el 0.04 del fedora), así que se ven mate en lugar de como metal pulido. Una línea `roughness` o `metallic` explícita tiene prioridad. Los colores (`diffuseColor`, y los colores `piece` del `.mhm`) son colores de pantalla (sRGB), como los muestra MakeHuman; se escriben en el `baseColorFactor` de glTF, que es lineal, a través de la curva de sRGB a lineal, o un traje oscuro se renderizaría gris claro.

Lo que no es de MakeHuman, cada cosa activada por una línea del `.mhm` o del `.mhmat` que MakeHuman omite (ver `assets/characters/README.md`):

- **Pelo bajo los sombreros.** El pelo por encima de la cinta de un sombrero o de un kepí generados se deja afuera, para que no atraviese la copa. `--keep-hair-under-hats` lo desactiva.
- **Decimación.** `decimate` simplifica una prenda densa después de ajustarla y aplicarle el skinning, por colapso de aristas cuádrico (Garland y Heckbert). Un vértice se fusiona con un vecino y conserva la posición y la UV de ese vecino, así que no se interpola ninguna UV. Los vértices sobre un borde abierto o una costura de UV nunca se mueven, lo que conserva exactamente el contorno y las costuras de la textura (y es lo que detiene una prenda por encima de su objetivo). Se rechaza un colapso que daría vuelta un triángulo o pellizcaría la superficie. El vértice que sobrevive toma los pesos de ambos vértices, recortados a los 4 más fuertes y renormalizados. Primero el más barato, los empates por índice: la salida es determinista.
- **Brazos a los costados.** La pose de reposo de MakeHuman tiene los brazos abiertos en A. El conversor los baja: cada brazo gira alrededor de su hombro para colgar hacia abajo y un poco hacia afuera (`ARM_HANG`), cada antebrazo alrededor de su codo para seguirlo, un poco hacia adelante. Los huesos de abajo lo siguen y cada vértice se mueve según sus pesos; esa pose es la pose de reposo que se escribe, así que el juego no necesita ninguna corrección.
- **Texturas en escala de grises.** `greyscaleTexture True` en un `.mhmat` convierte la textura difusa a gris, estirada para que sus texels más claros sean blancos, antes de que `diffuseColor` la tiña: así se puede hacer blanco el pelo oscuro.
- **Prendas recortadas del cuerpo.** `piece <garment> key=value ...` hace una prenda a partir del propio proxy del cuerpo en pose (`male1591` o, para las vecinas, `female1605`), así que parece de 1919 en lugar de una prenda moderna de paquete y se dobla exactamente con el cuerpo:
  - *Regiones.* Cada vértice de la piel recibe un miembro según su articulación más fuerte (el tronco, el cuello y las piernas; un brazo; la cabeza, que nunca se viste) y un lugar en él: en el tronco y las piernas una *estación*, su altura en una escala que pasa por las articulaciones (0 en las plantas, 1 los tobillos, 2 las rodillas, 3 las caderas, 4 la cintura en `spine_02`, 5 los hombros, 6 el cuello, 7 la cabeza), y en un brazo un *alcance* (0 en el hombro, 1 en el codo, 2 en la muñeca). Así, los cortes siguen las proporciones propias de cada personaje.
  - *Cáscara.* Una prenda cubre las caras de la piel cuyos tres vértices caen en su rango. Las copia, empujadas hacia afuera a lo largo de las normales suaves con un desplazamiento que varía a lo largo de la estación o del alcance (`GARMENT_CUTS`: una camisa ajustada, un chaleco encima, un saco lo más afuera; pantalones ajustados debajo de la rodilla; una bombacha amplia desde la pantorrilla hasta la cadera; botas ceñidas con la caña acampanada y una suela). Cada vértice conserva los pesos y la UV de su vértice de piel (las UV repetidas para la textura de tela).
  - *Bordes.* Dondequiera que termina una prenda (un dobladillo, un puño, un cuello, una sisa), un borde va desde su orilla hasta justo por encima de la piel, para que el final nunca sea transparente.
  - *Capas.* Donde las prendas se superponen, cada cara va a la que queda más afuera, y las otras la dejan afuera (la camisa bajo el chaleco, los pantalones metidos en las botas). Las caras de la piel bajo cualquier prenda también se dejan afuera.
  - *Polleras largas.* `piece skirt` (la pollera de una mujer) es un tubo de 32 lados desde una pretina en la estación `top` (3.9) hasta un dobladillo a la altura del tobillo en `hem` (1.15), alrededor de las caderas y las piernas vestidas juntas (`make_long_skirt`). Debajo de las caderas nunca se angosta; hacia el dobladillo se redondea en forma de campana y se ensancha según `flare` (1.2), en 8 pliegues suaves de `folds` (0.08) de profundidad, y su dobladillo se dobla hacia adentro. Su skin: la pretina va con la pelvis y `spine_01` (los pesos del cuerpo debajo de ella); más abajo cada vértice toma hasta `follow` (0.8) del giro de los muslos, una proporción que crece hacia el dobladillo y se reparte entre los dos muslos según cuánto cuelga hacia cada lado, así que el centro del frente toma la mitad de cada uno; la parte de atrás del dobladillo también toma hasta `kick` (0.9) del giro de las canillas, para que un talón que sube por detrás la levante en lugar de atravesarla. La piel del tronco y de las piernas debajo de ella se deja afuera desde la estación `hide` (1.6) hacia arriba. `piece apron` pone un delantal sobre el frente de la pollera (`width` 0.8 de su mitad delantera, hasta la estación `hem` 1.7), justo por fuera de ella y con el mismo skinning. La pollera se dimensionó contra las poses propias del juego: con skinning en CPU a lo largo de los clips de caminata, espera y habla (`rig::tests::the_women_keep_their_legs_in_their_skirts_and_their_hands_out_of_them` de `pampa_render`), las piernas y las botas quedan al menos 1.2 cm por dentro y las manos 1.8 cm por fuera, con los brazos separados 0.14 rad por el juego.
  - *Faldones de saco.* Un saco con `skirt` recibe un tubo desde su cintura hasta la altura de esa estación, alrededor de las caderas y las piernas vestidas, acampanado en el dobladillo y abierto por delante según `open` sectores; va con la pelvis, y su parte inferior en parte con los muslos.

  Sus parámetros, todos opcionales: `color`; `top` y `bottom` (estaciones); `cuff` (un alcance; 0, sin mangas); `loose` (un factor sobre los desplazamientos); `vneck` y `vwidth` (una abertura en V en el frente: la estación de su punta y su medio ancho en el cuello); `skirt`, `open` y `flare` (el faldón del saco); `pad` (hombros cuadrados, con hombreras); `sole` (botas: el grosor de la suela):

  | Prenda | Textura | Valores por defecto |
  |---|---|---|
  | `shirt` (mangas hasta la muñeca, un cuello pequeño) | `cotton` | `top` 6.15, `bottom` 3.2, `cuff` 2.08 |
  | `waistcoat` (sin mangas, una V) | `wool` | `top` 6.0, `bottom` 3.6, `vneck` 5.0 |
  | `jacket` (mangas, una V, un faldón); con `vneck=0` y un `top` alto, una guerrera cerrada con un cuello alto y rígido | `wool` | `top` 6.1, `bottom` 3.9, `cuff` 2.07, `vneck` 4.7, `skirt` 2.8, `open` 1 |
  | `trousers` | `wool` | `top` 4.3, `bottom` 1.2 |
  | `bombacha` | `cotton` | `top` 4.3, `bottom` 1.8 |
  | `boots` | `leather` | `top` 2.0, `sole` 0.1 |
  | `alpargatas` (calzado de lona con suela de yute) | `cotton` | `top` 1.1, `sole` 0.15 |
  | `shawl` (chal): alrededor de los hombros por encima de la estación 5, sobre los brazos hasta `cuff`, y por la espalda hasta una punta en `point` en el centro (`shawl_cut`); sus dos extremos se generan (más abajo) | `wool` | `cuff` 0.85, `point` 3.9, `width` 1.9 (el de los extremos) |
  | `headscarf` (pañuelo de cabeza): recortado de la piel de la cabeza y de la parte superior del cuello, alrededor de un óvalo abierto para la cara (de `face` de ancho, desde el mentón hasta `brow`, una proporción de la altura de la cabeza hacia abajo desde su parte superior), separado `loose` de ella y más amplio en la coronilla (`crown`); con un nudo bajo el mentón (`knot` de ancho) y su punta doblada bajando por la nuca (`drop` de largo), con skinning de la cabeza, el cuello y el pecho | `cotton` | `loose` 0.12, `crown` 0.08, `brow` 0.3, `face` 0.5, `knot` 0.12, `drop` 1.1 |
  | `hair` recogido: recortado de la piel de la cabeza hasta una línea de nacimiento del pelo `brow` hacia abajo en el frente, las orejas y la nuca | ninguna (un color liso) | `loose` 0.05, `crown` 0.05, `brow` 0.2 |

- **Texturas de tela.** Las prendas, la faja y el poncho llevan pequeñas texturas repetibles (64 px) hechas en código, iguales en cada ejecución: `cotton` (un tejido liso fino), `wool` (una sarga), `stripe` (la sarga con rayas), `leather` (un moteado suave), todas grises para que el color las tiña, y `poncho` (rojo oscuro con guardas tejidas color crema).
- **Piezas generadas.** `piece <kind> key=value ...` agrega una pequeña mesh ajustada al cuerpo vestido. Una pieza sobre un solo hueso (los sombreros y el kepí en la cabeza, el pañuelo de cuello y el cuello de camisa en el cuello) recibe un skinning rígido a ese hueso (peso 1). Una pieza que se apoya sobre varios toma su skin del cuerpo vestido que tiene debajo, como las prendas de MakeHuman toman el del cuerpo: el poncho y la correa del hombro a la cadera, la faja y la rastra alrededor de la cintura, y los botones, la cadena del reloj y las solapas a lo largo del pecho. Cada vértice recibe la media ponderada por distancia inversa de los pesos de sus 8 puntos del cuerpo más cercanos, limitada a las articulaciones del tronco que la pieza puede seguir (`SKIN_FROM`: pelvis, columna y clavículas, nunca los brazos). Así, el poncho gira con la clavícula en el hombro y con la pelvis en la cadera. Luego el poncho se mantiene apartado del brazo sobre el que se apoya (`clear_arms`): desde la articulación del hombro hacia afuera, cada vértice a menos de 2.5 cm del brazo vestido (una cápsula desde el hombro hasta un poco más allá del codo, tan gruesa como el brazo en su parte media) se empuja directamente hacia afuera hasta esa distancia, y los vértices de allí toman hasta el 90% del peso del brazo, que se desvanece a lo largo de 15 cm, así que se apoya sobre el deltoides cuando el brazo se balancea (`ARM_CLEARANCE`, `ARM_SHARE`, `ARM_REACH`). El pecho queda como estaba, así que el despeje del pecho de los clips no cambia. Cada pieza se ajusta a un contorno de los puntos que la rodean (para cada una de 20 direcciones alrededor de un eje vertical, el punto más lejano a esa altura, más un margen) o se apoya sobre la superficie más adelantada encontrada a lo largo de z. Las longitudes están en los decímetros de MakeHuman, los colores son `r,g,b`:

  | Tipo | Articulación | Parámetros (valores por defecto) |
  |---|---|---|
  | `hat`: una copa de fieltro con la parte superior plana, un ala con grosor y el borde curvado, y una cinta (gacho, chambergo, ala ancha) | `head` | `color`, `band` (el color de la cinta); `crown` 0.25 (la parte superior por encima de la de la cabeza); `taper` 0.97 (ancho superior / ancho de la cinta); `brim` 0.75; `droop`, `back` y `sides` 0.05, 0, 0 (cuánto bajan los bordes delantero, trasero y laterales del ala; un valor negativo los sube); `dent` 0 (un pliegue a lo largo de la parte superior, de adelante hacia atrás); `pinch` 0 (angosta el frente de la copa); `round` 0 (suaviza el borde superior, hasta 1); `thick` 0.07; `curl` 0.04 (el borde levantado); `tilt` 0.15 (la cinta más baja atrás); `fit` 1.05 (la cinta por debajo de la parte superior de la cabeza). La cinta (de un sombrero o de un kepí) se ciñe al contorno de la piel del cráneo, no al del pelo |
  | `kepi`: una copa de tela con la parte superior plana un poco más ancha que su cinta, inclinada hacia el frente; una cinta de cuero, una visera rígida y un barbijo con dos botones | `head` | `color`, `visor` (el color del cuero), `button`, `crown` 0.02, `taper` 1.06, `slope` 0.3, `width` 0.6 (visera), `droop` 0.3, `tilt` 0, `fit` 0.62 |
  | `neckerchief` (pañuelo): una banda alrededor del cuello, una punta sobre el pecho, un nudo a un lado con dos extremos cortos | `neck_01` | `color`, `drop` 0.9, `knot` 0.11 |
  | `collar`: un cuello rígido de tira con pequeñas puntas de pajarita | `neck_01` | `color`, `height` 0.28, `wings` 0.12 |
  | `sash` (faja): una banda a rayas alrededor de la cintura | `pelvis` | `color`, `width` 1.1, `height` 0 |
  | `rastra`: un cinto de cuero sobre la faja, con una chapa y monedas al frente (`coins=0`: un cinto liso con hebilla) | `pelvis` | `color`, `coin` (el color de las monedas; metálico), `coins` 8, `height` 0 |
  | `buttons`: botones de bronce a lo largo del frente | `spine_02` | `color` (metálico), `count` 6, `spread` 0 (separación de las columnas; por encima de 0, dos columnas) |
  | `chain`: una cadena de reloj que cruza el chaleco | `spine_02` | `color` (metálico), `height` 0.5 (de `spine_01` a `spine_02`), `reach` 0.55, `sag` 0.18 |
  | `lapels`: las solapas de un saco a cada lado de su abertura | `spine_03` | `color`, `point` 0.3 (de `spine_02` al cuello), `open` 0.6, `width` 0.4 |
  | `strap`: una correa de cuero desde un hombro que cruza el pecho y la espalda hasta la cadera opuesta | `spine_03` | `color`, `side` -1 (1 el hombro izquierdo), `width` 0.5 |
  | `poncho`: un poncho doblado a lo largo sobre un hombro, con los extremos colgando por debajo de la cadera | `spine_03` | `color` (tiñe la textura tejida), `side` 1, `width` 1.8, `thick` 0.16, `hang` 1.4 |
  | `boina`: una copa blanda y plana que sobresale de su cinta, tirada hacia la izquierda, con un pequeño rabillo | `head` | `color`, `width` 1.3 (la copa sobre la cinta), `height` 0.18 (su parte superior por encima de la de la cabeza), `lean` 0.25, `droop` 0.25, `tilt` 0.1, `fit` 0.95 |
  | `bun` (rodete): una bola aplastada detrás de la cabeza | `head` | `color`, `size` 0.85, `high` 0.5 (de la nuca a la coronilla) |
  | los dos extremos del chal (de una línea `shawl`): bandas que salen de debajo del chal en cada hombro, cruzadas sobre el pecho hasta el otro lado de la cintura, la derecha sobre la izquierda, cada punta apoyada sobre lo que está más adelante allí | tronco y clavículas (`SKIN_FROM`) | el `color` y el `width` del chal |

Los datos son locales, pero se leen como entrada no confiable:

- Los archivos de texto y JSON tienen límites de tamaño, y los números no finitos se rechazan.
- Los archivos de numpy se cargan con `allow_pickle=False`. Se comprueban el tipo, la forma y el tamaño de cada arreglo, y también cada índice.
- Las rutas se mantienen dentro de la carpeta de datos o de la carpeta propia del `.mhm`.
- Los PNG los decodifica el propio script: de 8 o 16 bits, no entrelazados (un PNG entrelazado se rechaza). Deshace sus filtros a lo largo de antidiagonales con numpy.

Una entrada incorrecta recibe un error de una línea. Cualquier otra excepción es un bug y conserva su traceback.

## `glbtrim.py`: recortar GLB animados

Conserva solo los clips de animación nombrados de un GLB autocontenido y descarta los accessors, las vistas de buffer y los datos binarios que nada más usa. Los nodos, las meshes, los skins y los materiales conservan sus índices, así que la escena no cambia. Usa solo la biblioteca estándar de Python 3, trata la entrada como datos no confiables (tamaños acotados, cada índice comprobado, sin URIs externas) y escribe los mismos bytes para la misma entrada. Hay que ejecutarlo aislado:

```sh
python3 -I tools/glbtrim.py ~/Downloads/quaternius/horse_a.glb \
  assets/models/fauna/quaternius/horse.glb \
  --keep Idle Idle_2 Idle_Headlow Eating Walk Gallop Gallop_Jump
```

Los clips se escriben en el orden de `--keep`, del que depende el juego (`pampa_render::fauna::Clip`). Imprime el tamaño antes y después, los triángulos, las articulaciones y los clips conservados.

También recalcula normales de vértice suaves, porque las exportaciones tienen sombreado plano y los cuerpos se ven facetados. Esto está activado por defecto; `--smooth-angle DEGREES` fija el ángulo (60 por defecto) y `--smooth-angle 0` conserva las normales del archivo.

- Solo para las normales, los vértices se sueldan por posición exacta y `JOINTS_0`/`WEIGHTS_0` iguales, así que nada se suaviza entre skinnings distintos. La soldadura abarca todas las primitivas de una mesh, así que no se ve ninguna costura donde se encuentran dos materiales.
- Alrededor de cada punto soldado, las caras que comparten una arista que se dobla menos que el ángulo forman un grupo suave, como el auto smooth. Cada vértice recibe las normales de cara de su grupo, ponderadas por el ángulo de la esquina, así que los bordes de los cascos y las puntas de las orejas y los cuernos quedan definidos.
- Solo cambian los bytes de NORMAL: la cantidad de vértices, las posiciones, las UV, los colores y el tamaño del archivo quedan iguales.

## `fbx2glb.py`: de FBX animado a GLB, con Blender

Convierte un FBX animado (el gato y el águila del Animal Pack Vol.2 de Quaternius, que solo trae Blend, FBX y OBJ) en un GLB que el juego carga como los demás modelos de fauna. Es un script de Blender (probado con Blender 5.2), sin tests propios: los tests de `pampa_render::fauna::livestock` comprueban lo que produce. Hay que ejecutarlo headless con `--factory-startup` (sin scripts del archivo), desde fuera de la carpeta que tiene la entrada, y después pasar el resultado por `glbtrim.py`, que ordena los clips y suaviza las normales:

```sh
blender -b --factory-startup --python tools/fbx2glb.py -- Cat.fbx cat_raw.glb \
  --turn -90 --rename Walking=Walk
python3 -I tools/glbtrim.py cat_raw.glb assets/models/fauna/quaternius/cat.glb --keep Idle Walk

blender -b --factory-startup --python tools/fbx2glb.py -- Eagle.fbx eagle_raw.glb \
  --turn 180 --triangles 2400 --rename Flying=Flap Idle=Perch --rest-clip Glide
python3 -I tools/glbtrim.py eagle_raw.glb assets/models/fauna/quaternius/eagle.glb --keep Glide Flap
```

| Opción | Significado |
|---|---|
| `--turn DEGREES` | gira el modelo entero alrededor del eje vertical (respecto del origen) y lo aplica, para que mire hacia +Z en glTF como los modelos de FBX2glTF |
| `--triangles N` | diezma cada mesh hasta unos N triángulos (colapso), antes de que la armadura la deforme |
| `--rename OLD=NEW` | renombra clips; los nombres `…\|CLIP` de las acciones ya quedan como `CLIP` |
| `--rest-clip NAME` | agrega un clip quieto de la pose de reposo (las alas abiertas del águila, que está modelada planeando) |

El GLB sale con Y hacia arriba, en unidades del modelo, con skins, cada acción como un clip y materiales lisos (el juego los tiñe por especie).

## `retarget.py`: clips de animación para los personajes de MakeHuman

Adapta (retargeting) las Universal Animation Library 1 y 2 de Quaternius, CC0 (sus GLB de Godot), al esqueleto `game_engine` de MakeHuman, y escribe los clips que reproduce el juego en un único GLB de animación compartido. Usa la biblioteca estándar de Python 3, más numpy para el paso de despeje del pecho (`--model`), como `mh2gltf.py`. Lee los GLB a través del lector acotado de `glbtrim.py`, trata cada entrada como datos no confiables (sin URIs externas, cada índice, nombre y número comprobados, solo samplers LINEAR y huesos sin escala) y escribe los mismos bytes para las mismas entradas. Hay que ejecutarlo aislado:

```sh
python3 -I tools/retarget.py --ual1 UAL1.glb --ual2 UAL2.glb \
  --target assets/characters/protagonist/model.gltf \
  --model assets/characters/protagonist/model.gltf --model assets/characters/hidalgo/model.gltf \
  --model assets/characters/farach/model.gltf --model assets/characters/bairoletto/model.gltf \
  --talker assets/characters/townsfolk_paisano/model.gltf --talker assets/characters/townsfolk_peon/model.gltf \
  --talker assets/characters/townsfolk_volga/model.gltf --talker assets/characters/townsfolk_headscarf/model.gltf \
  --talker assets/characters/townsfolk_bun/model.gltf \
  -o assets/characters/animations/humanoid.glb
```

La salida contiene el nodo `Root` del objetivo y los 52 huesos que tiene debajo, en reposo, más los clips, y ninguna mesh. Cada clip tiene un canal de rotación por hueso y un canal de traslación en la pelvis. Bevy enlaza un clip a un hueso por la ruta del nombre del hueso desde la raíz de la escena, así que los clips se reproducen en cualquier personaje construido con el mismo esqueleto. Cada clip se renombra con su nombre en el juego y se escribe en un orden fijo, del que depende el juego (`pampa_render::character::clips::Clip`; ver `CLIPS` en el script). Las caminatas y las carreras llevan el `ground_speed` (m/s) y el `stride` (m por ciclo) que midió el script, en los `extras` de la animación. El script también los imprime.

Cómo se adapta un clip, en cada frame:

- **Nombres de huesos.** Los huesos Rigify `DEF-*` de UAL1 y los huesos Unreal de UAL2 se mapean a los 52 de `game_engine`, incluidos los dedos. Las puntas `*_leaf` y `*_04` de UAL2 no tienen equivalente.
- **Alineación del reposo.** UAL reposa en pose T; el reposo de MakeHuman tiene los brazos abajo. Primero la fuente se pone en la pose de reposo del objetivo (su *referencia*). Bajando por la jerarquía, cada hueso conserva su giro de reposo bajo su padre, y luego oscila por el arco más corto hasta apuntar adonde apunta el hueso objetivo. Eso es hacia la siguiente articulación de su cadena (la columna hacia el cuello, el brazo hacia el codo), o a lo largo de su +Y al final de una cadena. La pelvis no oscila.
- **Rotaciones.** El giro de cada hueso en el mundo respecto de la referencia se aplica al reposo en el mundo del hueso objetivo, y luego se hace local bajo el padre en pose del objetivo. Todo esto ocurre en el marco del mundo compartido (Y hacia arriba, mirando hacia +Z, la izquierda en +X), así que el roll de los huesos de ninguno de los dos rigs importa: ningún miembro se tuerce y no se invierte ningún signo.
- **Pelvis.** Su desplazamiento en el mundo desde el reposo se escala según el largo de pierna del objetivo sobre el de la fuente (de la cadera al tobillo, 1.043 para el protagonista). A los bucles se les quita la deriva horizontal, para que se queden en el lugar (el juego mueve la entidad); el balanceo vertical y el lateral se mantienen. Dondequiera que el objetivo, construido de otra forma, pondría un tobillo o la parte delantera de la planta del pie por debajo de su altura de reposo, la pelvis se sube hasta que no sea así.
- **Postura tranquila.** Las esperas de Quaternius están paradas como un boxeador: los pies separados el doble que las caderas, uno 42 cm adelante, las rodillas dobladas unos 20°, los brazos 17° hacia afuera. Para `idle`, `idle_talk`, `idle_fold_arms`, `yes` y `no` (`CALM`), los pies se acercan hasta como máximo 1.3 veces la separación de las articulaciones de la cadera y 12 cm uno delante del otro, a las mismas alturas y con su propio giro. Las piernas se vuelven a resolver a través de las rodillas, que conservan el 35% de su flexión, y las caderas suben tanto como lo permite la pierna más recta (o bajan, cuando los pies quedan debajo de ellas). En `idle`, `yes` y `no`, los brazos se bajan hasta como máximo 10° hacia afuera (el del reposo de MakeHuman es de 11°). Las carreras se dejan como las tiene la fuente.
- **Caminata erguida.** El `Walk_Loop` de Quaternius es una caminata de juego: en el protagonista, su rodilla de apoyo se dobla 25° en promedio (36° como máximo), el torso se inclina de 7 a 9°, los hombros van 6 cm hacia atrás y los codos se doblan de 32 a 41°, y el juego la reproducía 1.5 veces más rápido que lo grabado. En el juego parecía que acechaba. Para `walk` (`UPRIGHT`), cada clave se endereza hasta una caminata de calle tranquila de 1919 (`Retargeter.upright`): el tranco se alarga 1.15 veces (la posición de cada tobillo por delante o por detrás de su cadera se estira, a su propia altura), las rodillas se enderezan 19° (nunca por debajo del 30% de su flexión, ni de 4°: una rodilla estirada del todo se traba y salta, lo que en el juego parecía una luxación), las caderas suben según lo piden las piernas, mezcladas según qué pie está más cerca del suelo y suavizadas a lo largo del bucle sobre 2 claves a cada lado para que nunca den saltos, una pierna nunca se estira más que su propia flexión suavizada (en cambio, su pie se levanta un poco), y la flexión de cada rodilla en el aire se suaviza a lo largo del bucle para que la canilla se pliegue a un ritmo parejo, el torso se gira hacia atrás en `spine_01` hasta una inclinación media de 2°, las clavículas vuelven al reposo, los brazos cuelgan como en reposo y se balancean de 10° hacia atrás a 14° hacia adelante al ritmo del balanceo de la fuente, los codos se doblan de 10 a 25°, y la cabeza vuelve a su pitch de reposo. El resultado: la rodilla de apoyo se dobla 11° en promedio (19° como máximo) y nunca menos de 4°, una rodilla cambia como máximo 16° por clave (13 en la fuente; menos de 11° por frame a 60 frames por segundo en la caminata más rápida) sin trabarse ni dar latigazos, las caderas se mueven con suavidad, el torso se inclina de 2 a 4°, y las manos se balancean desde la línea de la cadera hasta 30 cm por delante de ella. La caminata se marca con `"upright": true` en sus `extras`, y su velocidad se mide sobre el propio objetivo enderezado (1.27 m/s, frente a 1.10 en la fuente). `Walk_Formal_Loop` no era una mejor base: mantiene las manos detrás de las caderas todo el tiempo.
- **Despeje del pecho** (con `--model`, y `--talker` solo para las manos que hablan). El pecho del maniquí de origen es más delgado que el de los hombres de MakeHuman vestidos, así que sus brazos cruzados se hundían de 8 a 12 cm en los de ellos. Para `idle_fold_arms` e `idle_talk` (`CLEAR`), los brazos se llevan hacia adelante tal como están: cada clavícula oscila hacia adelante, redondeando los hombros, el menor múltiplo de 5 mm que mantiene los antebrazos y las manos de cada modelo dado al menos 1.5 cm por delante de su pecho. Un modelo dado con `--talker` (los vecinos, que nunca cruzan los brazos) cuenta solo para `idle_talk`. Cada modelo se pone en pose como lo pone el juego (las rotaciones del clip, las caderas ajustadas a sus piernas) y con skinning en CPU. El pecho se muestrea a lo largo de su normal (+z) entre `spine_02` y el cuello, a menos de 10 cm de su centro. La cara interna del codo (vértices con menos del 75% de peso en un hueso) no se cuenta, ya que el skinning lineal la pellizca hacia dentro del cuerpo en esa flexión. Los brazos cruzados avanzan 12 cm y las manos que hablan 6 cm (3 cm antes de las vecinas, cuyos bustos y chales cruzados son lo que más sobresale). Ambos se escriben como `reach` en los `extras` del clip.
- **Velocidad.** La velocidad sobre el suelo de un bucle es la rapidez con que un punto del pie sobre el suelo (a menos de 1 cm de su altura más baja) se desliza hacia atrás una vez quitada la deriva, más la deriva: la mediana a lo largo del clip, escalada al objetivo. El juego reproduce una caminata a la velocidad sobre el suelo dividida por esta velocidad, para que los pies no se deslicen.

`test_retarget.py` cubre esto con pequeños rigs sintéticos: los mapeos de nombres contra el esqueleto incluido, una fuente en la pose de referencia que da el reposo del objetivo, un giro conocido, cada hueso apuntando adonde apunta su hueso de origen, sin torsión del antebrazo, el escalado por largo de pierna, los bucles mantenidos en el lugar, el límite contra el suelo, la postura tranquila, los brazos llevados hacia adelante, la medición de la velocidad y la salida determinista. La caminata erguida también se cubre allí, sobre una caminata agachada sintética (`UprightWalk`), sin los paquetes: la misma salida cada vez y en el empalme del bucle, ninguna rodilla por debajo de `KNEE_MIN`, el torso enderezado cerca de `LEAN`, los brazos balanceándose dentro de `ARM_SWING` con los codos dentro de `ELBOW`, la cabeza nivelada, una clave en la que ninguna pierna llega tomando sus caderas de sus vecinas (y un error cuando ninguna clave puede), piernas y brazos rectos (sin polo, sin bisagra) resueltos de forma finita, y la velocidad del lado del objetivo (el deslizamiento de un pie apoyado; un error cuando ningún pie se queda en el suelo). También comprueba el orden de los clips, las rutas de los huesos y los clips calmados del GLB incluido. Comprueba que ningún antebrazo ni mano de ninguno de los cuatro hombres llegue a menos de 1.5 cm de su pecho en los brazos cruzados y en las manos que hablan, ni del de ningún vecino en las manos que hablan, y que los modelos `--talker` cuenten solo para la charla. Comprueba que la caminata incluida es erguida: la rodilla de apoyo por debajo de 20°, la inclinación por debajo de 5°, la cabeza a menos de 3° de la horizontal, los hombros no echados hacia atrás, cada mano más de 12 cm por delante de la línea de la cadera en el extremo delantero de su balanceo y detrás de ella en menos de un cuarto de las claves, los codos de 8 a 27°, y las rodillas nunca por debajo de 4° (ninguna doblada hacia atrás), cambiando menos de 17° por clave y su tasa de cambio menos de 14, las canillas a menos de 3° del plano del muslo y las caderas sin saltar nunca 2 cm en una clave, en el empalme del bucle. Cada alcance se prueba primero en una de cada cinco claves, lo que descarta la mayoría a bajo costo, y luego en todas las claves (`CLEAR_EVERY`). Un `--model` incorrecto (que no es UTF-8, no es JSON, datos que no toman su forma, o sin numpy) es un error de una línea, como cualquier otra entrada incorrecta, y la salida se escribe en un archivo nuevo junto al viejo y luego se renombra sobre él, así que una ejecución fallida deja intacto el archivo viejo. Cuando los paquetes están en `~/Downloads/quaternius/ual/`, también reconstruye el GLB y lo compara byte por byte con el incluido. Con los paquetes también mide la caminata de la fuente (agachada: su rodilla de apoyo por encima de 30°, su inclinación por encima de 6°, sus hombros 4 cm o más hacia atrás) y comprueba la misma caminata, enderezada por la herramienta, contra esos límites; comprueba que el torso y la cabeza se inclinan como los de la fuente (caminata, espera, brazos cruzados), que vistas de frente las piernas y los brazos se abren como los de la fuente antes de calmarlos, y que las esperas calmadas están paradas con naturalidad.

## `rhea.py` y `animal_rig.py`: animales propios modelados en Blender

`rhea.py` modela, arma, pinta y anima el ñandú (*Rhea americana*) por script en Blender (probado con Blender 5.2), sin assets de terceros; `animal_rig.py` reúne las piezas genéricas para las demás especies. Hay que ejecutarlo headless con `--factory-startup`; después `textures_to_ktx2.sh` convierte los mapas y `glbtrim.py` ordena los clips como los reproduce el juego (`pampa_render::fauna::Species::clip_names`), conservando las normales de Blender:

```sh
blender -b --factory-startup --python tools/rhea.py -- rhea_raw.glb \
  --textures assets/models/fauna/own --preview /tmp/rhea
FORCE=1 sh tools/textures_to_ktx2.sh assets/models/fauna/own/rhea_body assets/models/fauna/own/rhea_feathers
python3 -I tools/glbtrim.py rhea_raw.glb assets/models/fauna/own/rhea.glb \
  --keep Idle Idle_Look Graze Walk Run --smooth-angle 0
```

| Opción | Significado |
|---|---|
| `--textures DIR` | escribe las fuentes JPG de los dos conjuntos KTX2: `DIR/rhea_body` (color, normal_gl, roughness, ao) y `DIR/rhea_feathers` (color, normal_gl, roughness, opacity) |
| `--preview DIR` | renderiza PNG con Eevee (1000 x 800, fondo neutro): de costado, de tres cuartos de frente y de atrás, primeros planos a 2–3 m, y cuadros de la caminata, la carrera, el pastoreo y la espera alerta |
| `--look v13\|v12\|v8` | el juego de perillas del pelaje y las plumas (`LOOKS`): `v13` (por defecto, el modelo incluido), `v12` o `v8` |
| `--no-clips` | solo el modelo y el rig, para iterar la forma más rápido |

Una corrida completa tarda unos 7 minutos en la CPU (los horneados de Cycles).

**Piso de calidad.** Regla del usuario (2026-10-29): "quiero como Mínimo de aca en adelante y para absolutamente todo el detalle de la V8 del ñandú, ese nivel de detalle debe considerarse lo más bajo, de este nivel para arriba". Cada versión nueva, de este animal o de cualquier otro hecho con esta herramienta, tiene que estar a ese nivel o por encima, nunca por debajo. `--look v8` vuelve a construir esa versión para comparar. El 2026-10-30 el usuario aprobó la v13 (las plumas se distinguen una por una: barbas abiertas, raquis visible, bordes más oscuros, plumas del ala largas y caídas, y la coronilla y la nuca oscuras) y subió el piso: "ese nivel de detalle queremos para todo el juego, primero los animales, despues los personajes, las ropas, las telas, las caras, etc".

Los pasos, todos deterministas:

1. **Forma.** Cada parte es un tubo cerrado a lo largo de una curva suave con una sección elíptica por punto de control (`animal_rig.loft`): el cuerpo, el cuello en S, la cabeza aplanada con los ojos, el pico corto y algo curvo, las patas (muslo grueso que se afina hacia el tarso, dedos largos con nudillos) y unos 80 lóbulos de plumas sueltas (el faldón, más largo a los costados que adelante; las plumas de las alas caídas a los lados; el polisón de la grupa; mechones sobre el lomo). `warp` estira las patas y levanta el resto del ave en un solo lugar.
2. **Escultura y mesh del juego.** `fuse_dense` une las partes en una superficie cerrada densa (remesh de vóxeles de 6 mm, unos 290.000 triángulos) y la pinta: un color por vértice según la zona (lomo pardo grisáceo, flancos y vientre más claros, alas oscuras con las puntas claras, la base del cuello y el pecho negros del macho en degradé irregular, cuello superior y cabeza grises, patas grises) y un material procedural de plumas (barbas colgantes, filas de puntas, manchas por pluma de 10–25 cm con el borde más oscuro, escamas en las patas). `decimated_copy` la reduce a 5.200 triángulos y `unwrap` le da UV.
3. **Mapas.** Cycles hornea de la escultura a la mesh (1024²): el color, el normal map (de la forma y del relieve de las plumas) y la oclusión ambiental. `feather_atlas` pinta seis plumas sueltas (cuatro pardas, dos negras) con su opacidad, su normal map y su rugosidad.
4. **Tarjetas de plumas.** 324 tiras de dos segmentos (`cards`) sobre el faldón, las alas, el polisón, el lomo y la base del cuello, colgando cada una a su manera y de su largo, que se separan de la superficie hacia la punta; usan el atlas con alpha test (los mips conservan la cobertura, como las hojas de los árboles) y toman los pesos de la piel más cercana (`transfer_weights`).
5. **Rig.** 20 huesos (cuerpo, grupa, tres del cuello, cabeza; por lado muslo, tibia, tarso, dedos, ala, plumas del ala y faldón) y pesos automáticos (calor), a lo sumo 4 huesos por vértice.
6. **Clips.** Cada clip es una función de la fase: `animal_rig.Pose` convierte direcciones por hueso en su pose local. Las patas apoyan con una IK analítica (el talón siempre dobla hacia atrás) sobre una trayectoria de pie que se desliza a la velocidad del clip mientras apoya: la caminata a 0.88 m/s y la carrera a 7.23 m/s (en el modelo, sin escalar), que el juego ajusta a la velocidad real (`Species::walk_speed`, `gallop_speed`). Movimiento secundario: el cabeceo de las aves al caminar (la cabeza se queda quieta y después avanza de golpe), el faldón que se balancea con cada muslo y las plumas del ala y del faldón que se sacuden con cada pisada al correr. Todos en el lugar, sin movimiento de raíz, y el último cuadro igual al primero.

El GLB lleva dos materiales lisos, `Body` y `Feathers`; el juego les pone los mapas KTX2 (`Species::skin`) y dibuja las tarjetas solo a menos de 40 m de la cámara. Costo para la iGPU Renoir: 6.496 triángulos por ñandú (a lo sumo 10 ñandúes cargados, unos 65.000), 3 mapas de 1024² por conjunto en BC7 (unos 8 MB de video entre los dos conjuntos con sus mips, compartidos por todos los ñandúes) y las tarjetas con alpha test solo de cerca. Sin tests propios: los tests de `pampa_render::fauna::livestock` cargan el resultado y comprueban sus clips, sus huesos, sus UV y tangentes, su presupuesto, su tamaño, sus mapas y el LOD de las tarjetas. Para otra especie: copiar `rhea.py`, cambiar las partes, los huesos, la pintura, las tarjetas y las funciones de los clips, y agregar la especie a `Species`.

## `horse.py`: el caballo criollo modelado en Blender

`horse.py` modela, arma, pinta y anima el caballo criollo por script en Blender (probado con Blender 5.2), con las mismas piezas de `animal_rig.py` que el ñandú y al piso de calidad de la v13. Es el caballo del juego (los de corral y los ensillados), en `assets/models/fauna/own/` (la v20, aprobada por el usuario el 2026-10-30); reemplaza a `assets/models/fauna/quaternius/horse.glb`. Para volver a generarlo:

```sh
blender -b --factory-startup --python tools/horse.py -- horse_raw.glb \
  --textures assets/models/fauna/own [--preview /tmp/horse] \
  [--reference assets/models/fauna/quaternius/horse.glb]
FORCE=1 sh tools/textures_to_ktx2.sh assets/models/fauna/own/horse_body assets/models/fauna/own/horse_hair
python3 -I tools/glbtrim.py horse_raw.glb assets/models/fauna/own/horse.glb \
  --keep Idle Idle_2 Idle_Headlow Eating Walk Gallop Gallop_Jump --smooth-angle 0
```

Las constantes del juego medidas sobre los clips (`HORSE_WALK_SPEED`, `HORSE_GALLOP_SPEED`, `GALLOP_SPEED`, `GALLOP_MEANS`, `GATHER`, `GALLOP_PLANT`, `GALLOP_LANDING`, `WALK_PLANT`, `JUMP_FLIGHT`, `PARTIAL`, `UNANIMATED`) dicen en su documentación cómo se midieron: si cambia un clip, hay que volver a medirlas.

| Opción | Significado |
|---|---|
| `--textures DIR` | escribe las fuentes JPG de los dos conjuntos KTX2: `DIR/horse_body` (color, normal_gl, roughness, ao) y `DIR/horse_hair` (color, normal_gl, roughness, opacity) |
| `--preview DIR` | renderiza PNG con Eevee (1000 x 800): de costado y de tres cuartos de frente en cuatro pelajes (bayo, tordillo, overo, zaino, pintados como `Coat::color_at`), de tres cuartos de atrás, primeros planos a 2–3 m, cuadros de los siete clips (14 del galope) y primeros planos del cuarto y la paleta a lo largo del galope (`gallop_quarters_*`, `gallop_shoulder_*`) |
| `--reference GLB` | renderiza también el caballo de Quaternius con los mismos pelajes y cámaras (`ref_*.png`) y compara los rayos del apero con los suyos |
| `--draft` | vóxeles de 8 mm en vez de 4,5 mm, para iterar la forma más rápido |
| `--no-clips` | solo el modelo y el rig |
| `--shots REGEX` | con `--preview`, solo las capturas cuyo nombre coincide (por ejemplo `'gallop_(hip\|mane)'`: la punta de la cadera a lo largo del vuelo de la pata trasera, `gallop_hip_*`, y la crin al galope de los dos lados en un alazán y un bayo, `gallop_mane_*`) |

**Contrato con el juego.** El rig copia el de Quaternius: los mismos 50 joints con sus nombres, padres y ejes de reposo (el `Torso` con +Y hacia el frente y +X a la derecha), los mismos nodos hoja `_end`, y los vasos colgando de los huesos IK (`IKFrontLeg.*`, `IKBackLeg.*`). Agrega 18 huesos: `Mane1`–`Mane4` y `Forelock` (crin y copete), `Tail8` y `Tail9` (la cola más larga), `Jaw`, `LipUpper`, `LipLower` y `Nostril.L/R` (la cara), y seis músculos (`MUSCLES`): `Quarter.L/R` (el cuarto trasero, en el fémur), `Gaskin.L/R` (en la tibia) y `Scapula.L/R` (la paleta y el tríceps, en el muñón de la paleta). Los siete clips tienen los largos del original (`Gallop_Jump` con el vuelo entre 0.42 y 1.17 s); la caminata y el galope son ciclos en el lugar cuyos vasos se deslizan a la velocidad medida por el juego (unos 1.25 y 7.3 m/s). La cruz queda a 1.5 m con los vasos en el origen, y el script compara los rayos del apero (`tack.rs`: costillar, cabeza, freno, cuello y quijada) con los del caballo de Quaternius a la misma escala. Los materiales llevan los nombres del original: `Main` es todo el pelaje (el juego lo pinta por vértice con `Coat::color_at`, así que el GLB no trae colores), `Hair` la crin, el copete y la cola, y `Main_Dark` (los espejuelos y los espolones), `Muzzle` (solo el interior de los ollares y la boca: la piel oscura del hocico va en `Main`, oscurecida en degradé en sus mapas), `Hooves`, `Main_Light` (la frente y la caña nasal), `Eye_Black` y `Eye_White`.

Los pasos, todos deterministas:

1. **Escultura.** Un campo de distancias con signo (`animal_rig.Sdf`, vóxeles de 4,5 mm) une con uniones suaves las masas y los músculos (costillar, cruz, grupa, paleta, tríceps, antebrazo, pectorales, glúteos, cuarto trasero, babilla, gaskin, cuello con su crinera, cabeza con carrillos, quijadas, ollares, labios, párpados y orejas huecas), y graba la anatomía de superficie con surcos y relieves (`groove`, `ridge`): la espina de la paleta, las costillas apenas marcadas, la línea entre los músculos del cuarto, el pliegue de la babilla, la vena del vientre, la cresta facial, los tendones y las venas de las patas. OpenVDB la convierte en una superficie densa.
2. **Mapas.** La escultura lleva por vértice la dirección del pelo (hacia atrás en el cuerpo, hacia abajo en el cuello, las patas y las nalgas, hacia adelante en el vientre, remolinos en la frente y en los ijares) y un tono que no depende del pelaje (el lomo más oscuro, los ijares y el vientre más claros, el contorno del ojo y el hocico oscuros); Cycles hornea sobre la mesh del juego (7.200 triángulos, más densa donde el pelaje cambia de color en una línea: los cabos, la raya del lomo, el hocico, los ojos y las orejas; mapas de 1024²) el normal map (forma, músculos y pelo corto), una luminancia neutra (pelo por tono) que el juego multiplica por el color del pelaje, la rugosidad (el brillo satinado del pelo, más áspero en el hocico y el vaso) y la oclusión. La mesh del juego sale de diezmar la escultura; la diezma deja unas 30 astillas (un vértice casi sobre el lado opuesto de su triángulo) que, con la menor flexión, se dan vuelta y se ven como una muesca oscura (había una en la punta de la cadera, al final del vuelo de la pata trasera): `flip_slivers` gira el lado largo de cada una hacia los otros dos vértices cuando así los dos triángulos quedan más llenos, sin mover ningún vértice. Los vasos llevan anillos de crecimiento y túbulos; el hocico, poros y arrugas.
3. **Pelo largo.** El atlas de pelo tiene ocho columnas con su opacidad: seis de mechones tupidos (`lock_atlas`: cuatro mechones separados de 34 hebras finas cada uno, superpuestos en la raíz y abriéndose hacia la punta, para que entre ellos se vean el cuello y la luz; cada mechón con su tono, sus hebras muy variadas y más oscuras en la raíz y donde se abre, así que una tarjeta se lee como pelo y no como una lámina pintada) y dos ralas (`hair_atlas`) para el copete, las pestañas y las orejas; `strand_cards` arma tiras que caen desde la crinera (228 en tres capas superpuestas y cada vez más apartadas del cuello, llena y larga sobre el lado derecho del cuello, algunas al izquierdo; cada mechón gira sobre su largo hasta 35° hacia la punta, `MANE_TWIST`, así que cada uno toma la luz a su manera), el copete (26 mechones finos y ralos entre los ojos), la cola (130, llena, hasta la altura de los corvejones o apenas más abajo, como la de un criollo), las pestañas y unos pelos claros dentro de las orejas, apartadas del cuerpo con el campo de distancias.
4. **Rig y pesos.** Pesos automáticos (calor) de los huesos que mueven la piel, corregidos en las patas traseras (`hinds`: el gaskin atado a la tibia, `BackUpperLeg`, y el muslo de afuera al fémur, `BackLeg`, fundidos en la babilla, el corvejón y hacia la cadera, para que al galope el gaskin no se afine como un palito). La punta de la cadera es pelvis (`pelvis`): por delante y por encima de cada cadera el peso del fémur pasa al cuarto trasero (`Back`), del todo a `PELVIS_REACH` (10 cm) de la articulación. Cada músculo (`weight_muscles`) toma, dentro de su elipsoide (que se desvanece en su tercio de afuera y hacia la línea media), el peso de su hueso padre (el cuarto, también el de la cola en la nalga, que así sigue al fémur y no a la cola levantada; ya no el de la tibia sobre la babilla, que partía la piel de detrás de la babilla entre el fémur y la tibia en una línea donde se aplastaba al venir la pata adelante): sin escala se mueve igual que antes. Después `smooth_folds` suaviza todos los pesos alrededor de la punta de la cadera y sobre el pliegue de detrás de la babilla (`FOLDS`: cada pasada lleva los pesos de un vértice a mitad de camino hacia el promedio de sus vecinos, del todo en el centro y cada vez menos hacia el borde, para que no aparezca otro borde donde termina), así que la piel se pliega a lo ancho de una mano en vez de aplastar unos pocos triángulos (en el juego, a lo sumo 16 de 1.002 triángulos del muslo bajo un tercio de su área, contra 24). Las tarjetas de la crin y el copete cuelgan cada vez más de su hueso hacia la punta (las del lado izquierdo de la crin, solo un cuarto, `MANE_LEFT`, para que al levantarse la crin no se metan en el cuello); las de la cola siguen la cadena de la cola.
5. **Clips.** `animal_rig.Pose` arma cada cuadro por cinemática directa (`turn`, `aim`, `move`); las patas alcanzan el menudillo con una IK analítica, y el vaso rueda sobre los talones al apoyar y sobre la pinza al despegar. Movimiento secundario: crin, copete y cola que se balancean con retardo por hueso, orejas que se mueven en las esperas, la mandíbula que mastica al comer. Cada hueso se anima en cada clip. Las referencias: las láminas de Muybridge (*Animal Locomotion*, 1887, lám. 636 y 639, dominio público) y la literatura de biomecánica equina.
   - **Esperas:** la cara siempre por delante de la vertical (de pie 24–31°; dormitando con la cabeza baja, la nuca a 1.38 m y la cara a 16–21°, abriendo la nuca; pastando, la cara llega al pasto casi vertical, 2° por delante). Para descansar apoya una pata trasera sobre la pinza con la cadera caída de ese lado, nunca una mano plegada; el cuerpo queda casi a nivel (rolido de 0.8°), así que las rodillas no se doblan.
   - **Caminata:** la cabeza sube y baja unos 9 cm en la nuca, dos veces por tranco (`WALK_NOD`, 3°; Loscher et al. 2016: 9,1 ± 3,4 cm).
   - **Músculos:** al armar cada cuadro (`Pose.apply`, en todos los clips), `muscle_shape` mide la flexión de cada articulación contra el reposo y engrosa su músculo tanto como lo acorta a lo largo (escala del hueso): el cuarto se redondea hasta un 18 % cuando el fémur va adelante bajo la panza (7 % al empujar atrás), el gaskin hasta un 20 % cuando el corvejón se abre para empujar, y la paleta avanza y sube hasta 4,5 y 2 cm cuando la mano se estira, con el tríceps engrosado hasta un 18 % cuando la pata barre atrás apoyada. Como en los videos del usuario (L42–L43 del documento de la fauna): la piel ya no solo sigue a los huesos.
   - **Galope:** contra los videos del usuario. Las patas traseras apoyan muy por debajo de la panza (el menudillo hasta 0.57 m por delante de la cadera, la pata 33° por delante de la vertical) y se estiran atrás hasta unos 30° al empujar: su barrido de apoyo es más largo que el de las manos (1.36 contra 1.02 m, `sweep` y `duty` por (manos, patas)) a la misma velocidad; en el vuelo vienen adelante temprano, el vaso por delante del corvejón (`hind_lead`, `hind_peak`, `hip_swing`). En el vuelo las manos suben el antebrazo hasta casi la horizontal (`lift` 0.46, alto recién a mitad del vuelo, `fore_peak`) con la caña colgando hacia abajo y atrás, nunca plegada plana bajo el antebrazo (la rodilla no se cierra más de unos 60°: la primera vuelta la cerraba a 43° y en el juego parecía dislocada, L55), y la mano de galope se estira casi recta 0.55 m por delante del codo, pasándose 10 cm del apoyo y volviendo (`reach`). Una mano apoyada va casi recta: la paleta sube o baja por el pecho lo que haga falta (`sling`, `SLING_REACH`, `SLING_MOST`, el tórax colgando entre las paletas), así que la rodilla no se dobla bajo el peso. El cuerpo cabecea con la grupa más baja al apoyar las traseras (5,5° hacia arriba) y apenas 2,5° hacia abajo sobre las manos; el cuello sube sobre las traseras y baja sobre las manos; el lomo lo redondea el juego en fase con el apoyo de las traseras (`gait.rs` `GATHER`), y el clip lo dobla 2° en contra para que los bastos no se despeguen. `clips` imprime cuánto les falta a los vasos apoyados para llegar a su marca (a lo sumo 2 cm en las manos al apoyar). Los vasos pliegan hasta 60° (manos) y 55° (patas) en el aire. La crin vuela: se levanta del cuello 32° (`MANE_GALLOP_LIFT`), se va 16° hacia la cruz (`MANE_GALLOP_BACK`) y una onda la recorre a lo largo de la crinera en cada tranco (con -12° y sin irse atrás quedaba a 2,9 cm del cuello, una lámina en el juego; ahora a unos 5,5); su tinte siempre se aparta del pelaje (`Coat::mane`: negra en el zaino y el bayo, blanquecina en el tordillo, rubia en el alazán, más oscura en el overo). La cola sale levantada del maslo (`tail_lift`), cae en curva (`tail_curve`, cada hueso más abajo) y una onda la recorre con retardo en cada tranco (`tail_whip`): nunca una vara recta.
   - **Salto:** las manos despegan primero; las patas traseras apoyan bajo la panza (`JUMP_PLANT`), empujan mientras sus vasos se deslizan hacia atrás a la velocidad del galope y dejan el suelo estiradas (ahí empieza el vuelo del juego, 0.41 s). En lo alto las manos van plegadas y las patas traseras flexionadas y arrastradas por detrás de las babillas, nunca bajo la panza. Las manos se despliegan recién en el último décimo del vuelo (`JUMP_REACH`), primero la de atrás (la derecha, con mano de galope izquierda), que apoya cuando el vuelo termina con el cuerpo todavía inclinado hacia adelante; las patas traseras siguen atrás y apoyan después, en el galope que retoma con las manos apoyadas (`GALLOP_LANDING` en el juego). Un `tail_lift` positivo aleja la cola de las patas traseras.

Costo para la iGPU Renoir: 11.944 triángulos por caballo (7.200 del cuerpo, 4.296 de pelo con alpha test, 448 de los ojos), dos conjuntos de mapas de 1024² (la luminancia con el tono del lomo, el vientre y manchones suaves, los surcos oscurecidos por la oclusión al 60 %, el pelo en vetas y la rugosidad entre 0.45 y 0.9 a lo largo del pelo; como el ñandú, unos 8 MB de video entre los dos con sus mips, compartidos por todos los caballos). En el juego (`Species::skin`), `horse_body` viste `Main` (por los colores por vertex del pelaje) y las partes de un solo color (`Main_Dark`, `Main_Light`, `Muzzle`, `Hooves`, por su color), y `horse_hair` viste `Hair` con alpha test, por ambos lados con la normal del dorso invertida y teñido con la crin del pelaje; las tarjetas de pelo se dibujan a cualquier distancia. Una corrida completa tarda unos 4 minutos en la CPU.

## `cattle.py`: la vaca y el toro modelados en Blender

`cattle.py` modela, arma, pinta y anima la vaca y el toro de la estancia de 1919 por script en Blender (probado con Blender 5.2), al piso de calidad de la v13. Es el caballo con anatomía bovina: `animal_rig.Steps` ejecuta `horse.py` sin su `main()` y le reemplaza los nombres (el esqueleto, la escultura, las regiones del pelaje, el tono, los pesos, las tarjetas de pelo, la cara de la cabeza, los planes de marcha y las funciones de los clips), así que la escultura horneada, la IK de las patas, las marchas, los músculos y las vistas previas son los mismos pasos que los del caballo. Son la vaca y el toro del juego (la v6, aprobada el 2026-10-31), en `assets/models/fauna/own/`; reemplazan a la vaca y el toro de Quaternius, que se borraron. Para volver a generarlos (unos 6 minutos cada uno en la CPU):

```sh
O=assets/models/fauna/own
blender -b --factory-startup --python tools/cattle.py -- cow_raw.glb \
  --textures $O [--preview /tmp/cow]
blender -b --factory-startup --python tools/cattle.py -- bull_raw.glb --bull \
  --textures $O [--preview /tmp/bull]
FORCE=1 sh tools/textures_to_ktx2.sh $O/cow_body $O/bull_body $O/cattle_hair
# Cada pelaje, como el color de un conjunto (el normal map y la rugosidad
# solo completan el conjunto que pide el script).
for set in cow_body bull_body; do
  for coat in hereford angus roan overo; do
    t=$(mktemp -d)
    cp $O/$set/$coat.jpg $t/color.jpg
    cp $O/$set/normal_gl.jpg $O/$set/roughness.jpg $t/
    sh tools/textures_to_ktx2.sh $t && mv $t/color.ktx2 $O/$set/$coat.ktx2
    rm -r $t
  done
done
for a in cow bull; do
  python3 -I tools/glbtrim.py ${a}_raw.glb $O/$a.glb \
    --keep Idle Idle_2 Idle_Headlow Eating Walk Gallop Gallop_Jump --smooth-angle 0
done
```

Las constantes del juego medidas sobre el modelo (`CATTLE_WALK_SPEED`, `CATTLE_GALLOP_SPEED`, `COW_WITHERS`, `BULL_WITHERS` en `crates/render/src/fauna/livestock.rs`) dicen en su documentación cómo se midieron: si cambia el modelo o un clip, hay que volver a medirlas.

| Opción | Significado |
|---|---|
| `--bull` | el toro: el mismo esqueleto, con giba de músculo sobre el cuello, papada más honda, antebrazos y cabeza más gruesos, cuernos cortos y gruesos, prepucio con su mechón y escroto (el juego lo dibuja más grande, `Species::withers`) |
| `--textures DIR` | escribe las fuentes JPG de `DIR/cow_body` o `DIR/bull_body` (color, normal_gl, roughness, ao, y el mapa de color de cada pelaje: `hereford`, `angus` y `overo` de 1024², `roan` de 2048²) y `DIR/cattle_hair` (color, normal_gl, roughness, opacity) |
| `--preview DIR` | renderiza PNG con Eevee: de costado y de tres cuartos en los cuatro pelajes (`COATS`, con los mapas de cada pelaje como en el juego), primeros planos de la cabeza, el ojo, el pelo, las patas, las pezuñas, la ubre y la cola, cuadros de los siete clips y la malla liviana sola en cada pelaje (`light_*`) |
| `--draft`, `--no-clips`, `--shots REGEX` | como en `horse.py` |

Razas y pelajes: hacia 1919 el criollo pampeano ya estaba absorbido por cruza en las razas británicas. El Shorthorn (el "Durham", base del rodeo argentino y del comercio de carne enfriada; colorado, rosillo o blanco), el Hereford (colorado con la cara, la cruz, el pecho, la línea de abajo, las patas y la borla blancos, con cuernos) y el Aberdeen Angus (negro y mocho, sin cuernos), con algún overo cruza criolla. `color_at` pinta los cuatro (`hereford`, `angus`, `roan` y `overo`, con las manchas de `overo_black`) en el marco del cuerpo, con bordes ondulados. **Mapas de pelaje.** En lugar de portar a Rust `color_at`, `warp` y `unwarp`, `bake_coats` hornea cada pelaje en un mapa de color del cuerpo: Cycles hornea el punto y la normal del modelo en cada texel de las UV (`bake_geometry`), `coat_pixels` evalúa ahí el pelaje (el del rosillo, con su mapa de pelos de 2048², `bake_roan`, blanco donde el pelo es blanco, que a media distancia se lee gris) y lo multiplica por la luminancia neutra como el juego (en luz lineal). Así los bordes del blanco del Hereford quedan tan nítidos como el mapa, y no como los colores por vértice de unos 6.000 triángulos. En el juego, `Main` se dibuja con el mapa de su pelaje (`Paint::Map`) y las partes de un solo color multiplican la luminancia neutra por su color (`paint`, con los valores de `COATS`). El modelo tiene un solo juego de cuernos: el juego oculta `Horns` en el Angus, y como son superficies aparte, la nuca queda entera.

Anatomía: un vacuno de carne de 1919, bajo y macizo como en las fotos de referencia: la anatomía se diseña sobre un marco más alto y `warp` acorta las patas (entre los menudillos y la panza cada altura se comprime al 80 %, y todo lo de arriba baja 10 cm), así el cuerpo, la cabeza y las pezuñas conservan su forma; los pelajes leen cada punto con `unwarp`, y el puerto a Rust debe hacer lo mismo. Costillar hondo y ancho, línea dorsal recta, puntas de anca e isquiones marcados, papada y pecho, cola larga hasta los corvejones con la borla por debajo, frente ancha y chata con la nuca entre los cuernos, orejas horizontales, ojos salientes, morro ancho y húmedo, patas gruesas de vacuno de carne y pezuñas hendidas (dos pezuñas de unos 10 cm por pata, con la hendidura, la punta inclinada, los talones, la corona donde empieza el pelo y las dos pezuñas accesorias en punta detrás del menudillo), y ubre con cuatro pezones, de piel lampiña (rosada o pigmentada según el pelaje, nunca del color del pelo, con un borde suave hacia la panza). Los cuernos son piezas aparte: todas sus caras, y solo ellas, son `Horns`. Las caminatas apoyan las manos con el cabestrillo torácico y dentro de su alcance (a lo sumo 2 mm de falta en la caminata, 1 cm en el galope). El rig conserva los nombres y la jerarquía del cuadrúpedo de Quaternius (`Body`, `Back`, `Torso`–`Torso3`, `Neck1`–`Neck3`, `Head`, las patas, `Tail1`–`Tail7` y los huesos IK con sus hojas `_end`) y agrega las orejas (`Ear1.L/R`), `Jaw`, `LipUpper`, `Nostril.L/R`, `Tail8` y `Tail9`, los seis músculos del caballo, la papada (`Dewlap`) y la ubre (`Udder`; en el toro `Scrotum`): 58 huesos. Los clips son los siete del original con sus largos: rumiar (la mandíbula de costado, una vez por segundo), las orejas, la cola que espanta moscas, mirar atrás descansando una pata, dormitar con la cabeza baja, pastar arrancando cada bocado, la caminata lateral (1.04 m/s medidos en los vasos) con la cabeza que cabecea y la papada y la ubre que se balancean, el galope (5.62 m/s, dos zancadas y algo por segundo) con la cabeza baja y la cola levantada, y en `Gallop_Jump` una corcoveada (el juego no lo reproduce en los vacunos). Materiales: `Main` (todo el pelaje, con el mapa de su pelaje), `Main_Light` (la piel de la ubre), `Muzzle` (el morro pelado y la boca), `Hooves`, `Horns`, `Hair`, `Eye_Black` y `Eye_White`.

Costo: vaca 8.192 triángulos (cuerpo unos 5.900, cuernos, pelo con alpha test y ojos), toro 8.792; la malla liviana (`light_mesh`, `Cow_LOD`: la piel sola diezmada a 1.400 triángulos, con sus UV y los pesos interpolados de la completa) que el juego dibuja desde los 40 m (`LIGHT_FROM`, fundida con dithering). Video: un conjunto de mapas de 1024² por cuerpo y uno compartido de pelo (unos 12 MB entre los tres con sus mips) y los mapas de pelaje (tres de 1024² y el rosillo de 2048² por cuerpo: unos 20 MB entre los ocho, BC7 con mips). Tests sin Blender (`test_cattle.py`): los pelajes, sus mapas y las velocidades de las marchas.

## `fox.py`: el zorro gris pampeano modelado en Blender

`fox.py` modela, arma, pinta y anima el zorro gris pampeano (*Lycalopex gymnocercus*) por script en Blender (probado con Blender 5.2), al piso de calidad de la v13. Como la vaca, es el caballo con otra anatomía: `animal_rig.Steps` ejecuta `horse.py` sin su `main()` y le reemplaza los nombres (el esqueleto, la escultura, la pintura de la escultura, los pesos, las tarjetas de pelo, la cara de la cabeza, los planes de marcha y las funciones de los clips). Es el zorro del juego (la v5, aprobada el 2026-11-01), en `assets/models/fauna/own/`; reemplaza al zorro de Quaternius, que se borró. En el juego (`Species::Fox`) sus materiales `Main` y `Hair` se dibujan con sus mapas tal como están (`Paint::Keep`), y sus velocidades (`FOX_WALK_SPEED`, `FOX_GALLOP_SPEED`) y su cruz (`FOX_WITHERS`, 0.38 m) son las medidas de este modelo. Para volver a generarlo (unos 4 minutos en la CPU):

```sh
blender -b --factory-startup --python tools/fox.py -- fox_raw.glb \
  --textures assets/models/fauna/own [--preview /tmp/fox]
FORCE=1 sh tools/textures_to_ktx2.sh assets/models/fauna/own/fox_body assets/models/fauna/own/fox_hair
python3 -I tools/glbtrim.py fox_raw.glb assets/models/fauna/own/fox.glb \
  --keep Idle Idle_2 Idle_Headlow Eating Walk Gallop Gallop_Jump --smooth-angle 0
```

| Opción | Significado |
|---|---|
| `--textures DIR` | escribe las fuentes JPG de `DIR/fox_body` (color, normal_gl, roughness, ao) y `DIR/fox_hair` (color, normal_gl, roughness, opacity) |
| `--preview DIR` | renderiza PNG con Eevee: de costado (los dos lados), de tres cuartos de frente y de atrás, de frente, primeros planos de la cabeza (tres vistas), el ojo, el pelaje, la cola, las manos y las patas traseras, y cuadros de los siete clips |
| `--draft`, `--no-clips`, `--shots REGEX` | como en `horse.py` |

**Pelaje.** El zorro tiene un solo pelaje, así que se pinta sobre la escultura (`coat_at`) y se hornea en el mapa de color del cuerpo: el juego dibuja `Main` y `Hair` con sus mapas tal como están (sin `paint` ni colores por vértice). Lomo y flancos canosos con una línea más oscura sobre el espinazo; cabeza leonada rojiza con el hocico gris, los costados del hocico y la barbilla oscuros, y una línea oscura del ojo al hocico; mejillas y garganta blancas, el pecho crema y algo canoso entre las manos, con el borde del blanco irregular (`_wobble`), sin tarjetas encima; orejas grandes, rojizas por detrás y blancas por dentro; panza y cara interna de las patas crema; patas leonadas rojizas por fuera, con una mancha oscura detrás de cada garrón; cola tupida gris, más oscura arriba, con la mancha de la glándula y la punta negra. El canoso viene del pelo: el material de la escultura dibuja pelos finos agrupados en mechones según su caída (`hair_direction`) y, donde el pelaje es canoso, el moteado agutí de los pelos de guarda (banda clara y punta negra, cada pelo con la suya). Las tarjetas (`fur_atlas`) son mechones de pelos finos de su propio largo y su propio color (gris en la raíz, banda clara y punta negra desplazadas al azar, un tercio de borra corta y clara debajo); cada mechón toma la columna del atlas que corresponde al pelaje debajo de él (`card_column`).

Anatomía: 0.38 m a la cruz, cola de 0.37 m llevada hacia atrás y abajo en diagonal, en un cepillo tupido; patas finas sobre manos digitígradas (cuatro dedos con sus almohadillas y uñas, la almohadilla principal atrás, el espolón alto en las manos), tronco compacto de pecho hondo y angosto pero chato por delante (de costado la garganta baja casi recta a las manos, el pecho apenas delante de los codos y no más bajo que ellos: `TRUNK`, `BRISKET`, `chest_front`, `chest_bottom`; el babero blanco es solo color) y panza recogida (la anatomía se diseña larga y `warp` acorta el tronco entre la cruz y la cadera y el cuello sobre los hombros; el pelaje y las regiones de la piel leen cada punto con `unwarp`), dedos separados por surcos, hocico largo y fino, orejas de 7 cm, anchas en la base y ahuecadas por delante. Las manos y patas paradas usan el cabestrillo torácico (`stand_legs`): al bajar la cabeza al suelo el pecho se hunde entre las paletas y las manos quedan casi verticales. Las tarjetas: la gorguera de las mejillas, los pelos largos de la nuca, la cruz y el espinazo, el "pantalón" detrás de los muslos (que sigue al fémur), el fleco de la panza, los codos, los pelos blancos dentro de las orejas, los bigotes negros y unos 400 mechones de la cola, cada uno de su largo, ancho y ángulo (ninguno bajo la raíz, donde quedarían colgando detrás del muslo). El rig conserva los nombres y la jerarquía del cuadrúpedo de Quaternius (los del zorro que dibuja hoy el juego) con dos huesos por oreja (`Ear1`, `Ear2`), `Jaw` y nueve de cola: 49 huesos. Pesos: calor (con el zorro escalado cuatro veces para el solucionador), la cola y las orejas atadas a sus propias cadenas (`tail_only`, `ears_only`) y los codos como en el caballo. Clips (los siete del ganado, en ese orden después de `glbtrim.py --keep`): atento girando la cabeza y las orejas, mirar atrás y olfatear el aire, rastrear con la nariz en el suelo, comer desgarrando y masticando, la caminata lateral (0.54 m/s medidos como el juego) y el galope rotatorio con el lomo que se flexiona, las orejas atrás y la cola tendida (4.21 m/s), y en `Gallop_Jump` el salto sobre el ratón (el juego no lo reproduce en los animales silvestres). Materiales: `Main`, `Hair`, `Eye_Black` y `Eye_Iris` (ámbar, pupila ovalada vertical).

Costo: 12.524 triángulos (el cuerpo unos 5.200, las tarjetas con alpha test y los ojos). Video: un conjunto de mapas de 1024² para el cuerpo y uno de pelo. Tests sin Blender (`test_fox.py`): el pelaje, los colores de los pelos de las tarjetas, el perfil del pecho, las velocidades de las marchas y el esqueleto.

## `tack.py`: los mapas del recado criollo

`tack.py` pinta con numpy (sin Blender ni imágenes de terceros) cuatro de los conjuntos de mapas de los aperos del caballo ensillado (`pampa_render::character::tack`), en `assets/models/tack/` (los otros dos los hace `recado.py`):

```sh
python3 -I tools/tack.py assets/models/tack
FORCE=1 sh tools/textures_to_ktx2.sh assets/models/tack/*/
```

| Conjunto | Tamaño | Qué viste |
| --- | --- | --- |
| `tack_blankets` | 1024² | la bajera (mitad izquierda: jerga de rayas) y la matra (mitad derecha: guarda escalonada en rojo, índigo y crudo, rayas adelante y atrás y los flecos de la urdimbre en la opacidad) |
| `tack_leather` | 512² | la suela de la carona (doble línea labrada a fuego con puntos estampados y canto bruñido), los bastos y las aciones |
| `tack_rawhide` | 512² | el cuero crudo: la trenza redonda de cuatro tientos (riendas y cabezada), una tira lisa (cinchón y látigo) y la cincha de tientos con su panel tejido |
| `tack_iron` | 256² | el hierro forjado del freno, las argollas y los estribos, martillado y con óxido |

También pinta el carpincho y la lana que `recado.py` hornea en su atlas. Cada conjunto lleva `color.jpg`, `normal_gl.jpg`, `roughness.jpg` y `ao.jpg` (más `opacity.jpg` las mantas), las fuentes editables que `textures_to_ktx2.sh` convierte. El ruido tiene semilla, así que la salida es la misma en cada corrida. Costo: unos 6,6 MB de video entre los cuatro con sus mips (BC7, un byte por texel), compartidos por todos los caballos. Tests: `python3 -I -B -m unittest discover -s tools -p test_tack.py`.

## `recado.py`: el recado y la brida modelados en Blender

`recado.py` modela sobre el caballo propio en reposo (`assets/models/fauna/own/horse.glb`) las piezas del recado que necesitan volumen y la brida, y les da skinning a su rig:

```sh
blender -b --factory-startup --python tools/recado.py -- assets/models/tack/recado.glb \
  --textures assets/models/tack [--preview /tmp/recado]
FORCE=1 sh tools/textures_to_ktx2.sh assets/models/tack/tack_recado assets/models/tack/tack_wool
```

- **Los bastos**: dos rollos de suela sobre la carona, a cada lado de la columna, con su costura y sus puntas planas labradas, que asoman adelante y atrás del cojinillo.
- **El cojinillo**: un cuero de oveja sobre ellos, con la lana espesa sobre el asiento y más fina hacia el ruedo, que cae adelante, atrás y por los costados, con bultos en su superficie y los rizos en sus mapas; mechones anchos con alpha test, ondulados y deshilachados en la punta, se apoyan sobre la lana y cuelgan del ruedo en grupos al azar.
- **El sobrepuesto**: de carpincho, sobre la lana aplastada del medio: el asiento (su marca `Mark_Seat`).
- **La cabezada**: testera, frontalera, quijeras, muserola, fiador y barbada, tiras planas apoyadas sobre la piel de la cabeza siguiendo los puntos del cráneo (nuca, orejas, ojos, quijada, comisuras).
- **El freno**: el bocado cruza la boca entre las comisuras (las encuentra por los pesos de los labios), las piernas curvas de hierro se apoyan contra los labios, con su ojo arriba y la argolla de las riendas abajo (`Mark_ReinL`, `Mark_ReinR`).

Cada pieza toma los pesos de la piel que tiene debajo, sin los de los controles de IK. Las del lomo (los bastos, el cojinillo, el sobrepuesto y los mechones) solo con los huesos del tronco (`Body`, `Torso`, `Torso2`, `Torso3`): la piel de la cruz lleva pesos de la crin del cuello y la de los costados de las patas, que levantarían piezas a 10 cm de la piel; los bastos y el sobrepuesto, rígidos (los mismos pesos en todos sus vertices). La testera va con la cabeza y lo alto del cuello como la piel de la nuca (sin orejas ni crin); la frontalera, solo con la cabeza. Después el script reproduce cada clip que juega un caballo ensillado (no `Eating`: los ensillados no pastan) y aparta de la piel lo justo cada vertex al que la piel en movimiento se le acercó a menos de 3,5 mm; cada canto de una tira se apoya sobre la piel, así que la tira se dobla con la cabeza. El atlas `tack_recado` (1024²: el cojinillo, los bastos y el sobrepuesto sobre sus propias UV) y los mechones `tack_wool` (512², con opacidad) se pintan con numpy, con las funciones de `tack.py`. `--preview DIR` hace PNG del recado sobre el caballo. Costo: 6.730 triángulos (1.088 de los bastos, 1.840 del cojinillo, 608 del sobrepuesto, 750 de mechones, 1.400 de la cabezada y 1.044 del freno) y unos 5,2 MB de video entre sus dos conjuntos. Tests (sin Blender): `python3 -I -B -m unittest discover -s tools -p test_recado.py`.
