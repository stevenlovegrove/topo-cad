// A lap (examples/joinery/04-lap.ts), written on the part core: where two members overlap, each keeps a part of it.
//
// Three laps in one frame of 2x4s on edge, all at one height, along a rail:
//
// - a cross lap, where two pass through each other: each is let in by half its depth;
// - a tee lap, where one ends on one that passes: the one that ends runs on over the other, to its far face;
// - a corner lap, where both end: each runs on over the other, to its far face.
//
// The one that lands keeps the upper part, `share` of the depth (`lap`, across the depth: half, unless said); the
// rail the lower. The corner gives its lander two thirds. Each lap nailed, two 8d through the face. The rail's line a
// foot from the frame's near side; the others cross it a foot apart, from that side; each held where it starts.

import { part, node, span, lap, endOn, flip, neg, nailed, hold, roles, lumber, nominal, DFL, add, sub, div, ft, type Length0 } from "topo-part";

const twoByFour = lumber(nominal["2x4"], DFL.No2);
const [half, up] = [div(twoByFour.thickness, 2), div(twoByFour.depth, 2)];

export default part("lapped frame", (f) => {
  // (Each on edge: its line its middle, half its depth up from the floor; laid from it across by its thickness, so its
  // line is half that back from its centre, and its depth up.)
  const at = (x: Length0, y: Length0) => node(x, y, up);
  // (The rail along x, laid toward +y; the others along y, laid toward −x.)
  const starts = { rail: at(0, sub(ft(1), half)), cross: at(add(ft(1), half), 0), tee: at(add(ft(2), half), 0), corner: at(add(ft(3), half), 0) };
  const ends = { rail: at(ft(3), sub(ft(1), half)), cross: at(add(ft(1), half), ft(2)), tee: at(add(ft(2), half), ft(1)), corner: at(add(ft(3), half), ft(1)) };
  const along = (k: keyof typeof starts) => (k === "rail" ? f.y : f.x);
  const piece = (k: keyof typeof starts) => span(starts[k], ends[k], k === "rail" ? along(k) : neg(along(k)), twoByFour, { role: roles.rail });
  const [rail, crosser, stub, leg] = [piece("rail"), piece("cross"), piece("tee"), piece("corner")];
  const depth = { across: "depth" as const };
  return {
    starts, ends, rail, crosser, stub, leg,
    laps: { cross: lap(rail, crosser, depth), tee: lap(rail, stub, depth), corner: lap(rail, leg, { ...depth, share: 2 / 3 }) },
    // (Those that end run on to the other's far face: the rail's away from the frame's near side, its +y; the leg's
    // away from the rail, its −y.)
    runOn: { stub: endOn(stub, flip(rail.faces["+y"])), leg: endOn(leg, flip(rail.faces["+y"])), rail: endOn(rail, flip(leg.faces["-y"])) },
    nails: [crosser, stub, leg].map((p) => nailed(p, rail, { count: 2, penny: 8, method: "face" })),
    holds: Object.values(starts).map((s) => hold(s, "fixed")),
  };
});
