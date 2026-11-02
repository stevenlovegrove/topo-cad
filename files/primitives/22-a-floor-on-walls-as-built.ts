// A floor on walls as they are built: each joist sits for itself (examples/primitives/22-a-floor-on-walls-as-built.ts),
// written on the part core.
//
// A floor that is one rigid thing sits on walls whose tops are in one plane. Walls as built are not: here the back wall
// was surveyed 5 mm taller at one end than at the other, the front wall level, so the ends of the two tops are a twist,
// and no plane lies on them. (The two walls its joists span: walls at its sides, their tops sloping to meet the back's
// at its corners, are framing not made yet.) The floor is `loose`: nothing joins its joists, so each joist, each band joist and each rim is a
// thing of its own, each end of it sat on the wall under it, as each truss of a roof is. Each joist then slopes a little
// differently from the next, front to back, as the real ones do.
//
// Each sits where it first touches: its front end on the front wall, where it is lowest; its back end, rising to the
// back wall, which rises to the east, on the top plate's inner edge, under its east side. (The pieces are still rigid,
// each square in itself: set on a twist, a joist's end leans into the rim it butts by a few tenths of a millimetre,
// where a real one would crush or be shimmed. It is built to half a millimetre.)

import { part, bears, node, free, plane, support, placed, facing, neg, must, on, areaLoad, cases, designBasis, lumber, nominal, DFL, add, inch, ft, mm, psf, type Length, type PartBase } from "topo-part";
import { floor } from "topo-part/framing";
import { wall } from "topo-part/walls";

const stud = lumber(nominal["2x4"], DFL.No2), joist = lumber(nominal["2x10"], DFL.No2);
const [width, depth, height] = [ft(12), ft(10), ft(8)];

export default part("a floor on walls as built", () => {
  // The walls, each outside out: the front, level; the back, surveyed 5 mm higher where it starts (its east end) than
  // where it ends.
  const leg = { spacing: inch(16), stock: stud };
  const front = wall("Front", { ...leg, from: node(0, 0, 0), to: node(width, 0, 0), height });
  const back = wall("Back", { ...leg, from: node(width, depth, 0), to: node(0, depth, 0), height: [add(height, mm(5)), height] });
  // The floor, its pieces loose, drawn on the walls' tops where they would be level: each end of each found where it sits.
  const f = floor("floor", { width, length: depth, spacing: inch(16), stock: joist, loose: true, at: placed({ origin: [0, 0, height] }) });
  // (Its seats, as it gives them: each rim's two ends (the front's, then the back's), each band joist's (the west's, front
  // then back; the east's, back then front), then each joist's, its front end and its back end.)
  const top = (w: typeof front) => w.topPlate.faces["-y"];
  const under = (k: number) => (k < 8 ? [front, front, back, back, front, back, back, front][k] : k % 2 === 0 ? front : back);
  // (The pieces whose seats are on the back wall but the rim's: the bands', and the joists'.)
  const piece = (k: number): PartBase | undefined => (k === 5 ? f.bands[0] : k === 6 ? f.bands[1] : k >= 8 && k % 2 === 1 ? f.joists[(k - 9) / 2] : undefined);
  const touches = f.seats!.flatMap((s, k) => (piece(k) ? [{ k, at: node(free(s.x as Length), free(depth), free(height)) }] : []));
  const sits = Object.fromEntries([
    ...f.seats!.flatMap((s, k) => (piece(k) ? [] : [[`seat ${k} on its wall`, must(on(s, top(under(k))))]])),
    ...touches.flatMap(({ k, at }) => {
      const p = piece(k)!;
      return [
        [`seat ${k} on the back wall's top`, must(on(at, top(back)))], [`seat ${k} at its inside edge`, must(on(at, back.faces.inside))],
        [`seat ${k} under the piece`, must(on(at, facing(p, neg(f.z))))], [`seat ${k} under its east side`, must(on(at, facing(p, f.x)))],
      ];
    }),
  ]);
  const ground = [node(0, 0, 0), node(mm(1), 0, 0), node(0, mm(1), 0)];
  return {
    front, back, floor: f, touches: touches.map((t) => t.at), ground,
    // (Its load path: the floor on its walls.)
    rests: [bears(f, front), bears(f, back)],
    slab: support(plane(ground[0], ground[1], ground[2])),
    dead: areaLoad(f, psf(10), cases.dead),
    live: areaLoad(f, psf(40), cases.live),
    design: designBasis({ tolerance: mm(0.5) }),
    says: sits,
  };
});
