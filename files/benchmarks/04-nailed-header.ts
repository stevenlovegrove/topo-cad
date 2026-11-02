// Benchmark 4 on the part core: a header hung on nails, no jack studs (as examples/benchmarks/04-nailed-header.ts).
//
// A (2) 2x8 DF-L No.2 header between two 4x4 posts 6'-0" apart (centre to centre), 7' high, its top 6" under theirs:
// butted on their inside faces and held only by (4) 16d common nails end-nailed through each post into its end grain,
// as many garage headers are built. It carries 100 plf dead and 150 plf snow. Its span is the posts' clear 5'-8½",
// where the present pipeline's analytical header runs 6'-0" from centre to centre.

import { part, node, span, plane, support, lineLoad, butt, nailed, cases, lumber, nominal, DFL, sub, mul, div, ft, inch, mm, plf, type Length } from "topo-part";

const length = ft(6), height = ft(7);

const post = lumber(nominal["4x4"], DFL.No2);
const header = lumber(nominal["2x8"], DFL.No2, { plies: 2 });

export default part("opening", (self) => {
  const back = div(post.depth, 2);
  const feet = [ft(0), length].map((x: Length) => ({ foot: node(sub(x, back), 0, 0), top: node(sub(x, back), 0, height) }));
  const posts = feet.map((f) => span(f.foot, f.top, self.x, post));
  // On edge, its plies side by side, centred on the posts; its top 6" under theirs.
  const z = sub(sub(height, inch(6)), div(header.depth, 2)), y = mul(-1, header.thickness);
  const ends = [node(back, y, z), node(sub(length, back), y, z)];
  const h = span(ends[0], ends[1], self.y, header);
  const ground = [node(0, 0, 0), node(mm(1), 0, 0), node(0, mm(1), 0)];
  return {
    feet, posts, ends, header: h, ground,
    butts: [butt(h, posts[0], { on: posts[0].faces["+y"] }), butt(h, posts[1], { on: posts[1].faces["-y"] })],
    nails: posts.map((p) => nailed(h, p, { count: 4, penny: 16, method: "end", name: "Header to post" })),
    slab: support(plane(ground[0], ground[1], ground[2])),
    dead: lineLoad(h, [0, 0, plf(-100)], cases.dead),
    snow: lineLoad(h, [0, 0, plf(-150)], cases.snow),
  };
});
