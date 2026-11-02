// A floor on walls (examples/primitives/11-a-floor-on-walls.ts), written on the part core.
//
// A floor platform of 2x10 DF-L No.2 joists at 16" (topo-part/framing): rim joists across the ends of its joists, a
// band joist along each side, common joists between. Its rectangle is given on plan, 12' by 10', over a room of walls
// 8' tall, its joists running front to back. Its height is not given: it is free, and its underside is on the walls'
// top, so the solve sits it there. Dead load 10 psf and live 40 psf on its joists.

import { part, node, free, plane, support, placed, joints, must, on, areaLoad, cases, lumber, nominal, DFL, inch, ft, mm, psf } from "topo-part";
import { floor } from "topo-part/framing";
import { perimeter } from "topo-part/walls";

const stud = lumber(nominal["2x4"], DFL.No2);
const joist = lumber(nominal["2x10"], DFL.No2);
const [width, depth, height] = [ft(12), ft(10), ft(8)];

export default part("a floor on walls", () => {
  const w = perimeter("walls", { corners: [node(0, 0, 0), node(width, 0, 0), node(width, depth, 0), node(0, depth, 0)], legs: { height, spacing: inch(16), stock: stud } });
  // (Drawn at nothing: its height is where its underside is on the walls.)
  // (It slides up and down only: how high is found, where its corner is on the walls.)
  const rise = free(mm(0));
  const corner = node(0, 0, rise);
  const f = floor("floor", { width, length: depth, spacing: inch(16), stock: joist, at: placed({ joint: joints.slider([0, 0, 1], rise) }) });
  const ground = [node(0, 0, 0), node(mm(1), 0, 0), node(0, mm(1), 0)];
  return {
    walls: w,
    corner,
    floor: f,
    ground,
    slab: support(plane(ground[0], ground[1], ground[2])),
    dead: areaLoad(f, psf(10), cases.dead),
    live: areaLoad(f, psf(40), cases.live),
    says: {
      "on the walls": must(on(corner, w.legs[0].topPlate.faces["-y"])),
    },
  };
});
