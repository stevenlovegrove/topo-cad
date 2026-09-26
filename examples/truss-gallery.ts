// Every standard truss type, each on its own small building, for comparison.
import { Building, DFL, ft, inch, Perimeter, TrussRoof, TrussShape, Wall } from "topo-cad";

const span = ft(24);
const o = { span, pitch: 6 / 12, overhang: inch(12) };
const shapes: [string, TrussShape][] = [
  ["Fink", TrussShape.fink(o)],
  ["Fan", TrussShape.fan(o)],
  ["King post", TrussShape.kingPost({ ...o, span: ft(16) })],
  ["Howe", TrussShape.howe({ ...o, panels: 6 })],
  ["Pratt", TrussShape.pratt({ ...o, panels: 6 })],
  ["Scissors", TrussShape.scissors({ ...o, bottomPitch: 3 / 12 })],
  ["Mono", TrussShape.mono({ ...o, span: ft(12), panels: 4 })],
];

const wall = Wall.template({ height: ft(8), grade: DFL.No2 });
const bay = ft(4);

const items = shapes.flatMap(([name, shape], i) => {
  const x0 = i * ft(10);
  const s = shape.spec.span;
  const walls = Perimeter.start(`${name} walls`, [x0, 0])
    .by([bay, 0], wall)
    .by([0, s], wall)
    .by([-bay, 0], wall)
    .close(wall);
  const roof = TrussRoof.of(shape, { name, origin: [x0, 0], spanDir: [0, 1], length: bay, grade: DFL.No2 }).bearingOn(walls);
  return [walls, roof];
});

export default Building.named("Standard truss types").info({ designer: "topo-cad", date: "2026-09-26" }).add(items);
