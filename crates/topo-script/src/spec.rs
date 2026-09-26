//! The scene spec a script exports (mirrors `ts/topo-cad.ts`) and its
//! expansion into a `Model` using the Rust generators.

use serde::Deserialize;
use std::collections::HashMap;
use topo_core::*;
use topo_timber::fastening as fx;
use topo_timber::lumber::graded;
use topo_timber::lumber::{built_up, sawn};
use topo_timber::{Justify, Opening, OpeningKind, Perimeter, PerimeterParts, Side, TrussRoof, TrussShape, Wall};

#[derive(Debug, Deserialize)]
pub struct SceneSpec {
    pub name: String,
    #[serde(default)]
    pub info: InfoSpec,
    #[serde(default)]
    pub items: Vec<ItemSpec>,
    #[serde(default)]
    pub measurements: Vec<topo_geom::measure::Measurement>,
    #[serde(default)]
    pub unknowns: Vec<UnknownSpec>,
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
    /// `nailed`, `truss_plate` or `none`: connection recorded where members meet.
    pub joints: String,
    #[serde(default)]
    pub bears_on: Vec<String>,
    /// Points that bear on the foundation.
    #[serde(default)]
    pub supports: Vec<String>,
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
    pub grade: GradeSpec,
    pub header_grade: Option<GradeSpec>,
    pub justify: String,
    pub openings: Vec<OpeningSpec>,
    pub cap_breaks: Vec<f64>,
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
    pub height: Option<f64>,
    pub bears_on: Vec<String>,
    pub loads: Vec<AreaLoadSpec>,
}

struct Builder {
    m: Model,
    grades: HashMap<GradeSpec, MaterialId>,
    perimeters: HashMap<String, (PerimeterParts, f64)>,
    root: GroupId,
}

impl Builder {
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
        let parts = per.build(&mut self.m);
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

    fn truss_roof(&mut self, r: &TrussRoofSpec) -> Result<(), String> {
        let mat = self.material(&r.grade)?;
        let mut plates = vec![];
        let mut height = r.height;
        for name in &r.bears_on {
            let (parts, top) = self.perimeters.get(name).ok_or_else(|| format!("roof {}: no perimeter named {name}", r.name))?;
            plates.extend(parts.cap_plates());
            height = height.or(Some(*top));
        }
        let h = height.ok_or_else(|| format!("roof {}: give a height or bearingOn(...)", r.name))?;
        let span = Vec3::new(r.span_dir.0, r.span_dir.1, 0.0).try_normalized().ok_or("roof span direction is zero")?;
        let mut roof = TrussRoof::new(&r.name, v3(r.origin.0, r.origin.1, h), span, r.shape.clone(), r.length, r.spacing, mat);
        roof.chord = r.chord;
        roof.web = r.web;
        r.shape.validate()?;
        let roof = roof
        .build(&mut self.m);
        self.m.group_mut(roof.group).parent = Some(self.root);
        let c = self.m.add_connection(fx::truss_to_plate());
        let chords = roof.bottom_chords();
        for n in self.m.connect_members(&chords, &plates) {
            for &bc in &chords {
                if self.m.member(bc).path.contains(&n) {
                    let plate = plates.iter().copied().find(|p| self.m.member(*p).path.contains(&n));
                    self.m.connect(n, bc, plate, c);
                }
            }
        }
        for l in &r.loads {
            let kind = match l.kind.as_str() {
                "dead" => LoadKind::Dead,
                "live" => LoadKind::Live,
                "roof_live" => LoadKind::RoofLive,
                "snow" => LoadKind::Snow,
                k => return Err(format!("unknown load kind {k}")),
            };
            let case = match self.m.load_cases.iter().find(|c| c.kind == kind) {
                Some(c) => c.id,
                None => {
                    let name = match kind {
                        LoadKind::Dead => "D",
                        LoadKind::Live => "L",
                        LoadKind::RoofLive => "Lr",
                        _ => "S",
                    };
                    self.m.add_load_case(name, kind)
                }
            };
            self.m.loads.push(Load::Area { case, group: roof.group, pressure: l.pressure, direction: -Vec3::Z });
        }
        Ok(())
    }
}

impl Builder {
    fn assembly(&mut self, a: &AssemblySpec) -> Result<(), String> {
        let f = a.placement.frame();
        let g = self.m.add_group(&a.name, "assembly", f, Some(self.root));
        let mut nodes: HashMap<&str, NodeId> = HashMap::new();
        for p in &a.points {
            let n = self.m.node_at(f.to_world(v3(p.at[0], p.at[1], p.at[2])));
            if nodes.insert(p.name.as_str(), n).is_some() {
                return Err(format!("assembly {}: duplicate point {}", a.name, p.name));
            }
        }
        let mut ids = vec![];
        for (i, mm) in a.members.iter().enumerate() {
            let path: Vec<NodeId> = mm
                .path
                .iter()
                .map(|n| nodes.get(n.as_str()).copied().ok_or(format!("assembly {}: member {i} uses unknown point {n}", a.name)))
                .collect::<Result<_, _>>()?;
            if path.len() < 2 {
                return Err(format!("assembly {}: member {i} needs two points", a.name));
            }
            let sec = if mm.plies > 1 { built_up(mm.plies, mm.size.0, mm.size.1, 0.0) } else { sawn(mm.size.0, mm.size.1) };
            let sec = self.m.add_section(sec);
            let mat = self.material(&mm.grade)?;
            // Depth direction in the assembly's local frame (default: up, or +y
            // for vertical members), then placed with it, so a rotated copy is
            // identical to the original.
            let (la, lb) = (&a.points.iter().find(|p| p.name == mm.path[0]).unwrap().at, &a.points.iter().find(|p| p.name == *mm.path.last().unwrap()).unwrap().at);
            let local_axis = v3(lb[0] - la[0], lb[1] - la[1], lb[2] - la[2]);
            let local_depth = match mm.depth {
                Some(d) => v3(d[0], d[1], d[2]),
                None if local_axis.try_normalized().is_some_and(|x| x.dot(Vec3::Z).abs() > 0.99) => Vec3::Y,
                None => Vec3::Z,
            };
            let mut spec = MemberSpec::new(&mm.role, sec, mat).priority(mm.priority).group(g).depth_dir(f.dir_to_world(local_depth));
            if let Some((u, v)) = mm.anchor {
                spec = spec.anchor(Anchor { u, v, offset: Vec2::ZERO });
            }
            ids.push(self.m.add_member(&path, &spec));
        }
        let conn = match a.joints.as_str() {
            "nailed" => Some(self.m.add_connection(fx::plate_corner())),
            "truss_plate" => Some(self.m.add_connection(fx::truss_plate())),
            "none" => None,
            j => return Err(format!("assembly {}: unknown joint kind {j}", a.name)),
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
        let mut plates = vec![];
        for name in &a.bears_on {
            let (parts, _) = self.perimeters.get(name).ok_or_else(|| format!("assembly {}: no perimeter named {name}", a.name))?;
            plates.extend(parts.cap_plates());
        }
        if !plates.is_empty() {
            let c = self.m.add_connection(fx::joist_to_plate());
            for n in self.m.connect_members(&ids, &plates) {
                for &id in &ids {
                    if self.m.member(id).path.contains(&n) {
                        let plate = plates.iter().copied().find(|p| self.m.member(*p).path.contains(&n));
                        self.m.connect(n, id, plate, c);
                    }
                }
            }
        }
        Ok(())
    }
}

/// Expands a scene spec into a model.
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
    let root = m.add_group("Structure", "building", Frame::WORLD, None);
    let mut b = Builder { m, grades: HashMap::new(), perimeters: HashMap::new(), root };
    // Perimeters first so roofs can bear on them regardless of order.
    for item in &spec.items {
        if let ItemSpec::Perimeter(p) = item {
            b.perimeter(p)?;
        }
    }
    for item in &spec.items {
        match item {
            ItemSpec::TrussRoof(r) => b.truss_roof(r)?,
            ItemSpec::Assembly(a) => b.assembly(a)?,
            ItemSpec::Perimeter(_) => {}
        }
    }
    Ok(b.m)
}
