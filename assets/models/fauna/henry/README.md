# Caballo de henrysteve973

El "Horse" de henrysteve973 (Sketchfab, CC BY 4.0; ver `CREDITS.md`), convertido para el juego por `tools/henry_horse.py` (ver `tools/README.md`). Es el caballo del juego (`Species::Horse` en `crates/render/src/fauna/livestock.rs`: los de corral, los ensillados y los de tiro); reemplaza a `../own/horse.glb`. Los aperos, el jinete y los arneses todavía están hechos para el caballo anterior (H5–H7 de `odd/tasks/henry-horse.md`).

| Archivo | Contenido |
|---|---|
| `horse.glb` | mesh con skinning `Horse` (20.049 triángulos), 96 articulaciones, 28 clips; materiales sin texturas (el juego pone las suyas) |
| `horse_body/` | piel del caballo (`Horse`): `color.jpg` (el overo propio del modelo, que el juego no dibuja), `normal_gl.jpg` y `roughness.jpg` de 2048², y el mapa de color de cada pelaje criollo (`zaino.jpg`, `tordillo.jpg`, `overo.jpg`, `alazan.jpg`, `bayo.jpg`, de 2048², `tools/henry_coats.py`), y sus KTX2 |
| `horse_hair/` | atlas de las tarjetas de la crin, el copete y la cola (`Hair`): `color.jpg`, `opacity.jpg`, `normal_gl.jpg` y `roughness.jpg` de 2048², y sus KTX2 (el color lleva la opacidad en el alfa) |
| `horse_shoe/` | herraduras (`Horseshoe`): `color.jpg`, `normal_gl.jpg` y `roughness.jpg` de 512², y sus KTX2 |
| `CREDITS.md` | el crédito exigido por la licencia |

Para volver a generarlo (unos 40 segundos; el reporte de los clips, unos 30 más):

```sh
python3 -I tools/henry_horse.py ~/Downloads/sketchfab/henry_horse assets/models/fauna/henry
FORCE=1 sh tools/textures_to_ktx2.sh assets/models/fauna/henry/horse_body \
  assets/models/fauna/henry/horse_hair assets/models/fauna/henry/horse_shoe
# Los pelajes (unos 40 segundos), cada uno convertido como el color de un
# conjunto (el normal map y la rugosidad solo completan el conjunto).
O=assets/models/fauna/henry/horse_body
python3 -I tools/henry_coats.py ~/Downloads/sketchfab/henry_horse assets/models/fauna/henry
for coat in zaino tordillo overo alazan bayo; do
  t=$(mktemp -d)
  cp $O/$coat.jpg $t/color.jpg
  cp $O/normal_gl.jpg $O/roughness.jpg $t/
  sh tools/textures_to_ktx2.sh $t && mv $t/color.ktx2 $O/$coat.ktx2
  rm -r $t
done
```

## Malla

Una sola mesh `Horse` con una primitiva por material, en este orden:

| Material | Triángulos | Texturas |
|---|---|---|
| `Horse` | 12.602 | `horse_body/`: en el juego, el mapa de su pelaje en lugar de `color` |
| `Hair` | 6.103 | `horse_hair/`, con alpha test (`alphaMode` `MASK`, corte 0,5) |
| `Cornea` | 400 | ninguna: casi negro (`baseColorFactor` 0,002) |
| `Horseshoe` | 944 | `horse_shoe/` |

Se quitaron la montura y la brida (material `Saddle`, 10.701 triángulos). Todos los materiales son de doble cara, sin metalicidad, con rugosidad 0,82 (el original no trae mapas de rugosidad; `roughness.jpg` es ese valor parejo).

## Esqueleto

96 articulaciones `BN_*` (las 100 del original menos los dos huesos de cada estribo, `BN_*_Stirrup_*`, que solo movían la montura). La raíz `_rootJoint` es la identidad; debajo, `BN_Root_01_01` lleva el giro del original. Ninguna articulación tiene escala, las traslaciones están en metros y las matrices de bind inversas son rígidas. La pose de bind de los vértices no es la pose de reposo de los nodos (así viene el original; Blender lo importa bien con `guess_original_bind_pose=False`); el juego siempre reproduce un clip.

Medidas en el primer cuadro de `Idle_1`: mira hacia +Z con Y hacia arriba, los vasos en y = 0 y su centro en el origen; la nuca a 2,04 m, la articulación `BN_Spine_03_05_05` (la cruz) a 1,37 m, la pelvis a 1,33 m; de la cola al hocico, de z = -1,00 a 1,56 m.

## Clips

En este orden (`CLIPS` en `tools/henry_horse.py`). Los siete primeros ocupan los lugares de los clips del caballo propio (`Idle`, `Idle_2`, `Idle_Headlow`, `Eating`, `Walk`, `Gallop`, `Gallop_Jump`). Todos son lineales, a 24 cuadros por segundo.

| # | Clip | Nombre original | Duración (s) | Raíz (dz, m) | Pelvis y (m) | Velocidad (m/s) |
|---|---|---|---|---|---|---|
| 0 | `Idle_1` | `1 Ilde` | 5,958 | 0 | 1,33 | |
| 1 | `Idle_2` | `2 Ilde` | 1,875 | 0 | 1,32–1,33 | |
| 2 | `Idle_3` | `3 Ilde` | 2,917 | 0 | 1,30–1,33 | |
| 3 | `Eat` | `6 Eat` | 7,667 | 0 | 1,25–1,34 | |
| 4 | `Walk` | `Walk` | 1,000 | 0 | 1,32–1,33 | 0,98 |
| 5 | `Gallop` | `Gallop` | 1,000 | 0 | 1,08–1,33 | 4,50 |
| 6 | `Jump` | `Jump` | 2,333 | 0 | 0,85–2,90 | |
| 7 | `Jump_Land` | `Jump_Land` | 2,000 | +0,85 | 0,72–2,20 | |
| 8 | `Jump_Run` | `Jump_Run` | 2,000 | 0 | 1,02–2,20 | |
| 9 | `S_Jump` | `S.Jump` | 1,500 | 0 | 0,86–1,93 | |
| 10 | `Walk_L` | `Walk_L` | 1,000 | 0 | 1,32–1,33 | 1,01 |
| 11 | `Walk_R` | `Walk_R` | 1,000 | 0 | 1,32–1,33 | 1,01 |
| 12 | `Gallop_L` | `Gallop_L` | 0,958 | 0 | 1,08–1,33 | 4,20 |
| 13 | `Gallop_R` | `Gallop_R` | 0,958 | 0 | 1,08–1,33 | 4,37 |
| 14 | `Run_Stop` | `Run_Stop` | 2,000 | 0 | 0,77–1,42 | |
| 15 | `Attack_1` | `4 Attack_1` | 1,333 | 0 | 1,22–1,33 | |
| 16 | `Attack_2` | `5 Attack_2` | 1,333 | 0 | 1,24–1,47 | |
| 17 | `Buck_1` | `Buck_1` | 3,333 | 0 | 1,15–1,33 | |
| 18 | `Buck_2` | `Buck_2` | 4,792 | 0 | 1,01–1,34 | |
| 19 | `Buck_3` | `Buck_3` | 4,583 | 0 | 0,96–2,02 | |
| 20 | `Arise` | `Arise` | 3,167 | 0 | 0,22–1,33 | |
| 21 | `Sleep` | `Sleep` | 3,292 | 0 | 1,32 | |
| 22 | `Falling` | `Falling` | 2,667 | 0 | 2,48–2,51 | |
| 23 | `Fallen` | `Fallen` | 2,000 | 0 | 0,24–2,40 | |
| 24 | `Death` | `Death` | 3,000 | 0 | 0,29–1,33 | |
| 25 | `Swim` | `Swim` | 0,833 | 0 | 1,27–1,28 | 4,41 |
| 26 | `Swim_L` | `Swim_L` | 0,875 | 0 | 1,27–1,28 | 4,21 |
| 27 | `Swim_R` | `Swim_R` | 0,875 | 0 | 1,27–1,28 | 3,98 |

- **Raíz:** cuánto avanza `BN_Root_01_01` del primer al último cuadro. Solo `Jump_Land` tiene movimiento de raíz (0,85 m hacia adelante, sin desplazamiento lateral ni vertical); los demás se reproducen en el lugar.
- **Pelvis y:** la altura de la pelvis a lo largo del clip. El cuerpo sube y baja dentro de los clips: en `Jump` la pelvis llega a 2,90 m (el vuelo está en el clip, no en la raíz), en `Jump_Run` y `Fallen` a 2,20 y 2,40 m. `Falling` es una caída en el aire (la pelvis queda a 2,5 m y los vasos a más de 0,43 m del suelo en todo el clip), y `Swim` nada a la altura de parado: el juego tiene que hundir el cuerpo.
- **Velocidad:** solo en los ciclos que se reproducen en el lugar: cuánto se deslizan hacia atrás los vasos apoyados (el centro de cada herradura, a menos de 5 cm de su punto más bajo y yendo hacia atrás), muestreado a 96 por segundo. Es la velocidad sobre el suelo del clip. El caballo no tiene trote (S6 de `odd/tasks/henry-horse.md`).
- **`Gallop` cerrado:** el original termina un cuadro antes de volver a su primera pose (dura 0,958 s y la mano de galope, apoyada, saltaba 21 cm en cada vuelta del ciclo). `tools/henry_horse.py` le agrega una clave final, la primera, a 1,000 s (`CLOSE_LOOPS`); el resto del archivo queda igual byte a byte. `Walk`, `Gallop_L`, `Gallop_R` y los `Swim` muestran el mismo cuadro de menos (en `Walk` los vasos apoyados saltan 3 cm) y no se tocaron; las esperas y `Eat` cierran bien.
- El salto (S3): `Jump` es el despegue y el vuelo, `Jump_Run` el aterrizaje que sigue galopando, `Jump_Land` el que se detiene y `S_Jump` un salto corto completo desde parado o al paso.

## En el juego

- **Tamaño:** la cruz (lo más alto del lomo donde empieza la crin) a 1,60 m en el primer cuadro de `Idle_1` (`HORSE_WITHERS`); el juego la lleva a 1,5 m (escala 0,9375). Así la caminata muestra 0,92 m/s y el galope 4,22 m/s en el juego, reproducidos tal como se grabaron. La caminata más rápida reparte lo que falta entre el ritmo y el tranco; el galope va al ritmo de su vaso apoyado mientras apoya y a 1,25 veces en la suspensión, así que alarga el tranco (S7 de `odd/tasks/henry-horse.md`).
- **Clips:** el juego carga los nueve primeros menos `S_Jump` (`Clip` y `HENRY_CLIPS`); el juego no tiene saltos desde parado ni al paso (solo se salta a galope tendido), así que `S_Jump` y los demás quedan en el archivo para más adelante.
- **Salto:** `Jump` empieza en el primer cuadro de `Gallop` y `Jump_Run` termina en él. Las patas traseras dejan el suelo en 0,47 s de `Jump` (`JUMP_TAKEOFF`); el primer vaso toca el suelo en 0,18 s de `Jump_Run` y en 0,23 s de `Jump_Land` (`touchdown`): ese tramo, unos 2,05 s del clip, se reparte en el vuelo de la física (1,12 s). El juego baja el modelo lo que el clip sube el cuerpo, para que el vaso más bajo siga el arco de la física, y le quita a `Jump_Land` el movimiento de raíz.
- **Pelajes:** los cinco criollos de `Coat` (zaino, tordillo, overo, alazán, el de Don Hidalgo, y bayo), cada uno horneado en su mapa de `horse_body/` (`Coat::map_name`, `Paint::Map`); la crin, el copete y la cola tiñen el atlas gris de `horse_hair/` con el color de la crin del pelaje (`Coat::mane`). Ver `henry_coats.py` en `tools/README.md`.
- **Huesos:** el juego los pide por su papel (`crates/render/src/character/bones.rs`): la raíz `BN_Root_01_01` y las puntas de los vasos `BN_*_Toe_*_0_*`, a 0,09 m del suelo con el vaso apoyado (`TOE_STANDING`).
- **Pisadas:** el galope es a mano derecha (apoyan la pata izquierda, la derecha, la mano izquierda y la derecha); la mano izquierda apoya en 0,656 del ciclo del galope y en 0,115 del de la caminata (`GALLOP_PLANT`, `WALK_PLANT`).

Si cambia un clip, hay que volver a medir estas constantes (`tools/henry_horse.py` imprime el reporte de los clips).

Se revisaron los 28 clips renderizados en Blender (cuatro cuadros por clip): la crin no se desarma y los vasos quedan en el suelo en los clips de pie (el vaso más bajo, entre -6 y +4 cm del suelo en cada cuadro).
