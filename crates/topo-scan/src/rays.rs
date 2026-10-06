//! Free space from the sensor rays: each point was seen along the segment
//! from the sensor to it, so that segment is empty. A real member blocks the
//! rays behind it; a false one (mixed returns trailing an edge, the laps of
//! a roof sheet, a chance alignment of clutter) is crossed by rays that end
//! beyond it.

use rayon::prelude::*;

use crate::{add, dot, sub, Obb, V3};

/// Per box: rays that pass through it (entering it shrunk by `margin` and
/// ending beyond it grown by `margin`), and rays that end in it (grown).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RayStats {
    pub pierced: u32,
    pub ended: u32,
}

impl RayStats {
    /// The fraction of rays reaching the box that pass through it.
    pub fn pierce_frac(&self) -> f32 {
        self.pierced as f32 / (self.pierced + self.ended).max(1) as f32
    }
}

/// What the ray from the sensor `s` to its point `p` does at a box (centre,
/// unit axes, half sizes): `Some(false)` ends in it (grown by `margin`),
/// `Some(true)` passes through it (shrunk by `margin`) to end beyond, `None`
/// misses it.
pub fn ray_meets(centre: V3, axes: &[V3; 3], h: V3, margin: f32, s: V3, p: V3) -> Option<bool> {
    // In the box frame.
    let (ds, dp) = (sub(s, centre), sub(p, centre));
    let a: [f32; 3] = std::array::from_fn(|x| dot(ds, axes[x]));
    let e: [f32; 3] = std::array::from_fn(|x| dot(dp, axes[x]));
    if (0..3).all(|x| e[x].abs() <= h[x] + margin) {
        return Some(false);
    }
    // Slab test of the segment against the shrunk box.
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for x in 0..3 {
        let hs = (h[x] - margin).max(0.5 * h[x]);
        let d = e[x] - a[x];
        if d.abs() < 1e-9 {
            if a[x].abs() > hs {
                return None;
            }
        } else {
            let (u0, u1) = ((-hs - a[x]) / d, (hs - a[x]) / d);
            t0 = t0.max(u0.min(u1));
            t1 = t1.min(u0.max(u1));
            if t0 > t1 {
                return None;
            }
        }
    }
    Some(true)
}

/// Free space in a box (centre, unit axes, half sizes), as a picture over its
/// first two axes: per cell of a `w` × `h` grid, how many rays (of every
/// `stride`-th point of `used`) pass over it inside the box, from the sensor
/// to where they end. Row 0 is the low side of the second axis. Where rays
/// passed, nothing was in the way; a cell no ray crosses is solid or unseen.
pub fn free_space(xyz: &[V3], sensor: &[V3], used: &[u32], stride: usize, centre: V3, axes: &[V3; 3], half: V3, w: usize, h: usize) -> Vec<u32> {
    let (sx, sy) = (w as f32 / (2.0 * half[0]), h as f32 / (2.0 * half[1]));
    used.par_iter()
        .step_by(stride.max(1))
        .fold(
            || vec![0u32; w * h],
            |mut grid, &i| {
                let (s, p) = (sensor[i as usize], xyz[i as usize]);
                if !s[0].is_finite() {
                    return grid;
                }
                let (ds, dp) = (sub(s, centre), sub(p, centre));
                let a: [f32; 3] = std::array::from_fn(|x| dot(ds, axes[x]));
                let e: [f32; 3] = std::array::from_fn(|x| dot(dp, axes[x]));
                // The part of the ray inside the box.
                let (mut t0, mut t1) = (0.0f32, 1.0f32);
                for x in 0..3 {
                    let d = e[x] - a[x];
                    if d.abs() < 1e-9 {
                        if a[x].abs() > half[x] {
                            return grid;
                        }
                    } else {
                        let (u0, u1) = ((-half[x] - a[x]) / d, (half[x] - a[x]) / d);
                        t0 = t0.max(u0.min(u1));
                        t1 = t1.min(u0.max(u1));
                        if t0 >= t1 {
                            return grid;
                        }
                    }
                }
                let at = |t: f32| ((a[0] + t * (e[0] - a[0]) + half[0]) * sx, (a[1] + t * (e[1] - a[1]) + half[1]) * sy);
                let ((x0, y0), (x1, y1)) = (at(t0), at(t1));
                // A cell at a time along it (each cell once per ray, near enough).
                let n = (x1 - x0).abs().max((y1 - y0).abs()).ceil().max(1.0) as usize;
                let mut last = usize::MAX;
                for k in 0..=n {
                    let f = k as f32 / n as f32;
                    let (x, y) = ((x0 + f * (x1 - x0)) as usize, (y0 + f * (y1 - y0)) as usize);
                    let c = x.min(w - 1) + w * y.min(h - 1);
                    if c != last {
                        grid[c] += 1;
                        last = c;
                    }
                }
                grid
            },
        )
        .reduce(
            || vec![0u32; w * h],
            |mut a, b| {
                for (x, y) in a.iter_mut().zip(b) {
                    *x += y;
                }
                a
            },
        )
}

/// Ray statistics of `boxes` over every `stride`-th point of `used`.
pub fn ray_stats(boxes: &[Obb], xyz: &[V3], sensor: &[V3], used: &[u32], stride: usize, margin: f32) -> Vec<RayStats> {
    if boxes.is_empty() {
        return Vec::new();
    }
    // World bounds of each box, grown, for a quick reject.
    let bounds: Vec<(V3, V3)> = boxes
        .iter()
        .map(|b| {
            let c = b.corners();
            let lo = c.iter().fold([f32::MAX; 3], |a, p| [a[0].min(p[0]), a[1].min(p[1]), a[2].min(p[2])]);
            let hi = c.iter().fold([f32::MIN; 3], |a, p| [a[0].max(p[0]), a[1].max(p[1]), a[2].max(p[2])]);
            (sub(lo, [margin; 3]), add(hi, [margin; 3]))
        })
        .collect();
    let half: Vec<[f32; 3]> = boxes.iter().map(|b| [b.length / 2.0, b.width / 2.0, b.depth / 2.0]).collect();
    used.par_iter()
        .step_by(stride.max(1))
        .fold(
            || vec![RayStats::default(); boxes.len()],
            |mut acc, &i| {
                let (s, p) = (sensor[i as usize], xyz[i as usize]);
                if !s[0].is_finite() {
                    return acc;
                }
                let (rlo, rhi) = ([s[0].min(p[0]), s[1].min(p[1]), s[2].min(p[2])], [s[0].max(p[0]), s[1].max(p[1]), s[2].max(p[2])]);
                for (k, b) in boxes.iter().enumerate() {
                    let (lo, hi) = bounds[k];
                    if (0..3).any(|a| rhi[a] < lo[a] || rlo[a] > hi[a]) {
                        continue;
                    }
                    match ray_meets(b.centre, &b.axes, half[k], margin, s, p) {
                        Some(true) => acc[k].pierced += 1,
                        Some(false) => acc[k].ended += 1,
                        None => {}
                    }
                }
                acc
            },
        )
        .reduce(
            || vec![RayStats::default(); boxes.len()],
            |mut a, b| {
                for k in 0..a.len() {
                    a[k].pierced += b[k].pierced;
                    a[k].ended += b[k].ended;
                }
                a
            },
        )
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_space_is_where_rays_pass_in_the_box() {
        // A box 4 × 2 × 0.2 about the origin; one ray along x at y = 0.5 from x = -1 to x = 1, in its plane.
        let axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let g = free_space(&[[1.0, 0.5, 0.0], [0.0, 0.0, 5.0]], &[[-1.0, 0.5, 0.0], [0.0, 0.0, 4.0]], &[0, 1], 1, [0.0; 3], &axes, [2.0, 1.0, 0.1], 8, 4);
        // Cells are 0.5 m: x from -1 to 1 is columns 2 to 6 (the end lands on a cell's edge); y = 0.5 is row 3. The second ray misses the box.
        let row: Vec<u32> = g[3 * 8..4 * 8].to_vec();
        assert_eq!(row, vec![0, 0, 1, 1, 1, 1, 1, 0]);
        assert_eq!(g.iter().sum::<u32>(), 5);
    }

    #[test]
    fn counts_rays_through_and_into_a_box() {
        let b = Obb { centre: [0.0, 0.0, 1.0], axes: [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], length: 2.0, width: 0.04, depth: 0.09 };
        let xyz = vec![[0.0, 0.03, 1.0], [0.0, 1.0, 1.0], [0.5, 1.0, 1.0]];
        let sensor = vec![[0.0, -2.0, 1.0]; 3];
        let r = ray_stats(&[b], &xyz, &sensor, &[0, 1, 2], 1, 0.01);
        // Ends in it; passes through it (to y = 1); misses it.
        assert_eq!(r[0], RayStats { pierced: 1, ended: 1 });
    }
}
