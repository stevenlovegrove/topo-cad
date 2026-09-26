//! Model validation: structural sanity of the topology and references.

use crate::ids::{MemberId, NodeId};
use crate::math::LEN_TOL;
use crate::model::{JointRule, Model};
use crate::topology::{JunctionKind, Topology};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Issue {
    pub severity: Severity,
    /// Stable machine-readable code, e.g. `unconnected-nodes`.
    pub code: String,
    pub message: String,
    pub nodes: Vec<NodeId>,
    pub members: Vec<MemberId>,
}

impl Issue {
    pub fn new(severity: Severity, code: &str, message: impl Into<String>) -> Issue {
        Issue { severity, code: code.into(), message: message.into(), nodes: vec![], members: vec![] }
    }
    pub fn nodes(mut self, n: impl IntoIterator<Item = NodeId>) -> Issue {
        self.nodes.extend(n);
        self
    }
    pub fn members(mut self, m: impl IntoIterator<Item = MemberId>) -> Issue {
        self.members.extend(m);
        self
    }
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{:?}] {}: {}", self.severity, self.code, self.message)
    }
}

pub fn validate(model: &Model, topo: &Topology) -> Vec<Issue> {
    let mut out = vec![];
    for m in &model.members {
        if m.section.idx() >= model.sections.len() || m.material.idx() >= model.materials.len() {
            out.push(Issue::new(Severity::Error, "bad-reference", format!("{} references a missing section/material", m.id)).members([m.id]));
            continue;
        }
        let (a, b) = (model.pos(m.start()), model.pos(m.end()));
        let len = a.distance(b);
        if len < LEN_TOL {
            out.push(Issue::new(Severity::Error, "zero-length", format!("{} has zero length", m.id)).members([m.id]));
            continue;
        }
        let x = (b - a) / len;
        let mut prev_t = -f64::INFINITY;
        for &n in &m.path {
            let p = model.pos(n) - a;
            let t = p.dot(x);
            let off = (p - x * t).norm();
            if off > LEN_TOL {
                out.push(
                    Issue::new(Severity::Error, "non-collinear", format!("{n} is {off:.4} m off the axis of {}", m.id))
                        .members([m.id])
                        .nodes([n]),
                );
            }
            if t <= prev_t + LEN_TOL {
                out.push(
                    Issue::new(Severity::Error, "path-order", format!("path of {} is not strictly ordered at {n}", m.id))
                        .members([m.id])
                        .nodes([n]),
                );
            }
            prev_t = t;
        }
    }

    // Coincident but distinct nodes: almost always a missed connection.
    let mut sorted: Vec<&crate::model::Node> = model.nodes.iter().collect();
    sorted.sort_by(|p, q| p.pos.x.total_cmp(&q.pos.x));
    for i in 0..sorted.len() {
        for j in i + 1..sorted.len() {
            if sorted[j].pos.x - sorted[i].pos.x > LEN_TOL {
                break;
            }
            if sorted[i].pos.distance(sorted[j].pos) <= LEN_TOL {
                let (a, b) = (sorted[i].id, sorted[j].id);
                if topo.junction(a).kind != JunctionKind::Isolated && topo.junction(b).kind != JunctionKind::Isolated {
                    out.push(
                        Issue::new(Severity::Warning, "unconnected-nodes", format!("{a} and {b} coincide but are not the same node"))
                            .nodes([a, b]),
                    );
                }
            }
        }
    }

    for j in &model.joints {
        let junction = topo.junction(j.node);
        let at = |m: MemberId| junction.get(m).is_some();
        for r in &j.rules {
            let ms: Vec<MemberId> = match r {
                JointRule::Butt { member, against } => vec![*member, *against],
                JointRule::Through { member }
                | JointRule::Square { member }
                | JointRule::Plane { member, .. }
                | JointRule::Plumb { member } => vec![*member],
                JointRule::Miter { a, b } => vec![*a, *b],
            };
            for m in ms {
                if !at(m) {
                    out.push(
                        Issue::new(Severity::Error, "rule-not-incident", format!("joint rule at {} names {m}, which does not meet there", j.node))
                            .nodes([j.node])
                            .members([m]),
                    );
                }
            }
        }
        for c in &j.connections {
            if !at(c.member) || c.to.is_some_and(|t| !at(t)) {
                out.push(
                    Issue::new(Severity::Error, "connection-not-incident", format!("connection at {} names a member that does not meet there", j.node))
                        .nodes([j.node])
                        .members([c.member]),
                );
            }
        }
    }

    for s in &model.supports {
        if topo.junction(s.node).kind == JunctionKind::Isolated {
            out.push(Issue::new(Severity::Warning, "support-on-isolated-node", format!("support at {} has no members", s.node)).nodes([s.node]));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::MaterialId;
    use crate::material::Material;
    use crate::math::v3;
    use crate::model::MemberSpec;
    use crate::section::Section;

    #[test]
    fn flags_unconnected_coincident_nodes() {
        let mut m = Model::new("t");
        let s = m.add_section(Section::rect("r", 0.04, 0.09));
        let mat = m.add_material(Material {
            id: MaterialId(0),
            name: "x".into(),
            family: "x".into(),
            e: 1.0,
            g: 1.0,
            density: 1.0,
            design_key: None,
        });
        let spec = MemberSpec::new("b", s, mat);
        let a = m.add_node(v3(0., 0., 0.));
        let b = m.add_node(v3(1., 0., 0.));
        let c = m.add_node(v3(1., 0., 0.)); // duplicate of b
        let d = m.add_node(v3(1., 1., 0.));
        m.add_member(&[a, b], &spec);
        m.add_member(&[c, d], &spec);
        let issues = validate(&m, &Topology::build(&m));
        assert!(issues.iter().any(|i| i.code == "unconnected-nodes"));
    }
}
