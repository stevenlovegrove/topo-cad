//! Drafting annotations sized in paper units and converted to model units via the view scale.

use crate::doc::{text_width, Align, Drawing, Layer};
use topo_core::units::INCH;
use topo_core::Vec2;

/// Paper sizes (m) converted to model units by `scale` (paper / model).
#[derive(Clone, Copy, Debug)]
pub struct Style {
    pub scale: f64,
}

impl Style {
    /// Paper length (inches) → model length (m).
    pub fn m(&self, paper_in: f64) -> f64 {
        paper_in * INCH / self.scale
    }
    pub fn text_h(&self) -> f64 {
        self.m(3.0 / 32.0)
    }
    pub fn small_h(&self) -> f64 {
        self.m(5.0 / 64.0)
    }
    pub fn title_h(&self) -> f64 {
        self.m(3.0 / 16.0)
    }
    /// Distance of the tier-`k` dimension line from the measured points.
    pub fn tier_offset(&self, tier: u32) -> f64 {
        self.m(0.45 + 0.35 * tier as f64)
    }
}

/// Keeps text upright: returns (direction, angle°) with angle in (-90°, 90°].
fn readable(dir: Vec2) -> (Vec2, f64) {
    let mut ang = dir.y.atan2(dir.x).to_degrees();
    let mut d = dir;
    if ang <= -90.0 + 1e-6 || ang > 90.0 + 1e-6 {
        ang += if ang <= -90.0 + 1e-6 { 180.0 } else { -180.0 };
        d = -dir;
    }
    (d, ang)
}

/// Linear dimension between `a` and `b`, with the dimension line offset by
/// `offset` toward `side`. Architectural tick marks.
pub fn dim(d: &mut Drawing, a: Vec2, b: Vec2, side: Vec2, offset: f64, text: &str, st: &Style) {
    let len = a.distance(b);
    if len < 1e-9 {
        return;
    }
    let dir = (b - a) / len;
    let n0 = side - dir * side.dot(dir);
    let n = if n0.norm() < 1e-9 { dir.perp() } else { n0.normalized() };
    let (a1, b1) = (a + n * offset, b + n * offset);
    let (gap, over, tick) = (st.m(1.0 / 32.0), st.m(1.0 / 16.0), st.m(1.0 / 16.0));
    if offset > gap {
        d.line(a + n * gap, a1 + n * over, Layer::Dims);
        d.line(b + n * gap, b1 + n * over, Layer::Dims);
    }
    d.line(a1 - dir * over, b1 + dir * over, Layer::Dims);
    let t = (dir + n).normalized() * (tick * 0.5);
    for p in [a1, b1] {
        d.line(p - t, p + t, Layer::Dims);
    }
    let (tdir, ang) = readable(dir);
    let up = tdir.perp();
    let h = st.small_h();
    let mid = (a1 + b1) * 0.5;
    let w = text_width(text, h);
    if w < len * 0.95 {
        d.text_rot(mid + up * st.m(1.0 / 32.0), text, h, Align::Middle, ang, Layer::Dims);
    } else {
        // Too tight: place beside the far end.
        let far = if (b1 - a1).dot(tdir) > 0.0 { b1 } else { a1 };
        d.text_rot(far + tdir * st.m(1.0 / 8.0) + up * st.m(1.0 / 32.0), text, h, Align::Start, ang, Layer::Dims);
    }
}

/// Filled arrowhead with its tip at `tip`, pointing along `dir`.
pub fn arrowhead(d: &mut Drawing, tip: Vec2, dir: Vec2, st: &Style) {
    let dir = dir.normalized();
    let (l, w) = (st.m(0.12), st.m(0.035));
    let base = tip - dir * l;
    d.fill(vec![tip, base + dir.perp() * w, base - dir.perp() * w], Layer::Dims);
}

/// Double-headed span arrow with a callout above it.
pub fn span(d: &mut Drawing, a: Vec2, b: Vec2, text: &str, st: &Style) {
    d.line(a, b, Layer::Dims);
    let dir = (b - a).normalized();
    arrowhead(d, b, dir, st);
    arrowhead(d, a, -dir, st);
    let (tdir, ang) = readable(dir);
    d.text_rot((a + b) * 0.5 + tdir.perp() * st.m(1.0 / 16.0), text, st.text_h(), Align::Middle, ang, Layer::Text);
}

/// Piece-mark tag: boxed text centred at `at`.
pub fn tag(d: &mut Drawing, at: Vec2, text: &str, st: &Style) {
    let h = st.small_h();
    let w = text_width(text, h) + st.m(0.06);
    let hh = h + st.m(0.06);
    d.rect(at - Vec2::new(w / 2.0, hh / 2.0), at + Vec2::new(w / 2.0, hh / 2.0), Layer::Tags);
    d.text(at - Vec2::new(0.0, h / 2.0), text, h, Align::Middle, Layer::Tags);
}

/// View title with underline and scale caption, placed below `bottom_left`.
pub fn view_title(d: &mut Drawing, at: Vec2, title: &str, subtitle: Option<&str>, scale_label: &str, st: &Style) {
    let h = st.title_h();
    d.bold(at, title.to_uppercase(), h, Align::Start, Layer::Text);
    let w = text_width(title, h);
    d.line(at - Vec2::new(0.0, st.m(0.06)), at + Vec2::new(w, -st.m(0.06)), Layer::Title);
    let mut y = at.y - st.m(0.06) - st.text_h() * 1.6;
    d.text(Vec2::new(at.x, y), format!("SCALE: {scale_label}"), st.text_h(), Align::Start, Layer::Text);
    if let Some(s) = subtitle {
        y -= st.text_h() * 1.6;
        d.text(Vec2::new(at.x, y), s, st.text_h(), Align::Start, Layer::Text);
    }
}
