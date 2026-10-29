//! Personas 3D low-poly, construidas con piezas rígidas facetadas como los
//! edificios y el tren: colores lisos, tonos por vertex con oclusión ambiental
//! horneada, un acabado PBR por material y unas pocas texturas pequeñas
//! pintadas (ver [`finish`](super::finish)).
//!
//! Una figura es una entidad de cuerpo (girada hacia el rumbo) que contiene un
//! árbol de entidades de articulación: pelvis, cintura, cuello, hombros, codos,
//! caderas y rodillas. Cada pieza (cabeza, torso, un antebrazo, el ala de un
//! sombrero...) es una mesh unitaria escalada colgada de su articulación, así
//! que la animación solo gira articulaciones (ver [`pampa_core::pose`]). Todas
//! las figuras comparten las mismas cuatro meshes unitarias y un material por
//! color, así que no se construye nada por figura ni por frame.
//!
//! La figura mira hacia el +X local, su derecha es +Z y sus pies se apoyan en
//! el origen del cuerpo, a unos 1.75 m por debajo de la parte superior de su
//! cabeza.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use pampa_core::{AnimationState, Heading, JointAngles, pose};

use super::finish::{Finish, Palette, face_uvs, occlude, ramp};

/// Un color RGB en bytes sRGB.
pub type Rgb = [u8; 3];

/// Cuántos aspectos de vecinos hay; el `look` de la lógica elige uno.
pub const TOWNSFOLK_LOOKS: u8 = 5;

/// Cómo se dibuja una figura.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FigureKind {
    /// El jugador a pie: un empleado de ferretería de General Pico con su ropa
    /// de ciudad.
    Protagonist,
    /// Un vecino con un `look` de la lógica: `look % TOWNSFOLK_LOOKS` elige el
    /// atuendo y el resto, los colores que lleva (ver
    /// [`townsfolk`](super::townsfolk)).
    Townsfolk(u8),
    /// Don Hidalgo, el viejo estanciero que da el primer mandado.
    Hidalgo,
    /// El comisario Elías Farach, de uniforme.
    Farach,
    /// Juan Bautista Bairoletto, de 25 años en 1919: camisa negra, pañuelo de
    /// cuello blanco, un chambergo claro; parado al lado de Farach.
    Bairoletto,
}

impl FigureKind {
    /// Un vecino según un valor de aspecto de la lógica.
    pub fn townsfolk(look: u8) -> Self {
        Self::Townsfolk(look)
    }

    /// Todas las figuras.
    pub fn all() -> impl Iterator<Item = Self> {
        [
            Self::Protagonist,
            Self::Hidalgo,
            Self::Farach,
            Self::Bairoletto,
        ]
        .into_iter()
        .chain((0..TOWNSFOLK_LOOKS).map(Self::Townsfolk))
    }
}

/// Un pivote que gira la animación. La izquierda es -Z y la derecha, +Z.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Joint {
    Pelvis,
    Waist,
    Neck,
    Shoulder(Side),
    Elbow(Side),
    Hip(Side),
    Knee(Side),
    /// Dónde se apoya el ala del sombrero sobre la cabeza; el sombrero se
    /// inclina con el paso.
    Brim,
    /// Los hombros de los que cuelga un poncho o un chal.
    Drape,
    /// La cintura de la que cuelga una pollera.
    Hem,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    pub(super) const BOTH: [Self; 2] = [Self::Left, Self::Right];

    pub(super) fn index(self) -> usize {
        self as usize
    }

    /// Signo a lo largo del eje Z del cuerpo.
    pub(super) fn z(self) -> f32 {
        match self {
            Self::Left => -1.0,
            Self::Right => 1.0,
        }
    }
}

impl Joint {
    /// Todas las articulaciones, cada una después de su padre.
    pub const ALL: [Self; 14] = [
        Self::Pelvis,
        Self::Waist,
        Self::Neck,
        Self::Shoulder(Side::Left),
        Self::Shoulder(Side::Right),
        Self::Elbow(Side::Left),
        Self::Elbow(Side::Right),
        Self::Hip(Side::Left),
        Self::Hip(Side::Right),
        Self::Knee(Side::Left),
        Self::Knee(Side::Right),
        Self::Brim,
        Self::Drape,
        Self::Hem,
    ];

    /// La articulación de la que cuelga esta (`None`: el cuerpo).
    pub fn parent(self) -> Option<Self> {
        match self {
            Self::Pelvis => None,
            Self::Waist | Self::Hip(_) | Self::Hem => Some(Self::Pelvis),
            Self::Neck | Self::Shoulder(_) | Self::Drape => Some(Self::Waist),
            Self::Brim => Some(Self::Neck),
            Self::Elbow(side) => Some(Self::Shoulder(side)),
            Self::Knee(side) => Some(Self::Hip(side)),
        }
    }

    /// Desplazamiento desde la articulación padre (o desde los pies) estando de
    /// pie.
    pub fn rest(self) -> Vec3 {
        match self {
            Self::Pelvis => Vec3::new(0.0, 0.92, 0.0),
            Self::Waist => Vec3::new(0.0, 0.08, 0.0),
            Self::Neck => Vec3::new(0.0, 0.46, 0.0),
            Self::Shoulder(s) => Vec3::new(0.0, 0.42, 0.21 * s.z()),
            Self::Elbow(_) => Vec3::new(0.0, -0.29, 0.0),
            Self::Hip(s) => Vec3::new(0.0, 0.0, 0.09 * s.z()),
            Self::Knee(_) => Vec3::new(0.0, -0.42, 0.0),
            Self::Brim => Vec3::new(0.0, 0.25, 0.0),
            Self::Drape => Vec3::new(0.0, 0.44, 0.0),
            Self::Hem => Vec3::ZERO,
        }
    }

    /// Posición de pie por encima de los pies.
    pub fn rest_position(self) -> Vec3 {
        self.rest() + self.parent().map_or(Vec3::ZERO, Self::rest_position)
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|j| *j == self).unwrap_or(0)
    }

    /// El giro de esta articulación en `angles`, alrededor del eje lateral del
    /// cuerpo; la cabeza además gira hacia un costado según `glance`.
    pub(super) fn rotation(self, a: &JointAngles, glance: f32) -> Quat {
        // Una Z positiva gira un miembro que cuelga hacia +X (hacia adelante).
        match self {
            Self::Hip(s) => Quat::from_rotation_z(a.hip[s.index()]),
            Self::Knee(s) => Quat::from_rotation_z(a.knee[s.index()]),
            Self::Pelvis => Quat::IDENTITY,
            Self::Waist => Quat::from_rotation_z(-a.lean),
            // La cabeza se mantiene casi nivelada mientras el torso se inclina.
            Self::Neck => {
                Quat::from_rotation_y(glance) * Quat::from_rotation_z(0.6 * a.lean - a.nod)
            }
            Self::Brim => Quat::from_rotation_z(-a.hat),
            Self::Drape | Self::Hem => {
                Quat::from_rotation_z(a.drape) * Quat::from_rotation_x(-a.drape_side)
            }
            Self::Shoulder(s) => Quat::from_rotation_z(a.shoulder[s.index()]),
            Self::Elbow(s) => Quat::from_rotation_z(a.elbow[s.index()]),
        }
    }
}

/// Las meshes unitarias compartidas de las que las piezas son copias escaladas.
/// Cada una cabe en una caja de 1 m centrada en el origen, así que el tamaño de
/// una pieza es su caja envolvente.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Shape {
    /// Miembros que se afinan, con los extremos redondeados: muslo, canilla,
    /// brazo, antebrazo.
    Thigh,
    Shin,
    /// La parte baja de la pierna de una bombacha: holgada, fruncida en el
    /// tobillo.
    BaggyShin,
    UpperArm,
    Forearm,
    Neck,
    /// Bloque redondeado de una mano.
    Hand,
    /// Un zapato a lo largo de +X: talón, empeine y una punta más baja.
    Shoe,
    /// Una bota de trabajo simple a lo largo de +X, con la caña subiendo en el
    /// talón.
    Boot,
    /// Pelvis redondeada.
    Hips,
    /// Pecho más ancho que la cintura, con hombros.
    Torso,
    /// El torso de una chaqueta o una túnica, acampanado en el ruedo.
    Coat,
    /// Una chaqueta de cuero gastada: el corte del saco con un sombreado
    /// raspado.
    Jacket,
    /// Un panel delantero delgado (chaleco, pechera de camisa), acampanado en
    /// el ruedo.
    Panel,
    /// La cabeza: una esfera suave de 18 lados y 12 anillos.
    Head,
    /// Un pequeño bulto redondeado (mandíbula, pelo, boina, nudo, hebilla).
    Lump,
    /// Una protuberancia diminuta (nariz, bigote, botones).
    Bump,
    /// Un disco plano y redondo con aristas vivas (alas de sombrero, cintas, la
    /// visera del quepí).
    Disc,
    /// Un anillo más claro para puños, costuras y suelas.
    Band,
    /// Un ala blanda inclinada levemente hacia abajo (el gacho).
    Brim,
    /// Una copa de sombrero, un poco más angosta arriba.
    Crown,
    /// Una pollera larga acampanada hacia el ruedo.
    Skirt,
    /// Un poncho que cae sobre los hombros, con pliegues marcados.
    Poncho,
}

/// Un anillo de un loft: altura, semiextensiones a lo largo de X y Z,
/// desplazamiento en X y un tono que se multiplica en el color (para oscurecer
/// de forma barata los ruedos y las articulaciones).
#[derive(Debug, Clone, Copy)]
pub(super) struct Ring(pub f32, pub f32, pub f32, pub f32, pub f32);

/// Un anillo redondo de radio `r` en `y`.
pub(super) const fn round(y: f32, r: f32) -> Ring {
    Ring(y, r, r, 0.0, 1.0)
}

/// Lados alrededor de los miembros, el cuello, las manos y los pies.
pub(super) const LIMB_SIDES: usize = 14;
/// Lados alrededor del torso, las caderas y los sacos.
pub(super) const BODY_SIDES: usize = 16;

/// Un miembro desde un radio `top` hasta un radio `bottom`, con los extremos
/// redondeados, más oscuro hacia su extremo inferior. Su músculo se hincha un
/// poco por encima del centro y se angosta antes de la articulación inferior,
/// para que se distingan las rodillas, las pantorrillas y los codos.
pub(super) fn limb(top: f32, bottom: f32) -> Vec<Ring> {
    let at = |t: f32| bottom + (top - bottom) * t;
    let shaded = |ring: Ring, shade: f32| Ring(ring.0, ring.1, ring.2, 0.0, shade);
    vec![
        shaded(round(-0.5, bottom * 0.45), 0.86),
        shaded(round(-0.45, bottom * 0.86), 0.88),
        shaded(round(-0.36, bottom), 0.92),
        shaded(round(-0.12, at(0.3) * 0.96), 0.96),
        round(0.12, at(0.8)),
        round(0.36, top),
        round(0.45, top * 0.86),
        round(0.5, top * 0.45),
    ]
}

impl Shape {
    pub const ALL: [Self; 23] = [
        Self::Band,
        Self::Boot,
        Self::Jacket,
        Self::Brim,
        Self::Thigh,
        Self::Shin,
        Self::BaggyShin,
        Self::UpperArm,
        Self::Forearm,
        Self::Neck,
        Self::Hand,
        Self::Shoe,
        Self::Hips,
        Self::Torso,
        Self::Coat,
        Self::Panel,
        Self::Head,
        Self::Lump,
        Self::Bump,
        Self::Disc,
        Self::Crown,
        Self::Skirt,
        Self::Poncho,
    ];

    /// Construye esta mesh unitaria con su oclusión ambiental horneada en los
    /// tonos de los vertices: axilas, entrepierna, debajo de la mandíbula y
    /// debajo de las alas.
    pub fn mesh(self) -> Mesh {
        let mesh = self.build();
        let under = |n: Vec3, from: f32| ramp(-n.y, from, 0.95);
        match self {
            Self::Torso | Self::Coat | Self::Jacket => occlude(mesh, |p, _| {
                let armpit =
                    ramp(p.z.abs(), 0.34, 0.48) * (1.0 - ramp((p.y - 0.22).abs(), 0.0, 0.18));
                1.0 - 0.32 * armpit
            }),
            Self::Hips => occlude(mesh, |p, _| {
                1.0 - 0.35 * ramp(-p.y, 0.2, 0.45) * (1.0 - ramp(p.z.abs(), 0.15, 0.35))
            }),
            Self::Thigh | Self::UpperArm => occlude(mesh, |p, _| 1.0 - 0.22 * ramp(p.y, 0.3, 0.5)),
            Self::Head => occlude(mesh, |p, n| {
                1.0 - 0.3 * under(n, 0.3) * ramp(-p.y, 0.1, 0.4)
            }),
            Self::Lump => occlude(mesh, |_, n| 1.0 - 0.25 * under(n, 0.4)),
            Self::Disc | Self::Brim | Self::Crown => {
                occlude(mesh, |_, n| 1.0 - 0.4 * under(n, 0.5))
            }
            _ => mesh,
        }
    }

    /// Construye la mesh unitaria desnuda. La piel y las partes redondeadas del
    /// cuerpo reciben normales suavizadas; las alas y el poncho conservan
    /// aristas vivas.
    fn build(self) -> Mesh {
        let sphere = Sphere::new(0.5);
        match self {
            // Los pantalones se arrugan hacia la rodilla.
            Self::Thigh => loft_cloth(
                &limb(0.5, 0.36),
                LIMB_SIDES,
                Cloth {
                    folds: 3,
                    depth: 0.08,
                    ..Cloth::PLAIN
                },
            ),
            Self::Shin => loft(&limb(0.5, 0.34), LIMB_SIDES),
            Self::BaggyShin => loft(
                &[
                    Ring(-0.5, 0.18, 0.18, 0.0, 0.85),
                    Ring(-0.45, 0.3, 0.3, 0.0, 0.85),
                    round(-0.3, 0.5),
                    round(0.05, 0.48),
                    round(0.35, 0.44),
                    round(0.46, 0.38),
                    round(0.5, 0.2),
                ],
                LIMB_SIDES,
            ),
            Self::UpperArm => loft(&limb(0.5, 0.4), LIMB_SIDES),
            Self::Forearm => loft(&limb(0.5, 0.34), LIMB_SIDES),
            Self::Neck => loft(
                &[
                    round(-0.5, 0.4),
                    round(-0.3, 0.5),
                    Ring(0.3, 0.45, 0.45, 0.0, 0.72),
                    Ring(0.5, 0.3, 0.3, 0.0, 0.7),
                ],
                LIMB_SIDES,
            ),
            Self::Hand => loft(
                &[
                    Ring(-0.5, 0.3, 0.24, 0.0, 0.95),
                    Ring(-0.35, 0.45, 0.42, 0.0, 1.0),
                    Ring(0.1, 0.5, 0.5, 0.0, 1.0),
                    Ring(0.4, 0.42, 0.45, 0.0, 0.9),
                    Ring(0.5, 0.28, 0.3, 0.0, 0.9),
                ],
                8,
            ),
            // Loft del talón a la punta a lo largo de Y, y después acostado a
            // lo largo de +X: la semiextensión en X de un anillo pasa a ser la
            // altura y su desplazamiento baja la punta.
            Self::Shoe => loft(
                &[
                    Ring(-0.5, 0.36, 0.3, 0.0, 0.9),
                    Ring(-0.42, 0.5, 0.42, 0.0, 1.0),
                    Ring(-0.1, 0.5, 0.45, 0.0, 1.0),
                    Ring(0.2, 0.4, 0.5, 0.1, 1.0),
                    Ring(0.4, 0.3, 0.42, 0.18, 1.0),
                    Ring(0.5, 0.15, 0.24, 0.25, 1.0),
                ],
                LIMB_SIDES,
            )
            .rotated_by(Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2)),
            // Una bota de trabajo: como el zapato, con una caña que sube en el
            // talón.
            Self::Boot => loft(
                &[
                    Ring(-0.5, 0.5, 0.3, -0.0, 0.85),
                    Ring(-0.42, 0.5, 0.42, 0.0, 0.95),
                    Ring(-0.1, 0.5, 0.45, 0.0, 1.0),
                    Ring(0.2, 0.3, 0.5, 0.2, 0.95),
                    Ring(0.4, 0.22, 0.42, 0.28, 0.9),
                    Ring(0.5, 0.12, 0.24, 0.33, 0.9),
                ],
                LIMB_SIDES,
            )
            .rotated_by(Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2)),
            Self::Hips => loft(
                &[
                    Ring(-0.5, 0.3, 0.36, 0.0, 0.8),
                    Ring(-0.3, 0.45, 0.48, 0.0, 0.9),
                    round(0.1, 0.5),
                    Ring(0.5, 0.45, 0.46, 0.0, 1.0),
                ],
                BODY_SIDES,
            ),
            Self::Torso => loft(&torso(0.4, 0.36), BODY_SIDES),
            Self::Coat => loft(&torso(0.5, 0.48), BODY_SIDES),
            // Cuero gastado: el corte del saco con un sombreado raspado e
            // irregular.
            Self::Jacket => loft_cloth(
                &torso(0.5, 0.48),
                BODY_SIDES,
                Cloth {
                    scuff: 0.16,
                    ..Cloth::PLAIN
                },
            ),
            Self::Panel => loft(
                &[
                    Ring(-0.5, 0.5, 0.5, 0.0, 0.85),
                    Ring(-0.3, 0.5, 0.42, 0.0, 1.0),
                    Ring(0.2, 0.5, 0.38, 0.0, 1.0),
                    Ring(0.5, 0.5, 0.2, 0.0, 1.0),
                ],
                8,
            ),
            // Doce anillos les dan su curva a los pómulos y al mentón.
            Self::Head => face_uvs(sphere.mesh().uv(18, 12)),
            Self::Lump => sphere
                .mesh()
                .ico(1)
                .unwrap_or_else(|_| sphere.mesh().uv(10, 6)),
            Self::Bump => sphere.mesh().uv(8, 5),
            Self::Disc => Cylinder::new(0.5, 1.0).mesh().resolution(24).build(),
            // Un anillo claro para puños, costuras y suelas.
            Self::Band => Cylinder::new(0.5, 1.0).mesh().resolution(14).build(),
            // Un ala blanda y ancha inclinada levemente hacia abajo todo
            // alrededor.
            Self::Brim => loft_cloth(
                &[
                    Ring(-0.5, 0.5, 0.5, 0.0, 0.85),
                    Ring(-0.1, 0.44, 0.44, 0.0, 0.95),
                    round(0.3, 0.3),
                    round(0.5, 0.24),
                ],
                24,
                Cloth {
                    folds: 5,
                    depth: 0.06,
                    ..Cloth::PLAIN
                },
            ),
            Self::Crown => loft(
                &[
                    round(-0.5, 0.5),
                    round(0.1, 0.48),
                    Ring(0.38, 0.42, 0.4, 0.0, 1.0),
                    Ring(0.5, 0.3, 0.26, 0.0, 1.0),
                ],
                14,
            ),
            Self::Skirt => loft_cloth(
                &[
                    Ring(-0.5, 0.5, 0.5, 0.0, 0.78),
                    Ring(-0.42, 0.48, 0.48, 0.0, 0.9),
                    round(-0.1, 0.4),
                    round(0.3, 0.32),
                    round(0.5, 0.28),
                ],
                24,
                Cloth {
                    folds: 12,
                    depth: 0.2,
                    ..Cloth::PLAIN
                },
            ),
            Self::Poncho => loft_cloth(
                &[
                    Ring(-0.5, 0.5, 0.5, 0.0, 0.8),
                    Ring(-0.38, 0.48, 0.49, 0.0, 0.92),
                    round(0.1, 0.36),
                    round(0.38, 0.26),
                    round(0.5, 0.14),
                ],
                20,
                Cloth {
                    folds: 10,
                    depth: 0.22,
                    ..Cloth::PLAIN
                },
            )
            .with_duplicated_vertices()
            .with_computed_flat_normals(),
        }
    }
}

/// Un torso desde la cintura (o el ruedo) hasta los hombros y la base del
/// cuello: `hem` es el semiancho abajo y `waist`, un poco por encima.
fn torso(hem: f32, waist: f32) -> Vec<Ring> {
    vec![
        Ring(-0.5, hem * 0.9, hem, 0.0, 0.82),
        Ring(-0.36, waist * 0.95, waist, 0.0, 0.92),
        Ring(-0.12, 0.44, 0.42, 0.0, 1.0),
        Ring(0.15, 0.5, 0.47, 0.0, 1.0),
        Ring(0.32, 0.47, 0.5, 0.0, 1.0),
        Ring(0.42, 0.38, 0.46, 0.0, 0.95),
        Ring(0.48, 0.25, 0.3, 0.0, 0.9),
        Ring(0.5, 0.12, 0.12, 0.0, 0.9),
    ]
}

/// Cómo cae la tela de un loft: `folds` pliegues verticales, que se profundizan
/// hacia el ruedo hasta `depth` (una fracción del radio), y `scuff`, qué tan
/// irregular es el desgaste de su sombreado.
#[derive(Debug, Clone, Copy)]
pub(super) struct Cloth {
    pub folds: u32,
    pub depth: f32,
    pub scuff: f32,
}

impl Cloth {
    pub const PLAIN: Self = Self {
        folds: 0,
        depth: 0.0,
        scuff: 0.0,
    };
}

/// Una superficie cerrada a través de `rings` (de abajo hacia arriba), con
/// `segments` lados alrededor, normales suavizadas y tonos por vertex.
pub(super) fn loft(rings: &[Ring], segments: usize) -> Mesh {
    loft_cloth(rings, segments, Cloth::PLAIN)
}

/// [`loft`] con pliegues y desgaste (ver [`Cloth`]).
pub(super) fn loft_cloth(rings: &[Ring], segments: usize, cloth: Cloth) -> Mesh {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{Indices, PrimitiveTopology};
    use std::f32::consts::TAU;
    let n = segments;
    // Cada anillo tiene n + 1 vertices: el primero y el último son el mismo
    // punto atrás (-X), así que las texturas dan la vuelta con la costura
    // detrás; `u` vale 0.5 en el frente (+X), y `v` va del anillo inferior (0)
    // al superior (1).
    let row = n + 1;
    let mut positions = Vec::with_capacity(rings.len() * row + 2);
    let mut colors = Vec::with_capacity(rings.len() * row + 2);
    let mut uvs = Vec::with_capacity(rings.len() * row + 2);
    for (r, &Ring(y, rx, rz, dx, shade)) in rings.iter().enumerate() {
        for kk in 0..row {
            let k = (kk + n / 2) % n;
            uvs.push([kk as f32 / n as f32, y + 0.5]);
            let angle = k as f32 / n as f32 * TAU;
            let (s, c) = angle.sin_cos();
            // Los pliegues se meten entre sus crestas, sobre todo en el ruedo
            // (y = -0.5).
            let fold = (1.0 - (cloth.folds as f32 * angle).cos()) / 2.0;
            let pull = 1.0 - cloth.depth * (0.5 - y) * fold;
            positions.push([dx + rx * pull * c, y, rz * pull * s]);
            // Desgaste determinista: un hash barato del vertex.
            let wear = ((r * 31 + k * 17) % 7) as f32 / 6.0;
            let shade = shade * (1.0 - cloth.scuff * wear) * (1.0 - 0.25 * cloth.depth * fold);
            colors.push([shade, shade, shade, 1.0]);
        }
    }
    let mut indices: Vec<u32> = Vec::new();
    for r in 0..rings.len().saturating_sub(1) {
        for k in 0..n {
            let at = |ring: usize, k: usize| (ring * row + k) as u32;
            let (a, b, c, d) = (at(r, k), at(r, k + 1), at(r + 1, k), at(r + 1, k + 1));
            indices.extend([a, c, b, b, c, d]);
        }
    }
    // Cerrar ambos extremos con un abanico alrededor de un punto central.
    let last = rings.len() - 1;
    for (ring, bottom) in [(0, true), (last, false)] {
        let Ring(y, _, _, dx, shade) = rings[ring];
        let center = positions.len() as u32;
        positions.push([dx, y, 0.0]);
        colors.push([shade, shade, shade, 1.0]);
        uvs.push([0.5, y + 0.5]);
        for k in 0..n {
            let a = (ring * row + k) as u32;
            let b = (ring * row + k + 1) as u32;
            indices.extend(if bottom {
                [center, a, b]
            } else {
                [center, b, a]
            });
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices))
    .with_computed_smooth_normals();
    // Las dos copias de cada vertex de la costura comparten una normal, así que
    // no se ve ningún pliegue a lo largo de la espalda.
    if let Some(bevy::mesh::VertexAttributeValues::Float32x3(normals)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_NORMAL)
    {
        for r in 0..rings.len() {
            let (first, last) = (r * row, r * row + n);
            let joined =
                (Vec3::from(normals[first]) + Vec3::from(normals[last])).normalize_or_zero();
            normals[first] = joined.into();
            normals[last] = joined.into();
        }
    }
    mesh
}

/// Qué es una pieza; los tests y los accesorios se guían por esto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Piece {
    Head,
    Jaw,
    /// La marca del frente: va en la cara, a lo largo de +X.
    Nose,
    Hair,
    Neck,
    Torso,
    Hips,
    UpperArm,
    Forearm,
    Hand,
    UpperLeg,
    LowerLeg,
    Foot,
    /// La parte delantera de un chaleco.
    Vest,
    /// La pechera de la camisa que se ve en una chaqueta abierta.
    Shirt,
    /// Un cuello alto: el cuello duro de la camisa de ciudad, o el cuello alto
    /// de una blusa.
    Collar,
    Tie,
    /// Pañuelo anudado al cuello.
    Neckerchief,
    Button,
    /// La rastra o el cinturón de cuero, y la hebilla de plata de la rastra.
    Belt,
    Buckle,
    /// Una faja de color alrededor de la cintura.
    Faja,
    Moustache,
    Poncho,
    Shawl,
    Skirt,
    /// Un sombrero de fieltro de ala ancha: ala y copa.
    Hat,
    /// El bombín del hombre de ciudad: una copa redonda y un ala angosta.
    Bowler,
    /// El gacho: un sombrero de fieltro blando de ala ancha, con el ala
    /// inclinada levemente hacia abajo.
    Gacho,
    /// El cuello y las solapas de la chaqueta de cuero.
    Lapel,
    Pocket,
    /// Detalles pequeños de la cara: el arco de las cejas, las orejas y las
    /// cuencas de los ojos en sombra.
    Brow,
    Ear,
    Eye,
    Thumb,
    /// Los puños de las mangas o de la camisa, la costura del ruedo de la
    /// chaqueta, la costura de la copa del quepí.
    Cuff,
    Seam,
    /// Las suelas de las botas y los zapatos con sus tacos.
    Sole,
    /// El pelo recogido en un rodete.
    Bun,
    Boina,
    Kepi,
    Headscarf,
}

/// Una pieza de una figura, en reposo: un [`Shape`] unitario escalado centrado
/// en `center` respecto de su articulación.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PartSpec {
    pub piece: Piece,
    pub joint: Joint,
    pub shape: Shape,
    pub color: Rgb,
    pub center: Vec3,
    pub size: Vec3,
    /// De qué está hecha.
    pub finish: Finish,
}

/// El sombrero de un aspecto.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Headwear {
    /// El bombín de ciudad con una cinta oscura, que se conserva para los
    /// caballeros del pueblo de la guía de ambientación; todavía no lo usa
    /// nadie.
    #[expect(dead_code, reason = "for the later town-gentleman looks")]
    Bowler {
        felt: Rgb,
        band: Rgb,
    },
    /// Un sombrero de campo blando de ala ancha.
    FeltHat(Rgb),
    /// El gacho: un sombrero de fieltro blando de ala ancha, con el ala
    /// inclinada levemente hacia abajo, una copa hundida y una cinta.
    Gacho {
        felt: Rgb,
        band: Rgb,
    },
    /// Sin sombrero: el pelo recogido en un rodete.
    Bun,
    Boina(Rgb),
    Kepi {
        cloth: Rgb,
        visor: Rgb,
    },
    Headscarf(Rgb),
}

/// La ropa y los colores de una figura.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Look {
    skin: Rgb,
    hair: Rgb,
    /// Chaqueta, camisa, blusa o túnica: torso y mangas.
    coat: Rgb,
    /// Una chaqueta o una túnica, acampanada sobre las caderas.
    jacket: bool,
    /// Chaleco: un panel delantero en una chaqueta abierta o, si no, un chaleco
    /// suelto.
    vest: Option<Rgb>,
    /// La pechera de la camisa que se ve en una chaqueta abierta.
    shirt: Option<Rgb>,
    /// Un cuello alto (cuello duro de camisa o blusa de cuello alto) y una
    /// corbata.
    collar: Option<Rgb>,
    tie: Option<Rgb>,
    neckerchief: Option<Rgb>,
    /// Una faja, o un cinturón de cuero simple.
    faja: Option<Rgb>,
    shawl: Option<Rgb>,
    /// Botas altas hasta la rodilla, del color de los zapatos.
    tall_boots: bool,
    /// Botas de trabajo simples en lugar de zapatos o alpargatas.
    boots: bool,
    /// Una chaqueta de cuero gastada con cuello, solapas y bolsillos.
    leather: bool,
    /// Pantalón, bombacha o pollera.
    legs: Rgb,
    /// Bombacha holgada fruncida en el tobillo.
    bombacha: bool,
    skirt: bool,
    shoes: Rgb,
    head: Headwear,
    poncho: Option<Rgb>,
    moustache: Option<Rgb>,
    rastra: bool,
    buttons: Option<Rgb>,
}

// Paleta terrosa y apagada de 1919 (ver `odd/setting-1919.md`): polvo, ocre,
// azul desteñido, lana marrón, gris y negra, camisas de blanco roto. Color
// brillante solo en los detalles: pañuelos, una faja, los botones de bronce de
// la policía.
const SKIN: Rgb = [206, 162, 124];
const HAIR: Rgb = [58, 42, 32];
const OFF_WHITE: Rgb = [214, 206, 186];
const ALPARGATAS: Rgb = [190, 178, 150];
const BOOTS: Rgb = [52, 38, 30];
const PANUELO_RED: Rgb = [164, 44, 36];

/// Un aspecto masculino simple, para que los vecinos lo varíen: un paisano de
/// estilo vasco con camisa sin cuello, boina, bombacha, faja, alpargatas y un
/// pañuelo.
const PAISANO: Look = Look {
    skin: SKIN,
    hair: HAIR,
    coat: OFF_WHITE,
    jacket: false,
    vest: None,
    shirt: None,
    collar: None,
    tie: None,
    neckerchief: Some(PANUELO_RED),
    faja: Some([132, 44, 40]),
    shawl: None,
    tall_boots: false,
    boots: false,
    leather: false,
    legs: [50, 48, 50],
    bombacha: true,
    skirt: false,
    shoes: ALPARGATAS,
    head: Headwear::Boina([38, 42, 56]),
    poncho: None,
    moustache: None,
    rastra: false,
    buttons: None,
};

/// El cuero marrón gastado del protagonista.
pub const LEATHER: Rgb = [108, 72, 46];

/// El protagonista, un humilde empleado de ferretería de General Pico: una
/// camisa de trabajo simple, una chaqueta de cuero gastada, pantalones
/// desteñidos, botas usadas y un gacho. Ropa de trabajo de un hombre de pueblo,
/// no la de un gaucho.
const PROTAGONIST: Look = Look {
    skin: [216, 174, 136],
    hair: [70, 50, 36],
    coat: LEATHER,
    jacket: true,
    leather: true,
    vest: None,
    shirt: Some([190, 184, 164]),
    collar: None,
    tie: None,
    neckerchief: None,
    faja: None,
    legs: [98, 92, 82],
    bombacha: false,
    shoes: [62, 46, 34],
    boots: true,
    head: Headwear::Gacho {
        felt: [84, 72, 58],
        band: [46, 38, 30],
    },
    moustache: Some([70, 50, 36]),
    ..PAISANO
};

const TOWNSFOLK: [Look; TOWNSFOLK_LOOKS as usize] = [
    PAISANO,
    // Peón: chaleco sobre una camisa sin cuello, sombrero blando de ala ancha,
    // botas gastadas y un pañuelo blanco.
    Look {
        skin: [190, 142, 104],
        coat: [170, 158, 132],
        vest: Some([88, 66, 46]),
        legs: [96, 86, 70],
        shoes: BOOTS,
        head: Headwear::FeltHat([92, 76, 58]),
        neckerchief: Some([226, 222, 210]),
        faja: Some([64, 46, 34]),
        moustache: Some(HAIR),
        ..PAISANO
    },
    // Un peón alemán del Volga con un poncho contra el frío.
    Look {
        skin: [212, 172, 140],
        hair: [150, 120, 80],
        coat: [148, 132, 108],
        legs: [74, 68, 60],
        head: Headwear::FeltHat([66, 56, 46]),
        poncho: Some([124, 76, 52]),
        neckerchief: None,
        faja: None,
        shoes: BOOTS,
        ..PAISANO
    },
    // Mujeres: polleras hasta el tobillo, blusas de cuello alto y manga larga,
    // un chal, y un pañuelo en la cabeza o el pelo recogido.
    Look {
        skin: [212, 168, 132],
        coat: [116, 128, 142],
        collar: Some([116, 128, 142]),
        legs: [72, 56, 44],
        bombacha: false,
        skirt: true,
        shoes: [56, 42, 34],
        head: Headwear::Headscarf([196, 176, 128]),
        shawl: Some([102, 70, 54]),
        neckerchief: None,
        faja: None,
        ..PAISANO
    },
    Look {
        hair: [40, 30, 26],
        coat: OFF_WHITE,
        collar: Some(OFF_WHITE),
        legs: [44, 50, 62],
        bombacha: false,
        skirt: true,
        shoes: BOOTS,
        head: Headwear::Bun,
        shawl: Some([60, 58, 62]),
        neckerchief: None,
        faja: None,
        ..PAISANO
    },
];

/// Don Hidalgo, un estanciero mayor: una buena chaqueta marrón sobre un chaleco
/// oscuro, bombacha, botas altas, una rastra, un sombrero claro de ala ancha y
/// un bigote blanco.
const HIDALGO: Look = Look {
    skin: [212, 168, 132],
    hair: [226, 222, 214],
    coat: [96, 74, 54],
    jacket: true,
    vest: Some([46, 42, 44]),
    shirt: Some(OFF_WHITE),
    legs: [62, 58, 56],
    shoes: BOOTS,
    tall_boots: true,
    head: Headwear::FeltHat([188, 176, 144]),
    moustache: Some([236, 234, 228]),
    rastra: true,
    neckerchief: None,
    faja: None,
    ..PAISANO
};

/// El azul oscuro del uniforme de policía.
pub const POLICE_BLUE: Rgb = [38, 50, 86];

/// El comisario Farach: quepí, túnica azul oscuro con botones de bronce, un
/// cinturón de cuero, pantalones oscuros y botas.
const FARACH: Look = Look {
    skin: [196, 150, 112],
    hair: [36, 28, 24],
    coat: POLICE_BLUE,
    jacket: true,
    legs: [30, 32, 42],
    bombacha: false,
    shoes: [26, 24, 24],
    tall_boots: true,
    head: Headwear::Kepi {
        cloth: [32, 40, 64],
        visor: [20, 20, 22],
    },
    moustache: Some([36, 28, 24]),
    buttons: Some([204, 166, 70]),
    neckerchief: None,
    faja: Some([36, 28, 24]),
    ..PAISANO
};

/// Bairoletto: un joven delgado con camisa negra y pañuelo de cuello blanco,
/// pantalones oscuros, botas y un chambergo de fieltro claro; pelo castaño
/// claro y una barba corta (dibujada como bigote).
const BAIROLETTO: Look = Look {
    skin: [222, 184, 152],
    hair: [124, 92, 62],
    coat: [30, 28, 30],
    jacket: false,
    vest: None,
    shirt: None,
    legs: [52, 48, 46],
    bombacha: false,
    shoes: [40, 32, 26],
    boots: true,
    head: Headwear::FeltHat([176, 164, 136]),
    moustache: Some([124, 92, 62]),
    neckerchief: Some([236, 234, 226]),
    faja: None,
    ..PAISANO
};

/// `color` un tono más oscuro: costuras, puños, suelas y cuencas de los ojos.
pub(super) fn darker(color: Rgb) -> Rgb {
    color.map(|c| (c as f32 * 0.78) as u8)
}

fn look(kind: FigureKind) -> &'static Look {
    match kind {
        FigureKind::Protagonist => &PROTAGONIST,
        FigureKind::Townsfolk(n) => &TOWNSFOLK[(n % TOWNSFOLK_LOOKS) as usize],
        FigureKind::Hidalgo => &HIDALGO,
        FigureKind::Farach => &FARACH,
        FigureKind::Bairoletto => &BAIROLETTO,
    }
}

/// Todas las piezas de `kind`, de pie. Determinista. Los sombreros, los ponchos
/// y las polleras cuelgan de sus propias articulaciones para poder balancearse
/// ([`hang`]).
pub fn figure_parts(kind: FigureKind) -> Vec<PartSpec> {
    let l = look(kind);
    let mut parts = laid_parts(kind);
    for part in &mut parts {
        part.finish = finish_of(l, part);
    }
    hang(parts)
}

/// Todas las piezas de `kind`, de pie, cada una en la articulación del cuerpo
/// sobre la que está dispuesta.
///
/// Se mantiene como una tabla, una pieza por línea.
#[rustfmt::skip]
fn laid_parts(kind: FigureKind) -> Vec<PartSpec> {
    let l = look(kind);
    let mut parts = Vec::with_capacity(32);
    let mut add = |piece, joint, shape, color, center: [f32; 3], size: [f32; 3]| {
        parts.push(PartSpec {
            piece,
            joint,
            shape,
            color,
            center: Vec3::from(center),
            size: Vec3::from(size),
            finish: Finish::Cloth,
        });
    };
    use Joint::*;
    use Piece::*;

    // Piernas, desde la cadera hasta la suela en el origen de los pies. Los
    // segmentos se superponen más allá de cada articulación para que no se abra
    // ningún hueco al girar. Los pantalones son más anchos que una pierna
    // desnuda; una bombacha, más holgada todavía.
    let (hips, thigh, shin, shin_shape) = if l.bombacha {
        ([0.27, 0.22, 0.4], [0.22, 0.5, 0.22], [0.19, 0.45, 0.19], Shape::BaggyShin)
    } else {
        ([0.24, 0.2, 0.36], [0.17, 0.5, 0.17], [0.14, 0.46, 0.14], Shape::Shin)
    };
    add(Hips, Pelvis, Shape::Hips, l.legs, [0.0, -0.01, 0.0], hips);
    for s in Side::BOTH {
        add(UpperLeg, Hip(s), Shape::Thigh, l.legs, [0.0, -0.2, 0.0], thigh);
        if l.tall_boots {
            add(LowerLeg, Knee(s), Shape::Shin, l.shoes, [0.0, -0.2, 0.0], [0.15, 0.46, 0.15]);
        } else {
            add(LowerLeg, Knee(s), shin_shape, l.legs, [0.0, -0.2, 0.0], shin);
        }
        if l.boots {
            add(Foot, Knee(s), Shape::Boot, l.shoes, [0.045, -0.43, 0.0], [0.27, 0.14, 0.12]);
            add(Sole, Knee(s), Shape::Band, darker(l.shoes), [0.04, -0.49, 0.0], [0.29, 0.02, 0.12]);
        } else {
            add(Foot, Knee(s), Shape::Shoe, l.shoes, [0.05, -0.45, 0.0], [0.27, 0.1, 0.115]);
            add(Sole, Knee(s), Shape::Band, darker(l.shoes), [0.045, -0.49, 0.0], [0.28, 0.02, 0.115]);
        }
    }
    if l.skirt {
        add(Skirt, Pelvis, Shape::Skirt, l.legs, [0.0, -0.36, 0.0], [0.58, 0.88, 0.62]);
    }

    // El torso, los brazos y lo que se lleva encima. Una chaqueta o una túnica
    // llega por encima de las caderas y se acampana en el ruedo.
    if l.leather {
        // Abierta por delante sobre la camisa: un cuello vuelto, solapas y dos
        // bolsillos con tapa, todo del mismo cuero gastado, un tono más oscuro.
        let edge = [l.coat[0] - 18, l.coat[1] - 14, l.coat[2] - 10];
        add(Torso, Waist, Shape::Jacket, l.coat, [0.0, 0.2, 0.0], [0.27, 0.6, 0.43]);
        add(Seam, Waist, Shape::Band, edge, [0.0, -0.075, 0.0], [0.27, 0.02, 0.43]);
        if let Some(shirt) = l.shirt {
            add(Shirt, Waist, Shape::Panel, shirt, [0.126, 0.3, 0.0], [0.02, 0.34, 0.1]);
        }
        add(Lapel, Waist, Shape::Disc, edge, [-0.005, 0.47, 0.0], [0.17, 0.05, 0.25]);
        for z in [-0.06, 0.06] {
            add(Lapel, Waist, Shape::Panel, edge, [0.133, 0.36, z], [0.02, 0.2, 0.06]);
            add(Pocket, Waist, Shape::Panel, edge, [0.128, 0.04, 2.0 * z], [0.02, 0.07, 0.09]);
        }
    } else if l.jacket {
        add(Torso, Waist, Shape::Coat, l.coat, [0.0, 0.2, 0.0], [0.27, 0.6, 0.43]);
    } else {
        add(Torso, Waist, Shape::Torso, l.coat, [0.0, 0.24, 0.0], [0.25, 0.5, 0.42]);
    }
    for s in Side::BOTH {
        add(UpperArm, Shoulder(s), Shape::UpperArm, l.coat, [0.0, -0.14, 0.0], [0.11, 0.33, 0.11]);
        add(Forearm, Elbow(s), Shape::Forearm, l.coat, [0.0, -0.13, 0.0], [0.095, 0.29, 0.095]);
        add(Hand, Elbow(s), Shape::Hand, l.skin, [0.0, -0.31, 0.0], [0.085, 0.1, 0.05]);
        add(Thumb, Elbow(s), Shape::Bump, l.skin, [0.03, -0.295, -0.02 * s.z()], [0.03, 0.06, 0.03]);
        add(Cuff, Elbow(s), Shape::Band, darker(l.coat), [0.0, -0.255, 0.0], [0.1, 0.025, 0.1]);
    }
    match (l.vest, l.jacket) {
        // Un chaleco y una pechera de camisa que se ven en la chaqueta abierta.
        (Some(vest), true) => {
            add(Vest, Waist, Shape::Panel, vest, [0.125, 0.2, 0.0], [0.03, 0.42, 0.2]);
            if let Some(shirt) = l.shirt {
                add(Shirt, Waist, Shape::Panel, shirt, [0.137, 0.34, 0.0], [0.02, 0.2, 0.09]);
            }
        }
        // Un chaleco sobre la camisa, con sus mangas a la vista.
        (Some(vest), false) => {
            add(Vest, Waist, Shape::Torso, vest, [0.0, 0.2, 0.0], [0.27, 0.42, 0.44]);
        }
        (None, _) => {}
    }
    if let Some(brass) = l.buttons {
        for y in [0.1, 0.2, 0.3] {
            add(Button, Waist, Shape::Bump, brass, [0.135, y, 0.0], [0.02, 0.025, 0.025]);
        }
    }
    if let Some(tie) = l.tie {
        add(Tie, Waist, Shape::Panel, tie, [0.15, 0.36, 0.0], [0.015, 0.17, 0.035]);
    }
    if let Some(faja) = l.faja {
        add(Faja, Waist, Shape::Disc, faja, [0.0, 0.03, 0.0], [0.27, 0.08, 0.43]);
    }
    if l.rastra {
        add(Belt, Waist, Shape::Disc, [42, 32, 26], [0.0, 0.02, 0.0], [0.28, 0.07, 0.44]);
        add(Buckle, Waist, Shape::Lump, [196, 198, 196], [0.135, 0.02, 0.0], [0.025, 0.08, 0.13]);
    }
    if let Some(poncho) = l.poncho {
        add(Poncho, Waist, Shape::Poncho, poncho, [0.0, 0.15, 0.0], [0.6, 0.72, 0.66]);
    }
    if let Some(shawl) = l.shawl {
        add(Shawl, Waist, Shape::Poncho, shawl, [0.0, 0.3, 0.0], [0.36, 0.42, 0.52]);
    }

    // La cabeza: cuello, cráneo, mandíbula, la nariz como marca del frente, el
    // pelo y el sombrero.
    let (neck, skin) = (Joint::Neck, l.skin);
    add(Piece::Neck, neck, Shape::Neck, skin, [0.0, 0.04, 0.0], [0.1, 0.12, 0.1]);
    add(Head, neck, Shape::Head, skin, [0.0, 0.17, 0.0], [0.2, 0.22, 0.18]);
    add(Jaw, neck, Shape::Lump, skin, [0.025, 0.105, 0.0], [0.15, 0.1, 0.14]);
    add(Nose, neck, Shape::Bump, skin, [0.1, 0.16, 0.0], [0.04, 0.05, 0.035]);
    add(Brow, neck, Shape::Bump, skin, [0.085, 0.2, 0.0], [0.03, 0.03, 0.15]);
    for z in [-0.045, 0.045] {
        add(Eye, neck, Shape::Bump, darker(skin), [0.082, 0.18, z], [0.02, 0.025, 0.04]);
    }
    for z in [-0.092, 0.092] {
        add(Ear, neck, Shape::Bump, skin, [-0.005, 0.165, z], [0.035, 0.06, 0.025]);
    }
    add(Hair, neck, Shape::Lump, l.hair, [-0.02, 0.21, 0.0], [0.2, 0.16, 0.19]);
    if let Some(collar) = l.collar {
        add(Collar, neck, Shape::Disc, collar, [0.0, 0.05, 0.0], [0.12, 0.06, 0.12]);
    }
    if let Some(scarf) = l.neckerchief {
        add(Neckerchief, neck, Shape::Lump, scarf, [0.045, 0.02, 0.0], [0.08, 0.06, 0.13]);
    }
    if let Some(moustache) = l.moustache {
        add(Moustache, neck, Shape::Lump, moustache, [0.095, 0.125, 0.0], [0.03, 0.025, 0.09]);
    }
    match l.head {
        Headwear::Bowler { felt, band } => {
            add(Bowler, neck, Shape::Disc, felt, [0.0, 0.255, 0.0], [0.27, 0.015, 0.25]);
            add(Bowler, neck, Shape::Lump, felt, [0.0, 0.28, 0.0], [0.21, 0.16, 0.2]);
            add(Bowler, neck, Shape::Disc, band, [0.0, 0.272, 0.0], [0.212, 0.025, 0.202]);
        }
        Headwear::Bun => {
            add(Bun, neck, Shape::Lump, l.hair, [-0.09, 0.25, 0.0], [0.08, 0.08, 0.09]);
        }
        Headwear::Gacho { felt, band } => {
            add(Gacho, neck, Shape::Brim, felt, [0.0, 0.25, 0.0], [0.46, 0.05, 0.44]);
            add(Gacho, neck, Shape::Crown, felt, [0.0, 0.31, 0.0], [0.21, 0.12, 0.2]);
            add(Gacho, neck, Shape::Disc, band, [0.0, 0.275, 0.0], [0.214, 0.03, 0.204]);
        }
        Headwear::FeltHat(felt) => {
            add(Hat, neck, Shape::Disc, felt, [0.0, 0.255, 0.0], [0.44, 0.015, 0.42]);
            add(Hat, neck, Shape::Crown, felt, [0.0, 0.3, 0.0], [0.21, 0.1, 0.2]);
        }
        Headwear::Boina(cloth) => {
            add(Boina, neck, Shape::Lump, cloth, [-0.01, 0.275, 0.0], [0.25, 0.06, 0.24]);
        }
        Headwear::Kepi { cloth, visor } => {
            add(Kepi, neck, Shape::Crown, cloth, [0.0, 0.3, 0.0], [0.19, 0.14, 0.18]);
            add(Kepi, neck, Shape::Disc, visor, [0.1, 0.245, 0.0], [0.11, 0.015, 0.15]);
            add(Seam, neck, Shape::Band, visor, [0.0, 0.335, 0.0], [0.188, 0.012, 0.178]);
        }
        Headwear::Headscarf(cloth) => {
            add(Headscarf, neck, Shape::Head, cloth, [-0.005, 0.19, 0.0], [0.215, 0.2, 0.2]);
            add(Headscarf, neck, Shape::Lump, cloth, [-0.11, 0.1, 0.0], [0.08, 0.1, 0.08]);
        }
    }
    parts
}

/// `parts` con los sombreros, los ponchos y las polleras pasados a sus propias
/// articulaciones (colgando de la articulación sobre la que estaban
/// dispuestos), en el mismo lugar de reposo.
fn hang(mut parts: Vec<PartSpec>) -> Vec<PartSpec> {
    use Joint::*;
    use Piece::*;
    for part in &mut parts {
        let joint = match part.piece {
            Hat | Gacho | Bowler | Boina | Kepi => Some(Brim),
            Seam if part.joint == Joint::Neck => Some(Brim),
            Poncho | Shawl => Some(Drape),
            Skirt => Some(Hem),
            _ => None,
        };
        if let Some(joint) = joint {
            part.center -= joint.rest();
            part.joint = joint;
        }
    }
    parts
}

/// De qué está hecha la pieza `part` del aspecto `l`.
fn finish_of(l: &Look, part: &PartSpec) -> Finish {
    use Piece::*;
    match part.piece {
        Head => Finish::Face,
        Jaw | Nose | Neck | Hand | Thumb | Ear | Brow | Eye => Finish::Skin,
        Hair | Moustache | Bun => Finish::Hair,
        Button => Finish::Brass,
        Buckle => Finish::Silver,
        Hat | Gacho | Bowler | Kepi | Boina => Finish::Felt,
        Poncho | Shawl => Finish::Wool,
        Foot | Sole if part.color == ALPARGATAS || part.color == darker(ALPARGATAS) => {
            Finish::Cloth
        }
        Belt | Foot | Sole => Finish::Leather,
        LowerLeg if l.tall_boots => Finish::Leather,
        Torso | UpperArm | Forearm | Lapel | Pocket | Seam | Cuff if l.leather => Finish::Leather,
        _ => Finish::Cloth,
    }
}

/// La altura de la parte superior de la cabeza o el sombrero de `kind`, de pie.
pub fn figure_height(kind: FigureKind) -> f32 {
    figure_parts(kind)
        .iter()
        .map(|p| p.joint.rest_position().y + p.center.y + p.size.y / 2.0)
        .fold(0.0, f32::max)
}

/// El giro del cuerpo para un rumbo en el suelo `heading`: su frente (+X)
/// apunta según el rumbo sobre el suelo (el norte del suelo es -Z en Bevy).
pub fn body_rotation(heading: Heading) -> Quat {
    Quat::from_rotation_y(heading.radians())
}

/// Las meshes unitarias compartidas; los materiales vienen de la [`Palette`].
#[derive(Resource, Default)]
pub struct FigureKit {
    meshes: HashMap<Shape, Handle<Mesh>>,
}

impl FigureKit {
    pub fn new(meshes: &mut Assets<Mesh>) -> Self {
        Self {
            meshes: Shape::ALL.map(|s| (s, meshes.add(s.mesh()))).into(),
        }
    }
}

/// Los almacenes de assets y la paleta con los que se dibuja una figura.
pub(super) type Paints<'a> = (
    &'a mut Palette,
    &'a mut Assets<StandardMaterial>,
    &'a mut Assets<Image>,
);

/// El cuerpo que gira de una figura, hijo de la entidad del personaje.
#[derive(Component, Debug, Clone)]
pub struct FigureBody {
    pub kind: FigureKind,
    /// Qué tan avanzado está el ciclo del andar (o de la respiración), en
    /// ciclos (`0..1`).
    pub phase: f32,
    /// Segundos desde que se dibujó (con vuelta en
    /// [`pampa_core::MOTION_CLOCK_PERIOD`]), y su propia semilla, para mirar a
    /// su alrededor.
    pub clock: f32,
    pub seed: u32,
    /// Cuánto está girada la cabeza hacia un costado en este momento,
    /// acercándose suavemente a la mirada; un modelo de personaje sobre ella
    /// gira la cabeza igual (ver [`rig`](super::rig)).
    pub glance: f32,
    /// Las entidades de articulación en el orden de [`Joint::ALL`].
    joints: Vec<Entity>,
}

/// Una entidad de articulación de una figura.
#[derive(Component, Debug, Clone, Copy)]
pub struct FigureJoint(pub Joint);

/// Una mesh de pieza de una figura.
#[derive(Component, Debug, Clone, Copy)]
pub struct FigurePart(pub Piece);

/// Crea el cuerpo, las articulaciones y las piezas de `kind` bajo `parent`;
/// devuelve el cuerpo.
pub(super) fn spawn_figure(
    commands: &mut Commands,
    kit: &mut FigureKit,
    (palette, materials, images): Paints,
    parent: Entity,
    kind: FigureKind,
    phase: f32,
) -> Entity {
    let body = commands
        .spawn((
            Name::new("Figure"),
            Transform::default(),
            Visibility::default(),
            ChildOf(parent),
        ))
        .id();
    let mut joints: Vec<Entity> = Vec::with_capacity(Joint::ALL.len());
    for joint in Joint::ALL {
        let holder = joint.parent().map_or(body, |p| joints[p.index()]);
        joints.push(
            commands
                .spawn((
                    FigureJoint(joint),
                    Transform::from_translation(joint.rest()),
                    Visibility::default(),
                    ChildOf(holder),
                ))
                .id(),
        );
    }
    for part in figure_parts(kind) {
        let Some(mesh) = kit.meshes.get(&part.shape).cloned() else {
            continue;
        };
        commands.spawn((
            FigurePart(part.piece),
            Mesh3d(mesh),
            MeshMaterial3d(palette.material(part.color, part.finish, materials, images)),
            Transform::from_translation(part.center).with_scale(part.size),
            ChildOf(joints[part.joint.index()]),
        ));
    }
    commands.entity(body).insert(FigureBody {
        kind,
        phase,
        clock: 0.0,
        seed: parent.index_u32(),
        glance: 0.0,
        joints,
    });
    body
}

/// Gira, hace rebotar y pone en pose cada figura a partir de la pose de su
/// personaje, con su andar al ritmo de la velocidad.
pub(super) fn update_figures(
    time: Res<Time>,
    poses: Query<&super::VisualPose>,
    mut bodies: Query<(&ChildOf, &mut FigureBody, &mut Transform), Without<FigureJoint>>,
    mut joints: Query<(&FigureJoint, &mut Transform), Without<FigureBody>>,
) {
    let dt = time.delta_secs();
    for (child_of, mut body, mut transform) in &mut bodies {
        // Con vuelta, sin saltos: un juego que queda corriendo mantiene un
        // reloj preciso.
        body.clock = pampa_core::tick_clock(body.clock, dt);
        let Ok(visual) = poses.get(child_of.parent()) else {
            continue;
        };
        let idle = visual.animation == AnimationState::Idle;
        body.phase = pose::advance(body.phase, visual.animation, visual.speed, dt);
        let angles = pose::pose(visual.animation, body.phase);
        transform.rotation = body_rotation(visual.heading);
        transform.translation.y = angles.bob;
        // La gente parada mira a su alrededor de vez en cuando; el giro entra y
        // sale de forma gradual, así que al empezar a caminar vuelve a mirar al
        // frente con suavidad.
        let target = if idle {
            pose::glance(body.seed, body.clock)
        } else {
            0.0
        };
        body.glance += (target - body.glance) * (1.0 - (-8.0 * dt).exp());
        let glance = body.glance;
        for &entity in &body.joints {
            let Ok((joint, mut transform)) = joints.get_mut(entity) else {
                continue;
            };
            let breath = if joint.0 == Joint::Waist {
                angles.breath
            } else {
                0.0
            };
            transform.translation = joint.0.rest() + Vec3::Y * breath;
            transform.rotation = joint.0.rotation(&angles, glance);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world_from_ground;

    fn has(kind: FigureKind, piece: Piece) -> bool {
        figure_parts(kind).iter().any(|p| p.piece == piece)
    }

    fn color_of(kind: FigureKind, piece: Piece) -> Option<Rgb> {
        figure_parts(kind)
            .iter()
            .find(|p| p.piece == piece)
            .map(|p| p.color)
    }

    #[test]
    fn every_look_builds_about_as_tall_as_a_person_and_the_same_each_time() {
        for kind in FigureKind::all() {
            let parts = figure_parts(kind);
            assert!(parts.len() >= 18, "{kind:?}: {} parts", parts.len());
            assert_eq!(parts, figure_parts(kind), "{kind:?} is deterministic");
            let height = figure_height(kind);
            assert!((1.6..=1.9).contains(&height), "{kind:?}: {height} m");
            // Los pies en el suelo.
            let sole = parts
                .iter()
                .map(|p| p.joint.rest_position().y + p.center.y - p.size.y / 2.0)
                .fold(f32::MAX, f32::min);
            assert!(sole.abs() < 1e-4, "{kind:?}: sole at {sole}");
        }
        // La parte baja del aspecto elige la ropa: 7 se dibuja como 2 (`models
        // off`); el resto se guarda para los colores del modelo.
        assert_eq!(FigureKind::townsfolk(7), FigureKind::Townsfolk(7));
        assert_eq!(
            figure_parts(FigureKind::townsfolk(7)),
            figure_parts(FigureKind::Townsfolk(2))
        );
        let looks: Vec<_> = (0..TOWNSFOLK_LOOKS)
            .map(|n| figure_parts(FigureKind::Townsfolk(n)))
            .collect();
        for (i, a) in looks.iter().enumerate() {
            for b in &looks[i + 1..] {
                assert_ne!(a, b, "townsfolk looks differ");
            }
        }
    }

    #[test]
    fn the_protagonist_wears_a_humble_townsmans_clothes() {
        let me = FigureKind::Protagonist;
        let shape_of = |piece| {
            figure_parts(me)
                .iter()
                .find(|p| p.piece == piece)
                .map(|p| p.shape)
        };
        // Un gacho, una chaqueta de cuero gastada con solapas, una camisa de
        // trabajo, botas.
        assert!(has(me, Piece::Gacho) && has(me, Piece::Shirt));
        assert_eq!(shape_of(Piece::Torso), Some(Shape::Jacket));
        assert_eq!(color_of(me, Piece::Torso), Some(LEATHER));
        assert!(has(me, Piece::Lapel) && has(me, Piece::Pocket));
        assert_eq!(shape_of(Piece::Foot), Some(Shape::Boot));
        // No es un caballero del pueblo: sin bombín, corbata ni cuello duro.
        assert!(!has(me, Piece::Bowler) && !has(me, Piece::Tie) && !has(me, Piece::Collar));
        // No es un gaucho: sin poncho, bombacha ni faja.
        assert!(!has(me, Piece::Poncho) && !has(me, Piece::Faja) && !has(me, Piece::Boina));
        assert!(!has(me, Piece::Skirt) && !has(me, Piece::Neckerchief));
        // Nadie más lleva un gacho ni su chaqueta.
        for kind in FigureKind::all().filter(|k| *k != me) {
            assert!(
                !has(kind, Piece::Gacho) && !has(kind, Piece::Lapel),
                "{kind:?}"
            );
        }
        // Pantalones, no bombacha: tan delgados como los del comisario.
        let thigh = |k| {
            figure_parts(k)
                .iter()
                .find(|p| p.piece == Piece::UpperLeg)
                .map(|p| p.size.x)
        };
        assert_eq!(thigh(me), thigh(FigureKind::Farach));
        assert!(thigh(me) < thigh(FigureKind::Townsfolk(0)));
        // Su chaqueta es de un color que nadie del pueblo lleva.
        let jacket = color_of(me, Piece::Torso);
        for n in 0..TOWNSFOLK_LOOKS {
            assert_ne!(jacket, color_of(FigureKind::Townsfolk(n), Piece::Torso));
        }
        // Los vecinos llevan la ropa de campo.
        assert!((0..TOWNSFOLK_LOOKS).any(|n| has(FigureKind::Townsfolk(n), Piece::Poncho)));
        assert!((0..TOWNSFOLK_LOOKS).any(|n| has(FigureKind::Townsfolk(n), Piece::Boina)));
        assert!((0..TOWNSFOLK_LOOKS).any(|n| has(FigureKind::Townsfolk(n), Piece::Skirt)));
    }

    #[test]
    fn women_wear_ankle_length_skirts_and_high_necked_blouses() {
        let women: Vec<_> = (0..TOWNSFOLK_LOOKS)
            .map(FigureKind::Townsfolk)
            .filter(|k| has(*k, Piece::Skirt))
            .collect();
        assert_eq!(women.len(), 2);
        for kind in women {
            let skirt = figure_parts(kind)
                .into_iter()
                .find(|p| p.piece == Piece::Skirt)
                .unwrap();
            let hem = skirt.joint.rest_position().y + skirt.center.y - skirt.size.y / 2.0;
            // Hasta el tobillo, justo por encima de la parte superior de los
            // zapatos (0.1 m).
            assert!((0.05..=0.15).contains(&hem), "{kind:?}: hem at {hem} m");
            assert!(
                has(kind, Piece::Collar) && has(kind, Piece::Shawl),
                "{kind:?}"
            );
            assert!(
                has(kind, Piece::Headscarf) || has(kind, Piece::Bun),
                "{kind:?}"
            );
        }
        // Los peones llevan la faja y el pañuelo.
        assert!(has(FigureKind::Townsfolk(0), Piece::Faja));
        assert!(has(FigureKind::Townsfolk(1), Piece::Neckerchief));
    }

    #[test]
    fn the_errand_people_have_their_own_looks() {
        let hidalgo = FigureKind::Hidalgo;
        assert!(has(hidalgo, Piece::Moustache) && has(hidalgo, Piece::Buckle));
        assert!(has(hidalgo, Piece::Hat) && has(hidalgo, Piece::Vest));
        assert_eq!(color_of(hidalgo, Piece::Moustache), Some([236, 234, 228]));
        let farach = FigureKind::Farach;
        assert!(has(farach, Piece::Kepi) && has(farach, Piece::Button));
        assert_eq!(color_of(farach, Piece::Torso), Some(POLICE_BLUE));
        let others: Vec<_> = FigureKind::all()
            .filter(|k| *k != hidalgo && *k != farach)
            .collect();
        for kind in others {
            assert_ne!(figure_parts(kind), figure_parts(hidalgo));
            assert_ne!(figure_parts(kind), figure_parts(farach));
            assert!(!has(kind, Piece::Kepi) && !has(kind, Piece::Buckle));
        }
    }

    #[test]
    fn the_body_faces_its_ground_heading() {
        let nose = figure_parts(FigureKind::Protagonist)
            .into_iter()
            .find(|p| p.piece == Piece::Nose)
            .unwrap();
        let point = nose.joint.rest_position() + nose.center;
        assert!(
            point.x > 0.08 && point.z.abs() < 1e-6,
            "the nose is in front"
        );
        for degrees in [0.0_f32, 45.0, 90.0, 180.0, 250.0, 300.0] {
            let heading = Heading::from_radians(degrees.to_radians());
            let turned = body_rotation(heading) * point;
            let along = Vec3::new(turned.x, 0.0, turned.z).normalize();
            let want = world_from_ground(heading.forward(), 0.0);
            assert!(along.distance(want) < 1e-4, "{degrees}°: {along} vs {want}");
        }
    }

    fn finish_of_piece(kind: FigureKind, piece: Piece) -> Option<Finish> {
        figure_parts(kind)
            .iter()
            .find(|p| p.piece == piece)
            .map(|p| p.finish)
    }

    #[test]
    fn each_part_has_the_finish_of_its_material() {
        use FigureKind::*;
        assert_eq!(
            finish_of_piece(Protagonist, Piece::Torso),
            Some(Finish::Leather)
        );
        assert_eq!(
            finish_of_piece(Protagonist, Piece::Foot),
            Some(Finish::Leather)
        );
        assert_eq!(
            finish_of_piece(Protagonist, Piece::Gacho),
            Some(Finish::Felt)
        );
        assert_eq!(
            finish_of_piece(Protagonist, Piece::Head),
            Some(Finish::Face)
        );
        assert_eq!(
            finish_of_piece(Protagonist, Piece::Hand),
            Some(Finish::Skin)
        );
        assert_eq!(finish_of_piece(Farach, Piece::Button), Some(Finish::Brass));
        assert_eq!(finish_of_piece(Farach, Piece::Torso), Some(Finish::Cloth));
        assert_eq!(
            finish_of_piece(Hidalgo, Piece::Buckle),
            Some(Finish::Silver)
        );
        assert_eq!(
            finish_of_piece(Townsfolk(2), Piece::Poncho),
            Some(Finish::Wool)
        );
        assert_eq!(
            finish_of_piece(Townsfolk(0), Piece::UpperLeg),
            Some(Finish::Cloth)
        );
        assert_eq!(
            finish_of_piece(Townsfolk(0), Piece::Foot),
            Some(Finish::Cloth),
            "alpargatas"
        );
        // Cada cabeza lleva la textura de la cara.
        for kind in FigureKind::all() {
            assert_eq!(
                finish_of_piece(kind, Piece::Head),
                Some(Finish::Face),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn re_parented_parts_keep_their_rest_place_and_hang_from_their_old_joint() {
        let mut moved = 0;
        for kind in FigureKind::all() {
            let laid = laid_parts(kind);
            let hung = figure_parts(kind);
            assert_eq!(laid.len(), hung.len(), "{kind:?}");
            for (before, after) in laid.iter().zip(&hung) {
                assert_eq!(before.piece, after.piece);
                let at = |p: &PartSpec| p.joint.rest_position() + p.center;
                assert!(
                    at(before).distance(at(after)) < 1e-5,
                    "{kind:?} {:?}",
                    after.piece
                );
                if before.joint != after.joint {
                    moved += 1;
                    assert_eq!(
                        after.joint.parent(),
                        Some(before.joint),
                        "{kind:?} {:?}",
                        after.piece
                    );
                }
            }
        }
        assert!(moved > 10, "{moved}");
    }

    #[test]
    fn hats_ponchos_and_skirts_hang_from_joints_that_sway() {
        let on = |kind, piece| {
            figure_parts(kind)
                .iter()
                .filter(|p| p.piece == piece)
                .map(|p| p.joint)
                .collect::<Vec<_>>()
        };
        assert!(
            on(FigureKind::Protagonist, Piece::Gacho)
                .iter()
                .all(|j| *j == Joint::Brim)
        );
        assert!(
            on(FigureKind::Farach, Piece::Kepi)
                .iter()
                .all(|j| *j == Joint::Brim)
        );
        assert_eq!(on(FigureKind::Townsfolk(2), Piece::Poncho), [Joint::Drape]);
        assert_eq!(on(FigureKind::Townsfolk(3), Piece::Skirt), [Joint::Hem]);
        // Cada articulación nueva cuelga de aquella de la que colgaban antes
        // sus piezas.
        assert_eq!(Joint::Brim.parent(), Some(Joint::Neck));
        assert_eq!(Joint::Drape.parent(), Some(Joint::Waist));
        assert_eq!(Joint::Hem.parent(), Some(Joint::Pelvis));
        // El balanceo las gira; quietas, se quedan en su lugar en reposo.
        let walk = pose::pose(AnimationState::Walk, 0.4);
        assert_ne!(Joint::Brim.rotation(&walk, 0.0), Quat::IDENTITY);
        assert_ne!(Joint::Drape.rotation(&walk, 0.0), Quat::IDENTITY);
        assert_eq!(
            Joint::Hem.rotation(&JointAngles::default(), 0.0),
            Quat::IDENTITY
        );
        // Una mirada gira la cabeza alrededor de la vertical.
        let turned = Joint::Neck.rotation(&JointAngles::default(), 0.4) * Vec3::X;
        assert!(turned.z.abs() > 0.3 && turned.y.abs() < 1e-5);
    }

    /// Las posiciones, las normales y los tonos de los vertices de la mesh
    /// unitaria de `shape`.
    fn shaded(shape: Shape) -> Vec<(Vec3, Vec3, f32)> {
        use bevy::mesh::VertexAttributeValues as V;
        let mesh = shape.mesh();
        let p = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap();
        let n = mesh
            .attribute(Mesh::ATTRIBUTE_NORMAL)
            .unwrap()
            .as_float3()
            .unwrap();
        let Some(V::Float32x4(c)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR) else {
            panic!("{shape:?} has shades");
        };
        p.iter()
            .zip(n)
            .zip(c)
            .map(|((p, n), c)| (Vec3::from(*p), Vec3::from(*n), c[0]))
            .collect()
    }

    fn mean(values: impl Iterator<Item = f32>) -> f32 {
        let (sum, n) = values.fold((0.0, 0), |(s, n), v| (s + v, n + 1));
        assert!(n > 0, "no samples");
        sum / n as f32
    }

    #[test]
    fn ambient_occlusion_darkens_armpits_crotch_jaw_and_brim_undersides() {
        let torso = shaded(Shape::Torso);
        let armpit = mean(
            torso
                .iter()
                .filter(|(p, _, _)| p.z.abs() > 0.44 && (0.15..0.3).contains(&p.y))
                .map(|v| v.2),
        );
        let chest = mean(
            torso
                .iter()
                .filter(|(p, _, _)| p.x > 0.4 && (0.0..0.3).contains(&p.y))
                .map(|v| v.2),
        );
        assert!(armpit < chest * 0.85, "armpit {armpit} vs chest {chest}");
        let hips = shaded(Shape::Hips);
        let crotch = mean(hips.iter().filter(|(p, _, _)| p.y < -0.4).map(|v| v.2));
        let seat = mean(hips.iter().filter(|(p, _, _)| p.y > 0.0).map(|v| v.2));
        assert!(crotch < seat * 0.85, "{crotch} vs {seat}");
        let head = shaded(Shape::Head);
        let jaw = mean(
            head.iter()
                .filter(|(p, n, _)| n.y < -0.6 && p.y < -0.3)
                .map(|v| v.2),
        );
        let brow = mean(head.iter().filter(|(p, _, _)| p.y > 0.2).map(|v| v.2));
        assert!(jaw < brow * 0.85, "{jaw} vs {brow}");
        let brim = shaded(Shape::Disc);
        let under = mean(brim.iter().filter(|(_, n, _)| n.y < -0.9).map(|v| v.2));
        let over = mean(brim.iter().filter(|(_, n, _)| n.y > 0.9).map(|v| v.2));
        assert!(under < over * 0.7, "{under} vs {over}");
    }

    #[test]
    fn lofts_wrap_their_textures_with_the_seam_behind() {
        use bevy::mesh::VertexAttributeValues as V;
        let mesh = Shape::Thigh.mesh();
        let p = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap();
        let Some(V::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else {
            panic!("uvs");
        };
        for (p, uv) in p.iter().zip(uv) {
            assert!((0.0..=1.0).contains(&uv[0]) && (0.0..=1.0).contains(&uv[1]));
            // El frente es el centro de la textura; sus bordes se juntan atrás.
            if p[0] > 0.1 && p[2].abs() < 1e-4 {
                assert!((uv[0] - 0.5).abs() < 1e-4, "{uv:?}");
            }
            if uv[0] == 0.0 || uv[0] == 1.0 {
                assert!(p[0] < 0.0 && p[2].abs() < 1e-4, "{p:?}");
            }
        }
    }

    fn triangles(shape: Shape) -> usize {
        shape.mesh().triangles().map(|t| t.count()).unwrap_or(0)
    }

    #[test]
    fn a_figure_has_ps1_to_ps2_era_detail() {
        for kind in FigureKind::all() {
            let total: usize = figure_parts(kind).iter().map(|p| triangles(p.shape)).sum();
            println!("{kind:?}: {total} triangles");
            assert!((3500..=5500).contains(&total), "{kind:?}: {total}");
        }
    }

    #[test]
    fn heads_and_limbs_are_smooth_rounds_not_boxes() {
        use bevy::mesh::VertexAttributeValues;
        let rounded = [
            Shape::Head,
            Shape::Thigh,
            Shape::Shin,
            Shape::UpperArm,
            Shape::Forearm,
            Shape::Torso,
            Shape::Hand,
        ];
        for shape in rounded {
            let mesh = shape.mesh();
            let Some(VertexAttributeValues::Float32x3(normals)) =
                mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
            else {
                panic!("{shape:?} has normals");
            };
            let slanted = normals
                .iter()
                .filter(|n| n.iter().filter(|c| c.abs() > 0.2).count() >= 2)
                .count();
            assert!(
                slanted * 3 > normals.len(),
                "{shape:?}: mostly axis-aligned normals"
            );
            // Suavizada: las caras comparten vertices, así que hay menos de
            // tres vertices por triángulo.
            assert!(
                mesh.count_vertices() < triangles(shape) * 2,
                "{shape:?} is faceted"
            );
            // Cabe en su caja unitaria, así que el tamaño de una pieza es su
            // caja envolvente.
            let Some(VertexAttributeValues::Float32x3(points)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("{shape:?} has positions");
            };
            assert!(points.iter().flatten().all(|c| c.abs() <= 0.5 + 1e-4));
        }
        // Las alas conservan su arista viva; el poncho, sus pliegues.
        assert!(
            Shape::Poncho.mesh().indices().is_none(),
            "unshared vertices"
        );
    }
}
