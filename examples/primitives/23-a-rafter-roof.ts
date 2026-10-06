// A roof of rafters.
//
// The roof is given as its surfaces: it is over the walls, at 6 in 12, and the
// front and back are gables; so the other two edges are eaves, and the ridge
// is where their planes meet. It is framed with rafters: pairs from the
// walls' plates up to the ridge, every 24", with a ceiling tie across each
// pair at the plates. It is not given a height: the rafters sit on the plates,
// as a floor does, and the roof's planes are the tops of the rafters.
//
// Each rafter's foot is cut level and sits flat on the middle of the plate;
// its head is cut plumb, against its pair's. The ties lie on the plates
// beside the rafters.
//
// (What this does not show yet: a ridge board, a birdsmouth or a tail past the
// wall; the ties are not fastened to the rafters.)
import { Building, DFL, ft, inch, Perimeter, pitch, psf, Roof, Wall } from "topo-cad";

const grade = DFL.No2;
const [wide, deep] = [ft(12), ft(16)];
const wall = Wall.template({ height: ft(8), grade });
const walls = Perimeter.start("Walls", [0, 0])
  .by([wide, 0], wall.named("Front"))
  .by([0, deep], wall.named("Right"))
  .by([-wide, 0], wall.named("Back"))
  .close(wall.named("Left"));

const roof = Roof.named("Roof")
  .over(walls, { pitch: pitch(6), gables: ["Front", "Back"], rafters: { size: [2, 8], grade, spacing: inch(24) } })
  .load("dead", psf(12))
  .load("snow", psf(25));

export default Building.named("A rafter roof").add(walls, roof);
