use super::*;
use topo_core::units::inch;
use topo_core::*;

fn lumber_model() -> (Model, SectionId, SectionId, MaterialId) {
    let mut m = Model::new("t");
    let s24 = m.add_section(Section::rect("2x4", inch(1.5), inch(3.5)));
    let dbl = m.add_section(Section::built_up("(2) 2x4", Shape::Rect { b: inch(1.5), d: inch(3.5) }, 2, 0.0));
    let mat = m.add_material(Material {
        id: MaterialId(0),
        name: "DF".into(),
        family: "sawn_lumber".into(),
        e: 1.1e10,
        g: 6.9e8,
        density: 500.0,
        design_key: None,
    });
    (m, s24, dbl, mat)
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// One wall segment along +x, interior toward +y, height `h` (bottom of
/// bottom plate to top of double top plate), one stud at `xs`.
fn wall(m: &mut Model, s24: SectionId, dbl: SectionId, mat: MaterialId, len: f64, h: f64, xs: f64) -> [MemberId; 3] {
    let (a, b) = (Vec3::ZERO, v3(len, 0.0, 0.0));
    let ax = m.axes_between(a, b, Some(Vec3::Y));
    let bottom = MemberSpec::new("plate", s24, mat)
        .priority(10)
        .depth_dir(Vec3::Y)
        .anchor(Anchor::body_toward(&ax, Some(Vec3::Z), Some(Vec3::Y)));
    let top = MemberSpec::new("plate", dbl, mat)
        .priority(10)
        .depth_dir(Vec3::Y)
        .anchor(Anchor::body_toward(&ax, Some(-Vec3::Z), Some(Vec3::Y)));
    let bp = m.add_member_between(a, b, &bottom);
    let tp = m.add_member_between(a + Vec3::Z * h, b + Vec3::Z * h, &top);
    let sax = m.axes_between(Vec3::ZERO, Vec3::Z, Some(Vec3::Y));
    let stud = MemberSpec::new("stud", s24, mat).depth_dir(Vec3::Y).anchor(Anchor::body_toward(&sax, None, Some(Vec3::Y)));
    let n0 = m.node_along(bp, xs).unwrap();
    let n1 = m.node_along(tp, xs).unwrap();
    let st = m.add_member(&[n0, n1], &stud);
    [bp, tp, st]
}

#[test]
fn stud_cut_length_is_wall_height_minus_three_plates() {
    let (mut m, s24, dbl, mat) = lumber_model();
    let h = inch(97.125);
    let [bp, tp, st] = wall(&mut m, s24, dbl, mat, inch(48.0), h, inch(24.0));
    let topo = Topology::build(&m);
    let g = Geometry::build(&m, &topo);
    assert!(g.issues.is_empty(), "{:?}", g.issues);
    let stud = g.member(st);
    assert!(close(stud.cut_length(), inch(92.625)), "stud = {}", stud.cut_length() / inch(1.0));
    assert!(matches!(stud.start.kind, CutKind::Butt { against } if against == bp));
    assert!(matches!(stud.end.kind, CutKind::Butt { against } if against == tp));
    assert!(close(g.member(bp).cut_length(), inch(48.0)));
    assert!(g.clashes().is_empty(), "{:?}", g.clashes());
    // Stud body sits inside the wall thickness.
    let bb = stud.bbox();
    assert!(close(bb.min.y, 0.0) && close(bb.max.y, inch(3.5)));
    assert!(close(bb.min.x, inch(23.25)) && close(bb.max.x, inch(24.75)));
}

#[test]
fn l_corner_higher_rank_runs_through() {
    let (mut m, s24, _, mat) = lumber_model();
    // Wall A along +x from origin (interior +y); wall B along +y from origin (interior +x).
    let ax_a = m.axes_between(Vec3::ZERO, Vec3::X, Some(Vec3::Y));
    let a = m.add_member_between(
        Vec3::ZERO,
        v3(2.0, 0.0, 0.0),
        &MemberSpec::new("plate", s24, mat).priority(10).depth_dir(Vec3::Y).anchor(Anchor::body_toward(&ax_a, Some(Vec3::Z), Some(Vec3::Y))),
    );
    let ax_b = m.axes_between(Vec3::ZERO, Vec3::Y, Some(Vec3::X));
    let b = m.add_member_between(
        Vec3::ZERO,
        v3(0.0, 2.0, 0.0),
        &MemberSpec::new("plate", s24, mat).priority(10).depth_dir(Vec3::X).anchor(Anchor::body_toward(&ax_b, Some(Vec3::Z), Some(Vec3::X))),
    );
    let topo = Topology::build(&m);
    assert_eq!(topo.junction(NodeId(0)).kind, JunctionKind::Corner);
    let g = Geometry::build(&m, &topo);
    // A (lower id) runs through to the outside corner; B butts A's inner face.
    assert!(matches!(g.member(a).start.kind, CutKind::Square | CutKind::Through { .. }));
    assert!(close(g.member(a).cut_length(), 2.0));
    assert!(matches!(g.member(b).start.kind, CutKind::Butt { against } if against == a));
    assert!(close(g.member(b).cut_length(), 2.0 - inch(3.5)));
    assert!(g.clashes().is_empty());
}

#[test]
fn centred_l_corner_extends_through_member() {
    let (mut m, s24, _, mat) = lumber_model();
    let spec = MemberSpec::new("rail", s24, mat).priority(5);
    let a = m.add_member_between(Vec3::ZERO, v3(1.0, 0.0, 0.0), &spec);
    let b = m.add_member_between(Vec3::ZERO, v3(0.0, 1.0, 0.0), &spec);
    let g = Geometry::build(&m, &Topology::build(&m));
    // Horizontal members default to depth along Z, so their width (u) is 1.5" in plan.
    assert!(matches!(g.member(a).start.kind, CutKind::Through { ref past } if past == &vec![b]));
    assert!(close(g.member(a).cut_length(), 1.0 + inch(0.75)));
    assert!(close(g.member(b).cut_length(), 1.0 - inch(0.75)));
    assert!(g.clashes().is_empty());
}

#[test]
fn miter_rule() {
    let (mut m, s24, _, mat) = lumber_model();
    let spec = MemberSpec::new("rail", s24, mat);
    let a = m.add_member_between(Vec3::ZERO, v3(1.0, 0.0, 0.0), &spec);
    let b = m.add_member_between(Vec3::ZERO, v3(0.0, 1.0, 0.0), &spec);
    m.add_rule(NodeId(0), JointRule::Miter { a, b });
    let g = Geometry::build(&m, &Topology::build(&m));
    let (sa, _) = g.member(a).cut_angles();
    assert!(close(sa, 45.0));
    // Long point of each rail reaches the outer corner.
    assert!(close(g.member(a).cut_length(), 1.0 + inch(0.75)));
    assert!(g.clashes().is_empty());
}

#[test]
fn angled_member_gets_plumb_cut_against_ridge() {
    let (mut m, s24, _, mat) = lumber_model();
    // Ridge along y at height 2; rafter from eave (x=-2, z=0) up to the ridge line.
    let ridge = m.add_member_between(v3(0.0, -1.0, 2.0), v3(0.0, 1.0, 2.0), &MemberSpec::new("ridge", s24, mat).priority(20));
    let top = m.node_along(ridge, 1.0).unwrap();
    let eave = m.node_at(v3(-2.0, 0.0, 0.0));
    let r = m.add_member(&[eave, top], &MemberSpec::new("rafter", s24, mat));
    let g = Geometry::build(&m, &Topology::build(&m));
    let rg = g.member(r);
    assert!(matches!(rg.end.kind, CutKind::Butt { against } if against == ridge));
    // The cut face is vertical (plumb): its normal is horizontal.
    assert!(rg.end.plane.normal.z.abs() < 1e-9);
    assert!(g.clashes().is_empty());
}

#[test]
fn detects_stud_overhanging_plate_end() {
    let (mut m, s24, dbl, mat) = lumber_model();
    // Stud centred 1/2" from the plate's end: 1/4" of its 1-1/2" width overhangs.
    let [bp, _, st] = wall(&mut m, s24, dbl, mat, inch(48.0), inch(97.125), inch(0.5));
    let g = Geometry::build(&m, &Topology::build(&m));
    let over = g.bearing_overhangs();
    assert_eq!(over.len(), 2, "{over:?}"); // bottom and top
    let (a, _, d) = over.iter().find(|o| o.1 == bp).copied().unwrap();
    assert_eq!(a, st);
    assert!((d - inch(0.25)).abs() < 1e-6, "{}", d / inch(1.0));
    // Fully bearing stud: no report.
    let (mut m2, s24, dbl, mat) = lumber_model();
    wall(&mut m2, s24, dbl, mat, inch(48.0), inch(97.125), inch(0.75));
    assert!(Geometry::build(&m2, &Topology::build(&m2)).bearing_overhangs().is_empty());
}

#[test]
fn detects_clash() {
    let (mut m, s24, _, mat) = lumber_model();
    let spec = MemberSpec::new("b", s24, mat);
    // Two members crossing at mid-height with no shared node.
    m.add_member_between(v3(0.0, 0.0, 0.0), v3(1.0, 0.0, 0.0), &spec);
    m.add_member_between(v3(0.5, -0.5, 0.0), v3(0.5, 0.5, 0.0), &spec);
    let g = Geometry::build(&m, &Topology::build(&m));
    assert_eq!(g.clashes().len(), 1);
}

#[test]
fn mesh_is_closed_box() {
    let (mut m, s24, _, mat) = lumber_model();
    m.add_member_between(Vec3::ZERO, v3(1.0, 0.0, 0.0), &MemberSpec::new("b", s24, mat));
    let g = Geometry::build(&m, &Topology::build(&m));
    let mesh = Mesh::from_geometry(&g);
    assert_eq!(mesh.triangles.len(), 12);
    // Divergence theorem: volume from outward-wound triangles.
    let vol: f64 = mesh
        .triangles
        .iter()
        .map(|t| {
            let (a, b, c) = (mesh.positions[t[0] as usize], mesh.positions[t[1] as usize], mesh.positions[t[2] as usize]);
            a.dot(b.cross(c)) / 6.0
        })
        .sum();
    assert!((vol - inch(1.5) * inch(3.5) * 1.0).abs() < 1e-9, "vol {vol}");
}

#[test]
fn inline_splice_over_a_post_is_square() {
    let (mut m, s24, _, mat) = lumber_model();
    // Two collinear beams meeting over a post: both stop at the node.
    let post = MemberSpec::new("post", s24, mat);
    let beam = MemberSpec::new("beam", s24, mat).priority(10);
    let a = m.add_member_between(v3(-1.0, 0.0, 1.0), v3(0.0, 0.0, 1.0), &beam);
    let b = m.add_member_between(v3(0.0, 0.0, 1.0), v3(1.0, 0.0, 1.0), &beam);
    m.add_member_between(Vec3::ZERO, v3(0.0, 0.0, 1.0), &post);
    let g = Geometry::build(&m, &Topology::build(&m));
    assert!(matches!(g.member(a).end.kind, CutKind::Square));
    assert!(matches!(g.member(b).start.kind, CutKind::Square));
    assert!(close(g.member(a).cut_length(), 1.0) && close(g.member(b).cut_length(), 1.0));
    assert!(g.clashes().is_empty());
}
