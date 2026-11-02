// What is said and done of anything in the scene alike, whatever kind of thing it is: where it is and
// how big (its extent, in its own frame where it has one), whose it is, going to it, and telling of
// it in Details. A kind says the little that is its own (kinds.js: `points`, `frame`, `info`); what
// follows from that is here, once.

import { vec } from "../glview.js";
import { model } from "../project.js";
import { describeThing } from "../shell/panel.js";
import { kindOf } from "./kinds.js";
import { boxAbout } from "./geom.js";
import { homeOf } from "./homes.js";
import { rot, toModel } from "./state.js";

/**
 * Where a thing is and how big: `points` (its extent, in the frame the views draw in), `axes` (its own, or
 * the model's), `own` (whether they are its own), `size` (along each of them), `middle` (of the box it is in
 * along them, in the model's frame), `box` ({ min, max } about its points, for going to it). Null: it has no
 * extent (a kept view, an issue that is not pinned).
 */
export function extentOf(kindName, it) {
  const k = kindOf(kindName), points = it && k?.points?.(it);
  if (!points?.length) return null;
  const frame = k.frame?.(it), axes = frame?.axes ?? [[1, 0, 0], [0, 1, 0], [0, 0, 1]].map(rot);
  const lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity];
  for (const p of points) for (let a = 0; a < 3; a++) { const x = vec.dot(p, axes[a]); lo[a] = Math.min(lo[a], x); hi[a] = Math.max(hi[a], x); }
  const mid = [0, 1, 2].reduce((p, a) => vec.add(p, vec.scale(axes[a], 0.5 * (lo[a] + hi[a]))), [0, 0, 0]);
  return { points, axes, own: !!frame, size: [0, 1, 2].map((a) => hi[a] - lo[a]), middle: toModel(mid), box: boxAbout(points) };
}

/** Whose a thing is, in words: the project's, an issue's, or nobody's yet. (Of a kind whose things have homes.) */
function belongs(k, it) {
  if (!k.all) return null;
  const home = homeOf(it);
  return home === "project" ? "the project" : home === "scratch" ? "nothing yet (it is under Unsorted)" : `issue #${home}`;
}

/** What Details was last told of by `describeObject`: [its kind, its id]. */
let shown = null;
/** Whether Details tells of this thing (as it was told when it was chosen). */
export const describes = (kindName, id) => !!shown && shown[0] === kindName && shown[1] === id;

/** Tells of a thing in Details: what is said of everything (what it is, its size, where, whose, who made it), then its kind's own. */
export async function describeObject(kindName, it, id) {
  shown = [kindName, id];
  const k = kindOf(kindName), said = await k?.info?.(it);
  if (!said) return;
  const e = extentOf(kindName, it), metric = (await model.parts()).stats?.metric;
  // (A length: millimetres of what is small, metres of what is not; inches and feet where the model is not metric.)
  const big = e && Math.max(...e.size) >= 2;
  const len = (x) => (metric === false ? (big ? `${(x / 0.3048).toFixed(2)}` : `${(x / 0.0254).toFixed(2)}`) : big ? x.toFixed(3) : (x * 1000).toFixed(1));
  const unit = metric === false ? (big ? " ft" : " in") : big ? " m" : " mm";
  const rows = [
    ["What", said.what],
    ...(e ? [["Size", `${e.size.map(len).join(" × ")}${unit}${e.own ? " (along its own axes)" : ""}`], ["Middle", `${e.middle.map(len).join(", ")}${unit}`]] : []),
    ...(belongs(k, it) ? [["Belongs to", belongs(k, it)]] : []),
    ...(it.by ? [["Made by", it.by]] : []),
  ];
  describeThing({ name: said.name, id: said.id ?? "", rows, more: said.rows ?? [], note: said.note ?? "", actions: said.actions, mount: said.mount });
}
