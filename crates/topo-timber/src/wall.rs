//! Stud wall generator: plates, studs, openings (kings, jacks, headers, sills,
//! cripples) as topology. All positions are in the wall's local frame:
//! x along the wall line, y toward the interior, z up. The wall line lies on the
//! exterior face of framing (or the centre line for `Justify::Center`) at the
//! bottom of the bottom plate; `height` is to the top of the double top plate.

use crate::fastening as fx;
use crate::lumber::{built_up, dressed_in, sawn};
use crate::survey::{Post, PostRole, ResolvedItem, Survey, SurveyItem, WallContext};
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
    /// As built: layout studs' centres along the wall from its start. The first sets the wall's layout out (the
    /// others follow at `spacing` either way); each further one sets out the stretch between openings it is in.
    /// Empty: the layout runs from the start (studs at spacing, 2·spacing, …).
    pub layout_from: Vec<f64>,
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
    /// As-built layout from field measurements, instead of the layout rules.
    pub survey: Option<Survey>,
    /// Inside faces of the walls met at the start and end, as x along this
    /// wall; set by the perimeter (datums for surveys).
    pub corner_inside: [Option<f64>; 2],
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
            layout_from: vec![],
            interior_left: true,
            justify: Justify::Exterior,
            openings: vec![],
            start_inset: 0.0,
            end_inset: 0.0,
            cap_breaks: vec![],
            parent: None,
            survey: None,
            corner_inside: [None, None],
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
    /// Sets the stud layout out from studs whose centres are at `xs` along the wall (as built), not from the
    /// wall's start: the first for the wall, each further one for the stretch between openings it is in.
    pub fn layout_from(mut self, xs: &[f64]) -> Wall {
        self.layout_from = xs.to_vec();
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
    /// Frame the wall exactly as surveyed (see `survey`) instead of by rule.
    pub fn surveyed(mut self, s: Survey) -> Wall {
        self.survey = Some(s);
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
        self.try_build(m).unwrap_or_else(|e| panic!("{e}"))
    }

    /// Builds the wall: from its survey if it has one, else by layout rules.
    pub fn try_build(&self, m: &mut Model) -> Result<WallParts, String> {
        match &self.survey {
            Some(s) => self.build_surveyed(m, s),
            None => Ok(self.build_generated(m)),
        }
    }

    fn build_generated(&self, m: &mut Model) -> WallParts {
        let len = self.start.distance(self.end);
        let doors: Vec<(f64, f64)> = self
            .openings
            .iter()
            .filter(|o| o.kind == OpeningKind::Door)
            .map(|o| (o.center - o.width / 2.0, o.center + o.width / 2.0))
            .collect();
        let mut f = Framing::new(self, m, &doors);
        let (h, b, ht) = (f.h, f.b, f.ht);

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
        let mut blocked: Vec<(f64, f64)> = vec![];
        let grid = |from: f64, lo: f64, hi: f64| -> Vec<f64> {
            let from = from.rem_euclid(self.spacing);
            (0..).map(|k| from + k as f64 * self.spacing).take_while(|&x| x < hi).filter(|&x| x > lo + 1e-9).collect()
        };
        let mut layout = grid(self.layout_from.first().copied().unwrap_or(0.0), 0.0, len);
        for &a in self.layout_from.iter().skip(1) {
            // The stretch between openings (or the wall's ends) this stud is in.
            let lo = openings.iter().map(|o| o.center + o.width / 2.0).filter(|&x| x <= a).fold(0.0, f64::max);
            let hi = openings.iter().map(|o| o.center - o.width / 2.0).filter(|&x| x >= a).fold(len, f64::min);
            layout.retain(|&x| x <= lo || x >= hi);
            layout.extend(grid(a, lo, hi));
        }
        layout.sort_by(|a, b| a.total_cmp(b));
        for o in &openings {
            let (x0, x1) = (o.center - o.width / 2.0, o.center + o.width / 2.0);
            let nj = o.jacks as f64;
            // Posts each side, innermost first: jacks then the king.
            let (xk0, xk1) = (x0 - (nj + 0.5) * b, x1 + (nj + 0.5) * b);
            blocked.push((xk0 - b, xk1 + b));
            let king0 = f.full(m, xk0, "king_stud", None).unwrap();
            let king1 = f.full(m, xk1, "king_stud", None).unwrap();
            let header = f.header(m, o, xk0, xk1, king0, king1);

            // Jacks, bonded to each other and to the king.
            let (mut post0, mut post1) = (king0, king1);
            for k in (0..o.jacks).rev() {
                let (xj0, xj1) = (x0 - (k as f64 + 0.5) * b, x1 + (k as f64 + 0.5) * b);
                let jack0 = f.jack(m, xj0, o.head, header, post0, None).unwrap();
                let jack1 = f.jack(m, xj1, o.head, header, post1, None).unwrap();
                (post0, post1) = (jack0, jack1);
            }
            let (xp0, xp1) = (x0 - 0.5 * b, x1 + 0.5 * b);
            let inside: Vec<f64> = layout.iter().copied().filter(|&x| x >= x0 + 0.5 * b && x <= x1 - 0.5 * b).collect();
            let sill = (o.kind == OpeningKind::Window).then(|| f.sill(m, o.sill(), post0, post1, xp0, xp1));
            for &x in &inside {
                f.cripples(m, x, o, header, sill, None).unwrap();
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
            f.full(m, x, "stud", None).unwrap();
        }
        let _ = ht;
        let (g, parts) = (f.g, f.parts);
        self.annotate(m, g, len, h, &openings);
        parts
    }

    /// Builds the wall exactly as surveyed: each post where it was measured,
    /// openings between the posts either side of them.
    fn build_surveyed(&self, m: &mut Model, survey: &Survey) -> Result<WallParts, String> {
        let len = self.start.distance(self.end);
        let b = inch(dressed_in(self.stud.0));
        let d = (self.end - self.start) / len;
        let cx = WallContext { len, ply: b, inside: self.corner_inside, dir: [d.x, d.y] };
        let r = survey.resolve(&self.name, &cx)?;
        let (h, t) = (self.height, self.thickness());
        let _ = t;
        let items = &r.items;
        let post_at = |k: usize| match &items[k] {
            ResolvedItem::Post(p) => Some(p),
            _ => None,
        };
        // Positions in messages: from the outside corner at the wall start,
        // named by compass where the wall runs square to one.
        let start_name = {
            let back = -(self.end - self.start) / len;
            [("east", back.x), ("west", -back.x), ("north", back.y), ("south", -back.y)]
                .into_iter()
                .find(|(_, c)| *c > 0.9)
                .map(|(n, _)| format!("the {n} outside corner"))
                .unwrap_or_else(|| "the start (outside corner)".into())
        };
        let describe = |p: &Post| {
            let at = format!("{} from {start_name}", topo_core::units::fmt_inches(p.x));
            match &p.name {
                Some(n) => format!("{n} ({at})"),
                None => format!("{} at {at}", p.role.role().replace('_', " ")),
            }
        };

        // Openings: rough-opening edges from the nearest posts each side
        // (past any cripples listed beside the opening), the kings outermost.
        struct Plan {
            op: Opening,
            kings: [usize; 2],
            jacks: [Vec<usize>; 2],
            cripples: Vec<usize>,
            x: (f64, f64),
        }
        let mut plans: Vec<Plan> = vec![];
        for (k, it) in items.iter().enumerate() {
            let ResolvedItem::Opening(i) = it else { continue };
            let SurveyItem::Opening { label, kind, head, sill, header, header_flush } = &survey.items[*i] else { unreachable!() };
            let mut cripples = vec![];
            let mut side = |dir: isize| -> Result<(Vec<usize>, usize), String> {
                let mut j = k as isize + dir;
                while j >= 0 && (j as usize) < items.len() && post_at(j as usize).is_some_and(|p| p.role == PostRole::Cripple) {
                    cripples.push(j as usize);
                    j += dir;
                }
                // Posts nearest the opening first: jacks, then the king.
                let mut group = vec![];
                while j >= 0 && (j as usize) < items.len() {
                    match post_at(j as usize) {
                        Some(p) if p.role == PostRole::Jack => group.push(j as usize),
                        Some(p) if p.role == PostRole::King => {
                            return Ok((group, j as usize));
                        }
                        _ => break,
                    }
                    j += dir;
                }
                Err(format!(
                    "{}: opening {label} has no king stud on its {} side (list jacks and then a king between it and any other stud)",
                    self.name,
                    if dir < 0 { "start" } else { "end" }
                ))
            };
            let (mut jacks0, mut king0) = side(-1)?;
            let (mut jacks1, mut king1) = side(1)?;
            // Listed the other way along the wall (e.g. from the end corner).
            if post_at(king0).unwrap().x > post_at(king1).unwrap().x {
                std::mem::swap(&mut jacks0, &mut jacks1);
                std::mem::swap(&mut king0, &mut king1);
            }
            let inner = |group: &[usize], king: usize| *group.first().unwrap_or(&king);
            let (p0, p1) = (post_at(inner(&jacks0, king0)).unwrap(), post_at(inner(&jacks1, king1)).unwrap());
            let x = (p0.x + p0.width(b) / 2.0, p1.x - p1.width(b) / 2.0);
            if x.1 <= x.0 {
                return Err(format!("{}: opening {label} has no width between {} and {}", self.name, describe(p0), describe(p1)));
            }
            let kind = match kind.as_str() {
                "door" => OpeningKind::Door,
                "window" => OpeningKind::Window,
                k => return Err(format!("{}: opening {label}: unknown kind {k}", self.name)),
            };
            let head_z = if *header_flush {
                h - 2.0 * b - inch(dressed_in(header.2))
            } else {
                head.as_ref().ok_or_else(|| format!("{}: opening {label} needs a head height (or a flush header)", self.name))?.z(h, b)
            };
            let height = match kind {
                OpeningKind::Door => head_z,
                OpeningKind::Window => head_z - sill.as_ref().ok_or_else(|| format!("{}: window {label} needs a sill height", self.name))?.z(h, b),
            };
            if height <= 0.0 {
                return Err(format!("{}: opening {label}: sill is not below the head", self.name));
            }
            let op = Opening { label: label.clone(), kind, center: (x.0 + x.1) / 2.0, width: x.1 - x.0, height, head: head_z, header: *header, jacks: jacks0.len().max(jacks1.len()) as u32, header_flush: *header_flush };
            plans.push(Plan { op, kings: [king0, king1], jacks: [jacks0, jacks1], cripples, x });
        }

        // Checks on the layout: overlaps, missing studs, corners.
        let mut posts: Vec<&Post> = items.iter().filter_map(|i| if let ResolvedItem::Post(p) = i { Some(p) } else { None }).collect();
        posts.sort_by(|a, b| a.x.total_cmp(&b.x));
        let mut issues = vec![];
        let tight = inch(0.125);
        for w in posts.windows(2) {
            let gap = (w[1].x - w[1].width(b) / 2.0) - (w[0].x + w[0].width(b) / 2.0);
            if gap < -tight {
                return Err(format!("{}: {} and {} overlap by {} — check the readings", self.name, describe(w[0]), describe(w[1]), topo_core::units::fmt_inches(-gap)));
            } else if gap < -1e-6 {
                issues.push(Issue::new(Severity::Warning, "survey-overlap", format!("{}: {} and {} overlap by {} (measurement slop?)", self.name, describe(w[0]), describe(w[1]), topo_core::units::fmt_inches(-gap))));
            }
        }
        // Full-height studs (and kings) more than a layout space apart,
        // outside openings: a stud may be missing from the survey.
        let in_opening = |x: f64| plans.iter().any(|p| x > p.x.0 - 1e-6 && x < p.x.1 + 1e-6);
        let mut bounds: Vec<(f64, String)> = vec![];
        if let Some(x) = self.corner_inside[0] {
            bounds.push((x, format!("the wall at {}", start_name.trim_start_matches("the ").replace("outside ", ""))));
        }
        for p in &posts {
            if matches!(p.role, PostRole::Stud | PostRole::King | PostRole::Jack) {
                bounds.push((p.x, describe(p)));
            }
        }
        if let Some(x) = self.corner_inside[1] {
            bounds.push((x, "the wall at the far corner".into()));
        }
        let limit = self.spacing + inch(0.5);
        for w in bounds.windows(2) {
            if w[1].0 - w[0].0 > limit && !in_opening((w[0].0 + w[1].0) / 2.0) {
                issues.push(Issue::new(
                    Severity::Warning,
                    "survey-gap",
                    format!("{}: {} between {} and {} with no stud (layout {} o.c.) — one missing from the survey?", self.name, topo_core::units::fmt_inches(w[1].0 - w[0].0), w[0].1, w[1].1, topo_core::units::fmt_inches(self.spacing)),
                ));
            }
        }
        for (what, measured, model, tol) in &r.checks {
            let d = measured - model;
            let sev = if d.abs() > *tol { Severity::Warning } else { Severity::Info };
            issues.push(Issue::new(
                sev,
                "survey-check",
                format!("{}: {what}: measured {}, model {} ({}{})", self.name, topo_core::units::fmt_inches(*measured), topo_core::units::fmt_inches(*model), if d >= 0.0 { "+" } else { "−" }, topo_core::units::fmt_inches(d.abs())),
            ));
        }

        // Build.
        let doors: Vec<(f64, f64)> = plans.iter().filter(|p| p.op.kind == OpeningKind::Door).map(|p| p.x).collect();
        let mut f = Framing::new(self, m, &doors);
        let mut ids: std::collections::HashMap<usize, Vec<MemberId>> = Default::default();
        let plies = |p: &Post| -> Vec<f64> { (0..p.plies).map(|k| p.x - p.width(b) / 2.0 + (k as f64 + 0.5) * b).collect() };
        let name_of = |p: &Post, k: usize| p.name.as_ref().map(|n| if p.plies > 1 { format!("{n}#{}", k + 1) } else { n.clone() });
        // Full-height posts first (headers hang on the kings).
        for (k, it) in items.iter().enumerate() {
            let ResolvedItem::Post(p) = it else { continue };
            if matches!(p.role, PostRole::Stud | PostRole::King) {
                let mut v = vec![];
                for (j, x) in plies(p).into_iter().enumerate() {
                    let id = f.full(m, x, p.role.role(), name_of(p, j).as_deref()).map_err(|e| format!("{}: {}: {e}", self.name, describe(p)))?;
                    if let Some(&prev) = v.last() {
                        m.bond(prev, id, f.c_jack);
                    }
                    v.push(id);
                }
                ids.insert(k, v);
            }
        }
        let mut openings = vec![];
        for pl in &plans {
            let o = &pl.op;
            let (k0, k1) = (post_at(pl.kings[0]).unwrap(), post_at(pl.kings[1]).unwrap());
            // The header runs between the kings' faces toward the opening.
            let (king0, king1) = (*ids[&pl.kings[0]].last().unwrap(), ids[&pl.kings[1]][0]);
            let xk0 = k0.x + k0.width(b) / 2.0 - 0.5 * b;
            let xk1 = k1.x - k1.width(b) / 2.0 + 0.5 * b;
            let header = f.header(m, o, xk0, xk1, king0, king1);
            let mut post = [king0, king1];
            let mut xp = [xk0, xk1];
            for side in 0..2 {
                // From the king inward.
                for &j in pl.jacks[side].iter().rev() {
                    let p = post_at(j).unwrap();
                    let mut xs = plies(p);
                    if side == 1 {
                        xs.reverse();
                    }
                    let mut v = vec![];
                    for (n, x) in xs.into_iter().enumerate() {
                        let id = f.jack(m, x, o.head, header, post[side], name_of(p, n).as_deref()).map_err(|e| format!("{}: {}: {e}", self.name, describe(p)))?;
                        post[side] = id;
                        xp[side] = x;
                        v.push(id);
                    }
                    ids.insert(j, v);
                }
            }
            let sill = (o.kind == OpeningKind::Window).then(|| f.sill(m, o.sill(), post[0], post[1], xp[0], xp[1]));
            for &c in &pl.cripples {
                let p = post_at(c).unwrap();
                if p.x - p.width(b) / 2.0 < pl.x.0 - 1e-6 || p.x + p.width(b) / 2.0 > pl.x.1 + 1e-6 {
                    return Err(format!("{}: {} is listed with opening {} but lies outside it", self.name, describe(p), o.label));
                }
                let mut built = 0;
                for (n, x) in plies(p).into_iter().enumerate() {
                    built += f.cripples(m, x, o, header, sill, name_of(p, n).as_deref()).map_err(|e| format!("{}: {}: {e}", self.name, describe(p)))?;
                }
                if built == 0 {
                    issues.push(Issue::new(
                        Severity::Warning,
                        "survey-cripple",
                        format!("{}: {} was not built: no room above the {} header and no sill under it — check the head height and header size", self.name, describe(p), o.label),
                    ));
                }
            }
            openings.push(o.clone());
        }
        // Cripples or jacks not attached to an opening.
        for (k, it) in items.iter().enumerate() {
            if let ResolvedItem::Post(p) = it {
                if matches!(p.role, PostRole::Jack | PostRole::Cripple) && !ids.contains_key(&k) && !plans.iter().any(|pl| pl.cripples.contains(&k)) {
                    return Err(format!("{}: {} is not beside an opening (list it next to one)", self.name, describe(p)));
                }
            }
        }
        let (g, parts) = (f.g, f.parts);
        m.issues.extend(issues);
        self.annotate(m, g, len, h, &openings);
        let framing = format!("{}x{} {} STUDS, AS SURVEYED", self.stud.0, self.stud.1, m.material(self.material).name);
        m.group_mut(g).props.insert("framing".into(), framing);
        Ok(parts)
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

/// Shared construction for a wall: group, plates, and the members that hang
/// off them.
struct Framing {
    frame: Frame,
    g: GroupId,
    parts: WallParts,
    segs: Vec<(f64, f64, MemberId)>,
    stud_sec: SectionId,
    mat: MaterialId,
    hdr_mat: MaterialId,
    inward: Vec3,
    v_side: Option<Vec3>,
    h: f64,
    b: f64,
    t: f64,
    ht: f64,
    c_bot: ConnectionId,
    c_top: ConnectionId,
    c_hdr: ConnectionId,
    c_jack: ConnectionId,
    c_sill: ConnectionId,
    c_crip: ConnectionId,
    c_plate_hdr: ConnectionId,
}

impl Framing {
    /// Group, sections, connections and plates; `doors` interrupt the bottom plate.
    fn new(w: &Wall, m: &mut Model, doors: &[(f64, f64)]) -> Framing {
        let len = w.start.distance(w.end);
        let dir = (w.end - w.start) / len;
        let inward = if w.interior_left { Vec3::Z.cross(dir) } else { dir.cross(Vec3::Z) };
        let frame = Frame { origin: w.start, x: dir, y: inward, z: Vec3::Z };
        let (h, b, t) = (w.height, inch(dressed_in(w.stud.0)), w.thickness());
        let v_side = match w.justify {
            Justify::Exterior => Some(inward),
            Justify::Center => None,
        };
        let g = m.add_group(&w.name, "wall", frame, w.parent);
        m.name_direction(g, "along", Vec3::X);
        m.name_direction(g, "inside", Vec3::Y);
        m.name_direction(g, "outside", -Vec3::Y);
        let stud_sec = m.add_section(sawn(w.stud.0, w.stud.1));
        let mat = w.material;
        let hdr_mat = w.header_material.unwrap_or(mat);
        let mut f = Framing {
            frame,
            g,
            parts: WallParts { group: g, ..Default::default() },
            segs: vec![],
            stud_sec,
            mat,
            hdr_mat,
            inward,
            v_side,
            h,
            b,
            t,
            ht: h - b,
            c_bot: m.add_connection(fx::stud_to_bottom_plate()),
            c_top: m.add_connection(fx::stud_to_top_plate()),
            c_hdr: m.add_connection(fx::header_to_king()),
            c_jack: m.add_connection(fx::jack_to_king()),
            c_sill: m.add_connection(fx::sill_to_jack()),
            c_crip: m.add_connection(fx::cripple_to_plate()),
            c_plate_hdr: m.add_connection(fx::top_plate_to_header()),
        };
        let wp = |x: f64, z: f64| frame.to_world(v3(x, 0.0, z));

        // Plates. Door openings interrupt the bottom plate.
        let plate_ax = m.axes_between(w.start, w.end, Some(inward));
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
        let mut doors = doors.to_vec();
        doors.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut segs = vec![];
        let mut x = 0.0;
        for &(d0, d1) in &doors {
            segs.push((x, d0));
            x = d1;
        }
        segs.push((x, len));
        for (k, &(a, bx)) in segs.iter().enumerate() {
            let name = if segs.len() == 1 { "bottom_plate".to_string() } else { format!("bottom_plate#{}", k + 1) };
            let id = m.add_member_between(wp(a, 0.0), wp(bx, 0.0), &bottom.clone().named(&name));
            f.segs.push((a, bx, id));
            f.parts.bottom_plates.push(id);
        }
        // Double top plate as two members: lower ply (axis on its top face)
        // and cap plate(s) above, face-nailed together.
        f.parts.top_plate = m.add_member_between(wp(0.0, f.ht), wp(len, f.ht), &top.clone().named("top_plate"));
        let cap = MemberSpec { role: "cap_plate".into(), ..top.clone() };
        let c_dbl = m.add_connection(fx::double_top_plate());
        let mut breaks: Vec<f64> = w.cap_breaks.iter().copied().filter(|&x| x > 1e-6 && x < len - 1e-6).collect();
        breaks.sort_by(f64::total_cmp);
        let mut x0 = 0.0;
        let n_caps = breaks.len() + 1;
        for (k, x1) in breaks.into_iter().chain([len]).enumerate() {
            let name = if n_caps == 1 { "cap_plate".to_string() } else { format!("cap_plate#{}", k + 1) };
            let id = m.add_member_between(wp(x0, h), wp(x1, h), &cap.clone().named(&name));
            m.bond(id, f.parts.top_plate, c_dbl);
            f.parts.cap_plates.push(id);
            x0 = x1;
        }
        f
    }

    fn w(&self, x: f64, z: f64) -> Vec3 {
        self.frame.to_world(v3(x, 0.0, z))
    }

    fn bottom_at(&self, x: f64) -> Result<MemberId, String> {
        self.segs.iter().find(|(a, bx, _)| x >= *a && x <= *bx).map(|s| s.2).ok_or_else(|| "stands over a door opening (no bottom plate there)".to_string())
    }

    fn vspec(&self, role: &str, m: &Model, name: Option<&str>) -> MemberSpec {
        let stud_ax = m.axes_between(Vec3::ZERO, Vec3::Z, Some(self.inward));
        let s = MemberSpec::new(role, self.stud_sec, self.mat).depth_dir(self.inward).anchor(Anchor::body_toward(&stud_ax, None, self.v_side)).group(self.g);
        match name {
            Some(n) => s.named(n),
            None => s,
        }
    }

    /// Vertical member at `x` from a point on `lo` to a point on `hi`.
    #[allow(clippy::too_many_arguments)]
    fn vertical(&mut self, m: &mut Model, x: f64, z0: f64, z1: f64, lo: MemberId, hi: MemberId, role: &str, c0: ConnectionId, c1: ConnectionId, name: Option<&str>) -> Result<MemberId, String> {
        let n0 = m.node_on(lo, self.w(x, z0)).map_err(|e| format!("base not on its supporting member: {e:?}"))?;
        let n1 = m.node_on(hi, self.w(x, z1)).map_err(|e| format!("top not on its supporting member: {e:?}"))?;
        let spec = self.vspec(role, m, name);
        let id = m.add_member(&[n0, n1], &spec);
        m.connect(n0, id, Some(lo), c0);
        m.connect(n1, id, Some(hi), c1);
        self.parts.studs.push(id);
        Ok(id)
    }

    /// Full-height stud or king at `x`.
    fn full(&mut self, m: &mut Model, x: f64, role: &str, name: Option<&str>) -> Result<MemberId, String> {
        let lo = self.bottom_at(x)?;
        let (top, ht, c0, c1) = (self.parts.top_plate, self.ht, self.c_bot, self.c_top);
        self.vertical(m, x, 0.0, ht, lo, top, role, c0, c1, name)
    }

    /// Jack at `x` under `header`, bonded to the post beside it.
    fn jack(&mut self, m: &mut Model, x: f64, head: f64, header: MemberId, beside: MemberId, name: Option<&str>) -> Result<MemberId, String> {
        let lo = self.bottom_at(x)?;
        let (c0, c1, cj) = (self.c_bot, self.c_crip, self.c_jack);
        let id = self.vertical(m, x, 0.0, head, lo, header, "jack_stud", c0, c1, name)?;
        m.bond(id, beside, cj);
        Ok(id)
    }

    /// Header between king centres `xk0`, `xk1`, its underside at the opening head.
    fn header(&mut self, m: &mut Model, o: &Opening, xk0: f64, xk1: f64, king0: MemberId, king1: MemberId) -> MemberId {
        let b = self.b;
        let (plies, hdr_thick, hd) = o.header;
        let gap = if plies > 1 { ((self.t - plies as f64 * b) / (plies - 1) as f64).max(0.0) / inch(1.0) } else { 0.0 };
        let hdr_sec = m.add_section(built_up(plies, hdr_thick, hd, gap));
        let hn0 = m.node_on(king0, self.w(xk0, o.head)).unwrap();
        let hn1 = m.node_on(king1, self.w(xk1, o.head)).unwrap();
        let hax = m.axes_between(self.w(xk0, o.head), self.w(xk1, o.head), Some(Vec3::Z));
        let header = m.add_member(
            &[hn0, hn1],
            &MemberSpec::new("header", hdr_sec, self.hdr_mat)
                .named(&format!("header.{}", o.label))
                .priority(20)
                .depth_dir(Vec3::Z)
                .anchor(Anchor::body_toward(&hax, self.v_side, Some(Vec3::Z)))
                .group(self.g),
        );
        m.connect(hn0, header, Some(king0), self.c_hdr);
        m.connect(hn1, header, Some(king1), self.c_hdr);
        self.parts.headers.push(header);
        // Header tight to the top plate: face-to-face bearing contact.
        let clear = self.h - 2.0 * b - (o.head + m.section(hdr_sec).props.depth);
        if clear.abs() < 1e-6 {
            m.bond(self.parts.top_plate, header, self.c_plate_hdr);
        }
        header
    }

    /// Window sill between the innermost posts (centres `xp0`, `xp1`).
    fn sill(&mut self, m: &mut Model, zs: f64, post0: MemberId, post1: MemberId, xp0: f64, xp1: f64) -> MemberId {
        let sn0 = m.node_on(post0, self.w(xp0, zs)).unwrap();
        let sn1 = m.node_on(post1, self.w(xp1, zs)).unwrap();
        let sax = m.axes_between(self.w(xp0, zs), self.w(xp1, zs), Some(self.inward));
        let sill = m.add_member(
            &[sn0, sn1],
            &MemberSpec::new("sill", self.stud_sec, self.mat)
                .priority(5)
                .depth_dir(self.inward)
                .anchor(Anchor::body_toward(&sax, Some(-Vec3::Z), self.v_side))
                .group(self.g),
        );
        m.connect(sn0, sill, Some(post0), self.c_sill);
        m.connect(sn1, sill, Some(post1), self.c_sill);
        sill
    }

    /// Cripples at `x` in an opening: under the sill, and over the header if
    /// there is room for one.
    /// Returns how many were built.
    fn cripples(&mut self, m: &mut Model, x: f64, o: &Opening, header: MemberId, sill: Option<MemberId>, name: Option<&str>) -> Result<usize, String> {
        let mut n = 0;
        let (c_bot, c_crip, c_top, top, ht) = (self.c_bot, self.c_crip, self.c_top, self.parts.top_plate, self.ht);
        if let Some(s) = sill {
            let lo = self.bottom_at(x)?;
            self.vertical(m, x, 0.0, o.sill(), lo, s, "cripple", c_bot, c_crip, name)?;
            n += 1;
        }
        let depth = m.section(m.member(header).section).props.depth;
        if self.h - 2.0 * self.b - (o.head + depth) >= self.b {
            self.vertical(m, x, o.head, ht, header, top, "cripple", c_crip, c_top, name)?;
            n += 1;
        }
        Ok(n)
    }
}
