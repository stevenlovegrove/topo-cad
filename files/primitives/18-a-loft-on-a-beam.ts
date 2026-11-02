// A floor over part of a room: a loft, on the back wall and a beam (examples/primitives/18-a-loft-on-a-beam.ts), written
// on the part core.
//
// A floor sits on what is under it, and that may be a beam as well as a wall. A beam crosses the room half way back, on
// a post at each end: the posts stand on the slab, and are as tall as leaves the beam's top at the walls' top. The
// loft's joists run from the beam to the back wall, each resting on the back wall's plates and on the beam where it
// crosses them, its bands on the side walls. The floor is given on plan only: it slides up and down, found where it
// rests on the walls. Walls 12' by 16' (their outsides), 8' tall, of 2x4 at 16"; a 4x10 beam on 4x4 posts just inside
// the side walls; the loft's joists 2x8 at 16". Dead load 10 psf, live 30 psf.

import { part, node, free, plane, span, support, areaLoad, cases, roles, must, on, placed, joints, lumber, nominal, DFL, add, sub, div, inch, ft, mm, psf } from "topo-part";
import { floor } from "topo-part/framing";
import { perimeter } from "topo-part/walls";

const stud = lumber(nominal["2x4"], DFL.No2);
const joist = lumber(nominal["2x8"], DFL.No2);
const post = lumber(nominal["4x4"], DFL.No2);
const girder = lumber(nominal["4x10"], DFL.No2);
const [wide, deep, half, height] = [ft(12), ft(16), ft(8), ft(8)];
// (The posts' middles just inside the side walls; the beam's underside a 4x10's depth under the walls' top.)
const [x0, x1, under] = [inch(3.5 + 1.75), sub(wide, inch(3.5 + 1.75)), sub(height, girder.depth)];

export default part("a loft on a beam", (s) => {
  const w = perimeter("walls", { corners: [node(0, 0, 0), node(wide, 0, 0), node(wide, deep, 0), node(0, deep, 0)], legs: { height, spacing: inch(16), stock: stud } });
  // The beam on its posts: each post from the slab to the beam's underside; each laid from its line along its y (a post's
  // the walls' x, the beam's their y), so its line is half a 4x back from its middle.
  const beam = part("beam", (b) => {
    const h = inch(1.75);
    const feet = [node(sub(x0, h), half, 0), node(sub(x1, h), half, 0)], tops = [node(sub(x0, h), half, under), node(sub(x1, h), half, under)];
    const axis = add(under, div(girder.depth, 2));
    const ends = [node(sub(x0, h), sub(half, h), axis), node(add(x1, h), sub(half, h), axis)];
    return {
      feet, tops, ends,
      posts: feet.map((f, i) => span(f, tops[i], b.x, post, { role: roles.post })),
      beam: span(ends[0], ends[1], b.y, girder, { role: roles.beam }),
    };
  });
  // The loft: its joists along y from over the beam to the back wall's outside, found where it rests.
  const rise = free(mm(0));
  const start = sub(half, inch(1.75));
  const corner = node(0, start, rise);
  const loft = floor("loft", { width: wide, length: sub(deep, start), spacing: inch(16), stock: joist, at: placed({ origin: [0, start, 0], joint: joints.slider([0, 0, 1], rise) }) });
  const ground = [node(0, 0, 0), node(mm(1), 0, 0), node(0, mm(1), 0)];
  return {
    walls: w,
    beam,
    corner,
    loft,
    ground,
    slab: support(plane(ground[0], ground[1], ground[2])),
    dead: areaLoad(loft, psf(10), cases.dead),
    live: areaLoad(loft, psf(30), cases.live),
    says: {
      "on the walls": must(on(corner, w.legs[2].topPlate.faces["-y"])),
    },
  };
});
