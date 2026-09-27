// Benchmark 3 — an axially loaded post (column stability).
//
// A 4x4 DF-L No.2 post, 8'-0" tall, pinned top and bottom (K_e = 1.0), not
// braced along its height, carries 2,000 lb dead + 3,000 lb live at its top.
//
// Hand calculation (the test recomputes it independently):
//   l_e/d = 96" / 3.5" = 27.4
//   F_c* = F_c C_D C_F = 1350 × 1.0 × 1.15;   F_cE = 0.822 E_min / (l_e/d)²
//   C_P = (1 + F_cE/F_c*)/2c − √[((1 + F_cE/F_c*)/2c)² − (F_cE/F_c*)/c],  c = 0.8
//   f_c = P/A = 5,000 lb / 12.25 in² vs F'_c = F_c* C_P
import { Assembly, Building, DFL, ft, lbf, standardSheets } from "topo-cad";

const post = Assembly.named("Post")
  .point("base", [0, 0, 0]).point("top", [0, 0, ft(8)])
  .member("post", ["base", "top"], { size: [4, 4], grade: DFL.No2 })
  .supportedAt("base")
  .pointLoad("dead", "top", lbf(2000))
  .pointLoad("live", "top", lbf(3000));

export default Building.named("Benchmark 3: post in compression")
  .info({ notes: ["4x4 DF-L No.2 post, 8'-0\", P = 2,000 lb D + 3,000 lb L."] })
  .add(post)
  .sheets(standardSheets("cover", "analytical", "schedules"));
