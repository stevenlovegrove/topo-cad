// What a floor and a roof are made of (examples/primitives/27-what-a-floor-and-a-roof-are-made-of.ts), written on the
// part core.
//
// A floor's deck and a roof's covering are given as their layers, top first, by the names of materials (`layers(...)`,
// from the table of material weights): the floor's, hardwood flooring on a plywood subfloor; the roof's, standing-seam
// steel on an underlayment on plywood. The deck is as thick as its layers together, a gap on the joists the upper walls
// bear across (a fit filled); and its weight is a dead load on the floor, per area of floor. The covering's is a dead
// load on the trusses, per area of slope. Otherwise as the second storey: walls 12' by 16', 8' tall, of 2x4 at 16"; the
// floor's joists 2x10 at 16" along the 12'; Fink trusses across the 12', 6 in 12, 12" past the walls, every 24". Live
// load 40 psf on the floor, snow 25 psf on the roof.

import { part, node, free, plane, support, areaLoad, cases, must, on, offset, placed, joints, frame, add, sub, div, fit, lumber, nominal, DFL, inch, ft, mm, psf, layers, weights, type Length } from "topo-part";
import { floor, fink, row } from "topo-part/framing";
import { perimeter } from "topo-part/walls";

const stud = lumber(nominal["2x4"], DFL.No2);
const joist = lumber(nominal["2x10"], DFL.No2);
const [wide, deep, height] = [ft(12), ft(16), ft(8)];
const decking = layers(weights.hardwoodFlooring7_8, weights.plywood23_32);
const covering = layers(weights.standingSeam24ga, weights.polystickMts, weights.plywood15_32);
const deck = decking.layers!.reduce((t: Length, l) => add(t, l.thickness), mm(0));
// (A storey of walls on their outsides' corners, where it stands.)
const storey = (name: string, at?: ReturnType<typeof placed>) =>
  perimeter(name, { corners: [node(0, 0, 0), node(wide, 0, 0), node(wide, deep, 0), node(0, deep, 0)], legs: { height, spacing: inch(16), stock: stud }, at });

export default part("what a floor and a roof are made of", (s) => {
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
    // (The deck: what the upper walls stand on, on the joists. Not drawn: a gap it fills, by its build-up's label,
    // which they bear across.)
    decked: fit(f, upper, { gap: deck, fill: decking.label, name: "deck" }),
    deckWeight: areaLoad(f, decking.pressure, cases.dead),
    live: areaLoad(f, psf(40), cases.live),
    coveringWeight: areaLoad(roof, covering.pressure, cases.dead, { plan: false }),
    snow: areaLoad(roof, psf(25), cases.snow),
    says: {
      "floor on the walls": must(on(corner, ground.legs[0].topPlate.faces["-y"])),
      "walls on the deck": must(offset(corner, base, s.z, add(joist.depth, deck))),
    },
  };
});
