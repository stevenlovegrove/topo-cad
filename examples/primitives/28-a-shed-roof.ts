// A shed roof.
//
// One edge of the roof is an eave (the front), the others are not: so it is
// one plane, rising from the front wall to the back at 3 in 12. Framed with
// trusses, each is a mono truss: its heel on the front wall, its high end over
// the back wall, its bottom chord level on both plates.
//
// The roof's plane is the top of the trusses' top chords, and runs a foot out
// past the front wall and 6" past the other three.
import { Building, DFL, ft, inch, Perimeter, pitch, psf, Roof, Wall } from "topo-cad";

const grade = DFL.No2;
const [wide, deep] = [ft(16), ft(10)];
const wall = Wall.template({ height: ft(8), grade });
const walls = Perimeter.start("Walls", [0, 0])
  .by([wide, 0], wall.named("Front"))
  .by([0, deep], wall.named("Right"))
  .by([-wide, 0], wall.named("Back"))
  .close(wall.named("Left"));

const roof = Roof.named("Roof")
  .over(walls, { pitch: pitch(3), overhang: inch(12), rake: inch(6), gables: ["Right", "Back", "Left"], trusses: { grade } })
  .load("snow", psf(25));

export default Building.named("A shed roof").add(walls, roof);
