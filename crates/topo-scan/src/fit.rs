//! Fitting a surface someone has pointed at: a plane through scan points,
//! with the support stated (which points may count: a region, a band about
//! the plane, optionally only where the points' own local normal agrees),
//! and with what a reader needs to judge the fit: how many points, their
//! spread, how the offset varies over the patch, and what other surfaces lie
//! in front of and behind it.
//!
//! The result is evidence, not a verdict: which physical face the plane is
//! (a stud's face, sheathing seen between studs, a finish) is for the person
//! or LLM who marked it to say.

use std::collections::HashMap;

use crate::stats::point_stats;
use crate::{cross, dot, norm, scale, sub, V3};

#[derive(Debug, Clone)]
pub struct FitSpec {
    /// A point on (or near) the surface.
    pub seed: V3,
    /// The normal expected, if known; the fit may turn from it by at most
    /// `normal_tol` (radians), else it is held.
    pub normal: Option<V3>,
    pub normal_tol: f32,
    /// Points within this of the plane count (m).
    pub thickness: f32,
    /// Keep only points whose own neighbourhood (a 5 cm cell) is a surface
    /// facing as the plane does, within `normal_tol`: drops edges, clutter
    /// and surfaces crossing the band.
    pub cell_normals: bool,
}

#[derive(Debug, Clone)]
pub struct Fit {
    /// A point on the plane (the inliers' centroid), its unit normal, and
    /// n·p = d.
    pub centre: V3,
    pub normal: V3,
    pub d: f32,
    /// In-plane axes (v is up where the plane is not level), and the
    /// inliers' extent along them about `centre` (2nd–98th percentiles).
    pub u: V3,
    pub v: V3,
    pub ext: [f32; 4],
    pub count: usize,
    pub candidates: usize,
    /// RMS distance of the inliers from the plane (m).
    pub rms: f32,
    /// Mean offset from the plane of the inliers in each quadrant of the
    /// patch (−u−v, +u−v, −u+v, +u+v), where it has 20 or more: a warp, a
    /// tilt the fit could not take, or two surfaces taken as one show here.
    pub quadrants: [Option<f32>; 4],
    /// Share of the patch's 10 cm cells that hold inliers.
    pub cover: f32,
    /// Other surfaces across the plane within the patch: (offset along the
    /// normal, share of the candidates), strongest first; the plane itself
    /// is near 0.
    pub profile: Vec<(f32, f32)>,
    /// Whether the normal was held at the expected one.
    pub held: bool,
    /// The inliers, as indices into the candidates given.
    pub inliers: Vec<u32>,
}

/// Fits a plane to `pts` (the candidates: the points of the support region)
/// from a seed. None when too few points lie near the seed.
pub fn fit_plane(pts: &[V3], spec: &FitSpec) -> Option<Fit> {
    let unit = |a: V3| scale(a, 1.0 / norm(a).max(1e-12));
    // Where to start: the expected normal, or the plane of what is by the seed.
    let n0 = match spec.normal {
        Some(n) => unit(n),
        None => {
            let near = point_stats(pts.iter().copied().filter(|p| norm(sub(*p, spec.seed)) < 0.2));
            if near.count < 20 {
                return None;
            }
            near.plane?.normal
        }
    };
    // Local normals in 5 cm cells, for the filter.
    let cells: HashMap<(i32, i32, i32), V3> = if spec.cell_normals {
        let key = |p: V3| ((p[0] / 0.05).floor() as i32, (p[1] / 0.05).floor() as i32, (p[2] / 0.05).floor() as i32);
        let mut groups: HashMap<(i32, i32, i32), Vec<V3>> = HashMap::new();
        for p in pts {
            groups.entry(key(*p)).or_default().push(*p);
        }
        groups.into_iter().filter_map(|(k, v)| {
            let s = point_stats(v.into_iter());
            // A surface: thin across, and at least 8 points.
            (s.count >= 8 && s.sigma[2] < 0.012).then(|| (k, s.plane.unwrap().normal))
        }).collect()
    } else {
        HashMap::new()
    };
    let agrees = |p: V3, n: V3| {
        if !spec.cell_normals {
            return true;
        }
        let k = ((p[0] / 0.05).floor() as i32, (p[1] / 0.05).floor() as i32, (p[2] / 0.05).floor() as i32);
        cells.get(&k).is_some_and(|c| dot(*c, n).abs() >= spec.normal_tol.cos())
    };

    let (mut n, mut d) = (n0, dot(n0, spec.seed));
    let mut held = false;
    let mut inliers: Vec<u32> = Vec::new();
    for it in 0..10 {
        // A wider band at first, to find the surface from an imprecise seed.
        let band = if it < 2 { 2.0 * spec.thickness } else { spec.thickness };
        inliers = (0..pts.len() as u32).filter(|&i| (dot(n, pts[i as usize]) - d).abs() <= band && agrees(pts[i as usize], n)).collect();
        if inliers.len() < 20 {
            return None;
        }
        let s = point_stats(inliers.iter().map(|&i| pts[i as usize]));
        let mut m = s.plane.as_ref()?.normal;
        if dot(m, n) < 0.0 {
            m = scale(m, -1.0);
        }
        held = false;
        if spec.normal.is_some() && dot(m, n0) < spec.normal_tol.cos() {
            m = n0;
            held = true;
        }
        let d_new = dot(m, s.centroid);
        let moved = (d_new - d).abs() + norm(sub(m, n));
        (n, d) = (m, d_new);
        if it >= 2 && moved < 1e-5 {
            break;
        }
    }
    let s = point_stats(inliers.iter().map(|&i| pts[i as usize]));
    let centre = sub(s.centroid, scale(n, dot(n, s.centroid) - d));
    let rms = (inliers.iter().map(|&i| (dot(n, pts[i as usize]) - d).powi(2)).sum::<f32>() / inliers.len() as f32).sqrt();
    // In-plane axes: v up where the plane is not level, u = v × n.
    let up = if n[2].abs() < 0.9 { [0.0, 0.0, 1.0] } else { [0.0, 1.0, 0.0] };
    let v = unit(sub(up, scale(n, dot(up, n))));
    let u = cross(v, n);
    let uv: Vec<(f32, f32, f32)> = inliers.iter().map(|&i| {
        let q = sub(pts[i as usize], centre);
        (dot(q, u), dot(q, v), dot(q, n))
    }).collect();
    let pct = |mut x: Vec<f32>, f: f32| {
        let k = ((x.len() - 1) as f32 * f) as usize;
        *x.select_nth_unstable_by(k, |a, b| a.total_cmp(b)).1
    };
    let us: Vec<f32> = uv.iter().map(|p| p.0).collect();
    let vs: Vec<f32> = uv.iter().map(|p| p.1).collect();
    let ext = [pct(us.clone(), 0.02), pct(us, 0.98), pct(vs.clone(), 0.02), pct(vs, 0.98)];
    let mut q = [(0.0f32, 0u32); 4];
    let mut occupied = std::collections::HashSet::new();
    for &(a, b, c) in &uv {
        let k = (a > 0.0) as usize + 2 * (b > 0.0) as usize;
        q[k].0 += c;
        q[k].1 += 1;
        if a >= ext[0] && a <= ext[1] && b >= ext[2] && b <= ext[3] {
            occupied.insert((((a - ext[0]) / 0.1) as i32, ((b - ext[2]) / 0.1) as i32));
        }
    }
    let quadrants = q.map(|(sum, k)| (k >= 20).then(|| sum / k as f32));
    let cells_in_patch = (((ext[1] - ext[0]) / 0.1).ceil().max(1.0) * ((ext[3] - ext[2]) / 0.1).ceil().max(1.0)) as f32;
    // What else lies across the plane, within the patch: 1 cm bins to ±0.4 m.
    let mut h = vec![0u32; 80];
    let mut total = 0u32;
    for p in pts {
        let r = sub(*p, centre);
        let (a, b, c) = (dot(r, u), dot(r, v), dot(r, n));
        if a >= ext[0] && a <= ext[1] && b >= ext[2] && b <= ext[3] && c.abs() < 0.4 {
            h[((c + 0.4) / 0.01) as usize % 80] += 1;
            total += 1;
        }
    }
    let sm: Vec<u32> = (0..80).map(|k: usize| h[k.saturating_sub(1)..(k + 2).min(80)].iter().sum()).collect();
    let mut order: Vec<usize> = (0..80).filter(|&k| sm[k] > 0 && (k == 0 || sm[k] >= sm[k - 1]) && (k == 79 || sm[k] >= sm[k + 1])).collect();
    order.sort_by(|&a, &b| sm[b].cmp(&sm[a]));
    let mut profile: Vec<(f32, f32)> = Vec::new();
    for k in order {
        let (w, x) = (k.saturating_sub(1)..(k + 2).min(80)).fold((0.0f32, 0.0f32), |a, j| (a.0 + h[j] as f32, a.1 + h[j] as f32 * (-0.4 + (j as f32 + 0.5) * 0.01)));
        let x = x / w.max(1.0);
        if profile.iter().all(|p| (p.0 - x).abs() > 0.04) && sm[k] as f32 >= 0.02 * total as f32 {
            profile.push((x, sm[k] as f32 / total.max(1) as f32));
        }
        if profile.len() == 5 {
            break;
        }
    }
    Some(Fit { centre, normal: n, d, u, v, ext, count: inliers.len(), candidates: pts.len(), rms, quadrants, cover: occupied.len() as f32 / cells_in_patch, profile, held, inliers })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stud wall seen from inside: sheathing at y = 2.0 between studs,
    /// stud faces at y = 1.86, and a ladder leaning across.
    fn wall() -> Vec<V3> {
        let mut pts = Vec::new();
        let mut r = 12345u32;
        let mut noise = || {
            r = r.wrapping_mul(1664525).wrapping_add(1013904223);
            ((r >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.008
        };
        for i in 0..160 {
            for j in 0..100 {
                let (x, z) = (i as f32 * 0.02, 0.2 + j as f32 * 0.02);
                // Studs 38 mm wide every 406 mm.
                let on_stud = (x % 0.406) < 0.038;
                pts.push([x, if on_stud { 1.86 } else { 2.0 } + noise(), z]);
            }
        }
        // Clutter: a slanted board crossing the band.
        for i in 0..400 {
            let t = i as f32 / 400.0;
            pts.push([1.0 + 0.3 * t, 1.7 + 0.4 * t + noise(), 0.3 + 1.5 * t]);
        }
        pts
    }

    #[test]
    fn fits_the_surface_at_the_seed_and_not_its_neighbour() {
        let pts = wall();
        let spec = FitSpec { seed: [1.2, 2.01, 1.0], normal: None, normal_tol: 15f32.to_radians(), thickness: 0.03, cell_normals: false };
        let f = fit_plane(&pts, &spec).expect("a fit");
        assert!((f.d * f.normal[1].signum() - 2.0).abs() < 0.002, "sheathing at y = 2.0, got {}", f.d * f.normal[1].signum());
        assert!(f.normal[1].abs() > 0.9999);
        assert!(f.rms < 0.004);
        // The stud faces show in the profile, 14 cm in front.
        assert!(f.profile.iter().any(|p| (p.0.abs() - 0.14).abs() < 0.01), "{:?}", f.profile);
        // Seeded on a stud's face, the studs' plane is found instead.
        let f = fit_plane(&pts, &FitSpec { seed: [0.415, 1.86, 1.0], normal: Some([0.0, 1.0, 0.0]), ..spec.clone() }).expect("a fit");
        assert!((f.d - 1.86).abs() < 0.002, "{}", f.d);
        // In strips, so thinly covered.
        assert!(f.cover < 0.5, "{}", f.cover);
    }

    #[test]
    fn the_normal_filter_drops_what_crosses_the_band() {
        let pts = wall();
        let spec = FitSpec { seed: [1.2, 2.0, 1.0], normal: Some([0.0, 1.0, 0.0]), normal_tol: 15f32.to_radians(), thickness: 0.05, cell_normals: false };
        let plain = fit_plane(&pts, &spec).unwrap();
        let filtered = fit_plane(&pts, &FitSpec { cell_normals: true, ..spec }).unwrap();
        // The board's points in the band are among the plain fit's inliers, and not the filtered one's.
        let board = |f: &Fit| f.inliers.iter().filter(|&&i| i as usize >= 16000).count();
        assert!(board(&plain) > 20, "{}", board(&plain));
        assert_eq!(board(&filtered), 0);
        assert!((filtered.d - 2.0).abs() < 0.002);
    }

    #[test]
    fn a_normal_far_from_the_expected_is_held() {
        let pts = wall();
        // Expecting a wall facing x where the surface faces y: held, and said so.
        let f = fit_plane(&pts, &FitSpec { seed: [1.2, 2.0, 1.0], normal: Some([1.0, 0.0, 0.0]), normal_tol: 10f32.to_radians(), thickness: 0.05, cell_normals: false }).unwrap();
        assert!(f.held);
        assert!(f.normal[0] > 0.999);
    }
}
