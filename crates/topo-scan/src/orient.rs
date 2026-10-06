//! How the scan is turned against the model: one small rotation fitted over many surfaces at
//! once. Each surface is the scan's points near a set of coplanar faces of the model, whose
//! normal is the model's; where along that normal the surface is, is left free (a finish stands
//! off the framing), so only orientation is fitted. Each surface's own turn is reported beside
//! the joint one: what the joint fit cannot take is the building's (a wall that leans), or a
//! surface that is not what the model has there.

/// The scan's points near one surface of the model: enough of their sums to fit a turn.
#[derive(Clone, Debug)]
pub struct Patch {
    /// The model's normal there.
    pub n: [f64; 3],
    pub w: f64,
    sum: [f64; 3],
    /// Σ w p pᵀ (xx, xy, xz, yy, yz, zz).
    sq: [f64; 6],
}

type M3 = [[f64; 3]; 3];

impl Patch {
    pub fn new(n: [f64; 3]) -> Patch {
        Patch { n, w: 0.0, sum: [0.0; 3], sq: [0.0; 6] }
    }
    pub fn add(&mut self, p: [f64; 3], w: f64) {
        self.w += w;
        for k in 0..3 {
            self.sum[k] += w * p[k];
        }
        let [x, y, z] = p;
        for (k, v) in [x * x, x * y, x * z, y * y, y * z, z * z].into_iter().enumerate() {
            self.sq[k] += w * v;
        }
    }
    pub fn merge(&mut self, o: &Patch) {
        self.w += o.w;
        for k in 0..3 {
            self.sum[k] += o.sum[k];
        }
        for k in 0..6 {
            self.sq[k] += o.sq[k];
        }
    }
    /// Σ w p̃ p̃ᵀ about its own middle.
    fn scatter(&self) -> M3 {
        let c = self.sum.map(|s| s / self.w);
        let s = self.sq;
        let full = [[s[0], s[1], s[2]], [s[1], s[3], s[4]], [s[2], s[4], s[5]]];
        std::array::from_fn(|i| std::array::from_fn(|j| full[i][j] - self.w * c[i] * c[j]))
    }
    /// How far its points are from the plane through their middle square to the model's normal (rms, m).
    pub fn rms(&self) -> f64 {
        let c = self.scatter();
        (mul_v(&c, self.n).iter().zip(self.n).map(|(a, b)| a * b).sum::<f64>().max(0.0) / self.w).sqrt()
    }
    /// Its part of the normal equations of the turn ω (applied to the scan: p → p + ω × p) that
    /// brings the points onto planes square to the model's normals: with N = [n]×, (N C Nᵀ) ω = N C n.
    fn equations(&self) -> (M3, [f64; 3]) {
        let (c, n) = (self.scatter(), self.n);
        let nx = [[0.0, -n[2], n[1]], [n[2], 0.0, -n[0]], [-n[1], n[0], 0.0]];
        let nc = mul(&nx, &c);
        (mul(&nc, &transpose(&nx)), mul_v(&nc, n))
    }
}

fn mul(a: &M3, b: &M3) -> M3 {
    std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| a[i][k] * b[k][j]).sum()))
}
fn mul_v(a: &M3, v: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| (0..3).map(|k| a[i][k] * v[k]).sum())
}
fn transpose(a: &M3) -> M3 {
    std::array::from_fn(|i| std::array::from_fn(|j| a[j][i]))
}

/// The eigenvalues (ascending) and vectors (columns) of a symmetric 3×3, by Jacobi turns.
fn eigen(a: &M3) -> ([f64; 3], M3) {
    let (mut a, mut v) = (*a, [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
    for _ in 0..60 {
        let (p, q) = [(0, 1), (0, 2), (1, 2)].into_iter().max_by(|x, y| a[x.0][x.1].abs().total_cmp(&a[y.0][y.1].abs())).unwrap();
        if a[p][q].abs() < 1e-300 {
            break;
        }
        let th = 0.5 * (2.0 * a[p][q]).atan2(a[q][q] - a[p][p]);
        let (c, s) = (th.cos(), th.sin());
        let mut r = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        r[p][p] = c;
        r[q][q] = c;
        r[p][q] = s;
        r[q][p] = -s;
        a = mul(&mul(&transpose(&r), &a), &r);
        v = mul(&v, &r);
    }
    let mut order = [0, 1, 2];
    order.sort_by(|&i, &j| a[i][i].total_cmp(&a[j][j]));
    (order.map(|k| a[k][k]), std::array::from_fn(|i| std::array::from_fn(|j| v[i][order[j]])))
}

/// A turn fitted over some surfaces.
#[derive(Clone, Debug)]
pub struct Turn {
    /// The small rotation (radians about x, y, z) that, applied to the scan, brings its surfaces
    /// square to the model's normals; 0 along any direction the surfaces do not say.
    pub omega: [f64; 3],
    /// Directions (unit) the surfaces say nothing about (a turn about a wall's own normal, say).
    pub unsaid: Vec<[f64; 3]>,
}

impl Turn {
    /// The same as a turn about the vertical (radians, anticlockwise from above), and a slope
    /// to level out: what rises by this per metre along x and along y would be level after it.
    pub fn yaw(&self) -> f64 {
        self.omega[2]
    }
    pub fn level(&self) -> [f64; 2] {
        [self.omega[1], -self.omega[0]]
    }
}

/// The turn that best fits these surfaces together. A direction is said only if turning about
/// it by 1 mm per metre would move the points, rms, by more than a twentieth of a millimetre.
pub fn fit(patches: &[&Patch]) -> Option<Turn> {
    let (mut a, mut b, mut w) = ([[0.0; 3]; 3], [0.0; 3], 0.0);
    for p in patches {
        let (pa, pb) = p.equations();
        for i in 0..3 {
            b[i] += pb[i];
            for j in 0..3 {
                a[i][j] += pa[i][j];
            }
        }
        w += p.w;
    }
    if w <= 0.0 {
        return None;
    }
    let (vals, vecs) = eigen(&a);
    let (mut omega, mut unsaid) = ([0.0; 3], vec![]);
    for k in 0..3 {
        let e: [f64; 3] = std::array::from_fn(|i| vecs[i][k]);
        // (ωᵀ A ω / w is the mean square the points move by, for a turn ω.)
        if vals[k] / w < (0.05e-3f64 / 1e-3).powi(2) {
            unsaid.push(e);
            continue;
        }
        let c = (0..3).map(|i| e[i] * b[i]).sum::<f64>() / vals[k];
        for i in 0..3 {
            omega[i] += c * e[i];
        }
    }
    Some(Turn { omega, unsaid })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A patch of points on the plane through `c` square to `n`, then turned by `-omega` (as a
    /// scan out by that would have them).
    fn patch(n: [f64; 3], c: [f64; 3], half: f64, omega: [f64; 3]) -> Patch {
        let u = if n[2].abs() > 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 0.0, 1.0] };
        let v = [n[1] * u[2] - n[2] * u[1], n[2] * u[0] - n[0] * u[2], n[0] * u[1] - n[1] * u[0]];
        let mut p = Patch::new(n);
        for i in -10..=10 {
            for j in -10..=10 {
                let (a, b) = (half * i as f64 / 10.0, half * j as f64 / 10.0);
                let q: [f64; 3] = std::array::from_fn(|k| c[k] + a * u[k] + b * v[k]);
                let w = omega.map(|x| -x);
                let turned = [q[0] + w[1] * q[2] - w[2] * q[1], q[1] + w[2] * q[0] - w[0] * q[2], q[2] + w[0] * q[1] - w[1] * q[0]];
                p.add(turned, 1.0);
            }
        }
        p
    }

    #[test]
    fn a_turn_is_found_from_surfaces_together_and_what_they_do_not_say_is_said() {
        let omega = [0.002, -0.003, 0.004];
        let floor = patch([0.0, 0.0, 1.0], [3.0, 5.0, 0.0], 2.0, omega);
        let west = patch([1.0, 0.0, 0.0], [0.0, 5.0, 1.2], 1.0, omega);
        let north = patch([0.0, -1.0, 0.0], [3.0, 10.0, 1.2], 1.0, omega);
        let t = fit(&[&floor, &west, &north]).unwrap();
        for k in 0..3 {
            assert!((t.omega[k] - omega[k]).abs() < 2e-5, "{:?}", t.omega);
        }
        assert!(t.unsaid.is_empty());
        // As a slope to level: the floor, out by this, rises 3 mm per metre along x... (ω_y = -0.003: it falls).
        assert!((t.level()[0] + 0.003).abs() < 2e-5 && (t.level()[1] + 0.002).abs() < 2e-5 && (t.yaw() - 0.004).abs() < 2e-5);
        // A floor alone says nothing of a turn about the vertical; a wall alone, nothing of one about its normal.
        let t = fit(&[&floor]).unwrap();
        assert!(t.unsaid.len() == 1 && t.unsaid[0][2].abs() > 0.999 && t.omega[2].abs() < 1e-9 && (t.omega[0] - omega[0]).abs() < 2e-5);
        let t = fit(&[&west]).unwrap();
        assert!(t.unsaid.len() == 1 && t.unsaid[0][0].abs() > 0.999 && (t.omega[1] - omega[1]).abs() < 2e-5 && (t.omega[2] - omega[2]).abs() < 2e-5);
        // Where a surface is along its normal does not matter: the same wall 12 mm further in fits the same.
        let lined = patch([1.0, 0.0, 0.0], [0.012, 5.0, 1.2], 1.0, omega);
        let t2 = fit(&[&floor, &lined, &north]).unwrap();
        assert!((0..3).all(|k| (t2.omega[k] - omega[k]).abs() < 2e-5));
        assert!(floor.rms() < 0.012 && floor.rms() > 0.001);
    }
}
