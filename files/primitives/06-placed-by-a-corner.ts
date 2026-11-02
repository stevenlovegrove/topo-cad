// A building placed by a corner (examples/primitives/06-placed-by-a-corner.ts), written on the part core: no
// coordinates but a datum.
//
// The building (walls 16' by 12', 8' tall, of 2x4 at 16"; Fink trusses across the 12' every 24", 6 in 12, 12" overhangs)
// is drawn at its own origin, on a free joint: that is only where the settle starts. One relation places it: the top of
// its front wall's outside corner is a surveyed mark, a point of the world (5 m, 3 m, 8'). What that leaves free
// (turning about the corner, tilting) its hints settle: upright, and turned as drawn. Everything in it moves with it:
// its trusses sit on its plates where they now are.
//
// A post, drawn anywhere on a free joint of its own, stands in front of the front wall: its top made one with the point
// of the front wall's top plate 1.5 m along it (between two trusses' tails).

import { part, node, free, plane, support, placed, joints, frame, span, must, hinted, parallel, coincide, lumber, nominal, DFL, add, div, sub, inch, ft, m, mm } from "topo-part";
import { perimeter } from "topo-part/walls";
import { fink, trussRow, row } from "topo-part/framing";

const stud = lumber(nominal["2x4"], DFL.No2), post4 = lumber(nominal["4x4"], DFL.No2);
const [wide, deep, height] = [ft(16), ft(12), ft(8)];

export default part("placed by a corner", (s) => {
  const building = part("building", () => {
    const walls = perimeter("walls", { corners: [node(0, 0, 0), node(wide, 0, 0), node(wide, deep, 0), node(0, deep, 0)], legs: { height, spacing: inch(16), stock: stud } });
    // (Its trusses along the 16', spanning the 12' from the front wall's outside, their axes half a depth over the plates.)
    const z = add(height, div(stud.depth, 2));
    const roof = trussRow("roof", {
      stations: row(wide, inch(24), stud.thickness), height: mm(0),
      make: (k, at) => fink(`truss ${k + 1}`, { span: deep, pitch: 6 / 12, overhang: inch(12), stock: stud, at }),
      at: frame(node(0, 0, z), { x: node(mm(1000), 0, z), y: node(0, mm(1000), z) }),
    });
    // (The top of its front wall's outside corner; and the point of the front wall's top plate 1.5 m along it, on its
    // top, in its middle.)
    const corner = node(0, 0, height), plate = node(m(1.5), div(stud.depth, 2), height);
    return { walls, roof, corner, plate };
  }, { at: placed({ joint: joints.free({ x: free(mm(0)), y: free(mm(0)), z: free(mm(0)) }) }) });
  // The surveyed mark.
  const mark = node(m(5), m(3), height);
  // The post: drawn 3 m along and 2 m in front of the origin, on a free joint, its foot to its top 8' up; its face
  // toward the wall on its line, so that it stands in front of the wall, not astride it.
  const post = part("post", (p) => {
    const foot = node(0, sub(mm(0), post4.thickness), 0), top = node(0, sub(mm(0), post4.thickness), height);
    return { foot, top, post: span(foot, top, p.y, post4) };
  }, { at: placed({ origin: [m(3), m(-2), 0], joint: joints.free({ x: free(mm(0)), y: free(mm(0)), z: free(mm(0)) }) }) });
  const ground = [node(0, 0, 0), node(mm(1), 0, 0), node(0, mm(1), 0)];
  return {
    building, mark, post, ground,
    slab: support(plane(ground[0], ground[1], ground[2])),
    says: {
      "the corner on the mark": must(coincide(building.corner, mark)),
      "the post's top at the plate": must(coincide(post.top, building.plate)),
      // (Upright and turned as drawn, each of them, where nothing else says.)
      "the building upright": hinted(parallel(building.z, s.z)), "the building turned as drawn": hinted(parallel(building.x, s.x)),
      "the post upright": hinted(parallel(post.z, s.z)), "the post turned as drawn": hinted(parallel(post.x, s.x)),
    },
  };
});
