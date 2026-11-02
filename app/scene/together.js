// Others in a session, in the 3D views (docs/collaboration.md, "Seeing each other"). From outside, each is a slate where their
// camera is, with their picture on it and their name by it, and a ray of their colour along their cursor. Through their view
// (following them: a click on their icon, a double-click on their slate, or their row twice), their camera is this view's until it
// is moved, and what they do with the pointer and keys is drawn over it as a recording's is.
//
// Loaded only for a page in a session. All of it goes by the lossy channel (motion.js), and is drawn as it arrives, with nothing
// held back to smooth it: what is late is worth less than what is lost. Nothing here changes the model.

import { el, esc, openMenu } from "../ui.js";
import { vec } from "../glview.js";
import * as objects from "../objects.js";
import { workspace } from "../workspace.js";
import { bring, current, focused, onLayout, openDoc } from "../shell/dock.js";
import { say } from "../shell/panel.js";
import { kind, kindOf } from "./kinds.js";
import { active, sel, views } from "./state.js";
import { choose } from "./chosen.js";
import { partAt, displayExact } from "./exact.js";
import { addKindRows, renderTree } from "./tree.js";
import { applyVisibility, renderTools, theView, viewDrawn } from "./views.js";
import { slabOf, syncSlabs, tempSlab } from "./slabs.js";
import { viewPhoto } from "./photos.js";
import { personActions } from "./people.js";
import { Mesh } from "../session/motion.js";

/** The share of the height of a view a slate is, as seen, however far off it is. */
const SLATE_SHARE = 0.09;
/** Its picture: how many pixels high (across, as their view is), and of them how large the circle with who it is. */
const PICTURE_HIGH = 120, CIRCLE = 40;
/** How wide what is drawn over the view is, in points. */
const RAY_WIDTH = 2;
/** The least time between packets of the pointer moving (ms). What it presses or types goes at once. */
const AT_MOST_EVERY = 16;
/** How far a camera has to be from where it was put, for it to have been moved by someone (m, and way, unitless). */
const STEERED = 1e-3;
/** How long a person's last word is kept for (ms): past it, they are taken to be elsewhere, and are not drawn. */
const STALE_AFTER = 30000;

let session = null, mesh = null;
/** Each peer: what the channel has last said of them: `{ cam, aspect, depth, ray, pointer, at }`. */
const motion = new Map();
/** Who is being followed: `{ who, peer, v, sync, mine, expected }`. */
let following = null;

const rgbOf = (hex) => [1, 3, 5].map((k) => parseInt(hex.slice(k, k + 2), 16));
const personOf = (who) => session?.participants.get(who) ?? null;
const whoOfPeer = (peer) => session?.here.get(peer)?.who;
const initial = (p) => p.name.slice(0, 1).toUpperCase();
const typingIn = (t) => t?.closest?.("input, textarea, select, [contenteditable]");

// ----- what is sent of this person -----------------------------------------------------------------------------------------

/** The view this person's pointer or camera is of: the one under the pointer, else the one last used. */
let pointerIn = null;
const theirView = () => pointerIn ?? views[active] ?? Object.values(views)[0] ?? null;
let lastSent = 0, sendLater = 0, pending = [];

function packet() {
  const v = theirView(), cs = v?.scene?.cameraState();
  if (!cs) return null;
  const w = v.canvas.clientWidth || 1, h = v.canvas.clientHeight || 1, p = { c: { at: cs.at, forward: cs.forward, up: cs.up, ortho: cs.ortho ?? null }, a: w / h };
  p.d = Math.max(0.05, vec.dot(vec.sub(v.lookingAt(), cs.at), cs.forward));
  if (pointerIn && lastPointer) {
    const ray = v.ray(lastPointer.x, lastPointer.y), hit = v.state.layers.model ? partAt(v, ray) : null;
    p.r = [...ray.origin, ...ray.dir, hit?.t ?? p.d];
    p.n = [lastPointer.x / w, lastPointer.y / h];
    p.b = lastPointer.buttons;
  }
  if (pending.length) { p.e = pending; pending = []; }
  return p;
}
function send() {
  clearTimeout(sendLater);
  sendLater = 0;
  lastSent = performance.now();
  const p = packet();
  if (p) mesh.send(p);
}
/** Goes now, or (a stream of moves) when it may. */
function sendSoon() {
  const wait = AT_MOST_EVERY - (performance.now() - lastSent);
  if (wait <= 0) send();
  else if (!sendLater) sendLater = setTimeout(send, wait);
}

let lastPointer = null;
const heldNow = (e) => [e.shiftKey && "Shift", e.ctrlKey && "Ctrl", e.altKey && "Alt", e.metaKey && "Cmd"].filter(Boolean).join("+");
let heldWas = "";
/** Modifiers said, where they are not as they were. */
function noteHeld(e) {
  const h = heldNow(e);
  if (h !== heldWas) { heldWas = h; pending.push({ t: "modifiers", held: h }); }
}
const viewOfCanvas = (t) => Object.values(views).find((v) => v.canvas === t) ?? null;
function localPointer(e) {
  if (!mesh.anyone) return;
  const v = viewOfCanvas(e.target);
  if (!v) {
    if (pointerIn) { pointerIn = null; lastPointer = null; pending.push({ t: "left" }); sendSoon(); }
    return;
  }
  const r = v.canvas.getBoundingClientRect();
  pointerIn = v;
  lastPointer = { x: e.clientX - r.left, y: e.clientY - r.top, buttons: e.buttons };
  noteHeld(e);
  const n = [lastPointer.x / (r.width || 1), lastPointer.y / (r.height || 1)];
  if (e.type === "pointerdown") { pending.push({ t: "press", button: e.button, n }); return send(); }
  if (e.type === "pointerup") { pending.push({ t: "release", button: e.button }); return send(); }
  pending.push({ t: "move", n });
  sendSoon();
}
function localWheel(e) {
  if (!mesh.anyone || !viewOfCanvas(e.target)) return;
  noteHeld(e);
  pending.push({ t: "scroll", dots: -e.deltaY / 40 });
  sendSoon();
}
const KEY_SHOWN = { " ": "␣", Enter: "⏎", ArrowLeft: "←", ArrowRight: "→", ArrowUp: "↑", ArrowDown: "↓", Tab: "⇥", Backspace: "⌫", Delete: "⌦", Escape: "Esc" };
/** A key pressed, for those watching: not one typed into anything, which may be anything at all, and they are told so (`showSharing`). */
function localKey(e) {
  if (!mesh.anyone || typingIn(e.target) || !pointerIn) return;
  noteHeld(e);
  if (["Shift", "Control", "Alt", "Meta"].includes(e.key)) return sendSoon();
  const chord = e.metaKey || e.ctrlKey || e.altKey, label = KEY_SHOWN[e.key] ?? (e.key.length === 1 ? e.key.toUpperCase() : e.key);
  pending.push(chord ? { t: "key", label: `${heldNow(e)}+${label}` } : e.key.length === 1 ? { t: "text", text: e.key } : { t: "key", label });
  send();
}

/** A view drawn, which it is when its camera has moved: those watching are told. And a follower's camera looked at. */
function drawn(v) {
  checkSteered(v);
  drawPeopleIn(v);
  if (!mesh?.anyone || v !== theirView()) return;
  const cs = v.scene?.cameraState(), key = cs && JSON.stringify([cs.at, cs.forward, cs.up, cs.ortho, v.canvas.clientWidth, v.canvas.clientHeight]);
  if (key !== lastCamera) { lastCamera = key; sendSoon(); }
  tellVisible(v);
}
let lastCamera = null;

// ----- what is true of this person now -------------------------------------------------------------------------------------

/** What this person has in front of them, for whoever follows them: a drawing or a file, and what is visible in a view. */
function tellDoc() {
  const type = focused(), key = type && type !== "view" ? current(type)?.key : null;
  session.present({ doc: key ?? null });
}
let visibleWas = null;
function tellVisible(v) {
  const s = slabOf(v.slab), said = { hidden: [...objects.hidden], slab: s ? { centre: s.centre, axes: s.axes, half: s.half, filter: !!s.filter } : null, display: v.display ?? null, photo: v.state.photo?.name ?? null };
  const text = JSON.stringify(said);
  if (text === visibleWas) return;
  visibleWas = text;
  session.present({ visible: said });
}
/** That what is pressed and typed in a view is seen by whoever follows, said where the person can see it. */
function showSharing() {
  const here = session.others().length;
  say("session", here ? `in a session with ${here} other${here === 1 ? "" : "s"}: they see your camera, cursor and the keys you press in a 3D view (not what you type in a field)` : "in a session: nobody else is here yet");
}

// ----- others, from outside --------------------------------------------------------------------------------------------------

const pictures = new Map();
/** A person's picture on their slate, the shape of their view: their colour, and a circle with their face, or their initial till
 *  that is had. */
function pictureOf(p, aspect) {
  const h = PICTURE_HIGH, w = Math.max(CIRCLE * 2, Math.round(h * (aspect ?? 1.6))), key = `${p.id}|${p.name}|${p.colour}|${w}`;
  let had = pictures.get(key);
  if (had) return had;
  const c = new OffscreenCanvas(w, h), g = c.getContext("2d", { willReadFrequently: true });
  const paint = (face) => {
    g.fillStyle = p.colour;
    g.fillRect(0, 0, w, h);
    g.fillStyle = "rgba(255,255,255,.22)";
    g.fillRect(4, 4, w - 8, h - 8);
    g.beginPath();
    g.arc(w / 2, h / 2, CIRCLE, 0, 2 * Math.PI);
    g.fillStyle = p.colour;
    g.fill();
    if (face) {
      g.save();
      g.clip();
      g.drawImage(face, w / 2 - CIRCLE, h / 2 - CIRCLE, 2 * CIRCLE, 2 * CIRCLE);
      g.restore();
    } else {
      g.fillStyle = "#fff";
      g.font = "bold 44px sans-serif";
      g.textAlign = "center";
      g.textBaseline = "middle";
      g.fillText(initial(p), w / 2, h / 2 + 2);
    }
    return { w, h, rgba: new Uint8Array(g.getImageData(0, 0, w, h).data) };
  };
  had = { image: paint(null), stamp: `${key} plain` };
  pictures.set(key, had);
  if (p.login) {
    const face = new Image();
    face.crossOrigin = "anonymous";
    face.onload = () => { had.image = paint(face); had.stamp = `${key} face`; drawPeople(); };
    face.src = p.picture;
  }
  return had;
}

/** The peers to draw: who has said where they are, is not long silent and is shown, and is not who this view is following. */
function shownIn(v) {
  const now = performance.now(), out = [];
  for (const [peer, m] of motion) {
    const who = whoOfPeer(peer), p = who && personOf(who);
    if (!p || !m.cam || now - m.at > STALE_AFTER || !objects.vis(`person:${who}`) || !session.here.has(peer)) continue;
    if (following?.peer === peer && following.v === v) continue;
    out.push({ peer, who, p, m });
  }
  return out;
}
/** Where someone's slate stands, for this view to see it as large whatever the distance: its corners, the first row's ends then the last's. */
function slateIn(v, { cam, aspect, depth }) {
  const at = cam.ortho != null ? vec.add(cam.at, vec.scale(cam.forward, depth)) : cam.at;
  const mine = v.camera(), pixel = mine ? (mine.ortho ? (2 * mine.hh) / mine.h : (2 * Math.tan(mine.fov / 2) * Math.max(0.05, vec.dot(vec.sub(at, mine.eye), mine.forward))) / mine.h) : 0.01;
  const h = (SLATE_SHARE * (mine?.h ?? 600) * pixel) / 2, w = h * aspect, right = vec.unit(vec.cross(cam.forward, cam.up));
  return [[-w, h], [w, h], [-w, -h], [w, -h]].map(([a, b]) => vec.add(at, vec.add(vec.scale(right, a), vec.scale(cam.up, b))));
}
/** The box about points given flat; null of none. */
function extentOf(xyz) {
  if (!xyz.length) return null;
  const lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity];
  for (let i = 0; i < xyz.length; i++) { lo[i % 3] = Math.min(lo[i % 3], xyz[i]); hi[i % 3] = Math.max(hi[i % 3], xyz[i]); }
  return { lo, hi };
}
/** Everyone, as this view sees them. Only what it has changed is sent on: this is done whenever anything moves. */
function drawPeopleIn(v) {
  if (!v.scene || !session) return;
  const pos = [], col = [], markers = [], slates = [], areas = new Map(), list = shownIn(v);
  const line = (a, b, rgb) => { pos.push(...a, ...b); col.push(...rgb, 255, ...rgb, 255); };
  for (const { peer, who, p, m } of list) {
    const rgb = rgbOf(p.colour), c = slateIn(v, m), pic = pictureOf(p, m.aspect);
    slates.push({ key: `person ${peer}`, corners: c, image: pic.image, stamp: pic.stamp });
    areas.set(who, [c[0], c[1], c[3], c[2]]);
    for (const [a, b] of [[0, 1], [1, 3], [3, 2], [2, 0]]) line(c[a], c[b], rgb);
    const top = vec.scale(vec.add(c[0], c[1]), 0.5);
    markers.push({ id: `name ${peer}`, at: top, text: { text: p.name, on: true, colour: [255, 255, 255, 255], background: [...rgb, 235], size: 11 }, tip: `${p.name}: double-click their slate to follow their view` });
    if (m.ray) {
      const o = m.ray.slice(0, 3), d = m.ray.slice(3, 6), end = vec.add(o, vec.scale(d, m.ray[6]));
      line(o, end, rgb);
      markers.push({ id: `cursor ${peer}`, at: end, shape: "dot", size: m.buttons ? 13 : 9, fill: [...rgb, 255], edge: [255, 255, 255, 255], edge_width: 1.5 });
    }
  }
  v.peopleAreas = areas;
  const text = JSON.stringify([pos, col]);
  if (v.peopleOver !== text) {
    v.peopleOver = text;
    v.setOver("people", pos.length ? { pos: new Float32Array(pos), col: new Uint8Array(col), width: RAY_WIDTH } : null);
  }
  const said = JSON.stringify(markers);
  if (v.peopleMarkers !== said) {
    v.peopleMarkers = said;
    v.setMarkers("people", markers.length ? { markers } : null);
  }
  const placed = JSON.stringify(slates.map((s) => [s.key, s.corners, s.stamp]));
  if (v.peopleSlates !== placed) {
    v.peopleSlates = placed;
    v.setSlateSet("people", slates);
    // (They are among what is drawn, for how near and far the view sees: they may well be away from the model.)
    v.setExtent("people", extentOf(pos));
  }
}
function drawPeople() {
  for (const v of Object.values(views)) drawPeopleIn(v);
}

// ----- following ---------------------------------------------------------------------------------------------------------------

/** Whom following this person comes to: whoever they are following, and so on (none of it round to this person). */
function followedThrough(who) {
  const seen = new Set([session.me.who]);
  for (let at = who; ;) {
    seen.add(at);
    const next = session.here.get(session.peerOf(at))?.following;
    if (!next || seen.has(next)) return at;
    at = next;
  }
}
/** Follows someone in a view: their camera is its camera till it is moved. Whether they could be followed. */
export function follow(who, v = theView()) {
  who = followedThrough(who);
  const peer = session.peerOf(who), p = personOf(who);
  if (!peer || !p || who === session.me.who) return false;
  if (following) stop();
  bring(v.key);
  following = { who, peer, v, sync: false, mine: { camera: v.get().camera }, expected: null };
  v.scene?.watch?.(rgbOf(p.colour).map((x) => x / 255));
  bindBanner();
  session.present({ following: who });
  const m = motion.get(peer);
  if (m?.cam) track(m);
  showDoc();
  renderBanner();
  drawPeople();
  renderTree();
  return true;
}
/** Let go of them: the view stays where it is, what is visible is the viewer's own again. */
export function stop() {
  const f = following;
  if (!f) return;
  following = null;
  session.present({ following: null });
  if (f.sync) unsync(f);
  f.v.scene?.watch?.(undefined);
  f.banner?.remove();
  f.frame?.remove();
  drawPeople();
  renderTree();
  renderTools(f.v);
}
function track(m) {
  const f = following, cam = { at: m.cam.at, forward: m.cam.forward, up: m.cam.up, ortho: m.cam.ortho, flat: false, photo: null };
  f.expected = cam;
  f.v.track(cam);
  placeFrame(m);
}
/** The view moved by something other than what it is following: the person watching has gone their own way. */
function checkSteered(v) {
  const f = following;
  if (f?.v !== v || !f.expected) return;
  const c = v.scene?.cameraState();
  if (!c) return;
  const off = (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);
  if (off(c.at, f.expected.at) > STEERED || off(c.forward, f.expected.forward) > STEERED || off(c.up, f.expected.up) > STEERED || Math.abs((c.ortho ?? 0) - (f.expected.ortho ?? 0)) > STEERED) stop();
}
/** Their pointer, in this canvas's pixels: a view fits their height, and is centred across, whatever size each is. */
function inMine(f, m, n) {
  const c = f.v.canvas, h = c.height, w = c.width, theirW = h * (m.aspect ?? w / h);
  return { x: w / 2 + (n[0] - 0.5) * theirW, y: n[1] * h };
}
function watchEvents(f, m, events) {
  const out = [];
  for (const e of events) {
    if (e.t === "move") out.push({ t: "move", ...inMine(f, m, e.n) });
    else if (e.t === "press") out.push({ t: "press", button: e.button, ...inMine(f, m, e.n) });
    else if (e.t === "release") out.push({ t: "release", button: e.button });
    else if (e.t === "scroll") out.push({ t: "scroll", dots: e.dots });
    else if (e.t === "modifiers") out.push({ t: "modifiers", held: e.held });
    else if (e.t === "key") out.push({ t: "key", label: e.label });
    else if (e.t === "text") out.push({ t: "text", text: e.text });
    else if (e.t === "left") out.push({ t: "left" });
  }
  if (out.length) { f.v.scene?.watched?.(out); f.v.redraw(); }
}
/** Their frame, outlined, where their view is another shape from this one. */
function placeFrame(m) {
  const f = following, c = f.v.canvas, h = c.clientHeight, w = c.clientWidth, theirW = h * (m.aspect ?? w / h);
  f.frame ??= f.v.host.appendChild(el("div", { class: "theirframe" }));
  f.frame.style.setProperty("--who", personOf(f.who)?.colour ?? "#888");
  f.frame.style.left = `${w / 2 - theirW / 2}px`;
  f.frame.style.width = `${theirW}px`;
}
/** What they have in front of them, brought up: a drawing, or a file; else the view that follows them. */
function showDoc() {
  const f = following;
  if (!f) return;
  const said = session.here.get(f.peer);
  if (said?.doc) openDoc(said.doc); else bring(f.v.key);
}

// ----- what is visible, as they have it ----------------------------------------------------------------------------------------

function applySync(f) {
  const said = session.here.get(f.peer)?.visible, v = f.v;
  if (!said) return;
  objects.borrow(said.hidden, () => { if (following === f && f.sync) { f.sync = false; restoreView(f); renderBanner(); } });
  if (said.slab) {
    const s = tempSlab({ centre: said.slab.centre, axes: said.slab.axes, half: said.slab.half });
    s.filter = said.slab.filter;
    v.slab = s.id;
  } else v.slab = null;
  syncSlabs();
  if (said.display) { v.display = { ...said.display }; displayExact(v); }
  if (said.photo) viewPhoto(said.photo, v);
  applyVisibility();
}
function restoreView(f) {
  const v = f.v;
  v.slab = f.mine.slab ?? null;
  v.display = f.mine.display;
  syncSlabs();
  displayExact(v);
  applyVisibility();
}
function unsync(f) {
  f.sync = false;
  objects.giveBack();
  restoreView(f);
}
function setSync(on) {
  const f = following;
  if (!f || f.sync === on) return;
  if (on) {
    f.mine.slab = f.v.slab;
    f.mine.display = { ...f.v.display };
    f.sync = true;
    applySync(f);
  } else unsync(f);
  renderBanner();
}

// ----- the banner -------------------------------------------------------------------------------------------------------------------

function bindBanner() {
  const f = following;
  f.banner = el("div", { class: "followbar" });
  f.v.pane.append(f.banner);
  f.banner.addEventListener("click", (e) => {
    const a = e.target.closest("[data-a]")?.dataset.a;
    if (a === "stop") stop();
  });
  f.banner.addEventListener("change", (e) => { if (e.target.dataset.a === "sync") setSync(e.target.checked); });
}
function renderBanner() {
  const f = following;
  if (!f?.banner) return;
  const p = personOf(f.who), said = session.here.get(f.peer);
  f.banner.style.setProperty("--who", p?.colour ?? "#888");
  // (Clear of the view's own controls, as they wrap.)
  f.banner.style.top = `${f.v.tools.offsetTop + f.v.tools.offsetHeight + 8}px`;
  const where = said?.doc ? ` · on ${esc(docName(said.doc))}` : "";
  f.banner.innerHTML = `<b>Following ${esc(p?.name ?? "someone")}</b><span class="muted">${where}</span><label title="See what they see of the model: what is hidden, what the view is clipped to, how bodies are drawn. Your own settings come back when this is off"><input type="checkbox" data-a="sync"${f.sync ? " checked" : ""}> Same as theirs</label><button data-a="stop" title="Stop following (moving this view does too)">Stop</button>`;
}

// ----- heard --------------------------------------------------------------------------------------------------------------------

function heard(peer, p) {
  const m = motion.get(peer) ?? {};
  motion.set(peer, m);
  m.at = performance.now();
  if (p.c) { m.cam = p.c; m.aspect = p.a; m.depth = p.d; }
  m.ray = p.r ?? null;
  m.buttons = p.b ?? 0;
  const f = following?.peer === peer ? following : null;
  if (f) {
    if (p.c) track(m);
    if (p.e) watchEvents(f, m, p.e);
  }
  // (All views, as it comes: it is waiting that would make it late. Each draws at its next frame.)
  drawPeople();
}
/** Who is here, and what they say of themselves, has changed. */
function peopleChanged() {
  if (following) {
    if (!session.here.has(following.peer)) stop();
    else {
      const through = followedThrough(following.who);
      if (through !== following.who) follow(through, following.v);
      else {
        showDoc();
        if (following.sync) applySync(following);
        renderBanner();
      }
    }
  }
  for (const peer of [...motion.keys()]) if (!session.here.has(peer)) motion.delete(peer);
  showSharing();
  drawPeople();
  renderTree();
}

// ----- as a kind of thing in the scene --------------------------------------------------------------------------------------------

const stateOfPeer = (peer) => mesh.stateOf(peer);
/** What a document is called, briefly (its key: `file:a/b.ts`, `sheet:A-101`). */
const docName = (key) => key.slice(key.indexOf(":") + 1).split("/").pop();
function noteOf(p) {
  const peer = session.peerOf(p.id);
  if (p.id === session.me.who) return "you";
  if (!peer) return "offline";
  if (following?.who === p.id) return "following";
  const s = stateOfPeer(peer);
  if (s === "failed") return "can't connect directly";
  if (s !== "open") return "connecting…";
  const doc = session.here.get(peer)?.doc;
  return doc ? `on ${docName(doc)}` : "here";
}
kind("person", {
  field: "person", group: "participants", order: 3,
  key: (id) => `person:${id}`,
  find: (id) => personOf(id),
  title: (p) => p.name,
  lookThrough: true,
  card: (p) => ({ label: "Participant", id: p.name, note: noteOf(p), body: `<a href="#" data-follow>Follow their view</a>`, bind: (chip) => { const a = chip.querySelector("a[data-follow]"); if (a) a.onclick = (e) => { e.preventDefault(); follow(p.id); }; } }),
  info: (p) => ({ what: "Participant", name: p.name, id: p.login ?? "", rows: [["Now", noteOf(p)]], note: "Double-click their slate, or click their icon at the top right, to follow their view." }),
  where: () => [],
  rows(row, list) {
    if (!session) return "";
    const everyone = session.people();
    return list("participants", "Participants", () => everyone.map((p) => row(1, "person", `person:${p.id}`, p.name, { id: p.id, on: sel.person === p.id, note: noteOf(p), noEye: p.id === session.me.who, title: p.id === session.me.who ? "You" : `${p.name}. Twice: follow their view` })), { note: everyone.filter((p) => p.presence === "here").length, noEye: true, title: "Everyone who has taken part in this session" });
  },
  goto(id) {
    if (!follow(id)) choose("person", id);
  },
  menu(id, m) {
    if (session.peerOf(id)) m.add("Follow their view", () => follow(id));
    if (id !== session.me.who) m.add("Send a message", () => personActions.chat?.(id));
  },
  near(out, v) {
    for (const [who, q] of v.peopleAreas ?? []) out.area(who, q);
  },
});

/** How the connections to others stand. */
export const connections = () => mesh?.report() ?? [];

/** Starts it: a page's own session, told of everything from here. */
export function attach(s) {
  session = s;
  mesh = new Mesh({
    signal: (peer, value) => session.room.signal(peer, value),
    on: { packet: heard, state: () => { renderTree(); if (mesh.anyone) { lastCamera = null; sendSoon(); } } },
  });
  const peers = () => mesh.peersAre(session.peer, session.others());
  session.on.peers = peers;
  const was = session.on.people;
  // (Someone found to be the host is no one to connect to.)
  session.on.people = () => { was?.(); peers(); peopleChanged(); };
  peers();
  session.listen((from, value) => { if (value.rtc) mesh.signalled(from, value.rtc); });
  // (Only what happens over a view is sent, and only while someone would hear it.)
  for (const type of ["pointermove", "pointerdown", "pointerup"]) document.addEventListener(type, localPointer, { capture: true, passive: true });
  document.addEventListener("wheel", localWheel, { capture: true, passive: true });
  document.addEventListener("keydown", localKey, true);
  document.addEventListener("keyup", (e) => { if (mesh.anyone && !typingIn(e.target)) { noteHeld(e); sendSoon(); } }, true);
  viewDrawn.add(drawn);
  onLayout(tellDoc);
  objects.on("visibility", () => { const v = theirView(); if (v) tellVisible(v); });
  objects.open.add("participants");
  addKindRows(kindOf("person"));
  personActions.follow = (who) => follow(who);
  personActions.isFollowed = (who) => following?.who === who;
  personActions.menuFor = (who, e, chat) => openMenu([session.peerOf(who) && { label: "Follow their view", run: () => follow(who) }, { label: "Send a message", run: chat }].filter(Boolean), e.clientX, e.clientY);
  tellDoc();
  showSharing();
  renderTree();
  workspace.refresh();
}
