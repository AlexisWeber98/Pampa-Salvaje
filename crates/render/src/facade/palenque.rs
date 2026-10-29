//! Un palenque, la barra para atar caballos de un pueblo pampeano: dos postes
//! de madera ochavados y una barra redonda que los atraviesa, de tablas
//! curtidas por la intemperie ([`Surface::Planks`]). Se apoya en el suelo en el
//! origen, con la barra a lo largo de `x` (se gira con un transform para una
//! barra en el otro eje). Core decide dónde va cada uno
//! ([`pampa_core::TownLayout::palenques`] y el
//! [`pampa_core::town::HitchingPost`] del almacén); render los construye todos
//! en una sola mesh por pueblo.

use bevy::prelude::*;

use super::materials::Surface;
use super::shape::Shape;

/// Lado de un poste, en metros.
pub const POST: f32 = 0.18;
/// Altura de un poste sobre el suelo, en metros.
pub const POST_HEIGHT: f32 = 1.15;
/// Chaflán de las cuatro aristas verticales de un poste, en metros.
const POST_CHAMFER: f32 = 0.04;
/// Cuánto se mete el pie de un poste bajo el suelo, en metros.
const FOOT: f32 = -0.05;
/// Distancia desde el extremo de una barra hasta el centro de su poste, en
/// metros.
pub const POST_INSET: f32 = 0.2;
/// Altura del eje de la barra sobre el suelo, en metros.
pub const BAR_HEIGHT: f32 = 1.0;
/// Radio de la barra, en metros.
pub const BAR_RADIUS: f32 = 0.05;
/// Lados de la sección de la barra.
const BAR_SEGMENTS: u32 = 8;

/// Un palenque de `length` a lo largo de `x`, en el suelo en su centro: un
/// poste cerca de cada extremo y la barra cruzada de punta a punta.
pub(crate) fn palenque(length: f32) -> Shape {
    let mut shape = Shape::default();
    let half = POST / 2.0;
    for end in [-1.0, 1.0] {
        let x = end * (length / 2.0 - POST_INSET);
        shape.chamfered(
            Surface::Planks,
            Vec3::new(x - half, FOOT, -half),
            Vec3::new(x + half, POST_HEIGHT, half),
            POST_CHAMFER,
        );
    }
    // La barra es un torneado a lo largo del Y local, acostado a lo largo de
    // `x`.
    let lay = Transform::from_translation(Vec3::new(-length / 2.0, BAR_HEIGHT, 0.0))
        .with_rotation(Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2));
    shape.lathe(
        Surface::Planks,
        lay,
        &[
            (0.0, 0.0),
            (BAR_RADIUS, 0.0),
            (BAR_RADIUS, length),
            (0.0, length),
        ],
        BAR_SEGMENTS,
    );
    shape
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_palenque_is_two_chamfered_posts_and_a_round_bar_in_boards() {
        let length = 2.8;
        let shape = palenque(length);
        assert_eq!(shape.surfaces().collect::<Vec<_>>(), [Surface::Planks]);
        let vertices = shape.vertices(Surface::Planks);
        // Dentro de su huella: el largo de la barra, el ancho de un poste, la
        // altura de los postes.
        for (p, _, _) in &vertices {
            assert!(p.x.abs() <= length / 2.0 + 1e-4, "{p}");
            assert!(p.z.abs() <= POST / 2.0 + 1e-4, "{p}");
            assert!((FOOT..=POST_HEIGHT + 1e-4).contains(&p.y), "{p}");
        }
        // Postes cerca de ambos extremos, sobresaliendo del suelo.
        for end in [-1.0f32, 1.0] {
            assert!(vertices.iter().any(|(p, _, _)| {
                (p.x - end * (length / 2.0 - POST_INSET)).abs() < POST / 2.0
                    && (p.y - POST_HEIGHT).abs() < 1e-4
            }));
        }
        // Ochavados: caras inclinadas 45 grados alrededor de la vertical,
        // además de las cuadradas.
        assert!(
            vertices
                .iter()
                .any(|(_, n, _)| n.y.abs() < 1e-3 && n.x.abs() > 0.5 && n.z.abs() > 0.5),
            "no chamfer"
        );
        // Una barra redonda de punta a punta a su altura: las normales dan toda
        // la vuelta alrededor.
        let bar: Vec<_> = vertices
            .iter()
            .filter(|(p, _, _)| {
                (p.y - BAR_HEIGHT).abs() <= BAR_RADIUS + 1e-3 && p.z.abs() <= BAR_RADIUS + 1e-3
            })
            .collect();
        assert!(
            bar.iter()
                .any(|(p, _, _)| (p.x + length / 2.0).abs() < 1e-3)
        );
        assert!(
            bar.iter()
                .any(|(p, _, _)| (p.x - length / 2.0).abs() < 1e-3)
        );
        let up = bar.iter().any(|(_, n, _)| n.y > 0.9);
        let down = bar.iter().any(|(_, n, _)| n.y < -0.9);
        let sides = bar.iter().any(|(_, n, _)| n.z.abs() > 0.9);
        assert!(up && down && sides, "the bar is not round");
        // Liviano: dos postes y una barra.
        assert!(shape.triangles() < 120, "{} triangles", shape.triangles());
    }
}
