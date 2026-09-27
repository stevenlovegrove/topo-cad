//! `topo serve <model.ts> [port]`: a local web UI for a model.
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

const UI: &str = include_str!("ui.html");

struct Built {
    model: Model,
    geom: Geometry,
    set: DrawingSet,
    fit: Option<SolveReport>,
    issues: Vec<String>,
    svgs: Vec<String>,
}

struct App {
    file: PathBuf,
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
    Ok(Built { model, geom, set, fit, issues, svgs })
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
            Err(e) => json!({ "version": self.version, "error": e, "sidecar": sidecar_info }),
            Ok(b) => json!({
                "version": self.version,
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
struct SourceReq {
    path: String,
    text: String,
}

#[derive(Deserialize)]
struct ParseReq {
    text: String,
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

pub fn serve(file: &Path, port: u16) -> Result<(), String> {
    if !file.exists() {
        return Err(format!("{} not found", file.display()));
    }
    let server = Server::http(("127.0.0.1", port)).map_err(|e| format!("cannot listen on port {port}: {e}"))?;
    let mut app = App { file: file.to_path_buf(), stamp: vec![], version: 0, built: Err("not built".into()) };
    app.refresh();
    eprintln!("topo serve: http://127.0.0.1:{port}/  ({})", file.display());
    for req in server.incoming_requests() {
        handle(&mut app, req);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_import_specifiers() {
        let src = "import {\n  Building, ft,\n} from \"topo-cad\";\nimport { m } from './x.measured';\nimport \"./side\";\nconst transform = 1; // not an import\nexport { y } from \"../lib/y.ts\";";
        assert_eq!(import_specifiers(src), vec!["topo-cad", "./x.measured", "../lib/y.ts", "./side"]);
    }
}
