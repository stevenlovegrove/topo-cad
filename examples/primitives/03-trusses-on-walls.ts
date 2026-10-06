// The same, on walls: where axes meet, the bearing point is an ordinary node.
//
// A wall's cap plate has its axis on its top face, and a truss's bottom chord
// has its axis on its underside. Sat on the plates, the chord's axis crosses
// each plate's axis, so each bearing point is a node on both axes (in both
// paths): no node on a face is needed. The two end trusses lie along the end
// walls and share those walls' plates' end nodes.
//
// In the 3D view, turn on Skeleton: there are no orange diamonds here, only
// axis nodes; the trusses are still free things sat by the solve (squares).
import { Building, DFL, ft, inch, Perimeter, TrussRoof, TrussShape, Wall } from "topo-cad";

const wall = Wall.template({ height: ft(8), grade: DFL.No2 });
const walls = Perimeter.start("Walls", [0, 0])
  .by([ft(8), 0], wall)
  .by([0, ft(16)], wall)
  .by([-ft(8), 0], wall)
  .close(wall);

const roof = TrussRoof.of(TrussShape.fink({ span: ft(16), pitch: 6 / 12, overhang: inch(12) }), { at: walls.corner(1), spanDir: [0, 1], length: ft(8), grade: DFL.No2 })
  .bearingOn(walls);

export default Building.named("Trusses on walls").add(walls, roof);
