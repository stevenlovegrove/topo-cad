// A roof that turns a corner.
//
// The building is a long bar with a wing out of its front. Its roof is two
// parts, each a plain gable roof over a rectangle: one along the bar, one along
// the wing and on into the bar as far as its ridge. Where two parts overlap,
// the higher is the roof: so the wing's ridge runs back until it meets the
// main roof's slope, and the two valleys from there down to the inside corners
// are where the parts' planes cross. Neither is given.
//
// The wing is narrower than the bar, so its ridge is lower. Each eave is as
// high as the plate of the wall under it; where the main roof's front eave
// crosses the wing there is no wall, and it is as high as the rest.
//
// (The surfaces only: valleys are not framed yet.)
import { Building, DFL, ft, inch, Perimeter, pitch, Roof, Wall } from "topo-cad";

const wall = Wall.template({ height: ft(8), grade: DFL.No2 });
const walls = Perimeter.start("Walls", [0, 0])
  .by([ft(6), 0], wall.named("Front left"))
  .by([0, -ft(10)], wall.named("Wing left"))
  .by([ft(12), 0], wall.named("Wing front"))
  .by([0, ft(10)], wall.named("Wing right"))
  .by([ft(12), 0], wall.named("Front right"))
  .by([0, ft(16)], wall.named("Right"))
  .by([-ft(30), 0], wall.named("Back"))
  .close(wall.named("Left"));

const roof = Roof.named("Roof")
  .part("Main", [[0, 0], [ft(30), 0], [ft(30), ft(16)], [0, ft(16)]], { pitch: pitch(6), overhang: inch(12), rake: inch(12), gables: [2, 4], on: [walls] })
  .part("Wing", [[ft(6), -ft(10)], [ft(18), -ft(10)], [ft(18), ft(8)], [ft(6), ft(8)]], { pitch: pitch(6), overhang: inch(12), rake: inch(12), gables: [1, 3], on: [walls] });

export default Building.named("A roof that turns a corner").add(walls, roof);
