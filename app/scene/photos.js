// Photos of the model: each a picture, with its camera as the solve found it and what is seen in
// it (things of the model at pixels of the picture). A photo is a thing of the scene, as a slab
// is: a row under Photos, a mark in the 3D views (where its camera was, and what it took in),
// what an issue can be pinned to, and what a view can look through: a 3D view through its camera
// is over its picture, with the model drawn as the camera has it (as a view clipped to a slab is
// over the scan's points there).
//
// A photo is a part script's (`photo` of topo-part: a camera part, its picture and how it sees);
// the exact view draws through its camera, its picture in the scene behind the bodies (exact.js).
// In such a view a thing is said to be seen by dragging it to where it is in the picture, as a
// face is dragged to where a scan has it: an edge of a part's body (anywhere along it: it is the
// edge that is lit under the cursor, and the edge that is said), or a corner (the edges that meet
// there, each seen there), as the exact view finds them through the camera. What is said is kept
// in a file beside the model (`NAME.sightings.gen.ts`), which the model says (`seenIn(photo,
// sightings)`): the model is built again with it, and the camera, and whatever of the model is
// unknown, move to meet it.
//
// Building it again is a fit, and may take seconds: it is not waited for. What is said is drawn
// where it was said as soon as it is (in another colour, till the model is built with it: `ahead`),
// and more can be said, moved and taken back meanwhile: the model is built with all of it next.
// How far the fit has come is shown in the view (and in the page's header: shell/panel.js).

import { kind, kindOf } from "./kinds.js";
import { bring, openDoc } from "../shell/dock.js";
import { model, project, storage } from "../project.js";
import { freshNumber } from "../store.js";
import { esc, openMenu } from "../ui.js";
import { msg, showSolving } from "../shell/panel.js";
import { workspace } from "../workspace.js";
import { active, rot, sel, setActive, toScan, views } from "./state.js";
import { choose } from "./chosen.js";
import { renderTree } from "./tree.js";
import { rebuildMarks } from "./marks.js";
import { pushHistory } from "./links.js";
import { syncSlabs } from "./slabs.js";
import { renderLayers, viewKey } from "./views.js";
import { members, modelBounds, rebuildModel } from "./parts.js";
import { findExact, lensRays, sightOff } from "./exact.js";
import { frameSrc, keepFrame, slateOfPhoto } from "./sessions.js";
import { vis } from "../objects.js";

/** The model's photos, as the engine gives them (`photos`). */
let photos = [];
/** Frames of a recording looked through, not (yet) kept: photos that the model does not say (sessions.js). */
const offered = [];
/** A frame of a recording, to be looked through as a photo is. */
export function offerFrame(p) {
  const was = offered.findIndex((q) => q.name === p.name);
  if (was >= 0) offered.splice(was, 1);
  offered.push(p);
  // (Only the one looked through and those before it that are kept matter: a few.)
  while (offered.length > 4) { const gone = offered.shift(); photos = photos.filter((q) => q !== gone); }
  if (!photos.some((q) => q.name === p.name)) photos.push(p);
}
/** Its pictures read: whatever shows them, again. */
export function photosChanged() {
  refresh();
  rebuildMarks();
}
/** The frames of a session the model has kept. */
export const framesOfSession = (name) => photos.filter((p) => p.image?.session === name && !p.unkept);
const find = (name) => photos.find((p) => p.name === name) ?? null;
const v3 = (a) => (Array.isArray(a) ? a : [a.x, a.y, a.z]);
const add = (a, b, k = 1) => [a[0] + k * b[0], a[1] + k * b[1], a[2] + k * b[2]];

/** Has the engine say them again (the model was built again), and draws again whatever shows them. */
export async function loadPhotos() {
  photos = model.built ? await model.ask("photos").catch(() => []) : [];
  if (!Array.isArray(photos)) photos = [];
  // (A frame looked through and not kept is one more, as long as it is not of the model.)
  photos.push(...offered.filter((p) => !photos.some((q) => q.name === p.name)));
  // (Built with all that was said here, or as nearly as it will be: nothing more is drawn ahead of it.)
  if (!model.busy && !writing) said = null;
  // (What was pointed at is by its place among what is seen, which may now be another's.)
  pointed = null;
  refresh();
  renderTree();
  rebuildMarks();
}

/**
 * The corners of what its camera took in, a little way off from it (a fifth of how far it is from the model's
 * origin: a mark of where the camera is and which way it looks, that stops short of what it looks at).
 */
/** Whether a photo's mark (and, chosen, its picture) is drawn: as its eye and the eye of the photos' group say. */
const shownPhoto = (p) => vis("photos", `photo:${p.name}`);
// The model's extent, kept while its parts are the same (a picture drawn to scale hangs beyond it).
let boundsOf = { of: undefined, b: null };
const bounds = () => {
  if (boundsOf.of !== members) boundsOf = { of: members, b: modelBounds() };
  return boundsOf.b;
};
function frustum(p) {
  const o = v3(p.pose.origin), [w, h] = p.size;
  // (Seen parallel, a drawing to scale: the picture as big as what it shows, and beyond it, a backdrop, the way it
  // looks: just past the far side of the model, so that it is behind what it shows and not over it. Otherwise, a
  // little way along.)
  const b = p.lens.parallel ? bounds() : null;
  const corners = b ? [0, 1, 2, 3, 4, 5, 6, 7].map((k) => [(k & 1 ? b.hi : b.lo)[0], (k & 2 ? b.hi : b.lo)[1], (k & 4 ? b.hi : b.lo)[2]]) : [];
  const far = b ? Math.max(...corners.map((q) => dot3(add(q, o, -1), v3(p.pose.z)))) + 0.02 * b.radius : 0.2 * Math.max(p.away, 0.01);
  // (Each corner along its own line of sight, as the lens has it, as far off along the way it looked. Before the lenses are
  // loaded, about as large.)
  const r = lensRays(p.lens, [0, 0, w, 0, w, h, 0, h]);
  if (!r) { const by = (p.lens.parallel ? 1 : far) / p.lens.resolution; return [[0, 0], [w, 0], [w, h], [0, h]].map(([u, v]) => add(add(add(o, v3(p.pose.z), far), v3(p.pose.x), by * (u - w / 2)), v3(p.pose.y), by * (v - h / 2))); }
  return [0, 1, 2, 3].map((k) => { const [ox, oy, , dx, dy, dz] = r.slice(6 * k, 6 * k + 6), along = far / dz; return add(add(add(o, v3(p.pose.z), far), v3(p.pose.x), ox + along * dx), v3(p.pose.y), oy + along * dy); });
}
/**
 * A photo's picture, small (some forty patches across, each one colour): to draw where its camera's mark has what
 * it took in. Read once for each picture, and the marks are drawn again when it is. Null: not read yet, or not to
 * be read (a picture from somewhere that does not let a page read what it shows).
 */
const smalls = new Map();
function small(p) {
  const src = href(p);
  if (!src) return null;
  if (smalls.has(src)) return smalls.get(src);
  smalls.set(src, null);
  const img = new Image();
  img.crossOrigin = "anonymous";
  // (One that is not to be read does not load at all, asked for so: and is left as not read.)
  img.onload = () => {
    const w = 40, h = Math.max(1, Math.round((w * p.size[1]) / p.size[0])), c = Object.assign(document.createElement("canvas"), { width: w, height: h }), g = c.getContext("2d");
    g.drawImage(img, 0, 0, w, h);
    smalls.set(src, { w, h, data: g.getImageData(0, 0, w, h).data });
    rebuildMarks();
  };
  img.src = src;
  return null;
}
/** A thing being dragged to where it is, or what is seen being moved: { id, pixel (of the picture, under the cursor), x, y, moved }. */
let pull = null;
const dot3 = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
/**
 * What is off, of a thing seen at a pixel: the place of the thing whose image is nearest where it was seen, and where it was
 * seen (on the pixel's ray, as far off as that place is). As the fit counts it, through the photo's lens (`sightOff`).
 */
function offOf(p, what, pixel) {
  const out = sightOff({ lens: p.lens, at: v3(p.pose.origin), axes: [v3(p.pose.x), v3(p.pose.y), v3(p.pose.z)], shutter: p.shutter ?? undefined, what, pixel });
  if (!out) { const q = what.at ? v3(what.at) : what.a ? v3(what.a) : what.centre ? v3(what.centre) : v3(what.points[0]); return { on: q, seen: q }; }
  return { on: [out[0], out[1], out[2]], seen: [out[3], out[4], out[5]] };
}
/**
 * How far, as far off as a photo's marks are drawn, one pixel of the screen is, in the view that is through it
 * (where none is, one pixel of the picture).
 */
const onScreen = (p) => (p.lens.parallel ? 1 : Math.max(p.away, 0.01)) / p.lens.resolution / (looking(p.name)[0]?.camera()?.through?.k ?? 1);
/** A circle of the model: where it is at an angle round it. */
function roundOf(c) {
  const a = v3(c.axis), s = Math.abs(a[0]) < 0.9 ? [1, 0, 0] : [0, 1, 0], u0 = [a[1] * s[2] - a[2] * s[1], a[2] * s[0] - a[0] * s[2], a[0] * s[1] - a[1] * s[0]], l = Math.hypot(...u0) || 1, u = u0.map((x) => x / l);
  const la = Math.hypot(...a) || 1, w = [(a[1] * u[2] - a[2] * u[1]) / la, (a[2] * u[0] - a[0] * u[2]) / la, (a[0] * u[1] - a[1] * u[0]) / la], mid = v3(c.centre);
  return (t) => add(add(mid, u, c.radius * Math.cos(t)), w, c.radius * Math.sin(t));
}
/** A circle of the model as places round it: or, of one that is only so much of a circle, along as much as it is (not closed). */
function roundOrArc(c, n = 40) {
  if (!c.arc) return ringOf(c, n);
  const at = roundOf(c), mid = v3(c.centre), u = add(at(0), mid, -1), w = add(at(Math.PI / 2), mid, -1), from = v3(c.arc[0]), start = Math.atan2(dot3(from, w), dot3(from, u));
  const steps = Math.max(2, Math.ceil((n * c.arc[1]) / (2 * Math.PI)));
  return Array.from({ length: steps + 1 }, (_, k) => at(start + (c.arc[1] * k) / steps));
}
/** A circle of the model as places round it. */
const ringOf = (c, n = 40) => { const at = roundOf(c); return Array.from({ length: n }, (_, k) => at((2 * Math.PI * k) / n)); };
/** What of the model was found under the cursor through a photo (`under`), by the id it is lit and dragged by. */
const found = new Map();
/** What a handle is of: the photo, and the thing (as what is seen is said: a point, a line, a circle), or what is seen already. */
function handled(id) {
  const [i, what, k] = id.split("."), p = photos[Number(i)];
  if (!p) return null;
  if (what === "said") { const s = p.sightings[Number(k)]; return s ? { p, said: { ...s, pixel: ahead(p).moved.get(ownOf(s)) ?? s.pixel }, thing: s.what } : null; }
  // (One said here that the model is not yet built with: by its number.)
  if (what === "wait") { const a = ahead(p).added.find((a) => a.n === Number(k)); return a ? { p, said: { name: a.name, what: a.thing, pixel: a.pixel, off: null, within: 3 }, thing: a.thing } : null; }
  const x = found.get(id);
  if (!x) return null;
  const mm = (q) => q.map((c) => c / 1000);
  const thing = x.what === "vertex" ? { kind: "point", at: mm(x.at) } : x.points ? { kind: "curve", points: x.points.map(mm) } : { kind: "line", a: mm(x.ends[0]), b: mm(x.ends[1]) };
  return { p, what: x.what, x, thing };
}
/**
 * What of the model is under the cursor in a view through a photo: a corner of a body (a vertex, within a few pixels),
 * else an edge, straight or round (anywhere along it: a hole's rim), else where a round face turns away from the camera
 * (its outline), as the exact view finds them through the camera: of what is shown, and not behind anything. As a
 * handle's id: the photo's place among the photos, and what it is.
 */
function under(v, e) {
  const i = photos.findIndex((p) => p.name === v.state.photo?.name);
  if (i < 0) return null;
  const r = v.canvas.getBoundingClientRect(), x = e.clientX - r.left, y = e.clientY - r.top;
  const f = findExact(v, x, y, "vertex") ?? findExact(v, x, y, "edge") ?? findExact(v, x, y, "outline");
  if (!f) return null;
  const id = `${i}.found.${f.address}|${f.what}|${f.index}`;
  found.set(id, f);
  return id;
}
/** A view through a photo says what of the model is under the cursor, and lets it be dragged to where it is in the picture. */
function bind(v) {
  if (v.sights) return;
  v.sights = true;
  // (Let go of the photo, the view is of the model as any is: and what was seen through it is not marked in it.)
  v.onPhotoGone = () => { magnify(v, null); if (v.fitbox) v.fitbox.hidden = true; rebuildModel(); rebuildMarks(); };
  const ray = (e) => { const r = v.canvas.getBoundingClientRect(); return v.ray(e.clientX - r.left, e.clientY - r.top); };
  const of = () => kindOf("photo");
  v.onHover = (e) => {
    const id = e && v.state.photo ? under(v, e) : null;
    v.canvas.style.cursor = id ? "grab" : "";
    if (id !== hover) { hover = id; rebuildMarks(); }
  };
  // (What else the view lets be grabbed, where nothing of the photo's is: grab.js.)
  const other = { grab: v.onGrab, move: v.onGrabMove, end: v.onGrabEnd };
  v.onGrab = (e) => {
    const id = v.state.photo ? under(v, e) : null;
    if (!id) return other.grab?.(e) ?? false;
    pointed = null;
    v.grabbed = id;
    of().drag(v, id, "start", ray(e), e);
    return true;
  };
  v.onGrabMove = (e) => (v.grabbed ? of().drag(v, v.grabbed, "move", ray(e), e) : other.move?.(e));
  // (Let go of because the pointer was taken away: nothing is said.)
  v.onGrabEnd = (e, lost) => { if (!v.grabbed) return other.end?.(e, lost); if (lost) dropPull(); else of().drag(v, v.grabbed, "end", ray(e), e); v.grabbed = null; };
  // (What is drawn over the picture is a pixel or two of the screen wide: drawn again as the picture is brought nearer.)
  let again = 0;
  v.onPhotoZoom = () => { cancelAnimationFrame(again); again = requestAnimationFrame(rebuildMarks); };
  // Whether the model is fitted to what is seen, or shown as placed: for whoever looks through a photo.
  // And how strongly the model tints the picture: none, to see the picture's own edges under it.
  const box = document.createElement("div");
  box.className = "fitbox";
  box.innerHTML = `<label title="How strongly the model tints the picture where it is over it. None: only its edges are drawn, and the picture's own show under it."><span>Tint</span> <input type="range" min="0" max="0.6" step="0.02"></label>` +
    `<label title="What is said to be seen from here on is kept left out of the fit: the camera is not moved by it, and how far off it is is still said. To say several things of a camera that is only roughly placed, then fit to them all at once."><input type="checkbox" data-a="leftout"> New ones left out</label>` +
    `<button data-a="allout" title="Leave everything said to be seen in this photo out of the fit (kept: how far off each is is still said)">Leave all out</button><button data-a="allin" title="Fit to everything said to be seen in this photo again">Fit to all</button>`;
  const leftout = box.querySelector("input[data-a=leftout]");
  leftout.checked = sayLeftOut;
  leftout.onchange = () => { sayLeftOut = leftout.checked; for (const o of Object.values(views)) { const c = o.fitbox?.querySelector("input[data-a=leftout]"); if (c) c.checked = sayLeftOut; } };
  box.querySelector("button[data-a=allout]").onclick = () => v.state.photo && leaveAllOut(v.state.photo.name, true);
  box.querySelector("button[data-a=allin]").onclick = () => v.state.photo && leaveAllOut(v.state.photo.name, false);
  // (And, while the model is being built again, how far that has come.)
  box.insertAdjacentHTML("beforeend", `<span class="solving" hidden><i></i></span>`);
  showSolving(box.lastChild, model.solving);
  const tint = box.querySelector("input[type=range]");
  tint.value = v.state.tint;
  tint.oninput = () => { for (const o of Object.values(views)) { o.set({ tint: Number(tint.value) }); const t = o.fitbox?.querySelector("input[type=range]"); if (t && t !== tint) t.value = tint.value; } };
  for (const type of ["pointerdown", "wheel", "dblclick"]) box.addEventListener(type, (e) => e.stopPropagation());
  v.host.appendChild(box);
  v.fitbox = box;
}
/**
 * A glass by the cursor in a view through a photo, while a thing is dragged to where it is seen (or what is seen
 * is moved): the picture under the cursor, larger, with a cross on the very pixel, to put it there to within one.
 * (The picture alone: nothing of the model is over it.)
 */
let glass = null;
function magnify(v, e) {
  const q = e && v.state.photo;
  if (!q?.src) { if (glass) glass.style.display = "none"; return; }
  if (!glass) {
    glass = document.createElement("div");
    glass.className = "glass";
    glass.innerHTML = "<i></i><b></b>";
    document.body.appendChild(glass);
  }
  const r = v.canvas.getBoundingClientRect(), cam = v.camera(), [px, py] = v.pixel(cam, e.clientX - r.left, e.clientY - r.top);
  // (Four times as large as the view has it, and never less than four of the screen's pixels to one of the picture's;
  // nor, where the view is itself brought near, more than a dozen, or half as large again as the view.)
  const SIZE = 132, k = cam.through.k, m = Math.min(Math.max(4, 4 * k), Math.max(12, 1.5 * k));
  // (Up and to the right of the cursor; to the other side of it near the window's edge.)
  const left = e.clientX + 24 + SIZE > window.innerWidth ? e.clientX - 24 - SIZE : e.clientX + 24, top = e.clientY - 24 - SIZE < 0 ? e.clientY + 24 : e.clientY - 24 - SIZE;
  Object.assign(glass.style, { display: "", left: `${left}px`, top: `${top}px`, backgroundImage: `url(${JSON.stringify(q.src)})`, backgroundSize: `${q.size[0] * m}px ${q.size[1] * m}px`, backgroundPosition: `${(SIZE / 2 - px * m).toFixed(1)}px ${(SIZE / 2 - py * m).toFixed(1)}px` });
}
// A drag is let go of with nothing said, by Shift or Escape while it is held. And what is said here is taken back
// and said again, one step at a time: ⌘Z (or Ctrl+Z), and with Shift (or Ctrl+Y); not while something is typed in.
function dropPull() {
  if (!pull) return;
  pull = null;
  for (const v of Object.values(views)) magnify(v, null);
  rebuildMarks();
}
window.addEventListener("keydown", (e) => {
  if (pull && (e.key === "Shift" || e.key === "Escape")) return dropPull();
  // (In the view that is being worked in, where that is through a photo: not wherever one happens to be open.)
  const typing = /^(INPUT|TEXTAREA|SELECT)$/.test(e.target?.tagName ?? "") || e.target?.isContentEditable;
  // (On a branch, what is said is taken back as all else is: by walking its history.)
  if (typing || !(e.metaKey || e.ctrlKey) || !views[active]?.state.photo || storage.sharedWithOthers()) return;
  const k = e.key.toLowerCase(), redo = (k === "z" && e.shiftKey) || k === "y";
  if (k !== "z" && !redo) return;
  e.preventDefault();
  step(redo);
});
/** What of the model the cursor is on, through a photo (as `under` says it), or the thing seen that it is on. */
let hover = null;
/**
 * A reading that is pointed at from elsewhere (an issue that says it does not agree with the rest): lit as what the
 * cursor is on is, until another is, the cursor takes hold of something, or the model is built again. As a handle's id.
 */
let pointed = null;
const worst = (p) => p.sightings.reduce((m, s) => (s.disabled || s.off == null ? m : Math.max(m, s.off)), 0);
/** How far off a reading is, said (unknown, where its thing is no longer there to say). */
const offSaid = (s) => (s.off == null ? "how far off is not known" : `${s.off.toFixed(1)} px off`);
const short = (name) => name.split(": ").pop();

// ----- what is said in the workspace: the file beside the model ------------------------------------------

const HEADER = (stem) => `// Things seen in the model's photos, said in the workspace (which writes this file): each a reading that an edge of a
// part's body (by its address and the edge's name), or a corner where edges meet, is at a pixel of a photo. The model
// says them: \`import { sightings } from "./${stem}.sightings.gen"\` and \`says: { ...seenIn(photo, sightings) }\`.
import type { Sighting } from "topo-part";

export const sightings: Sighting[] = [
`;
const num = (x) => String(+(+x).toFixed(5));
/** The file as `entries` last read it: what is written is made from that. */
let asRead = null;
const entries = (src) => ((asRead = src), src ?? "").split("\n").filter((l) => l.trimStart().startsWith("{ photo:")).map((line) => ({ n: Number(line.split("// ").pop().split(":")[0]), line })).filter((e) => e.n > 0);
const sidecar = () => model.state.file.replace(/\.ts$/, ".sightings.gen.ts");
const stemOf = (id) => id.split("/").pop().replace(/\.sightings\.gen\.ts$/, "");
async function write(list) {
  const id = sidecar(), was = await storage.store.read(id), now = HEADER(stemOf(id)) + list.map((e) => e.line + "\n").join("") + "];\n";
  // (What it was is kept, to take this back, with what it was made: and nothing taken back is left to say again.)
  if (was != null) { undone.push({ id, text: was, from: now }); if (undone.length > 200) undone.shift(); }
  redone.length = 0;
  await put(id, now, asRead);
}
/**
 * The file of what is seen is made this: drawn as it then is at once, kept, and the model built again with it
 * (now, not when its files are next looked at; and not waited for).
 */
async function put(id, text, base) {
  const before = said;
  writing++;
  say_ahead(id, text);
  try {
    await storage.store.write(id, text, { base });
    await project.touched();
  } catch (e) {
    said = before;
    rebuildMarks();
    throw e;
  } finally {
    writing--;
  }
}
/**
 * What the file of what is seen was last made here, while the model is not yet built with it: `{ id, lines }`,
 * each line by its number `{ photo, name, pixel }`. And how many writings of it are not yet done.
 */
let said = null, writing = 0;
function say_ahead(id, text) {
  const lines = new Map();
  for (const e of entries(text)) {
    const m = e.line.match(/\{ photo: ("(?:[^"\\]|\\.)*"), name: ("(?:[^"\\]|\\.)*"), pixel: \[([^\]]*)\]/);
    if (m) lines.set(e.n, { photo: JSON.parse(m[1]), name: JSON.parse(m[2]), pixel: m[3].split(",").map(Number) });
  }
  said = { id, lines };
  rebuildMarks();
}
/** The things said here to be seen, each as it was when it was dragged (a point, a line, a circle), by its file and number: to draw one before the model is built with it. */
const things = new Map();
/** The number of a thing seen that was said in the workspace (its name ends with it, in brackets, but for the edge
 *  of a corner's that it is); 0: it was said in the script. */
const ownOf = (s) => Number(s.name.match(/\[(\d+)\](?: \([^)]*\))?$/)?.[1]) || 0;
/**
 * What is said here of a photo that the model is not yet built with: `gone` (the numbers of those taken back),
 * `moved` (of those seen at another pixel, where), `added` (those it has not got: `{ n, name, thing, pixel }`).
 * By what the file was made, against what the model as built has: so it is nothing once the model is built with it.
 */
function ahead(p) {
  const out = { gone: new Set(), moved: new Map(), added: [] };
  if (!said || said.id !== sidecar()) return out;
  const built = new Set();
  for (const s of p.sightings) {
    const n = ownOf(s), l = n && said.lines.get(n);
    if (!n) continue;
    built.add(n);
    if (!l || l.photo !== p.name) out.gone.add(n);
    else if (Math.hypot(l.pixel[0] - s.pixel[0], l.pixel[1] - s.pixel[1]) > 1e-4) out.moved.set(n, l.pixel);
  }
  for (const [n, l] of said.lines) {
    const thing = l.photo === p.name && !built.has(n) && things.get(`${said.id}#${n}`);
    if (thing) out.added.push({ n, name: l.name, thing, pixel: l.pixel });
  }
  return out;
}
model.onSolving((s) => { for (const v of Object.values(views)) if (v.fitbox) showSolving(v.fitbox.querySelector(".solving"), s); });
/**
 * The changes made here to the file of what is seen, the last last, and those taken back: each the file it is of,
 * what the file is put back to (`text`), and what the file is to be when that is done (`from`: what the change
 * left; if it is anything else, someone else has changed it since, an agent or an editor, and it is not put back
 * over what they said).
 */
const undone = [], redone = [];
/**
 * One change to the file at a time: each reads it, changes what it read and writes it, so two begun together (two
 * things taken back one after the other, before the first is written) would each write the file without what
 * the other did, and the first would be undone by the second. Each waits for the one before it.
 */
let turn = Promise.resolve();
const inTurn = (f) => (...a) => (turn = turn.then(() => f(...a), () => f(...a)));
/** What was taken back here, of which file: one still drawn (the model is not yet built again without it) is not said to be the script's. */
const gone = new Set();
/** Takes back the last change made here to what is seen (or, `again`: makes the last one taken back again). */
const step = inTurn(async (again) => {
  const [from, to] = again ? [redone, undone] : [undone, redone], id = sidecar();
  // (Of the model that is open: what was changed of another is that one's, and is kept for it.)
  const k = from.findLastIndex((x) => x.id === id);
  if (k < 0) return msg(again ? "Nothing to say again." : "Nothing said here to take back.");
  const [s] = from.splice(k, 1), is = (await storage.store.read(id)) ?? "";
  if (is !== s.from) {
    for (const stack of [undone, redone]) for (let i = stack.length - 1; i >= 0; i--) if (stack[i].id === id) stack.splice(i, 1);
    return msg("What is seen was changed elsewhere since (by an agent, or in an editor): it is left as it is.", true);
  }
  to.push({ id, text: is, from: s.text });
  await put(id, s.text);
  gone.clear();
  msg(`${again ? "Said again" : "Taken back"}: the model is built again${again ? "" : " without it"}. (${again ? "⌘Z takes it back" : "⇧⌘Z says it again"}.)`);
});
/** Whether the model adds what is said here; where it does not, what to add to it is said (the file is made, so that it can). */
async function added() {
  const id = sidecar(), stem = stemOf(id);
  if ((await storage.store.read(id)) == null) await write([]);
  const src = (await storage.store.read(model.state.file)) ?? "";
  if (src.includes(`./${stem}.sightings.gen`) && /seenIn(Frames)?\(/.test(src)) return true;
  msg(`The model does not say what is seen here yet. Add to it, once: import { sightings } from "./${stem}.sightings.gen"; and, in what the part with the photo says, ...seenIn(photo, sightings).`, true);
  return false;
}
/** A thing of the model is seen at a pixel of a photo: kept, and the model built again with it. */
const say = inTurn(async (photo, what, label, pixel, thing) => {
  if (!(await added())) return;
  // (A frame of a recording is kept as something is first said to be seen in it.)
  if (photo.unkept && !(await keepFrame(photo))) return;
  const list = entries(await storage.store.read(sidecar())), next = freshNumber(list.map((e) => e.n), storage.sharedWithOthers());
  things.set(`${sidecar()}#${next}`, thing);
  const who = workspace.me?.().name ?? "someone", when = new Date().toISOString().slice(0, 16).replace("T", " ") + " UTC";
  list.push({ n: next, line: `  { photo: ${JSON.stringify(photo.name)}, name: ${JSON.stringify(`${label} [${next}]`)}, pixel: [${num(pixel[0])}, ${num(pixel[1])}], within: 3,${sayLeftOut ? " disabled: true," : ""} what: ${what} }, // ${next}: ${who}, ${when}` });
  await write(list);
  msg(`${label} is seen at (${Math.round(pixel[0])}, ${Math.round(pixel[1])}) of ${photo.name}: ${sayLeftOut ? "kept, left out of the fit (how far off it is is said)" : "the model is built again with it"}.`);
});
/** One said here is seen at another pixel (it was dragged): its line of the file, with that. */
const moveTo = inTurn(async (n, pixel) => {
  const list = entries(await storage.store.read(sidecar())), e = list.find((x) => x.n === n);
  if (!e) return msg(gone.has(`${sidecar()}#${n}`) ? `Sighting ${n} was taken back: the model is being built again without it.` : "That one is not in the file of what is seen (beside the model) but said in its script: move it into that file to drag it here.", true);
  e.line = e.line.replace(/pixel: \[[^\]]*\]/, `pixel: [${num(pixel[0])}, ${num(pixel[1])}]`);
  await write(list);
  msg(`Sighting ${n} is now at (${Math.round(pixel[0])}, ${Math.round(pixel[1])}): the model is built again with it.`);
});
/** One said here is left out of the fit, and kept (`out`), or fitted to again: its line of the file, with `disabled: true` or without. */
const leaveOut = inTurn(async (n, out) => {
  const list = entries(await storage.store.read(sidecar())), e = list.find((x) => x.n === n);
  if (!e) return msg(`Sighting ${n} is not in the file of what is seen (beside the model).`, true);
  e.line = e.line.replace(/ disabled: true,/, "");
  if (out) e.line = e.line.replace(/(within: [^,]*,)/, "$1 disabled: true,");
  await write(list);
  msg(`Sighting ${n} is ${out ? "left out of the fit (and kept: how far off it is is still said)" : "fitted to again"}: the model is built again ${out ? "without" : "with"} it.`);
});
/** One said here sighted to its line segment (`on`: between the edge's ends), or to its whole line: its line of the
 *  file, with `segment: true` or without. */
const sightToSegment = inTurn(async (n, on) => {
  const list = entries(await storage.store.read(sidecar())), e = list.find((x) => x.n === n);
  if (!e) return msg(`Sighting ${n} is not in the file of what is seen (beside the model).`, true);
  e.line = e.line.replace(/ segment: true,/, "");
  if (on) e.line = e.line.replace(/(within: [^,]*,)/, "$1 segment: true,");
  await write(list);
  msg(`Sighting ${n} is ${on ? "sighted to its line segment: past the edge's ends it is off by how far it is from the nearer end" : "sighted to the edge's whole line"}: the model is built again with it.`);
});
/** Every one said here of a photo left out of the fit (`out`), or fitted to again: each line of the file. */
const leaveAllOut = inTurn(async (photo, out) => {
  const list = entries(await storage.store.read(sidecar())), mine = list.filter((e) => e.line.includes(`{ photo: ${JSON.stringify(photo)},`));
  if (!mine.length) return msg(`Nothing said here is seen in ${photo}.`, true);
  for (const e of mine) {
    e.line = e.line.replace(/ disabled: true,/, "");
    if (out) e.line = e.line.replace(/(within: [^,]*,)/, "$1 disabled: true,");
  }
  await write(list);
  msg(`What is seen in ${photo} (${mine.length} said here) is ${out ? "left out of the fit, and kept: how far off each is is still said" : "fitted to again"}: the model is built again ${out ? "without" : "with"} it.`);
});
/** Whether what is said to be seen from here on is said left out of the fit (to place a camera about right first, from
 *  several things, before it is fitted to them all at once). */
let sayLeftOut = false;

/** Takes back one said here (by the number its name ends with). */
const takeBack = inTurn(async (n) => {
  const list = entries(await storage.store.read(sidecar()));
  if (!list.some((e) => e.n === n)) return msg(gone.has(`${sidecar()}#${n}`) ? `Sighting ${n} is taken back already: the model is being built again without it.` : "That one is not in the file of what is seen (beside the model) but said in its script: it is taken out there.", true);
  await write(list.filter((e) => e.n !== n));
  gone.add(`${sidecar()}#${n}`);
  msg(`Took back sighting ${n}: the model is built again without it.`);
});

// ----- its view: a 3D view, through its camera, over its picture ------------------------------------------

/** Where the picture is for the page: on the web as it is; a file beside the model, through the site's files. */
function href(p) {
  if (typeof p.image === "object") return frameSrc(p.image);
  if (/^https?:\/\//.test(p.image)) return p.image;
  const dir = model.state.file.split("/").slice(0, -1).join("/");
  return storage.site && storage.store === storage.site ? storage.site.url(`${dir ? dir + "/" : ""}${p.image}`) : "";
}
/** A photo's camera as a view looks through it (the scan's frame, as the views draw); `was`: how the view had it, kept. */
const through = (p, was) => ({ name: p.name, origin: toScan(v3(p.pose.origin)), x: rot(v3(p.pose.x)), y: rot(v3(p.pose.y)), z: rot(v3(p.pose.z)), lens: p.lens, size: p.size, src: href(p), pan: was?.pan ?? null, zoom: was?.zoom ?? 1, shutter: p.shutter ?? undefined });
const looking = (name) => Object.values(views).filter((v) => v.state.photo?.name === name);
/**
 * Looks through a photo's camera, in a view: the one that is already, or the one last through another photo's,
 * else a view of its own beside what is open. The model is drawn over the picture as the camera has it; a drag
 * slides the picture, and the wheel zooms it.
 */
export function viewPhoto(name, v) {
  const p = find(name);
  if (!p) return null;
  const all = Object.values(views);
  v ??= looking(name)[0] ?? all.find((x) => x.state.photo) ?? openDoc(viewKey(true), { beside: true }).view;
  bring(v.key);
  if (v.state.photo?.name !== name) pushHistory(v);
  setActive(v.name);
  if (v.slab) { v.slab = null; syncSlabs(); }
  // (Where the picture is panned and zoomed to now, as the view's camera has it.)
  v.syncCamera();
  v.set({ photo: through(p, v.state.photo?.name === name ? v.state.photo : null) }, true);
  bind(v);
  v.fitbox.hidden = false;
  rebuildModel(); renderLayers(); rebuildMarks();
  return v;
}
/**
 * Goes to a reading of a photo: looks through the photo, with where the thing was seen in the middle of the view and
 * near enough to tell from its neighbours, and lights it (the thing where the model has it, and what it is off by).
 * `what`: the reading as it is said (`edge of wall #122`). One that is no thing seen (a number of a photo's lens, read to
 * be about what it was given as): the photo alone. Whether there is such a photo.
 */
export function viewReading(photo, what) {
  const i = photos.findIndex((p) => p.name === photo), p = photos[i];
  if (!p) return false;
  const v = viewPhoto(photo), k = p.sightings.findIndex((s) => short(s.name) === what);
  if (!v || k < 0) return true;
  pointed = `${i}.said.${k}`;
  const at = ahead(p).moved.get(ownOf(p.sightings[k])) ?? p.sightings[k].pixel;
  v.set({ photo: { ...v.state.photo, pan: [at[0], at[1]], zoom: Math.max(v.state.photo.zoom || 1, 3) } }, true);
  rebuildMarks();
  return true;
}
/** Views through a photo's camera, once the model is built again: through it as it is now (or let go, if it is gone). */
function refresh() {
  for (const v of Object.values(views)) {
    // (Where the picture is panned and zoomed to now, as the view's camera has it.)
    v.syncCamera();
    const was = v.state.photo, p = was && find(was.name);
    if (was) { v.set({ photo: p ? through(p, was) : null }, true); bind(v); }
    if (v.fitbox) v.fitbox.hidden = !v.state.photo;
  }
}

// ----- the kind ---------------------------------------------------------------------------------------------

/** One of a lens's numbers, as it is said (the page's world in metres: a scale, to a millimetre). */
const lensNumber = (n) => (n.unit === "pixels_per_length" ? `${(n.value / 1000).toFixed(3)} px to a mm` : n.unit === "pixels" ? `${n.value.toFixed(2)} px` : n.value.toFixed(5));
const RGB = [225, 95, 0], CHOSEN = [255, 60, 200], LIVE = [255, 240, 60], DARK = [20, 20, 24], OFF = [255, 170, 40], AHEAD = [90, 200, 255], LEFT_OUT = [150, 150, 150];
kind("photo", {
  field: "photo", group: "photos", order: 46,
  key: (id) => `photo:${id}`,
  find,
  title: (p) => `photo ${p.name}`,
  points: (p) => [v3(p.pose.origin), ...frustum(p)].map(toScan),
  frame: (p) => ({ origin: toScan(v3(p.pose.origin)), axes: [v3(p.pose.x), v3(p.pose.y), v3(p.pose.z)] }),
  info: (p) => ({
    what: "Photo", name: p.name, id: typeof p.image === "object" ? `${p.image.camera} ${p.image.sequence}` : p.image.split("/").pop(),
    rows: [["Picture", `${p.size[0]} × ${p.size[1]} px`], ["Lens", p.lens.label], ...p.lens.params.map((n) => [n.label, lensNumber(n)]), ["Camera", `${Math.round(p.away * 1000)} mm from the model's origin`], ["Seen in it", `${p.sightings.length} thing${p.sightings.length === 1 ? "" : "s"}`], ...(p.sightings.length ? [["Worst off", `${worst(p).toFixed(1)} px`]] : [])],
    note: "Where its camera was, and any of its lens's numbers left free, are found from what is seen in it. Look through it (double-click) to see the model over the picture, and drag an edge or a corner of the model to where it is in the picture.",
  }),
  where: (p) => [{ key: "photo", label: `Photo ${p.name}` }],
  card: (p) => ({
    label: "Photo", id: p.name,
    note: `${p.sightings.length} seen${p.sightings.length ? `, the worst ${worst(p).toFixed(1)} px off` : ""}`,
    body: `Its camera, as what is seen in it puts it: ${Math.round(p.away * 1000)} mm from the model's origin, through its lens: ${esc(p.lens.label.toLowerCase())}.<br><span class="muted">Through it, drag an edge or a corner of the model to where it is in the picture.</span>`,
  }),
  rows(row, list) {
    if (!photos.length) return "";
    const own = photos.filter((p) => typeof p.image !== "object");
    if (!own.length) return "";
    return list("photos", "Photos", () => own.map((p) => row(1, "photo", `photo:${p.name}`, p.name, { id: p.name, on: sel.photo === p.name, note: `${p.sightings.length} seen${p.sightings.length ? `, ≤ ${worst(p).toFixed(0)} px off` : ""}`, title: `A photo of the model (${p.image}): double-click to look through its camera, the model drawn over the picture` })), { note: own.length, title: "Photos of the model: where each camera was is found from what is seen in it" });
  },
  goto(id) {
    if (!find(id)) return;
    choose("photo", id);
    viewPhoto(id);
  },
  menu(id, m, view) {
    if (!find(id)) return;
    m.add("Look through it", () => this.goto(id));
    if (view?.state.photo) m.add("Back to the model in 3D", () => { view.set({ photo: null }); view.onPhotoGone?.(); });
    m.add("Choose it", () => choose("photo", id));
    m.issue("New issue on this photo");
  },
  // A click on the lines of a camera's mark is on that photo.
  near(out) {
    for (const p of photos.filter(shownPhoto)) {
      // (A frame kept: its slate, which its session draws.)
      if (typeof p.image === "object") { const q = !p.unkept && slateOfPhoto(p); if (q) out.area(p.name, q); continue; }
      const o = toScan(v3(p.pose.origin)), c = frustum(p).map(toScan);
      // (Of a drawing to scale, seen parallel, there is no point it is seen from: its picture's outline only.)
      for (let k = 0; k < 4; k++) { if (!p.lens.parallel) out.line(p.name, o, c[k]); out.line(p.name, c[k], c[(k + 1) % 4]); }
    }
  },
  // Where its camera was: from it, the corners of what it took in (of the one chosen, with its picture). And, in
  // the views through it, what is seen in it: from where the model has each thing to where it was seen.
  marks(out) {
    for (const p of photos.filter((q) => shownPhoto(q) && !(q.unkept && !looking(q.name).length))) {
      // (A frame of a recording has its session's slate, not this.)
      if (typeof p.image !== "object") {
      const rgb = p.name === sel.photo ? CHOSEN : RGB, o = toScan(v3(p.pose.origin)), c = frustum(p).map(toScan);
      for (let k = 0; k < 4; k++) { if (!p.lens.parallel) out.line(o, c[k], rgb); out.line(c[k], c[(k + 1) % 4], rgb); }
      // (Which way up its picture is: a mark over the top edge of what it took in.)
      const mid = (a, b, t = 0.5) => add(a, add(b, a, -1), t), up = add(mid(c[0], c[1]), add(c[0], c[3], -1), 0.18);
      out.line(mid(c[0], c[1], 0.35), up, rgb); out.line(up, mid(c[0], c[1], 0.65), rgb);
      // Of the one chosen: its picture, small, where the mark has what it took in. (Not in a view through it:
      // there the picture itself is under the model.)
      const pic = p.name === sel.photo ? small(p) : null;
      if (pic) {
        const at = (u, w) => add(mid(c[0], c[1], u), add(mid(c[3], c[2], u), mid(c[0], c[1], u), -1), w), through = (v) => v.state.photo?.name === p.name;
        for (let j = 0; j < pic.h; j++) for (let i = 0; i < pic.w; i++) {
          const k = 4 * (j * pic.w + i);
          out.fill([at(i / pic.w, j / pic.h), at((i + 1) / pic.w, j / pic.h), at((i + 1) / pic.w, (j + 1) / pic.h), at(i / pic.w, (j + 1) / pic.h)], [pic.data[k], pic.data[k + 1], pic.data[k + 2], 255], through);
        }
      }
      }
      if (!looking(p.name).length) continue;
      // (In the views through it only: a line from a thing to where it was seen is along the picture, and from
      // anywhere else is a line to nowhere.)
      const here = (v) => v.state.photo?.name === p.name;
      // What each is off by, always: from the nearest of the thing to where it was seen, a thin line of the picture, light
      // between dark so that it shows on whatever the picture is of. (One being moved: to where the cursor has it. One
      // the cursor is on is drawn in another colour: `lit`.)
      // (What is said here that the model is not yet built with, as it is said: one taken back is not drawn, one
      // moved is where it was moved to, and one it has not got is from where the model has the thing now.)
      const px = onScreen(p), sides = [v3(p.pose.x), v3(p.pose.y)], i = photos.indexOf(p), a = ahead(p);
      const off = (what, pixel, rgb) => {
        const off = offOf(p, what, pixel);
        for (const [w, c] of [[1, DARK], [-1, DARK], [0, rgb]]) for (const d of sides) out.line(toScan(add(off.on, d, w * px)), toScan(add(off.seen, d, w * px)), c, here);
      };
      p.sightings.forEach((s, k) => {
        // (What the engine says no place of: a cylinder's outline. Its pixel alone is not drawn.)
        if (!s.what) return;
        const n = ownOf(s), moving = pull?.id === `${i}.said.${k}` && pull.moved;
        if (!a.gone.has(n)) off(s.what, moving ? pull.pixel : a.moved.get(n) ?? s.pixel, moving ? LIVE : a.moved.has(n) ? AHEAD : s.disabled ? LEFT_OUT : OFF);
      });
      for (const w of a.added) { const moving = pull?.id === `${i}.wait.${w.n}` && pull.moved; off(w.thing, moving ? pull.pixel : w.pixel, moving ? LIVE : AHEAD); }
    }
    this.lit(out);
  },
  // The thing a handle is of, lit while the cursor is on it or it is dragged: a point (a small cross), an edge
  // (all of it), a circle (round it); and, dragged, what it would be off by where the cursor is.
  lit(out) {
    for (const id of new Set([pointed, hover, pull?.id].filter(Boolean))) {
      const h = handled(id);
      if (!h) continue;
      // (A line of the screen's pixels, one bright between two dark, so that it shows on whatever the picture is of
      // and hides little of it: as wide however near the picture is brought.)
      const px = onScreen(h.p), sides = [v3(h.p.pose.x), v3(h.p.pose.y)], here = (v) => v.state.photo?.name === h.p.name;
      const wide = (a, b) => {
        for (const [k, rgb] of [[1, DARK], [-1, DARK], [0, LIVE]]) for (const d of sides) out.line(toScan(add(a, d, k * px)), toScan(add(b, d, k * px)), rgb, here);
      };
      // And the thing itself in every other view, where that has it: through another photo as wide as here, by that
      // photo's own pixels; in a view of the model, a line of it. (It is the thing of the model that is lit, wherever
      // it is looked at from: to tell which edge of the picture is which of the model, and of another picture.)
      const others = photos.filter((q) => q !== h.p && looking(q.name).length).map((q) => ({ px: onScreen(q), sides: [v3(q.pose.x), v3(q.pose.y)], only: (v) => v.state.photo?.name === q.name }));
      const plain = (v) => !v.state.photo;
      const also = (a, b) => {
        for (const o of others) for (const [k, rgb] of [[1, DARK], [-1, DARK], [0, LIVE]]) for (const d of o.sides) out.line(toScan(add(a, d, k * o.px)), toScan(add(b, d, k * o.px)), rgb, o.only);
        // (In a view of the model: the line, and one beside it each way by a pixel or two of this picture, so that it
        // is more than a hair where the view is about as near.)
        out.line(toScan(a), toScan(b), LIVE, plain);
        for (const d of [[1, 0, 0], [0, 1, 0], [0, 0, 1]]) for (const k of [-1.5, 1.5]) out.line(toScan(add(a, d, k * px)), toScan(add(b, d, k * px)), LIVE, plain);
      };
      const both = (a, b) => { wide(a, b); also(a, b); };
      const t = h.thing;
      if (!t) continue;
      if (t.kind === "line") both(v3(t.a), v3(t.b));
      else if (t.kind === "curve") for (let k = 0; k + 1 < t.points.length; k++) both(v3(t.points[k]), v3(t.points[k + 1]));
      else if (t.kind === "circle") { const r = roundOrArc(t, 120); for (let k = 0; k + (t.arc ? 1 : 0) < r.length; k++) both(r[k], r[(k + 1) % r.length]); }
      else {
        const q = v3(t.at), e = px * 9;
        for (const d of sides) wide(add(q, d, -e), add(q, d, e));
        // (A point: a cross as big as here, along each view's own picture; and along the model's axes in a view of the model.)
        for (const o of others) for (const d of o.sides) for (const [k, rgb] of [[1, DARK], [-1, DARK], [0, LIVE]]) for (const w of o.sides) out.line(toScan(add(add(q, d, -9 * o.px), w, k * o.px)), toScan(add(add(q, d, 9 * o.px), w, k * o.px)), rgb, o.only);
        for (const d of [[1, 0, 0], [0, 1, 0], [0, 0, 1]]) out.line(toScan(add(q, d, -e)), toScan(add(q, d, e)), LIVE, plain);
      }
      // What it is, or would be, off by: from the nearest of it to where the cursor has it (or to where it is seen).
      const at = pull?.id === id && pull.moved ? pull.pixel : h.said ? h.said.pixel : null;
      if (at) { const off = offOf(h.p, t, at); wide(off.on, off.seen); }
    }
  },
  hover(v, id, on) {
    hover = on ? id : hover === id ? null : hover;
    rebuildMarks();
  },
  // Of a photo a view is through: what is seen already, each where it was seen (to drag, to say it better; to
  // take back). In those views only. What of the model can be said to be seen has no handle: it is the thing
  // itself, under the cursor (`under`), that is lit and dragged.
  handles(add) {
    photos.forEach((p, i) => {
      if (!looking(p.name).length) return;
      const only = (v) => v.state.photo?.name === p.name;
      const a = ahead(p), at = (id, what, pixel) => toScan(offOf(p, what, pull?.id === id && pull.moved ? pull.pixel : pixel).seen);
      for (const w of a.added) add({ id: `${i}.wait.${w.n}`, pos: at(`${i}.wait.${w.n}`, w.thing, w.pixel), cls: "said own ahead", only, title: `${short(w.name)}: seen here. The model is being built again with it. Drag it to say better where it is seen; right-click to take it back.` });
      // (A corner's edges are each seen there: one handle for it.)
      const once = new Set();
      p.sightings.forEach((s, k) => !s.what || a.gone.has(ownOf(s)) || (ownOf(s) && once.has(ownOf(s))) || (once.add(ownOf(s)), 0) || add({ id: `${i}.said.${k}`, pos: at(`${i}.said.${k}`, s.what, a.moved.get(ownOf(s)) ?? s.pixel), cls: `said${ownOf(s) ? " own" : ""}${a.moved.has(ownOf(s)) ? " ahead" : ""}${s.disabled ? " disabled" : ""}${pointed === `${i}.said.${k}` ? " pointed" : ""}`, only, title: a.moved.has(ownOf(s)) ? `${short(s.name)}: moved here. The model is being built again with it.` : `${short(s.name)}: seen here, ${offSaid(s)} (read to ${s.within})${s.disabled ? ", left out of the fit" : ""}. ${ownOf(s) ? "Drag it to say better where it is seen; right-click to take it back." : "Said in the model's script, and moved there: not by dragging it here."}` }));
    });
  },
  drag(v, id, phase, ray, e) {
    const h = handled(id), q = v.state.photo;
    if (!h || !q) return;
    // (What is seen already is moved only where it was said here: one said in the script is moved there.)
    const own = h.said ? ownOf(h.said) : null;
    if (h.said && !own) return;
    // The pixel under the cursor.
    const r = v.canvas.getBoundingClientRect(), pixel = v.pixel(v.camera(), e.clientX - r.left, e.clientY - r.top);
    if (phase === "start") { pull = { id, pixel, x: e.clientX, y: e.clientY, moved: false }; magnify(v, e); return rebuildMarks(); }
    // (Let go of by Shift or Escape: nothing more comes of it.)
    if (!pull) return;
    magnify(v, phase === "end" ? null : e);
    pull.pixel = pixel;
    if (Math.hypot(e.clientX - pull.x, e.clientY - pull.y) > 3) pull.moved = true;
    if (phase !== "end") return rebuildMarks();
    const moved = pull.moved;
    pull = null;
    rebuildMarks();
    if (!moved) return;
    if (h.said) return void moveTo(own, pixel);
    // (An edge is said by the part its body is of and its name, so it is seen wherever the model then has it; a corner,
    // by the edges that meet there.)
    const x = h.x, of = x.address.split("/").pop();
    if (h.what === "edge") say(h.p, `{ part: ${JSON.stringify(x.address)}, edge: ${JSON.stringify(x.name)} }`, `${x.points ? "rim" : "edge"} of ${of}`, pixel, h.thing);
    else if (h.what === "outline") say(h.p, `{ part: ${JSON.stringify(x.address)}, round: ${JSON.stringify(x.name)} }`, `outline of ${of}`, pixel, h.thing);
    else say(h.p, `{ part: ${JSON.stringify(x.address)}, edges: ${JSON.stringify(x.edges ?? [])} }`, `corner of ${of}`, pixel, h.thing);
  },
  handleMenu(v, id, e) {
    const s = handled(id)?.said;
    if (!s) return;
    const n = ownOf(s);
    const head = { label: `${short(s.name)}: ${s.off == null && !s.disabled ? "the model is being built again with it" : `${offSaid(s)}${s.disabled ? ", left out of the fit" : ""}`}`, cls: "head", run() {} };
    if (!n) return openMenu([head, { label: "Said in the model's script: taken out there", cls: "head", run() {} }], e.clientX, e.clientY);
    // (Left out of the fit and kept, to see whether it is what pulls the fit off; and put back in.)
    const left = s.disabled ? { label: "Fit to it again", run: () => leaveOut(n, false) } : { label: "Leave it out of the fit", run: () => leaveOut(n, true) };
    // (A straight edge is sighted to its whole line unless it is said to be to its segment, between its ends.)
    const segment = s.what?.kind === "line" ? [{ label: "Sight to line segment", checked: !!s.what.segment, run: () => sightToSegment(n, !s.what.segment) }] : [];
    openMenu([head, left, ...segment, { label: "Take it back", run: () => takeBack(n) }], e.clientX, e.clientY);
  },
});
