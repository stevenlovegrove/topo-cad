//! Views and sheets from the drawing specs a script writes (`sheet(...)`,
//! `plan(...)`, `elevation(...)`, `detail(...)`, …).

use crate::schedule;
use crate::sheet::{self, Sheet};
use crate::views::{self, Ctx, Projected, View};
use crate::{hlr::Projector, text_block};
use topo_core::units::INCH;
use topo_core::{resolve_direction_in, DirRef, Frame, GroupId, MemberId, Model, Vec2, Vec3, ViewKind, ViewSpec};

/// Structural results, for views and schedules that show them.
#[derive(Clone, Copy)]
pub struct Results<'a> {
    pub takedown: &'a topo_analysis::Takedown,
    pub analysis: &'a topo_analysis::Analysis,
}

/// Whether any view in a sheet list needs structural results.
pub fn needs_results(specs: &[ViewSpec]) -> bool {
    specs.iter().any(|v| v.kind == ViewKind::Utilization || (v.kind == ViewKind::Schedule && matches!(v.table.as_deref(), Some("checks") | Some("reactions"))))
}

/// Governing check per member, worst first (up to `limit`).
fn checks_table(model: &Model, marks: &crate::Marks, r: Results, limit: usize) -> crate::Drawing {
    let a = r.analysis;
    let mut gov: Vec<usize> = a.governing.iter().flatten().copied().collect();
    gov.sort_by(|x, y| a.checks[*y].ratio().total_cmp(&a.checks[*x].ratio()));
    let more = gov.len().saturating_sub(limit);
    let rows: Vec<Vec<String>> = gov
        .iter()
        .take(limit)
        .map(|&i| {
            let c = &a.checks[i];
            let p = model.member_path(c.member);
            let short = p.split('/').rev().take(2).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("/");
            let f = |v: f64| if v.abs() >= 100.0 { format!("{v:.0}") } else { format!("{v:.2}") };
            vec![
                marks.of(c.member).to_string(),
                short,
                format!("{}{}", c.title, if c.indicative { " (indicative)" } else { "" }),
                c.combination.clone(),
                format!("{} / {} {}", f(c.demand), f(c.capacity), c.unit),
                format!("{:.2}{}", c.ratio(), if c.ratio() > 1.0 && !c.indicative { " NG" } else { "" }),
                format!("{}{}", c.clause, if c.verified { "" } else { " *" }),
            ]
        })
        .collect();
    let mut d = crate::schedule::table(
        "Member checks (governing per member, NDS ASD)",
        &["Mark", "Member", "Check", "Comb.", "Demand / capacity", "Ratio", "Ref."],
        &[0.6, 2.4, 2.6, 1.3, 1.9, 0.7, 1.0],
        &[crate::Align::Start, crate::Align::Start, crate::Align::Start, crate::Align::Start, crate::Align::End, crate::Align::End, crate::Align::Start],
        &rows,
    );
    let bb = d.bbox();
    let mut notes = vec!["NG: ratio above 1.0. * reference value not yet verified against its source.".to_string()];
    if more > 0 {
        notes.push(format!("{more} further members have lower ratios."));
    }
    for (k, n) in notes.iter().enumerate() {
        d.text(Vec2::new(bb.min.x, bb.min.y - 0.18 * INCH * (k as f64 + 1.0)), n.clone(), 0.08 * INCH, crate::Align::Start, crate::Layer::Text);
    }
    d
}

/// Foundation reactions by load case (lb, downward on the foundation).
fn reactions_table(model: &Model, r: Results) -> crate::Drawing {
    let td = r.takedown;
    let cases = &model.load_cases;
    // Group by node across cases (reaction lists are aligned).
    let mut rows: Vec<(String, Vec<f64>)> = vec![];
    let mut index: std::collections::HashMap<(u32, u32), usize> = Default::default();
    for (ci, list) in td.reactions.iter().enumerate() {
        for x in list.iter().filter(|x| x.by.is_none()) {
            let key = ((x.at.x * 1000.0).round() as i64 as u32, (x.at.y * 1000.0).round() as i64 as u32);
            let k = *index.entry(key).or_insert_with(|| {
                let p = model.member_path(x.member);
                let short = p.split('/').rev().take(2).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("/");
                rows.push((format!("{short} at ({:.1}', {:.1}')", x.at.x / 0.3048, x.at.y / 0.3048), vec![0.0; cases.len()]));
                rows.len() - 1
            });
            rows[k].1[ci] += x.force.z / 4.448_221_615_260_5;
        }
    }
    let mut headers: Vec<String> = vec!["Support".into()];
    headers.extend(cases.iter().map(|c| format!("{} (lb)", c.name)));
    let hs: Vec<&str> = headers.iter().map(String::as_str).collect();
    let mut widths = vec![3.4];
    widths.extend(cases.iter().map(|_| 0.9));
    let mut aligns = vec![crate::Align::Start];
    aligns.extend(cases.iter().map(|_| crate::Align::End));
    let mut body: Vec<Vec<String>> = rows.iter().map(|(n, v)| std::iter::once(n.clone()).chain(v.iter().map(|x| format!("{x:.0}"))).collect()).collect();
    let totals: Vec<f64> = (0..cases.len()).map(|c| rows.iter().map(|r| r.1[c]).sum()).collect();
    body.push(std::iter::once("Total".to_string()).chain(totals.iter().map(|x| format!("{x:.0}"))).collect());
    crate::schedule::table("Foundation reactions (unfactored, by load case)", &hs, &widths, &aligns, &body)
}

/// Members and groups named by `of` selectors (groups by path tail, else
/// members); empty selects the whole model.
pub fn select(model: &Model, sels: &[String]) -> Result<(Vec<MemberId>, Vec<GroupId>), String> {
    if sels.is_empty() {
        return Ok((model.members.iter().map(|m| m.id).collect(), vec![]));
    }
    let (mut ms, mut gs) = (vec![], vec![]);
    for sel in sels {
        match model.find_group(sel) {
            Ok(g) => {
                gs.push(g);
                ms.extend(model.members_in_tree(g));
            }
            Err(ge) if ge.contains("ambiguous") => return Err(ge),
            Err(_) => match model.find_member(sel) {
                Ok(m) => ms.push(m),
                Err(me) if me.contains("ambiguous") => return Err(me),
                Err(_) => return Err(format!("\"{sel}\" names no group or member")),
            },
        }
    }
    let mut seen = std::collections::HashSet::new();
    ms.retain(|m| seen.insert(*m));
    Ok((ms, gs))
}

/// The building frame a group sits in (its root ancestor's), else the world.
fn building_frame(model: &Model, g: Option<GroupId>) -> Frame {
    let mut cur = g;
    let mut frame = Frame::WORLD;
    while let Some(id) = cur {
        frame = model.group(id).frame;
        cur = model.group(id).parent;
    }
    frame
}

/// Resolves a view direction; names without an explicit frame are looked up
/// in the drawn group (if one), its ancestors, then the world.
fn direction(model: &Model, d: &DirRef, group: Option<GroupId>) -> Result<Vec3, String> {
    resolve_direction_in(model, d, group)
}

fn projector(toward: Vec3, up: Vec3) -> Projector {
    let up = if up.normalized().dot(toward.normalized()).abs() > 0.99 {
        if toward.dot(Vec3::Y).abs() < 0.9 { Vec3::Y } else { Vec3::X }
    } else {
        up
    };
    Projector::new(toward, up)
}

fn default_title(model: &Model, spec: &ViewSpec) -> String {
    let names = if spec.of.is_empty() { model.info.name.clone() } else { spec.of.join(", ") };
    match spec.kind {
        ViewKind::Plan => format!("{names} plan"),
        ViewKind::Elevation => format!("{names} elevation"),
        ViewKind::Iso => names,
        ViewKind::Detail => format!("Detail: {names}"),
        ViewKind::Analytical => "Analytical model".into(),
        ViewKind::Utilization => format!("Member utilization: {names}"),
        ViewKind::Schedule => spec.table.clone().unwrap_or_default(),
        ViewKind::Notes => "Notes".into(),
    }
}

/// Builds one view. `avail` is the paper space (m) to fit it in when no
/// scale is given; `extras` are tool-supplied tables schedules may name.
pub fn view(ctx: &Ctx, spec: &ViewSpec, avail: (f64, f64), extras: &[View], results: Option<Results>) -> Result<View, String> {
    let model = ctx.model;
    let title = spec.title.clone().unwrap_or_else(|| default_title(model, spec));
    match spec.kind {
        ViewKind::Schedule => {
            let name = spec.table.as_deref().ok_or("a schedule needs a table name")?;
            let t = match name {
                "members" => View::paper("Member schedule", schedule::member_schedule_table(ctx.marks)),
                "connections" => View::paper("Connection schedule", schedule::connection_schedule_table(model)),
                "junctions" => View::paper("Junctions", schedule::junction_table(ctx.topo)),
                "checks" => View::paper("Member checks", checks_table(model, ctx.marks, results.ok_or("no structural results")?, 40)),
                "reactions" => View::paper("Foundation reactions", reactions_table(model, results.ok_or("no structural results")?)),
                other => extras.iter().find(|v| v.title.eq_ignore_ascii_case(other)).cloned().ok_or_else(|| {
                    let mut known = vec!["members".to_string(), "connections".into(), "junctions".into(), "checks".into(), "reactions".into()];
                    known.extend(extras.iter().map(|v| v.title.clone()));
                    format!("unknown schedule \"{other}\" (one of {})", known.join(", "))
                })?,
            };
            return Ok(t);
        }
        ViewKind::Notes => return Ok(View::paper(&title, text_block(&title, &spec.lines, 7.0))),
        _ => {}
    }
    let (members, groups) = select(model, &spec.of)?;
    if members.is_empty() {
        return Err(format!("{title}: nothing to draw"));
    }
    let (dashed, _) = if spec.dashed.is_empty() { (vec![], vec![]) } else { select(model, &spec.dashed)? };
    let group = (groups.len() == 1).then(|| groups[0]);
    let home = building_frame(model, groups.first().copied().or_else(|| model.member(members[0]).group));
    let dir = |d: &Option<DirRef>| d.as_ref().map(|d| direction(model, d, group)).transpose();
    let (from, up) = (dir(&spec.from)?, dir(&spec.up)?);
    // Default viewing side: the first drawn group's −y (a wall's outside, a truss's face).
    let facing_group = || groups.first().map(|&g| -model.group(g).frame.y).unwrap_or(-home.y);
    let proj = match spec.kind {
        ViewKind::Plan => projector(from.unwrap_or(Vec3::Z), up.unwrap_or(home.y)),
        ViewKind::Elevation | ViewKind::Detail => projector(from.unwrap_or_else(facing_group), up.unwrap_or(Vec3::Z)),
        // Roofs and floors in plan, walls and trusses in elevation.
        ViewKind::Utilization => {
            let flat = group.is_some_and(|g| matches!(model.group(g).kind.as_str(), "roof" | "floor"));
            if flat {
                projector(from.unwrap_or(Vec3::Z), up.unwrap_or(home.y))
            } else {
                projector(from.unwrap_or_else(facing_group), up.unwrap_or(Vec3::Z))
            }
        }
        ViewKind::Iso | ViewKind::Analytical => {
            projector(from.unwrap_or_else(|| home.dir_to_world(views::iso_projector().toward)), up.unwrap_or(Vec3::Z))
        }
        ViewKind::Schedule | ViewKind::Notes => unreachable!(),
    };
    let clip = match spec.kind {
        ViewKind::Detail => {
            let at = spec.at.as_ref().ok_or("a detail needs `at` (a feature, e.g. meet(...))")?;
            let p = topo_geom::measure::feature(model, ctx.geom, at).map_err(|e| format!("detail centre: {e}"))?;
            Some((proj.p(p.point), spec.radius.unwrap_or(12.0 * INCH)))
        }
        _ => None,
    };
    // Detail: only members that reach into the circle.
    let (members, dashed) = match clip {
        Some((c, r)) => {
            let near = |m: &MemberId| {
                let hull: Vec<Vec2> = ctx.geom.member(*m).convex().verts.iter().map(|v| proj.p(*v)).collect();
                let b = topo_core::BBox2::from_points(hull.iter().copied());
                c.x + r >= b.min.x && c.x - r <= b.max.x && c.y + r >= b.min.y && c.y - r <= b.max.y
            };
            (members.into_iter().filter(near).collect::<Vec<_>>(), dashed.into_iter().filter(near).collect::<Vec<_>>())
        }
        None => (members, dashed),
    };
    let scale = match &spec.scale {
        Some(s) => views::named_scale(model.units, s)?,
        None => {
            let (w, h) = match clip {
                Some((_, r)) => (2.0 * r, 2.0 * r),
                None => {
                    let mut all = members.clone();
                    all.extend(&dashed);
                    let e = views::projected_extent(ctx, &all, &proj);
                    (e.width(), e.height())
                }
            };
            views::fit_scale(model.units, w, h, avail.0, avail.1)
        }
    };
    if spec.kind == ViewKind::Analytical {
        let mut v = views::analytical(ctx, &members, &proj, &title, scale.0);
        v.scale_label = scale.1;
        return Ok(v);
    }
    // Axonometric views are not measurable at any scale.
    let scale = if spec.kind == ViewKind::Iso { (scale.0, "NOT TO SCALE".to_string()) } else { scale };
    let subtitle = spec.subtitle.clone().or_else(|| match (spec.kind, group) {
        (ViewKind::Elevation, Some(g)) => model.group(g).props.get("framing").cloned(),
        _ => None,
    });
    let utilization = spec.kind == ViewKind::Utilization;
    let results = if utilization { Some(results.ok_or("no structural results")?) } else { None };
    let subtitle = if utilization { subtitle.or_else(|| Some("Highest demand/capacity ratio of each member over all load combinations (NDS ASD)".into())) } else { subtitle };
    let mut v = views::projected(
        ctx,
        Projected {
            title,
            subtitle,
            members: members.clone(),
            dashed,
            proj,
            scale: scale.clone(),
            tags: spec.tags && !utilization,
            annotate: if spec.dims { groups } else { vec![] },
            clip,
        },
    );
    if let Some(r) = results {
        heat_fills(ctx, &mut v, &members, &proj, r, scale.0);
    }
    Ok(v)
}

/// Fills each member's silhouette by its utilization band (far to near) and
/// labels it with its ratio; adds a legend beside the view.
fn heat_fills(ctx: &Ctx, v: &mut View, members: &[MemberId], proj: &Projector, r: Results, scale: f64) {
    use crate::doc::{Align, Drawing, Layer};
    let st = crate::annot::Style { scale };
    let mut shapes: Vec<(f64, MemberId, Vec<Vec2>)> = members
        .iter()
        .map(|&m| {
            let g = ctx.geom.member(m);
            let pts: Vec<Vec2> = g.convex().verts.iter().map(|q| proj.p(*q)).collect();
            (proj.depth(g.centroid()), m, topo_core::convex_hull(&pts))
        })
        .collect();
    shapes.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut fills = Drawing::default();
    let mut labels = Drawing::default();
    for (_, m, hull) in &shapes {
        let ratio = r.analysis.utilization(*m);
        if r.analysis.governing[m.idx()].is_none() || hull.len() < 3 {
            continue;
        }
        fills.fill(hull.clone(), Layer::heat(ratio));
        let c = hull.iter().fold(Vec2::ZERO, |a, p| a + *p) / hull.len() as f64;
        let text = format!("{ratio:.2}");
        let h = st.small_h();
        let w = crate::doc::text_width(&text, h) + st.m(0.04);
        let size = crate::BBox2::from_points(hull.iter().copied());
        // Label members big enough to hold it.
        if size.width().max(size.height()) > 2.0 * w {
            labels.fill(vec![c + Vec2::new(-w / 2.0, -h * 0.7), c + Vec2::new(w / 2.0, -h * 0.7), c + Vec2::new(w / 2.0, h * 0.9), c + Vec2::new(-w / 2.0, h * 0.9)], Layer::Knockout);
            labels.text(c - Vec2::new(0.0, h * 0.4), text, h, Align::Middle, Layer::Text);
        }
    }
    // Legend to the right.
    let bb = v.drawing.bbox();
    let x0 = bb.max.x + st.m(0.4);
    let mut y = bb.max.y;
    let h = st.text_h();
    labels.bold(Vec2::new(x0, y), "UTILIZATION", h, Align::Start, Layer::Text);
    for (band, label) in [(0.25, "≤ 0.5"), (0.65, "0.5 – 0.8"), (0.9, "0.8 – 1.0"), (1.2, "1.0 – 1.5  (over)"), (2.0, "> 1.5")] {
        y -= h * 2.2;
        let b = st.m(0.18);
        labels.fill(vec![Vec2::new(x0, y), Vec2::new(x0 + b, y), Vec2::new(x0 + b, y + b * 0.8), Vec2::new(x0, y + b * 0.8)], Layer::heat(band));
        labels.text(Vec2::new(x0 + b * 1.4, y), label, h, Align::Start, Layer::Text);
    }
    let mut d = fills;
    d.extend(std::mem::take(&mut v.drawing));
    d.extend(labels);
    v.drawing = d;
}

/// `S-501` → (`S-`, 501, 3 digits).
fn split_number(n: &str) -> Option<(&str, u32, usize)> {
    let digits = n.len() - n.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    (digits > 0).then(|| (&n[..n.len() - digits], n[n.len() - digits..].parse().unwrap_or(0), digits))
}

/// A sheet number `i` sheets after `first` (`S-501` → `S-502`; `D1` → `D2`;
/// `Details` → `Details (2)`).
pub fn nth_number(first: &str, i: usize) -> String {
    match split_number(first) {
        _ if i == 0 => first.to_string(),
        Some((prefix, n, width)) => format!("{prefix}{:0width$}", n + i as u32),
        None => format!("{first} ({})", i + 1),
    }
}

/// A script-defined sheet (continued on further sheets if its views do not
/// fit). View errors are drawn in place and returned.
pub fn sheet(ctx: &Ctx, number: &str, title: &str, specs: &[ViewSpec], extras: &[View], results: Option<Results>, errors: &mut Vec<String>) -> Vec<Sheet> {
    let area = Sheet::area(sheet::ARCH_D);
    let projected = specs.iter().filter(|s| !matches!(s.kind, ViewKind::Schedule | ViewKind::Notes)).count().max(1);
    let cols = (projected as f64).sqrt().ceil() as usize;
    let rows = projected.div_ceil(cols);
    let avail = (area.width() / cols as f64 - 3.0 * INCH, area.height() / rows as f64 - 3.0 * INCH);
    let views: Vec<View> = specs
        .iter()
        .enumerate()
        .map(|(i, s)| {
            view(ctx, s, avail, extras, results).unwrap_or_else(|e| {
                let name = s.title.clone().unwrap_or_else(|| format!("view {}", i + 1));
                let msg = format!("sheet {number}, {name}: {e}");
                errors.push(msg.clone());
                View::paper("View error", text_block("View error", &[msg], 6.0))
            })
        })
        .collect();
    let mut out = sheet::pack("", 0, title, views);
    for (i, s) in out.iter_mut().enumerate() {
        s.number = nth_number(number, i);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sheet_numbers_continue() {
        assert_eq!(nth_number("S-501", 0), "S-501");
        assert_eq!(nth_number("S-501", 2), "S-503");
        assert_eq!(nth_number("A-009", 1), "A-010");
        assert_eq!(nth_number("Details", 1), "Details (2)");
    }
}
