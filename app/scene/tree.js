import { beginDrag, bring, endDrag, openDoc } from "../shell/dock.js";
// The scene's rows in the tree of objects (objects.js): each kind's (kinds.js), with the model's
// parts as the tree's own (in their groups); and what a click, a second click, an eye and a
// right-click on a row do.

import { openMenu } from "../ui.js";
import * as objects from "../objects.js";
import { docMenu } from "../shell/browser.js";
import { st } from "./state.js";
import { everyKind, kindOf } from "./kinds.js";
import { objectMenu, pick } from "./chosen.js";
import { gotoObject } from "./links.js";
import { applyVisibility, theView, viewKey } from "./views.js";

export const renderTree = () => objects.render();

// A kind's group: its own row, and (unfolded) a row for each of them.
const list = (key, label, rows, opts) => objects.row(0, "top", key, label, { group: true, ...opts }) + (objects.open.has(key) ? rows().join("") : "");
/** A kind's rows in the tree: of those said at the start, and of one said later (a session's). */
export function addKindRows(k) {
  objects.section({ key: k.name, order: k.order, rows: () => (st ? k.rows(objects.row, list) : "") });
}

export function bindTree() {
  for (const k of everyKind()) if (k.rows) addKindRows(k);
  objects.model(kindOf("member").tree);
  // A click chooses, a second goes to it, a right-click asks what can be done with it.
  objects.on("select", pick);
  objects.on("goto", (kind, id) => (kind === "top" || kind === "model" ? id === "model" && bring(theView().key) : gotoObject(kind, id)));
  // With shift, or dragged onto a pane: a 3D view is turned to it (views.js: what a "thing" opens).
  // (A row that is of a document, a drawing or a file, is whoever lists it's to open.)
  const thing = (kind) => kind === "group" || !!kindOf(kind);
  // (The model's own row: twice, the 3D view of it is brought up, or one opened; with shift, another.)
  objects.on("open", (kind, id) => { if (kind === "model" || (kind === "top" && id === "model")) openDoc(viewKey(true), { beside: true }); else if (thing(kind)) openDoc(`thing:${kind}:${id}`, { beside: true }); });
  objects.on("drag", (kind, id, e) => { if (!kind) endDrag(); else if (thing(kind)) beginDrag(e, `thing:${kind}:${id}`); });
  objects.on("visibility", applyVisibility);
  // (A row of a document, a drawing or a file, has whoever lists it's menu.)
  objects.menu((kind, id, e) => openMenu(kind === "doc" ? docMenu(id) : objectMenu(kind, id), e.clientX, e.clientY));
}
