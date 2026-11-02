// A butt (examples/joinery/01-butt.ts), written on the part core: the member that lands is cut to what it lands onto.
//
// A rail between two posts. Its line runs between the posts' axes; each end is on the face of the post it meets
// (`endOn`), so the rail comes out as long as the clear distance between the posts without anyone working that out:
//
// - at the left post it is cut tight to the face;
// - at the right post it is cut 1/8" short of it (`gap`): room to fit it, said as a fit (a gap, open).
//
// Each end is toe-nailed (two 8d). The posts are 4x4, 4' tall and 4' apart; the rail a 2x6, 3' up.

import { part, node, span, endOn, facing, nailed, fit, hold, neg, roles, lumber, nominal, DFL, sub, mul, div, inch, ft } from "topo-part";

const fourByFour = lumber(nominal["4x4"], DFL.No2);
const twoBySix = lumber(nominal["2x6"], DFL.No2);
const gap = inch(1 / 8);

export default part("rail between posts", (f) => {
  // (Each post's line half a 4x back from its middle along x, laid from it; the rail's half a 2x back along y.)
  const h = div(fourByFour.thickness, 2);
  const feet = [node(mul(-1, h), 0, 0), node(sub(ft(4), h), 0, 0)], tops = [node(mul(-1, h), 0, ft(4)), node(sub(ft(4), h), 0, ft(4))];
  const posts = feet.map((foot, i) => span(foot, tops[i], f.x, fourByFour, { role: roles.post }));
  const y = mul(-1, div(twoBySix.thickness, 2));
  const ends = [node(0, y, ft(3)), node(ft(4), y, ft(3))];
  const rail = span(ends[0], ends[1], f.y, twoBySix, { role: roles.rail });
  // (The faces of the posts toward each other.)
  const faces = [facing(posts[0], f.x), facing(posts[1], neg(f.x))];
  return {
    feet, tops, posts, ends, rail, faces,
    butts: [endOn(rail, faces[0]), endOn(rail, faces[1], { gap })],
    room: fit(rail, posts[1], { gap, name: "room to fit it" }),
    nails: [nailed(rail, posts[0], { count: 2, penny: 8, method: "toe" }), nailed(rail, posts[1], { count: 2, penny: 8, method: "toe" })],
    holds: [hold(feet[0], "fixed"), hold(feet[1], "fixed")],
  };
});
