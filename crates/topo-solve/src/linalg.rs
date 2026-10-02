//! The little dense linear algebra the solver needs. Problems here are small
//! (a scope's unknowns: tens, not thousands), so clarity is preferred to speed.

/// A matrix as rows.
pub type Mat = Vec<Vec<f64>>;

pub fn zeros(m: usize, n: usize) -> Mat {
    vec![vec![0.0; n]; m]
}

pub fn identity(n: usize) -> Mat {
    let mut a = zeros(n, n);
    for (i, row) in a.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    a
}

pub fn mul(a: &Mat, b: &Mat) -> Mat {
    let (m, k) = (a.len(), b.len());
    let n = b.first().map_or(0, Vec::len);
    let mut c = zeros(m, n);
    for i in 0..m {
        for l in 0..k {
            let x = a[i][l];
            if x != 0.0 {
                for j in 0..n {
                    c[i][j] += x * b[l][j];
                }
            }
        }
    }
    c
}

pub fn mul_vec(a: &Mat, x: &[f64]) -> Vec<f64> {
    a.iter().map(|row| row.iter().zip(x).map(|(p, q)| p * q).sum()).collect()
}

/// The eigenvalues and eigenvectors (as columns) of a symmetric matrix, by Jacobi rotations.
pub fn eig_sym(a: &Mat) -> (Vec<f64>, Mat) {
    let n = a.len();
    let (mut a, mut v) = (a.clone(), identity(n));
    for _ in 0..100 {
        let off: f64 = (0..n).flat_map(|i| (0..i).map(move |j| (i, j))).map(|(i, j)| a[i][j] * a[i][j]).sum();
        let diag: f64 = (0..n).map(|i| a[i][i] * a[i][i]).sum();
        if off <= 1e-30 * diag.max(1e-300) {
            break;
        }
        for p in 0..n {
            for q in p + 1..n {
                if a[p][q].abs() < 1e-300 {
                    continue;
                }
                let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let t = if theta == 0.0 { 1.0 } else { t };
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for k in 0..n {
                    let (akp, akq) = (a[k][p], a[k][q]);
                    a[k][p] = c * akp - s * akq;
                    a[k][q] = s * akp + c * akq;
                }
                for k in 0..n {
                    let (apk, aqk) = (a[p][k], a[q][k]);
                    a[p][k] = c * apk - s * aqk;
                    a[q][k] = s * apk + c * aqk;
                }
                for k in 0..n {
                    let (vkp, vkq) = (v[k][p], v[k][q]);
                    v[k][p] = c * vkp - s * vkq;
                    v[k][q] = s * vkp + c * vkq;
                }
            }
        }
    }
    ((0..n).map(|i| a[i][i]).collect(), v)
}

/// The least-norm `z` minimising |A z + b| (A: m × k), a basis (as columns)
/// of what A leaves free, and A's rank. With no rows, everything is free.
pub fn lsq_null(a: &Mat, b: &[f64], k: usize) -> (Vec<f64>, Mat, usize) {
    if a.is_empty() || k == 0 {
        return (vec![0.0; k], identity(k), 0);
    }
    // The normal matrix, and its eigenvectors: those with (near) no eigenvalue are what A does not see.
    let mut g = zeros(k, k);
    for row in a {
        for i in 0..k {
            if row[i] != 0.0 {
                for j in 0..k {
                    g[i][j] += row[i] * row[j];
                }
            }
        }
    }
    let (lam, v) = eig_sym(&g);
    let tol = (lam.iter().cloned().fold(0.0, f64::max) * 1e-10).max(1e-16);
    let atb: Vec<f64> = (0..k).map(|j| a.iter().zip(b).map(|(row, bi)| row[j] * bi).sum()).collect();
    let mut z = vec![0.0; k];
    let mut free = Vec::new();
    for i in 0..k {
        if lam[i] > tol {
            let c: f64 = (0..k).map(|j| v[j][i] * atb[j]).sum::<f64>() / lam[i];
            for j in 0..k {
                z[j] -= v[j][i] * c;
            }
        } else {
            free.push(i);
        }
    }
    let mut null = zeros(k, free.len());
    for (c, &i) in free.iter().enumerate() {
        for j in 0..k {
            null[j][c] = v[j][i];
        }
    }
    (z, null, k - free.len())
}

/// A basis (as columns) of the same space, with each vector as nearly on one
/// axis as it can be: so a freedom reads as "this unknown", not as a mixture.
pub fn tidy_basis(basis: &Mat) -> Mat {
    let n = basis.len();
    let k = basis.first().map_or(0, Vec::len);
    // Work on the vectors as rows; reduce to echelon form, pivoting on the largest entry.
    let mut rows: Mat = (0..k).map(|c| (0..n).map(|j| basis[j][c]).collect()).collect();
    let mut used = vec![false; n];
    for r in 0..k {
        let Some(p) = (0..n).filter(|&j| !used[j]).max_by(|&a, &b| rows[r][a].abs().total_cmp(&rows[r][b].abs())) else { break };
        if rows[r][p].abs() < 1e-12 {
            continue;
        }
        used[p] = true;
        let d = rows[r][p];
        for x in rows[r].iter_mut() {
            *x /= d;
        }
        for o in 0..k {
            if o != r {
                let f = rows[o][p];
                if f != 0.0 {
                    let pivot = rows[r].clone();
                    for (x, y) in rows[o].iter_mut().zip(&pivot) {
                        *x -= f * y;
                    }
                }
            }
        }
    }
    let mut out = zeros(n, k);
    for (c, row) in rows.iter().enumerate() {
        for j in 0..n {
            out[j][c] = row[j];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn least_norm_and_what_is_left_free() {
        // One equation in three unknowns: x + y = 2. Least norm: (1, 1, 0); two freedoms left.
        let (z, null, rank) = lsq_null(&vec![vec![1.0, 1.0, 0.0]], &[-2.0], 3);
        assert_eq!(rank, 1);
        assert!((z[0] - 1.0).abs() < 1e-12 && (z[1] - 1.0).abs() < 1e-12 && z[2].abs() < 1e-12, "{z:?}");
        assert_eq!(null[0].len(), 2);
        // Tidied, one of the free directions is z alone.
        let t = tidy_basis(&null);
        assert!((0..2).any(|c| t[2][c].abs() > 0.99 && t[0][c].abs() < 1e-9), "{t:?}");
    }
}
