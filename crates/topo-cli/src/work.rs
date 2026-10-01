//! The scan workspace, served beside the app when `topo serve` is given a
//! scan: one workspace (`topo_scan::workspace`) that every browser and
//! every agent works in.
//!
//! - `GET /api/workspace`: that the service is here (the app asks on load).
//! - `GET /api/events?who=ID`: server-sent events; the workspace's state,
//!   again whenever it changes.
//! - `POST /api/call` `{who, tool, args}`: runs a tool as that person.
//! - `GET /api/members[?proposal=COMMIT]`: the members with their evidence,
//!   or a proposal's before / after / changes / diff.
//! - `GET /api/cloud`: the thinned scan; `GET /api/slab?o=&n=&half=&max=`:
//!   every point of a slab (both as `topo_scan::cloud` bytes, scan frame).
//! - `POST /api/pick` `{origin, dir, z?, slab?}`: what a ray meets.
//! - `POST /mcp[?as=NAME]`: the same tools for an agent (MCP).

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response};
use topo_scan::cloud::ScanIndex;
use topo_scan::scan::Scan;
use topo_scan::workspace::Workspace;

#[derive(Clone)]
pub struct Work {
    /// The model, as the id the app opens it by (among the served files).
    entry: String,
    ws: Arc<Mutex<Workspace>>,
    index: Arc<Mutex<Option<Arc<ScanIndex>>>>,
    /// What is being loaded, for the app to show ("" when ready).
    loading: Arc<Mutex<String>>,
}

fn header(k: &str, v: &str) -> Header {
    Header::from_bytes(k.as_bytes(), v.as_bytes()).unwrap()
}

fn respond_json(req: Request, status: u16, v: &Value) {
    let _ = req.respond(Response::from_string(v.to_string()).with_status_code(status).with_header(header("Content-Type", "application/json")).with_header(header("Cache-Control", "no-cache")));
}

fn respond_bytes(req: Request, bytes: Vec<u8>) {
    let _ = req.respond(Response::from_data(bytes).with_header(header("Content-Type", "application/octet-stream")).with_header(header("Cache-Control", "no-cache")));
}

fn v3(s: Option<&String>) -> Option<[f32; 3]> {
    let v: Vec<f32> = s?.split(',').filter_map(|x| x.trim().parse().ok()).collect();
    (v.len() == 3).then(|| [v[0], v[1], v[2]])
}

fn arr3(v: &Value) -> Option<[f32; 3]> {
    let a = v.as_array()?;
    (a.len() == 3).then(|| [a[0].as_f64().unwrap_or(0.0) as f32, a[1].as_f64().unwrap_or(0.0) as f32, a[2].as_f64().unwrap_or(0.0) as f32])
}

impl Work {
    /// Opens the project `model` is in, and loads the scan in the background.
    pub fn start(model: &Path, scan: Option<&str>, entry: String) -> Result<Work, String> {
        let ws = Workspace::open(model, scan).map_err(|e| format!("{e:#}"))?;
        let path = ws.items.scan.clone().ok_or("no scan: give one (a .ply) after the model")?;
        let work = Work { entry, ws: Arc::new(Mutex::new(ws)), index: Arc::new(Mutex::new(None)), loading: Arc::new(Mutex::new("loading the scan".into())) };
        let w = work.clone();
        std::thread::spawn(move || {
            let t = Instant::now();
            let say = |s: &str| {
                eprintln!("topo serve: {} ({:.1} s)", if s.is_empty() { "the scan is ready" } else { s }, t.elapsed().as_secs_f64());
                *w.loading.lock().unwrap() = s.to_string();
                w.ws.lock().unwrap().touch();
            };
            // Outside the lock: the app is usable meanwhile.
            let scan = match Scan::load(&path) {
                Ok(s) => Arc::new(s),
                Err(e) => return say(&format!("the scan did not load: {e:#}")),
            };
            say("placing the model in the scan");
            {
                let mut ws = w.ws.lock().unwrap();
                ws.scan = Some(scan.clone());
                if let Err(e) = ws.evidence() {
                    eprintln!("topo serve: {e:#}");
                }
            }
            say("thinning the scan for the 3D view");
            let ix = ScanIndex::build(&scan, 0.025);
            eprintln!("topo serve: {} of {} points in the thinned cloud", ix.thin.len(), scan.xyz.len());
            let ix = Arc::new(ix);
            w.ws.lock().unwrap().index = Some(ix.clone());
            *w.index.lock().unwrap() = Some(ix);
            say("");
        });
        Ok(work)
    }

    pub fn handles(url: &str) -> bool {
        url.starts_with("/api/") || url == "/mcp"
    }

    /// A file of the project was saved from the app.
    pub fn saved(&self, who: &str, path: &Path) {
        let mut ws = self.ws.lock().unwrap();
        let _ = ws.join(who, "person");
        if let Err(e) = ws.saved(who, path) {
            eprintln!("topo serve: committing {}: {e:#}", path.display());
        }
    }

    fn state(&self) -> Value {
        let mut s = self.ws.lock().unwrap().state_json();
        s["loading"] = json!(*self.loading.lock().unwrap());
        s["cloud"] = json!(self.index.lock().unwrap().as_ref().map(|i| i.thin.len()));
        s
    }

    pub fn handle(&self, mut req: Request, url: &str, q: &HashMap<String, String>) {
        let method = req.method().clone();
        let mut body = String::new();
        if method == Method::Post && req.as_reader().read_to_string(&mut body).is_err() {
            return respond_json(req, 400, &json!({"error": "the body must be UTF-8"}));
        }
        let fail = |req: Request, e: String| respond_json(req, 200, &json!({"error": e}));
        match (method, url) {
            (Method::Get, "/api/workspace") => respond_json(req, 200, &json!({"service": "topo-cad scan workspace", "entry": self.entry})),
            (Method::Get, "/api/events") => self.events(req, q.get("who").cloned().unwrap_or_default()),
            (Method::Post, "/api/call") => {
                let Ok(v) = serde_json::from_str::<Value>(&body) else { return fail(req, "the body must be JSON".into()) };
                let (who, tool) = (v["who"].as_str().unwrap_or(""), v["tool"].as_str().unwrap_or(""));
                if who.is_empty() {
                    return fail(req, "who: your participant id".into());
                }
                match topo_scan::mcp::call(&self.ws, who, "person", tool, v.get("args").unwrap_or(&json!({}))) {
                    Ok(content) => respond_json(req, 200, &json!({"content": content})),
                    Err(e) => fail(req, format!("{e:#}")),
                }
            }
            (Method::Get, "/api/members") => {
                let r = self.ws.lock().unwrap().members_json(q.get("proposal").map(String::as_str));
                match r {
                    Ok(v) => respond_json(req, 200, &v),
                    Err(e) => fail(req, format!("{e:#}")),
                }
            }
            (Method::Get, "/api/cloud") => match self.index.lock().unwrap().clone() {
                Some(ix) => respond_bytes(req, ix.thin.bytes()),
                None => respond_json(req, 503, &json!({"error": "the scan is still loading"})),
            },
            (Method::Get, "/api/slab") => {
                let (ix, scan) = (self.index.lock().unwrap().clone(), self.ws.lock().unwrap().scan.clone());
                let (Some(ix), Some(scan)) = (ix, scan) else { return respond_json(req, 503, &json!({"error": "the scan is still loading"})) };
                let (Some(o), Some(n)) = (v3(q.get("o")), v3(q.get("n"))) else { return respond_json(req, 400, &json!({"error": "o=x,y,z and n=x,y,z"})) };
                let half = q.get("half").and_then(|x| x.parse().ok()).unwrap_or(0.15);
                let max = q.get("max").and_then(|x| x.parse().ok()).unwrap_or(2_000_000usize).min(6_000_000);
                let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-9);
                respond_bytes(req, ix.slab(&scan, o, n.map(|x| x / len), half, max).bytes())
            }
            (Method::Post, "/api/pick") => {
                let Ok(v) = serde_json::from_str::<Value>(&body) else { return fail(req, "the body must be JSON".into()) };
                let (Some(o), Some(d)) = (arr3(&v["origin"]), arr3(&v["dir"])) else { return fail(req, "origin and dir: [x, y, z] (scan frame)".into()) };
                let z = v["z"].as_array().filter(|a| a.len() == 2).map(|a| [a[0].as_f64().unwrap_or(-1e9) as f32, a[1].as_f64().unwrap_or(1e9) as f32]);
                let slab = match (arr3(&v["slab"]["origin"]), arr3(&v["slab"]["normal"]), v["slab"]["half"].as_f64()) {
                    (Some(o), Some(n), Some(h)) => Some((o, n, h as f32)),
                    _ => None,
                };
                let r = self.ws.lock().unwrap().pick_json(o, d, z, slab);
                match r {
                    Ok(v) => respond_json(req, 200, &v),
                    Err(e) => fail(req, format!("{e:#}")),
                }
            }
            (Method::Post, "/mcp") => {
                let who = q.get("as").cloned().unwrap_or_else(|| "agent".into());
                let reply = match serde_json::from_str::<Value>(&body) {
                    Ok(Value::Array(batch)) => {
                        let rs: Vec<Value> = batch.iter().filter_map(|m| topo_scan::mcp::handle(&self.ws, &who, m)).collect();
                        (!rs.is_empty()).then(|| Value::Array(rs))
                    }
                    Ok(msg) => topo_scan::mcp::handle(&self.ws, &who, &msg),
                    Err(e) => Some(json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": e.to_string()}})),
                };
                match reply {
                    Some(r) => respond_json(req, 200, &r),
                    None => {
                        let _ = req.respond(Response::empty(202));
                    }
                }
            }
            (Method::Delete, "/mcp") => {
                let _ = req.respond(Response::empty(200));
            }
            _ => respond_json(req, 404, &json!({"error": format!("no such request: {url}")})),
        }
    }

    /// The state, and again whenever it changes, until the browser goes.
    fn events(&self, req: Request, who: String) {
        let mut out = req.into_writer();
        if out.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\nretry: 1000\n\n").is_err() {
            return;
        }
        let (mut sent, mut seen_rev, mut last, mut tick) = (String::new(), 0u64, Instant::now(), 0u32);
        loop {
            let rev = {
                let mut ws = self.ws.lock().unwrap();
                if !who.is_empty() {
                    let _ = ws.join(&who, "person");
                }
                if tick % 5 == 0 {
                    ws.sync();
                }
                ws.rev
            };
            // Who is here changes with time, not only with what is done: look again every 2 s.
            let state = if rev != seen_rev || tick % 10 == 0 {
                seen_rev = rev;
                let mut s = self.state();
                s["rev"] = json!(0);
                s.to_string()
            } else {
                sent.clone()
            };
            let wrote = if state != sent {
                sent = state;
                last = Instant::now();
                write!(out, "event: state\ndata: {sent}\n\n")
            } else if last.elapsed() > Duration::from_secs(15) {
                last = Instant::now();
                write!(out, ": here\n\n")
            } else {
                Ok(())
            };
            if wrote.is_err() || out.flush().is_err() {
                return;
            }
            tick += 1;
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}
