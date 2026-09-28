//! Surfaces: the sheathing and finish layers over a framing group (a roof's
//! sloped planes, its flat part). They carry the build-up for drawings,
//! schedules and the 3D view; their weight reaches the frame through the
//! group's dead loads, not from here.

use crate::ids::GroupId;
use serde::{Deserialize, Serialize};

/// Where on the group the surface lies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceRegion {
    /// The sloped roof planes, over the truss top chords.
    Slope,
    /// The flat roof over bottom chords beyond the heels.
    Flat,
}

/// Parallel strips making up a layer (boards with gaps, standing-seam ribs).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Strips {
    /// Strip width and centre spacing (m).
    pub width: f64,
    pub spacing: f64,
    /// `slope` (down the roof) or `run` (along the ridge).
    pub along: String,
    /// Whether a continuous sheet lies under the strips (a standing-seam
    /// panel's pan); otherwise only the strips (spaced boards).
    #[serde(default)]
    pub sheet: bool,
}

/// One layer of a build-up.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub name: String,
    /// Weight, psf of surface.
    pub psf: f64,
    /// Thickness (m); for drawing.
    pub thickness: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strips: Option<Strips>,
    #[serde(default = "yes")]
    pub verified: bool,
}

fn yes() -> bool {
    true
}

/// Layers over part of a framing group, listed top (weather side) first.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Surface {
    pub name: String,
    pub group: GroupId,
    pub region: SurfaceRegion,
    pub layers: Vec<Layer>,
}

impl Surface {
    pub fn thickness(&self) -> f64 {
        self.layers.iter().map(|l| l.thickness).sum()
    }
    pub fn psf(&self) -> f64 {
        self.layers.iter().map(|l| l.psf).sum()
    }
}
