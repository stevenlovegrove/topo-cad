// Drawings: a sheet of the model's set (SVG, as the engine draws it) in a pane, to zoom and pan, with what is drawn
// over it (each member drawn coloured by how far it is used, the part under the cursor); any number of them open.

import { $, el, esc, openMenu, tip } from "./ui.js";
import { model } from "./project.js";
import { defineType, openDoc, openOf } from "./shell/dock.js";
import { heatColor, inspected, msg } from "./shell/panel.js";
import { selectMember } from "./scene/parts.js";

const NS = "http://www.w3.org/2000/svg";

/** The sheets open: each { number, el, svg, viewBox, heat, hover, … } (see `makeSheet`). */
const sheets = () => openOf("sheet");
const indexOf = (S) => (model.state?.sheets ?? []).findIndex((x) => x.number === S.number);
/** The model has been built again: every sheet open is drawn again (where it was looked at). */
export function loadSheets() {
  return Promise.all(sheets().map(loadSheet));
}

async function loadSheet(S) {
  const sheet = indexOf(S), holder = S.el.querySelector(".sheet");
  if (!model.built || sheet < 0) { S.svg = null; holder.innerHTML = `<div class="nothing muted">${!model.built ? "The model does not build: see its script." : model.state.drawn === false ? "Drawing the sheets…" : `The model's set has no sheet ${esc(S.number)} now.`}</div>`; return; }
  const text = await model.ask("sheet", { sheet });
  holder.innerHTML = text;
  const svg = (S.svg = holder.querySelector("svg"));
  svg.removeAttribute("width");
  svg.removeAttribute("height");
  S.viewBox ??= fitViews(S);
  svg.setAttribute("viewBox", S.viewBox.join(" "));
  for (const id of ["heat", "hl"]) svg.appendChild(document.createElementNS(NS, "g")).setAttribute("class", id);
  await loadHeat(S);
}

// Initial zoom: the model views on the sheet (the whole sheet if there are none).
function fitViews(S) {
  const views = model.state.sheets[indexOf(S)].views || [];
  if (!views.length) return S.svg.getAttribute("viewBox").split(/\s+/).map(Number);
  let [x0, y0, x1, y1] = [Infinity, Infinity, -Infinity, -Infinity];
  for (const v of views) { x0 = Math.min(x0, v.bbox[0]); y0 = Math.min(y0, v.bbox[1]); x1 = Math.max(x1, v.bbox[2]); y1 = Math.max(y1, v.bbox[3]); }
  const m = 0.06 * Math.max(x1 - x0, y1 - y0);
  return [x0 - m, y0 - m, x1 - x0 + 2 * m, y1 - y0 + 2 * m];
}

const px = (S) => 1 / S.svg.getScreenCTM().a;  // one screen pixel in SVG units

/** A polygon on a sheet (SVG inches), not in the way of the cursor. */
function outline(S, g, pts, attrs) {
  const e = g.appendChild(document.createElementNS(NS, "polygon"));
  for (const [k, v] of Object.entries({ points: pts.map((q) => q.join(",")).join(" "), ...attrs })) e.setAttribute(k, v);
  e.style.pointerEvents = "none";
}

// Colour: how far each member drawn on a sheet is used (the checks panel's "Colour by", and its combination); coloured
// otherwise (by material, by the scan), the member inspected is still outlined.
async function loadHeat(S) {
  const mode = $("heatMode").value, sheet = indexOf(S);
  S.heat = !S.svg || !model.built ? [] : await model.ask("heat", { sheet, mode: ["material", "scan"].includes(mode) ? "utilization" : mode, combo: $("heatCombo").value });
  drawHeat(S);
}
function drawHeat(S) {
  const g = S.svg?.querySelector(".heat");
  if (!g) return;
  g.innerHTML = "";
  const p = px(S), off = ["material", "scan"].includes($("heatMode").value);
  for (const h of off ? S.heat.filter((x) => x.path === inspected) : S.heat) {
    outline(S, g, h.outline, { fill: off ? "#1f5fbf" : heatColor(h.ratio), "fill-opacity": off ? 0.12 : 0.6, stroke: off || h.path === inspected ? "#111" : "none", "stroke-width": 2.5 * p });
  }
}
/** What the sheets are coloured by has changed (the colouring, or the combination); or only which member is inspected. */
export const loadHeats = () => Promise.all(sheets().map(loadHeat));
export const drawHeats = () => { for (const S of sheets()) drawHeat(S); };

/** The part under the cursor, outlined (a click chooses it). */
function drawHover(S) {
  const g = S.svg?.querySelector(".hl");
  if (!g) return;
  g.innerHTML = "";
  if (S.hover) outline(S, g, S.hover.member_outline, { fill: "#1f5fbf", "fill-opacity": 0.15, stroke: "#1f5fbf", "stroke-width": 2 * px(S) });
}
// What is under the cursor on a sheet: asked of the engine, at most one request in flight.
async function hoverAt(S, e) {
  S.queued = e;
  if (S.pending) return;
  while (S.queued) {
    const q = S.queued, p = svgPoint(S, q);
    S.queued = null; S.pending = true;
    try { S.hover = await model.ask("pick", { sheet: indexOf(S), x: p.x, y: p.y, radius: 6 * px(S) }); } finally { S.pending = false; }
    const h = S.hover && S.heat.find((x) => x.path === S.hover.member);
    tip(S.hover ? `${S.hover.member_label}${h ? ` · ${h.ratio.toFixed(2)} ${h.title.toLowerCase()} (${h.combination})` : ""} — click to choose it` : null, { clientX: q.clientX, clientY: q.clientY });
    drawHover(S);
  }
}

function svgPoint(S, evt) {
  const pt = S.svg.createSVGPoint();
  pt.x = evt.clientX; pt.y = evt.clientY;
  return pt.matrixTransform(S.svg.getScreenCTM().inverse());
}

/** Hands the browser a file to keep: its name, and its text. */
export function download(name, text) {
  const a = el("a", { href: URL.createObjectURL(new Blob([text], { type: "application/dxf" })), download: name });
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(a.href), 1000);
}
/**
 * What a sheet can be taken away as (the engine's list of the model's exports, each a DXF): the sheet as it is
 * on paper, each of its views full size, and the flat pattern to cut of every sheet of stock in the model.
 */
async function exportMenu(S, e) {
  // (Where the button is, before anything is waited for: an event forgets what it was on.)
  const r = e.currentTarget.getBoundingClientRect(), sheet = indexOf(S), all = await model.ask("exports");
  const item = (x, label) => ({ label, run: async () => { const f = await model.ask("export", { name: x.name }); download(f.name, f.text); msg(`${f.name}: ${x.what}`); } });
  const items = [{ label: "Take away as DXF", cls: "head", run() {} }];
  for (const x of all.filter((x) => x.sheet === sheet)) items.push(item(x, x.kind === "sheet" ? `Sheet ${S.number}, as on paper` : `${x.what.split(" (sheet ")[0]}, full size`));
  const flats = all.filter((x) => x.kind === "flat");
  if (flats.length) items.push(null, { label: "To cut", cls: "head", run() {} }, { label: "All of them, nested on stock…", run: () => openDoc("nest:cut") }, ...flats.map((x) => item(x, `${x.what.split(" laid flat")[0]}, laid flat`)));
  openMenu(items, r.left, r.top - 8 - 26 * items.length);
}

/** A sheet in a pane (by its number in the set): its element, and what is kept of how it is looked at. */
function makeSheet(number, kept) {
  const pane = el("div", { class: "sheetpane" }, `<div class="sheet"></div><div class="hint">scroll to zoom · drag to pan · double-click to fit</div><button class="export" title="This sheet, its views full size, and flat patterns to cut, as DXF">DXF</button>`);
  const S = { number, el: pane, svg: null, viewBox: kept?.viewBox ?? null, heat: [], hover: null, pending: false, queued: null };
  const holder = pane.querySelector(".sheet"), redraw = () => S.svg.setAttribute("viewBox", S.viewBox.join(" "));
  let drag = null;
  holder.addEventListener("wheel", (e) => {
    if (!S.svg) return;
    e.preventDefault();
    const p = svgPoint(S, e), f = Math.exp(e.deltaY * 0.0015), vb = S.viewBox;
    S.viewBox = [p.x - (p.x - vb[0]) * f, p.y - (p.y - vb[1]) * f, vb[2] * f, vb[3] * f];
    redraw();
  }, { passive: false });
  holder.addEventListener("mousedown", (e) => { if (S.svg) drag = { x: e.clientX, y: e.clientY, vb: S.viewBox.slice(), moved: false }; });
  holder.addEventListener("mouseup", () => {
    const d = drag;
    drag = null;
    if (d && !d.moved && S.hover) selectMember(S.hover.member);
  });
  holder.addEventListener("dblclick", () => { if (S.svg) { S.viewBox = fitViews(S); redraw(); } });
  holder.addEventListener("mousemove", (e) => {
    if (!S.svg) return;
    if (drag && (drag.moved ||= Math.abs(e.clientX - drag.x) + Math.abs(e.clientY - drag.y) > 4)) {
      const k = px(S);
      S.viewBox = [drag.vb[0] - (e.clientX - drag.x) * k, drag.vb[1] - (e.clientY - drag.y) * k, drag.vb[2], drag.vb[3]];
      return redraw();
    }
    hoverAt(S, e);
  });
  holder.addEventListener("mouseleave", () => { drag = null; S.queued = null; S.hover = null; tip(null); drawHover(S); });
  pane.querySelector(".export").onclick = (e) => { if (model.built) exportMenu(S, e); };
  loadSheet(S);
  return Object.assign(S, { keep: () => ({ viewBox: S.viewBox }), update: (k) => { if (k?.viewBox && S.svg) { S.viewBox = k.viewBox; redraw(); } } });
}
const sheetOf = (number) => (model.state?.sheets ?? []).find((x) => x.number === number);
defineType("sheet", {
  title: (number) => `${number} ${sheetOf(number)?.title ?? ""}`.trim(),
  label: (number) => `<b>${esc(number)}</b> ${esc(sheetOf(number)?.title ?? "")}`,
  make: makeSheet,
});
