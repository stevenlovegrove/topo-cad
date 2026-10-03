//! How much a scan point is to be believed: how far off its surface it is
//! likely to be (the lidar's noise, by range), and whether it is a point of
//! a surface at all.
//!
//! - **Noise by range.** Off its surface a point is spread by about
//!   σ(range) = 2.4 mm + 0.45 mm per metre (measured on a Livox Mid-360 scan of
//!   a garage, `examples/noise.rs`: 3 mm under 2 m, 7 mm at 8 to 12 m; it
//!   hardly changes with the angle the ray meets the surface at).
//! - **The rays.** A point of a surface stops the rays that reach it; one in
//!   free space has others pass through where it is. Where a beam straddles
//!   an edge the return mixes the near and the far surface, and the point
//!   lands between them, on the ray: a smear behind occlusion edges. A point
//!   is doubted by the share of the rays near it that pass through its cell
//!   and end more than 3σ (of their own range) beyond it (so a point's own
//!   noise in front of its surface does not count against it). A point within
//!   3σ of a surface the points about it make (a clean plane in 2, 4 or 8 cm,
//!   the smallest there is) is
//!   judged by the rays that go through that surface there: that cross its
//!   plane and end more than 3σ behind it. One skimming along a wall passes
//!   close in front of it over a long way, and says nothing about whether the
//!   wall is there.
//! - **The lidar's flags** (Livox: noise by position, by intensity), where
//!   the scan has them: they mark some of the same edge points.
//!
//! A point's confidence (0 to 1) is (1 − that share)² times what its flags
//! allow; its weight in a fit is its confidence over σ².

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

use rayon::prelude::*;

use crate::scan::Scan;
use crate::{dot, norm, sub, V3};

/// How far off its surface a point at this range is likely to be (σ, m).
pub fn sigma(range: f32) -> f32 {
    0.0024 + 0.00045 * range
}

/// How much the lidar's flags allow a point (Livox: bits 0–1 noise by
/// position, 2–3 by intensity; 1 high, 2 moderate, 3 low confidence of noise).
pub fn tag_factor(tag: u8) -> f32 {
    let by = |bits: u8| -> f32 { match bits & 3 {
        1 => 0.1,
        2 => 0.3,
        3 => 0.7,
        _ => 1.0,
    } };
    by(tag) * by(tag >> 2).max(0.5)
}

/// The confidence (0 to 1) of each of the points `idx` of a scan, from the
/// rays of all its used points through the box about them.
pub fn confidence(scan: &Scan, idx: &[u32]) -> Vec<f32> {
    if idx.is_empty() {
        return vec![];
    }
    let tag = |i: u32| scan.tag.get(i as usize).map_or(1.0, |&t| tag_factor(t));
    let Some(sensor) = scan.sensor.as_deref() else {
        return idx.iter().map(|&i| tag(i)).collect();
    };
    // The box about the points, a little grown; cells of 4 mm, or larger to keep to 4 million.
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for &i in idx {
        let p = scan.xyz[i as usize];
        for a in 0..3 {
            lo[a] = lo[a].min(p[a] - 0.02);
            hi[a] = hi[a].max(p[a] + 0.02);
        }
    }
    let size: V3 = std::array::from_fn(|a| hi[a] - lo[a]);
    let cell = 0.004f32.max((size[0] * size[1] * size[2] / 4.0e6).cbrt());
    let n: [usize; 3] = std::array::from_fn(|a| ((size[a] / cell).ceil() as usize).max(1));
    let cell_of = |p: V3| -> Option<usize> {
        let k: [isize; 3] = std::array::from_fn(|a| ((p[a] - lo[a]) / cell).floor() as isize);
        (0..3).all(|a| k[a] >= 0 && (k[a] as usize) < n[a]).then(|| k[0] as usize + n[0] * (k[1] as usize + n[1] * k[2] as usize))
    };
    // Where the points judged make a surface: cells of 2, 4 and 8 cm of them that are a plane (thin across: under
    // 25 mm, and under half their spread along it; a smear, a line, and haze are not); for each cell of the grid,
    // the plane at each size (if any). Several sizes, so that a
    // surface is found beside an edge (a small cell) and through noise (a large one).
    const SIZES: [f32; 3] = [0.02, 0.04, 0.08];
    let coarse = |p: V3, s: usize| -> (usize, i32, i32, i32) { (s, (p[0] / SIZES[s]).floor() as i32, (p[1] / SIZES[s]).floor() as i32, (p[2] / SIZES[s]).floor() as i32) };
    let mut groups: HashMap<(usize, i32, i32, i32), Vec<V3>> = HashMap::new();
    for &i in idx {
        let p = scan.xyz[i as usize];
        for sz in 0..SIZES.len() {
            groups.entry(coarse(p, sz)).or_default().push(p);
        }
    }
    let mut planes: Vec<(V3, f32)> = Vec::new();
    let mut surface_at: HashMap<(usize, i32, i32, i32), u32> = HashMap::new();
    for (k, v) in groups {
        let st = crate::stats::point_stats(v.into_iter());
        if st.count >= 8 && st.sigma[2] < 0.025 && st.sigma[2] < 0.5 * st.sigma[1] {
            surface_at.insert(k, planes.len() as u32);
            planes.push((st.axes[2], dot(st.axes[2], st.centroid)));
        }
    }
    let cells = n[0] * n[1] * n[2];
    let surface: Vec<[u32; 3]> = (0..cells)
        .into_par_iter()
        .map(|c| {
            let k = [c % n[0], (c / n[0]) % n[1], c / (n[0] * n[1])];
            let p: V3 = std::array::from_fn(|a| lo[a] + (k[a] as f32 + 0.5) * cell);
            std::array::from_fn(|sz| surface_at.get(&coarse(p, sz)).copied().unwrap_or(u32::MAX))
        })
        .collect();
    // Off any surface: the rays that pass each cell, and the points that end in it. On a surface: the rays that go
    // through it there (cross its plane, and end more than 3σ behind it), and the points that end on it there.
    let pass: Vec<AtomicU32> = (0..cells).map(|_| AtomicU32::new(0)).collect();
    let ends: Vec<AtomicU32> = (0..cells).map(|_| AtomicU32::new(0)).collect();
    let pierce: Vec<AtomicU32> = (0..planes.len()).map(|_| AtomicU32::new(0)).collect();
    let on: Vec<AtomicU32> = (0..planes.len()).map(|_| AtomicU32::new(0)).collect();
    scan.used.par_iter().for_each(|&i| {
        let (s, p) = (sensor[i as usize], scan.xyz[i as usize]);
        if !s[0].is_finite() {
            return;
        }
        if let Some(c) = cell_of(p) {
            ends[c].fetch_add(1, Ordering::Relaxed);
            for &sf in &surface[c] {
                if sf != u32::MAX {
                    on[sf as usize].fetch_add(1, Ordering::Relaxed);
                }
            }
        }
        let d = sub(p, s);
        let len = norm(d);
        if len < 1e-6 {
            return;
        }
        let margin = 3.0 * sigma(len);
        // The part of the ray in the box, short of its end by 3σ.
        let (mut t0, mut t1) = (0.0f32, (len - margin).max(0.0) / len);
        for a in 0..3 {
            if d[a].abs() < 1e-9 {
                if s[a] < lo[a] || s[a] > hi[a] {
                    return;
                }
            } else {
                let (u0, u1) = ((lo[a] - s[a]) / d[a], (hi[a] - s[a]) / d[a]);
                t0 = t0.max(u0.min(u1));
                t1 = t1.min(u0.max(u1));
            }
        }
        if t0 >= t1 {
            return;
        }
        let steps = ((t1 - t0) * len / (0.5 * cell)).ceil() as usize;
        let (mut last, mut last_surface) = (usize::MAX, [u32::MAX; 3]);
        for k in 0..=steps {
            let t = t0 + (t1 - t0) * k as f32 / steps.max(1) as f32;
            let x = [s[0] + d[0] * t, s[1] + d[1] * t, s[2] + d[2] * t];
            let Some(c) = cell_of(x) else { continue };
            if c == last {
                continue;
            }
            last = c;
            pass[c].fetch_add(1, Ordering::Relaxed);
            for (sz, &sf) in surface[c].iter().enumerate() {
                if sf == u32::MAX || sf == last_surface[sz] {
                    continue;
                }
                // Through the surface here: at its plane, from in front of it to well behind.
                let (nm, off) = planes[sf as usize];
                let (at, from, to) = (dot(nm, x) - off, dot(nm, s) - off, dot(nm, p) - off);
                if at.abs() <= cell && from.signum() != to.signum() && to.abs() > margin {
                    pierce[sf as usize].fetch_add(1, Ordering::Relaxed);
                    last_surface[sz] = sf;
                }
            }
        }
    });
    idx.iter()
        .map(|&i| {
            let q = scan.xyz[i as usize];
            // On a surface (within 3σ of its plane): by the rays through it there. Else by the rays through where it is.
            let range = norm(sub(q, sensor[i as usize]));
            let on_surface = (0..SIZES.len()).filter_map(|sz| surface_at.get(&coarse(q, sz)).copied()).find(|&sf| {
                let (nm, off) = planes[sf as usize];
                (dot(nm, q) - off).abs() <= 3.0 * sigma(range)
            });
            let (p, e) = match on_surface {
                Some(sf) => (pierce[sf as usize].load(Ordering::Relaxed), on[sf as usize].load(Ordering::Relaxed)),
                None => match cell_of(q) {
                    Some(c) => (pass[c].load(Ordering::Relaxed), ends[c].load(Ordering::Relaxed)),
                    None => return tag(i),
                },
            };
            let share = p as f32 / (p + e).max(1) as f32;
            (1.0 - share).powi(2) * tag(i)
        })
        .collect()
}

/// The weight of each point in a fit: its confidence over σ² (of its range), scaled so the best is 1.
pub fn weights(scan: &Scan, idx: &[u32], confidence: &[f32]) -> Vec<f32> {
    let range = |i: u32| scan.sensor.as_ref().map_or(2.0, |s| norm(sub(scan.xyz[i as usize], s[i as usize])));
    let s0 = sigma(0.0);
    idx.iter().zip(confidence).map(|(&i, &c)| c * (s0 / sigma(range(i))).powi(2)).collect()
}

/// A plane through weighted points: centroid, unit normal, and the weighted
/// rms distance from it. None for fewer than three points or no weight.
pub fn weighted_plane(pts: &[V3], w: &[f32]) -> Option<(V3, V3, f32)> {
    let sw: f64 = w.iter().map(|&x| x as f64).sum();
    if pts.len() < 3 || sw <= 0.0 {
        return None;
    }
    let c: [f64; 3] = std::array::from_fn(|a| pts.iter().zip(w).map(|(p, &x)| p[a] as f64 * x as f64).sum::<f64>() / sw);
    let mut m = [[0f64; 3]; 3];
    for (p, &x) in pts.iter().zip(w) {
        let d: [f64; 3] = std::array::from_fn(|a| p[a] as f64 - c[a]);
        for a in 0..3 {
            for b in 0..3 {
                m[a][b] += x as f64 * d[a] * d[b] / sw;
            }
        }
    }
    let (vals, vecs) = crate::stats::eigen_sym3(m);
    let k = (0..3).min_by(|&a, &b| vals[a].total_cmp(&vals[b]))?;
    let n: V3 = [vecs[0][k] as f32, vecs[1][k] as f32, vecs[2][k] as f32];
    let c: V3 = c.map(|x| x as f32);
    let rms = (pts.iter().zip(w).map(|(p, &x)| x as f64 * (dot(sub(*p, c), n) as f64).powi(2)).sum::<f64>() / sw).sqrt() as f32;
    Some((c, n, rms))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_in_free_space_is_doubted_and_one_on_a_surface_is_not() {
        // A wall at x = 2 seen from the origin's side (rays 2.5 mm apart at the wall, as dense as a real scan's
        // there), and a stray point at x = 1.5 on one of the rays, as a mixed return would be.
        let mut xyz = Vec::new();
        let mut sensor = Vec::new();
        for i in 0..60 {
            for j in 0..60 {
                let (y, z) = (-0.075 + 0.0025 * i as f32, 0.925 + 0.0025 * j as f32);
                xyz.push([2.0, y, z]);
                sensor.push([0.0, 0.0, 1.0]);
            }
        }
        // The stray: on the way to the wall, in the middle of the rays.
        xyz.push([1.5, 0.0, 1.0]);
        sensor.push([0.0, 0.0, 1.0]);
        // And something for the floor finder.
        for i in 0..100 {
            for j in 0..100 {
                xyz.push([0.02 * i as f32, -1.0 + 0.02 * j as f32, 0.0]);
                sensor.push([1.0, 0.0, 1.5]);
            }
        }
        let n = xyz.len();
        let scan = Scan::new("t", xyz, vec![0.5; n], Some(sensor), None, 0.1).unwrap();
        let idx: Vec<u32> = vec![30 * 60 + 30, 3600];
        let c = confidence(&scan, &idx);
        assert!(c[0] > 0.9, "the wall: {}", c[0]);
        assert!(c[1] < 0.2, "the stray: {}", c[1]);
    }

    #[test]
    fn rays_skimming_along_a_wall_do_not_doubt_it() {
        // A wall at x = 2, seen square on from the origin's side and almost edge on from far along it (those rays
        // pass a few millimetres in front of its near part on the way to its far part).
        let (mut xyz, mut sensor) = (Vec::new(), Vec::new());
        for i in 0..200 {
            for j in 0..60 {
                let (y, z) = (0.005 * i as f32, 0.85 + 0.005 * j as f32);
                xyz.push([2.0, y, z]);
                sensor.push(if (i + j) % 2 == 0 { [0.0, 0.5, 1.0] } else { [1.9, -6.0, 1.0] });
            }
        }
        for i in 0..100 {
            for j in 0..100 {
                xyz.push([0.02 * i as f32, -1.0 + 0.02 * j as f32, 0.0]);
                sensor.push([1.0, 0.0, 1.5]);
            }
        }
        let n = xyz.len();
        let scan = Scan::new("t", xyz, vec![0.5; n], Some(sensor), None, 0.1).unwrap();
        let idx: Vec<u32> = (0..200 * 60).collect();
        let c = confidence(&scan, &idx);
        let low = c.iter().filter(|&&x| x < 0.5).count();
        assert!(low * 50 < idx.len(), "{low} of {} wall points doubted", idx.len());
    }

    #[test]
    fn the_lidar_flags_lower_confidence() {
        assert_eq!(tag_factor(0), 1.0);
        assert!(tag_factor(0b01) < tag_factor(0b10) && tag_factor(0b10) < tag_factor(0b11) && tag_factor(0b11) < 1.0);
        assert!(tag_factor(0b0100) < 1.0 && tag_factor(0b0100) >= 0.5);
        // The return number does not count.
        assert_eq!(tag_factor(0b01_0000), 1.0);
    }

    #[test]
    fn a_weighted_plane_ignores_what_has_no_weight() {
        let mut pts: Vec<V3> = (0..100).map(|k| [(k % 10) as f32 * 0.1, (k / 10) as f32 * 0.1, 0.0]).collect();
        let mut w = vec![1.0; 100];
        pts.extend((0..20).map(|k| [k as f32 * 0.05, 0.5, 0.3]));
        w.extend([0.0; 20]);
        let (c, n, rms) = weighted_plane(&pts, &w).unwrap();
        assert!(n[2].abs() > 0.9999 && c[2].abs() < 1e-6 && rms < 1e-6, "{c:?} {n:?} {rms}");
    }
}
