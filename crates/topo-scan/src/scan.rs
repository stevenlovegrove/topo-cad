//! A scan loaded for comparison with a model: its points, the sensor
//! position of each (when a trajectory is beside it), its floor and wall
//! direction.

use std::path::Path;

use anyhow::{Context, Result};

use crate::ply::read_ply;
use crate::trajectory::Trajectory;
use crate::V3;

pub struct Scan {
    pub path: String,
    pub xyz: Vec<V3>,
    /// 0..1 (empty when the scan has none).
    pub intensity: Vec<f32>,
    /// Where each point was seen from, when there is a trajectory.
    pub sensor: Option<Vec<V3>>,
    /// Points far enough from the sensor to trust (≥ 1 m, when ranges are given).
    pub used: Vec<u32>,
    /// Every 60th used point, for registration and scores.
    pub sub: Vec<V3>,
    pub floor_z: f32,
    pub wall_axis: V3,
}

impl Scan {
    /// Reads a PLY (and the trajectory beside it) and finds its floor and
    /// wall direction.
    pub fn load(ply: &str) -> Result<Scan> {
        let cloud = read_ply(ply)?;
        let sensor = match Trajectory::beside(Path::new(ply))? {
            Some(tr) if cloud.time.len() == cloud.xyz.len() => Some(tr.sensor_positions(&cloud.time, cloud.t0_ns)),
            _ => None,
        };
        Scan::new(ply, cloud.xyz, cloud.intensity, sensor, (!cloud.range.is_empty()).then_some(&cloud.range[..]))
    }

    pub fn new(path: &str, xyz: Vec<V3>, intensity: Vec<f32>, sensor: Option<Vec<V3>>, range: Option<&[f32]>) -> Result<Scan> {
        let used: Vec<u32> = (0..xyz.len() as u32).filter(|&i| range.is_none_or(|r| r[i as usize] >= 1.0)).collect();
        let sub: Vec<V3> = used.iter().step_by(60).map(|&i| xyz[i as usize]).collect();
        // The floor and walls from a thinner sample: they are large.
        let thin: Vec<V3> = used.iter().step_by(7).map(|&i| xyz[i as usize]).collect();
        let floor_z = crate::axes::floor_z(&thin).context("no floor found in the scan")?;
        let wall_axis = crate::axes::wall_axis(&thin, floor_z);
        Ok(Scan { path: path.to_string(), xyz, intensity, sensor, used, sub, floor_z, wall_axis })
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
