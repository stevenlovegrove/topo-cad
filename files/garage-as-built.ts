// An existing detached garage, modelled as built, on the part core (as examples/garage-as-built.ts). Its 16' garage
// door header has no jack studs: it hangs off its king studs on nails, and roof trusses bear on the wall above it.
//
// 24' by 22' to the outsides of its walls, 8'-1⅛" to the top of their top plates, of DF-L No.2. Its front wall has the
// garage door, centred, its header (2) 2x10 tight under the top plates; its east wall a man door 17' along it; its back
// wall a window, centred. Fink trusses at 24" on centre span its depth, 6/12, 12" overhangs, their heels on the front
// and back walls; roof dead load 20 psf and snow 25 psf on plan.

import { part, node, plane, support, frame, neg, areaLoad, cases, lumber, nominal, DFL, add, div, ft, ftIn, inch, mm, psf, pitch } from "topo-part";
import { fink, row } from "topo-part/framing";
import { perimeter, type Opening } from "topo-part/walls";

const width = ft(24), depth = ft(22), height = ftIn(8, 1.125);
const twoByFour = lumber(nominal["2x4"], DFL.No2);
const header = lumber(nominal["2x10"], DFL.No2, { plies: 2 });
const smallHeader = lumber(nominal["2x8"], DFL.No2, { plies: 2 });
// (Each opening's centre along its wall from the wall's outside corner.)
const garageDoor: Opening = { label: "GD", kind: "door", centre: div(width, 2), width: ft(16), header, jacks: 0, flush: true };
const manDoor: Opening = { label: "D1", kind: "door", centre: ft(17), width: inch(38), head: inch(82.5), header: smallHeader };
const window_: Opening = { label: "W1", kind: "window", centre: div(width, 2), width: ft(3), head: inch(82.5), height: ft(3), header: smallHeader };

export default part("garage", () => {
  // (Its walls' outside corners, anticlockwise from the front's west end.)
  const leg = { height, spacing: inch(16), stock: twoByFour };
  const w = perimeter("walls", {
    corners: [node(0, 0, 0), node(width, 0, 0), node(width, depth, 0), node(0, depth, 0)],
    legs: [{ ...leg, openings: [garageDoor] }, { ...leg, openings: [manDoor] }, { ...leg, openings: [window_] }, leg],
  });
  // Each truss across the depth, its heels on the outsides of the front and back walls and its bottom chord's
  // underside on their top plates; its pieces centred on its line, every 24" along the width.
  const roof = part("roof", (r) => {
    const lines = row(width, inch(24), twoByFour.thickness);
    const half = div(twoByFour.thickness, 2), z = add(height, div(twoByFour.depth, 2));
    const heels = lines.map((x) => [node(add(x, half), 0, z), node(add(x, half), depth, z)]);
    // (Its x across the depth, its y west: so its z is up.)
    const trusses = heels.map((h) => fink("truss", { span: depth, pitch: pitch(6), overhang: inch(12), stock: twoByFour, at: frame(h[0], { x: h[1], y: neg(r.x) }) }));
    return { heels, trusses };
  });
  const ground = [node(0, 0, 0), node(mm(1), 0, 0), node(0, mm(1), 0)];
  return {
    walls: w, roof, ground,
    slab: support(plane(ground[0], ground[1], ground[2])),
    dead: areaLoad(roof, psf(20), cases.dead),
    snow: areaLoad(roof, psf(25), cases.snow),
  };
});
