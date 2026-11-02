// A kids' trike of an Infento 1.0 kit (examples/kids-trike.ts, on the part core): 40 x 40 slotted profile cut to
// length, square joints by angle brackets or a screw into the core bore (choose: Scenario), and toothed pivots at an
// angle. Not a building: millimetres, a 3D frame analysis, and load cases that are things a child does to it.
//
// No one works out where anything is. The pieces are cut to their lengths and the pivots set the angles between them;
// the wheels stand on the ground. The solve places the rest: the rail comes out a little nose-down, because a 480 beam at
// 135° and a column at 75° put the front axle where they put it. Change a length or an angle and the rest follows.
// Each piece starts near where it ends up (`at`), so that the solve begins close; where it stands on the ground is
// said (the rear axle's middle over the origin); which way it faces, where nothing says, is as it started.

import { part, mm, kg, N, deg, placed, join, joinery, lands, hold, load, situation, designBasis, scenarios, must, offset } from "topo-part";
import { profile, angleBracket, endFastener, pivot, axleClamp } from "topo-part/infento";
import { wheel, standsOn, onEnd, nutted } from "topo-part/wheels";
import { member, sections, materials, joints } from "topo-part";

// ----- what there is: lengths and angles (x forward, y left, z up) ---------------------------------------------------
const rearTrack = mm(400);
const railLength = mm(300); // from the rear beam to the knee
const mainBeam = mm(480); // from the knee to the head
const seatAlong = mm(140); // where the seat post stands on the main beam, from the knee
const columnLength = mm(480); // the steering column: the front wheel at its foot, the handlebar on its top
const headAlong = mm(360); // where the main beam meets it, from its foot
const seatPostLength = mm(200);
const seatWidth = mm(240);
const handleWidth = mm(360);

// How what meets square is joined.
const squareJoint = scenarios({
  "Angle brackets, two per joint": angleBracket({ count: 2 }),
  "Screw into the core bore": endFastener(),
}, { describe: { "Angle brackets, two per joint": "Die-cast brackets either side, rated 2 x (50 N·m, 1000 N)", "Screw into the core bore": "No capacity is published for this fastener: those joints are not checked" } });

// Situations: what a child does to it.
const seated = situation("Child seated"), kerb = situation("Off a kerb, 2.5 g"), heave = situation("Heaving on the handles");
const child = 245; // N: 25 kg

const start = (origin: [number, number, number], x: [number, number, number], y: [number, number, number]) => placed({ origin: [mm(origin[0]), mm(origin[1]), mm(origin[2])], x, y, joint: joints.free() });

export default part("kids' trike", (trike) => {
  const across = trike.y;
  const rearBeam = profile("rear beam", rearTrack, { at: start([0, 200, 100], [0, -1, 0], [0, 0, 1]) });
  const rail = profile("rail", railLength, { at: start([0, 0, 100], [1, 0, 0], [0, 1, 0]) });
  const main = profile("main beam", mainBeam, { at: start([300, 0, 100], [0.707, 0, 0.707], [0, 1, 0]) });
  const column = profile("column", columnLength, { at: start([819, 0, 127], [-0.5, 0, 0.866], [0, 1, 0]) });
  const handlebar = profile("handlebar", handleWidth, { at: start([579, 180, 543], [0, -1, 0], [0.866, 0, 0.5]) });
  const seatPost = profile("seat post", seatPostLength, { at: start([399, 0, 199], [0, 0, 1], [0, 1, 0]) });
  const seatRail = profile("seat rail", seatWidth, { at: start([399, 120, 399], [0, -1, 0], [1, 0, 0]) });
  const frontAxle = member("front axle", sections.rod(mm(12)), materials.chromoly4130N, { length: mm(60), at: start([819, 0, 127], [0, 1, 0], [1, 0, 0]) });
  const wheelOf = (name: string, at: [number, number, number]) => wheel(name, { diameter: mm(200), width: mm(36), bore: mm(30), mass: kg(0.45), at: start(at, [1, 0, 0], [0, 0, 1]) });
  const wheels = { left: wheelOf("left wheel", [0, 225, 100]), right: wheelOf("right wheel", [0, -225, 100]), front: wheelOf("front wheel", [819, 60, 100]) };

  return {
    rearBeam, rail, main, column, handlebar, seatPost, seatRail, frontAxle, wheels,
    joints: {
      // Square, by the rule (below): the rail on the rear beam's middle; the column's top on the handlebar's middle;
      // the seat post's top on the seat rail's middle.
      tail: join(rail.start, rearBeam.middle.side()),
      handle: join(column.end, handlebar.middle.side()),
      seat: join(seatPost.end, seatRail.middle.side()),
      // The knee: two ends, a cap on each; the main beam turned up 135° from the rail's way back.
      knee: join(rail.end, main.start, pivot({ angle: deg(135), from: rail.start, to: main.end, about: across })),
      // The head: the column passes; one cap on the beam's end, the other fixed into the column's slot. The beam's way
      // back is the column's way down turned 75°.
      head: join(main.end, column.along(headAlong).side(), pivot({ angle: deg(75), from: column.start, to: main.start, about: across })),
      // The seat post's foot on the main beam: its way up is the beam's way ahead turned back 45°.
      seatFoot: join(seatPost.start, main.along(seatAlong).side(), pivot({ angle: deg(45), from: seatPost.end, to: main.end, about: across })),
      // The front axle clamped into the column's foot; the wheels on their axles, 25 out past the rear beam's ends.
      frontAxle: join(frontAxle.start, column.start, axleClamp()),
      left: join(wheels.left.hub, rearBeam.start, onEnd({ out: mm(25) })),
      right: join(wheels.right.hub, rearBeam.end, onEnd({ out: mm(25) })),
      front: join(wheels.front.hub, frontAxle.end, nutted()),
    },
    says: {
      ...standsOn(trike, wheels.left, wheels.right, wheels.front),
      // Where on the ground: the rear axle's middle (the rail's start) over the origin.
      x: must(offset(trike.origin, rail.start.node, trike.x, mm(0))),
      y: must(offset(trike.origin, rail.start.node, trike.y, mm(0))),
    },
    // The structure: held at the rear beam's ends and the column's foot, as its wheels hold it (free to roll, held
    // sideways at the left).
    holds: [hold(rearBeam.start.node, "pinned"), hold(rearBeam.end.node, { x: true, z: true }), hold(column.start.node, { y: true, z: true })],
    loads: [
      load(handlebar.start.node, [0, 0, N(-0.05 * child)], seated), load(handlebar.end.node, [0, 0, N(-0.05 * child)], seated),
      load(seatRail.start.node, [0, 0, N(-0.45 * child)], seated), load(seatRail.end.node, [0, 0, N(-0.45 * child)], seated),
      load(handlebar.start.node, [0, 0, N(-0.125 * child)], kerb), load(handlebar.end.node, [0, 0, N(-0.125 * child)], kerb),
      load(seatRail.start.node, [0, 0, N(-1.125 * child)], kerb), load(seatRail.end.node, [0, 0, N(-1.125 * child)], kerb),
      load(handlebar.start.node, [N(-100), 0, 0], heave), load(handlebar.end.node, [N(-100), 0, 0], heave),
      load(seatRail.start.node, [0, 0, N(-0.5 * child)], heave), load(seatRail.end.node, [0, 0, N(-0.5 * child)], heave),
    ],
    // Elastic stress against yield with a factor of safety of 2; displacement limited to 5 mm.
    design: designBasis({ analysis: "frame", combinations: "each-case", safetyFactor: 2, deflectionLimit: mm(5) }),
  };
}, {
  // Profile on profile, wherever nothing else is said: square, as chosen.
  joinery: joinery(lands(profile, profile, squareJoint)),
});
