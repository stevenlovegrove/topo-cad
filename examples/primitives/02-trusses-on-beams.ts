// Free variables: trusses sat on what is under them.
//
// Three trusses on two beams, the far beam 2 inches lower. The roof is given
// no height: `bearingOn` is a utility that finds what is under each truss
// and makes nodes there.
//
// - Support nodes, fixed on the beams: under each truss, an edge, the middle
//   and the other edge of each beam's top.
// - Which two carry each truss: sloping down to the lower beam, the far edge
//   of each top.
// - Chord nodes: the point of the truss's chord that is over each of those
//   two. Each is on a line of the truss: one number, bounded by the chord.
//
// Each truss is a rigid thing, free to move (6 freedoms). It is asked:
//
// - hard: each chord node is on its support node (level with it, over it);
// - hard (datums): its heel is where the layout puts it on plan;
// - hints: its plane is plumb and square to the ridge, and it is level.
//
// The supports decide its height and its tilt; the level hint gives way to
// them (and says by how much); the other hint settles its roll and its turn
// on plan.
//
// In the 3D view, turn on Skeleton. Squares are the trusses (at their heels);
// small circles are the solve's nodes (hover one for what it is a point of,
// and its number with its bounds); rings are constraints; orange diamonds
// are the model's bearing points, which are the support nodes the trusses
// sit on (on the beams' top faces: their axes are through their middles).
import { Assembly, Building, DFL, ft, inch, TrussRoof, TrussShape } from "topo-cad";

const grade = DFL.No2;
const o = { size: [4, 8] as const, grade };
const near = inch(2), far = ft(16) - inch(2);
const beams = Assembly.named("Beams")
  .point("a", [-ft(1), near, ft(8)]).point("b", [ft(5), near, ft(8)])
  .point("c", [-ft(1), far, ft(8) - inch(2)]).point("d", [ft(5), far, ft(8) - inch(2)])
  .member("beam", ["a", "b"], o)
  .member("beam", ["c", "d"], o)
  .supportedAt("a", "b", "c", "d");

const roof = TrussRoof.of(TrussShape.fink({ span: ft(16), pitch: 6 / 12, overhang: inch(12) }), { origin: [0, 0], spanDir: [0, 1], length: ft(4), grade })
  .bearingOn(beams);

export default Building.named("Trusses on beams").add(beams, roof);
