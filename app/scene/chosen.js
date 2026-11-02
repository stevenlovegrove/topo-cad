// The one thing chosen: a point (a click in a view, with the member there), a part of the model,
// a selection, a plane, a slab or a polyline: one at a time, wherever it was chosen (a view,
// the tree, a link). The card over the views says what it is; the tree shows it; a new issue
// is pinned to it. What can be done with it is in its menu, which is the same in the tree, on
// the card and in the views. What each of those says of a thing is its kind's to say (kinds.js).

import { esc, openMenu } from "../ui.js";
import { vis } from "../objects.js";
import { did } from "../actions.js";
import { active, scanned, sel, shared, shortPath, views } from "./state.js";
import { everyKind, kindOf } from "./kinds.js";
import { describeObject, describes } from "./object.js";
import { forgetInspected } from "../shell/panel.js";
import { gotoMembers } from "./links.js";
import { rebuildMarks } from "./marks.js";
import { rebuildModel, selectMember } from "./parts.js";
import { partAt } from "./exact.js";
import { drawing, endDrawing, lineLength } from "./polylines.js";
import { startDraft } from "./issues.js";
import { renderTree } from "./tree.js";
import { renderTools, toggleVis } from "./views.js";

/** Chooses one thing (of a kind, by its id; a point: what was picked), letting go of any other; no kind: nothing. */
export function choose(kind, value) {
  for (const k of everyKind()) if (k.field) sel[k.field] = k.name === kind ? value : null;
  // (Said as what was done, where it is a thing with an id: a point picked is not one that can be chosen again by name.)
  if (!kind || typeof value !== "object") did("select", kind ?? null, kind ? value : undefined);
  // (Where on a member it was clicked, when it was chosen in a view.)
  sel.at = null;
  renderChips(); rebuildMarks(); rebuildModel(); renderTree();
  // (Details tells of it: a part's own way, in parts.js; anything else's, as its kind says: object.js.)
  if (kind && kindOf(kind)?.info) describeObject(kind, kindOf(kind).find(value), value);
  // (With nothing chosen, Details tells of nothing: not of what was chosen last.)
  if (!kind) forgetInspected();
  // (What can be done with the part picked is in the views' own controls.)
  for (const v of Object.values(views)) renderTools(v);
}
/** Chooses a thing as its kind would have it chosen (a part is also shown in the tree and the panel; an issue is opened). */
export function pick(kind, id) {
  const k = kindOf(kind);
  if (k?.choose) k.choose(id); else if (k?.field) choose(kind, id);
}
/** What is chosen: { kind (its name), id, it (the thing itself), of (its kind) }, or null. */
export function chosen() {
  for (const of of everyKind()) {
    const id = of.field && sel[of.field], it = id != null && id !== false ? of.find(id) : null;
    if (it) return { kind: of.name, id, it, of };
  }
  return null;
}

/** Where a new issue can go, given what is chosen: [{ key, label }], the likeliest first. */
export function whereOptions() {
  const c = chosen();
  return [...(c?.of.where?.(c.it) ?? []), { key: "none", label: "Not pinned" }];
}

/** Puts the caret in the card's name field (of what is chosen). */
function focusName() {
  const n = views[active]?.chip.querySelector(".selname");
  if (n) { n.focus(); document.getSelection().selectAllChildren(n); }
}

/**
 * The menu of a thing: the same in the tree, on the card and in the views. Of a kind by its id (a point: what
 * was picked); or of a row of the tree that is a group ("group": of parts, by its path; "top": by its key).
 * `v`: the view it was asked for in, if one.
 */
export function objectMenu(kind, id, v) {
  if (kind === "group" || kind === "top") {
    const items = [], add = (label, run, cls) => items.push({ label, run, cls });
    const key = kind === "group" ? `g:${id}` : id;
    add(vis(key) ? "Hide" : "Show", () => toggleVis(key));
    if (kind === "group") add("Go to it", () => gotoMembers(`/${id}/`));
    else everyKind().find((k) => k.group === id)?.groupMenu?.(add);
    return items;
  }
  return menuOf((m) => m.of(kind, id), v);
}

/**
 * A menu as `build(m)` says it: what can be done first, and what is of issues (a new issue on the thing,
 * whose the thing is) last, under a line. `m` is what a kind's `menu` is given to say its own with:
 *   add(label, run, cls), head(label), divider()
 *   issue(label, where)      a new issue on the thing (pinned to it, or to `where`)
 *   last(label, run)         something else that is of issues
 *   name(), showHide()       of the thing
 *   group(label, build)      a menu of its own, opened from an item of this one: of something else that is
 *                            pointed at there (a face, a member), with what is of issues last in it too
 *   of(kind, id)             all that a kind says of one of its things, here
 */
function menuOf(build, v) {
  const items = [], after = [];
  const add = (label, run, cls) => items.push({ label, run, cls });
  const helpers = (kind, id) => {
    const of = kindOf(kind);
    return {
      add,
      head: (label) => add(label, () => {}, "head"),
      divider: () => { if (items.length && items.at(-1)) items.push(null); },
      issue: (label, where) => { if (shared()) after.push({ label, run: () => { pick(kind, id); startDraft(where ?? kind); } }); },
      last: (label, run) => after.push({ label, run }),
      name: () => add("Name it…", () => { choose(kind, id); focusName(); }),
      showHide: () => add(vis(of.key(id)) ? "Hide" : "Show", () => toggleVis(of.key(id))),
      group: (label, inner) => { const sub = menuOf((g) => inner({ ...g, ...helpersOf(g, kind, id) }), v); if (sub.length) items.push({ label, items: sub }); },
      of: (k, i) => kindOf(k)?.menu?.(i, helpers(k, i), v),
    };
  };
  // (In a group of a thing's menu, an issue is still on that thing, unless the group is of another: `of`.)
  const helpersOf = (g, kind, id) => (kind ? { issue: (label, where) => { if (shared()) g.last(label, () => { pick(kind, id); startDraft(where ?? kind); }); } } : {});
  build(helpers(null, null));
  while (items.length && !items.at(-1)) items.pop();
  return [...items, ...(items.length && after.length ? [null] : []), ...after];
}

/** A part's name with what it is of (below the model: the last three of its path), where its own name alone is one of many. */
export const placed = (path) => { const p = path.split("/").slice(1); return p.length > 3 ? `…/${p.slice(-3).join("/")}` : p.join("/") || shortPath(path); };

/**
 * The right-click menu of a view, by what is pointed at there: the member, the face and the point under the
 * cursor (`p`: what was picked there), what else the ray through the cursor meets (`ray`), and what is chosen,
 * where that is something else. Each has a menu of its own; where there is only one, it is the menu.
 */
export function contextMenu(v, p, x, y, ray) {
  const c = chosen(), under = p?.member?.path;
  // (Where the cursor is: an edge is pointed at by being near it in the view.)
  if (p) p.cursor = [x, y];
  const items = menuOf((m) => {
    if (p?.hit) m.of("point", p);
    // Along the ray: the parts behind the first, to be chosen though they are hidden by it.
    const along = ray ? partAt(v, ray, true).filter((a, i, all) => all.findIndex((b) => b.path === a.path) === i) : [];
    if (along.length > 1 || (!p?.hit && shared())) m.group("Along this ray", (g) => {
      for (const a of along.slice(0, 12)) g.add(`Choose ${placed(a.path)}`, () => selectMember(a.path));
      if (along.length > 12) g.head(`and ${along.length - 12} more behind`);
      if (shared()) g.last("New issue, not pinned", () => startDraft("none"));
    });
    if (c && c.kind !== "point" && !(c.kind === "member" && c.id === under)) m.group(`Selected: ${c.of.title(c.it)}`, (g) => g.of(c.kind, c.id));
  }, v);
  if (!items.length) return;
  // (One thing pointed at: its menu, under its name.)
  openMenu(items.length === 1 && items[0].items ? [{ label: items[0].label, run: () => {}, cls: "head" }, ...items[0].items] : items, x, y);
}

// The card, over the views: the one thing chosen, a line or two about it, and three small buttons: an issue
// on it, its menu, and letting go of it.
/** Details told again of what is chosen, as it is now (a slab dragged, its heights set, changed by someone else):
 *  where Details still tells of it; once a frame at most. */
let again = 0;
function describeAgain(c) {
  // (Not while one of its fields is being typed in.)
  if (!c?.of.info || !describes(c.kind, c.id) || document.activeElement?.closest?.("#inspector")) return;
  cancelAnimationFrame(again);
  again = requestAnimationFrame(() => { const now = chosen(); if (now && describes(now.kind, now.id)) describeObject(now.kind, now.it, now.id); });
}

export function renderChips() {
  const c = chosen(), card = c?.of.card(c.it);
  describeAgain(c);
  for (const v of Object.values(views)) {
    // A polyline being drawn: what to do, and how to end.
    if (drawing) {
      v.chip.hidden = drawing.view !== v.name;
      const n = drawing.points.length;
      v.chip.innerHTML = `<div class="cardhead"><span><b>Polyline</b> ${n} point${n === 1 ? "" : "s"}${n > 1 ? `, ${lineLength(drawing).toFixed(3)} m` : ""}</span><span class="cardbtns"><button data-a="done" title="Keep it (Enter, or a double click)">✓</button><button data-a="cancel" title="Drop it (Esc)">×</button></span></div><div class="muted">Click points ${v.state.flat && v.state.box ? "in the slab" : scanned() ? "on the scan" : "on the model"}; click the first again to close it.</div>`;
      for (const b of v.chip.querySelectorAll("button")) b.onclick = () => endDrawing(b.dataset.a === "done");
      v.chip.oncontextmenu = null;
      continue;
    }
    v.chip.hidden = !c;
    if (!c) continue;
    // Being named or sized: leave it as it is.
    if (v.chip.contains(document.activeElement) && (document.activeElement.classList.contains("selname") || document.activeElement.tagName === "INPUT")) continue;
    const named = card.name ? `<span class="selname" contenteditable="plaintext-only" data-ph="name it" title="Its name: type, Enter keeps it">${esc(card.name.text ?? "")}</span>` : card.note ? `<span class="muted">${card.note}</span>` : "";
    v.chip.innerHTML = `<div class="cardhead"><span><b>${card.label}</b> ${esc(card.id)} ${named}</span><span class="cardbtns">${shared() ? `<button data-a="issue" title="New issue on this">＋</button>` : ""}<button data-a="menu" title="What can be done with it (the same as right-clicking it)">⋯</button><button data-a="clear" title="Let go of it">×</button></span></div><div class="muted">${card.body}</div>`;
    const name = v.chip.querySelector(".selname");
    if (name) {
      const was = card.name.text ?? "";
      name.onkeydown = (e) => { e.stopPropagation(); if (e.key === "Enter") { e.preventDefault(); name.blur(); } if (e.key === "Escape") { name.textContent = was; name.blur(); } };
      name.onblur = () => { const t = name.textContent.trim(); if (t !== was) card.name.save(t); };
    }
    card.bind?.(v.chip);
    for (const b of v.chip.querySelectorAll("button")) b.onclick = (e) => {
      const a = b.dataset.a;
      if (a === "clear") choose(null);
      else if (a === "issue") startDraft();
      else { const r = b.getBoundingClientRect(); openMenu(objectMenu(c.kind, c.id, v), r.left, r.top - 4); e.stopPropagation(); }
    };
    v.chip.oncontextmenu = (e) => { e.preventDefault(); openMenu(objectMenu(c.kind, c.id, v), e.clientX, e.clientY); };
  }
}
