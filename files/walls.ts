// Walls on an outline, written on the part core (topo-part), as objects: a part's nodes, params,
// features and children are its fields, and what refers to a thing refers to it.
//
// The outline's frame is the walls' frame: its surface z = 0, what they stand on. Each corner has a
// raised peer sharing its x and y. Each run is a part whose frame is made of its corners (x along
// it, y up in the wall's plane, z out of it); its plates span its corners, and the faces of the
// plates that the studs meet are edges measured by the run's x. How many studs follows the run's
// length, so they are made in a scope once the length is found: each a mark along the run, its
// layout a hint, spanning from one plate's face to the other at its mark.

import { part, free, param, node, onEdge, edge, plane, facing, neg, dist, div, mul, frame, fallback, must, read, coincide, distance, at, when, box, span, endOn, SPF, mm, type Length, type Stock } from "topo-part";

function walls(outline: [Length, Length][], height0: Length, spacing: Length, twoByFour: Stock) {
  const n = outline.length;
  const height = free(height0);
  const corners = outline.map(([x, y]) => node(free(x), free(y), 0));
  const tops = corners.map((c) => node(c.x, c.y, height));
  // (An outline of two corners is one run; of more, a closed loop.)
  const runs = corners.slice(0, n === 2 ? 1 : n).map((c, i) => {
    const d = corners[(i + 1) % n], ct = tops[i], dt = tops[(i + 1) % n];
    const length = param(dist(c, d));
    const start = node(0, 0, 0), end = node(length, 0, 0);
    const startUp = node(0, ct.z!, 0), endUp = node(length, dt.z!, 0);
    // (Made as a function of the run itself: its plates are laid by its own axes.)
    const run = part(`run ${i}`, (run) => {
      const plate = span(start, end, run.y, twoByFour);
      const topPlate = span(startUp, endUp, neg(run.y), twoByFour);
      // (The faces the studs meet, measured along the run: a stud's ends on both at one mark.)
      const bottom = edge(plate.front.start, plate.front.end, run.x);
      const top = edge(topPlate.front.start, topPlate.front.end, run.x);
      return { length, start, end, startUp, endUp, plate, topPlate, bottom, top };
    }, { at: frame(c, { x: d, xy: ct }) });
    // How many studs follows the run's length: once it is found.
    when(run, (s) => {
      const marks = Array.from({ length: Math.floor(div(s.value(length), spacing)) - 1 }, (_, k) => free(mul(k + 1, spacing)));
      // (Each stud's ends are the run's, on its plates' faces at its mark; the stud is a part spanning them.)
      const ends = marks.map((m) => ({ low: onEdge(run.bottom, m), high: onEdge(run.top, m) }));
      const studs = ends.map((e) => span(e.low, e.high, run.x, twoByFour));
      // (Each stud's ends on the plates' faces: butts, by construction; on a sloping top, its top is the slope.)
      const butts = studs.map((s) => ({ bottom: endOn(s, run.plate.faces["+y"]), top: endOn(s, run.topPlate.faces["+y"]) }));
      return { marks, ends, studs, butts, layout: marks.map((m, k) => fallback(m, mul(k + 1, spacing))) };
    });
    return run;
  });
  return part("walls", { height, corners, tops, runs });
}

const w = walls(mm.each([[0, 0], [3000, 0]]), mm(2440), mm(406), box("2x4", { thickness: mm(38), depth: mm(89), material: SPF.No1No2 }));
const scan = { s0: node(mm(1228), 0, 0), s1: node(mm(1228), 0, mm(100)), s2: node(mm(1228), mm(100), 0) };
const seen = plane(scan.s0, scan.s1, scan.s2);

// A stud knows its faces only in its own frame (`faces["-y"]`, its back). Which of them faces west is a question of
// where it has been put: asked in the shed's frame (+x east, +y north, +z up), of where it is now, whatever placed
// it. A stud the run's scope has not made yet is named by its address.
export default part("shed", (shed) => {
  const westFace = (k: number) => facing(at(`walls/runs[0]/studs[${k}]`), neg(shed.x));
  const stud3West = westFace(2);
  // Its near end is the shed's origin. Its far end was taped at 2900 from it; a plane fitted to the scan lies on
  // stud 3's west face, 10 east of its layout.
  return {
    walls: w,
    scan,
    seen,
    stud3West,
    says: {
      "near end": must(coincide(w.corners[0], shed.origin)),
      taped: read(distance(w.corners[0], w.corners[1], mm(2900)), mm(2)),
      "stud 3 seen": read(coincide(seen, stud3West), mm(1)),
    },
  };
});
