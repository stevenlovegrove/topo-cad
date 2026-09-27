//! North American dimension lumber: nominal → dressed sizes, species/grades.
//!
//! Reference design values are from the NDS Supplement (2018), Table 4A,
//! visually graded dimension lumber, 2"–4" thick. They are data for the NDS
//! design-code plug-in; verify against the edition adopted by the AHJ.

use topo_core::units::{inch, psi, PCF};
use topo_core::{Material, MaterialId, Section, Shape};

/// Dressed (actual) dimension in inches for a nominal dimension (dry, S4S).
pub fn dressed_in(nominal: u32) -> f64 {
    match nominal {
        1 => 0.75,
        2 => 1.5,
        3 => 2.5,
        4 => 3.5,
        6 => 5.5,
        8 => 7.25,
        10 => 9.25,
        12 => 11.25,
        14 => 13.25,
        16 => 15.25,
        n => n as f64 - 0.75,
    }
}

/// Sawn lumber section, e.g. `sawn(2, 10)` → 1½" × 9¼" named `2x10`.
/// Thickness (first) is along u, width (second) along v (depth).
pub fn sawn(thick: u32, width: u32) -> Section {
    let name = format!("{thick}x{width}");
    Section::rect(&name, inch(dressed_in(thick)), inch(dressed_in(width)))
        .with_tag("nominal", &name)
        .with_tag("family", "sawn")
}

/// Built-up section of `plies` sawn members, e.g. `(2) 2x10`.
pub fn built_up(plies: u32, thick: u32, width: u32, gap_in: f64) -> Section {
    let name = format!("({plies}) {thick}x{width}");
    Section::built_up(
        &name,
        Shape::Rect { b: inch(dressed_in(thick)), d: inch(dressed_in(width)) },
        plies,
        inch(gap_in),
    )
    .with_tag("nominal", &format!("{thick}x{width}"))
    .with_tag("family", "sawn")
}

/// Board feet of a sawn section of given length (by nominal size).
pub fn board_feet(section: &Section, length_m: f64) -> Option<f64> {
    let nominal = section.tags.get("nominal")?;
    let (t, w) = nominal.split_once('x')?;
    let (t, w): (f64, f64) = (t.parse().ok()?, w.parse().ok()?);
    Some(t * w * (length_m / inch(12.0)) / 12.0 * section.plies as f64)
}

/// NDS reference design values (psi) for a visually graded species/grade,
/// from the reference data (with provenance) in `topo-data`.
pub fn nds_reference(species: &str, grade: &str) -> Option<&'static topo_data::LumberValues> {
    topo_data::lumber(species, grade)
}

fn species_name(code: &str) -> &str {
    match code {
        "DFL" => "DF-L",
        "SPF" => "SPF",
        other => other,
    }
}

/// Material for a visually graded species/grade. Stiffness uses the NDS
/// reference E; density from the specific gravity at ~12 % MC.
pub fn graded(species: &str, grade: &str) -> Material {
    let r = nds_reference(species, grade).unwrap_or_else(|| panic!("no NDS Table 4A row for {species} {grade}"));
    let e = psi(r.e);
    Material {
        id: MaterialId(0),
        name: format!("{} {}", species_name(species), grade),
        family: "sawn_lumber".into(),
        e,
        g: e / 16.0,
        // Approx. oven-dry SG scaled to ~12 % MC: ρ ≈ 62.4·G·(1 + MC) pcf.
        density: 62.4 * r.g * 1.12 * PCF,
        design_key: Some(format!("NDS:{species}:{grade}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dressed_sizes_and_board_feet() {
        let s = sawn(2, 10);
        assert!((s.props.depth - inch(9.25)).abs() < 1e-12);
        // 2x10x12' = 20 bf
        assert!((board_feet(&s, inch(144.0)).unwrap() - 20.0).abs() < 1e-9);
        let h = built_up(2, 2, 10, 0.5);
        assert!((h.props.width - inch(3.5)).abs() < 1e-12);
    }

    #[test]
    fn dfl2_material() {
        let m = graded("DFL", "No.2");
        assert_eq!(m.design_key.as_deref(), Some("NDS:DFL:No.2"));
        assert!((m.e / 1e9 - 11.03).abs() < 0.01);
    }
}
