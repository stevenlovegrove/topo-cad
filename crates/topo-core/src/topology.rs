//! Junction classification: how members meet at each node.

use crate::ids::{MemberId, NodeId};
use crate::math::Vec3;
use crate::model::Model;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Incidence {
    Start,
    End,
    /// Interior node at `path[index]`.
    Through { index: usize },
}

impl Incidence {
    pub fn is_end(self) -> bool {
        !matches!(self, Incidence::Through { .. })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemberAtNode {
    pub member: MemberId,
    pub incidence: Incidence,
    /// For ends: unit vector from the node *out of* the member body (the
    /// direction the member was travelling when it arrived). For through
    /// members: the member's axis direction.
    pub dir: Vec3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JunctionKind {
    /// No members (orphan node).
    Isolated,
    /// One member ends here and nothing else: a dangling end.
    Free,
    /// A single member passes through: an attachment/load point.
    Pass,
    /// Two collinear member ends.
    Splice,
    /// Two angled member ends (L).
    Corner,
    /// One or more ends landing on one or more through members (T).
    Tee,
    /// Two or more through members and no ends (X: stacked bearing, lap).
    Cross,
    /// Three or more ends, no through member.
    Complex,
}

impl JunctionKind {
    pub fn glyph(self) -> &'static str {
        match self {
            JunctionKind::Isolated => "?",
            JunctionKind::Free => "F",
            JunctionKind::Pass => "P",
            JunctionKind::Splice => "S",
            JunctionKind::Corner => "L",
            JunctionKind::Tee => "T",
            JunctionKind::Cross => "X",
            JunctionKind::Complex => "C",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            JunctionKind::Isolated => "isolated node",
            JunctionKind::Free => "free end",
            JunctionKind::Pass => "attachment point",
            JunctionKind::Splice => "splice",
            JunctionKind::Corner => "corner (L)",
            JunctionKind::Tee => "tee (T)",
            JunctionKind::Cross => "cross / bearing (X)",
            JunctionKind::Complex => "complex",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Junction {
    pub node: NodeId,
    pub members: Vec<MemberAtNode>,
    pub kind: JunctionKind,
}

impl Junction {
    pub fn ends(&self) -> impl Iterator<Item = &MemberAtNode> {
        self.members.iter().filter(|m| m.incidence.is_end())
    }
    pub fn throughs(&self) -> impl Iterator<Item = &MemberAtNode> {
        self.members.iter().filter(|m| !m.incidence.is_end())
    }
    pub fn get(&self, m: MemberId) -> Option<&MemberAtNode> {
        self.members.iter().find(|x| x.member == m)
    }
}

/// Junctions indexed by node id.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Topology {
    pub junctions: Vec<Junction>,
}

impl Topology {
    pub fn build(model: &Model) -> Topology {
        let mut junctions: Vec<Junction> = model
            .nodes
            .iter()
            .map(|n| Junction { node: n.id, members: vec![], kind: JunctionKind::Isolated })
            .collect();
        for m in &model.members {
            let (a, b) = (model.pos(m.start()), model.pos(m.end()));
            let Some(x) = (b - a).try_normalized() else { continue };
            let last = m.path.len() - 1;
            for (i, &n) in m.path.iter().enumerate() {
                let (incidence, dir) = match i {
                    0 => (Incidence::Start, -x),
                    i if i == last => (Incidence::End, x),
                    i => (Incidence::Through { index: i }, x),
                };
                junctions[n.idx()].members.push(MemberAtNode { member: m.id, incidence, dir });
            }
        }
        for j in &mut junctions {
            j.kind = classify(&j.members);
        }
        Topology { junctions }
    }

    pub fn junction(&self, n: NodeId) -> &Junction {
        &self.junctions[n.idx()]
    }

    /// Count of junctions of each kind (excluding isolated nodes).
    pub fn census(&self) -> Vec<(JunctionKind, usize)> {
        use JunctionKind::*;
        [Free, Pass, Splice, Corner, Tee, Cross, Complex]
            .into_iter()
            .map(|k| (k, self.junctions.iter().filter(|j| j.kind == k).count()))
            .filter(|(_, c)| *c > 0)
            .collect()
    }
}

fn classify(ms: &[MemberAtNode]) -> JunctionKind {
    let ends: Vec<&MemberAtNode> = ms.iter().filter(|m| m.incidence.is_end()).collect();
    let through = ms.len() - ends.len();
    match (ends.len(), through) {
        (0, 0) => JunctionKind::Isolated,
        (1, 0) => JunctionKind::Free,
        (0, 1) => JunctionKind::Pass,
        (2, 0) => {
            if ends[0].dir.dot(ends[1].dir) < -1.0 + 1e-6 {
                JunctionKind::Splice
            } else {
                JunctionKind::Corner
            }
        }
        (_, 0) => JunctionKind::Complex,
        (0, _) => JunctionKind::Cross,
        (_, _) => JunctionKind::Tee,
    }
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
    fn classifies_basic_junctions() {
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
        // Plate along x; stud landing mid-span (T); second plate spliced; post at the corner (L).
        let plate = m.add_member_between(v3(0., 0., 0.), v3(2., 0., 0.), &spec);
        let tee = m.node_along(plate, 1.0).unwrap();
        m.add_member_between(v3(1., 0., 0.), v3(1., 0., 2.), &spec);
        m.add_member_between(v3(2., 0., 0.), v3(4., 0., 0.), &spec);
        m.add_member_between(v3(0., 0., 0.), v3(0., 0., 2.), &spec);
        let topo = Topology::build(&m);
        let kind_at = |p| topo.junction(m.clone().find_node(p).unwrap()).kind;
        assert_eq!(topo.junction(tee).kind, JunctionKind::Tee);
        assert_eq!(kind_at(v3(2., 0., 0.)), JunctionKind::Splice);
        assert_eq!(kind_at(v3(0., 0., 0.)), JunctionKind::Corner);
        assert_eq!(kind_at(v3(1., 0., 2.)), JunctionKind::Free);
    }
}
