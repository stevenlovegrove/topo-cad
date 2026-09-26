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
/// * Bearing: A's underside is at or above B's top (A rests on B).
/// * Fastened: A ends against B's face (butt cut) without resting on it.
pub fn support_graph(model: &Model, topo: &Topology, geom: &Geometry) -> Vec<SupportEdge> {
    let tol = 1e-4;
    let boxes: Vec<_> = geom.members.iter().map(|g| g.bbox()).collect();
    let rests_on = |a: MemberId, b: MemberId| boxes[a.idx()].min.z >= boxes[b.idx()].max.z - tol;
    let mut out = vec![];
    for j in &topo.junctions {
        for a in &j.members {
            for b in &j.members {
                if a.member == b.member {
                    continue;
                }
                let (ba, bb) = (&boxes[a.member.idx()], &boxes[b.member.idx()]);
                let transfer = if rests_on(a.member, b.member) {
                    Some(Transfer::Bearing)
                } else if a.incidence.is_end() && ba.max.z > bb.min.z + tol {
                    // (A entirely below B would be *supporting* B, e.g. a stud under a plate.)
                    let g = geom.member(a.member);
                    let cut = if a.incidence == Incidence::Start { &g.start } else { &g.end };
                    matches!(cut.kind, CutKind::Butt { against } if against == b.member).then_some(Transfer::Fastened)
                } else {
                    None
                };
                if let Some(t) = transfer {
                    out.push(SupportEdge { member: a.member, by: b.member, node: Some(j.node), transfer: t });
                }
            }
        }
    }
    for bond in &model.bonds {
        for (a, b) in [(bond.a, bond.b), (bond.b, bond.a)] {
            if rests_on(a, b) {
                out.push(SupportEdge { member: a, by: b, node: None, transfer: Transfer::Bearing });
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
