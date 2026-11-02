// The page below its header: a dock. In the middle, the documents of the project that are open
// (a drawing, a file, a 3D view, …), any number of them, in panes that are side by side or one
// over another, each a stack of tabs. Round the middle, on the left, the right and below, the
// tools (the project's lists, the tree of objects, the checks, …), in stacks of tabs of their
// own. A tab is dragged by its name: onto a pane's tabs to join them, onto a pane's middle to be
// shown there, onto one of its sides to stand there. A document goes among the documents and a
// tool among the tools.
//
// What there is to show is said by whoever has it:
//
//   defineType(type, { title(id), label(id), make(id, kept), valid(id), replaces })
//       a kind of document. `make` gives one of them: { el, shown(), dispose(), keep(),
//       update(kept) } (what it is on the page; what to do when it comes into sight and when
//       it is closed; what of it to keep with the layout, and taking that up again).
//   addTool(name, { title, label(), el, side, beside, over, fit })
//       a tool: one element, there for good; where it stands until it is moved (on a side; `beside`
//       another tool, a tab with it; or `over` one, in a stack of its own above that one's; `fit`: it
//       is a short list, given the room it needs rather than a share).
//   opensFor(prefix, f)
//       what can be dragged or opened that is not itself a document (a thing of the model):
//       `f(rest, onto)` gives the document's key to open for it (or does it in `onto`, the
//       document it was dropped on, and gives nothing).
//
// A document's key is `type:id`; a tool's, `tool:name`.

import { $, el, esc, store, stored } from "../ui.js";

const types = new Map(), openers = new Map();
/** What is on the page: key → { key, type, id, def, el, shown, dispose, keep, update }. Tools always; documents while open. */
const items = new Map();
const infos = {};
const stack = (tabs = [], active = tabs[0] ?? null) => ({ tabs, active });

// The layout: the middle (a tree: a stack, or { dir: "row" | "col", children, sizes }), and each side's
// (the same, or none), with the sides' sizes (px) and which are shut.
let centre = stack();
const sides = { left: null, right: null, bottom: null };
const size = { left: 250, right: 420, bottom: 240 }, shut = {};
// On a narrow page (a phone) the sides are shut until they are opened, and that is kept apart from how
// they stand on a wide one: so a layout made at a desk is not undone by looking at it on a phone.
const NARROW = 760, shutNarrow = {};
const narrow = () => window.innerWidth < NARROW;
const isShut = (side) => (narrow() ? shutNarrow[side] ?? true : !!shut[side]);
function setShut(side, yes) {
  if (!narrow()) { shut[side] = yes; return; }
  shutNarrow[side] = yes;
  // (There is room for one of them open.)
  if (!yes) for (const k of Object.keys(sides)) if (k !== side) shutNarrow[k] = true;
}
/** The project whose documents these are (its layout is kept under its name). */
let project = null;
/** The pane last used, and the document of each type last looked at. */
let focus = null;
const lastOf = {};
let drag = null;   // what is being dragged: { key, tool }
const laidOut = [];

const typeOf = (key) => key.slice(0, key.indexOf(":"));
const idOf = (key) => key.slice(key.indexOf(":") + 1);
const isTool = (key) => typeOf(key) === "tool";

export function defineType(type, def) { types.set(type, def); }
export function opensFor(prefix, f) { openers.set(prefix, f); }
export function addTool(name, def) {
  const key = `tool:${name}`;
  items.set(key, { key, type: "tool", id: name, def: { side: "left", ...def }, el: def.el });
}
/** A tool that turns out to have nothing to show here, gone from wherever it was, and from the layout kept. */
export function dropTool(name) {
  const key = `tool:${name}`, f = find(key);
  items.delete(key);
  if (!f) return;
  f.stack.tabs = f.stack.tabs.filter((k) => k !== key);
  if (f.stack.active === key) f.stack.active = f.stack.tabs[0] ?? null;
  layOut();
}
/** `f()` whenever the dock has been laid out again (what is open, and where, may have changed). */
export const onLayout = (f) => laidOut.push(f);

// ----- the trees ------------------------------------------------------------------------------

const stacksOf = (n) => (!n ? [] : n.tabs ? [n] : n.children.flatMap(stacksOf));
const roots = () => [["centre", centre], ...Object.entries(sides)];
const everyStack = () => roots().flatMap(([, n]) => stacksOf(n));
/** Where a key is: { stack, side ("centre", "left", …) }, or null. */
function find(key) {
  for (const [side, n] of roots()) for (const s of stacksOf(n)) if (s.tabs.includes(key)) return { stack: s, side };
  return null;
}
const sideOf = (s) => roots().find(([, n]) => stacksOf(n).includes(s))?.[0] ?? null;
const setRoot = (side, n) => { if (side === "centre") centre = n ?? stack(); else sides[side] = n; };
const rootOf = (side) => (side === "centre" ? centre : sides[side]);
function parentOf(n, child) {
  if (!n || n.tabs) return null;
  if (n.children.includes(child)) return n;
  for (const c of n.children) { const p = parentOf(c, child); if (p) return p; }
  return null;
}
/** A tree without its empty stacks (a split left with one child is that child); none, if nothing is left. */
function pruned(n, keepEmpty) {
  if (!n) return null;
  if (n.tabs) return n.tabs.length || keepEmpty ? n : null;
  const kids = [], sizes = [];
  n.children.forEach((c, i) => { const p = pruned(c, false); if (p) { kids.push(p); sizes.push(n.sizes[i]); } });
  if (kids.length < 2) return kids[0] ?? null;
  n.children = kids; n.sizes = sizes;
  return n;
}
/** Puts a new stack on a side ("left", "right", "top", "bottom") of another, in the same tree, taking half its room. */
function splitOff(at, where, fresh) {
  const side = sideOf(at), dir = where === "left" || where === "right" ? "row" : "col", after = where === "right" || where === "bottom";
  const p = parentOf(rootOf(side), at);
  if (p?.dir === dir) {
    const i = p.children.indexOf(at), s = p.sizes[i] / 2;
    p.sizes[i] = s;
    p.children.splice(i + (after ? 1 : 0), 0, fresh); p.sizes.splice(i + (after ? 1 : 0), 0, s);
    return;
  }
  const split = { dir, children: after ? [at, fresh] : [fresh, at], sizes: [1, 1] };
  if (p) p.children[p.children.indexOf(at)] = split; else setRoot(side, split);
}
function tidy() {
  centre = pruned(centre, false) ?? stack();
  for (const k of Object.keys(sides)) sides[k] = pruned(sides[k], false);
  for (const s of everyStack()) if (!s.tabs.includes(s.active)) s.active = s.tabs[0] ?? null;
  if (!stacksOf(centre).includes(focus)) focus = stacksOf(centre)[0];
}

// ----- what is open ---------------------------------------------------------------------------

/** The thing on the page for a key (made, if it is a document not yet open); null if there is no such thing. */
function ensure(key, kept) {
  if (items.has(key)) return items.get(key);
  const type = typeOf(key), id = idOf(key), def = types.get(type);
  if (!def || def.valid?.(id) === false) return null;
  const made = def.make(id, kept);
  if (!made) return null;
  // (The thing itself, as whoever made it has it: what they change of it is seen here.)
  const item = Object.assign(made, { key, type, id, def });
  items.set(key, item);
  return item;
}
function discard(key) {
  const item = items.get(key);
  if (!item || item.type === "tool") return;
  item.el.remove();
  item.dispose?.();
  items.delete(key);
  delete infos[key];
}
/** A key that names a document, for one that may name something else that a document is opened for. */
function resolved(key, onto) {
  const f = openers.get(typeOf(key));
  return f ? f(idOf(key), onto) : key;
}

/** Whether a document is open (in any pane, in sight or not); and whether a document or a tool is in sight. */
export const isOpen = (key) => !!find(key);
export const isShown = (key) => { const f = find(key); return !!f && f.stack.active === key && !f.stack.folded && (f.side === "centre" || !isShut(f.side)); };
/** The documents open of a type: their items (as `make` gave them, with `key` and `id`). */
export const openOf = (type) => [...items.values()].filter((i) => i.type === type);
/** The document of a type to act in: the one in the pane last used, else the one last looked at, else any. */
export function current(type) {
  const here = focus && items.get(focus.active);
  if (here?.type === type) return here;
  return (lastOf[type] && items.get(lastOf[type])) ?? openOf(type)[0] ?? null;
}

/** The type of the document the pane last used shows ("view", "sheet", "file", …; none: null): where a key pressed is meant. */
export const focused = () => (focus && items.get(focus.active))?.type ?? null;

/** Shows `key` in a stack: in place of what it shows, if that is of the same type and can be; else as one more tab. */
function place(s, key, asTab) {
  const was = items.get(s.active), item = items.get(key);
  if (!asTab && was && was.type === item.type && item.type !== "tool" && item.def.replaces !== false) {
    s.tabs[s.tabs.indexOf(was.key)] = key;
    discard(was.key);
  } else s.tabs.splice(s.tabs.indexOf(s.active) + 1, 0, key);
  s.active = key;
  s.folded = false;
}

/**
 * Opens a document (or what a document is opened for), or brings it to the front if it is open. In the pane
 * last used: in place of what that shows (a drawing for a drawing), else as one more tab there. `beside`: in a
 * pane of its own, beside that one. `tab`: as a tab, whatever is shown. Its item (as `make` gave it).
 */
export function openDoc(key, { beside = false, tab = false } = {}) {
  key = resolved(key, null);
  if (!key) return null;
  if (isOpen(key) || isTool(key)) return bring(key);
  const item = ensure(key);
  if (!item) return null;
  tidy();
  if (beside && focus.tabs.length) { const fresh = stack([key]); splitOff(focus, "right", fresh); focus = fresh; }
  else place(focus, key, tab);
  layOut();
  return item;
}
/**
 * Brings what is open (or a tool) to the front of its stack, unfolded; a document's pane becomes the one last
 * used, and a tool's side is opened if it was put away. `quiet`: a tool is only made the one its stack shows:
 * what was put away, or folded, stays so (what fills a tool as things are chosen does not open it each time).
 */
export function bring(key, { quiet = false } = {}) {
  const f = find(key);
  if (!f) return null;
  if (quiet && f.side !== "centre" && f.stack.active === key) return items.get(key);
  f.stack.active = key;
  if (f.side === "centre") { f.stack.folded = false; focus = f.stack; }
  else if (!quiet) { f.stack.folded = false; setShut(f.side, false); }
  layOut();
  return items.get(key);
}
function closeDoc(key) {
  const f = find(key);
  if (!f || isTool(key)) return;
  const i = f.stack.tabs.indexOf(key);
  f.stack.tabs.splice(i, 1);
  if (f.stack.active === key) f.stack.active = f.stack.tabs[Math.min(i, f.stack.tabs.length - 1)] ?? null;
  discard(key);
  layOut();
}
/** Says something beside a pane's tabs, of the document it shows (kept while the panes are laid out again). */
export function setInfo(key, text) {
  infos[key] = text;
  const f = find(key), e = f && f.stack.active === key && f.stack.info;
  if (e) e.textContent = text ?? "";
}
/** The names on the tabs have changed (a file has edits, there are more issues, …). */
export function retitle() {
  for (const t of document.querySelectorAll("#dock .tab")) {
    const item = items.get(t.dataset.key), name = t.querySelector(".name");
    if (item && name) { name.innerHTML = labelOf(item); t.title = titleOf(item); }
  }
}
const titleOf = (item) => (item.type === "tool" ? item.def.title : item.def.title(item.id));
const labelOf = (item) => (item.type === "tool" ? item.def.label?.() : item.def.label?.(item.id)) ?? esc(titleOf(item));

// ----- dropping -------------------------------------------------------------------------------

/** Whether what is dragged may land in a stack: a tool among the tools, a document among the documents. */
const fits = (s) => drag && drag.tool === (sideOf(s) !== "centre");
/** Where in a stack's body the cursor is: "into" (its middle), or a side it would stand on. A side's stacks are only split along the side. */
function zoneAt(s, body, e) {
  const r = body.getBoundingClientRect(), x = (e.clientX - r.left) / r.width, y = (e.clientY - r.top) / r.height;
  if (Math.abs(x - 0.5) < 0.2 && Math.abs(y - 0.5) < 0.2) return "into";
  const side = sideOf(s), d = side === "centre" ? { left: x, right: 1 - x, top: y, bottom: 1 - y } : side === "bottom" ? { left: x, right: 1 - x } : { top: y, bottom: 1 - y };
  const z = Object.keys(d).reduce((a, b) => (d[b] < d[a] ? b : a));
  return side !== "centre" && d[z] > 0.3 ? "into" : z;
}
/** What is dragged is let go on a stack: among its tabs ("tab"), in its middle ("into"), or on a side of it. */
function dropOn(s, zone) {
  const { key: raw } = drag;
  // (Here, not left to the drag's own end: that is told to what was dragged, which may be gone from the page by then.)
  endDrag();
  const key = resolved(raw, zone === "into" ? items.get(s.active) : null);
  if (!key) return layOut();
  const from = find(key);
  if (from && from.stack === s && (zone === "into" || zone === "tab" || s.tabs.length === 1)) { s.active = key; return layOut(); }
  if (!from && !ensure(key)) return layOut();
  if (from) { from.stack.tabs.splice(from.stack.tabs.indexOf(key), 1); if (from.stack.active === key) from.stack.active = from.stack.tabs[0] ?? null; }
  if (zone === "into" || zone === "tab") place(s, key, zone === "tab" || !!from);
  else splitOff(s, zone, stack([key]));
  const now = find(key);
  if (now.side === "centre") focus = now.stack;
  layOut();
}
/** A tool is let go on an edge of the page where no tools stand: it stands there. */
function dropOnEdge(side) {
  const { key } = drag, from = find(key);
  endDrag();
  if (from) from.stack.tabs.splice(from.stack.tabs.indexOf(key), 1);
  sides[side] = stack([key]);
  setShut(side, false);
  layOut();
}
/** Makes an element something that can be dragged into the dock as `key()` (a document's, a tool's, or what an opener takes). */
export function draggable(e, key) {
  e.draggable = true;
  e.addEventListener("dragstart", (ev) => beginDrag(ev, key()));
  e.addEventListener("dragend", endDrag);
}
/** A drag has begun (`ev`) of what `key` names (nothing: it is not let begin); and has ended, wherever. */
export function beginDrag(ev, key) {
  if (!key) return ev.preventDefault();
  ev.dataTransfer.setData("text/plain", key);
  ev.dataTransfer.effectAllowed = "move";
  drag = { key, tool: isTool(key) };
  // (After the drag has begun: changing the page under it would end it.)
  setTimeout(() => $("dock").classList.add("dragging", drag?.tool ? "tool" : "doc"), 0);
}
export function endDrag() {
  drag = null;
  $("dock").classList.remove("dragging", "tool", "doc");
}

// ----- laying out -----------------------------------------------------------------------------

function stackEl(s) {
  const side = sideOf(s), box = el("div", { class: `stack${s === focus ? " focus" : ""}${s.folded ? " folded" : ""}` });
  const tabs = el("div", { class: "tabs" });
  for (const key of s.tabs) {
    const item = items.get(key);
    if (!item) continue;
    const t = el("div", { class: `tab${key === s.active ? " on" : ""}`, "data-key": key, title: titleOf(item) }, `<span class="name">${labelOf(item)}</span>`);
    draggable(t, () => key);
    t.onclick = () => {
      // (A tool's own tab, clicked again: its stack folds away, or back.)
      if (side !== "centre" && s.active === key) s.folded = !s.folded; else { s.active = key; s.folded = false; }
      if (side === "centre") focus = s;
      layOut();
    };
    if (item.type !== "tool") {
      const x = el("span", { class: "x", title: "Close" }, "×");
      x.onclick = (e) => { e.stopPropagation(); closeDoc(key); };
      t.append(x);
      t.onauxclick = (e) => { if (e.button === 1) closeDoc(key); };
    }
    tabs.append(t);
  }
  s.info = el("span", { class: "info" });
  s.info.textContent = infos[s.active] ?? "";
  tabs.append(s.info);
  // Onto its tabs: one more of them.
  tabs.ondragover = (e) => { if (fits(s)) { e.preventDefault(); tabs.classList.add("over"); } };
  tabs.ondragleave = () => tabs.classList.remove("over");
  tabs.ondrop = (e) => { if (fits(s)) { e.preventDefault(); dropOn(s, "tab"); } };
  const body = el("div", { class: "stackbody" }), active = items.get(s.active);
  if (active) body.append(active.el);
  else if (side === "centre") body.append(el("div", { class: "nothing muted" }, "Nothing is open here. Click a drawing, a file or the 3D view on the left to open it; shift-click opens it beside what is open; or drag it to where it should stand."));
  // Onto what it shows: in its middle, to be shown here; on a side, to stand there.
  const drop = el("div", { class: "drop" }), hint = el("i");
  drop.append(hint);
  drop.ondragover = (e) => {
    if (!fits(s)) return;
    e.preventDefault();
    const z = zoneAt(s, drop, e), half = (a, b) => (z === a || z === b ? "50%" : z === "into" ? "60%" : "100%");
    Object.assign(hint.style, { display: "block", left: z === "right" ? "50%" : z === "into" ? "20%" : "0", top: z === "bottom" ? "50%" : z === "into" ? "20%" : "0", width: half("left", "right"), height: half("top", "bottom") });
  };
  drop.ondragleave = () => { hint.style.display = "none"; };
  drop.ondrop = (e) => { if (fits(s)) { e.preventDefault(); dropOn(s, zoneAt(s, drop, e)); } };
  body.append(drop);
  if (side === "centre") box.addEventListener("pointerdown", () => {
    if (focus === s) return;
    focus = s;
    if (active) lastOf[active.type] = active.key;
    for (const b of document.querySelectorAll("#dock .stack")) b.classList.toggle("focus", b === box);
    for (const f of laidOut) f();
  }, true);
  box.append(tabs, body);
  return box;
}

/** The bar between two things that share room: dragged, it gives one what it takes from the other. */
function gutter(row, set, done) {
  const g = el("div", { class: `gutter${row ? "" : " over"}`, title: "Drag to resize" });
  g.onpointerdown = (e) => {
    e.preventDefault();
    g.setPointerCapture(e.pointerId);
    g.classList.add("on");
    g.onpointermove = (m) => { set(row ? m.clientX : m.clientY); window.dispatchEvent(new Event("resize")); };
    g.onpointerup = () => { g.onpointermove = g.onpointerup = null; g.classList.remove("on"); done(); };
  };
  return g;
}
// (Shares of the room there is, as flex takes them: made large, since shares that come to less than one whole
// leave room unused, as they do when a neighbour is folded or fitted and takes none.)
const grow = (share) => `${Math.max(share, 0.01) * 1000} 1 0`;
function nodeEl(n) {
  if (n.tabs) return stackEl(n);
  const row = n.dir === "row", box = el("div", { class: `split${row ? "" : " over"}` });
  const kids = n.children.map(nodeEl);
  kids.forEach((e, i) => {
    if (i) {
      const a = kids[i - 1];
      box.append(gutter(row, (at) => {
        const ra = a.getBoundingClientRect(), rb = e.getBoundingClientRect(), from = row ? ra.left : ra.top, span = (row ? rb.right : rb.bottom) - from;
        const f = Math.max(0.08, Math.min(0.92, (at - from) / Math.max(1, span))), total = n.sizes[i - 1] + n.sizes[i];
        n.sizes[i - 1] = f * total; n.sizes[i] = (1 - f) * total;
        // (Sized by hand: no longer by what is in them.)
        n.children[i - 1].sized = n.children[i].sized = true;
        a.style.flex = grow(n.sizes[i - 1]); e.style.flex = grow(n.sizes[i]);
        a.style.maxHeight = e.style.maxHeight = "";
      }, keep));
    }
    // (A stack folded away takes only its tabs' room; one of short lists, till it is sized by hand, the room they need.)
    const c = n.children[i], fitted = !row && c.tabs && !c.sized && c.tabs.every((k) => items.get(k)?.def.fit);
    e.style.flex = c.folded || fitted ? "0 0 auto" : grow(n.sizes[i]);
    if (fitted && !c.folded) e.style.maxHeight = "38%";
    box.append(e);
  });
  return box;
}

/** A side of the page: its tools, at its size; shut, a strip to open it by; with none, where one can be dropped. */
function sideEl(side) {
  const n = sides[side], row = side !== "bottom";
  if (!n) { const edge = el("div", { class: `edge ${side}` }); edge.ondragover = (e) => { if (drag?.tool) { e.preventDefault(); edge.classList.add("over"); } }; edge.ondragleave = () => edge.classList.remove("over"); edge.ondrop = (e) => { if (drag?.tool) { e.preventDefault(); dropOnEdge(side); } }; return [edge]; }
  // Which way its arrow points: toward where it goes when it is shut (and from there, when it is opened).
  const arrow = (open) => ({ left: open ? "‹" : "›", right: open ? "›" : "‹", bottom: open ? "⌄" : "⌃" })[side];
  if (isShut(side)) {
    const strip = el("div", { class: `shut ${side}`, title: "Show the tools here" });
    strip.append(el("span", { class: "grip" }, arrow(false)));
    strip.onclick = () => { setShut(side, false); layOut(); };
    return [strip];
  }
  const area = el("div", { class: `area ${side}` });
  // (On a narrow page, no wider than leaves some of what is behind it in sight.)
  const room = () => (narrow() ? Math.min(size[side], (row ? window.innerWidth : window.innerHeight) * 0.82) : size[side]);
  area.style[row ? "width" : "height"] = `${room()}px`;
  area.append(nodeEl(n));
  const g = gutter(row, (at) => {
    const r = area.getBoundingClientRect(), to = side === "left" ? at - r.left : side === "right" ? r.right - at : r.bottom - at;
    size[side] = Math.max(120, Math.min(row ? window.innerWidth * 0.6 : window.innerHeight * 0.7, to));
    area.style[row ? "width" : "height"] = `${room()}px`;
  }, keep);
  g.title = "Drag to resize; click the arrow (or double-click) to put these tools away";
  g.ondblclick = () => { setShut(side, true); layOut(); };
  // The arrow on the bar: clicked, the tools are put away. (It is not the bar: pressing it does not begin a resize.)
  const grip = el("span", { class: "grip", title: "Put these tools away" }, arrow(true));
  grip.onpointerdown = (e) => e.stopPropagation();
  grip.ondblclick = (e) => e.stopPropagation();
  grip.onclick = (e) => { e.stopPropagation(); setShut(side, true); layOut(); };
  g.classList.add("side");
  g.append(grip);
  return side === "left" ? [area, g] : [g, area];
}

let seen = new Set();
/** Puts the layout on the page: every stack, with what each shows; and keeps it. */
function layOut() {
  // (A tool not yet anywhere stands where it says: with the tool it names, or in a stack of its own on its side.)
  for (const item of items.values()) {
    if (item.type !== "tool" || find(item.key)) continue;
    const mate = item.def.beside && find(`tool:${item.def.beside}`), under = item.def.over && find(`tool:${item.def.over}`);
    if (under) splitOff(under.stack, "top", stack([item.key]));
    else if (mate) mate.stack.tabs.push(item.key);
    else if (!sides[item.def.side]) sides[item.def.side] = stack([item.key]);
    else splitOff(stacksOf(sides[item.def.side]).at(-1), item.def.side === "bottom" ? "right" : "bottom", stack([item.key]));
  }
  tidy();
  const dock = $("dock"), parked = $("parked");
  // (What each thing shown was scrolled to: its stack is made again, and is put back there.)
  const scrolled = new Map([...items.values()].filter((item) => item.el.parentElement?.classList.contains("stackbody")).map((item) => [item, item.el.parentElement.scrollTop]));
  for (const item of items.values()) parked.append(item.el);
  const mid = el("div", { class: "mid" });
  const middle = el("div", { class: "centre" });
  middle.append(nodeEl(centre));
  mid.append(middle, ...sideEl("bottom"));
  dock.replaceChildren(...sideEl("left"), mid, ...sideEl("right"));
  for (const [item, top] of scrolled) if (top && item.el.parentElement?.classList.contains("stackbody")) item.el.parentElement.scrollTop = top;
  // What has come into sight is told so (and everything, that its room may have changed).
  const now = new Set(everyStack().filter((s) => !s.folded && s.active).map((s) => s.active));
  for (const key of now) { const item = items.get(key); if (item && !seen.has(key)) item.shown?.(); if (item && item.type !== "tool" && find(key).stack === focus) lastOf[item.type] = key; }
  seen = now;
  window.dispatchEvent(new Event("resize"));
  keep();
  for (const f of laidOut) f();
}

// ----- keeping --------------------------------------------------------------------------------

const bare = (n) => (!n ? null : n.tabs ? { tabs: n.tabs.slice(), active: n.active, folded: !!n.folded, sized: !!n.sized } : { dir: n.dir, sizes: n.sizes.slice(), children: n.children.map(bare) });
/** What is open and where, with what each document keeps of itself: to go back to (`apply`). */
function snapshot() {
  const kept = {};
  for (const item of items.values()) if (item.type !== "tool" && item.keep) kept[item.key] = item.keep();
  return { centre: bare(centre), kept };
}
/** Keeps the layout: the tools' (for every project), and the open project's documents (under its name). */
export function keep() {
  store("topo.dock.tools", { v: 2, sides: Object.fromEntries(Object.entries(sides).map(([k, n]) => [k, bare(n)])), size, shut, shutNarrow });
  if (project) store(`topo.dock:${project}`, snapshot());
}
/** Makes the documents what a snapshot has: opening what it has that is not open, closing what it has not, and giving
 *  the rest what was kept of them. */
function apply(snap) {
  const wanted = new Set(stacksOf(snap.centre).flatMap((s) => s.tabs));
  for (const item of [...items.values()]) if (item.type !== "tool" && !wanted.has(item.key)) discard(item.key);
  for (const key of wanted) { const had = items.get(key); if (had) had.update?.(snap.kept?.[key]); else ensure(key, snap.kept?.[key]); }
  // (What could not be made again, a drawing the model no longer has, is not in it.)
  centre = bare(snap.centre);
  for (const s of stacksOf(centre)) s.tabs = s.tabs.filter((k) => items.has(k));
  focus = null;
  layOut();
}
/** The project the page is of is open: its documents as they were left; the first time, `first()` says what to open. */
export function openProject(name, first) {
  // (What was opened while it was being opened (its files, the build log), kept open, in front: with what was left
  // open of it before laid out about them.)
  const meanwhile = stacksOf(centre).flatMap((s) => s.tabs), front = focus?.active;
  const kept = stored(`topo.dock:${name}`);
  apply(kept?.centre ? kept : { centre: stack(), kept: {} });
  project = name;
  if (!stacksOf(centre).some((s) => s.tabs.length)) for (const key of first()) openDoc(key, { tab: true });
  for (const key of meanwhile) if (!isOpen(key)) openDoc(key, { tab: true });
  if (front && isOpen(front)) bring(front);
  keep();
}
/** The tools back where they stand to begin with, and the documents in one pane. */
export function resetLayout() {
  for (const k of Object.keys(sides)) { sides[k] = null; shut[k] = false; delete shutNarrow[k]; }
  Object.assign(size, { left: 250, right: 420, bottom: 240 });
  const all = stacksOf(centre).flatMap((s) => s.tabs);
  centre = stack(all, focus?.active ?? all[0]);
  layOut();
}

/** Puts the dock on the page, the tools where they were left. */
export function initDock() {
  const kept = stored("topo.dock.tools");
  if (kept) {
    // (A tool that is no longer one is not kept a place.)
    const known = (n) => (!n ? null : n.tabs ? { ...n, tabs: n.tabs.filter((k) => items.has(k)) } : { ...n, children: n.children.map(known) });
    for (const k of Object.keys(sides)) sides[k] = known(kept.sides?.[k]);
    Object.assign(size, kept.size);
    Object.assign(shut, kept.shut);
    Object.assign(shutNarrow, kept.shutNarrow);
    // (A layout kept before a tool was given a stack of its own, where the tool is still a tab with the one it
    // now stands over: it is taken out, and goes where it stands to begin with.)
    if ((kept.v ?? 1) < 2) for (const item of items.values()) {
      const at = item.type === "tool" && item.def.over && find(item.key);
      if (at && at.stack.tabs.includes(`tool:${item.def.over}`)) { at.stack.tabs.splice(at.stack.tabs.indexOf(item.key), 1); if (at.stack.active === item.key) at.stack.active = at.stack.tabs[0]; }
    }
  }
  // The page made narrow, or wide again: its sides are as they stand at that width.
  let was = narrow();
  window.addEventListener("resize", () => { if (narrow() !== was) { was = narrow(); layOut(); } });
  window.addEventListener("beforeunload", keep);
  layOut();
}
