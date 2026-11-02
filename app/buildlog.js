// The build log: what the page and its engines did to show the project, step by step, with how long each took (times.js
// keeps it; each engine says its own steps, worker.js). A document of the dock (`log:`), opened from the header (what
// is being done now is said there, while it is) or the menu; and as text, to read or to paste (`topoLog()` in the
// console, or its Copy button).

import { $, esc } from "./ui.js";
import { model } from "./project.js";
import { defineType, openDoc } from "./shell/dock.js";
import { doing, frames, log, onLog } from "./times.js";

const secs = (ms) => (ms / 1000).toFixed(2);
const took = (r) => (r.running ? `${secs(r.ms)} s…` : r.ms >= 1000 ? `${secs(r.ms)} s` : `${Math.round(r.ms)} ms`);
/** Each source its colour (the page's thread grey). */
const COLOURS = { page: "#8a8f98", "model A": "#ff7a00", "model B": "#d9534f", sheets: "#1f5fbf", checks: "#2e8b57" };
const colour = (source) => COLOURS[source] ?? "#7a5cc2";

/** The log as text: one row a step, its start and how long it took, where, what. */
export function logText() {
  const rows = log();
  return [framesText(), ...rows.map((r) => `${secs(r.at).padStart(7)} s  ${(r.mark && !r.ms ? "" : took(r)).padStart(10)}  ${r.source.padEnd(8)}  ${r.what}`)].join("\n");
}
globalThis.topoLog = () => console.log(logText());
/** The 3D view's last frames, in words: on the page's thread, and from one to the next while it moves. */
function framesText() {
  const f = frames(), ms = (x) => `${x.toFixed(1)} ms`;
  if (!f.n) return "3D view: no frames drawn yet";
  const k = (x) => (x >= 1e6 ? `${(x / 1e6).toFixed(1)}M` : x >= 1e3 ? `${(x / 1e3).toFixed(0)}k` : `${Math.round(x)}`);
  return `3D view, its last ${f.n} frames: each ${ms(f.cpu.median)} on the page's thread (worst ${ms(f.cpu.worst)})` + (f.gap ? `; while it moves, ${ms(f.gap.median)} from one to the next (${(1000 / f.gap.median).toFixed(0)} a second; worst ${ms(f.gap.worst)})` : "")
    + (f.gpu ? `; the GPU ${ms(f.gpu.mean)} a frame (of the last ${f.gpu.frames})` : "")
    + (f.brep ? `; its B-rep shader a frame: ${k(f.brep.fragments)} pixels, ${k(f.brep.intersections)} surfaces met, ${k(f.brep.trim_loops)} trim loops, ${k(f.brep.trim_edges)} trim curves, ${k(f.brep.shaded)} shaded` : "");
}

function makeLog() {
  const pane = document.createElement("div");
  pane.className = "buildlog";
  pane.innerHTML = `<div class="bl-head"><span class="bl-sum"></span><label><input type="checkbox" data-a="long"> only steps of 100 ms or more</label><button data-a="copy" title="The log as text, to paste">Copy</button></div><div class="bl-rows"></div>`;
  const rowsEl = pane.querySelector(".bl-rows"), sumEl = pane.querySelector(".bl-sum"), longEl = pane.querySelector("[data-a=long]");
  pane.querySelector("[data-a=copy]").onclick = () => navigator.clipboard?.writeText(logText());
  longEl.onchange = () => render();
  function render() {
    const rows = log(), end = Math.max(1, ...rows.map((r) => r.at + r.ms));
    // (How long each source was at work, in all.)
    const busy = {};
    for (const r of rows) if (!r.mark) busy[r.source] = (busy[r.source] ?? 0) + r.ms;
    sumEl.title = framesText();
    sumEl.innerHTML = `${esc(framesText())}<br>${secs(end)} s in all · ` + Object.entries(busy).map(([s, ms]) => `<span style="color:${colour(s)}">${esc(s)}</span> ${secs(ms)} s`).join(" · ");
    const shown = longEl.checked ? rows.filter((r) => r.ms >= 100 || (r.mark && !r.ms)) : rows;
    rowsEl.innerHTML = shown.map((r) => {
      const left = (100 * r.at) / end, width = Math.max(0.3, (100 * r.ms) / end);
      const bar = r.mark && !r.ms ? `<i class="bl-mark" style="left:${left}%"></i>` : `<i style="left:${left}%;width:${width}%;background:${colour(r.source)}${r.running ? ";opacity:.6" : ""}"></i>`;
      return `<div class="bl-row${r.mark ? " bl-is-mark" : ""}" title="${esc(`${secs(r.at)} s: ${r.source}: ${r.what}`)}"><span class="bl-at">${secs(r.at)}</span><span class="bl-took">${r.mark && !r.ms ? "" : took(r)}</span><span class="bl-src" style="color:${colour(r.source)}">${esc(r.source)}</span><span class="bl-what">${esc(r.what)}</span><span class="bl-bar">${bar}</span></div>`;
    }).join("");
  }
  // (Kept up to date while it is open: and once a second, for the steps still running and the frames drawn.)
  onLog(render);
  setInterval(() => { if (pane.isConnected) render(); }, 1000);
  render();
  return { el: pane, render };
}
defineType("log", { title: () => "Build log", label: () => "Build log", make: makeLog });
export const openLog = () => openDoc("log:", { tab: true });

// In the header, while something is being done: what, by the model's engines first (the page's, while it waits for
// nothing else). A click opens the log.
function sayDoing() {
  const now = doing(), models = Object.entries(now).filter(([s]) => s.startsWith("model"));
  const [source, what] = models.at(-1) ?? Object.entries(now).find(([s]) => s === "page") ?? Object.entries(now)[0] ?? [];
  const s = $("solving");
  if (s && !s.hidden) s.lastChild.textContent = what ? `building · ${what}` : "building";
  // (Before the model is first built, the status says what is being done.)
  if (!model.state?.file && what) $("status").textContent = `${what}…`;
  if (s) s.title = what ? `${source}: ${what} (click: the build log)` : "the build log";
}
onLog(sayDoing);
$("solving").style.cursor = "pointer";
$("solving").onclick = openLog;
$("status").addEventListener("click", (e) => { if (!model.state?.file) { e.preventDefault(); openLog(); } });
$("buildLog").onclick = () => { openLog(); $("menu").hidden = true; };
