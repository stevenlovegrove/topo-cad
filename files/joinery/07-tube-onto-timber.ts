// The shape of a joint does not ask what the members are (examples/joinery/07-tube-onto-timber.ts), written on the part
// core.
//
// Tube and timber rails between a timber post and a steel tube post:
//
// - a tube onto the timber post: its end on the post's face, as anything's is onto a flat face;
// - a tube onto the tube post: coped;
// - a timber onto the tube post: its end on the face of the tube that looks toward it, touching it along a line (a
//   timber scribed to a round is not made yet: fits are of round members);
// - a timber onto the timber post: on its face;
// - a tube in the timber post: `housed`, its end 40 into the post in a round socket bored to its outside;
// - a peg through the timber post: `housed`, of a member that passes, in a hole right through.
//
// What does ask is the fastening, given per joint here (`fastened(how, by)`): a bolt through the timber, welds
// between the tubes, glue in the socket, the hole and on the tube, toe nails between the timbers. Each asks something of what it
// joins, and is refused where it does not get it: weld the tube rail to the timber post (`weld()` at `high1`) and the
// model does not build, saying a weld joins metals.

import { part, node, member, sections, nominal, DFL, materials, join, fastened, hold, designBasis, neg, mm, ft, inch } from "topo-part";
import { cope, housed } from "topo-part/tubes";
import { endOnFace } from "topo-part/framing";
import { weld, glued, nails, bolt } from "topo-part/fastenings";

const grade = DFL.No2, steel = materials.chromoly4130N;
const rail = sections.tube(mm(42.4), mm(2.6));
const z = { peg: ft(0.5), low: ft(1.25), mid: ft(2), high: ft(2.75), top: ft(3.25) };
const across = ft(4);
const timber = (n: keyof typeof nominal) => sections.lumber(nominal[n].thickness, nominal[n].depth);

export default part("tube and timber", (f) => {
  const feet = [node(0, 0, 0), node(across, 0, 0)];
  // (The timber post laid back from its face toward the rails, which land on that face; the tube post on its axis.)
  const timberPost = member("timber post", timber("4x4"), grade, { from: feet[0], to: node(0, 0, z.top), y: neg(f.x), role: "post" });
  const tubePost = member("tube post", sections.tube(mm(60.3), mm(3.2)), steel, { from: feet[1], to: node(across, 0, z.top), y: f.y, role: "post" });
  const between = (h: typeof z.low, name: string, section: ReturnType<typeof timber> | typeof rail, material: typeof steel) =>
    member(name, section, material, { from: node(0, 0, h), to: node(across, 0, h), y: f.y, role: "rail" });
  const tubeRail = between(z.high, "tube rail", rail, steel);
  const socketedRail = between(z.mid, "socketed rail", rail, steel);
  const timberRail = between(z.low, "timber rail", timber("2x4"), grade);
  const peg = member("peg", sections.tube(mm(26.9), mm(2.3)), steel, { from: node(inch(-6), 0, z.peg), to: node(inch(6), 0, z.peg), y: f.y });
  return {
    feet, timberPost, tubePost, tubeRail, socketedRail, timberRail, peg,
    joints: {
      high1: join(tubeRail.start, timberPost.along(z.high), fastened(endOnFace(), bolt(mm(10), { length: mm(130) }))),
      high2: join(tubeRail.end, tubePost.along(z.high), fastened(cope(), weld())),
      mid1: join(socketedRail.start, timberPost.along(z.mid), fastened(housed({ depth: mm(40) }), glued())),
      mid2: join(socketedRail.end, tubePost.along(z.mid), fastened(cope(), weld())),
      low1: join(timberRail.start, timberPost.along(z.low), fastened(endOnFace(), nails(2, 8, "toe"))),
      // (The original bolts it through the tube: a hole through a round wall is not made yet, and `bolt` says so.)
      low2: join(timberRail.end, tubePost.along(z.low), fastened(endOnFace(), glued())),
      peg: join(peg.middle, timberPost.along(z.peg), fastened(housed(), glued())),
    },
    holds: [hold(feet[0], "fixed"), hold(feet[1], "fixed")],
    // (Posts held at their feet: a frame, not a takedown.)
    design: designBasis({ analysis: "frame" }),
  };
});
