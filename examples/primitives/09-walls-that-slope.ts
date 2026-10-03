// Walls that slope, and a step: a perimeter is built from numbers of the one solve.
//
// A lean-to: the front wall 8 ft high, the back 10 ft; the side walls' tops
// slope between them (`heights(start, end)`). Where two walls meet at one
// height they are held at one height there. The east corner of the front
// sits 4" lower than the rest (`base`): the bottom plates of the walls that
// meet there slope to it (heights are above a wall's bottom).
//
// The perimeter's numbers are its corners (three each), each wall's height at
// its ends, and its studs. Where they are drawn is a hint. A reading on
// anything the walls are built with moves them: here the top of the right
// wall at its back end was surveyed half an inch higher than drawn. The
// right wall's end height goes up, and the back wall's start with it (they
// are held at one height there); the back wall's far end stays as drawn, so
// its top now slopes a little. The walls are built again at the numbers the
// solve gives, and solved again, until the numbers settle.
//
// (Walls that meet at different heights as drawn are not held at one: that
// corner is a step, and is said to be.)
//
// In the 3D view, turn on Skeleton: hover the right wall's cap plate end for
// the numbers it moves with and what settled each.
import { Building, DFL, ft, inch, Perimeter, survey, Wall } from "topo-cad";

const wall = Wall.template({ height: ft(8), grade: DFL.No2 });
const lo = ft(8);
const hi = ft(10);
const walls = Perimeter.start("Lean-to", [0, 0])
  .by([ft(12), 0], wall.named("Front").heights(lo, lo))
  .by([0, ft(10)], wall.named("Right").heights(lo, hi))
  .by([-ft(12), 0], wall.named("Back").heights(hi, hi))
  .close(wall.named("Left").heights(hi, lo))
  .base(2, -inch(4));

export default Building.named("Walls that slope").add(
  walls,
  survey(walls.node("Right/cap_plate", "end"), { z: hi + inch(0.5) }),
);
