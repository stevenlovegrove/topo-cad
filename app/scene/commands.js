// What of the scene can be done by name (actions.js): what a shortcut's snippet calls as `app.…`.
// Each is what a click somewhere already does, so a key does no more and no less than the click.

import { command } from "../actions.js";
import * as objects from "../objects.js";
import { workspace } from "../workspace.js";
import { openDoc } from "../shell/dock.js";
import { choose, chosen, pick } from "./chosen.js";
import { gotoObject } from "./links.js";
import { theView } from "./views.js";

/** What is chosen, with the key its eye is kept under (a point picked: the part it is on). */
function selected() {
  const c = chosen();
  if (!c) return null;
  const key = c.kind === "group" ? `g:${c.id}` : c.kind === "point" ? (c.it.member ? `m:${c.it.member.path}` : null) : c.of.key?.(c.id) ?? null;
  return { kind: c.kind, id: c.kind === "point" ? null : c.id, key };
}
/** A key given, or that of what is chosen; of the model's parts only, where only those will do. */
const keyOr = (key, ofModel) => { const k = key ?? selected()?.key; return k && (!ofModel || /^[mg]:/.test(k)) ? k : null; };

command("selected", "app.selected(): what is chosen, { kind, id, key } (kind: member, group, slab, polyline, selection, plane, photo, …; id: a part's path, or the thing's id; key: what its eye in the tree is kept under: `m:` and a part's path, `g:` and a group's path below the model, `slab:b1`, …), or null", selected);
command("toggle", "app.toggle(key?): shows or hides what a key names (default: what is chosen); it stays chosen while hidden, so again shows it", (key) => { const k = keyOr(key); if (k) objects.toggle(k); });
command("only", "app.only(key?): shows only that part or group of the model (default: the one chosen) and hides every other part; again, of what is already the only thing shown: every part is shown", (key) => { const k = keyOr(key, true); if (k) objects.isolate(k); });
command("showAll", "app.showAll(): every part of the model is shown again", () => objects.showAll());
command("shown", "app.shown(key): whether what a key names is shown (its own eye: not whether a group it is in is)", (key) => objects.vis(key));
command("select", "app.select(kind, id): chooses a thing, as a click on its row in the tree does (\"member\" and a part's path, \"group\" and its path, \"slab\" and its id, …); app.select(null): lets go of what is chosen", (kind, id) => (kind ? pick(kind, id) : choose(null)));
command("goto", "app.goto(kind, id): the 3D view last used goes to a thing (Back in that view returns)", (kind, id) => gotoObject(kind, id));
command("view", "app.view(name): presses a button of the 3D view last used, by its name: ortho, top, fit, solid, exact, skeleton, deflected, balance, noroof, line, cut, goback, goforward, only, all (those that are there: some are only with something chosen, or in a slab: whole, square, flat, free, back, fwd)", (name) => theView().tools.querySelector(`button[data-a="${name}"]`)?.click());
command("glview", "app.glview(): the 3D view last used itself, for what has no button: its `state` ({ az, el, dist, target, ortho, solid, … }), `set({ … })` to change some of it at once, `animate({ … })` to move to it", () => theView());
command("open", "app.open(key): opens a document, or brings it to the front: \"sheet:B-101\" (a drawing by its number), \"file:PATH\", \"view:1\"", (key) => openDoc(key));
command("call", "await app.call(tool, args): runs one of the workspace's tools as the person (the tools an agent has: slabs, polylines, views, …): { text, images, error }", (tool, args) => workspace.call(tool, args));
