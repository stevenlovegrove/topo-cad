// A lean-to open at the front, its trusses on a header on posts and on its back wall: what an as-built garage asks of
// framing (docs/part-core.md, "First", step 1), each from `topo-part/framing`.
//
// - Its back wall: 2x6 at 16", 2439 to the top of its plates (a leg, `topo-part/walls`).
// - Across the front, a built-up header on posts (`headerOnPosts`): (3) 2x12 on a (2) 2x6 post at each end, its top 10
//   lower than the walls' (as built, it was set a little low).
// - A row of trusses of two shapes (`trussRow`): Fink commons at 24", and at the north end a gable (studs on its
//   bottom chord, no tails); the third set 19 north of its station (it was the end truss before the gable was added).
//   Each rests on the header's front edge and the back wall's inside edge, its heels' heights found where it touches:
//   on a header lower than the walls, each slopes a little, by what it rests on, not by what is said of it.
// - Its roof's coverings (`covering`): asphalt shingles on felt on 7/16" OSB, each slope's laid on its top chords; each
//   weighs what its layers do, a dead load on the trusses along its slope. The gable, laid out by its chords' undersides
//   (as `truss` lays one out) where the commons are by their axes, stands proud of them by half a chord's depth less its
//   cosine's share, 3.45: each covering rests on the first common and on the gable, clear of the commons between.

import { part, bears, node, free, plane, support, frame, areaLoad, cases, designBasis, must, on, facing, layers, weights, lumber, nominal, DFL, add, div, sub, ft, inch, mm, psf, pitch, as, type Length } from "topo-part";
import { fink, headerOnPosts, trussRow, row, covering } from "topo-part/framing";
import { truss } from "topo-part/trusses";
import { wall } from "topo-part/walls";

const width = ft(12), depth = ft(16), wallHeight = mm(2439);
const twoBySix = lumber(nominal["2x6"], DFL.No2), twoByFour = lumber(nominal["2x4"], DFL.No2);
const beam = lumber(nominal["2x12"], DFL.No2, { plies: 3 }), post = lumber(nominal["2x6"], DFL.No2, { plies: 2 });
// (Across the front: the header's thickness, its plies side by side; the walls stand behind it.)
const beamWidth = mm(3 * 38.1);
const k = pitch(5);

export default part("trusses on a header", (self) => {
  // (Its outside face on y = depth, walked east to west so its outside is north.)
  const back = wall("back wall", { from: node(width, depth, 0), to: node(0, depth, 0), height: wallHeight, spacing: inch(16), stock: twoBySix });
  // The header across the front: its frame's x along it, y up, z across (out of the front).
  const ends = [node(0, 0, 0), node(width, 0, 0), node(0, 0, mm(1000))];
  const front = headerOnPosts("front header", { length: width, top: sub(wallHeight, mm(10)), beam, post, at: frame(ends[0], { x: ends[1], y: ends[2] }) });
  // The trusses: from the header's front face to the back wall's outside, every 24" along the width.
  const heel = [node(0, mm(-3 * 38.1 / 2), 0), node(width, mm(-3 * 38.1 / 2), 0), node(0, depth, 0)];
  const span = add(depth, div(beamWidth, 2));
  const stations = row(width, inch(24), twoByFour.thickness);
  const roof = trussRow("trusses", {
    stations, height: wallHeight,
    shift: { 2: mm(19) },
    make: (n, at) => n === stations.length - 1
      // (The gable: studs from its bottom chord to its top chords, at its quarters and its middle.)
      ? truss("gable", {
          points: { a: [0, 0], q1: [div(span, 4), 0], m: [div(span, 2), 0], q3: [div(span, 4 / 3), 0], b: [span, 0], t1: [div(span, 4), div(span, 4 / k)], peak: [div(span, 2), div(span, 2 / k)], t3: [div(span, 4 / 3), div(span, 4 / k)] },
          top: [["a", "t1", "peak"], ["peak", "t3", "b"]], bottom: ["a", "q1", "m", "q3", "b"], webs: [["q1", "t1"], ["m", "peak"], ["q3", "t3"]],
          topChord: twoByFour, bottomChord: twoByFour, web: twoByFour, at,
        })
      : fink("truss", { span, pitch: k, overhang: inch(12), stock: twoByFour, at }),
    // (Each heel on what bears it: the front on the header's top at its front edge; the back on the wall's top plate at
    // its inside edge.)
    bears: () => [{ on: front.on, edge: front.front }, { on: back.topPlate.faces["-y"], edge: back.faces.inside }],
    at: frame(heel[0], { x: heel[1], y: heel[2] }),
  });
  // The coverings: each slope's from its eaves (the top chords' ends, 12" past the heels on plan) to the peak, the length
  // of the row; its underside on the top faces of the first truss's chord and the gable's, each node found on that face
  // where it is on plan.
  const build = layers(weights.asphaltShingles, weights.roofingFelt, weights.osb7_16);
  const [y0, peak, y1] = [-3 * 38.1 / 2 - 304.8, -3 * 38.1 / 2 + as(span, mm) / 2, -3 * 38.1 / 2 + as(span, mm) + 304.8];
  type Chord = Parameters<typeof facing>[0];
  const first = roof.trusses[0] as unknown as Record<"left" | "right", Chord>, gable = roof.trusses[stations.length - 1] as unknown as { top: Chord[] };
  const slope = (name: string, side: 0 | 1, from: number, to: number) => {
    const c = covering(name, { outline: [[0, mm(from)], [width, mm(from)], [width, mm(to)], [0, mm(to)]], under: [[0, mm(from), free(wallHeight)], [width, mm(from), free(wallHeight)], [0, mm(to), free(wallHeight)]], build, on: roof });
    const [onFirst, onGable] = [facing(first[side === 0 ? "left" : "right"], self.z), facing(gable.top[side], self.z)];
    return { covering: c, weight: c.weight!, rests: { [`${name} on the first truss`]: must(on(c.under[0], onFirst)), [`${name} on the gable`]: must(on(c.under[1], onGable)), [`${name} up the first truss`]: must(on(c.under[2], onFirst)) } };
  };
  const coverings = { front: slope("front slope", 0, y0, peak), back: slope("back slope", 1, peak, y1) };
  const ground = [node(0, 0, 0), node(mm(1), 0, 0), node(0, mm(1), 0)];
  return {
    back, ends, front, heel, roof, coverings, ground,
    // (Its load path: the trusses on the wall at the back and the header on posts at the front.)
    rests: [bears(roof, back), bears(roof, front)],
    slab: support(plane(ground[0], ground[1], ground[2])),
    // (Built to a fifth of a millimetre: each truss is square in itself, set sloping, so the plumb face of its heel leans
    // a tenth of a millimetre into the header at its foot.)
    design: designBasis({ tolerance: mm(0.2) }),
    snow: areaLoad(roof, psf(25), cases.snow),
  };
});
