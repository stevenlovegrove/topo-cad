//! Where a surface's layers lie: flat panels on the framing's top faces.

use crate::Geometry;
use serde::{Deserialize, Serialize};
use topo_core::{MemberId, Model, Surface, SurfaceRegion, Vec3};

/// A rectangle on top of the framing: points `origin + s·slope + r·run` for
/// s, r in [0, 1]; layers stack outward along `normal`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Panel {
    pub origin: Vec3,
    /// Across the ridge: from the low edge to the high edge (horizontal on a flat roof).
    pub slope: Vec3,
    /// Along the ridge, from the first truss to the last.
    pub run: Vec3,
    /// Unit, outward (up).
    pub normal: Vec3,
}

fn solid_points(geom: &Geometry, id: MemberId) -> Vec<Vec3> {
    geom.member(id).faces().into_iter().flat_map(|f| f.outer).collect()
}

/// Panels for a roof surface: one per top chord (sloped) or bottom-chord
/// extension (flat) of a typical truss, run from the outer face of the first
/// truss to that of the last.
pub fn surface_panels(model: &Model, geom: &Geometry, s: &Surface) -> Vec<Panel> {
    let roof = model.group(s.group);
    let run = roof.frame.x.normalized();
    let trusses: Vec<_> = model.groups.iter().filter(|g| g.parent == Some(s.group) && g.kind == "truss").collect();
    if trusses.is_empty() {
        return vec![];
    }
    let with_role = |g: &topo_core::Group, role: &str| -> Vec<MemberId> { g.members.iter().copied().filter(|&m| model.member(m).role == role).collect() };
    // Extent along the ridge: over every chord of every truss.
    let (mut r0, mut r1) = (f64::INFINITY, f64::NEG_INFINITY);
    for t in &trusses {
        for m in with_role(t, "top_chord").into_iter().chain(with_role(t, "bottom_chord")) {
            for p in solid_points(geom, m) {
                r0 = r0.min(p.dot(run));
                r1 = r1.max(p.dot(run));
            }
        }
    }
    // A typical truss (the middle one: the ends may be gable or special trusses).
    let typical = trusses[trusses.len() / 2];
    let tops = with_role(typical, "top_chord");
    let mut out = vec![];
    // A panel on member `m`'s top face between axial stations (t0, t1) of its node line.
    let mut panel = |m: MemberId, normal_hint: Option<Vec3>, range: Option<(f64, f64)>| {
        let mm = model.member(m);
        let (a, b) = (model.pos(mm.start()), model.pos(mm.end()));
        let d = (b - a).normalized();
        let mut n = normal_hint.unwrap_or_else(|| run.cross(d).normalized());
        if n.z < 0.0 {
            n = -n;
        }
        let pts = solid_points(geom, m);
        let off = pts.iter().map(|p| (*p - a).dot(n)).fold(f64::NEG_INFINITY, f64::max);
        let (t0, t1) = range.unwrap_or_else(|| {
            let ts = pts.iter().map(|p| (*p - a).dot(d));
            ts.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), t| (lo.min(t), hi.max(t)))
        });
        let base = a + d * t0 + n * off;
        out.push(Panel { origin: base + run * (r0 - base.dot(run)), slope: d * (t1 - t0), run: run * (r1 - r0), normal: n });
    };
    match s.region {
        SurfaceRegion::Slope => {
            for &m in &tops {
                panel(m, None, None);
            }
        }
        SurfaceRegion::Flat => {
            // Bottom chords beyond the nodes they share with the top chords.
            let top_nodes: std::collections::HashSet<_> = tops.iter().flat_map(|&m| model.member(m).path.clone()).collect();
            for bc in with_role(typical, "bottom_chord") {
                let mm = model.member(bc);
                let a = model.pos(mm.start());
                let len = model.pos(mm.end()).distance(a);
                let x = (model.pos(mm.end()) - a) / len;
                let ts: Vec<f64> = mm.path.iter().filter(|n| top_nodes.contains(n)).map(|&n| (model.pos(n) - a).dot(x)).collect();
                if ts.is_empty() {
                    continue;
                }
                let lo = ts.iter().copied().fold(f64::INFINITY, f64::min);
                let hi = ts.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                // The chord's solid extent along its line (its cut ends).
                let (e0, e1) = solid_points(geom, bc).iter().map(|p| (*p - a).dot(x)).fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), t| (l.min(t), h.max(t)));
                for (t0, t1) in [(e0, lo), (hi, e1)] {
                    if t1 - t0 > 1e-3 {
                        panel(bc, Some(Vec3::Z), Some((t0, t1)));
                    }
                }
            }
        }
    }
    out
}

