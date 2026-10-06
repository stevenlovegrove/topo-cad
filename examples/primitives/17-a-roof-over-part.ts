// A roof over part of a building, with another truss in some bays.
//
// A roof is set out from a node, for a length: it need not cover what it
// stands on. Here the walls are 24 ft long and the trusses start 4 ft along
// the front wall's cap plate (`walls.node("Front/cap_plate", ft(4))`) and run
// for 16 ft, so 4 ft at each end is open to the sky.
//
// Some trusses may be of another shape (`withShape`): the three in the middle
// here are fan trusses, the rest Fink. They have the same span and pitch, so
// they sit on the same plates and make the same roof.
import { Building, DFL, ft, inch, Perimeter, TrussRoof, TrussShape, Wall } from "topo-cad";

const [long, deep] = [ft(24), ft(14)];
const wall = Wall.template({ height: ft(8), grade: DFL.No2 });
const walls = Perimeter.start("Walls", [0, 0])
  .by([long, 0], wall.named("Front"))
  .by([0, deep], wall.named("Right"))
  .by([-long, 0], wall.named("Back"))
  .close(wall.named("Left"));

const shape = { span: deep, pitch: 5 / 12, overhang: inch(12) };
const roof = TrussRoof.of(TrussShape.fink(shape), { at: walls.node("Front/cap_plate", ft(4)), spanDir: [0, 1], length: ft(16), grade: DFL.No2 })
  .withShape([4, 5, 6], TrussShape.fan(shape))
  .bearingOn(walls);

export default Building.named("A roof over part").add(walls, roof);
