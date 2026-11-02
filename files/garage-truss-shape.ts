// The existing garage truss as a parametric shape, written on the part core (as examples/garage-truss-shape.ts): a
// Pratt variant, a centre vertical and one vertical on the right, a flat part on the left, a plumb-cut eave tail on the
// right. Its numbers (lengths, and a pitch, a ratio) may be free: the measurements fit them
// (examples/garage-truss.ts).

import { add, sub, mul, div, type Num, type Frame, type Stock } from "topo-part";
import { truss } from "topo-part/trusses";

export interface TrussParams {
  /** Its top chords' pitch (rise over run, a ratio). */
  pitch: Num<"ratio">;
  /** Heel to heel: where a top chord's underside meets the bottom of the bottom chord. */
  span: Num<"length">;
  /** Its bottom chord beyond the left heel (the flat part). */
  flat: Num<"length">;
  /** The right vertical's line, in from the right heel (and the left diagonal's top, in from the left). */
  rightVertical: Num<"length">;
  /** Its eave tail's plumb cut, out from the right heel. */
  tail: Num<"length">;
  /** What its pieces are of (its webs the top chords'), their thickness the truss's. */
  topChord: Stock;
  bottomChord: Stock;
}

/** The truss (topo-part/trusses): its points on its lines, the top chords' undersides and the bottom
 *  chord's bottom; a top chord from the left heel to the peak, seated on the bottom chord; one from the tail's plumb
 *  cut over the right heel to the peak, the bottom chord ending on its underside there; the bottom chord from the flat
 *  part's end, cut plumb; webs: the centre vertical, the left diagonal, the right vertical and the right diagonal. */
export function garageTruss(name: string, p: TrussParams & { at?: Frame }) {
  const { pitch: k, span: l, flat, rightVertical: v, tail } = p;
  const half = div(l, 2);
  return truss(name, {
    points: {
      F: [mul(-1, flat), 0],
      H0: [0, 0],
      H1: [l, 0],
      P: [half, mul(k, half)],
      C: [half, 0],
      T1: [v, mul(k, v)],
      "T1'": [sub(l, v), mul(k, v)],
      B3: [sub(l, v), 0],
      X1: [add(l, tail), mul(-1, k, tail)],
    },
    top: [["H0", "T1", "P"], ["X1", "H1", "T1'", "P"]],
    bottom: ["F", "H0", "C", "B3", "H1"],
    webs: [["P", "C"], ["T1", "C"], ["T1'", "B3"], ["T1'", "C"]],
    topChord: p.topChord,
    bottomChord: p.bottomChord,
    web: p.topChord,
    at: p.at,
  });
}
