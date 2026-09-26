//! The measurements sidecar file (`<model>.measured.ts`): tape readings added
//! from the UI, written as plain TypeScript the model imports. The script
//! stays the source of truth; this module only appends readable entries.

use std::path::{Path, PathBuf};
use topo_core::units::{INCH, MM};
use topo_core::Model;
use topo_geom::measure::{Feature, PlaneRef, Quantity};

/// `garage.ts` → `garage.measured.ts`.
pub fn sidecar_path(model_file: &Path) -> PathBuf {
    let stem = model_file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    model_file.with_file_name(format!("{stem}.measured.ts"))
}

/// The import line a model needs to pick up its sidecar.
pub fn import_hint(model_file: &Path) -> String {
    let stem = model_file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    format!("import {{ fieldMeasurements }} from \"./{stem}.measured\";  // then: .measure(fieldMeasurements)")
}

const TEMPLATE: &str = "// Field measurements, added with `topo serve`. Plain TypeScript: edit freely.
import { type Measurement, horizontal, inch, lengthOf, meet, measured, member, vertical } from \"topo-cad\";

export const fieldMeasurements: Measurement[] = [
];
";

/// Creates the sidecar from the template if it does not exist yet.
pub fn ensure(model_file: &Path) -> std::io::Result<PathBuf> {
    let path = sidecar_path(model_file);
    if !path.exists() {
        std::fs::write(&path, TEMPLATE)?;
    }
    Ok(path)
}

/// Parses a tape reading: `3' 6 5/16"`, `3'-6 5/16"`, `42.3"`, `42 1/2 in`,
/// `28'`, `28 ft`, `1070 mm`, `107 cm`, `1.07 m`. A bare number is inches.
pub fn parse_length(text: &str) -> Result<f64, String> {
    let t = text.trim().to_lowercase().replace('\u{2033}', "\"").replace('\u{2032}', "'").replace('\u{201d}', "\"").replace('\u{2019}', "'");
    let bad = || format!("can't read \"{}\" as a length (try 3' 6 5/16\", 42.3\" or 1070mm)", text.trim());
    if t.is_empty() {
        return Err(bad());
    }
    for (suffix, unit) in [("mm", MM), ("cm", 10.0 * MM), ("m", 1.0)] {
        if let Some(n) = t.strip_suffix(suffix) {
            return n.trim().parse::<f64>().map(|v| v * unit).map_err(|_| bad());
        }
    }
    // Imperial: optional feet, then optional inches with an optional fraction.
    let (feet_part, inch_part) = match t.find(['\'']).or_else(|| t.find("ft")) {
        Some(i) => {
            let skip = if t[i..].starts_with("ft") { 2 } else { 1 };
            (Some(&t[..i]), t[i + skip..].trim_start_matches(['-', ' ']))
        }
        None => (None, t.as_str()),
    };
    let feet = match feet_part {
        Some(f) => f.trim().parse::<f64>().map_err(|_| bad())?,
        None => 0.0,
    };
    let inch_text = inch_part.trim().trim_end_matches("inches").trim_end_matches("in").trim_end_matches('"').trim();
    let mut inches = 0.0;
    for word in inch_text.split_whitespace() {
        inches += match word.split_once('/') {
            Some((n, d)) => {
                let (n, d): (f64, f64) = (n.parse().map_err(|_| bad())?, d.parse().map_err(|_| bad())?);
                if d == 0.0 {
                    return Err(bad());
                }
                n / d
            }
            None => word.parse::<f64>().map_err(|_| bad())?,
        };
    }
    Ok(feet * 12.0 * INCH + inches * INCH)
}

/// Shortest `/`-separated tail of a member path that still names it uniquely.
pub fn short_selector(model: &Model, path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    for k in 1..=parts.len() {
        let sel = parts[parts.len() - k..].join("/");
        if model.find_member(&sel).is_ok() {
            return sel;
        }
    }
    path.to_string()
}

fn js_str(s: &str) -> String {
    serde_json::to_string(s).unwrap()
}

fn plane_ts(model: &Model, p: &PlaneRef) -> String {
    match p {
        PlaneRef::Face { member, side } => format!("member({}).face({})", js_str(&short_selector(model, member)), js_str(side)),
        PlaneRef::Mid { member, axis } => format!("member({}).mid({})", js_str(&short_selector(model, member)), js_str(axis)),
    }
}

fn feature_ts(model: &Model, f: &Feature) -> String {
    match f.planes.as_slice() {
        [one] => plane_ts(model, one),
        many => format!("meet({})", many.iter().map(|p| plane_ts(model, p)).collect::<Vec<_>>().join(", ")),
    }
}

/// TypeScript for a quantity, e.g. `horizontal(member("T2/…").face("start"), meet(…))`.
pub fn quantity_ts(model: &Model, q: &Quantity) -> String {
    match q {
        Quantity::Horizontal { a, b } => format!("horizontal({}, {})", feature_ts(model, a), feature_ts(model, b)),
        Quantity::Vertical { a, b } => format!("vertical({}, {})", feature_ts(model, a), feature_ts(model, b)),
        Quantity::Length { member, how } => format!("lengthOf(member({}), {})", js_str(&short_selector(model, member)), js_str(how)),
    }
}

/// `inch(42.3)` for a length in metres (to 1/1000").
pub fn value_ts(v: f64) -> String {
    let s = format!("{:.3}", v / INCH);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    format!("inch({s})")
}

/// Appends a measurement to the sidecar file (creating it if needed) and
/// returns the TypeScript line written.
pub fn append(model_file: &Path, model: &Model, name: &str, q: &Quantity, value: f64, note: Option<&str>) -> std::io::Result<String> {
    let path = ensure(model_file)?;
    let text = std::fs::read_to_string(&path)?;
    let comment = note.map(|n| format!(" // {n}")).unwrap_or_default();
    let line = format!("  measured({}, {}, {}),{comment}\n", js_str(name), quantity_ts(model, q), value_ts(value));
    let close = text.rfind("];").ok_or_else(|| std::io::Error::other(format!("{} has no closing `];`", path.display())))?;
    let mut out = text[..close].to_string();
    out.push_str(&line);
    out.push_str(&text[close..]);
    std::fs::write(&path, out)?;
    Ok(line.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use topo_core::units::{ft, inch, mm};

    #[test]
    fn parses_tape_readings() {
        let close = |t: &str, v: f64| {
            let got = parse_length(t).unwrap_or_else(|e| panic!("{e}"));
            assert!((got - v).abs() < 1e-9, "{t}: {got} vs {v}");
        };
        close("3' 6 5/16\"", ft(3.0) + inch(6.3125));
        close("3'-6 5/16\"", ft(3.0) + inch(6.3125));
        close("42.3\"", inch(42.3));
        close("42 1/2 in", inch(42.5));
        close("28'", ft(28.0));
        close("28 ft", ft(28.0));
        close("5/8", inch(0.625));
        close("26", inch(26.0));
        close("1070mm", mm(1070.0));
        close("1.07 m", 1.07);
        assert!(parse_length("three feet").is_err());
        assert!(parse_length("1/0\"").is_err());
    }

    #[test]
    fn writes_readable_values() {
        assert_eq!(value_ts(inch(42.3)), "inch(42.3)");
        assert_eq!(value_ts(ft(28.0)), "inch(336)");
    }
}
