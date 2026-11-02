// A roof fitted to a photo (examples/a-roof-fitted-to-a-photo.ts): a fit that drives a model's geometry live.
//
// A gable roof of two plates over a 4,000 by 4,000 plan, each from its eave up to the ridge. How high the ridge is
// (`rise`) is not said: it is found from a photo. Each plate is as long up its slope as its run and the rise make it
// (`hypot`: a square root of what is found), and it is placed by a frame through points of the roof that are
// expressions of the rise (its eave, along it, and the ridge): so as the fit moves the rise, the plates move and grow
// with it, in the one solve. What is seen in the photo is the plates' own edges: the ridge, the eaves, the gables'
// slopes (lines between the plates' corners).
//
// The photo is made up: its sightings are where those edges are, seen from a camera 2.5 m to the side and 5 m in
// front, 2.5 m up, at a rise of 900. Its camera is found with the rise, started off from where it was; the rise
// starts at 600.

import { part, node, free, line, plate, frame, hypot, photo, placed, joints, read, sighted, materials, mm } from "topo-part";

type V = [number, number, number];
const [run, length, t] = [2000, 4000, 18];

// (Where the camera was about: its x to the right of the picture, its y down, its z the way it looks.)
function camera(from: V, to: V, focal: number) {
  const unit = (a: V): V => { const l = Math.hypot(...a); return [a[0] / l, a[1] / l, a[2] / l]; };
  const cross = (a: V, b: V): V => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
  const ahead = unit([to[0] - from[0], to[1] - from[1], to[2] - from[2]]);
  const right = unit(cross(ahead, [0, 0, 1]));
  return photo("photo", { image: "made up", size: [1600, 1200], lens: { focal }, at: placed({ origin: [mm(from[0]), mm(from[1]), mm(from[2])], x: right, y: cross(ahead, right), joint: joints.free() }) });
}

const sightings = [
  { name: "ridge 0.3", line: "ridge", pixel: [674.388, 512.362] },
  { name: "ridge 0.7", line: "ridge", pixel: [913.77, 483.238] },
  { name: "left eave 0.3", line: "left eave", pixel: [892.305, 787.649] },
  { name: "left eave 0.7", line: "left eave", pixel: [1152.018, 722.283] },
  { name: "right eave 0.3", line: "right eave", pixel: [533.971, 595.544] },
  { name: "right eave 0.7", line: "right eave", pixel: [742.878, 564.974] },
  { name: "left gable 0.3", line: "left gable", pixel: [584.182, 741.443] },
  { name: "left gable 0.7", line: "left gable", pixel: [508.591, 618.538] },
  { name: "right gable 0.3", line: "right gable", pixel: [379.805, 600.764] },
  { name: "right gable 0.7", line: "right gable", pixel: [422.096, 567.683] },
] as const;

export default part("a roof fitted to a photo", () => {
  const rise = free(mm(600));
  // The roof's points: each eave's ends, and the ridge's (its height the rise).
  const points = {
    leftEave: node(0, 0, 0), leftEaveEnd: node(mm(length), 0, 0),
    rightEave: node(mm(length), mm(2 * run), 0), rightEaveEnd: node(0, mm(2 * run), 0),
    ridge: node(0, mm(run), rise), ridgeEnd: node(mm(length), mm(run), rise),
  };
  // Each plate: along its eave, up to the ridge; its underside in the roof's plane (its frame: at its eave's start,
  // its x along the eave, its xy plane through the ridge's end over that start).
  const up = hypot(mm(run), rise);
  const left = plate("left", mm(length), up, mm(t), materials.aluminium6061T6, [], { at: frame(points.leftEave, { x: points.leftEaveEnd, xy: points.ridge }) });
  const right = plate("right", mm(length), up, mm(t), materials.aluminium6061T6, [], { at: frame(points.rightEave, { x: points.rightEaveEnd, xy: points.ridgeEnd }) });
  // The edges seen: between the plates' own corners (each anticlockwise from its origin: its eave's start, its end,
  // up at its end, up at its start).
  const edges = {
    ridge: line(left.corners[3], left.corners[2]),
    "left eave": line(left.corners[0], left.corners[1]),
    "right eave": line(right.corners[0], right.corners[1]),
    "left gable": line(left.corners[0], left.corners[3]),
    "right gable": line(right.corners[1], right.corners[2]),
  };
  const cam = camera([-2300, -5300, 2700], [2200, 1800, 400], 1500);
  const seen = Object.fromEntries(sightings.map((s) => [s.name, read(sighted(cam, { seen: "line", line: edges[s.line] }, [...s.pixel] as [number, number]), 1)]));
  return { rise, points, left, right, edges, camera: cam, says: seen };
});
