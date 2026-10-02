# Frames and constraints

How things are placed relative to each other, stated generically (building
framing is the first use, not the only one). This is the design agreed in
discussion and a first implementation of it, `crates/topo-solve`. It is not
yet used by the model's generators: see "Not yet" below, and
[topology-audit.md](topology-audit.md) for why it is wanted.

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
  span, a spacing. A point is an expression over frames and scalars, not an
  unknown itself.
- **A constraint relates points and directions in whatever frames**:
  coincident, distance, offset along a direction, parallel, perpendicular,
  a scalar's value.
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

## How it solves

Gauss-Newton on the frames' own motions and the scalars, tier by tier,
each tier's least-squares step taken within the space the tiers above
leave free. Derivatives are by differences and the linear algebra is small
and dense: a scope's unknowns are tens, not thousands. Ideas are taken
from articulated-body formulations (the frame tree, joints as the open
freedoms, constraints between points of two frames, dropping what is
redundant); no outside solver is used.

## Not yet

- **Generators do not use it.** Trusses, walls and assemblies are still
  placed by coordinates. The next step is seating a truss on its bearings
  with it, and making a missing connection an error.
- **Scoped solving**: a scope solved on its own and then held rigid in its
  parent. Today everything is one problem.
- **Bounds** on scalars (a point between a member's ends).
- **Conflict feedback beyond the list**: the subgraph in conflict drawn,
  one gap per suspect.
- **Sparsity.** Dense is enough for now; the structure (each constraint
  touches only the frames between its two ends) is there to use later.
