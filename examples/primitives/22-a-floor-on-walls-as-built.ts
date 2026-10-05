// A floor on walls as they are built: each joist sits for itself.
//
// A floor that is one rigid thing sits on walls whose tops are in one plane.
// Walls as built are not: here the back wall was surveyed 5 mm taller at one
// end than at the other, the front wall level, so the four tops are a twist,
// and no plane lies on them. `floor.loose()` is the same floor with nothing
// joining its joists: each joist, each band joist and each rim is a thing of
// its own, sat on what is under it, as each truss of a roof is. Each joist
// then slopes a little differently from the next, front to back, as the real
// ones do.
//
// (The same model with `floor` in place of `floor.loose()` is refused: the
// floor cannot sit on all four walls, and it says how far off each joist is.
// The pieces are still rigid: on a twist much steeper than this one a square
// joist digs a corner a fraction of a millimetre into a plate that slopes
// under it, where a real one would crush or be shimmed, and that is said as
// a clash.)
import { Building, DFL, Floor, ft, inch, mm, Perimeter, psf, survey, Wall } from "topo-cad";

const grade = DFL.No2;
const [wide, deep] = [ft(12), ft(10)];
const wall = Wall.template({ height: ft(8), grade });
export const walls = Perimeter.start("Walls", [0, 0])
  .by([wide, 0], wall.named("Front"))
  .by([0, deep], wall.named("Right"))
  .by([-wide, 0], wall.named("Back"))
  .close(wall.named("Left"));

export const floor = Floor.of({ joists: [2, 10], grade, spacing: inch(16) })
  .named("Floor")
  .from([wide, 0], { along: [0, 1], length: deep, width: wide })
  .bearingOn(walls)
  .load("dead", psf(10))
  .load("live", psf(40));

export default Building.named("A floor on walls as built").add(
  walls,
  // The back wall's cap plate, surveyed: 5 mm high where it starts (at the right wall), as drawn where it ends.
  survey(walls.node("Back/cap_plate", "start"), { z: ft(8) + mm(5) }),
  survey(walls.node("Back/cap_plate", "end"), { z: ft(8) }),
  floor.loose(),
);
