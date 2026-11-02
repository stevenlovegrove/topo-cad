// The standard truss types (examples/truss-gallery.ts), written on the part core: each on a small building of its own,
// for comparison.
//
// Each type of `topo-part/trusses` (each a `truss` of its shape): a Fink (`topo-part/framing`), a fan, a king post
// (over 16'), a Howe and a Pratt (six panels), a scissors (its bottom chords rising at 3 in 12 to meet under its peak)
// and a mono (over 12', four panels), and a mono whose bottom chord rises at 2 in 12 from plate to plate; all at 6 in 12
// with 12" overhangs, of 2x4s. And a raised heel (over 16', its heels 12" high) of two pitches, 6 in 12 and 4 in 12: its
// ridge off the middle, each top chord trimmed to the other's top where they meet; and the same as a scissors, its
// bottom chords rising at 2 in 12 to meet under its peak. And the sloped mono again with a raised heel, 12" high. Each over walls 4' long, 8' tall, of 2x4 at 16", three of them every 24" (the ends' flush
// with the walls' outsides), their heels on the plates at the walls' outsides. (The sloped mono's high wall is taller:
// its chord, rising, bears on that wall's plate's inner top edge, where it is as high.) Snow 25 psf on plan.

import { part, node, plane, support, bears, frame, areaLoad, cases, lumber, nominal, DFL, inch, ft, mm, psf, add, sub, mul, div, type Feature, type Frame, type Length, type PartBase } from "topo-part";
import { perimeter } from "topo-part/walls";
import { fink, trussRow, row } from "topo-part/framing";
import { fan, kingPost, howe, pratt, scissors, mono, raisedHeel } from "topo-part/trusses";

const stud = lumber(nominal["2x4"], DFL.No2), chord = stud;
const [pitch, overhang, bay, height] = [6 / 12, inch(12), ft(4), ft(8)];
const common = { pitch, overhang, chord, web: chord };
/** Each type: its name, its span (feet), and how one is made at a frame (each `truss` with its bottom chord's underside
 *  at its frame's z = 0; a Fink with its axis there, half its depth up); and, of one whose bottom chord rises, at what. */
const types: [string, number, (at: Frame) => PartBase & { bottomUnder: Feature }, number?][] = [
  ["Fink", 24, (at) => fink("truss", { span: ft(24), pitch, overhang, stock: chord, at })],
  ["Fan", 24, (at) => fan("truss", { ...common, span: ft(24), at })],
  ["King post", 16, (at) => kingPost("truss", { ...common, span: ft(16), at })],
  ["Howe", 24, (at) => howe("truss", { ...common, span: ft(24), panels: 6, at })],
  ["Pratt", 24, (at) => pratt("truss", { ...common, span: ft(24), panels: 6, at })],
  ["Scissors", 24, (at) => scissors("truss", { ...common, span: ft(24), bottomPitch: 3 / 12, at })],
  ["Mono", 12, (at) => mono("truss", { ...common, span: ft(12), panels: 4, at })],
  ["Mono, sloped", 12, (at) => mono("truss", { ...common, span: ft(12), panels: 4, bottomPitch: 2 / 12, at }), 2 / 12],
  ["Mono, raised heel, sloped", 12, (at) => mono("truss", { ...common, span: ft(12), panels: 4, bottomPitch: 2 / 12, heel: inch(12), at }), 2 / 12],
  ["Raised heel, two pitches", 16, (at) => raisedHeel("truss", { ...common, span: ft(16), pitch: [6 / 12, 4 / 12], heels: [inch(12), inch(12)], at })],
  ["Raised-heel scissors", 16, (at) => raisedHeel("truss", { ...common, span: ft(16), pitch: [6 / 12, 4 / 12], heels: [inch(12), inch(12)], bottomPitch: 2 / 12, at })],
];

export default part("standard truss types", () => {
  const buildings = Object.fromEntries(types.map(([name, span, make, rise], i) => {
    // (Each 10' along from the last.)
    const at = (x: Length | 0, y: Length | 0, z: Length | 0 = 0) => node(add(ft(10 * i), x), y, z);
    // (Where its bottom chord rises: its side walls' tops rising with it, under the end trusses; its back wall as high
    // as the chord is over that wall's plate's inner edge.)
    const [far, back] = rise ? [add(height, mul(rise, ft(span))), add(height, mul(rise, sub(ft(span), stud.depth)))] : [height, height];
    const leg = { spacing: inch(16), stock: stud };
    const legs = [{ ...leg, height }, { ...leg, height: [height, far] as [Length, Length] }, { ...leg, height: back }, { ...leg, height: [far, height] as [Length, Length] }];
    const walls = perimeter(`${name} walls`, { corners: [at(0, 0), at(bay, 0), at(bay, ft(span)), at(0, ft(span))], legs });
    // (Its trusses along the bay, spanning y from the front wall's outside; their bottom chords' undersides on the plates.)
    const z = name === "Fink" ? add(height, div(chord.depth, 2)) : height;
    const trusses = trussRow(`${name} trusses`, {
      stations: row(bay, inch(24), chord.thickness), height: mm(0), make: (_, f) => make(f),
      at: frame(at(0, 0, z), { x: at(ft(1), 0, z), y: at(0, ft(1), z) }),
    });
    // (Its load path: the trusses on the walls.)
    return [name, { walls, trusses, rest: bears(trusses, walls), snow: areaLoad(trusses, psf(25), cases.snow) }];
  }));
  const ground = [node(0, 0, 0), node(mm(1), 0, 0), node(0, mm(1), 0)];
  return { buildings, ground, slab: support(plane(ground[0], ground[1], ground[2])) };
});
