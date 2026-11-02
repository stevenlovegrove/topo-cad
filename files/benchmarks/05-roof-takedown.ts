// Benchmark 5 on the part core: a roof's load taken down to the foundation (as
// examples/benchmarks/05-roof-takedown.ts).
//
// A 12' by 16' building: Fink trusses at 24" on centre spanning 12'-0" east from the west wall's outside to the
// east wall's, 4/12 pitch, 12" overhangs, their heels on the walls' top plates. Roof dead load 15 psf and snow 25 psf,
// both on plan. The walls that bear them run north and south, 8'-1⅛" to the top of their top plates, on the slab.

import { part, bears, node, plane, support, frame, areaLoad, cases, lumber, nominal, DFL, add, sub, div, ft, ftIn, inch, mm, psf, pitch, type Node } from "topo-part";
import { fink, row } from "topo-part/framing";
import { wall as leg } from "topo-part/walls";

const width = ft(12), length = ft(16), height = ftIn(8, 1.125);

const stud = lumber(nominal["2x4"], DFL.No2);
// (A wall from one place of its outside face to another: its outside to its right, looking along it.)
const wall = (name: string, from: Node, to: Node) => leg(name, { from, to, height, spacing: inch(16), stock: stud });

export default part("roof takedown", () => {
  const ends = { west: [node(0, length, 0), node(0, 0, 0)], east: [node(width, 0, 0), node(width, length, 0)] };
  const west = wall("west wall", ends.west[0], ends.west[1]), east = wall("east wall", ends.east[0], ends.east[1]);
  // Each truss's heel on the west wall's outside, its bottom chord's underside on the top plates; its pieces centred
  // on its line, every 24" from the north wall's line (y = 0) to the south's.
  const lines = row(length, inch(24), stud.thickness);
  const roof = part("roof", (r) => {
    const z = add(height, div(stud.depth, 2)), half = div(stud.thickness, 2);
    const heels = lines.map((y) => [node(0, sub(y, half), z), node(width, sub(y, half), z)]);
    // (Its x east, its y along the building: so its z is up.)
    const trusses = heels.map((h) => fink("truss", { span: width, pitch: pitch(4), overhang: inch(12), stock: stud, at: frame(h[0], { x: h[1], y: r.y }) }));
    return { heels, trusses };
  });
  const ground = [node(0, 0, 0), node(mm(1), 0, 0), node(0, mm(1), 0)];
  return {
    ends, west, east, roof, ground,
    // (Its load path: the roof's trusses on the walls.)
    rests: [bears(roof, west), bears(roof, east)],
    slab: support(plane(ground[0], ground[1], ground[2])),
    dead: areaLoad(roof, psf(15), cases.dead),
    snow: areaLoad(roof, psf(25), cases.snow),
  };
});
