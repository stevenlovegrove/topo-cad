// The sheet bracket (examples/sheet-bracket.ts), written on the part core: a bracket of 2 mm aluminium on a T-slot rail.
//
// The bracket is cut flat to a rectangle 60 by 70, its corners rounded to 5, with two holes in its base and one in
// its flange, and folded up along the line 40 from its edge through 90° at an inside radius of 3 (examples/
// sheet.ts). It is free, and goes where its holes say: each is fixed in the slot on top of the rail by a socket-head
// screw into a T-nut (examples/tslot.ts), so its base lies against the rail (its underside to the rail's
// top, which also says which way up it is) with both holes over the slot, anywhere along it (where it was drawn, unless something else says). The rail is held fixed at one end, and at the
// other against moving across and down: the bracket's weight is to one side of it, and twists it. It is solved as a
// frame, as the original is.
//
// On the flange's hole is a part from a library: a small block with a hole through it, read from a STEP file
// (`topo part parts/block-m4.step block-m4.ts --core 6061-T6` wrote it, its body exactly). It is put by its
// hole: the hole's mouth on the flange's, its face flat on the flange's underside (the side away from the base), turned
// about the hole as it was drawn. It is bolted there: an M4 socket-head screw (the largest the holes pass) through the
// flange and the block, its head on the flange, to a nut on the block's far side, as long as that takes.

import { part, node, must, against, hold, designBasis, frame, neg, place, placed, joints, nut, line, span, tslot, tslots, materials, add, sub, mul, ge, mm, deg, as } from "topo-part";
import { folded, flatPattern, type Fold } from "topo-part/sheet";
import { inSlot } from "./tslot";
import { capScrew, capScrewLengths } from "topo-part/fixings";
import { blockm4 } from "./block-m4";

const outline = ([[0, 0], [60, 0], [60, 70], [0, 70]] as [number, number][]).map((p) => ({ at: mm.each(p), round: mm(5) }));
const fold: Fold = { from: mm.each([0, 40] as [number, number]), to: mm.each([60, 40] as [number, number]), angle: deg(90), radius: mm(3) };
const thickness = mm(2);

export const flat = flatPattern({ thickness, outline, fold });

export default part("sheet bracket", (root) => {
  const a = node(0, 0, 0), b = node(mm(300), 0, 0);
  const rail = span(a, b, root.y, tslot(tslots["40x40"]));
  // (Drawn beside the rail; its holes put it on the rail.)
  const bracket = folded("bracket", {
    thickness,
    material: materials.aluminium6061T6,
    outline,
    holes: { h1: { at: [mm(15), mm(20)], d: mm(5.5) }, h2: { at: [mm(45), mm(20)], d: mm(5.5) }, m1: { at: [mm(30), mm(55)], d: mm(4.5) } },
    fold,
    // (Free: placed beside the rail as it was drawn, and found where its holes and its base put it.)
    at: placed({ origin: [mm(100), mm(-30), mm(30)], joint: joints.free() }),
  });
  // The block on the flange's hole: its hole's mouth (`h2`, at its z = 0, opening along its -z) on the flange's, its -z
  // into the flange.
  const flange = bracket.bend.turn.flange;
  const block = place(blockm4("block"), placed({ origin: [mm(-10), mm(-6), 0] }));
  // (Its screw from the flange's top, through the flange and the block, to a nut on the block's far face.)
  const [thread, grip] = [mm(4), add(thickness, mm(10))];
  const length = capScrewLengths.find((l) => ge(l, add(grip, mul(1.1, thread))))!;
  const entry = node(0, 0, mm(-2)), exit = node(0, 0, sub(length, thickness));
  const axis = line(entry, exit);
  const mount = part("mount", {
    block,
    entry,
    exit,
    axis,
    screw: capScrew("screw", thread, length, materials.A36, { at: placed({ origin: [0, 0, mm(-2)] }) }),
    nut: nut("nut", thread, materials.A36, { at: placed({ origin: [0, 0, mm(10)] }) }),
    fixing: { $: "fixing" as const, designation: `M4x${as(length, mm)} socket head cap screw, hex nut`, diameter: thread, length, joins: [flange, block], axis },
  }, { at: frame(flange.centres.m1, { x: flange.x, y: neg(flange.y) }) });
  const fix = (h: "h1" | "h2") => inSlot(h, { piece: bracket.base, centre: bracket.base.centres[h], hole: mm(5.5), thickness, profile: rail, slot: rail.slots["+z"], along: rail.x, across: rail.y });
  return {
    a,
    b,
    rail,
    bracket,
    fixings: { h1: fix("h1"), h2: fix("h2") },
    mount,
    holds: [hold(a, "fixed"), hold(b, { y: true, z: true })],
    design: designBasis({ analysis: "frame", combinations: "each-case" }),
    // (Its base's underside against the rail's top: which way up it is, too. Turned over about the slot, its holes
    // would still be in it and its base on the rail's plane, but its underside facing up.)
    says: { seated: must(against(bracket.base.faces["-z"], rail.faces["+z"])) },
  };
});
