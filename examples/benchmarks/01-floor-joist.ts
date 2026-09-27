// Benchmark 1 — a floor joist, simply supported.
//
// A 2x10 Douglas Fir-Larch No.2 floor joist at 16" on centre spans 14'-0"
// (centre to centre of bearing) between two 4x4 posts. Floor dead load 10 psf,
// live load 40 psf. Check bending, shear, deflection and bearing (NDS, ASD).
//
// Hand calculation (the test recomputes it independently):
//   w_D = 10 psf × 16/12 ft + self-weight,  w_L = 40 psf × 16/12 ft = 53.3 plf
//   D + L:  M = wL²/8,  V = R = wL/2,  Δ = 5wL⁴/384EI
//   f_b = M/S vs F'_b = F_b C_D C_F C_r   (C_D = 1.0, C_F = 1.1 for 2x10, C_r = 1.15)
//   f_v = 3V/2A vs F'_v = F_v C_D;  Δ vs L/240;  R/(3.5" × 1.5") vs F_c⊥
import { Assembly, Building, DFL, ft, plf, standardSheets } from "topo-cad";

const span = ft(14);
const h = ft(8);
const spacing = 16 / 12; // ft

const frame = Assembly.named("Joist")
  .point("a", [0, 0, 0]).point("b", [0, 0, h])
  .point("c", [span, 0, 0]).point("d", [span, 0, h])
  .member("post", ["a", "b"], { size: [4, 4], grade: DFL.No2 })
  .member("post", ["c", "d"], { size: [4, 4], grade: DFL.No2 })
  .member("joist", ["b", "d"], { size: [2, 10], grade: DFL.No2, anchor: [0, -0.5], priority: 10 })
  .supportedAt("a", "c")
  .lineLoad("dead", "joist", plf(10 * spacing))
  .lineLoad("live", "joist", plf(40 * spacing));

export default Building.named("Benchmark 1: floor joist")
  .info({ notes: ["2x10 DF-L No.2 @ 16\" o.c., 14'-0\" span, D = 10 psf, L = 40 psf."] })
  .add(frame)
  .sheets(standardSheets("cover", "analytical", "schedules"));
