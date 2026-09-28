//! Topology-first model IR for CAD-as-code.
//!
//! A [`Model`] is a graph of [`Node`]s and [`Member`]s (physical pieces whose
//! paths pass through nodes), annotated with sections, materials, connections,
//! groups and analysis inputs. See `DESIGN.md` at the repository root.

pub mod connection;
pub mod frames;
pub mod ids;
pub mod loads;
pub mod material;
pub mod math;
pub mod model;
pub mod refs;
pub mod section;
pub mod sheets;
pub mod surfaces;
pub mod topology;
pub mod units;
pub mod validate;

pub use connection::*;
pub use frames::{axis_direction, nearest_axis, WORLD_DIRECTIONS};
pub use ids::*;
pub use loads::*;
pub use material::Material;
pub use math::*;
pub use model::*;
pub use refs::{resolve_direction, resolve_direction_in, DirRef, Feature, PlaneRef};
pub use section::*;
pub use sheets::*;
pub use surfaces::{Layer, Strips, Surface, SurfaceRegion};
pub use topology::*;
pub use units::UnitSystem;
pub use validate::{validate, Issue, Severity};
