// What a floor and a roof are made of.
//
// A floor's deck and a roof's covering are given as their layers, top first,
// by the names of materials (`layers(...)`, from the table of material
// weights):
//
// - the floor: hardwood flooring on a plywood subfloor;
// - the roof: standing-seam steel, on an underlayment, on plywood.
//
// Each layer is a sheet of that material, one on another, as thick as it is:
// the floor's on its joists, the roof's on the planes of the roof (the tops of
// its trusses). What stands on the floor stands on the top of them. And their
// weight is a dead load on the framing under them, with nothing more said: per
// area of floor, and per area of slope.
//
// (Look at it in "3D, shaded": each layer is in the colour of what it is of;
// click one to be told. A ribbed sheet is drawn as the sheet, without its ribs.)
import { Building, DFL, Floor, ft, inch, layers, Perimeter, pitch, psf, Roof, Wall } from "topo-cad";

const grade = DFL.No2;
const [wide, deep] = [ft(12), ft(16)];
const wall = Wall.template({ height: ft(8), grade });
const box = (name: string) =>
  Perimeter.start(name, [0, 0])
    .by([wide, 0], wall.named("Front"))
    .by([0, deep], wall.named("Right"))
    .by([-wide, 0], wall.named("Back"))
    .close(wall.named("Left"));

const ground = box("Ground floor walls");
const floor = Floor.of({ joists: [2, 10], grade, spacing: inch(16) })
  .named("Floor")
  .from([0, 0], { along: [1, 0], length: wide, width: deep })
  .deck(layers("hardwood flooring 7/8", "plywood 23/32"))
  .bearingOn(ground)
  .load("live", psf(40));
const upper = box("Upper floor walls").on(floor);

const roof = Roof.named("Roof")
  .over(upper, { pitch: pitch(6), overhang: inch(12), gables: ["Front", "Back"], trusses: { grade } })
  .covering(layers("standing seam 24ga steel", "polystick mts", "plywood 15/32"))
  .load("snow", psf(25));

export default Building.named("What a floor and a roof are made of").add(ground, floor, upper, roof);
