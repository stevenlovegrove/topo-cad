//! The workspace's tools for agents, as MCP (JSON-RPC 2.0): each message
//! is answered with JSON (the "streamable HTTP" transport at its simplest:
//! a POST per message, no event stream). An agent is a participant like
//! any other; it cannot be pushed to, so it listens with the `wait` tool.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::workspace::{Workspace, INSTRUCTIONS};

/// Answers one JSON-RPC message for the agent `who`; None for a notification.
pub fn handle(ws: &Mutex<Workspace>, who: &str, msg: &Value) -> Option<Value> {
    let id = msg.get("id")?.clone();
    let method = msg["method"].as_str().unwrap_or("");
    let params = &msg["params"];
    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": params.get("protocolVersion").and_then(|v| v.as_str()).unwrap_or("2025-06-18"),
            "capabilities": {"tools": {"listChanged": false}},
            "serverInfo": {"name": "topo-cad", "version": env!("CARGO_PKG_VERSION")},
            "instructions": INSTRUCTIONS,
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": tools()})),
        "tools/call" => {
            let name = params["name"].as_str().unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            Ok(match call(ws, who, "agent", name, &args) {
                Ok(content) => json!({"content": content}),
                Err(e) => json!({"content": [{"type": "text", "text": format!("error: {e:#}")}], "isError": true}),
            })
        }
        m => Err(json!({"code": -32601, "message": format!("method not found: {m}")})),
    };
    Some(match result {
        Ok(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
        Err(e) => json!({"jsonrpc": "2.0", "id": id, "error": e}),
    })
}

/// Runs a tool as a participant (an agent or a person). `wait` blocks
/// here, without holding the workspace, until others have done something.
pub fn call(ws: &Mutex<Workspace>, who: &str, kind: &str, name: &str, args: &Value) -> anyhow::Result<Vec<Value>> {
    ws.lock().unwrap().join(who, kind)?;
    if name == "wait" {
        let secs = args.get("timeout_s").and_then(|v| v.as_f64()).unwrap_or(300.0).min(3600.0);
        let t = Instant::now();
        ws.lock().unwrap().set_waiting(who, true);
        while t.elapsed().as_secs_f64() < secs && !ws.lock().unwrap().has_activity(who) {
            std::thread::sleep(Duration::from_millis(200));
        }
        ws.lock().unwrap().set_waiting(who, false);
    }
    ws.lock().unwrap().call(who, name, args)
}

pub fn tools() -> Value {
    let frame = json!({"type": "string", "enum": ["model", "scan"], "description": "Coordinate frame of positions (default model: z up, floor at 0)."});
    let v3 = json!({"type": "array", "items": {"type": "number"}, "minItems": 3, "maxItems": 3});
    let bx = json!({"type": "object", "properties": {"min": v3, "max": v3}, "required": ["min", "max"], "description": "Axis-aligned box in `frame`."});
    let issue = json!({"type": "integer", "description": "Note this call in that issue's thread."});
    // The model as a change would have it, for the tools that look at it.
    let alt_proposal = json!({"type": "object", "description": "Look at the model as this proposal has it.", "properties": {"issue": {"type": "integer"}, "n": {"type": "integer"}}});
    let alt_edits = json!({"type": "array", "description": "Look at the model as these edits would have it (a trial: nothing is posted).", "items": {"type": "object", "properties": {"file": {"type": "string"}, "old": {"type": "string"}, "new": {"type": "string"}}, "required": ["old", "new"]}});
    let alt_files = json!({"type": "array", "items": {"type": "object", "properties": {"file": {"type": "string"}, "text": {"type": "string"}}, "required": ["file", "text"]}});
    let view = json!({"type": "object", "description": "A partial view state for people to follow as a link: only the fields that matter. slab: a kept slab's id (their view is clipped to it, seen square on: the best way to show a section, since you can render the same slab); free: true shows free space in it; pane: \"3d\" | \"section\"; section: {view: plan|x|y|custom, at, z, thickness, origin, normal, up} (as render_section); camera: {eye, target} (model frame) or {member: path} or {issue: id}; projection: \"ortho\" | \"perspective\"; colour: height | intensity | normal | depth | evidence; clip: {z_min, z_max} (model frame, null clears); layers: {scan, model, surfaces, rooms, issues} booleans; highlight: [member paths]; proposal: {issue, n, mode: before | after | changed | both}."});
    json!([
        {"name": "status", "description": "The project: model, scan, placement and its score, evidence per member group, the most contradicted members, who is here, issues.",
         "inputSchema": {"type": "object", "properties": {}}},
        {"name": "issues", "description": "The issues: questions and faults pinned to places, each with a thread, a status (open, doing = in hand, waiting = on someone to answer, done), a priority, an assignee and a category (scan, note, check). People raise them as they see things; raise your own for what you find or need to ask. Work them in the order `list` gives: set `doing` when you start one (it becomes yours), say what you found, ask in the thread (a comment ending in a question mark leaves it waiting on whoever raised it), and set `done` only when they agree or the evidence is plain. A comment can carry a `view` for people to follow.",
         "inputSchema": {"type": "object", "properties": {
            "action": {"type": "string", "enum": ["list", "get", "create", "comment", "evidence", "update", "delete"], "description": "list (default; not done unless all), get (the thread and the place), create, comment, evidence (a card: title + text, what the scan shows and how sure), update, delete (only one raised by mistake)."},
            "id": {"type": "integer"},
            "all": {"type": "boolean", "description": "list: include done."},
            "category": {"type": "string", "enum": ["scan", "note", "check"]},
            "title": {"type": "string", "description": "create, update, evidence: a line."},
            "text": {"type": "string", "description": "comment, evidence, create: a line or two. What it names becomes a link for people: a member by its group and name as in its path (\"West wall/stud#3\", \"T7/web.T1-Cl\"; a name alone, \"stud#3\", is guessed at), a selection, plane, slab or polyline by its id (s3, p2, b1, l4), an issue by #N."},
            "view": view, "view_label": {"type": "string", "description": "What the link shows, in a few words."},
            "status": {"type": "string", "enum": ["open", "doing", "waiting", "done"]},
            "priority": {"type": "string", "enum": ["high", "normal", "low"]},
            "assignee": {"type": ["string", "null"], "description": "update: a participant's name or id; null for nobody."},
            "point": v3, "box": bx,
            "member": {"type": "string", "description": "create: a member path (or part of one)."},
            "selection": {"type": "string", "description": "create: a selection's id."},
            "plane": {"type": "string", "description": "create: a kept plane's id."},
            "frame": frame}}},
        {"name": "wait", "description": "Wait for others to do something that concerns you (a new issue, a reply in an issue that is yours or that you took part in, an @mention), up to timeout_s (default 300), and return it. The way to work with people: act, say, wait, …",
         "inputSchema": {"type": "object", "properties": {"timeout_s": {"type": "number"}}}},
        {"name": "propose", "description": "Propose a change to the model's files in an issue, as a diff: exact replacements (each `old` must occur exactly once in its file) and/or whole files. The changed model is evaluated and scored against the scan (members added and removed; supported, contradicted, unseen; evidence per group); a change that does not evaluate proposes nothing. Nothing is applied: people see the proposal in the issue as a text diff and as before / after / changed in their views. A posted proposal is read by people at once, so check it first: the scores do not say where things landed. Try the same edits in render_section (through the place) and members (their extents) and compare with what you measured; `dry: true` here gives the scores without posting. Post when it is right, then look once more with `proposal`.",
         "inputSchema": {"type": "object", "properties": {"issue": {"type": "integer"}, "title": {"type": "string"}, "dry": {"type": "boolean", "description": "A trial: evaluate and score it, post nothing."}, "edits": {"type": "array", "items": {"type": "object", "properties": {"file": {"type": "string", "description": "Relative to the model's folder (default: the model's file)."}, "old": {"type": "string"}, "new": {"type": "string"}}, "required": ["old", "new"]}}, "files": {"type": "array", "items": {"type": "object", "properties": {"file": {"type": "string"}, "text": {"type": "string"}}, "required": ["file", "text"]}}}, "required": ["issue", "title"]}},
        {"name": "proposal", "description": "A proposal of an issue: diff (its text), apply (onto the working branch; refused if the files have moved on), reject, or revert (an applied one). Apply only when a person has agreed.",
         "inputSchema": {"type": "object", "properties": {"issue": {"type": "integer"}, "n": {"type": "integer"}, "action": {"type": "string", "enum": ["diff", "apply", "reject", "revert"]}}, "required": ["issue", "n"]}},
        {"name": "seen", "description": "Faces seen in the scan, placed by hand: add one (a face of a member is where it is seen: the reading is on the plane through `point` square to `face`, in the scan's frame, so it moves with the scan), list them, or remove one (by its number). Applied at once (people place them by dragging a face in the app; it is the same reading). The solve moves whatever is free to meet it (a stud along its wall, a wall's corners, a truss) and the reply says what moved and whether the reading is met; one that nothing free can meet is said as off. Kept beside the model (`NAME.seen.ts`), which the model adds once (`.add(seen)`).",
         "inputSchema": {"type": "object", "properties": {"action": {"type": "string", "enum": ["add", "list", "remove"]}, "member": {"type": "string", "description": "add: the member's path."}, "face": v3, "point": v3, "at": {"type": "number", "description": "add: where along the member (m from its start) the reading is (default: its middle)."}, "tolerance": {"type": "number", "description": "add: m (default 0.003)."}, "view": {"type": "string", "description": "add: where it was placed (a slab's id, say), for the record."}, "n": {"type": "integer", "description": "remove: the reading's number."}}}},
        {"name": "model_read", "description": "Read the model's script (or another .ts beside it: `file`, relative to the model's folder) with line numbers; list=true lists the folder's .ts files.",
         "inputSchema": {"type": "object", "properties": {"file": {"type": "string"}, "start_line": {"type": "integer"}, "end_line": {"type": "integer"}, "list": {"type": "boolean"}}}},
        {"name": "members", "description": "Model members with their evidence: path, role, box centre and size (model frame), scan points on their faces, density, mean offset, and the share of sensor rays passing through them. filter: substring of the path, or a role. sort: path | contradicted | support. Each line ends with the member's extent along the model's x, y and z: where it is, to compare with a face measured in the scan. With `proposal` ({issue, n}) or `edits` / `files` (as propose takes them): the members as that change would have them, which is how to check where a change puts things.",
         "inputSchema": {"type": "object", "properties": {"proposal": alt_proposal, "edits": alt_edits, "files": alt_files, "filter": {"type": "string"}, "sort": {"type": "string", "enum": ["path", "contradicted", "support"]}, "limit": {"type": "integer"}, "issue": issue}}},
        {"name": "render_section", "description": "An image of the scan in a slab about a plane, with the model cut by it (coloured by evidence), on a grid labelled in metres; the text gives the exact pixel ↔ metre mapping and the members cut. view=plan (z slab, default model z 0.1–2.4: walls), x (plane x=at, image right = +y), y (plane y=at, image right = +x), or custom (origin, normal, up). color = depth | normal | intensity helps to tell members from what is behind them. To show people the same section, attach it to a comment as a `view`. `slab` (an id, b3): the section of a kept slab, exactly what people see looking through it; make the slab first (slabs) and name it in your comment, so you and they look at the same thing. `free: true` adds free space under the points: a blue tint where the scanner's rays pass inside the slab, so a member drawn over blue has rays through it. `proposal` ({issue, n}) or `edits` / `files` (as propose takes them) draws the members as that change would have them instead of as they are: render the same slab before and after to see what a change does.",
         "inputSchema": {"type": "object", "properties": {"slab": {"type": "string"}, "free": {"type": "boolean"}, "proposal": alt_proposal, "edits": alt_edits, "files": alt_files, "view": {"type": "string", "enum": ["plan", "x", "y", "custom"]}, "at": {"type": "number"}, "z": {"type": "array", "items": {"type": "number"}}, "thickness": {"type": "number"}, "origin": v3, "normal": v3, "up": v3, "bounds": {"type": "array", "items": {"type": "number"}, "description": "[u0, u1, v0, v1] in metres"}, "frame": frame, "color": {"type": "string", "enum": ["density", "depth", "normal", "intensity"], "description": "density (default); depth: offset from the cut plane of the nearest point (what stands in front of what); normal: surfaces facing the viewer grey, edge-on surfaces hued by direction; intensity: the lidar return (materials)"}, "highlight": {"type": "array", "items": {"type": "string"}}, "members": {"type": "boolean"}, "max_px": {"type": "integer"}, "save": {"type": "string", "description": "Also write the PNG here."}, "name": {"type": "string", "description": "Keep the section by this name."}, "issue": issue}}},
        {"name": "render_view", "description": "A 3D picture of the scan with the model's edges over it (coloured by evidence), to look around with: by default the whole model from the south-west, above. Look at a `member` (path or part of one; it is highlighted), an `issue`, or from `eye` to `target`; `from` is the direction toward the viewer; top_down looks straight down, orthographic, the model's +y up. clip_z / clip_z_min hide what is above / below a height (model frame), e.g. clip_z 2.3 to look in under the roof. Nobody's own view changes. Measure in sections (render_section), which are exact; a 3D picture is for finding where to look.",
         "inputSchema": {"type": "object", "properties": {"member": {"type": "string"}, "issue": {"type": "integer"}, "eye": v3, "target": v3, "from": v3, "top_down": {"type": "boolean"}, "projection": {"type": "string", "enum": ["ortho", "perspective"]}, "half_height": {"type": "number"}, "color": {"type": "string", "enum": ["intensity", "height", "normal"]}, "clip_z": {"type": "number"}, "clip_z_min": {"type": "number"}, "highlight": {"type": "array", "items": {"type": "string"}}, "members": {"type": "boolean"}, "max_px": {"type": "integer"}, "frame": frame}}},
        {"name": "measure", "description": "Statistics of the scan points in a box or a selection: count, centroid, extent, percentiles per axis, best plane (normal, offset, rms), principal spreads, and the nearest model members with their mean offsets.",
         "inputSchema": {"type": "object", "properties": {"box": bx, "selection": {"type": "string", "description": "A selection's id, in place of a box."}, "frame": frame, "issue": issue}}},
        {"name": "probe", "description": "At a point: the nearest model face (member, distance, offset along its normal) and the scan points within `radius` (count, their plane).",
         "inputSchema": {"type": "object", "properties": {"point": v3, "radius": {"type": "number"}, "frame": frame, "issue": issue}, "required": ["point"]}},
        {"name": "walls", "description": "A profile across every wall of the model (each group with studs): the wall's inside and outside faces, and where the scan has surfaces across it, with each surface's distance from both faces. Evidence to read with care, not a verdict: a peak may be stud faces, sheathing seen between studs, a finish, insulation or clutter. Use it to see which walls deserve a closer look, then fit_surface.",
         "inputSchema": {"type": "object", "properties": {"filter": {"type": "string"}, "band": {"type": "number", "description": "how far either side of the wall to look (m, default 0.6)"}, "z": {"type": "array", "items": {"type": "number"}, "description": "height range of scan points used (default [0.3, 2.0])"}, "issue": issue}}},
        {"name": "orientation", "description": "How the scan is turned against the model: one small rotation fitted over all the model's surfaces at once, against their orientations as modelled. A surface is the scan's points near a set of coplanar faces (a wall's face, a floor, the undersides of a truss's chord); where it is along its normal is left free (a finish stands off the framing), so only orientation is fitted. Reports the joint turn (a turn about the vertical, and a slope to level out, which is what scan_filter level takes), fitted over all of them, over the level ones alone and over the upright ones alone, with how much each changes when any one part of the building is left out; and each part's own turn beside it, before and after: what the joint fit cannot take is the building's (a wall that leans) or a surface that is not what the model has there. Changes nothing. filter: only parts whose path contains this are fitted (all are still listed).",
         "inputSchema": {"type": "object", "properties": {"filter": {"type": "string"}, "gate": {"type": "number", "description": "How near a model face a scan point has to be to count as of it (m, default 0.03)."}}}},
        {"name": "studs", "description": "Where the scan has studs along each wall of the model, as distances from the wall's start (the end its stud#1 is at), and the set-out that fits them: how far the scan reaches into the wall along its length, the stud-like runs, and the grid at the stud spacing that most of them fall on. Evidence, not a verdict: a finished wall shows no studs, clutter and openings hide them, and corner or door framing is off the grid. Check a set-out against a section before proposing it.",
         "inputSchema": {"type": "object", "properties": {"filter": {"type": "string", "description": "Only walls whose path contains this."}, "spacing": {"type": "number", "description": "Stud spacing, m (default 0.4064, 16 in)."}, "z": {"type": "array", "items": {"type": "number"}}, "issue": issue}}},
        {"name": "fit_surface", "description": "Fit a plane to a surface someone has said what it is, with the support you choose. Seed: a point on it, or a selection (its points are then the support). Support: `box` (or a cube of `radius` about the seed), `thickness` (points within this of the plane count), `normal` + `normal_tol_deg` (the expected facing; the fit is held to it if the points would turn it further), `cell_normals` (only points whose own 5 cm neighbourhood is a surface facing the same way: drops edges, clutter, crossing surfaces). Reports the plane, which side it was seen from, inliers, rms, coverage, the offset per quadrant (tilt, warp, two surfaces), the other surfaces across it, and the signed distances of the model's parallel faces nearby. `name` + `describes` keep it as a surface (drawn in people's views).",
         "inputSchema": {"type": "object", "properties": {"seed": v3, "selection": {"type": "string", "description": "A selection's id: its points are the support (and its middle the seed)."}, "box": bx, "radius": {"type": "number"}, "normal": v3, "normal_tol_deg": {"type": "number"}, "thickness": {"type": "number"}, "cell_normals": {"type": "boolean"}, "keep": {"type": "boolean", "description": "Keep it as a plane (p1, p2, …) without naming it yet."}, "name": {"type": "string"}, "describes": {"type": "string", "description": "what the surface is, e.g. \"sheathing seen between studs: the studs' outside face\""}, "frame": frame, "issue": issue}}},
        {"name": "selections", "description": "Sets of scan points someone marked. People make them by dragging a window in a view (the surface nearest them, unless they chose all that is behind it too) and often pin an issue to one; make your own from a box. Use a selection's id in measure and fit_surface (its points are then the support) and as an issue's place. `through` (member) takes the points seen through a member (their rays from the sensor pass through it and end beyond): which rays contradict it, to show people and to see where they end. `ignore` (id; ignore: false undoes it) leaves a selection's points out of the scan for everyone (not drawn, picked or counted as evidence): for points that are not of the building as it is, such as reflections; say why in an issue, and do not ignore points merely because they disagree with the model. list | create (box, label) | through (member) | label (id, label) | ignore (id, ignore) | delete (id).",
         "inputSchema": {"type": "object", "properties": {"action": {"type": "string", "enum": ["list", "create", "through", "label", "ignore", "delete"]}, "ignore": {"type": "boolean"}, "id": {"type": "string"}, "label": {"type": "string"}, "member": {"type": "string"}, "box": bx, "frame": frame}}},
        {"name": "slabs", "description": "Slabs: oriented boxes in the model frame that people look through (a view clipped to a slab is a section; they drag its faces to move it). Listed with their centre, size (right × up × thickness) and the direction they are seen along. Make one to show people a section and keep it: on a kept `plane`, around a `selection`, or from `centre` + `normal` (the direction toward the viewer; snapped to a model axis within 5°) + `size` [right, up, thickness] m. list | create | update (id; centre, half, name) | delete (id). Refer to one by its id (b1) in a comment, or in a view link as `slab`.",
         "inputSchema": {"type": "object", "properties": {"action": {"type": "string", "enum": ["list", "create", "update", "delete"]}, "id": {"type": "string"}, "name": {"type": "string"}, "plane": {"type": "string"}, "selection": {"type": "string"}, "centre": v3, "normal": v3, "up": v3, "size": v3, "half": v3, "thickness": {"type": "number"}, "frame": frame}}},
        {"name": "chat", "description": "Talk with one participant directly, outside any issue: people open a chat with you from your badge in the app, and what they say comes to you from `wait` as \"chat from NAME: …\". Answer with send (to: their name, text). read (with: a name; last: how many) gives what was said. For questions and conversation; what is about a place in the building or needs deciding belongs in an issue, which you can raise from here and say so.",
         "inputSchema": {"type": "object", "properties": {"action": {"type": "string", "enum": ["send", "read"]}, "to": {"type": "string"}, "text": {"type": "string"}, "with": {"type": "string"}, "last": {"type": "integer"}}}},
        {"name": "scan_filter","description": "How the scan is taken in. Which points are used: those nearer the sensor than `min_range` (m; 1 by default) are left out, which removes whoever carried the scanner. And whether it is level: a scanner's sense of up is its accelerometer's and may be a few mm per metre out, which shows as floors and ceilings sloping and plumb walls leaning the same way by the same amount; `level: [rise per metre along x, along y]` (what is level rises by that, as the scan is now; in the model's axes unless frame is \"scan\") turns the scan so that it is level, added to what was set before. Without either: what they are now. Setting one is kept with the project and loads the scan again (a minute or so), for everyone.",
         "inputSchema": {"type": "object", "properties": {"min_range": {"type": "number"}, "level": {"type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2}, "frame": frame}}},
        {"name": "polylines","description": "Polylines people trace over a view (a footprint over a plan, an edge over a section), or that you draw for them: points in the model frame, open or closed. `list` gives every point and the length of every segment, so a traced outline can be read as dimensions. list | create (points, closed, name) | update (id; points, closed, name) | delete (id). Refer to one by its id (l1) in a comment.",
         "inputSchema": {"type": "object", "properties": {"action": {"type": "string", "enum": ["list", "create", "update", "delete"]}, "id": {"type": "string"}, "name": {"type": "string"}, "points": {"type": "array", "items": v3}, "closed": {"type": "boolean"}, "frame": frame}}},
        {"name": "surfaces", "description": "The planes kept from fits (p1, p2, …): people fit them from the app too, name them, pin issues to them and make sections on them. list | label (id, label, describes) | delete (id).",
         "inputSchema": {"type": "object", "properties": {"action": {"type": "string", "enum": ["list", "label", "delete"]}, "id": {"type": "string"}, "label": {"type": "string"}, "describes": {"type": "string"}}}},
        {"name": "rooms", "description": "Rooms: named boxes in the model frame (room-first: levels, rooms, then walls and openings), drawn in people's views. list | set (name, min, max, note) | delete.",
         "inputSchema": {"type": "object", "properties": {"action": {"type": "string", "enum": ["list", "set", "delete"]}, "name": {"type": "string"}, "min": v3, "max": v3, "note": {"type": "string"}}}},
        {"name": "views", "description": "Named views anyone can open: list | save (name, state: a view state as for a comment's `view`) | delete.",
         "inputSchema": {"type": "object", "properties": {"action": {"type": "string", "enum": ["list", "save", "delete"]}, "name": {"type": "string"}, "state": {"type": "object"}}}},
        {"name": "sections", "description": "Named sections (render_section with name=… keeps one): list | show (render again) | delete.",
         "inputSchema": {"type": "object", "properties": {"action": {"type": "string", "enum": ["list", "show", "delete"]}, "name": {"type": "string"}}}},
        {"name": "set_placement", "description": "Move the model in the scan (to fix a bad initialisation; then refine_placement). Absolute: yaw_deg, t (scan frame). Relative: dyaw_deg turns about `pivot` (default the model's middle), dt shifts (in the model's axes by default, or frame=scan). Reports the score before and after; dry_run does not keep it.",
         "inputSchema": {"type": "object", "properties": {"yaw_deg": {"type": "number"}, "t": v3, "dyaw_deg": {"type": "number"}, "pivot": v3, "dt": v3, "frame": frame, "dry_run": {"type": "boolean"}}}},
        {"name": "refine_placement", "description": "Refine the placement by robust point-to-plane ICP of the scan onto the model's faces, from where it is. Gates (m) shrink in turn; default [0.25, 0.1, 0.05, 0.03]. Only converges from near the answer.",
         "inputSchema": {"type": "object", "properties": {"gates": {"type": "array", "items": {"type": "number"}}, "dry_run": {"type": "boolean"}}}},
        {"name": "register_placement", "description": "Global registration: four yaw candidates from the scan's wall direction, each refined by ICP. Lists the candidates; apply=true keeps the best (or candidate `pick`).",
         "inputSchema": {"type": "object", "properties": {"apply": {"type": "boolean"}, "pick": {"type": "integer"}}}},
        {"name": "align_points", "description": "Place the model from correspondences: model-frame points and the scan points they should land on (a turn about the vertical and a shift; one pair shifts only).",
         "inputSchema": {"type": "object", "properties": {"pairs": {"type": "array", "items": {"type": "object", "properties": {"model": v3, "scan": v3}, "required": ["model", "scan"]}}, "dry_run": {"type": "boolean"}}, "required": ["pairs"]}}
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_listed_is_one_the_workspace_runs() {
        let src = include_str!("workspace.rs");
        for t in tools().as_array().unwrap() {
            let name = t["name"].as_str().unwrap();
            assert!(src.contains(&format!("\"{name}\"")), "{name} is listed but not dispatched");
        }
    }
}
