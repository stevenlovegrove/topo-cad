//! Nails and prescriptive fastening presets.
//!
//! Presets follow the typical light-frame fastening schedule (IRC Table
//! R602.3(1) / IBC Table 2304.10.2). The fastener counts are the common
//! prescriptive values; the governing edition must be verified by the designer
//! of record, so each preset carries its reference.

use topo_core::units::inch;
use topo_core::{Connection, ConnectionId, FastenMethod, Fastener, FastenerGroup, Fixity, Hardware};

pub fn common_nail(penny: u32) -> Fastener {
    // (shank diameter, length) in inches for common wire nails (NDS Table L4).
    let (d, l) = match penny {
        6 => (0.113, 2.0),
        8 => (0.131, 2.5),
        10 => (0.148, 3.0),
        12 => (0.148, 3.25),
        16 => (0.162, 3.5),
        20 => (0.192, 4.0),
        _ => panic!("unsupported common nail {penny}d"),
    };
    Fastener::Nail { designation: format!("{penny}d common ({l}\" x {d}\")"), diameter: inch(d), length: inch(l) }
}

fn nailed(name: &str, penny: u32, count: u32, method: FastenMethod, spacing_in: Option<f64>, reference: &str) -> Connection {
    Connection {
        id: ConnectionId(0),
        name: name.into(),
        fasteners: vec![FastenerGroup { fastener: common_nail(penny), count, method, spacing: spacing_in.map(inch) }],
        hardware: vec![],
        fixity: Fixity::Pinned,
        reference: Some(reference.into()),
        notes: None,
    }
}

const IRC: &str = "IRC R602.3(1)";

pub fn stud_to_bottom_plate() -> Connection {
    nailed("Stud to bottom plate", 16, 2, FastenMethod::EndNail, None, IRC)
}
pub fn stud_to_top_plate() -> Connection {
    nailed("Top plate to stud", 16, 2, FastenMethod::EndNail, None, IRC)
}
pub fn header_to_king() -> Connection {
    nailed("Header to king stud", 16, 4, FastenMethod::EndNail, None, IRC)
}
pub fn jack_to_king() -> Connection {
    nailed("Jack stud to king stud", 16, 1, FastenMethod::FaceNail, Some(24.0), IRC)
}
pub fn sill_to_jack() -> Connection {
    nailed("Rough sill to jack stud", 16, 2, FastenMethod::EndNail, None, IRC)
}
pub fn cripple_to_plate() -> Connection {
    nailed("Cripple to plate/header/sill", 16, 2, FastenMethod::EndNail, None, IRC)
}
pub fn plate_corner() -> Connection {
    nailed("Plate to plate at corner/intersection", 16, 2, FastenMethod::FaceNail, None, IRC)
}
pub fn joist_to_plate() -> Connection {
    nailed("Joist to sill/top plate", 8, 3, FastenMethod::ToeNail, None, IRC)
}
pub fn rim_to_joist() -> Connection {
    nailed("Rim joist to joist end", 16, 3, FastenMethod::EndNail, None, IRC)
}
pub fn rim_to_plate() -> Connection {
    nailed("Rim joist to top plate", 8, 1, FastenMethod::ToeNail, Some(6.0), IRC)
}
pub fn double_top_plate() -> Connection {
    nailed("Double top plate (cap to lower plate)", 16, 1, FastenMethod::FaceNail, Some(24.0), IRC)
}
pub fn top_plate_to_header() -> Connection {
    nailed("Top plate to header (tight)", 16, 1, FastenMethod::FaceNail, Some(16.0), "Typical practice; verify")
}
pub fn truss_to_plate() -> Connection {
    nailed("Truss to top plate", 10, 3, FastenMethod::ToeNail, None, IRC)
}
/// Metal connector plate at a truss joint (designed by the truss manufacturer).
pub fn truss_plate() -> Connection {
    Connection {
        id: ConnectionId(0),
        name: "Truss joint".into(),
        fasteners: vec![],
        hardware: vec![Hardware { kind: "metal truss connector plate".into(), model: Some("by truss mfr.".into()), count: 2 }],
        fixity: Fixity::Pinned,
        reference: Some("ANSI/TPI 1".into()),
        notes: None,
    }
}
pub fn joist_hanger(model: &str) -> Connection {
    Connection {
        id: ConnectionId(0),
        name: "Joist to flush beam".into(),
        fasteners: vec![],
        hardware: vec![Hardware { kind: "face-mount joist hanger".into(), model: Some(model.into()), count: 1 }],
        fixity: Fixity::Pinned,
        reference: Some("Manufacturer's catalog".into()),
        notes: Some("Fill all nail holes per manufacturer".into()),
    }
}
