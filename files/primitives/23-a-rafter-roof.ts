// A roof of rafters (examples/primitives/23-a-rafter-roof.ts), written on the part core.
//
// Walls 12' by 16' (their outsides), 8' to the top of their upper top plates. The roof over them is at 6 in 12, its
// front and back gables, so its eaves are on the left and right walls and its ridge is midway between them. It is
// framed with rafters (topo-part/roofs): pairs of 2x8 from the walls' plates up to the ridge, every 24",
// each foot cut level on its plate and plumb at the wall's outside, each head plumb against its pair's; and a 2x6
// ceiling tie across each pair, lying on the plates beside it. Dead load 12 psf and snow 25 psf on plan.

import { part, node, plane, support, span, areaLoad, cases, roles, lumber, nominal, DFL, add, sub, div, inch, ft, mm, psf, type Length } from "topo-part";
import { perimeter } from "topo-part/walls";
import { rafters, type Plan } from "topo-part/roofs";

const stud = lumber(nominal["2x4"], DFL.No2);
const tie = lumber(nominal["2x6"], DFL.No2);
const rafter = lumber(nominal["2x8"], DFL.No2);
const [wide, deep, height] = [ft(12), ft(16), ft(8)];
// (The walls' middles, half a wall in from their outsides: where the rafters' eaves are drawn from.)
const out = div(stud.depth, 2);

export default part("a rafter roof", () => {
  const w = perimeter("walls", { corners: [node(0, 0, 0), node(wide, 0, 0), node(wide, deep, 0), node(0, deep, 0)], legs: { height, spacing: inch(16), stock: stud } });
  // (Each pair every 24" from the front, the end ones flush with the gables' outsides.)
  const half = div(rafter.thickness, 2);
  const at = [...Array.from({ length: 8 }, (_, k) => (k === 0 ? half : inch(24 * k))), sub(deep, half)];
  const face = (x: Length, inward: [number, number]) => ({ eave: [[x, 0], [x, deep]] as [Plan, Plan], inward, plate: height, pitch: 0.5, run: sub(div(wide, 2), out), outside: out, at, stock: rafter });
  const roof = part("roof", (r) => {
    const right = rafters("right", face(sub(wide, out), [-1, 0]));
    const left = rafters("left", face(out, [1, 0]));
    // (Each tie beside its pair, on the side toward the middle; the last on the side toward the front.)
    const ties = at.map((y, k) => {
      const t = k === at.length - 1 ? sub(y, rafter.thickness) : add(y, rafter.thickness);
      const z = add(height, div(tie.depth, 2));
      const ends = [node(sub(wide, out), sub(t, div(tie.thickness, 2)), z), node(out, sub(t, div(tie.thickness, 2)), z)];
      return { ends, tie: span(ends[0], ends[1], r.y, tie, { role: roles.ceilingTie }) };
    });
    return { right, left, ties };
  });
  const ground = [node(0, 0, 0), node(mm(1), 0, 0), node(0, mm(1), 0)];
  return {
    walls: w,
    roof,
    ground,
    slab: support(plane(ground[0], ground[1], ground[2])),
    dead: areaLoad(roof, psf(12), cases.dead),
    snow: areaLoad(roof, psf(25), cases.snow),
  };
});
