//! Stud wall generator: plates, studs, openings (kings, jacks, headers, sills,
//! cripples) as topology. All positions are in the wall's local frame:
//! x along the wall line, y toward the interior, z up. The wall line lies on the
//! exterior face of framing (or the centre line for `Justify::Center`) at the
//! bottom of the bottom plate; `height` is to the top of the double top plate.

use crate::fastening as fx;
use crate::lumber::{built_up, dressed_in, sawn};
use topo_core::units::inch;
use topo_core::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Justify {
    /// Wall line on the exterior face of framing.
    Exterior,
    /// Wall line on the centre of the framing (interior partitions).
    Center,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpeningKind {
    Window,
    Door,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Opening {
    pub label: String,
    pub kind: OpeningKind,
    /// Centre of the rough opening, measured from the wall start (m).
    pub center: f64,
    /// Rough opening width and height (m).
    pub width: f64,
    pub height: f64,
    /// Top of rough opening above the bottom of the wall (m).
    pub head: f64,
    /// Header as (plies, nominal thickness, nominal depth).
    pub header: (u32, u32, u32),
    /// Jack (trimmer) studs each side. `0` models an as-built header carried by
    /// the king studs through nails alone.
    pub jacks: u32,
    /// Header set tight under the top plate; `head` (and, for doors, `height`)
    /// are then derived from the wall height and header depth.
    pub header_flush: bool,
}

impl Opening {
    pub fn window(label: &str, center: f64, width: f64, height: f64, head: f64) -> Opening {
        Opening { label: label.into(), kind: OpeningKind::Window, center, width, height, head, header: (2, 2, 8), jacks: 1, header_flush: false }
    }
    pub fn door(label: &str, center: f64, width: f64, height: f64) -> Opening {
        Opening { label: label.into(), kind: OpeningKind::Door, center, width, height, head: height, header: (2, 2, 8), jacks: 1, header_flush: false }
    }
    pub fn header(mut self, plies: u32, thick: u32, depth: u32) -> Opening {
        self.header = (plies, thick, depth);
        self
    }
    pub fn jacks(mut self, n: u32) -> Opening {
        self.jacks = n;
        self
    }
    /// Door whose header sits tight under the top plate (common for garage doors).
    pub fn header_flush(mut self) -> Opening {
        self.header_flush = true;
        self
    }
    fn sill(&self) -> f64 {
        self.head - self.height
    }
}

#[derive(Clone, Debug)]
pub struct Wall {
    pub name: String,
    pub start: Vec3,
    pub end: Vec3,
    pub height: f64,
    /// Stud nominal size, e.g. (2, 4).
    pub stud: (u32, u32),
    pub material: MaterialId,
    /// Material for headers (often a higher grade); defaults to `material`.
    pub header_material: Option<MaterialId>,
    pub spacing: f64,
    /// Interior on the left of the start→end direction (CCW plan loops).
    pub interior_left: bool,
    pub justify: Justify,
    pub openings: Vec<Opening>,
    /// Keep layout studs clear of a wall this one butts into at its start/end.
    pub start_inset: f64,
    pub end_inset: f64,
    /// Positions along the wall where the cap plate is interrupted so an
    /// intersecting partition's cap can lap through (see `lap_tee`).
    pub cap_breaks: Vec<f64>,
    pub parent: Option<GroupId>,
}

/// Handles to the generated members.
#[derive(Clone, Debug, Default)]
pub struct WallParts {
    pub group: GroupId,
    pub bottom_plates: Vec<MemberId>,
    /// Lower ply of the double top plate (studs end on it).
    pub top_plate: MemberId,
    /// Upper ply(s): the cap plate that laps at corners and intersections.
    pub cap_plates: Vec<MemberId>,
    pub studs: Vec<MemberId>,
    pub headers: Vec<MemberId>,
}

impl WallParts {
    pub fn plates(&self) -> Vec<MemberId> {
        let mut v = self.bottom_plates.clone();
        v.push(self.top_plate);
        v.extend(&self.cap_plates);
        v
    }
}

fn ends(m: &Model, id: MemberId) -> [NodeId; 2] {
    [m.member(id).start(), m.member(id).end()]
}

/// Laps the double top plate at a corner. `through` is the wall whose lower
/// top plate runs through the corner; the other wall's cap plate runs through
/// instead, so the plies interleave (IRC R602.3.2). Returns false if the two
/// walls' cap plates do not meet.
pub fn lap_corner(m: &mut Model, through: &WallParts, butting: &WallParts, c: ConnectionId) -> bool {
    for &ct in &through.cap_plates {
        for &cb in &butting.cap_plates {
            if let Some(n) = ends(m, ct).into_iter().find(|n| ends(m, cb).contains(n)) {
                m.add_rule(n, JointRule::Through { member: cb });
                m.add_rule(n, JointRule::Butt { member: ct, against: cb });
                m.connect(n, cb, Some(ct), c);
                return true;
            }
        }
    }
    false
}

/// Laps a partition's cap plate over the wall it tees into. `main` must have a
/// cap break at the intersection (`Wall::cap_break`). Returns false if the
/// partition's cap does not end at such a break.
pub fn lap_tee(m: &mut Model, partition: &WallParts, main: &WallParts, c: ConnectionId) -> bool {
    for &p in &partition.cap_plates {
        for n in ends(m, p) {
            let mains: Vec<MemberId> = main.cap_plates.iter().copied().filter(|&mc| ends(m, mc).contains(&n)).collect();
            if mains.is_empty() {
                continue;
            }
            m.add_rule(n, JointRule::Through { member: p });
            for mc in mains {
                m.add_rule(n, JointRule::Butt { member: mc, against: p });
            }
            m.connect(n, p, None, c);
            return true;
        }
    }
    false
}

impl Wall {
    pub fn new(name: &str, start: Vec3, end: Vec3, height: f64, material: MaterialId) -> Wall {
        Wall {
            name: name.into(),
            start,
            end,
            height,
            stud: (2, 4),
            material,
            header_material: None,
            spacing: inch(16.0),
            interior_left: true,
            justify: Justify::Exterior,
            openings: vec![],
            start_inset: 0.0,
            end_inset: 0.0,
            cap_breaks: vec![],
            parent: None,
        }
    }
    /// A wall with no position yet, to be placed by a `Perimeter` (or `between`).
    pub fn template(height: f64, material: MaterialId) -> Wall {
        Wall::new("", Vec3::ZERO, Vec3::ZERO, height, material)
    }
    pub fn named(mut self, name: &str) -> Wall {
        self.name = name.into();
        self
    }
    pub fn between(mut self, start: Vec3, end: Vec3) -> Wall {
        self.start = start;
        self.end = end;
        self
    }
    pub fn studs(mut self, thick: u32, width: u32, spacing: f64) -> Wall {
        self.stud = (thick, width);
        self.spacing = spacing;
        self
    }
    pub fn justify(mut self, j: Justify) -> Wall {
        self.justify = j;
        self
    }
    pub fn opening(mut self, o: Opening) -> Wall {
        self.openings.push(o);
        self
    }
    pub fn insets(mut self, start: f64, end: f64) -> Wall {
        self.start_inset = start;
        self.end_inset = end;
        self
    }
    /// Interrupt the cap plate at `x` along the wall (for a partition tee).
    pub fn cap_break(mut self, x: f64) -> Wall {
        self.cap_breaks.push(x);
        self
    }
    pub fn header_material(mut self, m: MaterialId) -> Wall {
        self.header_material = Some(m);
        self
    }
    pub fn parent(mut self, g: GroupId) -> Wall {
        self.parent = Some(g);
        self
    }
    /// Framing thickness (stud depth).
    pub fn thickness(&self) -> f64 {
        inch(dressed_in(self.stud.1))
    }

    pub fn build(&self, m: &mut Model) -> WallParts {
        let len = self.start.distance(self.end);
        let dir = (self.end - self.start) / len;
        let inward = if self.interior_left { Vec3::Z.cross(dir) } else { dir.cross(Vec3::Z) };
        let frame = Frame { origin: self.start, x: dir, y: inward, z: Vec3::Z };
        let w = |x: f64, z: f64| frame.to_world(v3(x, 0.0, z));
        let (h, b, t) = (self.height, inch(dressed_in(self.stud.0)), self.thickness());
        let v_side = match self.justify {
            Justify::Exterior => Some(inward),
            Justify::Center => None,
        };

        let g = m.add_group(&self.name, "wall", frame, self.parent);
        let stud_sec = m.add_section(sawn(self.stud.0, self.stud.1));
        let mat = self.material;
        let hdr_mat = self.header_material.unwrap_or(mat);
        let c_bot = m.add_connection(fx::stud_to_bottom_plate());
        let c_top = m.add_connection(fx::stud_to_top_plate());
        let c_hdr = m.add_connection(fx::header_to_king());
        let c_jack = m.add_connection(fx::jack_to_king());
        let c_sill = m.add_connection(fx::sill_to_jack());
        let c_crip = m.add_connection(fx::cripple_to_plate());

        // Plates. Door openings interrupt the bottom plate.
        let plate_ax = m.axes_between(self.start, self.end, Some(inward));
        let bottom = MemberSpec::new("bottom_plate", stud_sec, mat)
            .priority(10)
            .depth_dir(inward)
            .anchor(Anchor::body_toward(&plate_ax, Some(Vec3::Z), v_side))
            .group(g);
        let top = MemberSpec::new("top_plate", stud_sec, mat)
            .priority(10)
            .depth_dir(inward)
            .anchor(Anchor::body_toward(&plate_ax, Some(-Vec3::Z), v_side))
            .group(g);
        let mut doors: Vec<(f64, f64)> = self
            .openings
            .iter()
            .filter(|o| o.kind == OpeningKind::Door)
            .map(|o| (o.center - o.width / 2.0, o.center + o.width / 2.0))
            .collect();
        doors.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut segs = vec![];
        let mut x = 0.0;
        for &(d0, d1) in &doors {
            segs.push((x, d0));
            x = d1;
        }
        segs.push((x, len));
        let mut parts = WallParts { group: g, ..Default::default() };
        let mut seg_members = vec![];
        for &(a, bx) in &segs {
            let id = m.add_member_between(w(a, 0.0), w(bx, 0.0), &bottom);
            seg_members.push((a, bx, id));
            parts.bottom_plates.push(id);
        }
        let bottom_at = |x: f64| {
            seg_members.iter().find(|(a, bx, _)| x >= *a && x <= *bx).map(|s| s.2).expect("stud over a door opening")
        };
        // Double top plate as two members: lower ply (axis on its top face)
        // and cap plate(s) above, face-nailed together.
        let ht = h - b;
        parts.top_plate = m.add_member_between(w(0.0, ht), w(len, ht), &top);
        let cap = MemberSpec { role: "cap_plate".into(), ..top.clone() };
        let c_dbl = m.add_connection(fx::double_top_plate());
        let mut breaks: Vec<f64> = self.cap_breaks.iter().copied().filter(|&x| x > 1e-6 && x < len - 1e-6).collect();
        breaks.sort_by(f64::total_cmp);
        let mut x0 = 0.0;
        for x1 in breaks.into_iter().chain([len]) {
            let id = m.add_member_between(w(x0, h), w(x1, h), &cap);
            m.bond(id, parts.top_plate, c_dbl);
            parts.cap_plates.push(id);
            x0 = x1;
        }

        let stud_ax = m.axes_between(Vec3::ZERO, Vec3::Z, Some(inward));
        let vspec = |role: &str| {
            MemberSpec::new(role, stud_sec, mat)
                .depth_dir(inward)
                .anchor(Anchor::body_toward(&stud_ax, None, v_side))
                .group(g)
        };
        // Vertical member from a point on `lo` to a point on `hi`.
        let vertical = |m: &mut Model, x: f64, z0: f64, z1: f64, lo: MemberId, hi: MemberId, role: &str, c0, c1| {
            let n0 = m.node_on(lo, w(x, z0)).expect("vertical base not on supporting member");
            let n1 = m.node_on(hi, w(x, z1)).expect("vertical top not on supporting member");
            let id = m.add_member(&[n0, n1], &vspec(role));
            m.connect(n0, id, Some(lo), c0);
            m.connect(n1, id, Some(hi), c1);
            id
        };

        // Openings (flush headers resolved against the wall height first).
        let openings: Vec<Opening> = self
            .openings
            .iter()
            .map(|o| {
                let mut o = o.clone();
                if o.header_flush {
                    o.head = h - 2.0 * b - inch(dressed_in(o.header.2));
                    if o.kind == OpeningKind::Door {
                        o.height = o.head;
                    }
                }
                o
            })
            .collect();
        let c_plate_hdr = m.add_connection(fx::top_plate_to_header());
        let mut blocked: Vec<(f64, f64)> = vec![];
        let layout: Vec<f64> = (1..).map(|k| k as f64 * self.spacing).take_while(|&x| x < len).collect();
        for o in &openings {
            let (x0, x1) = (o.center - o.width / 2.0, o.center + o.width / 2.0);
            let nj = o.jacks as f64;
            // Posts each side, innermost first: jacks then the king.
            let (xk0, xk1) = (x0 - (nj + 0.5) * b, x1 + (nj + 0.5) * b);
            blocked.push((xk0 - b, xk1 + b));
            let king0 = vertical(m, xk0, 0.0, ht, bottom_at(xk0), parts.top_plate, "king_stud", c_bot, c_top);
            let king1 = vertical(m, xk1, 0.0, ht, bottom_at(xk1), parts.top_plate, "king_stud", c_bot, c_top);

            let (plies, hdr_thick, hd) = o.header;
            let gap = if plies > 1 { ((t - plies as f64 * b) / (plies - 1) as f64).max(0.0) / inch(1.0) } else { 0.0 };
            let hdr_sec = m.add_section(built_up(plies, hdr_thick, hd, gap));
            let hn0 = m.node_on(king0, w(xk0, o.head)).unwrap();
            let hn1 = m.node_on(king1, w(xk1, o.head)).unwrap();
            let hax = m.axes_between(w(xk0, o.head), w(xk1, o.head), Some(Vec3::Z));
            let header = m.add_member(
                &[hn0, hn1],
                &MemberSpec::new("header", hdr_sec, hdr_mat)
                    .priority(20)
                    .depth_dir(Vec3::Z)
                    .anchor(Anchor::body_toward(&hax, v_side, Some(Vec3::Z)))
                    .group(g),
            );
            m.connect(hn0, header, Some(king0), c_hdr);
            m.connect(hn1, header, Some(king1), c_hdr);
            parts.headers.push(header);
            parts.studs.extend([king0, king1]);

            // Jacks, bonded to each other and to the king.
            let (mut post0, mut post1) = (king0, king1);
            for k in (0..o.jacks).rev() {
                let (xj0, xj1) = (x0 - (k as f64 + 0.5) * b, x1 + (k as f64 + 0.5) * b);
                let jack0 = vertical(m, xj0, 0.0, o.head, bottom_at(xj0), header, "jack_stud", c_bot, c_crip);
                let jack1 = vertical(m, xj1, 0.0, o.head, bottom_at(xj1), header, "jack_stud", c_bot, c_crip);
                m.bond(jack0, post0, c_jack);
                m.bond(jack1, post1, c_jack);
                parts.studs.extend([jack0, jack1]);
                (post0, post1) = (jack0, jack1);
            }
            let (xp0, xp1) = (x0 - 0.5 * b, x1 + 0.5 * b);

            let inside: Vec<f64> = layout.iter().copied().filter(|&x| x >= x0 + 0.5 * b && x <= x1 - 0.5 * b).collect();
            if o.kind == OpeningKind::Window {
                let zs = o.sill();
                let sn0 = m.node_on(post0, w(xp0, zs)).unwrap();
                let sn1 = m.node_on(post1, w(xp1, zs)).unwrap();
                let sax = m.axes_between(w(xp0, zs), w(xp1, zs), Some(inward));
                let sill = m.add_member(
                    &[sn0, sn1],
                    &MemberSpec::new("sill", stud_sec, mat)
                        .priority(5)
                        .depth_dir(inward)
                        .anchor(Anchor::body_toward(&sax, Some(-Vec3::Z), v_side))
                        .group(g),
                );
                m.connect(sn0, sill, Some(post0), c_sill);
                m.connect(sn1, sill, Some(post1), c_sill);
                for &x in &inside {
                    let id = vertical(m, x, 0.0, zs, bottom_at(x), sill, "cripple", c_bot, c_crip);
                    parts.studs.push(id);
                }
            }
            let clear = h - 2.0 * b - (o.head + m.section(hdr_sec).props.depth);
            if clear >= b {
                for &x in &inside {
                    let id = vertical(m, x, o.head, ht, header, parts.top_plate, "cripple", c_crip, c_top);
                    parts.studs.push(id);
                }
            } else if clear.abs() < 1e-6 {
                // Header tight to the top plate: face-to-face bearing contact.
                m.bond(parts.top_plate, header, c_plate_hdr);
            }
        }

        // Common studs: end studs plus layout positions clear of openings.
        let first = self.start_inset + 0.5 * b;
        let last = len - self.end_inset - 0.5 * b;
        let mut xs = vec![first];
        xs.extend(layout.iter().copied().filter(|&x| x - first >= b && last - x >= b));
        xs.push(last);
        for x in xs {
            if blocked.iter().any(|&(a, bx)| x > a && x < bx) {
                continue;
            }
            let id = vertical(m, x, 0.0, ht, bottom_at(x), parts.top_plate, "stud", c_bot, c_top);
            parts.studs.push(id);
        }

        self.annotate(m, g, len, h, &openings);
        parts
    }

    fn annotate(&self, m: &mut Model, g: GroupId, len: f64, h: f64, openings: &[Opening]) {
        let fmt = |v: f64| m.units.fmt_len(v);
        let mut anns = vec![];
        let mut stations = vec![0.0, len];
        let mut vchains: Vec<Vec<f64>> = vec![];
        for o in openings {
            let (x0, x1) = (o.center - o.width / 2.0, o.center + o.width / 2.0);
            stations.extend([x0, x1]);
            let chain = match o.kind {
                OpeningKind::Window => vec![0.0, o.sill(), o.head],
                OpeningKind::Door => vec![0.0, o.head],
            };
            if !vchains.contains(&chain) {
                vchains.push(chain);
            }
            let (zs, zh) = (if o.kind == OpeningKind::Window { o.sill() } else { 0.0 }, o.head);
            anns.push(Annotation::Region {
                min: v3(x0, 0.0, zs),
                max: v3(x1, 0.0, zh),
                label: format!("{}  RO {} x {}", o.label, fmt(o.width), fmt(o.height)),
            });
        }
        stations.sort_by(f64::total_cmp);
        stations.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        // A chain with no intermediate stations would just repeat the overall dimension.
        for p in stations.windows(2).filter(|_| stations.len() > 2) {
            anns.push(Annotation::Dim { a: v3(p[0], 0.0, 0.0), b: v3(p[1], 0.0, 0.0), side: -Vec3::Z, tier: 0, text: None });
        }
        anns.push(Annotation::Dim { a: v3(0.0, 0.0, 0.0), b: v3(len, 0.0, 0.0), side: -Vec3::Z, tier: 1, text: None });
        let ntiers = vchains.len() as u32;
        for (k, chain) in vchains.iter().enumerate() {
            for p in chain.windows(2) {
                anns.push(Annotation::Dim { a: v3(len, 0.0, p[0]), b: v3(len, 0.0, p[1]), side: Vec3::X, tier: k as u32, text: None });
            }
        }
        anns.push(Annotation::Dim { a: v3(len, 0.0, 0.0), b: v3(len, 0.0, h), side: Vec3::X, tier: ntiers, text: None });
        let mat_name = m.material(self.material).name.clone();
        let group = m.group_mut(g);
        group.annotations.extend(anns);
        group.props.insert(
            "framing".into(),
            format!("{}x{} {} STUDS @ {} O.C.", self.stud.0, self.stud.1, mat_name, topo_core::units::fmt_inches(self.spacing)),
        );
        group.props.insert("height".into(), topo_core::units::fmt_ft_in(h));
    }
}
