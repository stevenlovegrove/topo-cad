// A floor sits on what is under it.
//
// A floor platform is rim joists across the ends of its joists, a band joist
// along each side, and common joists between at a spacing. Its rectangle is
// given on plan. Its height is not: `bearingOn(walls)` rests each joist on
// each cap plate it crosses, and the floor is free, so the solve sits it on
// them (it is drawn at zero, which is only where the solve starts).
//
// Here the joists run front to back over a 12 ft by 10 ft room; the walls
// are 8 ft tall, so the underside of the joists is at 8 ft. On plan it stays
// where it is drawn (a hint: a relation to one of its corners,
// `floor.node("corner 1")`, would move it).
//
// It is one rigid thing: were the back wall half an inch taller, it would
// sit tilted on the walls as they are; were the walls' tops not in one
// plane, it could not sit on them all, and that is said.
//
// In the 3D view, turn on Skeleton: each joist's bearing points are orange
// diamonds on the plates' tops.
import { Building, DFL, Floor, ft, inch, Perimeter, psf, Wall } from "topo-cad";

const grade = DFL.No2;
const wall = Wall.template({ height: ft(8), grade });
export const walls = Perimeter.start("Walls", [0, 0])
  .by([ft(12), 0], wall.named("Front"))
  .by([0, ft(10)], wall.named("Right"))
  .by([-ft(12), 0], wall.named("Back"))
  .close(wall.named("Left"));

export const floor = Floor.of({ joists: [2, 10], grade, spacing: inch(16) })
  .named("Floor")
  .from([ft(12), 0], { along: [0, 1], length: ft(10), width: ft(12) })
  .bearingOn(walls)
  .load("dead", psf(10))
  .load("live", psf(40));

export default Building.named("A floor on walls").add(walls, floor);
