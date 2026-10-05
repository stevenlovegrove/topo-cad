// An opening in a floor, and stairs up through it.
//
// An opening is framed in the floor: a trimmer joist either side of it, a
// header across each end hung from the trimmers, and the joists it cuts
// stopping at the headers (tail joists), hung from them. The deck stops
// round it.
//
// The stairs are a flight of equal risers from the ground to the floor's
// top: 8 ft of wall, the joists' depth and the deck. That rise makes 14
// risers of a little over 7 1/2", so 13 treads, going 10 ft 10 in on plan.
// The flight climbs along the opening and its head, cut plumb, is at the far
// end of it, against the trimmer there; its feet are cut level, on the ground.
import { Building, DFL, Floor, ft, inch, Perimeter, psf, Stairs, Wall } from "topo-cad";

const grade = DFL.No2;
const [wide, deep] = [ft(12), ft(16)];
const wall = Wall.template({ height: ft(8), grade });
const walls = Perimeter.start("Walls", [0, 0])
  .by([wide, 0], wall.named("Front"))
  .by([0, deep], wall.named("Right"))
  .by([-wide, 0], wall.named("Back"))
  .close(wall.named("Left"));

// The joists run across the building (along x); the opening is 3 ft 6 in of their length, a foot in from the left
// wall, and 10 ft across them, from 4 ft back.
const well = { along: ft(1), across: ft(4), length: ft(3.5), width: ft(10) };
const floor = Floor.of({ joists: [2, 10], grade, spacing: inch(16) })
  .named("Floor")
  .from([0, 0], { along: [1, 0], length: wide, width: deep })
  .opening(well)
  .deck(inch(0.75))
  .bearingOn(walls)
  .load("dead", psf(10))
  .load("live", psf(40));

// The flight: up the middle of the opening, its head at the opening's far end.
const flight = Stairs.of({ rise: ft(8) + floor.top, width: inch(36), grade }).named("Stairs");
const stairs = flight.from([well.along + well.length / 2, well.across + well.width - flight.going, 0], { toward: [0, 1] });

export default Building.named("A stairwell and stairs").add(walls, floor, stairs);
