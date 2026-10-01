//! Section views as images: the scan points in a slab about a plane, drawn
//! as density in the plane's (u, v) axes, with the model cut by the plane
//! (thick) and its edges within the slab (thin), annotations and marks, on a
//! metric grid. Made for an LLM to look at, so the grid is labelled in
//! metres and the pixel ↔ metre mapping is exact and reported.

use rayon::prelude::*;

use crate::{cross, dot, norm, scale, sub, V3};

/// A plane and the slab about it: points p with |n·(p − origin)| ≤ half
/// are drawn at (u·(p − origin), v·(p − origin)); v is up in the image.
#[derive(Debug, Clone, Copy)]
pub struct Section {
    pub origin: V3,
    pub n: V3,
    pub u: V3,
    pub v: V3,
    pub half: f32,
}

impl Section {
    /// A section through `origin` with normal `n`, `up` giving the image's up
    /// (projected into the plane); u (the image's right) makes (u, v, n)
    /// right-handed, so the view looks along −n.
    pub fn new(origin: V3, n: V3, up: V3, thickness: f32) -> Section {
        let n = scale(n, 1.0 / norm(n));
        let mut v = sub(up, scale(n, dot(up, n)));
        if norm(v) < 1e-6 {
            // up along the normal: any perpendicular.
            v = if n[0].abs() < 0.9 { cross(n, [1.0, 0.0, 0.0]) } else { cross(n, [0.0, 1.0, 0.0]) };
        }
        let v = scale(v, 1.0 / norm(v));
        let u = cross(v, n);
        Section { origin, n, u, v, half: 0.5 * thickness }
    }
    /// (across the slab, u, v).
    pub fn project(&self, p: V3) -> (f32, f32, f32) {
        let q = sub(p, self.origin);
        (dot(q, self.n), dot(q, self.u), dot(q, self.v))
    }
}

/// Where a plane cuts a solid's faces: for each face, the segment between
/// the points where its outline crosses the plane.
pub fn cut_segments(faces: &[Vec<V3>], origin: V3, n: V3) -> Vec<(V3, V3)> {
    let mut segs = Vec::new();
    for f in faces {
        let mut xs = Vec::new();
        for k in 0..f.len() {
            let (p, q) = (f[k], f[(k + 1) % f.len()]);
            let (a, b) = (dot(sub(p, origin), n), dot(sub(q, origin), n));
            if (a < 0.0) != (b < 0.0) {
                let t = a / (a - b);
                xs.push([p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1]), p[2] + t * (q[2] - p[2])]);
            }
        }
        for c in xs.chunks_exact(2) {
            segs.push((c[0], c[1]));
        }
    }
    segs
}

pub struct Canvas {
    pub w: usize,
    pub h: usize,
    pub rgb: Vec<u8>,
}

impl Canvas {
    pub fn new(w: usize, h: usize, bg: [u8; 3]) -> Canvas {
        Canvas { w, h, rgb: bg.iter().copied().cycle().take(w * h * 3).collect() }
    }
    pub fn set(&mut self, x: i32, y: i32, c: [u8; 3]) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            let k = 3 * (y as usize * self.w + x as usize);
            self.rgb[k..k + 3].copy_from_slice(&c);
        }
    }
    pub fn dot(&mut self, x: i32, y: i32, r: i32, c: [u8; 3]) {
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy <= r * r + r {
                    self.set(x + dx, y + dy, c);
                }
            }
        }
    }
    pub fn line(&mut self, a: (f32, f32), b: (f32, f32), width: i32, c: [u8; 3]) {
        let n = ((b.0 - a.0).abs().max((b.1 - a.1).abs()).ceil() as usize).clamp(1, 20_000);
        for k in 0..=n {
            let t = k as f32 / n as f32;
            let (x, y) = (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1));
            if width <= 1 {
                self.set(x.round() as i32, y.round() as i32, c);
            } else {
                self.dot(x.round() as i32, y.round() as i32, width / 2, c);
            }
        }
    }
    /// Digits, '.', '-' and ' ' in a 3×5 font, `s` pixels per font pixel.
    pub fn text(&mut self, x: i32, y: i32, s: i32, text: &str, c: [u8; 3]) {
        for (k, ch) in text.chars().enumerate() {
            let rows: [u8; 5] = match ch {
                '0' => [7, 5, 5, 5, 7],
                '1' => [2, 6, 2, 2, 7],
                '2' => [7, 1, 7, 4, 7],
                '3' => [7, 1, 7, 1, 7],
                '4' => [5, 5, 7, 1, 1],
                '5' => [7, 4, 7, 1, 7],
                '6' => [7, 4, 7, 5, 7],
                '7' => [7, 1, 1, 1, 1],
                '8' => [7, 5, 7, 5, 7],
                '9' => [7, 5, 7, 1, 7],
                '-' => [0, 0, 7, 0, 0],
                '.' => [0, 0, 0, 0, 2],
                _ => [0; 5],
            };
            for (r, bits) in rows.iter().enumerate() {
                for col in 0..3 {
                    if bits >> (2 - col) & 1 == 1 {
                        for dy in 0..s {
                            for dx in 0..s {
                                self.set(x + k as i32 * 4 * s + col * s + dx, y + r as i32 * s + dy, c);
                            }
                        }
                    }
                }
            }
        }
    }
    pub fn png(&self) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut e = png::Encoder::new(&mut out, self.w as u32, self.h as u32);
            e.set_color(png::ColorType::Rgb);
            e.set_depth(png::BitDepth::Eight);
            let mut w = e.write_header().expect("png header");
            w.write_image_data(&self.rgb).expect("png data");
        }
        out
    }
}

pub fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let x = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for k in 0..4 {
            if k <= c.len() {
                s.push(T[(x >> (18 - 6 * k) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

/// A member to draw: its faces (in the section's frame), colour, emphasis.
pub struct MemberLayer {
    pub faces: Vec<Vec<V3>>,
    pub colour: [u8; 3],
    pub highlight: bool,
}

/// A set of points to mark (in the section's frame).
pub struct PointLayer {
    pub pts: Vec<V3>,
    pub colour: [u8; 3],
}

/// How a section's points are coloured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PointColour {
    /// Dark where dense.
    #[default]
    Density,
    /// By the offset from the cut plane of the point nearest the viewer in
    /// each pixel: blue at the back of the slab, through green, to red at
    /// its front. What stands in front of what.
    Depth,
    /// By the local surface normal (of 4 cm cells): grey where the surface
    /// faces the viewer (lighter the more squarely), a hue where it is seen
    /// edge-on, by the direction it faces in the image (red: left–right,
    /// cyan: up–down). Faces against edges: sheathing against the sides of
    /// members.
    Normal,
    /// By the lidar's return intensity (dark low, yellow high), stretched to
    /// the slab's range: materials and finishes.
    Intensity,
}

/// A hue (0–1 round the wheel) at full saturation, as RGB 0–255.
fn hue(h: f32, value: f32) -> [f32; 3] {
    let h6 = (h.rem_euclid(1.0)) * 6.0;
    let x = 1.0 - (h6 % 2.0 - 1.0).abs();
    let (r, g, b) = match h6 as u32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    [255.0 * value * r, 255.0 * value * g, 255.0 * value * b]
}

pub struct SectionRequest<'a> {
    pub section: Section,
    /// Scan points (and which to use), and the map into the section's frame.
    pub xyz: &'a [V3],
    pub used: &'a [u32],
    pub to_frame: &'a (dyn Fn(V3) -> V3 + Sync),
    /// (u0, u1, v0, v1) in metres, or fitted to the points.
    pub bounds: Option<[f32; 4]>,
    /// Image width (px) at most; height follows the bounds, at most `max_px`.
    pub max_px: usize,
    pub colour: PointColour,
    /// The scan's intensities (0–1, by point index), for `PointColour::Intensity`.
    pub intensity: Option<&'a [f32]>,
    pub members: Vec<MemberLayer>,
    pub layers: Vec<PointLayer>,
    /// Crosses at these (section-frame) positions.
    pub marks: Vec<V3>,
    /// A viewer's eye and view direction.
    pub eye: Option<(V3, V3)>,
}

pub struct SectionImage {
    pub canvas: Canvas,
    pub bounds: [f32; 4],
    /// Pixels per metre, and the left and top margins (px): a point at
    /// (u, v) is at pixel (left + (u − u0)·scale, top + (v1 − v)·scale).
    pub scale: f32,
    pub left: f32,
    pub top: f32,
    pub points: usize,
    /// Per member drawn: the extent of its cut by the plane (u0, u1, v0, v1),
    /// when the plane cuts it.
    pub cuts: Vec<Option<[f32; 4]>>,
    pub grid_step: f32,
}

pub fn render_section(r: &SectionRequest) -> SectionImage {
    let sec = r.section;
    let pts: Vec<(f32, f32, f32, u32)> = r
        .used
        .par_iter()
        .filter_map(|&i| {
            let (s, a, b) = sec.project((r.to_frame)(r.xyz[i as usize]));
            (s.abs() <= sec.half).then_some((s, a, b, i))
        })
        .collect();

    // The model cut: per face, the segment where the plane crosses it.
    let cuts: Vec<Vec<((f32, f32), (f32, f32))>> = r
        .members
        .iter()
        .map(|m| {
            let mut segs = Vec::new();
            for f in &m.faces {
                let ps: Vec<(f32, f32, f32)> = f.iter().map(|&p| sec.project(p)).collect();
                let mut xs = Vec::new();
                for k in 0..ps.len() {
                    let (p, q) = (ps[k], ps[(k + 1) % ps.len()]);
                    if (p.0 < 0.0) != (q.0 < 0.0) {
                        let t = p.0 / (p.0 - q.0);
                        xs.push((p.1 + t * (q.1 - p.1), p.2 + t * (q.2 - p.2)));
                    }
                }
                for c in xs.chunks_exact(2) {
                    segs.push((c[0], c[1]));
                }
            }
            segs
        })
        .collect();
    let cut_ext: Vec<Option<[f32; 4]>> = cuts
        .iter()
        .map(|segs| {
            (!segs.is_empty()).then(|| {
                segs.iter().flat_map(|s| [s.0, s.1]).fold([f32::MAX, f32::MIN, f32::MAX, f32::MIN], |e, p| [e[0].min(p.0), e[1].max(p.0), e[2].min(p.1), e[3].max(p.1)])
            })
        })
        .collect();

    let bounds = r.bounds.unwrap_or_else(|| {
        let pct = |mut v: Vec<f32>, f: f32| -> Option<f32> {
            if v.is_empty() {
                return None;
            }
            let k = ((v.len() - 1) as f32 * f) as usize;
            Some(*v.select_nth_unstable_by(k, |a, b| a.total_cmp(b)).1)
        };
        let us: Vec<f32> = pts.iter().step_by((pts.len() / 200_000).max(1)).map(|p| p.1).collect();
        let vs: Vec<f32> = pts.iter().step_by((pts.len() / 200_000).max(1)).map(|p| p.2).collect();
        let mut b = [pct(us.clone(), 0.002), pct(us, 0.998), pct(vs.clone(), 0.002), pct(vs, 0.998)].map(|x| x.unwrap_or(f32::NAN));
        for e in cut_ext.iter().flatten() {
            b = [b[0].min(e[0]), b[1].max(e[1]), b[2].min(e[2]), b[3].max(e[3])];
        }
        if b.iter().any(|x| !x.is_finite()) {
            b = [-1.0, 1.0, -1.0, 1.0];
        }
        let pad = 0.03 * (b[1] - b[0]).max(b[3] - b[2]).max(0.2);
        [b[0] - pad, b[1] + pad, b[2] - pad, b[3] + pad]
    });
    let (du, dv) = ((bounds[1] - bounds[0]).max(1e-3), (bounds[3] - bounds[2]).max(1e-3));
    let (left, right, top, bottom) = (44.0f32, 12.0f32, 12.0f32, 28.0f32);
    let max_px = r.max_px.max(200) as f32;
    let scale = ((max_px - left - right) / du).min((max_px - top - bottom) / dv);
    let (w, h) = ((left + right + du * scale).ceil() as usize, (top + bottom + dv * scale).ceil() as usize);
    let to_px = |a: f32, b: f32| (left + (a - bounds[0]) * scale, top + (bounds[3] - b) * scale);
    let mut c = Canvas::new(w, h, [252, 252, 250]);

    // Grid: a step at least ~60 px apart.
    let grid_step = [0.05f32, 0.1, 0.2, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0].into_iter().find(|s| s * scale >= 60.0).unwrap_or(50.0);
    let decimals = if grid_step < 0.1 { 2 } else if grid_step < 1.0 { 1 } else { 0 };
    let mut g = (bounds[0] / grid_step).ceil() * grid_step;
    while g <= bounds[1] {
        let (x, _) = to_px(g, 0.0);
        c.line((x, top), (x, h as f32 - bottom), 1, if g.abs() < 1e-4 { [190, 190, 200] } else { [228, 228, 232] });
        let s = format!("{g:.decimals$}");
        c.text(x as i32 - 4 * s.len() as i32, h as i32 - bottom as i32 + 8, 2, &s, [90, 90, 100]);
        g += grid_step;
    }
    let mut g = (bounds[2] / grid_step).ceil() * grid_step;
    while g <= bounds[3] {
        let (_, y) = to_px(0.0, g);
        c.line((left, y), (w as f32 - right, y), 1, if g.abs() < 1e-4 { [190, 190, 200] } else { [228, 228, 232] });
        let s = format!("{g:.decimals$}");
        c.text(left as i32 - 4 - 8 * s.len() as i32, y as i32 - 5, 2, &s, [90, 90, 100]);
        g += grid_step;
    }

    // Points: per pixel, how many, and the one nearest the viewer (greatest s).
    let mut count = vec![0u32; w * h];
    let mut front: Vec<(f32, u32)> = vec![(f32::MIN, 0); w * h];
    for (j, &(s, a, b, _)) in pts.iter().enumerate() {
        let (x, y) = to_px(a, b);
        let (x, y) = (x as i32, y as i32);
        if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
            let k = y as usize * w + x as usize;
            count[k] += 1;
            if s > front[k].0 {
                front[k] = (s, j as u32);
            }
        }
    }
    let mut nz: Vec<u32> = count.iter().copied().filter(|&n| n > 0).collect();
    let top_count = if nz.is_empty() { 1 } else {
        let k = (nz.len() - 1) * 98 / 100;
        *nz.select_nth_unstable(k).1
    }.max(1);
    let lt = (1.0 + top_count as f32).ln();
    // Local normals (section frame: across, u, v) in 4 cm cells, for Normal.
    let cell = |p: &(f32, f32, f32, u32)| ((p.0 / 0.04).floor() as i32, (p.1 / 0.04).floor() as i32, (p.2 / 0.04).floor() as i32);
    let normals: std::collections::HashMap<(i32, i32, i32), V3> = if r.colour == PointColour::Normal {
        let mut groups: std::collections::HashMap<(i32, i32, i32), Vec<V3>> = Default::default();
        for p in &pts {
            groups.entry(cell(p)).or_default().push([p.0, p.1, p.2]);
        }
        groups.into_iter().filter_map(|(k, v)| {
            let st = crate::stats::point_stats(v.into_iter());
            (st.count >= 6 && st.sigma[2] < 0.012 && st.sigma[1] > 0.004).then(|| (k, st.plane.unwrap().normal))
        }).collect()
    } else {
        Default::default()
    };
    // The intensities' range in the slab (2nd–98th percentiles).
    let (i0, i1) = match (r.colour, r.intensity) {
        (PointColour::Intensity, Some(int)) if !pts.is_empty() => {
            let mut v: Vec<f32> = pts.iter().step_by((pts.len() / 100_000).max(1)).map(|p| int[p.3 as usize]).collect();
            let n = v.len() - 1;
            let lo = *v.select_nth_unstable_by(n * 2 / 100, |a, b| a.total_cmp(b)).1;
            let hi = *v.select_nth_unstable_by(n * 98 / 100, |a, b| a.total_cmp(b)).1;
            (lo, (hi - lo).max(1e-6))
        }
        _ => (0.0, 1.0),
    };
    for k in 0..w * h {
        if count[k] == 0 {
            continue;
        }
        let f = ((1.0 + count[k] as f32).ln() / lt).clamp(0.0, 1.0);
        let p = &pts[front[k].1 as usize];
        let col: [f32; 3] = match r.colour {
            PointColour::Density => {
                let g = 252.0 - (0.35 + 0.65 * f) * 222.0;
                [g, g, g]
            }
            PointColour::Depth => {
                // Back of the slab blue (hue 2/3), through green, to its front red (hue 0).
                let t = (p.0 / sec.half.max(1e-6) * 0.5 + 0.5).clamp(0.0, 1.0);
                hue(2.0 / 3.0 * (1.0 - t), 0.75 + 0.2 * f)
            }
            PointColour::Normal => match normals.get(&cell(p)) {
                Some(n) => {
                    let facing = n[0].abs();
                    if facing > 0.7 {
                        let g = 120.0 + 110.0 * ((facing - 0.7) / 0.3);
                        [g, g, g + 12.0]
                    } else {
                        // Edge-on: the hue by the direction it faces in the image (mod 180°).
                        hue(n[2].atan2(n[1]).rem_euclid(std::f32::consts::PI) / std::f32::consts::PI, 0.85)
                    }
                }
                // No surface there (clutter, an edge, too few points): pale.
                None => [215.0, 205.0, 190.0],
            },
            PointColour::Intensity => {
                let t = r.intensity.map_or(0.5, |int| ((int[p.3 as usize] - i0) / i1).clamp(0.0, 1.0));
                // Dark blue, through magenta, to yellow.
                [30.0 + 225.0 * t, 20.0 + 215.0 * t * t, 110.0 + 60.0 * (1.0 - t) - 150.0 * t * t.max(0.0)].map(|x: f32| x.clamp(0.0, 255.0))
            }
        };
        let (x, y) = ((k % w) as i32, (k / w) as i32);
        c.set(x, y, col.map(|x| x.clamp(0.0, 255.0) as u8));
    }

    for layer in &r.layers {
        for &p in &layer.pts {
            let (s, a, b) = sec.project(p);
            if s.abs() <= sec.half {
                let (x, y) = to_px(a, b);
                c.set(x as i32, y as i32, layer.colour);
            }
        }
    }

    // Members: edges within the slab (thin, lightened), then the cut.
    for m in &r.members {
        let light = m.colour.map(|x| ((x as u16 + 2 * 235) / 3) as u8);
        let light = if m.highlight { m.colour } else { light };
        for f in &m.faces {
            for k in 0..f.len() {
                let (p, q) = (sec.project(f[k]), sec.project(f[(k + 1) % f.len()]));
                // Clip the edge to |s| ≤ half.
                let (mut t0, mut t1) = (0.0f32, 1.0f32);
                let ds = q.0 - p.0;
                if ds.abs() < 1e-9 {
                    if p.0.abs() > sec.half {
                        continue;
                    }
                } else {
                    let (ta, tb) = ((-sec.half - p.0) / ds, (sec.half - p.0) / ds);
                    t0 = t0.max(ta.min(tb));
                    t1 = t1.min(ta.max(tb));
                    if t0 > t1 {
                        continue;
                    }
                }
                let at = |t: f32| to_px(p.1 + t * (q.1 - p.1), p.2 + t * (q.2 - p.2));
                c.line(at(t0), at(t1), if m.highlight { 2 } else { 1 }, light);
            }
        }
    }
    for (m, segs) in r.members.iter().zip(&cuts) {
        for s in segs {
            c.line(to_px(s.0 .0, s.0 .1), to_px(s.1 .0, s.1 .1), if m.highlight { 5 } else { 3 }, if m.highlight { [220, 0, 200] } else { m.colour });
        }
    }

    for &p in &r.marks {
        let (_, a, b) = sec.project(p);
        let (x, y) = to_px(a, b);
        c.line((x - 8.0, y - 8.0), (x + 8.0, y + 8.0), 3, [230, 30, 30]);
        c.line((x - 8.0, y + 8.0), (x + 8.0, y - 8.0), 3, [230, 30, 30]);
    }
    if let Some((eye, fwd)) = r.eye {
        let (_, a, b) = sec.project(eye);
        let (x, y) = to_px(a, b);
        let (fa, fb) = (dot(fwd, sec.u), -dot(fwd, sec.v));
        let l = (fa * fa + fb * fb).sqrt().max(1e-6);
        c.dot(x as i32, y as i32, 6, [30, 120, 230]);
        c.line((x, y), (x + 40.0 * fa / l, y + 40.0 * fb / l), 3, [30, 120, 230]);
    }

    SectionImage { canvas: c, bounds, scale, left, top, points: pts.len(), cuts: cut_ext, grid_step }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_known_values() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn a_plan_section_has_x_right_and_y_up() {
        let s = Section::new([0.0; 3], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0], 1.0);
        assert_eq!(s.project([2.0, 3.0, 0.25]), (0.25, 2.0, 3.0));
    }

    #[test]
    fn draws_points_and_cuts_a_member() {
        let xyz: Vec<V3> = (0..1000).map(|k| [k as f32 * 0.004, 1.0, 0.5]).collect();
        let used: Vec<u32> = (0..1000).collect();
        let id = |p: V3| p;
        // A unit cube's six faces, centred at (2, 2, 0.5).
        let cube = |c: V3| {
            let v = |x: f32, y: f32, z: f32| [c[0] + x, c[1] + y, c[2] + z];
            vec![
                vec![v(-0.5, -0.5, -0.5), v(0.5, -0.5, -0.5), v(0.5, 0.5, -0.5), v(-0.5, 0.5, -0.5)],
                vec![v(-0.5, -0.5, 0.5), v(0.5, -0.5, 0.5), v(0.5, 0.5, 0.5), v(-0.5, 0.5, 0.5)],
                vec![v(-0.5, -0.5, -0.5), v(0.5, -0.5, -0.5), v(0.5, -0.5, 0.5), v(-0.5, -0.5, 0.5)],
                vec![v(-0.5, 0.5, -0.5), v(0.5, 0.5, -0.5), v(0.5, 0.5, 0.5), v(-0.5, 0.5, 0.5)],
                vec![v(-0.5, -0.5, -0.5), v(-0.5, 0.5, -0.5), v(-0.5, 0.5, 0.5), v(-0.5, -0.5, 0.5)],
                vec![v(0.5, -0.5, -0.5), v(0.5, 0.5, -0.5), v(0.5, 0.5, 0.5), v(0.5, -0.5, 0.5)],
            ]
        };
        let req = SectionRequest {
            section: Section::new([0.0, 0.0, 0.5], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0], 0.2),
            xyz: &xyz,
            used: &used,
            to_frame: &id,
            bounds: None,
            max_px: 600,
            colour: PointColour::Density,
            intensity: None,
            members: vec![MemberLayer { faces: cube([2.0, 2.0, 0.5]), colour: [0, 160, 0], highlight: false }],
            layers: vec![],
            marks: vec![],
            eye: None,
        };
        let img = render_section(&req);
        assert_eq!(img.points, 1000);
        let cut = img.cuts[0].expect("the plane cuts the cube");
        for (got, want) in cut.iter().zip([1.5, 2.5, 1.5, 2.5]) {
            assert!((got - want).abs() < 1e-5, "{cut:?}");
        }
        // The bounds hold the points and the cut.
        assert!(img.bounds[0] < 0.01 && img.bounds[1] > 2.5 && img.bounds[2] < 1.0 && img.bounds[3] > 2.5, "{:?}", img.bounds);
        assert!(img.canvas.png().starts_with(b"\x89PNG"));
    }
}
