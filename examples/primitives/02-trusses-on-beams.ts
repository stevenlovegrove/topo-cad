// Free variables: trusses sat on what is under them.
//
// Three trusses on two beams, the far beam 2 inches lower. The roof is given
// no height. Each truss is a rigid thing, free to move (6 freedoms), with
// weight. It is asked, all hard:
//
// - its chord's underside is no lower than each edge of each beam's top (a
//   one-sided contact: it may sit on it, or be clear of it);
// - its heel is where the layout puts it on plan (datums);
// - it is plumb and square to the ridge (it is braced).
//
// Its weight then brings it down onto whatever holds it up. The solve finds
// which contacts those are: sloping down to the lower beam, one edge of each
// top; it is clear of the other two by a fraction of a millimetre.
//
// In the 3D view, turn on Skeleton. Squares are the trusses (at their heels),
// short bars are numbers (where along the chord it is over each edge), rings
// are constraints (faint and dotted: a contact that is not touching), and
// orange diamonds are the bearing points the trusses were then joined to the
// beams at (on the beams' top faces: their axes are through their middles).
// Hover a square for its six freedoms, each coloured by what settled it:
// hard, or its weight.
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
