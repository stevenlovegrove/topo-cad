// Walls standing straight on a slab laid to fall.
//
// Without a curb, a wall's bottom is the slab's top, wherever the wall is.
// The slab here rises 20 mm per metre along x (it is at zero at its west
// side). So:
//
// - the east wall stands 73 mm higher than the west (12 ft along). The slab
//   slopes across each of those walls' plates: a plate sits on the higher
//   edge of what is under it, not through it, so the west wall is 1.8 mm
//   above zero (its plate's inner edge is on the slab, its outer edge clear);
// - the south and north walls' bottom plates slope with the slab, and since a
//   wall's height is above its bottom, so do their tops: each wall is 8 ft
//   tall all along, on a floor that is not level;
// - the south wall's curb is of one height (`slab.curb(mm(40))`: its top
//   follows the slab, 40 mm above it all along), so that wall stands 40 mm
//   higher than the others where it meets them: a step at both its ends.
//
// (A curb whose top is level is `slab.curbTo(level)`: see the example before
// this one.)
import { Building, DFL, ft, inch, mm, Perimeter, Slab, Wall } from "topo-cad";

const [wide, deep] = [ft(12), ft(8)];
const slab = Slab.named("Slab")
  .outline([[0, 0], [wide, 0], [wide, deep], [0, deep]])
  .sloped([mm(20), 0]);
const wall = Wall.template({ height: ft(8), grade: DFL.No2 }).studs([2, 4], inch(16));
const walls = Perimeter.start("Walls", [0, 0])
  .on(slab)
  .by([wide, 0], wall.named("South").on(slab.curb(mm(40))))
  .by([0, deep], wall.named("East"))
  .by([-wide, 0], wall.named("North"))
  .close(wall.named("West"));

export default Building.named("Walls on a slab laid to fall").add(slab, walls);
