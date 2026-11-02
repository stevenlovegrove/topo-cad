// Walls that slope (examples/primitives/09-walls-that-slope.ts), written on the part core: tops that slope, a corner
// set low, and a corner's height read.
//
// A lean-to 12' by 10' (its walls' outsides), of 2x4 at 16": the front wall's top 8' up, the back's 10'; the side walls'
// tops slope between them (each leg's `level` at its start and its end: how high its top is, from nothing). The east
// corner of the front sits 4" lower than the rest: the bottom plates of the walls that meet there slope down to it (their
// `base` at each end), their tops 4" lower there too, as the original's heights were above a wall's bottom. The ground
// is level but under those two, where it falls to that corner.
//
// The back right corner's top is one number, found: the right wall's top at its end and the back's at its start are
// it, so they are at one height there. It was surveyed half an inch higher than drawn: the right wall's top slopes up
// to it, and the back wall's starts from it, its far end staying as drawn, so its top slopes a little.

import { part, node, free, plane, support, read, is, lumber, nominal, DFL, add, sub, inch, ft, mm } from "topo-part";
import { perimeter } from "topo-part/walls";

const stud = lumber(nominal["2x4"], DFL.No2);
const [lo, hi, step] = [ft(8), ft(10), inch(4)];

export default part("walls that slope", () => {
  // (The back right corner's top: drawn at 10', found.)
  const corner = free(hi);
  const legs = { spacing: inch(16), stock: stud };
  const walls = perimeter("lean-to", {
    corners: [node(0, 0, 0), node(ft(12), 0, 0), node(ft(12), ft(10), 0), node(0, ft(10), 0)],
    names: ["front", "right", "back", "left"],
    legs: [
      { ...legs, base: [0, sub(mm(0), step)], level: [lo, sub(lo, step)] },
      { ...legs, base: [sub(mm(0), step), 0], level: [sub(lo, step), corner] },
      { ...legs, level: [corner, hi] },
      { ...legs, level: [hi, lo] },
    ],
  });
  // What they stand on: level, and under the two that meet at the low corner, falling to it.
  const ground = [node(0, 0, 0), node(mm(1), 0, 0), node(0, mm(1), 0)];
  const front = [node(0, 0, 0), node(ft(12), 0, sub(mm(0), step)), node(0, mm(-1), 0)];
  const right = [node(ft(12), 0, sub(mm(0), step)), node(ft(12), ft(10), 0), node(ft(13), 0, sub(mm(0), step))];
  return {
    corner,
    walls,
    ground,
    front,
    right,
    slab: support(plane(ground[0], ground[1], ground[2])),
    falling: [support(plane(front[0], front[1], front[2])), support(plane(right[0], right[1], right[2]))],
    says: { surveyed: read(is(corner, add(hi, inch(0.5))), inch(0.25)) },
  };
});
