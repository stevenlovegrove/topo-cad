// A road bike, its chart as readings and its frame in one of three systems of joinery (docs/joinery.md, the sketch
// docs/sketches/road-bike.ts made real). As examples/bike.ts lays it out, but:
//
// - The chart's figures are published rounded, so each is a reading (`measured`, to within half a millimetre or a
//   twentieth of a degree), and both wheels on the ground is what must hold: the solve says which figure gave way,
//   and by how much.
// - What the frame is made of and how its tubes meet is one choice (a scenario): steel brazed into lugs, each lug
//   chosen by the angle its tubes meet at (and said where none of the set bends that far); aluminium coped and TIG
//   welded; or carbon moulded in one piece. Each fastening asks what it joins: a weld or a braze joins metals, what is
//   moulded is laid up of a laminate.
// - The fork, steerer, stem and bar turn together in the headset, within its lock either way; the seatpost is inserted
//   in the seat tube at least 65 deep; the stays are plugged into socketed dropouts, each fastened as its system fastens.

import { part, bears, node, free, measured, fallback, frame, joints, member, sections, materials, joinery, lands, join, meet, fastened, socket, must, on, line as lineThrough, offset, distance, hold, designBasis, scenarios, add, sub, div, mul, neg, mm, deg, type Basis, type Geo, type Material, type Node, type Num, type Rules, type Section } from "topo-part";
import { cope, clamp, lugged, headset, inserted, housed, dropout, type LugSet } from "topo-part/tubes";
import { weld, braze, moulded, glued } from "topo-part/fastenings";

const { tube, rod } = sections;
// (The rear axle and the cockpit are the same in each.)
const axleSteel = materials.columbusOmnicrom, cockpit = materials.aluminium6061T6;

/** A set of lugs for this frame, as its builder lays it out (not a maker's catalogue): each for the angle between the
 *  tubes it joins, the lesser way round, bent up to 2° either way to fit; each sleeve 40 along its tube, 1.2 thick. */
const lugs: LugSet = {
  name: "this frame's set",
  bend: deg(2), length: mm(40), wall: mm(1.2),
  lugs: [
    { name: "head lug (top)", angle: deg(80) },
    { name: "head lug (down)", angle: deg(60) },
    { name: "seat lug", angle: deg(80) },
    { name: "seatstay lug", angle: deg(50) },
    { name: "shell socket", angle: deg(90) },
    { name: "chainstay socket", angle: deg(81) },
    { name: "fork crown socket", angle: deg(90) },
  ],
};

/** What the frame is made of, its tubes, and how they meet. */
interface System { material: Material; dropouts: Material; joinery: Rules; head: Section<"tube">; top: Section<"tube">; down: Section<"tube">; seat: Section<"tube">; shell: Section<"tube">; chainstay: Section<"tube">; seatstay: Section<"tube">; blade: Section<"tube">; post: Section<"tube"> }
// (Each stay plugged into its socket in its dropout, fastened as the system fastens.)
const plugged = (how: Parameters<typeof fastened>[1]) => lands(tube, socket, fastened(housed(), how));
// (The front axle, a stand-in for the fork's dropouts and the wheel's, coped to the blades in each.)
const axle = lands(rod, tube, cope());
const system = scenarios<System>({
  "Steel, brazed into lugs": {
    material: materials.columbusCromor, dropouts: materials.columbusCromor,
    joinery: joinery(lands(tube, tube, fastened(lugged(lugs), braze("brass"))), plugged(braze("silver")), axle),
    head: tube(mm(31.7), mm(1.0)), top: tube(mm(25.4), mm(0.8)), down: tube(mm(28.6), mm(0.8)), seat: tube(mm(28.6), mm(0.8)),
    shell: tube(mm(38), mm(2.0)), chainstay: tube(mm(22.2), mm(0.7)), seatstay: tube(mm(14), mm(0.7)), blade: tube(mm(24), mm(0.9)), post: tube(mm(27.0), mm(2.2)),
  },
  "Aluminium 6061-T6, TIG welded": {
    material: materials.aluminium6061T6, dropouts: materials.aluminium6061T6,
    joinery: joinery(lands(tube, tube, fastened(cope({ gap: mm(0.2) }), weld("TIG"))), plugged(weld("TIG")), axle),
    head: tube(mm(44), mm(2.0)), top: tube(mm(31.8), mm(1.5)), down: tube(mm(42), mm(1.6)), seat: tube(mm(31.8), mm(1.6)),
    shell: tube(mm(46), mm(3.0)), chainstay: tube(mm(22.2), mm(1.8)), seatstay: tube(mm(19), mm(1.4)), blade: tube(mm(28), mm(1.8)), post: tube(mm(28.6), mm(2.2)),
  },
  "Carbon, moulded in one piece": {
    material: materials.carbonFabric0_90, dropouts: materials.aluminium6061T6,
    joinery: joinery(lands(tube, tube, fastened(cope(), moulded())), plugged(glued()), axle),
    head: tube(mm(48), mm(2.0)), top: tube(mm(32), mm(1.2)), down: tube(mm(46), mm(1.4)), seat: tube(mm(34), mm(1.4)),
    shell: tube(mm(55), mm(3.0)), chainstay: tube(mm(24), mm(1.5)), seatstay: tube(mm(18), mm(1.2)), blade: tube(mm(28), mm(1.6)), post: tube(mm(31.2), mm(2.2)),
  },
}, { describe: { "Steel, brazed into lugs": "Columbus Cromor tubes, each meeting in a lug of the set, brass brazed", "Aluminium 6061-T6, TIG welded": "Oversized tubes, coped with 0.2 of fit-up, TIG welded", "Carbon, moulded in one piece": "Carbon fabric tubes, laid up together" } });

const wheelRadius = mm(336), rearHub = mm(130), frontHub = mm(100), shellWidth = mm(68);
// (The fork's crown and the cockpit's tubes: the same in each.)
const crown = tube(mm(28.6), mm(2.0)), steererTube = tube(mm(28.6), mm(1.55)), stemTube = tube(mm(31.8), mm(2.0)), barTube = tube(mm(23.8), mm(1.6));

export default part("road bike", (bike) => {
  const ahead = bike.x, back = neg(bike.x), across = bike.y;
  const lengthWithin = mm(0.5), angleWithin = deg(0.05);
  // The chart (Ritchey Road Logic, size 55): readings, each to within what it is rounded to.
  const chart = {
    stack: measured(mm(568), lengthWithin), reach: measured(mm(395), lengthWithin), headAngle: measured(deg(73.5), angleWithin), seatAngle: measured(deg(73.5), angleWithin),
    headTube: measured(mm(160), lengthWithin), seatTube: measured(mm(525), lengthWithin), chainstay: measured(mm(415), lengthWithin), bbDrop: measured(mm(70), lengthWithin),
    forkLength: measured(mm(371), lengthWithin), rake: measured(mm(43), lengthWithin),
  };
  // (The rider's fit, and how far the bar is turned: fallbacks, below the chart.)
  const fit = { spacers: fallback(mm(35)), stem: fallback(mm(100)), bar: fallback(mm(400)), seatpost: fallback(mm(160)), insert: fallback(mm(100)) };
  const turn = fallback(deg(0)), lock = deg(60);
  const left = (w: Num<"length">) => div(w, 2), right = (w: Num<"length">) => mul(-0.5, w);

  /** A line from a point along a way, turned about the bike's across by an angle (ahead turns down, back turns up). */
  const line = <P extends Record<string, Node>>(name: string, from: Geo, way: Basis, angle: Num<"angle">, points: P) =>
    part(name, points, { at: frame(from, { x: way, y: across, joint: joints.hinge([0, 1, 0], angle) }) });

  const rearAxle = node(0, 0, wheelRadius);
  const chainLine = line("chainstay line", rearAxle, ahead, free(deg(10)), {
    bb: node(chart.chainstay, 0, 0), shellL: node(chart.chainstay, left(shellWidth), 0), shellR: node(chart.chainstay, right(shellWidth), 0),
    dropoutL: node(0, left(rearHub), 0), dropoutR: node(0, right(rearHub), 0),
  });
  const seatLine = line("seat tube line", chainLine.bb, back, chart.seatAngle, {
    top: node(chart.seatTube, 0, 0), inside: node(sub(chart.seatTube, fit.insert), 0, 0), saddle: node(add(chart.seatTube, fit.seatpost), 0, 0),
  });
  const headTop = part("head tube top", { top: node(0, 0, 0) }, { at: frame(chainLine.bb, { x: ahead, y: across, joint: joints.moves({ x: chart.reach, y: mm(0), z: chart.stack }) }) });
  const headLine = line("head tube line", headTop.top, ahead, chart.headAngle, { crown: node(chart.headTube, 0, 0) });
  // What turns in the headset: the fork, steerer, stem and bar, about the head tube's axis by `turn`.
  const forkAlong = free(mm(370)), fork = add(chart.headTube, forkAlong), above = mul(-1, fit.spacers);
  const steered = part("steered", {
    crown: node(chart.headTube, 0, 0), crownL: node(chart.headTube, left(frontHub), 0), crownR: node(chart.headTube, right(frontHub), 0),
    frontAxle: node(fork, 0, chart.rake), frontL: node(fork, left(frontHub), chart.rake), frontR: node(fork, right(frontHub), chart.rake),
    stemClamp: node(above, 0, 0), barCentre: node(above, 0, fit.stem), barL: node(above, left(fit.bar), fit.stem), barR: node(above, right(fit.bar), fit.stem),
  }, { at: frame(headTop.top, { x: headLine.x, y: headLine.y, joint: joints.hinge([1, 0, 0], turn) }) });

  const s = system, m = s.material;
  const shell = member("bb shell", s.shell, m, { from: chainLine.shellL, to: chainLine.shellR, y: ahead });
  const bb = shell.middle;
  const headT = member("head tube", s.head, m, { from: headLine.crown, to: headTop.top, y: across });
  const seatT = member("seat tube", s.seat, m, { from: bb, to: seatLine.top, y: across });
  const cluster = seatT.along(mm(25), seatT.end);
  const downT = member("down tube", s.down, m, { from: bb, to: headT.along(mm(35)), y: across });
  const topT = member("top tube", s.top, m, { from: cluster, to: headT.along(mm(30), headT.end), y: across });
  // The dropouts: a block each side round the rear axle, cast with sockets the stays plug into.
  // (Its x along the chainstay's line, toward the bottom bracket; its y up: the chainstay's socket ahead of the axle, the
  // seatstay's above it. The chainstay's is across the block where the chainstay's line from the bottom bracket goes
  // through the axle, as a chainstay is aimed.)
  const drop = (n: Node) => dropout("dropout", { size: mm(56), thickness: mm(36), material: s.dropouts, sockets: { chainstay: [mm(18), 0, free(mm(0))], seatstay: [mm(6), mm(18)] }, at: frame(n, { x: chainLine.x, y: bike.z }) });
  const [dropL, dropR] = [drop(chainLine.dropoutL), drop(chainLine.dropoutR)];
  const chainL = member("chainstay", s.chainstay, m, { from: bb, to: dropL.sockets.chainstay, y: across });
  const chainR = member("chainstay", s.chainstay, m, { from: bb, to: dropR.sockets.chainstay, y: across });
  const stayL = member("seatstay", s.seatstay, m, { from: cluster, to: dropL.sockets.seatstay, y: across });
  const stayR = member("seatstay", s.seatstay, m, { from: cluster, to: dropR.sockets.seatstay, y: across });
  const rear = member("rear axle", rod(mm(10)), axleSteel, { from: chainLine.dropoutL, to: chainLine.dropoutR, y: ahead });
  const forkCrown = member("fork crown", crown, m, { from: steered.crownL, to: steered.crownR, y: ahead });
  const bladeL = member("fork blade", s.blade, m, { from: forkCrown.start, to: steered.frontL, y: across });
  const bladeR = member("fork blade", s.blade, m, { from: forkCrown.end, to: steered.frontR, y: across });
  const front = member("front axle", rod(mm(9)), axleSteel, { from: bladeL.end, to: bladeR.end, y: ahead });
  const steerer = member("steerer", steererTube, m, { from: headTop.top, to: steered.stemClamp, y: across });
  const stem = member("stem", stemTube, cockpit, { from: steered.stemClamp, to: steered.barCentre, y: across });
  const bar = member("handlebar", barTube, cockpit, { from: steered.barL, to: steered.barR, y: ahead });
  const post = member("seatpost", s.post, cockpit, { from: seatLine.inside, to: seatLine.saddle, y: across });

  return {
    chart, fit, turn, rearAxle, forkAlong, chainLine, seatLine, headTop, headLine, steered,
    shell, headT, seatT, downT, topT, dropL, dropR, chainL, chainR, stayL, stayR, rear, forkCrown, bladeL, bladeR, front, steerer, stem, bar, post,
    // (Its load paths where one tube sits in another: the seatpost in the seat tube, the steerer in the head tube.)
    rests: [bears(post, seatT), bears(steerer, headT)],
    says: {
      drop: must(offset(chainLine.bb, rearAxle, bike.z, chart.bbDrop)),
      "left chainstay aimed at the axle": must(on(dropL.sockets.chainstay.node, lineThrough(chainLine.bb, chainLine.dropoutL))),
      "right chainstay aimed at the axle": must(on(dropR.sockets.chainstay.node, lineThrough(chainLine.bb, chainLine.dropoutR))),
      forkLength: must(distance(steered.crown, steered.frontAxle, chart.forkLength)),
      // Both wheels on the ground (the rear axle is put there): the chart gives way to this.
      "front wheel on the ground": must(offset(bike.origin, steered.frontAxle, bike.z, wheelRadius)),
    },
    holds: [hold(chainLine.dropoutL, "pinned"), hold(chainLine.dropoutR, { x: true, z: true }), hold(steered.frontL, { y: true, z: true }), hold(steered.frontR, { z: true })],
    design: designBasis({ analysis: "frame" }),
    joints: {
      // (Each fitted to those before it: the down tube first, the biggest in two of the three, the smaller fitted to it.)
      bb: meet(bb, [downT.start, seatT.start, chainL.start, chainR.start]),
      cluster: meet(cluster, [topT.start, stayL.start, stayR.start]),
      // The rear axle's ends in the dropouts, to their middles.
      rearLeft: join(rear.start, dropL.socket, housed()),
      rearRight: join(rear.end, dropR.socket, housed()),
      // The head tube on the fork's crown; the steerer turning in the head tube, within its lock; the stem clamped on the
      // steerer, the bar in the stem; the seatpost in the seat tube, at least 65 in.
      crown: join(headT.start, forkCrown.middle, clamp()),
      steering: join(steerer.start, headT.end, headset({ turn, lock })),
      stemOnSteerer: join(stem.start, steerer.end, clamp()),
      barInStem: join(stem.end, bar.middle, clamp()),
      seatpost: join(post.start, seatT.end, inserted({ least: mm(65) })),
    },
  };
}, { joinery: system.joinery });
