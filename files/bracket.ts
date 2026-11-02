// A folded steel bracket, as a library part: a file of its own, which another model includes as a part
// (examples/walls.ts), solved with it or held as it is.
//
// Its base is a plate 60 × 40, 3 thick, with a hole for an M10 bolt. At its far edge it is bent up at an inside
// radius of 2 by `angle` (90 unless said), carrying on into its leg, a plate 50 × 40 with a hole of its own.

import { part, plate, bend, placed, materials, mm, deg, type Num } from "topo-part";

export function bracket(name = "bracket", o: { angle?: Num<"angle">; held?: boolean } = {}) {
  const steel = materials.A36;
  const leg = plate("leg", mm(50), mm(40), mm(3), steel, [{ at: [mm(30), mm(20)], r: mm(5.5) }]);
  const up = bend("bend", leg, { inner: mm(2), thickness: mm(3), from: 0, to: mm(40), angle: o.angle ?? deg(90), material: steel, at: placed({ origin: [mm(60), 0, mm(5)] }) });
  const base = plate("base", mm(60), mm(40), mm(3), steel, [{ at: [mm(30), mm(20)], r: mm(5.5) }]);
  return part(name, { base, bend: up }, { held: o.held });
}

export default bracket();
