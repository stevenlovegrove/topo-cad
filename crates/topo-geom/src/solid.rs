//! Member solids: section prism intersected with the end half-spaces, per ply.
//! Each end has a primary cut plane plus optional extra planes for compound
//! cuts (e.g. a web fitted under both top chords at a truss apex).

use serde::{Deserialize, Serialize};
use topo_core::{convex_hull, BBox3, MemberAxes, MemberId, Plane, PlyOutline, Vec2, Vec3};

/// How an end cut was decided (kept for traceability in schedules/reports).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CutKind {
    /// Square cut at the node (free end, splice, or nothing to butt against).
    Square,
    /// Cut on a face of `against` (T-junction, or the lower-ranked member of an L).
    Butt { against: MemberId },
    /// Extended through the envelope of lower-ranked members (higher-ranked member of an L).
    Through { past: Vec<MemberId> },
    /// Mitred with `with`.
    Miter { with: MemberId },
    /// An explicit cut plane (plumb tail, bevel…) from a joint rule.
    Custom,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EndCut {
    /// Primary clip plane; the normal points away from the member body.
    pub plane: Plane,
    /// Additional clip planes for compound cuts.
    #[serde(default)]
    pub extra: Vec<Plane>,
    /// Members the extra planes belong to (a compound butt bears on all of them).
    #[serde(default)]
    pub also: Vec<MemberId>,
    pub kind: CutKind,
}

impl EndCut {
    pub fn planes(&self) -> impl Iterator<Item = &Plane> {
        std::iter::once(&self.plane).chain(self.extra.iter())
    }
}

/// Sutherland–Hodgman clip of a planar polygon, keeping `signed_distance ≤ 0`.
fn clip(poly: &[Vec3], plane: &Plane) -> Vec<Vec3> {
    let n = poly.len();
    let mut out = Vec::with_capacity(n + 2);
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let (da, db) = (plane.signed_distance(a), plane.signed_distance(b));
        if da <= 0.0 {
            out.push(a);
        }
        if (da < 0.0 && db > 0.0) || (da > 0.0 && db < 0.0) {
            out.push(a.lerp(b, da / (da - db)));
        }
    }
    out
}

fn area3(poly: &[Vec3]) -> f64 {
    let n = poly.len();
    let mut c = Vec3::ZERO;
    for i in 0..n {
        c += poly[i].cross(poly[(i + 1) % n]);
    }
    c.norm() / 2.0
}

/// World-space placement of a member's section, before end cuts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    pub origin: Vec3,
    pub x: Vec3,
    pub u: Vec3,
    pub v: Vec3,
    /// Distance between the member's end nodes.
    pub length: f64,
    /// Ply outlines in section coordinates, shifted so the axis is at (0, 0).
    pub plies: Vec<PlyOutline>,
    /// Convex envelope of all plies (shifted).
    pub hull: Vec<Vec2>,
    /// Section centroid (shifted).
    pub centroid: Vec2,
}

impl Placement {
    pub fn new(axes: &MemberAxes, plies: Vec<PlyOutline>, shift: Vec2, centroid: Vec2) -> Placement {
        let plies: Vec<PlyOutline> = plies
            .into_iter()
            .map(|p| PlyOutline {
                outer: p.outer.iter().map(|&q| q - shift).collect(),
                holes: p.holes.iter().map(|h| h.iter().map(|&q| q - shift).collect()).collect(),
            })
            .collect();
        let all: Vec<Vec2> = plies.iter().flat_map(|p| p.outer.clone()).collect();
        Placement {
            origin: axes.origin,
            x: axes.x,
            u: axes.u,
            v: axes.v,
            length: axes.length,
            hull: convex_hull(&all),
            plies,
            centroid: centroid - shift,
        }
    }

    /// World point for section point `p` at axial parameter `t`.
    pub fn at(&self, p: Vec2, t: f64) -> Vec3 {
        self.origin + self.x * t + self.u * p.x + self.v * p.y
    }

    /// Outward world normal of the side face on section edge `a → b` (CCW polygon).
    pub fn side_normal(&self, a: Vec2, b: Vec2) -> Vec3 {
        let e = b - a;
        (self.u * e.y - self.v * e.x).normalized()
    }

    /// Axial parameter where the line through section point `p` meets `plane`.
    pub fn t_on(&self, p: Vec2, plane: &Plane) -> f64 {
        plane.intersect_line(self.at(p, 0.0), self.x).expect("cut plane parallel to member axis")
    }

    /// Radius of the section envelope about the axis.
    pub fn radius(&self) -> f64 {
        self.hull.iter().map(|p| p.norm()).fold(0.0, f64::max)
    }
}

/// A planar face; `holes` are inner loops (cap faces of hollow profiles).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Face {
    pub outer: Vec<Vec3>,
    pub holes: Vec<Vec<Vec3>>,
    pub normal: Vec3,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemberGeom {
    pub member: MemberId,
    pub place: Placement,
    pub start: EndCut,
    pub end: EndCut,
}

impl MemberGeom {
    fn all_planes(&self) -> Vec<Plane> {
        self.start.planes().chain(self.end.planes()).copied().collect()
    }

    /// Boundary faces of the prism on `outer`/`holes`, clipped by every end plane.
    fn clipped_faces(&self, outer: &[Vec2], holes: &[Vec<Vec2>]) -> Vec<Face> {
        let pl = &self.place;
        let planes = self.all_planes();
        let far = pl.length + 100.0;
        let clip_all = |mut poly: Vec<Vec3>, skip: Option<usize>| {
            for (i, p) in planes.iter().enumerate() {
                if Some(i) != skip && poly.len() >= 3 {
                    poly = clip(&poly, p);
                }
            }
            poly
        };
        let mut out = vec![];
        let side = |ring: &[Vec2], inward: bool, out: &mut Vec<Face>| {
            let n = ring.len();
            for i in 0..n {
                let (a, b) = (ring[i], ring[(i + 1) % n]);
                let quad = vec![pl.at(a, -far), pl.at(b, -far), pl.at(b, far), pl.at(a, far)];
                let poly = clip_all(quad, None);
                if poly.len() >= 3 && area3(&poly) > 1e-12 {
                    let nrm = pl.side_normal(a, b);
                    out.push(Face { outer: poly, holes: vec![], normal: if inward { -nrm } else { nrm } });
                }
            }
        };
        side(outer, false, &mut out);
        for h in holes {
            side(h, true, &mut out);
        }
        for (i, p) in planes.iter().enumerate() {
            let onto = |ring: &[Vec2]| ring.iter().map(|&q| pl.at(q, pl.t_on(q, p))).collect::<Vec<_>>();
            let cap = clip_all(onto(outer), Some(i));
            if cap.len() >= 3 && area3(&cap) > 1e-12 {
                let hs = holes.iter().map(|h| clip_all(onto(h), Some(i))).filter(|h| h.len() >= 3).collect();
                out.push(Face { outer: cap, holes: hs, normal: p.normal });
            }
        }
        out
    }

    /// All boundary faces, over all plies.
    pub fn faces(&self) -> Vec<Face> {
        (0..self.place.plies.len()).flat_map(|k| self.ply_faces(k)).collect()
    }

    /// Boundary faces of ply `k`.
    pub fn ply_faces(&self, k: usize) -> Vec<Face> {
        let ply = &self.place.plies[k];
        self.clipped_faces(&ply.outer, &ply.holes)
    }

    /// Bounding planes (outward normals) of the whole member's convex envelope.
    pub fn envelope_planes(&self) -> Vec<Plane> {
        let h = &self.place.hull;
        let n = h.len();
        (0..n)
            .map(|i| Plane { point: self.place.at(h[i], 0.0), normal: self.place.side_normal(h[i], h[(i + 1) % n]) })
            .chain(self.all_planes())
            .collect()
    }

    /// Convex envelope of the whole member (hull of all plies, between the cut planes).
    pub fn convex(&self) -> ConvexSolid {
        self.convex_of(&self.place.hull)
    }

    /// Convex envelope of ply `k` alone. Built-up members are *not* convex as a
    /// whole (gaps, and plies can hide each other), so occlusion works per ply.
    pub fn ply_convex(&self, k: usize) -> ConvexSolid {
        self.convex_of(&convex_hull(&self.place.plies[k].outer))
    }

    fn convex_of(&self, hull: &[Vec2]) -> ConvexSolid {
        let planes = self.all_planes();
        let faces = self.clipped_faces(hull, &[]);
        let mut verts: Vec<Vec3> = faces.iter().flat_map(|f| f.outer.iter().copied()).collect();
        verts.dedup_by(|a, b| a.distance(*b) < 1e-12);
        let mut normals: Vec<Vec3> = planes.iter().map(|p| p.normal).collect();
        let mut edges = vec![self.place.x];
        let n = hull.len();
        for i in 0..n {
            let sn = self.place.side_normal(hull[i], hull[(i + 1) % n]);
            normals.push(sn);
            edges.extend(planes.iter().map(|p| sn.cross(p.normal)));
        }
        for (i, a) in planes.iter().enumerate() {
            for b in &planes[i + 1..] {
                edges.push(a.normal.cross(b.normal));
            }
        }
        ConvexSolid { verts, normals, edges }
    }

    /// Axial extent (min, max) of the body over all plies.
    pub fn t_range(&self) -> (f64, f64) {
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for f in self.faces() {
            for &p in &f.outer {
                let t = (p - self.place.origin).dot(self.place.x);
                lo = lo.min(t);
                hi = hi.max(t);
            }
        }
        (lo, hi)
    }

    /// Long-point to long-point cut length.
    pub fn cut_length(&self) -> f64 {
        let (lo, hi) = self.t_range();
        hi - lo
    }

    /// Angle (degrees) between each end cut and a square cut.
    pub fn cut_angles(&self) -> (f64, f64) {
        let ang = |p: &Plane| p.normal.dot(self.place.x).abs().clamp(-1.0, 1.0).acos().to_degrees();
        (ang(&self.start.plane), ang(&self.end.plane))
    }

    pub fn bbox(&self) -> BBox3 {
        BBox3::from_points(self.convex().verts)
    }

    pub fn centroid(&self) -> Vec3 {
        let (lo, hi) = self.t_range();
        self.place.at(self.place.centroid, 0.5 * (lo + hi))
    }
}

/// Convex polytope described by vertices plus candidate separating directions.
#[derive(Clone, Debug, PartialEq)]
pub struct ConvexSolid {
    pub verts: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub edges: Vec<Vec3>,
}

/// Result of a separating-axis query between two convex solids.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Separation {
    /// Separated (or touching) along `axis`; `a_below` is true if A lies on the −axis side.
    Separated { axis: Vec3, a_below: bool },
    /// Interpenetrating; `depth` is the minimum overlap found.
    Overlap { depth: f64 },
}

impl ConvexSolid {
    fn project(&self, a: Vec3) -> (f64, f64) {
        self.verts.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
            let d = p.dot(a);
            (lo.min(d), hi.max(d))
        })
    }

    fn axes(&self, o: &ConvexSolid) -> Vec<Vec3> {
        let mut axes: Vec<Vec3> = self.normals.iter().chain(o.normals.iter()).copied().collect();
        for &e1 in &self.edges {
            for &e2 in &o.edges {
                if let Some(c) = e1.cross(e2).try_normalized() {
                    axes.push(c);
                }
            }
        }
        axes.into_iter().filter_map(|a| a.try_normalized()).collect()
    }

    /// Separating-axis test. Overlaps smaller than `tol` count as touching.
    /// When several axes separate, the one maximizing `|axis · prefer|` is
    /// returned (useful for view-dependent ordering).
    pub fn separation(&self, o: &ConvexSolid, tol: f64, prefer: Option<Vec3>) -> Separation {
        let mut best: Option<(f64, Vec3, bool)> = None;
        let mut min_overlap = f64::INFINITY;
        for a in self.axes(o) {
            let (a0, a1) = self.project(a);
            let (b0, b1) = o.project(a);
            let overlap = (a1.min(b1) - a0.max(b0)).max(0.0);
            if overlap <= tol {
                let score = prefer.map_or(1.0, |p| a.dot(p).abs());
                if best.is_none_or(|(s, _, _)| score > s) {
                    best = Some((score, a, a1 <= b0 + tol));
                    if prefer.is_none() {
                        break;
                    }
                }
            } else {
                min_overlap = min_overlap.min(overlap);
            }
        }
        match best {
            Some((_, axis, a_below)) => Separation::Separated { axis, a_below },
            None => Separation::Overlap { depth: min_overlap },
        }
    }
}
