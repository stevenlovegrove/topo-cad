//! Derived geometry: exact member solids from topology + properties.

pub mod measure;
mod mesh;
mod resolve;
mod solid;
pub mod surfaces;

pub use mesh::{to_obj, triangulate, Mesh};
pub use solid::*;
pub use surfaces::{surface_panels, Panel};

use serde::{Deserialize, Serialize};
use topo_core::{Issue, MemberId, Model, Severity, Topology, Vec3};

/// Overlap (m) above which two members are reported as clashing: 0.1 mm.
pub const CLASH_TOL: f64 = 1e-4;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Geometry {
    /// Indexed by member id.
    pub members: Vec<MemberGeom>,
    pub issues: Vec<Issue>,
}

impl Geometry {
    pub fn build(model: &Model, topo: &Topology) -> Geometry {
        let mut r = resolve::Resolver::new(model, topo);
        let members = model.members.iter().map(|m| r.member_geom(m.id)).collect();
        Geometry { members, issues: r.issues }
    }

    pub fn member(&self, id: MemberId) -> &MemberGeom {
        &self.members[id.idx()]
    }

    /// Pairs of interpenetrating members (depth > `CLASH_TOL`).
    pub fn clashes(&self) -> Vec<(MemberId, MemberId, f64)> {
        let solids: Vec<ConvexSolid> = self.members.iter().map(|g| g.convex()).collect();
        let boxes: Vec<_> = self.members.iter().map(|g| g.bbox()).collect();
        let mut order: Vec<usize> = (0..boxes.len()).collect();
        order.sort_by(|&a, &b| boxes[a].min.x.total_cmp(&boxes[b].min.x));
        let mut out = vec![];
        for (k, &i) in order.iter().enumerate() {
            for &j in &order[k + 1..] {
                if boxes[j].min.x > boxes[i].max.x - CLASH_TOL {
                    break;
                }
                if !boxes[i].overlaps(&boxes[j], -CLASH_TOL) {
                    continue;
                }
                if let Separation::Overlap { depth } = solids[i].separation(&solids[j], CLASH_TOL, None) {
                    let (a, b) = (self.members[i].member, self.members[j].member);
                    out.push((a.min(b), a.max(b), depth));
                }
            }
        }
        out.sort_by_key(|a| (a.0, a.1));
        out
    }

    /// Clashes as validation issues.
    pub fn clash_issues(&self) -> Vec<Issue> {
        self.clashes()
            .into_iter()
            .map(|(a, b, d)| {
                Issue::new(Severity::Warning, "clash", format!("{a} and {b} interpenetrate by {:.1} mm", d * 1000.0))
                    .members([a, b])
            })
            .collect()
    }

    /// Butt joints whose cut end face does not fully land on the member it
    /// butts against (e.g. a stud standing past the end of its plate). The
    /// value is the largest overhang (m) of the end face beyond that member.
    /// Only ends that bear: cut on a face that faces up or down (an end that
    /// meets a side face, a plate butting another at a corner, carries no
    /// weight through its end).
    pub fn bearing_overhangs(&self) -> Vec<(MemberId, MemberId, f64)> {
        let mut out = vec![];
        for g in &self.members {
            for cut in [&g.start, &g.end] {
                let CutKind::Butt { against } = cut.kind else { continue };
                if cut.plane.normal.z.abs() < 0.5 {
                    continue;
                }
                // A compound butt bears on all the members it is cut against.
                let targets: Vec<Vec<topo_core::Plane>> =
                    std::iter::once(against).chain(cut.also.iter().copied()).map(|t| self.member(t).envelope_planes()).collect();
                // End-face vertices of this member (on any of its cut planes at this end).
                let face_pts: Vec<Vec3> = g
                    .faces()
                    .into_iter()
                    .filter(|f| {
                        cut.planes().any(|pl| f.normal.dot(pl.normal) > 1.0 - 1e-9 && pl.signed_distance(f.outer[0]).abs() < 1e-9)
                    })
                    .flat_map(|f| f.outer)
                    .collect();
                let beyond = |p: Vec3, planes: &Vec<topo_core::Plane>| planes.iter().map(|pl| pl.signed_distance(p)).fold(0.0, f64::max);
                let over = face_pts
                    .iter()
                    .map(|&p| targets.iter().map(|t| beyond(p, t)).fold(f64::INFINITY, f64::min))
                    .fold(0.0, f64::max);
                if over > BEARING_TOL {
                    out.push((g.member, against, over));
                }
            }
        }
        out
    }

    /// Partial-bearing issues, excluding ends fastened with engineered
    /// hardware (truss plates, hangers): there the hardware carries the load
    /// and fit-up gaps are expected, so bearing contact is not relied upon.
    pub fn bearing_issues(&self, model: &Model) -> Vec<Issue> {
        let hardware_at = |m: MemberId, n: topo_core::NodeId| {
            model.joint(n).is_some_and(|j| {
                j.connections.iter().any(|c| c.member == m && !model.connection(c.connection).hardware.is_empty())
            })
        };
        self.bearing_overhangs()
            .into_iter()
            .filter(|&(a, b, _)| {
                let mm = model.member(a);
                // The end in question is the one cut against `b`.
                let g = self.member(a);
                let n = if matches!(g.start.kind, CutKind::Butt { against } if against == b) { mm.start() } else { mm.end() };
                !hardware_at(a, n)
            })
            .map(|(a, b, d)| {
                Issue::new(Severity::Warning, "partial-bearing", format!("end of {a} overhangs {b} by {:.1} mm", d * 1000.0))
                    .members([a, b])
            })
            .collect()
    }
}

/// What a part of the building is: a framing member, or a solid it stands on (a slab, a curb).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartKind {
    Member(MemberId),
    /// (Its index in `Model::solids`.)
    Solid(usize),
}

/// A part of the building as a thing in space, whatever it is: what a view draws, a person
/// picks and a scan is compared with. Members and solids are different things to build and to
/// analyse; to look at, they are parts alike.
#[derive(Clone, Debug)]
pub struct Part {
    pub kind: PartKind,
    /// Its full path (`Garage/Walls/East wall/stud#3`, `Garage/Slab/slab`).
    pub path: String,
    pub role: String,
    /// Its faces: each an outline, anticlockwise seen from outside, and its outward normal.
    pub faces: Vec<(Vec<Vec3>, Vec3)>,
    /// Its own directions (unit, square to each other): for a member, along it and its section's
    /// two; for a solid, along its longest edge on plan, across that, and up.
    pub axes: [Vec3; 3],
    /// Its two ends along the first of those (a member's first and last nodes).
    pub ends: [Vec3; 2],
}

impl Geometry {
    /// Every part of the model: its members (in order, so a member's id is its index here), then its solids.
    pub fn parts(&self, model: &Model) -> Vec<Part> {
        let members = model.members.iter().map(|m| {
            let g = self.member(m.id);
            Part {
                kind: PartKind::Member(m.id),
                path: model.member_path(m.id),
                role: m.role.clone(),
                faces: g.faces().into_iter().map(|f| (f.outer, f.normal)).collect(),
                axes: [g.place.x, g.place.u, g.place.v],
                ends: [model.pos(m.start()), model.pos(m.end())],
            }
        });
        let solids = model.solids.iter().enumerate().map(|(k, s)| {
            let n = s.outline.len();
            let long = (0..n).map(|k| s.outline[(k + 1) % n] - s.outline[k]).max_by(|a, b| a.norm().total_cmp(&b.norm())).filter(|d| d.norm() > 1e-9).map_or(Vec3::X, |d| Vec3::new(d.x, d.y, 0.0) / d.norm());
            let faces: Vec<(Vec<Vec3>, Vec3)> = s.faces().into_iter().map(|f| (f.outer, f.normal)).collect();
            let axes = [long, Vec3::Z.cross(long), Vec3::Z];
            // (Its ends: the middle of its box, half its length either way.)
            let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
            for p in faces.iter().flat_map(|f| &f.0) {
                for a in 0..3 {
                    lo[a] = lo[a].min(p.dot(axes[a]));
                    hi[a] = hi[a].max(p.dot(axes[a]));
                }
            }
            let mid = |a: usize| 0.5 * (lo[a] + hi[a]);
            let at = |t: f64| axes[0] * t + axes[1] * mid(1) + axes[2] * mid(2);
            Part { kind: PartKind::Solid(k), path: model.solid_path(s), role: s.role.clone(), faces, axes, ends: [at(lo[0]), at(hi[0])] }
        });
        members.chain(solids).collect()
    }
}

/// Overhang (m) of a butt-cut end face beyond its target tolerated as full bearing: 1 mm.
pub const BEARING_TOL: f64 = 1e-3;

#[cfg(test)]
mod tests;
