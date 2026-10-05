//! Tube and bar sections, named by their size in millimetres.

use crate::mm;
use topo_core::Section;

/// Round tube `od` × `wall` (a rod if `wall` is zero): `Ø28.6 x 0.8`.
pub fn round(od: f64, wall: f64) -> Section {
    let name = if wall > 0.0 && wall < od / 2.0 { format!("Ø{} x {}", mm(od), mm(wall)) } else { format!("Ø{} rod", mm(od)) };
    Section::round(name, od, wall).with_tag("family", "tube")
}

/// Rectangular tube `b` × `d` × `wall`, or a solid bar without a wall.
pub fn rect(b: f64, d: f64, wall: Option<f64>) -> Section {
    match wall.filter(|w| *w > 0.0 && *w < b.min(d) / 2.0) {
        Some(w) => Section::rect_tube(format!("{} x {} x {}", mm(b), mm(d), mm(w)), b, d, w).with_tag("family", "tube"),
        None => Section::rect(format!("{} x {} bar", mm(b), mm(d)), b, d).with_tag("family", "bar"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_shapes() {
        assert_eq!(round(0.0286, 0.0008).name, "Ø28.6 x 0.8");
        assert_eq!(round(0.01, 0.0).name, "Ø10 rod");
        assert_eq!(rect(0.04, 0.02, Some(0.002)).name, "40 x 20 x 2");
        assert_eq!(rect(0.04, 0.02, None).name, "40 x 20 bar");
        assert!(round(0.0286, 0.0008).is_round());
    }
}
