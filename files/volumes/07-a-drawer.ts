// A drawer (examples/volumes/07-a-drawer.ts), written on the part core: a slider's range, and a drawer that pulls out of
// a cabinet until its stop meets the cabinet's lip.
//
// The drawer slides along x, as far as 50 out: its travel is free between nothing and 50. A small stop on its back top
// meets the lip under the cabinet's top when it has come 42 out: the issues say so, and by which corner and face. Pushed
// back in, toward the least of its range, it is there already: nothing is asked.

import { part, node, placed, joints, polygon, extrusion, free, materials, mm, type Length } from "topo-part";

/** A box from `lo` to `hi` (x, y, z), its own nodes, extruded up its z. */
function box(name: string, lo: [number, number, number], hi: [number, number, number]) {
  const at = (x: number, y: number) => node(mm(x), mm(y), mm(lo[2]) as Length);
  const corners = { a: at(lo[0], lo[1]), b: at(hi[0], lo[1]), c: at(hi[0], hi[1]), d: at(lo[0], hi[1]) };
  const b = part(name, corners, { kind: "solid" });
  b.shapes = [extrusion(polygon(corners.a, corners.b, corners.c, corners.d), { from: mm(lo[2]), to: mm(hi[2]), material: materials.PETG })];
  return b;
}

export default part("a drawer", () => {
  const runner = part("runner", { drawer: box("drawer", [2, 2, 0], [58, 48, 28]), stop: box("stop", [2, 20, 28], [6, 30, 31]) }, {
    at: placed({ joint: joints.slider([1, 0, 0], free(mm(0), { min: mm(0), max: mm(50) })) }),
  });
  return {
    top: box("cabinet top", [0, 0, 32], [60, 50, 35]),
    lip: box("lip", [48, 0, 29], [52, 50, 32]),
    runner,
  };
});
