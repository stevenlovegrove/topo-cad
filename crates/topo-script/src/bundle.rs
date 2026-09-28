//! Model scripts as one self-contained JavaScript program.
//!
//! Each module (the `topo-cad` API and the model's own `.ts` files) has its
//! types stripped and its `import` / `export` statements rewritten into a
//! plain function `function (__require, __exports) { … }`; the bundle maps
//! every module's import specifiers to module ids ahead of time. Evaluating
//! the bundle yields a runner, `(params, scenario) => JSON`, that instantiates
//! the modules afresh and returns the entry's default export as JSON.
//!
//! This runs synchronously in any JavaScript engine — QuickJS on the command
//! line, the browser's own in a Web Worker — so both behave the same and
//! the measurement solver can re-evaluate a model many times.

use crate::{strip_types, ScriptError};
use oxc_allocator::Allocator;
use oxc_ast::ast::{
    BindingPattern, Declaration, ExportDefaultDeclarationKind, ImportDeclarationSpecifier, ModuleExportName, Statement,
};
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType};
use std::collections::BTreeMap;

/// The module id of the API.
pub const API: &str = "topo-cad";

/// Resolves an import specifier in module `base`: `topo-cad`, or a relative
/// `./x` / `../x` path (`.ts` implied) against `base`'s folder. Paths use `/`.
pub fn resolve(base: &str, spec: &str) -> Result<String, String> {
    if spec == API {
        return Ok(API.into());
    }
    if !(spec.starts_with("./") || spec.starts_with("../")) {
        return Err(format!("{base}: cannot import \"{spec}\": only \"topo-cad\" and relative imports (./file) are supported"));
    }
    let file = if spec.ends_with(".ts") { spec.to_string() } else { format!("{spec}.ts") };
    let dir = base.rfind('/').map(|i| &base[..i]).unwrap_or("");
    let joined = if dir.is_empty() { file } else { format!("{dir}/{file}") };
    Ok(normalize(&joined))
}

/// `a/./b/../c` → `a/c` (keeps a leading `/` and leading `..`s).
pub fn normalize(p: &str) -> String {
    let abs = p.starts_with('/');
    let mut out: Vec<&str> = vec![];
    for part in p.split('/') {
        match part {
            "" | "." => {}
            ".." if out.last().is_some_and(|l| *l != "..") => {
                out.pop();
            }
            _ => out.push(part),
        }
    }
    format!("{}{}", if abs { "/" } else { "" }, out.join("/"))
}

/// A module rewritten as a function body, with the specifiers it imports.
pub struct ModuleCode {
    pub body: String,
    pub imports: Vec<String>,
}

fn names(p: &BindingPattern) -> Vec<String> {
    p.get_binding_identifiers().iter().map(|b| b.name.to_string()).collect()
}

fn export_name(n: &ModuleExportName) -> String {
    n.name().to_string()
}

fn js_str(s: &str) -> String {
    serde_json::to_string(s).unwrap()
}

/// Strips a module's types and rewrites its imports and exports.
pub fn module_code(source: &str, file: &str) -> Result<ModuleCode, ScriptError> {
    let js = strip_types(source, file)?;
    let alloc = Allocator::default();
    let ret = Parser::new(&alloc, &js, SourceType::mjs()).parse();
    if !ret.diagnostics.is_empty() {
        return Err(ScriptError::Syntax(format!("{file}: {}", ret.diagnostics.iter().map(|d| d.to_string()).collect::<Vec<_>>().join("; "))));
    }
    let mut edits: Vec<(u32, u32, String)> = vec![];
    let mut prologue = String::new();
    let mut imports: Vec<String> = vec![];
    let getter = |name: &str, expr: &str| format!("Object.defineProperty(__exports, {}, {{ enumerable: true, get: () => {expr} }});\n", js_str(name));
    let mut n = 0;
    let mut tmp = || {
        n += 1;
        format!("__m{n}")
    };
    for stmt in &ret.program.body {
        match stmt {
            Statement::ImportDeclaration(d) => {
                let src = d.source.value.to_string();
                imports.push(src.clone());
                let m = tmp();
                let mut code = format!("const {m} = __require({});", js_str(&src));
                for s in d.specifiers.iter().flatten() {
                    match s {
                        ImportDeclarationSpecifier::ImportSpecifier(s) => {
                            code.push_str(&format!(" const {} = {m}[{}];", s.local.name, js_str(&export_name(&s.imported))));
                        }
                        ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => code.push_str(&format!(" const {} = {m}.default;", s.local.name)),
                        ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) => code.push_str(&format!(" const {} = {m};", s.local.name)),
                    }
                }
                edits.push((d.span.start, d.span.end, code));
            }
            Statement::ExportDeclaration(d) => {
                // `export const/function/class …`: keep the declaration.
                let declared: Vec<String> = match &d.declaration {
                    Declaration::VariableDeclaration(v) => v.declarations.iter().flat_map(|x| names(&x.id)).collect(),
                    Declaration::FunctionDeclaration(f) => f.id.iter().map(|i| i.name.to_string()).collect(),
                    Declaration::ClassDeclaration(c) => c.id.iter().map(|i| i.name.to_string()).collect(),
                    _ => vec![],
                };
                for name in declared {
                    prologue.push_str(&getter(&name, &name));
                }
                edits.push((d.span.start, d.declaration.span().start, String::new()));
            }
            Statement::ExportFromDeclaration(d) => {
                // `export { a, b as c } from "x"`.
                let src = d.source.value.to_string();
                imports.push(src.clone());
                let m = tmp();
                prologue.push_str(&format!("const {m} = __require({});\n", js_str(&src)));
                for sp in &d.specifiers {
                    prologue.push_str(&getter(&export_name(&sp.exported), &format!("{m}[{}]", js_str(&export_name(&sp.local)))));
                }
                edits.push((d.span.start, d.span.end, String::new()));
            }
            Statement::ExportNamedDeclaration(d) => {
                // `export { a, b as c }`.
                for sp in &d.specifiers {
                    prologue.push_str(&getter(&export_name(&sp.exported), &export_name(&sp.local)));
                }
                edits.push((d.span.start, d.span.end, String::new()));
            }
            Statement::ExportDefaultDeclaration(d) => {
                let named = match &d.declaration {
                    ExportDefaultDeclarationKind::FunctionDeclaration(f) => f.id.as_ref().map(|i| i.name.to_string()),
                    ExportDefaultDeclarationKind::ClassDeclaration(c) => c.id.as_ref().map(|i| i.name.to_string()),
                    _ => None,
                };
                match named {
                    // `export default function f() {}`: keep `f` as a binding.
                    Some(name) => {
                        prologue.push_str(&getter("default", &name));
                        edits.push((d.span.start, d.declaration.span().start, String::new()));
                    }
                    None => edits.push((d.span.start, d.declaration.span().start, "__exports.default = ".into())),
                }
            }
            Statement::ExportAllDeclaration(d) => {
                let src = d.source.value.to_string();
                imports.push(src.clone());
                let m = tmp();
                let code = match &d.exported {
                    Some(ns) => format!("const {m} = __require({}); Object.defineProperty(__exports, {}, {{ enumerable: true, get: () => {m} }});", js_str(&src), js_str(&export_name(ns))),
                    None => format!(
                        "const {m} = __require({}); for (const k of Object.keys({m})) if (k !== \"default\" && !(k in __exports)) Object.defineProperty(__exports, k, {{ enumerable: true, get: () => {m}[k] }});",
                        js_str(&src)
                    ),
                };
                edits.push((d.span.start, d.span.end, code));
            }
            _ => {}
        }
    }
    let mut body = js.clone();
    edits.sort_by_key(|e| std::cmp::Reverse(e.0));
    for (a, b, text) in edits {
        body.replace_range(a as usize..b as usize, &text);
    }
    Ok(ModuleCode { body: format!("{prologue}{body}"), imports })
}

/// Specifiers a TypeScript module imports (types stripped, so type-only
/// imports are not included).
pub fn imports(source: &str, file: &str) -> Result<Vec<String>, ScriptError> {
    Ok(module_code(source, file)?.imports)
}

/// The files a model needs, given its entry: the entry and every local
/// module it imports (transitively), read with `read` (`None`: missing).
pub fn collect(entry: &str, read: &mut dyn FnMut(&str) -> Option<String>) -> Result<BTreeMap<String, String>, ScriptError> {
    let mut files = BTreeMap::new();
    let mut todo = vec![entry.to_string()];
    while let Some(id) = todo.pop() {
        if files.contains_key(&id) {
            continue;
        }
        let text = read(&id).ok_or_else(|| ScriptError::Runtime(format!("cannot read {id}")))?;
        for spec in imports(&text, &id)? {
            let dep = resolve(&id, &spec).map_err(ScriptError::Runtime)?;
            if dep != API {
                todo.push(dep);
            }
        }
        files.insert(id, text);
    }
    Ok(files)
}

/// One JavaScript program for a model: evaluating it gives
/// `(params, scenario) => json` (the entry's default export as JSON).
/// `files` are the model's modules by id; the API is added.
pub fn bundle(files: &BTreeMap<String, String>, entry: &str) -> Result<String, ScriptError> {
    let mut defs = String::new();
    let mut maps = String::new();
    let mut add = |id: &str, source: &str| -> Result<(), ScriptError> {
        let m = module_code(source, id)?;
        let mut map = vec![];
        for spec in &m.imports {
            let target = resolve(id, spec).map_err(ScriptError::Runtime)?;
            if target != API && !files.contains_key(&target) {
                return Err(ScriptError::Runtime(format!("{id}: cannot import \"{spec}\" ({target} not found)")));
            }
            map.push(format!("{}: {}", js_str(spec), js_str(&target)));
        }
        defs.push_str(&format!("{}: function (__require, __exports) {{\n{}\n}},\n", js_str(id), m.body));
        maps.push_str(&format!("{}: {{ {} }},\n", js_str(id), map.join(", ")));
        Ok(())
    };
    add(API, crate::API_TS)?;
    for (id, text) in files {
        add(id, text)?;
    }
    if !files.contains_key(entry) {
        return Err(ScriptError::Runtime(format!("{entry} not found")));
    }
    Ok(format!(
        r#"(() => {{
const __defs = {{
{defs}}};
const __imports = {{
{maps}}};
return function (params, scenario) {{
  globalThis.__topo_params = params == null ? undefined : params;
  globalThis.__topo_scenario = scenario == null ? undefined : scenario;
  const cache = {{}};
  const load = (id) => {{
    if (id in cache) return cache[id];
    const exports = {{}};
    cache[id] = exports;
    __defs[id]((spec) => load(__imports[id][spec]), exports);
    return exports;
  }};
  const m = load({entry});
  if (m.default === undefined) throw new Error({no_default});
  return JSON.stringify(m.default);
}};
}})()"#,
        entry = js_str(entry),
        no_default = js_str(&format!("{entry} has no default export (export default a Building or Site)")),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_imports() {
        assert_eq!(resolve("projects/garage.ts", "./home-site").unwrap(), "projects/home-site.ts");
        assert_eq!(resolve("projects/garage.ts", "../examples/x.ts").unwrap(), "examples/x.ts");
        assert_eq!(resolve("/a/b/c.ts", "../d").unwrap(), "/a/d.ts");
        assert_eq!(resolve("c.ts", "./d.measured").unwrap(), "d.measured.ts");
        assert_eq!(resolve("x.ts", "topo-cad").unwrap(), "topo-cad");
        assert!(resolve("x.ts", "lodash").is_err());
    }

    #[test]
    fn rewrites_imports_and_exports() {
        let src = r#"
            import { a, b as c, type T } from "./dep";
            import def, * as ns from "./other";
            export const x: number = a + c + def + ns.k;
            export function f() { return x; }
            export class K {}
            const y = 2;
            export { y, y as z };
            export { q } from "./dep";
            export * from "./star";
            export default { x, y };
        "#;
        let m = module_code(src, "m.ts").unwrap();
        assert_eq!(m.imports, vec!["./dep", "./other", "./dep", "./star"]);
        for want in [
            "__require(\"./dep\")",
            "const a = __m1[\"a\"];",
            "const c = __m1[\"b\"];",
            "const def = __m2.default;",
            "const ns = __m2;",
            "get: () => x",
            "get: () => f",
            "get: () => K",
            "\"z\", { enumerable: true, get: () => y",
            "__exports.default = ",
        ] {
            assert!(m.body.contains(want), "missing {want} in:\n{}", m.body);
        }
        assert!(!m.body.contains("export ") && !m.body.contains("import "), "{}", m.body);
    }
}
