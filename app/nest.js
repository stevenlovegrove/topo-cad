// Nesting: the model's sheets of stock laid out on a sheet of stock (its width across, the parts as short along it
// as can be found), optimised while it is looked at; or on a machine's bed, photographed, with the stock on it found
// in the photo or drawn. With Optimise on (as it is unless turned off), whenever the pane is in sight a few searches
// run side by side (one a CPU core, each a worker of its own from its own seed: sparrow, through topo-nest), going
// on from the best layout found so far, until they have had the time they are given in all (a minute, unless said
// otherwise under "…"). What each is trying is drawn as outlines over the stock, and the best yet over that; it can
// be taken away as a DXF to cut from at any moment, or (on a MetalFab) handed to xTool Studio.
//
// What is nested: the parts of one material (a stock: material and thickness), or of any; and of those, the ones
// not left out. Below the stock is a tray of the parts left out: a part dragged from the stock to the tray is left
// out, and one dragged back onto the stock is nested again. A part dragged about on the stock, or turned there
// (Q/E), is locked where it is put (L to let it go again): the search goes round it. Which parts are left out and
// which locked are kept in this browser, by project and material. The best found goes on: less a part left out,
// with one put back past its end, or with one locked where it was put, for the search to fold in.
//
// The best, and how long has been spent finding it, are kept (in this browser, by project, where it is laid out and
// material) when the pane is left, closed or the page reloaded, and let go of when what it is of changes: the
// model's parts, the material, the width across, the gap or what a part may be turned by. The stock's length, the
// time given and the CPU cores used change nothing found.
//
// On a machine's bed (where the app is served by `topo serve`, which knows the machines): its photo, taken from
// here; the stock on it, found from a click on the sheet in the photo (F) with the holes cut in it, or drawn
// (rectangles, R; outlines, P, clicked corner by corner or dragged round), the stock drawn adding together and what
// is drawn with Alt (⌥) not to be cut taken from it; each such area dragged, or right-clicked to be made the other.
// No stock said: the whole bed is. Parts that do not fit are put past the bed and said; what does is handed to xTool
// Studio as a project of its own (a new file each time, what was set in the last kept). The bed and its areas are
// kept beside the model (`<model>.layout.json`); its photos by `topo serve` (layouts/).

import { esc, el, openMenu, store, stored } from "./ui.js";
import { model, project, storage } from "./project.js";
import { defineType, isShown, openOf } from "./shell/dock.js";
import { msg } from "./shell/panel.js";
import { download } from "./drawing.js";
import { addMetalFab, api, BEDS, dotFor, machinesKnown, onMachines, poll, stateOf, unwatch, watch } from "./machine-panel.js";
import { showMachines } from "./machines-tool.js";

const NS = "http://www.w3.org/2000/svg";
const SETTINGS = "topo-nest";
/** The material that is any: every part, whatever it is cut from. */
const ANY = "*";
const DEFAULTS = { width: 300, height: 600, gap: 2, seconds: 60, turns: "edges", searches: Math.max(1, Math.min(4, Math.floor((navigator.hardwareConcurrency || 4) / 2))), optimise: true, more: false, hdr: true };
const TURNS = { edges: "to lay an edge along or across", quarter: "by quarter turns", free: "by any angle", none: "not at all" };
/** How often each search says what it is trying, at most (ms). */
const TRIAL_MS = 120;

const fmt = (x) => (Math.round(x * 10) / 10).toFixed(1);
const slug = (s) => s.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
/** A short digest of a text (FNV-1a), to know whether what was nested is still what it was. */
function digest(text) {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) h = Math.imul(h ^ text.charCodeAt(i), 0x01000193);
  return (h >>> 0).toString(16);
}
/** A part's outline where a placement puts it (turned about its origin, then moved), y up. */
function placed(pts, p) {
  const t = (p.rotation * Math.PI) / 180, c = Math.cos(t), s = Math.sin(t);
  return pts.map(([x, y]) => [c * x - s * y + p.x, s * x + c * y + p.y]);
}
const bounds = (pts) => { const xs = pts.map((q) => q[0]), ys = pts.map((q) => q[1]); return [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)]; };
const area = (pts) => Math.abs(pts.reduce((a, q, i) => a + q[0] * pts[(i + 1) % pts.length][1] - pts[(i + 1) % pts.length][0] * q[1], 0)) / 2;
const rectOf = ([x0, y0], [x1, y1]) => [[Math.min(x0, x1), Math.min(y0, y1)], [Math.max(x0, x1), Math.min(y0, y1)], [Math.max(x0, x1), Math.max(y0, y1)], [Math.min(x0, x1), Math.max(y0, y1)]];
/** What is kept of a project's nesting of a material where it is laid out (`{ sig, best, spent }`); which parts it leaves out and locks (by material), in this browser. */
const keptKey = (target, stock) => `topo-nest-best:${project.current() ?? ""}:${target ? `${target}:` : ""}${stock}`;
const outKey = () => `topo-nest-out:${project.current() ?? ""}`;
const lockKey = () => `topo-nest-locked:${project.current() ?? ""}`;
const layoutFile = () => project.current()?.replace(/\.ts$/, ".layout.json");

/**
 * Parts in rows, as they are drawn (no turn), `along` long each: where each is in the picture (its box's top
 * left), and how deep the rows are in all.
 */
function rows(parts, along, gap) {
  const out = [];
  let x = 0, y = 0, deep = 0;
  for (const p of [...parts].sort((a, b) => (bounds(b.outline)[3] - bounds(b.outline)[1]) - (bounds(a.outline)[3] - bounds(a.outline)[1]))) {
    const [x0, y0, x1, y1] = bounds(p.outline), w = x1 - x0, h = y1 - y0;
    if (x > 0 && x + w > along) { x = 0; y += deep + gap; deep = 0; }
    out.push({ p, x, y, box: [x0, y0, x1, y1] });
    x += w + gap;
    deep = Math.max(deep, h);
  }
  return { at: out, deep: y + deep };
}

/** A nesting pane: `cut`, the model's sheets with no plan; `plan/<path>`, a layout of the build plans (docs/build-plans.md). */
function makeNest(id = "cut") {
  const KEY = `nest:${id}`;
  const pane = el("div", { class: "nestpane" }, `
    <div class="nestbar">
      <label class="targetl" hidden title="Where the parts are laid out: on a sheet of stock of a size, or on a machine's bed (photographed, with the stock on it found or drawn)">On <select class="target"></select></label>
      <label class="sized" title="The stock the parts are cut from: across, and along">Stock <input class="width" type="number" min="1" step="1"> × <input class="height" type="number" min="1" step="1"> mm</label>
      <label title="Which parts are nested: those cut from one material (at its thickness), or any">Material <select class="stock"></select></label>
      <label title="Between parts, and half that from the stock's edges">Gap <input class="gap" type="number" min="0" step="0.5"> mm</label>
      <label class="switch" title="Search for a shorter layout while this is in sight, until the time given is spent"><input class="optimise" type="checkbox"> Optimise</label>
      <button class="act dxf" title="The best layout yet, as one DXF to cut from (the stock on a layer of its own)" disabled>DXF</button>
      <button class="more" title="More settings">…</button>
    </div>
    <div class="nestmore" hidden>
      <label title="What a part may be turned by">Turned <select class="turns">${Object.entries(TURNS).map(([k, v]) => `<option value="${k}">${esc(v)}</option>`).join("")}</select></label>
      <label title="How long to search for in all, however many times the pane is looked at, until what is nested changes">At most <input class="seconds" type="number" min="1" step="1"> s</label>
      <label title="How many searches run side by side, one on each core: each goes its own way (from a seed of its own), and the best any finds is kept">CPU cores <input class="searches" type="number" min="1" max="8" step="1"></label>
    </div>
    <div class="nestbar nestmachine" hidden>
      <button class="act photo" title="Moves the head up and back out of the camera's way, sets the lights, and photographs the bed">Photograph bed</button>
      <label title="Each light alone at five exposures (55 to 3520), merged through the camera's measured response into one image of the light, so neither the lights' glare nor the dark corners are lost. The camera's own photos and each lighting's .hdr are kept as a capture set (~/.topo/captures)."><input type="checkbox" class="hdr"> HDR</label>
      <button class="act find" title="Then click on the sheet in the photo: it is outlined, with the holes cut in it, as stock (F). Draw stock (P) to add to it, or what is not to be cut (Alt+P) to take from it.">Find stock</button>
      <span class="sep"></span>
      <span class="sep"></span>
      <span class="mstat" title="How the machine is: its card and its work are in the Machines panel"></span>
      <button class="act showm" title="The machine's card and its work (and every other machine's), beside the checks">Machines</button>
      <span class="sep"></span>
      <button class="act xtool" title="Writes the layout as an xTool Studio project (cut; bend lines engraved lightly) and opens it there, to be looked at and cut from. A new file each time (xTool Studio, already open, would show the old one's tab again); what was set in the last one and saved there is kept, part by part.">Open in xTool Studio</button>
    </div>
    <div class="neststatus muted"></div>
    <div class="nestbody"><div class="nestview"><svg xmlns="${NS}"></svg></div></div>
    <div class="nestfoot"><span class="hint"></span>
      <span class="credit">Nested by <a href="https://github.com/JeroenGar/sparrow" target="_blank" rel="noopener">sparrow</a> (MIT), with <a href="https://github.com/JeroenGar/jagua-rs" target="_blank" rel="noopener">jagua-rs</a> (MPL-2.0)</span></div>`);
  const $$ = (c) => pane.querySelector(c);
  const svg = $$("svg");
  const N = {
    el: pane,
    parts: [],          // the model's, as last asked (each { id, name, stock, outline, holes, bends, area, angles, loops })
    error: null,
    // (Not `key`: the dock's own name for the pane, which it keeps on this object.)
    kept: null,         // where what is found is kept (by project, where it is laid out and material)
    sig: null,          // what the best must be of: the parts nested, and how they are nested
    best: null,         // the shortest layout found (topo_nest::Report, with `seed`; `tentative` when put by hand, not yet searched)
    spent: 0,           // seconds spent searching since what is nested last changed (not counting the search going on)
    ended: null,        // why searching is over before its time, if it is: "short" (it can be no shorter), or what went wrong
    run: null,          // the search going on: { workers, live, began, sig }
    ghosts: new Map(),  // each search's latest: seed → report
    viewBox: null,      // where it is looked at, once moved by hand (else: fitted)
    drag: null,         // a part being dragged: { name, from ("stock" | "tray"), x, y (where it was taken, in the picture), dx, dy, moved }
    zones: null,        // where the stock and the tray are in the picture: { W, trayTop, L }
    queued: false,
    sel: null,          // what is chosen: { kind: "part", name } | { kind: "region", index }
    tool: null,         // a tool on the bed: { kind: "find" } | { kind: "rect" | "poly", keepOut, pts }
    // On a machine's bed:
    machines: null,     // the machines `topo serve` knows; null where the app is not served
    target: "",         // where the parts are laid out: "" a sheet of stock; else a machine's id
    doc: { machine: "", bed: null, regions: [] },   // the bed's photo and areas, kept beside the model
    file: null,         // the file `doc` was read from
    asked: new Set(),   // photos of the bed asked for here (one done is put under the layout)
    taken: new Set(),
    // Of a plan: the layout it is (its path in the build plans), as the plans script says it (each part it makes,
    // its stock, what it needs, how it is nested, with what the groups it is in say), or why it cannot be had.
    plan: id.startsWith("plan/") ? id.slice("plan/".length) : null,
    leaf: null,
    leafError: null,
  };

  // ----- settings: kept in this browser -----
  const s0 = { ...DEFAULTS, ...stored(SETTINGS, {}) };
  for (const k of ["width", "height", "gap", "seconds", "searches"]) $$(`.${k}`).value = s0[k];
  $$(".turns").value = s0.turns;
  $$(".optimise").checked = s0.optimise;
  $$(".hdr").checked = s0.hdr;
  $$(".nestmore").hidden = !s0.more;
  const machineOf = () => (N.target ? N.machines?.find((m) => m.id === N.target) ?? null : null);
  /** The bed laid out on, mm (along x, across y), or null: on a sheet of stock. */
  const bed = () => { const m = machineOf(); return m ? BEDS[m.kind] ?? null : null; };
  const settings = () => {
    const b = bed(), L = N.leaf;
    const size = L?.stock?.size;
    return {
      scope: N.plan ? `plan:${N.plan}` : $$(".stock").value,
      width: b ? b[1] : size ? size[0] : +$$(".width").value, height: b ? b[0] : size ? size[1] : +$$(".height").value,
      gap: L?.settings?.gap ?? +$$(".gap").value, seconds: L?.settings?.seconds ?? +$$(".seconds").value,
      searches: Math.max(1, Math.min(8, Math.round(+$$(".searches").value || 1))), turns: L?.settings?.turns ?? $$(".turns").value,
      stock: $$(".stock").value, optimise: $$(".optimise").checked, more: !$$(".nestmore").hidden, hdr: $$(".hdr").checked,
    };
  };
  /** What a bed's y (towards the front, down the photo) is on the strip (y up from the bed's front edge), and back. */
  const flipY = (pts, W) => pts.map(([x, y]) => [x, W - y]);
  /** The parts it may nest (of a plan: those its layout makes), of a material (or any). */
  const ofStock = (stock) => { const base = N.leaf ? N.parts.filter((p) => N.leaf.made.includes(p.id)) : N.parts; return stock === ANY ? base : base.filter((p) => p.stock === stock); };
  /** The names of the parts of a material left out; locked. */
  const outOf = (stock) => new Set(stored(outKey(), {})[stock] ?? []);
  const lockedOf = (stock) => new Set(stored(lockKey(), {})[stock] ?? []);
  const setLocked = (stock, set) => { const all = stored(lockKey(), {}); all[stock] = [...set]; store(lockKey(), all); };
  const chosen = (s) => { const out = outOf(s.scope); return ofStock(s.stock).filter((p) => !out.has(p.name)); };
  const leftOut = (s) => { const out = outOf(s.scope); return ofStock(s.stock).filter((p) => out.has(p.name)); };
  const turnsOf = (s, p) => (s.turns === "quarter" ? { by: "step", step: 90 } : s.turns === "free" ? { by: "free" } : s.turns === "none" ? { by: "none" } : { by: "angles", angles: p.angles });
  const partNamed = (name) => N.parts.find((p) => p.name === name);
  const placementOf = (p) => N.best?.placements.find((pl) => pl.id === p.id) ?? null;
  /** The parts nested that are locked where they are (and are somewhere), and those the search may move. */
  function split(s) {
    const locked = lockedOf(s.scope), all = chosen(s);
    const pinned = all.filter((p) => locked.has(p.name) && placementOf(p));
    return { pinned, free: all.filter((p) => !pinned.includes(p)) };
  }
  /** On a bed: the stock (what is drawn or found of it; none said, the whole bed) and what is not to be cut, on the strip. */
  function bedStock(s) {
    const regions = N.doc.regions, W = s.width;
    const stock = regions.filter((r) => r.kind === "stock").map((r) => ({ outline: flipY(r.pts, W), holes: (r.holes ?? []).map((h) => flipY(h, W)) }));
    return {
      stock: stock.length ? stock : [{ outline: [[0, 0], [s.height, 0], [s.height, W], [0, W]] }],
      keepouts: regions.filter((r) => r.kind === "keepout").map((r) => flipY(r.pts, W)),
    };
  }
  /** The job a search is given, as things are now (less its seed and time). */
  function jobOf(s) {
    const { pinned, free } = split(s);
    const onBed = bed() ? bedStock(s) : { stock: [], keepouts: [] };
    return {
      parts: free.map((p) => ({ id: p.id, outline: p.outline, turns: turnsOf(s, p) })),
      width: s.width, gap: s.gap, trial_ms: TRIAL_MS, workers: 1,
      stock: onBed.stock,
      fixed: [...pinned.map((p) => placed(p.outline, placementOf(p))), ...onBed.keepouts],
    };
  }

  // ----- what is kept: the best found, and the time spent finding it, of what is nested now -----
  // (Under the project it is of: taken when what is nested is looked at, so a project opened since is not written over.
  // Nothing is kept of nothing: a model that does not build for a while does not lose what was found.)
  function save() {
    if (N.sig && N.kept) store(N.kept, { sig: N.sig, best: N.best, spent: N.spent + running(), ended: N.ended });
  }
  /**
   * What is nested has been looked at again (the model's parts, the settings, what is locked or left out, the bed's
   * areas): the best is kept if it is still of it. With `carry` (what changed is which parts, where, or what is
   * locked), what was found goes on as far as it can: its placements of the parts still nested, and those put back
   * past its end.
   */
  function rekey(carry) {
    const s = settings(), job = jobOf(s);
    const sig = job.parts.length + job.fixed.length && s.width > 0 && s.gap >= 0 ? digest(JSON.stringify([N.target, s.scope, job])) : null;
    const key = keptKey(N.target, s.scope);
    if (sig === N.sig && N.kept === key) return;
    const was = N.kept === key ? N.best : null;
    pause(true);
    N.sig = sig;
    N.kept = key;
    const kept = stored(key, null);
    const same = kept && kept.sig === sig && sig;
    N.best = same ? kept.best : null;
    N.spent = same ? kept.spent ?? 0 : 0;
    N.ended = same ? kept.ended ?? null : null;
    if (!same && carry && was && sig) N.best = carried(was, s);
    N.ghosts.clear();
    save();
  }
  /** A layout of these placements, as one found is said: how long it is, and how much of that is parts. */
  function layoutOf(placements, s) {
    const byId = new Map(N.parts.map((p) => [p.id, p]));
    let length = 0, parts = 0;
    for (const pl of placements) {
      const p = byId.get(pl.id);
      if (!p) continue;
      length = Math.max(length, bounds(placed(p.outline, pl))[2]);
      parts += area(p.outline);
    }
    return { kind: "best", length, strip: length + s.gap, density: length > 0 ? parts / (length * s.width) : 0, placements, off_stock: [], seed: 0, at: 0 };
  }
  /**
   * What was found, gone on with: the placements of the parts still nested, and each part nested and not in it put
   * past its end (turned the way, of those it may be, that takes least of the stock's length and fits across it),
   * where the search finds it room. Tentative: it may overlap, and the search's first is taken over it.
   */
  function carried(was, s) {
    const nested = chosen(s), ids = new Set(nested.map((p) => p.id));
    let placements = was.placements.filter((pl) => ids.has(pl.id));
    let end = placements.length ? layoutOf(placements, s).length : 0;
    for (const part of nested.filter((p) => !placements.some((pl) => pl.id === p.id))) {
      const angles = s.turns === "none" ? [0] : s.turns === "edges" ? part.angles : [0, 90, 180, 270];
      let best = null;
      for (const a of angles) {
        const [x0, y0, x1, y1] = bounds(placed(part.outline, { rotation: a, x: 0, y: 0 }));
        if (y1 - y0 <= s.width - s.gap && (!best || x1 - x0 < best.w)) best = { a, w: x1 - x0, x0, y0 };
      }
      if (!best) continue;
      placements = [...placements, { id: part.id, rotation: best.a, x: end + s.gap - best.x0, y: s.gap / 2 - best.y0 }];
      end += s.gap + best.w;
    }
    return placements.length ? { ...layoutOf(placements, s), tentative: true } : null;
  }
  /** Leaves parts out of what is nested, or puts them back (by name). */
  function setOut(names, out) {
    const s = settings(), set = outOf(s.scope);
    const parts = ofStock(s.stock).filter((p) => names.includes(p.name) && set.has(p.name) !== out);
    if (!parts.length) return;
    for (const p of parts) if (out) set.add(p.name); else set.delete(p.name);
    const all = stored(outKey(), {});
    all[s.scope] = [...set];
    store(outKey(), all);
    // (A part left out is no longer locked anywhere.)
    if (out) { const locked = lockedOf(s.scope); for (const p of parts) locked.delete(p.name); setLocked(s.scope, locked); }
    if (out && N.sel?.kind === "part" && names.includes(N.sel.name)) N.sel = null;
    rekey(true);
  }
  /** Locks a part where it is, or lets it go (`lock` undefined: the other of what it is). */
  function lock(name, on) {
    const s = settings(), set = lockedOf(s.scope), p = partNamed(name);
    if (!p || !placementOf(p)) return;
    if (on ?? !set.has(name)) set.add(name); else set.delete(name);
    setLocked(s.scope, set);
    rekey(true);
  }
  /** A part moved or turned by hand: where it is put, it stays (locked), and what was found goes on round it. */
  function moved(name, change) {
    const s = settings(), p = partNamed(name), pl = p && placementOf(p);
    if (!pl || lockedOf(s.scope).has(name) && !change.force) return;
    const placements = N.best.placements.map((q) => (q.id === p.id ? change.to(q) : q));
    N.best = { ...layoutOf(placements, s), tentative: true };
    const set = lockedOf(s.scope);
    set.add(name);
    setLocked(s.scope, set);
    rekey(true);
  }
  /** A part turned by a step (degrees, anticlockwise) about its middle. */
  function turn(name, step) {
    const p = partNamed(name);
    if (!p || !placementOf(p)) return;
    moved(name, {
      to: (pl) => {
        const [x0, y0, x1, y1] = bounds(placed(p.outline, pl));
        const next = { ...pl, rotation: (((pl.rotation + step) % 360) + 360) % 360 };
        const [a0, b0, a1, b1] = bounds(placed(p.outline, next));
        return { ...next, x: next.x + (x0 + x1 - a0 - a1) / 2, y: next.y + (y0 + y1 - b0 - b1) / 2 };
      },
    });
  }

  // ----- where it is laid out: a sheet of stock, or a machine's bed (kept beside the model) -----
  let saving = null;
  function keepDoc() {
    clearTimeout(saving);
    saving = setTimeout(async () => {
      // (Only into the file it was read from: not, after the project is changed, the new one's.)
      const f = N.file;
      if (!f || N.machines == null || f !== layoutFile()) return;
      try { await storage.store.write(f, JSON.stringify(N.doc, null, 2) + "\n"); } catch (e) { say(`not kept: ${e.message ?? e}`); }
    }, 400);
  }
  async function loadDoc() {
    clearTimeout(saving);
    const f = layoutFile();
    N.file = f;
    let d = null;
    try { d = f && N.machines != null ? JSON.parse((await storage.store.read(f)) ?? "null") : null; } catch { d = null; }
    N.doc = { machine: "", bed: null, regions: [], ...(d ?? {}) };
  }
  /** The machines asked again, and the select of where it is laid out made of them. */
  async function loadMachines() {
    N.machines = await machinesKnown();
    const sel = $$(".target");
    $$(".targetl").hidden = N.machines == null;
    if (N.machines == null) { N.target = ""; return; }
    sel.innerHTML = `<option value="">a sheet of stock</option>` + N.machines.map((m) => `<option value="${esc(m.id)}">${esc(m.name || m.id)} (${esc(m.host)})</option>`).join("") + `<option value="__add__">Add a MetalFab…</option>`;
  }
  function setTarget(t) {
    // (The machine laid out on is watched by this pane: asked how it is, for its head's place and its photos' end.)
    const was = N.target;
    N.target = N.machines?.some((m) => m.id === t) ? t : "";
    if (was && was !== N.target) unwatch(was, KEY);
    $$(".target").value = N.target;
    const on = !!bed();
    $$(".sized").hidden = on;
    $$(".nestmachine").hidden = !on;
    $$(".hint").textContent = on
      ? "F: find stock (click on the sheet) · R: stock rectangle · P: stock outline (click corners, Enter to close; or drag round) · with Alt (⌥): not to cut · drag a part to move it (it is locked there) · Q/E: turn · L: lock · Delete · right-click for more · scroll to zoom · double-click to fit"
      : "scroll to zoom · drag to pan · double-click to fit · drag a part to move it (it is locked there) · Q/E: turn · L: lock · Delete: leave out · outlines: being tried (red: still overlapping) · blue: the best yet";
    N.tool = null;
    N.viewBox = null;
    if (on) watch(N.target, KEY);
    machineSaid();
  }
  $$(".target").onchange = async (e) => {
    let t = e.target.value;
    if (t === "__add__") {
      try { t = (await addMetalFab(N.machines ?? [])) ?? N.target; } catch (err) { msg(err.message, true); t = N.target; }
      N.machines = null;
      await loadMachines();
    }
    setTarget(t);
    N.doc.machine = N.target;
    keepDoc();
    rekey(false);
    tick(); say(); draw();
  };
  /** A layout from before (the bed's own pane, its parts in bed mm, y down) taken as what was found, once. */
  function adopt(s) {
    const items = N.doc.items;
    if (!items || !bed()) return;
    delete N.doc.items;
    keepDoc();
    if (N.best) return;
    const placements = [], locked = lockedOf(s.scope);
    for (const [id, it] of Object.entries(items)) {
      const p = N.parts.find((q) => q.id === +id);
      if (!p || it.changed) continue;
      // (A turn the other way, and y from the other edge: the bed's y runs down its photo.)
      placements.push({ id: p.id, rotation: (360 - it.rotation) % 360, x: it.x, y: s.width - it.y });
      if (it.locked) locked.add(p.name);
    }
    setLocked(s.scope, locked);
    if (placements.length) N.best = { ...layoutOf(placements, s), tentative: true };
    rekey(true);
  }

  /**
   * The layout of the build plans this pane is (`N.plan`), as the plans script says it now: what it makes, its stock,
   * what it needs (a machine of a kind: the first of the workshop's of that kind is where it is laid out), and how it
   * is nested (with what the groups it is in say). What the plan says is not to be changed here.
   */
  async function loadLeaf() {
    const r = await model.plans();
    let found = null;
    const walk = (x, inherited) => {
      const settings = { ...inherited, ...(x.settings ?? {}) };
      if (x.kind === "group") x.children.forEach((c) => walk(c, settings));
      else if (x.kind === "alternatives") x.options.forEach((c) => walk(c, settings));
      else if (x.kind === "layout" && x.at === N.plan) found = { ...x, settings };
    };
    r.tree?.children.forEach((c) => walk(c, {}));
    N.leaf = found;
    N.leafError = found ? null : r.error ?? (r.none ? "there is no plans script beside the model" : `no layout ${N.plan} in the build plans`);
    $$(".stock").closest("label").hidden = true;
    $$(".stock").value = ANY;
    for (const [k, v] of [["gap", found?.settings?.gap], ["seconds", found?.settings?.seconds], ["turns", found?.settings?.turns]]) {
      $$(`.${k}`).disabled = v != null;
      if (v != null) $$(`.${k}`).value = v;
      $$(`.${k}`).title = v != null ? "as the plan says" : "";
    }
    const size = found?.stock?.size;
    for (const [k, v] of [["width", size?.[0]], ["height", size?.[1]]]) { $$(`.${k}`).disabled = v != null; if (v != null) $$(`.${k}`).value = v; }
    const kind = found?.needs?.machine?.kind;
    if (kind) {
      if (!machineOf() || machineOf().kind !== kind) setTarget(N.machines?.find((m) => m.kind === kind)?.id ?? "");
    } else if (found?.output === "dxf" && N.target) setTarget("");
  }
  /** The model's parts asked again (it has been built again, or the pane is new). */
  let loads = 0;
  N.load = async () => {
    // (A load begun before another is let go of when it comes.)
    const mine = ++loads, stale = () => mine !== loads;
    // (Read again where it is of another layout: what was shown was the last one's.)
    if (N.file !== layoutFile()) { pause(true); N.sel = null; await loadMachines(); await loadDoc(); setTarget(N.doc.machine); }
    const parts = model.built ? await model.ask("nest_parts") : { error: "the model does not build" };
    if (stale()) return;
    N.parts = Array.isArray(parts) ? parts : [];
    N.error = parts.error ?? null;
    if (N.plan) await loadLeaf();
    if (stale()) return;
    const stocks = [...new Set(N.parts.map((p) => p.stock))].sort((a, b) => ofStock(b).length - ofStock(a).length);
    const sel = $$(".stock"), was = sel.value || stored(SETTINGS, {}).stock;
    sel.innerHTML = [`<option value="${ANY}">Any (${N.parts.length})</option>`, ...stocks.map((s) => `<option value="${esc(s)}">${esc(s)} (${ofStock(s).length})</option>`)].join("");
    sel.value = was === ANY || stocks.includes(was) ? was : stocks[0] ?? ANY;
    rekey(false);
    adopt(settings());
    tick();
    say();
    draw();
  };
  pane.querySelectorAll(".nestbar input, .nestbar select:not(.target), .nestmore input, .nestmore select").forEach((e) => e.addEventListener("change", () => {
    store(SETTINGS, settings());
    rekey(false);
    // (More or fewer cores: the searches going on stop, and as many as are asked for go on from the best.)
    if (e.classList.contains("searches")) pause(true);
    tick();
    say();
    draw();
  }));
  $$(".more").onclick = () => { $$(".nestmore").hidden = !$$(".nestmore").hidden; store(SETTINGS, settings()); };

  // ----- searching: while it is in sight, Optimise is on, and there is time left -----
  const left = () => Math.max(0, settings().seconds - N.spent);
  const running = () => (N.run ? (performance.now() - N.run.began) / 1000 : 0);
  /** Whether to be searching now; and starting or stopping, so that it is. */
  function tick() {
    const s = settings();
    const want = s.optimise && N.sig && jobOf(s).parts.length && !N.ended && isShown(KEY) && document.visibilityState === "visible" && left() > 0.5;
    if (want && !N.run) begin(s);
    else if (!want && N.run) pause(true);
    else if (N.run && N.spent + running() >= s.seconds) pause(true);
    // (What has been spent is kept as it goes: a page closed or reloaded meanwhile loses none of it.)
    if (N.run) save();
    say();
  }
  /** Whether a layout found is better than the best: fewer parts off the stock, then shorter (anything is, over one put by hand). */
  const better = (a, b) => !b || b.tentative || a.off_stock.length < (b.off_stock?.length ?? 0) || (a.off_stock.length === (b.off_stock?.length ?? 0) && a.length < b.length - 1e-6);
  function begin(s) {
    const base = jobOf(s), free = new Set(base.parts.map((p) => p.id));
    const mine = N.best?.placements.filter((pl) => free.has(pl.id)) ?? [];
    const job = { ...base, seconds: left(), start: mine.length === free.size && free.size ? { strip: N.best.strip, placements: mine } : null };
    const R = { workers: [], live: 0, began: performance.now(), sig: N.sig };
    N.run = R;
    for (let i = 0; i < s.searches; i++) {
      // (Each time it goes on, from other seeds: so what is tried is not what was tried before.)
      const seed = 1 + i + s.searches * Math.round(N.spent), w = new Worker(new URL("./nest-worker.js", import.meta.url), { type: "module" });
      R.workers.push(w); R.live++;
      w.onmessage = (e) => {
        if (N.run !== R) return;
        const { report, done, error } = e.data;
        const r = report ?? done;
        if (r) {
          N.ghosts.set(seed, r);
          if (r.kind !== "trial") {
            // (What the search places, with what is locked where it is.)
            const pinned = N.best?.placements.filter((pl) => !free.has(pl.id)) ?? [];
            const found = { ...layoutOf([...r.placements, ...pinned], s), kind: r.kind, strip: r.strip, off_stock: r.off_stock ?? [], seed, at: N.spent + running() };
            if (better(found, N.best)) { N.best = found; save(); }
          }
          redraw();
        }
        if (done || error) {
          // (A search over before its time has got as short as can be: sparrow stops when the strip is no longer than
          // the parts' area, or the widest of them, needs. One that fails would fail again. Either way, it is not begun
          // again, however much time is left, till what is nested changes.)
          if (error) N.ended = `search ${seed}: ${error}`;
          else if (running() < job.seconds - 0.5) N.ended ??= "short";
          w.terminate();
          if (--R.live === 0) pause(true);
        }
      };
      w.postMessage({ job: { ...job, seed } });
    }
  }
  /** Stops the searches going on, keeping what they found and the time they took (`count`: that time is spent). */
  function pause(count) {
    const R = N.run;
    if (!R) return;
    for (const w of R.workers) w.terminate();
    if (count && R.sig === N.sig) N.spent += running();
    N.run = null;
    N.ghosts.clear();
    save();
    say();
    draw();
  }
  const timer = setInterval(tick, 400);
  document.addEventListener("visibilitychange", tick);

  /** What it says above the stock: how the search is going, and what is best. */
  function say(error, note) {
    const st = $$(".neststatus"), s = settings(), B = N.best;
    $$(".dxf").disabled = !B;
    if (error) { st.innerHTML = `<span style="color:var(--bad)">${esc(error)}</span>`; return; }
    if (N.error && !N.parts.length) { st.innerHTML = `<span style="color:var(--bad)">${esc(N.error)}</span>`; return; }
    if (N.plan && !N.leaf) { st.innerHTML = `<span style="color:var(--bad)">${esc(N.leafError ?? "reading the build plans…")}</span>`; return; }
    if (!N.parts.length) { st.textContent = "The model has no sheet of stock to nest."; return; }
    const bits = [];
    const needs = N.leaf?.needs?.machine?.kind;
    if (needs && !bed()) bits.push(`<span style="color:var(--bad)">the plan says a ${esc(needs)}, and none is set up: laid out on a sheet of stock meanwhile</span>`);
    if (note) bits.push(esc(note));
    if (N.tool) bits.push(N.tool.kind === "find" ? "find stock: click on the sheet in the photo (Esc: not now)" : `${N.tool.keepOut ? "not to cut" : "stock"}: ${N.tool.kind === "rect" ? "drag a rectangle" : "click its corners, Enter to close; or drag round it"} (Esc: not now)`);
    const t = N.spent + running(), nested = chosen(s), out = leftOut(s).length, locked = split(s).pinned.length;
    if (N.run) bits.push(`optimising: ${fmt(t)} of ${s.seconds} s`);
    else if (!nested.length) bits.push(`every part is left out: drag some from the tray onto the stock`);
    else if (!N.sig) bits.push(`the stock's width and the gap must be more than nothing`);
    else if (!jobOf(s).parts.length) bits.push(`every part is locked where it is: unlock some (L) for them to be nested`);
    else if (N.ended === "short") bits.push(`as short as it can be: no shorter than the parts' area (and the widest of them) needs`);
    else if (N.ended) bits.push(`<span style="color:var(--bad)">stopped: ${esc(N.ended)}</span>`);
    else if (t >= s.seconds - 0.5) bits.push(`optimised for ${fmt(t)} s`);
    else if (!s.optimise) bits.push(`${t > 0 ? `optimised for ${fmt(t)} s; ` : ""}Optimise is off`);
    else bits.push(`${fmt(t)} of ${s.seconds} s spent`);
    const stocks = [...new Set(nested.map((p) => p.stock))];
    if (nested.length) bits.push(`${nested.length} parts${s.stock === ANY ? (stocks.length > 1 ? ` of ${stocks.length} materials together` : "") : ` of ${esc(s.stock)}`}${out ? `, ${out} left out` : ""}${locked ? `, ${locked} locked` : ""}`);
    if (bed()) {
      if (N.doc.bed) bits.push(`bed photographed ${new Date(N.doc.bed.taken).toLocaleString()}${N.doc.bed.hdr ? " (HDR)" : ""}`);
      else bits.push(`the bed not photographed yet`);
      if (!N.doc.regions.some((r) => r.kind === "stock")) bits.push("no stock found or drawn: the whole bed is stock");
      const off = B?.off_stock?.length ?? 0;
      if (off) bits.push(`<span style="color:var(--bad)">${off} did not fit on the stock</span>`);
    } else if (B) {
      const need = B.length + s.gap / 2;
      const room = s.height - need;
      bits.push(room >= 0 ? `<b>best: ${fmt(need)} mm of the ${fmt(s.height)} mm</b> (${fmt(room)} mm to spare), ${fmt(B.density * 100)}% parts` : `<b style="color:var(--bad)">best: ${fmt(need)} mm, longer than the stock's ${fmt(s.height)} mm by ${fmt(-room)} mm</b>`);
    }
    if (N.error && N.parts.length) bits.push(`<span style="color:var(--bad)">${esc(N.error)}</span>`);
    st.innerHTML = bits.join(" · ");
  }

  async function exportDxf() {
    const B = N.best, s = settings();
    if (!B) return;
    const length = Math.max(s.height, B.length + s.gap / 2);
    const r = await model.ask("nest_dxf", { placements: B.placements, width: s.width, length });
    if (r.error) { msg(r.error, true); return; }
    const name = `nest-${N.plan ? slug(N.plan) : s.stock === ANY ? "any" : slug(s.stock)}.dxf`;
    download(name, r.text);
    msg(`${name}: ${B.placements.length} parts on ${fmt(s.width)} × ${fmt(length)} mm`);
  }
  $$(".dxf").onclick = exportDxf;

  // ----- the machine: how it is, a photo of its bed, the stock found in it, the layout handed to its software -----
  /** How the machine laid out on is, in the bar: its dot and what it is doing (its card and its work: the Machines panel). */
  function machineSaid() {
    const M = bed() ? stateOf(N.target) : null;
    $$(".mstat").innerHTML = !M ? "" : `<i class="mdot ${dotFor(N.target)}"></i> ${esc(M.connected ? M.status?.doing || M.status?.mode || "connected" : "not connected")}${(M.jobs ?? []).some((j) => !j.ended) ? " · at work" : ""}`;
  }
  $$(".showm").onclick = () => showMachines();
  const heard = onMachines(() => {
    if (!bed()) return;
    for (const j of stateOf(N.target)?.jobs ?? []) taken(j);
    machineSaid();
    redraw();
  });
  /** A photo of the bed asked for here, now done: put under the layout. */
  function taken(j) {
    if (j.state !== "done" || !j.result?.image || !N.asked.has(j.id) || N.taken.has(j.id)) return;
    N.taken.add(j.id);
    const b = j.result.bed;
    N.doc.bed = { image: j.result.image, mm: b.mm, px_per_mm: b.px_per_mm, taken: b.taken, serial: b.serial, hdr: j.result.hdr?.set };
    keepDoc();
    say(null, "the bed photographed");
  }
  $$(".photo").onclick = async () => {
    if (!confirm("The machine's head will move to the top and back of its travel, and its lights will be switched, to photograph the bed. Is it clear to move?")) return;
    try {
      const hdr = $$(".hdr").checked;
      store(SETTINGS, settings());
      const r = await api(`${encodeURIComponent(N.target)}/bed?model=${encodeURIComponent(project.current())}${hdr ? "&hdr=1" : ""}`, { method: "POST" });
      N.asked.add(r.job);
      poll(N.target);
    } catch (e) { say(`the photo: ${e.message}`); }
  };
  function findTool() {
    if (!N.doc.bed?.image) return say("photograph the bed first: the stock is found in its photo");
    N.tool = { kind: "find" };
    say();
    draw();
  }
  $$(".find").onclick = () => { findTool(); view.focus(); };
  /** The sheet under a point (bed mm), outlined with its holes by `topo serve` from the model's photo of the bed. */
  async function findAt(x, y) {
    N.tool = null;
    say(null, "finding the stock…");
    try {
      const f = await api(`stock?model=${encodeURIComponent(project.current())}&x=${x.toFixed(1)}&y=${y.toFixed(1)}`, { method: "POST" });
      N.doc.regions.push({ kind: "stock", pts: f.outline, holes: f.holes, from: "photo", at: [x, y], taken: f.taken });
      N.sel = { kind: "region", index: N.doc.regions.length - 1 };
      regionsChanged();
      say(null, `stock found: ${fmt(f.area / 1e4)} dm² with ${f.holes.length} hole${f.holes.length === 1 ? "" : "s"}${f.lit ? "" : " (from the photo alone: an HDR photo finds it better)"}; draw stock (P) to add to it, or Alt+P what is not to be cut`);
    } catch (e) { say(`the stock: ${e.message}`); }
    draw();
  }
  function regionsChanged() { keepDoc(); rekey(true); tick(); }
  /** The parts on the stock that fit, as shapes in bed mm (y down, as xTool Studio's canvas has it); null if none (said). */
  function handed() {
    const s = settings(), B = N.best, W = s.width;
    if (!B) { say("nothing nested yet"); return null; }
    const off = new Set(B.off_stock ?? []);
    const on = B.placements.filter((pl) => !off.has(pl.id));
    if (!on.length) { say("no part fits on the stock"); return null; }
    // (On the strip y is up; on the bed down: so each point's y from the other edge, and each arc's bulge the other way.)
    const shapes = on.map((pl) => {
      const p = N.parts.find((q) => q.id === pl.id);
      const loops = p.loops?.length ? p.loops : [p.outline, ...p.holes].map((c) => ({ corners: c, bulges: c.map(() => 0) }));
      return {
        name: p.name,
        loops: loops.map((l) => ({ corners: flipY(placed(l.corners, pl), W), bulges: l.bulges.map((b) => -b) })),
        marks: p.bends.map((b) => { const [[x1, y1], [x2, y2]] = flipY(placed([[b[0], b[1]], [b[2], b[3]]], pl), W); return [x1, y1, x2, y2]; }),
        at: { rotation: 0, x: 0, y: 0 },
      };
    });
    const name = `${(project.current() ?? "layout").split("/").pop().replace(/\.ts$/, "")}-${slug(N.plan ?? (s.stock === ANY ? "any" : s.stock))}`;
    return { shapes, name, n: on.length, note: off.size ? ` (${off.size} that did not fit on the stock left out)` : "" };
  }
  $$(".xtool").onclick = async () => {
    const h = handed();
    if (!h) return;
    try {
      const r = await api(`xtool/open?model=${encodeURIComponent(project.current())}&name=${encodeURIComponent(h.name)}`, { method: "POST", body: JSON.stringify({ shapes: h.shapes }) });
      const file = (f) => f.split("/").pop();
      const kept = r.settings_from ? ` Settings kept for ${r.settings_kept} part${r.settings_kept === 1 ? "" : "s"} from ${file(r.settings_from)}: its tab in xTool Studio can be closed.` : "";
      msg(r.opened ? `Opened in xTool Studio: ${h.n} part${h.n === 1 ? "" : "s"}${h.note}, as ${file(r.path)} (layouts/).${kept}` : `Written as ${r.path} (${r.why_not}): open it in xTool Studio${h.note}.${kept}`);
    } catch (e) { msg(`xTool Studio: ${e.message}`, true); }
  };

  // ----- drawing: the stock (or the bed), what is being tried, the best; the tray below -----
  function redraw() {
    if (N.queued) return;
    N.queued = true;
    requestAnimationFrame(() => { N.queued = false; draw(); say(); });
  }
  /** How the picture is laid out: the stock W across (drawn top to bottom) and L along; the tray below it. */
  function layout() {
    const s = settings(), W = s.width || 300;
    let L = s.height || 0;
    for (const r of [N.best, ...N.ghosts.values()]) if (r) L = Math.max(L, r.length + s.gap / 2);
    L = Math.max(L, 1);
    // (Between the stock and the tray, room for what is said of the tray and its two buttons: some 34 pixels.)
    const pad = Math.max(4, 0.03 * W), space = Math.max(0.08 * W, 34 * (N.k ?? 0.5));
    const tray = rows(leftOut(s), L - 2 * pad, Math.max(s.gap, pad));
    const trayTop = W + space, trayDeep = Math.max(tray.deep + 2 * pad, 0.2 * W);
    // (Nothing found yet: the parts to nest wait on the stock in rows, to be dragged as they are.)
    const waiting = N.best ? null : rows(chosen(s), L - 2 * pad, Math.max(s.gap, pad));
    return { s, W, L, pad, tray, trayTop, trayDeep, waiting };
  }
  const fit = () => {
    const { W, L, trayTop, trayDeep } = layout(), m = 0.05 * Math.max(W, L);
    return [-m, -m, L + 2 * m, trayTop + trayDeep + 2 * m];
  };
  const poly = (pts) => "M" + pts.map(([x, y]) => `${x.toFixed(2)},${y.toFixed(2)}`).join("L") + "Z";
  function draw() {
    const { s, W, L, pad, tray, trayTop, trayDeep, waiting } = layout(), vb = N.viewBox ?? fit(), onBed = !!bed();
    N.zones = { W, trayTop, L };
    svg.setAttribute("viewBox", vb.join(" "));
    const k = (N.k = Math.max(vb[2] / Math.max(1, svg.clientWidth || 600), vb[3] / Math.max(1, svg.clientHeight || 400)));   // one pixel, in mm
    const Y = (y) => W - y;   // (y up on the stock; down in the picture: on a bed, its own y)
    const path = (pts) => "M" + pts.map(([x, y]) => `${x.toFixed(2)},${Y(y).toFixed(2)}`).join("L") + "Z";
    const parts = new Map(N.parts.map((p) => [p.id, p]));
    const locked = lockedOf(s.scope), off = new Set(N.best?.off_stock ?? []);
    const label = (p, [x0, y0, x1, y1]) => `<text data-part="${esc(p.name)}" x="${(x0 + x1) / 2}" y="${(y0 + y1) / 2}" font-size="${Math.min(11 * k, Math.min(x1 - x0, y1 - y0) / 3)}" text-anchor="middle" dominant-baseline="middle" fill="#1b1b1b" font-family="sans-serif" pointer-events="none">${locked.has(p.name) ? "🔒 " : ""}${esc(p.name)}</text>`;
    /** A part in rows (on the stock waiting, or in the tray): its outline and holes, as drawn, at `top` down. */
    const inRow = (r, left, top, from, fill) => {
      const [x0, , , y1] = r.box, at = (pts) => "M" + pts.map(([x, y]) => `${(left + r.x + x - x0).toFixed(2)},${(top + r.y + y1 - y).toFixed(2)}`).join("L") + "Z";
      const w = r.box[2] - r.box[0], h = r.box[3] - r.box[1];
      return `<path class="part" data-part="${esc(r.p.name)}" data-from="${from}" d="${[r.p.outline, ...r.p.holes].map(at).join("")}" fill="${fill}" fill-rule="evenodd" stroke="#6b6b66" stroke-width="${1.2 * k}"><title>${esc(r.p.name)} (${esc(r.p.stock)})</title></path>` + label(r.p, [left + r.x, top + r.y, left + r.x + w, top + r.y + h]);
    };

    // The tray first, so that whatever is over it is drawn over it: what is left out, and the two buttons above it.
    let out = `<defs><pattern id="keepout" width="${8 * k}" height="${8 * k}" patternUnits="userSpaceOnUse" patternTransform="rotate(45)"><line x1="0" y1="0" x2="0" y2="${8 * k}" stroke="#c0392b" stroke-width="${2 * k}"/></pattern></defs>`;
    out += `<rect class="tray" x="0" y="${trayTop}" width="${L}" height="${trayDeep}" fill="#f7f7f3" stroke="#8a8a84" stroke-width="${k}" stroke-dasharray="${5 * k} ${4 * k}" rx="${3 * k}"/>`;
    const mid = W + (trayTop - W) / 2, fs = 11 * k;
    out += `<text x="0" y="${mid}" font-size="${fs}" fill="#6b6b66" font-family="sans-serif" dominant-baseline="middle" pointer-events="none">Not nested${tray.at.length ? ` (${tray.at.length})` : ""}: drag parts between the ${onBed ? "bed" : "stock"} and here</text>`;
    const button = (act, text, right, on) => {
      const w = text.length * 6.4 * k + 14 * k, x = right - w;
      return { w, svg: `<g class="nestbtn${on ? "" : " off"}" data-act="${act}"><rect x="${x}" y="${mid - 10 * k}" width="${w}" height="${20 * k}" rx="${4 * k}" fill="#fff" stroke="${on ? "var(--accent)" : "#c9c9c3"}" stroke-width="${k}"/><text x="${x + w / 2}" y="${mid}" font-size="${fs}" text-anchor="middle" dominant-baseline="middle" fill="${on ? "var(--accent)" : "#b0b0aa"}" font-family="sans-serif">${text}</text></g>` };
    };
    const a = button("out", "▼ Leave all out", L, chosen(s).length > 0), b = button("in", "▲ Nest all", L - a.w - 6 * k, tray.at.length > 0);
    out += a.svg + b.svg;
    if (onBed) {
      // The bed, its photo, and the stock on it (found, or drawn) and what is not to be cut: in its own mm.
      out += `<rect x="0" y="0" width="${s.height}" height="${W}" fill="#2b2b28" stroke="#8a8a84" stroke-width="${1.2 * k}"/>`;
      if (N.doc.bed?.image) out += `<image href="${esc(storage.store.url ? storage.store.url(N.doc.bed.image) : `files/${N.doc.bed.image}`)}?t=${N.doc.bed.taken}" x="0" y="0" width="${s.height}" height="${W}" preserveAspectRatio="none" opacity="0.9"/>`;
      N.doc.regions.forEach((r, i) => {
        const on = N.sel?.kind === "region" && N.sel.index === i;
        out += r.kind === "stock"
          ? `<path d="${poly(r.pts)}${(r.holes ?? []).map(poly).join("")}" fill="#2b9348" fill-opacity="0.18" fill-rule="evenodd" stroke="#2b9348" stroke-width="${(on ? 3 : 1.5) * k}" data-region="${i}"/>`
          : `<path d="${poly(r.pts)}" fill="url(#keepout)" fill-opacity="0.6" stroke="#c0392b" stroke-width="${(on ? 3 : 1.5) * k}" data-region="${i}"/>`;
      });
    } else out += `<rect x="0" y="0" width="${s.height}" height="${W}" fill="#ecece7" stroke="#8a8a84" stroke-width="${1.2 * k}"/>`;
    // (Past the stock's end, where a layout is longer than it.)
    if (L > s.height) out += `<rect x="${s.height}" y="0" width="${L - s.height}" height="${W}" fill="#fdecea" stroke="var(--bad)" stroke-width="${k}" stroke-dasharray="${4 * k} ${3 * k}"/>`;
    for (const [seed, r] of N.ghosts) {
      if (N.best && seed === N.best.seed && Math.abs(r.length - N.best.length) < 1e-6) continue;
      const colour = r.kind === "trial" ? "#c0392b" : "#6b6b66";
      for (const pl of r.placements) {
        const p = parts.get(pl.id);
        if (p) out += `<path d="${path(placed(p.outline, pl))}" fill="${colour}" fill-opacity="0.06" stroke="${colour}" stroke-opacity="0.6" stroke-width="${1.2 * k}" pointer-events="none"/>`;
      }
    }
    const B = N.best;
    if (B) {
      if (!onBed) {
        const end = B.length + s.gap / 2;
        out += `<line x1="${end}" y1="${-0.02 * W}" x2="${end}" y2="${1.02 * W}" stroke="var(--accent)" stroke-width="${1.5 * k}" stroke-dasharray="${6 * k} ${4 * k}"/>`;
        out += `<text x="${end}" y="${-0.03 * W}" font-size="${12 * k}" text-anchor="middle" fill="var(--accent)" font-family="sans-serif">${fmt(end)} mm</text>`;
      }
      for (const pl of B.placements) {
        const p = parts.get(pl.id);
        if (!p) continue;
        const d = [p.outline, ...p.holes].map((ring) => path(placed(ring, pl))).join("");
        const sel = N.sel?.kind === "part" && N.sel.name === p.name, lk = locked.has(p.name);
        const colour = off.has(p.id) ? "#c0392b" : lk ? "#1f4e8c" : "var(--accent)", fill = off.has(p.id) ? "#f6d5d1" : lk ? "#b9cdea" : "#cfe0f7";
        out += `<path class="part" data-part="${esc(p.name)}" data-from="stock" d="${d}" fill="${fill}" fill-opacity="${onBed ? 0.85 : 1}" fill-rule="evenodd" stroke="${sel ? "#e6a700" : colour}" stroke-width="${(sel ? 2.5 : 1.2) * k}" style="cursor:${lk ? "default" : "grab"}"><title>${esc(p.name)} (${esc(p.stock)})${lk ? ", locked (L to let it go)" : ""}${off.has(p.id) ? ": did not fit on the stock" : ""}: drag to the tray to leave it out</title></path>`;
        for (const b of p.bends) {
          const [[x1, y1], [x2, y2]] = placed([[b[0], b[1]], [b[2], b[3]]], pl);
          out += `<line data-part="${esc(p.name)}" x1="${x1}" y1="${Y(y1)}" x2="${x2}" y2="${Y(y2)}" stroke="#2b9348" stroke-width="${k}" stroke-dasharray="${4 * k} ${3 * k}" pointer-events="none"/>`;
        }
        const [x0, y0, x1, y1] = bounds(placed(p.outline, pl));
        out += label(p, [x0, Y(y1), x1, Y(y0)]);
      }
    } else if (waiting) for (const r of waiting.at) out += inRow(r, pad, pad, "stock", "#e3e9f2");
    for (const r of tray.at) out += inRow(r, pad, trayTop + pad, "tray", "#ffffff");
    // Where the machine says its head is (its own coordinates, which are its bed's).
    const M = onBed ? stateOf(N.target) : null, head = M?.connected && M.status?.head;
    if (head) {
      const [hx, hy] = head, r = 9 * k;
      out += `<g pointer-events="none" stroke="#ff4d6d" stroke-width="${1.5 * k}" fill="none"><circle cx="${hx}" cy="${hy}" r="${r}"/><line x1="${hx - 2 * r}" y1="${hy}" x2="${hx + 2 * r}" y2="${hy}"/><line x1="${hx}" y1="${hy - 2 * r}" x2="${hx}" y2="${hy + 2 * r}"/></g><text x="${hx + 2.2 * r}" y="${hy + 4 * k}" font-size="${11 * k}" fill="#ff4d6d" font-family="sans-serif" pointer-events="none">head</text>`;
    }
    // What is being drawn.
    if (N.tool?.pts?.length) {
      const pts = [...N.tool.pts, ...(N.tool.at ? [N.tool.at] : [])];
      const d = N.tool.kind === "rect" && pts.length > 1 ? poly(rectOf(pts[0], pts[pts.length - 1])) : "M" + pts.map(([x, y]) => `${x},${y}`).join("L");
      out += `<path d="${d}" fill="none" stroke="${N.tool.keepOut ? "#c0392b" : "#2b9348"}" stroke-width="${2 * k}" stroke-dasharray="${5 * k} ${3 * k}" pointer-events="none"/>`;
    }
    svg.innerHTML = out;
    svg.style.cursor = N.tool ? "crosshair" : "";
    lift();
  }
  /** The part being dragged, where it is dragged to, over everything. */
  function lift() {
    if (!N.drag?.moved) return;
    for (const e of svg.querySelectorAll(`[data-part="${CSS.escape(N.drag.name)}"]`)) {
      e.setAttribute("transform", `translate(${N.drag.dx} ${N.drag.dy})`);
      svg.appendChild(e);
    }
  }

  // ----- hands: looking about, parts dragged and turned, areas drawn on the bed, keys, right-click -----
  const view = $$(".nestview");
  view.tabIndex = 0;
  const point = (e) => { const r = svg.getBoundingClientRect(), vb = N.viewBox ?? fit(), s = Math.max(vb[2] / r.width, vb[3] / r.height); return [vb[0] + (e.clientX - r.left - (r.width - vb[2] / s) / 2) * s, vb[1] + (e.clientY - r.top - (r.height - vb[3] / s) / 2) * s, s]; };
  view.addEventListener("wheel", (e) => {
    e.preventDefault();
    const [x, y] = point(e), f = Math.exp(e.deltaY * 0.0015), vb = N.viewBox ?? fit();
    N.viewBox = [x - (x - vb[0]) * f, y - (y - vb[1]) * f, vb[2] * f, vb[3] * f];
    draw();
  }, { passive: false });
  let pan = null, area_ = null;
  view.addEventListener("mousedown", (e) => {
    view.focus();
    if (e.button !== 0 || e.target.closest("a")) return;
    const [x, y, s] = point(e);
    if (N.tool) {
      if (N.tool.kind === "find") return findAt(x, y);
      if (N.tool.kind === "rect") { N.tool.pts = [[x, y]]; area_ = { kind: "rect" }; }
      else { N.tool.pts.push([x, y]); area_ = { kind: "lasso", s, moved: false }; }
      return draw();
    }
    const act = e.target.closest?.(".nestbtn:not(.off)")?.dataset.act;
    if (act) {
      setOut(ofStock(settings().stock).map((p) => p.name), act === "out");
      tick(); say(); draw();
      return;
    }
    if (e.target.closest?.(".nestbtn")) return;
    const part = e.target.closest?.("[data-part].part"), region = e.target.closest?.("[data-region]");
    if (part) N.drag = { name: part.dataset.part, from: part.dataset.from, x, y, cx: e.clientX, cy: e.clientY, dx: 0, dy: 0, moved: false };
    else if (region) {
      const index = +region.dataset.region, r = N.doc.regions[index];
      N.sel = { kind: "region", index };
      area_ = { kind: "move", index, from: [x, y], pts: r.pts, holes: r.holes, moved: false, s };
      draw();
    } else { N.sel = null; pan = { x: e.clientX, y: e.clientY, vb: (N.viewBox ?? fit()).slice(), s }; draw(); }
  });
  window.addEventListener("mouseup", (e) => {
    pan = null;
    const A = area_;
    area_ = null;
    if (A?.kind === "move" && A.moved) regionsChanged();
    // (A polygon dragged round, not clicked corner by corner: closed when let go.)
    if (A?.kind === "lasso" && A.moved && N.tool?.kind === "poly") closeArea();
    if (A?.kind === "rect" && N.tool?.pts?.length && N.tool.at) {
      const [p, q] = [N.tool.pts[0], N.tool.at];
      if (Math.abs(p[0] - q[0]) > 1 && Math.abs(p[1] - q[1]) > 1) { N.doc.regions.push({ kind: N.tool.keepOut ? "keepout" : "stock", pts: rectOf(p, q), from: "rectangle" }); regionsChanged(); }
      N.tool = null;
      say(); draw();
    }
    const d = N.drag;
    if (!d) return;
    N.drag = null;
    if (d.moved) {
      const [, y] = point(e), Z = N.zones;
      const to = y >= Z.trayTop - 0.25 * (Z.trayTop - Z.W) ? "tray" : y >= 0 && y <= Z.W ? "stock" : null;
      if (to && to !== d.from) { setOut([d.name], to === "tray"); tick(); }
      // (Moved about on the stock: it stays where it is put.)
      else if (to === "stock" && d.from === "stock") { moved(d.name, { to: (pl) => ({ ...pl, x: pl.x + d.dx, y: pl.y - d.dy }) }); tick(); }
    } else N.sel = { kind: "part", name: d.name };
    say();
    draw();
  });
  view.addEventListener("mousemove", (e) => {
    const [x, y] = point(e);
    if (area_?.kind === "lasso" && N.tool?.kind === "poly" && e.buttons & 1) {
      const last = N.tool.pts[N.tool.pts.length - 1];
      if (Math.hypot(x - last[0], y - last[1]) > 4 * area_.s) { N.tool.pts.push([x, y]); area_.moved = true; }
    }
    if (N.tool) { N.tool.at = [x, y]; return redraw(); }
    if (area_?.kind === "move") {
      const r = N.doc.regions[area_.index], dx = x - area_.from[0], dy = y - area_.from[1];
      // (Not moved by a click that slips: the stock found in the photo is under most of the bed.)
      if (!r || (!area_.moved && Math.hypot(dx, dy) < 3 * area_.s)) return;
      const by = (pts) => pts.map(([px, py]) => [px + dx, py + dy]);
      r.pts = by(area_.pts);
      if (area_.holes) r.holes = area_.holes.map(by);
      area_.moved = true;
      return redraw();
    }
    if (N.drag) {
      if (lockedOf(settings().stock).has(N.drag.name) && N.drag.from === "stock") return;
      N.drag.dx = x - N.drag.x; N.drag.dy = y - N.drag.y;
      if (Math.abs(e.clientX - N.drag.cx) + Math.abs(e.clientY - N.drag.cy) > 4) N.drag.moved = true;
      lift();
      return;
    }
    if (!pan) return;
    N.viewBox = [pan.vb[0] - (e.clientX - pan.x) * pan.s, pan.vb[1] - (e.clientY - pan.y) * pan.s, pan.vb[2], pan.vb[3]];
    draw();
  });
  view.addEventListener("dblclick", () => { if (!N.tool) { N.viewBox = null; draw(); } });
  function closeArea() {
    if (N.tool.pts.length >= 3) { N.doc.regions.push({ kind: N.tool.keepOut ? "keepout" : "stock", pts: N.tool.pts, from: "outline" }); regionsChanged(); }
    N.tool = null;
    say(); draw();
  }
  function remove() {
    if (N.sel?.kind === "region") { N.doc.regions.splice(N.sel.index, 1); N.sel = null; regionsChanged(); }
    else if (N.sel?.kind === "part") setOut([N.sel.name], true);
  }
  view.addEventListener("keydown", (e) => {
    // (Letters by the key, not what it types: on a Mac, ⌥P types π.)
    const k = /^Key[A-Z]$/.test(e.code) ? e.code.slice(3).toLowerCase() : e.key.toLowerCase();
    const onBed = !!bed();
    if (k === "f" && onBed) findTool();
    else if ((k === "r" || k === "p") && onBed) { N.tool = { kind: k === "r" ? "rect" : "poly", keepOut: e.altKey, pts: [] }; say(); }
    else if (k === "escape") { N.tool = null; say(); }
    else if (k === "enter" && N.tool?.kind === "poly") closeArea();
    else if ((k === "delete" || k === "backspace") && N.sel) { N.drag = null; area_ = null; remove(); tick(); }
    else if ((k === "q" || k === "e") && N.sel?.kind === "part") { turn(N.sel.name, (e.shiftKey ? 5 : 90) * (k === "q" ? 1 : -1)); tick(); }
    else if (k === "l" && N.sel?.kind === "part") { lock(N.sel.name); tick(); }
    else return;
    e.preventDefault();
    say();
    draw();
  });
  view.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    if (N.tool) return;
    const part = e.target.closest?.("[data-part].part"), region = e.target.closest?.("[data-region]");
    const after = () => { tick(); say(); draw(); };
    if (part) {
      const name = part.dataset.part, s = settings(), lk = lockedOf(s.scope).has(name), on = part.dataset.from === "stock" && N.best;
      N.sel = { kind: "part", name };
      draw();
      openMenu([
        { label: name, cls: "head" },
        ...(on ? [{ label: lk ? "Unlock (L)" : "Lock where it is (L)", run: () => { lock(name); after(); } },
          { label: "Turn left a quarter (Q)", run: () => { turn(name, 90); after(); } },
          { label: "Turn right a quarter (E)", run: () => { turn(name, -90); after(); } }, null] : []),
        part.dataset.from === "tray" ? { label: "Nest it", run: () => { setOut([name], false); after(); } } : { label: "Leave it out (Delete)", run: () => { setOut([name], true); after(); } },
      ], e.clientX, e.clientY);
    } else if (region) {
      const index = +region.dataset.region, r = N.doc.regions[index], stock = r.kind === "stock";
      N.sel = { kind: "region", index };
      draw();
      openMenu([
        { label: `${stock ? "Stock" : "Not to cut"}${r.from === "photo" ? ", found in the photo" : ""}`, cls: "head" },
        stock ? { label: "Make it not to cut (taken from the stock)", run: () => { r.kind = "keepout"; regionsChanged(); after(); } } : { label: "Make it stock (nested on)", run: () => { r.kind = "stock"; regionsChanged(); after(); } },
        null,
        { label: "Delete (Delete)", run: () => { N.doc.regions.splice(index, 1); N.sel = null; regionsChanged(); after(); } },
      ], e.clientX, e.clientY);
    }
  });

  N.load();
  const dispose = () => { pause(true); clearInterval(timer); heard(); if (N.target) unwatch(N.target, KEY); document.removeEventListener("visibilitychange", tick); };
  return Object.assign(N, { shown: () => { tick(); draw(); }, dispose, keep: () => ({}) });
}

defineType("nest", {
  title: (id) => (id?.startsWith("plan/") ? `Build plan: ${id.slice(5)}` : "Nest flat patterns on stock"),
  label: (id) => (id?.startsWith("plan/") ? `<b>${esc(id.slice(5).split("/").pop())}</b> (plan)` : "<b>Nest</b> flat patterns"),
  make: makeNest,
});
// (The model built again: each nesting open asks for its parts again.)
model.onChange(() => { for (const N of openOf("nest")) N.load(); });
// (The plans script saved: each layout of it open reads what it now says.)
project.onFiles(() => { for (const N of openOf("nest")) if (N.plan) N.load(); });
