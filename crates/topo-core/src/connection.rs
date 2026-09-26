//! Connections: how members at a junction are fastened. A connection has a
//! mechanical meaning (fasteners/hardware → capacity, checked by a design code)
//! and an analytical meaning (end fixity → element releases).

use crate::ids::ConnectionId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Fastener {
    /// e.g. `16d common` with shank diameter and length (m).
    Nail { designation: String, diameter: f64, length: f64 },
    Screw { designation: String, diameter: f64, length: f64 },
    Bolt { designation: String, diameter: f64, length: f64 },
    Other { designation: String },
}

impl Fastener {
    pub fn designation(&self) -> &str {
        match self {
            Fastener::Nail { designation, .. }
            | Fastener::Screw { designation, .. }
            | Fastener::Bolt { designation, .. }
            | Fastener::Other { designation } => designation,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FastenMethod {
    EndNail,
    ToeNail,
    FaceNail,
    Through,
    Other,
}

impl FastenMethod {
    pub fn label(self) -> &'static str {
        match self {
            FastenMethod::EndNail => "end nail",
            FastenMethod::ToeNail => "toe nail",
            FastenMethod::FaceNail => "face nail",
            FastenMethod::Through => "through",
            FastenMethod::Other => "",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FastenerGroup {
    pub fastener: Fastener,
    pub count: u32,
    pub method: FastenMethod,
    /// Optional spacing for distributed fastening (m), e.g. 16d @ 24" o.c.
    pub spacing: Option<f64>,
}

/// Proprietary or generic hardware (hangers, straps, corner brackets …).
/// Capacities come from the manufacturer; the model stores the reference only.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hardware {
    pub kind: String,
    pub model: Option<String>,
    pub count: u32,
}

/// Rotational/translational fixity of a member end, in the member's local axes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Fixity {
    /// Moments released about both bending axes; torsion restrained by the element.
    #[default]
    Pinned,
    /// Fully continuous.
    Rigid,
    /// Rotational springs about (u, v) axes, N·m/rad.
    Semi { k_u: f64, k_v: f64 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Connection {
    pub id: ConnectionId,
    /// Short description for schedules, e.g. `Stud to plate`.
    pub name: String,
    pub fasteners: Vec<FastenerGroup>,
    pub hardware: Vec<Hardware>,
    /// Analytical fixity of the connected member end.
    pub fixity: Fixity,
    /// Prescriptive reference, e.g. `IRC Table R602.3(1)`.
    pub reference: Option<String>,
    pub notes: Option<String>,
}

impl Connection {
    /// One-line description, e.g. `(2) 16d common end nail`.
    pub fn describe(&self) -> String {
        let mut parts: Vec<String> = self
            .fasteners
            .iter()
            .map(|g| {
                let mut s = format!("({}) {}", g.count, g.fastener.designation());
                let m = g.method.label();
                if !m.is_empty() {
                    s.push(' ');
                    s.push_str(m);
                }
                if let Some(sp) = g.spacing {
                    s.push_str(&format!(" @ {}", crate::units::fmt_inches(sp)));
                }
                s
            })
            .collect();
        parts.extend(self.hardware.iter().map(|h| match &h.model {
            Some(m) => format!("({}) {} {}", h.count, h.kind, m),
            None => format!("({}) {}", h.count, h.kind),
        }));
        parts.join(" + ")
    }
}
