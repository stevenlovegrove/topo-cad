// Free variables: trusses sat on what is under them, in the one solve.
//
// Three trusses on two beams, the far beam 2 inches lower. The roof is given
// no height. Each truss is a rigid thing, free to move (6 freedoms), in the
// building's one solve with everything else. `bearingOn` is a utility: it
// looks at what is under each truss (as things are put) and states how it
// sits:
//
// - Under each truss: an edge, the middle and the other edge of each beam's
//   top. Which carry it: those of the upper hull of them that its middle is
//   between (sloping down to the lower beam, the far edge of each top).
// - Each contact: a node on a line of the truss's chord (its underside: one
//   number, bounded by the chord) and on that line of the beam's top (one
//   number, bounded by the beam), held to be one point.
//
// Each truss is also asked: hard, its heel is where the layout puts it across
// the ridge; hints, it is where the layout puts it along the ridge, upright,
// and turned as it was put (so plumb, square to the ridge and level). The
// contacts decide its height and its tilt; the level hint gives way to them
// (and says by how much); the other hints settle its roll, its turn on plan
// and its place along the ridge.
//
// In the 3D view, turn on Skeleton. Squares are the trusses (at their heels);
// small circles are the solve's nodes (hover one for what it is a point of,
// and its number with its bounds); rings are constraints; orange diamonds
// are the model's bearing points, which are the contact nodes (on the beams'
// top faces: their axes are through their middles).
import { Assembly, Building, DFL, Frame, ft, inch, TrussRoof, TrussShape } from "topo-cad";

const grade = DFL.No2;
const o = { size: [4, 8] as const, grade };
const near = inch(2), far = ft(16) - inch(2);
const beams = Assembly.named("Beams")
  .point("a", [-ft(1), near, ft(8)]).point("b", [ft(5), near, ft(8)])
  .point("c", [-ft(1), far, ft(8) - inch(2)]).point("d", [ft(5), far, ft(8) - inch(2)])
  .member("beam", ["a", "b"], o)
  .member("beam", ["c", "d"], o)
  .supportedAt("a", "b", "c", "d");

// Where the roof is set out on plan: a datum (a point of the world, x and y only).
const setOut = Frame.world.point("set-out", { x: 0, y: 0 });
const roof = TrussRoof.of(TrussShape.fink({ span: ft(16), pitch: 6 / 12, overhang: inch(12) }), { at: setOut.node("set-out"), spanDir: [0, 1], length: ft(4), grade })
  .bearingOn(beams);

export default Building.named("Trusses on beams").add(beams, setOut, roof);
