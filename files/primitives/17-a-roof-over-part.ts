// A roof over part of a building, with another truss in some bays (examples/primitives/17-a-roof-over-part.ts), written
// on the part core.
//
// A roof is set out from a place, for a length: it need not cover what it stands on. The walls are 24' long and the
// trusses start 4' along the front wall and run for 16', so 4' at each end is open to the sky. The three in the middle
// are fan trusses (topo-part/trusses, laid out as the present pipeline lays one out), the rest Fink (topo-part/
// framing's): of the same span and pitch, they sit on the same plates and make the same roof. Walls 24' by 14' (their
// outsides), 8' tall, of 2x4 at 16"; trusses of 2x4 across the 14', 5 in 12, 12" past the walls, every 24".

import { part, node, plane, support, frame, neg, lumber, nominal, DFL, add, div, inch, ft, mm, as, type Length } from "topo-part";
import { fink } from "topo-part/framing";
import { perimeter } from "topo-part/walls";
import { truss } from "topo-part/trusses";

const stud = lumber(nominal["2x4"], DFL.No2);
const [long, deep, height] = [ft(24), ft(14), ft(8)];
const [k, overhang] = [5 / 12, inch(12)];

/** A fan truss `l` long, at pitch `k`, `o` past each heel: as the present pipeline lays one out. (Its points in
 *  millimetres, worked out from its length and overhang.) */
function fan(name: string, length: Length, at: ReturnType<typeof frame>) {
  const [l, o] = [as(length, mm), as(overhang, mm)];
  const p = (x: number, z: number): [Length, Length] => [mm(x), mm(z)];
  const t = (s: number) => p(s, s * k);
  return truss(name, {
    points: { X0: p(-o, -o * k), X1: p(l + o, -o * k), H0: p(0, 0), H1: p(l, 0), P: t(l / 2), T1: t(l / 6), T2: t(l / 3), "T1'": p(l - l / 6, (l / 6) * k), "T2'": p(l - l / 3, (l / 3) * k), B1: p(l / 3, 0), B2: p((2 * l) / 3, 0) },
    top: [["X0", "H0", "T1", "T2", "P"], ["X1", "H1", "T1'", "T2'", "P"]],
    bottom: ["H0", "B1", "B2", "H1"],
    webs: [["B1", "T1"], ["B1", "T2"], ["B1", "P"], ["B2", "P"], ["B2", "T2'"], ["B2", "T1'"]],
    topChord: stud, bottomChord: stud, web: stud, at,
  });
}

export default part("a roof over part", () => {
  const w = perimeter("walls", { corners: [node(0, 0, 0), node(long, 0, 0), node(long, deep, 0), node(0, deep, 0)], legs: { height, spacing: inch(16), stock: stud } });
  // Nine trusses from 4' along the front for 16', each across the depth, its heels at the walls' outsides, its pieces
  // centred on its line; the 4th to the 6th fans.
  const roof = part("roof", (r) => {
    const lines = Array.from({ length: 9 }, (_, i) => add(ft(4), inch(24 * i)));
    const trusses = lines.map((x, i) => {
      const f = i >= 3 && i <= 5;
      // (A Fink's heels are its bottom chord's middle; a fan's, its bottom chord's bottom.)
      const z = f ? height : add(height, div(stud.depth, 2));
      const h = [node(add(x, div(stud.thickness, 2)), 0, z), node(add(x, div(stud.thickness, 2)), deep, z)];
      const at = frame(h[0], { x: h[1], y: neg(r.x) });
      return { heels: h, truss: f ? fan("fan", deep, at) : fink("fink", { span: deep, pitch: k, overhang, stock: stud, at }) };
    });
    return { trusses };
  });
  const ground = [node(0, 0, 0), node(mm(1), 0, 0), node(0, mm(1), 0)];
  return {
    walls: w,
    roof,
    ground,
    slab: support(plane(ground[0], ground[1], ground[2])),
  };
});
