//! Runs topo-cad models written in TypeScript.
//!
//! Types are stripped with `oxc` (no type checking — that is the editor's /
//! `tsc --noEmit`'s job), the module runs in an embedded QuickJS runtime with
//! the `topo-cad` API available as a built-in import, and the script's default
//! export (a `Building`) is serialized to a [`SceneSpec`] and expanded into a
//! [`Model`](topo_core::Model) by the Rust generators.

pub mod measfile;
pub mod solve;
pub mod spec;

pub use spec::{build_scene, BuildingSpec, PlacementSpec, SceneSpec};

use rquickjs::loader::{ImportAttributes, Loader, Resolver};
use rquickjs::{CatchResultExt, Context, Ctx, Module, Runtime, Value};
use std::fmt;
use std::path::{Component, Path, PathBuf};
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
        let msgs: Vec<String> = ret.diagnostics.iter().map(|e| e.to_string()).collect();
        return Err(ScriptError::Syntax(format!("{file}: {}", msgs.join("; "))));
    }
    let mut program = ret.program;
    let scoping = SemanticBuilder::new().build(&program).semantic.into_scoping();
    let opts = TransformOptions::default();
    let r = Transformer::new(&alloc, Path::new(file), &opts).build_with_scoping(scoping, &mut program);
    if !r.diagnostics.is_empty() {
        let msgs: Vec<String> = r.diagnostics.iter().map(|e| e.to_string()).collect();
        return Err(ScriptError::Syntax(format!("{file}: {}", msgs.join("; "))));
    }
    Ok(Codegen::new().build(&program).code)
}

/// Lexically normalizes `a/./b/../c` → `a/c` (no filesystem access).
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir if matches!(out.components().next_back(), Some(Component::Normal(_))) => {
                out.pop();
            }
            c => out.push(c),
        }
    }
    out
}

/// Resolves `topo-cad` and relative `./x` / `../x` imports (`.ts` implied).
struct ModelResolver;

impl Resolver for ModelResolver {
    fn resolve<'js>(&mut self, _ctx: &Ctx<'js>, base: &str, name: &str, _a: Option<ImportAttributes<'js>>) -> rquickjs::Result<String> {
        if name == "topo-cad" {
            return Ok(name.into());
        }
        if !(name.starts_with("./") || name.starts_with("../")) {
            return Err(rquickjs::Error::new_resolving_message(base, name, "only \"topo-cad\" and relative imports (./file) are supported"));
        }
        // `.ts` is implied unless given (`./x.measured` → `./x.measured.ts`).
        let file = if name.ends_with(".ts") { name.to_string() } else { format!("{name}.ts") };
        let p = Path::new(base).parent().unwrap_or(Path::new("")).join(file);
        Ok(normalize(&p).to_string_lossy().into_owned())
    }
}

/// Loads the API module and TypeScript files from disk, stripping types.
struct ModelLoader {
    api_js: String,
}

impl Loader for ModelLoader {
    fn load<'js>(&mut self, ctx: &Ctx<'js>, name: &str, _a: Option<ImportAttributes<'js>>) -> rquickjs::Result<Module<'js>> {
        if name == "topo-cad" {
            return Module::declare(ctx.clone(), name, self.api_js.clone());
        }
        let src = std::fs::read_to_string(name).map_err(|e| rquickjs::Error::new_loading_message(name, e.to_string()))?;
        let js = strip_types(&src, name).map_err(|e| rquickjs::Error::new_loading_message(name, e.to_string()))?;
        Module::declare(ctx.clone(), name, js)
    }
}

/// Evaluates a TypeScript model and returns its default export as JSON.
/// `file` names the module; relative imports resolve against its directory.
pub fn run_to_json(source: &str, file: &str) -> Result<String, ScriptError> {
    Script::compile(source, file)?.eval(None)
}

/// A model script with types stripped once, evaluated as often as needed
/// (e.g. by the solver, with different values for its `unknown`s).
pub struct Script {
    api_js: String,
    js: String,
    file: String,
}

impl Script {
    pub fn compile(source: &str, file: &str) -> Result<Script, ScriptError> {
        Ok(Script { api_js: strip_types(API_TS, "topo-cad.ts")?, js: strip_types(source, file)?, file: file.into() })
    }

    /// Default export as JSON; `params` override `unknown(...)` values.
    pub fn eval(&self, params: Option<&std::collections::BTreeMap<String, f64>>) -> Result<String, ScriptError> {
        eval_js(&self.api_js, &self.js, &self.file, params)
    }
}

fn eval_js(api: &str, js: &str, file: &str, params: Option<&std::collections::BTreeMap<String, f64>>) -> Result<String, ScriptError> {
    let api = api.to_string();
    let rt = Runtime::new().map_err(|e| ScriptError::Runtime(e.to_string()))?;
    rt.set_loader(ModelResolver, ModelLoader { api_js: api });
    let ctx = Context::full(&rt).map_err(|e| ScriptError::Runtime(e.to_string()))?;
    ctx.with(|ctx| {
        // Native helpers the API calls (see `declare const __topo` in topo-cad.ts).
        let native = || -> rquickjs::Result<()> {
            let truss = rquickjs::Function::new(ctx.clone(), |req: String| -> String { standard_truss_json(&req) })?;
            ctx.globals().set("__topo_truss_standard", truss)?;
            if let Some(p) = params {
                ctx.eval::<(), _>(format!("globalThis.__topo_params = {};", serde_json::to_string(p).unwrap()))?;
            }
            Ok(())
        };
        native().map_err(|e| ScriptError::Runtime(e.to_string()))?;
        let run = || -> rquickjs::Result<Option<String>> {
            let (module, promise) = Module::declare(ctx.clone(), file, js)?.eval()?;
            promise.finish::<()>()?;
            let default: Value = module.get("default")?;
            if default.is_undefined() {
                return Ok(None);
            }
            ctx.json_stringify(default)?.map(|s| s.to_string()).transpose()
        };
        match run().catch(&ctx) {
            Ok(Some(json)) => Ok(json),
            Ok(None) => Err(ScriptError::Runtime(format!("{file} has no default export (export default a Building or Site)"))),
            Err(e) => Err(ScriptError::Runtime(e.to_string())),
        }
    })
}

/// `{"kind": .., "span": .., "pitch": .., "overhang": .., ...}` → a standard
/// truss shape as JSON, or `{"error": "..."}`.
fn standard_truss_json(req: &str) -> String {
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
pub fn run(source: &str, file: &str) -> Result<Model, ScriptError> {
    Ok(run_solved(source, file)?.0)
}

/// Like [`run`], also returning the fit report when the model has unknowns
/// or measurements.
pub fn run_solved(source: &str, file: &str) -> Result<(Model, Option<solve::SolveReport>), ScriptError> {
    let script = Script::compile(source, file)?;
    solve::solve(&script)
}

pub(crate) fn parse_spec(json: &str) -> Result<SceneSpec, ScriptError> {
    SceneSpec::from_json(json).map_err(|e| ScriptError::Spec(format!("{e} in exported scene")))
}

#[cfg(test)]
mod tests;
