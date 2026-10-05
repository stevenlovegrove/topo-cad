//! Build-ups: the layers a surface of the model is made of (a roof's covering, a floor's
//! deck), top first. They are what its sheets are drawn from and what a schedule lists; their
//! weight is a dead load on what carries the surface.

use crate::ids::GroupId;
use serde::{Deserialize, Serialize};

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

/// The layers of something (a roof, a part of one, a floor's deck), top (weather side) first.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Buildup {
    pub name: String,
    pub group: GroupId,
    pub layers: Vec<Layer>,
}

impl Buildup {
    pub fn thickness(&self) -> f64 {
        self.layers.iter().map(|l| l.thickness).sum()
    }
    pub fn psf(&self) -> f64 {
        self.layers.iter().map(|l| l.psf).sum()
    }
}

impl Layer {
    /// How thick it is drawn, as a sheet: a ribbed sheet (standing seam) as the sheet without its ribs.
    pub fn sheet(&self) -> f64 {
        if self.strips.as_ref().is_some_and(|s| s.sheet) { self.thickness.min(0.001) } else { self.thickness }
    }
}
