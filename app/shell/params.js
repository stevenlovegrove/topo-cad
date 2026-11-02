// The Params tool (docs/part-core.md, "What is built"): of a part script, each param as it was found; what decides
// it (what must hold, a level, nothing); what the script says of it, and how firmly; what is kept of it in a layer
// over the script (docs/layers.md: yours, a draft, in italics and kept for everyone by ↑; or the model's own), and how
// firmly; and how far it is from what is said of it (red where it gave way). Its level is chosen beside it: as the script says, required (a must), measured, held, dragged, a hint, a
// fallback, free, or any level between; and how near (±) it is weighed against others at its level. A value dragged
// across keeps it there (held, unless another level is chosen), and the model is built again; × lets it go, and what
// the script says of it is where it is again. What is decided by what must hold is shown so, and not moved. What is
// kept of params the script no longer makes is listed, to be let go; and the readings kept. Of another model, nothing.

import { $, esc } from "../ui.js";
import { model } from "../project.js";
import { msg } from "./panel.js";
import { addTool } from "./dock.js";

addTool("params", { title: "Params", el: document.querySelector('.pane[data-pane="params"]'), side: "right", beside: "checks" });

/** The levels named, the firmest first, as the select offers them: its value, and its name. */
const STOPS = [["must", "required"], ["80", "measured"], ["70", "held"], ["60", "dragged"], ["40", "hint"], ["20", "fallback"], ["start", "free"]];
/** A tier as the engine has it (`"must"`, `{ at: { level, within } }`, `"start"`), from one as it is said (`{ name,
 *  level, within }`) or a select's value, to within `within` where it is at a level. */
function engineTier(t, within) {
  const v = typeof t === "string" ? t : t.name === "must" ? "must" : t.name === "free" ? "start" : String(t.level);
  return v === "must" || v === "start" ? v : { at: { level: Number(v), within: within ?? t.within ?? 1 } };
}
/** A tier as it is said, in a select's value. */
const selectValue = (t) => (t.name === "must" ? "must" : t.name === "free" ? "start" : String(t.level));

/** How a value is shown, by its kind (`measure`, docs/units.md) in the project's units (`units`): the engine has a
 *  length in millimetres, an angle in degrees, everything else in SI. Its text; how far a pixel of drag moves it (in
 *  the engine's units); what one of the unit it is typed in is, in the engine's; and that unit's name. Of no kind
 *  (a script of before), as a number. */
function shown(measure, units) {
  const plain = (v) => v.toFixed(Math.abs(v) < 10 ? 3 : 1);
  if (measure === "length" && units === "imperial") return { text: feetInches, step: 25.4 / 16, unit: 25.4, name: "in" };
  if (measure === "length") return { text: (v) => `${v.toFixed(1)} mm`, step: 1, unit: 1, name: "mm" };
  if (measure === "angle") return { text: (v) => `${v.toFixed(2)}°`, step: 0.1, unit: 1, name: "°" };
  if (measure === "ratio") return { text: (v) => v.toFixed(4), step: 0.001, unit: 1, name: "" };
  return { text: plain, step: 0.01, unit: 1, name: "" };
}
/** A length in millimetres as feet and inches, to a sixteenth: `8' 3 5/16"`. */
function feetInches(v) {
  const sign = v < 0 ? "-" : "";
  let sixteenths = Math.round((Math.abs(v) / 25.4) * 16);
  const feet = Math.floor(sixteenths / 192);
  sixteenths -= feet * 192;
  const inches = Math.floor(sixteenths / 16), part = sixteenths % 16;
  const g = (a, b) => (b ? g(b, a % b) : a), d = g(part, 16);
  const frac = part ? ` ${part / d}/${16 / d}` : "";
  return feet ? `${sign}${feet}' ${inches}${frac}"` : `${sign}${inches}${frac}"`;
}

export async function renderParams() {
  const table = $("params"), orphaned = $("paramsOrphaned");
  if (!table) return;
  const p = model.state && !model.state.error ? await model.ask("params") : null;
  if (!p || p.error || !Array.isArray(p.params)) {
    table.innerHTML = `<tr><td class="muted">${p?.error ? esc(p.error) : "Not a part script."}</td></tr>`;
    orphaned.textContent = "";
    $("paramsRead").textContent = "";
    return;
  }
  const keep = async (address, value, tier) => {
    const w = await model.params({ [address]: tier === null ? null : { value, tier } });
    if (w.error) msg(w.error, true);
  };
  table.innerHTML = `<tr><th>Param</th><th class=num>Value</th><th title="What decides it now">By</th><th title="How firmly it is said: as the script says, or kept beside it">Level</th><th class=num title="How near it is weighed against others at its level">±</th><th></th></tr>`;
  for (const r of p.params) {
    const tr = document.createElement("tr");
    const kept = r.kept, as = shown(r.measure, p.units);
    // (Decided by what must hold, or held as a part: not a person's to move here.)
    const fixed = r.by === "must" || r.by === "part held";
    const said = kept?.tier ?? r.own?.tier;
    const off = r.off != null && r.off > 1e-6 * Math.max(1, Math.abs(r.value));
    const options = [`<option value="script">${esc(r.own ? `script: ${r.own.tier.name}` : "script: free")}</option>`]
      .concat(STOPS.map(([v, n]) => `<option value="${v}">${n}</option>`))
      .concat(kept && !STOPS.some(([v]) => v === selectValue(kept.tier)) ? [`<option value="${selectValue(kept.tier)}">${esc(kept.tier.name)}</option>`] : [])
      .concat([`<option value="custom">level…</option>`]);
    tr.innerHTML = `<td title="${esc(r.address)}" style="${kept ? "font-weight:600" : ""}${kept?.yours ? ";font-style:italic" : ""}">${esc(r.address)}</td>`
      + `<td class="num${fixed ? " muted" : " drag"}" title="${fixed ? "Decided by what must hold: not moved here" : "Drag across to keep it there"}">${esc(as.text(r.value))}</td>`
      + `<td class="${r.by === "free" || fixed ? "muted" : ""}">${fixed ? "🔒 " : ""}${esc(r.by)}${off ? `<div style="color:var(--bad);font-weight:600;font-size:11px" title="It gave way to firmer things: ${esc(as.text(r.value))}, where ${kept ? "what is kept" : "the script"} says ${esc(as.text((kept ?? r.own).value))}">off ${esc(as.text(r.off))}</div>` : ""}</td>`
      + `<td><select class="mini" style="width:7.5em" ${fixed ? "disabled" : ""}>${options.join("")}</select></td>`
      + `<td class="num">${said?.within != null && kept ? `<input type="number" step="any" style="width:4em" value="${+(said.within / as.unit).toPrecision(4)}" title="To within (${esc(as.name)})">` : said?.within != null ? `<span class="muted">${esc(String(+(said.within / as.unit).toPrecision(4)))}</span>` : ""}</td>`
      + `<td style="white-space:nowrap">${kept?.yours ? `<button class="mini" data-share title="Yours (a draft in your layer over the model): keep it for everyone, in the model's own generated file">↑</button>` : ""}${kept ? `<button class="mini" data-let title="Let go of what is kept of it: as the script says">×</button>` : ""}</td>`;
    if (off) tr.style.background = "color-mix(in srgb, var(--bad) 8%, transparent)";
    const select = tr.querySelector("select");
    select.value = kept ? selectValue(kept.tier) : "script";
    select.onchange = async () => {
      let v = select.value;
      if (v === "script") return keep(r.address, r.value, null);
      if (v === "custom") {
        const n = Number(prompt("Its level: a whole number (measured 80, held 70, dragged 60, hint 40, fallback 20)", kept?.tier.level ?? 70));
        if (!Number.isInteger(n)) return (select.value = kept ? selectValue(kept.tier) : "script");
        v = String(n);
      }
      return keep(r.address, kept?.value ?? r.value, engineTier(v, kept?.tier.within));
    };
    const within = tr.querySelector("input");
    if (within) within.onchange = () => {
      const w = Number(within.value) * as.unit;
      if (!(w > 0)) return msg("How near it is weighed is more than none.", true);
      return keep(r.address, kept.value, engineTier(kept.tier, w));
    };
    const cell = tr.children[1];
    if (!fixed) {
      cell.style.cursor = "ew-resize";
      cell.onpointerdown = (e) => {
        const x0 = e.clientX;
        let value = r.value;
        try { cell.setPointerCapture(e.pointerId); } catch {}
        cell.onpointermove = (m) => {
          value = r.value + Math.round(m.clientX - x0) * as.step;
          cell.textContent = as.text(value);
        };
        cell.onpointerup = async () => {
          cell.onpointermove = cell.onpointerup = null;
          if (value === r.value) return;
          await keep(r.address, value, kept ? engineTier(kept.tier) : undefined);
        };
      };
    }
    tr.querySelector("button[data-let]")?.addEventListener("click", () => keep(r.address, r.value, null));
    tr.querySelector("button[data-share]")?.addEventListener("click", async () => {
      const w = await model.share([`param ${r.address}`]);
      if (w.error) msg(w.error, true);
    });
    if (kept?.yours) tr.children[0].title = `${r.address}: yours, a draft (in your layer over the model, kept for nobody else yet)`;
    table.appendChild(tr);
  }
  // (Everything kept of params, let go at once; everything of yours, kept for everyone.)
  const kept = p.params.filter((r) => r.kept != null).map((r) => r.address).concat(p.orphaned ?? []);
  const yours = p.params.filter((r) => r.kept?.yours).length + (p.drags ?? []).filter((d) => d.yours).length;
  if (kept.length || yours) {
    const tr = document.createElement("tr");
    tr.innerHTML = `<td colspan="6">${yours ? `<button class="mini" data-share title="Keep everything in your layer (params held, drags) for everyone: moved into the model's own generated file">Keep all ${yours} of yours for everyone</button> ` : ""}${kept.length ? `<button class="mini" data-let title="Let go of everything kept of its params in your layer: each is as the script says again">Let go of all ${kept.length} kept</button>` : ""}</td>`;
    tr.querySelector("button[data-let]")?.addEventListener("click", async () => {
      const w = await model.params(Object.fromEntries(kept.map((a) => [a, null])));
      if (w.error) msg(w.error, true);
    });
    tr.querySelector("button[data-share]")?.addEventListener("click", async () => {
      const w = await model.share(null);
      if (w.error) msg(w.error, true);
    });
    table.appendChild(tr);
  }
  // (Everything that gave way to firmer things, folded: each param's is beside it above; the rest (a drag, a reading
  // of geometry, a layout) only here.)
  const gave = (p.gave ?? []).map((g) => `<div>${esc(g.said)}: ${esc(g.tier)}, off by ${g.off.toFixed(g.off < 10 ? 3 : 1)}${g.within != null ? ` (± ${+g.within.toPrecision(4)})` : ""}</div>`);
  orphaned.innerHTML = (p.orphaned ?? []).map((a) => `<div>${esc(a)} is kept, and is not there. <button class="mini" data-release="${esc(a)}">Let it go</button></div>`).join("")
    + (gave.length ? `<details style="margin-top:4px"><summary class="muted" style="font-size:11px">Everything that gave way to firmer things (${gave.length})</summary>${gave.join("")}</details>` : "");
  for (const b of orphaned.querySelectorAll("button[data-release]")) b.onclick = () => model.params({ [b.dataset.release]: null });
  renderReadings(p);
}

/** What was read, kept: each listed with what it says, to be let go. (A param read is kept as it is held: at the level
 *  measured, to within so much, in the table above.) */
function renderReadings(p) {
  const box = $("paramsRead");
  const readings = Object.entries(p.readings ?? {});
  const said = (r) => (r.what.kind === "is" ? `${r.what.param} = ${r.what.value}` : `${r.what.kind} ${[r.what.a, r.what.b].filter(Boolean).join(", ")}${r.what.value != null ? ` = ${r.what.value}` : ""}`);
  box.innerHTML = readings.length ? `<div class="muted" style="font-size:11px">Read (measured), kept beside the script:</div>`
    + readings.map(([n, r]) => `<div>${esc(n)}: ${esc(said(r))} ± ${r.within} <button class="mini" data-unread="${esc(n)}" title="Let this reading go">×</button></div>`).join("") : "";
  for (const b of box.querySelectorAll("button[data-unread]")) b.onclick = () => model.read(b.dataset.unread, null);
}
