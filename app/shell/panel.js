// What the model says of itself, as tools of the dock: its checks (with what drawings and 3D views
// are coloured by), the member inspected (its diagrams and checks; of another kind of part, what
// it is) and its loads. And, over the dock, what must
// be said whatever is open: that the model does not build, what a save did.

import { $, esc } from "../ui.js";
import { inertiaSaid, kgSaid, notInIt, tally, weightSaid } from "../weight.js";
import { model } from "../project.js";
import { renderEditors } from "../editor.js";
import { addTool, bring } from "./dock.js";
import { renderLists } from "./browser.js";
import { drawHeats, loadHeats } from "../drawing.js";

// The tools: each a section of the page (index.html), by its name.
// (What is chosen, whatever it is, is told of in "Details": a member's diagrams and checks, a part's or a bolt's
// particulars. It stands over the others, in a stack of its own, so that it is in sight beside them.)
for (const [name, title, beside, over] of [["checks", "Checks"], ["member", "Details", null, "checks"], ["loads", "Loads", "checks"]])
  addTool(name, { title, el: document.querySelector(`.pane[data-pane="${name}"]`), side: "right", beside, over });
/** Brings one of the tools into sight (by its name: "member", "issues", …): its side is opened if it was put away. */
export const showTab = (name) => bring(`tool:${name}`);
/** Makes a tool the one its stack shows, where that is in sight: what was put away stays put away. */
const fillTab = (name) => bring(`tool:${name}`, { quiet: true });

/** A bar saying the model is being built again (`s`: `model.solving`; null: it is not, and none is shown). */
export function showSolving(el, s) {
  el.hidden = !s;
  el.classList.toggle("unsure", true);
  el.title = solvingText(s);
}
/** That it is being built, in words, and what waits for it. */
function solvingText(s) {
  if (!s) return "";
  return `building${s.queued ? `; ${s.queued} more change${s.queued === 1 ? "" : "s"} to build after` : ""}`;
}
/** In the page's header, by what the model is said to be: that it is being built again. */
export function renderSolving(s) {
  showSolving($("solving"), s);
  $("solving").lastChild.textContent = !s ? "" : `building${s.queued ? ` · ${s.queued} more to build` : ""}`;
}

export function renderPanel() {
  const s = model.state;
  $("title").textContent = s.name || (s.file ? "topo-cad" : "Projects");
  const banner = $("banner");
  banner.innerHTML = "";
  renderLists();
  // (The page of no project: the projects page is over it.)
  if (!s.file) {
    $("status").textContent = "no project open";
    return;
  }
  if (s.error) {
    banner.innerHTML = `<div class="err">${esc(s.error)}</div>`;
    $("status").textContent = "model error";
    renderEditors();
    return;
  }
  renderEditors();
  if (s.sheet_errors && s.sheet_errors.length) {
    banner.innerHTML += `<div class="warn"><b>Sheet problems</b><br>${s.sheet_errors.map(esc).join("<br>")}</div>`;
  }
  renderStructure();
  // (What is read of it, and how far it is from that, is the Params panel's to say.)
  const st = $("status");
  st.innerHTML = `<a data-tab="issues" title="Show the issues">${s.issues.length} issues</a>`;
  for (const a of st.querySelectorAll("a[data-tab]")) a.onclick = () => showTab(a.dataset.tab);
  // (The model's checks are listed with the workspace's issues, of which they are a category: app/scan.js.)
}

/** The member inspected (its path). */
export let inspected = null;
/** The colour of a ratio of use (demand over capacity): green through amber to red at 1, purple beyond. */
/** The heat of a ratio: its colour at each of these ratios (sRGB bytes), between them between theirs. The model's parts
 *  are drawn through the same, as a colormap. */
export const HEAT_STOPS = [[0, [43, 147, 72]], [0.5, [167, 201, 87]], [0.8, [244, 161, 0]], [1.0, [214, 40, 40]], [1.5, [123, 31, 162]]];
function heatRgb(r) {
  const stops = HEAT_STOPS;
  const x = Math.max(0, Math.min(1.5, r));
  const i = Math.max(1, stops.findIndex((s) => x <= s[0])), [a, ca] = stops[i - 1], [b, cb] = stops[i];
  return ca.map((c, k) => Math.round(c + ((cb[k] - c) * (x - a)) / (b - a)));
}
export const heatColor = (r) => `rgb(${heatRgb(r).join(",")})`;

function renderStructure() {
  const a = model.state.analysis;
  $("noStructure").hidden = !!a;
  if (!a) { $("structure").hidden = true; return; }
  $("structure").hidden = false;
  // Scenarios.
  const sc = model.state.scenarios || { available: [] };
  $("scenarioBox").hidden = sc.available.length < 2;
  const ss = $("scenario");
  ss.innerHTML = sc.available.map((x) => `<option ${x.name === sc.current ? "selected" : ""}>${esc(x.name)}</option>`).join("");
  const cur = sc.available.find((x) => x.name === sc.current);
  $("scenarioDesc").textContent = cur && cur.description ? cur.description : "";
  // Load cases and combinations.
  const L = model.state.loads;
  if (L) {
    const on = L.cases.filter((c) => !c.excluded).map((c) => c.name);
    $("loadsSummary").textContent = `— ${on.join(", ")}${L.cases.some((c) => c.excluded) ? " (some left out)" : ""}`;
    $("loadCases").innerHTML = `<div class="muted" style="font-size:11px;margin:4px 0">${esc(L.standard)}. Untick a case to leave it out of the checks.</div>` +
      L.cases.map((c) => `<div class="case ${c.excluded ? "off" : ""}"><label><input type="checkbox" data-case="${esc(c.name)}" ${c.excluded ? "" : "checked"}> ${esc(c.name)} <span class="muted" style="font-weight:400">${esc(c.kind)}</span></label><ul>${c.items.map((i) => `<li>${esc(i)}</li>`).join("")}</ul></div>`).join("");
    for (const box of $("loadCases").querySelectorAll("input[data-case]")) {
      box.onchange = async () => {
        const off = [...$("loadCases").querySelectorAll("input[data-case]")].filter((b) => !b.checked).map((b) => b.dataset.case);
        await model.exclude(off);
      };
    }
    const cd = L.duration_factors !== false;  // the wood load-duration factor, where it applies
    $("combos").innerHTML = `<tr><th>Combination</th><th>Factors</th>${cd ? "<th class=num>C<sub>D</sub></th>" : ""}</tr>` +
      L.combos.map((c) => `<tr><td>${esc(c.name)}</td><td>${c.factors.map(([n, f]) => `<span class="chip">${f === 1 ? "" : f + "×"}${esc(n)}</span>`).join("")}</td>${cd ? `<td class=num title="${esc(c.cd_why)}">${c.cd.toFixed(2)}</td>` : ""}</tr>`).join("");
  }
  $("asce").textContent = a.standard || `NDS 2018 ASD · ${a.combinations || "ASCE " + a.asce7}`;
  // (A pass is of what was checked: said so where some members were not, with which.)
  const all = !a.members || a.checked >= a.members;
  const over = a.over ? `<b class="bad">${a.over} over capacity</b>` : !a.checked ? `<b class="bad">nothing checked</b>` : `<b class="ok">${all ? "all" : "those"} within capacity</b>`;
  const left = all ? "" : ` · <span class="muted" title="${esc((a.unchecked || []).map((u) => `${u.count} ${u.role}`).join(", "))}: no load reaches them, or no check applies">${a.members - a.checked} not checked</span>`;
  const ind = a.indicative_over ? ` · <span class="muted" title="Members designed by others (e.g. manufactured trusses): checked for information only">${a.indicative_over} indicative over 1.0</span>` : "";
  $("strSummary").innerHTML = `${a.checked}${all ? "" : ` of ${a.members}`} members checked${left} · ${over}${ind} · max ratio ${Number.isFinite(a.max) ? a.max.toFixed(2) : "none"}` +
    (a.unverified ? `<span class="badge" title="Some reference values are not yet verified against their source">unverified data</span>` : "") +
    (a.displacement ? `<div style="margin-top:4px">Largest displacement ${a.displacement.mm.toFixed(2)} mm <span class="muted">at ${esc(a.displacement.member)} (${esc(a.displacement.combination)})</span></div>` : "") +
    (a.issues.length ? `<div class="muted" style="margin-top:4px">${a.issues.map(esc).join("<br>")}</div>` : "");
  const sel = $("heatCombo"), prev = sel.value;
  sel.innerHTML = `<option value="">governing (all)</option>` + a.combos.map((c) => `<option>${esc(c)}</option>`).join("");
  if ([...sel.options].some((o) => o.value === prev)) sel.value = prev;
  $("worst").innerHTML = a.worst.map((w) => `<li data-path="${esc(w.path)}"><span class="ratio" style="color:${heatColor(w.ratio ?? 99)}">${Number.isFinite(w.ratio) ? w.ratio.toFixed(2) : "n/a"}</span><span>${esc(w.label)} <span class="muted">— ${esc(w.title)} (${esc(w.combination)})${w.indicative ? " · indicative" : ""}</span></span></li>`).join("");
  for (const li of $("worst").children) li.onclick = () => inspect(li.dataset.path);
  if (inspected) inspect(inspected, true);
}

$("heatMode").addEventListener("change", loadHeats);
$("scenario").addEventListener("change", async (e) => { await model.scenario(e.target.value); });
$("heatCombo").addEventListener("change", () => { loadHeats(); if (inspected) inspect(inspected, true); });

function chart(t, v, title, unit, color, down) {
  if (!v.length || v.every((x) => Math.abs(x) < 5e-4)) return "";  // nothing but rounding noise
  const shown = v;
  if (down) v = v.map((x) => -x);  // draw downward-positive values below the axis
  const w = 380, h = 70, pad = 4;
  const t0 = t[0], t1 = t[t.length - 1] || 1;
  const vmax = Math.max(...v.map(Math.abs)) || 1;
  const x = (tt) => pad + (w - 2 * pad) * (tt - t0) / ((t1 - t0) || 1);
  const y = (vv) => h / 2 - (h / 2 - pad) * vv / vmax;
  const pts = t.map((tt, i) => `${x(tt).toFixed(1)},${y(v[i]).toFixed(1)}`).join(" ");
  const i = v.reduce((k, vv, j) => (Math.abs(vv) > Math.abs(v[k]) ? j : k), 0);
  return `<div class="muted" style="font-size:11px;margin-top:6px">${title}: max ${fmtNum(shown[i])} ${unit} at ${fmtNum(t[i])} ${chartLen}</div>
    <svg class="chart" viewBox="0 0 ${w} ${h}" preserveAspectRatio="none">
      <line x1="0" x2="${w}" y1="${h / 2}" y2="${h / 2}" stroke="#bbb" stroke-width="1"/>
      <polygon points="${x(t0)},${h / 2} ${pts} ${x(t1)},${h / 2}" fill="${color}" fill-opacity="0.18"/>
      <polyline points="${pts}" fill="none" stroke="${color}" stroke-width="1.5"/>
    </svg>`;
}
let chartLen = "ft";
function fmtNum(v) { const a = Math.abs(v); return a >= 100 ? v.toFixed(0) : a >= 10 ? v.toFixed(1) : v.toFixed(2); }

/**
 * What is said of anything chosen, the same of each: a member, another of the model's bodies, or a group of any of
 * them. What it is, what it is made of, what it weighs, where the middle
 * of its weight is, and how big it is (the box it is in, along the model's x, y and z). Then, under it, what is
 * its kind's own to say.
 */
function common(parts, name, path, metric, whole) {
  const len = (x) => (metric === false ? `${(x / 0.0254).toFixed(2)}` : `${(x * 1000).toFixed(1)}`), unit = metric === false ? " in" : " mm";
  const t = tally(parts), lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity], of = new Set();
  for (const m of parts) {
    const a = m.about ?? {};
    if (a.of) of.add(a.of);
    // (The box it is in, as the engine says; of a group, the box they are all in.)
    if (a.bounds) for (let k = 0; k < 3; k++) { lo[k] = Math.min(lo[k], a.bounds[0][k]); hi[k] = Math.max(hi[k], a.bounds[1][k]); }
  }
  // (`whole`: a part that is its group, said with what is under it: one thing, weighed and sized with all of it.)
  const one = whole ?? (parts.length === 1 ? parts[0] : null), a = one?.about ?? {};
  const what = !one ? `Group of ${parts.length} parts`
    : a.kind === "member" ? `Member${one.role ? `: ${one.role.replace(/_/g, " ")}` : ""}${a.section ? `, ${a.section}` : ""}`
    : `${a.what ? `${a.what[0].toUpperCase()}${a.what.slice(1)}` : "Part"}${one.role ? `: ${one.role.replace(/_/g, " ")}` : ""}`;
  const made = [...of];
  // (Of one thing that has no weight of its own, why.)
  const weight = one && !(t.kg > 0) ? { with_whole: whole ? "nothing says" : "with the part it is a piece of", excluded: "left out on purpose", unknown: "nothing says" }[a.how] ?? "—" : weightSaid(t, metric);
  const rows = [
    ["What", what],
    ["Made of", made.length ? (made.length > 3 ? `${made.slice(0, 3).join(", ")} and ${made.length - 3} more` : made.join(", ")) : "—"],
    ["Weight", weight],
    ["Middle of its weight", t.at ? `${t.at.map(len).join(", ")}${unit}` : "—"],
    ["Size", lo[0] <= hi[0] ? `${[0, 1, 2].map((k) => len(hi[k] - lo[k])).join(" × ")}${unit}` : "—"],
    // (About the middle of its weight, along the model's x, y and z. Where some of it says only a weight, at least that.)
    ["Moments of inertia", t.inertia && !(one && t.asPoints) ? `${t.asPoints ? "at least " : ""}${inertiaSaid(t.inertia, metric)}` : "—"],
  ];
  const spread = !one && t.asPoints ? ` ${t.asPoints} of them say what they weigh and not how it is spread: each is counted as if its weight were all at its middle.` : "";
  const why = !one ? `${notInIt(t)}${spread}`.trim() : "";
  const note = why ? `<div class="muted" style="margin-top:4px">${esc(why)}</div>` : "";
  return `<div class="head"><b>${esc(name)}</b><br><span class="muted">${esc(path)}</span></div><table class="common" style="margin-top:8px">${rows.map(([k, v]) => `<tr><td class="muted" style="padding-right:10px">${esc(k)}</td><td>${esc(v)}</td></tr>`).join("")}</table>${note}`;
}
const detail = (rows) => (rows.length ? `<table style="margin-top:8px">${rows.map(([k, v]) => `<tr><td class="muted" style="padding-right:10px">${esc(k)}</td><td>${esc(v)}</td></tr>`).join("")}</table>` : "");

/** What a part that is not a member is (a sheet, a block, a part bought), in the member's tab. */
export async function describe(m, withIts = [m]) {
  inspected = m.path;
  fillTab("member");
  const metric = (await model.parts()).stats?.metric;
  // (A part that is its group is said with its shapes and its parts: what it says it weighs, or they do.)
  $("inspector").innerHTML = common(withIts, m.label, m.path, metric, withIts.length > 1 ? m : undefined) + `<div class="muted" style="margin-top:8px">It is not checked as a member: its weight is carried by what it is on.</div>`;
}

/**
 * What a group of the model's parts is, in Details: how many, and what they weigh together (`w`: `weightOf`
 * of them), with where the middle of that weight is.
 */
export async function describeGroup(path, parts, w) {
  inspected = null;
  fillTab("member");
  const metric = (await model.parts()).stats?.metric;
  const kg = (x) => kgSaid(x, metric);
  // (Under what is said of everything: the heaviest of what is in it, by what is next below it.)
  const under = new Map();
  for (const m of parts) { const k = m.path.slice(m.path.indexOf(`/${path}/`) + path.length + 2).split("/")[0]; if (m.about?.kg > 0) under.set(k, (under.get(k) ?? 0) + m.about.kg); }
  const most = [...under].sort((a, b) => b[1] - a[1]).slice(0, 8);
  $("inspector").innerHTML = common(parts, path.split("/").pop(), path, metric) +
    (most.length > 1 ? `<table style="margin-top:8px">${most.map(([k, v]) => `<tr><td style="padding-right:10px">${esc(k)}</td><td class="num">${esc(kg(v))}</td></tr>`).join("")}</table>` : "");
}

/**
 * What a thing of the scene that is not a part of the model is (a slab, a polyline, a plane, a selection,
 * a feature of a part), in Details: `rows` said of everything (object.js), then `more`, its kind's own.
 */
export function describeThing({ name, id, rows, more, note, actions, mount }) {
  inspected = null;
  fillTab("member");
  // (What can be done with it: each a button; or, where it is a setting (`field`: { value, step, min, max, unit }), its
  // label and a number to change, done as it changes.)
  const act = (a, k) => a.field
    ? `<label class="setting" title="${esc(a.title ?? "")}">${esc(a.label)} <input type="number" data-act="${k}" value="${a.field.value}" step="${a.field.step ?? "any"}" ${a.field.min != null ? `min="${a.field.min}"` : ""} ${a.field.max != null ? `max="${a.field.max}"` : ""}> ${esc(a.field.unit ?? "")}</label>`
    : `<button class="mini" data-act="${k}" title="${esc(a.title ?? "")}">${esc(a.label)}</button>`;
  $("inspector").innerHTML = `<div class="head"><b>${esc(name)}</b>${id ? `<br><span class="muted">${esc(id)}</span>` : ""}</div>${detail(rows).replace("<table", '<table class="common"')}${detail(more)}${note ? `<div class="muted" style="margin-top:8px">${esc(note)}</div>` : ""}${actions?.length ? `<div class="acts" style="margin-top:8px">${actions.map(act).join(" ")}</div>` : ""}`;
  for (const b of $("inspector").querySelectorAll("button[data-act]")) b.onclick = () => actions[+b.dataset.act].run();
  for (const i of $("inspector").querySelectorAll("input[data-act]")) {
    i.onkeydown = (e) => { e.stopPropagation(); if (e.key === "Enter") i.blur(); };
    i.onchange = () => { if (i.value !== "" && Number.isFinite(Number(i.value))) actions[+i.dataset.act].run(Number(i.value)); };
  }
  // (What it shows of its own there, beside what is said: a slider.)
  mount?.($("inspector"));
}

/** Nothing is inspected (nothing is chosen). */
export function forgetInspected() {
  inspected = null;
  $("inspector").innerHTML = `<div class="muted empty">Click anything in a drawing, a 3D view or the object tree to see what it is: a member's diagrams and checks, a part's or a bolt's particulars.</div>`;
}

export async function inspect(path, keep) {
  inspected = path;
  if (!keep) fillTab("member");
  const r = await model.ask("member", { path, combo: $("heatCombo").value });
  if (r.error) { $("inspector").innerHTML = `<div class="err">${esc(r.error)}</div>`; return; }
  const sup = r.supports.map((s) => `${s.by} at ${s.at}${s.transfer === "Fastened" ? " (nails only)" : ""}`).join("; ");
  const d = r.diagram, u = r.units, frame = r.behaviour === "frame";
  chartLen = u.t;
  // What is said of everything first (its weight and size among it); then what is a member's own.
  const all = await model.parts(), part = all.members?.find((x) => x.path === path);
  let html = (part ? common([part], r.label, path, all.stats?.metric) : `<div class="head"><b>${esc(r.label)}</b></div>`) +
    detail([["Cut length", r.length], ["Carries load as", r.behaviour], [frame ? "Supports" : "Supported by", sup || "—"]]);
  html += `<div class="muted" style="font-size:11px;margin-top:6px">Diagrams for ${esc(r.combination)}</div>`;
  html += frame
    ? chart(d.t, d.m, "Moment about y (bending along the section's z)", u.m, "#1d4ed8") + chart(d.t, d.m2, "Moment about z", u.m, "#4f83e8") + chart(d.t, d.tq, "Torque", u.m, "#b35900") +
      chart(d.t, d.v, "Shear along z", u.f, "#2b9348") + chart(d.t, d.v2, "Shear along y", u.f, "#6fbf73") + chart(d.t, d.n, "Axial (tension +)", u.f, "#8a2be2") + chart(d.t, d.disp, "Displacement", u.y, "#d62828")
    : chart(d.t, d.m, "Moment", u.m, "#1d4ed8") + chart(d.t, d.v, "Shear", u.f, "#2b9348") + chart(d.t, d.n, "Axial (tension +)", u.f, "#8a2be2") + chart(d.t, d.y, "Deflection (downward)", u.y, "#d62828", true);
  html += `<h3 style="margin-top:12px">Checks</h3>`;
  r.checks.forEach((c, k) => {
    const pct = Math.min(100, c.ratio * 100 / 1.5);
    html += `<div class="chk"><div class="top" data-k="${k}"><span>${esc(c.title)} <span class="muted">${esc(c.combination)}${c.other ? " · with " + esc(c.other) : ""}</span>${c.verified ? "" : `<span class="badge">unverified data</span>`}${c.indicative ? `<span class="badge" style="background:#eef">indicative</span>` : ""}</span>
      <span class="ratio" style="color:${heatColor(c.ratio)}">${c.ratio.toFixed(2)}</span></div>
      <div class="bar"><i style="width:${pct}%;background:${heatColor(c.ratio)}"></i></div>
      <div class="muted">${fmtNum(c.demand)} / ${fmtNum(c.capacity)} ${esc(c.unit)} · ${esc(c.clause)}</div>
      <div class="trace" hidden><table>${c.trace.map((s) => `<tr><td><b>${esc(s.symbol)}</b></td><td>${esc(s.formula)}${s.substituted ? ` <span class="muted">[${esc(s.substituted)}]</span>` : ""}</td><td class="num">${fmtNum(s.value)} ${esc(s.unit)}</td><td class="muted">${esc(s.reference || "")}</td></tr>`).join("")}</table>
      ${c.notes.map((n) => `<div class="note">${esc(n)}</div>`).join("")}</div></div>`;
  });
  $("inspector").innerHTML = html;
  for (const t of $("inspector").querySelectorAll(".chk .top")) t.onclick = () => { const tr = t.parentElement.querySelector(".trace"); tr.hidden = !tr.hidden; };
  drawHeats();
  if (!keep) $("inspector").scrollIntoView({ behavior: "smooth", block: "start" });
}

/** Says something beside the model's own status, under a name (none: no longer): what a workspace is doing, say. */
export function say(name, text) {
  let e = $(`status-${name}`);
  if (!e) { e = document.createElement("span"); e.id = `status-${name}`; e.className = "also"; $("status").after(e); }
  e.textContent = text ? `· ${text}` : "";
}

/** Says something above the documents, to be dismissed (×); what went well goes by itself after a while,
 * what went wrong stays until dismissed or said over. None: nothing. */
let msgTimer = null;
export function msg(text, isErr) {
  const m = $("msg");
  clearTimeout(msgTimer);
  m.className = text ? (isErr ? "err" : "ok") : "";
  m.textContent = "";
  if (!text) return;
  const t = document.createElement("span");
  t.textContent = text;
  const x = document.createElement("button");
  x.className = "dismiss";
  x.title = "Dismiss";
  x.textContent = "×";
  x.onclick = () => msg(null);
  m.append(t, x);
  if (!isErr) msgTimer = setTimeout(() => msg(null), 8000);
}
