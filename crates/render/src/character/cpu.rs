//! Test-only CPU skinning: every mesh drawn in a world, posed as the GPU
//! would pose it, for the clearance checks and the offline previews.

use bevy::mesh::VertexAttributeValues;
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::prelude::*;

/// One drawn mesh in world space.
#[derive(Debug, Clone)]
pub struct Drawn {
    pub entity: Entity,
    /// Its glTF material's name, or else its own.
    pub name: String,
    pub positions: Vec<Vec3>,
    pub triangles: Vec<[u32; 3]>,
    /// Linear RGB per vertex: the material's colour times the mesh's.
    pub colors: Vec<[f32; 3]>,
    /// Each vertex's strongest joint (none for a mesh not skinned).
    pub strongest: Vec<Option<Entity>>,
}

/// Every visible mesh under `root` (or in the world), skinned or placed.
pub fn drawn(world: &mut World, root: Option<Entity>) -> Vec<Drawn> {
    let mut query = world.query::<(
        Entity,
        &Mesh3d,
        &GlobalTransform,
        Option<&SkinnedMesh>,
        Option<&MeshMaterial3d<StandardMaterial>>,
        Option<&Name>,
        Option<&bevy::gltf::GltfMaterialName>,
    )>();
    // Hidden by itself or an ancestor (the headless apps have no
    // visibility systems to work it out).
    let hidden = |world: &World, e: Entity| {
        let mut at = e;
        loop {
            if world.get::<Visibility>(at) == Some(&Visibility::Hidden) {
                return true;
            }
            match world.get::<ChildOf>(at) {
                Some(c) => at = c.parent(),
                None => return false,
            }
        }
    };
    let under = |world: &World, e: Entity| {
        let Some(root) = root else { return true };
        let mut at = e;
        loop {
            if at == root {
                return true;
            }
            match world.get::<ChildOf>(at) {
                Some(c) => at = c.parent(),
                None => return false,
            }
        }
    };
    let meshes = world.resource::<Assets<Mesh>>();
    let binds = world.get_resource::<Assets<SkinnedMeshInverseBindposes>>();
    let materials = world.get_resource::<Assets<StandardMaterial>>();
    let mut out = Vec::new();
    for (entity, mesh, at, skin, material, name, gltf) in query.iter(world) {
        if hidden(world, entity) || !under(world, entity) {
            continue;
        }
        let Some(mesh) = meshes.get(&mesh.0) else {
            continue;
        };
        let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            continue;
        };
        let joints: Option<Vec<Mat4>> = skin.and_then(|s| {
            let ibm = binds?.get(&s.inverse_bindposes)?;
            s.joints
                .iter()
                .zip(ibm.iter())
                .map(|(j, b)| Some(world.get::<GlobalTransform>(*j)?.to_matrix() * *b))
                .collect()
        });
        let mut strongest = vec![None; p.len()];
        let positions: Vec<Vec3> = match (
            joints,
            mesh.attribute(Mesh::ATTRIBUTE_JOINT_INDEX),
            mesh.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT),
        ) {
            (
                Some(joints),
                Some(VertexAttributeValues::Uint16x4(ji)),
                Some(VertexAttributeValues::Float32x4(jw)),
            ) => {
                if let Some(skin) = skin {
                    for ((s, j), w) in strongest.iter_mut().zip(ji).zip(jw) {
                        let k = (0..4).max_by(|a, b| w[*a].total_cmp(&w[*b])).unwrap_or(0);
                        *s = skin.joints.get(j[k] as usize).copied();
                    }
                }
                p.iter()
                    .zip(ji)
                    .zip(jw)
                    .map(|((p, j), w)| {
                        (0..4)
                            .map(|k| w[k] * joints[j[k] as usize].transform_point3(Vec3::from(*p)))
                            .sum()
                    })
                    .collect()
            }
            _ => p
                .iter()
                .map(|p| at.transform_point(Vec3::from(*p)))
                .collect(),
        };
        let base = material
            .and_then(|m| materials?.get(&m.0))
            .map_or(LinearRgba::gray(0.8), |m| m.base_color.to_linear());
        let colors = match mesh.attribute(Mesh::ATTRIBUTE_COLOR) {
            Some(VertexAttributeValues::Float32x4(c)) => c
                .iter()
                .map(|c| [base.red * c[0], base.green * c[1], base.blue * c[2]])
                .collect(),
            _ => vec![[base.red, base.green, base.blue]; positions.len()],
        };
        let triangles: Vec<[u32; 3]> = match mesh.indices() {
            Some(i) => {
                let i: Vec<u32> = i.iter().map(|i| i as u32).collect();
                i.as_chunks::<3>().0.to_vec()
            }
            None => (0..positions.len() as u32 / 3)
                .map(|t| [3 * t, 3 * t + 1, 3 * t + 2])
                .collect(),
        };
        out.push(Drawn {
            entity,
            name: gltf.map_or_else(
                || name.map_or_else(String::new, |n| n.to_string()),
                |m| m.0.to_string(),
            ),
            positions,
            triangles,
            colors,
            strongest,
        });
    }
    out.sort_by_key(|d| d.entity);
    out
}

/// Writes `drawn` as text for the offline preview renderer: a `m name`
/// line per mesh, then `v x y z r g b` lines and `f a b c` lines (0-based
/// within the mesh).
pub fn write(path: &std::path::Path, drawn: &[Drawn]) {
    use std::fmt::Write as _;
    let mut text = String::new();
    for d in drawn {
        let _ = writeln!(text, "m {}", d.name.replace(' ', "_"));
        for (p, c) in d.positions.iter().zip(&d.colors) {
            let _ = writeln!(
                text,
                "v {:.4} {:.4} {:.4} {:.3} {:.3} {:.3}",
                p.x, p.y, p.z, c[0], c[1], c[2]
            );
        }
        for t in &d.triangles {
            let _ = writeln!(text, "f {} {} {}", t[0], t[1], t[2]);
        }
    }
    std::fs::write(path, text).expect("preview written");
}
