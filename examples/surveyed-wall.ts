// A wall framed exactly as measured, instead of by layout rules.
//
// Each member is an absolute laser reading from an explicit datum (so errors
// don't accumulate), to the face nearer the datum unless `hit` says
// otherwise. Corners are named by compass direction, and a datum that sits
// on something the model doesn't frame (drywall) says so with an offset.
import { Building, Datum, DFL, ft, inch, Level, Perimeter, standardSheets, Survey, Wall } from "topo-cad";

const wall = Wall.template({ height: ft(8), grade: DFL.No2 }).studs([2, 6], inch(16));

// The west wall, walking north from the south corner, then the rest of it
// from the north corner, which has drywall on it.
const west = Survey.from(Datum.corner("south"))
  .stud(0) // tight to the south wall's framing
  .stud(inch(15.25))
  // A window whose header is nailed to its kings (no jacks), as found.
  .king(inch(26.5))
  .window("W1", { head: inch(82.5), sill: inch(46.5), header: [2, 2, 8] })
  .king(inch(77.5))
  .stud(inch(93.25))
  // A door with jacks, and a cripple over the header.
  .king(inch(104)).jack(inch(105.5))
  .door("D1", { head: Level.slab.at(inch(82.5)), header: [2, 2, 8] })
  .cripple(inch(122))
  .jack(inch(141.25)).king(inch(142.75))
  .from(Datum.corner("north").offset(inch(0.5), "1/2\" drywall on the north wall"))
  .stud(0)
  .stud(inch(15))
  .stud(inch(31))
  // Tape, corner to corner inside, as a check on the footprint and readings.
  .check(Datum.corner("south"), Datum.corner("north"), ft(16) - inch(11));

const p = Perimeter.start("Shed", [0, 0])
  .by([ft(12), 0], wall.named("South"))
  .by([0, ft(16)], wall.named("East"))
  .by([-ft(12), 0], wall.named("North"))
  .close(wall.named("West").surveyed(west));

export default Building.named("Surveyed wall example").add(p).sheets(standardSheets("walls"));
