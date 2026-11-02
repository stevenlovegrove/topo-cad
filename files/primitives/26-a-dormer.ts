// A dormer (examples/primitives/26-a-dormer.ts), written on the part core: a roof on a roof.
//
// A gable roof at 8 in 12 over walls 24' by 20', 8' tall, and on its front slope a small part of its own: a dormer, 6'
// wide, its eaves 11' up, at 6 in 12, its ridge running back into the slope. It is another part of the same roof
// (`roofSurface`, topo-part/framing), and where it is the higher of the two it is the roof: its ridge stops where it
// meets the slope, and the valleys either side are where its planes cross the main roof's. Nothing is under a dormer's
// eaves, so their height is given. Its front is a gable. (The surfaces only: the dormer's walls and framing are not
// drawn.)

import { part, node, plane, support, layers, weights, lumber, nominal, DFL, inch, ft, mm, type Length } from "topo-part";
import { perimeter } from "topo-part/walls";
import { roofSurface } from "topo-part/framing";

const stud = lumber(nominal["2x4"], DFL.No2);
const [wide, deep] = [24, 20];
const eave = (pitch: number, height: Length, overhang: Length) => ({ pitch, height: [height, height] as [Length, Length], overhang });
/** A rectangle on plan, in feet; its edges along low y, high x, high y, low x. */
const rect = (x0: number, y0: number, x1: number, y1: number): [Length, Length][] => [[ft(x0), ft(y0)], [ft(x1), ft(y0)], [ft(x1), ft(y1)], [ft(x0), ft(y1)]];

export default part("a dormer", () => {
  const walls = perimeter("walls", { corners: [node(0, 0, 0), node(ft(wide), 0, 0), node(ft(wide), ft(deep), 0), node(0, ft(deep), 0)], legs: { height: ft(8), spacing: inch(16), stock: stud } });
  const roof = roofSurface("roof", {
    parts: [
      // The main roof: its eaves along the front and the back, on the walls; gables at the ends.
      { outline: rect(0, 0, wide, deep), edges: [eave(8 / 12, ft(8), inch(12)), null, eave(8 / 12, ft(8), inch(12)), null], rake: inch(8) },
      // The dormer: its eaves along its sides, 11' up; its front a gable, its back run into the main roof.
      { outline: rect(9, 2, 15, 10), edges: [null, eave(6 / 12, ft(11), inch(6)), null, eave(6 / 12, ft(11), inch(6))], rake: inch(6) },
    ],
    build: layers(weights.asphaltShingles, weights.roofingFelt, weights.osb7_16),
  });
  const ground = [node(0, 0, 0), node(mm(1), 0, 0), node(0, mm(1), 0)];
  return { walls, roof, ground, slab: support(plane(ground[0], ground[1], ground[2])) };
});
