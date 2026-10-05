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
- **Nodes are the interface.** A node is a named point of the system
  (`System::node`): fixed in its frame, or with numbers of its own (one:
  it is somewhere on a line; two: on a face), which may be bounded.
  Constraints are stated between nodes, and the report lists each node
  with what it is a point of, its kind, its numbers with their bounds,
  and what settled each.
- **A constraint relates points and directions in whatever frames**:
  coincident, distance, offset along a direction, aligned (the same
  direction), parallel (either sense: held as aligned in whichever sense
  is nearer), perpendicular, two numbers equal.
- **A sketch is not a special thing**: it is a frame whose contents are
  held to its plane (a planar joint, or `z = 0`).

## Tiers

A constraint has a tier, and the tiers are solved in order, each only
within what the ones above leave free:

1. **Hard**: it must hold. Connections, rigid shapes, datums. A hard
   constraint may be **one-sided**: a number at least another (a bound),
   a point on or beyond a plane (a contact: it may touch, or be clear).
2. **Weight**: frames given weight settle as low as the hard constraints
   let them, onto whatever holds them up. Which one-sided constraints end
   up touching is found here, not chosen beforehand. Weight moves frames,
   never a scalar. *The solver has this; the model does not use it yet*
   (one-sided contact and weight wait for loads: see
   [unified-plan.md](unified-plan.md)).
3. **Measured**: it should hold, to a stated tolerance. A reading from a
   scan or a tape.
4. **Default**: a generator's hint ("my y is up"), or a layout rule. It
   decides only what nothing above decides.
5. **Fallback**: a hint that gives way to the hints. A stud's place in
   its wall's layout is one: a reading on a stud is then met by moving the
   stud, not the wall's corners (where they are drawn is a hint).

Solving in order, not with small weights, means a hint cannot bend a
measurement and a measurement cannot bend a hard constraint, and the
problem stays well conditioned. Whatever is still free after these stays
where it was put (its nominal place).

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
- **What is touching**: the one-sided constraints that are met exactly
  (a bound reached, a contact made), and for the others how much room
  they have.
- **What falls**: a thing its weight carries off without limit. Nothing
  holds it up, and that is an error.
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
members), and nothing else is looked for. The roof is set out on plan from
a node (`at`): a corner of the walls, a point of a scan's frame, or a
datum (a point of the world).

**Each truss is a free body of the building's one solve**, holding what
its generator wrote (`TrussRoof::place`). Its heel is a node: across the
ridge it is where the layout puts it (hard), along the ridge a hint, and a
reading of it (`shifted`) a reading. Its body's hints keep it upright and
turned as it was put (plumb, square to the ridge, level) where nothing
says otherwise.

**What is under a truss** is found by a utility, by geometry, as things
are put: every top of what it bears on that is under some part of a bottom
chord, across the truss (its two edges and its middle, where it crosses
the chord's axis) or along it (a wall under a gable truss: each end of
where they overlap). Not only at the heels: a beam under the middle is
under it too. Of those, the truss is carried by the two of the upper hull
(how high the truss must be to touch each) that its middle is between; a
chord that rises (scissors) is carried where it meets an edge first. It
touches every line on the line through those two, and **each is stated as
a contact** (`Scene::rest_on_lines`): a node on a line of the chord's
underside (its axis, which is in it) and on that line of the top, one
number along each, bounded by its member, held to be one point. Where the
chord lies on the whole of a top it crosses, the line is the top's axis,
if that is in the top (a cap plate's): the bearing point is then a node on
both axes. Along a top, it is on the chord's axis where that is over the
top (else the middle of where they overlap).

Writing the scene makes each contact a node of the chord and of what it is
on (a member's own node there, if it has one: two members' nodes there are
made one, because the contact says so). It is fastened there. **Nothing
else joins a truss to anything**: a generator joins only what it makes
(`Model::scoped`), so a heel is not joined to a plate's end because it is
where that end is.

What cannot be done is said, as an error on the model:

- `truss-not-seated`: none of what the roof bears on is under any part
  of the truss;
- `truss-not-held`: it is on one support only, or all that is under it
  is to one side of its middle (it is placed level there);
- a contact that cannot hold (the truss cannot touch what was found
  under it) stops the model building, naming the contact and how far off
  it is.

Nothing here is particular to buildings or to a code. A rule such as
"a truss bears at its heels" or "at a panel point" is a check for a
script to ask for; none is built in, and there is not yet a way to ask.

## Nodes on faces

Positions are nodes, and things are joined at nodes. A member's path is
the nodes on its axis; but things touch at faces, and a face is not
where the axis is. So a node may also be **on a member's face**
(`Model::attachments`, in `topo-core`):

- which face (`+u`, `-u`, `+v`, `-v` of the section);
- two numbers, free within the face: how far **along** the member, and
  how far **across** the face from its middle.

The face's plane is not stored. It follows from the member: its axes,
its section, its anchor (`Model::face`). So the node's position is a
function of the two numbers and the member, and the two numbers are
bounded by the face (a box). Validation says so if a node is no longer
where its member puts that point of its face, or is outside the face.

A node on the axis is the case of no offset: asked to put a node on a
face at a point that is on the axis, the model puts it in the path. One
node may be in one member's path and on the faces of others: that is a
bearing point, made by whatever joins the two (the glue), not by either
of them.

Downstream, an attachment is a rigid arm from the member's axis to the
node (`AnalysisModel::links`), and a point of support in the load path:
what is at a node on a member's upward face rests on the member there.

In the solver's terms this is a point with two bounded scalars in the
member's frame, held to another member's point. The solver has bounds
now, but bearing points are not yet its unknowns: they are read off where
the contacts touch (see [unified-plan.md](unified-plan.md)).

## The scene

`topo_core::scene` is how things are placed. A **body** is a rigid thing
(a frame), fixed where it is put, or free: then its placement is only
where the solve starts, relations place it, and a hint keeps it upright.
A **node** is a named point fixed in a body. **Members** run between a
body's nodes. **Relations** are between nodes:

- `coincide`: two nodes are one point, and one node of the model (a
  node made one with a point of the world is placed there: that is a
  datum);
- `rest`: one member rests on another where they cross. It makes a node
  on the underside of the one and the top of the other: on each face two
  numbers (along, across) bounded by the face; held to be the same
  point; with a hint for each, the middle of its face across. If the two
  cannot touch (both fixed, apart), it is refused, with how far apart.

The scene is solved, then written into the model: each body's group at
its pose, a model node per node (one for nodes made one), the members,
and the nodes on faces as attachments (their numbers are the solve's).
The solve is kept in the model to be shown.

A **body can be what a generator already wrote**: a perimeter is written
by its generator where it is drawn, and is a body of the scene with that
content. Fixed, it stays; free, the relations to its nodes place it and
the scene moves all of it (nodes, groups, members' directions) to where
the solve puts it before anything else is written. A point of any member
a generator made is a node by name: `Walls/Front/cap_plate:start` (or
`:end`), or `Walls/Front/cap_plate@1.2` (so far along it: written as a
node in the member's path). A point **on a face** of a member is a node
too: `member("Trusses/T1/bottom_chord").node("south", 1)` is the middle of
the face toward the south, a metre along it (a direction, or a compass
word; written as an attachment on that face). So a face seen in a scan is
read where the scan sees it, not where the member's axis would be. Nodes
made on the same face of a member at the same point are one node. On a
member a generator built at numbers (a stud), it moves with them, as the
member's ends do: a face reading on a stud moves the stud.

A point of a frame may also be **on a plane** of it:
`Frame.named("Scan").on(at, normal)` (in the frame's coordinates) is a node
anywhere on that plane (where it was put, unless something says
otherwise), found by the frame's name with no point declared. A face seen in
a scan is read on one: `coincide(scan.on(at, normal), member(path).node(normal,
along), { tolerance })`. This is what dragging a face in the scan workspace
writes.

Writing makes a model node for each node, and joins none to another
because of where it is: only `coincide` (or a generator, inside what it
makes) does that. What a generator already wrote can be brought in: a
model node as a node fixed in the world, a member as a member fixed
there (for something to rest on); relations to them join to them.

In the script, assemblies are bodies in one scene: their points are
nodes named `Assembly name/point`, and `Assembly.free()`,
`assembly.node(point)` and `coincide(a, b)` relate them.

A **frame** is a body with points and no members (`Frame.named("Scan")`,
placed where it is registered, or `free()`); each point is a node
(`Scan/heel line`), so moving the frame moves every reference in it.
`Frame.world` is the building's own frame, fixed: its points are
**datums**, and a node is placed at one by `coincide`. A point may give
only some of its coordinates (`{ x, y }`, from a georeference): along the
others it is a number of its own, 0 unless something says otherwise. The end of any member a generator made is a node too:
`walls.node("Front/cap_plate", "start")`. `Assembly.bearingOn(walls)` is
a utility: each of the assembly's members that crosses a cap plate rests
on it (`examples/primitives/05-joists-on-walls.ts`). `Perimeter.free()`
and `TrussRoof.of(shape, { at: node, ... })` let a building be placed by
a datum on a corner, with the roof going where the walls go
(`examples/primitives/06-placed-by-a-corner.ts`).

**Readings** are measured constraints between nodes: `tape(a, b,
distance)` (two nodes this far apart) and `survey(node, { x, y, z })` (a
node here along whichever axes are given), each to a tolerance (3 mm by
default). They give way to anything hard, share a misfit among themselves
by their tolerances, and the solve says how far each is off. `coincide(a,
b, { tolerance })` is a reading too: the two nodes read to be one point
(a point of a scan's frame read on a member's face).

A **perimeter is built from numbers of the one solve**:

- its **corners** (`walls.corner(1)` is its start, then each corner in
  turn): three numbers each, in the perimeter's body, at the bottom of the
  walls there; where a corner is drawn (`Perimeter.base(corner, z)` for
  one not at the perimeter's `z`) is a hint. Two walls in one line (a wall
  that changes what it stands on, or its height, part way) meet end to
  end, their plates cut square;
- each wall's **height at its start and its end** (`Wall.heights(start,
  end)`: its top slopes between them; heights are above the wall's
  bottom); as drawn a hint. Where two walls meet at one height as drawn
  they are held at one height there; where they meet at different
  heights, that corner is a step (said, as `corner-step`);
- each wall's **bottom**, where it stands on something (below): at its
  start and its end, held to that thing's top. A corner is then at the
  bottom of the wall that leaves it (else of the one that arrives);
- each wall's **common studs** (`Wall::common_studs`), along it from its
  start: where the layout puts one a fallback hint, a stud given by
  `studsAt` a reading, a stud against the wall it meets at an end held
  there (the last as far from the end corner as the layout has it,
  wherever the corners are). They keep their order (one-sided: no two
  overlap), and so stay in the wall. Their ends are nodes,
  `walls.stud("Front", 3, "bottom")` (or `"top"`), numbered along each
  wall from its start.

The perimeter is **built at the numbers**, as drawn first. Any node it
built that a relation names (`walls.node(...)`, `walls.stud(...)`) is a
node of the scene that moves with the numbers as the built one would:
what each number does to it is found by building the perimeter again with
that number moved a little (the studs all at once: each stud's ends move
with its own number only). Once solved, the perimeter is **built again at
the solved numbers**, and solved again, until they do not change (they
settle in a pass or two; `unsettled` is said if not). So what the
generator builds is what the solve asked for: a corner moved by a reading
makes a longer wall with more studs, not a stretched one.

**What walls stand on is a thing**, and a wall's bottom is where that
thing's top is. A `Slab` is a plan outline with its top at a height; a
curb on it (`slab.curb(height)`) is as wide as the wall's plates, under
each stretch of its bottom plate. A perimeter's walls stand on what it
says (`Perimeter.on(slab.curb(mm(40)))`), or a wall on what it says itself
(`Wall.on(slab)`). The slab's top is a number of the one solve: where it
is drawn is a hint, so a reading of a point of it (`slab.at([x, y])`: a
`survey`, or a floor seen in a scan, `coincide(scan.on(p, [0, 0, 1]),
slab.at([x, y]), { tolerance })`) moves it, and each wall on it has its
bottom held to it (plus its curb's height), so they move with it; the
slab and the walls are built again where the solve has them, as a
perimeter is. A wall's height is above its bottom: beside a wall as tall
on something higher its top is as much lower, and the two meet at a step
at both, top and bottom (`corner-step`). Two walls are held at one height
at a corner where their *tops* are one as drawn. A floor at two heights is
two slabs.

A slab may be **laid to fall** (`Slab.sloped([sx, sy], about)`: its top a
plane, rising so much per metre along x and y; its one number is its
height at `about`). A wall standing on it, or on a curb of one height on
it, has its bottom at the slab's top at each of its ends, so its bottom
plate slopes with the slab along the wall, and where the slab slopes
across the wall the plate is turned about its own length to lie on it
(`Wall::base_cross`; the studs stand plumb, cut to it) (and, its height being above its bottom, its
top with it). A curb whose **top is level** (`slab.curbTo(level)`) is what
gives walls a level seat on such a slab: the wall's bottom is at that
level, held there whatever the slab does, and the curb is as tall under it
as the slab is below that there (none where the slab comes up to it), so a
reading of the slab changes the curb, not the walls
(`examples/primitives/12-a-slab-laid-to-fall-and-a-level-curb.ts`,
`13-walls-on-a-slab-laid-to-fall.ts`).

Slabs and curbs are **solids** of the model (`Model::solids`: a plan
outline between two surfaces, each level or sloping: `Level`), not members: they have no axis, join
nothing, and are not analysed; they are drawn, and seen in a scan, as
members are (the exact-lines 3D view hides what is behind the convex hull
of a solid's outline). A member through a solid, or one solid through
another, is a `clash` like any other (`Geometry::solid_clashes`), and a
test builds every example and asserts there are none
(`nothing_passes_through_anything_in_any_example`). What each bottom plate bears on is said in its support
(`Bottom plate on a curb on Slab`). *Not yet:* a curb's height or level,
and a slab's slope, are given, not numbers of the solve; a slab's top is a
plane, not a surface fitted to what was scanned; a free perimeter's curbs do not move
with it; and floors, stem walls, footings and piers are not generated.
(`examples/primitives/10-walls-on-a-slab-and-a-curb.ts`.)

**A floor sits on what is under it.** A `Floor` is given its rectangle on
plan (a corner, the way its joists run, a length and a width) and its
framing; its height is not given. It is a free assembly (rims across the
joists' ends, a band joist along each side, commons between at the
spacing: `Floor.assembly()`), and `bearingOn(walls)` rests each of its
members on each cap plate it crosses on plan (one that ends within an inch
of a plate's line it runs toward bears on it), and on each level member (a
beam) of an assembly it is said to bear on (one added before it), so the solve sits it on the
walls as they are, tilted if they are. On plan it stays where it is drawn,
as a fallback (`Scene::stays_on_plan`, on its first corner; its turn is
the hint any free body has), so a relation to one of its corners moves it.
Its loads are per area, shared to the joists by their spacing. *Known
limitation:* it is one rigid body, so on plates that are not in one plane
it cannot sit, and is refused (with how far each joist is off): joists
that each sit for themselves, as trusses do, are not done. *Not yet:* a
deck on it, openings in it, beams and posts under it, and walls standing
on it. (`examples/primitives/11-a-floor-on-walls.ts`.)

A **surveyed wall**'s posts are numbers of it as well (`Survey::relations`):
each starts where its reading puts it; each reading is a reading of the
posts (from a corner of the wall, in the corners' numbers; or from the face
of another post), to 1/16"; each check (a tape between two datums) is a
reading too, to its own tolerance. They keep their order. So a check that
disagrees with the readings and the drawing is shared out as readings and
hints are, not only reported; the wall is built with its posts where the
solve puts them, and its `survey-check` notes say what is left.

Any reading the solve leaves off by more than three times its tolerance is
said, as a warning (`reading-off`): it does not agree with the rest.

So a tape between corners, a survey of a cap plate's end, a post joined to
a stud's top: each moves the numbers it needs to, and the walls are built
there (`examples/primitives/07-walls-on-readings.ts`,
`08-studs-on-readings.ts`, `09-walls-that-slope.ts`). A reading on a stud
moves the stud rather than the wall's corner (the layout is a fallback);
a hard relation the stud cannot meet alone (a fixed post off the wall's
line) moves the wall. A truss's station is a node too: where the layout
puts it along the ridge is a fallback (below its being upright, as a
stud's layout is), and `shifted` is a reading of how far from it the truss
is; a reading of the truss itself (a face of its chord against a scan)
moves it without leaning it. Where along a top a truss's contact is (as
things were put) is a fallback too. `examples/primitives/04-placed-by-nodes.ts` places
a free beam on two posts by its end nodes alone.

Where a wall slopes (or two meet at a step), the plates at its corners
cannot interleave: they butt. A plate of the second wall that ends at the
same node as one of the first butts against it there, cut on its side
toward it (`JointRule::ButtFace`: where they slope differently, a line
along the one may first meet the other's top); one that does not end
there (the plies at different heights; or a bottom plate, where the walls'
bottoms are) is cut back to a node on the first face of the other wall
that any of its section runs into (`Model::attach`: a face reaches past
its member's end nodes by its section's size, as a joint may run it), and
its end is cut on that face. Such ends meet side faces and carry nothing
through their ends, so they are not checked as bearing (only an end cut
on a face that faces up or down is). *Known limitation:* at a step, the
lower wall's plies end on the side of the taller wall's end stud, which
is narrower than they are wide: no backing is framed for them yet.

## Seeing it

`examples/primitives` has the smallest models that show each thing:

1. **Beam on beam.** Two joists crossing a beam, joined by
   `Assembly.resting(upper, lower)`: a node where they touch, on the
   underside of the one and the top of the other. One joist has its axis
   on its underside, so that node is in its path; the other's axis is
   through its middle, so the node is on its underside face. For both it
   is on the beam's top face. They have to touch: `resting` joins, it
   does not move, and says how far apart they are if they do not.
2. **Trusses on beams**, one beam lower: free things sat by a solve, then
   joined to the beams at bearing points on the beams' top faces.
3. **Trusses on walls**: the same, where the axes meet, so every bearing
   point is an ordinary node on both axes.
4. **Placed by nodes**: a free beam on two posts by its end nodes alone.
5. **Joists on walls**: `Assembly.bearingOn(walls)`, a rest for each
   joist on each cap plate it crosses.
6. **Placed by a corner**: free walls placed by a datum on a node of
   theirs; the roof set out from the same node.
7. **Walls on readings**: corners moved by a survey and a tape; the walls
   built on them.
8. **Studs on readings**: studs moved by a tape and a survey.
9. **Walls that slope**: a lean-to's heights, a lowered corner, and a
   survey of a cap plate's end that the generator is built again to meet.
10. **Walls on a slab and a curb**: three walls on a curb, one on the slab
    itself (a step at both its ends), and a reading of the slab's top that
    moves everything standing on it.
11. **A floor on walls**: a floor platform given on plan only, sat by the
    solve on the walls under it.
12. **A slab laid to fall and a level curb**: the walls on a curb whose top
    is level, as tall as the slab is below it; a reading of the slab moves
    the slab and the curb, not the walls.
13. **Walls on a slab laid to fall**: without a level curb, their bottoms
    follow the slab.
14. **A floor at two heights**: two slabs with a step, walls on each, one
    top; walls in one line meet end to end.
15. **A slab read from a scan**: readings in a scan's frame put the slab
    where the scan has the floor, and the walls with it.
16. **From the slab up**: a slab laid to fall, a level curb, walls, and
    trusses, each on the one under it.
17. **A roof over part**: set out from a node part way along a plate, for
    a length; some trusses of another shape.
18. **A loft on a beam**: a floor over part of a room, on the back wall and
    a beam on posts.

The 3D view draws the **skeleton** over the model, with a panel to show
or hide each kind:

- **nodes on axes** (dots: darker where members meet);
- **bearing points, on faces** (orange diamonds). Hovering one draws the
  face it is on (the box its two numbers are free in), the two ways it
  may move there, and says the numbers and their bounds;
- **solve nodes** (small circles): the solve's named points. A filled
  one is fixed in what it is a point of; a hollow one has numbers of its
  own, and is ringed in the colour of what settled them. Hovering one
  says what it is a point of, draws the range each number is kept in
  (a bar through the node, with ticks at its bounds) and the way it
  moves, and lists the constraints on it. A number that is a node's is
  shown with the node, not on its own;
- **free variables**: what a solve had to find. A square is a rigid thing
  free to move, at its origin; a short bar is a number, at the point it
  moves and along the way it moves it. Each is coloured by what settled
  it (hard, measured, hint; red if open). Hovering a square draws each of
  its freedoms (along an axis, or dashed about one) in the colour of what
  settled it, lists the constraints on it, and marks them;
- **constraints** (rings where they hold points together; dashed for
  hints). Hovering one says what it is, what it depends on, and how far
  off it is.

The tiers can be shown or hidden one by one, which filters both the
variables (by what settled them) and the constraints (by their tier).

A solve that places part of a model is kept with the model for this
(`Model::solves`): its variables, their freedoms and what settled each
(the solver now says so per freedom), and its constraints with their
gaps.

## How it solves

Gauss-Newton on the joints' coordinates and the scalars, component by
component and, within one, tier by tier, each tier's least-squares step
taken within the space the tiers above leave free.

One-sided constraints are held by an **active set**: those taken as met
exactly are solved as equalities with the hard constraints. One that a
step would cross becomes active; an active one is let go when, without
it, the step would move off it into the clear. Weight is a stride
downhill within what the hard and active constraints leave free, no
longer than a cap and no further than the first inactive contact in the
way; the stride is brought back onto the hard constraints before it is
judged. A part still going down when the passes run out is falling. 

**Through the structure of a part.** What the tiers above leave free is
held in one of two forms (`free.rs`). Small parts are dense: a basis, and
each tier a dense least-squares solve (rank and what is free by
eigenvectors: Householder reduction and QL). A part with structure that
isolates most of it is an **arrowhead**: its unknowns split into a few
shared ones (the most connected: a free perimeter's body, say) and blocks
that no constraint joins (each truss and the numbers of its contacts),
found by taking the most connected unknown of a group too big as shared
until none is. A tier is then solved by eliminating each block's own
coordinates against the shared ones (a small dense solve per block), then
solving what is left on the shared ones; what is free afterward has the
same form, and the step is made least-norm by taking out its part in what
is still free. Each tier still decides only what the ones above leave
free; the steps are the dense solve's (a test holds the two equal). Forty
trusses on walls that can move are forty small solves and one of the
walls, not one of three hundred unknowns.

**What a step moves least.** A step is least in how far things move, not
in its coordinates: a frame's turn counts as far as it moves the farthest
point of it anything is asked of, and a number (where a contact is along a
member, a corner, a height, a stud: the shape of what is built) ten times
a move. So a datum far from where something is drawn moves it, rather than
turning it about its origin or reshaping it. Further, a part that starts
far from meeting its hard constraints is first put in place as it is,
numbers held, and then solved from there.

**Judging a step.** The whole step (every tier's), or as much of it as
does not make things worse; where the hard constraints are not met, the
hard tier's part alone too, and whichever meets them better (the lower
tiers' part, made where the hard constraints are not met, can keep them
from being met: a turn is not straight). Once they are met, a step is
brought back onto them before it is judged, and then (what it does to the
readings being second-order too) back to what the readings ask there; and
it is judged tier by tier, the readings first, a tier no worse (but for
rounding) letting the next decide. Without that, a hint's step beside a
reading at a lever arm of metres was refused for the reading's
second-order change, and the solve stopped short. Ideas are taken from
articulated-body formulations; no outside solver is used.

## Not yet

- **The roof's layers do not follow a tilted truss.**
- **Checks a script asks for** (a ruleset): rules particular to a kind of
  building or a code, kept out of the generic model.
- **Instances of instances** are not tried: a definition that uses
  another should work (it is a system like any other), with no test yet.
- **Conflict feedback drawn**: the component in conflict shown in the
  view, and one gap per suspect (each released in turn).
- **Structure within structure.** A part is split once into shared and
  blocks; a block too big that has its own structure is solved densely.
  Nested splitting (or a sparse factorisation, `sparten`) would go further.
- **Sharing the blocks with `robot`.** `blocks.rs` repeats the traits and
  the parent-child matrix of `robot/src/block_matrix.rs`, which depends on
  nothing but the standard library. Split out as its own crate, both
  would use it.
