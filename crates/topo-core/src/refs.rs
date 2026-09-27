//! References to physical features and directions, by stable names: member
//! faces (`T2/bottom_chord.F-H1` `+z`), their intersections, and directions
//! named in any frame. Used by field measurements and drawing specs; they are
//! evaluated against member solids in `topo-geom`.

use crate::math::Vec3;
use crate::model::Model;
use serde::{Deserialize, Serialize};

/// One plane of a member solid.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlaneRef {
    /// A face by canonical name: `+x`/`-x` (ends), `±y` (wide faces), `±z`
    /// (edges). Deprecated aliases: `end`/`start`, `back`/`front`, `top`/`bottom`.
    Face { member: String, side: String },
    /// Mid-plane across the member's `z` (depth) or `y` (thickness), e.g. a
    /// wall centreline. Aliases: `depth`, `width`.
    Mid { member: String, axis: String },
    /// The face whose outward normal points closest to `direction`.
    Facing { member: String, direction: DirRef },
}

/// A direction: a name (`north`, `up`, `inside`, `+x`…) or a vector, in the
/// frame of a group or member (`in`; default the world).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DirRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vector: Option<[f64; 3]>,
    #[serde(default, rename = "in", skip_serializing_if = "Option::is_none")]
    pub frame: Option<String>,
}

/// World unit vector of a direction reference.
pub fn resolve_direction(model: &Model, d: &DirRef) -> Result<Vec3, String> {
    resolve_direction_in(model, d, None)
}

/// As [`resolve_direction`], with a group to use when the reference names
/// no frame (e.g. the group a view draws).
pub fn resolve_direction_in(model: &Model, d: &DirRef, default: Option<crate::ids::GroupId>) -> Result<Vec3, String> {
    // A named frame is a group, or failing that a member (its canonical frame).
    let (group, member_frame) = match &d.frame {
        None => (default, None),
        Some(sel) => match model.find_group(sel) {
            Ok(g) => (Some(g), None),
            Err(ge) => match model.find_member(sel) {
                Ok(m) => (model.member(m).group, Some(model.member_axes(m).frame())),
                Err(_) => return Err(ge),
            },
        },
    };
    let frame = member_frame.or(group.map(|g| model.group(g).frame));
    match (&d.name, d.vector) {
        (Some(n), _) => match (member_frame, crate::frames::axis_direction(n)) {
            (Some(f), Some(v)) => Ok(f.dir_to_world(v)),
            _ => model.direction(n, group),
        },
        (None, Some(v)) => {
            let v = Vec3::new(v[0], v[1], v[2]);
            Ok(frame.map(|f| f.dir_to_world(v)).unwrap_or(v).try_normalized().ok_or("zero direction vector")?)
        }
        (None, None) => Err("a direction needs a name or a vector".into()),
    }
}

/// Intersection of planes: 1 → a plane, 2 → a line, 3 → a point.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Feature {
    pub planes: Vec<PlaneRef>,
}

