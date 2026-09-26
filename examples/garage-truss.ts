// The existing garage roof truss, fitted to field measurements.
//
// The truss is an ordinary parametric shape (a Pratt variant: centre vertical
// plus one vertical on the right, a flat part on the left, a plumb-cut eave
// tail on the right). Its parameters are *unknowns*; the tape readings at the
// bottom are *measurements between physical features* (faces, corners, cut
// ends). Running the model fits the unknowns to the measurements — there is no
// hand-derived geometry here.
import {
  Building, DFL, dressed, ft, horizontal, inch, lengthOf, meet, measured, type Nominal, Perimeter, type Point, psf,
  TrussRoof, TrussShape, unknown, Wall,
} from "topo-cad";
import { fieldMeasurements } from "./garage-truss.measured";

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

// ----- unknowns (guesses only) ------------------------------------------------

const params: TrussParams = {
  pitch: unknown("pitch", 4.5 / 12, { min: 0.05, max: 2 }),
  span: unknown("span", ft(25), { unit: "length", min: ft(4) }),
  flat: unknown("flat", ft(3), { unit: "length", min: 0 }),
  rightVertical: unknown("rightVertical", ft(6), { unit: "length", min: inch(4) }),
  tail: unknown("tail", ft(2), { unit: "length", min: 0 }),
  topChord: [2, 4],
  bottomChord: [2, 6],
};
// Wall centrelines, horizontally from the left heel node.
const leftWallAt = unknown("leftWall", 0, { unit: "length" });
const rightWallAt = unknown("rightWall", ft(24), { unit: "length" });

// ----- the preview model: trusses on the two bearing walls -------------------

// Drawing dimensions in the terms things were measured (positions follow from
// the fitted parameters; the outer corner is where the top chord's top face
// meets the top of the bottom chord).
const { span: l, pitch: k } = params;
const outer = (dressed(params.bottomChord[1]) - dressed(params.topChord[1]) * Math.hypot(1, k)) / k;
const at = (s: number): Point => [s, 0];
const shape = garageTruss(params)
  .dim(at(-params.flat), at(outer), "below", 0)
  .dim(at(outer), at(rightWallAt), "below", 0)
  .dim(at(rightWallAt), at(l), "below", 0)
  .dim(at(l), at(l + params.tail), "below", 0)
  .dim(at(-params.flat), at(l), "below", 1);

const run = ft(6);
const wall = Wall.template({ height: ft(8), grade: DFL.No2 }).studs([2, 6]).justify("center");
const leftWalls = Perimeter.start("Left wall", [0, leftWallAt]).to([run, leftWallAt], wall.named("Left bearing wall"));
const rightWalls = Perimeter.start("Right wall", [0, rightWallAt]).to([run, rightWallAt], wall.named("Right bearing wall"));
const roof = TrussRoof.of(shape, { name: "Existing trusses", origin: [0, 0], spanDir: [0, 1], length: run, spacing: inch(24), grade: DFL.No2 })
  .bearingOn(leftWalls, rightWalls)
  .load("dead", psf(20))
  .load("snow", psf(25));

// ----- field measurements (on an interior truss, T2) -------------------------

const T = (name: string) => roof.trussMember(2, name);
const bc = T("bottom_chord.F-H1");
const leftTop = T("top_chord.H0-P");
const rightTop = T("top_chord.X1-P");
/** Where the top face of the left top chord meets the top of the bottom chord. */
const outerCorner = meet(leftTop.face("top"), bc.face("top"));
/** The bottom chord's right end (its long point, on the bottom face). */
const chordEnd = meet(bc.face("end"), bc.face("bottom"));
const leftWallCL = leftWalls.wallMember("Left bearing wall", "cap_plate").mid("depth");
const rightWallCL = rightWalls.wallMember("Right bearing wall", "cap_plate").mid("depth");

export const measurements = [
  measured("bottom chord lumber", lengthOf(bc, "long"), ft(28)),
  measured("flat: chord end to outer corner", horizontal(bc.face("start"), outerCorner), inch(42.3)),
  measured("centre vertical, clear", lengthOf(T("web.P-C"), "centreline"), inch(52.5)),
  measured("right vertical, clear", lengthOf(T("web.T1'-B3"), "centreline"), inch(26)),
  measured("eave tail past chord end", horizontal(chordEnd, rightTop.face("start")), inch(25.25)),
  measured("right wall ℄ in from chord end", horizontal(rightWallCL, chordEnd), inch(9)),
  measured("left wall ℄ under outer corner", horizontal(leftWallCL, outerCorner), 0),
];

export default Building.named("Existing garage truss")
  .info({
    designer: "topo-cad",
    date: "2026-09-26",
    notes: ["2x6 bottom chord; 2x4 top chords and webs. Eave tail plumb cut.", "Left flat part cantilevers past the left wall. Wall height is a placeholder."],
  })
  .add(leftWalls, rightWalls, roof)
  .measure(measurements, fieldMeasurements);
