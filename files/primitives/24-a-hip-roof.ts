// A hip roof (examples/primitives/24-a-hip-roof.ts), written on the part core.
//
// Walls 24' by 16' (their outsides), 8' to the top of their upper top plates. The roof over them is at 6 in 12 from
// every wall: no edge is a gable, so a plane rises from each, and the roof is the lowest of them. Its ridge and its four
// hips are where the planes meet, as the engine finds them (`roofFaces`). It is framed from them (topo-part/roofs): on
// each face, 2x8 rafters square to its eave every 24" from the outside corner (the first at 12"), each up until it
// leaves its face, against its pair at the ridge or against a hip rafter's side; a hip rafter on each hip,
// cut against the last pair at the ridge. Every rafter's foot is cut level on its plate and plumb at the wall's
// outside. Dead load 12 psf and snow 25 psf on plan.
//
// (Its commons are laid out from the ridge's ends and its jacks from the corners, as the original's; a 2x6 ceiling tie
// lies on the plates beside each pair. The original runs 16" past the walls, which its rafters do not reach (they
// have no tails), so that is left out here.)

import { part, node, plane, support, areaLoad, cases, lumber, nominal, DFL, sub, div, inch, ft, mm, psf } from "topo-part";
import { perimeter } from "topo-part/walls";
import { roof, type Plan } from "topo-part/roofs";

const stud = lumber(nominal["2x4"], DFL.No2);
const tie = lumber(nominal["2x6"], DFL.No2);
const rafter = lumber(nominal["2x8"], DFL.No2);
const [wide, deep, height] = [ft(24), ft(16), ft(8)];
// (The roof's outline is the walls' middles, half a wall in from their outsides.)
const out = div(stud.depth, 2);
const outline: Plan[] = [[out, out], [sub(wide, out), out], [sub(wide, out), sub(deep, out)], [out, sub(deep, out)]];

export default part("a hip roof", () => {
  const w = perimeter("walls", { corners: [node(0, 0, 0), node(wide, 0, 0), node(wide, deep, 0), node(0, deep, 0)], legs: { height, spacing: inch(16), stock: stud } });
  const r = roof("roof", { outline, plate: height, pitch: 0.5, outside: out, spacing: inch(24), from: inch(12), stock: rafter, ties: tie });
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
