//! Benchmark problems (`examples/benchmarks/*.ts`): each is run through the
//! load takedown and NDS checks, and compared with the hand calculation in
//! its header, recomputed here independently of the engine.

use crate::run;
use topo_analysis::{nds_asd, takedown, Analysis, Takedown};
use topo_core::{Model, Topology};
use topo_geom::Geometry;

const IN: f64 = 0.0254;
const LB: f64 = 4.448_221_615_260_5;
const PLF: f64 = LB / (12.0 * IN);

struct Bench {
    m: Model,
    td: Takedown,
    a: Analysis,
}

fn bench(file: &str) -> Bench {
    let path = format!("{}/../../examples/benchmarks/{file}", env!("CARGO_MANIFEST_DIR"));
    let src = std::fs::read_to_string(&path).unwrap();
    let m = run(&src, &path).unwrap_or_else(|e| panic!("{e}"));
    let topo = Topology::build(&m);
    let g = Geometry::build(&m, &topo);
    let td = takedown(&m, &topo, &g);
    assert!(td.issues.is_empty(), "{file}: {:#?}", td.issues);
    let a = nds_asd(&m, &g, &td);
    Bench { m, td, a }
}

impl Bench {
    fn check(&self, member: &str, title: &str, combo: &str) -> &topo_analysis::Check {
        let id = self.m.find_member(member).unwrap();
        let hits: Vec<_> = self.a.checks.iter().filter(|c| c.member == id && c.title.starts_with(title) && c.combination == combo).collect();
        assert_eq!(hits.len(), 1, "{member} / {title} / {combo}: {:?}", self.a.checks.iter().filter(|c| c.member == id).map(|c| (&c.title, &c.combination)).collect::<Vec<_>>());
        hits[0]
    }
    /// Self-weight of a member (plf).
    fn self_weight(&self, member: &str) -> f64 {
        let m = self.m.member(self.m.find_member(member).unwrap());
        self.m.material(m.material).density * 9.80665 * self.m.section(m.section).props.area / PLF
    }
}

fn close(what: &str, got: f64, want: f64, rel: f64) {
    assert!((got - want).abs() <= rel * want.abs(), "{what}: got {got:.4}, hand calc {want:.4}");
}

/// NDS column stability factor (Eq. 3.7-1), c = 0.8.
fn cp(fce: f64, fcs: f64) -> f64 {
    let r = fce / fcs;
    let k = (1.0 + r) / 1.6;
    k - (k * k - r / 0.8).sqrt()
}

#[test]
fn b1_floor_joist() {
    let b = bench("01-floor-joist.ts");
    let (l, s, a, i) = (168.0, 1.5 * 9.25f64.powi(2) / 6.0, 1.5 * 9.25, 1.5 * 9.25f64.powi(3) / 12.0);
    let w = 10.0 * 16.0 / 12.0 + b.self_weight("joist#1") + 40.0 * 16.0 / 12.0; // plf, D + L
    let m = w / 12.0 * l * l / 8.0;
    let r = w / 12.0 * l / 2.0;
    let c = b.check("joist#1", "Bending", "D + L");
    close("f_b", c.demand, m / s, 0.002);
    close("F'_b", c.capacity, 900.0 * 1.0 * 1.1 * 1.15, 1e-9);
    let c = b.check("joist#1", "Shear", "D + L");
    close("f_v", c.demand, 1.5 * r / a, 0.002);
    close("F'_v", c.capacity, 180.0, 1e-9);
    let c = b.check("joist#1", "Deflection", "D + L");
    close("Δ", c.demand, 5.0 * (w / 12.0) * l.powi(4) / (384.0 * 1.6e6 * i), 0.005);
    close("L/240", c.capacity, l / 240.0, 1e-6);
    // Each post takes half, plus its own weight to the footing.
    let post = b.check("post#1", "Compression", "D + L");
    close("post f_c", post.demand, (r + b.self_weight("post#1") * 8.0) / 12.25, 0.002);
}

#[test]
fn b2_header_point_loads() {
    let b = bench("02-header-point-loads.ts");
    let s = 2.0 * 1.5 * 11.25f64.powi(2) / 6.0;
    let sw = b.self_weight("header#1");
    let p = 900.0;
    let m = 16.0 * p * 12.0 + sw / 12.0 * 192.0f64.powi(2) / 8.0; // lb·in, D + S
    let c = b.check("header#1", "Bending", "D + S");
    close("f_b", c.demand, m / s, 0.002);
    close("F'_b", c.capacity, 900.0 * 1.15 * 1.0, 1e-9);
    let r = 3.5 * p + sw * 8.0;
    let c = b.check("header#1", "Shear", "D + S");
    close("f_v", c.demand, 1.5 * r / (2.0 * 1.5 * 11.25), 0.002);
    // Reactions: each post gets half of everything on the header.
    let (_, rs) = b.td.combine(&b.a.combos.iter().find(|c| c.name == "D + S").unwrap().factors);
    let hdr = b.m.find_member("header#1").unwrap();
    for rr in rs.iter().filter(|x| x.member == hdr) {
        close("reaction", rr.force.z / LB, r, 0.002);
    }
}

#[test]
fn b3_post_column() {
    let b = bench("03-post-column.ts");
    let c = b.check("post#1", "Compression", "D + L");
    let p = 5000.0; // at the top; the post's own weight adds toward the base
    let fcs = 1350.0 * 1.0 * 1.15;
    let fce = 0.822 * 580_000.0 / (96.0f64 / 3.5).powi(2);
    close("F'_c", c.capacity, fcs * cp(fce, fcs), 1e-6);
    // Governs at the base, including the post's own weight.
    close("f_c", c.demand, (p + b.self_weight("post#1") * 8.0) / 12.25, 0.002);
}

#[test]
fn b4_nailed_header() {
    let b = bench("04-nailed-header.ts");
    let sw = b.self_weight("header#1");
    let r = (100.0 + 150.0 + sw) * 3.0; // lb per end, D + S
    let hdr = b.m.find_member("header#1").unwrap();
    let nails: Vec<_> = b.a.checks.iter().filter(|c| c.member == hdr && c.title.starts_with("Nailed") && c.combination == "D + S").collect();
    assert_eq!(nails.len(), 2, "one per end");
    let z = 141.0; // NDS Table 12N, 16d common, 1-1/2" side, G = 0.50
    for c in nails {
        close("R", c.demand, r, 0.002);
        close("4 Z'", c.capacity, 4.0 * z * 1.15 * 0.67, 0.003);
        assert!(c.ratio() > 1.0, "the nailing is inadequate");
    }
    assert!(b.a.issues.iter().any(|i| i.code == "nail-only-support"));
}

#[test]
fn b5_roof_takedown() {
    let b = bench("05-roof-takedown.ts");
    let combo = b.a.combos.iter().find(|c| c.name == "D + S").unwrap();
    let (_, rs) = b.td.combine(&combo.factors);
    // Reactions of each truss onto the walls.
    let truss_weight = |t: &str| -> f64 {
        b.m.members.iter().filter(|m| b.m.member_path(m.id).contains(&format!("/{t}/"))).map(|m| {
            b.m.material(m.material).density * 9.80665 * b.m.section(m.section).props.area * b.m.member_length(m.id) / LB
        }).sum()
    };
    let truss_reaction = |t: &str| -> f64 {
        rs.iter().filter(|r| b.m.member_path(r.member).contains(&format!("/{t}/")) && r.by.is_some_and(|by| !b.m.member_path(by).contains("/T"))).map(|r| r.force.z / LB).sum()
    };
    // Interior truss: 40 psf × 2' × 14' plan, plus its own weight.
    close("interior truss", truss_reaction("T4"), 40.0 * 2.0 * 14.0 + truss_weight("T4"), 0.002);
    // Gable-end truss: half the tributary width.
    close("end truss", truss_reaction("T1"), 40.0 * 1.0 * 14.0 + truss_weight("T1"), 0.002);
    // Everything reaches the foundation.
    let applied: f64 = combo.factors.iter().map(|(id, f)| f * b.td.applied[id.idx()]).sum::<f64>() / LB;
    let found: f64 = rs.iter().filter(|r| r.by.is_none()).map(|r| r.force.z / LB).sum();
    close("foundation", found, applied, 1e-9);
    close("roof load", applied - b.m.members.iter().map(|m| b.m.material(m.material).density * 9.80665 * b.m.section(m.section).props.area * b.m.member_length(m.id) / LB).sum::<f64>(), 40.0 * 14.0 * 16.0, 1e-6);
}

#[test]
fn b6_awc_span_table() {
    let b = bench("06-awc-span-table.ts");
    // Published: F_b required 1,255 psi at 16'-5" (rounded span).
    let c = b.check("joist#1", "Bending", "D + L");
    close("f_b vs AWC Table F-2", c.demand, 1255.0, 0.006);
    assert!(c.ratio() > 1.0, "DF-L No.2 does not make this span in bending");
    // Live-load deflection is right at L/360.
    let l: f64 = 197.0;
    let d = b.check("joist#1", "Deflection (live)", "L");
    close("Δ_L", d.demand, 5.0 * (40.0 * 16.0 / 12.0 / 12.0) * l.powi(4) / (384.0 * 1.6e6 * 1.5 * 9.25f64.powi(3) / 12.0), 0.003);
    close("L/360", d.capacity, l / 360.0, 1e-6);
    assert!((d.ratio() - 1.0).abs() < 0.01, "at the deflection limit: {}", d.ratio());
}

/// ASCE 7-22 ground snow loads are strength-level: ASD takes 0.7S.
#[test]
fn asce7_22_snow_combinations() {
    let src = std::fs::read_to_string(format!("{}/../../examples/benchmarks/02-header-point-loads.ts", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let src = src.replace(".add(header)", ".designBasis({ asce7: \"7-22\" }).add(header)");
    let m = run(&src, "b2.ts").unwrap();
    assert_eq!(m.standards.asce7, "7-22");
    let names: Vec<String> = topo_analysis::asce7_asd(&m.load_cases, &m.standards.asce7).into_iter().map(|c| c.name).collect();
    assert_eq!(names, vec!["D", "D + 0.7S"]);
}

/// Structural results on sheets: a utilization view (members filled by
/// band) and the checks and reactions schedules, all from the script.
#[test]
fn structural_sheets() {
    use topo_draw::{DrawingSet, Layer, Prim};
    let b = bench("04-nailed-header.ts");
    let topo = Topology::build(&b.m);
    let g = Geometry::build(&b.m, &topo);
    let set = DrawingSet::build(&b.m, &topo, &g);
    assert!(set.errors.is_empty(), "{:?}", set.errors);
    let s = set.sheets.iter().find(|s| s.number == "S-601").expect("S-601");
    let titles: Vec<&str> = s.views.iter().map(|v| v.view.title.as_str()).collect();
    assert_eq!(titles, vec!["Member utilization: Opening", "Member checks", "Foundation reactions"]);
    let fills = |l: Layer| s.views[0].view.drawing.prims.iter().filter(|p| matches!(p, Prim::Poly { fill: true, layer, .. } if *layer == l)).count();
    // The header on nails (ratio ≈ 1.8) is in the top band; the posts in the lowest.
    assert!(fills(Layer::Heat4) >= 2, "header fill + legend swatch");
    assert!(fills(Layer::Heat0) >= 3, "two posts + legend swatch");
}

/// A model takes its snow load from the site template: ASCE 7-16
/// p_f = 0.7 C_e C_t I_s p_g = 0.7 × 1.0 × 1.2 × 1.0 × 25 = 21 psf.
#[test]
fn roof_snow_from_site_hazards() {
    let dir = format!("{}/../../examples/benchmarks", env!("CARGO_MANIFEST_DIR"));
    let src = std::fs::read_to_string(format!("{dir}/05-roof-takedown.ts")).unwrap()
        .replace("import { Building,", "import { site } from \"../site-template\";\nimport { roofSnow, Building,")
        .replace(".load(\"snow\", psf(25))", ".snow(roofSnow(site, { ce: 1.0, ct: 1.2 }))");
    let m = run(&src, &format!("{dir}/snow.ts")).unwrap_or_else(|e| panic!("{e}"));
    let snow = m.loads.iter().find_map(|l| match l {
        topo_core::Load::Area { case, pressure, label, .. } if m.load_cases[case.idx()].kind == topo_core::LoadKind::Snow => Some((*pressure, label.clone())),
        _ => None,
    });
    let (p, label) = snow.expect("a snow load");
    close("p_f", p / 47.880259, 21.0, 1e-9);
    assert!(label.unwrap().contains("county IRC Table R301.2(1)"), "the source travels with the load");
}

/// King County (unincorporated) snow: P_g = C_g·h (Public Rule 16-04, Table
/// 16-V), P_f = C_e·I·P_g, never below 25 psf (KCC 16.04.410); refused above
/// 1,000 ft, where a full snow analysis is required.
#[test]
fn king_county_snow() {
    let dir = format!("{}/../../examples/benchmarks", env!("CARGO_MANIFEST_DIR"));
    let base = std::fs::read_to_string(format!("{dir}/05-roof-takedown.ts")).unwrap();
    let roof_snow = |location: &str, elevation: f64| -> Result<(f64, String), String> {
        let src = base
            .replace("import { Building,", "import { kingCounty, kingCountyRoofSnow } from \"../sites/king-county-wa\";\nimport { Building,")
            .replace(
                ".load(\"snow\", psf(25))",
                &format!(".snow(kingCountyRoofSnow(kingCounty({{ location: \"{location}\", elevationFt: {elevation}, seismicDesignCategory: \"D2\", exposure: \"B\" }}), {{ elevationFt: {elevation}, ce: 1.0, importance: 1.0 }}))"),
            );
        let m = run(&src, &format!("{dir}/kc.ts")).map_err(|e| e.to_string())?;
        Ok(m.loads.iter().find_map(|l| match l {
            topo_core::Load::Area { case, pressure, label, .. } if m.load_cases[case.idx()].kind == topo_core::LoadKind::Snow => Some((pressure / 47.880259, label.clone().unwrap())),
            _ => None,
        }).unwrap())
    };
    // Issaquah, 400 ft: P_g = 0.054 × 400 = 21.6 psf → the 25 psf minimum governs.
    let (p, label) = roof_snow("Issaquah", 400.0).unwrap();
    close("Issaquah 400 ft", p, 25.0, 1e-9);
    assert!(label.contains("21.6") && label.contains("minimum 25 psf governs"), "{label}");
    // North Bend, 900 ft: P_g = 0.075 × 900 = 67.5 psf.
    let (p, _) = roof_snow("North Bend", 900.0).unwrap();
    close("North Bend 900 ft", p, 67.5, 1e-9);
    // Above 1,000 ft the helper refuses.
    assert!(roof_snow("Skykomish", 1100.0).unwrap_err().contains("above 1,000 ft"));
}
