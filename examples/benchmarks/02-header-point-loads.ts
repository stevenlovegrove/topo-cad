// Benchmark 2 — a garage-door header carrying truss point loads.
//
// A (2) 2x12 DF-L No.2 header spans 16'-0" (centre to centre of bearing) on
// 4x4 posts. Roof trusses at 24" on centre land on it at 2', 4', … 14', each
// bringing 400 lb dead and 500 lb snow. Check the header (NDS, ASD).
//
// Hand calculation (the test recomputes it independently):
//   Seven equal loads P, symmetric:  R = 7P/2 (+ self-weight)
//   Midspan moment:  M = R·8' − P(6' + 4' + 2') = 16 P·ft  (+ wL²/8 self-weight)
//   D + S:  P = 900 lb  →  M ≈ 14,400 lb·ft,  R ≈ 3,150 lb;  C_D = 1.15, C_F(2x12) = 1.0
import { Assembly, Building, DFL, ft, lbf, standardSheets } from "topo-cad";

const span = ft(16);
const h = ft(7);
const loads = [2, 4, 6, 8, 10, 12, 14];

let header = Assembly.named("Header")
  .point("a", [0, 0, 0]).point("b", [0, 0, h])
  .point("c", [span, 0, 0]).point("d", [span, 0, h]);
for (const x of loads) header = header.point(`t${x}`, [ft(x), 0, h]);
header = header
  .member("post", ["a", "b"], { size: [4, 4], grade: DFL.No2 })
  .member("post", ["c", "d"], { size: [4, 4], grade: DFL.No2 })
  .member("header", ["b", ...loads.map((x) => `t${x}`), "d"], { size: [2, 12], plies: 2, grade: DFL.No2, anchor: [0, -0.5], priority: 10 })
  .supportedAt("a", "c");
for (const x of loads) header = header.pointLoad("dead", `t${x}`, lbf(400)).pointLoad("snow", `t${x}`, lbf(500));

export default Building.named("Benchmark 2: header with point loads")
  .info({ notes: ["(2) 2x12 DF-L No.2 header, 16'-0\" span, trusses @ 24\" o.c.: 400 lb D + 500 lb S each."] })
  .add(header)
  .sheets(standardSheets("cover", "analytical", "schedules"));
