// A hip roof.
//
// A roof is given as its surfaces, and those by its edges: a roof over these
// walls, at 6 in 12, running 16" out past them. No edge is said to be a
// gable, so every edge is an eave: a plane rises inward from each, and the roof
// is the lowest of them. The ridge and the four hips are not given: they are
// where the planes meet.
//
// It is framed with rafters, and the framing follows from the same edges:
// pairs of rafters along the ridge; at each end, where the end wall is an eave
// too, a hip rafter from each corner to the end of the ridge, and jack rafters
// from the plates up to the hips, each cut against its hip's side. The hips
// are cut against the last pair. Every rafter's foot is cut level on a plate.
//
// Say one wall is a gable (`gables: ["Left"]`) and that end is a gable, with
// a pair of rafters at it, and the other still a hip.
//
// (Not shown yet: tails past the walls, a ridge board, a hip backed to the
// planes.)
import { Building, DFL, ft, inch, Perimeter, pitch, psf, Roof, Wall } from "topo-cad";

const [wide, deep] = [ft(24), ft(16)];
const wall = Wall.template({ height: ft(8), grade: DFL.No2 });
const walls = Perimeter.start("Walls", [0, 0])
  .by([wide, 0], wall.named("Front"))
  .by([0, deep], wall.named("Right"))
  .by([-wide, 0], wall.named("Back"))
  .close(wall.named("Left"));

const roof = Roof.named("Roof")
  .over(walls, { pitch: pitch(6), overhang: inch(16), rafters: { size: [2, 8], grade: DFL.No2 } })
  .load("dead", psf(12))
  .load("snow", psf(25));

export default Building.named("A hip roof").add(walls, roof);
