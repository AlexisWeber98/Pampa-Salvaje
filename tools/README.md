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
for pair in "cow_a cow" "bull bull" "horse_a horse"; do
  set -- $pair
  python3 -I tools/glbtrim.py ~/Downloads/quaternius/$1.glb \
    assets/models/fauna/quaternius/$2.glb \
    --keep Idle Idle_2 Idle_Headlow Eating Walk Gallop Gallop_Jump
done
```

Los clips se escriben en el orden de `--keep`, del que depende el juego (`pampa_render::fauna::Clip`). Imprime el tamaño antes y después, los triángulos, las articulaciones y los clips conservados.

También recalcula normales de vértice suaves, porque las exportaciones tienen sombreado plano y los cuerpos se ven facetados. Esto está activado por defecto; `--smooth-angle DEGREES` fija el ángulo (60 por defecto) y `--smooth-angle 0` conserva las normales del archivo.

- Solo para las normales, los vértices se sueldan por posición exacta y `JOINTS_0`/`WEIGHTS_0` iguales, así que nada se suaviza entre skinnings distintos. La soldadura abarca todas las primitivas de una mesh, así que no se ve ninguna costura donde se encuentran dos materiales.
- Alrededor de cada punto soldado, las caras que comparten una arista que se dobla menos que el ángulo forman un grupo suave, como el auto smooth. Cada vértice recibe las normales de cara de su grupo, ponderadas por el ángulo de la esquina, así que los bordes de los cascos y las puntas de las orejas y los cuernos quedan definidos.
- Solo cambian los bytes de NORMAL: la cantidad de vértices, las posiciones, las UV, los colores y el tamaño del archivo quedan iguales.

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
