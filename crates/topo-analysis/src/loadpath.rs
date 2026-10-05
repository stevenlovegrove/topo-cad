//! Gravity support graph: which member bears on which, and how, derived from
//! junctions plus solid geometry. This is the backbone of a light-frame load
//! takedown (roof → trusses → plates → headers → jacks → foundation).

use serde::{Deserialize, Serialize};
use topo_core::{Incidence, Issue, MemberId, Model, NodeId, Severity, Topology};
use topo_geom::{CutKind, Geometry};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Transfer {
    /// Direct compression contact (end grain or side bearing).
    Bearing,
    /// Load must pass through fasteners/hardware in shear (end-nailed, hanger).
    Fastened,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportEdge {
    /// Member being supported.
    pub member: MemberId,
    /// Supporting member.
    pub by: MemberId,
    /// Junction where the transfer happens; `None` for distributed face
    /// contact along a bond (e.g. top plate on a flush header).
    pub node: Option<NodeId>,
    pub transfer: Transfer,
}

/// All gravity support relations between members meeting at a junction or bonded face to face.
/// * Bearing: A's underside is at or above B's top (A rests on B); or B ends butted up into A
///   from below (a stud under a plate, sloping or not), or A ends butted down onto B.
/// * Fastened: A ends against B's face (butt cut) without resting on it.
pub fn support_graph(model: &Model, topo: &Topology, geom: &Geometry) -> Vec<SupportEdge> {
    let tol = 1e-4;
    let boxes: Vec<_> = geom.members.iter().map(|g| g.bbox()).collect();
    let rests_on = |a: MemberId, b: MemberId| boxes[a.idx()].min.z >= boxes[b.idx()].max.z - tol;
    // An end of a member at a junction, butted against another: which way it arrives (up, down, or across).
    let butted = |a: &topo_core::MemberAtNode, b: MemberId| {
        let g = geom.member(a.member);
        let cut = if a.incidence == Incidence::Start { &g.start } else { &g.end };
        a.incidence.is_end() && matches!(cut.kind, CutKind::Butt { against } if against == b)
    };
    let mut out = vec![];
    let mut push = |e: SupportEdge| {
        if !out.contains(&e) {
            out.push(e);
        }
    };
    for j in &topo.junctions {
        for a in &j.members {
            for b in &j.members {
                if a.member == b.member {
                    continue;
                }
                let (ba, bb) = (&boxes[a.member.idx()], &boxes[b.member.idx()]);
                let edge = |member, by, transfer| SupportEdge { member, by, node: Some(j.node), transfer };
                if rests_on(a.member, b.member) {
                    push(edge(a.member, b.member, Transfer::Bearing));
                } else if butted(a, b.member) && a.dir.z > 0.5 {
                    // A comes up into B from below and ends on it: B rests on A.
                    push(edge(b.member, a.member, Transfer::Bearing));
                } else if butted(a, b.member) && a.dir.z < -0.5 {
                    // A comes down onto B and ends on it: A rests on B.
                    push(edge(a.member, b.member, Transfer::Bearing));
                } else if butted(a, b.member) && ba.max.z > bb.min.z + tol {
                    // (A entirely below B would be *supporting* B, e.g. a stud under a plate.)
                    push(edge(a.member, b.member, Transfer::Fastened));
                }
            }
        }
    }
    // A node on a member's face joins that member to those the node is on the axis of (and to
    // others it is on the face of). The face says which rests on which: what is at a node on a
    // member's upward face rests on the member there, and the member rests on what is at a node
    // on its downward face.
    // (Only what is beyond that face: where the node is also the end of plates level with the member, a
    // tee's, those are beside it, not on it.)
    let middle = |m: MemberId| {
        let pts: Vec<topo_core::Vec3> = geom.member(m).faces().into_iter().flat_map(|f| f.outer).collect();
        pts.iter().fold(topo_core::Vec3::ZERO, |a, p| a + *p) / pts.len().max(1) as f64
    };
    for at in &model.attachments {
        let face = model.face(at.member, at.face);
        let up = face.normal.z;
        let others = topo.junction(at.node).members.iter().map(|m| m.member).chain(model.attached_at(at.node).map(|o| o.member)).filter(|&o| o != at.member);
        for other in others {
            if (middle(other) - face.origin).dot(face.normal) <= tol {
                continue;
            }
            let (member, by) = if up > 0.5 {
                (other, at.member)
            } else if up < -0.5 {
                (at.member, other)
            } else {
                continue;
            };
            push(SupportEdge { member, by, node: Some(at.node), transfer: Transfer::Bearing });
        }
    }
    // Members bonded face to face: one rests on the other where it is above it, across their
    // length (a cap plate on the plate under it, sloping or not; not the plies of a post, side by side).
    let centre = |m: MemberId| {
        let pts: Vec<topo_core::Vec3> = geom.member(m).faces().into_iter().flat_map(|f| f.outer).collect();
        pts.iter().fold(topo_core::Vec3::ZERO, |a, p| a + *p) / pts.len().max(1) as f64
    };
    for bond in &model.bonds {
        for (a, b) in [(bond.a, bond.b), (bond.b, bond.a)] {
            let x = model.member_axes(b).x;
            let d = centre(a) - centre(b);
            let d = d - x * d.dot(x);
            if rests_on(a, b) || (d.z > tol && d.z >= 0.7 * d.norm()) {
                push(SupportEdge { member: a, by: b, node: None, transfer: Transfer::Bearing });
            }
        }
    }
    out
}

/// Members whose gravity load has no reliable path: supported only through
/// nails in shear, or not supported at all. Members with a support, a
/// bearing contact, or engineered hardware (hangers, truss plates) pass.
pub fn load_path_issues(model: &Model, topo: &Topology, geom: &Geometry) -> Vec<Issue> {
    let edges = support_graph(model, topo, geom);
    let supported_node: Vec<bool> = {
        let mut v = vec![false; model.nodes.len()];
        for s in &model.supports {
            v[s.node.idx()] = true;
        }
        v
    };
    let mut hardware = vec![false; model.members.len()];
    for j in &model.joints {
        for c in &j.connections {
            if !model.connection(c.connection).hardware.is_empty() {
                hardware[c.member.idx()] = true;
                if let Some(t) = c.to {
                    hardware[t.idx()] = true;
                }
            }
        }
    }
    let mut out = vec![];
    for m in &model.members {
        if m.path.iter().any(|n| supported_node[n.idx()]) || hardware[m.id.idx()] {
            continue;
        }
        let mine: Vec<&SupportEdge> = edges.iter().filter(|e| e.member == m.id).collect();
        if mine.iter().any(|e| e.transfer == Transfer::Bearing) {
            continue;
        }
        if mine.is_empty() {
            out.push(
                Issue::new(Severity::Warning, "no-gravity-support", format!("{} ({}) has no gravity support path", m.id, m.role))
                    .members([m.id]),
            );
        } else {
            let by: Vec<String> = mine.iter().map(|e| format!("{} ({})", e.by, model.member(e.by).role)).collect();
            out.push(
                Issue::new(
                    Severity::Warning,
                    "fastener-only-support",
                    format!(
                        "{} ({}) is carried only by nails in shear to {} — no bearing path",
                        m.id,
                        m.role,
                        by.join(", ")
                    ),
                )
                .members(std::iter::once(m.id).chain(mine.iter().map(|e| e.by))),
            );
        }
    }
    out
}
