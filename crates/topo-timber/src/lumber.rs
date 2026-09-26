//! North American dimension lumber: nominal → dressed sizes, species/grades.
//!
//! Reference design values are from the NDS Supplement (2018), Table 4A,
//! visually graded dimension lumber, 2"–4" thick. They are data for the NDS
//! design-code plug-in; verify against the edition adopted by the AHJ.

use serde::{Deserialize, Serialize};
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

/// NDS reference design values (psi) for visually graded dimension lumber.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NdsReference {
    pub species: &'static str,
    pub grade: &'static str,
    pub fb: f64,
    pub ft: f64,
    pub fv: f64,
    pub fc_perp: f64,
    pub fc: f64,
    pub e: f64,
    pub e_min: f64,
    /// Specific gravity (NDS Table 12.3.3A) — used for fastener design.
    pub g: f64,
}

/// Selected rows of NDS Supplement Table 4A (2018).
pub const NDS_TABLE_4A: &[NdsReference] = &[
    NdsReference { species: "DFL", grade: "Select Structural", fb: 1500., ft: 1000., fv: 180., fc_perp: 625., fc: 1700., e: 1_900_000., e_min: 690_000., g: 0.50 },
    NdsReference { species: "DFL", grade: "No.1", fb: 1000., ft: 675., fv: 180., fc_perp: 625., fc: 1500., e: 1_700_000., e_min: 620_000., g: 0.50 },
    NdsReference { species: "DFL", grade: "No.2", fb: 900., ft: 575., fv: 180., fc_perp: 625., fc: 1350., e: 1_600_000., e_min: 580_000., g: 0.50 },
    NdsReference { species: "DFL", grade: "Stud", fb: 700., ft: 450., fv: 180., fc_perp: 625., fc: 850., e: 1_400_000., e_min: 510_000., g: 0.50 },
    NdsReference { species: "SPF", grade: "No.1/No.2", fb: 875., ft: 450., fv: 135., fc_perp: 425., fc: 1150., e: 1_400_000., e_min: 510_000., g: 0.42 },
    NdsReference { species: "SPF", grade: "Stud", fb: 675., ft: 350., fv: 135., fc_perp: 425., fc: 725., e: 1_200_000., e_min: 440_000., g: 0.42 },
];

pub fn nds_reference(species: &str, grade: &str) -> Option<&'static NdsReference> {
    NDS_TABLE_4A.iter().find(|r| r.species == species && r.grade == grade)
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
