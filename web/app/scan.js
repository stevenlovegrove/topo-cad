// The scan workspace in the app (only when the site has the service: see
// service.js). It adds:
//   - panes: "3D with scan", "Section" (both glview.js) and "Board";
//   - in the browser: layers (how the focused view shows the scan) and views;
//   - on the right: the issues and their threads, with proposals;
//   - in the header: who is here, and Back / Forward through views followed.
//
// Nobody's view is moved for them. A comment can carry a view link (a
// partial view state: only what matters); following it changes just that
// and can be undone with Back.

import { service } from "./service.js";
import { GLView, vec, COLOURS } from "./glview.js";

let app;                       // hooks into the page (index.html)
let st = null;                 // the workspace's state
let pl = { yaw: 0, t: [0, 0, 0] };
let members = null, membersKey = "";
let diff = null;               // a selected proposal's before / after / changes
const views = {};              // scan3d, section: GLView
let active = "scan3d";         // the scan view last used (what Layers controls)
const sel = { issue: null, proposal: null, pick: null, filter: "all", done: false, drafting: false };
const history = [], future = [];
let cloudLoaded = false, slabKey = "", slabTimer = null;

const $ = (id) => document.getElementById(id);
const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
const el = (tag, attrs = {}, html) => {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) if (k === "class") e.className = v; else e.setAttribute(k, v);
  if (html != null) e.innerHTML = html;
  return e;
};

// ----- frames: model ↔ scan -------------------------------------------------------
const toScan = (p) => { const c = Math.cos(pl.yaw), s = Math.sin(pl.yaw); return [c * p[0] - s * p[1] + pl.t[0], s * p[0] + c * p[1] + pl.t[1], p[2] + pl.t[2]]; };
const toModel = (p) => { const q = vec.sub(p, pl.t), c = Math.cos(pl.yaw), s = Math.sin(pl.yaw); return [c * q[0] + s * q[1], -s * q[0] + c * q[1], q[2]]; };
const rot = (n) => { const c = Math.cos(pl.yaw), s = Math.sin(pl.yaw); return [c * n[0] - s * n[1], s * n[0] + c * n[1], n[2]]; };
const unrot = (n) => { const c = Math.cos(pl.yaw), s = Math.sin(pl.yaw); return [c * n[0] + s * n[1], -s * n[0] + c * n[1], n[2]]; };
const f3 = (p) => `[${p.map((x) => x.toFixed(2)).join(", ")}]`;
const round = (p, k = 1000) => p.map((x) => Math.round(x * k) / k);

// ----- participants ---------------------------------------------------------------
function badge(id, small) {
  const p = service.participant(id);
  const initials = p.kind === "agent" ? (p.name.match(/[A-Za-z]/)?.[0] ?? "A").toUpperCase() + (p.name.match(/\d+/)?.[0] ?? "") : p.name.slice(0, 1).toUpperCase();
  return `<span class="who${small ? " small" : ""}${p.kind === "agent" ? " agent" : ""}" style="--who:${esc(p.colour)}" title="${esc(p.name)}${p.kind === "agent" ? " (agent)" : ""}">${esc(initials)}</span>`;
}
const PRESENCE = { here: "here", working: "working", waiting: "listening", away: "away" };
function renderPeople() {
  const box = $("people");
  box.innerHTML = st.participants.map((p) => `<span class="person ${p.presence}${p.id === service.id ? " me" : ""}" data-id="${esc(p.id)}" title="${esc(p.name)} — ${p.kind === "agent" ? "agent, " : ""}${PRESENCE[p.presence]}${p.id === service.id ? " (you: click to change your name)" : ""}">${badge(p.id)}<i></i></span>`).join("");
  const me = box.querySelector(".me");
  if (me) me.onclick = async () => {
    const name = prompt("Your name, as others see it:", service.me()?.name ?? "");
    if (name && name.trim()) await service.call("rename", { name: name.trim() });
  };
}

// ----- view state, links and history -----------------------------------------------

/** A section in the scan's frame from a link's (as `render_section` takes it, model frame). */
function sectionFrom(spec) {
  const thick = spec.thickness;
  let origin, n, up, t;
  if (spec.view === "x") [origin, n, up, t] = [[spec.at ?? 0, 0, 0], [1, 0, 0], [0, 0, 1], thick ?? 0.3];
  else if (spec.view === "y") [origin, n, up, t] = [[0, spec.at ?? 0, 0], [0, -1, 0], [0, 0, 1], thick ?? 0.3];
  else if (spec.view === "custom") [origin, n, up, t] = [spec.origin, spec.normal, spec.up ?? [0, 0, 1], thick ?? 0.3];
  else { const z = spec.z ?? [0.1, 2.4]; [origin, n, up, t] = [[0, 0, 0.5 * (z[0] + z[1])], [0, 0, 1], [0, 1, 0], Math.abs(z[1] - z[0])]; }
  n = vec.unit(n);
  let v = vec.sub(up, vec.scale(n, vec.dot(up, n)));
  if (Math.hypot(...v) < 1e-6) v = Math.abs(n[0]) < 0.9 ? vec.cross(n, [1, 0, 0]) : vec.cross(n, [0, 1, 0]);
  v = vec.unit(v);
  const u = vec.cross(v, n);
  return { origin: toScan(origin), n: rot(n), u: rot(u), v: rot(v), half: 0.5 * t };
}

/** The model's extent (model frame), from its members. */
function modelBounds(filter) {
  const lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity];
  for (const m of members || []) {
    if (filter && !m.path.includes(filter)) continue;
    for (const f of m.faces) for (const p of f) for (let k = 0; k < 3; k++) { lo[k] = Math.min(lo[k], p[k]); hi[k] = Math.max(hi[k], p[k]); }
  }
  return lo[0] === Infinity ? null : { lo, hi, centre: lo.map((x, k) => 0.5 * (x + hi[k])), radius: 0.5 * Math.hypot(hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]) };
}

/** Puts a section in the section view, framed on the model (or on `bounds` [u0, u1, v0, v1]). */
function setSection(sec, bounds) {
  const v = views.section;
  let centre = [0, 0], zoom = 4;
  if (bounds) { centre = [0.5 * (bounds[0] + bounds[1]), 0.5 * (bounds[2] + bounds[3])]; zoom = 0.55 * Math.max(bounds[3] - bounds[2], (bounds[1] - bounds[0]) * 0.7); }
  else {
    const b = modelBounds();
    if (b) {
      let [u0, u1, v0, v1] = [Infinity, -Infinity, Infinity, -Infinity];
      for (const x of [b.lo[0], b.hi[0]]) for (const y of [b.lo[1], b.hi[1]]) for (const z of [b.lo[2], b.hi[2]]) {
        const q = vec.sub(toScan([x, y, z]), sec.origin);
        u0 = Math.min(u0, vec.dot(q, sec.u)); u1 = Math.max(u1, vec.dot(q, sec.u)); v0 = Math.min(v0, vec.dot(q, sec.v)); v1 = Math.max(v1, vec.dot(q, sec.v));
      }
      centre = [0.5 * (u0 + u1), 0.5 * (v0 + v1)];
      const aspect = (v.canvas.clientWidth || 4) / (v.canvas.clientHeight || 3);
      zoom = 0.55 * Math.max(v1 - v0, (u1 - u0) / aspect);
    }
  }
  v.set({ section: sec, centre, zoom });
}

function snapshot() {
  return { slots: app.slots(), scan3d: views.scan3d.get(), section: views.section.get(), proposal: sel.proposal && { ...sel.proposal } };
}
function restore(s) {
  views.scan3d.set(s.scan3d, true);
  views.section.set(s.section, true);
  app.setSlots(s.slots);
  selectProposal(s.proposal, true);
  renderLayers(); requestSlab(); rebuildMarks();
}
function renderHistory() {
  $("histBack").disabled = !history.length;
  $("histFwd").disabled = !future.length;
  $("histBack").title = history.length ? `Back to the view before the last link (${history.length})` : "Back: nothing followed yet";
}
function pushHistory() {
  history.push(snapshot());
  if (history.length > 50) history.shift();
  future.length = 0;
  renderHistory();
}

/** Follows a view link: sets what it names, leaves the rest; Back undoes it. */
function followLink(link) {
  pushHistory();
  const kind = link.pane === "section" || (link.section && link.pane !== "3d") ? "section" : "scan3d";
  const v = views[kind];
  // Beside what is open, if there is one pane; else where it is, or in the focused pane.
  if (!app.shown(kind) && app.slots().length < 2) app.openBeside(kind);
  else app.showView(kind);
  active = kind;
  const patch = {};
  if (link.section) setSection(sectionFrom(link.section), link.section.bounds);
  if (link.camera) {
    const c = link.camera;
    if (c.eye && c.target) {
      const [e, t] = [toScan(c.eye), toScan(c.target)], d = vec.sub(e, t), dist = Math.hypot(...d) || 1;
      Object.assign(patch, { target: t, dist, az: (Math.atan2(d[0], d[1]) * 180) / Math.PI, el: (Math.asin(d[2] / dist) * 180) / Math.PI });
    } else if (c.member) {
      const b = modelBounds(c.member);
      if (b) v.frame(toScan(b.centre), Math.max(b.radius, 0.5));
    } else if (c.issue != null) {
      const i = st.issues.find((x) => x.id === c.issue);
      if (i?.place) v.frame(i.place.min.map((x, k) => 0.5 * (x + i.place.max[k])), Math.max(0.75, 0.5 * Math.hypot(...vec.sub(i.place.max, i.place.min))));
    } else if (c.target) Object.assign(patch, { target: toScan(c.target) });
    if (c.half != null) patch.half = c.half;
    if (c.top_down) Object.assign(patch, { az: (-pl.yaw * 180) / Math.PI + 180, el: 89.9, ortho: true });
  }
  if (link.projection) patch.ortho = link.projection.startsWith("ortho");
  if (link.colour && COLOURS.includes(link.colour)) patch.colour = link.colour;
  if (link.clip) { if ("z_min" in link.clip) patch.zMin = link.clip.z_min; if ("z_max" in link.clip) patch.zMax = link.clip.z_max; }
  if (link.layers) patch.layers = link.layers;
  if (link.point_size) patch.pointSize = link.point_size;
  v.set(patch);
  if (link.highlight) { sel.highlight = link.highlight; rebuildModel(); }
  if (link.proposal) {
    const i = st.issues.find((x) => x.id === link.proposal.issue);
    const p = i?.entries.find((e) => e.kind === "proposal" && e.n === link.proposal.n);
    if (p) selectProposal({ issue: i.id, n: p.n, commit: p.commit, mode: link.proposal.mode || "changed" }, true);
  }
  renderLayers(); requestSlab(); rebuildMarks();
}

/** The focused scan view as a link: what someone else needs to see the same. */
function linkOfView() {
  const v = views[active], s = v.state;
  const link = { pane: s.kind === "section" ? "section" : "3d", colour: s.colour, clip: { z_min: s.zMin, z_max: s.zMax } };
  if (s.kind === "section" && s.section) {
    const sec = s.section, hw = s.zoom * ((v.canvas.clientWidth || 4) / (v.canvas.clientHeight || 3));
    link.section = { view: "custom", origin: round(toModel(sec.origin)), normal: round(unrot(sec.n)), up: round(unrot(sec.v)), thickness: Math.round(2000 * sec.half) / 1000, bounds: round([s.centre[0] - hw, s.centre[0] + hw, s.centre[1] - s.zoom, s.centre[1] + s.zoom], 100) };
  } else {
    const cam = v.camera();
    link.camera = { eye: round(toModel(s.ortho ? vec.add(s.target, vec.scale(cam.forward, -s.dist)) : cam.eye), 100), target: round(toModel(s.target), 100) };
    link.projection = s.ortho ? "ortho" : "perspective";
    if (s.ortho) link.camera.half = Math.round(s.half * 100) / 100;
  }
  if (sel.proposal) link.proposal = { issue: sel.proposal.issue, n: sel.proposal.n, mode: sel.proposal.mode };
  return link;
}
function describeView() {
  const s = views[active].state;
  return `${s.kind === "section" ? "section" : s.ortho ? "3D ortho" : "3D"}, ${s.colour}`;
}

// ----- data: the cloud, the members, a section's slab --------------------------------

async function loadCloud() {
  cloudLoaded = true;
  const c = await service.cloud();
  if (!c) { cloudLoaded = false; return; }
  for (const v of Object.values(views)) v.setCloud("thin", c);
}

function requestSlab() {
  const s = views.section.state.section;
  const key = s ? JSON.stringify([s.origin, s.n, s.half]) : "";
  if (key === slabKey || !st?.scan.loaded) return;
  clearTimeout(slabTimer);
  slabTimer = setTimeout(async () => {
    slabKey = key;
    if (!s) return views.section.setCloud("slab", null);
    const c = await service.slab(s.origin, s.n, s.half);
    if (slabKey === key) views.section.setCloud("slab", c);
    setInfo("section", c ? `${c.count.toLocaleString()} points in the slab` : "");
  }, 200);
}

async function loadMembers() {
  const key = `${st.model.rev}|${JSON.stringify(st.placement)}`;
  if (key === membersKey || !st.scan.loaded || !st.placement) return;
  membersKey = key;
  const r = await service.members();
  if (r.error) { membersKey = ""; return; }
  members = r.members;
  const first = !views.scan3d.framed;
  if (first) {
    // Once: look at the model.
    const b = modelBounds();
    if (b) {
      views.scan3d.framed = true;
      views.scan3d.set({ target: toScan(b.centre), dist: b.radius * 2.6, half: b.radius * 1.1 }, true);
      setSection(sectionFrom({ view: "plan" }));
      requestSlab();
    }
  }
  if (sel.proposal) await loadDiff();
  rebuildModel(); rebuildMarks();
}

async function loadDiff() {
  const p = sel.proposal;
  if (!p) { diff = null; return; }
  const d = await service.members(p.commit);
  diff = d.error ? null : d;
  if (d.error) setInfo(active, d.error);
}

const CHANGE = { added: [60, 200, 90], removed: [235, 70, 60], moved: [60, 200, 90], from: [235, 70, 60] };
/** The members to draw, each with its colour, for the proposal's mode (or as they are). */
function drawList() {
  const hl = (m) => (sel.highlight || []).some((h) => m.path.includes(h)) || (sel.issue != null && st.issues.find((i) => i.id === sel.issue)?.place?.member === m.path);
  const plain = (m) => ({ m, rgb: hl(m) ? [255, 60, 220] : m.colour.map((c) => Math.round(c * 255)), fill: hl(m) ? 90 : 34, line: 235, change: hl(m) });
  if (!diff || !sel.proposal) return (members || []).map(plain);
  const mode = sel.proposal.mode, ch = diff.changes;
  if (mode === "before") return diff.before.map(plain);
  if (mode === "after") return diff.after.map(plain);
  const out = [];
  const faint = (m) => ({ m, rgb: [150, 155, 165], fill: 0, line: 60 });
  for (const m of diff.before) {
    const c = ch[m.path];
    if (c === "removed" || c === "moved") out.push({ m, rgb: CHANGE.from, fill: 70, line: 255, change: true });
    else out.push(mode === "both" ? plain(m) : faint(m));
  }
  for (const m of diff.after) {
    const c = ch[m.path];
    if (c === "added" || c === "moved") out.push({ m, rgb: CHANGE.added, fill: 70, line: 255, change: true });
  }
  return out;
}

/// Whether a view's points are coloured by hue (depth, normal): the model is
/// then drawn in white outline, so it reads against them. Evidence colours
/// are for views whose points are grey.
const neutralModel = (v) => v.state.colour === "depth" || v.state.colour === "normal" || v.state.colour === "height";

function rebuildModel() {
  const list = drawList();
  let nt = 0, nl = 0;
  for (const { m } of list) for (const f of m.faces) { nt += (f.length - 2) * 3; nl += f.length * 2; }
  const build = (neutral) => {
    const tris = new Float32Array(nt * 3), tcol = new Uint8Array(nt * 4), lines = new Float32Array(nl * 3), lcol = new Uint8Array(nl * 4);
    let ti = 0, li = 0;
    const put = (arr, col, k, p, rgb, a) => { arr.set(p, k * 3); col.set([rgb[0], rgb[1], rgb[2], a], k * 4); };
    for (const { m, rgb, fill, line, change } of list) {
      // A proposal's changes and highlights keep their colours.
      const plain = neutral && !change;
      const [c, a] = plain ? [[255, 255, 255], 0] : [rgb, fill];
      for (const f of m.faces) {
        const q = f.map(toScan);
        if (a) for (let i = 1; i + 1 < q.length; i++) for (const p of [q[0], q[i], q[i + 1]]) put(tris, tcol, ti++, p, c, a);
        for (let i = 0; i < q.length; i++) { put(lines, lcol, li++, q[i], c, plain ? Math.min(line, 200) : line); put(lines, lcol, li++, q[(i + 1) % q.length], c, plain ? Math.min(line, 200) : line); }
      }
    }
    return [tris.subarray(0, ti * 3), tcol.subarray(0, ti * 4), lines, lcol];
  };
  const made = {};
  for (const v of Object.values(views)) {
    const k = neutralModel(v);
    v.modelNeutral = k;
    v.setModel(...(made[k] ??= build(k)));
  }
}

/** Line segments of a box given by corners (8 points). */
function boxLines(c, rgb, out) {
  for (const [a, b] of [[0, 1], [2, 3], [4, 5], [6, 7], [0, 2], [1, 3], [4, 6], [5, 7], [0, 4], [1, 5], [2, 6], [3, 7]]) out.push([c[a], c[b], rgb]);
}
const aabb = (lo, hi) => [0, 1, 2, 3, 4, 5, 6, 7].map((k) => [k & 1 ? hi[0] : lo[0], k & 2 ? hi[1] : lo[1], k & 4 ? hi[2] : lo[2]]);

function rebuildMarks() {
  if (!st) return;
  const segs = [];
  // The selected issue's place.
  const issue = sel.issue != null && st.issues.find((i) => i.id === sel.issue);
  if (issue?.place) boxLines(aabb(issue.place.min, issue.place.max), [255, 255, 255], segs);
  // The pick: a small cross.
  if (sel.pick?.hit) {
    const p = sel.pick.hit, r = 0.08;
    for (const a of [[r, 0, 0], [0, r, 0], [0, 0, r]]) segs.push([vec.sub(p, a), vec.add(p, a), [255, 220, 40]]);
  }
  // Rooms (model frame boxes) and fitted surfaces (patches).
  for (const r of st.rooms || []) boxLines(aabb(r.min, r.max).map(toScan), [120, 170, 255], segs);
  for (const s of st.surfaces || []) {
    const at = (a, b) => toScan(vec.add(s.centre, vec.add(vec.scale(s.u, a), vec.scale(s.v, b))));
    const q = [at(s.ext[0], s.ext[2]), at(s.ext[1], s.ext[2]), at(s.ext[1], s.ext[3]), at(s.ext[0], s.ext[3])];
    for (let i = 0; i < 4; i++) segs.push([q[i], q[(i + 1) % 4], [40, 210, 200]]);
  }
  const build = (extra) => {
    const all = segs.concat(extra);
    const pos = new Float32Array(all.length * 6), col = new Uint8Array(all.length * 8);
    all.forEach(([a, b, rgb], i) => { pos.set(a, i * 6); pos.set(b, i * 6 + 3); col.set([...rgb, 255, ...rgb, 255], i * 8); });
    return [pos, col];
  };
  // In 3D, the section's plane (where the Section pane is looking).
  const sec = views.section.state.section, extra = [];
  if (sec && app.shown("section")) {
    const s = views.section.state, hw = s.zoom * ((views.section.canvas.clientWidth || 4) / (views.section.canvas.clientHeight || 3));
    const at = (a, b) => vec.add(sec.origin, vec.add(vec.scale(sec.u, s.centre[0] + a), vec.scale(sec.v, s.centre[1] + b)));
    const q = [at(-hw, -s.zoom), at(hw, -s.zoom), at(hw, s.zoom), at(-hw, s.zoom)];
    for (let i = 0; i < 4; i++) extra.push([q[i], q[(i + 1) % 4], [255, 150, 40]]);
  }
  views.scan3d.setMarks(...build(extra));
  views.section.setMarks(...build([]));
  // Issues as pins.
  const PRI = { 1: "#d62828", 2: "#1f5fbf", 3: "#6b6b66" };
  const pins = st.issues.filter((i) => i.place && (i.status !== "done" || i.id === sel.issue)).map((i) => ({
    id: i.id, pos: i.place.min.map((x, k) => 0.5 * (x + i.place.max[k])), text: `#${i.id} ${i.title.length > 26 ? i.title.slice(0, 25) + "…" : i.title}`,
    title: `${i.title} — ${STATUS[i.status]}`, colour: PRI[i.priority], on: i.id === sel.issue, dim: i.status === "done",
  }));
  for (const v of Object.values(views)) v.setLabels(pins);
}

// ----- panes: the views' chrome ------------------------------------------------------

function setInfo(kind, text) {
  const e = $(`slotinfo-${kind}`);
  if (e) e.textContent = text;
}

function makeGlPane(kind, glKind) {
  const pane = el("div", { id: `${kind}Pane`, class: "glpane" });
  const host = el("div", { class: "glhost" });
  const chip = el("div", { class: "glchip", hidden: "" });
  const bar = el("div", { class: "diffbar", hidden: "" });
  pane.append(host, chip, bar);
  $("parked").appendChild(pane);
  const v = (views[kind] = new GLView(host, glKind));
  v.chip = chip; v.diffbar = bar; v.name = kind;
  host.addEventListener("pointerdown", () => { if (active !== kind) { active = kind; renderLayers(); } }, true);
  v.onChange = () => {
    if (kind === "section") { requestSlab(); rebuildMarks(); }
    if (v.modelNeutral !== neutralModel(v)) rebuildModel();
    renderLayers();
  };
  v.onLabel = (id) => openIssue(id);
  v.onPick = async (ray) => {
    const cuts = v.cuts();
    const r = await service.pick(ray.origin, ray.dir, cuts.z, cuts.slab);
    sel.pick = r.hit ? r : null;
    renderChips(); rebuildMarks();
  };
  host.addEventListener("dblclick", () => { if (sel.pick?.hit && glKind === "3d") v.set({ target: sel.pick.hit }); });
  return pane;
}

function renderChips() {
  for (const v of Object.values(views)) {
    const p = sel.pick;
    v.chip.hidden = !p;
    if (!p) continue;
    const m = p.member;
    const ev = m && members?.find((x) => x.path === m.path)?.evidence;
    v.chip.innerHTML = `<b>model ${f3(p.model)}</b>` +
      (m ? `<br>${esc(m.path.split("/").slice(-2).join("/"))} · ${(m.distance * 1000).toFixed(0)} mm from its face` + (ev ? `<br><span class="ev ${ev.state}">${ev.state}</span> ${ev.points} points, ${ev.rays_through} of ${ev.rays_through + ev.rays_ending} rays through` : "") : "<br>no member within 0.5 m") +
      `<div class="row"><button data-a="issue">New issue here</button>${m ? `<button data-a="checks">Checks</button>` : ""}<button data-a="plan" title="A plan section at this height">Plan</button><button data-a="x" title="A section across the model's x at this point">X</button><button data-a="y" title="A section across the model's y at this point">Y</button><button data-a="clear">×</button></div>`;
    for (const b of v.chip.querySelectorAll("button")) b.onclick = () => {
      const a = b.dataset.a;
      if (a === "clear") { sel.pick = null; renderChips(); rebuildMarks(); }
      else if (a === "issue") startDraft();
      else if (a === "checks") app.inspect(m.path);
      else {
        const q = p.model;
        const spec = a === "plan" ? { view: "plan", z: [q[2] - 0.05, q[2] + 0.05] } : a === "x" ? { view: "x", at: q[0], thickness: 0.2 } : { view: "y", at: q[1], thickness: 0.2 };
        followLink({ pane: "section", section: spec });
      }
    };
  }
}

function renderDiffBars() {
  for (const v of Object.values(views)) {
    const p = sel.proposal;
    v.diffbar.hidden = !p;
    if (!p) continue;
    const n = diff ? Object.keys(diff.changes).length : 0;
    v.diffbar.innerHTML = `<span>#${p.issue} proposal ${p.n}${diff ? ` · ${n} member${n === 1 ? "" : "s"} change` : " · loading"}</span>` +
      ["before", "after", "changed", "both"].map((m) => `<button data-m="${m}" class="${m === p.mode ? "on" : ""}">${m[0].toUpperCase() + m.slice(1)}</button>`).join("") + `<button data-m="">×</button>`;
    for (const b of v.diffbar.querySelectorAll("button")) b.onclick = () => (b.dataset.m ? selectProposal({ ...p, mode: b.dataset.m }) : selectProposal(null));
  }
}

async function selectProposal(p, quiet) {
  const changed = p?.commit !== sel.proposal?.commit;
  sel.proposal = p ? { ...p } : null;
  if (changed) { diff = null; renderDiffBars(); await loadDiff(); }
  renderDiffBars(); rebuildModel();
  if (!quiet) renderIssues();
}

// ----- the browser: layers and views -------------------------------------------------

function renderLayers() {
  const v = views[active], s = v.state, box = $("layersPane");
  if (!box) return;
  const num = (x) => (x == null ? "" : x);
  box.innerHTML = `
    <h4>${s.kind === "section" ? "Section" : "3D with scan"}</h4>
    <label class="f">Colour points by <select data-k="colour">${COLOURS.map((c) => `<option ${c === s.colour ? "selected" : ""}>${c}</option>`).join("")}</select></label>
    <label class="f">Point size <input type="range" min="1" max="6" step="0.5" value="${s.pointSize}" data-k="pointSize"></label>
    ${["scan", "model", "marks", "issues"].map((k) => `<label class="c"><input type="checkbox" data-l="${k}" ${s.layers[k] ? "checked" : ""}> ${{ scan: "Scan points", model: "Model members", marks: "Marks, rooms, surfaces", issues: "Issue pins" }[k]}</label>`).join("")}
    <h4>Height cut <span class="muted">model z, m</span></h4>
    <div class="row2"><input type="number" step="0.1" placeholder="from" value="${num(s.zMin)}" data-z="zMin"><input type="number" step="0.1" placeholder="to" value="${num(s.zMax)}" data-z="zMax"></div>
    <div class="row2"><button data-a="noroof" title="Hide what is above 2.3 m: look in from above">No roof</button><button data-a="nofloor" title="Hide what is below 0.1 m">No floor</button><button data-a="nocut">All</button></div>
    ${s.kind === "section" ? `
    <h4>Slab</h4>
    <label class="f">Thickness, m <input type="number" step="0.05" min="0.01" value="${s.section ? (2 * s.section.half).toFixed(2) : ""}" data-a="thick"></label>
    <div class="row2"><button data-a="back" title="Move the slab back by half its thickness">◀ slab</button><button data-a="fwd" title="Move the slab forward by half its thickness">slab ▶</button></div>
    <div class="row2"><button data-a="plan">Plan</button><button data-a="secx">X</button><button data-a="secy">Y</button></div>` : `
    <h4>Camera</h4>
    <label class="c"><input type="checkbox" data-k="ortho" ${s.ortho ? "checked" : ""}> Orthographic</label>
    <div class="row2"><button data-a="top" title="Straight down, the model's +y up the screen">Top-down</button><button data-a="fit">Fit model</button></div>`}
    <h4>Scan</h4>
    <div class="muted">${st?.scan.loaded ? `${st.scan.points.toLocaleString()} points; ${(st.cloud ?? 0).toLocaleString()} drawn in 3D` : esc(st?.loading || "loading")}${st?.tally ? `<br>${st.tally.supported} members supported, ${st.tally.contradicted} contradicted, ${st.tally.unseen} unseen` : ""}</div>`;
  for (const i of box.querySelectorAll("[data-k]")) i.oninput = () => { v.set({ [i.dataset.k]: i.type === "checkbox" ? i.checked : i.type === "range" ? Number(i.value) : i.value }); };
  for (const i of box.querySelectorAll("[data-l]")) i.onchange = () => v.set({ layers: { [i.dataset.l]: i.checked } });
  for (const i of box.querySelectorAll("[data-z]")) i.onchange = () => v.set({ [i.dataset.z]: i.value === "" ? null : Number(i.value) });
  const target = () => toModel(views.scan3d.state.target);
  const acts = {
    noroof: () => v.set({ zMax: 2.3 }), nofloor: () => v.set({ zMin: 0.1 }), nocut: () => v.set({ zMin: null, zMax: null }),
    top: () => v.set({ az: (-pl.yaw * 180) / Math.PI + 180, el: 89.9, ortho: true }),
    fit: () => { const b = modelBounds(); if (b) v.frame(toScan(b.centre), b.radius); },
    back: () => { const c = s.section; v.set({ section: { ...c, origin: vec.add(c.origin, vec.scale(c.n, -c.half)) } }); },
    fwd: () => { const c = s.section; v.set({ section: { ...c, origin: vec.add(c.origin, vec.scale(c.n, c.half)) } }); },
    plan: () => setSection(sectionFrom({ view: "plan" })),
    secx: () => setSection(sectionFrom({ view: "x", at: target()[0] })),
    secy: () => setSection(sectionFrom({ view: "y", at: target()[1] })),
  };
  for (const b of box.querySelectorAll("button[data-a]")) b.onclick = () => { acts[b.dataset.a](); requestSlab(); rebuildMarks(); renderLayers(); };
  const th = box.querySelector("input[data-a=thick]");
  if (th) th.onchange = () => { if (s.section && Number(th.value) > 0) { v.set({ section: { ...s.section, half: Number(th.value) / 2 } }); requestSlab(); } };
}

function renderViews() {
  const box = $("viewsPane");
  box.innerHTML = `<ul>${(st.views || []).map((v, i) => `<li data-i="${i}" title="Open this view (Back returns)">${esc(v.name)}<span class="x" data-x="${i}" title="Delete">×</span></li>`).join("") || `<div class="muted" style="padding:4px 6px">No saved views yet.</div>`}</ul>
    <div class="row2"><button id="saveView">Save this view…</button></div>
    <h4>Sections kept by agents</h4>
    <ul>${(st.sections || []).map((v, i) => `<li data-s="${i}">${esc(v.name)}</li>`).join("") || `<div class="muted" style="padding:4px 6px">None.</div>`}</ul>`;
  for (const li of box.querySelectorAll("li[data-i]")) li.onclick = (e) => {
    const i = Number(li.dataset.i);
    if (e.target.dataset.x != null) { if (confirm(`Delete the view “${st.views[i].name}”?`)) service.call("views", { action: "delete", name: st.views[i].name }); }
    else followLink(st.views[i].state);
  };
  for (const li of box.querySelectorAll("li[data-s]")) li.onclick = () => { const a = st.sections[Number(li.dataset.s)].args; followLink({ pane: "section", section: a, colour: a.color }); };
  $("saveView").onclick = async () => {
    const name = prompt("Name for this view:", describeView());
    if (name) await service.call("views", { action: "save", name, state: linkOfView() });
  };
}

// ----- issues: the list, a thread, the board -------------------------------------------

const STATUS = { waiting: "Waiting", doing: "In hand", open: "Open", done: "Done" };
const PRIORITY = { 1: "high", 2: "normal", 3: "low" };
const when = (t) => {
  const d = Date.now() / 1000 - t;
  return d < 60 ? "now" : d < 3600 ? `${Math.round(d / 60)} min` : d < 86400 ? `${Math.round(d / 3600)} h` : new Date(t * 1000).toLocaleDateString();
};
/** The model's own checks (validation, clashes, bearing…) as issues that live only while they fail. */
function checkIssues() {
  const kept = new Set(st.issues.filter((i) => i.category === "check").map((i) => i.title));
  return app.checks().filter((t) => !kept.has(String(t))).map((t, k) => ({ id: `c${k}`, title: String(t), category: "check", status: "open", priority: String(t).includes("[Error]") ? 1 : 2, entries: [], ephemeral: true }));
}
function ordered() {
  const col = { doing: 0, open: 1, waiting: 2, done: 3 };
  return [...st.issues].sort((a, b) => col[a.status] - col[b.status] || a.priority - b.priority || a.rank - b.rank || a.id - b.id);
}

async function openIssue(id) {
  sel.issue = id;
  sel.drafting = false;
  app.showTab("issues");
  renderIssues(); rebuildMarks(); rebuildModel(); renderBoard();
}

function startDraft() {
  sel.issue = null;
  sel.drafting = true;
  app.showTab("issues");
  renderIssues();
  $("issuePanel").querySelector("#draftTitle")?.focus();
}

async function createIssue(title, text) {
  const args = { action: "create", title, category: sel.pick ? "scan" : "note" };
  if (text) args.text = text;
  if (sel.pick?.hit) {
    args.frame = "scan";
    args.point = sel.pick.hit;
    // On a member's face: pinned to the member.
    if (sel.pick.member && sel.pick.member.distance < 0.06) { delete args.point; delete args.frame; args.member = sel.pick.member.path; }
  }
  const r = await service.call("issues", args);
  if (r.error) return alert(r.error);
  sel.drafting = false;
  sel.pick = null;
  renderChips();
  const id = Number(r.text.match(/#(\d+)/)?.[1]);
  if (id) openIssue(id);
}

function renderIssues() {
  const box = $("issuePanel");
  if (!box || !st) return;
  const open = st.issues.filter((i) => i.status !== "done").length + checkIssues().length;
  const waiting = st.issues.filter((i) => i.status === "waiting" && i.waiting_on === service.id).length;
  const c = $("issueCount");
  c.textContent = open || "";
  c.className = "count" + (waiting ? " warn" : "");
  c.title = waiting ? `${waiting} waiting on you` : "";
  const issue = sel.issue != null && st.issues.find((i) => i.id === sel.issue);
  if (issue) return renderThread(box, issue);
  const all = [...ordered(), ...checkIssues()].filter((i) => (sel.filter === "all" || i.category === sel.filter) && (sel.done || i.status !== "done"));
  const typing = box.querySelector("#draftTitle"), typed = typing?.value ?? "", typingNow = document.activeElement === typing;
  box.innerHTML = `
    <div class="ibar">
      <button class="act primary" id="newIssue" title="${sel.pick ? "Pinned to the point you picked" : "Pick a point in a scan view first to pin it there; otherwise a note"}">New issue${sel.pick ? " here" : ""}</button>
      <select id="issueFilter" title="Category">${["all", "scan", "note", "check"].map((k) => `<option ${k === sel.filter ? "selected" : ""}>${k}</option>`).join("")}</select>
      <label class="c"><input type="checkbox" id="issueDone" ${sel.done ? "checked" : ""}> done</label>
    </div>
    ${sel.drafting ? `<div class="draft"><input type="text" id="draftTitle" placeholder="What is wrong, or what do you want to know?"><div class="muted">${sel.pick?.hit ? `At model ${f3(sel.pick.model)}${sel.pick.member && sel.pick.member.distance < 0.06 ? `, on ${esc(sel.pick.member.path.split("/").slice(-2).join("/"))}` : ""}.` : "Not pinned to a place (pick a point in a scan view to pin it)."} Enter to raise it, Esc to cancel.</div></div>` : ""}
    <ul class="ilist">${all.map((i) => `
      <li data-id="${i.id}" class="${i.status}${i.waiting_on === service.id ? " mine" : ""}">
        <span class="pri p${i.priority}" title="${PRIORITY[i.priority]} priority"></span>
        <span class="t">${i.ephemeral ? "" : `<b>#${i.id}</b> `}${esc(i.title)}</span>
        <span class="meta">${i.assignee ? badge(i.assignee, true) : ""}<span class="chip ${i.status}">${i.status === "waiting" ? `waiting on ${esc(service.participant(i.waiting_on).name)}` : STATUS[i.status]}</span><span class="cat">${i.category}</span></span>
      </li>`).join("") || `<li class="none muted">Nothing here. Pick a point in a scan view and raise what you see.</li>`}</ul>`;
  $("newIssue").onclick = startDraft;
  $("issueFilter").onchange = (e) => { sel.filter = e.target.value; renderIssues(); renderBoard(); };
  $("issueDone").onchange = (e) => { sel.done = e.target.checked; renderIssues(); };
  const dt = $("draftTitle");
  if (dt) { dt.value = typed; if (typingNow) dt.focus(); }
  if (dt) dt.onkeydown = (e) => {
    if (e.key === "Enter" && dt.value.trim()) createIssue(dt.value.trim());
    if (e.key === "Escape") { sel.drafting = false; renderIssues(); e.stopPropagation(); }
  };
  for (const li of box.querySelectorAll("li[data-id]")) li.onclick = async () => {
    const id = li.dataset.id;
    if (!id.startsWith("c")) return openIssue(Number(id));
    // A check: keeping it (to discuss it) makes it an issue.
    const chk = checkIssues().find((x) => x.id === id);
    if (chk && confirm(`Keep this check as an issue to discuss?\n\n${chk.title}`)) { sel.pick = null; const r = await service.call("issues", { action: "create", title: chk.title, category: "check" }); const n = Number(r.text.match(/#(\d+)/)?.[1]); if (n) openIssue(n); }
  };
}

function entryHtml(issue, e, k) {
  const who = `${badge(e.who, true)} <b>${esc(service.participant(e.who).name)}</b> <span class="muted">${when(e.time)}</span>`;
  if (e.kind === "comment") return `<div class="entry comment">${who}<div class="text">${esc(e.text)}</div>${e.view ? `<button class="viewlink" data-view="${k}" title="Show this in a pane (Back returns to your view)">Show: ${esc(e.view_label || "view")}</button>` : ""}</div>`;
  if (e.kind === "tool") return `<div class="entry tool">${badge(e.who, true)} ran <code>${esc(e.name)}</code> <span class="muted">${esc(e.summary)}</span></div>`;
  if (e.kind === "change") return `<div class="entry tool">${badge(e.who, true)} <span class="muted">${esc(e.what)} · ${when(e.time)}</span></div>`;
  if (e.kind === "evidence") return `<div class="entry card">${who}<div class="k">Evidence</div><b>${esc(e.title)}</b><div class="text">${esc(e.text)}</div></div>`;
  if (e.kind === "proposal") {
    const on = sel.proposal?.commit === e.commit;
    const d = (a, b) => (a === b ? `${a}` : `${a} → <b>${b}</b>`);
    return `<div class="entry card proposal ${e.state}${on ? " on" : ""}">${who}
      <div class="k">Proposal ${e.n} · ${e.state}</div><b>${esc(e.title)}</b>
      <div class="muted">${e.files.map(esc).join(", ")} <span class="add">+${e.added}</span> <span class="del">−${e.removed}</span>${e.members_added.length || e.members_removed.length ? ` · members +${e.members_added.length} −${e.members_removed.length}` : ""}</div>
      <div class="tally">supported ${d(e.before.supported, e.after.supported)} · contradicted ${d(e.before.contradicted, e.after.contradicted)} · unseen ${d(e.before.unseen, e.after.unseen)}</div>
      <div class="row"><button data-p="${e.n}" data-a="show">${on ? "Hide" : "Show in views"}</button><button data-p="${e.n}" data-a="diff">Text diff</button>${e.state === "open" ? `<button data-p="${e.n}" data-a="apply" class="go">${sel.sure?.commit === e.commit && Date.now() < sel.sure.until ? "Apply to the files?" : "Apply"}</button><button data-p="${e.n}" data-a="reject">Reject</button>` : e.state === "applied" ? `<button data-p="${e.n}" data-a="revert">Revert</button>` : ""}</div>
      <pre class="difftext" id="diff-${issue.id}-${e.n}" hidden></pre></div>`;
  }
  return "";
}

function diffHtml(text) {
  return text.split("\n").filter((l) => !l.startsWith("diff --git") && !l.startsWith("index ")).map((l) => `<span class="${l.startsWith("+") && !l.startsWith("+++") ? "add" : l.startsWith("-") && !l.startsWith("---") ? "del" : l.startsWith("@@") ? "hunk" : ""}">${esc(l)}</span>`).join("\n");
}

function renderThread(box, i) {
  // The page is redrawn when anyone does anything: what is being typed stays as it is.
  const focused = document.activeElement;
  if (focused?.id === "iTitle") return;
  const caret = focused?.id === "reply" ? [focused.selectionStart, focused.selectionEnd] : null;
  const draft = box.querySelector("#reply")?.value ?? "";
  const attach = box.querySelector("#attach")?.checked ?? false;
  const openDiffs = [...box.querySelectorAll("pre.difftext:not([hidden])")].map((p) => [p.id, p.innerHTML]);
  box.innerHTML = `
    <div class="ibar"><button class="act" id="backToList">← Issues</button>
      <select id="iStatus" title="Status">${Object.entries(STATUS).map(([k, v]) => `<option value="${k}" ${k === i.status ? "selected" : ""}>${v}</option>`).join("")}</select>
      <select id="iPriority" title="Priority">${[1, 2, 3].map((p) => `<option value="${PRIORITY[p]}" ${p === i.priority ? "selected" : ""}>${PRIORITY[p]}</option>`).join("")}</select>
      <select id="iAssignee" title="Whose it is"><option value="">nobody's</option>${st.participants.map((p) => `<option value="${esc(p.id)}" ${p.id === i.assignee ? "selected" : ""}>${esc(p.name)}</option>`).join("")}</select>
    </div>
    <div class="ihead"><b>#${i.id}</b> <span id="iTitle" title="Click to rename">${esc(i.title)}</span>
      <div class="muted">${i.category} · raised by ${esc(service.participant(i.by).name)} ${when(i.time)}${i.place ? ` · <a href="#" id="iGo">${i.place.member ? esc(i.place.member.split("/").slice(-2).join("/")) : "at model " + f3(toModel(i.place.min.map((x, k) => 0.5 * (x + i.place.max[k]))))}</a>` : " · not pinned"}${i.status === "waiting" ? ` · <b>waiting on ${esc(service.participant(i.waiting_on).name)}</b>` : ""}</div></div>
    <div class="thread">${i.entries.map((e, k) => entryHtml(i, e, k)).join("") || `<div class="muted" style="padding:8px 2px">Nothing said yet.</div>`}</div>
    <div class="composer"><textarea id="reply" rows="2" placeholder="Reply in #${i.id}… (Enter sends; @name to call someone)"></textarea>
      <div class="row"><label class="c" title="Attach the focused scan view (${esc(describeView())}) as a link others can follow"><input type="checkbox" id="attach" ${attach ? "checked" : ""}> attach my view</label><button class="act primary" id="send">Send</button></div></div>`;
  const reply = $("reply");
  reply.value = draft;
  if (caret) { reply.focus(); reply.setSelectionRange(...caret); }
  for (const [id, html] of openDiffs) { const p = $(id); if (p) { p.hidden = false; p.innerHTML = html; } }
  const send = async () => {
    const text = reply.value.trim();
    if (!text) return;
    const args = { action: "comment", id: i.id, text };
    if ($("attach").checked) { args.view = linkOfView(); args.view_label = describeView(); }
    reply.value = "";
    $("attach").checked = false;
    const r = await service.call("issues", args);
    if (r.error) alert(r.error);
  };
  reply.onkeydown = (e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); send(); } e.stopPropagation(); };
  $("send").onclick = send;
  $("backToList").onclick = () => { sel.issue = null; renderIssues(); rebuildMarks(); rebuildModel(); renderBoard(); };
  const update = (args) => service.call("issues", { action: "update", id: i.id, ...args }).then((r) => r.error && alert(r.error));
  $("iStatus").onchange = (e) => update({ status: e.target.value });
  $("iPriority").onchange = (e) => update({ priority: e.target.value });
  $("iAssignee").onchange = (e) => update({ assignee: e.target.value || null });
  // The title is edited in place: Enter keeps it, Esc puts it back.
  const title = $("iTitle");
  title.contentEditable = "plaintext-only";
  title.onkeydown = (e) => {
    e.stopPropagation();
    if (e.key === "Enter") { e.preventDefault(); title.blur(); }
    if (e.key === "Escape") { title.textContent = i.title; title.blur(); }
  };
  title.onblur = () => { const t = title.textContent.trim(); if (t && t !== i.title) update({ title: t }); else title.textContent = i.title; };
  const go = $("iGo");
  if (go) go.onclick = (e) => { e.preventDefault(); followLink({ pane: views[active].state.kind === "section" ? "section" : "3d", camera: { issue: i.id } }); };
  for (const b of box.querySelectorAll("button[data-view]")) b.onclick = () => followLink(i.entries[Number(b.dataset.view)].view);
  for (const b of box.querySelectorAll("button[data-p]")) b.onclick = async () => {
    const p = i.entries.find((e) => e.kind === "proposal" && e.n === Number(b.dataset.p)), a = b.dataset.a;
    if (a === "show") return selectProposal(sel.proposal?.commit === p.commit ? null : { issue: i.id, n: p.n, commit: p.commit, mode: "changed" });
    if (a === "diff") {
      const pre = $(`diff-${i.id}-${p.n}`);
      pre.hidden = !pre.hidden;
      if (!pre.hidden && !pre.innerHTML) pre.innerHTML = diffHtml((await service.call("proposal", { issue: i.id, n: p.n, action: "diff" })).text);
      return;
    }
    // Applying changes the model's files: asked twice, in place.
    if (a === "apply" && !(sel.sure?.commit === p.commit && Date.now() < sel.sure.until)) {
      sel.sure = { commit: p.commit, until: Date.now() + 5000 };
      setTimeout(renderIssues, 5100);
      return renderIssues();
    }
    sel.sure = null;
    b.disabled = true;
    const r = await service.call("proposal", { issue: i.id, n: p.n, action: a });
    if (r.error) alert(r.error);
    else if (sel.proposal?.commit === p.commit) selectProposal(null);
  };
  // Something new in the thread: show it.
  const seen = `${i.id}:${i.entries.length}`;
  if (box.dataset.seen !== seen) { box.dataset.seen = seen; const a = box.closest("aside"); a.scrollTop = a.scrollHeight; }
}

function renderBoard() {
  const box = $("boardPane");
  if (!box || !st) return;
  const cols = ["waiting", "doing", "open", "done"];
  const list = ordered().filter((i) => sel.filter === "all" || i.category === sel.filter);
  box.innerHTML = cols.map((c) => {
    const cards = list.filter((i) => i.status === c);
    return `<div class="col" data-col="${c}"><h4>${STATUS[c]} <span class="muted">${cards.length}</span></h4><div class="cards">${cards.map((i) => `
      <div class="card p${i.priority}${i.id === sel.issue ? " on" : ""}${i.waiting_on === service.id ? " mine" : ""}" draggable="true" data-id="${i.id}">
        <div><b>#${i.id}</b> ${esc(i.title)}</div>
        <div class="meta">${i.assignee ? badge(i.assignee, true) : ""}<span class="cat">${i.category}</span>${i.entries.length ? `<span class="muted">${i.entries.length}</span>` : ""}${i.status === "waiting" ? `<span class="muted">on ${esc(service.participant(i.waiting_on).name)}</span>` : ""}</div>
      </div>`).join("")}</div></div>`;
  }).join("");
  for (const card of box.querySelectorAll(".card")) {
    card.onclick = () => openIssue(Number(card.dataset.id));
    card.ondragstart = (e) => { e.dataTransfer.setData("text/plain", card.dataset.id); e.dataTransfer.effectAllowed = "move"; card.classList.add("drag"); };
    card.ondragend = () => card.classList.remove("drag");
  }
  for (const col of box.querySelectorAll(".col")) {
    col.ondragover = (e) => { e.preventDefault(); col.classList.add("over"); };
    col.ondragleave = () => col.classList.remove("over");
    col.ondrop = async (e) => {
      e.preventDefault();
      col.classList.remove("over");
      const id = Number(e.dataTransfer.getData("text/plain")), status = col.dataset.col;
      // Where in the column: before the card under the cursor. Its priority and a rank just above it.
      const cards = [...col.querySelectorAll(".card")].filter((c) => Number(c.dataset.id) !== id);
      const below = cards.find((c) => e.clientY < c.getBoundingClientRect().top + c.offsetHeight / 2);
      const above = below ? cards[cards.indexOf(below) - 1] : cards[cards.length - 1];
      const iss = (c) => c && st.issues.find((i) => i.id === Number(c.dataset.id));
      const [a, b] = [iss(above), iss(below)];
      const args = { action: "update", id, status };
      if (a || b) {
        args.priority = PRIORITY[(b || a).priority];
        args.rank = a && b && a.priority === b.priority ? 0.5 * (a.rank + b.rank) : b ? b.rank - 1 : a.rank + 1;
      }
      const r = await service.call("issues", args);
      if (r.error) alert(r.error);
    };
  }
  const n = (c) => st.issues.filter((i) => i.status === c).length;
  setInfo("board", `waiting ${n("waiting")} · in hand ${n("doing")} · open ${n("open")} · done ${n("done")}`);
}

// ----- state -------------------------------------------------------------------------

function onState(s) {
  if (!s) return;
  st = s;
  if (s.placement) pl = { yaw: (s.placement.yaw_deg * Math.PI) / 180, t: s.placement.t };
  for (const v of Object.values(views)) { v.zOffset = pl.t[2]; v.floorZ = s.scan.floor_z ?? 0; }
  if (s.cloud && !cloudLoaded) loadCloud();
  loadMembers();
  app.setStatus(s.loading ? `scan: ${s.loading}…` : !service.connected ? "the workspace service is not answering" : null);
  if (sel.issue != null && !s.issues.some((i) => i.id === sel.issue)) sel.issue = null;
  renderPeople(); renderIssues(); renderBoard(); renderViews(); renderLayers(); rebuildMarks(); renderDiffBars(); renderHistory();
}

export async function initScan(hooks) {
  app = hooks;
  // Panes.
  makeGlPane("scan3d", "3d");
  makeGlPane("section", "section");
  $("parked").appendChild(el("div", { id: "boardPane", class: "board" }));
  app.registerPane("scan3d", { label: "3D with scan", el: "scan3dPane", shown: () => { active = "scan3d"; renderLayers(); app.showBrowser("layers"); } });
  app.registerPane("section", { label: "Section", el: "sectionPane", shown: () => { active = "section"; renderLayers(); requestSlab(); rebuildMarks(); app.showBrowser("layers"); } });
  app.registerPane("board", { label: "Issue board", el: "boardPane", shown: () => renderBoard() });
  // The browser: layers and views.
  for (const [k, label] of [["layers", "Layers"], ["views", "Views"]]) {
    const b = el("button", { "data-b": k }, label);
    b.onclick = () => app.showBrowser(k);
    $("browserTabs").appendChild(b);
    $("browser").appendChild(el("div", { class: "bpane", "data-b": k, id: `${k}Pane`, hidden: "" }));
  }
  // The header: Back / Forward, and who is here.
  const nav = el("span", { id: "histNav" }, `<button id="histBack" disabled>◀</button><button id="histFwd" disabled title="Forward">▶</button>`);
  const people = el("span", { id: "people" });
  document.querySelector("header").append(nav, people);
  $("histBack").onclick = () => { if (history.length) { future.push(snapshot()); restore(history.pop()); renderHistory(); } };
  $("histFwd").onclick = () => { if (future.length) { history.push(snapshot()); restore(future.pop()); renderHistory(); } };
  // The issues take over the panel's Issues tab (the model's checks are a category of them).
  const pane = document.querySelector('aside .pane[data-pane="issues"]');
  pane.innerHTML = "";
  pane.appendChild(el("div", { id: "issuePanel" }));
  document.addEventListener("keydown", (e) => { if (e.key === "Escape" && sel.drafting) { sel.drafting = false; renderIssues(); } });
  window.addEventListener("topo-checks", () => renderIssues());
  service.onState(onState);
  service.connect();
  // A first look: the scan beside what was open.
  if (!app.shown("scan3d") && !app.shown("section")) app.openBeside("scan3d");
}
