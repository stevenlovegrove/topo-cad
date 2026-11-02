// How a machine is, drawn: its alarms active now first, what can be read of it (each judged ok or not),
// and, folded away, its alarms earlier since it was switched on, what it has said lately, and what it is
// (firmware, use). From `topo serve`'s /machine/<id>/state (topo_machine::status). Used by the layout
// pane and the survey page; it brings its own styles.

const esc = (s) => String(s ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
/** How long ago a time (ms) was, said. */
export const ago = (t) => { const s = Math.max(0, (Date.now() - t) / 1000); return s < 60 ? `${s.toFixed(0)} s ago` : s < 3600 ? `${(s / 60).toFixed(0)} min ago` : `${(s / 3600).toFixed(1)} h ago`; };
const LEVEL = { fatal: "Fatal", error: "Error", warning: "Warning", notice: "Notice" };

const CSS = `
.mstatus { display: grid; gap: 4px; margin-top: 4px; }
.mstatus .malarm { border-radius: 4px; padding: 4px 6px; display: flex; gap: 6px; align-items: baseline; }
.mstatus .malarm.fatal, .mstatus .malarm.error { background: #f8d7d3; color: #8e2a1e; }
.mstatus .malarm.warning { background: #fff1c7; color: #6b4b00; }
.mstatus .malarm.notice { background: #eef2f8; color: #33415c; }
.mstatus .malarm .lvl { font-size: 10px; text-transform: uppercase; letter-spacing: .03em; font-weight: 600; flex: none; }
.mstatus .malarm .src { margin-left: auto; font-size: 10px; opacity: .7; font-family: ui-monospace, monospace; }
.mstatus .mreadings { display: grid; grid-template-columns: auto 1fr; gap: 1px 8px; }
.mstatus .mreadings dt { color: var(--muted, #6b6b66); }
.mstatus .mreadings dd { margin: 0; }
.mstatus .mreadings dd.ok::before, .mstatus .mreadings dd.warn::before, .mstatus .mreadings dd.bad::before { content: "●"; margin-right: 4px; font-size: 9px; vertical-align: 1px; }
.mstatus .mreadings dd.ok::before { color: #2b9348; } .mstatus .mreadings dd.warn::before { color: #e6a700; } .mstatus .mreadings dd.bad::before { color: #c0392b; }
.mstatus .mreadings dd.warn { color: #6b4b00; } .mstatus .mreadings dd.bad { color: #8e2a1e; font-weight: 600; }
.mstatus details { color: var(--muted, #6b6b66); }
.mstatus details summary { cursor: pointer; }
.mstatus details ul { margin: 2px 0 2px 14px; padding: 0; }
.mstatus .mnote { color: var(--muted, #6b6b66); font-size: 11px; }
`;
let styled = false;
function style() {
  if (styled) return;
  styled = true;
  const s = document.createElement("style");
  s.textContent = CSS;
  document.head.append(s);
  // (Drawn again as the machine says more: what is open stays open.)
  document.addEventListener("toggle", (e) => {
    const k = e.target?.dataset?.fold;
    if (k && e.target.closest?.(".mstatus")) e.target.open ? opened.add(k) : opened.delete(k);
  }, true);
}
/** Which folded parts are open (by name): kept across drawing. */
const opened = new Set();
const fold = (k, summary, body) => `<details data-fold="${k}"${opened.has(k) ? " open" : ""}><summary>${summary}</summary>${body}</details>`;

/** The dot's class for how it is: ok, busy (working, or something to look at), bad. */
export function dotOf(M) {
  const st = M?.status ?? {};
  if (!M?.connected || st.error) return "bad";
  if (st.worst === "bad") return "bad";
  if (st.worst === "warn") return "busy";
  return st.mode === "P_IDLE" || st.mode === "P_WORK_DONE" || st.mode === "P_SLEEP" ? "ok" : "busy";
}

/** The machine's status as HTML (its readings and alarms; nothing if it said none). */
export function statusHtml(M) {
  style();
  const st = M?.status;
  if (!M?.connected || !st) return "";
  const active = (st.alarms ?? []).filter((a) => a.active), earlier = (st.alarms ?? []).filter((a) => !a.active);
  const alarm = (a) => `<div class="malarm ${esc(a.level)}"><span class="lvl">${LEVEL[a.level] ?? esc(a.level)}</span><span>${esc(a.text)}${a.info && !/^E\d+$/.test(a.info) ? ` (${esc(a.info)})` : ""}</span><span class="src" title="as the machine says it">${esc(a.source)} ${esc(a.kind)}${a.info ? ` ${esc(a.info)}` : ""}</span></div>`;
  const line = (a) => `<li>${esc(a.text)}${a.at ? ` · ${ago(a.at)}` : ""} <span class="mnote">${esc(a.level)}</span></li>`;
  let out = `<div class="mstatus">`;
  out += active.map(alarm).join("");
  if (st.readings?.length) out += `<dl class="mreadings">${st.readings.map((r) => `<dt>${esc(r.label)}</dt><dd class="${esc(r.judged)}">${esc(r.text)}</dd>`).join("")}</dl>`;
  if (earlier.length) out += fold("earlier", `Earlier since switched on (${earlier.length})`, `<ul>${earlier.map(line).join("")}</ul>`);
  const lately = (st.events ?? []).slice().reverse();
  if (lately.length) out += fold("lately", `Said lately (${lately.length})`, `<ul>${lately.slice(0, 20).map(line).join("")}</ul>`);
  const ab = M.about, fw = ab?.firmware, use = ab?.use;
  if (fw || use) {
    const hours = (s) => (s == null ? "—" : `${(Number(s) / 3600).toFixed(1)} h`);
    out += fold("about", "About it", `<dl class="mreadings">
      ${fw?.package_version ? `<dt>Firmware</dt><dd>${esc(fw.package_version)}</dd>` : ""}
      ${fw?.build_time ? `<dt>Built</dt><dd>${esc(fw.build_time)}</dd>` : ""}
      ${use ? `<dt>Jobs</dt><dd>${(use.numOnlineWorking ?? 0) + (use.numOfflineWorking ?? 0)}</dd><dt>Working</dt><dd>${hours(use.timeModeWorking)}</dd><dt>Switched on</dt><dd>${hours(use.timeSystemWork)}</dd>` : ""}
      ${["main", "deep"].filter((c) => ab.cameras?.[c]?.rms_mean != null).map((c) => `<dt>${c === "main" ? "Bed" : "Head"} camera</dt><dd>${ab.cameras[c].image_width}×${ab.cameras[c].image_height}, calibrated to ${Number(ab.cameras[c].rms_mean).toFixed(2)} px</dd>`).join("")}
    </dl>`);
  }
  if (st.errors?.length && !st.error) out += `<div class="mnote">Not said: ${esc(st.errors.join("; "))}</div>`;
  out += `<div class="mnote">The lights and the cameras' exposure cannot be read back from it. Its gas pressure is not asked for: it says it, every 2 s or so.</div>`;
  return out + `</div>`;
}
