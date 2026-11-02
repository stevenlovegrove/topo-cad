// A four-bar linkage, put together by lengths and angles: a kit whose pieces close a loop.
//
// Four aluminium bars, each made to its length and drawn on its own: a ground bar, fixed; a crank, a coupler and a
// rocker, each free to move in the plane and turn in it (a planar joint: its place in the plane and its turn are
// its own numbers, started roughly where it goes). Each is joined to the next by a hinge at their ends: the pins one
// point, their axes one way. A tree of parts cannot close a loop (each part has one parent), so the hinges are
// constraints between parts, not their joints. The crank's hinge on the ground is set at 60 degrees: that, and the
// lengths, are all that place the coupler and the rocker.
//
// The bars are stacked so that none passes through another: the ground at the bottom, the crank and the rocker on
// it, the coupler on them; each pin is where the one bar's top meets the next's underside.

import { part, node, free, placed, joints, polygon, extrusion, hinge, hold, materials, div, mul, mm, deg, type Length } from "topo-part";

const width = mm(12), thick = mm(5);

/** A bar `length` long along its x, `width` wide, `thick` thick from its underside: a pin at each end on its
 *  underside (`a0`, `b0`) and on its top (`a1`, `b1`). */
function bar(name: string, length: Length, o: { at: ReturnType<typeof placed> }) {
  const h = div(width, 2), nh = mul(-1, h);
  const corners = [node(0, nh, 0), node(length, nh, 0), node(length, h, 0), node(0, h, 0)];
  return part(name, { corners, a0: node(0, 0, 0), a1: node(0, 0, thick), b0: node(length, 0, 0), b1: node(length, 0, thick) }, {
    at: o.at,
    shapes: [extrusion(polygon(...corners), { to: thick, material: materials.aluminium6061T6 })],
  });
}

/** In the plane, from where it is drawn: at (u, v), turned so many degrees, each free from there. */
const inPlane = (u: Length, v: Length, angle: number, z: Length) => placed({ origin: [0, 0, z], joint: joints.planar([0, 0, 1], { u: free(u), v: free(v), angle: free(deg(angle)) }) });

export default part("four-bar", () => {
  const ground = bar("ground", mm(200), { at: placed() });
  const crank = bar("crank", mm(60), { at: inPlane(mm(0), mm(0), 50, thick) });
  const coupler = bar("coupler", mm(180), { at: inPlane(mm(30), mm(50), 20, mul(2, thick)) });
  const rocker = bar("rocker", mm(120), { at: inPlane(mm(200), mm(0), 80, thick) });
  return {
    ground,
    crank,
    coupler,
    rocker,
    // (Each hinge: the pins one, the axes, each bar's z, one way.)
    drive: hinge(ground.a1, crank.a0, { axis: [ground.z, crank.z], angle: { from: ground.b1, to: crank.b0, turn: deg(60) } }),
    elbow: hinge(crank.b1, coupler.a0, { axis: [crank.z, coupler.z] }),
    wrist: hinge(coupler.b0, rocker.b1, { axis: [coupler.z, rocker.z] }),
    foot: hinge(rocker.a0, ground.b1, { axis: [rocker.z, ground.z] }),
    // (The ground bar is mounted: held at both its pins.)
    mounted: [hold(ground.a0, "fixed"), hold(ground.b0, "fixed")],
  };
});
