// Walls standing straight on a slab laid to fall (examples/primitives/13-walls-on-a-slab-laid-to-fall.ts), written on
// the part core.
//
// Without a curb, a wall's bottom is the slab's top, wherever the wall is. The slab rises 20 mm a metre along x (it is
// at nothing at its west side). So:
//
// - the west wall stands at nothing and the east wall 73 mm higher (12' along). The slab slopes across each of them:
//   its bottom plate lies on the slab, turned about its own length by the slope (`across`), its studs plumb on it,
//   their ends cut to it;
// - the north wall's bottom plate slopes with the slab (its base a pair: its start's and its end's);
// - the south wall's curb is of one height (40 mm above the slab all along, concrete cut to it), so that wall stands
//   40 mm higher than the others where it meets them (the curb stopping at the east wall's inside, 2 mm short, where
//   that wall's bottom plate runs through to the south wall's outside).
//
// Their tops are level, all at one height (each corner's top is said, from nothing, not its height): 8' above the
// highest thing they stand on, the curb at its east end. So each wall is as tall as it has to be, the north wall's
// studs shorter toward the east, and at each corner the plates lap. Walls 12' by 8' (their outsides), of 2x4 at 16".

import { part, node, plane, polygon, cut, sub, add, mul, div, extrusion, lumber, nominal, DFL, materials, mm, inch, ft, type Length } from "topo-part";
import { slab } from "topo-part/framing";
import { perimeter } from "topo-part/walls";

const stud = lumber(nominal["2x4"], DFL.No2);
const [wide, deep, height] = [ft(12), ft(8), ft(8)];
const out = div(stud.depth, 2), inside = stud.depth, curbed = mm(40), fall = 20 / 1000;
const z = (x: Length): Length => mul(fall, x);
const level = add(z(sub(wide, out)), curbed, height);

export default part("walls on a slab laid to fall", () => {
  // (The slab: 4" of concrete to the walls' outsides, its top through these, the ground under it.)
  const floor = slab("slab", { outline: [[0, 0], [wide, 0], [wide, deep], [0, deep]], top: [node(0, 0, 0), node(mm(1000), 0, z(mm(1000))), node(0, mm(1000), 0)], thickness: inch(4) });
  const slabTop = floor.top;
  const top = [node(0, 0, curbed), node(mm(1000), 0, add(z(mm(1000)), curbed)), node(mm(1000), mm(1000), add(z(mm(1000)), curbed))];
  // (The curb's top, facing down: what is above it taken away; the slab's, facing up: what is below it.)
  const curbTop = plane(top[0], top[2], top[1]);
  // (Stopping 2 mm short of the east wall's inside: its bottom plate, turned to the slab, leans that far.)
  const short = sub(sub(wide, inside), mm(2));
  const corners = [node(0, 0, 0), node(short, 0, 0), node(short, inside, 0), node(0, inside, 0)];
  const curb = part("curb", { corners }, { kind: "curb", shapes: [extrusion(polygon(...corners), { from: mm(-300), to: mm(400), material: materials.concrete })] });
  // (South, on the curb, sloping with it; east, on the slab, level along it, turned across to it; north, sloping with it;
  // west, level along it, turned across. Where a base slopes along a wall, it is said at the wall's ends, its outside
  // corners; where it slopes across, under its middle.)
  const leg = { level, spacing: inch(16), stock: stud };
  const w = perimeter("walls", {
    corners: [node(0, 0, 0), node(wide, 0, 0), node(wide, deep, 0), node(0, deep, 0)],
    legs: [
      { ...leg, base: [add(z(mm(0)), curbed), add(z(wide), curbed)] },
      { ...leg, base: z(sub(wide, out)), across: fall },
      { ...leg, base: [z(wide), z(mm(0))] },
      { ...leg, base: z(out), across: -fall },
    ],
  });
  return {
    slab: floor,
    top,
    curbTop,
    curb,
    curbCuts: { slab: cut(curb, { tool: "planes", planes: [slabTop] }, "slab"), top: cut(curb, { tool: "planes", planes: [curbTop] }, "top") },
    walls: w,
  };
});
