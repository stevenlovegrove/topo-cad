//! Brute-force verification of hidden-line removal: every sample point on an
//! edge reported *visible* must have a clear ray to the viewer, and every point
//! reported *hidden* must be blocked.

use topo_core::{Plane, Topology, Vec3};
use topo_draw::hlr::{hidden_lines, Projector, Seg};
use topo_draw::views::iso_projector;
use topo_geom::{Geometry, MemberGeom};

/// Does the ray `p + s·d` (s > 0) pass through the interior of ply `k` of `g`?
fn blocks(g: &MemberGeom, k: usize, p: Vec3, d: Vec3) -> bool {
    let pl = &g.place;
    let hull = topo_core::convex_hull(&pl.plies[k].outer);
    let n = hull.len();
    let mut planes: Vec<Plane> = (0..n)
        .map(|i| Plane { point: pl.at(hull[i], 0.0), normal: pl.side_normal(hull[i], hull[(i + 1) % n]) })
        .collect();
    planes.extend(g.start.planes().copied());
    planes.extend(g.end.planes().copied());
    let (mut s0, mut s1) = (1e-6f64, f64::INFINITY);
    for pl in planes {
        let num = -pl.signed_distance(p);
        let den = pl.normal.dot(d);
        if den.abs() < 1e-12 {
            if num < 1e-6 {
                return false; // parallel and outside (or on) this face
            }
        } else if den < 0.0 {
            s0 = s0.max(num / den);
        } else {
            s1 = s1.min(num / den);
        }
    }
    s1 - s0 > 1e-5
}

fn check(members: &[&MemberGeom], proj: &Projector) -> Vec<(Seg, f64, usize)> {
    let segs = hidden_lines(members, proj);
    let mut bad = vec![];
    for s in &segs {
        if s.a3.distance(s.b3) < 1e-4 {
            continue;
        }
        for k in 1..8 {
            let t = k as f64 / 8.0;
            let p = s.a3.lerp(s.b3, t);
            // Any *other ply* can occlude — including other plies of the same member.
            let hit = members.iter().position(|g| {
                (0..g.place.plies.len()).any(|k| (g.member, k) != (s.member, s.ply) && blocks(g, k, p, proj.toward))
            });
            if let (false, Some(h)) = (s.hidden, hit) {
                bad.push((*s, t, h));
                break;
            }
        }
    }
    bad
}

#[test]
fn garage_iso_has_no_leaked_hidden_lines() {
    let m = topo_timber::examples::garage_studio();
    let topo = Topology::build(&m);
    let g = Geometry::build(&m, &topo);
    let members: Vec<&MemberGeom> = g.members.iter().collect();
    let bad = check(&members, &iso_projector());
    let report: Vec<String> = bad
        .iter()
        .take(15)
        .map(|(s, t, j)| {
            format!(
                "{} {} edge visible at t={t:.2} but blocked by {} {}",
                s.member,
                m.member(s.member).role,
                members[*j].member,
                m.member(members[*j].member).role
            )
        })
        .collect();
    assert!(bad.is_empty(), "{} leaked segments, e.g.\n{}", bad.len(), report.join("\n"));
}

#[test]
fn truss_roof_iso_has_no_leaked_hidden_lines() {
    let m = topo_timber::examples::garage_as_built();
    let topo = Topology::build(&m);
    let g = Geometry::build(&m, &topo);
    let members: Vec<&MemberGeom> = g.members.iter().collect();
    for proj in [iso_projector(), Projector::new(-Vec3::X, Vec3::Z)] {
        let bad = check(&members, &proj);
        let report: Vec<String> = bad.iter().take(10).map(|(s, _, j)| format!("{} {} / {} {}", s.member, m.member(s.member).role, members[*j].member, m.member(members[*j].member).role)).collect();
        assert!(bad.is_empty(), "{} leaked for {:?}:\n{}", bad.len(), proj.toward, report.join("\n"));
    }
}

#[test]
fn garage_plan_and_elevation_have_no_leaked_hidden_lines() {
    let m = topo_timber::examples::garage_studio();
    let topo = Topology::build(&m);
    let g = Geometry::build(&m, &topo);
    let members: Vec<&MemberGeom> = g.members.iter().collect();
    for proj in [Projector::new(Vec3::Z, Vec3::Y), Projector::new(-Vec3::Y, Vec3::Z), Projector::new(Vec3::new(1.0, 0.8, 0.6), Vec3::Z)] {
        let bad = check(&members, &proj);
        assert!(bad.is_empty(), "{} leaked segments for {:?}", bad.len(), proj.toward);
    }
}
