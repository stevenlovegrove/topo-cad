//! Floor/ceiling platform generator: rim joists, band (end) joists and common
//! joists. Local frame: x along the joists, y across them, z up; the origin is
//! the outside corner at the *bottom* of the joists, so joist axes lie on the
//! bearing surface and share nodes with the plates they sit on.

use crate::fastening as fx;
use crate::lumber::sawn;
use topo_core::units::{fmt_inches, inch};
use topo_core::*;

#[derive(Clone, Debug)]
pub struct Floor {
    pub name: String,
    pub origin: Vec3,
    /// Joist direction (horizontal).
    pub x_dir: Vec3,
    /// Direction across the joists; `x_dir × y_dir` must be +Z.
    pub y_dir: Vec3,
    /// Out-to-out dimension along the joists.
    pub length: f64,
    /// Out-to-out dimension across the joists.
    pub width: f64,
    pub joist: (u32, u32),
    pub spacing: f64,
    pub material: MaterialId,
    pub parent: Option<GroupId>,
}

#[derive(Clone, Debug, Default)]
pub struct FloorParts {
    pub group: GroupId,
    pub rims: Vec<MemberId>,
    pub joists: Vec<MemberId>,
}

impl FloorParts {
    pub fn all(&self) -> Vec<MemberId> {
        self.rims.iter().chain(self.joists.iter()).copied().collect()
    }
}

impl Floor {
    pub fn new(name: &str, origin: Vec3, x_dir: Vec3, y_dir: Vec3, length: f64, width: f64, material: MaterialId) -> Floor {
        Floor {
            name: name.into(),
            origin,
            x_dir,
            y_dir,
            length,
            width,
            joist: (2, 10),
            spacing: inch(16.0),
            material,
            parent: None,
        }
    }
    pub fn joists(mut self, thick: u32, depth: u32, spacing: f64) -> Floor {
        self.joist = (thick, depth);
        self.spacing = spacing;
        self
    }

    pub fn build(&self, m: &mut Model) -> FloorParts {
        let (x, y) = (self.x_dir.normalized(), self.y_dir.normalized());
        assert!(x.cross(y).dot(Vec3::Z) > 0.999, "floor x_dir × y_dir must point up");
        let frame = Frame { origin: self.origin, x, y, z: Vec3::Z };
        let p = |a: f64, b: f64| frame.to_world(v3(a, b, 0.0));
        let (len, wid) = (self.length, self.width);
        let g = m.add_group(&self.name, "floor", frame, self.parent);
        let sec = m.add_section(sawn(self.joist.0, self.joist.1));
        let b = m.section(sec).props.width;
        let c_rim = m.add_connection(fx::rim_to_joist());
        let mut parts = FloorParts { group: g, ..Default::default() };

        // Rims (across the joists) run through; everything else butts into them.
        for (x0, inward) in [(0.0, x), (len, -x)] {
            let ax = m.axes_between(p(x0, 0.0), p(x0, wid), None);
            let spec = MemberSpec::new("rim_joist", sec, self.material)
                .priority(20)
                .anchor(Anchor::body_toward(&ax, Some(inward), Some(Vec3::Z)))
                .group(g);
            parts.rims.push(m.add_member_between(p(x0, 0.0), p(x0, wid), &spec));
        }
        let joist_ax = m.axes_between(p(0.0, 0.0), p(len, 0.0), None);
        let mut ys = vec![(0.0, Some(y), "band_joist")];
        ys.extend(
            (1..)
                .map(|k| k as f64 * self.spacing)
                .take_while(|&yy| yy < wid - 1.5 * b)
                .map(|yy| (yy, None, "joist")),
        );
        ys.push((wid, Some(-y), "band_joist"));
        for (yy, side, role) in ys {
            let spec = MemberSpec::new(role, sec, self.material)
                .priority(10)
                .anchor(Anchor::body_toward(&joist_ax, side, Some(Vec3::Z)))
                .group(g);
            let n0 = m.node_on(parts.rims[0], p(0.0, yy)).unwrap();
            let n1 = m.node_on(parts.rims[1], p(len, yy)).unwrap();
            let j = m.add_member(&[n0, n1], &spec);
            m.connect(n0, j, Some(parts.rims[0]), c_rim);
            m.connect(n1, j, Some(parts.rims[1]), c_rim);
            parts.joists.push(j);
        }

        let callout = format!(
            "{}x{} {} @ {} O.C.",
            self.joist.0,
            self.joist.1,
            m.material(self.material).name,
            fmt_inches(self.spacing)
        );
        let group = m.group_mut(g);
        group.annotations.push(Annotation::Span {
            a: v3(0.0, wid * 0.5 + self.spacing * 0.5, 0.0),
            b: v3(len, wid * 0.5 + self.spacing * 0.5, 0.0),
            text: callout.clone(),
        });
        group.annotations.push(Annotation::Dim { a: v3(0.0, 0.0, 0.0), b: v3(len, 0.0, 0.0), side: -Vec3::Y, tier: 0, text: None });
        group.annotations.push(Annotation::Dim { a: v3(len, 0.0, 0.0), b: v3(len, wid, 0.0), side: Vec3::X, tier: 0, text: None });
        group.props.insert("framing".into(), callout);
        parts
    }
}
