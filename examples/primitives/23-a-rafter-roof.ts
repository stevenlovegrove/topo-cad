// A roof of rafters.
//
// Pairs of rafters run from the walls' plates up to the ridge, where each pair
// meets, every 24", with a ceiling tie across each pair at the plates. The roof is given on
// plan (where its ridge runs, how long), with its span and pitch; it is not
// given a height: it sits on the plates, as a floor does.
//
// Each rafter's foot is cut level and sits flat on the middle of the plate;
// its head is cut plumb, against its pair's. The ties lie on the plates
// beside the rafters.
//
// (What this does not show yet: a ridge board, a birdsmouth or a tail past the
// wall; the ties are not fastened to the rafters; and there are no hips.)
import { Building, DFL, ft, inch, Perimeter, psf, RafterRoof, Wall } from "topo-cad";

const grade = DFL.No2;
const [wide, deep] = [ft(12), ft(16)];
const wall = Wall.template({ height: ft(8), grade });
const walls = Perimeter.start("Walls", [0, 0])
  .by([wide, 0], wall.named("Front"))
  .by([0, deep], wall.named("Right"))
  .by([-wide, 0], wall.named("Back"))
  .close(wall.named("Left"));

const roof = RafterRoof.of({ span: wide, pitch: 6 / 12, rafters: [2, 8], grade, spacing: inch(24) })
  .named("Roof")
  .from([wide, 0], { along: [0, 1], length: deep })
  .bearingOn(walls)
  .load("dead", psf(12))
  .load("snow", psf(25));

export default Building.named("A rafter roof").add(walls, roof);
