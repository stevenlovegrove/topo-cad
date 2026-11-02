// A girder truss carrying hung commons (examples/primitives/34-a-girder-truss-carrying-commons.ts), on the part core.
//
// A building 16' by 24', its walls 8' tall, of 2x4 at 16". Across its middle, between its side walls, a girder truss of
// two plies (each a Howe, four panels; their chords nailed together, ten 16d through the face, chord to chord) bears on
// them where it crosses them. Either side of it, seven common trusses (king posts, every
// 2') span from the front wall, and from the back, to the girder: each bears on its wall at one heel, and at the other
// is hung on the girder's side, its heel cut plumb against it, in a face-mount hanger on the girder's bottom chord: the
// lightest of the hangers table that fits a 2x4 and carries its load, checked against its catalog's allowable load. So the girder carries half of each common: it is
// what no wall does, its plies sharing it. All at 6 in 12, of 2x4s; snow 25 psf on plan.

import { part, node, plane, frame, support, areaLoad, hanger, cut, cases, lumber, nominal, DFL, inch, ft, mm, psf, sub, add, type Length, type PartBase } from "topo-part";
import { perimeter } from "topo-part/walls";
import { trussRow } from "topo-part/framing";
import { howe, kingPost, plied } from "topo-part/trusses";

const stud = lumber(nominal["2x4"], DFL.No2), chord = stud;
const [width, depth, height, pitch] = [ft(16), ft(24), ft(8), 6 / 12];
// (The girder's near face, a ply's thickness before the middle: its two plies either side of it.)
const y0 = sub(ft(12), chord.thickness);
const span = y0;
const stations: Length[] = [1, 2, 3, 4, 5, 6, 7].map((k) => ft(2 * k));

export default part("girder and hung commons", () => {
  const at = (x: Length | 0, y: Length | 0, z: Length | 0 = 0) => node(x, y, z);
  const walls = perimeter("walls", { corners: [at(0, 0), at(width, 0), at(width, depth), at(0, depth)], legs: { height, spacing: inch(16), stock: stud } });
  // The girder: its first ply in the plane across the building at y0, its pieces laid from it toward the back, the
  // second on it; their heels at the side walls' outsides, their bottom chords' undersides on their plates.
  const girder = plied("girder", {
    plies: 2, thickness: chord.thickness, nails: { count: 10, penny: 16 },
    make: (k, f) => howe(`ply ${k + 1}`, { span: width, pitch, panels: 4, chord, web: chord, at: f }),
    at: frame(at(0, y0, height), { x: at(ft(1), y0, height), y: at(0, add(y0, ft(1)), height) }),
  });
  const [near, far] = girder.plies;
  // The commons: from the front wall's outside to the girder's near face; from the back wall's outside to its far face
  // (laid out from the back right corner, along −x, so that they rise the same way).
  const common = (f: ReturnType<typeof frame>) => kingPost("truss", { span, pitch, chord, web: chord, at: f });
  const front = trussRow("front commons", { stations, height: mm(0), make: (_, f) => common(f), at: frame(at(0, 0, height), { x: at(ft(1), 0, height), y: at(0, ft(1), height) }) });
  const back = trussRow("back commons", { stations, height: mm(0), make: (_, f) => common(f), at: frame(at(width, depth, height), { x: at(sub(width, ft(1)), depth, height), y: at(width, sub(depth, ft(1)), height) }) });
  // (Each common's top chord at its hung heel, its second: cut against the girder's side it is hung on (what of it is
  // past that face, taken off), and held to the girder's bottom chord by a hanger.)
  const rows = [[front, near], [back, far]] as const;
  const hung = rows.flatMap(([r, ply], k) => (r as unknown as { trusses: { top: PartBase[] }[] }).trusses.map((t) => ({
    against: cut(t.top[1], { tool: "planes", planes: [ply.bottom.faces[k === 0 ? "-y" : "+y"]] }, "against the girder"),
    hanger: hanger(t.top[1], ply.bottom),
  })));
  const ground = [node(0, 0, 0), node(mm(1), 0, 0), node(0, mm(1), 0)];
  return {
    walls, girder, front, back, hung, ground,
    slab: support(plane(ground[0], ground[1], ground[2])),
    snow: [areaLoad(front, psf(25), cases.snow), areaLoad(back, psf(25), cases.snow), areaLoad(girder, psf(25), cases.snow)],
  };
});
