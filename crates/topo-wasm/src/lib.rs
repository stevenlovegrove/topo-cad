//! WebAssembly bindings. A `Project` is built once from a JSON model (the IR)
//! and then queried for sheets, DXF, mesh, issues and analysis data.

use topo_core::{validate, Model, Topology};
use topo_draw::DrawingSet;
use topo_geom::{Geometry, Mesh};
use wasm_bindgen::prelude::*;

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
