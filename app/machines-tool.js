// The Machines panel: the machines of the workshop (those `topo serve` knows), each with how it is and its queue of
// work, beside the checks. A machine's row opened is a machine watched (asked how it is, and so connected to); one
// closed is not, unless something else watches it or it has work not finished. Machines are added here.

import { el, esc, store, stored } from "./ui.js";
import { addTool, bring, retitle } from "./shell/dock.js";
import { msg } from "./shell/panel.js";
import { addMetalFab, dotFor, known, machineHtml, machinesKnown, onMachines, reloadMachines, stateOf, unwatch, watch, wireCancels } from "./machine-panel.js";

const OPEN = "topo-machines-open";
const box = el("div", { class: "machinestool" });
let opened = new Set(stored(OPEN, []));

/** What the panel's tab says: how many machines, and whether any is working or has something wrong. */
function label() {
  const list = known() ?? [];
  const busy = list.some((m) => (stateOf(m.id)?.jobs ?? []).some((j) => !j.ended));
  const bad = list.some((m) => dotFor(m.id) === "bad" && opened.has(m.id));
  return `Machines${list.length ? ` <span class="muted">${list.length}</span>` : ""}${busy ? ` <i class="mdot busy"></i>` : bad ? ` <i class="mdot bad"></i>` : ""}`;
}

function render() {
  const list = known();
  if (list == null) {
    box.innerHTML = `<div class="muted mpad">Machines are known to <code>topo serve</code>, which is not serving this page.</div>`;
    return;
  }
  let out = `<div class="mtoolbar"><b>The workshop</b><button class="act add">Add a MetalFab…</button></div>`;
  if (!list.length) out += `<div class="muted mpad">No machine yet. Add one: it is looked for on the network.</div>`;
  for (const m of list) {
    const on = opened.has(m.id), M = stateOf(m.id), live = (M?.jobs ?? []).filter((j) => !j.ended).length;
    out += `<div class="mmachine${on ? " open" : ""}" data-id="${esc(m.id)}">
      <div class="mhead" title="${on ? "Close: no longer asked how it is" : "Open: asked how it is (connected to)"}"><span class="tw">${on ? "▾" : "▸"}</span><span class="mdot ${dotFor(m.id)}"></span><b>${esc(m.name || m.id)}</b><span class="muted">${esc(m.kind)} · ${esc(m.host)}${live ? ` · ${live} at work` : ""}</span></div>
      ${on ? `<div class="mbody">${machineHtml(m.id)}</div>` : ""}
    </div>`;
  }
  box.innerHTML = out;
  box.querySelector(".add").onclick = async () => { try { await addMetalFab(known() ?? []); } catch (e) { msg(e.message, true); } };
  box.querySelectorAll(".mmachine .mhead").forEach((h) => (h.onclick = () => {
    const id = h.parentElement.dataset.id;
    if (opened.has(id)) { opened.delete(id); unwatch(id, "panel"); } else { opened.add(id); watch(id, "panel"); }
    store(OPEN, [...opened]);
    render();
  }));
  wireCancels(box);
}

addTool("machines", { title: "Machines", label, el: box, side: "right", beside: "checks" });
onMachines(() => { render(); retitle(); });
machinesKnown().then((list) => { for (const id of opened) if (list?.some((m) => m.id === id)) watch(id, "panel"); render(); });
/** Brings the Machines panel into sight. */
export const showMachines = () => bring("tool:machines");
export { reloadMachines };
