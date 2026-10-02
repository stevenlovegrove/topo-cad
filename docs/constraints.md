# Frames and constraints

How things are placed relative to each other, stated generically (building
framing is the first use, not the only one). This is the design agreed in
discussion and a first implementation of it, `crates/topo-solve`. Its first
use in the model is seating a roof's trusses on what they bear on (see
"In the model" below); [topology-audit.md](topology-audit.md) says why it
is wanted, and "Not yet" what is left.

## The model

- **Frames are first class.** A frame is a coordinate system placed in a
  parent frame. Its **joint** says which of its six freedoms are open:
  fixed, free, slider, hinge, planar.
- **Geometry is made in a frame.** A generator (a truss, a wall, a sketch)
  makes its vertices in its own frame, as plain numbers or as parameters. A
  generator with absolute values has nothing free inside it: its frame is a
  rigid body.
- **Anything outside refers to it through the frames**, never by a world
  coordinate.
- **Scalars are separate unknowns**: how far along a member a point is, a
  span, a spacing. Each belongs to a frame (a parameter of what is made
  there). A point is an expression, not an unknown: coordinates in a
  frame, plus so much of each of some scalars along directions of that
  frame. A number (a distance to hold, say) is likewise a constant plus so
  much of some scalars. So a generator with a `span` puts its far heel at
  `span` along x, and whatever it is fixed to sizes it.
- **A constraint relates points and directions in whatever frames**:
  coincident, distance, offset along a direction, aligned (the same
  direction), parallel (either sense: held as aligned in whichever sense
  is nearer), perpendicular, two numbers equal.
- **A sketch is not a special thing**: it is a frame whose contents are
  held to its plane (a planar joint, or `z = 0`).

## Tiers

A constraint has a tier, and the tiers are solved in order, each only
within what the ones above leave free:

1. **Hard**: it must hold. Connections, rigid shapes, datums.
2. **Measured**: it should hold, to a stated tolerance. A reading from a
   scan or a tape.
3. **Default**: a generator's hint ("my y is up"), or a layout rule. It
   decides only what nothing above decides.

Solving in order, not with small weights, means a hint cannot bend a
measurement and a measurement cannot bend a hard constraint, and the
problem stays well conditioned. Whatever is still free after the three
stays where it was put (its nominal place).

A generator is given its parent scope, so it can add constraints and hints
between its frame and what it is in (and named things further up: gravity,
north, a survey datum).

## What the solve reports

- **What settled each freedom**: how many of the unknowns the hard
  constraints, the measurements and the hints each decided.
- **What is still open**, each freedom in words ("truss can turn about its
  x", "where the truss bears on the west plate can change, together with:
  truss can move along its x").
- **Every constraint's gap.** A measurement or a hint that gives way to
  something above it is not an error; its gap is the finding.
- **Refusal.** Hard constraints that cannot all hold are refused: nothing
  downstream may use the result. The report lists the hard constraints not
  met, with their gaps, at the pose that comes nearest to meeting them all.
  That is the simplest feedback, to be tried before anything cleverer
  (releasing one suspect at a time and showing the gap there).

## The worked example

A truss in its own frame, absolute values, nothing free inside. The model
that uses it adds two points on its bottom chord (two scalars) and says
each is on a plate vertex in another frame:

- unknowns: 6 (the truss's pose) + 2 = 8;
- two coincidences: 6 settled;
- 2 left open, and named: sliding along its span, and rolling about the
  chord.

A hint that its y is up settles the roll (plumb); a datum (its heel so far
outside a bearing) settles the slide. With the plates at different heights
the truss tilts in its plane as the bearings say, the hint gives way by
exactly that angle, and that gap is reported. These are the crate's tests.

## The structure of the problem

Everything is a block, one to a frame (or to a scalar, or to a
constraint), and a few sparse operators between them
(`crates/topo-solve/src/blocks.rs`). The conventions are the `robot`
crate's, so the two can share one implementation:

- **Blocks are not matrices.** A block says how it multiplies and how its
  transpose does (`BlockAct`, `BlockActTranspose`), and is stored however
  suits it. A frame's pose is a rotation and a translation, and
  "multiplying" by it is a composition; a step along a joint is a
  retraction, so a pose stays a pose.
- **The topology is a matrix, not a walk of a tree** (`ParentChild`): unit
  lower triangular, with one block per frame in its parent's column.
  Frames are numbered parents first. Solving with it is a forward
  substitution, and with its transpose a back substitution.
- An absent block is zero.

| What | Shape | Block |
| --- | --- | --- |
| Where each frame is in its parent | topology | a pose; world poses are its products from the root |
| How motions carry | the same topology | the pose as it carries a twist to the child's axes |
| Joints | block diagonal | the open freedoms, as unit twists |
| A constraint on frames | a sparse row | per frame touched: each residual against that frame's twist |
| A constraint on scalars | a sparse row | per scalar touched: each residual against it |

A twist is a frame's small motion in its own axes, turn first. The
unknowns are a block per frame (its joint's coordinates) and a number per
scalar; a change of them is a `Step`, not a place.

The derivative of the residuals is an operator, the product

    J = on_frames · carry⁻¹ · joints + on_scalars

applied either way (`Linearised::apply`, `apply_transpose`): joint
coordinates become twists, each frame's twist gathers its ancestors' by
substitution through the topology, and each constraint reads the twists of
the frames it touches. Each constraint's blocks are written out
analytically. Tests hold this up: the rows against central differences
taken along the retraction, `<y, J x> = <Jᵀ y, x>`, and the rows against
the operator.

## Scopes and components

A constraint's **scope** is the nearest frame that holds every frame it
touches and every scalar it reads (`System::scope`). Nothing outside its
scope bears on it: moving the scope, or anything above it, moves all it
touches together. This is made exact, not approximate:

- a residual that is a vector (coincident, parallel) is read in the
  scope's axes, not the world's;
- a constraint's row, written out (`Linearised::row`), has blocks only for
  joints strictly inside its scope and for its scalars.

So a sketch's constraints are rows over the sketch's own unknowns
whatever the sketch is in, and the same sketch placed twice has the same
rows.

Unknowns that some constraint depends on together are tied; the parts
that nothing ties are **components** (`System::components`), read off the
rows' blocks. Each is solved on its own: forty trusses on fixed plates
are forty problems of eight unknowns, not one of three hundred and
twenty. A conflict is reported for its component: the hard constraints
not met, the frames that move with them, and the scope they share. The
rest of the system is solved all the same.

## Definitions and instances

A **definition** is a system made to be used more than once
(`Definition::new`): its world is its root, and it is solved once, on its
own. Its report (what settled what, what is open) is the definition's,
said once; one whose hard constraints cannot all hold is refused there,
not at each use.

An **instance** (`System::instance`) is a frame in the system that uses
it, standing for the definition's root: the same subgraph, through
another transform. Things of the definition are referred to through the
instance (`Instance::point`, `dir`, `num`).

- **What the definition's hard constraints settle is not made again.** A
  frame they settle is rigid with the instance's frame (or with an open
  frame it hangs from); a scalar they settle is a constant. A definition
  with nothing open is one frame per use, with nothing to solve inside.
- **What they leave open is each instance's own**: those frames and
  scalars are made in the using system, starting where the definition
  left them, with the constraints of those parts. Two instances share a
  value only if the user ties them to a scalar of the parent.
- **A part closed only by measurements or hints counts as open.**
  Something outside, of a higher tier, may yet decide it otherwise: a
  truss's "7 m unless told" span gives way to the bearings it is put on,
  and its gap says by how much.

The constraints of the open parts are written into the using system once
per instance (the same constraint, through the instance's mapping), not
held by reference.

## In the model: trusses on what is under them

A truss roof is not given a height. The script says what it bears on
(`bearingOn(...)`: a perimeter's cap plates, an assembly's horizontal
members), and nothing else is looked for. Each of those is given as the
top of the member: the line along the middle of its top face, and that
face's width. The roof's `origin` is where the first heel is on plan.

**What is under a truss** is every one of those tops that is under some
part of a bottom chord, as modelled: across the truss (where it crosses
the chord's lumber, at either edge of the top) or along it (a wall under
a gable truss, for as far as the two overlap). Not only at the heels: a
beam under the middle is under it too.

**It rests on them as a rigid thing does**: on the two it would touch,
either side of its middle (the upper hull of the tops' heights, less the
chord's own height there), level or tilted as they say, and clear of any
that are lower. A chord that rises (scissors) sits where it is over the
bearing, on the edge it meets first.

Each truss is a rigid frame, free in the world, in one system for the
roof (`TrussRoof::seats`):

- **hard**: the chord's underside, somewhere along it, is at the top of
  each bearing that carries it, where that bearing is;
- **hard, datums**: its heel is where the layout puts it on plan, along
  the ridge and along the span;
- **hints**: its plane is square to the ridge and plumb, and it is
  level. These settle what the bearings leave open and give way where
  they do not.

Nothing ties one truss to another, so each is a component of its own.
On level plates no step is taken and the trusses are exactly where the
plates' height puts them.

**It is fastened to what it sits on, and to nothing else.** Every
bearing the chord touches once it is placed is one it sits on: at a
shared node where the chord's axis meets the bearing's (a cap plate,
whose axis is on its top face), and otherwise face to face (a bond: a
beam whose axis is through its middle, a wall along a gable truss).
There is no other connecting of trusses to things they happen to cross.

What cannot be done is said, as an error on the model:

- `truss-not-seated`: none of what the roof bears on is under any part
  of the truss;
- `truss-not-held`: it is on one bearing only, or all that is under it
  is to one side of its middle (it is placed level there);
- a truss whose hard constraints cannot all hold stops the model
  building, naming the truss and the gaps.

Nothing here is particular to buildings or to a code. A rule such as
"a truss bears at its heels" or "at a panel point" is a check for a
script to ask for; none is built in, and there is not yet a way to ask.

## How it solves

Gauss-Newton on the joints' coordinates and the scalars, component by
component and, within one, tier by tier, each tier's least-squares step
taken within the space the tiers above leave free. A component's rows
are laid out as a small dense matrix for that step, and the
rank-revealing solve (what is settled, what is left free) is by
eigenvectors. Ideas are taken from articulated-body formulations; no
outside solver is used.

## Not yet

- **Only truss seating uses it.** Walls and assemblies are still placed
  by coordinates. An assembly's own `bearingOn` still connects it to cap
  plates wherever axes happen to meet, and says nothing where they do
  not. The roof's layers do not follow a tilted truss. A perimeter's walls
  cannot yet differ in height (its cap plates must meet at the corners),
  so a roof tilted by its walls needs the lower bearing made as an
  assembly.
- **Checks a script asks for** (a ruleset): rules particular to a kind of
  building or a code, kept out of the generic model.
- **A scope handle for generators**: a generator is not yet handed its
  frame and its parent's to add hints to; the roof's seating writes the
  truss's hint itself.
- **Instances of instances** are not tried: a definition that uses
  another should work (it is a system like any other), with no test yet.
- **Bounds** on scalars (a point between a member's ends).
- **Conflict feedback drawn**: the component in conflict shown in the
  view, and one gap per suspect (each released in turn).
- **A sparse step within a component.** One large component (walls that
  can move under every truss) is still one dense matrix. That is where
  `sparten` would come in.
- **Sharing the blocks with `robot`.** `blocks.rs` repeats the traits and
  the parent-child matrix of `robot/src/block_matrix.rs`, which depends on
  nothing but the standard library. Split out as its own crate, both
  would use it.
