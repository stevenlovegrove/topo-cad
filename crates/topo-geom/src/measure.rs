//! Field measurements between physical features of member solids.
//!
//! Faces are named in the member's canonical frame: `±x` are the cut ends
//! (x runs with the grain, first node to last), `±y` the wide faces (across
//! the thickness), `±z` the narrow edges (across the depth). Names never
//! depend on which way is up; use `PlaneRef::Facing` to select a face by a
//! direction named in any frame (`north`, a wall's `inside`, …).
//!
//! A *feature* is the intersection of one to three named planes of member
//! solids — a face, an edge/corner line, or a point — e.g. "where the top face
//! of the top chord meets the top face of the bottom chord". Features are
//! named through stable member paths (`Model::find_member`), so they keep
//! their meaning when the model's parameters change. A measurement relates
//! features (or a member's length) to a value read off a tape.

use crate::solid::MemberGeom;
use crate::Geometry;
use serde::{Deserialize, Serialize};
use topo_core::units::INCH;
use topo_core::{Model, Plane, Vec2, Vec3};

pub use topo_core::refs::{resolve_direction, DirRef, Feature, PlaneRef};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Quantity {
    /// Horizontal distance between two features. The direction is the one
    /// horizontal direction along which both features have a definite position.
    Horizontal { a: Feature, b: Feature },
    /// Vertical distance between two features.
    Vertical { a: Feature, b: Feature },
    /// Length of a member: `long` (long point to long point — the lumber
    /// length) or `centreline` (between the end cuts along the centreline).
    Length { member: String, how: String },
    /// Distance between two features along a direction.
    Along { a: Feature, b: Feature, direction: DirRef },
    /// Rise of a sloped member over a horizontal `run` (e.g. 12" for pitch).
    Rise { member: String, run: f64 },
}

fn default_tolerance() -> f64 {
    INCH / 16.0
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Measurement {
    pub name: String,
    pub quantity: Quantity,
    /// Measured value (m).
    pub value: f64,
    /// Expected accuracy (m); residuals are weighted by it. Default 1/16".
    #[serde(default = "default_tolerance")]
    pub tolerance: f64,
    #[serde(default)]
    pub note: Option<String>,
}

fn member_geom<'a>(model: &Model, geom: &'a Geometry, path: &str) -> Result<&'a MemberGeom, String> {
    Ok(geom.member(model.find_member(path)?))
}

/// Canonical face names, in picking order.
pub const SIDES: [&str; 6] = ["+z", "-z", "+y", "-y", "-x", "+x"];

/// Canonical name for a face name or deprecated alias.
pub fn canonical_side(side: &str) -> Result<&'static str, String> {
    Ok(match side {
        "+x" | "end" => "+x",
        "-x" | "start" => "-x",
        "+y" | "back" => "+y",
        "-y" | "front" => "-y",
        "+z" | "top" => "+z",
        "-z" | "bottom" => "-z",
        s => return Err(format!("unknown face \"{s}\" (use +x/-x for the ends, ±y for the wide faces, ±z for the edges)")),
    })
}

/// What a canonical face is, in lumber terms: `end`, `face` or `edge`.
pub fn side_kind(side: &str) -> &'static str {
    match side {
        "+x" | "-x" => "end",
        "+y" | "-y" => "face",
        _ => "edge",
    }
}

/// The named face plane of a member solid (outward normal).
pub fn face_plane(g: &MemberGeom, side: &str) -> Result<Plane, String> {
    let target = match canonical_side(side)? {
        "-x" => return Ok(g.start.plane),
        "+x" => return Ok(g.end.plane),
        "+z" => Vec2::new(0.0, 1.0),
        "-z" => Vec2::new(0.0, -1.0),
        "+y" => Vec2::new(1.0, 0.0),
        _ => Vec2::new(-1.0, 0.0),
    };
    let h = &g.place.hull;
    let n = h.len();
    // The envelope side whose outward normal best matches the requested side.
    let score = |k: usize| {
        let e = h[(k + 1) % n] - h[k];
        Vec2::new(e.y, -e.x).normalized().dot(target)
    };
    let i = (0..n).max_by(|&i, &j| score(i).total_cmp(&score(j))).unwrap();
    Ok(Plane::new(g.place.at(h[i], 0.0), g.place.side_normal(h[i], h[(i + 1) % n])))
}

/// The canonical face of a member whose outward normal is closest to `dir`
/// (within 45°).
pub fn facing_side(g: &MemberGeom, dir: Vec3) -> Result<&'static str, String> {
    let scored: Vec<(f64, &'static str)> = SIDES.iter().filter_map(|&s| face_plane(g, s).ok().map(|p| (p.normal.dot(dir), s))).collect();
    let (c, s) = scored.into_iter().max_by(|a, b| a.0.total_cmp(&b.0)).ok_or("member has no faces")?;
    if c < std::f64::consts::FRAC_1_SQRT_2 {
        return Err(format!("no face points within 45° of that direction (closest: {s}, {:.0}° off)", c.clamp(-1.0, 1.0).acos().to_degrees()));
    }
    Ok(s)
}

/// Mid-plane of a member across its `depth` (normal v) or `width` (normal u).
pub fn mid_plane(g: &MemberGeom, axis: &str) -> Result<Plane, String> {
    let (lo, hi) = g.place.hull.iter().fold((Vec2::new(f64::MAX, f64::MAX), Vec2::new(f64::MIN, f64::MIN)), |(a, b), p| (a.min(*p), b.max(*p)));
    let mid = (lo + hi) * 0.5;
    match axis {
        "z" | "depth" => Ok(Plane::new(g.place.at(mid, 0.0), g.place.v)),
        "y" | "width" => Ok(Plane::new(g.place.at(mid, 0.0), g.place.u)),
        a => Err(format!("unknown mid-plane axis \"{a}\" (use z for the depth or y for the thickness)")),
    }
}

/// The plane named by `r`, with an outward normal for faces.
pub fn plane(model: &Model, geom: &Geometry, r: &PlaneRef) -> Result<Plane, String> {
    match r {
        PlaneRef::Face { member, side } => face_plane(member_geom(model, geom, member)?, side),
        PlaneRef::Mid { member, axis } => mid_plane(member_geom(model, geom, member)?, axis),
        PlaneRef::Facing { member, direction } => {
            let g = member_geom(model, geom, member)?;
            face_plane(g, facing_side(g, resolve_direction(model, direction)?)?)
        }
    }
}

/// An affine subspace: a point plus orthonormal directions (0 = point, 1 = line, 2 = plane).
#[derive(Clone, Debug, PartialEq)]
pub struct Affine {
    pub point: Vec3,
    pub dirs: Vec<Vec3>,
}

/// Orthonormal basis of the span of `vs` (Gram–Schmidt, dropping dependents).
fn basis(vs: &[Vec3]) -> Vec<Vec3> {
    let mut out: Vec<Vec3> = vec![];
    for &v in vs {
        let r = out.iter().fold(v, |r, b| r - *b * r.dot(*b));
        if let Some(u) = r.try_normalized().filter(|_| r.norm() > 1e-9) {
            out.push(u);
        }
    }
    out
}

/// Intersection of 1–3 planes.
#[allow(clippy::needless_range_loop)] // row operations index two rows at once
pub fn intersect(planes: &[Plane]) -> Result<Affine, String> {
    if planes.is_empty() || planes.len() > 3 {
        return Err("a feature needs one to three planes".into());
    }
    let normals: Vec<Vec3> = planes.iter().map(|p| p.normal).collect();
    if basis(&normals).len() < planes.len() {
        return Err("these faces are parallel, so they do not meet in an edge or corner".into());
    }
    // Point: x = Σ y_j n_j with (N Nᵀ) y = c, solved by Gaussian elimination.
    let k = planes.len();
    let c: Vec<f64> = planes.iter().map(|p| p.normal.dot(p.point)).collect();
    let mut a: Vec<Vec<f64>> = (0..k).map(|i| (0..k).map(|j| normals[i].dot(normals[j])).chain([c[i]]).collect()).collect();
    for col in 0..k {
        let piv = (col..k).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs())).unwrap();
        a.swap(col, piv);
        for row in 0..k {
            if row != col {
                let f = a[row][col] / a[col][col];
                for x in col..=k {
                    a[row][x] -= f * a[col][x];
                }
            }
        }
    }
    let point = (0..k).fold(Vec3::ZERO, |acc, i| acc + normals[i] * (a[i][k] / a[i][i]));
    let dirs = match k {
        1 => {
            let n = normals[0];
            let d1 = n.any_perpendicular();
            vec![d1, n.cross(d1).normalized()]
        }
        2 => vec![normals[0].cross(normals[1]).normalized()],
        _ => vec![],
    };
    Ok(Affine { point, dirs })
}

pub fn feature(model: &Model, geom: &Geometry, f: &Feature) -> Result<Affine, String> {
    let planes = f.planes.iter().map(|r| plane(model, geom, r)).collect::<Result<Vec<_>, _>>()?;
    intersect(&planes)
}

/// Unit direction along which both features have a single, definite
/// horizontal position, or an error if there is none.
pub fn horizontal_direction(a: &Affine, b: &Affine) -> Result<Vec3, String> {
    let mut span: Vec<Vec3> = a.dirs.iter().chain(&b.dirs).copied().collect();
    span.push(Vec3::Z);
    let bs = basis(&span);
    match bs.len() {
        1 => {
            let d = b.point - a.point;
            Ok(Vec3::new(d.x, d.y, 0.0).try_normalized().unwrap_or(Vec3::X))
        }
        2 => Ok(bs[0].cross(bs[1]).normalized()),
        _ => Err("a horizontal distance between these features is not well defined: pick vertical faces, edges or corners".into()),
    }
}

/// Model value of a measured quantity.
pub fn evaluate(model: &Model, geom: &Geometry, q: &Quantity) -> Result<f64, String> {
    match q {
        Quantity::Horizontal { a, b } => {
            let (fa, fb) = (feature(model, geom, a)?, feature(model, geom, b)?);
            let d = horizontal_direction(&fa, &fb)?;
            Ok((fb.point - fa.point).dot(d).abs())
        }
        Quantity::Vertical { a, b } => {
            let (fa, fb) = (feature(model, geom, a)?, feature(model, geom, b)?);
            if fa.dirs.iter().chain(&fb.dirs).any(|d| d.z.abs() > 1e-6) {
                return Err("a vertical distance needs horizontal faces, edges or corners".into());
            }
            Ok((fb.point.z - fa.point.z).abs())
        }
        Quantity::Length { member, how } => {
            let g = member_geom(model, geom, member)?;
            match how.as_str() {
                "long" => Ok(g.cut_length()),
                "centreline" | "centerline" => {
                    let c = g.place.centroid;
                    // Innermost cut plane at each end, along the centreline.
                    let lo = g.start.planes().map(|p| g.place.t_on(c, p)).fold(f64::NEG_INFINITY, f64::max);
                    let hi = g.end.planes().map(|p| g.place.t_on(c, p)).fold(f64::INFINITY, f64::min);
                    Ok(hi - lo)
                }
                h => Err(format!("unknown length \"{h}\" (use long or centreline)")),
            }
        }
        Quantity::Along { a, b, direction } => {
            let (fa, fb) = (feature(model, geom, a)?, feature(model, geom, b)?);
            let d = resolve_direction(model, direction)?;
            if fa.dirs.iter().chain(&fb.dirs).any(|v| v.dot(d).abs() > 1e-6) {
                return Err("a distance along this direction is not well defined: pick features square to it".into());
            }
            Ok((fb.point - fa.point).dot(d).abs())
        }
        Quantity::Rise { member, run } => {
            let x = member_geom(model, geom, member)?.place.x;
            let horizontal = (x.x * x.x + x.y * x.y).sqrt();
            if horizontal < 1e-9 {
                return Err("a vertical member has no rise over a horizontal run".into());
            }
            Ok(run * x.z.abs() / horizontal)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use topo_core::units::inch;
    use topo_core::*;

    fn face(member: &str, side: &str) -> PlaneRef {
        PlaneRef::Face { member: member.into(), side: side.into() }
    }

    /// A 2x4 post under a 2x6 beam; distances between named faces and corners.
    #[test]
    fn measures_between_faces_and_corners() {
        let mut m = Model::new("t");
        let s24 = m.add_section(Section::rect("2x4", inch(1.5), inch(3.5)));
        let s26 = m.add_section(Section::rect("2x6", inch(1.5), inch(5.5)));
        let mat = m.add_material(Material { id: MaterialId(0), name: "x".into(), family: "x".into(), e: 1e10, g: 1e9, density: 500.0, design_key: None });
        let ax = m.axes_between(Vec3::ZERO, Vec3::X, None);
        let beam = MemberSpec::new("beam", s26, mat).named("beam").priority(10).anchor(Anchor::body_toward(&ax, None, Some(Vec3::Z)));
        m.add_member_between(v3(0.0, 0.0, 2.0), v3(3.0, 0.0, 2.0), &beam);
        let post = MemberSpec::new("post", s24, mat).named("post").depth_dir(Vec3::X);
        m.add_member_between(v3(1.0, 0.0, 0.0), v3(1.0, 0.0, 2.0), &post);
        let g = Geometry::build(&m, &Topology::build(&m));
        let feat = |ps: Vec<PlaneRef>| Feature { planes: ps };

        // Beam end to the post's far face: 1 m + half the post depth (3.5"/2).
        let q = Quantity::Horizontal { a: feat(vec![face("beam", "start")]), b: feat(vec![face("post", "top")]) };
        let v = evaluate(&m, &g, &q).unwrap();
        assert!((v - (1.0 + inch(1.75))).abs() < 1e-9, "{v}");
        // Corner: beam bottom face ∩ post front face, to the beam's start end.
        let corner = feat(vec![face("beam", "bottom"), face("post", "bottom")]);
        let v = evaluate(&m, &g, &Quantity::Horizontal { a: feat(vec![face("beam", "start")]), b: corner.clone() }).unwrap();
        assert!((v - (1.0 - inch(1.75))).abs() < 1e-9, "{v}");
        // Clear height under the beam (floor = post start face).
        let v = evaluate(&m, &g, &Quantity::Vertical { a: feat(vec![face("post", "start")]), b: feat(vec![face("beam", "bottom")]) }).unwrap();
        assert!((v - 2.0).abs() < 1e-9, "{v}");
        // Lengths.
        let v = evaluate(&m, &g, &Quantity::Length { member: "post".into(), how: "long".into() }).unwrap();
        assert!((v - 2.0).abs() < 1e-9, "post butts the beam's underside: {v}");
        // A sloped reference cannot be measured horizontally.
        let mut m2 = m.clone();
        m2.add_member_between(v3(0.0, 0.0, 0.0), v3(0.5, 0.0, 0.4), &MemberSpec::new("brace", s24, mat).named("brace"));
        let g2 = Geometry::build(&m2, &Topology::build(&m2));
        let e = evaluate(&m2, &g2, &Quantity::Horizontal { a: feat(vec![face("brace", "top")]), b: feat(vec![face("post", "top")]) }).unwrap_err();
        assert!(e.contains("not well defined"), "{e}");
        // Unknown and ambiguous names are reported.
        assert!(evaluate(&m, &g, &Quantity::Length { member: "nope".into(), how: "long".into() }).unwrap_err().contains("no member"));
    }

    #[test]
    fn intersections() {
        let p = |pt: Vec3, n: Vec3| Plane::new(pt, n);
        let line = intersect(&[p(v3(1.0, 0.0, 0.0), Vec3::X), p(v3(0.0, 2.0, 0.0), Vec3::Y)]).unwrap();
        assert_eq!(line.dirs.len(), 1);
        assert!((line.point.x - 1.0).abs() < 1e-12 && (line.point.y - 2.0).abs() < 1e-12);
        let pt = intersect(&[p(v3(1.0, 0.0, 0.0), Vec3::X), p(v3(0.0, 2.0, 0.0), Vec3::Y), p(v3(0.0, 0.0, 3.0), Vec3::Z)]).unwrap();
        assert!((pt.point - v3(1.0, 2.0, 3.0)).norm() < 1e-12);
        assert!(intersect(&[p(Vec3::ZERO, Vec3::X), p(Vec3::X, -Vec3::X)]).is_err());
    }
}
