// A lid and its stop (examples/volumes/06-a-lid-and-its-stop.ts), written on the part core: a joint's range, and where
// what moves first meets something on the way.
//
// A printed box, 60 by 40 by 30, and its lid, 3 thick, on a hinge along the box's back top edge (along −y, so opening
// turns the lid up and back) that may open from shut to 110 degrees: its angle is free between those. Behind the box is
// a wall, its face 12 behind the hinge: the lid, opening, meets it before it gets to 110. Where, and by what, is found
// exactly (not by trying angles): turning about the hinge, a corner goes round a circle, so where it comes to a face is
// in closed form, and where an edge comes to an edge a root of a trigonometric polynomial; the least of them is the
// first contact (topo-brep's contact). The issues say: the lid meets the wall at so many degrees, which corner of the
// lid against which face of the wall. Move the wall back, or the range down, and it goes all the way. Shut, toward the
// least of its range, it is there already: nothing is asked.

import { part, node, placed, joints, polygon, extrusion, free, materials, mm, deg, type Length } from "topo-part";

/** A box from `lo` to `hi` (x, y, z), its own nodes, extruded up its z. */
function box(name: string, lo: [number, number, number], hi: [number, number, number], o: { at?: ReturnType<typeof placed> } = {}) {
  const at = (x: number, y: number) => node(mm(x), mm(y), mm(lo[2]) as Length);
  const corners = { a: at(lo[0], lo[1]), b: at(hi[0], lo[1]), c: at(hi[0], hi[1]), d: at(lo[0], hi[1]) };
  const b = part(name, corners, { at: o.at, kind: "solid" });
  b.shapes = [extrusion(polygon(corners.a, corners.b, corners.c, corners.d), { from: mm(lo[2]), to: mm(hi[2]), material: materials.PETG })];
  return b;
}

export default part("a lid and its stop", () => {
  const lid = box("lid", [0, 0, 0], [60, 40, 3], { at: placed({ origin: [0, 0, mm(30)], joint: joints.hinge([0, -1, 0], free(deg(0), { min: deg(0), max: deg(110) })) }) });
  return {
    box: box("box", [0, 0, 0], [60, 40, 30]),
    lid,
    wall: box("wall", [-20, -10, 0], [-12, 50, 100]),
  };
});
