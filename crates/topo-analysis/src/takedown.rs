//! Gravity load takedown: loads are applied where they act (roof and floor
//! surfaces by tributary width, member self-weight, point and line loads) and
//! carried down the support graph member by member to the foundation, the way
//! an engineer does it by hand:
//!
//! * **Beams** (horizontal or sloped members) are simple spans between
//!   adjacent supports (an overhang belongs to its end span); loads over a
//!   support go straight into it. Reactions by statics, internal forces by
//!   free body, deflection by integrating M/EI.
//! * **Columns** (vertical members) take everything above down to their
//!   lowest support.
//! * **Trusses** (groups of kind `truss`) are solved as plane frames: chords
//!   continuous through panel points, members pinned to each other.
//!
//! A reaction becomes a point load on the supporting member where they meet.
//! Transfers through nails in shear (`Transfer::Fastened`) are kept for
//! connection checks. Everything is linear, so each load case is taken down
//! separately and combinations are superposed.

use crate::frame2d::{Elem, Frame2d};
use crate::loadpath::{support_graph, Transfer};
use serde::Serialize;
use std::collections::HashMap;
use topo_core::{AreaBasis, GroupId, Issue, Load, LoadCaseId, MemberId, Model, NodeId, Severity, Topology, Vec3};
use topo_geom::Geometry;

const G: f64 = 9.80665;
/// Support points closer than this along a member are one support.
const CLUSTER: f64 = 0.005;
/// Spacing of support points representing a bonded (face-to-face) contact.
const BED_STEP: f64 = 0.1;
/// Offset used to sample just either side of a point load or support.
const EPS: f64 = 1e-6;
/// Nailed supports this close to a bearing support are not relied on.
const NAIL_NEAR: f64 = 0.3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Behaviour {
    /// Bends between supports.
    Beam,
    /// Carries load axially to its base.
    Column,
    /// Part of a truss solved as a plane frame.
    Truss,
    /// No support found: its load goes nowhere (reported as an issue).
    Unsupported,
}

/// Where a member is supported.
#[derive(Clone, Debug, Serialize)]
pub struct SupportPt {
    /// Distance along the member axis from its first node.
    pub t: f64,
    /// The supporting member (`None`: the foundation).
    pub by: Option<MemberId>,
    pub node: Option<NodeId>,
    pub transfer: Transfer,
    /// World position.
    pub at: Vec3,
    /// Contact length along the member (for bearing stress).
    pub bearing: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct MemberInfo {
    pub behaviour: Behaviour,
    pub supports: Vec<SupportPt>,
    /// Clear spans between adjacent supports, centre to centre (m).
    pub spans: Vec<f64>,
    /// Analytical length (first to last node).
    pub length: f64,
    /// Cosine of the member's slope (1 for horizontal).
    pub cos: f64,
    /// Bending stiffness for vertical-plane bending (N·m²).
    pub ei: f64,
    /// The member this one lies on along its length and bends with.
    pub rides_on: Option<MemberId>,
}

/// Internal forces along a member, sampled at `t` (distance along the axis).
#[derive(Clone, Debug, Default, Serialize)]
pub struct Diagram {
    pub t: Vec<f64>,
    /// Axial force, tension positive (N).
    pub n: Vec<f64>,
    /// Shear perpendicular to the member (N).
    pub v: Vec<f64>,
    /// Bending moment, sagging positive (N·m).
    pub m: Vec<f64>,
    /// Deflection perpendicular to the member, downward positive (m).
    pub y: Vec<f64>,
}

impl Diagram {
    fn zeros(t: Vec<f64>) -> Diagram {
        let n = t.len();
        Diagram { t, n: vec![0.0; n], v: vec![0.0; n], m: vec![0.0; n], y: vec![0.0; n] }
    }
    fn add_scaled(&mut self, o: &Diagram, f: f64) {
        for (a, b) in [(&mut self.n, &o.n), (&mut self.v, &o.v), (&mut self.m, &o.m), (&mut self.y, &o.y)] {
            for (x, y) in a.iter_mut().zip(b) {
                *x += f * y;
            }
        }
    }
    pub fn max_abs(v: &[f64]) -> (f64, usize) {
        v.iter().enumerate().fold((0.0, 0), |(m, i), (k, x)| if x.abs() > m { (x.abs(), k) } else { (m, i) })
    }
}

/// Force on a member from one of its supports.
#[derive(Clone, Debug, Serialize)]
pub struct Reaction {
    pub member: MemberId,
    pub by: Option<MemberId>,
    pub node: Option<NodeId>,
    pub at: Vec3,
    /// Force on the member from the support (N); upward for gravity.
    pub force: Vec3,
    pub transfer: Transfer,
}

#[derive(Clone, Debug, Default)]
struct Loading {
    /// (t, force) point loads.
    points: Vec<(f64, Vec3, Option<MemberId>)>,
    /// (t0, t1, w0, w1) line loads per unit member length.
    lines: Vec<(f64, f64, Vec3, Vec3)>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Takedown {
    pub cases: Vec<LoadCaseId>,
    pub info: Vec<MemberInfo>,
    /// `[case][member]`.
    pub diagrams: Vec<Vec<Diagram>>,
    /// `[case]`, aligned across cases (same order and supports).
    pub reactions: Vec<Vec<Reaction>>,
    /// Total vertical load applied per case (N, downward positive).
    pub applied: Vec<f64>,
    pub issues: Vec<Issue>,
}

/// Axis of a member: start point, unit direction, length.
fn axis(model: &Model, m: MemberId) -> (Vec3, Vec3, f64) {
    let mem = model.member(m);
    let (a, b) = (model.pos(mem.start()), model.pos(mem.end()));
    let l = a.distance(b);
    (a, (b - a) / l, l)
}

fn param(model: &Model, m: MemberId, p: Vec3) -> f64 {
    let (a, x, l) = axis(model, m);
    (p - a).dot(x).clamp(0.0, l)
}

/// Second moment for bending about `axis` (a unit vector in the section plane).
fn i_about(model: &Model, m: MemberId, geom: &Geometry, a: Vec3) -> f64 {
    let p = &model.section(model.member(m).section).props;
    let g = geom.member(m);
    // i_u is about the u axis (depth along v), i_v about v.
    p.i_u * a.dot(g.place.u).powi(2) + p.i_v * a.dot(g.place.v).powi(2)
}

fn ei_vertical_plane(model: &Model, m: MemberId, geom: &Geometry) -> f64 {
    let (_, x, _) = axis(model, m);
    let a = x.cross(Vec3::Z).try_normalized().unwrap_or(Vec3::X);
    model.material(model.member(m).material).e * i_about(model, m, geom, a)
}

/// Members receiving an area load on a group: truss top chords and rafters
/// for roofs, joists for floors.
fn receivers(model: &Model, g: GroupId) -> Vec<MemberId> {
    let tree = model.members_in_tree(g);
    for roles in [&["top_chord", "rafter"][..], &["joist"][..]] {
        let r: Vec<MemberId> = tree.iter().copied().filter(|&m| roles.contains(&model.member(m).role.as_str())).collect();
        if !r.is_empty() {
            return r;
        }
    }
    vec![]
}

/// Tributary width per receiving member: members are grouped into lines by
/// their position across their span (horizontal, perpendicular to the axis);
/// each line takes half the gap to each neighbour.
fn tributary(model: &Model, ms: &[MemberId]) -> HashMap<MemberId, f64> {
    let Some(&first) = ms.first() else { return HashMap::new() };
    let (_, x0, _) = axis(model, first);
    let across = Vec3::new(-x0.y, x0.x, 0.0).try_normalized().unwrap_or(Vec3::Y);
    let pos = |m: MemberId| {
        let (a, x, l) = axis(model, m);
        (a + x * (l / 2.0)).dot(across)
    };
    let mut lines: Vec<f64> = vec![];
    for &m in ms {
        let p = pos(m);
        if !lines.iter().any(|q| (q - p).abs() < 0.01) {
            lines.push(p);
        }
    }
    lines.sort_by(f64::total_cmp);
    let trib = |i: usize| -> f64 {
        let lo = if i > 0 { (lines[i] - lines[i - 1]) / 2.0 } else { 0.0 };
        let hi = if i + 1 < lines.len() { (lines[i + 1] - lines[i]) / 2.0 } else { 0.0 };
        lo + hi
    };
    ms.iter()
        .map(|&m| {
            let p = pos(m);
            let i = lines.iter().position(|q| (q - p).abs() < 0.01).unwrap();
            (m, trib(i))
        })
        .collect()
}

/// Supports of each member: support-graph edges (points), bonds (a bed of
/// points along the contact) and foundation supports at its nodes.
fn supports_of(model: &Model, topo: &Topology, geom: &Geometry) -> Vec<Vec<SupportPt>> {
    let mut out: Vec<Vec<SupportPt>> = vec![vec![]; model.members.len()];
    let extent = |m: MemberId, on: MemberId| -> (f64, f64) {
        let (a, x, _) = axis(model, on);
        let ts: Vec<f64> = geom.member(m).convex().verts.iter().map(|v| (*v - a).dot(x)).collect();
        (ts.iter().copied().fold(f64::INFINITY, f64::min), ts.iter().copied().fold(f64::NEG_INFINITY, f64::max))
    };
    for e in support_graph(model, topo, geom) {
        let (a, x, l) = axis(model, e.member);
        match e.node {
            Some(n) => {
                let p = model.pos(n);
                let t = param(model, e.member, p);
                let (lo, hi) = extent(e.by, e.member);
                let (mlo, mhi) = extent(e.member, e.member);
                let bearing = (hi.min(mhi) - lo.max(mlo)).max(0.0);
                out[e.member.idx()].push(SupportPt { t, by: Some(e.by), node: Some(n), transfer: e.transfer, at: p, bearing });
            }
            None => {
                // A bed along the overlap with the supporter.
                let (lo, hi) = extent(e.by, e.member);
                let (lo, hi) = (lo.max(0.0), hi.min(l));
                if hi <= lo {
                    continue;
                }
                let n = ((hi - lo) / BED_STEP).ceil().max(1.0) as usize;
                let step = (hi - lo) / n as f64;
                for k in 0..=n {
                    let t = lo + step * k as f64;
                    let bearing = if k == 0 || k == n { step / 2.0 } else { step };
                    out[e.member.idx()].push(SupportPt { t, by: Some(e.by), node: None, transfer: e.transfer, at: a + x * t, bearing });
                }
            }
        }
    }
    for s in &model.supports {
        for m in &model.members {
            if m.path.contains(&s.node) {
                let p = model.pos(s.node);
                out[m.id.idx()].push(SupportPt { t: param(model, m.id, p), by: None, node: Some(s.node), transfer: Transfer::Bearing, at: p, bearing: 0.0 });
            }
        }
    }
    // Nails are relied on only where there is no bearing nearby: a plate end
    // nailed at a corner, or a header also nailed to its kings, carries its
    // load in bearing a few inches away.
    for v in &mut out {
        let bearing: Vec<f64> = v.iter().filter(|s| s.transfer == Transfer::Bearing).map(|s| s.t).collect();
        v.retain(|s| s.transfer == Transfer::Bearing || !bearing.iter().any(|b| (b - s.t).abs() < NAIL_NEAR));
        v.sort_by(|a, b| a.t.total_cmp(&b.t));
    }
    out
}

/// A member lying on another along (nearly) its whole length rides on it:
/// the two bend together (e.g. a double top plate). Returns, per member, the
/// member it rides on.
fn riders(model: &Model, sup: &[Vec<SupportPt>]) -> Vec<Option<MemberId>> {
    sup.iter()
        .enumerate()
        .map(|(mi, s)| {
            let beds: Vec<&SupportPt> = s.iter().filter(|p| p.node.is_none()).collect();
            if beds.len() < 3 || beds.len() * 5 < s.len() * 4 {
                return None;
            }
            let (_, _, l) = axis(model, model.members[mi].id);
            let mut count: HashMap<MemberId, (usize, f64)> = HashMap::new();
            for b in &beds {
                let e = count.entry(b.by?).or_insert((0, 0.0));
                e.0 += 1;
                e.1 += b.bearing;
            }
            let (&by, &(_, covered)) = count.iter().max_by(|a, b| a.1 .1.total_cmp(&b.1 .1))?;
            (covered >= 0.8 * l).then_some(by)
        })
        .collect()
}

/// Support points grouped by position: `(t, indices)`.
fn clusters(sup: &[SupportPt]) -> Vec<(f64, Vec<usize>)> {
    let mut out: Vec<(f64, Vec<usize>)> = vec![];
    for (i, s) in sup.iter().enumerate() {
        match out.last_mut() {
            Some((t, v)) if (s.t - *t).abs() < CLUSTER => v.push(i),
            _ => out.push((s.t, vec![i])),
        }
    }
    out
}

fn sample_points(len: f64, keys: &[f64]) -> Vec<f64> {
    let n = 200;
    let mut t: Vec<f64> = (0..=n).map(|i| len * i as f64 / n as f64).collect();
    for &k in keys {
        for d in [-EPS, 0.0, EPS] {
            let v = k + d;
            if (0.0..=len).contains(&v) {
                t.push(v);
            }
        }
    }
    t.sort_by(f64::total_cmp);
    t.dedup_by(|a, b| (*a - *b).abs() < EPS / 10.0);
    t
}

pub fn takedown(model: &Model, topo: &Topology, geom: &Geometry) -> Takedown {
    let nm = model.members.len();
    let cases: Vec<LoadCaseId> = model.load_cases.iter().map(|c| c.id).collect();
    let nc = cases.len();
    let mut issues = vec![];
    let sup = supports_of(model, topo, geom);

    // ----- carriers: single members, or whole trusses ------------------------
    let truss_of: Vec<Option<GroupId>> = model
        .members
        .iter()
        .map(|m| {
            let mut g = m.group;
            while let Some(id) = g {
                if model.group(id).kind == "truss" {
                    return Some(id);
                }
                g = model.group(id).parent;
            }
            None
        })
        .collect();
    let mut carrier_of = vec![0usize; nm];
    let mut carriers: Vec<Vec<MemberId>> = vec![];
    let mut truss_index: HashMap<GroupId, usize> = HashMap::new();
    for m in &model.members {
        let c = match truss_of[m.id.idx()] {
            Some(g) => *truss_index.entry(g).or_insert_with(|| {
                carriers.push(vec![]);
                carriers.len() - 1
            }),
            None => {
                carriers.push(vec![]);
                carriers.len() - 1
            }
        };
        carriers[c].push(m.id);
        carrier_of[m.id.idx()] = c;
    }
    let is_truss = |c: usize| truss_of[carriers[c][0].idx()].is_some();

    // Order: a carrier is processed once everything resting on it is done.
    let ncar = carriers.len();
    let mut below: Vec<Vec<usize>> = vec![vec![]; ncar];
    let mut above_count = vec![0usize; ncar];
    for (mi, s) in sup.iter().enumerate() {
        let c = carrier_of[mi];
        for p in s {
            if let Some(b) = p.by {
                let cb = carrier_of[b.idx()];
                if cb != c && !below[c].contains(&cb) {
                    below[c].push(cb);
                    above_count[cb] += 1;
                }
            }
        }
    }
    let mut order = vec![];
    let mut ready: Vec<usize> = (0..ncar).filter(|&c| above_count[c] == 0).collect();
    while let Some(c) = ready.pop() {
        order.push(c);
        for &b in &below[c] {
            above_count[b] -= 1;
            if above_count[b] == 0 {
                ready.push(b);
            }
        }
    }
    if order.len() < ncar {
        let stuck: Vec<MemberId> = (0..ncar).filter(|c| !order.contains(c)).flat_map(|c| carriers[c].clone()).collect();
        issues.push(Issue::new(Severity::Warning, "support-cycle", format!("{} members support each other in a cycle; their loads are not taken down", stuck.len())).members(stuck));
    }

    // ----- applied loads ---------------------------------------------------------
    let mut loading: Vec<Vec<Loading>> = vec![vec![Loading::default(); nm]; nc];
    let mut applied = vec![0.0; nc];
    for (ci, case) in model.load_cases.iter().enumerate() {
        if case.self_weight {
            for m in &model.members {
                let (_, _, l) = axis(model, m.id);
                let w = model.material(m.material).density * G * model.section(m.section).props.area;
                loading[ci][m.id.idx()].lines.push((0.0, l, Vec3::Z * -w, Vec3::Z * -w));
                applied[ci] += w * l;
            }
        }
    }
    for load in &model.loads {
        match load {
            Load::Area { case, group, pressure, direction, basis, .. } => {
                let ci = case.idx();
                let rs = receivers(model, *group);
                if rs.is_empty() {
                    issues.push(Issue::new(Severity::Warning, "area-load-unassigned", format!("area load on {} has no receiving members (rafters, truss top chords or joists)", model.group(*group).name)));
                    continue;
                }
                let trib = tributary(model, &rs);
                for m in rs {
                    let (_, x, l) = axis(model, m);
                    let cos = (1.0 - x.z * x.z).sqrt();
                    let per_len = match basis {
                        AreaBasis::Plan => pressure * trib[&m] * cos,
                        AreaBasis::Surface => pressure * trib[&m],
                    };
                    let w = direction.normalized() * per_len;
                    loading[ci][m.idx()].lines.push((0.0, l, w, w));
                    applied[ci] += -w.z * l;
                }
            }
            Load::MemberUniform { case, member, w } => {
                let (_, _, l) = axis(model, *member);
                loading[case.idx()][member.idx()].lines.push((0.0, l, *w, *w));
                applied[case.idx()] += -w.z * l;
            }
            Load::Node { case, node, force, .. } => {
                if let Some(m) = model.members.iter().find(|m| m.path.contains(node)) {
                    let t = param(model, m.id, model.pos(*node));
                    loading[case.idx()][m.id.idx()].points.push((t, *force, None));
                    applied[case.idx()] += -force.z;
                }
            }
        }
    }

    // ----- member info ------------------------------------------------------------
    let mut info: Vec<MemberInfo> = model
        .members
        .iter()
        .map(|m| {
            let (_, x, l) = axis(model, m.id);
            let behaviour = if truss_of[m.id.idx()].is_some() {
                Behaviour::Truss
            } else if sup[m.id.idx()].is_empty() {
                Behaviour::Unsupported
            } else if x.z.abs() > 0.9 {
                Behaviour::Column
            } else {
                Behaviour::Beam
            };
            let cl = clusters(&sup[m.id.idx()]);
            let spans = cl.windows(2).map(|w| w[1].0 - w[0].0).collect();
            MemberInfo { behaviour, supports: sup[m.id.idx()].clone(), spans, length: l, cos: (1.0 - x.z * x.z).sqrt(), ei: ei_vertical_plane(model, m.id, geom), rides_on: None }
        })
        .collect();

    let rides_on = riders(model, &sup);
    let mut riders_of: Vec<Vec<MemberId>> = vec![vec![]; nm];
    for (mi, r) in rides_on.iter().enumerate() {
        if let Some(b) = r {
            if info[mi].behaviour == Behaviour::Beam && info[b.idx()].behaviour == Behaviour::Beam {
                riders_of[b.idx()].push(model.members[mi].id);
                info[mi].rides_on = Some(*b);
            }
        }
    }

    // ----- take down ---------------------------------------------------------------
    let mut diagrams: Vec<Vec<Diagram>> = vec![vec![Diagram::default(); nm]; nc];
    let mut reactions: Vec<Vec<Reaction>> = vec![vec![]; nc];
    let mut unsupported_reported = vec![false; nm];
    for &c in &order {
        if is_truss(c) {
            match truss(model, geom, &carriers[c], &sup, &loading) {
                Ok((ds, rs)) => {
                    for ci in 0..nc {
                        for (m, d) in &ds[ci] {
                            diagrams[ci][m.idx()] = d.clone();
                        }
                        for r in &rs[ci] {
                            transfer(model, r, &mut loading[ci]);
                        }
                        reactions[ci].extend(rs[ci].iter().cloned());
                    }
                }
                Err(e) => {
                    let g = truss_of[carriers[c][0].idx()].unwrap();
                    issues.push(Issue::new(Severity::Warning, "truss-unsolved", format!("{}: {e}", model.group_path(g))).members(carriers[c].clone()));
                    for &m in &carriers[c] {
                        info[m.idx()].behaviour = Behaviour::Unsupported;
                    }
                }
            }
            continue;
        }
        let m = carriers[c][0];
        let mi = m.idx();
        let keys: Vec<f64> = sup[mi].iter().map(|s| s.t).chain((0..nc).flat_map(|ci| loading[ci][mi].points.iter().map(|p| p.0).collect::<Vec<_>>())).collect();
        let ts = sample_points(info[mi].length, &keys);
        for ci in 0..nc {
            let (d, rs) = match info[mi].behaviour {
                Behaviour::Beam => beam(m, &info[mi], &sup[mi], &loading[ci][mi], &ts),
                Behaviour::Column => column(model, m, &info[mi], &sup[mi], &loading[ci][mi], &ts),
                _ => {
                    let has_load = !loading[ci][mi].points.is_empty() || !loading[ci][mi].lines.is_empty();
                    if has_load && !unsupported_reported[mi] {
                        unsupported_reported[mi] = true;
                        issues.push(Issue::new(Severity::Warning, "unsupported-load", format!("{} ({}) carries load but has no support; its load is lost", model.member_path(m), model.member(m).role)).members([m]));
                    }
                    (Diagram::zeros(ts.clone()), vec![])
                }
            };
            for r in &rs {
                transfer(model, r, &mut loading[ci]);
            }
            diagrams[ci][mi] = d;
            reactions[ci].extend(rs);
        }
        // Members riding on this one share its bending in proportion to
        // stiffness (same curvature), on top of their own local bending.
        if !riders_of[mi].is_empty() {
            let total: f64 = info[mi].ei + riders_of[mi].iter().map(|r| info[r.idx()].ei).sum::<f64>();
            for dc in diagrams.iter_mut() {
                let base = dc[mi].clone();
                for &r in &riders_of[mi] {
                    let share = info[r.idx()].ei / total;
                    let rd = &mut dc[r.idx()];
                    let (ra, rx, _) = axis(model, r);
                    for k in 0..rd.t.len() {
                        let tb = param(model, m, ra + rx * rd.t[k]);
                        let (v, mm, y) = interpolate(&base, tb);
                        rd.v[k] += v * share;
                        rd.m[k] += mm * share;
                        rd.y[k] = y;
                    }
                }
                let own = info[mi].ei / total;
                let d = &mut dc[mi];
                for k in 0..d.t.len() {
                    d.v[k] *= own;
                    d.m[k] *= own;
                }
            }
        }
    }
    Takedown { cases, info, diagrams, reactions, applied, issues }
}

/// Puts a reaction onto its supporting member as an equal and opposite load.
fn transfer(model: &Model, r: &Reaction, loading: &mut [Loading]) {
    if let Some(by) = r.by {
        let t = param(model, by, r.at);
        loading[by.idx()].points.push((t, -r.force, Some(r.member)));
    }
}

/// Point loads (t, downward force) and linear line loads (t0, t1, w0, w1).
type VerticalLoads = (Vec<(f64, f64)>, Vec<(f64, f64, f64, f64)>);

/// Vertical (downward-positive) components of a member's loads.
fn vertical_loads(l: &Loading) -> VerticalLoads {
    let pts = l.points.iter().map(|p| (p.0, -p.1.z)).collect();
    let lines = l.lines.iter().map(|&(a, b, w0, w1)| (a, b, -w0.z, -w1.z)).collect();
    (pts, lines)
}

/// Resultant and centroid of a linear load from `wa` at `a` to `wb` at `b`.
fn resultant(a: f64, b: f64, wa: f64, wb: f64) -> (f64, f64) {
    let r = (wa + wb) / 2.0 * (b - a);
    let x = if (wa + wb).abs() > 1e-300 { a + (b - a) * (wa + 2.0 * wb) / (3.0 * (wa + wb)) } else { (a + b) / 2.0 };
    (r, x)
}

/// Clips a linear load to [lo, hi].
fn clip(a: f64, b: f64, wa: f64, wb: f64, lo: f64, hi: f64) -> Option<(f64, f64, f64, f64)> {
    let (c, d) = (a.max(lo), b.min(hi));
    if d <= c {
        return None;
    }
    let at = |t: f64| if b > a { wa + (wb - wa) * (t - a) / (b - a) } else { wa };
    Some((c, d, at(c), at(d)))
}

fn beam(m: MemberId, info: &MemberInfo, sup: &[SupportPt], l: &Loading, ts: &[f64]) -> (Diagram, Vec<Reaction>) {
    let cl = clusters(sup);
    let (pts, lines) = vertical_loads(l);
    let mut react = vec![0.0; sup.len()];
    let mut d = Diagram::zeros(ts.to_vec());
    let give = |ci: usize, f: f64, react: &mut Vec<f64>| {
        let idx = &cl[ci].1;
        for &i in idx {
            react[i] += f / idx.len() as f64;
        }
    };
    if cl.len() == 1 {
        // A single support: everything goes into it (bending not computed).
        let total: f64 = pts.iter().map(|p| p.1).sum::<f64>() + lines.iter().map(|&(a, b, wa, wb)| resultant(a, b, wa, wb).0).sum::<f64>();
        give(0, total, &mut react);
    } else {
        // Span k runs between clusters k and k+1; overhangs join the end spans.
        let nspan = cl.len() - 1;
        let bound = |k: usize| -> (f64, f64) {
            let lo = if k == 0 { f64::NEG_INFINITY } else { cl[k].0 };
            let hi = if k + 1 == nspan { f64::INFINITY } else { cl[k + 1].0 };
            (lo, hi)
        };
        for k in 0..nspan {
            let (lo, hi) = bound(k);
            let (cl0, cl1) = (cl[k].0, cl[k + 1].0);
            let span = cl1 - cl0;
            // Loads in this span group: (position, downward force).
            let mut forces: Vec<(f64, f64)> = vec![];
            for &(t, f) in &pts {
                let at_support = cl.iter().position(|c| (c.0 - t).abs() < CLUSTER);
                match at_support {
                    // Over a support: straight into it (counted once, in the span to its right).
                    Some(s) if s == k || (s == k + 1 && k + 1 == nspan) => give(s, f, &mut react),
                    Some(_) => {}
                    None if t >= lo && t < hi => forces.push((t, f)),
                    None => {}
                }
            }
            let mut pieces = vec![];
            for &(a, b, wa, wb) in &lines {
                if let Some(p) = clip(a, b, wa, wb, lo.max(0.0), hi.min(info.length)) {
                    pieces.push(p);
                    let (r, x) = resultant(p.0, p.1, p.2, p.3);
                    forces.push((x, r));
                }
            }
            let (mut r0, mut r1) = (0.0, 0.0);
            for &(t, f) in &forces {
                r0 += f * (cl1 - t) / span;
                r1 += f * (t - cl0) / span;
            }
            give(k, r0, &mut react);
            give(k + 1, r1, &mut react);
            // Between two points of a continuous bed (face-to-face contact) the
            // member is fully supported: load passes straight through and
            // there is no bending of its own.
            let bed = |c: usize| cl[c].1.iter().all(|&i| sup[i].node.is_none());
            if bed(k) && bed(k + 1) {
                continue;
            }
            // Internal forces at samples in this group (by free body from the
            // group's left end): shear = upward forces to the left.
            let pts_here: Vec<(f64, f64)> = pts.iter().copied().filter(|&(t, _)| t >= lo && t < hi && !cl.iter().any(|c| (c.0 - t).abs() < CLUSTER)).collect();
            let idx: Vec<usize> = (0..ts.len()).filter(|&i| ts[i] >= lo.max(0.0) && (ts[i] < hi || (k + 1 == nspan && ts[i] <= info.length))).collect();
            for &i in &idx {
                let x = ts[i];
                let (mut v, mut mm) = (0.0, 0.0);
                for (t, f) in [(cl0, r0), (cl1, r1)] {
                    if t <= x {
                        v += f;
                        mm += f * (x - t);
                    }
                }
                for &(t, f) in &pts_here {
                    if t <= x {
                        v -= f;
                        mm -= f * (x - t);
                    }
                }
                for &(a, b, wa, wb) in &pieces {
                    if let Some((c, e, wc, we)) = clip(a, b, wa, wb, a, x) {
                        let (r, cx) = resultant(c, e, wc, we);
                        v -= r;
                        mm -= r * (x - cx);
                    }
                }
                d.v[i] = v * info.cos;
                d.m[i] = mm * info.cos;
            }
            // Deflection: δ'' = −M/EI, zero at the two supports.
            if info.ei > 0.0 && idx.len() > 1 {
                let (mut slope, mut defl) = (vec![0.0; idx.len()], vec![0.0; idx.len()]);
                for w in 1..idx.len() {
                    let h = ts[idx[w]] - ts[idx[w - 1]];
                    slope[w] = slope[w - 1] - h * (d.m[idx[w]] + d.m[idx[w - 1]]) / (2.0 * info.ei);
                    defl[w] = defl[w - 1] + h * (slope[w] + slope[w - 1]) / 2.0;
                }
                let at = |t: f64| -> f64 {
                    let w = idx.iter().position(|&i| ts[i] >= t - EPS / 10.0).unwrap_or(idx.len() - 1);
                    defl[w]
                };
                let (y0, y1) = (at(cl0), at(cl1));
                for (w, &i) in idx.iter().enumerate() {
                    d.y[i] = defl[w] - (y0 + (y1 - y0) * (ts[i] - cl0) / span);
                }
            }
        }
    }
    let rs = sup
        .iter()
        .zip(react)
        .map(|(s, f)| Reaction { member: m, by: s.by, node: s.node, at: s.at, force: Vec3::Z * f, transfer: s.transfer })
        .collect();
    (d, rs)
}

fn column(model: &Model, m: MemberId, info: &MemberInfo, sup: &[SupportPt], l: &Loading, ts: &[f64]) -> (Diagram, Vec<Reaction>) {
    let (a, x, _) = axis(model, m);
    let z = |t: f64| (a + x * t).z;
    let (pts, lines) = vertical_loads(l);
    let mut d = Diagram::zeros(ts.to_vec());
    for (i, &t) in ts.iter().enumerate() {
        let mut above = 0.0;
        for &(p, f) in &pts {
            if z(p) > z(t) + EPS {
                above += f;
            }
        }
        for &(s0, s1, wa, wb) in &lines {
            // Portion of the line load above the section.
            let (lo, hi) = if x.z > 0.0 { (t, info.length) } else { (0.0, t) };
            if let Some(p) = clip(s0, s1, wa, wb, lo, hi) {
                above += resultant(p.0, p.1, p.2, p.3).0;
            }
        }
        d.n[i] = -above;
    }
    let total: f64 = pts.iter().map(|p| p.1).sum::<f64>() + lines.iter().map(|&(s0, s1, wa, wb)| resultant(s0, s1, wa, wb).0).sum::<f64>();
    // Everything to the lowest support(s).
    let zmin = sup.iter().map(|s| s.at.z).fold(f64::INFINITY, f64::min);
    let base: Vec<usize> = (0..sup.len()).filter(|&i| sup[i].at.z < zmin + CLUSTER).collect();
    let rs = sup
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let f = if base.contains(&i) { total / base.len() as f64 } else { 0.0 };
            Reaction { member: m, by: s.by, node: s.node, at: s.at, force: Vec3::Z * f, transfer: s.transfer }
        })
        .collect();
    (d, rs)
}

type TrussResult = (Vec<Vec<(MemberId, Diagram)>>, Vec<Vec<Reaction>>);

/// Solves a truss (all load cases) as a plane frame in its own plane.
fn truss(model: &Model, geom: &Geometry, ms: &[MemberId], sup: &[Vec<SupportPt>], loading: &[Vec<Loading>]) -> Result<TrussResult, String> {
    let g = model.members.iter().find(|m| m.id == ms[0]).and_then(|m| m.group).ok_or("truss member without group")?;
    let mut gid = g;
    while model.group(gid).kind != "truss" {
        gid = model.group(gid).parent.ok_or("no truss group")?;
    }
    let frame = model.group(gid).frame;
    let (origin, sx) = (frame.origin, frame.x);
    let normal = frame.y;
    let to2 = |p: Vec3| [(p - origin).dot(sx), p.z - origin.z];
    // Nodes and per-member rotations.
    let mut node_ix: HashMap<NodeId, usize> = HashMap::new();
    let mut nodes: Vec<[f64; 2]> = vec![];
    let mut elems: Vec<Elem> = vec![];
    // (member, segment start t, element index)
    let mut seg: Vec<(MemberId, f64, f64, usize)> = vec![];
    let mut n_rot = 0;
    for &m in ms {
        let path = &model.member(m).path;
        let mut rot: HashMap<NodeId, usize> = HashMap::new();
        for &n in path {
            node_ix.entry(n).or_insert_with(|| {
                nodes.push(to2(model.pos(n)));
                nodes.len() - 1
            });
            rot.insert(n, n_rot);
            n_rot += 1;
        }
        let mat = model.material(model.member(m).material);
        let a = model.section(model.member(m).section).props.area;
        let iz = i_about(model, m, geom, normal);
        for w in path.windows(2) {
            let (t0, t1) = (param(model, m, model.pos(w[0])), param(model, m, model.pos(w[1])));
            seg.push((m, t0, t1, elems.len()));
            elems.push(Elem { i: node_ix[&w[0]], j: node_ix[&w[1]], ri: rot[&w[0]], rj: rot[&w[1]], e: mat.e, a, iz, w: [0.0; 2], points: vec![] });
        }
    }
    // Bearings: supports from outside the truss.
    let mut bearing: Vec<(usize, &SupportPt, MemberId)> = vec![];
    for &m in ms {
        for s in &sup[m.idx()] {
            if s.by.is_none_or(|b| !ms.contains(&b)) {
                let n = match s.node.and_then(|n| node_ix.get(&n)) {
                    Some(&i) => i,
                    None => {
                        let p = to2(s.at);
                        (0..nodes.len()).min_by(|&a, &b| dist2(nodes[a], p).total_cmp(&dist2(nodes[b], p))).unwrap()
                    }
                };
                if !bearing.iter().any(|b| b.0 == n) {
                    bearing.push((n, s, m));
                }
            }
        }
    }
    if bearing.is_empty() {
        return Err("no bearings".into());
    }
    bearing.sort_by(|a, b| nodes[a.0][0].total_cmp(&nodes[b.0][0]));
    let mut fixed = vec![(bearing[0].0, 0)];
    fixed.extend(bearing.iter().map(|b| (b.0, 1)));

    let nc = loading.len();
    // A point load at a joint belongs to the segment starting there.
    let on_segment = |t: f64, t0: f64, t1: f64, m: MemberId| t >= t0 - EPS && t <= t1 + EPS && !(t > t1 - EPS && seg.iter().any(|s| s.0 == m && (s.1 - t).abs() < EPS));
    // Sample positions per element: the same for every case, so cases superpose.
    let samples: Vec<Vec<f64>> = seg
        .iter()
        .map(|&(m, t0, t1, _)| {
            let len = t1 - t0;
            let mut xs: Vec<f64> = (0..=40).map(|i| len * i as f64 / 40.0).collect();
            for l in loading.iter().map(|c| &c[m.idx()]) {
                for &(t, _, _) in &l.points {
                    if on_segment(t, t0, t1, m) {
                        xs.extend([t - t0 - EPS, t - t0 + EPS].iter().filter(|x| (0.0..=len).contains(*x)));
                    }
                }
            }
            xs.sort_by(f64::total_cmp);
            xs.dedup_by(|a, b| (*a - *b).abs() < EPS / 10.0);
            xs
        })
        .collect();
    let mut ds: Vec<Vec<(MemberId, Diagram)>> = vec![vec![]; nc];
    let mut rs: Vec<Vec<Reaction>> = vec![vec![]; nc];
    for ci in 0..nc {
        let mut f = Frame2d { nodes: nodes.clone(), n_rot, elems: elems.clone(), loads: vec![[0.0; 2]; nodes.len()], fixed: fixed.clone() };
        for &(m, t0, t1, k) in &seg {
            let l = &loading[ci][m.idx()];
            let len = t1 - t0;
            for &(a, b, w0, w1) in &l.lines {
                // Averaged over the element (loads here are uniform per member).
                let avg = |c0: f64, c1: f64| clip(a, b, c0, c1, t0, t1).map(|(c, e, u, v)| (u + v) / 2.0 * (e - c) / len).unwrap_or(0.0);
                f.elems[k].w[0] += avg(w0.dot(sx), w1.dot(sx));
                f.elems[k].w[1] += avg(w0.z, w1.z);
            }
            for &(t, force, _) in &l.points {
                if on_segment(t, t0, t1, m) {
                    f.elems[k].points.push(((t - t0).clamp(0.0, len), [force.dot(sx), force.z]));
                }
            }
        }
        let sol = f.solve()?;
        for &m in ms {
            let mut d = Diagram::default();
            for (si, &(_, t0, _, k)) in seg.iter().enumerate().filter(|s| s.1 .0 == m) {
                for &x in &samples[si] {
                    let (nn, v, mm, disp) = f.internal(&sol, k, x);
                    d.t.push(t0 + x);
                    d.n.push(nn);
                    d.v.push(v);
                    d.m.push(mm);
                    // Downward global component of the transverse displacement.
                    let ex = sol.elems[k].ex;
                    d.y.push(-(disp * ex[0]));
                }
            }
            ds[ci].push((m, d));
        }
        for (k, &(node, axis2)) in fixed.iter().enumerate() {
            if axis2 != 1 {
                continue;
            }
            let (_, s, m) = bearing.iter().find(|b| b.0 == node).unwrap();
            let r = sol.reactions[k];
            // Horizontal reaction (at the pinned bearing) is reported with it.
            let h = fixed.iter().position(|&(n2, a2)| n2 == node && a2 == 0).map(|i| sol.reactions[i]).unwrap_or(0.0);
            rs[ci].push(Reaction { member: *m, by: s.by, node: s.node, at: s.at, force: Vec3::Z * r + sx * h, transfer: s.transfer });
        }
    }
    Ok((ds, rs))
}

/// Linear interpolation of (v, m, y) at `t`.
fn interpolate(d: &Diagram, t: f64) -> (f64, f64, f64) {
    if d.t.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let k = d.t.partition_point(|&x| x < t).clamp(1, d.t.len() - 1);
    let (t0, t1) = (d.t[k - 1], d.t[k]);
    let f = if t1 > t0 { ((t - t0) / (t1 - t0)).clamp(0.0, 1.0) } else { 0.0 };
    let lerp = |v: &[f64]| v[k - 1] + (v[k] - v[k - 1]) * f;
    (lerp(&d.v), lerp(&d.m), lerp(&d.y))
}

fn dist2(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)
}

impl Takedown {
    /// Superposes load cases: `factors` per case.
    pub fn combine(&self, factors: &[(LoadCaseId, f64)]) -> (Vec<Diagram>, Vec<Reaction>) {
        let nm = self.info.len();
        let mut ds: Vec<Diagram> = (0..nm).map(|m| Diagram::zeros(self.diagrams.first().map(|c| c[m].t.clone()).unwrap_or_default())).collect();
        let mut rs: Vec<Reaction> = self.reactions.first().cloned().unwrap_or_default();
        for r in &mut rs {
            r.force = Vec3::ZERO;
        }
        for &(case, f) in factors {
            let Some(ci) = self.cases.iter().position(|c| *c == case) else { continue };
            for (d, c) in ds.iter_mut().zip(&self.diagrams[ci]) {
                if d.t.len() == c.t.len() {
                    d.add_scaled(c, f);
                }
            }
            for (r, s) in rs.iter_mut().zip(&self.reactions[ci]) {
                r.force += s.force * f;
            }
        }
        (ds, rs)
    }

    /// Foundation reactions (vertical, upward positive) for factors.
    pub fn foundation(&self, factors: &[(LoadCaseId, f64)]) -> Vec<Reaction> {
        self.combine(factors).1.into_iter().filter(|r| r.by.is_none()).collect()
    }
}
