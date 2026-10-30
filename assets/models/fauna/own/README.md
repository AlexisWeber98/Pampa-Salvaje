# Modelos propios de fauna

Modelos hechos para el juego, sin assets de terceros: no necesitan crédito.
Se generan con los scripts de Blender de `tools/` (ver `tools/README.md`).

| Archivo | Contenido | Script |
|---|---|---|
| `rhea.glb` | ñandú (*Rhea americana*): mesh con skinning, materiales `Body` y `Feathers`, clips | `tools/rhea.py`, después `tools/glbtrim.py` |
| `rhea_body/` | mapas del cuerpo, horneados de la escultura: fuentes JPG y KTX2 | `tools/rhea.py --textures`, después `tools/textures_to_ktx2.sh` |
| `rhea_feathers/` | atlas de plumas sueltas con su opacidad: fuentes JPG y KTX2 | ídem |
