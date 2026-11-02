import { defineType, openDoc, opensFor, retitle, setInfo } from "../shell/dock.js";
import { madeIn } from "./homes.js";
// The 3D views (glview.js) on the page: each in its pane with its own controls, a gizmo, the
// card of what is chosen and a proposal's strip; what a click, a drag and a right-click in one
// do; and which layers each draws.

import { $, el, esc, openMenu } from "../ui.js";
import { did } from "../actions.js";
import { GLView, vec } from "../glview.js";
import { onPrefs, prefs, setPrefs } from "../prefs.js";
import { workspace } from "../workspace.js";
import * as objects from "../objects.js";
import { vis } from "../objects.js";
import { msg } from "../shell/panel.js";
import { active, pl, scanned, sel, setActive, st, toScan, unrot, views } from "./state.js";
import { choose, contextMenu, objectMenu, renderChips } from "./chosen.js";
import { openIssue } from "./issues.js";
import { canGoBack, canGoForward, goBack, goForward, gotoIssue, gotoMembers, gotoObject, keepView } from "./links.js";
import { handleDrag, handleHover, handleMenu, objectAt, rebuildMarks } from "./marks.js";
import { kindOf } from "./kinds.js";
import { deflects, drawOverlay, overlayMarker, placeKeys, shapeOver } from "./overlay.js";
import { counted, markBalance, marksBalance, members, modelBounds, pickIn, pointAt, ratios, rebuildModel, selectMember, showMember } from "./parts.js";
import { primePoints, requestFree, requestSlab, scanSeenIn, seenSel, showSelection, syncHighlights } from "./points.js";
import { drawPoint, drawing, endDrawing, startDrawing } from "./polylines.js";
import { placeDiffbar } from "./proposals.js";
import { hasSkeleton } from "./skeleton.js";
import { CLEAR, boxOf, cutAlong, heightsCut, leaveSlab, slabName, slabOf, syncSlabs, toggleHeights, updateSlab, viewSlab } from "./slabs.js";
import { viewPhoto } from "./photos.js";
import { renderTree } from "./tree.js";
import { displayExact, exactFor, exactIn, findExact, gizmoLook, modelLevels, partAt, sectionExact } from "./exact.js";
import { PICK_KINDS } from "./bodyparts.js";
import { bindGrab, canUndoDrag, undoDrag } from "./grab.js";

/** Who else wants to know that a view has been drawn, which it is when its camera moves. None, for a page with no session. */
export const viewDrawn = new Set();

/** The key of a 3D view to open: the one last used (or any there is); `fresh`, or with none open: of a new one. */
export function viewKey(fresh) {
  const have = views[active] ?? Object.values(views)[0];
  if (have && !fresh) return have.key;
  let n = 1;
  while (views[n]) n++;
  return `view:${n}`;
}
/** The 3D view to act in: the one last used, else any; with none open, one is opened. */
// (What the viewer chooses of how views look, every view takes; the toolbars say what of it they show.)
// "O", the cursor over a 3D view: that view from perspective to parallel, or back, about what is under the cursor (as
// Pangolino's views do), or what it looks at where nothing is.
document.addEventListener("keydown", (e) => {
  if (e.key.toLowerCase() !== "o" || e.metaKey || e.ctrlKey || e.altKey || e.target.closest?.("input, textarea, select, [contenteditable]")) return;
  const v = Object.values(views).find((w) => w.cursorAt);
  if (!v) return;
  e.preventDefault();
  const r = v.canvas.getBoundingClientRect();
  v.switchProjection({ x: v.cursorAt.x - r.left, y: v.cursorAt.y - r.top });
  renderTools(v);
});
onPrefs(({ display, gizmo, ...look }) => {
  for (const v of Object.values(views)) {
    if (display) { v.display = { ...display }; displayExact(v); }
    if (gizmo) gizmoLook(v);
    v.set(look, true);
    renderTools(v);
  }
});

export function theView() {
  return views[active] ?? Object.values(views)[0] ?? openDoc(viewKey(true), { tab: true }).view;
}

/** What a new view is to be turned to once it is there (by its id): a thing of the scene it was opened for. */
const pending = {};
/** Turns a view to a thing of the scene: clipped to it, for a slab; else looking at it. */
function turnTo(v, kind, id) {
  setActive(v.name);
  if (kind === "slab") { choose("slab", id); viewSlab(id, v); } else if (kind === "photo") { choose("photo", id); viewPhoto(id, v); } else gotoObject(kind, id);
}
// A thing of the scene, dragged onto a pane or opened: onto a 3D view, that view is turned to it; else a new view is, once it is there.
opensFor("thing", (what, onto) => {
  const [kind, id] = [what.slice(0, what.indexOf(":")), what.slice(what.indexOf(":") + 1)];
  if (onto?.type === "view") { turnTo(onto.view, kind, id); return null; }
  const key = viewKey(true);
  pending[key.slice(5)] = { kind, id };
  return key;
});

/** A 3D view in a pane (by its id), with what is kept of how it looks: what the dock shows, and the view itself. */
function makeView(id, kept) {
  const pane = el("div", { class: "glpane" });
  const host = el("div", { class: "glhost" });
  const chip = el("div", { class: "glchip", hidden: "" });
  const bar = el("div", { class: "diffbar", hidden: "" });
  // Its own controls. (What it is turned by, a gizmo in its corner, and what is over it (overlay.js), the scene
  // draws: exact.js.)
  const tools = el("div", { class: "gltools" });
  pane.append(host, chip, bar, tools);
  const v = (views[id] = new GLView(host));
  // (A part script's bodies, drawn exactly under it where the page can.)
  exactFor(v);
  Object.assign(v, { name: id, key: `view:${id}`, pane, chip, diffbar: bar, tools, slab: kept?.slab ?? null, display: { ...prefs().display }, pickKind: kept?.pickKind ?? "part", skeleton: !!kept?.skeleton, deflected: !!kept?.deflected });
  // (How the viewer has chosen views look: theirs, not the view's kept state's.)
  const { display: _, gizmo: __, ...look } = prefs();
  v.set(look, true);
  // (The model's parts are picked where they are drawn.)
  const pickable = () => v.state.layers.model;
  v.onPoint = (ray) => pointAt(ray, v);
  v.onDraw = () => { placeKeys(v); for (const f of viewDrawn) f(v); };
  v.onMarker = (ev) => overlayMarker(ev);
  // What the cursor is over says what it is (as the scene says what its markers are: it draws both).
  let hovering = 0;
  host.addEventListener("pointermove", (e) => {
    // (Where the cursor is over the view's canvas, for what a key does about it.)
    v.cursorAt = e.target === v.canvas ? { x: e.clientX, y: e.clientY } : null;
    if (e.buttons || e.target !== v.canvas) return;
    cancelAnimationFrame(hovering);
    hovering = requestAnimationFrame(() => {
      const r = v.canvas.getBoundingClientRect(), m = pickable() && partAt(v, v.ray(e.clientX - r.left, e.clientY - r.top));
      const part = m && members.find((x) => x.path === m.path), used = part && ratios[part.path];
      v.sceneTip(part ? `${part.label}${used ? ` · ${used.ratio.toFixed(2)} ${used.title.toLowerCase()} (${used.combination})` : ""}` : null);
      sel.hover = part?.path ?? null;
    });
  });
  host.addEventListener("pointerleave", () => { v.cursorAt = null; cancelAnimationFrame(hovering); v.sceneTip(null); if (sel.hover) { sel.hover = null; drawOverlay(v); } });
  pane.addEventListener("pointerdown", () => { if (active !== id) { setActive(id); renderLayers(); drawOverlay(v); } }, true);
  v.onHandle = (id, phase, ray, e) => handleDrag(v, id, phase, ray, e);
  v.onHandleMenu = (id, e) => handleMenu(v, id, e);
  v.onHandleHover = (id, on) => handleHover(v, id, on);
  v.onLine = (a, b) => cutAlong(v, a, b);
  v.onChange = () => {
    requestFree(v);
    renderLayers();
  };
  v.onLabel = (id) => openIssue(id);
  // Double-click a pin: this view goes to its place (Back returns).
  v.onLabelZoom = (id) => gotoIssue(id, v);
  const pickAt = async (ray, e) => {
    if (drawing) return drawPoint(v, ray, e);
    // Of a part script's bodies, what the view's selection says: a face, an edge, a vertex of one, or a node.
    if (exactIn(v) && v.pickKind !== "part" && e) {
      const found = findExact(v, e.offsetX, e.offsetY, v.pickKind);
      return void choose(found ? "bodypart" : null, found);
    }
    // On the lines of a slab, a plane or a polyline: that is what is meant.
    const o = !e?.altKey && objectAt(v, e);
    if (o) return choose(o.kind, o.id);
    const cuts = v.cuts();
    // (Of the scan, only where it is drawn.)
    const r = scanSeenIn(v) ? await workspace.pick(ray.origin, ray.dir, cuts.z, cuts.slab, v.pixelSize()) : { hit: null, selections: [] };
    // What is drawn there is what is selected: a shown selection's points; the member under
    // the cursor (the one the ray through it meets first, as it is drawn), unless the scan's
    // surface there is well in front of it (the click is then on that surface); else the
    // point itself. With alt: always the point.
    const selId = !e?.altKey && (r.selections || []).find((id) => { const s = st.selections.find((x) => x.id === id); return s && seenSel(s); });
    // (The body under the cursor, exactly, as the part it is of.)
    const m = !e?.altKey && pickable() && partAt(v, ray);
    // (A lining stands a little in front of what it covers: that much is allowed.)
    const onMember = m && (!r.hit || m.t <= r.range + 0.15) ? m.path : null;
    if (selId) choose("selection", selId);
    else if (onMember) { selectMember(onMember); if (r.hit) { sel.at = r; renderChips(); rebuildMarks(); } }
    else {
      // (A point of the scan: its card says how the scan's points are drawn.)
      choose(r.hit ? "point" : null, r.hit ? { ...r, scan: true } : null);
      // The member near it fills the Member tab (without leaving the tab you are on).
      if (r.member) showMember(r.member.path);
    }
  };
  v.onPick = pickAt;
  // Right-click: pick what is there, then offer what can be done with it.
  v.onContext = async (ray, e) => {
    // (Looking at what is there does not change what is selected.)
    const o = objectAt(v, e);
    if (o) return openMenu(objectMenu(o.kind, o.id, v), e.clientX, e.clientY);
    contextMenu(v, await pickIn(v, ray), e.clientX, e.clientY, ray);
  };
  host.addEventListener("dblclick", (e) => {
    if (e.target !== v.canvas) return;
    if (drawing) return endDrawing(true);
    // On what is looked through (a slate of a frame of a recording, a photo's mark, someone's in a session): that is done.
    const frame = !e.altKey && objectAt(v, e);
    if (frame && kindOf(frame.kind).lookThrough) return kindOf(frame.kind).goto(frame.id);
    // On a part of the model: this view goes to it, as a second click on its row in the tree does (Back returns).
    const box = v.canvas.getBoundingClientRect(), on = !e.altKey && pickable() && partAt(v, v.ray(e.clientX - box.left, e.clientY - box.top));
    if (on) return gotoMembers(on.path, v);
    const p = sel.pick ?? sel.at;
    if (p?.hit) v.centreOn(p.hit);
  });
  // A window dragged with shift: the points in it become a selection. Of the whole scan only the
  // surface nearest the viewer is taken; in a view clipped to a slab, all of the slab's points.
  // (With alt, or with the view's "everything in the window" on: all that is behind it too.)
  v.onSelect = async (w, e) => {
    if (!scanned()) return;
    const cuts = v.cuts();
    const r = await workspace.call("selections", { action: "window", ...madeIn(), ...w, z: cuts.z, slab: cuts.slab, front_only: !cuts.slab && !e?.altKey && !v.state.selectAll });
    if (r.error) return setInfo(v.key, r.error);
    const made = r.text.match(/selected (s\d+)/)?.[1];
    if (made) showSelection(made);
  };
  // As it was left; else as the others are (against a scan's points, on the dark; alone, on paper).
  if (kept?.state) v.set(kept.state, true); else v.set({ paper: !scanned() }, true);
  v.framed = !!kept?.state;
  primeView(v);
  // (A node of a part script's, dragged: before whatever else in the view can be.)
  bindGrab(v);
  const to = pending[id];
  delete pending[id];
  if (to) setTimeout(() => turnTo(v, to.kind, to.id), 0);
  return {
    el: pane, view: v,
    shown: () => { setActive(id); if (st) { syncSlabs(); rebuildMarks(); } renderLayers(); v.redraw(); },
    dispose: () => {
      // (What stood over it that is not its own, the skeleton's key and the deflected shape's scale, is kept.)
      for (const e of pane.querySelectorAll("#struct3d, #deflect3d")) { e.hidden = true; $("parked").append(e); }
      delete views[id];
      if (active === id) setActive(Object.keys(views)[0] ?? null);
      v.exactView?.view.free();
    },
    keep: () => ({ state: v.get(), slab: v.slab, pickKind: v.pickKind, skeleton: v.skeleton, deflected: v.deflected }),
    // (Gone back to: it moves to how it was, in sight.)
    update: (k) => { if (!k) return; v.slab = k.slab; v.animate(k.state); syncSlabs(); },
  };
}
defineType("view", {
  title: (id) => { const sb = slabOf(views[id]?.slab); return sb ? `3D · slab ${slabName(sb)}` : `3D view${id === "1" ? "" : ` ${id}`}`; },
  make: makeView,
  // (One 3D view is not put in place of another: each is looked at as it was left.)
  replaces: false,
});

/** Gives a view just made what the others have: the scan's points, the model's parts, the marks; and looks at the model. */
function primeView(v) {
  v.zOffset = pl.t[2];
  v.floorZ = st?.scan?.floor_z ?? 0;
  primePoints(v);
  syncLayers();
  if (!st) return;
  syncSlabs(); rebuildModel(); rebuildMarks(); syncHighlights(); renderChips(); shapeOver();
  const b = !v.framed && !v.slab && modelBounds();
  if (b) { v.framed = true; v.frame(toScan(b.centre), b.radius, 0); }
  setInfo(v.key, v.slab ? "" : counted);
  renderTools(v);
}

// What is shown, apart from what is chosen. Every object and every group of the tree has an eye: whether it is drawn (objects.js keeps
// which). That is separate from selecting it (clicking it), which says what the chip, the
// Member tab and a new issue refer to.

/** Which of a view's layers are drawn: the scan's points where there are any and they are shown; the model's parts. */
export function syncLayers() {
  for (const v of Object.values(views)) v.set({ layers: { scan: vis("scan") && scanned(), model: true, marks: true, issues: true } }, true);
}
/** What is shown has changed: the views are drawn again. */
export function applyVisibility() {
  syncLayers();
  rebuildModel(); rebuildMarks(); syncHighlights();
  // (Each scan's eye: its cloud drawn or not.)
  for (const v of Object.values(views)) { renderTools(v); v.redraw(); }
}
export const toggleVis = (key) => objects.toggle(key);

// A view's own controls: in the view, over it.
const svg = (d, extra = "") => `<svg viewBox="0 0 16 16" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" stroke-linecap="round">${extra}<path d="${d}"/></svg>`;
const ICON = {
  whole: svg("M3 3l10 10 M13 3L3 13"),
  noroof: svg("M3.5 8v5.5h9V8", `<path d="M2 8l6-5 6 5" stroke-dasharray="1.6 1.8"/>`),
  back: svg("M10 3L5 8l5 5"),
  fwd: svg("M6 3l5 5-5 5"),
  square: svg("M3 3h10v10H3z M8 5.5v5 M5.5 8h5"),
  lock: svg("M4.5 7.5h7v5.5h-7z M6 7.5V5.5a2 2 0 0 1 4 0v2"),
  line: svg("M2.5 12.5l4-7 4 4 3-6", `<circle cx="2.5" cy="12.5" r="1.1" fill="currentColor"/><circle cx="6.5" cy="5.5" r="1.1" fill="currentColor"/><circle cx="10.5" cy="9.5" r="1.1" fill="currentColor"/><circle cx="13.5" cy="3.5" r="1.1" fill="currentColor"/>`),
  free: svg("M2.5 13.5L8 3.5 M2.5 13.5L12 5.5 M2.5 13.5L13.5 9.5", `<circle cx="2.5" cy="13.5" r="1.3" fill="currentColor"/>`),
  skeleton: svg("M3 13L8 3l5 10 M5.2 8.5h5.6", `<circle cx="3" cy="13" r="1.2" fill="currentColor"/><circle cx="8" cy="3" r="1.2" fill="currentColor"/><circle cx="13" cy="13" r="1.2" fill="currentColor"/>`),
  deflected: svg("M2 5h12 M2 5c3 7 9 7 12 0", `<path d="M8 5v5" stroke-dasharray="1.5 1.5"/>`),
  balance: svg("M8 2.5a5.5 5.5 0 1 0 0 11a5.5 5.5 0 1 0 0-11 M8 1v14 M1 8h14", `<path d="M8 8V2.5A5.5 5.5 0 0 1 13.5 8z M8 8v5.5A5.5 5.5 0 0 1 2.5 8z" fill="currentColor" stroke="none"/>`),
  solids: svg("M8 1.8l5.5 3v6.4L8 14.2l-5.5-3V4.8z", `<path d="M8 1.8l5.5 3L8 7.8 2.5 4.8z" fill="currentColor" fill-opacity=".35" stroke="none"/>`),
  edges: svg("M8 1.8l5.5 3v6.4L8 14.2l-5.5-3V4.8z M2.5 4.8L8 7.8l5.5-3 M8 7.8v6.4"),
  section: svg("M2.5 4.5h7v7h-7z M9.5 4.5l4-2.5v7l-4 2.5", `<path d="M6 1v14" stroke-dasharray="1.6 1.2"/>`),
  xray: svg("M8 1.8l5.5 3v6.4L8 14.2l-5.5-3V4.8z M2.5 4.8L8 7.8l5.5-3 M8 7.8v6.4", `<path d="M2.5 11.2L8 8.2l5.5 3 M8 1.8v6.4" stroke-dasharray="1.2 1.4" opacity=".6"/>`),
  pickpart: svg("M8 1.8l5.5 3v6.4L8 14.2l-5.5-3V4.8z M2.5 4.8L8 7.8l5.5-3 M8 7.8v6.4"),
  pickface: svg("M8 1.8l5.5 3v6.4L8 14.2l-5.5-3V4.8z", `<path d="M8 7.8l5.5-3v6.4L8 14.2z" fill="currentColor" stroke="none"/>`),
  pickedge3: svg("M8 1.8l5.5 3v6.4L8 14.2l-5.5-3V4.8z", `<path d="M8 7.8v6.4" stroke-width="3"/>`),
  pickvertex: svg("M8 1.8l5.5 3v6.4L8 14.2l-5.5-3V4.8z", `<circle cx="8" cy="7.8" r="2.3" fill="currentColor"/>`),
  picknode: svg("M3 13L13 3", `<circle cx="8" cy="8" r="3" fill="var(--bg, #fff)"/><circle cx="8" cy="8" r="1.3" fill="currentColor"/>`),
  cut: svg("M2 13L14 3", `<path d="M5 14.5l-3-1.5 1.5-3 M12.5 1.5l1.5 1.5-3 1.5" stroke-width="1"/>`),
};

export function renderTools(v) {
  const s = v.state, sb = slabOf(v.slab);
  // Not while one of its fields is being typed in.
  if (v.tools.contains(document.activeElement) && document.activeElement.tagName === "INPUT") return;
  const wasOpen = v.tools.querySelector("details.more")?.open;
  const btn = (a, icon, title, on) => `<button data-a="${a}" class="${on ? "on" : ""}" title="${title}">${icon}</button>`;
  // (What is of a scan's points is there only where there is a scan.)
  const scan = scanned(), picked = sel.member;
  // (The model's bodies, drawn exactly: their solids, their edges, x-ray; where the page draws them.)
  const bodies = exactIn(v), d = v.display ?? {};
  // (Its heights, where it is clipped to a plan; and whether it is cut to a box of its own, the roof taken away.)
  const lv = modelLevels(), roofOff = heightsCut(v);
  const cm = `${Math.round(CLEAR * 100)} cm`;
  const roofBtn = () => {
    const can = roofOff || v.heightsKept || lv.roof != null;
    const title = roofOff
      ? "The roof is taken away: the view is cut to a box of its own (click it to drag its faces; Delete it, from its menu, to start again). Again, to show everything: the box is kept for next time"
      : v.heightsKept
        ? "Take the roof away again: the view cut to its box as it was left"
        : !can
          ? "The model has no roof (nothing whose role is a truss's chord or web, a rafter or a ceiling tie) to take away: view a plan slab (the Slabs group's menu, in the tree: New slab: plan) and drag its faces, or set its heights on its card"
          : `Take the roof away: the view cut to a box whose top is ${cm} under the roof's lowest part (parts cut open show their sections), chosen, so its faces can be dragged (its bottom up, to take the floor away too)`;
    return `<button data-a="noroof" class="${roofOff ? "on" : ""}" ${can ? "" : "disabled"} title="${title}">${ICON.noroof}</button>`;
  };
  v.tools.innerHTML =
    // (Its projection, a view from a side or from above, and fitting it: the gizmo in its corner, and "O".)
    `${canGoBack(v) || canGoForward(v) ? `<span class="grp">${btn("goback", ICON.back, "Back: as this view was before the last link followed, or thing gone to, in it")}${canGoForward(v) ? btn("goforward", ICON.fwd, "Forward again") : ""}</span>` : ""}` +
    `<span class="grp">${bodies ? `${btn("solids", ICON.solids, "Solids: the bodies' faces", d.solids)}${btn("edges", ICON.edges, "Edges: the bodies' edges, and where round faces turn away. Without solids, what is behind them is removed", d.edges)}${btn("xray", ICON.xray, "X-ray: see what is hidden. Solids become ghosts; edges hidden behind are drawn faintly", d.xray)}${btn("nodes", ICON.picknode, "Nodes: the points the part's solves put, on a body or not (green: something is kept of where it is: a param of it, or a drag)", d.nodes)}${btn("section", ICON.section, "Section: the bodies cut exactly by the plumb plane through their middle, square to the way the view looks, the half toward you taken away (set as the view is when pressed)", !!v.section)}${canUndoDrag() ? btn("undodrag", ICON.back, "Undo the last drag: the drag it kept let go, and any it took the place of put back") : ""}` : ""}${hasSkeleton() ? btn("skeleton", ICON.skeleton, "The skeleton: the structure's nodes, where its members meet and end, over this view", v.skeleton) : ""}${deflects() ? btn("deflected", ICON.deflected, "The deflected shape, exaggerated, for the load combination chosen in the checks", v.deflected) : ""}${btn("balance", ICON.balance, "Mark the middle of the weight of what is chosen (a part, or a group of them), seen through whatever is in front of it", marksBalance())}</span>` +
    (bodies ? `<span class="grp" title="What a click picks: a part, or a face, an edge or a vertex of a body, or a node (where the part's solves put a point: the skeleton shows them)">${PICK_KINDS.map((k) => btn(`pick:${k}`, ICON[k === "edge" ? "pickedge3" : `pick${k}`], { part: "Pick a part", face: "Pick a face of a body", edge: "Pick an edge of a body (seen; with x-ray, hidden ones too)", vertex: "Pick a vertex of a body (seen; with x-ray, hidden ones too)", node: "Pick a node: a point the part's solves put, on a body or not (shows them)" }[k], (v.pickKind ?? "part") === k)).join("")}</span>` : "") +
    `<span class="grp">${picked ? `<button data-a="only" title="Show only the part picked (again: show all). The same as a right-click on its eye in the tree">only this</button>` : ""}${objects.isolated() ? `<button data-a="all" title="Show every part again">show all</button>` : ""}</span>` +
    (sb && !sb.filter
      ? `<span class="grp">${btn("whole", ICON.whole, `Leave slab ${esc(slabName(sb))}: the view shows everything again (a slab is viewed again from the tree: double-click it)`)}${btn("square", ICON.square, "Look square on at the slab, fitted in the view")}${btn("flat", ICON.lock, "Held square on: a drag slides the view. Off: a drag turns it", s.flat)}${workspace.can("rays") ? btn("free", ICON.free, "Free space: where the scanner's rays pass in the slab, under the points. Brighter: more rays. Where it is dark, something was in the way, or nothing looked. It is made for what the view sees, a cell a pixel: zoom in for finer." + (s.free ? v.freeInfo ?? "" : ""), s.free) : ""}</span>` +
        `<span class="grp" title="The slab: its thickness (m), and stepping it back and forward by half of that">${btn("back", ICON.back, "Slab back (away from the viewer)")}<input type="number" step="0.05" min="0.01" value="${(2 * sb.half[2]).toFixed(2)}" data-a="thick">${btn("fwd", ICON.fwd, "Slab forward (toward the viewer)")}</span>`
      : "") +
    `<span class="grp">${btn("line", ICON.line, "Draw a polyline: click its points (on what is there, or in the slab seen square on)", drawing?.view === v.name)}${btn("cut", ICON.cut, "Cut a slab: drag a line across the building, seen from above, to look at what it cuts through", v.tool === "line")}</span>` +
    `<span class="grp">${roofBtn()}</span>` +
    `<details class="more" ${wasOpen ? "open" : ""}><summary title="Keeping this view, and how a shift-drag selects">⋯</summary><div><label ${scan ? "" : "hidden"} title="A shift-drag then takes every point inside the window, however far behind. Off: only the surface nearest you (alt-shift-drag takes everything once)"><input type="checkbox" data-a="selall" ${s.selectAll ? "checked" : ""}> A shift-drag selects everything in the window, not only the nearest surface</label><br><button data-a="keep" title="Keep this view with the project, by a name: it is listed under Views, to open again">Keep this view…</button> <button data-a="copy" title="This view's direction, as a sheet's view takes it: to paste into a sheet(...) in the script">Copy the view for a sheet</button></div></details>`;
  v.tools.querySelector("input[data-a=selall]").onchange = (e) => setPrefs({ selectAll: e.target.checked });
  // (Its menu is kept within the view: moved left by as much as it would go past the view's right edge.)
  const more = v.tools.querySelector("details.more"), place = () => {
    const m = more.querySelector(":scope > div");
    m.style.left = "0px";
    if (!more.open) return;
    const over = m.getBoundingClientRect().right - (v.pane.getBoundingClientRect().right - 8);
    if (over > 0) m.style.left = `${-over}px`;
  };
  more.ontoggle = place;
  place();
  const step = (g) => updateSlab(sb.id, { centre: vec.add(sb.centre, vec.scale(sb.axes[2], g * sb.half[2])) });
  const acts = {
    // (About what the view looks at, which keeps its place and size; "O" over the view, about what is under the cursor.)
    // (What is drawn of the bodies is the viewer's choice, in every view, and kept.)
    solids: () => setPrefs({ display: { ...v.display, solids: !v.display.solids } }),
    edges: () => setPrefs({ display: { ...v.display, edges: !v.display.edges } }),
    xray: () => setPrefs({ display: { ...v.display, xray: !v.display.xray } }),
    nodes: () => setPrefs({ display: { ...v.display, nodes: !v.display.nodes } }),
    section: () => {
      v.section = !v.section;
      const whole = sectionExact(v, v.section);
      if (whole) msg(`Not sectioned (a round face crosses the plane), shown whole: ${whole.split("\n").join(", ")}`, true);
    },
    undodrag: () => undoDrag(),
    skeleton: () => { v.skeleton = !v.skeleton; drawOverlay(v); },
    deflected: () => { v.deflected = !v.deflected; drawOverlay(v); shapeOver(); },
    balance: () => { markBalance(!marksBalance()); for (const o of Object.values(views)) if (o !== v) renderTools(o); },
    copy: () => copyView(v),
    keep: () => keepView(v),
    goback: () => goBack(v),
    goforward: () => goForward(v),
    only: () => { if (sel.member) objects.isolate(`m:${sel.member}`); },
    all: () => objects.showAll(),
    square: () => v.squareOn(boxOf(sb)),
    flat: () => (s.flat ? v.set({ flat: false }) : v.squareOn(boxOf(sb))),
    free: () => { v.set({ free: !s.free }); requestSlab(v); },
    line: () => (drawing ? endDrawing(true) : startDrawing(v)),
    cut: () => { v.tool = v.tool === "line" ? null : "line"; },
    noroof: () => toggleHeights(v, lv.roof),
    whole: () => leaveSlab(v),
    back: () => step(-1),
    fwd: () => step(1),
  };
  // (What a click picks, of a part script's bodies. Picking nodes shows them.)
  for (const k of PICK_KINDS) acts[`pick:${k}`] = () => { v.pickKind = k; if (k === "node" && !v.display?.nodes) setPrefs({ display: { ...v.display, nodes: true } }); };
  for (const b of v.tools.querySelectorAll("button[data-a]")) b.onclick = () => { did("view", b.dataset.a); acts[b.dataset.a](); renderTools(v); };
  const th = v.tools.querySelector("input[data-a=thick]");
  if (th) th.onchange = () => { if (Number(th.value) > 0) updateSlab(sb.id, { half: [sb.half[0], sb.half[1], Number(th.value) / 2] }); };
}

/** A view's direction as a sheet's view takes it (a direction in the model toward the viewer), copied to paste into the script. */
async function copyView(v) {
  const code = `iso(undefined, { from: [${unrot(vec.scale(v.camera().forward, -1)).map((x) => x.toFixed(3)).join(", ")}] })`;
  try { await navigator.clipboard.writeText(code); msg(`Copied: ${code} — paste into a sheet(...) in your script (a direction in the model toward the viewer).`, false); }
  catch (_) { msg(`View: ${code}`, false); }
}

export function renderLayers() {
  renderTree();
  for (const v of Object.values(views)) { renderTools(v); placeDiffbar(v); }
  // (A view's name says the slab it is clipped to.)
  retitle();
}
