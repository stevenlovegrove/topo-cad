// Existing detached garage, modelled as built. The 16' garage door header has
// no jack studs: it hangs off the king studs on nails, and roof trusses bear
// on the wall above it.
import { Building, DFL, ft, ftIn, inch, Opening, Perimeter, psf, TrussRoof, Wall } from "topo-cad";

// Dimensions, defined once.
const width = ft(24);
const depth = ft(22);
const wallHeight = ftIn(8, 1.125);

// A template wall; every wall below is a modified copy of it.
const wall = Wall.template({ height: wallHeight, grade: DFL.No2 });

const garageDoor = Opening.door({ label: "GD", width: ft(16) })
  .header(2, 2, 10)
  .flushHeader()
  .jacks(0); // as built: no jack studs

const manDoor = Opening.door({ label: "D1", width: inch(38), height: inch(82.5) });
const window = Opening.window({ label: "W1", width: ft(3), height: ft(3), head: inch(82.5) });

const walls = Perimeter.start("Walls", [0, 0])
  .to([width, 0], wall.named("Wall A (Front)").opening(garageDoor.at(width / 2)))
  .to([width, depth], wall.named("Wall B (East)").opening(manDoor.at(ft(17))))
  .to([0, depth], wall.named("Wall C (Back)").opening(window.at(width / 2)))
  .close(wall.named("Wall D (West)"));

const roof = TrussRoof.fink({
  origin: [0, 0],
  spanDir: [0, 1],
  span: depth,
  length: width,
  pitch: 6 / 12,
  spacing: inch(24),
  overhang: inch(12),
  grade: DFL.No2,
})
  .bearingOn(walls)
  .load("dead", psf(20))
  .load("snow", psf(25));

export default Building.named("Garage (existing)")
  .info({
    number: "TC-0002",
    client: "Homeowner",
    designer: "topo-cad",
    date: "2026-09-26",
    design_basis: [
      "EXISTING CONDITIONS as observed; member sizes and grades assumed where not visible",
      "Wood design: ANSI/AWC NDS-2018 (ASD)",
      "Loads: ASCE 7-16. Roof TCDL 10 psf, BCDL 10 psf, snow 25 psf (assumed)",
      "Lumber: DF-L No.2 assumed",
    ],
    notes: [
      'Observed: top plate deflects approx. 1/8" relative to the garage door header.',
      "Garage door header has no jack studs; header is end-nailed to king studs.",
    ],
  })
  .add(walls, roof);
