// Benchmark 6 — AWC Span Tables for Joists and Rafters (2024), Table F-2.
//
// Published: floor joists, 40 psf live + 10 psf dead, live-load deflection
// limit L/360. A 2x10 at 16" on centre with E = 1,600,000 psi spans 16'-5",
// requiring F_b = 1,255 psi (the table's F_b is the bending stress at that
// span; the tables include the joist's weight in the 10 psf dead load).
// Source: https://awc.org (AWC_STJR2024), Table F-2.
//
// Check: at 16'-5", f_b (D + L) ≈ 1,255 psi and Δ_L ≈ L/360. With DF-L No.2
// (F'_b = 900 × 1.1 × 1.15 = 1,139 psi < 1,255) the span is limited by
// bending, not deflection.
import { Assembly, Building, DFL, ftIn, plf, standardSheets } from "topo-cad";

const span = ftIn(16, 5);
const h = ftIn(8, 0);
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

export default Building.named("Benchmark 6: AWC span table")
  .info({ notes: ["AWC STJR 2024 Table F-2: 2x10 @ 16\" o.c., 16'-5\", 40 psf L + 10 psf D, L/360."] })
  // The table's 10 psf dead load already includes the joist.
  .designBasis({ selfWeight: false })
  .add(frame)
  .sheets(standardSheets("cover", "analytical", "schedules"));
