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
    let view = json!({"type": "object", "description": "A partial view state for people to follow as a link: only the fields that matter. pane: \"3d\" | \"section\"; section: {view: plan|x|y|custom, at, z, thickness, origin, normal, up} (as render_section); camera: {eye, target} (model frame) or {member: path} or {issue: id}; projection: \"ortho\" | \"perspective\"; colour: height | intensity | normal | depth | evidence; clip: {z_min, z_max} (model frame, null clears); layers: {scan, model, surfaces, rooms, issues} booleans; highlight: [member paths]; proposal: {issue, n, mode: before | after | changed | both}."});
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
            "text": {"type": "string", "description": "comment, evidence, create: a line or two."},
            "view": view, "view_label": {"type": "string", "description": "What the link shows, in a few words."},
            "status": {"type": "string", "enum": ["open", "doing", "waiting", "done"]},
            "priority": {"type": "string", "enum": ["high", "normal", "low"]},
            "assignee": {"type": ["string", "null"], "description": "update: a participant's name or id; null for nobody."},
            "point": v3, "box": bx,
            "member": {"type": "string", "description": "create: a member path (or part of one)."},
            "selection": {"type": "string", "description": "create: a selection's id."},
            "frame": frame}}},
        {"name": "wait", "description": "Wait for others to do something that concerns you (a new issue, a reply in an issue that is yours or that you took part in, an @mention), up to timeout_s (default 300), and return it. The way to work with people: act, say, wait, …",
         "inputSchema": {"type": "object", "properties": {"timeout_s": {"type": "number"}}}},
        {"name": "propose", "description": "Propose a change to the model's files in an issue, as a diff: exact replacements (each `old` must occur exactly once in its file) and/or whole files. The changed model is evaluated and scored against the scan (members added and removed; supported, contradicted, unseen; evidence per group); a change that does not evaluate proposes nothing. Nothing is applied: people see the proposal in the issue as a text diff and as before / after / changed in their views.",
         "inputSchema": {"type": "object", "properties": {"issue": {"type": "integer"}, "title": {"type": "string"}, "edits": {"type": "array", "items": {"type": "object", "properties": {"file": {"type": "string", "description": "Relative to the model's folder (default: the model's file)."}, "old": {"type": "string"}, "new": {"type": "string"}}, "required": ["old", "new"]}}, "files": {"type": "array", "items": {"type": "object", "properties": {"file": {"type": "string"}, "text": {"type": "string"}}, "required": ["file", "text"]}}}, "required": ["issue", "title"]}},
        {"name": "proposal", "description": "A proposal of an issue: diff (its text), apply (onto the working branch; refused if the files have moved on), reject, or revert (an applied one). Apply only when a person has agreed.",
         "inputSchema": {"type": "object", "properties": {"issue": {"type": "integer"}, "n": {"type": "integer"}, "action": {"type": "string", "enum": ["diff", "apply", "reject", "revert"]}}, "required": ["issue", "n"]}},
        {"name": "model_read", "description": "Read the model's script (or another .ts beside it: `file`, relative to the model's folder) with line numbers; list=true lists the folder's .ts files.",
         "inputSchema": {"type": "object", "properties": {"file": {"type": "string"}, "start_line": {"type": "integer"}, "end_line": {"type": "integer"}, "list": {"type": "boolean"}}}},
        {"name": "members", "description": "Model members with their evidence: path, role, box centre and size (model frame), scan points on their faces, density, mean offset, and the share of sensor rays passing through them. filter: substring of the path, or a role. sort: path | contradicted | support.",
         "inputSchema": {"type": "object", "properties": {"filter": {"type": "string"}, "sort": {"type": "string", "enum": ["path", "contradicted", "support"]}, "limit": {"type": "integer"}, "issue": issue}}},
        {"name": "render_section", "description": "An image of the scan in a slab about a plane, with the model cut by it (coloured by evidence), on a grid labelled in metres; the text gives the exact pixel ↔ metre mapping and the members cut. view=plan (z slab, default model z 0.1–2.4: walls), x (plane x=at, image right = +y), y (plane y=at, image right = +x), or custom (origin, normal, up). color = depth | normal | intensity helps to tell members from what is behind them. To show people the same section, attach it to a comment as a `view`.",
         "inputSchema": {"type": "object", "properties": {"view": {"type": "string", "enum": ["plan", "x", "y", "custom"]}, "at": {"type": "number"}, "z": {"type": "array", "items": {"type": "number"}}, "thickness": {"type": "number"}, "origin": v3, "normal": v3, "up": v3, "bounds": {"type": "array", "items": {"type": "number"}, "description": "[u0, u1, v0, v1] in metres"}, "frame": frame, "color": {"type": "string", "enum": ["density", "depth", "normal", "intensity"], "description": "density (default); depth: offset from the cut plane of the nearest point (what stands in front of what); normal: surfaces facing the viewer grey, edge-on surfaces hued by direction; intensity: the lidar return (materials)"}, "highlight": {"type": "array", "items": {"type": "string"}}, "members": {"type": "boolean"}, "max_px": {"type": "integer"}, "save": {"type": "string", "description": "Also write the PNG here."}, "name": {"type": "string", "description": "Keep the section by this name."}, "issue": issue}}},
        {"name": "render_view", "description": "A 3D picture of the scan with the model's edges over it (coloured by evidence), to look around with: by default the whole model from the south-west, above. Look at a `member` (path or part of one; it is highlighted), an `issue`, or from `eye` to `target`; `from` is the direction toward the viewer; top_down looks straight down, orthographic, the model's +y up. clip_z / clip_z_min hide what is above / below a height (model frame), e.g. clip_z 2.3 to look in under the roof. Nobody's own view changes. Measure in sections (render_section), which are exact; a 3D picture is for finding where to look.",
         "inputSchema": {"type": "object", "properties": {"member": {"type": "string"}, "issue": {"type": "integer"}, "eye": v3, "target": v3, "from": v3, "top_down": {"type": "boolean"}, "projection": {"type": "string", "enum": ["ortho", "perspective"]}, "half_height": {"type": "number"}, "color": {"type": "string", "enum": ["intensity", "height", "normal"]}, "clip_z": {"type": "number"}, "clip_z_min": {"type": "number"}, "highlight": {"type": "array", "items": {"type": "string"}}, "members": {"type": "boolean"}, "max_px": {"type": "integer"}, "frame": frame}}},
        {"name": "measure", "description": "Statistics of the scan points in a box or a selection: count, centroid, extent, percentiles per axis, best plane (normal, offset, rms), principal spreads, and the nearest model members with their mean offsets.",
         "inputSchema": {"type": "object", "properties": {"box": bx, "selection": {"type": "string", "description": "A selection's id, in place of a box."}, "frame": frame, "issue": issue}}},
        {"name": "probe", "description": "At a point: the nearest model face (member, distance, offset along its normal) and the scan points within `radius` (count, their plane).",
         "inputSchema": {"type": "object", "properties": {"point": v3, "radius": {"type": "number"}, "frame": frame, "issue": issue}, "required": ["point"]}},
        {"name": "walls", "description": "A profile across every wall of the model (each group with studs): the wall's inside and outside faces, and where the scan has surfaces across it, with each surface's distance from both faces. Evidence to read with care, not a verdict: a peak may be stud faces, sheathing seen between studs, a finish, insulation or clutter. Use it to see which walls deserve a closer look, then fit_surface.",
         "inputSchema": {"type": "object", "properties": {"filter": {"type": "string"}, "band": {"type": "number", "description": "how far either side of the wall to look (m, default 0.6)"}, "z": {"type": "array", "items": {"type": "number"}, "description": "height range of scan points used (default [0.3, 2.0])"}, "issue": issue}}},
        {"name": "studs", "description": "Where the scan has studs along each wall of the model, as distances from the wall's start (the end its stud#1 is at), and the set-out that fits them: how far the scan reaches into the wall along its length, the stud-like runs, and the grid at the stud spacing that most of them fall on. Evidence, not a verdict: a finished wall shows no studs, clutter and openings hide them, and corner or door framing is off the grid. Check a set-out against a section before proposing it.",
         "inputSchema": {"type": "object", "properties": {"filter": {"type": "string", "description": "Only walls whose path contains this."}, "spacing": {"type": "number", "description": "Stud spacing, m (default 0.4064, 16 in)."}, "z": {"type": "array", "items": {"type": "number"}}, "issue": issue}}},
        {"name": "fit_surface", "description": "Fit a plane to a surface someone has said what it is, with the support you choose. Seed: a point on it, or a selection (its points are then the support). Support: `box` (or a cube of `radius` about the seed), `thickness` (points within this of the plane count), `normal` + `normal_tol_deg` (the expected facing; the fit is held to it if the points would turn it further), `cell_normals` (only points whose own 5 cm neighbourhood is a surface facing the same way: drops edges, clutter, crossing surfaces). Reports the plane, which side it was seen from, inliers, rms, coverage, the offset per quadrant (tilt, warp, two surfaces), the other surfaces across it, and the signed distances of the model's parallel faces nearby. `name` + `describes` keep it as a surface (drawn in people's views).",
         "inputSchema": {"type": "object", "properties": {"seed": v3, "selection": {"type": "string", "description": "A selection's id: its points are the support (and its middle the seed)."}, "box": bx, "radius": {"type": "number"}, "normal": v3, "normal_tol_deg": {"type": "number"}, "thickness": {"type": "number"}, "cell_normals": {"type": "boolean"}, "name": {"type": "string"}, "describes": {"type": "string", "description": "what the surface is, e.g. \"sheathing seen between studs: the studs' outside face\""}, "frame": frame, "issue": issue}}},
        {"name": "selections", "description": "Sets of scan points someone marked. People make them by dragging a window in a view (the surface nearest them, unless they chose all that is behind it too) and often pin an issue to one; make your own from a box. Use a selection's id in measure and fit_surface (its points are then the support) and as an issue's place. list | create (box, label) | label (id, label) | delete (id).",
         "inputSchema": {"type": "object", "properties": {"action": {"type": "string", "enum": ["list", "create", "label", "delete"]}, "id": {"type": "string"}, "label": {"type": "string"}, "box": bx, "frame": frame}}},
        {"name": "surfaces", "description": "The fitted surfaces kept: list | delete (name).",
         "inputSchema": {"type": "object", "properties": {"action": {"type": "string", "enum": ["list", "delete"]}, "name": {"type": "string"}}}},
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
