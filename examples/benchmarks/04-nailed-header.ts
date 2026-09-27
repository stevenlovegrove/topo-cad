// Benchmark 4 — a header hung on nails (no jack studs).
//
// A (2) 2x8 DF-L No.2 header spans 6'-0" between two 4x4 posts, held only by
// (4) 16d common nails end-nailed through each post into the header's end
// grain — the as-built condition of many garage headers. It carries 100 plf
// dead + 150 plf snow. Is the nailing adequate?
//
// Hand calculation (the test recomputes it independently):
//   R = wL/2 per end (+ self-weight)
//   Z (16d common, 1.5" side member, G = 0.50) by the NDS yield-limit
//   equations; mode IV governs (≈ 141 lb, NDS Table 12N)
//   Z' = Z · C_D · C_eg = Z · 1.15 · 0.67  (end grain);  capacity = 4 Z'
import { Assembly, Building, DFL, ft, plf, schedule, sheet, standardSheets, utilization } from "topo-cad";

const span = ft(6);
const h = ft(7);

const opening = Assembly.named("Opening")
  .point("a", [0, 0, 0]).point("b", [0, 0, h]).point("m1", [0, 0, h - ft(0.5)])
  .point("c", [span, 0, 0]).point("d", [span, 0, h]).point("m2", [span, 0, h - ft(0.5)])
  .member("post", ["a", "m1", "b"], { size: [4, 4], grade: DFL.No2, priority: 10 })
  .member("post", ["c", "m2", "d"], { size: [4, 4], grade: DFL.No2, priority: 10 })
  .member("header", ["m1", "m2"], { size: [2, 8], plies: 2, grade: DFL.No2 })
  .joints({ nails: 4, penny: 16, method: "end" })
  .supportedAt("a", "c")
  .lineLoad("dead", "header", plf(100))
  .lineLoad("snow", "header", plf(150));

export default Building.named("Benchmark 4: header on nails only")
  .info({ notes: ["(2) 2x8 DF-L No.2 header, 6'-0\" span, (4) 16d end nails per end, no jack studs; 100 plf D + 150 plf S."] })
  .add(opening)
  .sheets(
    standardSheets("cover"),
    sheet("S-601", "Structural checks", utilization(opening, { from: "south", scale: '1"' }), schedule("checks"), schedule("reactions")),
    standardSheets("analytical", "schedules"),
  );
