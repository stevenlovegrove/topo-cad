//! Views and sheets from the drawing specs a script writes (`sheet(...)`,
//! `plan(...)`, `elevation(...)`, `detail(...)`, …).

use crate::schedule;
use crate::sheet::{self, Sheet};
use crate::views::{self, Ctx, Projected, View};
use crate::{hlr::Projector, text_block};
use topo_core::units::INCH;
use topo_core::{resolve_direction_in, DirRef, Frame, GroupId, MemberId, Model, Vec2, Vec3, ViewKind, ViewSpec};

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
        ViewKind::Schedule => spec.table.clone().unwrap_or_default(),
        ViewKind::Notes => "Notes".into(),
    }
}

/// Builds one view. `avail` is the paper space (m) to fit it in when no
/// scale is given; `extras` are tool-supplied tables schedules may name.
pub fn view(ctx: &Ctx, spec: &ViewSpec, avail: (f64, f64), extras: &[View]) -> Result<View, String> {
    let model = ctx.model;
    let title = spec.title.clone().unwrap_or_else(|| default_title(model, spec));
    match spec.kind {
        ViewKind::Schedule => {
            let name = spec.table.as_deref().ok_or("a schedule needs a table name")?;
            let t = match name {
                "members" => View::paper("Member schedule", schedule::member_schedule_table(ctx.marks)),
                "connections" => View::paper("Connection schedule", schedule::connection_schedule_table(model)),
                "junctions" => View::paper("Junctions", schedule::junction_table(ctx.topo)),
                other => extras.iter().find(|v| v.title.eq_ignore_ascii_case(other)).cloned().ok_or_else(|| {
                    let mut known = vec!["members".to_string(), "connections".into(), "junctions".into()];
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
    Ok(views::projected(
        ctx,
        Projected {
            title,
            subtitle,
            members,
            dashed,
            proj,
            scale,
            tags: spec.tags,
            annotate: if spec.dims { groups } else { vec![] },
            clip,
        },
    ))
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
pub fn sheet(ctx: &Ctx, number: &str, title: &str, specs: &[ViewSpec], extras: &[View], errors: &mut Vec<String>) -> Vec<Sheet> {
    let area = Sheet::area(sheet::ARCH_D);
    let projected = specs.iter().filter(|s| !matches!(s.kind, ViewKind::Schedule | ViewKind::Notes)).count().max(1);
    let cols = (projected as f64).sqrt().ceil() as usize;
    let rows = projected.div_ceil(cols);
    let avail = (area.width() / cols as f64 - 3.0 * INCH, area.height() / rows as f64 - 3.0 * INCH);
    let views: Vec<View> = specs
        .iter()
        .enumerate()
        .map(|(i, s)| {
            view(ctx, s, avail, extras).unwrap_or_else(|e| {
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
