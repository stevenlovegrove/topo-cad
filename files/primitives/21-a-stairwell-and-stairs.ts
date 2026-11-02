// A stairwell and stairs (examples/primitives/21-a-stairwell-and-stairs.ts), written on the part core: an opening in a
// floor, and stairs up through it.
//
// Walls 12' by 16' (their outsides), 8' tall, of 2x4 at 16"; a floor on them of 2x12 joists at 16", front to back, with
// a 3/4" deck. An opening is framed in it (`floor`'s `opening`, topo-part/framing): a doubled trimmer joist either side,
// a doubled header across each end hung from the trimmers, and the joists it cuts stopping at the headers (tail joists).
// The joists run the way the stairs do, so the opening's length is along them and only a few are cut: it is 10' long,
// from 4' back, and 3'-8" across them, a foot in from the left wall.
//
// The stairs (`stairs`): a flight of equal risers from the ground to the floor's top (8' of wall, the joists' depth and
// the deck: 14 risers of a little under 7 3/4", so 13 treads going 10'-10" on plan), up the middle of the opening, its
// stringers' heads cut plumb at its far end, against the header there; their feet cut level, on the ground. Its treads
// are housed in the stringers, half an inch: each bears in its housing. The opening is wider than the flight by 2 1/2"
// a side, beyond its stringers. Floor loads 10 psf dead, 40 psf live.

import { part, node, free, plane, support, placed, joints, frame, fit, areaLoad, cases, must, on, lumber, box, nominal, DFL, add, inch, ft, mm, psf } from "topo-part";
import { perimeter } from "topo-part/walls";
import { floor, stairs } from "topo-part/framing";

const stud = lumber(nominal["2x4"], DFL.No2), joist = lumber(nominal["2x12"], DFL.No2), stringer = lumber(nominal["2x12"], DFL.No2);
const tread = box("2x12 tread", { thickness: inch(11.25), depth: inch(1.5), material: DFL.No2 });
const [wide, deep, height, deck] = [ft(12), ft(16), ft(8), inch(0.75)];
// (The flight between its stringers' insides; the opening as wide as that, its stringers, and 2 1/2" a side.)
const [flight, clear] = [36, 2.5];
const opening = { across: 12, along: 48, width: flight + 2 * (1.5 + clear), length: 120 };

export default part("a stairwell and stairs", (s) => {
  const walls = perimeter("walls", { corners: [node(0, 0, 0), node(wide, 0, 0), node(wide, deep, 0), node(0, deep, 0)], legs: { height, spacing: inch(16), stock: stud } });
  // The floor: its joists along y, across x; its underside on the walls' plates (found, sliding up to them).
  const rise = free(mm(0));
  const corner = node(0, 0, rise);
  const f = floor("floor", {
    width: wide, length: deep, spacing: inch(16), stock: joist,
    opening: { across: inch(opening.across), along: inch(opening.along), width: inch(opening.width), length: inch(opening.length) },
    at: placed({ joint: joints.slider([0, 0, 1], rise) }),
  });
  // The flight: up the middle of the opening, its head at the opening's far end, as high as the floor's top.
  const up = stairs("stairs", { rise: add(height, joist.depth, deck), width: inch(flight), stringer, tread });
  const going = 13 * 10;
  const x0 = opening.across + 1.5 + clear, y0 = opening.along + opening.length - going;
  const flightAt = frame(node(inch(x0), inch(y0), 0), { x: node(inch(x0 + 12), inch(y0), 0), y: node(inch(x0), inch(y0 + 12), 0) });
  const placedUp = stairs("stairs", { rise: add(height, joist.depth, deck), width: inch(flight), stringer, tread, at: flightAt });
  void up;
  const ground = [node(0, 0, 0), node(mm(1), 0, 0), node(0, mm(1), 0)];
  return {
    walls,
    corner,
    floor: f,
    stairs: placedUp,
    ground,
    slab: support(plane(ground[0], ground[1], ground[2])),
    dead: areaLoad(f, psf(10), cases.dead),
    live: areaLoad(f, psf(40), cases.live),
    says: { "floor on the walls": must(on(corner, walls.legs[0].topPlate.faces["-y"])) },
  };
});
