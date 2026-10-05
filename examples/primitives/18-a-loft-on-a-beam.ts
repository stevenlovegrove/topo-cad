// A floor over part of a room: a loft, on the back wall and a beam.
//
// A floor sits on what is under it, and that may be a beam as well as a
// wall. Here a beam crosses the room half way back, on a post at each end
// (an assembly: the posts stand on the foundation, and are as tall as leaves
// the beam's top at the walls' top). The loft's joists run from the beam to
// the back wall: `bearingOn(walls, beam)` rests each on the back wall's cap
// plate and on the beam, where it crosses them, and its rims on the side
// walls. The floor is given on plan only; the solve sits it.
//
// (The beam is added to the building before the floor that bears on it.)
import { Assembly, Building, DFL, Floor, ft, inch, Perimeter, psf, Wall } from "topo-cad";

const grade = DFL.No2;
const [wide, deep, half] = [ft(12), ft(16), ft(8)];
const wall = Wall.template({ height: ft(8), grade });
const walls = Perimeter.start("Walls", [0, 0])
  .by([wide, 0], wall.named("Front"))
  .by([0, deep], wall.named("Right"))
  .by([-wide, 0], wall.named("Back"))
  .close(wall.named("Left"));

// A 4x10 on two 4x4 posts just inside the side walls; its top at the walls' top (a 4x10 is 9.25" deep).
const under = ft(8) - inch(9.25);
const [x0, x1] = [inch(3.5) + inch(1.75), wide - inch(3.5) - inch(1.75)];
const beam = Assembly.named("Beam")
  .point("west foot", [x0, half, 0]).point("west top", [x0, half, under])
  .point("east foot", [x1, half, 0]).point("east top", [x1, half, under])
  .member("post", ["west foot", "west top"], { size: [4, 4], grade })
  .member("post", ["east foot", "east top"], { size: [4, 4], grade })
  .member("beam", ["west top", "east top"], { size: [4, 10], grade, anchor: [0, -0.5], priority: 10 })
  .supportedAt("west foot", "east foot");

const loft = Floor.of({ joists: [2, 8], grade, spacing: inch(16) })
  .named("Loft")
  .from([wide, half - inch(1.75)], { along: [0, 1], length: deep - half + inch(1.75), width: wide })
  .bearingOn(walls, beam)
  .load("dead", psf(10))
  .load("live", psf(30));

export default Building.named("A loft on a beam").add(walls, beam, loft);
