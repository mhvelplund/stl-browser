//! Load triangles from STL (binary or ASCII) and OBJ files.

use std::io::{BufReader, Cursor};

/// One triangle: three vertices.
pub type Tri = [[f32; 3]; 3];

/// Triangles of an `.stl` or `.obj` file (by the extension of `name`), without non-finite ones.
pub fn load_mesh(name: &str, bytes: &[u8]) -> Result<Vec<Tri>, String> {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    let tris = match ext.as_str() {
        "stl" => load_stl(bytes)?,
        "obj" => load_obj(bytes)?,
        _ => return Err(format!("not a mesh: {name}")),
    };
    let tris: Vec<Tri> = tris.into_iter().filter(|t| t.iter().flatten().all(|c| c.is_finite())).collect();
    if tris.is_empty() {
        return Err("no triangles".into());
    }
    Ok(tris)
}

/// Streams the triangles (no vertex dedup). A file that breaks off keeps what was read before.
fn load_stl(bytes: &[u8]) -> Result<Vec<Tri>, String> {
    let mut cur = Cursor::new(bytes);
    let reader = stl_io::create_stl_reader(&mut cur).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for t in reader {
        match t {
            Ok(t) => out.push(t.vertices.map(|v| v.0)),
            Err(e) if out.is_empty() => return Err(e.to_string()),
            Err(_) => break,
        }
    }
    Ok(out)
}

fn load_obj(bytes: &[u8]) -> Result<Vec<Tri>, String> {
    let opts = tobj::LoadOptions { triangulate: true, single_index: true, ..Default::default() };
    let (models, _materials) = tobj::load_obj_buf(&mut BufReader::new(bytes), &opts, |_| Err(tobj::LoadError::OpenFileFailed))
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for m in models {
        let (p, idx) = (&m.mesh.positions, &m.mesh.indices);
        let v = |i: u32| -> Option<[f32; 3]> {
            let i = i as usize * 3;
            Some([*p.get(i)?, *p.get(i + 1)?, *p.get(i + 2)?])
        };
        for t in idx.chunks_exact(3) {
            if let (Some(a), Some(b), Some(c)) = (v(t[0]), v(t[1]), v(t[2])) {
                out.push([a, b, c]);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{cube, stl_bytes};

    #[test]
    fn loads_binary_and_ascii_stl_and_obj() {
        assert_eq!(load_mesh("a.stl", &stl_bytes(&cube(1.0))).unwrap().len(), 12);
        let ascii = b"solid t\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid t\n";
        assert_eq!(load_mesh("a.STL", ascii).unwrap().len(), 1);
        let obj = b"v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1 2 3 4\n";
        assert_eq!(load_mesh("q.obj", obj).unwrap().len(), 2);
        assert!(load_mesh("e.obj", b"v 0 0 0\n").is_err());
        assert!(load_mesh("j.stl", b"\x00\x01junk").is_err());
    }

    #[test]
    fn non_finite_vertices_are_dropped() {
        let mut tris = cube(1.0);
        tris[0][1] = [f32::NAN, 0.0, 0.0];
        assert_eq!(load_mesh("a.stl", &stl_bytes(&tris)).unwrap().len(), 11);
    }
}
