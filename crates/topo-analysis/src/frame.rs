//! The whole model as one 3D frame (`Standards::analysis = "frame"`), for
//! things that are not light framing: a welded tube frame, a machine base of
//! bolted extrusions. Every member is a beam through its nodes; where a
//! member ends, its connection's fixity says how it is held (rigid, pinned,
//! or on a connector's rotational spring); loads act in any direction.
//!
//! Assumptions: Euler–Bernoulli beams (no shear deformation) on the node
//! lines (a section anchored off its axis is analysed on the axis); joints
//! have no size. The results take the same form as a load takedown's, so
//! checks, diagrams and drawings work on either.

use crate::frame3d::{Case, Elem, EndRelease, Frame3d, SolveError};
use crate::loadpath::Transfer;
use crate::takedown::{ei_vertical_plane, Behaviour, Diagram, MemberInfo, Reaction, SupportPt, Takedown, G};
use std::collections::HashMap;
use topo_core::{Fixity, Issue, Load, LoadCaseId, MemberId, Model, NodeId, Severity, Vec3};
use topo_geom::Geometry;

/// Samples per element (between two nodes of a member).
const SAMPLES: usize = 16;

/// How a member's end at `node` is held: its recorded connection's fixity,
/// continuous if none is recorded.
fn end_release(model: &Model, m: MemberId, node: NodeId) -> EndRelease {
    let fixity = model.joint(node).and_then(|j| j.connections.iter().find(|c| c.member == m)).map(|c| model.connection(c.connection).fixity);
    match fixity {
        None | Some(Fixity::Rigid) => None,
        Some(Fixity::Pinned) => Some([0.0, 0.0]),
        Some(Fixity::Semi { k_u, k_v }) => Some([k_u, k_v]),
    }
}

pub fn frame(model: &Model, geom: &Geometry) -> Takedown {
    let cases: Vec<LoadCaseId> = model.load_cases.iter().map(|c| c.id).collect();
    let (nm, nc) = (model.members.len(), cases.len());
    let mut issues = vec![];

    // ----- nodes and elements ------------------------------------------------
    let mut index: HashMap<NodeId, usize> = HashMap::new();
    let mut nodes: Vec<Vec3> = vec![];
    let mut node_ids: Vec<NodeId> = vec![];
    let mut elems: Vec<Elem> = vec![];
    // Per element: member, distance of its i end along the member.
    let mut seg: Vec<(MemberId, f64)> = vec![];
    for m in &model.members {
        let (sec, mat) = (&model.section(m.section).props, model.material(m.material));
        let v = geom.member(m.id).place.v;
        let start = model.pos(m.start());
        let last = m.path.len() - 2;
        for (k, w) in m.path.windows(2).enumerate() {
            let mut ix = |n: NodeId| {
                *index.entry(n).or_insert_with(|| {
                    nodes.push(model.pos(n));
                    node_ids.push(n);
                    nodes.len() - 1
                })
            };
            let (i, j) = (ix(w[0]), ix(w[1]));
            seg.push((m.id, model.pos(w[0]).distance(start)));
            elems.push(Elem {
                i,
                j,
                v,
                ea: mat.e * sec.area,
                gj: mat.g * sec.j,
                ei_u: mat.e * sec.i_u,
                ei_v: mat.e * sec.i_v,
                rel_i: if k == 0 { end_release(model, m.id, w[0]) } else { None },
                rel_j: if k == last { end_release(model, m.id, w[1]) } else { None },
            });
        }
    }
    let mut fixed: Vec<(usize, usize)> = vec![];
    for s in &model.supports {
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
            for (k, &(m, _)) in seg.iter().enumerate() {
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
                for (k, &(m, t0)) in seg.iter().enumerate().filter(|(_, s)| s.0 == *member) {
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
            let loose: Vec<NodeId> = held.iter().filter(|h| h.1 < 3).map(|h| node_ids[h.0]).collect();
            if !loose.is_empty() {
                issues.push(Issue::new(Severity::Warning, "frame-loose", format!("{} points are not held in every direction (no load acts on them, so they are left where they are)", loose.len())).nodes(loose));
            }
            for (ci, sol) in sols.iter().enumerate() {
                for (k, &(m, t0)) in seg.iter().enumerate() {
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
                    let id = node_ids[node];
                    let at: Vec<String> = model.members.iter().filter(|m| m.path.contains(&id)).take(3).map(|m| model.member_path(m.id)).collect();
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
