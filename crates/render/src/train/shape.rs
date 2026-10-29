//! Un pequeño constructor de meshes para el material rodante: torneados
//! (calderas, chimeneas, cubos de ruedas, faroles), prismas de un perfil
//! dibujado (techos, bielas, almohadones de asientos) y cajas, cada uno con
//! normales suavizadas o con aristas vivas y UVs en metros, así que una textura
//! se repite a su propia escala sea cual sea el tamaño de la pieza.
//!
//! Un [`Shape`] son datos simples en el marco de un vagón (+X hacia adelante,
//! +Y hacia arriba, +Z a la derecha, origen en la cara superior del riel); el
//! constructor del vagón fusiona las formas de un mismo material en una sola
//! mesh.

use std::f32::consts::{PI, TAU};

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

/// Dos aristas vecinas cuyas normales difieren como mucho en este ángulo
/// comparten normales suavizadas en su punto común; una esquina más aguda queda
/// como arista viva.
const CREASE_COS: f32 = 0.6;

/// En qué sentido corren las tablas de una pieza plana en su textura: a lo
/// largo del primer eje de textura de la cara (tablas horizontales en una pared
/// lateral) o a lo ancho (tablas verticales).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grain {
    Along,
    Across,
}

/// Vertices con normales y UVs, y los triángulos sobre ellos.
#[derive(Debug, Clone, Default)]
pub struct Shape {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

impl Shape {
    pub fn triangles(&self) -> usize {
        self.indices.len() / 3
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Agrega los vertices y los triángulos de `other`.
    pub fn append(&mut self, other: &Shape) {
        let base = self.positions.len() as u32;
        self.positions.extend_from_slice(&other.positions);
        self.normals.extend_from_slice(&other.normals);
        self.uvs.extend_from_slice(&other.uvs);
        self.indices.extend(other.indices.iter().map(|i| i + base));
    }

    /// La forma movida por `t`: posiciones escaladas, giradas y desplazadas, y
    /// normales giradas (y corregidas para una escala no uniforme).
    pub fn transformed(&self, t: Transform) -> Shape {
        let inverse_transpose = Mat3::from_diagonal(t.scale).inverse().transpose();
        let mut out = self.clone();
        for p in &mut out.positions {
            *p = t.transform_point(Vec3::from(*p)).to_array();
        }
        for n in &mut out.normals {
            *n = (t.rotation * (inverse_transpose * Vec3::from(*n)))
                .normalize_or_zero()
                .to_array();
        }
        out
    }

    pub fn translated(&self, by: Vec3) -> Shape {
        self.transformed(Transform::from_translation(by))
    }

    /// La cantidad de vertices hasta ahora, como base de índices.
    fn base(&self) -> u32 {
        self.positions.len() as u32
    }

    fn vertex(&mut self, p: Vec3, n: Vec3, uv: Vec2) {
        self.positions.push(p.to_array());
        self.normals.push(n.to_array());
        self.uvs.push(uv.to_array());
    }

    /// Un triángulo sobre tres vertices ya agregados, ordenado para que mire
    /// hacia donde miran las normales de sus vertices.
    fn triangle(&mut self, a: u32, b: u32, c: u32) {
        let at = |i: u32| Vec3::from(self.positions[i as usize]);
        let facing = (at(b) - at(a)).cross(at(c) - at(a));
        let normal = [a, b, c]
            .into_iter()
            .map(|i| Vec3::from(self.normals[i as usize]))
            .sum::<Vec3>();
        if facing.length_squared() < 1e-14 {
            return;
        }
        if facing.dot(normal) >= 0.0 {
            self.indices.extend([a, b, c]);
        } else {
            self.indices.extend([a, c, b]);
        }
    }

    /// Un quad plano con sus UVs en metros, proyectado a lo largo del eje
    /// dominante de su normal: `(z, y)` para una cara orientada a lo largo de
    /// X, `(x, z)` para una orientada hacia arriba o hacia abajo, `(x, y)` para
    /// una orientada a lo largo de Z. [`Grain::Across`] intercambia los dos
    /// ejes de textura.
    pub fn flat(&mut self, corners: [Vec3; 4], grain: Grain) {
        let normal = (corners[1] - corners[0])
            .cross(corners[3] - corners[0])
            .normalize_or_zero();
        let base = self.base();
        for c in corners {
            let (u, v) = project(c, normal);
            let uv = match grain {
                Grain::Along => Vec2::new(u, v),
                Grain::Across => Vec2::new(v, u),
            };
            self.vertex(c, normal, uv);
        }
        self.triangle(base, base + 1, base + 2);
        self.triangle(base, base + 2, base + 3);
    }

    /// Un polígono convexo plano `ring`, en abanico desde su primera esquina,
    /// mirando hacia `normal`.
    pub fn flat_polygon(&mut self, ring: &[Vec3], normal: Vec3, grain: Grain) {
        let base = self.base();
        for &c in ring {
            let (u, v) = project(c, normal);
            let uv = match grain {
                Grain::Along => Vec2::new(u, v),
                Grain::Across => Vec2::new(v, u),
            };
            self.vertex(c, normal, uv);
        }
        for i in 1..ring.len() as u32 - 1 {
            self.triangle(base, base + i, base + i + 1);
        }
    }

    /// Una caja.
    pub fn cuboid(center: Vec3, size: Vec3, grain: Grain) -> Shape {
        let mut shape = Shape::default();
        let h = size / 2.0;
        let corner = |sx: f32, sy: f32, sz: f32| center + Vec3::new(sx * h.x, sy * h.y, sz * h.z);
        let faces = [
            // +X, -X, +Y, -Y, +Z, -Z, cada una en sentido antihorario vista
            // desde afuera.
            [(1., -1., 1.), (1., -1., -1.), (1., 1., -1.), (1., 1., 1.)],
            [
                (-1., -1., -1.),
                (-1., -1., 1.),
                (-1., 1., 1.),
                (-1., 1., -1.),
            ],
            [(-1., 1., 1.), (1., 1., 1.), (1., 1., -1.), (-1., 1., -1.)],
            [
                (-1., -1., -1.),
                (1., -1., -1.),
                (1., -1., 1.),
                (-1., -1., 1.),
            ],
            [(-1., -1., 1.), (1., -1., 1.), (1., 1., 1.), (-1., 1., 1.)],
            [
                (1., -1., -1.),
                (-1., -1., -1.),
                (-1., 1., -1.),
                (1., 1., -1.),
            ],
        ];
        for face in faces {
            shape.flat(face.map(|(x, y, z)| corner(x, y, z)), grain);
        }
        shape
    }

    /// Una superficie de revolución alrededor del eje X: `profile` son puntos
    /// `(x, radius)` y `sides` las facetas alrededor. Suavizada donde el perfil
    /// se curva suavemente y con arista viva donde gira bruscamente (se repite
    /// un punto para empezar un tramo nuevo); un radio de cero cierra el
    /// extremo. UVs: `u` es la vuelta en metros y `v` la longitud del perfil.
    pub fn lathe(profile: &[(f32, f32)], sides: usize) -> Shape {
        let points: Vec<Vec2> = profile.iter().map(|&(x, r)| Vec2::new(x, r)).collect();
        let edges = creased(&points, false);
        let mut shape = Shape::default();
        let mut arc = 0.0;
        for (k, edge) in edges.iter().enumerate() {
            let (a, b) = (points[k], points[k + 1]);
            let len = a.distance(b);
            let mean = (a.y + b.y) / 2.0;
            let base = shape.base();
            for i in 0..=sides {
                let angle = TAU * i as f32 / sides as f32;
                let (sin, cos) = angle.sin_cos();
                let u = angle * mean;
                for (point, normal, v) in [(a, edge[0], arc), (b, edge[1], arc + len)] {
                    shape.vertex(
                        Vec3::new(point.x, point.y * cos, point.y * sin),
                        Vec3::new(normal.x, normal.y * cos, normal.y * sin),
                        Vec2::new(u, v),
                    );
                }
            }
            for i in 0..sides as u32 {
                let q = base + 2 * i;
                shape.triangle(q, q + 1, q + 3);
                shape.triangle(q, q + 3, q + 2);
            }
            arc += len;
        }
        shape
    }

    /// El polígono cerrado `ring` (en sentido antihorario, en las coordenadas
    /// `(a, b)` de un plano) extruido `depth` a lo largo de la normal del plano
    /// desde `-depth / 2` hasta `depth / 2`, llevado al marco por `place`, que
    /// envía `(a, b, along)` a un punto. `smooth` redondea las normales de una
    /// esquina entre aristas vecinas donde se curvan suavemente. UVs: `u` a lo
    /// largo de la extrusión y `v` a lo largo del anillo, en metros; las tapas
    /// (cuando `caps`, en abanico desde la primera esquina: el anillo debe ser
    /// convexo) son planas.
    pub fn prism(
        ring: &[Vec2],
        depth: f32,
        smooth: bool,
        caps: bool,
        place: impl Fn(f32, f32, f32) -> Vec3,
    ) -> Shape {
        let mut shape = Shape::default();
        let mut closed = ring.to_vec();
        closed.push(ring[0]);
        let edges = if smooth {
            creased(&closed, true)
        } else {
            flat_edges(&closed)
        };
        let origin = place(0.0, 0.0, 0.0);
        // Las direcciones se transforman linealmente; la normal también se saca
        // de `place`.
        let turn = |v: Vec2| place(v.x, v.y, 0.0) - origin;
        let mut arc = 0.0;
        for (k, edge) in edges.iter().enumerate() {
            let (a, b) = (closed[k], closed[k + 1]);
            let len = a.distance(b);
            let base = shape.base();
            for (point, normal, v) in [(a, edge[0], arc), (b, edge[1], arc + len)] {
                for (end, along) in [(0.0, -depth / 2.0), (1.0, depth / 2.0)] {
                    shape.vertex(
                        place(point.x, point.y, along),
                        turn(normal).normalize_or_zero(),
                        Vec2::new(end * depth, v),
                    );
                }
            }
            shape.triangle(base, base + 1, base + 3);
            shape.triangle(base, base + 3, base + 2);
            arc += len;
        }
        if caps {
            let out = place(0.0, 0.0, 1.0) - origin;
            for (sign, along) in [(-1.0, -depth / 2.0), (1.0, depth / 2.0)] {
                let corners: Vec<Vec3> = ring.iter().map(|p| place(p.x, p.y, along)).collect();
                shape.flat_polygon(&corners, out * sign, Grain::Along);
            }
        }
        shape
    }

    /// `prism` a lo largo de X: `ring` en `(z, y)`, desde `x0` hasta `x1`.
    pub fn along_x(ring: &[Vec2], x0: f32, x1: f32, smooth: bool, caps: bool) -> Shape {
        let mid = (x0 + x1) / 2.0;
        Shape::prism(ring, x1 - x0, smooth, caps, move |z, y, x| {
            Vec3::new(mid + x, y, z)
        })
    }

    /// `prism` a lo largo de Z: `ring` en `(x, y)`, desde `z0` hasta `z1`.
    pub fn along_z(ring: &[Vec2], z0: f32, z1: f32, smooth: bool, caps: bool) -> Shape {
        let mid = (z0 + z1) / 2.0;
        Shape::prism(ring, z1 - z0, smooth, caps, move |x, y, z| {
            Vec3::new(x, y, mid + z)
        })
    }

    /// `prism` a lo largo de Y: `ring` en `(x, z)`, desde `y0` hasta `y1`.
    pub fn along_y(ring: &[Vec2], y0: f32, y1: f32, smooth: bool, caps: bool) -> Shape {
        let mid = (y0 + y1) / 2.0;
        Shape::prism(ring, y1 - y0, smooth, caps, move |x, z, y| {
            Vec3::new(x, mid + y, z)
        })
    }

    /// La mesh a dibujar.
    pub fn into_mesh(self) -> Mesh {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs)
        .with_inserted_indices(Indices::U32(self.indices))
    }

    /// La caja que rodea a todos los vertices: `(min, max)`.
    pub fn bounds(&self) -> (Vec3, Vec3) {
        self.positions.iter().fold(
            (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
            |(lo, hi), p| (lo.min(Vec3::from(*p)), hi.max(Vec3::from(*p))),
        )
    }
}

/// Las `(u, v)` de una caja en metros sobre una cara con esta normal.
fn project(p: Vec3, normal: Vec3) -> (f32, f32) {
    let a = normal.abs();
    if a.x >= a.y && a.x >= a.z {
        (p.z, p.y)
    } else if a.y >= a.z {
        (p.x, p.z)
    } else {
        (p.x, p.y)
    }
}

/// La normal unitaria hacia afuera de cada arista de `points` y, por arista,
/// sus dos normales de extremo: la propia de la arista, o el promedio con la de
/// la vecina donde se curvan menos que el ángulo de arista viva. `wrap` une la
/// última arista con la primera (un anillo cerrado).
fn creased(points: &[Vec2], wrap: bool) -> Vec<[Vec2; 2]> {
    let own = edge_normals(points, wrap);
    let n = own.len();
    let blend = |a: Vec2, b: Vec2| {
        if a.dot(b) >= CREASE_COS {
            (a + b).normalize_or_zero()
        } else {
            a
        }
    };
    (0..n)
        .map(|k| {
            let before = if k > 0 {
                Some(own[k - 1])
            } else if wrap {
                Some(own[n - 1])
            } else {
                None
            };
            let after = if k + 1 < n {
                Some(own[k + 1])
            } else if wrap {
                Some(own[0])
            } else {
                None
            };
            [
                before.map_or(own[k], |b| blend(own[k], b)),
                after.map_or(own[k], |a| blend(own[k], a)),
            ]
        })
        .collect()
}

fn flat_edges(points: &[Vec2]) -> Vec<[Vec2; 2]> {
    edge_normals(points, true)
        .into_iter()
        .map(|n| [n, n])
        .collect()
}

/// La normal unitaria de cada arista de una polilínea. Un `ring` cerrado tiene
/// sus normales hacia afuera sin importar en qué sentido se recorra; un perfil
/// abierto (los puntos `(x, radius)` de un torneado, recorridos desde el
/// extremo bajo hasta el alto por el exterior) las tiene a la izquierda de su
/// dirección. Una arista de longitud cero conserva la de su predecesora.
fn edge_normals(points: &[Vec2], ring: bool) -> Vec<Vec2> {
    let counter_clockwise = !ring
        || points
            .windows(2)
            .map(|w| w[0].x * w[1].y - w[1].x * w[0].y)
            .sum::<f32>()
            >= 0.0;
    let mut last = Vec2::Y;
    points
        .windows(2)
        .map(|w| {
            let d = (w[1] - w[0]).normalize_or_zero();
            if d != Vec2::ZERO {
                // La derecha de la dirección es hacia afuera para un anillo que
                // va en sentido antihorario; el perfil necesita la izquierda.
                let right = Vec2::new(d.y, -d.x);
                last = match (ring, counter_clockwise) {
                    (true, true) | (false, false) => right,
                    _ => -right,
                };
            }
            last
        })
        .collect()
}

/// `n + 1` puntos del arco de radio `r` alrededor de `center` desde el ángulo
/// `a0` hasta `a1`, en radianes.
pub fn arc(center: Vec2, r: f32, a0: f32, a1: f32, n: usize) -> Vec<Vec2> {
    (0..=n)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / n as f32;
            center + Vec2::new(a.cos(), a.sin()) * r
        })
        .collect()
}

/// Un rectángulo redondeado de `w` por `h` alrededor del origen, en sentido
/// antihorario, con esquinas de radio `r` en `n` pasos.
pub fn rounded_rect(w: f32, h: f32, r: f32, n: usize) -> Vec<Vec2> {
    let (hx, hy) = (w / 2.0 - r, h / 2.0 - r);
    let mut ring = Vec::new();
    for (corner, a0) in [
        (Vec2::new(hx, hy), 0.0),
        (Vec2::new(-hx, hy), PI / 2.0),
        (Vec2::new(-hx, -hy), PI),
        (Vec2::new(hx, -hy), 3.0 * PI / 2.0),
    ] {
        ring.extend(arc(corner, r, a0, a0 + PI / 2.0, n));
    }
    ring
}

/// Una barra de `length` de largo y `width` de ancho alrededor del origen a lo
/// largo de X, con los extremos redondeados.
pub fn stadium(length: f32, width: f32, n: usize) -> Vec<Vec2> {
    let r = width / 2.0;
    let half = length / 2.0 - r;
    let mut ring = arc(Vec2::new(half, 0.0), r, -PI / 2.0, PI / 2.0, n);
    ring.extend(arc(Vec2::new(-half, 0.0), r, PI / 2.0, 3.0 * PI / 2.0, n));
    ring
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cada triángulo mira hacia donde miran las normales de sus vertices,
    /// ninguno es degenerado y todos los índices están dentro del rango.
    fn assert_sound(shape: &Shape) {
        assert!(shape.triangles() > 0);
        assert_eq!(shape.positions.len(), shape.normals.len());
        assert_eq!(shape.positions.len(), shape.uvs.len());
        for t in shape.indices.chunks(3) {
            let at = |i: u32| Vec3::from(shape.positions[i as usize]);
            let facing = (at(t[1]) - at(t[0])).cross(at(t[2]) - at(t[0]));
            assert!(facing.length() > 1e-7, "degenerate");
            let normal: Vec3 = t
                .iter()
                .map(|&i| Vec3::from(shape.normals[i as usize]))
                .sum();
            assert!(facing.dot(normal) > 0.0, "winding against its normal");
        }
        for n in &shape.normals {
            assert!((Vec3::from(*n).length() - 1.0).abs() < 1e-3);
        }
    }

    #[test]
    fn a_box_is_twelve_triangles_facing_out_with_uvs_in_meters() {
        let shape = Shape::cuboid(
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::new(2.0, 1.0, 0.5),
            Grain::Along,
        );
        assert_sound(&shape);
        assert_eq!(shape.triangles(), 12);
        // Cada normal apunta en dirección opuesta al centro.
        for (p, n) in shape.positions.iter().zip(&shape.normals) {
            let out = Vec3::from(*p) - Vec3::new(1.0, 2.0, 3.0);
            assert!(out.dot(Vec3::from(*n)) > 0.0);
        }
        // La cara +Z abarca 2 m en x y 1 m en y: lo mismo su UV.
        let front: Vec<_> = shape
            .positions
            .iter()
            .zip(&shape.uvs)
            .zip(&shape.normals)
            .filter(|(_, n)| n[2] > 0.9)
            .map(|((_, uv), _)| *uv)
            .collect();
        let span = |k: usize| {
            front.iter().map(|uv| uv[k]).fold(f32::MIN, f32::max)
                - front.iter().map(|uv| uv[k]).fold(f32::MAX, f32::min)
        };
        assert!((span(0) - 2.0).abs() < 1e-5 && (span(1) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn across_grain_swaps_the_texture_axes() {
        let along = Shape::cuboid(Vec3::ZERO, Vec3::new(2.0, 1.0, 0.5), Grain::Along);
        let across = Shape::cuboid(Vec3::ZERO, Vec3::new(2.0, 1.0, 0.5), Grain::Across);
        for (a, b) in along.uvs.iter().zip(&across.uvs) {
            assert_eq!([a[1], a[0]], *b);
        }
    }

    #[test]
    fn a_lathe_is_round_smooth_and_closed_at_a_zero_radius() {
        // Un cilindro de 2 m de largo y 1 m de diámetro, tapado por dos discos.
        let shape = Shape::lathe(&[(-1.0, 0.0), (-1.0, 0.5), (1.0, 0.5), (1.0, 0.0)], 16);
        assert_sound(&shape);
        let (lo, hi) = shape.bounds();
        assert!((lo - Vec3::new(-1.0, -0.5, -0.5)).abs().max_element() < 1e-5);
        assert!((hi - Vec3::new(1.0, 0.5, 0.5)).abs().max_element() < 1e-5);
        // Las normales del cilindro son radiales; las de las tapas, axiales.
        for (p, n) in shape.positions.iter().zip(&shape.normals) {
            let (p, n) = (Vec3::from(*p), Vec3::from(*n));
            if n.x.abs() < 0.01 {
                let radial = Vec3::new(0.0, p.y, p.z).normalize();
                assert!(radial.dot(n) > 0.999);
            } else {
                assert!(n.x.abs() > 0.999);
            }
        }
        // Alrededor del cilindro, `u` avanza en metros: la circunferencia.
        let barrel: Vec<f32> = shape
            .uvs
            .iter()
            .zip(&shape.normals)
            .filter(|(_, n)| n[0].abs() < 0.01)
            .map(|(uv, _)| uv[0])
            .collect();
        let span = barrel.iter().copied().fold(f32::MIN, f32::max);
        assert!((span - PI).abs() < 1e-4, "{span}");
    }

    #[test]
    fn a_gentle_bend_is_smooth_and_a_sharp_one_is_creased() {
        // Un perfil con una curva suave y después un ángulo recto.
        let shape = Shape::lathe(&[(0.0, 1.0), (1.0, 1.4), (2.0, 1.6), (2.0, 0.0)], 8);
        let normals_at = |x: f32| -> Vec<Vec3> {
            shape
                .positions
                .iter()
                .zip(&shape.normals)
                .filter(|(p, _)| (p[0] - x).abs() < 1e-5 && p[2].abs() < 1e-5 && p[1] > 0.5)
                .map(|(_, n)| Vec3::from(*n))
                .collect()
        };
        // En x = 1 ambas aristas comparten una normal mezclada.
        let mid = normals_at(1.0);
        assert!(mid.len() >= 2 && mid.windows(2).all(|w| w[0].dot(w[1]) > 0.9999));
        // En x = 2 el cilindro y el disco conservan la suya: perpendiculares.
        let corner = normals_at(2.0);
        assert!(corner.iter().any(|n| n.x > 0.99) && corner.iter().any(|n| n.x < 0.3));
    }

    #[test]
    fn a_prism_along_x_has_smooth_sides_and_flat_caps_in_meters() {
        let ring = rounded_rect(1.0, 0.6, 0.2, 4);
        let shape = Shape::along_x(&ring, 0.0, 3.0, true, true);
        assert_sound(&shape);
        let (lo, hi) = shape.bounds();
        assert!(
            (lo - Vec3::new(0.0, -0.3, -0.5)).abs().max_element() < 1e-5,
            "{lo}"
        );
        assert!((hi - Vec3::new(3.0, 0.3, 0.5)).abs().max_element() < 1e-5);
        // `u` recorre los 3 m de su largo.
        let u_max = shape.uvs.iter().map(|uv| uv[0]).fold(f32::MIN, f32::max);
        assert!((u_max - 3.0).abs() < 1e-5);
        // Las normales laterales se mantienen perpendiculares a su largo.
        for (n, uv) in shape.normals.iter().zip(&shape.uvs) {
            if n[0].abs() > 0.5 {
                assert!(n[0].abs() > 0.999 && uv[0].abs() < 3.01);
            }
        }
    }

    #[test]
    fn a_stadium_bar_is_as_long_and_wide_as_asked() {
        let ring = stadium(3.0, 0.2, 4);
        let xs = ring.iter().map(|p| p.x);
        let ys = ring.iter().map(|p| p.y);
        assert!((xs.clone().fold(f32::MIN, f32::max) - 1.5).abs() < 1e-5);
        assert!((xs.fold(f32::MAX, f32::min) + 1.5).abs() < 1e-5);
        assert!((ys.fold(f32::MIN, f32::max) - 0.1).abs() < 1e-5);
    }
}
