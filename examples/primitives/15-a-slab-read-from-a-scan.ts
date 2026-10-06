// A slab where a scan has it.
//
// The slab is drawn at zero: only where the solve starts. A scan saw the
// floor at three places. The scan has its own frame (it is placed in the
// model: turned 20 degrees, its origin 0.4 m above zero), and each reading is
// a point of that frame: "the floor is on the plane through this point,
// facing up", made one with a point of the slab's top, to 3 mm.
//
// The three readings put the floor 11, 12 and 13 mm below where it is drawn.
// The slab's top is one number: it goes to 12 mm down, between them, and each
// reading says how far off it is left. The walls stand on the slab, so they
// go with it.
//
// (Readings like these are what dragging a face onto a scan makes. Moving the
// scan's frame, registering it anew, moves them, and the slab with them.)
import { Building, coincide, DFL, Frame, ft, inch, mm, Perimeter, Placement, Slab, Wall } from "topo-cad";

const [wide, deep] = [ft(12), ft(8)];
const slab = Slab.named("Slab").outline([[0, 0], [wide, 0], [wide, deep], [0, deep]]);
const wall = Wall.template({ height: ft(8), grade: DFL.No2 }).studs([2, 4], inch(16));
const walls = Perimeter.start("Walls", [0, 0])
  .on(slab)
  .by([wide, 0], wall.named("Front"))
  .by([0, deep], wall.named("Right"))
  .by([-wide, 0], wall.named("Back"))
  .close(wall.named("Left"));

const scan = Frame.named("Scan").placed(Placement.at([1, 0.5, 0.4], 20));
// Where the scan saw the floor, in the scan's own coordinates (its z is 0.4 m above the model's zero).
const seen: [number, number, number][] = [
  [0.5, 0.4, -0.4 - mm(11)],
  [1.8, 0.2, -0.4 - mm(12)],
  [1.1, 1.3, -0.4 - mm(13)],
];

export default Building.named("A slab read from a scan").add(
  slab,
  walls,
  scan,
  seen.map((p, k) => coincide(scan.on(p, [0, 0, 1]), slab.at([ft(3) * (k + 1), ft(4)]), { tolerance: mm(3) })),
);
