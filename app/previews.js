// What each project looks like: a picture of its 3D view, taken as it rests after its model is built (opened, saved,
// changed), kept in this browser (whichever store the projects are kept in, and read-only ones too) for the projects
// page to show. Of each project, the last taken.

import { idb } from "./store.js";
import { project, storage } from "./project.js";

const db = idb("topo-cad-previews", "shots");
/** The size a picture is kept at (CSS pixels of a tile, twice over). */
const SHOT = { w: 480, h: 360 };
/** How long after the model is built, at the least, its view is pictured: its exact lines are found by then. */
export const SETTLE_MS = 1500;
/** How often, at most, a project is pictured again: a drag builds it again many times. */
const EVERY_MS = 5000;

/** Where a project's picture is kept: of the store it is in (`store`: the page's), by its model's id. */
const keyOf = (path, store = storage.store) => `${store?.label ?? ""}\n${path}`;
let last = 0;

/** How long till the open project can be pictured again (0: now). */
export const untilShot = () => Math.max(0, EVERY_MS - (Date.now() - last));

/**
 * Takes the picture of the open project from a view's canvas, just drawn (in the same task: a WebGPU canvas's picture
 * is gone once the task is over): the middle of it, as a tile is shaped, made small; kept.
 */
export function shoot(canvas) {
  const path = project.current(), now = Date.now();
  if (!path || !canvas.width || !canvas.height) return;
  last = now;
  const out = document.createElement("canvas");
  out.width = SHOT.w;
  out.height = SHOT.h;
  // (As much of it as is the tile's shape, about its middle.)
  const k = Math.min(canvas.width / SHOT.w, canvas.height / SHOT.h), w = SHOT.w * k, h = SHOT.h * k;
  out.getContext("2d").drawImage(canvas, (canvas.width - w) / 2, (canvas.height - h) / 2, w, h, 0, 0, SHOT.w, SHOT.h);
  out.toBlob((blob) => { if (blob) db.set(keyOf(path), { blob, at: now }).catch(() => {}); }, "image/webp", 0.82);
}

/** The picture kept of a project (by its model's id, in a location), as a URL to show it by (let go of with
 *  `URL.revokeObjectURL`); null where none is. */
export async function previewOf(path, kind) {
  const store = storage.of(kind);
  if (!store) return null;
  const got = await db.get(keyOf(path, store)).catch(() => null);
  return got?.blob ? URL.createObjectURL(got.blob) : null;
}
