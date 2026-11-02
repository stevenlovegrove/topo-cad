// The machines, as the page knows them: those `topo serve` knows (none where the app is not served), adding one
// (found on the network, as xTool Studio finds it, or typed), and how each is and what each has been given to do (its
// queue of jobs, each a step at a time, cancelled from here). A machine is asked how it is only while something
// watches it (its row open in the Machines panel, a nesting laid out on its bed, work it has not finished): a
// machine that is off, or not looked at, is not connected to. Each has its own queue: a laser can cut while a
// printer prints.

import { esc } from "./ui.js";
import { ago, dotOf, statusHtml } from "./machine-status.js";

const fmt = (x) => (Math.round(x * 10) / 10).toFixed(1);
const url = (path) => new URL(`machine/${path}`, location.href).href;

/** A request to `topo serve`'s machines: what it answers, or an error with what it said. */
export async function api(path, opts = {}) {
  const r = await fetch(url(path), opts);
  const v = await r.json().catch(() => ({ error: `${r.status} ${r.statusText}` }));
  if (!r.ok || v?.error) throw new Error(v?.error ?? `${r.status}`);
  return v;
}

/** The machines known (`[{id, kind, host, name}]`); null where the app is not served (no machines here at all). */
export async function machines() {
  try { const r = await fetch(url("list")); return r.ok ? await r.json() : null; } catch { return null; }
}

/** A bed's size by the machine's kind, mm (across x, along y as its software draws it). */
export const BEDS = { metalfab: [610, 610] };   // topo_machine::metalfab::BED_MM

/** Adds a MetalFab: its address looked for on the network first (as xTool Studio looks), else typed. Its id, or null. */
export async function addMetalFab(known) {
  let found = [];
  // (Again, longer, if none answered: a first ask from a process can be lost while macOS lets it onto the local network.)
  for (const s of [2.5, 4]) {
    try { found = (await api(`discover?seconds=${s}`)).filter?.((f) => f.code === "HJ003" || !f.code) ?? []; } catch {}
    if (found.length) break;
  }
  const list = found.map((f) => `${f.ip}  ${f.name || "?"} ${f.serial ? `(${f.serial})` : ""} over ${f.channel}`).join("\n");
  const host = prompt(found.length ? `Found on the network:\n${list}\n\nThe MetalFab's address:` : "No MetalFab answered on the network. Its address (e.g. 192.168.4.100):", found[0]?.ip ?? "");
  if (!host) return null;
  const id = `metalfab-${host.replace(/[^0-9a-z]+/gi, "-")}`;
  await api("list", { method: "PUT", body: JSON.stringify([...known.filter((m) => m.id !== id), { id, kind: "metalfab", host, name: `MetalFab ${host}` }]) });
  await reloadMachines();
  return id;
}

// ----- the machines, watched -----
const shop = {
  list: null,                 // the machines known; null: not served (or not asked yet)
  asked: null,                // the list being asked for
  states: new Map(),          // id → how it was last said to be (`/machine/<id>/state`)
  watchers: new Map(),        // id → who watches it (a Set of names)
  timers: new Map(),          // id → the next asking
  listeners: new Set(),
};
const tell = () => { for (const f of shop.listeners) f(); };
/** `f()` whenever the machines, or how any watched one is, may have changed; what is given back stops it. */
export function onMachines(f) { shop.listeners.add(f); return () => shop.listeners.delete(f); }
/** The machines known, as last asked (null: not served, or not yet asked). */
export const known = () => shop.list;
/** How a machine was last said to be (null: not asked). */
export const stateOf = (id) => shop.states.get(id) ?? null;
/** The machines asked for again. */
export async function reloadMachines() {
  shop.asked = machines().then((l) => { shop.list = l; tell(); return l; });
  return shop.asked;
}
/** The machines, asked for once (and then as known). */
export const machinesKnown = () => shop.asked ?? reloadMachines();
/** Whether anything watches a machine (or it has work not finished). */
const watched = (id) => (shop.watchers.get(id)?.size ?? 0) > 0 || (stateOf(id)?.jobs ?? []).some((j) => !j.ended);
/** Watches a machine, by a name of the watcher's own: it is asked how it is from now on, until let go of. */
export function watch(id, who) {
  if (!id) return;
  if (!shop.watchers.has(id)) shop.watchers.set(id, new Set());
  shop.watchers.get(id).add(who);
  if (!shop.timers.has(id)) poll(id);
}
export function unwatch(id, who) { shop.watchers.get(id)?.delete(who); }
/** Asks a machine how it is now (and again soon while it works, less often while not, while it is watched). */
export async function poll(id) {
  clearTimeout(shop.timers.get(id));
  shop.timers.set(id, null);
  try { shop.states.set(id, await api(`${encodeURIComponent(id)}/state`)); }
  catch (e) { shop.states.set(id, { connected: false, error: e.message, jobs: stateOf(id)?.jobs ?? [] }); }
  tell();
  if (!watched(id)) { shop.timers.delete(id); return; }
  const M = stateOf(id), busy = (M?.jobs ?? []).some((j) => !j.ended);
  shop.timers.set(id, setTimeout(() => poll(id), busy ? 500 : M?.connected ? 2500 : 5000));
}
/** Cancels a machine's job. */
async function cancelJob(id, job) {
  try { await api(`${encodeURIComponent(id)}/jobs/${job}/cancel`, { method: "POST" }); } catch {}
  poll(id);
}

const took = (a, b) => (a ? `${(((b ?? Date.now()) - a) / 1000).toFixed(1)} s` : "");
const MARK = { waiting: "○", doing: "◐", done: "✓", failed: "✗", skipped: "–" };

/** How a machine is, as HTML: its card (what it is doing, where its head is, its readings and alarms) and its queue. */
export function machineHtml(id) {
  const M = stateOf(id);
  if (!M) return `<div class="mcard muted">Asking the machine…</div>`;
  const st = M.status ?? {};
  const head = st.head ? `x ${fmt(st.head[0])} · y ${fmt(st.head[1])} · z ${fmt(st.head[2])} mm` : "—";
  let out = `<div class="mcard">
    <div class="mrow muted">${esc([M.serial, M.host].filter(Boolean).join(" · "))}</div>
    ${M.connected ? `<div class="mrow">Doing: <b>${esc(st.doing || st.mode || "?")}</b></div><div class="mrow">Head: ${head}</div><div class="mrow muted">as of ${st.at ? ago(st.at) : "—"}</div>` : ""}
    ${M.error || st.error ? `<div class="mrow bad">${esc(M.error ?? st.error)}</div>` : ""}
    ${statusHtml(M)}
    <div class="mrow"><a href="capture.html" target="_blank">Bed survey (experiment) ↗</a></div>
  </div>`;
  const jobs = [...(M.jobs ?? [])].reverse();
  out += `<div class="mqueue"><div class="mtitle">Its work${jobs.length ? "" : `<span class="muted"> · nothing yet</span>`}</div>`;
  for (const j of jobs) {
    const pct = j.of ? Math.round((100 * j.done) / j.of) : 0, live = !j.ended;
    out += `<div class="mjob ${j.state}">
      <div class="mjobhead"><b>${esc(j.title)}</b><span class="chip ${j.state}">${j.cancelling && live ? "cancelling" : j.state}</span></div>
      <div class="mbar"><div style="width:${pct}%"></div></div>
      <div class="mrow muted">${j.done} of ${j.of} steps · ${j.began ? took(j.began, j.ended) : `queued ${ago(j.queued)}`}${live ? ` <button class="act cancel" data-machine="${esc(id)}" data-job="${j.id}" ${j.cancelling ? "disabled" : ""}>Cancel</button>` : ""}</div>
      ${j.error && j.state !== "cancelled" ? `<div class="mrow bad">${esc(j.error)}</div>` : ""}
      <ol class="msteps">${j.steps_from ? `<li class="muted">… ${j.steps_from} before</li>` : ""}${j.steps.map((s) => `<li class="${s.state}"><span class="mark">${MARK[s.state] ?? ""}</span>${esc(s.label)}<span class="muted">${s.began ? took(s.began, s.ended) : ""}</span></li>`).join("")}${j.of > (j.steps_from ?? 0) + j.steps.length ? `<li class="muted">… ${j.of - (j.steps_from ?? 0) - j.steps.length} more</li>` : ""}</ol>
    </div>`;
  }
  return out + `</div>`;
}
/** A machine's dot (as `machine-status.js` has it), or none where it is not asked. */
export const dotFor = (id) => (stateOf(id) ? dotOf(stateOf(id)) : "");
/** Cancel buttons in what `machineHtml` drew, made to cancel. */
export function wireCancels(box) {
  box.querySelectorAll(".cancel[data-job]").forEach((b) => (b.onclick = () => { b.disabled = true; cancelJob(b.dataset.machine, b.dataset.job); }));
}
