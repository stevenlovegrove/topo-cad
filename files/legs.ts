// Walls as legs (topo-part/walls, docs/walls.md): a shed of four legs on a slab, two of 2x6 and two of 2x4, a door
// under a flush header in the front, a window in the back, and a partition of 2x4 teed into the front from the
// inside.
//
// Each leg runs between two corners of its outside face, anticlockwise seen from above. Where two meet their plates
// lap, and each one's end stud stands inside the other's inside face: so a 2x4 leg meeting a 2x6 one stops its end
// stud 140 in from the corner, the 2x6 one 89 in. The partition's bottom and lower top plates end on the front's inside
// face; its upper top plate laps over the front's lower one; the front has a stud each side of it to back it (where it
// is said to be teed into it), its end stud nailed to them. So at the back.

import { part, node, sub, lumber, nominal, DFL, mm, inch, ft } from "topo-part";
import { slab } from "topo-part/framing";
import { wall, perimeter, tee } from "topo-part/walls";

const twoBySix = lumber(nominal["2x6"], DFL.No2), twoByFour = lumber(nominal["2x4"], DFL.No2);
const header = lumber(nominal["2x10"], DFL.No2, { plies: 2 });
const [wide, deep, height] = [ft(16), ft(10), ft(8)];
const corners = [node(0, 0, 0), node(wide, 0, 0), node(wide, deep, 0), node(0, deep, 0)];
const legs = {
  front: { stock: twoBySix, height, spacing: inch(16), backs: [{ at: mm(3000), width: twoByFour.depth }], openings: [{ label: "D1", kind: "door" as const, centre: mm(1500), width: mm(1000), header, flush: true }] },
  side: { stock: twoByFour, height, spacing: inch(16) },
  back: { stock: twoBySix, height, spacing: inch(24), backs: [{ at: mm(4876.8 - 3000), width: twoByFour.depth }], openings: [{ label: "W1", kind: "window" as const, centre: mm(1000), width: mm(1200), head: mm(2100), height: mm(1000), header }] },
};

export default part("legs", () => {
  const floor = slab("slab", { outline: [[0, 0], [wide, 0], [wide, deep], [0, deep]], top: [node(0, 0, 0), node(mm(1000), 0, 0), node(0, mm(1000), 0)], thickness: inch(4) });
  const walls = perimeter("walls", { corners, legs: [legs.front, legs.side, legs.back, legs.side], names: ["front", "right", "back", "left"] });
  // (The partition: from the front's inside face, 3 m along it, to the back's.)
  // (Its line is its +z face, so half its depth east of its middle.)
  const partition = wall("partition", { from: node(mm(3044.5), twoBySix.depth, 0), to: node(mm(3044.5), sub(deep, twoBySix.depth), 0), stock: twoByFour, height, spacing: inch(16) });
  const tees = [tee(walls.legs[0], partition, mm(3000)), tee(walls.legs[2], partition, mm(4876.8 - 3000), { end: "end" })];
  return { slab: floor, walls, partition, tees };
});
