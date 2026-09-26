//! Wood light-frame domain for topo-cad.

pub mod examples;
pub mod fastening;
pub mod floor;
pub mod lumber;
pub mod perimeter;
pub mod truss;
pub mod wall;

pub use floor::{Floor, FloorParts};
pub use perimeter::{Perimeter, PerimeterParts, Side};
pub use truss::{RoofParts, ShapeMember, ShapePoint, ShapeRole, StandardTruss, Truss, TrussParts, TrussRoof, TrussShape};
pub use wall::{Justify, Opening, OpeningKind, Wall, WallParts};
