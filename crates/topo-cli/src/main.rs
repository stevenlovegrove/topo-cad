//! `topo example <name> <out-dir>` — build an example model and render it.
//! `topo run <model.ts> <out-dir>` — run a TypeScript model and render it.
//! `topo serve [dir | model.ts]… [port] [--web <dir>]` — the web app, with those folders' files as a writable project store.
//! `topo site <out-dir> <dir>… [--web <dir>]` — the web app and those folders' files, for any static web server.
//! `topo render <model.json> <out-dir>` — render a model from its JSON IR.

use std::fs;
use std::path::Path;
use topo_core::{Model, Topology};
use topo_draw::DrawingSet;
use topo_script::solve::SolveReport;
use topo_geom::Geometry;

fn usage() -> ! {
    eprintln!("usage:\n  topo example <garage|garage-as-built> <out-dir>\n  topo run <model.ts> <out-dir>\n  topo serve [dir | model.ts]... [port] [--web <dir>]\n  topo site <out-dir> <dir>... [--web <dir>]\n  topo render <model.json> <out-dir>");
    std::process::exit(2);
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let web = match args.iter().position(|a| a == "--web") {
        Some(i) if i + 1 < args.len() => {
            let w = std::path::PathBuf::from(args.remove(i + 1));
            args.remove(i);
            w
        }
        _ => serve::default_web_dir(),
    };
    let (model, out, fit) = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["example", "garage", out] => (topo_timber::examples::garage_studio(), out.to_string(), None),
        ["example", "garage-as-built", out] => (topo_timber::examples::garage_as_built(), out.to_string(), None),
        ["example", "l-shaped", out] => (topo_timber::examples::l_shaped_perimeter(), out.to_string(), None),
        ["run", path, out] => {
            let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {path}: {e}"));
            match topo_script::run_solved(&text, path) {
                Ok((model, fit)) => (model, out.to_string(), fit),
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        }
        ["serve", rest @ ..] => {
            let port = rest.iter().find_map(|a| a.parse::<u16>().ok()).unwrap_or(8765);
            let paths: Vec<std::path::PathBuf> = rest.iter().filter(|a| a.parse::<u16>().is_err()).map(std::path::PathBuf::from).collect();
            if let Err(e) = serve::serve(&paths, port, &web) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
            return;
        }
        ["site", out, rest @ ..] if !rest.is_empty() => {
            let paths: Vec<std::path::PathBuf> = rest.iter().map(std::path::PathBuf::from).collect();
            if let Err(e) = serve::site(Path::new(out), &paths, &web) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
            return;
        }
        ["render", path, out] => {
            let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {path}: {e}"));
            let model: Model = serde_json::from_str(&text).unwrap_or_else(|e| panic!("parsing {path}: {e}"));
            (model, out.to_string(), None)
        }
        _ => usage(),
    };
    if let Err(e) = render(&model, Path::new(&out), fit.as_ref()) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

mod serve;

use topo_app::{all_issues, fit_tables};

fn render(model: &Model, out: &Path, fit: Option<&SolveReport>) -> std::io::Result<()> {
    fs::create_dir_all(out.join("dxf"))?;
    let topo = Topology::build(model);
    let geom = Geometry::build(model, &topo);
    let issues = all_issues(model, &topo, &geom);

    fs::write(out.join("model.json"), serde_json::to_string_pretty(model).unwrap())?;
    fs::write(out.join("model.obj"), topo_geom::to_obj(model, &geom, model.units.drawing_unit()))?;

    let set = DrawingSet::build_with(model, &topo, &geom, fit.map(fit_tables).unwrap_or_default());
    if let Some(f) = fit {
        fs::write(out.join("fit.json"), serde_json::to_string_pretty(f).unwrap())?;
    }
    let mut links = String::new();
    for (i, s) in set.sheets.iter().enumerate() {
        let name = format!("{}.svg", s.number);
        fs::write(out.join(&name), set.sheet_svg(model, i))?;
        fs::write(out.join("dxf").join(format!("{}.dxf", s.number)), set.sheet_dxf(model, i))?;
        for (k, pv) in s.views.iter().enumerate().filter(|(_, pv)| pv.view.scale != 1.0) {
            let file = format!("{}-{}.dxf", s.number, k + 1);
            fs::write(out.join("dxf").join(file), DrawingSet::view_dxf(model, &pv.view))?;
        }
        links.push_str(&format!(
            "<section><h2>{} — {}</h2><img src=\"{name}\" alt=\"{}\"></section>\n",
            s.number, s.title, s.title
        ));
    }
    let analysis = topo_analysis::AnalysisModel::extract(model, &topo, &geom);
    fs::write(out.join("analysis-model.json"), serde_json::to_string_pretty(&analysis).unwrap())?;

    fs::write(
        out.join("index.html"),
        format!(
            "<!doctype html><meta charset=utf-8><title>{}</title><style>body{{font-family:sans-serif;background:#eee;margin:0;padding:16px}}img{{width:100%;background:#fff;box-shadow:0 1px 4px #0003}}section{{margin-bottom:24px}}</style><h1>{}</h1>\n{links}",
            model.info.name, model.info.name
        ),
    )?;

    let mut report = String::new();
    report.push_str(&format!("{}: {} nodes, {} members\n", model.info.name, model.nodes.len(), model.members.len()));
    for (k, n) in topo.census() {
        report.push_str(&format!("  {:<22} {n}\n", k.label()));
    }
    report.push_str(&format!(
        "analysis model: {} nodes, {} elements, {} supports\n",
        analysis.nodes.len(),
        analysis.elements.len(),
        analysis.supports.len()
    ));
    if let Some(f) = fit {
        report.push_str(&f.text());
    }
    for e in &set.errors {
        report.push_str(&format!("sheet problem: {e}\n"));
    }
    report.push_str(&format!("issues: {}\n", issues.len()));
    for i in &issues {
        report.push_str(&format!("  {i}\n"));
    }
    fs::write(out.join("report.txt"), &report)?;
    print!("{report}");
    println!("wrote {} sheets to {}", set.sheets.len(), out.display());
    Ok(())
}
