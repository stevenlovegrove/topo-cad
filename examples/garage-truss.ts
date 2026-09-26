// The existing garage roof truss, reconstructed from field measurements.
//
// A Pratt variant with two verticals (centre, and under the right diagonal),
// the bottom chord continuing past the left slope as a flat roof area, and a
// plumb-cut eave tail on the right. Everything is derived from the measured
// values below, so correcting a measurement updates the whole truss.
import {
  Building, DFL, dressed, ft, inch, type Length, type Nominal, Perimeter, type Point, psf, TrussRoof, TrussShape, Wall,
} from "topo-cad";

export interface TrussMeasurements {
  readonly topChord: Nominal;
  readonly bottomChord: Nominal;
  /** Bottom chord lumber, end to end. */
  readonly bottomChordLength: Length;
  /**
   * Flat part: from the left end of the bottom chord to the *outer* corner,
   * where the top face of the top chord meets the top of the bottom chord.
   */
  readonly leftFlat: Length;
  /** Centre vertical: clear height from the top of the bottom chord to the underside of the top chord. */
  readonly centerVertical: Length;
  /** Right vertical (at the top of the right diagonal): clear height, as above. */
  readonly rightVertical: Length;
  /** Eave tail: from the right end of the bottom chord to the plumb cut end of the top chord, horizontal. */
  readonly rightTail: Length;
  /** Right wall centreline, measured in from the right end of the bottom chord. */
  readonly rightWallFromEnd: Length;
  /** Wall thickness (both walls). */
  readonly wallThickness: Length;
}

export const measured: TrussMeasurements = {
  topChord: [2, 4],
  bottomChord: [2, 6],
  bottomChordLength: ft(28),
  leftFlat: inch(42.3),
  centerVertical: inch(52.5),
  rightVertical: inch(26),
  rightTail: inch(25.25),
  rightWallFromEnd: inch(9),
  wallThickness: inch(6),
};

/**
 * Geometry implied by the measurements, in the truss's own coordinates: s
 * along the span from the left heel node (where the top chord's underside
 * meets the bottom of the bottom chord), z up. With pitch k and node span L:
 *   centre clear height   Vc = k·L/2 − d_bottom
 *   outer corner          s₀ = (d_bottom − d_top·√(1+k²)) / k
 *   bottom chord lumber      = leftFlat − s₀ + L    (right end cut to the slope)
 * The last two depend on each other through k, so solve by iteration.
 */
export function solve(m: TrussMeasurements) {
  const dt = dressed(m.topChord[1]);
  const db = dressed(m.bottomChord[1]);
  let k = 0.4;
  let span = 0;
  let outer = 0;
  for (let i = 0; i < 100; i++) {
    outer = (db - dt * Math.hypot(1, k)) / k;
    span = m.bottomChordLength - m.leftFlat + outer;
    k = (2 * (m.centerVertical + db)) / span;
  }
  const rightWall = span - m.rightWallFromEnd;
  const vertical = span - (m.rightVertical + db) / k; // right vertical centreline
  return {
    pitch: k,
    pitchPer12: 12 * k,
    /** Heel node to heel node. */
    span,
    /** Left outer corner = left wall centreline. */
    leftWall: outer,
    rightWall,
    /** Wall centreline to wall centreline. */
    bearingSpan: rightWall - outer,
    /** Right vertical position as a fraction of the bearing span, from the right wall. */
    rightVerticalFraction: (rightWall - vertical) / (rightWall - outer),
    bottomDepth: db,
  };
}

export function garageTruss(m: TrussMeasurements = measured): TrussShape {
  const g = solve(m);
  const { span: l, pitch: k, bottomDepth: db } = g;
  const top = (s: number) => k * Math.min(s, l - s); // underside of the top chord
  const sVert = (m.rightVertical + db) / k; // from each heel node
  const left = g.leftWall - m.leftFlat; // left end of the flat part
  const at = (s: number): Point => [s, 0];
  return (
    TrussShape.custom({
      name: "Existing (Pratt variant)",
      span: l,
      pitch: k,
      points: {
        F: [left, 0],
        H0: [0, 0],
        H1: [l, 0],
        P: [l / 2, top(l / 2)],
        C: [l / 2, 0],
        T1: [sVert, top(sVert)],
        "T1'": [l - sVert, top(sVert)],
        B3: [l - sVert, 0],
        X1: [l + m.rightTail, -m.rightTail * k], // plumb-cut tail end
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
      .sized("bottom_chord", m.bottomChord)
      .sized("top_chord", m.topChord)
      .sized("web", m.topChord)
      // Dimensioned the way it was measured: a chain along the bottom…
      .dim(at(left), at(g.leftWall), "below", 0) // flat part (to the left wall centreline)
      .dim(at(g.leftWall), at(g.rightWall), "below", 0) // wall ℄ to wall ℄
      .dim(at(g.rightWall), at(l), "below", 0) // right wall ℄ to chord end
      .dim(at(l), at(l + m.rightTail), "below", 0) // eave tail
      .dim(at(left), at(l), "below", 1) // bottom chord lumber
      // …and the clear heights of the verticals.
      .dim([l / 2 + 0.3, db], [l / 2 + 0.3, top(l / 2)], "right", 0)
      .dim([l - sVert + 0.2, db], [l - sVert + 0.2, top(sVert)], "right", 0)
  );
}

// ----- stand-alone preview: a few trusses on the two bearing walls ------------
// Left wall centred under the outer corner; right wall centred 9" in from the
// bottom chord's end (both confirmed on site). Wall height is a placeholder.

const g = solve(measured);
const run = ft(6);
const wall = Wall.template({ height: ft(8), grade: DFL.No2 }).studs([2, 6]).justify("center");
const leftWall = Perimeter.start("Left wall", [0, 0]).to([run, 0], wall.named("Left bearing wall"));
const rightWall = Perimeter.start("Right wall", [0, g.bearingSpan]).to([run, g.bearingSpan], wall.named("Right bearing wall"));

const roof = TrussRoof.of(garageTruss(), {
  name: "Existing trusses",
  origin: [0, -g.leftWall], // heel node, relative to the left wall centreline
  spanDir: [0, 1],
  length: run,
  spacing: inch(24),
  grade: DFL.No2,
})
  .bearingOn(leftWall, rightWall)
  .load("dead", psf(20))
  .load("snow", psf(25));

export default Building.named("Existing garage truss")
  .info({
    designer: "topo-cad",
    date: "2026-09-26",
    notes: [
      `Derived pitch ${g.pitchPer12.toFixed(2)}:12; bearing span ${(g.bearingSpan / 0.3048).toFixed(2)} ft (wall centrelines).`,
      "2x6 bottom chord; 2x4 top chords and webs. Eave tail plumb cut.",
      "Left flat part cantilevers past the left wall. Wall height is a placeholder.",
    ],
  })
  .add(leftWall, rightWall, roof);
