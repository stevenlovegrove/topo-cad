// A clearance (examples/volumes/09-a-clearance.ts), written on the part core: a face kept so far from another,
// one-sided.
//
// A printed spacer, 20 square, stands on the floor under a plate sloping at 15 degrees. It is drawn too tall (40): its
// top would go through the plate. Its height is free, and each corner of its top is said to be clear of the plate's
// underside by no less than 1.5 (`above`, the way the underside faces, down): so the solve brings the top down, level,
// until its highest corner is 1.5 under the plate, and no further. A clearance holds on one side only: drawn lower, the
// spacer would stay as drawn.
//
// (Compare `endOn`, in 01 and 02, which puts an end in another's plane: there it takes the plate's slope. Here it stays
// level, as it was drawn, and only moves down.)

import { part, node, free, plate, placed, polygon, extrusion, above, must, materials, mm } from "topo-part";

const tilt = (15 * Math.PI) / 180;

export default part("a clearance", () => {
  const shelf = plate("plate", mm(80), mm(50), mm(2), materials.aluminium6061T6, [], {
    at: placed({ origin: [0, 0, mm(20)], x: [Math.cos(tilt), 0, Math.sin(tilt)], y: [0, 1, 0] }),
  });
  const height = free(mm(40));
  const foot = { a: node(mm(20), mm(15), 0), b: node(mm(40), mm(15), 0), c: node(mm(40), mm(35), 0), d: node(mm(20), mm(35), 0) };
  const top = [node(mm(20), mm(15), height), node(mm(40), mm(15), height), node(mm(40), mm(35), height), node(mm(20), mm(35), height)];
  const spacer = part("spacer", { ...foot, top }, { kind: "solid" });
  spacer.shapes = [extrusion(polygon(foot.a, foot.b, foot.c, foot.d), { to: height, material: materials.PETG })];
  const under = shelf.faces["-z"];
  return { plate: shelf, spacer, clear: top.map((corner) => must(above(under, corner, under, mm(1.5)))) };
});
