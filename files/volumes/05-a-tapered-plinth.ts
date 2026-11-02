// A tapered plinth (examples/volumes/05-a-tapered-plinth.ts), written on the part core: a loft, between a loop of
// corners and a smaller copy of it, higher up.
//
// A plinth 40 square at its foot and 24 square at its top, 30 high, its top moved back 4. Each side is between a run of
// the foot and the run of the top that matches it, parallel to it, so it is flat: a loft of matched corners whose runs
// stay parallel is a frustum, exactly (its volume h (A₁ + A₂ + √(A₁A₂)) / 3, wherever its top is over its foot). Turn
// the top and the sides would twist: a ruled face, refused until such faces are made.

import { part, node, loft, materials, mm, type Node } from "topo-part";

/** A square's corners, anticlockwise from its −x −y one: half its side, where its middle is along x, how high. */
const square = (half: number, x: number, z: number): Node[] =>
  [[-1, -1], [1, -1], [1, 1], [-1, 1]].map(([i, j]) => node(mm(x + i * half), mm(j * half), mm(z)));

export default part("a tapered plinth", () => {
  const [foot, top] = [square(20, 0, 0), square(12, -4, 30)];
  const nodes = Object.fromEntries([...foot.map((n, i) => [`foot ${"abcd"[i]}`, n]), ...top.map((n, i) => [`top ${"abcd"[i]}`, n])]);
  const plinth = part("plinth", nodes, { kind: "solid" });
  plinth.shapes = [loft(foot.map((node) => ({ node })), top.map((node) => ({ node })), materials.PLA)];
  return { plinth };
});
