//! How a scan's noise goes with range, incidence and intensity, and where the
//! points smeared over occlusion edges (mixed returns) are.
//!
//!   cargo run --release -p topo-scan --example noise -- SCAN.ply [OUT.png x0 x1 y0 y1 z0 z1]
//!
//! For a sample of points: the plane of their neighbours (within 4 cm, not the
//! point itself), how far the point is off it (along its normal), the angle
//! the sensor's ray meets it at, and whether the neighbourhood is a line along
//! the ray (a smear). Tables of the spread (1.4826 × median absolute) by each.
//! With a box (in the scan's frame): a plan image of it, smears red.

use std::collections::HashMap;

use topo_scan::scan::Scan;
use topo_scan::stats::eigen_sym3;
use topo_scan::V3;

const R: f32 = 0.04;

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn norm(a: V3) -> f32 {
    dot(a, a).sqrt()
}

struct Grid {
    cells: HashMap<(i32, i32, i32), (u32, u32)>,
    order: Vec<u32>,
}

fn key(p: V3) -> (i32, i32, i32) {
    ((p[0] / R).floor() as i32, (p[1] / R).floor() as i32, (p[2] / R).floor() as i32)
}

impl Grid {
    fn new(xyz: &[V3], idx: &[u32]) -> Grid {
        let mut order = idx.to_vec();
        order.sort_unstable_by_key(|&i| key(xyz[i as usize]));
        let mut cells = HashMap::new();
        let mut s = 0;
        while s < order.len() {
            let k = key(xyz[order[s] as usize]);
            let mut e = s;
            while e < order.len() && key(xyz[order[e] as usize]) == k {
                e += 1;
            }
            cells.insert(k, (s as u32, (e - s) as u32));
            s = e;
        }
        Grid { cells, order }
    }
    fn near(&self, xyz: &[V3], p: V3, out: &mut Vec<u32>) {
        out.clear();
        let (a, b, c) = key(p);
        for i in a - 1..=a + 1 {
            for j in b - 1..=b + 1 {
                for k in c - 1..=c + 1 {
                    if let Some(&(s, n)) = self.cells.get(&(i, j, k)) {
                        for &q in &self.order[s as usize..(s + n) as usize] {
                            let d = sub(xyz[q as usize], p);
                            if dot(d, d) <= R * R {
                                out.push(q);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// What the neighbourhood of a point says of it.
#[derive(Clone, Copy)]
struct Local {
    i: u32,
    off: f32,
    range: f32,
    cos_inc: f32,
    intensity: f32,
    /// Smallest over middle eigenvalue (0: a plane).
    flat: f32,
    /// How much a line along the sensor's ray the neighbourhood is (0..1).
    smear: f32,
}

fn local(scan: &Scan, g: &Grid, i: u32, buf: &mut Vec<u32>) -> Option<Local> {
    let xyz = &scan.xyz;
    let p = xyz[i as usize];
    g.near(xyz, p, buf);
    let nb: Vec<V3> = buf.iter().filter(|&&q| q != i).map(|&q| xyz[q as usize]).collect();
    if nb.len() < 12 {
        return None;
    }
    let n = nb.len() as f64;
    let c = nb.iter().fold([0f64; 3], |a, q| [a[0] + q[0] as f64 / n, a[1] + q[1] as f64 / n, a[2] + q[2] as f64 / n]);
    let mut m = [[0f64; 3]; 3];
    for q in &nb {
        let d = [q[0] as f64 - c[0], q[1] as f64 - c[1], q[2] as f64 - c[2]];
        for a in 0..3 {
            for b in 0..3 {
                m[a][b] += d[a] * d[b] / n;
            }
        }
    }
    let (vals, vecs) = eigen_sym3(m);
    let mut ord = [0usize, 1, 2];
    ord.sort_by(|&a, &b| vals[a].total_cmp(&vals[b]));
    let col = |k: usize| -> V3 { [vecs[0][k] as f32, vecs[1][k] as f32, vecs[2][k] as f32] };
    let (normal, line) = (col(ord[0]), col(ord[2]));
    let (l0, l1, l2) = (vals[ord[0]].max(0.0), vals[ord[1]].max(1e-18), vals[ord[2]].max(1e-18));
    let s = scan.sensor.as_ref()?[i as usize];
    let ray = sub(p, s);
    let range = norm(ray);
    let d = [ray[0] / range, ray[1] / range, ray[2] / range];
    let off = dot(sub(p, [c[0] as f32, c[1] as f32, c[2] as f32]), normal);
    let linear = (1.0 - l1 / l2) as f32;
    Some(Local {
        i,
        off,
        range,
        cos_inc: dot(d, normal).abs(),
        intensity: scan.intensity.get(i as usize).copied().unwrap_or(0.0),
        flat: (l0 / l1) as f32,
        smear: dot(d, line).abs() * linear,
    })
}

fn spread(v: &mut [f32]) -> f32 {
    if v.is_empty() {
        return f32::NAN;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    let med = v[v.len() / 2];
    let mut d: Vec<f32> = v.iter().map(|x| (x - med).abs()).collect();
    d.sort_by(|a, b| a.total_cmp(b));
    1.4826 * d[d.len() / 2]
}

fn table(title: &str, rows: &[Local], edges: &[f32], by: impl Fn(&Local) -> f32, unit: &str) {
    println!("\n{title}");
    println!("  {:>14}  {:>8}  {:>9}  {:>10}  {:>8}", unit, "points", "σ⊥ mm", "σ⊥/cos mm", "smear %");
    for w in edges.windows(2) {
        let sel: Vec<&Local> = rows.iter().filter(|l| by(l) >= w[0] && by(l) < w[1]).collect();
        if sel.len() < 50 {
            continue;
        }
        let planar: Vec<&&Local> = sel.iter().filter(|l| l.flat < 0.05 && l.smear < 0.5).collect();
        let mut off: Vec<f32> = planar.iter().map(|l| l.off).collect();
        let mut along: Vec<f32> = planar.iter().map(|l| l.off / l.cos_inc.max(0.05)).collect();
        let smear = sel.iter().filter(|l| l.smear > 0.7).count() as f32 / sel.len() as f32;
        println!("  {:>6.2}–{:<6.2}  {:>8}  {:>9.2}  {:>10.2}  {:>8.1}", w[0], w[1], planar.len(), 1000.0 * spread(&mut off), 1000.0 * spread(&mut along), 100.0 * smear);
    }
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let scan = Scan::load(&args[0], 0.8, [0.0, 0.0])?;
    eprintln!("{} points, {} used; sensor positions: {}", scan.xyz.len(), scan.used.len(), scan.sensor.is_some());
    let g = Grid::new(&scan.xyz, &scan.used);
    use rayon::prelude::*;
    let step = (scan.used.len() / 400_000).max(1);
    let sample: Vec<u32> = scan.used.iter().step_by(step).copied().collect();
    let rows: Vec<Local> = sample.par_chunks(4096).flat_map_iter(|ch| {
        let mut buf = Vec::new();
        ch.iter().filter_map(|&i| local(&scan, &g, i, &mut buf)).collect::<Vec<_>>()
    }).collect();
    println!("{} sampled points with 12 or more neighbours within {:.0} cm", rows.len(), R * 100.0);
    println!("σ⊥: spread of a point off its neighbours' plane (planar ones: smallest/middle eigenvalue < 0.05, not a smear);");
    println!("σ⊥/cos: the same along the ray (if the noise is in range alone, this is flat in incidence). smear %: neighbourhood a line along the ray.");
    if !scan.tag.is_empty() {
        let mut count: HashMap<u8, usize> = HashMap::new();
        for &t in &scan.tag {
            *count.entry(t).or_default() += 1;
        }
        let total = scan.tag.len() as f64;
        let field = |shift: u32| -> String {
            (0..4u8).map(|v| format!("{v}: {:.3}%", 100.0 * count.iter().filter(|(t, _)| (*t >> shift) & 3 == v).map(|(_, n)| *n).sum::<usize>() as f64 / total)).collect::<Vec<_>>().join(", ")
        };
        println!("\nTags over the whole scan: bits 0–1 (noise by position) {}\n  bits 2–3 (noise by intensity) {}\n  bits 4–5 (return) {}\n  bits 6–7 {}", field(0), field(2), field(4), field(6));
        let flagged: Vec<Local> = rows.iter().filter(|l| scan.tag[l.i as usize] & 3 != 0).copied().collect();
        println!("  sampled points flagged noise by position: {} of {}", flagged.len(), rows.len());
        table("Flagged noise by position (bits 0–1 not 0), by range", &flagged, &[0.8, 1.5, 3.0, 6.0, 12.0], |l| l.range, "range m");
    }
    table("By range", &rows, &[0.8, 1.0, 1.25, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0, 8.0, 12.0], |l| l.range, "range m");
    table("By incidence (angle off the surface normal)", &rows, &[0.0, 15.0, 30.0, 45.0, 60.0, 70.0, 80.0, 90.0], |l| l.cos_inc.clamp(0.0, 1.0).acos().to_degrees(), "deg");
    table("By intensity", &rows, &[0.0, 0.02, 0.05, 0.1, 0.15, 0.2, 0.3, 0.5, 1.01], |l| l.intensity, "intensity");
    for (lo, hi) in [(0.8, 2.0), (2.0, 4.0), (4.0, 12.0)] {
        let sub: Vec<Local> = rows.iter().filter(|l| l.range >= lo && l.range < hi).copied().collect();
        table(&format!("By incidence, range {lo}–{hi} m"), &sub, &[0.0, 30.0, 50.0, 65.0, 75.0, 90.0], |l| l.cos_inc.clamp(0.0, 1.0).acos().to_degrees(), "deg");
    }

    // A plan image of a box (scan frame): each point by its smear score (grey to red).
    if args.len() >= 8 {
        let b: Vec<f32> = args[2..8].iter().map(|s| s.parse().unwrap()).collect();
        let px = 2000.0; // pixels per metre
        let (w, h) = (((b[1] - b[0]) * px) as usize, ((b[3] - b[2]) * px) as usize);
        let inside: Vec<u32> = scan.used.iter().copied().filter(|&i| {
            let p = scan.xyz[i as usize];
            p[0] >= b[0] && p[0] < b[1] && p[1] >= b[2] && p[1] < b[3] && p[2] >= b[4] && p[2] < b[5]
        }).collect();
        // Rays through the box (plan cells of `cell`): how many pass each cell and end more than `margin`
        // beyond it (they say it is empty), and how many end in it.
        let cell = 0.004f32;
        let margin = 0.012f32;
        let (cw, ch) = (((b[1] - b[0]) / cell).ceil() as usize, ((b[3] - b[2]) / cell).ceil() as usize);
        let sensor = scan.sensor.as_ref().unwrap();
        let cell_of = |p: V3| -> Option<usize> {
            (p[0] >= b[0] && p[0] < b[1] && p[1] >= b[2] && p[1] < b[3] && p[2] >= b[4] && p[2] < b[5]).then(|| ((p[0] - b[0]) / cell) as usize + cw * ((p[1] - b[2]) / cell) as usize)
        };
        let (pass, ends) = scan.used.par_iter().fold(|| (vec![0u32; cw * ch], vec![0u32; cw * ch]), |(mut pass, mut ends), &i| {
            let (s, p) = (sensor[i as usize], scan.xyz[i as usize]);
            if let Some(c) = cell_of(p) {
                ends[c] += 1;
            }
            let d = sub(p, s);
            let len = norm(d);
            // Only rays that come near the box.
            let mid = [0.5 * (b[0] + b[1]), 0.5 * (b[2] + b[3]), 0.5 * (b[4] + b[5])];
            let reach = 0.5 * norm([b[1] - b[0], b[3] - b[2], b[5] - b[4]]);
            let t = (dot(sub(mid, s), d) / len).clamp(0.0, len);
            let q = [s[0] + d[0] * t / len, s[1] + d[1] * t / len, s[2] + d[2] * t / len];
            if norm(sub(q, mid)) > reach {
                return (pass, ends);
            }
            let n = ((len - margin) / (cell * 0.5)).floor().max(0.0) as usize;
            let mut last = usize::MAX;
            for k in 0..n {
                let f = k as f32 * cell * 0.5 / len;
                if let Some(c) = cell_of([s[0] + d[0] * f, s[1] + d[1] * f, s[2] + d[2] * f]) {
                    if c != last {
                        pass[c] += 1;
                        last = c;
                    }
                }
            }
            (pass, ends)
        }).reduce(|| (vec![0u32; cw * ch], vec![0u32; cw * ch]), |(mut a, mut b), (x, y)| {
            for k in 0..a.len() {
                a[k] += x[k];
                b[k] += y[k];
            }
            (a, b)
        });
        let mut img = vec![255u8; w * h * 3];
        let mut hist = [0usize; 10];
        for &i in &inside {
            let p = scan.xyz[i as usize];
            let Some(c) = cell_of(p) else { continue };
            let score = pass[c] as f32 / (pass[c] + ends[c]).max(1) as f32;
            hist[((score * 10.0) as usize).min(9)] += 1;
            let (x, y) = (((p[0] - b[0]) * px) as usize, h - 1 - ((p[1] - b[2]) * px) as usize);
            let s = ((score - 0.5) / 0.4).clamp(0.0, 1.0);
            let k = (y * w + x) * 3;
            img[k..k + 3].copy_from_slice(&[(60.0 + 195.0 * s) as u8, (60.0 * (1.0 - s)) as u8, (60.0 * (1.0 - s)) as u8]);
        }
        eprintln!("points by the share of rays through their cell (0.0–0.1, …, 0.9–1.0): {hist:?}");
        // As the workspace weighs them (`confidence`): red doubted, grey believed.
        {
            let conf = topo_scan::confidence::confidence(&scan, &inside);
            let mut img = vec![255u8; w * h * 3];
            for (&i, &c) in inside.iter().zip(&conf) {
                let p = scan.xyz[i as usize];
                let (x, y) = (((p[0] - b[0]) * px) as usize, h - 1 - ((p[1] - b[2]) * px) as usize);
                let s = (1.0 - c).clamp(0.0, 1.0);
                let k = (y * w + x) * 3;
                img[k..k + 3].copy_from_slice(&[(60.0 + 195.0 * s) as u8, (60.0 * (1.0 - s)) as u8, (60.0 * (1.0 - s)) as u8]);
            }
            let out = args[1].replace(".png", "-confidence.png");
            let f = std::fs::File::create(&out)?;
            let mut e = png::Encoder::new(std::io::BufWriter::new(f), w as u32, h as u32);
            e.set_color(png::ColorType::Rgb);
            e.write_header()?.write_image_data(&img)?;
            eprintln!("{} of {} doubted (under 0.5); wrote {out}", conf.iter().filter(|&&c| c < 0.5).count(), conf.len());
        }
        // The lidar's own flags, against that: and a picture of them (bits 0–1, noise by position).
        if !scan.tag.is_empty() {
            let mut img = vec![255u8; w * h * 3];
            let mut by: HashMap<u8, (usize, f64, usize)> = HashMap::new();
            for &i in &inside {
                let p = scan.xyz[i as usize];
                let t = scan.tag[i as usize];
                if let Some(c) = cell_of(p) {
                    let score = pass[c] as f64 / (pass[c] + ends[c]).max(1) as f64;
                    let e = by.entry(t & 3).or_default();
                    e.0 += 1;
                    e.1 += score;
                    e.2 += (score > 0.5) as usize;
                }
                let (x, y) = (((p[0] - b[0]) * px) as usize, h - 1 - ((p[1] - b[2]) * px) as usize);
                let k = (y * w + x) * 3;
                img[k..k + 3].copy_from_slice(&[[80, 80, 80], [230, 30, 30], [240, 140, 0], [40, 90, 230]][(t & 3) as usize]);
            }
            let mut keys: Vec<_> = by.keys().copied().collect();
            keys.sort();
            println!("\nIn the box, by the position-noise bits (0 normal, 1 high, 2 moderate, 3 low confidence of noise):");
            for k in keys {
                let (n, sum, over) = by[&k];
                println!("  {k}: {n:>7} points; mean share of rays through their cell {:.2}; {:.0}% over half", sum / n as f64, 100.0 * over as f64 / n as f64);
            }
            let out = args[1].replace(".png", "-tag.png");
            let f = std::fs::File::create(&out)?;
            let mut e = png::Encoder::new(std::io::BufWriter::new(f), w as u32, h as u32);
            e.set_color(png::ColorType::Rgb);
            e.write_header()?.write_image_data(&img)?;
            eprintln!("wrote {out}");
        }
        let f = std::fs::File::create(&args[1])?;
        let mut e = png::Encoder::new(std::io::BufWriter::new(f), w as u32, h as u32);
        e.set_color(png::ColorType::Rgb);
        e.write_header()?.write_image_data(&img)?;
        eprintln!("{} points in the box; wrote {}", inside.len(), args[1]);
    }
    Ok(())
}
