//! Prefabricated roof trusses and truss roofs.
//!
//! A truss is described by a [`TrussShape`]: named points in the truss plane
//! (s along the span from the left heel, z up) and members as paths through
//! them. Standard types (Fink, Howe, Pratt, king post, fan, scissors, mono)
//! are shape generators; custom trusses are shapes written or edited by hand.
//!
//! Chord axes lie on their *bottom* faces (the bearing convention used
//! throughout), so a bottom chord shares its heel node with the wall plate it
//! bears on and the top chord's underside passes through the heel. Webs are
//! centred on the lines between panel points.

use crate::fastening as fx;
use crate::lumber::sawn;
use serde::{Deserialize, Serialize};
use topo_core::units::{fmt_ft_in, inch};
use topo_core::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeRole {
    TopChord,
    BottomChord,
    Web,
}

impl ShapeRole {
    fn role(self) -> &'static str {
        match self {
            ShapeRole::TopChord => "top_chord",
            ShapeRole::BottomChord => "bottom_chord",
            ShapeRole::Web => "web",
        }
    }
    fn is_chord(self) -> bool {
        self != ShapeRole::Web
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShapePoint {
    pub name: String,
    pub s: f64,
    pub z: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShapeMember {
    pub role: ShapeRole,
    /// Point names along the member (≥ 2, collinear, in order).
    pub path: Vec<String>,
    /// Junction precedence override (defaults: bottom chord 30, top chord 20,
    /// vertical web 6, other webs 5).
    #[serde(default)]
    pub priority: Option<i32>,
    /// Anchor along the in-plane depth direction, −½…½ (defaults: chords −½ =
    /// axis on the bottom face; webs 0 = centred).
    #[serde(default)]
    pub anchor: Option<f64>,
    /// Nominal size override, e.g. (2, 6).
    #[serde(default)]
    pub size: Option<(u32, u32)>,
    /// Stable name (default `role.first-last`, e.g. `bottom_chord.H0-H1`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl ShapeMember {
    /// The member's stable name within its truss.
    pub fn local_name(&self) -> String {
        self.name.clone().unwrap_or_else(|| format!("{}.{}-{}", self.role.role(), self.path[0], self.path.last().unwrap()))
    }
    pub fn new(role: ShapeRole, path: &[&str]) -> ShapeMember {
        ShapeMember { role, path: path.iter().map(|s| s.to_string()).collect(), priority: None, anchor: None, size: None, name: None }
    }
}

/// A truss as data: points and members in the truss plane.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrussShape {
    /// Type name for drawings, e.g. `Howe (6 panels)`.
    pub name: String,
    /// Heel to heel (out-to-out of bearings).
    pub span: f64,
    /// Nominal top-chord pitch (rise/run), for labels only.
    #[serde(default)]
    pub pitch: Option<f64>,
    pub points: Vec<ShapePoint>,
    pub members: Vec<ShapeMember>,
    /// Dimensions to draw on the truss elevation, in shape coordinates. When
    /// empty, overall span, tails and height are dimensioned automatically.
    #[serde(default)]
    pub dims: Vec<ShapeDim>,
}

/// A drawing dimension between two points of the truss plane `(s, z)`,
/// offset toward `side` (a direction in the plane).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShapeDim {
    pub a: (f64, f64),
    pub b: (f64, f64),
    pub side: (f64, f64),
    #[serde(default)]
    pub tier: u32,
}

/// Standard truss types. `panels` applies to Howe, Pratt and mono (even for
/// Howe/Pratt); `bottom_pitch` to scissors.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StandardTruss {
    Fink,
    Fan,
    KingPost,
    Howe { panels: u32 },
    Pratt { panels: u32 },
    Scissors { bottom_pitch: f64 },
    Mono { panels: u32 },
}

/// Builder for symmetric pitched shapes: points on the left top chord are
/// mirrored to the right with a `'` suffix.
struct Sym {
    l: f64,
    k: f64,
    o: f64,
    points: Vec<ShapePoint>,
    left_top: Vec<String>,
    bottom: Vec<String>,
}

impl Sym {
    fn new(l: f64, k: f64, o: f64) -> Sym {
        let mut s = Sym { l, k, o, points: vec![], left_top: vec![], bottom: vec![] };
        s.pt("H0", 0.0, 0.0);
        s.pt("H1", l, 0.0);
        s.pt("P", l / 2.0, l / 2.0 * k);
        if o > 0.0 {
            s.pt("X0", -o, -o * k);
            s.pt("X1", l + o, -o * k);
        }
        s
    }
    fn pt(&mut self, name: &str, s: f64, z: f64) {
        self.points.push(ShapePoint { name: name.into(), s, z });
    }
    /// Panel point on the left top chord at `s` (and its mirror).
    fn top(&mut self, name: &str, s: f64) {
        self.pt(name, s, s * self.k);
        self.pt(&format!("{name}'"), self.l - s, s * self.k);
        self.left_top.push(name.into());
    }
    /// Panel point on the bottom chord at `s` (z from `z_of`).
    fn bottom(&mut self, name: &str, s: f64, z: f64) {
        self.pt(name, s, z);
        self.bottom.push(name.into());
    }
    fn chords(&self) -> Vec<ShapeMember> {
        let mut left: Vec<String> = vec![];
        let mut right: Vec<String> = vec![];
        if self.o > 0.0 {
            left.push("X0".into());
            right.push("X1".into());
        }
        left.push("H0".into());
        right.push("H1".into());
        let pos = |n: &str| self.points.iter().find(|p| p.name == n).unwrap().s;
        let mut lt = self.left_top.clone();
        lt.sort_by(|a, b| pos(a).total_cmp(&pos(b)));
        left.extend(lt.iter().cloned());
        right.extend(lt.iter().map(|n| format!("{n}'")));
        left.push("P".into());
        right.push("P".into());
        let mut bottom = vec!["H0".to_string()];
        let mut b = self.bottom.clone();
        b.sort_by(|a, c| pos(a).total_cmp(&pos(c)));
        bottom.extend(b);
        bottom.push("H1".into());
        let m = |role, p: Vec<String>| ShapeMember { role, path: p, priority: None, anchor: None, size: None, name: None };
        vec![m(ShapeRole::TopChord, left), m(ShapeRole::TopChord, right), m(ShapeRole::BottomChord, bottom)]
    }
}

fn webs(pairs: &[(&str, &str)]) -> Vec<ShapeMember> {
    pairs.iter().map(|(a, b)| ShapeMember::new(ShapeRole::Web, &[a, b])).collect()
}

/// Mirror of a point name (`T1` ↔ `T1'`, `B2` ↔ `B{n-2}` handled by callers).
fn mirror(n: &str) -> String {
    match n {
        "P" | "C" => n.into(),
        "H0" => "H1".into(),
        "H1" => "H0".into(),
        _ if n.ends_with('\'') => n.trim_end_matches('\'').into(),
        _ => format!("{n}'"),
    }
}

impl TrussShape {
    /// A standard truss of `span` (heel to heel) at `pitch` with top-chord
    /// overhang `overhang` (horizontal) at the heel(s).
    pub fn standard(kind: StandardTruss, span: f64, pitch: f64, overhang: f64) -> Result<TrussShape, String> {
        let (l, k, o) = (span, pitch, overhang);
        if l <= 0.0 || k <= 0.0 || o < 0.0 {
            return Err("truss span and pitch must be positive, overhang non-negative".into());
        }
        let even_panels = |n: u32| {
            if n >= 4 && n.is_multiple_of(2) {
                Ok(n as usize)
            } else {
                Err(format!("{kind:?}: panels must be an even number ≥ 4"))
            }
        };
        let shape = |name: String, sym: Sym, w: Vec<ShapeMember>| {
            let mut members = sym.chords();
            members.extend(w);
            TrussShape { name, span: l, pitch: Some(k), points: sym.points, members, dims: vec![] }
        };
        Ok(match kind {
            StandardTruss::Fink => {
                let mut s = Sym::new(l, k, o);
                s.top("T1", l / 4.0);
                s.bottom("B1", l / 3.0, 0.0);
                s.bottom("B2", 2.0 * l / 3.0, 0.0);
                shape("Fink".into(), s, webs(&[("T1", "B1"), ("B1", "P"), ("P", "B2"), ("B2", "T1'")]))
            }
            StandardTruss::Fan => {
                let mut s = Sym::new(l, k, o);
                s.top("T1", l / 6.0);
                s.top("T2", l / 3.0);
                s.bottom("B1", l / 3.0, 0.0);
                s.bottom("B2", 2.0 * l / 3.0, 0.0);
                let w = [("B1", "T1"), ("B1", "T2"), ("B1", "P"), ("B2", "P"), ("B2", "T2'"), ("B2", "T1'")];
                shape("Fan".into(), s, webs(&w))
            }
            StandardTruss::KingPost => {
                let mut s = Sym::new(l, k, o);
                s.bottom("B1", l / 2.0, 0.0);
                shape("King post".into(), s, webs(&[("B1", "P")]))
            }
            StandardTruss::Howe { panels } | StandardTruss::Pratt { panels } => {
                let n = even_panels(panels)?;
                let h = n / 2;
                let mut s = Sym::new(l, k, o);
                for i in 1..h {
                    s.top(&format!("T{i}"), i as f64 * l / n as f64);
                }
                for i in 1..n {
                    s.bottom(&format!("B{i}"), i as f64 * l / n as f64, 0.0);
                }
                // Left-half names; the right half mirrors T{i} → T{i}' and B{i} → B{n-i}.
                let top = |i: usize| if i == h { "P".to_string() } else { format!("T{i}") };
                let mut pairs: Vec<(String, String)> = (1..=h).map(|i| (top(i), format!("B{i}"))).collect();
                let howe = matches!(kind, StandardTruss::Howe { .. });
                for i in 1..h {
                    pairs.push(if howe { (format!("B{i}"), top(i + 1)) } else { (top(i), format!("B{}", i + 1)) });
                }
                let mut all = pairs.clone();
                for (a, b) in &pairs {
                    let mb = |x: &str| match x.strip_prefix('B') {
                        Some(i) => format!("B{}", n - i.parse::<usize>().unwrap()),
                        None => mirror(x),
                    };
                    let (ma, mbb) = (mb(a), mb(b));
                    if (ma.clone(), mbb.clone()) != (a.clone(), b.clone()) {
                        all.push((ma, mbb));
                    }
                }
                let refs: Vec<(&str, &str)> = all.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
                let name = format!("{} ({n} panels)", if howe { "Howe" } else { "Pratt" });
                shape(name, s, webs(&refs))
            }
            StandardTruss::Scissors { bottom_pitch } => {
                let kb = bottom_pitch;
                if kb <= 0.0 || kb >= k {
                    return Err("scissors: bottom pitch must be between 0 and the top pitch".into());
                }
                let mut s = Sym::new(l, k, o);
                s.top("T1", l / 4.0);
                s.pt("B1", l / 3.0, l / 3.0 * kb);
                s.pt("B1'", 2.0 * l / 3.0, l / 3.0 * kb);
                s.pt("C", l / 2.0, l / 2.0 * kb);
                let mut members = s.chords();
                // Replace the flat bottom chord with two sloped ones meeting at C.
                members.retain(|mm| mm.role != ShapeRole::BottomChord);
                members.push(ShapeMember::new(ShapeRole::BottomChord, &["H0", "B1", "C"]));
                members.push(ShapeMember::new(ShapeRole::BottomChord, &["H1", "B1'", "C"]));
                members.extend(webs(&[("T1", "B1"), ("B1", "P"), ("C", "P"), ("P", "B1'"), ("B1'", "T1'")]));
                TrussShape { name: "Scissors".into(), span: l, pitch: Some(k), points: s.points, members, dims: vec![] }
            }
            StandardTruss::Mono { panels } => {
                let n = panels.max(2) as usize;
                let mut pts = vec![
                    ShapePoint { name: "H0".into(), s: 0.0, z: 0.0 },
                    ShapePoint { name: "H1".into(), s: l, z: 0.0 },
                    ShapePoint { name: "E".into(), s: l, z: l * k },
                ];
                let mut top: Vec<String> = vec![];
                if o > 0.0 {
                    pts.push(ShapePoint { name: "X0".into(), s: -o, z: -o * k });
                    top.push("X0".into());
                }
                top.push("H0".into());
                let mut bottom = vec!["H0".to_string()];
                for i in 1..n {
                    let si = i as f64 * l / n as f64;
                    pts.push(ShapePoint { name: format!("T{i}"), s: si, z: si * k });
                    pts.push(ShapePoint { name: format!("B{i}"), s: si, z: 0.0 });
                    top.push(format!("T{i}"));
                    bottom.push(format!("B{i}"));
                }
                top.push("E".into());
                bottom.push("H1".into());
                let mut members = vec![
                    ShapeMember { role: ShapeRole::TopChord, path: top, priority: None, anchor: None, size: None, name: None },
                    ShapeMember { role: ShapeRole::BottomChord, path: bottom, priority: None, anchor: None, size: None, name: None },
                    // End post: outer face flush with the high end; runs past the
                    // top chord (priority between the chords) and sits on the bottom chord.
                    ShapeMember { role: ShapeRole::Web, path: vec!["H1".into(), "E".into()], priority: Some(25), anchor: Some(0.5), size: None, name: None },
                ];
                let top_at = |i: usize| if i == n { "E".to_string() } else { format!("T{i}") };
                for i in 1..n {
                    members.push(ShapeMember::new(ShapeRole::Web, &[&format!("T{i}"), &format!("B{i}")]));
                    members.push(ShapeMember::new(ShapeRole::Web, &[&format!("B{i}"), &top_at(i + 1)]));
                }
                TrussShape { name: format!("Mono ({n} panels)"), span: l, pitch: Some(k), points: pts, members, dims: vec![] }
            }
        })
    }

    pub fn point(&self, name: &str) -> Option<&ShapePoint> {
        self.points.iter().find(|p| p.name == name)
    }

    /// Checks references and that every member path is straight and ordered.
    pub fn validate(&self) -> Result<(), String> {
        let mut names: Vec<&str> = self.points.iter().map(|p| p.name.as_str()).collect();
        names.sort();
        if names.windows(2).any(|w| w[0] == w[1]) {
            return Err(format!("truss {}: duplicate point names", self.name));
        }
        for (i, mm) in self.members.iter().enumerate() {
            if mm.path.len() < 2 {
                return Err(format!("truss {}: member {i} needs at least two points", self.name));
            }
            let pts: Vec<Vec2> = mm
                .path
                .iter()
                .map(|n| self.point(n).map(|p| Vec2::new(p.s, p.z)).ok_or(format!("truss {}: unknown point {n}", self.name)))
                .collect::<Result<_, _>>()?;
            let (a, b) = (pts[0], *pts.last().unwrap());
            let d = (b - a).normalized();
            let mut last = -1e-9;
            for (n, p) in mm.path.iter().zip(&pts) {
                let t = (*p - a).dot(d);
                if (*p - a).cross(d).abs() > 1e-6 {
                    return Err(format!("truss {}: {} is not on the straight line of {:?}", self.name, n, mm.path));
                }
                if t <= last {
                    return Err(format!("truss {}: points of {:?} are not in order", self.name, mm.path));
                }
                last = t;
            }
        }
        Ok(())
    }
}

/// Rise per 12 of run, to two decimals without trailing zeros: `6`, `4.43`.
fn fmt_pitch(k: f64) -> String {
    let s = format!("{:.2}", k * 12.0);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[derive(Clone, Debug)]
pub struct Truss {
    pub name: String,
    /// Left bearing point: outside face of the wall at the top of the plate.
    pub heel: Vec3,
    /// Unit direction toward the other heel (horizontal, unless the truss sits tilted).
    pub span_dir: Vec3,
    /// The truss's own up, in its plane and square to `span_dir` (the world's, unless it sits tilted).
    pub up: Vec3,
    pub shape: TrussShape,
    pub chord: (u32, u32),
    pub web: (u32, u32),
    pub material: MaterialId,
    /// Side of the truss plane the lumber sits on (`None` = centred); used for
    /// flush gable-end trusses.
    pub face: Option<Vec3>,
    pub parent: Option<GroupId>,
}

#[derive(Clone, Debug, Default)]
pub struct TrussParts {
    pub group: GroupId,
    pub top_chords: Vec<MemberId>,
    pub bottom_chords: Vec<MemberId>,
    pub webs: Vec<MemberId>,
}

impl TrussParts {
    pub fn all(&self) -> Vec<MemberId> {
        self.top_chords.iter().chain(&self.bottom_chords).chain(&self.webs).copied().collect()
    }
}

impl Truss {
    pub fn new(name: &str, heel: Vec3, span_dir: Vec3, shape: TrussShape, material: MaterialId) -> Truss {
        Truss { name: name.into(), heel, span_dir: span_dir.normalized(), up: Vec3::Z, shape, chord: (2, 4), web: (2, 4), material, face: None, parent: None }
    }

    /// Fink ("W") truss with a 12" overhang.
    pub fn fink(name: &str, heel: Vec3, span_dir: Vec3, span: f64, pitch: f64, material: MaterialId) -> Truss {
        let shape = TrussShape::standard(StandardTruss::Fink, span, pitch, inch(12.0)).expect("valid fink");
        Truss::new(name, heel, span_dir, shape, material)
    }

    pub fn build(&self, m: &mut Model) -> TrussParts {
        self.shape.validate().unwrap_or_else(|e| panic!("{e}"));
        let s = self.span_dir;
        let up = self.up;
        let n = up.cross(s);
        let frame = Frame { origin: self.heel, x: s, y: n, z: up };
        let g = m.add_group(&self.name, "truss", frame, self.parent);
        m.name_direction(g, "span", Vec3::X);
        m.name_direction(g, "normal", Vec3::Y);
        let plate = m.add_connection(fx::truss_plate());
        let nodes: Vec<(String, NodeId)> = self
            .shape
            .points
            .iter()
            .map(|p| (p.name.clone(), m.node_at(frame.to_world(v3(p.s, 0.0, p.z)))))
            .collect();
        let node = |name: &str| nodes.iter().find(|(n, _)| n == name).unwrap().1;

        let mut parts = TrussParts { group: g, ..Default::default() };
        let mut placed: Vec<(MemberId, ShapeRole)> = vec![];
        for sm in &self.shape.members {
            let path: Vec<NodeId> = sm.path.iter().map(|p| node(p)).collect();
            let (a, b) = (m.pos(path[0]), m.pos(*path.last().unwrap()));
            let x = (b - a).normalized();
            let vertical = x.dot(up).abs() > 0.99;
            // In-plane depth direction: up for sloped/horizontal members, along the span for verticals.
            let depth = if vertical { s } else { up };
            let ax = m.axes_between(a, b, Some(depth));
            let size = sm.size.unwrap_or(if sm.role.is_chord() { self.chord } else { self.web });
            let sec = m.add_section(sawn(size.0, size.1));
            let default_anchor = if sm.role.is_chord() { -0.5 } else { 0.0 };
            let mut anchor = Anchor::body_toward(&ax, self.face, None);
            anchor.v = sm.anchor.unwrap_or(default_anchor);
            let prio = sm.priority.unwrap_or(match sm.role {
                ShapeRole::BottomChord => 30,
                ShapeRole::TopChord => 20,
                ShapeRole::Web if vertical => 6,
                ShapeRole::Web => 5,
            });
            let spec = MemberSpec::new(sm.role.role(), sec, self.material)
                .priority(prio)
                .depth_dir(depth)
                .anchor(anchor)
                .group(g)
                .named(&sm.local_name());
            let id = m.add_member(&path, &spec);
            placed.push((id, sm.role));
            match sm.role {
                ShapeRole::TopChord => parts.top_chords.push(id),
                ShapeRole::BottomChord => parts.bottom_chords.push(id),
                ShapeRole::Web => parts.webs.push(id),
            }
        }

        // Joints: a plate wherever a member ends on another truss member;
        // two chords of the same kind ending together are mitred (ridge, scissors apex).
        for &(id, role) in &placed {
            for end in [m.member(id).start(), m.member(id).end()] {
                let others: Vec<(MemberId, ShapeRole, bool)> = placed
                    .iter()
                    .filter(|(o, _)| *o != id && m.member(*o).path.contains(&end))
                    .map(|&(o, r)| (o, r, m.member(o).start() == end || m.member(o).end() == end))
                    .collect();
                let Some(&(to, ..)) = others.iter().find(|(_, _, is_end)| !is_end).or(others.first()) else {
                    // A free top-chord end is an eave tail: plumb cut.
                    if role == ShapeRole::TopChord {
                        m.add_rule(end, JointRule::Plumb { member: id });
                    }
                    continue;
                };
                m.connect(end, id, Some(to), plate);
                if role.is_chord() {
                    let through_chord = others.iter().any(|(_, r, is_end)| r.is_chord() && !is_end);
                    let partner = others.iter().find(|(o, r, is_end)| *r == role && *is_end && *o > id);
                    if let (false, Some(&(p, ..))) = (through_chord, partner) {
                        m.add_rule(end, JointRule::Miter { a: id, b: p });
                    }
                }
            }
        }

        let sh = &self.shape;
        let span = sh.span;
        let zmax = sh.points.iter().map(|p| p.z).fold(0.0, f64::max);
        let group = m.group_mut(g);
        if sh.dims.is_empty() {
            group.annotations.push(Annotation::Dim { a: v3(0.0, 0.0, 0.0), b: v3(span, 0.0, 0.0), side: -Vec3::Z, tier: 1, text: None });
            let zmin = sh.points.iter().map(|p| p.z).fold(0.0, f64::min);
            for p in sh.points.iter().filter(|p| p.s < 0.0 || p.s > span) {
                let (a, b) = if p.s < 0.0 { (p.s, 0.0) } else { (span, p.s) };
                group.annotations.push(Annotation::Dim { a: v3(a, 0.0, zmin), b: v3(b, 0.0, zmin), side: -Vec3::Z, tier: 0, text: None });
            }
            group.annotations.push(Annotation::Dim { a: v3(span, 0.0, 0.0), b: v3(span, 0.0, zmax), side: Vec3::X, tier: 2, text: None });
        } else {
            for d in &sh.dims {
                group.annotations.push(Annotation::Dim {
                    a: v3(d.a.0, 0.0, d.a.1),
                    b: v3(d.b.0, 0.0, d.b.1),
                    side: v3(d.side.0, 0.0, d.side.1),
                    tier: d.tier,
                    text: None,
                });
            }
        }
        if let Some(k) = sh.pitch {
            group.annotations.push(Annotation::Note {
                at: v3(span * 0.62, 0.0, zmax * 0.9),
                text: format!("PITCH {}:12", fmt_pitch(k)),
            });
        }
        // Member sizes as actually built (shape overrides included).
        let size_of = |role: ShapeRole| {
            let mut v: Vec<String> = sh
                .members
                .iter()
                .filter(|mm| mm.role == role)
                .map(|mm| {
                    let (t, w) = mm.size.unwrap_or(if role.is_chord() { self.chord } else { self.web });
                    format!("{t}x{w}")
                })
                .collect();
            v.sort();
            v.dedup();
            v.join("/")
        };
        // Custom-dimensioned shapes show their own measured spans instead.
        let span_text = if sh.dims.is_empty() { format!(" {} SPAN", fmt_ft_in(span)) } else { String::new() };
        let framing = format!(
            "{} TRUSS{span_text}: {} TOP CHORDS, {} BOTTOM CHORD, {} WEBS. TRUSS DESIGN BY MANUFACTURER",
            sh.name.to_uppercase(),
            size_of(ShapeRole::TopChord),
            size_of(ShapeRole::BottomChord),
            size_of(ShapeRole::Web),
        );
        m.group_mut(g).props.insert("framing".into(), framing);
        parts
    }
}

/// A run of trusses at a spacing, with flush gable-end trusses at both ends.
#[derive(Clone, Debug)]
pub struct TrussRoof {
    pub name: String,
    /// Heel line start (outside corner of the bearing wall, top of plate).
    pub origin: Vec3,
    pub span_dir: Vec3,
    /// Direction along the ridge.
    pub run_dir: Vec3,
    pub shape: TrussShape,
    pub length: f64,
    pub spacing: f64,
    pub chord: (u32, u32),
    pub web: (u32, u32),
    pub material: MaterialId,
    /// Shapes for particular trusses (1-based numbers) instead of `shape`.
    pub overrides: Vec<(Vec<usize>, TrussShape)>,
    /// Trusses as built off the layout (1-based numbers): further along the
    /// ridge by this much than their place at the spacing.
    pub shifts: Vec<(Vec<usize>, f64)>,
    /// What the trusses may sit on: the tops of plates, beams and headers. A truss sits on
    /// whichever of these are under any part of its bottom chords, as they are modelled: it
    /// rests on them as a rigid thing does (on the two that carry it, level or not), and
    /// `origin`'s height is only where it starts from. Without any, it is built at `origin`.
    pub bearings: Vec<Bearing>,
}

/// The top of something a truss may sit on: the line along the middle of its top face, and half that face's width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bearing {
    pub a: Vec3,
    pub b: Vec3,
    pub half_width: f64,
}

#[derive(Clone, Debug, Default)]
pub struct RoofParts {
    pub group: GroupId,
    pub trusses: Vec<TrussParts>,
    /// For each truss: the bearings it sits on (their places in `TrussRoof::bearings`), and
    /// the point of its bottom chord's underside that is on each.
    pub seats: Vec<Vec<(usize, Vec3)>>,
    /// Trusses (1-based numbers) with nothing under any part of them, among the bearings given: built at the height they started from.
    pub unseated: Vec<usize>,
    /// Trusses (1-based numbers) that what is under them does not hold up (one bearing only, or
    /// all to one side of their middle): left where they started.
    pub unsteady: Vec<usize>,
    /// The solve that sat them, to be shown (it is moved into the model's `solves`).
    pub solved: Option<topo_core::Solved>,
    /// Trusses that could not be seated on the bearings found for them, and why.
    pub problems: Vec<String>,
}

impl RoofParts {
    pub fn bottom_chords(&self) -> Vec<MemberId> {
        self.trusses.iter().flat_map(|t| t.bottom_chords.clone()).collect()
    }
    pub fn all(&self) -> Vec<MemberId> {
        self.trusses.iter().flat_map(|t| t.all()).collect()
    }
}

impl TrussRoof {
    /// Trusses spanning along `span_dir` from the heel line at `origin`; the
    /// ridge runs to the right of the span direction (so `run × span` is up).
    pub fn new(name: &str, origin: Vec3, span_dir: Vec3, shape: TrussShape, length: f64, spacing: f64, material: MaterialId) -> TrussRoof {
        let span_dir = span_dir.normalized();
        let run_dir = v3(span_dir.y, -span_dir.x, 0.0);
        TrussRoof { name: name.into(), origin, span_dir, run_dir, shape, length, spacing, chord: (2, 4), web: (2, 4), material, overrides: vec![], shifts: vec![], bearings: vec![] }
    }

    pub fn build(&self, m: &mut Model) -> RoofParts {
        let run = self.run_dir.normalized();
        let frame = Frame { origin: self.origin, x: run, y: self.span_dir, z: run.cross(self.span_dir) };
        let g = m.add_group(&self.name, "roof", frame, None);
        m.name_direction(g, "ridge", Vec3::X);
        m.name_direction(g, "span", Vec3::Y);
        let b = inch(1.5);
        let mut stations = vec![(0.0, Some(run))];
        stations.extend(
            (1..)
                .map(|k| k as f64 * self.spacing)
                .take_while(|&r| r < self.length - 1.5 * b)
                .map(|r| (r, None)),
        );
        stations.push((self.length, Some(-run)));
        let mut parts = RoofParts { group: g, ..Default::default() };
        let stations: Vec<(f64, Option<Vec3>)> = stations.into_iter().enumerate().map(|(i, (r, face))| (r + self.shifts.iter().filter(|(ns, _)| ns.contains(&(i + 1))).map(|(_, d)| d).sum::<f64>(), face)).collect();
        let seats = self.seats(&stations.iter().map(|(r, face)| (self.origin + run * *r, *face)).collect::<Vec<_>>(), &mut parts);
        if let Some(solved) = parts.solved.take() {
            m.solves.push(solved);
        }
        for (i, (_, face)) in stations.into_iter().enumerate() {
            let shape = self.overrides.iter().find(|(ns, _)| ns.contains(&(i + 1))).map(|(_, s)| s).unwrap_or(&self.shape);
            let (heel, span_dir, up) = seats[i];
            let mut t = Truss::new(&format!("T{}", i + 1), heel, span_dir, shape.clone(), self.material);
            t.up = up;
            t.chord = self.chord;
            t.web = self.web;
            t.face = face;
            t.parent = Some(g);
            parts.trusses.push(t.build(m));
        }
        let over: f64 = self.shape.points.iter().map(|p| (-p.s).max(p.s - self.shape.span)).fold(0.0, f64::max);
        let callout = format!("PREFAB {} TRUSSES @ {} O.C.", self.shape.name.to_uppercase(), topo_core::units::fmt_inches(self.spacing));
        let group = m.group_mut(g);
        group.annotations.push(Annotation::Span {
            a: v3(self.spacing * 0.5, -over, 0.0),
            b: v3(self.spacing * 0.5, self.shape.span + over, 0.0),
            text: callout.clone(),
        });
        group.annotations.push(Annotation::Dim { a: v3(0.0, 0.0, 0.0), b: v3(self.length, 0.0, 0.0), side: -Vec3::Y, tier: 1, text: None });
        group.props.insert("framing".into(), callout);
        parts
    }

    /// Where each truss sits (its heel, span direction and up), given where the layout puts
    /// its heel on plan (and which side of its plane its lumber is on).
    ///
    /// What may hold a truss up: every bearing whose top is under some part of a bottom chord,
    /// across it (at each edge of the top) or along it (at each end of where they overlap).
    ///
    /// Each truss is a rigid frame, free in the world, with weight, in one system for the roof.
    /// Hard: its chord's underside is no lower than each of those tops (a one-sided contact: it
    /// may be clear of one); its heel is where the layout puts it on plan (datums); it is plumb
    /// and square to the ridge (it is braced). Its weight then brings it down onto whichever
    /// of them hold it up, level or tilted as they say: the solve finds which. One that nothing
    /// holds (a single bearing, or all to one side of its middle) goes on down, and is said.
    /// Nothing ties one truss to another, so each is solved on its own.
    fn seats(&self, heels: &[(Vec3, Option<Vec3>)], parts: &mut RoofParts) -> Vec<(Vec3, Vec3, Vec3)> {
        use topo_solve::{Dir, Joint, Kind, Point, Pose, System, Tier, WORLD};
        struct Under {
            bearing: usize,
            /// Along the span from the heel, and the top's height there.
            s: f64,
            z: f64,
            /// The point of the chord's underside over it, in the truss's own frame, and the chord's direction there.
            chord: [f64; 3],
            along: [f64; 3],
        }
        let (span, run) = (self.span_dir, self.run_dir.normalized());
        let normal = Vec3::Z.cross(span);
        let arr = |v: Vec3| [v.x, v.y, v.z];
        let thick = {
            let bb = sawn(self.chord.0, self.chord.1).bbox();
            bb.width().min(bb.height())
        };
        let mut sys = System::new();
        let mut placed = vec![];
        for (i, &(heel, face)) in heels.iter().enumerate() {
            let shape = self.overrides.iter().find(|(ns, _)| ns.contains(&(i + 1))).map(|(_, s)| s).unwrap_or(&self.shape);
            // The bottom chords' undersides (their axes), as height over distance along the span.
            let chords: Vec<[(f64, f64); 2]> = (shape.members.iter().filter(|m| m.role == ShapeRole::BottomChord))
                .filter_map(|m| Some([shape.point(&m.path[0])?, shape.point(m.path.last()?)?].map(|p| (p.s, p.z))))
                .map(|[a, b]| if a.0 <= b.0 { [a, b] } else { [b, a] })
                .collect();
            // (The height, and the slope.)
            let underside = |s: f64| chords.iter().filter(|[a, b]| s >= a.0 - 1e-9 && s <= b.0 + 1e-9 && b.0 > a.0).map(|[a, b]| (a.1 + (b.1 - a.1) * (s - a.0) / (b.0 - a.0), (b.1 - a.1) / (b.0 - a.0))).min_by(|a, b| a.0.total_cmp(&b.0));
            let (lo, hi) = chords.iter().fold((f64::MAX, f64::MIN), |(lo, hi), [a, b]| (lo.min(a.0), hi.max(b.0)));
            // The middle of the lumber: in the truss's plane, or to one side of it.
            let middle = heel + face.map_or(Vec3::ZERO, |f| run * (f.dot(run).signum() * thick / 2.0));
            let mut under: Vec<Under> = vec![];
            for (k, b) in self.bearings.iter().enumerate() {
                let len = (b.b - b.a).norm();
                let along = (b.b - b.a) / len;
                let mut at = |p: Vec3| {
                    let s = (p - heel).dot(span);
                    if let Some((z, slope)) = underside(s) {
                        let n = (1.0 + slope * slope).sqrt();
                        under.push(Under { bearing: k, s, z: p.z, chord: [s, 0.0, z], along: [1.0 / n, 0.0, slope / n] });
                    }
                };
                if along.dot(run).abs() >= 0.2 {
                    // Across the truss: under it where it crosses the lumber.
                    let t = (middle - b.a).dot(run) / along.dot(run);
                    if t > -thick / 2.0 - 1e-6 && t < len + thick / 2.0 + 1e-6 {
                        // (Its top's two edges: a chord that slopes sits on one of them.)
                        let edge = b.half_width / along.dot(run).abs();
                        at(b.a + along * t - span * edge);
                        if edge > 0.0 {
                            at(b.a + along * t + span * edge);
                        }
                    }
                } else {
                    // Along the truss: under it for as far as the two overlap, if it is under the lumber at all.
                    let (sa, sb) = ((b.a - heel).dot(span), (b.b - heel).dot(span));
                    let (from, to) = (sa.min(sb).max(lo), sa.max(sb).min(hi));
                    let point = |s: f64| b.a + (b.b - b.a) * ((s - sa) / (sb - sa));
                    if to - from > 1e-6 && (point((from + to) / 2.0) - middle).dot(run).abs() <= b.half_width + thick / 2.0 + 1e-6 {
                        at(point(from));
                        at(point(to));
                    }
                }
            }
            if under.is_empty() {
                if !self.bearings.is_empty() {
                    parts.unseated.push(i + 1);
                }
                placed.push((None, under));
                continue;
            }
            let name = format!("T{}", i + 1);
            let pose = Pose { r: [[span.x, normal.x, 0.0], [span.y, normal.y, 0.0], [span.z, normal.z, 1.0]], t: arr(heel) };
            let truss = sys.frame(&name, WORLD, Joint::Free, pose);
            for u in &under {
                // The chord's underside, where it is over this top (wherever along the chord that
                // is), is no lower than it: it may sit on it, or be clear of it.
                let top = heel + span * u.s;
                let top = Point::at(WORLD, [top.x, top.y, u.z]);
                let slid = sys.scalar(truss, &format!("where {name} is over what is under it at {:.2} m", u.s), 0.0);
                let chord = Point::at(truss, u.chord).plus(slid, u.along);
                sys.constrain(&format!("{name} over what is under it at {:.2} m", u.s), Tier::Hard, Kind::offset(top.clone(), chord.clone(), Dir(WORLD, arr(span)), 0.0));
                sys.constrain(&format!("{name} on what is under it at {:.2} m", u.s), Tier::Hard, Kind::above(top, chord, Dir(WORLD, [0.0, 0.0, 1.0]), 0.0));
            }
            let (laid, origin) = (Point::at(WORLD, arr(heel)), Point::at(truss, [0.0; 3]));
            sys.constrain(&format!("{name} at its place along the ridge"), Tier::Hard, Kind::offset(laid.clone(), origin.clone(), Dir(WORLD, arr(run)), 0.0));
            sys.constrain(&format!("{name} at its place along the span"), Tier::Hard, Kind::offset(laid, origin, Dir(WORLD, arr(span)), 0.0));
            // (A truss is braced: it does not lie down under its own weight.)
            sys.constrain(&format!("{name} plumb and square to the ridge"), Tier::Hard, Kind::Aligned(Dir(truss, [0.0, 1.0, 0.0]), Dir(WORLD, arr(normal))));
            // Its weight, at about its middle, brings it down onto what holds it up.
            let count = shape.points.len().max(1) as f64;
            let middle = shape.points.iter().fold([0.0; 3], |m, p| [m[0] + p.s / count, 0.0, m[2] + p.z / count]);
            sys.weigh(truss, middle, 1.0);
            placed.push((Some(truss), under));
        }
        let report = sys.solve();
        if placed.iter().any(|(f, _)| f.is_some()) {
            let e = sys.explain(&report);
            let (tier, at) = (|t: usize| ["hard", "measured", "hint", "weight"][t].to_string(), |p: [f64; 3]| v3(p[0], p[1], p[2]));
            parts.solved = Some(topo_core::Solved {
                name: format!("{}: trusses on what is under them", self.name),
                variables: (e.variables.iter())
                    .map(|v| topo_core::SolvedVariable {
                        name: v.name.clone(),
                        kind: v.kind.into(),
                        at: at(v.at),
                        value: v.value,
                        freedoms: v.freedoms.iter().map(|f| topo_core::SolvedFreedom { what: f.what.clone(), dir: at(f.dir), turn: f.turn, settled: f.settled.map_or("open".into(), tier) }).collect(),
                    })
                    .collect(),
                constraints: (e.constraints.iter()).map(|c| topo_core::SolvedConstraint { name: c.name.clone(), tier: tier(c.tier), kind: c.kind.into(), gap: c.gap, room: c.room, points: c.points.iter().map(|p| at(*p)).collect(), touches: c.touches.clone() }).collect(),
            });
        }
        for c in &report.conflicts {
            let gaps: Vec<String> = c.constraints.iter().map(|g| format!("{} ({:.0} mm off)", g.constraint, g.size * 1000.0)).collect();
            parts.problems.push(format!("{} cannot sit on what is under it: {}", c.frames.join(", "), gaps.join("; ")));
        }
        (placed.iter().zip(heels).enumerate())
            .map(|(i, ((frame, under), &(heel, _)))| match frame {
                // (One that nothing holds up has gone on down: it is left where it started, and said.)
                Some(f) if report.falls.iter().any(|n| n == sys.name(*f)) => {
                    parts.unsteady.push(i + 1);
                    parts.seats.push(vec![]);
                    (heel, span, Vec3::Z)
                }
                Some(f) if !report.conflicts.iter().any(|c| c.frames.iter().any(|n| n == sys.name(*f))) => {
                    let p = sys.in_world(*f);
                    // It sits on everything its underside touches (to a fifth of a millimetre).
                    let on = under.iter().map(|u| (u.bearing, p.apply(u.chord), u.z)).filter(|(_, at, z)| (at[2] - z).abs() < 2e-4).map(|(k, at, _)| (k, v3(at[0], at[1], at[2]))).collect();
                    parts.seats.push(on);
                    let col = |k: usize| v3(p.r[0][k], p.r[1][k], p.r[2][k]);
                    (v3(p.t[0], p.t[1], p.t[2]), col(0), col(2))
                }
                _ => {
                    parts.seats.push(vec![]);
                    (heel, span, Vec3::Z)
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lumber::graded;
    use topo_core::units::ft;
    use topo_geom::Geometry;

    fn low_ends(m: &Model, t: &TrussParts) -> (Vec3, Vec3) {
        let bc = m.member(t.bottom_chords[0]);
        (m.pos(bc.start()), m.pos(*t.bottom_chords.last().map(|&id| m.member(id).path.last().unwrap()).unwrap()))
    }

    fn roof_on(bearings: Vec<(Vec3, Vec3)>, shape: StandardTruss) -> (Model, RoofParts) {
        roof_on_wide(bearings, shape, 0.0)
    }

    fn roof_on_wide(bearings: Vec<(Vec3, Vec3)>, shape: StandardTruss, half_width: f64) -> (Model, RoofParts) {
        let mut m = Model::new("seated");
        let mat = m.add_material(graded("DFL", "No.2"));
        let shape = TrussShape::standard(shape, 6.0, 4.0 / 12.0, 0.0).unwrap();
        // It starts from 2.4 m up; what is under it says where it sits.
        let mut roof = TrussRoof::new("Roof", v3(0.0, 0.0, 2.4), Vec3::Y, shape, 4.0 * inch(24.0), inch(24.0), mat);
        roof.bearings = bearings.into_iter().map(|(a, b)| Bearing { a, b, half_width }).collect();
        let parts = roof.build(&mut m);
        (m, parts)
    }

    #[test]
    fn trusses_on_level_bearings_are_exactly_where_the_layout_puts_them() {
        // The ridge runs along x; plates under each heel line, a little inside it (as a wall's line is).
        let (plain, _) = roof_on(vec![], StandardTruss::Fink);
        let (m, parts) = roof_on(vec![(v3(-0.5, 0.05, 2.4), v3(3.0, 0.05, 2.4)), (v3(-0.5, 5.95, 2.4), v3(3.0, 5.95, 2.4))], StandardTruss::Fink);
        assert!(parts.seats.iter().all(|on| on.len() == 2) && parts.problems.is_empty() && parts.unseated.is_empty(), "{parts:#?}");
        assert_eq!(m.nodes.iter().map(|n| n.pos).collect::<Vec<_>>(), plain.nodes.iter().map(|n| n.pos).collect::<Vec<_>>());
    }

    #[test]
    fn trusses_sit_on_what_is_under_them_and_tilt_as_it_says() {
        // The far plate 38 mm lower; under the second and third trusses, near their near heels, a header's
        // cap 30 mm above the plate; and under the middle of all of them, a beam too low to touch.
        let (m, parts) = roof_on(
            vec![
                (v3(-0.5, 0.05, 2.4), v3(3.0, 0.05, 2.4)),
                (v3(-0.5, 5.95, 2.362), v3(3.0, 5.95, 2.362)),
                (v3(0.4, 0.3, 2.43), v3(1.4, 0.3, 2.43)),
                (v3(-0.5, 3.0, 2.2), v3(3.0, 3.0, 2.2)),
            ],
            StandardTruss::Fink,
        );
        assert!(parts.problems.is_empty() && parts.unseated.is_empty(), "{parts:#?}");
        for (i, t) in parts.trusses.iter().enumerate() {
            let on_header = i == 1 || i == 2;
            let (a, b) = low_ends(&m, t);
            // The chord's bottom is a straight line through what carries it: the far plate, and the near plate or the header's cap.
            let at = |y: f64| a.z + (b.z - a.z) * (y - a.y) / (b.y - a.y);
            let (near, height) = if on_header { (0.3, 2.43) } else { (0.05, 2.4) };
            assert!((at(near) - height).abs() < 1e-7 && (at(5.95) - 2.362).abs() < 1e-7, "T{}: {} {}", i + 1, at(near), at(5.95));
            // It sits on those two and on nothing else (over the header, it is clear of the near plate).
            let on: Vec<usize> = parts.seats[i].iter().map(|(k, _)| *k).collect();
            assert_eq!(on, if on_header { vec![1, 2] } else { vec![0, 1] }, "T{}", i + 1);
            // Its heel is where the layout puts it on plan, and its plane is plumb and square to the ridge.
            assert!((a.x - i as f64 * inch(24.0)).abs() < 1e-7 && a.y.abs() < 1e-7 && (b.x - a.x).abs() < 1e-7, "T{}: {a:?} {b:?}", i + 1);
        }
    }

    #[test]
    fn a_truss_sits_on_whatever_is_under_any_part_of_it() {
        // No plates under the heels: two beams inboard, at the third points. It sits on those, level, at their height.
        let (m, parts) = roof_on(vec![(v3(-0.5, 2.0, 2.5), v3(3.0, 2.0, 2.5)), (v3(-0.5, 4.0, 2.5), v3(3.0, 4.0, 2.5))], StandardTruss::Fink);
        assert!(parts.seats.iter().all(|on| on.len() == 2) && parts.unseated.is_empty(), "{parts:#?}");
        let (a, b) = low_ends(&m, &parts.trusses[2]);
        assert!((a.z - 2.5).abs() < 1e-9 && (b.z - 2.5).abs() < 1e-9, "{a:?} {b:?}");

        // One beam only, off its middle: that does not hold it up. It is left where it started, and said.
        let (m, parts) = roof_on(vec![(v3(-0.5, 2.0, 2.5), v3(3.0, 2.0, 2.5))], StandardTruss::Fink);
        assert!(parts.seats.iter().all(|on| on.is_empty()) && parts.unsteady == [1, 2, 3, 4, 5], "{parts:#?}");
        let (a, b) = low_ends(&m, &parts.trusses[2]);
        assert!((a.z - 2.4).abs() < 1e-9 && (b.z - 2.4).abs() < 1e-9, "{a:?} {b:?}");

        // A wall along the first truss (as under a gable), a little to one side of its plane but under its
        // lumber: that truss sits on it along its length. The others have nothing under them, and say so.
        let (_, parts) = roof_on_wide(vec![(v3(0.03, 0.0, 2.45), v3(0.03, 6.0, 2.45))], StandardTruss::Fink, 0.045);
        assert_eq!(parts.seats[0].len(), 2, "{parts:#?}");
        assert_eq!(parts.unseated, [2, 3, 4, 5]);
    }

    #[test]
    fn a_chord_that_rises_sits_where_it_is_over_the_bearing() {
        // Scissors: the bottom chords rise from the heels. On beams a metre in from each heel, the chord
        // there (not the heel line) is at the beams' height.
        let (m, parts) = roof_on(vec![(v3(-0.5, 1.0, 2.5), v3(3.0, 1.0, 2.5)), (v3(-0.5, 5.0, 2.5), v3(3.0, 5.0, 2.5))], StandardTruss::Scissors { bottom_pitch: 2.0 / 12.0 });
        assert!(parts.seats.iter().all(|on| on.len() == 2), "{parts:#?}");
        let heel = m.pos(m.member(parts.trusses[2].bottom_chords[0]).start());
        assert!((heel.z - (2.5 - 1.0 * 2.0 / 12.0)).abs() < 1e-9, "{heel:?}");

        // On a beam with some width, it is the beam's outer edge the rising chord sits on: it does not sink into the beam.
        let (m, parts) = roof_on_wide(vec![(v3(-0.5, 1.0, 2.5), v3(3.0, 1.0, 2.5)), (v3(-0.5, 5.0, 2.5), v3(3.0, 5.0, 2.5))], StandardTruss::Scissors { bottom_pitch: 2.0 / 12.0 }, 0.05);
        assert!(parts.seats.iter().all(|on| on.len() == 2) && parts.unsteady.is_empty(), "{parts:#?}");
        let heel = m.pos(m.member(parts.trusses[2].bottom_chords[0]).start());
        assert!((heel.z - (2.5 - 0.95 * 2.0 / 12.0)).abs() < 1e-9, "{heel:?}");
    }

    fn all_standard() -> Vec<StandardTruss> {
        vec![
            StandardTruss::Fink,
            StandardTruss::Fan,
            StandardTruss::KingPost,
            StandardTruss::Howe { panels: 6 },
            StandardTruss::Pratt { panels: 6 },
            StandardTruss::Howe { panels: 4 },
            StandardTruss::Pratt { panels: 8 },
            StandardTruss::Scissors { bottom_pitch: 3.0 / 12.0 },
            StandardTruss::Mono { panels: 4 },
        ]
    }

    /// Every standard type builds a clean truss: valid shape, no clashes,
    /// every butt joint fully bearing.
    #[test]
    fn standard_trusses_are_sound() {
        for kind in all_standard() {
            for overhang in [0.0, inch(12.0)] {
                let shape = TrussShape::standard(kind, ft(24.0), 6.0 / 12.0, overhang).unwrap();
                shape.validate().unwrap();
                let mut m = Model::new("t");
                let mat = m.add_material(graded("DFL", "No.2"));
                Truss::new("T", Vec3::ZERO, Vec3::Y, shape, mat).build(&mut m);
                let topo = Topology::build(&m);
                let issues = validate(&m, &topo);
                assert!(issues.iter().all(|i| i.severity < Severity::Warning), "{kind:?}: {issues:#?}");
                let g = Geometry::build(&m, &topo);
                assert!(g.issues.is_empty(), "{kind:?}: {:#?}", g.issues);
                let clashes: Vec<String> = g
                    .clashes()
                    .iter()
                    .map(|(a, b, d)| format!("{} {:?} / {} {:?}: {:.1} mm", m.member(*a).role, m.member(*a).path, m.member(*b).role, m.member(*b).path, d * 1000.0))
                    .collect();
                assert!(clashes.is_empty(), "{kind:?} overhang {overhang}: {clashes:#?}");
                // Raw geometric bearing (ignoring the plate exemption, since every
                // truss joint is plated). The one known exception is the scissors
                // apex post, which sits on a ridge between two sloped chords and
                // would need a notch; its fit-up gap must stay small.
                for (a, b, d) in g.bearing_overhangs() {
                    let apex_post = matches!(kind, StandardTruss::Scissors { .. }) && m.member(a).role == "web" && m.member(b).role == "bottom_chord";
                    assert!(apex_post && d < inch(1.0), "{kind:?} overhang {overhang}: end of {a} overhangs {b} by {:.1} mm", d * 1000.0);
                }
                assert!(g.bearing_issues(&m).is_empty(), "{kind:?}: plated joints are exempt");
            }
        }
    }

    #[test]
    fn howe_and_pratt_diagonals_run_opposite_ways() {
        let l = ft(24.0);
        let diag_dir = |kind| {
            let sh = TrussShape::standard(kind, l, 0.5, 0.0).unwrap();
            // First left diagonal: does it rise toward the centre?
            let d = sh.members.iter().find(|m| m.role == ShapeRole::Web && {
                let (a, b) = (sh.point(&m.path[0]).unwrap(), sh.point(&m.path[1]).unwrap());
                (a.s - b.s).abs() > 1e-9 && a.s.max(b.s) < l / 2.0
            }).unwrap();
            let (a, b) = (sh.point(&d.path[0]).unwrap(), sh.point(&d.path[1]).unwrap());
            let (lo, hi) = if a.z < b.z { (a, b) } else { (b, a) };
            hi.s > lo.s // upper end nearer the centre?
        };
        assert!(diag_dir(StandardTruss::Howe { panels: 6 }), "Howe diagonals rise toward the centre");
        assert!(!diag_dir(StandardTruss::Pratt { panels: 6 }), "Pratt diagonals fall toward the centre");
    }

    #[test]
    fn rejects_bad_shapes() {
        assert!(TrussShape::standard(StandardTruss::Howe { panels: 5 }, 10.0, 0.5, 0.0).is_err());
        let mut sh = TrussShape::standard(StandardTruss::Fink, 10.0, 0.5, 0.0).unwrap();
        sh.points.iter_mut().find(|p| p.name == "T1").unwrap().z += 0.1; // kink the top chord
        assert!(sh.validate().unwrap_err().contains("not on the straight line"));
    }
}
