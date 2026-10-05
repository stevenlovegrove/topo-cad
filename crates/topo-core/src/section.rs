//! Cross-sections ("profiles"). Coordinates are in the member's `(u, v)` plane,
//! where `v` is the depth direction and `u = v × x` (see `DESIGN.md` §2).

use crate::ids::SectionId;
use crate::math::{convex_hull, polygon_area, v2, BBox2, Vec2};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Shape {
    /// Solid rectangle, `b` along u (width/thickness), `d` along v (depth).
    Rect { b: f64, d: f64 },
    /// Arbitrary polygon with holes; any orientation, in (u, v) coordinates.
    Polygon { outer: Vec<Vec2>, holes: Vec<Vec<Vec2>> },
    /// Round tube of outside diameter `d` and wall thickness `wall`; a solid
    /// rod if `wall` is zero or at least `d / 2`. Section properties are
    /// exact; the outline is a polygon of [`ROUND_FACETS`] sides.
    Round { d: f64, wall: f64 },
}

/// Sides of the polygon a round section is drawn and cut as.
pub const ROUND_FACETS: usize = 24;

fn circle(r: f64) -> Vec<Vec2> {
    (0..ROUND_FACETS)
        .map(|k| {
            let a = std::f64::consts::TAU * k as f64 / ROUND_FACETS as f64;
            v2(r * a.cos(), r * a.sin())
        })
        .collect()
}

impl Shape {
    fn outer(&self) -> Vec<Vec2> {
        match self {
            Shape::Rect { b, d } => {
                let (hb, hd) = (b / 2.0, d / 2.0);
                vec![v2(-hb, -hd), v2(hb, -hd), v2(hb, hd), v2(-hb, hd)]
            }
            Shape::Polygon { outer, .. } => ccw(outer.clone()),
            Shape::Round { d, .. } => circle(d / 2.0),
        }
    }
    fn holes(&self) -> Vec<Vec<Vec2>> {
        match self {
            Shape::Rect { .. } => vec![],
            Shape::Polygon { holes, .. } => holes.iter().map(|h| ccw(h.clone())).collect(),
            Shape::Round { .. } => match self.bore() {
                Some(r) => vec![circle(r)],
                None => vec![],
            },
        }
    }
    fn bbox(&self) -> BBox2 {
        BBox2::from_points(self.outer())
    }
    /// Inside radius of a round tube (`None` for a solid rod or other shapes).
    fn bore(&self) -> Option<f64> {
        match self {
            Shape::Round { d, wall } if *wall > 0.0 && *wall < d / 2.0 => Some(d / 2.0 - wall),
            _ => None,
        }
    }
}

fn ccw(mut pts: Vec<Vec2>) -> Vec<Vec2> {
    if polygon_area(&pts) < 0.0 {
        pts.reverse();
    }
    pts
}

/// One ply's outline in section coordinates (outer CCW, holes CCW).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlyOutline {
    pub outer: Vec<Vec2>,
    pub holes: Vec<Vec<Vec2>>,
}

/// Geometric section properties of the full (built-up) section.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SectionProps {
    /// Area (m²).
    pub area: f64,
    /// Centroid in section coordinates.
    pub centroid: Vec2,
    /// ∫v² dA about the centroidal u-axis (bending in the depth direction), m⁴.
    pub i_u: f64,
    /// ∫u² dA about the centroidal v-axis, m⁴.
    pub i_v: f64,
    /// St-Venant torsion constant (sum over plies; approximate for polygons), m⁴.
    pub j: f64,
    /// Elastic section moduli (m³).
    pub s_u: f64,
    pub s_v: f64,
    /// Overall extents.
    pub width: f64,
    pub depth: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Section {
    pub id: SectionId,
    /// Display name, e.g. `2x10`, `(2) 2x10`, `2020 T-slot`.
    pub name: String,
    pub shape: Shape,
    /// Number of identical plies stacked along `u` (built-up members). ≥ 1.
    pub plies: u32,
    /// Clear gap between plies (e.g. a ½" spacer in a header).
    pub ply_gap: f64,
    pub props: SectionProps,
    /// Free-form metadata, e.g. `nominal = "2x10"`, `family = "sawn"`.
    pub tags: BTreeMap<String, String>,
}

impl Section {
    pub fn new(name: impl Into<String>, shape: Shape) -> Section {
        Section::built_up(name, shape, 1, 0.0)
    }

    pub fn rect(name: impl Into<String>, b: f64, d: f64) -> Section {
        Section::new(name, Shape::Rect { b, d })
    }

    pub fn built_up(name: impl Into<String>, shape: Shape, plies: u32, ply_gap: f64) -> Section {
        assert!(plies >= 1, "a section needs at least one ply");
        let mut s = Section {
            id: SectionId(u32::MAX),
            name: name.into(),
            shape,
            plies,
            ply_gap,
            props: SectionProps::default(),
            tags: BTreeMap::new(),
        };
        s.props = s.compute_props();
        s
    }

    /// Round tube (or solid rod, with `wall` zero), e.g. `round("28.6 x 0.8", …)`.
    pub fn round(name: impl Into<String>, d: f64, wall: f64) -> Section {
        Section::new(name, Shape::Round { d, wall })
    }

    /// Rectangular hollow section `b` × `d` of wall thickness `wall`.
    pub fn rect_tube(name: impl Into<String>, b: f64, d: f64, wall: f64) -> Section {
        let ring = |hb: f64, hd: f64| vec![v2(-hb, -hd), v2(hb, -hd), v2(hb, hd), v2(-hb, hd)];
        let mut s = Section::new(name, Shape::Polygon { outer: ring(b / 2.0, d / 2.0), holes: vec![ring(b / 2.0 - wall, d / 2.0 - wall)] });
        // Closed thin-walled section (Bredt): J = 4 A_m² t / p_m, on the mid-line.
        let (bm, dm) = (b - wall, d - wall);
        s.props.j = 2.0 * wall * bm * bm * dm * dm / (bm + dm);
        s
    }

    pub fn with_tag(mut self, k: &str, v: &str) -> Section {
        self.tags.insert(k.into(), v.into());
        self
    }

    /// Replaces computed properties with published ones (a maker's catalog
    /// values for a profile whose outline is drawn only approximately).
    /// Extents and the centroid are kept.
    pub fn with_catalog_props(mut self, area: f64, i_u: f64, i_v: f64, j: Option<f64>) -> Section {
        let p = &mut self.props;
        (p.s_u, p.s_v) = (p.s_u * i_u / p.i_u, p.s_v * i_v / p.i_v);
        (p.area, p.i_u, p.i_v) = (area, i_u, i_v);
        if let Some(j) = j {
            p.j = j;
        }
        self
    }

    /// Whether the side surface is curved (drawn without facet edges).
    pub fn is_round(&self) -> bool {
        matches!(self.shape, Shape::Round { .. })
    }

    /// Outside diameter and wall of a round section (`wall` zero for a rod).
    pub fn round_dims(&self) -> Option<(f64, f64)> {
        match self.shape {
            Shape::Round { d, wall } => Some((d, if self.shape.bore().is_some() { wall } else { 0.0 })),
            _ => None,
        }
    }

    /// Ply outlines, laid out along `u` and centred on the section's bounding box.
    pub fn ply_outlines(&self) -> Vec<PlyOutline> {
        let bb = self.shape.bbox();
        let w = bb.width();
        let total = w * self.plies as f64 + self.ply_gap * (self.plies - 1) as f64;
        let c = bb.center();
        (0..self.plies)
            .map(|k| {
                let du = -total / 2.0 + w / 2.0 + k as f64 * (w + self.ply_gap) - c.x;
                let shift = |p: &Vec2| v2(p.x + du, p.y - c.y);
                PlyOutline {
                    outer: self.shape.outer().iter().map(shift).collect(),
                    holes: self.shape.holes().iter().map(|h| h.iter().map(shift).collect()).collect(),
                }
            })
            .collect()
    }

    /// Bounding box of the full section in section coordinates (centred at origin).
    pub fn bbox(&self) -> BBox2 {
        BBox2::from_points(self.ply_outlines().iter().flat_map(|p| p.outer.clone()))
    }

    /// Convex envelope of all plies (used for butt-joint ray casting and clashes).
    pub fn hull(&self) -> Vec<Vec2> {
        let pts: Vec<Vec2> = self.ply_outlines().into_iter().flat_map(|p| p.outer).collect();
        convex_hull(&pts)
    }

    fn compute_props(&self) -> SectionProps {
        if let Shape::Round { d, .. } = self.shape {
            // Exact for the circle (the outline polygon is only for drawing).
            let (ro, ri) = (d / 2.0, self.shape.bore().unwrap_or(0.0));
            let n = self.plies as f64;
            let area = std::f64::consts::PI * (ro * ro - ri * ri);
            let i = std::f64::consts::PI * (ro.powi(4) - ri.powi(4)) / 4.0;
            let bb = self.bbox();
            // Plies side by side along u: parallel-axis terms about the group's centre.
            let pitch = d + self.ply_gap;
            let offsets: f64 = (0..self.plies).map(|k| (k as f64 - (n - 1.0) / 2.0) * pitch).map(|o| o * o).sum();
            let i_v = n * i + area * offsets;
            return SectionProps {
                area: n * area,
                centroid: v2(0.0, 0.0),
                i_u: n * i,
                i_v,
                j: n * 2.0 * i,
                s_u: n * i / ro,
                s_v: i_v / (bb.width() / 2.0),
                width: bb.width(),
                depth: bb.height(),
            };
        }
        // Integrals about the section origin, accumulated over all plies and holes.
        let (mut a, mut qu, mut qv, mut iuu, mut ivv) = (0.0, 0.0, 0.0, 0.0, 0.0);
        let mut acc = |poly: &[Vec2], sign: f64| {
            let n = poly.len();
            for i in 0..n {
                let (p, q) = (poly[i], poly[(i + 1) % n]);
                let c = p.cross(q) * sign;
                a += c / 2.0;
                qu += (p.x + q.x) * c / 6.0;
                qv += (p.y + q.y) * c / 6.0;
                ivv += (p.x * p.x + p.x * q.x + q.x * q.x) * c / 12.0;
                iuu += (p.y * p.y + p.y * q.y + q.y * q.y) * c / 12.0;
            }
        };
        let plies = self.ply_outlines();
        for ply in &plies {
            acc(&ply.outer, 1.0);
            for h in &ply.holes {
                acc(h, -1.0);
            }
        }
        let centroid = v2(qu / a, qv / a);
        let i_u = iuu - a * centroid.y * centroid.y;
        let i_v = ivv - a * centroid.x * centroid.x;
        let bb = self.bbox();
        let j_ply = match &self.shape {
            Shape::Rect { b, d } => rect_torsion(*b, *d),
            Shape::Polygon { .. } => {
                // Saint-Venant approximation for compact solid sections: J ≈ A⁴ / (40 Ip).
                let ap = a / self.plies as f64;
                let ip = (i_u + i_v) / self.plies as f64;
                ap.powi(4) / (40.0 * ip)
            }
            Shape::Round { .. } => unreachable!("handled above"),
        };
        let c_v = (bb.max.y - centroid.y).max(centroid.y - bb.min.y);
        let c_u = (bb.max.x - centroid.x).max(centroid.x - bb.min.x);
        SectionProps {
            area: a,
            centroid,
            i_u,
            i_v,
            j: j_ply * self.plies as f64,
            s_u: i_u / c_v,
            s_v: i_v / c_u,
            width: bb.width(),
            depth: bb.height(),
        }
    }
}

/// Torsion constant of a solid rectangle (Roark approximation).
fn rect_torsion(b: f64, d: f64) -> f64 {
    let (a, t) = if b >= d { (b, d) } else { (d, b) };
    a * t.powi(3) * (1.0 / 3.0 - 0.21 * (t / a) * (1.0 - t.powi(4) / (12.0 * a.powi(4))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_props() {
        let s = Section::rect("r", 0.04, 0.2);
        assert!((s.props.area - 0.008).abs() < 1e-12);
        assert!((s.props.i_u - 0.04 * 0.2f64.powi(3) / 12.0).abs() < 1e-12);
        assert!((s.props.i_v - 0.2 * 0.04f64.powi(3) / 12.0).abs() < 1e-12);
        assert!((s.props.s_u - 0.04 * 0.2 * 0.2 / 6.0).abs() < 1e-12);
    }

    #[test]
    fn two_ply_props() {
        let s = Section::built_up("2ply", Shape::Rect { b: 1.0, d: 2.0 }, 2, 0.0);
        assert!((s.props.area - 4.0).abs() < 1e-12);
        assert!((s.props.width - 2.0).abs() < 1e-12);
        // Strong axis doubles; weak axis equals a solid 2×2 square.
        assert!((s.props.i_u - 2.0 * 1.0 * 8.0 / 12.0).abs() < 1e-12);
        assert!((s.props.i_v - 2.0 * 8.0 / 12.0).abs() < 1e-12);
        assert_eq!(s.ply_outlines().len(), 2);
    }

    #[test]
    fn round_tube_props_are_exact() {
        use std::f64::consts::PI;
        // 28.6 x 0.8 mm tube.
        let s = Section::round("t", 0.0286, 0.0008);
        let (ro, ri) = (0.0143f64, 0.0135f64);
        assert!((s.props.area - PI * (ro * ro - ri * ri)).abs() < 1e-15);
        let i = PI * (ro.powi(4) - ri.powi(4)) / 4.0;
        assert!((s.props.i_u - i).abs() < 1e-18 && (s.props.i_v - i).abs() < 1e-18);
        assert!((s.props.j - 2.0 * i).abs() < 1e-18);
        assert!((s.props.width - 0.0286).abs() < 1e-12 && (s.props.depth - 0.0286).abs() < 1e-12);
        assert_eq!(s.ply_outlines()[0].holes.len(), 1);
        assert_eq!(s.round_dims(), Some((0.0286, 0.0008)));
        // A rod has no bore.
        let r = Section::round("r", 0.01, 0.0);
        assert!(r.ply_outlines()[0].holes.is_empty());
        assert!((r.props.area - PI * 0.005f64.powi(2)).abs() < 1e-15);
    }

    #[test]
    fn rect_tube_torsion_is_bredt() {
        // Square 40 x 40 x 2: J = t·b_m³ = 2e-3 · 0.038³.
        let s = Section::rect_tube("q", 0.04, 0.04, 0.002);
        assert!((s.props.j - 0.002 * 0.038f64.powi(3)).abs() < 1e-15);
        assert!((s.props.area - (0.04f64.powi(2) - 0.036f64.powi(2))).abs() < 1e-12);
    }

    #[test]
    fn polygon_with_hole_matches_rect_difference() {
        let sq = |h: f64| vec![v2(-h, -h), v2(h, -h), v2(h, h), v2(-h, h)];
        let s = Section::new("tube", Shape::Polygon { outer: sq(1.0), holes: vec![sq(0.5)] });
        assert!((s.props.area - (4.0 - 1.0)).abs() < 1e-12);
        assert!((s.props.i_u - (16.0 / 12.0 - 1.0 / 12.0)).abs() < 1e-12);
    }
}
