// Fixing to T-slot profile, as library parts written on the part core: a socket-head bolt through a hole into a T-nut
// in a profile's slot, the nut shaped as the present pipeline draws one (`topo_fab::tslot::t_nut`), sized to the slot,
// and the bolt as it chooses one (topo-part/fixings).
//
// A slot here is of the 40 series as item draws its Profile 8 40x40 light (8 wide, its lips 4.5 thick; below).

import { part, node, line, frame, placed, neg, cut, polygon, circle, extrusion, must, on, materials, mm, as, type Node, type PartBase, type Basis, type Num, type Length, type Material } from "topo-part";
import { capScrew, capScrewLengths, threadFor } from "topo-part/fixings";

/** Millimetres of a known length: inside, the slot is laid out in them. */
const mmOf = (l: Length) => as(l, mm);

/** A slot of the 40 series, across it: its mouth, its lips' thickness, how deep it reaches, half its width under the
 *  lips, the straight wall there and half its floor's width (millimetres; `slot40` has them as lengths). As item's
 *  drawing of the Profile 8 40x40 light has it (crates/topo-data/data/tslot-outlines.json): its lips 4.5 thick, its
 *  undercut 10 either side of the slot's middle, its wall straight 1.308 below the lips before it rounds in, its floor
 *  12.25 down. */
const SLOT = { width: 8, lip: 4.5, deep: 12.25, wide: 10, straight: 1.308, floor: 3.558 };
/** A slot of the 40 series, its sizes as lengths. */
export const slot40 = { width: mm(SLOT.width), lip: mm(SLOT.lip), deep: mm(SLOT.deep), wide: mm(SLOT.wide), straight: mm(SLOT.straight), floor: mm(SLOT.floor) };

/** How much narrower a nut is than its slot, each side (mm). */
const CLEARANCE = 0.3;
/** A T-nut for a bolt of thread `d` in the 40 series' slot, along its x (the slot's way), its top on its xy plane (the
 *  lips' underside) and down its z: it fills the slot under the lips less a clearance each side, its sides square to
 *  the lips as far as the slot's wall is straight and then in at 45° beside the slot's flanks; as thick as takes a
 *  diameter of thread below that wall; half as long again as the slot's mouth is wide (not less than two diameters);
 *  threaded through at its middle. */
export function tNut(name: string, thread: Length, material: Material = materials.A36, o: { at?: ReturnType<typeof placed> } = {}) {
  const d = mmOf(thread), slot = SLOT;
  const half = slot.wide - CLEARANCE;
  const taper = Math.max(0, Math.min(d, half - 0.8 * d, slot.deep - slot.lip - slot.straight - CLEARANCE));
  const length = Math.max(1.5 * slot.width, 2 * d);
  const thick = slot.straight + taper;
  const p = (y: number, z: number) => node(mm(y), mm(z), 0);
  // (Its section across the slot, in its yz plane, laid along its x: a profile in its own frame turned so.)
  const section = [p(-half, 0), p(half, 0), p(half, slot.straight), p(half - taper, thick), p(-half + taper, thick), p(-half, slot.straight)];
  const body = part("body", { section }, { at: placed({ origin: [mm(-length / 2), 0, 0], x: [0, 1, 0], y: [0, 0, 1] }), kind: "fixing", shapes: [extrusion(polygon(...section), { to: mm(length), material })] });
  const top = node(0, 0, 0), bottom = node(0, 0, mm(thick));
  const axis = line(top, bottom);
  return part(name, { body, top, bottom, axis, thread: cut(body, { tool: "bore", line: axis, r: mm(d / 2) }, "thread") }, { at: o.at, kind: "fixing" });
}

/** A hole of a flat piece fixed in a profile's slot: its centre on the slot's middle line (`slot`, on the face the
 *  piece lies on, which faces `out`), a socket-head screw through the hole (the largest thread it clears, as long as
 *  a diameter of thread into the nut below the lips, of `capScrewLengths`), and a T-nut in the slot under the lips.
 *  The fixing's frame is at the hole on the piece's underside: its z into the slot, its x along it. */
export function inSlot(name: string, o: { piece: PartBase; centre: Node; hole: Length; thickness: Length; profile: PartBase; slot: { start: Node; end: Node; line: ReturnType<typeof line> }; along: Basis; across: Basis }) {
  const thread = threadFor(o.hole), d = mmOf(thread), thickness = mmOf(o.thickness);
  const need = thickness + SLOT.lip + Math.min(d, SLOT.straight + Math.max(0, Math.min(d, SLOT.wide - CLEARANCE - 0.8 * d)));
  const length = capScrewLengths.find((l) => mmOf(l) >= need) ?? capScrewLengths[capScrewLengths.length - 1];
  // (Its z into the slot: x along the slot, y the other way across it from the profile's.)
  const at = frame(o.centre, { x: o.along, y: neg(o.across) });
  const entry = node(0, 0, mm(-thickness)), exit = node(0, 0, mm(mmOf(length) - thickness));
  const axis = line(entry, exit);
  return part(name, {
    entry,
    exit,
    axis,
    screw: capScrew("screw", thread, length, materials.A36, { at: placed({ origin: [0, 0, mm(-thickness)] }) }),
    nut: tNut("nut", thread, materials.A36, { at: placed({ origin: [0, 0, mm(SLOT.lip)] }) }),
    fixing: { $: "fixing" as const, designation: `M${d}x${mmOf(length)} socket head cap screw, T-nut`, diameter: thread, length, joins: [o.piece, o.profile], axis },
    says: { "in the slot": must(on(o.centre, o.slot.line)) },
  }, { at });
}
