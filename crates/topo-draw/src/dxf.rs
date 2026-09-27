//! DXF R12 (AC1009) output. Model-space views are written full size in the
//! model's drawing unit (inches or millimetres); annotation sizes were already
//! computed for the view scale, and `$LTSCALE` is set to match.

use crate::doc::{Align, Drawing, Layer, Prim};
use std::fmt::Write;
use topo_core::units::{INCH, MM};
use topo_core::{UnitSystem, Vec2};

fn color(l: Layer) -> u8 {
    match l {
        Layer::Framing => 7,
        Layer::Hidden => 8,
        Layer::Center | Layer::Analytical => 5,
        Layer::Dims => 3,
        Layer::Text => 7,
        Layer::Annot => 9,
        Layer::Tags => 2,
        Layer::Title => 7,
        Layer::Nodes => 1,
        Layer::Supports => 6,
        Layer::Knockout => 7,
        Layer::Heat0 => 3,
        Layer::Heat1 => 51,
        Layer::Heat2 => 40,
        Layer::Heat3 => 1,
        Layer::Heat4 => 6,
    }
}

struct W(String);

impl W {
    fn g(&mut self, code: i32, v: impl std::fmt::Display) {
        let _ = write!(self.0, "{code}\n{v}\n");
    }
    fn f(&mut self, code: i32, v: f64) {
        let _ = write!(self.0, "{code}\n{v:.6}\n");
    }
    fn pt(&mut self, base: i32, p: Vec2) {
        self.f(base, p.x);
        self.f(base + 10, p.y);
        self.f(base + 20, 0.0);
    }
}

/// Writes `d` (coordinates in metres) with 1 drawing unit = `unit` metres.
/// `ltscale` scales the dashed linetypes (model units per paper inch/mm).
pub fn to_dxf(d: &Drawing, units: UnitSystem, ltscale: f64) -> String {
    let unit = match units {
        UnitSystem::Imperial => INCH,
        UnitSystem::Metric => MM,
    };
    let k = |p: Vec2| p / unit;
    let mut w = W(String::new());
    w.g(0, "SECTION");
    w.g(2, "HEADER");
    w.g(9, "$ACADVER");
    w.g(1, "AC1009");
    w.g(9, "$INSUNITS");
    w.g(70, if units == UnitSystem::Imperial { 1 } else { 4 });
    w.g(9, "$LTSCALE");
    w.f(40, ltscale);
    w.g(0, "ENDSEC");

    w.g(0, "SECTION");
    w.g(2, "TABLES");
    w.g(0, "TABLE");
    w.g(2, "LTYPE");
    w.g(70, 3);
    let paper = if units == UnitSystem::Imperial { 1.0 } else { 25.4 };
    for (name, desc, pat) in [
        ("CONTINUOUS", "Solid line", vec![]),
        ("HIDDEN", "Hidden __ __ __", vec![0.25, -0.125]),
        ("CENTER", "Center ____ _ ____", vec![1.25, -0.25, 0.25, -0.25]),
    ] {
        w.g(0, "LTYPE");
        w.g(2, name);
        w.g(70, 0);
        w.g(3, desc);
        w.g(72, 65);
        w.g(73, pat.len());
        w.f(40, pat.iter().map(|x: &f64| x.abs()).sum::<f64>() * paper);
        for x in pat {
            w.f(49, x * paper);
        }
    }
    w.g(0, "ENDTAB");
    w.g(0, "TABLE");
    w.g(2, "LAYER");
    w.g(70, Layer::ALL.len());
    for l in Layer::ALL {
        w.g(0, "LAYER");
        w.g(2, l.name());
        w.g(70, 0);
        w.g(62, color(l));
        w.g(6, l.dashed().unwrap_or("CONTINUOUS"));
    }
    w.g(0, "ENDTAB");
    w.g(0, "ENDSEC");

    w.g(0, "SECTION");
    w.g(2, "ENTITIES");
    for p in &d.prims {
        match p {
            Prim::Line { a, b, layer } => {
                w.g(0, "LINE");
                w.g(8, layer.name());
                w.pt(10, k(*a));
                w.pt(11, k(*b));
            }
            Prim::Poly { pts, closed, fill, layer } => {
                if *layer == Layer::Knockout {
                    continue;
                }
                if *fill && (pts.len() == 3 || pts.len() == 4) {
                    // SOLID vertex order is 1,2,4,3.
                    let q: Vec<Vec2> = pts.iter().map(|&x| k(x)).collect();
                    let (p3, p4) = if q.len() == 3 { (q[2], q[2]) } else { (q[3], q[2]) };
                    w.g(0, "SOLID");
                    w.g(8, layer.name());
                    w.pt(10, q[0]);
                    w.pt(11, q[1]);
                    w.pt(12, p3);
                    w.pt(13, p4);
                    continue;
                }
                w.g(0, "POLYLINE");
                w.g(8, layer.name());
                w.g(66, 1);
                w.g(70, if *closed { 1 } else { 0 });
                w.pt(10, Vec2::ZERO);
                for &q in pts {
                    w.g(0, "VERTEX");
                    w.g(8, layer.name());
                    w.pt(10, k(q));
                }
                w.g(0, "SEQEND");
                w.g(8, layer.name());
            }
            Prim::Circle { c, r, layer, .. } => {
                w.g(0, "CIRCLE");
                w.g(8, layer.name());
                w.pt(10, k(*c));
                w.f(40, r / unit);
            }
            Prim::Text { at, text, height, align, rotation, layer, .. } => {
                w.g(0, "TEXT");
                w.g(8, layer.name());
                w.pt(10, k(*at));
                w.f(40, height / unit);
                w.g(1, text.replace('°', "%%d"));
                if rotation.abs() > 1e-9 {
                    w.f(50, *rotation);
                }
                let h = match align {
                    Align::Start => 0,
                    Align::Middle => 1,
                    Align::End => 2,
                };
                if h != 0 {
                    w.g(72, h);
                    w.pt(11, k(*at));
                }
            }
        }
    }
    w.g(0, "ENDSEC");
    w.g(0, "EOF");
    w.0
}
