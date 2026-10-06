//! `render_view`: a 3D picture of the scan with the model over it, for an
//! agent to look at (the person's own views are never touched).

use anyhow::Result;
use serde_json::Value;

use crate::workspace::Workspace;

impl Workspace {
    #[cfg(not(feature = "pango"))]
    pub(crate) fn render_view(&mut self, _args: &Value) -> Result<Vec<Value>> {
        anyhow::bail!("this service was built without the 3D renderer (cargo feature `pango`): use render_section, which shows the same points in a slab")
    }

    #[cfg(feature = "pango")]
    pub(crate) fn render_view(&mut self, args: &Value) -> Result<Vec<Value>> {
        use crate::render::base64;
        use crate::render3d::{render, Seg, Shot};
        use crate::workspace::{arr3, f3};
        use crate::{add, evidence_colour, norm, scale, sub, V3};
        use anyhow::{bail, Context};
        use serde_json::json;

        let model = Self::frame_is_model(args)?;
        let pl = self.placement()?;
        let ev = self.evidence()?;
        let index = self.index.clone().context("the scan is still being prepared: try again in a moment")?;
        // What to look at: given, a member, an issue, or the whole model.
        let filter = args.get("member").and_then(|v| v.as_str());
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        if let Some(id) = args.get("issue").and_then(|v| v.as_u64()) {
            let p = self.store.get(id as u32)?.place.clone().with_context(|| format!("issue #{id} is not pinned to a place"))?;
            (lo, hi) = (p.min, p.max);
        } else {
            let ks = self.members_matching(filter)?;
            if ks.is_empty() {
                bail!("no member matches {:?}", filter.unwrap_or(""));
            }
            let m = self.model()?;
            for p in ks.iter().flat_map(|&k| &m.pred.members[k].faces).flat_map(|f| &f.poly) {
                let q = pl.apply(*p);
                for a in 0..3 {
                    lo[a] = lo[a].min(q[a]);
                    hi[a] = hi[a].max(q[a]);
                }
            }
        }
        let centre: V3 = std::array::from_fn(|a| 0.5 * (lo[a] + hi[a]));
        let radius = (0.5 * norm(sub(hi, lo))).max(0.5);
        let top_down = args.get("top_down").and_then(|v| v.as_bool()).unwrap_or(false);
        let ortho = top_down || args.get("projection").and_then(|v| v.as_str()).is_some_and(|p| p.starts_with("ortho"));
        let (eye, target) = match (args.get("eye").and_then(arr3), args.get("target").and_then(arr3)) {
            (Some(e), Some(t)) => (self.to_scan(e, model)?, self.to_scan(t, model)?),
            (Some(_), None) => bail!("eye needs a target"),
            (None, t) => {
                let target = match t {
                    Some(t) => self.to_scan(t, model)?,
                    None => centre,
                };
                // The direction toward the viewer (model frame): from the south-west, above, unless given.
                let from = if top_down { [0.0, 0.0, 1.0] } else { args.get("from").and_then(arr3).unwrap_or([-0.55, -0.55, 0.62]) };
                let from = pl.rotate(scale(from, 1.0 / norm(from).max(1e-6)));
                (add(target, scale(from, (radius / 0.38).max(2.0))), target)
            }
        };
        let half = args.get("half_height").and_then(|v| v.as_f64()).map_or(radius * 1.1, |x| x as f32);
        let width = args.get("max_px").and_then(|v| v.as_u64()).unwrap_or(1200).clamp(200, 2400) as u32;
        let shot = Shot { eye, target, ortho_half: ortho.then_some(half), up: top_down.then(|| pl.rotate([0.0, 1.0, 0.0])), width, height: width * 3 / 4 };

        // The points, coloured here (the renderer draws position and colour).
        let colour = args.get("color").and_then(|v| v.as_str()).unwrap_or("intensity");
        if !matches!(colour, "intensity" | "height" | "normal") {
            bail!("color must be intensity, height or normal, not {colour:?}");
        }
        let z = |k: &str, d: f32| args.get(k).and_then(|v| v.as_f64()).map_or(d, |x| x as f32 + pl.t[2]);
        let (z0, z1) = (z("clip_z_min", f32::MIN), z("clip_z", f32::MAX));
        let floor = self.scan()?.floor_z;
        let to_eye = scale(sub(eye, target), 1.0 / norm(sub(eye, target)).max(1e-6));
        let hue = |h: f32| -> [f32; 3] { [0.0, 2.0 / 3.0, 1.0 / 3.0].map(|o| (((h + o).fract() * 6.0 - 3.0).abs() - 1.0).clamp(0.0, 1.0)) };
        let cloud = &index.thin;
        let points: Vec<(V3, [f32; 3])> = (0..cloud.len())
            .filter(|&i| cloud.xyz[i][2] >= z0 && cloud.xyz[i][2] <= z1)
            .map(|i| {
                let n = cloud.normal[i].map(|x| x as f32 / 127.0);
                let surf = crate::dot(n, n) > 0.2;
                let lit = if surf { 0.55 + 0.45 * crate::dot(n, to_eye).abs() } else { 0.8 };
                let c = match colour {
                    "height" => hue(0.66 - 0.66 * ((cloud.xyz[i][2] - floor) / 3.6).clamp(0.0, 1.0)).map(|x| x * (0.35 + 0.65 * lit)),
                    "normal" if surf => n.map(|x| 0.2 + 0.8 * x.abs()),
                    "normal" => [0.42; 3],
                    _ => [(0.12 + 0.88 * (cloud.intensity[i] as f32 / 255.0).powf(0.55)) * lit; 3],
                };
                (cloud.xyz[i], c)
            })
            .collect();

        // The model's edges, by evidence; highlighted members heavy, in magenta.
        let highlight: Vec<String> = args.get("highlight").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
        let mut segs = Vec::new();
        if args.get("members").and_then(|v| v.as_bool()).unwrap_or(true) {
            for (k, m) in self.model()?.pred.members.iter().enumerate() {
                let hl = highlight.iter().any(|h| m.path.contains(h.as_str())) || filter.is_some_and(|f| m.path.contains(f));
                let c = evidence_colour(&ev[k], 0.5, 100);
                let rgba = if hl { [1.0, 0.25, 0.85, 1.0] } else { [c[0], c[1], c[2], 0.9] };
                for f in &m.faces {
                    for i in 0..f.poly.len() {
                        let (a, b) = (pl.apply(f.poly[i]), pl.apply(f.poly[(i + 1) % f.poly.len()]));
                        // The part of the edge within the height cut.
                        let at = |z: f32| if (b[2] - a[2]).abs() < 1e-9 { 0.0 } else { (z - a[2]) / (b[2] - a[2]) };
                        let (t0, t1) = (at(z0.max(-1e6)).min(at(z1.min(1e6))), at(z0.max(-1e6)).max(at(z1.min(1e6))));
                        let level = (b[2] - a[2]).abs() < 1e-9;
                        let (t0, t1) = if level { (0.0, if a[2] >= z0 && a[2] <= z1 { 1.0 } else { -1.0 }) } else { (t0.max(0.0), t1.min(1.0)) };
                        if t1 > t0 {
                            let on = |t: f32| add(a, scale(sub(b, a), t));
                            segs.push(Seg { a: on(t0), b: on(t1), rgba, heavy: hl });
                        }
                    }
                }
            }
        }
        let png = render(&points, &segs, &shot)?;
        let text = format!(
            "3D view, {}×{} px, {}: eye {} looking at {} (model frame){}. {} scan points (thinned to one per {:.1} cm), coloured by {colour}{}. \
             Member edges: green = supported, amber/red = rays pass through it, grey = unseen{}. A picture to look around with: measure in sections (render_section), which are exact.",
            shot.width, shot.height, if ortho { format!("orthographic, {:.1} m tall", 2.0 * half) } else { "perspective".into() }, f3(pl.inverse(eye)), f3(pl.inverse(target)),
            if top_down { ", straight down with the model's +y up" } else { "" }, points.len(), index.voxel * 100.0,
            if z0 > f32::MIN || z1 < f32::MAX { " (cut by height)" } else { "" },
            if highlight.is_empty() && filter.is_none() { String::new() } else { "; magenta, heavy = highlighted".into() });
        Ok(vec![json!({"type": "image", "data": base64(&png), "mimeType": "image/png"}), json!({"type": "text", "text": text})])
    }
}
