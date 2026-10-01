//! The workspace's selections: made by a person's window in a view or by a
//! box, listed, labelled, and used by the other tools (`measure`,
//! `fit_surface`, an issue's place).

use std::sync::Arc;

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::cloud::Cloud;
use crate::selection::{select, How, Selection};
use crate::workspace::{arr3, f3, Workspace};

impl Workspace {
    /// A selection's points (found again from how it was made, once).
    pub fn selection_indices(&mut self, id: &str) -> Result<Arc<Vec<u32>>> {
        if let Some(i) = self.sel_cache.get(id) {
            return Ok(i.clone());
        }
        let how = self.items.selections.iter().find(|s| s.id == id).with_context(|| format!("no selection {id}"))?.how.clone();
        let idx = Arc::new(select(self.scan()?, &how));
        self.sel_cache.insert(id.to_string(), idx.clone());
        Ok(idx)
    }

    /// A selection's points for a viewer to draw, thinned evenly to `max`.
    pub fn selection_cloud(&mut self, id: &str, max: usize) -> Result<Cloud> {
        let idx = self.selection_indices(id)?;
        let scan = self.scan()?;
        let mut c = Cloud::default();
        for &i in idx.iter().step_by(idx.len().div_ceil(max.max(1)).max(1)) {
            c.push(scan.xyz[i as usize], 255, [0; 3]);
        }
        Ok(c)
    }

    fn selection_line(&self, s: &Selection) -> String {
        let pl = self.placement.unwrap_or_default();
        let mut t = format!("{} {:?} by {}: {} points, centroid model {}", s.id, s.label, self.store.name_of(&s.by), s.count, f3(pl.inverse(s.centroid)));
        if let Some(p) = &s.plane {
            t += &format!("; plane normal {} (model), rms {:.1} mm", f3(pl.unrotate(p.normal)), p.rms * 1e3);
        }
        t
    }

    pub(crate) fn selections(&mut self, who: &str, args: &Value) -> Result<Vec<Value>> {
        let id = args.get("id").and_then(|v| v.as_str());
        let text = |s: String| Ok(vec![json!({"type": "text", "text": s})]);
        let how = match args.get("action").and_then(|v| v.as_str()).unwrap_or("list") {
            "list" => {
                let lines: Vec<String> = self.items.selections.iter().map(|s| format!("  {}\n", self.selection_line(s))).collect();
                return text(format!("{} selections (use one by id in measure, fit_surface, or an issue's place):\n{}", lines.len(), lines.concat()));
            }
            "label" => {
                let id = id.context("id")?;
                let label = args.get("label").and_then(|v| v.as_str()).context("label")?.to_string();
                self.items.selections.iter_mut().find(|s| s.id == id).with_context(|| format!("no selection {id}"))?.label = label;
                self.save_items(who)?;
                return text(format!("{id} labelled"));
            }
            "delete" => {
                let id = id.context("id")?;
                self.items.selections.retain(|s| s.id != id);
                self.sel_cache.remove(id);
                self.save_items(who)?;
                return text(format!("deleted {id}"));
            }
            // A person's window in a view (the app sends its view).
            "window" => {
                let nums = |k: &str, n: usize| -> Result<Vec<f32>> {
                    let a = args.get(k).and_then(|v| v.as_array()).filter(|a| a.len() == n).with_context(|| format!("{k}: {n} numbers"))?;
                    Ok(a.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect())
                };
                let pl = self.placement()?;
                let (vp, rect) = (nums("vp", 16)?, nums("rect", 4)?);
                let b = &args["slab"];
                let obb = match (arr3(&b["c"]), arr3(&b["u"]), arr3(&b["v"]), arr3(&b["n"]), arr3(&b["h"])) {
                    (Some(c), Some(u), Some(v), Some(n), Some(h)) => Some((c, [u, v, n], h)),
                    _ => None,
                };
                How::Window {
                    vp: std::array::from_fn(|k| vp[k]),
                    rect: std::array::from_fn(|k| rect[k]),
                    eye: arr3(&args["eye"]).context("eye")?,
                    forward: arr3(&args["forward"]).context("forward")?,
                    // Heights are given in the model's frame.
                    z: args.get("z").and_then(|v| v.as_array()).filter(|a| a.len() == 2).map(|a| [a[0].as_f64().unwrap_or(-1e9) as f32 + pl.t[2], a[1].as_f64().unwrap_or(1e9) as f32 + pl.t[2]]),
                    slab: None,
                    obb,
                    front_only: args.get("front_only").and_then(|v| v.as_bool()).unwrap_or(true),
                }
            }
            "create" => {
                let b = args.get("box").context("box: {min, max}")?;
                let (lo, hi) = (arr3(&b["min"]).context("box.min")?, arr3(&b["max"]).context("box.max")?);
                let pl = self.placement()?;
                How::Box { min: std::array::from_fn(|a| lo[a].min(hi[a])), max: std::array::from_fn(|a| lo[a].max(hi[a])), frame: Self::frame_is_model(args)?.then_some((pl.yaw, pl.t)) }
            }
            a => bail!("action must be list, create, label or delete, not {a:?}"),
        };
        let idx = select(self.scan()?, &how);
        if idx.is_empty() {
            bail!("no scan points there");
        }
        let n = self.items.selections.iter().filter_map(|s| s.id.strip_prefix('s')?.parse::<u32>().ok()).max().unwrap_or(0) + 1;
        let mut s = Selection::new(format!("s{n}"), who, how, self.scan()?, &idx);
        s.label = args.get("label").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let line = self.selection_line(&s);
        self.sel_cache.insert(s.id.clone(), Arc::new(idx));
        self.items.selections.push(s);
        self.save_items(who)?;
        text(format!("selected {line}"))
    }
}
