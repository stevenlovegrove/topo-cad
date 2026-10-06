//! A building perimeter described as a plan path; each segment is a wall
//! (built from a template `Wall` value) or an open side. Corner handling —
//! which wall runs through, layout insets, convex vs. reflex corners, and
//! arbitrary angles — is derived from the path.

use crate::fastening as fx;
use crate::wall::{lap_corner, Wall, WallParts};
use topo_core::*;

#[derive(Clone, Debug)]
pub enum Side {
    /// A wall built from this template (its start/end/insets are replaced).
    Wall(Wall),
    /// No wall on this segment (open side, or attached to another structure).
    Open,
}

#[derive(Clone, Debug)]
pub struct Perimeter {
    pub name: String,
    /// Elevation of the bottom of the walls.
    pub z: f64,
    pub points: Vec<Vec2>,
    /// `sides[i]` spans `points[i] → points[i + 1]` (wrapping when closed).
    pub sides: Vec<Side>,
    pub closed: bool,
    /// The bottom of the walls at each point, where it is not `z` (a stepped or sloping base).
    pub bases: Vec<Option<f64>>,
    /// Each side's wall's height at its start and its end, where they are not its template's
    /// (a top that slopes; and where two walls meet at different heights, a step).
    pub heights: Vec<Option<(f64, f64)>>,
    /// Each side's wall's bottom at its start and its end, where it is not the corners' (a wall
    /// that stands on something of its own: where it meets one at another height, its bottom
    /// plate does not meet the other's).
    pub wall_bases: Vec<Option<(f64, f64)>>,
}

#[derive(Clone, Debug, Default)]
pub struct PerimeterParts {
    pub group: GroupId,
    /// Per side; `None` for open sides.
    pub walls: Vec<Option<WallParts>>,
}

impl PerimeterParts {
    pub fn all_walls(&self) -> impl Iterator<Item = &WallParts> {
        self.walls.iter().flatten()
    }
    /// Uppermost plates (bearing surface for joists and trusses).
    pub fn cap_plates(&self) -> Vec<MemberId> {
        self.all_walls().flat_map(|w| w.cap_plates.clone()).collect()
    }
    pub fn bottom_plates(&self) -> Vec<MemberId> {
        self.all_walls().flat_map(|w| w.bottom_plates.clone()).collect()
    }
}

impl Perimeter {
    pub fn start(name: &str, at: Vec2) -> Perimeter {
        Perimeter { name: name.into(), z: 0.0, points: vec![at], sides: vec![], closed: false, bases: vec![], heights: vec![], wall_bases: vec![] }
    }
    /// Straight segment to an absolute plan point.
    pub fn to(mut self, p: Vec2, side: Side) -> Perimeter {
        self.points.push(p);
        self.sides.push(side);
        self
    }
    /// Straight segment by a relative offset.
    pub fn by(self, dx: f64, dy: f64, side: Side) -> Perimeter {
        let last = *self.points.last().unwrap();
        self.to(last + Vec2::new(dx, dy), side)
    }
    /// Closes the loop back to the start point with a final segment.
    pub fn close(mut self, side: Side) -> Perimeter {
        self.sides.push(side);
        self.closed = true;
        self
    }

    fn seg(&self, i: usize) -> (Vec2, Vec2) {
        let n = self.points.len();
        (self.points[i], self.points[(i + 1) % n])
    }

    pub fn build(&self, m: &mut Model) -> PerimeterParts {
        self.try_build(m).unwrap_or_else(|e| panic!("{e}"))
    }

    /// The inside framing face of side `i`'s wall, as a plan line (point, direction).
    fn inside_line(&self, i: usize, interior_left: bool) -> Option<(Vec2, Vec2)> {
        let Side::Wall(w) = &self.sides[i] else { return None };
        let (a, b) = self.seg(i);
        let d = (b - a).normalized();
        let n = if interior_left { Vec2::new(-d.y, d.x) } else { Vec2::new(d.y, -d.x) };
        let off = match w.justify {
            crate::wall::Justify::Exterior => w.thickness(),
            crate::wall::Justify::Center => w.thickness() / 2.0,
        };
        Some((a + n * off, d))
    }

    /// Where the inside faces of the walls at each end of side `i` cross its
    /// own inside face, as distances along it from its start point.
    fn corner_inside(&self, i: usize, interior_left: bool) -> [Option<f64>; 2] {
        let ns = self.sides.len();
        let Some((p, d)) = self.inside_line(i, interior_left) else { return [None, None] };
        let (a0, _) = self.seg(i);
        let meet = |j: Option<usize>| -> Option<f64> {
            let (q, e) = self.inside_line(j?, interior_left)?;
            let den = d.cross(e);
            if den.abs() < 1e-9 {
                return None;
            }
            let s = (q - p).cross(e) / den;
            // Measured from the outside corner, along the wall.
            Some((p + d * s - a0).dot(d))
        };
        let prev = if i > 0 { Some(i - 1) } else if self.closed { Some(ns - 1) } else { None };
        let next = if i + 1 < ns { Some(i + 1) } else if self.closed { Some(0) } else { None };
        [meet(prev), meet(next)]
    }

    /// Its walls as it would build them (placed, with their insets at corners), side by side;
    /// none for an open side.
    pub fn walls(&self) -> Vec<Option<Wall>> {
        let ns = self.sides.len();
        assert_eq!(ns, if self.closed { self.points.len() } else { self.points.len() - 1 }, "one side per segment");
        // Orientation decides which side is the interior.
        let signed = if self.closed { polygon_area(&self.points) } else { 1.0 };
        let interior_left = signed > 0.0;

        let mut start_inset = vec![0.0; ns];
        let mut wall_corners = vec![];
        let mut end_inset = vec![0.0; ns];
        // Corner k joins side a = k-1 (incoming) and side b = k (outgoing).
        let corners: Vec<(usize, usize)> = if self.closed {
            (0..ns).map(|k| ((k + ns - 1) % ns, k)).collect()
        } else {
            (1..ns).map(|k| (k - 1, k)).collect()
        };
        for (a, b) in corners {
            let (Side::Wall(wa), Side::Wall(wb)) = (&self.sides[a], &self.sides[b]) else { continue };
            wall_corners.push((a, b));
            let (a0, a1) = self.seg(a);
            let (b0, b1) = self.seg(b);
            let (da, db) = ((a1 - a0).normalized(), (b1 - b0).normalized());
            let turn = da.cross(db);
            let convex = if interior_left { turn > 1e-9 } else { turn < -1e-9 };
            if !convex {
                continue; // reflex or straight: the butting plate starts at the corner node
            }
            // Interior angle φ between the walls. A point on the butting wall at
            // distance s along it and depth w into the wall lies s·sinφ − w·cosφ
            // from the through wall's line; its first stud must clear the through
            // wall's thickness t for every w ∈ [0, t]:
            //   s ≥ t·(1 + max(0, cos φ)) / sin φ   (= t at 90°, larger when acute).
            let phi = (-da).dot(db).clamp(-1.0, 1.0).acos();
            // Lower member ids run through: that is `a` (so `b` butts at its
            // start), except at the closing corner where `b` was built first.
            let butts_start = a < b;
            let t = if butts_start { wa.thickness() } else { wb.thickness() };
            let inset = t * (1.0 + phi.cos().max(0.0)) / phi.sin();
            if butts_start {
                start_inset[b] = inset;
            } else {
                end_inset[a] = inset;
            }
        }

        let _ = &wall_corners;
        (self.sides.iter().enumerate())
            .map(|(i, side)| {
                let Side::Wall(template) = side else { return None };
                let (p0, p1) = self.seg(i);
                let mut w = template.clone();
                if w.name.is_empty() {
                    w.name = format!("{} wall {}", self.name, i + 1);
                }
                let n = self.points.len();
                let base = |k: usize| self.bases.get(k % n).copied().flatten().unwrap_or(self.z);
                let (b0, b1) = self.wall_bases.get(i).copied().flatten().unwrap_or((base(i), base(i + 1)));
                w.start = v3(p0.x, p0.y, b0);
                w.end = v3(p1.x, p1.y, b1);
                if let Some((h0, h1)) = self.heights.get(i).copied().flatten() {
                    w.height = h0;
                    w.height_end = ((h1 - h0).abs() > 1e-9).then_some(h1);
                }
                w.interior_left = interior_left;
                w.start_inset = start_inset[i];
                w.end_inset = end_inset[i];
                w.corner_inside = self.corner_inside(i, interior_left);
                Some(w)
            })
            .collect()
    }

    /// The corners where two walls meet (incoming side, outgoing side).
    fn wall_corners(&self) -> Vec<(usize, usize)> {
        let ns = self.sides.len();
        let corners: Vec<(usize, usize)> = if self.closed { (0..ns).map(|k| ((k + ns - 1) % ns, k)).collect() } else { (1..ns).map(|k| (k - 1, k)).collect() };
        corners.into_iter().filter(|&(a, b)| matches!((&self.sides[a], &self.sides[b]), (Side::Wall(_), Side::Wall(_)))).collect()
    }

    pub fn try_build(&self, m: &mut Model) -> Result<PerimeterParts, String> {
        self.try_build_walls(m, self.walls())
    }

    /// Builds it with these walls (as `walls` gives them, perhaps with their studs solved).
    pub fn try_build_walls(&self, m: &mut Model, planned: Vec<Option<Wall>>) -> Result<PerimeterParts, String> {
        let g = m.add_group(&self.name, "perimeter", Frame::WORLD, None);
        let wall_corners = self.wall_corners();
        let sloped: Vec<bool> = planned.iter().map(|w| w.as_ref().is_some_and(|w| w.height_end.is_some_and(|h| (h - w.height).abs() > 1e-9) || (w.start.z - w.end.z).abs() > 1e-9)).collect();
        let insets: Vec<(f64, f64)> = planned.iter().map(|w| w.as_ref().map_or((0.0, 0.0), |w| (w.start_inset, w.end_inset))).collect();
        let mut walls = vec![];
        for w in planned {
            let Some(mut w) = w else {
                walls.push(None);
                continue;
            };
            w.parent = Some(g);
            walls.push(Some(w.try_build(m)?));
        }
        // Interleave the top plate plies at every corner: the wall built first
        // runs its lower plate through, so the other wall's cap runs through.
        let c = m.add_connection(fx::plate_corner());
        for (a, b) in wall_corners {
            let (first, second) = if a < b { (a, b) } else { (b, a) };
            let (Some(pf), Some(ps)) = (&walls[first], &walls[second]) else { continue };
            // A wall whose top or bottom slopes: its plates meet the other's at an angle, and are
            // not interleaved (that is said).
            let corner = self.points[b % self.points.len()];
            // Two walls in one line (a wall that changes what it stands on, or its height, part way): their
            // plates meet end to end, each cut square; where they end at one node they are joined there.
            let ((a0, a1), (b0, b1)) = (self.seg(a), self.seg(b));
            if (a1 - a0).normalized().cross((b1 - b0).normalized()).abs() < 1e-9 {
                let plates = |w: &WallParts| w.plates();
                for p2 in plates(ps) {
                    for p1 in plates(pf) {
                        let e2 = [m.member(p2).start(), m.member(p2).end()];
                        if let Some(n) = [m.member(p1).start(), m.member(p1).end()].into_iter().find(|n| e2.contains(n)) {
                            m.connect(n, p2, Some(p1), c);
                        }
                    }
                }
                continue;
            }
            // (How far the second wall's framing keeps clear of the first at this corner: at its start, where
            // it leaves the corner; at its end, where it arrives.)
            let inset = if second == b { insets[second].0 } else { insets[second].1 };
            if sloped[a] || sloped[b] {
                butt_plates(m, pf, ps, corner, inset, c);
                m.issues.push(Issue::new(Severity::Info, "corner-sloped", format!("{}: a wall at the corner of sides {} and {} slopes: its plates butt the other's there, not lapped", self.name, a + 1, b + 1)));
                continue;
            }
            // Walls of different heights there: a step. Their plates do not lap; that is said.
            if !lap_corner(m, pf, ps, c) {
                butt_plates(m, pf, ps, corner, inset, c);
                m.issues.push(Issue::new(Severity::Info, "corner-step", format!("{}: the walls of sides {} and {} meet at different heights: their top plates do not lap there", self.name, a + 1, b + 1)));
                continue;
            }
            // Bottom and lower top plates meeting at the corner node.
            let lower = |w: &WallParts| {
                let mut v = w.bottom_plates.clone();
                v.push(w.top_plate);
                v
            };
            for p2 in lower(ps) {
                for p1 in lower(pf) {
                    let e2 = [m.member(p2).start(), m.member(p2).end()];
                    if let Some(n) = [m.member(p1).start(), m.member(p1).end()].into_iter().find(|n| e2.contains(n)) {
                        m.connect(n, p2, Some(p1), c);
                    }
                }
            }
        }
        Ok(PerimeterParts { group: g, walls })
    }
}

/// Where two walls' plates cannot lap at a corner (a top that slopes, or a step). The bottom
/// plates meet at the corner node as at any corner. Each top ply of `butting` that ends at
/// `corner` butts against a top ply of `through` ending at the same node; or, where none does
/// (the plies at different heights there), it is cut back to end on the first face of `through`
/// it runs into (a plate, or the end stud of a taller wall), at a node on that face.
fn butt_plates(m: &mut Model, through: &WallParts, butting: &WallParts, corner: Vec2, inset: f64, c: ConnectionId) {
    let near = |m: &Model, id: MemberId| {
        let mm = m.member(id);
        [mm.start(), mm.end()].into_iter().find(|&n| {
            let p = m.pos(n);
            Vec2::new(p.x, p.y).distance(corner) < 1e-6
        })
    };
    let ends = |m: &Model, id: MemberId| [m.member(id).start(), m.member(id).end()];
    for &p2 in &butting.bottom_plates {
        let mut met = false;
        for &p1 in &through.bottom_plates {
            if let Some(n) = ends(m, p1).into_iter().find(|n| ends(m, p2).contains(n)) {
                m.connect(n, p2, Some(p1), c);
                met = true;
            }
        }
        // (The walls' bottoms at different heights there: it ends on the other wall, as a top ply does.)
        if !met {
            if let Some(n) = near(m, p2) {
                // (Lower than all of the other wall's framing, it meets none of it: then it is the curb under
                // that wall that has the corner, and it ends on that, as far back as its own studs keep.)
                if !cut_back(m, through, p2, n, c) && through.curbed && inset > 1e-9 {
                    let far = ends(m, p2).into_iter().find(|&e| e != n).unwrap();
                    let e = m.node_at(m.pos(n) + (m.pos(far) - m.pos(n)).normalized() * inset);
                    let path = &mut m.member_mut(p2).path;
                    if path[0] == n {
                        path[0] = e;
                    } else {
                        *path.last_mut().unwrap() = e;
                    }
                }
            }
        }
    }
    let tops = |w: &WallParts| std::iter::once(w.top_plate).chain(w.cap_plates.iter().copied()).collect::<Vec<_>>();
    for p in tops(butting) {
        let Some(n) = near(m, p) else { continue };
        if let Some(t) = tops(through).into_iter().find(|&t| ends(m, t).contains(&n)) {
            // (Sloping differently, it is cut on the other's side toward it, not where a line along
            // it first meets the other: that may be the other's top.)
            let slope = |id: MemberId| m.member_axes(id).x.z;
            let rule = if (slope(p) - slope(t)).abs() > 1e-9 {
                let far = ends(m, p).into_iter().find(|&e| e != n).unwrap();
                let toward = m.pos(far) - m.pos(n);
                JointRule::ButtFace { member: p, against: t, face: m.face_toward(t, Vec3::new(toward.x, toward.y, 0.0)) }
            } else {
                JointRule::Butt { member: p, against: t }
            };
            m.add_rule(n, rule);
            m.connect(n, p, Some(t), c);
            continue;
        }
        cut_back(m, through, p, n, c);
    }
}

/// Cuts a plate of one wall back from the corner node `n` to end on the first face of the
/// `through` wall it runs into (a plate, or its end stud), at a node on that face.
fn cut_back(m: &mut Model, through: &WallParts, p: MemberId, n: NodeId, c: ConnectionId) -> bool {
    let ends = |m: &Model, id: MemberId| [m.member(id).start(), m.member(id).end()];
    // From its far end toward the corner: the first face of the other wall that any of it enters
    // (lines along it through its axis, its middle, and near each edge of its section: its axis
    // is on an edge of it, and may pass beside what the rest of it runs into).
    let far = ends(m, p).into_iter().find(|&e| e != n).unwrap();
    let (o, d) = (m.pos(far), (m.pos(n) - m.pos(far)).normalized());
    let sides = [Face::PlusU, Face::MinusU, Face::PlusV, Face::MinusV].map(|f| {
        let f = m.face(p, f);
        f.origin + f.along * (o - f.origin).dot(f.along) - o
    });
    let mid = (sides[0] + sides[1] + sides[2] + sides[3]) / 4.0;
    let (hu, hv) = ((sides[0] - sides[1]) / 2.0, (sides[2] - sides[3]) / 2.0);
    let mut lines = vec![Vec3::ZERO, mid];
    for (su, sv) in [(-0.9, -0.9), (-0.9, 0.9), (0.9, -0.9), (0.9, 0.9)] {
        lines.push(mid + hu * su + hv * sv);
    }
    // (Each face it meets: how far along, and whether its axis is one of the lines that meet it.)
    let mut hits: Vec<(f64, bool, MemberId, Face)> = vec![];
    for q in std::iter::once(through.top_plate).chain(through.cap_plates.iter().copied()).chain(through.bottom_plates.iter().copied()).chain(through.studs.iter().copied()) {
        for face in [Face::PlusU, Face::MinusU, Face::PlusV, Face::MinusV] {
            let f = m.face(q, face);
            // (A face it meets, not one it runs along.)
            let towards = d.dot(f.normal);
            if towards > -0.5 {
                continue;
            }
            let past = m.face_reach(q);
            for (k, &line) in lines.iter().enumerate() {
                let t = (f.origin - (o + line)).dot(f.normal) / towards;
                let x = o + line + d * t;
                let (along, across) = ((x - f.origin).dot(f.along), (x - f.origin).dot(f.across));
                if t > 0.0 && along >= -past && along <= f.length + past && across.abs() <= f.half_width + 1e-6 {
                    hits.push((t, k == 0, q, face));
                }
            }
        }
    }
    // The first of them; of those as near, one its axis meets (its end is a node on that face).
    let Some(first) = hits.iter().map(|h| h.0).min_by(f64::total_cmp) else { return false };
    hits.retain(|h| h.0 < first + 1e-6);
    hits.sort_by_key(|h| !h.1);
    let Some((e, q)) = hits.iter().find_map(|&(_, _, q, face)| {
        let f = m.face(q, face);
        let e = m.node_at(o + d * ((f.origin - o).dot(f.normal) / d.dot(f.normal)));
        (e != far && m.attach(e, q, face).is_ok()).then_some((e, q))
    }) else {
        return false;
    };
    let path = &mut m.member_mut(p).path;
    if path[0] == n {
        path[0] = e;
    } else {
        *path.last_mut().unwrap() = e;
    }
    m.add_rule(e, JointRule::Butt { member: p, against: q });
    m.connect(e, p, Some(q), c);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lumber::graded;
    use crate::wall::Opening;
    use topo_core::units::{ft, ft_in, inch};
    use topo_geom::Geometry;

    fn check(m: &Model) {
        let topo = Topology::build(m);
        let issues = validate(m, &topo);
        assert!(issues.iter().all(|i| i.severity < Severity::Warning), "{issues:#?}");
        let g = Geometry::build(m, &topo);
        assert!(g.issues.is_empty(), "{:#?}", g.issues);
        let clashes: Vec<String> = g
            .clashes()
            .iter()
            .map(|(a, b, d)| format!("{} {} / {} {} : {:.1} mm", a, m.member(*a).role, b, m.member(*b).role, d * 1000.0))
            .collect();
        assert!(clashes.is_empty(), "{clashes:#?}");
        let bearing: Vec<String> = g.bearing_issues(m).iter().map(|i| i.message.clone()).collect();
        assert!(bearing.is_empty(), "{bearing:#?}");
    }

    fn template(m: &mut Model) -> Wall {
        let dfl = m.add_material(graded("DFL", "No.2"));
        Wall::template(ft_in(8.0, 1.125), dfl)
    }

    #[test]
    fn walls_whose_tops_slope_and_a_step_where_two_meet_at_different_heights() {
        // A shed: the front low, the back high, the sides' tops sloping between them; built clean.
        let mut m = Model::new("shed");
        let w = template(&mut m);
        let (lo, hi) = (ft(8.0), ft(10.0));
        let mut p = Perimeter::start("Shed", Vec2::new(0.0, 0.0))
            .to(Vec2::new(ft(12.0), 0.0), Side::Wall(w.clone().named("Front")))
            .to(Vec2::new(ft(12.0), ft(10.0)), Side::Wall(w.clone().named("Right")))
            .to(Vec2::new(0.0, ft(10.0)), Side::Wall(w.clone().named("Back")))
            .close(Side::Wall(w.clone().named("Left")));
        p.heights = vec![Some((lo, lo)), Some((lo, hi)), Some((hi, hi)), Some((hi, lo))];
        let parts = p.build(&mut m);
        // (Where a sloping ply butts a level one at a corner, its end face rises past the ply it
        // is cut against: it meets a side face, and carries no weight through its end.)
        check(&m);
        let right = parts.walls[1].as_ref().unwrap();
        let cap = right.cap_plates[0];
        let (a, b) = (m.pos(m.member(cap).start()), m.pos(m.member(cap).end()));
        assert!((a.z - lo).abs() < 1e-9 && (b.z - hi).abs() < 1e-9, "{a:?} {b:?}");
        // Each stud of the right wall ends under the sloping double top plate where it stands (a ply's
        // thickness measured plumb).
        let b2 = inch(1.5) * (1.0 + ((hi - lo) / ft(10.0)).powi(2)).sqrt();
        for &s in right.studs.iter().filter(|&&s| m.member(s).role == "stud") {
            let (s0, s1) = (m.pos(m.member(s).start()), m.pos(m.member(s).end()));
            let expect = lo + (hi - lo) * s0.y / ft(10.0) - b2;
            assert!((s1.z - expect).abs() < 1e-9 && (s0.x - s1.x).abs() < 1e-12, "{s0:?} {s1:?} {expect}");
        }
        // Where a wall slopes, the plates at its corners butt rather than lap, and that is said.
        assert_eq!(m.issues.iter().filter(|i| i.code == "corner-sloped").count(), 4, "{:#?}", m.issues);

        // Now the back wall stands taller than the sides reach: steps at its corners, said, not refused.
        let mut m = Model::new("stepped");
        let w = template(&mut m);
        let mut p = Perimeter::start("Stepped", Vec2::new(0.0, 0.0))
            .to(Vec2::new(ft(12.0), 0.0), Side::Wall(w.clone()))
            .to(Vec2::new(ft(12.0), ft(10.0)), Side::Wall(w.clone()))
            .to(Vec2::new(0.0, ft(10.0)), Side::Wall(w.clone()))
            .close(Side::Wall(w.clone()));
        p.heights = vec![None, None, Some((hi, hi)), None];
        p.build(&mut m);
        // (The lower wall's plies end on the taller wall's end stud, on its side face.)
        check(&m);
        assert_eq!(m.issues.iter().filter(|i| i.code == "corner-step").count(), 2, "{:#?}", m.issues);
    }

    #[test]
    fn a_wall_that_stands_lower_than_the_others_and_curbs_under_those() {
        // Three walls on a 40 mm curb, the fourth on the slab the curb is on: it is as tall, so its
        // top is as much lower, and it meets the others at a step at both ends, top and bottom.
        let mut m = Model::new("on a curb");
        let w = template(&mut m);
        let curb = 0.04;
        let on_curb = Wall { curb: Some((Level::flat(-curb), Level::flat(0.0))), ..w.clone() };
        let mut p = Perimeter::start("Walls", Vec2::new(0.0, 0.0))
            .to(Vec2::new(ft(12.0), 0.0), Side::Wall(on_curb.clone().named("Front")))
            .to(Vec2::new(ft(12.0), ft(10.0)), Side::Wall(w.clone().named("Right")))
            .to(Vec2::new(0.0, ft(10.0)), Side::Wall(on_curb.clone().named("Back")))
            .close(Side::Wall(on_curb.clone().named("Left")));
        p.wall_bases = vec![None, Some((-curb, -curb)), None, None];
        let parts = p.build(&mut m);
        check(&m);
        let right = parts.walls[1].as_ref().unwrap();
        let z = |id: MemberId| (m.pos(m.member(id).start()).z, m.pos(m.member(id).end()).z);
        assert_eq!(z(right.bottom_plates[0]), (-curb, -curb));
        let h = ft_in(8.0, 1.125);
        let (a, b) = z(right.cap_plates[0]);
        assert!((a - (h - curb)).abs() < 1e-9 && (b - (h - curb)).abs() < 1e-9, "{a} {b}");
        let front = parts.walls[0].as_ref().unwrap();
        assert!((z(front.cap_plates[0]).0 - h).abs() < 1e-9);
        assert_eq!(m.issues.iter().filter(|i| i.code == "corner-step").count(), 2, "{:#?}", m.issues);
        // A curb under each wall that stands on one: its top the wall's bottom, as wide as its plates.
        let curbs: Vec<&Solid> = m.solids.iter().filter(|s| s.role == "curb").collect();
        assert_eq!(curbs.len(), 3);
        for c in &curbs {
            assert!(c.top == Level::flat(0.0) && c.bottom == Level::flat(-curb), "{c:?}");
        }
        let front_curb = curbs.iter().find(|c| m.solid_path(c) == "Walls/Front/curb").unwrap();
        let (lo, hi) = front_curb.outline.iter().fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p.y), hi.max(p.y)));
        assert!(lo.abs() < 1e-12 && (hi - inch(3.5)).abs() < 1e-9, "{lo} {hi}");
    }

    #[test]
    fn l_shaped_with_reflex_corner_and_chamfer() {
        let mut m = Model::new("L");
        let w = template(&mut m);
        // Counter-clockwise L with a 45° chamfer at one outside corner; the
        // corner at (12', 10') is reflex.
        let p = Perimeter::start("L", Vec2::new(0.0, 0.0))
            .to(Vec2::new(ft(24.0), 0.0), Side::Wall(w.clone().named("A").opening(Opening::door("GD", ft(12.0), ft(16.0), 0.0).header(2, 2, 10).header_flush())))
            .to(Vec2::new(ft(24.0), ft(8.0)), Side::Wall(w.clone()))
            .to(Vec2::new(ft(22.0), ft(10.0)), Side::Wall(w.clone()))
            .to(Vec2::new(ft(12.0), ft(10.0)), Side::Wall(w.clone()))
            .to(Vec2::new(ft(12.0), ft(20.0)), Side::Wall(w.clone()))
            .to(Vec2::new(0.0, ft(20.0)), Side::Wall(w.clone()))
            .close(Side::Wall(w.clone()));
        let parts = p.build(&mut m);
        assert_eq!(parts.walls.iter().flatten().count(), 7);
        check(&m);
        // Every corner node joins exactly the two bottom plates meeting there.
        let topo = Topology::build(&m);
        for &pt in &p.points {
            let n = m.clone().find_node(v3(pt.x, pt.y, 0.0)).unwrap();
            assert_eq!(topo.junction(n).kind, JunctionKind::Corner, "at {pt:?}");
        }
    }

    /// At every corner the lower plates and the cap plates run through on
    /// opposite walls, and the through cap reaches the outside corner.
    #[test]
    fn top_plates_interleave_at_corners() {
        use topo_geom::CutKind;
        let mut m = Model::new("lap");
        let w = template(&mut m);
        let (lx, ly) = (ft(20.0), ft(12.0));
        let p = Perimeter::start("R", Vec2::new(0.0, 0.0))
            .to(Vec2::new(lx, 0.0), Side::Wall(w.clone()))
            .to(Vec2::new(lx, ly), Side::Wall(w.clone()))
            .to(Vec2::new(0.0, ly), Side::Wall(w.clone()))
            .close(Side::Wall(w.clone()));
        let parts = p.build(&mut m);
        check(&m);
        let g = Geometry::build(&m, &Topology::build(&m));
        let runs = |id: MemberId, n: NodeId| {
            let cut = if m.member(id).start() == n { &g.member(id).start } else { &g.member(id).end };
            !matches!(cut.kind, CutKind::Butt { .. })
        };
        let walls: Vec<&crate::wall::WallParts> = parts.all_walls().collect();
        for (i, a) in walls.iter().enumerate() {
            let b = walls[(i + 1) % walls.len()];
            // Corner at the end of a / start of b.
            let (la, lb) = (a.top_plate, b.top_plate);
            let (ca, cb) = (*a.cap_plates.last().unwrap(), b.cap_plates[0]);
            let nl = m.member(la).end();
            let nc = m.member(ca).end();
            assert_eq!(nl, m.member(lb).start());
            assert_eq!(nc, m.member(cb).start());
            assert_ne!(runs(la, nl), runs(lb, nl), "one lower plate runs through");
            assert_ne!(runs(ca, nc), runs(cb, nc), "one cap runs through");
            assert_ne!(runs(la, nl), runs(ca, nc), "plies alternate");
        }
        // Each cap spans the full out-to-out length of its wall on one side of
        // a corner: total cap length = perimeter out-to-out minus butted laps.
        let cap_len: f64 = walls.iter().flat_map(|w| w.cap_plates.clone()).map(|c| g.member(c).cut_length()).sum();
        let low_len: f64 = walls.iter().map(|w| g.member(w.top_plate).cut_length()).sum();
        assert!((cap_len - low_len).abs() < 1e-6, "same total, different distribution");
    }

    #[test]
    fn studs_placed_one_by_one() {
        // Three measured studs, not at the wall's spacing: those exactly, none between, the spacing carried on beyond.
        let mut m = Model::new("studs-at");
        let w = template(&mut m).named("W").between(Vec3::ZERO, Vec3::new(3.0, 0.0, 0.0)).studs_at(&[1.0, 1.36, 1.72]);
        w.build(&mut m);
        let xs: Vec<f64> = m.members.iter().filter(|x| x.role == "stud").map(|x| m.node(x.start()).pos.x).collect();
        let has = |x: f64| xs.iter().any(|&s| (s - x).abs() < 1e-6);
        let s = inch(16.0);
        assert!(has(1.0) && has(1.36) && has(1.72), "the studs given: {xs:?}");
        assert!(has(1.0 - s) && has(1.0 - 2.0 * s) && has(1.72 + s) && has(1.72 + 2.0 * s), "the spacing beyond them: {xs:?}");
        assert_eq!(xs.iter().filter(|&&x| x > 1.0 + 1e-6 && x < 1.72 - 1e-6).count(), 1, "one between the outer two, the one given: {xs:?}");
    }

    #[test]
    fn stud_layout_set_out_as_built() {
        // A wall with two doors: the layout from a stud 0.1 m along, the pier between the doors from one at 3.0 m.
        let mut m = Model::new("set-out");
        let w = template(&mut m)
            .named("W")
            .between(Vec3::ZERO, Vec3::new(6.0, 0.0, 0.0))
            .opening(Opening::door("D1", 2.0, 1.0, 2.0))
            .opening(Opening::door("D2", 4.0, 1.0, 2.0))
            .layout_from(&[0.1, 3.0]);
        w.build(&mut m);
        let xs: Vec<f64> = m.members.iter().filter(|x| x.role == "stud").map(|x| m.node(x.start()).pos.x).collect();
        let has = |x: f64| xs.iter().any(|&s| (s - x).abs() < 1e-6);
        let s = inch(16.0);
        assert!(has(0.1) && has(0.1 + s) && has(0.1 + 2.0 * s), "the wall's layout: {xs:?}");
        assert!(has(3.0) && !has(0.1 + 7.0 * s), "the pier's own layout: {xs:?}");
        assert!(has(0.1 + 12.0 * s), "past the second door, the wall's layout again: {xs:?}");
    }

    #[test]
    fn acute_corner_clears_studs() {
        let mut m = Model::new("acute");
        let w = template(&mut m);
        // A wedge with a 40° corner at the origin (interior angle).
        let a = 40f64.to_radians();
        let far = ft(16.0);
        let p = Perimeter::start("V", Vec2::new(0.0, 0.0))
            .to(Vec2::new(far, 0.0), Side::Wall(w.clone()))
            .to(Vec2::new(far, far * a.tan()), Side::Wall(w.clone()))
            .close(Side::Wall(w.clone()));
        p.build(&mut m);
        check(&m);
    }

    #[test]
    fn open_side_for_attached_garage() {
        let mut m = Model::new("attached");
        let w = template(&mut m);
        // Three walls; the fourth side is the house wall (not modelled).
        let p = Perimeter::start("G", Vec2::new(0.0, 0.0))
            .by(ft(20.0), 0.0, Side::Wall(w.clone()))
            .by(0.0, ft(22.0), Side::Wall(w.clone()))
            .by(-ft(20.0), 0.0, Side::Open)
            .close(Side::Wall(w.clone()));
        let parts = p.build(&mut m);
        assert!(parts.walls[2].is_none());
        check(&m);
    }

    #[test]
    fn clockwise_path_puts_interior_on_the_right() {
        let mut m = Model::new("cw");
        let w = template(&mut m);
        let p = Perimeter::start("CW", Vec2::new(0.0, 0.0))
            .by(0.0, ft(10.0), Side::Wall(w.clone()))
            .by(ft(12.0), 0.0, Side::Wall(w.clone()))
            .by(0.0, -ft(10.0), Side::Wall(w.clone()))
            .close(Side::Wall(w.clone()));
        p.build(&mut m);
        check(&m);
        // All framing lies inside the footprint.
        let g = Geometry::build(&m, &Topology::build(&m));
        let bb = g.members.iter().fold(BBox3::EMPTY, |b, x| b.union(x.bbox()));
        assert!(bb.min.x > -1e-9 && bb.min.y > -1e-9 && bb.max.x < ft(12.0) + 1e-9 && bb.max.y < ft(10.0) + 1e-9);
    }

    /// A wall framed as surveyed: readings from the inside faces of the walls
    /// at each end (one with drywall), a window whose header is nailed to its
    /// kings (no jacks), a door with jacks and a cripple, a tie-out check, and
    /// a warning for a gap where a stud was not recorded.
    #[test]
    fn surveyed_wall() {
        use crate::survey::*;
        let mut m = Model::new("survey");
        let w = template(&mut m).studs(2, 6, inch(16.0));
        let (lx, ly) = (ft(20.0), ft(12.0));
        let start = |_: f64| DatumRef { datum: Datum::Corner { end: "start".into(), face: CornerFace::Inside }, offsets: vec![] };
        let drywall = || DatumRef {
            datum: Datum::Corner { end: "end".into(), face: CornerFace::Inside },
            offsets: vec![Offset { distance: inch(0.5), note: "1/2\" drywall".into() }],
        };
        let post = |role, from: DatumRef, d: f64| SurveyItem::Post { role, at: Reading { from, distance: inch(d), hit: Hit::Near }, plies: 1, name: None };
        let _ = start(0.0);
        let s = Survey {
            items: vec![
                post(PostRole::Stud, start(0.0), 0.0),
                post(PostRole::Stud, start(0.0), 15.25),
                post(PostRole::King, start(0.0), 30.0),
                SurveyItem::Opening {
                    label: "W1".into(),
                    kind: "window".into(),
                    head: Some(Height { level: Level::Slab, offsets: vec![], distance: inch(82.5) }),
                    sill: Some(Height { level: Level::Slab, offsets: vec![], distance: inch(46.5) }),
                    header: (2, 2, 8),
                    header_flush: false,
                },
                post(PostRole::King, start(0.0), 77.5),
                // 79" to 120": no stud recorded here (warning).
                post(PostRole::King, start(0.0), 120.0),
                post(PostRole::Jack, start(0.0), 121.5),
                SurveyItem::Opening { label: "D1".into(), kind: "door".into(), head: Some(Height { level: Level::Slab, offsets: vec![], distance: inch(82.5) }), sill: None, header: (2, 2, 10), header_flush: false },
                post(PostRole::Cripple, start(0.0), 140.0),
                post(PostRole::Jack, start(0.0), 160.5),
                post(PostRole::King, start(0.0), 162.0),
                // The rest measured from the far corner, which has drywall.
                post(PostRole::Stud, drywall(), 50.0),
                post(PostRole::Stud, drywall(), 34.0),
                post(PostRole::Stud, drywall(), 18.0),
                post(PostRole::Stud, drywall(), 2.0),
                post(PostRole::Stud, drywall(), -0.5), // tight to the framing, behind the drywall line
            ],
            checks: vec![SurveyCheck {
                from: start(0.0),
                to: DatumRef { datum: Datum::Corner { end: "end".into(), face: CornerFace::Inside }, offsets: vec![] },
                distance: lx - inch(11.0) + inch(0.0625),
                tolerance: inch(0.25),
            }],
        };
        let p = Perimeter::start("R", Vec2::new(0.0, 0.0))
            .to(Vec2::new(lx, 0.0), Side::Wall(w.clone().named("S").surveyed(s)))
            .to(Vec2::new(lx, ly), Side::Wall(w.clone()))
            .to(Vec2::new(0.0, ly), Side::Wall(w.clone()))
            .close(Side::Wall(w.clone()));
        let parts = p.try_build(&mut m).unwrap_or_else(|e| panic!("{e}"));
        // The survey's own findings are checked below; the framing must be sound.
        let issues = std::mem::take(&mut m.issues);
        check(&m);
        let wall = parts.walls[0].as_ref().unwrap();
        assert_eq!(wall.headers.len(), 2);
        assert_eq!(wall.bottom_plates.len(), 2, "the door interrupts the bottom plate");
        let studs: Vec<&Member> = wall.studs.iter().map(|&i| m.member(i)).collect();
        let count = |r: &str| studs.iter().filter(|x| x.role == r).count();
        assert_eq!((count("stud"), count("king_stud"), count("jack_stud"), count("cripple")), (7, 4, 2, 1));
        // The first stud is tight to the inside face of the west wall (5.5"),
        // the last to the east wall's framing: x = 240 − 5.5 − 0.75.
        let xs: Vec<f64> = studs.iter().filter(|x| x.role == "stud").map(|x| m.pos(x.start()).x / inch(1.0)).collect();
        assert!(xs.iter().any(|x| (x - 6.25).abs() < 1e-6), "{xs:?}");
        assert!(xs.iter().any(|x| (x - 233.75).abs() < 1e-6), "{xs:?}");
        // Drywall offset: 50" from the drywall face = 240 − 5.5 − 0.5 − 50 − 0.75.
        assert!(xs.iter().any(|x| (x - 183.25).abs() < 1e-6), "{xs:?}");
        // The window header hangs on its kings; the door's on its jacks.
        assert_eq!(issues.iter().filter(|i| i.code == "survey-gap").count(), 1, "{issues:#?}");
        assert!(issues.iter().any(|i| i.code == "survey-gap" && i.message.contains("between king stud at 83 3/4\" from the west outside corner and king stud at 126 1/4\" from the west outside corner")), "{issues:#?}");
        let chk = issues.iter().find(|i| i.code == "survey-check").expect("check reported");
        assert_eq!(chk.severity, Severity::Info, "{}", chk.message);
        assert!(chk.message.contains("+0.0625") || chk.message.contains("+1/16"), "{}", chk.message);
    }
}
