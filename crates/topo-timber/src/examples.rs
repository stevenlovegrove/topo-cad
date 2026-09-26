//! Example models used by the CLI, WASM demo and tests.

use crate::fastening as fx;
use crate::floor::Floor;
use crate::lumber::graded;
use crate::perimeter::{Perimeter, Side};
use crate::truss::{StandardTruss, TrussRoof, TrussShape};
use crate::wall::{lap_tee, Justify, Opening, Wall};
use topo_core::units::{ft, ft_in, inch, psf};
use topo_core::*;

/// A 24'×16' single-storey structure: four exterior 2x4 walls (door and
/// windows), an interior bearing wall at mid-length that tees into the
/// exterior walls, and a 2x10 floor/ceiling platform bearing on the plates.
pub fn garage_studio() -> Model {
    let mut m = Model::new("Garage Studio");
    m.info.number = "TC-0001".into();
    m.info.client = "Example".into();
    m.info.designer = "topo-cad".into();
    m.info.date = "2026-09-26".into();
    m.info.design_basis = vec![
        "Wood design: ANSI/AWC NDS-2018 (ASD)".into(),
        "Loads: ASCE 7-16. Floor D = 10 psf, L = 40 psf".into(),
        "Lumber: DF-L No.2 unless noted, 19% max MC".into(),
        "Fastening per IRC Table R602.3(1) unless noted".into(),
    ];
    m.info.notes = vec![
        "Structural calculations not yet performed (Milestone 2).".into(),
        "Corner backing studs, blocking and sheathing not shown.".into(),
    ];

    let dfl2 = m.add_material(graded("DFL", "No.2"));
    let h = ft_in(8.0, 1.125);
    let (lx, ly) = (ft(24.0), ft(16.0));
    let t = inch(3.5);
    let root = m.add_group("Structure", "building", Frame::WORLD, None);

    // Exterior walls from a counter-clockwise perimeter path. Walls A and C get
    // a cap-plate break where the interior wall E tees in (x = 12').
    let wall = Wall::template(h, dfl2);
    let ext = Perimeter::start("Exterior walls", Vec2::new(0.0, 0.0))
        .to(
            Vec2::new(lx, 0.0),
            Side::Wall(
                wall.clone()
                    .named("Wall A (South)")
                    .cap_break(lx / 2.0)
                    .opening(Opening::door("D1", ft(5.0), inch(38.0), inch(82.5)).header(2, 2, 8))
                    .opening(Opening::window("W1", ft(17.0), ft(5.0), ft(4.0), inch(82.5)).header(2, 2, 10)),
            ),
        )
        .to(Vec2::new(lx, ly), Side::Wall(wall.clone().named("Wall B (East)").opening(Opening::window("W2", ft(8.0), ft(3.0), ft(3.0), inch(82.5)))))
        .to(
            Vec2::new(0.0, ly),
            Side::Wall(
                wall.clone()
                    .named("Wall C (North)")
                    .cap_break(lx / 2.0)
                    .opening(Opening::window("W3", ft(6.0), ft(4.0), ft(4.0), inch(82.5)).header(2, 2, 10))
                    .opening(Opening::window("W4", ft(18.0), ft(4.0), ft(4.0), inch(82.5)).header(2, 2, 10)),
            ),
        )
        .close(Side::Wall(wall.clone().named("Wall D (West)")));
    let ext = ext.build(&mut m);
    m.group_mut(ext.group).parent = Some(root);
    let (south, north) = (ext.walls[0].clone().unwrap(), ext.walls[2].clone().unwrap());

    // Interior bearing wall on the centreline x = 12', teeing into A and C.
    let mid = Wall::new("Wall E (Interior)", v3(lx / 2.0, 0.0, 0.0), v3(lx / 2.0, ly, 0.0), h, dfl2)
        .parent(root)
        .justify(Justify::Center)
        .insets(t, t)
        .opening(Opening::door("D2", ft(8.0), inch(38.0), inch(82.5)))
        .build(&mut m);
    let c_corner = m.add_connection(fx::plate_corner());
    // Lower plates tee into the exterior plates at shared nodes; caps lap over them.
    let ext_plates: Vec<MemberId> = ext.all_walls().flat_map(|w| w.plates()).collect();
    for n in m.connect_members(&mid.plates(), &ext_plates) {
        let here: Vec<MemberId> =
            mid.plates().into_iter().filter(|p| m.member(*p).path.contains(&n) && m.member(*p).role != "cap_plate").collect();
        for p in here {
            let to = ext_plates.iter().copied().find(|e| m.member(*e).path.contains(&n));
            m.connect(n, p, to, c_corner);
        }
    }
    assert!(lap_tee(&mut m, &mid, &south, c_corner) && lap_tee(&mut m, &mid, &north, c_corner));

    let floor = Floor::new("Floor/ceiling framing", v3(0.0, 0.0, h), Vec3::X, Vec3::Y, lx, ly, dfl2)
        .joists(2, 10, inch(16.0))
        .build(&mut m);
    m.group_mut(floor.group).parent = Some(root);
    let mut caps = ext.cap_plates();
    caps.extend(&mid.cap_plates);
    let c_bear = m.add_connection(fx::joist_to_plate());
    let c_rim = m.add_connection(fx::rim_to_plate());
    bear_on(&mut m, &floor.all(), &caps, |role| if role == "rim_joist" || role == "band_joist" { c_rim } else { c_bear });

    let mut bottom = ext.bottom_plates();
    bottom.extend(&mid.bottom_plates);
    support_bottom_plates(&mut m, &bottom, "Bottom plate anchored to concrete");

    let dead = m.add_load_case("D", LoadKind::Dead);
    let live = m.add_load_case("L", LoadKind::Live);
    m.loads.push(Load::Area { case: dead, group: floor.group, pressure: psf(10.0), direction: -Vec3::Z });
    m.loads.push(Load::Area { case: live, group: floor.group, pressure: psf(40.0), direction: -Vec3::Z });
    m
}

/// Existing detached 2-car garage, 24'×22', modelled as built: a 16' garage
/// door header of (2) 2x10 set tight under the top plate, with **no jack
/// studs** — the header hangs off the king studs on end nails. Fink trusses at
/// 24" o.c. bear on the door wall, so roof load reaches the header through the
/// top plate.
pub fn garage_as_built() -> Model {
    let mut m = Model::new("Garage (existing)");
    m.info.number = "TC-0002".into();
    m.info.client = "Homeowner".into();
    m.info.designer = "topo-cad".into();
    m.info.date = "2026-09-26".into();
    m.info.design_basis = vec![
        "EXISTING CONDITIONS as observed; member sizes and grades assumed where not visible".into(),
        "Wood design: ANSI/AWC NDS-2018 (ASD)".into(),
        "Loads: ASCE 7-16. Roof TCDL 10 psf, BCDL 10 psf, snow 25 psf (assumed)".into(),
        "Lumber: DF-L No.2 assumed".into(),
    ];
    m.info.notes = vec![
        "Observed: top plate deflects approx. 1/8\" relative to the garage door header.".into(),
        "Garage door header has no jack studs; header is end-nailed to king studs.".into(),
    ];

    let dfl2 = m.add_material(graded("DFL", "No.2"));
    let h = ft_in(8.0, 1.125);
    let (lx, ly) = (ft(24.0), ft(22.0));
    let root = m.add_group("Structure", "building", Frame::WORLD, None);
    let wall = Wall::template(h, dfl2);
    let walls = Perimeter::start("Walls", Vec2::new(0.0, 0.0))
        .to(
            Vec2::new(lx, 0.0),
            Side::Wall(
                wall.clone()
                    .named("Wall A (Front)")
                    .opening(Opening::door("GD", lx / 2.0, ft(16.0), 0.0).header(2, 2, 10).header_flush().jacks(0)),
            ),
        )
        .to(Vec2::new(lx, ly), Side::Wall(wall.clone().named("Wall B (East)").opening(Opening::door("D1", ft(17.0), inch(38.0), inch(82.5)))))
        .to(Vec2::new(0.0, ly), Side::Wall(wall.clone().named("Wall C (Back)").opening(Opening::window("W1", ft(12.0), ft(3.0), ft(3.0), inch(82.5)))))
        .close(Side::Wall(wall.clone().named("Wall D (West)")))
        .build(&mut m);
    m.group_mut(walls.group).parent = Some(root);

    let fink = TrussShape::standard(StandardTruss::Fink, ly, 6.0 / 12.0, inch(12.0)).expect("fink");
    let roof = TrussRoof::new("Roof trusses", v3(0.0, 0.0, h), Vec3::Y, fink, lx, inch(24.0), dfl2)
    .build(&mut m);
    m.group_mut(roof.group).parent = Some(root);
    let c_truss = m.add_connection(fx::truss_to_plate());
    bear_on(&mut m, &roof.bottom_chords(), &walls.cap_plates(), |_| c_truss);
    support_bottom_plates(&mut m, &walls.bottom_plates(), "Bottom plate on slab");
    let dead = m.add_load_case("D", LoadKind::Dead);
    let snow = m.add_load_case("S", LoadKind::Snow);
    m.loads.push(Load::Area { case: dead, group: roof.group, pressure: psf(20.0), direction: -Vec3::Z });
    m.loads.push(Load::Area { case: snow, group: roof.group, pressure: psf(25.0), direction: -Vec3::Z });
    m
}

/// Irregular footprint from a perimeter path: an L-shape with a reflex corner,
/// a 45° chamfer, and one open side where it attaches to an existing house.
pub fn l_shaped_perimeter() -> Model {
    let mut m = Model::new("L-shaped perimeter");
    m.info.designer = "topo-cad".into();
    m.info.date = "2026-09-26".into();
    let dfl2 = m.add_material(graded("DFL", "No.2"));
    let wall = Wall::template(ft_in(8.0, 1.125), dfl2);
    let door = Opening::door("GD", ft(10.0), ft(16.0), 0.0).header(2, 2, 12).header_flush().jacks(2);
    let p = Perimeter::start("Garage", Vec2::new(0.0, 0.0))
        .to(Vec2::new(ft(20.0), 0.0), Side::Wall(wall.clone().named("A").opening(door)))
        .to(Vec2::new(ft(20.0), ft(10.0)), Side::Wall(wall.clone().named("B")))
        .to(Vec2::new(ft(16.0), ft(14.0)), Side::Wall(wall.clone().named("C (chamfer)")))
        .to(Vec2::new(ft(10.0), ft(14.0)), Side::Wall(wall.clone().named("D")))
        .to(Vec2::new(ft(10.0), ft(24.0)), Side::Wall(wall.clone().named("E")))
        .to(Vec2::new(0.0, ft(24.0)), Side::Open) // house wall
        .close(Side::Wall(wall.clone().named("F").opening(Opening::door("D1", ft(12.0), inch(38.0), inch(82.5)))));
    let parts = p.build(&mut m);
    support_bottom_plates(&mut m, &parts.bottom_plates(), "Bottom plate on slab");
    m
}

/// Connects `members` to the `plates` their axes cross or run along (bearing),
/// recording a connection chosen by member role.
fn bear_on(m: &mut Model, members: &[MemberId], plates: &[MemberId], conn: impl Fn(&str) -> ConnectionId) {
    for n in m.connect_members(members, plates) {
        for &j in members {
            if !m.member(j).path.contains(&n) {
                continue;
            }
            let plate = plates.iter().copied().find(|p| m.member(*p).path.contains(&n));
            let c = conn(&m.member(j).role);
            m.connect(n, j, plate, c);
        }
    }
}

/// Every node of the given bottom plates bears on the foundation.
fn support_bottom_plates(m: &mut Model, plates: &[MemberId], what: &str) {
    let mut nodes: Vec<NodeId> = plates.iter().flat_map(|&b| m.member(b).path.clone()).collect();
    nodes.sort();
    nodes.dedup();
    for n in nodes {
        m.supports.push(Support { node: n, restraint: Restraint::PINNED, description: Some(what.into()) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use topo_geom::{CutKind, Geometry};

    #[test]
    fn garage_is_valid_and_clash_free() {
        let m = garage_studio();
        let topo = Topology::build(&m);
        let issues = validate(&m, &topo);
        assert!(issues.iter().all(|i| i.severity < Severity::Warning), "{issues:#?}");
        let g = Geometry::build(&m, &topo);
        assert!(g.issues.is_empty(), "{:#?}", g.issues);
        let clashes = g.clashes();
        let describe: Vec<String> = clashes
            .iter()
            .map(|(a, b, d)| format!("{} {} / {} {} : {:.1} mm", a, m.member(*a).role, b, m.member(*b).role, d * 1000.0))
            .collect();
        assert!(clashes.is_empty(), "{describe:#?}");
        assert!(g.bearing_issues(&m).is_empty(), "{:#?}", g.bearing_issues(&m));
    }

    #[test]
    fn full_height_studs_are_precut_length() {
        let m = garage_studio();
        let topo = Topology::build(&m);
        let g = Geometry::build(&m, &topo);
        for mm in m.members.iter().filter(|x| x.role == "stud") {
            let l = g.member(mm.id).cut_length();
            assert!((l - inch(92.625)).abs() < 1e-6, "{} length {}", mm.id, l / inch(1.0));
        }
    }

    #[test]
    fn joists_bear_on_interior_wall_and_butt_rims() {
        let m = garage_studio();
        let topo = Topology::build(&m);
        let g = Geometry::build(&m, &topo);
        let joists: Vec<&Member> = m.members.iter().filter(|x| x.role == "joist").collect();
        assert!(joists.len() >= 10);
        let e_top = m
            .members
            .iter()
            .find(|x| x.role == "cap_plate" && m.group(x.group.unwrap()).name.starts_with("Wall E"))
            .unwrap();
        for j in joists {
            // Rim at each end, interior bearing wall at mid-span.
            assert!(matches!(g.member(j.id).start.kind, CutKind::Butt { .. }));
            assert!(matches!(g.member(j.id).end.kind, CutKind::Butt { .. }));
            // Joists share a node with the interior top plate: a Cross (bearing), or a
            // Tee where a wall stud lands directly beneath (in-line framing).
            let shared = j.path[1..j.path.len() - 1].iter().find(|n| e_top.path.contains(n));
            let kind = topo.junction(*shared.expect("joist not connected to interior wall")).kind;
            assert!(matches!(kind, JunctionKind::Cross | JunctionKind::Tee), "{kind:?}");
            assert!((g.member(j.id).cut_length() - (ft(24.0) - inch(3.0))).abs() < 1e-6);
        }
    }

    #[test]
    fn garage_as_built_is_valid_and_clash_free() {
        let m = garage_as_built();
        let topo = Topology::build(&m);
        let issues = validate(&m, &topo);
        assert!(issues.iter().all(|i| i.severity < Severity::Warning), "{issues:#?}");
        let g = Geometry::build(&m, &topo);
        assert!(g.issues.is_empty(), "{:#?}", g.issues);
        let describe: Vec<String> = g
            .clashes()
            .iter()
            .map(|(a, b, d)| format!("{} {} / {} {} : {:.1} mm", a, m.member(*a).role, b, m.member(*b).role, d * 1000.0))
            .collect();
        assert!(describe.is_empty(), "{describe:#?}");
        assert!(g.bearing_issues(&m).is_empty(), "{:#?}", g.bearing_issues(&m));
        // No jacks anywhere near the garage door; header tight to plate.
        let hdr = m.members.iter().find(|x| x.role == "header" && x.group.map(|gid| m.group(gid).name.contains("Front")) == Some(true)).unwrap();
        let hb = g.member(hdr.id).bbox();
        assert!((hb.max.z - (ft_in(8.0, 1.125) - inch(3.0))).abs() < 1e-6);
        // No jacks: the kings sit at the RO edges, so the header is exactly the RO width.
        assert!((g.member(hdr.id).cut_length() - ft(16.0)).abs() < 1e-6, "king to king");
        assert!(m.bonds.iter().any(|b| b.b == hdr.id && m.member(b.a).role == "top_plate"));
    }

    #[test]
    fn trusses_have_plumb_ridge_and_bear_on_plates() {
        let m = garage_as_built();
        let topo = Topology::build(&m);
        let g = Geometry::build(&m, &topo);
        let tops: Vec<&Member> = m.members.iter().filter(|x| x.role == "top_chord").collect();
        assert_eq!(tops.len(), 2 * 13);
        for tc in tops {
            let e = &g.member(tc.id).end;
            assert!(matches!(e.kind, CutKind::Miter { .. }));
            assert!(e.plane.normal.z.abs() < 1e-9, "ridge cut is plumb");
        }
        for bc in m.members.iter().filter(|x| x.role == "bottom_chord") {
            // Heels share nodes with the eave-wall top plates.
            let (s, e) = (bc.start(), bc.end());
            for n in [s, e] {
                assert!(topo.junction(n).members.iter().any(|x| m.member(x.member).role == "cap_plate"));
            }
            // Bottom chord ends are cut to the top chord's underside (the heel).
            assert!(matches!(g.member(bc.id).start.kind, CutKind::Butt { against } if m.member(against).role == "top_chord"));
        }
    }

    #[test]
    fn l_shaped_perimeter_is_sound() {
        let m = l_shaped_perimeter();
        let topo = Topology::build(&m);
        let issues = validate(&m, &topo);
        assert!(issues.iter().all(|i| i.severity < Severity::Warning), "{issues:#?}");
        let g = Geometry::build(&m, &topo);
        assert!(g.issues.is_empty() && g.clashes().is_empty() && g.bearing_issues(&m).is_empty());
    }

    #[test]
    fn header_length_is_ro_plus_three() {
        let m = garage_studio();
        let g = Geometry::build(&m, &Topology::build(&m));
        let hdr = m.members.iter().find(|x| x.role == "header").unwrap();
        // D1: 38" RO → 41" header.
        assert!((g.member(hdr.id).cut_length() - inch(41.0)).abs() < 1e-6);
    }
}
