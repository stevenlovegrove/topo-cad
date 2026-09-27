# topo-cad — design

Topology-first CAD-as-code for framed structures (wood house framing first,
aluminum extrusion builds second). The user describes **what connects to what**;
solid geometry, drawings, schedules and (Milestone 2) structural calcs are all
*derived* from that topology plus properties attached to it.

```
 script / DSL  ──►  Model (IR, serde JSON)  ──►  Topology  ──►  Geometry  ──►  Drawings (SVG/DXF), mesh (OBJ)
                        │                           │               │
                        └──────── loads, supports, fixity ─────────►  AnalysisModel ──► DesignCode (NDS…) ──► calc report
```

## 1. Principles

1. **Topology is the source of truth.** Nodes (vertices) and members (edges) are
   explicit. A T-junction exists because a member *ends* on a node that another
   member *passes through* — never because two solids happen to touch.
2. **Geometry is derived, never authored.** A member's solid = its section
   swept along its axis, justified by its anchor, clipped by end planes that are
   computed from the junctions it participates in.
3. **One model, many views.** The analytical model (M2) and the drawings are
   both projections of the same model; nothing is re-entered.
4. **Every derived number is traceable.** Cut lengths, junction decisions and
   (M2) code checks carry the rule / clause that produced them so an engineer
   can audit the chain.
5. **Domain-agnostic core.** `topo-core` knows nothing about "studs". Domain
   crates (`topo-timber`, later `topo-extrusion`) supply catalogs, generators and
   presets. Design codes are plugins behind a trait.
6. **Internal units are SI** (m, N, Pa, kg/m³). Unit systems exist only at the
   edges (input helpers, formatting).

## 2. Data model (`topo-core`)

| Concept | Meaning |
|---|---|
| `Node` | A point. Where members meet, loads apply, supports sit. |
| `Member` | One **physical piece**: an ordered, collinear *path* of ≥2 nodes. Interior path nodes are connection points (T/X junctions). Carries section, material, role, priority, depth direction, anchor. |
| `Section` | 2D profile (rect or polygon with holes) × `plies` (built-up members: 2-ply headers, double top plates). Section properties computed. |
| `Material` | Generic mechanical props (E, G, ρ) + `design_key` that a design code resolves to its own tables (e.g. `"NDS:DFL:No.2"`). |
| `Group` | Hierarchical assembly (wall, floor, roof, frame) with a local frame. Generators create groups; views are defined in group frames; groups carry drawing annotations. |
| `JointSpec` | Optional per-node overrides: resolution rules (butt/through/miter/square), connection assignments, end fixity. |
| `Connection` | Fastener groups + hardware + reference (e.g. "2-16d common end nail, IRC R602.3(1)"). Has both a *mechanical* meaning (capacity, M2) and an *analytical* one (fixity). |
| `Support`, `LoadCase`, `Load` | Analysis inputs, stored in the model so the IR is complete (M2 consumes them). |

### Member placement

### Frames and directions

Everything is placed through an explicit tree of frames:

* **World**: canonical, +x east, +y north, +z up. Compass names (`east`,
  `west`, `north`, `south`, `up`, `down`) always resolve here.
* **Building**: a convenience frame placed in the world by an origin and the
  compass bearing of its +x axis (`Building.placed({ origin, xBearing })`).
  A `Site` holds several peer buildings; each is built in its own frame and
  then placed, so buildings never share nodes by accident. The building is
  the root group of its members' paths (`Garage/T2/bottom_chord.F-H1`).
* **Groups** (walls, trusses, roofs, floors, assemblies) have frames within
  their building, and may **name directions** in their frame: a wall's
  `inside`/`outside`/`along`, a truss's `span`/`normal`, a roof's
  `ridge`/`span`, a floor's `joists`/`across`, or any building direction set
  with `Building.direction("street", [0, -1, 0])`. A name resolves in a group,
  then up through its ancestors, then as a compass name; axis names `±x/±y/±z`
  are always that frame's axes.
* **Members** have a canonical frame: `x` with the grain (first → last path
  node), `z` the depth direction (`depth_dir` projected ⊥ x; by default the
  parent group's z, or its y for a member parallel to that z), `y = z × x`
  across the thickness. The section lives in `(y, z)` (also called `(u, v)`).
  `Anchor(u, v) ∈ [-½, ½]²` names the point of the section's bounding box that
  lies **on** the axis, so `Anchor(0, -½)` puts the axis on the −z face and
  the body toward +z.

Faces are named in the member frame: `±x` the cut ends, `±y` the wide faces,
`±z` the narrow edges (the legacy `start/end`, `front/back`, `bottom/top` are
aliases). Names never depend on which way is up; `facing(m, "north")` or
`facing(stud, "inside", { in: "Wall A" })` selects the face whose outward
normal is closest (within 45°) to a direction named in any frame. The UI
labels faces canonically with a gloss (`+z edge of T2/bottom_chord (up)`) and
draws the member's axes (x red, y green, z blue) on hover.

**Convention for framing: axes lie on contact/reference surfaces.** Wall
nodes lie on the exterior face of framing at the bottom of the bottom plate and
the top of the top plate; joist axes lie on their bottom face. Consequently a
joist crossing a top plate *shares a node* with it (X-junction = bearing), and
a stud of a wall of height H comes out at H − 3×1½" (the familiar 92⅝" precut
for a 97⅛" wall) without anyone typing that number.

## 3. Topology (`topo-core::topology`)

For each node, the incidences are classified as **end** (member starts/ends
there) or **through** (interior path node). Junction kinds:

| ends | through | kind |
|---|---|---|
| 1 | 0 | Free (dangling end) |
| 0 | 1 | Pass (load/attachment point) |
| 2 collinear | 0 | Splice |
| 2 angled | 0 | Corner (L) |
| ≥1 | ≥1 | Tee |
| 0 | ≥2 | Cross (X — stacked bearing or lap) |
| ≥3 | 0 | Complex |

Validation reports: non-collinear or non-monotonic paths, zero-length segments,
coincident-but-unconnected nodes (the #1 modelling error: "you meant these to
connect"), dangling ends, bad references, and geometric clashes (below).

## 4. Geometry (`topo-geom`)

Each member end gets a clip plane from its junction, using **ray casts of the
member's centroid line against the other members' convex section envelopes
(infinite prisms)**:

1. JointSpec rule, if any.
2. **Butt**: among other members at the node that are *through* members, or
   *end* members of higher rank (priority, then lower id), take the first
   envelope face the ray enters → cut on that face. (Stud onto plate, joist
   into rim, header into king stud, rafter plumb-cut against ridge.)
3. **Through (L)**: otherwise, extend to the farthest exit face of lower-ranked
   end members hit (plate running past the butting plate at a wall corner).
4. **Square**: otherwise, cut ⊥ axis at the node.
5. **Miter** only by explicit rule.

Every face the ray enters near the node becomes a cut plane: the first is the
primary cut, the rest make a **compound cut** (e.g. a Fink web fitted under
both top chords at the apex). The solid is exact: the section prism
intersected with all end half-spaces, per ply (still convex per ply). This covers
square, bevel, plumb and miter cuts for arbitrary profiles (incl. non-convex
T-slot extrusions). Non-planar features (notches, birdsmouths, laps, holes) are
a planned `Feature` list of subtractive solids.

**Clash detection**: separating-axis test between convex member solids,
touching faces allowed. Generators are tested to produce zero clashes.

## 5. Drawings (`topo-draw`)

* **Views** = projection frame + member set + annotation scale.
  - Elevation of a group (wall, looking from exterior), plan (framing plan),
    isometric, and an **analytical/topology diagram** (centrelines, node ids,
    junction glyphs, supports) for the engineer to check connectivity.
* **Exact hidden-line removal** per ply (built-up members are not convex as a
  whole, and plies hide each other): front-facing face
  edges are clipped against the projected silhouettes (convex polygons) of
  members proven to be in front via their separating axis. Hidden segments go
  to a HIDDEN layer (dashed) — e.g. plates under joists in a framing plan.
* Annotations: dimensions (architectural ticks, ft-in-fractions or mm), member
  marks, notes; generators emit semantic annotations (opening RO sizes, header
  heights) in group coordinates.
* **Schedules**: member/cut list (mark, qty, size, grade, cut length, board
  feet), connection schedule, junction summary.
* **Sheets**: border + title block (project, sheet no., scale, basis of design,
  "not for construction" status) with views packed at standard architectural
  scales.
* **Sheets as code.** The drawing set is part of the model (`Model::sheets`,
  kept in the JSON IR): an ordered list of generated sets (`cover`, `plans`,
  `trusses`, `walls`, `analytical`, `schedules`) and script-defined sheets of
  views — `plan`, `elevation`, `iso`, `detail` (an enlarged region around a
  feature, clipped to a circle), `analytical`, `schedule`, `notes` — each
  naming what it draws by group/member path, the side it is seen from by a
  named direction (default: the drawn group's −y, e.g. a wall's outside), and
  optionally a standard scale. Drawn groups' dimensions appear where they are
  true length in the view. Without a list, the standard set is drawn. View
  problems (unknown target, scale or table) are reported and drawn in place;
  the rest of the set still builds. In a site, the site's list comes first
  and buildings' own custom sheets follow.
* Backends: SVG (sheets) and DXF R12 (full-scale model-space, layered: `FRAMING`,
  `HIDDEN`, `CENTER`, `DIMS`, `TEXT`, `TITLE`).

## 6. Analysis (`topo-analysis`, reference data in `topo-data`)

Gravity analysis follows how an engineer checks light-frame wood by hand, so
every number can be reviewed; a plane-frame solver handles trusses.

* **Reference data with provenance** (`topo-data`): JSON tables compiled in
  (so they work in WASM) — NDS Supplement Table 4A values, size and flat-use
  factors, load-duration factors, nail dimensions and F_yb, NDS Table 12N
  values, dead loads of assemblies. Each table cites publication, edition and
  table; each row says whether it was verified against that source. Checks
  carry the citation, and unverified values are flagged in the UI and on
  sheets. Scripts read tables too (`layers(...)` builds a dead load from the
  material-weights table).
* **Loads**: load cases by kind; area loads on roof/floor groups (on plan or
  sloped surface) go to their receiving members (truss top chords, rafters,
  joists) by tributary width; member line loads, point loads, self-weight
  (switchable). `Building.designBasis({ asce7 })` selects the ASCE 7 edition,
  which sets the ASD combinations (7-22 ground snow is strength-level: 0.7S).
  Site hazards live in a sourced, per-jurisdiction file
  (`examples/site-template.ts`, from the county's IRC Table R301.2(1) and the
  USGS seismic service); `roofSnow(site, { ce, ct })` derives p_f with its
  equation and source in the load's label.
* **Takedown** (`takedown`): carriers are processed top-down in support-graph
  order. Beams are simple spans between adjacent supports (overhangs join the
  end span), reactions by statics, V/M by free body, deflection by integrating
  M/EI; columns take everything above to their base; trusses are solved as
  plane frames (chords continuous, members pinned to each other). Reactions
  become point loads on the supporting member. Nailed transfers are relied on
  only where there is no bearing nearby. A member lying along another (a
  double top plate) bends with it, sharing moment by stiffness; face-to-face
  contact passes load straight through. Each case is taken down separately;
  combinations superpose. Equilibrium (foundation reactions = applied load)
  is tested on every example.
* **Checks** (`nds_asd`, NDS 2018 ASD): bending (C_D, C_F, C_fu, C_r, C_L = 1
  assumed braced), shear, deflection (total L/240; live L/360, snow/roof live
  L/240 per IRC R301.7), bearing perpendicular to grain, column stability
  (C_P; wall studs braced on the weak axis), combined stresses in truss
  members (indicative only: trusses are designed by their manufacturer), and
  nailed connections by the NDS yield-limit equations (reproducing every
  Table 12N value held) with C_eg / C_tn. Every check returns a calculation
  trace: symbol, formula, substituted values, result, clause, source.
* **Benchmarks** (`examples/benchmarks/`): textbook-style problems with the
  hand calculation in their header, recomputed independently in tests, plus
  the AWC span tables (2x10 @ 16", 16'-5": f_b ≈ 1,255 psi, Δ_L ≈ L/360).
* **Presentation**: in `topo serve`, a Structure panel (summary, worst
  members, colour-by utilization/bending/shear/…, per combination) and a
  member inspector (checks with expandable traces, M/V/N/deflection
  diagrams). On sheets: `utilization(...)` views (members filled by ratio
  band, labelled, with legend) and `schedule("checks")` /
  `schedule("reactions")` tables for the engineer and permit set.

Next: lateral loads (wind, seismic), sheathing and shear walls (SDPWS),
hold-downs and straps from part catalogs, and a 3D frame solver for irregular
and as-built conditions.

## 7. Crates

| crate | role |
|---|---|
| `topo-core` | math, units, ids, model IR, builder API, topology, validation |
| `topo-geom` | junction resolution, member solids, clash detection, mesh (OBJ) |
| `topo-draw` | views, HLR, annotations, schedules, sheets, SVG + DXF writers |
| `topo-timber` | lumber catalog, NDS reference data, connection presets, wall/floor generators, examples |
| `topo-analysis` | analytical-model extraction, load/combination types, `DesignCode` trait, `CalcTrace` (M2 grows here) |
| `topo-wasm` | wasm-bindgen façade: JSON model in → SVG/DXF/report out |
| `topo-cli` | renders examples / JSON models to files |

## 8. Scripting: functional TypeScript

Decision: models are written in **TypeScript** as immutable values with
builder methods that return modified copies (`wall.opening(...)`,
`perimeter.to(...)`). Parameters are ordinary `const`s and functions; design
variants (existing vs. proposed) are functions of each other.

* **Describe, then build.** Scripts produce plain immutable descriptions; one
  `build()` step expands them into the Model IR (shared nodes need a single
  mutable model).
* **Generators stay in Rust** (walls, perimeters, floors, trusses); TS types
  are generated from the Rust spec structs. User generators can be written in
  TS from low-level primitives (`node`, `member`, `connect`) and compose with
  the built-ins.
* **Runtimes.** Native: embedded QuickJS (`rquickjs`) with `oxc` stripping
  types (`topo-script` crate, `topo run model.ts out/`); ≈2.5 ms transpile+run,
  no V8. Browser: the page's own JS engine calling the WASM core. Type-checking
  is the editor's / `tsc --noEmit`'s job; the runtime only strips types.
* **Units** are branded `Length` values (`ft`, `inch`, `ftIn`).
* **Footprints are paths.** `Perimeter` takes a plan path; each segment is a
  wall template or an open side. Orientation, convex/reflex corners, which wall
  runs through, and angle-dependent layout insets are derived.
* **Top plates lap** at corners and tees (IRC R602.3.2): the double top plate
  is two members; at each corner the lower ply of one wall and the cap of the
  other run through. Partitions lap their cap over the main wall's lower plate
  at a cap break.
* **Trusses are data** (`TrussShape`): named points in the truss plane and
  members as paths. Standard types (Fink, fan, king post, Howe, Pratt,
  scissors, mono) are Rust generators exposed to scripts, so TS shapes are the
  Rust shapes and can be edited (`withoutWeb`, `withChordPoint`, `web`,
  `sized`) or written from scratch (`custom`, `symmetric` from a left half).
* **Assemblies** are arbitrary custom topology in a local frame, placed with
  `placed`/`moved`/`rotated` and duplicated with `repeat`; copies that share
  points connect automatically. Defaults (e.g. section orientation) are
  computed in the local frame, so a moved or rotated copy is identical.
* **Plated/hardware joints** are exempt from the bearing check (the hardware
  carries the load; fit-up gaps such as at a scissors apex are expected).
* **Stable names** for generated pieces: members have paths such as
  `Existing trusses/T2/bottom_chord.F-H1`, matched by suffix.

## 8a. Field measurements and fitting

Measurements relate *physical features* rather than topology nodes (which
are often not measurable). A feature is the intersection of 1–3 named planes
of member solids — faces (`±x` ends, `±y` wide faces, `±z` edges, or
`facing` a named direction) or mid-planes — so "where the top chord's upper
edge meets the bottom chord's upper edge" is a precise, stable reference.
Quantities: horizontal/vertical distance (direction inferred from the
features; an error if ambiguous), distance `along` a named direction in any
frame, member length (lumber or centreline), and `riseOver` a horizontal run
(a level held on a sloped member).

A script declares `unknown(...)` parameters and `measured(...)` readings;
`topo run` fits the unknowns by Levenberg–Marquardt, re-evaluating the script
per trial, and reports values ± 1σ, misfits, and undetermined unknowns. The
script stays the source of truth: there is no separate solved state.

`topo serve model.ts` is the visual companion: pick faces, corners and
members on the drawings (server-side hit testing against the exact solids),
see the model's value, type the tape reading, and it is appended as readable
TypeScript to `model.measured.ts`. File changes trigger a re-fit; measurements
are drawn on the sheets coloured by fit. The tabs are the model's sheets, plus
a **Script** tab: an editor for the model file and the local files it imports
(and its sidecar); saving rebuilds, and a syntax error marks its line. Only
those files can be written.

## 9. Roadmap

* **M1 (this):** core IR + topology + geometry + clash + SVG/DXF sheets +
  schedules; timber walls (openings, corners, T-walls) and floor platforms;
  analytical extraction; WASM build.
* **M2:** loads → tributary takedown → NDS member checks + ASCE 7 ASD combos →
  calc report; frame solver.
* **M3:** features (notches, birdsmouth, drilled holes), roofs, extrusion
  domain crate (T-slot profiles, brackets), DSL front-end, 3D web viewer.
