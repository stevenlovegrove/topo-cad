// The bearing point: a node where one member rests on another.
//
// A beam on two posts, and two joists crossing it. `resting` joins each joist
// to the beam at a node where they touch: on the joist's underside and on the
// beam's top. A member's path is the nodes on its axis; so whether that node
// is in a member's path, or on one of its faces, depends only on where the
// member's axis is in its section:
//
// - the first joist has its axis on its underside (anchor [0, -0.5]): the
//   bearing point is on its axis, in its path;
// - the second has its axis through its middle: the bearing point is on its
//   underside face;
// - the beam's axis is on its underside (it sits on the posts), so for both
//   joists the bearing point is on the beam's top face.
//
// In the 3D view, turn on Skeleton: axis nodes are dots, bearing points are
// orange diamonds. Hover a diamond for the face it is on (the box its two
// numbers are free in) and those two numbers, along and across.
import { Assembly, Building, DFL, ft, inch } from "topo-cad";

const grade = DFL.No2;
const post = ft(3);                 // the posts' height: the beam's underside
const top = post + inch(7.25);      // the top of the beam (a 4x8 on edge)

const frame = Assembly.named("Beam on beam")
  .point("p1", [0, 0, 0]).point("b1", [0, 0, post])
  .point("p2", [ft(8), 0, 0]).point("b2", [ft(8), 0, post])
  .member("post", ["p1", "b1"], { size: [4, 4], grade })
  .member("post", ["p2", "b2"], { size: [4, 4], grade })
  .member("beam", ["b1", "b2"], { size: [4, 8], grade, anchor: [0, -0.5], priority: 10 })
  // A joist with its axis on its underside, at the height of the beam's top.
  .point("a1", [ft(2), -ft(3), top]).point("a2", [ft(2), ft(3), top])
  .member("joist", ["a1", "a2"], { size: [2, 8], grade, anchor: [0, -0.5] })
  // A joist with its axis through its middle: half its depth higher.
  .point("c1", [ft(6), -ft(3), top + inch(7.25) / 2]).point("c2", [ft(6), ft(3), top + inch(7.25) / 2])
  .member("joist", ["c1", "c2"], { size: [2, 8], grade })
  .resting("joist#1", "beam")
  .resting("joist#2", "beam")
  .supportedAt("p1", "p2");

export default Building.named("Beam on beam").add(frame);
