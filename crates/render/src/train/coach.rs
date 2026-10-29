//! El coche de pasajeros: un coche de madera de fabricación británica en
//! granate con filetes color crema, un techo abovedado con linterna,
//! plataformas en los extremos con portezuelas y estribos, y un interior hueco
//! de época dispuesto por [`CoachBody`].
//!
//! Por fuera: tablas granate con paneles, ventanas vidriadas en marcos
//! barnizados, una línea de cintura y un filete de alero color crema, el techo
//! de lona pintada, estribos de chapa antideslizante y pasamanos de bronce. Por
//! dentro: revestimiento barnizado con molduras, piso de tablones y una
//! alfombra de pasillo, un cielorraso forrado con costillas y faroles de
//! aceite, bancos tapizados con los respaldos enfrentados en un compartimento
//! junto a cada ventana, portaequipajes de bronce sobre las ventanas y puertas
//! de los extremos forradas.

use std::f32::consts::PI;

use bevy::prelude::*;

use pampa_core::CoachBody;

use super::builder::CarBuilder;
use super::kit::Mat;
use super::shape::{Grain, Shape, rounded_rect};

/// Semiancho exterior del techo, sobre los aleros.
const EAVES: f32 = 1.5;
/// Altura de la cumbrera del techo sobre la linterna.
pub const CROWN: f32 = 4.16;

impl CarBuilder<'_, '_> {
    pub fn coach(&mut self, length: f32) {
        self.underframe(length);
        self.bogies(length);
        self.coach_shell(length);
        self.coach_roof(length);
        self.coach_platforms(length);
        self.coach_inside(length);
    }

    /// Las paredes laterales con sus aberturas de ventana, y las de los
    /// extremos con puertas.
    fn coach_shell(&mut self, length: f32) {
        let (floor, ceiling, wall) = (CoachBody::FLOOR, CoachBody::CEILING, CoachBody::WALL);
        let (sill, lintel) = (CoachBody::SILL, CoachBody::LINTEL);
        let inner = length - 2.0 * CoachBody::END_INSET;
        let half = CoachBody::HALF_WIDTH;
        let (windows, open) = CoachBody::windows(length);
        let band = |lo: f32, hi: f32| ((lo + hi) / 2.0, hi - lo);
        let skirt = DECK_SIDE;
        let mut edges = vec![-inner / 2.0];
        for &x in &windows {
            edges.extend([x - open / 2.0, x + open / 2.0]);
        }
        edges.push(inner / 2.0);
        for side in [-1.0, 1.0] {
            let z = side * (half - wall / 2.0);
            let outside = side * half;
            for (lo, hi) in [(skirt, sill), (lintel, ceiling)] {
                let (y, h) = band(lo, hi);
                self.boxed(Mat::MAROON, Vec3::new(0.0, y, z), Vec3::new(inner, h, wall));
            }
            let (y, h) = band(sill, lintel);
            for pier in edges.chunks(2) {
                let (x, w) = band(pier[0], pier[1]);
                self.boxed(Mat::MAROON, Vec3::new(x, y, z), Vec3::new(w, h, wall));
            }
            // Filetes color crema: la cintura bajo las ventanas, los aleros,
            // una moldura a cada lado de cada pilar y los marcos barnizados de
            // las hojas.
            for y in [sill - 0.06, sill + 0.0, ceiling - 0.1] {
                self.boxed(
                    Mat::Cream,
                    Vec3::new(0.0, y, outside + side * 0.006),
                    Vec3::new(inner - 0.1, 0.022, 0.012),
                );
            }
            for &x in &windows {
                let (y, h) = band(sill, lintel);
                for (cy, ch) in [(sill - 0.02, 0.07), (lintel + 0.02, 0.07)] {
                    self.boxed(
                        Mat::VARNISH,
                        Vec3::new(x, cy, outside + side * 0.015),
                        Vec3::new(open + 0.14, ch, 0.03),
                    );
                }
                for post in [-1.0, 1.0] {
                    self.boxed(
                        Mat::VARNISH,
                        Vec3::new(x + post * (open / 2.0 + 0.035), y, outside + side * 0.015),
                        Vec3::new(0.07, h, 0.03),
                    );
                }
                // Vidrio, con un travesaño de hoja que lo cruza y un pestillo
                // de bronce.
                self.boxed(Mat::Glass, Vec3::new(x, y, z), Vec3::new(open, h, 0.012));
                self.boxed(
                    Mat::VARNISH,
                    Vec3::new(x, y + 0.12, z + side * 0.012),
                    Vec3::new(open, 0.05, 0.03),
                );
                self.boxed(
                    Mat::BRASS,
                    Vec3::new(x, y + 0.12, outside + side * 0.035),
                    Vec3::new(0.08, 0.03, 0.02),
                );
            }
            // Una tabla de estribo a lo largo del pie de la caja, y un
            // pasamanos de bronce en cada extremo junto a la puerta.
            for end in [-1.0, 1.0] {
                self.bar(
                    Mat::BRASS,
                    Vec3::new(
                        end * (inner / 2.0 - 0.08),
                        floor + 0.1,
                        outside + side * 0.05,
                    ),
                    Vec3::new(
                        end * (inner / 2.0 - 0.08),
                        floor + 1.0,
                        outside + side * 0.05,
                    ),
                    0.018,
                );
            }
        }
        // Las paredes de los extremos, cada una con una puerta en el centro.
        let door = 0.9;
        for end in [-1.0, 1.0] {
            let x = end * (inner / 2.0 - wall / 2.0);
            let w = half - door / 2.0;
            let (y, h) = band(skirt, ceiling);
            for side in [-1.0, 1.0] {
                self.boxed(
                    Mat::MAROON,
                    Vec3::new(x, y, side * (half - w / 2.0)),
                    Vec3::new(wall, h, w),
                );
            }
            let (y, h) = band(floor + 2.0, ceiling);
            self.boxed(Mat::MAROON, Vec3::new(x, y, 0.0), Vec3::new(wall, h, door));
            // La hoja de la puerta: barnizada, con paneles, con la parte
            // superior vidriada.
            let outer = end * (inner / 2.0 + 0.012);
            let (y, h) = band(floor, floor + 2.0);
            self.boxed(
                Mat::VARNISH,
                Vec3::new(outer, y, 0.0),
                Vec3::new(0.024, h, door),
            );
            self.boxed(
                Mat::Glass,
                Vec3::new(outer + end * 0.004, floor + 1.5, 0.0),
                Vec3::new(0.01, 0.45, door - 0.3),
            );
            for dz in [-1.0, 1.0] {
                self.boxed(
                    Mat::MAROON,
                    Vec3::new(outer + end * 0.014, floor + 0.55, dz * 0.22),
                    Vec3::new(0.012, 0.7, 0.3),
                );
            }
            self.boxed(
                Mat::BRASS,
                Vec3::new(outer + end * 0.025, floor + 1.0, 0.33),
                Vec3::new(0.04, 0.05, 0.12),
            );
        }
    }

    /// El techo abovedado con su linterna.
    fn coach_roof(&mut self, length: f32) {
        let eave = CoachBody::CEILING + 0.08;
        // El techo: una cara inferior plana en los aleros, con el arco subiendo
        // hasta una cumbrera baja.
        let arch = |half: f32, base: f32, rise: f32, count: usize| -> Vec<Vec2> {
            (0..=count)
                .map(|i| {
                    let a = PI * i as f32 / count as f32;
                    Vec2::new(half * a.cos(), base + rise * a.sin())
                })
                .collect()
        };
        let roof = arch(EAVES, eave, 0.14, 12);
        let reach = length / 2.0 - 0.08;
        self.add(
            Mat::Canvas,
            Shape::along_x(&roof, -reach, reach, true, true),
        );
        // La linterna: un lucernario elevado a lo largo del centro con
        // laterales verticales, la parte superior redondeada y una franja
        // vidriada oscura a cada lado.
        let (low, shoulder) = (eave + 0.1, eave + 0.22);
        let mut lantern = vec![Vec2::new(0.62, low)];
        lantern.extend(arch(0.62, shoulder, CROWN - shoulder, 8));
        lantern.push(Vec2::new(-0.62, low));
        let run = length / 2.0 - 1.1;
        self.add(Mat::Canvas, Shape::along_x(&lantern, -run, run, true, true));
        for side in [-1.0, 1.0] {
            let mut x = -run + 0.25;
            while x + 0.5 <= run - 0.2 {
                self.boxed(
                    Mat::Dark,
                    Vec3::new(x + 0.25, (low + shoulder) / 2.0 + 0.03, side * 0.622),
                    Vec3::new(0.5, 0.07, 0.01),
                );
                x += 0.62;
            }
        }
        // Goterones a lo largo de los aleros.
        for side in [-1.0, 1.0] {
            self.boxed(
                Mat::BLACK,
                Vec3::new(0.0, eave - 0.005, side * (EAVES - 0.01)),
                Vec3::new(2.0 * reach, 0.04, 0.03),
            );
        }
    }

    /// Las plataformas de los extremos: piso, portezuelas y estribos.
    fn coach_platforms(&mut self, length: f32) {
        let floor = CoachBody::FLOOR;
        let inner = length - 2.0 * CoachBody::END_INSET;
        for end in [-1.0, 1.0] {
            let x = end * (length / 2.0 - 0.28);
            // La chapa antideslizante de la plataforma, apoyada sobre el
            // testero.
            self.boxed(
                Mat::TREAD,
                Vec3::new(x, floor - 0.03, 0.0),
                Vec3::new(0.5, 0.06, 2.5),
            );
            // Portezuelas: una baranda de bronce a cada lado de la puerta,
            // sobre postes.
            for side in [-1.0, 1.0] {
                let gate = end * (length / 2.0 - 0.1);
                let span = (0.5, 1.3);
                self.bar(
                    Mat::BRASS,
                    Vec3::new(gate, floor + 0.95, side * span.0),
                    Vec3::new(gate, floor + 0.95, side * span.1),
                    0.018,
                );
                self.bar(
                    Mat::BRASS,
                    Vec3::new(gate, floor + 0.5, side * span.0),
                    Vec3::new(gate, floor + 0.5, side * span.1),
                    0.014,
                );
                for z in [span.0, span.1] {
                    self.bar(
                        Mat::BRASS,
                        Vec3::new(gate, floor, side * z),
                        Vec3::new(gate, floor + 0.98, side * z),
                        0.02,
                    );
                }
                // Estribos que bajan en la esquina, de chapa antideslizante
                // sobre colgantes.
                for (y, dx) in [(0.82, 0.0), (0.58, -end * 0.02)] {
                    self.boxed(
                        Mat::TREAD,
                        Vec3::new(x + dx, y, side * 1.2),
                        Vec3::new(0.34, 0.05, 0.36),
                    );
                }
                self.boxed(
                    Mat::BLACK,
                    Vec3::new(x, 0.9, side * 1.36),
                    Vec3::new(0.05, 0.7, 0.04),
                );
            }
            // Un soporte de farol de cola en la pared del extremo.
            self.boxed(
                Mat::BLACK,
                Vec3::new(end * (inner / 2.0 + 0.05), floor + 2.3, 0.0),
                Vec3::new(0.1, 0.22, 0.2),
            );
            self.boxed(
                Mat::Lamp,
                Vec3::new(end * (inner / 2.0 + 0.105), floor + 2.3, 0.0),
                Vec3::new(0.01, 0.14, 0.12),
            );
        }
    }

    /// Todo lo que está dentro de la carcasa.
    fn coach_inside(&mut self, length: f32) {
        let (floor, ceiling, wall) = (CoachBody::FLOOR, CoachBody::CEILING, CoachBody::WALL);
        let (sill, lintel) = (CoachBody::SILL, CoachBody::LINTEL);
        let inner = length - 2.0 * CoachBody::END_INSET;
        let reach = CoachBody::HALF_WIDTH - wall;
        let (windows, open) = CoachBody::windows(length);
        let pitch = (length - 3.0) / windows.len() as f32;
        // El piso de tablones, una alfombra de pasillo y el cielorraso forrado.
        self.inside_box(
            Mat::VARNISH,
            Vec3::new(0.0, floor - 0.05, 0.0),
            Vec3::new(inner, 0.1, 2.0 * reach),
        );
        self.inside_box(
            Mat::Runner,
            Vec3::new(0.0, floor + 0.006, 0.0),
            Vec3::new(inner - 0.3, 0.012, 0.5),
        );
        self.inside_box(
            Mat::VARNISH,
            Vec3::new(0.0, ceiling + 0.04, 0.0),
            Vec3::new(inner, 0.08, 2.0 * reach),
        );
        // Costillas que cruzan el cielorraso sobre cada pilar, y una moldura a
        // lo largo.
        for x in std::iter::once(-inner / 2.0 + 0.5)
            .chain(windows.iter().map(|x| x + pitch / 2.0))
            .take(windows.len() + 1)
        {
            self.inside_box(
                Mat::Cream,
                Vec3::new(x, ceiling - 0.012, 0.0),
                Vec3::new(0.06, 0.024, 2.0 * reach),
            );
        }
        for z in [-0.55, 0.55] {
            self.inside_box(
                Mat::Cream,
                Vec3::new(0.0, ceiling - 0.008, z),
                Vec3::new(inner, 0.016, 0.03),
            );
        }
        // Revestimiento de pared bajo las ventanas y sobre ellas, con molduras.
        let mut edges = vec![-inner / 2.0];
        for &x in &windows {
            edges.extend([x - open / 2.0, x + open / 2.0]);
        }
        edges.push(inner / 2.0);
        for side in [-1.0, 1.0] {
            let z = side * (reach - 0.012);
            let band = |lo: f32, hi: f32| ((lo + hi) / 2.0, hi - lo);
            for (lo, hi) in [(floor, sill), (lintel, ceiling)] {
                let (y, h) = band(lo, hi);
                self.inside_box(
                    Mat::VARNISH,
                    Vec3::new(0.0, y, z),
                    Vec3::new(inner - 0.04, h, 0.024),
                );
            }
            // Los pilares entre las ventanas, con paneles.
            let (y, h) = band(sill, lintel);
            for pier in edges.chunks(2) {
                let (x, w) = band(pier[0], pier[1]);
                self.inside_box(Mat::VARNISH, Vec3::new(x, y, z), Vec3::new(w, h, 0.024));
            }
            // Molduras de panel en relieve bajo cada ventana, y el derrame
            // barnizado y el alféizar de la ventana.
            for &x in &windows {
                let (cy, ch) = band(floor + 0.12, sill - 0.12);
                let panel = open + 0.3;
                let bead = |cx: f32, cy: f32, w: f32, h: f32| {
                    Shape::cuboid(
                        Vec3::new(cx, cy, z - side * 0.014),
                        Vec3::new(w, h, 0.012),
                        Grain::Along,
                    )
                };
                for shape in [
                    bead(x, cy - ch / 2.0, panel, 0.035),
                    bead(x, cy + ch / 2.0, panel, 0.035),
                    bead(x - panel / 2.0, cy, 0.035, ch),
                    bead(x + panel / 2.0, cy, 0.035, ch),
                ] {
                    self.inside(Mat::Cream, shape);
                }
                let (wy, wh) = band(sill, lintel);
                self.inside_box(
                    Mat::VARNISH,
                    Vec3::new(x, sill + 0.015, side * (reach - 0.06)),
                    Vec3::new(open + 0.1, 0.03, 0.12),
                );
                for post in [-1.0, 1.0] {
                    self.inside_box(
                        Mat::VARNISH,
                        Vec3::new(x + post * (open / 2.0 + 0.0), wy, z - side * 0.005),
                        Vec3::new(0.05, wh, 0.03),
                    );
                }
            }
        }
        // Las paredes de los extremos por dentro: barnizadas, con una puerta
        // forrada y un mapa enmarcado.
        let door = 0.9;
        for end in [-1.0, 1.0] {
            let x = end * (inner / 2.0 - wall - 0.012);
            let (y, h) = ((floor + ceiling) / 2.0, ceiling - floor);
            for side in [-1.0, 1.0] {
                let w = reach - door / 2.0;
                self.inside_box(
                    Mat::VARNISH,
                    Vec3::new(x, y, side * (reach - w / 2.0)),
                    Vec3::new(0.024, h, w),
                );
            }
            self.inside_box(
                Mat::VARNISH,
                Vec3::new(x, floor + 2.0 + (ceiling - floor - 2.0) / 2.0, 0.0),
                Vec3::new(0.024, ceiling - floor - 2.0, door),
            );
            let leaf = x - end * 0.012;
            self.inside_box(
                Mat::VARNISH,
                Vec3::new(leaf, floor + 1.0, 0.0),
                Vec3::new(0.02, 2.0, door),
            );
            self.inside_box(
                Mat::Cream,
                Vec3::new(leaf - end * 0.012, floor + 1.55, 0.0),
                Vec3::new(0.006, 0.45, door - 0.3),
            );
            self.inside_box(
                Mat::BRASS,
                Vec3::new(leaf - end * 0.02, floor + 1.0, 0.33),
                Vec3::new(0.03, 0.05, 0.12),
            );
            // Un mapa del recorrido enmarcado junto a la puerta.
            self.inside_box(
                Mat::BRASS,
                Vec3::new(x - end * 0.02, floor + 1.7, 0.95),
                Vec3::new(0.012, 0.44, 0.64),
            );
            self.inside_box(
                Mat::Cream,
                Vec3::new(x - end * 0.027, floor + 1.7, 0.95),
                Vec3::new(0.006, 0.38, 0.58),
            );
        }
        // Bancos: un asiento y un respaldo en cada extremo de cada
        // compartimento, a ambos lados del pasillo.
        let (seat, back_cushion, frame, end_panel) = bench_shapes();
        for &x in &windows {
            for facing in [-1.0, 1.0] {
                let back = x - facing * (pitch / 2.0 - 0.1);
                let at = back + facing * 0.25;
                for side in [-1.0, 1.0] {
                    // Construido para el lado derecho mirando hacia +X;
                    // espejado.
                    let turn = Quat::from_rotation_y(if facing > 0.0 { 0.0 } else { PI });
                    let mirror = if side > 0.0 { 1.0 } else { -1.0 };
                    let place = |shape: &Shape| {
                        let mut s = shape.clone();
                        if mirror < 0.0 {
                            s = mirror_z(&s);
                        }
                        s.transformed(Transform::from_xyz(at, 0.0, 0.0).with_rotation(turn))
                    };
                    self.inside(Mat::Leather, place(&seat));
                    self.inside(Mat::Leather, place(&back_cushion));
                    self.inside(Mat::VARNISH, place(&frame));
                    self.inside(Mat::VARNISH, place(&end_panel));
                }
            }
        }
        // Portaequipajes de bronce sobre las ventanas de cada lado.
        for side in [-1.0, 1.0] {
            let rail = side * (reach - 0.3);
            let wall_rail = side * (reach - 0.03);
            let y = lintel + 0.28;
            let (x0, x1) = (-inner / 2.0 + 0.3, inner / 2.0 - 0.3);
            self.inside(
                Mat::BRASS,
                super::builder::bar_shape(Vec3::new(x0, y, rail), Vec3::new(x1, y, rail), 0.014, 6),
            );
            self.inside(
                Mat::BRASS,
                super::builder::bar_shape(
                    Vec3::new(x0, y + 0.06, wall_rail),
                    Vec3::new(x1, y + 0.06, wall_rail),
                    0.012,
                    6,
                ),
            );
            let mut x = x0;
            while x <= x1 + 1e-3 {
                // Un soporte desde la pared hasta la barra, y una cuerda de la
                // red.
                self.inside(
                    Mat::BRASS,
                    super::builder::bar_shape(
                        Vec3::new(x, y + 0.1, wall_rail),
                        Vec3::new(x, y, rail),
                        0.012,
                        6,
                    ),
                );
                self.inside(
                    Mat::Runner,
                    super::builder::bar_shape(
                        Vec3::new(x + 0.3, y + 0.06, wall_rail),
                        Vec3::new(x + 0.3, y, rail),
                        0.008,
                        4,
                    ),
                );
                x += 0.6;
            }
        }
        // Faroles de aceite bajo el cielorraso, copas de bronce bajo un globo
        // encendido.
        for x in [-length / 4.0, 0.0, length / 4.0] {
            let cup = [
                (0.0, 0.0),
                (0.0, 0.1),
                (0.05, 0.1),
                (0.08, 0.06),
                (0.08, 0.0),
            ];
            let globe = [
                (0.08, 0.0),
                (0.08, 0.06),
                (0.14, 0.11),
                (0.22, 0.12),
                (0.3, 0.08),
                (0.34, 0.0),
            ];
            let at = Transform::from_xyz(x, ceiling, 0.0)
                .with_rotation(Quat::from_rotation_arc(Vec3::X, -Vec3::Y));
            self.inside(Mat::BRASS, Shape::lathe(&cup, 12).transformed(at));
            self.inside(Mat::Lamp, Shape::lathe(&globe, 12).transformed(at));
            self.inside(
                Mat::BRASS,
                Shape::lathe(&[(0.3, 0.0), (0.3, 0.03), (0.34, 0.0)], 12).transformed(at),
            );
        }
    }
}

/// Hasta dónde baja el faldón de la pared por el costado, sobre la cara
/// superior de los rieles.
const DECK_SIDE: f32 = 1.12;

/// El banco del lado derecho mirando hacia +X, en metros alrededor del centro
/// del asiento del banco (lo coloca el constructor del vagón): el almohadón del
/// asiento, el almohadón del respaldo, el armazón barnizado y el panel lateral
/// junto al pasillo.
pub(super) fn bench_shapes() -> (Shape, Shape, Shape, Shape) {
    let floor = CoachBody::FLOOR;
    let reach = CoachBody::HALF_WIDTH - CoachBody::WALL;
    let (z0, z1) = (0.34, reach - 0.02);
    let width = z1 - z0;
    let mid = (z0 + z1) / 2.0;
    // Almohadón del asiento: 0.5 de profundidad, 0.1 de grosor, redondeado.
    let cushion = Shape::along_z(&rounded_rect(0.5, 0.1, 0.04, 3), z0, z1, true, true)
        .translated(Vec3::new(0.0, floor + 0.43, 0.0));
    // El respaldo se inclina un poco alejándose del centro del compartimento
    // (hacia -X).
    let back = Shape::along_z(&rounded_rect(0.1, 0.62, 0.04, 3), z0, z1, true, true).transformed(
        Transform::from_xyz(-0.27, floor + 0.78, 0.0).with_rotation(Quat::from_rotation_z(0.16)),
    );
    let mut frame = Shape::cuboid(
        Vec3::new(0.0, floor + 0.2, mid),
        Vec3::new(0.46, 0.38, width - 0.04),
        Grain::Along,
    );
    // La tabla del respaldo detrás de su almohadón y un travesaño superior.
    frame.append(
        &Shape::cuboid(
            Vec3::new(-0.0, floor + 0.78, mid),
            Vec3::new(0.03, 0.7, width - 0.08),
            Grain::Along,
        )
        .transformed(Transform::from_xyz(-0.33, 0.0, 0.0)),
    );
    let end_panel = Shape::cuboid(
        Vec3::new(-0.04, floor + 0.45, z0 - 0.02),
        Vec3::new(0.56, 0.9, 0.04),
        Grain::Along,
    );
    (cushion, back, frame, end_panel)
}

/// `shape` espejada respecto del plano Z = 0, conservando el sentido de los
/// triángulos.
fn mirror_z(shape: &Shape) -> Shape {
    let mut out = shape.clone();
    for p in &mut out.positions {
        p[2] = -p[2];
    }
    for n in &mut out.normals {
        n[2] = -n[2];
    }
    // El sentido de los triángulos se invierte con el espejado.
    for t in out.indices.chunks_mut(3) {
        t.swap(1, 2);
    }
    out
}
