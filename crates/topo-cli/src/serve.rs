//! `topo serve [dir | model.ts]… [port]`: the web app, for local use.
//!
//! The app is static files (`web/`: the page, its scripts, and the engine
//! compiled to wasm), so any web server can host it; this one adds local
//! files as a project store the app can write to:
//!
//! - `/…`: the app (from `web/`, or `--web <dir>`).
//! - `/files/manifest.json`: the `.ts` files under the given folders (and
//!   `./projects`), as ids relative to their common parent.
//! - `GET`/`HEAD /files/<id>`: a file (with an `ETag` that changes when it
//!   does, so the app notices edits made outside it, e.g. by an editor or an
//!   LLM helper); `PUT /files/<id>` saves one (`.ts` files under those
//!   folders only).
//!
//! A static host serves the same app read-only: `topo site` writes the app
//! and a project folder's files, with the manifest, for any web server.

use serde_json::json;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;
use tiny_http::{Header, Method, Request, Response, Server};

/// The app's static files, from the source tree unless given.
pub fn default_web_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../web"))
}

struct Files {
    /// Ids are paths relative to this.
    base: PathBuf,
    /// Folders (under `base`) whose `.ts` files are listed and writable.
    roots: Vec<PathBuf>,
}

fn skip_dir(name: &str) -> bool {
    name.starts_with('.') || matches!(name, "node_modules" | "target" | "pkg") || name.starts_with("out")
}

impl Files {
    /// The given folders (or model files' folders); with `projects`, also
    /// `./projects`, where the app creates new projects.
    fn new(paths: &[PathBuf], projects: bool) -> Result<Files, String> {
        let mut roots = vec![];
        for p in paths {
            let p = p.canonicalize().map_err(|e| format!("{}: {e}", p.display()))?;
            let dir = if p.is_dir() { p } else { p.parent().unwrap_or(Path::new("/")).to_path_buf() };
            if !roots.contains(&dir) {
                roots.push(dir);
            }
        }
        if projects {
            if let Ok(pr) = Path::new("projects").canonicalize() {
                if !roots.contains(&pr) {
                    roots.push(pr);
                }
            } else if let Ok(cwd) = std::env::current_dir() {
                // New projects go here (created on first save).
                roots.push(cwd.join("projects"));
            }
        }
        if roots.is_empty() {
            return Err("no folders given".into());
        }
        let base = common_ancestor(&roots);
        Ok(Files { base, roots })
    }

    fn id(&self, p: &Path) -> Option<String> {
        p.strip_prefix(&self.base).ok().map(|r| r.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/"))
    }

    /// The path for an id, if it names a `.ts` file under a root.
    fn path(&self, id: &str) -> Option<PathBuf> {
        let rel = Path::new(id);
        if !id.ends_with(".ts") || rel.components().any(|c| !matches!(c, Component::Normal(_))) {
            return None;
        }
        let p = self.base.join(rel);
        self.roots.iter().any(|r| p.starts_with(r)).then_some(p)
    }

    fn manifest(&self) -> serde_json::Value {
        let mut files = vec![];
        for root in &self.roots {
            let mut stack = vec![(root.clone(), 0)];
            while let Some((d, depth)) = stack.pop() {
                let Ok(rd) = std::fs::read_dir(&d) else { continue };
                for e in rd.flatten() {
                    let p = e.path();
                    let name = e.file_name().to_string_lossy().into_owned();
                    if p.is_dir() {
                        if depth < 4 && !skip_dir(&name) {
                            stack.push((p, depth + 1));
                        }
                    } else if name.ends_with(".ts") && !name.ends_with(".d.ts") {
                        if let Some(id) = self.id(&p) {
                            if !files.iter().any(|f: &serde_json::Value| f["id"] == id) {
                                files.push(json!({ "id": id, "mtime": etag(&p) }));
                            }
                        }
                    }
                }
            }
        }
        files.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
        let label = format!("Files in {}", self.base.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| self.base.display().to_string()));
        json!({ "label": label, "writable": true, "files": files })
    }
}

fn common_ancestor(paths: &[PathBuf]) -> PathBuf {
    let mut base = paths.first().cloned().unwrap_or_default();
    for p in &paths[1..] {
        while !p.starts_with(&base) {
            if !base.pop() {
                break;
            }
        }
    }
    base
}

/// Changes whenever the file does (modification time in ns and size).
fn etag(p: &Path) -> String {
    let md = std::fs::metadata(p).ok();
    let t = md.as_ref().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_nanos()).unwrap_or(0);
    format!("\"{t}-{}\"", md.map(|m| m.len()).unwrap_or(0))
}

fn content_type(p: &Path) -> &'static str {
    match p.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "wasm" => "application/wasm",
        "json" => "application/json",
        "css" => "text/css",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ts" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn header(k: &str, v: &str) -> Header {
    Header::from_bytes(k.as_bytes(), v.as_bytes()).unwrap()
}

fn text(req: Request, status: u16, body: &str) {
    let _ = req.respond(Response::from_string(body).with_status_code(status).with_header(header("Content-Type", "text/plain; charset=utf-8")));
}

/// `a%20b` → `a b`.
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
            (c, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn handle(files: &Files, web: &Path, mut req: Request) {
    let url = req.url().split('?').next().unwrap_or("/").to_string();
    let method = req.method().clone();
    if let Some(rest) = url.strip_prefix("/files/") {
        let id = percent_decode(rest);
        if id == "manifest.json" {
            let body = files.manifest().to_string();
            let _ = req.respond(Response::from_string(body).with_header(header("Content-Type", "application/json")).with_header(header("Cache-Control", "no-cache")));
            return;
        }
        let Some(path) = files.path(&id) else { return text(req, 403, &format!("{id}: not a .ts file in the served folders")) };
        match method {
            Method::Get | Method::Head => match std::fs::read(&path) {
                Ok(bytes) => {
                    let r = Response::from_data(if method == Method::Head { vec![] } else { bytes })
                        .with_header(header("Content-Type", content_type(&path)))
                        .with_header(header("ETag", &etag(&path)))
                        .with_header(header("Cache-Control", "no-cache"));
                    let _ = req.respond(r);
                }
                Err(_) => text(req, 404, &format!("{id} not found")),
            },
            Method::Put => {
                let mut body = String::new();
                if req.as_reader().read_to_string(&mut body).is_err() {
                    return text(req, 400, "the file must be UTF-8 text");
                }
                let r = path.parent().map(std::fs::create_dir_all).unwrap_or(Ok(())).and_then(|_| std::fs::write(&path, body));
                match r {
                    Ok(()) => {
                        eprintln!("saved {id}");
                        text(req, 200, "saved")
                    }
                    Err(e) => text(req, 500, &e.to_string()),
                }
            }
            _ => text(req, 405, "GET, HEAD or PUT"),
        }
        return;
    }
    // The app's static files.
    let rel = if url == "/" { "index.html".to_string() } else { percent_decode(url.trim_start_matches('/')) };
    if Path::new(&rel).components().any(|c| !matches!(c, Component::Normal(_))) {
        return text(req, 403, "bad path");
    }
    let p = web.join(&rel);
    match std::fs::read(&p) {
        Ok(bytes) => {
            let _ = req.respond(Response::from_data(bytes).with_header(header("Content-Type", content_type(&p))).with_header(header("Cache-Control", "no-cache")));
        }
        Err(_) => text(req, 404, &format!("{rel} not found")),
    }
}

pub fn serve(paths: &[PathBuf], port: u16, web: &Path) -> Result<(), String> {
    let paths: Vec<PathBuf> = if paths.is_empty() { vec![PathBuf::from(".")] } else { paths.to_vec() };
    if !web.join("pkg").join("topo_wasm_bg.wasm").exists() {
        return Err(format!("the app's engine is not built: run web/build.sh (looked in {})", web.join("pkg").display()));
    }
    let files = Files::new(&paths, true)?;
    let server = Server::http(("127.0.0.1", port)).map_err(|e| format!("cannot listen on port {port}: {e}"))?;
    eprintln!("topo serve: http://127.0.0.1:{port}/");
    eprintln!("  app: {}", web.display());
    eprintln!("  files: {} ({})", files.base.display(), files.roots.iter().map(|r| r.strip_prefix(&files.base).unwrap_or(r).display().to_string()).collect::<Vec<_>>().join(", "));
    for req in server.incoming_requests() {
        handle(&files, web, req);
    }
    Ok(())
}

/// `topo site <out> <dir>…`: the app plus the `.ts` files under the given
/// folders (only those), with a manifest, ready for any static web server
/// (read-only there: visitors' edits stay in their browser).
pub fn site(out: &Path, paths: &[PathBuf], web: &Path) -> Result<(), String> {
    let pkg = web.join("pkg");
    if !pkg.join("topo_wasm_bg.wasm").exists() {
        return Err("the app's engine is not built: run web/build.sh".into());
    }
    let copy = |from: &Path, to: &Path| -> Result<(), String> {
        if let Some(d) = to.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::copy(from, to).map(|_| ()).map_err(|e| format!("{}: {e}", from.display()))
    };
    copy(&web.join("index.html"), &out.join("index.html"))?;
    for dir in ["app", "pkg"] {
        for e in std::fs::read_dir(web.join(dir)).map_err(|e| e.to_string())?.flatten() {
            if e.path().is_file() {
                copy(&e.path(), &out.join(dir).join(e.file_name()))?;
            }
        }
    }
    // Only the folders asked for: a site is for publishing.
    let files = Files::new(paths, false)?;
    let mut manifest = files.manifest();
    let mut n = 0;
    for f in manifest["files"].as_array_mut().unwrap() {
        let id = f["id"].as_str().unwrap().to_string();
        let src = files.base.join(&id);
        let text = std::fs::read_to_string(&src).map_err(|e| format!("{id}: {e}"))?;
        f["project"] = json!(text.contains("export default") && !id.ends_with(".measured.ts"));
        copy(&src, &out.join("files").join(&id))?;
        n += 1;
    }
    manifest["writable"] = json!(false);
    std::fs::write(out.join("files").join("manifest.json"), serde_json::to_string_pretty(&manifest).unwrap()).map_err(|e| e.to_string())?;
    println!("wrote the app and {n} files to {} — serve that folder with any web server", out.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_stay_inside_the_roots() {
        let f = Files { base: PathBuf::from("/r"), roots: vec![PathBuf::from("/r/examples"), PathBuf::from("/r/projects")] };
        assert_eq!(f.path("examples/a.ts"), Some(PathBuf::from("/r/examples/a.ts")));
        assert_eq!(f.path("projects/new/b.ts"), Some(PathBuf::from("/r/projects/new/b.ts")));
        assert_eq!(f.path("crates/x.ts"), None, "outside the roots");
        assert_eq!(f.path("examples/../crates/x.ts"), None);
        assert_eq!(f.path("/etc/passwd.ts"), None);
        assert_eq!(f.path("examples/a.rs"), None, "only .ts files");
        assert_eq!(f.id(Path::new("/r/examples/b/c.ts")).as_deref(), Some("examples/b/c.ts"));
        assert_eq!(common_ancestor(&[PathBuf::from("/r/examples"), PathBuf::from("/r/projects/x")]), PathBuf::from("/r"));
        assert_eq!(percent_decode("a%20b%2Fc"), "a b/c");
    }
}
