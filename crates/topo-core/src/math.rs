//! Small, dependency-free linear algebra for 2D/3D geometry.

use serde::{Deserialize, Serialize};
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

/// Length tolerance (metres) used for coincidence tests: 0.5 mm.
pub const LEN_TOL: f64 = 5e-4;
/// Cosine tolerance used for parallelism tests.
pub const PARALLEL_TOL: f64 = 1e-6;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

pub const fn v3(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3 { x, y, z }
}

impl Vec3 {
    pub const ZERO: Vec3 = v3(0.0, 0.0, 0.0);
    pub const X: Vec3 = v3(1.0, 0.0, 0.0);
    pub const Y: Vec3 = v3(0.0, 1.0, 0.0);
    pub const Z: Vec3 = v3(0.0, 0.0, 1.0);

    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        v3(x, y, z)
    }
    pub fn dot(self, o: Vec3) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    pub fn cross(self, o: Vec3) -> Vec3 {
        v3(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }
    pub fn norm2(self) -> f64 {
        self.dot(self)
    }
    pub fn norm(self) -> f64 {
        self.norm2().sqrt()
    }
    pub fn distance(self, o: Vec3) -> f64 {
        (self - o).norm()
    }
    pub fn try_normalized(self) -> Option<Vec3> {
        let n = self.norm();
        if n > 1e-12 {
            Some(self / n)
        } else {
            None
        }
    }
    /// Unit vector; panics on (near) zero vectors, which indicate a modelling bug.
    pub fn normalized(self) -> Vec3 {
        self.try_normalized().expect("normalizing zero-length vector")
    }
    pub fn lerp(self, o: Vec3, t: f64) -> Vec3 {
        self + (o - self) * t
    }
    /// Component of `self` perpendicular to unit vector `axis`.
    pub fn reject(self, axis: Vec3) -> Vec3 {
        self - axis * self.dot(axis)
    }
    /// Some unit vector perpendicular to `self` (which need not be unit).
    pub fn any_perpendicular(self) -> Vec3 {
        let a = if self.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
        self.cross(a).normalized()
    }
    pub fn is_parallel(self, o: Vec3) -> bool {
        let (a, b) = (self.normalized(), o.normalized());
        1.0 - a.dot(b).abs() < PARALLEL_TOL
    }
    pub fn min(self, o: Vec3) -> Vec3 {
        v3(self.x.min(o.x), self.y.min(o.y), self.z.min(o.z))
    }
    pub fn max(self, o: Vec3) -> Vec3 {
        v3(self.x.max(o.x), self.y.max(o.y), self.z.max(o.z))
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        v3(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl AddAssign for Vec3 {
    fn add_assign(&mut self, o: Vec3) {
        *self = *self + o;
    }
}
impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        v3(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl SubAssign for Vec3 {
    fn sub_assign(&mut self, o: Vec3) {
        *self = *self - o;
    }
}
impl Mul<f64> for Vec3 {
    type Output = Vec3;
    fn mul(self, s: f64) -> Vec3 {
        v3(self.x * s, self.y * s, self.z * s)
    }
}
impl Div<f64> for Vec3 {
    type Output = Vec3;
    fn div(self, s: f64) -> Vec3 {
        v3(self.x / s, self.y / s, self.z / s)
    }
}
impl Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        v3(-self.x, -self.y, -self.z)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

pub const fn v2(x: f64, y: f64) -> Vec2 {
    Vec2 { x, y }
}

impl Vec2 {
    pub const ZERO: Vec2 = v2(0.0, 0.0);
    pub const fn new(x: f64, y: f64) -> Self {
        v2(x, y)
    }
    pub fn dot(self, o: Vec2) -> f64 {
        self.x * o.x + self.y * o.y
    }
    /// z-component of the 3D cross product.
    pub fn cross(self, o: Vec2) -> f64 {
        self.x * o.y - self.y * o.x
    }
    pub fn norm(self) -> f64 {
        self.dot(self).sqrt()
    }
    pub fn normalized(self) -> Vec2 {
        self / self.norm()
    }
    /// Rotated +90°.
    pub fn perp(self) -> Vec2 {
        v2(-self.y, self.x)
    }
    pub fn lerp(self, o: Vec2, t: f64) -> Vec2 {
        self + (o - self) * t
    }
    pub fn distance(self, o: Vec2) -> f64 {
        (self - o).norm()
    }
    pub fn min(self, o: Vec2) -> Vec2 {
        v2(self.x.min(o.x), self.y.min(o.y))
    }
    pub fn max(self, o: Vec2) -> Vec2 {
        v2(self.x.max(o.x), self.y.max(o.y))
    }
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        v2(self.x + o.x, self.y + o.y)
    }
}
impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        v2(self.x - o.x, self.y - o.y)
    }
}
impl Mul<f64> for Vec2 {
    type Output = Vec2;
    fn mul(self, s: f64) -> Vec2 {
        v2(self.x * s, self.y * s)
    }
}
impl Div<f64> for Vec2 {
    type Output = Vec2;
    fn div(self, s: f64) -> Vec2 {
        v2(self.x / s, self.y / s)
    }
}
impl Neg for Vec2 {
    type Output = Vec2;
    fn neg(self) -> Vec2 {
        v2(-self.x, -self.y)
    }
}

/// Orthonormal right-handed frame. `x`, `y`, `z` are unit axes in world coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub origin: Vec3,
    pub x: Vec3,
    pub y: Vec3,
    pub z: Vec3,
}

impl Default for Frame {
    fn default() -> Self {
        Frame::WORLD
    }
}

impl Frame {
    pub const WORLD: Frame = Frame { origin: Vec3::ZERO, x: Vec3::X, y: Vec3::Y, z: Vec3::Z };

    /// Frame with the given x axis and a z axis as close as possible to `z_hint`.
    pub fn from_x_z(origin: Vec3, x: Vec3, z_hint: Vec3) -> Frame {
        let x = x.normalized();
        let z = z_hint.reject(x).try_normalized().unwrap_or_else(|| x.any_perpendicular());
        let y = z.cross(x);
        Frame { origin, x, y, z }
    }
    pub fn to_world(&self, p: Vec3) -> Vec3 {
        self.origin + self.x * p.x + self.y * p.y + self.z * p.z
    }
    pub fn dir_to_world(&self, d: Vec3) -> Vec3 {
        self.x * d.x + self.y * d.y + self.z * d.z
    }
    pub fn to_local(&self, p: Vec3) -> Vec3 {
        self.dir_to_local(p - self.origin)
    }
    pub fn dir_to_local(&self, d: Vec3) -> Vec3 {
        v3(d.dot(self.x), d.dot(self.y), d.dot(self.z))
    }
}

/// Oriented plane. By convention in this crate the normal points *away* from
/// the material that is kept (signed distance ≤ 0 is inside).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Plane {
    pub point: Vec3,
    pub normal: Vec3,
}

impl Plane {
    pub fn new(point: Vec3, normal: Vec3) -> Plane {
        Plane { point, normal: normal.normalized() }
    }
    pub fn signed_distance(&self, p: Vec3) -> f64 {
        (p - self.point).dot(self.normal)
    }
    /// Parameter `t` where `origin + dir * t` meets the plane, if not parallel.
    pub fn intersect_line(&self, origin: Vec3, dir: Vec3) -> Option<f64> {
        let den = self.normal.dot(dir);
        if den.abs() < 1e-12 {
            None
        } else {
            Some((self.point - origin).dot(self.normal) / den)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BBox3 {
    pub min: Vec3,
    pub max: Vec3,
}

impl BBox3 {
    pub const EMPTY: BBox3 = BBox3 {
        min: v3(f64::INFINITY, f64::INFINITY, f64::INFINITY),
        max: v3(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY),
    };
    pub fn from_points<I: IntoIterator<Item = Vec3>>(pts: I) -> BBox3 {
        pts.into_iter().fold(BBox3::EMPTY, |b, p| b.including(p))
    }
    pub fn including(self, p: Vec3) -> BBox3 {
        BBox3 { min: self.min.min(p), max: self.max.max(p) }
    }
    pub fn union(self, o: BBox3) -> BBox3 {
        BBox3 { min: self.min.min(o.min), max: self.max.max(o.max) }
    }
    pub fn overlaps(&self, o: &BBox3, tol: f64) -> bool {
        self.min.x <= o.max.x + tol
            && o.min.x <= self.max.x + tol
            && self.min.y <= o.max.y + tol
            && o.min.y <= self.max.y + tol
            && self.min.z <= o.max.z + tol
            && o.min.z <= self.max.z + tol
    }
    pub fn is_empty(&self) -> bool {
        self.min.x > self.max.x
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BBox2 {
    pub min: Vec2,
    pub max: Vec2,
}

impl BBox2 {
    pub const EMPTY: BBox2 = BBox2 {
        min: v2(f64::INFINITY, f64::INFINITY),
        max: v2(f64::NEG_INFINITY, f64::NEG_INFINITY),
    };
    pub fn from_points<I: IntoIterator<Item = Vec2>>(pts: I) -> BBox2 {
        pts.into_iter().fold(BBox2::EMPTY, |b, p| b.including(p))
    }
    pub fn including(self, p: Vec2) -> BBox2 {
        BBox2 { min: self.min.min(p), max: self.max.max(p) }
    }
    pub fn union(self, o: BBox2) -> BBox2 {
        BBox2 { min: self.min.min(o.min), max: self.max.max(o.max) }
    }
    pub fn overlaps(&self, o: &BBox2, tol: f64) -> bool {
        self.min.x <= o.max.x + tol
            && o.min.x <= self.max.x + tol
            && self.min.y <= o.max.y + tol
            && o.min.y <= self.max.y + tol
    }
    pub fn width(&self) -> f64 {
        self.max.x - self.min.x
    }
    pub fn height(&self) -> f64 {
        self.max.y - self.min.y
    }
    pub fn center(&self) -> Vec2 {
        (self.min + self.max) * 0.5
    }
    pub fn is_empty(&self) -> bool {
        self.min.x > self.max.x
    }
    pub fn expanded(&self, d: f64) -> BBox2 {
        BBox2 { min: self.min - v2(d, d), max: self.max + v2(d, d) }
    }
}

/// Signed area of a polygon (positive if counter-clockwise).
pub fn polygon_area(pts: &[Vec2]) -> f64 {
    let n = pts.len();
    (0..n).map(|i| pts[i].cross(pts[(i + 1) % n])).sum::<f64>() * 0.5
}

/// Convex hull (Andrew's monotone chain), counter-clockwise, no collinear points.
pub fn convex_hull(points: &[Vec2]) -> Vec<Vec2> {
    let mut pts: Vec<Vec2> = points.to_vec();
    pts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    pts.dedup_by(|a, b| a.distance(*b) < 1e-12);
    if pts.len() < 3 {
        return pts;
    }
    let mut hull: Vec<Vec2> = Vec::with_capacity(pts.len() * 2);
    for pass in 0..2 {
        let start = hull.len();
        let iter: Box<dyn Iterator<Item = &Vec2>> =
            if pass == 0 { Box::new(pts.iter()) } else { Box::new(pts.iter().rev()) };
        for &p in iter {
            while hull.len() >= start + 2 {
                let (a, b) = (hull[hull.len() - 2], hull[hull.len() - 1]);
                if (b - a).cross(p - a) <= 1e-15 {
                    hull.pop();
                } else {
                    break;
                }
            }
            hull.push(p);
        }
        hull.pop();
    }
    hull
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_is_right_handed() {
        let f = Frame::from_x_z(Vec3::ZERO, v3(1.0, 1.0, 0.0), Vec3::Z);
        assert!((f.x.cross(f.y) - f.z).norm() < 1e-12);
        let p = v3(0.3, -2.0, 5.0);
        assert!((f.to_local(f.to_world(p)) - p).norm() < 1e-12);
    }

    #[test]
    fn hull_of_square_with_interior_point() {
        let pts = [v2(0., 0.), v2(1., 0.), v2(1., 1.), v2(0., 1.), v2(0.5, 0.5), v2(0.5, 0.)];
        let h = convex_hull(&pts);
        assert_eq!(h.len(), 4);
        assert!((polygon_area(&h) - 1.0).abs() < 1e-12);
    }
}
