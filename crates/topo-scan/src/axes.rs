//! A scan's floor height and wall direction, to start a registration from.
//! Both frames are gravity-aligned (z up), so these are a height and a
//! direction in plan.

use std::collections::HashMap;

use crate::V3;

/// The floor's height: the lowest strong level in the histogram of heights
/// (2 cm bins; strong = at least a fifth of the strongest), refined to the
/// mean of the points within 3 cm of it.
pub fn floor_z(pts: &[V3]) -> Option<f32> {
    if pts.is_empty() {
        return None;
    }
    const BIN: f32 = 0.02;
    let (lo, hi) = pts.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p[2]), a.1.max(p[2])));
    let bins = (((hi - lo) / BIN) as usize + 1).min(1 << 20);
    let mut h = vec![0u32; bins];
    for p in pts {
        h[(((p[2] - lo) / BIN) as usize).min(bins - 1)] += 1;
    }
    // Smoothed over three bins, so a floor straddling two bins still counts.
    let sm: Vec<u32> = (0..bins).map(|k| h[k.saturating_sub(1)..(k + 2).min(bins)].iter().sum()).collect();
    let top = *sm.iter().max()?;
    let k = (0..bins).find(|&k| sm[k] * 5 >= top && (k == 0 || sm[k] >= sm[k - 1]) && (k + 1 == bins || sm[k] >= sm[k + 1]))?;
    let z = lo + (k as f32 + 0.5) * BIN;
    let (s, n) = pts.iter().filter(|p| (p[2] - z).abs() <= 0.03).fold((0.0f64, 0u32), |a, p| (a.0 + p[2] as f64, a.1 + 1));
    Some(if n > 0 { (s / n as f64) as f32 } else { z })
}

/// The direction in plan that walls run along (up to a quarter turn): of
/// the points at wall height, the direction whose two perpendicular
/// profiles are sharpest. Walls are lines in plan, so along a wall
/// direction the points pile into a few bins.
pub fn wall_axis(pts: &[V3], floor: f32) -> V3 {
    // Occupied 5 cm plan cells (each counts once: a wall is not outvoted by
    // a densely scanned patch).
    let mut cells: HashMap<(i32, i32), ()> = HashMap::new();
    for p in pts.iter().filter(|p| p[2] > floor + 0.3 && p[2] < floor + 2.0) {
        cells.insert(((p[0] / 0.05).floor() as i32, (p[1] / 0.05).floor() as i32), ());
    }
    let xy: Vec<[f32; 2]> = cells.keys().map(|k| [(k.0 as f32 + 0.5) * 0.05, (k.1 as f32 + 0.5) * 0.05]).collect();
    if xy.len() < 10 {
        return [1.0, 0.0, 0.0];
    }
    let sharpness = |th: f32| {
        let (c, s) = (th.cos(), th.sin());
        let mut sum = 0.0f64;
        for axis in [[c, s], [-s, c]] {
            let mut h: HashMap<i32, u32> = HashMap::new();
            for p in &xy {
                *h.entry(((p[0] * axis[0] + p[1] * axis[1]) / 0.05).floor() as i32).or_default() += 1;
            }
            sum += h.values().map(|&n| (n as f64) * (n as f64)).sum::<f64>();
        }
        sum
    };
    let search = |from: f32, to: f32, step: f32| {
        let n = ((to - from) / step).round() as usize;
        (0..n).map(|i| from + i as f32 * step).max_by(|a, b| sharpness(*a).total_cmp(&sharpness(*b))).unwrap_or(from)
    };
    let q = std::f32::consts::FRAC_PI_2;
    let coarse = search(0.0, q, 1f32.to_radians());
    let th = search(coarse - 1f32.to_radians(), coarse + 1f32.to_radians(), 0.05f32.to_radians());
    [th.cos(), th.sin(), 0.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 6 × 4 × 2.5 m room turned by `yaw`, its floor at `z0`: floor,
    /// ceiling and four walls on a 5 cm lattice.
    pub(crate) fn room(yaw: f32, z0: f32) -> Vec<V3> {
        let (c, s) = (yaw.cos(), yaw.sin());
        let mut pts = Vec::new();
        let mut put = |x: f32, y: f32, z: f32| pts.push([c * x - s * y + 3.0, s * x + c * y - 1.0, z + z0]);
        for i in 0..=120 {
            for j in 0..=80 {
                let (x, y) = (i as f32 * 0.05, j as f32 * 0.05);
                put(x, y, 0.0);
                put(x, y, 2.5);
            }
            for k in 0..=50 {
                let (x, z) = (i as f32 * 0.05, k as f32 * 0.05);
                put(x, 0.0, z);
                put(x, 4.0, z);
            }
        }
        for j in 0..=80 {
            for k in 0..=50 {
                let (y, z) = (j as f32 * 0.05, k as f32 * 0.05);
                put(0.0, y, z);
                put(6.0, y, z);
            }
        }
        pts
    }

    #[test]
    fn finds_the_floor_and_the_walls_direction() {
        let pts = room(20f32.to_radians(), -1.2);
        let z = floor_z(&pts).unwrap();
        assert!((z + 1.2).abs() < 0.005, "{z}");
        let a = wall_axis(&pts, z);
        let th = a[1].atan2(a[0]).to_degrees().rem_euclid(90.0);
        assert!((th - 20.0).abs() < 0.2, "{th}");
    }
}
