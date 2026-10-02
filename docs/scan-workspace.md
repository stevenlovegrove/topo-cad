# The scan workspace

A topo-cad model is a **hypothesis** about a building. A lidar scan is
**evidence**. The scan workspace is where people and LLM agents work the
one against the other: they look, raise issues pinned to places, propose
changes to the model as diffs, and apply the ones that hold up.

This document is the agreed design. It records decisions, and says which
parts exist. The code is `crates/topo-scan` (the computation) and, as it
is built, the workspace service in `topo serve` and the web app in `web/`.

## Principles

- **Evidence, not verdicts.** Tools report what the scan shows (points on
  faces, how far rays reach, surfaces, stud-like runs) with what is needed
  to weigh it. What a surface *is* (a stud's face, sheathing, a finish,
  clutter) is a judgement a participant states; a fit then carries its
  stated support.
- **One source of truth.** The model script is edited by diffs. Nothing
  else holds dimensions.
- **Nobody's view is taken over.** An agent never moves a person's camera.
  It leaves links; the person follows them and can go back.
- **The app works without the service.** With no service it is the static
  app it is today.

## Architecture

```
browser (static app: page + wasm engine)
   │  SSE: workspace state, pushed          POST: actions
   │  GET: point data (binary)              PUT /files: script saves
   ▼
topo serve MODEL SCAN        one process, one port
   ├─ the app's static files and /files (as today)
   ├─ /api/…   the workspace: scan, placement, evidence, issues, proposals
   └─ /mcp     the same tools, for agents (JSON-RPC over HTTP POST)
```

- **Optional service.** `topo serve` grows the workspace when given a
  scan. The app probes for it on load; absent, nothing changes.
- **Transport.** Server-sent events push state; the browser POSTs actions
  and fetches point data as binary. No async runtime.
- **One code path.** A person's click and an agent's tool call run the
  same tool as a named participant, and land in the same record.
- **Agents attach over MCP.** MCP cannot push, so an agent waits with a
  blocking tool for activity addressed to it. The tool table is
  transport-neutral, so a loop in the service that calls a model API can
  be added later.
- **Where the work runs.** The service holds the full cloud and does all
  computation (evidence, fits, picks, stud detection). The browser gets a
  decimated cloud for 3D and full-density points for a section's slab on
  demand.

## The project is a git repository

A scan project is always a git repository.

- **A proposal is a commit** on its own ref, made from a text diff
  against the project's files. It carries the evidence change it causes.
- **Applying** a proposal puts it onto the working branch (proposed by its
  author, applied by whoever applied it). **Undo** is a revert.
- **Issues are files in the repository**, one per issue, so their threads
  have history and merge cleanly.
- The scan is referenced by path, not copied in.

## Participants

Everyone who acts is a participant: a name, a kind (person or agent), a
colour and an initial badge. Comments, proposals, status changes and tool
calls carry one.

- A person gets a random word as a name on first use, and changes it from
  their badge. There is no login: the service is local.
- An agent is named by its MCP connection.
- Presence is per participant (here, working, waiting, away), shown as a
  row of badges.
- Several people take part as browsers on one running service.

## Chat

Beside the issues there is plain conversation: one participant with
another (a person with an agent, people with each other).

- Clicking someone's badge in the header opens a chat with them: what was
  said, and a box to say more. A badge shows a bell when there is
  something unread from them (what is read is each browser's own).
- An agent's badge has a robot's head in it and a ring round it, there
  and beside everything it says in a thread.
- To an agent, a message arrives from `wait` ("chat from NAME: …") like
  anything else that concerns it, and it answers with the `chat` tool. An
  agent that is not listening gets it when it next is; the panel says so.
- What a message names (members, selections, issues) is a link, as in a
  thread.
- Messages are kept beside the repository's history (in its git
  directory), not in it: they survive a restart and are not commits.
  What is about a place in the building, or needs deciding, belongs in
  an issue.

## Issues

An issue is a question or a fault pinned to a place, with a thread.

- **Where:** a point, a selection of points, a box, or a member. (In a
  scan view: click picks a point; shift-drag selects; drag orbits, and
  right- or alt-drag pans.)
- **State:** status (open, in hand, waiting on someone, done), priority,
  assignee, category.
- **Thread:** comments by participants. Tool calls show as collapsed
  lines, evidence as cards with its numbers and uncertainty, proposals as
  items to select. A comment can carry a view link.
- **Categories.** `check`: generated by the model's own checks
  (validation, clashes, bearing, load path, capacity), keyed by kind and
  member paths; it resolves when the check passes, and is kept once
  someone comments on it. `scan` and `note`: raised by participants.
- **General** is the thread pinned nowhere.
- Agents work issues in order (in hand, then open by priority), take one
  by assigning it, and ask in its thread.

## Views and view links

A **view state** is one object: the slab the view is clipped to (if any:
that is what a section is), camera and projection, point colouring
(density, depth, normal, intensity), point filters (height cuts, slab),
layer visibility, highlighted members, marks, and the diff display of a
selected proposal.

- A **link** carries a partial view state. Following it sets only the
  fields it names, in the focused pane, and leaves the rest.
- Every change by a link pushes the previous state on a history: **Back**
  and **Forward** return to it.
- A saved view is a named view state.
- A person can attach their current view to a comment, which is how an
  agent learns what they were looking at.
- Links appear only in issue threads.

## Proposals as diffs

A proposal belongs to an issue. Selecting it shows it, one at a time:

- as a **text diff** in a script pane;
- in the 3D views (clipped to a slab or not), with a switch: **before**, **after**,
  **changed only**, **both**. Changed members are coloured by kind
  (added, removed, moved);
- with its evidence change (members supported, contradicted, unseen, and
  per group).

Apply or Reject is recorded in the thread.

## Layout

- **Top bar:** project and branch, view history (Back, Forward),
  participants.
- **Left rail:** a browser of what the project has: layers (scan, model,
  surfaces, rooms, selections), drawings (grouped: plans, elevations,
  details, schedules), files, saved views.
- **Centre: panes.** The area splits into one or more panes. Each picks
  its kind: 3D with the scan (up to three of them), drawing, script,
  board. A script beside a 3D view, a sheet beside its script, or a 3D
  view beside the same scan seen through a slab, is a two-pane
  arrangement.
  Panes are arranged as a tree of splits, side by side or one over
  another. The bar between two is dragged to resize them (double-click:
  equal). A pane is dragged by its title bar onto another: onto its
  left, right, top or bottom to stand there, onto its middle to change
  places with it. The arrangement is remembered in the browser.
- **Right:** issues (list, then the thread), member, loads, measure.
- **Board:** a strip under the panes that expands, or a pane. Columns by
  status; drag between columns, and within one for priority.

The exact hidden-line drawings stay as they are, as the drawing pane. The
3D pane with the scan is a new WebGL2 view beside them, until it can
match their quality.

## What an agent sees

- Sections, rendered in the service (`topo_scan::render`).
- 3D pictures (`render_view`), drawn offscreen in the service by
  Pangolino's `pango-render`: the thinned scan, coloured by intensity,
  height or normal, with the model's edges by evidence; of the whole
  model, a member or an issue; perspective or top-down; with height cuts.
  They are for finding where to look; sections are what is measured in.
  Never the person's own pane.

  This is the cargo feature `pango` (`cargo build --release -p topo-cli
  --features pango`). Pangolino is not published yet, so `topo-scan` names
  it by a local path: until it is, the workspace only builds on a machine
  with that checkout, feature or not. Without the feature the tool says
  it is not there.

## Running it

```
./web/build.sh                                     # the app's engine (once)
cargo build --release -p topo-cli
target/release/topo serve MODEL.ts SCAN.ply        # http://127.0.0.1:8765/
```

- The model's folder must be in a git repository (`git init` there).
- Folders the model imports from outside its own are given too, so the
  app can read them: `topo serve MODEL.ts ../../examples SCAN.ply`.
- The scan is a binary PLY; a trajectory beside it (`full.tum` or
  `odom.tum`) gives the sensor rays. After the first run the project
  remembers its scan: `topo serve MODEL.ts --scan`.
- An agent attaches over MCP, under a name of its own:

  ```
  claude mcp add --transport http topo http://127.0.0.1:8765/mcp/claude-1
  ```

Without a scan, `topo serve` and any static host serve the app as before.

## How it is built

| Where | What |
|---|---|
| `crates/topo-scan` | The computation and the workspace. `scan`, `ply`, `trajectory`, `axes`: loading a scan. `lib`: prediction, registration, evidence. `fit`, `render`, `cloud`: plane fits, section images, the thinned cloud and slabs. `git`, `issues`: the project as a repository. `workspace`, `tools`, `view`: the workspace, its tools, and what the app draws from. `mcp`: the tools for agents. |
| `crates/topo-cli/src/work.rs` | The service: `/api/…` and `/mcp` beside the app's files. |
| `web/app/service.js` | The app's client of the service (absent: the app is as before). |
| `web/app/glview.js` | A WebGL2 view: an orbit camera over the points, optionally clipped to a box (a slab), with handles to drag. |
| `web/app/scan.js` | The scan's panes, layers, views, issues, threads, proposals, board, people. |
| `web/index.html` | The shell: the browser of drawings and files, the panes, the inspector. |

Details that were decided in the building:

- **A proposal** is made with a scratch index, so the working tree and
  index are untouched until it is applied. It is applied by cherry-pick:
  its author stays the proposer, its committer is whoever applied it. One
  that no longer applies changes nothing and says so.
- **Edits made outside the app** are committed as such before a proposal
  is applied or reverted.
- **Every change to an issue** is a commit on the working branch
  (`#4: comment`).
- **Waiting.** A comment ending in a question mark, from someone other
  than the issue's raiser, leaves the issue waiting on the raiser; their
  reply hands it back. Taking an issue in hand assigns it.
- **What concerns an agent** (what `wait` returns): issues it raised, is
  assigned or took part in, issues nobody has taken, and `@name` mentions.
- **A tool called with `issue: N`** is noted in that issue's thread.
- **The browser draws** a cloud thinned to one point per 2.5 cm cube (with
  intensity and a surface normal per point), and asks for every point in
  a slab when a view is clipped to it or it changes.
- **A selection** is kept as how it was made (the window and the view it
  was dragged in, or a box), not as a list of points: it is small, lives
  in the repository, and gives the same points again from the same scan.
  Shift-drag in a scan view makes one. In 3D only the surface nearest the
  viewer is taken (within 8 cm of the nearest point around it), so a
  surface seen at a grazing angle is taken thinly; in a view clipped to a
  slab, all of the slab's points in the window. Alt-shift-drag, or the
  view's "selects everything in the window" (under ⋯), takes all that is
  behind the nearest surface too: for scattered points with no surface. `measure`, `fit_surface` and an
  issue's place take a selection by its id.
- **Pointing at things is one flow.** In a scan view a click picks a
  point (and the member there), shift-drag selects a region, and a
  right-click opens a menu for what is there. The chip says what is
  pointed at: point, member, selection, plane. An issue is raised on any
  of them the same way (from the chip, the menu or the Issues tab), and
  its draft says what it is pinned to, to change before it is raised.
- **The objects are one tree** (the browser's Objects tab): the scan, the
  model's groups and members (each with its evidence as a dot), issues,
  selections, planes, slabs, polylines, rooms. Groups fold. Each row has an eye, whether it
  is drawn, which is apart from clicking the row, which selects it (the
  chip, the Member tab and a new issue then refer to it). Hiding a group
  hides what is in it. Double-click goes to it; right-click offers what
  can be done with it (an issue on it, name, delete, hide, fit, slab).
- **A scan view's own controls are in the view**: how the points are
  coloured, what it shows (the whole scan, or a slab), orthographic,
  top-down, fit, drawing a polyline, cutting a slab, cutting away the
  roof or the floor, and under ⋯ the point size and the exact height
  cut; clipped to a slab, looking square on,
  whether it is held so, and the slab's thickness and stepping. Each has
  a compass: click a letter to look from that side of the model, the
  middle to look straight down.
- **One thing is selected at a time**: a point (a click in a view), a
  member, a selection, a plane, a slab or a polyline, or an issue. The card over the views
  says what it is in a line or two, with three small buttons (an issue on
  it, its menu, let go). Its menu is the same wherever it is opened: on
  its row in the tree, on the card, or by right-clicking in a view (which
  also offers what can be done at the point under the cursor, without
  changing what is selected).
- **Being taken somewhere is seen**: going to an object (double-click its
  row, or a pin), following a link, Back, the compass and Fit all fly the
  camera there over about half a second. A drag or the wheel stops it.
- **The wheel zooms toward what the cursor is on**: the scan point under
  it (asked of the service when the cursor comes to rest somewhere), so
  that point stays under the cursor; until it is known, the point at the
  depth of the view's target.
- **Selecting a member** (in a view or the tree) fills the Member tab:
  what the scan shows of it, then its structural checks.
- **Selections, planes, slabs and polylines are things with ids** (`s3`,
  `p2`, `b1`, `l4`): shown, named in place, deleted, and referred to. An id written in a reply
  becomes a link that shows the thing and goes to it; an issue's number
  (`#4`) likewise. A member named in a reply is a link too, which
  selects it and goes to it: by its group and name (`West wall/stud#3`),
  or by its name alone (`stud#3`), which is taken as the only member of
  that name, else the one in the group named nearest before it in the
  text, else in the group of the member the issue is on, else the one
  nearest the issue's place.
- **A plane** is a fit kept: to a selection's points, or about a picked
  point ("Fit a plane" in the menu; `fit_surface` with `keep` for
  agents).
- **A section is a 3D view clipped to a slab.** There is no section pane:
  a slab is an object (an oriented box in the model's frame: a centre,
  three axes — right, up, toward the viewer — and half sizes), kept with
  the project, and any of the three scan views can be clipped to one
  (its "Whole scan / Slab …" menu). "View it" (or a double-click on its
  row) opens a second view on it, seen square on and held so (a drag
  slides the view; the lock in its toolbar lets it turn), drawing every
  scan point in the box. Up to three panes: a view already on the slab
  is used, else a second view not open, else the second view that is.
- **Making a slab is pointing at what to see.** Right-click a point:
  along the surface there, cut through that surface, a plan at that
  height, a cut across x or y. On a selection: around it. On a plane: on
  it. With the cut tool: a line dragged across the building, seen from
  above. From a view's menu: a plan or a cut through what it looks at.
  Slabs stand upright (turned only about the vertical) and square to the
  model when within 5° of it, unless made from a tilted surface; they
  reach over the model and are 0.1 to 0.2 m thick to start.
- **A slab is changed by its handles**, in any view while it is selected:
  each face drags along its normal (the one across from it stays), the
  middle slides it along its thin direction, ⟳ turns it about the
  vertical in 5° steps (alt: freely); its sizes can be typed in the
  card. A view clipped to it follows as it is dragged. A handle that
  would move toward or away from the viewer is not shown.
- **The scan's points nearer the sensor than a distance are left out**
  (1 m unless set): whoever carried the scanner. A point's range is as
  the file gives it, else its distance from where the trajectory says
  the sensor was. The distance is set from the Scan row's right-click
  menu (or by `scan_filter`), is kept with the project
  (`<model>.scan.json`), and applies to everyone and everything made
  from the points; changing it loads the scan again in the background.
- **Free space can be seen in a slab.** A view clipped to a slab has a
  free-space switch (when the scan has a trajectory): under the points,
  a picture over the slab's middle plane of where the scanner's rays
  pass inside it, brighter where more do. Every ray is taken: the part
  of it inside the slab is drawn into a grid, each cell counting the
  rays over it; the counts are shown on a logarithmic scale. The grid
  covers what the view sees of the slab, a cell to a pixel (no finer
  than 1 mm, no more than 2048 across), and is made again when the view
  comes to rest: zoomed in close, single rays are seen as lines. Where it is dark, something was in the way or nothing looked;
  a streak through a wall is an opening, or rays that should not be
  there. The service makes it (`/api/free`, about a third of a second
  for 14 million rays) and it follows the slab as it is dragged.
- **A selection's points can be ignored**: left out of the scan for
  everyone (not drawn, picked or counted as evidence), for points that
  are not of the building as it is: reflections, something that was
  moved. It is the selection's "Ignore its points" (`selections` with
  `ignore` for agents). The selection stays, marked ignored, still shows
  its points while its eye is on, and can be put back; deleting it puts
  them back too. Like the distance from the sensor, it is kept with the
  project and the scan is loaded again when it changes. Ignoring is for
  what is not real, not for what disagrees with the model: the reason
  belongs in an issue.
- **What contradicts a member can be seen.** "Contradicted" is about
  rays, not points on the member: the scanner saw things along lines
  that pass through where it is. A member's menu has "Show the points
  seen through it" (`selections` with `through` for agents): a selection
  of the points whose rays pass through the member and end beyond it.
  While it is selected, a sample of those rays is drawn from the sensor
  to the points. (The evidence counts every fourth ray; the selection
  takes them all.)
- **A click in a view selects what is drawn there**: a slab, plane or
  polyline by its lines (within a few pixels of its outline); a shown
  selection by its points; a shown member by its faces as drawn (the
  one the ray through the cursor meets first), unless the scan's surface
  there is more than 0.15 m in front of it, which hides it; else the
  scan point. Alt-click always takes the point. A right-click on a slab's,
  plane's or polyline's lines opens its menu. What
  is hidden (its eye, or its group's) is not drawn, has no handles and
  cannot be picked, selected or not. The selected slab's faces and the
  selected plane's patch are drawn faintly (a slab's more strongly while
  it is dragged): points in front hide them and points behind are
  tinted, which shows where they meet the scan.
- **A section in a link** (`section`, as `render_section` takes it) opens
  as a slab that is not kept, until someone keeps it; a link can name a
  kept slab instead (`slab`). A person's attached view carries both.
- **A polyline** is a run of points in the model's frame, open or closed,
  with its length (and each stretch's). It is drawn in a view point by
  point: on the scan under the cursor, or, square on to a slab, in the
  slab's middle plane (so over a plan or a section it is flat). Enter or
  a double-click ends it, the first point again closes it, Esc drops it.
  Selected, its points drag (landing on the scan, or staying in the
  slab's plane), a stretch's length drags out a new point, and a point's
  right-click removes it. Agents have the same (`polylines`).
- **The model over the points** is coloured by evidence where the points
  are grey (intensity), and drawn in white outline where the points are
  coloured by hue (depth, normal, height).

## Status

| Part | State |
|---|---|
| `topo-scan`: scan loading, registration, evidence, plane fits, section images, thinned cloud | built, tested |
| The project as a git repository: proposals, apply, revert, issues as files | built, tested |
| The workspace and its tools; MCP | built, tested (MCP by hand-made requests, not yet by a model) |
| The service in `topo serve` | built, run on a real scan |
| The app's shell: browser, split panes, inspector | built |
| 3D with scan, the object tree, picks | built |
| Slabs: made by pointing, shown in a clipped 3D view, changed by handles | built, tried by hand (no automated test of the browser side) |
| Polylines: drawn, points dragged, added and removed | built, tried by hand |
| Selections (a window in a view, or a box) | built, tested (the window by scripted events) |
| Issues: list, threads, typing in place, pins, board with drag | built |
| View links, Back and Forward, saved views | built |
| Proposals: text diff, before / after / changed / both, apply, reject, revert | built |
| Participants: names, badges, presence | built |
| 3D pictures for agents (`render_view`, feature `pango`) | built, run on a real scan |

Not built yet:

- **Painted selections.** Selections are made by a window (or a box).
- **Checks as issues.** The model's checks are listed as a category while
  they fail, and one can be kept as an issue to discuss; a kept one does
  not yet close itself when the check passes.
- **The board as a strip** under the panes: it is a pane kind.
- **Evidence as a "Colour by" on the drawings**: it is in the scan views,
  the tree and the Member tab.
- **A proposal's diff in the script pane**: it is in the thread.
- **Placement by hand** in the app: agents have the placement tools.
- **Drawings grouped** (plans, elevations, schedules) in the browser:
  they are one list.

## `topo-scan`

```
cargo run --release -p topo-scan --example evidence -- MODEL.ts SCAN.ply
```

loads a model and a scan, registers the model in the scan and prints the
evidence per group of members.

- **Scan:** a binary PLY (x, y, z; optionally intensity, time, range). A
  trajectory beside it gives the sensor position for each point, which
  turns points into rays. Only `trajectory.rs` knows how a trajectory is
  stored (today a TUM text file).
- **Placement:** the model in the scan is a turn about the vertical and a
  translation (both frames are gravity-aligned).
- **Registration:** four yaw candidates from the scan's wall direction,
  each refined by robust point-to-plane ICP onto points sampled on the
  model's faces.
- **Evidence per member:** scan points on its faces (count, density, mean
  and RMS offset along the face normal), and sensor rays that pass
  through its solid or end on it.
- **Plane fits** (`fit.rs`): a plane from a seed with stated support, and
  what is needed to judge it (inliers, spread, coverage, offset by
  quadrant, other surfaces across it).
