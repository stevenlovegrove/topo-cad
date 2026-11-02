// An interior wall (examples/primitives/19-an-interior-wall.ts), written on the part core: walls that meet other walls
// part way along them.
//
// Walls 24' by 16' (their outsides), 8' tall. A partition is a leg of wall (topo-part/walls) from a place on one wall
// to a place on another (or to a point, where it ends free), teed into each it meets: from the face of the wall it
// starts at to the face of the one it ends at, as tall as they are; there, that wall's upper top plate is cut and the
// partition's laps over it, its lower plates end on that wall's face, and that wall has a stud each side of it to back
// it (it says where it is met: `backs`). The middle wall, 12' along, is a bearing wall: the floor's joists run the
// length of the building and sit on the left wall, on it, and on the right wall. A second partition runs from the
// middle wall to the right wall, 9' back, with a 32" door 5' along it; one more runs 5' in from the left wall, 8' back,
// and ends free. The floor's joists are 2x10 at 16"; dead load 10 psf, live 40 psf.

import { part, node, free, plane, support, areaLoad, cases, must, on, placed, joints, lumber, nominal, DFL, add, sub, div, inch, ft, mm, psf, type Length, type Node } from "topo-part";
import { floor } from "topo-part/framing";
import { perimeter, wall, tee, type Opening } from "topo-part/walls";

const stud = lumber(nominal["2x4"], DFL.No2);
const joist = lumber(nominal["2x10"], DFL.No2);
const [long, deep, height] = [ft(24), ft(16), ft(8)];
// (Half a wall, from its middle to its faces; a wall, from its outside to its inside.)
const out = div(stud.depth, 2), inside = stud.depth;
const leg = { height, spacing: inch(16), stock: stud };
// (A partition from one place of its line to another: its line is its +z face, half a wall to its right, looking along
// it, from its middle. Each laid out from the line of the wall it starts at, as the original's: the walls' outsides for
// the middle wall and the stub, the middle wall's middle for the second.)
const partition = (name: string, from: Node, to: Node, layoutFrom: Length, openings: Opening[] = []) => wall(name, { ...leg, from, to, setOuts: [sub(mm(0), layoutFrom)], openings });
// (The door, 5' along the second from the middle wall's middle.)
const door: Opening = { label: "Door", kind: "door", centre: sub(ft(5), out), width: inch(32), head: inch(82), header: lumber(nominal["2x8"], DFL.No2, { plies: 2 }), jacks: 1 };
const backs = (...at: Length[]) => at.map((a) => ({ at: a, width: stud.depth }));

export default part("an interior wall", () => {
  // (The front, right, back and left, anticlockwise: the back from its east end, so 12' from its start is the middle
  // too; the left from its north end, so 8' from its start is 8' back.)
  const walls = perimeter("walls", {
    corners: [node(0, 0, 0), node(long, 0, 0), node(long, deep, 0), node(0, deep, 0)],
    legs: [{ ...leg, backs: backs(ft(12)) }, { ...leg, backs: backs(ft(9)) }, { ...leg, backs: backs(ft(12)) }, { ...leg, backs: backs(ft(8)) }],
  });
  // The middle wall north from the front's inside to the back's, its +z east; the second east from its east face to
  // the right wall's inside, its +z south; the stub east from the left wall's inside, its +z south, ending free.
  const middle = wall("middle", { ...leg, from: node(add(ft(12), out), inside, 0), to: node(add(ft(12), out), sub(deep, inside), 0), setOuts: [sub(mm(0), inside)], backs: backs(sub(ft(9), inside)) });
  const cross = partition("cross", node(add(ft(12), out), sub(ft(9), out), 0), node(sub(long, inside), sub(ft(9), out), 0), out, [door]);
  const stub = partition("stub", node(inside, sub(ft(8), out), 0), node(ft(5), sub(ft(8), out), 0), inside);
  const tees = {
    middle: [tee(walls.legs[0], middle, ft(12)), tee(walls.legs[2], middle, ft(12), { end: "end" })],
    cross: [tee(middle, cross, sub(ft(9), inside), { side: "outside" }), tee(walls.legs[1], cross, ft(9), { end: "end" })],
    stub: tee(walls.legs[3], stub, ft(8)),
  };
  // The floor: its joists along the 24' (its y along the walls' x, its x back from the far end), their undersides on
  // the walls' plates.
  const rise = free(mm(0));
  const corner = node(0, 0, rise);
  const f = floor("floor", { width: deep, length: long, spacing: inch(16), stock: joist, at: placed({ origin: [0, deep, 0], x: [0, -1, 0], y: [1, 0, 0], joint: joints.slider([0, 0, 1], rise) }) });
  const ground = [node(0, 0, 0), node(mm(1), 0, 0), node(0, mm(1), 0)];
  return {
    walls,
    middle,
    cross,
    stub,
    tees,
    corner,
    floor: f,
    ground,
    slab: support(plane(ground[0], ground[1], ground[2])),
    dead: areaLoad(f, psf(10), cases.dead),
    live: areaLoad(f, psf(40), cases.live),
    says: {
      "on the walls": must(on(corner, walls.legs[0].topPlate.faces["-y"])),
    },
  };
});
