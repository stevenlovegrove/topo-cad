//! topo-cad models against scans. The model (a topo-cad script) is the
//! hypothesis; the scan is evidence. This crate evaluates the script, derives
//! its member solids (topo-geom), registers the building to the scan, and
//! scores each member against the points and the sensor rays:
//!
//! - **support**: scan points on the member's faces (how many, how dense,
//!   their mean offset along the face normal);
//! - **rays**: sensor rays passing through the member's solid (it is not
//!   there, or not there exactly) or ending on it.
//!
//! Members are keyed by topo-cad's stable paths (`Model::member_path`), so
//! evidence can go back to the model (a heatmap, a delta to its script).

use std::collections::HashMap;

pub mod axes;
pub mod fit;
pub mod ply;
pub mod rays;
pub mod render;
pub mod scan;
pub mod stats;
pub mod trajectory;

use anyhow::{anyhow, Result};
use rayon::prelude::*;
use rays::{ray_stats, RayStats};
use topo_core::topology::Topology;
use topo_geom::Geometry;

pub type V3 = [f32; 3];

/// An oriented box: a member's solid, for the ray test.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Obb {
    pub centre: V3,
    /// Unit axes: along the member, then its section's two directions.
    pub axes: [V3; 3],
    /// Extents along the axes, m.
    pub length: f32,
    pub width: f32,
    pub depth: f32,
}

impl Obb {
    pub fn corners(&self) -> [V3; 8] {
        let h = [self.length / 2.0, self.width / 2.0, self.depth / 2.0];
        std::array::from_fn(|k| {
            let mut p = self.centre;
            for a in 0..3 {
                let s = if k >> a & 1 == 1 { 1.0 } else { -1.0 };
                p = add(p, scale(self.axes[a], s * h[a]));
            }
            p
        })
    }
}

/// A face of a predicted member, in the model's frame (m).
#[derive(Debug, Clone)]
pub struct PredFace {
    pub poly: Vec<V3>,
    pub normal: V3,
    pub area: f32,
}

/// A predicted member: its path, role, faces and oriented box.
#[derive(Debug, Clone)]
pub struct PredMember {
    pub id: usize,
    pub path: String,
    pub role: String,
    pub faces: Vec<PredFace>,
    /// Oriented box about its solid (member axes: along the grain, u, v).
    pub obb: Obb,
}

/// A model evaluated: its members' predicted solids.
pub struct Prediction {
    pub name: String,
    pub members: Vec<PredMember>,
    /// The fit report text when the script has unknowns or measurements.
    pub report: Option<String>,
}

fn v(p: topo_core::math::Vec3) -> V3 {
    [p.x as f32, p.y as f32, p.z as f32]
}

/// Evaluates (and solves) a topo-cad script; relative imports resolve from
/// its directory on disk.
pub fn predict(path: &str) -> Result<Prediction> {
    predict_source(&std::fs::read_to_string(path)?, path)
}

/// Evaluates a script's source as though it were the file at `path` (to
/// score an edit before it is written).
pub fn predict_source(source: &str, path: &str) -> Result<Prediction> {
    let (model, report) = topo_script::run_solved(source, path).map_err(|e| anyhow!("{e:?}"))?;
    let geom = Geometry::build(&model, &Topology::build(&model));
    let members = model
        .members
        .iter()
        .enumerate()
        .map(|(k, m)| {
            let g = geom.member(m.id);
            let faces: Vec<PredFace> = g
                .faces()
                .into_iter()
                .map(|f| {
                    let poly: Vec<V3> = f.outer.iter().map(|&p| v(p)).collect();
                    PredFace { area: poly_area(&poly), poly, normal: v(f.normal) }
                })
                .collect();
            // Oriented box in the member's axes.
            let axes = [v(g.place.x), v(g.place.u), v(g.place.v)];
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for f in &faces {
                for p in &f.poly {
                    for a in 0..3 {
                        let t = dot(*p, axes[a]);
                        lo[a] = lo[a].min(t);
                        hi[a] = hi[a].max(t);
                    }
                }
            }
            let c: [f32; 3] = std::array::from_fn(|a| 0.5 * (lo[a] + hi[a]));
            let centre = add(add(scale(axes[0], c[0]), scale(axes[1], c[1])), scale(axes[2], c[2]));
            let obb = Obb { centre, axes, length: hi[0] - lo[0], width: hi[1] - lo[1], depth: hi[2] - lo[2] };
            PredMember { id: k, path: model.member_path(m.id), role: m.role.clone(), faces, obb }
        })
        .collect();
    Ok(Prediction { name: model.info.name.clone(), members, report: report.map(|r| r.text()) })
}

// ----- small vector helpers ---------------------------------------------------

pub(crate) fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub(crate) fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub(crate) fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub(crate) fn scale(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
pub(crate) fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
pub(crate) fn norm(a: V3) -> f32 {
    dot(a, a).sqrt()
}
fn poly_area(p: &[V3]) -> f32 {
    let mut c = [0.0f32; 3];
    for i in 0..p.len() {
        c = add(c, cross(p[i], p[(i + 1) % p.len()]));
    }
    0.5 * norm(c)
}

// ----- the model's placement in the scan ---------------------------------------

/// Model frame → scan frame: a turn about the vertical, then a translation
/// (both frames are gravity-aligned).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Placement {
    pub yaw: f32,
    pub t: V3,
}

impl Placement {
    pub fn apply(&self, p: V3) -> V3 {
        let (c, s) = (self.yaw.cos(), self.yaw.sin());
        [c * p[0] - s * p[1] + self.t[0], s * p[0] + c * p[1] + self.t[1], p[2] + self.t[2]]
    }
    pub fn rotate(&self, n: V3) -> V3 {
        let (c, s) = (self.yaw.cos(), self.yaw.sin());
        [c * n[0] - s * n[1], s * n[0] + c * n[1], n[2]]
    }
    /// A scan-frame direction in the model's frame.
    pub fn unrotate(&self, n: V3) -> V3 {
        let (c, s) = (self.yaw.cos(), self.yaw.sin());
        [c * n[0] + s * n[1], -s * n[0] + c * n[1], n[2]]
    }
    /// Scan frame → model frame.
    pub fn inverse(&self, p: V3) -> V3 {
        let q = sub(p, self.t);
        let (c, s) = (self.yaw.cos(), self.yaw.sin());
        [c * q[0] + s * q[1], -s * q[0] + c * q[1], q[2]]
    }
    /// A member's box, placed in the scan.
    pub fn obb(&self, b: &Obb) -> Obb {
        Obb { centre: self.apply(b.centre), axes: b.axes.map(|a| self.rotate(a)), ..*b }
    }
    /// The placement turned by `dyaw` about the vertical through `pivot`
    /// (scan frame), so what is at the pivot stays there.
    pub fn turned_about(&self, dyaw: f32, pivot: V3) -> Placement {
        let (c, s) = (dyaw.cos(), dyaw.sin());
        let q = sub(self.t, pivot);
        Placement { yaw: self.yaw + dyaw, t: [c * q[0] - s * q[1] + pivot[0], s * q[0] + c * q[1] + pivot[1], self.t[2]] }
    }
    /// The placement best taking model points onto scan points (least
    /// squares; a turn needs two pairs or more, one pair only translates
    /// from `from`'s yaw).
    pub fn from_pairs(pairs: &[(V3, V3)], from: &Placement) -> Option<Placement> {
        if pairs.is_empty() {
            return None;
        }
        let n = pairs.len() as f32;
        let cm = pairs.iter().fold([0.0f32; 3], |a, p| add(a, scale(p.0, 1.0 / n)));
        let cs = pairs.iter().fold([0.0f32; 3], |a, p| add(a, scale(p.1, 1.0 / n)));
        let yaw = if pairs.len() >= 2 {
            // 2D Procrustes about the centroids.
            let (mut sc, mut ss) = (0.0f32, 0.0f32);
            for (m, s) in pairs {
                let (a, b) = (sub(*m, cm), sub(*s, cs));
                sc += a[0] * b[0] + a[1] * b[1];
                ss += a[0] * b[1] - a[1] * b[0];
            }
            if sc.abs() + ss.abs() < 1e-9 {
                from.yaw
            } else {
                ss.atan2(sc)
            }
        } else {
            from.yaw
        };
        let r = Placement { yaw, t: [0.0; 3] }.apply(cm);
        Some(Placement { yaw, t: sub(cs, r) })
    }
}

/// Points sampled on the predicted faces (model frame), with their normal
/// and member, in a hash grid for nearest-sample queries.
pub struct FaceSamples {
    pub pts: Vec<V3>,
    pub normals: Vec<V3>,
    pub member: Vec<u32>,
    /// Hash grids at two cell sizes (fine for small gates, coarse for large).
    grids: [(f32, HashMap<(i32, i32, i32), Vec<u32>>); 2],
}

impl FaceSamples {
    /// Samples every `step` (m) over every face.
    pub fn new(pred: &Prediction, step: f32) -> FaceSamples {
        let (mut pts, mut normals, mut member) = (Vec::new(), Vec::new(), Vec::new());
        for m in &pred.members {
            for f in &m.faces {
                // Fan triangles; a regular lattice of barycentric points per triangle.
                for i in 1..f.poly.len().saturating_sub(1) {
                    let (a, b, c) = (f.poly[0], f.poly[i], f.poly[i + 1]);
                    let area = 0.5 * norm(cross(sub(b, a), sub(c, a)));
                    let n = ((area / (step * step)).sqrt().ceil() as usize).max(1);
                    for u in 0..n {
                        for w in 0..(n - u) {
                            let (s, t) = ((u as f32 + 1.0 / 3.0) / n as f32, (w as f32 + 1.0 / 3.0) / n as f32);
                            pts.push(add(a, add(scale(sub(b, a), s), scale(sub(c, a), t))));
                            normals.push(f.normal);
                            member.push(m.id as u32);
                        }
                    }
                }
            }
        }
        let grid = |cell: f32| {
            let mut g: HashMap<(i32, i32, i32), Vec<u32>> = HashMap::new();
            for (k, p) in pts.iter().enumerate() {
                g.entry(key(*p, cell)).or_default().push(k as u32);
            }
            (cell, g)
        };
        let grids = [grid(0.1), grid(0.5)];
        FaceSamples { pts, normals, member, grids }
    }

    /// The nearest sample to `p` (model frame) within `gate`.
    pub fn nearest(&self, p: V3, gate: f32) -> Option<(u32, f32)> {
        let (cell, grid) = if gate > 0.2 { (self.grids[1].0, &self.grids[1].1) } else { (self.grids[0].0, &self.grids[0].1) };
        let r = (gate / cell).ceil() as i32;
        let k = key(p, cell);
        let mut best: Option<(u32, f32)> = None;
        for dx in -r..=r {
            for dy in -r..=r {
                for dz in -r..=r {
                    if let Some(list) = grid.get(&(k.0 + dx, k.1 + dy, k.2 + dz)) {
                        for &i in list {
                            let d = norm(sub(self.pts[i as usize], p));
                            if d <= gate && best.is_none_or(|b| d < b.1) {
                                best = Some((i, d));
                            }
                        }
                    }
                }
            }
        }
        best
    }
}

fn key(p: V3, cell: f32) -> (i32, i32, i32) {
    ((p[0] / cell).floor() as i32, (p[1] / cell).floor() as i32, (p[2] / cell).floor() as i32)
}

#[derive(Debug, Clone)]
pub struct RegReport {
    pub placement: Placement,
    /// Of the scan points used, the fraction within 3 cm of a predicted face.
    pub inliers: f32,
    /// RMS point-to-face distance of those, m.
    pub rms: f32,
    pub candidates: Vec<(Placement, f32)>,
}

/// Robust point-to-plane ICP of scan points `pts` against the model's face
/// samples, from `init`, the gate shrinking from `gates[0]` to the last.
pub fn icp(samples: &FaceSamples, pts: &[V3], init: Placement, gates: &[f32]) -> (Placement, f32, f32) {
    let mut pl = init;
    // Coarse stages on a subsample (their gates reach many samples each).
    let coarse: Vec<V3> = pts.iter().step_by((pts.len() / 20_000).max(1)).copied().collect();
    for &gate in gates {
        let pts: &[V3] = if gate > 0.2 { &coarse } else { pts };
        for _ in 0..6 {
            // Correspondences (in the model frame), residuals and Jacobians
            // in the scan frame: r = n·(p − q), q = R qm + t, n = R nm.
            let rows: Vec<([f32; 4], f32, f32)> = pts
                .par_iter()
                .filter_map(|&p| {
                    let m = pl.inverse(p);
                    let (i, _) = samples.nearest(m, gate)?;
                    let (qm, nm) = (samples.pts[i as usize], samples.normals[i as usize]);
                    let (q, n) = (pl.apply(qm), pl.rotate(nm));
                    let r = dot(n, sub(p, q));
                    // d q / d yaw = ez × (R qm); d n / d yaw = ez × n.
                    let rq = sub(q, pl.t);
                    let dq = [-rq[1], rq[0], 0.0];
                    let dn = [-n[1], n[0], 0.0];
                    let j_yaw = -dot(n, dq) + dot(dn, sub(p, q));
                    let j = [j_yaw, -n[0], -n[1], -n[2]];
                    // Huber weight at a third of the gate.
                    let k = gate / 3.0;
                    let w = if r.abs() <= k { 1.0 } else { k / r.abs() };
                    Some((j, r, w))
                })
                .collect();
            if rows.len() < 50 {
                break;
            }
            let mut a = [[0.0f64; 4]; 4];
            let mut b = [0.0f64; 4];
            for (j, r, w) in &rows {
                for x in 0..4 {
                    b[x] -= (*w * j[x] * r) as f64;
                    for y in 0..4 {
                        a[x][y] += (*w * j[x] * j[y]) as f64;
                    }
                }
            }
            for x in 0..4 {
                a[x][x] += 1e-6;
            }
            let Some(dx) = solve4(a, b) else { break };
            pl.yaw += dx[0] as f32;
            for k in 0..3 {
                pl.t[k] += dx[k + 1] as f32;
            }
            if dx.iter().map(|x| x.abs()).fold(0.0, f64::max) < 1e-5 {
                break;
            }
        }
    }
    let (inliers, rms) = score(samples, pts, &pl);
    (pl, inliers, rms)
}

/// How well a placement puts the scan on the model: the fraction of `pts`
/// within 3 cm of a predicted face, and their RMS distance (m).
pub fn score(samples: &FaceSamples, pts: &[V3], pl: &Placement) -> (f32, f32) {
    let d: Vec<f32> = pts.par_iter().filter_map(|&p| samples.nearest(pl.inverse(p), 0.03).map(|x| x.1)).collect();
    let rms = (d.iter().map(|x| x * x).sum::<f32>() / d.len().max(1) as f32).sqrt();
    (d.len() as f32 / pts.len().max(1) as f32, rms)
}

fn solve4(mut a: [[f64; 4]; 4], mut b: [f64; 4]) -> Option<[f64; 4]> {
    for c in 0..4 {
        let p = (c..4).max_by(|&i, &j| a[i][c].abs().total_cmp(&a[j][c].abs()))?;
        if a[p][c].abs() < 1e-18 {
            return None;
        }
        a.swap(c, p);
        b.swap(c, p);
        for r in c + 1..4 {
            let f = a[r][c] / a[c][c];
            for k in c..4 {
                a[r][k] -= f * a[c][k];
            }
            b[r] -= f * b[c];
        }
    }
    let mut x = [0.0; 4];
    for r in (0..4).rev() {
        x[r] = (b[r] - (r + 1..4).map(|k| a[r][k] * x[k]).sum::<f64>()) / a[r][r];
    }
    Some(x)
}

/// Registers the model in the scan: yaw candidates from the scan's wall axis
/// (the model's x along it, turned by 0/90/180/270°), translations lining up
/// the floors (model z = 0 on `floor_z`) and the plan centroids of the scan's
/// wall-height points and the model; each refined by ICP; the best kept.
pub fn register(pred: &Prediction, samples: &FaceSamples, pts: &[V3], wall_axis: V3, floor_z: f32) -> RegReport {
    let theta0 = wall_axis[1].atan2(wall_axis[0]);
    let centroid = |ps: &mut dyn Iterator<Item = V3>| {
        let (mut c, mut n) = ([0.0f64; 2], 0.0f64);
        for p in ps {
            c[0] += p[0] as f64;
            c[1] += p[1] as f64;
            n += 1.0;
        }
        [(c[0] / n.max(1.0)) as f32, (c[1] / n.max(1.0)) as f32]
    };
    let cs = centroid(&mut pts.iter().copied().filter(|p| p[2] > floor_z + 0.3 && p[2] < floor_z + 2.2));
    let cm = centroid(&mut pred.members.iter().map(|m| m.obb.centre).filter(|c| c[2] < 2.2));
    let mut candidates = Vec::new();
    for k in 0..4 {
        let yaw = theta0 + k as f32 * std::f32::consts::FRAC_PI_2;
        let (c, s) = (yaw.cos(), yaw.sin());
        let rc = [c * cm[0] - s * cm[1], s * cm[0] + c * cm[1]];
        let init = Placement { yaw, t: [cs[0] - rc[0], cs[1] - rc[1], floor_z] };
        let (pl, inl, _) = icp(samples, pts, init, &[1.0, 0.5, 0.25, 0.1, 0.05]);
        candidates.push((pl, inl));
    }
    let best = candidates.iter().max_by(|a, b| a.1.total_cmp(&b.1)).unwrap().0;
    let (placement, inliers, rms) = icp(samples, pts, best, &[0.05, 0.03]);
    RegReport { placement, inliers, rms, candidates }
}

/// A member's evidence in the scan.
#[derive(Debug, Clone, Default)]
pub struct Evidence {
    /// Scan points within the gate of its faces (and nearer them than any
    /// other member's), their density on its surface (points / m²), their
    /// mean and RMS offset along the face normals (m).
    pub points: u32,
    pub density: f32,
    pub mean_offset: f32,
    pub rms: f32,
    /// Sensor rays through its solid / ending on it.
    pub rays: RayStats,
}

/// Per-member evidence of a placed prediction.
pub fn evidence(pred: &Prediction, samples: &FaceSamples, pl: &Placement, xyz: &[V3], used: &[u32], sensor: Option<&[V3]>, gate: f32, ray_stride: usize) -> Vec<Evidence> {
    let n = pred.members.len();
    // Nearest face sample of every used point (model frame).
    let hits: Vec<(u32, f32)> = used
        .par_iter()
        .filter_map(|&i| {
            let m = pl.inverse(xyz[i as usize]);
            let (k, _) = samples.nearest(m, gate)?;
            let off = dot(samples.normals[k as usize], sub(m, samples.pts[k as usize]));
            Some((samples.member[k as usize], off))
        })
        .collect();
    let mut out = vec![Evidence::default(); n];
    let mut sums = vec![(0.0f64, 0.0f64); n];
    for (m, off) in hits {
        out[m as usize].points += 1;
        sums[m as usize].0 += off as f64;
        sums[m as usize].1 += (off * off) as f64;
    }
    for (k, e) in out.iter_mut().enumerate() {
        let area: f32 = pred.members[k].faces.iter().map(|f| f.area).sum();
        e.density = e.points as f32 / area.max(1e-6);
        if e.points > 0 {
            e.mean_offset = (sums[k].0 / e.points as f64) as f32;
            e.rms = (sums[k].1 / e.points as f64).sqrt() as f32;
        }
    }
    if let Some(sensor) = sensor {
        let boxes: Vec<Obb> = pred.members.iter().map(|m| pl.obb(&m.obb)).collect();
        let stats = ray_stats(&boxes, xyz, sensor, used, ray_stride, 0.01);
        for (e, s) in out.iter_mut().zip(stats) {
            e.rays = s;
        }
    }
    out
}

// ----- reporting -----------------------------------------------------------------

/// Latest modification time of the `.ts` files beside the model and in the
/// folders it imports from (relative imports only): changes when the model
/// might evaluate differently.
pub fn model_mtime(model: &str) -> Option<std::time::SystemTime> {
    let dir = std::path::Path::new(model).parent()?;
    let mut dirs = vec![dir.to_path_buf()];
    if let Ok(src) = std::fs::read_to_string(model) {
        for line in src.lines().filter(|l| l.contains("from \"") || l.contains("from '")) {
            if let Some(spec) = line.split(['"', '\'']).nth(1).filter(|s| s.starts_with('.')) {
                if let Some(d) = dir.join(spec).parent() {
                    dirs.push(d.to_path_buf());
                }
            }
        }
    }
    dirs.iter()
        .filter_map(|d| std::fs::read_dir(d).ok())
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "ts"))
        .filter_map(|e| e.metadata().ok()?.modified().ok())
        .max()
}

/// A hash of the contents of the files `model_mtime` watches: whether a
/// touched file really changed.
pub fn model_hash(model: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let Some(dir) = std::path::Path::new(model).parent() else { return 0 };
    let mut dirs = vec![dir.to_path_buf()];
    if let Ok(src) = std::fs::read_to_string(model) {
        for line in src.lines().filter(|l| l.contains("from \"") || l.contains("from '")) {
            if let Some(spec) = line.split(['"', '\'']).nth(1).filter(|s| s.starts_with('.')) {
                if let Some(d) = dir.join(spec).parent() {
                    dirs.push(d.to_path_buf());
                }
            }
        }
    }
    let mut files: Vec<std::path::PathBuf> = dirs.iter().filter_map(|d| std::fs::read_dir(d).ok()).flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "ts")).collect();
    files.sort();
    files.dedup();
    for f in files {
        f.hash(&mut h);
        std::fs::read(&f).unwrap_or_default().hash(&mut h);
    }
    h.finish()
}

/// A member's colour from its evidence: green where the scan supports it,
/// through amber to red as more of the rays reaching it pass through it
/// (all red at `red_at`), grey when too few rays or points reach it to say.
pub fn evidence_colour(e: &Evidence, red_at: f32, min_rays: u32) -> [f32; 3] {
    let n = e.rays.pierced + e.rays.ended;
    if n < min_rays && e.points < 50 {
        return [0.5, 0.5, 0.55];
    }
    let t = (e.rays.pierce_frac() / red_at).clamp(0.0, 1.0);
    if t < 0.5 {
        [0.2 + 1.4 * t, 0.9, 0.25]
    } else {
        [0.9, 0.9 - 1.4 * (t - 0.5), 0.2]
    }
}

/// Members grouped by their path's parent.
pub fn groups(pred: &Prediction) -> std::collections::BTreeMap<String, Vec<usize>> {
    let mut g: std::collections::BTreeMap<String, Vec<usize>> = Default::default();
    for (k, m) in pred.members.iter().enumerate() {
        g.entry(m.path.rsplit_once('/').map_or("", |x| x.0).to_string()).or_default().push(k);
    }
    g
}

/// A table of evidence per group of members.
pub fn group_table(pred: &Prediction, ev: &[Evidence]) -> String {
    use std::fmt::Write;
    let mut s = format!("{:<48} {:>5} {:>9} {:>8} {:>9} {:>8}\n", "group", "n", "seen", "pts/m²", "offset mm", "pierced");
    for (g, ks) in &groups(pred) {
        let seen = ks.iter().filter(|&&k| ev[k].points >= 50 || ev[k].rays.pierced + ev[k].rays.ended >= 100).count();
        let pts: u32 = ks.iter().map(|&k| ev[k].points).sum();
        let area: f32 = ks.iter().map(|&k| pred.members[k].faces.iter().map(|f| f.area).sum::<f32>()).sum();
        let off = ks.iter().map(|&k| ev[k].mean_offset * ev[k].points as f32).sum::<f32>() / pts.max(1) as f32;
        let (pi, en) = ks.iter().fold((0, 0), |a, &k| (a.0 + ev[k].rays.pierced, a.1 + ev[k].rays.ended));
        let _ = writeln!(s, "{:<48} {:>5} {:>4}/{:<4} {:>8.0} {:>9.1} {:>7.0}%", g, ks.len(), seen, ks.len(), pts as f32 / area.max(1e-6), off * 1e3, 100.0 * pi as f32 / (pi + en).max(1) as f32);
    }
    s
}

/// The members the rays most contradict (of those at least `min_rays` reach).
pub fn contradicted(pred: &Prediction, ev: &[Evidence], min_rays: u32, n: usize) -> Vec<usize> {
    let mut worst: Vec<usize> = (0..pred.members.len()).filter(|&k| ev[k].rays.pierced + ev[k].rays.ended >= min_rays).collect();
    worst.sort_by(|&a, &b| ev[b].rays.pierce_frac().total_cmp(&ev[a].rays.pierce_frac()));
    worst.truncate(n);
    worst
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_from_pairs_recovers_a_turn_and_shift() {
        let truth = Placement { yaw: 0.7, t: [1.0, -2.0, 0.3] };
        let model = [[0.0, 0.0, 0.0], [4.0, 0.0, 0.0], [4.0, 3.0, 2.5], [0.0, 3.0, 1.0]];
        let pairs: Vec<(V3, V3)> = model.iter().map(|&m| (m, truth.apply(m))).collect();
        let p = Placement::from_pairs(&pairs, &Placement::default()).unwrap();
        assert!((p.yaw - truth.yaw).abs() < 1e-5, "{p:?}");
        for k in 0..3 {
            assert!((p.t[k] - truth.t[k]).abs() < 1e-4, "{p:?}");
        }
    }

    #[test]
    fn turning_about_a_pivot_keeps_it_in_place() {
        let p = Placement { yaw: 0.2, t: [1.0, 2.0, 0.0] };
        let m = [3.0, -1.0, 0.5];
        let pivot = p.apply(m);
        let q = p.turned_about(1.1, pivot);
        let r = q.apply(m);
        for k in 0..3 {
            assert!((r[k] - pivot[k]).abs() < 1e-5, "{r:?} {pivot:?}");
        }
        assert!((q.yaw - 1.3).abs() < 1e-6);
    }
}
