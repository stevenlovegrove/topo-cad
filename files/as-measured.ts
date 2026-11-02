// A deck's frame as built, against its design: laid out 2400 by 1200 mm and square, then taped.
//
// Its corners are free, each started where the design has it and held there by a fallback: the design, the
// weakest thing said of it. Its sides and diagonals were taped, each to within 3 mm: readings, firmer than the
// design. The near corner is the frame's origin and the far one along its front is on its x: so what is found is
// the frame's shape, not where it lies. Six tapes of five numbers do not quite agree; the solve takes them as
// nearly as each may be off, and says what gave way: the design where the frame was built otherwise, and each tape
// by how far it disagrees with the rest.

import { part, node, free, read, fallback, distance, mm } from "topo-part";

const design = { length: mm(2400), width: mm(1200) };
// (What each tape read, in millimetres.)
const taped = { front: 2405, right: 1198, back: 2403, left: 1201, rising: 2690, falling: 2685 };
const tolerance = mm(3);

export default part("deck", () => {
  const x1 = free(design.length), x2 = free(design.length), y2 = free(design.width), x3 = free(mm(0)), y3 = free(design.width);
  const corners = [node(0, 0, 0), node(x1, 0, 0), node(x2, y2, 0), node(x3, y3, 0)];
  const [a, b, c, d] = corners;
  return {
    corners,
    design: {
      length: fallback(x1, design.length),
      "back length": fallback(x2, design.length),
      width: fallback(y2, design.width),
      square: fallback(x3, 0),
      "left width": fallback(y3, design.width),
    },
    taped: {
      front: read(distance(a, b, mm(taped.front)), tolerance),
      right: read(distance(b, c, mm(taped.right)), tolerance),
      back: read(distance(c, d, mm(taped.back)), tolerance),
      left: read(distance(d, a, mm(taped.left)), tolerance),
      rising: read(distance(a, c, mm(taped.rising)), tolerance),
      falling: read(distance(b, d, mm(taped.falling)), tolerance),
    },
  };
});
