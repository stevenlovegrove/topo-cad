// The exact view (docs/part-core.md, "What is built"): what a 3D view shows, drawn by Pangolino's renderer through
// WebGPU (topo-view, web/pkg/topo_view.js) on a canvas under the view's own, which takes the pointer and holds the
// labels and handles: a part script's bodies as they are, the scan's points and the picture of free space, and the
// marks (lines and faint surfaces), each a layer of its own, the bodies in the model's frame and its millimetres, the
// rest in the scan's; and what a click is on is the body under it, exactly (`pickExact`). Where the page has no WebGPU,
// the view says it cannot draw.
//
// The camera is the view's own, there (topo-view's `Navigator`): the gizmo in its corner, and what moves it as the
// pointer says, which the page offers the pointer to (`pointer`, `wheel`) after its own tools have passed it by. The
// page asks where it is (`camera`, `cameraState`) and sends it places (`setCamera`, `fit`, `switchProjection`). How the
// gizmo looks is the viewer's choice, kept (`prefs().gizmo`): a right-click on it goes on to its other look.
//
// What the views draw of the bodies is the viewer's choice, kept (`v.display`, from prefs.js: solids, edges, x-ray, in
// any combination); what the tree of objects hides (its eyes, and "only this") is hidden in every view, and hides
// nothing. A view clipped to a box (a slab, or the heights it shows) has its bodies cut to it on the GPU as the box
// moves, those it cuts open toward the eye showing their sections, and exactly once it rests (their sections then faces
// of them, of what passes through the box too, their edges and corners picked as any are); what is cut away is not
// picked.

import { model } from "../project.js";
import { COLOURS, vec } from "../glview.js";
import { PREF_DEFAULTS, prefs, setPrefs } from "../prefs.js";
import * as objects from "../objects.js";
import { active, pl, sel, toModel, unrot, views } from "./state.js";
import { SETTLE_MS, shoot, untilShot } from "../previews.js";
import { describeObject } from "./object.js";
import { counters as countersRead, frame as frameTimed, recount } from "../times.js";
import { renderTools } from "./views.js";
import { renderTree } from "./tree.js";
import { rebuildMarks } from "./marks.js";

let lib = null, loaded = null;
/** What is off, of a thing seen at a pixel of a picture (`topo-view`'s `sight_off`: as the fit counts it, through any lens):
 *  its numbers `[on..., seen...]`. Null till the view's module is here. */
export const sightOff = (asked) => loaded?.sight_off(JSON.stringify(asked)) ?? null;
/** The ways, in a camera's frame, of pixels of its picture through a lens: null till the view's module is here. */
export const lensRays = (lens, pixels) => loaded?.lens_rays(lens.kind, new Float64Array(lens.numbers), new Float64Array(pixels)) ?? null;
/** A lens as the engine says one (its model, its numbers each with what it is), of its model's name and its numbers; null: none. */
export const lensSaid = (kind, numbers) => { const s = loaded?.lens_said(kind, new Float64Array(numbers)); return s ? JSON.parse(s) : null; };
/** The bodies of the build shown (JSON), and of which build; and the part's nodes (JSON), and its params. */
let shown = null, version = null, nodes = "[]", params = null;
/** The model's bodies as built (`[{address, material, role, kind, body}]`), or null. */
let built = null;
export const modelBodies = () => built;
/** Bodies drawn in place of the model's (a proposal's side), or null: `{ json, addresses }`. */
let side = null;
/** The bodies drawn: a side's, else the model's. */
const drawn = () => side?.json ?? shown;

/** The part's params as built (`params` answers): each, as found, what decides it, and what is kept of it; and the
 *  drags kept. */
export const paramsNow = () => params;
/** The params of a node (by its address): its coordinates, each `{ address, value, by, kept }`. */
export const paramsOfNode = (address, of = params) => (of?.params ?? []).filter((p) => p.address.startsWith(`${address}.`) && !p.address.slice(address.length + 1).includes("/"));
/** A part's nodes as the exact view draws them: each with whether something is kept of where it is (a param of it, or a
 *  drag of it). */
const nodesOf = (p) => (p?.nodes ?? []).map((n) => ({ ...n, hinted: paramsOfNode(n.address, p).some((q) => q.kept != null) || (p.drags ?? []).some((d) => d.what.node === n.address) }));
/** The addresses of the bodies shown. */
let addresses = [];
/** The name the model's parts' paths start with (a body's address is under it). */
let root = "";

/** What a view draws of the bodies, unless it says (the viewer's choice: prefs.js). */
const DISPLAY = PREF_DEFAULTS.display;
/** The addresses of the bodies the tree hides, one to a line. */
const hiddenBodies = () => (side?.addresses ?? addresses).filter((a) => !objects.memberVisible(pathOf(a))).join("\n");

// (By its URL: it is built (web/build.sh), not one of the app's scripts.)
const load = () => (lib ??= import(new URL("../../pkg/topo_view.js", import.meta.url).href).then(async (m) => (await m.default(), (loaded = m))));

/** Whether a view draws the model exactly. */
export const exactIn = (v) => !!(shown && v.exactView?.view);
/** Whether Pangolino's renderer draws a view's scene (its bodies, if any, and the scan's points), under its canvas. */
const drawsIn = (v) => !!v.exactView?.view;

/** What a roof is made of and held up by, by its parts' roles: what the roof cut clears. */
const ROOF = new Set(["top_chord", "bottom_chord", "web", "rafter", "hip_rafter", "valley_rafter", "ceiling_tie", "truss"]);
/** Where the model's roof begins and its floor ends (m, the model's frame), from its bodies: `roof` the lowest point of
 *  what is of the roof (by its part's role: a truss's chords and webs, rafters, ceiling ties) over what holds it up (on
 *  plan, within what is neither of the roof nor a slab: an eave's tail hangs lower, outside the walls), `floor` the
 *  highest of what is a slab (by its part's kind). Each null where there is none. (By the bodies' straight edges, and
 *  the ends of any others.) */
let levels = { roof: null, floor: null };
export const modelLevels = () => levels;
function levelsOf(bodies) {
  const mm = (p) => p.map((x) => x / 1000);
  const roofs = bodies.filter((b) => ROOF.has(b.role)), slabs = bodies.filter((b) => b.kind === "slab");
  const floor = slabs.flatMap((b) => b.body.vertices.map((v) => v.p[2] / 1000));
  // (What holds the roof up, on plan.)
  const held = bodies.filter((b) => !ROOF.has(b.role) && b.kind !== "slab").flatMap((b) => b.body.vertices.map((v) => mm(v.p)));
  const lo = held.length ? [0, 1].map((k) => Math.min(...held.map((p) => p[k]))) : [-Infinity, -Infinity];
  const hi = held.length ? [0, 1].map((k) => Math.max(...held.map((p) => p[k]))) : [Infinity, Infinity];
  const inside = (p) => p[0] >= lo[0] && p[0] <= hi[0] && p[1] >= lo[1] && p[1] <= hi[1];
  let roof = Infinity;
  for (const b of roofs) {
    for (const e of b.body.edges) {
      const line = e.curve?.Line;
      if (!line) {
        for (const k of e.v) { const p = mm(b.body.vertices[k].p); if (inside(p)) roof = Math.min(roof, p[2]); }
        continue;
      }
      // (A straight edge clipped to it on plan: its lowest point within it is at one end of what is left.)
      let [t0, t1] = e.t;
      for (let k = 0; k < 2 && t0 <= t1; k++) {
        const o = line.o[k] / 1000, d = line.d[k] / 1000;
        if (Math.abs(d) < 1e-15) { if (o < lo[k] || o > hi[k]) t1 = t0 - 1; continue; }
        const [a, c] = [(lo[k] - o) / d, (hi[k] - o) / d];
        t0 = Math.max(t0, Math.min(a, c)); t1 = Math.min(t1, Math.max(a, c));
      }
      if (t0 <= t1) roof = Math.min(roof, (line.o[2] + line.d[2] * t0) / 1000, (line.o[2] + line.d[2] * t1) / 1000);
    }
  }
  return { roof: roof < Infinity ? roof : null, floor: floor.length ? Math.max(...floor) : null };
}

/** The model's bodies, asked of the engine once for each build, shown in every 3D view (a view made since: `exactFor`)
 *  where the page has WebGPU; and where its roof and floor are, either way. */
export async function loadExact() {
  if (!model.state) return;
  const now = model.state.version;
  if (now === version) return;
  version = now;
  const bodies = await model.ask("bodies");
  if (now !== model.state?.version) return;
  const some = Array.isArray(bodies) && bodies.length;
  levels = some ? levelsOf(bodies) : { roof: null, floor: null };
  built = some ? bodies : null;
  shown = some && navigator.gpu ? JSON.stringify(bodies) : null;
  params = shown ? await model.ask("params").catch(() => null) : null;
  nodes = JSON.stringify(nodesOf(params));
  // (A node chosen is told of again, as it now is: where it is, and what is kept of it.)
  const n = sel.bodyPart?.what === "node" && params?.nodes?.find((m) => m.address === sel.bodyPart.name);
  if (n) { sel.bodyPart = { ...sel.bodyPart, at: n.at }; describeObject("bodypart", sel.bodyPart); }
  addresses = shown ? bodies.map((b) => b.address) : [];
  root = (await model.parts().catch(() => null))?.members?.[0]?.path.split("/")[0] ?? "";
  // (The drags kept are of these params: listed, and drawn.)
  renderTree();
  rebuildMarks();
  await Promise.all(Object.values(views).map(exactFor));
  // (What the project looks like, built: the view last used, pictured once it has settled.)
  if (shown) shotIn(views[active] ?? Object.values(views)[0], SETTLE_MS);
}

/** A view to be pictured (`previews.js`) as it next rests, `ms` from now at the soonest. */
function shotIn(v, ms) {
  const e = v?.exactView;
  if (!e) return;
  e.shotAt = performance.now() + ms;
  clearTimeout(e.shooting);
  e.shooting = setTimeout(() => v.redraw(), ms + 5);
}

/** A 3D view drawing the bodies shown, exactly (made where it has none); or not, where none are. Where the page cannot
 *  (no WebGPU), it says so in the view. */
export async function exactFor(v) {
  noGpu(v, !navigator.gpu);
  if (!navigator.gpu) return v.redraw();
  const m = await load().catch((e) => (console.warn("exact view:", e), null));
  if (!m) return;
  // (Made once: whoever asks while it is being made waits for it.)
  v.exactMaking ??= (async () => {
    const canvas = document.createElement("canvas");
    canvas.style.cssText = "position:absolute;inset:0;width:100%;height:100%;pointer-events:none";
    v.host.insertBefore(canvas, v.canvas);
    const dpr = devicePixelRatio || 1;
    canvas.width = Math.max(1, Math.round(v.host.clientWidth * dpr));
    canvas.height = Math.max(1, Math.round(v.host.clientHeight * dpr));
    try {
      const view = await m.View.create(canvas);
      view.ink(1.5 * dpr);
      view.resize(canvas.width, canvas.height, dpr);
      v.exactView = { canvas, view, said: "" };
      v.under = (cam, w, h) => drawUnder(v, cam, w, h);
      gizmoLook(v);
      // (Pixels of the canvas, from CSS pixels of the page.)
      const px = (e) => { const r = canvas.getBoundingClientRect(), k = canvas.width / Math.max(1, r.width); return [(e.clientX - r.left) * k, (e.clientY - r.top) * k]; };
      const scale = () => canvas.width / Math.max(1, canvas.getBoundingClientRect().width);
      // (Where the scan is in the model, before the camera is put anywhere in the scan's frame.)
      const placed = () => view.scan_at(pl.yaw, ...pl.t);
      // (The scan's points drawn by it too, each cloud as it is set; and its camera, which the page asks about.)
      v.drawnBy({
        camera: () => {
          const c = view.camera();
          if (!c) return null;
          const out = JSON.parse(c);
          if (out.through) out.through.k /= scale();
          return out;
        },
        cameraState: () => { placed(); const c = view.camera_state(); return c ? JSON.parse(c) : null; },
        setCamera: (state, ms) => { placed(); view.set_camera(JSON.stringify(state), ms); },
        fit: (c, r, ms) => { placed(); view.fit(c[0], c[1], c[2], r, ms); },
        seenAt: (p) => { placed(); const q = view.seen_at(p[0], p[1], p[2]); return [q[0] / scale(), q[1] / scale()]; },
        rayAt: (x, y) => { placed(); const r = view.ray_at(x * scale(), y * scale()); return { origin: [r[0], r[1], r[2]], dir: [r[3], r[4], r[5]] }; },
        orbitAbout: (p) => { placed(); view.orbit_about(p[0], p[1], p[2]); },
        switchProjection: (x, y) => view.switch_projection(x * scale(), y * scale()),
        wheel: (e, lines) => view.wheel(...px(e), lines),
        points: (key, c, highlight) => (c ? view.points(key, c.xyz, c.intensity, c.normal, c.confidence ?? new Uint8Array(0), highlight) : view.drop_points(key)),
        picture: (img) => view.free_space(new Float32Array(img ? img.corners.flat() : []), img?.w ?? 0, img?.h ?? 0, img?.data ?? new Uint8Array(0)),
        marks: (m) => view.marks(m?.pos ?? new Float32Array(0), m?.col ?? new Uint8Array(0)),
        over: (key, o) => view.over(key, o?.pos ?? new Float32Array(0), o?.col ?? new Uint8Array(0), o?.width ?? 1),
        markers: (key, set) => view.markers(key, set ? JSON.stringify(set) : undefined),
        tip: (text) => { const gone = view.tip(text ?? undefined); wakeFor(v); return gone; },
        dragged: (what, ends) => view[what === "band" ? "band" : "rule"](ends ? new Float64Array(ends.map((x) => x * scale())) : undefined),
        fills: (f) => view.fills(f?.pos ?? new Float32Array(0), f?.col ?? new Uint8Array(0)),
        // (Its pixels only when they are another's: otherwise it is moved.)
        slates: (list) => {
          const had = v.slatesDrawn ?? new Map(), now = new Map(), none = new Uint8Array(0);
          for (const s of list) {
            const same = had.get(s.key) === s.stamp;
            view.slate(s.key, new Float32Array(s.corners.flat()), same ? 0 : s.image.w, same ? 0 : s.image.h, same ? none : s.image.rgba);
            now.set(s.key, s.stamp);
          }
          for (const k of had.keys()) if (!now.has(k)) view.drop_slate(k);
          v.slatesDrawn = now;
        },
        pins: (labels) => view.pins(JSON.stringify(labels.map((l) => ({ id: String(l.id), pos: l.pos, text: l.text, title: l.title ?? "", colour: l.colour ?? null, on: !!l.on, dim: !!l.dim }))), devicePixelRatio || 1),
        pin: (id, shown) => view.show_pin(String(id), shown),
        pointer: (what, e) => pointerIn(v, what, e),
        // (Someone's input over the view, as they are followed.)
        watch: (colour) => view.watch(colour ? new Float32Array(colour) : undefined),
        watched: (events) => view.watched(JSON.stringify(events)),
      });
    } catch (e) {
      console.warn("exact view:", e);
      canvas.remove();
      noGpu(v, true);
    }
  })();
  await v.exactMaking;
  if (!v.exactView) return;
  v.exactView.canvas.style.display = "";
  displayExact(v);
  v.exactView.view.hide(hiddenBodies());
  colourIn(v);
  v.exactView.said = v.exactView.view.show(drawn() ?? "[]");
  v.exactView.view.show_nodes(nodes);
  renderTools(v);
  v.redraw();
}

/** The pointer offered to a view's scene (`what`: "move", "down", "drag", "up", "leave"): its pins, its gizmo, its
 *  camera. `{ taken, cursor, moved }`: what took it ("pin", "gizmo", "camera", or ""), the cursor it says, whether the
 *  camera moved. A click on a pin opens what it is of, a double click goes to it; it is drawn again where it has
 *  changed, and when its tooltip is due. A right-click on the gizmo that turns it to its other look is kept as the
 *  viewer's choice. */
function pointerIn(v, what, e) {
  const view = v.exactView.view, r = v.exactView.canvas.getBoundingClientRect(), k = v.exactView.canvas.width / Math.max(1, r.width);
  const out = JSON.parse(view.pointer(what, (e.clientX - r.left) * k, (e.clientY - r.top) * k, e.button ?? 0));
  for (const c of out.clicks) {
    const id = v.labels.find((l) => String(l.id) === c.id)?.id ?? c.id;
    (c.double ? v.onLabelZoom : v.onLabel)?.(id);
  }
  if (out.redraw) v.redraw();
  wakeFor(v);
  if (what === "up" && out.taken === "gizmo") {
    const style = GIZMO_STYLES[view.gizmo_style()] ?? GIZMO_STYLES[0];
    if (style !== prefs().gizmo.style) setPrefs({ gizmo: { ...prefs().gizmo, style } });
  }
  return out;
}

/** A view drawn again when its scene next has something to draw with no input: a tooltip whose time has come. */
function wakeFor(v) {
  const wake = v.exactView.view.wake_in();
  clearTimeout(v.exactView.wake);
  if (wake != null) v.exactView.wake = setTimeout(() => v.redraw(), wake + 5);
}

/** The gizmo's looks, in the order a right-click goes through them (as topo-view gives them: 0 the compass, 1 the
 *  cube). */
const GIZMO_STYLES = ["compass", "cube"];

/** A view's gizmo as the viewer has chosen it look (`prefs().gizmo`: its style, its labels the model's compass
 *  headings or its axes). */
export function gizmoLook(v) {
  const g = prefs().gizmo, e = v.exactView?.view;
  if (!e) return;
  e.gizmo(Math.max(0, GIZMO_STYLES.indexOf(g.style)), g.labels === "axes");
  v.redraw();
}

/** Says in a view that the model and the scan cannot be drawn in this page (`no`), or takes that back: they are drawn
 *  through WebGPU only. */
function noGpu(v, no) {
  let note = v.host.querySelector(".nogpu");
  if (!no) return note?.remove();
  if (note) return;
  note = document.createElement("div");
  note.className = "nogpu";
  note.textContent = "The model and the scan are drawn through WebGPU, which this browser does not offer (Chrome, Edge and Safari 26 do).";
  note.style.cssText = "position:absolute;left:12px;bottom:12px;max-width:340px;padding:6px 9px;border-radius:6px;background:rgba(30,30,34,.85);color:#f2f2f0;font-size:12px;pointer-events:none";
  v.host.appendChild(note);
}

/** Bodies and nodes as a drag has them settled for now (as `bodies` and `params` answer), in every view: until it
 *  is let go of, and the model is built again (or, nothing kept, shown as built: `exactFor`). */
export function showSettling(bodies, settling) {
  const b = JSON.stringify(bodies), n = JSON.stringify(nodesOf(settling));
  for (const v of Object.values(views)) if (v.exactView?.view) { v.exactView.view.show(b); v.exactView.view.show_nodes(n); v.redraw(); }
}

/** What a view draws of the bodies, as it now says (`v.display`). */
export function displayExact(v) {
  const d = (v.display ??= { ...DISPLAY });
  if (v.exactView?.view) { v.exactView.view.display(d.solids, d.edges, d.xray, !!d.nodes); v.redraw(); }
}

/** A view's bodies sectioned (`on`), exactly: by the plane through their middle, plumb and square to the way it
 *  looks, what is toward the eye cut away; or whole again. Where it is set it stays as the eye moves, until set again.
 *  The addresses of the bodies that could not be cut (shown whole), one to a line. */
export function sectionExact(v, on) {
  const e = v.exactView?.view;
  if (!e) return "";
  if (!on) {
    e.section(undefined);
    v.redraw();
    return "";
  }
  const { at, target } = JSON.parse(e.eye());
  const [nx, ny] = [at[0] - target[0], at[1] - target[1]];
  const l = Math.hypot(nx, ny) || 1;
  const whole = e.section(new Float64Array([NaN, NaN, NaN, nx / l, ny / l, 0]));
  v.redraw();
  return whole;
}

/** What the tree hides, hidden in every view. */
objects.on("visibility", () => {
  const hide = hiddenBodies();
  for (const v of Object.values(views)) if (v.exactView?.view) { v.exactView.view.hide(hide); v.redraw(); }
});

/** How long a view's box is still before the bodies are cut to it exactly (milliseconds). */
const REST = 150;

/** A photo's picture, read once for each (`src`), given the exact views that are through it as its pixels; and drawn
 *  again where it is read. (One from somewhere that does not let a page read what it shows is not drawn.) */
const pictures = new Map(), tried = new Set(), PICTURES_KEPT = 8;
function pictureFor(v, src, size) {
  const e = v.exactView;
  if (!src || e.picture === src) return;
  e.picture = src;
  let p = pictures.get(src);
  if (!p) {
    p = new Promise((done) => {
      const img = new Image();
      img.crossOrigin = "anonymous";
      img.onload = () => {
        const c = Object.assign(document.createElement("canvas"), { width: img.naturalWidth || size[0], height: img.naturalHeight || size[1] }), g = c.getContext("2d");
        g.drawImage(img, 0, 0, c.width, c.height);
        try { done({ w: c.width, h: c.height, data: g.getImageData(0, 0, c.width, c.height).data }); } catch { done(null); }
      };
      img.onerror = () => done(null);
      img.src = src;
    });
    pictures.set(src, p);
    // (Each holds all of its pixels: only the last few are kept.)
    for (const k of pictures.keys()) { if (pictures.size <= PICTURES_KEPT) break; pictures.delete(k); }
  }
  p.then((pic) => {
    // (Not read: asked for once more, the next time it is wanted, where a copy kept without asking may have stood in
    // the way; then left, not drawn.)
    if (!pic) { if (!tried.has(src)) { tried.add(src); pictures.delete(src); if (e.picture === src) e.picture = null; } return; }
    if (e.picture !== src) return;
    e.view.picture(pic.w, pic.h, new Uint8Array(pic.data.buffer));
    v.redraw();
  });
}

/** How the scan's points are drawn: by what they are coloured (GLView's `COLOURS`), and what is done with those the
 *  scan doubts. */
const DOUBT = { show: 0, fade: 1, hide: 2 };

/** Draws a view's scene from its camera: its bodies, and the scan's points as the view says to draw them. */
function drawUnder(v, cam, w, h) {
  if (!drawsIn(v)) return false;
  const e = v.exactView;
  e.canvas.style.display = "";
  if (e.canvas.width !== w || e.canvas.height !== h) {
    e.canvas.width = w;
    e.canvas.height = h;
    e.view.resize(w, h, w / Math.max(1, cam.w));
  }
  // (What is drawn, as a sphere about it: what the camera's near and far are fitted to, and turns about where nothing
  // is under the cursor.)
  const sp = v.sphere();
  e.view.sphere(...(sp?.c ?? [0, 0, 0]), sp?.r ?? 0);
  // (Cut to the box the view is clipped to, as it now is: on the GPU, its bodies cut open showing their sections.)
  const mm = (p) => p.map((x) => x * 1000);
  const box = v.state.box, cut = box ? JSON.stringify([...mm(toModel(box.c)), ...unrot(box.u), ...unrot(box.v), ...unrot(box.n), ...mm(box.h)]) : "";
  if (cut !== e.cut) { e.cut = cut; e.view.cut(cut ? new Float64Array(JSON.parse(cut)) : undefined); }
  // Through a photo's camera: its picture behind the bodies (once read), which they tint.
  const photo = v.state.photo;
  if (photo) pictureFor(v, photo.src, photo.size);
  e.view.tint(photo ? v.state.tint ?? 0 : undefined);
  points(v, cam, w);
  // (Drawn again where there is more to draw: the camera on its way somewhere, glyphs of the pins' text made as they
  // were asked for. Once it is where it was going, the view's state is of where it is.)
  const moving = e.view.animating();
  // (Timed: on the page's thread, and from the frame before where it followed straight on, the view moving.)
  const t0 = performance.now(), more = e.view.frame(), t1 = performance.now();
  frameTimed(t1 - t0, e.drewAt != null && t0 - e.drewAt < 100 ? t0 - e.drewAt : null);
  e.drewAt = t0;
  // (The renderer's counters, read now and then while frames are drawn: the GPU's time, what its shader did.)
  if (!(t0 - (e.countedAt ?? 0) < 500)) { e.countedAt = t0; try { countersRead(JSON.parse(e.view.counters())); } catch (_) {} }
  if (more) v.redraw();
  // (At rest, where it is to be pictured: now, of the scene alone, drawn so and then again as it was; or later, where
  // that is too soon.)
  else if (e.shotAt != null && performance.now() >= e.shotAt && !e.view.animating()) {
    e.shotAt = null;
    const wait = untilShot();
    if (wait) shotIn(v, wait);
    else {
      e.view.plain(true);
      e.view.frame();
      shoot(e.canvas);
      e.view.plain(false);
      v.redraw();
    }
  }
  if (e.moving && !e.view.animating()) { v.syncCamera(); v.onChange?.(); }
  e.moving = moving || e.view.animating();
  // (Once the view rests, cut to its box exactly: on the GPU till then.)
  clearTimeout(e.resting);
  e.resting = setTimeout(() => { if (e.view?.rest_cut()) v.redraw(); }, REST);
  return true;
}

/** The scan's points in a view as it now says: where the scan is in the model, which clouds are drawn, and how. */
function points(v, cam, w) {
  const e = v.exactView, s = v.state;
  e.view.scan_at(pl.yaw, ...pl.t);
  // (On paper, or on the dark: as the canvas clears to where nothing is drawn under it.)
  e.view.background(...(s.paper ? [0.97, 0.97, 0.955] : [0.085, 0.09, 0.105]));
  const { scan, highlights } = v.cloudsDrawn(), drawn = new Set([...scan, ...highlights]);
  for (const k of [...Object.keys(v.clouds).filter((k) => v.clouds[k]), ...highlights]) e.view.show_points(k, drawn.has(k));
  e.view.show_free(!!s.layers.scan);
  e.view.show_marks(!!s.layers.marks);
  // (A pixel of the canvas is this many of the CSS pixels a point's size is in.)
  const dpr = w / Math.max(1, cam.w), box = s.box;
  const plane = box ? [...box.n, vec.dot(box.c, box.n), box.h[2]] : [0, 0, 0, 0, 0];
  // (Cut to a box and seen square on, as a slab is, the points are seen against the sections: over the bodies; else
  // hidden by what is in front of them.)
  e.view.points_over(!!box && !!cam.ortho);
  e.view.points_look(new Float32Array([s.pointSize * dpr, Math.max(0, COLOURS.indexOf(s.colour)), DOUBT[s.doubt] ?? 1, v.floorZ, ...cam.forward.map((x) => -x), ...plane]));
}

/** The part of the model (its path) a body (by its address) is of. (Of a proposal's side drawn over the other, marked
 *  `after:`: the part of that path.) */
const pathOf = (address) => { const a = address.replace(/^after:/, ""); return root ? `${root}/${a}` : a; };

/**
 * Bodies drawn in every view in place of the model's (`[{address, material, body}]`: a proposal's side; those of the
 * other side drawn over it marked `after:` in their addresses), or the model's again (null). Picked, coloured and
 * chosen by their addresses as the model's are.
 */
export function showBodies(bodies) {
  const json = bodies ? JSON.stringify(bodies) : null;
  if ((side?.json ?? null) === json) return;
  side = json ? { json, addresses: bodies.map((b) => b.address) } : null;
  for (const v of Object.values(views)) if (v.exactView?.view) { v.exactView.view.hide(hiddenBodies()); v.exactView.said = v.exactView.view.show(drawn() ?? "[]"); v.redraw(); }
}

/**
 * What a ray (scan frame) meets of the bodies a view draws, exactly (those hidden left out; in the box the view is cut
 * to, where it is): `{ path, address, face, t, n, at }`: the part's path, the body's address and the face's name, how
 * far along the ray (m), the way the face faces there (the model's frame, turned toward the ray's origin), and where
 * (m, the model's frame). Null where it meets none. `every`: all it meets, the nearest first, each where it first
 * meets it. (Where the page draws no bodies exactly: none.)
 */
export function partAt(v, ray, every = false) {
  const e = shown && v?.exactView?.view;
  if (!e) return every ? [] : null;
  const [ox, oy, oz] = toModel(ray.origin).map((x) => x * 1000), [dx, dy, dz] = unrot(ray.dir);
  const hit = (h) => ({ path: pathOf(h.address), address: h.address, face: h.face, t: h.t / 1000, n: h.normal, at: h.at.map((x) => x / 1000) });
  if (every) return JSON.parse(e.pick_all(ox, oy, oz, dx, dy, dz)).map(hit);
  const one = e.pick_ray(ox, oy, oz, dx, dy, dz);
  return one ? hit(JSON.parse(one)) : null;
}

/** How the bodies are coloured, other than by their materials, in every view: `paints` plain colours by part path
 *  (`[r, g, b, a?]` bytes, or `{ body, faces }`), `values` a value by part path drawn through `colormap`
 *  (`{ stops: [[value, [r, g, b]], …], range }`; null: no values drawn). */
let colours = { paints: "{}", values: "{}", colormap: null };
export function colourExact(paints, values, colormap) {
  const byAddress = (o) => JSON.stringify(Object.fromEntries(Object.entries(o).map(([path, x]) => [bodyOf(path), x]).filter(([a]) => a)));
  colours = { paints: byAddress(paints), values: byAddress(values), colormap: colormap ? JSON.stringify(colormap) : null };
  for (const v of Object.values(views)) colourIn(v);
}
function colourIn(v) {
  const e = v.exactView?.view;
  if (!e) return;
  e.paint(colours.paints);
  e.values(colours.values);
  e.colormap(colours.colormap ?? undefined);
  v.redraw();
}

/** The address of the body a part of the model (by its path: the part's own, under the model's name) is drawn by. */
export const bodyOf = (path) => {
  if (!path) return null;
  // (Of a proposal's side drawn over the other: by its `after:` address.)
  const after = path.startsWith("after:"), p = after ? path.slice(6) : path;
  const of = side?.addresses ?? addresses, a = of.find((x) => { const y = after ? x.slice(6) : x; return (after === x.startsWith("after:")) && (p === y || p.endsWith(`/${y}`)); });
  return a ?? null;
};

/** What of the body at an address is drawn as chosen in every view (none: nothing): all of it ("body"), or its face,
 *  edge or vertex at `index`. */
export function chooseExact(address, what = "body", index = 0) {
  for (const v of Object.values(views)) if (v.exactView?.view) { v.exactView.view.choose_in(address ?? undefined, what, index); v.redraw(); }
}

/** What of the bodies is under or near a pixel of a view (CSS pixels from its top left), as `what` says ("face",
 *  "edge", "vertex"): { address, what, index, name, at, … } (millimetres in the model's frame); null where none is. */
export function findExact(v, x, y, what) {
  if (!exactIn(v)) return null;
  const dpr = devicePixelRatio || 1;
  const hit = v.exactView.view.find(x * dpr, y * dpr, what, 8 * dpr);
  return hit ? JSON.parse(hit) : null;
}

// To tell what a 3D view is slow at (in the console): `topoView.debug({ count, untrimmed, flat, everyCurve })` asks
// every 3D view's renderer to count what its B-rep shader does, to draw faces without their trim test, to shade flat,
// or to test every curve of a trim (not only those of the point's strip); what is counted, and the GPU's time a frame,
// are at the head of the build log (and `topoTimes().frames`).
globalThis.topoView = {
  debug({ count = false, untrimmed = false, flat = false, everyCurve = false } = {}) {
    for (const v of Object.values(views)) v.exactView?.view.brep_debug(!!count, !!untrimmed, !!flat, !!everyCurve);
    recount();
    for (const v of Object.values(views)) v.redraw?.();
    return { count: !!count, untrimmed: !!untrimmed, flat: !!flat, everyCurve: !!everyCurve };
  },
};
