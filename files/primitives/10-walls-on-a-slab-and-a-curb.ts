// Walls on a slab and a curb (examples/primitives/10-walls-on-a-slab-and-a-curb.ts), written on the part core: what
// walls stand on is a thing.
//
// A wall is not given a height to start at: it stands on something, and its bottom is that thing's top. Here a slab,
// its top drawn 40 mm below nothing, and a 40 mm curb on it:
//
// - three walls stand on the curb (each run's base is the curb's top);
// - the right wall stands on the slab itself, 40 mm lower; its top is level with the others' (each wall's top is said,
//   from nothing, not its height), so it is 40 mm taller, and its plates lap theirs at its corners as any wall's do
//   (the curb stops at its inside face, as the others' bottom plates do).
//
// The slab's top is a number of the one solve: where it is drawn is a fallback. The last line is a reading of it (a
// level put on the floor): 45 mm below nothing, not 40. The slab moves down to it, and everything standing on it moves
// with it.

import { part, node, free, polygon, must, read, fallback, on, equal, add, sub, placed, joints, extrusion, lumber, nominal, DFL, materials, mm, inch, ft, type Length } from "topo-part";
import { slab } from "topo-part/framing";
import { perimeter } from "topo-part/walls";

const stud = lumber(nominal["2x4"], DFL.No2);
const [wide, deep, height] = [ft(12), ft(8), ft(8)];
const inside = stud.depth, curbed = mm(40);

export default part("walls on a slab and a curb", () => {
  // The slab's top: level, at `top`.
  const top = free(mm(-40));
  // (The slab: 4" of concrete to the walls' outsides, its top through these, the ground under it.)
  const floor = slab("slab", { outline: [[0, 0], [wide, 0], [wide, deep], [0, deep]], top: [node(0, 0, top), node(mm(1000), 0, top), node(0, mm(1000), top)], thickness: inch(4) });
  const slabTop = floor.top;
  // The curb: 40 mm of concrete under the front, back and left walls, on the slab (its frame slides to the slab's top).
  const lift = free(mm(-40));
  const curbBase = node(0, 0, lift);
  const piece = (name: string, x0: Length | 0, y0: Length | 0, x1: Length, y1: Length) => {
    const corners = [node(x0, y0, 0), node(x1, y0, 0), node(x1, y1, 0), node(x0, y1, 0)];
    return part(name, { corners }, { kind: "curb", shapes: [extrusion(polygon(...corners), { to: curbed, material: materials.concrete })] });
  };
  const curb = part("curb", {
    front: piece("front", 0, 0, sub(wide, inside), inside),
    back: piece("back", 0, sub(deep, inside), sub(wide, inside), deep),
    left: piece("left", 0, inside, inside, sub(deep, inside)),
  }, { at: placed({ joint: joints.slider([0, 0, 1], lift) }) });
  // The walls: the front, back and left on the curb's top, the right on the slab's; every corner's top 8' above the
  // curb's.
  const onCurb = add(top, curbed);
  const level = add(onCurb, height);
  const leg = { level, spacing: inch(16), stock: stud, base: onCurb };
  const w = perimeter("walls", { corners: [node(0, 0, 0), node(wide, 0, 0), node(wide, deep, 0), node(0, deep, 0)], legs: [leg, { ...leg, base: top }, leg, leg] });
  // (A level put on the slab, in the middle of the room: 45 mm below nothing.)
  const survey = node(ft(6), ft(4), mm(-45));
  return {
    slab: floor,
    curb,
    curbBase,
    walls: w,
    survey,
    says: {
      "drawn 40 down": fallback(top, mm(-40)),
      "the curb on the slab": must(equal(lift, top)),
      "a level on the slab": read(on(survey, slabTop), mm(1)),
    },
  };
});
