// Benchmark 6 on the part core: AWC Span Tables for Joists and Rafters (2024), Table F-2 (as
// examples/benchmarks/06-awc-span-table.ts): the floor joist of benchmark 1 at the span the table gives a 2x10 at
// 16" on centre, 16'-5", its 10 psf of dead load including its own weight (so none is added for it).
//
// A 2x10 Douglas Fir-Larch No.2 floor joist at 16" on centre spans 16'-5" (centre to centre of bearing) between two
// 4x4 posts, 8' high, standing on the slab. Floor dead load 10 psf, live load 40 psf, along the joist by its spacing.
// The joist runs over the whole of each post's top, so it bears on all 3½" of it, as the hand calculation has it.

import { part, bears, node, span, plane, support, lineLoad, designBasis, cases, roles, lumber, nominal, DFL, add, sub, mul, div, ft, ftIn, mm, plf, type Length } from "topo-part";

const length = ftIn(16, 5), height = ft(8), spacing = 16 / 12;

const post = lumber(nominal["4x4"], DFL.No2);
const joist = lumber(nominal["2x10"], DFL.No2);

export default part("span table joist", (self) => {
  // Each post up from the slab, its back face 1¾" behind its centre (its y along the joist), centred across.
  const back = div(post.depth, 2);
  const feet = [ft(0), length].map((x: Length) => ({ foot: node(sub(x, back), 0, 0), top: node(sub(x, back), 0, height) }));
  const posts = feet.map((f) => span(f.foot, f.top, self.x, post));
  // The joist on edge (its depth up its z, its thickness across), its underside on the posts' tops, from the
  // outside of one to the outside of the other.
  const z = add(height, div(joist.depth, 2)), y = mul(-1, div(joist.thickness, 2));
  const ends = [node(mul(-1, back), y, z), node(add(length, back), y, z)];
  const j = span(ends[0], ends[1], self.y, joist, { role: roles.joist });
  const ground = [node(0, 0, 0), node(mm(1), 0, 0), node(0, mm(1), 0)];
  return {
    feet, posts, ends, joist: j, ground,
    // (Its load path: the joist on its posts.)
    rests: posts.map((p) => bears(j, p)),
    slab: support(plane(ground[0], ground[1], ground[2])),
    basis: designBasis({ selfWeight: false }),
    floorDead: lineLoad(j, [0, 0, plf(-10 * spacing)], cases.dead),
    floorLive: lineLoad(j, [0, 0, plf(-40 * spacing)], cases.live),
  };
});
