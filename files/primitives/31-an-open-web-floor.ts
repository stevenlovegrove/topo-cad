// An open-web floor (examples/primitives/31-an-open-web-floor.ts), written on the part core: a floor of parallel-chord
// trusses on walls, and walls standing on its deck.
//
// Walls 16' by 24' (their outsides), 8' tall, of 2x6 at 16". The floor's trusses span the 16', every 24": 18" deep,
// their chords 2x4s laid flat, their webs 2x4s on edge, in panels about as long as they are deep; a rim board 1¼" by 18"
// across each end. Its 3/4" deck carries the upper walls, 8' tall, of 2x6 at 16". The floor's dead load 15 psf and live
// load 40 psf (a house's, as its plans give them for floor trusses).
//
// As in the second storey (20), nothing above the ground floor is given a height: the floor slides up and down only, and
// is found resting on the walls' plates; the upper walls on its deck.

import { part, node, free, plane, support, areaLoad, cases, must, on, offset, placed, joints, fit, add, box, lumber, nominal, DFL, inch, ft, mm, psf } from "topo-part";
import { openWebFloor } from "topo-part/trusses";
import { perimeter } from "topo-part/walls";

const stud = lumber(nominal["2x6"], DFL.No2);
const twoByFour = lumber(nominal["2x4"], DFL.No2);
// (A 2x4 laid flat: 3½" across, 1½" deep.)
const flat = box("2x4 flat", { thickness: twoByFour.depth, depth: twoByFour.thickness, material: DFL.No2 });
const rim = box("1¼ rim board", { thickness: inch(1.25), depth: inch(18), material: DFL.No2 });
const [wide, deep, height, depth, deck] = [ft(16), ft(24), ft(8), inch(18), inch(0.75)];
const storey = (name: string, at?: ReturnType<typeof placed>) =>
  perimeter(name, { corners: [node(0, 0, 0), node(wide, 0, 0), node(wide, deep, 0), node(0, deep, 0)], legs: { height, spacing: inch(16), stock: stud }, at });

export default part("an open-web floor", (s) => {
  const ground = storey("ground");
  // The floor: its trusses along the 16' (its y along the walls' x), across the 24' every 24"; its underside on the
  // ground floor's plates.
  const rise = free(mm(0));
  const corner = node(0, 0, rise);
  const f = openWebFloor("floor", { width: deep, length: wide, spacing: inch(24), depth, chord: flat, web: twoByFour, rim, at: placed({ origin: [0, deep, 0], x: [0, -1, 0], y: [1, 0, 0], joint: joints.slider([0, 0, 1], rise) }) });
  // The upper walls: on the deck, which is on the trusses.
  const lift = free(mm(0));
  const base = node(0, 0, lift);
  const upper = storey("upper", placed({ joint: joints.slider([0, 0, 1], lift) }));
  const slab = [node(0, 0, 0), node(mm(1), 0, 0), node(0, mm(1), 0)];
  return {
    ground,
    corner,
    floor: f,
    base,
    upper,
    slab,
    bears: support(plane(slab[0], slab[1], slab[2])),
    decked: fit(f, upper, { gap: deck, fill: "3/4\" deck", name: "deck" }),
    dead: areaLoad(f, psf(15), cases.dead),
    live: areaLoad(f, psf(40), cases.live),
    says: {
      "floor on the walls": must(on(corner, ground.legs[0].topPlate.faces["-y"])),
      "walls on the deck": must(offset(corner, base, s.z, add(depth, deck))),
    },
  };
});
