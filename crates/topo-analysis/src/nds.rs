//! NDS (ASD) checks of sawn lumber members and nailed connections, from a
//! load takedown. Each check carries a calculation trace in US units with
//! the clause and table it rests on, so it reads like a hand calculation.
//!
//! Scope and assumptions (stated on each check where they apply):
//! * Dry service, normal temperature (C_M = C_t = 1), no incising.
//! * Beams: compression edge braced (C_L = 1); shear at the support (not
//!   reduced within d of it).
//! * Wall studs: weak axis braced by sheathing or blocking.
//! * Existing trusses: indicative only (plates and manufacturer design not
//!   checked).

use crate::code::{asce7_asd, CalcStep, CalcTrace, Check, Combination};
use crate::loadpath::Transfer;
use crate::takedown::{Behaviour, Diagram, MemberInfo, Takedown};
use serde::Serialize;
use topo_core::{FastenMethod, Fastener, Issue, LoadKind, MemberId, Model, Severity, Vec3};
use topo_data::{LumberValues, Source};

const IN: f64 = 0.0254;
const LB: f64 = 4.448_221_615_260_5;

fn lb(n: f64) -> f64 {
    n / LB
}
fn inch(m: f64) -> f64 {
    m / IN
}

/// The result of checking a model.
#[derive(Clone, Debug, Serialize)]
pub struct Analysis {
    pub combos: Vec<Combination>,
    pub checks: Vec<Check>,
    /// Per member: index of its governing check (highest ratio).
    pub governing: Vec<Option<usize>>,
    pub issues: Vec<Issue>,
}

impl Analysis {
    /// Highest ratio for a member (0 if unchecked).
    pub fn utilization(&self, m: MemberId) -> f64 {
        self.governing[m.idx()].map(|i| self.checks[i].ratio()).unwrap_or(0.0)
    }
}

fn load_key(k: LoadKind) -> &'static str {
    match k {
        LoadKind::Dead => "dead",
        LoadKind::Live => "live",
        LoadKind::RoofLive => "roof_live",
        LoadKind::Snow => "snow",
        LoadKind::Wind => "wind",
        LoadKind::Seismic => "seismic",
        LoadKind::Other => "live",
    }
}

/// C_D for a combination: the shortest-duration load in it (NDS 2.3.2).
fn load_duration(model: &Model, combo: &Combination) -> (f64, String, bool) {
    combo
        .factors
        .iter()
        .filter(|(_, f)| *f != 0.0)
        .filter_map(|(id, _)| topo_data::load_duration(load_key(model.load_cases[id.idx()].kind)))
        .map(|r| (r.cd, format!("{} ({})", r.load.replace('_', " "), r.duration), r.verified))
        .fold((0.0, String::new(), true), |a, b| if b.0 > a.0 { (b.0, b.1, a.2 && b.2) } else { (a.0, a.1, a.2 && b.2) })
}

struct Wood<'a> {
    v: &'a LumberValues,
    cite: &'a Source,
    /// Nominal (thickness, width) of one ply.
    nominal: Option<(u32, u32)>,
}

fn wood(model: &Model, m: MemberId) -> Option<Wood<'static>> {
    let mat = model.material(model.member(m).material);
    let key = mat.design_key.as_deref()?;
    let mut parts = key.split(':');
    if parts.next()? != "NDS" {
        return None;
    }
    let (species, grade) = (parts.next()?, parts.next()?);
    let (v, source) = topo_data::lumber_with_source(species, grade)?;
    let sec = model.section(model.member(m).section);
    let nominal = sec.tags.get("nominal").and_then(|n| {
        let (t, w) = n.split_once('x')?;
        Some((t.trim().parse().ok()?, w.trim().parse().ok()?))
    });
    Some(Wood { v, cite: source, nominal })
}

struct Trace {
    steps: Vec<CalcStep>,
}

impl Trace {
    fn new() -> Trace {
        Trace { steps: vec![] }
    }
    fn s(&mut self, symbol: &str, formula: &str, substituted: String, value: f64, unit: &str, reference: Option<&str>) -> f64 {
        self.steps.push(CalcStep { symbol: symbol.into(), formula: formula.into(), substituted, value, unit: unit.into(), reference: reference.map(Into::into) });
        value
    }
    fn given(&mut self, symbol: &str, what: &str, value: f64, unit: &str, reference: Option<&str>) -> f64 {
        self.s(symbol, what, fmt(value), value, unit, reference)
    }
    fn done(self) -> CalcTrace {
        CalcTrace { steps: self.steps }
    }
}

fn fmt(v: f64) -> String {
    let a = v.abs();
    if a >= 1000.0 {
        format!("{v:.0}")
    } else if a >= 10.0 {
        format!("{v:.1}")
    } else if a >= 1.0 {
        format!("{v:.2}")
    } else {
        format!("{v:.3}")
    }
}

/// Dressed depth (in) of a timber of nominal thickness × width.
fn topo_timber_depth(t: u32, w: u32) -> f64 {
    if t.min(w) >= 5 { w as f64 - 0.5 } else { w as f64 }
}

const REPETITIVE: [&str; 6] = ["joist", "rafter", "top_chord", "bottom_chord", "stud", "king_stud"];

/// Size factor for the property column `which` (`fb`, `ft`, `fc`).
fn cf(w: &Wood, which: &str) -> (f64, String, bool) {
    if topo_data::is_timber_grade(&w.v.grade) {
        // Table 4D: C_F = (12/d)^(1/9) on F_b for depths over 12"; 1.0 otherwise.
        let d = w.nominal.map(|(t, wd)| topo_timber_depth(t, wd)).unwrap_or(0.0);
        let v = if which == "fb" && d > 12.0 { (12.0 / d).powf(1.0 / 9.0) } else { 1.0 };
        return (v, format!("timber, d = {d:.1}\", NDS Supplement (2018) Table 4D"), true);
    }
    match w.nominal.and_then(|(t, wd)| topo_data::size_factor(&w.v.grade, t, wd).map(|r| (r, t, wd))) {
        Some((r, t, wd)) => {
            let v = match which {
                "fb" => r.fb,
                "ft" => r.ft,
                _ => r.fc,
            };
            (v, format!("{t}x{wd}, {}", topo_data::nds_size_factors().source.cite()), r.verified)
        }
        None => (1.0, "size unknown; taken as 1.0".into(), false),
    }
}

struct Ctx<'a> {
    model: &'a Model,
    /// Reactions for this combination.
    rs: &'a [crate::takedown::Reaction],
    combo: &'a Combination,
    cd: f64,
    cd_why: String,
    cd_verified: bool,
}

impl Ctx<'_> {
    #[allow(clippy::too_many_arguments)]
    fn check(&self, m: MemberId, title: &str, clause: &str, demand: f64, capacity: f64, unit: &str, trace: Trace) -> Check {
        Check {
            member: m,
            combination: self.combo.name.clone(),
            title: title.into(),
            clause: clause.into(),
            demand,
            capacity,
            unit: unit.into(),
            other: None,
            at: None,
            notes: vec![],
            verified: false,
            indicative: false,
            trace: trace.done(),
        }
    }

    fn ref_values(&self, t: &mut Trace, w: &Wood, sym: &str, v: f64) -> f64 {
        let cite = format!("{}, {} {}{}", w.cite.cite(), w.v.species, w.v.grade, if w.v.verified { "" } else { " — unverified" });
        t.given(sym, "reference design value", v, "psi", Some(&cite))
    }

    fn cd_step(&self, t: &mut Trace) -> f64 {
        let src = &topo_data::nds_load_duration().source;
        t.given("C_D", &format!("load duration: {}", self.cd_why), self.cd, "", Some(&format!("{}{}", src.cite(), if self.cd_verified { "" } else { " — unverified" })))
    }
}

/// Section modulus and area for bending in the vertical plane (in³, in²),
/// with a description of the orientation.
fn bending_props(model: &Model, m: MemberId, geom_v: Vec3) -> (f64, f64, f64, bool) {
    let p = &model.section(model.member(m).section).props;
    // Depth (v) vertical: bending about u.
    let on_edge = geom_v.z.abs() >= 0.5;
    let s = if on_edge { p.s_u } else { p.s_v };
    let depth = if on_edge { p.depth } else { p.width };
    (s / IN.powi(3), p.area / (IN * IN), inch(depth), on_edge)
}

fn beam_checks(cx: &Ctx, m: MemberId, info: &MemberInfo, d: &Diagram, w: &Wood, v_axis: Vec3, out: &mut Vec<Check>) {
    let model = cx.model;
    let role = model.member(m).role.as_str();
    let (s_in3, a_in2, depth_in, on_edge) = bending_props(model, m, v_axis);
    let data_ok = w.v.verified && cx.cd_verified;
    // Bending.
    let (mmax, im) = Diagram::max_abs(&d.m);
    let mut t = Trace::new();
    let m_lbin = t.s("M", "maximum bending moment (takedown)", format!("{} lb·ft", fmt(lb(mmax) / IN / 12.0)), lb(mmax) / IN, "lb·in", None);
    let s_ = t.given("S", if on_edge { "section modulus (on edge)" } else { "section modulus (flat)" }, s_in3, "in³", None);
    let fb = t.s("f_b", "M / S", format!("{} / {}", fmt(m_lbin), fmt(s_)), m_lbin / s_, "psi", Some("NDS 3.3.1"));
    let fb_ref = cx.ref_values(&mut t, w, "F_b", w.v.fb);
    let cdv = cx.cd_step(&mut t);
    let (cfv, cf_why, cf_ok) = cf(w, "fb");
    t.given("C_F", &format!("size factor ({cf_why})"), cfv, "", Some("NDS 4.3.6"));
    let rep = REPETITIVE.contains(&role);
    let cr = t.given("C_r", if rep { "repetitive member (≤ 24\" o.c., ≥ 3, load-distributing element)" } else { "not a repetitive member" }, if rep { 1.15 } else { 1.0 }, "", Some("NDS 4.3.9"));
    let cl = t.given("C_L", "beam stability: compression edge braced (assumed)", 1.0, "", Some("NDS 3.3.3"));
    let (cfu, cfu_ok) = if on_edge {
        (1.0, true)
    } else {
        match w.nominal.and_then(|(th, wd)| topo_data::flat_use(th, wd)) {
            Some(r) => (t.given("C_fu", "flat use (bending about the weak axis)", r.cfu, "", Some(&topo_data::nds_flat_use().source.cite())), r.verified),
            None => (1.0, false),
        }
    };
    let fbp = t.s("F'_b", "F_b · C_D · C_M · C_t · C_L · C_F · C_fu · C_i · C_r", format!("{} · {} · 1 · 1 · {} · {} · {} · 1 · {}", fmt(fb_ref), cdv, cl, cfv, cfu, cr), fb_ref * cdv * cl * cfv * cfu * cr, "psi", Some("NDS Table 4.3.1"));
    let mut c = cx.check(m, "Bending", "NDS 3.3", fb, fbp, "psi", t);
    c.at = d.t.get(im).copied();
    c.verified = data_ok && cf_ok && cfu_ok;
    c.notes.push("Compression edge assumed braced (C_L = 1).".into());
    out.push(c);

    // Shear.
    let (vmax, iv) = Diagram::max_abs(&d.v);
    let mut t = Trace::new();
    let v_lb = t.given("V", "maximum shear (takedown, at the support face not reduced)", lb(vmax), "lb", None);
    let a = t.given("A", "area", a_in2, "in²", None);
    let fv = t.s("f_v", "3V / 2A", format!("3 · {} / (2 · {})", fmt(v_lb), fmt(a)), 1.5 * v_lb / a, "psi", Some("NDS 3.4.2"));
    let fv_ref = cx.ref_values(&mut t, w, "F_v", w.v.fv);
    let cdv = cx.cd_step(&mut t);
    let fvp = t.s("F'_v", "F_v · C_D · C_M · C_t · C_i", format!("{} · {}", fmt(fv_ref), cdv), fv_ref * cdv, "psi", Some("NDS Table 4.3.1"));
    let mut c = cx.check(m, "Shear", "NDS 3.4", fv, fvp, "psi", t);
    c.at = d.t.get(iv).copied();
    c.verified = data_ok;
    out.push(c);

    // Deflection (total load of this combination). A member riding on
    // another deflects with it; that one is checked.
    let (ymax, iy) = Diagram::max_abs(&d.y);
    if let Some(span) = span_at(info, d.t.get(iy).copied().unwrap_or(0.0)).filter(|_| info.rides_on.is_none()) {
        let mut t = Trace::new();
        let delta = t.given("Δ", "maximum deflection (takedown, integrating M/EI)", inch(ymax), "in", None);
        let l = t.given("L", "span (support to support; twice the overhang for a cantilever)", inch(span), "in", None);
        let limit = t.s("Δ_allow", "L / 240", format!("{} / 240", fmt(l)), l / 240.0, "in", Some("IRC Table R301.7"));
        let mut c = cx.check(m, "Deflection (total)", "IRC R301.7", delta, limit, "in", t);
        c.at = d.t.get(iy).copied();
        c.verified = true;
        c.notes.push(format!("Depth {depth_in:.2}\"; E = reference E (no creep factor)."));
        out.push(c);
    }

    // Bearing on each support (perpendicular to grain).
    for sp in &info.supports {
        let Some(by) = sp.by else { continue };
        if sp.node.is_none() || sp.bearing <= 0.0 || sp.transfer != Transfer::Bearing {
            continue;
        }
        bearing_check(cx, m, by, sp.at, sp.bearing, member_width(model, m, v_axis), out);
    }
}

fn member_width(model: &Model, m: MemberId, v_axis: Vec3) -> f64 {
    let p = &model.section(model.member(m).section).props;
    if v_axis.z.abs() >= 0.5 { p.width } else { p.depth }
}

/// Span containing `t` (for deflection limits); a cantilever counts twice its length.
fn span_at(info: &MemberInfo, t: f64) -> Option<f64> {
    let ts: Vec<f64> = {
        let mut v: Vec<f64> = info.supports.iter().map(|s| s.t).collect();
        v.dedup_by(|a, b| (*a - *b).abs() < 0.005);
        v
    };
    if ts.len() < 2 {
        return None;
    }
    if t < ts[0] {
        return Some(2.0 * (ts[0] - t));
    }
    if t > *ts.last().unwrap() {
        return Some(2.0 * (t - ts.last().unwrap()));
    }
    ts.windows(2).find(|w| t >= w[0] && t <= w[1]).map(|w| w[1] - w[0])
}

fn reaction_at(cx: &Ctx, m: MemberId, by: MemberId, at: Vec3) -> f64 {
    cx.rs.iter().filter(|r| r.member == m && r.by == Some(by) && r.at.distance(at) < 1e-6).map(|r| r.force.z).sum()
}

fn bearing_check(cx: &Ctx, m: MemberId, by: MemberId, at: Vec3, length: f64, width: f64, out: &mut Vec<Check>) {
    let model = cx.model;
    let (Some(wm), Some(wb)) = (wood(model, m), wood(model, by)) else { return };
    let r = reaction_at(cx, m, by, at);
    if r <= 0.0 {
        return;
    }
    let mut t = Trace::new();
    let rl = t.given("R", &format!("reaction on {}", model.member_path(by)), lb(r), "lb", None);
    let lbv = t.given("l_b", "bearing length along the member", inch(length), "in", None);
    let b = t.given("b", "bearing width", inch(width), "in", None);
    let fc = t.s("f_c⊥", "R / (l_b · b)", format!("{} / ({} · {})", fmt(rl), fmt(lbv), fmt(b)), rl / (lbv * b), "psi", Some("NDS 3.10.2"));
    // The weaker of the two members in side-grain bearing.
    let weaker = if wb.v.fc_perp < wm.v.fc_perp { &wb } else { &wm };
    let fref = cx.ref_values(&mut t, weaker, "F_c⊥", weaker.v.fc_perp);
    let cb = t.given("C_b", "bearing area factor (conservatively 1.0)", 1.0, "", Some("NDS 3.10.4"));
    let fcp = t.s("F'_c⊥", "F_c⊥ · C_M · C_t · C_i · C_b", format!("{} · {}", fmt(fref), cb), fref * cb, "psi", Some("NDS Table 4.3.1"));
    let mut c = cx.check(m, "Bearing (perpendicular to grain)", "NDS 3.10.2", fc, fcp, "psi", t);
    c.other = Some(by);
    c.verified = wm.v.verified && wb.v.verified;
    c.notes.push("Checked against the weaker of the two members; C_D does not apply to F_c⊥.".into());
    out.push(c);
}

fn column_checks(cx: &Ctx, m: MemberId, info: &MemberInfo, d: &Diagram, w: &Wood, braced_weak: bool, out: &mut Vec<Check>) {
    let model = cx.model;
    let p = &model.section(model.member(m).section).props;
    let (nmin, i) = d.n.iter().enumerate().fold((0.0f64, 0), |(a, k), (j, &x)| if x < a { (x, j) } else { (a, k) });
    if nmin >= 0.0 {
        return;
    }
    let data_ok = w.v.verified && cx.cd_verified;
    let mut t = Trace::new();
    let pc = t.given("P", "axial compression (takedown)", lb(-nmin), "lb", None);
    let a = t.given("A", "area", p.area / (IN * IN), "in²", None);
    let fc = t.s("f_c", "P / A", format!("{} / {}", fmt(pc), fmt(a)), pc / a, "psi", Some("NDS 3.6.3"));
    let fc_ref = cx.ref_values(&mut t, w, "F_c", w.v.fc);
    let emin = cx.ref_values(&mut t, w, "E_min", w.v.e_min);
    let cdv = cx.cd_step(&mut t);
    let (cfv, cf_why, cf_ok) = cf(w, "fc");
    t.given("C_F", &format!("size factor ({cf_why})"), cfv, "", Some("NDS 4.3.6"));
    let fcs = t.s("F_c*", "F_c · C_D · C_M · C_t · C_F · C_i", format!("{} · {} · {}", fmt(fc_ref), cdv, cfv), fc_ref * cdv * cfv, "psi", Some("NDS 3.7.1"));
    let le = t.given("l_e", "effective length (K_e = 1.0, member length)", inch(info.length), "in", Some("NDS 3.7.1.2"));
    // Slenderness about each axis; the weak axis may be braced.
    let (d1, d2) = (inch(p.depth), inch(p.width));
    let (dmax, dmin) = (d1.max(d2), d1.min(d2));
    let ratio = if braced_weak {
        t.s("l_e/d", "l_e / d (strong axis; weak axis braced by sheathing — assumed)", format!("{} / {}", fmt(le), fmt(dmax)), le / dmax, "", Some("NDS 3.7.1.3"))
    } else {
        t.s("l_e/d", "l_e / d (least dimension; unbraced)", format!("{} / {}", fmt(le), fmt(dmin)), le / dmin, "", Some("NDS 3.7.1.3"))
    };
    let fce = t.s("F_cE", "0.822 · E'_min / (l_e/d)²", format!("0.822 · {} / {}²", fmt(emin), fmt(ratio)), 0.822 * emin / ratio.powi(2), "psi", Some("NDS 3.7.1"));
    let c_ = 0.8;
    let r = fce / fcs;
    let k = (1.0 + r) / (2.0 * c_);
    let cp = t.s("C_P", "(1 + F_cE/F_c*)/2c − √[((1 + F_cE/F_c*)/2c)² − (F_cE/F_c*)/c], c = 0.8", format!("F_cE/F_c* = {}", fmt(r)), k - (k * k - r / c_).sqrt(), "", Some("NDS Eq. 3.7-1"));
    let fcp = t.s("F'_c", "F_c* · C_P", format!("{} · {}", fmt(fcs), fmt(cp)), fcs * cp, "psi", Some("NDS Table 4.3.1"));
    let mut c = cx.check(m, "Compression (column stability)", "NDS 3.7", fc, fcp, "psi", t);
    c.at = d.t.get(i).copied();
    c.verified = data_ok && cf_ok;
    if braced_weak {
        c.notes.push("Weak axis assumed braced by wall sheathing or blocking.".into());
    }
    if ratio > 50.0 {
        c.notes.push(format!("l_e/d = {ratio:.0} exceeds 50 (NDS 3.7.1.4)."));
        c.capacity = 0.0;
    }
    out.push(c);

    // End bearing of the column on what it stands on.
    for sp in &info.supports {
        let Some(by) = sp.by else { continue };
        if sp.transfer != Transfer::Bearing {
            continue;
        }
        let Some(wb) = wood(model, by) else { continue };
        let r = reaction_at(cx, m, by, sp.at);
        if r <= 0.0 {
            continue;
        }
        let mut t = Trace::new();
        let rl = t.given("R", &format!("load onto {}", model.member_path(by)), lb(r), "lb", None);
        let a = t.given("A", "contact area (column section)", p.area / (IN * IN), "in²", None);
        let fcb = t.s("f_c⊥", "R / A", format!("{} / {}", fmt(rl), fmt(a)), rl / a, "psi", Some("NDS 3.10.2"));
        let fref = cx.ref_values(&mut t, &wb, "F_c⊥", wb.v.fc_perp);
        let lbv = inch(p.width.min(p.depth));
        let cb = t.s("C_b", "(l_b + 0.375) / l_b, l_b < 6\" (bearing ≥ 3\" from the plate end, assumed)", format!("({} + 0.375) / {}", fmt(lbv), fmt(lbv)), if lbv < 6.0 { (lbv + 0.375) / lbv } else { 1.0 }, "", Some("NDS 3.10.4"));
        let fcp = t.s("F'_c⊥", "F_c⊥ · C_b", format!("{} · {}", fmt(fref), fmt(cb)), fref * cb, "psi", Some("NDS Table 4.3.1"));
        let mut c = cx.check(m, "Bearing on plate (perpendicular to grain)", "NDS 3.10.2", fcb, fcp, "psi", t);
        c.other = Some(by);
        c.verified = wb.v.verified;
        out.push(c);
    }
}

fn truss_checks(cx: &Ctx, m: MemberId, info: &MemberInfo, d: &Diagram, w: &Wood, out: &mut Vec<Check>) {
    let model = cx.model;
    let p = &model.section(model.member(m).section).props;
    let role = model.member(m).role.as_str();
    let a_in2 = p.area / (IN * IN);
    let s_in3 = p.s_u / IN.powi(3);
    let (cdv0, data_ok) = (cx.cd, w.v.verified && cx.cd_verified);
    // Governing point: largest combined ratio along the member.
    let panel = {
        let path = &model.member(m).path;
        path.windows(2).map(|w| model.pos(w[0]).distance(model.pos(w[1]))).fold(0.0, f64::max)
    };
    let (cfb, _, ok_b) = cf(w, "fb");
    let (cfc, _, ok_c) = cf(w, "fc");
    let (cft, _, ok_t) = cf(w, "ft");
    let rep = REPETITIVE.contains(&role);
    let fbp = w.v.fb * cdv0 * cfb * if rep { 1.15 } else { 1.0 };
    let fcs = w.v.fc * cdv0 * cfc;
    let ftp = w.v.ft * cdv0 * cft;
    // In-plane buckling over the longest panel on edge; out of plane braced
    // for chords (sheathing / ceiling), unbraced for webs.
    let (le, dd) = if role == "web" { (panel, inch(p.width.min(p.depth))) } else { (panel, inch(p.depth.max(p.width))) };
    let ratio = inch(le) / dd;
    let fce = 0.822 * w.v.e_min / ratio.powi(2);
    let r = fce / fcs;
    let k = (1.0 + r) / 1.6;
    let cp = k - (k * k - r / 0.8).sqrt();
    let fcp = fcs * cp;
    let mut worst = (0.0, 0usize, 0.0, 0.0, 0.0);
    for i in 0..d.t.len() {
        let n = lb(d.n[i]);
        let mb = lb(d.m[i].abs()) / IN;
        let fb = mb / s_in3;
        let f = n / a_in2;
        let u = if f < 0.0 { (-f / fcp).powi(2) + fb / (fbp * (1.0 - (-f / fce)).max(1e-6)) } else { f / ftp + fb / fbp };
        if u > worst.0 {
            worst = (u, i, f, fb, mb);
        }
    }
    let (u, i, f, fb, _) = worst;
    let mut t = Trace::new();
    t.given("f", if f < 0.0 { "axial compression stress at the governing point" } else { "axial tension stress at the governing point" }, f.abs(), "psi", None);
    t.given("f_b", "bending stress at the governing point", fb, "psi", None);
    cx.cd_step(&mut t);
    t.given("F'_b", "F_b · C_D · C_F · C_r", fbp, "psi", Some("NDS Table 4.3.1"));
    if f < 0.0 {
        t.given("l_e/d", if role == "web" { "longest panel / least dimension (web unbraced out of plane)" } else { "longest panel / depth (braced out of plane)" }, ratio, "", Some("NDS 3.7.1"));
        t.given("F'_c", "F_c · C_D · C_F · C_P", fcp, "psi", Some("NDS 3.7"));
        t.s("ratio", "(f_c/F'_c)² + f_b / [F'_b (1 − f_c/F_cE)]", format!("F_cE = {}", fmt(fce)), u, "", Some("NDS Eq. 3.9-3"));
    } else {
        t.given("F'_t", "F_t · C_D · C_F", ftp, "psi", Some("NDS Table 4.3.1"));
        t.s("ratio", "f_t/F'_t + f_b/F'_b", String::new(), u, "", Some("NDS Eq. 3.9-1"));
    }
    let mut c = cx.check(m, if f < 0.0 { "Truss member: compression + bending" } else { "Truss member: tension + bending" }, "NDS 3.9", u, 1.0, "", t);
    c.at = d.t.get(i).copied();
    c.verified = data_ok && ok_b && ok_c && ok_t;
    c.notes.push("Existing truss: indicative only. Truss plates and the manufacturer's design are not checked.".into());
    c.indicative = true;
    let _ = info;
    out.push(c);
}

/// Lateral design value per nail by the yield-limit equations (NDS 12.3.1),
/// single shear, both members of specific gravity `g`.
fn nail_z(d: f64, fyb: f64, ls: f64, lm: f64, g: f64, t: &mut Trace) -> f64 {
    let fe = t.s(
        "F_e",
        "16,600 · G^1.84 (D < 1/4\"), rounded to 50 psi as tabulated",
        format!("16,600 · {g}^1.84"),
        (16_600.0 * g.powf(1.84) / 50.0).round() * 50.0,
        "psi",
        Some("NDS Table 12.3.3 and footnote 2"),
    );
    let (fem, fes) = (fe, fe);
    let re = fem / fes;
    let rt = lm / ls;
    let rd = t.s("R_d", "K_D (D ≤ 0.17\": 2.2; else 10D + 0.5)", fmt(d), if d <= 0.17 { 2.2 } else { 10.0 * d + 0.5 }, "", Some("NDS Table 12.3.1B"));
    let k1 = ((re + 2.0 * re * re * (1.0 + rt + rt * rt) + rt * rt * re.powi(3)).sqrt() - re * (1.0 + rt)) / (1.0 + re);
    let k2 = -1.0 + (2.0 * (1.0 + re) + 2.0 * fyb * (1.0 + 2.0 * re) * d * d / (3.0 * fem * lm * lm)).sqrt();
    let k3 = -1.0 + (2.0 * (1.0 + re) / re + 2.0 * fyb * (2.0 + re) * d * d / (3.0 * fem * ls * ls)).sqrt();
    let modes = [
        ("I_m", d * lm * fem / rd),
        ("I_s", d * ls * fes / rd),
        ("II", k1 * d * ls * fes / rd),
        ("III_m", k2 * d * lm * fem / ((1.0 + 2.0 * re) * rd)),
        ("III_s", k3 * d * ls * fem / ((2.0 + re) * rd)),
        ("IV", d * d / rd * (2.0 * fem * fyb / (3.0 * (1.0 + re))).sqrt()),
    ];
    let (mode, z) = modes.iter().copied().fold(("", f64::INFINITY), |a, b| if b.1 < a.1 { b } else { a });
    let listing = modes.iter().map(|(n, z)| format!("{n} {}", fmt(*z))).collect::<Vec<_>>().join(", ");
    t.s("Z", &format!("least yield mode (governs: {mode})"), listing, z, "lb", Some("NDS Eq. 12.3-1 to 12.3-6"))
}

fn nail_checks(cx: &Ctx, out: &mut Vec<Check>, issues: &mut Vec<Issue>) {
    let model = cx.model;
    for r in cx.rs.iter().filter(|r| r.transfer == Transfer::Fastened) {
        let (Some(by), Some(node)) = (r.by, r.node) else { continue };
        let demand = r.force.norm();
        if demand <= 0.0 {
            continue;
        }
        let conn = model.joint(node).and_then(|j| {
            j.connections.iter().find(|c| (c.member == r.member && c.to == Some(by)) || (c.member == by && c.to == Some(r.member)))
        });
        let (Some(wm), Some(wb)) = (wood(model, r.member), wood(model, by)) else { continue };
        let mut t = Trace::new();
        let dl = t.given("R", &format!("load from {} into {} through fasteners", model.member_path(r.member), model.member_path(by)), lb(demand), "lb", None);
        let Some(conn) = conn.map(|c| model.connection(c.connection)) else {
            let mut c = cx.check(r.member, "Nailed connection", "NDS 12", dl, 0.0, "lb", t);
            c.other = Some(by);
            c.notes.push("No fasteners recorded for this connection.".into());
            out.push(c);
            continue;
        };
        let mut capacity = 0.0;
        let mut ok = cx.cd_verified;
        let mut notes = vec![];
        for gp in &conn.fasteners {
            let Fastener::Nail { designation, diameter, length } = &gp.fastener else { continue };
            let (d, l) = (inch(*diameter), inch(*length));
            let Some(nd) = topo_data::nails().rows.iter().find(|n| (n.d - d).abs() < 1e-3 && (n.l - l).abs() < 1e-3) else {
                notes.push(format!("{designation}: no nail data"));
                continue;
            };
            ok &= nd.verified && wm.v.verified && wb.v.verified;
            t.given("n", &format!("{} ({})", designation, gp.method.label()), gp.count as f64, "", conn.reference.as_deref());
            t.given("D", "shank diameter", d, "in", Some(&topo_data::nails().source.cite()));
            t.given("F_yb", "bending yield strength", nd.fyb, "psi", Some(&topo_data::nails().source.cite()));
            let side = 1.5;
            let (ls, lm) = match gp.method {
                FastenMethod::ToeNail => (l / 3.0, l * (30f64.to_radians()).cos() - l / 3.0),
                _ => (side, l - side),
            };
            t.given("l_s", "side member bearing length", ls, "in", None);
            t.given("l_m", "penetration into the main member", lm, "in", None);
            let g = wm.v.g.min(wb.v.g);
            let z = nail_z(d, nd.fyb, ls, lm, g, &mut t);
            let (factor, fname, fref) = match gp.method {
                FastenMethod::EndNail => (0.67, "C_eg (end grain)", "NDS 12.5.2"),
                FastenMethod::ToeNail => (0.83, "C_tn (toe-nail)", "NDS 12.5.4"),
                _ => (1.0, "no geometry factor", ""),
            };
            let f = t.given(fname, "", factor, "", Some(fref));
            let cdv = cx.cd_step(&mut t);
            let zp = t.s("Z'", "Z · C_D · C_M · C_t · C_g · C_Δ · C_eg · C_di · C_tn", format!("{} · {} · {}", fmt(z), cdv, f), z * cdv * f, "lb", Some("NDS Table 11.3.1"));
            capacity += t.s("n Z'", "", format!("{} · {}", gp.count, fmt(zp)), gp.count as f64 * zp, "lb", None);
            if lm < 6.0 * d {
                notes.push(format!("Penetration {lm:.2}\" is less than 6D = {:.2}\" (NDS 12.1.6.2).", 6.0 * d));
            }
            if gp.method == FastenMethod::EndNail {
                notes.push("Nails in end grain: NDS 12.5.2 applies C_eg = 0.67 (and prohibits end-grain withdrawal).".into());
            }
            if gp.method == FastenMethod::ToeNail {
                notes.push("Toe-nail geometry approximated (side length L/3, 30°).".into());
            }
        }
        let mut c = cx.check(r.member, &format!("Nailed connection: {}", conn.name), "NDS 12.3", dl, capacity, "lb", t);
        c.other = Some(by);
        c.verified = ok;
        c.notes = notes;
        c.notes.push("Gravity load carried by fasteners in shear, with no bearing path.".into());
        if !issues.iter().any(|i| i.code == "nail-only-support" && i.members.contains(&r.member)) {
            issues.push(Issue::new(Severity::Warning, "nail-only-support", format!("{} relies on nails in shear to carry gravity load", model.member_path(r.member))).members([r.member, by]));
        }
        out.push(c);
    }
}

fn is_wall_stud(model: &Model, m: MemberId) -> bool {
    let role = model.member(m).role.as_str();
    matches!(role, "stud" | "king_stud" | "jack_stud" | "cripple") && model.member(m).group.is_some_and(|g| model.group(g).kind == "wall")
}

/// Checks every member and nailed connection for every ASCE 7 ASD combination.
pub fn nds_asd(model: &Model, geom: &topo_geom::Geometry, td: &Takedown) -> Analysis {
    let combos = asce7_asd(&model.load_cases, &model.standards.asce7);
    let mut checks = vec![];
    let mut issues = vec![];
    let mut unknown = vec![false; model.members.len()];
    for combo in &combos {
        let (ds, rs) = td.combine(&combo.factors);
        let (cd, cd_why, cd_verified) = load_duration(model, combo);
        let cx = Ctx { model, rs: &rs, combo, cd, cd_why, cd_verified };
        for m in &model.members {
            let info = &td.info[m.id.idx()];
            let Some(w) = wood(model, m.id) else {
                if !unknown[m.id.idx()] {
                    unknown[m.id.idx()] = true;
                    issues.push(Issue::new(Severity::Info, "no-design-values", format!("{}: no NDS design values for its material; not checked", model.member_path(m.id))).members([m.id]));
                }
                continue;
            };
            let d = &ds[m.id.idx()];
            match info.behaviour {
                Behaviour::Beam => beam_checks(&cx, m.id, info, d, &w, geom.member(m.id).place.v, &mut checks),
                Behaviour::Column => column_checks(&cx, m.id, info, d, &w, is_wall_stud(model, m.id), &mut checks),
                Behaviour::Truss => truss_checks(&cx, m.id, info, d, &w, &mut checks),
                Behaviour::Unsupported => {}
            }
        }
        nail_checks(&cx, &mut checks, &mut issues);
    }
    // Serviceability: deflection under each variable load alone (IRC Table
    // R301.7: floors L/360 under live load; roofs L/240 under snow/roof live).
    for case in model.load_cases.iter().filter(|c| matches!(c.kind, LoadKind::Live | LoadKind::Snow | LoadKind::RoofLive)) {
        let (ds, _) = td.combine(&[(case.id, 1.0)]);
        let limit = if case.kind == LoadKind::Live { 360.0 } else { 240.0 };
        for m in &model.members {
            let info = &td.info[m.id.idx()];
            if info.behaviour != Behaviour::Beam || info.rides_on.is_some() {
                continue;
            }
            let d = &ds[m.id.idx()];
            let (ymax, iy) = Diagram::max_abs(&d.y);
            let Some(span) = span_at(info, d.t.get(iy).copied().unwrap_or(0.0)) else { continue };
            if ymax <= 0.0 {
                continue;
            }
            let mut t = Trace::new();
            let delta = t.given("Δ", &format!("maximum deflection under {} alone", case.name), inch(ymax), "in", None);
            let l = t.given("L", "span", inch(span), "in", None);
            let allow = t.s("Δ_allow", &format!("L / {limit}"), format!("{} / {limit}", fmt(l)), l / limit, "in", Some("IRC Table R301.7"));
            checks.push(Check {
                member: m.id,
                combination: case.name.clone(),
                title: format!("Deflection ({})", if case.kind == LoadKind::Live { "live" } else { "snow / roof live" }),
                clause: "IRC R301.7".into(),
                demand: delta,
                capacity: allow,
                unit: "in".into(),
                other: None,
                at: d.t.get(iy).copied(),
                notes: vec![],
                verified: true,
                indicative: false,
                trace: t.done(),
            });
        }
    }
    let mut governing: Vec<Option<usize>> = vec![None; model.members.len()];
    for (i, c) in checks.iter().enumerate() {
        let g = &mut governing[c.member.idx()];
        if g.is_none_or(|j| c.ratio() > checks[j].ratio()) {
            *g = Some(i);
        }
    }
    Analysis { combos, checks, governing, issues }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The yield-limit equations reproduce every value of NDS Table 12N we
    /// hold (8d/10d/16d common, 1-1/2" side member, four species groups),
    /// computed as the table is: main-member penetration 10D.
    #[test]
    fn nail_yield_limit_matches_table() {
        for row in &topo_data::nail_z().rows {
            let n = topo_data::nail(&row.kind, row.penny).unwrap();
            let mut t = Trace::new();
            let z = nail_z(n.d, n.fyb, row.side, 10.0 * n.d, row.g, &mut t);
            assert_eq!(z.round(), row.z, "{}d, G = {}: Z = {z:.1} vs table {}", row.penny, row.g, row.z);
        }
    }
}
