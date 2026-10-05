use crate::ids::MaterialId;
use serde::{Deserialize, Serialize};

/// Generic mechanical material. Code-specific design values (e.g. NDS reference
/// Fb, Fv, …) are *not* stored here: a design code resolves `design_key` against
/// its own tables, so one model can be checked against several codes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub id: MaterialId,
    /// Display name, e.g. `DF-L No.2`.
    pub name: String,
    /// Broad family: `sawn_lumber`, `glulam`, `lvl`, `aluminum`, `steel`, …
    pub family: String,
    /// Modulus of elasticity used for stiffness (Pa).
    pub e: f64,
    /// Shear modulus (Pa).
    pub g: f64,
    /// Density for self-weight (kg/m³).
    pub density: f64,
    /// Key into a design code's tables, e.g. `NDS:DFL:No.2`.
    pub design_key: Option<String>,
    /// Strength, for materials checked by plain mechanics (stress against
    /// yield) rather than by a design code: metals, laminates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strength: Option<Strength>,
}

/// A material's strength and where the values come from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Strength {
    /// Yield strength (Pa): 0.2 % proof stress of a metal; for a laminate,
    /// the stress it is taken to carry without damage.
    pub yield_: f64,
    /// Ultimate tensile strength (Pa).
    pub ultimate: f64,
    pub source: String,
    /// Whether the values were checked against that source.
    pub verified: bool,
}

impl Material {
    /// A material with stiffness and density only (no design values).
    pub fn new(name: &str, family: &str, e: f64, g: f64, density: f64) -> Material {
        Material { id: MaterialId(0), name: name.into(), family: family.into(), e, g, density, design_key: None, strength: None }
    }
}
