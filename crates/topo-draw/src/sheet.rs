//! Sheets: border, title block, and packed views on paper.

use crate::doc::{text_width, Align, Drawing, Layer};
use crate::views::View;
use topo_core::units::INCH;
use topo_core::{v2, BBox2, Model, Vec2};

/// ARCH D landscape.
pub const ARCH_D: Vec2 = Vec2 { x: 36.0 * INCH, y: 24.0 * INCH };
const MARGIN: f64 = 0.5 * INCH;
const TITLE_W: f64 = 3.5 * INCH;
const GAP: f64 = 0.8 * INCH;

#[derive(Clone, Debug)]
pub struct PlacedView {
    pub view: View,
    /// Paper offset applied after scaling the view drawing.
    pub offset: Vec2,
}

#[derive(Clone, Debug)]
pub struct Sheet {
    pub number: String,
    pub title: String,
    pub size: Vec2,
    pub views: Vec<PlacedView>,
}

impl Sheet {
    /// Drawing area (paper) excluding border and title block.
    pub fn area(size: Vec2) -> BBox2 {
        BBox2 { min: v2(MARGIN + 0.25 * INCH, MARGIN + 0.25 * INCH), max: v2(size.x - MARGIN - TITLE_W - 0.25 * INCH, size.y - MARGIN - 0.25 * INCH) }
    }

    /// Everything on the sheet in paper metres.
    pub fn render(&self, model: &Model, index: usize, count: usize) -> Drawing {
        let mut d = Drawing::default();
        for pv in &self.views {
            d.extend(pv.view.drawing.transformed(pv.view.scale, pv.offset));
        }
        d.extend(title_block(model, self, index, count));
        d
    }
}

/// Shelf-packs views onto as many sheets as needed.
pub fn pack(number_prefix: &str, first_number: u32, title: &str, views: Vec<View>) -> Vec<Sheet> {
    let area = Sheet::area(ARCH_D);
    let mut sheets: Vec<Sheet> = vec![];
    let mut cur: Vec<PlacedView> = vec![];
    let (mut x, mut y, mut row_h) = (area.min.x, area.max.y, 0.0f64);
    let flush = |cur: &mut Vec<PlacedView>, sheets: &mut Vec<Sheet>| {
        if !cur.is_empty() {
            let n = first_number + sheets.len() as u32;
            sheets.push(Sheet { number: format!("{number_prefix}{n}"), title: title.into(), size: ARCH_D, views: std::mem::take(cur) });
        }
    };
    for v in views {
        let bb = v.paper_bbox();
        let (w, h) = (bb.width(), bb.height());
        if x > area.min.x && x + w > area.max.x {
            x = area.min.x;
            y -= row_h + GAP;
            row_h = 0.0;
        }
        if y < area.max.y && y - h < area.min.y {
            flush(&mut cur, &mut sheets);
            x = area.min.x;
            y = area.max.y;
            row_h = 0.0;
        }
        let offset = v2(x - bb.min.x, y - bb.max.y);
        cur.push(PlacedView { view: v, offset });
        x += w + GAP;
        row_h = row_h.max(h);
    }
    flush(&mut cur, &mut sheets);
    sheets
}

fn title_block(model: &Model, sheet: &Sheet, index: usize, count: usize) -> Drawing {
    let mut d = Drawing::default();
    let s = sheet.size;
    d.rect(v2(MARGIN, MARGIN), v2(s.x - MARGIN, s.y - MARGIN), Layer::Title);
    let x0 = s.x - MARGIN - TITLE_W;
    let x1 = s.x - MARGIN;
    d.line(v2(x0, MARGIN), v2(x0, s.y - MARGIN), Layer::Title);
    let pad = 0.12 * INCH;
    let th = 0.11 * INCH;
    let mut y = s.y - MARGIN - pad - 0.2 * INCH;
    let rule = |d: &mut Drawing, y: f64| d.line(v2(x0, y), v2(x1, y), Layer::Title);
    let wrap = |text: &str, h: f64| -> Vec<String> {
        let mut lines = vec![];
        let mut cur = String::new();
        for w in text.split_whitespace() {
            let cand = if cur.is_empty() { w.to_string() } else { format!("{cur} {w}") };
            if text_width(&cand, h) > TITLE_W - 2.0 * pad && !cur.is_empty() {
                lines.push(std::mem::replace(&mut cur, w.to_string()));
            } else {
                cur = cand;
            }
        }
        if !cur.is_empty() {
            lines.push(cur);
        }
        lines
    };
    d.bold(v2(x0 + pad, y), "TOPO-CAD", 0.2 * INCH, Align::Start, Layer::Text);
    y -= 0.25 * INCH;
    d.text(v2(x0 + pad, y), "topology-first framing model", th * 0.8, Align::Start, Layer::Text);
    y -= 0.25 * INCH;
    rule(&mut d, y);
    y -= 0.3 * INCH;
    d.text(v2(x0 + pad, y), "PROJECT", th * 0.8, Align::Start, Layer::Text);
    y -= 0.28 * INCH;
    for l in wrap(&model.info.name.to_uppercase(), 0.16 * INCH) {
        d.bold(v2(x0 + pad, y), l, 0.16 * INCH, Align::Start, Layer::Text);
        y -= 0.24 * INCH;
    }
    for (k, v) in [("No.", &model.info.number), ("Client", &model.info.client), ("Address", &model.info.address)] {
        if !v.is_empty() {
            d.text(v2(x0 + pad, y), format!("{k}: {v}"), th, Align::Start, Layer::Text);
            y -= 0.2 * INCH;
        }
    }
    y -= 0.1 * INCH;
    rule(&mut d, y);
    // Status stamp.
    y -= 0.15 * INCH;
    let stamp_h = 0.75 * INCH;
    d.rect(v2(x0 + pad, y - stamp_h), v2(x1 - pad, y), Layer::Title);
    d.bold(v2((x0 + x1) / 2.0, y - 0.3 * INCH), "PRELIMINARY", 0.16 * INCH, Align::Middle, Layer::Text);
    d.text(v2((x0 + x1) / 2.0, y - 0.55 * INCH), "NOT FOR CONSTRUCTION", th, Align::Middle, Layer::Text);
    y -= stamp_h + 0.15 * INCH;
    d.text(v2(x0 + pad, y - 0.1 * INCH), "Engineer of record review and seal required.", th * 0.8, Align::Start, Layer::Text);
    y -= 0.3 * INCH;
    rule(&mut d, y);
    y -= 0.3 * INCH;
    for (k, v) in [("Drawn", &model.info.designer), ("Date", &model.info.date)] {
        d.text(v2(x0 + pad, y), format!("{k}: {v}"), th, Align::Start, Layer::Text);
        y -= 0.2 * INCH;
    }
    // Bottom: sheet title and number.
    let yb = MARGIN + 1.6 * INCH;
    rule(&mut d, yb);
    let mut yt = yb - 0.3 * INCH;
    for l in wrap(&sheet.title.to_uppercase(), 0.14 * INCH) {
        d.bold(v2(x0 + pad, yt), l, 0.14 * INCH, Align::Start, Layer::Text);
        yt -= 0.22 * INCH;
    }
    d.bold(v2(x0 + pad, MARGIN + 0.25 * INCH), sheet.number.clone(), 0.4 * INCH, Align::Start, Layer::Text);
    d.text(v2(x1 - pad, MARGIN + 0.25 * INCH), format!("{} of {}", index + 1, count), th, Align::End, Layer::Text);
    d
}
