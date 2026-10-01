//! Selections: sets of scan points someone marked, to pin an issue to, to
//! measure, or to fit a surface from. A selection is kept as how it was
//! made (a window in a view, or a box), not as a list of points: that is
//! small, lives in the repository with the rest, and gives the same points
//! again from the same scan.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::scan::Scan;
use crate::stats::{point_stats, PlaneFit};
use crate::V3;

/// How a selection's points are chosen (scan frame).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "how", rename_all = "snake_case")]
pub enum How {
    /// The points drawn inside a rectangle of a view.
    Window {
        /// The view's projection (column-major 4×4: scan frame → clip space).
        vp: [f32; 16],
        /// The rectangle in normalised device coordinates: x0, y0, x1, y1.
        rect: [f32; 4],
        /// The viewer and the direction of view, for which points are in front.
        eye: V3,
        forward: V3,
        /// The view's cuts: heights (scan z), and a section's slab (origin, unit normal, half thickness).
        #[serde(default)]
        z: Option<[f32; 2]>,
        #[serde(default)]
        slab: Option<(V3, V3, f32)>,
        /// A view's clip box (centre, unit axes, half sizes).
        #[serde(default)]
        obb: Option<(V3, [V3; 3], V3)>,
        /// Only the surface nearest the viewer (what was seen), not what lies behind it.
        #[serde(default)]
        front_only: bool,
    },
    /// The points in a box.
    Box {
        min: V3,
        max: V3,
        /// The frame the box is in, as a placement in the scan (yaw in radians, translation): the model as it was placed. None: the scan frame.
        #[serde(default)]
        frame: Option<(f32, V3)>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Selection {
    pub id: String,
    #[serde(default)]
    pub label: String,
    pub by: String,
    pub time: f64,
    #[serde(flatten)]
    pub how: How,
    /// Of its points, when made.
    pub count: usize,
    pub centroid: V3,
    pub min: V3,
    pub max: V3,
    pub plane: Option<PlaneFit>,
}

/// The used points a selection takes.
pub fn select(scan: &Scan, how: &How) -> Vec<u32> {
    match how {
        How::Box { min, max, frame } => {
            let pl = frame.map(|(yaw, t)| crate::Placement { yaw, t });
            scan.used.par_iter().copied().filter(|&i| {
                let p = pl.map_or(scan.xyz[i as usize], |pl| pl.inverse(scan.xyz[i as usize]));
                (0..3).all(|a| p[a] >= min[a] && p[a] <= max[a])
            }).collect()
        }
        How::Window { vp, rect, eye, forward, z, slab, obb, front_only } => {
            let (x0, x1, y0, y1) = (rect[0].min(rect[2]), rect[0].max(rect[2]), rect[1].min(rect[3]), rect[1].max(rect[3]));
            // (index, x and y in the rectangle as 0..1, depth along the view).
            let inside: Vec<(u32, f32, f32, f32)> = scan
                .used
                .par_iter()
                .filter_map(|&i| {
                    let p = scan.xyz[i as usize];
                    if z.is_some_and(|z| p[2] < z[0] || p[2] > z[1]) || slab.is_some_and(|(o, n, half)| crate::dot(n, crate::sub(p, o)).abs() > half) || obb.is_some_and(|(c, axes, half)| !crate::cloud::inside(p, c, &axes, half)) {
                        return None;
                    }
                    let w = vp[3] * p[0] + vp[7] * p[1] + vp[11] * p[2] + vp[15];
                    if w <= 1e-6 {
                        return None;
                    }
                    let (x, y) = ((vp[0] * p[0] + vp[4] * p[1] + vp[8] * p[2] + vp[12]) / w, (vp[1] * p[0] + vp[5] * p[1] + vp[9] * p[2] + vp[13]) / w);
                    (x >= x0 && x <= x1 && y >= y0 && y <= y1).then(|| (i, (x - x0) / (x1 - x0).max(1e-9), (y - y0) / (y1 - y0).max(1e-9), crate::dot(crate::sub(p, *eye), *forward)))
                })
                .collect();
            if !front_only {
                return inside.into_iter().map(|p| p.0).collect();
            }
            // The nearest surface: what is within 8 cm of the nearest point in its cell of the
            // rectangle or the cells around it (a surface seen through the gaps between a
            // nearer one's points is behind it all the same).
            const N: usize = 64;
            let at = |p: &(u32, f32, f32, f32)| (((p.1 * N as f32) as usize).min(N - 1), ((p.2 * N as f32) as usize).min(N - 1));
            let mut near = vec![f32::MAX; N * N];
            for p in &inside {
                let (x, y) = at(p);
                near[x + N * y] = near[x + N * y].min(p.3);
            }
            let around: Vec<f32> = (0..N * N)
                .map(|c| {
                    let (x, y) = (c % N, c / N);
                    let mut d = f32::MAX;
                    for yy in y.saturating_sub(1)..(y + 2).min(N) {
                        for xx in x.saturating_sub(1)..(x + 2).min(N) {
                            d = d.min(near[xx + N * yy]);
                        }
                    }
                    d
                })
                .collect();
            inside.into_iter().filter(|p| { let (x, y) = at(p); p.3 <= around[x + N * y] + 0.08 }).map(|p| p.0).collect()
        }
    }
}

impl Selection {
    pub fn new(id: String, by: &str, how: How, scan: &Scan, idx: &[u32]) -> Selection {
        let st = point_stats(idx.iter().map(|&i| scan.xyz[i as usize]));
        Selection { id, label: String::new(), by: by.to_string(), time: crate::issues::now(), how, count: st.count, centroid: st.centroid, min: st.min, max: st.max, plane: st.plane }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_takes_what_is_in_front() {
        // Two walls facing the viewer at x = 2 and x = 4, seen from the origin along +x (orthographic, 2 m tall).
        let mut xyz = Vec::new();
        for j in 0..100 {
            for k in 0..100 {
                let (y, z) = (-1.0 + 0.02 * j as f32, 0.5 + 0.02 * k as f32);
                xyz.push([2.0, y, z]);
                xyz.push([4.0, y * 1.5, z]);
            }
        }
        // A floor so the scan has one.
        for i in 0..50 {
            for j in 0..50 {
                xyz.push([0.1 * i as f32, -2.5 + 0.1 * j as f32, 0.0]);
            }
        }
        let scan = Scan::new("t", xyz, Vec::new(), None, None, 1.0).unwrap();
        // Looking along +x: right = −y, up = z; x is depth. Clip x = −y / 2, y = (z − 1.5).
        let vp = [0.0, 0.0, 0.0, 0.0, -0.5, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, -1.5, 0.0, 1.0];
        let window = |front_only| How::Window { vp, rect: [-0.2, -0.5, 0.2, 0.5], eye: [0.0; 3], forward: [1.0, 0.0, 0.0], z: None, slab: None, obb: None, front_only };
        let front = select(&scan, &window(true));
        assert!(!front.is_empty() && front.iter().all(|&i| scan.xyz[i as usize][0] == 2.0), "only the near wall");
        assert!(front.iter().all(|&i| scan.xyz[i as usize][1].abs() <= 0.4001 && (scan.xyz[i as usize][2] - 1.5).abs() <= 0.5001));
        let through = select(&scan, &window(false));
        assert!(through.len() > front.len() && through.iter().any(|&i| scan.xyz[i as usize][0] == 4.0));
        let s = Selection::new("s1".into(), "p", window(true), &scan, &front);
        assert!(s.plane.as_ref().is_some_and(|p| p.normal[0].abs() > 0.999 && p.rms < 1e-4), "{:?}", s.plane);
        // As a box.
        let b = select(&scan, &How::Box { min: [1.9, -0.105, 0.995], max: [2.1, 0.105, 1.205], frame: None });
        assert_eq!(b.len(), 11 * 11);
    }
}
