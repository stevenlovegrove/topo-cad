// A wheel on an axle (examples/volumes/03-a-wheel-on-an-axle.ts), written on the part core: a revolve, a profile
// turned about its axle, kept as what it is.
//
// The profile is in the wheel's x (out from the axle) and z (along it): a bore of 2.9 to be pressed onto a 3 mm rod,
// a tyre 30 across and 8 wide, its shoulders rounded off to 2 (each corner's `round`), and its outer face dished, an
// arc of radius 20 from the rim to the hub, 0.58 deep at its middle. Each run of the profile is a face of the wheel,
// exactly: the tread a cylinder, the bore a cylinder facing in, the rounds tori, the dish a torus too (about a circle
// whose radius is less than its own: it is the middle of that, nearest the axle), the inner face a flat ring. Its edges
// are the circles its corners make (`c0 round`, …). Its volume and its middle are exact (the profile's area, by how
// far its middle goes round).
//
// The wheel turns about its axle (a hinge about its own z): the axle's line is along y, 15 up, so a wheel 30 across
// stands on the ground. How far it has turned is a freedom of the solve, which nothing here says.
//
// (The original's dish bulged out, 0.58 proud of the wheel's face: its arc was turned the other way from what it said.
// This one is dished, as it says.)

import { part, node, placed, joints, revolve, free, materials, mm, deg } from "topo-part";

const [bore, tyre, wide, round, dish] = [2.9, 30, 8, 2, 20];
const rim = 11;
// (The dish's centre: on the outside, level with the middle of the chord from the rim to the hub.)
const half = (rim - bore / 2) / 2;
const centre = [bore / 2 + half, wide + Math.sqrt(dish * dish - half * half)];

export default part("a wheel on an axle", () => {
  const wheel = part("wheel", {
    hubIn: node(mm(bore / 2), 0, 0),
    shoulderIn: node(mm(tyre / 2), 0, 0),
    shoulderOut: node(mm(tyre / 2), 0, mm(wide)),
    rim: node(mm(rim), 0, mm(wide)),
    dishCentre: node(mm(centre[0]), 0, mm(centre[1])),
    hubOut: node(mm(bore / 2), 0, mm(wide)),
  }, {
    // (Its z along the world's y, 15 up: its x the world's, its y down.)
    at: placed({ origin: [0, 0, mm(tyre / 2)], x: [1, 0, 0], y: [0, 0, -1], joint: joints.hinge([0, 0, 1], free(deg(0))) }),
    kind: "solid",
  });
  wheel.shapes = [revolve([
    { node: wheel.hubIn },
    { node: wheel.shoulderIn, round: mm(round) },
    { node: wheel.shoulderOut, round: mm(round) },
    { node: wheel.rim, arc: { centre: wheel.dishCentre, ccw: false } },
    { node: wheel.hubOut },
  ], materials.PETG)];
  return { wheel };
});
