//! 2D drawing primitives. Coordinates are metres in the view's own plane
//! (model scale for projected views, paper scale for tables/title blocks).

use serde::{Deserialize, Serialize};
use topo_core::{BBox2, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Layer {
    /// Visible member edges.
    Framing,
    /// Hidden member edges (dashed).
    Hidden,
    /// Centre lines / analytical model.
    Center,
    Dims,
    Text,
    /// Opening outlines and other drafting aids (dashed).
    Annot,
    /// Member tags.
    Tags,
    Title,
    /// Junction markers in analytical views.
    Nodes,
    Supports,
    /// Opaque background behind labels (SVG only).
    Knockout,
    /// Analytical (centre-line) model.
    Analytical,
}

impl Layer {
    pub const ALL: [Layer; 12] = [
        Layer::Knockout,
        Layer::Analytical,
        Layer::Framing,
        Layer::Hidden,
        Layer::Center,
        Layer::Dims,
        Layer::Text,
        Layer::Annot,
        Layer::Tags,
        Layer::Title,
        Layer::Nodes,
        Layer::Supports,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Layer::Framing => "S-FRAM",
            Layer::Hidden => "S-FRAM-HIDN",
            Layer::Center => "S-CNTR",
            Layer::Dims => "S-DIMS",
            Layer::Text => "S-TEXT",
            Layer::Annot => "S-ANNO",
            Layer::Tags => "S-TAGS",
            Layer::Title => "G-TTLB",
            Layer::Nodes => "S-NODE",
            Layer::Supports => "S-SUPP",
            Layer::Knockout => "S-MASK",
            Layer::Analytical => "S-ANLY",
        }
    }
    pub fn dashed(self) -> Option<&'static str> {
        match self {
            Layer::Hidden | Layer::Annot => Some("HIDDEN"),
            Layer::Center => Some("CENTER"),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Align {
    Start,
    Middle,
    End,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Prim {
    Line { a: Vec2, b: Vec2, layer: Layer },
    Poly { pts: Vec<Vec2>, closed: bool, fill: bool, layer: Layer },
    Circle { c: Vec2, r: f64, fill: bool, layer: Layer },
    /// `at` is the baseline anchor; `height` is cap height; rotation in degrees CCW.
    Text { at: Vec2, text: String, height: f64, align: Align, rotation: f64, layer: Layer, bold: bool },
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Drawing {
    pub prims: Vec<Prim>,
}

impl Drawing {
    pub fn line(&mut self, a: Vec2, b: Vec2, layer: Layer) {
        self.prims.push(Prim::Line { a, b, layer });
    }
    pub fn poly(&mut self, pts: Vec<Vec2>, closed: bool, layer: Layer) {
        self.prims.push(Prim::Poly { pts, closed, fill: false, layer });
    }
    pub fn fill(&mut self, pts: Vec<Vec2>, layer: Layer) {
        self.prims.push(Prim::Poly { pts, closed: true, fill: true, layer });
    }
    pub fn rect(&mut self, min: Vec2, max: Vec2, layer: Layer) {
        self.poly(vec![min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)], true, layer);
    }
    pub fn circle(&mut self, c: Vec2, r: f64, fill: bool, layer: Layer) {
        self.prims.push(Prim::Circle { c, r, fill, layer });
    }
    pub fn text(&mut self, at: Vec2, text: impl Into<String>, height: f64, align: Align, layer: Layer) {
        self.prims.push(Prim::Text { at, text: text.into(), height, align, rotation: 0.0, layer, bold: false });
    }
    pub fn text_rot(&mut self, at: Vec2, text: impl Into<String>, height: f64, align: Align, rotation: f64, layer: Layer) {
        self.prims.push(Prim::Text { at, text: text.into(), height, align, rotation, layer, bold: false });
    }
    pub fn bold(&mut self, at: Vec2, text: impl Into<String>, height: f64, align: Align, layer: Layer) {
        self.prims.push(Prim::Text { at, text: text.into(), height, align, rotation: 0.0, layer, bold: true });
    }
    pub fn extend(&mut self, o: Drawing) {
        self.prims.extend(o.prims);
    }

    /// Approximate extents (text measured with a 0.6·h average glyph width).
    pub fn bbox(&self) -> BBox2 {
        let mut b = BBox2::EMPTY;
        for p in &self.prims {
            match p {
                Prim::Line { a, b: e, .. } => b = b.including(*a).including(*e),
                Prim::Poly { pts, .. } => b = pts.iter().fold(b, |bb, q| bb.including(*q)),
                Prim::Circle { c, r, .. } => b = b.including(*c - Vec2::new(*r, *r)).including(*c + Vec2::new(*r, *r)),
                Prim::Text { at, text, height, align, rotation, .. } => {
                    let w = text_width(text, *height);
                    let x0 = match align {
                        Align::Start => 0.0,
                        Align::Middle => -w / 2.0,
                        Align::End => -w,
                    };
                    let (s, c) = rotation.to_radians().sin_cos();
                    for (dx, dy) in [(x0, 0.0), (x0 + w, 0.0), (x0, *height), (x0 + w, *height)] {
                        b = b.including(*at + Vec2::new(dx * c - dy * s, dx * s + dy * c));
                    }
                }
            }
        }
        b
    }

    /// Applies `p ↦ p·scale + offset` to every primitive (text heights scale too).
    pub fn transformed(&self, scale: f64, offset: Vec2) -> Drawing {
        let f = |p: Vec2| p * scale + offset;
        let prims = self
            .prims
            .iter()
            .map(|p| match p {
                Prim::Line { a, b, layer } => Prim::Line { a: f(*a), b: f(*b), layer: *layer },
                Prim::Poly { pts, closed, fill, layer } => {
                    Prim::Poly { pts: pts.iter().map(|q| f(*q)).collect(), closed: *closed, fill: *fill, layer: *layer }
                }
                Prim::Circle { c, r, fill, layer } => Prim::Circle { c: f(*c), r: r * scale, fill: *fill, layer: *layer },
                Prim::Text { at, text, height, align, rotation, layer, bold } => Prim::Text {
                    at: f(*at),
                    text: text.clone(),
                    height: height * scale,
                    align: *align,
                    rotation: *rotation,
                    layer: *layer,
                    bold: *bold,
                },
            })
            .collect();
        Drawing { prims }
    }
}

/// Estimated rendered width of `text` at cap height `h`.
pub fn text_width(text: &str, h: f64) -> f64 {
    text.chars().count() as f64 * h * 0.62
}
