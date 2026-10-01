//! Slabs and polylines: things people (and agents) put in the scene to look
//! and measure with, kept with the project like selections and planes.
//!
//! - A **slab** is an oriented box in the model's frame. A view clipped to
//!   one is a section: what used to be a special kind of view is a 3D view
//!   of a slab. Its third axis is the thin one, the way such a view looks.
//! - A **polyline** is a run of points in the model's frame (a footprint
//!   traced over a plan, a member's edge over a section), open or closed.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::stats::point_stats;
use crate::workspace::{arr3, f3, Workspace};
use crate::{add, cross, dot, norm, scale, sub, V3};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Slab {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub by: String,
    /// Model frame: its middle, its unit axes (right, up, toward the viewer) and half sizes along them.
    pub centre: V3,
    pub axes: [V3; 3],
    pub half: V3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Polyline {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub by: String,
    /// Model frame.
    pub points: Vec<V3>,
    #[serde(default)]
    pub closed: bool,
}

impl Polyline {
    pub fn length(&self) -> f32 {
        let n = self.points.len();
        (0..n.saturating_sub(if self.closed { 0 } else { 1 })).map(|k| norm(sub(self.points[(k + 1) % n], self.points[k]))).sum()
    }
}

fn unit(a: V3) -> V3 {
    scale(a, 1.0 / norm(a).max(1e-9))
}

/// A direction snapped to a model axis when within 5° of one (buildings are mostly square).
fn snapped(n: V3) -> V3 {
    let n = unit(n);
    match (0..3).find(|&k| n[k].abs() > 0.9962) {
        Some(k) => std::array::from_fn(|a| if a == k { n[k].signum() } else { 0.0 }),
        None => n,
    }
}

/// A slab's axes from the direction it is seen along (toward the viewer): up
/// is the model's up (its +y, for a plan), right makes them right-handed.
pub fn axes_from_normal(n: V3, up: Option<V3>) -> [V3; 3] {
    let n = snapped(n);
    let up = up.unwrap_or(if n[2].abs() > 0.9 { [0.0, 1.0, 0.0] } else { [0.0, 0.0, 1.0] });
    let v = unit(sub(up, scale(n, dot(up, n))));
    [cross(v, n), v, n]
}

fn next_id(prefix: char, ids: impl Iterator<Item = String>) -> String {
    format!("{prefix}{}", ids.filter_map(|i| i.strip_prefix(prefix)?.parse::<u32>().ok()).max().unwrap_or(0) + 1)
}

impl Workspace {
    fn slab_line(&self, s: &Slab) -> String {
        format!("{} {:?}: centre {}, {:.2} × {:.2} × {:.2} m (right × up × thickness), seen along −{}; by {}", s.id, s.name, f3(s.centre), 2.0 * s.half[0], 2.0 * s.half[1], 2.0 * s.half[2], f3(s.axes[2]), self.store.name_of(&s.by))
    }

    pub(crate) fn slabs(&mut self, who: &str, args: &Value) -> Result<Vec<Value>> {
        let text = |s: String| Ok(vec![json!({"type": "text", "text": s})]);
        let id = args.get("id").and_then(|v| v.as_str());
        let at = |ws: &Self, id: Option<&str>| -> Result<usize> {
            let id = id.context("id")?;
            ws.items.slabs.iter().position(|s| s.id == id || s.name == id).with_context(|| format!("no slab {id:?}"))
        };
        let model = Self::frame_is_model(args)?;
        match args.get("action").and_then(|v| v.as_str()).unwrap_or("list") {
            "list" => text(format!("{} slabs (model frame; a view clipped to one is a section):\n{}", self.items.slabs.len(), self.items.slabs.iter().map(|s| format!("  {}\n", self.slab_line(s))).collect::<String>())),
            "create" => {
                let thickness = args.get("thickness").and_then(|v| v.as_f64()).unwrap_or(0.2) as f32;
                let (centre, axes, half) = if let Some(p) = args.get("plane").and_then(|v| v.as_str()) {
                    // On a kept plane: its patch, with room around it.
                    let s = self.items.surfaces.iter().find(|s| s.id == p || s.name == p).with_context(|| format!("no plane {p:?}"))?;
                    let c = add(s.centre, add(scale(s.u, 0.5 * (s.ext[0] + s.ext[1])), scale(s.v, 0.5 * (s.ext[2] + s.ext[3]))));
                    (c, [s.u, s.v, s.normal], [0.5 * (s.ext[1] - s.ext[0]) + 0.3, 0.5 * (s.ext[3] - s.ext[2]) + 0.3, 0.5 * thickness])
                } else if let Some(sel) = args.get("selection").and_then(|v| v.as_str()) {
                    // Around a selection: the box of its points, turned as they lie.
                    let pl = self.placement()?;
                    let idx = self.selection_indices(sel)?;
                    let scan = self.scan()?;
                    let pts: Vec<V3> = idx.iter().map(|&i| pl.inverse(scan.xyz[i as usize])).collect();
                    let st = point_stats(pts.iter().copied());
                    if st.count < 3 {
                        bail!("the selection has too few points");
                    }
                    // Upright unless the points lie on a slope: flat things get a plan slab, walls an elevation.
                    let n0 = st.axes[2];
                    let n = if n0[2].abs() > 0.9 { [0.0, 0.0, 1.0] } else if n0[2].abs() < 0.2 { unit([n0[0], n0[1], 0.0]) } else { n0 };
                    let up = (n[2].abs() > 0.9).then(|| { let a = st.axes[0]; snapped(unit([a[0], a[1], 0.0])) });
                    let axes = axes_from_normal(n, up);
                    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
                    for p in &pts {
                        for k in 0..3 {
                            let t = dot(*p, axes[k]);
                            lo[k] = lo[k].min(t);
                            hi[k] = hi[k].max(t);
                        }
                    }
                    let c = (0..3).fold([0.0; 3], |a, k| add(a, scale(axes[k], 0.5 * (lo[k] + hi[k]))));
                    (c, axes, std::array::from_fn(|k| (0.5 * (hi[k] - lo[k]) + 0.05).max(0.05)))
                } else {
                    let c = args.get("centre").and_then(arr3).context("centre [x, y, z] with normal (the direction toward the viewer), or a plane, or a selection")?;
                    let n = args.get("normal").and_then(arr3).context("normal: the direction toward the viewer")?;
                    let (c, n, up) = if model { (c, n, args.get("up").and_then(arr3)) } else { let pl = self.placement()?; (pl.inverse(c), pl.unrotate(n), args.get("up").and_then(arr3).map(|u| pl.unrotate(u))) };
                    let size = args.get("size").and_then(arr3).unwrap_or([4.0, 3.0, thickness]);
                    (c, axes_from_normal(n, up), size.map(|x| (0.5 * x).max(0.01)))
                };
                let slab = Slab { id: next_id('b', self.items.slabs.iter().map(|s| s.id.clone())), name: args.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(), by: who.to_string(), centre, axes, half };
                let line = self.slab_line(&slab);
                self.items.slabs.push(slab);
                self.save_items(who)?;
                text(format!("made slab {line}"))
            }
            "update" => {
                let k = at(self, id)?;
                let s = &mut self.items.slabs[k];
                if let Some(c) = args.get("centre").and_then(arr3) {
                    s.centre = c;
                }
                if let Some(h) = args.get("half").and_then(arr3) {
                    s.half = h.map(|x| x.max(0.005));
                }
                if let Some(a) = args.get("axes").and_then(|v| v.as_array()).filter(|a| a.len() == 3) {
                    if let (Some(u), Some(v), Some(n)) = (arr3(&a[0]), arr3(&a[1]), arr3(&a[2])) {
                        s.axes = [unit(u), unit(v), unit(n)];
                    }
                }
                if let Some(n) = args.get("name").and_then(|v| v.as_str()) {
                    s.name = n.to_string();
                }
                let line = self.slab_line(&self.items.slabs[k]);
                self.save_items(who)?;
                text(format!("slab {line}"))
            }
            "delete" => {
                let k = at(self, id)?;
                let s = self.items.slabs.remove(k);
                self.save_items(who)?;
                text(format!("deleted {}", s.id))
            }
            a => bail!("action must be list, create, update or delete, not {a:?}"),
        }
    }

    fn line_text(&self, l: &Polyline) -> String {
        let mut s = format!("{} {:?}: {} points, {}, {:.3} m long; by {}\n", l.id, l.name, l.points.len(), if l.closed { "closed" } else { "open" }, l.length(), self.store.name_of(&l.by));
        for (k, p) in l.points.iter().enumerate() {
            let next = (k + 1 < l.points.len() || l.closed).then(|| norm(sub(l.points[(k + 1) % l.points.len()], *p)));
            s += &format!("    {k}: {}{}\n", f3(*p), next.map_or(String::new(), |d| format!("  then {d:.3} m")));
        }
        s
    }

    pub(crate) fn polylines(&mut self, who: &str, args: &Value) -> Result<Vec<Value>> {
        let text = |s: String| Ok(vec![json!({"type": "text", "text": s})]);
        let id = args.get("id").and_then(|v| v.as_str());
        let model = Self::frame_is_model(args)?;
        let points = |ws: &mut Self| -> Result<Option<Vec<V3>>> {
            let Some(a) = args.get("points").and_then(|v| v.as_array()) else { return Ok(None) };
            let pts: Vec<V3> = a.iter().map(|p| arr3(p).context("points: [[x, y, z], …]")).collect::<Result<_>>()?;
            Ok(Some(if model { pts } else { let pl = ws.placement()?; pts.into_iter().map(|p| pl.inverse(p)).collect() }))
        };
        match args.get("action").and_then(|v| v.as_str()).unwrap_or("list") {
            "list" => text(format!("{} polylines (model frame):\n{}", self.items.polylines.len(), self.items.polylines.iter().map(|l| format!("  {}", self.line_text(l))).collect::<String>())),
            "create" => {
                let pts = points(self)?.context("points: [[x, y, z], …]")?;
                if pts.len() < 2 {
                    bail!("a polyline has two points or more");
                }
                let l = Polyline { id: next_id('l', self.items.polylines.iter().map(|l| l.id.clone())), name: args.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(), by: who.to_string(), points: pts, closed: args.get("closed").and_then(|v| v.as_bool()).unwrap_or(false) };
                let line = self.line_text(&l);
                self.items.polylines.push(l);
                self.save_items(who)?;
                text(format!("made polyline {line}"))
            }
            "update" => {
                let id = id.context("id")?;
                let pts = points(self)?;
                let l = self.items.polylines.iter_mut().find(|l| l.id == id || l.name == id).with_context(|| format!("no polyline {id:?}"))?;
                if let Some(p) = pts {
                    if p.len() < 2 {
                        bail!("a polyline has two points or more");
                    }
                    l.points = p;
                }
                if let Some(c) = args.get("closed").and_then(|v| v.as_bool()) {
                    l.closed = c;
                }
                if let Some(n) = args.get("name").and_then(|v| v.as_str()) {
                    l.name = n.to_string();
                }
                let l = l.clone();
                self.save_items(who)?;
                text(format!("polyline {}", self.line_text(&l)))
            }
            "delete" => {
                let id = id.context("id")?;
                let n = self.items.polylines.len();
                self.items.polylines.retain(|l| l.id != id && l.name != id);
                if self.items.polylines.len() == n {
                    bail!("no polyline {id:?}");
                }
                self.save_items(who)?;
                text(format!("deleted {id}"))
            }
            a => bail!("action must be list, create, update or delete, not {a:?}"),
        }
    }

    /// The box (scan frame) of a slab or a polyline, for an issue's place.
    pub(crate) fn object_box(&mut self, id: &str) -> Result<(V3, V3)> {
        let pl = self.placement()?;
        let pts: Vec<V3> = if let Some(s) = self.items.slabs.iter().find(|s| s.id == id) {
            (0..8).map(|k| (0..3).fold(s.centre, |p, a| add(p, scale(s.axes[a], if k >> a & 1 == 1 { s.half[a] } else { -s.half[a] })))).collect()
        } else if let Some(l) = self.items.polylines.iter().find(|l| l.id == id) {
            l.points.clone()
        } else {
            bail!("no slab or polyline {id:?}")
        };
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for p in pts {
            let q = pl.apply(p);
            for k in 0..3 {
                lo[k] = lo[k].min(q[k] - 0.02);
                hi[k] = hi[k].max(q[k] + 0.02);
            }
        }
        Ok((lo, hi))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slab_axes_are_upright_and_snap_to_the_model() {
        // Seen from the south, 2° off square: snapped, x to the right and z up.
        let a = axes_from_normal([0.035, -0.999, 0.0], None);
        assert_eq!(a, [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]]);
        // A plan: seen from above, x to the right and y up.
        let a = axes_from_normal([0.0, 0.0, 1.0], None);
        assert_eq!(a, [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
        // 30° off an axis: kept as it is, still upright.
        let a = axes_from_normal([0.5, -0.866, 0.0], None);
        assert!((a[2][0] - 0.5).abs() < 1e-3 && a[1] == [0.0, 0.0, 1.0] && dot(a[0], a[2]).abs() < 1e-6);
    }

    #[test]
    fn a_polyline_has_a_length() {
        let mut l = Polyline { id: "l1".into(), name: String::new(), by: "p".into(), points: vec![[0.0, 0.0, 0.0], [3.0, 0.0, 0.0], [3.0, 4.0, 0.0]], closed: false };
        assert!((l.length() - 7.0).abs() < 1e-6);
        l.closed = true;
        assert!((l.length() - 12.0).abs() < 1e-6);
    }
}
