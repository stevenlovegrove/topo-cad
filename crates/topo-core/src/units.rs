//! Unit helpers. The model is always SI (m, N, Pa, kg/m³); these convert at the edges.

use serde::{Deserialize, Serialize};

pub const MM: f64 = 0.001;
pub const INCH: f64 = 0.0254;
pub const FOOT: f64 = 0.3048;

pub const LBF: f64 = 4.448_221_615_260_5;
pub const KIP: f64 = 1000.0 * LBF;
pub const PSI: f64 = LBF / (INCH * INCH);
pub const PSF: f64 = LBF / (FOOT * FOOT);
pub const PLF: f64 = LBF / FOOT;
pub const PCF: f64 = 16.018_463_373_960_14; // lb/ft³ → kg/m³

pub fn mm(v: f64) -> f64 {
    v * MM
}
pub fn inch(v: f64) -> f64 {
    v * INCH
}
pub fn ft(v: f64) -> f64 {
    v * FOOT
}
pub fn ft_in(feet: f64, inches: f64) -> f64 {
    feet * FOOT + inches * INCH
}
pub fn psi(v: f64) -> f64 {
    v * PSI
}
pub fn psf(v: f64) -> f64 {
    v * PSF
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitSystem {
    #[default]
    Imperial,
    Metric,
}

impl UnitSystem {
    /// Drawing/model length unit used for DXF output.
    pub fn drawing_unit(self) -> f64 {
        match self {
            UnitSystem::Imperial => INCH,
            UnitSystem::Metric => MM,
        }
    }
    pub fn fmt_len(self, metres: f64) -> String {
        match self {
            UnitSystem::Imperial => fmt_ft_in(metres),
            UnitSystem::Metric => format!("{:.0}", metres / MM),
        }
    }
    /// Short lengths such as section sizes (inches or mm, no feet).
    pub fn fmt_short(self, metres: f64) -> String {
        match self {
            UnitSystem::Imperial => fmt_inches(metres),
            UnitSystem::Metric => format!("{:.0}", metres / MM),
        }
    }
}

/// Whole number plus reduced fraction of sixteenths, e.g. `3 1/2`.
fn fmt_sixteenths(total16: i64) -> String {
    let whole = total16 / 16;
    let mut num = total16 % 16;
    if num == 0 {
        return whole.to_string();
    }
    let mut den = 16;
    while num % 2 == 0 {
        num /= 2;
        den /= 2;
    }
    if whole == 0 {
        format!("{num}/{den}")
    } else {
        format!("{whole} {num}/{den}")
    }
}

/// Inches rounded to 1/16", e.g. `3 1/2"`.
pub fn fmt_inches(metres: f64) -> String {
    let t = (metres / INCH * 16.0).round() as i64;
    let sign = if t < 0 { "-" } else { "" };
    format!("{sign}{}\"", fmt_sixteenths(t.abs()))
}

/// Feet-inches rounded to 1/16", e.g. `8'-1 1/8"`; below one foot, inches only.
pub fn fmt_ft_in(metres: f64) -> String {
    let t = (metres / INCH * 16.0).round() as i64;
    let sign = if t < 0 { "-" } else { "" };
    let t = t.abs();
    let feet = t / (12 * 16);
    let rem = t % (12 * 16);
    if feet == 0 {
        format!("{sign}{}\"", fmt_sixteenths(rem))
    } else if rem > 0 && rem < 16 {
        // Keep the zero inch: 7'-0 7/8", not 7'-7/8".
        format!("{sign}{feet}'-0 {}\"", fmt_sixteenths(rem))
    } else {
        format!("{sign}{feet}'-{}\"", fmt_sixteenths(rem))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_imperial() {
        assert_eq!(fmt_ft_in(ft_in(8.0, 1.125)), "8'-1 1/8\"");
        assert_eq!(fmt_ft_in(inch(92.625)), "7'-8 5/8\"");
        assert_eq!(fmt_ft_in(inch(3.5)), "3 1/2\"");
        assert_eq!(fmt_ft_in(ft(16.0)), "16'-0\"");
        assert_eq!(fmt_ft_in(ft_in(7.0, 0.875)), "7'-0 7/8\"");
        assert_eq!(fmt_ft_in(inch(0.875)), "7/8\"");
        assert_eq!(fmt_inches(inch(1.5)), "1 1/2\"");
        assert_eq!(fmt_inches(inch(0.25)), "1/4\"");
    }

    #[test]
    fn pressure_conversions() {
        // 1 psf ≈ 47.88 Pa
        assert!((psf(1.0) - 47.880_26).abs() < 1e-3);
        // 1 psi ≈ 6894.76 Pa
        assert!((psi(1.0) - 6894.757).abs() < 1e-2);
    }
}
