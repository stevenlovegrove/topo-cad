//! The scan's own axes, from the scan alone: no model is used. Every small cell of the scan
//! that is a surface gives a normal, with a weight from its own points only (how many, how
//! flat, how far they spread in the plane: how well they say which way it faces). A building's
//! normals gather about six directions, square to each other (a Manhattan frame: Coughlan and
//! Yuille; fitted on the sphere of normals as in Straub et al., "A Mixture of Manhattan
//! Frames", where each gathering is a von Mises-Fisher lobe). One rotation is fitted to all
//! six lobes together; then each lobe is looked at for itself: where a lobe sits off its axis,
//! those surfaces are not square to the rest (a floor laid to fall, walls that lean), and what
//! is not flat, or noise, is spread thin and does not gather.

use rayon::prelude::*;

use crate::scan::Scan;
use crate::stats::point_stats;
use crate::V3;

/// A small surface of the scan: where, which way it faces (toward the sensor, when rays are
/// known), and how much its points say so.
#[derive(Clone, Copy, Debug)]
pub struct Surfel {
    pub at: V3,
    pub n: [f64; 3],
    pub w: f64,
}

/// The surfaces of the scan by cubes of side `cell`: each cube whose points are a surface
/// (60 or more, thin to 15 mm and to a quarter of their lesser spread in the plane: a scanned
/// floor is some 10 mm thick, from its own unevenness and the passes not quite agreeing). Its
/// weight is the inverse of the variance of its normal's direction the way it is least sure,
/// N σ₂² / σ₃² (capped: no cube counts for more than 900 points' worth each).
pub fn surfels(scan: &Scan, cell: f32) -> Vec<Surfel> {
    let key = |p: V3| ((p[0] / cell).floor() as i32, (p[1] / cell).floor() as i32, (p[2] / cell).floor() as i32);
    let mut order: Vec<((i32, i32, i32), u32)> = scan.used.par_iter().map(|&i| (key(scan.xyz[i as usize]), i)).collect();
    order.par_sort_unstable();
    let mut runs: Vec<(usize, usize)> = vec![];
    let mut a = 0;
    for k in 1..=order.len() {
        if k == order.len() || order[k].0 != order[a].0 {
            runs.push((a, k));
            a = k;
        }
    }
    runs.par_iter()
        .filter_map(|&(a, b)| {
            let run = &order[a..b];
            if run.len() < 60 {
                return None;
            }
            let st = point_stats(run.iter().map(|r| scan.xyz[r.1 as usize]));
            let [_, s2, s3] = st.sigma;
            if s3 > 0.015 || s3 > s2 / 4.0 || s2 < cell / 8.0 {
                return None;
            }
            let mut n = st.axes[2].map(f64::from);
            if let Some(sensor) = &scan.sensor {
                let i = run[run.len() / 2].1 as usize;
                let to = crate::sub(sensor[i], scan.xyz[i]).map(f64::from);
                if n[0] * to[0] + n[1] * to[1] + n[2] * to[2] < 0.0 {
                    n = n.map(|x| -x);
                }
            }
            let (s2, s3) = (s2 as f64, (s3 as f64).max(0.0005));
            Some(Surfel { at: st.centroid, n, w: run.len() as f64 * (s2 * s2 / (s3 * s3)).min(900.0) })
        })
        .collect()
}

type M3 = [[f64; 3]; 3];

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn unit(a: [f64; 3]) -> [f64; 3] {
    let l = dot(a, a).sqrt();
    a.map(|x| x / l)
}
fn solve3(a: &M3, b: [f64; 3]) -> Option<[f64; 3]> {
    let det = dot(a[0], cross(a[1], a[2]));
    if det.abs() < 1e-12 {
        return None;
    }
    let col = |k: usize| [a[0][k], a[1][k], a[2][k]];
    let (c0, c1, c2) = (col(0), col(1), col(2));
    Some([dot(b, cross(c1, c2)) / det, dot(c0, cross(b, c2)) / det, dot(c0, cross(c1, b)) / det])
}

/// The frame (three unit axes, square to each other, as rows) whose six directions the normals
/// gather about best, from `start`: each normal within the gate of one of the six pulls that
/// axis toward itself, by its weight; the gate narrows from 10° to 2°.
pub fn fit(normals: &[Surfel], start: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut f = start;
    for gate in [10.0f64, 6.0, 4.0, 3.0, 2.0, 2.0] {
        let cos = gate.to_radians().cos();
        for _ in 0..4 {
            // In the frame: m = F n; its axis a; the small turn ω with m + ω × m ≈ a over all:
            // (Σ w (I − m mᵀ)) ω = Σ w m × a.
            let (a, b) = normals
                .par_iter()
                .fold(
                    || ([[0.0; 3]; 3], [0.0; 3]),
                    |(mut a, mut b), s| {
                        let m = [dot(f[0], s.n), dot(f[1], s.n), dot(f[2], s.n)];
                        let k = (0..3).max_by(|&i, &j| m[i].abs().total_cmp(&m[j].abs())).unwrap();
                        if m[k].abs() < cos {
                            return (a, b);
                        }
                        let mut axis = [0.0; 3];
                        axis[k] = m[k].signum();
                        let c = cross(m, axis);
                        for i in 0..3 {
                            b[i] += s.w * c[i];
                            for j in 0..3 {
                                a[i][j] += s.w * (if i == j { 1.0 } else { 0.0 } - m[i] * m[j]);
                            }
                        }
                        (a, b)
                    },
                )
                .reduce(
                    || ([[0.0; 3]; 3], [0.0; 3]),
                    |(mut a, mut b), (c, d)| {
                        for i in 0..3 {
                            b[i] += d[i];
                            for j in 0..3 {
                                a[i][j] += c[i][j];
                            }
                        }
                        (a, b)
                    },
                );
            let Some(w) = solve3(&a, b) else { return f };
            // m' = m + ω × m for every normal is the frame's axes turned the other way: each row r
            // (an axis, in the scan) becomes r − ω_frame × ... taken in the scan: F ← (I + [ω]×) F.
            let rows = f;
            for i in 0..3 {
                // (Row i of [ω]× F.)
                let wx = [[0.0, -w[2], w[1]], [w[2], 0.0, -w[0]], [-w[1], w[0], 0.0]];
                for j in 0..3 {
                    f[i][j] = rows[i][j] + (0..3).map(|k| wx[i][k] * rows[k][j]).sum::<f64>();
                }
            }
            // (Square again.)
            f[0] = unit(f[0]);
            f[1] = unit(cross(f[2], f[0]));
            f[2] = cross(f[0], f[1]);
            if dot(w, w).sqrt() < 1e-7 {
                break;
            }
        }
    }
    f
}

/// One of the six lobes, looked at for itself, in a frame given (`hx`, `hy`, `up`, as rows): how
/// its surfaces sit off square. `a`, `b` are small, as mm per metre would be over 1000:
/// - facing up or down (`axis` 2): the surface rises `a` along hx and `b` along hy;
/// - facing along hx (0) or hy (1): it is turned `a` anticlockwise seen from above, and its top
///   leans `b` the way of that axis (+hx, or +hy).
#[derive(Clone, Copy, Debug)]
pub struct Lobe {
    pub axis: usize,
    /// Whether it faces the axis's positive way.
    pub positive: bool,
    /// Its share of all the weight within 2° of any axis.
    pub share: f64,
    /// Where it gathers: the weighted median each way, then (three times) the weighted mean of
    /// what is within 6 mm/m of that.
    pub a: f64,
    pub b: f64,
    /// How far what is within 6 mm/m of there spreads (rms), the share of the lobe that is, and
    /// how many surfaces that is.
    pub spread: f64,
    pub core: f64,
    pub count: usize,
}

/// A normal's two small numbers in its lobe (see `Lobe`), if it is within `gate` (as a tangent) of an axis.
pub fn off_axis(frame: &[[f64; 3]; 3], n: [f64; 3], gate: f64) -> Option<(usize, bool, f64, f64)> {
    let m = [dot(frame[0], n), dot(frame[1], n), dot(frame[2], n)];
    let k = (0..3).max_by(|&i, &j| m[i].abs().total_cmp(&m[j].abs())).unwrap();
    let (a, b) = match k {
        0 => (m[1] / m[0], -m[2] / m[0]),
        1 => (-m[0] / m[1], -m[2] / m[1]),
        _ => (-m[0] / m[2], -m[1] / m[2]),
    };
    (a.abs() < gate && b.abs() < gate).then_some((k, m[k] > 0.0, a, b))
}

/// The six lobes in `frame`, and each lobe's weighted histogram (bins of `bin`, out to `gate`
/// either way, row by row from −gate) for drawing.
pub fn lobes(normals: &[Surfel], frame: &[[f64; 3]; 3], gate: f64, bin: f64) -> Vec<(Lobe, Vec<f64>, usize)> {
    let side = (2.0 * gate / bin).round() as usize;
    let mut hist = vec![vec![0.0f64; side * side]; 6];
    let mut pts: Vec<Vec<(f64, f64, f64)>> = vec![vec![]; 6];
    for s in normals {
        if let Some((k, pos, a, b)) = off_axis(frame, s.n, gate) {
            let l = 2 * k + usize::from(!pos);
            let (i, j) = (((a + gate) / bin) as usize, ((b + gate) / bin) as usize);
            if i < side && j < side {
                hist[l][j * side + i] += s.w;
                pts[l].push((a, b, s.w));
            }
        }
    }
    let total: f64 = pts.iter().flatten().map(|p| p.2).sum::<f64>().max(1e-12);
    (0..6)
        .map(|l| {
            let w: f64 = pts[l].iter().map(|p| p.2).sum();
            let median = |which: usize| -> f64 {
                let mut v: Vec<(f64, f64)> = pts[l].iter().map(|p| (if which == 0 { p.0 } else { p.1 }, p.2)).collect();
                v.sort_by(|x, y| x.0.total_cmp(&y.0));
                let mut acc = 0.0;
                v.iter().find(|x| { acc += x.1; acc >= w / 2.0 }).map_or(0.0, |x| x.0)
            };
            let (mut a, mut b) = (median(0), median(1));
            let (mut spread, mut core, mut count) = (0.0, 0.0, 0);
            for _ in 0..3 {
                let near: Vec<&(f64, f64, f64)> = pts[l].iter().filter(|p| (p.0 - a).hypot(p.1 - b) < 0.006).collect();
                let ww: f64 = near.iter().map(|p| p.2).sum();
                if ww <= 0.0 {
                    break;
                }
                a = near.iter().map(|p| p.0 * p.2).sum::<f64>() / ww;
                b = near.iter().map(|p| p.1 * p.2).sum::<f64>() / ww;
                spread = (near.iter().map(|p| ((p.0 - a).powi(2) + (p.1 - b).powi(2)) * p.2).sum::<f64>() / ww).sqrt();
                core = ww / w.max(1e-12);
                count = near.len();
            }
            (Lobe { axis: l / 2, positive: l % 2 == 0, share: w / total, a, b, spread, core, count }, hist[l].clone(), side)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turned(v: [f64; 3], w: [f64; 3]) -> [f64; 3] {
        unit([v[0] + w[1] * v[2] - w[2] * v[1], v[1] + w[2] * v[0] - w[0] * v[2], v[2] + w[0] * v[1] - w[1] * v[0]])
    }

    #[test]
    fn the_frame_is_found_and_a_lobe_off_its_axis_is_seen() {
        // A box of a room, turned a little all together; its floor laid to fall 6 mm per metre along x besides.
        let all = [0.004, -0.003, 0.02];
        let mut ns = vec![];
        let mut k = 0u32;
        let mut noise = || {
            k = k.wrapping_mul(1664525).wrapping_add(1013904223);
            (k >> 8) as f64 / (1u32 << 24) as f64 - 0.5
        };
        for (axis, weight) in [([1.0, 0.0, 0.0], 1.0), ([-1.0, 0.0, 0.0], 1.0), ([0.0, 1.0, 0.0], 1.0), ([0.0, -1.0, 0.0], 1.0), ([0.0, 0.0, 1.0], 2.0)] {
            for _ in 0..400 {
                let mut n = [axis[0] + 0.002 * noise(), axis[1] + 0.002 * noise(), axis[2] + 0.002 * noise()];
                if axis[2] > 0.5 {
                    // (z = 0.006 x: its normal leans back along x.)
                    n[0] -= 0.006;
                }
                ns.push(Surfel { at: [0.0; 3], n: turned(unit(n), all), w: weight });
            }
        }
        // And clutter, facing every way.
        for _ in 0..300 {
            ns.push(Surfel { at: [0.0; 3], n: unit([noise(), noise(), noise()]), w: 1.0 });
        }
        let f = fit(&ns, [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
        // Looked at with the walls' up (the frame the walls alone give): the floor rises 6 mm/m along x; the walls are plumb.
        let up = plumb(&upright(&ns, &f, 0.035), f[2]);
        let hx = unit(cross(cross(up, f[0]), up));
        let frame = [hx, cross(up, hx), up];
        let ls = lobes(&ns, &frame, 0.035, 0.00025);
        let floor = ls.iter().find(|l| l.0.axis == 2 && l.0.positive).unwrap().0;
        assert!((floor.a - 0.006).abs() < 3e-4 && floor.b.abs() < 3e-4, "{floor:?}");
        for l in ls.iter().filter(|l| l.0.axis < 2) {
            assert!(l.0.b.abs() < 3e-4 && l.0.a.abs() < 3e-4, "{:?}", l.0);
        }
    }
}

/// The surfaces that face along one of a frame's two level axes, within `gate` (as a tangent):
/// the upright ones, without what faces every other way.
pub fn upright(normals: &[Surfel], frame: &[[f64; 3]; 3], gate: f64) -> Vec<Surfel> {
    normals.iter().copied().filter(|s| off_axis(frame, s.n, gate).is_some_and(|x| x.0 < 2)).collect()
}

/// Straight up as the upright surfaces have it: the direction they are all most nearly square
/// to (the least of Σ w n nᵀ over them), the way of `toward`.
pub fn plumb(upright: &[Surfel], toward: [f64; 3]) -> [f64; 3] {
    let mut a = [[0.0; 3]; 3];
    for s in upright {
        for i in 0..3 {
            for j in 0..3 {
                a[i][j] += s.w * s.n[i] * s.n[j];
            }
        }
    }
    let (vals, vecs) = crate::stats::eigen_sym3(a);
    let k = (0..3).min_by(|&i, &j| vals[i].total_cmp(&vals[j])).unwrap();
    let v = [vecs[0][k], vecs[1][k], vecs[2][k]];
    if dot(v, toward) < 0.0 {
        v.map(|x| -x)
    } else {
        v
    }
}
