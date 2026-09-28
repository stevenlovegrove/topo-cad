//! As-built wall layouts from field measurements.
//!
//! A survey lists the vertical members of a wall as they are, each located by
//! an absolute reading from an explicit datum (so errors do not accumulate,
//! as they would with a chain of spacings), plus the openings between them.
//!
//! Every reading says three things:
//! - **where the tape or laser sits** ([`DatumRef`]): a corner of the wall
//!   (the inside face of the wall it meets, or the outside corner of the
//!   footprint), or a face of another surveyed member, moved by explicit,
//!   named offsets (e.g. ½" drywall on that face);
//! - **the distance**;
//! - **which face of the member was hit** ([`Hit`]): the face nearer the
//!   datum, the far face, or the centre.
//!
//! Positions are resolved to the wall's local x (from the outside corner at
//! the wall start) when the wall is built, where the corners are known.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WallEnd {
    Start,
    End,
}

impl WallEnd {
    /// Direction of measurement away from this end, along the wall's x.
    fn inward(self) -> f64 {
        match self {
            WallEnd::Start => 1.0,
            WallEnd::End => -1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CornerFace {
    /// The framing face of the wall met at this corner, inside the building.
    Inside,
    /// The outside corner of the footprint (the wall line's end point).
    Outside,
}

/// The surface a reading is taken from, before offsets. Ends of the wall
/// are named `start` / `end` (the order the perimeter path visits them) or
/// by compass direction (`north`, `south`, `east`, `west`), which picks
/// whichever end lies that way.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Datum {
    /// A corner of this wall; measuring away from it, along the wall.
    Corner { end: String, face: CornerFace },
    /// The face of another surveyed member on its `toward` side; measuring
    /// from it toward that end of the wall.
    Member { name: String, toward: String },
}

/// Which end of a wall running along `dir` (plan, start → end) `sel` names.
pub fn wall_end(sel: &str, dir: [f64; 2]) -> Result<WallEnd, String> {
    let v = match sel {
        "start" => return Ok(WallEnd::Start),
        "end" => return Ok(WallEnd::End),
        "north" => [0.0, 1.0],
        "south" => [0.0, -1.0],
        "east" => [1.0, 0.0],
        "west" => [-1.0, 0.0],
        o => return Err(format!("unknown wall end \"{o}\" (start, end, north, south, east or west)")),
    };
    let d = v[0] * dir[0] + v[1] * dir[1];
    if d > 0.5 {
        Ok(WallEnd::End)
    } else if d < -0.5 {
        Ok(WallEnd::Start)
    } else {
        Err(format!("the wall does not run {sel}ward enough to have a {sel} end; use start or end"))
    }
}

/// A named shift of a datum surface, along the direction of measurement
/// (positive: into the space being measured, e.g. the thickness of drywall
/// the laser sits on).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Offset {
    pub distance: f64,
    pub note: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DatumRef {
    pub datum: Datum,
    #[serde(default)]
    pub offsets: Vec<Offset>,
}

/// Which face of the measured member the reading reaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Hit {
    /// The face nearer the datum.
    Near,
    Centre,
    /// The face farther from the datum.
    Far,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Reading {
    pub from: DatumRef,
    pub distance: f64,
    pub hit: Hit,
}

/// Vertical datums, in the wall.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    /// Bottom of the bottom plate (top of slab); measuring up.
    Slab,
    /// Top of the bottom plate; measuring up.
    PlateTop,
    /// Underside of the double top plate; measuring down.
    UnderTopPlate,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Height {
    pub level: Level,
    #[serde(default)]
    pub offsets: Vec<Offset>,
    pub distance: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostRole {
    /// Full height, bottom plate to top plate.
    Stud,
    /// Full height beside an opening; carries the header's end.
    King,
    /// Under a header, beside the opening.
    Jack,
    /// Short stud within an opening: below the sill and/or above the header.
    Cripple,
}

impl PostRole {
    pub fn role(self) -> &'static str {
        match self {
            PostRole::Stud => "stud",
            PostRole::King => "king_stud",
            PostRole::Jack => "jack_stud",
            PostRole::Cripple => "cripple",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "item")]
pub enum SurveyItem {
    Post {
        role: PostRole,
        at: Reading,
        /// Plies side by side along the wall (a doubled stud is 2).
        #[serde(default = "one")]
        plies: u32,
        #[serde(default)]
        name: Option<String>,
    },
    /// A rough opening between the posts listed before and after it (its
    /// cripples may be listed next to it).
    Opening {
        label: String,
        /// `door` or `window`.
        kind: String,
        /// Top of the rough opening (bottom of the header).
        #[serde(default)]
        head: Option<Height>,
        /// Top of the sill (windows).
        #[serde(default)]
        sill: Option<Height>,
        /// Header as (plies, nominal thickness, nominal depth).
        header: (u32, u32, u32),
        /// Header tight under the top plate (then `head` is derived).
        #[serde(default)]
        header_flush: bool,
    },
}

fn one() -> u32 {
    1
}

/// A measured distance between two datums, checked against the model.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SurveyCheck {
    pub from: DatumRef,
    pub to: DatumRef,
    pub distance: f64,
    pub tolerance: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Survey {
    pub items: Vec<SurveyItem>,
    #[serde(default)]
    pub checks: Vec<SurveyCheck>,
}

/// What a survey is resolved against.
#[derive(Clone, Copy, Debug)]
pub struct WallContext {
    /// Wall length, outside corner to outside corner (m).
    pub len: f64,
    /// Thickness of one stud ply along the wall.
    pub ply: f64,
    /// Inside faces of the walls met at the start and end, as x along this
    /// wall (on its inside face); `None` where no wall meets it.
    pub inside: [Option<f64>; 2],
    /// Plan direction of the wall, start → end (for compass-named ends).
    pub dir: [f64; 2],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Post {
    pub role: PostRole,
    /// Centre along the wall.
    pub x: f64,
    pub plies: u32,
    pub name: Option<String>,
    /// Position in the survey's item list.
    pub item: usize,
}

impl Post {
    pub fn width(&self, ply: f64) -> f64 {
        self.plies as f64 * ply
    }
}

/// The survey with every position resolved to wall x, items as listed.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedSurvey {
    pub items: Vec<ResolvedItem>,
    /// Measured check distances: (description, measured, model, tolerance).
    pub checks: Vec<(String, f64, f64, f64)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ResolvedItem {
    Post(Post),
    Opening(usize),
}

fn fmt_in(m: f64) -> String {
    format!("{:.2}\"", m / 0.0254)
}

impl DatumRef {
    fn describe(&self) -> String {
        let base = match &self.datum {
            Datum::Corner { end, face } => format!(
                "{end} corner ({})",
                if *face == CornerFace::Inside { "inside face of the wall there" } else { "outside corner" }
            ),
            Datum::Member { name, toward } => format!("{name} ({toward}-side face)"),
        };
        self.offsets.iter().fold(base, |s, o| format!("{s} + {} {}", fmt_in(o.distance), o.note))
    }
}

impl Survey {
    /// Resolves every reading to a centre x along the wall.
    pub fn resolve(&self, wall: &str, cx: &WallContext) -> Result<ResolvedSurvey, String> {
        // Posts, resolved in dependency order (member datums need their member first).
        let mut xs: Vec<Option<f64>> = vec![None; self.items.len()];
        let names: std::collections::HashMap<&str, usize> = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, it)| match it {
                SurveyItem::Post { name: Some(n), .. } => Some((n.as_str(), i)),
                _ => None,
            })
            .collect();
        let width = |i: usize| match &self.items[i] {
            SurveyItem::Post { plies, .. } => *plies as f64 * cx.ply,
            _ => 0.0,
        };
        // Datum surface position and measuring direction, if resolvable yet.
        let datum = |d: &DatumRef, xs: &[Option<f64>]| -> Result<Option<(f64, f64)>, String> {
            let (x0, dir) = match &d.datum {
                Datum::Corner { end, face: CornerFace::Outside } => {
                    let end = wall_end(end, cx.dir).map_err(|e| format!("{wall}: {e}"))?;
                    (if end == WallEnd::Start { 0.0 } else { cx.len }, end.inward())
                }
                Datum::Corner { end, face: CornerFace::Inside } => {
                    let end = wall_end(end, cx.dir).map_err(|e| format!("{wall}: {e}"))?;
                    let k = if end == WallEnd::Start { 0 } else { 1 };
                    let x = cx.inside[k].ok_or_else(|| {
                        format!("{wall}: no wall meets its {} to measure from; use the outside corner or a member", if k == 0 { "start" } else { "end" })
                    })?;
                    (x, end.inward())
                }
                Datum::Member { name, toward } => {
                    let &i = names.get(name.as_str()).ok_or_else(|| format!("{wall}: no member named {name} to measure from"))?;
                    let Some(c) = xs[i] else { return Ok(None) };
                    // The face on the `toward` side; measuring on toward that end.
                    let s = if wall_end(toward, cx.dir).map_err(|e| format!("{wall}: {e}"))? == WallEnd::End { 1.0 } else { -1.0 };
                    (c + s * width(i) / 2.0, s)
                }
            };
            Ok(Some((x0 + dir * d.offsets.iter().map(|o| o.distance).sum::<f64>(), dir)))
        };
        loop {
            let mut progress = false;
            let mut pending = vec![];
            for (i, it) in self.items.iter().enumerate() {
                let SurveyItem::Post { at, .. } = it else { continue };
                if xs[i].is_some() {
                    continue;
                }
                match datum(&at.from, &xs)? {
                    Some((x0, dir)) => {
                        let half = width(i) / 2.0;
                        let hit = match at.hit {
                            Hit::Near => half,
                            Hit::Centre => 0.0,
                            Hit::Far => -half,
                        };
                        xs[i] = Some(x0 + dir * (at.distance + hit));
                        progress = true;
                    }
                    None => pending.push(i),
                }
            }
            if pending.is_empty() {
                break;
            }
            if !progress {
                return Err(format!("{wall}: readings measured from each other in a loop (items {pending:?})"));
            }
        }
        // Items stay in the order listed: that order says which posts are
        // beside each opening (positions come from the readings themselves,
        // so the list may jump, e.g. to readings from the other corner).
        let items = (0..self.items.len())
            .map(|i| match &self.items[i] {
                SurveyItem::Post { role, plies, name, .. } => ResolvedItem::Post(Post { role: *role, x: xs[i].unwrap(), plies: *plies, name: name.clone(), item: i }),
                SurveyItem::Opening { .. } => ResolvedItem::Opening(i),
            })
            .collect();
        let mut checks = vec![];
        for c in &self.checks {
            let (Some((a, dir)), Some((b, _))) = (datum(&c.from, &xs)?, datum(&c.to, &xs)?) else { unreachable!("all posts resolved") };
            checks.push((format!("{} → {}", c.from.describe(), c.to.describe()), c.distance, dir * (b - a), c.tolerance));
        }
        Ok(ResolvedSurvey { items, checks })
    }
}

impl Height {
    /// Height above the bottom of the wall, for a wall of total height `h`
    /// with plates `b` thick (double top plate).
    pub fn z(&self, h: f64, b: f64) -> f64 {
        let d = self.distance + self.offsets.iter().map(|o| o.distance).sum::<f64>();
        match self.level {
            Level::Slab => d,
            Level::PlateTop => b + d,
            Level::UnderTopPlate => h - 2.0 * b - d,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const IN: f64 = 0.0254;

    fn post(role: PostRole, from: Datum, d: f64, hit: Hit) -> SurveyItem {
        // `d` in inches.
        SurveyItem::Post { role, at: Reading { from: DatumRef { datum: from, offsets: vec![] }, distance: d * IN, hit }, plies: 1, name: None }
    }
    fn start_in() -> Datum {
        Datum::Corner { end: "start".into(), face: CornerFace::Inside }
    }
    fn cx() -> WallContext {
        WallContext { len: 120.0 * IN, ply: 1.5 * IN, inside: [Some(5.5 * IN), Some(114.5 * IN)], dir: [0.0, 1.0] }
    }
    fn xs(r: &ResolvedSurvey) -> Vec<f64> {
        r.items.iter().filter_map(|i| if let ResolvedItem::Post(p) = i { Some(p.x / IN) } else { None }).collect()
    }

    #[test]
    fn datums_offsets_and_faces() {
        let end_in = Datum::Corner { end: "end".into(), face: CornerFace::Inside };
        let s = Survey {
            items: vec![
                post(PostRole::Stud, start_in(), 0.0, Hit::Near),                                             // tight to the corner: 5.5 + 0.75
                post(PostRole::Stud, start_in(), 16.0, Hit::Centre),                                          // 21.5
                SurveyItem::Post {
                    role: PostRole::Stud,
                    at: Reading { from: DatumRef { datum: start_in(), offsets: vec![Offset { distance: 0.5 * IN, note: "drywall".into() }] }, distance: 30.0 * IN, hit: Hit::Far },
                    plies: 2,
                    name: Some("D".into()),
                },                                                                                              // 5.5 + 0.5 + 30 − 1.5 = 34.5
                post(PostRole::Stud, Datum::Member { name: "D".into(), toward: "end".into() }, 10.0, Hit::Near), // 36 + 10 + 0.75
                post(PostRole::Stud, end_in, 0.0, Hit::Near),                                                  // 114.5 − 0.75
            ],
            checks: vec![],
        };
        let r = s.resolve("W", &cx()).unwrap();
        let got = xs(&r);
        let want = [6.25, 21.5, 34.5, 46.75, 113.75];
        for (g, w) in got.iter().zip(want) {
            assert!((g - w).abs() < 1e-9, "{got:?}");
        }
    }

    #[test]
    fn reverse_order_and_checks() {
        let end_out = Datum::Corner { end: "end".into(), face: CornerFace::Outside };
        let s = Survey {
            items: vec![post(PostRole::Stud, end_out.clone(), 0.0, Hit::Near), post(PostRole::Stud, end_out, 16.0, Hit::Centre)],
            checks: vec![SurveyCheck {
                from: DatumRef { datum: start_in(), offsets: vec![] },
                to: DatumRef { datum: Datum::Corner { end: "end".into(), face: CornerFace::Inside }, offsets: vec![] },
                distance: 109.25 * IN,
                tolerance: 0.25 * IN,
            }],
        };
        let r = s.resolve("W", &cx()).unwrap();
        assert_eq!(xs(&r), vec![119.25, 104.0], "measured from the end");
        let (_, measured, model, _) = r.checks[0].clone();
        assert!((measured - model - 0.25 * IN).abs() < 1e-9);
    }

    #[test]
    fn missing_corner_and_compass_ends() {
        let s = Survey { items: vec![post(PostRole::Stud, start_in(), 20.0, Hit::Near)], checks: vec![] };
        let open = WallContext { inside: [None, None], ..cx() };
        assert!(s.resolve("W", &open).unwrap_err().contains("no wall meets its start"));
        // Compass-named ends: this wall runs north, so its south end is the start.
        assert_eq!(wall_end("south", [0.0, 1.0]), Ok(WallEnd::Start));
        assert_eq!(wall_end("north", [0.0, 1.0]), Ok(WallEnd::End));
        assert!(wall_end("east", [0.0, 1.0]).is_err());
    }
}
