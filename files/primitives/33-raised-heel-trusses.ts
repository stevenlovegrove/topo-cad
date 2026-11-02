// Raised-heel trusses (examples/primitives/33-raised-heel-trusses.ts), written on the part core: a roof's planes said
// by its heels, its trusses' tops in them.
//
// Walls 24' by 32' (their outsides), 8' tall, of 2x6 at 16". The roof is a gable at 4 in 12 over the 24', its ridge 9'
// from the front wall: its heels are what put it there (at the front wall 36" over the plate, at the back 12", which its
// longer run takes down to), so its planes are said by them, as a plan set says them. Its surface (asphalt
// shingles on 7/16" OSB) is laid on its planes (`roofSurface`, topo-part/framing); under it, a raised-heel truss every 24"
// (`raisedHeel`, topo-part/trusses: a vertical at each heel, Pratt webs at panel points no more than 6' apart, 2x4s),
// its tops in those planes, 2' overhangs. Dead load 12 psf and snow 25 psf on plan.

import { part, node, plane, support, frame, areaLoad, cases, layers, weights, lumber, nominal, DFL, inch, ft, mm, as, psf, type Length } from "topo-part";
import { perimeter } from "topo-part/walls";
import { roofSurface, trussRow, row } from "topo-part/framing";
import { raisedHeel } from "topo-part/trusses";

const stud = lumber(nominal["2x6"], DFL.No2), chord = lumber(nominal["2x4"], DFL.No2);
const [wide, deep, height, pitch] = [24, 32, 96, 4 / 12];
// (The ridge 9' from the front: its heel 36", the back's what puts its plane through the same ridge.)
const [front, ridgeAt] = [36, 108];
const ridge = height + front + ridgeAt * pitch;
const back = ridge - (wide * 12 - ridgeAt) * pitch - height;
const eave = (h: number) => ({ pitch, height: [inch(height + h), inch(height + h)] as [Length, Length], overhang: ft(2) });

export default part("raised-heel trusses", () => {
  const walls = perimeter("walls", { corners: [node(0, 0, 0), node(ft(wide), 0, 0), node(ft(wide), ft(deep), 0), node(0, ft(deep), 0)], legs: { height: inch(height), spacing: inch(16), stock: stud } });
  // The roof over the 24' (x), its eaves along the front (x = 0) and the back (x = 24'), gables at the ends.
  const roof = roofSurface("roof", {
    parts: [{ outline: [[0, 0], [ft(wide), 0], [ft(wide), ft(deep)], [0, ft(deep)]], edges: [null, eave(back), null, eave(front)], rake: ft(1) }],
    build: layers(weights.asphaltShingles, weights.osb7_16),
  });
  // The trusses across the 24', every 24" along the 32' from its far end, their heels on the walls' plates at the front
  // wall's outside: the row's x back along y, its y across x (so its z is up).
  const trusses = trussRow("trusses", {
    stations: row(ft(deep), inch(24), chord.thickness),
    height: mm(0),
    make: (k, at) => raisedHeel(`truss ${k + 1}`, { span: ft(wide), pitch, heels: [inch(front), inch(back)], overhang: ft(2), chord, web: chord, at }),
    at: frame(node(0, ft(deep), inch(height)), { x: node(0, mm(as(ft(deep), mm) - 1000), inch(height)), y: node(mm(1000), ft(deep), inch(height)) }),
  });
  const ground = [node(0, 0, 0), node(mm(1), 0, 0), node(0, mm(1), 0)];
  return {
    walls,
    roof,
    trusses,
    ground,
    slab: support(plane(ground[0], ground[1], ground[2])),
    dead: areaLoad(trusses, psf(12), cases.dead),
    snow: areaLoad(trusses, psf(25), cases.snow),
  };
});
