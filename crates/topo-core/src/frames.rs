//! The frame tree and named directions.
//!
//! The world frame is canonical: +x east, +y north, +z up. Buildings are
//! placed in it; groups (walls, roofs, trusses, assemblies) have frames within
//! their building; members have canonical frames (x with the grain, y across
//! the thickness, z across the depth). Directions can be named in any frame —
//! compass names in the world, `inside` in a wall, `span` in a truss — and
//! resolved from any group.

use crate::ids::GroupId;
use crate::math::{Frame, Vec3};
use crate::model::{JointRule, Model};

/// Compass and vertical directions of the world frame.
pub const WORLD_DIRECTIONS: [(&str, Vec3); 6] = [
    ("east", Vec3::X),
    ("west", Vec3::new(-1.0, 0.0, 0.0)),
    ("north", Vec3::Y),
    ("south", Vec3::new(0.0, -1.0, 0.0)),
    ("up", Vec3::Z),
    ("down", Vec3::new(0.0, 0.0, -1.0)),
];

/// `+x`, `-y`, … (also `x` for `+x`) as a local unit vector.
pub fn axis_direction(name: &str) -> Option<Vec3> {
    let (sign, axis) = match name.as_bytes() {
        [b'+', a] => (1.0, *a),
        [b'-', a] => (-1.0, *a),
        [a] => (1.0, *a),
        _ => return None,
    };
    let v = match axis {
        b'x' => Vec3::X,
        b'y' => Vec3::Y,
        b'z' => Vec3::Z,
        _ => return None,
    };
    Some(v * sign)
}

/// The canonical axis name closest to a local direction, e.g. `+z`.
pub fn nearest_axis(local: Vec3) -> &'static str {
    let c = [local.x, local.y, local.z];
    let (i, v) = c.iter().enumerate().max_by(|a, b| a.1.abs().total_cmp(&b.1.abs())).unwrap();
    match (i, *v >= 0.0) {
        (0, true) => "+x",
        (0, false) => "-x",
        (1, true) => "+y",
        (1, false) => "-y",
        (2, true) => "+z",
        _ => "-z",
    }
}

impl Model {
    /// A group's path: its ancestors' names and its own, `/`-separated.
    pub fn group_path(&self, g: GroupId) -> String {
        let mut parts = vec![];
        let mut cur = Some(g);
        while let Some(id) = cur {
            parts.push(self.groups[id.idx()].name.clone());
            cur = self.groups[id.idx()].parent;
        }
        parts.reverse();
        parts.join("/")
    }

    /// The group whose path ends with `selector` (e.g. `Existing trusses/T2`).
    pub fn find_group(&self, selector: &str) -> Result<GroupId, String> {
        let want: Vec<&str> = selector.split('/').filter(|s| !s.is_empty()).collect();
        let hits: Vec<GroupId> = self
            .groups
            .iter()
            .map(|g| g.id)
            .filter(|&id| {
                let path = self.group_path(id);
                let have: Vec<&str> = path.split('/').collect();
                have.len() >= want.len() && have[have.len() - want.len()..] == want[..]
            })
            .collect();
        match hits.as_slice() {
            [one] => Ok(*one),
            [] => Err(format!("no group matches \"{selector}\"")),
            many => Err(format!(
                "\"{selector}\" is ambiguous: {}",
                many.iter().take(4).map(|&g| self.group_path(g)).collect::<Vec<_>>().join(", ")
            )),
        }
    }

    /// Resolves a direction name to a world unit vector. With a group:
    /// axis names (`+x`…) are in its frame; other names are looked up in it
    /// and then its ancestors. Compass names (`north`, `up`…) always work.
    pub fn direction(&self, name: &str, in_group: Option<GroupId>) -> Result<Vec3, String> {
        let frame = in_group.map(|g| self.groups[g.idx()].frame).unwrap_or(Frame::WORLD);
        if let Some(v) = axis_direction(name) {
            return Ok(frame.dir_to_world(v));
        }
        let mut cur = in_group;
        while let Some(g) = cur {
            let grp = &self.groups[g.idx()];
            if let Some(v) = grp.directions.get(name) {
                return Ok(grp.frame.dir_to_world(*v).normalized());
            }
            cur = grp.parent;
        }
        if let Some((_, v)) = WORLD_DIRECTIONS.iter().find(|(n, _)| *n == name) {
            return Ok(*v);
        }
        let known: Vec<String> = {
            let mut k: Vec<String> = WORLD_DIRECTIONS.iter().map(|(n, _)| n.to_string()).collect();
            let mut cur = in_group;
            while let Some(g) = cur {
                k.extend(self.groups[g.idx()].directions.keys().cloned());
                cur = self.groups[g.idx()].parent;
            }
            k
        };
        Err(format!("unknown direction \"{name}\" (known here: {}, or ±x/±y/±z)", known.join(", ")))
    }

    /// Named directions within `angle` degrees of a world direction, nearest
    /// first: those of `in_group` and its ancestors, then compass names.
    pub fn describe_direction(&self, world: Vec3, in_group: Option<GroupId>, angle: f64) -> Vec<String> {
        let cos = angle.to_radians().cos();
        let mut out: Vec<(f64, String)> = vec![];
        let mut cur = in_group;
        while let Some(g) = cur {
            let grp = &self.groups[g.idx()];
            for (n, v) in &grp.directions {
                let c = grp.frame.dir_to_world(*v).normalized().dot(world);
                if c >= cos {
                    out.push((c, n.clone()));
                }
            }
            cur = grp.parent;
        }
        for (n, v) in WORLD_DIRECTIONS {
            let c = v.dot(world);
            if c >= cos {
                out.push((c, n.to_string()));
            }
        }
        out.sort_by(|a, b| b.0.total_cmp(&a.0));
        out.dedup_by(|a, b| a.1 == b.1);
        out.into_iter().map(|(_, n)| n).collect()
    }

    /// Moves the whole model rigidly: every position, frame, orientation and
    /// load direction (group-local data such as annotations is unchanged).
    pub fn transform(&mut self, f: &Frame) {
        for n in &mut self.nodes {
            n.pos = f.to_world(n.pos);
        }
        self.reset_index();
        for g in &mut self.groups {
            g.frame = Frame {
                origin: f.to_world(g.frame.origin),
                x: f.dir_to_world(g.frame.x),
                y: f.dir_to_world(g.frame.y),
                z: f.dir_to_world(g.frame.z),
            };
        }
        for m in &mut self.members {
            m.depth_dir = m.depth_dir.map(|d| f.dir_to_world(d));
        }
        for j in &mut self.joints {
            for r in &mut j.rules {
                if let JointRule::Plane { normal, .. } = r {
                    *normal = f.dir_to_world(*normal);
                }
            }
        }
        for l in &mut self.loads {
            match l {
                crate::loads::Load::Node { force, moment, .. } => {
                    *force = f.dir_to_world(*force);
                    *moment = f.dir_to_world(*moment);
                }
                crate::loads::Load::MemberUniform { w, .. } => *w = f.dir_to_world(*w),
                crate::loads::Load::Area { direction, .. } => *direction = f.dir_to_world(*direction),
            }
        }
    }

    /// Moves everything in `other` into this model (renumbering its ids, and
    /// sharing load cases by name and kind). Nodes are never merged, so peers
    /// such as separate buildings stay topologically disjoint. Sheet lists
    /// are not merged (see `sheets::merge_sheets`).
    pub fn absorb(&mut self, other: Model) {
        use crate::ids::*;
        use crate::loads::Load;
        let (dn, dm, dg) = (self.nodes.len() as u32, self.members.len() as u32, self.groups.len() as u32);
        let (ds, dmat, dc) = (self.sections.len() as u32, self.materials.len() as u32, self.connections.len() as u32);
        let n = |id: NodeId| NodeId(id.0 + dn);
        let m = |id: MemberId| MemberId(id.0 + dm);
        let g = |id: GroupId| GroupId(id.0 + dg);
        let c = |id: ConnectionId| ConnectionId(id.0 + dc);
        let cases: Vec<LoadCaseId> = other
            .load_cases
            .iter()
            .map(|lc| match self.load_cases.iter().find(|x| x.name == lc.name && x.kind == lc.kind) {
                Some(x) => x.id,
                None => {
                    let id = self.add_load_case(&lc.name, lc.kind);
                    self.load_cases[id.idx()].self_weight = lc.self_weight;
                    id
                }
            })
            .collect();
        let case = |id: LoadCaseId| cases[id.idx()];
        for mut x in other.nodes {
            x.id = n(x.id);
            self.nodes.push(x);
        }
        for mut x in other.members {
            x.id = m(x.id);
            x.path = x.path.into_iter().map(n).collect();
            x.section = SectionId(x.section.0 + ds);
            x.material = MaterialId(x.material.0 + dmat);
            x.group = x.group.map(g);
            self.members.push(x);
        }
        for mut x in other.groups {
            x.id = g(x.id);
            x.parent = x.parent.map(g);
            x.members = x.members.into_iter().map(m).collect();
            self.groups.push(x);
        }
        for mut x in other.sections {
            x.id = SectionId(x.id.0 + ds);
            self.sections.push(x);
        }
        for mut x in other.materials {
            x.id = MaterialId(x.id.0 + dmat);
            self.materials.push(x);
        }
        for mut x in other.connections {
            x.id = c(x.id);
            self.connections.push(x);
        }
        for mut j in other.joints {
            j.node = n(j.node);
            for r in &mut j.rules {
                *r = match r.clone() {
                    JointRule::Butt { member, against } => JointRule::Butt { member: m(member), against: m(against) },
                    JointRule::Through { member } => JointRule::Through { member: m(member) },
                    JointRule::Square { member } => JointRule::Square { member: m(member) },
                    JointRule::Miter { a, b } => JointRule::Miter { a: m(a), b: m(b) },
                    JointRule::Plane { member, normal } => JointRule::Plane { member: m(member), normal },
                    JointRule::Plumb { member } => JointRule::Plumb { member: m(member) },
                };
            }
            for u in &mut j.connections {
                u.member = m(u.member);
                u.to = u.to.map(m);
                u.connection = c(u.connection);
            }
            self.joints.push(j);
        }
        for b in other.bonds {
            self.bonds.push(crate::model::Bond { a: m(b.a), b: m(b.b), connection: c(b.connection) });
        }
        for mut s in other.supports {
            s.node = n(s.node);
            self.supports.push(s);
        }
        for l in other.loads {
            self.loads.push(match l {
                Load::Node { case: k, node, force, moment } => Load::Node { case: case(k), node: n(node), force, moment },
                Load::MemberUniform { case: k, member, w, range } => Load::MemberUniform { case: case(k), member: m(member), w, range },
                Load::Area { case: k, group, pressure, direction, basis, label } => Load::Area { case: case(k), group: g(group), pressure, direction, basis, label },
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::v3;

    #[test]
    fn resolves_directions_through_the_frame_tree() {
        let mut m = Model::new("t");
        // A building turned 90°: its +x points north.
        let b = m.add_group("B", "building", Frame { origin: Vec3::ZERO, x: Vec3::Y, y: -Vec3::X, z: Vec3::Z }, None);
        m.name_direction(b, "street", Vec3::new(0.0, -1.0, 0.0));
        let w = m.add_group("Wall A", "wall", Frame::from_x_z(Vec3::ZERO, Vec3::Y, Vec3::Z), Some(b));
        m.name_direction(w, "inside", Vec3::Y);
        let close = |a: Vec3, e: Vec3| assert!((a - e).norm() < 1e-12, "{a:?} vs {e:?}");
        close(m.direction("+x", Some(b)).unwrap(), Vec3::Y);
        close(m.direction("street", Some(w)).unwrap(), Vec3::X); // inherited from the building
        close(m.direction("inside", Some(w)).unwrap(), -Vec3::X);
        close(m.direction("north", Some(w)).unwrap(), Vec3::Y);
        assert!(m.direction("sideways", Some(w)).unwrap_err().contains("inside"));
        assert_eq!(m.find_group("B/Wall A").unwrap(), w);
        assert_eq!(m.describe_direction(Vec3::X, Some(w), 10.0), vec!["street", "east"]);
        assert_eq!(nearest_axis(v3(0.1, -0.9, 0.2)), "-y");
    }
}
