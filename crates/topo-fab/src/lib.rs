//! Fabricated frames: things made of tube or extrusion that is cut and then
//! welded, brazed, bonded or bolted together — a bicycle frame, a machine
//! base, a kids' cart of T-slot profile.
//!
//! Like `topo-timber` for light framing, this supplies the domain's parts to
//! the domain-agnostic core: materials (from the reference tables, with
//! their sources), sections (round and box tube, T-slot profiles with their
//! makers' properties), and connections (joining processes, T-slot
//! connectors with their rated capacities).

pub mod joints;
pub mod materials;
pub mod sections;
pub mod tslot;

/// `28.6`, `0.8`, `40`: a length in mm without trailing zeros.
pub(crate) fn mm(metres: f64) -> String {
    let s = format!("{:.2}", metres * 1000.0);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}
