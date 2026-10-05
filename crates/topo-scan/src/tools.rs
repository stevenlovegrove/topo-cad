//! The workspace's tools that read the scan against the model: placement,
//! members' evidence, section images, measurements, wall profiles, stud
//! set-outs and plane fits. They report evidence with what is needed to
//! weigh it; none of them decides what a surface is.

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::render::{base64, render_section, MemberLayer, PointColour, Section, SectionRequest};
use crate::stats::point_stats;
use crate::workspace::{arr3, describe, f3, Room, SectionDef, Surface, Workspace};
use crate::{evidence_colour, Evidence, Placement, V3};

impl Workspace {
    /// Supported / contradicted / unseen member counts.
    pub fn tally(ev: &[Evidence]) -> String {
        let (mut sup, mut con, mut unseen) = (0, 0, 0);
        for e in ev {
            if e.rays.pierced + e.rays.ended < 100 && e.points < 50 {
                unseen += 1;
            } else if e.rays.pierce_frac() >= 0.35 {
                con += 1;
            } else {
                sup += 1;
            }
        }
        format!("{sup} members supported, {con} contradicted (≥35% of rays through them), {unseen} unseen")
    }

    pub(crate) fn frame_is_model(args: &Value) -> Result<bool> {
        match args.get("frame").and_then(|f| f.as_str()).unwrap_or("model") {
            "model" => Ok(true),
            "scan" => Ok(false),
            f => bail!("frame must be \"model\" or \"scan\", not {f:?}"),
        }
    }
    /// Frame → scan.
    pub(crate) fn to_scan(&mut self, p: V3, model: bool) -> Result<V3> {
        Ok(if model { self.placement()?.apply(p) } else { p })
    }

    /// Scan point indices inside an axis-aligned box of `frame`.
    pub(crate) fn in_box(&mut self, b: &Value, model: bool) -> Result<Vec<u32>> {
        let (lo, hi) = (arr3(&b["min"]).context("box.min: [x, y, z]")?, arr3(&b["max"]).context("box.max: [x, y, z]")?);
        let pl = if model { Some(self.placement()?) } else { None };
        let scan = self.scan()?;
        use rayon::prelude::*;
        Ok(scan.used.par_iter().copied().filter(|&i| {
            let p = scan.xyz[i as usize];
            let p = pl.map_or(p, |pl| pl.inverse(p));
            (0..3).all(|a| p[a] >= lo[a] && p[a] <= hi[a])
        }).collect())
    }

    pub(crate) fn members_matching(&self, filter: Option<&str>) -> Result<Vec<usize>> {
        let m = self.model()?;
        Ok((0..m.pred.members.len()).filter(|&k| filter.is_none_or(|f| m.pred.members[k].path.contains(f) || m.pred.members[k].role == f)).collect())
    }

    /// The nearest model face to a point (model frame): member, distance,
    /// offset along the face normal.
    pub(crate) fn nearest_member(&self, pm: V3, gate: f32) -> Option<(usize, f32, f32)> {
        let m = self.model.as_ref()?;
        let (i, d) = m.samples.nearest(pm, gate)?;
        let n = m.samples.normals[i as usize];
        let q = m.samples.pts[i as usize];
        let off = (pm[0] - q[0]) * n[0] + (pm[1] - q[1]) * n[1] + (pm[2] - q[2]) * n[2];
        Some((m.samples.member[i as usize] as usize, d, off))
    }
    /// Scores a new placement against the old; keeps it unless `dry_run`.
    pub(crate) fn try_placement(&mut self, who: &str, before: Placement, p: Placement, args: &Value, what: &str) -> Result<Vec<Value>> {
        // Where the model has the scan in its Scan frame, that is where it is: the placement is
        // the script's, changed by changing the frame's placement there (a proposal).
        let in_script = self.model()?.pred.scan_frame.is_some();
        let dry = in_script || args.get("dry_run").and_then(|v| v.as_bool()).unwrap_or(false);
        let sb = self.score_text(&before)?;
        let sa = self.score_text(&p)?;
        let ev_before = self.evidence()?;
        self.trial = Some(p);
        let ev_after = self.evidence();
        self.trial = None;
        let ev_after = ev_after?;
        if !dry {
            self.set_placement(p, who)?;
        }
        let mut said = format!(
            "{what}{}:\n  before: {}\n          {sb}\n          {}\n  after:  {}\n          {sa}\n          {}",
            if dry { " (dry run: not kept)" } else { "" },
            describe(&before), Self::tally(&ev_before), describe(&p), Self::tally(&ev_after));
        if in_script {
            // The Scan frame that puts the scan there: model → scan, turned back.
            let o = p.inverse([0.0; 3]);
            said += &format!(
                "\n\nThe model places the scan with its Scan frame, so the placement is the script's. To keep this one, propose the frame placed so:\n  Frame.named(\"Scan\").placed(Placement.at([{:.4}, {:.4}, {:.4}], {:.4}))",
                o[0], o[1], o[2], -p.yaw.to_degrees());
        }
        Ok(vec![json!({"type": "text", "text": said})])
    }
    pub(crate) fn members(&mut self, args: &Value) -> Result<Vec<Value>> {
        // The model as it is, or as a proposal or a trial of some edits would have it.
        let alt = self.alternative(args)?;
        let ev = match &alt {
            Some((e, _)) => e.ev.clone(),
            None => self.evidence()?,
        };
        let pl = self.placement()?;
        let pred = match &alt {
            Some((e, _)) => &e.pred,
            None => &self.model()?.pred,
        };
        let filter = args.get("filter").and_then(|v| v.as_str());
        let mut ks: Vec<usize> = (0..pred.members.len()).filter(|&k| filter.is_none_or(|f| pred.members[k].path.contains(f) || pred.members[k].role == f)).collect();
        match args.get("sort").and_then(|v| v.as_str()).unwrap_or("path") {
            "contradicted" => ks.sort_by(|&a, &b| ev[b].rays.pierce_frac().total_cmp(&ev[a].rays.pierce_frac())),
            "support" => ks.sort_by(|&a, &b| ev[b].density.total_cmp(&ev[a].density)),
            _ => {}
        }
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(60) as usize;
        let mut s = format!("{} members match{}; model frame; size = length × width × depth (m); x, y, z: its extent along each model axis\n", ks.len(), alt.as_ref().map_or(String::new(), |(_, l)| format!(", {l}")));
        s += &format!("{:<52} {:<10} {:>26} {:>20} {:>7} {:>7} {:>8} {:>13}  {}\n", "path", "role", "centre", "size", "points", "pts/m²", "off mm", "rays through", "x, y, z extents");
        for &k in ks.iter().take(limit) {
            let x = &pred.members[k];
            let e = &ev[k];
            // Where it is, in plain numbers: what a face measured in the scan is compared with.
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for p in x.faces.iter().flat_map(|f| f.poly.iter()) {
                for a in 0..3 {
                    lo[a] = lo[a].min(p[a]);
                    hi[a] = hi[a].max(p[a]);
                }
            }
            s += &format!("{:<52} {:<10} {:>26} {:>20} {:>7} {:>7.0} {:>8.1} {:>5.0}% of {:<5}  x {:.3}..{:.3}, y {:.3}..{:.3}, z {:.3}..{:.3}\n", x.path, x.role, f3(x.obb.centre), format!("{:.3}×{:.3}×{:.3}", x.obb.length, x.obb.width, x.obb.depth), e.points, e.density, e.mean_offset * 1e3, 100.0 * e.rays.pierce_frac(), e.rays.pierced + e.rays.ended, lo[0], hi[0], lo[1], hi[1], lo[2], hi[2]);
        }
        if ks.len() == 1 {
            let x = &pred.members[ks[0]];
            s += &format!("axes (along, u, v): {}, {}, {}\ncentre in the scan: {}\n{} faces\n", f3(x.obb.axes[0]), f3(x.obb.axes[1]), f3(x.obb.axes[2]), f3(pl.apply(x.obb.centre)), x.faces.len());
        }
        Ok(vec![json!({"type": "text", "text": s})])
    }
    pub(crate) fn render(&mut self, who: &str, args: &Value) -> Result<Vec<Value>> {
        // A kept slab: the section people see when they look through it (model frame).
        let slab = match args.get("slab").and_then(|v| v.as_str()) {
            Some(id) => Some(self.items.slabs.iter().find(|s| s.id == id || s.name == id).cloned().with_context(|| format!("no slab {id:?}"))?),
            None => None,
        };
        let model = slab.is_some() || Self::frame_is_model(args)?;
        let pl = self.placement()?;
        // The model as it is, or as a proposal or a trial of some edits would have it.
        let alt = self.alternative(args)?;
        let ev = match &alt {
            Some((e, _)) => e.ev.clone(),
            None => self.evidence()?,
        };
        let floor = if model { 0.0 } else { self.scan()?.floor_z };
        let view = if slab.is_some() { "slab" } else { args.get("view").and_then(|v| v.as_str()).unwrap_or("plan") };
        let at = args.get("at").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let thick = args.get("thickness").and_then(|v| v.as_f64()).map(|x| x as f32);
        let section = match view {
            "slab" => {
                let s = slab.as_ref().unwrap();
                Section::new(s.centre, s.axes[2], s.axes[1], 2.0 * s.half[2])
            }
            "plan" => {
                let z = args.get("z").and_then(|v| v.as_array()).map(|a| [a[0].as_f64().unwrap_or(0.1) as f32, a[1].as_f64().unwrap_or(2.4) as f32]).unwrap_or([floor + 0.1, floor + 2.4]);
                Section::new([0.0, 0.0, 0.5 * (z[0] + z[1])], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0], (z[1] - z[0]).abs())
            }
            "x" => Section::new([at, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0], thick.unwrap_or(0.3)),
            "y" => Section::new([0.0, at, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0], thick.unwrap_or(0.3)),
            "custom" => Section::new(
                args.get("origin").and_then(arr3).context("custom: origin [x, y, z]")?,
                args.get("normal").and_then(arr3).context("custom: normal [x, y, z]")?,
                args.get("up").and_then(arr3).unwrap_or([0.0, 0.0, 1.0]),
                thick.unwrap_or(0.3),
            ),
            v => bail!("view must be plan, x, y or custom, not {v:?}"),
        };
        let bounds = args.get("bounds").and_then(|v| v.as_array()).and_then(|a| (a.len() == 4).then(|| std::array::from_fn(|k| a[k].as_f64().unwrap_or(0.0) as f32)))
            // (A slab's own width and height, unless a part of it is asked for.)
            .or(slab.as_ref().map(|s| [-s.half[0], s.half[0], -s.half[1], s.half[1]]));
        let highlight: Vec<String> = args.get("highlight").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
        let show_members = args.get("members").and_then(|v| v.as_bool()).unwrap_or(true);
        let free = args.get("free").and_then(|v| v.as_bool()).unwrap_or(false);
        let tf = move |p: V3| if model { pl.inverse(p) } else { p };
        self.scan()?;
        let pred = match &alt {
            Some((e, _)) => &e.pred,
            None => &self.model()?.pred,
        };
        let mut drawn = Vec::new();
        let members: Vec<MemberLayer> = if show_members {
            pred.members.iter().enumerate().map(|(k, x)| {
                drawn.push(k);
                let c = evidence_colour(&ev[k], 0.5, 100);
                MemberLayer {
                    faces: x.faces.iter().map(|f| f.poly.iter().map(|&p| if model { p } else { pl.apply(p) }).collect()).collect(),
                    colour: [(c[0] * 200.0) as u8, (c[1] * 170.0) as u8, (c[2] * 200.0) as u8],
                    highlight: highlight.iter().any(|h| x.path.contains(h.as_str())),
                }
            }).collect()
        } else {
            Vec::new()
        };
        let scan = self.scan.as_ref().unwrap();
        let req = SectionRequest {
            section,
            xyz: &scan.xyz,
            used: &scan.used,
            to_frame: &tf,
            bounds,
            max_px: args.get("max_px").and_then(|v| v.as_u64()).unwrap_or(1200) as usize,
            colour: match args.get("color").and_then(|v| v.as_str()).unwrap_or("density") {
                "density" => PointColour::Density,
                "depth" | "offset" => PointColour::Depth,
                "normal" | "normals" => PointColour::Normal,
                "intensity" => PointColour::Intensity,
                c => bail!("color must be density, depth, normal or intensity, not {c:?}"),
            },
            intensity: Some(&scan.intensity[..]).filter(|i| i.len() == scan.xyz.len()),
            members,
            layers: Vec::new(),
            marks: Vec::new(),
            eye: None,
            sensor: if free { Some(scan.sensor.as_deref().context("free: this scan has no trajectory, so where each point was seen from is not known")?) } else { None },
        };
        let img = render_section(&req);
        let b = img.bounds;
        let mut s = String::new();
        if let Some(sl) = &slab {
            s += &format!("Slab {} {:?}: what people see looking through it (name it in a comment and it is a link).\n", sl.id, sl.name);
        }
        if let Some((_, label)) = &alt {
            s += &format!("THE MODEL {}, not as it is on disk.\n", label.to_uppercase());
        }
        if free {
            s += "Free space: the blue tint is where the scanner's rays pass inside the slab (stronger: more rays). Where a member is drawn over blue, rays went through it; a real member has none behind its face.\n";
        }
        s += &format!(
            "{view} section, {} frame: image {}×{} px. Right = u = {}, up = v = {}; looking along −n = {}.\n\
             Slab: points within ±{:.3} m of the plane through {}.\n\
             Bounds: u ∈ [{:.3}, {:.3}], v ∈ [{:.3}, {:.3}] m (u, v measured from that point along those axes).\n\
             Pixel (px, py) ↔ u = {:.3} + (px − {:.0}) / {:.2}, v = {:.3} − (py − {:.0}) / {:.2}. Grid every {} m, labelled in metres.\n\
             {} scan points drawn (dark = dense{}). Members: thick = cut by the plane, thin = edges within the slab; \
             green = supported, amber/red = rays pass through it, grey = unseen{}.",
            if model { "model" } else { "scan" }, img.canvas.w, img.canvas.h, f3(section.u), f3(section.v), f3([-section.n[0], -section.n[1], -section.n[2]]),
            section.half, f3(section.origin), b[0], b[1], b[2], b[3], b[0], img.left, img.scale, b[3], img.top, img.scale, img.grid_step, img.points,
            match req.colour {
                PointColour::Density => "",
                PointColour::Depth => "; colour = offset from the cut plane of the point nearest the viewer: blue at the back of the slab, green at the plane, red at the front",
                PointColour::Normal => "; colour = local surface normal: grey where the surface faces the viewer, a hue where it is edge-on (red: facing left/right, cyan: facing up/down), pale where there is no clean surface",
                PointColour::Intensity => "; colour = lidar intensity (dark blue low, through magenta, yellow high), stretched to this slab",
            },
            if highlight.is_empty() { String::new() } else { format!("; magenta = highlighted ({})", highlight.join(", ")) });
        // (Those in the picture: the plane cuts many more outside it.)
        let mut cut: Vec<(usize, [f32; 4])> = drawn.iter().zip(&img.cuts).filter_map(|(&k, c)| c.map(|c| (k, c))).filter(|(_, c)| c[1] >= b[0] && c[0] <= b[1] && c[3] >= b[2] && c[2] <= b[3]).collect();
        cut.sort_by(|a, b| a.1[0].total_cmp(&b.1[0]));
        s += &format!("\nMembers cut by the plane, in the picture ({}):", cut.len());
        for (k, c) in cut.iter().take(80) {
            s += &format!("\n  {:<52} u [{:.3}, {:.3}] v [{:.3}, {:.3}]", pred.members[*k].path, c[0], c[1], c[2], c[3]);
        }
        if cut.len() > 80 {
            s += "\n  …";
        }
        let png = img.canvas.png();
        if let Some(path) = args.get("save").and_then(|v| v.as_str()) {
            std::fs::write(path, &png)?;
            s += &format!("\nsaved to {path}");
        }
        // Keep it by name.
        if let Some(name) = args.get("name").and_then(|v| v.as_str()) {
                let mut kept = args.clone();
                kept.as_object_mut().map(|o| o.remove("save"));
                self.items.sections.retain(|d| d.name != name);
                self.items.sections.push(SectionDef { name: name.to_string(), args: kept });
                self.save_items(who)?;
        }
        Ok(vec![json!({"type": "image", "data": base64(&png), "mimeType": "image/png"}), json!({"type": "text", "text": s})])
    }
    pub(crate) fn measure(&mut self, args: &Value) -> Result<Vec<Value>> {
        let model = Self::frame_is_model(args)?;
        let idx = match args.get("selection").and_then(|v| v.as_str()) {
            Some(id) => self.selection_indices(id)?.to_vec(),
            None => self.in_box(args.get("box").context("give a box {min, max} or a selection's id")?, model)?,
        };
        let pl = self.placement()?;
        let scan = self.scan()?;
        let pts: Vec<V3> = idx.iter().filter(|&&i| (i as usize) < scan.xyz.len()).map(|&i| {
            let p = scan.xyz[i as usize];
            if model { pl.inverse(p) } else { p }
        }).collect();
        let st = point_stats(pts.iter().copied());
        if st.count == 0 {
            return Ok(vec![json!({"type": "text", "text": "no scan points there"})]);
        }
        let pct = |a: usize| {
            let mut v: Vec<f32> = pts.iter().map(|p| p[a]).collect();
            let mut q = |f: f32| {
                let k = ((v.len() - 1) as f32 * f) as usize;
                *v.select_nth_unstable_by(k, |x, y| x.total_cmp(y)).1
            };
            [q(0.02), q(0.5), q(0.98)]
        };
        let mut s = format!("{} scan points ({} frame)\ncentroid {}\nmin {}\nmax {}\n", st.count, if model { "model" } else { "scan" }, f3(st.centroid), f3(st.min), f3(st.max));
        for (a, name) in ["x", "y", "z"].iter().enumerate() {
            let p = pct(a);
            s += &format!("{name}: 2% {:.3}, median {:.3}, 98% {:.3}\n", p[0], p[1], p[2]);
        }
        if let Some(pf) = &st.plane {
            s += &format!("best plane: normal {}, n·p = {:.4}, rms {:.1} mm\n", f3(pf.normal), pf.d, pf.rms * 1e3);
        }
        // The same, each point weighted by how much it is to be believed (its range, the rays, the lidar's flags).
        let kept: Vec<u32> = idx.iter().copied().filter(|&i| (i as usize) < scan.xyz.len()).collect();
        let conf = crate::confidence::confidence(scan, &kept);
        let wts = crate::confidence::weights(scan, &kept, &conf);
        if let Some((c, mut n, rms)) = crate::confidence::weighted_plane(&pts, &wts) {
            if let Some(pf) = &st.plane {
                if crate::dot(n, pf.normal) < 0.0 {
                    n = crate::scale(n, -1.0);
                }
            }
            let low = conf.iter().filter(|&&c| c < 0.5).count();
            s += &format!("weighted by confidence: normal {}, n·p = {:.4}, rms {:.1} mm; {low} of the points doubted ({:.0}%: other rays pass where they are, or the lidar flags them)\n", f3(n), crate::dot(n, c), rms * 1e3, 100.0 * low as f32 / conf.len().max(1) as f32);
        }
        s += &format!("principal spreads (σ, m): {:.3}, {:.3}, {:.4} along {}, {}, {}\n", st.sigma[0], st.sigma[1], st.sigma[2], f3(st.axes[0]), f3(st.axes[1]), f3(st.axes[2]));
        // Nearest members (model frame).
        let mut near: std::collections::HashMap<usize, (u32, f32)> = Default::default();
        for p in pts.iter().step_by((pts.len() / 20_000).max(1)) {
            let pm = if model { *p } else { pl.inverse(*p) };
            if let Some((k, _, off)) = self.nearest_member(pm, 0.15) {
                let e = near.entry(k).or_default();
                e.0 += 1;
                e.1 += off;
            }
        }
        let mut near: Vec<(usize, (u32, f32))> = near.into_iter().collect();
        near.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
        let m = self.model()?;
        s += "nearest model members (points within 15 cm of their faces, sampled; mean offset along the face normal):\n";
        for (k, (n, off)) in near.iter().take(8) {
            s += &format!("  {:<52} {:>6} points, {:+.1} mm\n", m.pred.members[*k].path, n, off / *n as f32 * 1e3);
        }
        Ok(vec![json!({"type": "text", "text": s})])
    }
    pub(crate) fn probe(&mut self, args: &Value) -> Result<Vec<Value>> {
        let model = Self::frame_is_model(args)?;
        let p = args.get("point").and_then(arr3).context("point: [x, y, z]")?;
        let r = args.get("radius").and_then(|v| v.as_f64()).unwrap_or(0.1) as f32;
        let pl = self.placement()?;
        let pm = if model { p } else { pl.inverse(p) };
        let mut s = format!("point {} ({} frame); in the model frame {}, in the scan {}\n", f3(p), if model { "model" } else { "scan" }, f3(pm), f3(pl.apply(pm)));
        match self.nearest_member(pm, 1.0) {
            Some((k, d, off)) => {
                let m = self.model()?;
                let x = &m.pred.members[k];
                s += &format!("nearest model face: {} ({}), {:.3} m away, {:+.3} m along its normal; member centre {}, {:.3}×{:.3}×{:.3} m\n", x.path, x.role, d, off, f3(x.obb.centre), x.obb.length, x.obb.width, x.obb.depth);
            }
            None => s += "no model face within 1 m\n",
        }
        let ps = pl.apply(pm);
        let b = json!({"min": [ps[0] - r, ps[1] - r, ps[2] - r], "max": [ps[0] + r, ps[1] + r, ps[2] + r]});
        let idx: Vec<u32> = self.in_box(&b, false)?;
        let scan = self.scan()?;
        let near: Vec<V3> = idx.iter().map(|&i| scan.xyz[i as usize]).filter(|q| ((q[0] - ps[0]).powi(2) + (q[1] - ps[1]).powi(2) + (q[2] - ps[2]).powi(2)).sqrt() <= r).map(|q| if model { pl.inverse(q) } else { q }).collect();
        let st = point_stats(near.iter().copied());
        s += &format!("{} scan points within {r} m", st.count);
        if let Some(pf) = st.plane.filter(|_| st.count >= 10) {
            s += &format!("; their plane: normal {}, n·p = {:.4}, rms {:.1} mm; centroid {}", f3(pf.normal), pf.d, pf.rms * 1e3, f3(st.centroid));
        }
        Ok(vec![json!({"type": "text", "text": s})])
    }
    pub(crate) fn rooms(&mut self, who: &str, args: &Value) -> Result<Vec<Value>> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("list");
        let name = args.get("name").and_then(|v| v.as_str());
        let out = match action {
            "list" => {
                let mut s = format!("{} rooms (boxes in the model frame):\n", self.items.rooms.len());
                for r in &self.items.rooms {
                    s += &format!("  {:?}: {} – {}, {:.2} × {:.2} × {:.2} m{}\n", r.name, f3(r.min), f3(r.max), r.max[0] - r.min[0], r.max[1] - r.min[1], r.max[2] - r.min[2], if r.note.is_empty() { String::new() } else { format!(" ({})", r.note) });
                }
                s
            }
            "set" => {
                let name = name.context("name")?.to_string();
                let (lo, hi) = (args.get("min").and_then(arr3).context("min [x, y, z] (model frame)")?, args.get("max").and_then(arr3).context("max [x, y, z] (model frame)")?);
                let note = args.get("note").and_then(|v| v.as_str()).unwrap_or("").to_string();
                self.items.rooms.retain(|r| r.name != name);
                self.items.rooms.push(Room { name: name.clone(), min: lo, max: hi, note });
                self.save_items(who)?;
                format!("room {name:?} set")
            }
            "delete" => {
                let name = name.context("name")?;
                self.items.rooms.retain(|r| r.name != name);
                self.save_items(who)?;
                format!("deleted room {name:?}")
            }
            a => bail!("action must be list, set or delete, not {a:?}"),
        };
        self.touch();
        Ok(vec![json!({"type": "text", "text": out})])
    }
    /// Every wall of the model against the scan: where the scan's surface
    /// lies across each wall, beside the wall's faces.
    pub(crate) fn walls(&mut self, args: &Value) -> Result<Vec<Value>> {
        let pl = self.placement()?;
        let band = args.get("band").and_then(|v| v.as_f64()).unwrap_or(0.6) as f32;
        let zr = args.get("z").and_then(|v| v.as_array()).map(|a| [a[0].as_f64().unwrap_or(0.3) as f32, a[1].as_f64().unwrap_or(2.0) as f32]).unwrap_or([0.3, 2.0]);
        let filter = args.get("filter").and_then(|v| v.as_str());
        self.scan()?;
        let (scan, m) = (self.scan.as_ref().unwrap(), self.model()?);
        // The building's middle (for which side of a wall is inside).
        let n_all = m.pred.members.len().max(1) as f32;
        let mid = m.pred.members.iter().fold([0.0f32; 2], |a, x| [a[0] + x.obb.centre[0] / n_all, a[1] + x.obb.centre[1] / n_all]);
        // Scan points at wall height, in the model frame.
        use rayon::prelude::*;
        let pts: Vec<[f32; 2]> = scan.used.par_iter().filter_map(|&i| {
            let p = pl.inverse(scan.xyz[i as usize]);
            (p[2] >= zr[0] && p[2] <= zr[1]).then_some([p[0], p[1]])
        }).collect();
        let mut s = format!(
            "Across each wall: where the scan has surfaces, beside the wall's faces (model frame; scan points with z in [{:.2}, {:.2}], \
             within ±{band} m of the wall, over its length less 0.15 m at each end). Positions are along the wall's outward normal.\n\
             This is a profile, not a verdict. A peak may be the studs' faces, sheathing or cladding seen between studs (near the outside \
             face), a finish on the studs (just inside the inside face), insulation, or things standing against the wall; peaks are \
             histogram centroids (1 cm bins), good to a centimetre or so. To pin a face down, mark the surface, say what it is, and \
             fit it (fit_surface).\n\n",
            zr[0], zr[1]);
        s += &format!("{:<32} {:>6} {:>8} {:>16} {:>9} {:>9}  {}\n", "wall", "len", "normal", "at (along wall)", "inside", "outside", "scan surfaces: position (share of the band's points; from the inside face, from the outside face)");
        for (g, ks) in &crate::groups(&m.pred) {
            if !ks.iter().any(|&k| m.pred.members[k].role == "stud") || filter.is_some_and(|f| !g.contains(f)) {
                continue;
            }
            // The wall's line: the principal direction of its members' centres in plan.
            let cs: Vec<[f32; 2]> = ks.iter().map(|&k| [m.pred.members[k].obb.centre[0], m.pred.members[k].obb.centre[1]]).collect();
            let n = cs.len() as f32;
            let c = cs.iter().fold([0.0f32; 2], |a, p| [a[0] + p[0] / n, a[1] + p[1] / n]);
            let (mut sxx, mut sxy, mut syy) = (0.0f32, 0.0f32, 0.0f32);
            for p in &cs {
                let (dx, dy) = (p[0] - c[0], p[1] - c[1]);
                sxx += dx * dx;
                sxy += dx * dy;
                syy += dy * dy;
            }
            let th = 0.5 * (2.0 * sxy).atan2(sxx - syy);
            let along = [th.cos(), th.sin()];
            // The normal, pointing out of the building.
            let mut nrm = [-along[1], along[0]];
            if (c[0] - mid[0]) * nrm[0] + (c[1] - mid[1]) * nrm[1] < 0.0 {
                nrm = [-nrm[0], -nrm[1]];
            }
            let (mut a0, mut a1, mut n0, mut n1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
            for &k in ks {
                for f in &m.pred.members[k].faces {
                    for p in &f.poly {
                        let (a, b) = (p[0] * along[0] + p[1] * along[1], p[0] * nrm[0] + p[1] * nrm[1]);
                        a0 = a0.min(a);
                        a1 = a1.max(a);
                        n0 = n0.min(b);
                        n1 = n1.max(b);
                    }
                }
            }
            // The scan across the wall: a histogram of 1 cm bins, smoothed over 3.
            let (lo, bins) = (n0 - band, ((n1 - n0 + 2.0 * band) / 0.01).ceil() as usize);
            let mut h = vec![0u32; bins];
            for p in &pts {
                let (a, b) = (p[0] * along[0] + p[1] * along[1], p[0] * nrm[0] + p[1] * nrm[1]);
                if a > a0 + 0.15 && a < a1 - 0.15 && b >= lo {
                    let k = ((b - lo) / 0.01) as usize;
                    if k < bins {
                        h[k] += 1;
                    }
                }
            }
            let total: u32 = h.iter().sum();
            let sm: Vec<u32> = (0..bins).map(|k| h[k.saturating_sub(1)..(k + 2).min(bins)].iter().sum()).collect();
            // Peaks: local maxima, the strongest first, 8 cm apart at least.
            let mut order: Vec<usize> = (0..bins).filter(|&k| sm[k] > 0 && (k == 0 || sm[k] >= sm[k - 1]) && (k + 1 == bins || sm[k] >= sm[k + 1])).collect();
            order.sort_by(|&a, &b| sm[b].cmp(&sm[a]));
            let mut peaks: Vec<(f32, f32)> = Vec::new();
            for k in order {
                // The centroid of the peak's bins.
                let (w, x) = (k.saturating_sub(1)..(k + 2).min(bins)).fold((0.0f32, 0.0f32), |a, j| (a.0 + h[j] as f32, a.1 + h[j] as f32 * (lo + (j as f32 + 0.5) * 0.01)));
                let x = x / w.max(1.0);
                if peaks.iter().all(|p| (p.0 - x).abs() > 0.08) && sm[k] as f32 >= 0.03 * total as f32 {
                    peaks.push((x, sm[k] as f32 / total.max(1) as f32));
                }
                if peaks.len() == 3 {
                    break;
                }
            }
            let name = g.rsplit_once('/').map_or(g.as_str(), |x| x.1);
            let scan_txt = if peaks.is_empty() { "(none)".to_string() } else { peaks.iter().map(|p| format!("{:.3} ({:.0}%; {:+.3}, {:+.3})", p.0, 100.0 * p.1, p.0 - n0, p.0 - n1)).collect::<Vec<_>>().join("  ") };
            s += &format!("{:<32} {:>6.2} {:>8} {:>16} {:>9.3} {:>9.3}  {}\n", name, a1 - a0, format!("{:+.0},{:+.0}", nrm[0], nrm[1]), format!("{:.2}..{:.2}", a0, a1), n0, n1, scan_txt);
        }
        Ok(vec![json!({"type": "text", "text": s})])
    }
    /// Where the scan has studs along each wall, and the set-out they fit.
    pub(crate) fn studs(&mut self, args: &Value) -> Result<Vec<Value>> {
        let pl = self.placement()?;
        let spacing = args.get("spacing").and_then(|v| v.as_f64()).unwrap_or(0.4064) as f32;
        let zr = args.get("z").and_then(|v| v.as_array()).map(|a| [a[0].as_f64().unwrap_or(0.3) as f32, a[1].as_f64().unwrap_or(2.0) as f32]).unwrap_or([0.3, 2.0]);
        let filter = args.get("filter").and_then(|v| v.as_str());
        self.scan()?;
        let (scan, m) = (self.scan.as_ref().unwrap(), self.model()?);
        use rayon::prelude::*;
        let pts: Vec<[f32; 2]> = scan.used.par_iter().filter_map(|&i| {
            let p = pl.inverse(scan.xyz[i as usize]);
            (p[2] >= zr[0] && p[2] <= zr[1]).then_some([p[0], p[1]])
        }).collect();
        let mut s = format!(
            "Studs along each wall, from the scan (points with z in [{:.2}, {:.2}] within the wall's thickness ± 3 cm; 1 cm bins along the wall). \
             Distances are from the wall's start (where its stud#1 is) to a stud's centre. A wall with a finish over its studs, or \
             hidden by things, shows few or none; corner, door and window framing is off the grid. `studsFrom` is the set-out that \
             fits the most runs at {:.3} m; check it against a plan section before keeping it.\n",
            zr[0], zr[1], spacing);
        for (g, ks) in &crate::groups(&m.pred) {
            if !ks.iter().any(|&k| m.pred.members[k].role == "stud") || filter.is_some_and(|f| !g.contains(f)) {
                continue;
            }
            let cs: Vec<[f32; 2]> = ks.iter().map(|&k| [m.pred.members[k].obb.centre[0], m.pred.members[k].obb.centre[1]]).collect();
            let n = cs.len() as f32;
            let c = cs.iter().fold([0.0f32; 2], |a, p| [a[0] + p[0] / n, a[1] + p[1] / n]);
            let (mut sxx, mut sxy, mut syy) = (0.0f32, 0.0f32, 0.0f32);
            for p in &cs {
                let (dx, dy) = (p[0] - c[0], p[1] - c[1]);
                sxx += dx * dx;
                sxy += dx * dy;
                syy += dy * dy;
            }
            let th = 0.5 * (2.0 * sxy).atan2(sxx - syy);
            let (along, nrm) = ([th.cos(), th.sin()], [-th.sin(), th.cos()]);
            let (mut a0, mut a1, mut n0, mut n1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
            let mut modelled: Vec<f32> = Vec::new();
            let mut first: Option<f32> = None;
            for &k in ks {
                let mb = &m.pred.members[k];
                for f in &mb.faces {
                    for p in &f.poly {
                        let (a, b) = (p[0] * along[0] + p[1] * along[1], p[0] * nrm[0] + p[1] * nrm[1]);
                        a0 = a0.min(a);
                        a1 = a1.max(a);
                        n0 = n0.min(b);
                        n1 = n1.max(b);
                    }
                }
                if mb.role.contains("stud") {
                    let a = mb.obb.centre[0] * along[0] + mb.obb.centre[1] * along[1];
                    modelled.push(a);
                    if mb.path.ends_with("/stud#1") {
                        first = Some(a);
                    }
                }
            }
            // The start: the end stud#1 is at. d = distance from it.
            let from_low = first.is_none_or(|a| a - a0 < a1 - a);
            let len = a1 - a0;
            let dist = |a: f32| if from_low { a - a0 } else { a1 - a };
            let bins = (len / 0.01).ceil() as usize;
            // The room side: the face with more scan points just beyond it.
            let (mut below, mut above) = (0u32, 0u32);
            for p in &pts {
                let (a, b) = (p[0] * along[0] + p[1] * along[1], p[0] * nrm[0] + p[1] * nrm[1]);
                if a > a0 && a < a1 {
                    below += (b < n0 - 0.05 && b > n0 - 0.4) as u32;
                    above += (b > n1 + 0.05 && b < n1 + 0.4) as u32;
                }
            }
            let room_low = below > above;
            // Per bin along the wall: how far into the wall (from its room face) the scan gets, in 1 cm steps.
            let thick = n1 - n0;
            let nd = ((thick + 0.06) / 0.01).ceil() as usize;
            let mut hist = vec![0u32; bins * nd];
            for p in &pts {
                let (a, b) = (p[0] * along[0] + p[1] * along[1], p[0] * nrm[0] + p[1] * nrm[1]);
                let depth = if room_low { b - n0 } else { n1 - b };
                if a > a0 && a < a1 && depth > -0.03 && depth < thick + 0.03 {
                    let k = ((dist(a) / 0.01) as usize).min(bins - 1);
                    hist[k * nd + (((depth + 0.03) / 0.01) as usize).min(nd - 1)] += 1;
                }
            }
            let name = g.rsplit_once('/').map_or(g.as_str(), |x| x.1);
            let (axis, sgn) = if along[0].abs() > along[1].abs() { ("x", along[0].signum()) } else { ("y", along[1].signum()) };
            let nsgn = if nrm[0].abs() > nrm[1].abs() { nrm[0].signum() } else { nrm[1].signum() };
            let start = sgn * if from_low { a0 } else { a1 };
            s += &format!("\n{name}: {len:.2} m, {:.0} mm thick, start at {axis} = {start:.3} running {}{axis}; room face at {:.3}\n", 1000.0 * thick, if from_low == (sgn > 0.0) { "+" } else { "-" }, nsgn * if room_low { n0 } else { n1 });
            // How deep the scan gets in each bin.
            let val: Vec<Option<f32>> = (0..bins).map(|k| {
                let h = &hist[k * nd..(k + 1) * nd];
                // The deepest step with a fair share of the points at or behind the room face (things in front do not count).
                let inside: u32 = h[2..].iter().sum();
                if inside < 8 {
                    return None;
                }
                let j = (2..nd).rev().find(|&j| h[j] * 8 >= inside).unwrap_or(2);
                Some(j as f32 * 0.01 - 0.025)
            }).collect();
            let mut vs: Vec<f32> = val.iter().flatten().copied().collect();
            if vs.len() < 20 {
                s += "  too few scan points in the wall's thickness\n";
                continue;
            }
            vs.sort_by(|a, b| a.total_cmp(b));
            let (lo, hi) = (vs[vs.len() * 10 / 100], vs[vs.len() * 85 / 100]);
            if hi - lo < 0.04 {
                s += &format!("  no bays seen: the scan reaches {:.0} to {:.0} mm into the wall all along it (a finish over the studs, or the wall is elsewhere)\n", 1000.0 * lo, 1000.0 * hi);
                continue;
            }
            let stud_lvl = lo;
            let is_stud = |v: f32| v - lo < 0.3 * (hi - lo);
            let mut runs: Vec<(f32, f32)> = Vec::new(); // centre, width
            let mut k = 0;
            while k < bins {
                if val[k].is_some_and(is_stud) {
                    let k0 = k;
                    while k < bins && val[k].is_some_and(is_stud) {
                        k += 1;
                    }
                    runs.push((0.005 * (k0 + k) as f32, 0.01 * (k - k0) as f32));
                } else {
                    k += 1;
                }
            }
            let narrow: Vec<f32> = runs.iter().filter(|r| r.1 >= 0.02 && r.1 <= 0.10).map(|r| r.0).collect();
            let wide: Vec<String> = runs.iter().filter(|r| r.1 > 0.10).map(|r| format!("{:.2}..{:.2}", r.0 - 0.5 * r.1, r.0 + 0.5 * r.1)).collect();
            s += &format!("  the scan reaches {:.0} mm into the wall at studs, {:.0} mm in the bays\n", 1000.0 * stud_lvl, 1000.0 * hi);
            if narrow.len() < 2 {
                s += &format!("  {} stud-like run(s): too few for a set-out\n", narrow.len());
            } else {
                let res = |x: f32, ph: f32| (x - ph + 0.5 * spacing).rem_euclid(spacing) - 0.5 * spacing;
                let best = (0..(spacing / 0.005) as usize).map(|i| i as f32 * 0.005).max_by_key(|&ph| narrow.iter().filter(|&&x| res(x, ph).abs() < 0.04).count()).unwrap();
                let on: Vec<f32> = narrow.iter().copied().filter(|&x| res(x, best).abs() < 0.04).collect();
                let ph = (best + on.iter().map(|&x| res(x, best)).sum::<f32>() / on.len() as f32).rem_euclid(spacing);
                let rms = (on.iter().map(|&x| res(x, ph).powi(2)).sum::<f32>() / on.len() as f32).sqrt();
                let now = {
                    // The modelled layout's phase, from its common studs (not the end ones).
                    let mut ds: Vec<f32> = modelled.iter().map(|&a| dist(a)).collect();
                    ds.sort_by(|a, b| a.total_cmp(b));
                    ds.get(ds.len() / 2).map(|&d| d.rem_euclid(spacing))
                };
                s += &format!(
                    "  {} stud-like runs, {} on a {:.3} m grid: studsFrom({:.3}), rms {:.0} mm{}\n",
                    narrow.len(), on.len(), spacing, ph, 1000.0 * rms,
                    now.map_or(String::new(), |n| format!("; the model's studs are at {:.3} on that grid ({:+.0} mm)", n, 1000.0 * res(n, ph))));
                s += &format!("  runs at: {}\n", narrow.iter().map(|&x| format!("{:.2}{}", x, if res(x, ph).abs() < 0.04 { "" } else { "*" })).collect::<Vec<_>>().join(" "));
            }
            if !wide.is_empty() {
                s += &format!("  solid (no bay) over: {}\n", wide.join(", "));
            }
        }
        s += "\n* = off the grid.\n";
        Ok(vec![json!({"type": "text", "text": s})])
    }
    /// Fits a plane to a surface someone pointed at, with stated support,
    /// and reports what is needed to judge it and to compare it with the
    /// model's faces.
    pub(crate) fn fit_surface(&mut self, who: &str, args: &Value) -> Result<Vec<Value>> {
        use crate::fit::{fit_plane, FitSpec};
        let model = Self::frame_is_model(args)?;
        let pl = self.placement()?;
        let to_model = |p: V3| if model { p } else { pl.inverse(p) };
        // The seed: a point, or the middle of a selection (whose points are then the support).
        let sel = args.get("selection").and_then(|v| v.as_str());
        let sel_idx = sel.map(|id| self.selection_indices(id)).transpose()?;
        let seed = match (args.get("seed").and_then(arr3), sel) {
            (Some(p), _) => to_model(p),
            (None, Some(id)) => pl.inverse(self.items.selections.iter().find(|s| s.id == id).context("selection")?.centroid),
            (None, None) => bail!("give a seed [x, y, z] or a selection"),
        };
        let radius = args.get("radius").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
        let (lo, hi) = match args.get("box") {
            Some(b) => (to_model(arr3(&b["min"]).context("box.min")?), to_model(arr3(&b["max"]).context("box.max")?)),
            None => ([seed[0] - radius, seed[1] - radius, seed[2] - radius], [seed[0] + radius, seed[1] + radius, seed[2] + radius]),
        };
        let (lo, hi): (V3, V3) = (std::array::from_fn(|a| lo[a].min(hi[a])), std::array::from_fn(|a| lo[a].max(hi[a])));
        let spec = FitSpec {
            seed,
            normal: args.get("normal").and_then(arr3).map(|n| if model { n } else { pl.unrotate(n) }),
            normal_tol: (args.get("normal_tol_deg").and_then(|v| v.as_f64()).unwrap_or(15.0) as f32).to_radians(),
            thickness: args.get("thickness").and_then(|v| v.as_f64()).unwrap_or(0.03) as f32,
            cell_normals: args.get("cell_normals").and_then(|v| v.as_bool()).unwrap_or(false),
        };
        // The candidates: the selection's points, or the used points in the box (model frame).
        self.scan()?;
        let scan = self.scan.as_ref().unwrap().clone();
        use rayon::prelude::*;
        let from_selection = sel_idx.is_some() && args.get("box").is_none();
        let cand: Vec<(u32, V3)> = match &sel_idx {
            Some(idx) if from_selection => idx.iter().map(|&i| (i, pl.inverse(scan.xyz[i as usize]))).collect(),
            _ => scan.used.par_iter().filter_map(|&i| {
                let p = pl.inverse(scan.xyz[i as usize]);
                (0..3).all(|a| p[a] >= lo[a] && p[a] <= hi[a]).then_some((i, p))
            }).collect(),
        };
        let pts: Vec<V3> = cand.iter().map(|c| c.1).collect();
        // Each point weighted by how much it is to be believed (its range; the rays; the lidar's flags), unless not.
        let weighted = args.get("weighted").and_then(|v| v.as_bool()).unwrap_or(true);
        let idx: Vec<u32> = cand.iter().map(|c| c.0).collect();
        let conf = if weighted { crate::confidence::confidence(&scan, &idx) } else { vec![] };
        let wts = weighted.then(|| crate::confidence::weights(&scan, &idx, &conf));
        let mut f = fit_plane(&pts, wts.as_deref(), &spec).with_context(|| format!("too few points near the seed {} to fit ({} candidates in the support)", f3(seed), pts.len()))?;
        let plain = if weighted { fit_plane(&pts, None, &spec) } else { None };
        // The normal toward where the surface was seen from.
        let mut seen = None;
        if let Some(sensor) = &scan.sensor {
            let side: f32 = f.inliers.iter().step_by((f.inliers.len() / 2000).max(1)).map(|&k| {
                let i = cand[k as usize].0 as usize;
                crate::dot(pl.unrotate(crate::sub(sensor[i], scan.xyz[i])), f.normal).signum()
            }).sum();
            let n_s = f.inliers.iter().step_by((f.inliers.len() / 2000).max(1)).count() as f32;
            if side < 0.0 {
                f.normal = crate::scale(f.normal, -1.0);
                f.d = -f.d;
                f.u = crate::scale(f.u, -1.0);
                f.ext = [-f.ext[1], -f.ext[0], f.ext[2], f.ext[3]];
                f.quadrants = [f.quadrants[1], f.quadrants[0], f.quadrants[3], f.quadrants[2]];
                for p in &mut f.profile {
                    p.0 = -p.0;
                }
            }
            seen = Some(side.abs() / n_s.max(1.0));
        }
        let n = f.normal;
        let mut s = String::from("Plane fit (model frame). Evidence to weigh, not a verdict: what face this is, is yours to say.\n");
        let axis = (0..3).find(|&a| n[a].abs() > 0.995);
        s += &format!("  plane: n·p = {:.4} with n = {}{}; through {}\n", f.d, f3(n), axis.map_or(String::new(), |a| format!(" (facing {}{}; at the patch's middle, {} = {:.3} m)", if n[a] > 0.0 { "+" } else { "−" }, ["x", "y", "z"][a], ["x", "y", "z"][a], f.centre[a])), f3(f.centre));
        s += &match seen {
            Some(share) => format!("  the normal points to the side it was seen from ({:.0}% of the inliers' rays agree)\n", 100.0 * share),
            None => "  no sensor rays: the normal's sign is arbitrary\n".to_string(),
        };
        s += &format!("  support: {} candidate points ({}); {} inliers within ±{:.0} mm of the plane{}{}\n", f.candidates,
            if from_selection { format!("selection {}", sel.unwrap()) } else { format!("box {} – {}", f3(lo), f3(hi)) },
            f.count, spec.thickness * 1e3, if spec.cell_normals { ", only where the points' own 5 cm neighbourhood faces the same way" } else { "" },
            if f.held { "; THE NORMAL WAS HELD at the one given (the points wanted to turn it further than the tolerance: check the support)" } else { "" });
        if weighted {
            let low = conf.iter().filter(|&&c| c < 0.5).count();
            s += &format!("  weighted by confidence: {low} of the {} candidates doubted (under 0.5: other rays pass where they are, or the lidar flags them){}\n", conf.len(),
                plain.as_ref().map_or(String::new(), |p| {
                    // (Where the unweighted plane is at this one's centre, along this one's normal.)
                    let t = (p.d - crate::dot(p.normal, f.centre)) / crate::dot(p.normal, f.normal);
                    format!("; unweighted, at the patch's middle the plane is {:+.1} mm along n from this one", t * 1e3)
                }));
        }
        s += &format!("  spread: rms {:.1} mm; the offset's standard error from scatter alone is {:.2} mm (range bias, registration and what the surface really is are not in that)\n", f.rms * 1e3, f.rms * 1e3 / (f.count as f32).sqrt());
        s += &format!("  patch: u [{:.2}, {:.2}] × v [{:.2}, {:.2}] m about that point (u = {}, v = {}); inliers in {:.0}% of its 10 cm cells\n", f.ext[0], f.ext[1], f.ext[2], f.ext[3], f3(f.u), f3(f.v), 100.0 * f.cover);
        s += &format!("  offset by quadrant (−u−v, +u−v, −u+v, +u+v), mm: {} (a spread here is a tilt, a warp, or two surfaces taken as one)\n", f.quadrants.iter().map(|q| q.map_or("-".to_string(), |x| format!("{:+.1}", x * 1e3))).collect::<Vec<_>>().join(", "));
        s += &format!("  across the plane within the patch (offset along n: share of the points there): {}\n", f.profile.iter().map(|p| format!("{:+.3} ({:.0}%)", p.0, 100.0 * p.1)).collect::<Vec<_>>().join(", "));
        // Model faces parallel to it nearby.
        let m = self.model()?;
        let mut faces: std::collections::BTreeMap<(String, String, bool), (f32, f32, u32)> = Default::default();
        for x in &m.pred.members {
            for face in &x.faces {
                let c = crate::dot(face.normal, n);
                if c.abs() < 0.985 || face.poly.is_empty() {
                    continue;
                }
                // Overlapping the patch (grown by 0.3 m)?
                let (mut a0, mut a1, mut b0, mut b1, mut off) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN, 0.0f32);
                for p in &face.poly {
                    let r = crate::sub(*p, f.centre);
                    let (a, b) = (crate::dot(r, f.u), crate::dot(r, f.v));
                    a0 = a0.min(a);
                    a1 = a1.max(a);
                    b0 = b0.min(b);
                    b1 = b1.max(b);
                    off += crate::dot(r, n) / face.poly.len() as f32;
                }
                if a1 < f.ext[0] - 0.3 || a0 > f.ext[1] + 0.3 || b1 < f.ext[2] - 0.3 || b0 > f.ext[3] + 0.3 || off.abs() > 1.0 {
                    continue;
                }
                // The member's group: the last part of its parent's path.
                let group = x.path.rsplit_once('/').map_or(x.path.clone(), |g| g.0.rsplit('/').next().unwrap_or(g.0).to_string());
                let e = faces.entry((group, x.role.clone(), c > 0.0)).or_insert((off, off, 0));
                *e = (e.0.min(off), e.1.max(off), e.2 + 1);
            }
        }
        let mut rows: Vec<_> = faces.into_iter().collect();
        rows.sort_by(|a, b| (a.1 .0 + a.1 .1).abs().total_cmp(&(b.1 .0 + b.1 .1).abs()));
        s += "  model faces parallel to it by the patch: their distance from the fitted plane along n (+ is toward the side it was seen from; \
              a range over a wall's members is the plane and the model not being quite parallel):\n";
        if rows.is_empty() {
            s += "    (none within 1 m)\n";
        }
        for ((group, role, toward), (lo, hi, k)) in rows.iter().take(16) {
            let d = if hi - lo < 0.0015 { format!("{:+.3} m", 0.5 * (lo + hi)) } else { format!("{lo:+.3} to {hi:+.3} m") };
            s += &format!("    {d}: {k} × {role} of {group:?}, the faces turned {}\n", if *toward { "toward the viewer's side" } else { "away from it" });
        }
        if rows.len() > 16 {
            s += "    …\n";
        }
        // Kept (with `keep`, or by giving it a name) as a plane: p1, p2, …
        let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("");
        if !name.is_empty() || args.get("keep").and_then(|v| v.as_bool()).unwrap_or(false) {
            let describes = args.get("describes").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let mut support = args.clone();
            support.as_object_mut().map(|o| {
                o.remove("name");
                o.remove("describes");
                o.remove("keep");
            });
            // Fitted again under the same name: the same plane, as it now is.
            let old = (!name.is_empty()).then(|| self.items.surfaces.iter().position(|x| x.name == name)).flatten();
            let id = match old {
                Some(k) => self.items.surfaces.remove(k).id,
                None => format!("p{}", self.items.surfaces.iter().filter_map(|x| x.id.strip_prefix('p')?.parse::<u32>().ok()).max().unwrap_or(0) + 1),
            };
            self.items.surfaces.push(Surface { id: id.clone(), name: name.to_string(), describes: describes.clone(), by: who.to_string(), centre: f.centre, normal: n, u: f.u, v: f.v, ext: f.ext, rms: f.rms, count: f.count, support });
            self.save_items(who)?;
            s += &format!("  kept as plane {id} {name:?}: {describes}\n");
        } else {
            s += "  (not kept: give `keep`, or `name` and `describes`, to keep it)\n";
        }
        Ok(vec![json!({"type": "text", "text": s})])
    }
}

impl Workspace {
    /// How the scan is turned against the model: a joint fit of one small rotation over all the
    /// model's surfaces (`orient`), and each part's own beside it.
    pub(crate) fn orientation(&mut self, args: &Value) -> Result<Vec<Value>> {
        use crate::orient::{fit, Patch, Turn};
        use rayon::prelude::*;
        use std::collections::{BTreeMap, HashMap};
        let pl = self.placement()?;
        let gate = args.get("gate").and_then(|v| v.as_f64()).unwrap_or(0.03) as f32;
        let filter = args.get("filter").and_then(|v| v.as_str());
        self.scan()?;
        let (scan, index, m) = (self.scan.clone().unwrap(), self.index.clone(), self.model()?);
        // Each member's part of the building: its group (the trusses together).
        let part = |path: &str| -> String {
            let g = path.rsplit_once('/').map_or(path, |x| x.0);
            let g = g.split_once('/').map_or(g, |x| x.1);
            if let Some((roof, t)) = g.rsplit_once('/') {
                if t.starts_with('T') && t[1..].chars().all(|c| c.is_ascii_digit()) {
                    return roof.to_string();
                }
            }
            g.to_string()
        };
        let parts: Vec<String> = m.pred.members.iter().map(|x| part(&x.path)).collect();
        let stride = (scan.used.len() / 4_000_000).max(1);
        // A surface: the faces of one member group that lie in one plane (to 5 mm) and face one way.
        type Key = (u32, [i16; 3], i32);
        let surfaces: HashMap<Key, Patch> = scan
            .used
            .par_iter()
            .step_by(stride)
            .fold(HashMap::new, |mut acc: HashMap<Key, Patch>, &i| {
                let p = scan.xyz[i as usize];
                let pm = pl.inverse(p);
                let Some((s, _)) = m.samples.nearest(pm, gate) else { return acc };
                let (n, q) = (m.samples.normals[s as usize], m.samples.pts[s as usize]);
                // (Only points of a surface that faces as the face does: not edges, clutter, or what crosses it.)
                if let Some(ix) = &index {
                    match ix.normal_at(p) {
                        Some(ns) if crate::dot(pl.unrotate(ns), n).abs() > 0.94 => {}
                        _ => return acc,
                    }
                }
                let key = (m.samples.member[s as usize], n.map(|x| (x * 50.0).round() as i16), (crate::dot(n, q) / 0.005).round() as i32);
                acc.entry(key).or_insert_with(|| Patch::new(n.map(f64::from))).add(pm.map(f64::from), 1.0);
                acc
            })
            .reduce(HashMap::new, |mut a, b| {
                for (k, v) in b {
                    a.entry(k).and_modify(|x| x.merge(&v)).or_insert(v);
                }
                a
            });
        // Coplanar faces of one part are one surface; then by part, and by which way they face.
        let mut merged: BTreeMap<(String, [i16; 3], i32), Patch> = BTreeMap::new();
        for ((member, n, d), p) in surfaces {
            merged.entry((parts[member as usize].clone(), n, d)).and_modify(|x| x.merge(&p)).or_insert(p);
        }
        let kind = |n: [f64; 3]| if n[2].abs() > 0.9 { "level" } else if n[2].abs() < 0.1 { "upright" } else { "sloped" };
        let mut rows: BTreeMap<(String, &str), Vec<Patch>> = BTreeMap::new();
        let (mut dropped, mut points) = (0, 0.0);
        for ((part, _, _), p) in merged {
            // (Enough points to be a surface, and flat: not two surfaces taken as one.)
            if p.w < 150.0 || p.rms() > 0.012 {
                dropped += 1;
                continue;
            }
            points += p.w;
            rows.entry((part, kind(p.n))).or_default().push(p);
        }
        if rows.is_empty() {
            bail!("no surface of the model has enough scan points near it (within {gate} m): is the model placed in the scan?");
        }
        let fitted = |k: &(String, &str)| filter.is_none_or(|f| k.0.contains(f));
        let over = |want: &dyn Fn(&(String, &str)) -> bool, but: Option<&(String, &str)>| -> Option<Turn> {
            let ps: Vec<&Patch> = rows.iter().filter(|(k, _)| fitted(k) && want(k) && Some(*k) != but).flat_map(|(_, v)| v).collect();
            fit(&ps)
        };
        let mm = |x: f64| x * 1e3;
        let mut s = format!(
            "The scan against the model's orientations: {} surfaces of {} parts, {:.0} scan points (every {stride}th, within {:.0} mm of a face of the model \
             and on a surface facing as it does); {dropped} surfaces left out (too few points, or not flat to 12 mm).\n\
             A turn is small, in mm per metre (1 mm/m = 0.057°). The model's frame: x east, y north, z up.\n\
             How to read it: a scan that is out of level by [a, b] shows as every floor rising [a, b] and every wall's top leaning [-a, -b]; \
             if the floors and the walls do not agree, it is the building, not the scan (or a surface is not what the model has there).\n\n",
            rows.values().map(Vec::len).sum::<usize>(), rows.len(), points, gate * 1e3);
        // The joint fits.
        s += "Joint fit (what the scan would be turned by to agree with the model):\n";
        let all = |_: &(String, &str)| true;
        let level = |k: &(String, &str)| k.1 == "level";
        let upright = |k: &(String, &str)| k.1 == "upright";
        let sets: [(&str, &dyn Fn(&(String, &str)) -> bool); 3] = [("all surfaces", &all), ("level surfaces only", &level), ("upright surfaces only", &upright)];
        let mut joint: Option<Turn> = None;
        for (name, want) in sets {
            let Some(t) = over(want, None) else { continue };
            // (How much it moves when any one part is left out: the most, each way.)
            let mut spread = [0.0f64; 3];
            for k in rows.keys().filter(|k| fitted(k) && want(k)) {
                if let Some(o) = over(want, Some(k)) {
                    let d = [o.level()[0] - t.level()[0], o.level()[1] - t.level()[1], o.yaw() - t.yaw()];
                    for a in 0..3 {
                        spread[a] = spread[a].max(d[a].abs());
                    }
                }
            }
            let said = |axis: usize| !t.unsaid.iter().any(|e| e[axis].abs() > 0.7);
            let part = |v: f64, sp: f64, ok: bool| if ok { format!("{:+.2} (±{:.2})", mm(v), mm(sp)) } else { "not said".to_string() };
            s += &format!(
                "  {name:<24} rises along x {}, along y {} mm/m; turned {} mm/m anticlockwise about the vertical\n",
                part(t.level()[0], spread[0], said(1)), part(t.level()[1], spread[1], said(0)), part(-t.yaw(), spread[2], said(2)));
            if joint.is_none() {
                joint = Some(t);
            }
        }
        let joint = joint.context("nothing to fit")?;
        s += "  (± is the most it changes when any one part is left out. \"Rises\" is what scan_filter level would take, added to what it has, to level it out.)\n\n";
        // Each part's own, as scanned, and what is left of it after the joint turn.
        s += &format!("{:<44} {:>8} {:>9} {:>4}  {:<34} {}\n", "part", "faces", "points", "srf", "as scanned, against the model", "left after the joint turn of all");
        for (k, ps) in &rows {
            let refs: Vec<&Patch> = ps.iter().collect();
            let Some(own) = fit(&refs) else { continue };
            let left: [f64; 3] = {
                let mut d: [f64; 3] = std::array::from_fn(|a| own.omega[a] - joint.omega[a]);
                for e in &own.unsaid {
                    let c: f64 = (0..3).map(|a| d[a] * e[a]).sum();
                    for a in 0..3 {
                        d[a] -= c * e[a];
                    }
                }
                d
            };
            let said = |axis: usize| !own.unsaid.iter().any(|e| e[axis].abs() > 0.7);
            let show = |w: [f64; 3]| match k.1 {
                "level" => format!("rises [{:+.1}, {:+.1}]", mm(w[1]), mm(-w[0])),
                "upright" => {
                    let lean = if said(0) || said(1) { format!("top leans [{:+.1}, {:+.1}]", mm(-w[1]), mm(w[0])) } else { "lean not said".into() };
                    let yaw = if said(2) { format!(", turned {:+.1}", mm(-w[2])) } else { String::new() };
                    format!("{lean}{yaw}")
                }
                _ => format!("turn [{:+.1}, {:+.1}, {:+.1}]", mm(w[0]), mm(w[1]), mm(w[2])),
            };
            let n: f64 = ps.iter().map(|p| p.w).sum();
            s += &format!("{:<44} {:>8} {:>9.0} {:>4}  {:<34} {}{}\n", k.0, k.1, n, ps.len(), show(own.omega), show(left), if fitted(k) { "" } else { "  (not fitted)" });
        }
        s += "\n(mm per metre; [x, y]: east, north. A wall's lean is said only across it: a wall running north says its lean east or west. \"Turned\" is anticlockwise from above.)\n";
        Ok(vec![json!({"type": "text", "text": s})])
    }
}
