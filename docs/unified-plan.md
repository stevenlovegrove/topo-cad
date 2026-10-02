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
| Constraints are between anonymous points; bearing points are made after the solve, by geometry | Nodes are first class in the solve, named; a model's nodes are the solve's nodes |
| `TrussRoof.bearingOn` finds what is under each truss inside the roof generator | An explicit utility the script calls, returning nodes; the roof rests on nodes |
| Walls inside a perimeter, floors, roofs: written straight into the model by their generators | Bodies with published nodes in the scene, placed by datums and relations |
| `TrussRoof` can still be given a plan `origin` (the garage's script does) | Set out from a node (`at`), which it can be now |
| `shifted`, `studsAt`, surveyed walls: as-built numbers written over the layout rule | Readings: measured constraints on the same unknowns the layout rule hints |
| The truss-shape fit from field measurements is its own least-squares code | The same solve, measured tier |
| Loads, load path and member checks are computed beside the model from its geometry | Loads act on nodes and members of the one graph; reactions are the contacts' forces |
| Members are perfectly rigid; a sagging or racked building cannot be modelled | Bodies may deform: deflection under the same loads, in the same solve |
| The scan is compared with the model after the fact | Scan evidence is measured constraints |

## The order of work

Each step leaves everything working and removes one row above.

1. **Contact in the solver.** *Done, and set aside.* Inequalities and
   bounds held by an active set; weights as a tier after hard; falling
   detected. Bounds are used; one-sided contact and weight wait for
   step 7 (decided: get the rest right first).
2. **Nodes first class in the solve.** Named points, fixed in a frame or
   with numbers of their own (bounded); constraints are between nodes;
   the report and the 3D view show nodes with their kind. Truss seating
   states its supports as nodes (a utility finds them and chooses which
   two carry each truss) and its contacts as equalities; the model's
   bearing points are those nodes, by name.
3. **A scene: bodies, nodes, relations, then solve, then the model.**
   *Done* (`topo_core::scene`). Bodies (fixed, or free and placed by
   relations, kept upright by a hint), nodes fixed in them, members
   between nodes, relations (`coincide`, `datum`, `rest`); solved, then
   written into the model, which keeps the solve.
4. **Assemblies on the scene.** *Done.* Each assembly is a body, its
   points are nodes (`Assembly name/point`), `free()` makes it placed
   only by relations; the script has node handles (`assembly.node(...)`)
   and `coincide`, `datum`; `resting` is a node on the faces of both,
   with two bounded numbers on each, solved. Writing the scene makes a
   model node for each node and joins none by position (the pergola's
   bays are now joined by `coincide`). `Assembly.bearingOn` is a utility
   that makes a `rest` for each member crossing a cap plate, and says so
   where one does not touch.
5. **Walls and perimeters publish nodes.** *Largely done.* A perimeter is
   a body of the scene: its generator writes it where it is drawn, and if
   it is `free()`, relations to its nodes place it and everything it made
   moves with it. Any point of a member a generator made is a node a
   script can name: an end (`walls.node("Front/cap_plate", "start")`) or
   a distance along it (`walls.node("Front/cap_plate", 1.2)`, which
   becomes a node in the member's path). A roof can be set out from a
   node (`at`) instead of a point, so it goes where the walls go. Still
   to go: the walls inside a perimeter as bodies of their own (today the
   perimeter is one rigid body); `TrussRoof.origin` removed once the
   garage's script uses `at`; the roof's stations as nodes along the
   plates (today: a node for the first, the spacing for the rest).
6. **Readings as measured constraints.** `shifted`, `studsAt`, surveyed
   walls and the truss-shape fit move onto the measured tier; the layout
   rules become hints.
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
