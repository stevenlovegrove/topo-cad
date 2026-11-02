// What the open project has, as one tree: under the project, its drawings, its files, its views,
// the model's parts in their groups, and whatever else is in its scene (slabs, polylines, issues,
// a scan and what is made on it): each a group that whoever has it adds. A row of what is drawn
// in a view has an eye (whether it is drawn), which is apart from choosing it (clicking it).
//
// This module owns the tree, what is hidden and what is folded; it knows nothing of what the
// rows are of.
// Others give it what to list:
//
//   model({ parts, decorate, header, selected })   the model's parts (the one list there is)
//   section({ key, order, rows })                  another group of rows, by key
//   on("select" | "goto" | "open" | "drag" | "visibility", fn)
//                                                  what a click, a double click, a shift-click, a drag, an eye did
//   menu(fn)                                       the right-click menu of a row
//
// and call `render()` when what they list has changed.

import { el, esc, store, stored } from "./ui.js";
import { iconOf } from "./icons.js";
import { did } from "./actions.js";

/** What is hidden being someone else's for now (following them): the viewer's own, kept for after; and who is told if the viewer changes anything. */
let lent = null;
// What is hidden and what is folded open, by key; remembered. (Where it was kept when only a scan's
// views had a tree, it is taken from there: what was hidden still is.)
export const hidden = new Set(stored("topo.objects.hidden", stored("topo.scan.hidden", [])));
export const open = new Set(stored("topo.objects.open", stored("topo.scan.open", ["scan", "model", "issues", "selections", "planes", "slabs", "polylines"])));
export const keep = () => { store("topo.objects.hidden", [...(lent?.own ?? hidden)]); store("topo.objects.open", [...open]); };
// (Groups that came later start open too.)
if (stored("topo.objects.rooted", 0) < 2) { for (const k of ["project", "drawings", "views", "scratch"]) open.add(k); keep(); store("topo.objects.rooted", 2); }
/** What is hidden made these keys for now, not kept as the viewer's own. `over()`: the viewer has gone their own way. */
export function borrow(keys, over) {
  lent = { own: lent?.own ?? [...hidden], over };
  hidden.clear();
  for (const k of keys) hidden.add(k);
}
/** What is hidden is the viewer's own again. */
export function giveBack() {
  if (!lent) return;
  const { own } = lent;
  lent = null;
  hidden.clear();
  for (const k of own) hidden.add(k);
}
/** The viewer changing what is hidden: from their own, and whoever lent it is told. */
function mine() {
  if (!lent) return;
  const { over } = lent;
  giveBack();
  over?.();
}
/** Whether every one of these keys is shown. */
export const vis = (...keys) => keys.every((k) => !hidden.has(k));

/** A part's place in the tree: its groups (below the building) and its own name. */
function memberParts(path) {
  const parts = path.split("/");
  return { groups: parts.slice(1, -1), name: parts[parts.length - 1] };
}
/** Whether a part of the model is drawn: the model, its groups and itself are all shown. */
export function memberVisible(path) {
  if (hidden.has("model") || hidden.has(`m:${path}`)) return false;
  // (A part that is hidden hides what is under it: its shapes, its parts, what holds it.)
  for (let up = path.lastIndexOf("/"); up > 0; up = path.lastIndexOf("/", up - 1)) if (hidden.has(`m:${path.slice(0, up)}`)) return false;
  const g = memberParts(path).groups;
  for (let k = 1; k <= g.length; k++) if (hidden.has(`g:${g.slice(0, k).join("/")}`)) return false;
  return true;
}

const listeners = { select: new Set(), goto: new Set(), open: new Set(), drag: new Set(), drop: new Set(), visibility: new Set() };
/** Listens: "select" (kind, id), "goto" (kind, id), "open" (kind, id: a shift-click; of a group's own row, kind "top" and its key), "drag" (kind, id, event: a row's drag has begun), "drop" (the row dragged, the row it was let go on: { kind, id, key }), "visibility" (). */
export function on(what, fn) { listeners[what].add(fn); }
const tell = (what, ...args) => { for (const fn of listeners[what]) fn(...args); };

/** Says a row was chosen (from a view: a part picked there), as a click on it would. */
export function select(kind, id) { tell("select", kind, id); render(); }
/** What is hidden has been changed (by more than one eye: every member shown again): it is kept, and said. */
export function changed() { keep(); tell("visibility"); render(); }
/** Makes what these keys name shown (a row, and the group it is in), without saying so: whoever asks draws again. */
export function show(...keys) {
  for (const k of keys) hidden.delete(k);
  keep();
}
/** Shows or hides what a key names. */
export function toggle(key) {
  mine();
  did("toggle", key);
  hidden.has(key) ? hidden.delete(key) : hidden.add(key);
  changed();
}

const ofModel = (k) => k.startsWith("m:") || k.startsWith("g:");
/** Whether any part of the model, or group of them, is hidden. */
export function isolated() { return [...hidden].some(ofModel); }
/** Every part of the model is shown again. */
export function showAll() {
  mine();
  did("showAll");
  for (const k of [...hidden]) if (ofModel(k) || k === "model") hidden.delete(k);
  changed();
}
/**
 * Only this: what a key names is shown (a part, `m:` and its path; or a group, `g:` and its
 * path below the building), and every other part of the model is hidden. Asked again of what
 * is already the only thing shown: all are shown.
 */
export function isolate(key) {
  mine();
  did("only", key);
  const list = theModel?.parts() || [];
  const group = key.startsWith("g:"), target = key.slice(2);
  // The groups it is in (a group: and itself): what is outside them, at each, is hidden as one thing.
  const chain = group ? target.split("/") : memberParts(target).groups;
  const next = new Set();
  for (const m of list) {
    // (A part, and what is under it.)
    if (!group && (m.path === target || m.path.startsWith(`${target}/`))) continue;
    const g = memberParts(m.path).groups;
    let k = 0;
    while (k < chain.length && g[k] === chain[k]) k++;
    // (In the group; or the part that is the group, which is where the group is.)
    if (group && (k === chain.length || m.path.split("/").slice(1).join("/") === target)) continue;
    next.add(g.length > k ? `g:${g.slice(0, k + 1).join("/")}` : `m:${m.path}`);
  }
  const was = [...hidden].filter(ofModel).sort().join("\n"), now = [...next].sort().join("\n");
  for (const k of [...hidden]) if (ofModel(k) || k === "model") hidden.delete(k);
  if (was !== now) for (const k of next) hidden.add(k);
  changed();
}

/**
 * Brings a part's row into sight: the groups it is in are folded open (their rows are seen to
 * open), and the tree is scrolled to it from where it was, no further than it takes.
 */
export function reveal(path) {
  const g = memberParts(path).groups, top = path.split("/")[0];
  open.add("project");
  open.add("model");
  // (Each group it is in, and each part it is under: a part that other things are under is folded as a group is.)
  for (let k = 1; k <= g.length; k++) { open.add(`g:${g.slice(0, k).join("/")}`); open.add(`m:${top}/${g.slice(0, k).join("/")}`); }
  keep();
  // (A part chosen while parts are being searched for: the search is put away, or its row may not be listed.)
  if (query) { find.hidden = true; find.value = ""; query = ""; }
  const opened = render();
  // (Once what opened has opened: its rows are not yet as high as they will be.)
  setTimeout(toChosen, opened ? UNFOLD + 20 : 0);
}
let going = 0;
/**
 * Scrolls the tree to the row that is chosen, from where it is, no further than it takes: far off,
 * most of the way at once, and the rest eased. The row and what scrolls the tree are found again at
 * each step, since choosing a part draws the tree again and lays the page out again meanwhile.
 */
function toChosen() {
  const mine = ++going;
  let steps = 0;
  const step = () => {
    const row = list?.querySelector(".trow.on");
    if (mine !== going || !row || !box.offsetParent || ++steps > 90) return;
    const within = scroller(), v = within.getBoundingClientRect(), r = row.getBoundingClientRect();
    // (Clear of the bar over the rows, and of the panel's foot.)
    const off = r.bottom > v.bottom - 4 ? r.bottom - (v.bottom - 4) : r.top < v.top + 18 ? r.top - (v.top + 18) : 0;
    if (Math.abs(off) < 0.5) return;
    const far = Math.abs(off) > 200 ? off - Math.sign(off) * 120 : 0;
    const by = far || (matchMedia("(prefers-reduced-motion: reduce)").matches ? off : Math.sign(off) * Math.max(1, Math.abs(off) * 0.28));
    const was = within.scrollTop;
    within.scrollTop = was + by;
    // (It can go no further: the row is as near as it gets.)
    if (within.scrollTop === was && steps > 3) return;
    // (By the clock, not by the frame: a frame is not drawn while the page is behind another, and it is to be there when it is looked at.)
    setTimeout(step, 16);
  };
  step();
}

let menuOf = null;
/** The right-click menu of a row: `fn(kind, id or key, event)`. */
export function menu(fn) { menuOf = fn; }

// ----- what is listed -------------------------------------------------------------------

const EYE = `<svg viewBox="0 0 16 16" width="14" height="14"><path d="M1.5 8s2.4-4.2 6.5-4.2S14.5 8 14.5 8s-2.4 4.2-6.5 4.2S1.5 8 1.5 8z" fill="none" stroke="currentColor" stroke-width="1.2"/><circle cx="8" cy="8" r="2" fill="currentColor"/></svg>`;
const EYE_OFF = `<svg viewBox="0 0 16 16" width="14" height="14"><path d="M1.5 8s2.4-4.2 6.5-4.2S14.5 8 14.5 8s-2.4 4.2-6.5 4.2S1.5 8 1.5 8z" fill="none" stroke="currentColor" stroke-width="1.2" opacity=".45"/><path d="M3 13L13 3" stroke="currentColor" stroke-width="1.3"/></svg>`;

/** A row: `kind` and `id` say what it is (to a click), `key` what its eye and its fold are kept under. */
export function row(depth, kind, key, name, opts = {}) {
  const tw = opts.group ? `<span class="tw" data-tw>${opts.unfolded ?? open.has(key) ? "▾" : "▸"}</span>` : `<span class="tw"></span>`;
  // (What is not drawn in a view, a drawing or a file, has no eye.)
  const eye = opts.noEye ? "" : `<span class="eye" data-eye title="Show or hide (right-click: only this)">${vis(key) ? EYE : EYE_OFF}</span>`;
  // What it is (a picture), how it stands (a dot), its name, what is said of it (in brackets); its eye at the right.
  const note = opts.note != null && opts.note !== "" ? `<span class="muted">(${esc(opts.note)})</span>` : "";
  return `<div class="trow${opts.on ? " on" : ""}${vis(key) ? "" : " off"}${opts.cls ? ` ${opts.cls}` : ""}"${kind === "top" ? "" : ` draggable="true"`} data-kind="${kind}" data-key="${esc(key)}" ${opts.id != null ? `data-id="${esc(opts.id)}"` : ""} style="padding-left:${4 + (depth + under) * 12}px" title="${esc(opts.title || name)}">${tw}<span class="ti">${iconOf(kind, key)}</span>${opts.dot ? `<i class="dot ${opts.dot}"></i>` : ""}<span class="tn">${esc(name)}</span>${note}${eye}</div>`;
}

// The model: who lists its parts (the scene: the engine's, with what a scan says of each).
let theModel = null;
/**
 * The model's parts. `parts()`: each with a `path`; `decorate(part)`: its `dot` (a class) and
 * `title` (and a `name`, to be shown in place of the last of its path); `counts(part)`: whether it
 * is one of those counted beside a group (none: every one is); `header()`: the `note`
 * and `title` of the model's own row; `selected()`: the path of the part chosen;
 * `count(parts)`: what is said beside a group (none: how many).
 */
export function model(def) { theModel = def; render(); }

function modelRows() {
  const def = theModel;
  if (!def) return "";
  const list = def.parts() || [];
  // (What is counted beside a group: what has a shape. A part that is its group is the group, not one more in it.)
  const counts = (m) => def.counts?.(m) ?? true;
  const head = def.header?.(list) ?? {};
  // (The model's own row: chosen by a click, its card says what its parts are coloured by; its arrow folds it.)
  let html = row(0, "model", "model", "Model", { group: true, id: "model", on: head.on, note: head.note ?? list.filter(counts).length, title: `${head.title ?? "Model"}. Click it to say what its parts are coloured by` });
  if (!query && !open.has("model")) return html;
  const node = { groups: new Map(), members: [], self: null };
  // (Searched: only the parts whose names have what is typed, each in its groups, all of them open.)
  const named = (m) => (def.decorate?.(m)?.name ?? memberParts(m.path).name);
  const shown = query ? list.filter((m) => named(m).toLowerCase().includes(query) || m.path.toLowerCase().includes(query)) : list;
  const at = (groups) => { let n = node; for (const g of groups) { if (!n.groups.has(g)) n.groups.set(g, { groups: new Map(), members: [], self: null }); n = n.groups.get(g); } return n; };
  for (const m of shown) at(memberParts(m.path).groups);
  // A part that other things are under (its shapes, its parts, what holds it) is the row they are under: one
  // row, which is the part, and not a part and a group of its name.
  for (const m of shown) {
    const { groups, name } = memberParts(m.path), n = at(groups), own = n.groups.get(name);
    if (own && !own.self) own.self = m; else n.members.push(m);
  }
  const all = (n) => [...(n.self ? [n.self] : []), ...n.members, ...[...n.groups.values()].flatMap(all)];
  const chosen = def.selected?.(), chosenGroup = def.selectedGroup?.();
  const member = (m, depth, opts) => {
    const said = def.decorate?.(m) ?? {};
    html += row(depth, "member", `m:${m.path}`, opts?.name ?? said.name ?? memberParts(m.path).name, { id: m.path, dot: said.dot, on: m.path === chosen, title: said.title ?? m.path, ...opts });
  };
  const walk = (n, depth, prefix) => {
    for (const [name, g] of n.groups) {
      const path = prefix ? `${prefix}/${name}` : name, inside = all(g).filter(counts);
      // (A group that holds only a group of its own name is one level, not two: what is in the inner is listed under the outer.)
      let within = g, inner = path;
      while (!within.members.length && within.groups.size === 1 && within.groups.has(name) && !within.groups.get(name).self) { within = within.groups.get(name); inner = `${inner}/${name}`; }
      const key = g.self ? `m:${g.self.path}` : `g:${path}`;
      // (A group that is all one thing, a sheet of one piece, is one row: the thing, by the group's name.)
      const whole = all(g);
      if (!query && !g.self && whole.length === 1) { member(whole[0], depth, { name }); continue; }
      // (A part of one shape and no more is one row: the part's, lit when its shape is what is chosen.)
      const only = g.self && !within.groups.size && within.members.length === 1 && !counts(g.self) ? within.members[0] : null;
      if (only) { member(g.self, depth, { on: chosen === g.self.path || chosen === only.path }); continue; }
      if (g.self) member(g.self, depth, { group: true, unfolded: query ? true : undefined, note: inside.length });
      else {
        const said = def.group?.(path, inside) ?? {};
        html += row(depth, "group", key, name, { group: true, unfolded: query ? true : undefined, id: path, on: path === chosenGroup, note: inside.length, dot: said.dot ?? "", title: said.title ?? `${path}: ${inside.length}` });
      }
      if (query || open.has(key)) walk(within, depth + 1, inner);
    }
    for (const m of n.members) member(m, depth);
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

// The tree's one root: what everything listed is of (the project), by whoever knows its name.
let rootOf = null, under = 0;
/** Says what the root row is: `f()` gives { name, title } (nothing: there is no root, and the groups stand at the top). */
export function root(f) { rootOf = f; render(); }

let box = null, list = null, find = null;
/** How long rows take to open (ms): as the page's styles have it (`.trow.new`). */
const UNFOLD = 160;
// What is typed to find a part by (lower case; nothing: all are listed).
let query = "";
// The rows as they were last drawn, by key: a row that was not there has just been opened.
let drawn = null;
const fresh = new Map();

/** What scrolls the tree: itself, or what it is put in (a panel's body). */
function scroller() {
  for (let e = box; e && e !== document.body; e = e.parentElement) {
    const o = getComputedStyle(e).overflowY;
    if ((o === "auto" || o === "scroll") && e.scrollHeight > e.clientHeight) return e;
  }
  return box;
}

/** Draws the tree again, where it was scrolled to; says whether rows were opened (they are seen to open). */
let drawnHtml = null;
export function render() {
  if (!box) return false;
  const within = scroller(), top = within.scrollTop, r = rootOf?.();
  let html = r ? row(0, "top", "project", r.name, { group: true, noEye: true, title: r.title }) : "";
  if (!r || query || open.has("project")) {
    // (Every group is a row of the root's: one step in.)
    under = r ? 1 : 0;
    html += sections.filter((s) => !query || s.key === "model").map((s) => s.rows()).join("");
    under = 0;
  }
  // (Drawn again only where it is not as it was: asked often, it mostly is, and a row replaced while it is
  // pressed is not the row let go of, so the click on it would be lost.)
  if (html === drawnHtml) return false;
  drawnHtml = html;
  // (It keeps its height while its rows are replaced, so that what scrolls it is not sent to its top.)
  list.style.minHeight = `${list.offsetHeight}px`;
  list.innerHTML = html;
  const rows = [...list.querySelectorAll(".trow")], keys = new Set(rows.map((x) => x.dataset.key));
  // (A row that has just been opened is seen to open, though the tree be drawn again while it does.)
  const now = performance.now();
  let opened = false;
  if (drawn && !query) for (const x of rows) if (!drawn.has(x.dataset.key)) { fresh.set(x.dataset.key, now + UNFOLD); opened = true; }
  for (const [k, until] of fresh) if (until < now) fresh.delete(k);
  if (!query) for (const x of rows) if (fresh.has(x.dataset.key)) x.classList.add("new");
  drawn = query ? null : keys;
  list.style.minHeight = "";
  within.scrollTop = top;
  return opened;
}

/** Every group is folded open, or every one shut (but the tree's root). */
function unfoldAll(yes) {
  if (!yes) { open.clear(); open.add("project"); keep(); render(); return; }
  // (Whoever lists a group knows its key: each pass opens what the last one showed shut.)
  for (let pass = 0; pass < 8; pass++) {
    const shut = [...list.querySelectorAll(".trow")].filter((x) => x.querySelector("[data-tw]")?.textContent === "▸").map((x) => x.dataset.key);
    if (!shut.length) break;
    for (const k of shut) open.add(k);
    drawn = null;
    render();
  }
  keep();
}

/** Shows or puts away the box a part is found by; shown, what is typed in it lists only the parts that have it. */
export function search(yes = find.hidden) {
  find.hidden = !yes;
  if (yes) { find.focus(); find.select(); }
  else { find.value = ""; find.blur(); }
  query = yes ? find.value.trim().toLowerCase() : "";
  render();
}

/** What a row of the tree is: { kind, key, id }. */
const rowOf = (e) => { const r = e.target.closest(".trow"); return r && { el: r, kind: r.dataset.kind, key: r.dataset.key, id: r.dataset.id }; };
/** The row pressed (its key, where): a click let go of where the tree was drawn again meanwhile is the pressed row's. */
let pressed = null;
const pressedRow = (e) => {
  if (!pressed || Math.hypot(e.clientX - pressed.x, e.clientY - pressed.y) > 6) return null;
  const r = list?.querySelector(`.trow[data-key="${CSS.escape(pressed.key)}"]`);
  return r && { el: r, kind: r.dataset.kind, key: r.dataset.key, id: r.dataset.id };
};

/** Makes the tree's element (once): to be put on the page by whoever shows it. */
export function mount() {
  if (box) return box;
  box = el("div", { class: "treebox" });
  // Over the rows, small and out of the way: every group open or shut, and a part found by its name.
  const ICON = {
    open: `<svg viewBox="0 0 12 12" width="11" height="11"><path d="M2.5 2.5L6 5.5l3.5-3M2.5 6.5L6 9.5l3.5-3" fill="none" stroke="currentColor" stroke-width="1.2"/></svg>`,
    shut: `<svg viewBox="0 0 12 12" width="11" height="11"><path d="M2.5 5.5L6 2.5l3.5 3M2.5 9.5L6 6.5l3.5 3" fill="none" stroke="currentColor" stroke-width="1.2"/></svg>`,
    find: `<svg viewBox="0 0 12 12" width="11" height="11"><circle cx="5" cy="5" r="3.2" fill="none" stroke="currentColor" stroke-width="1.2"/><path d="M7.4 7.4L10.5 10.5" stroke="currentColor" stroke-width="1.3"/></svg>`,
  };
  const bar = el("div", { class: "treebar" });
  bar.innerHTML = `<input class="treefind" type="search" placeholder="Find a part" spellcheck="false" hidden><span class="tb" data-do="open" title="Open every group">${ICON.open}</span><span class="tb" data-do="shut" title="Shut every group">${ICON.shut}</span><span class="tb" data-do="find" title="Find a part by its name ( / )">${ICON.find}</span>`;
  find = bar.querySelector("input");
  list = el("div", { class: "tree" });
  box.append(bar, list);
  bar.addEventListener("click", (e) => {
    const what = e.target.closest("[data-do]")?.dataset.do;
    if (what === "find") search(); else if (what) unfoldAll(what === "open");
  });
  find.addEventListener("input", () => { query = find.value.trim().toLowerCase(); render(); });
  find.addEventListener("keydown", (e) => {
    if (e.key === "Escape") { e.preventDefault(); search(false); }
    // (Enter: the first of what is found is chosen, and the tree is as it was, open to it.)
    if (e.key === "Enter") {
      const first = list.querySelector('.trow[data-kind="member"]');
      if (!first) return;
      e.preventDefault();
      const id = first.dataset.id;
      search(false);
      tell("select", "member", id);
      reveal(id);
    }
  });
  // (A slash, typed where nothing is being written, while the tree is on the page.)
  document.addEventListener("keydown", (e) => {
    const t = e.target;
    if (e.key !== "/" || e.metaKey || e.ctrlKey || e.altKey || !box.offsetParent) return;
    if (t instanceof HTMLElement && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName))) return;
    e.preventDefault();
    search(true);
  });
  // A row is dragged as what it is of (not a group's own row, which is of nothing).
  let dragged = null;
  // (Its end is listened for on the row itself: the tree may be drawn again while it is dragged, and a row no longer
  // on the page is still told, but nothing above it is.)
  const ended = (e) => { dragged = null; tell("drag", null, null, e); };
  box.addEventListener("dragstart", (e) => { const r = rowOf(e); if (r && r.kind !== "top") { dragged = r; r.el.addEventListener("dragend", ended, { once: true }); tell("drag", r.kind, r.id, e); } else e.preventDefault(); });
  // A row let go on another row (of the tree's own: "drop" says which on which; whoever knows what that means does it).
  const over = (e) => { const r = dragged && rowOf(e); return r && r.el !== dragged.el ? r : null; };
  box.addEventListener("dragover", (e) => { if (over(e)) e.preventDefault(); });
  box.addEventListener("drop", (e) => { const onto = over(e); if (!onto) return; e.preventDefault(); tell("drop", { kind: dragged.kind, id: dragged.id }, { kind: onto.kind, id: onto.id, key: onto.key }); });
  // (A row is drawn again when it is selected, so a double click is told by its timing, not by the browser.)
  let last = { key: null, t: 0 };
  box.addEventListener("mousedown", (e) => { const r = rowOf(e); pressed = r && { key: r.key, x: e.clientX, y: e.clientY }; });
  box.addEventListener("click", (e) => {
    const r = (e.target.closest(".trow") ? rowOf(e) : pressedRow(e));
    pressed = null;
    if (!r) return;
    if (e.target.closest("[data-eye]")) return toggle(r.key);
    const kind = r.kind, now = performance.now();
    const twice = last.key === r.key && now - last.t < 400;
    last = { key: r.key, t: twice ? 0 : now };
    // With shift: open it (in a view of its own). Twice: go to it.
    // (Of a group's own row, by its key; the first of the two clicks folded it, which is undone.)
    if (e.shiftKey) return tell("open", kind, kind === "top" ? r.key : r.id);
    if (twice) {
      if (kind === "top" && r.el.querySelector("[data-tw]")) { open.has(r.key) ? open.delete(r.key) : open.add(r.key); keep(); render(); }
      return tell("goto", kind, kind === "top" ? r.key : r.id);
    }
    if (kind === "top" || (e.target.closest("[data-tw]") && r.el.querySelector("[data-tw]"))) {
      // The arrow (or the row, of a heading that has no other use) folds it.
      if (r.el.querySelector("[data-tw]")) { open.has(r.key) ? open.delete(r.key) : open.add(r.key); keep(); render(); }
      return;
    }
    // (A group of the model's parts is chosen as a part is: its arrow folds it.)
    select(kind, r.id);
  });
  box.addEventListener("contextmenu", (e) => {
    const r = rowOf(e);
    // On an eye: only this (of the model: all of it).
    if (r && e.target.closest("[data-eye]") && (r.kind === "member" || r.kind === "group" || r.key === "model")) {
      e.preventDefault();
      return r.key === "model" ? showAll() : isolate(r.key);
    }
    if (!r || !menuOf) return;
    e.preventDefault();
    menuOf(r.kind, r.kind === "top" ? r.key : r.id, e);
  });
  render();
  return box;
}
