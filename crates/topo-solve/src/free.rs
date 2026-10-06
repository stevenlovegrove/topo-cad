//! What the tiers above leave free, and solving a tier within it.
//!
//! A part's step is solved tier by tier, each only within what the tiers above leave free: a
//! space of moves `x = F z`. Held densely, `F` is n × k and every tier costs a dense solve over
//! all of a part's unknowns. Most parts have structure that isolates most unknowns: things
//! (trusses, posts) each touching a few shared ones (the walls they sit on) and nothing else.
//! Held as an **arrowhead**, the unknowns are split into shared ones and blocks, no constraint
//! touching two blocks; a move is then
//!
//! ```text
//! x_shared  = S w_shared
//! x_block_i = B_i w_shared + N_i w_i
//! ```
//!
//! and a tier is solved by eliminating each block's own coordinates against the shared ones
//! (a small dense solve per block), then solving what is left on the shared ones (a small dense
//! solve). What is free afterwards has the same form. Each tier still decides only what the ones
//! above leave free: the result is the dense one's (but for which of the equally good steps is
//! taken where something is left free).

use crate::linalg::{eig_sym, identity, lsq_null, lsq_null_tol, mul, mul_vec, zeros, Mat};

/// Which unknowns (columns of a part) are shared, and the blocks of the rest.
#[derive(Clone, Debug)]
pub struct Partition {
    pub shared: Vec<usize>,
    pub blocks: Vec<Vec<usize>>,
}

#[derive(Clone, Debug)]
pub struct Arrow {
    n: usize,
    shared: Vec<usize>,
    blocks: Vec<Vec<usize>>,
    /// The shared unknowns' move, per shared coordinate.
    s: Mat,
    /// Each block's unknowns' move: per shared coordinate, and per coordinate of its own.
    b: Vec<Mat>,
    nl: Vec<Mat>,
}

#[derive(Clone, Debug)]
pub enum Free {
    Dense(Mat),
    Arrow(Arrow),
}

fn cols(m: &Mat) -> usize {
    m.first().map_or(0, Vec::len)
}

/// `a · b` where `a` has `m` rows (and may have no columns).
fn mul_rows(a: &Mat, b: &Mat, m: usize, k: usize) -> Mat {
    if cols(a) == 0 {
        return zeros(m, k);
    }
    mul(a, b)
}

impl Free {
    /// Everything free: `n` unknowns, as one block of the whole (dense), or split as `p` says.
    pub fn all(n: usize, p: Option<&Partition>) -> Free {
        match p {
            None => Free::Dense(identity(n)),
            Some(p) => Free::Arrow(Arrow {
                n,
                shared: p.shared.clone(),
                blocks: p.blocks.clone(),
                s: identity(p.shared.len()),
                b: p.blocks.iter().map(|b| zeros(b.len(), p.shared.len())).collect(),
                nl: p.blocks.iter().map(|b| identity(b.len())).collect(),
            }),
        }
    }

    /// How many coordinates are free.
    pub fn dim(&self) -> usize {
        match self {
            Free::Dense(f) => cols(f),
            Free::Arrow(a) => a.shared_dim() + a.nl.iter().map(cols).sum::<usize>(),
        }
    }

    /// Whether unknown `j` cannot move.
    pub fn fixes(&self, j: usize) -> bool {
        let zero = |row: &Vec<f64>| row.iter().all(|x| x.abs() < 1e-8);
        match self {
            Free::Dense(f) => zero(&f[j]),
            Free::Arrow(a) => {
                if let Some(r) = a.shared.iter().position(|&c| c == j) {
                    return zero(&a.s[r]);
                }
                for (i, block) in a.blocks.iter().enumerate() {
                    if let Some(r) = block.iter().position(|&c| c == j) {
                        return zero(&a.b[i][r]) && zero(&a.nl[i][r]);
                    }
                }
                true
            }
        }
    }

    /// As an n × k matrix.
    pub fn to_dense(&self) -> Mat {
        match self {
            Free::Dense(f) => f.clone(),
            Free::Arrow(a) => {
                let ks = a.shared_dim();
                let k = ks + a.nl.iter().map(cols).sum::<usize>();
                let mut f = zeros(a.n, k);
                for (r, &c) in a.shared.iter().enumerate() {
                    f[c][..ks].copy_from_slice(&a.s[r]);
                }
                let mut at = ks;
                for (i, block) in a.blocks.iter().enumerate() {
                    let ki = cols(&a.nl[i]);
                    for (r, &c) in block.iter().enumerate() {
                        f[c][..ks].copy_from_slice(&a.b[i][r]);
                        f[c][at..at + ki].copy_from_slice(&a.nl[i][r]);
                    }
                    at += ki;
                }
                f
            }
        }
    }

    /// Within what is free: the move `F z` with `z` minimising `|A F z + b|` (A over all the
    /// part's unknowns), what is then free, and how many coordinates the rows settled.
    pub fn solve(&self, a: &Mat, b: &[f64]) -> (Vec<f64>, Free, usize) {
        match self {
            Free::Dense(f) => {
                let n = f.len();
                let k = cols(f);
                if a.is_empty() || k == 0 {
                    return (vec![0.0; n], self.clone(), 0);
                }
                let af = mul(a, f);
                let (z, null, rank) = lsq_null(&af, b, k);
                (mul_vec(f, &z), Free::Dense(if cols(&null) == 0 { zeros(n, 0) } else { mul(f, &null) }), rank)
            }
            Free::Arrow(arrow) => match arrow.solve(a, b) {
                // The step with least norm, as the dense solve takes: the step found, less its part in
                // what is still free (a step along that would be as good, and only moves things).
                Some((dx, next, rank)) => {
                    let f = next.to_dense();
                    let k = cols(&f);
                    if k == 0 {
                        return (dx, next, rank);
                    }
                    let minus: Vec<f64> = dx.iter().map(|x| -x).collect();
                    let (c, _, _) = lsq_null(&f, &minus, k);
                    let along = mul_vec(&f, &c);
                    (dx.iter().zip(along).map(|(x, y)| x - y).collect(), next, rank)
                }
                // (A row across two blocks: not this structure after all.)
                None => Free::Dense(self.to_dense()).solve(a, b),
            },
        }
    }
}

impl Arrow {
    /// How many shared coordinates are free (none where nothing is shared).
    fn shared_dim(&self) -> usize {
        if self.shared.is_empty() { 0 } else { cols(&self.s) }
    }

    fn solve(&self, a: &Mat, b: &[f64]) -> Option<(Vec<f64>, Free, usize)> {
        let ks = self.shared_dim();
        let nb = self.blocks.len();
        // Which block each unknown is in.
        let mut in_block = vec![usize::MAX; self.n];
        for (i, block) in self.blocks.iter().enumerate() {
            for &c in block {
                in_block[c] = i;
            }
        }
        // Each row, against the free coordinates: its shared part and (in one block) its own part.
        let mut shared_rows: Vec<(Vec<f64>, f64)> = vec![];
        let mut block_rows: Vec<Vec<(Vec<f64>, Vec<f64>, f64)>> = vec![vec![]; nb];
        for (row, &bi) in a.iter().zip(b) {
            let mut which = None;
            for (c, &x) in row.iter().enumerate() {
                if x != 0.0 && in_block[c] != usize::MAX {
                    match which {
                        None => which = Some(in_block[c]),
                        Some(w) if w != in_block[c] => return None,
                        _ => {}
                    }
                }
            }
            let mut sh: Vec<f64> = (0..ks).map(|q| self.shared.iter().enumerate().map(|(r, &c)| row[c] * self.s[r][q]).sum()).collect();
            match which {
                None => shared_rows.push((sh, bi)),
                Some(i) => {
                    let block = &self.blocks[i];
                    for (q, x) in sh.iter_mut().enumerate() {
                        *x += block.iter().enumerate().map(|(r, &c)| row[c] * self.b[i][r][q]).sum::<f64>();
                    }
                    let ki = cols(&self.nl[i]);
                    let own: Vec<f64> = (0..ki).map(|q| block.iter().enumerate().map(|(r, &c)| row[c] * self.nl[i][r][q]).sum()).collect();
                    block_rows[i].push((sh, own, bi));
                }
            }
        }
        // Each block's own coordinates, eliminated: w_i = -P_i (L_i w_s + b_i) + N'_i u_i, and
        // what its rows ask of the shared coordinates is what its own cannot do (Q_i = I - Ll_i P_i).
        struct Elim {
            p: Mat,
            ls: Mat,
            bi: Vec<f64>,
            null: Mat,
            rank: usize,
        }
        // (What counts as no eigenvalue is as for the whole: against the largest of all of them,
        // which is at least that of any block's, or of the shared rows.)
        let normal = |rows: &Mat, k: usize| {
            let mut g = zeros(k, k);
            for row in rows {
                for x in 0..k {
                    if row[x] != 0.0 {
                        for y in 0..k {
                            g[x][y] += row[x] * row[y];
                        }
                    }
                }
            }
            g
        };
        let mut eigs = vec![];
        let mut largest: f64 = 0.0;
        for (i, rows) in block_rows.iter().enumerate() {
            let ki = cols(&self.nl[i]);
            let ll: Mat = rows.iter().map(|r| r.1.clone()).collect();
            let sl: Mat = rows.iter().map(|r| r.0.iter().chain(&r.1).copied().collect()).collect();
            let whole = sl.iter().map(|r| r.iter().map(|x| x * x).sum::<f64>()).fold(0.0, f64::max);
            largest = largest.max(whole);
            let e = if rows.is_empty() || ki == 0 { (vec![], vec![]) } else { eig_sym(&normal(&ll, ki)) };
            largest = largest.max(e.0.iter().cloned().fold(0.0, f64::max));
            eigs.push(e);
        }
        largest = shared_rows.iter().map(|r| r.0.iter().map(|x| x * x).sum::<f64>()).fold(largest, f64::max);
        let tol = (largest * 1e-10).max(1e-16);
        let mut elims = vec![];
        let mut reduced: Vec<(Vec<f64>, f64)> = shared_rows;
        for ((i, rows), (lam, v)) in block_rows.into_iter().enumerate().zip(eigs) {
            let ki = cols(&self.nl[i]);
            let m = rows.len();
            let ls: Mat = rows.iter().map(|r| r.0.clone()).collect();
            let ll: Mat = rows.iter().map(|r| r.1.clone()).collect();
            let bi: Vec<f64> = rows.iter().map(|r| r.2).collect();
            if m == 0 || ki == 0 {
                reduced.extend(ls.iter().cloned().zip(bi.iter().copied()));
                elims.push(Elim { p: zeros(ki, m), ls, bi, null: identity(ki), rank: 0 });
                continue;
            }
            // The pseudo-inverse of Ll, and what it leaves free, from its normal matrix.
            let kept: Vec<usize> = (0..ki).filter(|&q| lam[q] > tol).collect();
            let gone: Vec<usize> = (0..ki).filter(|&q| lam[q] <= tol).collect();
            // P = V_kept diag(1/λ) V_keptᵀ Llᵀ  (ki × m)
            let mut p = zeros(ki, m);
            for &q in &kept {
                let llv: Vec<f64> = ll.iter().map(|row| (0..ki).map(|x| row[x] * v[x][q]).sum()).collect();
                for x in 0..ki {
                    let f = v[x][q] / lam[q];
                    for (y, l) in llv.iter().enumerate() {
                        p[x][y] += f * l;
                    }
                }
            }
            let null: Mat = (0..ki).map(|x| gone.iter().map(|&q| v[x][q]).collect()).collect();
            // Q = I - Ll P
            let llp = mul_rows(&ll, &p, m, m);
            for y in 0..m {
                let qrow: Vec<f64> = (0..m).map(|z| if y == z { 1.0 } else { 0.0 } - llp[y][z]).collect();
                let s: Vec<f64> = (0..ks).map(|q| (0..m).map(|z| qrow[z] * ls[z][q]).sum()).collect();
                let c: f64 = (0..m).map(|z| qrow[z] * bi[z]).sum();
                reduced.push((s, c));
            }
            elims.push(Elim { p, ls, bi, null, rank: kept.len() });
        }
        // The shared coordinates.
        let rs: Mat = reduced.iter().map(|r| r.0.clone()).collect();
        let rb: Vec<f64> = reduced.iter().map(|r| r.1).collect();
        let (ws, ns, rank_s) = if ks == 0 { (vec![], zeros(0, 0), 0) } else { lsq_null_tol(&rs, &rb, ks, Some(tol)) };
        let ks2 = if ks == 0 { 0 } else { cols(&ns) };
        // The move, and what is free now.
        let mut dx = vec![0.0; self.n];
        let ds = if ks == 0 { vec![0.0; self.shared.len()] } else { mul_vec(&self.s, &ws) };
        for (r, &c) in self.shared.iter().enumerate() {
            dx[c] = ds[r];
        }
        let s2 = if ks2 == 0 { zeros(self.shared.len(), 0) } else { mul(&self.s, &ns) };
        let mut b2 = vec![];
        let mut n2 = vec![];
        let mut rank = rank_s;
        for (i, e) in elims.iter().enumerate() {
            let ki = cols(&self.nl[i]);
            let bn = self.blocks[i].len();
            // w_i = -P (Ls ws + bi); its move with the shared that stay free: C = -P Ls Ns.
            let lsws: Vec<f64> = if ks == 0 { vec![0.0; e.bi.len()] } else { e.ls.iter().map(|row| row.iter().zip(&ws).map(|(a, b)| a * b).sum()).collect() };
            let wi: Vec<f64> = (0..ki).map(|x| -(0..e.bi.len()).map(|y| e.p[x][y] * (lsws[y] + e.bi[y])).sum::<f64>()).collect();
            let bw = if ks == 0 { vec![0.0; bn] } else { mul_vec(&self.b[i], &ws) };
            let nw = if ki == 0 { vec![0.0; bn] } else { mul_vec(&self.nl[i], &wi) };
            for (r, &c) in self.blocks[i].iter().enumerate() {
                dx[c] = bw[r] + nw[r];
            }
            let lsns = if ks2 == 0 { zeros(e.bi.len(), 0) } else { mul_rows(&e.ls, &ns, e.bi.len(), ks2) };
            let cm: Mat = (0..ki).map(|x| (0..ks2).map(|q| -(0..e.bi.len()).map(|y| e.p[x][y] * lsns[y][q]).sum::<f64>()).collect()).collect();
            let mut bnew = if ks2 == 0 { zeros(bn, 0) } else { mul(&self.b[i], &ns) };
            if ki > 0 && ks2 > 0 {
                let nc = mul(&self.nl[i], &cm);
                for (row, add) in bnew.iter_mut().zip(nc) {
                    for (x, y) in row.iter_mut().zip(add) {
                        *x += y;
                    }
                }
            }
            b2.push(bnew);
            n2.push(if ki == 0 || cols(&e.null) == 0 { zeros(bn, 0) } else { mul(&self.nl[i], &e.null) });
            rank += e.rank;
        }
        Some((dx, Free::Arrow(Arrow { n: self.n, shared: self.shared.clone(), blocks: self.blocks.clone(), s: s2, b: b2, nl: n2 }), rank))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two blocks and two shared unknowns; three tiers. Solved arrowhead and dense, the same.
    #[test]
    fn an_arrowhead_solves_each_tier_as_the_dense_one_does() {
        // Unknowns: 0, 1 shared; block A: 2, 3, 4; block B: 5, 6.
        let n = 7;
        let p = Partition { shared: vec![0, 1], blocks: vec![vec![2, 3, 4], vec![5, 6]] };
        let row = |e: &[(usize, f64)]| {
            let mut r = vec![0.0; n];
            for &(c, x) in e {
                r[c] = x;
            }
            r
        };
        // Hard: block A's 2 and 3 tied to shared 0; block B's 5 tied to shared 1; 0 + 1 = 1.
        let hard: Mat = vec![row(&[(2, 1.0), (0, -1.0)]), row(&[(3, 1.0), (4, 1.0), (0, -2.0)]), row(&[(5, 1.0), (1, -1.0)]), row(&[(0, 1.0), (1, 1.0)])];
        let hb = vec![0.5, -0.2, 0.1, -1.0];
        // Measured: what is left of A (4) and B (6), and of the shared (0 - 1).
        let meas: Mat = vec![row(&[(4, 1.0)]), row(&[(6, 2.0), (5, 1.0)]), row(&[(0, 1.0), (1, -1.0)])];
        let mb = vec![0.3, -0.7, 0.25];
        // Hints: everything to zero (only what is still free is decided by them).
        let hint: Mat = (0..n).map(|c| row(&[(c, 1.0)])).collect();
        let hb2 = vec![0.0; n];
        let run = |f: Free| {
            let mut dx = vec![0.0; n];
            let mut f = f;
            let mut ranks = vec![];
            for (a, b) in [(&hard, &hb), (&meas, &mb), (&hint, &hb2)] {
                let b: Vec<f64> = mul_vec(a, &dx).iter().zip(b.iter()).map(|(x, y)| x + y).collect();
                let (d, nf, rank) = f.solve(a, &b);
                for (x, y) in dx.iter_mut().zip(d) {
                    *x += y;
                }
                f = nf;
                ranks.push(rank);
            }
            (dx, ranks, f.dim())
        };
        let (dense, rd, kd) = run(Free::all(n, None));
        let (arrow, ra, ka) = run(Free::all(n, Some(&p)));
        assert_eq!((rd, kd), (ra, ka));
        for (x, y) in dense.iter().zip(&arrow) {
            assert!((x - y).abs() < 1e-9, "{dense:?} {arrow:?}");
        }
    }
}
