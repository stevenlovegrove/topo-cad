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
use topo_core::scene;

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
    /// ridge by this much than their place at the spacing (a reading).
    pub shifts: Vec<(Vec<usize>, f64)>,
}

#[derive(Clone, Debug, Default)]
pub struct RoofParts {
    pub group: GroupId,
    pub trusses: Vec<TrussParts>,
}

impl RoofParts {
    pub fn bottom_chords(&self) -> Vec<MemberId> {
        self.trusses.iter().flat_map(|t| t.bottom_chords.clone()).collect()
    }
    pub fn all(&self) -> Vec<MemberId> {
        self.trusses.iter().flat_map(|t| t.all()).collect()
    }
}

/// A roof placed in a scene: its trusses (each a free body of it, written by its generator),
/// and how each sits on what is under it.
#[derive(Clone, Debug, Default)]
pub struct PlacedRoof {
    pub parts: RoofParts,
    /// Each contact: the node (on a line of a bottom chord's underside and of a bearing's top),
    /// the chord, and what it is on.
    pub rests: Vec<(scene::NodeRef, MemberId, scene::MemberRef)>,
    /// For each truss, what carries it (places in the bearings given).
    pub on: Vec<Vec<usize>>,
    /// Trusses with nothing under them, or not held by what is (one bearing, or all to one side
    /// of their middle): said.
    pub issues: Vec<Issue>,
}

/// Something under a truss that may carry it: a line of a bearing's top (`across` from its
/// middle), where the truss is over it, as things were put.
#[derive(Clone, Debug)]
struct Under {
    bearing: usize,
    across: f64,
    /// Which part of the bearing's top, in words.
    label: String,
    /// The point of the top, in the world; how far along the span from the heel it is.
    top: Vec3,
    s: f64,
    /// How high the truss's heel must be for its underside to touch here (above the heel as put).
    need: f64,
    /// Where the truss crosses a bearing: which crossing it is one of the lines of (its edges and
    /// middle), and that crossing's point on the top's axis, if the axis is in the top.
    crossing: Option<usize>,
    axis: Option<(f64, Vec3)>,
}

impl TrussRoof {
    /// Trusses spanning along `span_dir` from the heel line at `origin`; the
    /// ridge runs to the right of the span direction (so `run × span` is up).
    pub fn new(name: &str, origin: Vec3, span_dir: Vec3, shape: TrussShape, length: f64, spacing: f64, material: MaterialId) -> TrussRoof {
        let span_dir = span_dir.normalized();
        let run_dir = v3(span_dir.y, -span_dir.x, 0.0);
        TrussRoof { name: name.into(), origin, span_dir, run_dir, shape, length, spacing, chord: (2, 4), web: (2, 4), material, overrides: vec![], shifts: vec![] }
    }

    /// Where the layout puts each truss along the ridge from the origin, and which side of its
    /// plane its lumber is on (the gable ends are flush).
    fn stations(&self) -> Vec<(f64, Option<Vec3>)> {
        let run = self.run_dir.normalized();
        let b = inch(1.5);
        let mut stations = vec![(0.0, Some(run))];
        stations.extend((1..).map(|k| k as f64 * self.spacing).take_while(|&r| r < self.length - 1.5 * b).map(|r| (r, None)));
        stations.push((self.length, Some(-run)));
        stations
    }

    fn shape_of(&self, i: usize) -> &TrussShape {
        self.overrides.iter().find(|(ns, _)| ns.contains(&(i + 1))).map(|(_, s)| s).unwrap_or(&self.shape)
    }

    /// How far along the ridge from its station truss `i` was read to be.
    fn read(&self, i: usize) -> f64 {
        self.shifts.iter().filter(|(ns, _)| ns.contains(&(i + 1))).map(|(_, d)| d).sum()
    }

    fn group(&self, m: &mut Model, origin: Vec3) -> GroupId {
        let run = self.run_dir.normalized();
        let frame = Frame { origin, x: run, y: self.span_dir, z: run.cross(self.span_dir) };
        let g = m.add_group(&self.name, "roof", frame, None);
        m.name_direction(g, "ridge", Vec3::X);
        m.name_direction(g, "span", Vec3::Y);
        let over: f64 = self.shape.points.iter().map(|p| (-p.s).max(p.s - self.shape.span)).fold(0.0, f64::max);
        let callout = format!("PREFAB {} TRUSSES @ {} O.C.", self.shape.name.to_uppercase(), topo_core::units::fmt_inches(self.spacing));
        let group = m.group_mut(g);
        group.annotations.push(Annotation::Span { a: v3(self.spacing * 0.5, -over, 0.0), b: v3(self.spacing * 0.5, self.shape.span + over, 0.0), text: callout.clone() });
        group.annotations.push(Annotation::Dim { a: v3(0.0, 0.0, 0.0), b: v3(self.length, 0.0, 0.0), side: -Vec3::Y, tier: 1, text: None });
        group.props.insert("framing".into(), callout);
        g
    }

    fn truss(&self, i: usize, heel: Vec3, face: Option<Vec3>, g: GroupId) -> Truss {
        let mut t = Truss::new(&format!("T{}", i + 1), heel, self.span_dir, self.shape_of(i).clone(), self.material);
        t.chord = self.chord;
        t.web = self.web;
        t.face = face;
        t.parent = Some(g);
        t
    }

    /// The trusses where the layout (and what was read of them) puts them, at the origin's height.
    pub fn build(&self, m: &mut Model) -> RoofParts {
        let g = self.group(m, self.origin);
        let run = self.run_dir.normalized();
        let trusses = (self.stations().into_iter().enumerate()).map(|(i, (r, face))| self.truss(i, self.origin + run * (r + self.read(i)), face, g).build(m)).collect();
        RoofParts { group: g, trusses }
    }

    /// The trusses as free bodies of a scene, each written by its generator, sat on what is under
    /// them among `bearings` (as things were put):
    ///
    /// - each truss's heel is a node (`Roof/T1/heel`); where the layout puts it along the ridge
    ///   from the node the roof is set out from (a point of the world, of a scan's frame, of the
    ///   walls) is a fallback (layout: below what is read of it, and below its being upright),
    ///   a reading of it (`shifted`) a reading, and
    ///   across the ridge it is where the layout puts it (hard). Its plane is plumb and square
    ///   to the ridge, and it is level, unless what it sits on says otherwise (the body's hints);
    /// - what carries it is found by a utility, by geometry, as things were put: the lines of
    ///   the bearings' tops under any part of its bottom chords (across a bearing: its two
    ///   edges and its middle; along one, each end of where they overlap); the two of the
    ///   upper hull of them (how high the truss must be to touch each) that its middle is
    ///   between, or one if it is not between two (said: nothing holds it so); where the chord
    ///   lies on the whole of a top (level on it), the top's axis, if that is in it. Each is a
    ///   `rest_on_lines`: a node on the chord's underside and on that line of the top, one
    ///   number along each. A truss with nothing under it keeps the height it started from (a
    ///   hint), and that is said.
    ///
    /// The trusses start at the height of the highest top. The scene's solve sits them; writing
    /// it moves each truss, and makes each contact a node of the chord and of what it is on.
    pub fn place(&self, m: &mut Model, sc: &mut scene::Scene, set_out: scene::NodeRef, bearings: &[scene::MemberRef]) -> Result<PlacedRoof, String> {
        let (span, run) = (self.span_dir, self.run_dir.normalized());
        let normal = Vec3::Z.cross(span);
        let tops: Vec<(Face, FacePlane, Option<f64>, String)> = bearings
            .iter()
            .map(|&b| {
                let (face, top) = sc.face_toward(b, Vec3::Z, m);
                let (o, _) = sc.axis(b);
                let on = ((o - top.origin).dot(top.normal).abs() < 1e-9).then(|| (o - top.origin).dot(top.across));
                (face, top, on, sc.label(b).to_string())
            })
            .collect();
        let start = tops.iter().map(|(_, t, ..)| (t.origin.z).max((t.origin + t.along * t.length).z)).fold(f64::MIN, f64::max);
        let at = sc.position(set_out);
        let origin = v3(at.x, at.y, if tops.is_empty() { at.z } else { start });
        let g = self.group(m, origin);
        let mut placed = PlacedRoof { parts: RoofParts { group: g, trusses: vec![] }, ..Default::default() };
        let thick = {
            let bb = sawn(self.chord.0, self.chord.1).bbox();
            bb.width().min(bb.height())
        };
        for (i, (r, face)) in self.stations().into_iter().enumerate() {
            let name = format!("{}/T{}", self.name, i + 1);
            let read = self.read(i);
            let heel = origin + run * (r + read);
            // The truss, as its generator writes it, a free body.
            let before = (m.nodes.len(), m.members.len(), m.groups.len());
            let parts = m.scoped(|m| self.truss(i, heel, face, g).build(m));
            let content = scene::Content {
                group: parts.group,
                groups: (before.2..m.groups.len()).map(|k| GroupId(k as u32)).collect(),
                nodes: (before.0..m.nodes.len()).map(|k| NodeId(k as u32)).collect(),
                members: (before.1..m.members.len()).map(|k| MemberId(k as u32)).collect(),
            };
            let body = sc.body_written(&name, Frame { origin: heel, x: span, y: normal, z: Vec3::Z }, true, content.clone());
            // Its heel (its own node there, if it has one), and its station: from the node the roof is
            // set out from, or a point of the world.
            let heel_name = format!("{name}/heel");
            let heel_node = match content.nodes.iter().copied().find(|&n| m.pos(n).distance(heel) < 1e-9) {
                Some(n) => sc.node_built(&heel_name, body, Vec3::ZERO, &[], n)?,
                None => sc.node(&heel_name, body, Vec3::ZERO)?,
            };
            let station = |sc: &mut scene::Scene, what: &str, dir: Vec3, value: f64, how: scene::Tier| sc.offset(&format!("{name} {what}"), set_out, heel_node, dir, value + if dir == run { r } else { 0.0 }, how);
            station(sc, "at its station along the ridge", run, 0.0, scene::Tier::Fallback);
            if read != 0.0 {
                station(sc, &format!("read {:.0} mm along the ridge from its station", read * 1000.0), run, read, scene::Tier::Reading(0.001));
            }
            station(sc, "at its place along the span", span, 0.0, scene::Tier::Hard);
            // What carries it.
            let shape = self.shape_of(i);
            let chords: Vec<[(f64, f64); 2]> = (shape.members.iter().filter(|m| m.role == ShapeRole::BottomChord))
                .filter_map(|m| Some([shape.point(&m.path[0])?, shape.point(m.path.last()?)?].map(|p| (p.s, p.z))))
                .map(|[a, b]| if a.0 <= b.0 { [a, b] } else { [b, a] })
                .collect();
            let underside = |s: f64| chords.iter().filter(|[a, b]| s >= a.0 - 1e-9 && s <= b.0 + 1e-9 && b.0 > a.0).map(|[a, b]| a.1 + (b.1 - a.1) * (s - a.0) / (b.0 - a.0)).min_by(|a, b| a.total_cmp(b));
            let (lo, hi) = chords.iter().fold((f64::MAX, f64::MIN), |(lo, hi), [a, b]| (lo.min(a.0), hi.max(b.0)));
            let middle = heel + face.map_or(Vec3::ZERO, |f| run * (f.dot(run).signum() * thick / 2.0));
            let mut under: Vec<Under> = vec![];
            for (k, (_, top, axis, label)) in tops.iter().enumerate() {
                let along = top.along;
                let a = top.origin;
                let mut found = vec![];
                let crossing = under.len();
                let mut at = |p: Vec3, across: f64, part: &str, crossing: Option<usize>, axis: Option<(f64, Vec3)>| {
                    let s = (p - heel).dot(span);
                    if let Some(z) = underside(s) {
                        found.push(Under { bearing: k, across, label: format!("{part} of {label}"), top: p, s, need: p.z - z - heel.z, crossing, axis });
                    }
                };
                if along.dot(run).abs() >= 0.2 {
                    // Across the truss: under it where it crosses the chord's axis (its underside, in the
                    // truss's plane); the top's two edges and its middle.
                    let t = (heel - a).dot(run) / along.dot(run);
                    if t > -thick / 2.0 - 1e-6 && t < top.length + thick / 2.0 + 1e-6 {
                        let edge = top.half_width / along.dot(run).abs();
                        let side = span.dot(top.across).signum();
                        let on_axis = axis.map(|x| (x, a + along * t + top.across * x));
                        if edge > 0.0 {
                            at(a + along * t - span * edge, -side * top.half_width, "near edge", Some(crossing), on_axis);
                            at(a + along * t, 0.0, "middle", Some(crossing), on_axis);
                            at(a + along * t + span * edge, side * top.half_width, "far edge", Some(crossing), on_axis);
                        } else {
                            at(a + along * t, 0.0, "top", None, None);
                        }
                    }
                } else {
                    // Along the truss: under it for as far as the two overlap, if it is under the lumber at all.
                    let b = a + along * top.length;
                    let (sa, sb) = ((a - heel).dot(span), (b - heel).dot(span));
                    let (from, to) = (sa.min(sb).max(lo), sa.max(sb).min(hi));
                    let point = |s: f64| a + (b - a) * ((s - sa) / (sb - sa));
                    if to - from > 1e-6 && (point((from + to) / 2.0) - middle).dot(run).abs() <= top.half_width + thick / 2.0 + 1e-6 {
                        at(point(from), 0.0, "near end", None, None);
                        at(point(to), 0.0, "far end", None, None);
                    }
                }
                under.extend(found);
            }
            let short = format!("T{}", i + 1);
            if under.is_empty() {
                if !tops.is_empty() {
                    placed.issues.push(Issue::new(Severity::Error, "truss-not-seated", format!("roof {}: truss {short} bears on nothing: none of what the roof bears on is under any part of it", self.name)).members(parts.bottom_chords.clone()));
                }
                sc.offset_from(&format!("{name} at the height it started from"), heel, heel_node, Vec3::Z, 0.0, scene::Tier::Hint);
                placed.parts.trusses.push(parts);
                placed.on.push(vec![]);
                continue;
            }
            // What carries it: the upper hull of (where, how high the truss must be to touch there).
            let mut order: Vec<usize> = (0..under.len()).collect();
            order.sort_by(|&a, &b| under[a].s.total_cmp(&under[b].s).then(under[b].need.total_cmp(&under[a].need)));
            order.dedup_by(|b, a| (under[*a].s - under[*b].s).abs() < 1e-9);
            let mut hull: Vec<usize> = vec![];
            for k in order {
                while hull.len() >= 2 {
                    let (a, b) = (hull[hull.len() - 2], hull[hull.len() - 1]);
                    let turn = (under[b].s - under[a].s) * (under[k].need - under[b].need) - (under[b].need - under[a].need) * (under[k].s - under[b].s);
                    if turn >= -1e-12 {
                        hull.pop();
                    } else {
                        break;
                    }
                }
                hull.push(k);
            }
            let mid = (lo + hi) / 2.0;
            let carry: Vec<usize> = match hull.windows(2).find(|w| under[w[0]].s <= mid && mid <= under[w[1]].s) {
                Some(w) => w.to_vec(),
                None => vec![*hull.iter().min_by(|&&a, &&b| (under[a].s - mid).abs().total_cmp(&(under[b].s - mid).abs())).unwrap()],
            };
            // It touches every line under it that is on the line through what carries it (as it will
            // sit), and each is a contact. Where the chord lies on the whole of a top it crosses (on
            // all its lines), the contact is the top's axis, if that is in the top.
            let sits = |s: f64| match carry[..] {
                [a, b] => under[a].need + (under[b].need - under[a].need) * (s - under[a].s) / (under[b].s - under[a].s),
                _ => under[carry[0]].need,
            };
            let touching: Vec<&Under> = under.iter().filter(|u| (u.need - sits(u.s)).abs() < 1e-9).collect();
            let mut contacts: Vec<Under> = vec![];
            for (k, c) in touching.iter().enumerate() {
                let lines: Vec<&&Under> = touching.iter().filter(|u| u.crossing.is_some() && u.crossing == c.crossing).collect();
                match (c.axis, lines.len()) {
                    (Some((across, p)), 3) => {
                        // (Once for the crossing.)
                        if touching[..k].iter().all(|u| u.crossing != c.crossing) {
                            contacts.push(Under { across, label: c.label.replacen(c.label.split(" of ").next().unwrap(), "axis", 1), top: p, s: (p - heel).dot(span), ..(*c).clone() });
                        }
                    }
                    _ => contacts.push((*c).clone()),
                }
            }
            if carry.len() < 2 {
                placed.issues.push(Issue::new(Severity::Error, "truss-not-held", format!("roof {}: truss {short} is not held: what is under it is one bearing, or all to one side of its middle", self.name)).members(parts.bottom_chords.clone()));
            }
            // Each contact: on the chord's underside (the chord that is over it) and on that line of the top.
            for u in &contacts {
                let chord = (parts.bottom_chords.iter().copied())
                    .find(|&c| {
                        let (a, b) = ((m.pos(m.member(c).start()) - heel).dot(span), (m.pos(m.member(c).end()) - heel).dot(span));
                        u.s >= a.min(b) - 1e-6 && u.s <= a.max(b) + 1e-6
                    })
                    .ok_or_else(|| format!("roof {}: {short}: no bottom chord over the {}", self.name, u.label))?;
                let chord_ref = sc.import_member(m, chord, &m.member_path(chord));
                let (_, under_face) = sc.face_toward(chord_ref, -Vec3::Z, m);
                let (on_chord, on_top) = if u.crossing.is_some() || u.label.starts_with("top ") {
                    // Across the top: on the chord, its axis (which is in its underside).
                    ((sc.axis(chord_ref).0 - under_face.origin).dot(under_face.across), u.across)
                } else {
                    // Along the top: on the chord's axis, where that is over the top; else the middle of
                    // where the chord's underside and the top overlap, across.
                    let top = &tops[u.bearing].1;
                    let (cr, tr) = (under_face.across.dot(run).signum(), top.across.dot(run).signum());
                    let (c, t) = ((under_face.origin - heel).dot(run), (top.origin - heel).dot(run));
                    let axis = (sc.axis(chord_ref).0 - heel).dot(run);
                    let at = if (axis - t).abs() <= top.half_width + 1e-9 {
                        axis
                    } else {
                        ((c - under_face.half_width).max(t - top.half_width) + (c + under_face.half_width).min(t + top.half_width)) / 2.0
                    };
                    ((at - c) * cr, (at - t) * tr)
                };
                // (Along a top, the chord may lie anywhere across it: the line found is a hint.)
                let along_it = !(u.crossing.is_some() || u.label.starts_with("top "));
                let n = sc.rest_on_lines(&format!("{name} on the {}", u.label), chord_ref, on_chord, bearings[u.bearing], on_top, u.top, along_it, m)?;
                placed.rests.push((n, chord, bearings[u.bearing]));
            }
            placed.on.push(contacts.iter().map(|u| u.bearing).collect());
            placed.parts.trusses.push(parts);
        }
        Ok(placed)
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

    fn roof_on(bearings: Vec<(Vec3, Vec3)>, shape: StandardTruss) -> (Model, PlacedRoof) {
        roof_on_wide(bearings, shape, inch(1.5))
    }

    /// A roof sat on beams of this width (their tops along these lines), in a scene, and written.
    fn roof_on_wide(bearings: Vec<(Vec3, Vec3)>, shape: StandardTruss, width: f64) -> (Model, PlacedRoof) {
        let mut m = Model::new("seated");
        let mat = m.add_material(graded("DFL", "No.2"));
        let sec = m.add_section(Section::rect("beam", width, inch(5.5)));
        let mut sc = scene::Scene::new("roof");
        let beams: Vec<scene::MemberRef> = (bearings.iter().enumerate())
            .map(|(k, &(a, b))| {
                let axes = MemberAxes::new(a, b, Some(Vec3::Z));
                let spec = MemberSpec::new("beam", sec, mat).depth_dir(Vec3::Z).anchor(Anchor::body_toward(&axes, None, Some(-Vec3::Z)));
                let id = m.add_member_between(a, b, &spec.named(&format!("beam{}", k + 1)));
                sc.import_member(&m, id, &format!("beam {}", k + 1))
            })
            .collect();
        let shape = TrussShape::standard(shape, 6.0, 4.0 / 12.0, 0.0).unwrap();
        // It starts from the highest top; what is under it says where it sits.
        let roof = TrussRoof::new("Roof", v3(0.0, 0.0, 2.4), Vec3::Y, shape, 4.0 * inch(24.0), inch(24.0), mat);
        let set_out = sc.frame_point("World/set-out", None, [Some(0.0), Some(0.0), Some(2.4)]).unwrap();
        let placed = roof.place(&mut m, &mut sc, set_out, &beams).unwrap();
        sc.write(&mut m).unwrap();
        (m, placed)
    }

    fn said(p: &PlacedRoof, code: &str) -> Vec<String> {
        p.issues.iter().filter(|i| i.code == code).map(|i| i.message.split(": truss ").nth(1).unwrap().split(' ').next().unwrap().to_string()).collect()
    }

    #[test]
    fn trusses_on_level_bearings_are_exactly_where_the_layout_puts_them() {
        // The ridge runs along x; plates under each heel line, a little inside it (as a wall's line is).
        let mut plain = Model::new("plain");
        let mat = plain.add_material(graded("DFL", "No.2"));
        let shape = TrussShape::standard(StandardTruss::Fink, 6.0, 4.0 / 12.0, 0.0).unwrap();
        let laid = TrussRoof::new("Roof", v3(0.0, 0.0, 2.4), Vec3::Y, shape, 4.0 * inch(24.0), inch(24.0), mat).build(&mut plain);
        let (m, placed) = roof_on(vec![(v3(-0.5, 0.05, 2.4), v3(3.0, 0.05, 2.4)), (v3(-0.5, 5.95, 2.4), v3(3.0, 5.95, 2.4))], StandardTruss::Fink);
        assert!(placed.on.iter().all(|on| on == &[0, 1]) && placed.issues.is_empty(), "{placed:#?}");
        for (t, p) in placed.parts.trusses.iter().zip(&laid.trusses) {
            for (&a, &b) in t.all().iter().zip(&p.all()) {
                let (ma, mb) = (m.member(a), plain.member(b));
                assert!(m.pos(ma.start()).distance(plain.pos(mb.start())) < 1e-9 && m.pos(ma.end()).distance(plain.pos(mb.end())) < 1e-9);
            }
        }
        // Where a chord is level on a plate, it sits on the plate's axis (here, the middle of its top): a node of both.
        let beam = m.members.iter().find(|x| x.role == "beam").unwrap();
        let chord = m.member(placed.parts.trusses[1].bottom_chords[0]);
        assert!(beam.path.iter().any(|n| chord.path.contains(n)), "{:?} {:?}", beam.path, chord.path);
    }

    #[test]
    fn trusses_sit_on_what_is_under_them_and_tilt_as_it_says() {
        // The far plate 38 mm lower; under the second and third trusses, near their near heels, a header's
        // cap 30 mm above the plate; and under the middle of all of them, a beam too low to touch.
        let (m, placed) = roof_on(
            vec![
                (v3(-0.5, 0.05, 2.4), v3(3.0, 0.05, 2.4)),
                (v3(-0.5, 5.95, 2.362), v3(3.0, 5.95, 2.362)),
                (v3(0.4, 0.3, 2.43), v3(1.4, 0.3, 2.43)),
                (v3(-0.5, 3.0, 2.2), v3(3.0, 3.0, 2.2)),
            ],
            StandardTruss::Fink,
        );
        assert!(placed.issues.is_empty(), "{placed:#?}");
        for (i, t) in placed.parts.trusses.iter().enumerate() {
            let on_header = i == 1 || i == 2;
            let (a, b) = low_ends(&m, t);
            // The chord's bottom is a straight line through what carries it (it slopes down toward the far plate, so
            // touches each top at its edge nearer the far side): the far plate's outer edge, and the inner edge of the
            // near plate or of the header's cap.
            let at = |y: f64| a.z + (b.z - a.z) * (y - a.y) / (b.y - a.y);
            let (near, height) = if on_header { (0.3 + inch(0.75), 2.43) } else { (0.05 + inch(0.75), 2.4) };
            assert!((at(near) - height).abs() < 1e-7 && (at(5.95 + inch(0.75)) - 2.362).abs() < 1e-7, "T{}: {} {}", i + 1, at(near), at(5.95 + inch(0.75)));
            // It sits on those two and on nothing else.
            let mut on = placed.on[i].clone();
            on.sort();
            assert_eq!(on, if on_header { vec![1, 2] } else { vec![0, 1] }, "T{}", i + 1);
            // Its heel is where the layout puts it on plan, and its plane is plumb and square to the ridge.
            assert!((a.x - i as f64 * inch(24.0)).abs() < 1e-7 && a.y.abs() < 1e-7 && (b.x - a.x).abs() < 1e-7, "T{}: {a:?} {b:?}", i + 1);
        }
    }

    #[test]
    fn a_truss_sits_on_whatever_is_under_any_part_of_it() {
        // No plates under the heels: two beams inboard, at the third points. It sits on those, level, at their height.
        let (m, placed) = roof_on(vec![(v3(-0.5, 2.0, 2.5), v3(3.0, 2.0, 2.5)), (v3(-0.5, 4.0, 2.5), v3(3.0, 4.0, 2.5))], StandardTruss::Fink);
        assert!(placed.on.iter().all(|on| on.len() == 2) && placed.issues.is_empty(), "{placed:#?}");
        let (a, b) = low_ends(&m, &placed.parts.trusses[2]);
        assert!((a.z - 2.5).abs() < 1e-9 && (b.z - 2.5).abs() < 1e-9, "{a:?} {b:?}");

        // One beam only: it sits on it, level (nothing else saying), and it is said that nothing holds it so.
        let (m, placed) = roof_on(vec![(v3(-0.5, 2.0, 2.5), v3(3.0, 2.0, 2.5))], StandardTruss::Fink);
        assert!(placed.on.iter().all(|on| on.len() == 1) && said(&placed, "truss-not-held") == ["T1", "T2", "T3", "T4", "T5"], "{placed:#?}");
        let (a, b) = low_ends(&m, &placed.parts.trusses[2]);
        assert!((a.z - 2.5).abs() < 1e-9 && (b.z - 2.5).abs() < 1e-9, "{a:?} {b:?}");

        // A wall along the first truss (as under a gable), a little to one side of its plane but under its
        // lumber: that truss sits on it along its length. The others have nothing under them, and say so.
        let (m, placed) = roof_on_wide(vec![(v3(0.03, 0.0, 2.45), v3(0.03, 6.0, 2.45))], StandardTruss::Fink, 0.09);
        assert_eq!(placed.on[0].len(), 2, "{placed:#?}");
        assert_eq!(said(&placed, "truss-not-seated"), ["T2", "T3", "T4", "T5"]);
        let (a, b) = low_ends(&m, &placed.parts.trusses[0]);
        assert!((a.z - 2.45).abs() < 1e-9 && (b.z - 2.45).abs() < 1e-9, "{a:?} {b:?}");
    }

    #[test]
    fn a_chord_that_rises_sits_where_it_is_over_the_bearing() {
        // Scissors: the bottom chords rise from the heels. On beams a metre in from each heel, the chord's
        // underside sits on each beam's outer edge (it does not sink into the beam).
        let (m, placed) = roof_on_wide(vec![(v3(-0.5, 1.0, 2.5), v3(3.0, 1.0, 2.5)), (v3(-0.5, 5.0, 2.5), v3(3.0, 5.0, 2.5))], StandardTruss::Scissors { bottom_pitch: 2.0 / 12.0 }, 0.1);
        assert!(placed.on.iter().all(|on| on.len() == 2) && placed.issues.is_empty(), "{placed:#?}");
        let heel = m.pos(m.member(placed.parts.trusses[2].bottom_chords[0]).start());
        assert!((heel.z - (2.5 - 0.95 * 2.0 / 12.0)).abs() < 1e-9, "{heel:?}");
        // The contact is a node of the chord (on its axis, its underside) and on the beam's top face, at its edge.
        let beam = m.members.iter().find(|x| x.role == "beam").unwrap().id;
        let on: Vec<_> = m.attachments.iter().filter(|a| a.member == beam).collect();
        assert!(!on.is_empty() && on.iter().all(|a| (a.across.abs() - 0.05).abs() < 1e-9), "{on:#?}");
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
