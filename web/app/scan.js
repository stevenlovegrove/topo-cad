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
  if (bounds) { centre = [0.5 * (bounds[0] + bounds[1]), 0.5 * (bounds[2] + bounds[3])]; zoom = 0.52 * Math.max(bounds[3] - bounds[2], (bounds[1] - bounds[0]) / ((v.canvas.clientWidth || 4) / (v.canvas.clientHeight || 3))); }
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
  views.scan3d.animate(s.scan3d);
  views.section.animate(s.section);
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
  v.animate(patch);
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
  const hl = (m) => m.path === sel.member || (sel.highlight || []).some((h) => m.path.includes(h)) || (sel.issue != null && st.issues.find((i) => i.id === sel.issue)?.place?.member === m.path);
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
  const list = drawList().filter(({ m }) => memberVisible(m.path));
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
  if (vis("rooms")) for (const r of st.rooms || []) boxLines(aabb(r.min, r.max).map(toScan), [120, 170, 255], segs);
  for (const s of (st.surfaces || []).filter((s) => vis("planes", `plane:${s.id}`) || s.id === sel.plane)) {
    const at = (a, b) => toScan(vec.add(s.centre, vec.add(vec.scale(s.u, a), vec.scale(s.v, b))));
    const q = [at(s.ext[0], s.ext[2]), at(s.ext[1], s.ext[2]), at(s.ext[1], s.ext[3]), at(s.ext[0], s.ext[3])];
    for (let i = 0; i < 4; i++) segs.push([q[i], q[(i + 1) % 4], s.id === sel.plane ? [200, 255, 250] : [40, 210, 200]]);
    if (s.id === sel.plane) { segs.push([q[0], q[2], [200, 255, 250]]); segs.push([q[1], q[3], [200, 255, 250]]); }
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
  const pins = st.issues.filter((i) => i.place && vis("issues", `issue:${i.id}`) && (i.status !== "done" || i.id === sel.issue)).map((i) => ({
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
  // The view's own controls, and (in 3D) a compass to turn it by.
  const tools = el("div", { class: "gltools" });
  const compass = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  compass.setAttribute("viewBox", "-40 -40 80 80");
  compass.setAttribute("class", "glcompass");
  pane.append(host, chip, bar, tools);
  if (glKind === "3d") pane.append(compass);
  $("parked").appendChild(pane);
  const v = (views[kind] = new GLView(host, glKind));
  v.chip = chip; v.diffbar = bar; v.name = kind; v.tools = tools; v.compass = compass;
  if (glKind === "3d") {
    v.onDraw = (cam) => drawCompass(v, cam);
    compass.addEventListener("click", (e) => {
      const d = e.target.closest("[data-dir]")?.dataset.dir;
      if (!d) return;
      if (d === "top") return v.animate({ az: (-pl.yaw * 180) / Math.PI + 180, el: 89.9, ortho: true });
      // From that side of the model: the viewer stands there, looking back.
      const w = rot({ N: [0, 1, 0], E: [1, 0, 0], S: [0, -1, 0], W: [-1, 0, 0] }[d]);
      v.animate({ az: (Math.atan2(w[0], w[1]) * 180) / Math.PI, el: 12 });
    });
  }
  host.addEventListener("pointerdown", () => { if (active !== kind) { active = kind; renderLayers(); } }, true);
  v.onChange = () => {
    if (kind === "section") { requestSlab(); rebuildMarks(); }
    if (v.modelNeutral !== neutralModel(v)) rebuildModel();
    renderLayers();
  };
  v.onLabel = (id) => openIssue(id);
  // Double-click a pin: this view goes to its place (Back returns).
  v.onLabelZoom = (id) => {
    const i = st.issues.find((x) => x.id === id);
    if (!i?.place) return;
    pushHistory();
    v.frame(i.place.min.map((x, k) => 0.5 * (x + i.place.max[k])), Math.max(1.2, 0.8 * Math.hypot(...vec.sub(i.place.max, i.place.min))));
  };
  const pickAt = async (ray) => {
    const cuts = v.cuts();
    const r = await service.pick(ray.origin, ray.dir, cuts.z, cuts.slab);
    choose(r.hit ? "point" : null, r.hit ? r : null);
    renderChips(); rebuildMarks(); rebuildModel(); renderTree();
    // The member there fills the Member tab (without leaving the tab you are on).
    if (r.member) showMember(r.member.path);
  };
  v.onPick = pickAt;
  // What the cursor is on, for zooming toward it.
  v.onDepth = async (ray) => { const cuts = v.cuts(); return (await service.pick(ray.origin, ray.dir, cuts.z, cuts.slab)).hit ?? null; };
  // Right-click: pick what is there, then offer what can be done with it.
  v.onContext = async (ray, e) => {
    // (Looking at what is there does not change what is selected.)
    const cuts = v.cuts();
    const p = await service.pick(ray.origin, ray.dir, cuts.z, cuts.slab);
    contextMenu(v, p, e.clientX, e.clientY);
  };
  host.addEventListener("dblclick", () => { if (sel.pick?.hit && glKind === "3d") v.animate({ target: sel.pick.hit }); });
  // A window dragged with shift: the points in it become a selection. In 3D only the
  // surface nearest the viewer is taken; in a section, all of the slab's points.
  v.onSelect = async (w) => {
    const cuts = v.cuts();
    const r = await service.call("selections", { action: "window", ...w, z: cuts.z, slab: cuts.slab, front_only: glKind === "3d" });
    if (r.error) return setInfo(kind, r.error);
    const id = r.text.match(/selected (s\d+)/)?.[1];
    if (id) showSelection(id);
  };
  return pane;
}

// ----- the one thing selected ------------------------------------------------------------
// A point (a click in a view, with the member there), a member, a selection
// or a plane: one at a time, wherever it was chosen (a view, the tree, a
// link). The card over the views says what it is; the tree shows it; a new
// issue is pinned to it. What can be done with it is in its right-click
// menu, which is the same in the tree, on the card and in the views.

const shortPath = (p) => p.split("/").slice(-2).join("/");
const theSelection = () => (sel.selection && st?.selections?.find((s) => s.id === sel.selection)) || null;
const selName = (s) => `${s.id}${s.label ? " " + s.label : ""}`;
const thePlane = () => (sel.plane && st?.surfaces?.find((s) => s.id === sel.plane)) || null;
const planeName = (s) => `${s.id}${s.name ? " " + s.name : ""}`;
const planeCorners = (s, grow = 0) => [[s.ext[0] - grow, s.ext[2] - grow], [s.ext[1] + grow, s.ext[2] - grow], [s.ext[1] + grow, s.ext[3] + grow], [s.ext[0] - grow, s.ext[3] + grow]].map(([a, b]) => toScan(vec.add(s.centre, vec.add(vec.scale(s.u, a), vec.scale(s.v, b)))));

/** Selects one thing ("point": a pick; "member": a path; "selection", "plane": an id), letting go of any other; no kind: nothing. */
function choose(kind, value) {
  sel.pick = kind === "point" ? value : null;
  sel.member = kind === "member" ? value : null;
  sel.selection = kind === "selection" ? value : null;
  sel.plane = kind === "plane" ? value : null;
  renderChips(); rebuildMarks(); rebuildModel(); renderTree();
}
/** What is selected: { kind, id } or null. */
function chosen() {
  if (sel.pick) return { kind: "point", id: null };
  if (sel.member) return { kind: "member", id: sel.member };
  if (theSelection()) return { kind: "selection", id: sel.selection };
  if (thePlane()) return { kind: "plane", id: sel.plane };
  return null;
}

/// Makes a selection's points drawn (its eye on, and its group's).
async function revealSelection(id) {
  hidden.delete(`sel:${id}`); hidden.delete("selections"); keep();
  renderTree();
  await syncHighlights();
}
/// Selects a selection and shows its points (null: lets go).
async function showSelection(id) {
  choose(id ? "selection" : null, id);
  if (id) await revealSelection(id);
}
function showPlane(id) {
  choose(id ? "plane" : null, id);
}

/** Where a new issue can go, given what is selected: [{ key, label }], the likeliest first. */
function whereOptions() {
  const out = [], p = sel.pick, s = theSelection(), pn = thePlane();
  const onFace = p?.member && p.member.distance < 0.06;
  if (sel.member) out.push({ key: "member", label: `Member ${shortPath(sel.member)}` });
  if (p?.member && onFace) out.push({ key: "member", label: `Member ${shortPath(p.member.path)}` });
  if (p?.hit) out.push({ key: "point", label: `Point ${f3(p.model)}` });
  if (p?.member && !onFace) out.push({ key: "member", label: `Member ${shortPath(p.member.path)}` });
  if (s) out.push({ key: "selection", label: `Selection ${selName(s)}` });
  if (pn) out.push({ key: "plane", label: `Plane ${planeName(pn)}` });
  out.push({ key: "none", label: "Not pinned" });
  return out;
}

// ----- planes: fitted to a selection or about a point, kept, named, referred to -----------

/** Fits a plane (to a selection's points, or about a point) and keeps it. What the fit found is kept to read. */
async function fitPlane(args) {
  const r = await service.call("fit_surface", { ...args, keep: true });
  if (r.error) return alert(r.error);
  const id = r.text.match(/kept as plane (p\d+)/)?.[1];
  if (!id) return;
  (sel.fits ??= {})[id] = r.text;
  hidden.delete(`plane:${id}`); hidden.delete("planes"); keep();
  choose("plane", id);
}

/** A section on a plane: the slab about it (a volume between two planes offset from it), seen square on. */
function sectionOnPlane(s, thickness = 0.2) {
  followLink({ pane: "section", section: { view: "custom", origin: s.centre, normal: s.normal, up: s.v, thickness, bounds: [s.ext[0] - 0.3, s.ext[1] + 0.3, s.ext[2] - 0.3, s.ext[3] + 0.3] } });
}

function gotoPlane(id) {
  const s = st.surfaces?.find((x) => x.id === id);
  if (!s) return;
  pushHistory();
  const c = planeCorners(s);
  views[active].frame(c[0].map((x, k) => 0.5 * (x + c[2][k])), Math.max(0.8, 0.6 * Math.hypot(...vec.sub(c[2], c[0]))));
}
const deletePlane = async (id) => { await service.call("surfaces", { action: "delete", id }); if (sel.plane === id) choose(null); };

const sectionAt = (a, q) => followLink({ pane: "section", section: a === "plan" ? { view: "plan", z: [q[2] - 0.05, q[2] + 0.05] } : a === "x" ? { view: "x", at: q[0], thickness: 0.2 } : { view: "y", at: q[1], thickness: 0.2 } });
const deleteSelection = async (id) => { await service.call("selections", { action: "delete", id }); if (sel.selection === id) choose(null); };
const nameSelection = (id, label) => service.call("selections", { action: "label", id, label: label.trim() });

/** Goes to a selection: frames the focused scan view on its points (Back returns). */
function gotoSelection(id) {
  const s = st.selections?.find((x) => x.id === id);
  if (!s) return;
  pushHistory();
  revealSelection(id);
  views[active].frame(s.min.map((x, k) => 0.5 * (x + s.max[k])), Math.max(0.8, 0.6 * Math.hypot(...vec.sub(s.max, s.min))));
}

/** Goes to what a row or a link names, in the focused scan view. */
function gotoObject(kind, id) {
  if (kind === "member") gotoMembers(id);
  else if (kind === "group") gotoMembers(`/${id}/`);
  else if (kind === "issue") gotoIssue(Number(id));
  else if (kind === "selection") gotoSelection(id);
  else if (kind === "plane") gotoPlane(id);
  else if (kind === "point" && sel.pick?.hit) { pushHistory(); views[active].frame(sel.pick.hit, 1.5); }
}

// ----- menus: what can be done with a thing ------------------------------------------------

/** Puts the caret in the card's name field (of the selection or plane selected). */
function focusName() {
  const n = views[active].chip.querySelector(".selname");
  if (n) { n.focus(); document.getSelection().selectAllChildren(n); }
}

/** The menu of a point picked in a view `v` (`p`: the pick). */
function pointMenu(p, v) {
  const items = [], add = (label, run, cls) => items.push({ label, run, cls });
  add("New issue at this point", () => { choose("point", p); startDraft("point"); });
  if (p.member) add(`New issue on ${shortPath(p.member.path)}`, () => { choose("point", p); startDraft("member"); });
  items.push(null);
  add("Section here: plan", () => sectionAt("plan", p.model));
  add("Section here: across x", () => sectionAt("x", p.model));
  add("Section here: across y", () => sectionAt("y", p.model));
  if (v && v.state.kind !== "section") add("Orbit about this point", () => v.animate({ target: p.hit }));
  add("Fit a plane here (points within 1 m)", () => fitPlane({ seed: p.hit, frame: "scan", radius: 1.0 }));
  if (p.member) add(`Checks for ${shortPath(p.member.path)}`, () => { showMember(p.member.path); app.showTab("member"); });
  return items;
}

/** The menu of an object: the same in the tree, on the card and in the views. `key`: its row in the tree, for hiding. */
function objectMenu(kind, id) {
  const items = [], add = (label, run, cls) => items.push({ label, run, cls });
  const key = { member: `m:${id}`, group: `g:${id}`, issue: `issue:${id}`, selection: `sel:${id}`, plane: `plane:${id}`, top: id }[kind];
  const showHide = () => add(vis(key) ? "Hide" : "Show", () => toggleVis(key));
  if (kind === "member") {
    add("New issue on this member", () => { selectMember(id); startDraft("member"); });
    add("Go to it", () => gotoMembers(id));
    add("Its checks", () => { selectMember(id); app.showTab("member"); });
    showHide();
  } else if (kind === "group" || kind === "top") {
    showHide();
    if (kind === "group") add("Go to it", () => gotoMembers(`/${id}/`));
    if (id === "model") add("Show every member", () => { for (const k of [...hidden]) if (k.startsWith("g:") || k.startsWith("m:")) hidden.delete(k); hidden.delete("model"); applyVisibility(); });
  } else if (kind === "issue") {
    const i = st.issues.find((x) => x.id === Number(id));
    add("Open it", () => openIssue(i.id));
    if (i.place) add("Go to it", () => gotoIssue(i.id));
    add(i.status === "done" ? "Reopen it" : "Mark it done", () => service.call("issues", { action: "update", id: i.id, status: i.status === "done" ? "open" : "done" }));
    showHide();
    items.push(null);
    add("Delete it", () => { if (confirm(`Delete issue #${i.id} and its thread?\n\n${i.title}`)) service.call("issues", { action: "delete", id: i.id }); }, "danger");
  } else if (kind === "selection") {
    add("New issue on this selection", () => { choose("selection", id); startDraft("selection"); });
    add("Name it…", () => { choose("selection", id); focusName(); });
    add("Go to it", () => gotoSelection(id));
    add("Fit a plane to it", () => fitPlane({ selection: id }));
    showHide();
    items.push(null);
    add("Delete it", () => deleteSelection(id), "danger");
  } else if (kind === "plane") {
    const s = st.surfaces.find((x) => x.id === id);
    add("New issue on this plane", () => { choose("plane", id); startDraft("plane"); });
    add("Name it…", () => { choose("plane", id); focusName(); });
    add("Go to it", () => gotoPlane(id));
    add("Section on it", () => sectionOnPlane(s));
    showHide();
    items.push(null);
    add("Delete it", () => deletePlane(id), "danger");
  }
  return items;
}

/** The right-click menu of a scan view: for the point under the cursor, then for what is selected. */
function contextMenu(v, p, x, y) {
  const items = p?.hit ? pointMenu(p, v) : [{ label: "New issue, not pinned", run: () => startDraft("none") }];
  const c = chosen();
  if (c && c.kind !== "point") {
    const name = c.kind === "member" ? shortPath(c.id) : c.kind === "selection" ? `selection ${selName(theSelection())}` : `plane ${planeName(thePlane())}`;
    items.push(null, { label: `Selected: ${name}`, run: () => {}, cls: "head" }, ...objectMenu(c.kind, c.id));
  }
  openMenu(items, x, y);
}

/** A menu at the cursor: [{ label, run, cls }], null for a divider. */
function openMenu(items, x, y) {
  closeMenu();
  const menu = el("div", { class: "ctxmenu", id: "ctxmenu" });
  for (const it of items) {
    if (!it) { menu.appendChild(el("hr")); continue; }
    const b = el(it.cls === "head" ? "div" : "button", { class: it.cls || "" });
    b.textContent = it.label;
    b.onclick = () => { closeMenu(); it.run(); };
    menu.appendChild(b);
  }
  document.body.appendChild(menu);
  // Inside the window.
  menu.style.left = `${Math.max(4, Math.min(x, window.innerWidth - menu.offsetWidth - 6))}px`;
  menu.style.top = `${Math.max(4, Math.min(y, window.innerHeight - menu.offsetHeight - 6))}px`;
}
function closeMenu() {
  $("ctxmenu")?.remove();
}

// ----- the card: what is selected, over the views -----------------------------------------
// One thing, a line or two about it, and three small buttons: an issue on it,
// its menu, and letting go of it.

function renderChips() {
  const c = chosen(), p = sel.pick, s = theSelection(), pn = thePlane();
  for (const v of Object.values(views)) {
    v.chip.hidden = !c;
    if (!c) continue;
    // Being named: leave it as it is.
    if (v.chip.contains(document.activeElement) && document.activeElement.classList.contains("selname")) continue;
    const evLine = (path) => { const e = members?.find((x) => x.path === path)?.evidence; return e ? `<span class="ev ${e.state}">${e.state}</span> · ${e.points.toLocaleString()} points, ${e.rays_through} of ${e.rays_through + e.rays_ending} rays through` : ""; };
    const nameField = (text, plane) => `<span class="selname" ${plane ? 'data-plane="1"' : ""} contenteditable="plaintext-only" data-ph="name it" title="Its name: type, Enter keeps it">${esc(text)}</span>`;
    let head = "", body = "";
    if (c.kind === "point") {
      head = `<b>Point</b> ${f3(p.model)}`;
      body = p.member ? `${(p.member.distance * 1000).toFixed(0)} mm from ${esc(shortPath(p.member.path))} · ${evLine(p.member.path)}` : "no member within 0.5 m";
    } else if (c.kind === "member") {
      head = `<b>Member</b> ${esc(shortPath(c.id))}`;
      body = evLine(c.id);
    } else if (c.kind === "selection") {
      head = `<b>Selection</b> ${esc(s.id)} ${nameField(s.label, false)}`;
      body = `${s.count.toLocaleString()} points${vis("selections", `sel:${s.id}`) ? "" : " · hidden"}`;
    } else {
      head = `<b>Plane</b> ${esc(pn.id)} ${nameField(pn.name, true)}`;
      body = `facing ${f3(pn.normal)} · ${pn.count.toLocaleString()} points, rms ${(pn.rms * 1000).toFixed(1)} mm · ${(pn.ext[1] - pn.ext[0]).toFixed(2)} × ${(pn.ext[3] - pn.ext[2]).toFixed(2)} m` +
        (sel.fits?.[pn.id] ? `<details><summary>What the fit found</summary><pre>${esc(sel.fits[pn.id])}</pre></details>` : "");
    }
    v.chip.innerHTML = `<div class="cardhead"><span>${head}</span><span class="cardbtns"><button data-a="issue" title="New issue on this">＋</button><button data-a="menu" title="What can be done with it (the same as right-clicking it)">⋯</button><button data-a="clear" title="Let go of it">×</button></span></div><div class="muted">${body}</div>`;
    const name = v.chip.querySelector(".selname");
    if (name) {
      const plane = !!name.dataset.plane, was = plane ? pn.name : s.label;
      name.onkeydown = (e) => { e.stopPropagation(); if (e.key === "Enter") { e.preventDefault(); name.blur(); } if (e.key === "Escape") { name.textContent = was; name.blur(); } };
      name.onblur = () => { const t = name.textContent.trim(); if (t !== was) plane ? service.call("surfaces", { action: "label", id: pn.id, label: t }) : nameSelection(s.id, t); };
    }
    for (const b of v.chip.querySelectorAll("button")) b.onclick = (e) => {
      const a = b.dataset.a;
      if (a === "clear") choose(null);
      else if (a === "issue") startDraft();
      else { const r = b.getBoundingClientRect(); openMenu(c.kind === "point" ? pointMenu(p, v) : objectMenu(c.kind, c.id), r.left, r.top - 4); e.stopPropagation(); }
    };
    v.chip.oncontextmenu = (e) => { e.preventDefault(); openMenu(c.kind === "point" ? pointMenu(p, v) : objectMenu(c.kind, c.id), e.clientX, e.clientY); };
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

// ----- what is shown: visibility, apart from what is selected ---------------------------
// Every object and every group of the tree has an eye: whether it is drawn.
// That is separate from selecting it (clicking it), which says what the chip,
// the Member tab and a new issue refer to. Hidden keys are remembered.

const stored = (k, d) => { try { return JSON.parse(localStorage.getItem(k) ?? "null") ?? d; } catch (_) { return d; } };
const hidden = new Set(stored("topo.scan.hidden", []));
const open = new Set(stored("topo.scan.open", ["scan", "model", "issues", "selections", "planes"]));
const keep = () => { try { localStorage.setItem("topo.scan.hidden", JSON.stringify([...hidden])); localStorage.setItem("topo.scan.open", JSON.stringify([...open])); } catch (_) {} };
const vis = (...keys) => keys.every((k) => !hidden.has(k));

/** A member's place in the tree: its groups (below the building) and its own name. */
function memberParts(path) {
  const parts = path.split("/");
  return { groups: parts.slice(1, -1), name: parts[parts.length - 1] };
}
/** Whether a member is drawn: the model, its groups and itself are all shown. */
function memberVisible(path) {
  if (hidden.has("model") || hidden.has(`m:${path}`)) return false;
  const g = memberParts(path).groups;
  for (let k = 1; k <= g.length; k++) if (hidden.has(`g:${g.slice(0, k).join("/")}`)) return false;
  return true;
}

function applyVisibility() {
  keep();
  for (const v of Object.values(views)) v.set({ layers: { scan: vis("scan"), model: true, marks: true, issues: true } }, true);
  rebuildModel(); rebuildMarks(); syncHighlights(); renderTree();
}
function toggleVis(key) {
  hidden.has(key) ? hidden.delete(key) : hidden.add(key);
  applyVisibility();
}

// Selections shown (their eye is on) are drawn as highlighted points; loaded once each.
const selClouds = new Map();
async function syncHighlights() {
  const want = (st?.selections || []).filter((s) => vis("selections", `sel:${s.id}`)).map((s) => s.id);
  for (const v of Object.values(views)) for (const key of [...v.highlights.keys()]) if (!want.includes(key)) v.setHighlight(key, null);
  for (const id of want) {
    if (!selClouds.has(id)) selClouds.set(id, await service.selectionCloud(id));
    const c = selClouds.get(id);
    if (c) for (const v of Object.values(views)) if (!v.highlights.has(id)) v.setHighlight(id, c);
  }
}

// ----- the tree of objects --------------------------------------------------------------

const EYE = `<svg viewBox="0 0 16 16" width="14" height="14"><path d="M1.5 8s2.4-4.2 6.5-4.2S14.5 8 14.5 8s-2.4 4.2-6.5 4.2S1.5 8 1.5 8z" fill="none" stroke="currentColor" stroke-width="1.2"/><circle cx="8" cy="8" r="2" fill="currentColor"/></svg>`;
const EYE_OFF = `<svg viewBox="0 0 16 16" width="14" height="14"><path d="M1.5 8s2.4-4.2 6.5-4.2S14.5 8 14.5 8s-2.4 4.2-6.5 4.2S1.5 8 1.5 8z" fill="none" stroke="currentColor" stroke-width="1.2" opacity=".45"/><path d="M3 13L13 3" stroke="currentColor" stroke-width="1.3"/></svg>`;

function selectMember(path) {
  choose("member", path);
  showMember(path);
}

/** Fills the Member tab: what the scan shows of the member, then its structural checks. */
async function showMember(path) {
  await app.inspect(path, true);
  const m = members?.find((x) => x.path === path), box = $("inspector");
  if (!m || !box) return;
  const e = m.evidence, rays = e.rays_through + e.rays_ending;
  box.insertAdjacentHTML("afterbegin", `<div class="head evidence"><b>In the scan</b> <span class="ev ${e.state}">${e.state}</span>
    <table><tr><td>Scan points on its faces</td><td class="num">${e.points.toLocaleString()} <span class="muted">(${e.density.toLocaleString()} per m²)</span></td></tr>
    <tr><td>Their offset from the faces</td><td class="num">${e.offset_mm.toFixed(1)} mm mean, ${e.rms_mm.toFixed(1)} mm rms</td></tr>
    <tr><td>Sensor rays reaching it</td><td class="num">${rays.toLocaleString()}: ${e.rays_through.toLocaleString()} pass through (${rays ? Math.round((100 * e.rays_through) / rays) : 0}%)</td></tr></table>
    <div class="muted">${e.state === "contradicted" ? "Rays passing through a member say it is not there, or not exactly there." : e.state === "unseen" ? "Too few points and rays to say." : "Points on its faces, and few rays through it."} Evidence, not a verdict.</div></div>`);
}

function gotoMembers(filter) {
  const b = modelBounds(filter);
  if (!b) return;
  pushHistory();
  views[active].frame(toScan(b.centre), Math.max(b.radius, 0.6));
}
function gotoIssue(id) {
  const i = st.issues.find((x) => x.id === id);
  if (!i?.place) return;
  pushHistory();
  views[active].frame(i.place.min.map((x, k) => 0.5 * (x + i.place.max[k])), Math.max(1.2, 0.8 * Math.hypot(...vec.sub(i.place.max, i.place.min))));
}

function renderTree() {
  const box = $("layersPane");
  if (!box || !st) return;
  const row = (depth, kind, key, name, opts = {}) => {
    const tw = opts.group ? `<span class="tw" data-tw>${open.has(key) ? "▾" : "▸"}</span>` : `<span class="tw"></span>`;
    return `<div class="trow${opts.on ? " on" : ""}${vis(key) ? "" : " off"}" data-kind="${kind}" data-key="${esc(key)}" ${opts.id != null ? `data-id="${esc(opts.id)}"` : ""} style="padding-left:${4 + depth * 12}px" title="${esc(opts.title || name)}">${tw}<span class="eye" data-eye title="Show or hide">${vis(key) ? EYE : EYE_OFF}</span>${opts.dot ? `<i class="dot ${opts.dot}"></i>` : ""}<span class="tn">${esc(name)}</span>${opts.note != null ? `<span class="muted">${esc(opts.note)}</span>` : ""}</div>`;
  };
  const mil = (n) => `${(n / 1e6).toFixed(1)}M`;
  let html = row(0, "top", "scan", "Scan", { note: st.scan.loaded ? `${mil(st.cloud ?? 0)} of ${mil(st.scan.points)} points` : st.loading || "loading", title: "The scan's points (thinned for the 3D view; a section draws every point of its slab)" });
  // The model: its groups, then its members, with their evidence as a dot.
  const list = members || [];
  html += row(0, "top", "model", "Model", { group: true, note: st.tally ? `${list.length} · ${st.tally.contradicted} contradicted` : list.length, title: st.tally ? `${st.tally.supported} members supported by the scan, ${st.tally.contradicted} contradicted, ${st.tally.unseen} unseen` : "Model" });
  if (open.has("model")) {
    const node = { groups: new Map(), members: [] };
    for (const m of list) {
      let n = node;
      for (const g of memberParts(m.path).groups) { if (!n.groups.has(g)) n.groups.set(g, { groups: new Map(), members: [] }); n = n.groups.get(g); }
      n.members.push(m);
    }
    const count = (n) => n.members.length + [...n.groups.values()].reduce((a, g) => a + count(g), 0);
    const bad = (n) => n.members.filter((m) => m.evidence.state === "contradicted").length + [...n.groups.values()].reduce((a, g) => a + bad(g), 0);
    const walk = (n, depth, prefix) => {
      for (const [name, g] of n.groups) {
        const path = prefix ? `${prefix}/${name}` : name, key = `g:${path}`, b = bad(g);
        html += row(depth, "group", key, name, { group: true, id: path, note: count(g), dot: b ? "contradicted" : "", title: `${path}: ${count(g)} members${b ? `, ${b} contradicted by the scan` : ""}` });
        if (open.has(key)) walk(g, depth + 1, path);
      }
      for (const m of n.members) html += row(depth, "member", `m:${m.path}`, memberParts(m.path).name, { id: m.path, dot: m.evidence.state, on: m.path === (sel.pick?.member?.path ?? sel.member), title: `${m.path} — ${m.evidence.state}` });
    };
    walk(node, 1, "");
  }
  html += row(0, "top", "issues", "Issues", { group: true, note: st.issues.filter((i) => i.status !== "done").length });
  if (open.has("issues")) for (const i of ordered()) html += row(1, "issue", `issue:${i.id}`, `#${i.id} ${i.title}`, { id: i.id, on: i.id === sel.issue && !chosen(), note: i.place ? "" : "not pinned", dot: i.status === "done" ? "unseen" : i.status === "waiting" ? "contradicted" : "" });
  html += row(0, "top", "selections", "Selections", { group: true, note: (st.selections || []).length || "shift-drag" });
  if (open.has("selections")) for (const s of st.selections || []) html += row(1, "sel", `sel:${s.id}`, selName(s), { id: s.id, on: s.id === sel.selection, note: s.count.toLocaleString() });
  html += row(0, "top", "planes", "Planes", { group: true, note: (st.surfaces || []).length || "right-click: fit" });
  if (open.has("planes")) for (const s of st.surfaces || []) html += row(1, "plane", `plane:${s.id}`, planeName(s), { id: s.id, on: s.id === sel.plane, note: `${(s.rms * 1000).toFixed(0)} mm` });
  if ((st.rooms || []).length) html += row(0, "top", "rooms", "Rooms", { note: st.rooms.length });
  const top = box.scrollTop;
  box.innerHTML = `<div class="tree">${html}</div>`;
  box.scrollTop = top;
}

/** What a row of the tree is: { kind, key, id }. */
const rowOf = (e) => { const r = e.target.closest(".trow"); return r && { el: r, kind: r.dataset.kind, key: r.dataset.key, id: r.dataset.id }; };

function bindTree() {
  const box = $("layersPane");
  // (A row is drawn again when it is selected, so a double click is told by its timing, not by the browser.)
  let last = { key: null, t: 0 };
  box.addEventListener("click", (e) => {
    const r = rowOf(e);
    if (!r) return;
    if (e.target.closest("[data-eye]")) return toggleVis(r.key);
    const kind = r.kind === "sel" ? "selection" : r.kind, now = performance.now();
    const twice = last.key === r.key && now - last.t < 400;
    last = { key: r.key, t: twice ? 0 : now };
    // Twice: go to it.
    if (twice && kind !== "top") return gotoObject(kind, r.id);
    if (kind === "top" || kind === "group") {
      // The arrow (or the row, of a group that has no other use) folds it.
      if (r.el.querySelector("[data-tw]")) { open.has(r.key) ? open.delete(r.key) : open.add(r.key); keep(); renderTree(); }
      return;
    }
    if (kind === "member") selectMember(r.id);
    else if (kind === "issue") openIssue(Number(r.id));
    else if (kind === "selection") choose("selection", r.id);
    else if (kind === "plane") choose("plane", r.id);
  });
  box.addEventListener("contextmenu", (e) => {
    const r = rowOf(e);
    if (!r) return;
    e.preventDefault();
    openMenu(objectMenu(r.kind === "sel" ? "selection" : r.kind, r.kind === "top" ? r.key : r.id), e.clientX, e.clientY);
  });
}

// ----- a view's own controls: in the view, not in the browser ----------------------------

const svg = (d, extra = "") => `<svg viewBox="0 0 16 16" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" stroke-linecap="round">${extra}<path d="${d}"/></svg>`;
const ICON = {
  ortho: svg("M3 5l5-2.5L13 5v6l-5 2.5L3 11z M3 5l5 2.5L13 5 M8 7.5v6"),
  top: svg("M3 9.5h10v4H3z M8 2v5.5 M5.8 5.3L8 7.5l2.2-2.2"),
  fit: svg("M2.5 6V2.5H6 M10 2.5h3.5V6 M13.5 10v3.5H10 M6 13.5H2.5V10"),
  noroof: svg("M3.5 8v5.5h9V8", `<path d="M2 8l6-5 6 5" stroke-dasharray="1.6 1.8"/>`),
  nofloor: svg("M2 8l6-5 6 5 M3.5 8v5.5 M12.5 8v5.5", `<path d="M3.5 13.5h9" stroke-dasharray="1.6 1.8"/>`),
  back: svg("M10 3L5 8l5 5"),
  fwd: svg("M6 3l5 5-5 5"),
};

function renderTools(v) {
  const s = v.state, sec = s.kind === "section";
  // Not while one of its fields is being typed in.
  if (v.tools.contains(document.activeElement) && document.activeElement.tagName === "INPUT") return;
  const btn = (a, icon, title, on) => `<button data-a="${a}" class="${on ? "on" : ""}" title="${title}">${icon}</button>`;
  v.tools.innerHTML =
    `<select data-k="colour" title="Colour the points by">${COLOURS.map((c) => `<option ${c === s.colour ? "selected" : ""}>${c}</option>`).join("")}</select>` +
    (sec
      ? `<span class="grp">${btn("plan", "Plan", "A plan section (walls: 0.1 to 2.4 m)")}${btn("secx", "X", "A section across the model's x, at the 3D view's target")}${btn("secy", "Y", "A section across the model's y, at the 3D view's target")}</span>` +
        `<span class="grp" title="The slab: its thickness (m), and stepping it back and forward by half of that">${btn("back", ICON.back, "Slab back")}<input type="number" step="0.05" min="0.01" value="${s.section ? (2 * s.section.half).toFixed(2) : ""}" data-a="thick">${btn("fwd", ICON.fwd, "Slab forward")}</span>`
      : `<span class="grp">${btn("ortho", ICON.ortho, "Orthographic (no perspective)", s.ortho)}${btn("top", ICON.top, "Straight down, the model's +y up")}${btn("fit", ICON.fit, "Fit the model in the view")}</span>`) +
    `<span class="grp">${btn("noroof", ICON.noroof, "Cut away what is above 2.3 m (the roof)", s.zMax != null)}${btn("nofloor", ICON.nofloor, "Cut away what is below 0.1 m (the floor)", s.zMin != null)}</span>` +
    `<span class="grp" title="Point size">${btn("smaller", "−", "Smaller points")}${btn("bigger", "+", "Larger points")}</span>` +
    `<details class="more"><summary title="Height cut, exactly">⋯</summary><div>Show model z from <input type="number" step="0.1" placeholder="any" value="${s.zMin ?? ""}" data-z="zMin"> to <input type="number" step="0.1" placeholder="any" value="${s.zMax ?? ""}" data-z="zMax"> m</div></details>`;
  const target = () => toModel(views.scan3d.state.target);
  const acts = {
    ortho: () => v.set({ ortho: !s.ortho }),
    top: () => v.animate({ az: (-pl.yaw * 180) / Math.PI + 180, el: 89.9, ortho: true }),
    fit: () => { const b = modelBounds(); if (b) v.frame(toScan(b.centre), b.radius); },
    noroof: () => v.set({ zMax: s.zMax != null ? null : 2.3 }), nofloor: () => v.set({ zMin: s.zMin != null ? null : 0.1 }),
    smaller: () => v.set({ pointSize: Math.max(1, s.pointSize - 0.5) }), bigger: () => v.set({ pointSize: Math.min(6, s.pointSize + 0.5) }),
    back: () => { const c = s.section; v.set({ section: { ...c, origin: vec.add(c.origin, vec.scale(c.n, -c.half)) } }); },
    fwd: () => { const c = s.section; v.set({ section: { ...c, origin: vec.add(c.origin, vec.scale(c.n, c.half)) } }); },
    plan: () => setSection(sectionFrom({ view: "plan" })),
    secx: () => setSection(sectionFrom({ view: "x", at: target()[0] })),
    secy: () => setSection(sectionFrom({ view: "y", at: target()[1] })),
  };
  for (const b of v.tools.querySelectorAll("button[data-a]")) b.onclick = () => { acts[b.dataset.a](); requestSlab(); rebuildMarks(); };
  v.tools.querySelector("select").onchange = (e) => v.set({ colour: e.target.value });
  for (const i of v.tools.querySelectorAll("[data-z]")) i.onchange = () => v.set({ [i.dataset.z]: i.value === "" ? null : Number(i.value) });
  const th = v.tools.querySelector("input[data-a=thick]");
  if (th) th.onchange = () => { if (s.section && Number(th.value) > 0) { v.set({ section: { ...s.section, half: Number(th.value) / 2 } }); requestSlab(); } };
}

/** The 3D view's compass: the model's north, east, south and west as the view has them. Click one to look from that side; the middle, straight down. */
function drawCompass(v, cam) {
  const R = 25, names = { N: "north", E: "east", S: "south", W: "west" };
  // The far ones first, so the near ones are drawn over them; letters are outlined, not boxed, so none hides another.
  const dirs = [["N", [0, 1, 0]], ["E", [1, 0, 0]], ["S", [0, -1, 0]], ["W", [-1, 0, 0]]].map(([name, d]) => {
    const w = rot(d);
    return { name, x: vec.dot(w, cam.right) * R, y: -vec.dot(w, cam.up) * R, depth: vec.dot(w, cam.forward) };
  }).sort((a, b) => b.depth - a.depth);
  let out = `<circle r="${R + 12}" fill="rgba(22,23,27,.78)" stroke="rgba(255,255,255,.4)"/>`;
  const up = [vec.dot([0, 0, 1], cam.right) * R * 0.6, -vec.dot([0, 0, 1], cam.up) * R * 0.6];
  out += `<line x2="${up[0].toFixed(1)}" y2="${up[1].toFixed(1)}" stroke="#6ea8ff" stroke-width="1.5"/>`;
  for (const d of dirs) {
    const far = d.depth > 0.25, col = d.name === "N" ? "#ff7a6e" : "#ffffff";
    const lx = d.x * 1.22, ly = d.y * 1.22;
    out += `<line x2="${d.x.toFixed(1)}" y2="${d.y.toFixed(1)}" stroke="${col}" stroke-opacity="${far ? 0.35 : 0.8}" stroke-width="${d.name === "N" ? 2 : 1.2}"/>` +
      `<g class="hot" data-dir="${d.name}"><title>Look from the ${names[d.name]}</title><circle class="hit" cx="${lx.toFixed(1)}" cy="${ly.toFixed(1)}" r="8"/>` +
      `<text x="${lx.toFixed(1)}" y="${(ly + 4).toFixed(1)}" text-anchor="middle" font-size="11" font-weight="700" fill="${col}" fill-opacity="${far ? 0.55 : 1}" stroke="#16171b" stroke-width="2.6" paint-order="stroke">${d.name}</text></g>`;
  }
  out += `<g class="hot" data-dir="top"><title>Look straight down</title><circle class="hit" r="7"/><circle r="3.2" fill="#6ea8ff" stroke="#16171b"/></g>`;
  if (v.compassHtml !== out) { v.compassHtml = out; v.compass.innerHTML = out; }
}

function renderLayers() {
  renderTree();
  for (const v of Object.values(views)) renderTools(v);
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
// Who an issue waits on: someone, or (an agent asking in its own issue) any person.
const waitingOn = (i) => (i.waiting_on ? service.participant(i.waiting_on).name : "a person");
const onMe = (i) => i.status === "waiting" && (i.waiting_on ? i.waiting_on === service.id : true);
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
  // The issue is what is selected now.
  choose(null);
  sel.issue = id;
  sel.drafting = false;
  app.showTab("issues");
  renderIssues(); rebuildMarks(); rebuildModel(); renderBoard();
  // An issue pinned to a selection shows its points.
  const s = st.issues.find((i) => i.id === id)?.place?.selection;
  if (s) revealSelection(s);
}

/// Starts a new issue: its title is typed in place; `where` is what it is
/// pinned to ("point", "member", "selection" or "none"; default the likeliest).
function startDraft(where) {
  sel.issue = null;
  sel.drafting = true;
  sel.where = where ?? whereOptions()[0].key;
  app.showTab("issues");
  renderIssues();
  $("issuePanel").querySelector("#draftTitle")?.focus();
}

async function createIssue(title, text) {
  const where = whereOptions().some((w) => w.key === sel.where) ? sel.where : "none";
  const args = { action: "create", title, category: where === "none" ? "note" : "scan" };
  if (text) args.text = text;
  if (where === "selection") args.selection = sel.selection;
  else if (where === "plane") args.plane = sel.plane;
  else if (where === "member") args.member = sel.pick?.member?.path ?? sel.member;
  else if (where === "point") { args.frame = "scan"; args.point = sel.pick.hit; }
  const r = await service.call("issues", args);
  if (r.error) return alert(r.error);
  sel.drafting = false;
  choose(null);
  renderChips();
  const id = Number(r.text.match(/#(\d+)/)?.[1]);
  if (id) openIssue(id);
}

function renderIssues() {
  const box = $("issuePanel");
  if (!box || !st) return;
  const open = st.issues.filter((i) => i.status !== "done").length + checkIssues().length;
  const waiting = st.issues.filter((i) => onMe(i)).length;
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
      <button class="act primary" id="newIssue" title="Raise an issue: on the point, member or selection you are pointing at in a scan view, or not pinned">New issue…</button>
      <select id="issueFilter" title="Category">${["all", "scan", "note", "check"].map((k) => `<option ${k === sel.filter ? "selected" : ""}>${k}</option>`).join("")}</select>
      <label class="c"><input type="checkbox" id="issueDone" ${sel.done ? "checked" : ""}> done</label>
    </div>
    ${sel.drafting ? `<div class="draft"><input type="text" id="draftTitle" placeholder="What is wrong, or what do you want to know?">
      <div class="where"><span class="muted">Pinned to</span>${whereOptions().map((w) => `<button data-where="${w.key}" class="${w.key === sel.where ? "on" : ""}">${esc(w.label)}</button>`).join("")}</div>
      <div class="muted">${whereOptions().length === 1 ? "Click a point, or shift-drag a region, in a scan view to pin it there. " : ""}Enter to raise it, Esc to cancel.</div></div>` : ""}
    <ul class="ilist">${all.map((i) => `
      <li data-id="${i.id}" class="${i.status}${onMe(i) ? " mine" : ""}">
        <span class="pri p${i.priority}" title="${PRIORITY[i.priority]} priority"></span>
        <span class="t">${i.ephemeral ? "" : `<b>#${i.id}</b> `}${esc(i.title)}</span>
        <span class="meta">${i.assignee ? badge(i.assignee, true) : ""}<span class="chip ${i.status}">${i.status === "waiting" ? `waiting on ${esc(waitingOn(i))}` : STATUS[i.status]}</span><span class="cat">${i.category}</span></span>
      </li>`).join("") || `<li class="none muted">Nothing here. Pick a point in a scan view and raise what you see.</li>`}</ul>`;
  $("newIssue").onclick = () => startDraft();
  for (const b of box.querySelectorAll("button[data-where]")) b.onclick = () => { sel.where = b.dataset.where; renderIssues(); $("draftTitle")?.focus(); };
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
    if (chk && confirm(`Keep this check as an issue to discuss?\n\n${chk.title}`)) { const r = await service.call("issues", { action: "create", title: chk.title, category: "check" }); const n = Number(r.text.match(/#(\d+)/)?.[1]); if (n) openIssue(n); }
  };
}

/// Text with what it refers to made live: a selection by its id (s3) shows that selection; an issue (#4) opens it.
function refs(text) {
  return esc(text).replace(/(^|[^\w#])(s\d+|p\d+|#\d+)\b/g, (all, pre, id) => {
    if (id[0] === "p") { const pn = st.surfaces?.find((x) => x.id === id); return pn ? `${pre}<a href="#" class="ref sel" data-planeref="${id}" title="Show this plane">${esc(planeName(pn))}</a>` : all; }
    if (id[0] === "#") return st.issues.some((i) => i.id === Number(id.slice(1))) ? `${pre}<a href="#" class="ref" data-issue="${id.slice(1)}">${id}</a>` : all;
    const s = st.selections?.find((x) => x.id === id);
    return s ? `${pre}<a href="#" class="ref sel" data-selref="${id}" title="Show this selection (${s.count.toLocaleString()} points)">${esc(selName(s))}</a>` : all;
  });
}

function entryHtml(issue, e, k) {
  const who = `${badge(e.who, true)} <b>${esc(service.participant(e.who).name)}</b> <span class="muted">${when(e.time)}</span>`;
  if (e.kind === "comment") return `<div class="entry comment">${who}<div class="text">${refs(e.text)}</div>${e.view ? `<button class="viewlink" data-view="${k}" title="Show this in a pane (Back returns to your view)">Show: ${esc(e.view_label || "view")}</button>` : ""}</div>`;
  if (e.kind === "tool") return `<div class="entry tool">${badge(e.who, true)} ran <code>${esc(e.name)}</code> <span class="muted">${esc(e.summary)}</span></div>`;
  if (e.kind === "change") return `<div class="entry tool">${badge(e.who, true)} <span class="muted">${esc(e.what)} · ${when(e.time)}</span></div>`;
  if (e.kind === "evidence") return `<div class="entry card">${who}<div class="k">Evidence</div><b>${esc(e.title)}</b><div class="text">${refs(e.text)}</div></div>`;
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
      <div class="muted">${i.category} · raised by ${esc(service.participant(i.by).name)} ${when(i.time)}${i.place ? ` · <a href="#" id="iGo" title="Go there (Back returns)">${i.place.member ? "member " + esc(shortPath(i.place.member)) : i.place.selection ? "selection " + esc(selName(st.selections?.find((s) => s.id === i.place.selection) ?? { id: i.place.selection })) : i.place.plane ? "plane " + esc(planeName(st.surfaces?.find((s) => s.id === i.place.plane) ?? { id: i.place.plane })) : "at model " + f3(toModel(i.place.min.map((x, k) => 0.5 * (x + i.place.max[k]))))}</a>` : " · not pinned"}${i.status === "waiting" ? ` · <b>waiting on ${esc(waitingOn(i))}</b>` : ""}</div></div>
    <div class="thread">${i.entries.map((e, k) => entryHtml(i, e, k)).join("") || `<div class="muted" style="padding:8px 2px">Nothing said yet.</div>`}</div>
    <div class="composer"><textarea id="reply" rows="2" placeholder="Reply in #${i.id}… (Enter sends; @name to call someone)"></textarea>
      <div class="row"><label class="c" title="Attach the focused scan view (${esc(describeView())}) as a link others can follow"><input type="checkbox" id="attach" ${attach ? "checked" : ""}> attach my view</label>${theSelection() ? `<button data-ref="${esc(sel.selection)}" title="Refer to the selection you have shown: its id is put in your reply, and becomes a link">+ ${esc(selName(theSelection()))}</button>` : ""}${thePlane() ? `<button data-ref="${esc(sel.plane)}" title="Refer to the plane you have shown">+ ${esc(planeName(thePlane()))}</button>` : ""}<button class="act primary" id="send">Send</button></div></div>`;
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
  for (const a of box.querySelectorAll("a[data-selref]")) a.onclick = (e) => { e.preventDefault(); gotoSelection(a.dataset.selref); };
  for (const a of box.querySelectorAll("a[data-planeref]")) a.onclick = (e) => { e.preventDefault(); gotoPlane(a.dataset.planeref); };
  for (const b of box.querySelectorAll("button[data-ref]")) b.onclick = () => { const at = reply.selectionStart ?? reply.value.length; reply.setRangeText(`${at && !/\s$/.test(reply.value.slice(0, at)) ? " " : ""}${b.dataset.ref} `, at, reply.selectionEnd ?? at, "end"); reply.focus(); };
  for (const a of box.querySelectorAll("a[data-issue]")) a.onclick = (e) => { e.preventDefault(); openIssue(Number(a.dataset.issue)); };
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
      <div class="card p${i.priority}${i.id === sel.issue ? " on" : ""}${onMe(i) ? " mine" : ""}" draggable="true" data-id="${i.id}">
        <div><b>#${i.id}</b> ${esc(i.title)}</div>
        <div class="meta">${i.assignee ? badge(i.assignee, true) : ""}<span class="cat">${i.category}</span>${i.entries.length ? `<span class="muted">${i.entries.length}</span>` : ""}${i.status === "waiting" ? `<span class="muted">on ${esc(waitingOn(i))}</span>` : ""}</div>
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
  renderPeople(); renderIssues(); renderBoard(); renderViews(); renderLayers(); rebuildMarks(); renderDiffBars(); renderHistory(); renderChips(); syncHighlights();
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
  for (const [k, label] of [["layers", "Objects"], ["views", "Views"]]) {
    const b = el("button", { "data-b": k }, label);
    b.onclick = () => app.showBrowser(k);
    $("browserTabs").appendChild(b);
    $("browser").appendChild(el("div", { class: "bpane", "data-b": k, id: `${k}Pane`, hidden: "" }));
  }
  // The header: Back / Forward, and who is here.
  const nav = el("span", { id: "histNav" }, `<button id="histBack" disabled>◀</button><button id="histFwd" disabled title="Forward">▶</button>`);
  const people = el("span", { id: "people" });
  document.querySelector("header").append(nav, people);
  bindTree();
  $("histBack").onclick = () => { if (history.length) { future.push(snapshot()); restore(history.pop()); renderHistory(); } };
  $("histFwd").onclick = () => { if (future.length) { history.push(snapshot()); restore(future.pop()); renderHistory(); } };
  // The issues take over the panel's Issues tab (the model's checks are a category of them).
  const pane = document.querySelector('aside .pane[data-pane="issues"]');
  pane.innerHTML = "";
  pane.appendChild(el("div", { id: "issuePanel" }));
  document.addEventListener("keydown", (e) => { if (e.key === "Escape" && sel.drafting) { sel.drafting = false; renderIssues(); } });
  window.addEventListener("topo-checks", () => renderIssues());
  // The right-click menu closes on anything else.
  document.addEventListener("pointerdown", (e) => { if (!e.target.closest?.("#ctxmenu")) closeMenu(); }, true);
  document.addEventListener("keydown", (e) => { if (e.key === "Escape") closeMenu(); });
  service.onState(onState);
  service.connect();
  // A first look: the scan beside what was open.
  if (!app.shown("scan3d") && !app.shown("section")) app.openBeside("scan3d");
}
