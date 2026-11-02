// A wheel well (examples/volumes/08-a-wheel-well.ts), written on the part core: a block with a wheel's well taken out
// of it, and a slot for a strap.
//
// A printed block, 60 by 29 by 25, and a wheel on an axle beside it (as in 03, without its dish), the axle along y at x
// 30, 15 up, turning in it (a hinge, how far it has turned a freedom of the solve). The well is the wheel's own profile
// a millimetre larger all round, in to 1 at the axle (so a pin of the block is left in the wheel's bore), turned about
// the same axle: a revolve that is only a tool, in the axle's frame, so it goes where the axle goes and the block is cut
// where it now is. The block less the well is exact (`topo-brep`'s carve): its faces are pieces of the block's (named
// as they were: its end, `start`, in two pieces either side of the well, and the pin's end, `start 3`) and of the
// well's (`well/o1`, its round, facing out of the block), each edge where a face of the well meets a face of the block
// the exact curve where they meet: a circle on the end, a line along the tread, where the top and bottom cut the
// rounds a curve on a torus. Less a slot (a box, 50 by 10 by 2), far from the well. The wheel turns in the well with a
// millimetre all round: nothing passes through anything.

import { part, node, placed, joints, revolve, extrusion, polygon, cut, free, materials, mm, deg, type Corner } from "topo-part";

/** The wheel's profile, its outer corners rounded to 2, a millimetre larger all round (`out`) and in to 1 at the axle. */
const profile = (out: number) => ({
  hubIn: node(mm(out > 0 ? 1 : 1.45), 0, mm(2 - out)),
  shoulderIn: node(mm(15 + out), 0, mm(2 - out)),
  shoulderOut: node(mm(15 + out), 0, mm(10 + out)),
  hubOut: node(mm(out > 0 ? 1 : 1.45), 0, mm(10 + out)),
});
const corners = (p: ReturnType<typeof profile>, round: number): Corner[] => [
  { node: p.hubIn },
  { node: p.shoulderIn, round: mm(round) },
  { node: p.shoulderOut, round: mm(round) },
  { node: p.hubOut },
];

export default part("a wheel well", () => {
  const petg = materials.PETG;
  const wheel = part("wheel", profile(0), { kind: "solid" });
  wheel.shapes = [revolve(corners(wheel as never, 2), petg)];
  const well = part("well", profile(1));
  well.shapes = [revolve(corners(well as never, 3), petg, { tool: true })];
  // (The axle's frame: its z along the world's y, 15 up at x 30; its x the world's, its y down.)
  const axle = part("axle", { wheel, well }, { at: placed({ origin: [mm(30), 0, mm(15)], x: [1, 0, 0], y: [0, 0, -1], joint: joints.hinge([0, 0, 1], free(deg(0))) }) });
  const outline = { a: node(0, mm(-20), 0), b: node(mm(60), mm(-20), 0), c: node(mm(60), mm(9), 0), d: node(0, mm(9), 0) };
  const block = part("block", outline, { kind: "solid" });
  block.shapes = [extrusion(polygon(outline.a, outline.b, outline.c, outline.d), { from: mm(5), to: mm(30), material: petg })];
  return {
    axle,
    block,
    cuts: [cut(block, { tool: "body", part: well }, "well"), cut(block, { tool: "box", frame: block, lo: [mm(5), mm(-25), mm(26)], hi: [mm(55), mm(-15), mm(28)] }, "slot")],
  };
});
