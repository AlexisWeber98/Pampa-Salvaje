//! Normales de vértice suaves que conservan los pliegues marcados, para mallas
//! importadas que llegan con una normal por cara (los OBJ de vegetación).
//!
//! Es el mismo algoritmo que usa `tools/glbtrim.py` para los animales GLB. Los
//! vértices se sueldan por posición, solo para las normales: cada vértice
//! conserva su propio color y UV, así que las costuras entre materiales quedan
//! donde estaban. Alrededor de cada punto soldado, las caras que comparten una
//! arista que se dobla menos que el umbral se unen en un mismo grupo suave,
//! como hace el auto smooth de un modelador; una esquina toma el grupo de su
//! propia cara, promediado por ángulo de esquina. Unir a través de aristas
//! compartidas, en lugar de compararlas contra una cara de referencia, permite
//! que un cono o cilindro tosco se suavice en toda su vuelta mientras la arista
//! de una caja sigue marcada.
//!
//! Se ejecuta una sola vez, cuando se hornea un modelo; el resultado es
//! determinista y tiene exactamente una normal por vértice de entrada.

use std::collections::HashMap;

use bevy::math::Vec3;

/// Normales suaves para una lista de triángulos sin índices (`positions` en
/// tripletes, caras en sentido antihorario), una por vértice. Las caras que
/// comparten una arista que se dobla menos de `max_angle_degrees` se mezclan;
/// las aristas más agudas siguen marcadas. Una esquina de una cara de área cero
/// toma el promedio alrededor de su punto, o +Y.
pub fn smooth_normals(positions: &[[f32; 3]], max_angle_degrees: f32) -> Vec<[f32; 3]> {
    let limit = max_angle_degrees.to_radians().cos();
    // Ids de soldadura por posición exacta (+0 y -0 por igual), en orden de
    // aparición.
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

    // Alrededor de cada punto soldado: sus caras y sus ángulos de esquina, y la
    // ranura de cada vértice en el abanico de su punto.
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

    // Las caras alrededor de un punto que comparten una arista que se dobla
    // menos que el límite se unen en un grupo (union-find sobre las ranuras del
    // abanico).
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
            // (el otro punto soldado de la arista, ranura), ordenado para que
            // las ranuras de la misma arista queden juntas.
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

    /// Dos triángulos que se juntan a lo largo de x = 0 con una cresta que se
    /// dobla `bend` grados en total, como una lista de triángulos de 6
    /// vértices.
    fn ridge(bend: f32) -> Vec<[f32; 3]> {
        let half = (bend / 2.0).to_radians();
        let (dy, dx) = (half.sin(), half.cos());
        // La cara izquierda sube hasta la cresta en x = 0, la derecha baja
        // desde ella.
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
        // Las dos esquinas sobre la arista compartida apuntan derecho hacia
        // arriba y coinciden.
        for (a, b) in [(1, 3), (2, 5)] {
            assert_eq!(p[a], p[b]);
            assert!((v(n[a]) - v(n[b])).length() < 1e-6, "{n:?}");
            assert!(v(n[a]).dot(Vec3::Y) > 0.9999, "{:?}", n[a]);
        }
        // Las esquinas exteriores conservan la normal de su propia cara.
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

    /// Un cono de `sides` facetas alrededor con el vértice superior hacia
    /// arriba: cada faceta toca el vértice, así que las dos facetas opuestas
    /// del cono quedan muy dobladas entre sí.
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
        // Las facetas vecinas se doblan unos 32 grados; las facetas opuestas a
        // través del vértice superior, unos 90. Unir por aristas compartidas
        // igual forma un solo grupo.
        let n = smooth_normals(&p, 45.0);
        let apex: Vec<Vec3> = (0..8).map(|k| v(n[3 * k + 2])).collect();
        for a in &apex {
            assert!(a.dot(Vec3::Y) > 0.9999, "apex normal {a:?}");
        }
        // Las esquinas del anillo compartidas por dos facetas coinciden y se
        // inclinan hacia afuera.
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
        // Dos caras de una caja unitaria que se juntan a 90 grados, con la tapa
        // dividida en dos triángulos cuya diagonal compartida es una costura
        // plana.
        let p = vec![
            // Tapa, y = 1.
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, 1.0, -1.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, -1.0],
            [0.0, 1.0, -1.0],
            // Frente, z = 0.
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
