// The Cybertruck model's proportions (examples/cybertruck.ts), fitted to two photos of the truck: each free, between
// bounds, started where the present pipeline's fit started it (old-pipeline: examples/cybertruck.ts, its `unknown`s);
// the photos' cameras free, each started about where it was. What is seen in them (beside this, in
// `cybertruck.sightings.ts`) is the truck's important edges and its hubs: each edge here a line through points of the
// truck that are expressions of the proportions, on the outer face of the plate that is it, so that the fit moves it.
// The model is then made at what the fit finds (`proportions`, a scope of the model's).
//
// The photos are from Wikimedia Commons ("IMG Tesla Cybertruck - AUTO ATHINA 2024 - Side.jpg" and
// "StockholmCyberBackLeft02.jpg", each under the licence its page there gives), named by where they are on the web and
// not kept with this example.
//
// x is forward, y to the left, z up; the ground under the tailgate is the origin (mm).

import { node, free, line, add, sub, mul, div, dist, read, is, sighted, photo, placed, joints, mm, type Num, type Node } from "topo-part";
import { sightings } from "./cybertruck.sightings";

type E = Num<"length">;
type P = [E, E, E];
type V = [number, number, number];

// What is held: the sides' inner faces, either way (the truck's width, 2.03 m); the plates' thickness; the roof's peak,
// each way; the wheelbase (the truck's, 3807: no photo can say it); where the cab's back is; how high the front arch's
// slope comes down to the corner; the light bar's band, how deep; where the stainless ends at the front corners; the wheels' size and where their outer faces are.
export const held = { w: 33, t: 2, peakW: 25, wheelbase: 134, cabX: 72, meets: 32, band: 5, frontLow: 27, wheelR: 15.65, wheelOut: 33 + 2 + 1 };

/** Each proportion: where the original's fit started it, and its bounds (mm). */
export const starts = {
  bodyLow: [20, 15, 25], // the body's lower edge
  tailTop: [53.5, 45, 60], // the tail's top: the sails' points
  peakAlong: [120, 100, 135], // the roof's peak, along the truck
  peakHigh: [63, 56, 70], // and high
  chamfer: [6, 1, 9], // the front corners' chamfers, across
  barAlong: [198, 188, 212], // the light bar's top edge, along
  barHigh: [41, 36, 46], // and high
  beltBehind: [8, 3, 40], // where the beltline meets the hood, behind the front corner
  rearAxle: [46, 34, 60], // the rear axle, along
  archTop: [12, 8, 16], // each arch's top corners, each way of its axle
  archFoot: [22, 16, 34], // its feet, each way
  rearArch: [42, 37, 48], // the rear arch's top, high
  frontArch: [40.5, 37, 48], // the front arch's top, high
  frontArchGap: [4, 2.5, 12], // the front arch's front top corner, behind the front corner
  frontArchHigh: [38, 37, 46], // and high
  cabRise: [0, -6, 8], // the roof's back, over the line from the tail to the peak
  rockerLow: [13.5, 10, 17], // the rocker's lower edge
} as const;
export type Proportions = Record<keyof typeof starts, number>;
/** A camera: where it is, its x (to the right of the picture) and y (down it), and its focal length (pixels). */
export type Camera = { origin: V; x: V; y: V; focal: number };

const v = (p: V): P => [mm(p[0]), mm(p[1]), mm(p[2])];
const plus = (a: P, b: P): P => [add(a[0], b[0]), add(a[1], b[1]), add(a[2], b[2])];
const minus = (a: P, b: P): P => [sub(a[0], b[0]), sub(a[1], b[1]), sub(a[2], b[2])];
const times = (a: P, k: Num<"ratio">): P => [mul(a[0], k), mul(a[1], k), mul(a[2], k)];

/** A camera where it was about (`from`, looking at `to`): its x to the right of the picture, its y down, its z the way
 *  it looks; free to move and turn from there. Its focal length read to within half of what it was guessed. */
function camera(name: string, image: string, size: [number, number], from: V, to: V, focal: number, start?: Camera) {
  const unit = (a: V): V => { const l = Math.hypot(...a); return [a[0] / l, a[1] / l, a[2] / l]; };
  const cross = (a: V, b: V): V => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
  const ahead = unit([to[0] - from[0], to[1] - from[1], to[2] - from[2]]);
  // (Started where it was about, or where `start` says.)
  const right = start?.x ?? unit(cross(ahead, [0, 0, 1]));
  const down = start?.y ?? cross(ahead, right);
  from = start?.origin ?? from;
  const f = free(start?.focal ?? focal);
  const p = photo(name, { image, size, lens: { focal: f }, at: placed({ origin: [mm(from[0]), mm(from[1]), mm(from[2])], x: right, y: down, joint: joints.free() }) });
  return { p, about: read(is(p.focal, focal), focal / 2) };
}

/** The fit, as fields of the part that holds it: its proportions (started where the original's fit started them, or
 *  where `from` says), its photos (their cameras started about where they were, or where `cameras` says), the truck's
 *  edges they see, and what is seen. */
export function fit(from: Partial<Proportions> = {}, cameras: Partial<Record<"side" | "bed", Camera>> = {}) {
  const { w, t, peakW, wheelbase, cabX, meets, band, frontLow, wheelR, wheelOut } = held;
  const f = Object.fromEntries(Object.entries(starts).map(([k, [at, min, max]]) => [k, free(mm(from[k as keyof Proportions] ?? at), { min: mm(min), max: mm(max) })])) as unknown as Record<keyof typeof starts, E>;
  // The truck's points, as the model has them (each side: s 1 left, −1 right), but as expressions of the proportions.
  const corner = sub(f.barAlong, f.chamfer), barW = sub(mm(w), f.chamfer);
  const beltFrom = sub(corner, f.beltBehind);
  const hoodZ = (x: E): E => add(f.peakHigh, mul(sub(f.barHigh, f.peakHigh), div(sub(x, f.peakAlong), sub(f.barAlong, f.peakAlong))));
  const pt = {
    tail: (s: number): P => [0, mm(s * w), f.tailTop],
    belt: (s: number): P => [beltFrom, mm(s * w), hoodZ(beltFrom)],
    peak: (s: number): P => [f.peakAlong, mm(s * peakW), f.peakHigh],
    front: (s: number): P => [corner, mm(s * w), hoodZ(corner)],
    bar: (s: number): P => [f.barAlong, mul(s, barW), f.barHigh],
    chin: (s: number): P => [f.barAlong, mul(s, barW), sub(f.barHigh, mm(band))],
    // (The chin's lower corner each way: on its plane, as high as the stainless comes down at the front.)
    chinLow: (s: number): P => {
      const x = add(corner, mul(sub(f.barAlong, corner), div(sub(mm(frontLow), f.bodyLow), sub(sub(f.barHigh, mm(band)), f.bodyLow))));
      return [x, mul(s, sub(mm(w), sub(x, corner))), mm(frontLow)];
    },
    // (The roof's back edge, at the cab's back, so high over the line from the tail to the peak: in the window's plane.)
    cab: (s: number): P => {
      const a = pt.tail(s), [u, q] = [minus(pt.belt(s), a), minus(pt.peak(s), a)];
      const n = [sub(mul(u[1], q[2]), mul(u[2], q[1])), sub(mul(u[2], q[0]), mul(u[0], q[2])), sub(mul(u[0], q[1]), mul(u[1], q[0]))];
      const z = add(add(f.tailTop, mul(sub(f.peakHigh, f.tailTop), div(mm(cabX), f.peakAlong))), f.cabRise);
      return [mm(cabX), sub(a[1], add(mul(sub(mm(cabX), a[0]), div(n[0], n[1])), mul(sub(z, a[2]), div(n[2], n[1])))), z];
    },
  };
  const axles = { rear: f.rearAxle, front: add(f.rearAxle, mm(wheelbase)) };
  const arch = {
    rear: { r: [sub(axles.rear, f.archFoot), f.bodyLow], rt: [sub(axles.rear, f.archTop), f.rearArch], ft: [add(axles.rear, f.archTop), f.rearArch], f: [add(axles.rear, f.archFoot), f.bodyLow] },
    front: { r: [sub(axles.front, f.archFoot), f.bodyLow], rt: [sub(axles.front, f.archTop), f.frontArch], ft: [sub(corner, f.frontArchGap), f.frontArchHigh], f: [corner, mm(meets)] },
  } as Record<"rear" | "front", Record<"r" | "rt" | "ft" | "f", [E, E]>>;

  // Points of the root, and the edges between them (each on the outer face of the plate that is it: moved out by its
  // thickness, square to it; the length of a way that is an expression, as a distance between nodes).
  const nodes: Record<string, Node> = {};
  const edges: Record<string, ReturnType<typeof line>> = {};
  const origin = node(0, 0, 0);
  nodes.origin = origin;
  // (Keyed without "/", which is the way into a part in an address: `side R: lower edge`.)
  const key = (name: string) => name.replace("/", ": ");
  const at = (name: string, p: P) => (nodes[key(name)] = node(p[0], p[1], p[2]));
  // (Each seen as a whole line: in a photo, an edge goes on past where the model's ends.)
  const edge = (name: string, a: P, b: P) => { edges[key(name)] = line(at(`${name} a`, a), at(`${name} b`, b)); };
  /** `t` out along a way (an expression, of any size). */
  const out = (name: string, way: P): P => {
    const k = div(mm(t), dist(origin, at(`${name} way`, way)));
    return times(way, k);
  };
  const side = (s: number, x: E, z: E): P => [x, mm(s * (w + t)), z];
  // Of the right side: its lower edge, its arches' edges, its beltline, its back edge; and the left side's back edge.
  edge("side R/lower edge", side(-1, mm(0), f.bodyLow), side(-1, mm(100), f.bodyLow));
  for (const k of ["rear", "front"] as const) {
    const a = arch[k];
    edge(`side R/${k} arch rear slope`, side(-1, ...a.r), side(-1, ...a.rt));
    edge(`side R/${k} arch top`, side(-1, ...a.rt), side(-1, ...a.ft));
    edge(`side R/${k} arch front slope`, side(-1, ...a.ft), side(-1, ...a.f));
  }
  edge("side R/beltline", side(-1, 0, f.tailTop), side(-1, beltFrom, hoodZ(beltFrom)));
  for (const [n, s] of [["L", 1], ["R", -1]] as const) edge(`side ${n}/back edge`, side(s, 0, 0), side(s, 0, mm(50)));
  // Of the windows: the roof's line behind the cab and over it, and the right one's front edge (the A-pillar), up the
  // hood. Each window's way out: square to the beltline and the line to the peak, outward.
  for (const [n, s] of [["L", 1], ["R", -1]] as const) {
    const a = pt.tail(s), [u, q] = [minus(pt.belt(s), a), minus(pt.peak(s), a)];
    // (Its normal, a length: of the cross product, by a millimetre. The cross product looks to the right either side:
    // the left one's is turned.)
    const nn = [sub(mul(u[1], q[2]), mul(u[2], q[1])), sub(mul(u[2], q[0]), mul(u[0], q[2])), sub(mul(u[0], q[1]), mul(u[1], q[0]))].map((c) => div(c, mm(-s))) as unknown as P;
    const o = out(`window ${n}`, nn);
    edge(`window ${n}/roof behind the cab`, plus(pt.tail(s), o), plus(pt.cab(s), o));
    if (s < 0) {
      edge(`window ${n}/roof over the cab`, plus(pt.cab(s), o), plus(pt.peak(s), o));
      edge(`window ${n}/A-pillar`, plus(pt.belt(s), o), plus(pt.peak(s), o));
    }
  }
  // Of the hood: its right edge over the chamfer, from the side's front corner to the light bar's end. Its way out: up
  // and forward, square to its slope.
  const hoodOut = out("hood", [sub(f.peakHigh, f.barHigh), 0, sub(f.barAlong, f.peakAlong)]);
  edge("hood/edge R over the chamfer", plus(pt.front(-1), hoodOut), plus(pt.bar(-1), hoodOut));
  // Of the right chamfer: its lower front edge (seen from the side, the light bar's end), up the chin's slope from where
  // the stainless ends at the front corner to the chin's top. (Its way out: at 45°.)
  const chamferOut: P = [mm(t / Math.SQRT2), mm(-t / Math.SQRT2), 0];
  edge("chamfer R/front edge", plus(pt.chinLow(-1), chamferOut), plus(pt.chin(-1), chamferOut));
  // The right rocker's lower edge: its foot, a millimetre in from the side's outer face.
  edge("rocker R/foot", [0, mm(-(w + t - 1)), f.rockerLow], [mm(100), mm(-(w + t - 1)), f.rockerLow]);
  // Each right hub: the middle of its wheel's outer face, as where a line along the truck and one up cross there.
  const hubs: Record<string, P> = { "axle rear/hub R": [axles.rear, mm(-wheelOut), mm(wheelR)], "axle front/hub R": [axles.front, mm(-wheelOut), mm(wheelR)] };
  for (const [n, h] of Object.entries(hubs)) {
    edge(`${n} along`, h, plus(h, [mm(10), 0, 0]));
    edge(`${n} up`, h, plus(h, [0, 0, mm(10)]));
  }

  // The photos: one of its right side, square on; one of its bed, from behind and to its left.
  const side_ = camera("side photo", "https://upload.wikimedia.org/wikipedia/commons/thumb/e/ee/IMG_Tesla_Cybertruck_-_AUTO_ATHINA_2024_-_Side.jpg/960px-IMG_Tesla_Cybertruck_-_AUTO_ATHINA_2024_-_Side.jpg", [960, 720], [83, -181, 38], [83, 0, 38], 693, cameras.side);
  const bed = camera("bed photo", "https://upload.wikimedia.org/wikipedia/commons/thumb/e/e3/StockholmCyberBackLeft02.jpg/960px-StockholmCyberBackLeft02.jpg", [960, 640], [-117, 15, 56], [70, 0, 30], 1000, cameras.bed);
  const photos = { side: side_.p, bed: bed.p };
  const says: Record<string, unknown> = { "side photo's focal, about": side_.about, "bed photo's focal, about": bed.about };
  for (const s of sightings) {
    const lines = s.edge in hubs ? [`${s.edge} along`, `${s.edge} up`] : [s.edge];
    for (const l of lines) {
      if (!edges[key(l)]) throw new Error(`a sighting of ${l}, which is not an edge here`);
      says[lines.length > 1 ? `${s.name} (${key(l)})` : s.name] = read(sighted(photos[s.photo], { seen: "line", line: edges[key(l)] }, s.pixel), s.within);
    }
  }
  return { proportions: f, nodes, edges, photos, says };
}
