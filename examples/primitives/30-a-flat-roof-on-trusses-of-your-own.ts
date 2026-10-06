// A flat roof beside a sloped one, on trusses of your own.
//
// The trusses here are not a standard shape: each has a bottom chord that
// runs on 3 ft past its front heel (a canopy over the front wall), and a tail
// past its back heel. They are given as they always are (`TrussShape.custom`,
// `TrussRoof.of`), member by member, and nothing of that is the roof's to say.
//
// The roof is given the trusses as its framing, and says what surfaces are
// on them:
//
// - a sloped part, two planes on the top chords, covered with standing-seam
//   steel on an underlayment on plywood;
// - a flat part over the front, on the bottom chords' run past the heel
//   (`carriedBy: "bottom_chord"`), covered with a membrane on plywood.
//
// The two parts overlap by a foot at the heel, and where they meet is not
// given: the higher is the roof, so they fold where the top chord's top comes
// up through the level of the bottom chord's, a little in from the heel. Each
// part's covering is its dead load, and the snow is over both: every load
// goes on the members that carry that part, over what of each is under the
// part where it is the roof, by the width of roof each truss carries.
import { Building, DFL, ft, inch, layers, Perimeter, pitch, type Point, psf, Roof, TrussRoof, TrussShape, Wall } from "topo-cad";

const grade = DFL.No2;
const [span, length, k, canopy, tail] = [ft(20), ft(12), pitch(4), ft(3), ft(1.5)];
const top = (s: number) => k * Math.min(s, span - s);
const shape = TrussShape.custom({
  name: "With a canopy",
  span,
  pitch: k,
  points: {
    F: [-canopy, 0],
    H0: [0, 0],
    H1: [span, 0],
    P: [span / 2, top(span / 2)],
    C: [span / 2, 0],
    T0: [span / 4, top(span / 4)],
    T1: [(3 * span) / 4, top(span / 4)],
    X1: [span + tail, -tail * k],
  },
  topChords: [
    ["H0", "T0", "P"],
    ["X1", "H1", "T1", "P"],
  ],
  bottomChords: [["F", "H0", "C", "H1"]],
  webs: [["P", "C"], ["T0", "C"], ["T1", "C"]],
})
  .sized("bottom_chord", [2, 6])
  .sized("top_chord", [2, 4])
  .sized("web", [2, 4]);

const wall = Wall.template({ height: ft(8), grade });
const walls = Perimeter.start("Walls", [0, 0])
  .by([length, 0], wall.named("Front"))
  .by([0, span], wall.named("Right"))
  .by([-length, 0], wall.named("Back"))
  .close(wall.named("Left"));

const trusses = TrussRoof.of(shape, { name: "Trusses", at: walls.corner(1), spanDir: [0, 1], length, spacing: inch(24), grade }).bearingOn(walls);

const strip = (y0: number, y1: number): Point[] => [[0, y0], [length, y0], [length, y1], [0, y1]];
const roof = Roof.named("Roof")
  .part("Slope", strip(0, span), { pitch: k, gables: [2, 4], edges: { 3: { overhang: tail } }, framing: trusses })
  .flat("Canopy", strip(-canopy, ft(1)), { framing: trusses, carriedBy: "bottom_chord" })
  .covering(layers("standing seam 24ga steel", "polystick mts", "plywood 15/32"), { on: "Slope" })
  .covering(layers("tpo 60 mil", "plywood 15/32"), { on: "Canopy" })
  .load("snow", psf(25))
  .load("roof_live", psf(20), { on: "Canopy" });

export default Building.named("A flat roof on trusses of your own").add(walls, roof);
