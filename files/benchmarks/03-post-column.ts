// Benchmark 3 on the part core: an axially loaded post (as examples/benchmarks/03-post-column.ts).
//
// A 4x4 DF-L No.2 post, 8'-0" tall, standing on the slab, not braced along its height, carries 2,000 lb dead and
// 3,000 lb live at its top.

import { part, node, span, plane, support, load, cases, lumber, nominal, DFL, mul, div, ft, mm, lbf } from "topo-part";

const stock = lumber(nominal["4x4"], DFL.No2);

export default part("post", (self) => {
  const back = mul(-1, div(stock.depth, 2));
  const foot = node(back, 0, 0), top = node(back, 0, ft(8));
  const post = span(foot, top, self.x, stock);
  // (Its top's middle.)
  const head = node(0, 0, ft(8));
  const ground = [node(0, 0, 0), node(mm(1), 0, 0), node(0, mm(1), 0)];
  return {
    foot, top, post, head, ground,
    slab: support(plane(ground[0], ground[1], ground[2])),
    dead: load(head, [0, 0, lbf(-2000)], cases.dead),
    live: load(head, [0, 0, lbf(-3000)], cases.live),
  };
});
