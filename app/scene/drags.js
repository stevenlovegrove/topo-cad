// The drags kept beside a part script (docs/part-core.md, "Dragging"): each a sighting of what was dragged, on the
// cursor's ray as it was let go, at its level (dragged, unless it is set otherwise: below a param held and what was
// measured, above the hints). In the tree, under "Drags": each by what it is of, red where what it is of is off its
// ray (it gave way to something firmer). Chosen, its card sets its level and how near (±) it is weighed against others
// at its level; its menu (and the group's) lets it go. In the views: its ray, near what it is of, and a line from the
// ray to that where it is off.

import { esc } from "../ui.js";
import { vec } from "../glview.js";
import { model } from "../project.js";
import { msg } from "../shell/panel.js";
import { vis } from "../objects.js";
import { kind } from "./kinds.js";
import { sel, toScan } from "./state.js";
import { paramsNow } from "./exact.js";
import { choose } from "./chosen.js";
import { goTo } from "./links.js";
import { boxAbout } from "./geom.js";

/** How far each way along its ray a drag is drawn, from the point of it nearest what it is of (mm). */
const REACH = 250;
/** How far off its ray what a drag is of may be and be said to be on it (mm). */
const ON = 0.5;
/** The levels named, as the card offers them. */
const STOPS = [["80", "measured"], ["70", "held"], ["60", "dragged"], ["40", "hint"], ["20", "fallback"]];

const dragsNow = () => paramsNow()?.drags ?? [];
const dragOf = (id) => dragsNow().find((d) => String(d.index) === String(id)) ?? null;
const name = (d) => `${d.index + 1}: ${d.of}`;
/** A point (the root's frame, mm) in the frame the views draw in. */
const shown = (p) => toScan(p.map((x) => x / 1000));

/** Where what a drag is of is now (mm, the root's frame): a node where it is; a part, the middle of its nodes. */
function whereOf(d) {
  const nodes = paramsNow()?.nodes ?? [];
  if (d.what.node) return nodes.find((n) => n.address === d.what.node)?.at ?? null;
  const part = (d.what.point ?? d.what.face)?.part, of = nodes.filter((n) => n.address.startsWith(`${part}/`));
  return of.length ? vec.scale(of.reduce((a, n) => vec.add(a, n.at), [0, 0, 0]), 1 / of.length) : null;
}
/** The point of a drag's ray nearest `p` (mm). */
function nearest(d, p) {
  const along = vec.sub(d.to, d.from), l = Math.hypot(...along), u = vec.scale(along, 1 / l);
  return vec.add(d.from, vec.scale(u, Math.max(0, vec.dot(vec.sub(p, d.from), u))));
}
/** What of a drag's ray is drawn (mm): so far each way from the point of it nearest what it is of; and that point. */
function stretch(d) {
  const at = whereOf(d) ?? d.from, n = nearest(d, at), along = vec.sub(d.to, d.from), u = vec.scale(along, 1 / Math.hypot(...along));
  return { a: vec.sub(n, vec.scale(u, REACH)), b: vec.add(n, vec.scale(u, REACH)), n, at };
}

/** Keeps these drags beside the script, all of them; it is built again. */
async function keepDrags(drags) {
  const w = await model.drags(drags.map(({ what, from, to, tier }) => ({ what, from, to, tier: engineTier(tier) })));
  if (w.error) msg(w.error, true);
}
/** A tier as the engine has it, from one as it is said (`{ name, level, within }`). */
const engineTier = (t) => (t.name === "must" ? "must" : { at: { level: t.level, within: t.within ?? 1 } });
/** Each drag kept, with one of them changed (`change(drag)`: as it is to be, or null to let it go). */
async function change(id, to) {
  const all = dragsNow().flatMap((d) => (String(d.index) === String(id) ? [to(d)].filter(Boolean) : [d]));
  // (One let go: the rest are numbered again, so none of them is chosen.)
  if (all.length < dragsNow().length && sel.drag != null) choose(null);
  await keepDrags(all);
}
/** Whether a drag is drawn: its eye and its group's are on. */
const seen = (d) => vis("drags", `drag:${d.index}`);

kind("drag", {
  field: "drag", group: "drags", order: 15,
  key: (id) => `drag:${id}`,
  find: dragOf,
  title: (d) => `drag ${name(d)}`,
  rows(row, list) {
    const all = dragsNow();
    if (!all.length) return "";
    const off = all.filter((d) => d.off > ON).length;
    return list("drags", "Drags", () => all.map((d) => row(1, "drag", `drag:${d.index}`, d.of, {
      id: String(d.index), on: String(sel.drag) === String(d.index), dot: d.orphaned || d.off > ON ? "contradicted" : "supported",
      note: `${d.orphaned ? "not there" : d.off > ON ? `off ${d.off.toFixed(1)} mm` : d.tier.name}${d.yours ? " · yours" : ""}`,
      title: `${d.of}, kept ${d.tier.name}${d.tier.within != null ? ` ± ${d.tier.within} mm` : ""}${d.orphaned ? ". What it is of is not there" : d.off > ON ? `. It gave way to something firmer: ${d.off.toFixed(2)} mm off its ray` : ", on its ray"}. Click to choose it (its level is set on its card); right-click to let it go`,
    })), { note: off ? `${all.length}, ${off} off` : all.length, title: "Where things were dragged to: each kept beside the script as a sighting on the ray it was let go on" });
  },
  groupMenu: (add) => add("Let go of every drag", () => keepDrags([]), "danger"),
  points: (d) => { const s = stretch(d); return [shown(s.a), shown(s.b)]; },
  info: (d) => ({
    what: "Drag: where a thing was dragged to, kept as a sighting of it on the ray it was let go on", name: `Drag ${name(d)}`, id: String(d.index + 1),
    rows: [["Of", d.of], ["Level", `${d.tier.name}${d.tier.level != null ? ` (${d.tier.level})` : ""}`], ...(d.tier.within != null ? [["Within", `${d.tier.within} mm`]] : []), ["Off its ray", d.orphaned ? "what it is of is not there" : `${d.off.toFixed(2)} mm`]],
  }),
  goto(id) {
    const d = dragOf(id);
    if (!d) return;
    choose("drag", id);
    const s = stretch(d);
    goTo(boxAbout([shown(s.a), shown(s.b), shown(s.at)]), 0.6);
  },
  menu(id, m) {
    const d = dragOf(id);
    m.add("Go to it", () => this.goto(id));
    if (d?.yours && d.key) m.add("Keep it for everyone", async () => { const w = await model.share([d.key]); if (w.error) msg(w.error, true); });
    m.showHide();
    m.divider();
    m.add("Let go of it", () => change(id, () => null), "danger");
  },
  card(d) {
    const level = String(d.tier.level ?? "");
    const options = STOPS.concat(STOPS.some(([v]) => v === level) ? [] : [[level, d.tier.name]]).map(([v, n]) => `<option value="${v}"${v === level ? " selected" : ""}>${esc(n)}</option>`).join("");
    return {
      label: "Drag", id: String(d.index + 1),
      body: `of ${esc(d.of)} · <select data-level title="How firmly it is kept">${options}</select> ± <input type="number" step="any" min="0" data-within value="${d.tier.within ?? 1}" style="width:4em" title="How near its ray it is weighed against others at its level (mm)"> mm${d.orphaned ? " · <b>not there</b>" : d.off > ON ? ` · <b style="color:var(--bad)">off ${d.off.toFixed(1)} mm</b>` : " · on its ray"}`,
      bind(card) {
        const s = card.querySelector("select[data-level]"), w = card.querySelector("input[data-within]");
        s.onchange = () => change(d.index, (x) => ({ ...x, tier: { ...x.tier, name: "", level: Number(s.value) } }));
        w.onkeydown = (e) => { e.stopPropagation(); if (e.key === "Enter") w.blur(); };
        w.onchange = () => { const v = Number(w.value); if (v > 0) change(d.index, (x) => ({ ...x, tier: { ...x.tier, within: v } })); };
      },
    };
  },
  marks(out) {
    for (const d of dragsNow()) {
      if (!seen(d) || d.orphaned) continue;
      const on = String(sel.drag) === String(d.index), s = stretch(d), bad = d.off > ON;
      out.line(shown(s.a), shown(s.b), on ? [255, 230, 140] : bad ? [235, 90, 60] : [90, 190, 255]);
      // (Where what it is of is off its ray: from the ray to it.)
      if (bad) out.line(shown(s.n), shown(s.at), [235, 90, 60]);
    }
  },
  near(test) {
    for (const d of dragsNow()) if (seen(d) && !d.orphaned) { const s = stretch(d); test.line(d.index, shown(s.a), shown(s.b)); }
  },
});
