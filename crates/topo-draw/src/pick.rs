//! Picking named physical features on drawing sheets, and drawing field
//! measurements over them. Coordinates in and out are SVG sheet coordinates
//! (inches, y down), as used by `svg::to_svg`.
//!
//! In a view, a member face seen edge-on appears as a line; where two such
//! lines cross is an edge seen end-on (a "corner"). Both are exact, named
//! features (`topo_geom::measure::Feature`) that measurements can refer to.

use crate::hlr::Projector;
use crate::sheet::Sheet;
use crate::views::View;
use serde::Serialize;
use topo_core::units::INCH;
use topo_core::{convex_hull, MemberId, Model, Plane, Vec2, Vec3};
use topo_geom::measure::{self, Feature, PlaneRef, Quantity, SIDES};
use topo_geom::{Geometry, MemberGeom};

/// One pickable feature.
#[derive(Clone, Debug, Serialize)]
pub struct Candidate {
    pub label: String,
    pub feature: Feature,
    /// Highlight: line segments `[x1, y1, x2, y2]` and points `[x, y]`, SVG inches.
    pub lines: Vec<[f64; 4]>,
    pub points: Vec<[f64; 2]>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Pick {
    /// `corner`, `face` or `member`.
    pub kind: String,
    /// The best feature under the cursor (`None` for a member body).
    pub primary: Option<Candidate>,
    /// Other features at the same spot (the faces of a corner, centrelines…).
    pub alternatives: Vec<Candidate>,
    /// The member under the cursor (for member-length measurements).
    pub member: String,
    pub member_label: String,
    /// Outline of that member, SVG inches.
    pub member_outline: Vec<[f64; 2]>,
    /// That member's canonical axes as arrows from its centre (x with the
    /// grain, y across the thickness, z across the depth), SVG inches.
    pub member_axes: Vec<AxisArrow>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AxisArrow {
    pub name: &'static str,
    pub from: [f64; 2],
    pub to: [f64; 2],
}

/// Mapping between a placed view's model plane and SVG sheet coordinates.
struct Frame<'a> {
    view: &'a View,
    proj: Projector,
    offset: Vec2,
    sheet_h: f64,
}

impl Frame<'_> {
    fn to_model(&self, svg: Vec2) -> Vec2 {
        let paper = Vec2::new(svg.x * INCH, (self.sheet_h - svg.y) * INCH);
        (paper - self.offset) / self.view.scale
    }
    fn to_svg(&self, m: Vec2) -> [f64; 2] {
        let p = m * self.view.scale + self.offset;
        [p.x / INCH, self.sheet_h - p.y / INCH]
    }
    fn line(&self, a: Vec2, b: Vec2) -> [f64; 4] {
        let (p, q) = (self.to_svg(a), self.to_svg(b));
        [p[0], p[1], q[0], q[1]]
    }
}

fn frames(sheet: &Sheet) -> Vec<Frame<'_>> {
    sheet
        .views
        .iter()
        .filter_map(|pv| pv.view.proj.map(|proj| Frame { view: &pv.view, proj, offset: pv.offset, sheet_h: sheet.size.y / INCH }))
        .collect()
}

/// A face of a member seen edge-on in a view: its 2D segment.
struct EdgeOn {
    member: MemberId,
    plane_ref: PlaneRef,
    a: Vec2,
    b: Vec2,
    depth: f64,
}

fn short_path(model: &Model, id: MemberId) -> String {
    let p = model.member_path(id);
    let parts: Vec<&str> = p.split('/').collect();
    parts[parts.len().saturating_sub(2)..].join("/")
}

/// Label for a plane, with the canonical name and, where one applies, the
/// named direction it faces: `+z edge of T2/bottom_chord.F-H1 (up)`.
fn describe(model: &Model, geom: &Geometry, r: &PlaneRef) -> String {
    let who = |member: &str| match model.find_member(member) {
        Ok(i) => (short_path(model, i), Some(i)),
        Err(_) => (member.to_string(), None),
    };
    match r {
        PlaneRef::Face { member, side } => {
            let side = measure::canonical_side(side).unwrap_or(side);
            let (name, id) = who(member);
            let gloss = id
                .and_then(|i| measure::face_plane(geom.member(i), side).ok().map(|p| (i, p)))
                .and_then(|(i, p)| model.describe_direction(p.normal, model.member(i).group, 30.0).into_iter().next());
            format!("{side} {} of {name}{}", measure::side_kind(side), gloss.map(|g| format!(" ({g})")).unwrap_or_default())
        }
        PlaneRef::Mid { member, axis } => {
            let axis = match axis.as_str() {
                "depth" => "z",
                "width" => "y",
                a => a,
            };
            format!("{axis} centreline of {}", who(member).0)
        }
        PlaneRef::Facing { member, direction } => {
            let d = direction.name.clone().unwrap_or_else(|| format!("{:?}", direction.vector.unwrap_or_default()));
            format!("face of {} facing {d}", who(member).0)
        }
    }
}

/// Segment of `plane` (edge-on in `proj`) covered by the member's solid.
fn edge_on_segment(g: &MemberGeom, plane: &Plane, proj: &Projector) -> Option<(Vec2, Vec2)> {
    if plane.normal.dot(proj.toward).abs() > 1e-6 {
        return None;
    }
    let n2 = Vec2::new(plane.normal.dot(proj.right), plane.normal.dot(proj.up));
    let dir = n2.perp().normalized();
    let on: Vec<Vec2> = g.convex().verts.iter().filter(|v| plane.signed_distance(**v).abs() < 1e-6).map(|v| proj.p(*v)).collect();
    // Mid-planes pass through the solid: use its extent along the line.
    let pts: Vec<Vec2> = if on.len() >= 2 {
        on
    } else {
        let all: Vec<Vec2> = g.convex().verts.iter().map(|v| proj.p(*v)).collect();
        let foot = proj.p(plane.point);
        let span: Vec<f64> = all.iter().map(|p| (*p - foot).dot(dir)).collect();
        let (lo, hi) = span.iter().fold((f64::MAX, f64::MIN), |(a, b), s| (a.min(*s), b.max(*s)));
        vec![foot + dir * lo, foot + dir * hi]
    };
    let (mut lo, mut hi) = (pts[0], pts[0]);
    for p in &pts {
        if p.dot(dir) < lo.dot(dir) {
            lo = *p;
        }
        if p.dot(dir) > hi.dot(dir) {
            hi = *p;
        }
    }
    (lo.distance(hi) > 1e-9).then_some((lo, hi))
}

fn seg_distance(q: Vec2, a: Vec2, b: Vec2) -> f64 {
    let d = b - a;
    let t = ((q - a).dot(d) / d.dot(d)).clamp(0.0, 1.0);
    q.distance(a + d * t)
}

fn line_intersection(a: (Vec2, Vec2), b: (Vec2, Vec2)) -> Option<Vec2> {
    let (d1, d2) = (a.1 - a.0, b.1 - b.0);
    let den = d1.cross(d2);
    if den.abs() < 1e-12 * d1.norm() * d2.norm() {
        return None;
    }
    Some(a.0 + d1 * ((b.0 - a.0).cross(d2) / den))
}

/// The feature under an SVG point on a sheet, if any. `radius` is the pick
/// tolerance in SVG inches.
pub fn pick(model: &Model, geom: &Geometry, sheet: &Sheet, svg: [f64; 2], radius: f64) -> Option<Pick> {
    let s = Vec2::new(svg[0], svg[1]);
    for f in frames(sheet) {
        let q = f.to_model(s);
        let r = radius * INCH / f.view.scale;
        let bb = f.view.drawing.bbox();
        if q.x < bb.min.x - r || q.x > bb.max.x + r || q.y < bb.min.y - r || q.y > bb.max.y + r {
            continue;
        }
        if f.view.clip.is_some_and(|(c, rad)| q.distance(c) > rad) {
            continue;
        }
        let mut edges: Vec<EdgeOn> = vec![];
        let mut body: Option<(MemberId, f64)> = None;
        for &id in &f.view.members {
            let g = geom.member(id);
            let sil = convex_hull(&g.convex().verts.iter().map(|v| f.proj.p(*v)).collect::<Vec<_>>());
            // Skip members whose projected box (grown by the pick radius) misses the cursor.
            if !topo_core::BBox2::from_points(sil.iter().copied()).expanded(4.0 * r).overlaps(&topo_core::BBox2 { min: q, max: q }, 0.0) {
                continue;
            }
            let inside = crate::hlr::point_in_convex(q, &sil);
            let depth = f.proj.depth(g.centroid());
            if inside && body.is_none_or(|(_, d)| depth > d) {
                body = Some((id, depth));
            }
            let path = model.member_path(id);
            for side in SIDES {
                let Ok(plane) = measure::face_plane(g, side) else { continue };
                if let Some((a, b)) = edge_on_segment(g, &plane, &f.proj) {
                    edges.push(EdgeOn { member: id, plane_ref: PlaneRef::Face { member: path.clone(), side: side.into() }, a, b, depth });
                }
            }
        }
        let cand = |e: &EdgeOn| Candidate {
            label: describe(model, geom, &e.plane_ref),
            feature: Feature { planes: vec![e.plane_ref.clone()] },
            lines: vec![f.line(e.a, e.b)],
            points: vec![],
        };
        // Corners: two edge-on faces crossing near the cursor.
        let near: Vec<&EdgeOn> = edges.iter().filter(|e| seg_distance(q, e.a, e.b) < 4.0 * r).collect();
        let mut best_corner: Option<(f64, Vec2, usize, usize)> = None;
        for i in 0..near.len() {
            for j in i + 1..near.len() {
                let Some(x) = line_intersection((near[i].a, near[i].b), (near[j].a, near[j].b)) else { continue };
                let on = |e: &EdgeOn| seg_distance(x, e.a, e.b) < r;
                if on(near[i]) && on(near[j]) && x.distance(q) < r && best_corner.is_none_or(|(d, ..)| x.distance(q) < d) {
                    best_corner = Some((x.distance(q), x, i, j));
                }
            }
        }
        let member_of = |id: MemberId| {
            let g = geom.member(id);
            let sil = convex_hull(&g.convex().verts.iter().map(|v| f.proj.p(*v)).collect::<Vec<_>>());
            // Arrows 0.35" long on paper (foreshortened when not in the view plane).
            let c = f.proj.p(g.centroid());
            let len = 0.35 * INCH / f.view.scale;
            let axes = [("x", g.place.x), ("y", g.place.u), ("z", g.place.v)]
                .into_iter()
                .map(|(name, d)| AxisArrow { name, from: f.to_svg(c), to: f.to_svg(c + Vec2::new(d.dot(f.proj.right), d.dot(f.proj.up)) * len) })
                .collect::<Vec<_>>();
            (model.member_path(id), short_path(model, id), sil.iter().map(|p| f.to_svg(*p)).collect::<Vec<_>>(), axes)
        };
        // Centreline alternatives for a member (mid-planes seen edge-on).
        let mids = |id: MemberId| -> Vec<Candidate> {
            let g = geom.member(id);
            ["z", "y"]
                .iter()
                .filter_map(|axis| {
                    let plane = measure::mid_plane(g, axis).ok()?;
                    let (a, b) = edge_on_segment(g, &plane, &f.proj)?;
                    let r = PlaneRef::Mid { member: model.member_path(id), axis: (*axis).into() };
                    Some(Candidate { label: describe(model, geom, &r), feature: Feature { planes: vec![r] }, lines: vec![f.line(a, b)], points: vec![] })
                })
                .collect()
        };
        if let Some((_, x, i, j)) = best_corner {
            let (a, b) = (near[i], near[j]);
            let corner = Candidate {
                label: format!("corner: {} × {}", describe(model, geom, &a.plane_ref), describe(model, geom, &b.plane_ref)),
                feature: Feature { planes: vec![a.plane_ref.clone(), b.plane_ref.clone()] },
                lines: vec![f.line(a.a, a.b), f.line(b.a, b.b)],
                points: vec![f.to_svg(x)],
            };
            let front = if a.depth >= b.depth { a.member } else { b.member };
            let (path, label, outline, axes) = member_of(front);
            return Some(Pick { kind: "corner".into(), primary: Some(corner), alternatives: vec![cand(a), cand(b)], member: path, member_label: label, member_outline: outline, member_axes: axes });
        }
        // Nearest face (front-most on ties).
        let face = edges
            .iter()
            .map(|e| (seg_distance(q, e.a, e.b), e))
            .filter(|(d, _)| *d < r)
            .min_by(|(d1, e1), (d2, e2)| (d1 - e1.depth * 1e-9).total_cmp(&(d2 - e2.depth * 1e-9)));
        if let Some((_, e)) = face {
            let (path, label, outline, axes) = member_of(e.member);
            return Some(Pick { kind: "face".into(), primary: Some(cand(e)), alternatives: mids(e.member), member: path, member_label: label, member_outline: outline, member_axes: axes });
        }
        if let Some((id, _)) = body {
            let (path, label, outline, axes) = member_of(id);
            return Some(Pick { kind: "member".into(), primary: None, alternatives: mids(id), member: path, member_label: label, member_outline: outline, member_axes: axes });
        }
    }
    None
}

/// A measurement drawn over a sheet.
#[derive(Clone, Debug, Serialize)]
pub struct Overlay {
    pub name: String,
    /// Dimension line first, then extension lines; SVG inches.
    pub lines: Vec<[f64; 4]>,
    pub label_at: [f64; 2],
    pub text: String,
    /// Fit status: within tolerance, out of tolerance, or not evaluated.
    pub ok: Option<bool>,
}

/// A measurement to overlay: quantity, name, label text and fit status.
pub struct OverlayInput<'a> {
    pub name: &'a str,
    pub quantity: &'a Quantity,
    pub text: String,
    pub ok: Option<bool>,
}

fn members_of(model: &Model, q: &Quantity) -> Vec<MemberId> {
    let from = |f: &Feature| {
        f.planes
            .iter()
            .filter_map(|p| match p {
                PlaneRef::Face { member, .. } | PlaneRef::Mid { member, .. } | PlaneRef::Facing { member, .. } => model.find_member(member).ok(),
            })
            .collect::<Vec<_>>()
    };
    match q {
        Quantity::Horizontal { a, b } | Quantity::Vertical { a, b } | Quantity::Along { a, b, .. } => {
            let mut v = from(a);
            v.extend(from(b));
            v
        }
        Quantity::Length { member, .. } | Quantity::Rise { member, .. } => model.find_member(member).ok().into_iter().collect(),
    }
}

/// Measurements drawn on the views of `sheet` that show the members they refer to.
pub fn overlay(model: &Model, geom: &Geometry, sheet: &Sheet, items: &[OverlayInput]) -> Vec<Overlay> {
    let mut out = vec![];
    for f in frames(sheet) {
        for it in items {
            let ms = members_of(model, it.quantity);
            // Draw where at least one referenced member is shown (walls under
            // a truss are not in the truss elevation, but the chord is).
            if ms.is_empty() || !ms.iter().any(|m| f.view.members.contains(m)) {
                continue;
            }
            let p2 = |v: Vec3| f.proj.p(v);
            let mut lines = vec![];
            let (start, end) = match it.quantity {
                Quantity::Horizontal { a, b } | Quantity::Vertical { a, b } | Quantity::Along { a, b, .. } => {
                    let (Ok(fa), Ok(fb)) = (measure::feature(model, geom, a), measure::feature(model, geom, b)) else { continue };
                    let d = match it.quantity {
                        Quantity::Horizontal { .. } => match measure::horizontal_direction(&fa, &fb) {
                            Ok(d) => d,
                            Err(_) => continue,
                        },
                        Quantity::Along { direction, .. } => match measure::resolve_direction(model, direction) {
                            Ok(d) => d,
                            Err(_) => continue,
                        },
                        _ => Vec3::Z,
                    };
                    // Only where the dimension is seen true length (in the view plane).
                    if d.dot(f.proj.toward).abs() > 1e-6 {
                        continue;
                    }
                    let d2 = Vec2::new(d.dot(f.proj.right), d.dot(f.proj.up));
                    // Anchor on the lower-dimensional feature; run the dimension along d.
                    let (base, other) = if fa.dirs.len() <= fb.dirs.len() { (&fa, &fb) } else { (&fb, &fa) };
                    let delta = (other.point - base.point).dot(d);
                    let s = p2(base.point);
                    let e = s + d2 * delta;
                    // Extension from the other feature to the dimension line, if it's a point/edge.
                    if other.dirs.len() <= 1 {
                        lines.push(f.line(p2(other.point), e));
                    }
                    (s, e)
                }
                Quantity::Length { member, .. } | Quantity::Rise { member, .. } => {
                    let Ok(id) = model.find_member(member) else { continue };
                    let g = geom.member(id);
                    if g.place.x.dot(f.proj.toward).abs() > 1e-6 {
                        continue;
                    }
                    let (lo, hi) = g.t_range();
                    (p2(g.place.at(g.place.centroid, lo)), p2(g.place.at(g.place.centroid, hi)))
                }
            };
            // In a detail, only measurements that fit inside its circle.
            if f.view.clip.is_some_and(|(c, r)| start.distance(c) > r || end.distance(c) > r) {
                continue;
            }
            lines.insert(0, f.line(start, end));
            let mid = (start + end) * 0.5;
            out.push(Overlay { name: it.name.into(), lines, label_at: f.to_svg(mid), text: it.text.clone(), ok: it.ok });
        }
    }
    out
}

/// Titles and SVG bounding boxes `[x0, y0, x1, y1]` of the model views on a sheet.
pub fn view_boxes(sheet: &Sheet) -> Vec<(String, [f64; 4])> {
    frames(sheet)
        .into_iter()
        .map(|f| {
            let b = f.view.drawing.bbox();
            let (p, q) = (f.to_svg(b.min), f.to_svg(b.max));
            (f.view.title.clone(), [p[0].min(q[0]), p[1].min(q[1]), p[0].max(q[0]), p[1].max(q[1])])
        })
        .collect()
}

/// Where a model point appears on `sheet` (SVG inches), in the first view that
/// shows `member`.
pub fn locate(sheet: &Sheet, member: MemberId, point: Vec3) -> Option<[f64; 2]> {
    frames(sheet).into_iter().find(|f| f.view.members.contains(&member)).map(|f| f.to_svg(f.proj.p(point)))
}
