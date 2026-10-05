// A roof that turns a corner.
//
// The building is a long bar with a wing out of its front. Its roof is two
// parts, each a plain gable roof over its own walls: one along the bar, one
// along the wing and run on into the bar as far as its ridge (`reach`). Where
// two parts overlap, the higher is the roof: so the wing's ridge runs back
// until it meets the main roof's slope, and the two valleys from there down
// to the inside corners are where the parts' planes cross. Neither is given.
//
// Both are framed with trusses. The main roof, given first, is framed
// through. The wing's trusses stop where it reaches the main roof (its eave),
// and from there the wing is carried on over the main roof by a valley set:
// trusses of the wing's own slopes, smaller and smaller, each with a level
// bottom chord that lies across the main roof's top chords (touching each at
// its uphill face, since they slope). None of that is given: it follows from
// the two parts overlapping and both being trussed.
//
// The wing is narrower than the bar, so its ridge is lower.
//
// (Not yet: the last of the valley set, too short to cross two of the trusses
// under it: the top of the wing's ridge there is its surfaces only. In
// building, the main roof is sheathed first and the set sits on the
// sheathing; here it sits on the chords. The main roof's eave is drawn
// running on behind the wing's last truss.)
import { Building, DFL, ft, inch, Perimeter, pitch, psf, Roof, Wall } from "topo-cad";

const grade = DFL.No2;
const wall = Wall.template({ height: ft(8), grade });
const main = Perimeter.start("Walls", [0, 0])
  .by([ft(30), 0], wall.named("Front"))
  .by([0, ft(16)], wall.named("Right"))
  .by([-ft(30), 0], wall.named("Back"))
  .close(wall.named("Left"));
// (The wing's walls: three, from the bar's front wall out and back to it.)
const wing = Perimeter.start("Wing walls", [ft(7.5), 0])
  .by([0, -ft(10)], wall.named("Wing left"))
  .by([ft(12), 0], wall.named("Wing front"))
  .by([0, ft(10)], wall.named("Wing right"));

const roof = Roof.named("Roof")
  .over(main, { name: "Main", pitch: pitch(6), overhang: inch(12), rake: inch(12), gables: ["Left", "Right"], trusses: { grade } })
  .over(wing, { name: "Wing", pitch: pitch(6), overhang: inch(12), rake: inch(12), gables: ["Wing front", 4], reach: { 4: ft(8) }, trusses: { grade } })
  .load("snow", psf(25));

export default Building.named("A roof that turns a corner").add(main, wing, roof);
