// A housing (examples/joinery/05-housed.ts), written on the part core: the lander's end runs into the other, which is
// cut out to take it.
//
// A shelf between two uprights (a dado, to a cabinetmaker). The shelf's line runs between the uprights' axes; each end
// runs 1/2" into the upright's face (`endOn` with a gap of less than none), and each upright is cut by the shelf (a
// housing the size of its end, 1/2" deep). The shelf comes out an inch longer than the clear distance between the
// uprights. All three 2x10, their width front to back; the uprights 3' tall and 3' apart, the shelf 2' up.

import { part, node, span, endOn, facing, cut, hold, neg, roles, lumber, nominal, DFL, sub, mul, div, inch, ft } from "topo-part";

const twoByTen = lumber(nominal["2x10"], DFL.No2);
const into = inch(-0.5);

export default part("shelf in two uprights", (f) => {
  // (Each upright's line half its thickness back along x, laid from it; its width along y. The shelf's laid along z
  // from a line half its thickness under its middle, its width along y.)
  const h = div(twoByTen.thickness, 2);
  const y = mul(-1, div(twoByTen.depth, 2));
  const feet = [node(mul(-1, h), y, 0), node(sub(ft(3), h), y, 0)], tops = [node(mul(-1, h), y, ft(3)), node(sub(ft(3), h), y, ft(3))];
  const uprights = feet.map((foot, i) => span(foot, tops[i], f.x, twoByTen, { role: roles.upright }));
  const ends = [node(0, y, sub(ft(2), h)), node(ft(3), y, sub(ft(2), h))];
  const shelf = span(ends[0], ends[1], f.z, twoByTen, { role: roles.shelf });
  const faces = [facing(uprights[0], f.x), facing(uprights[1], neg(f.x))];
  return {
    feet, tops, uprights, ends, shelf, faces,
    ins: [endOn(shelf, faces[0], { gap: into }), endOn(shelf, faces[1], { gap: into })],
    housings: [cut(uprights[0], { tool: "body", part: shelf }, "housing"), cut(uprights[1], { tool: "body", part: shelf }, "housing")],
    holds: [hold(feet[0], "fixed"), hold(feet[1], "fixed")],
  };
});
