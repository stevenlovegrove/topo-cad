// The kinds of thing there are in a scene: a part of the model, a point picked, a slab, a
// polyline, a selection of points, a plane, an issue, … Each kind is said once, by the module
// that is of it (`kind(name, { … })`), and everything that shows or handles "a thing" (the tree
// of objects, the card of what is chosen, the menus, the marks and handles in the views, going
// to it) asks the kind. A kind says what applies to it, of:
//
//   in the tree     order, rows(row, list): its rows (its group's, and one for each of them; ""
//                   where the workspace has none of its kind); group: its group's key;
//                   groupMenu(add): what its group's row can do
//   one of them     field: where in `sel` the one chosen is (its id); find(id): it, or null;
//                   key(id): its row's key (what its eye is kept under); title(it): what it is
//                   called in a menu; choose(id): choosing it, where that is more than being
//                   the one chosen; goto(id); menu(id, m, view): what can be done with it
//                   (m: add, issue, name, showHide, head, divider); card(it): what the card
//                   says of it ({ label, id, name: { text, save }, note, body, bind(card) });
//                   where(it): what a new issue could be pinned to, given it is chosen
//                   ([{ key, label }]); refer(it): the id a reply would refer to it by
//   what it is      points(it): its extent, as points (in the frame the views draw in); frame(it):
//                   its own frame there ({ origin, axes: [x, y, z] }), where it has one; info(it):
//                   what Details says of it that is its kind's own ({ what, name, id, rows:
//                   [[label, value]], note }), under what is said of everything (object.js: its
//                   size, where it is, whose it is, who made it). Going to one that says no `goto`
//                   frames a view on its extent.
//   in the views    marks(out): its lines, patches and pins (out: line, box, fill, pin);
//                   near(test, view): which of them a click in a view is on (test: line, loop);
//                   handles(add): what of the one chosen can be dragged; drag(view, id, phase,
//                   ray, event), handleMenu(view, id, event): dragging one, right-clicking one

const kinds = new Map();
/** Says what a kind of thing is. */
export function kind(name, def) {
  kinds.set(name, { name, ...def });
}
export const kindOf = (name) => kinds.get(name);
export const everyKind = () => [...kinds.values()];
