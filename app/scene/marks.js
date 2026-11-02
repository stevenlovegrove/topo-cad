// What is drawn in the views beside the model and the points: the lines, patches and pins of
// what is in the scene, and the handles of what is chosen, with what a click on one of them and
// a drag of a handle do. What each kind of thing draws, and what its handles do, is its kind's
// to say (kinds.js): here they are gathered, and given to the views.

import { st, views } from "./state.js";
import { BOX_EDGES } from "./geom.js";
import { everyKind, kindOf } from "./kinds.js";
import { cursorIn, toRun, toStretch } from "./lines.js";

/** Every kind's marks and handles, as they are now, in every view. */
export function rebuildMarks() {
  if (!st) return;
  const segs = [], fills = [], pins = [], handles = [], slates = [];
  const out = {
    /** A line between two points (scan frame), in a colour; `only`: in the views it says so of (a function of a view), not in every one. */
    line: (a, b, rgb, only) => segs.push([a, b, rgb, only]),
    /** The edges of a box given by its 8 corners. */
    box: (c, rgb) => { for (const [a, b] of BOX_EDGES) segs.push([c[a], c[b], rgb]); },
    /** A filled patch (four corners, in order; rgba); not in a view clipped to the slab `inside`, which is inside it (or, `inside` a function of a view: not in one it says so of). */
    fill: (q, rgba, inside) => fills.push([q[0], q[1], q[2], rgba, inside], [q[0], q[2], q[3], rgba, inside]),
    /** A picture standing in the scene: { key, corners (the first row's ends, then the last's), stamp, image: { w, h, rgba } }; `only`: in the views it says so of. */
    slate: (s, only) => slates.push([s, only]),
    /** A pin with a label: { id, pos, text, title, colour, on, dim }. */
    pin: (p) => pins.push(p),
  };
  for (const k of everyKind()) {
    k.marks?.(out);
    // (A handle is its kind's: so whose it is can be told when it is dragged.)
    k.handles?.((h) => handles.push({ ...h, id: `${k.name}/${h.id}` }));
  }
  // (A colour of four is with how far it is seen through: a faint mark.)
  const lines = (list) => {
    const pos = new Float32Array(list.length * 6), col = new Uint8Array(list.length * 8);
    list.forEach(([a, b, rgb], i) => { const c = rgb.length > 3 ? rgb : [...rgb, 255]; pos.set(a, i * 6); pos.set(b, i * 6 + 3); col.set([...c, ...c], i * 8); });
    return [pos, col];
  };
  // (The lines every view has, made once; a view that has some of its own, with those.)
  const shared = segs.filter((s) => !s[3]), everywhere = lines(shared), own = segs.filter((s) => s[3]);
  for (const v of Object.values(views)) {
    const mine = own.filter((s) => s[3](v));
    v.setMarks(...(mine.length ? lines([...shared, ...mine]) : everywhere));
    const t = fills.filter((f) => f[4] == null || (typeof f[4] === "function" ? !f[4](v) : f[4] !== v.slab));
    const fp = new Float32Array(t.length * 9), fc = new Uint8Array(t.length * 12);
    t.forEach(([a, b, c, rgba], i) => { fp.set(a, i * 9); fp.set(b, i * 9 + 3); fp.set(c, i * 9 + 6); fc.set([...rgba, ...rgba, ...rgba], i * 12); });
    v.setFills(fp, fc);
    v.setSlates(slates.filter(([, only]) => !only || only(v)).map(([s]) => s));
    v.setLabels(pins);
    // (A handle may be of some views only: what is dragged in a view through a photo's camera.)
    v.setHandles(handles.filter((h) => !h.only || h.only(v)));
  }
}

/** Whether the cursor is in what four places on the screen go round. */
function inside(c, p) {
  let sign = 0;
  for (let i = 0; i < 4; i++) {
    const [a, b] = [p[i], p[(i + 1) % 4]], cross = (b[0] - a[0]) * (c.y - a[1]) - (b[1] - a[1]) * (c.x - a[0]);
    if (sign && cross * sign < 0) return false;
    if (cross) sign = cross;
  }
  return true;
}

/** The thing whose lines (as drawn: the shown ones) a click in a view is on, within a few pixels: { kind, id } of the nearest, or null. */
export function objectAt(v, e) {
  const c = cursorIn(v, e);
  let best = null;
  // (Of faces under the cursor, as much off it as each other, the nearest the viewer: `far`.)
  const take = (kind, id, at, far = 0) => { if (at && at.d < 7 && (!best || at.d < best.d || (at.d === best.d && far < best.far))) best = { kind, id, d: at.d, far }; };
  const eye = () => v.scene?.cameraState?.()?.at ?? [0, 0, 0];
  for (const k of everyKind()) k.near?.({
    /** A line of one of them, between two points (scan frame). */
    line: (id, a, b) => take(k.name, id, toStretch(c, a, b)),
    /** A run of points of one of them (closed: back to the first). */
    loop: (id, pts, closed) => take(k.name, id, toRun(c, pts, closed)),
    /** The face of one of them, four corners round it. (Only a line just under the cursor is nearer: not one that merely passes by, as a session's trajectory passes through each of its slates.) */
    area: (id, q) => { const p = q.map(c.on); if (p.every(Boolean) && inside(c, p)) { const e = eye(); take(k.name, id, { d: 0.5 }, Math.hypot(...[0, 1, 2].map((i) => (q[0][i] + q[2][i]) / 2 - e[i]))); } },
  }, v);
  return best;
}

/** A handle dragged in a view (`phase`: "start", "move", "end"; `ray`: through the cursor): its kind's to do. */
export function handleDrag(v, id, phase, ray, e) {
  const [kind, own] = id.split("/");
  kindOf(kind).drag?.(v, own, phase, ray, e);
}
/** The cursor is on a handle, or has left it: its kind's to show what it is a handle of. */
export function handleHover(v, id, on) {
  const [kind, own] = id.split("/");
  kindOf(kind).hover?.(v, own, on);
}
/** A handle right-clicked. */
export function handleMenu(v, id, e) {
  const [kind, own] = id.split("/");
  kindOf(kind).handleMenu?.(v, own, e);
}
