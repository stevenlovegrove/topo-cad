// A lean-to of legs (topo-part/walls, docs/walls.md): walls standing on a slab laid to fall, under a roof that slopes.
//
// The slab's top rises 1 in 100 to the back; every leg stands on it (its bottom plate's underside on the slab's top,
// along its middle): the sides' bottom plates slope with it, their studs plumb; the front's and the back's are turned
// across to lie on it. Each top is a level (from nothing, not from where it stands): the front's at 3 m, the back's at
// 2.4 m, the sides' sloping from one to the other, and at each corner the leg whose top falls the more away from it
// laps over, seated where it crosses (the sides over the front, the back over the sides). The back has a
// window whose centre is a hint (to be read or dragged), and a wall teed into it from each side: a partition inside
// and a wing outside, an X, both backed by the same pair of studs; the partition's upper top plate laps over the back's,
// the wing's stops at it.

import { part, node, hinted, lumber, nominal, DFL, mm, inch } from "topo-part";
import { slab } from "topo-part/framing";
import { wall, perimeter, tee } from "topo-part/walls";

const twoBySix = lumber(nominal["2x6"], DFL.No2), twoByFour = lumber(nominal["2x4"], DFL.No2);
const header = lumber(nominal["2x8"], DFL.No2, { plies: 2 });
const [wide, deep] = [mm(4000), mm(3000)];
const corners = [node(0, 0, 0), node(wide, 0, 0), node(wide, deep, 0), node(0, deep, 0)];
// (Where the X is along the back, from its start (its east end), and how far from the front the back's inside is.)
const along = mm(2400), back = 3000 - 139.7;

export default part("lean-to", () => {
  // (The slab runs on past the back, under the wing.)
  const floor = slab("slab", { outline: [[0, 0], [wide, 0], [wide, mm(4400)], [0, mm(4400)]], top: [node(0, 0, 0), node(mm(1000), 0, 0), node(0, mm(1000), mm(10))], thickness: inch(4) });
  const leg = { stock: twoBySix, base: floor.top, spacing: inch(16) };
  const walls = perimeter("walls", {
    corners,
    names: ["front", "right", "back", "left"],
    legs: [
      { ...leg, level: mm(3000), across: -0.01 },
      { ...leg, level: [mm(3000), mm(2400)] },
      { ...leg, level: mm(2400), across: 0.01, backs: [{ at: along, width: twoByFour.depth }], openings: [{ label: "W1", kind: "window", centre: hinted(mm(1000)), width: mm(900), head: mm(2000), height: mm(1100), header }] },
      { ...leg, level: [mm(2400), mm(3000)] },
    ],
  });
  // (A partition inside, from the back's inside face toward the front, and a wing outside it: each's line its +z face,
  // half its depth from its middle, 1600 from the west end.)
  const x = 4000 - 2400;
  const partition = wall("partition", { from: node(mm(x - 44.5), mm(back), 0), to: node(mm(x - 44.5), mm(1500), 0), stock: twoByFour, base: floor.top, level: mm(2400), spacing: inch(16) });
  const wing = wall("wing", { from: node(mm(x + 44.5), mm(3000), 0), to: node(mm(x + 44.5), mm(4200), 0), stock: twoByFour, base: floor.top, level: mm(2400), spacing: inch(16) });
  const tees = [tee(walls.legs[2], partition, along), tee(walls.legs[2], wing, along, { side: "outside", lap: false })];
  return { slab: floor, walls, partition, wing, tees };
});
