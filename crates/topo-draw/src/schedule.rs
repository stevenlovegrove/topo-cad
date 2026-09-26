//! Piece marks, member (cut-list) schedule, connection schedule, and table layout.

use crate::doc::{text_width, Align, Drawing, Layer};
use std::collections::BTreeMap;
use topo_core::units::{fmt_ft_in, INCH};
use topo_core::{ConnectionId, MemberId, Model, Topology};
use topo_geom::Geometry;
use topo_timber::lumber::board_feet;

fn prefix(role: &str) -> &'static str {
    match role {
        "stud" => "S",
        "king_stud" => "K",
        "jack_stud" => "J",
        "cripple" => "CR",
        "header" => "H",
        "sill" => "SL",
        "bottom_plate" => "BP",
        "top_plate" => "TP",
        "cap_plate" => "CP",
        "joist" => "FJ",
        "rim_joist" => "RJ",
        "band_joist" => "BJ",
        "beam" => "B",
        "post" => "P",
        "rafter" => "R",
        "top_chord" => "TC",
        "bottom_chord" => "BC",
        "web" => "W",
        _ => "M",
    }
}

#[derive(Clone, Debug)]
pub struct MemberRow {
    pub mark: String,
    pub role: String,
    pub section: String,
    pub material: String,
    /// Long-point cut length (m).
    pub length: f64,
    /// End cut angles from square (degrees).
    pub angles: (f64, f64),
    pub members: Vec<MemberId>,
    pub board_feet_each: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct Marks {
    /// Mark per member id.
    pub by_member: Vec<String>,
    pub rows: Vec<MemberRow>,
}

impl Marks {
    pub fn of(&self, m: MemberId) -> &str {
        &self.by_member[m.idx()]
    }

    pub fn assign(model: &Model, geom: &Geometry) -> Marks {
        type Key = (String, String, String, i64, i64, i64);
        let mut groups: BTreeMap<Key, MemberRow> = BTreeMap::new();
        for m in &model.members {
            let g = geom.member(m.id);
            let len = g.cut_length();
            let (a0, a1) = g.cut_angles();
            let sec = model.section(m.section);
            let key = (
                m.mark.clone().unwrap_or_else(|| prefix(&m.role).to_string()),
                sec.name.clone(),
                model.material(m.material).name.clone(),
                -((len / INCH * 16.0).round() as i64),
                (a0.min(a1) * 10.0).round() as i64,
                (a0.max(a1) * 10.0).round() as i64,
            );
            groups
                .entry(key)
                .or_insert_with(|| MemberRow {
                    mark: String::new(),
                    role: m.role.replace('_', " "),
                    section: sec.name.clone(),
                    material: model.material(m.material).name.clone(),
                    length: len,
                    angles: (a0.min(a1), a0.max(a1)),
                    members: vec![],
                    board_feet_each: board_feet(sec, len),
                })
                .members
                .push(m.id);
        }
        let mut counters: BTreeMap<String, usize> = BTreeMap::new();
        let mut by_member = vec![String::new(); model.members.len()];
        let mut rows = vec![];
        for ((pfx, ..), mut row) in groups {
            let explicit = model.member(row.members[0]).mark.is_some();
            row.mark = if explicit {
                pfx.clone()
            } else {
                let c = counters.entry(pfx.clone()).or_insert(0);
                *c += 1;
                format!("{pfx}{c}")
            };
            for &id in &row.members {
                by_member[id.idx()] = row.mark.clone();
            }
            rows.push(row);
        }
        rows.sort_by_key(|a| natural(&a.mark));
        Marks { by_member, rows }
    }
}

/// Sort key splitting alphabetic prefix and numeric suffix (`S2` < `S10`).
fn natural(s: &str) -> (String, u64) {
    let split = s.find(|c: char| c.is_ascii_digit()).unwrap_or(s.len());
    (s[..split].to_string(), s[split..].parse().unwrap_or(0))
}

#[derive(Clone, Debug)]
pub struct ConnRow {
    pub mark: String,
    pub id: ConnectionId,
    pub name: String,
    pub description: String,
    pub qty: usize,
    pub reference: String,
}

pub fn connection_schedule(model: &Model) -> Vec<ConnRow> {
    let mut qty = vec![0usize; model.connections.len()];
    for j in &model.joints {
        for c in &j.connections {
            qty[c.connection.idx()] += 1;
        }
    }
    for b in &model.bonds {
        qty[b.connection.idx()] += 1;
    }
    model
        .connections
        .iter()
        .filter(|c| qty[c.id.idx()] > 0)
        .map(|c| ConnRow {
            mark: format!("C{}", c.id.0 + 1),
            id: c.id,
            name: c.name.clone(),
            description: c.describe(),
            qty: qty[c.id.idx()],
            reference: c.reference.clone().unwrap_or_default(),
        })
        .collect()
}

/// Table layout in paper units (m); top-left corner at the origin, growing down.
pub fn table(title: &str, headers: &[&str], widths_in: &[f64], aligns: &[Align], rows: &[Vec<String>]) -> Drawing {
    let mut d = Drawing::default();
    let (row_h, text_h, pad) = (0.24 * INCH, (3.0 / 32.0) * INCH, 0.06 * INCH);
    let total_w: f64 = widths_in.iter().sum::<f64>() * INCH;
    d.bold(topo_core::v2(0.0, 0.05 * INCH), title.to_uppercase(), (5.0 / 32.0) * INCH, Align::Start, Layer::Text);
    let n = rows.len() + 1;
    let height = n as f64 * row_h;
    d.rect(topo_core::v2(0.0, -height), topo_core::v2(total_w, 0.0), Layer::Title);
    d.line(topo_core::v2(0.0, -row_h), topo_core::v2(total_w, -row_h), Layer::Title);
    for r in 2..n {
        let y = -(r as f64) * row_h;
        d.line(topo_core::v2(0.0, y), topo_core::v2(total_w, y), Layer::Text);
    }
    let mut x = 0.0;
    for (c, w) in widths_in.iter().enumerate() {
        let w = w * INCH;
        if c > 0 {
            d.line(topo_core::v2(x, 0.0), topo_core::v2(x, -height), Layer::Text);
        }
        let cell = |row: usize, s: &str, d: &mut Drawing, bold: bool| {
            let y = -(row as f64 + 1.0) * row_h + (row_h - text_h) / 2.0;
            let mut s = s.to_string();
            while s.chars().count() > 3 && text_width(&s, text_h) > w - 2.0 * pad {
                s.pop();
            }
            let (px, al) = match aligns.get(c).copied().unwrap_or(Align::Start) {
                Align::Start => (x + pad, Align::Start),
                Align::Middle => (x + w / 2.0, Align::Middle),
                Align::End => (x + w - pad, Align::End),
            };
            if bold {
                d.bold(topo_core::v2(px, y), s, text_h, al, Layer::Text);
            } else {
                d.text(topo_core::v2(px, y), s, text_h, al, Layer::Text);
            }
        };
        cell(0, &headers[c].to_uppercase(), &mut d, true);
        for (r, row) in rows.iter().enumerate() {
            cell(r + 1, row.get(c).map(String::as_str).unwrap_or(""), &mut d, false);
        }
        x += w;
    }
    d
}

pub fn member_schedule_table(marks: &Marks) -> Drawing {
    let rows: Vec<Vec<String>> = marks
        .rows
        .iter()
        .map(|r| {
            let cut = if r.angles.1 > 0.05 { format!("{:.1}°", r.angles.1) } else { "SQ".into() };
            vec![
                r.mark.clone(),
                r.members.len().to_string(),
                r.section.clone(),
                r.material.clone(),
                fmt_ft_in(r.length),
                cut,
                r.role.clone(),
                r.board_feet_each.map(|b| format!("{:.1}", b * r.members.len() as f64)).unwrap_or_default(),
            ]
        })
        .collect();
    let total: f64 = marks.rows.iter().filter_map(|r| r.board_feet_each.map(|b| b * r.members.len() as f64)).sum();
    let mut rows = rows;
    rows.push(vec!["".into(), marks.rows.iter().map(|r| r.members.len()).sum::<usize>().to_string(), "".into(), "".into(), "".into(), "".into(), "TOTAL".into(), format!("{total:.0}")]);
    use Align::*;
    table(
        "Member schedule / cut list",
        &["Mark", "Qty", "Size", "Grade", "Length (L.P.)", "Cut", "Role", "Bd ft"],
        &[0.7, 0.5, 1.0, 1.1, 1.1, 0.6, 1.2, 0.7],
        &[Middle, End, Start, Start, End, Middle, Start, End],
        &rows,
    )
}

pub fn connection_schedule_table(model: &Model) -> Drawing {
    let rows: Vec<Vec<String>> = connection_schedule(model)
        .into_iter()
        .map(|c| vec![c.mark, c.name, c.description, c.qty.to_string(), c.reference])
        .collect();
    use Align::*;
    table(
        "Connection schedule",
        &["Mark", "Connection", "Fastening", "Qty", "Reference"],
        &[0.6, 2.6, 3.2, 0.5, 1.4],
        &[Middle, Start, Start, End, Start],
        &rows,
    )
}

pub fn junction_table(topo: &Topology) -> Drawing {
    let rows: Vec<Vec<String>> = topo
        .census()
        .into_iter()
        .map(|(k, n)| vec![k.glyph().to_string(), k.label().to_string(), n.to_string()])
        .collect();
    use Align::*;
    table("Junction summary", &["Sym", "Junction", "Count"], &[0.5, 1.8, 0.7], &[Middle, Start, End], &rows)
}
