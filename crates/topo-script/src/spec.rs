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
    /// Two nodes are one (`Assembly name/point`).
    Coincide { a: String, b: String },
    /// A node is at this place.
    Datum { node: String, at: [f64; 3] },
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
    pub height: f64,
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

#[derive(Debug, Deserialize)]
pub struct SegmentSpec {
    pub to: (f64, f64),
    pub side: Option<WallSpec>,
}

#[derive(Debug, Deserialize)]
pub struct PerimeterSpec {
    pub name: String,
    pub start: (f64, f64),
    pub z: f64,
    pub segments: Vec<SegmentSpec>,
    pub closed: bool,
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
    pub origin: (f64, f64),
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

struct Builder {
    m: Model,
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

    fn perimeter(&mut self, p: &PerimeterSpec) -> Result<(), String> {
        let mut per = Perimeter::start(&p.name, Vec2::new(p.start.0, p.start.1));
        per.z = p.z;
        let n = p.segments.len();
        let mut top: f64 = p.z;
        for (i, s) in p.segments.iter().enumerate() {
            let side = match &s.side {
                Some(w) => {
                    top = top.max(p.z + w.height);
                    Side::Wall(self.wall(w)?)
                }
                None => Side::Open,
            };
            if p.closed && i == n - 1 {
                per = per.close(side);
            } else {
                per = per.to(Vec2::new(s.to.0, s.to.1), side);
            }
        }
        let parts = per.try_build(&mut self.m)?;
        self.m.group_mut(parts.group).parent = Some(self.root);
        // Every bottom plate bears on the foundation.
        let mut nodes: Vec<NodeId> = parts.bottom_plates().iter().flat_map(|&b| self.m.member(b).path.clone()).collect();
        nodes.sort();
        nodes.dedup();
        for node in nodes {
            self.m.supports.push(Support { node, restraint: Restraint::PINNED, description: Some("Bottom plate on foundation".into()) });
        }
        self.perimeters.insert(p.name.clone(), (parts, top));
        Ok(())
    }

    /// The top of a member: the line along the middle of its highest face, and half that face's width.
    fn top_of(&self, id: MemberId) -> topo_timber::truss::Bearing {
        let (mm, axes) = (self.m.member(id), self.m.member_axes(id));
        let bb = self.m.section(mm.section).bbox();
        // The point of the section that is on the axis; the body's corners are placed from it.
        let on_axis = (bb.min.x + (mm.anchor.u + 0.5) * bb.width() + mm.anchor.offset.x, bb.min.y + (mm.anchor.v + 0.5) * bb.height() + mm.anchor.offset.y);
        let corners = [(bb.min.x, bb.min.y), (bb.min.x + bb.width(), bb.min.y), (bb.min.x, bb.min.y + bb.height()), (bb.min.x + bb.width(), bb.min.y + bb.height())].map(|(x, y)| axes.u * (x - on_axis.0) + axes.v * (y - on_axis.1));
        let lift = corners.iter().map(|c| c.z).fold(f64::MIN, f64::max);
        let top: Vec<Vec3> = corners.into_iter().filter(|c| c.z > lift - 1e-9).collect();
        let middle = top.iter().fold(Vec3::ZERO, |a, c| a + *c) / top.len() as f64;
        let half_width = top.iter().map(|c| (*c - middle).norm()).fold(0.0, f64::max);
        topo_timber::truss::Bearing { name: self.m.member_path(id), a: self.m.pos(mm.start()) + middle, b: self.m.pos(mm.end()) + middle, half_width }
    }

    fn truss_roof(&mut self, r: &TrussRoofSpec) -> Result<(), String> {
        let mat = self.material(&r.grade)?;
        r.shape.validate()?;
        for o in &r.overrides {
            o.shape.validate()?;
        }
        // What the script says it bears on: a perimeter's cap plates, an assembly's horizontal
        // members (beams, headers). Nothing else is looked for.
        let mut plates = vec![];
        for name in &r.bears_on {
            if let Some((parts, _)) = self.perimeters.get(name) {
                plates.extend(parts.cap_plates());
            } else if let Some(ms) = self.assemblies.get(name) {
                plates.extend(ms.iter().copied().filter(|&m| {
                    let mm = self.m.member(m);
                    (self.m.pos(mm.start()).z - self.m.pos(mm.end()).z).abs() < 1e-6
                }));
            } else {
                return Err(format!("roof {}: nothing named {name} to bear on (a perimeter or an assembly)", r.name));
            }
        }
        if plates.is_empty() {
            return Err(format!("roof {}: say what it bears on, with bearingOn(...): its height is that of what is under it", r.name));
        }
        let bearings: Vec<_> = plates.iter().map(|&p| self.top_of(p)).collect();
        // Where the trusses start from before they are sat on what is under them: the highest top.
        let start = bearings.iter().map(|b| b.a.z.max(b.b.z)).fold(f64::MIN, f64::max);
        let span = Vec3::new(r.span_dir.0, r.span_dir.1, 0.0).try_normalized().ok_or("roof span direction is zero")?;
        let mut roof = TrussRoof::new(&r.name, v3(r.origin.0, r.origin.1, start), span, r.shape.clone(), r.length, r.spacing, mat);
        roof.chord = r.chord;
        roof.web = r.web;
        for o in &r.overrides {
            roof.overrides.push((o.trusses.clone(), o.shape.clone()));
        }
        for s in &r.shifts {
            roof.shifts.push((s.trusses.clone(), s.along));
        }
        roof.bearings = bearings;
        let roof = roof.build(&mut self.m);
        if !roof.problems.is_empty() {
            return Err(format!("roof {}: {}", r.name, roof.problems.join(". ")));
        }
        self.m.group_mut(roof.group).parent = Some(self.root);
        // Each truss is fastened to what it sits on: at a node where the chord's axis meets the
        // other's axis, and otherwise face to face.
        let c = self.m.add_connection(fx::truss_to_plate());
        for (i, t) in roof.trusses.iter().enumerate() {
            if roof.unseated.contains(&(i + 1)) {
                let message = format!("roof {}: truss T{} bears on nothing: none of what the roof bears on is under any part of it", r.name, i + 1);
                self.m.issues.push(Issue::new(Severity::Error, "truss-not-seated", message).members(t.bottom_chords.clone()));
            }
            if roof.unsteady.contains(&(i + 1)) {
                let message = format!("roof {}: truss T{} is not held: what is under it is one bearing, or all to one side of its middle", r.name, i + 1);
                self.m.issues.push(Issue::new(Severity::Error, "truss-not-held", message).members(t.bottom_chords.clone()));
            }
            let mut on: Vec<usize> = roof.seats[i].iter().map(|(k, ..)| *k).collect();
            on.dedup();
            for k in on {
                let plate = plates[k];
                // The bearing point is the support node the chord is on: the middle one, where it is
                // on that; else the first it is on (an edge, an end). The model's node has its name.
                let here: Vec<&(usize, Vec3, String)> = roof.seats[i].iter().filter(|(j, ..)| *j == k).collect();
                let named = here.iter().find(|(_, _, name)| name.contains(": middle of ") || name.contains(": top ")).unwrap_or(&here[0]).2.clone();
                if let Some((bc, n)) = t.bottom_chords.iter().find_map(|&bc| self.m.intersect(bc, plate).ok().map(|n| (bc, n))) {
                    self.m.connect(n, bc, Some(plate), c);
                    self.m.nodes[n.idx()].name.get_or_insert(named);
                    continue;
                }
                // The axes do not meet: a bearing point is a node on the chord (its axis is its
                // underside) that is on the other's top face. Across the other, there is one, in
                // the middle of where they touch; along it, one at each end of where they touch.
                let top = self.m.face_toward(plate, Vec3::Z);
                let touching: Vec<Vec3> = here.iter().map(|(_, p, _)| *p).collect();
                let (first, last) = (touching[0], touching[touching.len() - 1]);
                let along = self.m.face(plate, top).along;
                let points = if (last - first).dot(along).abs() > 2.0 * LEN_TOL { vec![first, last] } else { vec![touching.iter().fold(Vec3::ZERO, |a, p| a + *p) / touching.len() as f64] };
                for at in points {
                let joined = t.bottom_chords.iter().find_map(|&bc| {
                    let (along, off) = self.m.project_on_member(bc, at);
                    let on_chord = off <= LEN_TOL && along > -LEN_TOL && along < self.m.member_length(bc) + LEN_TOL;
                    let face = self.m.face(plate, top);
                    let d = at - face.origin;
                    let on_face = d.dot(face.normal).abs() <= LEN_TOL && d.dot(face.along) > -LEN_TOL && d.dot(face.along) < face.length + LEN_TOL && d.dot(face.across).abs() <= face.half_width + LEN_TOL;
                    (on_chord && on_face).then_some(bc)
                });
                match joined {
                    Some(bc) => {
                        let n = self.m.node_on(bc, at).map_err(|e| format!("roof {}: {e}", r.name))?;
                        self.m.attach(n, plate, top).map_err(|e| format!("roof {}: {e}", r.name))?;
                        self.m.connect(n, bc, Some(plate), c);
                        self.m.nodes[n.idx()].name.get_or_insert(named.clone());
                    }
                    // (Its axis is beside the other's top, its lumber over it: face to face.)
                    None => {
                        if !self.m.bonds.iter().any(|b| (b.a, b.b) == (t.bottom_chords[0], plate)) {
                            self.m.bond(t.bottom_chords[0], plate, c);
                        }
                    }
                }
                }
            }
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
            let label = format!("{}#{}", mm.role, a.members[..i].iter().filter(|o| o.role == mm.role).count() + 1);
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
pub fn build_scene(spec: &SceneSpec) -> Result<Model, String> {
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
        let mut local = build_building(bs)?;
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
    Ok(m)
}

fn build_building(spec: &BuildingSpec) -> Result<Model, String> {
    let mut m = Model::new(&spec.name);
    let root = m.add_group(&spec.name, "building", Frame::WORLD, None);
    for (name, v) in &spec.directions {
        let d = v3(v[0], v[1], v[2]).try_normalized().ok_or_else(|| format!("building {}: direction {name} is zero", spec.name))?;
        m.name_direction(root, name, d);
    }
    let mut b = Builder { m, grades: HashMap::new(), perimeters: HashMap::new(), assemblies: HashMap::new(), root };
    // Perimeters first so roofs can bear on them regardless of order.
    for item in &spec.items {
        if let ItemSpec::Perimeter(p) = item {
            b.perimeter(p)?;
        }
    }
    // Then assemblies (roofs may bear on them), placed together with the relations between
    // their nodes: one scene, solved, then written into the model. Then roofs.
    let mut sc = scene::Scene::new("Assemblies and the relations between them");
    let mut declared = vec![];
    for item in &spec.items {
        if let ItemSpec::Assembly(a) = item {
            declared.push((a, b.declare(&mut sc, a)?));
        }
    }
    // A node by name: one of the scene's, or an end of a member a generator wrote
    // (`Walls/Front/cap_plate:start`), brought in.
    let node = |sc: &mut scene::Scene, m: &Model, name: &str| -> Result<scene::NodeRef, String> {
        if let Some(n) = sc.node_named(name) {
            return Ok(n);
        }
        let end = name.rsplit_once(':').and_then(|(path, end)| {
            let id = m.find_member(path).ok()?;
            match end {
                "start" => Some(m.member(id).start()),
                "end" => Some(m.member(id).end()),
                _ => None,
            }
        });
        match end {
            Some(n) => sc.import_node(name, m, n),
            None => Err(format!("no node named {name} (an assembly's point, as `Assembly/point`, or an end of a member, as `Perimeter/Wall/cap_plate:start` or `:end`)")),
        }
    };
    for item in &spec.items {
        match item {
            ItemSpec::Coincide { a, b: other } => {
                let (na, nb) = (node(&mut sc, &b.m, a)?, node(&mut sc, &b.m, other)?);
                sc.coincide(&format!("{a} is {other}"), na, nb);
            }
            ItemSpec::Datum { node: n, at } => {
                let nn = node(&mut sc, &b.m, n)?;
                sc.datum(&format!("{n} at [{}, {}, {}]", at[0], at[1], at[2]), nn, v3(at[0], at[1], at[2]));
            }
            _ => {}
        }
    }
    let written = sc.write(&mut b.m)?;
    for open in &written.open {
        b.m.issues.push(Issue::new(Severity::Warning, "open", format!("nothing places this: {open}")));
    }
    for (a, d) in &declared {
        b.finish(a, d, &written)?;
    }
    for item in &spec.items {
        if let ItemSpec::TrussRoof(r) = item {
            b.truss_roof(r)?;
        }
    }
    Ok(b.m)
}
