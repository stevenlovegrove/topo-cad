// A roof of parts (examples/primitives/32-a-roof-of-parts.ts), written on the part core: two gables meeting in an L,
// their ridges at one height, the valleys where their planes cross found, not given.
//
// A main block 32' by 20' (its walls' outsides), 8' walls, its ridge along the 32'; a wing 16' wide off its back, out
// 14' past it, its ridge across. Both 6 in 12, 2' overhangs and 1' rakes. The main ridge is 5' over its eaves; the wing,
// being narrower, has its eaves 1' higher than the main's so that its ridge meets the main's at one height. Where the
// wing runs into the main roof its gable has no rake. Each part is a convex outline on plan and its edges; where parts
// overlap, the highest is the roof (`roofFaces`), and a covering of the roof's build is laid on each face found
// (`roofSurface`, topo-part/framing).

import { part, layers, weights, inch, ft, type Length } from "topo-part";
import { roofSurface } from "topo-part/framing";

const [pitch, wall] = [6 / 12, 96];
/** A rectangle on plan, in feet: its corners anticlockwise from its low x, low y; its edges in that order (along low y,
 *  high x, high y, low x). */
const rect = (x0: number, y0: number, x1: number, y1: number): [Length, Length][] => [[ft(x0), ft(y0)], [ft(x1), ft(y0)], [ft(x1), ft(y1)], [ft(x0), ft(y1)]];
const eave = (height: number) => ({ pitch, height: [inch(height), inch(height)] as [Length, Length], overhang: ft(2) });

// The ridges' height: the main block's, half its 20' up at 6 in 12 over its walls' 8'.
const ridge = wall + 120 * pitch;

export default part("a roof of parts", () => {
  const roof = roofSurface("roof", {
    parts: [
      // The main block: eaves along its 32' sides, gables at its ends.
      { outline: rect(0, 0, 32, 20), edges: [eave(wall), null, eave(wall), null], rake: ft(1) },
      // The wing off its back (high y), from inside the main block out 14': eaves along its sides, so high that its
      // ridge (8' in from each) is the main's; a gable at its far end, none where it runs into the main roof (low y).
      { outline: rect(8, 10, 24, 34), edges: [null, eave(ridge - 96 * pitch), null, eave(ridge - 96 * pitch)], rake: ft(1), rakes: [0, undefined, undefined, undefined] },
    ],
    build: layers(weights.asphaltShingles, weights.roofingFelt, weights.osb7_16),
  });
  return { roof };
});
