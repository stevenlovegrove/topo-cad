// The objects of what is open, as a tree in the browser ("Objects"): the model's parts in
// their groups, and whatever else is there to be shown (a scan and what is made on it, when a
// scan workspace is served: scan.js adds those). Every row has an eye (whether it is drawn),
// which is apart from selecting it (clicking it).
//
// This module owns the tree, what is hidden and what is folded; it knows nothing of a scan.
// Others give it what to list:
//
//   model({ parts, decorate, header, selected })   the model's parts (the one list there is)
//   section({ key, order, rows })                  another group of rows, by key
//   on("select" | "goto" | "visibility", fn)       what a click, a double click, an eye did
//   menu(fn)                                       the right-click menu of a row
//
// and call `render()` when what they list has changed.

const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
const stored = (k, d) => { try { return JSON.parse(localStorage.getItem(k) ?? "null") ?? d; } catch (_) { return d; } };

// What is hidden and what is folded open, by key; remembered. (The keys are as they were when
// only the scan's views had a tree: so what was hidden there still is.)
export const hidden = new Set(stored("topo.scan.hidden", []));
export const open = new Set(stored("topo.scan.open", ["scan", "model", "issues", "selections", "planes"]));
export const keep = () => { try { localStorage.setItem("topo.scan.hidden", JSON.stringify([...hidden])); localStorage.setItem("topo.scan.open", JSON.stringify([...open])); } catch (_) {} };
// (Groups that came later start open too.)
if (stored("topo.scan.groups", 1) < 2) { open.add("slabs"); open.add("polylines"); keep(); try { localStorage.setItem("topo.scan.groups", "2"); } catch (_) {} }
/** Whether every one of these keys is shown. */
export const vis = (...keys) => keys.every((k) => !hidden.has(k));

/** A part's place in the tree: its groups (below the building) and its own name. */
export function memberParts(path) {
  const parts = path.split("/");
  return { groups: parts.slice(1, -1), name: parts[parts.length - 1] };
}
/** Whether a part of the model is drawn: the model, its groups and itself are all shown. */
export function memberVisible(path) {
  if (hidden.has("model") || hidden.has(`m:${path}`)) return false;
  const g = memberParts(path).groups;
  for (let k = 1; k <= g.length; k++) if (hidden.has(`g:${g.slice(0, k).join("/")}`)) return false;
  return true;
}

const listeners = { select: new Set(), goto: new Set(), visibility: new Set() };
/** Listens: "select" (kind, id), "goto" (kind, id), "visibility" (). */
export function on(what, fn) { listeners[what].add(fn); }
const tell = (what, ...args) => { for (const fn of listeners[what]) fn(...args); };

/** Says a row was chosen (from a view: a part picked there), as a click on it would. */
export function select(kind, id) { tell("select", kind, id); render(); }
/** What is hidden has been changed (by more than one eye: every member shown again): it is kept, and said. */
export function changed() { keep(); tell("visibility"); render(); }
/** Shows or hides what a key names. */
export function toggle(key) {
  hidden.has(key) ? hidden.delete(key) : hidden.add(key);
  changed();
}

let menuOf = null;
/** The right-click menu of a row: `fn(kind, id or key, event)`. */
export function menu(fn) { menuOf = fn; }

// ----- what is listed -------------------------------------------------------------------

const EYE = `<svg viewBox="0 0 16 16" width="14" height="14"><path d="M1.5 8s2.4-4.2 6.5-4.2S14.5 8 14.5 8s-2.4 4.2-6.5 4.2S1.5 8 1.5 8z" fill="none" stroke="currentColor" stroke-width="1.2"/><circle cx="8" cy="8" r="2" fill="currentColor"/></svg>`;
const EYE_OFF = `<svg viewBox="0 0 16 16" width="14" height="14"><path d="M1.5 8s2.4-4.2 6.5-4.2S14.5 8 14.5 8s-2.4 4.2-6.5 4.2S1.5 8 1.5 8z" fill="none" stroke="currentColor" stroke-width="1.2" opacity=".45"/><path d="M3 13L13 3" stroke="currentColor" stroke-width="1.3"/></svg>`;

/** A row: `kind` and `id` say what it is (to a click), `key` what its eye and its fold are kept under. */
export function row(depth, kind, key, name, opts = {}) {
  const tw = opts.group ? `<span class="tw" data-tw>${open.has(key) ? "▾" : "▸"}</span>` : `<span class="tw"></span>`;
  return `<div class="trow${opts.on ? " on" : ""}${vis(key) ? "" : " off"}" data-kind="${kind}" data-key="${esc(key)}" ${opts.id != null ? `data-id="${esc(opts.id)}"` : ""} style="padding-left:${4 + depth * 12}px" title="${esc(opts.title || name)}">${tw}<span class="eye" data-eye title="Show or hide">${vis(key) ? EYE : EYE_OFF}</span>${opts.dot ? `<i class="dot ${opts.dot}"></i>` : ""}<span class="tn">${esc(name)}</span>${opts.note != null ? `<span class="muted">${esc(opts.note)}</span>` : ""}</div>`;
}

// The model: who lists its parts (one at a time: whoever knows most of them: the page from the
// engine's mesh; a scan workspace, which has what the scan shows of each).
let theModel = null;
/**
 * The model's parts. `parts()`: each with a `path`; `decorate(part)`: its `dot` (a class) and
 * `title`; `header()`: the `note` and `title` of the model's own row; `selected()`: the path of
 * the part chosen; `count(parts)`: what is said beside a group (none: how many).
 */
export function model(def) { theModel = def; render(); }

function modelRows() {
  const def = theModel;
  if (!def) return "";
  const list = def.parts() || [];
  const head = def.header?.(list) ?? {};
  let html = row(0, "top", "model", "Model", { group: true, note: head.note ?? list.length, title: head.title ?? "Model" });
  if (!open.has("model")) return html;
  const node = { groups: new Map(), members: [] };
  for (const m of list) {
    let n = node;
    for (const g of memberParts(m.path).groups) { if (!n.groups.has(g)) n.groups.set(g, { groups: new Map(), members: [] }); n = n.groups.get(g); }
    n.members.push(m);
  }
  const all = (n) => [...n.members, ...[...n.groups.values()].flatMap(all)];
  const chosen = def.selected?.();
  const walk = (n, depth, prefix) => {
    for (const [name, g] of n.groups) {
      const path = prefix ? `${prefix}/${name}` : name, key = `g:${path}`, inside = all(g);
      const said = def.group?.(path, inside) ?? {};
      html += row(depth, "group", key, name, { group: true, id: path, note: inside.length, dot: said.dot ?? "", title: said.title ?? `${path}: ${inside.length}` });
      if (open.has(key)) walk(g, depth + 1, path);
    }
    for (const m of n.members) {
      const said = def.decorate?.(m) ?? {};
      html += row(depth, "member", `m:${m.path}`, memberParts(m.path).name, { id: m.path, dot: said.dot, on: m.path === chosen, title: said.title ?? m.path });
    }
  };
  walk(node, 1, "");
  return html;
}

const sections = [{ key: "model", order: 10, rows: modelRows }];
/** A group of rows of its own (`rows()`: their HTML, made with `row`), placed by `order` (the model is 10). */
export function section(def) {
  const i = sections.findIndex((s) => s.key === def.key);
  if (i >= 0) sections[i] = def; else sections.push(def);
  sections.sort((a, b) => a.order - b.order);
  render();
}

// ----- the tree ---------------------------------------------------------------------------

let box = null;
export function render() {
  if (!box) return;
  const top = box.scrollTop;
  box.innerHTML = `<div class="tree">${sections.map((s) => s.rows()).join("")}</div>`;
  box.scrollTop = top;
}

/** What a row of the tree is: { kind, key, id }. */
const rowOf = (e) => { const r = e.target.closest(".trow"); return r && { el: r, kind: r.dataset.kind, key: r.dataset.key, id: r.dataset.id }; };

/**
 * Puts the tree in the browser, under a tab of its own ("Objects"). `page`: { tabs, panes (the
 * browser's two elements), showBrowser }.
 */
export function mount(page) {
  if (box) return;
  const b = document.createElement("button");
  b.dataset.b = "layers";
  b.textContent = "Objects";
  b.onclick = () => page.showBrowser("layers");
  page.tabs.appendChild(b);
  box = document.createElement("div");
  box.className = "bpane";
  box.dataset.b = "layers";
  box.id = "layersPane";
  box.hidden = true;
  page.panes.appendChild(box);
  // (A row is drawn again when it is selected, so a double click is told by its timing, not by the browser.)
  let last = { key: null, t: 0 };
  box.addEventListener("click", (e) => {
    const r = rowOf(e);
    if (!r) return;
    if (e.target.closest("[data-eye]")) return toggle(r.key);
    const kind = r.kind === "sel" ? "selection" : r.kind, now = performance.now();
    const twice = last.key === r.key && now - last.t < 400;
    last = { key: r.key, t: twice ? 0 : now };
    // Twice: go to it.
    if (twice && kind !== "top") return tell("goto", kind, r.id);
    if (kind === "top" || kind === "group") {
      // The arrow (or the row, of a group that has no other use) folds it.
      if (r.el.querySelector("[data-tw]")) { open.has(r.key) ? open.delete(r.key) : open.add(r.key); keep(); render(); }
      return;
    }
    select(kind, r.id);
  });
  box.addEventListener("contextmenu", (e) => {
    const r = rowOf(e);
    if (!r || !menuOf) return;
    e.preventDefault();
    menuOf(r.kind === "sel" ? "selection" : r.kind, r.kind === "top" ? r.key : r.id, e);
  });
  render();
}
