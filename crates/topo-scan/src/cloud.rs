//! The scan as a viewer draws it: a thinned cloud for the 3D view (one
//! point per voxel), and every point of a slab for a section. Each point
//! has its position (scan frame), its intensity and the normal of the
//! surface it lies on (of its 6 cm cell, turned toward the sensor; zero
//! where the cell is not a clean surface), so a viewer can colour by
//! height, intensity, normal or offset from a plane without asking again.
//!
//! As bytes (little-endian): `u32 n`, `n × 3 f32` positions, `n u8`
//! intensities, `n × 3 i8` normals (×127).

use std::collections::HashMap;

use rayon::prelude::*;

use crate::scan::Scan;
use crate::stats::point_stats;
use crate::V3;

const CELL: f32 = 0.06;

#[derive(Debug, Clone, Default)]
pub struct Cloud {
    pub xyz: Vec<V3>,
    pub intensity: Vec<u8>,
    pub normal: Vec<[i8; 3]>,
}

impl Cloud {
    pub fn len(&self) -> usize {
        self.xyz.len()
    }
    pub fn is_empty(&self) -> bool {
        self.xyz.is_empty()
    }
    pub(crate) fn push(&mut self, p: V3, intensity: u8, normal: [i8; 3]) {
        self.xyz.push(p);
        self.intensity.push(intensity);
        self.normal.push(normal);
    }
    pub fn bytes(&self) -> Vec<u8> {
        let n = self.len();
        let mut b = Vec::with_capacity(4 + 16 * n);
        b.extend_from_slice(&(n as u32).to_le_bytes());
        for p in &self.xyz {
            for x in p {
                b.extend_from_slice(&x.to_le_bytes());
            }
        }
        b.extend_from_slice(&self.intensity);
        for nm in &self.normal {
            b.extend(nm.iter().map(|&x| x as u8));
        }
        b
    }
}

fn key(p: V3, cell: f32) -> (i32, i32, i32) {
    ((p[0] / cell).floor() as i32, (p[1] / cell).floor() as i32, (p[2] / cell).floor() as i32)
}

/// Whether `p` is in the box with centre `c`, unit axes and half sizes.
pub fn inside(p: V3, c: V3, axes: &[V3; 3], half: V3) -> bool {
    let q = crate::sub(p, c);
    (0..3).all(|k| crate::dot(q, axes[k]).abs() <= half[k])
}

/// The scan's surface normals by 6 cm cell, and the thinned cloud.
pub struct ScanIndex {
    normals: HashMap<(i32, i32, i32), [i8; 3]>,
    pub thin: Cloud,
    pub voxel: f32,
}

impl ScanIndex {
    /// `voxel`: the thinned cloud keeps one point per cube of this side (m).
    pub fn build(scan: &Scan, voxel: f32) -> ScanIndex {
        // The used points in cell order, so each cell is a run.
        let mut order: Vec<((i32, i32, i32), u32)> = scan.used.par_iter().map(|&i| (key(scan.xyz[i as usize], CELL), i)).collect();
        order.par_sort_unstable();
        let mut runs: Vec<(usize, usize)> = Vec::new();
        let mut a = 0;
        for k in 1..=order.len() {
            if k == order.len() || order[k].0 != order[a].0 {
                runs.push((a, k));
                a = k;
            }
        }
        let intensity = |i: u32| scan.intensity.get(i as usize).map_or(128, |&x| (x * 255.0).round().clamp(0.0, 255.0) as u8);
        let cells: Vec<((i32, i32, i32), [i8; 3], Cloud)> = runs
            .par_iter()
            .map(|&(a, b)| {
                let run = &order[a..b];
                // A surface: thin across, and enough points to say.
                let st = point_stats(run.iter().map(|r| scan.xyz[r.1 as usize]));
                let mut n = [0i8; 3];
                if st.count >= 6 && st.sigma[2] < 0.012 && st.sigma[2] < 0.5 * st.sigma[1] {
                    let mut nm = st.axes[2];
                    if let Some(sensor) = &scan.sensor {
                        let i = run[0].1 as usize;
                        if crate::dot(nm, crate::sub(sensor[i], scan.xyz[i])) < 0.0 {
                            nm = crate::scale(nm, -1.0);
                        }
                    }
                    n = nm.map(|x| (x * 127.0).round() as i8);
                }
                // One point per voxel of the cell.
                let mut kept = Cloud::default();
                let mut seen: Vec<(i32, i32, i32)> = Vec::new();
                for r in run {
                    let p = scan.xyz[r.1 as usize];
                    let v = key(p, voxel);
                    if !seen.contains(&v) {
                        seen.push(v);
                        kept.push(p, intensity(r.1), n);
                    }
                }
                (run[0].0, n, kept)
            })
            .collect();
        let mut thin = Cloud::default();
        let mut normals = HashMap::with_capacity(cells.len());
        for (k, n, c) in cells {
            normals.insert(k, n);
            thin.xyz.extend(c.xyz);
            thin.intensity.extend(c.intensity);
            thin.normal.extend(c.normal);
        }
        ScanIndex { normals, thin, voxel }
    }

    /// The normal of the surface a point lies on (of its cell; toward the
    /// sensor), if its cell is a clean surface.
    pub fn normal_at(&self, p: V3) -> Option<V3> {
        // The cell the point is in; if its points gave no normal (too few, or an
        // edge), what the cells about it agree on.
        let k = key(p, CELL);
        if let Some(n) = self.normals.get(&k).filter(|n| **n != [0; 3]) {
            return Some(n.map(|x| x as f32 / 127.0));
        }
        let (mut sum, mut first) = ([0.0f32; 3], None::<V3>);
        for (dx, dy, dz) in (-2..=2).flat_map(|x| (-2..=2).flat_map(move |y| (-2..=2).map(move |z| (x, y, z)))) {
            let Some(n) = self.normals.get(&(k.0 + dx, k.1 + dy, k.2 + dz)).filter(|n| **n != [0; 3]) else { continue };
            let n = n.map(|x| x as f32 / 127.0);
            // (A normal's sign is arbitrary: taken the way of the first.)
            let s = if crate::dot(n, *first.get_or_insert(n)) < 0.0 { -1.0 } else { 1.0 };
            sum = crate::add(sum, crate::scale(n, s));
        }
        let len = crate::norm(sum);
        (len > 0.5).then(|| crate::scale(sum, 1.0 / len))
    }

    /// Every used point in an oriented box (centre, unit axes, half sizes; scan
    /// frame), thinned evenly to at most `max`.
    pub fn in_box(&self, scan: &Scan, c: V3, axes: [V3; 3], half: V3, max: usize) -> Cloud {
        let idx: Vec<u32> = scan.used.par_iter().copied().filter(|&i| inside(scan.xyz[i as usize], c, &axes, half)).collect();
        let step = idx.len().div_ceil(max.max(1)).max(1);
        let mut c = Cloud::default();
        for &i in idx.iter().step_by(step) {
            let p = scan.xyz[i as usize];
            c.push(p, scan.intensity.get(i as usize).map_or(128, |&x| (x * 255.0).round().clamp(0.0, 255.0) as u8), self.normals.get(&key(p, CELL)).copied().unwrap_or([0; 3]));
        }
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thins_to_one_point_per_voxel_with_normals_toward_the_sensor() {
        // A floor at z = 0 on a 1 cm lattice, seen from above; a wall at x = 0 seen from +x.
        let mut xyz = Vec::new();
        for i in 0..200 {
            for j in 0..200 {
                xyz.push([0.005 + 0.01 * i as f32, 0.005 + 0.01 * j as f32, 0.0]);
                xyz.push([0.0, 0.005 + 0.01 * i as f32, 0.105 + 0.01 * j as f32]);
            }
        }
        let n = xyz.len();
        let scan = Scan::new("t", xyz, vec![0.5; n], Some(vec![[1.0, 1.0, 1.0]; n]), None).unwrap();
        let ix = ScanIndex::build(&scan, 0.03);
        // 2 m × 2 m of floor and of wall at 3 cm: about 4444 voxels each.
        assert!(ix.thin.len() > 8000 && ix.thin.len() < 10500, "{}", ix.thin.len());
        let (mut up, mut east) = (0, 0);
        for (p, nm) in ix.thin.xyz.iter().zip(&ix.thin.normal) {
            if p[2] == 0.0 && p[0] > 0.1 {
                up += (nm[2] > 120) as usize;
            } else if p[0] == 0.0 && p[2] > 0.2 {
                east += (nm[0] > 120) as usize;
            }
        }
        assert!(up > 3500 && east > 3500, "{up} {east}");
        assert_eq!(ix.thin.bytes().len(), 4 + 16 * ix.thin.len());
        // A slab about the wall holds the wall's points (and the floor's edge), thinned to the cap.
        let s = ix.in_box(&scan, [0.0, 0.0, 0.0], [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], [0.003, 100.0, 100.0], 5000);
        assert!(s.len() <= 5000 && s.len() > 4000 && s.xyz.iter().all(|p| p[0].abs() <= 0.003), "{}", s.len());
    }
}
