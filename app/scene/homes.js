// Where a thing put in the scene belongs. A slab, a polyline, a selection, a plane or a view kept
// has one home: an issue (it was made while that issue was open, or the issue was raised on it),
// the project (it was named, or kept on purpose), or neither: scratch, made in passing, there
// until what is scratch is cleared. So what gathers is tidied by where things are:
//
//   - in the tree, a thing is listed in one place: with its issue, with the project's of its kind,
//     or under "Unsorted";
//   - in the views, an issue's things are drawn while that issue is open (or is kept in sight):
//     nothing is switched on or off for that, so closing the issue leaves everything as it was;
//   - a thing is moved by its menu, or by dragging its row onto an issue, a project's group, or
//     "Unsorted".

import { store, stored } from "../ui.js";
import { workspace } from "../workspace.js";
import * as objects from "../objects.js";
import { sel, shared, st } from "./state.js";
import { everyKind, kind, kindOf } from "./kinds.js";

/** A thing's home: its issue's number, "project", or "scratch". */
export const homeOf = (x) => x.issue ?? (x.kept && !x.temp ? "project" : "scratch");
/** What a thing made now is made with: the issue that is open, where there are issues (else nothing: it is scratch till named). */
export const madeIn = () => (shared() && sel.issue != null ? { issue: sel.issue } : {});

// The issues whose things are drawn though they are not open (asked for, of each; kept).
const inSight = new Set(stored("topo.issues.insight", []));
/** Whether a thing is drawn as far as its home goes: an issue's, while the issue is open; what is unsorted, while that group's eye is on (`chosen`: it is the one chosen, which always is). */
export const homeSeen = (x, chosen) => !!chosen || (x.issue == null ? homeOf(x) !== "scratch" || objects.vis("scratch") : x.issue === sel.issue || inSight.has(x.issue));
export const keptInSight = (issue) => inSight.has(issue);
export function keepInSight(issue, on) {
  on ? inSight.add(issue) : inSight.delete(issue);
  store("topo.issues.insight", [...inSight]);
  objects.changed();
}

/** Moves a thing (of a kind, by its id) to a home: an issue's number, "project" or "scratch". */
export async function rehome(kindName, id, to) {
  const r = await workspace.call("objects", { action: "move", to, ...(kindName === "saved" ? { view: kindOf("saved").find(id)?.name } : { id }) });
  if (r.error) alert(r.error);
}
/** The kinds of thing that have a home, each with all there are of it (`all`) and a row for one (`rowOf`). */
const homed = () => everyKind().filter((k) => k.all);
/** The things of every kind that are in a home, as rows at a depth. */
export const rowsOf = (home, row, depth, opts) => homed().flatMap((k) => k.all().filter((x) => homeOf(x) === home).map((x) => k.rowOf(row, x, depth, opts)));
export const countOf = (home) => homed().reduce((n, k) => n + k.all().filter((x) => homeOf(x) === home).length, 0);

/** What a thing's menu offers about where it belongs. */
export function homeMenu(m, kindName, x, id) {
  const home = homeOf(x);
  if (x.temp) return;
  // (Whose it is, an issue's or the project's: with what else is of issues, last in its menu.)
  if (home !== "project") m.last("Keep it with the project", () => rehome(kindName, id, "project"));
  if (shared() && sel.issue != null && home !== sel.issue) m.last(`Make it issue #${sel.issue}'s`, () => rehome(kindName, id, sel.issue));
  if (home !== "scratch" && kindName !== "saved") m.last("Let it go (to Unsorted)", () => rehome(kindName, id, "scratch"));
}

// What is nobody's, in the tree: one group, of every kind, to clear when it is done with.
kind("scratch", {
  group: "scratch", order: 90,
  rows: (row, list) => (countOf("scratch") ? list("scratch", "Unsorted", () => rowsOf("scratch", row, 1, { cls: "loose" }), { note: countOf("scratch"), title: "What was made in passing: neither an issue's nor kept with the project. Name one, or drag it onto an issue or a group above, to keep it; right-click here to clear them" }) : ""),
  groupMenu: (add) => add("Clear these", async () => {
    if (!confirm(`Clear the ${countOf("scratch")} things that are neither an issue's nor kept with the project?`)) return;
    // (The ones that are only this page's, slabs from links not kept, go too.)
    for (const k of homed()) k.clear?.();
    const r = await workspace.call("objects", { action: "clear" });
    if (r.error) alert(r.error);
  }, "danger"),
});

// A row dragged onto another: onto an issue, it is that issue's; onto a group of the project's (or the project), the
// project's; onto "Unsorted", nobody's.
const PROJECT = new Set(["project", "slabs", "polylines", "selections", "planes", "views"]);
objects.on("drop", (dragged, onto) => {
  if (!kindOf(dragged.kind)?.all) return;
  const to = onto.kind === "issue" ? Number(onto.id) : onto.kind === "top" && onto.key === "scratch" ? "scratch" : onto.kind === "top" && PROJECT.has(onto.key) ? "project" : null;
  if (to != null && st) rehome(dragged.kind, dragged.id, to);
});
