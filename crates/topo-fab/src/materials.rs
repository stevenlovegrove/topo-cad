//! Metals and laminates as model materials, from the reference table
//! (`topo-data`, `materials.json`) or given in full by the script.

use topo_core::{Material, MaterialId, Strength};

const GPA: f64 = 1e9;
const MPA: f64 = 1e6;

/// The material with table key `key` (e.g. `6061-T6`), with its strength and
/// the source of its values.
pub fn engineering(key: &str) -> Option<Material> {
    let r = topo_data::material(key)?;
    Some(Material {
        id: MaterialId(0),
        name: r.name.clone(),
        family: r.family.clone(),
        e: r.e_gpa * GPA,
        g: r.g_gpa * GPA,
        density: r.density,
        design_key: Some(format!("MECH:{key}")),
        strength: Some(Strength { yield_: r.fy_mpa * MPA, ultimate: r.fu_mpa * MPA, source: r.source.clone(), verified: r.verified }),
    })
}

/// Keys of the table's materials, for error messages.
pub fn known() -> Vec<String> {
    topo_data::materials().rows.iter().map(|r| r.key.clone()).collect()
}

/// A material the script describes itself (a particular layup, a tube
/// maker's alloy): SI values and where they come from. It is never marked
/// verified.
#[allow(clippy::too_many_arguments)]
pub fn custom(name: &str, family: &str, e: f64, g: f64, density: f64, yield_: f64, ultimate: f64, source: &str) -> Material {
    Material {
        id: MaterialId(0),
        name: name.into(),
        family: family.into(),
        e,
        g,
        density,
        design_key: None,
        strength: Some(Strength { yield_, ultimate, source: source.into(), verified: false }),
    }
}
