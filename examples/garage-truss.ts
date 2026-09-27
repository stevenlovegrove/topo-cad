// The existing garage roof truss, fitted to field measurements.
//
// The truss is an ordinary parametric shape (a Pratt variant: centre vertical
// plus one vertical on the right, a flat part on the left, a plumb-cut eave
// tail on the right). Its parameters are *unknowns*; the tape readings at the
// bottom are *measurements between physical features* (faces, corners, cut
// ends). Running the model fits the unknowns to the measurements — there is no
// hand-derived geometry here.
import {
  Building, detail, DFL, dressed, elevation, ft, horizontal, inch, iso, lengthOf, meet, measured, type Nominal, notes, Perimeter, plan, type Point,
  psf, schedule, sheet, standardSheets, TrussRoof, TrussShape, unknown, Wall,
} from "topo-cad";
import { fieldMeasurements } from "./garage-truss.measured";

export { garageTruss, type TrussParams } from "./garage-truss-shape";
import { garageTruss, type TrussParams } from "./garage-truss-shape";

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
const outerCorner = meet(leftTop.facing("up"), bc.facing("up"));
/** The bottom chord's right end (+x: its grain runs left to right) at its long point, on the bottom face. */
const chordEnd = meet(bc.face("+x"), bc.facing("down"));
const leftWallCL = leftWalls.wallMember("Left bearing wall", "cap_plate").mid("z");
const rightWallCL = rightWalls.wallMember("Right bearing wall", "cap_plate").mid("z");

export const measurements = [
  measured("bottom chord lumber", lengthOf(bc, "long"), ft(28)),
  measured("flat: chord end to outer corner", horizontal(bc.face("-x"), outerCorner), inch(42.3)),
  measured("centre vertical, clear", lengthOf(T("web.P-C"), "centreline"), inch(52.5)),
  measured("right vertical, clear", lengthOf(T("web.T1'-B3"), "centreline"), inch(26)),
  measured("eave tail past chord end", horizontal(chordEnd, rightTop.face("-x")), inch(25.25)),
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
  .measure(measurements, fieldMeasurements)
  // The drawing set, in tab order: generated sheets and our own.
  .sheets(
    standardSheets("cover"),
    sheet("S-101", "Roof framing",
      plan(roof, { dashed: [leftWalls, rightWalls], title: "Roof framing plan" }),
      iso([roof, leftWalls, rightWalls], { title: "Roof framing, from the south-west" }),
    ),
    sheet("S-102", "Typical truss",
      elevation(roof.truss(2), { title: "Typical truss T2", scale: '1/2"' }),
      detail(outerCorner, [roof.truss(2), leftWalls], { title: "Left heel", radius: inch(16), scale: '1-1/2"' }),
      detail(chordEnd, [roof.truss(2), rightWalls], { title: "Right heel and eave tail", radius: inch(20), scale: '1-1/2"' }),
      notes("Field notes", "Truss plates not recorded; verify at heels before any repair."),
    ),
    standardSheets("walls", "analytical"),
    sheet("S-401", "Schedules and fit", schedule("members"), schedule("Field measurements"), schedule("Fitted unknowns")),
  );
