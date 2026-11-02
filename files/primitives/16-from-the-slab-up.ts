// A small building from the slab up (examples/primitives/16-from-the-slab-up.ts), written on the part core.
//
// Each thing stands on the one under it, and none is given a height:
//
// - a slab laid to fall 10 mm a metre toward the door in the west wall (its top a plane of the ground, at nothing on
//   its east side: a fallback);
// - a curb on it whose top is level, 20 mm above nothing: 20 mm tall at the east wall, 57 mm at the west, stopping at
//   the door (concrete, cut to the slab);
// - the walls on the curb, 8' tall, so their tops are level too (sliding up and down only, found there);
// - the roof over the walls: gables north and south, Fink trusses across the 12' on the walls' plates (4 in 12, 12" past
//   the walls, every 24"), found where they rest.
//
// Read the slab somewhere else and the curb changes; make the curb's level another and the walls and the roof go up or
// down with it. Walls 12' by 16' (their outsides), of 2x4 at 16"; an 8' door, its header (2) 2x12 tight under the top
// plates.

import { part, node, free, plane, polygon, cut, must, fallback, on, sub, add, div, placed, joints, frame, extrusion, lumber, nominal, DFL, materials, mm, inch, ft, type Length } from "topo-part";
import { slab, fink, row } from "topo-part/framing";
import { perimeter, type Opening } from "topo-part/walls";

const stud = lumber(nominal["2x4"], DFL.No2);
const [wide, deep, height] = [ft(12), ft(16), ft(8)];
const inside = stud.depth, level = mm(20);
// (The door, 8' in the west wall, centred: its header (2) 2x12 tight under the top plates.)
const door: Opening = { label: "Door", kind: "door", centre: div(deep, 2), width: ft(8), header: lumber(nominal["2x12"], DFL.No2, { plies: 2 }), jacks: 1, flush: true };

export default part("from the slab up", () => {
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
  const gap = [sub(div(deep, 2), ft(4)), add(div(deep, 2), ft(4))];
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
  // The roof: each truss across the 12', its heels at the walls' outsides, its bottom chord's middle a half-depth over
  // the walls' top (they stand on the curb), its pieces centred on its line, every 24" along the 16'.
  const roof = part("roof", (r) => {
    const lines = row(deep, inch(24), stud.thickness);
    const heel = add(lift, height, div(stud.depth, 2));
    const heels = lines.map((y) => [node(0, sub(y, div(stud.thickness, 2)), heel), node(wide, sub(y, div(stud.thickness, 2)), heel)]);
    const trusses = heels.map((h) => fink("truss", { span: wide, pitch: 4 / 12, overhang: inch(12), stock: stud, at: frame(h[0], { x: h[1], y: r.y }) }));
    return { heels, trusses };
  });
  return {
    slab: floor,
    curb,
    seat,
    base,
    walls: w,
    roof,
    says: {
      "drawn at nothing": fallback(top, mm(0)),
      "walls on the curb": must(on(base, plane(seat[0], seat[1], seat[2]))),
    },
  };
});
