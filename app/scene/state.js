// What the scene's modules share: the workspace's state as last said, the model's place in the
// scan, the 3D views, and what is chosen. Whoever changes one of these does it here; the rest
// read them as they are.

import { vec } from "../glview.js";
import { EMPTY, workspace } from "../workspace.js";

/**
 * The workspace's state (workspace.js), as last said; till it has said anything, one with nothing in it. The
 * model's parts are drawn, and can be chosen, before a workspace that a service keeps has said its first word
 * (which may be some seconds): whoever reads this then finds no issues, no planes, nobody, and not nothing at all.
 */
export let st = EMPTY;
/** Where the model is in the scan (model → scan: a turn about the vertical, then a shift). Without a scan: one frame. */
export let pl = { yaw: 0, t: [0, 0, 0] };
export function setState(s) {
  st = s;
  pl = s.placement ? { yaw: (s.placement.yaw_deg * Math.PI) / 180, t: s.placement.t } : { yaw: 0, t: [0, 0, 0] };
}

/** Whether the project has a scan (its points, and what they say); whether its workspace is shared (others, issues, readings). */
export const scanned = () => workspace.can("scan"), shared = () => workspace.can("shared");

/** The 3D views that are open, by id: a GLView each (views.js: with `slab`, the id of the slab it is clipped to, and what else a view has). */
export const views = {};
/** The view last used (its id; none: none is open): what a link or a "go to" is shown in. */
export let active = null;
export const setActive = (id) => { active = id; };

/**
 * What is chosen: one of `pick` (a point: what a ray met), `member` (a part's path; `at`: where on it), `selection`,
 * `plane`, `slab`, `polyline` (ids): see chosen.js. And beside it: the `issue` open, the `proposal` shown
 * ({ issue, n, commit, mode }), the part under the cursor (`hover`), parts to `highlight`, who is chatted with.
 */
export const sel = { issue: null, proposal: null, pick: null, filter: "all", done: false, drafting: false };
/** A slab or polyline as it is dragged, ahead of the workspace's state: { kind, id, value, done (the workspace has been told) }. */
export let live = null;
export const setLive = (value) => (live = value);

// The model's frame and the scan's (the views are in the scan's).
export const toScan = (p) => { const c = Math.cos(pl.yaw), s = Math.sin(pl.yaw); return [c * p[0] - s * p[1] + pl.t[0], s * p[0] + c * p[1] + pl.t[1], p[2] + pl.t[2]]; };
export const toModel = (p) => { const q = vec.sub(p, pl.t), c = Math.cos(pl.yaw), s = Math.sin(pl.yaw); return [c * q[0] + s * q[1], -s * q[0] + c * q[1], q[2]]; };
/** A direction of the model, in the scan; and of the scan, in the model. */
/**
 * What an issue's place is pinned to: { kind, id } (a part of the model, a selection, a plane, a slab, a
 * polyline), or null. (A service that says it the way it was once kept, a field for each kind, is read too.)
 */
export const pinOf = (place) => place?.on ?? (place?.member ? { kind: "member", id: place.member } : place?.selection ? { kind: "selection", id: place.selection } : place?.plane ? { kind: "plane", id: place.plane } : place?.object ? { kind: place.object[0] === "b" ? "slab" : "polyline", id: place.object } : null);
/** The id of what a place is pinned to, where that is of this kind. */
export const pinnedTo = (place, kind) => { const on = pinOf(place); return on?.kind === kind ? on.id : null; };
export const rot = (n) => { const c = Math.cos(pl.yaw), s = Math.sin(pl.yaw); return [c * n[0] - s * n[1], s * n[0] + c * n[1], n[2]]; };
export const unrot = (n) => { const c = Math.cos(pl.yaw), s = Math.sin(pl.yaw); return [c * n[0] + s * n[1], -s * n[0] + c * n[1], n[2]]; };

export const f3 = (p) => `[${p.map((x) => x.toFixed(2)).join(", ")}]`;
export const round = (p, k = 1000) => p.map((x) => Math.round(x * k) / k);
/** A part's path, short: its group and its name. */
export const shortPath = (p) => p.split("/").slice(-2).join("/");
