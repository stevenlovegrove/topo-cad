// A block under a shelf (examples/volumes/01-a-block-under-a-shelf.ts), written on the part core: the simplest volume.
//
// A shelf of 2 mm aluminium, 100 by 60, tilted 12 degrees, its underside 40 up at its low edge; and a printed block
// under it, 40 square, standing on the floor. The block is an extrusion of its square up its z, drawn 30 tall, its top
// end on the shelf's underside (`endOn`): so its top takes the shelf's slope and its sides stay upright. Tilt the shelf
// further, or raise it, and the block follows. Its faces are its square's runs (`o0`, …) and its ends (`start`, the
// floor; `end`, under the shelf).

import { part, node, plate, placed, polygon, extrusion, endOn, materials, mm } from "topo-part";

const tilt = (12 * Math.PI) / 180;

export default part("a block under a shelf", () => {
  const shelf = plate("shelf", mm(100), mm(60), mm(2), materials.aluminium6061T6, [], {
    at: placed({ origin: [0, 0, mm(40)], x: [Math.cos(tilt), 0, Math.sin(tilt)], y: [0, 1, 0] }),
  });
  const corners = { a: node(mm(20), mm(10), 0), b: node(mm(60), mm(10), 0), c: node(mm(60), mm(50), 0), d: node(mm(20), mm(50), 0) };
  const block = part("block", corners, { kind: "solid" });
  block.shapes = [extrusion(polygon(corners.a, corners.b, corners.c, corners.d), { to: mm(30), material: materials.PETG })];
  return { shelf, block, under: endOn(block, shelf.faces["-z"]) };
});
