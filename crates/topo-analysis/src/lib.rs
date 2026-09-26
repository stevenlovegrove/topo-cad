//! Structural analysis layer (Milestone 2 grows here).
//!
//! Present in M1: analytical-model extraction, the gravity support graph,
//! load combinations and the `DesignCode` interface. Planned: tributary load
//! takedown, a sparse 3D frame solver and NDS member/connection checks.

pub mod code;
pub mod loadpath;
pub mod model;

pub use code::*;
pub use loadpath::{load_path_issues, support_graph, SupportEdge, Transfer};
pub use model::{AnalysisModel, Element};

#[cfg(test)]
mod tests {
    use super::*;
    use topo_core::{LoadKind, Topology};
    use topo_geom::Geometry;
    use topo_timber::examples::garage_studio;

    fn role(m: &topo_core::Model, id: topo_core::MemberId) -> &str {
        &m.member(id).role
    }

    #[test]
    fn extracts_elements_per_path_segment() {
        let m = garage_studio();
        let topo = Topology::build(&m);
        let g = Geometry::build(&m, &topo);
        let a = AnalysisModel::extract(&m, &topo, &g);
        let segs: usize = m.members.iter().map(|x| x.path.len() - 1).sum();
        assert_eq!(a.elements.len(), segs);
        // Studs are pinned at both ends (nailed connections).
        let stud = a.elements.iter().find(|e| role(&m, e.member) == "stud").unwrap();
        assert_eq!(stud.fix_i, topo_core::Fixity::Pinned);
        assert_eq!(stud.fix_j, topo_core::Fixity::Pinned);
        assert!(!a.couplings.is_empty());
    }

    #[test]
    fn load_path_follows_framing() {
        let m = garage_studio();
        let topo = Topology::build(&m);
        let g = Geometry::build(&m, &topo);
        let edges = support_graph(&m, &topo, &g);
        let has = |a: &str, b: &str, t: Transfer| {
            edges.iter().any(|e| role(&m, e.member) == a && role(&m, e.by) == b && e.transfer == t)
        };
        assert!(has("stud", "bottom_plate", Transfer::Bearing));
        assert!(has("top_plate", "stud", Transfer::Bearing));
        assert!(has("header", "jack_stud", Transfer::Bearing));
        assert!(has("header", "king_stud", Transfer::Fastened));
        assert!(has("joist", "cap_plate", Transfer::Bearing));
        assert!(has("cap_plate", "top_plate", Transfer::Bearing));
        assert!(has("joist", "rim_joist", Transfer::Fastened));
        assert!(has("rim_joist", "cap_plate", Transfer::Bearing));
        // Nothing should be "supported" by something resting on it.
        assert!(!has("bottom_plate", "stud", Transfer::Bearing));
        assert!(!has("stud", "top_plate", Transfer::Bearing));
    }

    #[test]
    fn properly_framed_building_has_no_load_path_issues() {
        let m = garage_studio();
        let topo = Topology::build(&m);
        let g = Geometry::build(&m, &topo);
        let issues = load_path_issues(&m, &topo, &g);
        assert!(issues.is_empty(), "{issues:#?}");
    }

    #[test]
    fn as_built_garage_header_is_flagged() {
        let m = topo_timber::examples::garage_as_built();
        let topo = Topology::build(&m);
        let g = Geometry::build(&m, &topo);
        let issues = load_path_issues(&m, &topo, &g);
        assert_eq!(issues.len(), 1, "{issues:#?}");
        assert_eq!(issues[0].code, "fastener-only-support");
        let hdr = issues[0].members[0];
        assert_eq!(role(&m, hdr), "header");
        assert!(issues[0].members[1..].iter().all(|&k| role(&m, k) == "king_stud"));
        // The roof really does load it: trusses → top plate → (bond) → header.
        let edges = support_graph(&m, &topo, &g);
        assert!(edges.iter().any(|e| role(&m, e.member) == "top_plate" && e.by == hdr && e.transfer == Transfer::Bearing));
        assert!(edges.iter().any(|e| role(&m, e.member) == "bottom_chord" && role(&m, e.by) == "cap_plate"));
    }

    #[test]
    fn asd_combinations_for_dead_and_live() {
        let m = garage_studio();
        let combos = asce7_asd(&m.load_cases);
        let names: Vec<&str> = combos.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["D", "D + L"]);
        assert!((nds_load_duration(&m, &combos[0]) - 0.9).abs() < 1e-12);
        assert!((nds_load_duration(&m, &combos[1]) - 1.0).abs() < 1e-12);
        assert!(m.load_cases.iter().any(|c| c.kind == LoadKind::Live));
    }
}
