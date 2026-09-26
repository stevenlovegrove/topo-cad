//! Triangle meshes and OBJ export.

use crate::Geometry;
use serde::{Deserialize, Serialize};
use std::fmt::Write;
use topo_core::{Model, Vec2, Vec3};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Mesh {
    pub positions: Vec<Vec3>,
    pub triangles: Vec<[u32; 3]>,
    /// Member id per triangle.
    pub member: Vec<u32>,
}

/// Ear-clipping triangulation of a simple polygon (any orientation).
pub fn triangulate(poly: &[Vec2]) -> Vec<[usize; 3]> {
    let n = poly.len();
    if n < 3 {
        return vec![];
    }
    let ccw = topo_core::polygon_area(poly) > 0.0;
    let mut idx: Vec<usize> = if ccw { (0..n).collect() } else { (0..n).rev().collect() };
    let mut out = vec![];
    let mut guard = 0;
    while idx.len() > 3 && guard < 10 * n {
        guard += 1;
        let m = idx.len();
        let mut clipped = false;
        for k in 0..m {
            let (a, b, c) = (idx[(k + m - 1) % m], idx[k], idx[(k + 1) % m]);
            let (pa, pb, pc) = (poly[a], poly[b], poly[c]);
            if (pb - pa).cross(pc - pb) <= 1e-15 {
                continue; // reflex or degenerate
            }
            let inside = idx.iter().any(|&q| {
                if q == a || q == b || q == c {
                    return false;
                }
                let p = poly[q];
                (pb - pa).cross(p - pa) >= 0.0 && (pc - pb).cross(p - pb) >= 0.0 && (pa - pc).cross(p - pc) >= 0.0
            });
            if !inside {
                out.push([a, b, c]);
                idx.remove(k);
                clipped = true;
                break;
            }
        }
        if !clipped {
            break;
        }
    }
    if idx.len() == 3 {
        out.push([idx[0], idx[1], idx[2]]);
    }
    out
}

impl Mesh {
    pub fn from_geometry(geom: &Geometry) -> Mesh {
        let mut mesh = Mesh::default();
        for g in &geom.members {
            for f in g.faces() {
                // Project onto the face plane for triangulation (holes are not bridged).
                let n = f.normal;
                let a = n.any_perpendicular();
                let b = n.cross(a);
                let pts: Vec<Vec2> = f.outer.iter().map(|p| Vec2::new(p.dot(a), p.dot(b))).collect();
                let base = mesh.positions.len() as u32;
                mesh.positions.extend(f.outer.iter().copied());
                for t in triangulate(&pts) {
                    // Keep outward winding.
                    let (p0, p1, p2) = (f.outer[t[0]], f.outer[t[1]], f.outer[t[2]]);
                    let tri = if (p1 - p0).cross(p2 - p0).dot(n) >= 0.0 { t } else { [t[0], t[2], t[1]] };
                    mesh.triangles.push([base + tri[0] as u32, base + tri[1] as u32, base + tri[2] as u32]);
                    mesh.member.push(g.member.0);
                }
            }
        }
        mesh
    }
}

/// Wavefront OBJ with one object per member (named by id and role), in model units scaled by `unit`.
pub fn to_obj(model: &Model, geom: &Geometry, unit: f64) -> String {
    let mut s = String::from("# topo-cad OBJ export\n");
    let mut base = 1usize;
    for g in &geom.members {
        let m = model.member(g.member);
        let _ = writeln!(s, "o {}_{}", m.id, m.role);
        let faces = g.faces();
        for f in &faces {
            for p in &f.outer {
                let _ = writeln!(s, "v {:.6} {:.6} {:.6}", p.x / unit, p.y / unit, p.z / unit);
            }
        }
        for f in &faces {
            let idx: Vec<String> = (0..f.outer.len()).map(|k| (base + k).to_string()).collect();
            let _ = writeln!(s, "f {}", idx.join(" "));
            base += f.outer.len();
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triangulates_concave_polygon() {
        // L-shape, area 3.
        let l = [
            Vec2::new(0., 0.),
            Vec2::new(2., 0.),
            Vec2::new(2., 1.),
            Vec2::new(1., 1.),
            Vec2::new(1., 2.),
            Vec2::new(0., 2.),
        ];
        let tris = triangulate(&l);
        assert_eq!(tris.len(), 4);
        let area: f64 = tris.iter().map(|t| 0.5 * (l[t[1]] - l[t[0]]).cross(l[t[2]] - l[t[0]]).abs()).sum();
        assert!((area - 3.0).abs() < 1e-12);
    }
}
