//! Frames, and constraints between things in them.
//!
//! - A **frame** is a coordinate system placed in a parent frame; its
//!   **joint** says which of its six freedoms are open. Geometry is made in a
//!   frame's own coordinates (a generator's vertices are plain numbers there)
//!   and is referred to from elsewhere only through the frames.
//! - A **scalar** is an unknown number (how far along a member a point is, a
//!   span): separate from the frames' poses.
//! - A **point** or **direction** is an expression over those, in a frame.
//! - A **constraint** relates points and directions, in whatever frames, and
//!   has a **tier**: hard (it must hold: a connection, a rigid shape),
//!   measured (it should hold, to a tolerance: a reading), or default (a
//!   generator's hint, deciding only what nothing above it decides).
//!
//! The tiers are solved in order, each within what the ones above leave free,
//! so a hint never bends a measurement and a measurement never bends a hard
//! constraint. The report says, for every freedom, what settled it, and names
//! what is still open. Hard constraints that cannot all hold are refused:
//! the report lists the ones not met, with their gaps, at the pose that
//! comes nearest to meeting them.

pub mod linalg;

use linalg::{lsq_null, mul, mul_vec, tidy_basis, zeros, Mat};

pub type V3 = [f64; 3];

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn scale(a: V3, k: f64) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn norm(a: V3) -> f64 {
    dot(a, a).sqrt()
}
fn unit(a: V3) -> V3 {
    scale(a, 1.0 / norm(a).max(1e-300))
}

/// Where a frame is in its parent: a point `p` of the frame is at `r·p + t` in the parent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    pub r: [V3; 3],
    pub t: V3,
}

impl Pose {
    pub fn identity() -> Pose {
        Pose { r: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], t: [0.0; 3] }
    }
    pub fn at(t: V3) -> Pose {
        Pose { t, ..Pose::identity() }
    }
    pub fn rotate(&self, v: V3) -> V3 {
        [dot(self.r[0], v), dot(self.r[1], v), dot(self.r[2], v)]
    }
    pub fn apply(&self, p: V3) -> V3 {
        add(self.rotate(p), self.t)
    }
    /// This, then a pose given in this frame: the child's pose in this frame's parent.
    pub fn then(&self, child: &Pose) -> Pose {
        let col = |k: usize| self.rotate([child.r[0][k], child.r[1][k], child.r[2][k]]);
        let (c0, c1, c2) = (col(0), col(1), col(2));
        Pose { r: [[c0[0], c1[0], c2[0]], [c0[1], c1[1], c2[1]], [c0[2], c1[2], c2[2]]], t: self.apply(child.t) }
    }
    /// Moved by a small motion given in the frame's own axes: a shift `v`, and a turn `w` (an axis times an angle).
    pub fn moved(&self, v: V3, w: V3) -> Pose {
        let angle = norm(w);
        let turn = if angle < 1e-14 {
            Pose::identity()
        } else {
            // Rodrigues' formula.
            let k = scale(w, 1.0 / angle);
            let (s, c) = angle.sin_cos();
            let row = |i: usize| -> V3 {
                let e: V3 = std::array::from_fn(|j| if i == j { 1.0 } else { 0.0 });
                let kx: V3 = [[0.0, -k[2], k[1]], [k[2], 0.0, -k[0]], [-k[1], k[0], 0.0]][i];
                std::array::from_fn(|j| c * e[j] + s * kx[j] + (1.0 - c) * k[i] * k[j])
            };
            Pose { r: [row(0), row(1), row(2)], t: [0.0; 3] }
        };
        self.then(&Pose { r: turn.r, t: v })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameId(pub usize);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScalarId(pub usize);

/// Which freedoms a frame has in its parent (axes in the frame's own coordinates).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Joint {
    /// None: it is where its pose says.
    Fixed,
    /// All six.
    Free,
    /// Sliding along an axis.
    Slider(V3),
    /// Turning about an axis through its origin.
    Hinge(V3),
    /// Moving in the plane with this normal, and turning about the normal.
    Planar(V3),
}

impl Joint {
    /// Each freedom as a unit motion (shift, turn), with what to call it.
    fn freedoms(&self) -> Vec<(V3, V3, String)> {
        let z = [0.0; 3];
        let axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        match *self {
            Joint::Fixed => vec![],
            Joint::Free => (0..3).map(|k| (axes[k], z, format!("move along its {}", ["x", "y", "z"][k]))).chain((0..3).map(|k| (z, axes[k], format!("turn about its {}", ["x", "y", "z"][k])))).collect(),
            Joint::Slider(a) => vec![(unit(a), z, "slide".into())],
            Joint::Hinge(a) => vec![(z, unit(a), "turn".into())],
            Joint::Planar(n) => {
                let n = unit(n);
                let u = unit(cross(n, if n[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] }));
                let v = cross(n, u);
                vec![(u, z, "move in its plane".into()), (v, z, "move in its plane (the other way)".into()), (z, n, "turn in its plane".into())]
            }
        }
    }
}

/// A point, in a frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Point {
    /// At these coordinates of the frame.
    At(FrameId, V3),
    /// On the line from `a` toward `b` of the frame, a distance `s` (an unknown, in the frame's unit) from `a`.
    Along(FrameId, V3, V3, ScalarId),
}

/// A direction, in a frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dir(pub FrameId, pub V3);

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    /// The two points are the same place.
    Coincident(Point, Point),
    /// The two points are this far apart.
    Distance(Point, Point, f64),
    /// From the first point to the second is this far along the direction (0: level with it, on its plane).
    Offset(Point, Point, Dir, f64),
    /// The directions agree (or oppose).
    Parallel(Dir, Dir),
    /// The directions are at right angles.
    Perpendicular(Dir, Dir),
    /// The scalar has this value.
    Value(ScalarId, f64),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tier {
    /// It must hold.
    Hard,
    /// It should hold, to about `sigma` (in the constraint's own unit: metres, or radians for directions).
    Measured { sigma: f64 },
    /// A hint: it decides only what nothing above it does.
    Default,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Constraint {
    pub name: String,
    pub tier: Tier,
    pub kind: Kind,
}

#[derive(Debug, Clone)]
struct FrameDef {
    name: String,
    parent: Option<FrameId>,
    joint: Joint,
    pose: Pose,
}

/// How far a constraint is from holding.
#[derive(Debug, Clone, PartialEq)]
pub struct Gap {
    pub constraint: String,
    pub tier: Tier,
    /// In the constraint's unit (metres; radians for directions).
    pub size: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// The hard constraints cannot all hold: the system is as near to them as it gets, and is not to be used.
    pub refused: bool,
    /// The hard constraints not met (when refused), the worst first.
    pub conflict: Vec<Gap>,
    /// Every constraint's gap.
    pub gaps: Vec<Gap>,
    pub passes: usize,
    /// The unknowns, and how many of them each tier settled.
    pub unknowns: usize,
    pub by_hard: usize,
    pub by_measured: usize,
    pub by_default: usize,
    /// What is still open: each a freedom nothing settles, in words.
    pub open: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct System {
    frames: Vec<FrameDef>,
    scalars: Vec<(String, f64)>,
    pub constraints: Vec<Constraint>,
}

/// The world: the frame everything is in.
pub const WORLD: FrameId = FrameId(0);

impl System {
    pub fn new() -> System {
        System { frames: vec![FrameDef { name: "world".into(), parent: None, joint: Joint::Fixed, pose: Pose::identity() }], scalars: vec![], constraints: vec![] }
    }

    /// A frame in `parent`, at `pose` there to start with (its nominal place), free as `joint` says.
    pub fn frame(&mut self, name: &str, parent: FrameId, joint: Joint, pose: Pose) -> FrameId {
        self.frames.push(FrameDef { name: name.into(), parent: Some(parent), joint, pose });
        FrameId(self.frames.len() - 1)
    }

    /// An unknown number, starting at `value`.
    pub fn scalar(&mut self, name: &str, value: f64) -> ScalarId {
        self.scalars.push((name.into(), value));
        ScalarId(self.scalars.len() - 1)
    }

    pub fn constrain(&mut self, name: &str, tier: Tier, kind: Kind) {
        self.constraints.push(Constraint { name: name.into(), tier, kind });
    }

    pub fn value(&self, s: ScalarId) -> f64 {
        self.scalars[s.0].1
    }

    /// A frame's pose in the world.
    pub fn in_world(&self, f: FrameId) -> Pose {
        let d = &self.frames[f.0];
        match d.parent {
            Some(p) => self.in_world(p).then(&d.pose),
            None => d.pose,
        }
    }

    /// A point, in the world.
    pub fn point(&self, p: &Point) -> V3 {
        match *p {
            Point::At(f, x) => self.in_world(f).apply(x),
            Point::Along(f, a, b, s) => self.in_world(f).apply(add(a, scale(unit(sub(b, a)), self.value(s)))),
        }
    }

    /// A direction, in the world (a unit vector).
    pub fn dir(&self, d: &Dir) -> V3 {
        unit(self.in_world(d.0).rotate(d.1))
    }

    /// How far a constraint is from holding, as numbers that are all zero when it does.
    fn residual(&self, k: &Kind) -> Vec<f64> {
        match k {
            Kind::Coincident(a, b) => sub(self.point(b), self.point(a)).to_vec(),
            Kind::Distance(a, b, d) => vec![norm(sub(self.point(b), self.point(a))) - d],
            Kind::Offset(a, b, dir, d) => vec![dot(sub(self.point(b), self.point(a)), self.dir(dir)) - d],
            Kind::Parallel(a, b) => cross(self.dir(a), self.dir(b)).to_vec(),
            Kind::Perpendicular(a, b) => vec![dot(self.dir(a), self.dir(b))],
            Kind::Value(s, v) => vec![self.value(*s) - v],
        }
    }

    /// The unknowns: each frame's open freedoms, then the scalars; with what to call each.
    fn unknowns(&self) -> Vec<(Unknown, String)> {
        let mut out = Vec::new();
        for (i, f) in self.frames.iter().enumerate() {
            for (v, w, what) in f.joint.freedoms() {
                out.push((Unknown::Motion(i, v, w), format!("{} can {what}", f.name)));
            }
        }
        for (i, (name, _)) in self.scalars.iter().enumerate() {
            out.push((Unknown::Scalar(i), format!("{name} can change")));
        }
        out
    }

    /// The system with its unknowns changed by `dx` (motions in each frame's own axes).
    fn stepped(&self, unknowns: &[(Unknown, String)], dx: &[f64]) -> System {
        let mut s = self.clone();
        // A frame's freedoms are applied together, as one small motion.
        let mut motion: Vec<(V3, V3)> = vec![([0.0; 3], [0.0; 3]); self.frames.len()];
        for ((u, _), &d) in unknowns.iter().zip(dx) {
            match *u {
                Unknown::Motion(f, v, w) => {
                    motion[f].0 = add(motion[f].0, scale(v, d));
                    motion[f].1 = add(motion[f].1, scale(w, d));
                }
                Unknown::Scalar(i) => s.scalars[i].1 += d,
            }
        }
        for (f, (v, w)) in motion.into_iter().enumerate() {
            if v != [0.0; 3] || w != [0.0; 3] {
                s.frames[f].pose = s.frames[f].pose.moved(v, w);
            }
        }
        s
    }

    /// The residuals of one tier's constraints (measured ones in units of their tolerance), stacked.
    fn tier_residual(&self, tier: usize) -> Vec<f64> {
        self.constraints
            .iter()
            .filter(|c| tier_of(c.tier) == tier)
            .flat_map(|c| {
                let w = match c.tier {
                    Tier::Measured { sigma } => 1.0 / sigma.max(1e-12),
                    _ => 1.0,
                };
                self.residual(&c.kind).into_iter().map(move |r| r * w)
            })
            .collect()
    }

    /// One tier's residual and how it changes with each unknown (by differences: the problems are small).
    fn linearised(&self, unknowns: &[(Unknown, String)], tier: usize) -> (Vec<f64>, Mat) {
        let r = self.tier_residual(tier);
        let n = unknowns.len();
        let mut j = zeros(r.len(), n);
        const H: f64 = 1e-6;
        for k in 0..n {
            let mut dx = vec![0.0; n];
            dx[k] = H;
            let hi = self.stepped(unknowns, &dx).tier_residual(tier);
            dx[k] = -H;
            let lo = self.stepped(unknowns, &dx).tier_residual(tier);
            for i in 0..r.len() {
                j[i][k] = (hi[i] - lo[i]) / (2.0 * H);
            }
        }
        (r, j)
    }

    /// The step the tiers ask for, tier by tier, each within what the ones before leave free:
    /// the step, how many freedoms each tier settled, and what is left free (a basis, as columns).
    fn tiered_step(&self, unknowns: &[(Unknown, String)]) -> (Vec<f64>, [usize; 3], Mat) {
        let n = unknowns.len();
        let (mut dx, mut free, mut ranks) = (vec![0.0; n], linalg::identity(n), [0; 3]);
        for tier in 0..3 {
            let (r, j) = self.linearised(unknowns, tier);
            let k = free.first().map_or(0, Vec::len);
            if r.is_empty() || k == 0 {
                continue;
            }
            // Within what is free: minimise |J (dx + free·z) + r|.
            let a = mul(&j, &free);
            let b: Vec<f64> = mul_vec(&j, &dx).iter().zip(&r).map(|(p, q)| p + q).collect();
            let (z, null, rank) = lsq_null(&a, &b, k);
            for (d, m) in dx.iter_mut().zip(mul_vec(&free, &z)) {
                *d += m;
            }
            free = if null.first().map_or(0, Vec::len) == 0 { zeros(n, 0) } else { mul(&free, &null) };
            ranks[tier] = rank;
        }
        (dx, ranks, free)
    }

    fn hard_gap(&self) -> f64 {
        self.tier_residual(0).iter().map(|x| x * x).sum::<f64>().sqrt()
    }

    /// What the lower tiers cost: the measured, then (far behind) the hints.
    fn soft_cost(&self) -> f64 {
        let sq = |t: usize| self.tier_residual(t).iter().map(|x| x * x).sum::<f64>();
        sq(1) + 1e-8 * sq(2)
    }

    /// Whether going from `self` to `next` is no worse: judged on the hard constraints
    /// while they are not met, and on the rest only once they are.
    fn no_worse(&self, next: &System) -> bool {
        const MET: f64 = 1e-9;
        let (before, after) = (self.hard_gap(), next.hard_gap());
        if before > MET {
            after < before
        } else {
            after <= MET && next.soft_cost() <= self.soft_cost() * (1.0 + 1e-9) + 1e-20
        }
    }

    /// Solves for the unknowns (the frames' poses and the scalars are left
    /// at the answer, or, when refused, as near the hard constraints as they
    /// get), and says what settled what.
    pub fn solve(&mut self) -> Report {
        let unknowns = self.unknowns();
        let n = unknowns.len();
        let mut passes = 0;
        for _ in 0..60 {
            passes += 1;
            let (dx, _, _) = self.tiered_step(&unknowns);
            let size = dx.iter().map(|x| x * x).sum::<f64>().sqrt();
            if size < 1e-11 {
                break;
            }
            // The whole step, or as much of it as does not make things worse.
            let mut taken = false;
            for halving in 0..12 {
                let f = 0.5f64.powi(halving);
                let next = self.stepped(&unknowns, &dx.iter().map(|x| x * f).collect::<Vec<_>>());
                if self.no_worse(&next) {
                    *self = next;
                    taken = true;
                    break;
                }
            }
            if !taken {
                break;
            }
        }
        let (_, ranks, free) = self.tiered_step(&unknowns);
        let gaps: Vec<Gap> = self.constraints.iter().map(|c| Gap { constraint: c.name.clone(), tier: c.tier, size: self.residual(&c.kind).iter().map(|x| x * x).sum::<f64>().sqrt() }).collect();
        // (A tenth of a micron, or of a microradian: beyond what the differences resolve.)
        let refused = self.hard_gap() > 1e-7;
        let mut conflict: Vec<Gap> = if refused { gaps.iter().filter(|g| g.tier == Tier::Hard && g.size > 1e-7).cloned().collect() } else { vec![] };
        conflict.sort_by(|a, b| b.size.total_cmp(&a.size));
        // What is still open, each as the unknowns that move together in it.
        let tidy = tidy_basis(&free);
        let open = (0..tidy.first().map_or(0, Vec::len))
            .map(|c| {
                let col: Vec<f64> = (0..n).map(|j| tidy[j][c]).collect();
                let top = col.iter().fold(0.0f64, |a, x| a.max(x.abs()));
                let parts: Vec<&str> = (0..n).filter(|&j| col[j].abs() > 0.3 * top).map(|j| unknowns[j].1.as_str()).collect();
                parts.join(", together with: ")
            })
            .collect();
        Report { refused, conflict, gaps, passes, unknowns: n, by_hard: ranks[0], by_measured: ranks[1], by_default: ranks[2], open }
    }
}

#[derive(Debug, Clone, Copy)]
enum Unknown {
    /// A frame's freedom: a unit motion (shift, turn) in its own axes.
    Motion(usize, V3, V3),
    Scalar(usize),
}

fn tier_of(t: Tier) -> usize {
    match t {
        Tier::Hard => 0,
        Tier::Measured { .. } => 1,
        Tier::Default => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A truss made in its own frame (x along its span, y up, in the plane z = 0), with absolute
    /// values and nothing free inside it; two plates' bearing points in the world, at different heights.
    fn truss_on_two_plates() -> (System, FrameId, ScalarId, ScalarId) {
        let mut s = System::new();
        // Its nominal place: somewhere near, and not square to anything.
        let truss = s.frame("truss", WORLD, Joint::Free, Pose::at([0.3, 0.2, 2.0]).moved([0.0; 3], [0.2, -0.1, 0.15]));
        let (heel, far) = ([0.0, 0.0, 0.0], [7.0, 0.0, 0.0]);
        let s1 = s.scalar("where the truss bears on the west plate", 0.7);
        let s2 = s.scalar("where the truss bears on the east plate", 6.3);
        // The model that uses the truss: two points on its bottom chord, each on a plate.
        s.constrain("truss on the west plate", Tier::Hard, Kind::Coincident(Point::Along(truss, heel, far, s1), Point::At(WORLD, [0.0, 3.0, 2.477])));
        s.constrain("truss on the east plate", Tier::Hard, Kind::Coincident(Point::Along(truss, heel, far, s2), Point::At(WORLD, [6.0, 3.0, 2.439])));
        (s, truss, s1, s2)
    }

    #[test]
    fn two_bearings_leave_a_slide_and_a_roll() {
        let (mut s, ..) = truss_on_two_plates();
        let r = s.solve();
        assert!(!r.refused, "{r:#?}");
        assert_eq!((r.unknowns, r.by_hard, r.open.len()), (8, 6, 2), "{r:#?}");
    }

    #[test]
    fn a_hint_and_a_datum_settle_the_rest_and_the_truss_tilts() {
        let (mut s, truss, s1, _) = truss_on_two_plates();
        // The generator's hint: its y is up. A datum: its heel 0.1 m outside the west bearing.
        s.constrain("the truss stands up", Tier::Default, Kind::Parallel(Dir(truss, [0.0, 1.0, 0.0]), Dir(WORLD, [0.0, 0.0, 1.0])));
        s.constrain("heel overhang", Tier::Hard, Kind::Distance(Point::At(truss, [0.0; 3]), Point::At(WORLD, [0.0, 3.0, 2.477]), 0.1));
        let r = s.solve();
        assert!(!r.refused && r.open.is_empty(), "{r:#?}");
        assert_eq!((r.by_hard, r.by_default), (7, 1), "{r:#?}");
        // In its plane it tilts as the bearings say: 38 mm down over 6 m.
        let x = s.dir(&Dir(truss, [1.0, 0.0, 0.0]));
        assert!((x[2] - (-0.038 / (6.0f64 * 6.0 + 0.038 * 0.038).sqrt())).abs() < 1e-9 && x[1].abs() < 1e-9, "{x:?}");
        // Out of its plane it is plumb (the hint), which the bearings do not contradict.
        let z = s.dir(&Dir(truss, [0.0, 0.0, 1.0]));
        assert!(z[2].abs() < 1e-7, "{z:?}");
        // The hint is not quite met (the truss's up is tilted with it), and that is reported, not forced.
        let hint = r.gaps.iter().find(|g| g.constraint == "the truss stands up").unwrap();
        assert!((hint.size - 0.038 / 6.0).abs() < 1e-4, "{hint:?}");
        assert!((s.value(s1) - 0.1).abs() < 1e-9);
    }

    #[test]
    fn a_measurement_gives_way_to_a_hard_constraint_and_says_by_how_much() {
        let (mut s, truss, s1, _) = truss_on_two_plates();
        s.constrain("the truss stands up", Tier::Default, Kind::Parallel(Dir(truss, [0.0, 1.0, 0.0]), Dir(WORLD, [0.0, 0.0, 1.0])));
        s.constrain("heel overhang", Tier::Hard, Kind::Value(s1, 0.1));
        // A reading that the far end of the chord is at z = 2.40; the bearings put it lower.
        s.constrain("far end, from the scan", Tier::Measured { sigma: 0.005 }, Kind::Offset(Point::At(WORLD, [0.0; 3]), Point::At(truss, [7.0, 0.0, 0.0]), Dir(WORLD, [0.0, 0.0, 1.0]), 2.40));
        let r = s.solve();
        assert!(!r.refused, "{r:#?}");
        let g = r.gaps.iter().find(|g| g.constraint == "far end, from the scan").unwrap();
        assert!(g.size > 0.02 && g.size < 0.04, "the reading is not met, and its gap is known: {g:?}");
    }

    #[test]
    fn hard_constraints_that_cannot_all_hold_are_refused_and_named() {
        let (mut s, _, s1, s2) = truss_on_two_plates();
        // The bearings are 6 m apart; these put them 7 m apart along the chord.
        s.constrain("bears at its heel", Tier::Hard, Kind::Value(s1, 0.0));
        s.constrain("bears at its far end", Tier::Hard, Kind::Value(s2, 7.0));
        let r = s.solve();
        assert!(r.refused, "{r:#?}");
        // All four are in it (any one released would let the rest hold), and between them they are a metre out.
        let names: Vec<&str> = r.conflict.iter().map(|g| g.constraint.as_str()).collect();
        for n in ["truss on the west plate", "truss on the east plate", "bears at its heel", "bears at its far end"] {
            assert!(names.contains(&n), "{names:?}");
        }
        let total: f64 = r.conflict.iter().map(|g| g.size).sum();
        assert!((total - 1.0).abs() < 0.01, "a metre to share: {:#?}", r.conflict);
    }

    #[test]
    fn a_sketch_is_a_frame_whose_things_are_in_its_plane() {
        // A point free to move in a sketch's plane (a child frame on a planar joint), held 5 from the
        // origin and level with a point at height 3: a 3-4-5 triangle, in a sketch that is itself tilted.
        let mut s = System::new();
        let sketch = s.frame("sketch", WORLD, Joint::Fixed, Pose::at([1.0, 2.0, 3.0]).moved([0.0; 3], [0.4, 0.0, 0.0]));
        let p = s.frame("corner", sketch, Joint::Planar([0.0, 0.0, 1.0]), Pose::at([3.5, 2.5, 0.0]));
        s.constrain("5 from the origin", Tier::Hard, Kind::Distance(Point::At(sketch, [0.0; 3]), Point::At(p, [0.0; 3]), 5.0));
        s.constrain("3 up the sketch", Tier::Hard, Kind::Offset(Point::At(sketch, [0.0; 3]), Point::At(p, [0.0; 3]), Dir(sketch, [0.0, 1.0, 0.0]), 3.0));
        let r = s.solve();
        // Its turning in the plane is nothing anyone asked about: it is reported as open.
        assert!(!r.refused && r.by_hard == 2 && r.open.len() == 1 && r.open[0].contains("turn"), "{r:#?}");
        let local = s.frames[p.0].pose.t;
        assert!((local[0] - 4.0).abs() < 1e-8 && (local[1] - 3.0).abs() < 1e-8 && local[2].abs() < 1e-12, "{local:?}");
    }
}
