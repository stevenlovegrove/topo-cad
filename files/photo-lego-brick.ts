// A Lego brick, fitted to a photo of one (examples/photo-lego-brick.ts), written on the part core. The brick is a thing
// whose measures are well known, so what a photo says of it can be checked.
//
// What is held: the 2 by 4 brick's body, 31.8 by 15.8, and its eight studs, 8 apart. What is left to the photo (each
// free, started a little off): how tall the body is, how wide a stud is, and how far a stud stands up. A brick is 9.6
// tall, and its studs 4.8 across and 1.7 high. From this one photo, with what is said to be seen in it (beside this, in
// `photo-lego-brick.sightings.ts`: the body's corners and edges, and places round the studs' rims), the fit finds about
// 9.4, 4.6 and 1.9: within a few tenths of a millimetre, from pixels put by eye to within a few.
//
// Nothing is said of the camera but about where it was (a free joint, started there, looking at the brick) and about
// its focal length (1200 pixels, read to within a third: a guess). It is found with the brick. A corner seen is the two
// edges of the body that meet there, each seen at that pixel. The photo is a part (`photo`): its camera, its picture,
// and its focal length, a param of its own; what is said to be seen in it in the workspace (dragged there, through the
// photo) is beside this too, in `photo-lego-brick.sightings.gen.ts`, which the page writes.
//
// The photo is "Lego Brick.jpg" by Срђан Весић, from Wikimedia Commons
// (https://commons.wikimedia.org/wiki/File:Lego_Brick.jpg), licensed CC BY-SA 3.0. It is named by where it is on the
// web, and is not kept with this example.
//
// The brick's frame: its long way along x, its middle at the origin, its underside at z = 0, its studs up.

import { part, node, free, line, add, div, polygon, circle, extrusion, placed, frame, joints, read, is, sighted, photo, seenIn, materials, mm, type Node } from "topo-part";
import { sightings } from "./photo-lego-brick.sightings";
import { sightings as said } from "./photo-lego-brick.sightings.gen";

// (Laid out in millimetres: what is held of the brick, and where the camera was about.)
type V = [number, number, number];
const [long, wide, pitch] = [31.8, 15.8, 8];
const at = (x: number, y: number) => [mm(x), mm(y)] as const;
const sub3 = (a: V, b: V): V => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const cross = (a: V, b: V): V => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const unit = (a: V): V => { const l = Math.hypot(...a); return [a[0] / l, a[1] / l, a[2] / l]; };

// The camera, where it was about (70 in front of the brick and 80 above, a little to the left), looking at it: its x to
// the right of the picture, its y down, its z the way it looks. Free to move and turn from there.
const from: V = [-10, -70, 80];
const ahead = unit(sub3([0, 0, 0], from));
const right = unit(cross(ahead, [0, 0, 1]));
const down = cross(ahead, right);
const focal = free(1200);
const camera = photo("camera", {
  image: "https://upload.wikimedia.org/wikipedia/commons/1/12/Lego_Brick.jpg", size: [800, 600], lens: { focal },
  at: placed({ origin: [mm(from[0]), mm(from[1]), mm(from[2])], x: right, y: down, joint: joints.free() }),
});

export default part("brick", (b) => {
  const tall = free(mm(9)), across = free(mm(4)), up = free(mm(1));
  // The body's corners, by the edges of its plan that meet there (e1 along its front, e2 its right end, e3 its back, e4
  // its left end), at its top and its underside.
  const plan: Record<string, [number, number]> = { "e1/e2": [long / 2, -wide / 2], "e2/e3": [long / 2, wide / 2], "e3/e4": [-long / 2, wide / 2], "e4/e1": [-long / 2, -wide / 2] };
  // (Named without "/", which is the way into a part in an address: `e1 e2 top`.)
  const key = (id: string) => id.replace("/", " ").replace(".", " ");
  const corners: Record<string, Node> = {};
  for (const [k, [x, y]] of Object.entries(plan)) {
    corners[key(`${k}.top`)] = node(...at(x, y), tall);
    corners[key(`${k}.under`)] = node(...at(x, y), 0);
  }
  const ends: Record<string, [string, string]> = { e1: ["e4/e1", "e1/e2"], e2: ["e1/e2", "e2/e3"], e3: ["e2/e3", "e3/e4"], e4: ["e3/e4", "e4/e1"] };
  const edges: Record<string, ReturnType<typeof line>> = {};
  for (const [e, [a, c]] of Object.entries(ends)) for (const at of ["top", "under"]) edges[key(`${e}.${at}`)] = line(corners[key(`${a}.${at}`)], corners[key(`${c}.${at}`)]);
  // (A corner's two edges: those of its plan, at its height.)
  const edgesAt = (corner: string): string[] => { const [ab, at] = corner.split("."); return ab.split("/").map((e) => `${e}.${at}`); };
  const outline = [node(...at(-long / 2, -wide / 2), 0), node(...at(long / 2, -wide / 2), 0), node(...at(long / 2, wide / 2), 0), node(...at(-long / 2, wide / 2), 0)];
  const body = part("body", { outline }, { shapes: [extrusion(polygon(...outline), { to: tall, material: materials.ABS })] });
  // The studs, two rows of four: each a cylinder standing on the body's top; its top's rim what is seen.
  const studs: Record<string, object> = {};
  const rims: Record<string, { centre: Node; rim: Node }> = {};
  for (const [row, y] of [["back", pitch / 2], ["front", -pitch / 2]] as [string, number][]) {
    for (let k = 0; k < 4; k++) {
      const x = (k - 1.5) * pitch, name = `${row} ${k + 1}`;
      const foot = node(...at(x, y), tall), ex = node(...at(x + 1, y), tall), ey = node(...at(x, y + 1), tall);
      const centre = node(0, 0, 0), edge = node(div(across, 2), 0, 0);
      const stud = part(name, { centre, edge }, { at: frame(foot, { x: ex, xy: ey }), shapes: [extrusion(circle(centre, edge), { to: up, material: materials.ABS })] });
      const top = node(...at(x, y), add(tall, up)), rim = node(add(mm(x), div(across, 2)), mm(y), add(tall, up));
      studs[name] = { foot, ex, ey, stud };
      rims[name] = { centre: top, rim };
    }
  }
  // What is seen: each sighting a reading, to within its pixels.
  const seen: Record<string, unknown> = {};
  for (const s of sightings) {
    if (s.what.kind === "rim") {
      const r = rims[s.what.id];
      seen[s.name] = read(sighted(camera, { seen: "circle", centre: r.centre, rim: r.rim, normal: b.z }, s.pixel), s.within);
    } else {
      for (const e of s.what.kind === "corner" ? edgesAt(s.what.id) : [s.what.id]) seen[`${s.name} (${e})`] = read(sighted(camera, { seen: "line", line: edges[key(e)] }, s.pixel), s.within);
    }
  }
  return {
    tall, across, up, camera, corners, edges, body, studs, rims,
    says: { "focal, about": read(is(camera.focal, 1200), 400), ...seen, ...seenIn(camera, said) },
  };
});
