//! A 3D frame solver (direct stiffness, Euler–Bernoulli beams): six degrees
//! of freedom per node, elements with axial, torsional and two bending
//! stiffnesses, and bending ends that are rigid, pinned or sprung (a
//! connector's rotational stiffness). Loads are nodal forces and moments and
//! uniform loads along elements, in global axes.
//!
//! The stiffness is factored once and every load case solved against it.
//! A degree of freedom nothing restrains (a node only pinned members meet
//! can spin) is held at zero if no load works on it; a mechanism under load
//! is an error.

use topo_core::Vec3;

/// A bending end: `None` is continuous with the node; `Some([k_u, k_v])` is
/// a rotational spring about the element's u and v axes (N·m/rad; 0 = pin).
pub type EndRelease = Option<[f64; 2]>;

#[derive(Clone, Debug)]
pub struct Elem {
    pub i: usize,
    pub j: usize,
    /// The section's depth direction (unit, perpendicular to the axis).
    pub v: Vec3,
    pub ea: f64,
    pub gj: f64,
    /// E·I about the u axis: bending that deflects along `v`.
    pub ei_u: f64,
    /// E·I about the v axis: bending that deflects along u.
    pub ei_v: f64,
    pub rel_i: EndRelease,
    pub rel_j: EndRelease,
}

pub struct Frame3d {
    pub nodes: Vec<Vec3>,
    pub elems: Vec<Elem>,
    /// Restrained degrees of freedom: (node, 0–2 translation x/y/z, 3–5 rotation).
    pub fixed: Vec<(usize, usize)>,
}

/// One load case.
#[derive(Clone, Debug)]
pub struct Case {
    /// Force and moment per node (global).
    pub nodal: Vec<[f64; 6]>,
    /// Uniform load per unit length per element (global).
    pub w: Vec<Vec3>,
}

impl Case {
    pub fn zeros(nodes: usize, elems: usize) -> Case {
        Case { nodal: vec![[0.0; 6]; nodes], w: vec![Vec3::ZERO; elems] }
    }
}

#[derive(Clone, Debug)]
pub struct ElemResult {
    pub len: f64,
    /// Local axes (x along i → j, u, v).
    pub x: Vec3,
    pub u: Vec3,
    pub v: Vec3,
    /// Forces and moments on the element's i end, local (x, u, v).
    pub f: Vec3,
    pub m: Vec3,
    /// And on its j end.
    pub fj: Vec3,
    pub mj: Vec3,
    /// Uniform load, local.
    pub q: Vec3,
    /// Node displacements at i and j, global.
    pub di: Vec3,
    pub dj: Vec3,
}

/// Internal forces and displacement at a point of an element.
#[derive(Clone, Copy, Debug, Default)]
pub struct Section {
    /// Axial force, tension positive.
    pub n: f64,
    /// Shear along v and along u.
    pub v_v: f64,
    pub v_u: f64,
    /// Torque.
    pub t: f64,
    /// Bending moment deflecting along v (sagging toward +v positive) and along u.
    pub m_v: f64,
    pub m_u: f64,
    /// Displacement, global.
    pub d: Vec3,
}

pub struct Solution {
    /// Displacement (translation, rotation) per node.
    pub disp: Vec<[f64; 6]>,
    pub elems: Vec<ElemResult>,
    /// Reaction (force or moment from the support on the structure) per `fixed` entry.
    pub reactions: Vec<f64>,
    /// What it takes to hold each degree of freedom nothing restrains (as `Solved`'s list): zero
    /// where no load works on it; otherwise the structure is a mechanism there, held by this.
    pub held: Vec<f64>,
}

/// Why a frame could not be solved.
#[derive(Clone, Debug, PartialEq)]
pub enum SolveError {
    ZeroLength { elem: usize },
    /// Nothing resists degree of freedom `dof` (0–2 x/y/z, 3–5 rotations) of `node` under the loads.
    Mechanism { node: usize, dof: usize },
}

impl SolveError {
    pub fn dof_name(dof: usize) -> &'static str {
        ["movement along x", "movement along y", "movement along z", "rotation about x", "rotation about y", "rotation about z"][dof % 6]
    }
}

impl std::fmt::Display for SolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SolveError::ZeroLength { elem } => write!(f, "element {elem} has zero length"),
            SolveError::Mechanism { node, dof } => write!(f, "the frame is a mechanism under its loads: nothing resists {} at node {node}", SolveError::dof_name(*dof)),
        }
    }
}

type M12 = [[f64; 12]; 12];

/// A solution per case, and the (node, dof) held because nothing restrains them.
pub type Solved = (Vec<Solution>, Vec<(usize, usize)>);

fn local_k(e: &Elem, l: f64) -> M12 {
    let mut k = [[0.0; 12]; 12];
    let mut set = |r: usize, c: usize, v: f64| {
        k[r][c] = v;
        k[c][r] = v;
    };
    let (a, t) = (e.ea / l, e.gj / l);
    set(0, 0, a);
    set(0, 6, -a);
    set(6, 6, a);
    set(3, 3, t);
    set(3, 9, -t);
    set(9, 9, t);
    // Deflection along u, rotation about v.
    let b = e.ei_v / l.powi(3);
    set(1, 1, 12.0 * b);
    set(1, 5, 6.0 * b * l);
    set(1, 7, -12.0 * b);
    set(1, 11, 6.0 * b * l);
    set(5, 5, 4.0 * b * l * l);
    set(5, 7, -6.0 * b * l);
    set(5, 11, 2.0 * b * l * l);
    set(7, 7, 12.0 * b);
    set(7, 11, -6.0 * b * l);
    set(11, 11, 4.0 * b * l * l);
    // Deflection along v, rotation about u (a positive rotation lowers the far end).
    let c = e.ei_u / l.powi(3);
    set(2, 2, 12.0 * c);
    set(2, 4, -6.0 * c * l);
    set(2, 8, -12.0 * c);
    set(2, 10, -6.0 * c * l);
    set(4, 4, 4.0 * c * l * l);
    set(4, 8, 6.0 * c * l);
    set(4, 10, 2.0 * c * l * l);
    set(8, 8, 12.0 * c);
    set(8, 10, 6.0 * c * l);
    set(10, 10, 4.0 * c * l * l);
    k
}

/// Equivalent nodal loads of a uniform local load `q`.
fn local_enl(q: Vec3, l: f64) -> [f64; 12] {
    let mut f = [0.0; 12];
    (f[0], f[6]) = (q.x * l / 2.0, q.x * l / 2.0);
    (f[1], f[7]) = (q.y * l / 2.0, q.y * l / 2.0);
    (f[5], f[11]) = (q.y * l * l / 12.0, -q.y * l * l / 12.0);
    (f[2], f[8]) = (q.z * l / 2.0, q.z * l / 2.0);
    (f[4], f[10]) = (-q.z * l * l / 12.0, q.z * l * l / 12.0);
    f
}

/// Puts a rotational spring of stiffness `ks` in series with local degree
/// of freedom `r` (the element's own end rotation is condensed out). The
/// same reduction applies to each load vector.
fn spring(k: &mut M12, fs: &mut [[f64; 12]], r: usize, ks: f64) {
    let kqq = k[r][r];
    let den = kqq + ks;
    if den <= 0.0 {
        return;
    }
    let row = k[r];
    for a in 0..12 {
        for b in 0..12 {
            if a != r && b != r {
                k[a][b] -= row[a] * row[b] / den;
            }
        }
    }
    for a in 0..12 {
        if a != r {
            let v = row[a] * ks / den;
            k[a][r] = v;
            k[r][a] = v;
        }
    }
    k[r][r] = ks * kqq / den;
    for f in fs.iter_mut() {
        let fq = f[r];
        for a in 0..12 {
            if a != r {
                f[a] -= row[a] * fq / den;
            }
        }
        f[r] = ks * fq / den;
    }
}

/// A symmetric matrix kept by its skyline: of each column, the entries from
/// its first non-zero row down to the diagonal. Factored in place as
/// Uᵀ D U (no pivoting: the stiffness of a held structure is positive
/// definite). A degree of freedom with no stiffness left is held at zero
/// and listed.
struct Skyline {
    /// First row kept, per column.
    first: Vec<usize>,
    /// Per column: rows `first..=j`.
    cols: Vec<Vec<f64>>,
    dead: Vec<bool>,
}

impl Skyline {
    fn new(first: Vec<usize>) -> Skyline {
        let cols = first.iter().enumerate().map(|(j, &f)| vec![0.0; j - f + 1]).collect();
        Skyline { dead: vec![false; first.len()], first, cols }
    }
    fn add(&mut self, r: usize, c: usize, v: f64) {
        let (i, j) = if r <= c { (r, c) } else { (c, r) };
        self.cols[j][i - self.first[j]] += v;
    }
    #[allow(clippy::needless_range_loop)] // index-based elimination reads clearest
    fn factor(&mut self) {
        let n = self.first.len();
        let diag0: Vec<f64> = (0..n).map(|j| self.cols[j][j - self.first[j]]).collect();
        let scale = diag0.iter().fold(0.0f64, |m, v| m.max(v.abs())).max(1e-300);
        let mut d = vec![0.0; n];
        for j in 0..n {
            let fj = self.first[j];
            // Reduce the column against the columns before it.
            for i in fj + 1..j {
                let fi = self.first[i];
                let from = fi.max(fj);
                let mut sum = 0.0;
                for k in from..i {
                    sum += self.cols[i][k - fi] * self.cols[j][k - fj];
                }
                self.cols[j][i - fj] -= sum;
            }
            let mut dj = self.cols[j][j - fj];
            for i in fj..j {
                let g = self.cols[j][i - fj];
                let u = if self.dead[i] { 0.0 } else { g / d[i] };
                self.cols[j][i - fj] = u;
                dj -= g * u;
            }
            if dj <= 1e-9 * diag0[j].abs() || diag0[j].abs() <= 1e-13 * scale {
                // Nothing resists it: held, and apart from the rest.
                self.dead[j] = true;
                for v in self.cols[j].iter_mut() {
                    *v = 0.0;
                }
                dj = 1.0;
            }
            d[j] = dj;
            self.cols[j][j - fj] = dj;
        }
    }
    fn solve(&self, b: &[f64]) -> Vec<f64> {
        let n = self.first.len();
        let mut x = b.to_vec();
        for j in 0..n {
            let fj = self.first[j];
            if self.dead[j] {
                x[j] = 0.0;
                continue;
            }
            let mut sum = 0.0;
            for i in fj..j {
                sum += self.cols[j][i - fj] * x[i];
            }
            x[j] -= sum;
        }
        for j in 0..n {
            x[j] = if self.dead[j] { 0.0 } else { x[j] / self.cols[j][j - self.first[j]] };
        }
        for j in (0..n).rev() {
            let fj = self.first[j];
            let xj = x[j];
            if xj != 0.0 {
                for i in fj..j {
                    x[i] -= self.cols[j][i - fj] * xj;
                }
            }
        }
        x
    }
}

/// An order of the nodes that keeps joined ones near each other (reverse
/// Cuthill–McKee), so the stiffness's skyline is shallow.
fn node_order(n: usize, elems: &[Elem]) -> Vec<usize> {
    let mut adj: Vec<Vec<usize>> = vec![vec![]; n];
    for e in elems {
        if !adj[e.i].contains(&e.j) {
            adj[e.i].push(e.j);
            adj[e.j].push(e.i);
        }
    }
    let mut order = Vec::with_capacity(n);
    let mut seen = vec![false; n];
    let mut by_degree: Vec<usize> = (0..n).collect();
    by_degree.sort_by_key(|&i| adj[i].len());
    for &start in &by_degree {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let mut queue = std::collections::VecDeque::from([start]);
        while let Some(i) = queue.pop_front() {
            order.push(i);
            let mut next: Vec<usize> = adj[i].iter().copied().filter(|&j| !seen[j]).collect();
            next.sort_by_key(|&j| adj[j].len());
            for j in next {
                seen[j] = true;
                queue.push_back(j);
            }
        }
    }
    order.reverse();
    order
}

impl Frame3d {
    fn axes(&self, elem: usize) -> Result<(f64, Vec3, Vec3, Vec3), SolveError> {
        let e = &self.elems[elem];
        let d = self.nodes[e.j] - self.nodes[e.i];
        let l = d.norm();
        if l < 1e-9 {
            return Err(SolveError::ZeroLength { elem });
        }
        let x = d / l;
        let v = e.v.reject(x).try_normalized().unwrap_or_else(|| x.any_perpendicular());
        Ok((l, x, v.cross(x), v))
    }

    /// Solves every case. A degree of freedom nothing restrains is held at
    /// zero: each is returned as (node, dof), and what holding it takes is in
    /// each solution's `held` (zero where no load works on it).
    #[allow(clippy::needless_range_loop)] // index-based assembly reads clearest
    pub fn solve(&self, cases: &[Case]) -> Result<Solved, SolveError> {
        let n = 6 * self.nodes.len();
        let nc = cases.len();
        // Equations: the free degrees of freedom, node by node in an order that keeps the skyline shallow.
        let mut is_fixed = vec![false; n];
        for &(nd, d) in &self.fixed {
            is_fixed[6 * nd + d] = true;
        }
        let mut eq = vec![usize::MAX; n];
        let mut free = vec![];
        for node in node_order(self.nodes.len(), &self.elems) {
            for d in 0..6 {
                if !is_fixed[6 * node + d] {
                    eq[6 * node + d] = free.len();
                    free.push(6 * node + d);
                }
            }
        }
        let mut first: Vec<usize> = (0..free.len()).collect();
        for e in &self.elems {
            let eqs: Vec<usize> = (0..6).flat_map(|d| [eq[6 * e.i + d], eq[6 * e.j + d]]).filter(|&q| q != usize::MAX).collect();
            if let Some(&lo) = eqs.iter().min() {
                for &q in &eqs {
                    first[q] = first[q].min(lo);
                }
            }
        }
        let mut k = Skyline::new(first);
        let mut f = vec![vec![0.0; n]; nc];
        for (c, case) in cases.iter().enumerate() {
            for (i, l) in case.nodal.iter().enumerate() {
                for d in 0..6 {
                    f[c][6 * i + d] += l[d];
                }
            }
        }
        // Per element: geometry, condensed local stiffness and load vectors.
        let mut locals = vec![];
        for (ei, e) in self.elems.iter().enumerate() {
            let (len, x, u, v) = self.axes(ei)?;
            let rot = [x, u, v];
            let to_local = |g: Vec3| Vec3::new(g.dot(x), g.dot(u), g.dot(v));
            let mut kl = local_k(e, len);
            let qs: Vec<Vec3> = cases.iter().map(|c| to_local(c.w[ei])).collect();
            let mut enl: Vec<[f64; 12]> = qs.iter().map(|q| local_enl(*q, len)).collect();
            for (rel, base) in [(e.rel_i, 0), (e.rel_j, 6)] {
                if let Some([ku, kv]) = rel {
                    spring(&mut kl, &mut enl, base + 4, ku);
                    spring(&mut kl, &mut enl, base + 5, kv);
                }
            }
            // Global degree of freedom and local direction of each local one.
            let dof = |a: usize| -> (usize, Vec3) {
                let node = if a < 6 { e.i } else { e.j };
                (6 * node + if a % 6 < 3 { 0 } else { 3 }, rot[a % 3])
            };
            for a in 0..12 {
                let (ga, da) = dof(a);
                let ca = [da.x, da.y, da.z];
                for (c, load) in enl.iter().enumerate() {
                    for p in 0..3 {
                        f[c][ga + p] += ca[p] * load[a];
                    }
                }
                for b in 0..12 {
                    let kab = kl[a][b];
                    if kab == 0.0 {
                        continue;
                    }
                    let (gb, db) = dof(b);
                    let cb = [db.x, db.y, db.z];
                    for p in 0..3 {
                        for q in 0..3 {
                            let (r, cc) = (eq[ga + p], eq[gb + q]);
                            // (Each pair once: the matrix is symmetric, and kept by its upper half.)
                            if r != usize::MAX && cc != usize::MAX && r <= cc {
                                k.add(r, cc, ca[p] * kab * cb[q]);
                            }
                        }
                    }
                }
            }
            locals.push((len, x, u, v, kl, enl, qs));
        }
        k.factor();
        let held: Vec<(usize, usize)> = (0..free.len()).filter(|&q| k.dead[q]).map(|q| (free[q] / 6, free[q] % 6)).collect();
        let mut out = vec![];
        for c in 0..nc {
            let x = k.solve(&free.iter().map(|&d| f[c][d]).collect::<Vec<_>>());
            let mut u = vec![0.0; n];
            for (i, &d) in free.iter().enumerate() {
                u[d] = x[i];
            }
            let node = |i: usize| -> (Vec3, Vec3) { (Vec3::new(u[6 * i], u[6 * i + 1], u[6 * i + 2]), Vec3::new(u[6 * i + 3], u[6 * i + 4], u[6 * i + 5])) };
            // What the elements take from each node, less what is applied there: zero where a
            // node is free, and elsewhere what holds it.
            let mut residual = vec![0.0; n];
            for (d, r) in residual.iter_mut().enumerate() {
                *r = -cases[c].nodal[d / 6][d % 6];
            }
            let elems: Vec<ElemResult> = self
                .elems
                .iter()
                .zip(&locals)
                .map(|(e, (len, x, u_ax, v, kl, enl, qs))| {
                    let ((ti, ri), (tj, rj)) = (node(e.i), node(e.j));
                    let loc = |g: Vec3| [g.dot(*x), g.dot(*u_ax), g.dot(*v)];
                    let dl: Vec<f64> = [loc(ti), loc(ri), loc(tj), loc(rj)].concat();
                    let end: Vec<f64> = (0..12).map(|a| (0..12).map(|b| kl[a][b] * dl[b]).sum::<f64>() - enl[c][a]).collect();
                    for (base, at) in [(0, e.i), (6, e.j)] {
                        let (fo, mo) = (*x * end[base] + *u_ax * end[base + 1] + *v * end[base + 2], *x * end[base + 3] + *u_ax * end[base + 4] + *v * end[base + 5]);
                        for (d, val) in [fo.x, fo.y, fo.z, mo.x, mo.y, mo.z].into_iter().enumerate() {
                            residual[6 * at + d] += val;
                        }
                    }
                    ElemResult { len: *len, x: *x, u: *u_ax, v: *v, f: Vec3::new(end[0], end[1], end[2]), m: Vec3::new(end[3], end[4], end[5]), fj: Vec3::new(end[6], end[7], end[8]), mj: Vec3::new(end[9], end[10], end[11]), q: qs[c], di: ti, dj: tj }
                })
                .collect();
            let reactions = self.fixed.iter().map(|&(nd, d)| residual[6 * nd + d]).collect();
            let holding = held.iter().map(|&(nd, d)| residual[6 * nd + d]).collect();
            let disp = (0..self.nodes.len()).map(|i| [u[6 * i], u[6 * i + 1], u[6 * i + 2], u[6 * i + 3], u[6 * i + 4], u[6 * i + 5]]).collect();
            out.push(Solution { disp, elems, reactions, held: holding });
        }
        Ok((out, held))
    }

    /// Internal forces and displacement at distance `s` from end i of element `k`.
    pub fn section(&self, sol: &Solution, k: usize, s: f64) -> Section {
        let (e, r) = (&self.elems[k], &sol.elems[k]);
        let (l, f, m, q) = (r.len, r.f, r.m, r.q);
        // Free body of the part from i to the section.
        let m_v = m.y + s * f.z + q.z * s * s / 2.0;
        let m_u = -m.z + s * f.y + q.y * s * s / 2.0;
        // Deflection: the end displacements, plus the curvature integrated
        // twice with the slope that closes on the far end.
        let bend = |ei: f64, m0: f64, v0: f64, w: f64, a: f64, b: f64| -> f64 {
            let part = |t: f64| if ei > 0.0 { (m0 * t * t / 2.0 + v0 * t.powi(3) / 6.0 + w * t.powi(4) / 24.0) / ei } else { 0.0 };
            let slope = (b - a - part(l)) / l;
            a + slope * s + part(s)
        };
        let loc = |g: Vec3| (g.dot(r.x), g.dot(r.u), g.dot(r.v));
        let ((ax, au, av), (bx, bu, bv)) = (loc(r.di), loc(r.dj));
        let dx = ax + (bx - ax) * s / l;
        let dv = bend(e.ei_u, m.y, f.z, q.z, av, bv);
        let du = bend(e.ei_v, -m.z, f.y, q.y, au, bu);
        Section { n: -(f.x + q.x * s), v_v: f.z + q.z * s, v_u: f.y + q.y * s, t: -m.x, m_v, m_u, d: r.x * dx + r.u * du + r.v * dv }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use topo_core::v3;

    const E: f64 = 200e9;
    const G: f64 = 77e9;
    // A rectangular section with different stiffness each way.
    const A: f64 = 2e-3;
    const IU: f64 = 4e-6;
    const IV: f64 = 1e-6;
    const J: f64 = 2e-6;

    fn el(i: usize, j: usize) -> Elem {
        Elem { i, j, v: Vec3::Z, ea: E * A, gj: G * J, ei_u: E * IU, ei_v: E * IV, rel_i: None, rel_j: None }
    }
    fn all(node: usize) -> Vec<(usize, usize)> {
        (0..6).map(|d| (node, d)).collect()
    }
    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-9 * b.abs().max(1e-12)
    }

    /// Cantilever with a tip force each way and a tip torque: PL³/3EI about
    /// the right axis, PL/EA, TL/GJ; root reactions by statics.
    #[test]
    fn cantilever() {
        let l = 2.0;
        let f = Frame3d { nodes: vec![v3(0., 0., 0.), v3(l, 0., 0.)], elems: vec![el(0, 1)], fixed: all(0) };
        let mut c = Case::zeros(2, 1);
        c.nodal[1] = [1000.0, 200.0, -500.0, 50.0, 0.0, 0.0];
        let (s, held) = f.solve(&[c]).unwrap();
        assert!(held.is_empty());
        let d = s[0].disp[1];
        assert!(close(d[0], 1000.0 * l / (E * A)), "{d:?}");
        assert!(close(d[1], 200.0 * l.powi(3) / (3.0 * E * IV)), "{d:?}");
        assert!(close(d[2], -500.0 * l.powi(3) / (3.0 * E * IU)), "{d:?}");
        assert!(close(d[3], 50.0 * l / (G * J)), "{d:?}");
        // Reactions: forces opposite, moments from the lever arm.
        let r = &s[0].reactions;
        assert!(close(r[0], -1000.0) && close(r[1], -200.0) && close(r[2], 500.0) && close(r[3], -50.0));
        assert!(close(r[4], -500.0 * l) && close(r[5], -200.0 * l), "{r:?}");
        // At the root the moments are P·L; shear and axial are constant.
        let x = f.section(&s[0], 0, 0.0);
        assert!(close(x.n, 1000.0) && close(x.m_v.abs(), 500.0 * l) && close(x.m_u.abs(), 200.0 * l) && close(x.t.abs(), 50.0), "{x:?}");
        // Halfway: the cubic deflection P x²(3L − x)/6EI.
        let h = f.section(&s[0], 0, l / 2.0);
        assert!(close(h.d.z, -500.0 * (l / 2.0).powi(2) * (3.0 * l - l / 2.0) / (6.0 * E * IU)), "{h:?}");
    }

    /// Simply supported beam under a uniform load, in a skewed direction so
    /// the transformation is exercised: wL²/8 and 5wL⁴/384EI.
    #[test]
    fn simple_beam_uniform_load() {
        let l = 3.0;
        let dir = v3(1.0, 2.0, 0.0).normalized();
        let f = Frame3d {
            nodes: vec![v3(0., 0., 0.), dir * l],
            elems: vec![el(0, 1)],
            // Pin at 0; at 1 a roller in z, held in x against turning in plan.
            // (Spin about the beam's own axis is unrestrained and unloaded.)
            fixed: vec![(0, 0), (0, 1), (0, 2), (1, 2), (1, 0)],
        };
        let mut c = Case::zeros(2, 1);
        c.w[0] = v3(0.0, 0.0, -800.0);
        let (s, _) = f.solve(&[c]).unwrap();
        let mid = f.section(&s[0], 0, l / 2.0);
        assert!(close(mid.m_v, 800.0 * l * l / 8.0), "{mid:?}");
        assert!(close(mid.d.z, -5.0 * 800.0 * l.powi(4) / (384.0 * E * IU)), "{mid:?}");
        assert!(mid.v_v.abs() < 1e-6);
        assert!(close(s[0].reactions[2], 1200.0) && close(s[0].reactions[3], 1200.0));
    }

    /// An L in plan, fixed at one end, loaded down at the free tip: bending
    /// of both legs plus the twist of the first.
    #[test]
    fn bent_cantilever_bends_and_twists() {
        let (a, b, p) = (1.5, 1.0, 300.0);
        let f = Frame3d { nodes: vec![v3(0., 0., 0.), v3(a, 0., 0.), v3(a, b, 0.)], elems: vec![el(0, 1), el(1, 2)], fixed: all(0) };
        let mut c = Case::zeros(3, 2);
        c.nodal[2][2] = -p;
        let (s, _) = f.solve(&[c]).unwrap();
        let want = p * a.powi(3) / (3.0 * E * IU) + p * b.powi(3) / (3.0 * E * IU) + p * b * b * a / (G * J);
        assert!(close(s[0].disp[2][2], -want), "{} vs {want}", s[0].disp[2][2]);
        assert!(close(f.section(&s[0], 0, 0.1).t.abs(), p * b));
    }

    /// Ends on rotational springs: between pinned (k = 0) and fixed (k → ∞),
    /// with the closed-form end moment M = k q L³ / (24EI + 12kL).
    #[test]
    fn sprung_ends() {
        let (l, q) = (2.0, 1000.0);
        let ei = E * IU;
        let run = |k: Option<f64>| {
            let mut e = el(0, 1);
            e.rel_i = k.map(|k| [k, k]);
            e.rel_j = k.map(|k| [k, k]);
            let f = Frame3d { nodes: vec![v3(0., 0., 0.), v3(l, 0., 0.)], elems: vec![e], fixed: [all(0), all(1)].concat() };
            let mut c = Case::zeros(2, 1);
            c.w[0] = v3(0.0, 0.0, -q);
            let (s, _) = f.solve(&[c]).unwrap();
            let mid = f.section(&s[0], 0, l / 2.0);
            (mid.m_v, mid.d.z)
        };
        let pinned = run(Some(0.0));
        assert!(close(pinned.0, q * l * l / 8.0) && close(pinned.1, -5.0 * q * l.powi(4) / (384.0 * ei)), "{pinned:?}");
        let fixed = run(None);
        assert!(close(fixed.0, q * l * l / 24.0) && close(fixed.1, -q * l.powi(4) / (384.0 * ei)), "{fixed:?}");
        let k = 3.0 * ei / l;
        let me = k * q * l.powi(3) / (24.0 * ei + 12.0 * k * l);
        let semi = run(Some(k));
        assert!(close(semi.0, q * l * l / 8.0 - me), "{semi:?} vs end moment {me}");
        assert!(close(semi.1, -(5.0 * q * l.powi(4) / (384.0 * ei) - me * l * l / (8.0 * ei))), "{semi:?}");
    }

    /// A node only pinned members meet can spin freely: held, not an error.
    /// A frame that can move under its load is an error.
    #[test]
    fn free_spin_is_held_and_mechanisms_are_reported() {
        let pin = |i, j| Elem { rel_i: Some([0.0, 0.0]), rel_j: Some([0.0, 0.0]), ..el(i, j) };
        // A pin-jointed triangle in the x–z plane, braced out of plane.
        let nodes = vec![v3(0., 0., 0.), v3(2., 0., 0.), v3(1., 0., 1.)];
        let fixed = vec![(0, 0), (0, 1), (0, 2), (1, 1), (1, 2), (2, 1)];
        let f = Frame3d { nodes: nodes.clone(), elems: vec![pin(0, 1), pin(1, 2), pin(0, 2)], fixed: fixed.clone() };
        let mut c = Case::zeros(3, 3);
        c.nodal[2][2] = -1000.0;
        let (s, held) = f.solve(std::slice::from_ref(&c)).unwrap();
        assert!(!held.is_empty());
        // Statics of the truss: each rafter carries 500/sin 45° in compression.
        assert!(close(f.section(&s[0], 1, 0.3).n, -500.0 * 2f64.sqrt()), "{:?}", f.section(&s[0], 1, 0.3));
        assert!(close(s[0].reactions[2], 500.0));
        // Without the tie and the roller's partner, it spreads: a mechanism, held, and what
        // holds it is not nothing.
        let g = Frame3d { nodes, elems: vec![pin(1, 2), pin(0, 2)], fixed };
        let (s, held) = g.solve(&[Case { w: vec![Vec3::ZERO; 2], ..c }]).unwrap();
        assert!(held.iter().zip(&s[0].held).any(|(h, f)| h.1 < 3 && f.abs() > 100.0), "{held:?} {:?}", s[0].held);
    }
}
