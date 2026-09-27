//! A small plane-frame solver (direct stiffness), for trusses in their own
//! plane: chords continuous through panel points, different members pinned
//! to each other (each member has its own rotation at a node).

/// Element in the plane: nodes `i`, `j`; rotation DOF indices `ri`, `rj`.
#[derive(Clone, Debug)]
pub struct Elem {
    pub i: usize,
    pub j: usize,
    pub ri: usize,
    pub rj: usize,
    pub e: f64,
    pub a: f64,
    pub iz: f64,
    /// Uniform load per unit length in global (s, z) components.
    pub w: [f64; 2],
    /// Point loads: (distance from i, global (s, z) force).
    pub points: Vec<(f64, [f64; 2])>,
}

pub struct Frame2d {
    pub nodes: Vec<[f64; 2]>,
    /// Number of rotation DOFs (indexed 0..n_rot, placed after translations).
    pub n_rot: usize,
    pub elems: Vec<Elem>,
    /// Nodal forces (s, z).
    pub loads: Vec<[f64; 2]>,
    /// Restrained translations: (node, 0 = s / 1 = z).
    pub fixed: Vec<(usize, usize)>,
}

/// Local end forces and geometry of a solved element.
#[derive(Clone, Debug)]
pub struct ElemResult {
    pub len: f64,
    /// Unit axis (s, z) from i to j.
    pub ex: [f64; 2],
    /// Forces on the element at i and j in local (axial, transverse, moment).
    pub fi: [f64; 3],
    pub fj: [f64; 3],
    /// Local displacements (u, v, θ) at i and j.
    pub di: [f64; 3],
    pub dj: [f64; 3],
}

pub struct Solution {
    /// Displacements (s, z) per node.
    pub disp: Vec<[f64; 2]>,
    pub elems: Vec<ElemResult>,
    /// Reactions (force from the support on the structure) per `fixed` entry.
    pub reactions: Vec<f64>,
}

fn local_enl(len: f64, q: [f64; 2], points: &[(f64, [f64; 2])]) -> [f64; 6] {
    // Equivalent nodal loads (local u, v, θ at i then j).
    let (qx, qy) = (q[0], q[1]);
    let mut f = [qx * len / 2.0, qy * len / 2.0, qy * len * len / 12.0, qx * len / 2.0, qy * len / 2.0, -qy * len * len / 12.0];
    for &(a, [px, py]) in points {
        let b = len - a;
        let l3 = len.powi(3);
        f[0] += px * b / len;
        f[3] += px * a / len;
        f[1] += py * b * b * (3.0 * a + b) / l3;
        f[2] += py * a * b * b / (len * len);
        f[4] += py * a * a * (a + 3.0 * b) / l3;
        f[5] -= py * a * a * b / (len * len);
    }
    f
}

fn local_k(e: f64, a: f64, iz: f64, l: f64) -> [[f64; 6]; 6] {
    let (ea, ei) = (e * a / l, e * iz);
    let (k1, k2, k3, k4) = (12.0 * ei / l.powi(3), 6.0 * ei / (l * l), 4.0 * ei / l, 2.0 * ei / l);
    [
        [ea, 0.0, 0.0, -ea, 0.0, 0.0],
        [0.0, k1, k2, 0.0, -k1, k2],
        [0.0, k2, k3, 0.0, -k2, k4],
        [-ea, 0.0, 0.0, ea, 0.0, 0.0],
        [0.0, -k1, -k2, 0.0, k1, -k2],
        [0.0, k2, k4, 0.0, -k2, k3],
    ]
}

/// Solves `a x = b` by Gaussian elimination with partial pivoting.
#[allow(clippy::needless_range_loop)] // index-based elimination reads clearest
pub fn solve_dense(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    let scale = a.iter().flatten().fold(0.0f64, |m, v| m.max(v.abs())).max(1e-300);
    for c in 0..n {
        let p = (c..n).max_by(|&x, &y| a[x][c].abs().total_cmp(&a[y][c].abs()))?;
        if a[p][c].abs() < 1e-12 * scale {
            return None;
        }
        a.swap(c, p);
        b.swap(c, p);
        for r in c + 1..n {
            let f = a[r][c] / a[c][c];
            if f != 0.0 {
                for k in c..n {
                    a[r][k] -= f * a[c][k];
                }
                b[r] -= f * b[c];
            }
        }
    }
    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        let s: f64 = (r + 1..n).map(|k| a[r][k] * x[k]).sum();
        x[r] = (b[r] - s) / a[r][r];
    }
    Some(x)
}

impl Frame2d {
    fn dofs(&self, e: &Elem) -> [usize; 6] {
        let nt = 2 * self.nodes.len();
        [2 * e.i, 2 * e.i + 1, nt + e.ri, 2 * e.j, 2 * e.j + 1, nt + e.rj]
    }

    fn geom(&self, e: &Elem) -> (f64, f64, f64) {
        let (p, q) = (self.nodes[e.i], self.nodes[e.j]);
        let (ds, dz) = (q[0] - p[0], q[1] - p[1]);
        let l = (ds * ds + dz * dz).sqrt();
        (l, ds / l, dz / l)
    }

    /// Local load components for an element: global (s, z) → (axial, transverse).
    fn to_local(c: f64, s: f64, g: [f64; 2]) -> [f64; 2] {
        [g[0] * c + g[1] * s, -g[0] * s + g[1] * c]
    }

    pub fn solve(&self) -> Result<Solution, String> {
        let nt = 2 * self.nodes.len();
        let n = nt + self.n_rot;
        let mut k = vec![vec![0.0; n]; n];
        let mut f = vec![0.0; n];
        for (i, l) in self.loads.iter().enumerate() {
            f[2 * i] += l[0];
            f[2 * i + 1] += l[1];
        }
        let mut locals = vec![];
        for e in &self.elems {
            let (len, c, s) = self.geom(e);
            if len < 1e-9 {
                return Err("zero-length element".into());
            }
            let kl = local_k(e.e, e.a, e.iz, len);
            let pts: Vec<(f64, [f64; 2])> = e.points.iter().map(|&(a, g)| (a, Self::to_local(c, s, g))).collect();
            let enl = local_enl(len, Self::to_local(c, s, e.w), &pts);
            // T maps global → local: [c s 0; -s c 0; 0 0 1] per node.
            let t = |v: [f64; 6]| -> [f64; 6] { [c * v[0] + s * v[1], -s * v[0] + c * v[1], v[2], c * v[3] + s * v[4], -s * v[3] + c * v[4], v[5]] };
            let tt = |v: [f64; 6]| -> [f64; 6] { [c * v[0] - s * v[1], s * v[0] + c * v[1], v[2], c * v[3] - s * v[4], s * v[3] + c * v[4], v[5]] };
            let d = self.dofs(e);
            // Global stiffness Tᵀ k T, column by column.
            for col in 0..6 {
                let mut unit = [0.0; 6];
                unit[col] = 1.0;
                let lu = t(unit);
                let mut kl_u = [0.0; 6];
                for r in 0..6 {
                    kl_u[r] = (0..6).map(|m| kl[r][m] * lu[m]).sum();
                }
                let g = tt(kl_u);
                for r in 0..6 {
                    k[d[r]][d[col]] += g[r];
                }
            }
            let ge = tt(enl);
            for r in 0..6 {
                f[d[r]] += ge[r];
            }
            locals.push((len, c, s, kl, enl));
        }
        // Reduce out restrained translations.
        let fixed: Vec<usize> = self.fixed.iter().map(|&(nd, ax)| 2 * nd + ax).collect();
        let free: Vec<usize> = (0..n).filter(|d| !fixed.contains(d)).collect();
        let kr: Vec<Vec<f64>> = free.iter().map(|&r| free.iter().map(|&c| k[r][c]).collect()).collect();
        let fr: Vec<f64> = free.iter().map(|&r| f[r]).collect();
        let xr = solve_dense(kr, fr).ok_or("the truss is unstable (a mechanism) with these supports")?;
        let mut u = vec![0.0; n];
        for (i, &d) in free.iter().enumerate() {
            u[d] = xr[i];
        }
        let reactions = fixed.iter().map(|&d| (0..n).map(|c| k[d][c] * u[c]).sum::<f64>() - f[d]).collect();
        let mut elems = vec![];
        for (e, (len, c, s, kl, enl)) in self.elems.iter().zip(locals) {
            let d = self.dofs(e);
            let ug = [u[d[0]], u[d[1]], u[d[2]], u[d[3]], u[d[4]], u[d[5]]];
            let ul = [c * ug[0] + s * ug[1], -s * ug[0] + c * ug[1], ug[2], c * ug[3] + s * ug[4], -s * ug[3] + c * ug[4], ug[5]];
            let mut fe = [0.0; 6];
            for r in 0..6 {
                fe[r] = (0..6).map(|m| kl[r][m] * ul[m]).sum::<f64>() - enl[r];
            }
            elems.push(ElemResult {
                len,
                ex: [c, s],
                fi: [fe[0], fe[1], fe[2]],
                fj: [fe[3], fe[4], fe[5]],
                di: [ul[0], ul[1], ul[2]],
                dj: [ul[3], ul[4], ul[5]],
            });
        }
        let disp = (0..self.nodes.len()).map(|i| [u[2 * i], u[2 * i + 1]]).collect();
        Ok(Solution { disp, elems, reactions })
    }

    /// Internal forces (N tension +, V, M sagging +) and transverse
    /// displacement at distance `x` from node i of element `k`.
    pub fn internal(&self, sol: &Solution, k: usize, x: f64) -> (f64, f64, f64, f64) {
        let e = &self.elems[k];
        let r = &sol.elems[k];
        let (c, s) = (r.ex[0], r.ex[1]);
        let q = Self::to_local(c, s, e.w);
        let (mut qx_int, mut qy_int, mut qy_mom) = (q[0] * x, q[1] * x, q[1] * x * x / 2.0);
        for &(a, g) in &e.points {
            if a < x {
                let p = Self::to_local(c, s, g);
                qx_int += p[0];
                qy_int += p[1];
                qy_mom += p[1] * (x - a);
            }
        }
        let n = -(r.fi[0] + qx_int);
        let v = r.fi[1] + qy_int;
        let m = -r.fi[2] + r.fi[1] * x + qy_mom;
        // Transverse displacement: Hermite interpolation of the end values
        // plus the fixed-end particular solution for the uniform part.
        let l = r.len;
        let xi = x / l;
        let (h1, h2, h3, h4) = (1.0 - 3.0 * xi * xi + 2.0 * xi.powi(3), l * (xi - 2.0 * xi * xi + xi.powi(3)), 3.0 * xi * xi - 2.0 * xi.powi(3), l * (-xi * xi + xi.powi(3)));
        let mut v_disp = h1 * r.di[1] + h2 * r.di[2] + h3 * r.dj[1] + h4 * r.dj[2];
        if e.e * e.iz > 0.0 {
            v_disp += q[1] * x * x * (l - x) * (l - x) / (24.0 * e.e * e.iz);
        }
        (n, v, m, v_disp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Simply supported beam, uniform load: reactions wL/2, midspan moment
    /// wL²/8 and deflection 5wL⁴/384EI.
    #[test]
    fn simple_beam() {
        let (l, w, e, iz) = (4.0, -1000.0, 10e9, 1e-4);
        let f = Frame2d {
            nodes: vec![[0.0, 0.0], [l, 0.0]],
            n_rot: 2,
            elems: vec![Elem { i: 0, j: 1, ri: 0, rj: 1, e, a: 0.01, iz, w: [0.0, w], points: vec![] }],
            loads: vec![[0.0; 2]; 2],
            fixed: vec![(0, 0), (0, 1), (1, 1)],
        };
        let s = f.solve().unwrap();
        assert!((s.reactions[1] - 2000.0).abs() < 1e-6 && (s.reactions[2] - 2000.0).abs() < 1e-6);
        let (_, v, m, d) = f.internal(&s, 0, l / 2.0);
        assert!(v.abs() < 1e-6);
        assert!((m - 1000.0 * l * l / 8.0).abs() < 1e-6, "{m}");
        assert!((d + 5.0 * 1000.0 * l.powi(4) / (384.0 * e * iz)).abs() < 1e-9, "{d}");
    }

    /// A king-post truss (pinned joints between members) under a ridge load:
    /// close to the classic pin-jointed statics answer. Not exact: the
    /// bottom chord is continuous through the post, so a little load goes by
    /// bending (frame action); with real 2x4 proportions that is under 1 %.
    #[test]
    fn king_post_truss_statics() {
        // Heels at (0,0), (4,0); peak (2,1.5); post from (2,0) to peak.
        let nodes = vec![[0.0, 0.0], [2.0, 0.0], [4.0, 0.0], [2.0, 1.5]];
        // 2x4 on edge: A = 5.25 in², I = 5.36 in⁴.
        let el = |i, j, ri, rj| Elem { i, j, ri, rj, e: 11e9, a: 3.387e-3, iz: 2.231e-6, w: [0.0; 2], points: vec![] };
        // Bottom chord (one member through node 1), two top chords, post.
        let elems = vec![el(0, 1, 0, 1), el(1, 2, 1, 2), el(0, 3, 3, 4), el(2, 3, 5, 6), el(1, 3, 7, 8)];
        let mut loads = vec![[0.0; 2]; 4];
        loads[3] = [0.0, -10_000.0];
        let f = Frame2d { nodes, n_rot: 9, elems, loads, fixed: vec![(0, 0), (0, 1), (2, 1)] };
        let s = f.solve().unwrap();
        assert!((s.reactions[1] - 5000.0).abs() < 1e-3 && (s.reactions[2] - 5000.0).abs() < 1e-3);
        // Top chord compression: 5000 / sin θ, sin θ = 1.5/2.5.
        let (n_top, _, _, _) = f.internal(&s, 2, 0.5);
        assert!((n_top + 5000.0 / 0.6).abs() < 0.01 * 8333.0, "top chord {n_top}");
        // Bottom chord tension: 5000 / tan θ = 6666.7; the post carries ~0.
        let (n_bot, _, _, _) = f.internal(&s, 0, 0.5);
        assert!((n_bot - 5000.0 / 0.75).abs() < 0.01 * 6667.0, "bottom chord {n_bot}");
        let (n_post, _, _, _) = f.internal(&s, 4, 0.5);
        assert!(n_post.abs() < 0.01 * 10_000.0, "post {n_post}");
    }
}
