//! Exact hidden-line removal for convex member solids.
//!
//! Visible edges are the edges of front-facing faces. Each edge is clipped
//! against the projected silhouettes (convex polygons) of members proven to be
//! in front of it by a separating axis. Hidden portions are kept separately so
//! views can draw them dashed.

use std::collections::BTreeMap;
use topo_core::{convex_hull, BBox2, MemberId, Vec2, Vec3};
use topo_geom::{ConvexSolid, MemberGeom, Separation};

/// Orthographic projection: screen x = p·right, y = p·up; depth = p·toward
/// (larger is closer to the viewer).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Projector {
    pub right: Vec3,
    pub up: Vec3,
    pub toward: Vec3,
}

impl Projector {
    /// Looking along `-toward` with `up_hint` projected to screen-up.
    pub fn new(toward: Vec3, up_hint: Vec3) -> Projector {
        let toward = toward.normalized();
        let up = up_hint.reject(toward).normalized();
        Projector { right: up.cross(toward), up, toward }
    }
    pub fn p(&self, q: Vec3) -> Vec2 {
        Vec2::new(q.dot(self.right), q.dot(self.up))
    }
    pub fn depth(&self, q: Vec3) -> f64 {
        q.dot(self.toward)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seg {
    pub a: Vec2,
    pub b: Vec2,
    /// World-space endpoints.
    pub a3: Vec3,
    pub b3: Vec3,
    pub hidden: bool,
    pub member: MemberId,
    pub ply: usize,
}

/// Something to draw, and to hide what is behind it: a ply of a member, or a solid (a slab, a
/// curb). Its faces are drawn; what it hides is what is behind its convex envelope.
pub struct Piece {
    /// What its lines are marked as being of (for a solid: a number past the members').
    pub member: MemberId,
    pub ply: usize,
    pub convex: ConvexSolid,
    pub faces: Vec<topo_geom::Face>,
    /// Edges of its faces that are not edges of the thing (a solid's seams): not drawn.
    pub seams: Vec<(Vec3, Vec3)>,
}

impl Piece {
    /// The plies of members.
    pub fn of_members(members: &[&MemberGeom]) -> Vec<Piece> {
        members.iter().flat_map(|g| (0..g.place.plies.len()).map(move |k| Piece { member: g.member, ply: k, convex: g.ply_convex(k), faces: g.ply_faces(k), seams: vec![] })).collect()
    }
    /// A solid, marked as `member`. (It hides what is behind the convex hull of its outline: a
    /// slab with a notch in it hides a little more than it should, seen from below.)
    pub fn of_solid(s: &topo_core::Solid, member: MemberId) -> Piece {
        let faces: Vec<topo_geom::Face> = s.faces().into_iter().map(|f| topo_geom::Face { outer: f.outer, holes: vec![], normal: f.normal }).collect();
        // A seam: along it on the top and on the underside, and up its two ends.
        let at = |p: Vec2, l: &topo_core::Level| Vec3::new(p.x, p.y, l.over(p));
        let seams = s.seam_lines().into_iter().flat_map(|(a, b)| [(at(a, &s.top), at(b, &s.top)), (at(a, &s.bottom), at(b, &s.bottom)), (at(a, &s.bottom), at(a, &s.top)), (at(b, &s.bottom), at(b, &s.top))]).collect();
        Piece { member, ply: 0, convex: topo_geom::solid_convex(s), faces, seams }
    }
}

/// One convex occluder.
struct Item<'a> {
    piece: &'a Piece,
    convex: &'a ConvexSolid,
    sil: Vec<Vec2>,
    bbox: BBox2,
    depth: f64,
}

const EPS: f64 = 1e-7;

/// Whether `q` lies inside convex CCW polygon `poly`.
pub fn point_in_convex(q: Vec2, poly: &[Vec2]) -> bool {
    let n = poly.len();
    n >= 3 && (0..n).all(|i| (poly[(i + 1) % n] - poly[i]).cross(q - poly[i]) >= 0.0)
}

/// Parameter interval of segment `a→b` inside convex CCW polygon `poly`
/// shrunk by `EPS` (so shared boundaries do not hide).
fn clip_inside(a: Vec2, b: Vec2, poly: &[Vec2]) -> Option<(f64, f64)> {
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    let d = b - a;
    let n = poly.len();
    for i in 0..n {
        let (p, q) = (poly[i], poly[(i + 1) % n]);
        let e = q - p;
        let len = e.norm();
        if len < 1e-12 {
            continue;
        }
        // Inside if cross(e, x - p)/len > EPS.
        let f0 = e.cross(a - p) / len - EPS;
        let fd = e.cross(d) / len;
        if fd.abs() < 1e-15 {
            if f0 <= 0.0 {
                return None;
            }
        } else {
            let t = -f0 / fd;
            if fd > 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
        if t0 >= t1 {
            return None;
        }
    }
    Some((t0, t1))
}

fn subtract(intervals: &mut Vec<(f64, f64)>, cut: (f64, f64)) {
    let mut out = Vec::with_capacity(intervals.len() + 1);
    for &(s, e) in intervals.iter() {
        if cut.1 <= s || cut.0 >= e {
            out.push((s, e));
            continue;
        }
        if cut.0 > s {
            out.push((s, cut.0));
        }
        if cut.1 < e {
            out.push((cut.1, e));
        }
    }
    *intervals = out;
}

/// Whether `j` occludes `i` (is in front of it) for this projection.
fn in_front(i: &Item, j: &Item, proj: &Projector) -> bool {
    match i.convex.separation(j.convex, 1e-6, Some(proj.toward)) {
        Separation::Separated { axis, a_below } => {
            let s = axis.dot(proj.toward);
            if s > 1e-6 {
                a_below
            } else if s < -1e-6 {
                !a_below
            } else {
                false
            }
        }
        Separation::Overlap { .. } => j.depth > i.depth,
    }
}

/// Endpoint-order-independent key of a projected edge (µm grid).
type EdgeKey = ((i64, i64), (i64, i64));
/// Projected endpoints and their world positions.
type Edge = (Vec2, Vec2, Vec3, Vec3);

fn key(p: Vec2) -> (i64, i64) {
    ((p.x * 1e6).round() as i64, (p.y * 1e6).round() as i64)
}

/// Visible and hidden edge segments of `members` under `proj`.
pub fn hidden_lines(members: &[&MemberGeom], proj: &Projector) -> Vec<Seg> {
    hidden_lines_of(&Piece::of_members(members), proj)
}

/// Visible and hidden edge segments of these pieces (members' plies, solids) under `proj`.
pub fn hidden_lines_of(pieces: &[Piece], proj: &Projector) -> Vec<Seg> {
    let items: Vec<Item> = pieces
        .iter()
        .map(|piece| {
            let convex = &piece.convex;
            let pts: Vec<Vec2> = convex.verts.iter().map(|&v| proj.p(v)).collect();
            let sil = convex_hull(&pts);
            let bbox = BBox2::from_points(pts.iter().copied());
            let depth = proj.depth(convex.verts.iter().fold(Vec3::ZERO, |a, &v| a + v) / convex.verts.len() as f64);
            Item { piece, convex, sil, bbox, depth }
        })
        .collect();

    // Occluders per item (pairs with overlapping screen boxes only).
    let mut occluders: Vec<Vec<usize>> = vec![vec![]; items.len()];
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|&a, &b| items[a].bbox.min.x.total_cmp(&items[b].bbox.min.x));
    for (k, &i) in order.iter().enumerate() {
        for &j in &order[k + 1..] {
            if items[j].bbox.min.x >= items[i].bbox.max.x - EPS {
                break;
            }
            if !items[i].bbox.overlaps(&items[j].bbox, -EPS) {
                continue;
            }
            if in_front(&items[i], &items[j], proj) {
                occluders[i].push(j);
            } else if in_front(&items[j], &items[i], proj) {
                occluders[j].push(i);
            }
        }
    }

    let mut out = vec![];
    for (i, it) in items.iter().enumerate() {
        // Unique edges of this ply's front-facing faces.
        let mut edges: BTreeMap<EdgeKey, Edge> = BTreeMap::new();
        for f in &it.piece.faces {
            if f.normal.dot(proj.toward) <= 1e-9 {
                continue;
            }
            for ring in std::iter::once(&f.outer).chain(f.holes.iter()) {
                let n = ring.len();
                for k in 0..n {
                    let (a3, b3) = (ring[k], ring[(k + 1) % n]);
                    let (a, b) = (proj.p(a3), proj.p(b3));
                    if a.distance(b) < 1e-9 {
                        continue;
                    }
                    if it.piece.seams.iter().any(|&(p, q)| (p.distance(a3) < 1e-9 && q.distance(b3) < 1e-9) || (p.distance(b3) < 1e-9 && q.distance(a3) < 1e-9)) {
                        continue;
                    }
                    let (ka, kb) = (key(a), key(b));
                    let k2 = if ka <= kb { (ka, kb) } else { (kb, ka) };
                    edges.entry(k2).or_insert((a, b, a3, b3));
                }
            }
        }
        for (_, (a, b, a3, b3)) in edges {
            let mut vis = vec![(0.0, 1.0)];
            let bb = BBox2::from_points([a, b]);
            for &j in &occluders[i] {
                if !items[j].bbox.overlaps(&bb, EPS) {
                    continue;
                }
                if let Some(c) = clip_inside(a, b, &items[j].sil) {
                    subtract(&mut vis, c);
                    if vis.is_empty() {
                        break;
                    }
                }
            }
            let mut last = 0.0;
            let (member, ply) = (it.piece.member, it.piece.ply);
            for &(s, e) in &vis {
                if s > last + 1e-9 {
                    out.push(Seg { a: a.lerp(b, last), b: a.lerp(b, s), a3: a3.lerp(b3, last), b3: a3.lerp(b3, s), hidden: true, member, ply });
                }
                out.push(Seg { a: a.lerp(b, s), b: a.lerp(b, e), a3: a3.lerp(b3, s), b3: a3.lerp(b3, e), hidden: false, member, ply });
                last = e;
            }
            if last < 1.0 - 1e-9 {
                out.push(Seg { a: a.lerp(b, last), b, a3: a3.lerp(b3, last), b3, hidden: true, member, ply });
            }
        }
    }
    // Plies that touch share an edge (e.g. the line between the plies of a
    // double top plate); keep one copy of identical visible segments per member.
    let mut seen = std::collections::BTreeSet::new();
    out.retain(|s| {
        let (ka, kb) = (key(s.a), key(s.b));
        let k = if ka <= kb { (ka, kb) } else { (kb, ka) };
        seen.insert((s.member, s.hidden, k))
    });
    out
}

/// Whether the ray from `p` toward the viewer passes through the interior of
/// any ply of any member other than `own`.
pub fn is_occluded(p: Vec3, own: MemberId, members: &[&MemberGeom], proj: &Projector) -> bool {
    let d = proj.toward;
    members.iter().filter(|g| g.member != own).any(|g| {
        let pl = &g.place;
        (0..pl.plies.len()).any(|k| {
            let hull = convex_hull(&pl.plies[k].outer);
            let n = hull.len();
            let planes = (0..n)
                .map(|i| topo_core::Plane { point: pl.at(hull[i], 0.0), normal: pl.side_normal(hull[i], hull[(i + 1) % n]) })
                .chain(g.start.planes().copied())
                .chain(g.end.planes().copied());
            let (mut s0, mut s1) = (1e-6f64, f64::INFINITY);
            for q in planes {
                let num = -q.signed_distance(p);
                let den = q.normal.dot(d);
                if den.abs() < 1e-12 {
                    if num < 1e-6 {
                        return false;
                    }
                } else if den < 0.0 {
                    s0 = s0.max(num / den);
                } else {
                    s1 = s1.min(num / den);
                }
            }
            s1 - s0 > 1e-5
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use topo_core::v2;

    #[test]
    fn clips_segment_through_square() {
        let sq = [v2(0., 0.), v2(1., 0.), v2(1., 1.), v2(0., 1.)];
        let (t0, t1) = clip_inside(v2(-1., 0.5), v2(2., 0.5), &sq).unwrap();
        assert!((t0 - 1.0 / 3.0).abs() < 1e-6 && (t1 - 2.0 / 3.0).abs() < 1e-6);
        // Along the boundary: not hidden.
        assert!(clip_inside(v2(-1., 0.), v2(2., 0.), &sq).is_none());
    }

    #[test]
    fn interval_subtraction() {
        let mut v = vec![(0.0, 1.0)];
        subtract(&mut v, (0.2, 0.4));
        subtract(&mut v, (0.8, 1.2));
        assert_eq!(v, vec![(0.0, 0.2), (0.4, 0.8)]);
    }
}
