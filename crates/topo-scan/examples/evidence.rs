//! `cargo run --release -p topo-scan --example evidence -- MODEL.ts SCAN.ply [yaw_deg tx ty tz]`
//!
//! Loads a model and a scan, registers the model in the scan (or refines
//! the placement given) and prints the evidence per group of members.

use std::time::Instant;

use topo_scan::scan::Scan;
use topo_scan::{contradicted, evidence, group_table, icp, predict, register, FaceSamples, Placement};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [model, scan, rest @ ..] = args.as_slice() else {
        anyhow::bail!("usage: evidence MODEL.ts SCAN.ply [yaw_deg tx ty tz]");
    };
    let t = Instant::now();
    let pred = predict(model)?;
    let samples = FaceSamples::new(&pred, 0.05);
    println!("model {:?}: {} members ({:.0} ms)", pred.name, pred.members.len(), t.elapsed().as_secs_f64() * 1e3);
    let t = Instant::now();
    let scan = Scan::load(scan)?;
    println!("scan: {} points ({} used), floor z = {:.3}, wall axis {:.1}°, sensor rays {} ({:.1} s)", scan.xyz.len(), scan.used.len(), scan.floor_z, scan.wall_axis[1].atan2(scan.wall_axis[0]).to_degrees(), if scan.sensor.is_some() { "yes" } else { "no" }, t.elapsed().as_secs_f64());
    let t = Instant::now();
    let (pl, inliers, rms) = match rest {
        [yaw, x, y, z] => icp(&samples, &scan.sub, Placement { yaw: yaw.parse::<f32>()?.to_radians(), t: [x.parse()?, y.parse()?, z.parse()?] }, &[0.1, 0.05, 0.03]),
        _ => {
            let r = register(&pred, &samples, &scan.sub, scan.wall_axis, scan.floor_z);
            for (p, inl) in &r.candidates {
                println!("  candidate yaw {:.2}°: {:.1}% within 3 cm", p.yaw.to_degrees(), 100.0 * inl);
            }
            (r.placement, r.inliers, r.rms)
        }
    };
    println!("placement: yaw {:.3}°, t [{:.3}, {:.3}, {:.3}]; {:.1}% of scan points within 3 cm of a model face, rms {:.1} mm ({:.1} s)", pl.yaw.to_degrees(), pl.t[0], pl.t[1], pl.t[2], 100.0 * inliers, rms * 1e3, t.elapsed().as_secs_f64());
    let t = Instant::now();
    let ev = evidence(&pred, &samples, &pl, &scan.xyz, &scan.used, scan.sensor.as_deref(), 0.05, 4);
    println!("evidence in {:.1} s\n{}", t.elapsed().as_secs_f64(), group_table(&pred, &ev));
    println!("most contradicted:");
    for k in contradicted(&pred, &ev, 200, 10) {
        println!("  {:<56} {:>4.0}% of {:>6} rays through it; {:>6} points", pred.members[k].path, 100.0 * ev[k].rays.pierce_frac(), ev[k].rays.pierced + ev[k].rays.ended, ev[k].points);
    }
    Ok(())
}
