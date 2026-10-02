// A building placed by a corner: no coordinates but a datum.
//
// The walls are drawn at the origin, and made free: that is only where the
// solve starts. One relation places them: the start of the front wall's cap
// plate (a corner, at the top of the walls) is at a surveyed point (datum).
// What that leaves free (turning about the corner, tilting) the walls' own
// hints settle: upright, and turned as drawn. Everything the perimeter's
// generator made moves with it.
//
// The roof is set out from the same corner (`at`), so it goes where the
// walls go, and its trusses sit on the plates where they now are.
//
// A post stands in front of the front wall, its top made one node with the
// point of the front cap plate 1.2 m along it: that point becomes a node of
// the plate.
import { Assembly, Building, coincide, datum, DFL, ft, inch, Perimeter, TrussRoof, TrussShape, Wall } from "topo-cad";

const grade = DFL.No2;
const wall = Wall.template({ height: ft(8), grade });
const walls = Perimeter.start("Walls", [0, 0])
  .by([ft(16), 0], wall.named("Front"))
  .by([0, ft(12)], wall.named("Right"))
  .by([-ft(16), 0], wall.named("Back"))
  .close(wall.named("Left"))
  .free();
const corner = walls.node("Front/cap_plate", "start");

const roof = TrussRoof.of(TrussShape.fink({ span: ft(12), pitch: 6 / 12, overhang: inch(12) }), { at: corner, spanDir: [0, 1], length: ft(16), grade })
  .bearingOn(walls);

// A post drawn anywhere, free, its top on the plate 1.2 m along.
const post = Assembly.named("Post")
  .point("foot", [0, 0, 0]).point("top", [0, 0, ft(8)])
  .member("post", ["foot", "top"], { size: [4, 4], grade })
  .moved([3, -2, 0])
  .free();

export default Building.named("Placed by a corner").add(
  walls,
  roof,
  datum(corner, [5, 3, ft(8)]),
  post,
  coincide(post.node("top"), walls.node("Front/cap_plate", 1.2)),
);
