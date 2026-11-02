// The existing garage roof truss, fitted to field measurements (examples/garage-truss.ts), written on the part core.
//
// The truss is a parametric shape (examples/garage-truss-shape.ts: a Pratt variant). Its numbers are free,
// started at guesses; so are where its two bearing walls stand. The tape readings are measurements between physical
// features: faces, corners where faces meet, the clear lengths of webs. Each corner is a node held on the faces that
// make it; each reading is a distance between such things. Settling fits the numbers to the readings, seven of seven:
// there is no hand-derived geometry here.
//
// Its trusses span the walls' x, from the left heel at the origin; the bearing walls (2x6, 8' tall, a placeholder) run
// along y under them (each a leg, square to the trusses, where its east face is found), their bottom chords' undersides
// on the walls' plates. Roof dead load 20 psf, snow 25 psf, on plan.

import { part, node, free, plane, line, support, areaLoad, must, read, on, distance, frame, sheet, plan, iso, elevation, utilization, schedule, standardSheets, cases, lumber, nominal, DFL, add, sub, max, inch, ft, mm, psf, ratio, type Length, type Num } from "topo-part";
import { wall } from "topo-part/walls";
import { garageTruss } from "./garage-truss-shape";

const twoByFour = lumber(nominal["2x4"], DFL.No2);
const twoBySix = lumber(nominal["2x6"], DFL.No2);
const height = ft(8);
const run = ft(6);

/** A bearing wall along y, `run` long, its east face at `x` (free: a guess): a leg from y = 0 north, its +z east. */
const bearing = (name: string, x: Num<"length">) => wall(name, { from: node(x, 0, 0), to: node(x, run, 0), height, spacing: inch(16), stock: twoBySix });

export default part("existing garage truss", (g) => {
  // The unknowns: guesses only.
  const pitch = free(ratio(4.5, 12)), span = free(ft(25)), flat = free(ft(3)), rightVertical = free(ft(6)), tail = free(ft(2));
  const faces = { left: free(mm(70)), right: free(add(ft(24), mm(70))) };
  const left = bearing("left wall", faces.left), right = bearing("right wall", faces.right);
  // The trusses, every 24" along the walls, each its left heel over x = 0, its bottom chord's underside on the walls.
  const roof = part("roof", (r) => {
    // (Each truss's plane half its thickness back from its line, its pieces laid from it along y: centred on it; the
    // first flush with the walls' end.)
    const half = mm(19.05);
    const heels = [0, 1, 2, 3].map((k) => max(inch(24 * k), half)).map((y) => [node(0, sub(y, half), height), node(mm(1000), sub(y, half), height)]);
    const trusses = heels.map((h) => garageTruss("truss", { pitch, span, flat, rightVertical, tail, topChord: twoByFour, bottomChord: twoBySix, at: frame(h[0], { x: h[1], y: r.y }) }));
    return { heels, trusses };
  });
  const trusses = roof.trusses as ReturnType<typeof garageTruss>[];
  // Field measurements, on an interior truss (T2).
  const ground = [node(0, 0, 0), node(mm(1), 0, 0), node(0, mm(1), 0)];
  const T = trusses[2];
  const at = (n: string) => `trusses[2]/${n}`;
  // (A corner where faces meet: a node held on each, in the truss's plane.)
  const corner = (x: Length, z: Length) => node(free(x), sub(inch(48), mm(19.05)), free(z));
  const outerCorner = corner(ft(3), add(height, mm(140))), chordEnd = corner(ft(25), height);
  const centreFoot = corner(ft(12.5), add(height, mm(140))), verticalFoot = corner(ft(19), add(height, mm(140)));
  const wallMiddle = (w: typeof left) => plane(w.start, w.end, w.startUp);
  const [centreLine, verticalLine] = [line(T.points.P, T.points.C), line(T.points["T1'"], T.points.B3)];
  const middles = { left: wallMiddle(left), right: wallMiddle(right) };
  return {
    pitch, span, flat, rightVertical, tail,
    faces, left, right, roof,
    ground,
    slab: support(plane(ground[0], ground[1], ground[2])),
    outerCorner, chordEnd, centreFoot, verticalFoot, centreLine, verticalLine, middles,
    says: {
      // Where the top face of the left top chord meets the top of the bottom chord.
      "outer corner, on the top chord": must(on(outerCorner, T.top[0].faces["+z"])),
      "outer corner, on the bottom chord": must(on(outerCorner, T.bottomTop)),
      // The bottom chord's right end, at its long point, on its bottom face.
      "chord end, at its end": must(on(chordEnd, T.under[1])),
      "chord end, on its bottom": must(on(chordEnd, T.bottom.faces["-z"])),
      // Where each vertical's line meets the bottom chord's top: the foot of its clear length.
      "centre foot, on its line": must(on(centreFoot, centreLine)),
      "centre foot, on the bottom chord": must(on(centreFoot, T.bottomTop)),
      "vertical foot, on its line": must(on(verticalFoot, verticalLine)),
      "vertical foot, on the bottom chord": must(on(verticalFoot, T.bottomTop)),
    },
    // The drawing set, in tab order: its own sheets, and the standard ones of its walls.
    sheets: [
      sheet("S-101", "Roof framing", plan(roof, { title: "Roof framing plan" }), iso([roof, left, right], { title: "Roof framing, from the south-west" })),
      sheet("S-102", "Typical truss", elevation(T, { title: "Typical truss T2", scale: '1/2"' })),
      sheet("S-201", "Walls", standardSheets("walls")),
      sheet("S-301", "Utilization", utilization(roof, { title: "Roof framing utilization" })),
      sheet("S-401", "Schedules", schedule("members")),
    ],
    loads: {
      dead: areaLoad(roof, psf(20), cases.dead),
      snow: areaLoad(roof, psf(25), cases.snow),
    },
    measured: {
      "bottom chord lumber": read(distance(T.points.F, chordEnd, ft(28)), mm(1)),
      "flat: chord end to outer corner": read(distance(outerCorner, T.plumb.F, inch(42.3)), mm(1)),
      "centre vertical, clear": read(distance(centreFoot, T.points.P, inch(52.5)), mm(1)),
      "right vertical, clear": read(distance(verticalFoot, T.points["T1'"], inch(26)), mm(1)),
      "eave tail past chord end": read(distance(chordEnd, T.plumb.X1, inch(25.25)), mm(1)),
      "right wall ℄ in from chord end": read(distance(chordEnd, middles.right, inch(9)), mm(1)),
      "left wall ℄ under outer corner": read(on(outerCorner, middles.left), mm(1)),
    },
  };
});
