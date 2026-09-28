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
            const roof = TrussRoof.of(TrussShape.{ctor}, {{ origin: [0, 0], spanDir: [0, 1], length: ft(12), grade: DFL.No2 }}).bearingOn(walls);
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
        r#"import { Building, DFL, ft, TrussRoof, TrussShape } from "topo-cad";
           const s = TrussShape.fink({ span: ft(20), pitch: 0.5 }).withPoint("T1", [ft(5), ft(3)]);
           export default Building.named("t").add(TrussRoof.of(s, { origin: [0, 0], spanDir: [0, 1], length: ft(8), grade: DFL.No2, height: ft(8) }));"#,
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
        const roof = TrussRoof.of(shape, { origin: [0, 0], spanDir: [0, 1], length: ft(8), grade: DFL.No2 }).bearingOn(walls);
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
          TrussRoof.of(myTruss(DEFAULT_SPAN), { origin: [0, 0], spanDir: [0, 1], length: ft(8), grade: DFL.No2 }).bearingOn(walls));
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

    // Two readings for the same place.
    let bad = src.replace(".stud(0).stud(inch(15.25))", ".stud(0).stud(inch(0.5))");
    let e = run(&bad, "s.ts").unwrap_err().to_string();
    assert!(e.contains("overlap by 1\""), "{e}");
    // An offset must say what it is.
    let bad = src.replace("\"1/2\\\" drywall\"", "\"\"");
    let e = run(&bad, "s.ts").unwrap_err().to_string();
    assert!(e.contains("say what the offset is"), "{e}");
}
