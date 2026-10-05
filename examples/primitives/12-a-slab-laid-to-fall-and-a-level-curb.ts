// A slab laid to fall, and a curb that makes a level seat on it.
//
// A garage slab is laid to fall toward the door, so that water runs out. The
// walls' plates cannot follow it: they sit on a curb whose top is level. Here
// the slab's top rises 10 mm per metre along x (it falls toward the door in
// the west wall); it is at zero at its east side, where its height is given.
//
// - `slab.sloped([mm(10), 0], about)`: the slab's top is a plane.
// - `slab.curbTo(mm(20))`: a curb whose top is level, 20 mm above zero. Every
//   wall stands on it, so every wall's bottom is at 20 mm, and the walls are
//   as they would be on a level floor. Under them the curb is as tall as the
//   slab is below that level there: 20 mm at the east wall, 57 mm at the west.
//   It stops at the door, as the bottom plate does.
//
// The slab's height is still a number of the one solve. The last line reads
// the slab 5 mm lower than drawn at its east side: the slab moves down, all
// of it, and the curb is 5 mm taller everywhere; the walls do not move, since
// what they stand on is the curb's top, and that is the level it is given.
import { Building, DFL, ft, inch, mm, Opening, Perimeter, Slab, survey, Wall } from "topo-cad";

const [wide, deep] = [ft(12), ft(8)];
const slab = Slab.named("Slab")
  .outline([[0, 0], [wide, 0], [wide, deep], [0, deep]])
  .sloped([mm(10), 0], [wide, 0])
  .top(0);
const wall = Wall.template({ height: ft(8), grade: DFL.No2 }).studs([2, 4], inch(16));
const door = Opening.door({ label: "Door", width: ft(3) }).header(2, 2, 8).flushHeader().at(deep / 2);
const walls = Perimeter.start("Walls", [0, 0])
  .on(slab.curbTo(mm(20)))
  .by([wide, 0], wall.named("South"))
  .by([0, deep], wall.named("East"))
  .by([-wide, 0], wall.named("North"))
  .close(wall.named("West").opening(door));

export default Building.named("A slab laid to fall and a level curb").add(
  slab,
  walls,
  survey(slab.at([wide, deep / 2]), { z: -mm(5) }),
);
