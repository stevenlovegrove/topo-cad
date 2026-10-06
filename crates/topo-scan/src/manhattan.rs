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
use crate::V3;

/// A small surface of the scan: where, which way it faces (toward the sensor, when rays are
/// known), and how much its points say so.
#[derive(Clone, Copy, Debug)]
pub struct Surfel {
    pub at: V3,
    pub n: [f64; 3],
    pub w: f64,
}

/// A normal at each of about `want` of the scan's points, taken evenly through them: the plane
/// of the points within `radius` of it, each weighted by its confidence over its range's σ²
/// (`confidence`: by index, as `confidence::confidence_all` gives it; none: every point alike),
/// fitted twice (the second time without what lies more than 2.5 σ off the first: an edge's other
/// side, a stray). Kept where those points are a surface: 25 or more of them in effect, thin to
/// 15 mm and to a quarter of their lesser spread in the plane, and spread both ways in it.
///
/// Its weight is how well its points say which way it faces, the inverse of the variance of its
/// direction the way it is least sure, σ₂² / σ₃² (capped at 900), times the share of its points
/// that are believed; not times how many there are, so that a surface counts by its area, not
/// by how densely the scanner happened to sample it (where it is dense, more normals are taken).
pub fn normals(scan: &Scan, confidence: Option<&[f32]>, radius: f32, want: usize) -> Vec<Surfel> {
    let key = |p: V3| ((p[0] / radius).floor() as i32, (p[1] / radius).floor() as i32, (p[2] / radius).floor() as i32);
    let mut order: Vec<((i32, i32, i32), u32)> = scan.used.par_iter().map(|&i| (key(scan.xyz[i as usize]), i)).collect();
    order.par_sort_unstable();
    let mut runs: std::collections::HashMap<(i32, i32, i32), (usize, usize)> = std::collections::HashMap::new();
    let mut a = 0;
    for k in 1..=order.len() {
        if k == order.len() || order[k].0 != order[a].0 {
            runs.insert(order[a].0, (a, k));
            a = k;
        }
    }
    let s0 = crate::confidence::sigma(0.0);
    let weight = |i: u32| -> f64 {
        let c = confidence.map_or(1.0, |c| c[i as usize]);
        let range = scan.sensor.as_ref().map_or(2.0, |s| crate::norm(crate::sub(scan.xyz[i as usize], s[i as usize])));
        (c * (s0 / crate::confidence::sigma(range)).powi(2)) as f64
    };
    let stride = (order.len() / want.max(1)).max(1) as u32;
    let r2 = radius * radius;
    order
        .par_iter()
        .filter(|x| x.1.wrapping_mul(2654435761) % stride == 0)
        .filter_map(|&(k, i)| {
            let p = scan.xyz[i as usize];
            // The points about it (at most some 600 of them, taken evenly).
            let mut near: Vec<u32> = vec![];
            for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        if let Some(&(a, b)) = runs.get(&(k.0 + dx, k.1 + dy, k.2 + dz)) {
                            near.extend(order[a..b].iter().map(|x| x.1).filter(|&j| {
                                let d = crate::sub(scan.xyz[j as usize], p);
                                crate::dot(d, d) <= r2
                            }));
                        }
                    }
                }
            }
            let count = near.len();
            let step = (count / 600).max(1);
            let pts: Vec<([f64; 3], f64)> = near.iter().step_by(step).map(|&j| (scan.xyz[j as usize].map(f64::from), weight(j))).collect();
            let plane = |pts: &[([f64; 3], f64)]| -> Option<([f64; 3], [f64; 3], f64, f64, f64)> {
                let sw: f64 = pts.iter().map(|x| x.1).sum();
                if pts.len() < 8 || sw <= 0.0 {
                    return None;
                }
                let c: [f64; 3] = std::array::from_fn(|a| pts.iter().map(|x| x.0[a] * x.1).sum::<f64>() / sw);
                let mut m = [[0.0; 3]; 3];
                for (q, w) in pts {
                    for a in 0..3 {
                        for b in 0..3 {
                            m[a][b] += w * (q[a] - c[a]) * (q[b] - c[b]) / sw;
                        }
                    }
                }
                let (vals, vecs) = crate::stats::eigen_sym3(m);
                let mut o = [0, 1, 2];
                o.sort_by(|&x, &y| vals[x].total_cmp(&vals[y]));
                let n = [vecs[0][o[0]], vecs[1][o[0]], vecs[2][o[0]]];
                let n_eff = sw * sw / pts.iter().map(|x| x.1 * x.1).sum::<f64>();
                Some((c, n, vals[o[1]].max(0.0).sqrt(), vals[o[0]].max(0.0).sqrt(), n_eff))
            };
            let (c, n, _, s3, _) = plane(&pts)?;
            let keep = 2.5 * s3.max(0.002);
            let kept: Vec<([f64; 3], f64)> = pts.iter().copied().filter(|(q, _)| ((q[0] - c[0]) * n[0] + (q[1] - c[1]) * n[1] + (q[2] - c[2]) * n[2]).abs() <= keep).collect();
            let (_, mut n, s2, s3, n_eff) = plane(&kept)?;
            if n_eff < 25.0 || s3 > 0.015 || s3 > s2 / 4.0 || s2 < radius as f64 / 5.0 {
                return None;
            }
            if let Some(sensor) = &scan.sensor {
                let to = crate::sub(sensor[i as usize], p).map(f64::from);
                if n[0] * to[0] + n[1] * to[1] + n[2] * to[2] < 0.0 {
                    n = n.map(|x| -x);
                }
            }
            let believed = n_eff / pts.len() as f64;
            Some(Surfel { at: p, n, w: believed.min(1.0) * (s2 * s2 / (s3 * s3).max(2.5e-7)).min(900.0) })
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
/// gather about best, from `start`: a rotation, all three of its freedoms fitted together. Each
/// normal within the gate of one of the six pulls that axis toward itself, by its weight; the
/// gate narrows from 10° to 2°, and from 3° on a normal pulls less the further off it is (by
/// 1 / (1 + (θ / 10 mm/m)²), Cauchy's: what gathers decides, the haze about it hardly), so the
/// frame is where the lobes' peaks are square to each other, not where their means are.
pub fn fit(normals: &[Surfel], start: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut f = start;
    for gate in [10.0f64, 6.0, 4.0, 3.0, 2.0, 2.0, 2.0] {
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
                        let w = if gate <= 3.0 { s.w / (1.0 + dot(c, c) / (0.010 * 0.010)) } else { s.w };
                        for i in 0..3 {
                            b[i] += w * c[i];
                            for j in 0..3 {
                                a[i][j] += w * (if i == j { 1.0 } else { 0.0 } - m[i] * m[j]);
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
    /// Where it gathers: the weighted median each way, then (five times) the weighted mean of
    /// what is within 15 mm/m of that.
    pub a: f64,
    pub b: f64,
    /// How far what is within 15 mm/m of there spreads (rms, each way), the share of the lobe
    /// that is, and how many normals that is.
    pub spread: f64,
    pub core: f64,
    pub count: usize,
    /// How many of those stand apart (in different cubes of `apart`): normals taken close
    /// together share their points, and say one thing between them.
    pub independent: usize,
}

impl Lobe {
    /// About how sure where it gathers is, each way: its spread over the root of how many
    /// normals stand apart.
    pub fn sure(&self) -> f64 {
        self.spread / (self.independent.max(1) as f64).sqrt()
    }
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

/// Where a direction (in a frame: its three numbers along the frame's axes) falls on the cube
/// about the frame, unfolded as a cross seen from above: the face (0: +x, 1: −x, 2: +y, 3: −y,
/// 4: up, 5: down) and where on it, right and up, each −1 to 1. The up face is in the middle
/// (x to the right, y up), +x to its right, −x to its left, +y above, −y below, and the down
/// face beyond +x; each face is hinged on the edge it shares with its neighbour.
pub fn cube_face(m: [f64; 3]) -> (usize, f64, f64) {
    let k = (0..3).max_by(|&i, &j| m[i].abs().total_cmp(&m[j].abs())).unwrap();
    let d = m[k].abs();
    match (k, m[k] > 0.0) {
        (0, true) => (0, -m[2] / d, m[1] / d),
        (0, false) => (1, m[2] / d, m[1] / d),
        (1, true) => (2, m[0] / d, -m[2] / d),
        (1, false) => (3, m[0] / d, m[2] / d),
        (_, true) => (4, m[0] / d, m[1] / d),
        (_, false) => (5, -m[0] / d, m[1] / d),
    }
}

/// The normals' distribution over the whole sphere, as weight per solid angle on the six faces
/// of the cube about `frame` (each `side` × `side`, row by row from the bottom; see `cube_face`).
pub fn sphere_histogram(normals: &[Surfel], frame: &[[f64; 3]; 3], side: usize) -> Vec<Vec<f64>> {
    let mut h = vec![vec![0.0; side * side]; 6];
    for s in normals {
        let (f, a, b) = cube_face([dot(frame[0], s.n), dot(frame[1], s.n), dot(frame[2], s.n)]);
        let (i, j) = ((((a + 1.0) / 2.0) * side as f64) as usize, (((b + 1.0) / 2.0) * side as f64) as usize);
        // (A square of the face is less of the sphere the further it is from the face's middle.)
        h[f][j.min(side - 1) * side + i.min(side - 1)] += s.w * (1.0 + a * a + b * b).powf(1.5);
    }
    h
}

/// The six lobes in `frame`, and each lobe's weighted histogram (bins of `bin`, out to `gate`
/// either way, row by row from −gate) for drawing.
pub fn lobes(normals: &[Surfel], frame: &[[f64; 3]; 3], gate: f64, bin: f64, apart: f32) -> Vec<(Lobe, Vec<f64>, usize)> {
    let side = (2.0 * gate / bin).round() as usize;
    let mut hist = vec![vec![0.0f64; side * side]; 6];
    let mut pts: Vec<Vec<(f64, f64, f64, V3)>> = vec![vec![]; 6];
    for s in normals {
        if let Some((k, pos, a, b)) = off_axis(frame, s.n, gate) {
            let l = 2 * k + usize::from(!pos);
            let (i, j) = (((a + gate) / bin) as usize, ((b + gate) / bin) as usize);
            if i < side && j < side {
                hist[l][j * side + i] += s.w;
                pts[l].push((a, b, s.w, s.at));
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
            let (mut spread, mut core, mut count, mut independent) = (0.0, 0.0, 0, 0);
            for _ in 0..5 {
                let near: Vec<&(f64, f64, f64, V3)> = pts[l].iter().filter(|p| (p.0 - a).hypot(p.1 - b) < 0.015).collect();
                let ww: f64 = near.iter().map(|p| p.2).sum();
                if ww <= 0.0 {
                    break;
                }
                a = near.iter().map(|p| p.0 * p.2).sum::<f64>() / ww;
                b = near.iter().map(|p| p.1 * p.2).sum::<f64>() / ww;
                spread = (near.iter().map(|p| ((p.0 - a).powi(2) + (p.1 - b).powi(2)) * p.2).sum::<f64>() / ww / 2.0).sqrt();
                core = ww / w.max(1e-12);
                count = near.len();
                independent = near.iter().map(|p| p.3.map(|x| (x / apart).floor() as i32)).collect::<std::collections::HashSet<_>>().len();
            }
            (Lobe { axis: l / 2, positive: l % 2 == 0, share: w / total, a, b, spread, core, count, independent }, hist[l].clone(), side)
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
    fn normals_are_taken_from_the_points_about_each_and_doubted_points_count_for_little() {
        // A floor rising 5 mm per metre along x, 1 cm between points; and over half of it, as many stray points
        // 3 cm above it (a smear), which the rays doubt.
        let (mut xyz, mut sensor, mut conf) = (vec![], vec![], vec![]);
        for i in 0..100 {
            for j in 0..100 {
                let (x, y) = (i as f32 * 0.01, j as f32 * 0.01);
                xyz.push([x, y, 0.005 * x]);
                conf.push(1.0);
                if i >= 50 && (i + j) % 2 == 0 {
                    xyz.push([x, y, 0.005 * x + 0.03 * (j % 5) as f32 / 4.0]);
                    conf.push(0.02);
                }
            }
        }
        sensor.resize(xyz.len(), [0.5, 0.5, 2.0]);
        let used: Vec<u32> = (0..xyz.len() as u32).collect();
        let scan = Scan { path: String::new(), xyz, intensity: vec![], sensor: Some(sensor), tag: vec![], all: used.clone(), used, min_range: 0.0, ranged: true, sub: vec![], floor_z: 0.0, wall_axis: [1.0, 0.0, 0.0] };
        let slope = |ns: &[Surfel]| {
            let w: f64 = ns.iter().map(|s| s.w).sum();
            ns.iter().map(|s| -s.n[0] / s.n[2] * s.w).sum::<f64>() / w
        };
        // Counted by their confidence, the strays hardly turn the normals: the floor's slope is found all over.
        let with = normals(&scan, Some(&conf), 0.1, 2000);
        assert!(with.len() > 500 && with.iter().all(|s| s.n[2] > 0.9), "{}", with.len());
        assert!((slope(&with) - 0.005).abs() < 3e-4, "{}", slope(&with));
        // Counted alike, the normals where the strays begin are turned by them (the second fit drops only the
        // furthest): they scatter about the floor's slope several times as much.
        let without = normals(&scan, None, 0.1, 2000);
        let scatter = |ns: &[Surfel]| (ns.iter().map(|s| (-s.n[0] / s.n[2] - 0.005).powi(2)).sum::<f64>() / ns.len() as f64).sqrt();
        assert!(scatter(&with) < 1e-3 && scatter(&without) > 3.0 * scatter(&with), "{} {}", scatter(&with), scatter(&without));
    }

    #[test]
    fn a_direction_falls_on_the_face_it_is_nearest_and_neighbouring_faces_meet_at_their_edges() {
        assert_eq!(cube_face([0.0, 0.0, 1.0]), (4, 0.0, 0.0));
        assert_eq!(cube_face([0.2, -0.1, 1.0]), (4, 0.2, -0.1));
        // Just either side of the edge between the up face and +x: the up face's right edge, the +x face's left.
        let (f, a, _) = cube_face([0.999, 0.0, 1.0]);
        let (g, b, _) = cube_face([1.0, 0.0, 0.999]);
        assert!(f == 4 && a > 0.99 && g == 0 && b < -0.99);
        // And between +x and the down face beyond it.
        let (f, a, _) = cube_face([1.0, 0.0, -0.999]);
        let (g, b, _) = cube_face([0.999, 0.0, -1.0]);
        assert!(f == 0 && a > 0.99 && g == 5 && b < -0.99);
        // -x to the left of the up face, +y above it, -y below.
        assert!(cube_face([-1.0, 0.0, 0.999]).1 > 0.99 && cube_face([0.0, 1.0, 0.999]).2 < -0.99 && cube_face([0.0, -1.0, 0.999]).2 > 0.99);
        // Weight per solid angle: normals spread evenly over the sphere fill every face evenly, corners as much as middles.
        let mut ns = vec![];
        let mut k = 12345u32;
        let mut r = || {
            k = k.wrapping_mul(1664525).wrapping_add(1013904223);
            (k >> 8) as f64 / (1u32 << 24) as f64 * 2.0 - 1.0
        };
        while ns.len() < 200_000 {
            let v = [r(), r(), r()];
            let l = dot(v, v);
            if l > 0.05 && l < 1.0 {
                ns.push(Surfel { at: [0.0; 3], n: unit(v), w: 1.0 });
            }
        }
        let h = sphere_histogram(&ns, &[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], 4);
        let (lo, hi) = h.iter().flatten().fold((f64::MAX, 0.0f64), |(lo, hi), &x| (lo.min(x), hi.max(x)));
        assert!(hi / lo < 1.35, "{lo} {hi}");
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
        let ls = lobes(&ns, &frame, 0.035, 0.00025, 0.2);
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
