//! SVG output (paper space, inches).

use crate::doc::{Align, Drawing, Layer, Prim};
use std::fmt::Write;
use topo_core::units::INCH;
use topo_core::Vec2;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

const ORDER: [Layer; 12] = [
    Layer::Analytical,
    Layer::Hidden,
    Layer::Framing,
    Layer::Center,
    Layer::Annot,
    Layer::Supports,
    Layer::Nodes,
    Layer::Dims,
    Layer::Knockout,
    Layer::Tags,
    Layer::Title,
    Layer::Text,
];

const CSS: &str = "
  .S-FRAM { stroke:#000; stroke-width:0.012; fill:none; stroke-linecap:round }
  .S-FRAM-HIDN { stroke:#666; stroke-width:0.008; fill:none; stroke-dasharray:0.07 0.035 }
  .S-CNTR { stroke:#444; stroke-width:0.006; fill:none; stroke-dasharray:0.3 0.05 0.05 0.05 }
  .S-ANLY { stroke:#1f4e8c; stroke-width:0.008; fill:none }
  .S-DIMS { stroke:#000; stroke-width:0.005; fill:none }
  .S-DIMS.fill { fill:#000 }
  .S-ANNO { stroke:#555; stroke-width:0.007; fill:none; stroke-dasharray:0.06 0.04 }
  .S-TAGS { stroke:#000; stroke-width:0.005; fill:#fff }
  .S-MASK { fill:#fff; stroke:none }
  .S-NODE { stroke:#b3261e; stroke-width:0.006; fill:none }
  .S-NODE.fill { fill:#b3261e }
  .S-SUPP { stroke:#000; stroke-width:0.008; fill:none }
  .G-TTLB { stroke:#000; stroke-width:0.02; fill:none }
  text { font-family: 'Arial Narrow', 'Helvetica Neue', Arial, sans-serif; stroke:none; fill:#000 }
  text.b { font-weight:700 }
";

/// Renders a paper-space drawing of `size` (m) as a standalone SVG document.
pub fn to_svg(d: &Drawing, size: Vec2) -> String {
    let (w, h) = (size.x / INCH, size.y / INCH);
    let p = |q: Vec2| (q.x / INCH, h - q.y / INCH);
    let mut s = String::new();
    let _ = write!(
        s,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}in" height="{h}in" viewBox="0 0 {w} {h}"><style>{CSS}</style><rect width="{w}" height="{h}" fill="#fff"/>"##
    );
    for layer in ORDER {
        let _ = write!(s, r#"<g class="{}">"#, layer.name());
        for prim in &d.prims {
            match prim {
                Prim::Line { a, b, layer: l } if *l == layer => {
                    let (a, b) = (p(*a), p(*b));
                    let _ = write!(s, r#"<line x1="{:.4}" y1="{:.4}" x2="{:.4}" y2="{:.4}" class="{}"/>"#, a.0, a.1, b.0, b.1, layer.name());
                }
                Prim::Poly { pts, closed, fill, layer: l } if *l == layer => {
                    let pts: Vec<String> = pts.iter().map(|q| p(*q)).map(|(x, y)| format!("{x:.4},{y:.4}")).collect();
                    let tag = if *closed { "polygon" } else { "polyline" };
                    let cls = if *fill { format!("{} fill", layer.name()) } else { layer.name().to_string() };
                    let _ = write!(s, r#"<{tag} points="{}" class="{cls}"/>"#, pts.join(" "));
                }
                Prim::Circle { c, r, fill, layer: l } if *l == layer => {
                    let c = p(*c);
                    let cls = if *fill { format!("{} fill", layer.name()) } else { layer.name().to_string() };
                    let _ = write!(s, r#"<circle cx="{:.4}" cy="{:.4}" r="{:.4}" class="{cls}"/>"#, c.0, c.1, r / INCH);
                }
                Prim::Text { at, text, height, align, rotation, layer: l, bold } if *l == layer => {
                    let a = p(*at);
                    let anchor = match align {
                        Align::Start => "start",
                        Align::Middle => "middle",
                        Align::End => "end",
                    };
                    let fs = height / INCH / 0.72;
                    let rot = if rotation.abs() > 1e-9 { format!(r#" transform="rotate({:.3} {:.4} {:.4})""#, -rotation, a.0, a.1) } else { String::new() };
                    let cls = if *bold { r#" class="b""# } else { "" };
                    let _ = write!(s, r#"<text x="{:.4}" y="{:.4}" font-size="{fs:.4}" text-anchor="{anchor}"{cls}{rot}>{}</text>"#, a.0, a.1, esc(text));
                }
                _ => {}
            }
        }
        s.push_str("</g>");
    }
    s.push_str("</svg>\n");
    s
}
