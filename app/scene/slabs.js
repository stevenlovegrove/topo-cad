import { show, vis } from "../objects.js";
import { kind } from "./kinds.js";
import { BOX_EDGES, BOX_FACES, alongLine, boxAbout, onPlane } from "./geom.js";
import { bring, isShown, openDoc, setInfo } from "../shell/dock.js";
import { homeMenu, homeOf, homeSeen, madeIn, rehome } from "./homes.js";
// Slabs: oriented boxes in the model's frame ({ centre, axes: [right, up, toward the viewer],
// half }: a box from -half to half in a frame placed there), kept with the project. A view clipped
// to one is cut to it as it is, on the GPU (its points, the model's bodies, which show their
// sections where it cuts them open), and is its section: every scan point in it is fetched and
// drawn, again when it changes. The ones here that are not kept (`temp`) come from links' sections,
// and can be kept; or cut a view to heights (`filter`: its roof and floor cut away), and are no
// section, only a box the view is cut to.

import { until } from "../ui.js";
import { vec } from "../glview.js";
import { workspace } from "../workspace.js";
import { f3, live, rot, round, sel, setActive, setLive, st, toModel, toScan, unrot, views } from "./state.js";
import { choose, renderChips } from "./chosen.js";
import { goTo, pushHistory } from "./links.js";
import { rebuildMarks } from "./marks.js";
import { modelBounds, rebuildModel } from "./parts.js";
import { requestSlab } from "./points.js";
import { renderLayers, renderTools, theView, viewKey } from "./views.js";

/** Slabs not kept: made by following a link's section. */
const temps = [];
let tempN = 0;

export const slabs = () => [...(st?.slabs || []), ...temps].map((s) => (live?.kind === "slab" && live.id === s.id ? live.value : s));
export const slabOf = (id) => (id && slabs().find((s) => s.id === id)) || null;
export const slabName = (s) => (s.filter ? "heights shown (not kept)" : s.temp ? `${s.id} (not kept)` : `${s.id}${s.name ? " " + s.name : ""}`);
/** A slab as a view clips to it: scan frame. */
export const boxOf = (s) => ({ c: toScan(s.centre), u: rot(s.axes[0]), v: rot(s.axes[1]), n: rot(s.axes[2]), h: s.half });
const slabCorners = (s) => [0, 1, 2, 3, 4, 5, 6, 7].map((k) => toScan([0, 1, 2].reduce((p, a) => vec.add(p, vec.scale(s.axes[a], (k >> a) & 1 ? s.half[a] : -s.half[a])), s.centre)));

/** A slab's axes from the direction it is seen along (toward the viewer), as a served workspace makes them: upright, and square to the model when within 5° of it. */
function axesFromNormal(n, up) {
  n = vec.unit(n);
  const k = [0, 1, 2].find((a) => Math.abs(n[a]) > 0.9962);
  if (k != null) n = [0, 1, 2].map((a) => (a === k ? Math.sign(n[k]) : 0));
  up ??= Math.abs(n[2]) > 0.9 ? [0, 1, 0] : [0, 0, 1];
  const v = vec.unit(vec.sub(up, vec.scale(n, vec.dot(up, n))));
  return [vec.cross(v, n), v, n];
}

/** A slab through `origin` with these axes, reaching over the model (and a little more) across and up. */
function slabOver(origin, axes, thickness) {
  const b = modelBounds() ?? { lo: vec.sub(origin, [4, 4, 1.5]), hi: vec.add(origin, [4, 4, 1.5]) };
  let [u0, u1, v0, v1] = [Infinity, -Infinity, Infinity, -Infinity];
  for (const x of [b.lo[0], b.hi[0]]) for (const y of [b.lo[1], b.hi[1]]) for (const z of [b.lo[2], b.hi[2]]) {
    const q = vec.sub([x, y, z], origin);
    u0 = Math.min(u0, vec.dot(q, axes[0])); u1 = Math.max(u1, vec.dot(q, axes[0])); v0 = Math.min(v0, vec.dot(q, axes[1])); v1 = Math.max(v1, vec.dot(q, axes[1]));
  }
  return { centre: vec.add(origin, vec.add(vec.scale(axes[0], 0.5 * (u0 + u1)), vec.scale(axes[1], 0.5 * (v0 + v1)))), axes, half: [0.5 * (u1 - u0) + 0.4, 0.5 * (v1 - v0) + 0.4, 0.5 * thickness] };
}

/** A slab from a link's section (as `render_section` takes it, model frame). */
export function slabFromSpec(spec) {
  // (Origins as `render_section` has them: its `bounds` are measured from them.)
  const thick = spec.thickness;
  let origin, n, up, t;
  if (spec.view === "x") [origin, n, up, t] = [[spec.at ?? 0, 0, 0], [1, 0, 0], [0, 0, 1], thick ?? 0.3];
  else if (spec.view === "y") [origin, n, up, t] = [[0, spec.at ?? 0, 0], [0, -1, 0], [0, 0, 1], thick ?? 0.3];
  else if (spec.view === "custom") [origin, n, up, t] = [spec.origin, spec.normal, spec.up ?? [0, 0, 1], thick ?? 0.3];
  else { const z = spec.z ?? [0.1, 2.4]; [origin, n, up, t] = [[0, 0, 0.5 * (z[0] + z[1])], [0, 0, 1], [0, 1, 0], Math.abs(z[1] - z[0])]; }
  n = vec.unit(n);
  let v = vec.sub(up, vec.scale(n, vec.dot(up, n)));
  if (Math.hypot(...v) < 1e-6) v = Math.abs(n[0]) < 0.9 ? vec.cross(n, [1, 0, 0]) : vec.cross(n, [0, 1, 0]);
  v = vec.unit(v);
  const axes = [vec.cross(v, n), v, n], b = spec.bounds;
  if (!b) return slabOver(origin, axes, t);
  return { centre: vec.add(origin, vec.add(vec.scale(axes[0], 0.5 * (b[0] + b[1])), vec.scale(v, 0.5 * (b[2] + b[3])))), axes, half: [0.5 * (b[1] - b[0]), 0.5 * (b[3] - b[2]), 0.5 * t] };
}

/** A slab that is not kept (until asked): the same one again for the same place. */
export function tempSlab(geom) {
  const same = temps.find((t) => JSON.stringify([t.centre, t.axes, t.half]) === JSON.stringify([geom.centre, geom.axes, geom.half]));
  if (same) return same;
  const s = { id: `t${++tempN}`, name: "", temp: true, by: workspace.id, ...geom };
  temps.push(s);
  // A few are enough: the oldest that no view is clipped to goes.
  while (temps.length > 4) { const k = temps.findIndex((t) => !Object.values(views).some((v) => v.slab === t.id)); if (k < 0) break; temps.splice(k, 1); }
  return s;
}

/** Whether a slab is a plan: seen from above or below. */
const isPlan = (s) => Math.abs(s.axes[2][2]) > 1 - 1e-9;
/** A plan's bottom and top (m, the model's frame). */
const heightsOf = (s) => [s.centre[2] - s.half[2], s.centre[2] + s.half[2]];
/** The heights a view clipped to a slab is cut to (m, the model's frame), each way: null where it cuts nothing that way.
 *  A plan's faces; but of one `cutHeights` made, what it said, while the slab is as it left it. */
const cutsOf = (s) =>
  !s || !isPlan(s) ? { bottom: null, top: null } : s.temp && s.cuts?.at === JSON.stringify([s.centre, s.half]) ? s.cuts : { bottom: heightsOf(s)[0], top: heightsOf(s)[1] };
/** How far clear of the roof's lowest part, and over the floor's top, the roof and the floor are cut away: enough that
 *  the scan's points on them go too. Said where the cuts are offered; a drag of the slab's face moves either. */
export const CLEAR = 0.1;

/** What a view holds, in the model's frame: the model's parts and the points it draws. */
function heldBy(v) {
  const b = modelBounds(), lo = b ? [...b.lo] : [Infinity, Infinity, Infinity], hi = b ? [...b.hi] : [-Infinity, -Infinity, -Infinity];
  for (const e of Object.values(v.extents ?? {})) {
    if (!e) continue;
    for (let k = 0; k < 8; k++) {
      const q = toModel([0, 1, 2].map((a) => ((k >> a) & 1 ? e.hi[a] : e.lo[a])));
      for (let a = 0; a < 3; a++) { lo[a] = Math.min(lo[a], q[a]); hi[a] = Math.max(hi[a], q[a]); }
    }
  }
  return lo[0] === Infinity ? null : { lo, hi };
}

/**
 * Cuts a view to heights: what is under `bottom` and over `top` taken away (m, the model's frame; null: nothing that
 * way; left out: as it is). By a box in the model's frame over all the view holds, which only cuts the view (no
 * section: its points are not fetched): the view's own where it has one, else one made (Back returns to what it was
 * clipped to). Chosen, so its faces can be dragged. Where it then cuts nothing, the view is clipped to nothing.
 */
export function cutHeights(v, cut) {
  const all = heldBy(v);
  if (!all) return;
  const s = slabOf(v.slab), mine = !!s?.filter, as = cutsOf(s);
  const want = { bottom: cut.bottom === undefined ? as.bottom : cut.bottom, top: cut.top === undefined ? as.top : cut.top };
  if (want.bottom == null && want.top == null) {
    if (!mine) return;
    v.slab = null;
    temps.splice(temps.indexOf(s), 1);
    if (sel.slab === s.id) choose(null);
    syncSlabs(); renderLayers(); rebuildMarks();
    return;
  }
  const bottom = want.bottom ?? all.lo[2], top = want.top ?? all.hi[2];
  if (top <= bottom) return;
  const centre = [0.5 * (all.lo[0] + all.hi[0]), 0.5 * (all.lo[1] + all.hi[1]), 0.5 * (bottom + top)], half = [0.5 * (all.hi[0] - all.lo[0]), 0.5 * (all.hi[1] - all.lo[1]), 0.5 * (top - bottom)];
  const geom = { centre, axes: [[1, 0, 0], [0, 1, 0], [0, 0, 1]], half, filter: true, cuts: { ...want, at: JSON.stringify([centre, half]) } };
  if (mine) return updateSlab(s.id, geom);
  cutTo(v, geom);
}

/** A view cut to a box of its own (a `filter` one), chosen so its faces can be dragged (Back returns). */
function cutTo(v, geom) {
  pushHistory(v);
  const t = tempSlab(geom);
  v.slab = t.id;
  choose("slab", t.id);
  syncSlabs(); renderLayers(); rebuildMarks();
}

/** Whether a view is cut to a box of its own (its heights). */
export const heightsCut = (v) => !!slabOf(v.slab)?.filter;

/**
 * The roof taken away, or put back: a view's own box, on and off. Off, the box is kept with the view as it is (its
 * faces as dragged); on again, it is as it was; where the view has none, one is made whose top is `CLEAR` under the
 * roof's lowest part (`roof`, m: none, nothing is made). Deleting the box lets go of it: the next is made anew.
 */
export function toggleHeights(v, roof) {
  const s = slabOf(v.slab);
  if (s?.filter) {
    v.heightsKept = { centre: [...s.centre], axes: s.axes.map((a) => [...a]), half: [...s.half], filter: true, ...(s.cuts ? { cuts: s.cuts } : {}) };
    return cutHeights(v, { bottom: null, top: null });
  }
  if (v.heightsKept) return cutTo(v, v.heightsKept);
  if (roof != null) cutHeights(v, { top: roof - CLEAR });
}

/** Keeps a slab with the project; its id. */
async function createSlab(args) {
  const r = await workspace.call("slabs", { action: "create", frame: "model", ...madeIn(), ...args });
  if (r.error) { alert(r.error); return null; }
  const id = r.text.match(/made slab (b\d+)/)?.[1];
  if (!id) return null;
  show(`slab:${id}`, "slabs");
  await until(() => st?.slabs?.some((s) => s.id === id));
  return id;
}

/** Changes a slab (its centre, half sizes, axes or name): at once here, then with the workspace. */
export async function updateSlab(id, patch) {
  const s = slabOf(id);
  if (!s) return;
  if (s.temp) Object.assign(temps.find((t) => t.id === id), patch);
  else setLive({ kind: "slab", id, value: { ...s, ...patch } });
  syncSlabs(); rebuildMarks(); renderChips(); renderLayers();
  if (s.temp) return;
  const mine = live, r = await workspace.call("slabs", { action: "update", id, ...patch });
  if (r.error) alert(r.error);
  // (Let go of once the workspace's state says the same.)
  if (live === mine) live.done = true;
}

/** Makes every view's clipping box what its slab is now, and asks for the points in it. */
export function syncSlabs() {
  let changed = false;
  for (const v of Object.values(views)) {
    const s = slabOf(v.slab);
    if (v.slab && !s) v.slab = null;
    const box = s ? boxOf(s) : null;
    if (JSON.stringify(box) !== JSON.stringify(v.state.box)) changed = true;
    if (JSON.stringify(box) !== JSON.stringify(v.state.box)) v.set(box ? { box } : { box: null, flat: false }, true);
    requestSlab(v);
  }
  // (The members are drawn as the slab cuts them.)
  if (changed) rebuildModel();
}

/**
 * Shows a slab: a 3D view clipped to it, seen square on. In the view given; else the view already on it; else
 * the view last clipped to another (the one sections are looked at in); else a view of its own, beside what is open.
 */
export function viewSlab(id, v) {
  const s = slabOf(id);
  if (!s) return null;
  const all = Object.values(views);
  v ??= all.find((x) => x.slab === id) ?? all.find((x) => x.slab) ?? openDoc(viewKey(true), { beside: true }).view;
  bring(v.key);
  if (v.slab !== id) pushHistory(v);
  setActive(v.name);
  v.slab = id;
  syncSlabs();
  v.squareOn(boxOf(s));
  renderLayers(); rebuildMarks();
  return v;
}

/** A view no longer clipped to a slab: it shows everything (Back returns). */
export function leaveSlab(v) {
  if (!v?.slab) return;
  pushHistory(v);
  v.slab = null;
  syncSlabs(); renderLayers(); rebuildMarks(); renderTools(v);
}

/** A new slab through what a view is looking at (`how`: "plan", "x" or "y"), kept, chosen and viewed in it. */
async function newSlabIn(v, how) {
  const at = toModel(v.lookingAt());
  const g = slabOver(at, axesFromNormal(how === "plan" ? [0, 0, 1] : how === "x" ? [1, 0, 0] : [0, -1, 0]), how === "plan" ? 0.1 : 0.2);
  const made = await createSlab({ centre: round(g.centre), axes: g.axes, half: round(g.half) });
  if (!made) return;
  choose("slab", made);
  viewSlab(made, v);
}

/** A line drawn across a view (looking down on it): an upright slab along it, to see what it cuts through. */
export async function cutAlong(v, ra, rb) {
  v.tool = null;
  renderTools(v);
  const z = v.lookingAt()[2], a = onPlane([0, 0, z], [0, 0, 1], ra), b = onPlane([0, 0, z], [0, 0, 1], rb);
  if (!a || !b) return setInfo(v.key, "Draw a cut line looking down on the building.");
  const [p, q] = [toModel(a), toModel(b)], d = [q[0] - p[0], q[1] - p[1]], len = Math.hypot(...d);
  if (len < 0.05) return;
  const axes = axesFromNormal([d[1] / len, -d[0] / len, 0]), mb = modelBounds(), zs = mb ? [mb.lo[2] - 0.3, mb.hi[2] + 0.3] : [toModel(a)[2] - 1.5, toModel(a)[2] + 1.5];
  const id = await createSlab({ centre: round([0.5 * (p[0] + q[0]), 0.5 * (p[1] + q[1]), 0.5 * (zs[0] + zs[1])]), axes, half: round([0.5 * len, 0.5 * (zs[1] - zs[0]), 0.1]) });
  if (id) lookThrough(id);
}

/** The surface at a picked point: as the scan has it there, else the face of the member it lies on. */
export const surfaceAt = (p) => p.normal ?? (p.member && p.member.distance < 0.06 ? p.member.normal : null);
/** A slab through a picked point `p`: a plan at its height, a cut across the model's x or y, along the surface there, or through it. */
export async function slabAt(how, p, v) {
  const nrm = surfaceAt(p);
  let n = how === "plan" ? [0, 0, 1] : how === "x" ? [1, 0, 0] : how === "y" ? [0, -1, 0] : nrm;
  if (how === "along" && v) {
    // Seen from the side the viewer is on.
    if (vec.dot(n, unrot(v.camera().forward)) > 0) n = vec.scale(n, -1);
  } else if (how === "through") {
    const h = Math.hypot(nrm[0], nrm[1]);
    n = h < 0.3 ? [1, 0, 0] : vec.cross([0, 0, 1], [nrm[0] / h, nrm[1] / h, 0]);
  }
  const g = slabOver(p.model, axesFromNormal(n), how === "plan" ? 0.1 : 0.2);
  const id = await createSlab({ centre: round(g.centre), axes: g.axes, half: round(g.half) });
  if (id) lookThrough(id);
}

/** A slab made by the workspace (on a plane, around a selection), selected and shown. */
export async function slabFrom(args) {
  const id = await createSlab(args);
  if (id) lookThrough(id);
}
const deleteSlab = async (id) => {
  const k = temps.findIndex((t) => t.id === id);
  // (A view's own box deleted: what it kept of it goes too, so the roof's button makes one anew.)
  if (k >= 0 && temps[k].filter) for (const v of Object.values(views)) if (v.slab === id) delete v.heightsKept;
  if (k >= 0) temps.splice(k, 1); else await workspace.call("slabs", { action: "delete", id });
  if (sel.slab === id) choose(null);
  syncSlabs(); renderLayers(); rebuildMarks();
  for (const v of Object.values(views)) renderTools(v);
};
/** Keeps a slab that came from a link, where the views on it stay. */
async function keepSlab(id) {
  const t = slabOf(id);
  const kept = t && (await createSlab({ centre: t.centre, axes: t.axes, half: t.half }));
  if (!kept) return;
  // (Kept on purpose: the project's, where it is not the open issue's.)
  if (!("issue" in madeIn())) await rehome("slab", kept, "project");
  for (const v of Object.values(views)) if (v.slab === id) v.slab = kept;
  temps.splice(temps.findIndex((x) => x.id === id), 1);
  choose("slab", kept);
  syncSlabs(); renderLayers();
}

/** Looks through a slab: it is chosen, and a view is clipped to it, seen square on (Back returns). */
function lookThrough(id) {
  if (!slabOf(id)) return;
  choose("slab", id);
  viewSlab(id);
}

/** Whether a slab is drawn: its eye and its group's are on, and its home is in sight (homes.js). */
const seen = (s) => vis("slabs", `slab:${s.id}`) && homeSeen(s, s.id === sel.slab);

// What a slab is to the tree, the card, the menus and the views (kinds.js).
/** The slab as it was when a handle of it was taken hold of: { s, moved }. */
let grab = null;
kind("slab", {
  field: "slab", group: "slabs", order: 50,
  key: (id) => `slab:${id}`,
  find: slabOf,
  title: (s) => `slab ${slabName(s)}`,
  // (A box that only cuts a view to heights is the view's, not one of the slabs.)
  all: () => slabs().filter((s) => !s.filter),
  rowOf: (row, s, depth, opts) => row(depth, "slab", `slab:${s.id}`, slabName(s), { ...opts, id: s.id, on: s.id === sel.slab, note: `${(2 * s.half[0]).toFixed(1)} × ${(2 * s.half[1]).toFixed(1)} × ${(2 * s.half[2]).toFixed(2)}${Object.values(views).some((v) => v.slab === s.id && isShown(v.key)) ? " · in view" : ""}` }),
  // (Its group is of the project's; an issue's are with the issue, and the rest under "Unsorted": homes.js.)
  rows(row, list) { const mine = slabs().filter((s) => homeOf(s) === "project"); return list("slabs", "Slabs", () => mine.map((s) => this.rowOf(row, s, 1)), { note: mine.length || "right-click: new slab", title: "Slabs: boxes a 3D view can be clipped to, to see a section. Double-click one to look through it; right-click here for a new one" }); },
  // (A new one through what the view last used looks at.)
  groupMenu(add) {
    add("New slab: plan, through what the view looks at", () => newSlabIn(theView(), "plan"));
    add("New slab: across x", () => newSlabIn(theView(), "x"));
    add("New slab: across y", () => newSlabIn(theView(), "y"));
  },
  // (Those that are only this page's, from links, are cleared with what is scratch.)
  clear: () => { temps.length = 0; syncSlabs(); },
  where: (s) => (s.temp ? [] : [{ key: "slab", label: `Slab ${slabName(s)}` }]),
  refer: (s) => (s.temp ? null : s.id),
  // (A slab is gone to by looking through it.)
  goto: lookThrough,
  points: slabCorners,
  frame: (s) => ({ origin: toScan(s.centre), axes: s.axes.map(rot) }),
  info: (s) => ({
    what: s.filter ? "The heights a 3D view shows: a box it is cut to" : "Slab: a box a 3D view is clipped to, to see a section", name: s.filter ? "Heights shown" : `Slab ${slabName(s)}`,
    rows: [["Thickness", `${(2 * s.half[2]).toFixed(3)} m`], ["Looks along", f3(s.axes[2])], ["Shown in", Object.values(views).filter((v) => v.slab === s.id).length ? "a view" : "no view"]],
    note: s.filter ? "It is this view's only: keep it, to have it with the project as a slab (a section, its points fetched)." : s.temp ? "It is this page's only (from a link): keep it, to have it with the project." : "",
  }),
  menu(id, m) {
    const s = slabOf(id);
    const viewing = Object.values(views).filter((v) => v.slab === id);
    if (viewing.length) m.add("Leave it (its view shows everything again)", () => { for (const v of viewing) leaveSlab(v); });
    else m.add("View it (a 3D view clipped to it)", () => lookThrough(id));
    if (s.temp) m.add("Keep it", () => keepSlab(id));
    else { m.issue("New issue on this slab"); m.name(); }
    m.add("Go to it in this view", () => { choose("slab", id); goTo(boxAbout(slabCorners(s)), 0.6); });
    if (!s.temp) m.showHide();
    homeMenu(m, "slab", s, id);
    m.divider();
    m.add("Delete it", () => deleteSlab(id), "danger");
  },
  card(s) {
    const dim = (k, title) => `<input type="number" step="0.05" min="0.01" data-dim="${k}" value="${(2 * s.half[k]).toFixed(2)}" title="${title}">`;
    // (A plan's heights, its bottom and its top, in the model's frame: each set alone, the other kept.)
    const [lo, hi] = heightsOf(s);
    const heights = isPlan(s) ? `<br><span title="Its bottom and its top (m, the model's heights): what of the model is between them is shown">Heights <input type="number" step="0.05" data-z="0" value="${lo.toFixed(2)}"> to <input type="number" step="0.05" data-z="1" value="${hi.toFixed(2)}"> m</span>` : "";
    return {
      label: s.filter ? "Heights shown" : "Slab", id: s.id, name: s.temp ? null : { text: s.name, save: (t) => updateSlab(s.id, { name: t }) }, note: "not kept",
      body: `<span class="dims">${dim(0, "Its width (m)")} × ${dim(1, "Its height (m)")} × ${dim(2, "Its thickness (m)")} m</span> · seen from ${f3(s.axes[2])}${heights}<br>Drag its faces to size it, its middle to slide it, ⟳ to turn it.`,
      bind(card) {
        for (const i of card.querySelectorAll("input[data-dim], input[data-z]")) i.onkeydown = (e) => { e.stopPropagation(); if (e.key === "Enter") i.blur(); };
        for (const i of card.querySelectorAll("input[data-dim]")) {
          i.onchange = () => { const half = s.half.slice(); if (Number(i.value) > 0) { half[Number(i.dataset.dim)] = Number(i.value) / 2; updateSlab(s.id, { half }); } };
        }
        for (const i of card.querySelectorAll("input[data-z]")) {
          i.onchange = () => {
            const z = heightsOf(slabOf(s.id)), k = Number(i.dataset.z), v = Number(i.value);
            if (!Number.isFinite(v)) return;
            z[k] = v;
            if (z[1] <= z[0]) return;
            const c = slabOf(s.id).centre.slice(), half = slabOf(s.id).half.slice();
            c[2] = (z[0] + z[1]) / 2; half[2] = (z[1] - z[0]) / 2;
            updateSlab(s.id, { centre: c, half });
          };
        }
      },
    };
  },
  marks(out) {
    for (const s of slabs()) {
      if (!seen(s)) continue;
      const on = s.id === sel.slab, c = slabCorners(s);
      out.box(c, on ? [255, 205, 120] : [205, 120, 30]);
      // The one chosen has faint faces too, so it is seen where it meets the points (stronger while it is dragged).
      if (on) for (const f of BOX_FACES) out.fill(f.map((k) => c[k]), [255, 160, 50, grab ? 54 : 26], s.id);
    }
  },
  near(test) {
    for (const s of slabs()) {
      if (!seen(s)) continue;
      const c = slabCorners(s);
      for (const [a, b] of BOX_EDGES) test.line(s.id, c[a], c[b]);
    }
  },
  // The one chosen: its faces, its middle, and a handle to turn it by. (What is hidden has no handles either.)
  handles(add) {
    const s = slabOf(sel.slab);
    if (!s || !seen(s)) return;
    const c = toScan(s.centre), ax = s.axes.map(rot), names = ["width", "height", "thickness"];
    for (let a = 0; a < 3; a++) for (const g of [-1, 1]) add({ id: `f:${a}:${g}`, pos: vec.add(c, vec.scale(ax[a], g * s.half[a])), along: ax[a], cls: a === 2 ? "face thin" : "face", title: `Drag this face in or out (the slab's ${names[a]})` });
    add({ id: "mid", pos: c, along: ax[2], cls: "mid", title: "Drag to slide the slab through the building, along its thin direction" });
    add({ id: "yaw", pos: vec.add(c, vec.scale(ax[0], s.half[0] + 0.45)), cls: "yaw", text: "⟳", title: "Drag to turn the slab about the vertical (in steps of 5° of the model; alt: freely)" });
  },
  drag(v, id, phase, ray, e) {
    if (phase === "start") { grab = { s: structuredClone(slabOf(sel.slab)) }; return; }
    if (!grab) return;
    const s0 = grab.s, c0 = toScan(s0.centre);
    let value = null;
    if (id.startsWith("f:")) {
      // A face moves along its normal; the one across from it stays where it is.
      const [, a, g] = id.split(":").map(Number), ax = rot(s0.axes[a]);
      const t = alongLine(vec.add(c0, vec.scale(ax, g * s0.half[a])), vec.scale(ax, g), ray);
      if (t != null) {
        const h = Math.max(0.01, s0.half[a] + 0.5 * t), half = s0.half.slice();
        half[a] = Math.round(h * 1000) / 1000;
        value = { ...s0, half, centre: vec.add(s0.centre, vec.scale(s0.axes[a], g * (half[a] - s0.half[a]))) };
      }
    } else if (id === "mid") {
      const t = alongLine(c0, rot(s0.axes[2]), ray);
      if (t != null) value = { ...s0, centre: vec.add(s0.centre, vec.scale(s0.axes[2], t)) };
    } else if (id === "yaw") {
      const p = onPlane(c0, [0, 0, 1], ray);
      if (p) {
        const q = toModel(p), u = s0.axes[0];
        let to = Math.atan2(q[1] - s0.centre[1], q[0] - s0.centre[0]);
        if (!e.altKey) to = Math.round(to / (Math.PI / 36)) * (Math.PI / 36);
        const d = to - Math.atan2(u[1], u[0]), cs = Math.cos(d), sn = Math.sin(d);
        value = { ...s0, axes: s0.axes.map((x) => [cs * x[0] - sn * x[1], sn * x[0] + cs * x[1], x[2]]) };
      }
    }
    if (value) { setLive({ kind: "slab", id: s0.id, value }); grab.moved = true; syncSlabs(); rebuildMarks(); renderChips(); }
    if (phase === "end") {
      const now = live?.id === s0.id ? live.value : null, moved = grab.moved;
      grab = null;
      if (moved && now) updateSlab(s0.id, { centre: now.centre, half: now.half, axes: now.axes });
      else rebuildMarks();
    }
  },
});
