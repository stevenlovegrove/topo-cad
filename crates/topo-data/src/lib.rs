//! Reference data with provenance.
//!
//! Every table is a JSON file in `data/` (compiled in, so it works in WASM)
//! naming its publication, edition and table, and every row says whether it
//! has been checked against that source (`verified`). Calculations carry the
//! citation into their traces, and unverified values are flagged in reports,
//! so an engineer can see exactly what a number rests on.
//!
//! Units are those of the source (psi, lb, in, psf); callers convert.

use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// Where a table comes from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Source {
    /// e.g. `NDS Supplement`.
    pub publication: String,
    /// e.g. `2018`.
    pub edition: String,
    /// e.g. `Table 4A`.
    pub table: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

impl Source {
    /// `NDS Supplement (2018) Table 4A`.
    pub fn cite(&self) -> String {
        format!("{} ({}) {}", self.publication, self.edition, self.table)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Table<T> {
    pub source: Source,
    pub rows: Vec<T>,
}

/// A value and whether it has been checked against its source.
pub trait Verified {
    fn verified(&self) -> bool;
}

fn load<T: for<'de> Deserialize<'de>>(json: &str, name: &str) -> Table<T> {
    serde_json::from_str(json).unwrap_or_else(|e| panic!("data/{name}: {e}"))
}

macro_rules! table {
    ($fn:ident, $ty:ty, $file:literal) => {
        pub fn $fn() -> &'static Table<$ty> {
            static T: OnceLock<Table<$ty>> = OnceLock::new();
            T.get_or_init(|| load(include_str!(concat!("../data/", $file)), $file))
        }
    };
}

// ----- NDS sawn lumber ----------------------------------------------------------

/// Reference design values for visually graded dimension lumber (psi) and
/// specific gravity for fastener design.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LumberValues {
    /// `DFL`, `HF`, `SPF`, `SP`.
    pub species: String,
    /// `Select Structural`, `No.1`, `No.2`, `No.3`, `Stud`, …
    pub grade: String,
    pub fb: f64,
    pub ft: f64,
    pub fv: f64,
    pub fc_perp: f64,
    pub fc: f64,
    pub e: f64,
    pub e_min: f64,
    /// Specific gravity (NDS Table 12.3.3A).
    pub g: f64,
    pub verified: bool,
}

impl Verified for LumberValues {
    fn verified(&self) -> bool {
        self.verified
    }
}

table!(nds_lumber, LumberValues, "nds-lumber.json");

table!(nds_timbers, LumberValues, "nds-timbers.json");

/// Reference design values for a species/grade: dimension lumber (Table 4A)
/// or timbers (Table 4D, grades `B&S …` / `P&T …`).
pub fn lumber(species: &str, grade: &str) -> Option<&'static LumberValues> {
    lumber_with_source(species, grade).map(|(r, _)| r)
}

/// As [`lumber`], with the table it comes from.
pub fn lumber_with_source(species: &str, grade: &str) -> Option<(&'static LumberValues, &'static Source)> {
    for t in [nds_lumber(), nds_timbers()] {
        if let Some(r) = t.rows.iter().find(|r| r.species == species && r.grade == grade) {
            return Some((r, &t.source));
        }
    }
    None
}

/// Whether a grade is a timber grade (Table 4D).
pub fn is_timber_grade(grade: &str) -> bool {
    grade.starts_with("B&S") || grade.starts_with("P&T")
}

/// Size factors C_F by nominal width, for a thickness class.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SizeFactor {
    /// `standard` (Select Structural … No.3) or `stud`.
    pub grades: String,
    /// Nominal thicknesses this row applies to, e.g. `[2, 3]`.
    pub thickness: Vec<u32>,
    /// Nominal widths, e.g. `[2, 3, 4]`; `[14]` means 14" and wider.
    pub width: Vec<u32>,
    pub fb: f64,
    pub ft: f64,
    pub fc: f64,
    pub verified: bool,
}

impl Verified for SizeFactor {
    fn verified(&self) -> bool {
        self.verified
    }
}

table!(nds_size_factors, SizeFactor, "nds-size-factors.json");

/// C_F for a grade and nominal `thick` × `width` (widths beyond the table
/// use its last row; Stud 8" and wider uses the No.3 factors).
pub fn size_factor(grade: &str, thick: u32, width: u32) -> Option<&'static SizeFactor> {
    let class = if grade == "Stud" && width < 8 { "stud" } else { "standard" };
    let rows = &nds_size_factors().rows;
    let for_t: Vec<&SizeFactor> = rows.iter().filter(|r| r.grades == class && r.thickness.contains(&thick)).collect();
    for_t
        .iter()
        .find(|r| r.width.contains(&width))
        .or_else(|| for_t.iter().filter(|r| r.width.iter().all(|&w| w <= width)).max_by_key(|r| r.width.iter().max().copied().unwrap_or(0)))
        .copied()
}

/// Flat use factor C_fu (bending about the weak axis).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FlatUse {
    pub thickness: Vec<u32>,
    pub width: Vec<u32>,
    pub cfu: f64,
    pub verified: bool,
}

impl Verified for FlatUse {
    fn verified(&self) -> bool {
        self.verified
    }
}

table!(nds_flat_use, FlatUse, "nds-flat-use.json");

pub fn flat_use(thick: u32, width: u32) -> Option<&'static FlatUse> {
    let rows: Vec<&FlatUse> = nds_flat_use().rows.iter().filter(|r| r.thickness.contains(&thick)).collect();
    rows.iter()
        .find(|r| r.width.contains(&width))
        .or_else(|| rows.iter().filter(|r| r.width.iter().all(|&w| w <= width)).max_by_key(|r| r.width.iter().max().copied().unwrap_or(0)))
        .copied()
}

/// Load duration factor by load kind.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LoadDuration {
    /// `dead`, `live`, `snow`, `roof_live`, `wind`, `seismic`, `impact`.
    pub load: String,
    pub duration: String,
    pub cd: f64,
    pub verified: bool,
}

impl Verified for LoadDuration {
    fn verified(&self) -> bool {
        self.verified
    }
}

table!(nds_load_duration, LoadDuration, "nds-load-duration.json");

pub fn load_duration(load: &str) -> Option<&'static LoadDuration> {
    nds_load_duration().rows.iter().find(|r| r.load == load)
}

// ----- fasteners ------------------------------------------------------------------

/// A nail: dimensions and bending yield strength.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Nail {
    /// `common`, `box`, `sinker`.
    pub kind: String,
    /// Pennyweight, e.g. 16.
    pub penny: u32,
    /// Shank diameter (in).
    pub d: f64,
    /// Length (in).
    pub l: f64,
    /// Bending yield strength (psi).
    pub fyb: f64,
    pub verified: bool,
}

impl Verified for Nail {
    fn verified(&self) -> bool {
        self.verified
    }
}

table!(nails, Nail, "nails.json");

pub fn nail(kind: &str, penny: u32) -> Option<&'static Nail> {
    nails().rows.iter().find(|n| n.kind == kind && n.penny == penny)
}

/// Published lateral design values Z for single-shear nailed connections
/// (used to cross-check the yield-limit calculation).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NailZ {
    pub kind: String,
    pub penny: u32,
    /// Side member thickness (in).
    pub side: f64,
    /// Specific gravity of both members.
    pub g: f64,
    /// Reference lateral design value (lb).
    pub z: f64,
    pub verified: bool,
}

impl Verified for NailZ {
    fn verified(&self) -> bool {
        self.verified
    }
}

table!(nail_z, NailZ, "nail-z.json");

// ----- loads ---------------------------------------------------------------------------

/// Weight of a building material or assembly layer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MaterialWeight {
    /// Lookup key used in scripts, e.g. `asphalt shingles`.
    pub key: String,
    pub description: String,
    /// Weight (psf).
    pub psf: f64,
    pub verified: bool,
    /// Thickness for drawing (inches): nominal or estimated, see the source note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thickness_in: Option<f64>,
    /// Raised ribs (standing-seam panels), for drawing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ribs: Option<Ribs>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ribs {
    pub width_in: f64,
    pub spacing_in: f64,
    /// `slope` (down the roof) or `run` (along the ridge).
    pub along: String,
}

impl Verified for MaterialWeight {
    fn verified(&self) -> bool {
        self.verified
    }
}

table!(material_weights, MaterialWeight, "material-weights.json");

pub fn material_weight(key: &str) -> Option<&'static MaterialWeight> {
    material_weights().rows.iter().find(|r| r.key == key)
}

/// All tables, as JSON by name (for scripts and the UI).
pub fn table_json(name: &str) -> Option<String> {
    let v = match name {
        "nds-lumber" => serde_json::to_string(nds_lumber()),
        "nds-size-factors" => serde_json::to_string(nds_size_factors()),
        "nds-load-duration" => serde_json::to_string(nds_load_duration()),
        "nails" => serde_json::to_string(nails()),
        "nail-z" => serde_json::to_string(nail_z()),
        "material-weights" => serde_json::to_string(material_weights()),
        "nds-flat-use" => serde_json::to_string(nds_flat_use()),
        "nds-timbers" => serde_json::to_string(nds_timbers()),
        _ => return None,
    };
    v.ok()
}

pub const TABLES: [&str; 8] = ["nds-lumber", "nds-size-factors", "nds-load-duration", "nails", "nail-z", "material-weights", "nds-flat-use", "nds-timbers"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_parse_and_look_up() {
        for t in TABLES {
            assert!(table_json(t).is_some(), "{t}");
        }
        assert!(lumber("DFL", "No.2").is_some());
        assert_eq!(size_factor("No.2", 2, 10).unwrap().fb, 1.1);
        assert_eq!(size_factor("No.2", 2, 16).unwrap().width, vec![14]);
        assert_eq!(size_factor("No.2", 4, 8).unwrap().fb, 1.3);
        assert_eq!(size_factor("Stud", 2, 4).unwrap().fc, 1.05);
        assert_eq!(size_factor("Stud", 2, 8).unwrap().fb, 1.2);
        assert_eq!(load_duration("snow").unwrap().cd, 1.15);
        assert!(nail("common", 16).is_some());
        assert_eq!(flat_use(2, 6).unwrap().cfu, 1.15);
        assert_eq!(flat_use(2, 12).unwrap().cfu, 1.2);
        assert_eq!(lumber("DFL", "B&S No.2").unwrap().fb, 875.0);
        assert!(lumber_with_source("DFL", "B&S No.1").unwrap().1.table.contains("4D"));
    }
}
