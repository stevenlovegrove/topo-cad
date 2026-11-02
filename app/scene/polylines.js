import { show, vis } from "../objects.js";
import { kind } from "./kinds.js";
import { goTo } from "./links.js";
import { boxAbout, onPlane } from "./geom.js";
import { homeMenu, homeOf, homeSeen, madeIn } from "./homes.js";
// Polylines: runs of points in the model's frame, drawn in a view (a footprint traced over a
// plan, an edge over a section) and kept with the project. Their lengths are what they are for.

import { openMenu, until } from "../ui.js";
import { vec } from "../glview.js";
import { workspace } from "../workspace.js";
import { live, round, sel, setLive, st, toModel, toScan } from "./state.js";
import { choose, renderChips } from "./chosen.js";
import { rebuildMarks } from "./marks.js";
import { pickIn } from "./parts.js";
import { renderTree } from "./tree.js";
import { renderLayers } from "./views.js";

/** A polyline being drawn, point by point: { view, points (model frame), closed }. */
export let drawing = null;
const lines = () => (st?.polylines || []).map((l) => (live?.kind === "polyline" && live.id === l.id ? live.value : l));
const lineOf = (id) => (id && lines().find((l) => l.id === id)) || null;
export const lineName = (l) => `${l.id}${l.name ? " " + l.name : ""}`;
export const lineLength = (l) => l.points.reduce((a, p, k) => (k + 1 < l.points.length || l.closed ? a + Math.hypot(...vec.sub(l.points[(k + 1) % l.points.length], p)) : a), 0);

/** Changes a polyline (its points, whether it is closed, its name): at once here, then with the workspace. */
async function updateLine(id, patch) {
  const l = lineOf(id);
  if (!l) return;
  const mine = setLive({ kind: "polyline", id, value: { ...l, ...patch } });
  rebuildMarks(); renderChips(); renderTree();
  const r = await workspace.call("polylines", { action: "update", id, frame: "model", ...patch });
  if (r.error) alert(r.error);
  if (live === mine) live.done = true;
}

export function startDrawing(v) {
  choose(null);
  drawing = { view: v.name, points: [] };
  renderChips(); renderLayers(); rebuildMarks();
}
export async function endDrawing(keepIt) {
  const d = drawing;
  drawing = null;
  if (!d) return;
  // (A double click ends it: its second click is the same point again.)
  const pts = d.points.filter((p, k) => k === 0 || Math.hypot(...vec.sub(p, d.points[k - 1])) > 0.01);
  renderChips(); renderLayers(); rebuildMarks();
  if (!keepIt || pts.length < 2) return;
  const r = await workspace.call("polylines", { action: "create", frame: "model", ...madeIn(), points: pts.map((p) => round(p)), closed: !!d.closed });
  if (r.error) return alert(r.error);
  const id = r.text.match(/made polyline (l\d+)/)?.[1];
  if (!id) return;
  show(`line:${id}`, "polylines");
  await until(() => st?.polylines?.some((l) => l.id === id));
  choose("polyline", id);
}
/** A click while drawing: a point on what is there (the scan, else the model), or (square on to a slab) in the slab's middle plane. */
export async function drawPoint(v, ray, e) {
  let p = null;
  const box = v.state.box;
  if (v.state.flat && box) p = onPlane(box.c, box.n, ray);
  else p = (await pickIn(v, ray)).hit ?? null;
  if (!p || !drawing) return;
  // On the first point again: closed, and done.
  const first = drawing.points[0] && v.project(v.camera(), toScan(drawing.points[0])), r = v.canvas.getBoundingClientRect();
  if (drawing.points.length > 2 && first && Math.hypot(first[0] - (e.clientX - r.left), first[1] - (e.clientY - r.top)) < 10) { drawing.closed = true; return endDrawing(true); }
  drawing.points.push(toModel(p));
  renderChips(); rebuildMarks();
}
const deleteLine = async (id) => { await workspace.call("polylines", { action: "delete", id }); if (sel.polyline === id) choose(null); };

/** Whether a polyline is drawn: its eye and its group's are on, and its home is in sight (homes.js). */
const seen = (l) => vis("polylines", `line:${l.id}`) && homeSeen(l, l.id === sel.polyline);

// What a polyline is to the tree, the card, the menus and the views (kinds.js).
/** The polyline as it was when a point of it was taken hold of: { l, k (which point), at, x, y, moved }. */
let grab = null;
kind("polyline", {
  field: "polyline", group: "polylines", order: 60,
  key: (id) => `line:${id}`,
  find: lineOf,
  title: (l) => `polyline ${lineName(l)}`,
  all: lines,
  rowOf: (row, l, depth, opts) => row(depth, "polyline", `line:${l.id}`, lineName(l), { ...opts, id: l.id, on: l.id === sel.polyline, note: `${lineLength(l).toFixed(2)} m` }),
  rows(row, list) { const mine = lines().filter((l) => homeOf(l) === "project"); return list("polylines", "Polylines", () => mine.map((l) => this.rowOf(row, l, 1)), { note: mine.length || "draw in a view" }); },
  where: (l) => [{ key: "polyline", label: `Polyline ${lineName(l)}` }],
  points: (l) => l.points.map(toScan),
  info: (l) => ({ what: "Polyline: points joined, to measure by", name: `Polyline ${lineName(l)}`, rows: [["Points", String(l.points.length)], ["Length", `${lineLength(l).toFixed(3)} m${l.closed ? " around (closed)" : ""}`]] }),
  refer: (l) => l.id,
  goto(id) {
    const l = lineOf(id);
    if (!l) return;
    show(`line:${id}`, "polylines");
    choose("polyline", id);
    goTo(boxAbout(l.points.map(toScan)), 0.6);
  },
  menu(id, m) {
    const l = lineOf(id);
    m.issue("New issue on this polyline");
    m.name();
    m.add("Go to it", () => this.goto(id));
    if (l.points.length > 2) m.add(l.closed ? "Open it" : "Close it", () => updateLine(id, { closed: !l.closed }));
    m.showHide();
    homeMenu(m, "polyline", l, id);
    m.divider();
    m.add("Delete it", () => deleteLine(id), "danger");
  },
  card: (l) => ({
    label: "Polyline", id: l.id, name: { text: l.name, save: (t) => updateLine(l.id, { name: t }) },
    body: `${l.points.length} points · ${lineLength(l).toFixed(3)} m${l.closed ? " around (closed)" : ""}<br>Drag a point to move it, a length to add one; right-click a point to remove it.`,
  }),
  marks(out) {
    const run = (pts, closed, rgb) => { for (let k = 0; k + 1 < pts.length || (closed && k < pts.length && pts.length > 2); k++) out.line(toScan(pts[k]), toScan(pts[(k + 1) % pts.length]), rgb); };
    for (const l of lines()) if (seen(l)) run(l.points, l.closed, l.id === sel.polyline ? [240, 255, 170] : [180, 235, 60]);
    // The one being drawn: its run so far, and a small cross at each point.
    if (!drawing) return;
    run(drawing.points, false, [240, 255, 170]);
    for (const p of drawing.points) { const q = toScan(p), r = 0.03; for (const a of [[r, 0, 0], [0, r, 0], [0, 0, r]]) out.line(vec.sub(q, a), vec.add(q, a), [240, 255, 170]); }
  },
  near(test) {
    for (const l of lines()) if (seen(l)) test.loop(l.id, l.points.map(toScan), l.closed);
  },
  // The one chosen: its points, and each stretch's length (dragged: a new point there).
  handles(add) {
    const l = lineOf(sel.polyline);
    if (!l || !seen(l) || drawing) return;
    const n = l.points.length;
    l.points.forEach((p, k) => add({ id: `n:${k}`, pos: toScan(p), cls: "node", title: "Drag to move this point (it lands on what is under the cursor; alt: where you let go). Right-click: remove it" }));
    for (let k = 0; k + 1 < n || (l.closed && k < n && n > 2); k++) {
      const a = l.points[k], b = l.points[(k + 1) % n];
      add({ id: `m:${k}`, pos: toScan(a.map((x, i) => 0.5 * (x + b[i]))), cls: "seg", text: `${Math.hypot(...vec.sub(b, a)).toFixed(3)} m`, title: "This stretch's length. Drag to add a point here" });
    }
  },
  drag(v, id, phase, ray, e) {
    if (phase === "start") {
      const l0 = lineOf(sel.polyline);
      if (!l0) return;
      const pts = l0.points.map((p) => p.slice());
      let k = Number(id.slice(2));
      // A stretch's middle: a new point there, which is then what is dragged.
      if (id.startsWith("m:")) { const a = pts[k], b = pts[(k + 1) % pts.length]; pts.splice(++k, 0, a.map((x, i) => 0.5 * (x + b[i]))); }
      grab = { l: { ...l0, points: pts }, k, at: toScan(pts[k]), x: e.clientX, y: e.clientY };
      return;
    }
    if (!grab) return;
    // The point moves in the plane facing the viewer through where it was.
    const moved = grab.moved || Math.hypot(e.clientX - grab.x, e.clientY - grab.y) > 3;
    const p = moved && onPlane(grab.at, v.camera().forward, ray);
    if (p) {
      grab.moved = true;
      const pts = grab.l.points.map((x) => x.slice());
      pts[grab.k] = toModel(p);
      setLive({ kind: "polyline", id: grab.l.id, value: { ...grab.l, points: pts } });
      rebuildMarks(); renderChips();
    }
    if (phase !== "end") return;
    const g = grab;
    grab = null;
    if (!g.moved) { setLive(null); return rebuildMarks(); }
    (async () => {
      const pts = live.value.points.map((x) => x.slice());
      // Looking at it in the round, it lands on the surface under the cursor; square on to a slab, it stays in its plane.
      if (!v.state.flat && !e.altKey) {
        const hit = (await pickIn(v, ray)).hit;
        if (hit) pts[g.k] = toModel(hit);
      }
      updateLine(g.l.id, { points: pts.map((x) => round(x)) });
    })();
  },
  handleMenu(v, id, e) {
    const l = lineOf(sel.polyline);
    if (!l || !id.startsWith("n:")) return;
    const k = Number(id.slice(2)), items = [];
    if (l.points.length > 2) items.push({ label: "Remove this point", run: () => updateLine(l.id, { points: l.points.filter((_, i) => i !== k) }) });
    if (l.points.length > 2) items.push({ label: l.closed ? "Open the polyline" : "Close the polyline", run: () => updateLine(l.id, { closed: !l.closed }) });
    items.push({ label: "Orbit about this point", run: () => v.centreOn(toScan(l.points[k])) });
    openMenu(items, e.clientX, e.clientY);
  },
});
