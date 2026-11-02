// Benchmark 2 on the part core: a garage-door header carrying truss point loads (as
// examples/benchmarks/02-header-point-loads.ts).
//
// A (2) 2x12 DF-L No.2 header spans 16'-0" (centre to centre of bearing) on 4x4 posts, 7' high. Roof trusses at 24"
// on centre land on it at 2', 4', … 14', each bringing 400 lb dead and 500 lb snow. The header runs over the whole of
// each post's top.

import { part, bears, node, span, plane, support, load, cases, lumber, nominal, DFL, add, sub, mul, div, ft, mm, lbf, type Length } from "topo-part";

const length = ft(16), height = ft(7);

const post = lumber(nominal["4x4"], DFL.No2);
const header = lumber(nominal["2x12"], DFL.No2, { plies: 2 });

export default part("header", (self) => {
  const back = div(post.depth, 2);
  const feet = [ft(0), length].map((x: Length) => ({ foot: node(sub(x, back), 0, 0), top: node(sub(x, back), 0, height) }));
  const posts = feet.map((f) => span(f.foot, f.top, self.x, post));
  // On edge, its plies side by side across it, centred on the posts; its underside on their tops.
  const z = add(height, div(header.depth, 2)), y = mul(-1, header.thickness);
  const ends = [node(mul(-1, back), y, z), node(add(length, back), y, z)];
  const h = span(ends[0], ends[1], self.y, header);
  // Where each truss lands: on its top, across its middle.
  const trusses = [2, 4, 6, 8, 10, 12, 14].map((x) => node(ft(x), 0, add(height, header.depth)));
  const ground = [node(0, 0, 0), node(mm(1), 0, 0), node(0, mm(1), 0)];
  return {
    feet, posts, ends, header: h, trusses, ground,
    // (Its load path: the header on its posts.)
    rests: posts.map((p) => bears(h, p)),
    slab: support(plane(ground[0], ground[1], ground[2])),
    dead: trusses.map((t) => load(t, [0, 0, lbf(-400)], cases.dead)),
    snow: trusses.map((t) => load(t, [0, 0, lbf(-500)], cases.snow)),
  };
});
