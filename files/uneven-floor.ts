// A table on an uneven floor: how parts are put together on the core (docs/part-core.md).
//
// The table is a library part (examples/table.ts): a top, and four legs, each a copy of a library leg with
// a length of its own. One leg was made already, 700 long, and is used as built: it is held (its length is as it
// is). The other three are to be cut to suit.
//
// The floor falls 8 mm across the table's width and rises 5 mm across its depth. The table stands level: it is placed
// on a slider up and down (a joint), how high free. Every foot must be on the floor. So the leg as built says
// how high the table stands (it is the one thing that can), and each other leg is found as long as it must be to reach
// the floor from there.
//
// What each part says stays in it: the table sets its legs in from its edges and stands them square to it, and knows
// nothing of floors; the model, which has both, says where the feet are. Open the Params tab to see what was found:
// the height, and the three lengths cut to fit; the held leg's is there too, by "held" (it is as it is).
//
// (A part made by a function, as the leg is, is a copy each time, with params of its own. A part shared, one set of
// params wherever it is used, is `shared(part)` and `use(it, at)`.)

import { part, node, free, placed, joints, must, on, plane, support, mm } from "topo-part";
import { table } from "./table";

export default part("a table on an uneven floor", () => {
  // The floor: a plane through three points, falling along x and rising along y.
  const floor = [node(0, 0, 0), node(mm(1000), 0, mm(-8)), node(0, mm(1000), mm(5))];
  const ground = plane(floor[0], floor[1], floor[2]);
  // Where the table stands: level, sliding only up and down, how high found.
  const height = free(mm(725));
  const t = table("table", { asBuilt: [0], at: placed({ origin: [mm(50), mm(50), 0], joint: joints.slider([0, 0, 1], height) }) });
  return {
    floor,
    // (It bears the table: the structure's ground. It is not drawn: turn on the nodes to see its three points.)
    bears: support(ground),
    height,
    table: t,
    says: Object.fromEntries(t.legs.map((l, i) => [`leg ${i + 1} on the floor`, must(on(l.foot, ground))])),
  };
});
