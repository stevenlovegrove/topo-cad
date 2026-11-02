// An L of legs (topo-part/walls), said clockwise from its south-west corner as it was walked: north up its west side,
// east along its north, south down its inner east, east along its inner north, south down its east, and west home.
//
// Each leg is made the other way round (its outside is its outside), and what is said along it is as walked: the door
// 2 m along the west wall from the south-west corner, the window 2 m along the north wall from the north-west. Where
// the inner east wall meets the inner north wall the corner turns in: the one lapping over (the inner east) runs on past
// the corner to the other's inside face, its end stud there, and the other's upper top plate is over the corner. Every
// wall stands on a 40 mm curb on the slab, each curb taking the corner its wall starts at (or runs on past), stopping at
// the door. The north wall keeps its studs clear of a stretch where a post is to go.

import { part, node, lumber, nominal, DFL, mm, inch } from "topo-part";
import { slab } from "topo-part/framing";
import { perimeter, type Opening } from "topo-part/walls";

const twoBySix = lumber(nominal["2x6"], DFL.No2);
const header = lumber(nominal["2x10"], DFL.No2, { plies: 2 });
const corners = mm.each([[0, 0], [0, 6000], [4000, 6000], [4000, 3000], [7000, 3000], [7000, 0]] as [number, number][]);
const door: Opening = { label: "D1", kind: "door", centre: mm(2000), width: mm(900), head: mm(2050), header };
const window_: Opening = { label: "W1", kind: "window", centre: mm(2000), width: mm(1200), head: mm(2050), height: mm(1000), header };

export default part("an L", () => {
  const floor = slab("slab", { outline: corners, top: [node(0, 0, 0), node(mm(1000), 0, 0), node(0, mm(1000), 0)], thickness: inch(4) });
  const leg = { stock: twoBySix, height: mm(2400), base: mm(40), curb: { height: mm(40) }, spacing: inch(16) };
  const walls = perimeter("walls", {
    corners: corners.map(([x, y]) => node(x, y, 0)),
    names: ["west", "north", "inner east", "inner north", "east", "south"],
    legs: [{ ...leg, openings: [door] }, { ...leg, openings: [window_], clear: [{ from: mm(3000), to: mm(3100) }] }, leg, leg, leg, leg],
  });
  return { slab: floor, walls };
});
