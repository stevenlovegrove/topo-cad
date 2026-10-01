//! Reading a scan: a binary little-endian PLY's vertices, with x/y/z and,
//! when present, intensity, time (s since the first point) and range (the
//! distance from the sensor when observed, m). A header line
//! `comment t0_ns N` gives the absolute time of `time == 0`, so the times
//! can be related to a trajectory.

use std::fs::File;
use std::path::Path;

use anyhow::{Context, Result};

/// A point cloud read from a PLY. Optional columns are empty when absent.
#[derive(Debug, Default, Clone)]
pub struct PlyCloud {
    pub xyz: Vec<[f32; 3]>,
    /// 0..1.
    pub intensity: Vec<f32>,
    /// Seconds since the first point.
    pub time: Vec<f32>,
    /// Observation distance, m.
    pub range: Vec<f32>,
    /// Absolute time of `time == 0` (ns), when the file records it.
    pub t0_ns: Option<u64>,
}

fn type_size(t: &str) -> Result<usize> {
    Ok(match t {
        "char" | "uchar" | "int8" | "uint8" => 1,
        "short" | "ushort" | "int16" | "uint16" => 2,
        "int" | "uint" | "float" | "int32" | "uint32" | "float32" => 4,
        "double" | "float64" => 8,
        other => anyhow::bail!("unsupported PLY property type {other}"),
    })
}

fn read_value(t: &str, b: &[u8]) -> f64 {
    match t {
        "char" | "int8" => b[0] as i8 as f64,
        "uchar" | "uint8" => b[0] as f64,
        "short" | "int16" => i16::from_le_bytes([b[0], b[1]]) as f64,
        "ushort" | "uint16" => u16::from_le_bytes([b[0], b[1]]) as f64,
        "int" | "int32" => i32::from_le_bytes(b[..4].try_into().unwrap()) as f64,
        "uint" | "uint32" => u32::from_le_bytes(b[..4].try_into().unwrap()) as f64,
        "float" | "float32" => f32::from_le_bytes(b[..4].try_into().unwrap()) as f64,
        _ => f64::from_le_bytes(b[..8].try_into().unwrap()),
    }
}

pub fn read_ply(path: impl AsRef<Path>) -> Result<PlyCloud> {
    use std::io::{BufRead, BufReader, Read};
    let path = path.as_ref();
    let mut r = BufReader::with_capacity(1 << 24, File::open(path).with_context(|| format!("opening {}", path.display()))?);
    let mut line = String::new();
    r.read_line(&mut line)?;
    anyhow::ensure!(line.trim() == "ply", "{} is not a PLY file", path.display());
    let (mut n, mut props, mut in_vertex, mut format, mut t0_ns) = (0usize, Vec::new(), false, String::new(), None);
    loop {
        line.clear();
        anyhow::ensure!(r.read_line(&mut line)? > 0, "PLY header has no end_header");
        let f: Vec<&str> = line.split_whitespace().collect();
        match f.as_slice() {
            ["format", fmt, ..] => format = fmt.to_string(),
            ["comment", "t0_ns", v] => t0_ns = v.parse().ok().filter(|&t: &u64| t > 0),
            ["element", "vertex", count] => {
                n = count.parse()?;
                in_vertex = true;
            }
            ["element", ..] => in_vertex = false,
            ["property", "list", ..] if in_vertex => anyhow::bail!("list properties on vertices are not supported"),
            ["property", t, name] if in_vertex => props.push((t.to_string(), name.to_string())),
            ["end_header"] => break,
            _ => {}
        }
    }
    anyhow::ensure!(format == "binary_little_endian", "only binary_little_endian PLY is supported (got {format})");
    let mut offsets = Vec::new();
    let mut stride = 0;
    for (t, _) in &props {
        offsets.push(stride);
        stride += type_size(t)?;
    }
    let find = |name: &str| props.iter().position(|(_, p)| p == name);
    let (ix, iy, iz) = (find("x").context("no x")?, find("y").context("no y")?, find("z").context("no z")?);
    let (ii, it, ir) = (find("intensity"), find("time"), find("range"));
    let mut buf = vec![0u8; stride * n];
    r.read_exact(&mut buf).context("PLY body shorter than its header says")?;
    let mut c = PlyCloud { t0_ns, ..Default::default() };
    c.xyz.reserve(n);
    let int_scale = ii.map_or(1.0, |i| if props[i].0.contains("char") || props[i].0.contains("int8") { 1.0 / 255.0 } else { 1.0 });
    for k in 0..n {
        let rec = &buf[k * stride..(k + 1) * stride];
        let v = |i: usize| read_value(&props[i].0, &rec[offsets[i]..]);
        c.xyz.push([v(ix) as f32, v(iy) as f32, v(iz) as f32]);
        if let Some(i) = ii {
            c.intensity.push((v(i) * int_scale) as f32);
        }
        if let Some(i) = it {
            c.time.push(v(i) as f32);
        }
        if let Some(i) = ir {
            c.range.push(v(i) as f32);
        }
    }
    Ok(c)
}

/// Writes points as a PLY this module reads (x, y, z, intensity, time,
/// range): for tests and synthetic scans.
pub fn write_ply(path: impl AsRef<Path>, c: &PlyCloud) -> Result<()> {
    use std::io::Write;
    let mut out = std::io::BufWriter::new(File::create(path.as_ref())?);
    write!(out, "ply\nformat binary_little_endian 1.0\ncomment t0_ns {}\nelement vertex {}\nproperty float x\nproperty float y\nproperty float z\nproperty uchar intensity\nproperty float time\nproperty float range\nend_header\n", c.t0_ns.unwrap_or(0), c.xyz.len())?;
    for (k, p) in c.xyz.iter().enumerate() {
        for x in p {
            out.write_all(&x.to_le_bytes())?;
        }
        out.write_all(&[(c.intensity.get(k).copied().unwrap_or(0.0) * 255.0).round().clamp(0.0, 255.0) as u8])?;
        out.write_all(&c.time.get(k).copied().unwrap_or(0.0).to_le_bytes())?;
        out.write_all(&c.range.get(k).copied().unwrap_or(0.0).to_le_bytes())?;
    }
    out.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_written_cloud_reads_back() {
        let path = std::env::temp_dir().join(format!("topo_scan_ply_{}.ply", std::process::id()));
        let c = PlyCloud { xyz: vec![[1.0, 2.0, 3.0], [-4.0, 5.5, 0.25]], intensity: vec![0.0, 1.0], time: vec![0.0, 1.5], range: vec![2.0, 4.0], t0_ns: Some(1_000) };
        write_ply(&path, &c).unwrap();
        let back = read_ply(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(back.xyz, c.xyz);
        assert_eq!(back.intensity, c.intensity);
        assert_eq!(back.time, c.time);
        assert_eq!(back.range, c.range);
        assert_eq!(back.t0_ns, Some(1_000));
    }
}
