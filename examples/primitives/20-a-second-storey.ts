// A second storey: walls standing on a floor.
//
// A storey is stacking, not a number. The ground-floor walls stand at zero;
// the floor sits on their plates, with a 3/4" deck on it; the upper walls
// stand on the floor (`.on(floor)`): each of their corners is as high as the
// floor's top is under it. The roof sits on the upper walls.
//
// Nothing above the ground floor is given a height. The upper walls are drawn
// at zero like the lower ones, the floor too: that is only where the solve
// starts. Make the lower walls taller, or the joists deeper, and everything
// above goes up with them.
import { Building, DFL, Floor, ft, inch, Perimeter, psf, TrussRoof, TrussShape, Wall } from "topo-cad";

const grade = DFL.No2;
const [wide, deep] = [ft(12), ft(16)];
const wall = Wall.template({ height: ft(8), grade });
const box = (name: string) =>
  Perimeter.start(name, [0, 0])
    .by([wide, 0], wall.named("Front"))
    .by([0, deep], wall.named("Right"))
    .by([-wide, 0], wall.named("Back"))
    .close(wall.named("Left"));

const ground = box("Ground floor walls");
const floor = Floor.of({ joists: [2, 10], grade, spacing: inch(16) })
  .named("Floor")
  .from([0, 0], { along: [1, 0], length: wide, width: deep })
  .deck(inch(0.75))
  .bearingOn(ground)
  .load("dead", psf(10))
  .load("live", psf(40));
const upper = box("Upper floor walls").on(floor);

const roof = TrussRoof.of(TrussShape.fink({ span: wide, pitch: 6 / 12, overhang: inch(12) }), { at: upper.corner(2), spanDir: [-1, 0], length: deep, grade })
  .bearingOn(upper);

export default Building.named("A second storey").add(ground, floor, upper, roof);
