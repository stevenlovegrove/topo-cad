// The sheet seams (examples/sheet-seams.ts), written on the part core: three plates that are cut apart and welded. A
// base 100 by 60, lying flat; a web 80 by 40 standing square on it along its middle; and a cheek 25 by 40 standing
// across the web's end.
//
// Each plate is drawn where it goes (the web: its x along the base, its y up). Its corners where a seam is are free,
// drawn only about where they come, and each seam says the edge it is of (topo-part/sheet): the web's lower
// edge is put in the base's top, the cheek's end on the web's end (inner corner on inner corner), and the cheek's lower
// edge in the base's top. So what a plate meets says where its edges are, not the script.
//
// Each seam is tabbed and TIG welded: two tabs stand on the web's lower edge and go through two slots in the base; the
// cheek and the web meet edge to edge, at a corner, by fingers of the one and the other by turns; one tab stands on the
// cheek's lower edge. The tabs, slots and fingers are in the plates' flat patterns, and the schedule of seams says how
// long each is, the angle between the two, and what a square-cut edge leaves open where they meet.

import { part, free, placed, materials, mm } from "topo-part";
import { plate, onFace, atCorner, welds } from "topo-part/sheet";

export default part("sheet seams", (root) => {
  const [t, alu] = [mm(3), materials.aluminium6061T6];
  const base = plate("base", [["sw", [0, 0]], ["se", [mm(100), 0]], ["ne", [mm(100), mm(60)]], ["nw", [0, mm(60)]]], t, alu);
  // (On the base's top, 10 in from its end, its faces 1.5 either side of the base's middle.)
  const web = plate("web", [["a", [free(mm(0)), free(mm(2))]], ["b", [free(mm(80)), free(mm(2))]], ["c", [mm(80), mm(40)]], ["d", [0, mm(40)]]], t, alu, { at: placed({ origin: [mm(10), mm(31.5), mm(3)], x: [1, 0, 0], y: [0, 0, 1] }) });
  // (Across the web's end: its underside at the web's end; its own end is put at the web's inner face.)
  const cheek = plate("cheek", [["o", [free(mm(0)), free(mm(2))]], ["p", [free(mm(22)), free(mm(2))]], ["q", [free(mm(22)), free(mm(38))]], ["r", [0, mm(40)]]], t, alu, { at: placed({ origin: [mm(90), mm(3.5), mm(3)], x: [0, 1, 0], y: [0, 0, 1] }) });
  const welded = { tabbed: true, fastening: welds.tig };
  return {
    base,
    web,
    cheek,
    seams: {
      "web on base": onFace(root, "web on base", { plate: web, a: "a", b: "b" }, base, welded),
      "cheek to web": atCorner(root, "cheek to web", { plate: cheek, a: "p", b: "q" }, { plate: web, a: "b", b: "c" }, welded),
      "cheek on base": onFace(root, "cheek on base", { plate: cheek, a: "o", b: "p" }, base, welded),
    },
  };
});
