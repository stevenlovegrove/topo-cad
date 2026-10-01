//! Summaries of point sets: extent, centroid, a least-squares plane.

use serde::{Deserialize, Serialize};

use crate::V3;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlaneFit {
    pub normal: V3,
    /// Offset: n·p = d.
    pub d: f32,
    pub rms: f32,
}

#[derive(Debug, Clone, Default)]
pub struct PointStats {
    pub count: usize,
    pub centroid: V3,
    pub min: V3,
    pub max: V3,
    /// The best plane, when there are three points or more.
    pub plane: Option<PlaneFit>,
    /// Spread along the principal axes (standard deviations, largest first),
    /// and those axes.
    pub sigma: V3,
    pub axes: [V3; 3],
}

pub fn point_stats(pts: impl Iterator<Item = V3>) -> PointStats {
    let (mut n, mut s, mut ss) = (0usize, [0.0f64; 3], [[0.0f64; 3]; 3]);
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in pts {
        n += 1;
        for a in 0..3 {
            s[a] += p[a] as f64;
            lo[a] = lo[a].min(p[a]);
            hi[a] = hi[a].max(p[a]);
            for b in 0..3 {
                ss[a][b] += p[a] as f64 * p[b] as f64;
            }
        }
    }
    if n == 0 {
        return PointStats::default();
    }
    let m = s.map(|x| x / n as f64);
    let mut cov = [[0.0f64; 3]; 3];
    for a in 0..3 {
        for b in 0..3 {
            cov[a][b] = ss[a][b] / n as f64 - m[a] * m[b];
        }
    }
    let (vals, vecs) = eigen_sym3(cov);
    // Largest first.
    let mut order = [0, 1, 2];
    order.sort_by(|&i, &j| vals[j].total_cmp(&vals[i]));
    let axes = order.map(|k| [vecs[0][k] as f32, vecs[1][k] as f32, vecs[2][k] as f32]);
    let sigma = order.map(|k| vals[k].max(0.0).sqrt() as f32);
    let centroid = m.map(|x| x as f32);
    let plane = (n >= 3).then(|| {
        let normal = axes[2];
        PlaneFit { normal, d: crate::dot(normal, centroid), rms: sigma[2] }
    });
    PointStats { count: n, centroid, min: lo, max: hi, plane, sigma, axes }
}

/// Eigen-decomposition of a symmetric 3×3 matrix (cyclic Jacobi): the
/// values, and the vectors as the columns.
pub fn eigen_sym3(mut a: [[f64; 3]; 3]) -> ([f64; 3], [[f64; 3]; 3]) {
    let mut v = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for _ in 0..50 {
        let off = a[0][1].abs() + a[0][2].abs() + a[1][2].abs();
        if off < 1e-15 {
            break;
        }
        for (p, q) in [(0, 1), (0, 2), (1, 2)] {
            if a[p][q].abs() < 1e-300 {
                continue;
            }
            let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
            let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
            let t = if theta == 0.0 { 1.0 } else { t };
            let c = 1.0 / (t * t + 1.0).sqrt();
            let s = t * c;
            for k in 0..3 {
                let (akp, akq) = (a[k][p], a[k][q]);
                a[k][p] = c * akp - s * akq;
                a[k][q] = s * akp + c * akq;
            }
            for k in 0..3 {
                let (apk, aqk) = (a[p][k], a[q][k]);
                a[p][k] = c * apk - s * aqk;
                a[q][k] = s * apk + c * aqk;
            }
            for k in 0..3 {
                let (vkp, vkq) = (v[k][p], v[k][q]);
                v[k][p] = c * vkp - s * vkq;
                v[k][q] = s * vkp + c * vkq;
            }
        }
    }
    ([a[0][0], a[1][1], a[2][2]], v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fits_a_tilted_plane() {
        let n = [0.0f32, 0.6, 0.8];
        let (u, w) = ([1.0f32, 0.0, 0.0], [0.0f32, 0.8, -0.6]);
        let pts: Vec<V3> = (0..400).map(|k| {
            let (a, b) = ((k % 20) as f32 * 0.1, (k / 20) as f32 * 0.05);
            [u[0] * a + w[0] * b + 2.0 * n[0], u[1] * a + w[1] * b + 2.0 * n[1], u[2] * a + w[2] * b + 2.0 * n[2]]
        }).collect();
        let s = point_stats(pts.into_iter());
        let p = s.plane.unwrap();
        assert!(crate::dot(p.normal, n).abs() > 0.9999, "{:?}", p.normal);
        assert!((p.d * crate::dot(p.normal, n).signum() - 2.0).abs() < 1e-3, "{}", p.d);
        assert!(p.rms < 1e-3);
    }
}
