# Modelos propios de fauna

Modelos hechos para el juego, sin modelos de terceros. Los mapas del cuerpo
de los felinos y de las rapaces llevan, en la cara (y el de la liebre, en todo el cuerpo), una fotografía proyectada
que sí necesita crédito, y el atlas de plumas del carancho, plumas escaneadas
de dominio público (ver abajo y el `CREDITS.md` de la raíz). Se generan con los scripts de Blender de `tools/` (ver `tools/README.md`).

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
| `fox.glb` | zorro gris pampeano (*Lycalopex gymnocercus*): mesh con skinning (12.524 triángulos), rig de 49 huesos, materiales `Main`, `Hair`, `Eye_Black` y `Eye_Iris` (el iris y el ojo pintados en colores de vértice, `COLOR_0`; blancos en la piel y el pelo, así no la tiñen), siete clips | `tools/fox.py`, después `tools/glbtrim.py` |
| `fox_body/` | mapas de la piel horneados de la escultura, con el pelaje ya pintado (color, normal map, rugosidad, oclusión): fuentes JPG y KTX2 | `tools/fox.py --textures`, después `tools/textures_to_ktx2.sh` |
| `fox_hair/` | atlas de mechones de pelo con sus propios colores (canosos, crema, leonados, oscuros y negros) y su opacidad: fuentes JPG y KTX2 | ídem |
| `gato.glb`, `puma.glb`, `yagua.glb` | gato montés (*Leopardus geoffroyi*), puma (*Puma concolor*) y yaguareté (*Panthera onca*): mesh con skinning (11.618, 11.326 y 11.414 triángulos) más su malla liviana (`GatoMontes_LOD`, `Puma_LOD`, `Yaguarete_LOD`, 1.000) para cuando están lejos, rig de 49 huesos, materiales `Main`, `Hair`, `Eye_Black` y `Eye_Iris` (el iris y el ojo pintados en colores de vértice, `COLOR_0`; blancos en la piel y el pelo, así no la tiñen), siete clips | `tools/felid.py --species gato\|puma\|yagua`, después `tools/glbtrim.py` |
| `gato_body/`, `puma_body/`, `yagua_body/` | mapas de la piel horneados de la escultura, con el pelaje ya pintado y la cara de una fotografía (color, normal map, rugosidad, oclusión): fuentes JPG y KTX2 | `tools/felid.py --textures --faces`, después `tools/textures_to_ktx2.sh` |
| `gato_hair/`, `puma_hair/`, `yagua_hair/` | atlas de mechones de pelo con sus propios colores y su opacidad: fuentes JPG y KTX2 | `tools/felid.py --textures`, después `tools/textures_to_ktx2.sh` |
| `hare.glb` | liebre europea (*Lepus europaeus*): mesh con skinning (9.324 triángulos) más su malla liviana `Hare_LOD` (800, la piel sola) para cuando está lejos, rig de 43 huesos, materiales `Main`, `Hair`, `Eye_Black` y `Eye_Iris` (el iris y el ojo pintados en colores de vértice, `COLOR_0`; blancos en la piel y el pelo, así no la tiñen), siete clips | `tools/hare.py`, después `tools/glbtrim.py` |
| `hare_body/` | mapas de la piel horneados de la escultura, con el pelaje ya pintado y el pelo de una fotografía (color, normal map, rugosidad, oclusión), reducidos de 2048² a 1024²: fuentes JPG y KTX2 | `tools/hare.py --textures --photos`, después `tools/textures_to_ktx2.sh` |
| `hare_hair/` | atlas de mechones de pelo con los colores de la liebre y su opacidad: fuentes JPG y KTX2 | `tools/hare.py --textures`, después `tools/textures_to_ktx2.sh` |
| `chimango.glb`, `carancho.glb` | chimango (*Milvago chimango*) y carancho (*Caracara plancus*): mesh con skinning (12.540 y 17.852 triángulos) más su malla liviana (`Chimango_LOD`, `Carancho_LOD`, 946 cada una: la piel diezmada y las plumas de vuelo y de la cola en tarjetas rectas) para cuando están lejos, rig de 23 huesos, materiales `Main`, `Feathers`, `Eye_Black` y `Eye_Iris`, cuatro clips (`Glide`, `Flap`, `Bank_L`, `Bank_R`) | `tools/raptor.py --species chimango\|carancho`, después `tools/glbtrim.py` |
| `chimango_body/`, `carancho_body/` | mapas del cuerpo horneados de la escultura, con el plumaje ya pintado y la cara de una fotografía (color, normal map, rugosidad, oclusión), reducidos de 2048² a 1024²: fuentes JPG y KTX2 | `tools/raptor.py --textures --scans --faces`, después `tools/textures_to_ktx2.sh` |
| `chimango_feathers/`, `carancho_feathers/` | atlas de plumas (remeras, cola, cobijas, escapulares y el copete del carancho) con su opacidad, el del carancho de plumas escaneadas, reducidos de 2048² a 1024²: fuentes JPG y KTX2 | ídem |
| `torcaza.glb`, `picazuro.glb` | torcaza (*Zenaida auriculata*) y picazuró (*Patagioenas picazuro*): mesh con skinning (12.040 y 12.440 triángulos) más su malla liviana (unos 945 triángulos), con el rig de 23 huesos de las rapaces y los clips `Flap`, `Glide` y `FlapGlide`; vuelan en las bandadas del cielo (`crates/render/src/sky/birds.rs`) | `tools/pigeon.py` |
| `torcaza_body/`, `picazuro_body/` | mapas del cuerpo con el plumaje pintado y la cara de una fotografía, reducidos de 2048² a 1024²: fuentes JPG y KTX2 | `tools/pigeon.py --textures` |
| `torcaza_feathers/`, `picazuro_feathers/` | atlas de plumas con su opacidad, de plumas escaneadas, reducidos de 2048² a 1024²: fuentes JPG y KTX2 | ídem |
| `gorrion.glb`, `chingolo.glb` | gorrión (*Passer domesticus*) y chingolo (*Zonotrichia capensis*): mesh con skinning (15.116 y 15.932 triángulos) más su malla liviana (`Gorrion_LOD`, `Chingolo_LOD`, 1.122 cada una), rig de 25 huesos (los 23 de las rapaces y el ala plegada, `Fold.L` y `Fold.R`), materiales `Main`, `Feathers`, `Eye_Iris` y `Eye_Black`, cinco clips (`Flap`, `FlapBound`, `Hop`, `Peck`, `Idle`); comen, saltan y se posan cerca del jugador (`crates/render/src/fauna/sparrow.rs`) | `tools/sparrow.py --species gorrion\|chingolo`, después `tools/glbtrim.py --keep Flap FlapBound Hop Peck Idle --smooth-angle 0` |
| `gorrion_body/`, `chingolo_body/` | mapas del cuerpo con el plumaje pintado y la cara de una fotografía, reducidos de 2048² a 1024²: fuentes JPG y KTX2 | `tools/sparrow.py --textures --scans --faces`, después `tools/textures_to_ktx2.sh` |
| `gorrion_feathers/`, `chingolo_feathers/` | atlas de plumas (remeras, cola, cobijas, el ala plegada y el copete del chingolo) con su opacidad, con la cola y el grano de plumas escaneadas, reducidos de 2048² a 1024²: fuentes JPG y KTX2 | ídem |

## Créditos de las fotografías de las caras

Cada foto se proyectó sobre la cabeza en el mapa de color del cuerpo (`tools/felid.py --faces`: ajustada a los puntos de referencia de la cabeza, sin su propia luz, con el color igualado al del pelaje y mezclada con el pelaje procedural).

| Mapa | Foto | Autor | Licencia |
|---|---|---|---|
| `puma_body/` | [Cougar (17575969543)](https://commons.wikimedia.org/wiki/File:Cougar_(17575969543).jpg), Wikimedia Commons | Pete G | [CC BY 2.0](https://creativecommons.org/licenses/by/2.0/) |
| `yagua_body/` | [Jaguar, Tennōji Zoo (33686534416)](https://commons.wikimedia.org/wiki/File:Jaguar,_Tenn%C5%8Dji_Zoo_(33686534416).jpg), Wikimedia Commons | Jin Kemoole | [CC BY 2.0](https://creativecommons.org/licenses/by/2.0/) |
| `gato_body/` | [Leopardus geoffroyi, foto 235096757](https://www.inaturalist.org/photos/235096757), iNaturalist | Ben Costamagna | [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/) |

La foto de la liebre (`tools/hare.py --photos`: de costado, proyectada sobre los dos lados; la cabeza toma su color sin su luz, igualado al del pelaje, y el cuerpo solo el detalle del pelo) está en `hare_body/`:

| Mapa | Foto | Autor | Licencia |
|---|---|---|---|
| `hare_body/` | [Zając szarak na polu w Grocholicach w gminie Sadowie w województwie świętokrzyskim, 20260419 0913 5150](https://commons.wikimedia.org/wiki/File:Zaj%C4%85c_szarak_na_polu_w_Grocholicach_w_gminie_Sadowie_w_wojew%C3%B3dztwie_%C5%9Bwi%C4%99tokrzyskim,_20260419_0913_5150.jpg), Wikimedia Commons | Jakub Hałun | [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/) |

Las fotos de las caras de las rapaces y las plumas escaneadas (`tools/raptor.py --faces --scans`) están acreditadas en la sección "Raptor feather scans and face photographs" del `CREDITS.md` de la raíz: `chimango_body/` (iNaturalist, Hugo Hulsberg, CC0), `carancho_body/` (Wikimedia Commons, Riverbanks Outdoor Store / B A Bowen Photography, CC BY 2.0) y `carancho_feathers/` y `chimango_feathers/` (The Feather Atlas, U.S. Fish and Wildlife Service, dominio público).

Las fotos de las caras de las palomas y las plumas escaneadas (`tools/pigeon.py --faces --scans`) están acreditadas en la sección "Pigeon" del `CREDITS.md` de la raíz: `torcaza_body/` (Wikimedia Commons, Enrique González, CC BY 4.0), `picazuro_body/` (Wikimedia Commons, dfaulder, CC BY 2.0) y `torcaza_feathers/` y `picazuro_feathers/` (The Feather Atlas, U.S. Fish and Wildlife Service, dominio público: Mourning Dove y Band-tailed Pigeon).

Las fotos de las caras de los gorriones y las plumas escaneadas (`tools/sparrow.py --faces --scans`) están acreditadas en la sección "Sparrow" del `CREDITS.md` de la raíz: `gorrion_body/` (Wikimedia Commons, Alexis Lours, CC BY 4.0), `chingolo_body/` (Wikimedia Commons, dfaulder, CC BY 2.0) y `gorrion_feathers/` y `chingolo_feathers/` (The Feather Atlas, U.S. Fish and Wildlife Service, dominio público: House Sparrow y White-crowned Sparrow).
