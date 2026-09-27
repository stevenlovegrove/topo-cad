// Benchmark 5 — roof load takedown to the foundation.
//
// A 12' × 16' building with Fink trusses at 24" on centre spanning 12'-0",
// 4/12 pitch, 12" overhangs. Roof dead load 15 psf and snow 25 psf, both on
// plan area. Where does the load go?
//
// Hand calculation (the test recomputes it independently):
//   Interior truss reaction per bearing ≈ (15 + 25) psf × 2' × (12'/2 + 1')
//                                        + half the truss self-weight = 560 lb + …
//   Gable-end trusses take half the tributary width.
//   Everything applied (roof, framing self-weight) reaches the foundation.
import { Building, DFL, ft, ftIn, inch, Perimeter, psf, standardSheets, TrussRoof, Wall } from "topo-cad";

const width = ft(12);
const depth = ft(16);
const wall = Wall.template({ height: ftIn(8, 1.125), grade: DFL.No2 });

const walls = Perimeter.start("Walls", [0, 0])
  .to([width, 0], wall.named("South wall"))
  .to([width, depth], wall.named("East wall"))
  .to([0, depth], wall.named("North wall"))
  .close(wall.named("West wall"));

// Trusses span east (spanDir) and repeat along the ridge, to the right of
// the span direction (here southward), starting from the origin.
const roof = TrussRoof.fink({
  origin: [0, depth],
  spanDir: [1, 0],
  span: width,
  length: depth,
  pitch: 4 / 12,
  spacing: inch(24),
  overhang: inch(12),
  grade: DFL.No2,
})
  .bearingOn(walls)
  .load("dead", psf(15))
  .load("snow", psf(25));

export default Building.named("Benchmark 5: roof takedown")
  .info({ notes: ["12' x 16', Fink trusses @ 24\" o.c., 4/12, 12\" overhangs; 15 psf D + 25 psf S on plan."] })
  .add(walls, roof)
  .sheets(standardSheets());
