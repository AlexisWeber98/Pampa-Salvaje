# Pampa Salvaje

Un western argentino de bandidos rurales ambientado en la pampa seca en 1919,
inspirado en Red Dead Redemption. La dirección visual es un mundo 3D low-poly
con personas 3D low-poly al estilo de la era PS1/PS2.

Actualmente este repositorio contiene una pampa procedural abierta (8 km x 8 km
por defecto) cargada por streaming en chunks bajo una niebla de polvo, un
jugador que anda a pie o a caballo, una cámara de seguimiento, asentamientos y
un primer encargo. Los pueblos son pequeños pueblos ferroviarios inspirados
libremente en pueblos de La Pampa de la década de 1910 como Eduardo Castex (no
son una reconstrucción histórica), cada uno en su propio lugar del mundo. El
jugador empieza a bordo del tren de la mañana desde General Pico cuando entra a
Monte Nievas, se baja a pie en su andén, choca con los edificios y no puede
salir del mundo. El andén es un piso elevado, no una pared: rampas cortas bajan
desde ambos extremos, y sus lados largos son escalones para subir o bajar.

El pueblo está vivo: los vecinos deambulan por las calles entre las esquinas
(cuántos caminan por cada pueblo lo fija `townsfolk` en
`assets/world/towns.ron`), un caballo ensillado espera atado a un palenque
frente al almacén de ramos generales (y una ciudad ata algunos más en los
palenques de su comisaría, su iglesia y su estación), y el jugador se abre paso
entre la gente apartando a empujones a los peatones, mientras los peatones lo
esquivan.

Las calles están sombreadas como en la pampa húmeda. Las calles de un pueblo
miden 12 m de ancho (las de General Pico 13 m, las de la aldea Metileo también)
con veredas de 3.5 m que dejan una calzada de 5 m (la de Pico, de 6 m). Todas
tienen cordones: de ladrillo en una ciudad y alrededor de la plaza de un
pueblo, y una senda de tierra apisonada, que se funde con el pasto, a lo largo
de las demás manzanas de un pueblo y en todas partes en una aldea. Una ciudad
bordea sus veredas con árboles en cazuelas (un pequeño cuadrado de tierra
rodeado de revoque de cordón, a unos 0.4 m del cordón hacia adentro), uno cada
17 a 21 m, casi rectos bajo una copa baja y frondosa, y apartados de puertas,
esquinas, faroles, galerías, palenques y carteles. La plaza de General Pico
tiene isletas entre sus senderos además de su anillo de árboles en el borde;
los pueblos muy grandes de muestra tienen más de un tercio menos de árboles en
la plaza que en la primera plantación (las isletas raleadas, los anillos
conservados). Un pueblo o una aldea tiene árboles dispersos sobre la senda de
tierra frente a algunas casas, nunca en fila, y cada asentamiento tiene
árboles de patio detrás de sus casas, un 30% más que en la primera plantación,
con arbustos y canteros de flores junto a las paredes (low-poly, con meshes
compartidas). Las estancias, chacras y puestos rurales plantan un tercio más de
árboles de patio y también tienen arbustos y canteros de flores junto a sus
casas.

El día tiene un reloj: un día de juego dura 48 minutos reales (30×, un minuto
de juego cada dos segundos reales). El sol lo sigue: sale por el este, cruza el
cielo del norte y se pone por el oeste, cálido y bajo al amanecer y al
atardecer y pleno durante el día; las noches son tenues y azuladas bajo la luna
pero siguen siendo legibles, y la niebla de polvo toma el color de la hora sin
volverse más espesa. El cielo (`crates/render/src/sky/`, una bóveda y un
shader) va del azul profundo a una bruma polvorienta en el horizonte, arde en
naranja, rosa y violeta al atardecer y al amanecer, y se vuelve índigo en la
hora azul. Tiene un sol visible con su halo, una luna que pasa por sus fases
(un ciclo de 29.5 días que sigue los días de juego), un mapa estelar real (la
Vía Láctea, las Nubes de Magallanes, la Cruz del Sur) que gira sobre el sur,
cúmulos a la deriva y cirros altos que brillan al atardecer, y aves: chimangos
planeando de día, bandadas de palomas y teros que lo cruzan, y bandadas que
vuelan bajo hacia sus dormideros al atardecer, ninguna de noche. El juego
empieza a media mañana, a plena luz del día, a bordo del tren de la mañana
desde General Pico: el protagonista, un ex empleado de ferretería de Pico,
viaja en él hasta Monte Nievas. La hora del día (`HH:MM`) se muestra en la
esquina superior izquierda, rotulada con la tipografía del juego (IM Fell
English) sobre un panel de papel, con el objetivo actual debajo y, mientras se
viaja en el tren, la siguiente estación y su hora de llegada (por ejemplo
"Próxima: Eduardo Castex, 11:46").

Un tren a vapor recorre toda la línea del ferrocarril según un horario: un tren
mixto rural de 8 vagones (93 m), la locomotora, dos coches de pasajeros y cinco
vagones de carga, dibujado en un estilo de construcción británica de alrededor
de 1919, construido proceduralmente a partir de formas redondeadas (calderas
torneadas, techos curvos) con texturas PBR, a partir de meshes y materiales
unitarios compartidos. La locomotora, una 2-6-0 con ténder, tiene una caldera
con zunchos, una puerta de caja de humo, un farol delantero, cilindros, bielas
de acoplamiento y bielas motrices que giran con las ruedas motrices, un
miriñaque, una cabina con ventanas de anteojo, un ténder cargado de carbón y
algunas bocanadas de humo de chimenea mientras se mueve. Los coches tienen
costados de tablas, ventanas enmarcadas, un techo con linterna y plataformas
abiertas en los extremos con barandas; los vagones de carga son de tablas con
flejes de hierro; todos los vagones van sobre bogies con paragolpes y
enganches. Pasa la noche en General Pico, sale de Pico a las 07:00, para 25 s
en el apeadero de Metileo y en Monte Nievas, llega a Eduardo Castex, hace su
espera en la terminal, sale de Castex a las 16:00, vuelve a parar en Monte
Nievas y en Metileo y regresa a Pico para pasar la noche (un cambio del usuario
del 2026-10-21; hasta entonces dormía en Castex y salía de allí a las 07:00).
Circula a hasta 65 km/h y frena para detenerse con el centro de sus dos coches
junto a cada estación del camino. En las dos terminales todo el tren queda
centrado en la estación, así que da la vuelta en el lugar durante la espera en
la terminal. Tarda unos 11 minutos reales en recorrer los 10.0 km de línea
(Pico 07:00, Metileo 08:52–09:04, Monte Nievas 10:54–11:06, Castex 12:25;
Castex 16:00, Monte Nievas 17:18–17:30, Metileo 19:20–19:33, Pico 21:25, con el
reloj a su ritmo base; la parada de Metileo desde el 2026-10-23, sus manzanas
de 40 m desde el 2026-10-25). El reloj solo inicia cada recorrido: una vez en
marcha, el tren avanza en segundos reales, y un salto del reloj lo adelanta
exactamente hasta donde habría estado. Sus vagones siguen las curvas entre
pueblos y empujan a los jinetes fuera de la vía; los rieles en sí se pueden
cruzar.

Los lugares destacados llevan carteles pintados en el español de la época: el
almacén de ramos generales ("PULPERÍA" en un pueblo pequeño, "ALMACÉN DE RAMOS
GENERALES" en General Pico), la comisaría ("COMISARÍA"), la iglesia
("IGLESIA"), la estación ("ESTACIÓN EDUARDO CASTEX", en su fachada y en dos
nomencladores al borde de la vía, uno cerca de cada extremo del andén, más allá
de los coches, de frente a las ventanillas del tren) y la plaza. `pampa-core`
nombra los lugares e indica dónde va cada cartel (`TownLayout::places`); render
los rotula con una tipografía de tipos de madera Rye (TrueType incorporada,
con antialiasing) sobre tablas envejecidas. La estación es un edificio de
ladrillo al estilo del FCO con pilastras de revoque, aberturas con guardapolvos
y un pabellón central con frontón tanto en su frente a la calle como en su
frente a la vía, y un techo sobre el andén apoyado en columnas de hierro
fundido, con una cenefa y un lambrequín de madera calada. Los almacenes y las
comisarías se reparten por cada pueblo en lugar de agruparse alrededor de su
plaza, y General Pico tiene un almacén y una comisaría a cada lado de su línea.

Un minimapa con estilo de papel en la esquina superior derecha muestra el
pueblo más cercano con el norte hacia arriba, titulado con su nombre: calles,
manzanas y edificios según su tipo, la plaza, el ferrocarril, cada lugar con
su inicial, los peatones como puntos, los vagones del tren cuando están dentro
del mapa, el destino del encargo como un rombo dorado y el jugador como una
flecha según su rumbo. Los caminos de tierra se dibujan en un marrón cálido
debajo del ferrocarril. Una pequeña rosa de los vientos en su esquina inferior
izquierda marca N, E, S y O (oeste); el mapa tiene el norte hacia arriba, así
que la rosa queda fija y la flecha gira. Encuadra el pueblo más un margen de
30 m (no todo el mundo) para que las calles sigan siendo legibles, con
cualquier asentamiento que entre en ese encuadre. A más de 20 m del pueblo más
cercano (antes de que el jinete salga del encuadre del pueblo), o a menos de
20 m de un asentamiento, cambia a un encuadre de campo: 640 m alrededor del
jinete (2 m por píxel) con los caminos, el ferrocarril (una única línea sin
cortes, sin las marcas de durmientes del mapa del pueblo), los asentamientos
(edificios según su tipo, contornos de corrales, huellas como finas líneas de
tierra, árboles como puntos) y los restos abandonados al costado de los
caminos. Vuelve al pueblo a menos de 5 m de él y a 50 m de distancia de todos
los asentamientos, para que no parpadee. Un destino fuera del mapa queda fijado
al borde de su lado, así que señala el camino. La capa de cada pueblo se
rasteriza una sola vez, la primera vez que se muestra; la capa de campo se
vuelve a pintar en el lugar solo después de que el jinete se aleja 80 m de su
centro; solo los marcadores se redibujan en cada frame. `M` alterna un mapa
ampliado y centrado.

### Viajar en tren

**La apertura.** El juego empieza a las 10:43:59 con el protagonista sentado en
el primer coche del tren de la mañana desde Pico, 50 segundos reales antes de
que se detenga en Monte Nievas. La cámara empieza detrás del coche,
encuadrándolo junto con el pueblo que está adelante ("Próxima estación: Monte
Nievas"). El tren frena al entrar a la estación a las 10:53:59 ("Estación Monte
Nievas"), y dos segundos reales después, a las 10:54:23, él se baja al andén
junto a la puerta del coche, a pie. (La apertura sigue el horario: la parada de
Metileo la corrió 39 minutos de juego más tarde el 2026-10-23, y las manzanas
más anchas de Metileo, con una vía más larga a través de él, 19 segundos de
juego más el 2026-10-25).
`Space` omite la llegada, como un viaje corto (ver Saltar).

**Subir.** Mientras el tren está detenido en una estación (o hace su espera en
una terminal), `F` a menos de unos 4 m de la puerta de un coche, a pie sobre el
andén, sube a él. En el punto de parada de una terminal, el centro del andén,
`F` alcanza la puerta más cercana hasta a 8 m durante toda la espera. Cada
estación tiene tableros de horarios; `F` junto a uno lee los trenes del día. A
caballo, el empleado solo dice "Primero bajate del caballo": el caballo se
queda donde está, no viaja en tren.

**A bordo.** El jugador se sienta en el coche y va adonde vaya el tren. `V`
recorre tres vistas, nombradas en el HUD ("V: vista - ventana"): sentado junto
a una ventanilla dentro del coche, la vista por defecto, del lado que da al
andén de la siguiente estación (así la llegada se ve por la ventanilla), con la
ventanilla de 0.8 m a 1.8 m sobre el piso alrededor de los ojos sentados a
1.2 m, mirando hacia adelante en el sentido de la marcha; el mouse (o `Q`/`E`)
gira la cabeza hasta unos 120° hacia cada lado y 40° hacia arriba o hacia
abajo, así que cualquiera de las ventanillas, la pampa que pasa y el andén al
llegar quedan al alcance. Cada vez que se ocupa el asiento (la apertura, al
subir, cuando entra un viaje saltado, al volver con `V` a esta vista) la cabeza
vuelve a mirar hacia adelante; la órbita alrededor del coche desde 16 m, a la
que los vagones del propio tren nunca acercan; y el tren entero de costado,
siguiéndolo lentamente. Los coches son coches de segunda clase huecos de
alrededor de 1919: paredes de madera con aberturas de ventanas vidriadas (las
mismas ventanas desde afuera), un piso de tablas, bancos de madera enfrentados
en módulos, lámparas de aceite bajo el techo y una puerta en cada extremo. El
mundo sigue cargándose por streaming alrededor del tren en movimiento. El reloj
se desacelera a 12× mientras se viaja, así que toda la línea (unos 11 minutos
reales) equivale a unas dos horas de juego. `F` mientras el tren está detenido
en una estación baja al andén junto a la puerta del coche; `F` mientras está en
marcha baja en la siguiente parada. Al final de la línea, el jugador baja
cuando el tren hace su espera en la terminal. Cada parada se anuncia en un
panel en la parte superior de la pantalla ("Estación Monte Nievas"), y también
la siguiente cuando el tren arranca ("Próxima estación: Eduardo Castex").

**Saltar.** `Space` a bordo salta a la siguiente parada como un viaje corto: el
tren arranca de verdad desde la estación durante 3 segundos ("Próxima
estación: …"), la pantalla hace un fundido a negro, y el tren entra durante los
últimos 3 segundos y frena hasta detenerse ("Estación …"). El salto ocurre en
el negro: el reloj avanza el tiempo del viaje a 12×, y el tren queda
exactamente donde lo habría llevado seguir viajando. `Space` otra vez durante
el viaje lo acorta. El jugador se queda a bordo, para bajar o seguir viajando,
salvo que el viaje termine allí.

**Esperar.** En un andén sin tren, `F` cerca de su centro adelanta el reloj (al
ritmo base, detrás del mismo fundido) hasta que el próximo tren se detiene
allí, el del día siguiente si no llega ninguno antes de la medianoche.

Los árboles, las piedras, los arbustos y las matas de pasto se mantienen fuera
de todo el recorrido del tren, incluida la vía propia de cada pueblo y sus
prolongaciones más allá del pueblo, no solo los tramos entre pueblos.

Las reglas (quién puede subir, dónde se baja un pasajero, los saltos de omitir
y de esperar, la apertura) son lógica pura del `core` (`pampa_core::ride`),
probada sin el juego.

### El primer encargo

El jugador llega a pie. Arriba del andén de Monte Nievas, junto a los coches
del tren de la mañana, el objetivo dice "Buscá a Don Hidalgo en la plaza". Don
Hidalgo está en la plaza; hay que presionar `F` a menos de unos 3 m para
hablar. Monte Nievas no tiene comisaría, así que él manda al jugador a
presentarse ante el comisario Elías Farach en la comisaría de Castex, y le
presta su caballo, el que está atado en el almacén de ramos generales. El
pueblo de Farach no está fijo en el código: lo nombra `police_town` en
`assets/dialogue/errand.ron` (Eduardo Castex; sin él, o cuando ese pueblo no
tiene comisaría, el otro pueblo más cercano que tenga una), y la línea del
objetivo también lo nombra. Sus líneas se reproducen una tras otra en globos
de diálogo, unos segundos cada una. El caballo no es del jugador antes de esa
charla ("Este caballo no es mío."); después, `F` a menos de unos 3 m del
caballo lo monta: el caballo atado se lo llevan por el resto de la sesión y el
jugador cabalga desde donde estaba. Farach está parado en la calle frente a la
puerta de la comisaría de Castex; `F` a menos de unos 4 m, montado o no, hace
que el jugador se presente ante él y termina el encargo. La línea del objetivo
y el destino del minimapa siguen cada paso (Hidalgo, el caballo, luego Farach)
y desaparecen una vez terminado el encargo.

Una vez tomado, el caballo es del jugador. `F` detenido o al paso lento (hasta
1 m/s) lo hace bajar: el jugador queda parado junto al caballo, a su izquierda
(a su derecha si la izquierda está bloqueada), y el caballo se queda allí,
atado flojo, quieto y estorbando el paso como cualquier caballo atado. `F` a
menos de unos 3 m de él lo hace volver a subir. Ninguna de las dos cosas hace
avanzar el encargo: el caballo se toma una sola vez, y uno puede presentarse
ante Farach a pie o montado.

Tomar el caballo de Don Hidalgo por primera vez también le enseña al
protagonista a montar: "Ahora podés montar cualquier caballo" aparece en la
parte superior de la pantalla, y desde entonces `F` a menos de unos 3 m de
cualquier caballo parado (uno atado en otro pueblo, o uno suelto) lo hace subir.
El caballo atado de un pueblo se lo llevan entonces por el resto de la sesión,
y el caballo que se montaba antes queda donde se lo dejó. Antes de eso, solo el
caballo de Don Hidalgo lo deja subir. Montar es la primera de las habilidades
del jugador (`pampa_core::Abilities`); el lazo, las armas y los autos quedan
para más adelante.

A caballo a galope tendido (al menos el 85 % de la velocidad máxima del galope, 49 km/h), `Space` salta (`Rider::jump`). En el aire el caballo mantiene su línea y su velocidad, y pasa por encima de todo obstáculo bajo mientras está más alto que ese obstáculo. Los alambrados miden 1.3 m, y un objeto de un asentamiento es tan alto como su estructura. Todo lo demás cuenta como sin límite de altura (`StaticObstacles::push_low`, `World::resolve_above`). En el aire el caballo reproduce el clip `Gallop_Jump` de Quaternius (patas recogidas, cuerpo cabeceando), ajustado en cada frame a `Rider::flight` (la proporción del vuelo transcurrida) para que la pose siga el arco de la física y no el suyo propio; la subida propia del clip se le quita al cuerpo. Por encima del clip, las patas traseras impulsan visiblemente al caballo en el despegue y las delanteras se recogen un poco más en lo alto, mientras el jinete se inclina hacia adelante desde la cadera con la mano de las riendas a lo largo del cuello, y luego se endereza y absorbe el aterrizaje con las rodillas y la cadera.

A pie, el jugador camina a 1.6 m/s (unos 5.8 km/h) y trota con `Shift` a 18 km/h, con las mismas teclas, la misma colisión y un cuerpo del tamaño de una persona.
La gente le abre paso sin quejarse a alguien que va a pie; solo el caballo los
hace hablar.

### Un mundo abierto en chunks

El suelo se construye en chunks cuadrados de 200 m alrededor del jinete: se
construyen los chunks a menos de 3 chunks (como máximo uno por frame, todos en
el primer frame), y los chunks más allá de 4 se eliminan con sus rocas,
arbustos, pasto y árboles, así que la memoria queda acotada dondequiera que se
cabalgue. Un chunk que entra por streaming mientras se cabalga construye su
mesh de suelo, y dispone cualquier asentamiento que contenga, en un hilo en
segundo plano, y los recibe unos frames después, lejos en la niebla, así que
cruzar el borde de un chunk nunca traba un frame. El suelo tiene sombreado
suave: cada esquina de la cuadrícula de un chunk toma su normal de la
pendiente del campo de alturas del terreno, muestreado sobre una única red
para todo el mundo, así que el suelo ondula sin facetas y los bordes de los
chunks se sombrean sin costuras.

General Pico creció a ambos lados del Ferrocarril Oeste, que pasa por el centro
del pueblo (de este a oeste en el juego; la cuadrícula y la línea del pueblo
real están giradas 45°): la estación, su andén, los depósitos de granos y el
playón están sobre la línea en el centro, la plaza principal unas cuadras al
noroeste de ellos, y cada calle norte-sur que no toca el playón cruza la vía
por un paso a nivel con una plataforma de tablones y cruces de San Andrés
(`RailwayConfig::through`). Entre Monte Nievas y Pico, el tren para en el
apeadero de Metileo (estación de 1906): un almacén y un puñado de casas
alrededor de una pequeña cuadrícula (`density` en el archivo de pueblos). En el
extremo oeste de Metileo sale un ramal de la línea, como lo hacía el ramal
Metileo–Arizona real, y se curva hacia el noroeste con curvas de 500 m hasta
**Caleufú** (fundado en 1911), 7.5 km de vía que terminan en su estación cerca
del borde oeste del mundo (`RailBranch`, `branches` en el archivo de pueblos).
Caleufú es un pequeño pueblo ferroviario de 4 x 4 manzanas, construido en unos
dos tercios, con su plaza y su iglesia, una comisaría y una pulpería a varias
manzanas de distancia, y su propio paso a nivel y su huella rural junto a la
estación. La vía, las curvas, los pasos a nivel y el minimapa tratan el ramal
como la línea. Una **lanzadera del ramal**, una locomotora tanque 2-6-2T con un
coche, circula entre Caleufú y una vía muerta en Metileo y se encuentra allí
con el tren de la línea en ambos sentidos, así que el trasbordo se hace en
Metileo. El tren de la línea se mantiene en la línea principal. Al este-sureste
de Pico está la **Laguna La Arocena**, una laguna de agua dulce poco profunda
(`pampa_core::Lagoon`): una orilla casi cuadrada, de contornos suavemente
redondeados, de unos 215 x 145 m, un poco irregular en los bordes. Su agua es
una lámina plana de color pardo oliva oscuro 0.6 m por debajo del punto más
bajo de la ribera, lisa y reflectante como el agua, más oscura en el centro y
transparente sobre el barro cerca de la orilla, con ondas de viento tenues y
finas a partir de un pequeño normal map pintado en código y desplazado por su
transformación de UV (un draw call, sin pasada de reflejos). El suelo se hunde
en una cuenca debajo de ella (1.6 m de profundidad en el centro); solo una
franja de 3 m de barro húmedo rodea la línea del agua, y el pasto, las piedras
y los arbustos vuelven a crecer justo después. Juncos de totora, de 1.5 a 2.5 m
de alto, crecen en matas a lo largo de la orilla, en una sola mesh. Los
jinetes y los peatones vadean la parte poco profunda, pero se los hace volver
donde el agua se vuelve profunda. Los alambrados, los asentamientos y los
caminos se mantienen fuera de ella y de su ribera, los árboles quedan a 12 m
del agua, y el minimapa la pinta como agua.

**Faroles de kerosén** iluminan los pueblos, como en 1919, sin alumbrado
público eléctrico: un poste de hierro fundido con una linterna de vidrio, de
unos 3.3 m de alto. Cuántos hay depende del tamaño del pueblo
(`pampa_core::LampTier`):
- **Pueblos grandes** (General Pico, 105 faroles): uno junto a la boca de cada
  sendero en ambas plazas, cuatro más alrededor del monumento principal, y uno
  en el cordón de cada esquina.
- **Pueblos pequeños** (Monte Nievas 20, Eduardo Castex 25, Caleufú 21): cuatro
  en la plaza, uno junto a la boca de cada sendero, y uno en una de cada dos
  esquinas.
- **Aldeas** de como máximo 3 x 3 manzanas (Metileo, 14): dos en la plaza, en
  las bocas del sendero hacia la estación, y faroles de esquina solo alrededor
  de la plaza y junto a la estación.

Cada camino sale de un pueblo entre un par de faroles. Cada estancia tiene uno
en cada extremo de su galería y uno junto a su huella dentro de la tranquera.
La ubicación es determinista y queda apartada de los carriles, las puertas,
las galerías, los senderos de la plaza, el ferrocarril, los caminos, las
huellas y todo obstáculo (`pampa_core::StreetLamps`); cada poste es un
obstáculo delgado que se desvanece para no tapar la cámara. El vidrio brilla
del anochecer al amanecer según el reloj del juego, con fundidos de entrada y
salida, y solo los 8 faroles más cercanos a la cámara emiten luz real (un
grupo compartido de luces puntuales cálidas sin sombras, con 12 m de alcance);
de día no hay ninguna luz de farol (`pampa_render::lamps`).

El campo abierto entre los pueblos es pastizal pampeano:
- El color del suelo varía del amarillo seco al gris verdoso, mediante colores de vértice que son continuos a través de los bordes de los chunks. Sobre esa pintura va una textura de detalle repetida de tierra y pasto (también con mapas de normales, rugosidad y oclusión ambiental), con UV en el espacio del mundo sobre una baldosa de 6 m, así que continúa a través de los bordes de los chunks.
- Las matas de pasto (paja, flechilla) están densamente esparcidas, teñidas del gris verdoso al amarillo seco junto con el suelo, con algunos yuyos y flores silvestres entre ellas. Se ralean con la distancia.
- Los caldenes y los chañares crecen solos o en pequeñas isletas. Alrededor de uno de cada diez árboles es un caldén seco; algunos tienen un tocón o un tronco caído al lado. Los chañares crecen en matorrales densos.
- Piedras de tosca y pequeños arbustos de jarilla están esparcidos por el campo abierto.

Las plantas y las piedras son modelos low-poly CC0 del Ultimate Nature Pack de Quaternius, y las texturas del suelo son CC0 de ambientCG. Se leen de `assets/` al inicio con nuestros propios pequeños lectores de OBJ y PNM (sin crates adicionales), y se recolorean y reescalan en código para la pampa seca de 1919. Las normales facetadas del paquete se suavizan una sola vez cuando se prepara un modelo (`pampa_render::smooth`, el mismo auto smooth que `tools/glbtrim.py`): las copas, los arbustos y los yuyos funden las facetas de hasta 60° de diferencia, la madera muerta las de 50°, mientras que las piedras de tosca conservan las aristas de más de 40°. El pasto funde hasta 80° e inclina sus normales hacia arriba, así que una mata se ilumina como el suelo que tiene debajo. Un archivo faltante registra un aviso, y en su lugar se dibuja un sustituto procedural. No hay cambio de nivel de detalle: cada árbol es el mismo modelo hasta los 200 m y se desvanece dentro de la niebla de polvo de 180 m. Las matas se mantienen sólidas a menos de unos 40 m del jinete y se ralean suavemente más lejos. Cada elección de modelo, giro y tamaño usa como semilla su ubicación, así que un chunk recargado se ve exactamente igual. Cada asset tiene su crédito, con lo que se cambió, en [CREDITS.md](CREDITS.md).

Caminos, alambrados y tranqueras cruzan ese campo:
- **Caminos de tierra** para carretas y jinetes unen los pueblos vecinos de la línea (Eduardo Castex–Monte Nievas, Monte Nievas–Metileo, Metileo–General Pico), y cada pueblo de un ramal con su pueblo más cercano (Caleufú–Eduardo Castex, 5.8 km, que cruza la prolongación de la línea al oeste de Castex). Cada uno sale de un pueblo por la calle que da al otro pueblo y sigue una línea de curvas suaves con semilla. Es más directo que el ferrocarril, que pierde distancia en sus prolongaciones y en sus curvas amplias. Un camino se mantiene a 40 m de la vía y la cruza solo donde sus extremos quedan en lados opuestos, y entonces en perpendicular. Un camino nunca sale de General Pico a lo largo de su línea, que se prolonga por ambos extremos del pueblo. Un camino público sin salida también sale de General Pico por su lado este hacia la Laguna La Arocena, con un cartel indicador que nombra la laguna en su inicio; termina en la ribera de la laguna en un pequeño retorno que llega hasta el barro húmedo y no más allá.
- **Pasos a nivel y huellas rurales:** cada pueblo ferroviario tiene una calle que cruza su vía junto a la estación, en perpendicular, por un paso a nivel. Es la calle que atraviesa el playón de la estación más cercana a la estación, o la calle de circunvalación prolongada un poco más allá de la cuadrícula cuando el playón ocupa todo el lado de la vía del pueblo. El paso a nivel tiene tablones de madera a ras debajo de las cabezas de los rieles, entre los rieles y en una franja de alrededor de medio metro por fuera de cada uno, exactamente con el ancho del camino a lo largo de la vía. El propio camino de tierra sube hasta ellos como tierra compactada a lo largo de 7 m de cada lado (alrededor de 1 en 10 en lo más empinado), sobre su propio eje y tan ancho como el camino, con los bordes inclinados hacia el pasto. Los jinetes, las personas y la cámara suben la misma rampa; el terraplén de la vía nunca se baja ni se sube por el camino. Una cruz de San Andrés se levanta sobre un poste en cada acceso. Más allá, una corta huella rural con semilla de 300–800 m sale hacia las chacras y las estancias y termina en el campo abierto. Un jinete siempre puede cruzar; un vagón detenido sobre el paso a nivel igual lo bloquea.
- **Superficie del camino:** cada camino se hunde unos escasos 15 cm en el suelo, nivelado en sentido transversal y uniéndose suavemente con el terreno de al lado, así que `Terrain::height` sigue siendo continuo. No se toca nada que esté más lejos del camino que el alcance del corte, y el terraplén de la vía en un paso a nivel se deja como está. Lleva la tierra compactada Ground 102 de ambientCG, con los bordes fundidos con el pasto, que está gastado hasta la tierra a lo largo de él. Ningún árbol, mata, piedra ni arbusto crece sobre un camino ni sobre sus banquinas.
- **Carteles indicadores:** un cartel indicador de madera se levanta en la salida del camino de cada pueblo. Su tabla con forma de flecha nombra el pueblo lejano y la distancia en kilómetros, con un decimal, con la misma tipografía Rye, por ejemplo "GENERAL PICO 6,9 KM".
- **Alambrados** delimitan las tierras privadas: estancias, tambos, campos y lotes. Los caminos son públicos, y ningún alambrado ni tranquera cruza nunca uno.
  - Cada camino mide 6.5 m de ancho y corre entre dos alambrados a 8.5 m de su eje, al menos 5 m más allá de su borde de cada lado: una amplia "calle entre alambrados" para las carretas y, más adelante, las rutas. Los alambrados siguen las curvas del camino y se detienen en el terreno nivelado de los pueblos y junto al ferrocarril, así que no hay alambrado en un paso a nivel.
  - El campo lejos de los caminos está mensurado en una cuadrícula norte–sur, con líneas separadas entre 300–800 m según una semilla. Una línea de la cuadrícula se detiene en el alambrado del costado del camino y se une a él en un poste esquinero.
  - El frente de cada campo sobre un camino recibe una o dos tranqueras (simples o dobles). Los alambrados de la cuadrícula tienen una abertura cada 300–500 m: una tranquera entre potreros, o un tramo roto con un poste inclinado, uno caído y un alambre flojo.
  - Alrededor del 60% de las tranqueras de campo están abiertas. El resto están cerradas sobre su abertura y bloquean a los jinetes como el alambre. Los tramos rotos siempre se pueden atravesar.
  - Cada campo conserva una entrada: una tranquera abierta, un tramo roto o un lado sin alambrar. Cada frente sobre un camino conserva al menos una tranquera abierta.
  - Alrededor de uno de cada diez potreros es un descampado, un campo abandonado sin alambrado propio. Su frente sobre el camino no tiene alambrado, y solo lo bordean los alambrados de sus vecinos.
  - Los postes están a como máximo 10 m de distancia con cinco hilos de alambre, más juntos en las curvas cerradas. Cada tramo y cada tranquera cerrada es un obstáculo estático delgado, dividido en cajas cortas a lo largo de los tramos oblicuos: unas 125,000 cajas, colocadas en alrededor de 0.1 s. Los caminos quedan abiertos de punta a punta.
  - Abrir y cerrar tranqueras todavía no está en el juego.

Los caminos, los pasos a nivel, los carteles indicadores, los alambrados y las tranqueras se cargan por streaming con los chunks, como la vía.

Detrás de algunas tranqueras abiertas hay asentamientos rurales (`pampa_core::settlement`, dibujados por `pampa_render::settlement`): ranchos de adobe bajo paja o chapa ondulada, puestos con su corral y su aguada, chacras encaladas o de ladrillo con un galpón y un molino, y cascos de estancia con una galería al frente, casas de peones, corrales, una isleta de eucaliptos y una hilera de álamos. Huellas de tierra (dos surcos de ruedas) llevan desde la tranquera hasta cada puerta. Los edificios y los objetos son procedurales, con texturas PBR CC0 compartidas, un material por superficie; cada edificio, objeto, travesaño de corral y tronco de árbol es un obstáculo estático, y las casas y los galpones detienen la cámara. En algunos patios y en unas pocas banquinas se pudren carretas abandonadas, y barriles, cajones, una escalera, un balde y faroles de Poly Haven están repartidos alrededor de los galpones y los pozos.

Los edificios de los pueblos se construyen con el mismo kit (`pampa_render::facade`: el constructor de meshes, los materiales con texturas compartidos, las paredes, los techos y la ornamentación italianizante) como frentes italianizantes pampeanos de 1919. Las casas están encaladas, blanqueadas a la cal, pintadas en un color apagado de 1919 (blanco cal, ocre, rosa pálido, gris azulado o terracota claro) sobre un zócalo beige, de ladrillo visto, de ladrillo bajo una pintura gastada, o de vez en cuando de ladrillo viejo que pierde su revoque, con pilares esquineros ochavados, pilastras entre los módulos, una cornisa moldurada y un parapeto (macizo o con balaustrada, algunos con urnas o un pequeño frontón) que oculta una azotea plana o un techo de chapa; en las afueras, la mayoría son casitas de adobe más sencillas o encaladas bajo chapa. El almacén de ramos generales tiene un parapeto alto detrás de su cartel y una galería sobre la vereda apoyada en postes de hierro fundido; la pulpería de un pueblo pequeño, una sobre postes de madera; la comisaría tiene ventanas con rejas, una balaustrada y su mástil; la iglesia, una nave con frontón y un campanario con una espadaña de arcos de medio punto bajo una aguja de cinc. Los galpones y los depósitos de granos son galpones de chapa ondulada, que colisionan en sus paredes (y allí detienen la cámara) mientras sus aleros llegan hasta los bordes de sus lotes, y los molinos y los tanques de agua son el molino y el tanque australiano de los asentamientos. Las veredas de ladrillo con un cordón de revoque siguen el tamaño del pueblo (`pampa_core::town::Paving`): a lo largo de cada frente de manzana en General Pico, solo a lo largo del borde propio de la plaza en los pueblos pequeños (Eduardo Castex, Monte Nievas, Caleufú), y ninguna en una aldea de 3 x 3 manzanas o menos (Metileo), cuyas calles siguen siendo de tierra. Sobresalen unos centímetros sobre la calle, como los senderos de la plaza, y no cambian en nada cómo camina nadie. Cada frente de mampostería se mantiene dentro de su huella en planta del core, así que los colliders y los carteles de las fachadas siguen siendo fieles. Cada edificio es una mesh por superficie; un pueblo que entra por streaming los construye en un hilo en segundo plano, y se dibujan dentro de los rangos de visibilidad compartidos de los asentamientos.

Las calles son de tierra: una mesh de tierra compactada por pueblo (`Surface::Earth`, `dry_ground_01`) con UV en el espacio del mundo, así que las franjas se encuentran en las esquinas sin costura, a 3 cm por encima del suelo como antes. Las calles no llevan surcos de ruedas, ya que la gente camina y cabalga por ellas. Las huellas de carreta (dos surcos de `brown_mud_dry`, unos milímetros por encima de la tierra) marcan solo las entradas que usan las carretas: las huellas de las estancias desde su tranquera, y una corta huella desde la calle hasta la puerta de cada uno de los edificios de carga del playón de la estación (el galpón de 25 m y los depósitos de granos), nunca en la estación, las casas, los almacenes ni las plazas.

Además del palenque del almacén de ramos generales, `pampa_core::town::Palenque` pone un palenque frente a cada uno de los demás almacenes, comisarías, iglesias y estaciones, junto a la puerta y apartado de los carteles, los faroles, el mobiliario de la plaza, el andén y la línea; cada palenque, y cada caballo, es un obstáculo. En una ciudad (un pueblo Big o mayor, General Pico) una mitad de ellos elegida con semilla, como máximo tres, tiene un caballo atado (una entidad del juego, `PalenqueHorse`); los postes de los pueblos y las aldeas quedan vacíos. `pampa_render::facade::palenque` los construye (postes ochavados y una barra redonda de tablas envejecidas, una mesh por pueblo, dibujada dentro del rango de los objetos).

Cada plaza la dispone el core (`pampa_core::town::PlazaFurniture`) y la viste según el tamaño de su pueblo, como las veredas (`pampa_render::facade::plaza`):
- **General Pico**: senderos de ladrillo con cordones de revoque y una reja de hierro sobre un zócalo de revoque entre pilares.
- **Pueblos pequeños** (Eduardo Castex, Monte Nievas, Caleufú): senderos de tierra con cordones de ladrillo y sin reja.
- **Aldeas** (Metileo): senderos de tierra, sin cordones y sin reja.

Los dos senderos siguen cruzando la plaza, de 3 m de ancho, y se encuentran en una explanada pavimentada alrededor del monumento. El monumento sigue el estilo de la Pirámide de Mayo: escalones de revoque, un pedestal encalado con placas de bronce bajo una cornisa, un obelisco que se angosta hacia arriba y un remate de hierro. Mide unos 4.5 m de alto en una aldea, 6.4 m en un pueblo y 8.8 m en General Pico, donde hay urnas sobre su cornisa. Bancos de listones de madera con laterales de hierro fundido están en los canteros junto a los senderos, de frente a ellos: tres a cada lado de cada sendero en General Pico, pero solo uno junto a cada sendero en un pueblo pequeño y dos en total en una aldea, así que las plazas pequeñas quedan despejadas. La reja de General Pico se abre en la boca de cada sendero.

El escalón más bajo del monumento, los bancos y la reja son obstáculos. Los senderos, la explanada alrededor del monumento, los faroles y las calles de los vecinos quedan despejados, y los tests hacen caminar a una persona y a un jinete por cada plaza. Cada plaza son dos meshes por superficie: senderos y monumento dentro del rango de los edificios, bancos y reja dentro del de los objetos. Se construyen en el hilo en segundo plano junto con los edificios. La plaza principal de Pico tiene unos 22k triángulos.

Los árboles de las plazas son los árboles de las plazas pampeanas de 1919: el paraguas redondo de hojas pinnadas del paraíso, el alto jarrón del olmo, el alto eucalipto con sus hojas colgantes y el aromo, bajo y plumoso. Cada plaza planta una especie, elegida con la semilla del pueblo, y una plaza de la estación planta otra. Un árbol es un tronco de corteza que se angosta, con ramas y ramitas, que lleva tarjetas de hojas con alpha test (entre unos 720 y 960 triángulos). Cada especie tiene tres variantes con semilla, construidas una sola vez y compartidas por todos los árboles de plaza, giradas y escaladas un poco por árbol, así que los pueblos suman en total cuatro materiales de corteza y cuatro de hojas. Los árboles se desvanecen para no tapar la cámara, como los árboles de los asentamientos. Las texturas de corteza y de hojas son conjuntos CC0 de ambientCG: `tools/leaf_atlas.py` dispone las hojas escaneadas en atlas de ramitas (la hoja pinnada del paraíso se compone a partir de folíolos sueltos), y `tools/textures_to_ktx2.sh` empaqueta la opacidad de cada atlas en el alfa de su color (ver `assets/textures/trees/CREDITS.md`).

**Fauna** (primera pasada, `pampa_core::fauna`, dibujada por `pampa_render::fauna`, cargada por streaming por el módulo `fauna` del juego). Manadas de ganado Hereford, Aberdeen Angus y overo pastan dentro de algunos campos alambrados. Cada manada se mantiene en un potrero apartado de los caminos, los pueblos, las lagunas, el ferrocarril y los asentamientos. Las vacas caminan despacio, pastan con la cabeza baja y se quedan paradas; no se echan. Una manada de 10 cabezas o más lleva un toro de su raza. Se alejan caminando cuando un jinete se acerca, y bloquean al jinete como cuerpos blandos. Caballos sin aperos están en los corrales de las chacras y las estancias, con pelajes criollos, a ratos en espera, a ratos pastando. Una vez desbloqueada la monta, cualquiera de ellos se puede montar. Las liebres se sientan y saltan en el pasto abierto y huyen de un jinete. Los teros levantan vuelo en un arco bajo y se posan más adelante. Los chimangos dan vueltas en lo alto, y los patos nadan en la Laguna La Arocena. Las vacas, los toros y los caballos de corral son modelos animados CC0 de Quaternius (`assets/models/fauna/quaternius/`, recortados con `tools/glbtrim.py`). Se tiñen según el pelaje a partir de copias de materiales compartidas, y sus clips siguen la simulación: comer, esperar, caminar al ritmo de la velocidad sobre el suelo, galopar. Más allá de 120 m, o fuera de la vista, un animal se congela en su pose. Los animales más pequeños son unas pocas partes rígidas low-poly que comparten 4 meshes y 10 materiales, animadas solo cerca de la cámara. Los animales se cargan solo en los chunks a menos de dos del chunk del jugador, los más cercanos primero, con límites estrictos por especie (`pampa_core::fauna::caps`). En las compilaciones de depuración, el comando `fauna` de la consola cuenta lo que hay cerca.

Un tipo de estructura con una ranura de modelo (el molino, la carreta abandonada, la carreta, el pozo y otros) dibuja un modelo glTF de `assets/models/third-party/<slot>/` cuando hay uno, ajustado por `slots.ron`, y si no, su mesh procedural; ver `assets/models/third-party/README.md`. El molino y la carreta abandonada son modelos del 3TD Starter Pack (CC0) convertidos desde COLLADA con `tools/dae2gltf.py`, un script de Python de la biblioteca estándar con sus propios tests (`python3 -I -B -m unittest discover -s tools`; ver `tools/README.md`). Las texturas de paja las genera `assets/textures/buildings/thatch/generate.sh`, cuyo `--check-signs` prueba la convención OpenGL del normal map sobre rampas conocidas. Las texturas de los edificios se distribuyen como KTX2 comprimido para la GPU junto a sus fuentes JPG: `tools/textures_to_ktx2.sh` codifica cada conjunto a UASTC con mipmaps incorporados y empaqueta la oclusión, la rugosidad y la metalicidad en un único mapa `orm`, y Bevy los transcodifica al cargar al formato de bloques de la GPU (BC7 en escritorio), un cuarto de la memoria de video que ocupaba RGBA8 (ver `tools/README.md`).

Todo lo ubica `pampa_core::flora`, con el chunk como semilla, y apartado del terreno nivelado de los pueblos, el ferrocarril, los caminos, las líneas de alambrado y el borde del mundo. Cada tronco de árbol del mundo es un obstáculo estático: unos 113,000, plantados una sola vez al cargar en alrededor de 0.1 s, así que un caballo choca con un tronco incluso en un chunk que todavía no se dibuja. La niebla de polvo se traga todo mucho antes de los
400 m que siempre están cargados. Los pueblos se cargan por streaming de la misma forma: los edificios,
los carteles, la gente y el caballo atado de un pueblo aparecen a menos de 600 m de él y se van una vez que uno está
a 750 m. Las huellas en planta de cada pueblo se quedan en el mundo lógico en todo momento
(la colisión las busca en una cuadrícula uniforme). El tren de la línea siempre circula; sus
vagones se dibujan solo a menos de 400 m del jinete. Las posiciones son `f32`, así que el mundo está limitado a 8.4 km desde su centro.

La cámara orbita al jugador 360° completos con el mouse (o Q/E), hace zoom con
la rueda, vuelve suavemente detrás del jugador tras un momento sin entrada de
cámara mientras se mueve, y mira a un punto más bajo a pie que a caballo. Se
acerca de inmediato cuando un edificio, los vagones del tren de la línea o la
losa del techo del andén bloquearían su vista (igual puede mirar por debajo del
techo), y vuelve a alejarse suavemente a lo largo de alrededor de un segundo
una vez que la vista se despeja. Las cosas delgadas no la acercan. Un árbol, un
tramo de alambrado, un cartel indicador o una señal de paso a nivel que se
interpone entre la cámara y el jinete se vuelve transparente en cambio, y
reaparece una vez que la vista se despeja. Solo se comprueban las cosas
indexadas cerca de esa línea de visión, así que esto sigue siendo barato.

Fuera del tren, `V` alterna una vista en primera persona desde los ojos del
jugador, para ver los detalles de cerca, y de vuelta; la cámara se desliza
entre las dos en un cuarto de segundo, y la elección se mantiene durante la
sesión. Los ojos siguen el hueso de la cabeza del modelo del protagonista, a
pie y en la montura, con el balanceo de la caminata y del galope amortiguado y
la vista nivelada. El mouse (o `Q`/`E`) mira alrededor, hasta 80° hacia arriba
o hacia abajo; a pie, el cuerpo gira hacia la vista mientras camina, y en la
montura la cabeza gira hasta 120° hacia cada lado respecto del rumbo del
caballo mientras el caballo se dirige como siempre. La cabeza propia del
jugador no se dibuja desde los ojos: su cara y su cráneo se recortan de su
piel según sus pesos de skin cuando se carga el modelo, con el corte
rodeando el cuello por dentro del cuello de la camisa, y van con el sombrero,
el pelo y los ojos a una capa de render que esa cámara deja afuera, mientras
el sol sigue proyectando la sombra completa. Los ojos nunca quedan más de 3 cm
detrás de la cabeza, ni siquiera al galope. Los brazos, las manos, el cuello de
la camisa, las riendas y el cuello y las orejas del caballo quedan a la vista.
Cuando una pared acerca la cámara de órbita a menos de 0.6 m de la cabeza del
jugador, deja de dibujar su modelo (la sombra se mantiene) hasta que vuelve a
estar más allá de 0.8 m. Los ojos ven desde 5 cm (el plano cercano), con un
campo de visión vertical de 75°, y la primera cascada de sombras del sol se
acorta de 12 m a 6 m. A bordo, `V` sigue recorriendo las vistas del viaje,
cuyo asiento junto a la ventanilla es la vista propia del pasajero; una
elección de primera persona vuelve al bajarse.

Las personas son figuras 3D low-poly al estilo PS1/PS2 generadas en código (sin
arte de terceros), de unos 4,200–5,000 triángulos cada una, en colores planos y
terrosos con sombreado suave en la piel y en los miembros redondeados y aristas
duras solo en las alas de los sombreros y en los pliegues del poncho. Cada una
se construye con partes rígidas sobre articulaciones (miembros que se angostan,
un torso más ancho en el pecho, rodillas, pantorrillas y codos modelados a lo
largo de los miembros, una cabeza redondeada con pómulos, mentón, mandíbula,
frente, orejas, nariz y cuencas de los ojos sombreadas, manos con pulgares,
zapatos y botas con suela, puños y costuras, pantalones con raya, un ala de
gacho ondulada y pliegues profundos en polleras y ponchos), y la animación gira
las articulaciones: respiración en espera, una caminata con los brazos en
contrabalanceo y un trote inclinado hacia adelante, al ritmo de la velocidad.
Los dobladillos, la parte inferior del mentón y los extremos inferiores de los
miembros son un poco más oscuros.

La historia comienza en 1919, y cada aspecto sigue la guía de ambientación
[`odd/setting-1919.md`](odd/setting-1919.md): una paleta apagada de polvo,
ocre, azul desteñido, marrón, gris y lana negra, con colores vivos solo en los
acentos (un pañuelo rojo, una faja, botones de bronce).

- **El protagonista** no es un gaucho: es un humilde ex empleado de ferretería
  de General Pico, un pequeño pueblo de provincia, que empieza de nuevo en
  otro. Como la mayoría de la gente de este mundo, no es rico. Usa ropa de
  trabajo de hombre de pueblo: una camisa de trabajo sencilla, una campera de
  cuero marrón gastada y raspada con el cuello vuelto, solapas y bolsillos,
  pantalones desteñidos, botas sencillas usadas y un gacho (un sombrero de
  fieltro blando de ala ancha, con el ala un poco caída). Sin poncho, bombacha
  ni faja: es del pueblo, no del campo. Los trajes, los cuellos duros, las
  corbatas y los bombines son de los pocos señores del pueblo.
- **Los vecinos** tienen cinco aspectos, cada uno un modelo de MakeHuman con
  su propia edad y complexión: un paisano joven con boina, bombacha, faja,
  pañuelo y alpargatas; un peón criollo delgado con bigote, con chaleco,
  sombrero blando de ala ancha y botas gastadas; un peón alemán del Volga
  grandote y barbudo con poncho; y dos mujeres con polleras hasta el tobillo,
  blusas de cuello alto y manga larga, chales cruzados y botas altas con
  cordones, una campesina mayor con pañuelo de cabeza y delantal y una joven
  del pueblo con el pelo recogido en un rodete. La ropa de cada persona tiene
  sus propios colores, de una paleta apagada de 1919.
- **Don Hidalgo** es un estanciero mayor con bigote blanco, un sombrero claro
  de ala ancha, un buen saco sobre un chaleco oscuro, bombacha, botas altas y
  una rastra. **El comisario Elías Farach** lleva un kepí, una guerrera azul
  oscuro con botones de bronce, un cinturón de cuero, pantalones oscuros y
  botas.

Los caballos ensillados son el caballo animado CC0 de Quaternius, el modelo de
los caballos de corral, de alrededor de 1.5 m a la cruz, con un recado y una
cabezada de 1919, atados o montados
(`pampa_render::character::{horse, tack, gait, rider}`):

- **Los pelajes** se pintan por vértice según dónde está cada vértice en el
  caballo, así que los patrones recorren el cuerpo: **zaino** (marrón oscuro
  liso), **tordillo** (gris con manchas más claras), **overo** (castaño con
  grandes manchas blancas y calzado, algo de blanco visto desde cualquier
  lado), **alazán** y **bayo** (arena con crin, cola y patas negras y una raya
  dorsal oscura). El caballo de Don Hidalgo, el que monta el jugador, es bayo;
  el caballo atado en cada uno de los demás almacenes recibe un pelaje con la
  semilla de su pueblo.
- **Los aperos** corresponden a la Argentina rural de 1919: un recado (una
  sudadera tejida, una carona de cuero, los dos bastos y un pellón de cuero de
  oveja encima, una cincha de cuero crudo), aciones y estribos de hierro, y una
  cabezada de cuero crudo (testera, bozal, quijeras, argollas del freno) con
  riendas. Cada capa se envuelve sobre la piel propia del caballo en reposo y
  recibe skinning a sus huesos con los pesos de la piel que tiene debajo, así
  que se dobla con el barril del caballo en cada clip y nunca se mete en él.
  Los estribos de un caballo atado cuelgan y sus riendas descansan sobre la
  cabeza del recado; los pies de un jinete llevan los estribos y las riendas
  corren a lo largo del cuello hasta la mano izquierda del jinete.
- **Andares:** la espera de los clips, luego la caminata al ritmo de la
  velocidad sobre el suelo y estirada hasta unos 4.5 m/s (no hay clip de
  trote), luego el galope. La caminata y el galope cambian de golpe, con el
  nuevo iniciado al paso del anterior (un fundido rápido, nunca mitad y
  mitad); un clip que se desvanece nunca deja atrás un hueso que solo él
  mueve, y los cascos se mantienen en las patas y por encima del suelo. El
  tranco del galope se estira hasta el de un galope real (sus cascos se
  mantienen quietos sobre el suelo hasta unos 14.6 m/s), y el cuerpo se
  flexiona con él: la grupa baja cuando las patas traseras avanzan por debajo
  y sube en el impulso, la cruz cabecea al revés, el cuello bombea y la cola
  lo acompaña. Un caballo atado cambia de espera de vez en cuando; uno fuera de
  la vista o lejos conserva su última pose, pero nunca el del jugador.
- **Montado**, el modelo de MakeHuman del empleado se sienta sobre el hueso del
  lomo del caballo, así que sube, cabecea y gira con el caballo, reproduciendo
  el clip sentado `drive`: las piernas dobladas alrededor del barril hasta los
  estribos y la mano izquierda en las riendas mediante un pequeño IK. Encima,
  resortes con amortiguamiento crítico: su asiento devuelve parte del rebote
  del galope, su columna compensa con retraso el cabeceo y el balanceo de la
  montura, su cabeza se mantiene más estable que la montura, se inclina hacia
  adelante con la velocidad y hacia atrás al frenar, su mano de las riendas
  cede con la boca del caballo y su mano derecha descansa sobre el muslo.
  Quieto unos segundos, su mano se apoya en la cabeza del recado, cambia el
  peso de lado y mira alrededor.

**Pulido, sin triángulos extra.** Cada parte tiene un acabado PBR (cuero un
poco brillante, los botones de bronce y la plata de la rastra metálicos, la
lana y la tela mate, piel satinada), cacheado por color y acabado. La oclusión
ambiental está incorporada en los tonos de los vértices: axilas, entrepierna,
debajo de la mandíbula y de las alas de los sombreros. Pequeñas texturas
pintadas en código (de 64 x 64 cada una) agregan caras (ojos, cejas, una boca,
mapeadas solo sobre el frente de la cabeza), un tejido de tela en bombachas,
ponchos y camisas, grano de cuero y fieltro: cuatro texturas, 64 KB en total,
frente a los 3.7 MB de hojas de sprites que reemplazan. Los sombreros se
inclinan con el paso, los ponchos, los chales y las polleras se mecen, y la
gente parada mira alrededor de vez en cuando. Las sombras del sol usan tres
cascadas sobre los 100 m más cercanos, la primera terminando a 12 m, justo más
allá del jinete a la distancia de cámara por defecto, para sombras de las
figuras más nítidas en el mapa de 2048 texels.

Los vecinos responden: si se choca a alguien con el caballo, se queja en un
globo de diálogo sobre su cabeza ("¡Eh, mirá por dónde andás!" al paso, "¡Casi
me pisa el pingo, che!" al galope). Rozar a alguien muy despacio no provoca
ninguna línea, la misma persona espera unos segundos antes de volver a
quejarse, y la misma línea nunca se repite dos veces seguidas. Los globos son
de papel con un contorno de tinta, rotulados en IM Fell English (con
antialiasing, con minúsculas, acentos y `¡ ¿`), miran a la cámara y se
desvanecen después de unos 3 s.

### El diálogo es datos

Lo que dice la gente vive en archivos [RON](https://github.com/ron-rs/ron)
editables bajo `assets/dialogue/`, no en el código: `bumps.ron` contiene un
grupo de líneas por gravedad de choque (`walk`, `gallop`), y `errand.ron`
contiene lo que dicen Don Hidalgo y el comisario Farach durante el encargo. Se
editan y se reinicia el juego; sin recompilar. El crate `pampa-content` los
interpreta y valida (grupos no vacíos, como máximo 40 caracteres por línea); el
juego comprueba los caracteres contra la tipografía, y recurre a un pequeño
conjunto de líneas incorporadas con un aviso si el archivo falta o no es
válido. Ver [`assets/dialogue/README.md`](assets/dialogue/README.md) para saber
cómo escribir líneas.

### Los pueblos son datos

Los pueblos se generan a partir de configuraciones editables en
[`assets/world/towns.ron`](assets/world/towns.ron), no fijas en el código:
tamaño de la cuadrícula en manzanas, anchos de manzana y de calle, el lado y la
distancia del ferrocarril, la ubicación de la plaza y una semilla para los
lotes. `pampa-core` convierte una `TownConfig` en un trazado
(`TownLayout::generate`), comprobándola y explicando qué está mal; los
edificios, los carteles, el grafo de calles, los palenques, la parada de la
estación y el punto de llegada siguen todos la cuadrícula generada. Eduardo
Castex tiene 4 x 5 manzanas de 40 m con calles de 10 m, y como no tiene una
manzana central su plaza da a la estación al otro lado de la calle, como en
muchos pueblos ferroviarios de la pampa. El juego construye cada pueblo del
archivo en su `origin`, omite (con un aviso que los nombra) los pueblos que se
superponen con uno anterior o que pasan el borde del mundo, y recurre al Castex
incorporado cuando no queda ningún pueblo. El archivo también fija el tamaño
del mundo y el tamaño de los chunks. Ver
[`assets/world/README.md`](assets/world/README.md) para cada campo y cada regla.

## Arquitectura

Un workspace de Cargo con cuatro miembros:

| Crate | Ruta | Función |
|-------|------|------|
| `pampa-core` | `crates/core` | Lógica pura del juego. Sin Bevy, con tests unitarios completos. |
| `pampa-content` | `crates/content` | Contenido editable (diálogo, pueblos) cargado de archivos RON. Sin Bevy. |
| `pampa-render` | `crates/render` | Plugins de Bevy que dibujan el mundo. |
| `game` | `game` | Binario que conecta la entrada, core, content y render. |

La regla: **la lógica del juego nunca sabe cómo se dibujan las cosas.**

- El rumbo es un ángulo continuo en `core`, nunca un enum de 8 direcciones.
- Las formas de colisión pertenecen a la lógica, nunca se derivan de los
  píxeles de los sprites. El trazado del pueblo (`pampa_core::TownLayout`) es
  dueño de cada huella en planta; render solo elige un aspecto por tipo de
  edificio.
- La altura del suelo tiene una única fuente de verdad, `Terrain::height`.
  También nivela el suelo bajo cada pueblo y traza los caminos, así que los
  chunks cargados por streaming siempre se unen sin costuras.
- Los caminos, los pasos a nivel, los carteles indicadores (`pampa_core::road`)
  y los alambrados y las tranqueras (`pampa_core::fence`) se disponen en
  `core`, de forma pura y con semilla. `core` es dueño de sus obstáculos.
  `render` solo los dibuja y desvanece los oclusores delgados a lo largo de la
  línea de visión de la cámara.
- Qué chunks y qué pueblos existen es pura contabilidad (`StreamingSet` en
  `render`, `TownActivation` en `core`), probada sin GPU.
- La animación es un estado lógico (`Idle`, `Walk`, `Gallop`) en `core`, nunca un índice de frame.
- Los vecinos, el reloj y el tren son simulaciones deterministas de `core`
  (`Townsfolk`, una por pueblo; `GameClock`; `LineTrain`, uno para toda la
  línea, cuyo recorrido es una función pura de los segundos reales desde que
  salió); el juego las hace avanzar y render solo las lee. Viajar en él también
  es `core` (`Ride`, `boarding`, `alight_spot`, `skip_ride`, `wait_for_train`,
  `opening`); el juego lee `F` y `Space` y reproduce el fundido
  (`ScreenFade` en `render`).
- El encargo es una máquina de estados de `core` (`Errand`: `MeetHidalgo`,
  `TakeHorse`, `ReportToFarach`, `Done`), una variante del enum por etapa, con
  cada transición protegida por alcance y etapa para que se dispare una sola
  vez. `core` también indica dónde está parada la gente
  (`TownLayout::plaza_spot`, `police_door`, `platform_spot`), y qué altura
  tiene el suelo (`World::ground_height`: el terreno, elevado por los `Floor`
  de los andenes y sus rampas); el juego lee la tecla `F` y reproduce las
  líneas; render dibuja el HUD y el destino.
- Los choques se deciden en `core` (`BumpTracker`: umbral de velocidad,
  gravedad según el andar, enfriamiento por peatón; `LinePicker`: con semilla,
  sin repeticiones inmediatas); las palabras vienen de `pampa-content`; render
  solo dibuja el globo (`Say` → `SpeechBubble`).
- La representación visual es un componente intercambiable por entidad en
  `render` (`CharacterVisual`: `Figure` para una persona a pie, `Horse` para un
  caballo ensillado, `Mounted` para un jinete sobre uno). Insertar un valor
  nuevo cambia lo que se dibuja; las poses de andar puras y el movimiento
  secundario de los vecinos, y el andar del caballo, viven en `core`
  (`pampa_core::pose`).

Bevy está fijado a una versión exacta (`=0.20.0`) y solo se actualiza entre hitos.

## Ejecución

```sh
cargo run -p game
```

En Linux, la compilación enlaza con el `lld` de LLVM a través de `clang` (`.cargo/config.toml`), así que ambos deben estar instalados (Arch: `pacman -S clang lld`). La música del menú se reproduce a través de ALSA, así que también se necesitan sus archivos de desarrollo y `pkg-config` (Arch: `pacman -S alsa-lib pkgconf`; Debian/Ubuntu: `apt install libasound2-dev pkg-config`). Las compilaciones de desarrollo conservan solo las tablas de líneas de nuestros crates y nada de información de depuración de las dependencias. En esta máquina, eso hace que una recompilación incremental después de tocar `crates/render` tarde unos 12 s en lugar de 47 s, y que el binario de desarrollo pese 172 MB en lugar de 1.6 GB.

Comprobaciones:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Controles


El juego abre en una pantalla de título. A la izquierda, un cartel de "Se busca" de Juan Bautista Bairoletto y Mate Cosido (nacido Segundo David Peralta), dos de los bandidos rurales de "Bandidos rurales" de León Gieco, está clavado sobre un mapa de Argentina de 1882. A la derecha, una franja de pintura roja lleva "Pampa Salvaje" y las opciones Jugar, Sonido y Salir. `Esc` (o la tecla `Pause` del teclado) en el juego abre la misma pantalla sobre el mundo. El mundo queda visible, en gris y con las esquinas oscuras, y se queda quieto: el tiempo virtual está en pausa, así que el reloj, los trenes, los animales y la gente esperan. Sonido ajusta el volumen general y el de la música, de 0 a 100 % en pasos de 10. Se guardan, cuando el jugador sale de Sonido o sale del juego con esa pantalla abierta, en `settings.ron`, en el directorio de configuración del usuario (`~/.config/pampa-salvaje/` en Linux, `%APPDATA%\pampa-salvaje\` en Windows). La música del menú (`assets/menu/menu_2.mp3`, la canción propia del proyecto) se repite solo mientras hay un menú abierto, y empieza desde el principio cada vez que se abre el título o el menú de pausa. El recorrido de capturas de desarrollo (`PAMPA_SHOTS`) omite el título. `tools/menu_poster.sh` genera el cartel y la franja. Ver `game/src/menu.rs` y `game/src/sound.rs`.

| Tecla | Acción |
|-----|--------|
| `W` / `S` | Caminar o cabalgar hacia adelante / frenar y retroceder |
| `A` / `D` | Girar a la izquierda / a la derecha |
| `Shift` (mantener) | Trotar a pie, galopar a caballo |
| `F` | Interactuar: hablar con alguien, montar o bajarse de un caballo, subir o bajarse del tren en la puerta de un coche, o esperar el tren cerca del centro de un andén |
| `Space` | A caballo a galope tendido: saltar. Más lento, no hace nada. El arco llega a unos 1.55 m de altura, así que pasa por encima de un alambrado (1.3 m) y de los objetos bajos de un asentamiento (bebedero, brocal de pozo, palenque). Los árboles, los edificios, los corrales, los tanques, las carretas, las personas y los animales siguen estorbando. A bordo del tren: saltar a la siguiente parada |
| `V` | Fuera del tren: alternar la vista en primera persona desde los ojos del jugador. A bordo del tren: recorrer las vistas (asiento junto a la ventanilla, alrededor del coche, el tren entero) |
| Mouse | Orbitar la cámara, o mirar alrededor en primera persona (el cursor queda bloqueado mientras la ventana tiene el foco) |
| `Q` / `E` | Orbitar la cámara a la izquierda / a la derecha, o girar la cabeza en primera persona |
| Rueda del mouse | Acercar / alejar el zoom (6–25 m; no en primera persona) |
| `M` | Alternar el minimapa ampliado |
| `Esc` o `Pause` | Pausa: el mundo se queda quieto y se abre el menú (Continuar, Sonido, Salir); `Esc` o `Pause` de nuevo reanuda, o vuelve desde Sonido |
| `↑` / `↓` o `W` / `S`, luego `Enter`, `Space` o `F` | Elegir en la pantalla de título y en el menú de pausa |
| `←` / `→` o `A` / `D` | En Sonido: bajar / subir el volumen resaltado |

### Consola de desarrollo (solo en compilaciones de depuración)

`Ctrl`+`Shift`+`F12` abre una pequeña consola de trucos abajo a la izquierda en
las compilaciones de depuración (`cargo run -p game`). No se compila en las
compilaciones `--release`: el módulo se declara bajo `cfg(debug_assertions)`.
`Enter` ejecuta la línea, `Esc` o las mismas teclas la cierran, y mientras está
abierta el juego no recibe ninguna tecla.

| Comando | Efecto |
|---------|--------|
| `goto lake` | Ir al desembarcadero de la laguna más cercana (Laguna La Arocena) |
| `goto pico` / `castex` / `nievas` / `metileo` / `caleufu` / `arocena` | Ir a la entrada de un pueblo por su camino principal, o al desembarcadero de una laguna; sirve cualquier palabra del nombre de un pueblo o de una laguna, con acentos opcionales |
| `goto ranch` | Ir al patio de la estancia más cercana |
| `goto ranch castex` / `goto ranch pico` | Ir a la estancia más cercana a ese pueblo |
| `goto herd` | Ir al potrero de la manada de ganado más cercana |
| `goto X Y` | Ir a un punto, en metros del juego |
| `horse` | Poner un caballo ensillado junto al jugador (montable una vez desbloqueada la monta) |
| `turbo on` / `turbo off` | Dejar que el caballo del jugador galope a hasta 70 km/h, para recorrer el mapa rápido, o volver a lo normal |
| `unlock all` | Desbloquear todas las habilidades |
| `models on` / `models off` | Dibujar a las personas a pie (el protagonista, Don Hidalgo, Farach, Bairoletto y los vecinos) como los modelos de MakeHuman con skinning (por defecto), o de vuelta como las viejas figuras rígidas |
| `time 21` / `time 21:30` | Adelantar el reloj hasta esa hora (más tarde hoy, o mañana si ya pasó) |
| `view fp` / `view tp` | Mirar desde los ojos del jugador, o desde la cámara de órbita (como `V` fuera del tren) |
| `help` | Listar los comandos |

Los modelos de MakeHuman (`assets/characters/`, construidos con
`tools/mh2gltf.py`) visten un guardarropa de 1919, en su mayor parte recortado
del cuerpo por el conversor: camisas y chalecos, bombachas, botas hasta la
rodilla, una guerrera de policía con botones de bronce, un traje de tres
piezas, sombreros de fieltro, un kepí, una boina, pañuelos de cuello, un
poncho, la faja y la rastra, y para las mujeres blusas, polleras largas
acampanadas, delantales, chales cruzados, un pañuelo de cabeza y el pelo
recogido en un rodete. Se mueven con los clips de animación CC0 de Quaternius,
adaptados (retargeting) a su esqueleto con `tools/retarget.py`: están en
espera, caminan, trotan y corren, con los clips al ritmo de la velocidad sobre
el suelo (y de las piernas de cada persona) para que los pies no se deslicen.
La caminata de juego agachada del paquete se endereza hasta una caminata de
calle tranquila y erguida. La ropa teñida de los vecinos son copias de
materiales compartidas, y los brazos de las mujeres cuelgan un poco hacia
afuera sobre sus polleras. Farach espera con los brazos cruzados, y quien
habla lo hace con las manos. En la montura, el protagonista reproduce el clip
sentado `drive` sobre el lomo del caballo. Son los modelos por defecto, también
en las compilaciones de release; `models off` trae de vuelta las viejas figuras
rígidas de las personas a pie en una compilación de depuración (los jinetes y
los caballos ensillados ya no tienen figuras rígidas). Probar `goto castex`
(Farach con Bairoletto a su lado) o `goto nievas` (Don Hidalgo).

`goto` pone al jugador, y al caballo si cabalga, en el terreno despejado y
seco más cercano al lugar, y el mundo se carga por streaming a su alrededor de
inmediato. A bordo del tren se niega ("not on the train: get off first"):
primero hay que bajarse.

## Créditos

Los assets de terceros (texturas de suelo CC0 de ambientCG, modelos de vegetación CC0 del Ultimate Nature Pack de Quaternius, los assets CC0 de MakeHuman de los personajes, y sus clips de animación CC0 de la Universal Animation Library de Quaternius) están listados con su autor, su origen, su licencia y sus cambios en [CREDITS.md](CREDITS.md), y de nuevo junto a sus archivos.

## Licencia

Licenciado bajo cualquiera de

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) o <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) o <https://opensource.org/licenses/MIT>)

a su elección.

### Contribuciones

Salvo que usted indique explícitamente lo contrario, cualquier contribución
enviada intencionalmente por usted para su inclusión en la obra, según se
define en la licencia Apache-2.0, tendrá doble licencia como se indica arriba,
sin términos ni condiciones adicionales.
