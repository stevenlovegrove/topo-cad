// Placed by its nodes: a free assembly, put where relations between nodes say.
//
// Two posts are fixed where they stand. A beam is drawn in its own
// coordinates, somewhere else entirely and turned, and made free: its
// placement is only where the solve starts. Two relations place it: each end
// of the beam is one node with the top of a post (coincide). What those leave
// free (rolling about its length) the beam's own hint settles: it is upright.
//
// Each coincidence is one node of the model: the beam and the post share it,
// as a joint. Nothing here is joined because of where it happens to be.
//
// In the 3D view, turn on Skeleton: the square is the beam (free, its six
// freedoms settled by the two coincidences and the hint), the filled circles
// are the nodes of the two assemblies, and the rings are the coincidences.
import { Assembly, Building, coincide, DFL, ft } from "topo-cad";

const grade = DFL.No2;
const posts = Assembly.named("Posts")
  .point("a", [0, 0, 0]).point("a top", [0, 0, ft(7)])
  .point("b", [ft(10), 0, 0]).point("b top", [ft(10), 0, ft(7)])
  .member("post", ["a", "a top"], { size: [4, 4], grade })
  .member("post", ["b", "b top"], { size: [4, 4], grade })
  .supportedAt("a", "b");

// The beam, in its own coordinates (its length along x, its underside on its axis), put
// anywhere to start with: 3 m away, turned 40 degrees.
const beam = Assembly.named("Beam")
  .point("west end", [0, 0, 0]).point("east end", [ft(10), 0, 0])
  // (It outranks the posts, so it runs over their tops, bearing on them, rather than butting into their sides.)
  .member("beam", ["west end", "east end"], { size: [4, 8], grade, anchor: [0, -0.5], priority: 10 })
  .rotated(40)
  .moved([3, 2, 1])
  .free();

export default Building.named("Placed by nodes").add(
  posts,
  beam,
  coincide(beam.node("west end"), posts.node("a top")),
  coincide(beam.node("east end"), posts.node("b top")),
);
