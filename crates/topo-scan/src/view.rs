//! What the app draws from: the workspace's state, the members with their
//! evidence, and a proposal as before / after with what changed.

use anyhow::Result;
use serde_json::{json, Value};

use crate::workspace::{tally_counts, Workspace};
use crate::{evidence_colour, Evidence, Prediction, V3};

fn mm(p: V3) -> [f64; 3] {
    p.map(|x| (x as f64 * 1000.0).round() / 1000.0)
}

/// "supported", "contradicted" (≥ 35% of rays through it) or "unseen".
fn evidence_state(e: &Evidence) -> &'static str {
    if e.rays.pierced + e.rays.ended < 100 && e.points < 50 {
        "unseen"
    } else if e.rays.pierce_frac() >= 0.35 {
        "contradicted"
    } else {
        "supported"
    }
}

fn members_of(pred: &Prediction, ev: &[Evidence]) -> Vec<Value> {
    pred.members.iter().zip(ev).map(|(m, e)| {
        json!({
            "path": m.path,
            "role": m.role,
            "centre": mm(m.obb.centre),
            "size": [m.obb.length, m.obb.width, m.obb.depth],
            "faces": m.faces.iter().map(|f| f.poly.iter().map(|p| mm(*p)).collect::<Vec<_>>()).collect::<Vec<_>>(),
            "colour": evidence_colour(e, 0.5, 100),
            "evidence": {
                "state": evidence_state(e),
                "points": e.points,
                "density": e.density.round(),
                "offset_mm": (e.mean_offset as f64 * 1e4).round() / 10.0,
                "rms_mm": (e.rms as f64 * 1e4).round() / 10.0,
                "rays_through": e.rays.pierced,
                "rays_ending": e.rays.ended,
            },
        })
    }).collect()
}

/// Whether two versions of a member are the same solid (to a millimetre).
fn same_solid(a: &crate::PredMember, b: &crate::PredMember) -> bool {
    a.faces.len() == b.faces.len() && a.faces.iter().zip(&b.faces).all(|(f, g)| f.poly.len() == g.poly.len() && f.poly.iter().zip(&g.poly).all(|(p, q)| (0..3).all(|k| (p[k] - q[k]).abs() < 1e-3)))
}

impl Workspace {
    /// Everything but geometry: the project, the scan, the placement, who
    /// is here, the issues and the named things.
    pub fn state_json(&self) -> Value {
        let ev = self.current_evidence();
        json!({
            "rev": self.rev,
            "project": {"root": self.repo.root.display().to_string(), "branch": self.repo.branch(), "head": self.repo.head(), "model": self.model_rel},
            "scan": match &self.scan {
                Some(s) => json!({"path": s.path, "loaded": true, "points": s.xyz.len(), "used": s.used.len(), "floor_z": s.floor_z, "rays": s.sensor.is_some(), "min_range": s.min_range, "ranged": s.ranged}),
                None => json!({"path": self.items.scan, "loaded": false, "min_range": self.items.min_range()}),
            },
            // (Where the scan is: a placement being tried, else where the model's Scan frame has it, else the one kept.)
            "placement": self.trial.or_else(|| self.model.as_ref().and_then(|m| m.pred.scan_frame)).map(crate::workspace::PlacementJson::from).or(self.items.placement.clone()),
            "model": {
                "name": self.model.as_ref().map(|m| m.pred.name.clone()),
                "members": self.model.as_ref().map_or(0, |m| m.pred.members.len()),
                "rev": self.model_rev,
                "error": self.model_error,
            },
            "tally": ev.map(tally_counts),
            "participants": self.store.participants.iter().map(|p| json!({"id": p.id, "name": p.name, "kind": p.kind, "colour": p.colour, "presence": self.presence_of(&p.id)})).collect::<Vec<_>>(),
            "issues": self.store.issues,
            "views": self.items.views,
            "sections": self.items.sections,
            "rooms": self.items.rooms,
            "surfaces": self.items.surfaces,
            "selections": self.items.selections,
            // (The latest: a chat's older messages are read with the tool.)
            "chat": self.chat.messages[self.chat.messages.len().saturating_sub(400)..],
            "slabs": self.items.slabs,
            "polylines": self.items.polylines,
        })
    }

    /// The model's members (model frame) with their evidence. With a
    /// proposal's commit: `before` and `after`, and per member path what
    /// the proposal does to it ("added", "removed" or "moved").
    pub fn members_json(&mut self, proposal: Option<&str>) -> Result<Value> {
        let ev = self.evidence()?;
        let before = members_of(&self.model()?.pred, &ev);
        let Some(commit) = proposal else {
            return Ok(json!({"model_rev": self.model_rev, "members": before}));
        };
        let after = self.proposal_evaluated(commit)?;
        let now = &self.model()?.pred;
        let mut changes = serde_json::Map::new();
        for m in &after.pred.members {
            match now.members.iter().find(|x| x.path == m.path) {
                None => changes.insert(m.path.clone(), json!("added")),
                Some(x) if !same_solid(x, m) => changes.insert(m.path.clone(), json!("moved")),
                _ => None,
            };
        }
        for m in &now.members {
            if !after.pred.members.iter().any(|x| x.path == m.path) {
                changes.insert(m.path.clone(), json!("removed"));
            }
        }
        Ok(json!({
            "model_rev": self.model_rev,
            "before": before,
            "after": members_of(&after.pred, &after.ev),
            "changes": changes,
            "diff": self.repo.diff(commit)?,
        }))
    }

    /// What a ray from a viewer meets first (scan frame): the scan point,
    /// where that is in the model, and the nearest member. `z` (model
    /// frame) and `slab` (origin, unit normal, half thickness; scan frame)
    /// are the viewer's cuts: points outside them are not shown, so not hit.
    pub fn pick_json(&mut self, origin: V3, dir: V3, z: Option<[f32; 2]>, slab: Option<(V3, [V3; 3], V3)>) -> Result<Value> {
        let pl = self.placement()?;
        self.scan()?;
        let scan = self.scan.as_ref().unwrap().clone();
        let n = crate::norm(dir).max(1e-9);
        let dir = crate::scale(dir, 1.0 / n);
        let keep = move |p: V3| {
            z.is_none_or(|z| p[2] - pl.t[2] >= z[0] && p[2] - pl.t[2] <= z[1]) && slab.is_none_or(|(c, axes, half)| crate::cloud::inside(p, c, &axes, half))
        };
        // A narrow ray first; where the points are sparse, a wider one.
        let Some((i, t)) = scan.ray_hit(origin, dir, 0.015, 0.004, 1, &keep).or_else(|| scan.ray_hit(origin, dir, 0.04, 0.012, 1, &keep)) else {
            return Ok(json!({"hit": null}));
        };
        let p = scan.xyz[i as usize];
        let pm = pl.inverse(p);
        // The member there, with the normal of its face nearest the point (model frame).
        let near = self.model.as_ref().and_then(|m| {
            let (s, d) = m.samples.nearest(pm, 0.5)?;
            let (k, nm, q) = (m.samples.member[s as usize] as usize, m.samples.normals[s as usize], m.samples.pts[s as usize]);
            Some(json!({"path": m.pred.members[k].path, "distance": d, "offset": crate::dot(nm, crate::sub(pm, q)), "normal": mm(nm)}))
        });
        // The surface the point lies on in the scan: its normal, in the model frame.
        let surface = self.index.as_ref().and_then(|ix| ix.normal_at(p)).map(|n| mm(pl.unrotate(n)));
        // The selections the point is one of.
        let ids: Vec<String> = self.items.selections.iter().map(|s| s.id.clone()).collect();
        let selections: Vec<String> = ids.into_iter().filter(|id| self.selection_indices(id).is_ok_and(|ix| ix.contains(&i))).collect();
        Ok(json!({"hit": mm(p), "model": mm(pm), "range": t, "member": near, "normal": surface, "selections": selections}))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use crate::git::tests::temp_repo;
    use crate::issues::Body;
    use crate::scan::Scan;
    use crate::workspace::Workspace;
    use crate::{predict_with, FaceSamples, Placement, V3};

    fn shed(depth_ft: u32) -> String {
        format!(
            "import {{ Building, DFL, ft, Perimeter, Wall }} from \"topo-cad\";\n\
             const width = ft(12);\n\
             const depth = ft({depth_ft});\n\
             const wall = Wall.template({{ height: ft(8), grade: DFL.No2 }});\n\
             const walls = Perimeter.start(\"Walls\", [0, 0])\n\
             \x20 .to([width, 0], wall.named(\"South wall\"))\n\
             \x20 .to([width, depth], wall.named(\"East wall\"))\n\
             \x20 .to([0, depth], wall.named(\"North wall\"))\n\
             \x20 .close(wall.named(\"West wall\"));\n\
             export default Building.named(\"Shed\").add(walls);\n"
        )
    }

    /// A scan of the shed as built (16 ft deep), seen from its middle: the
    /// points of its members' faces and its floor, placed by `pl`.
    fn scan_of_truth(pl: &Placement) -> Scan {
        let truth = predict_with("shed.ts", &mut |_| Some(shed(16))).unwrap();
        let samples = FaceSamples::new(&truth, 0.03);
        let mut xyz: Vec<V3> = samples.pts.iter().map(|&p| pl.apply(p)).collect();
        for i in 0..120 {
            for j in 0..160 {
                xyz.push(pl.apply([0.03 * i as f32, 0.03 * j as f32, 0.0]));
            }
        }
        let sensor = vec![pl.apply([1.8, 2.4, 1.2]); xyz.len()];
        Scan::new("synthetic", xyz, Vec::new(), Some(sensor), None, 1.0).unwrap()
    }

    #[test]
    fn a_face_placed_by_hand_moves_what_is_free_and_can_be_taken_back() {
        // The model has the shed a foot short, and the scan's frame where the scan is; the person drags the north
        // wall's room face to where the scan has it.
        let pl = Placement { yaw: 0.3, t: [2.0, -1.0, 0.5] };
        let model = shed(15)
            .replace("import { Building,", "import { Building, Frame, Placement,")
            .replace("export default Building.named(\"Shed\").add(walls);", &format!(
                "import {{ seen }} from \"./shed.seen\";\nconst scan = Frame.named(\"Scan\").placed(Placement.at([{}, {}, {}], {}));\nexport default Building.named(\"Shed\").add(walls, scan, seen);",
                pl.t[0], pl.t[1], pl.t[2], (pl.yaw as f64).to_degrees()));
        let repo = temp_repo("seen", &[("shed.ts", &model), ("shed.seen.ts", "export const seen = [];\n")]);
        let root = repo.root.clone();
        let mut ws = Workspace::open(&root.join("shed.ts"), None).unwrap();
        ws.scan = Some(Arc::new(scan_of_truth(&pl)));
        ws.join("p1", "person").unwrap();
        ws.call("p1", "rename", &json!({"name": "Steven"})).unwrap();
        // (No placement was kept: the app is told where the model's Scan frame has the scan.)
        assert!((ws.state_json()["placement"]["yaw_deg"].as_f64().unwrap().abs() - (pl.yaw as f64).to_degrees()).abs() < 1e-3, "{}", ws.state_json()["placement"]);
        let north = |ws: &Workspace| ws.model().unwrap().pred.members.iter().find(|m| m.path == "Shed/Walls/North wall/bottom_plate").unwrap().obb.centre[1];
        let y0 = north(&ws);
        let ft = 0.3048f32;
        let inside = 16.0 * ft - 0.0889;
        let out = ws.call("p1", "seen", &json!({"action": "add", "member": "Shed/Walls/North wall/bottom_plate", "face": [0, -1, 0], "point": [1.8, inside, 0.0], "view": "b1"})).unwrap();
        let said = out[0]["text"].as_str().unwrap();
        assert!(said.contains("reading 1") && said.contains("North wall") && !said.contains("NOT MET"), "{said}");
        assert!((north(&ws) - (y0 + ft)).abs() < 2e-3, "{} → {}", y0, north(&ws));
        let file = std::fs::read_to_string(root.join("shed.seen.ts")).unwrap();
        assert!(file.contains("scan.on(") && file.contains("// 1: Steven") && file.contains("in b1"), "{file}");
        assert!(ws.call("p1", "seen", &json!({})).unwrap()[0]["text"].as_str().unwrap().contains("mm off"));
        // Taken back, the wall is where it was; nothing is left uncommitted.
        ws.call("p1", "seen", &json!({"action": "remove", "n": 1})).unwrap();
        assert!((north(&ws) - y0).abs() < 1e-4);
        let dirty = std::process::Command::new("git").arg("-C").arg(&root).args(["status", "--porcelain"]).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&dirty.stdout), "");
        // A model that does not add them: refused, nothing kept.
        std::fs::write(root.join("shed.ts"), model.replace("walls, scan, seen", "walls, scan")).unwrap();
        ws.sync();
        let e = ws.call("p1", "seen", &json!({"action": "add", "member": "Shed/Walls/North wall/bottom_plate", "face": [0, -1, 0], "point": [1.8, inside, 0.0]})).unwrap_err();
        assert!(format!("{e:#}").contains(".add(seen)"), "{e:#}");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn an_issue_is_raised_proposed_on_and_applied() {
        // The model has the shed a foot short.
        let repo = temp_repo("workspace", &[("shed.ts", &shed(15))]);
        let root = repo.root.clone();
        let pl = Placement { yaw: 0.3, t: [2.0, -1.0, 0.5] };
        let mut ws = Workspace::open(&root.join("shed.ts"), None).unwrap();
        ws.scan = Some(Arc::new(scan_of_truth(&pl)));
        ws.set_placement(pl, "topo-cad").unwrap();
        ws.join("p1", "person").unwrap();
        ws.join("claude", "agent").unwrap();
        ws.call("p1", "rename", &json!({"name": "Steven"})).unwrap();

        // The person raises an issue on the north wall; the agent hears of it and takes it.
        ws.call("p1", "issues", &json!({"action": "create", "title": "North wall looks short", "member": "North wall", "priority": "high"})).unwrap();
        assert!(ws.has_activity("claude"));
        let news = ws.activity_for("claude");
        assert!(news.contains("North wall looks short") && news.contains("(new)"), "{news}");
        assert!(!ws.has_activity("claude"));
        ws.call("claude", "issues", &json!({"action": "update", "id": 1, "status": "doing"})).unwrap();
        assert_eq!(ws.store.get(1).unwrap().assignee.as_deref(), Some("claude"));

        // It looks (noted in the thread), then proposes the fix: scored, not applied.
        ws.call("claude", "members", &json!({"filter": "North wall", "issue": 1})).unwrap();
        let out = ws.call("claude", "propose", &json!({"issue": 1, "title": "North leg is 16 ft", "edits": [{"old": "const depth = ft(15);", "new": "const depth = ft(16);"}]})).unwrap();
        assert!(out[0]["text"].as_str().unwrap().contains("Not applied"));
        assert_eq!(std::fs::read_to_string(root.join("shed.ts")).unwrap(), shed(15));
        let issue = ws.store.get(1).unwrap().clone();
        let Some(Body::Proposal { commit, before, after, state, files, .. }) = issue.entries.iter().map(|e| &e.body).find(|b| matches!(b, Body::Proposal { .. })) else { panic!("no proposal in {issue:?}") };
        assert_eq!((state.as_str(), files.as_slice()), ("open", &["shed.ts".to_string()][..]));
        assert!(after.contradicted < before.contradicted, "{before:?} → {after:?}");
        assert!(issue.entries.iter().any(|e| matches!(&e.body, Body::Tool { name, .. } if name == "members")));

        // The app's diff of it: the north wall's members move, the others do not.
        let d = ws.members_json(Some(commit)).unwrap();
        let changes = d["changes"].as_object().unwrap();
        assert!(changes.iter().any(|(k, v)| k.contains("North wall") && v == "moved"), "{changes:?}");
        assert!(!changes.keys().any(|k| k.contains("South wall")), "{changes:?}");
        assert!(d["diff"].as_str().unwrap().contains("+const depth = ft(16);"));

        // The person hears of it and applies it: the file, the model and the history change.
        assert!(ws.activity_for("p1").contains("proposed"));
        ws.call("p1", "proposal", &json!({"issue": 1, "n": 1, "action": "apply"})).unwrap();
        assert_eq!(std::fs::read_to_string(root.join("shed.ts")).unwrap(), shed(16));
        let st = ws.state_json();
        assert_eq!(st["tally"]["contradicted"], json!(after.contradicted));
        assert_eq!(st["issues"][0]["entries"].as_array().unwrap().iter().filter(|e| e["kind"] == "proposal").map(|e| e["state"].clone()).collect::<Vec<_>>(), vec![json!("applied")]);
        assert_eq!(st["participants"][0]["name"], "Steven");
        let log = std::process::Command::new("git").arg("-C").arg(&root).args(["log", "--format=%an/%cn: %s"]).output().unwrap();
        let log = String::from_utf8_lossy(&log.stdout).into_owned();
        assert!(log.contains("claude/Steven: North leg is 16 ft"), "{log}");
        assert!(log.contains("#1: proposal 1: North leg is 16 ft"), "{log}");
        // Nothing is left uncommitted.
        let dirty = std::process::Command::new("git").arg("-C").arg(&root).args(["status", "--porcelain"]).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&dirty.stdout), "");

        // And reverted, it is as it was.
        ws.call("p1", "proposal", &json!({"issue": 1, "n": 1, "action": "revert"})).unwrap();
        assert_eq!(std::fs::read_to_string(root.join("shed.ts")).unwrap(), shed(15));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
