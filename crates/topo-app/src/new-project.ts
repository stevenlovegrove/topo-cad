// NEW PROJECT
//
// A topo-cad model: plain TypeScript, the source of truth for the drawings.
// Dimensions are defined once and reused; everything below is ordinary code.
//
// Getting started (or ask your LLM helper to):
//   1. Trace the footprint as a path (plan coordinates; +x east, +y north).
//   2. Give each side a wall (height, stud size and spacing, openings).
//   3. Add the roof (trusses or rafters) bearing on the walls.
//   4. Add field measurements with the Distance / Member length tools, and
//      turn uncertain dimensions into `unknown(...)`s to fit them.
import { Building, DFL, ft, ftIn, inch, Opening, Perimeter, psf, standardSheets, TrussRoof, Wall } from "topo-cad";

// ----- dimensions -------------------------------------------------------------
const width = ft(12);    // east–west
const depth = ft(16);    // north–south
const wallHeight = ftIn(8, 1.125);

// ----- walls --------------------------------------------------------------------
const wall = Wall.template({ height: wallHeight, grade: DFL.No2 });
const door = Opening.door({ label: "D1", width: inch(38), height: inch(82.5) });
const window = Opening.window({ label: "W1", width: ft(3), height: ft(3), head: inch(82.5) });

const walls = Perimeter.start("Walls", [0, 0])
  .to([width, 0], wall.named("South wall").opening(door.at(width / 2)))
  .to([width, depth], wall.named("East wall"))
  .to([0, depth], wall.named("North wall").opening(window.at(width / 2)))
  .close(wall.named("West wall"));

// ----- roof ---------------------------------------------------------------------
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
  .load("roof_live", psf(20));

export default Building.named("NEW PROJECT")
  .info({ designer: "", date: "", notes: [] })
  .add(walls, roof)
  .sheets(standardSheets());
