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
