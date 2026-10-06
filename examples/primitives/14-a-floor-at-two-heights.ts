// A floor at two heights: two slabs, a step between them, and walls on each.
//
// A floor that steps is two slabs. Here the front of a room is a slab 50 mm
// below zero and the back, from 10 ft in, a slab at zero. Each wall stands on
// the slab under it; the side walls are each two walls, one either side of
// the step, since a wall stands on one thing.
//
// The walls on the lower slab are 50 mm taller, so every wall's top is at one
// height: where a wall on the lower slab meets one on the upper they are held
// at one height at their tops, and step only at their bottoms.
import { Building, DFL, ft, inch, mm, Perimeter, Slab, Wall } from "topo-cad";

const [wide, deep, step, drop] = [ft(12), ft(16), ft(10), mm(50)];
const lower = Slab.named("Lower slab").outline([[0, 0], [wide, 0], [wide, step], [0, step]]).top(-drop);
const upper = Slab.named("Upper slab").outline([[0, step], [wide, step], [wide, deep], [0, deep]]).top(0);

const wall = Wall.template({ height: ft(8), grade: DFL.No2 }).studs([2, 4], inch(16));
const low = wall.height(ft(8) + drop).on(lower);
const walls = Perimeter.start("Walls", [0, 0])
  .on(upper)
  .to([wide, 0], low.named("Front"))
  .to([wide, step], low.named("Right (front)"))
  .to([wide, deep], wall.named("Right (back)"))
  .to([0, deep], wall.named("Back"))
  .to([0, step], wall.named("Left (back)"))
  .close(low.named("Left (front)"));

export default Building.named("A floor at two heights").add(lower, upper, walls);
