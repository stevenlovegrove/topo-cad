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
        Perimeter { name: name.into(), z: 0.0, points: vec![at], sides: vec![], closed: false }
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
        let ns = self.sides.len();
        assert_eq!(ns, if self.closed { self.points.len() } else { self.points.len() - 1 }, "one side per segment");
        // Orientation decides which side is the interior.
        let signed = if self.closed { polygon_area(&self.points) } else { 1.0 };
        let interior_left = signed > 0.0;
        let g = m.add_group(&self.name, "perimeter", Frame::WORLD, None);

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

        let mut walls = vec![];
        for (i, side) in self.sides.iter().enumerate() {
            let Side::Wall(template) = side else {
                walls.push(None);
                continue;
            };
            let (p0, p1) = self.seg(i);
            let mut w = template.clone();
            if w.name.is_empty() {
                w.name = format!("{} wall {}", self.name, i + 1);
            }
            w.start = v3(p0.x, p0.y, self.z);
            w.end = v3(p1.x, p1.y, self.z);
            w.interior_left = interior_left;
            w.start_inset = start_inset[i];
            w.end_inset = end_inset[i];
            w.parent = Some(g);
            walls.push(Some(w.build(m)));
        }
        // Interleave the top plate plies at every corner: the wall built first
        // runs its lower plate through, so the other wall's cap runs through.
        let c = m.add_connection(fx::plate_corner());
        for (a, b) in wall_corners {
            let (first, second) = if a < b { (a, b) } else { (b, a) };
            let (Some(pf), Some(ps)) = (&walls[first], &walls[second]) else { continue };
            assert!(lap_corner(m, pf, ps, c), "cap plates of sides {a} and {b} do not meet");
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
        PerimeterParts { group: g, walls }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lumber::graded;
    use crate::wall::Opening;
    use topo_core::units::{ft, ft_in};
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
}
