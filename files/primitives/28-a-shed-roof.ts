// A shed roof (examples/primitives/28-a-shed-roof.ts), written on the part core.
//
// One edge of the roof is an eave (the front), the others are not: so it is one plane, rising from the front wall to the
// back at 3 in 12. Framed with trusses, each a mono truss (topo-part/trusses, laid out as the present
// pipeline lays one out: three panels, a post at its high end, its outer face flush with the end): its heel on the front
// wall's outside, its high end over the back wall's, its bottom chord level on both plates, its top chord running a
// foot out past the front wall. Walls 16' by 10' (their outsides), 8' tall, of 2x4 at 16"; trusses of 2x4 every 24".
// Snow 25 psf on plan.

import { part, node, plane, support, areaLoad, cases, frame, neg, lumber, nominal, DFL, add, div, inch, ft, mm, as, psf, type Length } from "topo-part";
import { row } from "topo-part/framing";
import { perimeter } from "topo-part/walls";
import { truss } from "topo-part/trusses";

const stud = lumber(nominal["2x4"], DFL.No2);
const [wide, deep, height] = [ft(16), ft(10), ft(8)];

/** A mono truss `length` long at pitch `k`, `over` past its heel, of `n` panels: its post's line half its depth in from
 *  the end. (Its points in millimetres, worked out from its length and overhang.) */
function mono(name: string, length: Length, k: number, over: Length, n: number, at: ReturnType<typeof frame>) {
  const [l, o] = [as(length, mm), as(over, mm)];
  const post = l - as(stud.depth, mm) / 2;
  const p = (x: number, z: number): [Length, Length] => [mm(x), mm(z)];
  const points: Record<string, [Length, Length]> = { X0: p(-o, -o * k), H0: p(0, 0), H1: p(l, 0), E: p(l, l * k), Ei: p(post, post * k), Hi: p(post, 0) };
  for (let i = 1; i < n; i++) {
    points[`T${i}`] = p((i * l) / n, (i * l * k) / n);
    points[`B${i}`] = p((i * l) / n, 0);
  }
  const inner = Array.from({ length: n - 1 }, (_, i) => i + 1);
  const webs: [string, string][] = [["Hi", "Ei"], ...inner.flatMap((i): [string, string][] => [[`T${i}`, `B${i}`], [`B${i}`, i + 1 < n ? `T${i + 1}` : "Ei"]])];
  return truss(name, { points, top: [["X0", "H0", ...inner.map((i) => `T${i}`), "Ei", "E"]], bottom: ["H0", ...inner.map((i) => `B${i}`), "Hi", "H1"], webs, topChord: stud, bottomChord: stud, web: stud, at });
}

export default part("a shed roof", () => {
  const w = perimeter("walls", { corners: [node(0, 0, 0), node(wide, 0, 0), node(wide, deep, 0), node(0, deep, 0)], legs: { height, spacing: inch(16), stock: stud } });
  // Each truss across the depth, its heel at the front wall's outside, its bottom chord's underside on the plates; its
  // pieces centred on its line, every 24" along the width.
  const roof = part("roof", (r) => {
    const lines = row(wide, inch(24), stud.thickness);
    const heels = lines.map((x) => [node(add(x, div(stud.thickness, 2)), 0, height), node(add(x, div(stud.thickness, 2)), deep, height)]);
    // (Its x across the depth, its y west: so its z is up.)
    const trusses = heels.map((h) => mono("truss", deep, 3 / 12, inch(12), 3, frame(h[0], { x: h[1], y: neg(r.x) })));
    return { heels, trusses };
  });
  const ground = [node(0, 0, 0), node(mm(1), 0, 0), node(0, mm(1), 0)];
  return {
    walls: w,
    roof,
    ground,
    slab: support(plane(ground[0], ground[1], ground[2])),
    snow: areaLoad(roof, psf(25), cases.snow),
  };
});
