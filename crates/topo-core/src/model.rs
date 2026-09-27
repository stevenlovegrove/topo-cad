//! The model IR and its builder API.

use crate::connection::Connection;
use crate::ids::*;
use crate::loads::{Load, LoadCase, LoadKind, Support};
use crate::material::Material;
use crate::math::{Frame, Vec2, Vec3, LEN_TOL};
use crate::section::Section;
use crate::units::UnitSystem;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub pos: Vec3,
    pub name: Option<String>,
}

/// Which point of the section's bounding box lies on the member axis.
/// `u`, `v` ∈ [-½, ½]; `(0, 0)` is centred, `(0, -½)` puts the axis on the
/// bottom (−v) face so the body extends toward +v. `offset` is an additional
/// shift of the axis point in section coordinates (m).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    pub u: f64,
    pub v: f64,
    pub offset: Vec2,
}

impl Anchor {
    pub const CENTER: Anchor = Anchor { u: 0.0, v: 0.0, offset: Vec2::ZERO };

    /// Anchor such that the body extends from the axis toward the world
    /// directions `u_side` / `v_side` (projected on the member's u / v axes);
    /// `None` centres the body on that axis.
    pub fn body_toward(axes: &MemberAxes, u_side: Option<Vec3>, v_side: Option<Vec3>) -> Anchor {
        let side = |axis: Vec3, dir: Option<Vec3>| match dir {
            Some(d) if axis.dot(d).abs() > 1e-9 => -0.5 * axis.dot(d).signum(),
            Some(_) => panic!("body_toward: direction is perpendicular to the section axis"),
            None => 0.0,
        };
        Anchor { u: side(axes.u, u_side), v: side(axes.v, v_side), offset: Vec2::ZERO }
    }
}

/// A member's local axes: `x` along the axis, `v` depth direction, `u = v × x`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MemberAxes {
    pub origin: Vec3,
    pub x: Vec3,
    pub u: Vec3,
    pub v: Vec3,
    pub length: f64,
}

impl MemberAxes {
    /// Canonical member frame: x along the grain (first → last node), z the
    /// depth direction (`depth_dir` projected; default world up, or north for
    /// a vertical member), y = z × x across the thickness.
    pub fn new(start: Vec3, end: Vec3, depth_dir: Option<Vec3>) -> MemberAxes {
        MemberAxes::in_frame(start, end, depth_dir, &Frame::WORLD)
    }

    /// As [`new`](Self::new), with the default depth direction taken from a
    /// parent frame: its z, or its y for a member parallel to that z.
    pub fn in_frame(start: Vec3, end: Vec3, depth_dir: Option<Vec3>, parent: &Frame) -> MemberAxes {
        let d = end - start;
        let length = d.norm();
        let x = d / length;
        let hint = depth_dir.unwrap_or(if x.dot(parent.z).abs() > 0.99 { parent.y } else { parent.z });
        let v = hint.reject(x).try_normalized().unwrap_or_else(|| x.any_perpendicular());
        let u = v.cross(x);
        MemberAxes { origin: start, x, u, v, length }
    }
    /// Frame with (x, y, z) = (x, u, v).
    pub fn frame(&self) -> Frame {
        Frame { origin: self.origin, x: self.x, y: self.u, z: self.v }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Member {
    pub id: MemberId,
    /// Assigned piece mark (e.g. `H1`); schedules assign marks when absent.
    pub mark: Option<String>,
    /// Free-form role tag (`stud`, `plate`, `joist`, `extrusion`, …).
    pub role: String,
    /// Junction precedence: higher-priority members run through lower ones at
    /// L-junctions. Ties are broken by lower id.
    pub priority: i32,
    /// Ordered collinear nodes; interior nodes are connection points.
    pub path: Vec<NodeId>,
    pub section: SectionId,
    pub material: MaterialId,
    /// World direction for the section's depth (v) axis; see `MemberAxes::new`.
    pub depth_dir: Option<Vec3>,
    pub anchor: Anchor,
    pub group: Option<GroupId>,
    /// Stable name within its group (e.g. `bottom_chord.F-H1`, `cap_plate`),
    /// used to refer to it from measurements; see `Model::member_path`.
    #[serde(default)]
    pub name: Option<String>,
}

impl Member {
    pub fn start(&self) -> NodeId {
        self.path[0]
    }
    pub fn end(&self) -> NodeId {
        *self.path.last().unwrap()
    }
}

/// Properties shared when creating members.
#[derive(Clone, Debug, PartialEq)]
pub struct MemberSpec {
    pub role: String,
    pub priority: i32,
    pub section: SectionId,
    pub material: MaterialId,
    pub depth_dir: Option<Vec3>,
    pub anchor: Anchor,
    pub mark: Option<String>,
    pub group: Option<GroupId>,
    pub name: Option<String>,
}

impl MemberSpec {
    pub fn new(role: &str, section: SectionId, material: MaterialId) -> MemberSpec {
        MemberSpec {
            role: role.into(),
            priority: 0,
            section,
            material,
            depth_dir: None,
            anchor: Anchor::CENTER,
            mark: None,
            group: None,
            name: None,
        }
    }
    pub fn named(mut self, name: &str) -> Self {
        self.name = Some(name.into());
        self
    }
    pub fn priority(mut self, p: i32) -> Self {
        self.priority = p;
        self
    }
    pub fn depth_dir(mut self, d: Vec3) -> Self {
        self.depth_dir = Some(d);
        self
    }
    pub fn anchor(mut self, a: Anchor) -> Self {
        self.anchor = a;
        self
    }
    pub fn mark(mut self, m: &str) -> Self {
        self.mark = Some(m.into());
        self
    }
    pub fn group(mut self, g: GroupId) -> Self {
        self.group = Some(g);
        self
    }
}

/// Drawing intent attached to a group, in group-local coordinates.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Annotation {
    /// Linear dimension between `a` and `b`, offset toward local direction
    /// `side`, on tier `tier` (0 = closest to the geometry).
    Dim { a: Vec3, b: Vec3, side: Vec3, tier: u32, text: Option<String> },
    /// Free text at a point.
    Note { at: Vec3, text: String },
    /// Rectangle outline (e.g. rough opening) with a label.
    Region { min: Vec3, max: Vec3, label: String },
    /// Double-headed span arrow with a callout (e.g. joist size and spacing).
    Span { a: Vec3, b: Vec3, text: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub id: GroupId,
    pub name: String,
    /// `wall`, `floor`, `roof`, `frame`, …
    pub kind: String,
    /// Local frame. For walls: x along the wall, y toward the interior, z up.
    pub frame: Frame,
    pub parent: Option<GroupId>,
    pub members: Vec<MemberId>,
    pub annotations: Vec<Annotation>,
    pub props: BTreeMap<String, String>,
    /// Named directions in this group's frame (local vectors), e.g. a wall's
    /// `inside`; resolved by `Model::direction`.
    #[serde(default)]
    pub directions: BTreeMap<String, Vec3>,
}

/// Per-node override of automatic junction resolution.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum JointRule {
    /// `member` is cut against the face of `against`.
    Butt { member: MemberId, against: MemberId },
    /// `member` extends through all other ending members at this node.
    Through { member: MemberId },
    /// `member` gets a square cut at the node.
    Square { member: MemberId },
    /// `a` and `b` are mitred to each other.
    Miter { a: MemberId, b: MemberId },
    /// `member` is cut on the plane through the node with this outward
    /// normal (e.g. horizontal for a plumb-cut rafter tail).
    Plane { member: MemberId, normal: Vec3 },
    /// `member` gets a plumb (vertical) cut at the node.
    Plumb { member: MemberId },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConnectionUse {
    /// Member being fastened (normally the one that ends at the node).
    pub member: MemberId,
    pub to: Option<MemberId>,
    pub connection: ConnectionId,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct JointSpec {
    pub node: NodeId,
    pub rules: Vec<JointRule>,
    pub connections: Vec<ConnectionUse>,
}

/// Face-to-face fastening of two parallel, adjacent members along their
/// length (jack to king stud, plies of a built-up column). These share no node,
/// so the relation is explicit; analysis may treat bonded members compositely.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bond {
    pub a: MemberId,
    pub b: MemberId,
    pub connection: ConnectionId,
}

/// Editions of the standards the model's loads are based on. The load
/// combinations must match the source of the loads: ASCE 7-22 ground snow
/// loads are strength-level (ASD uses 0.7S); ASCE 7-16 values are not.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Standards {
    /// `7-16` or `7-22`.
    pub asce7: String,
}

impl Default for Standards {
    fn default() -> Self {
        Standards { asce7: "7-16".into() }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub name: String,
    pub number: String,
    pub client: String,
    pub address: String,
    pub designer: String,
    pub date: String,
    /// Design basis statements printed on drawings (codes, loads, materials).
    pub design_basis: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ModelError {
    NotOnMember { member: MemberId, distance: f64 },
    OutsideMember { member: MemberId, t: f64 },
    NoIntersection { a: MemberId, b: MemberId, gap: f64 },
    Parallel { a: MemberId, b: MemberId },
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelError::NotOnMember { member, distance } => {
                write!(f, "point is {distance:.4} m off the axis of {member}")
            }
            ModelError::OutsideMember { member, t } => {
                write!(f, "point lies outside {member} (t = {t:.4} m)")
            }
            ModelError::NoIntersection { a, b, gap } => {
                write!(f, "axes of {a} and {b} do not meet (gap {gap:.4} m)")
            }
            ModelError::Parallel { a, b } => write!(f, "{a} and {b} are parallel"),
        }
    }
}

impl std::error::Error for ModelError {}

/// Spatial hash of node positions for `node_at` lookups (not serialized).
#[derive(Clone, Debug, Default)]
struct NodeIndex {
    cells: HashMap<(i64, i64, i64), Vec<NodeId>>,
    indexed: usize,
}

const CELL: f64 = 10.0 * LEN_TOL;

fn cell_of(p: Vec3) -> (i64, i64, i64) {
    ((p.x / CELL).floor() as i64, (p.y / CELL).floor() as i64, (p.z / CELL).floor() as i64)
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Model {
    pub info: ProjectInfo,
    /// Which editions of the design standards the loads follow.
    #[serde(default)]
    pub standards: Standards,
    pub units: UnitSystem,
    pub nodes: Vec<Node>,
    pub members: Vec<Member>,
    pub groups: Vec<Group>,
    pub sections: Vec<Section>,
    pub materials: Vec<Material>,
    pub connections: Vec<Connection>,
    pub joints: Vec<JointSpec>,
    pub bonds: Vec<Bond>,
    pub supports: Vec<Support>,
    pub load_cases: Vec<LoadCase>,
    pub loads: Vec<Load>,
    /// The drawing set, in sheet order; `None` for the standard set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheets: Option<Vec<crate::sheets::SheetSpec>>,
    #[serde(skip)]
    index: NodeIndex,
}

impl Model {
    pub fn new(name: &str) -> Model {
        Model { info: ProjectInfo { name: name.into(), ..Default::default() }, ..Default::default() }
    }

    // ----- accessors -------------------------------------------------------

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.idx()]
    }
    pub fn pos(&self, id: NodeId) -> Vec3 {
        self.nodes[id.idx()].pos
    }
    pub fn member(&self, id: MemberId) -> &Member {
        &self.members[id.idx()]
    }
    pub fn member_mut(&mut self, id: MemberId) -> &mut Member {
        &mut self.members[id.idx()]
    }
    pub fn section(&self, id: SectionId) -> &Section {
        &self.sections[id.idx()]
    }
    pub fn material(&self, id: MaterialId) -> &Material {
        &self.materials[id.idx()]
    }
    pub fn connection(&self, id: ConnectionId) -> &Connection {
        &self.connections[id.idx()]
    }
    pub fn group(&self, id: GroupId) -> &Group {
        &self.groups[id.idx()]
    }
    pub fn group_mut(&mut self, id: GroupId) -> &mut Group {
        &mut self.groups[id.idx()]
    }
    pub fn joint(&self, node: NodeId) -> Option<&JointSpec> {
        self.joints.iter().find(|j| j.node == node)
    }
    pub fn joint_mut(&mut self, node: NodeId) -> &mut JointSpec {
        if let Some(i) = self.joints.iter().position(|j| j.node == node) {
            &mut self.joints[i]
        } else {
            self.joints.push(JointSpec { node, ..Default::default() });
            self.joints.last_mut().unwrap()
        }
    }

    /// Axes of a member from its first to last path node.
    /// A member's canonical frame (x with the grain, y across the thickness,
    /// z across the depth). Without an explicit depth direction it follows
    /// the member's group frame.
    pub fn member_axes(&self, id: MemberId) -> MemberAxes {
        let m = self.member(id);
        let parent = m.group.map(|g| self.groups[g.idx()].frame).unwrap_or(Frame::WORLD);
        MemberAxes::in_frame(self.pos(m.start()), self.pos(m.end()), m.depth_dir, &parent)
    }

    /// Axes a member *would* have between two points (for computing anchors
    /// before creating it).
    pub fn axes_between(&self, a: Vec3, b: Vec3, depth_dir: Option<Vec3>) -> MemberAxes {
        MemberAxes::new(a, b, depth_dir)
    }

    /// Axes a member would have in group `g` (defaults from its frame).
    pub fn axes_in(&self, g: Option<GroupId>, a: Vec3, b: Vec3, depth_dir: Option<Vec3>) -> MemberAxes {
        let parent = g.map(|g| self.groups[g.idx()].frame).unwrap_or(Frame::WORLD);
        MemberAxes::in_frame(a, b, depth_dir, &parent)
    }

    /// Member length between its end nodes.
    pub fn member_length(&self, id: MemberId) -> f64 {
        let m = self.member(id);
        self.pos(m.start()).distance(self.pos(m.end()))
    }

    // ----- nodes -----------------------------------------------------------

    pub(crate) fn reset_index(&mut self) {
        self.index = NodeIndex::default();
    }

    fn ensure_index(&mut self) {
        if self.index.indexed > self.nodes.len() {
            self.index = NodeIndex::default();
        }
        while self.index.indexed < self.nodes.len() {
            let n = &self.nodes[self.index.indexed];
            self.index.cells.entry(cell_of(n.pos)).or_default().push(n.id);
            self.index.indexed += 1;
        }
    }

    /// Always creates a new node (even if one exists at `pos`).
    pub fn add_node(&mut self, pos: Vec3) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(Node { id, pos, name: None });
        id
    }

    /// Existing node within `LEN_TOL` of `pos`, if any.
    pub fn find_node(&mut self, pos: Vec3) -> Option<NodeId> {
        self.ensure_index();
        let (cx, cy, cz) = cell_of(pos);
        let mut best: Option<(f64, NodeId)> = None;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(ids) = self.index.cells.get(&(cx + dx, cy + dy, cz + dz)) {
                        for &id in ids {
                            let d = self.nodes[id.idx()].pos.distance(pos);
                            if d <= LEN_TOL && best.is_none_or(|(bd, _)| d < bd) {
                                best = Some((d, id));
                            }
                        }
                    }
                }
            }
        }
        best.map(|(_, id)| id)
    }

    /// Node at `pos`, reusing an existing coincident node (this is how shared
    /// vertices — and therefore junctions — are usually created).
    pub fn node_at(&mut self, pos: Vec3) -> NodeId {
        match self.find_node(pos) {
            Some(id) => id,
            None => self.add_node(pos),
        }
    }

    // ----- catalogue entities ----------------------------------------------

    pub fn add_section(&mut self, mut s: Section) -> SectionId {
        if let Some(existing) = self.sections.iter().find(|e| e.name == s.name && e.shape == s.shape && e.plies == s.plies) {
            return existing.id;
        }
        s.id = SectionId(self.sections.len() as u32);
        let id = s.id;
        self.sections.push(s);
        id
    }

    pub fn add_material(&mut self, mut m: Material) -> MaterialId {
        if let Some(existing) = self.materials.iter().find(|e| e.name == m.name && e.design_key == m.design_key) {
            return existing.id;
        }
        m.id = MaterialId(self.materials.len() as u32);
        let id = m.id;
        self.materials.push(m);
        id
    }

    pub fn add_connection(&mut self, mut c: Connection) -> ConnectionId {
        if let Some(existing) = self.connections.iter().find(|e| {
            e.name == c.name && e.fasteners == c.fasteners && e.hardware == c.hardware
        }) {
            return existing.id;
        }
        c.id = ConnectionId(self.connections.len() as u32);
        let id = c.id;
        self.connections.push(c);
        id
    }

    pub fn add_group(&mut self, name: &str, kind: &str, frame: Frame, parent: Option<GroupId>) -> GroupId {
        let id = GroupId(self.groups.len() as u32);
        self.groups.push(Group {
            id,
            name: name.into(),
            kind: kind.into(),
            frame,
            parent,
            members: vec![],
            annotations: vec![],
            props: BTreeMap::new(),
            directions: BTreeMap::new(),
        });
        id
    }

    /// Names a direction (given in the group's own frame) for `facing`/`along`.
    pub fn name_direction(&mut self, g: GroupId, name: &str, local: Vec3) {
        self.groups[g.idx()].directions.insert(name.into(), local.normalized());
    }

    pub fn add_load_case(&mut self, name: &str, kind: LoadKind) -> LoadCaseId {
        let id = LoadCaseId(self.load_cases.len() as u32);
        self.load_cases.push(LoadCase { id, name: name.into(), kind, self_weight: kind == LoadKind::Dead });
        id
    }

    // ----- members ---------------------------------------------------------

    /// Creates a member along `path` (≥ 2 nodes, ordered, collinear).
    pub fn add_member(&mut self, path: &[NodeId], spec: &MemberSpec) -> MemberId {
        assert!(path.len() >= 2, "a member needs at least two nodes");
        let id = MemberId(self.members.len() as u32);
        self.members.push(Member {
            id,
            mark: spec.mark.clone(),
            role: spec.role.clone(),
            priority: spec.priority,
            path: path.to_vec(),
            section: spec.section,
            material: spec.material,
            depth_dir: spec.depth_dir,
            anchor: spec.anchor,
            group: spec.group,
            name: spec.name.clone(),
        });
        if let Some(g) = spec.group {
            self.groups[g.idx()].members.push(id);
        }
        id
    }

    /// Straight member between two points (nodes are shared if they exist).
    pub fn add_member_between(&mut self, a: Vec3, b: Vec3, spec: &MemberSpec) -> MemberId {
        let (na, nb) = (self.node_at(a), self.node_at(b));
        self.add_member(&[na, nb], spec)
    }

    /// Parameter of `p` along member `m` and its distance off the axis.
    pub fn project_on_member(&self, m: MemberId, p: Vec3) -> (f64, f64) {
        let ax = self.member_axes(m);
        let t = (p - ax.origin).dot(ax.x);
        let off = (p - (ax.origin + ax.x * t)).norm();
        (t, off)
    }

    /// Inserts an existing node into a member's path (making it a connection
    /// point). No-op if already present.
    pub fn insert_node_on(&mut self, m: MemberId, node: NodeId) -> Result<(), ModelError> {
        if self.member(m).path.contains(&node) {
            return Ok(());
        }
        let (t, off) = self.project_on_member(m, self.pos(node));
        if off > LEN_TOL {
            return Err(ModelError::NotOnMember { member: m, distance: off });
        }
        let len = self.member_length(m);
        if t < -LEN_TOL || t > len + LEN_TOL {
            return Err(ModelError::OutsideMember { member: m, t });
        }
        let ts: Vec<f64> = self.member(m).path.iter().map(|&n| self.project_on_member(m, self.pos(n)).0).collect();
        let at = ts.iter().position(|&ti| ti > t).unwrap_or(ts.len());
        self.members[m.idx()].path.insert(at, node);
        Ok(())
    }

    /// Node on member `m` at point `p` (creating/reusing a node and inserting it into the path).
    pub fn node_on(&mut self, m: MemberId, p: Vec3) -> Result<NodeId, ModelError> {
        let (t, off) = self.project_on_member(m, p);
        if off > LEN_TOL {
            return Err(ModelError::NotOnMember { member: m, distance: off });
        }
        let ax = self.member_axes(m);
        let n = self.node_at(ax.origin + ax.x * t);
        self.insert_node_on(m, n)?;
        Ok(n)
    }

    /// Node on member `m` at distance `t` from its start node.
    pub fn node_along(&mut self, m: MemberId, t: f64) -> Result<NodeId, ModelError> {
        let ax = self.member_axes(m);
        self.node_on(m, ax.origin + ax.x * t)
    }

    /// Connects two members whose axes cross: creates (or reuses) the node at
    /// the crossing point and inserts it into both paths.
    pub fn intersect(&mut self, a: MemberId, b: MemberId) -> Result<NodeId, ModelError> {
        let (pa, pb) = (self.member_axes(a), self.member_axes(b));
        let w0 = pa.origin - pb.origin;
        let bdot = pa.x.dot(pb.x);
        let den = 1.0 - bdot * bdot;
        if den < 1e-12 {
            return Err(ModelError::Parallel { a, b });
        }
        let d = pa.x.dot(w0);
        let e = pb.x.dot(w0);
        let s = (bdot * e - d) / den;
        let t = (e - bdot * d) / den;
        let (qa, qb) = (pa.origin + pa.x * s, pb.origin + pb.x * t);
        let gap = qa.distance(qb);
        if gap > LEN_TOL {
            return Err(ModelError::NoIntersection { a, b, gap });
        }
        if s < -LEN_TOL || s > pa.length + LEN_TOL {
            return Err(ModelError::OutsideMember { member: a, t: s });
        }
        if t < -LEN_TOL || t > pb.length + LEN_TOL {
            return Err(ModelError::OutsideMember { member: b, t });
        }
        let n = self.node_at((qa + qb) * 0.5);
        self.insert_node_on(a, n)?;
        self.insert_node_on(b, n)?;
        Ok(n)
    }

    /// Records a connection used at `node` between `member` and `to`.
    pub fn connect(&mut self, node: NodeId, member: MemberId, to: Option<MemberId>, connection: ConnectionId) {
        self.joint_mut(node).connections.push(ConnectionUse { member, to, connection });
    }

    pub fn add_rule(&mut self, node: NodeId, rule: JointRule) {
        self.joint_mut(node).rules.push(rule);
    }

    pub fn bond(&mut self, a: MemberId, b: MemberId, connection: ConnectionId) {
        self.bonds.push(Bond { a, b, connection });
    }

    /// Connects every member in `a` with every member in `b` where their axes
    /// meet: crossing axes get a shared node; collinear overlapping axes share
    /// each other's path nodes. Returns the nodes that became shared.
    pub fn connect_members(&mut self, a: &[MemberId], b: &[MemberId]) -> Vec<NodeId> {
        let mut out = vec![];
        for &ma in a {
            for &mb in b {
                if ma == mb {
                    continue;
                }
                match self.intersect(ma, mb) {
                    Ok(n) => out.push(n),
                    Err(ModelError::Parallel { .. }) => {
                        for (from, to) in [(ma, mb), (mb, ma)] {
                            let nodes = self.member(from).path.clone();
                            for n in nodes {
                                if self.insert_node_on(to, n).is_ok() {
                                    out.push(n);
                                }
                            }
                        }
                    }
                    Err(_) => {}
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }

    // ----- serialization ---------------------------------------------------

    pub fn members_in_group(&self, g: GroupId) -> impl Iterator<Item = &Member> {
        self.groups[g.idx()].members.iter().map(move |&m| self.member(m))
    }

    /// All groups whose ancestor chain contains `g` (including `g`).
    pub fn subgroups(&self, g: GroupId) -> Vec<GroupId> {
        self.groups
            .iter()
            .filter(|c| {
                let mut cur = Some(c.id);
                while let Some(id) = cur {
                    if id == g {
                        return true;
                    }
                    cur = self.groups[id.idx()].parent;
                }
                false
            })
            .map(|c| c.id)
            .collect()
    }

    /// Full stable path of a member: its group chain then its name (or
    /// `role#k`, k counting that role within the group), e.g.
    /// `Structure/Existing trusses/T2/bottom_chord.F-H1`.
    pub fn member_path(&self, id: MemberId) -> String {
        let m = self.member(id);
        let local = m.name.clone().unwrap_or_else(|| {
            let siblings: Vec<MemberId> = match m.group {
                Some(g) => self.groups[g.idx()].members.clone(),
                None => self.members.iter().filter(|x| x.group.is_none()).map(|x| x.id).collect(),
            };
            let k = siblings.iter().filter(|&&s| self.member(s).role == m.role).position(|&s| s == id).unwrap_or(0);
            format!("{}#{}", m.role, k + 1)
        });
        let mut parts = vec![local];
        let mut g = m.group;
        while let Some(id) = g {
            parts.push(self.groups[id.idx()].name.clone());
            g = self.groups[id.idx()].parent;
        }
        parts.reverse();
        parts.join("/")
    }

    /// Finds the member whose path ends with the `/`-separated `selector`
    /// (e.g. `T2/bottom_chord.F-H1`). Errors if none or several match.
    pub fn find_member(&self, selector: &str) -> Result<MemberId, String> {
        let want: Vec<&str> = selector.split('/').filter(|s| !s.is_empty()).collect();
        let hits: Vec<MemberId> = self
            .members
            .iter()
            .map(|m| m.id)
            .filter(|&id| {
                let path = self.member_path(id);
                let have: Vec<&str> = path.split('/').collect();
                have.len() >= want.len() && have[have.len() - want.len()..] == want[..]
            })
            .collect();
        match hits.as_slice() {
            [one] => Ok(*one),
            [] => Err(format!("no member matches \"{selector}\"")),
            many => Err(format!(
                "\"{selector}\" is ambiguous: {}",
                many.iter().take(4).map(|&id| self.member_path(id)).collect::<Vec<_>>().join(", ")
            )),
        }
    }

    /// Members of `g` and all its subgroups.
    pub fn members_in_tree(&self, g: GroupId) -> Vec<MemberId> {
        let mut out: Vec<MemberId> =
            self.subgroups(g).iter().flat_map(|&s| self.groups[s.idx()].members.clone()).collect();
        out.sort();
        out.dedup();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::Material;
    use crate::math::v3;

    fn simple_model() -> (Model, MemberSpec) {
        let mut m = Model::new("t");
        let s = m.add_section(Section::rect("r", 0.04, 0.09));
        let mat = m.add_material(Material {
            id: MaterialId(0),
            name: "x".into(),
            family: "x".into(),
            e: 1e10,
            g: 1e9,
            density: 500.0,
            design_key: None,
        });
        (m, MemberSpec::new("beam", s, mat))
    }

    #[test]
    fn node_at_reuses_coincident_nodes() {
        let (mut m, _) = simple_model();
        let a = m.node_at(v3(1.0, 2.0, 3.0));
        let b = m.node_at(v3(1.0 + LEN_TOL / 2.0, 2.0, 3.0));
        let c = m.node_at(v3(1.0 + 3.0 * LEN_TOL, 2.0, 3.0));
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn insert_keeps_path_ordered() {
        let (mut m, spec) = simple_model();
        let mb = m.add_member_between(v3(0., 0., 0.), v3(4., 0., 0.), &spec);
        let n3 = m.node_along(mb, 3.0).unwrap();
        let n1 = m.node_along(mb, 1.0).unwrap();
        let p = &m.member(mb).path;
        assert_eq!(p.len(), 4);
        assert_eq!(p[1], n1);
        assert_eq!(p[2], n3);
        assert!(m.node_on(mb, v3(2.0, 0.1, 0.0)).is_err());
    }

    #[test]
    fn intersect_crossing_members() {
        let (mut m, spec) = simple_model();
        let a = m.add_member_between(v3(0., 0., 0.), v3(4., 0., 0.), &spec);
        let b = m.add_member_between(v3(1., -1., 0.), v3(1., 1., 0.), &spec);
        let n = m.intersect(a, b).unwrap();
        assert!((m.pos(n) - v3(1., 0., 0.)).norm() < 1e-12);
        assert!(m.member(a).path.contains(&n) && m.member(b).path.contains(&n));
        let c = m.add_member_between(v3(0., 0., 1.), v3(4., 0., 1.), &spec);
        assert!(matches!(m.intersect(a, c), Err(ModelError::Parallel { .. })));
    }

    #[test]
    fn json_roundtrip_rebuilds_index() {
        let (mut m, spec) = simple_model();
        m.add_member_between(v3(0., 0., 0.), v3(4., 0., 0.), &spec);
        let s = serde_json::to_string(&m).unwrap();
        let mut back: Model = serde_json::from_str(&s).unwrap();
        assert_eq!(back.nodes.len(), 2);
        assert_eq!(back.node_at(v3(4., 0., 0.)), NodeId(1));
    }
}
