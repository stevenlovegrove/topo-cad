//! The model as loaded: its graph solved as one elastic 3D frame.
//!
//! This is the second stage on the same graph. The solve that places things
//! (`topo-solve`, rigid) is not touched; about that placement, the graph's
//! analytical extract (`AnalysisModel`) is solved for each load case:
//!
//! * every member a beam through the nodes of its path;
//! * where a member ends, the fixity of its connection there (rigid, pinned,
//!   or a connector's rotational spring);
//! * a node on a member's face held to the member's axis by a rigid arm (a
//!   bearing point), and members bonded face to face tied along their length
//!   the same way;
//! * supports holding the directions they say;
//! * loads as the takedown has them (self-weight, along members, surfaces
//!   shared out by tributary width), and forces and moments at nodes.
//!
//! Light framing is not braced by its members alone (a stud wall racks, a
//! truss can fall over sideways): what would move with nothing to resist it
//! is held, and what holding it takes is reported, so that what the model
//! leaves to sheathing and blocking is said and sized. A model that asks to
//! be analysed as a frame (`Standards::analysis`) is held to more: any such
//! direction with load on it is an error.
//!
//! Assumptions: Euler–Bernoulli beams (no shear deformation) on the node
//! lines (a section anchored off its axis is analysed on the axis); joints
//! have no size; linear, about the placed geometry. The results take the
//! same form as a load takedown's, so checks, diagrams and drawings work on
//! either.

use crate::frame3d::{Case, Elem, EndRelease, Frame3d, SolveError};
use crate::loadpath::Transfer;
use crate::model::AnalysisModel;
use crate::takedown::{applied_loading, ei_vertical_plane, Behaviour, Diagram, MemberInfo, Reaction, SupportPt, Takedown};
use std::collections::HashMap;
use topo_core::{Fixity, Issue, Load, LoadCaseId, MemberId, Model, NodeId, Severity, Topology, Vec3};
use topo_geom::Geometry;

/// Samples per element (between two nodes of a member).
const SAMPLES: usize = 16;
/// Members bonded face to face are tied to each other this often along where they lie together.
const TIE_EVERY: f64 = 0.2;
/// A hold that takes more than this is not bracing left to sheathing (N, N·m).
const HELD_FORCE: f64 = 50.0;
const HELD_MOMENT: f64 = 50.0;
/// A bearing point pulled on by more than this share of the load has lifted.
const LIFT: f64 = 0.002;
/// What is left of the stiffness of a bearing point let go.
const GONE: f64 = 1e-10;
/// Nodes closer than this are one point of the frame.
const SAME: f64 = 1e-7;

fn release(f: Fixity) -> EndRelease {
    match f {
        Fixity::Rigid => None,
        Fixity::Pinned => Some([0.0, 0.0]),
        Fixity::Semi { k_u, k_v } => Some([k_u, k_v]),
    }
}

/// How many times a member's own stiffness an arm of this length is given: stiff against the
/// member, without being so stiff the equations lose their footing (a short arm is stiff already).
fn arm_stiffness(length: f64) -> f64 {
    (10.0 * (length / 0.3).powi(3)).clamp(10.0, 1e4)
}

pub fn frame(model: &Model, topo: &Topology, geom: &Geometry) -> Takedown {
    let cases: Vec<LoadCaseId> = model.load_cases.iter().map(|c| c.id).collect();
    let (nm, nc) = (model.members.len(), cases.len());
    let mut issues = vec![];
    let graph = AnalysisModel::extract(model, topo, geom);
    let along = |m: MemberId, n: NodeId| model.pos(n).distance(model.pos(model.member(m).start()));

    // ----- what is held to what, off the axes ---------------------------------
    // A tie holds two places together as by a rigid arm. Nodes on faces (bearing points) are
    // tied to the axis of the member whose face it is; members bonded face to face are tied
    // to each other all along where they lie together.
    #[derive(Clone, Copy)]
    enum At {
        Node(NodeId),
        On(MemberId, f64),
    }
    let mut ties: Vec<(At, At, MemberId)> = graph.links.iter().map(|l| (At::On(l.member, l.along), At::Node(l.node), l.member)).collect();
    // (The first of them are bearing points: things touching at a face, which can press and cannot pull.)
    let bearing_points = ties.len();
    for c in &graph.couplings {
        let (xa, xb) = (model.member_axes(c.a), model.member_axes(c.b));
        // Where b lies along a, as stations along a: the ends of the overlap, and every so often between.
        let (t0, t1) = {
            let ts = [(xb.origin - xa.origin).dot(xa.x), (xb.origin + xb.x * xb.length - xa.origin).dot(xa.x)];
            (ts[0].min(ts[1]).max(0.0), ts[0].max(ts[1]).min(xa.length))
        };
        if t1 - t0 < SAME {
            continue;
        }
        let n = ((t1 - t0) / TIE_EVERY).ceil().max(1.0) as usize;
        for k in 0..=n {
            let ta = t0 + (t1 - t0) * k as f64 / n as f64;
            let tb = ((xa.origin + xa.x * ta - xb.origin).dot(xb.x)).clamp(0.0, xb.length);
            ties.push((At::On(c.a, ta), At::On(c.b, tb), c.a));
        }
    }

    // ----- nodes and elements ------------------------------------------------
    let mut index: HashMap<NodeId, usize> = HashMap::new();
    let mut nodes: Vec<Vec3> = vec![];
    // The model's node each is (none: a point on a member's axis that something is tied to).
    let mut node_ids: Vec<Option<NodeId>> = vec![];
    let mut elems: Vec<Elem> = vec![];
    // Per element of a member: the member, and how far along it the element's i end is.
    let mut seg: Vec<Option<(MemberId, f64)>> = vec![];
    // (Model nodes at one place are one point of the frame: a member's segment of no length.)
    let mut same: HashMap<NodeId, NodeId> = HashMap::new();
    for e in graph.elements.iter().filter(|e| e.length < SAME) {
        let keep = *same.get(&e.i).unwrap_or(&e.i);
        same.insert(e.j, keep);
    }
    let mut ix = |n: NodeId, nodes: &mut Vec<Vec3>, node_ids: &mut Vec<Option<NodeId>>| {
        let n = *same.get(&n).unwrap_or(&n);
        *index.entry(n).or_insert_with(|| {
            nodes.push(model.pos(n));
            node_ids.push(Some(n));
            nodes.len() - 1
        })
    };
    // Each member's stations: its path's nodes, and the places along it that something is tied
    // to (a place past its end is its end: its solid runs on past its node line).
    let mut stations: Vec<Vec<(f64, usize)>> = vec![vec![]; nm];
    for m in &model.members {
        let axes = model.member_axes(m.id);
        let mut at: Vec<(f64, usize)> = m.path.iter().map(|&n| (along(m.id, n), ix(n, &mut nodes, &mut node_ids))).collect();
        let mut cuts: Vec<f64> = ties.iter().flat_map(|t| [t.0, t.1]).filter_map(|e| if let At::On(mm, t) = e { (mm == m.id).then_some(t.clamp(0.0, axes.length)) } else { None }).collect();
        cuts.sort_by(f64::total_cmp);
        for t in cuts {
            if !at.iter().any(|s| (s.0 - t).abs() <= 1e-4) {
                nodes.push(axes.origin + axes.x * t);
                node_ids.push(None);
                at.push((t, nodes.len() - 1));
            }
        }
        at.sort_by(|a, b| a.0.total_cmp(&b.0));
        stations[m.id.idx()] = at;
    }
    for m in &model.members {
        let Some(e) = graph.elements.iter().find(|e| e.member == m.id) else { continue };
        let last = graph.elements.iter().rfind(|e| e.member == m.id).unwrap();
        let st = &stations[m.id.idx()];
        for (k, w) in st.windows(2).enumerate() {
            if w[0].1 == w[1].1 || w[1].0 - w[0].0 < SAME {
                continue;
            }
            // (Its ends are held as their connections say; along it, it is one piece.)
            let fix = |end: bool, f: Fixity| if end { release(f) } else { None };
            seg.push(Some((m.id, w[0].0)));
            elems.push(Elem { i: w[0].1, j: w[1].1, v: e.v, ea: e.e * e.area, gj: e.g * e.j_t, ei_u: e.e * e.i_u, ei_v: e.e * e.i_v, rel_i: fix(k == 0, e.fix_i), rel_j: fix(k + 2 == st.len(), last.fix_j) });
        }
    }
    // The arms. (Of a bearing point: which element it is, the member whose face it is on, and the node.)
    let mut contacts: Vec<(usize, MemberId, NodeId, Vec3)> = vec![];
    for (which, &(a, b, of)) in ties.iter().enumerate() {
        let mut place = |at: At, nodes: &mut Vec<Vec3>, node_ids: &mut Vec<Option<NodeId>>| match at {
            At::Node(n) => ix(n, nodes, node_ids),
            At::On(m, t) => {
                let t = t.clamp(0.0, model.member_axes(m).length);
                stations[m.idx()].iter().min_by(|x, y| (x.0 - t).abs().total_cmp(&(y.0 - t).abs())).map(|s| s.1).unwrap()
            }
        };
        let (i, j) = (place(a, &mut nodes, &mut node_ids), place(b, &mut nodes, &mut node_ids));
        let Some(e) = graph.elements.iter().find(|e| e.member == of) else { continue };
        let l = nodes[i].distance(nodes[j]);
        if i != j && l > SAME {
            let k = arm_stiffness(l);
            if let (true, At::Node(n)) = (which < bearing_points, b) {
                // (Its face's outward normal: what is on the face presses against it.)
                let a = &model.attachments[which];
                contacts.push((elems.len(), of, n, model.face(a.member, a.face).normal));
            }
            seg.push(None);
            elems.push(Elem { i, j, v: e.v, ea: e.e * e.area * k, gj: e.g * e.j_t * k, ei_u: e.e * e.i_u * k, ei_v: e.e * e.i_v * k, rel_i: None, rel_j: None });
        }
    }
    let mut fixed: Vec<(usize, usize)> = vec![];
    for s in &graph.supports {
        let Some(&n) = index.get(same.get(&s.node).unwrap_or(&s.node)) else { continue };
        let r = &s.restraint;
        for (d, on) in [r.tx, r.ty, r.tz, r.rx, r.ry, r.rz].into_iter().enumerate() {
            if on && !fixed.contains(&(n, d)) {
                fixed.push((n, d));
            }
        }
    }
    let frame = Frame3d { nodes, elems, fixed };
    let frame_node = |n: NodeId| index.get(same.get(&n).unwrap_or(&n)).copied();

    // ----- loads ---------------------------------------------------------------
    // As the takedown has them along members; forces and moments at nodes as they are given.
    let (loading, applied) = applied_loading(model, &mut issues);
    let mut loads: Vec<Case> = (0..nc).map(|_| Case::zeros(frame.nodes.len(), frame.elems.len())).collect();
    let len = |k: usize| frame.nodes[frame.elems[k].i].distance(frame.nodes[frame.elems[k].j]);
    for (ci, per_member) in loading.iter().enumerate() {
        for (k, &(m, t0)) in seg.iter().enumerate().filter_map(|(k, s)| s.as_ref().map(|s| (k, s))) {
            let (l, t1) = (len(k), t0 + len(k));
            for &(a, b, w0, w1) in &per_member[m.idx()].lines {
                // (Averaged over the element: what of the load is on it, spread along it.)
                let (c, d) = (a.max(t0), b.min(t1));
                if d > c {
                    let at = |t: f64| if b > a { w0 + (w1 - w0) * ((t - a) / (b - a)) } else { w0 };
                    loads[ci].w[k] += (at(c) + at(d)) * (0.5 * (d - c) / l);
                }
            }
        }
    }
    for load in &model.loads {
        if let Load::Node { case, node, force, moment } = load {
            let Some(n) = frame_node(*node) else { continue };
            let l = &mut loads[case.idx()].nodal[n];
            for (d, v) in [force.x, force.y, force.z, moment.x, moment.y, moment.z].into_iter().enumerate() {
                l[d] += v;
            }
        }
    }

    // ----- member info -----------------------------------------------------------
    let supported: HashMap<NodeId, &topo_core::Support> = model.supports.iter().map(|s| (s.node, s)).collect();
    let mut info: Vec<MemberInfo> = model
        .members
        .iter()
        .map(|m| {
            let (a, b) = (model.pos(m.start()), model.pos(m.end()));
            let x = (b - a) / a.distance(b);
            let supports = m
                .path
                .iter()
                .filter(|n| supported.contains_key(n))
                .map(|&n| SupportPt { t: model.pos(n).distance(a), by: None, node: Some(n), transfer: Transfer::Bearing, at: model.pos(n), bearing: 0.0 })
                .collect();
            MemberInfo { behaviour: Behaviour::Frame, supports, spans: vec![], length: a.distance(b), cos: (1.0 - x.z * x.z).sqrt(), ei: ei_vertical_plane(model, m.id, geom), rides_on: None }
        })
        .collect();

    // ----- solve -------------------------------------------------------------------
    let ts: Vec<f64> = (0..=SAMPLES).map(|i| i as f64 / SAMPLES as f64).collect();
    let at_node = |node: usize| -> Vec<String> {
        match node_ids[node] {
            Some(id) => model.members.iter().filter(|m| m.path.contains(&id)).take(3).map(|m| model.member_path(m.id)).collect(),
            None => vec![],
        }
    };
    // A solution as the rest reads it: forces and displacement along each member, and what each support gives.
    let read = |frame: &Frame3d, sol: &crate::frame3d::Solution| -> (Vec<Diagram>, Vec<Reaction>) {
        let mut ds = vec![Diagram::default(); nm];
        for (k, &(m, t0)) in seg.iter().enumerate().filter_map(|(k, s)| s.as_ref().map(|s| (k, s))) {
            let d = &mut ds[m.idx()];
            let l = len(k);
            for &f in &ts {
                let s = frame.section(sol, k, f * l);
                d.t.push(t0 + f * l);
                d.n.push(s.n);
                d.v.push(s.v_v);
                d.m.push(s.m_v);
                d.y.push(-s.d.z);
                d.v2.push(s.v_u);
                d.m2.push(s.m_u);
                d.tq.push(s.t);
                d.d.push(s.d);
            }
        }
        let mut rs: Vec<Reaction> = vec![];
        for s in &model.supports {
            let Some(n) = frame_node(s.node) else { continue };
            let comp = |d: usize| frame.fixed.iter().position(|f| *f == (n, d)).map(|i| sol.reactions[i]).unwrap_or(0.0);
            let Some(member) = model.members.iter().find(|m| m.path.contains(&s.node)) else { continue };
            if rs.iter().any(|r| r.node.and_then(&frame_node) == Some(n)) {
                continue;
            }
            rs.push(Reaction { member: member.id, by: None, node: Some(s.node), at: model.pos(s.node), force: Vec3::new(comp(0), comp(1), comp(2)), transfer: Transfer::Bearing });
        }
        (ds, rs)
    };
    let mut diagrams: Vec<Vec<Diagram>> = vec![vec![Diagram::default(); nm]; nc];
    let mut reactions: Vec<Vec<Reaction>> = vec![vec![]; nc];
    let mut combined = vec![];
    let mut unsolved = None;
    match frame.solve(&loads) {
        Ok((sols, held)) => {
            // What is held because nothing resists it, and what holding it takes (the most, over the cases).
            let taking: Vec<f64> = (0..held.len()).map(|h| sols.iter().map(|s| s.held[h].abs()).fold(0.0, f64::max)).collect();
            let scale = applied.iter().fold(0.0f64, |a, w| a.max(w.abs())).max(loads.iter().flat_map(|c| c.nodal.iter()).map(|l| l[0].hypot(l[1]).hypot(l[2])).fold(0.0, f64::max)).max(1e-9);
            let loaded: Vec<usize> = (0..held.len()).filter(|&h| taking[h] > 1e-6 * scale).collect();
            if model.standards.is_frame() {
                // A model that says it is a frame is held together by its members and joints, or it is not solved.
                if let Some(&h) = loaded.iter().max_by(|a, b| taking[**a].total_cmp(&taking[**b])) {
                    unsolved = Some(SolveError::Mechanism { node: held[h].0, dof: held[h].1 });
                }
                let loose: Vec<NodeId> = held.iter().filter(|h| h.1 < 3).filter_map(|h| node_ids[h.0]).collect();
                if unsolved.is_none() && !loose.is_empty() {
                    issues.push(Issue::new(Severity::Warning, "frame-loose", format!("{} points are not held in every direction (no load acts on them, so they are left where they are)", loose.len())).nodes(loose));
                }
            } else if !loaded.is_empty() {
                // Light framing: what its members and joints do not brace is held, and said.
                let most = |turn: bool| loaded.iter().copied().filter(|&h| (held[h].1 >= 3) == turn).max_by(|a, b| taking[*a].total_cmp(&taking[*b]));
                let nodes: Vec<NodeId> = loaded.iter().filter_map(|&h| node_ids[held[h].0]).collect();
                let places = {
                    let mut n = nodes.clone();
                    n.sort();
                    n.dedup();
                    n.len()
                };
                // A hold that takes real force or moment is not bracing left to sheathing:
                // something is not joined to what carries it.
                for (turn, limit, unit, what) in [(false, HELD_FORCE, "N", "kept in place"), (true, HELD_MOMENT, "N·m", "kept from turning")] {
                    if let Some(h) = most(turn).filter(|&h| taking[h] > limit) {
                        let at = at_node(held[h].0);
                        issues.push(
                            Issue::new(Severity::Warning, "frame-unsupported", format!("as loaded, {} is {what} only by being held ({:.0} {unit}): it is not joined to what carries it", if at.is_empty() { "something".into() } else { at.join(", ") }, taking[h]))
                                .nodes(node_ids[held[h].0].into_iter()),
                        );
                    }
                }
                let force = most(false).map(|h| taking[h]).unwrap_or(0.0);
                let moment = most(true).map(|h| taking[h]).unwrap_or(0.0);
                if force > 1.0 || moment > 1.0 {
                    issues.push(
                        Issue::new(Severity::Info, "frame-held", format!("as loaded, {} directions at {places} points are braced by nothing in the model (sheathing, blocking): they are held, by up to {force:.0} N and {moment:.0} N·m", loaded.len())).nodes(nodes),
                    );
                }
            }
            if unsolved.is_none() {
                for (ci, sol) in sols.iter().enumerate() {
                    (diagrams[ci], reactions[ci]) = read(&frame, sol);
                }
            }
            // Bearing points press and cannot pull. Each case is solved with them all held (so
            // the cases add up); a combination that would pull on some is solved again as a whole
            // with those let go, until none pulls and none let go has closed again.
            if unsolved.is_none() && !contacts.is_empty() && !model.standards.is_frame() {
                // How hard a bearing point is pulled off its face: what the node puts on the arm, along
                // the face's outward normal (pressing is the other way).
                let pull = |sol: &crate::frame3d::Solution, c: &(usize, MemberId, NodeId, Vec3)| {
                    let r = &sol.elems[c.0];
                    (r.x * r.fj.x + r.u * r.fj.y + r.v * r.fj.z).dot(c.3)
                };
                // Whether one let go has come back onto its face.
                let closing = |f: &Frame3d, sol: &crate::frame3d::Solution, c: &(usize, MemberId, NodeId, Vec3)| {
                    let e = &f.elems[c.0];
                    let d = |n: usize| Vec3::new(sol.disp[n][0], sol.disp[n][1], sol.disp[n][2]);
                    (d(e.j) - d(e.i)).dot(c.3) < -1e-9
                };
                let mut worst: Option<(usize, String, usize)> = None;
                for combo in crate::nds::combinations(model, &[]) {
                    let tension = |c: &(usize, MemberId, NodeId, Vec3)| combo.factors.iter().map(|(case, f)| f * pull(&sols[case.idx()], c)).sum::<f64>();
                    if !contacts.iter().any(|c| tension(c) > LIFT * scale) {
                        continue;
                    }
                    // The combination as one case.
                    let mut case = Case::zeros(frame.nodes.len(), frame.elems.len());
                    for (id, f) in &combo.factors {
                        let l = &loads[id.idx()];
                        for (a, b) in case.nodal.iter_mut().zip(&l.nodal) {
                            for d in 0..6 {
                                a[d] += f * b[d];
                            }
                        }
                        for (a, b) in case.w.iter_mut().zip(&l.w) {
                            *a += *b * *f;
                        }
                    }
                    let mut off: Vec<bool> = contacts.iter().map(|c| tension(c) > LIFT * scale).collect();
                    let mut solved = None;
                    for _ in 0..12 {
                        let mut trial = Frame3d { nodes: frame.nodes.clone(), elems: frame.elems.clone(), fixed: frame.fixed.clone() };
                        for (c, _) in contacts.iter().zip(&off).filter(|(_, o)| **o) {
                            let e = &mut trial.elems[c.0];
                            (e.ea, e.gj, e.ei_u, e.ei_v) = (e.ea * GONE, e.gj * GONE, e.ei_u * GONE, e.ei_v * GONE);
                        }
                        let Ok((mut sol, _)) = trial.solve(std::slice::from_ref(&case)) else { break };
                        let sol = sol.remove(0);
                        let next: Vec<bool> = contacts.iter().zip(&off).map(|(c, &o)| if o { !closing(&trial, &sol, c) } else { pull(&sol, c) > LIFT * scale }).collect();
                        let settled = next == off;
                        off = next;
                        solved = Some((trial, sol));
                        if settled {
                            break;
                        }
                    }
                    let Some((trial, sol)) = solved else { continue };
                    let lifted = off.iter().filter(|o| **o).count();
                    let (diagrams, reactions) = read(&trial, &sol);
                    combined.push(crate::takedown::Combined { name: combo.name.clone(), factors: combo.factors.clone(), diagrams, reactions, lifted });
                    if worst.as_ref().is_none_or(|w| lifted > w.0) {
                        worst = Some((lifted, combo.name.clone(), contacts.iter().zip(&off).filter(|(_, o)| **o).map(|(c, _)| c.2.idx()).next().unwrap_or(0)));
                    }
                }
                if let Some((lifted, combo, node)) = worst.filter(|w| w.0 > 0) {
                    let n = topo_core::NodeId(node as u32);
                    let what: Vec<String> = model.members.iter().filter(|m| m.path.contains(&n)).take(2).map(|m| model.member_path(m.id)).collect();
                    issues.push(Issue::new(Severity::Info, "contact-lifts", format!("as loaded, {lifted} bearing points lift under {combo} (first: {}): they are let go there, and what is left carries it", what.join(", "))).nodes([n]));
                }
            }
        }
        Err(e) => unsolved = Some(e),
    }
    if let Some(e) = unsolved {
        let message = match e {
            SolveError::Mechanism { node, dof } => {
                let at = at_node(node);
                format!(
                    "the frame is a mechanism under its loads: nothing resists {} where {} meet{} — add a support, or a joint there that carries moment",
                    SolveError::dof_name(dof),
                    at.join(", "),
                    if at.len() == 1 { "s nothing else" } else { "" }
                )
            }
            other => other.to_string(),
        };
        issues.push(Issue::new(Severity::Error, "frame-unsolved", message));
        for i in &mut info {
            i.behaviour = Behaviour::Unsupported;
        }
        for dc in &mut diagrams {
            for (m, d) in dc.iter_mut().enumerate() {
                *d = Diagram::zeros(vec![0.0, info[m].length]);
            }
        }
        for r in &mut reactions {
            r.clear();
        }
        combined.clear();
    }
    Takedown { cases, info, diagrams, reactions, applied, issues, combined }
}
