// A small building from the slab up.
//
// Each thing stands on the one under it, and none is given a height:
//
// - a slab laid to fall 10 mm per metre toward the door in the west wall;
// - a curb on it whose top is level, 20 mm above zero: 20 mm tall at the east
//   wall, 57 mm at the west, stopping at the door;
// - the walls on the curb, 8 ft tall, so their tops are level too;
// - the trusses on the walls' plates, set out from a corner of the walls.
//
// Read the slab somewhere else and the curb changes; make the curb's level
// another and the walls and the roof go up or down with it.
import { Building, DFL, ft, inch, mm, Opening, Perimeter, Slab, TrussRoof, TrussShape, Wall } from "topo-cad";

const [wide, deep] = [ft(12), ft(16)];
const slab = Slab.named("Slab")
  .outline([[0, 0], [wide, 0], [wide, deep], [0, deep]])
  .sloped([mm(10), 0], [wide, 0]);
const wall = Wall.template({ height: ft(8), grade: DFL.No2 }).studs([2, 4], inch(16));
const door = Opening.door({ label: "Door", width: ft(8) }).header(2, 2, 12).flushHeader().at(deep / 2);
const walls = Perimeter.start("Walls", [0, 0])
  .on(slab.curbTo(mm(20)))
  .by([wide, 0], wall.named("South"))
  .by([0, deep], wall.named("East"))
  .by([-wide, 0], wall.named("North"))
  .close(wall.named("West").opening(door));

const roof = TrussRoof.of(TrussShape.fink({ span: wide, pitch: 4 / 12, overhang: inch(12) }), { at: walls.corner(2), spanDir: [-1, 0], length: deep, grade: DFL.No2 })
  .bearingOn(walls);

export default Building.named("From the slab up").add(slab, walls, roof);
