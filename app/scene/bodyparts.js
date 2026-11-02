// What of a part script's bodies a click in a view chooses, as the view's selection says (`v.pickKind`): a part
// (as ever), or a face, an edge or a vertex of a body, exactly (the exact view finds it: `findExact`); or a node,
// one of the points the part's solves put (they need not be on any body: the exact view draws them, where its
// display says). One of them chosen is told of in Details, and drawn chosen in every view: a node with how it can move
// (held, along a line, in a plane, any way) and the params that would move it.

import { kind } from "./kinds.js";
import { model } from "../project.js";
import { msg } from "../shell/panel.js";
import { paramsNow, paramsOfNode } from "./exact.js";
import { describeObject } from "./object.js";
import { sel, toScan } from "./state.js";

/** The kinds of thing a click chooses, in the order the view's controls have them. */
export const PICK_KINDS = ["part", "face", "edge", "vertex", "node"];

const mm = (x) => `${(+x).toFixed(2)} mm`;
const at = (p) => p.map((x) => (+x).toFixed(2)).join(", ");
/** What it is, said short. */
const title = (p) => (p.what === "node" ? `node ${p.name}` : `${p.what} ${p.name || `#${p.index}`} of ${p.address}`);

// How each node can move, as built, by its name: asked of the engine once (of a big part it takes a while), the
// latest asked for next where one is being found; Details tells of a node at once, and of how it can move once that is
// known, if it is still the one chosen.
const ways = { built: null, known: new Map(), asking: null, next: null };
/** How the node `name` can move (`model.ways`), if that is known; else nothing, and it is asked. */
function waysOf(name) {
  if (ways.built !== model.state) Object.assign(ways, { built: model.state, known: new Map(), next: null });
  if (ways.known.has(name)) return ways.known.get(name);
  if (ways.asking !== name) ways.next = name;
  ask();
  return undefined;
}
async function ask() {
  if (ways.asking || !ways.next) return;
  const name = (ways.asking = ways.next), built = ways.built;
  ways.next = null;
  const w = await model.ways(name);
  ways.asking = null;
  if (built === ways.built) {
    ways.known.set(name, w);
    if (sel.bodyPart?.what === "node" && sel.bodyPart.name === name) describeObject("bodypart", sel.bodyPart);
  }
  ask();
}

/** What can be let go of that is kept of a node (`p`): its params kept, and its drags; with what Details says of them
 *  (`rows`, where it is given). */
function keptOf(p, rows) {
  const actions = [];
  // Its coordinates that are params: each as found, what decides it, and what is kept of it, which can be let go; and
  // the drags kept of it.
  const own = paramsOfNode(p.name), kept = own.filter((q) => q.kept != null);
  for (const q of own) rows?.push([q.address.slice(p.name.length + 1), `${(+q.value).toFixed(2)} (${q.by}${q.kept != null ? `, kept ${q.kept.tier.name} at ${(+q.kept.value).toFixed(2)}` : ""})`]);
  const letGo = (qs) => async () => {
    const w = await model.params(Object.fromEntries(qs.map((q) => [q.address, null])));
    if (w.error) msg(w.error, true);
  };
  for (const q of kept) actions.push({ label: `Let go of ${q.address.slice(p.name.length + 1)}`, title: `What is kept of ${q.address} let go: it is as the script says again`, run: letGo([q]) });
  if (kept.length > 1) actions.push({ label: "Let go of all kept of it", run: letGo(kept) });
  const all = paramsNow()?.drags ?? [], drags = all.filter((d) => d.what.node === p.name);
  if (drags.length) {
    rows?.push(["Dragged", drags.map((d) => `${d.tier.name}${d.off > 0.5 ? `, off ${d.off.toFixed(1)} mm` : ""}`).join("; ")]);
    actions.push({ label: drags.length > 1 ? "Let go of its drags" : "Let go of its drag", run: async () => {
      const w = await model.drags(all.filter((d) => d.what.node !== p.name).map(({ what, from, to, tier }) => ({ what, from, to, tier: tier.name === "must" ? "must" : { at: { level: tier.level, within: tier.within ?? 1 } } })));
      if (w.error) msg(w.error, true);
    } });
  }
  return actions;
}

kind("bodypart", {
  field: "bodyPart",
  find: (p) => p,
  title,
  card: (p) => ({ label: p.what[0].toUpperCase() + p.what.slice(1), id: p.name || `#${p.index ?? ""}`, body: p.what === "node" ? `at ${at(p.at)} mm` : `of ${p.address}, at ${at(p.at)} mm` }),
  async info(p) {
    const rows = [];
    if (p.address) rows.push(["Body", p.address]);
    let actions = [];
    if (p.what === "node") {
      rows.push(["At", `${at(p.at)} mm`]);
      actions = keptOf(p, rows);
      // How it can move with all that must hold held: held, along a line, in a plane, any way; and by what.
      const w = waysOf(p.name);
      if (!w) rows.push(["Moves", "finding how…"]);
      else if (!w.error) {
        const way = (d) => d.map((x) => (Math.abs(x) < 5e-5 ? 0 : x).toFixed(3)).join(", ");
        const normal = w.rank === 2 ? [w.dirs[0][1] * w.dirs[1][2] - w.dirs[0][2] * w.dirs[1][1], w.dirs[0][2] * w.dirs[1][0] - w.dirs[0][0] * w.dirs[1][2], w.dirs[0][0] * w.dirs[1][1] - w.dirs[0][1] * w.dirs[1][0]] : null;
        // (A way, either way along it: said with its first that is not nothing forward.)
        const forward = (d) => (d.find((x) => Math.abs(x) >= 5e-5) < 0 ? d.map((x) => -x) : d);
        // (Only the one it is said: held, it has no way to say.)
        rows.push(["Moves", [() => "held: what must hold leaves it nowhere to go", () => `along a line (${way(forward(w.dirs[0]))})`, () => `in a plane (square to ${way(forward(normal))})`, () => "any way"][w.rank]()]);
        if (w.params.length) rows.push(["Moved by", w.params.slice(0, 4).map(([a]) => a).join(", ") + (w.params.length > 4 ? `, and ${w.params.length - 4} more` : "")]);
      }
    } else {
      rows.push([p.what === "vertex" ? "At" : "Picked at", `${at(p.at)} mm`]);
      if (p.plane) rows.push(["Surface", `plane, facing ${at(p.plane.normal)}`]);
      if (p.cylinder) rows.push(["Surface", `cylinder, radius ${mm(p.cylinder.radius)}, axis ${at(p.cylinder.axis)}`]);
      if (p.curve === "line") rows.push(["Curve", "line"]);
      if (p.curve?.arc) rows.push(["Curve", `arc, radius ${p.curve.arc.radii[0] === p.curve.arc.radii[1] ? mm(p.curve.arc.radii[0]) : p.curve.arc.radii.map(mm).join(" by ")}, ${p.curve.arc.turn.toFixed(1)}°, about ${at(p.curve.arc.centre)}`]);
      if (p.ends) rows.push(["From", `${at(p.ends[0])} mm`], ["To", `${at(p.ends[1])} mm`]);
      if (p.length != null) rows.push(["Length", mm(p.length)]);
    }
    return { what: p.what, name: title(p), id: p.name ?? "", rows, actions };
  },
  // Its menu: the view orbits about it; and, of a node, what is kept of it let go of.
  menu(p, m, v) {
    if (v) m.add("Orbit about it", () => v.centreOn(toScan(p.at.map((x) => x / 1000))));
    if (p.what === "node") for (const a of keptOf(p)) m.add(a.label, a.run);
  },
});
