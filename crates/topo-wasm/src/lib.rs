//! WebAssembly bindings.
//!
//! - [`App`]: the app engine (`topo-app`) for the browser UI, run in a Web
//!   Worker. Model scripts are bundled here and evaluated by the browser's
//!   own JavaScript engine.
//! - [`Project`]: a model built once from its JSON IR, queried for sheets,
//!   DXF, mesh, issues and analysis data.

use topo_core::{validate, Model, Topology};
use topo_draw::DrawingSet;
use topo_geom::{Geometry, Mesh};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

#[wasm_bindgen]
pub struct Project {
    model: Model,
    topo: Topology,
    geom: Geometry,
    set: DrawingSet,
}

#[wasm_bindgen]
impl Project {
    /// Builds topology, geometry and the drawing set from a model JSON string.
    #[wasm_bindgen(constructor)]
    pub fn new(model_json: &str) -> Result<Project, JsError> {
        let model: Model = serde_json::from_str(model_json)?;
        Ok(Project::from_model(model))
    }

    /// One of the built-in examples (`garage`).
    pub fn example(name: &str) -> Result<Project, JsError> {
        match name {
            "garage" => Ok(Project::from_model(topo_timber::examples::garage_studio())),
            _ => Err(JsError::new(&format!("unknown example {name}"))),
        }
    }

    pub fn model_json(&self) -> String {
        serde_json::to_string(&self.model).unwrap()
    }

    pub fn sheet_count(&self) -> usize {
        self.set.sheets.len()
    }

    pub fn sheet_number(&self, i: usize) -> String {
        self.set.sheets[i].number.clone()
    }

    pub fn sheet_svg(&self, i: usize) -> String {
        self.set.sheet_svg(&self.model, i)
    }

    pub fn sheet_dxf(&self, i: usize) -> String {
        self.set.sheet_dxf(&self.model, i)
    }

    /// Validation, resolution and clash issues as JSON.
    pub fn issues_json(&self) -> String {
        let mut v = validate(&self.model, &self.topo);
        v.extend(self.geom.issues.iter().cloned());
        v.extend(self.geom.clash_issues());
        v.extend(self.geom.bearing_issues(&self.model));
        v.extend(topo_analysis::load_path_issues(&self.model, &self.topo, &self.geom));
        serde_json::to_string(&v).unwrap()
    }

    /// Triangle mesh as JSON `{positions, triangles, member}` (metres).
    pub fn mesh_json(&self) -> String {
        serde_json::to_string(&Mesh::from_geometry(&self.geom)).unwrap()
    }

    pub fn analysis_model_json(&self) -> String {
        serde_json::to_string(&topo_analysis::AnalysisModel::extract(&self.model, &self.topo, &self.geom)).unwrap()
    }

    pub fn support_graph_json(&self) -> String {
        serde_json::to_string(&topo_analysis::support_graph(&self.model, &self.topo, &self.geom)).unwrap()
    }
}

impl Project {
    pub fn from_model(model: Model) -> Project {
        let topo = Topology::build(&model);
        let geom = Geometry::build(&model, &topo);
        let set = DrawingSet::build(&model, &topo, &geom);
        Project { model, topo, geom, set }
    }
}

// ----- the app engine -------------------------------------------------------

/// Evaluates a bundled script with the browser's JavaScript engine: the
/// bundle evaluates to `(params, scenario) => json`.
struct JsEval {
    run: js_sys::Function,
    scenario: Option<String>,
}

impl JsEval {
    fn new(script: &topo_script::Script) -> Result<JsEval, String> {
        let f = js_sys::eval(&script.program).map_err(|e| js_error_text(&e))?;
        let run = f.dyn_into::<js_sys::Function>().map_err(|_| "the script bundle is not a function".to_string())?;
        Ok(JsEval { run, scenario: script.scenario.clone() })
    }
}

fn js_error_text(e: &JsValue) -> String {
    if let Some(err) = e.dyn_ref::<js_sys::Error>() {
        return String::from(err.message());
    }
    e.as_string().unwrap_or_else(|| format!("{e:?}"))
}

impl topo_script::Evaluate for JsEval {
    fn eval(&self, params: Option<&std::collections::BTreeMap<String, f64>>) -> Result<String, topo_script::ScriptError> {
        let p = match params {
            Some(p) => js_sys::JSON::parse(&serde_json::to_string(p).unwrap()).map_err(|e| topo_script::ScriptError::Runtime(js_error_text(&e)))?,
            None => JsValue::UNDEFINED,
        };
        let s = self.scenario.as_deref().map(JsValue::from_str).unwrap_or(JsValue::UNDEFINED);
        let out = self.run.call2(&JsValue::NULL, &p, &s).map_err(|e| topo_script::ScriptError::Runtime(js_error_text(&e)))?;
        out.as_string().ok_or_else(|| topo_script::ScriptError::Runtime("the script did not return JSON".into()))
    }
}

/// A model open in the browser app (see `topo_app::Session`). Files are
/// given as a JSON object `{id: text}`; ids are `/`-separated paths.
#[wasm_bindgen]
pub struct App {
    session: topo_app::Session,
}

fn files_from_json(json: &str) -> Result<std::collections::BTreeMap<String, String>, JsError> {
    Ok(serde_json::from_str(json)?)
}

#[wasm_bindgen]
impl App {
    #[wasm_bindgen(constructor)]
    pub fn new(entry: &str, files_json: &str) -> Result<App, JsError> {
        Ok(App { session: topo_app::Session::new(entry, files_from_json(files_json)?) })
    }

    /// Replaces the model's files (e.g. after an edit) without rebuilding.
    pub fn set_files(&mut self, files_json: &str) -> Result<(), JsError> {
        self.session.files = files_from_json(files_json)?;
        Ok(())
    }

    /// Scenario to evaluate; empty for the script's default.
    pub fn set_scenario(&mut self, name: &str) {
        self.session.scenario = Some(name.to_string()).filter(|s| !s.is_empty());
    }

    /// Load cases to leave out of the checks, as a JSON array of names.
    pub fn set_excluded(&mut self, names_json: &str) -> Result<(), JsError> {
        self.session.excluded = serde_json::from_str(names_json)?;
        Ok(())
    }

    /// Bundles and evaluates the script, fits its measurements and builds
    /// everything. Errors end up in `state()`.
    pub fn rebuild(&mut self) {
        match self.session.script().and_then(|s| JsEval::new(&s)) {
            Ok(ev) => self.session.rebuild(&ev),
            Err(e) => {
                self.session.version += 1;
                self.session.built = Err(e);
            }
        }
    }

    /// A plain number in JavaScript.
    pub fn version(&self) -> f64 {
        self.session.version as f64
    }

    pub fn state(&self) -> String {
        self.session.state().to_string()
    }

    /// A read-only request (`sheet`, `overlay`, `heat`, `mesh`, `view3d`,
    /// `ratios`, `member`, `pick`, `preview`, `parse`) with JSON arguments.
    pub fn request(&self, op: &str, args: &str) -> Result<String, JsError> {
        self.session.request(op, args).map_err(|e| JsError::new(&e))
    }

    /// A new measurement: `{id, text, line}` — the file to write, then reload.
    pub fn measure(&self, args: &str) -> Result<String, JsError> {
        let (w, line) = self.session.measure(args).map_err(|e| JsError::new(&e))?;
        Ok(serde_json::json!({ "id": w.id, "text": w.text, "line": line }).to_string())
    }
}

/// The ids a module imports (resolved; `topo-cad` left out), as JSON: for
/// the host to gather a model's files from storage.
#[wasm_bindgen]
pub fn module_imports(source: &str, id: &str) -> Result<String, JsError> {
    let specs = topo_script::bundle::imports(source, id).map_err(|e| JsError::new(&e.to_string()))?;
    let mut ids = vec![];
    for s in specs {
        let r = topo_script::bundle::resolve(id, &s).map_err(|e| JsError::new(&e))?;
        if r != topo_script::bundle::API && !ids.contains(&r) {
            ids.push(r);
        }
    }
    Ok(serde_json::to_string(&ids).unwrap())
}

/// Id of a model's measurements sidecar.
#[wasm_bindgen]
pub fn sidecar_id(model_id: &str) -> String {
    topo_script::measfile::sidecar_id(model_id)
}

/// The text of a new project named `name`, and its file slug.
#[wasm_bindgen]
pub fn new_project(name: &str) -> String {
    serde_json::json!({ "slug": topo_app::slug(name), "text": topo_app::new_project(name) }).to_string()
}

/// Native helpers model scripts call (set as globals by the worker).
#[wasm_bindgen]
pub fn table_json(name: &str) -> String {
    topo_data::table_json(name).unwrap_or_else(|| "null".into())
}

#[wasm_bindgen]
pub fn standard_truss_json(req: &str) -> String {
    topo_script::standard_truss_json(req)
}

/// The `topo-cad` API source (for the editor's reference).
#[wasm_bindgen]
pub fn api_source() -> String {
    topo_script::API_TS.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_roundtrip_renders_identically() {
        let a = Project::from_model(topo_timber::examples::garage_studio());
        let b = Project::new(&a.model_json()).ok().unwrap();
        assert_eq!(a.sheet_count(), b.sheet_count());
        for i in 0..a.sheet_count() {
            assert_eq!(a.sheet_svg(i), b.sheet_svg(i));
        }
        assert_eq!(b.issues_json(), "[]");
    }
}
