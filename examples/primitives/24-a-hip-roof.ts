// A hip roof.
//
// A roof is given as its surfaces, and those by its edges: a roof over these
// walls, at 6 in 12, running a foot out past them. No edge is said to be a
// gable, so every edge is an eave: a plane rises inward from each, and the roof
// is the lowest of them. The ridge and the four hips are not given: they are
// where the planes meet.
//
// Each eave is as high as the plate of the wall under it. Say one wall is a
// gable (`gables: ["Left"]`) and that end is a gable and the other still a
// hip; give an edge its own pitch (`edges: { Back: { pitch: pitch(3) } }`) and
// the ridge moves over.
//
// (The surfaces only: hips are not framed yet.)
import { Building, DFL, ft, inch, Perimeter, pitch, Roof, Wall } from "topo-cad";

const [wide, deep] = [ft(24), ft(16)];
const wall = Wall.template({ height: ft(8), grade: DFL.No2 });
const walls = Perimeter.start("Walls", [0, 0])
  .by([wide, 0], wall.named("Front"))
  .by([0, deep], wall.named("Right"))
  .by([-wide, 0], wall.named("Back"))
  .close(wall.named("Left"));

const roof = Roof.named("Roof").over(walls, { pitch: pitch(6), overhang: inch(12) });

export default Building.named("A hip roof").add(walls, roof);
