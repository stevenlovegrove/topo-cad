// A slab where a scan has it (examples/primitives/15-a-slab-read-from-a-scan.ts), written on the part core.
//
// The slab's top is drawn at nothing: only where the solve starts (a fallback). A scan saw the floor at three places.
// The scan has its own frame (a part placed in the model: turned 20 degrees, its origin 0.4 m above nothing), and each
// reading is a point of that frame on the slab's top, to 3 mm. The three put the floor 11, 12 and 13 mm below where it
// is drawn: the slab's top is one number, it goes to 12 mm down, between them, and each reading says how far off it is
// left (what gave way). The walls stand on the slab, so they go with it. Walls 12' by 8' (their outsides), 8' tall, of
// 2x4 at 16".

import { part, node, free, read, fallback, on, placed, lumber, nominal, DFL, mm, inch, ft } from "topo-part";
import { slab } from "topo-part/framing";
import { perimeter } from "topo-part/walls";

const stud = lumber(nominal["2x4"], DFL.No2);
const [wide, deep, height] = [ft(12), ft(8), ft(8)];
const turn = (20 * Math.PI) / 180;

export default part("a slab read from a scan", () => {
  // The slab's top: level, at `top`.
  const top = free(mm(0));
  // (The slab: 4" of concrete to the walls' outsides, its top through these, the ground under it.)
  const floor = slab("slab", { outline: [[0, 0], [wide, 0], [wide, deep], [0, deep]], top: [node(0, 0, top), node(mm(1000), 0, top), node(0, mm(1000), top)], thickness: inch(4) });
  const slabTop = floor.top;
  const w = perimeter("walls", { corners: [node(0, 0, 0), node(wide, 0, 0), node(wide, deep, 0), node(0, deep, 0)], legs: { height, base: top, spacing: inch(16), stock: stud } });
  // The scan, where it was registered: and where it saw the floor, in its own coordinates.
  const seen = mm.each([[500, 400, -411], [1800, 200, -412], [1100, 1300, -413]]).map(([x, y, z]) => node(x, y, z));
  const scan = part("scan", { seen }, { at: placed({ origin: [mm(1000), mm(500), mm(400)], x: [Math.cos(turn), Math.sin(turn), 0], y: [-Math.sin(turn), Math.cos(turn), 0] }) });
  return {
    slab: floor,
    walls: w,
    scan,
    says: {
      "drawn at nothing": fallback(top, mm(0)),
      ...Object.fromEntries(seen.map((p, k) => [`seen ${k + 1}`, read(on(p, slabTop), mm(3))])),
    },
  };
});
