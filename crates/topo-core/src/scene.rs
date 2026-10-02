//! A scene: bodies, the named nodes of them, members between nodes, and
//! relations between nodes; solved, and then written into a model.
//!
//! This is how things are placed. A body is a rigid thing (a frame): what is
//! in it is given in its own coordinates. It is fixed where it is put, or
//! free, and then placed only by relations. A node is a named point: fixed in
//! a body, or (made by a relation) on a member's face, with two numbers of its
//! own bounded by the face. Relations are between nodes: two nodes are one
//! (`coincide`), a node is at a place in the world (`datum`), a member rests on
//! another where they cross (`rest`: a node on the faces of both). Nothing is
//! connected or placed by where it happens to be.
//!
//! What a generator has already written into the model can be brought in by
//! name: a node of the model as a node fixed in the world, a member as a
//! member fixed there. Relations to them are relations like any other, and
//! writing the scene joins to them rather than making them again.
//!
//! Solving finds every body's pose and every node's numbers. The model is
//! then written from the solution: the bodies' groups at their poses, a model
//! node for each node (nodes made one by `coincide` are one model node), the
//! members between them, and the nodes on faces as attachments. The solve is
//! kept in the model (`Model::solves`), to be shown.

use crate::ids::{GroupId, MemberId, NodeId};
use crate::math::{Frame, Vec3};
use crate::model::{face_plane, Face, MemberAxes, MemberSpec, Model, Solved, SolvedConstraint, SolvedFreedom, SolvedNode, SolvedNumber, SolvedVariable};
use topo_solve as ts;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BodyRef(pub usize);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeRef(pub usize);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemberRef(pub usize);

struct Body {
    name: String,
    kind: String,
    frame: ts::FrameId,
    /// Where it was put (for a fixed body, where it is).
    start: Frame,
    parent: Option<GroupId>,
    /// What a generator already wrote into the model for it, where it was put; moved with it.
    content: Option<Content>,
}

/// What a generator wrote into the model for a body: its group (and those in it), nodes and members.
#[derive(Debug, Clone, Default)]
pub struct Content {
    pub group: GroupId,
    pub groups: Vec<GroupId>,
    pub nodes: Vec<NodeId>,
    pub members: Vec<MemberId>,
}

struct Node {
    name: String,
    /// The solver's node(s): one, or (on the faces of two members) one on each face.
    of: Vec<ts::NodeId>,
    /// A node of the model, brought in: written as itself.
    model: Option<NodeId>,
    /// Where it is in its body, for a node fixed in one.
    fixed: Option<(usize, Vec3)>,
    /// The members it is on the face of.
    faces: Vec<(usize, Face)>,
    /// A point of a member of the model, this far along it: written as a node in its path.
    along: Option<(MemberId, f64)>,
}

struct Member {
    /// Its body; none for a member of the model, brought in (fixed in the world).
    body: Option<usize>,
    /// A member of the model, brought in: written as itself.
    model: Option<MemberId>,
    path: Vec<usize>,
    /// With its depth direction in the body's coordinates.
    spec: MemberSpec,
    /// Its axes in the body's coordinates.
    axes: MemberAxes,
    /// What to call it in what is said about it.
    label: String,
}

/// What writing a solved scene into a model made: for each body its group, for each node its
/// model node, for each member its model member; and what the solve left open.
#[derive(Debug, Clone)]
pub struct Written {
    pub groups: Vec<GroupId>,
    pub nodes: Vec<NodeId>,
    /// The nodes' names, as `nodes`.
    pub names: Vec<String>,
    pub members: Vec<MemberId>,
    pub open: Vec<String>,
}

pub struct Scene {
    name: String,
    sys: ts::System,
    bodies: Vec<Body>,
    nodes: Vec<Node>,
    members: Vec<Member>,
    /// Nodes made one by `coincide` (each points at another of its kind, or at itself).
    same: Vec<usize>,
    /// The `rest` relations: upper member, lower member, their node, and its name.
    rests: Vec<(usize, usize, usize, String)>,
}

fn arr(v: Vec3) -> [f64; 3] {
    [v.x, v.y, v.z]
}
fn vec(a: [f64; 3]) -> Vec3 {
    Vec3::new(a[0], a[1], a[2])
}
fn pose(f: &Frame) -> ts::Pose {
    ts::Pose { r: [[f.x.x, f.y.x, f.z.x], [f.x.y, f.y.y, f.z.y], [f.x.z, f.y.z, f.z.z]], t: arr(f.origin) }
}
fn frame(p: &ts::Pose) -> Frame {
    let col = |k: usize| Vec3::new(p.r[0][k], p.r[1][k], p.r[2][k]);
    Frame { origin: vec(p.t), x: col(0), y: col(1), z: col(2) }
}

impl Scene {
    /// A scene; `name` names its solve.
    pub fn new(name: &str) -> Scene {
        Scene { name: name.into(), sys: ts::System::new(), bodies: vec![], nodes: vec![], members: vec![], same: vec![], rests: vec![] }
    }

    /// A body (written as a group named `name`, of `kind`, in `parent`), put at `start`: fixed
    /// there, or (`free`) starting there and placed by relations, kept upright unless they say otherwise.
    pub fn body(&mut self, name: &str, kind: &str, start: Frame, free: bool, parent: Option<GroupId>) -> BodyRef {
        let joint = if free { ts::Joint::Free } else { ts::Joint::Fixed };
        let f = self.sys.frame(name, ts::WORLD, joint, pose(&start));
        if free {
            // (Hints: its z is up, and it is turned on plan as it was put. Relations override them.)
            self.sys.constrain(&format!("{name} upright"), ts::Tier::Default, ts::Kind::Aligned(ts::Dir(f, [0.0, 0.0, 1.0]), ts::Dir(ts::WORLD, [0.0, 0.0, 1.0])));
            self.sys.constrain(&format!("{name} turned as it was put"), ts::Tier::Default, ts::Kind::Aligned(ts::Dir(f, [1.0, 0.0, 0.0]), ts::Dir(ts::WORLD, arr(start.x))));
        }
        self.bodies.push(Body { name: name.into(), kind: kind.into(), frame: f, start, parent, content: None });
        BodyRef(self.bodies.len() - 1)
    }

    /// A body for what a generator has already written into the model, where it put it
    /// (`start`): fixed there, or free, and then moved, all of it, to where the solve puts it.
    pub fn body_written(&mut self, name: &str, start: Frame, free: bool, content: Content) -> BodyRef {
        let b = self.body(name, "", start, free, None);
        self.bodies[b.0].content = Some(content);
        b
    }

    /// The body whose written content has this member of the model.
    pub fn body_with(&self, m: MemberId) -> Option<BodyRef> {
        self.bodies.iter().position(|b| b.content.as_ref().is_some_and(|c| c.members.contains(&m))).map(BodyRef)
    }

    /// Where a point of the world (as things were put) is in a body: none for the world.
    fn local(&self, body: Option<BodyRef>, p: Vec3) -> (ts::FrameId, Vec3) {
        match body {
            Some(b) => (self.bodies[b.0].frame, self.bodies[b.0].start.to_local(p)),
            None => (ts::WORLD, p),
        }
    }

    /// A point of a member of the model, `at` along it from its start, as a node (fixed in the
    /// member's body, if it is in one): written as a node in the member's path.
    pub fn node_along(&mut self, name: &str, model: &Model, m: MemberId, at: f64) -> Result<NodeRef, String> {
        if self.node_named(name).is_some() {
            return Err(format!("two nodes named {name}"));
        }
        let axes = model.member_axes(m);
        if at < -1e-9 || at > axes.length + 1e-9 {
            return Err(format!("{name}: {at} m is not along {} ({:.3} m long)", model.member_path(m), axes.length));
        }
        let body = self.body_with(m);
        let (frame, local) = self.local(body, axes.origin + axes.x * at);
        let id = self.sys.node(name, ts::Point::at(frame, arr(local)));
        self.nodes.push(Node { name: name.into(), of: vec![id], model: None, fixed: body.map(|b| (b.0, local)), faces: vec![], along: Some((m, at)) });
        self.same.push(self.nodes.len() - 1);
        Ok(NodeRef(self.nodes.len() - 1))
    }

    /// A node fixed in a body, at `at` in its coordinates.
    pub fn node(&mut self, name: &str, body: BodyRef, at: Vec3) -> Result<NodeRef, String> {
        if self.node_named(name).is_some() {
            return Err(format!("two nodes named {name}"));
        }
        let id = self.sys.node(name, ts::Point::at(self.bodies[body.0].frame, arr(at)));
        self.nodes.push(Node { name: name.into(), of: vec![id], model: None, fixed: Some((body.0, at)), faces: vec![], along: None });
        self.same.push(self.nodes.len() - 1);
        Ok(NodeRef(self.nodes.len() - 1))
    }

    /// A node of the model (written by a generator), brought in under a name: fixed in the body
    /// whose content it is (moving with it), or else in the world.
    pub fn import_node(&mut self, name: &str, model: &Model, n: NodeId) -> Result<NodeRef, String> {
        if self.node_named(name).is_some() {
            return Err(format!("two nodes named {name}"));
        }
        let body = self.bodies.iter().position(|b| b.content.as_ref().is_some_and(|c| c.nodes.contains(&n))).map(BodyRef);
        let (frame, local) = self.local(body, model.pos(n));
        let id = self.sys.node(name, ts::Point::at(frame, arr(local)));
        self.nodes.push(Node { name: name.into(), of: vec![id], model: Some(n), fixed: body.map(|b| (b.0, local)), faces: vec![], along: None });
        self.same.push(self.nodes.len() - 1);
        Ok(NodeRef(self.nodes.len() - 1))
    }

    /// A member of the model (written by a generator), brought in, for relations to it
    /// (something resting on it): in the body whose content it is (moving with it), or else
    /// fixed in the world.
    pub fn import_member(&mut self, model: &Model, m: MemberId, label: &str) -> MemberRef {
        if let Some(i) = self.members.iter().position(|x| x.model == Some(m)) {
            return MemberRef(i);
        }
        let mm = model.member(m);
        let spec = MemberSpec { name: mm.name.clone(), ..MemberSpec::new(&mm.role, mm.section, mm.material).anchor(mm.anchor) };
        let body = self.body_with(m);
        let a = model.member_axes(m);
        let axes = match body {
            Some(b) => {
                let f = &self.bodies[b.0].start;
                MemberAxes { origin: f.to_local(a.origin), x: f.dir_to_local(a.x), u: f.dir_to_local(a.u), v: f.dir_to_local(a.v), length: a.length }
            }
            None => a,
        };
        self.members.push(Member { body: body.map(|b| b.0), model: Some(m), path: vec![], spec, axes, label: label.into() });
        MemberRef(self.members.len() - 1)
    }

    /// Where a member's body is (its solver frame, and where it was put): the world, for one brought in.
    fn body_of(&self, m: usize) -> (ts::FrameId, Frame) {
        match self.members[m].body {
            Some(b) => (self.bodies[b].frame, self.bodies[b].start),
            None => (ts::WORLD, Frame::WORLD),
        }
    }

    pub fn node_named(&self, name: &str) -> Option<NodeRef> {
        self.nodes.iter().position(|n| n.name == name).map(NodeRef)
    }

    pub fn node_names(&self) -> impl Iterator<Item = &str> {
        self.nodes.iter().map(|n| n.name.as_str())
    }

    /// A member of a body, between nodes fixed in it (its depth direction, if given, in the body's coordinates).
    pub fn member(&mut self, body: BodyRef, path: &[NodeRef], spec: MemberSpec, label: &str) -> Result<MemberRef, String> {
        let at = |n: &NodeRef| match self.nodes[n.0].fixed {
            Some((b, p)) if b == body.0 => Ok(p),
            _ => Err(format!("{} is not a node of {}", self.nodes[n.0].name, self.bodies[body.0].name)),
        };
        let (a, b) = (at(&path[0])?, at(path.last().ok_or("a member needs two nodes")?)?);
        for n in path {
            at(n)?;
        }
        let axes = MemberAxes::new(a, b, spec.depth_dir);
        self.members.push(Member { body: Some(body.0), model: None, path: path.iter().map(|n| n.0).collect(), spec, axes, label: label.into() });
        Ok(MemberRef(self.members.len() - 1))
    }

    fn root(&mut self, mut n: usize) -> usize {
        while self.same[n] != n {
            self.same[n] = self.same[self.same[n]];
            n = self.same[n];
        }
        n
    }

    /// Two nodes are one: the same point, and one node of the model.
    pub fn coincide(&mut self, name: &str, a: NodeRef, b: NodeRef) {
        let (pa, pb) = (self.sys.at(self.nodes[a.0].of[0]), self.sys.at(self.nodes[b.0].of[0]));
        self.sys.constrain(name, ts::Tier::Hard, ts::Kind::Coincident(pa, pb));
        let (ra, rb) = (self.root(a.0), self.root(b.0));
        self.same[ra.max(rb)] = ra.min(rb);
    }

    /// A node is at this place in the world.
    pub fn datum(&mut self, name: &str, n: NodeRef, at: Vec3) {
        let p = self.sys.at(self.nodes[n.0].of[0]);
        self.sys.constrain(name, ts::Tier::Hard, ts::Kind::Coincident(p, ts::Point::at(ts::WORLD, arr(at))));
    }

    /// One member rests on another where they cross: a node (named `name`) on the underside
    /// of the one and on the top of the other. On each face it has two numbers, bounded by
    /// the face; where in the patch they share is a hint (the middle of each, across).
    pub fn rest(&mut self, name: &str, upper: MemberRef, lower: MemberRef, model: &Model) -> Result<NodeRef, String> {
        let face_of = |s: &Scene, m: usize, toward: Vec3| -> (Face, crate::model::FacePlane) {
            let mm = &s.members[m];
            let local = s.body_of(m).1.dir_to_local(toward);
            let faces = [Face::PlusU, Face::MinusU, Face::PlusV, Face::MinusV].map(|f| (f, face_plane(&mm.axes, model.section(mm.spec.section), &mm.spec.anchor, f)));
            faces.into_iter().max_by(|a, b| a.1.normal.dot(local).total_cmp(&b.1.normal.dot(local))).unwrap()
        };
        let (fu, pu) = face_of(self, upper.0, -Vec3::Z);
        let (fl, pl) = face_of(self, lower.0, Vec3::Z);
        // To start from: where the middles of the two faces come nearest each other (in the world, as put).
        let (su, sl) = (&self.body_of(upper.0).1, &self.body_of(lower.0).1);
        let (ou, au, ol, al) = (su.to_world(pu.origin), su.dir_to_world(pu.along), sl.to_world(pl.origin), sl.dir_to_world(pl.along));
        let (w, b) = (ou - ol, au.dot(al));
        let (tu, tl) = if 1.0 - b * b < 1e-9 {
            (pu.length / 2.0, (ou + au * (pu.length / 2.0) - ol).dot(al))
        } else {
            let (d, e) = (au.dot(w), al.dot(w));
            ((b * e - d) / (1.0 - b * b), (e - b * d) / (1.0 - b * b))
        };
        let on = |s: &mut Scene, m: usize, p: &crate::model::FacePlane, start: f64, which: &str| {
            let body = s.body_of(m).0;
            let along = s.sys.scalar(body, &format!("{name}: along {which}"), start.clamp(0.0, p.length));
            let across = s.sys.scalar(body, &format!("{name}: across {which}"), 0.0);
            s.sys.bound(along, 0.0, p.length);
            s.sys.bound(across, -p.half_width, p.half_width);
            s.sys.constrain(&format!("{name}: in the middle of {which}, across"), ts::Tier::Default, ts::Kind::equal(across, 0.0));
            s.sys.node(&format!("{name} ({which})"), ts::Point::at(body, arr(p.origin)).plus(along, arr(p.along)).plus(across, arr(p.across)))
        };
        let (upper_name, lower_name) = (format!("the underside of {}", self.members[upper.0].label), format!("the top of {}", self.members[lower.0].label));
        let a = on(self, upper.0, &pu, tu, &upper_name);
        let b = on(self, lower.0, &pl, tl, &lower_name);
        let (pa, pb) = (self.sys.at(a), self.sys.at(b));
        self.sys.constrain(&format!("{name}: on both"), ts::Tier::Hard, ts::Kind::Coincident(pa, pb));
        self.nodes.push(Node { name: name.into(), of: vec![a, b], model: None, fixed: None, faces: vec![(upper.0, fu), (lower.0, fl)], along: None });
        self.same.push(self.nodes.len() - 1);
        let n = self.nodes.len() - 1;
        self.rests.push((upper.0, lower.0, n, name.into()));
        Ok(NodeRef(n))
    }

    /// Solves, and writes the result into `model`. Hard relations that cannot all hold are
    /// refused: nothing is written, and the error names them.
    pub fn write(mut self, model: &mut Model) -> Result<Written, String> {
        let report = self.sys.solve();
        if report.refused {
            let mut said = vec![];
            for (upper, lower, n, name) in &self.rests {
                // A rest that cannot hold: how far apart they are where they cross.
                let (a, b) = (self.sys.node_at(self.nodes[*n].of[0]), self.sys.node_at(self.nodes[*n].of[1]));
                if ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt() > 1e-6 {
                    let dz = a[2] - b[2];
                    let (u, l) = (&self.members[*upper].label, &self.members[*lower].label);
                    said.push(format!("{name}: {u} does not rest on {l}: where they cross, its underside is {:.1} mm {} the top of {l}", dz.abs() * 1000.0, if dz > 0.0 { "above" } else { "below" }));
                }
            }
            if said.is_empty() {
                for c in &report.conflicts {
                    said.push(format!("{} cannot all hold: {}", c.frames.join(", "), c.constraints.iter().map(|g| format!("{} ({:.1} mm off)", g.constraint, g.size * 1000.0)).collect::<Vec<_>>().join("; ")));
                }
            }
            return Err(said.join(". "));
        }
        // What generators wrote for bodies is moved to where the solve put them; other bodies get their groups.
        let mut groups = vec![];
        for b in &self.bodies {
            let at = frame(&self.sys.in_world(b.frame));
            match &b.content {
                Some(c) => {
                    let moved = |p: Vec3| at.to_world(b.start.to_local(p));
                    let turned = |d: Vec3| at.dir_to_world(b.start.dir_to_local(d));
                    if (moved(Vec3::ZERO) - Vec3::ZERO).norm() > 1e-12 || (turned(Vec3::X) - Vec3::X).norm() > 1e-12 || (turned(Vec3::Y) - Vec3::Y).norm() > 1e-12 {
                        for &n in &c.nodes {
                            model.nodes[n.idx()].pos = moved(model.nodes[n.idx()].pos);
                        }
                        for &g in &c.groups {
                            let f = model.groups[g.idx()].frame;
                            model.groups[g.idx()].frame = Frame { origin: moved(f.origin), x: turned(f.x), y: turned(f.y), z: turned(f.z) };
                        }
                        for &m in &c.members {
                            model.members[m.idx()].depth_dir = model.members[m.idx()].depth_dir.map(turned);
                        }
                        model.reset_index();
                    }
                    groups.push(c.group);
                }
                None => groups.push(model.add_group(&b.name, &b.kind, at, b.parent)),
            }
        }
        // A model node for each node, one for nodes made one (a node brought in is itself; nodes
        // made one with it are it). None is joined to another because of where it is.
        let mut written: Vec<Option<NodeId>> = vec![None; self.nodes.len()];
        for i in 0..self.nodes.len() {
            if let Some(n) = self.nodes[i].model {
                let root = self.root(i);
                written[root] = Some(n);
            }
        }
        // (So is a point along a member: nodes made one with it are that node of the member.)
        for i in 0..self.nodes.len() {
            let root = self.root(i);
            if let (None, Some((m, at))) = (written[root], self.nodes[i].along) {
                let n = model.node_along(m, at).map_err(|e| format!("{}: {e}", self.nodes[i].name))?;
                model.nodes[n.idx()].name.get_or_insert(self.nodes[i].name.clone());
                written[root] = Some(n);
            }
        }
        let mut nodes = vec![];
        for i in 0..self.nodes.len() {
            let root = self.root(i);
            let n = match written[root] {
                Some(n) => n,
                None => {
                    let n = model.add_node(vec(self.sys.node_at(self.nodes[root].of[0])));
                    model.nodes[n.idx()].name.get_or_insert(self.nodes[root].name.clone());
                    written[root] = Some(n);
                    n
                }
            };
            nodes.push(n);
        }
        let mut members = vec![];
        for m in &self.members {
            // (A member a generator wrote is itself; it has been moved with its body.)
            if let Some(id) = m.model {
                members.push(id);
                continue;
            }
            let body = m.body.expect("a member is of a body, or brought in");
            let pose = self.sys.in_world(self.bodies[body].frame);
            let r = frame(&pose);
            let mut spec = m.spec.clone();
            spec.depth_dir = spec.depth_dir.map(|d| r.dir_to_world(d));
            spec.group = Some(groups[body]);
            let path: Vec<NodeId> = m.path.iter().map(|&n| nodes[n]).collect();
            members.push(model.add_member(&path, &spec));
        }
        for (i, n) in self.nodes.iter().enumerate() {
            for &(m, f) in &n.faces {
                model.attach(nodes[i], members[m], f).map_err(|e| format!("{}: {e}", n.name))?;
            }
        }
        if !self.bodies.is_empty() && report.unknowns > 0 {
            model.solves.push(solved(&self.name, &self.sys.explain(&report)));
        }
        let names = self.nodes.iter().map(|n| n.name.clone()).collect();
        Ok(Written { groups, nodes, names, members, open: report.open })
    }
}

/// A solve, as the model keeps it to be shown.
pub fn solved(name: &str, e: &ts::Explained) -> Solved {
    let tier = |t: usize| ["hard", "measured", "hint", "weight"][t].to_string();
    Solved {
        name: name.into(),
        nodes: (e.nodes.iter())
            .map(|n| SolvedNode {
                name: n.name.clone(),
                of: n.of.clone(),
                kind: n.kind.into(),
                at: vec(n.at),
                numbers: n.numbers.iter().map(|x| SolvedNumber { name: x.name.clone(), value: x.value, dir: vec(x.dir), bounds: x.bounds, settled: x.settled.map_or("open".into(), tier) }).collect(),
            })
            .collect(),
        variables: (e.variables.iter())
            .map(|v| SolvedVariable {
                name: v.name.clone(),
                kind: v.kind.into(),
                at: vec(v.at),
                value: v.value,
                node: v.node.clone(),
                freedoms: v.freedoms.iter().map(|f| SolvedFreedom { what: f.what.clone(), dir: vec(f.dir), turn: f.turn, settled: f.settled.map_or("open".into(), tier) }).collect(),
            })
            .collect(),
        constraints: (e.constraints.iter())
            .map(|c| SolvedConstraint { name: c.name.clone(), tier: tier(c.tier), kind: c.kind.into(), gap: c.gap, room: c.room, points: c.points.iter().map(|p| vec(*p)).collect(), nodes: c.nodes.clone(), touches: c.touches.clone() })
            .collect(),
    }
}
