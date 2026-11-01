# Modelos propios de fauna

Modelos hechos para el juego, sin assets de terceros: no necesitan crédito.
Se generan con los scripts de Blender de `tools/` (ver `tools/README.md`).

| Archivo | Contenido | Script |
|---|---|---|
| `rhea.glb` | ñandú (*Rhea americana*): mesh con skinning, materiales `Body` y `Feathers`, clips | `tools/rhea.py`, después `tools/glbtrim.py` |
| `rhea_body/` | mapas del cuerpo, horneados de la escultura: fuentes JPG y KTX2 | `tools/rhea.py --textures`, después `tools/textures_to_ktx2.sh` |
| `rhea_feathers/` | atlas de plumas sueltas con su opacidad: fuentes JPG y KTX2 | ídem |
| `horse.glb` | caballo criollo: mesh con skinning (11.944 triángulos), rig de 62 huesos, los materiales del caballo de Quaternius, siete clips | `tools/horse.py`, después `tools/glbtrim.py` |
| `horse_body/` | mapas de la piel, horneados de la escultura (luminancia neutra, normal map, rugosidad, oclusión): fuentes JPG y KTX2 | `tools/horse.py --textures`, después `tools/textures_to_ktx2.sh` |
| `horse_hair/` | atlas de mechones de crin, copete y cola con su opacidad: fuentes JPG y KTX2 | ídem |
| `cow.glb`, `bull.glb` | vaca y toro (Hereford, Angus, overo y Shorthorn rosillo): mesh con skinning (8.192 y 8.792 triángulos) más su malla liviana `Cow_LOD` (1.400) para las tropas lejanas, rig de 58 huesos, materiales `Main`, `Main_Light`, `Muzzle`, `Hooves`, `Horns`, `Hair` y los ojos, siete clips | `tools/cattle.py` (`--bull` para el toro), después `tools/glbtrim.py` |
| `cow_body/`, `bull_body/` | mapas de la piel horneados de la escultura (luminancia neutra, normal map, rugosidad, oclusión) y el mapa de color de cada pelaje (`hereford`, `angus`, `overo` de 1024², `roan` de 2048²): fuentes JPG y KTX2 | `tools/cattle.py --textures`, después `tools/textures_to_ktx2.sh` (ver `tools/README.md`) |
| `cattle_hair/` | atlas de la borla de la cola, los rulos de la nuca, los flecos de las orejas y las pestañas, con su opacidad: fuentes JPG y KTX2 | ídem |
| `fox.glb` | zorro gris pampeano (*Lycalopex gymnocercus*): mesh con skinning (12.524 triángulos), rig de 49 huesos, materiales `Main`, `Hair`, `Eye_Black` y `Eye_Iris`, siete clips | `tools/fox.py`, después `tools/glbtrim.py` |
| `fox_body/` | mapas de la piel horneados de la escultura, con el pelaje ya pintado (color, normal map, rugosidad, oclusión): fuentes JPG y KTX2 | `tools/fox.py --textures`, después `tools/textures_to_ktx2.sh` |
| `fox_hair/` | atlas de mechones de pelo con sus propios colores (canosos, crema, leonados, oscuros y negros) y su opacidad: fuentes JPG y KTX2 | ídem |
