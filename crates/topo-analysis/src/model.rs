//! Analytical model: nodes and elements derived from topology.

use serde::{Deserialize, Serialize};
use topo_core::{Fixity, MaterialId, MemberId, Model, NodeId, SectionId, Support, Topology, Vec3};
use topo_geom::Geometry;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ANode {
    pub id: NodeId,
    pub pos: Vec3,
}

/// One analysis element = one segment of a member path between two nodes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Element {
    pub member: MemberId,
    /// Segment index along the member path.
    pub segment: usize,
    pub i: NodeId,
    pub j: NodeId,
    pub length: f64,
    pub section: SectionId,
    pub material: MaterialId,
    /// Stiffness properties (SI).
    pub e: f64,
    pub g: f64,
    pub area: f64,
    pub i_u: f64,
    pub i_v: f64,
    pub j_t: f64,
    /// Local axes (x along element, v = depth direction).
    pub x: Vec3,
    pub u: Vec3,
    pub v: Vec3,
    /// Offset from the node line to the section centroid (rigid-link eccentricity).
    pub eccentricity: Vec3,
    /// End fixities at i and j. Interior segment boundaries of one physical member are rigid.
    pub fix_i: Fixity,
    pub fix_j: Fixity,
}

/// Members fastened face-to-face (acting together), from `Model::bonds`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Coupling {
    pub a: MemberId,
    pub b: MemberId,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnalysisModel {
    pub nodes: Vec<ANode>,
    pub elements: Vec<Element>,
    pub supports: Vec<Support>,
    pub couplings: Vec<Coupling>,
}

impl AnalysisModel {
    pub fn extract(model: &Model, topo: &Topology, geom: &Geometry) -> AnalysisModel {
        let mut used = vec![false; model.nodes.len()];
        let mut elements = vec![];
        for m in &model.members {
            let sec = model.section(m.section);
            let mat = model.material(m.material);
            let g = geom.member(m.id);
            let ecc = g.place.u * g.place.centroid.x + g.place.v * g.place.centroid.y;
            let end_fix = |n: NodeId| {
                model
                    .joint(n)
                    .and_then(|j| j.connections.iter().find(|c| c.member == m.id))
                    .map(|c| model.connection(c.connection).fixity)
                    .unwrap_or_else(|| {
                        // No connection recorded: continuous only if nothing else meets here.
                        if topo.junction(n).members.len() == 1 {
                            Fixity::Rigid
                        } else {
                            Fixity::Pinned
                        }
                    })
            };
            let last = m.path.len() - 2;
            for (k, w) in m.path.windows(2).enumerate() {
                let (pi, pj) = (model.pos(w[0]), model.pos(w[1]));
                used[w[0].idx()] = true;
                used[w[1].idx()] = true;
                elements.push(Element {
                    member: m.id,
                    segment: k,
                    i: w[0],
                    j: w[1],
                    length: pi.distance(pj),
                    section: m.section,
                    material: m.material,
                    e: mat.e,
                    g: mat.g,
                    area: sec.props.area,
                    i_u: sec.props.i_u,
                    i_v: sec.props.i_v,
                    j_t: sec.props.j,
                    x: g.place.x,
                    u: g.place.u,
                    v: g.place.v,
                    eccentricity: ecc,
                    fix_i: if k == 0 { end_fix(w[0]) } else { Fixity::Rigid },
                    fix_j: if k == last { end_fix(w[1]) } else { Fixity::Rigid },
                });
            }
        }
        let nodes = model.nodes.iter().filter(|n| used[n.id.idx()]).map(|n| ANode { id: n.id, pos: n.pos }).collect();
        let couplings = model.bonds.iter().map(|b| Coupling { a: b.a, b: b.b }).collect();
        AnalysisModel { nodes, elements, supports: model.supports.clone(), couplings }
    }
}
