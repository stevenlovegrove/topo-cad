// Dragging a part script (docs/part-core.md, "Dragging"). In a view that picks nodes, a press on one grabs it; that
// picks faces, a flat face pressed with alt held is pushed and pulled along its normal; that picks parts, a part
// pressed with alt held is moved by the point of it pressed. A drag is said as a sighting is: what is grabbed on the
// cursor's ray, at the level dragged (below a param held in the Params panel and what was measured, above the hints).
// As the cursor moves, the engine settles the part with it (`model.grab`), and every view shows the bodies and nodes as
// they then are. On release it is kept beside the script, in place of a drag kept of the same thing (with shift held as
// it was pressed, beside them: two rays from two ways meet where the thing is), and it is built again; if the pointer
// is taken away, nothing is kept. A drag begins from the part as built; each move starts where the last was found,
// and a move made while the engine is still settling the last waits for it (only the latest is settled).

import { model } from "../project.js";
import { msg } from "../shell/panel.js";
import { choose } from "./chosen.js";
import { exactFor, exactIn, findExact, showSettling } from "./exact.js";
import { toModel, unrot, views } from "./state.js";

/** How far along the cursor's ray its far point is (millimetres): beyond anything a model has. */
const FAR = 1e6;

/** A view's node drags: in front of whatever else the view lets be grabbed (a photo's marks). */
export function bindGrab(v) {
  const other = { grab: v.onGrab, move: v.onGrabMove, end: v.onGrabEnd };
  let held = null;
  // The cursor's ray, in the model's frame, in millimetres: from where it starts, and far along it.
  const rayOf = (e) => {
    const r = v.canvas.getBoundingClientRect(), ray = v.ray(e.clientX - r.left, e.clientY - r.top);
    const from = toModel(ray.origin).map((x) => x * 1000), dir = unrot(ray.dir);
    return { from, to: from.map((x, k) => x + dir[k] * FAR) };
  };
  // (Only the latest move is settled: one waiting is replaced by the next. Of the drag it began with: let go while it
  // settles, the drag waits for it, and keeps what it found.)
  const settle = async (h = held) => {
    if (!h || h.busy || !h.next) return;
    const { from, to } = h.next;
    h.next = null;
    h.busy = true;
    const r = await model.grab(h.what, from, to, h.beside);
    h.busy = false;
    if (r.error) h.error = r.error;
    else { h.error = null; h.last = r; showSettling(r.bodies, r.params); }
    if (h.next) settle(h);
    else h.idle?.();
  };
  v.onGrab = (e) => {
    // (A node, on the ray; with alt held, a face, pushed and pulled along its normal, or a part, by the point of it
    // grabbed: so a press on a body still turns the view. Edges and vertices are chosen, not dragged.)
    if (exactIn(v) && (v.pickKind === "node" || (e.altKey && ["face", "part"].includes(v.pickKind)))) {
      const r = v.canvas.getBoundingClientRect(), n = findExact(v, e.clientX - r.left, e.clientY - r.top, v.pickKind === "node" ? "node" : "face");
      const what = !n ? null
        : v.pickKind === "node" ? { node: n.name }
        : v.pickKind === "face" ? (n.plane ? { face: { part: n.address, at: n.at, normal: n.plane.normal } } : null)
        : { point: { part: n.address, at: n.at } };
      if (what) {
        held = { what, beside: e.shiftKey, next: null, busy: false, last: null };
        if (v.pickKind !== "part") choose("bodypart", n);
        v.canvas.style.cursor = "grabbing";
        return true;
      }
    }
    held = null;
    return other.grab?.(e) ?? false;
  };
  v.onGrabMove = (e) => {
    if (!held) return other.move?.(e);
    held.next = rayOf(e);
    settle();
  };
  v.onGrabEnd = async (e, lost) => {
    if (!held) return other.end?.(e, lost);
    const h = held;
    held = null;
    v.canvas.style.cursor = "";
    if (h.busy) await new Promise((done) => (h.idle = done));
    if (h.error) msg(`It cannot be dragged so: ${h.error}`, true);
    // (Kept, and built again; nothing found (or lost): as built.)
    const w = await model.grabEnd(!lost && !!h.last);
    if (w.error) msg(w.error, true);
    if (w.file) lastDrag = w.drags ?? [];
    else for (const o of Object.values(views)) exactFor(o);
  };
}

/** The drags kept before the last drag was kept, to undo it. */
let lastDrag = null;
/** Whether there is a drag to undo. */
export const canUndoDrag = () => !!lastDrag;
/** Undoes the last drag: the drags kept before it kept again; built again. */
export async function undoDrag() {
  const was = lastDrag;
  lastDrag = null;
  if (!was) return;
  const w = await model.drags(was);
  if (w.error) msg(w.error, true);
}
