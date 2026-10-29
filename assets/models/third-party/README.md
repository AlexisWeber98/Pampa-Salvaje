# Ranuras de modelo

Una ranura de modelo permite que un modelo glTF descargado reemplace una estructura procedural de los asentamientos rurales sin cambiar el código. Cada ranura es una carpeta aquí con el nombre de la ranura. Cuando la carpeta contiene `model.glb` o `model.gltf` (gana el `.glb`), cada estructura de ese tipo dibuja el modelo en lugar de su mesh procedural. Cuando no, se dibuja la mesh procedural.

- Mientras el modelo carga, la mesh procedural actúa como sustituto.
- Si el modelo no carga (un archivo roto, una textura faltante), la mesh procedural se queda y el log indica por qué.
- Los obstáculos, las zonas vedadas para la flora y los oclusores de la cámara salen del trazado del juego y no cambian con el modelo. Un modelo debería quedar dentro de la huella en planta de su estructura.
- Cada modelo se dibuja dentro del rango de visibilidad compartido de los objetos de los asentamientos, y se desvanece para no tapar la cámara como la mesh procedural que reemplaza.

## Ranuras

| Ranura | Estructura | Tamaño que le da el trazado | Ocupada por |
|---|---|---|---|
| `windpump` | Molino: torre, rueda y veleta de cola | Base cuadrada de 2.4 m, cubo de la rueda a unos 10.5 m | Molino de 3TD (convertido) |
| `wrecked_wagon` | Carreta abandonada en un patio o en una banquina | 4.8 m de largo, 3.6 m de profundidad, hasta 2.2 m de alto | Carreta vieja de 3TD (convertida) |
| `cart` | Carreta: carro de bueyes de dos ruedas | 5 m de largo, 2.2 m de ancho, 2.5 m de alto | Procedural |
| `well` | Brocal de pozo de ladrillo con su armazón | 1.6 m de lado | Procedural |
| `trough` | Bebedero | 6–8 m de largo, 0.9 m de ancho | Procedural |
| `haystack` | Parva | 5–7 m de lado, 4 m de alto | Procedural |
| `hitching_post` | Palenque | 3 m de largo | Procedural |
| `eucalyptus` | Eucalipto de la isleta | 18–28 m de alto | Procedural (tronco del modelo, copa procedural) |
| `poplar` | Álamo criollo de la alameda | 14–20 m de alto | Procedural (tronco del modelo, copa procedural) |

Los nombres vienen de `StructureKind::slot` en `crates/core/src/settlement.rs`. Un test comprueba que cada carpeta de aquí (salvo las fuentes de `3td-starter-pack`) sea una ranura conocida con un modelo adentro.

## Ajustar un modelo: `slots.ron`

`slots.ron` indica cómo se ubica cada modelo en el espacio propio de su estructura. Ese espacio tiene `x` a lo largo del frente de la estructura, `y` hacia arriba desde el suelo en su centro, y `z` hacia afuera de su frente (hacia el patio del asentamiento). Una ranura con modelo y sin entrada se dibuja tal como viene.

```ron
(
    slots: {
        "windpump": (
            scale: 0.875,               // escala uniforme, se aplica primero
            offset: (0.0, 0.0, 0.0),    // metros, después de la escala y el yaw
            yaw: 0.0,                   // grados alrededor de la vertical
            rotor: Some((node: "rotor", axis: (0.0, 0.0, -1.0))),
        ),
    },
)
```

| Campo | Por defecto | Significado |
|---|---|---|
| `scale` | `1.0` | Escala uniforme. Para las ranuras de árboles es por metro de altura del árbol: modelar el árbol de 1 m de alto, o fijar `scale` en 1 dividido por su altura. |
| `offset` | `(0, 0, 0)` | Desplazamiento en metros, en el espacio de la estructura. |
| `yaw` | `0.0` | Giro alrededor de la vertical, en grados. |
| `rotor` | `None` | El nodo glTF que gira (la rueda de un molino), por nombre, y su eje en el espacio propio de ese nodo. Gira entre 1.4 y 2.4 radianes por segundo. |

Un nombre de ranura desconocido, una escala que no es positiva, un número no finito, o un rotor sin nombre o con un eje nulo hacen que todo el archivo sea inválido: el juego avisa y dibuja cada modelo sin ajustar.

## Agregar un modelo

1. Comprobar la licencia **en la página de origen del asset**. CC0 y CC-BY sirven. Anotar el autor, la URL de la página y la licencia.
2. Poner el modelo en `<slot>/model.glb` (o `model.gltf` con su `.bin` y sus texturas al lado). Mantenerlo dentro de los presupuestos de triángulos: objetos 0.5–5k, el molino 8–15k.
3. Un modelo COLLADA (`.dae`) se puede convertir con `tools/dae2gltf.py` (ver `tools/README.md`).
4. Ajustarlo en `slots.ron` y comprobarlo en el juego.
5. Darle crédito dos veces:
   - en un `CREDITS.md` junto a sus archivos (el de la carpeta, o el del paquete de origen): asset, autor, URL de origen, licencia, la fecha en que se descargó, y qué se cambió;
   - en el `CREDITS.md` de la raíz, en la tabla de modelos.
   Los modelos CC-BY deben conservar la línea de crédito del autor exactamente como lo pide la licencia.
