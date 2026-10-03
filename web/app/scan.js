// The scan workspace in the app (only when the site has the service: see
// service.js). It adds:
//   - panes: "3D with scan", two more 3D views (all glview.js) and "Board";
//     any of the 3D views can be clipped to a slab, which is what a section is;
//   - in the browser: the objects (model, issues, selections, planes, slabs,
//     polylines) and views;
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
const views = {};              // scan3d, view2, view3: GLView (each with `slab`: the id of the slab it is clipped to)
const KINDS = ["scan3d", "view2", "view3"], SECOND = ["view2", "view3"];
let active = "scan3d";         // the scan view last used
const sel = { issue: null, proposal: null, pick: null, filter: "all", done: false, drafting: false };
const history = [], future = [];
let cloudLoaded = false;
const temps = [];              // slabs not kept: made by following a section link
let tempN = 0;
let live = null;               // a slab or polyline as it is dragged: { kind, id, value, done }
let drawing = null;            // a polyline being drawn: { view, points (model frame) }

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
  // An agent's badge has a little robot's head in it, so it is not taken for a person.
  return `<span class="who${small ? " small" : ""}${p.kind === "agent" ? " agent" : ""}" style="--who:${esc(p.colour)}" title="${esc(p.name)}${p.kind === "agent" ? " (agent)" : ""}">${p.kind === "agent" ? BOT : ""}${esc(initials)}</span>`;
}
const BOT = `<svg class="bot" viewBox="0 0 12 12" width="11" height="11"><path d="M6 .8v1.7" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/><rect x="1.5" y="2.8" width="9" height="7.4" rx="2" fill="currentColor"/><circle cx="4.2" cy="6.3" r="1.1" fill="var(--who)"/><circle cx="7.8" cy="6.3" r="1.1" fill="var(--who)"/></svg>`;
const BELL = `<svg viewBox="0 0 12 12" width="11" height="11"><path d="M6 1.2c-2 0-3.2 1.5-3.2 3.4 0 2-.6 3-1.3 3.6h9c-.7-.6-1.3-1.600-1.300-3.600 0-1.900-1.200-3.400-3.200-3.400z M4.800 9.300a1.250 1.250 0 0 0 2.400 0" fill="currentColor"/></svg>`;

// ----- chat: with one other participant, outside the issues -----------------------------------
// Opened from their badge in the header. Messages are the service's; what
// has been read is this browser's (a bell on a badge: something unread from them).

const chatRead = stored0("topo.chat.read", {});
function stored0(k, d) { try { return JSON.parse(localStorage.getItem(k) ?? "null") ?? d; } catch (_) { return d; } }
const chatWith = (peer) => (st?.chat || []).filter((m) => (m.from === peer && m.to === service.id) || (m.from === service.id && m.to === peer));
const unreadFrom = (peer) => (st?.chat || []).filter((m) => m.from === peer && m.to === service.id && m.time > (chatRead[peer] ?? 0)).length;

function openChat(peer) {
  sel.chat = sel.chat === peer ? null : peer;
  renderChat(true);
  renderPeople();
}

function renderChat(focus) {
  let box = $("chatPanel");
  const peer = sel.chat && st?.participants.find((p) => p.id === sel.chat);
  if (!peer) { box?.remove(); return; }
  if (!box) {
    box = el("div", { id: "chatPanel" }, `<div class="chead"><span class="cwho"></span><button class="cx" title="Close">×</button></div><div class="cnote muted"></div><div class="clist"></div><textarea rows="2"></textarea>`);
    document.body.appendChild(box);
    box.querySelector(".cx").onclick = () => { sel.chat = null; renderChat(); };
    const ta = box.querySelector("textarea");
    ta.onkeydown = async (e) => {
      e.stopPropagation();
      if (e.key === "Escape") { sel.chat = null; return renderChat(); }
      if (e.key !== "Enter" || e.shiftKey) return;
      e.preventDefault();
      const text = ta.value.trim();
      if (!text) return;
      ta.value = "";
      const r = await service.call("chat", { action: "send", to: sel.chat, text });
      if (r.error) { alert(r.error); ta.value = text; }
    };
  }
  const agent = peer.kind === "agent";
  box.querySelector(".cwho").innerHTML = `${badge(peer.id)} <b>${esc(peer.name)}</b> <span class="muted">${agent ? "agent · " : ""}${PRESENCE[peer.presence]}</span>`;
  box.querySelector(".cnote").textContent = agent && peer.presence === "away" ? `${peer.name} is not listening just now: it gets this when it next is.` : !agent && peer.presence === "away" ? `${peer.name} is away: they see this when they are back.` : "";
  box.querySelector("textarea").placeholder = `Say something to ${peer.name}… (Enter sends)`;
  const list = box.querySelector(".clist"), msgs = chatWith(peer.id), key = `${peer.id}:${msgs.length}`;
  if (list.dataset.key !== key) {
    list.dataset.key = key;
    list.innerHTML = msgs.map((m) => `<div class="cmsg ${m.from === service.id ? "mine" : "theirs"}"><div class="text">${refs(m.text)}</div><span class="muted">${when(m.time)}</span></div>`).join("") ||
      `<div class="muted" style="padding:6px 2px">Nothing said yet. ${agent ? "Ask it something, or what it is doing. For something about a place in the building, raise an issue: it will see that too." : ""}</div>`;
    list.scrollTop = list.scrollHeight;
    bindRefs(list);
  }
  // Seen, now that it is open.
  const last = msgs.filter((m) => m.from === peer.id).reduce((a, m) => Math.max(a, m.time), 0);
  if (last > (chatRead[peer.id] ?? 0)) { chatRead[peer.id] = last; try { localStorage.setItem("topo.chat.read", JSON.stringify(chatRead)); } catch (_) {} }
  if (focus) box.querySelector("textarea").focus();
}

/** Makes what a text refers to (as `refs` marked it) go there when clicked. */
function bindRefs(box) {
  const on = (q, f) => { for (const a of box.querySelectorAll(q)) a.onclick = (e) => { e.preventDefault(); f(a.dataset); }; };
  on("a[data-member]", (d) => { choose("member", d.member); gotoMembers(d.member); });
  on("a[data-issue]", (d) => openIssue(Number(d.issue)));
  on("a[data-selref]", (d) => gotoSelection(d.selref));
  on("a[data-planeref]", (d) => gotoPlane(d.planeref));
  on("a[data-objref]", (d) => gotoObject(...d.objref.split(":")));
}
const PRESENCE = { here: "here", working: "working", waiting: "listening", away: "away" };
function renderPeople() {
  const box = $("people");
  box.innerHTML = st.participants.map((p) => {
    const me = p.id === service.id, n = me ? 0 : unreadFrom(p.id);
    return `<span class="person ${p.presence}${me ? " me" : ""}${sel.chat === p.id ? " chatting" : ""}" data-id="${esc(p.id)}" title="${esc(p.name)} — ${p.kind === "agent" ? "agent, " : ""}${PRESENCE[p.presence]}${me ? " (you: click to change your name)" : `. Click to chat${n ? ` (${n} unread)` : ""}`}">${badge(p.id)}<i></i>${n ? `<b class="bell" title="${n} unread">${BELL}</b>` : ""}</span>`;
  }).join("");
  for (const s of box.querySelectorAll(".person:not(.me)")) s.onclick = () => openChat(s.dataset.id);
  const me = box.querySelector(".me");
  if (me) me.onclick = async () => {
    const name = prompt("Your name, as others see it:", service.me()?.name ?? "");
    if (name && name.trim()) await service.call("rename", { name: name.trim() });
  };
}

// ----- view state, links and history -----------------------------------------------

// ----- slabs and polylines ----------------------------------------------------------
// A slab is an oriented box in the model's frame ({ centre, axes: [right, up,
// toward the viewer], half }); a view clipped to one is a section. A polyline
// is a run of points. Both are kept by the service; the ones here that are
// not (`temp`) come from section links and can be kept.

const slabs = () => [...(st?.slabs || []), ...temps].map((s) => (live?.kind === "slab" && live.id === s.id ? live.value : s));
const slabOf = (id) => (id && slabs().find((s) => s.id === id)) || null;
const lines = () => (st?.polylines || []).map((l) => (live?.kind === "polyline" && live.id === l.id ? live.value : l));
const lineOf = (id) => (id && lines().find((l) => l.id === id)) || null;
const slabName = (s) => (s.temp ? `${s.id} (not kept)` : `${s.id}${s.name ? " " + s.name : ""}`);
const lineName = (l) => `${l.id}${l.name ? " " + l.name : ""}`;
const lineLength = (l) => l.points.reduce((a, p, k) => (k + 1 < l.points.length || l.closed ? a + Math.hypot(...vec.sub(l.points[(k + 1) % l.points.length], p)) : a), 0);
/** A slab as a view clips to it: scan frame. */
const boxOf = (s) => ({ c: toScan(s.centre), u: rot(s.axes[0]), v: rot(s.axes[1]), n: rot(s.axes[2]), h: s.half });
const slabCorners = (s) => [0, 1, 2, 3, 4, 5, 6, 7].map((k) => toScan([0, 1, 2].reduce((p, a) => vec.add(p, vec.scale(s.axes[a], (k >> a) & 1 ? s.half[a] : -s.half[a])), s.centre)));
const until = async (f, ms = 2500) => { for (const t0 = performance.now(); performance.now() - t0 < ms; ) { const x = f(); if (x) return x; await new Promise((r) => setTimeout(r, 50)); } return f(); };

/** A slab's axes from the direction it is seen along (toward the viewer), as the service makes them: upright, and square to the model when within 5° of it. */
function axesFromNormal(n, up) {
  n = vec.unit(n);
  const k = [0, 1, 2].find((a) => Math.abs(n[a]) > 0.9962);
  if (k != null) n = [0, 1, 2].map((a) => (a === k ? Math.sign(n[k]) : 0));
  up ??= Math.abs(n[2]) > 0.9 ? [0, 1, 0] : [0, 0, 1];
  const v = vec.unit(vec.sub(up, vec.scale(n, vec.dot(up, n))));
  return [vec.cross(v, n), v, n];
}

/** A slab through `origin` with these axes, reaching over the model (and a little more) across and up. */
function slabOver(origin, axes, thickness) {
  const b = modelBounds() ?? { lo: vec.sub(origin, [4, 4, 1.5]), hi: vec.add(origin, [4, 4, 1.5]) };
  let [u0, u1, v0, v1] = [Infinity, -Infinity, Infinity, -Infinity];
  for (const x of [b.lo[0], b.hi[0]]) for (const y of [b.lo[1], b.hi[1]]) for (const z of [b.lo[2], b.hi[2]]) {
    const q = vec.sub([x, y, z], origin);
    u0 = Math.min(u0, vec.dot(q, axes[0])); u1 = Math.max(u1, vec.dot(q, axes[0])); v0 = Math.min(v0, vec.dot(q, axes[1])); v1 = Math.max(v1, vec.dot(q, axes[1]));
  }
  return { centre: vec.add(origin, vec.add(vec.scale(axes[0], 0.5 * (u0 + u1)), vec.scale(axes[1], 0.5 * (v0 + v1)))), axes, half: [0.5 * (u1 - u0) + 0.4, 0.5 * (v1 - v0) + 0.4, 0.5 * thickness] };
}

/** A slab from a link's section (as `render_section` takes it, model frame). */
function slabFromSpec(spec) {
  // (Origins as `render_section` has them: its `bounds` are measured from them.)
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
  const axes = [vec.cross(v, n), v, n], b = spec.bounds;
  if (!b) return slabOver(origin, axes, t);
  return { centre: vec.add(origin, vec.add(vec.scale(axes[0], 0.5 * (b[0] + b[1])), vec.scale(v, 0.5 * (b[2] + b[3])))), axes, half: [0.5 * (b[1] - b[0]), 0.5 * (b[3] - b[2]), 0.5 * t] };
}

/** A slab that is not kept (until asked): the same one again for the same place. */
function tempSlab(geom) {
  const same = temps.find((t) => JSON.stringify([t.centre, t.axes, t.half]) === JSON.stringify([geom.centre, geom.axes, geom.half]));
  if (same) return same;
  const s = { id: `t${++tempN}`, name: "", temp: true, by: service.id, ...geom };
  temps.push(s);
  // A few are enough: the oldest that no view is clipped to goes.
  while (temps.length > 4) { const k = temps.findIndex((t) => !Object.values(views).some((v) => v.slab === t.id)); if (k < 0) break; temps.splice(k, 1); }
  return s;
}

/** Keeps a slab with the project; its id. */
async function createSlab(args) {
  const r = await service.call("slabs", { action: "create", frame: "model", ...args });
  if (r.error) { alert(r.error); return null; }
  const id = r.text.match(/made slab (b\d+)/)?.[1];
  if (!id) return null;
  hidden.delete(`slab:${id}`); hidden.delete("slabs"); keep();
  await until(() => st?.slabs?.some((s) => s.id === id));
  return id;
}

/** Changes a slab (its centre, half sizes, axes or name): at once here, then with the service. */
async function updateSlab(id, patch) {
  const s = slabOf(id);
  if (!s) return;
  if (s.temp) Object.assign(temps.find((t) => t.id === id), patch);
  else live = { kind: "slab", id, value: { ...s, ...patch } };
  syncSlabs(); rebuildMarks(); renderChips(); renderLayers();
  if (s.temp) return;
  const mine = live, r = await service.call("slabs", { action: "update", id, ...patch });
  if (r.error) alert(r.error);
  // (Let go of once the service's state says the same.)
  if (live === mine) live.done = true;
}

/** Makes every view's clipping box what its slab is now, and asks for the points in it. */
function syncSlabs() {
  let changed = false;
  for (const v of Object.values(views)) {
    const s = slabOf(v.slab);
    if (v.slab && !s) v.slab = null;
    const box = s ? boxOf(s) : null;
    // (Without a slab there is no depth in it to colour by.)
    if (JSON.stringify(box) !== JSON.stringify(v.state.box)) changed = true;
    if (JSON.stringify(box) !== JSON.stringify(v.state.box)) v.set(box ? { box } : { box: null, flat: false, colour: v.state.colour === "depth" ? "intensity" : v.state.colour }, true);
    requestSlab(v);
  }
  // (The members are drawn as the slab cuts them.)
  if (changed) rebuildModel();
}

/**
 * Shows a slab: a 3D view clipped to it, seen square on. The view already on
 * it, else a second view not in use, else the second view that is open.
 */
function viewSlab(id, kind) {
  const s = slabOf(id);
  if (!s) return null;
  kind ??= KINDS.find((k) => views[k].slab === id && app.shown(k)) ?? (app.slots().length < 3 ? SECOND.find((k) => !app.shown(k)) : null) ?? SECOND.find((k) => app.shown(k)) ?? "view2";
  if (!app.shown(kind)) app.slots().length < 3 ? app.openBeside(kind) : app.showView(kind);
  const v = views[kind], fresh = !v.state.box;
  active = kind;
  v.slab = id;
  syncSlabs();
  // What stands in front of what is the first thing to see in one.
  if (fresh && v.state.colour === "intensity") v.set({ colour: "depth" }, true);
  v.squareOn(boxOf(s));
  renderLayers(); rebuildMarks();
  return v;
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

function snapshot() {
  return { slots: app.slots(), views: Object.fromEntries(KINDS.map((k) => [k, { state: views[k].get(), slab: views[k].slab }])), proposal: sel.proposal && { ...sel.proposal } };
}
function restore(s) {
  for (const k of KINDS) { views[k].slab = s.views[k].slab; views[k].animate(s.views[k].state); }
  app.setSlots(s.slots);
  selectProposal(s.proposal, true);
  syncSlabs(); renderLayers(); rebuildMarks();
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
  // A section is a view clipped to a slab: the one named, or one made (not kept) from what the link gives.
  const slab = slabOf(link.slab) ?? (link.section ? tempSlab(slabFromSpec(link.section)) : null);
  let v;
  if (slab) v = viewSlab(slab.id);
  else {
    v = views.scan3d;
    // Beside what is open, if there is one pane; else where it is, or in the focused pane.
    if (!app.shown("scan3d") && app.slots().length < 2) app.openBeside("scan3d");
    else app.showView("scan3d");
    active = "scan3d";
    if (link.slab === null) { v.slab = null; syncSlabs(); }
  }
  const patch = {};
  if (link.flat != null) patch.flat = !!link.flat;
  if (link.free != null) patch.free = !!link.free;
  if (link.camera) {
    // (A camera of its own: no longer held square on, unless the link says so.)
    if (slab && link.flat == null && (link.camera.eye || link.camera.top_down)) patch.flat = false;
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
  renderLayers(); rebuildMarks();
}

/** The focused scan view as a link: what someone else needs to see the same. */
function linkOfView() {
  const v = views[active], s = v.state, slab = slabOf(v.slab), cam = v.camera();
  const link = { pane: slab ? "section" : "3d", colour: s.colour, clip: { z_min: s.zMin, z_max: s.zMax }, slab: slab && !slab.temp ? slab.id : null };
  if (slab) {
    // The slab as a section too, for whoever does not have it (an agent's `render_section` takes the same).
    link.section = { view: "custom", origin: round(slab.centre), normal: round(slab.axes[2]), up: round(slab.axes[1]), thickness: Math.round(2000 * slab.half[2]) / 1000, bounds: round([-slab.half[0], slab.half[0], -slab.half[1], slab.half[1]], 100) };
    if (slab.temp) delete link.slab;
    link.flat = s.flat;
    if (s.free) link.free = true;
  }
  link.camera = { eye: round(toModel(s.ortho ? vec.add(s.target, vec.scale(cam.forward, -s.dist)) : cam.eye), 100), target: round(toModel(s.target), 100) };
  link.projection = s.ortho ? "ortho" : "perspective";
  if (s.ortho) link.camera.half = Math.round(s.half * 100) / 100;
  if (sel.proposal) link.proposal = { issue: sel.proposal.issue, n: sel.proposal.n, mode: sel.proposal.mode };
  return link;
}
function describeView() {
  const v = views[active], s = v.state, slab = slabOf(v.slab);
  return `${slab ? `slab ${slabName(slab)}` : s.ortho ? "3D ortho" : "3D"}, ${s.colour}`;
}

// ----- data: the cloud, the members, a section's slab --------------------------------

/** The thinned scan, as the service has it now (`key`: which points it is of); again when that changes. */
async function loadCloud(key) {
  cloudLoaded = key;
  const c = await service.cloud();
  if (!c) { cloudLoaded = false; return; }
  for (const v of Object.values(views)) { v.setCloud("thin", c); v.slabWant = null; requestSlab(v); }
  // (Selections' points are found again in it.)
  selClouds.clear(); selRays.clear();
  for (const v of Object.values(views)) for (const k of [...v.highlights.keys()]) v.setHighlight(k, null);
  syncHighlights();
}

/** How near the sensor a scan point may be: asked for, kept with the project; the scan is then loaded again. */
async function askMinRange() {
  const now = +(st.scan.min_range ?? 1).toFixed(2);
  const t = prompt(`Leave out scan points nearer the sensor than (metres):\n\nThis removes whoever carried the scanner. It is kept with the project, for everyone; the scan is then loaded again, which takes a little while.`, String(now));
  if (t == null || t.trim() === "" || Number(t) === now) return;
  if (!(Number(t) >= 0)) return alert("A distance in metres, 0 or more.");
  const r = await service.call("scan_filter", { min_range: Number(t) });
  if (r.error) alert(r.error);
}

/** Every scan point in the box a view is clipped to (a moment after it stops changing). */
function requestSlab(v) {
  requestFree(v);
  const box = v.state.box, key = box ? JSON.stringify(box) : "";
  if (key === (v.slabWant ?? "") || !st?.scan.loaded) return;
  v.slabWant = key;
  clearTimeout(v.slabTimer);
  v.slabTimer = setTimeout(async () => {
    if (!box) { v.setCloud("slab", null); return setInfo(v.name, ""); }
    const c = await service.slab(box);
    if (v.slabWant !== key) return;
    v.setCloud("slab", c);
    const s = slabOf(v.slab);
    setInfo(v.name, c ? `${s ? `slab ${slabName(s)} · ` : ""}${c.count.toLocaleString()} points` : "");
  }, 200);
}

/**
 * Free space in the slab a view is clipped to (when it is switched on): where the rays pass in
 * it, as a picture over its middle plane. Made for the part of the slab the view sees, with a
 * cell for each of its pixels, and again when the view or the slab changes: zooming in sharpens it.
 */
function requestFree(v) {
  const box = v.state.box;
  let want = null;
  if (box && v.state.free && st?.scan.loaded && st.scan.rays) {
    // What of the slab's middle plane the view sees: where the rays through its corners meet it.
    const W = v.canvas.clientWidth || 1, H = v.canvas.clientHeight || 1, dpr = window.devicePixelRatio || 1;
    let lo = [-box.h[0], -box.h[1]], hi = [box.h[0], box.h[1]];
    const pts = [[0, 0], [W, 0], [0, H], [W, H]].map(([x, y]) => onPlane(box.c, box.n, v.ray(x, y)));
    // (Not for a view along the plane, or one that looks past it to the horizon: the whole slab then.)
    if (pts.every(Boolean) && v.state.ortho) {
      const uv = pts.map((p) => { const q = vec.sub(p, box.c); return [vec.dot(q, box.u), vec.dot(q, box.v)]; });
      lo = lo.map((x, k) => Math.max(x, Math.min(...uv.map((p) => p[k]))));
      hi = hi.map((x, k) => Math.min(x, Math.max(...uv.map((p) => p[k]))));
    }
    if (hi[0] - lo[0] > 1e-3 && hi[1] - lo[1] > 1e-3) {
      // A cell a pixel: as fine as the view can show, no finer than 1 mm.
      const cam = v.camera(), perPx = v.state.ortho ? (2 * cam.hh) / (H * dpr) : Math.max(hi[0] - lo[0], hi[1] - lo[1]) / (Math.max(W, H) * dpr);
      const cell = Math.max(perPx, 0.001, Math.max(hi[0] - lo[0], hi[1] - lo[1]) / 2048);
      // (Snapped to cells, so a small move asks for the same picture.)
      const snap = (x, up) => (up ? Math.ceil : Math.floor)(x / (cell * 16)) * cell * 16;
      lo = lo.map((x, k) => Math.max(-box.h[k], snap(x, false))); hi = hi.map((x, k) => Math.min(box.h[k], snap(x, true)));
      const mid = [0.5 * (lo[0] + hi[0]), 0.5 * (lo[1] + hi[1])], half = [0.5 * (hi[0] - lo[0]), 0.5 * (hi[1] - lo[1])];
      want = { box: { ...box, c: vec.add(box.c, vec.add(vec.scale(box.u, mid[0]), vec.scale(box.v, mid[1]))), h: [half[0], half[1], box.h[2]] }, w: Math.max(1, Math.round((2 * half[0]) / cell)), h: Math.max(1, Math.round((2 * half[1]) / cell)), cell };
    }
  }
  const key = want ? JSON.stringify([want.box, want.w, want.h]) : "";
  if (key === (v.freeWant ?? "")) return;
  v.freeWant = key;
  clearTimeout(v.freeTimer);
  if (!want) return v.setImage(null);
  v.freeTimer = setTimeout(async () => {
    const f = await service.free(want.box, want.w, want.h);
    if (v.freeWant !== key) return;
    const b = want.box, at = (a, c) => vec.add(b.c, vec.add(vec.scale(b.u, a * b.h[0]), vec.scale(b.v, c * b.h[1])));
    v.setImage(f && f.max ? { ...f, corners: [at(-1, -1), at(1, -1), at(-1, 1), at(1, 1)] } : null);
    v.freeInfo = f ? ` Now in cells of ${want.cell >= 0.01 ? `${(want.cell * 100).toFixed(1)} cm` : `${(want.cell * 1000).toFixed(1)} mm`}, up to ${f.max.toLocaleString()} rays over one.` : "";
    renderTools(v);
  }, 150);
}

async function loadMembers() {
  const key = `${st.model.rev}|${JSON.stringify(st.placement)}|${st.scan.used}`;
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
      for (const v of Object.values(views)) if (!v.slab) v.set({ target: toScan(b.centre), dist: b.radius * 2.6, half: b.radius * 1.1 }, true);
    }
  }
  if (sel.proposal) await loadDiff();
  freshPlaces();
  rebuildModel(); rebuildMarks();
}

/// An issue pinned to a member is where the member is now: its box was taken when it was raised,
/// and the member may have moved since (a proposal applied), which would leave the box behind.
function freshPlaces() {
  for (const i of st?.issues || []) {
    const m = i.place?.member && members?.find((x) => x.path === i.place.member);
    if (!m) continue;
    const lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity];
    for (const f of m.faces) for (const p of f) { const q = toScan(p); for (let k = 0; k < 3; k++) { lo[k] = Math.min(lo[k], q[k] - 0.02); hi[k] = Math.max(hi[k], q[k] + 0.02); } }
    if (lo[0] !== Infinity) { i.place.min = lo; i.place.max = hi; }
  }
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
  // What changes is filled strongly in its colour (as it was: red; as it would be: green), in every mode,
  // so switching between before and after shows what moves.
  const was = (m) => ({ m, rgb: CHANGE.from, fill: 130, line: 255, change: true }), will = (m) => ({ m, rgb: CHANGE.added, fill: 130, line: 255, change: true });
  const goes = (m) => ch[m.path] === "removed" || ch[m.path] === "moved", comes = (m) => ch[m.path] === "added" || ch[m.path] === "moved";
  if (mode === "before") return diff.before.map((m) => (goes(m) ? was(m) : plain(m)));
  if (mode === "after") return diff.after.map((m) => (comes(m) ? will(m) : plain(m)));
  const out = [];
  const faint = (m) => ({ m, rgb: [150, 155, 165], fill: 0, line: 60 });
  for (const m of diff.before) out.push(goes(m) ? was(m) : mode === "both" ? plain(m) : faint(m));
  for (const m of diff.after) if (comes(m)) out.push(will(m));
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
  const build = (neutral, box) => {
    // (Clipped to a slab: room too for each member's cut by the slab's middle plane, at most a hexagon.)
    const extra = box ? list.length * 6 : 0;
    const tris = new Float32Array((nt + extra * 3) * 3), tcol = new Uint8Array((nt + extra * 3) * 4), lines = new Float32Array((nl + extra * 2) * 3), lcol = new Uint8Array((nl + extra * 2) * 4);
    let ti = 0, li = 0;
    const put = (arr, col, k, p, rgb, a) => { arr.set(p, k * 3); col.set([rgb[0], rgb[1], rgb[2], a], k * 4); };
    // Where the slab's middle plane cuts a member: its outline there, filled. Seen square on, a member's own
    // faces are edge-on or outside the slab, so this is what shows it (a stud in a plan, a plate in a section).
    const cut = (faces, c, a, lineA) => {
      const pts = [];
      for (const q of faces) for (let i = 0; i < q.length; i++) {
        const p0 = q[i], p1 = q[(i + 1) % q.length], d0 = vec.dot(box.n, vec.sub(p0, box.c)), d1 = vec.dot(box.n, vec.sub(p1, box.c));
        if ((d0 < 0) !== (d1 < 0)) pts.push(vec.add(p0, vec.scale(vec.sub(p1, p0), d0 / (d0 - d1))));
      }
      if (pts.length < 3) return;
      // In order round their middle (the cut of a box is convex); each corner comes from two faces: once is enough.
      const mid = vec.scale(pts.reduce((s, p) => vec.add(s, p), [0, 0, 0]), 1 / pts.length);
      const ang = (p) => { const r = vec.sub(p, mid); return Math.atan2(vec.dot(r, box.v), vec.dot(r, box.u)); };
      const ring = pts.map((p) => [ang(p), p]).sort((x, y) => x[0] - y[0]).map((x) => x[1]).filter((p, i, all) => !i || Math.hypot(...vec.sub(p, all[i - 1])) > 1e-4).slice(0, 6);
      if (ring.length < 3) return;
      for (let i = 1; i + 1 < ring.length; i++) for (const p of [ring[0], ring[i], ring[i + 1]]) put(tris, tcol, ti++, p, c, Math.max(a, 60));
      for (let i = 0; i < ring.length; i++) { put(lines, lcol, li++, ring[i], c, lineA); put(lines, lcol, li++, ring[(i + 1) % ring.length], c, lineA); }
    };
    for (const { m, rgb, fill, line, change } of list) {
      // A proposal's changes and highlights keep their colours.
      const plain = neutral && !change;
      const [c, a] = plain ? [[255, 255, 255], 0] : [rgb, fill];
      const qs = m.faces.map((f) => f.map(toScan));
      for (const q of qs) {
        if (a) for (let i = 1; i + 1 < q.length; i++) for (const p of [q[0], q[i], q[i + 1]]) put(tris, tcol, ti++, p, c, a);
        for (let i = 0; i < q.length; i++) { put(lines, lcol, li++, q[i], c, plain ? Math.min(line, 200) : line); put(lines, lcol, li++, q[(i + 1) % q.length], c, plain ? Math.min(line, 200) : line); }
      }
      if (box) cut(qs, c, plain ? 0 : a, 255);
    }
    return [tris.subarray(0, ti * 3), tcol.subarray(0, ti * 4), lines.subarray(0, li * 3), lcol.subarray(0, li * 4)];
  };
  const made = {};
  for (const v of Object.values(views)) {
    const k = neutralModel(v), box = v.state.box;
    v.modelNeutral = k;
    v.modelBox = box ? JSON.stringify(box) : "";
    // (A view clipped to a slab has its own: the members as the slab cuts them.)
    v.setModel(...(box ? build(k, box) : (made[k] ??= build(k))));
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
  if (issue?.place && vis("issues", `issue:${issue.id}`)) boxLines(aabb(issue.place.min, issue.place.max), [255, 255, 255], segs);
  // The pick: a small cross.
  if ((sel.pick ?? sel.at)?.hit) {
    const p = (sel.pick ?? sel.at).hit, r = 0.08;
    for (const a of [[r, 0, 0], [0, r, 0], [0, 0, r]]) segs.push([vec.sub(p, a), vec.add(p, a), [255, 220, 40]]);
  }
  // Rooms (model frame boxes) and fitted surfaces (patches).
  if (vis("rooms")) for (const r of st.rooms || []) boxLines(aabb(r.min, r.max).map(toScan), [120, 170, 255], segs);
  for (const s of (st.surfaces || []).filter((s) => vis("planes", `plane:${s.id}`))) {
    const at = (a, b) => toScan(vec.add(s.centre, vec.add(vec.scale(s.u, a), vec.scale(s.v, b))));
    const q = [at(s.ext[0], s.ext[2]), at(s.ext[1], s.ext[2]), at(s.ext[1], s.ext[3]), at(s.ext[0], s.ext[3])];
    for (let i = 0; i < 4; i++) segs.push([q[i], q[(i + 1) % 4], s.id === sel.plane ? [200, 255, 250] : [40, 210, 200]]);
    if (s.id === sel.plane) { segs.push([q[0], q[2], [200, 255, 250]]); segs.push([q[1], q[3], [200, 255, 250]]); }
  }
  // Slabs: their boxes. The one selected has faint faces too, so it is seen where it
  // meets the points (stronger while it is dragged); not in a view clipped to it, which is inside it.
  const fills = [];
  const quad = (q, rgba, skip) => fills.push([q[0], q[1], q[2], rgba, skip], [q[0], q[2], q[3], rgba, skip]);
  for (const s of slabs()) {
    if (!vis("slabs", `slab:${s.id}`)) continue;
    const on = s.id === sel.slab, c = slabCorners(s);
    boxLines(c, on ? [255, 205, 120] : [205, 120, 30], segs);
    if (on) for (const f of [[0, 1, 3, 2], [4, 5, 7, 6], [0, 1, 5, 4], [2, 3, 7, 6], [0, 2, 6, 4], [1, 3, 7, 5]]) quad(f.map((k) => c[k]), [255, 160, 50, grab?.s ? 54 : 26], s.id);
  }
  // The selected selection, if it is of points seen through a member: their rays from the sensor.
  const thr = theSelection();
  if (thr?.how === "through" && vis("selections", `sel:${thr.id}`)) {
    if (!selRays.has(thr.id)) { selRays.set(thr.id, []); service.selectionRays(thr.id).then((r) => { selRays.set(thr.id, r); rebuildMarks(); }); }
    for (const [a, b] of selRays.get(thr.id)) segs.push([a, b, [255, 110, 80]]);
  }
  const pn = thePlane();
  if (pn && vis("planes", `plane:${pn.id}`)) quad(planeCorners(pn), [60, 230, 215, 56]);
  // A face being dragged: where it would be.
  if (faceDrag?.moved) {
    const m = members?.find((x) => x.path === faceDrag.path), f = m?.faces[faceDrag.k];
    if (f) {
      const d = vec.scale(faceDrag.n, faceDrag.t), q = f.map((p) => toScan(vec.add(p, d)));
      for (let i = 0; i < q.length; i++) segs.push([q[i], q[(i + 1) % q.length], [255, 60, 220]]);
      segs.push([toScan(faceDrag.c), toScan(vec.add(faceDrag.c, d)), [255, 60, 220]]);
    }
  }
  // Polylines, and the one being drawn.
  const run = (pts, closed, rgb) => { for (let k = 0; k + 1 < pts.length || (closed && k < pts.length && pts.length > 2); k++) segs.push([toScan(pts[k]), toScan(pts[(k + 1) % pts.length]), rgb]); };
  for (const l of lines()) if (vis("polylines", `line:${l.id}`)) run(l.points, l.closed, l.id === sel.polyline ? [240, 255, 170] : [180, 235, 60]);
  if (drawing) {
    run(drawing.points, false, [240, 255, 170]);
    for (const p of drawing.points) { const q = toScan(p), r = 0.03; for (const a of [[r, 0, 0], [0, r, 0], [0, 0, r]]) segs.push([vec.sub(q, a), vec.add(q, a), [240, 255, 170]]); }
  }
  const pos = new Float32Array(segs.length * 6), col = new Uint8Array(segs.length * 8);
  segs.forEach(([a, b, rgb], i) => { pos.set(a, i * 6); pos.set(b, i * 6 + 3); col.set([...rgb, 255, ...rgb, 255], i * 8); });
  for (const v of Object.values(views)) {
    v.setMarks(pos, col);
    const t = fills.filter((f) => f[4] == null || f[4] !== v.slab);
    const fp = new Float32Array(t.length * 9), fc = new Uint8Array(t.length * 12);
    t.forEach(([a, b, c, rgba], i) => { fp.set(a, i * 9); fp.set(b, i * 9 + 3); fp.set(c, i * 9 + 6); fc.set([...rgba, ...rgba, ...rgba], i * 12); });
    v.setFills(fp, fc);
  }
  rebuildHandles();
  // Issues as pins.
  const PRI = { 1: "#d62828", 2: "#1f5fbf", 3: "#6b6b66" };
  const pins = st.issues.filter((i) => i.place && vis("issues", `issue:${i.id}`) && (i.status !== "done" || i.id === sel.issue)).map((i) => ({
    id: i.id, pos: i.place.min.map((x, k) => 0.5 * (x + i.place.max[k])), text: `#${i.id} ${i.title.length > 26 ? i.title.slice(0, 25) + "…" : i.title}`,
    title: `${i.title} — ${STATUS[i.status]}`, colour: PRI[i.priority], on: i.id === sel.issue, dim: i.status === "done",
  }));
  for (const v of Object.values(views)) v.setLabels(pins);
}

/**
 * The member under the cursor: the shown member a ray (scan frame) meets
 * first, by its faces as they are drawn in the view (so not where the
 * view's cuts hide them). { path, t (how far along the ray) }, or null.
 */
function memberAt(ray, v) {
  const o = toModel(ray.origin), d = unrot(ray.dir);
  const s = v?.state, box = s?.box;
  const cut = (p) => {
    if (s && ((s.zMin != null && p[2] < s.zMin) || (s.zMax != null && p[2] > s.zMax))) return true;
    if (!box) return false;
    const q = vec.sub(toScan(p), box.c);
    return [box.u, box.v, box.n].some((a, k) => Math.abs(vec.dot(a, q)) > box.h[k]);
  };
  let best = null, bt = Infinity;
  for (const m of members || []) {
    if (!memberVisible(m.path)) continue;
    for (const f of m.faces) {
      if (f.length < 3) continue;
      const n = vec.cross(vec.sub(f[1], f[0]), vec.sub(f[2], f[0])), k = vec.dot(n, d);
      if (Math.abs(k) < 1e-9) continue;
      const t = vec.dot(n, vec.sub(f[0], o)) / k;
      if (t <= 0 || t >= bt) continue;
      const p = vec.add(o, vec.scale(d, t));
      // Inside the face: on the same side of every edge.
      let pos = 0, neg = 0;
      for (let i = 0; i < f.length; i++) { const s = vec.dot(vec.cross(vec.sub(f[(i + 1) % f.length], f[i]), vec.sub(p, f[i])), n); s >= 0 ? pos++ : neg++; }
      if ((pos && neg) || cut(p)) continue;
      bt = t; best = m.path;
    }
  }
  return best && { path: best, t: bt };
}

/** The slab, plane or polyline whose lines (as drawn: the shown ones) a click in a view is on, within a few pixels; the nearest. */
function objectAt(v, e) {
  const r = v.canvas.getBoundingClientRect(), x = e.clientX - r.left, y = e.clientY - r.top, cam = v.camera();
  let best = null;
  const near = (kind, id, a, b) => {
    const p = v.project(cam, a), q = v.project(cam, b);
    if (!p || !q) return;
    const dx = q[0] - p[0], dy = q[1] - p[1], t = Math.max(0, Math.min(1, ((x - p[0]) * dx + (y - p[1]) * dy) / (dx * dx + dy * dy || 1)));
    const d = Math.hypot(x - p[0] - t * dx, y - p[1] - t * dy);
    if (d < 7 && (!best || d < best.d)) best = { kind, id, d };
  };
  const loop = (kind, id, pts, closed) => { for (let k = 0; k + 1 < pts.length || (closed && k < pts.length && pts.length > 2); k++) near(kind, id, pts[k], pts[(k + 1) % pts.length]); };
  for (const s of slabs()) {
    if (!vis("slabs", `slab:${s.id}`)) continue;
    const c = slabCorners(s);
    for (const [a, b] of [[0, 1], [2, 3], [4, 5], [6, 7], [0, 2], [1, 3], [4, 6], [5, 7], [0, 4], [1, 5], [2, 6], [3, 7]]) near("slab", s.id, c[a], c[b]);
  }
  for (const s of st?.surfaces || []) if (vis("planes", `plane:${s.id}`)) loop("plane", s.id, planeCorners(s), true);
  for (const l of lines()) if (vis("polylines", `line:${l.id}`)) loop("polyline", l.id, l.points.map(toScan), l.closed);
  return best;
}

// ----- handles: the selected slab's faces, the selected polyline's points --------------

function rebuildHandles() {
  // (What is hidden has no handles either.)
  const list = [], s = vis("slabs", `slab:${sel.slab}`) && slabOf(sel.slab), l = vis("polylines", `line:${sel.polyline}`) && lineOf(sel.polyline);
  if (s) {
    const c = toScan(s.centre), ax = s.axes.map(rot), names = ["width", "height", "thickness"];
    for (let a = 0; a < 3; a++) for (const g of [-1, 1]) list.push({ id: `f:${a}:${g}`, pos: vec.add(c, vec.scale(ax[a], g * s.half[a])), along: ax[a], cls: a === 2 ? "face thin" : "face", title: `Drag this face in or out (the slab's ${names[a]})` });
    list.push({ id: "mid", pos: c, along: ax[2], cls: "mid", title: "Drag to slide the slab through the building, along its thin direction" });
    list.push({ id: "yaw", pos: vec.add(c, vec.scale(ax[0], s.half[0] + 0.45)), cls: "yaw", text: "⟳", title: "Drag to turn the slab about the vertical (in steps of 5° of the model; alt: freely)" });
  }
  if (l && !drawing) {
    const n = l.points.length;
    l.points.forEach((p, k) => list.push({ id: `n:${k}`, pos: toScan(p), cls: "node", title: "Drag to move this point (it lands on the scan under the cursor; alt: where you let go). Right-click: remove it" }));
    for (let k = 0; k + 1 < n || (l.closed && k < n && n > 2); k++) {
      const a = l.points[k], b = l.points[(k + 1) % n];
      list.push({ id: `m:${k}`, pos: toScan(a.map((x, i) => 0.5 * (x + b[i]))), cls: "seg", text: `${Math.hypot(...vec.sub(b, a)).toFixed(3)} m`, title: "This stretch's length. Drag to add a point here" });
    }
  }
  // The selected member's faces: each can be dragged along its normal to where the scan has it (a reading).
  const m = sel.member && !drawing && !sel.proposal && vis("model") && members?.find((x) => x.path === sel.member);
  if (m) {
    m.faces.forEach((f, k) => {
      const g = faceGeom(m, f);
      if (!g || g.area < 0.001) return;
      const d = faceDrag?.k === k && faceDrag.path === m.path ? faceDrag.t : 0;
      list.push({ id: `sf:${k}`, pos: toScan(vec.add(g.c, vec.scale(g.n, d))), along: rot(g.n), cls: "seen", text: d ? `${d > 0 ? "+" : ""}${(d * 1000).toFixed(1)} mm` : "", title: "Drag this face to where the scan has it: a reading that it is there (the solve moves what is free to meet it). A click on the member only selects it." });
    });
  }
  for (const v of Object.values(views)) v.setHandles(list);
}

/** A member's face (model frame): its middle, outward normal and area; null for a degenerate one. */
function faceGeom(m, f) {
  if (f.length < 3) return null;
  let n = [0, 0, 0];
  for (let i = 1; i + 1 < f.length; i++) n = vec.add(n, vec.cross(vec.sub(f[i], f[0]), vec.sub(f[i + 1], f[0])));
  const len = Math.hypot(...n);
  if (len < 1e-9) return null;
  n = vec.scale(n, 1 / len);
  const c = vec.scale(f.reduce((a, p) => vec.add(a, p), [0, 0, 0]), 1 / f.length);
  if (vec.dot(n, vec.sub(c, m.centre)) < 0) n = vec.scale(n, -1);
  return { c, n, area: len / 2 };
}

// ----- faces placed by hand: a face dragged to where the scan has it ---------------------

let faceDrag = null;  // { path, k, c, n (model frame), t (how far it has moved, m), t0, x, y, moved }
let lastSeen = null;  // the last face placed here: { n, path }
function faceDragged(v, k, phase, ray, e) {
  const m = members?.find((x) => x.path === sel.member), g = m && faceGeom(m, m.faces[k]);
  if (!g) return;
  const along = (r) => alongLine(toScan(g.c), rot(g.n), r);
  if (phase === "start") { faceDrag = { path: m.path, k, c: g.c, n: g.n, t: 0, t0: along(ray) ?? 0, x: e.clientX, y: e.clientY, moved: false }; return; }
  if (!faceDrag) return;
  // (Only a drag moves it: a few pixels first.)
  if (!faceDrag.moved && Math.hypot(e.clientX - faceDrag.x, e.clientY - faceDrag.y) > 3) faceDrag.moved = true;
  const t = along(ray);
  if (faceDrag.moved && t != null) { faceDrag.t = Math.round((t - faceDrag.t0) * 2000) / 2000; rebuildMarks(); }
  if (phase === "end") {
    const d = faceDrag;
    faceDrag = null;
    rebuildMarks();
    if (d.moved && Math.abs(d.t) >= 0.0005) placeFace(v, d);
  }
}

async function placeFace(v, d) {
  const point = vec.add(d.c, vec.scale(d.n, d.t));
  note(`Placing ${shortPath(d.path)}: its face ${d.t > 0 ? "out" : "in"} ${(Math.abs(d.t) * 1000).toFixed(1)} mm… (solving)`);
  const r = await service.call("seen", { action: "add", member: d.path, face: d.n.map((x) => Math.round(x * 1e4) / 1e4), point: round(point, 1e5), view: v.slab || undefined });
  if (r.error) return note(r.error, { error: true });
  const n = Number(r.text.match(/reading (\d+)/)?.[1]);
  lastSeen = n ? { n, path: d.path } : null;
  note(r.text, { undo: lastSeen, warn: r.text.includes("NOT MET") });
}

async function takeBack(n) {
  note(`Taking back reading ${n}… (solving)`);
  const r = await service.call("seen", { action: "remove", n });
  if (lastSeen?.n === n) lastSeen = null;
  note(r.error || r.text, { error: !!r.error });
}

/** A note over the views: what a face placed by hand did (with a way to take it back). */
function note(text, o = {}) {
  let box = $("seenNote");
  if (!box) { box = el("div", { id: "seenNote", class: "seennote" }); document.body.appendChild(box); }
  box.className = `seennote${o.error ? " error" : o.warn ? " warn" : ""}`;
  box.innerHTML = `<pre>${esc(text.trim())}</pre><div class="row">${o.undo ? `<button data-undo="${o.undo.n}">Take it back</button>` : ""}<button data-close>Close</button></div>`;
  box.hidden = false;
  box.querySelector("[data-close]").onclick = () => { box.hidden = true; };
  const u = box.querySelector("[data-undo]");
  if (u) u.onclick = () => takeBack(Number(u.dataset.undo));
}

/** How far along a line (through `p`, along `d`) the point nearest a ray is; null if they run together. */
function alongLine(p, d, ray) {
  const w = vec.sub(p, ray.origin), a = vec.dot(d, d), b = vec.dot(d, ray.dir), c = vec.dot(ray.dir, ray.dir), den = a * c - b * b;
  return Math.abs(den) < 1e-6 ? null : (b * vec.dot(ray.dir, w) - c * vec.dot(d, w)) / den;
}
/** Where a ray meets a plane (through `p`, normal `n`); null if it runs along it. */
function onPlane(p, n, ray) {
  const k = vec.dot(n, ray.dir);
  return Math.abs(k) < 0.02 ? null : vec.add(ray.origin, vec.scale(ray.dir, vec.dot(n, vec.sub(p, ray.origin)) / k));
}

let grab = null;  // what a handle's drag started from
function handleDrag(v, id, phase, ray, e) {
  if (id.startsWith("sf:")) return faceDragged(v, Number(id.slice(3)), phase, ray, e);
  const refresh = () => { syncSlabs(); rebuildMarks(); renderChips(); };
  if (sel.slab && !id.startsWith("n:") && !id.startsWith("m:")) {
    if (phase === "start") { grab = { s: structuredClone(slabOf(sel.slab)) }; return; }
    if (!grab?.s) return;
    const s0 = grab.s, c0 = toScan(s0.centre);
    let value = null;
    if (id.startsWith("f:")) {
      // A face moves along its normal; the one across from it stays where it is.
      const [, a, g] = id.split(":").map(Number), ax = rot(s0.axes[a]);
      const t = alongLine(vec.add(c0, vec.scale(ax, g * s0.half[a])), vec.scale(ax, g), ray);
      if (t != null) {
        const h = Math.max(0.01, s0.half[a] + 0.5 * t), half = s0.half.slice();
        half[a] = Math.round(h * 1000) / 1000;
        value = { ...s0, half, centre: vec.add(s0.centre, vec.scale(s0.axes[a], g * (half[a] - s0.half[a]))) };
      }
    } else if (id === "mid") {
      const t = alongLine(c0, rot(s0.axes[2]), ray);
      if (t != null) value = { ...s0, centre: vec.add(s0.centre, vec.scale(s0.axes[2], t)) };
    } else if (id === "yaw") {
      const p = onPlane(c0, [0, 0, 1], ray);
      if (p) {
        const q = toModel(p), u = s0.axes[0];
        let to = Math.atan2(q[1] - s0.centre[1], q[0] - s0.centre[0]);
        if (!e.altKey) to = Math.round(to / (Math.PI / 36)) * (Math.PI / 36);
        const d = to - Math.atan2(u[1], u[0]), cs = Math.cos(d), sn = Math.sin(d);
        value = { ...s0, axes: s0.axes.map((x) => [cs * x[0] - sn * x[1], sn * x[0] + cs * x[1], x[2]]) };
      }
    }
    if (value) { live = { kind: "slab", id: s0.id, value }; grab.moved = true; refresh(); }
    if (phase === "end") {
      const now = live?.id === s0.id ? live.value : null, moved = grab.moved;
      grab = null;
      if (moved && now) updateSlab(s0.id, { centre: now.centre, half: now.half, axes: now.axes });
    }
    return;
  }
  const l0 = phase === "start" ? lineOf(sel.polyline) : grab?.l;
  if (!l0) return;
  if (phase === "start") {
    const pts = l0.points.map((p) => p.slice());
    let k = Number(id.slice(2));
    // A stretch's middle: a new point there, which is then what is dragged.
    if (id.startsWith("m:")) { const a = pts[k], b = pts[(k + 1) % pts.length]; pts.splice(++k, 0, a.map((x, i) => 0.5 * (x + b[i]))); }
    grab = { l: { ...l0, points: pts }, k, at: toScan(pts[k]), added: id.startsWith("m:"), x: e.clientX, y: e.clientY };
    return;
  }
  // The point moves in the plane facing the viewer through where it was.
  const moved = grab.moved || Math.hypot(e.clientX - grab.x, e.clientY - grab.y) > 3;
  const p = moved && onPlane(grab.at, v.camera().forward, ray);
  if (p) {
    grab.moved = true;
    const pts = grab.l.points.map((x) => x.slice());
    pts[grab.k] = toModel(p);
    live = { kind: "polyline", id: grab.l.id, value: { ...grab.l, points: pts } };
    rebuildMarks(); renderChips();
  }
  if (phase === "end") {
    const g = grab;
    grab = null;
    if (!g.moved) { live = null; return rebuildMarks(); }
    (async () => {
      const pts = live.value.points.map((x) => x.slice());
      // Looking at the scan in the round, it lands on the surface under the cursor; square on to a slab, it stays in its plane.
      if (!v.state.flat && !e.altKey) {
        const cuts = v.cuts(), hit = (await service.pick(ray.origin, ray.dir, cuts.z, cuts.slab)).hit;
        if (hit) pts[g.k] = toModel(hit);
      }
      updateLine(g.l.id, { points: pts.map((x) => round(x)) });
    })();
  }
}

/** Changes a polyline (its points, whether it is closed, its name): at once here, then with the service. */
async function updateLine(id, patch) {
  const l = lineOf(id);
  if (!l) return;
  const mine = (live = { kind: "polyline", id, value: { ...l, ...patch } });
  rebuildMarks(); renderChips(); renderTree();
  const r = await service.call("polylines", { action: "update", id, frame: "model", ...patch });
  if (r.error) alert(r.error);
  if (live === mine) live.done = true;
}

function handleMenu(v, id, e) {
  const l = lineOf(sel.polyline);
  if (!l || !id.startsWith("n:")) return;
  const k = Number(id.slice(2)), items = [];
  if (l.points.length > 2) items.push({ label: "Remove this point", run: () => updateLine(l.id, { points: l.points.filter((_, i) => i !== k) }) });
  if (l.points.length > 2) items.push({ label: l.closed ? "Open the polyline" : "Close the polyline", run: () => updateLine(l.id, { closed: !l.closed }) });
  items.push({ label: "Orbit about this point", run: () => v.animate({ target: toScan(l.points[k]) }) });
  openMenu(items, e.clientX, e.clientY);
}

// ----- drawing: a polyline, point by point; a cut line for a slab ------------------------

function startDrawing(v) {
  choose(null);
  drawing = { view: v.name, points: [] };
  renderChips(); renderLayers(); rebuildMarks();
}
async function endDrawing(keepIt) {
  const d = drawing;
  drawing = null;
  if (!d) return;
  // (A double click ends it: its second click is the same point again.)
  const pts = d.points.filter((p, k) => k === 0 || Math.hypot(...vec.sub(p, d.points[k - 1])) > 0.01);
  renderChips(); renderLayers(); rebuildMarks();
  if (!keepIt || pts.length < 2) return;
  const r = await service.call("polylines", { action: "create", frame: "model", points: pts.map((p) => round(p)), closed: !!d.closed });
  if (r.error) return alert(r.error);
  const id = r.text.match(/made polyline (l\d+)/)?.[1];
  if (!id) return;
  hidden.delete(`line:${id}`); hidden.delete("polylines"); keep();
  await until(() => st?.polylines?.some((l) => l.id === id));
  choose("polyline", id);
}
/** A click while drawing: a point on the scan there, or (square on to a slab) in the slab's middle plane. */
async function drawPoint(v, ray, e) {
  let p = null;
  const box = v.state.box;
  if (v.state.flat && box) p = onPlane(box.c, box.n, ray);
  else { const cuts = v.cuts(); p = (await service.pick(ray.origin, ray.dir, cuts.z, cuts.slab)).hit ?? null; }
  if (!p || !drawing) return;
  // On the first point again: closed, and done.
  const first = drawing.points[0] && v.project(v.camera(), toScan(drawing.points[0])), r = v.canvas.getBoundingClientRect();
  if (drawing.points.length > 2 && first && Math.hypot(first[0] - (e.clientX - r.left), first[1] - (e.clientY - r.top)) < 10) { drawing.closed = true; return endDrawing(true); }
  drawing.points.push(toModel(p));
  renderChips(); rebuildMarks();
}

/** A line drawn across a view (looking down on it): an upright slab along it, to see what it cuts through. */
async function cutAlong(v, ra, rb) {
  v.tool = null;
  renderTools(v);
  const z = v.state.target[2], a = onPlane([0, 0, z], [0, 0, 1], ra), b = onPlane([0, 0, z], [0, 0, 1], rb);
  if (!a || !b) return setInfo(v.name, "Draw a cut line looking down on the building.");
  const [p, q] = [toModel(a), toModel(b)], d = [q[0] - p[0], q[1] - p[1]], len = Math.hypot(...d);
  if (len < 0.05) return;
  const axes = axesFromNormal([d[1] / len, -d[0] / len, 0]), mb = modelBounds(), zs = mb ? [mb.lo[2] - 0.3, mb.hi[2] + 0.3] : [toModel(a)[2] - 1.5, toModel(a)[2] + 1.5];
  const id = await createSlab({ centre: round([0.5 * (p[0] + q[0]), 0.5 * (p[1] + q[1]), 0.5 * (zs[0] + zs[1])]), axes, half: round([0.5 * len, 0.5 * (zs[1] - zs[0]), 0.1]) });
  if (id) { pushHistory(); choose("slab", id); viewSlab(id); }
}

/** A slab through a picked point `p`: a plan at its height, a cut across the model's x or y, along the surface there, or through it. */
/** The surface at a picked point: as the scan has it there, else the face of the member it lies on. */
const surfaceAt = (p) => p.normal ?? (p.member && p.member.distance < 0.06 ? p.member.normal : null);
async function slabAt(how, p, v) {
  const nrm = surfaceAt(p);
  let n = how === "plan" ? [0, 0, 1] : how === "x" ? [1, 0, 0] : how === "y" ? [0, -1, 0] : nrm;
  if (how === "along" && v) {
    // Seen from the side the viewer is on.
    if (vec.dot(n, unrot(v.camera().forward)) > 0) n = vec.scale(n, -1);
  } else if (how === "through") {
    const h = Math.hypot(nrm[0], nrm[1]);
    n = h < 0.3 ? [1, 0, 0] : vec.cross([0, 0, 1], [nrm[0] / h, nrm[1] / h, 0]);
  }
  const g = slabOver(p.model, axesFromNormal(n), how === "plan" ? 0.1 : 0.2);
  const id = await createSlab({ centre: round(g.centre), axes: g.axes, half: round(g.half) });
  if (id) { pushHistory(); choose("slab", id); viewSlab(id); }
}

// ----- panes: the views' chrome ------------------------------------------------------

function setInfo(kind, text) {
  const e = $(`slotinfo-${kind}`);
  if (e) e.textContent = text;
}

function makeGlPane(kind) {
  const pane = el("div", { id: `${kind}Pane`, class: "glpane" });
  const host = el("div", { class: "glhost" });
  const chip = el("div", { class: "glchip", hidden: "" });
  const bar = el("div", { class: "diffbar", hidden: "" });
  // The view's own controls, and (in 3D) a compass to turn it by.
  const tools = el("div", { class: "gltools" });
  const compass = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  compass.setAttribute("viewBox", "-40 -40 80 80");
  compass.setAttribute("class", "glcompass");
  pane.append(host, chip, bar, tools, compass);
  $("parked").appendChild(pane);
  const v = (views[kind] = new GLView(host));
  v.chip = chip; v.diffbar = bar; v.name = kind; v.tools = tools; v.compass = compass; v.slab = null;
  v.onDraw = (cam) => drawCompass(v, cam);
  compass.addEventListener("click", (e) => {
    const d = e.target.closest("[data-dir]")?.dataset.dir;
    if (!d) return;
    if (d === "top") return v.animate({ az: (-pl.yaw * 180) / Math.PI + 180, el: 89.9, ortho: true });
    // From that side of the model: the viewer stands there, looking back.
    const w = rot({ N: [0, 1, 0], E: [1, 0, 0], S: [0, -1, 0], W: [-1, 0, 0] }[d]);
    v.animate({ az: (Math.atan2(w[0], w[1]) * 180) / Math.PI, el: 12, flat: false });
  });
  host.addEventListener("pointerdown", () => { if (active !== kind) { active = kind; renderLayers(); } }, true);
  v.onHandle = (id, phase, ray, e) => handleDrag(v, id, phase, ray, e);
  v.onHandleMenu = (id, e) => handleMenu(v, id, e);
  v.onLine = (a, b) => cutAlong(v, a, b);
  v.onChange = () => {
    requestFree(v);
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
  const pickAt = async (ray, e) => {
    if (drawing) return drawPoint(v, ray, e);
    // On the lines of a slab, a plane or a polyline: that is what is meant.
    const o = !e?.altKey && objectAt(v, e);
    if (o) return choose(o.kind, o.id);
    const cuts = v.cuts();
    const r = await service.pick(ray.origin, ray.dir, cuts.z, cuts.slab);
    // What is drawn there is what is selected: a shown selection's points; the member under
    // the cursor (the one the ray through it meets first, as it is drawn), unless the scan's
    // surface there is well in front of it (the click is then on that surface); else the
    // point itself. With alt: always the point.
    const selId = !e?.altKey && (r.selections || []).find((id) => vis("selections", `sel:${id}`));
    const m = !e?.altKey && v.state.layers.model && memberAt(ray, v);
    // (A lining stands a little in front of what it covers: that much is allowed.)
    const onMember = m && (!r.hit || m.t <= r.range + 0.15) ? m.path : null;
    if (selId) choose("selection", selId);
    else if (onMember) { selectMember(onMember); if (r.hit) { sel.at = r; renderChips(); rebuildMarks(); } }
    else {
      choose(r.hit ? "point" : null, r.hit ? r : null);
      // The member near it fills the Member tab (without leaving the tab you are on).
      if (r.member) showMember(r.member.path);
    }
  };
  v.onPick = pickAt;
  // What the cursor is on, for zooming toward it.
  v.onDepth = async (ray) => { const cuts = v.cuts(); return (await service.pick(ray.origin, ray.dir, cuts.z, cuts.slab)).hit ?? null; };
  // Right-click: pick what is there, then offer what can be done with it.
  v.onContext = async (ray, e) => {
    // (Looking at what is there does not change what is selected.)
    const o = objectAt(v, e);
    if (o) return openMenu(objectMenu(o.kind, o.id), e.clientX, e.clientY);
    const cuts = v.cuts();
    const p = await service.pick(ray.origin, ray.dir, cuts.z, cuts.slab);
    contextMenu(v, p, e.clientX, e.clientY);
  };
  host.addEventListener("dblclick", (e) => {
    if (e.target !== v.canvas) return;
    if (drawing) return endDrawing(true);
    const p = sel.pick ?? sel.at;
    if (p?.hit) v.animate({ target: p.hit });
  });
  // A window dragged with shift: the points in it become a selection. Of the whole scan only the
  // surface nearest the viewer is taken; in a view clipped to a slab, all of the slab's points.
  // (With alt, or with the view's "everything in the window" on: all that is behind it too.)
  v.onSelect = async (w, e) => {
    const cuts = v.cuts();
    const r = await service.call("selections", { action: "window", ...w, z: cuts.z, slab: cuts.slab, front_only: !cuts.slab && !e?.altKey && !v.state.selectAll });
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
  // (Where on a member it was clicked, when it was selected in a view.)
  sel.at = null;
  sel.selection = kind === "selection" ? value : null;
  sel.plane = kind === "plane" ? value : null;
  sel.slab = kind === "slab" ? value : null;
  sel.polyline = kind === "polyline" ? value : null;
  renderChips(); rebuildMarks(); rebuildModel(); renderTree();
}
/** What is selected: { kind, id } or null. */
function chosen() {
  if (sel.pick) return { kind: "point", id: null };
  if (sel.member) return { kind: "member", id: sel.member };
  if (theSelection()) return { kind: "selection", id: sel.selection };
  if (thePlane()) return { kind: "plane", id: sel.plane };
  if (slabOf(sel.slab)) return { kind: "slab", id: sel.slab };
  if (lineOf(sel.polyline)) return { kind: "polyline", id: sel.polyline };
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

/** Leaves a selection's points out of the scan for everyone (or puts them back): for points that are not of the building. */
async function ignoreSelection(s) {
  if (!s) return;
  if (!s.ignored && !confirm(`Ignore the ${s.count.toLocaleString()} points of ${selName(s)}?\n\nThey are left out of the scan for everyone: not drawn, not picked, not counted as evidence. This is for points that are not of the building (reflections, something that was moved). The selection stays, marked ignored, and can be put back from its menu.\n\nThe scan is loaded again, which takes a little while.`)) return;
  const r = await service.call("selections", { action: "ignore", id: s.id, ignore: !s.ignored });
  if (r.error) alert(r.error);
}

/** The points seen through a member (their rays pass through it): a selection of them, shown with the rays. */
async function seenThrough(path) {
  const r = await service.call("selections", { action: "through", member: path });
  if (r.error) return alert(r.error === "no scan points there" ? "No rays pass through this member." : r.error);
  const id = r.text.match(/selected (s\d+)/)?.[1];
  if (!id) return;
  await until(() => st?.selections?.some((s) => s.id === id));
  await showSelection(id);
}

/** Where a new issue can go, given what is selected: [{ key, label }], the likeliest first. */
function whereOptions() {
  const out = [], p = sel.pick, s = theSelection(), pn = thePlane();
  const onFace = p?.member && p.member.distance < 0.06;
  if (sel.member) out.push({ key: "member", label: `Member ${shortPath(sel.member)}` });
  if (sel.member && sel.at) out.push({ key: "point", label: `Point ${f3(sel.at.model)}` });
  if (p?.member && onFace) out.push({ key: "member", label: `Member ${shortPath(p.member.path)}` });
  if (p?.hit) out.push({ key: "point", label: `Point ${f3(p.model)}` });
  if (p?.member && !onFace) out.push({ key: "member", label: `Member ${shortPath(p.member.path)}` });
  if (s) out.push({ key: "selection", label: `Selection ${selName(s)}` });
  if (pn) out.push({ key: "plane", label: `Plane ${planeName(pn)}` });
  const sb = slabOf(sel.slab), ln = lineOf(sel.polyline);
  if (sb && !sb.temp) out.push({ key: "slab", label: `Slab ${slabName(sb)}` });
  if (ln) out.push({ key: "polyline", label: `Polyline ${lineName(ln)}` });
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

/** A slab made by the service (on a plane, around a selection), selected and shown. */
async function slabFrom(args) {
  const id = await createSlab(args);
  if (id) { pushHistory(); choose("slab", id); viewSlab(id); }
}
/** Frames the focused view on a slab or a polyline, without clipping to it (Back returns). */
function gotoBox(pts) {
  const lo = [0, 1, 2].map((k) => Math.min(...pts.map((p) => p[k]))), hi = [0, 1, 2].map((k) => Math.max(...pts.map((p) => p[k])));
  pushHistory();
  views[active].frame(lo.map((x, k) => 0.5 * (x + hi[k])), Math.max(0.6, 0.6 * Math.hypot(...vec.sub(hi, lo))));
}
const deleteSlab = async (id) => {
  const k = temps.findIndex((t) => t.id === id);
  if (k >= 0) temps.splice(k, 1); else await service.call("slabs", { action: "delete", id });
  if (sel.slab === id) choose(null);
  syncSlabs(); renderLayers(); rebuildMarks();
};
/** Keeps a slab that came from a link, where the views on it stay. */
async function keepSlab(id) {
  const t = slabOf(id);
  const kept = t && (await createSlab({ centre: t.centre, axes: t.axes, half: t.half }));
  if (!kept) return;
  for (const v of Object.values(views)) if (v.slab === id) v.slab = kept;
  temps.splice(temps.findIndex((x) => x.id === id), 1);
  choose("slab", kept);
  syncSlabs(); renderLayers();
}
const deleteLine = async (id) => { await service.call("polylines", { action: "delete", id }); if (sel.polyline === id) choose(null); };

function gotoPlane(id) {
  const s = st.surfaces?.find((x) => x.id === id);
  if (!s) return;
  pushHistory();
  const c = planeCorners(s);
  views[active].frame(c[0].map((x, k) => 0.5 * (x + c[2][k])), Math.max(0.8, 0.6 * Math.hypot(...vec.sub(c[2], c[0]))));
}
const deletePlane = async (id) => { await service.call("surfaces", { action: "delete", id }); if (sel.plane === id) choose(null); };

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
  // A slab is gone to by looking through it.
  else if (kind === "slab") { if (slabOf(id)) { pushHistory(); choose("slab", id); viewSlab(id); } }
  else if (kind === "polyline") { const l = lineOf(id); if (l) { hidden.delete(`line:${id}`); hidden.delete("polylines"); keep(); choose("polyline", id); gotoBox(l.points.map(toScan)); } }
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
  items.push(null, { label: "Slab through here", run: () => {}, cls: "head" });
  if (surfaceAt(p)) {
    add("Along this surface", () => slabAt("along", p, v));
    add("Cut through this surface", () => slabAt("through", p, v));
  }
  add("Plan at this height", () => slabAt("plan", p, v));
  add("Cut across x", () => slabAt("x", p, v));
  add("Cut across y", () => slabAt("y", p, v));
  items.push(null);
  if (v) add("Orbit about this point", () => v.animate({ target: p.hit }));
  add("Fit a plane here (points within 1 m)", () => fitPlane({ seed: p.hit, frame: "scan", radius: 1.0 }));
  if (p.member) add(`Checks for ${shortPath(p.member.path)}`, () => { showMember(p.member.path); app.showTab("member"); });
  return items;
}

/** The menu of an object: the same in the tree, on the card and in the views. `key`: its row in the tree, for hiding. */
function objectMenu(kind, id) {
  const items = [], add = (label, run, cls) => items.push({ label, run, cls });
  const key = { member: `m:${id}`, group: `g:${id}`, issue: `issue:${id}`, selection: `sel:${id}`, plane: `plane:${id}`, slab: `slab:${id}`, polyline: `line:${id}`, top: id }[kind];
  const showHide = () => add(vis(key) ? "Hide" : "Show", () => toggleVis(key));
  if (kind === "member") {
    add("New issue on this member", () => { selectMember(id); startDraft("member"); });
    add("Go to it", () => gotoMembers(id));
    add("Its checks", () => { selectMember(id); app.showTab("member"); });
    if (st.scan.rays) add("Show the points seen through it (what contradicts it)", () => seenThrough(id));
    add("Faces placed on it by hand…", () => facesPlacedOn(id));
    showHide();
  } else if (kind === "group" || kind === "top") {
    showHide();
    if (kind === "group") add("Go to it", () => gotoMembers(`/${id}/`));
    if (id === "scan") add(`Leave out points near the sensor… (now within ${+(st.scan.min_range ?? 1).toFixed(2)} m)`, askMinRange);
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
    add("Slab around it", () => slabFrom({ selection: id }));
    const sl = st.selections.find((x) => x.id === id);
    add(sl?.ignored ? "Use its points again" : "Ignore its points (leave them out of the scan)…", () => ignoreSelection(sl));
    showHide();
    items.push(null);
    add("Delete it", () => deleteSelection(id), "danger");
  } else if (kind === "plane") {
    const s = st.surfaces.find((x) => x.id === id);
    add("New issue on this plane", () => { choose("plane", id); startDraft("plane"); });
    add("Name it…", () => { choose("plane", id); focusName(); });
    add("Go to it", () => gotoPlane(id));
    add("Slab on it", () => slabFrom({ plane: s.id }));
    showHide();
    items.push(null);
    add("Delete it", () => deletePlane(id), "danger");
  } else if (kind === "slab") {
    const s = slabOf(id);
    add("View it (a 3D view clipped to it)", () => { pushHistory(); choose("slab", id); viewSlab(id); });
    if (s.temp) add("Keep it", () => keepSlab(id));
    else {
      add("New issue on this slab", () => { choose("slab", id); startDraft("slab"); });
      add("Name it…", () => { choose("slab", id); focusName(); });
    }
    add("Go to it in this view", () => { choose("slab", id); gotoBox(slabCorners(s)); });
    if (!s.temp) showHide();
    items.push(null);
    add("Delete it", () => deleteSlab(id), "danger");
  } else if (kind === "polyline") {
    const l = lineOf(id);
    add("New issue on this polyline", () => { choose("polyline", id); startDraft("polyline"); });
    add("Name it…", () => { choose("polyline", id); focusName(); });
    add("Go to it", () => gotoObject("polyline", id));
    if (l.points.length > 2) add(l.closed ? "Open it" : "Close it", () => updateLine(id, { closed: !l.closed }));
    showHide();
    items.push(null);
    add("Delete it", () => deleteLine(id), "danger");
  }
  return items;
}

/** The right-click menu of a scan view: for the point under the cursor, then for what is selected. */
/** The faces placed by hand on a member: each to take back. */
async function facesPlacedOn(path) {
  const r = await service.call("seen", { action: "list" });
  if (r.error) return note(r.error, { error: true });
  const rel = path.split("/").slice(1).join("/");
  const lines = r.text.split("\n").filter((l) => l.includes(`member(${JSON.stringify(rel)})`));
  if (!lines.length) return note(`No faces of ${shortPath(path)} have been placed by hand.`);
  const box = note(lines.map((l) => l.replace(/^\s*coincide\(.*?\), \/\/ /, "")).join("\n"));
  const at = $("seenNote").querySelector(".row");
  for (const l of lines) {
    const n = Number(l.match(/\/\/ (\d+):/)?.[1]);
    if (!n) continue;
    const b = el("button", {}, `Take back ${n}`);
    b.onclick = () => takeBack(n);
    at.prepend(b);
  }
  return box;
}

function contextMenu(v, p, x, y) {
  const items = p?.hit ? pointMenu(p, v) : [{ label: "New issue, not pinned", run: () => startDraft("none") }];
  const c = chosen();
  if (c && c.kind !== "point") {
    const name = c.kind === "member" ? shortPath(c.id) : c.kind === "selection" ? `selection ${selName(theSelection())}` : c.kind === "plane" ? `plane ${planeName(thePlane())}` : c.kind === "slab" ? `slab ${slabName(slabOf(c.id))}` : `polyline ${lineName(lineOf(c.id))}`;
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
  const c = chosen(), p = sel.pick, s = theSelection(), pn = thePlane(), sb = slabOf(sel.slab), ln = lineOf(sel.polyline);
  for (const v of Object.values(views)) {
    // A polyline being drawn: what to do, and how to end.
    if (drawing) {
      v.chip.hidden = drawing.view !== v.name;
      const n = drawing.points.length;
      v.chip.innerHTML = `<div class="cardhead"><span><b>Polyline</b> ${n} point${n === 1 ? "" : "s"}${n > 1 ? `, ${lineLength(drawing).toFixed(3)} m` : ""}</span><span class="cardbtns"><button data-a="done" title="Keep it (Enter, or a double click)">✓</button><button data-a="cancel" title="Drop it (Esc)">×</button></span></div><div class="muted">Click points ${v.state.flat && v.state.box ? "in the slab" : "on the scan"}; click the first again to close it.</div>`;
      for (const b of v.chip.querySelectorAll("button")) b.onclick = () => endDrawing(b.dataset.a === "done");
      v.chip.oncontextmenu = null;
      continue;
    }
    v.chip.hidden = !c;
    if (!c) continue;
    // Being named or sized: leave it as it is.
    if (v.chip.contains(document.activeElement) && (document.activeElement.classList.contains("selname") || document.activeElement.tagName === "INPUT")) continue;
    const evLine = (path) => { const e = members?.find((x) => x.path === path)?.evidence; return e ? `<span class="ev ${e.state}">${e.state}</span> · ${e.points.toLocaleString()} points, ${e.rays_through} of ${e.rays_through + e.rays_ending} rays through` : ""; };
    const nameField = (text) => `<span class="selname" contenteditable="plaintext-only" data-ph="name it" title="Its name: type, Enter keeps it">${esc(text)}</span>`;
    let head = "", body = "";
    if (c.kind === "point") {
      head = `<b>Point</b> ${f3(p.model)}`;
      body = p.member ? `${(p.member.distance * 1000).toFixed(0)} mm from ${esc(shortPath(p.member.path))} · ${evLine(p.member.path)}` : "no member within 0.5 m";
    } else if (c.kind === "member") {
      head = `<b>Member</b> ${esc(shortPath(c.id))}`;
      body = evLine(c.id) + (sel.at ? `<br>the scan point there: ${f3(sel.at.model)}${sel.at.member?.path === c.id ? `, ${(sel.at.member.distance * 1000).toFixed(0)} mm from its face` : ""} <span title="Alt-click picks the point itself">(alt-click: the point)</span>` : "");
    } else if (c.kind === "selection") {
      head = `<b>Selection</b> ${esc(s.id)} ${nameField(s.label)}`;
      body = `${s.count.toLocaleString()} points${vis("selections", `sel:${s.id}`) ? "" : " · hidden"}${s.ignored ? " · <b>ignored</b>: left out of the scan" : ""}`;
    } else if (c.kind === "plane") {
      head = `<b>Plane</b> ${esc(pn.id)} ${nameField(pn.name)}`;
      body = `facing ${f3(pn.normal)} · ${pn.count.toLocaleString()} points, rms ${(pn.rms * 1000).toFixed(1)} mm · ${(pn.ext[1] - pn.ext[0]).toFixed(2)} × ${(pn.ext[3] - pn.ext[2]).toFixed(2)} m` +
        (sel.fits?.[pn.id] ? `<details><summary>What the fit found</summary><pre>${esc(sel.fits[pn.id])}</pre></details>` : "");
    } else if (c.kind === "slab") {
      const dim = (k, title) => `<input type="number" step="0.05" min="0.01" data-dim="${k}" value="${(2 * sb.half[k]).toFixed(2)}" title="${title}">`;
      head = `<b>Slab</b> ${esc(sb.id)} ${sb.temp ? `<span class="muted">not kept</span>` : nameField(sb.name)}`;
      body = `<span class="dims">${dim(0, "Its width (m)")} × ${dim(1, "Its height (m)")} × ${dim(2, "Its thickness (m)")} m</span> · seen from ${f3(sb.axes[2])}<br>Drag its faces to size it, its middle to slide it, ⟳ to turn it.`;
    } else {
      head = `<b>Polyline</b> ${esc(ln.id)} ${nameField(ln.name)}`;
      body = `${ln.points.length} points · ${lineLength(ln).toFixed(3)} m${ln.closed ? " around (closed)" : ""}<br>Drag a point to move it, a length to add one; right-click a point to remove it.`;
    }
    v.chip.innerHTML = `<div class="cardhead"><span>${head}</span><span class="cardbtns"><button data-a="issue" title="New issue on this">＋</button><button data-a="menu" title="What can be done with it (the same as right-clicking it)">⋯</button><button data-a="clear" title="Let go of it">×</button></span></div><div class="muted">${body}</div>`;
    const name = v.chip.querySelector(".selname");
    if (name) {
      const was = { selection: s?.label, plane: pn?.name, slab: sb?.name, polyline: ln?.name }[c.kind] ?? "";
      const save = { selection: (t) => nameSelection(c.id, t), plane: (t) => service.call("surfaces", { action: "label", id: c.id, label: t }), slab: (t) => updateSlab(c.id, { name: t }), polyline: (t) => updateLine(c.id, { name: t }) }[c.kind];
      name.onkeydown = (e) => { e.stopPropagation(); if (e.key === "Enter") { e.preventDefault(); name.blur(); } if (e.key === "Escape") { name.textContent = was; name.blur(); } };
      name.onblur = () => { const t = name.textContent.trim(); if (t !== was) save(t); };
    }
    for (const i of v.chip.querySelectorAll("input[data-dim]")) {
      i.onkeydown = (e) => { e.stopPropagation(); if (e.key === "Enter") i.blur(); };
      i.onchange = () => { const half = sb.half.slice(); if (Number(i.value) > 0) { half[Number(i.dataset.dim)] = Number(i.value) / 2; updateSlab(sb.id, { half }); } };
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

/// The proposal's strip sits under the view's toolbar, which wraps to more rows in a narrow pane (and would cover it).
function placeDiffbar(v) {
  if (!v.diffbar.hidden) v.diffbar.style.top = `${v.tools.offsetTop + v.tools.offsetHeight + 8}px`;
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
    placeDiffbar(v);
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
// (Groups that came later start open too.)
if (stored("topo.scan.groups", 1) < 2) { open.add("slabs"); open.add("polylines"); keep(); try { localStorage.setItem("topo.scan.groups", "2"); } catch (_) {} }
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
const selClouds = new Map(), selRays = new Map();
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
  let html = row(0, "top", "scan", "Scan", { note: st.scan.loaded && st.cloud ? `${mil(st.cloud)} of ${mil(st.scan.points)} points` : st.loading || "loading", title: `The scan's points (thinned for the 3D view; a view clipped to a slab draws every point in it). ${st.scan.ranged === false ? "" : `Points nearer the sensor than ${+(st.scan.min_range ?? 1).toFixed(2)} m are left out (${((st.scan.points ?? 0) - (st.scan.used ?? 0)).toLocaleString()} of them): right-click to change that.`}` });
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
  if (open.has("selections")) for (const s of st.selections || []) html += row(1, "sel", `sel:${s.id}`, selName(s), { id: s.id, on: s.id === sel.selection, note: `${s.count.toLocaleString()}${s.ignored ? " · ignored" : ""}`, title: s.ignored ? `${selName(s)}: its points are left out of the scan (shown here in yellow while its eye is on)` : selName(s) });
  html += row(0, "top", "planes", "Planes", { group: true, note: (st.surfaces || []).length || "right-click: fit" });
  if (open.has("planes")) for (const s of st.surfaces || []) html += row(1, "plane", `plane:${s.id}`, planeName(s), { id: s.id, on: s.id === sel.plane, note: `${(s.rms * 1000).toFixed(0)} mm` });
  html += row(0, "top", "slabs", "Slabs", { group: true, note: slabs().length || "right-click: slab", title: "Slabs: boxes a 3D view can be clipped to, to see a section. Double-click one to look through it" });
  if (open.has("slabs")) for (const s of slabs()) html += row(1, "slab", `slab:${s.id}`, slabName(s), { id: s.id, on: s.id === sel.slab, note: `${(2 * s.half[0]).toFixed(1)} × ${(2 * s.half[1]).toFixed(1)} × ${(2 * s.half[2]).toFixed(2)}${KINDS.some((k) => views[k].slab === s.id && app.shown(k)) ? " · in view" : ""}` });
  html += row(0, "top", "polylines", "Polylines", { group: true, note: lines().length || "draw in a view" });
  if (open.has("polylines")) for (const l of lines()) html += row(1, "polyline", `line:${l.id}`, lineName(l), { id: l.id, on: l.id === sel.polyline, note: `${lineLength(l).toFixed(2)} m` });
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
    else if (kind === "slab") choose("slab", r.id);
    else if (kind === "polyline") choose("polyline", r.id);
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
  square: svg("M3 3h10v10H3z M8 5.5v5 M5.5 8h5"),
  lock: svg("M4.5 7.5h7v5.5h-7z M6 7.5V5.5a2 2 0 0 1 4 0v2"),
  line: svg("M2.5 12.5l4-7 4 4 3-6", `<circle cx="2.5" cy="12.5" r="1.1" fill="currentColor"/><circle cx="6.5" cy="5.5" r="1.1" fill="currentColor"/><circle cx="10.5" cy="9.5" r="1.1" fill="currentColor"/><circle cx="13.5" cy="3.5" r="1.1" fill="currentColor"/>`),
  free: svg("M2.5 13.5L8 3.5 M2.5 13.5L12 5.5 M2.5 13.5L13.5 9.5", `<circle cx="2.5" cy="13.5" r="1.3" fill="currentColor"/>`),
  cut: svg("M2 13L14 3", `<path d="M5 14.5l-3-1.5 1.5-3 M12.5 1.5l1.5 1.5-3 1.5" stroke-width="1"/>`),
};

function renderTools(v) {
  const s = v.state, sb = slabOf(v.slab);
  // Not while one of its fields is being typed in.
  if (v.tools.contains(document.activeElement) && document.activeElement.tagName === "INPUT") return;
  const wasOpen = v.tools.querySelector("details.more")?.open;
  const btn = (a, icon, title, on) => `<button data-a="${a}" class="${on ? "on" : ""}" title="${title}">${icon}</button>`;
  v.tools.innerHTML =
    `<select data-k="colour" title="Colour the points by">${COLOURS.map((c) => `<option ${c === s.colour ? "selected" : ""}>${c}</option>`).join("")}</select>` +
    `<select data-k="slab" title="What of the scan this view shows: all of it, or what is in a slab (a section)"><option value="">Whole scan</option>${slabs().map((x) => `<option value="${esc(x.id)}" ${x.id === v.slab ? "selected" : ""}>Slab ${esc(slabName(x))}</option>`).join("")}<option value="+plan">New slab: plan…</option><option value="+x">New slab: across x…</option><option value="+y">New slab: across y…</option></select>` +
    `<span class="grp">${btn("ortho", ICON.ortho, "Orthographic (no perspective)", s.ortho)}${btn("top", ICON.top, "Straight down, the model's +y up")}${btn("fit", ICON.fit, "Fit the model in the view")}</span>` +
    (sb
      ? `<span class="grp">${btn("square", ICON.square, "Look square on at the slab, fitted in the view")}${btn("flat", ICON.lock, "Held square on: a drag slides the view. Off: a drag turns it", s.flat)}${st?.scan.rays ? btn("free", ICON.free, "Free space: where the scanner's rays pass in the slab, under the points. Brighter: more rays. Where it is dark, something was in the way, or nothing looked. It is made for what the view sees, a cell a pixel: zoom in for finer." + (s.free ? v.freeInfo ?? "" : ""), s.free) : ""}</span>` +
        `<span class="grp" title="The slab: its thickness (m), and stepping it back and forward by half of that">${btn("back", ICON.back, "Slab back (away from the viewer)")}<input type="number" step="0.05" min="0.01" value="${(2 * sb.half[2]).toFixed(2)}" data-a="thick">${btn("fwd", ICON.fwd, "Slab forward (toward the viewer)")}</span>`
      : "") +
    `<span class="grp">${btn("line", ICON.line, "Draw a polyline: click its points (on the scan, or in the slab seen square on)", drawing?.view === v.name)}${btn("cut", ICON.cut, "Cut a slab: drag a line across the building, seen from above, to look at what it cuts through", v.tool === "line")}</span>` +
    `<span class="grp">${btn("noroof", ICON.noroof, "Cut away what is above 2.3 m (the roof)", s.zMax != null)}${btn("nofloor", ICON.nofloor, "Cut away what is below 0.1 m (the floor)", s.zMin != null)}</span>` +
    `<details class="more" ${wasOpen ? "open" : ""}><summary title="Point size, and the height cut exactly">⋯</summary><div><label>Point size <input type="range" min="1" max="6" step="0.5" value="${s.pointSize}" data-a="size"></label><br>Show model z from <input type="number" step="0.1" placeholder="any" value="${s.zMin ?? ""}" data-z="zMin"> to <input type="number" step="0.1" placeholder="any" value="${s.zMax ?? ""}" data-z="zMax"> m<br><label title="A shift-drag then takes every point inside the window, however far behind. Off: only the surface nearest you (alt-shift-drag takes everything once)"><input type="checkbox" data-a="selall" ${s.selectAll ? "checked" : ""}> A shift-drag selects everything in the window, not only the nearest surface</label></div></details>`;
  v.tools.querySelector("input[data-a=selall]").onchange = (e) => v.set({ selectAll: e.target.checked }, true);
  const size =v.tools.querySelector("input[data-a=size]");
  size.oninput = () => v.set({ pointSize: Number(size.value) }, true);
  const step = (g) => updateSlab(sb.id, { centre: vec.add(sb.centre, vec.scale(sb.axes[2], g * sb.half[2])) });
  const acts = {
    ortho: () => v.set({ ortho: !s.ortho }),
    square: () => v.squareOn(boxOf(sb)),
    flat: () => (s.flat ? v.set({ flat: false }) : v.squareOn(boxOf(sb))),
    free: () => { v.set({ free: !s.free }); requestSlab(v); },
    line: () => (drawing ? endDrawing(true) : startDrawing(v)),
    cut: () => { v.tool = v.tool === "line" ? null : "line"; },
    top: () => v.animate({ az: (-pl.yaw * 180) / Math.PI + 180, el: 89.9, ortho: true }),
    fit: () => { const b = modelBounds(); if (b) v.frame(toScan(b.centre), b.radius); },
    noroof: () => v.set({ zMax: s.zMax != null ? null : 2.3 }), nofloor: () => v.set({ zMin: s.zMin != null ? null : 0.1 }),
    back: () => step(-1),
    fwd: () => step(1),
  };
  for (const b of v.tools.querySelectorAll("button[data-a]")) b.onclick = () => { acts[b.dataset.a](); renderTools(v); };
  v.tools.querySelector("select[data-k=colour]").onchange = (e) => v.set({ colour: e.target.value });
  v.tools.querySelector("select[data-k=slab]").onchange = async (e) => {
    const id = e.target.value;
    active = v.name;
    if (id.startsWith("+")) {
      // A new slab through what the view is looking at.
      const at = toModel(s.target), how = id.slice(1);
      const g = slabOver(at, axesFromNormal(how === "plan" ? [0, 0, 1] : how === "x" ? [1, 0, 0] : [0, -1, 0]), how === "plan" ? 0.1 : 0.2);
      const made = await createSlab({ centre: round(g.centre), axes: g.axes, half: round(g.half) });
      if (!made) return renderTools(v);
      pushHistory(); choose("slab", made);
      return viewSlab(made, v.name);
    }
    pushHistory();
    if (id) return viewSlab(id, v.name);
    v.slab = null;
    syncSlabs(); renderLayers(); rebuildMarks();
  };
  for (const i of v.tools.querySelectorAll("[data-z]")) i.onchange = () => v.set({ [i.dataset.z]: i.value === "" ? null : Number(i.value) });
  const th = v.tools.querySelector("input[data-a=thick]");
  if (th) th.onchange = () => { if (Number(th.value) > 0) updateSlab(sb.id, { half: [sb.half[0], sb.half[1], Number(th.value) / 2] }); };
}

/** The 3D view's compass: the model's north, east, south and west as the view has them. Click one to look from that side; the middle, straight down. */
function drawCompass(v, cam) {
  const R = 25, names = { N: "north", E: "east", S: "south", W: "west" };
  // The far ones first, so the near ones are drawn over them; letters are outlined, not boxed, so none hides another.
  // (Styled inline: a drawing sheet, once loaded, brings a page-wide `text { fill: black; stroke: none }`.)
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
      `<text x="${lx.toFixed(1)}" y="${(ly + 4).toFixed(1)}" text-anchor="middle" style="font: 700 11px sans-serif; fill: ${col}; fill-opacity: ${far ? 0.6 : 1}; stroke: #16171b; stroke-width: 2.6px; paint-order: stroke">${d.name}</text></g>`;
  }
  out += `<g class="hot" data-dir="top"><title>Look straight down</title><circle class="hit" r="7"/><circle r="3.2" fill="#6ea8ff" stroke="#16171b"/></g>`;
  if (v.compassHtml !== out) { v.compassHtml = out; v.compass.innerHTML = out; }
}

function renderLayers() {
  renderTree();
  for (const v of Object.values(views)) { renderTools(v); placeDiffbar(v); }
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
  for (const li of box.querySelectorAll("li[data-s]")) li.onclick = () => { const a = st.sections[Number(li.dataset.s)].args; followLink({ section: a, colour: a.color }); };
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
  else if (where === "slab") args.slab = sel.slab;
  else if (where === "polyline") args.polyline = sel.polyline;
  else if (where === "member") args.member = sel.pick?.member?.path ?? sel.member;
  else if (where === "point") { args.frame = "scan"; args.point = (sel.pick ?? sel.at).hit; }
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
function refs(text, issue) {
  // Members first: the text between them is then looked at for the rest.
  const found = memberRefs(text, issue);
  if (!found.length) return objectRefs(text);
  let out = "", at = 0;
  for (const m of found) {
    out += objectRefs(text.slice(at, m.start)) + `<a href="#" class="ref" data-member="${esc(m.path)}" title="${esc(m.path)}: select it and go to it">${esc(text.slice(m.start, m.end))}</a>`;
    at = m.end;
  }
  return out + objectRefs(text.slice(at));
}

/**
 * Where a text names members: [{ start, end, path }], in order, not overlapping.
 * A member is named by its group and name ("West wall/stud#3", "T7/web.T1-Cl"),
 * or by its name alone where that says which ("stud#6", "top_chord.X1-P"): the
 * only one of that name, else the one in the group named nearest before it in
 * the text (or after), else in the group of the member the issue is on,
 * else the one nearest the issue's place.
 */
function memberRefs(text, issue) {
  if (!members?.length) return [];
  const short = new Map(), byName = new Map(), groups = new Set();
  for (const m of members) {
    const p = m.path.split("/"), name = p[p.length - 1], group = p[p.length - 2] ?? "";
    short.set(`${group}/${name}`, m.path);
    groups.add(group);
    if (!byName.has(name)) byName.set(name, []);
    byName.get(name).push({ group, path: m.path });
  }
  const out = [], taken = (s, e) => out.some((r) => s < r.end && e > r.start);
  const edge = (i) => i < 0 || i >= text.length || !/[\w#]/.test(text[i]);
  const each = (needle, f) => { for (let i = text.indexOf(needle); i >= 0; i = text.indexOf(needle, i + 1)) if (edge(i - 1) && edge(i + needle.length) && !taken(i, i + needle.length)) f(i); };
  // Group and name together: the longest first, so "stud#12" is not taken as "stud#1".
  for (const s of [...short.keys()].sort((a, b) => b.length - a.length)) if (text.includes(s)) each(s, (i) => out.push({ start: i, end: i + s.length, path: short.get(s) }));
  // Where groups are mentioned, for names alone.
  const mentions = [];
  for (const g of groups) if (g && text.includes(g)) for (let i = text.indexOf(g); i >= 0; i = text.indexOf(g, i + 1)) mentions.push({ at: i, group: g });
  const own = (issue?.place?.member ?? "").split("/").slice(-2)[0];
  for (const [name, list] of [...byName].sort((a, b) => b[0].length - a[0].length)) {
    // (Only names that could not be ordinary words.)
    if (!/[#.]/.test(name) || !text.includes(name)) continue;
    each(name, (i) => {
      let pick = list.length === 1 ? list[0] : null;
      if (!pick) {
        const has = mentions.filter((m) => list.some((l) => l.group === m.group));
        const near = has.filter((m) => m.at < i).sort((a, b) => b.at - a.at)[0] ?? has.sort((a, b) => a.at - b.at)[0];
        pick = list.find((l) => l.group === (near?.group ?? own));
        // Else the one of that name nearest the issue's place.
        if (!pick && issue?.place) {
          const c = toModel(issue.place.min.map((x, k) => 0.5 * (x + issue.place.max[k])));
          const far = (l) => { let s = [0, 0, 0], n = 0; for (const f of members.find((x) => x.path === l.path).faces) for (const p of f) { s = vec.add(s, p); n++; } return n ? Math.hypot(...vec.sub(vec.scale(s, 1 / n), c)) : Infinity; };
          pick = list.map((l) => [far(l), l]).sort((a, b) => a[0] - b[0])[0]?.[1];
        }
      }
      if (pick) out.push({ start: i, end: i + name.length, path: pick.path });
    });
  }
  return out.sort((a, b) => a.start - b.start);
}

function objectRefs(text) {
  return esc(text).replace(/(^|[^\w#])(s\d+|p\d+|b\d+|l\d+|#\d+)\b/g, (all, pre, id) => {
    if (id[0] === "b") { const s = st.slabs?.find((x) => x.id === id); return s ? `${pre}<a href="#" class="ref sel" data-objref="slab:${id}" title="Look through this slab">${esc(slabName(s))}</a>` : all; }
    if (id[0] === "l") { const l = st.polylines?.find((x) => x.id === id); return l ? `${pre}<a href="#" class="ref sel" data-objref="polyline:${id}" title="Show this polyline (${lineLength(l).toFixed(2)} m)">${esc(lineName(l))}</a>` : all; }
    if (id[0] === "p") { const pn = st.surfaces?.find((x) => x.id === id); return pn ? `${pre}<a href="#" class="ref sel" data-planeref="${id}" title="Show this plane">${esc(planeName(pn))}</a>` : all; }
    if (id[0] === "#") return st.issues.some((i) => i.id === Number(id.slice(1))) ? `${pre}<a href="#" class="ref" data-issue="${id.slice(1)}">${id}</a>` : all;
    const s = st.selections?.find((x) => x.id === id);
    return s ? `${pre}<a href="#" class="ref sel" data-selref="${id}" title="Show this selection (${s.count.toLocaleString()} points)">${esc(selName(s))}</a>` : all;
  });
}

function entryHtml(issue, e, k) {
  const who = `${badge(e.who, true)} <b>${esc(service.participant(e.who).name)}</b> <span class="muted">${when(e.time)}</span>`;
  if (e.kind === "comment") return `<div class="entry comment">${who}<div class="text">${refs(e.text, issue)}</div>${e.view ? `<button class="viewlink" data-view="${k}" title="Show this in a pane (Back returns to your view)">Show: ${esc(e.view_label || "view")}</button>` : ""}</div>`;
  if (e.kind === "tool") return `<div class="entry tool">${badge(e.who, true)} ran <code>${esc(e.name)}</code> <span class="muted">${esc(e.summary)}</span></div>`;
  if (e.kind === "change") return `<div class="entry tool">${badge(e.who, true)} <span class="muted">${esc(e.what)} · ${when(e.time)}</span></div>`;
  if (e.kind === "evidence") return `<div class="entry card">${who}<div class="k">Evidence</div><b>${esc(e.title)}</b><div class="text">${refs(e.text, issue)}</div></div>`;
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
      <div class="muted">${i.category} · raised by ${esc(service.participant(i.by).name)} ${when(i.time)}${i.place ? ` · <a href="#" id="iGo" title="Go there (Back returns)">${i.place.member ? "member " + esc(shortPath(i.place.member)) : i.place.selection ? "selection " + esc(selName(st.selections?.find((s) => s.id === i.place.selection) ?? { id: i.place.selection })) : i.place.plane ? "plane " + esc(planeName(st.surfaces?.find((s) => s.id === i.place.plane) ?? { id: i.place.plane })) : i.place.object ? (i.place.object[0] === "b" ? "slab " : "polyline ") + esc(i.place.object) : "at model " + f3(toModel(i.place.min.map((x, k) => 0.5 * (x + i.place.max[k]))))}</a>` : " · not pinned"}${i.status === "waiting" ? ` · <b>waiting on ${esc(waitingOn(i))}</b>` : ""}</div></div>
    <div class="thread">${i.entries.map((e, k) => entryHtml(i, e, k)).join("") || `<div class="muted" style="padding:8px 2px">Nothing said yet.</div>`}</div>
    <div class="composer"><textarea id="reply" rows="2" placeholder="Reply in #${i.id}… (Enter sends; @name to call someone)"></textarea>
      <div class="row"><label class="c" title="Attach the focused scan view (${esc(describeView())}) as a link others can follow"><input type="checkbox" id="attach" ${attach ? "checked" : ""}> attach my view</label>${theSelection() ? `<button data-ref="${esc(sel.selection)}" title="Refer to the selection you have shown: its id is put in your reply, and becomes a link">+ ${esc(selName(theSelection()))}</button>` : ""}${thePlane() ? `<button data-ref="${esc(sel.plane)}" title="Refer to the plane you have shown">+ ${esc(planeName(thePlane()))}</button>` : ""}${slabOf(sel.slab) && !slabOf(sel.slab).temp ? `<button data-ref="${esc(sel.slab)}" title="Refer to the slab you have selected">+ ${esc(slabName(slabOf(sel.slab)))}</button>` : ""}${lineOf(sel.polyline) ? `<button data-ref="${esc(sel.polyline)}" title="Refer to the polyline you have selected">+ ${esc(lineName(lineOf(sel.polyline)))}</button>` : ""}<button class="act primary" id="send">Send</button></div></div>`;
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
  if (go) go.onclick = (e) => { e.preventDefault(); i.place.object ? gotoObject(i.place.object[0] === "b" ? "slab" : "polyline", i.place.object) : gotoIssue(i.id); };
  for (const a of box.querySelectorAll("a[data-objref]")) a.onclick = (e) => { e.preventDefault(); gotoObject(...a.dataset.objref.split(":")); };
  for (const b of box.querySelectorAll("button[data-view]")) b.onclick = () => followLink(i.entries[Number(b.dataset.view)].view);
  for (const a of box.querySelectorAll("a[data-selref]")) a.onclick = (e) => { e.preventDefault(); gotoSelection(a.dataset.selref); };
  for (const a of box.querySelectorAll("a[data-planeref]")) a.onclick = (e) => { e.preventDefault(); gotoPlane(a.dataset.planeref); };
  for (const b of box.querySelectorAll("button[data-ref]")) b.onclick = () => { const at = reply.selectionStart ?? reply.value.length; reply.setRangeText(`${at && !/\s$/.test(reply.value.slice(0, at)) ? " " : ""}${b.dataset.ref} `, at, reply.selectionEnd ?? at, "end"); reply.focus(); };
  // A member named in the thread: selected (it is highlighted, and the Member tab is its) and gone to.
  for (const a of box.querySelectorAll("a[data-member]")) a.onclick = (e) => { e.preventDefault(); choose("member", a.dataset.member); gotoMembers(a.dataset.member); };
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
  // What was dragged here is in the service's state now.
  if (live?.done) live = null;
  syncSlabs();
  for (const v of Object.values(views)) { v.zOffset = pl.t[2]; v.floorZ = s.scan.floor_z ?? 0; }
  const cloudKey = `${s.scan.min_range}|${s.cloud}|${(s.selections || []).filter((x) => x.ignored).map((x) => x.id).join()}`;
  if (s.cloud && cloudLoaded !== cloudKey) loadCloud(cloudKey);
  loadMembers();
  freshPlaces();
  app.setStatus(s.loading ? `scan: ${s.loading}…` : !service.connected ? "the workspace service is not answering" : null);
  if (sel.issue != null && !s.issues.some((i) => i.id === sel.issue)) sel.issue = null;
  renderChat(); renderPeople(); renderIssues(); renderBoard(); renderViews(); renderLayers(); rebuildMarks(); renderDiffBars(); renderHistory(); renderChips(); syncHighlights();
}

export async function initScan(hooks) {
  app = hooks;
  // Panes.
  for (const k of KINDS) makeGlPane(k);
  $("parked").appendChild(el("div", { id: "boardPane", class: "board" }));
  // Three 3D views of the scan: the second and third are for looking through slabs beside the first.
  for (const [k, label] of [["scan3d", "3D with scan"], ["view2", "3D with scan, 2"], ["view3", "3D with scan, 3"]])
    app.registerPane(k, { label, el: `${k}Pane`, shown: () => { active = k; if (st) { syncSlabs(); rebuildMarks(); } renderLayers(); app.showBrowser("layers"); } });
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
  // A polyline being drawn: Enter keeps it, Esc drops it.
  document.addEventListener("keydown", (e) => {
    if (!drawing || e.target.closest?.("input, textarea, [contenteditable]")) return;
    if (e.key === "Enter") endDrawing(true);
    if (e.key === "Escape") endDrawing(false);
  });
  // (Panes change size: the toolbars wrap differently.)
  window.addEventListener("resize", () => { for (const v of Object.values(views)) placeDiffbar(v); });
  // (For looking into it from the console, and for tests that drive the page.)
  globalThis.__topoScan = { views, memberAt, toScan, toModel, members: () => members };
  service.onState(onState);
  service.connect();
  // A first look: the scan beside what was open.
  if (!KINDS.some((k) => app.shown(k))) app.openBeside("scan3d");
}
