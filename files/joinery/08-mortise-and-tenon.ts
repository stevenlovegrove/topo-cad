// A mortise and tenon (examples/joinery/08-mortise-and-tenon.ts), written on the part core: the lander's end is reduced,
// and goes into a hole its size.
//
// A frame of two 4x4 posts, 3' apart, and two rails between them:
//
// - the lower rail (a 2x4, 1' up) has a tenon 2" long at each end, a third of its thickness, in a blind mortise;
// - the upper rail's (a 2x6, 2'-6" up) go right through the posts, and are narrower than the rail (3 1/2" of its 5 1/2"),
//   so the rail has shoulders all round.
//
// Each tenon is the rail's end run into the post (`endOn` with a gap of less than none) and its cheeks and shoulders cut
// away (boxes, in the rail's frame); each mortise the post cut by a box of the tenon's size there, in the rail's frame.
// (The boxes are laid out in millimetres, said where each length is read and made.)

import { part, node, span, endOn, facing, cut, hold, neg, roles, lumber, nominal, DFL, mul, inch, ft, mm, as, type Length, type PartBase } from "topo-part";

const fourByFour = lumber(nominal["4x4"], DFL.No2);
const twoByFour = lumber(nominal["2x4"], DFL.No2);
const twoBySix = lumber(nominal["2x6"], DFL.No2);
// (In millimetres: the posts' spacing, half a post, a rail's thickness.)
const [span_, half, t] = [as(ft(3), mm), as(fourByFour.depth, mm) / 2, as(twoByFour.thickness, mm)];
const MM = (x: number) => mm(x);

/** The cuts that leave a tenon at each end of `rail` (its frame: x along it from the first post's axis, y across its
 *  thickness from 0, z across its depth about its middle): `long` into the post, a third of its thickness, `wide` of
 *  its depth `deep`. */
function tenons(rail: PartBase, long: number, wide: number, deep: number) {
  const ends = [[-long - half - 10, half], [span_ - half, span_ + half + long + 10]];
  const away = (x0: number, x1: number) => [
    cut(rail, { tool: "box", frame: rail, lo: [MM(x0), MM(-1), MM(-deep)], hi: [MM(x1), MM(t / 3), MM(deep)] }, "cheek"),
    cut(rail, { tool: "box", frame: rail, lo: [MM(x0), MM((2 * t) / 3), MM(-deep)], hi: [MM(x1), MM(t + 1), MM(deep)] }, "cheek"),
    // (The shoulders only between the cheeks: what of the waste the cheeks did not take.)
    ...(wide < deep ? [cut(rail, { tool: "box", frame: rail, lo: [MM(x0), MM(t / 3), MM(wide / 2)], hi: [MM(x1), MM((2 * t) / 3), MM(deep)] }, "shoulder"), cut(rail, { tool: "box", frame: rail, lo: [MM(x0), MM(t / 3), MM(-deep)], hi: [MM(x1), MM((2 * t) / 3), MM(-wide / 2)] }, "shoulder")] : []),
  ];
  return ends.flatMap(([x0, x1]) => away(x0, x1));
}

/** The mortises for `rail`'s tenons (as `tenons` makes them) in the posts: at each end, a box of the tenon's size, as far
 *  into the post as it goes. */
function mortises(posts: PartBase[], rail: PartBase, long: number, wide: number) {
  const ins = [[half - long, half + 1], [span_ - half - 1, span_ - half + long]];
  return posts.map((post, i) => cut(post, { tool: "box", frame: rail, lo: [MM(ins[i][0]), MM(t / 3), MM(-wide / 2)], hi: [MM(ins[i][1]), MM((2 * t) / 3), MM(wide / 2)] }, "mortise"));
}

export default part("tenoned frame", (f) => {
  // (Each post's line half a 4x back from its middle along x, laid from it, its depth about it; each rail's half its
  // thickness back along y.)
  const feet = [node(MM(-half), 0, 0), node(MM(span_ - half), 0, 0)], tops = [node(MM(-half), 0, ft(3)), node(MM(span_ - half), 0, ft(3))];
  const posts = feet.map((foot, i) => span(foot, tops[i], f.x, fourByFour, { role: roles.post }));
  const at = (x: number, z: Length) => node(MM(x), MM(-t / 2), z);
  const lowEnds = [at(0, ft(1)), at(span_, ft(1))], highEnds = [at(0, ft(2.5)), at(span_, ft(2.5))];
  const low = span(lowEnds[0], lowEnds[1], f.y, twoByFour, { role: roles.rail });
  const high = span(highEnds[0], highEnds[1], f.y, twoBySix, { role: roles.rail });
  const faces = [facing(posts[0], f.x), facing(posts[1], neg(f.x))];
  const [post, two, deep4, deep6, wide6] = [as(fourByFour.depth, mm), as(inch(2), mm), as(twoByFour.depth, mm), as(twoBySix.depth, mm), as(inch(3.5), mm)];
  return {
    feet, tops, posts, lowEnds, highEnds, low, high, faces,
    // Blind: 2" in. Through: to the posts' far faces.
    ends: [endOn(low, faces[0], { gap: mul(-1, inch(2)) }), endOn(low, faces[1], { gap: mul(-1, inch(2)) }), endOn(high, faces[0], { gap: mul(-1, fourByFour.depth) }), endOn(high, faces[1], { gap: mul(-1, fourByFour.depth) })],
    tenons: [...tenons(low, two, deep4, deep4), ...tenons(high, post, wide6, deep6)],
    mortises: [...mortises(posts, low, two, deep4), ...mortises(posts, high, post, wide6)],
    holds: [hold(feet[0], "fixed"), hold(feet[1], "fixed")],
  };
});
