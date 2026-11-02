// A saltbox (examples/primitives/29-a-saltbox.ts), written on the part core: a roof of two pitches.
//
// Walls 16' by 12' (their outsides), 8' to the top of their upper top plates. The front of the roof is at 8 in 12 and
// the back at 4 in 12; the ends are gables. The ridge is not given: it is where the two planes meet, a third of the way
// back (`roofFaces`). It is framed from them (topo-part/roofs): pairs of 2x8 rafters every 24", a short
// steep one and a long shallow one, meeting at the ridge, their feet on the two plates at one height; a 2x6 ceiling tie
// beside each pair. Dead load 12 psf and snow 25 psf on plan.

import { part, node, plane, support, areaLoad, cases, lumber, nominal, DFL, sub, div, inch, ft, mm, psf } from "topo-part";
import { perimeter } from "topo-part/walls";
import { roof, type Plan } from "topo-part/roofs";

const stud = lumber(nominal["2x4"], DFL.No2);
const tie = lumber(nominal["2x6"], DFL.No2);
const rafter = lumber(nominal["2x8"], DFL.No2);
const [wide, deep, height] = [ft(16), ft(12), ft(8)];
// (The roof's outline is the walls' middles, half a wall in from their outsides.)
const out = div(stud.depth, 2);
const outline: Plan[] = [[out, out], [sub(wide, out), out], [sub(wide, out), sub(deep, out)], [out, sub(deep, out)]];

export default part("a saltbox", () => {
  const w = perimeter("walls", { corners: [node(0, 0, 0), node(wide, 0, 0), node(wide, deep, 0), node(0, deep, 0)], legs: { height, spacing: inch(16), stock: stud } });
  const r = roof("roof", {
    outline, plate: height, pitch: 8 / 12, pitches: [8 / 12, undefined, 4 / 12, undefined], gables: [false, true, false, true],
    outside: out, spacing: inch(24), from: inch(12), stock: rafter, ties: tie,
  });
  const ground = [node(0, 0, 0), node(mm(1), 0, 0), node(0, mm(1), 0)];
  return {
    walls: w,
    roof: r,
    ground,
    slab: support(plane(ground[0], ground[1], ground[2])),
    dead: areaLoad(r, psf(12), cases.dead),
    snow: areaLoad(r, psf(25), cases.snow),
  };
});
