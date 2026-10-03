//! The scene spec a script exports (mirrors `ts/topo-cad.ts`) and its
//! expansion into a `Model` using the Rust generators.

use serde::Deserialize;
use std::collections::HashMap;
use topo_core::*;
use topo_core::scene;
use topo_timber::fastening as fx;
use topo_timber::lumber::graded;
use topo_timber::lumber::{built_up, sawn};
use topo_timber::{Justify, Opening, OpeningKind, Perimeter, PerimeterParts, Side, TrussRoof, TrussShape, Wall};

/// What a script exports, normalised to a site: one or more buildings, each
/// placed in the world frame (+x east, +y north, +z up).
#[derive(Debug, Deserialize)]
pub struct SceneSpec {
    pub name: String,
    #[serde(default)]
    pub info: InfoSpec,
    #[serde(default)]
    pub buildings: Vec<BuildingSpec>,
    #[serde(default)]
    pub measurements: Vec<topo_geom::measure::Measurement>,
    #[serde(default)]
    pub unknowns: Vec<UnknownSpec>,
    /// The drawing set (sheet tabs, in order); `None` for the standard set.
    #[serde(default)]
    pub sheets: Option<Vec<SheetSpec>>,
    /// ASCE 7 edition the loads follow (`7-16` default, or `7-22`).
    #[serde(default)]
    pub asce7: Option<String>,
    /// Include member self-weight in the dead load (default true).
    #[serde(default)]
    pub self_weight: Option<bool>,
    /// Load combinations from another code (`ubc-1976`); default ASCE 7.
    #[serde(default)]
    pub combinations: Option<String>,
    #[serde(default)]
    pub scenarios: Vec<ScenarioInfo>,
    #[serde(default)]
    pub scenario: Option<String>,
}

/// One building: items in its own frame, placed in the world.
#[derive(Debug, Deserialize)]
pub struct BuildingSpec {
    pub name: String,
    #[serde(default)]
    pub info: InfoSpec,
    #[serde(default)]
    pub items: Vec<ItemSpec>,
    #[serde(default)]
    pub measurements: Vec<topo_geom::measure::Measurement>,
    #[serde(default)]
    pub unknowns: Vec<UnknownSpec>,
    #[serde(default)]
    pub placement: Option<PlacementSpec>,
    /// Named directions in the building frame (e.g. `street`).
    #[serde(default)]
    pub directions: std::collections::BTreeMap<String, [f64; 3]>,
    #[serde(default)]
    pub sheets: Option<Vec<SheetSpec>>,
    #[serde(default)]
    pub asce7: Option<String>,
    #[serde(default)]
    pub self_weight: Option<bool>,
    /// Load combinations from another code (`ubc-1976`); default ASCE 7.
    #[serde(default)]
    pub combinations: Option<String>,
    #[serde(default)]
    pub scenarios: Vec<ScenarioInfo>,
    #[serde(default)]
    pub scenario: Option<String>,
}

/// Where a building sits: its origin in the world, and the compass bearing
/// of its +x axis (degrees clockwise from north; 90 = east, the default).
#[derive(Debug, Clone, Deserialize)]
pub struct PlacementSpec {
    #[serde(default)]
    pub origin: [f64; 3],
    #[serde(default = "east")]
    pub x_bearing: f64,
}

fn east() -> f64 {
    90.0
}

impl PlacementSpec {
    pub fn frame(&self) -> Frame {
        let t = (90.0 - self.x_bearing).to_radians();
        let x = v3(t.cos(), t.sin(), 0.0);
        Frame { origin: v3(self.origin[0], self.origin[1], self.origin[2]), x, y: Vec3::Z.cross(x), z: Vec3::Z }
    }
}

impl SceneSpec {
    /// Parses an exported `Site` (`{"type": "site", …}`) or a single
    /// `Building` (wrapped as a one-building site).
    pub fn from_json(json: &str) -> Result<SceneSpec, String> {
        let v: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
        if v.get("type").and_then(|t| t.as_str()) == Some("site") {
            let mut s: SceneSpec = serde_json::from_value(v).map_err(|e| e.to_string())?;
            let mut names = std::collections::HashSet::new();
            for b in &mut s.buildings {
                if !names.insert(b.name.clone()) {
                    return Err(format!("two buildings are named \"{}\"", b.name));
                }
                s.measurements.append(&mut b.measurements);
            }
            return Ok(s);
        }
        let mut b: BuildingSpec = serde_json::from_value(v).map_err(|e| e.to_string())?;
        Ok(SceneSpec {
            name: b.name.clone(),
            info: std::mem::take(&mut b.info),
            measurements: std::mem::take(&mut b.measurements),
            unknowns: std::mem::take(&mut b.unknowns),
            sheets: b.sheets.take(),
            asce7: b.asce7.take(),
            self_weight: b.self_weight.take(),
            combinations: b.combinations.take(),
            scenarios: std::mem::take(&mut b.scenarios),
            scenario: b.scenario.take(),
            buildings: vec![b],
        })
    }
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct UnknownSpec {
    pub name: String,
    pub guess: f64,
    /// `length` or `ratio` (for reporting).
    pub unit: String,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct InfoSpec {
    pub number: String,
    pub client: String,
    pub address: String,
    pub designer: String,
    pub date: String,
    pub design_basis: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ItemSpec {
    Perimeter(PerimeterSpec),
    TrussRoof(TrussRoofSpec),
    Assembly(AssemblySpec),
    /// A frame with points in it, as nodes (`Frame/point`): a body (a scan's, placed where it is
    /// registered, or free), or the world (its points are datums).
    Frame(FrameItemSpec),
    /// Two nodes are one (`Assembly name/point`); or, with a tolerance, were read to be one point.
    Coincide {
        a: String,
        b: String,
        #[serde(default)]
        tolerance: Option<f64>,
    },
    /// A reading with a tape: two nodes are this far apart, to about `tolerance`.
    Tape { a: String, b: String, distance: f64, tolerance: f64 },
    /// A surveyed point: a node is here, along whichever of x, y, z are given, to about `tolerance`.
    Survey {
        node: String,
        #[serde(default)]
        x: Option<f64>,
        #[serde(default)]
        y: Option<f64>,
        #[serde(default)]
        z: Option<f64>,
        tolerance: f64,
    },
}

impl ItemSpec {
    /// The nodes a relation names.
    fn nodes(&self) -> Vec<&str> {
        match self {
            ItemSpec::Coincide { a, b, .. } | ItemSpec::Tape { a, b, .. } => vec![a, b],
            ItemSpec::Survey { node, .. } => vec![node],
            _ => vec![],
        }
    }
}

/// A relation, into a scene, its nodes found by `node`.
fn relate(sc: &mut scene::Scene, item: &ItemSpec, node: &mut dyn FnMut(&mut scene::Scene, &str) -> Result<scene::NodeRef, String>) -> Result<(), String> {
    match item {
        ItemSpec::Coincide { a, b, tolerance } => {
            let (na, nb) = (node(sc, a)?, node(sc, b)?);
            match tolerance {
                None => sc.coincide(&format!("{a} is {b}"), na, nb),
                Some(t) => sc.read_at(&format!("{a} read to be at {b}"), na, nb, *t),
            }
        }
        ItemSpec::Tape { a, b, distance, tolerance } => {
            let (na, nb) = (node(sc, a)?, node(sc, b)?);
            sc.tape(&format!("{a} to {b}, read {distance:.4} m"), na, nb, *distance, *tolerance);
        }
        ItemSpec::Survey { node: n, x, y, z, tolerance } => {
            let nn = node(sc, n)?;
            sc.survey(&format!("{n} surveyed"), nn, [*x, *y, *z], *tolerance);
        }
        _ => {}
    }
    Ok(())
}

/// Placement of a local frame in world coordinates.
#[derive(Debug, Clone, Deserialize)]
pub struct FrameSpec {
    pub origin: [f64; 3],
    pub x: [f64; 3],
    pub y: [f64; 3],
    pub z: [f64; 3],
}

impl FrameSpec {
    fn frame(&self) -> Frame {
        let v = |a: [f64; 3]| v3(a[0], a[1], a[2]);
        Frame { origin: v(self.origin), x: v(self.x), y: v(self.y), z: v(self.z) }
    }
}

#[derive(Debug, Deserialize)]
pub struct AssemblyPoint {
    pub name: String,
    pub at: [f64; 3],
}

#[derive(Debug, Deserialize)]
pub struct AssemblyMember {
    pub role: String,
    pub path: Vec<String>,
    pub size: (u32, u32),
    #[serde(default = "one")]
    pub plies: u32,
    pub grade: GradeSpec,
    /// Local depth direction (defaults as for any member: up, or +y if vertical).
    #[serde(default)]
    pub depth: Option<[f64; 3]>,
    /// Section anchor (u, v) in [-½, ½].
    #[serde(default)]
    pub anchor: Option<(f64, f64)>,
    #[serde(default)]
    pub priority: i32,
}

fn one() -> u32 {
    1
}

#[derive(Debug, Deserialize)]
pub struct AssemblySpec {
    pub name: String,
    pub placement: FrameSpec,
    pub points: Vec<AssemblyPoint>,
    pub members: Vec<AssemblyMember>,
    /// `nailed`, `truss_plate`, `none`, or explicit nailing: connection
    /// recorded where members meet.
    pub joints: JointsSpec,
    #[serde(default)]
    pub bears_on: Vec<String>,
    /// Points that bear on the foundation.
    #[serde(default)]
    pub supports: Vec<String>,
    #[serde(default)]
    pub loads: Vec<AssemblyLoadSpec>,
    /// Pairs of members fastened face to face (`role` or `role#i`).
    #[serde(default)]
    pub bonds: Vec<(String, String)>,
    /// Pairs of members, the first resting on the second where they cross.
    #[serde(default)]
    pub rests: Vec<(String, String)>,
    /// Free: its placement is only where it starts; relations place it.
    #[serde(default)]
    pub free: bool,
}

/// A frame with points in it.
#[derive(Debug, Clone, Deserialize)]
pub struct FrameItemSpec {
    pub name: String,
    /// The world: fixed where it is, its points datums.
    #[serde(default)]
    pub world: bool,
    #[serde(default)]
    pub placement: Option<FrameSpec>,
    /// Free: its placement is only where it starts; relations to its points place it.
    #[serde(default)]
    pub free: bool,
    pub points: Vec<FramePointSpec>,
}

/// A point of a frame: where it is in the frame, along the axes given (the others are free).
#[derive(Debug, Clone, Deserialize)]
pub struct FramePointSpec {
    pub name: String,
    pub at: [Option<f64>; 3],
}

/// An assembly, declared in the scene: its points' nodes, its members, its rests' nodes.
struct Declared {
    points: HashMap<String, scene::NodeRef>,
    members: Vec<scene::MemberRef>,
    /// Each place one member rests on another: its node, and the two (in the scene).
    rests: Vec<(scene::NodeRef, scene::MemberRef, scene::MemberRef)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
pub struct GradeSpec {
    pub species: String,
    pub grade: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpeningSpec {
    pub label: String,
    pub kind: String,
    pub center: f64,
    pub width: f64,
    pub height: f64,
    pub head: f64,
    pub header: (u32, u32, u32),
    pub jacks: u32,
    pub header_flush: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WallSpec {
    pub name: String,
    /// Its height at its start; and at its end, if that is another (its top slopes).
    pub height: f64,
    #[serde(default)]
    pub height_end: Option<f64>,
    pub studs: (u32, u32),
    pub spacing: f64,
    #[serde(default)]
    pub layout_from: Vec<f64>,
    #[serde(default)]
    pub studs_at: Vec<f64>,
    pub grade: GradeSpec,
    pub header_grade: Option<GradeSpec>,
    pub justify: String,
    pub openings: Vec<OpeningSpec>,
    pub cap_breaks: Vec<f64>,
    /// As-built layout from field measurements.
    #[serde(default)]
    pub survey: Option<topo_timber::Survey>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SegmentSpec {
    pub to: (f64, f64),
    pub side: Option<WallSpec>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PerimeterSpec {
    pub name: String,
    pub start: (f64, f64),
    pub z: f64,
    pub segments: Vec<SegmentSpec>,
    pub closed: bool,
    /// Free: placed by relations to its nodes, not where it is drawn.
    #[serde(default)]
    pub free: bool,
    /// The bottom of the walls at each corner (from the start), where it is not `z`.
    #[serde(default)]
    pub bases: Vec<Option<f64>>,
}

/// The numbers a perimeter is built from: where its corners are (each at the bottom of the walls
/// there), each wall's height at its start and its end, and each wall's common studs (along it
/// from its start; none: where the layout puts them).
#[derive(Clone, Debug, PartialEq)]
struct PerimeterParams {
    corners: Vec<Vec3>,
    heights: Vec<Option<(f64, f64)>>,
    studs: Vec<Option<Vec<f64>>>,
}

impl PerimeterParams {
    /// As the script draws it.
    fn drawn(p: &PerimeterSpec) -> PerimeterParams {
        let n = if p.closed { p.segments.len() } else { p.segments.len() + 1 };
        let corners = (0..n)
            .map(|k| {
                let (x, y) = if k == 0 { p.start } else { p.segments[k - 1].to };
                v3(x, y, p.bases.get(k).copied().flatten().unwrap_or(p.z))
            })
            .collect();
        let heights = p.segments.iter().map(|s| s.side.as_ref().map(|w| (w.height, w.height_end.unwrap_or(w.height)))).collect();
        PerimeterParams { corners, heights, studs: vec![None; p.segments.len()] }
    }

    /// The numbers as solved.
    fn solved(n: &PerimeterNumbers, values: &HashMap<scene::ScalarId, f64>, origin: Vec3) -> PerimeterParams {
        let v = |s: &scene::ScalarId| values[s];
        PerimeterParams {
            corners: n.corners.iter().map(|c| v3(v(&c[0]), v(&c[1]), v(&c[2])) + origin).collect(),
            heights: n.heights.iter().map(|h| h.map(|(a, b)| (v(&a), v(&b)))).collect(),
            studs: n.studs.iter().map(|ss| ss.as_ref().map(|ss| ss.iter().map(v).collect())).collect(),
        }
    }

    /// How far apart two sets of numbers are (the most any one differs; studs that are not
    /// given in both differ).
    fn distance(&self, o: &PerimeterParams) -> f64 {
        let mut d = self.corners.iter().zip(&o.corners).map(|(a, b)| a.distance(*b)).fold(0.0, f64::max);
        for (a, b) in self.heights.iter().zip(&o.heights) {
            if let (Some(a), Some(b)) = (a, b) {
                d = d.max((a.0 - b.0).abs()).max((a.1 - b.1).abs());
            }
        }
        for (a, b) in self.studs.iter().zip(&o.studs) {
            match (a, b) {
                (Some(a), Some(b)) if a.len() == b.len() => d = a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(d, f64::max),
                (None, None) => {}
                _ => d = f64::INFINITY,
            }
        }
        d
    }
}

/// A perimeter's numbers in the scene (as `PerimeterParams`; corners in its body).
#[derive(Clone, Debug, Default)]
struct PerimeterNumbers {
    corners: Vec<[scene::ScalarId; 3]>,
    heights: Vec<Option<(scene::ScalarId, scene::ScalarId)>>,
    studs: Vec<Option<Vec<scene::ScalarId>>>,
}

#[derive(Debug, Deserialize)]
pub struct AreaLoadSpec {
    pub kind: String,
    pub pressure: f64,
    /// `plan` (default) or `surface`.
    #[serde(default)]
    pub basis: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    /// `slope` (default: the sloped roof, via the top chords) or `flat`: the
    /// parts of the bottom chords beyond the heels (a flat roof on a
    /// bottom-chord extension).
    #[serde(default)]
    pub on: Option<String>,
    /// The layers the load is made of (top first): recorded as the roof's
    /// surface build-up for drawings and the 3D view.
    #[serde(default)]
    pub layers: Option<Vec<topo_core::Layer>>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum JointsSpec {
    Kind(String),
    Nails { nails: u32, penny: u32, method: String },
}

/// A load on an assembly: along a member (`w`, N/m downward) or at a point
/// (`force`, N downward).
#[derive(Debug, Deserialize)]
pub struct AssemblyLoadSpec {
    pub kind: String,
    #[serde(default)]
    pub member: Option<String>,
    #[serde(default)]
    pub point: Option<String>,
    #[serde(default)]
    pub w: f64,
    #[serde(default)]
    pub force: f64,
}

#[derive(Debug, Deserialize)]
pub struct TrussRoofSpec {
    pub name: String,
    /// The node the first heel is set out from on plan (a point of the world, of a scan's frame,
    /// of the walls).
    pub at: String,
    pub span_dir: (f64, f64),
    pub shape: TrussShape,
    pub length: f64,
    pub spacing: f64,
    pub chord: (u32, u32),
    pub web: (u32, u32),
    pub grade: GradeSpec,
    pub bears_on: Vec<String>,
    pub loads: Vec<AreaLoadSpec>,
    /// Other shapes for particular trusses (1-based numbers).
    #[serde(default)]
    pub overrides: Vec<ShapeOverrideSpec>,
    /// Trusses as built off the layout: further along the ridge by `along`.
    #[serde(default)]
    pub shifts: Vec<TrussShiftSpec>,
}

#[derive(Debug, Deserialize)]
pub struct TrussShiftSpec {
    pub trusses: Vec<usize>,
    pub along: f64,
}

#[derive(Debug, Deserialize)]
pub struct ShapeOverrideSpec {
    pub trusses: Vec<usize>,
    pub shape: TrussShape,
}

#[derive(Clone)]
struct Builder {
    m: Model,
    /// Nodes a generator built that move with numbers of the scene: its body, where that was
    /// put, and how much each number moves it (`bind`).
    moving: HashMap<NodeId, (scene::BodyRef, Vec3, Vec<(scene::ScalarId, Vec3)>)>,
    grades: HashMap<GradeSpec, MaterialId>,
    perimeters: HashMap<String, (PerimeterParts, f64)>,
    /// Assembly members by assembly name (roofs may bear on them).
    assemblies: HashMap<String, Vec<MemberId>>,
    root: GroupId,
}

impl Builder {
    /// The load case for a kind (`dead`, `live`, `roof_live`, `snow`, `wind`,
    /// `seismic`), created on first use.
    fn case(&mut self, kind: &str) -> Result<LoadCaseId, String> {
        let (kind, name) = match kind {
            "dead" => (LoadKind::Dead, "D"),
            "live" => (LoadKind::Live, "L"),
            "roof_live" => (LoadKind::RoofLive, "Lr"),
            "snow" => (LoadKind::Snow, "S"),
            "wind" => (LoadKind::Wind, "W"),
            "seismic" => (LoadKind::Seismic, "E"),
            k => return Err(format!("unknown load kind {k} (dead, live, roof_live, snow, wind, seismic)")),
        };
        Ok(match self.m.load_cases.iter().find(|c| c.kind == kind) {
            Some(c) => c.id,
            None => self.m.add_load_case(name, kind),
        })
    }

    fn material(&mut self, g: &GradeSpec) -> Result<MaterialId, String> {
        if let Some(&id) = self.grades.get(g) {
            return Ok(id);
        }
        if topo_timber::lumber::nds_reference(&g.species, &g.grade).is_none() {
            return Err(format!("unknown lumber grade {} {}", g.species, g.grade));
        }
        let id = self.m.add_material(graded(&g.species, &g.grade));
        self.grades.insert(g.clone(), id);
        Ok(id)
    }

    fn wall(&mut self, w: &WallSpec) -> Result<Wall, String> {
        let mat = self.material(&w.grade)?;
        let mut wall = Wall::template(w.height, mat).named(&w.name).studs(w.studs.0, w.studs.1, w.spacing);
        if !w.layout_from.is_empty() {
            wall = wall.layout_from(&w.layout_from);
        }
        if !w.studs_at.is_empty() {
            wall = wall.studs_at(&w.studs_at);
        }
        if let Some(h) = &w.header_grade {
            wall = wall.header_material(self.material(h)?);
        }
        wall = wall.justify(match w.justify.as_str() {
            "center" => Justify::Center,
            "exterior" => Justify::Exterior,
            j => return Err(format!("wall {}: unknown justification {j}", w.name)),
        });
        for o in &w.openings {
            let base = match o.kind.as_str() {
                "door" => Opening::door(&o.label, o.center, o.width, o.height),
                "window" => Opening::window(&o.label, o.center, o.width, o.height, o.head),
                k => return Err(format!("opening {}: unknown kind {k}", o.label)),
            };
            let mut op = base.header(o.header.0, o.header.1, o.header.2).jacks(o.jacks);
            if o.header_flush {
                op = op.header_flush();
            } else if op.kind == OpeningKind::Door && o.height <= 0.0 {
                return Err(format!("door {} needs a height or .flushHeader()", o.label));
            }
            wall = wall.opening(op);
        }
        for &x in &w.cap_breaks {
            wall = wall.cap_break(x);
        }
        if let Some(s) = &w.survey {
            if !w.openings.is_empty() {
                return Err(format!("wall {}: a surveyed wall lists its openings in the survey, not with .opening()", w.name));
            }
            wall = wall.surveyed(s.clone());
        }
        Ok(wall)
    }

    /// A perimeter: written by its generator where it is drawn, and a body of the scene (fixed
    /// there, or free and moved to where the relations to its nodes put it).
    /// Builds a perimeter's walls into the model at these numbers: on its corners, each wall at
    /// its heights, its common studs where they are given (else where the layout puts them).
    fn build_perimeter(&mut self, p: &PerimeterSpec, at: &PerimeterParams) -> Result<(PerimeterParts, Vec<Option<Wall>>), String> {
        let c = &at.corners;
        let mut per = Perimeter::start(&p.name, Vec2::new(c[0].x, c[0].y));
        per.z = c[0].z;
        let n = p.segments.len();
        for (i, s) in p.segments.iter().enumerate() {
            let side = match &s.side {
                Some(w) => Side::Wall(self.wall(w)?),
                None => Side::Open,
            };
            if p.closed && i == n - 1 {
                per = per.close(side);
            } else {
                per = per.to(Vec2::new(c[i + 1].x, c[i + 1].y), side);
            }
        }
        per.bases = c.iter().map(|c| Some(c.z)).collect();
        per.heights = at.heights.clone();
        let mut walls = per.walls();
        for (w, studs) in walls.iter_mut().zip(&at.studs) {
            if let (Some(w), Some(studs)) = (w.as_mut(), studs) {
                match &w.survey {
                    None if w.common_studs().len() == studs.len() => w.studs_solved = Some(studs.clone()),
                    Some(survey) if survey.items.iter().filter(|i| matches!(i, topo_timber::survey::SurveyItem::Post { .. })).count() == studs.len() => w.survey_solved = Some(studs.clone()),
                    _ => {}
                }
            }
        }
        let parts = self.m.scoped(|m| per.try_build_walls(m, walls.clone()))?;
        Ok((parts, walls))
    }

    /// A perimeter, as a body of the scene, built by its generator at numbers of the scene:
    ///
    /// - its corners (`Perimeter/corner 1`, from its start), nodes of three numbers each (where it
    ///   is in the body: where it is drawn a hint), at the bottom of the walls there;
    /// - each wall's height at its ends (as drawn a hint; where two walls meet at one height as
    ///   drawn, held so: a wall meeting another at a different height is a step);
    /// - each wall's common studs, along it from its start: where the layout puts one a hint (one
    ///   that gives way to the corners' and heights': a reading on a stud moves it, not them), one
    ///   given by `studsAt` a reading, one against the wall at an end held there; in order (and
    ///   so in the wall).
    ///
    /// It is built at `at` (numbers as the solve last put them, or as drawn). Any node it built
    /// that a relation names is a node of the scene that moves with the numbers as the built one
    /// would (found by building it again with each moved a little). Once solved, it is built
    /// again at the solved numbers, until they do not change (`build_building`).
    fn perimeter(&mut self, p: &PerimeterSpec, sc: &mut scene::Scene, at: &PerimeterParams, drawn: &PerimeterParams, named: bool) -> Result<PerimeterNumbers, String> {
        let before = (self.m.nodes.len(), self.m.members.len(), self.m.groups.len());
        let snapshot = named.then(|| self.clone());
        let (parts, walls) = self.build_perimeter(p, at)?;
        self.m.group_mut(parts.group).parent = Some(self.root);
        // Every bottom plate bears on the foundation.
        let mut nodes: Vec<NodeId> = parts.bottom_plates().iter().flat_map(|&b| self.m.member(b).path.clone()).collect();
        nodes.sort();
        nodes.dedup();
        for node in nodes {
            self.m.supports.push(Support { node, restraint: Restraint::PINNED, description: Some("Bottom plate on foundation".into()) });
        }
        let content = scene::Content {
            group: parts.group,
            groups: (before.2..self.m.groups.len()).map(|g| GroupId(g as u32)).collect(),
            nodes: (before.0..self.m.nodes.len()).map(|n| NodeId(n as u32)).collect(),
            members: (before.1..self.m.members.len()).map(|m| MemberId(m as u32)).collect(),
        };
        // (Its body is where it is drawn from: the numbers are in it.)
        let origin = drawn.corners[0];
        let body = sc.body_written(&p.name, Frame { origin, ..Frame::WORLD }, p.free, content.clone());
        let mut numbers = PerimeterNumbers::default();
        // Corners.
        for (k, (c, d)) in at.corners.iter().zip(&drawn.corners).enumerate() {
            let name = format!("{}/corner {}", p.name, k + 1);
            let xyz: [scene::ScalarId; 3] = std::array::from_fn(|i| sc.number(&format!("{name}: {}", ["x", "y", "z"][i]), body, [c.x, c.y, c.z][i] - [origin.x, origin.y, origin.z][i]));
            // (A free perimeter is placed by its body, whose origin is its start: there, the corner is
            // where it is drawn in the body, held. Else where a corner is drawn is a hint.)
            let held = if p.free && k == 0 { scene::Tier::Hard } else { scene::Tier::Hint };
            for (i, &s) in xyz.iter().enumerate() {
                let axis = ["x", "y", "z"][i];
                sc.number_is(&format!("{name} where it is drawn: {axis}"), s, [d.x, d.y, d.z][i] - [origin.x, origin.y, origin.z][i], held);
            }
            let terms = [(xyz[0], Vec3::X), (xyz[1], Vec3::Y), (xyz[2], Vec3::Z)];
            let at_corner = content.nodes.iter().copied().find(|&n| self.m.pos(n).distance(*c) < 1e-9);
            match at_corner {
                Some(n) => sc.node_built(&name, body, *c - origin, &terms, n)?,
                None => sc.node_with(&name, body, Vec3::ZERO, &terms)?,
            };
            numbers.corners.push(xyz);
        }
        // Heights at each wall's ends; one at a corner where they are one as drawn.
        for (i, w) in walls.iter().enumerate() {
            let Some(w) = w else {
                numbers.heights.push(None);
                continue;
            };
            let (h0, h1) = at.heights[i].unwrap_or((w.height, w.height_end.unwrap_or(w.height)));
            let (d0, d1) = drawn.heights[i].unwrap_or((h0, h1));
            let name = format!("{}/{}", p.name, w.name);
            let s0 = sc.number(&format!("{name}: height at its start"), body, h0);
            let s1 = sc.number(&format!("{name}: height at its end"), body, h1);
            sc.number_is(&format!("{name}: height at its start as drawn"), s0, d0, scene::Tier::Hint);
            sc.number_is(&format!("{name}: height at its end as drawn"), s1, d1, scene::Tier::Hint);
            numbers.heights.push(Some((s0, s1)));
        }
        let nc = at.corners.len();
        for (a, b) in self.wall_corners(p) {
            let (Some(ha), Some(hb), Some(da), Some(db)) = (numbers.heights[a], numbers.heights[b], drawn.heights[a], drawn.heights[b]) else { continue };
            if (da.1 - db.0).abs() < 1e-9 {
                sc.equal(&format!("{}/corner {}: the walls are one height there", p.name, b % nc + 1), ha.1.into(), hb.0.into(), scene::Tier::Hard);
            }
        }
        // Studs; and a surveyed wall's posts.
        for (i, (w, wp)) in walls.iter().zip(&parts.walls).enumerate() {
            if let (Some(w), Some(_)) = (w, wp) {
                if let Some(survey) = &w.survey {
                    numbers.studs.push(Some(self.surveyed(p, w, survey, sc, body, &numbers, i, nc)?));
                    continue;
                }
            }
            let (Some(w), Some(wp)) = (w.as_ref().filter(|w| w.survey.is_none()), wp) else {
                numbers.studs.push(None);
                continue;
            };
            let commons: Vec<MemberId> = wp.studs.iter().copied().filter(|&id| self.m.member(id).role == "stud").collect();
            let planned = Wall { studs_solved: None, ..w.clone() }.common_studs();
            if commons.len() != planned.len() {
                numbers.studs.push(None);
                continue;
            }
            let len = w.length();
            // (Along the wall on plan, from its start corner to its end corner.)
            let u = Vec2::new(w.end.x - w.start.x, w.end.y - w.start.y) / len;
            let (ca, cb) = (numbers.corners[i], numbers.corners[(i + 1) % nc]);
            let b = topo_core::units::inch(topo_timber::lumber::dressed_in(w.stud.0));
            let built: Vec<f64> = w.studs_solved.clone().unwrap_or_else(|| planned.iter().map(|x| x.0).collect());
            let mut ss = vec![];
            for (k, (&(layout, from), &x)) in planned.iter().zip(&built).enumerate() {
                let name = format!("{}/{}/stud#{}", p.name, w.name, k + 1);
                let s = sc.number(&name, body, x);
                match from {
                    topo_timber::StudFrom::End if k == 0 => sc.number_is(&format!("{name} at the start of the wall"), s, layout, scene::Tier::Hard),
                    topo_timber::StudFrom::End => {
                        // As far from the end corner as the layout has it, wherever the corners are.
                        let from_end = scene::Num::from(s).plus(cb[0], -u.x).plus(cb[1], -u.y).plus(ca[0], u.x).plus(ca[1], u.y);
                        sc.equal(&format!("{name} at the end of the wall"), from_end, (layout - len).into(), scene::Tier::Hard);
                    }
                    topo_timber::StudFrom::Layout => sc.number_is(&format!("{name} where the layout puts it"), s, layout, scene::Tier::Fallback),
                    topo_timber::StudFrom::Read => {
                        sc.number_is(&format!("{name} where the layout puts it"), s, layout, scene::Tier::Fallback);
                        sc.number_is(&format!("{name} where it was read to be"), s, layout, scene::Tier::Reading(0.001));
                    }
                }
                // (In order, from the one at the start to the one at the end: so in the wall,
                // wherever its corners are.)
                if let Some(&before) = ss.last() {
                    sc.at_least(&format!("{name} clear of the stud before it"), s, before, b);
                }
                ss.push(s);
            }
            numbers.studs.push(Some(ss));
        }
        // Nodes it built, as they move with the numbers: built again with each moved a little
        // (its studs all at once: each stud's ends move with its own number only).
        if let Some(snapshot) = snapshot {
            const EPS: f64 = 1e-4;
            let built: Vec<Vec3> = content.nodes.iter().map(|&n| self.m.pos(n)).collect();
            let again = |moved: &PerimeterParams| -> Option<Vec<Vec3>> {
                let mut b2 = snapshot.clone();
                b2.build_perimeter(p, moved).ok()?;
                // (Built the same way: the same nodes, in the same order.)
                (b2.m.nodes.len() == before.0 + built.len()).then(|| content.nodes.iter().map(|&n| b2.m.pos(n)).collect())
            };
            let mut terms: HashMap<NodeId, Vec<(scene::ScalarId, Vec3)>> = HashMap::new();
            let mut column = |moved: &PerimeterParams, who: &dyn Fn(NodeId) -> Option<scene::ScalarId>| {
                let Some(now) = again(moved) else { return };
                for (k, &n) in content.nodes.iter().enumerate() {
                    let d = (now[k] - built[k]) / EPS;
                    if d.norm() > 1e-6 {
                        if let Some(s) = who(n) {
                            terms.entry(n).or_default().push((s, d));
                        }
                    }
                }
            };
            for k in 0..nc {
                for i in 0..3 {
                    let mut moved = at.clone();
                    let c = &mut moved.corners[k];
                    *[&mut c.x, &mut c.y, &mut c.z][i] += EPS;
                    column(&moved, &|_| Some(numbers.corners[k][i]));
                }
            }
            for (w, h) in numbers.heights.iter().enumerate() {
                let Some((s0, s1)) = *h else { continue };
                let wall = walls[w].as_ref().unwrap();
                let now = at.heights[w].unwrap_or((wall.height, wall.height_end.unwrap_or(wall.height)));
                for (end, s) in [(0, s0), (1, s1)] {
                    let mut moved = at.clone();
                    let mut hh = now;
                    if end == 0 { hh.0 += EPS } else { hh.1 += EPS }
                    moved.heights[w] = Some(hh);
                    column(&moved, &|_| Some(s));
                }
            }
            let mut moved = at.clone();
            let mut stud_of: HashMap<NodeId, scene::ScalarId> = HashMap::new();
            for (w, (ss, wp)) in numbers.studs.iter().zip(&parts.walls).enumerate() {
                let (Some(ss), Some(wp)) = (ss, wp) else { continue };
                if let Some(wall) = walls[w].as_ref().filter(|x| x.survey.is_some()) {
                    // (A surveyed wall's posts: each node of its upright members goes with the nearest.)
                    let u = (wall.end - wall.start).normalized();
                    let xs: Vec<f64> = ss.iter().map(|&s| sc.value(s)).collect();
                    for &id in &wp.studs {
                        for n in [self.m.member(id).start(), self.m.member(id).end()] {
                            let x = (self.m.pos(n) - wall.start).dot(u);
                            if let Some(k) = (0..xs.len()).min_by(|&a, &b| (xs[a] - x).abs().total_cmp(&(xs[b] - x).abs())) {
                                stud_of.insert(n, ss[k]);
                            }
                        }
                    }
                    moved.studs[w] = Some(xs.iter().map(|x| x + EPS).collect());
                    continue;
                }
                let commons: Vec<MemberId> = wp.studs.iter().copied().filter(|&id| self.m.member(id).role == "stud").collect();
                let mut xs = vec![];
                for (&id, &s) in commons.iter().zip(ss) {
                    for n in [self.m.member(id).start(), self.m.member(id).end()] {
                        stud_of.insert(n, s);
                    }
                    xs.push(sc.value(s) + EPS);
                }
                moved.studs[w] = Some(xs);
            }
            column(&moved, &|n| stud_of.get(&n).copied());
            for (n, t) in terms {
                self.moving.insert(n, (body, origin, t));
            }
        }
        let top = walls.iter().flatten().map(|w| w.start.z + w.height.max(w.height_end.unwrap_or(w.height))).fold(drawn.corners[0].z, f64::max);
        self.perimeters.insert(p.name.clone(), (parts, top));
        Ok(numbers)
    }

    /// A surveyed wall's posts, as numbers of the perimeter (centres along the wall from its start,
    /// in the order the survey lists them): each starts where its reading puts it (or where the
    /// solve last put it); each reading, and each check, is a reading of them (and of the wall's
    /// length, in its corners' numbers); they keep their order (allowing 1/8" of slop).
    #[allow(clippy::too_many_arguments)]
    fn surveyed(&self, p: &PerimeterSpec, w: &Wall, survey: &topo_timber::Survey, sc: &mut scene::Scene, body: scene::BodyRef, numbers: &PerimeterNumbers, i: usize, nc: usize) -> Result<Vec<scene::ScalarId>, String> {
        use topo_timber::survey::{ResolvedItem, WallContext};
        let len = w.length();
        let ply = topo_core::units::inch(topo_timber::lumber::dressed_in(w.stud.0));
        let u = Vec2::new(w.end.x - w.start.x, w.end.y - w.start.y) / len;
        let cx = WallContext { len, ply, inside: w.corner_inside, dir: [u.x, u.y] };
        let r = survey.resolve_at(&w.name, &cx, w.survey_solved.as_deref())?;
        let posts: Vec<&topo_timber::survey::Post> = r.items.iter().filter_map(|it| if let ResolvedItem::Post(q) = it { Some(q) } else { None }).collect();
        let mut seen: HashMap<&str, usize> = HashMap::new();
        let mut ss = vec![];
        let mut of_item = HashMap::new();
        for q in &posts {
            let role = q.role.role();
            let k = seen.entry(role).or_insert(0);
            *k += 1;
            let name = format!("{}/{}/{}", p.name, w.name, q.name.clone().unwrap_or_else(|| format!("{role}#{k}")));
            of_item.insert(q.item, ss.len());
            ss.push(sc.number(&name, body, q.x));
        }
        let (ca, cb) = (numbers.corners[i], numbers.corners[(i + 1) % nc]);
        for rel in survey.relations(&w.name, &cx)? {
            let mut num = scene::Num::from(0.0);
            for &(item, k) in &rel.posts {
                num = num.plus(ss[of_item[&item]], k);
            }
            if rel.len != 0.0 {
                // (The wall's length on plan, in its corners' numbers.)
                num = num.plus(cb[0], rel.len * u.x).plus(cb[1], rel.len * u.y).plus(ca[0], -rel.len * u.x).plus(ca[1], -rel.len * u.y);
            }
            sc.equal(&format!("{}/{}: {}", p.name, w.name, rel.what), num, rel.value.into(), scene::Tier::Reading(rel.tolerance));
        }
        let mut order: Vec<usize> = (0..posts.len()).collect();
        order.sort_by(|&a, &b| posts[a].x.total_cmp(&posts[b].x));
        for pair in order.windows(2) {
            let (a, b) = (posts[pair[0]], posts[pair[1]]);
            let gap = (a.width(ply) + b.width(ply)) / 2.0 - topo_core::units::inch(0.125);
            sc.at_least(&format!("{}/{}: post {} clear of post {}", p.name, w.name, pair[1] + 1, pair[0] + 1), ss[pair[1]], ss[pair[0]], gap);
        }
        Ok(ss)
    }

    /// The corners where two walls of a perimeter meet: (incoming side, outgoing side).
    fn wall_corners(&self, p: &PerimeterSpec) -> Vec<(usize, usize)> {
        let ns = p.segments.len();
        let corners: Vec<(usize, usize)> = if p.closed { (0..ns).map(|k| ((k + ns - 1) % ns, k)).collect() } else { (1..ns).map(|k| (k - 1, k)).collect() };
        corners.into_iter().filter(|&(a, b)| p.segments[a].side.is_some() && p.segments[b].side.is_some()).collect()
    }

    /// A node a generator built, as a node of the scene (`name`): if it moves with numbers of
    /// the scene, made of them; else fixed in the body it is in (or the world).
    fn bind(&self, sc: &mut scene::Scene, name: &str, n: NodeId) -> Result<scene::NodeRef, String> {
        match self.moving.get(&n) {
            Some((body, origin, terms)) => sc.node_built(name, *body, self.m.pos(n) - *origin, terms, n),
            None => sc.import_node(name, &self.m, n),
        }
    }

    /// A point `at` along a member a generator built, as `bind` (moving as its ends do).
    fn bind_along(&self, sc: &mut scene::Scene, name: &str, id: MemberId, at: f64) -> Result<scene::NodeRef, String> {
        sc.node_along(name, &self.m, id, at, &self.moving_at(id, at))
    }

    /// A point on a face of a member a generator built (the face facing most nearly `toward`),
    /// `at` along it, moving as the member's ends do.
    fn bind_on_face(&self, sc: &mut scene::Scene, name: &str, id: MemberId, toward: Vec3, at: f64) -> Result<scene::NodeRef, String> {
        sc.node_on_face(name, &self.m, id, toward, at, &self.moving_at(id, at))
    }

    /// How a point `at` along a member moves with the numbers its ends move with (none, for a
    /// member not built at numbers).
    fn moving_at(&self, id: MemberId, at: f64) -> Vec<(scene::ScalarId, Vec3)> {
        let mm = self.m.member(id);
        let (a, b) = (mm.start(), mm.end());
        let f = at / self.m.member_length(id).max(1e-12);
        let mut terms: Vec<(scene::ScalarId, Vec3)> = vec![];
        for (n, k) in [(a, 1.0 - f), (b, f)] {
            if let Some((_, _, t)) = self.moving.get(&n) {
                terms.extend(t.iter().map(|&(s, d)| (s, d * k)));
            }
        }
        terms
    }

    /// A member's path in the building (as nodes are named: `Walls/Front/cap_plate`).
    fn path(&self, id: MemberId) -> String {
        let full = self.m.member_path(id);
        full.strip_prefix(&format!("{}/", self.m.groups[self.root.idx()].name)).map(String::from).unwrap_or(full)
    }

    /// A truss roof, in the scene: its trusses free bodies of it, set out from a node of it
    /// (`at`), sat by `TrussRoof::place` on what the script says it bears on.
    fn place_roof(&mut self, r: &TrussRoofSpec, sc: &mut scene::Scene, set_out: scene::NodeRef, declared: &[(&AssemblySpec, Declared)]) -> Result<topo_timber::truss::PlacedRoof, String> {
        let mat = self.material(&r.grade)?;
        r.shape.validate()?;
        for o in &r.overrides {
            o.shape.validate()?;
        }
        // What the script says it bears on: a perimeter's cap plates, an assembly's horizontal
        // members (beams, headers). Nothing else is looked for.
        let mut bearings = vec![];
        for name in &r.bears_on {
            if let Some((parts, _)) = self.perimeters.get(name) {
                for p in parts.cap_plates() {
                    bearings.push(sc.import_member(&self.m, p, &self.path(p)));
                }
            } else if let Some((_, d)) = declared.iter().find(|(a, _)| a.name == *name) {
                bearings.extend(d.members.iter().copied().filter(|&m| sc.axis(m).1.z.abs() < 1e-6));
            } else {
                return Err(format!("roof {}: nothing named {name} to bear on (a perimeter or an assembly)", r.name));
            }
        }
        if bearings.is_empty() {
            return Err(format!("roof {}: say what it bears on, with bearingOn(...): its height is that of what is under it", r.name));
        }
        let span = Vec3::new(r.span_dir.0, r.span_dir.1, 0.0).try_normalized().ok_or("roof span direction is zero")?;
        let mut roof = TrussRoof::new(&r.name, Vec3::ZERO, span, r.shape.clone(), r.length, r.spacing, mat);
        roof.chord = r.chord;
        roof.web = r.web;
        for o in &r.overrides {
            roof.overrides.push((o.trusses.clone(), o.shape.clone()));
        }
        for s in &r.shifts {
            roof.shifts.push((s.trusses.clone(), s.along));
        }
        let placed = roof.place(&mut self.m, sc, set_out, &bearings)?;
        self.m.group_mut(placed.parts.group).parent = Some(self.root);
        Ok(placed)
    }

    /// A roof the scene has placed: each contact fastened, what was said about its trusses, its loads.
    fn finish_roof(&mut self, r: &TrussRoofSpec, placed: &topo_timber::truss::PlacedRoof, w: &scene::Written) -> Result<(), String> {
        let roof = &placed.parts;
        // (The roof's frame, which its drawings are given in, is its first truss's, where it now is.)
        if let Some(t) = roof.trusses.first() {
            let f = self.m.groups[t.group.idx()].frame;
            let run = Vec3::new(r.span_dir.1, -r.span_dir.0, 0.0).normalized();
            self.m.group_mut(roof.group).frame = Frame { origin: f.origin, x: run, y: f.x, z: run.cross(f.x) };
        }
        self.m.issues.extend(placed.issues.iter().cloned());
        // Each truss is fastened to what it sits on, at the node where they touch.
        let c = self.m.add_connection(fx::truss_to_plate());
        for &(n, chord, on) in &placed.rests {
            self.m.connect(w.nodes[n.0], chord, Some(w.members[on.0]), c);
        }
        for l in &r.loads {
            let case = self.case(&l.kind)?;
            let basis = match l.basis.as_deref() {
                None | Some("plan") => AreaBasis::Plan,
                Some("surface") => AreaBasis::Surface,
                Some(b) => return Err(format!("roof {}: unknown load basis {b} (plan or surface)", r.name)),
            };
            if let Some(layers) = &l.layers {
                let region = if l.on.as_deref() == Some("flat") { topo_core::SurfaceRegion::Flat } else { topo_core::SurfaceRegion::Slope };
                let name = format!("{}: {}", r.name, if region == topo_core::SurfaceRegion::Flat { "flat roof" } else { "sloped roof" });
                self.m.surfaces.push(topo_core::Surface { name, group: roof.group, region, layers: layers.clone() });
            }
            match l.on.as_deref() {
                None | Some("slope") => {
                    self.m.loads.push(Load::Area { case, group: roof.group, pressure: l.pressure, direction: -Vec3::Z, basis, label: l.label.clone() });
                }
                Some("flat") => {
                    // Per truss, the bottom chord beyond the nodes it shares
                    // with a top chord, at the truss's tributary width.
                    let n = roof.trusses.len();
                    for (i, t) in roof.trusses.iter().enumerate() {
                        let trib = if n == 1 { r.spacing } else if i == 0 || i + 1 == n { r.spacing / 2.0 } else { r.spacing };
                        let top: std::collections::HashSet<NodeId> = t.top_chords.iter().flat_map(|&m| self.m.member(m).path.clone()).collect();
                        for &bc in &t.bottom_chords {
                            let path = self.m.member(bc).path.clone();
                            let (a, b) = (self.m.pos(path[0]), self.m.pos(*path.last().unwrap()));
                            let len = a.distance(b);
                            let x = (b - a) / len;
                            let ts: Vec<f64> = path.iter().filter(|n| top.contains(n)).map(|&n| (self.m.pos(n) - a).dot(x)).collect();
                            if ts.is_empty() {
                                continue;
                            }
                            let (lo, hi) = (ts.iter().copied().fold(f64::INFINITY, f64::min), ts.iter().copied().fold(f64::NEG_INFINITY, f64::max));
                            for range in [(0.0, lo), (hi, len)] {
                                if range.1 - range.0 > 1e-3 {
                                    self.m.loads.push(Load::MemberUniform { case, member: bc, w: -Vec3::Z * (l.pressure * trib), range: Some(range) });
                                }
                            }
                        }
                    }
                }
                Some(o) => return Err(format!("roof {}: unknown load placement {o} (slope or flat)", r.name)),
            }
        }
        Ok(())
    }
}

impl Builder {
    /// An assembly, into the scene: a body (fixed at its placement, or free), a node for each
    /// point, its members between them, and a node for each member resting on another.
    fn declare(&mut self, sc: &mut scene::Scene, a: &AssemblySpec) -> Result<Declared, String> {
        let f = a.placement.frame();
        let body = sc.body(&a.name, "assembly", f, a.free, Some(self.root));
        let mut points = HashMap::new();
        for p in &a.points {
            if points.contains_key(&p.name) {
                return Err(format!("assembly {}: duplicate point {}", a.name, p.name));
            }
            points.insert(p.name.clone(), sc.node(&format!("{}/{}", a.name, p.name), body, v3(p.at[0], p.at[1], p.at[2]))?);
        }
        let mut members = vec![];
        for (i, mm) in a.members.iter().enumerate() {
            let path: Vec<scene::NodeRef> = mm.path.iter().map(|n| points.get(n).copied().ok_or(format!("assembly {}: member {i} uses unknown point {n}", a.name))).collect::<Result<_, _>>()?;
            if path.len() < 2 {
                return Err(format!("assembly {}: member {i} needs two points", a.name));
            }
            let sec = if mm.plies > 1 { built_up(mm.plies, mm.size.0, mm.size.1, 0.0) } else { sawn(mm.size.0, mm.size.1) };
            let sec = self.m.add_section(sec);
            let mat = self.material(&mm.grade)?;
            // Depth direction in the assembly's own frame (default: up, or +y for vertical
            // members); it is placed with the assembly, so a rotated copy is identical to the original.
            let (la, lb) = (&a.points.iter().find(|p| p.name == mm.path[0]).unwrap().at, &a.points.iter().find(|p| p.name == *mm.path.last().unwrap()).unwrap().at);
            let local_axis = v3(lb[0] - la[0], lb[1] - la[1], lb[2] - la[2]);
            let local_depth = match mm.depth {
                Some(d) => v3(d[0], d[1], d[2]),
                None if local_axis.try_normalized().is_some_and(|x| x.dot(Vec3::Z).abs() > 0.99) => Vec3::Y,
                None => Vec3::Z,
            };
            let mut spec = MemberSpec::new(&mm.role, sec, mat).priority(mm.priority).depth_dir(local_depth);
            if let Some((u, v)) = mm.anchor {
                spec = spec.anchor(Anchor { u, v, offset: Vec2::ZERO });
            }
            let label = format!("{}/{}#{}", a.name, mm.role, a.members[..i].iter().filter(|o| o.role == mm.role).count() + 1);
            members.push(sc.member(body, &path, spec, &label).map_err(|e| format!("assembly {}: {e}", a.name))?);
        }
        let find = |sel: &str| -> Result<usize, String> {
            let (role, index) = match sel.split_once('#') {
                Some((r, i)) => (r, i.parse::<usize>().map_err(|_| format!("assembly {}: bad member {sel}", a.name))?),
                None => (sel, 1),
            };
            (0..a.members.len()).filter(|&k| a.members[k].role == role).nth(index - 1).ok_or(format!("assembly {}: no member {sel}", a.name))
        };
        let mut rests = vec![];
        for (upper, lower) in &a.rests {
            let (u, l) = (find(upper)?, find(lower)?);
            let n = sc.rest(&format!("{}/{upper} on {lower}", a.name), members[u], members[l], &self.m)?;
            rests.push((n, members[u], members[l]));
        }
        // `bearingOn`: a utility the script calls. Each of its members that crosses a cap plate of
        // these perimeters (on plan) rests on it: a node on both, as for `resting`. They have to touch.
        let f = a.placement.frame();
        for name in &a.bears_on {
            let (parts, _) = self.perimeters.get(name).ok_or_else(|| format!("assembly {}: no perimeter named {name}", a.name))?;
            for plate in parts.cap_plates() {
                let (pa, pb) = (self.m.pos(self.m.member(plate).start()), self.m.pos(self.m.member(plate).end()));
                for (i, mm) in a.members.iter().enumerate() {
                    let at = |n: &String| a.points.iter().find(|p| p.name == *n).map(|p| f.to_world(v3(p.at[0], p.at[1], p.at[2]))).unwrap();
                    let (ma, mb) = (at(&mm.path[0]), at(mm.path.last().unwrap()));
                    if crosses_on_plan((ma, mb), (pa, pb)) {
                        let lower = sc.import_member(&self.m, plate, &self.m.member_path(plate));
                        let n = sc.rest(&format!("{}/{}#{} on {}", a.name, mm.role, a.members[..i].iter().filter(|o| o.role == mm.role).count() + 1, self.m.member_path(plate)), members[i], lower, &self.m)?;
                        rests.push((n, members[i], lower));
                    }
                }
            }
        }
        Ok(Declared { points, members, rests })
    }

    /// An assembly, once the scene is written: its fastenings, supports and loads, by its model ids.
    fn finish(&mut self, a: &AssemblySpec, d: &Declared, w: &scene::Written) -> Result<(), String> {
        let ids: Vec<MemberId> = d.members.iter().map(|m| w.members[m.0]).collect();
        let nodes: HashMap<&str, NodeId> = d.points.iter().map(|(k, n)| (k.as_str(), w.nodes[n.0])).collect();
        self.assemblies.insert(a.name.clone(), ids.clone());
        // Face-to-face bonds (a cap nailed on a header, plies of a post).
        let find = |sel: &str| -> Result<MemberId, String> {
            let (role, index) = match sel.split_once('#') {
                Some((r, i)) => (r, i.parse::<usize>().map_err(|_| format!("assembly {}: bad member {sel}", a.name))?),
                None => (sel, 1),
            };
            ids.iter().copied().enumerate().filter(|(k, _)| a.members[*k].role == role).map(|(_, id)| id).nth(index - 1).ok_or(format!("assembly {}: no member {sel}", a.name))
        };
        for (x, y) in &a.bonds {
            let (mx, my) = (find(x)?, find(y)?);
            let c = self.m.add_connection(fx::double_top_plate());
            self.m.bond(mx, my, c);
        }
        // Where one rests on another: fastened at that node.
        for &(n, u, l) in &d.rests {
            let c = self.m.add_connection(fx::joist_to_plate());
            self.m.connect(w.nodes[n.0], w.members[u.0], Some(w.members[l.0]), c);
        }
        let conn = match &a.joints {
            JointsSpec::Kind(k) => match k.as_str() {
                "nailed" => Some(self.m.add_connection(fx::plate_corner())),
                "truss_plate" => Some(self.m.add_connection(fx::truss_plate())),
                "none" => None,
                j => return Err(format!("assembly {}: unknown joint kind {j}", a.name)),
            },
            JointsSpec::Nails { nails, penny, method } => {
                let method = match method.as_str() {
                    "end" => FastenMethod::EndNail,
                    "toe" => FastenMethod::ToeNail,
                    "face" => FastenMethod::FaceNail,
                    m => return Err(format!("assembly {}: unknown nailing method {m} (end, toe, face)", a.name)),
                };
                if ![6, 8, 10, 12, 16, 20].contains(penny) {
                    return Err(format!("assembly {}: no {penny}d common nail", a.name));
                }
                Some(self.m.add_connection(fx::nails(&format!("{} joints", a.name), *penny, *nails, method)))
            }
        };
        if let Some(c) = conn {
            for &id in &ids {
                for end in [self.m.member(id).start(), self.m.member(id).end()] {
                    if let Some(&to) = ids.iter().find(|&&o| o != id && self.m.member(o).path.contains(&end)) {
                        self.m.connect(end, id, Some(to), c);
                    }
                }
            }
        }
        for name in &a.supports {
            let node = *nodes.get(name.as_str()).ok_or(format!("assembly {}: unknown support point {name}", a.name))?;
            self.m.supports.push(Support { node, restraint: Restraint::PINNED, description: Some(format!("{} on foundation", a.name)) });
        }
        for l in &a.loads {
            let case = self.case(&l.kind)?;
            match (&l.member, &l.point) {
                (Some(sel), None) => {
                    // `beam` (every member with that role) or `beam#2`.
                    let (role, index) = match sel.split_once('#') {
                        Some((r, i)) => (r, Some(i.parse::<usize>().map_err(|_| format!("assembly {}: bad member {sel}", a.name))?)),
                        None => (sel.as_str(), None),
                    };
                    let hits: Vec<MemberId> = ids.iter().copied().enumerate().filter(|(k, _)| a.members[*k].role == role).map(|(_, id)| id).collect();
                    let chosen: Vec<MemberId> = match index {
                        Some(i) => hits.get(i.wrapping_sub(1)).copied().into_iter().collect(),
                        None => hits,
                    };
                    if chosen.is_empty() {
                        return Err(format!("assembly {}: no member {sel} to load", a.name));
                    }
                    for m in chosen {
                        self.m.loads.push(Load::MemberUniform { case, member: m, w: -Vec3::Z * l.w, range: None });
                    }
                }
                (None, Some(pt)) => {
                    let node = *nodes.get(pt.as_str()).ok_or(format!("assembly {}: unknown load point {pt}", a.name))?;
                    self.m.loads.push(Load::Node { case, node, force: -Vec3::Z * l.force, moment: Vec3::ZERO });
                }
                _ => return Err(format!("assembly {}: a load needs a member or a point", a.name)),
            }
        }
        Ok(())
    }
}

/// Whether two members' axes cross on plan, within both.
fn crosses_on_plan(a: (Vec3, Vec3), b: (Vec3, Vec3)) -> bool {
    let (d1, d2, w) = (a.1 - a.0, b.1 - b.0, b.0 - a.0);
    let den = d1.x * d2.y - d1.y * d2.x;
    if den.abs() < 1e-12 {
        return false;
    }
    let (s, t) = ((w.x * d2.y - w.y * d2.x) / den, (w.x * d1.y - w.y * d1.x) / den);
    (-1e-9..=1.0 + 1e-9).contains(&s) && (-1e-9..=1.0 + 1e-9).contains(&t)
}

/// Expands a scene spec into a model. Each building is built in its own
/// frame (under a root group named after it) and then placed in the world.
/// A script's unknowns, as numbers of the one solve, and its measurements as readings of them:
/// each as it is about the unknowns' values now (`constant + Σ k u`), to its tolerance.
#[derive(Clone, Debug, Default)]
pub struct Fit {
    pub unknowns: Vec<(String, f64, Option<f64>, Option<f64>)>,
    pub readings: Vec<FitReading>,
}

#[derive(Clone, Debug)]
pub struct FitReading {
    pub name: String,
    pub terms: Vec<(usize, f64)>,
    pub constant: f64,
    pub measured: f64,
    pub tolerance: f64,
}

pub fn build_scene(spec: &SceneSpec) -> Result<Model, String> {
    build_scene_fitted(spec, None).map(|(m, _)| m)
}

/// Builds the scene, with a fit's unknowns and readings in the first building's solve; and the
/// unknowns as solved.
pub fn build_scene_fitted(spec: &SceneSpec, fit: Option<&Fit>) -> Result<(Model, Option<Vec<f64>>), String> {
    let mut fitted = None;
    let mut m = Model::new(&spec.name);
    let i = &spec.info;
    m.info.number = i.number.clone();
    m.info.client = i.client.clone();
    m.info.address = i.address.clone();
    m.info.designer = i.designer.clone();
    m.info.date = i.date.clone();
    m.info.design_basis = i.design_basis.clone();
    m.info.notes = i.notes.clone();
    // Sheets: the site's list if it has one (plus the buildings' own custom
    // sheets), otherwise the buildings' lists merged.
    let mut sheets = spec.sheets.clone();
    for (i, bs) in spec.buildings.iter().enumerate() {
        let (mut local, f) = build_building(bs, if i == 0 { fit } else { None })?;
        fitted = fitted.or(f);
        if let Some(p) = &bs.placement {
            local.transform(&p.frame());
        }
        m.absorb(local);
        let own = bs.sheets.clone();
        sheets = match (&spec.sheets, i) {
            (Some(_), _) => {
                let mut s = sheets.unwrap_or_default();
                s.extend(own.unwrap_or_default().into_iter().filter(|x| !matches!(x, SheetSpec::Standard { .. })));
                Some(s)
            }
            (None, 0) => own,
            (None, _) => merge_sheets(sheets, own),
        };
    }
    for s in sheets.iter().flatten() {
        if let SheetSpec::Standard { which } = s {
            if !STANDARD_SHEETS.contains(&which.as_str()) {
                return Err(format!("unknown standard sheets \"{which}\" (one of {})", STANDARD_SHEETS.join(", ")));
            }
        }
    }
    m.sheets = sheets;
    if let Some(e) = &spec.asce7 {
        if e != "7-16" && e != "7-22" {
            return Err(format!("unknown ASCE 7 edition {e} (7-16 or 7-22)"));
        }
        m.standards.asce7 = e.clone();
    }
    if let Some(c) = &spec.combinations {
        if c != "ubc-1976" {
            return Err(format!("unknown load combinations {c} (ubc-1976, or leave out for ASCE 7)"));
        }
        m.standards.combinations = Some(c.clone());
    }
    m.scenarios = Scenarios { current: spec.scenario.clone(), available: spec.scenarios.clone() };
    if spec.self_weight == Some(false) {
        for c in &mut m.load_cases {
            c.self_weight = false;
        }
    }
    Ok((m, fitted))
}

fn build_building(spec: &BuildingSpec, fit: Option<&Fit>) -> Result<(Model, Option<Vec<f64>>), String> {
    // Perimeters are built at numbers of the one solve: as drawn first; then again at the
    // numbers as solved, and solved again, until they do not change.
    let drawn: Vec<PerimeterParams> = spec.items.iter().filter_map(|i| if let ItemSpec::Perimeter(p) = i { Some(PerimeterParams::drawn(p)) } else { None }).collect();
    let mut at = drawn.clone();
    const PASSES: usize = 8;
    for pass in 1..=PASSES {
        let mut built = build_pass(spec, &at, &drawn, fit)?;
        let moved = built.solved.iter().zip(&at).map(|(a, b)| a.distance(b)).fold(0.0, f64::max);
        if moved < 1e-7 || pass == PASSES {
            if moved >= 1e-7 {
                built.b.m.issues.push(Issue::new(Severity::Warning, "unsettled", format!("the perimeters' numbers still moved {:.2} mm when built again at them for the {PASSES}th time", moved * 1000.0)));
            }
            return finish(built);
        }
        at = built.solved;
    }
    unreachable!()
}

/// A building built once (its perimeters at some numbers), solved and written.
struct Pass<'a> {
    b: Builder,
    written: scene::Written,
    declared: Vec<(&'a AssemblySpec, Declared)>,
    roofs: Vec<(&'a TrussRoofSpec, topo_timber::truss::PlacedRoof)>,
    /// The perimeters' numbers as solved.
    solved: Vec<PerimeterParams>,
    /// A fit's unknowns: their numbers.
    fit: Option<Vec<scene::ScalarId>>,
}

/// Its assemblies and roofs, on what the solve placed; and a fit's unknowns as solved.
fn finish(pass: Pass) -> Result<(Model, Option<Vec<f64>>), String> {
    let Pass { mut b, written, declared, roofs, fit, .. } = pass;
    let values: HashMap<scene::ScalarId, f64> = written.values.iter().copied().collect();
    let fitted = fit.map(|ss| ss.iter().map(|s| values[s]).collect());
    for open in &written.open {
        b.m.issues.push(Issue::new(Severity::Warning, "open", format!("nothing places this: {open}")));
    }
    for (what, off, tolerance) in &written.off {
        b.m.issues.push(Issue::new(Severity::Warning, "reading-off", format!("{what}: off by {:.1} times what it was read to: it does not agree with the rest", off.abs() / tolerance)));
    }
    for (a, d) in &declared {
        b.finish(a, d, &written)?;
    }
    for (r, placed) in &roofs {
        b.finish_roof(r, placed, &written)?;
    }
    b.m.compact_nodes();
    Ok((b.m, fitted))
}

/// Builds the building once, its perimeters at `at` (drawn as `drawn`), and solves and writes it.
fn build_pass<'a>(spec: &'a BuildingSpec, at: &[PerimeterParams], drawn: &[PerimeterParams], fit: Option<&Fit>) -> Result<Pass<'a>, String> {
    let mut m = Model::new(&spec.name);
    let root = m.add_group(&spec.name, "building", Frame::WORLD, None);
    for (name, v) in &spec.directions {
        let d = v3(v[0], v[1], v[2]).try_normalized().ok_or_else(|| format!("building {}: direction {name} is zero", spec.name))?;
        m.name_direction(root, name, d);
    }
    let mut b = Builder { m, moving: HashMap::new(), grades: HashMap::new(), perimeters: HashMap::new(), assemblies: HashMap::new(), root };
    // One scene: perimeters (written by their generator, as bodies of it, at numbers of it),
    // assemblies, and the relations between their nodes; solved, then written into the model.
    // Then roofs, on what it placed.
    let mut sc = scene::Scene::new("The building's parts and the relations between them");
    let perimeters: Vec<&PerimeterSpec> = spec.items.iter().filter_map(|i| if let ItemSpec::Perimeter(p) = i { Some(p) } else { None }).collect();
    let mut numbers = vec![];
    for (k, p) in perimeters.iter().enumerate() {
        // (Its built nodes are found as they move with its numbers if a relation names one.)
        let prefix = format!("{}/", p.name);
        let named = spec.items.iter().flat_map(|i| i.nodes()).any(|n| n.starts_with(&prefix) && !n[prefix.len()..].starts_with("corner "))
            || spec.items.iter().any(|i| matches!(i, ItemSpec::TrussRoof(TrussRoofSpec { at: n, .. }) if n.starts_with(&prefix)));
        numbers.push(b.perimeter(p, &mut sc, &at[k], &drawn[k], named)?);
    }
    // Frames: their points are nodes.
    let mut frames: HashMap<String, Option<scene::BodyRef>> = HashMap::new();
    for item in &spec.items {
        if let ItemSpec::Frame(f) = item {
            let body = (!f.world).then(|| sc.body(&f.name, "frame", f.placement.as_ref().map_or(Frame::WORLD, |p| p.frame()), f.free, Some(b.root)));
            for pt in &f.points {
                sc.frame_point(&format!("{}/{}", f.name, pt.name), body, pt.at)?;
            }
            frames.insert(f.name.clone(), body);
        }
    }
    let mut declared = vec![];
    for item in &spec.items {
        if let ItemSpec::Assembly(a) = item {
            declared.push((a, b.declare(&mut sc, a)?));
        }
    }
    // A node by name: one of the scene's; or a point of a member a generator wrote, brought
    // in: an end (`Walls/Front/cap_plate:start`) or so far along it (`Walls/Front/cap_plate@1.2`).
    let node = |sc: &mut scene::Scene, b: &Builder, name: &str| -> Result<scene::NodeRef, String> {
        if let Some(n) = sc.node_named(name) {
            return Ok(n);
        }
        let m = &b.m;
        let triple = |t: &str| -> Result<Vec3, String> {
            let v: Vec<f64> = t.split(',').map(|x| x.trim().parse::<f64>()).collect::<Result<_, _>>().map_err(|_| format!("{name}: {t} is not x,y,z"))?;
            match v[..] {
                [x, y, z] => Ok(v3(x, y, z)),
                _ => Err(format!("{name}: {t} is not x,y,z")),
            }
        };
        // A point of a frame on a plane of it: `Frame:plane x,y,z n nx,ny,nz` (through x,y,z, its
        // normal nx,ny,nz, in the frame's coordinates): where a face was seen in a scan.
        if let Some((frame, rest)) = name.split_once(":plane ") {
            let body = *frames.get(frame).ok_or_else(|| format!("{name}: no frame named {frame}"))?;
            let (at, n) = rest.split_once(" n ").ok_or_else(|| format!("{name}: a plane is `x,y,z n nx,ny,nz`"))?;
            return sc.frame_point_on(name, body, triple(at)?, triple(n)?);
        }
        // A point on a face of a member: `path:face x,y,z` (the face facing that way), in the middle
        // of it, or `@d` along it.
        if let Some((path, rest)) = name.split_once(":face ") {
            let (dir, at) = match rest.split_once('@') {
                Some((d, a)) => (d, Some(a.parse::<f64>().map_err(|_| format!("{name}: not a distance along it"))?)),
                None => (rest, None),
            };
            let id = m.find_member(path)?;
            let along = at.unwrap_or(m.member_length(id) / 2.0);
            return b.bind_on_face(sc, name, id, triple(dir)?, along);
        }
        if let Some((path, at)) = name.rsplit_once('@') {
            let at: f64 = at.parse().map_err(|_| format!("{name}: not a distance along {path}"))?;
            return b.bind_along(sc, name, m.find_member(path)?, at);
        }
        let end = name.rsplit_once(':').and_then(|(path, end)| {
            let id = m.find_member(path).ok()?;
            match end {
                // (A stud's bottom is its start, its top its end.)
                "start" | "bottom" => Some(m.member(id).start()),
                "end" | "top" => Some(m.member(id).end()),
                _ => None,
            }
        });
        match end {
            Some(n) => b.bind(sc, name, n),
            None => Err(format!("no node named {name} (an assembly's point, as `Assembly/point`; or a point of a member, as `Perimeter/Wall/cap_plate:start`, `:end` or `@1.2`)")),
        }
    };
    // Roofs: their trusses free bodies of the scene, set out from a node of it or a datum, sat on
    // what the script says they bear on.
    let mut roofs = vec![];
    for item in &spec.items {
        if let ItemSpec::TrussRoof(r) = item {
            let at = node(&mut sc, &b, &r.at)?;
            roofs.push((r, b.place_roof(r, &mut sc, at, &declared)?));
        }
    }
    for item in &spec.items {
        relate(&mut sc, item, &mut |sc: &mut scene::Scene, name: &str| node(sc, &b, name))?;
    }
    // A script's unknowns and its measurements of them.
    let fit = fit.map(|f| {
        let us: Vec<scene::ScalarId> = f.unknowns.iter().map(|(name, value, lo, hi)| {
            let u = sc.parameter(name, *value);
            if lo.is_some() || hi.is_some() {
                sc.bound(u, lo.unwrap_or(-1e12), hi.unwrap_or(1e12));
            }
            u
        }).collect();
        for r in &f.readings {
            let num = r.terms.iter().fold(scene::Num::from(r.constant), |n, &(k, c)| n.plus(us[k], c));
            sc.equal(&format!("measured {}", r.name), num, r.measured.into(), scene::Tier::Reading(r.tolerance));
        }
        us
    });
    let written = sc.write(&mut b.m)?;
    let values: HashMap<scene::ScalarId, f64> = written.values.iter().copied().collect();
    let solved = numbers.iter().zip(drawn).map(|(n, d)| PerimeterParams::solved(n, &values, d.corners[0])).collect();
    Ok(Pass { b, written, declared, roofs, solved, fit })
}
