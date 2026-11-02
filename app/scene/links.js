import { kind, kindOf } from "./kinds.js";
import { extentOf } from "./object.js";
import { across, middle } from "./geom.js";
import { madeIn, rehome } from "./homes.js";
// View links, and going to things. A link is a partial view state (only what matters: a camera,
// a slab, a colouring, a proposal); following one changes just that in a view, and that view's
// Back undoes it. Views kept by name are links; so is what a comment carries. Going to a thing frames the
// focused view on it.

import { COLOURS, lookFrom, vec } from "../glview.js";
import { workspace } from "../workspace.js";
import * as dock from "../shell/dock.js";
import { active, rot, round, sel, setActive, shared, st, toModel, toScan, views } from "./state.js";
import { rebuildMarks } from "./marks.js";
import { modelBounds, rebuildModel } from "./parts.js";
import { selectProposal } from "./proposals.js";
import { cutHeights, slabFromSpec, slabName, slabOf, syncSlabs, tempSlab, viewSlab } from "./slabs.js";
import { renderLayers, theView } from "./views.js";

// Each view has where it has been (`v.history`, and `v.future` once gone back from): how it looked, and the slab
// it was clipped to, before each link followed or thing gone to in it. Its own Back and Forward go through them.
const how = (v) => ({ state: v.get(), slab: v.slab });
function take(v, was) {
  v.slab = was.slab;
  syncSlabs();
  v.animate(was.state);
  renderLayers(); rebuildMarks();
}
export const canGoBack = (v) => !!v.history?.length, canGoForward = (v) => !!v.future?.length;
export function goBack(v) {
  if (canGoBack(v)) { (v.future ??= []).push(how(v)); take(v, v.history.pop()); }
}
export function goForward(v) {
  if (canGoForward(v)) { v.history.push(how(v)); take(v, v.future.pop()); }
}
/** A view is about to be turned to something else: how it looks now is kept, to go back to. */
export function pushHistory(v) {
  (v.history ??= []).push(how(v));
  if (v.history.length > 50) v.history.shift();
  v.future = [];
}

/** Follows a view link: sets what it names, leaves the rest; the view's Back undoes it. In the view `into`, if one is given. */
export function followLink(link, into) {
  // A section is a view clipped to a slab: the one named, or one made (not kept) from what the link gives.
  const slab = slabOf(link.slab) ?? (link.section ? tempSlab(slabFromSpec(link.section)) : null);
  let v;
  if (slab) v = viewSlab(slab.id, into);
  else {
    // (In a view of the whole, if one is open: one clipped to a slab is left on its slab unless the link says not.)
    v = into ?? Object.values(views).find((x) => !x.slab) ?? theView();
    pushHistory(v);
    dock.bring(v.key);
    setActive(v.name);
    if (link.slab === null) { v.slab = null; syncSlabs(); }
  }
  const patch = {};
  if (link.flat != null) patch.flat = !!link.flat;
  if (link.free != null) patch.free = !!link.free;
  // (Seen parallel or in perspective, as the link says, else as the view is; parallel, `half` the height seen, as the
  // link says, else as much as the view now sees at what it looks at.)
  const cam = v.camera(), now = v.state.camera, parallel = link.projection ? link.projection.startsWith("ortho") : now.ortho != null;
  const halfAt = (target, eye) => link.camera?.half ?? cam?.hh ?? Math.hypot(...vec.sub(target, eye)) * Math.tan((cam?.fov ?? Math.PI / 4) / 2);
  if (link.camera) {
    // (A camera of its own: no longer held square on, unless the link says so.)
    if (slab && link.flat == null && (link.camera.eye || link.camera.top_down)) patch.flat = false;
    const c = link.camera;
    if (c.top_down) {
      // Straight down on what it looks at, seen parallel, the model's +y up.
      const t = c.target ? toScan(c.target) : v.lookingAt(), eye = vec.add(t, [0, 0, Math.max(2, Math.hypot(...vec.sub(t, now.at)))]);
      patch.camera = lookFrom(eye, t, { up: rot([0, 1, 0]), ortho: halfAt(t, eye) });
    } else if (c.eye && c.target) {
      const [e, t] = [toScan(c.eye), toScan(c.target)];
      patch.camera = lookFrom(e, t, { ortho: parallel ? halfAt(t, e) : null });
    } else if (c.member) {
      const b = modelBounds(c.member);
      if (b) v.frame(toScan(b.centre), Math.max(b.radius, 0.5));
    } else if (c.issue != null) {
      const i = st.issues.find((x) => x.id === c.issue);
      if (i?.place) v.frame(middle(i.place), Math.max(0.75, 0.5 * across(i.place)));
    } else if (c.target) {
      // (What it looks at, the way the view looks.)
      const t = toScan(c.target), depth = Math.hypot(...vec.sub(t, now.at));
      patch.camera = { ...now, at: vec.sub(t, vec.scale(now.forward, depth)) };
    }
  }
  // (Only parallel or in perspective said, or only how much is seen: the view as it is, so.)
  if (!patch.camera && (link.projection || link.camera?.half != null) && !link.camera?.member && link.camera?.issue == null) {
    const t = v.lookingAt();
    patch.camera = { ...now, ortho: parallel ? halfAt(t, now.at) : null };
  }
  if (link.colour && COLOURS.includes(link.colour)) patch.colour = link.colour;
  if (link.layers) patch.layers = link.layers;
  if (link.point_size) patch.pointSize = link.point_size;
  v.animate(patch);
  // (Heights it is cut to, where it is not a section: a plan slab, as the view's own roof and floor cuts are.)
  if (!slab && link.clip && (link.clip.z_min != null || link.clip.z_max != null)) cutHeights(v, { bottom: link.clip.z_min ?? null, top: link.clip.z_max ?? null });
  if (link.highlight) { sel.highlight = link.highlight; rebuildModel(); }
  if (link.proposal) {
    const i = st.issues.find((x) => x.id === link.proposal.issue);
    const p = i?.entries.find((e) => e.kind === "proposal" && e.n === link.proposal.n);
    if (p) selectProposal({ issue: i.id, n: p.n, commit: p.commit, mode: link.proposal.mode || "changed" });
  }
  renderLayers(); rebuildMarks();
}

/** The view last used as a link: what someone else needs to see the same. */
export function linkOfView(v = theView()) {
  const s = v.state, slab = slabOf(v.slab), cam = v.camera();
  const link = { pane: slab ? "section" : "3d", colour: s.colour, slab: slab && !slab.temp ? slab.id : null };
  if (slab) {
    // The slab as a section too, for whoever does not have it (an agent's `render_section` takes the same).
    link.section = { view: "custom", origin: round(slab.centre), normal: round(slab.axes[2]), up: round(slab.axes[1]), thickness: Math.round(2000 * slab.half[2]) / 1000, bounds: round([-slab.half[0], slab.half[0], -slab.half[1], slab.half[1]], 100) };
    if (slab.temp) delete link.slab;
    link.flat = s.flat;
    if (s.free) link.free = true;
  }
  // (Where the eye is and what it looks at, as the view's camera is now: seen parallel, how much it sees too.)
  link.camera = { eye: round(toModel(cam.eye), 100), target: round(toModel(v.lookingAt()), 100) };
  link.projection = cam.ortho ? "ortho" : "perspective";
  if (cam.ortho) link.camera.half = Math.round(cam.hh * 100) / 100;
  if (sel.proposal && !sel.proposal.versions) link.proposal = { issue: sel.proposal.issue, n: sel.proposal.n, mode: sel.proposal.mode };
  return link;
}
export function describeView(v = views[active] ?? Object.values(views)[0]) {
  if (!v) return "no 3D view is open";
  const s = v.state, slab = slabOf(v.slab);
  return `${slab ? `slab ${slabName(slab)}` : s.camera.ortho != null ? "3D ortho" : "3D"}, ${s.colour}`;
}
/** Frames a view (the one last used, or `v`) on a box of the scan's frame ({ min, max }), with room about it, and no nearer than `least` m across. Back returns. */
export function goTo(box, least = 0.8, room = 0.6, v = theView()) {
  pushHistory(v);
  dock.bring(v.key);
  v.frame(middle(box), Math.max(least, room * across(box)));
}

/** Goes to what a row of the tree or a link names: a thing of a kind (by its id), or a group of the model's parts (by its path). */
export function gotoObject(kind, id) {
  const k = kindOf(kind);
  if (kind === "group") gotoMembers(`/${id}/`);
  else if (k?.goto) k.goto(id);
  // (A kind that says nothing of going to its things: a view is framed on the thing's extent.)
  else { const e = extentOf(kind, k?.find(id)); if (e) goTo(e.box, 0.6); }
}

/** Goes to the parts of the model whose paths have `filter` in them (one part: its path), in the view last used (or `v`). */
export function gotoMembers(filter, v) {
  const b = modelBounds(filter);
  // (No nearer than 0.6 m across, of a building; of a model that is itself small, a quarter of its own size.)
  const floor = Math.min(0.6, 0.25 * (modelBounds()?.radius ?? 0.6));
  if (b) goTo({ min: toScan(b.centre), max: toScan(b.centre) }, Math.max(b.radius, floor), 0.6, v ?? theView());
}
/** Goes to an issue's place, in the view last used (or `v`). */
export function gotoIssue(id, v) {
  const i = st.issues.find((x) => x.id === id);
  if (i?.place) goTo(i.place, 1.2, 0.8, v);
}

/** Keeps a view (as it looks now) with the project, under a name asked for: to open again, and for others to. */
export async function keepView(v) {
  const name = prompt("Name for this view:", describeView(v));
  if (!name) return;
  const r = await workspace.call("views", { action: "save", name, ...madeIn(), state: linkOfView(v) });
  if (r.error) alert(r.error);
}

// The views kept, in the tree: each by its name, among the other things of the scene (an issue's, with its issue:
// homes.js). A click opens one in a 3D view (whose Back returns); shift, in a view of its own.
kind("saved", {
  group: "views", order: 65,
  key: (name) => `saved:${name}`,
  find: (name) => st?.views.find((v) => v.name === name) ?? null,
  all: () => st?.views ?? [],
  rowOf: (row, v, depth, opts) => row(depth, "saved", `saved:${v.name}`, v.name, { ...opts, id: v.name, noEye: true, title: `${v.name}: open this view (its Back returns)` }),
  rows(row, list) {
    const mine = st.views.filter((v) => v.issue == null);
    return list("views", "Views", () => mine.map((v) => this.rowOf(row, v, 1)), { noEye: true, note: mine.length || "kept from a view's ⋯", title: "Views kept by name: how a 3D view looked, to open again (a view's ⋯ menu keeps one)" });
  },
  choose(name) { const v = this.find(name); if (v) followLink(v.state); },
  // (Turned to in the view last used: what a view it was dragged onto, or opened for, is.)
  goto(name) { const v = this.find(name); if (v) followLink(v.state, views[active]); },
  menu(name, m) {
    const v = this.find(name);
    m.add("Open it", () => this.choose(name));
    if (v.issue != null) m.add("Keep it with the project", () => rehome("saved", name, "project"));
    else if (shared() && sel.issue != null) m.add(`Make it issue #${sel.issue}'s`, () => rehome("saved", name, sel.issue));
    m.divider();
    m.add("Delete it", () => { if (confirm(`Delete the view “${name}”?`)) workspace.call("views", { action: "delete", name }); }, "danger");
  },
});
