//! Drawing intent: which sheets a model's drawing set has, in order, and the
//! views on them. Scripts write these (`sheet(...)`, `plan(...)`, …); the
//! drawing crate turns them into sheets. A model without a list gets the
//! standard set.

use crate::refs::{DirRef, Feature};
use serde::{Deserialize, Serialize};

/// The generated sheet sets, in their default order.
pub const STANDARD_SHEETS: [&str; 6] = ["cover", "plans", "trusses", "walls", "analytical", "schedules"];

/// One entry of a drawing set (a sheet tab, or a generated run of them).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SheetSpec {
    /// A generated set, one of [`STANDARD_SHEETS`]: `cover` (notes, index,
    /// 3D view), `plans` (framing plans), `trusses` (a typical truss per
    /// roof), `walls` (wall elevations), `analytical`, `schedules`.
    Standard { which: String },
    /// A sheet of views, packed in order (continued on further sheets if
    /// they do not fit).
    Sheet { number: String, title: String, views: Vec<ViewSpec> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewKind {
    /// Looking down (or `from` any direction), hidden lines removed.
    Plan,
    /// Looking horizontally from `from`.
    Elevation,
    /// Axonometric from `from` (default from the south-west, above).
    Iso,
    /// An enlarged, clipped region around `at` (a feature) of radius `radius`.
    Detail,
    /// Centre-line model with junction symbols.
    Analytical,
    /// Members filled by their highest demand/capacity ratio, labelled.
    Utilization,
    /// A table: `members`, `connections`, `junctions`, `checks` (governing
    /// check per member), `reactions` (foundation reactions by load case),
    /// or the title of a table the tool supplies (e.g. `Field measurements`).
    Schedule,
    /// A block of text lines.
    Notes,
}

/// The standard sets as a sheet list.
pub fn standard_sheets() -> Vec<SheetSpec> {
    STANDARD_SHEETS.iter().map(|w| SheetSpec::Standard { which: w.to_string() }).collect()
}

/// Concatenates two sheet lists (a missing list stands for the standard
/// set); each standard set appears once, where first listed.
pub fn merge_sheets(a: Option<Vec<SheetSpec>>, b: Option<Vec<SheetSpec>>) -> Option<Vec<SheetSpec>> {
    if a.is_none() && b.is_none() {
        return None;
    }
    let mut all = a.unwrap_or_else(standard_sheets);
    all.extend(b.unwrap_or_else(standard_sheets));
    let mut seen: Vec<String> = vec![];
    all.retain(|s| match s {
        SheetSpec::Standard { which } if seen.contains(which) => false,
        SheetSpec::Standard { which } => {
            seen.push(which.clone());
            true
        }
        _ => true,
    });
    Some(all)
}

fn yes() -> bool {
    true
}

/// A view on a sheet. Only the fields relevant to `kind` are used.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ViewSpec {
    pub kind: ViewKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// Groups or members drawn (path tails, e.g. `Left wall` or `T2`);
    /// empty means the whole model.
    #[serde(default)]
    pub of: Vec<String>,
    /// Further groups or members drawn with their hidden edges dashed
    /// (e.g. the walls below a roof plan); they are not tagged.
    #[serde(default)]
    pub dashed: Vec<String>,
    /// Direction toward the viewer. Defaults: plan `up`; elevation and
    /// detail the drawn group's −y (a wall's outside, a truss's face).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<DirRef>,
    /// Direction drawn upward on the page (default `up`, or the building's
    /// +y for a plan).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub up: Option<DirRef>,
    /// A standard scale by its label prefix (`1/4"`, `1-1/2"`, `1:50`);
    /// default: the largest that fits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<String>,
    /// Detail centre.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<Feature>,
    /// Detail radius (m).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius: Option<f64>,
    /// Tag members with their piece marks.
    #[serde(default = "yes")]
    pub tags: bool,
    /// Draw the drawn groups' dimensions and notes (those that are true
    /// length in this view).
    #[serde(default = "yes")]
    pub dims: bool,
    /// Schedule name, for `kind: schedule`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<String>,
    /// Text, for `kind: notes`.
    #[serde(default)]
    pub lines: Vec<String>,
}

impl ViewSpec {
    pub fn new(kind: ViewKind) -> ViewSpec {
        ViewSpec {
            kind,
            title: None,
            subtitle: None,
            of: vec![],
            dashed: vec![],
            from: None,
            up: None,
            scale: None,
            at: None,
            radius: None,
            tags: true,
            dims: true,
            table: None,
            lines: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn custom(n: &str) -> SheetSpec {
        SheetSpec::Sheet { number: n.into(), title: n.into(), views: vec![] }
    }
    fn names(v: &[SheetSpec]) -> Vec<String> {
        v.iter()
            .map(|s| match s {
                SheetSpec::Standard { which } => which.clone(),
                SheetSpec::Sheet { number, .. } => number.clone(),
            })
            .collect()
    }

    #[test]
    fn merging_sheet_lists() {
        assert_eq!(merge_sheets(None, None), None);
        // A building without a list contributes the standard set once.
        let m = merge_sheets(None, Some(vec![custom("A-1"), SheetSpec::Standard { which: "cover".into() }])).unwrap();
        assert_eq!(names(&m), vec!["cover", "plans", "trusses", "walls", "analytical", "schedules", "A-1"]);
        let m = merge_sheets(Some(vec![custom("A-1")]), Some(vec![custom("B-1")])).unwrap();
        assert_eq!(names(&m), vec!["A-1", "B-1"]);
    }
}
