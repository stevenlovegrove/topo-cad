//! Junction resolution: decides each member end's cut plane from topology.
//! See `DESIGN.md` §4 for the rules.

use crate::solid::{CutKind, EndCut, MemberGeom, Placement};
use topo_core::{
    v3, Issue, Junction, JointRule, Member, MemberAtNode, MemberId, Model, Plane, Severity, Topology, Vec2, Vec3,
};

/// World-space infinite prism of a member's convex envelope.
struct Envelope {
    faces: Vec<(Vec3, Vec3)>, // (point, outward normal)
}

struct Hit {
    s_enter: f64,
    enter: usize,
    s_exit: f64,
    exit: usize,
}

impl Envelope {
    fn new(p: &Placement) -> Envelope {
        let n = p.hull.len();
        let faces = (0..n)
            .map(|i| {
                let (a, b) = (p.hull[i], p.hull[(i + 1) % n]);
                (p.at(a, 0.0), p.side_normal(a, b))
            })
            .collect();
        Envelope { faces }
    }

    /// Cyrus–Beck clip of the ray `r0 + d s` (s ≥ 0) against the prism.
    fn raycast(&self, r0: Vec3, d: Vec3) -> Option<Hit> {
        let (mut s0, mut s1) = (f64::NEG_INFINITY, f64::INFINITY);
        let (mut enter, mut exit) = (usize::MAX, usize::MAX);
        for (i, &(q, n)) in self.faces.iter().enumerate() {
            let num = n.dot(q - r0);
            let den = n.dot(d);
            if den.abs() < 1e-12 {
                if num < 0.0 {
                    return None; // parallel and outside this face
                }
            } else {
                let s = num / den;
                if den < 0.0 {
                    if s > s0 {
                        s0 = s;
                        enter = i;
                    }
                } else if s < s1 {
                    s1 = s;
                    exit = i;
                }
            }
        }
        if enter == usize::MAX || exit == usize::MAX || s0 > s1 || s0 < 0.0 {
            return None;
        }
        Some(Hit { s_enter: s0, enter, s_exit: s1, exit })
    }

    fn plane(&self, face: usize, flip: bool) -> Plane {
        let (q, n) = self.faces[face];
        Plane::new(q, if flip { -n } else { n })
    }
}

fn outranks(a: &Member, b: &Member) -> bool {
    a.priority > b.priority || (a.priority == b.priority && a.id < b.id)
}

pub(crate) fn placement(model: &Model, m: &Member) -> Placement {
    let axes = model.member_axes(m.id);
    let sec = model.section(m.section);
    let bb = sec.bbox();
    let a = &m.anchor;
    let shift = Vec2::new(
        bb.min.x + (a.u + 0.5) * bb.width() + a.offset.x,
        bb.min.y + (a.v + 0.5) * bb.height() + a.offset.y,
    );
    Placement::new(&axes, sec.ply_outlines(), shift, sec.props.centroid)
}

pub(crate) struct Resolver<'a> {
    pub model: &'a Model,
    pub topo: &'a Topology,
    pub places: Vec<Placement>,
    envs: Vec<Envelope>,
    pub issues: Vec<Issue>,
}

impl<'a> Resolver<'a> {
    pub fn new(model: &'a Model, topo: &'a Topology) -> Self {
        let places: Vec<Placement> = model.members.iter().map(|m| placement(model, m)).collect();
        let envs = places.iter().map(Envelope::new).collect();
        Resolver { model, topo, places, envs, issues: vec![] }
    }

    pub fn member_geom(&mut self, m: MemberId) -> MemberGeom {
        let member = self.model.member(m);
        let start = self.resolve_end(member, member.start());
        let end = self.resolve_end(member, member.end());
        MemberGeom { member: m, place: self.places[m.idx()].clone(), start, end }
    }

    fn resolve_end(&mut self, member: &Member, node: topo_core::NodeId) -> EndCut {
        let j: &Junction = self.topo.junction(node);
        let me = *j.get(member.id).expect("member missing from its own junction");
        let p = &self.places[member.id.idx()];
        let d = me.dir;
        // Ray along the centroid line from the far end toward (and past) this node.
        let r0 = if me.incidence == topo_core::Incidence::End {
            p.at(p.centroid, 0.0)
        } else {
            p.at(p.centroid, p.length)
        };
        let s_node = p.length;
        let node_pos = self.model.pos(node);
        let square = EndCut { plane: Plane::new(node_pos, d), extra: vec![], also: vec![], kind: CutKind::Square };

        let others: Vec<MemberAtNode> = j
            .members
            .iter()
            .filter(|o| o.member != member.id && !self.places[o.member.idx()].x.is_parallel(p.x))
            .copied()
            .collect();
        // How far from the node a face of `o` can legitimately be met along the
        // ray: the two section sizes, stretched by 1/sin(angle) because a member
        // arriving at a shallow angle meets the other's face far from the node.
        let window = |o: MemberId| {
            let sin = d.cross(self.places[o.idx()].x).norm().max(0.1);
            (p.radius() + self.places[o.idx()].radius()) / sin + 1e-6
        };

        // Explicit rules first.
        if let Some(spec) = self.model.joint(node) {
            for rule in &spec.rules {
                match *rule {
                    JointRule::Butt { member: mm, against } if mm == member.id => {
                        return match self.envs[against.idx()].raycast(r0, d) {
                            Some(h) => EndCut { plane: self.envs[against.idx()].plane(h.enter, true), extra: vec![], also: vec![], kind: CutKind::Butt { against } },
                            None => {
                                self.issues.push(
                                    Issue::new(Severity::Warning, "butt-miss", format!("{} does not reach {against} at {node}; cut square", member.id))
                                        .members([member.id, against])
                                        .nodes([node]),
                                );
                                square
                            }
                        };
                    }
                    JointRule::Square { member: mm } if mm == member.id => return square,
                    JointRule::Plane { member: mm, normal } if mm == member.id => {
                        let n = if normal.dot(d) < 0.0 { -normal } else { normal };
                        return EndCut { plane: Plane::new(node_pos, n), extra: vec![], also: vec![], kind: CutKind::Custom };
                    }
                    JointRule::Plumb { member: mm } if mm == member.id => {
                        // Vertical plane through the node, facing out along the member's run.
                        return match v3(d.x, d.y, 0.0).try_normalized() {
                            Some(n) => EndCut { plane: Plane::new(node_pos, n), extra: vec![], also: vec![], kind: CutKind::Custom },
                            None => square, // a vertical member: plumb is square
                        };
                    }
                    JointRule::Through { member: mm } if mm == member.id => {
                        return self.extend_through(&others, r0, d, s_node, square, &window, |_| true);
                    }
                    JointRule::Miter { a, b } if a == member.id || b == member.id => {
                        let other = if a == member.id { b } else { a };
                        let od = j.get(other).map(|o| o.dir).unwrap_or(-d);
                        return match (d - od).try_normalized() {
                            Some(n) if d.dot(od) > -1.0 + 1e-9 => {
                                EndCut { plane: Plane::new(node_pos, n), extra: vec![], also: vec![], kind: CutKind::Miter { with: other } }
                            }
                            _ => square,
                        };
                    }
                    _ => {}
                }
            }
        }

        // Butt against every through member / higher-ranked end the centroid
        // ray enters near the node. The first face hit is the primary cut;
        // further faces make a compound cut (e.g. a web under both chords at an apex).
        let mut hits: Vec<(f64, MemberId, usize)> = vec![];
        for o in &others {
            let om = self.model.member(o.member);
            if o.incidence.is_end() && !outranks(om, member) {
                continue;
            }
            if let Some(h) = self.envs[o.member.idx()].raycast(r0, d) {
                if (h.s_enter - s_node).abs() <= window(o.member) {
                    hits.push((h.s_enter, o.member, h.enter));
                }
            }
        }
        hits.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        if let Some(&(_, against, face)) = hits.first() {
            let extra = hits[1..].iter().map(|&(_, m, f)| self.envs[m.idx()].plane(f, true)).collect();
            let also = hits[1..].iter().map(|&(_, m, _)| m).collect();
            return EndCut { plane: self.envs[against.idx()].plane(face, true), extra, also, kind: CutKind::Butt { against } };
        }

        // An in-line splice (another member continues straight on from this
        // node) is a square butt joint at the node: neither piece runs past it.
        let spliced = j.members.iter().any(|o| o.member != member.id && o.incidence.is_end() && o.dir.dot(d) < -1.0 + 1e-6);
        if spliced {
            return square;
        }

        // Otherwise run through lower-ranked ends.
        self.extend_through(&others, r0, d, s_node, square, &window, |o| {
            o.incidence.is_end() && outranks(member, self.model.member(o.member))
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn extend_through(
        &self,
        others: &[MemberAtNode],
        r0: Vec3,
        d: Vec3,
        s_node: f64,
        square: EndCut,
        window: &dyn Fn(MemberId) -> f64,
        eligible: impl Fn(&MemberAtNode) -> bool,
    ) -> EndCut {
        let mut best: Option<(f64, usize, MemberId)> = None;
        let mut past = vec![];
        for o in others.iter().filter(|o| o.incidence.is_end() && eligible(o)) {
            if let Some(h) = self.envs[o.member.idx()].raycast(r0, d) {
                if h.s_exit > s_node + 1e-9 && h.s_exit - s_node <= window(o.member) {
                    past.push(o.member);
                    if best.is_none_or(|(s, _, _)| h.s_exit > s) {
                        best = Some((h.s_exit, h.exit, o.member));
                    }
                }
            }
        }
        match best {
            Some((_, face, m)) => EndCut { plane: self.envs[m.idx()].plane(face, false), extra: vec![], also: vec![], kind: CutKind::Through { past } },
            None => square,
        }
    }
}
