//! Solids: what the framing stands on (a slab, a curb, a stem wall, a footing, a pier). Each is
//! a plan outline between two heights. They are not members: they have no axis, take no part in
//! junctions, and are not analysed; they are there to be stood on, drawn, and seen in a scan.

use crate::ids::GroupId;
use crate::math::{polygon_area, Vec2, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Solid {
    /// Its name within its group (`Slab`, `curb`).
    pub name: String,
    /// `slab`, `curb`, …
    pub role: String,
    /// What it is of (`concrete`).
    pub material: String,
    pub group: Option<GroupId>,
    /// Its outline on plan (world x, y), either way round.
    pub outline: Vec<Vec2>,
    /// Its underside and its top (world z).
    pub bottom: f64,
    pub top: f64,
}

/// A face of a solid: its outline (anticlockwise seen from outside) and outward normal.
#[derive(Clone, Debug, PartialEq)]
pub struct SolidFace {
    pub outer: Vec<Vec3>,
    pub normal: Vec3,
}

impl Solid {
    /// Its faces: top, underside, and a side for each edge of its outline.
    pub fn faces(&self) -> Vec<SolidFace> {
        let mut o = self.outline.clone();
        if polygon_area(&o) < 0.0 {
            o.reverse();
        }
        let at = |p: Vec2, z: f64| Vec3::new(p.x, p.y, z);
        let mut faces = vec![
            SolidFace { outer: o.iter().map(|&p| at(p, self.top)).collect(), normal: Vec3::Z },
            SolidFace { outer: o.iter().rev().map(|&p| at(p, self.bottom)).collect(), normal: -Vec3::Z },
        ];
        for (k, &a) in o.iter().enumerate() {
            let b = o[(k + 1) % o.len()];
            let d = b - a;
            let Some(n) = Vec3::new(d.y, -d.x, 0.0).try_normalized() else { continue };
            faces.push(SolidFace { outer: vec![at(a, self.bottom), at(b, self.bottom), at(b, self.top), at(a, self.top)], normal: n });
        }
        faces
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_solids_faces_face_outward() {
        let s = Solid { name: "Slab".into(), role: "slab".into(), material: "concrete".into(), group: None, outline: vec![Vec2::new(0.0, 0.0), Vec2::new(0.0, 2.0), Vec2::new(3.0, 2.0), Vec2::new(3.0, 0.0)], bottom: -0.1, top: 0.0 };
        let faces = s.faces();
        assert_eq!(faces.len(), 6);
        let centre = Vec3::new(1.5, 1.0, -0.05);
        for f in &faces {
            let c = f.outer.iter().fold(Vec3::ZERO, |a, p| a + *p) / f.outer.len() as f64;
            assert!((c - centre).dot(f.normal) > 0.0, "{f:?}");
            // (Anticlockwise seen from outside: its own normal is the one given.)
            let n = (f.outer[1] - f.outer[0]).cross(f.outer[2] - f.outer[1]);
            assert!(n.dot(f.normal) > 0.0, "{f:?}");
        }
    }
}
