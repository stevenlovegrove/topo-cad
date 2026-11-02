// The skeleton of a model, drawn over a 3D view: its structure's nodes, each where members meet or end (on their
// axes), by the address of the model's node there, and whether it is held there.

import { $, store, stored } from "../ui.js";

// What of it is shown, by its key's boxes (a view shows it or not, for itself); kept.
const skel = { axis: true, ...stored("topo.skeleton", {}) };
/** `changed()` whenever what of it is shown is changed in its key. */
export function bindSkeleton(changed) {
  $("struct3d").addEventListener("change", (e) => {
    if (!e.target.dataset.k) return;
    skel[e.target.dataset.k] = e.target.checked;
    store("topo.skeleton", skel);
    changed();
  });
}
/** What of it the cursor is over (its key among those drawn). */
let hoverSkel = null;

let has = false;
/** Whether the model as built has a skeleton to show. */
export const hasSkeleton = () => has;
/** Says in the panel how many nodes a model's skeleton has (none: it has none). */
export function countSkeleton(S) {
  has = !!S;
  if (!S) return;
  for (const c of $("struct3d").querySelectorAll("input[data-k]")) c.checked = !!skel[c.dataset.k];
  $("skelAxisN").textContent = `(${S.nodes.length})`;
}

function nodeTip(n) {
  const lines = [`Node ${n.id}${n.name ? ` "${n.name}"` : ""}${n.support ? " · held" : ""}`];
  if (n.on_axis.length) lines.push("on the axis of (in the path of):", ...n.on_axis.map((m) => `   ${m}`));
  return lines.join("\n");
}

/** A colour (`#rrggbb`) as the scene takes it: sRGB bytes, with how opaque (`a`, a share). */
const rgba = (hex, a = 1) => [1, 3, 5].map((k) => parseInt(hex.slice(k, k + 2), 16)).concat(Math.round(a * 255));

/**
 * A skeleton `S` as marks for a view's scene to draw over what it shows (`GLView.setMarkers`): its nodes, each saying
 * what it is where the cursor rests on it. Points in the scan's frame, by `toScan`.
 */
export function skeletonSet(S, toScan) {
  const markers = [];
  if (skel.axis) for (const n of S.nodes) {
    const key = `n:${n.id}`, hot = hoverSkel === key;
    markers.push({ id: key, at: toScan(n.pos), shape: "dot", size: hot ? 9 : n.on_axis.length > 1 ? 5.2 : 3.6, fill: rgba(hot ? "#111111" : n.on_axis.length > 1 ? "#444444" : "#999999"), tip: nodeTip(n), rank: 3 });
  }
  return { markers, lines: [], regions: [] };
}

/** The cursor is on a marker of the skeleton (`id`), or none: whether that changes what is drawn. */
export function hoverSkeleton(id) {
  if ((id ?? null) === hoverSkel) return false;
  hoverSkel = id ?? null;
  return true;
}
