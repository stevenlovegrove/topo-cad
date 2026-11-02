import { kind } from "./kinds.js";
import { gotoMembers } from "./links.js";
import { scanSeenIn, seenThrough } from "./points.js";
import { setInfo } from "../shell/dock.js";
// The model's parts in the views: the engine's (every part of the model as built), each with
// what a scan says of it where there is one; how they are coloured (the exact view draws them,
// exact.js); and what of them is under the cursor.

import { $, store, stored } from "../ui.js";
import { reached } from "../times.js";
import { kgSaid, tally, weightSaid } from "../weight.js";
import { vec } from "../glview.js";
import { model } from "../project.js";
import { bodyOf, chooseExact, colourExact, loadExact, partAt, showBodies } from "./exact.js";
import { workspace } from "../workspace.js";
import * as objects from "../objects.js";
import { HEAT_STOPS, describe, describeGroup, inspect, showTab } from "../shell/panel.js";
import { f3, pinnedTo, rot, scanned, sel, shortPath, st, toModel, toScan, unrot, views } from "./state.js";
import { choose, renderChips } from "./chosen.js";
import { prefs, setPrefs } from "../prefs.js";
import { rebuildMarks } from "./marks.js";
import { drawOverlay } from "./overlay.js";
import { diff, loadDiff, renderDiffBars } from "./proposals.js";
import { renderTree } from "./tree.js";

/** The model's parts (null: it does not build), and what they were asked for of. */
export let members = null;
let membersKey = "";
/** The model looked at (another is looked at whole), and it counted and weighed: said beside a view of all of it. */
let modelName = null;
export let counted = "";

/** The model's extent (model frame), from the boxes its parts are in. */
export function modelBounds(filter) {
  const lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity];
  for (const m of members || []) {
    if ((filter && !m.path.includes(filter)) || !m.shaped || !m.about?.bounds) continue;
    for (const p of m.about.bounds) for (let k = 0; k < 3; k++) { lo[k] = Math.min(lo[k], p[k]); hi[k] = Math.max(hi[k], p[k]); }
  }
  return lo[0] === Infinity ? null : { lo, hi, centre: lo.map((x, k) => 0.5 * (x + hi[k])), radius: 0.5 * Math.hypot(hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]) };
}

/**
 * The model's parts: the engine's (every part of it, as built), each with what the scan says of
 * it where there is a scan that was asked about it. Asked for again when the model is built
 * again, and when what the scan says changes (the model's place in it, which points are used).
 */
export async function loadMembers() {
  // (They are the engine's, and are asked of it as soon as it has built them: a workspace that has yet to say
  // anything, as one that a service keeps may for some seconds, holds up only what its scan says of them.)
  if (!model.state) return;
  const told = st && scanned() && st.scan?.loaded && st.placement;
  const key = `${model.state.version}|${told ? `${st.model.rev}|${JSON.stringify(st.placement)}|${st.scan.used}` : ""}`;
  if (key === membersKey) return;
  membersKey = key;
  const [mesh, ev] = await Promise.all([model.parts(), told ? workspace.evidence() : null]);
  if (key !== membersKey) return;
  if (mesh.error) { members = null; rebuildModel(); renderTree(); return; }
  members = mesh.members.map((m) => ({ ...m, ...ev?.parts?.[m.path] }));
  // (Another model than was looked at: it is looked at whole.)
  if (mesh.version_name !== modelName) for (const v of Object.values(views)) v.framed = false;
  modelName = mesh.version_name;
  const b = modelBounds();
  for (const v of Object.values(views)) if (b && !v.framed && !v.slab) { v.framed = true; v.frame(toScan(b.centre), b.radius, 0); }
  if (sel.proposal) { await loadDiff(); renderDiffBars(); }
  freshPlaces();
  // (A part script's bodies, drawn exactly where the page can: in place of the model's faces drawn here.)
  await loadExact();
  rebuildModel(); rebuildMarks();
  // (The model, counted and weighed: beside the view's name.)
  // (What it all weighs, as the part core weighs it: a part that says what it weighs, that.)
  const t = mesh.stats, w = t.weight, n = (k, one, many) => (k ? `${k} ${k === 1 ? one : many}` : "");
  counted = [n(t.members, "member", "members"), n(t.sheets, "sheet", "sheets"), w && kgSaid(w.kg, t.metric)].filter(Boolean).join(" · ");
  for (const v of Object.values(views)) if (!v.slab) setInfo(v.key, counted);
  // (The tree was drawn before they came, if the state came first.)
  renderTree();
  reached("parts");
}

/// An issue pinned to a member is where the member is now: its box was taken when it was raised,
/// and the member may have moved since (a proposal applied), which would leave the box behind.
export function freshPlaces() {
  for (const i of st?.issues || []) {
    const on = pinnedTo(i.place, "member"), m = on && members?.find((x) => x.path === on);
    if (!m) continue;
    const lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity];
    const [a, b] = m.about?.bounds ?? [];
    if (a && b) for (let c = 0; c < 8; c++) { const q = toScan([0, 1, 2].map((k) => ((c >> k) & 1 ? b : a)[k])); for (let k = 0; k < 3; k++) { lo[k] = Math.min(lo[k], q[k] - 0.02); hi[k] = Math.max(hi[k], q[k] + 0.02); } }
    if (lo[0] !== Infinity) { i.place.min = lo; i.place.max = hi; }
  }
}

const CHANGE = { added: [60, 200, 90], removed: [235, 70, 60], moved: [60, 200, 90], from: [235, 70, 60] };
// How far each member is used, by path, where the checks panel asks for parts to be coloured by that
// (`{ ratio, title, combination }`: the panel's "Colour by", and its combination).
export let ratios = {};
/** What the parts are coloured by (the model's card says it; the checks panel's "Colour by" is the same): "material",
 *  "scan" (how well the scan agrees with each), or how far the checks use each ("utilization", "stress", …). */
const colouredBy = () => $("heatMode").value;
const heated = () => !["material", "scan"].includes(colouredBy());
/** Colours the parts by one of those: kept as the viewer's choice. */
function colourBy(mode) {
  $("heatMode").value = mode;
  $("heatMode").dispatchEvent(new Event("change"));
}
// (As the viewer chose it last; and whatever chooses it, kept so.)
$("heatMode").value = prefs().modelColour;
if (!$("heatMode").value) $("heatMode").value = "scan";
$("heatMode").addEventListener("change", () => { setPrefs({ modelColour: colouredBy() }); renderChips(); });
export async function loadRatios() {
  ratios = heated() && model.built ? await model.ask("ratios", { mode: $("heatMode").value, combo: $("heatCombo").value }) : {};
  rebuildModel();
}

/** What is lit: what is chosen, the parts of what is chosen that is its group, of a group chosen, of a link's
 *  highlight, the open issue's part. */
const LIT = [255, 60, 220];
/** How each part is coloured, other than by what it is made of: `paint` a plain colour (sRGB bytes) or none, `value`
 *  its ratio where the parts are coloured by how far they are used (through the panel's heat, `HEAT_STOPS`); for the
 *  proposal's side shown (or as they are). Lit, or of a proposal's change: that colour; else its ratio, where the parts
 *  are coloured by it; else its own colour, where it has one (what the scan says of it). */
function drawList() {
  // (A part that is its group is lit with its shapes and its parts.)
  const its = itsGroup(sel.member);
  const hl = (m) => m.path === sel.member || under(m, its) || inGroup(m.path, sel.group) || (sel.highlight || []).some((h) => m.path.includes(h)) || (sel.issue != null && pinnedTo(st.issues.find((i) => i.id === sel.issue)?.place, "member") === m.path);
  // (Its own colour: what the scan says of it, where they are coloured by that. Else as its material is, or by how far
  // it is used, where they are coloured by that: a part not checked then is as its material is.)
  const own = (m) => (m.colour && colouredBy() === "scan" ? m.colour.map((c) => Math.round(c * 255)) : null);
  const plain = (m) => ({ m, paint: hl(m) ? LIT : own(m), value: heated() ? ratios[m.path]?.ratio ?? null : null });
  if (!diff || !sel.proposal) return (members || []).map(plain);
  // A proposal's two sides are each the whole model on that side, as the engine here builds it. Where a side does
  // not build here: the model as it is, its changes coloured.
  const whole = diff.sides, mode = sel.proposal.mode, ch = whole?.changes ?? diff.changes;
  // (Coloured by the proposal, not by the action: with it green, without it red. For an applied one shown as a
  // revert, the first side (now) has it, so the colours are the other way about.)
  const [first, second] = diff.offer?.reverting ? [CHANGE.added, CHANGE.from] : [CHANGE.from, CHANGE.added];
  const goes = (m) => ch[m.path] === "removed" || ch[m.path] === "moved", comes = (m) => ch[m.path] === "added" || ch[m.path] === "moved";
  const was = (m) => ({ m, paint: first, value: null }), will = (m) => ({ m, paint: second, value: null }), faint = (m) => ({ m, paint: [205, 208, 214], value: null });
  const before = whole?.before ?? members ?? [], after = whole?.after ?? [];
  if (mode === "before") return before.map((m) => (goes(m) ? was(m) : plain(m)));
  if (mode === "after") return after.map((m) => (comes(m) ? will(m) : plain(m)));
  return [...before.map((m) => (goes(m) ? was(m) : mode === "both" ? plain(m) : faint(m))), ...after.filter(comes).map((m) => ({ ...will(m), after: true }))];
}

/** The model's parts coloured in every view, as they now are to be (drawList), what is chosen with them. */
export function rebuildModel() {
  // (The chosen part, as the exact view draws it.)
  if (sel.bodyPart) chooseExact(sel.bodyPart.address, sel.bodyPart.what, sel.bodyPart.index);
  else chooseExact(bodyOf(sel.member));
  // (A proposal's side, where one is shown and it builds here: its bodies in place of the model's; changed and both,
  // the other side's that change drawn over the first.)
  const sides = sel.proposal && diff?.sides, mode = sel.proposal?.mode;
  if (sides) {
    const ch = sides.changes, root = (members ?? [])[0]?.path.split("/")[0] ?? "", path = (b) => (root ? `${root}/${b.address}` : b.address);
    const comes = (b) => ch[path(b)] === "added" || ch[path(b)] === "moved";
    showBodies(mode === "after" ? sides.bodies.after : mode === "before" ? sides.bodies.before : [...sides.bodies.before, ...sides.bodies.after.filter(comes).map((b) => ({ ...b, address: `after:${b.address}` }))]);
  } else showBodies(null);
  const paints = {}, values = {};
  for (const { m, paint, value, after } of drawList()) {
    const key = after ? `after:${m.path}` : m.path;
    if (paint) paints[key] = paint;
    else if (value != null) values[key] = value;
  }
  colourExact(paints, values, heated() ? { stops: HEAT_STOPS, range: [HEAT_STOPS[0][0], HEAT_STOPS[HEAT_STOPS.length - 1][0]] } : null);
  // (What the model takes up, in the scan's frame: each view's camera sees it whole, and in depth.)
  const b = modelBounds(), box = b && { lo: [Infinity, Infinity, Infinity], hi: [-Infinity, -Infinity, -Infinity] };
  if (b) for (let k = 0; k < 8; k++) { const q = toScan([0, 1, 2].map((a) => ((k >> a) & 1 ? b.hi[a] : b.lo[a]))); for (let a = 0; a < 3; a++) { box.lo[a] = Math.min(box.lo[a], q[a]); box.hi[a] = Math.max(box.hi[a], q[a]); } }
  for (const v of Object.values(views)) v.setExtent("model", box);
}

/**
 * What a ray (scan frame) meets in a view: the scan's point there (as the workspace has it: with the member
 * near it, the surface's normal, the selections it is in); where there is no scan point, the point of the
 * model the ray meets, in the same shape (on that member, with its face's normal). `hit`: null for neither.
 */
export async function pickIn(v, ray) {
  const cuts = v.cuts(), p = scanSeenIn(v) ? await workspace.pick(ray.origin, ray.dir, cuts.z, cuts.slab, v.pixelSize()) : { hit: null, selections: [] };
  const m = !p.hit && partAt(v, ray);
  // (A point of the scan, where the workspace met one first.)
  if (!m) return p.hit ? { ...p, scan: true } : p;
  const hit = vec.add(ray.origin, vec.scale(ray.dir, m.t));
  return { hit, model: toModel(hit), range: m.t, member: { path: m.path, distance: 0, offset: 0, normal: m.n }, normal: null, selections: [] };
}

/**
 * The point of the model a ray (scan frame) meets first, as a view draws it; where it meets none of it,
 * where it leaves the box the model is in; none, where it misses that too. What a view is moved about.
 */
export function pointAt(ray, v) {
  const m = partAt(v, ray);
  if (m) return vec.add(ray.origin, vec.scale(ray.dir, m.t));
  const b = modelBounds();
  if (!b) return null;
  const o = toModel(ray.origin), d = unrot(ray.dir);
  let near = -Infinity, far = Infinity;
  for (let k = 0; k < 3; k++) {
    if (Math.abs(d[k]) < 1e-12) { if (o[k] < b.lo[k] || o[k] > b.hi[k]) return null; continue; }
    const t0 = (b.lo[k] - o[k]) / d[k], t1 = (b.hi[k] - o[k]) / d[k];
    near = Math.max(near, Math.min(t0, t1)); far = Math.min(far, Math.max(t0, t1));
  }
  return far < Math.max(near, 1e-6) ? null : vec.add(ray.origin, vec.scale(ray.dir, far));
}

/** Whether a part (by its path) is of a group of the model's parts (by its path below the model). */
const inGroup = (path, group) => !!group && path.includes(`/${group}/`);
/** A part that is its group (by its path), where it is one: what is under its path is its shapes and its parts. */
const itsGroup = (path) => (path && members?.find((m) => m.path === path && m.about?.group) ? path : null);
/** Whether a part is of a part that is its group (by its path, or none): a shape of it, or a part of it. (What holds it on is not of it.) */
const under = (m, of) => !!of && m.path.startsWith(`${of}/`) && m.about?.kind !== "fixing";
/** A part with what is under it, where it is its group: itself first. Else itself alone. */
const withItsParts = (m) => (m.about?.group ? [m, ...(members || []).filter((x) => under(x, m.path))] : [m]);
/** The parts of a group. */
const partsOf = (group) => (members || []).filter((m) => inGroup(m.path, group));
/** What some parts weigh together (weight.js: `tally`), with where the middle of their weight is. */
export const weightOf = tally;
/** The parts that are chosen: the part, or the parts of the group. */
const chosenParts = () => (sel.group ? partsOf(sel.group) : (members || []).filter((m) => m.path === (sel.pick?.member?.path ?? sel.member)).flatMap(withItsParts));

// Whether the middle of the weight of what is chosen is marked in the views (kept).
let balance = stored("topo.view.balance", false);
export const marksBalance = () => balance;
export function markBalance(yes) { balance = yes; store("topo.view.balance", yes); rebuildMarks(); }

/** Chooses a group of the model's parts (by its path): its parts are lit in the views, and Details says what it is. */
function selectGroup(path) {
  choose("group", path);
  describeGroup(path, partsOf(path), weightOf(partsOf(path)));
  drawOverlay();
}

/** Chooses a part (by its path): it is shown in the tree, and the panel's Member tab is its. */
export function selectMember(path) {
  choose("member", path);
  // (Its row in the tree of objects: its groups folded open, and it in sight.)
  objects.reveal(path);
  showMember(path);
}

/** Fills the Member tab: what the scan shows of the member, then its structural checks. */
export async function showMember(path) {
  // (What holds a joint, or a part from a library, says what it is; a member, its checks.)
  const part = members?.find((x) => x.path === path);
  if (part?.about && part.about.kind !== "member") { describe(part, withItsParts(part)); return drawOverlay(); }
  await inspect(path, true);
  drawOverlay();
  const m = members?.find((x) => x.path === path), box = $("inspector");
  if (!m || !box || !m.evidence) return;
  const e = m.evidence, rays = e.rays_through + e.rays_ending;
  box.insertAdjacentHTML("afterbegin", `<div class="head evidence"><b>In the scan</b> <span class="ev ${e.state}">${e.state}</span>
    <table><tr><td>Scan points on its faces</td><td class="num">${e.points.toLocaleString()} <span class="muted">(${e.density.toLocaleString()} per m²)</span></td></tr>
    <tr><td>Their offset from the faces</td><td class="num">${e.offset_mm.toFixed(1)} mm mean, ${e.rms_mm.toFixed(1)} mm rms</td></tr>
    <tr><td>Sensor rays reaching it</td><td class="num">${rays.toLocaleString()}: ${e.rays_through.toLocaleString()} pass through (${rays ? Math.round((100 * e.rays_through) / rays) : 0}%)</td></tr></table>
    <div class="muted">${e.state === "contradicted" ? "Rays passing through a member say it is not there, or not exactly there." : e.state === "unseen" ? "Too few points and rays to say." : "Points on its faces, and few rays through it."} Evidence, not a verdict.</div></div>`);
}

/** What the scan says of a part (by its path), as a line of a card: nothing, where it says nothing. */
export function evidenceLine(path) {
  const e = members?.find((x) => x.path === path)?.evidence;
  return e?.state ? `<span class="ev ${e.state}">${e.state}</span> · ${e.points.toLocaleString()} points, ${e.rays_through} of ${e.rays_through + e.rays_ending} rays through` : "";
}

// What a part of the model is to the tree, the card and the menus (kinds.js). (The exact view draws the parts; how
// they are coloured: `rebuildModel`.)
const shaped = (parts) => parts.filter((m) => m.shaped).length;
const contradicted = (parts) => parts.filter((m) => m.evidence?.state === "contradicted").length;
kind("member", {
  field: "member", group: "model",
  key: (path) => `m:${path}`,
  // (By its path, whether or not the model as built has it just now.)
  find: (path) => members?.find((m) => m.path === path) ?? { path },
  title: (m) => shortPath(m.path),
  choose: selectMember,
  goto: gotoMembers,
  // Its rows in the tree are the model's own (objects.js: in their groups), with what the scan says of each as a dot.
  tree: {
    parts: () => members || [],
    // (A part that has no shape of its own is its group, not one more of what is in it.)
    counts: (m) => !!m.shaped,
    header: (parts) => ({ on: sel.model === "model", note: st?.tally ? `${shaped(parts)} · ${st.tally.contradicted} contradicted` : shaped(parts), title: st?.tally ? `${st.tally.supported} members supported by the scan, ${st.tally.contradicted} contradicted, ${st.tally.unseen} unseen` : "Model" }),
    group: (path, inside) => ({ dot: contradicted(inside) ? "contradicted" : "", title: `${path}: ${inside.length} parts${contradicted(inside) ? `, ${contradicted(inside)} contradicted by the scan` : ""}` }),
    decorate: (m) => ({ dot: m.evidence?.state, title: m.evidence ? `${m.path} — ${m.evidence.state}` : m.path }),
    selected: () => sel.pick?.member?.path ?? sel.member,
    selectedGroup: () => sel.group,
  },
  groupMenu: (add) => add("Show every part", () => objects.showAll()),
  where: (m) => [{ key: "member", label: `Member ${shortPath(m.path)}` }, ...(sel.at ? [{ key: "point", label: `Point ${f3(sel.at.model)}` }] : [])],
  menu(path, m) {
    m.issue("New issue on this member");
    m.add("Choose it", () => selectMember(path));
    m.add("Go to it", () => gotoMembers(path));
    m.add("Its checks", () => { selectMember(path); showTab("member"); });
    if (workspace.can("rays")) m.add("Show the points seen through it (what contradicts it)", () => seenThrough(path));
    m.showHide();
  },
  // The middle of the weight of what is chosen (a part, or a group's parts), where the views are asked to mark it:
  // three rings and a cross, faint, and seen through whatever is in front of it.
  marks(out) {
    const w = balance && weightOf(chosenParts());
    if (!w?.at) return;
    // (Dark and light together: it is seen on a part that is lit, on one that is not, and on the page behind them.)
    const c = toScan(w.at), r = Math.max(0.004, 0.05 * (modelBounds()?.radius ?? 0.2)), dark = [20, 20, 24, 200], light = [255, 255, 255, 210];
    const ax = [[1, 0, 0], [0, 1, 0], [0, 0, 1]].map(rot);
    for (let k = 0; k < 3; k++) {
      out.line(vec.sub(c, vec.scale(ax[k], 1.7 * r)), vec.add(c, vec.scale(ax[k], 1.7 * r)), dark);
      const [u, v] = [ax[(k + 1) % 3], ax[(k + 2) % 3]], n = 32;
      const on = (i, q) => vec.add(c, vec.add(vec.scale(u, q * Math.cos((2 * Math.PI * i) / n)), vec.scale(v, q * Math.sin((2 * Math.PI * i) / n))));
      for (let i = 0; i < n; i++) { out.line(on(i, r), on(i + 1, r), light); out.line(on(i, 1.08 * r), on(i + 1, 1.08 * r), dark); }
    }
  },
  card: (m) => ({
    label: "Member", id: shortPath(m.path),
    body: evidenceLine(m.path) + (sel.at ? `<br>the scan point there: ${f3(sel.at.model)}${sel.at.member?.path === m.path ? `, ${(sel.at.member.distance * 1000).toFixed(0)} mm from its face` : ""} <span title="Alt-click picks the point itself">(alt-click: the point)</span>` : ""),
  }),
});

// A group of the model's parts, chosen as a part is (from its row in the tree): its parts are lit, and what they
// weigh together is said. (Its menu is the tree's own: chosen.js.)
kind("group", {
  field: "group",
  find: (path) => ({ path }),
  title: (g) => g.path,
  // (An issue on a group is pinned to its parts together.)
  where: (g) => [{ key: "group", label: `Group ${g.path}` }],
  points: (g) => partsOf(g.path).flatMap((m) => (m.about?.bounds ?? []).map(toScan)),
  choose: selectGroup,
  card(g) {
    const w = weightOf(partsOf(g.path));
    return { label: "Group", id: g.path, body: `${w.of} parts${w.kg > 0 ? ` · ${weightSaid(w)}` : ""}` };
  },
});

// The model as a whole: chosen by its row in the tree, its card says what its parts are coloured by.
const COLOURINGS = [["material", "material"], ["scan", "agreement with the scan"], ["utilization", "utilization (all checks)"], ["stress", "stress"], ["bending", "bending"], ["shear", "shear"], ["compression", "compression"], ["bearing", "bearing"], ["deflection", "deflection"], ["connections", "connections"]];
kind("model", {
  field: "model",
  key: () => "model",
  find: (id) => (id === "model" && members ? { id, name: "Model" } : null),
  title: () => "the model",
  goto: () => { const b = modelBounds(), v = Object.values(views)[0]; if (b && v) v.frame(toScan(b.centre), b.radius); },
  menu(id, m) {
    m.showHide();
    m.add("Show every part", () => objects.showAll());
  },
  card() {
    const by = colouredBy(), checks = !["material", "scan"].includes(by);
    const combos = [...$("heatCombo").options].map((o) => [o.value, o.textContent]);
    // (What the colours say, of what they are coloured by.)
    const legend = by === "scan"
      ? `<span class="swatch" style="background:#33e640"></span>the scan supports it <span class="swatch" style="background:#e6a833"></span>some rays pass through it <span class="swatch" style="background:#e63333"></span>most do <span class="swatch" style="background:#80808c"></span>too little reaches it to say`
      : checks ? `<span class="heatbar"></span> 0 to ≥ 1.5 of what it may carry (the checks panel lists the highest)` : "each part as its material is";
    return {
      label: "Model", id: counted || "",
      body: `<span class="look"><label title="What the model's parts are coloured by (the checks panel's Colour by is the same)">Colour by <select data-by>${COLOURINGS.filter(([k]) => k !== "scan" || scanned()).map(([k, t]) => `<option value="${k}" ${k === by ? "selected" : ""}>${t}</option>`).join("")}</select></label>${checks ? ` <label title="The load combination the checks are of">for <select data-combo>${combos.map(([k, t]) => `<option value="${k}" ${k === $("heatCombo").value ? "selected" : ""}>${t}</option>`).join("")}</select></label>` : ""}</span><div class="legendline">${legend}</div>`,
      bind(card) {
        card.querySelector("select[data-by]").onchange = (e) => colourBy(e.target.value);
        const c = card.querySelector("select[data-combo]");
        if (c) c.onchange = (e) => { $("heatCombo").value = e.target.value; $("heatCombo").dispatchEvent(new Event("change")); };
        for (const s of card.querySelectorAll("select")) s.onkeydown = (e) => e.stopPropagation();
      },
    };
  },
});
