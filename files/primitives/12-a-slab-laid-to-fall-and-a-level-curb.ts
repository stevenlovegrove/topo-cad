// A slab laid to fall, and a curb that makes a level seat on it (examples/primitives/12-a-slab-laid-to-fall-and-a-level-
// curb.ts), written on the part core.
//
// A garage slab is laid to fall toward the door, so that water runs out. The walls' plates cannot follow it: they sit
// on a curb whose top is level. Here the slab's top rises 10 mm a metre along x (it falls toward the door in the west
// wall); it is at nothing at its east side, where its height is drawn.
//
// - The slab is what it is in the structure: its top, a plane, what everything stands on. Its height there is a free
//   number of the one solve, drawn at nothing (a fallback).
// - The curb is concrete under the walls, its top level at 20 mm, cut to the slab's top beneath it: 20 mm tall at the
//   east wall, 57 mm at the west. It stops at the door, as the bottom plate does.
// - Every wall stands on the curb's top (sliding up and down only, found there): its bottom is at 20 mm, as it would
//   be on a level floor.
//
// The last line reads the slab 5 mm lower than drawn at its east side: the slab moves down, all of it, and the curb is
// 5 mm taller everywhere; the walls do not move, since what they stand on is the curb's top, and that is the level it
// is given.

import { part, node, free, plane, polygon, cut, must, read, fallback, on, sub, add, div, placed, joints, extrusion, lumber, nominal, DFL, materials, mm, inch, ft, type Length } from "topo-part";
import { slab } from "topo-part/framing";
import { perimeter, type Opening } from "topo-part/walls";

const stud = lumber(nominal["2x4"], DFL.No2);
const [wide, deep, height] = [ft(12), ft(8), ft(8)];
const inside = stud.depth, level = mm(20);
// (The door, 3' in the west wall, centred: its header (2) 2x8 tight under the top plates.)
const door: Opening = { label: "Door", kind: "door", centre: div(deep, 2), width: ft(3), header: lumber(nominal["2x8"], DFL.No2, { plies: 2 }), jacks: 1, flush: true };

export default part("a slab laid to fall and a level curb", () => {
  // The slab's top: at `top` along its east side, rising 10 mm a metre along x (so falling west).
  const top = free(mm(0));
  // (The slab: 4" of concrete to the walls' outsides, its top through these, the ground under it.)
  const floor = slab("slab", { outline: [[0, 0], [wide, 0], [wide, deep], [0, deep]], top: [node(wide, 0, top), node(wide, mm(1000), top), node(sub(wide, mm(1000)), 0, sub(top, mm(10)))], thickness: inch(4) });
  const slabTop = floor.top;
  // The curb: under the walls, its top level, cut to the slab's top (what is below it taken away).
  const piece = (name: string, x0: Length | 0, y0: Length | 0, x1: Length, y1: Length) => {
    const corners = [node(x0, y0, 0), node(x1, y0, 0), node(x1, y1, 0), node(x0, y1, 0)];
    const p = part(name, { corners }, { kind: "curb", shapes: [extrusion(polygon(...corners), { from: mm(-300), to: level, material: materials.concrete })] });
    return { piece: p, cut: cut(p, { tool: "planes", planes: [slabTop] }, "slab") };
  };
  const gap = [sub(div(deep, 2), ft(1.5)), add(div(deep, 2), ft(1.5))];
  const curb = {
    south: piece("south", 0, 0, wide, inside),
    north: piece("north", 0, sub(deep, inside), wide, deep),
    east: piece("east", sub(wide, inside), inside, wide, sub(deep, inside)),
    "west 1": piece("west 1", 0, inside, inside, gap[0]),
    "west 2": piece("west 2", 0, gap[1], inside, sub(deep, inside)),
  };
  const seat = [node(0, 0, level), node(mm(1), 0, level), node(0, mm(1), level)];
  // The walls: on the curb's top.
  const lift = free(mm(0));
  const base = node(0, 0, lift);
  const leg = { height, spacing: inch(16), stock: stud };
  const w = perimeter("walls", { corners: [node(0, 0, 0), node(wide, 0, 0), node(wide, deep, 0), node(0, deep, 0)], legs: [leg, leg, leg, { ...leg, openings: [door] }], at: placed({ joint: joints.slider([0, 0, 1], lift) }) });
  // (A level put on the slab at the middle of its east side: 5 mm below where it is drawn.)
  const survey = node(wide, div(deep, 2), mm(-5));
  return {
    slab: floor,
    curb,
    seat,
    base,
    walls: w,
    survey,
    says: {
      "drawn at nothing": fallback(top, mm(0)),
      "walls on the curb": must(on(base, plane(seat[0], seat[1], seat[2]))),
      "a level on the slab": read(on(survey, slabTop), mm(1)),
    },
  };
});
