// What walls stand on is a thing.
//
// A wall is not given a height to start at: it stands on something, and its
// bottom is that thing's top. Here a slab, its top 40 mm below zero, and a
// 40 mm curb on it:
//
// - three walls stand on the curb (what the perimeter stands on);
// - the right wall stands on the slab itself (it says so). It is as tall as
//   the others, so its top is 40 mm lower: it meets them at a step at both
//   ends, top and bottom.
//
// The slab's top is a number of the one solve: where it is drawn is a hint.
// The last line is a reading of it (a level put on the floor, say): 45 mm
// below zero, not 40. The slab moves down to it, and everything standing on
// it moves with it.
import { Building, DFL, ft, inch, mm, Perimeter, Slab, survey, Wall } from "topo-cad";

const slab = Slab.named("Slab")
  .outline([[0, 0], [ft(12), 0], [ft(12), ft(8)], [0, ft(8)]])
  .top(-mm(40));
const wall = Wall.template({ height: ft(8), grade: DFL.No2 }).studs([2, 4], inch(16));
const walls = Perimeter.start("Walls", [0, 0])
  .on(slab.curb(mm(40)))
  .by([ft(12), 0], wall.named("Front"))
  .by([0, ft(8)], wall.named("Right").on(slab))
  .by([-ft(12), 0], wall.named("Back"))
  .close(wall.named("Left"));

export default Building.named("Walls on a slab and a curb").add(
  slab,
  walls,
  survey(slab.at([ft(6), ft(4)]), { z: -mm(45) }),
);
