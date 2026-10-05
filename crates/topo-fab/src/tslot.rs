//! T-slot aluminium extrusion: profiles with their makers' section
//! properties, and the connectors that join them (`topo-data`,
//! `tslot-profiles.json` and `tslot-connectors.json`).
//!
//! The outline is drawn schematically from the profile's size, slot width
//! and core bore (real profiles have rounded corners and shaped cavities);
//! area and stiffness are the maker's published values, not the outline's.

use topo_core::{v2, Connection, Fixity, Hardware, Rating, Section, Shape, Vec2, ROUND_FACETS};

const MM: f64 = 1e-3;
const CM2: f64 = 1e-4;
const CM4: f64 = 1e-8;

/// Outline of a profile `width` × `depth` (m) with a slot every `module`
/// along each side and a bore at each module centre.
fn outline(width: f64, depth: f64, slot: f64, bore: f64) -> Shape {
    let module = width.min(depth);
    let (nu, nv) = ((width / module).round().max(1.0) as usize, (depth / module).round().max(1.0) as usize);
    // Slot proportions, from the module: lip thickness, cavity half-width and depth.
    let (lip, cav, deep, half) = (0.1125 * module, 0.2 * module, 0.275 * module, slot / 2.0);
    // One side, walked left to right along +x at y = 0 with the profile above it.
    let side = |len: f64, n: usize| -> Vec<Vec2> {
        let mut pts = vec![];
        for k in 0..n {
            let c = module * (k as f64 + 0.5) + (len - module * n as f64) / 2.0;
            pts.extend([v2(c - half, 0.0), v2(c - half, lip), v2(c - cav, lip), v2(c - cav, deep), v2(c + cav, deep), v2(c + cav, lip), v2(c + half, lip), v2(c + half, 0.0)]);
        }
        pts
    };
    let (hw, hd) = (width / 2.0, depth / 2.0);
    let mut outer = vec![];
    // Bottom, right, top, left: each side's points placed with the profile to its left.
    let sides: [(Vec2, Vec2, Vec2, f64, usize); 4] = [
        (v2(-hw, -hd), v2(1.0, 0.0), v2(0.0, 1.0), width, nu),
        (v2(hw, -hd), v2(0.0, 1.0), v2(-1.0, 0.0), depth, nv),
        (v2(hw, hd), v2(-1.0, 0.0), v2(0.0, -1.0), width, nu),
        (v2(-hw, hd), v2(0.0, -1.0), v2(1.0, 0.0), depth, nv),
    ];
    for (corner, along, inward, len, n) in sides {
        outer.push(corner);
        outer.extend(side(len, n).into_iter().map(|p| corner + along * p.x + inward * p.y));
    }
    let mut holes = vec![];
    if bore > 0.0 {
        for i in 0..nu {
            for j in 0..nv {
                let c = v2(-hw + module * (i as f64 + 0.5) + (width - module * nu as f64) / 2.0, -hd + module * (j as f64 + 0.5) + (depth - module * nv as f64) / 2.0);
                holes.push((0..ROUND_FACETS).map(|k| std::f64::consts::TAU * k as f64 / ROUND_FACETS as f64).map(|a| c + v2(a.cos(), a.sin()) * (bore / 2.0)).collect());
            }
        }
    }
    Shape::Polygon { outer, holes }
}

/// The profile with table key `key` (e.g. `40x40`), with its maker's area,
/// second moments and torsion constant.
pub fn profile(key: &str) -> Option<Section> {
    let r = topo_data::tslot_profile(key)?;
    let s = Section::new(format!("{} {}", r.maker, r.name), outline(r.width_mm * MM, r.depth_mm * MM, r.slot_mm * MM, r.bore_mm * MM));
    // i_u is about the u (width) axis: the maker's Ix for a profile drawn width along x.
    Some(
        s.with_catalog_props(r.area_cm2 * CM2, r.ix_cm4 * CM4, r.iy_cm4 * CM4, r.it_cm4.map(|j| j * CM4))
            .with_tag("family", "tslot")
            .with_tag("profile", key)
            .with_tag("source", &format!("{}{}", r.source, if r.verified { "" } else { " (unverified)" })),
    )
}

/// The material a profile is extruded from (a `materials.json` key).
pub fn profile_material(key: &str) -> Option<&'static str> {
    topo_data::tslot_profile(key).map(|r| r.material.as_str())
}

pub fn known_profiles() -> Vec<String> {
    topo_data::tslot_profiles().rows.iter().map(|r| r.key.clone()).collect()
}

pub fn known_connectors() -> Vec<String> {
    topo_data::tslot_connectors().rows.iter().map(|r| r.key.clone()).collect()
}

/// `count` connectors with table key `key` at a joint, as a connection.
/// The joint is taken as rigid (makers publish strengths, not stiffness);
/// the ratings are those of one connector times `count`.
pub fn connector(key: &str, count: u32) -> Option<Connection> {
    let r = topo_data::tslot_connector(key)?;
    let n = count.max(1) as f64;
    let any = r.moment_nm.is_some() || r.shear_n.is_some() || r.tension_n.is_some();
    Some(Connection {
        hardware: vec![Hardware { kind: r.name.clone(), model: r.part.clone().map(|p| format!("{} {p}", r.maker)), count: count.max(1) }],
        reference: Some(r.source.clone()),
        notes: Some(match &r.note {
            Some(n) => format!("Taken as rigid in the analysis. {n}"),
            None => "Taken as rigid in the analysis.".into(),
        }),
        rating: any.then(|| Rating {
            moment: r.moment_nm.map(|v| v * n),
            shear: r.shear_n.map(|v| v * n),
            tension: r.tension_n.map(|v| v * n),
            source: r.source.clone(),
            verified: r.verified,
        }),
        ..Connection::new(&format!("{}{}", if count > 1 { format!("({count}) ") } else { String::new() }, r.name), Fixity::Rigid)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use topo_core::polygon_area;

    #[test]
    fn outline_is_a_slotted_square() {
        let Shape::Polygon { outer, holes } = outline(0.04, 0.04, 0.008, 0.0068) else { panic!() };
        // Four corners and four slots of eight points; one bore.
        assert_eq!(outer.len(), 4 * 9);
        assert_eq!(holes.len(), 1);
        // Counter-clockwise, inside the 40 mm square, less the four cavities.
        let a = polygon_area(&outer);
        assert!(a > 0.0 && a < 0.04 * 0.04, "{a}");
        let cavity = 0.008 * 0.0045 + 0.016 * (0.011 - 0.0045);
        assert!((a - (0.0016 - 4.0 * cavity)).abs() < 1e-12, "{a}");
        // A 40 x 80 has two slots on its long sides and two bores.
        let Shape::Polygon { outer, holes } = outline(0.04, 0.08, 0.008, 0.0068) else { panic!() };
        assert_eq!(outer.len(), 4 + 8 * 6);
        assert_eq!(holes.len(), 2);
    }

    #[test]
    fn catalog_rows_make_sections_and_connections() {
        for key in known_profiles() {
            let s = profile(&key).unwrap();
            let r = topo_data::tslot_profile(&key).unwrap();
            assert!((s.props.area - r.area_cm2 * CM2).abs() < 1e-12, "{key}");
            assert!((s.props.i_u - r.ix_cm4 * CM4).abs() < 1e-15 && s.props.s_u > 0.0, "{key}");
            assert!((s.props.width - r.width_mm * MM).abs() < 1e-9, "{key}");
            assert!(crate::materials::engineering(&r.material).is_some(), "{key}: material {}", r.material);
        }
        for key in known_connectors() {
            let c = connector(&key, 2).unwrap();
            assert_eq!(c.hardware[0].count, 2, "{key}");
            assert!(c.engineered());
        }
    }
}
