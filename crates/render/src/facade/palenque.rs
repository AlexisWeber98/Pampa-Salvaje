//! A palenque, the hitching rail of a pampa town: two chamfered timber
//! posts and a round bar through them, in weathered boards
//! ([`Surface::Planks`]). It stands on the ground at the origin, its rail
//! along `x` (turn it with a transform for a rail along the other axis).
//! Core lays out where each one stands ([`pampa_core::TownLayout::palenques`]
//! and the store's [`pampa_core::town::HitchingPost`]); render builds them
//! all into one mesh per town.

use bevy::prelude::*;

use super::materials::Surface;
use super::shape::Shape;

/// Side of a post, meters.
pub const POST: f32 = 0.18;
/// Height of a post over the ground, meters.
pub const POST_HEIGHT: f32 = 1.15;
/// Bevel of a post's four vertical edges, meters.
const POST_CHAMFER: f32 = 0.04;
/// How far a post's foot reaches under the ground, meters.
const FOOT: f32 = -0.05;
/// Distance from a rail's end to its post's middle, meters.
pub const POST_INSET: f32 = 0.2;
/// Height of the bar's axis over the ground, meters.
pub const BAR_HEIGHT: f32 = 1.0;
/// Radius of the bar, meters.
pub const BAR_RADIUS: f32 = 0.05;
/// Sides of the bar's section.
const BAR_SEGMENTS: u32 = 8;

/// A palenque of `length` along `x`, on the ground at its middle: a post
/// near each end and the bar across, end to end.
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
    // The bar is a lathe along local Y, laid down along `x`.
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
        // Within its footprint: the rail's length, a post's width, the posts' height.
        for (p, _, _) in &vertices {
            assert!(p.x.abs() <= length / 2.0 + 1e-4, "{p}");
            assert!(p.z.abs() <= POST / 2.0 + 1e-4, "{p}");
            assert!((FOOT..=POST_HEIGHT + 1e-4).contains(&p.y), "{p}");
        }
        // Posts near both ends, standing proud of the ground.
        for end in [-1.0f32, 1.0] {
            assert!(vertices.iter().any(|(p, _, _)| {
                (p.x - end * (length / 2.0 - POST_INSET)).abs() < POST / 2.0
                    && (p.y - POST_HEIGHT).abs() < 1e-4
            }));
        }
        // Chamfered: faces leaning 45 degrees about the vertical, besides the square ones.
        assert!(
            vertices
                .iter()
                .any(|(_, n, _)| n.y.abs() < 1e-3 && n.x.abs() > 0.5 && n.z.abs() > 0.5),
            "no chamfer"
        );
        // A round bar end to end at its height: normals turn all the way round it.
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
        // Light: two posts and one bar.
        assert!(shape.triangles() < 120, "{} triangles", shape.triangles());
    }
}
