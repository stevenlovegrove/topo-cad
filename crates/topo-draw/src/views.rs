//! Views: projections of (part of) the model with annotations, at a scale.

use crate::annot::{self, Style};
use crate::doc::{Align, Drawing, Layer};
use crate::hlr::{hidden_lines, Projector};
use crate::schedule::Marks;
use topo_core::units::{FOOT, INCH};
use topo_core::{v2, v3, Annotation, BBox2, GroupId, JunctionKind, MemberId, Model, Topology, UnitSystem, Vec2, Vec3};
use topo_geom::{Geometry, MemberGeom};

#[derive(Clone, Debug)]
pub struct View {
    pub title: String,
    pub subtitle: Option<String>,
    /// Paper length per model length.
    pub scale: f64,
    pub scale_label: String,
    /// In the view plane: model metres (or paper metres when `scale == 1`).
    pub drawing: Drawing,
}

impl View {
    /// Extents on paper (m), relative to the drawing origin.
    pub fn paper_bbox(&self) -> BBox2 {
        let b = self.drawing.bbox();
        BBox2 { min: b.min * self.scale, max: b.max * self.scale }
    }
    /// A paper-space view (tables, legends) with content already in paper metres.
    pub fn paper(title: &str, drawing: Drawing) -> View {
        View { title: title.into(), subtitle: None, scale: 1.0, scale_label: String::new(), drawing }
    }
}

pub struct Ctx<'a> {
    pub model: &'a Model,
    pub topo: &'a Topology,
    pub geom: &'a Geometry,
    pub marks: &'a Marks,
}

/// Standard drawing scales, largest first: (paper per model, label).
pub fn standard_scales(units: UnitSystem) -> Vec<(f64, String)> {
    match units {
        UnitSystem::Imperial => [
            (1.0, "1\""),
            (0.75, "3/4\""),
            (0.5, "1/2\""),
            (0.375, "3/8\""),
            (0.25, "1/4\""),
            (0.1875, "3/16\""),
            (0.125, "1/8\""),
            (0.0625, "1/16\""),
        ]
        .into_iter()
        .map(|(p, l)| (p * INCH / FOOT, format!("{l} = 1'-0\"")))
        .collect(),
        UnitSystem::Metric => [10.0, 20.0, 25.0, 50.0, 100.0, 200.0, 500.0]
            .into_iter()
            .map(|d| (1.0 / d, format!("1:{d}")))
            .collect(),
    }
}

/// Largest standard scale at which a `w × h` (model m) extent fits in `aw × ah` (paper m).
pub fn fit_scale(units: UnitSystem, w: f64, h: f64, aw: f64, ah: f64) -> (f64, String) {
    let scales = standard_scales(units);
    scales.iter().find(|(s, _)| w * s <= aw && h * s <= ah).cloned().unwrap_or_else(|| scales.last().cloned().unwrap())
}

/// Draws visible edges; hidden edges only for members in `hidden_for`.
/// Returns the members that are substantially visible (≥ 30 % of their
/// front-facing edge length), for tagging.
fn draw_members(d: &mut Drawing, geoms: &[&MemberGeom], proj: &Projector, hidden_for: &[MemberId]) -> Vec<MemberId> {
    let mut len: std::collections::BTreeMap<MemberId, (f64, f64)> = Default::default();
    for s in hidden_lines(geoms, proj) {
        let e = len.entry(s.member).or_default();
        let l = s.a.distance(s.b);
        e.1 += l;
        if s.hidden {
            if hidden_for.contains(&s.member) {
                d.line(s.a, s.b, Layer::Hidden);
            }
        } else {
            d.line(s.a, s.b, Layer::Framing);
            e.0 += l;
        }
    }
    len.into_iter().filter(|(_, (v, t))| *v >= 0.3 * t).map(|(m, _)| m).collect()
}

fn group_annotations(d: &mut Drawing, ctx: &Ctx, g: GroupId, proj: &Projector, st: &Style) {
    let group = ctx.model.group(g);
    let f = group.frame;
    let p = |q: Vec3| proj.p(f.to_world(q));
    let dir2 = |q: Vec3| {
        let w = f.dir_to_world(q);
        v2(w.dot(proj.right), w.dot(proj.up))
    };
    for a in &group.annotations {
        match a {
            Annotation::Dim { a, b, side, tier, text } => {
                let t = text.clone().unwrap_or_else(|| ctx.model.units.fmt_len(a.distance(*b)));
                annot::dim(d, p(*a), p(*b), dir2(*side), st.tier_offset(*tier), &t, st);
            }
            Annotation::Region { min, max, label } => {
                let corners = [
                    v3(min.x, min.y, min.z),
                    v3(max.x, min.y, min.z),
                    v3(max.x, max.y, max.z),
                    v3(min.x, max.y, max.z),
                    v3(min.x, min.y, max.z),
                    v3(max.x, max.y, min.z),
                ];
                let bb = BBox2::from_points(corners.iter().map(|&c| p(c)));
                d.rect(bb.min, bb.max, Layer::Annot);
                d.line(bb.min, bb.max, Layer::Annot);
                d.line(v2(bb.min.x, bb.max.y), v2(bb.max.x, bb.min.y), Layer::Annot);
                let c = bb.center();
                let h = st.text_h();
                let w = crate::doc::text_width(label, h) + st.m(0.1);
                d.fill(
                    vec![
                        c + v2(-w / 2.0, -h * 0.9),
                        c + v2(w / 2.0, -h * 0.9),
                        c + v2(w / 2.0, h * 0.9),
                        c + v2(-w / 2.0, h * 0.9),
                    ],
                    Layer::Knockout,
                );
                d.text(c - v2(0.0, h / 2.0), label.clone(), h, Align::Middle, Layer::Text);
            }
            Annotation::Span { a, b, text } => annot::span(d, p(*a), p(*b), text, st),
            Annotation::Note { at, text } => d.text(p(*at), text.clone(), st.text_h(), Align::Start, Layer::Text),
        }
    }
}

/// Tags members at a point on their centroid line, skipping members whose
/// tag point is covered by another solid (e.g. webs under a top chord in plan).
fn tag_members(d: &mut Drawing, ctx: &Ctx, members: &[MemberId], scene: &[&MemberGeom], proj: &Projector, st: &Style) {
    for &m in members {
        let g = ctx.geom.member(m);
        let (lo, hi) = g.t_range();
        let axis2 = v2(g.place.x.dot(proj.right), g.place.x.dot(proj.up));
        // Tag long horizontal-ish members off-centre so they don't collide with vertical tags.
        let t = if axis2.x.abs() > axis2.y.abs() { lo + 0.3 * (hi - lo) } else { 0.5 * (lo + hi) };
        let c = g.place.at(g.place.centroid, t);
        if crate::hlr::is_occluded(c, m, scene, proj) {
            continue;
        }
        annot::tag(d, proj.p(c), ctx.marks.of(m), st);
    }
}

fn finish(mut d: Drawing, title: &str, subtitle: Option<String>, scale: f64, scale_label: String, st: &Style) -> View {
    let bb = d.bbox();
    annot::view_title(&mut d, v2(bb.min.x, bb.min.y - st.m(0.45)), title, subtitle.as_deref(), &scale_label, st);
    View { title: title.into(), subtitle, scale, scale_label, drawing: d }
}

/// Framing elevation of a wall group, viewed from the exterior.
pub fn wall_elevation(ctx: &Ctx, g: GroupId, scale: (f64, String)) -> View {
    let title = format!("{} framing elevation", ctx.model.group(g).name);
    group_elevation(ctx, g, &title, scale)
}

/// Elevation of any group, looking along its local +y axis.
pub fn group_elevation(ctx: &Ctx, g: GroupId, title: &str, scale: (f64, String)) -> View {
    let group = ctx.model.group(g);
    let proj = Projector::new(-group.frame.y, Vec3::Z);
    let st = Style { scale: scale.0 };
    let members = ctx.model.members_in_tree(g);
    let geoms: Vec<&MemberGeom> = members.iter().map(|&m| ctx.geom.member(m)).collect();
    let mut d = Drawing::default();
    let _ = draw_members(&mut d, &geoms, &proj, &[]);
    tag_members(&mut d, ctx, &members, &geoms, &proj, &st);
    group_annotations(&mut d, ctx, g, &proj, &st);
    let sub = group.props.get("framing").cloned();
    finish(d, title, sub, scale.0, scale.1, &st)
}

/// Framing plan of a floor group; `below` members (e.g. wall top plates) are
/// drawn too, dashed where the floor hides them.
pub fn framing_plan(ctx: &Ctx, g: GroupId, below: &[MemberId], scale: (f64, String)) -> View {
    let proj = Projector::new(Vec3::Z, Vec3::Y);
    let st = Style { scale: scale.0 };
    let floor = ctx.model.members_in_tree(g);
    let mut all = floor.clone();
    all.extend_from_slice(below);
    let geoms: Vec<&MemberGeom> = all.iter().map(|&m| ctx.geom.member(m)).collect();
    let mut d = Drawing::default();
    let _ = draw_members(&mut d, &geoms, &proj, below);
    tag_members(&mut d, ctx, &floor, &geoms, &proj, &st);
    group_annotations(&mut d, ctx, g, &proj, &st);
    // Label the walls below at their midpoints, outside the footprint.
    let centre = BBox2::from_points(geoms.iter().map(|g| proj.p(g.centroid()))).center();
    for grp in ctx.model.groups.iter().filter(|x| x.kind == "wall") {
        let ms = ctx.model.members_in_tree(grp.id);
        if !ms.iter().any(|m| below.contains(m)) {
            continue;
        }
        let mid = proj.p(grp.frame.origin + grp.frame.x * (ctx.model.members_in_tree(grp.id).iter().map(|&m| ctx.geom.member(m).cut_length()).fold(0.0, f64::max) * 0.5));
        let out = (mid - centre).normalized();
        let label = grp.name.split(" (").next().unwrap_or(&grp.name).to_uppercase();
        // Beyond the dimension tiers.
        d.text(mid + out * st.m(1.3) - v2(0.0, st.text_h() * 0.5), label, st.text_h(), Align::Middle, Layer::Text);
    }
    let sub = Some("Bearing walls below shown dashed".to_string());
    finish(d, &format!("{} plan", ctx.model.group(g).name), sub, scale.0, scale.1, &st)
}

/// Axonometric view of `members` from the south-west, hidden lines removed.
pub fn isometric(ctx: &Ctx, members: &[MemberId], title: &str, scale: f64) -> View {
    let proj = iso_projector();
    let st = Style { scale };
    let geoms: Vec<&MemberGeom> = members.iter().map(|&m| ctx.geom.member(m)).collect();
    let mut d = Drawing::default();
    let _ = draw_members(&mut d, &geoms, &proj, &[]);
    finish(d, title, None, scale, "NOT TO SCALE".into(), &st)
}

pub fn iso_projector() -> Projector {
    Projector::new(v3(-1.0, -1.25, 0.9), Vec3::Z)
}

/// Model extents for a projection (for scale fitting).
pub fn projected_extent(ctx: &Ctx, members: &[MemberId], proj: &Projector) -> BBox2 {
    BBox2::from_points(members.iter().flat_map(|&m| ctx.geom.member(m).convex().verts).map(|v| proj.p(v)))
}

fn junction_symbol(d: &mut Drawing, kind: JunctionKind, c: Vec2, r: f64) {
    let sq = |d: &mut Drawing, r: f64, fill: bool| {
        let pts = vec![c + v2(-r, -r), c + v2(r, -r), c + v2(r, r), c + v2(-r, r)];
        if fill {
            d.fill(pts, Layer::Nodes)
        } else {
            d.poly(pts, true, Layer::Nodes)
        }
    };
    match kind {
        JunctionKind::Tee => d.circle(c, r, true, Layer::Nodes),
        JunctionKind::Cross => {
            d.line(c + v2(-r, -r), c + v2(r, r), Layer::Nodes);
            d.line(c + v2(-r, r), c + v2(r, -r), Layer::Nodes);
            d.circle(c, r * 1.2, false, Layer::Nodes);
        }
        JunctionKind::Corner => sq(d, r * 0.9, true),
        JunctionKind::Splice => d.fill(vec![c + v2(0.0, -r), c + v2(r, 0.0), c + v2(0.0, r), c + v2(-r, 0.0)], Layer::Nodes),
        JunctionKind::Free => d.circle(c, r, false, Layer::Nodes),
        JunctionKind::Complex => sq(d, r, false),
        JunctionKind::Pass => d.circle(c, r * 0.4, true, Layer::Nodes),
        JunctionKind::Isolated => {}
    }
}

/// Analytical (topology) model: member centre-line segments between path
/// nodes, junction symbols, supports. This is the model an engineer checks
/// for connectivity before any numbers are run.
pub fn analytical(ctx: &Ctx, members: &[MemberId], proj: &Projector, title: &str, scale: f64) -> View {
    let st = Style { scale };
    let mut d = Drawing::default();
    let m = ctx.model;
    let mut nodes = vec![];
    for &id in members {
        // Node-to-node element lines (section eccentricity is an element property).
        let path = &m.member(id).path;
        for w in path.windows(2) {
            d.line(proj.p(m.pos(w[0])), proj.p(m.pos(w[1])), Layer::Analytical);
        }
        nodes.extend(path.iter().copied());
    }
    nodes.sort();
    nodes.dedup();
    let r = st.m(0.03);
    // Draw back-to-front so nearer symbols are on top.
    nodes.sort_by(|a, b| proj.depth(m.pos(*a)).total_cmp(&proj.depth(m.pos(*b))));
    for &n in &nodes {
        junction_symbol(&mut d, ctx.topo.junction(n).kind, proj.p(m.pos(n)), r);
    }
    for s in m.supports.iter().filter(|s| nodes.contains(&s.node)) {
        let c = proj.p(m.pos(s.node));
        let h = st.m(0.07);
        d.poly(vec![c, c + v2(-h * 0.6, -h), c + v2(h * 0.6, -h)], true, Layer::Supports);
    }
    let legend_at = {
        let bb = d.bbox();
        v2(bb.max.x + st.m(0.5), bb.max.y)
    };
    let census = ctx.topo.census();
    let lh = st.text_h();
    d.bold(legend_at, "LEGEND", lh * 1.2, Align::Start, Layer::Text);
    for (i, (k, n)) in census.iter().enumerate() {
        let y = legend_at.y - (i as f64 + 1.5) * lh * 2.2;
        junction_symbol(&mut d, *k, v2(legend_at.x + r, y + lh * 0.4), r);
        d.text(v2(legend_at.x + st.m(0.15), y), format!("{} ({n})", k.label()), lh, Align::Start, Layer::Text);
    }
    let y = legend_at.y - (census.len() as f64 + 1.5) * lh * 2.2;
    let c = v2(legend_at.x + r, y + lh);
    let h = st.m(0.07);
    d.poly(vec![c, c + v2(-h * 0.6, -h), c + v2(h * 0.6, -h)], true, Layer::Supports);
    d.text(v2(legend_at.x + st.m(0.15), y), format!("support ({})", m.supports.len()), lh, Align::Start, Layer::Text);
    finish(d, title, Some("Centre-line model with junction classification".into()), scale, "NOT TO SCALE".into(), &st)
}

/// Units label for sheet notes.
pub fn units_note(u: UnitSystem) -> &'static str {
    match u {
        UnitSystem::Imperial => "Dimensions in feet-inches; lumber sizes nominal",
        UnitSystem::Metric => "Dimensions in millimetres",
    }
}
