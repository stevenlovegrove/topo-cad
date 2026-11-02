// What of a view's lines the cursor is on, and whether a place of the model is shown there: asked
// the one way by whoever asks it (a mark's lines clicked on, a part's edge pointed at, a thing of
// the model dragged to where it is in a photo).
//
//   - how far the cursor is, on the screen, from a place, from the stretch between two places, or
//     from a run of them, as the view has them (`toPlace`, `toStretch`, `toRun`);
//   - whether it is shown there (`shown`): of a part that is shown, and not behind anything: the
//     ray to it meets none of the bodies the view draws before it (exact.js: `partAt`). So what can
//     be pointed at is what is seen.
//
// Places are in the scan's frame, as the views draw.

import { memberVisible } from "../objects.js";
import { partAt } from "./exact.js";

/** How far a point of the screen is from the stretch between two others: how far (pixels), how far along it (0 to 1), and where on it. */
function offStretch(x, y, p, q) {
  const dx = q[0] - p[0], dy = q[1] - p[1], t = Math.max(0, Math.min(1, ((x - p[0]) * dx + (y - p[1]) * dy) / (dx * dx + dy * dy || 1)));
  const screen = [p[0] + t * dx, p[1] + t * dy];
  return { d: Math.hypot(x - screen[0], y - screen[1]), t, screen };
}

/** The cursor in a view (an event, or where it is in the window): where it is on the view, and where the view has a place. */
export function cursorIn(v, e) {
  const r = v.canvas.getBoundingClientRect(), cam = v.camera(), [cx, cy] = Array.isArray(e) ? e : [e.clientX, e.clientY];
  return { v, x: cx - r.left, y: cy - r.top, on: (p) => v.project(cam, p) };
}

/**
 * The cursor by the stretch between two places: how far from it on the screen (`d`), how far along it the nearest
 * of it is (`t`: as the screen has it, which in perspective is not how far along it is in space), and where the
 * view has that. None: an end is behind the viewer.
 */
export function toStretch(c, a, b) {
  const p = c.on(a), q = c.on(b);
  return p && q ? { ...offStretch(c.x, c.y, p, q), ends: [a, b] } : null;
}

/** The cursor by a run of places (`closed`: on from the last to the first): the nearest of its stretches, as `toStretch` has it. */
export function toRun(c, pts, closed) {
  let best = null;
  for (let i = 0; i + 1 < pts.length || (closed && i < pts.length && pts.length > 2); i++) {
    const at = toStretch(c, pts[i], pts[(i + 1) % pts.length]);
    if (at && (!best || at.d < best.d)) best = at;
  }
  return best;
}

/**
 * Whether what of the model the cursor is by (as `toPlace`, `toStretch` or `toRun` gave it) is shown in the view
 * where the cursor is by it: `part`, the part it is of (its path), is shown, and it is not behind anything there.
 * (What is of no part, a named point: nothing is in front of it.)
 */
export function shown(c, at, part) {
  if (part && !memberVisible(part)) return false;
  // The ray through where it is on the screen meets no body before it (but the thing's own there, which it meets at
  // the thing itself, give or take). How far along that ray it is: where
  // the ray and the stretch come nearest (the place of the stretch that is there on the screen, in perspective too).
  const ray = c.v.ray(at.screen[0], at.screen[1]), [a, b] = at.ends, e = sub(b, a), r = sub(a, ray.origin);
  const ee = dot(e, e), ed = dot(e, ray.dir), dd = dot(ray.dir, ray.dir), den = ee * dd - ed * ed;
  const s = den > 1e-18 ? Math.max(0, Math.min(1, (ed * dot(ray.dir, r) - dd * dot(e, r)) / den)) : at.t;
  const far = dot(sub([a[0] + s * e[0], a[1] + s * e[1], a[2] + s * e[2]], ray.origin), ray.dir) / Math.sqrt(dd);
  const hit = partAt(c.v, ray);
  return !hit || hit.t > far - (3e-4 + 0.002 * far);
}
const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]], dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
