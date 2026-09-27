//! `topo serve [model.ts | dir]… [port]`: a local web UI for models.
//!
//! Projects are the model files (`.ts` with a default export) found in the
//! given folders (and the folders of given files), plus `./projects`, where
//! new projects are created. The header has a picker to switch between them.
//!
//! Shows the drawing sheets with the model's field measurements overlaid
//! (coloured by fit), lets you pick physical features on the drawings and
//! record new tape readings. New measurements are appended to the model's
//! `<model>.measured.ts` sidecar; the script remains the source of truth, and
//! any change to the model's files triggers a re-fit.

use crate::{all_issues, fit_tables};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tiny_http::{Header, Method, Request, Response, Server};
use topo_core::units::fmt_ft_in;
use topo_core::{Model, Topology};
use topo_draw::pick::{overlay, pick, OverlayInput};
use topo_draw::DrawingSet;
use topo_geom::measure::{evaluate, Quantity};
use topo_geom::Geometry;
use topo_script::measfile;
use topo_script::solve::SolveReport;
use topo_analysis::{nds_asd, takedown, Analysis, Takedown};

const UI: &str = include_str!("ui.html");

struct Built {
    model: Model,
    geom: Geometry,
    td: Takedown,
    analysis: Analysis,
    set: DrawingSet,
    fit: Option<SolveReport>,
    issues: Vec<String>,
    svgs: Vec<String>,
}

struct App {
    file: PathBuf,
    /// Folders scanned for projects.
    roots: Vec<PathBuf>,
    /// Where new projects are created.
    new_dir: PathBuf,
    stamp: Vec<(PathBuf, SystemTime, u64)>,
    version: u64,
    built: Result<Built, String>,
}

/// Modification stamps of the `.ts` files a model can import (its folder tree).
fn stamp(dir: &Path) -> Vec<(PathBuf, SystemTime, u64)> {
    let mut out = vec![];
    let mut stack = vec![(dir.to_path_buf(), 0)];
    while let Some((d, depth)) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if p.is_dir() {
                if depth < 3 && !name.starts_with('.') && name != "target" && name != "node_modules" && !name.starts_with("out") {
                    stack.push((p, depth + 1));
                }
            } else if name.ends_with(".ts") {
                if let Ok(md) = e.metadata() {
                    out.push((p, md.modified().unwrap_or(SystemTime::UNIX_EPOCH), md.len()));
                }
            }
        }
    }
    out.sort();
    out
}

/// The model file and the local files it imports (recursively, `./` and
/// `../` specifiers), plus its measurements sidecar if present: the files
/// the Script view shows and may write.
fn local_sources(file: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = vec![];
    let mut todo = vec![file.to_path_buf()];
    while let Some(f) = todo.pop() {
        let Ok(f) = f.canonicalize() else { continue };
        if out.contains(&f) {
            continue;
        }
        let text = std::fs::read_to_string(&f).unwrap_or_default();
        out.push(f.clone());
        for spec in import_specifiers(&text) {
            if spec.starts_with("./") || spec.starts_with("../") {
                let name = if spec.ends_with(".ts") { spec } else { format!("{spec}.ts") };
                todo.push(f.parent().unwrap_or(Path::new(".")).join(name));
            }
        }
    }
    let sidecar = measfile::sidecar_path(file);
    if let Ok(sc) = sidecar.canonicalize() {
        if !out.contains(&sc) {
            out.push(sc);
        }
    }
    out
}

/// Module specifiers in `… from "x"` and `import "x"` (a plain scan; a
/// specifier-like string elsewhere at worst adds a file to the list).
fn import_specifiers(text: &str) -> Vec<String> {
    let mut out = vec![];
    for kw in ["from", "import"] {
        let mut rest = text;
        while let Some(i) = rest.find(kw) {
            let before_ok = i == 0 || !rest.as_bytes()[i - 1].is_ascii_alphanumeric();
            let after = &rest[i + kw.len()..];
            if before_ok && after.starts_with(char::is_whitespace) {
                if let Some(q) = quoted(after) {
                    out.push(q);
                }
            }
            rest = after;
        }
    }
    out
}

fn quoted(s: &str) -> Option<String> {
    let s = s.trim_start();
    let q = s.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let end = s[1..].find(q)?;
    Some(s[1..1 + end].to_string())
}

fn build(file: &Path) -> Result<Built, String> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("reading {}: {e}", file.display()))?;
    let (model, fit) = topo_script::run_solved(&text, &file.to_string_lossy()).map_err(|e| e.to_string())?;
    let topo = Topology::build(&model);
    let geom = Geometry::build(&model, &topo);
    let issues = all_issues(&model, &topo, &geom).iter().map(|i| i.to_string()).collect();
    let set = DrawingSet::build_with(&model, &topo, &geom, fit.as_ref().map(fit_tables).unwrap_or_default());
    let svgs = (0..set.sheets.len()).map(|i| set.sheet_svg(&model, i)).collect();
    let td = takedown(&model, &topo, &geom);
    let analysis = nds_asd(&model, &geom, &td);
    Ok(Built { model, geom, td, analysis, set, fit, issues, svgs })
}

impl App {
    fn refresh(&mut self) {
        let dir = self.file.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
        let now = stamp(dir);
        if now != self.stamp || self.version == 0 {
            self.stamp = now;
            self.version += 1;
            self.built = build(&self.file);
            match &self.built {
                Ok(b) => eprintln!("[v{}] built {}: {} members{}", self.version, self.file.display(), b.model.members.len(), b.fit.as_ref().map(|f| format!(", fit rms {:.3}", f.rms)).unwrap_or_default()),
                Err(e) => eprintln!("[v{}] {e}", self.version),
            }
        }
    }

    fn state(&self) -> Value {
        let model_text = std::fs::read_to_string(&self.file).unwrap_or_default();
        let sidecar = measfile::sidecar_path(&self.file);
        let stem = sidecar.file_name().map(|s| s.to_string_lossy().trim_end_matches(".ts").to_string()).unwrap_or_default();
        let sidecar_info = json!({
            "path": sidecar.display().to_string(),
            "exists": sidecar.exists(),
            "imported": model_text.contains(&format!("./{stem}")),
            "hint": measfile::import_hint(&self.file),
        });
        match &self.built {
            Err(e) => json!({ "version": self.version, "file": self.file.display().to_string(), "error": e, "sidecar": sidecar_info }),
            Ok(b) => json!({
                "version": self.version,
                "file": self.file.display().to_string(),
                "name": b.model.info.name,
                "sheets": b.set.sheets.iter().map(|s| json!({
                    "number": s.number,
                    "title": s.title,
                    "views": topo_draw::pick::view_boxes(s).into_iter().map(|(t, b)| json!({ "title": t, "bbox": b })).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
                "fit": b.fit.as_ref().map(|f| json!({
                    "rms": f.rms,
                    "redundancy": f.redundancy,
                    "iterations": f.iterations,
                    "undetermined": f.undetermined(),
                    "unknowns": f.unknown_rows(),
                    "measurements": f.measurement_rows(),
                    "names": f.measurements.iter().map(|m| &m.name).collect::<Vec<_>>(),
                })),
                "issues": b.issues,
                "sheet_errors": b.set.errors,
                "analysis": analysis_summary(b),
                "sidecar": sidecar_info,
            }),
        }
    }
}

#[derive(Deserialize)]
struct PickReq {
    sheet: usize,
    x: f64,
    y: f64,
    radius: f64,
}

#[derive(Deserialize)]
struct PreviewReq {
    quantity: Quantity,
}

#[derive(Deserialize)]
struct MeasureReq {
    name: String,
    quantity: Quantity,
    value: String,
    note: Option<String>,
}

#[derive(Deserialize)]
struct OpenReq {
    path: String,
}

#[derive(Deserialize)]
struct SourceReq {
    path: String,
    text: String,
}

#[derive(Deserialize)]
struct ParseReq {
    text: String,
}

// ----- structural analysis ------------------------------------------------------

/// Checks matching a colour-by mode.
fn mode_matches(mode: &str, title: &str) -> bool {
    match mode {
        "bending" => title.starts_with("Bending") || title.starts_with("Truss member"),
        "shear" => title.starts_with("Shear"),
        "compression" => title.starts_with("Compression") || title.starts_with("Truss member: compression"),
        "bearing" => title.starts_with("Bearing"),
        "deflection" => title.starts_with("Deflection"),
        "connections" => title.starts_with("Nailed"),
        _ => true,
    }
}

/// Highest ratio per member for a mode and combination (`""` = all).
fn member_ratios(b: &Built, mode: &str, combo: &str) -> Vec<Option<(f64, usize)>> {
    let mut out: Vec<Option<(f64, usize)>> = vec![None; b.model.members.len()];
    for (i, c) in b.analysis.checks.iter().enumerate() {
        if !mode_matches(mode, &c.title) || (!combo.is_empty() && c.combination != combo) {
            continue;
        }
        let r = c.ratio();
        let slot = &mut out[c.member.idx()];
        if slot.is_none_or(|(x, _)| r > x) {
            *slot = Some((r, i));
        }
    }
    out
}

fn analysis_summary(b: &Built) -> Value {
    let a = &b.analysis;
    let ratios = member_ratios(b, "utilization", "");
    let checked = ratios.iter().flatten().count();
    let over = ratios.iter().flatten().filter(|(r, i)| *r > 1.0 && !a.checks[*i].indicative).count();
    let indicative_over = ratios.iter().flatten().filter(|(r, i)| *r > 1.0 && a.checks[*i].indicative).count();
    let mut worst: Vec<(f64, usize)> = ratios.iter().flatten().copied().collect();
    worst.sort_by(|x, y| y.0.total_cmp(&x.0));
    let mut combos: Vec<String> = a.combos.iter().map(|c| c.name.clone()).collect();
    for c in &a.checks {
        if !combos.contains(&c.combination) {
            combos.push(c.combination.clone());
        }
    }
    json!({
        "asce7": b.model.standards.asce7,
        "combos": combos,
        "checked": checked,
        "over": over,
        "indicative_over": indicative_over,
        "max": worst.first().map(|w| w.0).unwrap_or(0.0),
        "unverified": a.checks.iter().any(|c| !c.verified),
        "worst": worst.iter().take(12).map(|&(r, i)| {
            let c = &a.checks[i];
            json!({ "path": b.model.member_path(c.member), "label": short(&b.model, c.member), "ratio": r, "title": c.title, "combination": c.combination, "indicative": c.indicative })
        }).collect::<Vec<_>>(),
        "issues": a.issues.iter().chain(b.td.issues.iter()).map(|i| i.message.clone()).collect::<Vec<_>>(),
    })
}

fn short(model: &Model, m: topo_core::MemberId) -> String {
    let p = model.member_path(m);
    let parts: Vec<&str> = p.split('/').collect();
    parts[parts.len().saturating_sub(2)..].join("/")
}

/// Member silhouettes on a sheet with their ratio for a mode/combination.
fn heat(b: &Built, sheet: usize, mode: &str, combo: &str) -> Value {
    let Some(sh) = b.set.sheets.get(sheet) else { return json!([]) };
    let ratios = member_ratios(b, mode, combo);
    let shapes: Vec<Value> = topo_draw::pick::member_shapes(&b.geom, sh)
        .into_iter()
        .filter_map(|s| {
            let (r, i) = ratios[s.member.idx()]?;
            let c = &b.analysis.checks[i];
            Some(json!({ "path": b.model.member_path(s.member), "outline": s.outline, "ratio": r, "title": c.title, "combination": c.combination }))
        })
        .collect();
    json!(shapes)
}

/// Everything about one member: checks with calculation traces, supports,
/// and internal-force diagrams (US units) for a combination.
fn member_detail(b: &Built, path: &str, combo: &str) -> Result<Value, String> {
    let m = b.model.find_member(path)?;
    let mem = b.model.member(m);
    let info = &b.td.info[m.idx()];
    let mut checks: Vec<&topo_analysis::Check> = b.analysis.checks.iter().filter(|c| c.member == m).collect();
    checks.sort_by(|x, y| y.ratio().total_cmp(&x.ratio()));
    // Diagram for the requested combination, else the governing one.
    let combo_name = if combo.is_empty() { checks.first().map(|c| c.combination.clone()).unwrap_or_default() } else { combo.to_string() };
    let factors: Vec<(topo_core::LoadCaseId, f64)> = match b.analysis.combos.iter().find(|c| c.name == combo_name) {
        Some(c) => c.factors.clone(),
        None => b.model.load_cases.iter().filter(|c| c.name == combo_name).map(|c| (c.id, 1.0)).collect(),
    };
    let (ds, _) = b.td.combine(&factors);
    let d = &ds[m.idx()];
    const FT: f64 = 0.3048;
    const LB: f64 = 4.448_221_615_260_5;
    let step = (d.t.len() / 300).max(1);
    let pick = |v: &[f64], f: f64| -> Vec<f64> { v.iter().step_by(step).map(|x| x * f).collect() };
    Ok(json!({
        "path": b.model.member_path(m),
        "label": short(&b.model, m),
        "role": mem.role,
        "section": b.model.section(mem.section).name,
        "material": b.model.material(mem.material).name,
        "behaviour": info.behaviour,
        "length_ft": info.length / FT,
        "supports": info.supports.iter().filter(|s| s.node.is_some()).map(|s| json!({
            "at_ft": s.t / FT,
            "by": s.by.map(|x| short(&b.model, x)).unwrap_or_else(|| "foundation".into()),
            "transfer": s.transfer,
        })).collect::<Vec<_>>(),
        "combination": combo_name,
        "diagram": {
            "t_ft": pick(&d.t, 1.0 / FT),
            "m_lbft": pick(&d.m, 1.0 / (LB * FT)),
            "v_lb": pick(&d.v, 1.0 / LB),
            "n_lb": pick(&d.n, 1.0 / LB),
            "y_in": pick(&d.y, 1.0 / 0.0254),
        },
        "checks": checks.iter().map(|c| json!({
            "title": c.title,
            "clause": c.clause,
            "combination": c.combination,
            "demand": c.demand,
            "capacity": c.capacity,
            "unit": c.unit,
            "ratio": c.ratio(),
            "other": c.other.map(|o| short(&b.model, o)),
            "at_ft": c.at.map(|t| t / FT),
            "notes": c.notes,
            "verified": c.verified,
            "indicative": c.indicative,
            "trace": c.trace.steps,
        })).collect::<Vec<_>>(),
    }))
}

/// Member solids for the 3D view: each member's boundary faces (outer loops,
/// world metres rounded to 0.1 mm) with its path and label.
fn mesh(b: &Built) -> Value {
    let r = |v: f64| (v * 10_000.0).round() / 10_000.0;
    let members: Vec<Value> = b
        .model
        .members
        .iter()
        .map(|m| {
            let g = b.geom.member(m.id);
            let faces: Vec<Vec<[f64; 3]>> = g.faces().iter().map(|f| f.outer.iter().map(|p| [r(p.x), r(p.y), r(p.z)]).collect()).collect();
            json!({ "path": b.model.member_path(m.id), "label": short(&b.model, m.id), "role": m.role, "faces": faces })
        })
        .collect();
    json!({ "version_name": b.model.info.name, "members": members })
}

/// Highest ratio per member (by path) for a colour-by mode and combination.
fn ratios(b: &Built, mode: &str, combo: &str) -> Value {
    let rs = member_ratios(b, mode, combo);
    let mut out = serde_json::Map::new();
    for (i, r) in rs.iter().enumerate() {
        if let Some((ratio, k)) = r {
            let c = &b.analysis.checks[*k];
            out.insert(b.model.member_path(topo_core::MemberId(i as u32)), json!({ "ratio": ratio, "title": c.title, "combination": c.combination }));
        }
    }
    Value::Object(out)
}

/// `?a=1&b=x%20y` → value of `key`.
fn query(url: &str, key: &str) -> String {
    let Some((_, q)) = url.split_once('?') else { return String::new() };
    for kv in q.split('&') {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        if k == key {
            return percent_decode(v);
        }
    }
    String::new()
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        let hex = (b[i] == b'%').then(|| b.get(i + 1..i + 3)).flatten().and_then(|h| u8::from_str_radix(std::str::from_utf8(h).ok()?, 16).ok());
        match (b[i], hex) {
            (b'%', Some(v)) => {
                out.push(v);
                i += 3;
            }
            (b'+', _) => {
                out.push(b' ');
                i += 1;
            }
            (c, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn body(req: &mut Request) -> String {
    let mut s = String::new();
    let _ = req.as_reader().read_to_string(&mut s);
    s
}

fn respond(req: Request, status: u16, content_type: &str, body: String) {
    let header = Header::from_bytes(&b"Content-Type"[..], content_type.as_bytes()).unwrap();
    let _ = req.respond(Response::from_string(body).with_status_code(status).with_header(header));
}

fn json_reply(req: Request, v: Value) {
    respond(req, 200, "application/json", v.to_string());
}

fn handle(app: &mut App, mut req: Request) {
    let url = req.url().to_string();
    let method = req.method().clone();
    app.refresh();
    let err = |req: Request, e: String| json_reply(req, json!({ "error": e }));
    match (method, url.as_str()) {
        (Method::Get, "/") => respond(req, 200, "text/html; charset=utf-8", UI.to_string()),
        (Method::Get, "/api/state") => {
            let s = app.state();
            json_reply(req, s)
        }
        (Method::Get, u) if u.starts_with("/api/sheet/") => {
            let i: usize = u.trim_start_matches("/api/sheet/").parse().unwrap_or(usize::MAX);
            match &app.built {
                Ok(b) if i < b.svgs.len() => respond(req, 200, "image/svg+xml", b.svgs[i].clone()),
                _ => respond(req, 404, "text/plain", "no such sheet".into()),
            }
        }
        (Method::Get, u) if u.starts_with("/api/overlay/") => {
            let i: usize = u.trim_start_matches("/api/overlay/").parse().unwrap_or(usize::MAX);
            let Ok(b) = &app.built else { return json_reply(req, json!([])) };
            let (Some(sheet), Some(fit)) = (b.set.sheets.get(i), &b.fit) else { return json_reply(req, json!([])) };
            let items: Vec<OverlayInput> = fit
                .measurements
                .iter()
                .map(|m| OverlayInput {
                    name: &m.name,
                    quantity: &m.quantity,
                    text: fmt_ft_in(m.measured),
                    ok: m.misfit().map(|f| f.abs() <= 1.0),
                })
                .collect();
            json_reply(req, serde_json::to_value(overlay(&b.model, &b.geom, sheet, &items)).unwrap())
        }
        (Method::Get, "/api/projects") => {
            let list: Vec<Value> = app
                .projects()
                .into_iter()
                .map(|(group, p)| json!({ "group": group, "name": p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(), "path": p.display().to_string() }))
                .collect();
            json_reply(req, json!({ "current": app.file.display().to_string(), "projects": list }))
        }
        (Method::Post, "/api/open") => {
            let text = body(&mut req);
            let Ok(p) = serde_json::from_str::<OpenReq>(&text) else { return err(req, "bad request".into()) };
            let target = PathBuf::from(&p.path);
            if !app.projects().iter().any(|(_, q)| *q == target) {
                return err(req, format!("{} is not one of the listed projects", p.path));
            }
            app.open(target);
            json_reply(req, json!({ "version": app.version }))
        }
        (Method::Post, "/api/new") => {
            let text = body(&mut req);
            let Ok(p) = serde_json::from_str::<OpenReq>(&text) else { return err(req, "bad request".into()) };
            let name = slug(&p.path);
            if name.is_empty() {
                return err(req, "give the project a name".into());
            }
            let file = app.new_dir.join(format!("{name}.ts"));
            if file.exists() {
                return err(req, format!("{} already exists", file.display()));
            }
            if let Err(e) = std::fs::create_dir_all(&app.new_dir).and_then(|_| std::fs::write(&file, NEW_PROJECT.replace("NEW PROJECT", p.path.trim()))) {
                return err(req, e.to_string());
            }
            match file.canonicalize() {
                Ok(f) => {
                    app.open(f);
                    json_reply(req, json!({ "version": app.version, "file": app.file.display().to_string() }))
                }
                Err(e) => err(req, e.to_string()),
            }
        }
        (Method::Get, "/api/sources") => {
            let base = app.file.canonicalize().ok().and_then(|f| f.parent().map(Path::to_path_buf)).unwrap_or_default();
            let files: Vec<Value> = local_sources(&app.file)
                .iter()
                .map(|p| {
                    let name = p.strip_prefix(&base).map(|r| r.display().to_string()).unwrap_or_else(|_| p.display().to_string());
                    json!({ "path": p.display().to_string(), "name": name, "text": std::fs::read_to_string(p).unwrap_or_default() })
                })
                .collect();
            json_reply(req, json!({ "files": files }))
        }
        (Method::Post, "/api/source") => {
            let text = body(&mut req);
            let Ok(p) = serde_json::from_str::<SourceReq>(&text) else { return err(req, "bad request".into()) };
            // Only the model's own files may be written.
            let target = PathBuf::from(&p.path);
            if !local_sources(&app.file).contains(&target) {
                return err(req, format!("{} is not one of this model's files", p.path));
            }
            match std::fs::write(&target, p.text) {
                Ok(()) => {
                    app.refresh();
                    let error = app.built.as_ref().err().cloned();
                    json_reply(req, json!({ "saved": p.path, "version": app.version, "model_error": error }))
                }
                Err(e) => err(req, e.to_string()),
            }
        }
        (Method::Get, u) if u.starts_with("/api/heat/") => {
            let i: usize = u.trim_start_matches("/api/heat/").split('?').next().unwrap_or("").parse().unwrap_or(usize::MAX);
            let Ok(b) = &app.built else { return json_reply(req, json!([])) };
            json_reply(req, heat(b, i, &query(u, "mode"), &query(u, "combo")))
        }
        (Method::Get, "/api/mesh") => {
            let Ok(b) = &app.built else { return err(req, "model does not build".into()) };
            json_reply(req, mesh(b))
        }
        (Method::Get, u) if u.starts_with("/api/ratios") => {
            let Ok(b) = &app.built else { return json_reply(req, json!({})) };
            json_reply(req, ratios(b, &query(u, "mode"), &query(u, "combo")))
        }
        (Method::Get, u) if u.starts_with("/api/member") => {
            let Ok(b) = &app.built else { return err(req, "model does not build".into()) };
            match member_detail(b, &query(u, "path"), &query(u, "combo")) {
                Ok(v) => json_reply(req, v),
                Err(e) => err(req, e),
            }
        }
        (Method::Post, "/api/pick") => {
            let text = body(&mut req);
            let Ok(p) = serde_json::from_str::<PickReq>(&text) else { return err(req, "bad pick request".into()) };
            let Ok(b) = &app.built else { return json_reply(req, Value::Null) };
            let Some(sheet) = b.set.sheets.get(p.sheet) else { return json_reply(req, Value::Null) };
            let hit = pick(&b.model, &b.geom, sheet, [p.x, p.y], p.radius);
            json_reply(req, serde_json::to_value(hit).unwrap())
        }
        (Method::Post, "/api/preview") => {
            let text = body(&mut req);
            let Ok(p) = serde_json::from_str::<PreviewReq>(&text) else { return err(req, "bad preview request".into()) };
            let Ok(b) = &app.built else { return err(req, "model does not build".into()) };
            match evaluate(&b.model, &b.geom, &p.quantity) {
                Ok(v) => json_reply(req, json!({ "value": v, "text": fmt_ft_in(v), "code": measfile::quantity_ts(&b.model, &p.quantity) })),
                Err(e) => err(req, e),
            }
        }
        (Method::Post, "/api/parse") => {
            let text = body(&mut req);
            let Ok(p) = serde_json::from_str::<ParseReq>(&text) else { return err(req, "bad request".into()) };
            match measfile::parse_length(&p.text) {
                Ok(v) => json_reply(req, json!({ "value": v, "text": fmt_ft_in(v) })),
                Err(e) => err(req, e),
            }
        }
        (Method::Post, "/api/measure") => {
            let text = body(&mut req);
            let Ok(p) = serde_json::from_str::<MeasureReq>(&text) else { return err(req, "bad measurement".into()) };
            let Ok(b) = &app.built else { return err(req, "model does not build".into()) };
            let name = p.name.trim();
            if name.is_empty() {
                return err(req, "give the measurement a name".into());
            }
            if b.fit.as_ref().is_some_and(|f| f.measurements.iter().any(|m| m.name == name)) {
                return err(req, format!("a measurement named \"{name}\" already exists"));
            }
            let value = match measfile::parse_length(&p.value) {
                Ok(v) => v,
                Err(e) => return err(req, e),
            };
            if let Err(e) = evaluate(&b.model, &b.geom, &p.quantity) {
                return err(req, e);
            }
            match measfile::append(&app.file, &b.model, name, &p.quantity, value, p.note.as_deref()) {
                Ok(line) => json_reply(req, json!({ "line": line, "file": measfile::sidecar_path(&app.file).display().to_string() })),
                Err(e) => err(req, e.to_string()),
            }
        }
        _ => respond(req, 404, "text/plain", "not found".into()),
    }
}

/// Model files under `roots` (to depth 3): `.ts` files with a default
/// export, excluding measurement sidecars. Grouped by folder, sorted.
fn find_projects(roots: &[PathBuf]) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = vec![];
    for root in roots {
        let Ok(root) = root.canonicalize() else { continue };
        let base = root.parent().unwrap_or(&root).to_path_buf();
        let mut stack = vec![(root.clone(), 0)];
        while let Some((d, depth)) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                let name = e.file_name().to_string_lossy().into_owned();
                if p.is_dir() {
                    if depth < 3 && !name.starts_with('.') && name != "node_modules" && name != "target" && !name.starts_with("out") {
                        stack.push((p, depth + 1));
                    }
                } else if name.ends_with(".ts") && !name.ends_with(".measured.ts") && !name.ends_with(".d.ts") {
                    let text = std::fs::read_to_string(&p).unwrap_or_default();
                    if text.contains("export default") && !out.iter().any(|(_, q)| *q == p) {
                        let group = p.parent().and_then(|g| g.strip_prefix(&base).ok()).map(|g| g.display().to_string()).unwrap_or_default();
                        out.push((group, p));
                    }
                }
            }
        }
    }
    out.sort();
    out
}

const NEW_PROJECT: &str = include_str!("new-project.ts");

/// `My Garage!` → `my-garage`.
fn slug(name: &str) -> String {
    let s: String = name.trim().to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-")
}

pub fn serve(paths: &[PathBuf], port: u16) -> Result<(), String> {
    let paths: Vec<PathBuf> = if paths.is_empty() { vec![PathBuf::from(".")] } else { paths.to_vec() };
    for p in &paths {
        if !p.exists() {
            return Err(format!("{} not found", p.display()));
        }
    }
    let new_dir = PathBuf::from("projects");
    let mut roots: Vec<PathBuf> = paths.iter().map(|p| if p.is_dir() { p.clone() } else { p.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new(".")).to_path_buf() }).collect();
    if new_dir.is_dir() {
        roots.push(new_dir.clone());
    }
    let first = match paths.iter().find(|p| p.is_file()) {
        Some(f) => f.canonicalize().map_err(|e| e.to_string())?,
        None => find_projects(&roots).first().map(|(_, p)| p.clone()).ok_or("no model files (.ts with a default export) found")?,
    };
    let server = Server::http(("127.0.0.1", port)).map_err(|e| format!("cannot listen on port {port}: {e}"))?;
    let mut app = App { file: first, roots, new_dir, stamp: vec![], version: 0, built: Err("not built".into()) };
    app.refresh();
    eprintln!("topo serve: http://127.0.0.1:{port}/  ({})", app.file.display());
    for req in server.incoming_requests() {
        handle(&mut app, req);
    }
    Ok(())
}

impl App {
    fn projects(&self) -> Vec<(String, PathBuf)> {
        let mut roots = self.roots.clone();
        if self.new_dir.is_dir() && !roots.iter().any(|r| r.canonicalize().ok() == self.new_dir.canonicalize().ok()) {
            roots.push(self.new_dir.clone());
        }
        find_projects(&roots)
    }

    fn open(&mut self, path: PathBuf) {
        self.file = path;
        self.stamp.clear();
        self.refresh();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_project_template_builds_clean() {
        let src = NEW_PROJECT.replace("NEW PROJECT", "Test shed");
        let m = topo_script::run(&src, "test-shed.ts").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(m.info.name, "Test shed");
        let topo = Topology::build(&m);
        let g = Geometry::build(&m, &topo);
        let issues = all_issues(&m, &topo, &g);
        assert!(issues.is_empty(), "{issues:#?}");
        assert_eq!(slug(" My Garage! (v2) "), "my-garage-v2");
    }

    #[test]
    fn decodes_queries() {
        assert_eq!(query("/api/member?path=Walls%2FWall%20A%2Fheader.GD&combo=D+%2B+S", "path"), "Walls/Wall A/header.GD");
        assert_eq!(query("/api/member?path=x&combo=D+%2B+S", "combo"), "D + S");
        assert_eq!(query("/api/member?path=100%", "path"), "100%");
        assert_eq!(query("/api/member", "path"), "");
    }

    #[test]
    fn finds_import_specifiers() {
        let src = "import {\n  Building, ft,\n} from \"topo-cad\";\nimport { m } from './x.measured';\nimport \"./side\";\nconst transform = 1; // not an import\nexport { y } from \"../lib/y.ts\";";
        assert_eq!(import_specifiers(src), vec!["topo-cad", "./x.measured", "../lib/y.ts", "./side"]);
    }
}
