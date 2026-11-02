// What is drawn over a 3D view, by its scene:
//
//   - the skeleton: the structure's nodes (skeleton.js), marks at
//     points that say what they are where the cursor rests on them;
//   - the deflected shape of a frame analysis, exaggerated: lines over everything.
//
// Each view has them or not, for itself (`v.skeleton`, `v.deflected`); the view is the one thing
// moved: nothing here has a camera of its own.

import { $ } from "../ui.js";
import { vec } from "../glview.js";
import { model } from "../project.js";
import * as objects from "../objects.js";
import { active, toScan, views } from "./state.js";
import { modelBounds } from "./parts.js";
import { countSkeleton, hasSkeleton, hoverSkeleton, skeletonSet } from "./skeleton.js";
import { renderTools } from "./views.js";

// ----- the deflected shape ----------------------------------------------------------------

let deflected = null;   // of the combination chosen: { combination, max, members: [{ pts, disp }] }
/** Whether the model has a deflected shape to draw (a frame analysis, for the combination chosen). */
export const deflects = () => !!deflected?.members?.length;
/** The deflected shape as a view's scene draws it (`setOver`): each member's line, displaced, as segments in the
 *  scan's frame; null, where the view has none. */
function deflectedShape(v) {
  if (!v.deflected || !deflects()) return null;
  // Exaggerated: the slider's middle draws the largest displacement as about a tenth of the model's size.
  const size = modelBounds()?.radius ?? 1, k = ((0.1 * size) / (deflected.max || 1)) * Math.pow(Number($("deflectScale").value) / 50, 2);
  $("deflectInfo").textContent = `×${k >= 10 ? k.toFixed(0) : k.toFixed(1)} · ${(deflected.max * 1000).toFixed(2)} mm · ${deflected.combination}`;
  const segs = [];
  for (const m of deflected.members) {
    const q = m.pts.map((p, i) => toScan(vec.add(p, vec.scale(m.disp[i], k))));
    for (let i = 1; i < q.length; i++) segs.push(q[i - 1], q[i]);
  }
  const pos = new Float32Array(segs.flat()), col = new Uint8Array(segs.length * 4);
  for (let i = 0; i < segs.length; i++) col.set(DEFLECTED, i * 4);
  return { pos, col, width: 2 };
}
/** The deflected shape's colour (sRGB, opaque). */
const DEFLECTED = [214, 40, 40, 255];

// ----- the overlay ------------------------------------------------------------------------

let parts = null;   // the model's parts, as built now

/** What is drawn over the views is of the model as built now (and, deflected, of the combination chosen). */
async function loadOverlay() {
  parts = model.built ? await model.parts() : null;
  if (parts?.error) parts = null;
  deflected = model.built && model.state.analysis?.displacement ? await model.ask("deflected", { combo: $("heatCombo").value }) : null;
  countSkeleton(parts?.structure);
  for (const v of Object.values(views)) { renderTools(v); drawOverlay(v); }
  shapeOver();
}

/** Has each view's scene draw the deflected shape, where it has one, as it is now. */
export function shapeOver() {
  for (const v of Object.values(views)) v.setOver("deflected", deflectedShape(v));
}

/** Puts the keys of what is over the views (the skeleton's, the deflected shape's scale) with the view last used,
 *  shown where it has them. */
export function placeKeys(v) {
  if (v.name !== active) return;
  if ($("struct3d").parentNode !== v.pane || $("deflect3d").parentNode !== v.pane) v.pane.append($("struct3d"), $("deflect3d"));
  $("struct3d").hidden = !(v.skeleton && hasSkeleton());
  $("deflect3d").hidden = !(v.deflected && deflects());
}

/** Has a view's scene draw what is over it as it is now (every view: no view given): the skeleton, if it shows it. */
export function drawOverlay(v) {
  if (!v) { for (const each of Object.values(views)) drawOverlay(each); return; }
  placeKeys(v);
  const set = v.skeleton && hasSkeleton() && parts ? skeletonSet(parts.structure, toScan) : null;
  // (Sent again only where it has changed: it is made again as what is chosen changes, often for nothing.)
  const said = set && JSON.stringify(set);
  if (said === v.skeletonSaid) return;
  v.skeletonSaid = said;
  v.setMarkers("skeleton", set);
}

/** What the pointer did to a view's markers (`ev`, as `GLView.onMarker` says): the cursor on the skeleton's, or off,
 *  shows what that is free to do and what holds it, in every view. */
export function overlayMarker(ev) {
  if (ev.set === "skeleton" && ev.kind === "hover" && hoverSkeleton(ev.id)) drawOverlay();
}

/** Keeps what is drawn over the views of the model, the combination chosen, and what is hidden. */
export function initOverlay() {
  model.onChange(loadOverlay);
  $("heatCombo").addEventListener("change", loadOverlay);
  $("deflectScale").oninput = () => shapeOver();
  objects.on("visibility", () => drawOverlay());
}
