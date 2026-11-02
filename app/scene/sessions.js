// Facet sessions (docs/facet-sessions.md): a recording, where its rig went and how its cameras sit on it, as a part of the
// model. In the tree a row, with the frames of it kept under it; in the views, where its eye is open, its trajectory, the rig at
// the time its slider says (each camera's frame, as a slate: its picture at the camera, about a phone's size unless the session
// says), and slates of frames near that time and of those kept. A slate is looked through (a double-click, or from what is
// chosen): the frame is then a photo as any is, through its lens and rolling shutter, its picture
// read from the recording. A thing said to be seen in it keeps it (`keepFrame`): a line of `NAME.frames.gen.ts`, a photo rigid in
// the session part from then on. All frames at once are never shown: there may be tens of thousands.
//
// What is read of a recording is read by a worker (session-worker.js: topo-session, facet_frames), through the store's byte ranges.

import { kind } from "./kinds.js";
import { model, project, storage } from "../project.js";
import { msg } from "../shell/panel.js";
import { open as unfolded, vis } from "../objects.js";
import { workspace } from "../workspace.js";
import { sel, toScan } from "./state.js";
import { choose } from "./chosen.js";
import { renderTree } from "./tree.js";
import { rebuildMarks } from "./marks.js";
import { framesOfSession, offerFrame, photosChanged, viewPhoto } from "./photos.js";
import { lensSaid } from "./exact.js";
import { esc } from "../ui.js";

/** How many frames near the slider's time are offered, either way, for each camera; and how far apart in time. */
const NEAR_FRAMES = 3, NEAR_APART_S = 0.5;
/** How many JPEGs are kept, read. */
const JPEGS_KEPT = 64;
/** How wide a frame is read, to see while seeking. */
const PREVIEW_WIDTH = 480;
/** How the rig's turning is shown beside the slider: its colour at rest and at this rate (rad/s) or more. */
const STILL_RGB = [52, 92, 120], TURNING_RGB = [224, 72, 40], TURNING_FULL = 1.5;
const TRAIL = [120, 120, 130], LIVE = [255, 200, 40], NEAR = [60, 150, 240], KEPT = [225, 95, 0], CHOSEN = [255, 60, 200];
/** How wide a frame is read to stand on a slate, and how many of those are kept; what a slate shows until its frame is read. */
const SLATE_WIDTH = 320, SLATES_KEPT = 96, BLANK = { w: 1, h: 1, rgba: new Uint8Array([70, 70, 76, 255]) };

/** The model's sessions, as the engine gives them (`sessions`). */
let sessions = [];
/** Each opened: its worker, what it read (`info`), its slider's time (`t`), its cameras then (`at`), the frames near it (`near`). */
const opened = new Map();
const find = (name) => sessions.find((s) => s.name === name) ?? null;
const v3 = (a) => (Array.isArray(a) ? a : [a.x, a.y, a.z]);
const shown = (s) => vis("sessions", `session:${s.name}`);

/** A place of the session's frame, in the model's. */
const inModel = (s, p) => { const { origin, x, y, z } = s.pose, o = v3(origin), [a, b, c] = [v3(x), v3(y), v3(z)]; return [0, 1, 2].map((k) => o[k] + a[k] * p[0] + b[k] * p[1] + c[k] * p[2]); };
/** A way of the session's frame, in the model's. */
const wayIn = (s, p) => { const { x, y, z } = s.pose, [a, b, c] = [v3(x), v3(y), v3(z)]; return [0, 1, 2].map((k) => a[k] * p[0] + b[k] * p[1] + c[k] * p[2]); };
/** The axes of a pose `[x, y, z, qx, qy, qz, qw]`: x, y and z of what it places. */
const axesOf = ([, , , x, y, z, w]) => [[1 - 2 * (y * y + z * z), 2 * (x * y + w * z), 2 * (x * z - w * y)], [2 * (x * y - w * z), 1 - 2 * (x * x + z * z), 2 * (y * z + w * x)], [2 * (x * z + w * y), 2 * (y * z - w * x), 1 - 2 * (x * x + y * y)]];

/** Has the engine say them again (the model was built again), and opens those shown. */
export async function loadSessions() {
  sessions = model.built ? await model.ask("sessions").catch(() => []) : [];
  if (!Array.isArray(sessions)) sessions = [];
  for (const s of sessions) if (shown(s)) openSession(s);
  renderTree();
  rebuildMarks();
}

/** Files of a session, by the store's ids: beside the model. */
const idOf = (path) => { const dir = model.state.file.split("/").slice(0, -1).join("/"); return `${dir ? dir + "/" : ""}${path}`; };

/** How far a session has come in being read, in words. */
const readingSaid = (o) => (o.reading ? `reading ${o.reading.what}${o.reading.share != null ? ` ${Math.round(o.reading.share * 100)}%` : "…"}` : "");
/** It is said where it shows: its row (and its group's, which may be folded), and if it is chosen its card and Details. */
function showReading(o) {
  renderTree();
  for (const el of document.querySelectorAll("[data-session-reading]")) el.textContent = readingSaid(o);
}
/** What it is chosen for says what it now can (read at last): the card, and Details. */
function told() {
  if (sel.session) choose("session", sel.session);
}

/** A session opened: read, once, by a worker of its own. (Whoever asks waits for it: `.ready`.) */
export function openSession(s) {
  let o = opened.get(s.name);
  if (o) return o;
  o = { name: s.name, t: null, near: [], at: [], info: null, error: null, pending: new Map(), next: 1, asked: false, reading: { what: "starting", share: null }, previews: {}, previewing: {}, slates: new Map(), wanted: [] };
  opened.set(s.name, o);
  o.ready = (async () => {
    const store = storage.store;
    if (!store.bytes) throw new Error(`${store.label} cannot read a recording: open the project from a folder, or through topo serve`);
    const [recording, layer, calibration, trajectory] = await Promise.all([`${s.recording}/session.mcap`, `${s.recording}/frames.mcap`, s.calibration, s.trajectory].map((p) => store.bytes(idOf(p))));
    o.worker = new Worker(new URL("../session-worker.js", import.meta.url), { type: "module" });
    o.worker.onmessage = (e) => {
      // (How far it has come, said as it goes.)
      if (e.data.progress) { o.reading = e.data.progress; return showReading(o); }
      const p = o.pending.get(e.data.id); o.pending.delete(e.data.id); if (e.data.error) p?.bad(new Error(e.data.error)); else p?.good(e.data.result);
    };
    o.info = await call(o, "open", { recording, layer, calibration, trajectory });
    o.t = o.t ?? (o.info.interval[0] + o.info.interval[1]) / 2;
    o.reading = null;
    await lookAround(o);
    renderTree();
    told();
    showPreviews(o);
    return o;
  })().catch((e) => { o.error = e.message; o.reading = null; msg(`Session ${s.name}: ${e.message}`, true); renderTree(); told(); });
  return o;
}
function call(o, what, args = {}) {
  return new Promise((good, bad) => { const id = o.next++; o.pending.set(id, { good, bad }); o.worker.postMessage({ id, call: what, ...args }); });
}

/** The frames of a camera near a time: the nearest, and a frame for each of some times about it, those on a grid of times (so
 *  that as the slider moves a slate stays, not one after another). */
function nearFrames(o) {
  const out = [], on = Math.round(o.t / NEAR_APART_S) * NEAR_APART_S;
  o.info.cameras.forEach((c, k) => {
    if (!o.info.times[k].length) return;
    const seq = o.info.sequences[k], seen = new Set([nearestOf(o, k, o.t)]);
    for (let j = -NEAR_FRAMES; j <= NEAR_FRAMES; j++) seen.add(nearestOf(o, k, on + j * NEAR_APART_S));
    for (const i of seen) out.push([c.id, seq[i]]);
  });
  return out;
}

/** Where the cameras were at the slider's time, and the frames near it, asked for; drawn when they are said. (One ask at a time: the last time wins.) */
async function lookAround(o) {
  if (o.asked) { o.again = true; return; }
  o.asked = true;
  try {
    const [at, near] = await Promise.all([call(o, "at", { t: o.t }), call(o, "view", { frames: nearFrames(o) })]);
    o.at = at;
    o.near = near;
  } catch (e) {
    msg(`Session ${o.name}: ${e.message}`, true);
  }
  o.asked = false;
  rebuildMarks();
  if (o.again) { o.again = false; lookAround(o); }
}
export function setTime(name, t) {
  const o = opened.get(name);
  if (!o?.info) return;
  o.t = Math.min(o.info.interval[1], Math.max(o.info.interval[0], t));
  lookAround(o);
}

/** The photo a frame is, through its lens and rolling shutter: as the page has any (photos.js), not yet kept. `seen` as the worker says it. */
function framePhoto(s, seen) {
  const [x, y, z] = axesOf(seen.pose), origin = inModel(s, seen.pose);
  // (Pixel i's middle at i in a calibration; at i + ½ in a picture: its centre, the third and fourth of any model's.)
  const lens = lensSaid(seen.model, seen.lens.map((n, k) => (k === 2 || k === 3 ? n + 0.5 : n)));
  return {
    name: `${s.name}.${seen.camera}.${seen.sequence}`, address: "", image: { session: s.name, camera: seen.camera, sequence: seen.sequence }, size: seen.size,
    lens, shutter: { rows: seen.size[1], poses: seen.rows },
    pose: { origin, x: wayIn(s, x), y: wayIn(s, y), z: wayIn(s, z) }, away: Math.hypot(...origin), sightings: [], unkept: seen,
  };
}

/** Looks through a frame (`session|camera|sequence`), in a view. */
export async function lookThrough(id) {
  const [name, camera, sequence] = id.split("|"), s = find(name), o = s && openSession(s);
  if (!o) return;
  await o.ready;
  const [seen] = await call(o, "view", { frames: [[camera, Number(sequence)]] });
  const p = framePhoto(s, seen);
  offerFrame(p);
  viewPhoto(p.name);
}

/** A frame's JPEG, as a page can show it: where it is once read, and "" till then (`photosChanged` is called when it is). */
const jpegs = new Map();
export function frameSrc(image) {
  const key = `${image.session}|${image.camera}|${image.sequence}`, had = jpegs.get(key);
  if (had !== undefined) { jpegs.delete(key); jpegs.set(key, had); return had ?? ""; }
  const s = find(image.session);
  if (!s) return "";
  jpegs.set(key, null);
  const o = openSession(s);
  Promise.resolve(o.ready).then(() => call(o, "jpeg", { camera: image.camera, sequence: image.sequence })).then((bytes) => {
    jpegs.set(key, URL.createObjectURL(new Blob([bytes], { type: "image/jpeg" })));
    for (const [k, url] of jpegs) { if (jpegs.size <= JPEGS_KEPT) break; if (url) URL.revokeObjectURL(url); jpegs.delete(k); }
    photosChanged();
  }).catch((e) => msg(`Frame ${key}: ${e.message}`, true));
  return "";
}

// ----- what is kept: the file beside the model -----------------------------------------------------------------

const HEADER = (stem) => `// The frames of its recordings seen in, kept by the workspace (which writes this file): each a photo rigid in its session.
// The model says them: \`import { frames } from "./${stem}.frames.gen"\` and \`session(name, { ..., frames })\`.
import type { FrameKept } from "topo-part";

export const frames: FrameKept[] = [
`;
const framesFile = () => model.state.file.replace(/\.ts$/, ".frames.gen.ts");
const num = (x) => String(+(+x).toPrecision(12));
/** A frame looked through is kept, as the first thing is said to be seen in it. Whether it is. */
export async function keepFrame(p) {
  const id = framesFile(), stem = id.split("/").pop().replace(/\.frames\.gen\.ts$/, "");
  const was = (await storage.store.read(id)) ?? "";
  const src = (await storage.store.read(model.state.file)) ?? "";
  if (!src.includes(`./${stem}.frames.gen`)) {
    if (!was) await storage.store.write(id, `${HEADER(stem)}];\n`);
    msg(`The model does not say the frames kept yet. Add to it, once: import { frames } from "./${stem}.frames.gen"; and, to its session, { ..., frames }.`, true);
    return false;
  }
  unfolded.add(`session:${p.image.session}`);
  const s = p.unkept, key = `session: ${JSON.stringify(p.image.session)}, camera: ${JSON.stringify(s.camera)}, sequence: ${s.sequence},`;
  if (was.includes(key)) return true;
  const who = workspace.me?.().name ?? "someone", when = new Date().toISOString().slice(0, 16).replace("T", " ") + " UTC";
  const line = `  { ${key} at: [${s.pose.map(num).join(", ")}], rows: [${s.rows.map((r) => `[${r.map(num).join(", ")}]`).join(", ")}], size: [${s.size.join(", ")}], model: ${JSON.stringify(s.model)}, lens: [${s.lens.map(num).join(", ")}] }, // ${who}, ${when}\n`;
  const lines = was.split("\n").filter((l) => l.trimStart().startsWith("{ session:")).map((l) => l + "\n");
  await storage.store.write(id, HEADER(stem) + lines.join("") + line + "];\n", { base: was || undefined });
  await project.touched();
  return true;
}

// ----- the slider, on the card and in Details ------------------------------------------------------------------

const clock = (t, o) => `${(t - o.info.interval[0]).toFixed(2)} s`;
/** How much the rig was turning along the session (dark: quickly; a frame there is blurred, and its rows far apart). */
function drawMotion(canvas, o) {
  const g = canvas.getContext("2d"), p = o.info.path, n = p.length / 8, w = canvas.width;
  const [a0, b0] = o.info.interval, x = (t) => (w * (t - a0)) / (b0 - a0);
  g.fillStyle = `rgb(${STILL_RGB})`;
  g.fillRect(0, 0, w, canvas.height);
  for (let k = 1; k < n; k++) {
    const dq = Math.abs(p[8 * k + 4] * p[8 * k - 4] + p[8 * k + 5] * p[8 * k - 3] + p[8 * k + 6] * p[8 * k - 2] + p[8 * k + 7] * p[8 * k - 1]);
    const rate = (2 * Math.acos(Math.min(1, dq))) / Math.max(1e-6, p[8 * k] - p[8 * k - 8]);
    const k1 = Math.min(1, rate / TURNING_FULL);
    g.fillStyle = `rgb(${STILL_RGB.map((a, c) => Math.round(a + (TURNING_RGB[c] - a) * k1))})`;
    // (Placed by its time, as the slider is; the next one draws over the pixel past its end.)
    g.fillRect(x(p[8 * k - 8]), 0, x(p[8 * k]) - x(p[8 * k - 8]) + 1, canvas.height);
  }
}
const sliderHtml = (o) => {
  const [a, b] = o.info.interval;
  // (Said once, over it: what the bar is, and which way its colours go.)
  const legend = `<div class="muted timeline-inset" style="display:flex;justify-content:space-between;align-items:center;gap:6px;font-size:11px"><span>Motion</span><span style="display:flex;align-items:center;gap:4px">still<i style="display:inline-block;width:28px;height:6px;outline:1px solid rgba(0,0,0,0.55);background:linear-gradient(to right, rgb(${STILL_RGB}), rgb(${TURNING_RGB}))"></i>fast</span></div>`;
  return `${legend}<div class="timeline"><canvas data-motion width="480" height="8"></canvas><input data-t type="range" min="${a}" max="${b}" step="any" value="${o.t}" title="The time: the rig is drawn where it was, with frames near it. The bar is how quickly it was turning: blue is still, and frames there are sharp; red is turning quickly"></div><div class="muted"><span data-at>${clock(o.t, o)}</span> of ${clock(b, o)}. Look through the nearest of ${o.info.cameras.map((c) => `<a href="#" data-look="${esc(c.id)}">${esc(c.id)}</a>`).join(" ")}, or double-click a picture in the view.</div>`;
};
/** What Details shows of each camera's frame at the slider's time, to look through it with a click. */
const previewsHtml = (o) => `<div style="display:flex;gap:4px;margin-top:6px">${o.info.cameras.map((c) => `<figure style="flex:1 1 0;min-width:0;margin:0"><canvas data-preview="${esc(o.name)}|${esc(c.id)}" width="${PREVIEW_WIDTH}" height="${Math.round((PREVIEW_WIDTH * c.size[1]) / c.size[0])}" style="display:block;width:100%;background:#222;cursor:pointer" title="${esc(c.id)} at this time: click to look through it"></canvas><figcaption class="muted" data-preview-said="${esc(o.name)}|${esc(c.id)}">${esc(c.id)}</figcaption></figure>`).join("")}</div>`;
/** The frame of a camera nearest a time: its place among the camera's frames. */
function nearestOf(o, k, t) {
  const times = o.info.times[k];
  let lo = 0, hi = times.length;
  while (lo < hi) { const m = (lo + hi) >> 1; if (times[m] < t) lo = m + 1; else hi = m; }
  return lo > 0 && (lo === times.length || t - times[lo - 1] < times[lo] - t) ? lo - 1 : Math.min(lo, times.length - 1);
}
/**
 * Each camera's frame at the slider's time, wherever one is shown. A frame is read and made small by the worker; asked for while
 * another of that camera is on its way, the last asked for follows it (so what is shown keeps up with the slider, skipping what it
 * has passed).
 */
function showPreviews(o) {
  if (!o.info || !document.querySelector(`canvas[data-preview^="${CSS.escape(o.name)}|"]`)) return;
  o.info.cameras.forEach((c, k) => {
    if (!o.info.times[k].length) return;
    const i = nearestOf(o, k, o.t), sequence = o.info.sequences[k][i];
    if (o.previewing[c.id]) { o.previewing[c.id] = sequence; return; }
    if (o.previews[c.id]?.sequence === sequence) return drawPreview(o, c.id);
    o.previewing[c.id] = sequence;
    call(o, "preview", { camera: c.id, sequence, width: PREVIEW_WIDTH }).then((bitmap) => {
      o.previews[c.id]?.bitmap.close();
      o.previews[c.id] = { sequence, bitmap };
      drawPreview(o, c.id);
    }).catch(() => {}).finally(() => {
      const next = o.previewing[c.id];
      o.previewing[c.id] = null;
      if (next !== sequence) showPreviews(o);
    });
  });
}
function drawPreview(o, id) {
  const shown = o.previews[id];
  if (!shown) return;
  const key = `${o.name}|${id}`;
  for (const canvas of document.querySelectorAll("canvas[data-preview]")) if (canvas.dataset.preview === key) canvas.getContext("2d").drawImage(shown.bitmap, 0, 0, canvas.width, canvas.height);
  for (const said of document.querySelectorAll("[data-preview-said]")) if (said.dataset.previewSaid === key) said.textContent = `${id} ${shown.sequence}`;
}
function bindSlider(host, name) {
  const o = opened.get(name), input = host.querySelector("input[data-t]");
  if (!input || !o?.info) return;
  for (const canvas of host.querySelectorAll("canvas[data-preview]")) {
    const id = canvas.dataset.preview.slice(name.length + 1);
    canvas.onclick = () => { const seq = o.previews[id]?.sequence; if (seq != null) lookThrough(`${name}|${id}|${seq}`); };
  }
  showPreviews(o);
  const m = host.querySelector("canvas[data-motion]");
  if (m) drawMotion(m, o);
  input.onpointerdown = (e) => e.stopPropagation();
  input.oninput = () => {
    setTime(name, Number(input.value));
    showPreviews(o);
    const at = host.querySelector("[data-at]");
    if (at) at.textContent = clock(o.t, o);
    // (The other slider, on the card or in Details, as this one.)
    for (const other of document.querySelectorAll("input[data-t]")) if (other !== input) { other.value = input.value; const a = other.closest(".timeline").parentElement.querySelector("[data-at]"); if (a) a.textContent = clock(o.t, o); }
  };
  for (const a of host.querySelectorAll("a[data-look]")) a.onclick = (e) => {
    e.preventDefault();
    const k = o.info.cameras.findIndex((c) => c.id === a.dataset.look);
    lookThrough(`${name}|${a.dataset.look}|${o.info.sequences[k][nearestOf(o, k, o.t)]}`);
  };
}

// ----- its marks ---------------------------------------------------------------------------------------------------

/** The corners of a slate at a camera, its first row's ends and then its last's: at `at`, along its picture's `x` and `y`, as wide
 *  as the session says and as high as its picture is for that. */
const cornersAt = (s, at, x, y, size) => {
  const w = (s.slate ?? 0.15) / 2, h = (w * size[1]) / size[0];
  return [[-w, -h], [w, -h], [-w, h], [w, h]].map(([a, b]) => [0, 1, 2].map((i) => at[i] + a * x[i] + b * y[i]));
};
/** Of a camera at a pose of the session's frame: scan frame. */
const slateOf = (s, pose, size) => { const [x, y] = axesOf(pose); return cornersAt(s, pose, x, y, size).map((q) => toScan(inModel(s, q))); };
/** Of a frame kept (a photo, where the model has it), while its session is shown: scan frame, round it. */
export function slateOfPhoto(p) {
  const s = find(p.image.session);
  if (!s || !shown(s)) return null;
  const c = cornersAt(s, v3(p.pose.origin), v3(p.pose.x), v3(p.pose.y), p.size).map(toScan);
  return [c[0], c[1], c[3], c[2]];
}
const frameId = (s, f) => `${s.name}|${f.camera}|${f.sequence}`;
/** The pixels of a frame on a slate, and what says they are those; read when they are not yet. */
function slateImage(o, camera, sequence) {
  const key = `${camera}|${sequence}`, had = o.slates.get(key);
  if (had) { o.slates.delete(key); o.slates.set(key, had); return { image: had, id: key }; }
  if (!o.wanted.some((w) => w.key === key)) o.wanted.push({ key, camera, sequence });
  return { image: BLANK, id: "" };
}
/** What is wanted of a recording for its slates, one after another, the last asked for first: what is on the screen now. */
async function readSlates(o) {
  if (o.readingSlates) return;
  o.readingSlates = true;
  while (o.wanted.length) {
    const { key, camera, sequence } = o.wanted.pop();
    if (o.wanted.length > SLATES_KEPT) o.wanted.splice(0, o.wanted.length - SLATES_KEPT);
    const image = await call(o, "slate", { camera, sequence, width: SLATE_WIDTH }).catch(() => null);
    if (image) o.slates.set(key, image);
    for (const k of o.slates.keys()) { if (o.slates.size <= SLATES_KEPT) break; o.slates.delete(k); }
    rebuildMarks();
  }
  o.readingSlates = false;
}

// ----- the kinds -----------------------------------------------------------------------------------------------------

kind("session", {
  field: "session", group: "sessions", order: 47,
  key: (id) => `session:${id}`,
  find,
  title: (s) => `session ${s.name}`,
  points: (s) => {
    const o = opened.get(s.name);
    if (!o?.info) return [toScan(v3(s.pose.origin))];
    const out = [], p = o.info.path;
    for (let k = 0; k < p.length; k += 8) out.push(toScan(inModel(s, [p[k + 1], p[k + 2], p[k + 3]])));
    return out;
  },
  info: (s) => {
    const o = opened.get(s.name);
    return {
      what: "Session", name: s.name, id: s.recording,
      rows: o?.info ? [["Cameras", o.info.cameras.map((c) => `${c.id}: ${c.frames} frames`).join(", ")], ["Trajectory", `${(o.info.interval[1] - o.info.interval[0]).toFixed(1)} s, ${s.trajectory}`], ["Calibration", s.calibration], ["Frames kept", String(framesOfSession(s.name).length)]] : [["Read", o?.error ?? (o ? readingSaid(o) : "not yet: show it")]],
      note: "Show it (its eye) to see its trajectory, the rig at the slider's time, and frames near it, each a picture where its camera was. Double-click one to look through it, and drag an edge of the model to where it is in the picture, as in a photo: the frame is kept from then on, rigid in the session.",
      mount: (host) => { if (o?.info) { const d = document.createElement("div"); d.style.margin = "8px 0"; d.innerHTML = sliderHtml(o) + previewsHtml(o); const head = host.querySelector(".head"); if (head) head.after(d); else host.prepend(d); bindSlider(d, s.name); } },
    };
  },
  where: () => [],
  card: (s) => {
    const o = opened.get(s.name);
    return { label: "Session", id: s.name, note: o?.info ? `${o.info.cameras.length} camera${o.info.cameras.length === 1 ? "" : "s"}` : o?.error ?? "", body: o?.info ? sliderHtml(o) : `<span data-session-reading>${o ? readingSaid(o) : ""}</span>`, bind: (chip) => bindSlider(chip, s.name) };
  },
  rows(row, list) {
    if (!sessions.length) return "";
    return list("sessions", "Sessions", () => sessions.flatMap((s) => {
      const o = opened.get(s.name);
      return [
        row(1, "session", `session:${s.name}`, s.name, { id: s.name, group: framesOfSession(s.name).length > 0, on: sel.session === s.name, note: o?.error ? "not read" : o?.reading ? readingSaid(o) : o?.info ? `${o.info.cameras.map((c) => c.frames).reduce((a, b) => a + b, 0)} frames` : "", title: `A facet session (${s.recording}): its trajectory, and any of its frames to look through` }),
        ...(unfolded.has(`session:${s.name}`) ? framesOfSession(s.name) : []).map((p) => row(2, "photo", `photo:${p.name}`, p.name.slice(s.name.length + 1), { id: p.name, on: sel.photo === p.name, note: `${p.sightings.length} seen`, title: "A frame kept: rigid in its session. Double-click to look through it" })),
      ];
    }), { note: sessions.map((s) => opened.get(s.name)).find((o) => o?.reading) ? readingSaid(sessions.map((s) => opened.get(s.name)).find((o) => o?.reading)) : sessions.length, title: "Recordings of the model: each placed by what is seen in its frames" });
  },
  goto(id) {
    const s = find(id);
    if (s) { openSession(s); choose("session", id); }
  },
  menu(id, m) {
    if (find(id)) m.add("Choose it", () => choose("session", id));
  },
  // A click on its trajectory is on it. (Not in a view through a photo: it is not drawn there.)
  near(out, v) {
    if (v.state.photo) return;
    for (const s of sessions.filter(shown)) {
      const o = opened.get(s.name);
      if (!o?.info) continue;
      const p = o.info.path, pts = [];
      for (let k = 0; k < p.length; k += 8) pts.push(toScan(inModel(s, [p[k + 1], p[k + 2], p[k + 3]])));
      out.loop(s.name, pts, false);
    }
  },
  // Its trajectory, the rig where it was at the slider's time, and slates of frames near that time and of those kept. The rig's each
  // show the frame nearest that time. (The first asked for of what is not read are the rig's: read last in, first out.)
  marks(out) {
    for (const s of sessions.filter(shown)) {
      const o = opened.get(s.name);
      if (!o?.info) continue;
      const p = o.info.path, here = (k) => toScan(inModel(s, [p[8 * k + 1], p[8 * k + 2], p[8 * k + 3]]));
      // (Not in a view through a photo: there what is seen is the picture.)
      const free = (v) => !v.state.photo;
      for (let k = 0; k + 1 < p.length / 8; k++) out.line(here(k), here(k + 1), sel.session === s.name ? CHOSEN : TRAIL, free);
      const stand = (key, corners, camera, sequence, rgb) => {
        const { image, id } = slateImage(o, camera, sequence);
        out.slate({ key, corners, image, stamp: `${id} ${s.slate}` }, free);
        for (const [a, b] of [[0, 1], [1, 3], [3, 2], [2, 0]]) out.line(corners[a], corners[b], rgb, free);
      };
      const kept = framesOfSession(s.name), keptIds = new Set(kept.map((f) => `${s.name}|${f.image.camera}|${f.image.sequence}`));
      for (const f of kept) {
        if (!vis(`photo:${f.name}`)) continue;
        const c = slateOfPhoto(f);
        if (c) stand(`kept ${f.name}`, [c[0], c[1], c[3], c[2]], f.image.camera, f.image.sequence, sel.photo === f.name ? CHOSEN : KEPT);
      }
      const nearest = new Set(o.info.cameras.map((c, k) => (o.info.times[k].length ? `${s.name}|${c.id}|${o.info.sequences[k][nearestOf(o, k, o.t)]}` : "")));
      for (const n of o.near) {
        const id = frameId(s, n);
        if (!nearest.has(id) && !keptIds.has(id)) stand(`near ${id}`, slateOf(s, n.pose, n.size), n.camera, n.sequence, sel.frame === id ? CHOSEN : NEAR);
      }
      o.info.cameras.forEach((c, k) => {
        if (o.at[k]?.length && o.info.times[k].length) stand(`rig ${s.name} ${c.id}`, slateOf(s, o.at[k], c.size), c.id, o.info.sequences[k][nearestOf(o, k, o.t)], LIVE);
      });
      readSlates(o);
    }
  },
});

kind("frame", {
  field: "frame",
  key: (id) => `frame:${id}`,
  find: (id) => {
    const [name, camera, sequence] = String(id).split("|"), o = opened.get(name);
    const n = o?.near.find((f) => f.camera === camera && f.sequence === Number(sequence));
    return n ? { id, name, seen: n } : null;
  },
  title: (f) => `frame ${f.seen.camera} ${f.seen.sequence}`,
  card: (f) => ({ label: "Frame", id: `${f.seen.camera} ${f.seen.sequence}`, note: `${f.seen.motion_px.toFixed(0)} px across its rows`, body: `A frame of session ${esc(f.name)}. Double-click it to look through it, or <a href="#" data-through>here</a>.`, bind: (chip) => { const a = chip.querySelector("a[data-through]"); if (a) a.onclick = (e) => { e.preventDefault(); lookThrough(f.id); }; } }),
  info: (f) => ({ what: "Frame", name: `${f.seen.camera} ${f.seen.sequence}`, id: f.name, rows: [["Camera", f.seen.camera], ["Number", String(f.seen.sequence)], ["Across its rows", `${f.seen.motion_px.toFixed(1)} px of turning`]], note: "Look through it to say what is seen in it." }),
  goto: (id) => lookThrough(id),
  menu(id, m) {
    m.add("Look through it", () => lookThrough(id));
  },
  // A click on a slate is on that frame. (The rig's is of the frame it shows, at where the rig is.)
  near(out, v) {
    if (v.state.photo) return;
    for (const s of sessions.filter(shown)) {
      const o = opened.get(s.name);
      if (!o?.info) continue;
      const round = ([a, b, c, d]) => [a, b, d, c];
      o.info.cameras.forEach((c, k) => {
        if (o.at[k]?.length && o.info.times[k].length) out.area(`${s.name}|${c.id}|${o.info.sequences[k][nearestOf(o, k, o.t)]}`, round(slateOf(s, o.at[k], c.size)));
      });
      const kept = new Set(framesOfSession(s.name).map((f) => `${s.name}|${f.image.camera}|${f.image.sequence}`));
      for (const n of o.near) if (!kept.has(frameId(s, n))) out.area(frameId(s, n), round(slateOf(s, n.pose, n.size)));
    }
  },
});
