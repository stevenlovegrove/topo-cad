# One system: the plan

Where the model is going, what is in the way, and the order of work. API
breaks are accepted throughout. [constraints.md](constraints.md) describes
what exists; [topology-audit.md](topology-audit.md) why this is wanted.

## The vision

**A model is one problem, and its geometry is that problem's solution.**
Nothing has a position because a script computed one. Things have
positions because the solve found them.

- **Bodies.** A rigid thing is a frame: a truss, a wall, a single member
  of an assembly. What is inside a body is given in its own coordinates and
  cannot move in it. A body that is used many times is a definition, used
  through instances.
- **Nodes are the interface.** A node is a named point. It is fixed in a
  body (a truss's heel, a plate's end), or free on a body's line or face
  (somewhere along a plate: one number; somewhere on a face: two, bounded
  by the face). Generators publish their nodes by name; any scope may
  refer to them. Members run between nodes, and every relation is between
  nodes (and directions and numbers): there is no other way for one thing
  to be placed against another.
- **Finding things by geometry is a utility, not the framework.** A script
  may call something that looks where one thing crosses or lies under
  another and makes nodes there. That is explicit, in the script, and what
  it returns is nodes like any others. Nothing connects or places things
  by coincidence of position on its own.
- **Relations.** Everything that places one thing relative to another is
  a constraint between nodes, directions and numbers: coincide, rest on,
  distance, offset, aligned, equal. Coincident nodes are one node in the
  topology; the solver may eliminate the variable.
- **Glue.** Where two things meet at faces, whatever joins them makes
  the node: a bearing point, held to a plane of each (its two numbers on
  each face bounded by that face), with a hint for where in the patch.
- **Contact, for now, is stated.** What a thing sits on is said (by the
  script, or by a utility it calls), and sitting is an equality: this
  point of the one is on that plane of the other. Later, with loads in
  the graph, contact becomes one-sided and weight presses things onto
  their supports, so the solve finds which carry it. The solver can do
  that already; the model does not use it until the rest is right.
- **Datums.** A coordinate is allowed in exactly two places: inside a
  body's own frame, and as a hard constraint to the world (a survey
  point, a slab corner). Everywhere else it is only where the solve
  starts from.
- **Tiers.** Hard (must hold; includes bounds), measured (readings, to a
  tolerance), default (hints). Each acts within what the ones before
  leave free. (Weight, a tier after hard by which things settle, is in
  the solver and waits for loads.)
- **Said, not silent.** Hard constraints that cannot all hold: refused,
  with the part in conflict. A freedom nothing settles: reported open.
- **The model that results** (nodes with positions, members, which nodes
  are on which axes and faces, connections) is read off the solution, and
  the problem is kept with it, so that everything shown, checked and
  analysed is the same thing.
- **Nothing particular to buildings in the core.** A rule from a code or
  a trade (trusses bear at panel points, studs at 16 inches) is a
  generator's default, a hint, or a check the script asks for.

## What is not yet this

| Today | What it should be |
| --- | --- |
| Where two walls meet at a step, the lower wall's plies end on the side of the taller wall's end stud | Corner framing (backing) for steps |
| Loads, load path and member checks are computed beside the model from its geometry | Loads act on nodes and members of the one graph; reactions are the contacts' forces |
| Members are perfectly rigid; a sagging or racked building cannot be modelled | Bodies may deform: deflection under the same loads, in the same solve |
| The scan is compared with the model after the fact | Scan evidence is measured constraints |

## The order of work

Each step leaves everything working and removes one row above.

1. **Contact in the solver.** *Done, and set aside.* Inequalities and
   bounds held by an active set; weights as a tier after hard; falling
   detected. Bounds are used; one-sided contact and weight wait for
   step 7 (decided: get the rest right first).
2. **Nodes first class in the solve.** *Done.* Named points, fixed in a
   frame or with numbers of their own (bounded); constraints are between
   nodes; the report and the 3D view show nodes with their kind. The
   model's nodes are the solve's: a bearing point is a contact node of the
   solve, not made afterward by geometry, and a generator joins only what
   it makes (`Model::scoped`).
3. **A scene: bodies, nodes, relations, then solve, then the model.**
   *Done* (`topo_core::scene`). Bodies (fixed, or free and placed by
   relations, kept upright by a hint), nodes fixed in them, members
   between nodes, relations (`coincide`, `rest`); solved, then
   written into the model, which keeps the solve.
4. **Assemblies on the scene.** *Done.* Each assembly is a body, its
   points are nodes (`Assembly name/point`), `free()` makes it placed
   only by relations; the script has node handles (`assembly.node(...)`)
   and `coincide`; `resting` is a node on the faces of both,
   with two bounded numbers on each, solved. Writing the scene makes a
   model node for each node and joins none by position (the pergola's
   bays are now joined by `coincide`). `Assembly.bearingOn` is a utility
   that makes a `rest` for each member crossing a cap plate, and says so
   where one does not touch.
5. **Walls and perimeters publish nodes.** *Done.* A perimeter is a body
   of the scene: its generator writes it where it is drawn, and if it is
   `free()`, relations to its nodes place it and everything it made moves
   with it. Its corners are nodes (`walls.corner(1)`, …): where they are
   drawn is a hint (see 6½). Any point
   of a member a generator made is a node a script can name: an end
   (`walls.node("Front/cap_plate", "start")`) or a distance along it
   (`walls.node("Front/cap_plate", 1.2)`, a node in the member's path),
   or a point on a face (`member(path).node("south", 1)`, an attachment).
   A roof can be set out from a node (`at`); each truss's station is a node.
   *Decided:* walls are not each a body: at a corner the plates lap and
   share nodes, so a wall cannot move alone; a wall moves by its corners.
   A wall that meets others part way along them (a partition) is built
   between them as they are, and has no numbers of its own.
   A roof is set out only `at` a node. A datum is a node: a point of the
   world (`Frame.world.point`, along only the coordinates given, if not
   all), placed by `coincide`; a scan is a frame with points in it
   (`Frame.named("Scan")`), so moving the scan moves what refers to it;
   its placement is the scan's registration (the workspace takes it from
   there). A face the scan sees is read by `coincide(scanPoint,
   member.node(face, at), { tolerance })`.
6. **Readings as measured constraints.** *Done.*
   `tape(a, b, distance)` and `survey(node, {x, y, z})` are measured
   constraints on any nodes (corners, assemblies' points). A truss's
   `shifted` is a reading of where it is along the ridge from its station,
   whose layout is a fallback (below its being upright). A wall's common studs are numbers of the
   wall: the layout is a hint on each, `studsAt` a reading, a stud
   against the wall at an end fixed; they keep their order and stay in
   the wall. Their ends are nodes (`walls.stud("Front", 3, "bottom")`):
   readings and datums on them move them, and the wall is built with its
   studs there. A surveyed wall's posts are numbers of it too: each
   reading is a reading of them (from a corner, in the corners' numbers; or
   from another post's face), and so is each check, so a tape between
   corners that disagrees with the drawing moves the corners, and the
   posts read from them. A reading the solve leaves off by more than three
   times its tolerance is said (`reading-off`). A script's `unknown(...)`
   parameters (the truss-shape fit) are numbers of the one solve too, and
   its measurements readings of them: the script is a generator like any
   other, evaluated again with each unknown moved a little to find how each
   measurement moves with it, and again at the solved values until they
   settle. No least-squares code of its own.
6½. **Generators of numbers, in one solve.** *Done for perimeters.* A
   perimeter's corners (three numbers each), its walls' heights at their
   ends and its walls' studs are numbers in the one solve with everything
   else; as drawn, hints (a stud's layout a fallback hint, below them).
   It is built at the numbers; the nodes it built that relations name
   move with them as the built ones would (found by building it again
   with each moved a little); once solved it is built again at the
   solved numbers, until they settle. So a corner moved by a reading
   makes a longer wall with its studs laid out on it, a survey of a cap
   plate's end changes the wall's height, and a post joined to a stud's
   top moves the stud (or, off the wall's line, the wall). Walls may slope
   (`Wall.heights(start, end)`, `Perimeter.base(corner, z)`); walls meeting
   at one height are held so, else the corner is a step. *And what they
   stand on:* a slab is a thing whose top is a number; a wall on it (or on
   a curb on it) has its bottom held to that, so no wall is given a height
   to start at (`Slab`, `Wall.on`, `Perimeter.on`). *And for roofs:*
   each truss is a free body of the one solve, written by its generator;
   `bearingOn` is a utility that looks at what is under each truss and
   states each contact (`rest_on_lines`), and the bearing points are those
   nodes. The solve goes through the structure of a part (the trusses are
   blocks, isolated from each other, eliminated against what they share),
   so this costs little more than solving them apart. *And a roof's
   shape:* a roof is its surfaces (`Roof`: parts, each an outline whose
   edges are eaves or gables; ridges, hips and valleys where the planes
   meet), made on what the solve placed, and its framing is set out from
   a part. *Not yet:* hip and valley framing; planes fitted to a scan.
7. **Loads in the graph.** Loads on nodes and members; contact becomes
   one-sided, with weight; reactions from the contacts; the takedown
   reads them.
8. **Deformable bodies.** Deflection under load in the same solve, so a
   sagged or tilted as-built state is a solution, and "unloaded" and
   "as loaded" are two states of one model.
9. **The scan as evidence in the solve.**

Along the way, as each needs it: the script API for named nodes; scopes
solved once and reused (definitions); a sparse step for large components;
checks a script asks for (rulesets).

## Open questions

- **Weight (for step 7).** Where it sits among the tiers (proposed: after
  hard, before measured), and what it acts on (proposed: bodies' poses
  only, never a shape parameter).
- **Edges of a face.** A tilted chord on a flat plate touches along an
  edge. Proposed: a contact at each edge of the supporting face (as
  seating does now), the bearing node in the middle of what is touched.
