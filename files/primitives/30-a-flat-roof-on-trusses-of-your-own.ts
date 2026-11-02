// A flat roof beside a sloped one, on trusses of your own (examples/primitives/30-a-flat-roof-on-trusses-of-your-own.ts),
// written on the part core.
//
// The trusses are not a standard shape: each has a bottom chord that runs on 3' past its front heel (a canopy over the
// front wall), and a tail past its back heel. They are given point by point and member by member (the trusses library,
// as the present pipeline's `TrussShape.custom`). The roof has two parts on them:
//
// - a sloped part, the top chords' planes, covered with standing-seam steel on an underlayment on plywood;
// - a flat part over the front, on the bottom chords' run past the heel, covered with a membrane on plywood.
//
// Where they meet, the higher is the roof: they fold where the top chord's top comes up through the level of the bottom
// chord's top, a little in from the heel. Each part's covering is its dead load (the slope's along its surface), and
// the snow is over both: each on the members that carry that part (`carriedBy`), over what of them is under it
// (`within`). Roof live load 20 psf on the canopy. Walls 12' by 20' (their outsides), 8' tall, of 2x4 at 16"; the
// trusses span the 20', 4 in 12, every 24".

import { part, node, plane, support, areaLoad, cases, roles, layers, weights, frame, neg, lumber, nominal, DFL, add, div, inch, ft, mm, as, psf, type Length, type Length0 } from "topo-part";
import { row } from "topo-part/framing";
import { perimeter } from "topo-part/walls";
import { truss } from "topo-part/trusses";

const stud = lumber(nominal["2x4"], DFL.No2);
const six = lumber(nominal["2x6"], DFL.No2);
const [span, length, k, canopy, tail, height] = [ft(20), ft(12), 4 / 12, ft(3), ft(1.5), ft(8)];
const [chord, web, bottom] = [stud, stud, six];
// (Its points in millimetres, worked out from its span, canopy and tail.)
const [S, C, T] = [as(span, mm), as(canopy, mm), as(tail, mm)];
const p = (x: number, z: number): [Length, Length] => [mm(x), mm(z)];
const top = (s: number) => k * Math.min(s, S - s);
// (Where the two parts fold: the top chord's top, up from its underside's line through the heel, at the bottom chord's
// top.)
const fold = (as(bottom.depth, mm) - as(chord.depth, mm) * Math.hypot(1, k)) / k;
const strip = (y0: number, y1: number): [Length0, Length0][] => [[mm(-1000), mm(y0)], [add(length, mm(1000)), mm(y0)], [add(length, mm(1000)), mm(y1)], [mm(-1000), mm(y1)]];

export default part("a flat roof on trusses of your own", () => {
  const w = perimeter("walls", { corners: [node(0, 0, 0), node(length, 0, 0), node(length, span, 0), node(0, span, 0)], legs: { height, spacing: inch(16), stock: stud } });
  // Each truss across the 20', its front heel at the front wall's outside, its bottom chord's underside on the plates;
  // its pieces centred on its line, every 24" along the 12'.
  const roof = part("roof", (r) => {
    const lines = row(length, inch(24), stud.thickness);
    const heels = lines.map((x) => [node(add(x, div(stud.thickness, 2)), 0, height), node(add(x, div(stud.thickness, 2)), span, height)]);
    const trusses = heels.map((h) => truss("truss", {
      points: { F: p(-C, 0), H0: p(0, 0), H1: p(S, 0), P: p(S / 2, top(S / 2)), C: p(S / 2, 0), T0: p(S / 4, top(S / 4)), T1: p((3 * S) / 4, top(S / 4)), X1: p(S + T, -T * k) },
      top: [["H0", "T0", "P"], ["X1", "H1", "T1", "P"]],
      bottom: ["F", "H0", "C", "H1"],
      webs: [["P", "C"], ["T0", "C"], ["T1", "C"]],
      topChord: chord, bottomChord: bottom, web,
      // (Its x across the 20', its y west: so its z is up.)
      at: frame(h[0], { x: h[1], y: neg(r.x) }),
    }));
    return { heels, trusses };
  });
  const [slope, flat] = [strip(fold, S + T + 100), strip(-C - 100, fold)];
  const ground = [node(0, 0, 0), node(mm(1), 0, 0), node(0, mm(1), 0)];
  return {
    walls: w,
    roof,
    ground,
    slab: support(plane(ground[0], ground[1], ground[2])),
    loads: {
      slopeCovering: areaLoad(roof, layers(weights.standingSeam24ga, weights.polystickMts, weights.plywood15_32).pressure, cases.dead, { plan: false, within: slope }),
      canopyCovering: areaLoad(roof, layers(weights.tpo60mil, weights.plywood15_32).pressure, cases.dead, { within: flat, carriedBy: roles.bottomChord }),
      slopeSnow: areaLoad(roof, psf(25), cases.snow, { within: slope }),
      canopySnow: areaLoad(roof, psf(25), cases.snow, { within: flat, carriedBy: roles.bottomChord }),
      canopyLive: areaLoad(roof, psf(20), cases.roofLive, { within: flat, carriedBy: roles.bottomChord }),
    },
  };
});
