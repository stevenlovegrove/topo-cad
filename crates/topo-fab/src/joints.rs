//! Joining processes as connections: what holds two members together when
//! it is not discrete fasteners. Each carries moment (`Fixity::Rigid`)
//! unless made as a pivot; the process name lets a check apply what the
//! process does to the parent material (see `topo-data`, `joint-zones.json`).

use topo_core::{Connection, Fixity, Hardware, Rating};

fn made(name: &str, process: &str, fixity: Fixity, notes: Option<&str>) -> Connection {
    Connection { process: Some(process.into()), notes: notes.map(Into::into), ..Connection::new(name, fixity) }
}

/// Fusion-welded: `tig` or `mig`. Tube ends are coped (notched) to the tube
/// they meet.
pub fn welded(process: &str) -> Connection {
    let (name, key) = match process {
        "mig" => ("MIG welded", "mig_weld"),
        _ => ("TIG welded", "tig_weld"),
    };
    made(name, key, Fixity::Rigid, Some("Tube ends coped to fit; weld all round"))
}

/// Brazed: into lugs (sockets), or fillet-brazed tube to tube.
pub fn brazed(lugged: bool) -> Connection {
    if lugged {
        Connection { hardware: vec![Hardware { kind: "lug".into(), model: None, count: 1 }], ..made("Brazed into lugs", "braze", Fixity::Rigid, Some("Tube ends mitred to fit inside the lug")) }
    } else {
        made("Fillet brazed", "braze", Fixity::Rigid, Some("Tube ends coped to fit"))
    }
}

/// Adhesive-bonded (tubes into lugs or sleeves).
pub fn bonded() -> Connection {
    made("Bonded", "bond", Fixity::Rigid, Some("Structural adhesive; bond area and surface preparation by the maker"))
}

/// One moulded piece: the members are regions of a single laminate
/// (a monocoque), continuous through the joint.
pub fn moulded() -> Connection {
    made("Moulded in one piece", "moulded", Fixity::Rigid, Some("Continuous laminate through the joint"))
}

/// Clamped or bolted solid (a stem on a steerer, a seatpost in its tube):
/// carries moment.
pub fn clamped(name: &str) -> Connection {
    made(name, "clamp", Fixity::Rigid, None)
}

/// A pivot: carries force, no bending moment.
pub fn pinned(name: &str) -> Connection {
    made(name, "pivot", Fixity::Pinned, None)
}

/// `stiffness` (N·m/rad about both bending axes) on a connection, and the
/// capacities it is rated for.
pub fn with_properties(mut c: Connection, stiffness: Option<f64>, rating: Option<Rating>) -> Connection {
    if let Some(k) = stiffness {
        c.fixity = Fixity::Semi { k_u: k, k_v: k };
    }
    if rating.is_some() {
        c.rating = rating;
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn processes() {
        assert_eq!(welded("tig").process.as_deref(), Some("tig_weld"));
        assert!(brazed(true).describe().contains("lug"));
        assert_eq!(pinned("Pivot").fixity, Fixity::Pinned);
        assert!(moulded().engineered());
        let c = with_properties(clamped("Stem clamp"), Some(5e3), None);
        assert_eq!(c.fixity, Fixity::Semi { k_u: 5e3, k_v: 5e3 });
    }
}
