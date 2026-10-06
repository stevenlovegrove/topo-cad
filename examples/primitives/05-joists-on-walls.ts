// What a generator makes, reached by its nodes.
//
// A perimeter of walls is made by its generator. Joists cross it, front to
// back, their undersides at the top of the walls. `bearingOn` is a utility:
// where each joist crosses a cap plate on plan, it makes a node on the
// joist's underside and on the plate's top, in the middle of where they
// touch, and fastens them there. The joist's axis is on its underside, so the
// node is in its path; the plate's axis is along one edge of its top (on the
// wall's line), so the node is on the plate's top face (two numbers, along
// and across). Were a joist not to touch a plate it crosses, that would be
// said, with how far off it is.
//
// The nodes of what the perimeter made can be named in relations too:
// `walls.node("Front/cap_plate", "start")` is the start of the front wall's
// cap plate.
//
// In the 3D view, turn on Skeleton: the joists' bearing points are orange
// diamonds; hover one for both members, the face, and its two numbers.
import { Assembly, Building, DFL, ft, inch, Perimeter, Wall } from "topo-cad";

const grade = DFL.No2;
const wall = Wall.template({ height: ft(8), grade });
export const walls = Perimeter.start("Walls", [0, 0])
  .by([ft(12), 0], wall.named("Front"))
  .by([0, ft(10)], wall.named("Right"))
  .by([-ft(12), 0], wall.named("Back"))
  .close(wall.named("Left"));

let joists = Assembly.named("Joists");
for (const [i, x] of [ft(3), ft(6), ft(9)].entries()) {
  joists = joists
    .point(`front ${i + 1}`, [x, -inch(6), ft(8)])
    .point(`back ${i + 1}`, [x, ft(10) + inch(6), ft(8)])
    .member("joist", [`front ${i + 1}`, `back ${i + 1}`], { size: [2, 8], grade, anchor: [0, -0.5] });
}

export default Building.named("Joists on walls").add(walls, joists.bearingOn(walls));
