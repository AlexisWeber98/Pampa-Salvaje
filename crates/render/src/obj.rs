//! A small Wavefront OBJ and MTL reader for the vegetation models.
//!
//! It reads the "Ultimate Nature Pack by Quaternius, CC0 1.0; see
//! assets/models/vegetation/quaternius-ultimate-nature/CREDITS.md". Only
//! what those files use is supported.

use std::collections::HashMap;
use std::fmt;
use std::path::Path;

/// Diffuse colours (`Kd`) by material name.
pub type MaterialColors = HashMap<String, [f32; 3]>;

/// The triangles that use one material.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ObjGroup {
    pub material: String,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Option<Vec<[f32; 2]>>,
}

/// A parsed OBJ file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ObjModel {
    pub mtllib: Option<String>,
    pub groups: Vec<ObjGroup>,
}

/// Why a model could not be read.
#[derive(Debug, Clone, PartialEq)]
pub enum ObjError {
    Io { path: String, error: String },
    Parse { line: usize, message: String },
    Empty,
}

impl fmt::Display for ObjError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, error } => write!(f, "cannot read {path}: {error}"),
            Self::Parse { line, message } => write!(f, "line {line}: {message}"),
            Self::Empty => write!(f, "no faces"),
        }
    }
}

impl std::error::Error for ObjError {}

fn parse_error(line: usize, message: impl Into<String>) -> ObjError {
    ObjError::Parse {
        line: line + 1,
        message: message.into(),
    }
}

/// `N` numbers from `words`, exactly.
fn numbers<const N: usize>(
    line: usize,
    mut words: std::str::SplitWhitespace,
) -> Result<[f32; N], ObjError> {
    let mut out = [0.0; N];
    for slot in &mut out {
        let word = words
            .next()
            .ok_or_else(|| parse_error(line, format!("expected {N} numbers")))?;
        *slot = word
            .parse()
            .ok()
            .filter(|v: &f32| v.is_finite())
            .ok_or_else(|| parse_error(line, format!("not a number: {word}")))?;
    }
    Ok(out)
}

/// Parses MTL text into each material's diffuse colour. Every statement
/// but `newmtl` and `Kd` is ignored.
pub fn parse_mtl(text: &str) -> Result<MaterialColors, ObjError> {
    let mut colors = MaterialColors::new();
    let mut current: Option<String> = None;
    for (n, line) in text.lines().enumerate() {
        let mut words = line.split_whitespace();
        match words.next() {
            Some("newmtl") => {
                let name = words
                    .next()
                    .ok_or_else(|| parse_error(n, "newmtl without a name"))?;
                colors.insert(name.to_owned(), [0.8; 3]);
                current = Some(name.to_owned());
            }
            Some("Kd") => {
                let kd = numbers::<3>(n, words)?;
                let name = current
                    .as_ref()
                    .ok_or_else(|| parse_error(n, "Kd before newmtl"))?;
                colors.insert(name.clone(), kd);
            }
            _ => {}
        }
    }
    Ok(colors)
}

/// One face corner's position, texture and normal indices, zero-based.
type Corner = (usize, Option<usize>, Option<usize>);

/// A one-based (or negative, relative) OBJ index into a list of `len`.
fn index(line: usize, word: &str, len: usize) -> Result<usize, ObjError> {
    let i: i64 = word
        .parse()
        .map_err(|_| parse_error(line, format!("bad index: {word}")))?;
    let resolved = if i > 0 {
        i - 1
    } else if i < 0 {
        len as i64 + i
    } else {
        -1
    };
    if (0..len as i64).contains(&resolved) {
        Ok(resolved as usize)
    } else {
        Err(parse_error(line, format!("index out of range: {word}")))
    }
}

/// A corner in the `v`, `v/vt`, `v//vn` or `v/vt/vn` form.
fn corner(line: usize, word: &str, lens: (usize, usize, usize)) -> Result<Corner, ObjError> {
    let mut parts = word.split('/');
    let v = index(line, parts.next().unwrap_or(""), lens.0)?;
    let optional = |part: Option<&str>, len| match part {
        None | Some("") => Ok(None),
        Some(p) => index(line, p, len).map(Some),
    };
    let vt = optional(parts.next(), lens.1)?;
    let vn = optional(parts.next(), lens.2)?;
    Ok((v, vt, vn))
}

/// Parses OBJ text into triangles grouped by material. Quads and n-gons
/// are fan-triangulated from their first corner; faces without normals get
/// their flat face normal. Statements other than `v`, `vt`, `vn`, `f`,
/// `usemtl` and `mtllib` are ignored.
pub fn parse_obj(text: &str) -> Result<ObjModel, ObjError> {
    let (mut v, mut vt, mut vn) = (Vec::new(), Vec::new(), Vec::new());
    let mut model = ObjModel::default();
    let mut current = 0;
    model.groups.push(ObjGroup::default());
    for (n, line) in text.lines().enumerate() {
        let mut words = line.split_whitespace();
        match words.next() {
            Some("v") => v.push(numbers::<3>(n, words)?),
            Some("vn") => vn.push(numbers::<3>(n, words)?),
            Some("vt") => {
                let [a, b] = numbers::<2>(n, words)?;
                vt.push([a, b]);
            }
            Some("mtllib") => model.mtllib = words.next().map(str::to_owned),
            Some("usemtl") => {
                let name = words.next().unwrap_or("").to_owned();
                current = match model.groups.iter().position(|g| g.material == name) {
                    Some(i) => i,
                    None => {
                        model.groups.push(ObjGroup {
                            material: name,
                            ..Default::default()
                        });
                        model.groups.len() - 1
                    }
                };
            }
            Some("f") => {
                let lens = (v.len(), vt.len(), vn.len());
                let corners = words
                    .map(|w| corner(n, w, lens))
                    .collect::<Result<Vec<Corner>, ObjError>>()?;
                if corners.len() < 3 {
                    return Err(parse_error(n, "a face needs three corners"));
                }
                let group = &mut model.groups[current];
                for k in 1..corners.len() - 1 {
                    let tri = [corners[0], corners[k], corners[k + 1]];
                    push_triangle(group, tri, (&v, &vt, &vn));
                }
            }
            _ => {}
        }
    }
    model.groups.retain(|g| !g.positions.is_empty());
    if model.groups.is_empty() {
        return Err(ObjError::Empty);
    }
    Ok(model)
}

type Lists<'a> = (&'a [[f32; 3]], &'a [[f32; 2]], &'a [[f32; 3]]);

fn push_triangle(group: &mut ObjGroup, tri: [Corner; 3], (v, vt, vn): Lists) {
    let p = tri.map(|(i, _, _)| v[i]);
    let face = {
        let [a, b, c] = p;
        let (e1, e2) = (sub(b, a), sub(c, a));
        let n = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if len > 0.0 {
            n.map(|c| c / len)
        } else {
            [0.0, 1.0, 0.0]
        }
    };
    group.positions.extend(p);
    group
        .normals
        .extend(tri.map(|(_, _, n)| n.map_or(face, |n| vn[n])));
    if tri.iter().all(|(_, t, _)| t.is_some()) {
        let uvs = group.uvs.get_or_insert_with(Vec::new);
        uvs.extend(tri.map(|(_, t, _)| t.map_or([0.0; 2], |t| vt[t])));
    } else if let Some(uvs) = &mut group.uvs {
        uvs.extend([[0.0; 2]; 3]);
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

impl ObjModel {
    /// Triangles in every group.
    pub fn triangles(&self) -> usize {
        self.groups.iter().map(|g| g.positions.len() / 3).sum()
    }
}

/// Reads the OBJ file at `path` and the `mtllib` beside it. A missing or
/// broken MTL leaves the colours empty rather than failing the model.
pub fn load(path: &Path) -> Result<(ObjModel, MaterialColors), ObjError> {
    let read = |p: &Path| {
        std::fs::read_to_string(p).map_err(|e| ObjError::Io {
            path: p.display().to_string(),
            error: e.to_string(),
        })
    };
    let model = parse_obj(&read(path)?)?;
    let colors = model
        .mtllib
        .as_ref()
        .and_then(|lib| read(&path.with_file_name(lib)).ok())
        .and_then(|text| parse_mtl(&text).ok())
        .unwrap_or_default();
    Ok((model, colors))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SQUARE: &str = "\
mtllib square.mtl
v 0 0 0
v 1 0 0
v 1 0 1
v 0 0 1
v 0.5 1 0.5
vn 0 1 0
vn 0 0 1
usemtl Green
f 1//1 2//1 3//1 4//1
usemtl Wood
f 1//2 2//2 5//2
";

    #[test]
    fn quads_and_triangles_are_triangulated_per_material() {
        let model = parse_obj(SQUARE).unwrap();
        assert_eq!(model.mtllib.as_deref(), Some("square.mtl"));
        assert_eq!(model.triangles(), 3);
        let green = &model.groups[0];
        assert_eq!(green.material, "Green");
        assert_eq!(green.positions.len(), 6);
        // Fan from the first corner: (1 2 3) and (1 3 4).
        assert_eq!(
            green.positions,
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 0.0, 1.0],
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
            ]
        );
        assert!(green.normals.iter().all(|n| *n == [0.0, 1.0, 0.0]));
        assert_eq!(model.groups[1].material, "Wood");
        assert_eq!(model.groups[1].normals[0], [0.0, 0.0, 1.0]);
    }

    #[test]
    fn n_gons_fan_and_uvs_and_missing_normals_are_handled() {
        let text = "\
v 0 0 0
v 1 0 0
v 2 0 1
v 1 0 2
v 0 0 1
vt 0 0
vt 1 0
vt 1 1
f 1/1 2/2 3/3 4/3 5/1
f -3/1/ -2/2/ -1/3/
";
        let model = parse_obj(text).unwrap();
        assert_eq!(model.triangles(), 4);
        let group = &model.groups[0];
        let uvs = group.uvs.as_ref().unwrap();
        assert_eq!(uvs.len(), 12);
        assert_eq!(uvs[1], [1.0, 0.0]);
        // No normals given: the face normal faces up for a counter-clockwise
        // face seen from above (+Y).
        for n in &group.normals {
            assert!((n[1].abs() - 1.0).abs() < 1e-5, "{n:?}");
        }
        // Negative indices count back from the last vertex.
        assert_eq!(group.positions[9], [2.0, 0.0, 1.0]);
    }

    #[test]
    fn mtl_colours_come_through() {
        let mtl = "\
# comment
newmtl Green
Ns 96
Kd 0.069954 0.121857 0.047888
newmtl Wood
Kd 0.12 0.05 0.04
";
        let colors = parse_mtl(mtl).unwrap();
        assert_eq!(colors.len(), 2);
        assert_eq!(colors["Green"], [0.069954, 0.121857, 0.047888]);
        assert_eq!(colors["Wood"], [0.12, 0.05, 0.04]);
    }

    #[test]
    fn every_shipped_model_loads_with_a_sane_triangle_count() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/models/vegetation/quaternius-ultimate-nature");
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| e == "obj"))
            .collect();
        files.sort();
        assert_eq!(files.len(), 28);
        for file in files {
            let (model, colors) = load(&file).unwrap();
            let tris = model.triangles();
            println!("{}: {tris} triangles", file.display());
            assert!((50..=3000).contains(&tris), "{}: {tris}", file.display());
            for group in &model.groups {
                assert!(colors.contains_key(&group.material), "{}", group.material);
                assert_eq!(group.normals.len(), group.positions.len());
            }
        }
    }

    #[test]
    fn malformed_input_is_an_error_not_a_panic() {
        for bad in [
            "v 0 0\nf 1 1 1\n",
            "v 0 0 0\nv 1 0 0\nf 1 2\n",
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 9\n",
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 0\n",
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1//4 2//4 3//4\n",
            "v a b c\n",
            "v 0 0 0\nf x y z\n",
            "",
            "# only a comment\n",
        ] {
            assert!(parse_obj(bad).is_err(), "{bad:?}");
        }
        assert!(parse_mtl("newmtl A\nKd 1 x 0\n").is_err());
        assert!(parse_mtl("Kd 1 1 1\n").is_err(), "Kd before newmtl");
        assert!(load(Path::new("/does/not/exist.obj")).is_err());
    }
}
