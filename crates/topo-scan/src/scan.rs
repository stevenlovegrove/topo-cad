//! A scan loaded for comparison with a model: its points, the sensor
//! position of each (when a trajectory is beside it), its floor and wall
//! direction.

use anyhow::{Context, Result};

use crate::ply::read_ply;
use crate::V3;

pub struct Scan {
    pub path: String,
    pub xyz: Vec<V3>,
    /// 0..1 (empty when the scan has none).
    pub intensity: Vec<f32>,
    /// Where each point was seen from, when the file gives it (`sensor_x/y/z`): its ray.
    pub sensor: Option<Vec<V3>>,
    /// The lidar driver's flags per point, when the file has them (empty if not).
    pub tag: Vec<u8>,
    /// Points far enough from the sensor to trust (`min_range` or more, when
    /// ranges are known): nearer ones are of whoever carried it.
    /// Less those of selections people set to be ignored (reflections, things that were moved).
    pub used: Vec<u32>,
    /// The points far enough from the sensor, before any are ignored.
    pub all: Vec<u32>,
    pub min_range: f32,
    /// Whether the points' ranges are known (given in the file, or from the trajectory).
    pub ranged: bool,
    /// Every 60th used point, for registration and scores.
    pub sub: Vec<V3>,
    pub floor_z: f32,
    pub wall_axis: V3,
}

/// The turn that makes level what rises by `slope` per metre along x and y: the one that takes
/// that plane's normal to straight up, about the line in the plane that is already level.
pub fn levelling(slope: [f32; 2]) -> impl Fn(V3) -> V3 {
    // The plane z = sx·x + sy·y has the normal (−sx, −sy, 1); turned to (0, 0, 1) about their cross product.
    let n = crate::scale([-slope[0], -slope[1], 1.0], 1.0 / (slope[0] * slope[0] + slope[1] * slope[1] + 1.0).sqrt());
    let axis = [n[1], -n[0], 0.0];
    let (sin, cos) = (crate::norm(axis), n[2]);
    let k = if sin > 1e-12 { crate::scale(axis, 1.0 / sin) } else { [1.0, 0.0, 0.0] };
    move |p: V3| {
        // (Rodrigues: p cos + (k × p) sin + k (k·p)(1 − cos).)
        let kxp = crate::cross(k, p);
        let kp = crate::dot(k, p) * (1.0 - cos);
        std::array::from_fn(|a| p[a] * cos + kxp[a] * sin + k[a] * kp)
    }
}

impl Scan {
    /// Reads a PLY and finds its floor and wall direction. `level`: what is level rises by this much
    /// per metre along the scan's x and y, as the file has it (a scanner's sense of up is its
    /// accelerometer's, and may be a few millimetres per metre out): the points, and where each was
    /// seen from, are turned (about the scan's origin) so that it is level. [0, 0]: as the file has it. Points nearer the sensor than `min_range` are left
    /// out (their ranges as the file gives them, else from the trajectory).
    pub fn load(ply: &str, min_range: f32, level: [f32; 2]) -> Result<Scan> {
        let mut cloud = read_ply(ply)?;
        if level != [0.0, 0.0] {
            let turn = levelling(level);
            for p in cloud.xyz.iter_mut().chain(cloud.sensor.iter_mut()) {
                *p = turn(*p);
            }
        }
        let sensor = (cloud.sensor.len() == cloud.xyz.len()).then(|| std::mem::take(&mut cloud.sensor));
        let tag = std::mem::take(&mut cloud.tag);
        let range: Option<Vec<f32>> = if !cloud.range.is_empty() {
            Some(cloud.range)
        } else {
            sensor.as_ref().map(|s| cloud.xyz.iter().zip(s).map(|(p, o)| crate::norm(crate::sub(*p, *o))).collect())
        };
        let mut scan = Scan::new(ply, cloud.xyz, cloud.intensity, sensor, range.as_deref(), min_range)?;
        scan.tag = tag;
        Ok(scan)
    }

    pub fn new(path: &str, xyz: Vec<V3>, intensity: Vec<f32>, sensor: Option<Vec<V3>>, range: Option<&[f32]>, min_range: f32) -> Result<Scan> {
        let used: Vec<u32> = (0..xyz.len() as u32).filter(|&i| range.is_none_or(|r| r[i as usize] >= min_range)).collect();
        let sub: Vec<V3> = used.iter().step_by(60).map(|&i| xyz[i as usize]).collect();
        // The floor and walls from a thinner sample: they are large.
        let thin: Vec<V3> = used.iter().step_by(7).map(|&i| xyz[i as usize]).collect();
        let floor_z = crate::axes::floor_z(&thin).context("no floor found in the scan")?;
        let wall_axis = crate::axes::wall_axis(&thin, floor_z);
        Ok(Scan { path: path.to_string(), xyz, intensity, sensor, tag: vec![], all: used.clone(), used, min_range, ranged: range.is_some(), sub, floor_z, wall_axis })
    }

    /// Leaves the points selections take (of all of them, as `how` each was
    /// made) out of those used. The floor and wall direction stay as found.
    pub fn ignore(&mut self, hows: &[crate::selection::How]) {
        if hows.is_empty() {
            return;
        }
        let mut out = vec![false; self.xyz.len()];
        for how in hows {
            for i in crate::selection::select_from(self, &self.all, how) {
                out[i as usize] = true;
            }
        }
        self.used = self.all.iter().copied().filter(|&i| !out[i as usize]).collect();
        self.sub = self.used.iter().step_by(60).map(|&i| self.xyz[i as usize]).collect();
    }

    /// The nearest scan point to a ray (the first surface it meets): of the
    /// used points `keep` allows (those shown) within `radius` + `slope`·t
    /// of it, the one with least t.
    pub fn ray_hit(&self, origin: V3, dir: V3, radius: f32, slope: f32, every: usize, keep: &(dyn Fn(V3) -> bool + Sync)) -> Option<(u32, f32)> {
        use rayon::prelude::*;
        self.used
            .par_iter()
            .step_by(every.max(1))
            .filter_map(|&i| {
                let p = self.xyz[i as usize];
                if !keep(p) {
                    return None;
                }
                let q = crate::sub(p, origin);
                let t = crate::dot(q, dir);
                if t <= 0.0 {
                    return None;
                }
                let perp = crate::norm(crate::sub(q, crate::scale(dir, t)));
                (perp <= radius + slope * t).then_some((i, t))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levelling_makes_level_what_sloped_and_plumb_what_leaned() {
        // What is level rises 4 mm per metre along y and falls 2 along x, as the scanner had it.
        let slope = [-0.002f32, 0.004];
        let turn = levelling(slope);
        // Points of that plane come to z = 0.
        for (x, y) in [(3.0f32, 5.0f32), (-2.0, 7.5), (10.0, -4.0)] {
            let p = turn([x, y, slope[0] * x + slope[1] * y]);
            assert!(p[2].abs() < 1e-5, "{p:?}");
        }
        // A line square to it (a plumb wall's edge, leaning in the scan) comes to vertical.
        let n = [-slope[0], -slope[1], 1.0];
        let (a, b) = (turn([1.0, 2.0, 0.006]), turn([1.0 + 2.4 * n[0], 2.0 + 2.4 * n[1], 0.006 + 2.4 * n[2]]));
        assert!((a[0] - b[0]).abs() < 1e-5 && (a[1] - b[1]).abs() < 1e-5 && (b[2] - a[2] - 2.4).abs() < 1e-3, "{a:?} {b:?}");
        // Lengths are kept, and nothing is turned about the vertical.
        let (p, q) = (turn([1.0, 0.0, 0.0]), turn([0.0, 1.0, 0.0]));
        assert!((crate::norm(p) - 1.0).abs() < 1e-6 && p[1].abs() < 1e-5 && q[0].abs() < 1e-5, "{p:?} {q:?}");
    }
}
