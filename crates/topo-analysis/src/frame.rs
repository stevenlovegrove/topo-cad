//! The whole model as one 3D frame (`Standards::analysis = "frame"`), for
//! things that are not light framing: a welded tube frame, a machine base of
//! bolted extrusions.
//!
//! It solves the model's analytical extract (`AnalysisModel`), the graph as
//! it is: every member a beam through the nodes of its path; where a member
//! ends, the fixity of its connection there (rigid, pinned, or a connector's
//! rotational spring); a node on a member's face held to the member's axis
//! by a rigid arm; supports holding the directions they say; loads in any
//! direction.
//!
//! Assumptions: Euler–Bernoulli beams (no shear deformation) on the node
//! lines (a section anchored off its axis is analysed on the axis); joints
//! have no size; members bonded face to face are not made to act as one.
//! The results take the same form as a load takedown's, so checks, diagrams
//! and drawings work on either.

use crate::frame3d::{Case, Elem, EndRelease, Frame3d, SolveError};
use crate::loadpath::Transfer;
use crate::model::AnalysisModel;
use crate::takedown::{ei_vertical_plane, Behaviour, Diagram, MemberInfo, Reaction, SupportPt, Takedown, G};
use std::collections::HashMap;
use topo_core::{Fixity, Issue, Load, LoadCaseId, MemberId, Model, NodeId, Severity, Topology, Vec3};
use topo_geom::Geometry;

/// Samples per element (between two nodes of a member).
const SAMPLES: usize = 16;
/// A rigid arm is this many times as stiff as the member it holds a node to.
const RIGID: f64 = 1e4;

fn release(f: Fixity) -> EndRelease {
    match f {
        Fixity::Rigid => None,
        Fixity::Pinned => Some([0.0, 0.0]),
        Fixity::Semi { k_u, k_v } => Some([k_u, k_v]),
    }
}

pub fn frame(model: &Model, topo: &Topology, geom: &Geometry) -> Takedown {
    let cases: Vec<LoadCaseId> = model.load_cases.iter().map(|c| c.id).collect();
    let (nm, nc) = (model.members.len(), cases.len());
    let mut issues = vec![];
    let graph = AnalysisModel::extract(model, topo, geom);

    // ----- nodes and elements ------------------------------------------------
    let mut index: HashMap<NodeId, usize> = HashMap::new();
    let mut nodes: Vec<Vec3> = vec![];
    // The model's node each is (none: a point on a member's axis under a node on its face).
    let mut node_ids: Vec<Option<NodeId>> = vec![];
    let mut elems: Vec<Elem> = vec![];
    // Per element of a member: the member, and how far along it the element's i end is.
    let mut seg: Vec<Option<(MemberId, f64)>> = vec![];
    let mut ix = |n: NodeId, nodes: &mut Vec<Vec3>, node_ids: &mut Vec<Option<NodeId>>| {
        *index.entry(n).or_insert_with(|| {
            nodes.push(model.pos(n));
            node_ids.push(Some(n));
            nodes.len() - 1
        })
    };
    for e in &graph.elements {
        let start = model.pos(model.member(e.member).start());
        let (pi, pj) = (model.pos(e.i), model.pos(e.j));
        let (t0, t1) = (pi.distance(start), pj.distance(start));
        // Nodes on this member's faces, within this stretch of it: each is held to the axis
        // there, so the member is divided there.
        let mut cuts: Vec<(f64, &crate::model::Link)> = graph.links.iter().filter(|l| l.member == e.member && l.along > t0 + 1e-6 && l.along < t1 - 1e-6).map(|l| (l.along, l)).collect();
        cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
        let stiff = |fi: Fixity, fj: Fixity, i: usize, j: usize, k: f64| Elem { i, j, v: e.v, ea: e.e * e.area * k, gj: e.g * e.j_t * k, ei_u: e.e * e.i_u * k, ei_v: e.e * e.i_v * k, rel_i: release(fi), rel_j: release(fj) };
        let (mut from, mut t, mut fix) = (ix(e.i, &mut nodes, &mut node_ids), t0, e.fix_i);
        for (along, link) in cuts {
            nodes.push(pi + e.x * (along - t0));
            node_ids.push(None);
            let on_axis = nodes.len() - 1;
            seg.push(Some((e.member, t)));
            elems.push(stiff(fix, Fixity::Rigid, from, on_axis, 1.0));
            // The arm, from the axis to the node on the face.
            let at = ix(link.node, &mut nodes, &mut node_ids);
            if link.arm.norm() > 1e-9 {
                seg.push(None);
                elems.push(stiff(Fixity::Rigid, Fixity::Rigid, on_axis, at, RIGID));
            }
            (from, t, fix) = (on_axis, along, Fixity::Rigid);
        }
        seg.push(Some((e.member, t)));
        elems.push(stiff(fix, e.fix_j, from, ix(e.j, &mut nodes, &mut node_ids), 1.0));
    }
    // (A node on a face where the member has a node already, at its end or in its path: an arm from that.)
    for l in &graph.links {
        let mem = model.member(l.member);
        let start = model.pos(mem.start());
        let Some(&near) = mem.path.iter().find(|&&n| (model.pos(n).distance(start) - l.along).abs() <= 1e-6) else { continue };
        let Some(e) = graph.elements.iter().find(|e| e.member == l.member) else { continue };
        let (i, j) = (ix(near, &mut nodes, &mut node_ids), ix(l.node, &mut nodes, &mut node_ids));
        if i != j && nodes[i].distance(nodes[j]) > 1e-9 {
            seg.push(None);
            elems.push(Elem { i, j, v: e.v, ea: e.e * e.area * RIGID, gj: e.g * e.j_t * RIGID, ei_u: e.e * e.i_u * RIGID, ei_v: e.e * e.i_v * RIGID, rel_i: None, rel_j: None });
        }
    }
    let mut fixed: Vec<(usize, usize)> = vec![];
    for s in &graph.supports {
        let Some(&n) = index.get(&s.node) else { continue };
        let r = &s.restraint;
        for (d, on) in [r.tx, r.ty, r.tz, r.rx, r.ry, r.rz].into_iter().enumerate() {
            if on && !fixed.contains(&(n, d)) {
                fixed.push((n, d));
            }
        }
    }
    let frame = Frame3d { nodes, elems, fixed };

    // ----- loads ---------------------------------------------------------------
    let mut loads: Vec<Case> = (0..nc).map(|_| Case::zeros(frame.nodes.len(), frame.elems.len())).collect();
    let mut applied = vec![0.0; nc];
    let len = |k: usize| frame.nodes[frame.elems[k].i].distance(frame.nodes[frame.elems[k].j]);
    for (ci, case) in model.load_cases.iter().enumerate() {
        if case.self_weight {
            for (k, &(m, _)) in seg.iter().enumerate().filter_map(|(k, s)| s.as_ref().map(|s| (k, s))) {
                let mem = model.member(m);
                let w = model.material(mem.material).density * G * model.section(mem.section).props.area;
                loads[ci].w[k] += Vec3::Z * -w;
                applied[ci] += w * len(k);
            }
        }
    }
    let mut area_loads = 0;
    for load in &model.loads {
        match load {
            Load::Node { case, node, force, moment } => {
                let Some(&n) = index.get(node) else { continue };
                let l = &mut loads[case.idx()].nodal[n];
                for (d, v) in [force.x, force.y, force.z, moment.x, moment.y, moment.z].into_iter().enumerate() {
                    l[d] += v;
                }
                applied[case.idx()] += -force.z;
            }
            Load::MemberUniform { case, member, w, range } => {
                for (k, &(m, t0)) in seg.iter().enumerate().filter_map(|(k, s)| s.as_ref().map(|s| (k, s))).filter(|(_, s)| s.0 == *member) {
                    // Over part of the member: averaged over each element it covers.
                    let l = len(k);
                    let share = match range {
                        Some((a, b)) => ((t0 + l).min(*b) - t0.max(*a)).max(0.0) / l,
                        None => 1.0,
                    };
                    let _ = m;
                    loads[case.idx()].w[k] += *w * share;
                    applied[case.idx()] += -w.z * share * l;
                }
            }
            Load::Area { .. } => area_loads += 1,
        }
    }
    if area_loads > 0 {
        issues.push(Issue::new(Severity::Warning, "area-load-unassigned", format!("{area_loads} area loads are not applied: a frame analysis takes loads on members and points")));
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
    let mut diagrams: Vec<Vec<Diagram>> = vec![vec![Diagram::default(); nm]; nc];
    let mut reactions: Vec<Vec<Reaction>> = vec![vec![]; nc];
    let ts: Vec<f64> = (0..=SAMPLES).map(|i| i as f64 / SAMPLES as f64).collect();
    match frame.solve(&loads) {
        Ok((sols, held)) => {
            // A point nothing holds in place (not merely free to spin) is worth saying.
            let loose: Vec<NodeId> = held.iter().filter(|h| h.1 < 3).filter_map(|h| node_ids[h.0]).collect();
            if !loose.is_empty() {
                issues.push(Issue::new(Severity::Warning, "frame-loose", format!("{} points are not held in every direction (no load acts on them, so they are left where they are)", loose.len())).nodes(loose));
            }
            for (ci, sol) in sols.iter().enumerate() {
                for (k, &(m, t0)) in seg.iter().enumerate().filter_map(|(k, s)| s.as_ref().map(|s| (k, s))) {
                    let d = &mut diagrams[ci][m.idx()];
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
                for s in &model.supports {
                    let Some(&n) = index.get(&s.node) else { continue };
                    let comp = |d: usize| frame.fixed.iter().position(|f| *f == (n, d)).map(|i| sol.reactions[i]).unwrap_or(0.0);
                    let Some(member) = model.members.iter().find(|m| m.path.contains(&s.node)) else { continue };
                    if reactions[ci].iter().any(|r| r.node == Some(s.node)) {
                        continue;
                    }
                    reactions[ci].push(Reaction { member: member.id, by: None, node: Some(s.node), at: model.pos(s.node), force: Vec3::new(comp(0), comp(1), comp(2)), transfer: Transfer::Bearing });
                }
            }
        }
        Err(e) => {
            let message = match e {
                SolveError::Mechanism { node, dof } => {
                    let at: Vec<String> = match node_ids[node] {
                        Some(id) => model.members.iter().filter(|m| m.path.contains(&id)).take(3).map(|m| model.member_path(m.id)).collect(),
                        None => vec![],
                    };
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
        }
    }
    Takedown { cases, info, diagrams, reactions, applied, issues }
}
