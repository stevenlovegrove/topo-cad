use super::*;
use topo_core::{Topology, UnitSystem};
use topo_geom::Geometry;

const GARAGE_TS: &str = include_str!("../../../examples/garage-as-built.ts");

#[test]
fn typescript_garage_matches_rust_example() {
    let ts = run(GARAGE_TS, "garage-as-built.ts").unwrap_or_else(|e| panic!("{e}"));
    let rs = topo_timber::examples::garage_as_built();
    assert_eq!(ts.units, UnitSystem::Imperial);
    assert_eq!(ts.members.len(), rs.members.len());
    assert_eq!(ts.nodes.len(), rs.nodes.len());
    // (Every truss's chord has its axis on the axes of the cap plates it sits on, across them or along
    // them: all the bearing points are nodes on both axes, none on a face.)
    assert!(ts.attachments.is_empty());
    assert_eq!(ts.bonds.len(), rs.bonds.len());
    assert_eq!(ts.loads.len(), rs.loads.len());
    let roles = |m: &topo_core::Model| {
        let mut v: Vec<String> = m.members.iter().map(|x| x.role.clone()).collect();
        v.sort();
        v
    };
    assert_eq!(roles(&ts), roles(&rs));

    let topo = Topology::build(&ts);
    let g = Geometry::build(&ts, &topo);
    assert!(g.clashes().is_empty() && g.bearing_issues(&ts).is_empty());
    let lp = topo_analysis::load_path_issues(&ts, &topo, &g);
    assert_eq!(lp.len(), 1, "{lp:#?}");
    assert_eq!(ts.member(lp[0].members[0]).role, "header");
}

#[test]
fn templates_are_immutable_values() {
    // Adding an opening to one derived wall must not leak into the template
    // or its other copies.
    let src = r#"
        import { Building, DFL, ft, Opening, Perimeter, Wall } from "topo-cad";
        const w = Wall.template({ height: ft(8), grade: DFL.No2 });
        const withDoor = w.opening(Opening.door({ label: "D", width: ft(3), height: ft(7) }).at(ft(5)));
        const p = Perimeter.start("P", [0, 0])
          .by([ft(12), 0], withDoor)
          .by([0, ft(10)], w)
          .by([-ft(12), 0], w)
          .close(w);
        export default Building.named("t").add(p);
    "#;
    let json = run_to_json(src, "t.ts").unwrap();
    let spec = SceneSpec::from_json(&json).unwrap();
    let spec::ItemSpec::Perimeter(p) = &spec.buildings[0].items[0] else { panic!() };
    let counts: Vec<usize> = p.segments.iter().map(|s| s.side.as_ref().unwrap().openings.len()).collect();
    assert_eq!(counts, vec![1, 0, 0, 0]);
    run(src, "t.ts").unwrap();
}

#[test]
fn reports_helpful_errors() {
    let unplaced = r#"
        import { Building, DFL, ft, Opening, Wall } from "topo-cad";
        Wall.template({ height: ft(8), grade: DFL.No2 }).opening(Opening.door({ label: "D9", width: ft(3), height: ft(7) }));
        export default Building.named("t");
    "#;
    let e = run(unplaced, "t.ts").unwrap_err().to_string();
    assert!(e.contains("D9") && e.contains(".at("), "{e}");

    let bad_grade = r#"
        import { Building, ft, Perimeter, Wall } from "topo-cad";
        const w = Wall.template({ height: ft(8), grade: { species: "OAK", grade: "Gold" } });
        export default Building.named("t").add(Perimeter.start("P", [0, 0]).by([ft(4), 0], w).by([0, ft(4)], w).close(w));
    "#;
    let e = run(bad_grade, "t.ts").unwrap_err().to_string();
    assert!(e.contains("OAK"), "{e}");

    let e = run("export default = ;", "bad.ts").unwrap_err();
    assert!(matches!(e, ScriptError::Syntax(_)), "{e}");

    let e = run("const x: number = 1;", "none.ts").unwrap_err().to_string();
    assert!(e.contains("default export"), "{e}");
}

#[test]
fn heading_based_paths() {
    // An irregular footprint with a 45° segment, drawn by headings and lengths.
    let src = r#"
        import { Building, DFL, ft, Perimeter, Wall } from "topo-cad";
        const w = Wall.template({ height: ft(8), grade: DFL.No2 });
        const p = Perimeter.start("P", [0, 0])
          .go(ft(20), 0, w)
          .go(ft(10), 90, w)
          .go(ft(4) * Math.SQRT2, 135, w)
          .go(ft(16), 180, w)
          .close(w);
        export default Building.named("t").add(p);
    "#;
    let m = run(src, "t.ts").unwrap();
    let topo = Topology::build(&m);
    let g = Geometry::build(&m, &topo);
    assert!(topo_core::validate(&m, &topo).is_empty());
    assert!(g.clashes().is_empty() && g.bearing_issues(&m).is_empty());
}

fn all_clean(m: &topo_core::Model) {
    let topo = Topology::build(m);
    let v = topo_core::validate(m, &topo);
    assert!(v.iter().all(|i| i.severity < topo_core::Severity::Warning), "{v:#?}");
    let g = Geometry::build(m, &topo);
    assert!(g.issues.is_empty(), "{:#?}", g.issues);
    assert!(g.clashes().is_empty(), "{:?}", g.clashes());
    assert!(g.bearing_issues(m).is_empty(), "{:#?}", g.bearing_issues(m));
    let lp = topo_analysis::load_path_issues(m, &topo, &g);
    assert!(lp.is_empty(), "{lp:#?}");
}

#[test]
fn standard_shapes_come_from_the_rust_generators() {
    // The TS constructors must return exactly the Rust shapes (one implementation).
    let src = r#"
        import { Building, ft, TrussShape } from "topo-cad";
        const o = { span: ft(24), pitch: 0.5, overhang: ft(1) };
        export const shapes = [
          TrussShape.fink(o), TrussShape.fan(o), TrussShape.kingPost(o),
          TrussShape.howe({ ...o, panels: 6 }), TrussShape.pratt({ ...o, panels: 6 }),
          TrussShape.scissors({ ...o, bottomPitch: 0.25 }), TrussShape.mono({ ...o, panels: 4 }),
        ];
        export default { name: "x", items: [], shapes };
    "#;
    use topo_timber::{StandardTruss as S, TrussShape};
    #[derive(serde::Deserialize)]
    struct Out {
        shapes: Vec<TrussShape>,
    }
    let out: Out = serde_json::from_str(&run_to_json(src, "s.ts").unwrap()).unwrap();
    let ft = topo_core::units::ft;
    let kinds = [S::Fink, S::Fan, S::KingPost, S::Howe { panels: 6 }, S::Pratt { panels: 6 }, S::Scissors { bottom_pitch: 0.25 }, S::Mono { panels: 4 }];
    assert_eq!(out.shapes.len(), kinds.len());
    for (ts, k) in out.shapes.iter().zip(kinds) {
        assert_eq!(*ts, TrussShape::standard(k, ft(24.0), 0.5, ft(1.0)).unwrap(), "{k:?}");
    }
}

#[test]
fn the_primitives_examples_show_what_they_say() {
    let sound = |m: &topo_core::Model| {
        let v = topo_core::validate(m, &Topology::build(m));
        assert!(v.iter().all(|i| i.severity < topo_core::Severity::Error) && m.issues.is_empty(), "{} {v:#?} {:#?}", m.info.name, m.issues);
    };
    // Beam on beam: two bearing points. The first joist's axis is on its underside, so its bearing
    // point is in its path and on the beam's top face; the second's is on its own underside face too.
    let m = run(include_str!("../../../examples/primitives/01-beam-on-beam.ts"), "01-beam-on-beam.ts").unwrap();
    sound(&m);
    let joists: Vec<_> = m.members.iter().filter(|x| x.role == "joist").collect();
    let beam = m.members.iter().find(|x| x.role == "beam").unwrap().id;
    let on = |node| m.attached_at(node).map(|a| a.member).collect::<Vec<_>>();
    let points: Vec<_> = m.attachments.iter().filter(|a| a.member == beam).map(|a| a.node).collect();
    assert_eq!(points.len(), 2);
    assert!(joists[0].path.contains(&points[0]) && on(points[0]) == [beam]);
    assert!(!joists[1].path.contains(&points[1]) && on(points[1]).contains(&joists[1].id) && on(points[1]).contains(&beam));
    // A joist that does not touch the beam is not joined to it: said, with how far off it is.
    let off = include_str!("../../../examples/primitives/01-beam-on-beam.ts").replace("[ft(2), -ft(3), top]", "[ft(2), -ft(3), top + inch(0.5)]");
    let e = run(&off, "off.ts").unwrap_err().to_string();
    assert!(e.contains("does not rest on") && e.contains("above"), "{e}");

    // Trusses on beams: three free things in the building's one solve; six bearing points on faces.
    let m = run(include_str!("../../../examples/primitives/02-trusses-on-beams.ts"), "02-trusses-on-beams.ts").unwrap();
    sound(&m);
    assert_eq!(m.attachments.len(), 6);
    assert_eq!(m.solves.len(), 1);
    let s = &m.solves[0];
    assert_eq!(s.variables.iter().filter(|v| v.kind == "free").count(), 3);
    assert!(s.variables.iter().all(|v| v.freedoms.iter().all(|f| f.settled != "open")), "nothing is left open: {:#?}", s.variables);
    // What it sits on fixes T1's height and its tilt, and its station across the ridge its place along the span
    // (hard); the hints settle its roll and its turn on plan, and the layout (a fallback, below them) its place along
    // the ridge.
    let t1 = s.variables.iter().find(|v| v.name == "Roof trusses/T1").unwrap();
    assert_eq!(t1.freedoms.iter().map(|f| f.settled.as_str()).collect::<Vec<_>>(), ["hard", "fallback", "hard", "hint", "hard", "hint"], "{t1:#?}");
    let station = s.constraints.iter().find(|c| c.name == "Roof trusses/T1 at its station along the ridge").unwrap();
    assert!(station.tier == "fallback" && station.gap.abs() < 1e-9);
    // The level hint (it is turned as it was put) gives way to the beams (2 inches over the span between them).
    let level = s.constraints.iter().find(|c| c.name == "Roof trusses/T1 turned as it was put").unwrap();
    assert!(level.gap > 0.009 && level.gap < 0.012, "{level:?}");
    // Each contact is a node of the solve on a line of the chord's underside (its axis: one number, bounded by the
    // chord, settled by the contact) and on a line of the beam's top (the far edge: sloping down to the lower beam,
    // it touches each there), held to be one point.
    let contacts = ["Roof trusses/T1 on the far edge of Beams/beam#1", "Roof trusses/T1 on the far edge of Beams/beam#2"];
    for c in contacts {
        let halves: Vec<_> = s.nodes.iter().filter(|n| n.name.starts_with(&format!("{c} ("))).collect();
        assert_eq!(halves.len(), 2, "{c}: {:#?}", s.nodes.iter().map(|n| &n.name).collect::<Vec<_>>());
        assert!(halves.iter().all(|n| n.kind == "on a line" && n.numbers.len() == 1 && n.numbers[0].bounds.is_some()), "{halves:#?}");
        assert_eq!(halves[0].of, "Roof trusses/T1");
        assert_eq!(halves[0].numbers[0].settled, "hard");
        let on = s.constraints.iter().find(|x| x.name == format!("{c}: on both")).unwrap();
        assert!(on.tier == "hard" && on.gap.abs() < 1e-9 && on.nodes.len() == 2);
    }
    // The model's bearing points are those nodes, by name: in the chord's path, on the beam's top face.
    let named: Vec<&str> = m.attachments.iter().filter_map(|a| m.nodes[a.node.idx()].name.as_deref()).filter(|n| n.starts_with("Roof trusses/T1 ")).collect();
    assert_eq!(named, contacts);

    // Placed by nodes: the free beam, put anywhere and turned, ends up on the posts' tops, sharing their nodes.
    let m = run(include_str!("../../../examples/primitives/04-placed-by-nodes.ts"), "04-placed-by-nodes.ts").unwrap();
    sound(&m);
    all_clean(&m);
    let ft = topo_core::units::ft;
    let beam = m.members.iter().find(|x| x.role == "beam").unwrap();
    let posts: Vec<_> = m.members.iter().filter(|x| x.role == "post").collect();
    assert_eq!((beam.start(), beam.end()), (posts[0].end(), posts[1].end()), "the beam's ends are the posts' top nodes");
    assert!(m.pos(beam.start()).distance(topo_core::v3(0.0, 0.0, ft(7.0))) < 1e-9 && m.pos(beam.end()).distance(topo_core::v3(ft(10.0), 0.0, ft(7.0))) < 1e-9);
    // (One node, made of two: it has the name of the one declared first.)
    assert_eq!(m.nodes[beam.start().idx()].name.as_deref(), Some("Posts/a top"));
    // Upright: its depth is up, as it was drawn.
    assert!(m.member_axes(beam.id).v.dot(topo_core::Vec3::Z) > 1.0 - 1e-9);
    let s = &m.solves[0];
    let b = s.variables.iter().find(|v| v.name == "Beam").unwrap();
    assert_eq!(b.freedoms.iter().map(|f| f.settled.as_str()).collect::<Vec<_>>(), ["hard", "hard", "hard", "hint", "hard", "hard"]);
    // A beam that does not fit between the posts cannot be placed by both: refused, and said.
    let long = include_str!("../../../examples/primitives/04-placed-by-nodes.ts").replace(r#".point("east end", [ft(10), 0, 0])"#, r#".point("east end", [ft(11), 0, 0])"#);
    let e = run(&long, "long.ts").unwrap_err().to_string();
    assert!(e.contains("Beam/east end is Posts/b top"), "{e}");

    // Joists on walls: where each crosses a cap plate it rests on it, at a node in the middle of where they touch:
    // on the joist's axis (in its path), and on the plate's top face (whose axis is along one edge of it, on the
    // wall's line), fastened there.
    let src = include_str!("../../../examples/primitives/05-joists-on-walls.ts");
    let m = run(src, "05-joists-on-walls.ts").unwrap();
    sound(&m);
    all_clean(&m);
    let caps: Vec<_> = m.members.iter().filter(|x| x.role == "cap_plate").map(|x| x.id).collect();
    for j in m.members.iter().filter(|x| x.role == "joist") {
        let on: Vec<_> = m.attachments.iter().filter(|a| caps.contains(&a.member) && j.path.contains(&a.node)).collect();
        assert_eq!(on.len(), 2, "{}", m.member_path(j.id));
        assert!(on.iter().all(|a| a.across.abs() < 1e-9 && m.joint(a.node).is_some_and(|jt| jt.connections.iter().any(|u| u.member == j.id && u.to == Some(a.member)))));
    }
    // A joist half an inch high does not touch the plates it crosses: said, with how far.
    let high = src.replace("[x, -inch(6), ft(8)]", "[x, -inch(6), ft(8) + inch(0.5)]").replace("[x, ft(10) + inch(6), ft(8)]", "[x, ft(10) + inch(6), ft(8) + inch(0.5)]");
    let e = run(&high, "high.ts").unwrap_err().to_string();
    assert!(e.contains("does not rest on") && e.contains("12.7 mm above"), "{e}");
    // A free post, its top made one node with the start of the front wall's cap plate (a node the perimeter made).
    let post = src.replace(
        "export default Building.named(\"Joists on walls\").add(walls, joists.bearingOn(walls));",
        "import { coincide } from \"topo-cad\";\nconst post = Assembly.named(\"Post\").point(\"foot\", [0, 0, 0]).point(\"top\", [0, 0, ft(8)]).member(\"post\", [\"foot\", \"top\"], { size: [4, 4], grade }).moved([2, 3, 0]).free();\nexport default Building.named(\"Joists on walls\").add(walls, post, coincide(post.node(\"top\"), walls.node(\"Front/cap_plate\", \"start\")));",
    );
    let m = run(&post, "post.ts").unwrap();
    let cap = m.find_member("Walls/Front/cap_plate").unwrap();
    let p = m.members.iter().find(|x| x.role == "post").unwrap();
    assert_eq!(p.end(), m.member(cap).start(), "the post's top is the plate's start node");
    assert!(m.pos(p.start()).distance(m.pos(p.end()) - topo_core::Vec3::Z * ft(8.0)) < 1e-9, "upright under it");

    // Placed by a corner: the free walls go where the datum says, all of them; the roof is set out from the
    // same corner and sits on the plates where they now are; the post's top is a node of the plate, 1.5 m along.
    let src = include_str!("../../../examples/primitives/06-placed-by-a-corner.ts");
    let m = run(src, "06-placed-by-a-corner.ts").unwrap();
    sound(&m);
    let v3 = topo_core::v3;
    let front = m.find_member("Walls/Front/cap_plate").unwrap();
    let corner = m.pos(m.member(front).start());
    assert!(corner.distance(v3(5.0, 3.0, ft(8.0))) < 1e-9, "{corner:?}");
    // Not turned: the front wall still runs along x.
    assert!((m.member_axes(front).x - topo_core::Vec3::X).norm() < 1e-9);
    // Its bottom plates moved too (they bear on the foundation where they now are).
    let bottom = m.find_member("Walls/Front/bottom_plate").unwrap();
    assert!((m.pos(m.member(bottom).start()).z).abs() < 1e-9 && m.pos(m.member(bottom).start()).y > 2.9);
    // The roof: the first truss's heel is at the corner on plan, and every truss sits on the plates.
    let t1 = m.find_member("T1/bottom_chord.H0-H1").unwrap_or_else(|_| m.members.iter().find(|x| x.role == "bottom_chord").unwrap().id);
    let heel = m.pos(m.member(t1).start());
    assert!((heel.x - corner.x).abs() < 1e-6 && (heel.y - corner.y).abs() < 1.0, "{heel:?} {corner:?}");
    assert!(m.issues.iter().all(|i| i.code != "truss-not-seated"));
    // The post's top is a node of the front cap plate, 1.5 m along it.
    let p = m.members.iter().find(|x| x.role == "post").unwrap();
    assert!(m.member(front).path.contains(&p.end()));
    assert!(m.pos(p.end()).distance(corner + topo_core::Vec3::X * 1.5) < 1e-9);
    // Moving the datum moves it all.
    let m2 = run(&src.replace("Frame.world.point(\"mark\", [5, 3, ft(8)])", "Frame.world.point(\"mark\", [7, 3, ft(8)])"), "moved.ts").unwrap();
    let front2 = m2.find_member("Walls/Front/cap_plate").unwrap();
    assert!((m2.pos(m2.member(front2).start()) - corner - v3(2.0, 0.0, 0.0)).norm() < 1e-9, "{:?} {:?}", m2.pos(m2.member(front2).start()), corner);

    // Walls on readings: the corners go where the readings put them, and the walls are built on them.
    let src = include_str!("../../../examples/primitives/07-walls-on-readings.ts");
    let m = run(src, "07-walls-on-readings.ts").unwrap();
    sound(&m);
    let inch = topo_core::units::inch;
    // (In the one solve, with everything else: each corner a node of three numbers of the walls.)
    assert_eq!(m.solves.len(), 1);
    let at = |k: usize| m.solves[0].nodes.iter().find(|n| n.name == format!("Walls/corner {k}")).unwrap().at;
    assert!(at(1).norm() < 1e-9 && (at(2).x - (ft(20.0) + inch(1.0))).abs() < 1e-9 && (at(3).y - (ft(12.0) + inch(0.75))).abs() < 1e-9, "{:?} {:?} {:?}", at(1), at(2), at(3));
    // Corner 3 kept its x (as drawn, a hint), and corner 4 is where it was drawn.
    assert!((at(3).x - ft(20.0)).abs() < 1e-9 && (at(4) - topo_core::v3(0.0, ft(12.0), 0.0)).norm() < 1e-9);
    // The front wall is built on them: its bottom plate runs the taped length.
    let plate = m.find_member("Walls/Front/bottom_plate").unwrap();
    assert!((m.member_length(plate) - (ft(20.0) + inch(1.0))).abs() < 0.2, "{}", m.member_length(plate));
    let front_cap = m.find_member("Walls/Front/cap_plate").unwrap();
    assert!(m.pos(m.member(front_cap).end()).x > ft(20.0) + inch(0.9));
    // Readings that cannot hold with a datum give way, and say by how much: here the tape and corner 1's survey
    // (both readings, to the same tolerance) share the inch between them.
    let fought = src
        .replace("survey(walls.corner(1), { x: 0, y: 0 }),", "survey(walls.corner(1), { x: 0, y: 0 }),\n  mark,\n  coincide(walls.corner(2), mark.node(\"corner 2\")),")
        .replace("export default", "const mark = Frame.world.point(\"corner 2\", [ft(20), 0, 0]);\nexport default")
        .replace("import { Building,", "import { Building, coincide, Frame,");
    let m = run(&fought, "fought.ts").unwrap();
    let c = &m.solves[0];
    let tape_gap = c.constraints.iter().find(|x| x.name.starts_with("Walls/corner 1 to Walls/corner 2")).unwrap();
    let survey_gap = c.constraints.iter().find(|x| x.name == "Walls/corner 1 surveyed: x = 0.0000").unwrap();
    assert!(tape_gap.tier == "measured" && (tape_gap.gap + survey_gap.gap - inch(1.0)).abs() < 1e-6 && (tape_gap.gap - survey_gap.gap).abs() < 1e-6, "{tape_gap:?} {survey_gap:?}");

    // Studs on readings: the walls are built with their studs where the readings put them.
    let src = include_str!("../../../examples/primitives/08-studs-on-readings.ts");
    let m = run(src, "08-studs-on-readings.ts").unwrap();
    sound(&m);
    all_clean(&m);
    let inch = topo_core::units::inch;
    let stud_x = |m: &topo_core::Model, k: usize| m.pos(m.member(m.find_member(&format!("Walls/Front/stud#{k}")).unwrap()).start()).x;
    let (plain, _) = (run(&src.replace("  tape(walls.stud(\"Front\", 2, \"bottom\"), walls.stud(\"Front\", 3, \"bottom\"), inch(15)),\n  survey(walls.stud(\"Front\", 4, \"bottom\"), { x: ft(5) }),\n", ""), "plain.ts").unwrap(), ());
    // The tape: the 2nd and 3rd each give way half an inch toward each other.
    assert!((stud_x(&m, 2) - (stud_x(&plain, 2) + inch(0.5))).abs() < 1e-9 && (stud_x(&m, 3) - (stud_x(&plain, 3) - inch(0.5))).abs() < 1e-9, "{} {}", stud_x(&m, 2), stud_x(&m, 3));
    assert!((stud_x(&m, 3) - stud_x(&m, 2) - inch(15.0)).abs() < 1e-9);
    // The survey: the 4th is at 5 ft; the others are where the layout puts them.
    assert!((stud_x(&m, 4) - ft(5.0)).abs() < 1e-9);
    assert!((stud_x(&m, 5) - stud_x(&plain, 5)).abs() < 1e-9 && (stud_x(&m, 1) - stud_x(&plain, 1)).abs() < 1e-9);
    // (In the one solve with everything else: made of the stud's number and of the numbers of the wall's corners it
    // moves with. The reading fixes the stud and corner 1 together; corner 1's hint (where it is drawn) settles
    // which: so both are settled by a hint, and the stud is 5 ft along.)
    let n = m.solves.iter().flat_map(|s| &s.nodes).find(|n| n.name == "Walls/Front/stud#4:bottom").unwrap();
    let number = |name: &str| n.numbers.iter().find(|x| x.name == name).unwrap();
    assert!((number("Walls/Front/stud#4").value - ft(5.0)).abs() < 1e-9 && number("Walls/corner 1: x").value.abs() < 1e-9, "{n:#?}");
    assert_eq!((number("Walls/Front/stud#4").settled.as_str(), number("Walls/corner 1: x").settled.as_str()), ("hint", "hint"));
    let survey = m.solves[0].constraints.iter().find(|c| c.name.starts_with("Walls/Front/stud#4:bottom surveyed")).unwrap();
    assert!(survey.gap.abs() < 1e-9 && survey.tier == "measured");
    // A stud's end is a node to other things too, as built: a free post's top made one with the top of the 2nd.
    let post = src.replace("import { Building,", "import { Assembly, Building, coincide,").replace(
        "  survey(walls.stud(\"Front\", 4, \"bottom\"), { x: ft(5) }),\n",
        "  survey(walls.stud(\"Front\", 4, \"bottom\"), { x: ft(5) }),\n  Assembly.named(\"Post\").point(\"foot\", [1, 1, 0]).point(\"top\", [1, 1, ft(8) - inch(1.5)]).member(\"post\", [\"foot\", \"top\"], { size: [4, 4], grade: DFL.No2 }).free(),\n",
    ).replace(
        "export default Building.named(\"Studs on readings\").add(",
        "const p = Assembly.named(\"Post\").point(\"foot\", [1, 1, 0]).point(\"top\", [1, 1, ft(8) - inch(1.5)]);\nexport default Building.named(\"Studs on readings\").add(\n  coincide(p.node(\"top\"), walls.stud(\"Front\", 2, \"top\")),",
    );
    let m2 = run(&post, "post.ts").unwrap();
    let p = m2.members.iter().find(|x| x.role == "post").unwrap();
    assert_eq!(p.end(), m2.member(m2.find_member("Walls/Front/stud#2").unwrap()).end(), "the post's top is the stud's top node");
    // In the one solve, something else can move a stud: a fixed post 2 inches along from where the layout puts the
    // 2nd stud, its top made one node with that stud's top. The stud (a hint) goes to the post; the wall's plates
    // keep their nodes in order, and the model is sound.
    let x2 = stud_x(&plain, 2);
    let pulled = src.replace("import { Building,", "import { Assembly, Building, coincide,").replace(
        "export default Building.named(\"Studs on readings\").add(",
        &format!("const p = Assembly.named(\"Post\").point(\"foot\", [{x}, -0.3, 0]).point(\"top\", [{x}, -0.3, ft(8) - inch(1.5)]).member(\"post\", [\"foot\", \"top\"], {{ size: [4, 4], grade: DFL.No2 }});\nexport default Building.named(\"Studs on readings\").add(\n  p,", x = x2 + inch(2.0)),
    ).replace("  tape(walls.stud", "  coincide(p.node(\"top\"), walls.stud(\"Front\", 2, \"top\")),\n  tape(walls.stud");
    // (The post stands 0.3 m off the wall's line. Where the wall's corners are drawn is a hint: the front wall goes
    // to the post, its stud's top is the post's, and the wall is built there.)
    let off = run(&pulled, "pulled.ts").unwrap();
    sound(&off);
    let p = off.members.iter().find(|x| x.role == "post").unwrap();
    let stud = off.member(off.find_member("Walls/Front/stud#2").unwrap());
    assert_eq!(p.end(), stud.end(), "the post's top is the stud's top node");
    assert!((off.pos(stud.start()).y - off.pos(p.start()).y).abs() < 1e-6, "{:?} {:?}", off.pos(stud.start()), off.pos(p.start()));
    let pulled = pulled.replace("-0.3, 0]", "0, 0]").replace("-0.3, ft(8)", "0, ft(8)");
    let m3 = run(&pulled, "pulled.ts").unwrap();
    sound(&m3);
    assert!((stud_x(&m3, 2) - (x2 + inch(2.0))).abs() < 1e-9, "{} {}", stud_x(&m3, 2), x2 + inch(2.0));
    // (Its neighbour, taped at 15" from it, came along.)
    assert!((stud_x(&m3, 3) - stud_x(&m3, 2) - inch(15.0)).abs() < 1e-9);
    // A survey that would put the 4th past the 5th: the 5th (a hint) is pushed along to stay clear of it.
    let far = run(&src.replace("{ x: ft(5) }", "{ x: inch(70) }"), "far.ts").unwrap();
    assert!((stud_x(&far, 4) - inch(70.0)).abs() < 1e-9 && (stud_x(&far, 5) - inch(71.5)).abs() < 1e-9, "{} {}", stud_x(&far, 4), stud_x(&far, 5));
    sound(&far);

    // Trusses on walls: every bearing point is on both axes; none is on a face.
    let m = run(include_str!("../../../examples/primitives/03-trusses-on-walls.ts"), "03-trusses-on-walls.ts").unwrap();
    sound(&m);
    // A face read where something else has it (a scan, say): a point on the west face of T1's bottom chord, read to be
    // at a point of the world given along x only (10 mm east of where T1 is drawn). The reading moves the truss (its
    // station along the ridge is a hint); in the model, that point is a node on the chord's west face.
    let read = include_str!("../../../examples/primitives/03-trusses-on-walls.ts")
        .replace("import { Building,", "import { Building, coincide, Frame, member,")
        .replace("export default Building.named(\"Trusses on walls\").add(walls, roof);", "const seen = Frame.world.point(\"T1 west face\", { x: 0.01 });\nexport default Building.named(\"Trusses on walls\").add(walls, roof, seen, coincide(seen.node(\"T1 west face\"), member(\"T1/bottom_chord.H0-H1\").node(\"west\"), { tolerance: 0.001 }));");
    let mr = run(&read, "read.ts").unwrap();
    let chord = mr.find_member("T1/bottom_chord.H0-H1").unwrap();
    let face = mr.face(chord, mr.face_toward(chord, topo_core::Vec3::new(-1.0, 0.0, 0.0)));
    assert!((face.origin.x - 0.01).abs() < 1e-6, "{:?}", face.origin);
    let on: Vec<_> = mr.attachments.iter().filter(|a| a.member == chord).collect();
    assert_eq!(on.len(), 1, "{:#?}", mr.attachments);
    assert_eq!(mr.nodes[on[0].node.idx()].name.as_deref(), Some("T1/bottom_chord.H0-H1:face -1,0,0"));
    // (One solve: the walls' numbers and the trusses, each a free body sat on the plates, in the building's.)
    assert!(m.attachments.is_empty(), "{:#?}", m.attachments);
    assert_eq!(m.solves.len(), 1);
    assert!(m.solves[0].variables.iter().filter(|v| v.kind == "free").count() >= 2);

    // Walls that slope: built from numbers of the one solve, and built again at the solved ones until they settle.
    let src = include_str!("../../../examples/primitives/09-walls-that-slope.ts");
    let m = run(src, "09-walls-that-slope.ts").unwrap();
    let v = topo_core::validate(&m, &Topology::build(&m));
    assert!(v.iter().all(|i| i.severity < topo_core::Severity::Error), "{v:#?}");
    // (Said: the corners where a wall slopes are butted, not lapped; nothing else.)
    assert!(m.issues.iter().all(|i| i.code == "corner-sloped"), "{:#?}", m.issues);
    let end = |path: &str| m.pos(m.member(m.find_member(path).unwrap()).end());
    let start = |path: &str| m.pos(m.member(m.find_member(path).unwrap()).start());
    // The survey: the right wall's cap plate ends half an inch higher than drawn; the back wall's start with it.
    assert!((end("Lean-to/Right/cap_plate").z - (ft(10.0) + inch(0.5))).abs() < 1e-6, "{:?}", end("Lean-to/Right/cap_plate"));
    assert!((start("Lean-to/Back/cap_plate").z - (ft(10.0) + inch(0.5))).abs() < 1e-6);
    // The back wall's far end stays as drawn, and the front wall is level at 8 ft above its bottom.
    assert!((end("Lean-to/Back/cap_plate").z - ft(10.0)).abs() < 1e-6, "{:?}", end("Lean-to/Back/cap_plate"));
    // The base: the front's bottom plate slopes down 4" to its east end.
    assert!((end("Lean-to/Front/bottom_plate").z + inch(4.0)).abs() < 1e-9 && start("Lean-to/Front/bottom_plate").z.abs() < 1e-9);
    // And no clashes. KNOWN LIMITATION: the front wall's end stud, under its sloping plate, reaches a millimetre past
    // the plate's square-cut end (reported as partial bearing). (Plates butting at the sloped corners meet side faces,
    // and carry nothing through their ends: not bearing.)
    let g = topo_geom::Geometry::build(&m, &Topology::build(&m));
    assert!(g.clashes().is_empty(), "{:?}", g.clashes());
    assert!(g.bearing_overhangs().iter().all(|&(a, b, d)| m.member(a).role == "stud" && m.member(b).role.ends_with("_plate") && d < 0.002), "{:?}", g.bearing_overhangs());
}

#[test]
fn a_floor_sits_on_the_walls_under_it() {
    let (ft, inch) = (topo_core::units::ft, topo_core::units::inch);
    let src = include_str!("../../../examples/primitives/11-a-floor-on-walls.ts");
    let m = run(src, "11-a-floor-on-walls.ts").unwrap();
    all_clean(&m);
    let group = |name: &str| m.groups.iter().find(|g| g.name == name).unwrap();
    let of = |role: &str| -> Vec<&topo_core::Member> { group("Floor").members.iter().map(|&id| m.member(id)).filter(|x| x.role == role).collect() };
    // Two rims, a band joist at each side, and commons at 16" between them (12 ft: 8 of them).
    assert_eq!((of("rim_joist").len(), of("band_joist").len(), of("joist").len()), (2, 2, 8));
    // It was drawn at zero and is free: it sits on the walls, the underside of its joists at their top.
    for j in of("joist").iter().chain(&of("band_joist")).chain(&of("rim_joist")) {
        for n in &j.path {
            assert!((m.pos(*n).z - ft(8.0)).abs() < 1e-6, "{} {:?}", j.role, m.pos(*n));
        }
    }
    // Where it was drawn on plan, it stays: the joists run front to back, the first common 16" from the right wall.
    let first = of("joist")[0];
    let (a, b) = (m.pos(first.start()), m.pos(first.end()));
    // (Between the middles of the rims, each 3/4" in from the walls' lines.)
    assert!((a.x - (ft(12.0) - inch(16.0))).abs() < 1e-6 && (a.y - inch(0.75)).abs() < 1e-6 && (b.y - (ft(10.0) - inch(0.75))).abs() < 1e-6 && (a.x - b.x).abs() < 1e-6, "{a:?} {b:?}");
    // Each common joist bears on the front and back walls' cap plates: a bearing point on both.
    let bearing = m.solves.iter().flat_map(|s| &s.constraints).filter(|c| c.name.starts_with("Floor/joist#") && c.name.ends_with(": on both")).count();
    assert_eq!(bearing, 16, "{:#?}", m.solves.iter().flat_map(|s| &s.constraints).map(|c| &c.name).filter(|n| n.starts_with("Floor/")).collect::<Vec<_>>());
    // Its loads are on its joists: 40 psf live at 16" on each common, half that on each band joist.
    let live = m.load_cases.iter().find(|c| c.kind == topo_core::LoadKind::Live).unwrap().id;
    let w = |id: topo_core::MemberId| m.loads.iter().find_map(|l| match l { topo_core::Load::MemberUniform { case, member, w, .. } if *case == live && *member == id => Some(-w.z), _ => None }).unwrap();
    let psf40 = 40.0 * 47.880259;
    assert!((w(first.id) - psf40 * inch(16.0)).abs() < 1e-6 && (w(of("band_joist")[0].id) - psf40 * inch(8.0)).abs() < 1e-6);
    // The back wall surveyed half an inch taller (both its ends): the side walls' tops slope up to it, and the
    // floor sits on them as they are, tilted.
    let back = |end: &str| format!("survey(walls.node(\"Back/cap_plate\", \"{end}\"), {{ z: ft(8) + inch(0.5) }})");
    let taller = src.replace("Building, DFL,", "Building, DFL, survey,").replace(".add(walls, floor)", &format!(".add(walls, floor, {}, {})", back("start"), back("end")));
    let t = run(&taller, "taller.ts").unwrap();
    let group = t.groups.iter().find(|g| g.name == "Floor").unwrap();
    let first = group.members.iter().map(|&id| t.member(id)).find(|x| x.role == "joist").unwrap();
    let (a, b) = (t.pos(first.start()), t.pos(first.end()));
    assert!(a.z - ft(8.0) < inch(0.05) && b.z > ft(8.0) + inch(0.45), "{a:?} {b:?}");
    // It is one rigid thing: on walls whose tops are not in one plane (the back wall taller at one end only) it
    // cannot sit, and that is said, with how far each joist is off, not built wrong.
    let warped = src.replace("Building, DFL,", "Building, DFL, survey,").replace(".add(walls, floor)", &format!(".add(walls, floor, {})", back("start")));
    let e = run(&warped, "warped.ts").unwrap_err().to_string();
    assert!(e.contains("does not rest on"), "{e}");
}

/// What passes through what in a model: members through members, members through solids, and solids through solids
/// (a curb sits on its slab, touching: that is not through).
fn through(m: &topo_core::Model) -> Vec<String> {
    let g = Geometry::build(m, &Topology::build(m));
    let mut out: Vec<String> = g.clashes().iter().map(|(a, b, d)| format!("{} through {} by {:.1} mm", m.member_path(*a), m.member_path(*b), d * 1e3)).collect();
    out.extend(g.solid_clashes(m).into_iter().map(|(a, b, d)| format!("{a} through {b} by {:.1} mm", d * 1e3)));
    out
}

#[test]
fn nothing_passes_through_anything_in_any_example() {
    // Every example that is a model, as the app would open it (its imports from beside it).
    fn files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                files(&p, out);
            } else if p.extension().is_some_and(|x| x == "ts") {
                out.push(p);
            }
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut all = vec![];
    files(&root, &mut all);
    all.sort();
    let (mut ran, mut found) = (0, vec![]);
    for p in &all {
        let entry = p.display().to_string();
        let Ok(set) = crate::bundle::collect(&entry, &mut |id| std::fs::read_to_string(id).ok()) else { continue };
        // (A file that is only parts for others to import is not a model.)
        let Ok(script) = Script::from_files(&set, &entry) else { continue };
        let Ok((m, _)) = crate::solve::solve(&script) else { continue };
        ran += 1;
        let name = p.strip_prefix(&root).unwrap().display().to_string();
        found.extend(through(&m).into_iter().map(|t| format!("{name}: {t}")));
    }
    assert!(ran >= 20, "only {ran} of {} examples are models that evaluate", all.len());
    assert!(found.is_empty(), "{} places where one thing passes through another:\n{}", found.len(), found.join("\n"));
}

#[test]
fn a_floor_at_two_heights_has_walls_on_each_slab_and_one_top() {
    let (ft, mm) = (topo_core::units::ft, |v: f64| v / 1000.0);
    let m = run(include_str!("../../../examples/primitives/14-a-floor-at-two-heights.ts"), "14-a-floor-at-two-heights.ts").unwrap();
    all_clean(&m);
    let z = |path: &str| {
        let id = m.find_member(path).unwrap();
        (m.pos(m.member(id).start()).z, m.pos(m.member(id).end()).z)
    };
    let is = |got: (f64, f64), want: f64| (got.0 - want).abs() < 1e-6 && (got.1 - want).abs() < 1e-6;
    // The walls on the lower slab start 50 mm down, those on the upper at zero; every wall's top is at 8 ft.
    for (wall, base) in [("Front", -mm(50.0)), ("Right (front)", -mm(50.0)), ("Left (front)", -mm(50.0)), ("Right (back)", 0.0), ("Back", 0.0), ("Left (back)", 0.0)] {
        assert!(is(z(&format!("Walls/{wall}/bottom_plate")), base), "{wall} {:?}", z(&format!("Walls/{wall}/bottom_plate")));
        assert!(is(z(&format!("Walls/{wall}/cap_plate")), ft(8.0)), "{wall} {:?}", z(&format!("Walls/{wall}/cap_plate")));
    }
    // Two slabs, meeting at the step; and where two walls are in one line, their plates meet end to end (said by nothing).
    assert_eq!(m.solids.iter().filter(|s| s.role == "slab").count(), 2);
    assert!(m.issues.is_empty(), "{:#?}", m.issues);
}

#[test]
fn a_slab_goes_where_a_scan_has_the_floor() {
    let mm = |v: f64| v / 1000.0;
    let m = run(include_str!("../../../examples/primitives/15-a-slab-read-from-a-scan.ts"), "15-a-slab-read-from-a-scan.ts").unwrap();
    all_clean(&m);
    // Three readings, 11, 12 and 13 mm below where it is drawn: the slab between them, the walls on it.
    let slab = m.solids.iter().find(|s| s.role == "slab").unwrap();
    assert!((slab.top.z + mm(12.0)).abs() < 2e-4, "{}", slab.top.z);
    let id = m.find_member("Walls/Front/bottom_plate").unwrap();
    assert!((m.pos(m.member(id).start()).z - slab.top.z).abs() < 1e-9);
    // Each reading is left a millimetre off, or none: measured, and said.
    let read: Vec<f64> = m.solves.iter().flat_map(|s| &s.constraints).filter(|c| c.tier == "measured").map(|c| c.gap.abs()).collect();
    assert!(read.len() == 3 && read.iter().all(|g| *g < 1.2e-3) && read.iter().filter(|g| **g > 0.8e-3).count() == 2, "{read:?}");
}

#[test]
fn a_building_from_the_slab_up_and_a_roof_over_part() {
    let (ft, mm) = (topo_core::units::ft, |v: f64| v / 1000.0);
    let m = run(include_str!("../../../examples/primitives/16-from-the-slab-up.ts"), "16-from-the-slab-up.ts").unwrap();
    all_clean(&m);
    // The trusses sit on walls that stand on the curb's level top: their bottom chords at 8 ft above 20 mm.
    let chords: Vec<&topo_core::Member> = m.members.iter().filter(|x| x.role == "bottom_chord").collect();
    assert_eq!(chords.len(), 9);
    assert!(chords.iter().all(|c| c.path.iter().all(|n| (m.pos(*n).z - ft(8.0) - mm(20.0)).abs() < 1e-6)));
    assert_eq!(m.solids.iter().filter(|s| s.role == "curb").count(), 5);

    let m = run(include_str!("../../../examples/primitives/17-a-roof-over-part.ts"), "17-a-roof-over-part.ts").unwrap();
    all_clean(&m);
    // Nine trusses, from 4 ft to 20 ft along a 24 ft building; the three in the middle of another shape (more webs).
    let trusses: Vec<&topo_core::Group> = m.groups.iter().filter(|g| g.kind == "truss").collect();
    assert_eq!(trusses.len(), 9);
    let xs: Vec<f64> = trusses.iter().map(|g| m.pos(m.member(g.members[0]).start()).x).collect();
    assert!((xs.iter().copied().fold(f64::MAX, f64::min) - ft(4.0)).abs() < 1e-6 && (xs.iter().copied().fold(f64::MIN, f64::max) - ft(20.0)).abs() < 1e-6, "{xs:?}");
    let webs = |k: usize| trusses[k].members.iter().filter(|&&id| m.member(id).role == "web").count();
    assert!(webs(3) == webs(4) && webs(4) == webs(5) && webs(3) > webs(0) && webs(0) == webs(8), "{:?}", (0..9).map(webs).collect::<Vec<_>>());
}

#[test]
fn a_loft_sits_on_a_wall_and_a_beam() {
    let ft = topo_core::units::ft;
    let src = include_str!("../../../examples/primitives/18-a-loft-on-a-beam.ts");
    let m = run(src, "18-a-loft-on-a-beam.ts").unwrap();
    all_clean(&m);
    let loft = m.groups.iter().find(|g| g.name == "Loft").unwrap();
    let joists: Vec<&topo_core::Member> = loft.members.iter().map(|&id| m.member(id)).filter(|x| x.role == "joist").collect();
    // Drawn at zero and free: it sits with the underside of its joists at the walls' top, which is the beam's.
    assert!(joists.len() == 8 && joists.iter().all(|j| j.path.iter().all(|n| (m.pos(*n).z - ft(8.0)).abs() < 1e-6)));
    // Each joist bears on the beam and on the back wall.
    let on = |what: &str| m.solves.iter().flat_map(|s| &s.constraints).filter(|c| c.name.starts_with("Loft/joist#") && c.name.contains(what) && c.name.ends_with(": on both")).count();
    assert_eq!((on("on Beam/beam#1"), on("Walls/Back/cap_plate")), (8, 8));
    // A beam an inch too low: the loft cannot sit on it and the walls both, and that is said.
    let low = run(&src.replace("const under = ft(8) - inch(9.25);", "const under = ft(8) - inch(10.25);"), "low.ts").unwrap_err().to_string();
    assert!(low.contains("does not rest on"), "{low}");
    // What it bears on has to be there before it.
    let e = run(&src.replace(".add(walls, beam, loft)", ".add(walls, loft, beam)"), "order.ts").unwrap_err().to_string();
    assert!(e.contains("nothing named Beam to bear on"), "{e}");
}

#[test]
fn a_slab_laid_to_fall_and_a_curb_whose_top_is_level() {
    use topo_core::Vec2;
    let (ft, mm) = (topo_core::units::ft, |v: f64| v / 1000.0);
    let src = include_str!("../../../examples/primitives/12-a-slab-laid-to-fall-and-a-level-curb.ts");
    let m = run(src, "12-a-slab-laid-to-fall-and-a-level-curb.ts").unwrap();
    all_clean(&m);
    let z = |path: &str| {
        let id = m.find_member(path).unwrap();
        (m.pos(m.member(id).start()).z, m.pos(m.member(id).end()).z)
    };
    let is = |got: (f64, f64), want: f64| (got.0 - want).abs() < 1e-6 && (got.1 - want).abs() < 1e-6;
    // The slab: read 5 mm lower than drawn at its east side, and rising 10 mm per metre along x, so its west side
    // is 12 ft of that lower still.
    let slab = m.solids.iter().find(|s| s.role == "slab").unwrap();
    let (east, west) = (Vec2::new(ft(12.0), ft(4.0)), Vec2::new(0.0, ft(4.0)));
    assert!((slab.top.over(east) + mm(5.0)).abs() < 1e-6 && (slab.top.over(west) + mm(5.0) + 0.010 * ft(12.0)).abs() < 1e-6, "{slab:?}");
    // Every wall stands on the curb's top, level at 20 mm: its bottom there and its top 8 ft above, slab or no.
    for wall in ["South", "East", "North"] {
        assert!(is(z(&format!("Walls/{wall}/bottom_plate")), mm(20.0)), "{wall} {:?}", z(&format!("Walls/{wall}/bottom_plate")));
        assert!(is(z(&format!("Walls/{wall}/cap_plate")), ft(8.0) + mm(20.0)), "{wall}");
    }
    assert!(is(z("Walls/West/bottom_plate#1"), mm(20.0)) && is(z("Walls/West/bottom_plate#2"), mm(20.0)));
    // The curb: its top level at 20 mm; its underside the slab's top. 25 mm tall under the east wall, and 12 ft of
    // fall more under the west; in two pieces there, either side of the door.
    let curbs: Vec<&topo_core::Solid> = m.solids.iter().filter(|s| s.role == "curb").collect();
    assert_eq!(curbs.len(), 5, "{:?}", curbs.iter().map(|c| m.solid_path(c)).collect::<Vec<_>>());
    assert!(curbs.iter().all(|c| c.top == topo_core::Level::flat(mm(20.0)) && c.bottom == slab.top));
    let tall = |name: &str, p: Vec2| {
        let c = curbs.iter().find(|c| m.solid_path(c).ends_with(name)).unwrap();
        c.top.over(p) - c.bottom.over(p)
    };
    assert!((tall("East/curb", east) - mm(25.0)).abs() < 1e-6 && (tall("West/curb#1", west) - mm(25.0) - 0.010 * ft(12.0)).abs() < 1e-6);
    // As drawn (without the reading) the walls are just where they were, and the curb 5 mm less tall.
    let drawn = run(&src.replace("  survey(slab.at([wide, deep / 2]), { z: -mm(5) }),\n", ""), "drawn.ts").unwrap();
    let id = drawn.find_member("Walls/East/bottom_plate").unwrap();
    assert!((drawn.pos(drawn.member(id).start()).z - mm(20.0)).abs() < 1e-9);
    assert!((drawn.solids.iter().find(|s| s.role == "slab").unwrap().top.over(east)).abs() < 1e-9);
}

#[test]
fn walls_stand_straight_on_a_slab_laid_to_fall() {
    let (ft, mm) = (topo_core::units::ft, |v: f64| v / 1000.0);
    let m = run(include_str!("../../../examples/primitives/13-walls-on-a-slab-laid-to-fall.ts"), "13-walls-on-a-slab-laid-to-fall.ts").unwrap();
    let v = topo_core::validate(&m, &Topology::build(&m));
    assert!(v.iter().all(|i| i.severity < topo_core::Severity::Error), "{v:#?}");
    let z = |path: &str| {
        let id = m.find_member(path).unwrap();
        (m.pos(m.member(id).start()).z, m.pos(m.member(id).end()).z)
    };
    let rise = 0.020 * ft(12.0);
    let near = |got: (f64, f64), want: (f64, f64)| (got.0 - want.0).abs() < 1e-6 && (got.1 - want.1).abs() < 1e-6;
    // The west wall at zero, the east wall 12 ft of slope higher; each 8 ft tall. The slab slopes across those two
    // walls' plates: each lies on it, turned about its own length by the slope (its inner edge 1.8 mm higher, or
    // lower, than its outer), and the studs on it stand plumb.
    let turned = |path: &str| m.member(m.find_member(path).unwrap()).depth_dir.unwrap().z;
    assert!((turned("Walls/West/bottom_plate") - 0.02).abs() < 1e-4 && (turned("Walls/East/bottom_plate") + 0.02).abs() < 1e-4 && turned("Walls/North/bottom_plate").abs() < 1e-9);
    let stud = m.member(m.find_member("Walls/West/stud#3").unwrap());
    assert!((m.pos(stud.start()).x - m.pos(stud.end()).x).abs() < 1e-9);
    assert!(near(z("Walls/West/bottom_plate"), (0.0, 0.0)) && near(z("Walls/East/bottom_plate"), (rise, rise)), "{:?} {:?}", z("Walls/West/bottom_plate"), z("Walls/East/bottom_plate"));
    assert!(near(z("Walls/East/cap_plate"), (ft(8.0) + rise, ft(8.0) + rise)));
    // The north wall runs back down the slope (from the east to the west): its bottom plate slopes with the slab, and its top with it.
    assert!(near(z("Walls/North/bottom_plate"), (rise, 0.0)) && near(z("Walls/North/cap_plate"), (ft(8.0) + rise, ft(8.0))), "{:?}", z("Walls/North/bottom_plate"));
    // The south wall is on a curb of one height: 40 mm above the slab all along, so its plate slopes too, 40 mm up.
    assert!(near(z("Walls/South/bottom_plate"), (mm(40.0), mm(40.0) + rise)), "{:?}", z("Walls/South/bottom_plate"));
    let curb = m.solids.iter().find(|s| s.role == "curb").unwrap();
    let slab = m.solids.iter().find(|s| s.role == "slab").unwrap();
    assert!(curb.bottom == slab.top && curb.top == slab.top.raised(mm(40.0)) && m.solids.iter().filter(|s| s.role == "curb").count() == 1);
    // Nothing through anything else.
    let g = topo_geom::Geometry::build(&m, &Topology::build(&m));
    assert!(g.clashes().is_empty(), "{:?}", g.clashes());
}

#[test]
fn walls_stand_on_a_slab_and_a_curb() {
    let (ft, mm) = (topo_core::units::ft, |v: f64| v / 1000.0);
    let src = include_str!("../../../examples/primitives/10-walls-on-a-slab-and-a-curb.ts");
    let m = run(src, "10-walls-on-a-slab-and-a-curb.ts").unwrap();
    let v = topo_core::validate(&m, &Topology::build(&m));
    assert!(v.iter().all(|i| i.severity < topo_core::Severity::Error), "{v:#?}");
    // (Said: the right wall meets the others at a step at both its ends; nothing else.)
    assert!(m.issues.iter().all(|i| i.code == "corner-step") && m.issues.len() == 2, "{:#?}", m.issues);
    let z = |path: &str| {
        let id = m.find_member(path).unwrap();
        (m.pos(m.member(id).start()).z, m.pos(m.member(id).end()).z)
    };
    let is = |got: (f64, f64), want: f64| (got.0 - want).abs() < 1e-6 && (got.1 - want).abs() < 1e-6;
    // The reading: the slab's top is 45 mm below zero, not the 40 it is drawn at; it moved there.
    let slab = m.solids.iter().find(|s| s.role == "slab").unwrap();
    assert!((slab.top.z + mm(45.0)).abs() < 1e-6 && (slab.bottom.z - (slab.top.z - topo_core::units::inch(4.0))).abs() < 1e-9, "{slab:?}");
    assert_eq!(m.solid_path(slab), "Walls on a slab and a curb/Slab/slab");
    // The right wall stands on it; the others on the curb, 40 mm up: each 8 ft tall from there.
    assert!(is(z("Walls/Right/bottom_plate"), -mm(45.0)), "{:?}", z("Walls/Right/bottom_plate"));
    assert!(is(z("Walls/Right/cap_plate"), ft(8.0) - mm(45.0)), "{:?}", z("Walls/Right/cap_plate"));
    for wall in ["Front", "Back", "Left"] {
        assert!(is(z(&format!("Walls/{wall}/bottom_plate")), -mm(5.0)), "{wall} {:?}", z(&format!("Walls/{wall}/bottom_plate")));
        assert!(is(z(&format!("Walls/{wall}/cap_plate")), ft(8.0) - mm(5.0)), "{wall}");
    }
    // A curb under each of those three, from the slab's top to the wall's bottom; none under the right wall.
    let curbs: Vec<&topo_core::Solid> = m.solids.iter().filter(|s| s.role == "curb").collect();
    assert_eq!(curbs.len(), 3);
    assert!(curbs.iter().all(|c| (c.bottom.z + mm(45.0)).abs() < 1e-6 && (c.top.z + mm(5.0)).abs() < 1e-6), "{curbs:#?}");
    assert!(curbs.iter().all(|c| !m.solid_path(c).contains("Right")));
    // What each bottom plate bears on is said.
    let on = |what: &str| m.supports.iter().filter(|s| s.description.as_deref() == Some(what)).count();
    assert!(on("Bottom plate on Slab") >= 2 && on("Bottom plate on a curb on Slab") >= 6, "{:#?}", m.supports.iter().map(|s| &s.description).collect::<Vec<_>>());
    // And it is built clean: nothing through anything else.
    let g = topo_geom::Geometry::build(&m, &Topology::build(&m));
    assert!(g.clashes().is_empty(), "{:?}", g.clashes());
    // Nor through a curb, nor one curb through another: at a corner, the wall that runs through has it (its curb, or
    // its plate where it stands lower), and the other wall's curb and bottom plate stop at its inside face.
    let span = |s: &topo_core::Solid| {
        let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
        for p in s.faces().iter().flat_map(|f| f.outer.clone()) {
            for (a, v) in [p.x, p.y, p.z].into_iter().enumerate() {
                lo[a] = lo[a].min(v);
                hi[a] = hi[a].max(v);
            }
        }
        (lo, hi)
    };
    let through = |a: &([f64; 3], [f64; 3]), b: &([f64; 3], [f64; 3])| (0..3).all(|k| a.1[k].min(b.1[k]) - a.0[k].max(b.0[k]) > 1e-4);
    for (i, c) in curbs.iter().enumerate() {
        for mg in &g.members {
            let bb = mg.bbox();
            let of = ([bb.min.x, bb.min.y, bb.min.z], [bb.max.x, bb.max.y, bb.max.z]);
            assert!(!through(&span(c), &of), "{} is through {}", m.member_path(mg.member), m.solid_path(c));
        }
        for d in &curbs[i + 1..] {
            assert!(!through(&span(c), &span(d)), "{} is through {}", m.solid_path(c), m.solid_path(d));
        }
    }
    // Without the reading, the slab is where it is drawn.
    let drawn = run(&src.replace("  survey(slab.at([ft(6), ft(4)]), { z: -mm(45) }),\n", ""), "drawn.ts").unwrap();
    assert!((drawn.solids.iter().find(|s| s.role == "slab").unwrap().top.z + mm(40.0)).abs() < 1e-9);
    // A wall cannot stand on what is not there.
    let e = run(&src.replace("  slab,\n", ""), "none.ts").unwrap_err().to_string();
    assert!(e.contains("nothing named Slab to stand on"), "{e}");
}

fn roof_on_beams(far: Option<f64>) -> topo_core::Model {
    // Beams with their axes through their middles (so their tops are half their depth above), under the heels.
    let far = far.map_or(String::new(), |z| format!(r#".point("c", [-1, ft(20) - inch(2), {z}]).point("d", [ft(12) + 1, ft(20) - inch(2), {z}]).member("beam", ["c", "d"], o)"#));
    let src = format!(
        r#"
        import {{ Assembly, Building, DFL, Frame, ft, inch, TrussRoof, TrussShape }} from "topo-cad";
        const shape = TrussShape.fink({{ span: ft(20), pitch: 6 / 12, overhang: inch(12) }});
        const setOut = Frame.world.point("set-out", {{ x: 0, y: 0 }});
        const o = {{ size: [4, 8], grade: DFL.No2 }};
        const beams = Assembly.named("Beams").point("a", [-1, inch(2), ft(8)]).point("b", [ft(12) + 1, inch(2), ft(8)]).member("beam", ["a", "b"], o){far};
        const roof = TrussRoof.of(shape, {{ at: setOut.node("set-out"), spanDir: [0, 1], length: ft(12), grade: DFL.No2 }}).bearingOn(beams);
        export default Building.named("t").add(beams, setOut, roof);
    "#
    );
    run(&src, "t.ts").unwrap()
}

#[test]
fn a_truss_on_one_bearing_is_an_error_on_the_model() {
    // A beam under the near heels only: each truss sits on it, level, and nothing holds it so.
    let m = roof_on_beams(None);
    let loose: Vec<_> = m.issues.iter().filter(|i| i.code == "truss-not-held").collect();
    assert!(!loose.is_empty() && loose.iter().all(|i| i.severity == topo_core::Severity::Error), "{:#?}", m.issues);
}

#[test]
fn trusses_sit_on_the_top_of_a_beam_and_are_fastened_to_it() {
    // The far beam 38 mm lower than the near one.
    let (ft, inch) = (topo_core::units::ft, topo_core::units::inch);
    let m = roof_on_beams(Some(ft(8.0) - 0.038));
    assert!(m.issues.iter().all(|i| i.code != "truss-not-seated"), "{:#?}", m.issues);
    let top = ft(8.0) + inch(7.25) / 2.0;
    let beams: Vec<_> = m.members.iter().filter(|mm| mm.role == "beam").map(|mm| mm.id).collect();
    for c in m.members.iter().filter(|mm| mm.role == "bottom_chord") {
        // Its bottom is on the beams' tops, not at the height the roof was given (the beams' axes).
        // (and tilted, as they say: sloping down to the far side, it is on the far-side edge of each top).
        let (a, b) = (m.pos(c.start()), m.pos(c.end()));
        let at = |y: f64| a.z + (b.z - a.z) * (y - a.y) / (b.y - a.y);
        let (near, far) = (at(inch(2.0 + 1.75)), at(ft(20.0) - inch(2.0 - 1.75)));
        assert!((near - top).abs() < 1e-6 && (far - (top - 0.038)).abs() < 1e-6, "{}: {near} {far}", m.member_path(c.id));
    }
    // The model is sound with them, and the load goes down through them: each truss rests on both beams.
    let topo = Topology::build(&m);
    assert!(topo_core::validate(&m, &topo).iter().all(|i| i.severity < topo_core::Severity::Error));
    let g = Geometry::build(&m, &topo);
    let edges = topo_analysis::support_graph(&m, &topo, &g);
    for c in m.members.iter().filter(|mm| mm.role == "bottom_chord") {
        assert_eq!(edges.iter().filter(|e| e.member == c.id && beams.contains(&e.by) && e.node.is_some()).count(), 2, "{}", m.member_path(c.id));
        // The axes do not meet, so each bearing point is a node of the chord that is on a beam's top face.
        let on: Vec<_> = m.attachments.iter().filter(|a| c.path.contains(&a.node) && beams.contains(&a.member)).collect();
        assert_eq!(on.len(), 2, "{}", m.member_path(c.id));
        assert!(on.iter().all(|a| a.across.abs() <= topo_core::units::inch(1.75) + 1e-9 && m.joint(a.node).is_some_and(|j| j.connections.iter().any(|u| u.member == c.id && u.to == Some(a.member)))));
    }
}

#[test]
fn every_standard_truss_roof_builds_clean_from_typescript() {
    for ctor in [
        "fink(o)", "fan(o)", "kingPost(o)", "howe({ ...o, panels: 6 })", "pratt({ ...o, panels: 8 })",
        "scissors({ ...o, bottomPitch: 3 / 12 })", "mono({ ...o, panels: 4 })",
    ] {
        let src = format!(
            r#"
            import {{ Building, DFL, ft, inch, Perimeter, TrussRoof, TrussShape, Wall }} from "topo-cad";
            const w = Wall.template({{ height: ft(8), grade: DFL.No2 }});
            const walls = Perimeter.start("W", [0, 0]).by([ft(12), 0], w).by([0, ft(20)], w).by([-ft(12), 0], w).close(w);
            const o = {{ span: ft(20), pitch: 6 / 12, overhang: inch(12) }};
            const roof = TrussRoof.of(TrussShape.{ctor}, {{ at: walls.corner(1), spanDir: [0, 1], length: ft(12), grade: DFL.No2 }}).bearingOn(walls);
            export default Building.named("t").add(walls, roof);
        "#
        );
        let m = run(&src, "t.ts").unwrap_or_else(|e| panic!("{ctor}: {e}"));
        all_clean(&m);
    }
}

#[test]
fn custom_trusses_example_is_clean() {
    let src = include_str!("../../../examples/custom-trusses.ts");
    let m = run(src, "custom-trusses.ts").unwrap_or_else(|e| panic!("{e}"));
    all_clean(&m);
    // Beams bear on the post tops; beams meeting over a post are square-spliced.
    let topo = Topology::build(&m);
    let g = Geometry::build(&m, &topo);
    let edges = topo_analysis::support_graph(&m, &topo, &g);
    let beams: Vec<_> = m.members.iter().filter(|x| x.role == "beam").collect();
    assert_eq!(beams.len(), 3);
    for b in &beams {
        let posts = edges.iter().filter(|e| e.member == b.id && m.member(e.by).role == "post" && e.transfer == topo_analysis::Transfer::Bearing);
        assert_eq!(posts.count(), 2, "each beam bears on two posts");
    }
    // Beams splice at the interior post centres and run over the end posts.
    use topo_core::units::{ft, inch};
    let mut lengths: Vec<f64> = beams.iter().map(|b| g.member(b.id).cut_length()).collect();
    lengths.sort_by(f64::total_cmp);
    let want = [ft(8.0), ft(8.0) + inch(1.75), ft(8.0) + inch(1.75)];
    assert!(lengths.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-6), "{lengths:?}");
}

#[test]
fn placement_does_not_change_the_piece() {
    // The same assembly placed with a rotation and offset has identical cut lengths.
    let frame = |placement: &str| {
        format!(
            r#"
            import {{ Assembly, Building, DFL, ft, Placement }} from "topo-cad";
            const f = Assembly.named("frame")
              .point("a", [0, 0, 0]).point("b", [0, 0, ft(7)]).point("c", [ft(6), 0, ft(7)]).point("d", [ft(6), 0, 0])
              .member("post", ["a", "b"], {{ size: [4, 4], grade: DFL.No2 }})
              .member("post", ["d", "c"], {{ size: [4, 4], grade: DFL.No2 }})
              .member("beam", ["b", "c"], {{ size: [4, 6], grade: DFL.No2, anchor: [0, -0.5], priority: 10 }})
              .supportedAt("a", "d");
            export default Building.named("t").add(f{placement});
        "#
        )
    };
    let lengths = |src: String| {
        let m = run(&src, "t.ts").unwrap();
        all_clean(&m);
        let g = Geometry::build(&m, &Topology::build(&m));
        let mut v: Vec<i64> = g.members.iter().map(|x| (x.cut_length() * 1e6).round() as i64).collect();
        v.sort();
        v
    };
    let base = lengths(frame(""));
    assert_eq!(base, lengths(frame(".rotated(37).moved([ft(10), -ft(3), ft(2)])")));
    assert_eq!(base, lengths(frame(".placed(Placement.at([1, 2, 0], 90))")));
}

#[test]
fn shape_edit_errors_are_reported() {
    let e = run(
        r#"import { ft, TrussShape } from "topo-cad";
           TrussShape.fink({ span: ft(20), pitch: 0.5 }).withoutWeb("T1", "P");
           export default {};"#,
        "t.ts",
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("no web T1–P"), "{e}");
    let e = run(
        r#"import { ft, TrussShape } from "topo-cad";
           TrussShape.howe({ span: ft(20), pitch: 0.5, panels: 5 });
           export default {};"#,
        "t.ts",
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("even"), "{e}");
    // A moved point that kinks a chord is caught when the roof is built.
    let e = run(
        r#"import { Building, DFL, Frame, ft, TrussRoof, TrussShape } from "topo-cad";
           const s = TrussShape.fink({ span: ft(20), pitch: 0.5 }).withPoint("T1", [ft(5), ft(3)]);
           const f = Frame.world.point("set-out", { x: 0, y: 0 });
           export default Building.named("t").add(f, TrussRoof.of(s, { at: f.node("set-out"), spanDir: [0, 1], length: ft(8), grade: DFL.No2 }));"#,
        "t.ts",
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("not on the straight line"), "{e}");
}

#[test]
fn edited_standard_shape_builds_clean() {
    let src = r#"
        import { Building, DFL, ft, inch, Perimeter, TrussRoof, TrussShape, Wall } from "topo-cad";
        const w = Wall.template({ height: ft(8), grade: DFL.No2 });
        const walls = Perimeter.start("W", [0, 0]).by([ft(8), 0], w).by([0, ft(24)], w).by([-ft(8), 0], w).close(w);
        const shape = TrussShape.howe({ span: ft(24), pitch: 8 / 12, panels: 6, overhang: inch(12) })
          .withoutWeb("B1", "T2")
          .withChordPoint("Tx", "T1", "T2", 0.5)
          .web("B1", "Tx")
          .web("Tx", "B2")
          .sized("bottom_chord", [2, 6]);
        const roof = TrussRoof.of(shape, { at: walls.corner(1), spanDir: [0, 1], length: ft(8), grade: DFL.No2 }).bearingOn(walls);
        export default Building.named("t").add(walls, roof);
    "#;
    let m = run(src, "t.ts").unwrap_or_else(|e| panic!("{e}"));
    all_clean(&m);
    // The resized bottom chords are 2x6.
    assert!(m.members.iter().filter(|x| x.role == "bottom_chord").all(|x| m.section(x.section).name == "2x6"));
}

const GARAGE_TRUSS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/garage-truss.ts");

/// The measured truss reproduces every field measurement from the model's
/// own derived geometry (cut lengths, clear heights), not from the inputs.
#[test]
fn measured_truss_reproduces_field_measurements() {
    use topo_core::units::{ft, inch};
    let src = std::fs::read_to_string(GARAGE_TRUSS).unwrap();
    let m = run(&src, GARAGE_TRUSS).unwrap_or_else(|e| panic!("{e}"));
    all_clean(&m);
    let g = Geometry::build(&m, &Topology::build(&m));
    // An interior truss (not a gable end) of the preview roof.
    let truss = m.groups.iter().filter(|x| x.kind == "truss").nth(1).unwrap().id;
    let members: Vec<_> = m.members_in_tree(truss).into_iter().map(|id| m.member(id)).collect();
    let bc = members.iter().find(|x| x.role == "bottom_chord").unwrap();
    assert!((g.member(bc.id).cut_length() - ft(28.0)).abs() < 1e-6, "bottom chord lumber {}", g.member(bc.id).cut_length() / inch(1.0));

    // Vertical webs: clear height between chords along their centreline.
    let clear = |id| {
        let v = g.member(id);
        let mid = v.place.centroid;
        // Each end's material boundary along the centreline is its innermost cut plane.
        let lo = v.start.planes().map(|p| v.place.t_on(mid, p)).fold(f64::NEG_INFINITY, f64::max);
        let hi = v.end.planes().map(|p| v.place.t_on(mid, p)).fold(f64::INFINITY, f64::min);
        hi - lo
    };
    let mut verticals: Vec<f64> = members
        .iter()
        .filter(|x| x.role == "web" && g.member(x.id).place.x.z.abs() > 0.99)
        .map(|x| clear(x.id))
        .collect();
    verticals.sort_by(f64::total_cmp);
    assert_eq!(verticals.len(), 2, "centre and right verticals only");
    assert!((verticals[0] - inch(26.0)).abs() < 1e-6, "right vertical {}", verticals[0] / inch(1.0));
    assert!((verticals[1] - inch(52.5)).abs() < 1e-6, "centre vertical {}", verticals[1] / inch(1.0));

    // 2x6 bottom chord, 2x4 top chords and webs.
    assert_eq!(m.section(bc.section).name, "2x6");
    assert!(members.iter().filter(|x| x.role != "bottom_chord").all(|x| m.section(x.section).name == "2x4"));

    // The trusses span along +y; horizontal positions are world y.
    let bb = |id: topo_core::MemberId| g.member(id).bbox();
    let chord_end = bb(bc.id).max.y;
    let tops: Vec<_> = members.iter().filter(|x| x.role == "top_chord").collect();
    let (left_tc, right_tc) = if bb(tops[0].id).min.y < bb(tops[1].id).min.y { (tops[0], tops[1]) } else { (tops[1], tops[0]) };
    // Eave tail: plumb cut 25.25" past the end of the bottom chord.
    let tail = bb(right_tc.id).max.y - chord_end;
    assert!((tail - inch(25.25)).abs() < 1e-6, "tail {}", tail / inch(1.0));
    let tail_cut = &g.member(right_tc.id).start;
    assert!(tail_cut.plane.normal.z.abs() < 1e-9, "tail is plumb cut");
    // Walls: right centreline 9" in from the chord end; left centreline under
    // the outer corner (the lowest-y point of the left top chord).
    let wall_centre = |name: &str| {
        let grp = m.groups.iter().find(|x| x.name == name).unwrap().id;
        let cap = m.members_in_tree(grp).into_iter().find(|&x| m.member(x).role == "cap_plate").unwrap();
        let b = bb(cap);
        (b.min.y + b.max.y) / 2.0
    };
    let right_wall = chord_end - wall_centre("Right bearing wall");
    assert!((right_wall - inch(9.0)).abs() < 1e-6, "right wall {}", right_wall / inch(1.0));
    let left_offset = wall_centre("Left bearing wall") - bb(left_tc.id).min.y;
    assert!(left_offset.abs() < 1e-6, "left wall centre vs outer corner {}", left_offset / inch(1.0));
    // Flat part: bottom chord end to the outer corner.
    let flat = bb(left_tc.id).min.y - bb(bc.id).min.y;
    assert!((flat - inch(42.3)).abs() < 1e-6, "flat {}", flat / inch(1.0));
}

#[test]
fn relative_imports_resolve_against_the_importing_file() {
    let dir = std::env::temp_dir().join(format!("topo-script-imports-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    std::fs::write(
        dir.join("lib/shapes.ts"),
        r#"import { ft, TrussShape } from "topo-cad";
           import { PITCH } from "../consts.v2";  // a dot in the name, no .ts
           export const myTruss = (span: number): TrussShape => TrussShape.kingPost({ span, pitch: PITCH });
           export const DEFAULT_SPAN = ft(16);"#,
    )
    .unwrap();
    std::fs::write(dir.join("consts.v2.ts"), "export const PITCH: number = 5 / 12;").unwrap();
    let main = dir.join("model.ts");
    let src = r#"
        import { Building, DFL, ft, Perimeter, TrussRoof, Wall } from "topo-cad";
        import { DEFAULT_SPAN, myTruss } from "./lib/shapes";
        const w = Wall.template({ height: ft(8), grade: DFL.No2 });
        const walls = Perimeter.start("W", [0, 0]).by([ft(8), 0], w).by([0, DEFAULT_SPAN], w).by([-ft(8), 0], w).close(w);
        export default Building.named("t").add(walls,
          TrussRoof.of(myTruss(DEFAULT_SPAN), { at: walls.corner(1), spanDir: [0, 1], length: ft(8), grade: DFL.No2 }).bearingOn(walls));
    "#;
    let m = run(src, main.to_str().unwrap()).unwrap_or_else(|e| panic!("{e}"));
    all_clean(&m);
    let e = run(r#"import { x } from "./missing"; export default x;"#, main.to_str().unwrap()).unwrap_err().to_string();
    assert!(e.contains("missing"), "{e}");
    let e = run(r#"import x from "lodash"; export default x;"#, main.to_str().unwrap()).unwrap_err().to_string();
    assert!(e.contains("relative imports"), "{e}");
    std::fs::remove_dir_all(&dir).ok();
}

/// The generic solver, working only from measurements between named solid
/// features, must agree with the closed-form derivation of the same truss.
#[test]
fn fitted_truss_matches_closed_form_derivation() {
    use topo_core::units::{ft, inch};
    let src = std::fs::read_to_string(GARAGE_TRUSS).unwrap();
    let (_, fit) = run_solved(&src, GARAGE_TRUSS).unwrap_or_else(|e| panic!("{e}"));
    let fit = fit.expect("model has unknowns");
    let get = |n: &str| fit.unknowns.iter().find(|u| u.name == n).unwrap().value;

    // Closed form: centre clear height Vc = k·L/2 − d_bc; the outer corner
    // s₀ = (d_bc − d_tc·√(1+k²))/k; lumber = flat_measured − s₀ + L.
    let (db, dt) = (inch(5.5), inch(3.5));
    let (lumber, flat_meas, vc) = (ft(28.0), inch(42.3), inch(52.5));
    let mut k: f64 = 0.4;
    let mut l = 0.0;
    for _ in 0..200 {
        let s0 = (db - dt * (1.0 + k * k).sqrt()) / k;
        l = lumber - flat_meas + s0;
        k = 2.0 * (vc + db) / l;
    }
    assert!((get("pitch") - k).abs() < 1e-6, "pitch {} vs {k}", get("pitch"));
    assert!((get("span") - l).abs() < 1e-5, "span {} vs {l}", get("span"));
    let s0 = (db - dt * (1.0 + k * k).sqrt()) / k;
    assert!((get("leftWall") - s0).abs() < 1e-5, "left wall under the outer corner");
    assert!((get("rightWall") - (l - inch(9.0))).abs() < 1e-5);
    assert!(fit.rms < 1e-3, "exactly determined: rms {}", fit.rms);
    assert_eq!(fit.redundancy, 0);
    assert!(fit.undetermined().is_empty());
    assert!(fit.measurements.iter().all(|m| m.misfit().unwrap().abs() < 1e-3));
}

const FIT_MODEL: &str = r#"
    import { Assembly, Building, DFL, facing, ft, horizontal, inch, lengthOf, measured, member, unknown } from "topo-cad";
    const w = unknown("width", ft(9), { unit: "length", min: ft(1) });
    const h = unknown("height", ft(7), { unit: "length", min: ft(1) });
    const spare = unknown("spare", 1.0);   // used by nothing measured
    const frame = Assembly.named("frame")
      .point("a", [0, 0, 0]).point("b", [0, 0, h]).point("c", [w, 0, h]).point("d", [w, 0, 0])
      .member("post", ["a", "b"], { size: [4, 4], grade: DFL.No2 })
      .member("post", ["d", "c"], { size: [4, 4], grade: DFL.No2 })
      .member("beam", ["b", "c"], { size: [4, 6], grade: DFL.No2, anchor: [0, -0.5], priority: 10 })
      .supportedAt("a", "d")
      .moved([spare * 0, 0, 0]);
    const post1 = member("frame/post#1"), post2 = member("frame/post#2"), beam = member("frame/beam#1");
    export default Building.named("t").add(frame).measure(
      measured("beam", lengthOf(beam, "long"), ft(10) + inch(3.5)),
      measured("post", lengthOf(post1, "long"), ft(8)),
      measured("clear", horizontal(post1.facing("east"), facing(post2, "west")), EXTRA),
    );
"#;

#[test]
fn fit_reports_conflicts_and_undetermined_unknowns() {
    use topo_core::units::{ft, inch};
    // Consistent redundant measurement: clear width between posts = 10' − 3.5".
    let ok = FIT_MODEL.replace("EXTRA", "ft(10) - inch(3.5)");
    let (_, fit) = run_solved(&ok, "t.ts").unwrap_or_else(|e| panic!("{e}"));
    let fit = fit.unwrap();
    let get = |n: &str| fit.unknowns.iter().find(|u| u.name == n).unwrap().clone();
    assert!((get("width").value - ft(10.0)).abs() < 1e-6, "{}", get("width").value);
    assert!((get("height").value - ft(8.0)).abs() < 1e-6, "{}", get("height").value);
    assert_eq!(fit.undetermined(), vec!["spare"]);
    assert!(fit.rms < 1e-3);
    assert!(fit.measurement_rows().iter().all(|r| r[4] == "ok"));

    // A conflicting measurement (off by 1") shows up as misfits, not silently absorbed.
    let bad = FIT_MODEL.replace("EXTRA", "ft(10) - inch(3.5) + inch(1)");
    let (_, fit) = run_solved(&bad, "t.ts").unwrap();
    let fit = fit.unwrap();
    assert!(fit.rms > 1.0, "rms {}", fit.rms);
    assert!(fit.measurement_rows().iter().any(|r| r[4] == "CHECK"));
    let _ = inch(0.0);
}

#[test]
fn fit_errors_name_the_measurement() {
    let src = FIT_MODEL.replace("EXTRA", "ft(9)").replace("frame/post#2", "frame/post#9");
    let e = run_solved(&src, "t.ts").unwrap_err().to_string();
    assert!(e.contains("measurement \"clear\"") && e.contains("post#9"), "{e}");
}

/// Clicking where the outer corner is drawn picks exactly that corner, and a
/// measurement built from the pick reproduces the tape reading.
#[test]
fn picking_the_outer_corner_on_the_truss_sheet() {
    use topo_draw::pick::{locate, overlay, pick, OverlayInput};
    use topo_geom::measure::{self, Feature, PlaneRef, Quantity};
    let src = std::fs::read_to_string(GARAGE_TRUSS).unwrap();
    let (m, fit) = run_solved(&src, GARAGE_TRUSS).unwrap();
    let topo = Topology::build(&m);
    let g = Geometry::build(&m, &topo);
    let set = topo_draw::DrawingSet::build(&m, &topo, &g);
    let bc = m.find_member("T2/bottom_chord.F-H1").unwrap();
    let face = |mm: &str, side: &str| PlaneRef::Face { member: m.member_path(m.find_member(mm).unwrap()), side: side.into() };
    let corner = Feature { planes: vec![face("T2/top_chord.H0-P", "+z"), face("T2/bottom_chord.F-H1", "+z")] };
    let at = measure::feature(&m, &g, &corner).unwrap().point;
    // The typical-truss sheet shows T2.
    let sheet = set.sheets.iter().find(|s| locate(s, bc, at).is_some() && s.title.to_lowercase().contains("truss")).expect("truss sheet");
    let svg = locate(sheet, bc, at).unwrap();

    // Click a little off the corner: still within the pick radius.
    let p = pick(&m, &g, sheet, [svg[0] + 0.01, svg[1] - 0.01], 0.06).expect("something picked");
    assert_eq!(p.kind, "corner", "{p:#?}");
    let picked = p.primary.unwrap().feature;
    let same = |a: &Feature, b: &Feature| a.planes.len() == b.planes.len() && a.planes.iter().all(|x| b.planes.contains(x));
    assert!(same(&picked, &corner), "{picked:?}");
    assert_eq!(p.alternatives.len(), 2, "the two faces are offered too");

    // A measurement made from the pick reproduces the tape reading.
    let start = Feature { planes: vec![face("T2/bottom_chord.F-H1", "-x")] };
    let q = Quantity::Horizontal { a: start, b: picked };
    let v = measure::evaluate(&m, &g, &q).unwrap();
    assert!((v - topo_core::units::inch(42.3)).abs() < 1e-6, "{}", v / topo_core::units::inch(1.0));

    // Clicking on the chord's top face away from any corner picks that face.
    let mid = g.member(bc).place.at(g.member(bc).place.centroid, 3.0) + topo_core::Vec3::Z * topo_core::units::inch(2.75);
    let svg2 = locate(sheet, bc, mid).unwrap();
    let p2 = pick(&m, &g, sheet, svg2, 0.06).expect("face picked");
    assert_eq!(p2.kind, "face", "{p2:#?}");
    let label = p2.primary.unwrap().label;
    assert_eq!(label, "+z edge of T2/bottom_chord.F-H1 (up)", "canonical name plus a direction gloss");
    assert_eq!(p2.member_axes.iter().map(|a| a.name).collect::<Vec<_>>(), vec!["x", "y", "z"]);

    // Overlays: every fitted measurement that involves T2 is drawn on the sheet.
    let fit = fit.unwrap();
    let items: Vec<OverlayInput> = fit
        .measurements
        .iter()
        .map(|mm| OverlayInput { name: &mm.name, quantity: &mm.quantity, text: mm.name.clone(), ok: Some(true) })
        .collect();
    let drawn = overlay(&m, &g, sheet, &items);
    let names: Vec<&String> = drawn.iter().map(|o| &o.name).collect();
    let unique: std::collections::BTreeSet<&String> = names.iter().copied().collect();
    assert_eq!(unique.len(), fit.measurements.len(), "{names:?}");
    // The heel details repeat just the measurements that fit inside their circles.
    let mut repeated: Vec<&str> = unique.iter().filter(|n| names.iter().filter(|m| m == n).count() > 1).map(|n| n.as_str()).collect();
    repeated.sort();
    assert_eq!(repeated, vec!["left wall ℄ under outer corner", "right wall ℄ in from chord end"]);
}

/// What the UI does when saving: append a picked quantity to the sidecar
/// file; re-running the model picks it up and fits it.
#[test]
fn sidecar_round_trip() {
    use topo_core::units::{ft, inch};
    use topo_geom::measure::{Feature, PlaneRef, Quantity};
    let dir = std::env::temp_dir().join(format!("topo-script-sidecar-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let model_path = dir.join("frame.ts");
    let model_src = r#"
        import { Assembly, Building, DFL, ft, unknown } from "topo-cad";
        import { fieldMeasurements } from "./frame.measured";
        const w = unknown("width", ft(9), { unit: "length", min: ft(1) });
        const frame = Assembly.named("frame")
          .point("a", [0, 0, 0]).point("b", [0, 0, ft(7)]).point("c", [w, 0, ft(7)]).point("d", [w, 0, 0])
          .member("post", ["a", "b"], { size: [4, 4], grade: DFL.No2 })
          .member("post", ["d", "c"], { size: [4, 4], grade: DFL.No2 })
          .member("beam", ["b", "c"], { size: [4, 6], grade: DFL.No2, anchor: [0, -0.5], priority: 10 })
          .supportedAt("a", "d");
        export default Building.named("t").add(frame).measure(fieldMeasurements);
    "#;
    std::fs::write(&model_path, model_src).unwrap();
    let file = model_path.to_str().unwrap();
    // Without the sidecar the import fails with a clear message.
    assert!(run(model_src, file).unwrap_err().to_string().contains("frame.measured"));

    // With an empty sidecar the model builds; then save a picked measurement.
    measfile::ensure(&model_path).unwrap();
    let (m, fit) = run_solved(model_src, file).unwrap();
    assert!(fit.unwrap().measurements.is_empty());
    let face = |mm: &str, side: &str| PlaneRef::Face { member: m.member_path(m.find_member(mm).unwrap()), side: side.into() };
    let q = Quantity::Horizontal { a: Feature { planes: vec![face("post#1", "+y")] }, b: Feature { planes: vec![face("post#2", "-y")] } };
    let line = measfile::append(&model_path, &m, "clear between posts", &q, ft(10.0) - inch(3.5), Some("at floor level")).unwrap();
    assert!(line.contains("horizontal(member(\"post#1\").face(\"+y\"), member(\"post#2\").face(\"-y\"))"), "{line}");

    let (_, fit) = run_solved(model_src, file).unwrap_or_else(|e| panic!("{e}"));
    let fit = fit.unwrap();
    assert_eq!(fit.measurements.len(), 1);
    let w = fit.unknowns.iter().find(|u| u.name == "width").unwrap().value;
    assert!((w - ft(10.0)).abs() < 1e-6, "fitted width {}", w / inch(1.0));
    std::fs::remove_dir_all(&dir).ok();
}

/// Placing a building elsewhere in the world (moved and turned to a compass
/// bearing) changes nothing about it: same fit, same cut lengths, and every
/// measurement (horizontal distances, canonical faces) means the same thing.
#[test]
fn rotated_building_is_congruent() {
    use topo_geom::measure::{self, Quantity};
    let src = std::fs::read_to_string(GARAGE_TRUSS).unwrap();
    let placed = src.replace("export default Building", "const b = Building")
        + "\nexport default b.placed({ origin: [12, -7, 0.5], xBearing: 30 }).direction(\"street\", [0, -1, 0]);\n";
    let (m0, f0) = run_solved(&src, GARAGE_TRUSS).unwrap_or_else(|e| panic!("{e}"));
    let (m1, f1) = run_solved(&placed, GARAGE_TRUSS).unwrap_or_else(|e| panic!("{e}"));
    let (f0, f1) = (f0.unwrap(), f1.unwrap());
    for (a, b) in f0.unknowns.iter().zip(&f1.unknowns) {
        assert!((a.value - b.value).abs() < 1e-6, "{}: {} vs {}", a.name, a.value, b.value);
    }
    let lengths = |m: &topo_core::Model| {
        let g = Geometry::build(m, &Topology::build(m));
        let mut v: Vec<(String, i64)> = (0..m.members.len())
            .map(|i| {
                let id = topo_core::MemberId(i as u32);
                let q = Quantity::Length { member: m.member_path(id), how: "long".into() };
                (m.member_path(id), (measure::evaluate(m, &g, &q).unwrap() * 1e6).round() as i64)
            })
            .collect();
        v.sort();
        v
    };
    assert_eq!(lengths(&m0), lengths(&m1));
    assert_eq!(m1.groups[0].name, "Existing garage truss", "the root group is the building");

    // Named directions follow the building: its +x is 30° east of north, and
    // "street" (building −y) is 30° south of east… turned: bearing 120°.
    let root = m1.find_group("Existing garage truss").unwrap();
    let x = m1.direction("+x", Some(root)).unwrap();
    assert!((x - topo_core::v3(30f64.to_radians().sin(), 30f64.to_radians().cos(), 0.0)).norm() < 1e-9, "{x:?}");
    let street = m1.direction("street", Some(m1.find_group("Left bearing wall").unwrap())).unwrap();
    assert!((street - topo_core::v3(120f64.to_radians().sin(), 120f64.to_radians().cos(), 0.0)).norm() < 1e-9);
}

/// `facing` picks faces by named directions in any frame; `along` measures
/// in a named direction; `riseOver` reads a pitch off a sloped member.
#[test]
fn named_directions_in_measurements() {
    use topo_core::units::inch;
    use topo_geom::measure::{self, DirRef, Feature, PlaneRef, Quantity};
    let (m, fit) = run_solved(&std::fs::read_to_string(GARAGE_TRUSS).unwrap(), GARAGE_TRUSS).unwrap();
    let fit = fit.unwrap();
    let g = Geometry::build(&m, &Topology::build(&m));
    let dir = |name: &str, frame: Option<&str>| DirRef { name: Some(name.into()), vector: None, frame: frame.map(Into::into) };
    let facing = |mm: &str, d: DirRef| PlaneRef::Facing { member: mm.into(), direction: d };
    let face = |mm: &str, side: &str| PlaneRef::Face { member: mm.into(), side: side.into() };
    let feat = |p: Vec<PlaneRef>| Feature { planes: p };

    // A wall's "inside" is its +y; the cap plate's face toward it is a wide
    // (±y) or narrow face depending on how the plate lies — resolve and check the normal.
    let wall = m.find_group("Left bearing wall").unwrap();
    let inside = m.direction("inside", Some(wall)).unwrap();
    let p = measure::plane(&m, &g, &facing("Left bearing wall/cap_plate", dir("inside", Some("Left bearing wall")))).unwrap();
    assert!(p.normal.dot(inside) > 0.999, "{:?} vs {inside:?}", p.normal);
    // …and compass names work anywhere: the roof spans north, so the
    // bottom chord's start (−x) faces south.
    let south = measure::plane(&m, &g, &facing("T2/bottom_chord.F-H1", dir("south", None))).unwrap();
    let start = measure::plane(&m, &g, &face("T2/bottom_chord.F-H1", "-x")).unwrap();
    assert!((south.normal - start.normal).norm() < 1e-9 && (south.point - start.point).dot(start.normal).abs() < 1e-9);

    // along(…, "span" in T2) and along(…, "north") both read the 42.3" flat.
    let corner = feat(vec![facing("T2/top_chord.H0-P", dir("up", None)), facing("T2/bottom_chord.F-H1", dir("up", None))]);
    for d in [dir("span", Some("T2")), dir("north", None), dir("+x", Some("T2/bottom_chord.F-H1"))] {
        let q = Quantity::Along { a: feat(vec![face("T2/bottom_chord.F-H1", "-x")]), b: corner.clone(), direction: d.clone() };
        let v = measure::evaluate(&m, &g, &q).unwrap_or_else(|e| panic!("{d:?}: {e}"));
        assert!((v - inch(42.3)).abs() < 1e-6, "{d:?}: {}", v / inch(1.0));
    }

    // A level on the top chord: rise over 12" is the fitted pitch × 12".
    let k = fit.unknowns.iter().find(|u| u.name == "pitch").unwrap().value;
    let rise = measure::evaluate(&m, &g, &Quantity::Rise { member: "T2/top_chord.H0-P".into(), run: inch(12.0) }).unwrap();
    assert!((rise - k * inch(12.0)).abs() < 1e-9, "{} vs {}", rise / inch(1.0), 12.0 * k);

    // Unknown names list what is available.
    let e = measure::plane(&m, &g, &facing("T2/bottom_chord.F-H1", dir("sideways", Some("T2")))).unwrap_err();
    assert!(e.contains("span") && e.contains("north"), "{e}");
}

/// A site holds peer buildings, each in its own frame; identical local
/// coordinates never merge across buildings, and measurements can span them.
#[test]
fn site_with_two_buildings() {
    let src = r#"
        import { along, Assembly, Building, DFL, facing, ft, inch, measured, member, Site } from "topo-cad";
        const frame = Assembly.named("frame")
          .point("a", [0, 0, 0]).point("b", [0, 0, ft(7)]).point("c", [ft(9), 0, ft(7)]).point("d", [ft(9), 0, 0])
          .member("post", ["a", "b"], { size: [4, 4], grade: DFL.No2 })
          .member("post", ["d", "c"], { size: [4, 4], grade: DFL.No2 })
          .member("beam", ["b", "c"], { size: [4, 6], grade: DFL.No2, anchor: [0, -0.5], priority: 10 })
          .supportedAt("a", "d");
        const house = Building.named("House").add(frame);
        const shed = Building.named("Shed").add(frame).placed({ origin: [ft(20), 0, 0], xBearing: 0 }).direction("door", [0, -1, 0]);
        export default Site.named("Lot 7").add(house, shed).measure(
          measured("gap", along(facing(member("House/frame/post#1"), "east"), facing(member("Shed/frame/post#1"), "west"), "east"), ft(20) - inch(3.5)),
        );
    "#;
    let (m, fit) = run_solved(src, "site.ts").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(m.info.name, "Lot 7");
    assert_eq!(m.members.len(), 6);
    assert_eq!(m.nodes.len(), 8, "no nodes shared between buildings");
    assert_eq!(m.supports.len(), 4);
    let beam = m.find_member("Shed/frame/beam#1").unwrap();
    let ax = m.member_axes(beam);
    assert!((ax.x - topo_core::Vec3::Y).norm() < 1e-9, "the shed's +x points north: {:?}", ax.x);
    assert!(m.find_member("frame/beam#1").unwrap_err().contains("ambiguous"));
    let fit = fit.unwrap();
    assert!(fit.measurements[0].misfit().unwrap().abs() < 1e-9, "{:?}", fit.measurements[0]);

    // The shed's own named direction, resolved from one of its groups.
    let door = m.direction("door", Some(m.find_group("Shed/frame").unwrap())).unwrap();
    assert!((door - topo_core::Vec3::X).norm() < 1e-9, "shed −y is east: {door:?}");
}

const SHEETS_MODEL: &str = r#"
    import { Assembly, Building, DFL, detail, elevation, facing, ft, inch, iso, meet, member, notes, plan, schedule, sheet, standardSheets } from "topo-cad";
    const frame = Assembly.named("frame")
      .point("a", [0, 0, 0]).point("b", [0, 0, ft(7)]).point("c", [ft(9), 0, ft(7)]).point("d", [ft(9), 0, 0])
      .member("post", ["a", "b"], { size: [4, 4], grade: DFL.No2 })
      .member("post", ["d", "c"], { size: [4, 4], grade: DFL.No2 })
      .member("beam", ["b", "c"], { size: [4, 6], grade: DFL.No2, anchor: [0, -0.5], priority: 10 })
      .supportedAt("a", "d");
    const corner = meet(facing(member("post#1"), "up"), facing(member("post#1"), "west"));
    export default Building.named("Shed").add(frame).sheets(
      sheet("A-201", "Frame", elevation(frame, { from: "south" }), iso(), plan(frame, { scale: '1/2"' })),
      sheet("A-501", "Details", detail(corner, frame, { from: "south", radius: inch(10), scale: '3"' }), notes("Notes", "One", "Two")),
      standardSheets("schedules"),
      SHEETS_EXTRA
    );
"#;

/// Sheets as code: order, numbering, views of each kind, details clipped to
/// their circle, and problems reported (and drawn) rather than fatal.
#[test]
fn sheets_from_the_script() {
    use topo_draw::{DrawingSet, Prim};
    let m = run(&SHEETS_MODEL.replace("SHEETS_EXTRA", "[]"), "shed.ts").unwrap_or_else(|e| panic!("{e}"));
    let g = Geometry::build(&m, &Topology::build(&m));
    let set = DrawingSet::build(&m, &Topology::build(&m), &g);
    assert!(set.errors.is_empty(), "{:?}", set.errors);
    let numbers: Vec<&str> = set.sheets.iter().map(|s| s.number.as_str()).collect();
    assert_eq!(numbers, vec!["A-201", "A-501", "S-401"], "only what is listed, in order");
    let titles: Vec<&str> = set.sheets[0].views.iter().map(|v| v.view.title.as_str()).collect();
    assert_eq!(titles, vec!["frame elevation", "Shed", "frame plan"]);
    assert!(set.sheets[0].views[2].view.scale_label.starts_with("1/2\""));
    // Elevation from the south: the frame's +x (east) runs to the right.
    let e = &set.sheets[0].views[0].view;
    assert!(e.proj.unwrap().right.dot(topo_core::Vec3::X) > 0.999);

    // The detail keeps only what lies inside its circle.
    let d = &set.sheets[1].views[0].view;
    let (c, r) = d.clip.expect("a detail is clipped");
    // (The view title and its rule sit below the circle.)
    let mut inside = 0;
    for p in &d.drawing.prims {
        match p {
            Prim::Line { a, b, .. } if a.y.max(b.y) > c.y - r => {
                assert!(a.distance(c) <= r * 1.0001 && b.distance(c) <= r * 1.0001, "line outside: {p:?}");
                inside += 1;
            }
            Prim::Text { at, .. } if at.y > c.y - r => assert!(at.distance(c) <= r, "text outside: {p:?}"),
            _ => {}
        }
    }
    assert!(inside > 4, "the detail draws something");
    assert!(d.members.iter().all(|&id| m.member_path(id).contains("post#1") || m.member_path(id).contains("beam")), "the far post is not in the detail");

    // Problems are reported per view and drawn in place; the set still builds.
    let bad = SHEETS_MODEL
        .replace("SHEETS_EXTRA", r#"sheet("A-601", "Bad", plan("nothing here"), elevation(frame, { scale: '5/8"' }), schedule("rafters"))"#);
    let m = run(&bad, "shed.ts").unwrap();
    let set = DrawingSet::build(&m, &Topology::build(&m), &g);
    assert_eq!(set.errors.len(), 3, "{:?}", set.errors);
    assert!(set.errors[0].contains("\"nothing here\" names no group or member"), "{}", set.errors[0]);
    assert!(set.errors[1].contains("unknown scale") && set.errors[1].contains("1/4\""), "{}", set.errors[1]);
    assert!(set.errors[2].contains("unknown schedule \"rafters\""), "{}", set.errors[2]);
    assert_eq!(set.sheets.last().unwrap().views.iter().filter(|v| v.view.title == "View error").count(), 3);

    // Unknown standard sets are a script error.
    let e = run(&SHEETS_MODEL.replace("SHEETS_EXTRA", r#"standardSheets("elevations" as any)"#), "shed.ts").unwrap_err().to_string();
    assert!(e.contains("unknown standard sheets \"elevations\"") && e.contains("walls"), "{e}");
}

/// Without `.sheets(...)` a model gets the standard set, as before.
#[test]
fn standard_sheets_by_default() {
    let m = run(GARAGE_TS, "garage-as-built.ts").unwrap();
    assert!(m.sheets.is_none());
    let topo = Topology::build(&m);
    let g = Geometry::build(&m, &topo);
    let set = topo_draw::DrawingSet::build(&m, &topo, &g);
    let numbers: Vec<&str> = set.sheets.iter().map(|s| s.number.as_str()).collect();
    assert_eq!(numbers.first(), Some(&"G-001"));
    assert!(numbers.contains(&"S-101") && numbers.contains(&"S-201") && numbers.contains(&"S-301") && numbers.contains(&"S-401"), "{numbers:?}");
}

/// Syntax errors point at the line, for the UI's editor.
#[test]
fn syntax_errors_have_a_line() {
    let e = run("import { Building } from \"topo-cad\";\n\nconst x = (1;\nexport default Building.named(\"x\");\n", "broken.ts").unwrap_err().to_string();
    assert!(e.contains("broken.ts:3:"), "{e}");
}

/// A surveyed wall from TypeScript: compass-named corners, a drywall offset,
/// a window on kings only (no jacks), half the readings from each end, a
/// check, and errors that name the problem.
#[test]
fn surveyed_wall_from_typescript() {
    let src = r#"
        import { Building, Datum, DFL, ft, inch, Perimeter, Survey, Wall } from "topo-cad";
        const w = Wall.template({ height: ft(8), grade: DFL.No2 }).studs([2, 6], inch(16));
        // The west wall runs north from the south-west corner.
        const west = Survey.from(Datum.corner("south"))
          .stud(0).stud(inch(15.25))
          .king(inch(30))
          .window("W1", { head: inch(82.5), sill: inch(46.5), header: [2, 2, 8] })
          .king(inch(77.5))
          .from(Datum.corner("north").offset(inch(0.5), "1/2\" drywall"))
          .stud(inch(0)).stud(inch(15)).stud(inch(31)).stud(inch(47)).stud(inch(63))
          .check(Datum.corner("south"), Datum.corner("north"), ft(12) - inch(11));
        const p = Perimeter.start("P", [0, 0])
          .by([ft(20), 0], w)
          .by([0, ft(12)], w)
          .by([-ft(20), 0], w)
          .close(w.named("West").surveyed(west));
        export default Building.named("S").add(p);
    "#;
    let m = run(src, "s.ts").unwrap_or_else(|e| panic!("{e}"));
    let west: Vec<&topo_core::Member> = m.members.iter().filter(|x| m.member_path(x.id).contains("/West/")).collect();
    let n = |r: &str| west.iter().filter(|x| x.role == r).count();
    assert_eq!((n("stud"), n("king_stud"), n("jack_stud"), n("header"), n("sill")), (7, 2, 0, 1, 1));
    // From the north: inside face of the north wall (144 − 5.5), less the
    // drywall, less 0, to the near face; centre 0.75" further.
    let ys: Vec<f64> = west.iter().filter(|x| x.role == "stud").map(|x| m.pos(x.start()).y / 0.0254).collect();
    assert!(ys.iter().any(|y| (y - (144.0 - 5.5 - 0.5 - 0.75)).abs() < 1e-6), "{ys:?}");
    let chk = m.issues.iter().find(|i| i.code == "survey-check").expect("check");
    assert_eq!(chk.severity, topo_core::Severity::Info, "{}", chk.message);
    // The posts are numbers of the one solve, and the check is a reading in it. Taped an inch longer than
    // the walls are drawn, the tape (a reading) puts the west wall's ends an inch further apart (where its
    // corners are drawn are hints, alike: each gives way half an inch); the posts read from each corner go
    // with it.
    let s = &m.solves[0];
    assert!(s.constraints.iter().any(|c| c.name.starts_with("P/West: south corner") && c.tier == "measured"), "{:#?}", s.constraints.iter().map(|c| &c.name).collect::<Vec<_>>());
    let longer = run(&src.replace("ft(12) - inch(11)", "ft(12) - inch(10)"), "s.ts").unwrap();
    let ys = |m: &topo_core::Model| {
        let mut v: Vec<f64> = m.members.iter().filter(|x| m.member_path(x.id).contains("/West/") && x.role == "stud").map(|x| m.pos(x.start()).y / 0.0254).collect();
        v.sort_by(|a, b| a.total_cmp(b));
        v
    };
    let (before, after) = (ys(&m), ys(&longer));
    assert!(after[..2].iter().zip(&before[..2]).all(|(a, b)| (a - b + 0.5).abs() < 1e-6), "{before:?} {after:?}");
    assert!(after[2..].iter().zip(&before[2..]).all(|(a, b)| (a - b - 0.5).abs() < 1e-6), "{before:?} {after:?}");
    assert!(longer.issues.iter().all(|i| i.code != "reading-off"), "{:#?}", longer.issues);

    // Two readings for the same place.
    let bad = src.replace(".stud(0).stud(inch(15.25))", ".stud(0).stud(inch(0.5))");
    let e = run(&bad, "s.ts").unwrap_err().to_string();
    assert!(e.contains("overlap by 1\""), "{e}");
    // An offset must say what it is.
    let bad = src.replace("\"1/2\\\" drywall\"", "\"\"");
    let e = run(&bad, "s.ts").unwrap_err().to_string();
    assert!(e.contains("say what the offset is"), "{e}");
}

#[test]
fn a_face_seen_in_a_scan_moves_what_it_is_a_face_of() {
    // A face of a member read on a plane of a scan's frame (where a person put it, dragging it on the scan):
    // the plane is in the scan's coordinates, and the frame is turned, so the plane is too.
    use topo_core::Vec3;
    let (ft, inch) = (topo_core::units::ft, topo_core::units::inch);
    let sound = |m: &topo_core::Model| {
        let v = topo_core::validate(m, &Topology::build(m));
        assert!(v.iter().all(|i| i.severity < topo_core::Severity::Error) && m.issues.is_empty(), "{v:#?} {:#?}", m.issues);
    };
    let (o, yaw) = (Vec3::new(1.0, 2.0, 0.0), 30f64.to_radians());
    let local = |p: Vec3| {
        let d = p - o;
        Vec3::new(yaw.cos() * d.x + yaw.sin() * d.y, -yaw.sin() * d.x + yaw.cos() * d.y, d.z)
    };
    let turn = |n: Vec3| Vec3::new(yaw.cos() * n.x + yaw.sin() * n.y, -yaw.sin() * n.x + yaw.cos() * n.y, n.z);
    let f = |v: Vec3| format!("[{}, {}, {}]", v.x, v.y, v.z);
    let seen = |path: &str, at: Vec3, n: Vec3| {
        format!("  coincide(scan.on({}, {}), member(\"{path}\").node({}), {{ tolerance: 0.003 }}),\n", f(local(at)), f(turn(n)), f(n))
    };
    let src = include_str!("../../../examples/primitives/08-studs-on-readings.ts");
    let base = src
        .replace("import { Building,", "import { Building, coincide, Frame, member, Placement,")
        .replace("  tape(walls.stud(\"Front\", 2, \"bottom\"), walls.stud(\"Front\", 3, \"bottom\"), inch(15)),\n  survey(walls.stud(\"Front\", 4, \"bottom\"), { x: ft(5) }),\n", "  scan,\n")
        .replace("export default", "const scan = Frame.named(\"Scan\").placed(Placement.at([1, 2, 0], 30));\nexport default");
    let plain = run(&base, "plain.ts").unwrap();
    let stud = |m: &topo_core::Model, k: usize| m.pos(m.member(m.find_member(&format!("Walls/Front/stud#{k}")).unwrap()).start());
    // The 4th stud's west face seen at 5 ft less half its thickness: the stud moves along the wall to 5 ft (its
    // layout is a fallback), and nothing else does.
    let face = seen("Walls/Front/stud#4", Vec3::new(ft(5.0) - inch(0.75), 0.05, 1.0), Vec3::new(-1.0, 0.0, 0.0));
    let m = run(&base.replace("  scan,\n", &format!("  scan,\n{face}")), "seen.ts").unwrap();
    sound(&m);
    assert!((stud(&m, 4).x - ft(5.0)).abs() < 1e-6, "{:?}", stud(&m, 4));
    for k in [1, 2, 3, 5] {
        assert!(stud(&m, k).distance(stud(&plain, k)) < 1e-9, "stud {k} moved");
    }
    let read = m.solves[0].constraints.iter().find(|c| c.name.contains("read to be at")).unwrap();
    assert!(read.tier == "measured" && read.gap.abs() < 1e-6, "{read:?}");
    // The wall's outside face (its line, y = 0) seen 20 mm out at both ends: the wall moves out, built there.
    let out = |x: f64| seen("Walls/Front/bottom_plate", Vec3::new(x, -0.02, 0.0), Vec3::new(0.0, -1.0, 0.0));
    let wall = run(&base.replace("  scan,\n", &format!("  scan,\n{}{}", out(0.5), out(ft(11.0)))), "wall.ts");
    let wall = wall.unwrap();
    assert!((stud(&wall, 3).y - (stud(&plain, 3).y - 0.02)).abs() < 1e-6, "{:?} {:?}", stud(&wall, 3), stud(&plain, 3));
}
