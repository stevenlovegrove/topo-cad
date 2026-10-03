// Studs are numbers of their wall.
//
// Each wall's common studs are numbers of the wall: where the layout puts
// them (16" on centre here) is a hint; studs given with `studsAt` are
// readings; a stud against the wall it meets at an end is fixed. They keep
// their order (no two overlap) and stay in the wall. Their ends are nodes:
// `walls.stud("Front", 3, "bottom")`.
//
// Readings on stud ends move them, and the wall is built with its studs
// there:
//
// - the front wall's 3rd stud taped at 15" from its 2nd (the layout has 16"):
//   both are hints, so each gives way half an inch;
// - its 4th stud surveyed 5 ft along the front wall (the layout has 4 ft).
//
// In the 3D view, turn on Skeleton: the solve "Studs of Walls" has a node at
// each end of each stud; hover one for its number (where along the wall),
// what it is kept between, and what settled it.
import { Building, DFL, ft, inch, Perimeter, survey, tape, Wall } from "topo-cad";

const wall = Wall.template({ height: ft(8), grade: DFL.No2 }).studs([2, 4], inch(16));
const walls = Perimeter.start("Walls", [0, 0])
  .by([ft(12), 0], wall.named("Front"))
  .by([0, ft(8)], wall.named("Right"))
  .by([-ft(12), 0], wall.named("Back"))
  .close(wall.named("Left"));

export default Building.named("Studs on readings").add(
  walls,
  tape(walls.stud("Front", 2, "bottom"), walls.stud("Front", 3, "bottom"), inch(15)),
  survey(walls.stud("Front", 4, "bottom"), { x: ft(5) }),
);
