//! A roof as its surfaces. A roof is one or more parts; a part is a convex outline on plan,
//! each edge of which is an eave (a plane rises inward from it, at a pitch) or not (a gable, or
//! a wall the roof runs into). Within a part the roof is the lowest of its planes; where parts
//! overlap, the highest part is what is seen: a wing against a main roof, a dormer on a slope.
//! So ridges, hips and valleys are not given: they are where the planes meet.
//!
//! Nothing here frames anything: this is the geometry the surfaces, and later the framing, are
//! made from.

use topo_core::{polygon_area, Level, Vec2, Vec3};

/// An eave: the plane that rises inward from an edge of a part.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Eave {
    /// The plane's height over the edge's line, at the edge's start and at its end (level: the same).
    pub height: (f64, f64),
    /// Its rise per metre inward, square to the edge.
    pub pitch: f64,
    /// How far the roof runs out past the edge's line.
    pub overhang: f64,
}

/// A part of a roof: a convex outline, and what each of its edges is (edge `k` runs from point
/// `k` to the next).
#[derive(Clone, Debug)]
pub struct RoofPart {
    pub name: String,
    pub outline: Vec<Vec2>,
    pub edges: Vec<Option<Eave>>,
    /// How far the roof runs out past an edge that is not an eave (a gable's rake).
    pub rake: f64,
}

/// A piece of a roof's surface: a convex outline on plan, in a plane.
#[derive(Clone, Debug)]
pub struct Face {
    /// Which part, and which of its eaves, it is of.
    pub part: usize,
    pub edge: usize,
    pub outline: Vec<Vec2>,
    pub plane: Level,
    /// Edges of its outline (edge k from point k to the next) along which it only abuts another
    /// piece of the same plane: where the plane was cut to make convex pieces of it.
    pub seams: Vec<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EdgeKind {
    /// Where two planes meet, level: the top of the roof.
    Ridge,
    /// Where two planes meet, sloping, the roof falling away either side.
    Hip,
    /// Where two planes meet, sloping, the roof rising either side.
    Valley,
    /// The low edge of a plane.
    Eave,
    /// Any other free edge of a plane: up a gable.
    Rake,
}

/// An edge of a roof's surface.
#[derive(Clone, Copy, Debug)]
pub struct Edge {
    pub kind: EdgeKind,
    pub a: Vec3,
    pub b: Vec3,
}

/// A half-plane on plan: where `n · p + c >= 0`.
#[derive(Clone, Copy)]
struct Half {
    n: Vec2,
    c: f64,
}

impl Half {
    fn at(&self, p: Vec2) -> f64 {
        self.n.dot(p) + self.c
    }
    fn not(&self) -> Half {
        Half { n: self.n * -1.0, c: -self.c }
    }
    /// Where the first surface is as high as the second, or higher (by `by`).
    fn above(a: &Level, b: &Level, by: f64) -> Half {
        Half { n: a.slope - b.slope, c: a.over(Vec2::new(0.0, 0.0)) - b.over(Vec2::new(0.0, 0.0)) - by }
    }
}

/// What of a convex polygon is where `n · p + c >= 0`.
pub fn cut(poly: &[Vec2], n: Vec2, c: f64) -> Vec<Vec2> {
    clip(poly, &Half { n, c })
}

/// What of a convex polygon is in a half-plane.
fn clip(poly: &[Vec2], h: &Half) -> Vec<Vec2> {
    let mut out = vec![];
    for (k, &a) in poly.iter().enumerate() {
        let b = poly[(k + 1) % poly.len()];
        let (fa, fb) = (h.at(a), h.at(b));
        if fa >= 0.0 {
            out.push(a);
        }
        if (fa > 0.0 && fb < 0.0) || (fa < 0.0 && fb > 0.0) {
            out.push(a + (b - a) * (fa / (fa - fb)));
        }
    }
    // (Points that have run together are one.)
    out.dedup_by(|a, b| a.distance(*b) < 1e-9);
    if out.len() > 1 && out[0].distance(*out.last().unwrap()) < 1e-9 {
        out.pop();
    }
    out
}

fn area(poly: &[Vec2]) -> f64 {
    if poly.len() < 3 {
        0.0
    } else {
        polygon_area(poly).abs()
    }
}

impl RoofPart {
    /// Its outline, anticlockwise, with its edges in step.
    fn ccw(&self) -> (Vec<Vec2>, Vec<Option<Eave>>) {
        if polygon_area(&self.outline) >= 0.0 {
            return (self.outline.clone(), self.edges.clone());
        }
        // (Turned about: edge k, from point k to k + 1, becomes the edge from n − 1 − k − 1 … with its ends swapped.)
        let n = self.outline.len();
        let outline: Vec<Vec2> = self.outline.iter().rev().copied().collect();
        let edges = (0..n).map(|k| self.edges[(2 * n - 2 - k) % n].map(|e| Eave { height: (e.height.1, e.height.0), ..e })).collect();
        (outline, edges)
    }

    /// The half-plane inside edge `k`'s line moved out by `d`, of an anticlockwise outline.
    fn inside(outline: &[Vec2], k: usize, d: f64) -> Half {
        let (a, b) = (outline[k], outline[(k + 1) % outline.len()]);
        let dir = (b - a).normalized();
        let inward = Vec2::new(-dir.y, dir.x);
        Half { n: inward, c: -inward.dot(a) + d }
    }

    /// How far it reaches on plan: its outline, out by its overhangs and its rake (anticlockwise).
    pub fn reach(&self) -> Result<Vec<Vec2>, String> {
        Ok(self.planes()?.1)
    }

    /// Its planes (one for each eave, by edge), and how far it reaches on plan (its outline, out by
    /// its overhangs and its rake).
    fn planes(&self) -> Result<(Vec<(usize, Level)>, Vec<Vec2>), String> {
        let (outline, edges) = self.ccw();
        let n = outline.len();
        if n < 3 || edges.len() != n {
            return Err(format!("roof part {}: an outline of three points or more, and what each of its edges is", self.name));
        }
        for k in 0..n {
            let (a, b, c) = (outline[k], outline[(k + 1) % n], outline[(k + 2) % n]);
            if (b - a).cross(c - b) < -1e-9 {
                return Err(format!("roof part {}: its outline has to be convex (a roof that turns a corner is two parts that overlap)", self.name));
            }
        }
        let mut planes = vec![];
        for (k, e) in edges.iter().enumerate() {
            let Some(e) = e else { continue };
            let (a, b) = (outline[k], outline[(k + 1) % n]);
            let len = a.distance(b);
            let dir = (b - a) / len;
            let inward = Vec2::new(-dir.y, dir.x);
            planes.push((k, Level { z: e.height.0, at: a, slope: inward * e.pitch + dir * ((e.height.1 - e.height.0) / len) }));
        }
        if planes.is_empty() {
            return Err(format!("roof part {}: one of its edges at least is an eave", self.name));
        }
        // Out by each edge's overhang (or the rake): the corners where the moved lines cross.
        let mut reach: Vec<Vec2> = vec![
            outline.iter().fold(Vec2::new(f64::MAX, f64::MAX), |m, p| Vec2::new(m.x.min(p.x), m.y.min(p.y))) - Vec2::new(1e3, 1e3),
            Vec2::new(outline.iter().map(|p| p.x).fold(f64::MIN, f64::max) + 1e3, outline.iter().map(|p| p.y).fold(f64::MAX, f64::min) - 1e3),
            outline.iter().fold(Vec2::new(f64::MIN, f64::MIN), |m, p| Vec2::new(m.x.max(p.x), m.y.max(p.y))) + Vec2::new(1e3, 1e3),
            Vec2::new(outline.iter().map(|p| p.x).fold(f64::MAX, f64::min) - 1e3, outline.iter().map(|p| p.y).fold(f64::MIN, f64::max) + 1e3),
        ];
        for k in 0..n {
            reach = clip(&reach, &Self::inside(&outline, k, edges[k].map_or(self.rake, |e| e.overhang)));
        }
        Ok((planes, reach))
    }
}

/// A piece of roof to be worked out: a convex outline on plan (as far as it reaches), and the
/// planes over it, each with which eave it is of. A part is one; so is a strip of a part whose
/// planes are its own (where what carries the part is not all in one plane).
#[derive(Clone, Debug)]
pub struct Piece {
    pub outline: Vec<Vec2>,
    pub planes: Vec<(usize, Level)>,
}

impl RoofPart {
    /// The part as a piece: its reach, and a plane to each eave (by edge, of its outline anticlockwise).
    pub fn piece(&self) -> Result<Piece, String> {
        let (planes, outline) = self.planes()?;
        Ok(Piece { outline, planes })
    }
}

/// The faces of a roof: for each part, where each of its planes is the lowest of that part's,
/// less what a higher part covers. In pieces, each convex.
pub fn faces(parts: &[RoofPart]) -> Result<Vec<Face>, String> {
    faces_of(&parts.iter().map(RoofPart::piece).collect::<Result<Vec<_>, _>>()?)
}

/// The same, of pieces: a face's `part` is which piece it is of.
pub fn faces_of(pieces: &[Piece]) -> Result<Vec<Face>, String> {
    let made: Vec<(Vec<(usize, Level)>, Vec<Vec2>)> = pieces
        .iter()
        .map(|p| {
            let mut outline = p.outline.clone();
            if polygon_area(&outline) < 0.0 {
                outline.reverse();
            }
            (p.planes.clone(), outline)
        })
        .collect();
    let mut out = vec![];
    for (a, (planes, reach)) in made.iter().enumerate() {
        for (edge, plane) in planes {
            // Where this plane is the part's lowest.
            let mut region = reach.clone();
            for (other, lower) in planes {
                if other != edge {
                    region = clip(&region, &Half::above(lower, plane, 0.0));
                }
            }
            let mut pieces = vec![region];
            // Less what each other part covers: where that part is, and all its planes are higher than this one
            // (of two parts as high as each other, the first is the one seen).
            for (b, (theirs, their_reach)) in made.iter().enumerate() {
                if b == a {
                    continue;
                }
                let by = if b < a { -1e-7 } else { 1e-7 };
                let mut hidden: Vec<Half> = (0..their_reach.len()).map(|k| RoofPart::inside(their_reach, k, 0.0)).collect();
                hidden.extend(theirs.iter().map(|(_, p)| Half::above(p, plane, by)));
                let mut left = vec![];
                for piece in pieces {
                    // (A piece none of which is covered is left whole.)
                    if area(&hidden.iter().fold(piece.clone(), |p, h| clip(&p, h))) <= 1e-8 {
                        left.push(piece);
                        continue;
                    }
                    let mut cur = piece;
                    for h in &hidden {
                        let outside = clip(&cur, &h.not());
                        if area(&outside) > 1e-8 {
                            left.push(outside);
                        }
                        cur = clip(&cur, h);
                        if area(&cur) <= 1e-8 {
                            break;
                        }
                    }
                }
                pieces = left;
            }
            out.extend(pieces.into_iter().filter(|p| area(p) > 1e-8).map(|outline| Face { part: a, edge: *edge, outline, plane: *plane, seams: vec![] }));
        }
    }
    seam(&mut out);
    Ok(out)
}

fn same_plane(a: &Face, b: &Face) -> bool {
    (a.plane.slope - b.plane.slope).norm() < 1e-9 && (a.plane.over(Vec2::new(0.0, 0.0)) - b.plane.over(Vec2::new(0.0, 0.0))).abs() < 1e-6
}

/// Marks where pieces of one plane abut: each outline is given a point wherever a neighbour's
/// side starts or stops along one of its own, so that each of its edges is a seam or is not.
fn seam(faces: &mut [Face]) {
    let was: Vec<Face> = faces.to_vec();
    for (i, f) in faces.iter_mut().enumerate() {
        let n = was[i].outline.len();
        let (mut outline, mut seams) = (vec![], vec![]);
        for k in 0..n {
            let (a, b) = (was[i].outline[k], was[i].outline[(k + 1) % n]);
            let len = a.distance(b);
            let dir = (b - a) / len;
            // The stretches of this side that a piece of the same plane has a side along.
            let mut shared: Vec<(f64, f64)> = vec![];
            for (j, g) in was.iter().enumerate() {
                if j == i || !same_plane(&was[i], g) {
                    continue;
                }
                let m = g.outline.len();
                for q in 0..m {
                    let (c, d) = (g.outline[q], g.outline[(q + 1) % m]);
                    if (c - a).cross(dir).abs() > 1e-6 || (d - a).cross(dir).abs() > 1e-6 {
                        continue;
                    }
                    let (t0, t1) = ((c - a).dot(dir).min((d - a).dot(dir)).max(0.0), (c - a).dot(dir).max((d - a).dot(dir)).min(len));
                    if t1 - t0 > 1e-6 {
                        shared.push((t0, t1));
                    }
                }
            }
            let mut cuts: Vec<f64> = shared.iter().flat_map(|&(t0, t1)| [t0, t1]).chain([0.0, len]).collect();
            cuts.sort_by(f64::total_cmp);
            cuts.dedup_by(|x, y| (*x - *y).abs() < 1e-6);
            for w in cuts.windows(2) {
                let mid = (w[0] + w[1]) / 2.0;
                if shared.iter().any(|&(t0, t1)| t0 < mid && mid < t1) {
                    seams.push(outline.len());
                }
                outline.push(if w[0] == 0.0 { a } else { a + dir * w[0] });
            }
        }
        f.outline = outline;
        f.seams = seams;
    }
}

/// The edges of a roof's surface: where two faces in different planes meet (a ridge, a hip or a
/// valley), and the free edges of faces (an eave where it is the plane's low edge, else a rake).
pub fn edges(faces: &[Face]) -> Vec<Edge> {
    let lift = |f: &Face, p: Vec2| Vec3::new(p.x, p.y, f.plane.over(p));
    let mut out = vec![];
    for (i, f) in faces.iter().enumerate() {
        let n = f.outline.len();
        let mid = f.outline.iter().fold(Vec2::new(0.0, 0.0), |m, p| m + *p) / n as f64;
        for k in 0..n {
            let (a, b) = (f.outline[k], f.outline[(k + 1) % n]);
            let len = a.distance(b);
            if len < 1e-6 {
                continue;
            }
            let dir = (b - a) / len;
            // What of this side is shared with another face: as stretches along it.
            let mut shared: Vec<(f64, f64, usize)> = vec![];
            for (j, g) in faces.iter().enumerate() {
                if j == i {
                    continue;
                }
                let m = g.outline.len();
                for q in 0..m {
                    let (c, d) = (g.outline[q], g.outline[(q + 1) % m]);
                    // (On one line, and at one height there: the two faces meet, they do not pass over each other.)
                    if (c - a).cross(dir).abs() > 1e-6 || (d - a).cross(dir).abs() > 1e-6 {
                        continue;
                    }
                    let (t0, t1) = ((c - a).dot(dir).min((d - a).dot(dir)).max(0.0), (c - a).dot(dir).max((d - a).dot(dir)).min(len));
                    if t1 - t0 < 1e-6 || (f.plane.over(a + dir * t0) - g.plane.over(a + dir * t0)).abs() > 1e-5 || (f.plane.over(a + dir * t1) - g.plane.over(a + dir * t1)).abs() > 1e-5 {
                        continue;
                    }
                    shared.push((t0, t1, j));
                }
            }
            shared.sort_by(|x, y| x.0.total_cmp(&y.0));
            let mut at = 0.0;
            let free = |t0: f64, t1: f64, out: &mut Vec<Edge>| {
                if t1 - t0 > 1e-6 {
                    let (p, q) = (lift(f, a + dir * t0), lift(f, a + dir * t1));
                    // (An eave: level, or nearly, and the plane rises from it into the face.)
                    let rises = f.plane.slope.dot(mid - (a + dir * ((t0 + t1) / 2.0))) > 0.0;
                    let level = (p.z - q.z).abs() < 0.02 * (t1 - t0);
                    out.push(Edge { kind: if level && rises { EdgeKind::Eave } else { EdgeKind::Rake }, a: p, b: q });
                }
            };
            for &(t0, t1, j) in &shared {
                free(at, t0, &mut out);
                at = at.max(t1);
                // (Each meeting once: from the face that comes first. Faces of one plane only abut.)
                if j < i || same_plane(f, &faces[j]) {
                    continue;
                }
                let (p, q) = (lift(f, a + dir * t0), lift(f, a + dir * t1));
                // Just inside this face, the other's plane is above this one (the roof falls away: a ridge or a
                // hip) or below it (it rises either side: a valley).
                let inside = a + dir * ((t0 + t1) / 2.0) + (mid - (a + dir * ((t0 + t1) / 2.0))).normalized() * 1e-3;
                let falls = faces[j].plane.over(inside) > f.plane.over(inside);
                let kind = if !falls {
                    EdgeKind::Valley
                } else if (p.z - q.z).abs() < 1e-6 {
                    EdgeKind::Ridge
                } else {
                    EdgeKind::Hip
                };
                out.push(Edge { kind, a: p, b: q });
            }
            free(at, len, &mut out);
        }
    }
    out
}

/// How much of each kind of edge there is (m).
pub fn lengths(edges: &[Edge]) -> std::collections::BTreeMap<EdgeKind, f64> {
    let mut out = std::collections::BTreeMap::new();
    for e in edges {
        *out.entry(e.kind).or_insert(0.0) += e.a.distance(e.b);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(name: &str, x0: f64, y0: f64, x1: f64, y1: f64, eaves: [bool; 4], pitch: f64, height: f64) -> RoofPart {
        let e = Some(Eave { height: (height, height), pitch, overhang: 0.0 });
        RoofPart { name: name.into(), outline: vec![Vec2::new(x0, y0), Vec2::new(x1, y0), Vec2::new(x1, y1), Vec2::new(x0, y1)], edges: eaves.map(|on| if on { e } else { None }).to_vec(), rake: 0.0 }
    }
    fn total(faces: &[Face]) -> f64 {
        faces.iter().map(|f| area(&f.outline)).sum()
    }
    fn of(edges: &[Edge], kind: EdgeKind) -> f64 {
        lengths(edges).get(&kind).copied().unwrap_or(0.0)
    }

    #[test]
    fn a_gable_and_a_hip_are_one_part_with_two_eaves_or_four() {
        // 8 by 6, eaves along the long sides: two planes, a ridge down the middle 1.5 up (3 at 6 in 12), a rake up each end.
        let gable = [rect("gable", 0.0, 0.0, 8.0, 6.0, [true, false, true, false], 0.5, 2.4)];
        let f = faces(&gable).unwrap();
        assert_eq!(f.len(), 2);
        assert!((total(&f) - 48.0).abs() < 1e-9);
        let e = edges(&f);
        assert!((of(&e, EdgeKind::Ridge) - 8.0).abs() < 1e-9 && (of(&e, EdgeKind::Eave) - 16.0).abs() < 1e-9 && of(&e, EdgeKind::Hip) == 0.0 && of(&e, EdgeKind::Valley) == 0.0);
        let ridge = e.iter().find(|x| x.kind == EdgeKind::Ridge).unwrap();
        assert!((ridge.a.z - 3.9).abs() < 1e-9 && (ridge.a.y - 3.0).abs() < 1e-9);
        // (A rake's length is up the slope: 3 on plan and 1.5 up, four times.)
        assert!((of(&e, EdgeKind::Rake) - 4.0 * (9.0f64 + 2.25).sqrt()).abs() < 1e-9);
        // The same with every edge an eave: four planes, a ridge 2 long (8 less 6), four hips.
        let hip = [rect("hip", 0.0, 0.0, 8.0, 6.0, [true; 4], 0.5, 2.4)];
        let f = faces(&hip).unwrap();
        assert_eq!(f.len(), 4);
        assert!((total(&f) - 48.0).abs() < 1e-9);
        let e = edges(&f);
        assert!((of(&e, EdgeKind::Ridge) - 2.0).abs() < 1e-9 && (of(&e, EdgeKind::Eave) - 28.0).abs() < 1e-9 && of(&e, EdgeKind::Rake) == 0.0);
        assert!((of(&e, EdgeKind::Hip) - 4.0 * (9.0f64 + 9.0 + 2.25).sqrt()).abs() < 1e-9, "{}", of(&e, EdgeKind::Hip));
        // One eave: a shed, one plane, no ridge.
        let shed = [rect("shed", 0.0, 0.0, 8.0, 6.0, [true, false, false, false], 0.25, 2.4)];
        let f = faces(&shed).unwrap();
        assert!(f.len() == 1 && of(&edges(&f), EdgeKind::Ridge) == 0.0 && (f[0].plane.over(Vec2::new(4.0, 6.0)) - 3.9).abs() < 1e-9);
    }

    #[test]
    fn where_parts_overlap_the_higher_is_seen_and_they_meet_in_valleys() {
        // A main roof 10 by 6, and a wing 4 wide run out of its south side, as high: a cross gable. Its ridge runs
        // into the main roof's south slope, and two valleys run down from where it meets it.
        // (Of two parts as high as each other the first is seen, by a ten-thousandth of a millimetre: so to 1e-6.)
        let main = rect("main", 0.0, 0.0, 10.0, 6.0, [true, false, true, false], 0.5, 2.4);
        let wing = rect("wing", 3.0, -4.0, 7.0, 3.0, [false, true, false, true], 0.5, 2.4);
        let f = faces(&[main.clone(), wing.clone()]).unwrap();
        let e = edges(&f);
        // (The wing's ridge is 1 up, where the main roof's south slope is 1 up at y = 2: it runs from y = -4 to 2.)
        let ridges: Vec<&Edge> = e.iter().filter(|x| x.kind == EdgeKind::Ridge).collect();
        assert!((of(&e, EdgeKind::Ridge) - 16.0).abs() < 1e-6, "{ridges:?}");
        // Two valleys, each from the eave corner (3 or 7, 0) up to (5, 2), 1 up.
        let valleys: Vec<&Edge> = e.iter().filter(|x| x.kind == EdgeKind::Valley).collect();
        assert!((of(&e, EdgeKind::Valley) - 2.0 * 3.0).abs() < 1e-6 && valleys.iter().all(|v| (v.a.z.max(v.b.z) - 3.4).abs() < 1e-6 && (v.a.z.min(v.b.z) - 2.4).abs() < 1e-6), "{valleys:?}");
        assert_eq!(of(&e, EdgeKind::Hip), 0.0);
        // A plane cut into pieces (the main roof's south slope, about the wing) has seams where they abut, and
        // every seam is one of two pieces'; a plane in one piece has none.
        let seams: usize = f.iter().map(|x| x.seams.len()).sum();
        assert!(seams > 0 && seams % 2 == 0 && f.iter().filter(|x| x.part == 0 && x.edge == 2).all(|x| x.seams.is_empty()), "{seams}");
        // Nothing is counted twice: the roof covers the main part and what of the wing is beyond it.
        assert!((total(&f) - (60.0 + 16.0)).abs() < 1e-6, "{}", total(&f));
        // A dormer: a small gable high on the south slope, its eaves above the main eave. Seen where it is higher.
        let dormer = rect("dormer", 4.0, 0.5, 6.0, 3.0, [false, true, false, true], 0.5, 3.0);
        let f = faces(&[main, dormer]).unwrap();
        let e = edges(&f);
        assert!(of(&e, EdgeKind::Valley) > 1.0 && f.iter().any(|x| x.part == 1));
    }

    #[test]
    fn overhangs_and_eaves_that_are_not_level() {
        // A gable with a 0.3 overhang at its eaves and 0.2 at its rakes: the faces reach out by those, and the eave
        // is lower by the pitch.
        let e = Some(Eave { height: (2.4, 2.4), pitch: 0.5, overhang: 0.3 });
        let part = RoofPart { name: "g".into(), outline: vec![Vec2::new(0.0, 0.0), Vec2::new(8.0, 0.0), Vec2::new(8.0, 6.0), Vec2::new(0.0, 6.0)], edges: vec![e, None, e, None], rake: 0.2 };
        let f = faces(&[part]).unwrap();
        assert!((total(&f) - 8.4 * 6.6).abs() < 1e-9);
        let lows: Vec<f64> = edges(&f).iter().filter(|x| x.kind == EdgeKind::Eave).map(|x| x.a.z).collect();
        assert!(lows.len() == 2 && lows.iter().all(|z| (z - 2.25).abs() < 1e-9), "{lows:?}");
        // Drawn clockwise, it is the same roof.
        let part = RoofPart { name: "g".into(), outline: vec![Vec2::new(0.0, 6.0), Vec2::new(8.0, 6.0), Vec2::new(8.0, 0.0), Vec2::new(0.0, 0.0)], edges: vec![e, None, e, None], rake: 0.2 };
        assert!((total(&faces(&[part]).unwrap()) - 8.4 * 6.6).abs() < 1e-9);
        // An outline that is not convex, and a part with no eave, are refused.
        let l = RoofPart { name: "l".into(), outline: [(0.0, 0.0), (4.0, 0.0), (4.0, 2.0), (2.0, 2.0), (2.0, 4.0), (0.0, 4.0)].map(|(x, y)| Vec2::new(x, y)).to_vec(), edges: vec![e; 6], rake: 0.0 };
        assert!(faces(&[l]).unwrap_err().contains("convex"));
        assert!(faces(&[rect("none", 0.0, 0.0, 1.0, 1.0, [false; 4], 0.5, 0.0)]).unwrap_err().contains("eave"));
    }
}
