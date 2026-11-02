// A bracket extruded to a plate (examples/volumes/02-a-bracket-extruded-to-a-plate.ts), written on the part core: an
// extrusion whose end is put on a face.
//
// An L-shaped printed bracket stands on the floor: its outline drawn on the floor, extruded up, with a 3.3 bore up
// through it for an M3 screw. Its top (`end`) is on a sloping plate's underside (`endOn`), so its sides stay flat and
// upright and the bore is as long as the bracket is where it goes through.
//
// Its outline has curves: the run from b to c is an arc of radius 6, curving in (its side a piece of a cylinder; the
// original's script did not say which way, its weight says in), and the inner corner d is rounded off to 4 (another).
// Its sides are named by its outline's runs (`o0`, the run from a; the round's own), its bore `h0.0`, its ends `start`
// (on the floor) and `end` (against the plate).

import { part, node, plate, placed, extrusion, circle, endOn, materials, mm, type Corner } from "topo-part";

const tilt = (15 * Math.PI) / 180;
// (The arc's centre: out from the middle of its chord, b to c (10 long), so its radius is 6 and it curves in.)
const inset = Math.sqrt(36 - 25);

export default part("a bracket extruded to a plate", () => {
  const shelf = plate("plate", mm(80), mm(50), mm(2), materials.aluminium6061T6, [], {
    at: placed({ origin: [0, 0, mm(30)], x: [Math.cos(tilt), 0, Math.sin(tilt)], y: [0, 1, 0] }),
  });
  const nodes = {
    a: node(mm(10), mm(10), 0), b: node(mm(40), mm(10), 0), arc: node(mm(40 + inset), mm(15), 0), c: node(mm(40), mm(20), 0),
    d: node(mm(20), mm(20), 0), e: node(mm(20), mm(40), 0), f: node(mm(10), mm(40), 0),
    bore: node(mm(15), mm(15), 0), rim: node(mm(15 + 1.65), mm(15), 0),
  };
  const outline: Corner[] = [
    { node: nodes.a }, { node: nodes.b, arc: { centre: nodes.arc, ccw: false } }, { node: nodes.c },
    { node: nodes.d, round: mm(4) }, { node: nodes.e }, { node: nodes.f },
  ];
  const bracket = part("bracket", nodes, { kind: "solid" });
  bracket.shapes = [extrusion(outline, { to: mm(20), holes: [circle(nodes.bore, nodes.rim)], material: materials.PETG })];
  return { plate: shelf, bracket, under: endOn(bracket, shelf.faces["-z"]) };
});
