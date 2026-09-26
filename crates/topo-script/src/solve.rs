//! Fits a script's `unknown(...)` parameters to its field measurements.
//!
//! The script is the model: for trial parameter values it is re-evaluated,
//! built, and each measurement is evaluated on the resulting geometry.
//! Levenberg–Marquardt minimises the tolerance-weighted residuals. The report
//! gives fitted values with 1σ uncertainties (assuming each measurement is
//! good to its tolerance), per-measurement misfits, and which unknowns the
//! measurements leave undetermined.

use crate::spec::{build_scene, SceneSpec, UnknownSpec};
use crate::{parse_spec, Script, ScriptError};
use serde::Serialize;
use std::collections::BTreeMap;
use topo_core::{Model, Topology};
use topo_geom::measure::{evaluate, Measurement};
use topo_geom::Geometry;

#[derive(Clone, Debug, Serialize)]
pub struct UnknownResult {
    pub name: String,
    pub unit: String,
    pub guess: f64,
    pub value: f64,
    /// 1σ uncertainty; `None` when the measurements do not determine it.
    pub sigma: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MeasurementResult {
    pub name: String,
    pub quantity: topo_geom::measure::Quantity,
    pub measured: f64,
    /// Value in the fitted model (`None` if it could not be evaluated).
    pub model: Option<f64>,
    pub tolerance: f64,
    pub error: Option<String>,
}

impl MeasurementResult {
    /// Misfit in units of the measurement's tolerance.
    pub fn misfit(&self) -> Option<f64> {
        self.model.map(|v| (v - self.measured) / self.tolerance)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct SolveReport {
    pub unknowns: Vec<UnknownResult>,
    pub measurements: Vec<MeasurementResult>,
    pub iterations: usize,
    /// Root-mean-square misfit in tolerance units (≲ 1 means consistent).
    pub rms: f64,
    /// Measurements minus unknowns.
    pub redundancy: i64,
}

impl SolveReport {
    pub fn undetermined(&self) -> Vec<&str> {
        self.unknowns.iter().filter(|u| u.sigma.is_none()).map(|u| u.name.as_str()).collect()
    }
}

fn build(spec: &SceneSpec) -> Result<(Model, Geometry), ScriptError> {
    let model = build_scene(spec).map_err(ScriptError::Spec)?;
    let geom = Geometry::build(&model, &Topology::build(&model));
    Ok((model, geom))
}

fn residuals(model: &Model, geom: &Geometry, ms: &[Measurement]) -> Vec<Result<f64, String>> {
    ms.iter().map(|m| evaluate(model, geom, &m.quantity).map(|v| (v - m.value) / m.tolerance)).collect()
}

struct Problem<'a> {
    script: &'a Script,
    unknowns: Vec<UnknownSpec>,
    measurements: Vec<Measurement>,
}

impl Problem<'_> {
    fn params(&self, x: &[f64]) -> BTreeMap<String, f64> {
        self.unknowns.iter().zip(x).map(|(u, &v)| (u.name.clone(), v)).collect()
    }
    fn clamp(&self, x: &mut [f64]) {
        for (u, v) in self.unknowns.iter().zip(x.iter_mut()) {
            if let Some(lo) = u.min {
                *v = v.max(lo);
            }
            if let Some(hi) = u.max {
                *v = v.min(hi);
            }
        }
    }
    /// Weighted residual vector, or `None` if the model fails to build or a
    /// measurement cannot be evaluated at these parameters.
    fn eval(&self, x: &[f64]) -> Option<Vec<f64>> {
        let spec = parse_spec(&self.script.eval(Some(&self.params(x))).ok()?).ok()?;
        let (model, geom) = build(&spec).ok()?;
        residuals(&model, &geom, &self.measurements).into_iter().collect::<Result<Vec<_>, _>>().ok()
    }
    fn jacobian(&self, x: &[f64], r0: &[f64]) -> Option<Vec<Vec<f64>>> {
        // Columns by forward differences.
        let mut cols = vec![];
        for j in 0..x.len() {
            let h = 1e-6 * x[j].abs().max(1e-2);
            let mut xp = x.to_vec();
            xp[j] += h;
            let rp = self.eval(&xp)?;
            cols.push(rp.iter().zip(r0).map(|(a, b)| (a - b) / h).collect::<Vec<f64>>());
        }
        Some(cols)
    }
}

/// Solves `A x = b` (small, dense) with partial pivoting; `None` if singular.
#[allow(clippy::needless_range_loop)] // row operations index two rows at once
fn solve_linear(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for c in 0..n {
        let p = (c..n).max_by(|&i, &j| a[i][c].abs().total_cmp(&a[j][c].abs()))?;
        if a[p][c].abs() < 1e-300 {
            return None;
        }
        a.swap(c, p);
        b.swap(c, p);
        for r in c + 1..n {
            let f = a[r][c] / a[c][c];
            for k in c..n {
                a[r][k] -= f * a[c][k];
            }
            b[r] -= f * b[c];
        }
    }
    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        x[r] = (b[r] - (r + 1..n).map(|k| a[r][k] * x[k]).sum::<f64>()) / a[r][r];
    }
    Some(x)
}

fn normal_equations(cols: &[Vec<f64>], r: &[f64]) -> (Vec<Vec<f64>>, Vec<f64>) {
    let n = cols.len();
    let a = (0..n).map(|i| (0..n).map(|j| cols[i].iter().zip(&cols[j]).map(|(p, q)| p * q).sum()).collect()).collect();
    let g = (0..n).map(|i| cols[i].iter().zip(r).map(|(p, q)| p * q).sum()).collect();
    (a, g)
}

fn cost(r: &[f64]) -> f64 {
    r.iter().map(|v| v * v).sum()
}

/// Builds the script's model, fitting unknowns to measurements if it has any.
pub fn solve(script: &Script) -> Result<(Model, Option<SolveReport>), ScriptError> {
    let spec0 = parse_spec(&script.eval(None)?)?;
    if spec0.unknowns.is_empty() && spec0.measurements.is_empty() {
        return Ok((build(&spec0)?.0, None));
    }
    let prob = Problem { script, unknowns: spec0.unknowns.clone(), measurements: spec0.measurements.clone() };
    let mut x: Vec<f64> = prob.unknowns.iter().map(|u| u.guess).collect();
    // The starting point must evaluate; otherwise report the first failure.
    let (m0, g0) = build(&spec0)?;
    for (m, r) in prob.measurements.iter().zip(residuals(&m0, &g0, &prob.measurements)) {
        r.map_err(|e| ScriptError::Spec(format!("measurement \"{}\": {e}", m.name)))?;
    }
    let mut r = prob.eval(&x).ok_or_else(|| ScriptError::Spec("model does not build at the initial guesses".into()))?;
    let mut iterations = 0;
    let n = x.len();
    if n > 0 {
        let mut lambda = 1e-3;
        for it in 0..100 {
            iterations = it + 1;
            let Some(cols) = prob.jacobian(&x, &r) else { break };
            let (a, g) = normal_equations(&cols, &r);
            let c0 = cost(&r);
            let mut improved = false;
            while lambda < 1e12 {
                let mut damped = a.clone();
                for (i, row) in damped.iter_mut().enumerate() {
                    row[i] += lambda * (a[i][i] + 1e-12);
                }
                let Some(dx) = solve_linear(damped, g.iter().map(|v| -v).collect()) else {
                    lambda *= 10.0;
                    continue;
                };
                let mut xn: Vec<f64> = x.iter().zip(&dx).map(|(a, b)| a + b).collect();
                prob.clamp(&mut xn);
                match prob.eval(&xn) {
                    Some(rn) if cost(&rn) < c0 => {
                        let step: f64 = dx.iter().zip(&x).map(|(d, v)| (d / v.abs().max(1e-2)).abs()).fold(0.0, f64::max);
                        x = xn;
                        r = rn;
                        lambda = (lambda / 10.0).max(1e-12);
                        improved = step > 1e-12;
                        break;
                    }
                    _ => lambda *= 10.0,
                }
            }
            if !improved || cost(&r) < 1e-20 {
                break;
            }
        }
    }

    // Final model and report.
    let spec = parse_spec(&script.eval(Some(&prob.params(&x)))?)?;
    let (model, geom) = build(&spec)?;
    let values: Vec<Result<f64, String>> = prob.measurements.iter().map(|m| evaluate(&model, &geom, &m.quantity)).collect();
    let sigmas: Vec<Option<f64>> = match prob.jacobian(&x, &r) {
        Some(cols) => {
            let (a, _) = normal_equations(&cols, &r);
            let scale = a.iter().enumerate().map(|(i, row)| row[i]).fold(0.0, f64::max).max(1e-300);
            // Unknowns no measurement depends on are undetermined; invert the
            // normal matrix over the rest (still singular ⇒ only combinations
            // of them are determined, so none is individually).
            let keep: Vec<usize> = (0..n).filter(|&j| a[j][j] > 1e-12 * scale).collect();
            let reduced: Vec<Vec<f64>> = keep.iter().map(|&i| keep.iter().map(|&j| a[i][j]).collect()).collect();
            let mut out = vec![None; n];
            for (col, &j) in keep.iter().enumerate() {
                let e: Vec<f64> = (0..keep.len()).map(|i| if i == col { 1.0 } else { 0.0 }).collect();
                out[j] = solve_linear(reduced.clone(), e).and_then(|c| (c[col] > 0.0 && c[col] < 1e12 / scale).then(|| c[col].sqrt()));
            }
            out
        }
        None => vec![None; n],
    };
    let rms = (cost(&r) / r.len().max(1) as f64).sqrt();
    let report = SolveReport {
        unknowns: prob
            .unknowns
            .iter()
            .zip(&x)
            .zip(&sigmas)
            .map(|((u, &v), s)| UnknownResult { name: u.name.clone(), unit: u.unit.clone(), guess: u.guess, value: v, sigma: *s })
            .collect(),
        measurements: prob
            .measurements
            .iter()
            .zip(values)
            .map(|(m, v)| MeasurementResult {
                name: m.name.clone(),
                quantity: m.quantity.clone(),
                measured: m.value,
                model: v.as_ref().ok().copied(),
                tolerance: m.tolerance,
                error: v.err(),
            })
            .collect(),
        iterations,
        rms,
        redundancy: prob.measurements.len() as i64 - n as i64,
    };
    Ok((model, Some(report)))
}

fn fmt_unknown(u: &UnknownResult, v: f64) -> String {
    if u.unit == "length" {
        topo_core::units::fmt_ft_in(v)
    } else if u.name.contains("pitch") {
        format!("{v:.4} ({:.2}:12)", v * 12.0)
    } else {
        format!("{v:.4}")
    }
}

impl SolveReport {
    /// Rows for a "fitted unknowns" table: name, value, ± 1σ.
    pub fn unknown_rows(&self) -> Vec<Vec<String>> {
        self.unknowns
            .iter()
            .map(|u| {
                let sigma = match u.sigma {
                    Some(s) if u.unit == "length" => format!("± {}", topo_core::units::fmt_inches(s)),
                    Some(s) => format!("± {s:.4}"),
                    None => "NOT DETERMINED".into(),
                };
                vec![u.name.clone(), fmt_unknown(u, u.value), sigma]
            })
            .collect()
    }

    /// Rows for a "field measurements" table: name, measured, model, difference, status.
    pub fn measurement_rows(&self) -> Vec<Vec<String>> {
        use topo_core::units::{fmt_ft_in, fmt_inches};
        self.measurements
            .iter()
            .map(|m| match (m.model, &m.error) {
                (Some(v), _) => {
                    let d = v - m.measured;
                    // No sign on differences that round to zero (at 1/16").
                    let zero = (d.abs() / topo_core::units::INCH * 16.0).round() == 0.0;
                    let sign = if zero { "" } else if d < 0.0 { "-" } else { "+" };
                    let status = if m.misfit().is_some_and(|f| f.abs() <= 1.0) { "ok" } else { "CHECK" };
                    vec![m.name.clone(), fmt_ft_in(m.measured), fmt_ft_in(v), format!("{sign}{}", fmt_inches(d.abs())), status.into()]
                }
                (None, e) => vec![m.name.clone(), fmt_ft_in(m.measured), "—".into(), "—".into(), e.clone().unwrap_or_default()],
            })
            .collect()
    }

    /// Plain-text summary for terminals and report files.
    pub fn text(&self) -> String {
        let mut s = format!(
            "fit: {} unknowns, {} measurements (redundancy {}), {} iterations, rms misfit {:.3} tolerances\n",
            self.unknowns.len(),
            self.measurements.len(),
            self.redundancy,
            self.iterations,
            self.rms
        );
        for r in self.unknown_rows() {
            s.push_str(&format!("  {:<28} {:<24} {}\n", r[0], r[1], r[2]));
        }
        s.push_str("  measurements (measured → model):\n");
        for r in self.measurement_rows() {
            s.push_str(&format!("  {:<34} {:>12} → {:<12} {:>8}  {}\n", r[0], r[1], r[2], r[3], r[4]));
        }
        let und = self.undetermined();
        if !und.is_empty() {
            s.push_str(&format!("  not determined by the measurements: {}\n", und.join(", ")));
        }
        s
    }
}
