// A corner of a shed's walls: the first model to carry every concept of the part core (docs/part-core.md), as far as joints and fastenings.
//
// Two legs of wall (topo-part/walls) on an open outline of three corners (their outsides') meet at the middle one; the
// far end of the second is higher, so its top plates slope. Each leg has a bottom plate and two top plates (the upper
// one, and the lower one under it), and studs between the bottom plate and the lower top plate, made in a scope once
// the leg's length is found, each butted on both (its top on the slope, where it slopes). At the corner the plates lap:
// the second leg rises away from the corner, so the first laps its upper top plate over the second's lower one, seated
// on it (cut where the second's rises into it). Once the first leg's studs are made, a brace is let in across them, and a folded steel
// bracket (a library part, ./bracket.ts) is bolted to one of them on the inside, the bolt's length chosen from the
// grip once that is found. The walls stand on a slab (the plane of the outline's corners), and the bracket carries a
// shelf's live load at a corner of its base.

import { part, node, plane, neg, frame, placed, place, box, span, letIn, bolted, support, load, cases, SPF, mm, N, type Length, type PartBase, type Piece } from "topo-part";
import { bracket } from "./bracket";
import { perimeter } from "topo-part/walls";

// (Metric 2x4 and 1x4, 38 by 89 and 19 by 89.)
const twoByFour = box("2x4", { thickness: mm(38), depth: mm(89), material: SPF.No1No2 });
const oneByFour = box("1x4", { thickness: mm(19), depth: mm(89), material: SPF.No1No2 });

// A brace across a leg, from `low` to `high` in its frame (x along it, y up), its outside flush with the studs'
// outside (the leg's outside face, its z = 0): let into each stud it crosses, as deep as it is thick. One it misses is
// not cut.
function brace(leg: PartBase, studs: Piece[], low: [Length, Length], high: [Length, Length]) {
  const ends = { low: node(low[0], low[1], 0), high: node(high[0], high[1], 0) };
  const member = span(ends.low, ends.high, neg(leg.z), oneByFour);
  return { ends, member, letIn: studs.map((st) => letIn(member, st)) };
}

// The bracket on a stud's inside face (its +z), `height` up it and across its middle: its leg flat on the face, the
// base standing out from it. The stud's corners make the frame it is in: x into the stud, y along the run, z up it.
// One M10 bolt through the leg's hole and the stud, its head on the leg.
function bracketOn(stud: Piece, height: Length) {
  const b = place(bracket("bracket"), placed({ origin: [mm(-65), mm(-1), height] }));
  const mount = part("mount", { bracket: b }, { at: frame(stud.corners["001"], { x: stud.corners["000"], xy: stud.corners["011"] }) });
  const leg = b.bend.turn.leg;
  const bolt = bolted("bolt", {
    at: frame(leg.centres[0], { x: leg.rims[0], y: neg(leg.y) }),
    through: leg,
    into: stud,
    d: mm(10),
    hole: mm(11),
    head: mm(-3),
    lengths: mm.each([60, 70, 80, 90, 100, 110, 120, 130, 140, 150]),
  });
  // (A shelf on it: 200 N down at its base's far corner.)
  return { mount, bolt, shelf: load(b.base.corners[0], [0, 0, N(-200)], cases.live) };
}

// The corners of the walls' outsides and their heights there, as they were set out.
const outline = mm.each([[0, -44.5], [3044.5, -44.5], [3044.5, 2400]] as [number, number][]);
const heights = mm.each([2440, 2440, 2740]);
const leg = { spacing: mm(406), stock: twoByFour };
const w = perimeter("walls", {
  corners: outline.map(([x, y]) => node(x, y, 0)),
  closed: false,
  legs: [
    { ...leg, height: heights[0], onStuds: (l, studs) => ({ brace: brace(l, studs, [mm(300), mm(150)], [mm(2500), mm(2200)]), ...bracketOn(studs[2], mm(1200)) }) },
    { ...leg, height: [heights[1], heights[2]] },
  ],
});

export default part("corner", () => ({
  walls: w,
  slab: support(plane(w.corners[0], w.corners[1], w.corners[2])),
}));
