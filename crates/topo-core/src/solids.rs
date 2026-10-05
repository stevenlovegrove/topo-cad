//! Solids: what the framing stands on (a slab, a curb, a stem wall, a footing, a pier). Each is
//! a plan outline between two surfaces, each level or laid to a slope (a slab laid to fall; a
//! curb on it whose top is level). They are not members: they have no axis, take no part in
//! junctions, and are not analysed; they are there to be stood on, drawn, and seen in a scan.

use crate::ids::GroupId;
use crate::math::{polygon_area, Vec2, Vec3};
use serde::{Deserialize, Serialize};

/// A flat surface over the plan: so high at one place, and rising from there by so much per
/// metre along x and along y (level: by nothing).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Level {
    /// Its height at `at`.
    pub z: f64,
    pub at: Vec2,
    /// Its rise per metre along x and along y.
    pub slope: Vec2,
}

impl Level {
    /// Level, at this height.
    pub fn flat(z: f64) -> Level {
        Level { z, at: Vec2::new(0.0, 0.0), slope: Vec2::new(0.0, 0.0) }
    }
    /// How high it is over a place on plan.
    pub fn over(&self, p: Vec2) -> f64 {
        self.z + self.slope.x * (p.x - self.at.x) + self.slope.y * (p.y - self.at.y)
    }
    /// The same surface, so much higher.
    pub fn raised(&self, by: f64) -> Level {
        Level { z: self.z + by, ..*self }
    }
    /// Straight out of it, upward.
    pub fn normal(&self) -> Vec3 {
        Vec3::new(-self.slope.x, -self.slope.y, 1.0).normalized()
    }
}

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
    /// Its underside and its top.
    pub bottom: Level,
    pub top: Level,
    /// Edges of its outline (edge k from point k to the next) where it only abuts another piece
    /// of the same sheet (a roof's deck, cut into convex pieces): no edge of the thing is there,
    /// and no face.
    #[serde(default)]
    pub seams: Vec<usize>,
}

/// A face of a solid: its outline (anticlockwise seen from outside) and outward normal.
#[derive(Clone, Debug, PartialEq)]
pub struct SolidFace {
    pub outer: Vec<Vec3>,
    pub normal: Vec3,
}

impl Solid {
    /// Its seams: the two ends of each, on plan.
    pub fn seam_lines(&self) -> Vec<(Vec2, Vec2)> {
        let n = self.outline.len();
        self.seams.iter().filter(|&&k| k < n).map(|&k| (self.outline[k], self.outline[(k + 1) % n])).collect()
    }

    /// Its faces: top, underside, and a side for each edge of its outline (but a seam).
    pub fn faces(&self) -> Vec<SolidFace> {
        let seams = self.seam_lines();
        let mut o = self.outline.clone();
        if polygon_area(&o) < 0.0 {
            o.reverse();
        }
        let at = |p: Vec2, l: &Level| Vec3::new(p.x, p.y, l.over(p));
        let mut faces = vec![
            SolidFace { outer: o.iter().map(|&p| at(p, &self.top)).collect(), normal: self.top.normal() },
            SolidFace { outer: o.iter().rev().map(|&p| at(p, &self.bottom)).collect(), normal: -self.bottom.normal() },
        ];
        for (k, &a) in o.iter().enumerate() {
            let b = o[(k + 1) % o.len()];
            let d = b - a;
            if seams.iter().any(|&(p, q)| (p == a && q == b) || (p == b && q == a)) {
                continue;
            }
            let Some(n) = Vec3::new(d.y, -d.x, 0.0).try_normalized() else { continue };
            faces.push(SolidFace { outer: vec![at(a, &self.bottom), at(b, &self.bottom), at(b, &self.top), at(a, &self.top)], normal: n });
        }
        faces
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_solids_faces_face_outward() {
        let s = Solid { name: "Slab".into(), role: "slab".into(), material: "concrete".into(), group: None, outline: vec![Vec2::new(0.0, 0.0), Vec2::new(0.0, 2.0), Vec2::new(3.0, 2.0), Vec2::new(3.0, 0.0)], bottom: Level::flat(-0.1), top: Level { z: 0.0, at: Vec2::new(0.0, 0.0), slope: Vec2::new(0.01, 0.0) }, seams: vec![] };
        let faces = s.faces();
        assert_eq!(faces.len(), 6);
        // (Its top is laid to a slope: 30 mm higher along x at its far side, and its top face faces square to it.)
        assert!((s.top.over(Vec2::new(3.0, 1.0)) - 0.03).abs() < 1e-12);
        assert!(faces[0].outer.iter().all(|p| (p.z - 0.01 * p.x).abs() < 1e-12) && faces[0].normal.x < -0.009);
        let centre = Vec3::new(1.5, 1.0, -0.04);
        for f in &faces {
            let c = f.outer.iter().fold(Vec3::ZERO, |a, p| a + *p) / f.outer.len() as f64;
            assert!((c - centre).dot(f.normal) > 0.0, "{f:?}");
            // (Anticlockwise seen from outside: its own normal is the one given.)
            let n = (f.outer[1] - f.outer[0]).cross(f.outer[2] - f.outer[1]);
            assert!(n.dot(f.normal) > 0.0, "{f:?}");
        }
    }
}
