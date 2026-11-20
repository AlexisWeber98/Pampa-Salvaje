# Créditos

This work is based on "Horse" (https://sketchfab.com/3d-models/horse-a6f860e43e364619bccb174a1ac7d0c9) by henrysteve973 (https://sketchfab.com/henrysteve973) licensed under CC-BY-4.0 (http://creativecommons.org/licenses/by/4.0/)

- Título: Horse
- Autor: henrysteve973 (https://sketchfab.com/henrysteve973)
- Fuente: https://sketchfab.com/3d-models/horse-a6f860e43e364619bccb174a1ac7d0c9 (descargado el 2026-11-13, glTF)
- Licencia: CC BY 4.0 (http://creativecommons.org/licenses/by/4.0/); hay que acreditar al autor, el uso comercial está permitido.

Cambios: convertido por `tools/henry_horse.py`. Se quitaron la montura, la brida y los huesos de los estribos; las transformaciones de los nodos envoltorio y la escala de la armadura se hornearon en un esqueleto en metros, mirando hacia +Z y parado en el suelo; los clips se renombraron (se corrigió la errata "Ilde") y se reordenaron; las texturas se achicaron a 2048 px (cuerpo y pelo) y 512 px (herraduras), el gris del pelo se convirtió en un mapa de opacidad, el normal map de las herraduras pasó de DirectX a OpenGL, y todo se convirtió a KTX2; al loop de `Gallop` se le agregó al final su primera clave, un cuadro después de la última (`CLOSE_LOOPS`).

Derivados de este modelo: los mapas de color de los cinco pelajes criollos de `horse_body/` (`tools/henry_coats.py`, pintados sobre su pelo sin su overo), el recado y la brida (`assets/models/tack/recado.glb`, `tools/recado.py`) y la guarnición de tiro (`assets/models/vehicles/harness.glb`, `tools/vehicle_harness.py`), modelados sobre él y con skinning sobre su esqueleto, parado como al empezar `Idle_1`.
