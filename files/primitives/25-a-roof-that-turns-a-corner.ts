// A roof that turns a corner (examples/primitives/25-a-roof-that-turns-a-corner.ts), written on the part core.
//
// The building is a long bar, 30' by 16', with a wing 12' wide out of its front, 10' deep; walls 8' tall, of 2x4 at
// 16". Its roof is two parts, each a plain gable at 6 in 12 over its own walls (`roofSurface`, topo-part/framing): one
// along the bar; one along the wing, run on into the bar as far as its ridge. Where the two overlap the higher is the
// roof: so the wing's ridge runs back until it meets the bar's slope, and the two valleys from there down to the inside
// corners are where their planes cross. Neither is given. The wing is narrower than the bar, so its ridge is lower.
//
// Both are framed with Fink trusses of 2x4, every 24". The bar's are framed through. The wing's stop at the bar's eave,
// and from there the wing is carried on over the bar's slope by a valley set: trusses of the wing's own slopes, smaller
// and smaller, each with a level bottom chord that lies across the bar's top chords (touching each at its uphill edge,
// since they slope), its top chords' tops in the wing's planes. (Not yet: the last of the set, too short to cross two of
// the bar's trusses under it: the wing's top there is its surface only. In building the bar is sheathed first and the
// set sits on the sheathing; here it sits on the chords.) Snow 25 psf on plan.

import { part, node, plane, support, frame, areaLoad, cases, layers, weights, lumber, nominal, DFL, inch, ft, mm, psf, type Length } from "topo-part";
import { perimeter, tee } from "topo-part/walls";
import { roofSurface, trussRow, fink, row } from "topo-part/framing";

const chord = lumber(nominal["2x4"], DFL.No2);
const pitch = 6 / 12;
// (In inches: the walls' plates' top; a 2x4's depth; where a Fink's top chords' tops are over its bottom chord's
// underside at its heels, which are on the walls' outsides: its axes meet half a depth up, and its top chord's top is
// half a depth square to its slope over that.)
const [plate, d, chord_t] = [96, 3.5, 1.5];
const heel = d / 2 + (d / 2) * Math.hypot(1, pitch);
const [bar, deep, wing, out, from] = [360, 192, 144, 120, 90];
const eave = (h: number) => ({ pitch, height: [inch(h), inch(h)] as [Length, Length], overhang: inch(12) });
const at = (x: number, y: number, z = 0) => node(inch(x), inch(y), z === 0 ? 0 : inch(z));
// (A row of trusses from `a`, along `along` (a way on plan), spanning to its left, its trusses' bottom chords' undersides
// at `z`: its frame there, x along the row, y across it, so z is up.)
const rowFrame = (a: [number, number], along: [number, number], z: number) =>
  frame(at(a[0], a[1], z), { x: at(a[0] + along[0], a[1] + along[1], z), y: at(a[0] - along[1], a[1] + along[0], z) });
const legs = { height: inch(plate), spacing: inch(16), stock: chord };

export default part("a roof that turns a corner", () => {
  // The bar's walls, and the wing's three, out from the bar's front and back to it, teed onto its outside.
  const walls = perimeter("walls", { corners: [at(0, 0), at(bar, 0), at(bar, deep), at(0, deep)], legs: [{ ...legs, backs: [{ at: inch(from + d / 2), width: chord.depth }, { at: inch(from + wing - d / 2), width: chord.depth }] }, legs, legs, legs] });
  const wingWalls = perimeter("wing walls", { corners: [at(from, 0), at(from, -out), at(from + wing, -out), at(from + wing, 0)], closed: false, legs });
  const tees = [tee(walls.legs[0], wingWalls.legs[0], inch(from + d / 2), { side: "outside" }), tee(walls.legs[0], wingWalls.legs[2], inch(from + wing - d / 2), { end: "end", side: "outside" })];
  // The roof's surface: the bar, its eaves along its front and back; the wing, its eaves along its sides, run on into
  // the bar to its ridge, its far gable with no rake.
  const roof = roofSurface("roof", {
    parts: [
      { outline: [[0, 0], [inch(bar), 0], [inch(bar), inch(deep)], [0, inch(deep)]], edges: [eave(plate + heel), null, eave(plate + heel), null], rake: inch(12) },
      { outline: [[inch(from), inch(-out)], [inch(from + wing), inch(-out)], [inch(from + wing), inch(deep / 2)], [inch(from), inch(deep / 2)]], edges: [null, eave(plate + heel), null, eave(plate + heel)], rake: inch(12), rakes: [undefined, undefined, 0, undefined] },
    ],
    build: layers(weights.asphaltShingles, weights.roofingFelt, weights.osb7_16),
  });
  // The bar's trusses, across it, every 24" along it; the wing's, across it, from its front back to the bar's eave (short
  // of its tails, which run out 12" past its wall). Each bottom chord's underside on the plates.
  const truss = (span: number) => (k: number, f: ReturnType<typeof frame>) => fink(`truss ${k + 1}`, { span: inch(span), pitch, overhang: inch(12), stock: chord, at: f });
  const bars = trussRow("bar trusses", { stations: row(inch(bar), inch(24), chord.thickness), height: mm(0), make: truss(deep), at: rowFrame([0, 0], [1, 0], plate + d / 2) });
  const wings = trussRow("wing trusses", { stations: row(inch(out - 18), inch(24), chord.thickness), height: mm(0), make: truss(wing), at: rowFrame([from + wing, -out], [0, 1], plate + d / 2) });
  // The valley set: from the bar's front wall (y 0) every 24" up its slope, each a Fink of the wing's pitch laid from
  // its station up the slope, its bottom chord's underside on the bar's top chords at its uphill edge, its heels where
  // its tops are in the wing's planes; while it is long enough to cross two of the bar's trusses.
  const set = [];
  for (let y = 0; ; y += 24) {
    // (The bar's tops are its eave's height and the slope's rise from its line; the chord's uphill edge, its thickness up.)
    const under = plate + heel + pitch * (y + chord_t);
    // (Where the wing's plane, up from its eave, is as high over the chord's underside as a Fink's top is at its heel.)
    const x0 = from + (under - plate) / pitch;
    const span = wing - 2 * (x0 - from);
    if (span < 48) break;
    const z = under + d / 2;
    set.push(fink(`valley truss ${set.length + 1}`, { span: inch(span), pitch, overhang: inch(0), stock: chord, at: frame(at(x0, y, z), { x: at(x0 + 12, y, z), y: at(x0, y + 12, z) }) }));
  }
  const ground = [node(0, 0, 0), node(mm(1), 0, 0), node(0, mm(1), 0)];
  return {
    walls, wingWalls, tees, roof, bars, wings, set, ground,
    slab: support(plane(ground[0], ground[1], ground[2])),
    snow: [areaLoad(bars, psf(25), cases.snow), areaLoad(wings, psf(25), cases.snow)],
  };
});
