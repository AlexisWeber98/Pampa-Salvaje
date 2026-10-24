//! Smooth vertex normals with hard creases kept, for imported meshes that
//! arrive with one normal per face (the vegetation OBJs).
//!
//! The same algorithm as `tools/glbtrim.py` uses for the GLB animals. Vertices
//! are welded by position, for the normals only: every vertex keeps its own
//! colour and UV, so the seams between materials stay where they were.
//! Around each welded point, faces that share an edge bending less than the
//! threshold join one smooth group, as a modeller's auto smooth does; a
//! corner takes its own face's group, averaged by corner angle. Joining
//! through shared edges rather than against one reference face lets a coarse
//! cone or cylinder smooth all the way round while a box edge stays sharp.
//!
//! It runs once, when a model is baked; the output is deterministic and has
//! exactly one normal per input vertex.

use std::collections::HashMap;

use bevy::math::Vec3;

/// Smooth normals for a non-indexed triangle list (`positions` in triplets,
/// counter-clockwise faces), one per vertex. Faces sharing an edge that bends
/// by less than `max_angle_degrees` blend; sharper edges stay creased. A
/// corner of a zero-area face takes the average around its point, or +Y.
pub fn smooth_normals(positions: &[[f32; 3]], max_angle_degrees: f32) -> Vec<[f32; 3]> {
    let limit = max_angle_degrees.to_radians().cos();
    // Weld ids by exact position (+0 and -0 alike), in first-seen order.
    let mut ids = HashMap::new();
    let weld: Vec<usize> = positions
        .iter()
        .map(|p| {
            let key = p.map(|c| (c + 0.0).to_bits());
            let next = ids.len();
            *ids.entry(key).or_insert(next)
        })
        .collect();
    let points: Vec<Vec3> = positions.iter().map(|p| Vec3::from_array(*p)).collect();
    let faces = positions.len() / 3;
    let normals: Vec<Option<Vec3>> = (0..faces)
        .map(|f| {
            let [a, b, c] = [0, 1, 2].map(|k| points[3 * f + k]);
            let n = (b - a).cross(c - a);
            (n.length_squared() > 1e-20).then(|| n.normalize())
        })
        .collect();

    // Around each welded point: its faces and their corner angles, and each
    // vertex's slot in its point's fan.
    let mut fans: Vec<Vec<(usize, f32)>> = vec![Vec::new(); ids.len()];
    let mut slot = vec![None; positions.len()];
    for (f, normal) in normals.iter().enumerate() {
        if normal.is_none() {
            continue;
        }
        for k in 0..3 {
            let v = 3 * f + k;
            let (prev, next) = (points[3 * f + (k + 2) % 3], points[3 * f + (k + 1) % 3]);
            let angle = (next - points[v]).angle_between(prev - points[v]);
            slot[v] = Some(fans[weld[v]].len());
            fans[weld[v]].push((f, angle));
        }
    }

    // Faces around a point that share an edge bending less than the limit
    // join one group (union-find over the fan's slots).
    let groups: Vec<Vec<usize>> = fans
        .iter()
        .enumerate()
        .map(|(w, fan)| {
            let weld = &weld;
            let mut parent: Vec<usize> = (0..fan.len()).collect();
            fn root(parent: &mut [usize], mut i: usize) -> usize {
                while parent[i] != i {
                    parent[i] = parent[parent[i]];
                    i = parent[i];
                }
                i
            }
            // (the edge's other welded point, slot), sorted so slots on the
            // same edge sit together.
            let mut edges: Vec<(usize, usize)> = fan
                .iter()
                .enumerate()
                .flat_map(|(s, &(f, _))| {
                    (0..3)
                        .map(move |k| weld[3 * f + k])
                        .filter(move |&o| o != w)
                        .map(move |o| (o, s))
                })
                .collect();
            edges.sort_unstable();
            edges.dedup();
            for run in edges.chunk_by(|a, b| a.0 == b.0) {
                for (x, &(_, s)) in run.iter().enumerate() {
                    for &(_, t) in &run[x + 1..] {
                        let (Some(a), Some(b)) = (normals[fan[s].0], normals[fan[t].0]) else {
                            continue;
                        };
                        if a.dot(b) >= limit {
                            let (rs, rt) = (root(&mut parent, s), root(&mut parent, t));
                            parent[rs] = rt;
                        }
                    }
                }
            }
            (0..fan.len()).map(|s| root(&mut parent, s)).collect()
        })
        .collect();

    (0..positions.len())
        .map(|v| {
            let (fan, group) = (&fans[weld[v]], &groups[weld[v]]);
            let mine = slot[v].map(|s| group[s]);
            let sum: Vec3 = fan
                .iter()
                .zip(group)
                .filter(|&(_, g)| mine.is_none_or(|m| *g == m))
                .filter_map(|(&(f, angle), _)| normals[f].map(|n| n * angle))
                .sum();
            let own = normals[v / 3];
            sum.try_normalize().or(own).unwrap_or(Vec3::Y).to_array()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(p: [f32; 3]) -> Vec3 {
        Vec3::from_array(p)
    }

    /// Two triangles meeting along x = 0 with a ridge bending `bend` degrees
    /// in total, as a triangle list of 6 vertices.
    fn ridge(bend: f32) -> Vec<[f32; 3]> {
        let half = (bend / 2.0).to_radians();
        let (dy, dx) = (half.sin(), half.cos());
        // Left face rises to the ridge at x = 0, right face falls from it.
        vec![
            [-dx, -dy, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
            [0.0, 0.0, 1.0],
            [dx, -dy, 0.0],
            [0.0, 0.0, -1.0],
        ]
    }

    #[test]
    fn a_gentle_ridge_is_smooth_across_its_edge() {
        let p = ridge(30.0);
        let n = smooth_normals(&p, 45.0);
        assert_eq!(n.len(), p.len(), "one normal per vertex");
        // Both corners on the shared edge point straight up, and match.
        for (a, b) in [(1, 3), (2, 5)] {
            assert_eq!(p[a], p[b]);
            assert!((v(n[a]) - v(n[b])).length() < 1e-6, "{n:?}");
            assert!(v(n[a]).dot(Vec3::Y) > 0.9999, "{:?}", n[a]);
        }
        // The outer corners keep their own face's normal.
        assert!(v(n[0]).dot(Vec3::Y) < 0.99);
        for x in &n {
            assert!((v(*x).length() - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn a_sharp_ridge_keeps_its_crease() {
        let p = ridge(90.0);
        let n = smooth_normals(&p, 45.0);
        assert_eq!(n.len(), p.len());
        let left = v(p[1]) - v(p[0]);
        let left = left.cross(v(p[2]) - v(p[0])).normalize();
        assert!(v(n[1]).dot(left) > 0.9999, "{:?} vs {left:?}", n[1]);
        assert!(v(n[1]).dot(v(n[3])) < 0.8, "the crease stays: {n:?}");
    }

    /// A cone of `sides` facets round with its apex up: every facet meets the
    /// apex, so the two facets across the cone bend far apart.
    fn cone(sides: u32) -> Vec<[f32; 3]> {
        let ring = |k: u32| {
            let a = std::f32::consts::TAU * k as f32 / sides as f32;
            [a.cos(), 0.0, -a.sin()]
        };
        (0..sides)
            .flat_map(|k| [ring(k), ring((k + 1) % sides), [0.0, 1.0, 0.0]])
            .collect()
    }

    #[test]
    fn a_coarse_cone_smooths_all_the_way_round_through_shared_edges() {
        let p = cone(8);
        // Neighbouring facets bend about 32 degrees; facets across the apex
        // bend about 90. Joining by shared edges still makes one group.
        let n = smooth_normals(&p, 45.0);
        let apex: Vec<Vec3> = (0..8).map(|k| v(n[3 * k + 2])).collect();
        for a in &apex {
            assert!(a.dot(Vec3::Y) > 0.9999, "apex normal {a:?}");
        }
        // Ring corners shared by two facets match, and lean outward.
        for k in 0..8 {
            let here = v(n[3 * k + 1]);
            let next = v(n[3 * ((k + 1) % 8)]);
            assert!((here - next).length() < 1e-5, "{k}: {here:?} {next:?}");
            let out = v(p[3 * k + 1]);
            assert!(here.dot(out) > 0.5);
        }
    }

    #[test]
    fn a_box_keeps_its_edges_hard_and_a_seam_does_not_split_a_flat_face() {
        // Two faces of a unit box meeting at 90 degrees, the top split
        // in two triangles whose shared diagonal is a flat seam.
        let p = vec![
            // Top, y = 1.
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, 1.0, -1.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, -1.0],
            [0.0, 1.0, -1.0],
            // Front, z = 0.
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        let n = smooth_normals(&p, 60.0);
        for k in 0..6 {
            assert!(v(n[k]).dot(Vec3::Y) > 0.9999, "top {k}: {:?}", n[k]);
            assert!(
                v(n[6 + k]).dot(Vec3::Z) > 0.9999,
                "front {k}: {:?}",
                n[6 + k]
            );
        }
    }

    #[test]
    fn smoothing_is_deterministic_and_handles_degenerate_faces() {
        let mut p = cone(7);
        p.extend([[2.0, 0.0, 0.0]; 3]);
        let a = smooth_normals(&p, 50.0);
        let b = smooth_normals(&p, 50.0);
        assert_eq!(a.len(), p.len());
        assert_eq!(
            a.iter().map(|n| n.map(f32::to_bits)).collect::<Vec<_>>(),
            b.iter().map(|n| n.map(f32::to_bits)).collect::<Vec<_>>()
        );
        assert!(a.iter().all(|n| n.iter().all(|c| c.is_finite())));
        assert_eq!(a[a.len() - 1], [0.0, 1.0, 0.0]);
        assert!(smooth_normals(&[], 50.0).is_empty());
    }
}
