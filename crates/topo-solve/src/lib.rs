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
//!
//! The problem is held as blocks and sparse operators between them
//! ([`blocks`]): the frames' poses as the topology, joints as a block
//! diagonal, each constraint as a row with a block for each frame and scalar
//! it touches. The derivative ([`Linearised`]) is their product, applied
//! either way and never formed; a step moves each frame along its joint, so
//! a pose stays a pose.

pub mod blocks;
pub mod linalg;

use blocks::{BlockAct, BlockActTranspose, BlockCol, BlockVector, Diagonal, ParentChild, SparseRows};
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
    /// A direction of the parent, in the frame's own axes.
    pub fn rotate_back(&self, v: V3) -> V3 {
        std::array::from_fn(|k| self.r[0][k] * v[0] + self.r[1][k] * v[1] + self.r[2][k] * v[2])
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
    /// Each freedom as a unit twist (turn first).
    pub fn basis(&self) -> Vec<Twist> {
        self.freedoms().into_iter().map(|(v, w, _)| twist(w, v)).collect()
    }

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

/// A small motion of a frame, in its own axes: a turn (an axis times an angle), then a shift.
pub type Twist = [f64; 6];

fn twist(w: V3, v: V3) -> Twist {
    [w[0], w[1], w[2], v[0], v[1], v[2]]
}
fn turn_of(x: &Twist) -> V3 {
    [x[0], x[1], x[2]]
}
fn shift_of(x: &Twist) -> V3 {
    [x[3], x[4], x[5]]
}

/// A frame's pose in its parent, as a block of the topology: it takes the
/// parent's place in the world to the frame's (a composition, not a product).
impl BlockAct<Pose> for Pose {
    type Output = Pose;
    fn act(&self, parent: &Pose) -> Pose {
        parent.then(self)
    }
}

/// A frame's pose in its parent, seen as how it carries a motion: the parent's
/// twist, as the frame has it in its own axes; and, transposed, what is asked
/// of the frame's twist as asked of the parent's.
#[derive(Debug, Clone, Copy)]
pub struct Carry(pub Pose);

impl BlockAct<Twist> for Carry {
    type Output = Twist;
    fn act(&self, parent: &Twist) -> Twist {
        let (w, v) = (turn_of(parent), shift_of(parent));
        twist(self.0.rotate_back(w), self.0.rotate_back(add(v, cross(w, self.0.t))))
    }
}

impl BlockActTranspose<Twist> for Carry {
    type Output = Twist;
    fn act_transpose(&self, own: &Twist) -> Twist {
        let (m, f) = (self.0.rotate(turn_of(own)), self.0.rotate(shift_of(own)));
        twist(add(m, cross(self.0.t, f)), f)
    }
}

/// A joint as a block: its coordinates (one to a freedom) to the frame's twist, and back.
impl BlockAct<Vec<f64>> for Joint {
    type Output = Option<Twist>;
    fn act(&self, q: &Vec<f64>) -> Option<Twist> {
        let basis = self.basis();
        (!basis.is_empty()).then(|| std::array::from_fn(|k| basis.iter().zip(q).map(|(b, q)| b[k] * q).sum()))
    }
}

impl BlockActTranspose<Twist> for Joint {
    type Output = Option<Vec<f64>>;
    fn act_transpose(&self, asked: &Twist) -> Option<Vec<f64>> {
        let basis = self.basis();
        (!basis.is_empty()).then(|| basis.iter().map(|b| (0..6).map(|k| b[k] * asked[k]).sum()).collect())
    }
}

/// A constraint's block for one frame: for each of its residuals, how it changes with the frame's twist.
#[derive(Debug, Clone, PartialEq)]
pub struct Rows(pub Vec<Twist>);

impl BlockAct<Twist> for Rows {
    type Output = Vec<f64>;
    fn act(&self, x: &Twist) -> Vec<f64> {
        self.0.iter().map(|r| (0..6).map(|k| r[k] * x[k]).sum()).collect()
    }
}

impl BlockActTranspose<Vec<f64>> for Rows {
    type Output = Twist;
    fn act_transpose(&self, y: &Vec<f64>) -> Twist {
        std::array::from_fn(|k| self.0.iter().zip(y).map(|(r, y)| r[k] * y).sum())
    }
}

/// A constraint's block for one scalar: for each of its residuals, how it changes with the scalar.
#[derive(Debug, Clone, PartialEq)]
pub struct Col(pub Vec<f64>);

impl BlockAct<f64> for Col {
    type Output = Vec<f64>;
    fn act(&self, x: &f64) -> Vec<f64> {
        self.0.iter().map(|c| c * x).collect()
    }
}

impl BlockActTranspose<Vec<f64>> for Col {
    type Output = f64;
    fn act_transpose(&self, y: &Vec<f64>) -> f64 {
        self.0.iter().zip(y).map(|(c, y)| c * y).sum()
    }
}

/// A change of the unknowns: for each frame, its joint's coordinates; for each scalar, a number.
/// (Not a place: a frame is moved by it along its joint, staying a pose.)
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Step {
    pub frames: BlockCol<Vec<f64>>,
    pub scalars: BlockCol<f64>,
}

/// Some constraints' residuals where the system is, and how they change with
/// the unknowns: an operator, the product of a few sparse block operators,
/// applied either way without ever being formed.
///
/// `J = on_frames · carry⁻¹ · joints + on_scalars`: a step's joint
/// coordinates become twists (block diagonal), each frame's twist gathers
/// its ancestors' (a forward substitution through the topology), and each
/// constraint reads the twists of the frames it touches.
#[derive(Debug, Clone)]
pub struct Linearised {
    carry: ParentChild<Carry>,
    joints: Diagonal<Joint>,
    on_frames: SparseRows<Rows>,
    on_scalars: SparseRows<Col>,
    scalars: usize,
    /// Each constraint's residual.
    pub residual: Vec<Vec<f64>>,
}

impl Linearised {
    /// How the residuals change with a step.
    pub fn apply(&self, x: &Step) -> BlockCol<Vec<f64>> {
        let twists = self.carry.solve(&self.joints.apply(&x.frames));
        let (by_frames, by_scalars): (BlockCol<Vec<f64>>, BlockCol<Vec<f64>>) = (self.on_frames.apply(&twists), self.on_scalars.apply(&x.scalars));
        by_frames
            .into_iter()
            .zip(by_scalars)
            .map(|(a, b)| match (a, b) {
                (Some(a), Some(b)) => Some(a.block_add(&b)),
                (a, b) => a.or(b),
            })
            .collect()
    }

    /// The transpose: what is asked of each residual, as asked of each unknown.
    pub fn apply_transpose(&self, y: &BlockCol<Vec<f64>>) -> Step {
        let asked: BlockCol<Twist> = self.carry.solve_transpose(&self.on_frames.apply_transpose(y, self.joints.0.len()));
        Step { frames: self.joints.apply_transpose(&asked), scalars: self.on_scalars.apply_transpose(y, self.scalars) }
    }
}

/// What a constraint asks of the frames and scalars it touches.
#[derive(Default)]
struct Terms {
    frames: Vec<(usize, Rows)>,
    scalars: Vec<(usize, Col)>,
}

#[derive(Debug, Clone)]
pub struct System {
    names: Vec<String>,
    /// The topology: each frame's pose in its parent.
    poses: ParentChild<Pose>,
    joints: Diagonal<Joint>,
    scalars: Vec<(String, f64)>,
    pub constraints: Vec<Constraint>,
}

/// The world: the frame everything is in.
pub const WORLD: FrameId = FrameId(0);

const AXES: [V3; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

impl Default for System {
    fn default() -> System {
        System::new()
    }
}

impl System {
    pub fn new() -> System {
        let mut s = System { names: vec![], poses: ParentChild::default(), joints: Diagonal(vec![]), scalars: vec![], constraints: vec![] };
        s.names.push("world".into());
        s.poses.push(None, Pose::identity());
        s.joints.0.push(Joint::Fixed);
        s
    }

    /// A frame in `parent`, at `pose` there to start with (its nominal place), free as `joint` says.
    pub fn frame(&mut self, name: &str, parent: FrameId, joint: Joint, pose: Pose) -> FrameId {
        self.names.push(name.into());
        self.joints.0.push(joint);
        FrameId(self.poses.push(Some(parent.0), pose))
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

    /// A frame's pose in its parent.
    pub fn in_parent(&self, f: FrameId) -> Pose {
        self.poses.blocks()[f.0]
    }

    /// Every frame's pose in the world.
    pub fn world(&self) -> Vec<Pose> {
        self.poses.from_world(&Pose::identity())
    }

    /// A frame's pose in the world.
    pub fn in_world(&self, f: FrameId) -> Pose {
        self.world()[f.0]
    }

    /// A point, in the world.
    pub fn point(&self, p: &Point) -> V3 {
        self.point_in(&self.world(), p)
    }

    /// A direction, in the world (a unit vector).
    pub fn dir(&self, d: &Dir) -> V3 {
        self.dir_in(&self.world(), d)
    }

    /// The frame a point is in, and its coordinates there.
    fn local(&self, p: &Point) -> (usize, V3) {
        match *p {
            Point::At(f, x) => (f.0, x),
            Point::Along(f, a, b, s) => (f.0, add(a, scale(unit(sub(b, a)), self.value(s)))),
        }
    }

    fn point_in(&self, world: &[Pose], p: &Point) -> V3 {
        let (f, x) = self.local(p);
        world[f].apply(x)
    }

    fn dir_in(&self, world: &[Pose], d: &Dir) -> V3 {
        unit(world[d.0 .0].rotate(d.1))
    }

    /// The smallest scope a constraint is within: the nearest frame that all the frames it touches are in.
    pub fn scope(&self, k: &Kind) -> FrameId {
        let frames: Vec<usize> = match k {
            Kind::Coincident(a, b) | Kind::Distance(a, b, _) => vec![self.local(a).0, self.local(b).0],
            Kind::Offset(a, b, d, _) => vec![self.local(a).0, self.local(b).0, d.0 .0],
            Kind::Parallel(a, b) | Kind::Perpendicular(a, b) => vec![a.0 .0, b.0 .0],
            Kind::Value(..) => vec![],
        };
        let parents = self.poses.parents();
        // Parents are numbered first: step the later of two up until they meet.
        let meet = |mut a: usize, mut b: usize| {
            while a != b {
                if a > b {
                    a = parents[a].unwrap_or(0);
                } else {
                    b = parents[b].unwrap_or(0);
                }
            }
            a
        };
        FrameId(frames.into_iter().reduce(meet).unwrap_or(0))
    }

    /// How far a constraint is from holding, as numbers that are all zero when it does.
    fn residual(&self, world: &[Pose], k: &Kind) -> Vec<f64> {
        let (point, dir) = (|p: &Point| self.point_in(world, p), |d: &Dir| self.dir_in(world, d));
        match k {
            Kind::Coincident(a, b) => sub(point(b), point(a)).to_vec(),
            Kind::Distance(a, b, d) => vec![norm(sub(point(b), point(a))) - d],
            Kind::Offset(a, b, along, d) => vec![dot(sub(point(b), point(a)), dir(along)) - d],
            Kind::Parallel(a, b) => cross(dir(a), dir(b)).to_vec(),
            Kind::Perpendicular(a, b) => vec![dot(dir(a), dir(b))],
            Kind::Value(s, v) => vec![self.value(*s) - v],
        }
    }

    /// What moving a point asks: `grads` is, for each residual, how it changes with the point's place in the world.
    fn point_terms(&self, world: &[Pose], p: &Point, grads: &[V3], out: &mut Terms) {
        let (f, x) = self.local(p);
        // The same, in the frame's own axes: a turn w and a shift v move the point by v + w × x.
        let own: Vec<V3> = grads.iter().map(|g| world[f].rotate_back(*g)).collect();
        out.frames.push((f, Rows(own.iter().map(|g| twist(cross(x, *g), *g)).collect())));
        if let Point::Along(_, a, b, s) = *p {
            let u = unit(sub(b, a));
            out.scalars.push((s.0, Col(own.iter().map(|g| dot(*g, u)).collect())));
        }
    }

    /// What turning a direction asks: `grads` is, for each residual, how it changes with the direction in the world.
    fn dir_terms(&self, world: &[Pose], d: &Dir, grads: &[V3], out: &mut Terms) {
        let f = d.0 .0;
        let own = unit(d.1);
        // A turn w moves the direction by w × d; a shift does not move it.
        out.frames.push((f, Rows(grads.iter().map(|g| twist(cross(own, world[f].rotate_back(*g)), [0.0; 3])).collect())));
    }

    /// A constraint's blocks: how its residuals change with the twists of the frames it touches, and with its scalars.
    fn terms(&self, world: &[Pose], k: &Kind) -> Terms {
        let mut out = Terms::default();
        let neg = |g: &[V3]| g.iter().map(|g| scale(*g, -1.0)).collect::<Vec<V3>>();
        match k {
            Kind::Coincident(a, b) => {
                self.point_terms(world, b, &AXES, &mut out);
                self.point_terms(world, a, &neg(&AXES), &mut out);
            }
            Kind::Distance(a, b, _) => {
                let n = [unit(sub(self.point_in(world, b), self.point_in(world, a)))];
                self.point_terms(world, b, &n, &mut out);
                self.point_terms(world, a, &neg(&n), &mut out);
            }
            Kind::Offset(a, b, along, _) => {
                let u = [self.dir_in(world, along)];
                self.point_terms(world, b, &u, &mut out);
                self.point_terms(world, a, &neg(&u), &mut out);
                self.dir_terms(world, along, &[sub(self.point_in(world, b), self.point_in(world, a))], &mut out);
            }
            Kind::Parallel(a, b) => {
                let (da, db) = (self.dir_in(world, a), self.dir_in(world, b));
                self.dir_terms(world, a, &AXES.map(|e| cross(db, e)), &mut out);
                self.dir_terms(world, b, &AXES.map(|e| cross(e, da)), &mut out);
            }
            Kind::Perpendicular(a, b) => {
                let (da, db) = (self.dir_in(world, a), self.dir_in(world, b));
                self.dir_terms(world, a, &[db], &mut out);
                self.dir_terms(world, b, &[da], &mut out);
            }
            Kind::Value(s, _) => out.scalars.push((s.0, Col(vec![1.0]))),
        }
        out
    }

    /// One tier's constraints, linearised where the system is (measured ones in units of their tolerance).
    pub fn linearised(&self, tier: usize) -> Linearised {
        let world = self.world();
        let mut l = Linearised {
            carry: self.poses.map(|p| Carry(*p)),
            joints: self.joints.clone(),
            on_frames: SparseRows(vec![]),
            on_scalars: SparseRows(vec![]),
            scalars: self.scalars.len(),
            residual: vec![],
        };
        for c in self.constraints.iter().filter(|c| tier_of(c.tier) == tier) {
            let w = weight(c.tier);
            let t = self.terms(&world, &c.kind);
            l.residual.push(self.residual(&world, &c.kind).iter().map(|r| r * w).collect());
            l.on_frames.0.push(t.frames.into_iter().map(|(f, Rows(r))| (f, Rows(r.iter().map(|r| r.map(|x| x * w)).collect()))).collect());
            l.on_scalars.0.push(t.scalars.into_iter().map(|(s, Col(c))| (s, Col(c.iter().map(|x| x * w).collect()))).collect());
        }
        l
    }

    /// What to call each unknown, in the order they are laid out flat: each frame's freedoms, then the scalars.
    fn unknowns(&self) -> Vec<String> {
        let frames = self.joints.0.iter().zip(&self.names).flat_map(|(j, name)| j.freedoms().into_iter().map(move |(_, _, what)| format!("{name} can {what}")));
        frames.chain(self.scalars.iter().map(|(name, _)| format!("{name} can change"))).collect()
    }

    /// A step, from its numbers laid out flat.
    fn step_of(&self, flat: &[f64]) -> Step {
        let mut at = 0;
        let frames = (self.joints.0.iter())
            .map(|j| {
                let k = j.basis().len();
                at += k;
                (k > 0).then(|| flat[at - k..at].to_vec())
            })
            .collect();
        Step { frames, scalars: flat[at..].iter().map(|x| Some(*x)).collect() }
    }

    /// The system with its unknowns changed by a step: each frame moved along its joint (staying a pose), each scalar added to.
    pub fn stepped(&self, step: &Step) -> System {
        let mut s = self.clone();
        let moves: BlockCol<Twist> = self.joints.apply(&step.frames);
        for (pose, m) in s.poses.blocks_mut().iter_mut().zip(&moves) {
            if let Some(m) = m {
                *pose = pose.moved(shift_of(m), turn_of(m));
            }
        }
        for (s, d) in s.scalars.iter_mut().zip(&step.scalars) {
            s.1 += d.unwrap_or(0.0);
        }
        s
    }

    /// The residuals of one tier's constraints (measured ones in units of their tolerance), stacked.
    fn tier_residual(&self, tier: usize) -> Vec<f64> {
        let world = self.world();
        (self.constraints.iter().filter(|c| tier_of(c.tier) == tier)).flat_map(|c| self.residual(&world, &c.kind).into_iter().map(move |r| r * weight(c.tier))).collect()
    }

    /// One tier's residual and, written out, how it changes with each unknown: the operator applied to
    /// each unknown in turn. (Written out only for the small dense solve below.)
    fn dense(&self, tier: usize, n: usize) -> (Vec<f64>, Mat) {
        let l = self.linearised(tier);
        let r: Vec<f64> = l.residual.iter().flatten().copied().collect();
        let mut j = zeros(r.len(), n);
        let mut one = vec![0.0; n];
        for k in 0..n {
            one[k] = 1.0;
            let col = l.apply(&self.step_of(&one));
            one[k] = 0.0;
            let mut i = 0;
            for (block, r) in col.iter().zip(&l.residual) {
                if let Some(block) = block {
                    for (d, x) in block.iter().enumerate() {
                        j[i + d][k] = *x;
                    }
                }
                i += r.len();
            }
        }
        (r, j)
    }

    /// The step the tiers ask for, tier by tier, each within what the ones before leave free:
    /// the step, how many freedoms each tier settled, and what is left free (a basis, as columns).
    fn tiered_step(&self, n: usize) -> (Vec<f64>, [usize; 3], Mat) {
        let (mut dx, mut free, mut ranks) = (vec![0.0; n], linalg::identity(n), [0; 3]);
        for tier in 0..3 {
            let (r, j) = self.dense(tier, n);
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
            let (dx, _, _) = self.tiered_step(n);
            let size = dx.iter().map(|x| x * x).sum::<f64>().sqrt();
            if size < 1e-11 {
                break;
            }
            // The whole step, or as much of it as does not make things worse.
            let mut taken = false;
            for halving in 0..12 {
                let f = 0.5f64.powi(halving);
                let next = self.stepped(&self.step_of(&dx.iter().map(|x| x * f).collect::<Vec<_>>()));
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
        let (_, ranks, free) = self.tiered_step(n);
        let world = self.world();
        let gaps: Vec<Gap> = self.constraints.iter().map(|c| Gap { constraint: c.name.clone(), tier: c.tier, size: self.residual(&world, &c.kind).iter().map(|x| x * x).sum::<f64>().sqrt() }).collect();
        // (A tenth of a micron, or of a microradian.)
        let refused = self.hard_gap() > 1e-7;
        let mut conflict: Vec<Gap> = if refused { gaps.iter().filter(|g| g.tier == Tier::Hard && g.size > 1e-7).cloned().collect() } else { vec![] };
        conflict.sort_by(|a, b| b.size.total_cmp(&a.size));
        // What is still open, each as the unknowns that move together in it.
        let tidy = tidy_basis(&free);
        let open = (0..tidy.first().map_or(0, Vec::len))
            .map(|c| {
                let col: Vec<f64> = (0..n).map(|j| tidy[j][c]).collect();
                let top = col.iter().fold(0.0f64, |a, x| a.max(x.abs()));
                let parts: Vec<&str> = (0..n).filter(|&j| col[j].abs() > 0.3 * top).map(|j| unknowns[j].as_str()).collect();
                parts.join(", together with: ")
            })
            .collect();
        Report { refused, conflict, gaps, passes, unknowns: n, by_hard: ranks[0], by_measured: ranks[1], by_default: ranks[2], open }
    }
}

fn tier_of(t: Tier) -> usize {
    match t {
        Tier::Hard => 0,
        Tier::Measured { .. } => 1,
        Tier::Default => 2,
    }
}

/// What a tier's residuals are multiplied by: a measurement is in units of its tolerance.
fn weight(t: Tier) -> f64 {
    match t {
        Tier::Measured { sigma } => 1.0 / sigma.max(1e-12),
        _ => 1.0,
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

    /// Frames three deep on every kind of joint, with every kind of constraint in every tier, nowhere near solved.
    fn a_bit_of_everything() -> System {
        let (mut s, truss, s1, s2) = truss_on_two_plates();
        let wall = s.frame("wall", WORLD, Joint::Hinge([0.0, 0.0, 1.0]), Pose::at([1.0, -2.0, 0.5]).moved([0.0; 3], [0.3, 0.2, -0.4]));
        let sketch = s.frame("sketch", wall, Joint::Slider([1.0, 2.0, 0.0]), Pose::at([0.2, 0.4, 1.0]).moved([0.0; 3], [-0.5, 0.1, 0.2]));
        let corner = s.frame("corner", sketch, Joint::Planar([0.0, 0.0, 1.0]), Pose::at([3.5, 2.5, 0.0]).moved([0.0; 3], [0.0, 0.0, 0.7]));
        let rider = s.frame("rider", truss, Joint::Free, Pose::at([1.0, 0.3, 0.2]).moved([0.0; 3], [0.1, 0.5, -0.3]));
        s.constrain("a", Tier::Hard, Kind::Distance(Point::At(corner, [0.1, 0.2, 0.3]), Point::Along(truss, [0.0; 3], [7.0, 1.0, 0.0], s1), 2.0));
        s.constrain("b", Tier::Hard, Kind::Offset(Point::At(rider, [0.4, 0.0, 0.1]), Point::At(corner, [0.0, 1.0, 0.0]), Dir(sketch, [0.0, 1.0, 2.0]), 0.3));
        s.constrain("c", Tier::Measured { sigma: 0.01 }, Kind::Parallel(Dir(rider, [0.0, 1.0, 0.0]), Dir(corner, [1.0, 0.0, 1.0])));
        s.constrain("d", Tier::Measured { sigma: 0.2 }, Kind::Coincident(Point::At(wall, [0.5, 0.5, 0.5]), Point::Along(rider, [0.0; 3], [0.0, 0.0, 1.0], s2)));
        s.constrain("e", Tier::Default, Kind::Perpendicular(Dir(wall, [1.0, 0.0, 0.0]), Dir(rider, [0.0, 0.0, 1.0])));
        s.constrain("f", Tier::Default, Kind::Value(s2, 1.0));
        s.constrain("g", Tier::Default, Kind::Distance(Point::At(corner, [0.0; 3]), Point::At(corner, [1.0, 0.0, 0.0]), 1.5));
        s
    }

    /// Numbers that look random, the same every time.
    fn noise(n: usize, seed: u64) -> Vec<f64> {
        let mut x = seed;
        (0..n)
            .map(|_| {
                x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                (x >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
            })
            .collect()
    }

    #[test]
    fn the_derivative_is_the_change_of_the_residual_along_each_unknown() {
        let s = a_bit_of_everything();
        let n = s.unknowns().len();
        assert_eq!(n, 6 + 1 + 1 + 3 + 6 + 2);
        const H: f64 = 1e-6;
        for tier in 0..3 {
            let (r, j) = s.dense(tier, n);
            assert_eq!(r, s.tier_residual(tier));
            for k in 0..n {
                let at = |h: f64| {
                    let mut dx = vec![0.0; n];
                    dx[k] = h;
                    s.stepped(&s.step_of(&dx)).tier_residual(tier)
                };
                let (hi, lo) = (at(H), at(-H));
                for i in 0..r.len() {
                    let by_differences = (hi[i] - lo[i]) / (2.0 * H);
                    assert!((j[i][k] - by_differences).abs() < 1e-6 * (1.0 + by_differences.abs()), "tier {tier}, residual {i}, unknown {k} ({}): {} against {by_differences}", s.unknowns()[k], j[i][k]);
                }
            }
        }
    }

    #[test]
    fn the_transpose_is_the_transpose() {
        // <y, J x> = <Jᵀ y, x>, for any x and y.
        let s = a_bit_of_everything();
        let n = s.unknowns().len();
        for tier in 0..3 {
            let l = s.linearised(tier);
            let x = s.step_of(&noise(n, 7 + tier as u64));
            let mut ys = noise(l.residual.iter().map(Vec::len).sum(), 99).into_iter();
            let y: BlockCol<Vec<f64>> = l.residual.iter().map(|r| Some(r.iter().map(|_| ys.next().unwrap()).collect())).collect();
            let rows = |a: &BlockCol<Vec<f64>>, b: &BlockCol<Vec<f64>>| -> f64 { a.iter().zip(b).map(|(a, b)| a.iter().flatten().zip(b.iter().flatten()).map(|(a, b)| a * b).sum::<f64>()).sum() };
            let (jx, jty) = (l.apply(&x), l.apply_transpose(&y));
            let unknowns = rows(&jty.frames, &x.frames) + jty.scalars.iter().zip(&x.scalars).map(|(a, b)| a.unwrap_or(0.0) * b.unwrap_or(0.0)).sum::<f64>();
            assert!((rows(&y, &jx) - unknowns).abs() < 1e-9 * (1.0 + unknowns.abs()), "tier {tier}: {} against {unknowns}", rows(&y, &jx));
        }
    }

    #[test]
    fn a_constraint_is_within_the_nearest_frame_that_holds_all_it_touches() {
        let s = a_bit_of_everything();
        let scope = |name: &str| s.names[s.scope(&s.constraints.iter().find(|c| c.name == name).unwrap().kind).0].clone();
        assert_eq!(scope("a"), "world"); // corner (in sketch, in wall) and truss
        assert_eq!(scope("e"), "world");
        assert_eq!(scope("g"), "corner");
        assert_eq!(scope("f"), "world"); // a scalar alone touches no frame
        let mut t = System::new();
        let a = t.frame("a", WORLD, Joint::Free, Pose::identity());
        let (b, c) = (t.frame("b", a, Joint::Free, Pose::identity()), t.frame("c", a, Joint::Free, Pose::identity()));
        assert_eq!(t.scope(&Kind::Parallel(Dir(b, [1.0, 0.0, 0.0]), Dir(c, [1.0, 0.0, 0.0]))), a);
        assert_eq!(t.scope(&Kind::Parallel(Dir(b, [1.0, 0.0, 0.0]), Dir(a, [1.0, 0.0, 0.0]))), a);
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
        let local = s.in_parent(p).t;
        assert!((local[0] - 4.0).abs() < 1e-8 && (local[1] - 3.0).abs() < 1e-8 && local[2].abs() < 1e-12, "{local:?}");
    }
}
