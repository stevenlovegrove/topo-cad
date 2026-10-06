# Topology audit: where the framework leaves its own rule

`DESIGN.md` states the rule: "Topology is the source of truth … a
T-junction exists because a member ends on a node that another member
passes through, never because two solids happen to touch", "geometry is
derived, never authored", "one model, many views".

This is an audit of where the code keeps to that and where it does not,
made after fitting a model to a scan showed a truss that should sit on its
supports being placed by a height instead. It is a reading of the code,
not a redesign; the last section proposes an order of work.

## Summary

The core model is topological and sound: members are paths of nodes,
junctions are shared nodes, connections are annotations on nodes. Walls
keep to it inside themselves. Three things do not:

1. **Things are placed by coordinates and then connected by looking for
   coincidence.** Trusses, headers built as assemblies, and wall corners
   become connected only if their coordinates happen to agree to 0.5 mm.
   When they do not, nothing says so.
2. **The roof is two things beside the model, not one thing in it**: a
   set of trusses positioned from an origin and a height, and a load and
   surface attached to the group, distributed by geometry.
3. **The analysis is rule-driven from geometry and role names** rather
   than from the graph's own supports and fixities, and it gives no
   displacement of nodes.

## What is topological today

- `Member.path` is an ordered list of node ids; interior nodes are
  connection points (`topo-core/src/model.rs`).
- A joint is a shared node, with `JointSpec` rules (butt, through, mitre)
  and `ConnectionUse` records (which fastener joins which members there).
- Within a wall, studs are made on nodes placed *on* the plates
  (`node_on`), and headers and sills on nodes on their posts.
- Within a truss, the shape is named points and members as paths of
  point names; the built truss shares nodes at every joint.
- `Topology` (junction kinds), solids, cuts and the analytical extract
  are derived.

## Where it leaves the rule

### Connection by coincidence, failing silently

- `node_at` reuses a node within 0.5 mm. This is how most shared nodes
  come to exist: two generators computing the same coordinate.
- `connect_members` tries every pair of members and inserts a shared node
  where their axes cross. Any pair that does not cross is dropped without
  a word (`Err(_) => {}`, `model.rs`).
- Validation reports nodes that coincide without being shared. It does
  not report a near miss: a member that was meant to bear and stands
  clear.

### Trusses are positioned, not seated

- A truss is built at `origin + run·station + up`, with every node at an
  absolute position. It never refers to what it bears on.
- `bearingOn(...)` in the script only names things. The build takes a
  height (given, or the first perimeter's top), builds the roof there,
  and *afterwards* runs `connect_members` between bottom chords and the
  named plates.
- So a truss raised off its plate (the new `raised`), or over a header
  built a little high, is not connected to anything. The plane-frame
  solve then finds no bearings and marks it unsolved; nothing at build
  time says why.
- The real fix for a truss that tilts because its two bearings are at
  different heights is not a number on the truss. It is for the truss to
  take its position from its bearings.

### Assemblies tie in only by coordinates

- An assembly's points are absolute coordinates. They merge with existing
  nodes only if they land within 0.5 mm.
- A header built this way in a real project has its posts "within the
  walls" by comment only, its top typed as the wall height, and no
  statement that it bears on or is carried by anything in the walls.

### Corners and heights are numbers

- Adjoining walls share a corner node because both plate ends compute to
  the same point. Stud clearance at corners is a computed inset.
- The bearing height exists three times: the wall's `height`, the roof's
  optional `height`, and whatever an assembly's author typed.
- The roof's plan position (`origin`, `span_dir`) is unrelated to the
  perimeter's points; a real project computes the heel line by hand from
  wall corners.
- As-built adjustments (`studsFrom`, `studsAt`, `shifted`, `raised`,
  `moved`) are each a separate positional override on a generator.

### Orientation is stored, not derived

- A member's section axes come from a stored world vector (`depth_dir`)
  on nearly every generated member, else from its group frame. Neighbours
  are never consulted, so "this plate lies flat on that stud wall" is not
  something the model knows.

### Loads sit beside the model

- A roof's build-up is recorded twice: as a pressure already added up in
  the script, and as a `Surface` with the same layers for drawings.
- Sloped loads are an `Area` load on the group; the analysis picks the
  members to load by role name (`top_chord`, `rafter`, `joist`) and gives
  each a tributary width from the positions of member midpoints.
- Flat loads are turned into member loads at build time using the
  nominal spacing, ignoring trusses that were shifted. Two tributary
  rules for one roof.
- The surfaces drawn are derived from the middle truss's solids, so a
  truss that is raised or shifted does not move the surface.

### The analysis is beside the graph

- `AnalysisModel::extract` does project the graph (elements per path
  segment, fixity from the connection at the node), but it is only
  exported. The checks that run use `takedown`, which decides behaviour
  by rule: group kind `truss` is a plane frame, a near-vertical member
  is a column, anything else a simple-span beam.
- What bears on what comes from shared junctions and bonds, but bearing
  against fastening is classified from solids' bounding boxes.
- Supports' restraint flags are not read.
- Deflection is per member along its length. There is no displacement of
  nodes, and a member's deflection does not include its supporter's. So
  "the truss is lower here because the header under it sags" cannot be
  computed, and neither can a deformed shape to compare with a scan.

## What it costs

- A model can look connected and not be, with no error.
- An as-built finding ("these trusses sit 30 mm higher at one end") has
  no natural place: it becomes another positional override, and what it
  bears on has to be moved by hand to match.
- Loads cannot follow as-built geometry.
- A deformed shape, which is what a scan of a loaded building shows,
  cannot be produced.

## A proposed order of work

Each step keeps the script's existing surface working.

1. **Make missing connections loud.** `connect_members` returns what did
   not meet; `bearingOn` and assembly ties report "T3 was to bear on the
   east plate and stands 30 mm clear of it". A validation for near
   misses. Small, and it stops the silent case at once.
2. **Seat by reference.** A truss's bearing points become nodes *on* the
   members it bears on (as studs already are on plates): the truss takes
   its height at each bearing from that member's top, so different
   heights tilt it. `raised` goes; a shim or extra plate is modelled as
   what it is. Assemblies get the same: a point defined as "on member X
   at s", and "on top of".
3. **One bearing height.** The roof's height comes only from what it
   bears on. An assembly's "flush with the plates" refers to the plates.
4. **Orientation from the joint.** Where a member lies on another, its
   depth direction comes from that contact (annotated on the node or the
   bond), with the stored vector as an override only.
5. **Loads on the model.** A surface is an element of the model spanning
   named members; its layers are its weight (recorded once); tributary
   areas come from the surface's edges and the members under it, so they
   follow shifted and tilted trusses. Flat and sloped loads use one rule.
6. **Analyse the graph.** Use the extracted analytical model (supports
   with their restraints, fixity from connections) for the frame solve,
   keeping the member checks. Output node displacements.
7. **Deformed prediction.** With node displacements, the scan comparison
   can use the model under the loads present when it was scanned, and
   report as-built offset, predicted deflection and residual separately.

Steps 1 and 2 answer the case that prompted this. Steps 5 to 7 are what
"loads are structural in a single model" needs, and are the larger part.

## Not established

- Whether a current real project produces `truss-unsolved` or
  unconnected-node warnings (needs a run).
- `Floor` exists in the engine but is not reachable from the script.
- The survey levels and datums were not read.

## Since

- Step 6, for models that ask for it (`analysis: "frame"`): the extracted
  analytical model is solved as a 3D frame, supports' restraints and
  connections' fixities read, node displacements given. Buildings still
  use the takedown.

- Truss roofs are no longer given a height or `raised`: they sit on what
  the script says they bear on, wherever it is under them, and are
  fastened only to what they sit on. See
  [constraints.md](constraints.md), "In the model".
