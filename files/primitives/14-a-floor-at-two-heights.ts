// A floor at two heights (examples/primitives/14-a-floor-at-two-heights.ts), written on the part core: two slabs, a step
// between them, and walls on each.
//
// A floor that steps is two slabs: the front of the room 50 mm below nothing, the back, from 10' in, at nothing. Each
// wall stands on the slab under it (its base), so the side walls are each two walls, one either side of the step: here
// a run of walls on each slab, the front's from the step round the front to the step, the back's on round the back, the
// two meeting end to end at the step. The walls on the lower slab are 50 mm taller, so every wall's top is at one
// height. The slabs are 4" of concrete, each to the walls' outsides and meeting at the step, the ground under them.
// Walls 12' by 16' (their outsides), 8' tall from the upper slab, of 2x4 at 16".

import { part, node, add, mul, lumber, nominal, DFL, mm, inch, ft, type Length, type Node } from "topo-part";
import { slab } from "topo-part/framing";
import { perimeter } from "topo-part/walls";

const stud = lumber(nominal["2x4"], DFL.No2);
const [wide, deep, step, drop, height] = [ft(12), ft(16), ft(10), mm(50), ft(8)];
// (A run of walls on corners of their outsides, open at its ends: on `base`, `tall` above it.)
const run = (name: string, corners: [Length | 0, Length | 0][], base: Length | 0, tall: Length) =>
  perimeter(name, { corners: corners.map(([x, y]) => node(x, y, 0)), closed: false, legs: { height: tall, base, spacing: inch(16), stock: stud } });

export default part("a floor at two heights", () => {
  const lower = run("lower", [[0, step], [0, 0], [wide, 0], [wide, step]], mul(-1, drop), add(height, drop));
  const upper = run("upper", [[wide, step], [wide, deep], [0, deep], [0, step]], 0, height);
  // (Each slab's top level, at its height.)
  const level = (z: Length | 0): [Node, Node, Node] => [node(0, 0, z), node(mm(1000), 0, z), node(0, mm(1000), z)];
  return {
    lower,
    upper,
    lowerSlab: slab("lower slab", { outline: [[0, 0], [wide, 0], [wide, step], [0, step]], top: level(mul(-1, drop)), thickness: inch(4) }),
    upperSlab: slab("upper slab", { outline: [[0, step], [wide, step], [wide, deep], [0, deep]], top: level(0), thickness: inch(4) }),
  };
});
