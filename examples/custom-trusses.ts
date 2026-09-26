// Custom truss shapes and repeated custom topology.
import {
  Assembly, Building, DFL, ft, inch, intersect, Perimeter, pitch, psf, repeat, TrussRoof, TrussShape, Wall,
} from "topo-cad";

const width = ft(20);
const depth = ft(24);
const wall = Wall.template({ height: ft(8), grade: DFL.No2 });

const walls = Perimeter.start("Walls", [0, 0])
  .to([width, 0], wall.named("Front"))
  .to([width, depth], wall.named("Right"))
  .to([0, depth], wall.named("Back"))
  .close(wall.named("Left"));

// A custom symmetric web pattern, written as its left half: two top-chord
// panel points, two bottom panel points, and a centre bottom point on the
// ridge line (not mirrored). The right half is generated.
const span = depth;
const k = pitch(8);
const custom = TrussShape.symmetric({
  name: "Double W",
  span,
  pitch: k,
  overhang: inch(18),
  top: { T1: span * 0.2, T2: span * 0.38 },
  bottom: { B1: span * 0.28, C: span / 2 },
  webs: [["T1", "B1"], ["B1", "T2"], ["T2", "C"], ["C", "P"]],
});

// Or start from a standard type and edit it: a Howe with its first diagonal
// replaced by a web to a new point where two lines cross.
const howe = TrussShape.howe({ span, pitch: k, panels: 6, overhang: inch(12) });
const kink = intersect(howe.point("H0"), howe.point("P"), howe.point("B2"), [span * 0.3, span]);
const edited = howe
  .withoutWeb("B1", "T2")
  .withChordPoint("Tx", "T1", "T2", 0.5)
  .web("B1", "Tx")
  .web("Tx", "B2")
  .named("Howe (edited)");
void kink;

const roof = TrussRoof.of(custom, { origin: [0, 0], spanDir: [0, 1], length: width, spacing: inch(24), grade: DFL.No2 })
  .bearingOn(walls)
  .load("dead", psf(20))
  .load("snow", psf(25));

// A pergola bay as custom topology: one post and the beam to the next post
// position. Repeating it makes the bays share points, so each beam lands on
// the next bay's post; one more post closes the run. The beam outranks the
// post, so it runs over the post top (bearing) rather than being nailed to
// the post's side; beams meeting over a post are spliced there.
const bayWidth = ft(8);
const postHeight = ft(8);
const bay = Assembly.named("Pergola bay")
  .point("base", [0, 0, 0])
  .point("top", [0, 0, postHeight])
  .point("next", [bayWidth, 0, postHeight])
  .member("post", ["base", "top"], { size: [4, 4], grade: DFL.No2 })
  .member("beam", ["top", "next"], { size: [4, 8], grade: DFL.No2, anchor: [0, -0.5], priority: 10 })
  .supportedAt("base");
const lastPost = Assembly.named("Pergola end post")
  .point("base", [0, 0, 0])
  .point("top", [0, 0, postHeight])
  .member("post", ["base", "top"], { size: [4, 4], grade: DFL.No2 })
  .supportedAt("base");
const pergolaAt: readonly [number, number, number] = [0, -ft(10), 0];
const pergola = [...repeat(bay.moved(pergolaAt), 3, [bayWidth, 0, 0]), lastPost.moved([3 * bayWidth, -ft(10), 0])];

export const alternatives = { custom, edited };

export default Building.named("Custom trusses")
  .info({ designer: "topo-cad", date: "2026-09-26" })
  .add(walls, roof, pergola);
