//! The app engine: everything `topo serve`'s UI shows, computed from a
//! model's files, with no server, filesystem or clock in it — so it runs
//! natively and in the browser (as wasm in a Web Worker).
//!
//! A [`Session`] holds a model's files (by id: `/`-separated paths), builds
//! it with a host-supplied script evaluator, and answers JSON requests
//! ([`Session::request`]). Anything that would write a file (a new field
//! measurement) is returned to the host to store.

use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use topo_analysis::{nds_asd_excluding, takedown, Analysis, Takedown};
use topo_core::units::fmt_ft_in;
use topo_core::{validate, Issue, Model, Topology};
use topo_draw::pick::{overlay, pick, OverlayInput};
use topo_draw::{DrawingSet, View};
use topo_geom::measure::{evaluate, Quantity};
use topo_geom::Geometry;
use topo_script::solve::SolveReport;
use topo_script::{measfile, Evaluate, Script};

/// Every validation, geometry and load-path issue for a built model.
pub fn all_issues(model: &Model, topo: &Topology, geom: &Geometry) -> Vec<Issue> {
    let mut issues = validate(model, topo);
    issues.extend(geom.issues.iter().cloned());
    issues.extend(geom.clash_issues());
    issues.extend(geom.bearing_issues(model));
    issues.extend(topo_analysis::load_path_issues(model, topo, geom));
    let td = topo_analysis::takedown(model, topo, geom);
    issues.extend(td.issues);
    issues
}

/// Cover-sheet tables for a measurement fit.
pub fn fit_tables(fit: &SolveReport) -> Vec<View> {
    use topo_draw::schedule::table;
    use topo_draw::Align::*;
    vec![
        View::paper(
            "Field measurements",
            table("Field measurements", &["Measurement", "Measured", "Model", "Diff", ""], &[2.6, 1.0, 1.0, 0.7, 0.6], &[Start, End, End, End, Middle], &fit.measurement_rows()),
        ),
        View::paper("Fitted unknowns", table("Fitted unknowns", &["Unknown", "Value", "± 1σ"], &[1.4, 1.6, 1.3], &[Start, End, End], &fit.unknown_rows())),
    ]
}

/// A built model and everything derived from it.
pub struct Built {
    pub model: Model,
    pub geom: Geometry,
    pub td: Takedown,
    pub analysis: Analysis,
    pub set: DrawingSet,
    pub fit: Option<SolveReport>,
    pub issues: Vec<String>,
    pub svgs: Vec<String>,
}

impl Built {
    /// Builds everything for a model (and its measurement fit), leaving the
    /// named load cases out of the checks.
    pub fn new(model: Model, fit: Option<SolveReport>, excluded: &[String]) -> Built {
        let topo = Topology::build(&model);
        let geom = Geometry::build(&model, &topo);
        let issues = all_issues(&model, &topo, &geom).iter().map(|i| i.to_string()).collect();
        let set = DrawingSet::build_with(&model, &topo, &geom, fit.as_ref().map(fit_tables).unwrap_or_default());
        let svgs = (0..set.sheets.len()).map(|i| set.sheet_svg(&model, i)).collect();
        let td = takedown(&model, &topo, &geom);
        let ids: Vec<_> = model.load_cases.iter().filter(|c| excluded.contains(&c.name)).map(|c| c.id).collect();
        let analysis = nds_asd_excluding(&model, &geom, &td, &ids);
        Built { model, geom, td, analysis, set, fit, issues, svgs }
    }
}

/// A model open in the app.
pub struct Session {
    /// The model's files (its modules and measurements sidecar) by id.
    pub files: BTreeMap<String, String>,
    /// The model file's id.
    pub entry: String,
    /// Scenario to evaluate (`None`: the script's default).
    pub scenario: Option<String>,
    /// Load cases left out of the checks (by name).
    pub excluded: Vec<String>,
    pub version: u64,
    pub built: Result<Built, String>,
}

/// A file the host should write (e.g. the measurements sidecar).
#[derive(Clone, Debug, PartialEq)]
pub struct FileWrite {
    pub id: String,
    pub text: String,
}

impl Session {
    pub fn new(entry: &str, files: BTreeMap<String, String>) -> Session {
        Session { files, entry: entry.into(), scenario: None, excluded: vec![], version: 0, built: Err("not built".into()) }
    }

    /// The bundled script for the current files.
    pub fn script(&self) -> Result<Script, String> {
        let mut s = Script::from_files(&self.files, &self.entry).map_err(|e| e.to_string())?;
        s.scenario = self.scenario.clone();
        Ok(s)
    }

    /// Rebuilds from the current files, evaluating the script with `eval`
    /// (the host's JavaScript engine running [`Script::program`]).
    pub fn rebuild(&mut self, eval: &dyn Evaluate) {
        self.version += 1;
        self.built = topo_script::solve::solve(eval).map_err(|e| e.to_string()).map(|(model, fit)| Built::new(model, fit, &self.excluded));
    }

    fn sidecar_id(&self) -> String {
        measfile::sidecar_id(&self.entry)
    }

    /// Everything the UI shows about the model (not the per-view data).
    pub fn state(&self) -> Value {
        let model_text = self.files.get(&self.entry).cloned().unwrap_or_default();
        let sidecar = self.sidecar_id();
        let stem = sidecar.rsplit('/').next().unwrap_or(&sidecar).trim_end_matches(".ts").to_string();
        let sidecar_info = json!({
            "path": sidecar,
            "exists": self.files.contains_key(&sidecar),
            "imported": model_text.contains(&format!("./{stem}")),
            "hint": measfile::import_hint_id(&self.entry),
        });
        match &self.built {
            Err(e) => json!({ "version": self.version, "file": self.entry, "error": e, "sidecar": sidecar_info }),
            Ok(b) => json!({
                "version": self.version,
                "file": self.entry,
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
                "loads": loads_summary(b, &self.excluded),
                "scenarios": b.model.scenarios,
                "sidecar": sidecar_info,
            }),
        }
    }

    /// Answers a read-only request: `op` with JSON `args`. Returns JSON, or
    /// for `sheet` the SVG text.
    pub fn request(&self, op: &str, args: &str) -> Result<String, String> {
        let a: Value = if args.trim().is_empty() { json!({}) } else { serde_json::from_str(args).map_err(|e| format!("bad arguments for {op}: {e}"))? };
        let s = |k: &str| a.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let n = |k: &str, d: f64| a.get(k).and_then(Value::as_f64).unwrap_or(d);
        let i = |k: &str| a.get(k).and_then(Value::as_u64).map(|x| x as usize).unwrap_or(usize::MAX);
        if op == "state" {
            return Ok(self.state().to_string());
        }
        if op == "parse" {
            return measfile::parse_length(&s("text")).map(|v| json!({ "value": v, "text": fmt_ft_in(v) }).to_string());
        }
        let b = self.built.as_ref().map_err(|_| "model does not build".to_string())?;
        let v = match op {
            "sheet" => return b.svgs.get(i("sheet")).cloned().ok_or_else(|| "no such sheet".into()),
            "overlay" => {
                let (Some(sheet), Some(fit)) = (b.set.sheets.get(i("sheet")), &b.fit) else { return Ok("[]".into()) };
                let items: Vec<OverlayInput> = fit
                    .measurements
                    .iter()
                    .map(|m| OverlayInput { name: &m.name, quantity: &m.quantity, text: fmt_ft_in(m.measured), ok: m.misfit().map(|f| f.abs() <= 1.0) })
                    .collect();
                serde_json::to_value(overlay(&b.model, &b.geom, sheet, &items)).unwrap()
            }
            "heat" => heat(b, i("sheet"), &s("mode"), &s("combo")),
            "mesh" => mesh(b),
            "view3d" => view3d(b, n("az", 225.0), n("el", 30.0)),
            "ratios" => ratios(b, &s("mode"), &s("combo")),
            "member" => member_detail(b, &s("path"), &s("combo"))?,
            "pick" => {
                let Some(sheet) = b.set.sheets.get(i("sheet")) else { return Ok("null".into()) };
                serde_json::to_value(pick(&b.model, &b.geom, sheet, [n("x", 0.0), n("y", 0.0)], n("radius", 0.0))).unwrap()
            }
            "preview" => {
                let q: Quantity = serde_json::from_value(a.get("quantity").cloned().unwrap_or(Value::Null)).map_err(|e| format!("bad quantity: {e}"))?;
                let v = evaluate(&b.model, &b.geom, &q)?;
                json!({ "value": v, "text": fmt_ft_in(v), "code": measfile::quantity_ts(&b.model, &q) })
            }
            _ => return Err(format!("unknown request {op}")),
        };
        Ok(v.to_string())
    }

    /// A new field measurement: checks it and returns the sidecar's new text
    /// for the host to write (then reload), with the line added.
    pub fn measure(&self, args: &str) -> Result<(FileWrite, String), String> {
        #[derive(Deserialize)]
        struct MeasureReq {
            name: String,
            quantity: Quantity,
            value: String,
            note: Option<String>,
        }
        let p: MeasureReq = serde_json::from_str(args).map_err(|e| format!("bad measurement: {e}"))?;
        let b = self.built.as_ref().map_err(|_| "model does not build".to_string())?;
        let name = p.name.trim();
        if name.is_empty() {
            return Err("give the measurement a name".into());
        }
        if b.fit.as_ref().is_some_and(|f| f.measurements.iter().any(|m| m.name == name)) {
            return Err(format!("a measurement named \"{name}\" already exists"));
        }
        let value = measfile::parse_length(&p.value)?;
        evaluate(&b.model, &b.geom, &p.quantity)?;
        let id = self.sidecar_id();
        let (text, line) = measfile::append_text(self.files.get(&id).map(String::as_str), &b.model, name, &p.quantity, value, p.note.as_deref())?;
        Ok((FileWrite { id, text }, line))
    }
}

/// Load cases (what each contains, whether left out) and the combinations
/// in use with their factors and load duration factor.
fn loads_summary(b: &Built, excluded: &[String]) -> Value {
    use topo_core::{AreaBasis, Load};
    const PSF: f64 = 47.880259;
    const PLF: f64 = 14.593903;
    let m = &b.model;
    let cases: Vec<Value> = m
        .load_cases
        .iter()
        .map(|c| {
            let mut items: Vec<String> = vec![];
            if c.self_weight {
                items.push("self-weight of all framing (from member sizes and species)".into());
            }
            let mut member_loads: BTreeMap<String, (usize, f64)> = Default::default();
            for l in &m.loads {
                match l {
                    Load::Area { case, group, pressure, basis, label, .. } if *case == c.id => {
                        let on = if *basis == AreaBasis::Surface { "along the slope" } else { "on plan" };
                        items.push(format!("{:.1} psf {on} on {}{}", pressure / PSF, m.group(*group).name, label.as_ref().map(|l| format!(" — {l}")).unwrap_or_default()));
                    }
                    Load::MemberUniform { case, member, w, range } if *case == c.id => {
                        let what = if range.is_some() { "part-length line loads" } else { "line loads" };
                        let e = member_loads.entry(format!("{what} on {}", m.member(*member).role)).or_insert((0, 0.0));
                        e.0 += 1;
                        e.1 = e.1.max(-w.z / PLF);
                    }
                    Load::Node { case, force, .. } if *case == c.id => items.push(format!("{:.0} lb point load", -force.z / 4.448_221_615)),
                    _ => {}
                }
            }
            for (k, (n, w)) in member_loads {
                items.push(format!("{k}: {n} members, up to {w:.1} plf"));
            }
            json!({ "name": c.name, "kind": format!("{:?}", c.kind), "excluded": excluded.contains(&c.name), "items": items })
        })
        .collect();
    let combos: Vec<Value> = b
        .analysis
        .combos
        .iter()
        .map(|c| {
            let (cd, why) = topo_analysis::load_duration_factor(m, c);
            json!({
                "name": c.name,
                "factors": c.factors.iter().map(|(id, f)| json!([m.load_cases[id.idx()].name, f])).collect::<Vec<_>>(),
                "cd": cd,
                "cd_why": why,
            })
        })
        .collect();
    json!({ "standard": format!("NDS 2018 ASD design values; {} load combinations", m.standards.combinations_label()), "cases": cases, "combos": combos })
}

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
        "combinations": b.model.standards.combinations_label(),
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
/// world metres rounded to 0.1 mm) with its path and label, and the
/// sheathing and finish build-ups with the panels they lie on.
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
    let v3 = |p: topo_core::Vec3| [r(p.x), r(p.y), r(p.z)];
    let surfaces: Vec<Value> = b
        .model
        .surfaces
        .iter()
        .map(|s| {
            let panels: Vec<Value> = topo_geom::surface_panels(&b.model, &b.geom, s)
                .into_iter()
                .map(|p| json!({ "origin": v3(p.origin), "slope": v3(p.slope), "run": v3(p.run), "normal": v3(p.normal) }))
                .collect();
            json!({ "name": s.name, "region": s.region, "layers": s.layers, "panels": panels, "psf": s.psf(), "thickness": s.thickness() })
        })
        .collect();
    json!({ "version_name": b.model.info.name, "members": members, "surfaces": surfaces })
}

/// Exact hidden-line view of the whole model from a compass bearing `az`
/// (degrees, the viewer's direction from the model) and elevation `el`:
/// visible edge segments in view-plane coordinates (m), as
/// `[member index, x0, y0, x1, y1]`. (The host times it.)
fn view3d(b: &Built, az: f64, el: f64) -> Value {
    let (a, e) = (az.to_radians(), el.clamp(-89.9, 89.9).to_radians());
    let toward = topo_core::Vec3::new(a.sin() * e.cos(), a.cos() * e.cos(), e.sin());
    let proj = topo_draw::hlr::Projector::new(toward, topo_core::Vec3::Z);
    let geoms: Vec<&topo_geom::MemberGeom> = b.geom.members.iter().collect();
    let segs = topo_draw::hlr::hidden_lines(&geoms, &proj);
    let r = |v: f64| (v * 100_000.0).round() / 100_000.0;
    let visible: Vec<[f64; 5]> = segs.iter().filter(|s| !s.hidden).map(|s| [s.member.0 as f64, r(s.a.x), r(s.a.y), r(s.b.x), r(s.b.y)]).collect();
    json!({ "az": az, "el": el, "segments": visible })
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

/// Model files for new projects.
pub const NEW_PROJECT: &str = include_str!("new-project.ts");

/// `My Garage!` → `my-garage`.
pub fn slug(name: &str) -> String {
    let s: String = name.trim().to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-")
}

/// The text of a new project named `name`.
pub fn new_project(name: &str) -> String {
    NEW_PROJECT.replace("NEW PROJECT", name.trim())
}

#[cfg(test)]
mod tests;
