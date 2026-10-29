# Personajes

Personas de MakeHuman con skinning. Cada carpeta contiene el `.mhm` fuente de un personaje (el archivo de modelo de MakeHuman, escrito a mano), los materiales recoloreados que tenga (`.mhmat`) y el glTF construido a partir de ellos: `model.gltf`, `model.bin` y `textures/`. Los créditos y las sustituciones están en [CREDITS.md](CREDITS.md).

El juego carga `protagonist`, `hidalgo`, `farach`, `bairoletto` y los cinco atuendos de los vecinos, `townsfolk_paisano`, `townsfolk_peon`, `townsfolk_volga`, `townsfolk_headscarf` y `townsfolk_bun` (`crates/render/src/character/model.rs`), cada uno una sola vez, y los dibuja por defecto; el `models off` de la consola de depuración trae de vuelta las viejas figuras rígidas. Un vecino lleva el atuendo de su aspecto (`look % 5`); el resto del aspecto elige, para cada prenda, uno de cuatro colores apagados, el primero el propio del modelo (`crates/render/src/character/townsfolk.rs`). Un material teñido es una copia que se hace una sola vez por (material, color) y que comparten todos los que lo usan. Las dos mujeres llevan polleras largas: el juego les mantiene los brazos 0.14 rad hacia afuera, para que las manos descansen sobre la pollera en lugar de hundirse en ella. El protagonista usa su modelo a pie y en la montura: allí se sienta sobre el hueso `Torso` del caballo y reproduce el clip sentado `drive`, con las piernas dobladas hacia los estribos y la mano hacia las riendas mediante un pequeño IK, con resortes para la holgura de su asiento, columna, cabeza y manos (`crates/render/src/character/rider.rs`). `models off` solo se aplica a las personas a pie. El conversor mantiene el poncho del protagonista apartado de su brazo izquierdo (`clear_arms` en `tools/mh2gltf.py`), así que queda sobre el hombro cuando el brazo se mueve.

Se mueven con clips de animación, todos en un único archivo compartido, `animations/humanoid.glb`: las Universal Animation Library 1 y 2 de Quaternius, CC0, adaptadas (retargeting) al esqueleto `game_engine` con `tools/retarget.py`. El controlador de clips (`crates/render/src/character/rig.rs`, con `clips.rs`) funciona así:

- la espera se funde con la caminata, el trote y la carrera según la velocidad sobre el suelo;
- cada clip de caminata se ajusta al ritmo de la velocidad sobre el suelo, según las piernas de la persona frente a las del protagonista, para que los pies queden quietos sobre el suelo y las personas más bajas den pasos más cortos y rápidos;
- la caminata es la caminata de juego del paquete, que la herramienta endereza hasta convertirla en una caminata de calle tranquila y erguida (rodillas más rectas, el torso arriba, los hombros abajo, los brazos balanceándose a los costados), con un tranco más largo;
- Farach espera con los brazos cruzados, y quien habla lo hace con las manos;
- cada persona empieza sus clips en su propio punto;
- la cabeza mira alrededor por encima del clip;
- un jinete reproduce solo el clip sentado: un clip con peso cero igual fija los huesos que solo él anima.

Los clips encuentran los huesos por la ruta de sus nombres desde `Root` (`Root/pelvis/spine_01/...`), así que un modelo reconstruido debe conservar los nombres y la jerarquía de huesos del esqueleto `game_engine`. Los clips se hicieron sobre el protagonista. A otro modelo se le escalan los movimientos de la cadera a sus propias piernas cuando los reproduce.

## Reconstruir los clips

Se necesitan los dos paquetes (ver [CREDITS.md](CREDITS.md); no están en el repositorio). Ejecutar, desde la raíz del repositorio:

```sh
python3 -I tools/retarget.py \
  --ual1 "PATH/Animation Library[Standard]/Godot/AnimationLibrary_Godot_Standard.glb" \
  --ual2 "PATH/Universal Animation Library 2 [Standard]/Unreal-Godot/UAL2_Standard.glb" \
  --target assets/characters/protagonist/model.gltf \
  --model assets/characters/protagonist/model.gltf --model assets/characters/hidalgo/model.gltf \
  --model assets/characters/farach/model.gltf --model assets/characters/bairoletto/model.gltf \
  --talker assets/characters/townsfolk_paisano/model.gltf --talker assets/characters/townsfolk_peon/model.gltf \
  --talker assets/characters/townsfolk_volga/model.gltf --talker assets/characters/townsfolk_headscarf/model.gltf \
  --talker assets/characters/townsfolk_bun/model.gltf \
  -o assets/characters/animations/humanoid.glb
```

La salida es determinista. Imprime cada clip con su duración y, para las caminatas y carreras, la velocidad sobre el suelo y el tranco que midió. El `Clip::ground_speed` del juego (`crates/render/src/character/clips.rs`) debe coincidir con ellos; un test lo comprueba. Las líneas `--model` indican los hombres cuyo pecho deben esquivar los brazos cruzados y las manos que hablan; las líneas `--talker`, los vecinos, cuyo pecho solo deben esquivar las manos que hablan (nunca cruzan los brazos). Los clips de pie también se calman para sacarlos de la guardia de boxeador de los paquetes (ver `tools/README.md`). Reconstruir los clips después de reconstruir cualquier modelo.

## Reconstruir un modelo

Se necesitan la carpeta de datos de MakeHuman 1.x (solo se leen los datos; la GUI nunca se inicia), los paquetes de assets de la comunidad que todavía usan los personajes (ver [CREDITS.md](CREDITS.md)) instalados en ella o en la carpeta de datos de usuario de MakeHuman, y Python 3 con numpy:

```sh
python3 -I tools/mh2gltf.py assets/characters/farach/farach.mhm -o assets/characters/farach
python3 -I tools/mh2gltf.py --check assets/characters/farach/model.gltf
```

La salida es determinista. Una textura que un cambio ya no usa queda en `textures/` hasta que se la borre: cada archivo allí debería estar nombrado por su `model.gltf`.

## Escribir un .mhm

Estas son las líneas propias de MakeHuman (ver `tools/README.md` para saber qué lee el conversor):

- `modifier macrodetails/Age 0.538462`: la edad es `0.5` a los 25 años y `1.0` a los 90. Los años por encima de 25 se corresponden con `0.5 + (years - 25) / 130`.
- `proxy`, `eyes`, `eyebrows`, `hair`, `clothes`: un nombre y el UUID del asset, que es el que cuenta.
- `material <name> <uuid> <file>`: un material para ese proxy. El archivo se busca primero junto al `.mhm`, y luego en las carpetas de datos.
- `skeleton game_engine.mhskel`, `skinMaterial skins/.../x.mhmat`, `clothesHideFaces True`.

Un `.mhmat` recoloreado apunta a la textura propia del asset con una ruta `data/` (por ejemplo `data/clothes/grinsegold_moustache/Moustache_black_diff.png`). Su `diffuseColor` multiplica esa textura. Puede oscurecer o teñir un asset, pero no aclararlo, salvo que `greyscaleTexture True` convierta antes la textura a gris y blanco (el pelo blanco de Don Hidalgo). Sin un `diffuseTexture`, la prenda es de un color liso (la bombacha de Bairoletto).

El conversor también lee líneas propias. MakeHuman avisa sobre ellas y las omite, así que los archivos siguen cargando allí:

| Línea | Significado |
|---|---|
| `decimate <name> <uuid> <triangles>` | Simplifica la prenda a lo sumo a esa cantidad de triángulos (se conservan las costuras de UV y los bordes, así que puede quedar por encima). |
| `piece <kind> key=value ...` | Una prenda recortada del cuerpo (`shirt`, `waistcoat`, `jacket`, `trousers`, `bombacha`, `boots`, `alpargatas`, `shawl`, `headscarf`, `hair`), la `skirt` larga de una mujer y su `apron`, o una pieza generada (`hat`, `kepi`, `boina`, `bun`, `neckerchief`, `sash` (faja), `rastra`, `buttons`, `chain`, `strap`, `poncho`, `lapels`, `collar`). Sus parámetros están en `tools/README.md`. |

La mayor parte de la ropa se recorta del propio cuerpo en lugar de tomarse de paquetes de assets, que se ven modernos: cada prenda es una copia de la piel del cuerpo sobre la parte que cubre, empujada un poco hacia afuera y con el mismo skinning, así que se dobla con el cuerpo. Solo Bairoletto usa ropa de paquetes: los pantalones bombachos como su bombacha y las botas de montar. Las vecinas se recortan del proxy de cuerpo femenino (`female1605`), de la misma forma.
