// A birdsmouth (examples/joinery/06-birdsmouth.ts), written on the part core: a member that passes over another, the
// other taken out of it.
//
// A rafter over a wall plate. The rafter does not end at the plate, it runs on to its tail; so where it sits on the
// plate it is notched by it (`letIn`: what of the rafter is where the plate is, taken out): a level seat cut on the
// plate's top and a plumb heel cut at its outer face. No one gives the seat or the heel: they follow from where the
// rafter is. Its ends are cut plumb (`endOn` a plumb plane at each).
//
// Where it is: the plate's top outer corner is a fifth of the rafter's depth up into it, square to its underside (as
// the original's: its axis a fifth of its depth above its underside, through the corner). The plate is a 2x4 flat,
// its width across the wall, its top 8' up; the rafter a 2x6 at 6 in 12, from its tail 9" outside the wall to 18"
// inside it, toe-nailed to the plate (three 8d). x is in from the wall's outside, y along the wall, z up.

import { part, node, plane, span, endOn, letIn, nailed, hold, neg, roles, lumber, nominal, DFL, add, mul, inch, ft } from "topo-part";

const twoByFour = lumber(nominal["2x4"], DFL.No2);
const twoBySix = lumber(nominal["2x6"], DFL.No2);
const pitch = 6 / 12;
const [tail, ridge] = [inch(-9), inch(18)];

export default part("rafter on a plate", (f) => {
  // The plate: along the wall, its line along its top through the middle of its width (laid down from it: its y down,
  // its width across, centred).
  const plateTop = ft(8);
  const plateEnds = [node(mul(0.5, twoByFour.depth), inch(-6), plateTop), node(mul(0.5, twoByFour.depth), inch(6), plateTop)];
  const plate = span(plateEnds[0], plateEnds[1], neg(f.z), twoByFour, { role: roles.topPlate });
  // The rafter: its line its middle, so far square to its underside from the plate's top outer corner (the origin's
  // line along the wall): up from its slope by its depth's half less a fifth, and back half its thickness along y.
  const [c, s] = [1 / Math.hypot(1, pitch), pitch / Math.hypot(1, pitch)];
  const up = mul(0.3, twoBySix.depth);
  const at = (x: ReturnType<typeof inch>) => node(add(x, mul(-s, up)), mul(-0.5, twoBySix.thickness), add(add(plateTop, mul(pitch, x)), mul(c, up)));
  const ends = [at(tail), at(ridge)];
  const rafter = span(ends[0], ends[1], f.y, twoBySix, { role: roles.rafter });
  // (A plumb plane at each end, across the wall, through its end: facing into the rafter.)
  const plumb = (x: ReturnType<typeof inch>, into: number) => {
    const marks = [node(x, 0, 0), node(x, 0, ft(1)), node(x, ft(1), 0)];
    return { marks, plane: into > 0 ? plane(marks[0], marks[2], marks[1]) : plane(marks[0], marks[1], marks[2]) };
  };
  const cuts = { tail: plumb(tail, 1), ridge: plumb(ridge, -1) };
  return {
    plateEnds, plate, ends, rafter, cuts,
    trimmed: [endOn(rafter, cuts.tail.plane), endOn(rafter, cuts.ridge.plane)],
    birdsmouth: letIn(plate, rafter),
    nails: nailed(rafter, plate, { count: 3, penny: 8, method: "toe" }),
    holds: [hold(plateEnds[0], "fixed"), hold(plateEnds[1], "fixed"), hold(ends[1], "pinned")],
  };
});
