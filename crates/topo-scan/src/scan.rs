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

impl Scan {
    /// Reads a PLY and finds its floor and wall direction. Points nearer the sensor than `min_range` are left
    /// out (their ranges as the file gives them, else from the trajectory).
    pub fn load(ply: &str, min_range: f32) -> Result<Scan> {
        let mut cloud = read_ply(ply)?;
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
