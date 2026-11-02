// A little geometry the scene's modules share: boxes, and rays against lines and planes.

import { vec } from "../glview.js";

/** The middle of a box ({ min, max }), and its size across. */
export const middle = (box) => box.min.map((x, k) => 0.5 * (x + box.max[k]));
export const across = (box) => Math.hypot(...vec.sub(box.max, box.min));
/** The box about some points: { min, max }. */
export const boxAbout = (points) => ({ min: [0, 1, 2].map((k) => Math.min(...points.map((p) => p[k]))), max: [0, 1, 2].map((k) => Math.max(...points.map((p) => p[k]))) });
/** A box's 8 corners (corner k: bit a of k says which end along axis a); the edges and faces of a box given so. */
export const corners = (box) => [0, 1, 2, 3, 4, 5, 6, 7].map((k) => [k & 1 ? box.max[0] : box.min[0], k & 2 ? box.max[1] : box.min[1], k & 4 ? box.max[2] : box.min[2]]);
export const BOX_EDGES = [[0, 1], [2, 3], [4, 5], [6, 7], [0, 2], [1, 3], [4, 6], [5, 7], [0, 4], [1, 5], [2, 6], [3, 7]];
export const BOX_FACES = [[0, 1, 3, 2], [4, 5, 7, 6], [0, 1, 5, 4], [2, 3, 7, 6], [0, 2, 6, 4], [1, 3, 7, 5]];

/** How far along a line (through `p`, along `d`) the point nearest a ray is; null if they run together. */
export function alongLine(p, d, ray) {
  const w = vec.sub(p, ray.origin), a = vec.dot(d, d), b = vec.dot(d, ray.dir), c = vec.dot(ray.dir, ray.dir), den = a * c - b * b;
  return Math.abs(den) < 1e-6 ? null : (b * vec.dot(ray.dir, w) - c * vec.dot(d, w)) / den;
}
/** Where a ray meets a plane (through `p`, normal `n`); null if it runs along it. */
export function onPlane(p, n, ray) {
  const k = vec.dot(n, ray.dir);
  return Math.abs(k) < 0.02 ? null : vec.add(ray.origin, vec.scale(ray.dir, vec.dot(n, vec.sub(p, ray.origin)) / k));
}
