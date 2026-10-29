//! Un pequeño constructor de meshes para los edificios y accesorios
//! procedurales de los asentamientos y los pueblos.
//!
//! Cada pieza se agrega al buffer de su [`Surface`], así que una estructura
//! termina como una mesh por cada superficie que lleva. Las caras llevan
//! normales planas, UVs escaladas al mundo (una repetición de la textura cada
//! [`Surface::tiles`] metros, sea cual sea el tamaño de la pieza) y tangentes
//! para los normal maps.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use super::materials::Surface;
use crate::ground::face_tangent;

/// Los vertices de la mesh de una superficie mientras se construye.
#[derive(Default, Debug, Clone)]
struct Buffers {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    tangents: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

/// Las meshes de una estructura, una por superficie, en el espacio propio de la
/// estructura.
#[derive(Default, Debug, Clone)]
pub(crate) struct Shape {
    parts: Vec<(Surface, Buffers)>,
}

/// Las caras del cubo unitario, con las esquinas en sentido antihorario vistas
/// desde afuera: +X, -X, +Y, +Z, -Z, -Y.
const CUBE: [[[f32; 3]; 4]; 6] = [
    [
        [0.5, -0.5, -0.5],
        [0.5, 0.5, -0.5],
        [0.5, 0.5, 0.5],
        [0.5, -0.5, 0.5],
    ],
    [
        [-0.5, -0.5, 0.5],
        [-0.5, 0.5, 0.5],
        [-0.5, 0.5, -0.5],
        [-0.5, -0.5, -0.5],
    ],
    [
        [-0.5, 0.5, -0.5],
        [-0.5, 0.5, 0.5],
        [0.5, 0.5, 0.5],
        [0.5, 0.5, -0.5],
    ],
    [
        [-0.5, -0.5, 0.5],
        [0.5, -0.5, 0.5],
        [0.5, 0.5, 0.5],
        [-0.5, 0.5, 0.5],
    ],
    [
        [0.5, -0.5, -0.5],
        [-0.5, -0.5, -0.5],
        [-0.5, 0.5, -0.5],
        [0.5, 0.5, -0.5],
    ],
    [
        [-0.5, -0.5, -0.5],
        [0.5, -0.5, -0.5],
        [0.5, -0.5, 0.5],
        [-0.5, -0.5, 0.5],
    ],
];

/// La UV planar de `p` en una cara con normal `n`: en las paredes y las
/// pendientes la `u` va horizontal y la `v` sube por la cara; los pisos y los
/// techos planos sobre el suelo toman la del plano del suelo.
fn planar_uv(p: Vec3, n: Vec3, tile: Vec2) -> [f32; 2] {
    if n.y.abs() > 0.9 {
        return [p.x / tile.x, p.z / tile.y];
    }
    let u = Vec3::Y.cross(n).normalize_or(Vec3::X);
    let v = n.cross(u);
    [p.dot(u) / tile.x, -p.dot(v) / tile.y]
}

impl Shape {
    fn buffers(&mut self, surface: Surface) -> &mut Buffers {
        let k = match self.parts.iter().position(|(s, _)| *s == surface) {
            Some(k) => k,
            None => {
                self.parts.push((surface, Buffers::default()));
                self.parts.len() - 1
            }
        };
        &mut self.parts[k].1
    }

    /// Un polígono plano (un triángulo o un quad), con las esquinas en sentido
    /// antihorario vistas desde afuera.
    pub fn face(&mut self, surface: Surface, corners: &[Vec3]) {
        debug_assert!(corners.len() == 3 || corners.len() == 4);
        let n = (corners[1] - corners[0])
            .cross(corners[2] - corners[0])
            .normalize_or(Vec3::Y);
        let tile = surface.tiles();
        let uv: Vec<[f32; 2]> = corners.iter().map(|&p| planar_uv(p, n, tile)).collect();
        let tangent = face_tangent([corners[0], corners[1], corners[2]], [uv[0], uv[1], uv[2]]);
        let b = self.buffers(surface);
        let base = b.positions.len() as u32;
        for (p, uv) in corners.iter().zip(uv) {
            b.positions.push(p.to_array());
            b.normals.push(n.to_array());
            b.uvs.push(uv);
            b.tangents.push(tangent);
        }
        b.indices.extend([base, base + 1, base + 2]);
        if corners.len() == 4 {
            b.indices.extend([base, base + 2, base + 3]);
        }
    }

    /// Una caja: el cubo unitario pasado por `transform`. `bottom` conserva su
    /// cara inferior (se omite para lo que se apoya en el suelo).
    pub fn cuboid(&mut self, surface: Surface, transform: Transform, bottom: bool) {
        let m = transform.to_matrix();
        let faces = if bottom { &CUBE[..] } else { &CUBE[..5] };
        for face in faces {
            let c = face.map(|p| m.transform_point3(Vec3::from_array(p)));
            self.face(surface, &c);
        }
    }

    /// Una caja alineada con los ejes desde `min` hasta `max`, sin su cara
    /// inferior.
    pub fn block(&mut self, surface: Surface, min: Vec3, max: Vec3) {
        self.cuboid(
            surface,
            Transform::from_translation((min + max) / 2.0).with_scale(max - min),
            false,
        );
    }

    /// Una caja alineada con los ejes desde `min` hasta `max` sin su cara
    /// inferior, con sus cuatro aristas verticales achaflanadas con un ancho de
    /// `chamfer`: un prisma octogonal.
    pub fn chamfered(&mut self, surface: Surface, min: Vec3, max: Vec3, chamfer: f32) {
        let c = chamfer
            .min((max.x - min.x) / 3.0)
            .min((max.z - min.z) / 3.0);
        if c <= 1e-4 {
            self.block(surface, min, max);
            return;
        }
        // Las esquinas de la sección, en sentido antihorario vistas desde
        // arriba.
        let ring = [
            (max.x - c, max.z),
            (max.x, max.z - c),
            (max.x, min.z + c),
            (max.x - c, min.z),
            (min.x + c, min.z),
            (min.x, min.z + c),
            (min.x, max.z - c),
            (min.x + c, max.z),
        ];
        for k in 0..ring.len() {
            let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
            self.face(
                surface,
                &[
                    Vec3::new(a.0, min.y, a.1),
                    Vec3::new(b.0, min.y, b.1),
                    Vec3::new(b.0, max.y, b.1),
                    Vec3::new(a.0, max.y, a.1),
                ],
            );
        }
        // La parte superior, como un abanico de quads alrededor de su centro.
        let top = |(x, z): (f32, f32)| Vec3::new(x, max.y, z);
        for k in (0..ring.len()).step_by(2) {
            let n = ring.len();
            self.face(
                surface,
                &[
                    top(ring[k]),
                    top(ring[(k + 1) % n]),
                    top(ring[(k + 2) % n]),
                    Vec3::new((min.x + max.x) / 2.0, max.y, (min.z + max.z) / 2.0),
                ],
            );
        }
    }

    /// Agrega todas las piezas de `other`, movidas por el `transform` rígido.
    pub fn merge(&mut self, other: Shape, transform: Transform) {
        let m = transform.to_matrix();
        for (surface, part) in other.parts {
            let b = self.buffers(surface);
            let base = b.positions.len() as u32;
            for k in 0..part.positions.len() {
                let p = m.transform_point3(Vec3::from_array(part.positions[k]));
                let n = transform.rotation * Vec3::from_array(part.normals[k]);
                let [tx, ty, tz, w] = part.tangents[k];
                let t = transform.rotation * Vec3::new(tx, ty, tz);
                b.positions.push(p.to_array());
                b.normals.push(n.to_array());
                b.uvs.push(part.uvs[k]);
                b.tangents.push([t.x, t.y, t.z, w]);
            }
            b.indices.extend(part.indices.iter().map(|i| base + i));
        }
    }

    /// Una viga de sección cuadrada de `thickness` de grosor desde `from` hasta
    /// `to`, con los extremos cerrados.
    pub fn beam(&mut self, surface: Surface, from: Vec3, to: Vec3, thickness: f32) {
        let d = to - from;
        let len = d.length();
        if len <= 1e-4 {
            return;
        }
        self.cuboid(
            surface,
            Transform::from_translation((from + to) / 2.0)
                .with_rotation(Quat::from_rotation_arc(Vec3::Y, d / len))
                .with_scale(Vec3::new(thickness, len, thickness)),
            true,
        );
    }

    /// Una superficie de revolución alrededor del eje Y local de `transform`:
    /// `profile` es (radio, altura) de abajo hacia arriba, con `segments` lados
    /// alrededor. Normales suavizadas alrededor, `u` a lo largo de la
    /// circunferencia y `v` subiendo por el perfil, ambas en metros sobre el
    /// tile de la superficie. Un radio de 0 cierra la forma en ese punto. Las
    /// normales miran hacia afuera cuando el perfil sube por el exterior; se
    /// pasa de arriba hacia abajo para una pared interior.
    pub fn lathe(
        &mut self,
        surface: Surface,
        transform: Transform,
        profile: &[(f32, f32)],
        segments: u32,
    ) {
        let m = transform.to_matrix();
        let normal_m = m.inverse().transpose();
        let tile = surface.tiles();
        let b = self.buffers(surface);
        let base = b.positions.len() as u32;
        let ring = segments + 1;
        // Longitud de arco subiendo por el perfil, para `v`.
        let mut along = vec![0.0f32; profile.len()];
        for k in 1..profile.len() {
            let (r0, y0) = profile[k - 1];
            let (r1, y1) = profile[k];
            along[k] = along[k - 1] + (r1 - r0).hypot(y1 - y0);
        }
        let mean_r = profile
            .iter()
            .map(|p| p.0.abs())
            .fold(0.0, f32::max)
            .max(0.05);
        for (k, &(r, y)) in profile.iter().enumerate() {
            // La pendiente del perfil en este punto, para la normal; un punto
            // repetido en el perfil crea una arista viva, y cada copia toma la
            // pendiente de su propio lado.
            let last = profile.len() - 1;
            let before = if k > 0 && profile[k - 1] != profile[k] {
                k - 1
            } else {
                k
            };
            let after = if k < last && profile[k + 1] != profile[k] {
                k + 1
            } else {
                k
            };
            let (a, c) = if before == after {
                (profile[k.saturating_sub(1)], profile[(k + 1).min(last)])
            } else {
                (profile[before], profile[after])
            };
            let (dr, dy) = (c.0 - a.0, c.1 - a.1);
            for s in 0..ring {
                let angle = s as f32 / segments as f32 * std::f32::consts::TAU;
                let (sin, cos) = angle.sin_cos();
                let local = Vec3::new(r * cos, y, r * sin);
                let n_local = Vec3::new(dy * cos, -dr, dy * sin).normalize_or(Vec3::Y);
                let t_local = Vec3::new(-sin, 0.0, cos);
                let p = m.transform_point3(local);
                let n = normal_m.transform_vector3(n_local).normalize_or(Vec3::Y);
                let t = m.transform_vector3(t_local).normalize_or(Vec3::X);
                // `v` crece bajando por el perfil, así que la bitangente apunta
                // hacia abajo por él: su signo mantiene el +Y del normal map
                // hacia arriba.
                let up = m.transform_vector3(Vec3::new(dr * cos, dy, dr * sin));
                let w = if n.cross(t).dot(-up) < 0.0 { -1.0 } else { 1.0 };
                b.positions.push(p.to_array());
                b.normals.push(n.to_array());
                b.uvs.push([angle * mean_r / tile.x, -along[k] / tile.y]);
                b.tangents.push([t.x, t.y, t.z, w]);
            }
        }
        for k in 0..profile.len() as u32 - 1 {
            for s in 0..segments {
                let (a, c) = (base + k * ring + s, base + (k + 1) * ring + s);
                b.indices.extend([a, c + 1, a + 1, a, c, c + 1]);
            }
        }
    }

    /// Un disco plano de radio `radius` mirando hacia arriba, alrededor del
    /// origen de `transform`.
    pub fn disc(&mut self, surface: Surface, transform: Transform, radius: f32, segments: u32) {
        let m = transform.to_matrix();
        let center = m.transform_point3(Vec3::ZERO);
        for s in 0..segments {
            let at = |k: u32| {
                let a = k as f32 / segments as f32 * std::f32::consts::TAU;
                m.transform_point3(Vec3::new(radius * a.cos(), 0.0, radius * a.sin()))
            };
            self.face(surface, &[center, at(s + 1), at(s)]);
        }
    }

    /// Los triángulos de todas sus meshes.
    pub fn triangles(&self) -> usize {
        self.parts.iter().map(|(_, b)| b.indices.len() / 3).sum()
    }

    /// Las superficies que lleva.
    #[cfg(test)]
    pub fn surfaces(&self) -> impl Iterator<Item = Surface> + '_ {
        self.parts.iter().map(|(s, _)| *s)
    }

    /// Los puntos de la mesh de `surface`, para tests y límites.
    #[cfg(test)]
    pub fn points(&self, surface: Surface) -> Vec<Vec3> {
        self.parts
            .iter()
            .filter(|(s, _)| *s == surface)
            .flat_map(|(_, b)| b.positions.iter().map(|p| Vec3::from_array(*p)))
            .collect()
    }

    /// Los triángulos de la mesh de `surface`, para tests.
    #[cfg(test)]
    pub fn triangles_of(&self, surface: Surface) -> Vec<[Vec3; 3]> {
        self.parts
            .iter()
            .filter(|(s, _)| *s == surface)
            .flat_map(|(_, b)| {
                b.indices
                    .chunks(3)
                    .map(|t| [0, 1, 2].map(|k| Vec3::from_array(b.positions[t[k] as usize])))
            })
            .collect()
    }

    /// Los vertices de la mesh de `surface` como (posición, normal, uv), para
    /// tests.
    #[cfg(test)]
    pub fn vertices(&self, surface: Surface) -> Vec<(Vec3, Vec3, Vec2)> {
        self.parts
            .iter()
            .filter(|(s, _)| *s == surface)
            .flat_map(|(_, b)| {
                (0..b.positions.len()).map(|i| {
                    (
                        Vec3::from_array(b.positions[i]),
                        Vec3::from_array(b.normals[i]),
                        Vec2::from_array(b.uvs[i]),
                    )
                })
            })
            .collect()
    }

    /// Los índices de la mesh de `surface`, en [`Self::vertices`], para tests y
    /// vistas previas.
    #[cfg(test)]
    pub fn indices(&self, surface: Surface) -> Vec<u32> {
        let mut base = 0;
        let mut out = Vec::new();
        for (s, b) in &self.parts {
            if *s == surface {
                out.extend(b.indices.iter().map(|i| base + i));
                base += b.positions.len() as u32;
            }
        }
        out
    }

    /// Una mesh por superficie.
    pub fn into_meshes(self) -> Vec<(Surface, Mesh)> {
        self.parts
            .into_iter()
            .filter(|(_, b)| !b.indices.is_empty())
            .map(|(surface, b)| {
                let mesh = Mesh::new(
                    PrimitiveTopology::TriangleList,
                    RenderAssetUsages::RENDER_WORLD,
                )
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, b.positions)
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, b.normals)
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, b.uvs)
                .with_inserted_attribute(Mesh::ATTRIBUTE_TANGENT, b.tangents)
                .with_inserted_indices(Indices::U32(b.indices));
                (surface, mesh)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn uvs_repeat_with_the_world_not_the_piece() {
        let mut shape = Shape::default();
        // Una pared de 10 m y una de 1 m: la pared larga se repite diez veces
        // más.
        shape.block(Surface::Whitewash, Vec3::ZERO, Vec3::new(10.0, 3.0, 0.5));
        let tile = Surface::Whitewash.tile();
        let mut meshes = shape.into_meshes();
        let (_, mesh) = meshes.remove(0);
        let Some(bevy::mesh::VertexAttributeValues::Float32x2(uvs)) =
            mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!()
        };
        // La cara frontal (+Z): la cuarta del cubo, cuatro esquinas.
        let front = &uvs[12..16];
        let span = front.iter().map(|u| u[0]).fold(f32::MIN, f32::max)
            - front.iter().map(|u| u[0]).fold(f32::MAX, f32::min);
        assert!((span - 10.0 / tile).abs() < 1e-3, "{span}");
        assert!(mesh.attribute(Mesh::ATTRIBUTE_TANGENT).is_some());
    }

    #[test]
    fn a_lathe_closes_round_and_faces_out() {
        let mut shape = Shape::default();
        shape.lathe(
            Surface::Galvanised,
            Transform::IDENTITY,
            &[(2.0, 0.0), (2.0, 1.5)],
            24,
        );
        assert_eq!(shape.triangles(), 48);
        let mesh = shape.into_meshes().remove(0).1;
        let (
            Some(bevy::mesh::VertexAttributeValues::Float32x3(p)),
            Some(bevy::mesh::VertexAttributeValues::Float32x3(n)),
        ) = (
            mesh.attribute(Mesh::ATTRIBUTE_POSITION),
            mesh.attribute(Mesh::ATTRIBUTE_NORMAL),
        )
        else {
            panic!()
        };
        for (p, n) in p.iter().zip(n) {
            let out = Vec3::new(p[0], 0.0, p[2]).normalize();
            assert!(Vec3::from_array(*n).dot(out) > 0.99);
        }
    }

    #[test]
    fn a_chamfered_block_bevels_its_vertical_edges_and_faces_out() {
        let mut shape = Shape::default();
        let (min, max) = (Vec3::new(-0.25, 0.0, -0.1), Vec3::new(0.25, 3.0, 0.1));
        shape.chamfered(Surface::Whitewash, min, max, 0.04);
        let vertices = shape.vertices(Surface::Whitewash);
        // Ocho lados y una tapa, sin cara inferior.
        assert_eq!(shape.triangles(), 8 * 2 + 4 * 2);
        assert!(vertices.iter().all(|(_, n, _)| n.y > -0.5));
        // Cuatro chaflanes a 45 grados entre las caras.
        let bevels: HashSet<[i32; 3]> = vertices
            .iter()
            .filter(|(_, n, _)| n.y.abs() < 0.1 && n.x.abs() > 0.5 && n.z.abs() > 0.5)
            .map(|(_, n, _)| (*n * 10.0).round().as_ivec3().to_array())
            .collect();
        assert_eq!(bevels.len(), 4, "{bevels:?}");
        // Cada cara mira en dirección opuesta al centro y queda dentro de la
        // caja.
        let mid = (min + max) / 2.0;
        for [a, b, c] in shape.triangles_of(Surface::Whitewash) {
            let n = (b - a).cross(c - a);
            assert!(n.dot((a + b + c) / 3.0 - mid) > 0.0);
            for p in [a, b, c] {
                assert!(p.cmpge(min - 1e-5).all() && p.cmple(max + 1e-5).all());
            }
        }
    }

    #[test]
    fn a_merged_shape_moves_rigidly_into_its_place() {
        let mut part = Shape::default();
        part.block(Surface::Brick, Vec3::ZERO, Vec3::new(2.0, 1.0, 0.5));
        part.block(Surface::Planks, Vec3::ZERO, Vec3::ONE);
        let before = part.vertices(Surface::Brick);
        let turn = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let place = Transform::from_xyz(5.0, 1.0, -3.0).with_rotation(turn);
        let mut shape = Shape::default();
        shape.block(Surface::Brick, Vec3::splat(10.0), Vec3::splat(11.0));
        shape.merge(part.clone(), place);
        assert_eq!(shape.triangles(), 10 + part.triangles());
        let after = shape.vertices(Surface::Brick);
        // Primero el bloque propio del ladrillo, después el fusionado, movido y
        // girado.
        for ((p0, n0, uv0), (p1, n1, uv1)) in before.iter().zip(&after[before.len()..]) {
            assert!(place.transform_point(*p0).distance(*p1) < 1e-5);
            assert!((turn * *n0).distance(*n1) < 1e-5);
            assert_eq!(uv0, uv1);
        }
        // Sus índices apuntan a sus propios vertices.
        let indices = shape.indices(Surface::Brick);
        assert!(
            indices[indices.len() / 2..]
                .iter()
                .all(|&i| i as usize >= before.len())
        );
    }
}
