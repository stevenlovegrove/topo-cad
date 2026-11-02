// A second storey (examples/primitives/20-a-second-storey.ts), written on the part core: walls standing on a floor.
//
// A storey is stacking, not a number. The ground floor's walls stand at nothing; the floor sits on their plates, with a
// 3/4" deck on its joists; the upper walls stand on the deck; Fink trusses sit on the upper walls. Nothing above the
// ground floor is given a height: each slides up and down only, drawn at nothing, and is found where it rests on what
// is under it. Make the lower walls taller, or the joists deeper, and everything above goes up with them.
//
// Walls 12' by 16' (their outsides), 8' tall, of 2x4 at 16"; the floor's joists 2x10 at 16" along the 12'; the trusses
// across the 12', 6 in 12, 12" past the walls, every 24". The floor's dead load 10 psf and live load 40 psf.

import { part, node, free, plane, support, areaLoad, cases, must, on, offset, placed, joints, frame, add, sub, div, fit, lumber, nominal, DFL, inch, ft, mm, psf } from "topo-part";
import { floor, fink, row } from "topo-part/framing";
import { perimeter } from "topo-part/walls";

const stud = lumber(nominal["2x4"], DFL.No2);
const joist = lumber(nominal["2x10"], DFL.No2);
const [wide, deep, height] = [ft(12), ft(16), ft(8)];
const deck = inch(0.75);
// (A storey of walls on their outsides' corners, where it stands.)
const storey = (name: string, at?: ReturnType<typeof placed>) =>
  perimeter(name, { corners: [node(0, 0, 0), node(wide, 0, 0), node(wide, deep, 0), node(0, deep, 0)], legs: { height, spacing: inch(16), stock: stud }, at });

export default part("a second storey", (s) => {
  const ground = storey("ground");
  // The floor: its joists along the 12' (its y along the walls' x, its x back from the far end), their undersides on
  // the ground floor's plates.
  const rise = free(mm(0));
  const corner = node(0, 0, rise);
  const f = floor("floor", { width: deep, length: wide, spacing: inch(16), stock: joist, at: placed({ origin: [0, deep, 0], x: [0, -1, 0], y: [1, 0, 0], joint: joints.slider([0, 0, 1], rise) }) });
  // The upper walls: on the deck, which is on the joists.
  const lift = free(mm(0));
  const base = node(0, 0, lift);
  const upper = storey("upper", placed({ joint: joints.slider([0, 0, 1], lift) }));
  // The trusses: each across the 12', its heels at the walls' outsides, its bottom chord's underside on the upper
  // walls' plates; its pieces centred on its line, every 24" along the 16'.
  const roof = part("roof", (r) => {
    const lines = row(deep, inch(24), stud.thickness);
    const z = add(lift, height, div(stud.depth, 2));
    const heels = lines.map((y) => [node(0, sub(y, div(stud.thickness, 2)), z), node(wide, sub(y, div(stud.thickness, 2)), z)]);
    const trusses = heels.map((h) => fink("truss", { span: wide, pitch: 6 / 12, overhang: inch(12), stock: stud, at: frame(h[0], { x: h[1], y: r.y }) }));
    return { heels, trusses };
  });
  const slab = [node(0, 0, 0), node(mm(1), 0, 0), node(0, mm(1), 0)];
  return {
    ground,
    corner,
    floor: f,
    base,
    upper,
    roof,
    slab,
    bears: support(plane(slab[0], slab[1], slab[2])),
    // (The deck: what the upper walls stand on, on the joists. Not drawn: a gap it fills, by name, which they bear
    // across.)
    decked: fit(f, upper, { gap: deck, fill: "3/4\" deck", name: "deck" }),
    dead: areaLoad(f, psf(10), cases.dead),
    live: areaLoad(f, psf(40), cases.live),
    says: {
      "floor on the walls": must(on(corner, ground.legs[0].topPlate.faces["-y"])),
      "walls on the deck": must(offset(corner, base, s.z, add(joist.depth, deck))),
    },
  };
});
