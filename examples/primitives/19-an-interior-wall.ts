// An interior wall: a wall that meets other walls part way along them.
//
// A perimeter is a run of walls that turn corners. A wall that meets another
// part way along it is a partition: it runs from a place on one wall
// (`walls.along("Front", ft(12))`: 12 ft along the front wall) to a place on
// another, and tees into each. There, that wall's cap plate is broken and the
// partition's laps over it; its lower plates end on that wall's; its studs
// keep clear of it. It is as tall as the walls it meets, and is built between
// them as they are, so it goes where they go.
//
// The middle wall here is a bearing wall: the floor's joists run the length
// of the building and sit on the left wall, on it, and on the right wall. A second
// partition runs from the middle wall to the right wall, with a door in it,
// and one more stops short, at a point, where it ends free.
import { Building, DFL, Floor, ft, inch, Opening, Partition, Perimeter, psf, Wall } from "topo-cad";

const grade = DFL.No2;
const [long, deep] = [ft(24), ft(16)];
const wall = Wall.template({ height: ft(8), grade });
const walls = Perimeter.start("Walls", [0, 0])
  .by([long, 0], wall.named("Front"))
  .by([0, deep], wall.named("Right"))
  .by([-long, 0], wall.named("Back"))
  .close(wall.named("Left"));

// (The back wall runs from right to left, so 12 ft from its start is the building's middle too.)
const middle = Partition.between(walls.along("Front", ft(12)), walls.along("Back", ft(12)), wall.named("Middle"));
const door = Opening.door({ label: "Door", width: inch(32), height: inch(82) }).at(ft(5));
const cross = Partition.between(middle.along(ft(9)), walls.along("Right", ft(9)), wall.named("Cross").opening(door));
const stub = Partition.between(walls.along("Left", ft(8)), [ft(5), ft(8)], wall.named("Stub"));

const floor = Floor.of({ joists: [2, 10], grade, spacing: inch(16) })
  .named("Floor")
  .from([0, 0], { along: [1, 0], length: long, width: deep })
  .bearingOn(walls, middle)
  .load("dead", psf(10))
  .load("live", psf(40));

export default Building.named("An interior wall").add(walls, middle, cross, stub, floor);
