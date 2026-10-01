//! The workspace people and agents share: a model (the hypothesis), a scan
//! (the evidence), the model's placement in the scan, the issues raised
//! about them and the proposals made to settle them.
//!
//! Everything is done through [`Workspace::call`], as a participant: a
//! person's click in the app and an agent's tool call are the same call.
//! The project is a git repository ([`crate::git`]): the model's files,
//! the issues (`issues/N.json`) and what is kept about the scan
//! (`<model>.scan.json`: its path, the placement, named views, rooms,
//! fitted surfaces) are files in it, and every change is a commit by
//! whoever made it. The workspace never steers anyone's view: it hands out
//! view states to follow as links.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::git::Repo;
use crate::issues::{now, priority_from, priority_name, Body, Issue, Place, Store, Tally};
use crate::scan::Scan;
use crate::{contradicted, evidence, group_table, model_hash, predict_with, register, score, Evidence, FaceSamples, Placement, Prediction, V3};

pub const INSTRUCTIONS: &str = "\
This is a topo-cad scan workspace: a model (a TypeScript script: the hypothesis) worked against a lidar scan (the \
evidence) by people and agents together. Coordinates: the model frame has z up with the floor at z = 0 (metres); \
the scan frame is the scan's own (also z up). The model is placed in the scan by a turn about the vertical (yaw) \
and a translation. Tools take `frame: \"model\"` (the default) or `\"scan\"`. Evidence per member: scan points on \
its faces, and sensor rays that pass through its solid (it is not there, or not exactly there) or end on it.\n\
The work is organised as issues: questions and faults pinned to places, each with a thread. Start with `status`, \
then `issues` (list gives the order to work in). Take an issue by setting it `doing` (it is then assigned to you), \
look (`render_section`, `members`, `walls`, `studs`, `measure`, `probe`, `fit_surface`), and say what you found in \
its thread, briefly. Pass `issue: N` to a tool to record its use in that thread. You cannot move anyone's view: \
attach a `view` to a comment instead (a partial view state: only the fields that matter, e.g. a section and its \
colouring), with a `view_label`; people follow it as a link and can go back. To change the model, `propose` a \
diff in the issue: it is evaluated and scored against the scan, and shown to people as before / after / changed. \
A person (or you, if they said so) applies it with `proposal`. A comment ending in a question mark leaves the card \
waiting on the person who raised it; `wait` returns what others add.\n\
The scan is evidence with noise, never the truth: a surface in it may be a stud's face, sheathing seen between \
studs, a finish, insulation or clutter, and which model face it should be compared with is a judgement. To pin a \
dimension down: someone says what a surface is; fit it with `fit_surface`, choosing the support (box, thickness, \
expected normal, the normal filter); check the fit's spread, coverage and neighbours; compare it with the model \
face it corresponds to; only then propose an edit.";

pub struct Model {
    /// The entry file, absolute.
    pub path: String,
    /// Of the contents of its files, as evaluated.
    pub hash: u64,
    pub pred: Prediction,
    pub samples: FaceSamples,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PlacementJson {
    pub yaw_deg: f32,
    pub t: V3,
}
impl From<Placement> for PlacementJson {
    fn from(p: Placement) -> Self {
        PlacementJson { yaw_deg: p.yaw.to_degrees(), t: p.t }
    }
}
impl From<PlacementJson> for Placement {
    fn from(p: PlacementJson) -> Self {
        Placement { yaw: p.yaw_deg.to_radians(), t: p.t }
    }
}

/// A view state kept by name (see the app's view links: any subset of a
/// pane's kind, camera, colouring, filters and layers).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamedView {
    pub name: String,
    pub state: Value,
}

/// A section kept by name: the `render_section` arguments that make it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionDef {
    pub name: String,
    pub args: Value,
}

/// A room: a named box in the model frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room {
    pub name: String,
    pub min: V3,
    pub max: V3,
    #[serde(default)]
    pub note: String,
}

/// A surface someone said what it is, fitted: a plane patch in the model
/// frame. Evidence about the building, kept to compare with the model as
/// the model changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Surface {
    pub name: String,
    /// What it is, in words ("sheathing seen between the studs").
    pub describes: String,
    pub by: String,
    pub centre: V3,
    pub normal: V3,
    pub u: V3,
    pub v: V3,
    pub ext: [f32; 4],
    pub rms: f32,
    pub count: usize,
    /// The support it was fitted with (the tool's arguments).
    pub support: Value,
}

/// What is kept about the scan, beside the model (`<model>.scan.json`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Items {
    #[serde(default)]
    pub scan: Option<String>,
    #[serde(default)]
    pub placement: Option<PlacementJson>,
    #[serde(default)]
    pub views: Vec<NamedView>,
    #[serde(default)]
    pub sections: Vec<SectionDef>,
    #[serde(default)]
    pub rooms: Vec<Room>,
    #[serde(default)]
    pub surfaces: Vec<Surface>,
}

#[derive(Debug, Clone, Default)]
pub struct Presence {
    /// When they last did something (a call, or the app checking in).
    pub seen: f64,
    /// In `wait`: listening for others.
    pub waiting: bool,
}

/// A proposal evaluated: its model and that model's evidence.
pub struct Evaluated {
    pub pred: Prediction,
    pub ev: Vec<Evidence>,
}

pub struct Workspace {
    pub repo: Repo,
    /// The model's entry file, relative to the repository's root.
    pub model_rel: String,
    model_abs: String,
    items_path: PathBuf,
    pub scan: Option<Arc<Scan>>,
    pub model: Option<Model>,
    /// Why the model as it is on disk does not evaluate.
    pub model_error: Option<String>,
    pub placement: Option<Placement>,
    /// Bumped whenever the model is re-evaluated.
    pub model_rev: u64,
    ev: Option<((u64, [u32; 4]), Vec<Evidence>)>,
    proposals: HashMap<String, Arc<Evaluated>>,
    pub store: Store,
    pub items: Items,
    pub presence: HashMap<String, Presence>,
    /// Up to when each participant has had what the others did (`wait`).
    cursors: HashMap<String, f64>,
    /// Bumped on every change a viewer would show.
    pub rev: u64,
}

pub(crate) fn arr3(v: &Value) -> Option<V3> {
    let a = v.as_array()?;
    if a.len() != 3 {
        return None;
    }
    Some([a[0].as_f64()? as f32, a[1].as_f64()? as f32, a[2].as_f64()? as f32])
}
pub(crate) fn f3(p: V3) -> String {
    format!("[{:.3}, {:.3}, {:.3}]", p[0], p[1], p[2])
}
fn pkey(p: &Placement) -> [u32; 4] {
    [p.yaw.to_bits(), p.t[0].to_bits(), p.t[1].to_bits(), p.t[2].to_bits()]
}
pub(crate) fn describe(p: &Placement) -> String {
    format!("yaw {:.3}°, t {}", p.yaw.to_degrees(), f3(p.t))
}
fn text(s: String) -> Vec<Value> {
    vec![json!({"type": "text", "text": s})]
}

/// Members supported, contradicted (≥ 35% of rays through them) and unseen.
pub fn tally_counts(ev: &[Evidence]) -> Tally {
    let mut t = Tally::default();
    for e in ev {
        if e.rays.pierced + e.rays.ended < 100 && e.points < 50 {
            t.unseen += 1;
        } else if e.rays.pierce_frac() >= 0.35 {
            t.contradicted += 1;
        } else {
            t.supported += 1;
        }
    }
    t
}

impl Workspace {
    // ----- state ------------------------------------------------------------------

    /// Opens the project a model is in (a git repository), with a scan to
    /// use (else the one the project names). The scan is loaded on first
    /// use, or by [`Workspace::load_scan`].
    pub fn open(model: &Path, scan: Option<&str>) -> Result<Workspace> {
        let model = model.canonicalize().with_context(|| format!("{}", model.display()))?;
        let repo = Repo::open(model.parent().context("model path")?)?;
        let model_rel = repo.rel(&model)?;
        if repo.head().is_none() {
            // A new repository: everything in it is the project as it starts.
            repo.commit(&["."], "Start of the scan project", "topo-cad")?;
        }
        let items_path = model.with_extension("scan.json");
        let mut items: Items = std::fs::read_to_string(&items_path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let store = Store::load(&repo.root.join("issues"))?;
        let given = scan.map(|s| std::fs::canonicalize(s).map(|p| p.display().to_string()).unwrap_or_else(|_| s.to_string()));
        let changed = given.is_some() && given != items.scan;
        if changed {
            items.scan = given;
            items.placement = None;
        }
        let placement = items.placement.map(Into::into);
        let mut w = Workspace { repo, model_rel, model_abs: model.display().to_string(), items_path, scan: None, model: None, model_error: None, placement, model_rev: 0, ev: None, proposals: HashMap::new(), store, items, presence: HashMap::new(), cursors: HashMap::new(), rev: 1 };
        if changed {
            w.save_items("topo-cad")?;
        }
        w.load_model();
        Ok(w)
    }

    pub fn touch(&mut self) {
        self.rev += 1;
    }

    fn commit_paths(&self, paths: &[PathBuf], message: &str, who: &str) -> Result<()> {
        let rels: Vec<String> = paths.iter().map(|p| self.repo.rel(p)).collect::<Result<_>>()?;
        self.repo.commit(&rels.iter().map(String::as_str).collect::<Vec<_>>(), message, &self.store.name_of(who))?;
        Ok(())
    }

    pub(crate) fn save_items(&mut self, who: &str) -> Result<()> {
        std::fs::write(&self.items_path, serde_json::to_string_pretty(&self.items)? + "\n")?;
        self.commit_paths(&[self.items_path.clone()], "Scan workspace", who)?;
        self.touch();
        Ok(())
    }

    fn save_issue(&mut self, id: u32, who: &str, what: &str) -> Result<()> {
        let path = self.store.save(id)?;
        self.commit_paths(&[path], &format!("#{id}: {what}"), who)?;
        self.touch();
        Ok(())
    }

    /// Makes `id` known as a participant (a person or an agent) and notes
    /// that they are here.
    pub fn join(&mut self, id: &str, kind: &str) -> Result<()> {
        let (_, wrote) = self.store.participant(id, kind)?;
        if let Some(path) = wrote {
            self.commit_paths(&[path], &format!("{} joined", self.store.name_of(id)), id)?;
            self.touch();
        }
        self.presence.entry(id.to_string()).or_default().seen = now();
        Ok(())
    }

    /// "waiting" (listening for others), "working", "here" or "away".
    pub fn presence_of(&self, id: &str) -> &'static str {
        let agent = self.store.participants.iter().any(|p| p.id == id && p.kind == "agent");
        match self.presence.get(id) {
            Some(p) if p.waiting => "waiting",
            Some(p) if now() - p.seen < if agent { 45.0 } else { 20.0 } => if agent { "working" } else { "here" },
            _ => "away",
        }
    }

    /// Evaluates the model as it is on disk. A model that does not
    /// evaluate leaves the last good one in place, with the error.
    pub fn load_model(&mut self) {
        let hash = model_hash(&self.model_abs);
        match predict_with(&self.model_abs, &mut |id| std::fs::read_to_string(id).ok()) {
            Ok(pred) => {
                let samples = FaceSamples::new(&pred, 0.05);
                self.model = Some(Model { path: self.model_abs.clone(), hash, pred, samples });
                self.model_error = None;
                self.model_rev += 1;
            }
            Err(e) => {
                self.model_error = Some(format!("{e:#}"));
                if let Some(m) = &mut self.model {
                    m.hash = hash;
                }
            }
        }
        self.touch();
    }

    /// Re-evaluates the model when its files changed on disk.
    pub fn sync(&mut self) {
        let seen = self.model.as_ref().map(|m| m.hash);
        if seen != Some(model_hash(&self.model_abs)) && (seen.is_some() || self.model_error.is_none()) {
            self.load_model();
        }
    }

    /// A file of the project was saved from the app: a commit by them.
    pub fn saved(&mut self, who: &str, path: &Path) -> Result<()> {
        let rel = self.repo.rel(path)?;
        self.repo.commit(&[&rel], &format!("Edit {rel}"), &self.store.name_of(who))?;
        self.sync();
        Ok(())
    }

    pub fn load_scan(&mut self) -> Result<()> {
        if self.scan.is_none() {
            let path = self.items.scan.clone().context("the project names no scan: start the service with one")?;
            self.scan = Some(Arc::new(Scan::load(&path)?));
            self.touch();
        }
        Ok(())
    }

    pub(crate) fn scan(&mut self) -> Result<&Scan> {
        self.load_scan()?;
        Ok(self.scan.as_deref().unwrap())
    }

    pub(crate) fn model(&self) -> Result<&Model> {
        self.model.as_ref().with_context(|| format!("the model does not evaluate: {}", self.model_error.as_deref().unwrap_or("not loaded")))
    }

    /// The placement, registering the model in the scan when there is none.
    pub fn placement(&mut self) -> Result<Placement> {
        if let Some(p) = self.placement {
            return Ok(p);
        }
        self.scan()?;
        let (scan, m) = (self.scan.as_ref().unwrap(), self.model()?);
        let r = register(&m.pred, &m.samples, &scan.sub, scan.wall_axis, scan.floor_z);
        self.set_placement(r.placement, "topo-cad")?;
        Ok(r.placement)
    }

    pub(crate) fn set_placement(&mut self, p: Placement, who: &str) -> Result<()> {
        self.placement = Some(p);
        self.items.placement = Some(p.into());
        self.save_items(who)
    }

    /// The evidence of the current model and placement, if computed.
    pub fn current_evidence(&self) -> Option<&[Evidence]> {
        let (k, ev) = self.ev.as_ref()?;
        (k.0 == self.model_rev && Some(k.1) == self.placement.as_ref().map(pkey)).then_some(&ev[..])
    }

    pub fn evidence(&mut self) -> Result<Vec<Evidence>> {
        let pl = self.placement()?;
        let key = (self.model_rev, pkey(&pl));
        if let Some((k, ev)) = &self.ev {
            if *k == key {
                return Ok(ev.clone());
            }
        }
        self.scan()?;
        let (scan, m) = (self.scan.as_ref().unwrap(), self.model()?);
        let ev = evidence(&m.pred, &m.samples, &pl, &scan.xyz, &scan.used, scan.sensor.as_deref(), 0.05, 4);
        self.ev = Some((key, ev.clone()));
        Ok(ev)
    }

    pub(crate) fn score_text(&mut self, pl: &Placement) -> Result<String> {
        self.scan()?;
        let (scan, m) = (self.scan.as_ref().unwrap(), self.model()?);
        let (inl, rms) = score(&m.samples, &scan.sub, pl);
        Ok(format!("{:.1}% of scan points within 3 cm of a model face (rms {:.1} mm)", 100.0 * inl, rms * 1e3))
    }

    // ----- calls --------------------------------------------------------------------

    /// Runs a tool as a participant. With `issue: N` among the arguments,
    /// the call is noted in that issue's thread.
    pub fn call(&mut self, who: &str, name: &str, args: &Value) -> Result<Vec<Value>> {
        self.presence.entry(who.to_string()).or_default().seen = now();
        self.sync();
        let out = self.call_inner(who, name, args)?;
        if !matches!(name, "issues" | "propose" | "proposal" | "wait" | "status" | "rename") {
            if let Some(id) = args.get("issue").and_then(|v| v.as_u64()) {
                let first = out.iter().find_map(|c| c["text"].as_str()).and_then(|t| t.lines().next()).unwrap_or("").to_string();
                let summary: String = first.chars().take(160).collect();
                self.store.push(id as u32, who, Body::Tool { name: name.to_string(), summary })?;
                self.save_issue(id as u32, who, name)?;
            }
        }
        self.presence.entry(who.to_string()).or_default().seen = now();
        self.touch();
        Ok(out)
    }

    fn call_inner(&mut self, who: &str, name: &str, args: &Value) -> Result<Vec<Value>> {
        match name {
            "status" => Ok(text(self.status()?)),
            "rename" => {
                let path = self.store.rename(who, args.get("name").and_then(|v| v.as_str()).context("name")?)?;
                self.commit_paths(&[path], "Renamed", who)?;
                Ok(text(format!("you are now {}", self.store.name_of(who))))
            }
            "set_placement" | "refine_placement" | "register_placement" | "align_points" => self.placement_tool(who, name, args),
            "model_read" => self.model_read(args),
            "propose" => self.propose(who, args),
            "proposal" => self.proposal(who, args),
            "members" => self.members(args),
            "render_section" => self.render(who, args),
            "measure" => self.measure(args),
            "probe" => self.probe(args),
            "walls" => self.walls(args),
            "studs" => self.studs(args),
            "fit_surface" => self.fit_surface(who, args),
            "surfaces" => self.surfaces(who, args),
            "rooms" => self.rooms(who, args),
            "views" => self.views(who, args),
            "sections" => self.sections(who, args),
            "issues" => self.issues(who, args),
            "wait" => Ok(text(self.activity_for(who))),
            _ => bail!("unknown tool {name}"),
        }
    }

    fn status(&mut self) -> Result<String> {
        let mut s = format!("project: {} (branch {})\nmodel: {}\n", self.repo.root.display(), self.repo.branch(), self.model_rel);
        s += &format!("scan: {}\n", self.items.scan.as_deref().unwrap_or("(none)"));
        if let Some(e) = &self.model_error {
            s += &format!("THE MODEL DOES NOT EVALUATE as it is on disk (the last good one is used): {e}\n");
        }
        let pl = self.placement()?;
        let scan = self.scan()?;
        s += &format!("scan: {} points ({} used), floor at z = {:.3}, sensor rays {}\n", scan.xyz.len(), scan.used.len(), scan.floor_z, if scan.sensor.is_some() { "yes" } else { "no" });
        let m = self.model()?;
        s += &format!("model {:?}: {} members\n", m.pred.name, m.pred.members.len());
        if let Some(r) = &m.pred.report {
            s += &format!("fit report:\n{r}\n");
        }
        s += &format!("placement (model → scan): {}\n  {}\n", describe(&pl), self.score_text(&pl)?);
        let ev = self.evidence()?;
        let m = self.model()?;
        s += &format!("{}\n\n{}", Self::tally(&ev), group_table(&m.pred, &ev));
        s += "most contradicted:\n";
        for k in contradicted(&m.pred, &ev, 200, 10) {
            s += &format!("  {:<56} {:>4.0}% of {:>6} rays through it; {:>6} points\n", m.pred.members[k].path, 100.0 * ev[k].rays.pierce_frac(), ev[k].rays.pierced + ev[k].rays.ended, ev[k].points);
        }
        s += "participants: ";
        s += &self.store.participants.iter().map(|p| format!("{} ({}, {})", p.name, p.kind, self.presence_of(&p.id))).collect::<Vec<_>>().join(", ");
        let open = self.store.issues.iter().filter(|i| i.status != "done").count();
        s += &format!("\nissues: {open} not done of {} (see `issues`)\n", self.store.issues.len());
        Ok(s)
    }

    fn placement_tool(&mut self, who: &str, name: &str, args: &Value) -> Result<Vec<Value>> {
        let before = self.placement()?;
        match name {
            "set_placement" => {
                let mut p = before;
                if let Some(y) = args.get("yaw_deg").and_then(|v| v.as_f64()) {
                    p.yaw = (y as f32).to_radians();
                }
                if let Some(t) = args.get("t").and_then(arr3) {
                    p.t = t;
                }
                if let Some(dy) = args.get("dyaw_deg").and_then(|v| v.as_f64()) {
                    let pivot = match args.get("pivot").and_then(arr3) {
                        Some(q) => self.to_scan(q, Self::frame_is_model(args)?)?,
                        None => {
                            // The model's middle, placed.
                            let m = self.model()?;
                            let n = m.pred.members.len().max(1) as f32;
                            p.apply(m.pred.members.iter().fold([0.0f32; 3], |a, x| [a[0] + x.obb.centre[0] / n, a[1] + x.obb.centre[1] / n, a[2] + x.obb.centre[2] / n]))
                        }
                    };
                    p = p.turned_about((dy as f32).to_radians(), pivot);
                }
                if let Some(dt) = args.get("dt").and_then(arr3) {
                    // Given in the model's axes by default: turn into the scan's.
                    let dt = if Self::frame_is_model(args)? { p.rotate(dt) } else { dt };
                    p.t = [p.t[0] + dt[0], p.t[1] + dt[1], p.t[2] + dt[2]];
                }
                self.try_placement(who, before, p, args, "set")
            }
            "refine_placement" => {
                let gates: Vec<f32> = args.get("gates").and_then(|g| g.as_array()).map(|a| a.iter().filter_map(|x| x.as_f64().map(|v| v as f32)).collect()).unwrap_or(vec![0.25, 0.1, 0.05, 0.03]);
                self.scan()?;
                let (scan, m) = (self.scan.as_ref().unwrap(), self.model()?);
                let (p, _, _) = crate::icp(&m.samples, &scan.sub, before, &gates);
                self.try_placement(who, before, p, args, "refined (ICP)")
            }
            "register_placement" => {
                self.scan()?;
                let (scan, m) = (self.scan.as_ref().unwrap(), self.model()?);
                let r = register(&m.pred, &m.samples, &scan.sub, scan.wall_axis, scan.floor_z);
                let mut s = String::from("candidates (yaw from the scan's wall direction, each refined by ICP):\n");
                for (k, (p, inl)) in r.candidates.iter().enumerate() {
                    s += &format!("  {k}: {}; {:.1}% within 3 cm\n", describe(p), 100.0 * inl);
                }
                let pick = args.get("pick").and_then(|v| v.as_u64()).map(|k| r.candidates.get(k as usize).map(|c| c.0).context("no such candidate")).transpose()?;
                let apply = args.get("apply").and_then(|v| v.as_bool()).unwrap_or(false);
                let mut out = self.try_placement(who, before, pick.unwrap_or(r.placement), &json!({"dry_run": !apply}), "registered")?;
                out.insert(0, json!({"type": "text", "text": s}));
                Ok(out)
            }
            _ => {
                let pairs_v = args.get("pairs").and_then(|v| v.as_array()).context("pairs: [{model: [x,y,z], scan: [x,y,z]}]")?;
                let mut pairs = Vec::new();
                for (k, pv) in pairs_v.iter().enumerate() {
                    pairs.push((arr3(&pv["model"]).with_context(|| format!("pairs[{k}].model: [x, y, z] in the model frame"))?, arr3(&pv["scan"]).with_context(|| format!("pairs[{k}].scan: [x, y, z] in the scan frame"))?));
                }
                let p = Placement::from_pairs(&pairs, &before).context("no pairs")?;
                let mut out = self.try_placement(who, before, p, args, "aligned to the pairs")?;
                let res: Vec<String> = pairs.iter().map(|(m, s)| {
                    let q = p.apply(*m);
                    format!("  model {} → {} vs scan {}: {:.3} m", f3(*m), f3(q), f3(*s), ((q[0] - s[0]).powi(2) + (q[1] - s[1]).powi(2) + (q[2] - s[2]).powi(2)).sqrt())
                }).collect();
                out.push(json!({"type": "text", "text": format!("pair residuals:\n{}", res.join("\n"))}));
                Ok(out)
            }
        }
    }

    /// A file of the model: the entry, or `file` relative to its folder.
    fn resolve_file(&self, file: Option<&str>) -> Result<PathBuf> {
        let p = match file {
            None => PathBuf::from(&self.model_abs),
            Some(f) if Path::new(f).is_absolute() => PathBuf::from(f),
            Some(f) => Path::new(&self.model_abs).parent().unwrap().join(f),
        };
        self.repo.rel(&p)?;
        Ok(p)
    }

    fn model_read(&mut self, args: &Value) -> Result<Vec<Value>> {
        let dir = Path::new(&self.model_abs).parent().unwrap().to_path_buf();
        if args.get("list").and_then(|v| v.as_bool()).unwrap_or(false) {
            let mut names: Vec<String> = std::fs::read_dir(&dir)?.flatten().map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.ends_with(".ts")).collect();
            names.sort();
            return Ok(text(format!("{}:\n{}", dir.display(), names.join("\n"))));
        }
        let file = self.resolve_file(args.get("file").and_then(|v| v.as_str()))?;
        let src = std::fs::read_to_string(&file).with_context(|| format!("reading {}", file.display()))?;
        let lines: Vec<&str> = src.lines().collect();
        let a = args.get("start_line").and_then(|v| v.as_u64()).unwrap_or(1).max(1) as usize;
        let b = args.get("end_line").and_then(|v| v.as_u64()).map_or(lines.len(), |x| x as usize).min(lines.len());
        let body: Vec<String> = (a..=b).map(|k| format!("{k:5}  {}", lines[k - 1])).collect();
        Ok(text(format!("{} (lines {a}–{b} of {}):\n{}", file.display(), lines.len(), body.join("\n"))))
    }

    // ----- proposals ------------------------------------------------------------------

    /// The model with some files replaced (absolute path → text), and its
    /// evidence at the current placement.
    fn evaluate(&mut self, files: &HashMap<String, String>) -> Result<Evaluated> {
        let pred = predict_with(&self.model_abs, &mut |id| files.get(id).cloned().or_else(|| std::fs::read_to_string(id).ok()))?;
        let samples = FaceSamples::new(&pred, 0.05);
        let pl = self.placement()?;
        self.scan()?;
        let scan = self.scan.as_ref().unwrap();
        let ev = evidence(&pred, &samples, &pl, &scan.xyz, &scan.used, scan.sensor.as_deref(), 0.05, 4);
        Ok(Evaluated { pred, ev })
    }

    /// A proposal's model and evidence (evaluated once per commit).
    pub fn proposal_evaluated(&mut self, commit: &str) -> Result<Arc<Evaluated>> {
        if let Some(e) = self.proposals.get(commit) {
            return Ok(e.clone());
        }
        let mut files = HashMap::new();
        for rel in self.repo.stat(commit)?.files {
            if let Some(t) = self.repo.file_at(commit, &rel) {
                files.insert(self.repo.root.join(&rel).display().to_string(), t);
            }
        }
        let e = Arc::new(self.evaluate(&files)?);
        self.proposals.insert(commit.to_string(), e.clone());
        Ok(e)
    }

    fn find_proposal(&self, issue: u32, n: u32) -> Result<(usize, String, String, Option<String>)> {
        let i = self.store.get(issue)?;
        i.entries.iter().enumerate().find_map(|(k, e)| match &e.body {
            Body::Proposal { n: m, commit, state, applied_as, .. } if *m == n => Some((k, commit.clone(), state.clone(), applied_as.clone())),
            _ => None,
        }).with_context(|| format!("issue #{issue} has no proposal {n}"))
    }

    fn propose(&mut self, who: &str, args: &Value) -> Result<Vec<Value>> {
        let issue = args.get("issue").and_then(|v| v.as_u64()).context("issue: the id of the issue this settles")? as u32;
        self.store.get(issue)?;
        let title = args.get("title").and_then(|v| v.as_str()).context("title: a line saying what changes")?.to_string();
        // The new text of each file touched: exact replacements, or whole contents.
        let mut texts: Vec<(PathBuf, String)> = Vec::new();
        let slot = |texts: &mut Vec<(PathBuf, String)>, p: PathBuf| -> Result<usize> {
            Ok(match texts.iter().position(|t| t.0 == p) {
                Some(k) => k,
                None => {
                    let src = std::fs::read_to_string(&p).with_context(|| format!("reading {}", p.display()))?;
                    texts.push((p, src));
                    texts.len() - 1
                }
            })
        };
        for (k, e) in args.get("edits").and_then(|v| v.as_array()).into_iter().flatten().enumerate() {
            let p = self.resolve_file(e.get("file").and_then(|v| v.as_str()))?;
            let (o, n) = (e["old"].as_str().with_context(|| format!("edits[{k}].old"))?, e["new"].as_str().with_context(|| format!("edits[{k}].new"))?);
            let s = slot(&mut texts, p)?;
            let c = texts[s].1.matches(o).count();
            if c != 1 {
                bail!("edits[{k}].old occurs {c} times in {}; it must occur exactly once (add surrounding context)", texts[s].0.display());
            }
            texts[s].1 = texts[s].1.replacen(o, n, 1);
        }
        for (k, f) in args.get("files").and_then(|v| v.as_array()).into_iter().flatten().enumerate() {
            let p = self.resolve_file(Some(f["file"].as_str().with_context(|| format!("files[{k}].file"))?))?;
            let t = f["text"].as_str().with_context(|| format!("files[{k}].text"))?.to_string();
            match texts.iter_mut().find(|x| x.0 == p) {
                Some(x) => x.1 = t,
                None => texts.push((p, t)),
            }
        }
        if texts.is_empty() {
            bail!("nothing to propose: give edits [{{file?, old, new}}] or files [{{file, text}}]");
        }
        let overlay: HashMap<String, String> = texts.iter().map(|(p, t)| (p.display().to_string(), t.clone())).collect();
        let after = match self.evaluate(&overlay) {
            Ok(e) => e,
            Err(e) => bail!("the model with this change does not evaluate (nothing proposed): {e:#}"),
        };
        let ev_before = self.evidence()?;
        let (tb, ta) = (tally_counts(&ev_before), tally_counts(&after.ev));
        let before_paths: std::collections::BTreeSet<&str> = self.model()?.pred.members.iter().map(|m| m.path.as_str()).collect();
        let after_paths: std::collections::BTreeSet<&str> = after.pred.members.iter().map(|m| m.path.as_str()).collect();
        let added: Vec<String> = after_paths.difference(&before_paths).map(|s| s.to_string()).collect();
        let removed: Vec<String> = before_paths.difference(&after_paths).map(|s| s.to_string()).collect();
        let n = self.store.get(issue)?.entries.iter().filter(|e| matches!(e.body, Body::Proposal { .. })).count() as u32 + 1;
        let rels: Vec<(String, String)> = texts.iter().map(|(p, t)| Ok((self.repo.rel(p)?, t.clone()))).collect::<Result<_>>()?;
        let commit = self.repo.propose(&format!("{issue}-{n}"), &rels, &format!("{title}\n\nProposal {n} of issue #{issue}."), &self.store.name_of(who))?;
        let st = self.repo.stat(&commit)?;
        let mut s = format!("proposal {n} of issue #{issue}: {title}\n{} (+{} −{})\n", st.files.join(", "), st.added, st.removed);
        s += &format!("members: {} → {} ({} added, {} removed)\n", before_paths.len(), after_paths.len(), added.len(), removed.len());
        let list = |v: &[String]| v.iter().take(12).map(String::as_str).collect::<Vec<_>>().join(", ");
        if !added.is_empty() {
            s += &format!("  added: {}{}\n", list(&added), if added.len() > 12 { ", …" } else { "" });
        }
        if !removed.is_empty() {
            s += &format!("  removed: {}{}\n", list(&removed), if removed.len() > 12 { ", …" } else { "" });
        }
        s += &format!("before: {}\nafter:  {}\n\n{}", Self::tally(&ev_before), Self::tally(&after.ev), group_table(&after.pred, &after.ev));
        s += "\nNot applied: people see it in the issue as a diff (before / after / changed). `proposal` applies or withdraws it.\n";
        self.store.push(issue, who, Body::Proposal { n, title: title.clone(), commit: commit.clone(), files: st.files, added: st.added, removed: st.removed, members_added: added, members_removed: removed, before: tb, after: ta, state: "open".into(), applied_as: None })?;
        self.proposals.insert(commit, Arc::new(after));
        self.save_issue(issue, who, &format!("proposal {n}: {title}"))?;
        Ok(text(s))
    }

    fn proposal(&mut self, who: &str, args: &Value) -> Result<Vec<Value>> {
        let issue = args.get("issue").and_then(|v| v.as_u64()).context("issue")? as u32;
        let n = args.get("n").and_then(|v| v.as_u64()).context("n: the proposal's number in the issue")? as u32;
        let (k, commit, state, applied_as) = self.find_proposal(issue, n)?;
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("diff");
        let name = self.store.name_of(who);
        let (new_state, new_applied, said) = match action {
            "diff" => return Ok(text(self.repo.diff(&commit)?)),
            "apply" => {
                if state != "open" {
                    bail!("proposal {n} is {state}, not open");
                }
                let c = self.repo.apply(&commit, &name)?;
                ("applied", Some(c), format!("applied proposal {n}"))
            }
            "reject" => {
                if state != "open" {
                    bail!("proposal {n} is {state}, not open");
                }
                ("rejected", None, format!("rejected proposal {n}"))
            }
            "revert" => {
                let c = applied_as.filter(|_| state == "applied").with_context(|| format!("proposal {n} is {state}, not applied"))?;
                self.repo.revert(&c, &name)?;
                ("reverted", None, format!("reverted proposal {n}"))
            }
            a => bail!("action: diff, apply, reject or revert (not {a:?})"),
        };
        if let Body::Proposal { state, applied_as, .. } = &mut self.store.get_mut(issue)?.entries[k].body {
            *state = new_state.to_string();
            *applied_as = new_applied;
        }
        self.store.push(issue, who, Body::Change { what: said.clone() })?;
        self.save_issue(issue, who, &said)?;
        self.load_model();
        let mut s = format!("{said}.\n");
        if let Ok(ev) = self.evidence() {
            s += &format!("now: {}\n", Self::tally(&ev));
        }
        Ok(text(s))
    }

    // ----- issues ---------------------------------------------------------------------

    fn issue_line(&self, i: &Issue) -> String {
        let at = match (&i.place, self.placement) {
            (Some(p), Some(pl)) => format!(" at model {}", f3(pl.inverse(p.centre()))),
            _ => String::new(),
        };
        let who = i.assignee.as_ref().map_or(String::new(), |a| format!(", {}'s", self.store.name_of(a)));
        let waiting = i.waiting_on.as_ref().map_or(String::new(), |a| format!(" on {}", self.store.name_of(a)));
        format!("#{} [{}{waiting}, {}, {}{who}] {:?}{at}", i.id, i.status, priority_name(i.priority), i.category, i.title)
    }

    fn entry_line(&self, e: &crate::issues::Entry) -> String {
        let who = self.store.name_of(&e.who);
        match &e.body {
            Body::Comment { text, view_label, view } => format!("{who}: {text}{}", if view.is_some() { format!(" [view: {}]", view_label.as_deref().unwrap_or("link")) } else { String::new() }),
            Body::Tool { name, summary } => format!("{who} ran {name}: {summary}"),
            Body::Evidence { title, text } => format!("{who}, evidence — {title}: {text}"),
            Body::Proposal { n, title, files, added, removed, before, after, state, .. } => format!("{who} proposed ({n}, {state}) {title:?}: {} +{added} −{removed}; contradicted {} → {}, supported {} → {}", files.join(", "), before.contradicted, after.contradicted, before.supported, after.supported),
            Body::Change { what } => format!("{who} ({what})"),
        }
    }

    fn issues(&mut self, who: &str, args: &Value) -> Result<Vec<Value>> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("list");
        let id = args.get("id").and_then(|v| v.as_u64()).map(|x| x as u32);
        match action {
            "list" => {
                let all = args.get("all").and_then(|v| v.as_bool()).unwrap_or(false);
                let cat = args.get("category").and_then(|v| v.as_str());
                let is: Vec<&Issue> = self.store.working_order().into_iter().filter(|i| (all || i.status != "done") && cat.is_none_or(|c| i.category == c)).collect();
                let mut s = format!("{} issue(s){}; in working order (doing, then open by priority; waiting = on someone to answer):\n", is.len(), if all { "" } else { " not done" });
                for i in is {
                    let last = i.entries.last().map_or(String::new(), |e| format!("  last: {}", self.entry_line(e)));
                    s += &format!("{}; {} entries.{}\n", self.issue_line(i), i.entries.len(), last);
                }
                Ok(text(s))
            }
            "get" => {
                let i = self.store.get(id.context("id")?)?.clone();
                let mut s = format!("{}\nraised by {}\n", self.issue_line(&i), self.store.name_of(&i.by));
                if let (Some(p), Ok(pl)) = (&i.place, self.placement()) {
                    let (a, b) = (pl.inverse(p.min), pl.inverse(p.max));
                    let (lo, hi): (V3, V3) = (std::array::from_fn(|k| a[k].min(b[k])), std::array::from_fn(|k| a[k].max(b[k])));
                    s += &format!("box (model frame, about) {} to {}\n", f3(lo), f3(hi));
                    if let Some(m) = &p.member {
                        s += &format!("pinned to the member {m}\n");
                    }
                    if let Some((k, d, _)) = self.nearest_member(pl.inverse(p.centre()), 0.5) {
                        s += &format!("nearest member: {} ({:.0} mm from its face)\n", self.model()?.pred.members[k].path, 1000.0 * d);
                    }
                }
                for e in &i.entries {
                    s += &format!("  {}\n", self.entry_line(e));
                }
                Ok(text(s))
            }
            "create" => {
                let title = args.get("title").and_then(|v| v.as_str()).context("title")?.to_string();
                let category = args.get("category").and_then(|v| v.as_str()).unwrap_or("scan");
                if !crate::issues::CATEGORIES.contains(&category) {
                    bail!("category: one of {}", crate::issues::CATEGORIES.join(", "));
                }
                let place = self.place_from(args)?;
                let id = self.store.create(who, &title, category, place, args.get("priority").and_then(|v| v.as_str()).map_or(2, priority_from));
                if let Some(t) = args.get("text").and_then(|v| v.as_str()) {
                    self.store.comment(id, who, t, args.get("view").cloned().filter(|v| !v.is_null()), args.get("view_label").and_then(|v| v.as_str()).map(String::from))?;
                }
                self.save_issue(id, who, &title)?;
                Ok(text(format!("issue #{id}")))
            }
            "comment" => {
                let id = id.context("id")?;
                let t = args.get("text").and_then(|v| v.as_str()).context("text")?;
                self.store.comment(id, who, t, args.get("view").cloned().filter(|v| !v.is_null()), args.get("view_label").and_then(|v| v.as_str()).map(String::from))?;
                if let Some(s) = args.get("status").and_then(|v| v.as_str()) {
                    self.update_issue(id, who, &json!({"status": s}))?;
                }
                self.save_issue(id, who, "comment")?;
                Ok(text("added".into()))
            }
            "evidence" => {
                let id = id.context("id")?;
                let title = args.get("title").and_then(|v| v.as_str()).context("title")?.to_string();
                let t = args.get("text").and_then(|v| v.as_str()).context("text")?.to_string();
                self.store.push(id, who, Body::Evidence { title, text: t })?;
                self.save_issue(id, who, "evidence")?;
                Ok(text("added".into()))
            }
            "update" => {
                let id = id.context("id")?;
                self.update_issue(id, who, args)?;
                self.save_issue(id, who, "update")?;
                Ok(text("updated".into()))
            }
            "delete" => {
                let id = id.context("id")?;
                self.store.get(id)?;
                self.store.issues.retain(|i| i.id != id);
                let path = self.store.dir.join(format!("{id}.json"));
                let _ = std::fs::remove_file(&path);
                self.commit_paths(&[path], &format!("#{id}: deleted"), who)?;
                self.touch();
                Ok(text("deleted".into()))
            }
            a => bail!("action: list, get, create, comment, evidence, update or delete (not {a:?})"),
        }
    }

    fn update_issue(&mut self, id: u32, who: &str, args: &Value) -> Result<()> {
        let status = args.get("status").and_then(|v| v.as_str());
        // Taking an issue in hand assigns it, unless it has someone.
        let mut assignee = match args.get("assignee") {
            Some(Value::Null) => Some(None),
            Some(v) => {
                let a = v.as_str().context("assignee: a participant's id or name, or null")?;
                let p = self.store.participants.iter().find(|p| p.id == a || p.name == a).with_context(|| format!("no participant {a:?}"))?;
                Some(Some(p.id.clone()))
            }
            None => None,
        };
        if status == Some("doing") && assignee.is_none() && self.store.get(id)?.assignee.is_none() {
            assignee = Some(Some(who.to_string()));
        }
        self.store.update(id, who, status, args.get("priority").and_then(|v| v.as_str()).map(priority_from), args.get("title").and_then(|v| v.as_str()), assignee, args.get("rank").and_then(|v| v.as_f64()))
    }

    /// Where an issue goes, from `point`, `box`, or `member` (in `frame`).
    fn place_from(&mut self, args: &Value) -> Result<Option<Place>> {
        let model = Self::frame_is_model(args)?;
        if let Some(f) = args.get("member").and_then(|v| v.as_str()) {
            let pl = self.placement()?;
            let ks = self.members_matching(Some(f))?;
            if ks.is_empty() {
                bail!("no member matches {f:?}");
            }
            let m = self.model()?;
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for p in ks.iter().flat_map(|&k| &m.pred.members[k].faces).flat_map(|f| &f.poly) {
                let q = pl.apply(*p);
                for a in 0..3 {
                    lo[a] = lo[a].min(q[a]);
                    hi[a] = hi[a].max(q[a]);
                }
            }
            return Ok(Some(Place { min: lo, max: hi, member: Some(if ks.len() == 1 { m.pred.members[ks[0]].path.clone() } else { f.to_string() }), selection: None }));
        }
        if let Some(b) = args.get("box") {
            let (a, c) = (arr3(&b["min"]).context("box: {min, max}")?, arr3(&b["max"]).context("box: {min, max}")?);
            let (a, c) = (self.to_scan(a, model)?, self.to_scan(c, model)?);
            return Ok(Some(Place { min: std::array::from_fn(|k| a[k].min(c[k])), max: std::array::from_fn(|k| a[k].max(c[k])), member: None, selection: None }));
        }
        if let Some(p) = args.get("point").and_then(arr3) {
            return Ok(Some(Place::about(self.to_scan(p, model)?, 0.1)));
        }
        Ok(None)
    }

    /// Whether what happens in an issue is for `who` to hear of: theirs
    /// (raised, assigned or taken part in), or nobody's yet.
    fn concerns(&self, i: &Issue, who: &str) -> bool {
        i.by == who || i.assignee.as_deref() == Some(who) || i.waiting_on.as_deref() == Some(who) || i.assignee.is_none() || i.entries.iter().any(|e| e.who == who)
    }

    fn news(&self, who: &str) -> Vec<(&Issue, Vec<&crate::issues::Entry>, bool)> {
        let since = self.cursors.get(who).copied().unwrap_or(0.0);
        let me = self.store.name_of(who);
        self.store.issues.iter().filter_map(|i| {
            let new: Vec<&crate::issues::Entry> = i.entries.iter().filter(|e| e.who != who && e.time > since && !matches!(e.body, Body::Tool { .. })).filter(|e| self.concerns(i, who) || matches!(&e.body, Body::Comment { text, .. } if text.contains(&format!("@{me}")))).collect();
            let raised = i.by != who && i.time > since && self.concerns(i, who);
            (!new.is_empty() || raised).then_some((i, new, raised))
        }).collect()
    }

    /// Whether others have done something `who` has not had from `wait`.
    pub fn has_activity(&self, who: &str) -> bool {
        !self.news(who).is_empty()
    }

    /// What others did since `who` last asked, in the issues that concern
    /// them (or that mention them by `@name`).
    pub fn activity_for(&mut self, who: &str) -> String {
        let mut s = String::new();
        for (i, new, raised) in self.news(who) {
            s += &format!("{}{}\n", self.issue_line(i), if raised { " (new)" } else { "" });
            for e in new {
                s += &format!("  {}\n", self.entry_line(e));
            }
        }
        self.cursors.insert(who.to_string(), now());
        if s.is_empty() {
            "(nothing new from the others yet)".into()
        } else {
            s
        }
    }

    pub fn set_waiting(&mut self, who: &str, waiting: bool) {
        let p = self.presence.entry(who.to_string()).or_default();
        p.waiting = waiting;
        p.seen = now();
        self.touch();
    }

    // ----- named things ---------------------------------------------------------------

    fn views(&mut self, who: &str, args: &Value) -> Result<Vec<Value>> {
        let name = args.get("name").and_then(|v| v.as_str());
        match args.get("action").and_then(|v| v.as_str()).unwrap_or("list") {
            "list" => Ok(text(format!("{} named views:\n{}", self.items.views.len(), self.items.views.iter().map(|v| format!("  {:?}: {}\n", v.name, v.state)).collect::<String>()))),
            "save" => {
                let name = name.context("name")?.to_string();
                let state = args.get("state").cloned().context("state: a view state (any of pane, camera, colour, filters, layers)")?;
                self.items.views.retain(|v| v.name != name);
                self.items.views.push(NamedView { name: name.clone(), state });
                self.save_items(who)?;
                Ok(text(format!("saved view {name:?}")))
            }
            "delete" => {
                let name = name.context("name")?;
                self.items.views.retain(|v| v.name != name);
                self.save_items(who)?;
                Ok(text(format!("deleted view {name:?}")))
            }
            a => bail!("action must be list, save or delete, not {a:?}"),
        }
    }

    fn surfaces(&mut self, who: &str, args: &Value) -> Result<Vec<Value>> {
        match args.get("action").and_then(|v| v.as_str()).unwrap_or("list") {
            "list" => {
                let mut s = format!("{} surfaces (model frame):\n", self.items.surfaces.len());
                for x in &self.items.surfaces {
                    s += &format!("  {:?}: {}\n    plane n·p = {:.4} with n = {}, through {}; rms {:.1} mm over {} points; by {}\n", x.name, x.describes, crate::dot(x.normal, x.centre), f3(x.normal), f3(x.centre), x.rms * 1e3, x.count, self.store.name_of(&x.by));
                }
                Ok(text(s))
            }
            "delete" => {
                let name = args.get("name").and_then(|v| v.as_str()).context("name")?;
                self.items.surfaces.retain(|x| x.name != name);
                self.save_items(who)?;
                Ok(text(format!("deleted surface {name:?}")))
            }
            a => bail!("action must be list or delete, not {a:?}"),
        }
    }

    fn sections(&mut self, who: &str, args: &Value) -> Result<Vec<Value>> {
        let name = args.get("name").and_then(|v| v.as_str());
        match args.get("action").and_then(|v| v.as_str()).unwrap_or("list") {
            "list" => Ok(text(format!("{} named sections:\n{}", self.items.sections.len(), self.items.sections.iter().map(|d| format!("  {:?}: {}\n", d.name, d.args)).collect::<String>()))),
            "show" => {
                let name = name.context("name")?;
                let d = self.items.sections.iter().find(|d| d.name == name).with_context(|| format!("no section named {name:?}"))?.clone();
                self.render(who, &d.args)
            }
            "delete" => {
                let name = name.context("name")?;
                self.items.sections.retain(|d| d.name != name);
                self.save_items(who)?;
                Ok(text(format!("deleted section {name:?}")))
            }
            a => bail!("action must be list, show or delete, not {a:?}"),
        }
    }
}
