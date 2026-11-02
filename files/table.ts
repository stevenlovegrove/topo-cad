// A table: a library of the parts it is made of, used by the model of one standing on an uneven floor
// (examples/uneven-floor.ts).
//
// A leg is a part of its own: a square section along its z from its top (where it is fixed to what it holds up) to its
// foot, as long as its own param says (free, unless whoever uses it says). Every leg made is a copy, with a length of
// its own.
//
// A table is a subassembly: a top, and a leg under each of its corners, each placed by a frame the table makes of its
// own geometry (at a corner, its z down the table's). What the table says of itself stays in it: its legs are set in
// from its edges by as much as it says, and stand square to it, by how they are placed. Whoever uses a table places
// it, and may say more of what is in it (where its legs' feet are), but need not know how it is made.

import { part, node, free, frame, neg, polygon, plate, extrusion, sub, div, mul, mm, DFL, type Frame, type Length, type Material, type Num } from "topo-part";

/** A leg `across` square, along its z from its top (at its origin) to its foot, `length` long: free (from where it
 *  was last found, 700 at first) unless said. */
export function leg(name: string, o: { across?: Length; length?: Num<"length">; material?: Material; at?: Frame; held?: boolean } = {}) {
  const { across = mm(45), material = DFL.No2 } = o;
  const length = o.length ?? free(mm(700), { min: mm(100) });
  const h = div(across, 2), nh = mul(-1, h);
  const corners = [node(nh, nh, 0), node(h, nh, 0), node(h, h, 0), node(nh, h, 0)];
  const top = node(0, 0, 0), foot = node(0, 0, length);
  return part(name, { length, corners, top, foot }, {
    at: o.at,
    held: o.held,
    kind: "lumber",
    shapes: [extrusion(polygon(...corners), { to: length, material })],
  });
}

/** A table `width` by `depth`, its top `thickness` thick, its origin at a corner of the top's underside: a leg under
 *  each corner, set in from the edges by `inset` to its middle, standing down (along the table's −z). The legs are
 *  `legs[0]` to `legs[3]` round from the origin; any of them `asBuilt` (by its place there) was made `length` long and
 *  is held (as it is), and the rest are free. */
export function table(name: string, o: { width?: Length; depth?: Length; thickness?: Length; inset?: Length; length?: Length; asBuilt?: number[]; at?: Frame } = {}) {
  const { width = mm(900), depth = mm(500), thickness = mm(25), inset = mm(60), length = mm(700), asBuilt = [] } = o;
  return part(name, (self) => {
    const [x1, y1] = [sub(width, inset), sub(depth, inset)];
    const corners = [[inset, inset], [x1, inset], [x1, y1], [inset, y1]].map(([x, y]) => node(x, y, 0));
    // (Each leg's frame: at its corner, its x the table's, its y against the table's, so its z is down.)
    const legs = corners.map((c, i) =>
      leg(`leg ${i + 1}`, {
        at: frame(c, { x: self.x, y: neg(self.y) }),
        // (A leg as built: its length free, as any leg's is, and held, so it is as it is wherever it is used.)
        ...(asBuilt.includes(i) ? { length: free(length, { min: mm(100) }), held: true } : {}),
      }),
    );
    // (A top of boards glued up.)
    return { top: plate("top", width, depth, thickness, DFL.No2), corners, legs };
  }, { at: o.at });
}
