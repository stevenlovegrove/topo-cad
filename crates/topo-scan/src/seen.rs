//! Faces seen in the scan, placed by hand: each a reading that a face of a member is on a plane
//! of the scan's frame, where someone put it (dragging the face in the app, or an agent with
//! `seen`). They are kept in a file beside the model (`garage.seen.ts` for `garage.ts`), which
//! the workspace writes and the model adds (`.add(seen)`); each is applied at once (a commit),
//! and taken back by removing it.

use crate::workspace::{arr3, Evaluated, Workspace};
use crate::V3;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;

const HEADER: &str = "// Faces seen in the scan, placed by hand in the scan workspace (which writes this file): each a reading
// that a face of a member is on a plane of the scan's frame (in its coordinates), where someone put it.
// The model adds them: `import { seen } from \"./NAME.seen\"` and `.add(seen)` on the building.
import { coincide, Frame, member } from \"topo-cad\";

const scan = Frame.named(\"Scan\");

export const seen = [
";
const FOOTER: &str = "];\n";

/// One reading: its line in the file, its number, and what it says (for a list).
struct Entry {
    n: u32,
    line: String,
}

fn text(s: String) -> Vec<Value> {
    vec![json!({"type": "text", "text": s})]
}

fn num(x: f32) -> String {
    let s = format!("{:.4}", x);
    if s == "-0.0000" { "0.0000".into() } else { s }
}

fn triple(v: V3) -> String {
    format!("[{}, {}, {}]", num(v[0]), num(v[1]), num(v[2]))
}

impl Workspace {
    /// The file the readings are kept in, beside the model.
    fn seen_path(&self) -> PathBuf {
        let model = PathBuf::from(&self.model_abs);
        let stem = model.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "model".into());
        model.with_file_name(format!("{stem}.seen.ts"))
    }

    fn seen_entries(src: &str) -> Vec<Entry> {
        src.lines()
            .filter(|l| l.trim_start().starts_with("coincide("))
            .filter_map(|l| {
                let n = l.rsplit_once("// ")?.1.split(':').next()?.trim().parse().ok()?;
                Some(Entry { n, line: l.to_string() })
            })
            .collect()
    }

    fn seen_text(&self, entries: &[Entry]) -> String {
        let stem = self.seen_path().file_name().unwrap().to_string_lossy().trim_end_matches(".seen.ts").to_string();
        let mut s = HEADER.replace("NAME", &stem);
        for e in entries {
            s += &e.line;
            s += "\n";
        }
        s + FOOTER
    }

    pub(crate) fn seen(&mut self, who: &str, args: &Value) -> Result<Vec<Value>> {
        let path = self.seen_path();
        let src = std::fs::read_to_string(&path).unwrap_or_default();
        let mut entries = Self::seen_entries(&src);
        let name = self.store.name_of(who);
        match args.get("action").and_then(|v| v.as_str()).unwrap_or("list") {
            "list" => {
                if entries.is_empty() {
                    return Ok(text(format!("no faces placed by hand yet ({} does not exist or is empty)", path.display())));
                }
                let mut s = format!("{} faces placed by hand ({}):\n", entries.len(), path.display());
                let gaps = self.model().map(|m| m.pred.readings.clone()).unwrap_or_default();
                for e in &entries {
                    let plane = e.line.split_once("scan.on(").map(|(_, r)| r.split(')').next().unwrap_or("")).unwrap_or("");
                    let gap = gaps.iter().find(|(n, _)| n.contains(&plane_key(plane))).map(|(_, g)| format!(" — {:.1} mm off", g * 1000.0)).unwrap_or_default();
                    s += &format!("  {}{gap}\n", e.line.trim());
                }
                Ok(text(s))
            }
            "remove" => {
                let n = args.get("n").and_then(|v| v.as_u64()).context("n: the reading's number (see list)")? as u32;
                let k = entries.iter().position(|e| e.n == n).with_context(|| format!("no reading {n}"))?;
                let gone = entries.remove(k);
                let new = self.seen_text(&entries);
                let (before, after) = self.seen_trial(&path, &new)?;
                self.seen_commit(&path, &new, &format!("Seen: took back reading {n}"), who)?;
                let mut s = format!("took back reading {n}: {}\n", gone.line.trim());
                s += &moved(&before, &after);
                Ok(text(s))
            }
            "add" => {
                let member = args.get("member").and_then(|v| v.as_str()).context("member: its path")?;
                let face = args.get("face").and_then(arr3).context("face: the outward normal of the face, [x, y, z] in the model frame")?;
                let point = args.get("point").and_then(arr3).context("point: a point where the face is to be, [x, y, z] in the model frame")?;

                let tolerance = args.get("tolerance").and_then(|v| v.as_f64()).unwrap_or(0.003);
                let m = self.model()?;
                let pl = m.pred.scan_frame.context("the model has no frame named Scan: a face seen in the scan is a reading on a plane of it (add `Frame.named(\"Scan\").placed(...)` to the model)")?;
                let building = format!("{}/", m.pred.name);
                let rel = member.strip_prefix(&building).unwrap_or(member).to_string();
                let found = m.pred.members.iter().find(|x| x.path == member || x.path == format!("{building}{rel}")).with_context(|| format!("no member {member}"))?;
                let full = found.path.clone();
                // Where along it: as given, or where the point is along its axis (on it, a little in from its ends).
                let [s0, s1] = found.ends;
                let axis = [s1[0] - s0[0], s1[1] - s0[1], s1[2] - s0[2]];
                let length = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt() as f64;
                let along = (0..3).map(|k| (point[k] - s0[k]) * axis[k]).sum::<f32>() as f64 / length.max(1e-9);
                let at = Some(args.get("at").and_then(|v| v.as_f64()).unwrap_or(along).clamp(0.02f64.min(length / 2.0), (length - 0.02).max(length / 2.0)));
                // The face's normal, as given (unit), in the model and in the scan's frame.
                let len = (face[0] * face[0] + face[1] * face[1] + face[2] * face[2]).sqrt();
                if len < 1e-6 {
                    bail!("face: not a direction");
                }
                let n = [face[0] / len, face[1] / len, face[2] / len];
                let (p_scan, n_scan) = (pl.apply(point), pl.rotate(n));
                let next = entries.iter().map(|e| e.n).max().unwrap_or(0) + 1;
                let when = chrono_now();
                let view = args.get("view").and_then(|v| v.as_str()).map(|v| format!(", in {v}")).unwrap_or_default();
                let node = match at {
                    Some(a) => format!("member({rel:?}).node({}, {:.3})", triple(n), a),
                    None => format!("member({rel:?}).node({})", triple(n)),
                };
                let line = format!("  coincide(scan.on({}, {}), {node}, {{ tolerance: {tolerance} }}), // {next}: {name}, {when}{view}", triple(p_scan), triple(n_scan));
                entries.push(Entry { n: next, line: line.clone() });
                let new = self.seen_text(&entries);
                let (before, after) = self.seen_trial(&path, &new)?;
                // The reading must be in the model: else it does not add them.
                let key = plane_key(&format!("{}, {}", triple(p_scan), triple(n_scan)));
                let Some((_, gap)) = after.pred.readings.iter().find(|(r, _)| r.contains(&key)).cloned() else {
                    let stem = path.file_stem().unwrap().to_string_lossy().to_string();
                    bail!("the model does not add the faces seen (nothing kept): add `import {{ seen }} from \"./{stem}\";` and `.add(seen)` on the building, once");
                };
                self.seen_commit(&path, &new, &format!("Seen: {rel}, face {}", triple(n)), who)?;
                let k = after.pred.members.iter().position(|x| x.path == full);
                let k0 = before.pred.members.iter().position(|x| x.path == full);
                let mut s = format!("reading {next}: {rel}'s face toward {} is where {name} put it, to {:.0} mm.\n", triple(n), tolerance * 1000.0);
                s += &format!("the solve leaves it {:.1} mm off{}\n", gap * 1000.0, if gap.abs() > 3.0 * tolerance { " — NOT MET: nothing that is free moves it there (said as reading-off)" } else { "" });
                s += &moved(&before, &after);
                if let (Some(k), Some(k0)) = (k, k0) {
                    let (a, b) = (&before.ev[k0], &after.ev[k]);
                    s += &format!("{rel}: rays through it {:.0}% of {} → {:.0}% of {}\n", 100.0 * a.rays.pierce_frac(), a.rays.pierced + a.rays.ended, 100.0 * b.rays.pierce_frac(), b.rays.pierced + b.rays.ended);
                }
                Ok(text(s))
            }
            a => bail!("action: add, list or remove (not {a:?})"),
        }
    }

    /// The model before, and with the readings file as `new` (it must evaluate).
    fn seen_trial(&mut self, path: &PathBuf, new: &str) -> Result<(Evaluated, Evaluated)> {
        let before = self.evaluate(&HashMap::new()).context("the model as it is does not evaluate")?;
        let overlay: HashMap<String, String> = [(path.display().to_string(), new.to_string())].into();
        let after = self.evaluate(&overlay).context("the model with this reading does not evaluate (nothing kept)")?;
        Ok((before, after))
    }

    fn seen_commit(&mut self, path: &PathBuf, new: &str, message: &str, who: &str) -> Result<()> {
        std::fs::write(path, new).with_context(|| format!("writing {}", path.display()))?;
        let rel = self.repo.rel(path)?;
        self.repo.commit(&[&rel], message, &self.store.name_of(who))?;
        self.sync();
        Ok(())
    }
}

/// A plane as the readings' names have it (`Scan:plane x,y,z n nx,ny,nz`), from its two
/// triples as written in the file (`[x, y, z], [nx, ny, nz]`).
fn plane_key(s: &str) -> String {
    let nums: Vec<String> = s.split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == 'e')).filter(|t| !t.is_empty()).map(|t| t.parse::<f64>().map(|x| x.to_string()).unwrap_or_default()).collect();
    if nums.len() < 6 {
        return s.to_string();
    }
    format!("plane {},{},{} n {},{},{}", nums[0], nums[1], nums[2], nums[3], nums[4], nums[5])
}

/// What moved between two versions of the model, most first.
fn moved(before: &Evaluated, after: &Evaluated) -> String {
    let mut out: Vec<(f32, &str)> = vec![];
    for m in &after.pred.members {
        let Some(b) = before.pred.members.iter().find(|x| x.path == m.path) else { continue };
        let d = (0..3).map(|k| (m.obb.centre[k] - b.obb.centre[k]).powi(2)).sum::<f32>().sqrt();
        if d > 5e-4 {
            out.push((d, &m.path));
        }
    }
    out.sort_by(|a, b| b.0.total_cmp(&a.0));
    if out.is_empty() {
        return "nothing moved\n".into();
    }
    let mut s = format!("{} members moved:\n", out.len());
    for (d, p) in out.iter().take(10) {
        s += &format!("  {:<60} {:.1} mm\n", p, d * 1000.0);
    }
    if out.len() > 10 {
        s += &format!("  … and {} more\n", out.len() - 10);
    }
    s
}

fn chrono_now() -> String {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    // Days since 1970 to a civil date (UTC).
    let (days, secs) = ((t / 86400) as i64, t % 86400);
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02} UTC", secs / 3600, (secs % 3600) / 60)
}
