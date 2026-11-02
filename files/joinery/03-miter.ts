// A mitre (examples/joinery/03-miter.ts), written on the part core: both ends cut on the plane where their insides and
// their outsides meet.
//
// A frame lying flat, a different corner at each end of its long side:
//
// - two boards alike, square to each other (a 2x4 and a 2x4): the plane is the bisector, 45 degrees on each;
// - a 2x4 and a 2x6: the plane runs from where their inner edges meet to where their outer edges meet, which is not
//   the bisector. Each is cut to its own angle, and the cut faces are the same length, so they close.
//
// No one gives the angles: each plane is through the corners the boards' edges make, and each board's end is on it
// (`miter`). The boards' lines are their middles, meeting at the frame's corners 3' by 2'; each mitre nailed (two 8d,
// end-nailed: the original said only that they are nailed). The two ends of the frame's short sides are held.

import { part, node, plane, span, miter, nailed, hold, roles, lumber, nominal, DFL, add, sub, mul, div, ft, type Length0 } from "topo-part";

const twoByFour = lumber(nominal["2x4"], DFL.No2);
const twoBySix = lumber(nominal["2x6"], DFL.No2);

export default part("mitred frame", (f) => {
  // (Each board flat: laid up from its line, its width across, centred on it.)
  const [w4, w6] = [div(twoByFour.depth, 2), div(twoBySix.depth, 2)];
  const at = (x: Length0, y: Length0) => node(x, y, 0);
  const corners = { a: at(0, ft(2)), b: at(0, 0), c: at(ft(3), 0), d: at(ft(3), ft(2)) };
  const side = span(corners.a, corners.b, f.z, twoByFour, { role: roles.rail });
  const long = span(corners.b, corners.c, f.z, twoByFour, { role: roles.rail });
  const wide = span(corners.c, corners.d, f.z, twoBySix, { role: roles.rail });
  // The planes, upright: through where the boards' outer edges meet and where their inner edges meet (each a mark on
  // the floor and one over it), facing into the first board of the corner.
  // (The first board is to the left of the way from the outer corner to the inner one, seen from above.)
  const upright = (outer: [Length0, Length0], inner: [Length0, Length0]) => {
    const marks = [at(...outer), at(...inner), node(outer[0], outer[1], ft(1))];
    return { marks, plane: plane(marks[0], marks[2], marks[1]) };
  };
  const cut = {
    // (At b: the side's outer edge x = −w/2 and the long side's y = −w/2; their inner edges at +w/2.)
    b: upright([mul(-1, w4), mul(-1, w4)], [w4, w4]),
    // (At c: the long side's outer edge y = −w4 and the wide's x = 3' + w6; inner, y = w4 and x = 3' − w6.)
    c: upright([add(ft(3), w6), mul(-1, w4)], [sub(ft(3), w6), w4]),
  };
  return {
    corners, side, long, wide, cut,
    mitres: { b: miter(side, long, { plane: cut.b.plane }), c: miter(long, wide, { plane: cut.c.plane }) },
    nails: [nailed(side, long, { count: 2, penny: 8, method: "end" }), nailed(long, wide, { count: 2, penny: 8, method: "end" })],
    holds: [hold(corners.a, "fixed"), hold(corners.d, "fixed")],
  };
});
