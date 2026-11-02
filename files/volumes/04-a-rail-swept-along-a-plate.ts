// A rail swept along a plate (examples/volumes/04-a-rail-swept-along-a-plate.ts), written on the part core: a sweep along
// nodes, which goes where the plate's corners go.
//
// An upright plate, 80 wide, its top a peak whose height is free, drawn at 40 and held at 50 by what is said of it. A
// printed rail is swept along the plate's top edges: its path is three of the plate's corners on its outer face (d, the
// peak, c), as nodes, so it follows the peak to where it is held, not where it is drawn. Between its runs it turns
// through a mitre, so each run is a prism, exactly; its ends are cut level (`endOn` planes through d and c, square to z).
//
// Its outline is in the plane square to its path: its x across the path (toward the plate's middle), its y out of the
// plate's face. Its faces are named by run: `o1 run 2`, the side from its outline's second corner along the second run.

import { part, node, free, placed, polygon, extrusion, sweep, plane, endOn, must, is, materials, mm, type Corner } from "topo-part";

export default part("a rail swept along a plate", () => {
  // (The plate upright in the world's xz plane: its x the world's, its y up, its thickness out along −y.)
  const peakY = free(mm(40));
  const shape = { a: node(0, 0, 0), b: node(mm(80), 0, 0), c: node(mm(80), mm(30), 0), peak: node(mm(40), peakY, 0), d: node(0, mm(30), 0) };
  const tops = { c: node(mm(80), mm(30), mm(2)), peak: node(mm(40), peakY, mm(2)), d: node(0, mm(30), mm(2)) };
  const level = { d: node(mm(10), mm(30), mm(2)), c: node(mm(90), mm(30), mm(2)), dOut: node(0, mm(30), mm(12)), cOut: node(mm(80), mm(30), mm(12)) };
  const plate = part("plate", { ...shape, tops, level }, { at: placed({ x: [1, 0, 0], y: [0, 0, 1] }), kind: "sheet" });
  plate.shapes = [extrusion(polygon(shape.a, shape.b, shape.c, shape.peak, shape.d), { to: mm(2), material: materials.aluminium6061T6 })];
  // (The rail holds the plate, so its path can be the plate's corners. Its outline in its own x and y as the sweep
  // carries them: its x is its part's y (the world's, out of the plate's back) made square to the path, its y across
  // the path, up: so toward the plate's middle is less than nothing in y, out of its face less than nothing in x.)
  const outline = { in: node(0, 0, 0), out: node(0, mm(-4), 0), edge: node(mm(-1.5), mm(-4), 0), over: node(mm(-2), 0, 0) };
  const corners: Corner[] = [outline.in, outline.out, outline.edge, outline.over].map((n) => ({ node: n }));
  const rail = part("rail", { ...outline, plate }, { kind: "solid" });
  rail.shapes = [sweep(corners, [tops.d, tops.peak, tops.c], materials.PETG)];
  return {
    rail,
    // (Its ends level: on the planes square to z through d and c, each facing into the rail, up.)
    ends: [endOn(rail, plane(tops.d, level.d, level.dOut)), endOn(rail, plane(tops.c, level.cOut, level.c))],
    peakHeld: must(is(peakY, mm(50))),
  };
});
