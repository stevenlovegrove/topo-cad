// A road bike's frame (examples/bike.ts, on the part core): its geometry chart drawn as lines at the chart's lengths and
// angles, and tubes between their points.
//
// Each figure of the chart is free, held at its figure by a hint (`hinted`), and each of the rider's fit by a fallback,
// which gives way first: change one in the Params panel, or drag a point of the bike, and it moves; anything firmer
// overrides it. The lines start at the rear axle:
// - the chainstay's, ahead to the bottom bracket, turned down as far as the bottom bracket's drop says;
// - the seat tube's, back from the bottom bracket, turned up by the seat angle;
// - the head tube's, from its top (so far ahead of the bottom bracket and above it: reach and stack), ahead and turned
//   down by the head angle, on through the crown to the front axle (beside it by the fork's rake, as far from the crown
//   as the fork is long), and back up it to the stem.
// A line is a frame turned about the bike's across; a point of it is where its lengths put it. The tubes run between
// those points, each landing where it was made from, coped.

import { part, bears, node, free, hinted, fallback, frame, joints, member, sections, materials, joinery, lands, join, meet, must, offset, distance, hold, designBasis, add, div, mul, neg, mm, deg, type Basis, type Geo, type Node, type Num } from "topo-part";
import { cope, clamp, inLine } from "topo-part/tubes";

const { tube, rod } = sections;
const steel = materials.columbusOmnicrom, cockpit = materials.aluminium6061T6;
// Columbus SL tubes (diameter, wall); the cockpit's, aluminium.
const sizes = {
  top: tube(mm(25.4), mm(0.8)), down: tube(mm(28.6), mm(0.8)), seat: tube(mm(28.6), mm(0.8)), head: tube(mm(31.7), mm(1.0)),
  chainstay: tube(mm(22.2), mm(0.7)), seatstay: tube(mm(14), mm(0.7)), blade: tube(mm(24), mm(0.9)), shell: tube(mm(38), mm(2.0)),
  crown: tube(mm(28.6), mm(2.0)), steerer: tube(mm(28.6), mm(1.55)), stem: tube(mm(31.8), mm(2.0)), bar: tube(mm(23.8), mm(1.6)), post: tube(mm(27.2), mm(2.2)),
};
// Wheels and parts.
const wheelRadius = mm(336), rearHub = mm(130), frontHub = mm(100), shellWidth = mm(68);

export default part("road bike", (bike) => {
  const ahead = bike.x, back = neg(bike.x), across = bike.y;
  // The chart (Ritchey Road Logic, size 55), and the rider's fit.
  const chart = {
    stack: hinted(mm(568)), reach: hinted(mm(395)), headAngle: hinted(deg(73.5)), seatAngle: hinted(deg(73.5)),
    headTube: hinted(mm(160)), seatTube: hinted(mm(525)), chainstay: hinted(mm(415)), bbDrop: hinted(mm(70)),
    forkLength: hinted(mm(371)), rake: hinted(mm(43)),
  };
  // (The fit is the rider's: held by fallbacks, below the chart's hints, so a drag of the saddle or the bar moves it
  // first, and the frame only where it cannot.)
  const fit = { spacers: fallback(mm(35)), stem: fallback(mm(100)), bar: fallback(mm(400)), seatpost: fallback(mm(160)) };
  // (Either side of the middle, by half a width.)
  const left = (w: Num<"length">) => div(w, 2), right = (w: Num<"length">) => mul(-0.5, w);

  /** A line from a point along a way, turned about the bike's across by an angle (ahead turns down, back turns up): its
   *  points are along it (x), across (y), and square to it in the bike's plane (z). */
  const line = <P extends Record<string, Node>>(name: string, from: Geo, way: Basis, angle: Num<"angle">, points: P) =>
    part(name, points, { at: frame(from, { x: way, y: across, joint: joints.hinge([0, 1, 0], angle) }) });

  const rearAxle = node(0, 0, wheelRadius);
  // The chainstay's line: its angle is what the bottom bracket's drop makes it (found: a start, nothing more).
  const chainLine = line("chainstay line", rearAxle, ahead, free(deg(10)), {
    bb: node(chart.chainstay, 0, 0), shellL: node(chart.chainstay, left(shellWidth), 0), shellR: node(chart.chainstay, right(shellWidth), 0),
    dropoutL: node(0, left(rearHub), 0), dropoutR: node(0, right(rearHub), 0),
  });
  const seatLine = line("seat tube line", chainLine.bb, back, chart.seatAngle, {
    top: node(chart.seatTube, 0, 0), saddle: node(add(chart.seatTube, fit.seatpost), 0, 0),
  });
  const headTop = part("head tube top", { top: node(0, 0, 0) }, { at: frame(chainLine.bb, { x: ahead, y: across, joint: joints.moves({ x: chart.reach, y: mm(0), z: chart.stack }) }) });
  // The head tube's line, down through the crown to the front axle (found: as far on as the fork's length says); back
  // up it by the spacers to the stem, and the bar so far ahead of that.
  const forkAlong = free(mm(370)), fork = add(chart.headTube, forkAlong), above = mul(-1, fit.spacers);
  const headLine = line("head tube line", headTop.top, ahead, chart.headAngle, {
    crown: node(chart.headTube, 0, 0), crownL: node(chart.headTube, left(frontHub), 0), crownR: node(chart.headTube, right(frontHub), 0),
    frontAxle: node(fork, 0, chart.rake), frontL: node(fork, left(frontHub), chart.rake), frontR: node(fork, right(frontHub), chart.rake),
    stemClamp: node(above, 0, 0), barCentre: node(above, 0, fit.stem), barL: node(above, left(fit.bar), fit.stem), barR: node(above, right(fit.bar), fit.stem),
  });

  // The tubes: between the lines' points, or made from places on the tubes they land on.
  const s = sizes;
  const shell = member("bb shell", s.shell, steel, { from: chainLine.shellL, to: chainLine.shellR, y: ahead });
  const bb = shell.middle;
  const headT = member("head tube", s.head, steel, { from: headLine.crown, to: headTop.top, y: across });
  const seatT = member("seat tube", s.seat, steel, { from: bb, to: seatLine.top, y: across });
  const cluster = seatT.along(mm(25), seatT.end);
  const downT = member("down tube", s.down, steel, { from: bb, to: headT.along(mm(35)), y: across });
  const topT = member("top tube", s.top, steel, { from: cluster, to: headT.along(mm(30), headT.end), y: across });
  const chainL = member("chainstay", s.chainstay, steel, { from: bb, to: chainLine.dropoutL, y: across });
  const chainR = member("chainstay", s.chainstay, steel, { from: bb, to: chainLine.dropoutR, y: across });
  const stayL = member("seatstay", s.seatstay, steel, { from: cluster, to: chainL.end, y: across });
  const stayR = member("seatstay", s.seatstay, steel, { from: cluster, to: chainR.end, y: across });
  const rear = member("rear axle", rod(mm(10)), steel, { from: chainLine.dropoutL, to: chainLine.dropoutR, y: ahead });
  const forkCrown = member("fork crown", s.crown, steel, { from: headLine.crownL, to: headLine.crownR, y: ahead });
  const bladeL = member("fork blade", s.blade, steel, { from: forkCrown.start, to: headLine.frontL, y: across });
  const bladeR = member("fork blade", s.blade, steel, { from: forkCrown.end, to: headLine.frontR, y: across });
  const front = member("front axle", rod(mm(9)), steel, { from: bladeL.end, to: bladeR.end, y: ahead });
  const steerer = member("steerer", s.steerer, steel, { from: headTop.top, to: headLine.stemClamp, y: across });
  const stem = member("stem", s.stem, cockpit, { from: headLine.stemClamp, to: headLine.barCentre, y: across });
  const bar = member("handlebar", s.bar, cockpit, { from: headLine.barL, to: headLine.barR, y: ahead });
  const post = member("seatpost", s.post, cockpit, { from: seatLine.top, to: seatLine.saddle, y: across });

  return {
    chart, fit, rearAxle, forkAlong, chainLine, seatLine, headTop, headLine,
    shell, headT, seatT, downT, topT, chainL, chainR, stayL, stayR, rear, forkCrown, bladeL, bladeR, front, steerer, stem, bar, post,
    // (Its load paths where one tube sits in another: the seatpost in the seat tube, the steerer in the head tube.)
    rests: [bears(post, seatT), bears(steerer, headT)],
    says: {
      // The bottom bracket so far below the rear axle; the front axle as far from the crown as the fork is long.
      drop: must(offset(chainLine.bb, rearAxle, bike.z, chart.bbDrop)),
      forkLength: must(distance(headLine.crown, headLine.frontAxle, chart.forkLength)),
    },
    // Held where its wheels hold it (the rear upright, free to spread sideways on one side; the front free to roll), its
    // own weight on it: a frame.
    holds: [hold(chainLine.dropoutL, "pinned"), hold(chainLine.dropoutR, { x: true, z: true }), hold(headLine.frontL, { y: true, z: true }), hold(headLine.frontR, { z: true })],
    design: designBasis({ analysis: "frame" }),
    joints: {
      // Where several land at one place: each fitted to what passes and to those before it that it meets.
      bb: meet(bb, [seatT.start, downT.start, chainL.start, chainR.start]),
      cluster: meet(cluster, [topT.start, stayL.start, stayR.start]),
      // The rear axle between the dropouts: coped onto each chainstay, and fitted to the seatstay there.
      rearLeft: join(rear.start, chainL.end, cope(), [stayL.end]),
      rearRight: join(rear.end, chainR.end, cope(), [stayR.end]),
      // The head tube on the fork's crown, through its headset; the steerer on from its top; the stem clamped against
      // the steerer, the bar in the stem; the seatpost on from the seat tube.
      headset: join(headT.start, forkCrown.middle, clamp()),
      steering: join(steerer.start, headT.end, inLine()),
      stemOnSteerer: join(stem.start, steerer.end, clamp()),
      barInStem: join(stem.end, bar.middle, clamp()),
      seatpost: join(post.start, seatT.end, inLine()),
    },
  };
}, { joinery: joinery(lands(sections.tube, sections.tube, cope()), lands(sections.rod, sections.tube, cope())) });
