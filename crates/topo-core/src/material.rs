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
}
