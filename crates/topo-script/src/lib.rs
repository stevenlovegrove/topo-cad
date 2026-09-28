//! Runs topo-cad models written in TypeScript.
//!
//! Types are stripped with `oxc` (no type checking — that is the editor's /
//! `tsc --noEmit`'s job), the module runs in an embedded QuickJS runtime with
//! the `topo-cad` API available as a built-in import, and the script's default
//! export (a `Building`) is serialized to a [`SceneSpec`] and expanded into a
//! [`Model`](topo_core::Model) by the Rust generators.

pub mod bundle;
pub mod measfile;
pub mod solve;
pub mod spec;

pub use spec::{build_scene, BuildingSpec, PlacementSpec, SceneSpec};

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
#[cfg(feature = "quickjs")]
use topo_core::Model;

/// Source of the `topo-cad` module (the TypeScript API).
pub const API_TS: &str = include_str!("../../../ts/topo-cad.ts");

#[derive(Debug)]
pub enum ScriptError {
    Syntax(String),
    Runtime(String),
    Spec(String),
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScriptError::Syntax(e) => write!(f, "syntax error: {e}"),
            ScriptError::Runtime(e) => write!(f, "script error: {e}"),
            ScriptError::Spec(e) => write!(f, "model error: {e}"),
        }
    }
}

impl std::error::Error for ScriptError {}

/// TypeScript → JavaScript by stripping types (ES module output).
pub fn strip_types(source: &str, file: &str) -> Result<String, ScriptError> {
    use oxc_allocator::Allocator;
    use oxc_codegen::Codegen;
    use oxc_parser::Parser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::SourceType;
    use oxc_transformer::{TransformOptions, Transformer};

    let alloc = Allocator::default();
    let ret = Parser::new(&alloc, source, SourceType::ts()).parse();
    if !ret.diagnostics.is_empty() {
        let msgs: Vec<String> = ret.diagnostics.iter().map(|e| located(source, e)).collect();
        return Err(ScriptError::Syntax(format!("{file}{}", msgs.join("; "))));
    }
    let mut program = ret.program;
    let scoping = SemanticBuilder::new().build(&program).semantic.into_scoping();
    let opts = TransformOptions::default();
    let r = Transformer::new(&alloc, Path::new(file), &opts).build_with_scoping(scoping, &mut program);
    if !r.diagnostics.is_empty() {
        let msgs: Vec<String> = r.diagnostics.iter().map(|e| located(source, e)).collect();
        return Err(ScriptError::Syntax(format!("{file}{}", msgs.join("; "))));
    }
    Ok(Codegen::new().build(&program).code)
}

/// `:line:col: message` for a diagnostic (just `: message` without a span).
fn located(source: &str, d: &oxc_diagnostics::OxcDiagnostic) -> String {
    match d.labels.first() {
        Some(l) => {
            let off = (l.offset() as usize).min(source.len());
            let before = &source[..off];
            let line = before.matches('\n').count() + 1;
            let col = before.len() - before.rfind('\n').map(|i| i + 1).unwrap_or(0) + 1;
            format!(":{line}:{col}: {d}")
        }
        None => format!(": {d}"),
    }
}

/// Evaluates a model script: its default export as JSON, for `params`
/// overriding its `unknown(...)` values. Implemented by the QuickJS host here
/// and by the browser host (a Web Worker) in `topo-wasm`.
pub trait Evaluate {
    fn eval(&self, params: Option<&BTreeMap<String, f64>>) -> Result<String, ScriptError>;
}

/// A model script bundled once (see [`bundle`]), evaluated as often as
/// needed (e.g. by the solver, with different values for its `unknown`s).
pub struct Script {
    /// The bundle: evaluates to `(params, scenario) => json`.
    pub program: String,
    pub entry: String,
    /// The scenario to evaluate (see `scenarios(...)` in topo-cad.ts).
    pub scenario: Option<String>,
}

impl Script {
    /// From a model's files by id (e.g. paths) and its entry.
    pub fn from_files(files: &BTreeMap<String, String>, entry: &str) -> Result<Script, ScriptError> {
        Ok(Script { program: bundle::bundle(files, entry)?, entry: entry.into(), scenario: None })
    }

    /// The model `source` (file `file`), reading the local files it imports from disk.
    #[cfg(feature = "quickjs")]
    pub fn compile(source: &str, file: &str) -> Result<Script, ScriptError> {
        let files = bundle::collect(file, &mut |id| if id == file { Some(source.to_string()) } else { std::fs::read_to_string(id).ok() })?;
        Script::from_files(&files, file)
    }
}

/// Evaluates a TypeScript model and returns its default export as JSON.
/// `file` names the module; relative imports resolve against its directory.
#[cfg(feature = "quickjs")]
pub fn run_to_json(source: &str, file: &str) -> Result<String, ScriptError> {
    Script::compile(source, file)?.eval(None)
}

/// Runs bundles in QuickJS, a fresh runtime per evaluation.
#[cfg(feature = "quickjs")]
impl Evaluate for Script {
    fn eval(&self, params: Option<&BTreeMap<String, f64>>) -> Result<String, ScriptError> {
        use rquickjs::{CatchResultExt, Context, Function, Runtime};
        let rt = Runtime::new().map_err(|e| ScriptError::Runtime(e.to_string()))?;
        let ctx = Context::full(&rt).map_err(|e| ScriptError::Runtime(e.to_string()))?;
        ctx.with(|ctx| {
            // Native helpers the API calls (see `declare const __topo` in topo-cad.ts).
            let native = || -> rquickjs::Result<()> {
                let truss = Function::new(ctx.clone(), |req: String| -> String { standard_truss_json(&req) })?;
                ctx.globals().set("__topo_truss_standard", truss)?;
                let data = Function::new(ctx.clone(), |name: String| -> String { topo_data::table_json(&name).unwrap_or_else(|| "null".into()) })?;
                ctx.globals().set("__topo_data", data)?;
                Ok(())
            };
            native().map_err(|e| ScriptError::Runtime(e.to_string()))?;
            let params = params.map(|p| serde_json::to_string(p).unwrap());
            let run = || -> rquickjs::Result<String> {
                let f: Function = ctx.eval(self.program.as_str())?;
                let p = match &params {
                    Some(j) => ctx.json_parse(j.as_str())?,
                    None => rquickjs::Value::new_undefined(ctx.clone()),
                };
                f.call((p, self.scenario.clone()))
            };
            run().catch(&ctx).map_err(|e| ScriptError::Runtime(e.to_string()))
        })
    }
}

/// `{"kind": .., "span": .., "pitch": .., "overhang": .., ...}` → a standard
/// truss shape as JSON, or `{"error": "..."}` (a native helper for scripts).
pub fn standard_truss_json(req: &str) -> String {
    #[derive(serde::Deserialize)]
    struct Req {
        #[serde(flatten)]
        kind: topo_timber::StandardTruss,
        span: f64,
        pitch: f64,
        overhang: f64,
    }
    let result = serde_json::from_str::<Req>(req)
        .map_err(|e| format!("bad truss request: {e}"))
        .and_then(|r| topo_timber::TrussShape::standard(r.kind, r.span, r.pitch, r.overhang));
    match result {
        Ok(shape) => serde_json::to_string(&shape).unwrap(),
        Err(e) => serde_json::json!({ "error": e }).to_string(),
    }
}

/// Evaluates a TypeScript model and builds it (fitting any `unknown`s to its
/// measurements first).
#[cfg(feature = "quickjs")]
pub fn run(source: &str, file: &str) -> Result<Model, ScriptError> {
    Ok(run_solved(source, file)?.0)
}

/// Like [`run`], also returning the fit report when the model has unknowns
/// or measurements.
#[cfg(feature = "quickjs")]
pub fn run_solved(source: &str, file: &str) -> Result<(Model, Option<solve::SolveReport>), ScriptError> {
    run_scenario(source, file, None)
}

/// Like [`run_solved`], for a named scenario (`None`: the script's default).
#[cfg(feature = "quickjs")]
pub fn run_scenario(source: &str, file: &str, scenario: Option<&str>) -> Result<(Model, Option<solve::SolveReport>), ScriptError> {
    let mut script = Script::compile(source, file)?;
    script.scenario = scenario.map(Into::into);
    solve::solve(&script)
}

pub(crate) fn parse_spec(json: &str) -> Result<SceneSpec, ScriptError> {
    SceneSpec::from_json(json).map_err(|e| ScriptError::Spec(format!("{e} in exported scene")))
}

#[cfg(test)]
mod benchmarks;
#[cfg(test)]
mod tests;
