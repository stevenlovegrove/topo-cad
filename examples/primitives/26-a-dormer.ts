// A dormer.
//
// A gable roof over the walls, and on its front slope a small part of its
// own: a dormer, 6 ft wide, its eaves 11 ft up, its ridge running back into the
// slope. It is another part of the same roof, and where it is the higher of
// the two it is the roof: its ridge stops where it meets the slope, and the
// valleys either side are where its planes cross the main roof's.
//
// Nothing is under a dormer's eaves, so their height is given. Its front is
// a gable.
//
// (The surfaces only: the dormer's walls and framing are not drawn.)
import { Building, DFL, ft, inch, Perimeter, pitch, Roof, Wall } from "topo-cad";

const [wide, deep] = [ft(24), ft(20)];
const wall = Wall.template({ height: ft(8), grade: DFL.No2 });
const walls = Perimeter.start("Walls", [0, 0])
  .by([wide, 0], wall.named("Front"))
  .by([0, deep], wall.named("Right"))
  .by([-wide, 0], wall.named("Back"))
  .close(wall.named("Left"));

const roof = Roof.named("Roof")
  .over(walls, { name: "Main", pitch: pitch(8), overhang: inch(12), rake: inch(8), gables: ["Left", "Right"] })
  .part("Dormer", [[ft(9), ft(2)], [ft(15), ft(2)], [ft(15), ft(10)], [ft(9), ft(10)]], { pitch: pitch(6), overhang: inch(6), rake: inch(6), gables: [1, 3], height: ft(11) });

export default Building.named("A dormer").add(walls, roof);
