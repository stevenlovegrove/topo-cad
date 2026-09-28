//! Design-code plug-in interface, calculation traces, and load combinations.

use serde::{Deserialize, Serialize};
use topo_core::{LoadCase, LoadCaseId, LoadKind, MemberId, Model};

/// One step of a calculation, printed in reports so an engineer can follow it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CalcStep {
    /// e.g. `F'b`
    pub symbol: String,
    /// e.g. `Fb · CD · CM · Ct · CL · CF · Cfu · Ci · Cr`
    pub formula: String,
    /// Formula with numbers substituted.
    pub substituted: String,
    pub value: f64,
    pub unit: String,
    /// e.g. `NDS 4.3.1`
    pub reference: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CalcTrace {
    pub steps: Vec<CalcStep>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Check {
    pub member: MemberId,
    pub combination: String,
    /// e.g. `Bending`
    pub title: String,
    /// e.g. `NDS 3.3`
    pub clause: String,
    pub demand: f64,
    pub capacity: f64,
    /// Unit of demand and capacity, e.g. `psi`, `lb`, `in`.
    #[serde(default)]
    pub unit: String,
    /// The other member involved (bearing on, fastened to), if any.
    #[serde(default)]
    pub other: Option<MemberId>,
    /// Where along the member it governs (m from its first node).
    #[serde(default)]
    pub at: Option<f64>,
    /// Assumptions and caveats an engineer should see.
    #[serde(default)]
    pub notes: Vec<String>,
    /// Whether every reference value used has been checked against its source.
    #[serde(default)]
    pub verified: bool,
    /// For information only (e.g. members designed by others, such as
    /// manufactured trusses); not counted as a failure.
    #[serde(default)]
    pub indicative: bool,
    pub trace: CalcTrace,
}

impl Check {
    pub fn ratio(&self) -> f64 {
        self.demand / self.capacity
    }
    pub fn ok(&self) -> bool {
        self.ratio() <= 1.0
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Combination {
    pub name: String,
    pub factors: Vec<(LoadCaseId, f64)>,
}

/// Internal forces on one member for one combination (filled by the solver / takedown in M2).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MemberForces {
    pub axial_max: f64,
    pub axial_min: f64,
    pub shear_max: f64,
    pub moment_max: f64,
    pub deflection_max: f64,
    /// Unbraced lengths (m) about the strong / weak axes and for lateral-torsional buckling.
    pub unbraced: (f64, f64, f64),
}

/// A structural design standard (NDS, Eurocode 5, CSA O86, Aluminum Design Manual, …).
pub trait DesignCode {
    fn name(&self) -> &str;
    /// Load combinations for the load cases present in the model.
    fn combinations(&self, cases: &[LoadCase]) -> Vec<Combination>;
    /// Member strength/serviceability checks. Codes that do not support a
    /// member's material return an empty list.
    fn check_member(&self, model: &Model, member: MemberId, combo: &Combination, forces: &MemberForces) -> Vec<Check>;
}

/// ASCE 7 §2.4.1 basic ASD combinations for an edition (`7-16` or `7-22`),
/// generated for the load kinds present. Several cases of one kind are
/// applied together. ASCE 7-22 uses strength-level snow loads, so snow
/// enters ASD combinations at 0.7S.
pub fn asce7_asd(cases: &[LoadCase], edition: &str) -> Vec<Combination> {
    use LoadKind::*;
    let s = if edition == "7-22" { 0.7 } else { 1.0 };
    let of = |k: LoadKind| cases.iter().filter(move |c| c.kind == k).map(|c| c.id).collect::<Vec<_>>();
    let has = |k: LoadKind| !of(k).is_empty();
    let mut out = vec![];
    let mut push = |terms: &[(LoadKind, f64)]| {
        if terms.iter().any(|(k, _)| !has(*k)) {
            return;
        }
        let name = terms
            .iter()
            .map(|&(k, f)| {
                let sym = match k {
                    Dead => "D",
                    Live => "L",
                    RoofLive => "Lr",
                    Snow => "S",
                    Wind => "W",
                    Seismic => "E",
                    Other => "O",
                };
                if (f - 1.0).abs() < 1e-9 { sym.to_string() } else { format!("{}{sym}", (f * 1000.0).round() / 1000.0) }
            })
            .collect::<Vec<_>>()
            .join(" + ");
        let factors = terms.iter().flat_map(|&(k, f)| of(k).into_iter().map(move |id| (id, f))).collect();
        out.push(Combination { name, factors });
    };
    push(&[(Dead, 1.0)]);
    push(&[(Dead, 1.0), (Live, 1.0)]);
    push(&[(Dead, 1.0), (RoofLive, 1.0)]);
    push(&[(Dead, 1.0), (Snow, s)]);
    push(&[(Dead, 1.0), (Live, 0.75), (RoofLive, 0.75)]);
    push(&[(Dead, 1.0), (Live, 0.75), (Snow, 0.75 * s)]);
    push(&[(Dead, 1.0), (Wind, 0.6)]);
    push(&[(Dead, 1.0), (Seismic, 0.7)]);
    push(&[(Dead, 1.0), (Live, 0.75), (Wind, 0.45), (Snow, 0.75 * s)]);
    push(&[(Dead, 1.0), (Live, 0.75), (Seismic, 0.525), (Snow, 0.75 * s)]);
    push(&[(Dead, 0.6), (Wind, 0.6)]);
    push(&[(Dead, 0.6), (Seismic, 0.7)]);
    out
}

/// Load combinations of the 1976 Uniform Building Code (working stress):
/// dead load plus each other load at full value, unfactored — roof live load
/// and snow are alternatives ("in place of", Sec. 2305(d)). Wind and
/// earthquake use the one-third stress increase (Sec. 2303(d)), carried by the
/// load-duration factor. Combinations of several transient loads were not
/// read from the code and are left out.
pub fn ubc1976(cases: &[LoadCase]) -> Vec<Combination> {
    use LoadKind::*;
    let sym = |k: LoadKind| match k {
        Dead => "D",
        Live => "L",
        RoofLive => "Lr",
        Snow => "S",
        Wind => "W",
        Seismic => "E",
        Other => "O",
    };
    let of = |k: LoadKind| cases.iter().filter(move |c| c.kind == k).map(|c| c.id).collect::<Vec<_>>();
    let mut out = vec![];
    for terms in [&[Dead][..], &[Dead, Live], &[Dead, RoofLive], &[Dead, Snow], &[Dead, Wind], &[Dead, Seismic]] {
        if terms.iter().any(|&k| of(k).is_empty()) {
            continue;
        }
        let name = terms.iter().map(|&k| sym(k)).collect::<Vec<_>>().join(" + ");
        out.push(Combination { name, factors: terms.iter().flat_map(|&k| of(k).into_iter().map(|id| (id, 1.0))).collect() });
    }
    out
}

/// NDS load duration factor C_D for a combination: governed by the
/// shortest-duration load present (NDS 2.3.2, Table 2.3.2).
pub fn nds_load_duration(model: &Model, combo: &Combination) -> f64 {
    combo
        .factors
        .iter()
        .map(|(id, _)| match model.load_cases[id.idx()].kind {
            LoadKind::Dead => 0.9,
            LoadKind::Live => 1.0,
            LoadKind::Snow => 1.15,
            LoadKind::RoofLive => 1.25,
            LoadKind::Wind | LoadKind::Seismic => 1.6,
            LoadKind::Other => 1.0,
        })
        .fold(0.0, f64::max)
}
