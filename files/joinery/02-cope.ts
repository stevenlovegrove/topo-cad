// A cope (examples/joinery/02-cope.ts), written on the part core: a tube's end fitted to the round it lands on.
//
// Two branches on a larger tube, one square and one leaning. Each is made from a place on the main tube, so it lands
// there, joined by the rule of the part (`cope()`): it runs to the main tube's axis, and what of it is within the main
// tube's outside is taken away. Its walls end exactly where they meet the main tube (a curve on each, by the angle
// round it), and its end is the main tube's own surface between them: a saddle that sits on it. Welded: rigid.
//
// (The original's third branch, cut flat to touch along a line, is not made here: a flat end on a round is not yet.)

import { part, node, member, sections, materials, joinery, lands, hold, designBasis, mm } from "topo-part";
import { cope } from "topo-part/tubes";

const main = sections.tube(mm(48.3), mm(3.2)), branch = sections.tube(mm(33.7), mm(2.6));
const steel = materials.chromoly4130N;

export default part("branches on a tube", (f) => {
  const ends = [node(0, 0, 0), node(mm(900), 0, 0)];
  const tube = member("main", main, steel, { from: ends[0], to: ends[1], y: f.y });
  // (The leaning branch's top 0.7 along for each 1 up.)
  const tops = [node(mm(200), 0, mm(300)), node(mm(660), 0, mm(300))];
  return {
    ends, tube, tops,
    square: member("branch", branch, steel, { from: tube.along(mm(200)), to: tops[0], y: f.y }),
    leaning: member("branch", branch, steel, { from: tube.along(mm(450)), to: tops[1], y: f.y }),
    holds: [hold(ends[0], "fixed"), hold(ends[1], "fixed")],
    // (Branches held at one end: a frame, not a takedown.)
    design: designBasis({ analysis: "frame" }),
  };
}, { joinery: joinery(lands(sections.tube, sections.tube, cope())) });
