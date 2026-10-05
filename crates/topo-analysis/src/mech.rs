//! Checks by plain mechanics, for members whose material is not covered by
//! a design code here (metals, laminates): elastic stress against the
//! material's yield strength with a factor of safety, Euler buckling,
//! displacement against a stated limit, and joints against their rated
//! capacities. SI units (MPa, N, N·m, mm).
//!
//! Scope and assumptions (stated on each check where they apply):
//! * Static strength only: no fatigue, no local buckling of thin walls, no
//!   stress concentration at joints.
//! * Beside a joint made by a process that weakens the parent material (the
//!   heat-affected zone of a weld), the strength is the zone's.
//! * A laminate is checked as if isotropic, against one allowable stress.

use crate::code::Check;
use crate::nds::{fmt, Trace};
use crate::takedown::{Diagram, MemberInfo};
use topo_core::{MemberId, Model, Shape};
use topo_geom::Geometry;

const MPA: f64 = 1e6;
const MM: f64 = 1e-3;

/// What these checks rest on, for labels.
pub fn basis(model: &Model) -> String {
    format!("elastic stress against yield (plain mechanics, factor of safety {})", fmt(safety(model)))
}

fn safety(model: &Model) -> f64 {
    model.standards.safety_factor.unwrap_or(1.0)
}

/// A stretch of a member beside a joint where the material is weaker.
struct Zone {
    /// Along the member (m from its first node).
    from: f64,
    to: f64,
    /// Yield strength there (Pa).
    fy: f64,
    why: String,
    verified: bool,
}

/// The weakened stretches of member `m`: around every node where it is
/// joined by a process with zone data for its material.
fn zones(model: &Model, geom: &Geometry, m: MemberId) -> (Vec<Zone>, Vec<String>) {
    let mem = model.member(m);
    let mat = model.material(mem.material);
    let key = mat.design_key.as_deref().and_then(|k| k.strip_prefix("MECH:"));
    let start = model.pos(mem.start());
    let (mut out, mut notes) = (vec![], vec![]);
    for &n in &mem.path {
        let Some(j) = model.joint(n) else { continue };
        for c in j.connections.iter().filter(|c| c.member == m || c.to == Some(m)) {
            let conn = model.connection(c.connection);
            let Some(process) = conn.process.as_deref() else { continue };
            let Some(z) = key.and_then(|k| topo_data::joint_zone(process, k)) else {
                let note = format!("No data on how {} affects {} beside the joint: the parent strength is used there.", process.replace('_', " "), mat.name);
                if topo_data::weakens(process) && !notes.contains(&note) {
                    notes.push(note);
                }
                continue;
            };
            // The joint's faces are about the other member's half-width from the node.
            let other = if c.member == m { c.to } else { Some(c.member) };
            let reach = other.map(|o| geom.member(o).place.radius()).unwrap_or(0.0) + z.extent_mm * MM;
            let t = model.pos(n).distance(start);
            out.push(Zone {
                from: t - reach,
                to: t + reach,
                fy: z.fy_mpa * MPA,
                why: format!("within {} mm of the {} joint ({}){}", fmt(z.extent_mm), process.replace('_', " "), z.source, if z.verified { "" } else { " — unverified" }),
                verified: z.verified,
            });
        }
    }
    (out, notes)
}

/// Peak stresses of a section under its forces: (normal stress at the
/// extreme fibre, shear there from torsion, shear at the neutral axis from
/// torsion and transverse shear), with how the section was treated.
struct Stress {
    axial: f64,
    bending: f64,
    torsion: f64,
    shear: f64,
}

impl Stress {
    /// Von Mises at the extreme fibre and at the neutral axis; the larger.
    fn von_mises(&self) -> f64 {
        let fibre = ((self.axial.abs() + self.bending).powi(2) + 3.0 * self.torsion.powi(2)).sqrt();
        let axis = (self.axial.powi(2) + 3.0 * (self.torsion + self.shear).powi(2)).sqrt();
        fibre.max(axis)
    }
}

fn stress(sec: &topo_core::Section, d: &Diagram, i: usize) -> Stress {
    let p = &sec.props;
    let at = |v: &[f64]| v.get(i).copied().unwrap_or(0.0);
    let (n, v, m, v2, m2, t) = (d.n[i], d.v[i], d.m[i], at(&d.v2), at(&d.m2), at(&d.tq).abs());
    let shear = v.hypot(v2);
    let (bending, torsion, k_shear) = match &sec.shape {
        Shape::Round { d: dia, .. } => (m.hypot(m2) / p.s_u, t * dia / 2.0 / p.j, if sec.round_dims().is_some_and(|(_, w)| w > 0.0) { 2.0 } else { 4.0 / 3.0 }),
        Shape::Rect { b, d: dd } => {
            let (long, short) = (b.max(*dd), b.min(*dd));
            (m.abs() / p.s_u + m2.abs() / p.s_v, t * (3.0 + 1.8 * short / long) / (long * short * short), 1.5)
        }
        Shape::Polygon { .. } => (m.abs() / p.s_u + m2.abs() / p.s_v, t * p.width.max(p.depth) / 2.0 / p.j, 2.0),
    };
    Stress { axial: n / p.area, bending, torsion, shear: k_shear * shear / p.area }
}

fn section_note(sec: &topo_core::Section) -> Option<String> {
    match &sec.shape {
        Shape::Polygon { .. } => Some("Profile section: shear taken as 2V/A and torsion stress as T·c/J at the outside (approximate).".into()),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn check(model_member: MemberId, combo: &str, title: &str, clause: &str, demand: f64, capacity: f64, unit: &str, t: Trace) -> Check {
    Check {
        member: model_member,
        combination: combo.into(),
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
        trace: t.done(),
    }
}

/// Stress, buckling and displacement checks of one member for one combination.
pub(crate) fn member_checks(model: &Model, geom: &Geometry, combo: &str, m: MemberId, info: &MemberInfo, d: &Diagram, out: &mut Vec<Check>) {
    let mem = model.member(m);
    let (sec, mat) = (model.section(mem.section), model.material(mem.material));
    let Some(strength) = &mat.strength else { return };
    if d.t.is_empty() {
        return;
    }
    let n_safe = safety(model);
    let (zs, zone_notes) = zones(model, geom, m);
    let laminate = mat.family == "cfrp" || mat.family == "laminate";

    // Stress: the worst ratio along the member, the strength depending on where.
    let fy_at = |t: f64| zs.iter().filter(|z| t >= z.from && t <= z.to).min_by(|a, b| a.fy.total_cmp(&b.fy));
    let mut worst = (0.0f64, 0usize);
    for i in 0..d.t.len() {
        let fy = fy_at(d.t[i]).map(|z| z.fy).unwrap_or(strength.yield_);
        let r = stress(sec, d, i).von_mises() / (fy / n_safe);
        if r > worst.0 {
            worst = (r, i);
        }
    }
    let i = worst.1;
    let s = stress(sec, d, i);
    let p = &sec.props;
    let at = |v: &[f64]| v.get(i).copied().unwrap_or(0.0);
    let mut t = Trace::new();
    t.given("N", "axial force (tension +)", d.n[i], "N", None);
    let mb = t.given("M", "bending moment (both axes combined)", d.m[i].hypot(at(&d.m2)), "N·m", None);
    let tq = t.given("T", "torque", at(&d.tq).abs(), "N·m", None);
    let vv = t.given("V", "shear (both axes combined)", d.v[i].hypot(at(&d.v2)), "N", None);
    let a = t.given("A", &format!("area of {}", sec.name), p.area / (MM * MM), "mm²", None);
    t.s("σ_a", "N / A", format!("{} / {}", fmt(d.n[i]), fmt(a)), s.axial / MPA, "MPa", None);
    t.s("σ_b", if sec.is_round() { "M / S" } else { "M_z / S_z + M_y / S_y" }, format!("M = {} N·m, S = {} mm³", fmt(mb), fmt(p.s_u / MM.powi(3))), s.bending / MPA, "MPa", None);
    t.s("τ_T", if sec.is_round() { "T · r / J" } else { "torsion shear at the outside" }, format!("T = {} N·m, J = {} mm⁴", fmt(tq), fmt(p.j / MM.powi(4))), s.torsion / MPA, "MPa", None);
    t.s("τ_V", "transverse shear at the neutral axis", format!("V = {} N", fmt(vv)), s.shear / MPA, "MPa", None);
    let vm = t.s("σ_vM", "larger of √((σ_a + σ_b)² + 3τ_T²) and √(σ_a² + 3(τ_T + τ_V)²)", String::new(), s.von_mises() / MPA, "MPa", Some("von Mises"));
    let zone = fy_at(d.t[i]);
    let fy = match zone {
        Some(z) => t.given("F_y", &format!("yield strength {}", z.why), z.fy / MPA, "MPa", None),
        None => t.given("F_y", &format!("{} of {}", if laminate { "allowable laminate stress" } else { "yield strength" }, mat.name), strength.yield_ / MPA, "MPa", Some(&format!("{}{}", strength.source, if strength.verified { "" } else { " — unverified" }))),
    };
    let nf = t.given("n", "factor of safety (design basis)", n_safe, "", None);
    let allow = t.s("F", "F_y / n", format!("{} / {}", fmt(fy), fmt(nf)), fy / nf, "MPa", None);
    let mut c = check(m, combo, if zone.is_some() { "Stress (beside a joint)" } else { "Stress" }, "von Mises", vm, allow, "MPa", t);
    c.at = Some(d.t[i]);
    c.verified = strength.verified && zone.is_none_or(|z| z.verified);
    c.notes.push("Static strength only: fatigue, local buckling of thin walls and stress concentration at joints are not checked.".into());
    c.notes.extend(section_note(sec));
    c.notes.extend(zone_notes);
    if laminate {
        c.notes.push("Laminate checked as if isotropic against one allowable stress: ply failure, layup and joints are not modelled.".into());
        c.indicative = true;
    }
    out.push(c);

    // Euler buckling of the longest stretch between nodes, about the weaker axis.
    let (nmin, ib) = d.n.iter().enumerate().fold((0.0f64, 0), |(a, k), (j, &x)| if x < a { (x, j) } else { (a, k) });
    if nmin < 0.0 {
        let le = mem.path.windows(2).map(|w| model.pos(w[0]).distance(model.pos(w[1]))).fold(0.0, f64::max);
        let mut t = Trace::new();
        let pc = t.given("P", "axial compression", -nmin, "N", None);
        let e = t.given("E", &format!("modulus of {}", mat.name), mat.e / MPA, "MPa", None);
        let imin = t.given("I", "second moment of area, weaker axis", p.i_u.min(p.i_v) / MM.powi(4), "mm⁴", None);
        let l = t.given("L", "longest stretch between joints (K = 1)", le / MM, "mm", None);
        let pcr = t.s("P_cr", "π² E I / L²", format!("π² · {} · {} / {}²", fmt(e), fmt(imin), fmt(l)), std::f64::consts::PI.powi(2) * e * imin / (l * l), "N", Some("Euler"));
        let nf = t.given("n", "factor of safety (design basis)", n_safe, "", None);
        let cap = t.s("P_allow", "P_cr / n", format!("{} / {}", fmt(pcr), fmt(nf)), pcr / nf, "N", None);
        let mut c = check(m, combo, "Buckling (Euler)", "Euler", pc, cap, "N", t);
        c.at = Some(d.t[ib]);
        c.verified = true;
        c.notes.push("Elastic buckling between joints with pinned ends assumed (K = 1); a stocky member is governed by the stress check.".into());
        out.push(c);
    }

    // Displacement against the stated limit.
    if let Some(limit) = model.standards.deflection_limit.filter(|_| !d.d.is_empty()) {
        let (dmax, id) = d.max_displacement();
        let mut t = Trace::new();
        let dm = t.given("δ", "largest displacement of the member (frame analysis)", dmax / MM, "mm", None);
        let lim = t.given("δ_allow", "limit (design basis)", limit / MM, "mm", None);
        let mut c = check(m, combo, "Displacement", "design basis", dm, lim, "mm", t);
        c.at = Some(d.t[id]);
        c.verified = true;
        out.push(c);
    }
    let _ = info;
}

/// Joints with rated capacities, against the forces at the member ends they hold.
pub(crate) fn joint_checks(model: &Model, combo: &str, ds: &[Diagram], out: &mut Vec<Check>) {
    for j in &model.joints {
        for c in &j.connections {
            let conn = model.connection(c.connection);
            let Some(rating) = &conn.rating else { continue };
            let mem = model.member(c.member);
            let d = &ds[c.member.idx()];
            if d.t.is_empty() {
                continue;
            }
            // The member's end at this node (a joint along its length is not rated here).
            let i = if mem.start() == j.node {
                0
            } else if mem.end() == j.node {
                d.t.len() - 1
            } else {
                continue;
            };
            let at = |v: &[f64]| v.get(i).copied().unwrap_or(0.0);
            let demands = [
                ("moment", "M", "bending moment at the joint (both axes combined)", d.m[i].hypot(at(&d.m2)), rating.moment, "N·m"),
                ("shear", "V", "force across the member at the joint", d.v[i].hypot(at(&d.v2)), rating.shear, "N"),
                ("pull-off", "N", "tension along the member at the joint", d.n[i].max(0.0), rating.tension, "N"),
            ];
            for (what, sym, desc, demand, cap, unit) in demands {
                let Some(cap) = cap else { continue };
                let mut t = Trace::new();
                let dv = t.given(sym, desc, demand, unit, None);
                let cv = t.given(&format!("{sym}_allow"), &format!("rated {what} of {}", conn.describe()), cap, unit, Some(&format!("{}{}", rating.source, if rating.verified { "" } else { " — unverified" })));
                let mut k = check(c.member, combo, &format!("Joint: {} ({what})", conn.name), "maker's rating", dv, cv, unit, t);
                k.other = c.to;
                k.at = Some(d.t[i]);
                k.verified = rating.verified;
                out.push(k);
            }
        }
    }
}
