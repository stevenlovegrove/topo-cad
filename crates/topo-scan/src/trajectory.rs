//! Where the sensor was over time: what turns a scan's points into rays
//! (each point was seen along the segment from the sensor to it).
//!
//! Only this module knows how a trajectory is stored. Today that is a TUM
//! text file beside the scan (`t x y z qx qy qz qw` per line, `t` in
//! seconds); the rest of the crate sees times and positions.

use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::V3;

/// Sensor positions at increasing times (ns, the scan's absolute clock).
#[derive(Debug, Clone, Default)]
pub struct Trajectory {
    pub t_ns: Vec<u64>,
    pub pos: Vec<V3>,
}

/// Seconds as written (`1693922456.299888`) to integer ns, without passing
/// through an f64 (which resolves epoch seconds only to ~0.2 µs).
fn parse_seconds(s: &str) -> Result<u64> {
    if s.contains(['e', 'E']) {
        let v: f64 = s.parse().with_context(|| format!("time {s:?}"))?;
        return Ok((v * 1e9).round() as u64);
    }
    let (int, frac) = s.split_once('.').unwrap_or((s, ""));
    let frac = &frac[..frac.len().min(9)];
    let int: u64 = int.parse().with_context(|| format!("time {s:?}"))?;
    let frac_ns: u64 = if frac.is_empty() { 0 } else { format!("{frac:0<9}").parse().with_context(|| format!("time {s:?}"))? };
    Ok(int * 1_000_000_000 + frac_ns)
}

impl Trajectory {
    /// The trajectory stored beside a scan, if there is one.
    pub fn beside(scan: &Path) -> Result<Option<Trajectory>> {
        for name in ["full.tum", "odom.tum"] {
            let p = scan.with_file_name(name);
            if p.exists() {
                return Trajectory::read_tum(&p).map(Some);
            }
        }
        let p = scan.with_extension("tum");
        if p.exists() {
            return Trajectory::read_tum(&p).map(Some);
        }
        Ok(None)
    }

    pub fn read_tum(path: &Path) -> Result<Trajectory> {
        let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let mut tr = Trajectory::default();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() < 4 {
                bail!("{}:{}: expected `t x y z …`", path.display(), n + 1);
            }
            let t = parse_seconds(f[0])?;
            let p: Vec<f32> = f[1..4].iter().map(|s| s.parse::<f32>()).collect::<Result<_, _>>().with_context(|| format!("{}:{}", path.display(), n + 1))?;
            if tr.t_ns.last().is_some_and(|&l| t <= l) {
                bail!("{}:{}: times must increase", path.display(), n + 1);
            }
            tr.t_ns.push(t);
            tr.pos.push([p[0], p[1], p[2]]);
        }
        if tr.t_ns.is_empty() {
            bail!("{}: no poses", path.display());
        }
        Ok(tr)
    }

    /// The sensor's position when each point was observed: `time` is
    /// seconds since `t0_ns` (default: the trajectory's start). Linear
    /// between poses, clamped at the ends.
    pub fn sensor_positions(&self, time: &[f32], t0_ns: Option<u64>) -> Vec<V3> {
        use rayon::prelude::*;
        let t0 = t0_ns.unwrap_or(self.t_ns[0]) as f64;
        let tt: Vec<f64> = self.t_ns.iter().map(|&x| (x as f64 - t0) * 1e-9).collect();
        time.par_iter()
            .map(|&t| {
                let t = t as f64;
                let i = tt.partition_point(|&s| s <= t);
                if i == 0 {
                    return self.pos[0];
                }
                if i >= tt.len() {
                    return self.pos[tt.len() - 1];
                }
                let w = if tt[i] > tt[i - 1] { ((t - tt[i - 1]) / (tt[i] - tt[i - 1])) as f32 } else { 0.0 };
                crate::add(crate::scale(self.pos[i - 1], 1.0 - w), crate::scale(self.pos[i], w))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seconds_parse_to_exact_ns() {
        assert_eq!(parse_seconds("1693922456.299888").unwrap(), 1_693_922_456_299_888_000);
        assert_eq!(parse_seconds("12").unwrap(), 12_000_000_000);
        assert_eq!(parse_seconds("0.000000001").unwrap(), 1);
    }

    #[test]
    fn positions_are_interpolated_and_clamped() {
        let tr = Trajectory { t_ns: vec![1_000_000_000, 3_000_000_000], pos: vec![[0.0, 0.0, 1.0], [2.0, 0.0, 1.0]] };
        let p = tr.sensor_positions(&[-1.0, 0.0, 1.0, 5.0], Some(1_000_000_000));
        assert_eq!(p, vec![[0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [2.0, 0.0, 1.0]]);
    }
}
