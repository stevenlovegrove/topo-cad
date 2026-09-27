//! Engineering drawings for topo-cad models.

pub mod annot;
pub mod custom;
pub mod doc;
pub mod dxf;
pub mod hlr;
pub mod pick;
pub mod schedule;
pub mod sheet;
pub mod svg;
pub mod views;

pub use doc::{Align, Drawing, Layer, Prim};
pub use schedule::Marks;
pub use sheet::Sheet;
pub use views::{Ctx, View};

use topo_core::units::INCH;
use topo_core::{v2, validate, BBox2, MemberId, Model, Severity, SheetSpec, Topology};
use topo_geom::Geometry;

/// Wrapped text block in paper units, top-left at the origin.
pub fn text_block(title: &str, paragraphs: &[String], width_in: f64) -> Drawing {
    let mut d = Drawing::default();
    let h = 0.1 * INCH;
    let lead = h * 1.7;
    d.bold(v2(0.0, 0.05 * INCH), title.to_uppercase(), (5.0 / 32.0) * INCH, Align::Start, Layer::Text);
    let mut y = -lead;
    for (i, p) in paragraphs.iter().enumerate() {
        let prefix = format!("{}. ", i + 1);
        let mut line = prefix.clone();
        for word in p.split_whitespace() {
            let cand = if line.ends_with(' ') { format!("{line}{word}") } else { format!("{line} {word}") };
            if doc::text_width(&cand, h) > width_in * INCH && line.len() > prefix.len() {
                d.text(v2(0.0, y), line.clone(), h, Align::Start, Layer::Text);
                y -= lead;
                line = format!("{}{word}", " ".repeat(prefix.len() + 1));
            } else {
                line = cand;
            }
        }
        d.text(v2(0.0, y), line, h, Align::Start, Layer::Text);
        y -= lead * 1.3;
    }
    d
}

/// Engineer-facing wording of a load-path issue, using piece marks.
pub fn load_path_text(model: &Model, marks: &Marks, issue: &topo_core::Issue) -> String {
    let name = |m: MemberId| format!("{} ({})", marks.of(m), model.member(m).role.replace('_', " "));
    let Some(&first) = issue.members.first() else { return issue.message.clone() };
    match issue.code.as_str() {
        "fastener-only-support" => {
            // Group identical marks: "K1 (king stud) x2".
            let mut by: Vec<(String, usize)> = vec![];
            for &m in &issue.members[1..] {
                let n = name(m);
                match by.iter_mut().find(|(b, _)| *b == n) {
                    Some((_, c)) => *c += 1,
                    None => by.push((n, 1)),
                }
            }
            let by: Vec<String> = by.into_iter().map(|(n, c)| if c > 1 { format!("{n} x{c}") } else { n }).collect();
            format!(
                "{} has no bearing support: its gravity load reaches {} only through nails in shear. Review required.",
                name(first),
                by.join(", ")
            )
        }
        "no-gravity-support" => format!("{} has no gravity support path. Review required.", name(first)),
        _ => issue.message.clone(),
    }
}

pub struct DrawingSet {
    pub sheets: Vec<Sheet>,
    pub marks: Marks,
    /// Problems with script-defined sheets (also drawn on the sheet).
    pub errors: Vec<String>,
}

/// Next free sheet number `prefix` + n, n ≥ `base`, in `base`'s hundred.
fn next_number(sheets: &[Sheet], prefix: &str, base: u32) -> u32 {
    let used = sheets
        .iter()
        .filter_map(|s| s.number.strip_prefix(prefix).and_then(|n| n.parse::<u32>().ok()))
        .filter(|n| (base..base + 100).contains(n))
        .max();
    used.map(|n| n + 1).unwrap_or(base)
}

impl DrawingSet {
    pub fn build(model: &Model, topo: &Topology, geom: &Geometry) -> DrawingSet {
        DrawingSet::build_with(model, topo, geom, vec![])
    }

    /// As [`build`](Self::build), with tool-supplied tables (`extras`, e.g.
    /// a measurement fit): shown on the cover sheet and available to
    /// script-defined schedules by title.
    ///
    /// The sheets are the model's `sheets` list in order (tabs in the UI),
    /// or the standard set: cover, framing plans, typical trusses, wall
    /// elevations, analytical model, schedules.
    pub fn build_with(model: &Model, topo: &Topology, geom: &Geometry, extras: Vec<View>) -> DrawingSet {
        let marks = Marks::assign(model, geom);
        let ctx = Ctx { model, topo, geom, marks: &marks };
        let list = model.sheets.clone().unwrap_or_else(topo_core::standard_sheets);
        // Structural results, only if a sheet shows them.
        let wants = list.iter().any(|s| matches!(s, SheetSpec::Sheet { views, .. } if custom::needs_results(views)));
        let td = wants.then(|| topo_analysis::takedown(model, topo, geom));
        let analysis = td.as_ref().map(|t| topo_analysis::nds_asd(model, geom, t));
        let results = td.as_ref().zip(analysis.as_ref()).map(|(takedown, analysis)| custom::Results { takedown, analysis });
        let mut sheets: Vec<Sheet> = vec![];
        let mut errors = vec![];
        let mut cover_at = None;
        for item in &list {
            match item {
                SheetSpec::Standard { which } => match which.as_str() {
                    "cover" => cover_at = Some(sheets.len()),
                    "plans" => {
                        let n = next_number(&sheets, "S-", 101);
                        sheets.extend(sheet::pack("S-", n, "Framing plans", framing_plans(&ctx)));
                    }
                    "trusses" => {
                        let n = next_number(&sheets, "S-", 101);
                        sheets.extend(sheet::pack("S-", n, "Truss elevations", typical_trusses(&ctx)));
                    }
                    "walls" => {
                        let n = next_number(&sheets, "S-", 201);
                        sheets.extend(sheet::pack("S-", n, "Wall framing elevations", wall_elevations(&ctx)));
                    }
                    "analytical" => {
                        let n = next_number(&sheets, "S-", 301);
                        sheets.extend(sheet::pack("S-", n, "Analytical model", vec![analytical_view(&ctx)]));
                    }
                    "schedules" => {
                        let n = next_number(&sheets, "S-", 401);
                        let tables = vec![
                            View::paper("Member schedule", schedule::member_schedule_table(&marks)),
                            View::paper("Connection schedule", schedule::connection_schedule_table(model)),
                        ];
                        sheets.extend(sheet::pack("S-", n, "Schedules", tables));
                    }
                    other => errors.push(format!("unknown standard sheets \"{other}\"")),
                },
                SheetSpec::Sheet { number, title, views } => sheets.extend(custom::sheet(&ctx, number, title, views, &extras, results, &mut errors)),
            }
        }
        if let Some(i) = cover_at {
            let cover = cover_sheets(&ctx, &sheets, extras);
            sheets.splice(i..i, cover);
        }
        let mut seen = std::collections::HashSet::new();
        for s in &sheets {
            if !seen.insert(s.number.clone()) {
                errors.push(format!("sheet number {} is used twice", s.number));
            }
        }
        DrawingSet { sheets, marks, errors }
    }

    pub fn sheet_svg(&self, model: &Model, i: usize) -> String {
        let s = &self.sheets[i];
        svg::to_svg(&s.render(model, i, self.sheets.len()), s.size)
    }

    /// Full-size DXF of one view in model units.
    pub fn view_dxf(model: &Model, v: &View) -> String {
        let ltscale = if v.scale == 1.0 { 1.0 } else { 1.0 / v.scale };
        dxf::to_dxf(&v.drawing, model.units, ltscale)
    }

    /// Whole sheet as a paper-space DXF (1 unit = 1 paper inch / mm).
    pub fn sheet_dxf(&self, model: &Model, i: usize) -> String {
        let s = &self.sheets[i];
        dxf::to_dxf(&s.render(model, i, self.sheets.len()), model.units, 1.0)
    }
}

fn plan_area() -> (f64, f64) {
    let area = Sheet::area(sheet::ARCH_D);
    (area.width(), area.height())
}

/// A framing plan per floor and roof, over the wall cap plates.
fn framing_plans(ctx: &Ctx) -> Vec<View> {
    let model = ctx.model;
    let (aw, ah) = plan_area();
    let top_plates: Vec<MemberId> = model
        .groups
        .iter()
        .filter(|g| g.kind == "wall")
        .flat_map(|g| model.members_in_tree(g.id))
        .filter(|&m| model.member(m).role == "cap_plate")
        .collect();
    let mut plans = vec![];
    for g in model.groups.iter().filter(|g| g.kind == "floor" || g.kind == "roof") {
        let mut ms = model.members_in_tree(g.id);
        ms.extend(&top_plates);
        let ext = views::projected_extent(ctx, &ms, &hlr::Projector::new(topo_core::Vec3::Z, topo_core::Vec3::Y));
        let sc = views::fit_scale(model.units, ext.width(), ext.height(), aw - 3.0 * INCH, ah - 3.0 * INCH);
        plans.push(views::framing_plan(ctx, g.id, &top_plates, sc));
    }
    plans
}

/// A typical (interior) truss per roof.
fn typical_trusses(ctx: &Ctx) -> Vec<View> {
    let model = ctx.model;
    let (aw, ah) = plan_area();
    let mut typicals = vec![];
    for roof in model.groups.iter().filter(|g| g.kind == "roof") {
        let trusses: Vec<_> = model.groups.iter().filter(|g| g.kind == "truss" && g.parent == Some(roof.id)).collect();
        if let Some(t) = trusses.get(1).or(trusses.first()) {
            let ms = model.members_in_tree(t.id);
            let ext = views::projected_extent(ctx, &ms, &hlr::Projector::new(-t.frame.y, topo_core::Vec3::Z));
            let sc = views::fit_scale(model.units, ext.width(), ext.height(), aw * 0.45, ah * 0.3);
            typicals.push(views::group_elevation(ctx, t.id, &format!("Typical truss: {}", roof.name), sc));
        }
    }
    typicals
}

/// Wall elevations at one common scale.
fn wall_elevations(ctx: &Ctx) -> Vec<View> {
    let model = ctx.model;
    let (aw, ah) = plan_area();
    let wall_groups: Vec<_> = model.groups.iter().filter(|g| g.kind == "wall").collect();
    let longest = wall_groups
        .iter()
        .map(|g| {
            let ms = model.members_in_tree(g.id);
            views::projected_extent(ctx, &ms, &hlr::Projector::new(-g.frame.y, topo_core::Vec3::Z))
        })
        .fold((0.0f64, 0.0f64), |(w, h), b| (w.max(b.width()), h.max(b.height())));
    let sc = views::fit_scale(model.units, longest.0, longest.1, aw / 2.0 - 2.5 * INCH, ah / 3.0 - 2.0 * INCH);
    wall_groups.iter().map(|g| views::wall_elevation(ctx, g.id, sc.clone())).collect()
}

fn analytical_view(ctx: &Ctx) -> View {
    let (aw, ah) = plan_area();
    let all: Vec<MemberId> = ctx.model.members.iter().map(|m| m.id).collect();
    let iso = views::iso_projector();
    let ext = views::projected_extent(ctx, &all, &iso);
    let s_an = ((aw - 6.0 * INCH) / ext.width()).min((ah - 3.0 * INCH) / ext.height());
    views::analytical(ctx, &all, &iso, "Analytical model", s_an)
}

/// Cover sheet(s): design basis, notes, load-path review, junctions, the
/// tool's extra tables, the sheet index and a 3D view.
fn cover_sheets(ctx: &Ctx, others: &[Sheet], extras: Vec<View>) -> Vec<Sheet> {
    let (model, topo, geom) = (ctx.model, ctx.topo, ctx.geom);
    let (aw, ah) = plan_area();
    let mut cover_views = vec![];
    let mut basis = model.info.design_basis.clone();
    basis.push(views::units_note(model.units).to_string());
    cover_views.push(View::paper("Design basis", text_block("Design basis", &basis, 7.0)));
    let issues = {
        let mut v = validate(model, topo);
        v.extend(geom.issues.iter().cloned());
        v.extend(geom.clash_issues());
        v.extend(geom.bearing_issues(model));
        v
    };
    let count = |s: Severity| issues.iter().filter(|i| i.severity == s).count();
    let mut notes = model.info.notes.clone();
    notes.push(format!(
        "Model: {} nodes, {} members, {} connections recorded. Validation: {} errors, {} warnings (incl. {} clashes).",
        model.nodes.len(),
        model.members.len(),
        model.joints.iter().map(|j| j.connections.len()).sum::<usize>() + model.bonds.len(),
        count(Severity::Error),
        count(Severity::Warning),
        geom.clashes().len()
    ));
    notes.push("Member lengths are derived from topology and are long-point cut lengths; verify in field.".into());
    cover_views.push(View::paper("General notes", text_block("General notes", &notes, 7.0)));
    let lp = topo_analysis::load_path_issues(model, topo, geom);
    if !lp.is_empty() {
        let items: Vec<String> = lp.iter().map(|i| load_path_text(model, ctx.marks, i)).collect();
        cover_views.push(View::paper("Load path review", text_block("Load path review", &items, 7.0)));
    }
    cover_views.push(View::paper("Junctions", schedule::junction_table(topo)));
    cover_views.extend(extras);
    let mut index: Vec<Vec<String>> = vec![vec!["G-001".into(), "General notes, index, 3D view".into()]];
    index.extend(others.iter().map(|s| vec![s.number.clone(), s.title.clone()]));
    cover_views.push(View::paper(
        "Sheet index",
        schedule::table("Sheet index", &["Sheet", "Title"], &[0.9, 3.2], &[Align::Start, Align::Start], &index),
    ));
    let all: Vec<MemberId> = model.members.iter().map(|m| m.id).collect();
    let ext = views::projected_extent(ctx, &all, &views::iso_projector());
    let s_iso = ((aw * 0.55) / ext.width()).min((ah * 0.55) / ext.height());
    cover_views.push(views::isometric(ctx, &all, "3D framing view", s_iso));
    let mut cover = sheet::pack("G-", 1, "General notes, index, 3D view", cover_views);
    for (i, c) in cover.iter_mut().enumerate() {
        c.number = format!("G-{:03}", i + 1);
    }
    cover
}
