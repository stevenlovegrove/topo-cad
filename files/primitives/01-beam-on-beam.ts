// A beam on two posts, and two joists crossing it (examples/primitives/01-beam-on-beam.ts), written on the part core.
//
// What rests on what is found where their faces touch: each joist's underside on the beam's top, the beam's underside
// on the posts' tops, the posts' feet on the ground. (The original says which member rests on which, and where each
// member's axis is in its section; here neither is said, and both joists rest alike.)

import { part, node, span, support, plane, lumber, nominal, DFL, roles, add, sub, mul, div, inch, ft } from "topo-part";

const post = lumber(nominal["4x4"], DFL.No2);
const beam = lumber(nominal["4x8"], DFL.No2);
const joist = lumber(nominal["2x8"], DFL.No2);

export default part("beam on beam", (root) => {
  const h = ft(3);
  // (A box's back face is on its path and its depth either side of it: the posts' middles on their nodes, the beam's
  // underside on the posts' tops, the joists' undersides on its top.)
  const back = mul(-1, div(post.thickness, 2));
  const at = (x: ReturnType<typeof ft>, z: ReturnType<typeof ft>) => node(x, back, z);
  const feet = [at(ft(0), ft(0)), at(ft(8), ft(0))];
  const heads = [at(ft(0), h), at(ft(8), h)];
  const posts = [0, 1].map((k) => span(feet[k], heads[k], root.y, post, { role: roles.post }));
  const axis = add(h, div(beam.depth, 2));
  const ends = [at(mul(-1, div(post.depth, 2)), axis), at(add(ft(8), div(post.depth, 2)), axis)];
  const b = span(ends[0], ends[1], root.y, beam, { role: roles.beam });
  const top = add(h, beam.depth, div(joist.depth, 2));
  const bearings = [ft(2), ft(6)].map((x) => [node(sub(x, div(joist.thickness, 2)), ft(-3), top), node(sub(x, div(joist.thickness, 2)), ft(3), top)]);
  const joists = bearings.map(([a, z]) => span(a, z, root.x, joist, { role: roles.joist }));
  const ground = [node(0, 0, 0), node(inch(1), 0, 0), node(0, inch(1), 0)];
  return { feet, heads, posts, ends, beam: b, bearings, joists, ground, slab: support(plane(ground[0], ground[1], ground[2])) };
});
