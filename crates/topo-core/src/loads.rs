//! Analysis inputs that live in the model IR so it is complete and portable.
//! Consumed by `topo-analysis` (Milestone 2).

use crate::ids::{GroupId, LoadCaseId, MemberId, NodeId};
use crate::math::Vec3;
use serde::{Deserialize, Serialize};

/// Restrained degrees of freedom at a node, in global axes.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Restraint {
    pub tx: bool,
    pub ty: bool,
    pub tz: bool,
    pub rx: bool,
    pub ry: bool,
    pub rz: bool,
}

impl Restraint {
    pub const FIXED: Restraint = Restraint { tx: true, ty: true, tz: true, rx: true, ry: true, rz: true };
    pub const PINNED: Restraint = Restraint { tx: true, ty: true, tz: true, rx: false, ry: false, rz: false };
    /// Vertical bearing only (roller).
    pub const BEARING_Z: Restraint = Restraint { tx: false, ty: false, tz: true, rx: false, ry: false, rz: false };
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Support {
    pub node: NodeId,
    pub restraint: Restraint,
    /// e.g. `Anchored to concrete foundation`.
    pub description: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LoadKind {
    Dead,
    Live,
    RoofLive,
    Snow,
    Wind,
    Seismic,
    Other,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LoadCase {
    pub id: LoadCaseId,
    pub name: String,
    pub kind: LoadKind,
    /// Include member self-weight in this case (normally only for Dead).
    pub self_weight: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Load {
    /// Concentrated force (N) and moment (N·m) at a node, global axes.
    Node { case: LoadCaseId, node: NodeId, force: Vec3, moment: Vec3 },
    /// Uniform line load along a member (N/m), global axes.
    /// Over `range` (distances along the member from its first node, m) if
    /// given, else the whole member.
    MemberUniform {
        case: LoadCaseId,
        member: MemberId,
        w: Vec3,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        range: Option<(f64, f64)>,
    },
    /// Uniform pressure (Pa) on a group's surface (floor, roof, wall) acting in
    /// `direction`; distributed to the group's spanning members by tributary width.
    Area {
        case: LoadCaseId,
        group: GroupId,
        pressure: f64,
        direction: Vec3,
        /// Whether `pressure` is per unit plan area (snow, roof live) or per
        /// unit surface area (dead load of sloped roofing).
        #[serde(default)]
        basis: AreaBasis,
        /// Where it came from, for reports (e.g. `Roof dead: shingles 2.0 + …`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AreaBasis {
    /// Per unit horizontal projected area.
    #[default]
    Plan,
    /// Per unit area along the (sloped) surface.
    Surface,
}
