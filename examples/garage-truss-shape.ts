// The existing garage truss as a parametric shape (a Pratt variant: centre
// vertical plus one vertical on the right, a flat part on the left, a
// plumb-cut eave tail on the right). Parameters are fitted to field
// measurements in garage-truss.ts; `fittedGarageTruss` holds the result.
import { type Nominal, TrussShape } from "topo-cad";

export interface TrussParams {
  /** Top chord pitch (rise/run). */
  readonly pitch: number;
  /** Heel node to heel node, where the top chord's underside meets the bottom of the bottom chord. */
  readonly span: number;
  /** Bottom chord beyond the left heel node (the flat part). */
  readonly flat: number;
  /** Right vertical's centreline, from the right heel node. */
  readonly rightVertical: number;
  /** Eave tail's plumb cut, horizontally from the right heel node. */
  readonly tail: number;
  readonly topChord: Nominal;
  readonly bottomChord: Nominal;
}

/** The truss shape for given parameters (nothing measurement-specific). */
export function garageTruss(p: TrussParams): TrussShape {
  const { span: l, pitch: k } = p;
  const top = (s: number) => k * Math.min(s, l - s); // underside of the top chord
  const v = p.rightVertical;
  return TrussShape.custom({
    name: "Existing (Pratt variant)",
    span: l,
    pitch: k,
    points: {
      F: [-p.flat, 0],
      H0: [0, 0],
      H1: [l, 0],
      P: [l / 2, top(l / 2)],
      C: [l / 2, 0],
      T1: [v, top(v)],
      "T1'": [l - v, top(v)],
      B3: [l - v, 0],
      X1: [l + p.tail, -p.tail * k],
    },
    topChords: [
      ["H0", "T1", "P"],
      ["X1", "H1", "T1'", "P"],
    ],
    bottomChords: [["F", "H0", "C", "B3", "H1"]],
    webs: [
      ["P", "C"], // centre vertical
      ["T1", "C"], // left diagonal
      ["T1'", "C"], // right diagonal
      ["T1'", "B3"], // right vertical (no left one)
    ],
  })
    .sized("bottom_chord", p.bottomChord)
    .sized("top_chord", p.topChord)
    .sized("web", p.topChord);
}


/**
 * The variant over the north section, where the trusses bear on an inner
 * wall `bearingAt` from the left heel: a vertical over that wall (Bw–Tw) and
 * a diagonal from its foot up to T1' replace the right vertical (T1'–B3).
 */
export function garageTrussInnerBearing(p: TrussParams, bearingAt: number): TrussShape {
  const { span: l, pitch: k } = p;
  const top = (s: number) => k * Math.min(s, l - s);
  const v = p.rightVertical;
  return TrussShape.custom({
    name: "Existing (Pratt variant, inner bearing)",
    span: l,
    pitch: k,
    points: {
      F: [-p.flat, 0],
      H0: [0, 0],
      H1: [l, 0],
      P: [l / 2, top(l / 2)],
      C: [l / 2, 0],
      T1: [v, top(v)],
      "T1'": [l - v, top(v)],
      Bw: [bearingAt, 0],
      Tw: [bearingAt, top(bearingAt)],
      X1: [l + p.tail, -p.tail * k],
    },
    topChords: [
      ["H0", "T1", "P"],
      ["X1", "H1", "Tw", "T1'", "P"],
    ],
    bottomChords: [["F", "H0", "C", "Bw", "H1"]],
    webs: [
      ["P", "C"], // centre vertical
      ["T1", "C"], // left diagonal
      ["T1'", "C"], // right diagonal
      ["Tw", "Bw"], // vertical over the inner bearing wall
      ["T1'", "Bw"], // diagonal from its foot
    ],
  })
    .sized("bottom_chord", p.bottomChord)
    .sized("top_chord", p.topChord)
    .sized("web", p.topChord);
}

/**
 * Fitted to the tape readings in garage-truss.ts (7 measurements, exactly
 * determined; 2026-09-26): pitch 4.67:12, heel-to-heel 24'-10 3/16".
 */
export const fittedGarageTruss: TrussParams = {
  pitch: 0.3890211569583045,
  span: 7.573881130366942,
  flat: 0.9605188696330536,
  rightVertical: 2.056700479366872,
  tail: 0.6413500000000009,
  topChord: [2, 4],
  bottomChord: [2, 6],
};

/** Bearing wall centrelines, horizontally from the left heel node (fitted). */
export const fittedWalls = { left: 0.11390113036694521, right: 7.345281130366942 };
