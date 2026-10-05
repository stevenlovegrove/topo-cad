// A saltbox: a roof of two pitches.
//
// The front of the roof is at 8 in 12 and the back at 4 in 12 (`edges`: an
// edge that differs from the rest). The ridge is not given, and is not in the
// middle: it is where the two planes meet, a third of the way back.
//
// Framed with rafters, each pair is a short steep rafter and a long shallow
// one, meeting there, their feet on the two plates at one height.
import { Building, DFL, ft, inch, Perimeter, pitch, psf, Roof, Wall } from "topo-cad";

const grade = DFL.No2;
const [wide, deep] = [ft(16), ft(12)];
const wall = Wall.template({ height: ft(8), grade });
const walls = Perimeter.start("Walls", [0, 0])
  .by([wide, 0], wall.named("Front"))
  .by([0, deep], wall.named("Right"))
  .by([-wide, 0], wall.named("Back"))
  .close(wall.named("Left"));

const roof = Roof.named("Roof")
  .over(walls, { pitch: pitch(8), edges: { Back: { pitch: pitch(4) } }, gables: ["Left", "Right"], rafters: { size: [2, 8], grade, spacing: inch(24) } })
  .load("dead", psf(12))
  .load("snow", psf(25));

export default Building.named("A saltbox").add(walls, roof);
