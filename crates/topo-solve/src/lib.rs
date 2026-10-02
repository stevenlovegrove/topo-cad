//! Frames, and constraints between things in them.
//!
//! - A **frame** is a coordinate system placed in a parent frame; its
//!   **joint** says which of its six freedoms are open. Geometry is made in a
//!   frame's own coordinates (a generator's vertices are plain numbers there)
//!   and is referred to from elsewhere only through the frames.
//! - A **scalar** is an unknown number (how far along a member a point is, a
//!   span): separate from the frames' poses, and belonging to a frame.
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
//! either way; a step moves each frame along its joint, so a pose stays a
//! pose. A constraint's **scope** is the nearest frame holding all it
//! touches: its row has blocks only for unknowns inside that. Unknowns that
//! no constraint ties together are separate **components**, solved apart.

pub mod blocks;
pub mod linalg;

use blocks::{accumulate, BlockAct, BlockActTranspose, BlockCol, BlockVector, Diagonal, ParentChild, SparseRows};
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

/// A number: a constant, and so much of each of some scalars.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Num {
    pub constant: f64,
    pub terms: Vec<(ScalarId, f64)>,
}

impl Num {
    /// And `k` of the scalar `s`.
    pub fn plus(mut self, s: ScalarId, k: f64) -> Num {
        self.terms.push((s, k));
        self
    }
}

impl From<f64> for Num {
    fn from(constant: f64) -> Num {
        Num { constant, terms: vec![] }
    }
}

impl From<ScalarId> for Num {
    fn from(s: ScalarId) -> Num {
        Num { constant: 0.0, terms: vec![(s, 1.0)] }
    }
}

/// A point, in a frame: at some coordinates of it, and so far along each of some directions of it as scalars say.
#[derive(Debug, Clone, PartialEq)]
pub struct Point {
    pub frame: FrameId,
    pub at: V3,
    pub along: Vec<(ScalarId, V3)>,
}

impl Point {
    /// At these coordinates of the frame.
    pub fn at(frame: FrameId, at: V3) -> Point {
        Point { frame, at, along: vec![] }
    }

    /// And `s` (in the frame's unit) times `dir`.
    pub fn plus(mut self, s: ScalarId, dir: V3) -> Point {
        self.along.push((s, dir));
        self
    }

    /// On the line from `a` toward `b` of the frame, a distance `s` from `a`.
    pub fn along(frame: FrameId, a: V3, b: V3, s: ScalarId) -> Point {
        Point::at(frame, a).plus(s, unit(sub(b, a)))
    }
}

/// A direction, in a frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dir(pub FrameId, pub V3);

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    /// The two points are the same place.
    Coincident(Point, Point),
    /// The two points are this far apart.
    Distance(Point, Point, Num),
    /// From the first point to the second is this far along the direction (0: level with it, on its plane).
    Offset(Point, Point, Dir, Num),
    /// The directions are the same.
    Aligned(Dir, Dir),
    /// The directions agree or oppose: the lines along them are parallel. It is held as `Aligned`
    /// in whichever sense is nearer where the system is, so it pulls toward that one from anywhere.
    Parallel(Dir, Dir),
    /// The directions are at right angles.
    Perpendicular(Dir, Dir),
    /// The two numbers are the same.
    Equal(Num, Num),
}

impl Kind {
    pub fn distance(a: Point, b: Point, d: impl Into<Num>) -> Kind {
        Kind::Distance(a, b, d.into())
    }
    pub fn offset(a: Point, b: Point, along: Dir, d: impl Into<Num>) -> Kind {
        Kind::Offset(a, b, along, d.into())
    }
    pub fn equal(a: impl Into<Num>, b: impl Into<Num>) -> Kind {
        Kind::Equal(a.into(), b.into())
    }

    /// The frames and the scalars it reads.
    fn touches(&self) -> (Vec<usize>, Vec<usize>) {
        let (mut frames, mut scalars) = (vec![], vec![]);
        let mut point = |p: &Point, frames: &mut Vec<usize>| {
            frames.push(p.frame.0);
            scalars.extend(p.along.iter().map(|(s, _)| s.0));
        };
        let mut nums: Vec<&Num> = vec![];
        match self {
            Kind::Coincident(a, b) => {
                point(a, &mut frames);
                point(b, &mut frames);
            }
            Kind::Distance(a, b, d) => {
                point(a, &mut frames);
                point(b, &mut frames);
                nums.push(d);
            }
            Kind::Offset(a, b, along, d) => {
                point(a, &mut frames);
                point(b, &mut frames);
                frames.push(along.0 .0);
                nums.push(d);
            }
            Kind::Aligned(a, b) | Kind::Parallel(a, b) | Kind::Perpendicular(a, b) => frames.extend([a.0 .0, b.0 .0]),
            Kind::Equal(a, b) => nums.extend([a, b]),
        }
        scalars.extend(nums.iter().flat_map(|n| n.terms.iter().map(|(s, _)| s.0)));
        (frames, scalars)
    }
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

/// Hard constraints that cannot all hold, and the part of the system they are in.
#[derive(Debug, Clone, PartialEq)]
pub struct Conflict {
    /// The ones not met, the worst first, at the pose that comes nearest to meeting them all.
    pub constraints: Vec<Gap>,
    /// The frames that move together with them (and with nothing else).
    pub frames: Vec<String>,
    /// The nearest frame that all of them are within.
    pub scope: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// Some hard constraints cannot all hold: the system is as near to them as it gets, and is not to be used.
    pub refused: bool,
    /// Each part of the system whose hard constraints cannot all hold.
    pub conflicts: Vec<Conflict>,
    /// Every constraint's gap.
    pub gaps: Vec<Gap>,
    /// The most passes any part took.
    pub passes: usize,
    /// The parts that were solved on their own (nothing ties one to another).
    pub components: usize,
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

/// The same operators take a constraint's whole block at once (each of its
/// residuals' asks together), which is how its row of the derivative is written out.
impl BlockVector for Rows {
    fn block_add(&self, rhs: &Rows) -> Rows {
        Rows(self.0.iter().zip(&rhs.0).map(|(a, b)| a.block_add(b)).collect())
    }
}

impl BlockActTranspose<Rows> for Carry {
    type Output = Rows;
    fn act_transpose(&self, own: &Rows) -> Rows {
        Rows(own.0.iter().map(|r| self.act_transpose(r)).collect())
    }
}

impl BlockActTranspose<Rows> for Joint {
    type Output = Option<Mat>;
    fn act_transpose(&self, asked: &Rows) -> Option<Mat> {
        asked.0.iter().map(|r| self.act_transpose(r)).collect()
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
/// applied either way without being formed.
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
    scopes: Vec<usize>,
    /// Which constraint each row is.
    pub constraints: Vec<usize>,
    /// Each constraint's residual.
    pub residual: Vec<Vec<f64>>,
}

/// One constraint's row of the derivative, written out: a block for each unknown it depends on, and no others.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub constraint: usize,
    pub residual: Vec<f64>,
    /// For a frame: each residual against each of its joint's coordinates.
    pub frames: Vec<(usize, Mat)>,
    /// For a scalar: each residual against it.
    pub scalars: Vec<(usize, Vec<f64>)>,
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

    /// Row `i`, written out: the same transposed operators, taking the constraint's blocks whole.
    ///
    /// Only frames inside the constraint's scope have a block. What it asks of the frames it
    /// touches comes to nothing once gathered at its scope (moving everything it touches
    /// together changes nothing), so the frames from there up are left out: exactly, not nearly.
    pub fn row(&self, i: usize) -> Row {
        let mut asked: BlockCol<Rows> = vec![None; self.joints.0.len()];
        for (f, rows) in &self.on_frames.0[i] {
            accumulate(&mut asked[*f], rows);
        }
        let blocks: BlockCol<Mat> = self.joints.apply_transpose(&self.carry.solve_transpose(&asked));
        let frames = blocks.into_iter().enumerate().filter(|(f, _)| *f > self.scopes[i]).filter_map(|(f, b)| b.map(|b| (f, b))).collect();
        let mut scalars: Vec<(usize, Vec<f64>)> = vec![];
        for (s, Col(c)) in &self.on_scalars.0[i] {
            match scalars.iter_mut().find(|(t, _)| t == s) {
                Some((_, sum)) => *sum = sum.block_add(c),
                None => scalars.push((*s, c.clone())),
            }
        }
        Row { constraint: self.constraints[i], residual: self.residual[i].clone(), frames, scalars }
    }

    pub fn rows(&self) -> Vec<Row> {
        (0..self.constraints.len()).map(|i| self.row(i)).collect()
    }
}

/// A part of the system that nothing ties to the rest: unknowns, and the constraints that
/// depend on them and on no others. It is solved on its own.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Component {
    /// Frames whose joints are open.
    pub frames: Vec<usize>,
    pub scalars: Vec<usize>,
    pub constraints: Vec<usize>,
}

/// What a constraint asks of the frames and scalars it touches.
#[derive(Default)]
struct Terms {
    frames: Vec<(usize, Rows)>,
    scalars: Vec<(usize, Col)>,
}

#[derive(Debug, Clone)]
struct Scalar {
    name: String,
    /// The frame it belongs to: it is within that frame's scope.
    owner: usize,
    value: f64,
}

#[derive(Debug, Clone)]
pub struct System {
    names: Vec<String>,
    /// The topology: each frame's pose in its parent.
    poses: ParentChild<Pose>,
    joints: Diagonal<Joint>,
    scalars: Vec<Scalar>,
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

    /// An unknown number belonging to `owner` (a parameter of what is made there, or of a point in it), starting at `value`.
    pub fn scalar(&mut self, owner: FrameId, name: &str, value: f64) -> ScalarId {
        self.scalars.push(Scalar { name: name.into(), owner: owner.0, value });
        ScalarId(self.scalars.len() - 1)
    }

    pub fn constrain(&mut self, name: &str, tier: Tier, kind: Kind) {
        self.constraints.push(Constraint { name: name.into(), tier, kind });
    }

    pub fn name(&self, f: FrameId) -> &str {
        &self.names[f.0]
    }

    pub fn value(&self, s: ScalarId) -> f64 {
        self.scalars[s.0].value
    }

    pub fn num(&self, n: &Num) -> f64 {
        n.constant + n.terms.iter().map(|(s, k)| k * self.value(*s)).sum::<f64>()
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

    /// A point's coordinates in its frame.
    fn local(&self, p: &Point) -> V3 {
        p.along.iter().fold(p.at, |x, (s, u)| add(x, scale(*u, self.value(*s))))
    }

    fn point_in(&self, world: &[Pose], p: &Point) -> V3 {
        world[p.frame.0].apply(self.local(p))
    }

    fn dir_in(&self, world: &[Pose], d: &Dir) -> V3 {
        unit(world[d.0 .0].rotate(d.1))
    }

    /// The nearest frame that two frames are both in (or are).
    fn meet(&self, mut a: usize, mut b: usize) -> usize {
        // Parents are numbered first: step the later of the two up until they meet.
        let parents = self.poses.parents();
        while a != b {
            if a > b {
                a = parents[a].unwrap_or(0);
            } else {
                b = parents[b].unwrap_or(0);
            }
        }
        a
    }

    /// The smallest scope a constraint is within: the nearest frame that holds every frame
    /// it touches and every scalar it reads. Nothing outside that frame bears on it.
    pub fn scope(&self, k: &Kind) -> FrameId {
        let (frames, scalars) = k.touches();
        FrameId(frames.into_iter().chain(scalars.into_iter().map(|s| self.scalars[s].owner)).reduce(|a, b| self.meet(a, b)).unwrap_or(0))
    }

    /// How far a constraint is from holding, as numbers that are all zero when it does.
    ///
    /// A residual that is a vector is in the axes of the constraint's scope, not the world's:
    /// so nothing outside the scope changes any of its numbers.
    fn residual(&self, world: &[Pose], k: &Kind) -> Vec<f64> {
        let (point, dir) = (|p: &Point| self.point_in(world, p), |d: &Dir| self.dir_in(world, d));
        let own = |v: V3| world[self.scope(k).0].rotate_back(v).to_vec();
        match k {
            Kind::Coincident(a, b) => own(sub(point(b), point(a))),
            Kind::Distance(a, b, d) => vec![norm(sub(point(b), point(a))) - self.num(d)],
            Kind::Offset(a, b, along, d) => vec![dot(sub(point(b), point(a)), dir(along)) - self.num(d)],
            Kind::Aligned(a, b) => own(sub(dir(b), dir(a))),
            Kind::Parallel(a, b) => own(sub(dir(b), scale(dir(a), sense(dir(a), dir(b))))),
            Kind::Perpendicular(a, b) => vec![dot(dir(a), dir(b))],
            Kind::Equal(a, b) => vec![self.num(a) - self.num(b)],
        }
    }

    /// What moving a point asks: `grads` is, for each residual, how it changes with the point's place in the world.
    fn point_terms(&self, world: &[Pose], p: &Point, grads: &[V3], out: &mut Terms) {
        let (f, x) = (p.frame.0, self.local(p));
        // The same, in the frame's own axes: a turn w and a shift v move the point by v + w × x.
        let own: Vec<V3> = grads.iter().map(|g| world[f].rotate_back(*g)).collect();
        out.frames.push((f, Rows(own.iter().map(|g| twist(cross(x, *g), *g)).collect())));
        for (s, u) in &p.along {
            out.scalars.push((s.0, Col(own.iter().map(|g| dot(*g, *u)).collect())));
        }
    }

    /// What turning a direction asks: `grads` is, for each residual, how it changes with the direction in the world.
    fn dir_terms(&self, world: &[Pose], d: &Dir, grads: &[V3], out: &mut Terms) {
        let f = d.0 .0;
        let own = unit(d.1);
        // A turn w moves the direction by w × d; a shift does not move it.
        out.frames.push((f, Rows(grads.iter().map(|g| twist(cross(own, world[f].rotate_back(*g)), [0.0; 3])).collect())));
    }

    /// What changing a number asks, of a residual that changes `sign` with it.
    fn num_terms(&self, n: &Num, sign: f64, out: &mut Terms) {
        out.scalars.extend(n.terms.iter().map(|(s, k)| (s.0, Col(vec![sign * k]))));
    }

    /// A constraint's blocks: how its residuals change with the twists of the frames it touches, and with its scalars.
    fn terms(&self, world: &[Pose], k: &Kind) -> Terms {
        let mut out = Terms::default();
        let neg = |g: &[V3]| g.iter().map(|g| scale(*g, -1.0)).collect::<Vec<V3>>();
        // For a residual that is a vector: the scope's axes in the world, and what turning the
        // scope asks (the vector, r, is read in its axes: a turn w changes it by r × w).
        let scope = self.scope(k).0;
        let axes = AXES.map(|e| world[scope].rotate(e));
        let read_in_scope = |r: V3, out: &mut Terms| out.frames.push((scope, Rows(AXES.iter().map(|e| twist(cross(*e, r), [0.0; 3])).collect())));
        match k {
            Kind::Coincident(a, b) => {
                self.point_terms(world, b, &axes, &mut out);
                self.point_terms(world, a, &neg(&axes), &mut out);
                read_in_scope(world[scope].rotate_back(sub(self.point_in(world, b), self.point_in(world, a))), &mut out);
            }
            Kind::Distance(a, b, d) => {
                let n = [unit(sub(self.point_in(world, b), self.point_in(world, a)))];
                self.point_terms(world, b, &n, &mut out);
                self.point_terms(world, a, &neg(&n), &mut out);
                self.num_terms(d, -1.0, &mut out);
            }
            Kind::Offset(a, b, along, d) => {
                let u = [self.dir_in(world, along)];
                self.point_terms(world, b, &u, &mut out);
                self.point_terms(world, a, &neg(&u), &mut out);
                self.dir_terms(world, along, &[sub(self.point_in(world, b), self.point_in(world, a))], &mut out);
                self.num_terms(d, -1.0, &mut out);
            }
            Kind::Aligned(a, b) => {
                self.dir_terms(world, b, &axes, &mut out);
                self.dir_terms(world, a, &neg(&axes), &mut out);
                read_in_scope(world[scope].rotate_back(sub(self.dir_in(world, b), self.dir_in(world, a))), &mut out);
            }
            Kind::Parallel(a, b) => {
                let (da, db) = (self.dir_in(world, a), self.dir_in(world, b));
                let sense = sense(da, db);
                self.dir_terms(world, b, &axes, &mut out);
                self.dir_terms(world, a, &axes.map(|e| scale(e, -sense)), &mut out);
                read_in_scope(world[scope].rotate_back(sub(db, scale(da, sense))), &mut out);
            }
            Kind::Perpendicular(a, b) => {
                let (da, db) = (self.dir_in(world, a), self.dir_in(world, b));
                self.dir_terms(world, a, &[db], &mut out);
                self.dir_terms(world, b, &[da], &mut out);
            }
            Kind::Equal(a, b) => {
                self.num_terms(a, 1.0, &mut out);
                self.num_terms(b, -1.0, &mut out);
            }
        }
        out
    }

    /// These constraints, linearised where the system is (measured ones in units of their tolerance).
    pub fn linearised(&self, constraints: &[usize]) -> Linearised {
        let world = self.world();
        let mut l = Linearised {
            carry: self.poses.map(|p| Carry(*p)),
            joints: self.joints.clone(),
            on_frames: SparseRows(vec![]),
            on_scalars: SparseRows(vec![]),
            scalars: self.scalars.len(),
            scopes: vec![],
            constraints: constraints.to_vec(),
            residual: vec![],
        };
        for c in constraints.iter().map(|&i| &self.constraints[i]) {
            let w = weight(c.tier);
            let t = self.terms(&world, &c.kind);
            l.scopes.push(self.scope(&c.kind).0);
            l.residual.push(self.residual(&world, &c.kind).iter().map(|r| r * w).collect());
            l.on_frames.0.push(t.frames.into_iter().map(|(f, Rows(r))| (f, Rows(r.iter().map(|r| r.map(|x| x * w)).collect()))).collect());
            l.on_scalars.0.push(t.scalars.into_iter().map(|(s, Col(c))| (s, Col(c.iter().map(|x| x * w).collect()))).collect());
        }
        l
    }

    /// Everything, as one part.
    pub fn everything(&self) -> Component {
        Component {
            frames: (0..self.names.len()).filter(|&f| self.joints.0[f] != Joint::Fixed).collect(),
            scalars: (0..self.scalars.len()).collect(),
            constraints: (0..self.constraints.len()).collect(),
        }
    }

    /// The parts nothing ties together: unknowns that some constraint depends on together are in one part.
    /// (Read off the derivative's blocks: a constraint ties the unknowns its row has blocks for.)
    pub fn components(&self) -> Vec<Component> {
        let frames = self.names.len();
        let mut group: Vec<usize> = (0..frames + self.scalars.len()).collect();
        fn find(group: &mut [usize], mut a: usize) -> usize {
            while group[a] != a {
                group[a] = group[group[a]];
                a = group[a];
            }
            a
        }
        let rows = self.linearised(&self.everything().constraints).rows();
        let unknowns = |r: &Row| r.frames.iter().map(|(f, _)| *f).chain(r.scalars.iter().map(|(s, _)| frames + s)).collect::<Vec<usize>>();
        for r in &rows {
            let u = unknowns(r);
            for pair in u.windows(2) {
                let (a, b) = (find(&mut group, pair[0]), find(&mut group, pair[1]));
                group[a.max(b)] = a.min(b);
            }
        }
        let all = self.everything();
        let mut parts: Vec<(usize, Component)> = vec![];
        let part = |parts: &mut Vec<(usize, Component)>, key: usize| match parts.iter().position(|(k, _)| *k == key) {
            Some(i) => i,
            None => {
                parts.push((key, Component::default()));
                parts.len() - 1
            }
        };
        for &f in &all.frames {
            let i = part(&mut parts, find(&mut group, f));
            parts[i].1.frames.push(f);
        }
        for &s in &all.scalars {
            let i = part(&mut parts, find(&mut group, frames + s));
            parts[i].1.scalars.push(s);
        }
        // Constraints between things that cannot move are a part with no unknowns.
        for r in &rows {
            let key = unknowns(r).first().map_or(usize::MAX, |&u| find(&mut group, u));
            let i = part(&mut parts, key);
            parts[i].1.constraints.push(r.constraint);
        }
        parts.into_iter().map(|(_, c)| c).collect()
    }

    /// What to call each of a part's unknowns, in the order they are laid out flat: each frame's freedoms, then the scalars.
    fn unknowns(&self, part: &Component) -> Vec<String> {
        let frames = part.frames.iter().flat_map(|&f| self.joints.0[f].freedoms().into_iter().map(move |(_, _, what)| format!("{} can {what}", self.names[f])));
        frames.chain(part.scalars.iter().map(|&s| format!("{} can change", self.scalars[s].name))).collect()
    }

    /// A step of a part's unknowns, from its numbers laid out flat.
    fn step_of(&self, part: &Component, flat: &[f64]) -> Step {
        let mut step = Step { frames: vec![None; self.names.len()], scalars: vec![None; self.scalars.len()] };
        let mut at = 0;
        for &f in &part.frames {
            let k = self.joints.0[f].basis().len();
            step.frames[f] = Some(flat[at..at + k].to_vec());
            at += k;
        }
        for &s in &part.scalars {
            step.scalars[s] = Some(flat[at]);
            at += 1;
        }
        step
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
            s.value += d.unwrap_or(0.0);
        }
        s
    }

    /// The residuals of one tier's constraints among these (measured ones in units of their tolerance), stacked.
    fn tier_residual(&self, constraints: &[usize], tier: usize) -> Vec<f64> {
        let world = self.world();
        (constraints.iter().map(|&i| &self.constraints[i]).filter(|c| tier_of(c.tier) == tier)).flat_map(|c| self.residual(&world, &c.kind).into_iter().map(move |r| r * weight(c.tier))).collect()
    }

    /// One tier's rows over a part's unknowns laid out flat, as a small dense matrix, and its residual.
    fn dense(&self, part: &Component, rows: &[Row], tier: usize) -> (Vec<f64>, Mat) {
        let (mut frame_at, mut scalar_at, mut n) = (vec![usize::MAX; self.names.len()], vec![usize::MAX; self.scalars.len()], 0);
        for &f in &part.frames {
            frame_at[f] = n;
            n += self.joints.0[f].basis().len();
        }
        for &s in &part.scalars {
            scalar_at[s] = n;
            n += 1;
        }
        let (mut r, mut j) = (vec![], vec![]);
        for row in rows.iter().filter(|row| tier_of(self.constraints[row.constraint].tier) == tier) {
            let mut lines = zeros(row.residual.len(), n);
            for (f, block) in &row.frames {
                for (line, b) in lines.iter_mut().zip(block) {
                    line[frame_at[*f]..frame_at[*f] + b.len()].copy_from_slice(b);
                }
            }
            for (s, block) in &row.scalars {
                for (line, b) in lines.iter_mut().zip(block) {
                    line[scalar_at[*s]] = *b;
                }
            }
            r.extend(&row.residual);
            j.extend(lines);
        }
        (r, j)
    }

    /// The step the tiers ask of a part, tier by tier, each within what the ones before leave free:
    /// the step, how many freedoms each tier settled, and what is left free (a basis, as columns).
    fn tiered_step(&self, part: &Component, n: usize) -> (Vec<f64>, [usize; 3], Mat) {
        let rows = self.linearised(&part.constraints).rows();
        let (mut dx, mut free, mut ranks) = (vec![0.0; n], linalg::identity(n), [0; 3]);
        for tier in 0..3 {
            let (r, j) = self.dense(part, &rows, tier);
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

    fn hard_gap(&self, constraints: &[usize]) -> f64 {
        self.tier_residual(constraints, 0).iter().map(|x| x * x).sum::<f64>().sqrt()
    }

    /// What the lower tiers cost: the measured, then (far behind) the hints.
    fn soft_cost(&self, constraints: &[usize]) -> f64 {
        let sq = |t: usize| self.tier_residual(constraints, t).iter().map(|x| x * x).sum::<f64>();
        sq(1) + 1e-8 * sq(2)
    }

    /// Whether going from `self` to `next` is no worse for these constraints: judged on the hard
    /// ones while they are not met, and on the rest only once they are.
    fn no_worse(&self, next: &System, constraints: &[usize]) -> bool {
        const MET: f64 = 1e-9;
        let (before, after) = (self.hard_gap(constraints), next.hard_gap(constraints));
        if before > MET {
            after < before
        } else {
            after <= MET && next.soft_cost(constraints) <= self.soft_cost(constraints) * (1.0 + 1e-9) + 1e-20
        }
    }

    /// Solves one part: how many passes it took, how many freedoms each tier settled, and what is open.
    fn solve_part(&mut self, part: &Component) -> (usize, [usize; 3], Vec<String>) {
        let unknowns = self.unknowns(part);
        let n = unknowns.len();
        if n == 0 {
            return (0, [0; 3], vec![]);
        }
        let mut passes = 0;
        for _ in 0..60 {
            passes += 1;
            let (dx, _, _) = self.tiered_step(part, n);
            let size = dx.iter().map(|x| x * x).sum::<f64>().sqrt();
            if size < 1e-11 {
                break;
            }
            // The whole step, or as much of it as does not make things worse.
            let mut taken = false;
            for halving in 0..12 {
                let f = 0.5f64.powi(halving);
                let next = self.stepped(&self.step_of(part, &dx.iter().map(|x| x * f).collect::<Vec<_>>()));
                if self.no_worse(&next, &part.constraints) {
                    *self = next;
                    taken = true;
                    break;
                }
            }
            if !taken {
                break;
            }
        }
        let (_, ranks, free) = self.tiered_step(part, n);
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
        (passes, ranks, open)
    }

    /// Solves for the unknowns, each part on its own (the frames' poses and the scalars are
    /// left at the answer, or, where refused, as near the hard constraints as they get),
    /// and says what settled what.
    pub fn solve(&mut self) -> Report {
        self.solve_parts().0
    }

    /// The same, with the parts and how many of each one's freedoms each tier settled.
    fn solve_parts(&mut self) -> (Report, Vec<Component>, Vec<[usize; 3]>) {
        let parts = self.components();
        let mut settled = vec![];
        let mut report = Report { refused: false, conflicts: vec![], gaps: vec![], passes: 0, components: parts.len(), unknowns: 0, by_hard: 0, by_measured: 0, by_default: 0, open: vec![] };
        for part in &parts {
            let (passes, ranks, open) = self.solve_part(part);
            report.passes = report.passes.max(passes);
            settled.push(ranks);
            report.unknowns += self.unknowns(part).len();
            report.by_hard += ranks[0];
            report.by_measured += ranks[1];
            report.by_default += ranks[2];
            report.open.extend(open);
        }
        let world = self.world();
        let gap = |c: &Constraint| Gap { constraint: c.name.clone(), tier: c.tier, size: self.residual(&world, &c.kind).iter().map(|x| x * x).sum::<f64>().sqrt() };
        report.gaps = self.constraints.iter().map(gap).collect();
        for part in &parts {
            // (A tenth of a micron, or of a microradian.)
            const MET: f64 = 1e-7;
            if self.hard_gap(&part.constraints) <= MET {
                continue;
            }
            let unmet: Vec<usize> = part.constraints.iter().copied().filter(|&i| self.constraints[i].tier == Tier::Hard && report.gaps[i].size > MET).collect();
            let scope = unmet.iter().map(|&i| self.scope(&self.constraints[i].kind).0).reduce(|a, b| self.meet(a, b)).unwrap_or(0);
            let mut constraints: Vec<Gap> = unmet.iter().map(|&i| report.gaps[i].clone()).collect();
            constraints.sort_by(|a, b| b.size.total_cmp(&a.size));
            report.conflicts.push(Conflict { constraints, frames: part.frames.iter().map(|&f| self.names[f].clone()).collect(), scope: self.names[scope].clone() });
        }
        report.refused = !report.conflicts.is_empty();
        (report, parts, settled)
    }

    /// The definition, used here: a frame in `parent` (at `pose` to start with, free as `joint`
    /// says) that stands for the definition's root. What the definition's own hard constraints
    /// settle is not made again: it is the definition's, seen through this frame. What they
    /// leave open is this instance's own: those frames, scalars and constraints are made here,
    /// starting where the definition left them.
    pub fn instance(&mut self, name: &str, parent: FrameId, joint: Joint, pose: Pose, of: &Definition) -> Instance {
        let d = &of.system;
        let root = self.frame(name, parent, joint, pose);
        let mut frames: Vec<(FrameId, Pose)> = vec![];
        for f in 0..d.names.len() {
            let at = match d.poses.parents()[f] {
                None => (root, Pose::identity()),
                Some(p) => {
                    let (anchor, from) = frames[p];
                    let pose = from.then(&d.poses.blocks()[f]);
                    if of.settled_frames[f] {
                        (anchor, pose)
                    } else {
                        (self.frame(&format!("{name}/{}", d.names[f]), anchor, d.joints.0[f], pose), Pose::identity())
                    }
                }
            };
            frames.push(at);
        }
        let scalars = (d.scalars.iter().enumerate())
            .map(|(i, s)| if of.settled_scalars[i] { Err(s.value) } else { Ok(self.scalar(frames[s.owner].0, &format!("{name}/{}", s.name), s.value)) })
            .collect();
        let instance = Instance { frame: root, frames, scalars, own: (0..d.names.len()).map(|f| f != 0 && !of.settled_frames[f]).collect() };
        for c in of.open.iter().map(|&i| &d.constraints[i]) {
            self.constrain(&format!("{name}/{}", c.name), c.tier, instance.kind(&c.kind));
        }
        instance
    }
}

/// A system made to be used more than once: solved once, on its own, with its world as its root.
#[derive(Debug, Clone)]
pub struct Definition {
    system: System,
    report: Report,
    /// Whether each frame's joint, and each scalar, is settled by the definition's own hard constraints.
    settled_frames: Vec<bool>,
    settled_scalars: Vec<bool>,
    /// The constraints of the parts that are not: each instance has these for itself.
    open: Vec<usize>,
}

impl Definition {
    /// Solves the system; it is refused (with why) if its hard constraints cannot all hold.
    ///
    /// A part of it is settled when its hard constraints alone decide all of it. A part closed
    /// only by measurements or hints is not: something outside, of a higher tier, may yet decide it otherwise.
    pub fn new(mut system: System) -> Result<Definition, Report> {
        let (report, parts, ranks) = system.solve_parts();
        if report.refused {
            return Err(report);
        }
        let (mut settled_frames, mut settled_scalars, mut open) = (vec![true; system.names.len()], vec![true; system.scalars.len()], vec![]);
        for (part, ranks) in parts.iter().zip(&ranks) {
            if ranks[0] < system.unknowns(part).len() {
                part.frames.iter().for_each(|&f| settled_frames[f] = false);
                part.scalars.iter().for_each(|&s| settled_scalars[s] = false);
                open.extend(&part.constraints);
            }
        }
        Ok(Definition { system, report, settled_frames, settled_scalars, open })
    }

    /// The definition, solved.
    pub fn system(&self) -> &System {
        &self.system
    }

    /// What its own solve found: said once, for the definition, not for each use of it.
    pub fn report(&self) -> &Report {
        &self.report
    }
}

/// A use of a definition: things of the definition, as they are in the system that uses it.
#[derive(Debug, Clone)]
pub struct Instance {
    /// The frame that stands for the definition's root.
    pub frame: FrameId,
    /// For each of the definition's frames: the frame here that it is rigid with, and its pose in that.
    frames: Vec<(FrameId, Pose)>,
    /// For each of the definition's scalars: this instance's own, or the value the definition settled.
    scalars: Vec<Result<ScalarId, f64>>,
    own: Vec<bool>,
}

impl Instance {
    /// This instance's own copy of one of the definition's scalars, if the definition leaves it open.
    pub fn scalar(&self, s: ScalarId) -> Option<ScalarId> {
        self.scalars[s.0].ok()
    }

    /// This instance's own copy of one of the definition's frames, if the definition leaves its joint open.
    pub fn own_frame(&self, f: FrameId) -> Option<FrameId> {
        self.own[f.0].then(|| self.frames[f.0].0)
    }

    /// A point of the definition, here.
    pub fn point(&self, p: &Point) -> Point {
        let (frame, from) = self.frames[p.frame.0];
        let mut at = p.at;
        let mut along = vec![];
        for (s, u) in &p.along {
            match self.scalars[s.0] {
                Ok(own) => along.push((own, from.rotate(*u))),
                Err(value) => at = add(at, scale(*u, value)),
            }
        }
        Point { frame, at: from.apply(at), along }
    }

    /// A direction of the definition, here.
    pub fn dir(&self, d: &Dir) -> Dir {
        let (frame, from) = self.frames[d.0 .0];
        Dir(frame, from.rotate(d.1))
    }

    /// A number of the definition, here.
    pub fn num(&self, n: &Num) -> Num {
        let mut out = Num::from(n.constant);
        for (s, k) in &n.terms {
            match self.scalars[s.0] {
                Ok(own) => out.terms.push((own, *k)),
                Err(value) => out.constant += k * value,
            }
        }
        out
    }

    /// A constraint of the definition, here.
    pub fn kind(&self, k: &Kind) -> Kind {
        match k {
            Kind::Coincident(a, b) => Kind::Coincident(self.point(a), self.point(b)),
            Kind::Distance(a, b, d) => Kind::Distance(self.point(a), self.point(b), self.num(d)),
            Kind::Offset(a, b, along, d) => Kind::Offset(self.point(a), self.point(b), self.dir(along), self.num(d)),
            Kind::Aligned(a, b) => Kind::Aligned(self.dir(a), self.dir(b)),
            Kind::Parallel(a, b) => Kind::Parallel(self.dir(a), self.dir(b)),
            Kind::Perpendicular(a, b) => Kind::Perpendicular(self.dir(a), self.dir(b)),
            Kind::Equal(a, b) => Kind::Equal(self.num(a), self.num(b)),
        }
    }
}

/// Which sense of `a` is nearer `b`: 1 as it is, -1 turned about.
fn sense(a: V3, b: V3) -> f64 {
    if dot(a, b) < 0.0 {
        -1.0
    } else {
        1.0
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

    const WEST: V3 = [0.0, 3.0, 2.477];
    const EAST: V3 = [6.0, 3.0, 2.439];

    /// A truss made in its own frame (x along its span, y up, in the plane z = 0), with absolute
    /// values and nothing free inside it. The model that uses it adds two points on its bottom
    /// chord, each on a plate's bearing point in the world (the plates at different heights).
    fn a_truss(s: &mut System, name: &str, y: f64) -> (FrameId, ScalarId, ScalarId) {
        // Its nominal place: somewhere near, and not square to anything.
        let truss = s.frame(name, WORLD, Joint::Free, Pose::at([0.3, y - 2.8, 2.0]).moved([0.0; 3], [0.2, -0.1, 0.15]));
        let (heel, far) = ([0.0, 0.0, 0.0], [7.0, 0.0, 0.0]);
        let s1 = s.scalar(truss, &format!("where the {name} bears on the west plate"), 0.7);
        let s2 = s.scalar(truss, &format!("where the {name} bears on the east plate"), 6.3);
        s.constrain(&format!("{name} on the west plate"), Tier::Hard, Kind::Coincident(Point::along(truss, heel, far, s1), Point::at(WORLD, [WEST[0], y, WEST[2]])));
        s.constrain(&format!("{name} on the east plate"), Tier::Hard, Kind::Coincident(Point::along(truss, heel, far, s2), Point::at(WORLD, [EAST[0], y, EAST[2]])));
        (truss, s1, s2)
    }

    fn truss_on_two_plates() -> (System, FrameId, ScalarId, ScalarId) {
        let mut s = System::new();
        let (truss, s1, s2) = a_truss(&mut s, "truss", 3.0);
        (s, truss, s1, s2)
    }

    /// Frames three deep on every kind of joint, with every kind of constraint in every tier, nowhere near solved.
    fn a_bit_of_everything() -> System {
        let (mut s, truss, s1, s2) = truss_on_two_plates();
        let wall = s.frame("wall", WORLD, Joint::Hinge([0.0, 0.0, 1.0]), Pose::at([1.0, -2.0, 0.5]).moved([0.0; 3], [0.3, 0.2, -0.4]));
        let sketch = s.frame("sketch", wall, Joint::Slider([1.0, 2.0, 0.0]), Pose::at([0.2, 0.4, 1.0]).moved([0.0; 3], [-0.5, 0.1, 0.2]));
        let corner = s.frame("corner", sketch, Joint::Planar([0.0, 0.0, 1.0]), Pose::at([3.5, 2.5, 0.0]).moved([0.0; 3], [0.0, 0.0, 0.7]));
        let peg = s.frame("peg", sketch, Joint::Free, Pose::at([0.5, -1.0, 0.3]).moved([0.0; 3], [0.4, -0.2, 0.1]));
        let rider = s.frame("rider", truss, Joint::Free, Pose::at([1.0, 0.3, 0.2]).moved([0.0; 3], [0.1, 0.5, -0.3]));
        let width = s.scalar(sketch, "width", 1.5);
        s.constrain("a", Tier::Hard, Kind::distance(Point::at(corner, [0.1, 0.2, 0.3]), Point::at(truss, [0.0; 3]).plus(s1, [0.9, 0.1, 0.0]).plus(s2, [0.0, 0.2, 0.1]), 2.0));
        s.constrain("b", Tier::Hard, Kind::offset(Point::at(rider, [0.4, 0.0, 0.1]), Point::at(corner, [0.0, 1.0, 0.0]), Dir(sketch, [0.0, 1.0, 2.0]), Num::from(0.3).plus(width, 0.5)));
        s.constrain("c", Tier::Measured { sigma: 0.01 }, Kind::Parallel(Dir(rider, [0.0, 1.0, 0.0]), Dir(corner, [1.0, 0.0, 1.0])));
        s.constrain("d", Tier::Measured { sigma: 0.2 }, Kind::Coincident(Point::at(wall, [0.5, 0.5, 0.5]), Point::along(rider, [0.0; 3], [0.0, 0.0, 1.0], s2)));
        s.constrain("e", Tier::Default, Kind::Perpendicular(Dir(wall, [1.0, 0.0, 0.0]), Dir(rider, [0.0, 0.0, 1.0])));
        s.constrain("f", Tier::Default, Kind::equal(s2, 1.0));
        // Within the sketch: nothing above it bears on these.
        s.constrain("g", Tier::Default, Kind::distance(Point::at(corner, [0.0; 3]), Point::at(peg, [1.0, 0.0, 0.0]), width));
        s.constrain("h", Tier::Hard, Kind::offset(Point::at(sketch, [0.0; 3]), Point::at(peg, [0.0, 0.2, 0.0]), Dir(corner, [1.0, 0.0, 0.0]), 0.4));
        s.constrain("i", Tier::Measured { sigma: 0.05 }, Kind::Parallel(Dir(peg, [0.0, 0.0, 1.0]), Dir(sketch, [0.0, 1.0, 1.0])));
        s.constrain("j", Tier::Default, Kind::Aligned(Dir(peg, [1.0, 0.0, 1.0]), Dir(corner, [0.0, 1.0, 0.0])));
        s.constrain("k", Tier::Measured { sigma: 0.1 }, Kind::Aligned(Dir(rider, [1.0, 0.0, 0.0]), Dir(WORLD, [0.0, 0.0, 1.0])));
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
    fn the_rows_are_the_change_of_the_residual_along_each_unknown() {
        let s = a_bit_of_everything();
        let all = s.everything();
        let n = s.unknowns(&all).len();
        assert_eq!(n, 6 + 1 + 1 + 3 + 6 + 6 + 3);
        let rows = s.linearised(&all.constraints).rows();
        const H: f64 = 1e-6;
        for tier in 0..3 {
            let (r, j) = s.dense(&all, &rows, tier);
            assert_eq!(r, s.tier_residual(&all.constraints, tier));
            for k in 0..n {
                let at = |h: f64| {
                    let mut dx = vec![0.0; n];
                    dx[k] = h;
                    s.stepped(&s.step_of(&all, &dx)).tier_residual(&all.constraints, tier)
                };
                let (hi, lo) = (at(H), at(-H));
                for i in 0..r.len() {
                    let by_differences = (hi[i] - lo[i]) / (2.0 * H);
                    assert!((j[i][k] - by_differences).abs() < 1e-6 * (1.0 + by_differences.abs()), "tier {tier}, residual {i}, unknown {k} ({}): {} against {by_differences}", s.unknowns(&all)[k], j[i][k]);
                }
            }
        }
    }

    #[test]
    fn a_row_has_blocks_only_for_what_is_inside_its_scope() {
        let s = a_bit_of_everything();
        let rows = s.linearised(&s.everything().constraints).rows();
        let blocks = |name: &str| -> Vec<&str> {
            let i = s.constraints.iter().position(|c| c.name == name).unwrap();
            rows[i].frames.iter().map(|(f, _)| s.names[*f].as_str()).collect()
        };
        // Between the sketch and things in it: the wall's hinge and the sketch's own slide move them all together.
        assert_eq!(blocks("g"), ["corner", "peg"]);
        assert_eq!(blocks("h"), ["corner", "peg"]);
        assert_eq!(blocks("i"), ["peg"]);
        // Across to the truss: everything from each end up to the world.
        assert_eq!(blocks("a"), ["truss", "wall", "sketch", "corner"]);
    }

    #[test]
    fn the_transpose_is_the_transpose_and_the_rows_are_the_operator() {
        let s = a_bit_of_everything();
        let all = s.everything();
        let n = s.unknowns(&all).len();
        let l = s.linearised(&all.constraints);
        let x = s.step_of(&all, &noise(n, 7));
        let mut ys = noise(l.residual.iter().map(Vec::len).sum(), 99).into_iter();
        let y: BlockCol<Vec<f64>> = l.residual.iter().map(|r| Some(r.iter().map(|_| ys.next().unwrap()).collect())).collect();
        let dots = |a: &BlockCol<Vec<f64>>, b: &BlockCol<Vec<f64>>| -> f64 { a.iter().zip(b).map(|(a, b)| a.iter().flatten().zip(b.iter().flatten()).map(|(a, b)| a * b).sum::<f64>()).sum() };
        // <y, J x> = <Jᵀ y, x>, for any x and y.
        let (jx, jty) = (l.apply(&x), l.apply_transpose(&y));
        let unknowns = dots(&jty.frames, &x.frames) + jty.scalars.iter().zip(&x.scalars).map(|(a, b)| a.unwrap_or(0.0) * b.unwrap_or(0.0)).sum::<f64>();
        assert!((dots(&y, &jx) - unknowns).abs() < 1e-9 * (1.0 + unknowns.abs()), "{} against {unknowns}", dots(&y, &jx));
        // Each row, written out, times x, is what the operator gives.
        for (row, by_operator) in l.rows().iter().zip(&jx) {
            for (i, by_operator) in by_operator.as_ref().unwrap().iter().enumerate() {
                let frames: f64 = row.frames.iter().map(|(f, b)| b[i].iter().zip(x.frames[*f].as_ref().unwrap()).map(|(a, b)| a * b).sum::<f64>()).sum();
                let scalars: f64 = row.scalars.iter().map(|(s, b)| b[i] * x.scalars[*s].unwrap()).sum();
                assert!((frames + scalars - by_operator).abs() < 1e-9, "{}: {} against {by_operator}", s.constraints[row.constraint].name, frames + scalars);
            }
        }
    }

    #[test]
    fn a_constraint_is_within_the_nearest_frame_that_holds_all_it_touches() {
        let s = a_bit_of_everything();
        let scope = |name: &str| s.name(s.scope(&s.constraints.iter().find(|c| c.name == name).unwrap().kind)).to_string();
        assert_eq!(scope("a"), "world"); // corner (in sketch, in wall) and truss
        assert_eq!(scope("e"), "world");
        assert_eq!(scope("g"), "sketch"); // corner, peg, and the sketch's width
        assert_eq!(scope("i"), "sketch");
        assert_eq!(scope("f"), "truss"); // a scalar of the truss, alone
    }

    #[test]
    fn two_bearings_leave_a_slide_and_a_roll() {
        let (mut s, ..) = truss_on_two_plates();
        let r = s.solve();
        assert!(!r.refused, "{r:#?}");
        assert_eq!((r.unknowns, r.by_hard, r.open.len(), r.components), (8, 6, 2, 1), "{r:#?}");
    }

    #[test]
    fn a_hint_and_a_datum_settle_the_rest_and_the_truss_tilts() {
        let (mut s, truss, s1, _) = truss_on_two_plates();
        // The generator's hint: its y is up. A datum: its heel 0.1 m outside the west bearing.
        s.constrain("the truss stands up", Tier::Default, Kind::Parallel(Dir(truss, [0.0, 1.0, 0.0]), Dir(WORLD, [0.0, 0.0, 1.0])));
        s.constrain("heel overhang", Tier::Hard, Kind::distance(Point::at(truss, [0.0; 3]), Point::at(WORLD, WEST), 0.1));
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
        s.constrain("heel overhang", Tier::Hard, Kind::equal(s1, 0.1));
        // A reading that the far end of the chord is at z = 2.40; the bearings put it lower.
        s.constrain("far end, from the scan", Tier::Measured { sigma: 0.005 }, Kind::offset(Point::at(WORLD, [0.0; 3]), Point::at(truss, [7.0, 0.0, 0.0]), Dir(WORLD, [0.0, 0.0, 1.0]), 2.40));
        let r = s.solve();
        assert!(!r.refused, "{r:#?}");
        let g = r.gaps.iter().find(|g| g.constraint == "far end, from the scan").unwrap();
        assert!(g.size > 0.02 && g.size < 0.04, "the reading is not met, and its gap is known: {g:?}");
    }

    #[test]
    fn hard_constraints_that_cannot_all_hold_are_refused_and_named() {
        let (mut s, _, s1, s2) = truss_on_two_plates();
        // The bearings are 6 m apart; these put them 7 m apart along the chord.
        s.constrain("bears at its heel", Tier::Hard, Kind::equal(s1, 0.0));
        s.constrain("bears at its far end", Tier::Hard, Kind::equal(s2, 7.0));
        let r = s.solve();
        assert!(r.refused && r.conflicts.len() == 1, "{r:#?}");
        // All four are in it (any one released would let the rest hold), and between them they are a metre out.
        let conflict = &r.conflicts[0];
        let names: Vec<&str> = conflict.constraints.iter().map(|g| g.constraint.as_str()).collect();
        for n in ["truss on the west plate", "truss on the east plate", "bears at its heel", "bears at its far end"] {
            assert!(names.contains(&n), "{names:?}");
        }
        let total: f64 = conflict.constraints.iter().map(|g| g.size).sum();
        assert!((total - 1.0).abs() < 0.01, "a metre to share: {conflict:#?}");
        assert_eq!((conflict.frames.as_slice(), conflict.scope.as_str()), (&["truss".to_string()][..], "world"));
    }

    #[test]
    fn what_nothing_ties_together_is_solved_apart_and_a_conflict_stays_where_it_is() {
        // Three trusses on the same plates: nothing ties one to another.
        let mut s = System::new();
        let trusses: Vec<_> = (0..3).map(|i| a_truss(&mut s, &format!("truss {i}"), 3.0 + 0.6 * i as f64)).collect();
        // A wall that nothing asks anything of, and a constraint between things that cannot move.
        s.frame("wall", WORLD, Joint::Slider([1.0, 0.0, 0.0]), Pose::identity());
        s.constrain("the plates are 6 m apart on plan", Tier::Hard, Kind::offset(Point::at(WORLD, WEST), Point::at(WORLD, EAST), Dir(WORLD, [1.0, 0.0, 0.0]), 6.0));
        let parts = s.components();
        assert_eq!(parts.len(), 5, "{parts:#?}");
        assert_eq!(parts.iter().map(|p| (p.frames.len(), p.scalars.len(), p.constraints.len())).collect::<Vec<_>>(), [(1, 2, 2), (1, 2, 2), (1, 2, 2), (1, 0, 0), (0, 0, 1)]);
        // The middle one is asked the impossible.
        s.constrain("bears at its heel", Tier::Hard, Kind::equal(trusses[1].1, 0.0));
        s.constrain("bears at its far end", Tier::Hard, Kind::equal(trusses[1].2, 7.0));
        let r = s.solve();
        assert!(r.refused && r.conflicts.len() == 1 && r.components == 5, "{r:#?}");
        assert_eq!(r.conflicts[0].frames, ["truss 1"]);
        assert!(r.conflicts[0].constraints.iter().all(|g| g.constraint.contains("truss 1") || g.constraint.starts_with("bears")), "{r:#?}");
        // The others are on their bearings all the same.
        for i in [0, 2] {
            let on = s.point(&Point::along(trusses[i].0, [0.0; 3], [7.0, 0.0, 0.0], trusses[i].1));
            assert!(norm(sub(on, [WEST[0], 3.0 + 0.6 * i as f64, WEST[2]])) < 1e-8, "{on:?}");
        }
        // And what is open is said for each: their slide and roll (the one in conflict, its roll), and the wall.
        assert_eq!(r.open.len(), 2 + 1 + 2 + 1, "{r:#?}");
    }

    #[test]
    fn a_generator_with_a_parameter_is_sized_by_what_it_is_fixed_to() {
        // A truss whose span is a parameter: its far heel is at (span, 0, 0). Both heels on bearings.
        let mut s = System::new();
        let truss = s.frame("truss", WORLD, Joint::Free, Pose::at([0.2, 2.5, 2.0]));
        let span = s.scalar(truss, "span", 5.0);
        s.constrain("near heel on the west plate", Tier::Hard, Kind::Coincident(Point::at(truss, [0.0; 3]), Point::at(WORLD, WEST)));
        s.constrain("far heel on the east plate", Tier::Hard, Kind::Coincident(Point::at(truss, [0.0; 3]).plus(span, [1.0, 0.0, 0.0]), Point::at(WORLD, EAST)));
        s.constrain("the truss stands up", Tier::Default, Kind::Parallel(Dir(truss, [0.0, 1.0, 0.0]), Dir(WORLD, [0.0, 0.0, 1.0])));
        let r = s.solve();
        assert!(!r.refused && r.open.is_empty(), "{r:#?}");
        assert!((s.value(span) - (36.0f64 + 0.038 * 0.038).sqrt()).abs() < 1e-9, "{}", s.value(span));
    }

    #[test]
    fn a_definition_is_solved_once_and_what_it_settles_is_not_made_again() {
        // A 3-4-5 corner in a sketch, its turn settled too: nothing in it is open.
        let mut d = System::new();
        let corner = d.frame("corner", WORLD, Joint::Planar([0.0, 0.0, 1.0]), Pose::at([3.5, 2.5, 0.0]));
        d.constrain("5 from the origin", Tier::Hard, Kind::distance(Point::at(WORLD, [0.0; 3]), Point::at(corner, [0.0; 3]), 5.0));
        d.constrain("3 up", Tier::Hard, Kind::offset(Point::at(WORLD, [0.0; 3]), Point::at(corner, [0.0; 3]), Dir(WORLD, [0.0, 1.0, 0.0]), 3.0));
        d.constrain("square to the sketch", Tier::Hard, Kind::Parallel(Dir(corner, [1.0, 0.0, 0.0]), Dir(WORLD, [1.0, 0.0, 0.0])));
        let d = Definition::new(d).unwrap();
        assert!(d.report().open.is_empty() && d.report().by_hard == 3, "{:#?}", d.report());
        // Used twice, each through its own transform: each use is one frame, with nothing to solve inside.
        let mut s = System::new();
        let poses = [Pose::at([1.0, 2.0, 3.0]).moved([0.0; 3], [0.4, 0.0, 0.0]), Pose::at([-2.0, 0.5, 1.0]).moved([0.0; 3], [0.0, 0.3, 1.1])];
        let uses = poses.map(|p| s.instance("sketch", WORLD, Joint::Fixed, p, &d));
        assert_eq!((s.names.len(), s.scalars.len(), s.constraints.len()), (3, 0, 0));
        for (u, p) in uses.iter().zip(&poses) {
            assert_eq!(u.own_frame(corner), None);
            // A point 1 along the corner's x, by way of the definition.
            let at = s.point(&u.point(&Point::at(corner, [1.0, 0.0, 0.0])));
            assert!(norm(sub(at, p.apply([5.0, 3.0, 0.0]))) < 1e-8, "{at:?}");
        }
        // A definition whose hard constraints cannot all hold is refused, once, as itself.
        let mut bad = System::new();
        let x = bad.scalar(WORLD, "x", 0.0);
        bad.constrain("x is 1", Tier::Hard, Kind::equal(x, 1.0));
        bad.constrain("x is 2", Tier::Hard, Kind::equal(x, 2.0));
        assert!(Definition::new(bad).is_err_and(|r| r.refused));
    }

    #[test]
    fn what_a_definition_leaves_open_is_each_instance_s_own() {
        // A truss with a span (its far heel at span along x), 7 m unless something says otherwise,
        // and a king post that is settled: its top at half of 7 m along and 1.2 up, whatever the span.
        let mut d = System::new();
        let span = d.scalar(WORLD, "span", 6.5);
        let rise = d.scalar(WORLD, "rise", 0.0);
        d.constrain("span, unless told", Tier::Default, Kind::equal(span, 7.0));
        d.constrain("rise", Tier::Hard, Kind::equal(rise, 1.2));
        let far = Point::at(WORLD, [0.0; 3]).plus(span, [1.0, 0.0, 0.0]);
        let top = Point::at(WORLD, [3.5, 0.0, 0.0]).plus(rise, [0.0, 1.0, 0.0]);
        let d = Definition::new(d).unwrap();
        assert!((d.system().value(span) - 7.0).abs() < 1e-12 && d.report().by_default == 1);

        // Three uses: on bearings 6 m apart, on bearings 5 m apart, and on one bearing only.
        let mut s = System::new();
        let mut uses = vec![];
        for (i, apart) in [Some(6.0), Some(5.0), None].into_iter().enumerate() {
            let y = i as f64;
            let u = s.instance(&format!("truss {i}"), WORLD, Joint::Free, Pose::at([0.1, y + 0.2, 2.1]), &d);
            s.constrain(&format!("truss {i} near heel"), Tier::Hard, Kind::Coincident(u.point(&Point::at(WORLD, [0.0; 3])), Point::at(WORLD, [0.0, y, 2.4])));
            if let Some(apart) = apart {
                s.constrain(&format!("truss {i} far heel"), Tier::Hard, Kind::Coincident(u.point(&far), Point::at(WORLD, [apart, y, 2.4])));
            }
            s.constrain(&format!("truss {i} stands up"), Tier::Default, Kind::Aligned(Dir(u.frame, [0.0, 1.0, 0.0]), Dir(WORLD, [0.0, 0.0, 1.0])));
            uses.push(u);
        }
        // Each has a span of its own, with the definition's hint; none has a rise (it is the definition's).
        assert_eq!((s.names.len(), s.scalars.len()), (4, 3));
        assert!(s.constraints.iter().any(|c| c.name == "truss 1/span, unless told") && !s.constraints.iter().any(|c| c.name.ends_with("/rise")));
        let r = s.solve();
        // (The third's span is tied to nothing but its hint: a part of its own.)
        assert!(!r.refused && r.components == 4, "{r:#?}");
        // Standing up settles each one's roll, though they start lying flat; the third can still turn about its one bearing.
        assert_eq!(r.open, ["truss 2 can turn about its y"], "{r:#?}");
        let spans: Vec<f64> = uses.iter().map(|u| s.value(u.scalar(span).unwrap())).collect();
        // The bearings size the first two (the hint gives way, and says by how much); the hint sizes the third.
        assert!((spans[0] - 6.0).abs() < 1e-9 && (spans[1] - 5.0).abs() < 1e-9 && (spans[2] - 7.0).abs() < 1e-9, "{spans:?}");
        let gap = r.gaps.iter().find(|g| g.constraint == "truss 1/span, unless told").unwrap();
        assert!((gap.size - 2.0).abs() < 1e-9, "{gap:?}");
        // The king post's top is the definition's, in each: 3.5 along and 1.2 up from the near heel.
        assert_eq!(uses[0].scalar(rise), None);
        let at = s.point(&uses[0].point(&top));
        assert!(norm(sub(at, [3.5, 0.0, 3.6])) < 1e-7, "{at:?}");
    }

    #[test]
    fn parallel_pulls_to_the_nearer_sense_from_anywhere() {
        // A frame on a hinge about x, its y to be parallel to the world's z: from exactly square to
        // it (where the angle between them is 90°), from just past, and from nearly there.
        for (start, y_dot_z) in [(0.0, 1.0), (-0.2, -1.0), (1.4, 1.0), (3.0, 1.0), (-3.0, -1.0)] {
            let mut s = System::new();
            let f = s.frame("leaf", WORLD, Joint::Hinge([1.0, 0.0, 0.0]), Pose::identity().moved([0.0; 3], [start, 0.0, 0.0]));
            s.constrain("parallel", Tier::Hard, Kind::Parallel(Dir(f, [0.0, 1.0, 0.0]), Dir(WORLD, [0.0, 0.0, 1.0])));
            let r = s.solve();
            assert!(!r.refused && r.open.is_empty(), "from {start}: {r:#?}");
            assert!((s.dir(&Dir(f, [0.0, 1.0, 0.0]))[2] - y_dot_z).abs() < 1e-9, "from {start}");
        }
    }

    #[test]
    fn a_sketch_is_a_frame_whose_things_are_in_its_plane() {
        // A point free to move in a sketch's plane (a child frame on a planar joint), held 5 from the
        // origin and level with a point at height 3: a 3-4-5 triangle, in a sketch that is itself tilted.
        let mut s = System::new();
        let sketch = s.frame("sketch", WORLD, Joint::Fixed, Pose::at([1.0, 2.0, 3.0]).moved([0.0; 3], [0.4, 0.0, 0.0]));
        let p = s.frame("corner", sketch, Joint::Planar([0.0, 0.0, 1.0]), Pose::at([3.5, 2.5, 0.0]));
        s.constrain("5 from the origin", Tier::Hard, Kind::distance(Point::at(sketch, [0.0; 3]), Point::at(p, [0.0; 3]), 5.0));
        s.constrain("3 up the sketch", Tier::Hard, Kind::offset(Point::at(sketch, [0.0; 3]), Point::at(p, [0.0; 3]), Dir(sketch, [0.0, 1.0, 0.0]), 3.0));
        let r = s.solve();
        // Its turning in the plane is nothing anyone asked about: it is reported as open.
        assert!(!r.refused && r.by_hard == 2 && r.open.len() == 1 && r.open[0].contains("turn"), "{r:#?}");
        let local = s.in_parent(p).t;
        assert!((local[0] - 4.0).abs() < 1e-8 && (local[1] - 3.0).abs() < 1e-8 && local[2].abs() < 1e-12, "{local:?}");
    }
}
