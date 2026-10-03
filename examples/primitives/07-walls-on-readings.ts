// Walls built on readings: corners are nodes, placed by what was measured.
//
// The walls are drawn as designed: 20 ft by 12 ft. Their corners are nodes
// (`walls.corner(1)` is the start, then each corner in turn), and where a
// corner is drawn is only a hint. Readings say where they are as built:
//
// - corner 1 surveyed at the origin;
// - the front wall taped at 20' 1" (corner 1 to corner 2);
// - corner 3 surveyed 3/4" further north than drawn.
//
// Readings are measured: they give way to anything that must hold, and the
// solve says by how much each is off. The walls are then built on the
// corners where they are: the front wall 20' 1" long, the right wall
// slightly out of square.
//
// In the 3D view, turn on Skeleton: the corners are the solve's nodes (in
// "Corners of the walls"); hover one for how it moved and what settled it.
import { Building, DFL, ft, inch, Perimeter, survey, tape, Wall } from "topo-cad";

const wall = Wall.template({ height: ft(8), grade: DFL.No2 });
const walls = Perimeter.start("Walls", [0, 0])
  .by([ft(20), 0], wall.named("Front"))
  .by([0, ft(12)], wall.named("Right"))
  .by([-ft(20), 0], wall.named("Back"))
  .close(wall.named("Left"));

export default Building.named("Walls on readings").add(
  walls,
  survey(walls.corner(1), { x: 0, y: 0 }),
  tape(walls.corner(1), walls.corner(2), ft(20) + inch(1)),
  survey(walls.corner(3), { y: ft(12) + inch(0.75) }),
);
