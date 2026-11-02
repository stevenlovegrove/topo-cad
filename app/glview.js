// A view of a scan with a model in it, and labels as HTML over its canvas, which takes the pointer. What is seen is
// drawn under the canvas (`under`), by what draws the scene (`drawnBy`: the exact view, scene/exact.js, Pangolino's
// renderer): the model, the scan's points (coloured by intensity, height, surface normal or offset from a plane), and
// the marks (lines and faint surfaces). Everything is in the scan's frame (z up, metres).
//
// The camera is what draws the scene's (topo-view's: its gizmo, and what moves it as the pointer says: a left drag
// pans, a right drag turns about what is under the cursor, the wheel zooms toward it; held square on to a slab, any
// drag pans; through a photo's camera, a drag slides its picture and the wheel zooms it). The page asks it where it is
// (`camera`), and sends it places (`go`, `frame`, `squareOn`, …). The page's own tools are offered the pointer first:
// grabbing what is under it, a window dragged with shift, the line tool; a click that moves nothing picks.
//
// `state` is the view state: plain data, so it can be saved, linked to and restored (its camera, `state.camera`, as
// the scene's camera last said: see `syncCamera`). `set(patch)` changes only what the patch names.

const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const add = (a, b) => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
const mulS = (a, s) => [a[0] * s, a[1] * s, a[2] * s];
const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
/** The box about points given flat ([x, y, z, x, y, z, ...]), and about `also` if given; null if there are none. */
const boxOf = (xyz, also = null) => {
  if (!xyz || xyz.length < 3) return also;
  const lo = also ? [...also.lo] : [Infinity, Infinity, Infinity], hi = also ? [...also.hi] : [-Infinity, -Infinity, -Infinity];
  for (let i = 0; i + 2 < xyz.length; i += 3) for (let k = 0; k < 3; k++) { const x = xyz[i + k]; if (x < lo[k]) lo[k] = x; if (x > hi[k]) hi[k] = x; }
  return { lo, hi };
};
const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const unit = (a) => { const n = Math.hypot(a[0], a[1], a[2]) || 1; return [a[0] / n, a[1] / n, a[2] / n]; };
export const vec = { sub, add, scale: mulS, dot, cross, unit };

export const COLOURS = ["intensity", "height", "normal", "depth", "confidence"];

/** A camera as the scene keeps it (`{ at, forward, up, ortho, flat, photo }`, the scan's frame) with its eye at `eye`,
 *  looking at `target`, upright (the world's z up, or `up` where it says which way is up on the screen); `ortho`: half
 *  the height it sees, seen parallel (none: in perspective). */
export function lookFrom(eye, target, { up = null, ortho = null } = {}) {
  const forward = unit(sub(target, eye));
  // (Looking straight down or up with no up said: the world's y is up on the screen.)
  let right = cross(forward, up ?? [0, 0, 1]);
  if (Math.hypot(...right) < 1e-6) right = cross(forward, [0, 1, 0]);
  right = unit(right);
  return { at: eye, forward, up: cross(right, forward), ortho, flat: false, photo: null };
}

function defaultState() {
  return {
    // Where the camera is (as the scene keeps it: `lookFrom`), the scan's frame: to begin with, from the south-west and
    // above, till it is fitted to what is drawn.
    camera: lookFrom([-7.1, -10.1, 7.8], [0, 0, 1.2]),
    // The box the view is clipped to (a slab: { c, u, v, n, h }, scan frame), and
    // whether the view is held square on to it (any drag slides it), as the camera last said.
    box: null, flat: false,
    // Whether free space (where the rays pass in the box) is shown under the points.
    free: false,
    // Whether a window dragged in the view takes every point in it, not only the surface nearest the viewer.
    selectAll: false,
    colour: "intensity",
    // Points the rays or the lidar's flags doubt (in a slab): "show", "fade" or "hide".
    doubt: "fade",
    // Height cuts, in the model's frame (null: none).
    layers: { scan: true, model: true, marks: true, issues: true },
    // On paper, not on black (where nothing is drawn under the canvas).
    paper: false,
    pointSize: 1,
    // Through a photo's camera (null: not): { name, origin, x, y, z (where the camera was, and its axes: x to the
    // right of its picture, y down it, z the way it looked), lens (its model and numbers, as the engine says one),
    // size (pixels), src (its picture), pan (the pixel of the picture at
    // the middle of the view), zoom (1: the whole picture fits) }. The model is then drawn over the picture as that
    // camera has it; a drag slides the picture and the wheel zooms it (pan and zoom as the camera last said), and
    // neither moves the camera, which is where the solve found it.
    photo: null,
    // How strongly the model tints a photo's picture where it is over it (0: not at all, only its edges drawn).
    tint: 0.2,
  };
}

// A handle's look, by its classes (`cls`), as markers: sRGB bytes with how opaque, lengths in points.
const ORANGE = [255, 150, 40, 255], INK = [22, 23, 27, 255];
const SAID = { own: [255, 122, 0, 255], ahead: [90, 200, 255, 255], disabled: [170, 170, 170, 115], other: [255, 255, 255, 191] };
/** The markers a handle is drawn as (one, or with a ring about it where it is pointed at). */
function handleMarkers(h) {
  const cls = new Set((h.cls || "").split(" ")), base = { id: h.id, at: h.pos, tip: h.title || undefined, press: "grab", along: h.along };
  if (cls.has("face")) return [{ ...base, shape: "square", size: 12, fill: cls.has("thin") ? [255, 224, 184, 255] : ORANGE, edge: INK, edge_width: 1.5 }];
  if (cls.has("mid")) return [{ ...base, shape: "dot", size: 14, fill: INK, edge: ORANGE, edge_width: 2.5 }];
  if (cls.has("yaw")) return [{ ...base, shape: "dot", size: 20, fill: INK, edge: ORANGE, edge_width: 1, text: { text: h.text, on: true, colour: ORANGE, background: [0, 0, 0, 0], size: 13 } }];
  if (cls.has("node")) return [{ ...base, shape: "dot", size: 11, fill: [230, 255, 122, 255], edge: INK, edge_width: 1.5 }];
  if (cls.has("seg")) return [{ ...base, text: { text: h.text, on: true, colour: [240, 255, 192, 255], background: [22, 23, 27, 209], border: [214, 255, 90, 140], size: 10.5 } }];
  if (cls.has("said")) {
    // (Said in the photo: a cross, of the page's own to drag, or of the script's, only to say what it is.)
    const own = cls.has("own"), edge = cls.has("disabled") ? SAID.disabled : cls.has("ahead") ? SAID.ahead : own ? SAID.own : SAID.other;
    const out = [{ ...base, shape: "cross", size: own ? 13 : 9, edge, edge_width: own ? 2 : 1, press: own ? "grab" : "pointer" }];
    if (cls.has("pointed")) out.push({ id: `${h.id} pointed`, at: h.pos, shape: "dot", size: 25, edge: [255, 240, 60, 255], edge_width: 2 });
    return out;
  }
  return [{ ...base, shape: "dot", size: 10, fill: ORANGE, edge: INK, edge_width: 1.5 }];
}

export class GLView {
  constructor(host) {
    this.host = host;
    this.canvas = document.createElement("canvas");
    this.canvas.style.cssText = "width:100%;height:100%;display:block;touch-action:none";
    this.canvas.style.position = "relative";
    host.append(this.canvas);
    /** The picture laid under the points as it was set (`setImage`), for what draws the scene. */
    this.picture = null;
    this.state = defaultState();
    this.handles = [];
    this.markerSets = {};
    /** "line": the next drag draws a line (onLine gets the rays at its ends). */
    this.tool = null;
    /** The model's floor in the scan (model z = scan z − zOffset). */
    this.zOffset = 0;
    this.floorZ = 0;
    this.clouds = { thin: null, slab: null };
    /** What draws the scene where the page can, in place of the canvas (`drawnBy`). */
    this.scene = null;
    /** The extent of each thing drawn ({ lo, hi }, or null), by what it is: see `sphere`. */
    this.extents = {};
    this.highlights = new Map();
    this.marks = null;
    this.over = {};
    this.labels = [];
    this.onPick = null; this.onChange = null; this.onLabel = null; this.onSelect = null;
    /** What is drawn under the canvas, each time it is drawn (`under(cam, w, h)`: true if it drew, and the canvas is
     *  then clear where it draws nothing): the exact view of a part script's bodies (scene/exact.js). */
    this.under = null;
    /** What of the scene a ray meets (a point, or none), said at once: what a key's switch of projection is about. */
    this.onPoint = null;
    this.dirty = false;
    this.bind();
    new ResizeObserver(() => this.redraw()).observe(host);
    this.redraw();
  }

  // ----- state --------------------------------------------------------------------

  /** Sets what `patch` names (layers merge), and redraws. A camera, and a photo looked through, go to the scene's
   *  camera at once. */
  set(patch, quiet) {
    const { layers, ...rest } = structuredClone(patch);
    Object.assign(this.state, rest);
    if (layers) Object.assign(this.state.layers, layers);
    // (Through a photo, the view is where its camera is: not framed after, as was asked before the scene drew.)
    if (rest.photo) this.framing = null;
    if ("camera" in rest || "photo" in rest || "flat" in rest) this.toScene(0);
    this.redraw();
    if (!quiet) this.onChange?.();
  }
  get() {
    this.syncCamera();
    return structuredClone(this.state);
  }
  redraw() {
    if (this.dirty) return;
    this.dirty = true;
    requestAnimationFrame(() => this.draw());
  }

  // ----- data ---------------------------------------------------------------------

  /** A set of points to draw highlighted, by key (null drops it). */
  setHighlight(key, cloud) {
    if (cloud) this.highlights.set(key, cloud);
    else this.highlights.delete(key);
    this.scene?.points(`highlight ${key}`, cloud, true);
    this.redraw();
  }
  /** A thinned scan (`which` "thin", or "thin N" of the Nth of several, each drawn where `cloudShown(which)` says, as
   *  ever unless it says) or every point of a slab ("slab", in `box`: drawn there in place of the thinned ones; null
   *  drops it). */
  setCloud(which, cloud, box = null) {
    this.clouds[which] = cloud;
    if (which === "slab") this.denseBox = cloud ? box : null;
    this.extents[`cloud ${which}`] = cloud ? boxOf(cloud.xyz) : null;
    this.scene?.points(which, cloud, false);
    this.redraw();
  }
  /**
   * What draws the scene (Pangolino's renderer: scene/exact.js), told of every cloud of points as it is set
   * (`points(key, cloud or null, highlighted)`), of the picture laid under them (`picture(img or null)`, as
   * `setImage` has it), of the marks and fills (`marks({ pos, col } or null)`, `fills(…)`), of what is drawn over
   * everything (`over(key, { pos, col, width } or null)`), marked at points over it (`markers(key, set or null)`), and flat
   * over it as a drag goes (`dragged(what, ends)`), and of the pins
   * (`pins(labels)`, `pin(id, shown)`); and asked first what the pointer is on (`pointer(what, event)`: whether a pin
   * took it). Told at once of those set before.
   */
  drawnBy(scene) {
    this.scene = scene;
    for (const [k, c] of Object.entries(this.clouds)) if (c) scene.points(k, c, false);
    for (const [k, c] of this.highlights) scene.points(`highlight ${k}`, c, true);
    scene.picture(this.picture ?? null);
    scene.marks(this.marks);
    scene.fills(this.fills ?? null);
    this.sendSlates();
    for (const [k, o] of Object.entries(this.over)) scene.over(k, o);
    for (const [k, m] of Object.entries(this.markerSets)) scene.markers(k, m);
    scene.pins(this.labels);
    // Where the state says (its camera, or its photo's); then framed, where that was asked before it could be.
    this.toScene(0);
    if (this.framing) { const [c, r] = this.framing; this.framing = null; this.frame(c, r, 0); }
    this.redraw();
  }
  /** Which clouds are drawn now, of the scan's (each by its key) and the highlighted: the thinned ones the host shows,
   *  and every point of a slab in their place where the view is clipped to the box those were fetched for. */
  cloudsDrawn() {
    const s = this.state, slab = !!(s.box && this.clouds.slab && JSON.stringify(this.denseBox) === JSON.stringify(s.box));
    const thin = Object.keys(this.clouds).filter((k) => this.clouds[k] && k.startsWith("thin") && (this.cloudShown?.(k) ?? true));
    return { scan: s.layers.scan ? (slab ? ["slab"] : thin) : [], highlights: [...this.highlights.keys()].map((k) => `highlight ${k}`) };
  }

  /** The extent of what else is drawn (`key`; `box`: { lo, hi }, scan frame, or null): the model's, which is drawn
   *  under the canvas (`under`), so that the camera sees it whole and in depth. */
  setExtent(key, box) {
    this.extents[key] = box;
    this.redraw();
  }
  /**
   * What is drawn, as a sphere about it (the model, the scan, the marks), or null: the camera's near and far planes
   * are set from it, so that a building and a part millimetres across are each seen whole and in depth.
   */
  sphere(only) {
    let box = null;
    for (const [k, b] of Object.entries(this.extents)) if (b && (!only || only(k))) box = box ? { lo: box.lo.map((x, k) => Math.min(x, b.lo[k])), hi: box.hi.map((x, k) => Math.max(x, b.hi[k])) } : b;
    if (!box) return null;
    return { c: box.lo.map((x, k) => 0.5 * (x + box.hi[k])), r: Math.max(1e-6, 0.5 * Math.hypot(...box.hi.map((x, k) => x - box.lo[k]))) };
  }
  /** Marks: line segments (boxes, a section's plane, picks, surfaces): two points each, and a colour (RGBA bytes) at
   *  each. */
  setMarks(pos, col) {
    this.marks = pos.length ? { pos, col } : null;
    this.extents.marks = boxOf(pos);
    this.scene?.marks(this.marks);
    this.redraw();
  }
  /**
   * A picture laid in the scene, under the points: { corners, w, h, data }
   * (corners: where its low-low, high-low, low-high and high-high corners
   * are; a byte per cell, row 0 at the low side). Null drops it.
   */
  setImage(img) {
    this.picture = img;
    this.scene?.picture(img);
    this.redraw();
  }
  /** What is drawn over everything, whatever is in front of it (`key`, one of the page's: the deflected shape, …):
   *  line segments as `setMarks` has them, `width` points wide; null, none. */
  setOver(key, o) {
    if (o && !o.pos.length) o = null;
    if (o) this.over[key] = o;
    else delete this.over[key];
    this.scene?.over(key, o);
    this.redraw();
  }
  /** What a drag draws flat over the view (`what`: "band", the window it chooses by; "rule", the line it lays), from
   *  where it began to where it is (`ends`: [x0, y0, x1, y1], CSS pixels of the canvas); null, none. */
  dragged(what, ends) {
    this.scene?.dragged(what, ends);
    this.redraw();
  }
  /** Faint surfaces (a selected slab's faces, a plane's patch): triangles, each vertex with an RGBA colour. Points in front of one hide it; those behind are tinted. */
  setFills(pos, col) {
    this.fills = pos.length ? { pos, col } : null;
    this.scene?.fills(this.fills);
    this.redraw();
  }
  /** Pictures standing in the scene: [{ key, corners, stamp, image: { w, h, rgba } }] (`stamp`: says when its pixels or size are
   *  another's). */
  setSlates(slates) {
    this.slates = slates;
    this.sendSlates();
  }
  /** More of them, of one of the page's that changes apart from the rest (`key`), and often: others in a session. */
  setSlateSet(key, slates) {
    (this.slateSets ??= {})[key] = slates;
    this.sendSlates();
  }
  sendSlates() {
    this.scene?.slates([...(this.slates ?? []), ...Object.values(this.slateSets ?? {}).flat()]);
    this.redraw();
  }
  /** Labels at places, pins: [{ id, pos, text, colour, title, on, dim }]; drawn by what draws the scene (`drawnBy`),
   *  each shown where the issues are and the view is not cut away from it (`draw`). */
  setLabels(labels) {
    this.labels = labels;
    this.scene?.pins(labels);
    this.redraw();
  }
  /**
   * Handles to drag, at places: [{ id, pos, cls, text, title, along }].
   * `onHandle(id, "start" | "move" | "end", ray, event)` as one is dragged;
   * `onHandleMenu(id, event)` on a right-click. One whose `along` (the line
   * it moves on) points at the viewer is not shown: it could not be dragged.
   */
  setHandles(list) {
    this.handles = list;
    this.setMarkers("handles", list.length ? { markers: list.flatMap(handleMarkers) } : null);
  }
  /**
   * Marks at points, drawn by the scene over what it shows (`key`: one of the page's, "handles" GLView's own):
   * { markers, lines, regions }, as topo-view's `markers` says them (points in the scan's frame); null, none. What the
   * pointer does to them is said to `onMarker(event, pointerEvent)` (`event`: { set, kind: "hover" | "press" | "drag" |
   * "release", id, button, moved }); to the handles, as `setHandles` says.
   */
  setMarkers(key, set) {
    if (set) this.markerSets[key] = set;
    else delete this.markerSets[key];
    this.scene?.markers(key, set);
    this.redraw();
  }
  /** What is under the cursor, said by the scene as its markers say what they are (once the cursor rests); null,
   *  nothing. */
  sceneTip(text) {
    if (this.scene?.tip(text || null)) this.redraw();
  }
  /** What the pointer did to a marker (`ev`), as the pointer event `e` was. */
  markerEvent(ev, e) {
    if (ev.set !== "handles") return this.onMarker?.(ev, e);
    const r = this.canvas.getBoundingClientRect(), ray = () => this.ray(e.clientX - r.left, e.clientY - r.top), id = ev.id;
    if (ev.kind === "hover") {
      // (The cursor on one, or off: for whoever shows what it is a handle of.)
      if (this.handleHovered && this.handleHovered !== id) this.onHandleHover?.(this.handleHovered, false);
      if (id && id !== this.handleHovered) this.onHandleHover?.(id, true);
      this.handleHovered = id;
    } else if (ev.kind === "press" && ev.button === 0) {
      this.handleHeld = id;
      this.onHandle?.(id, "start", ray(), e);
    } else if (ev.kind === "press" && ev.button === 2) this.onHandleMenu?.(id, e);
    else if (ev.kind === "drag" && this.handleHeld === id) this.onHandle?.(id, "move", ray(), e);
    else if (ev.kind === "release" && this.handleHeld === id) {
      this.handleHeld = null;
      this.onHandle?.(id, "end", ray(), e);
    }
  }

  // ----- camera -------------------------------------------------------------------

  /** The state's camera given to the scene's, over `ms` milliseconds: through its photo, if it has one (where it is
   *  panned and zoomed to), held square on if it is. */
  toScene(ms) {
    const s = this.state, photo = s.photo ? { ...s.photo, pan: s.photo.pan ?? [s.photo.size[0] / 2, s.photo.size[1] / 2], zoom: s.photo.zoom || 1 } : null;
    this.scene?.setCamera({ ...s.camera, flat: !!s.flat && !photo, photo }, ms);
  }

  /** The state's camera as the scene's now is: and what follows from it, a photo's pan and zoom, whether the view is
   *  held square on, and, where it has been turned out of a photo (by its gizmo), the photo let go of. (Through a
   *  photo, the camera kept is the view's own from before it, to go back to.) */
  syncCamera() {
    const c = this.scene?.cameraState();
    if (!c) return;
    const s = this.state, wasPhoto = s.photo;
    if (wasPhoto && c.photo) { s.photo = { ...wasPhoto, pan: c.photo.pan, zoom: c.photo.zoom }; return; }
    s.camera = { ...c, photo: null };
    s.flat = !!c.flat;
    if (wasPhoto) { s.photo = null; this.onPhotoGone?.(); }
  }

  /** What the view looks at: the point along the way it looks as deep as the middle of what is drawn (no nearer than
   *  a few centimetres), or a few metres off where nothing is. */
  lookingAt() {
    const c = this.state.camera, sp = this.sphere();
    const depth = sp ? Math.max(0.05, dot(sub(sp.c, c.at), c.forward)) : 5;
    return add(c.at, mulS(c.forward, depth));
  }

  /** The camera (as the scene keeps it, the scan's frame: `lookFrom`) gone to over `ms` milliseconds (as the view's
   *  moves are, by default), out of a photo's: where it was let go of, its photo too. */
  go(camera, ms = 450) {
    const s = this.state;
    if (s.photo && !camera.photo) { s.photo = null; this.onPhotoGone?.(); }
    s.camera = { ...camera, photo: null };
    s.flat = !!camera.flat;
    this.toScene(ms);
    this.redraw();
    this.onChange?.();
  }

  /** The camera put here at once, as often as it is said to be elsewhere: someone else's, being followed. Said to nobody. */
  track(camera) {
    if (this.state.photo) { this.state.photo = null; this.onPhotoGone?.(); }
    this.state.camera = camera;
    this.state.flat = false;
    this.toScene(0);
    this.redraw();
  }

  /** The camera as it is drawn: the matrix from the scan's frame to the canvas's clip space (`vp`), its axes (`right`,
   *  `up`, `forward`), where its eye is, whether it sees parallel (`ortho`, half the height and width seen `hh`, `hw`),
   *  how near and far it sees, how wide in perspective (`fov`, top to bottom), the canvas's size (CSS pixels), and
   *  through a photo's camera what of its picture is where, and its lens (`through`). As the scene draws it (while it moves, where it
   *  is on its way); null where nothing draws the scene (yet, or here: no WebGPU). */
  camera() {
    const s = this.state, w = this.canvas.clientWidth || 1, h = this.canvas.clientHeight || 1;
    const c = this.scene?.camera();
    if (!c) return null;
    const ortho = c.ortho != null, p = s.photo;
    const through = c.through && p ? { k: c.through.k, pan: c.through.pan, lens: p.lens } : null;
    return { vp: c.vp, right: c.right, up: c.up, forward: c.forward, eye: c.eye, ortho, hh: c.ortho, hw: ortho ? c.ortho * (w / h) : undefined, w, h, near: c.near, far: c.far, fov: c.fov, through };
  }

  /** Where a point is on the canvas (CSS pixels), or null if behind the viewer. (Through a photo's camera: as its lens sees.) */
  project(cam, p) {
    if (cam.through) return this.scene.seenAt(p);
    const m = cam.vp;
    const x = m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12], y = m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13], w = m[3] * p[0] + m[7] * p[1] + m[11] * p[2] + m[15];
    if (w <= 1e-6) return null;
    return [(x / w + 1) * 0.5 * cam.w, (1 - y / w) * 0.5 * cam.h];
  }

  /** Through a photo's camera: the pixel of its picture at a canvas position. */
  pixel(cam, x, y) {
    const q = cam.through;
    return [q.pan[0] + (x - cam.w / 2) / q.k, q.pan[1] + (y - cam.h / 2) / q.k];
  }

  /** How big a pixel of the view is along a ray: [m, m per metre along it] (seen parallel, the first; in perspective,
   *  the second; through a photo's camera, by its scale). */
  pixelSize() {
    const cam = this.camera();
    if (cam.through) { const a = 1 / ((cam.through.lens?.resolution ?? 1) * cam.through.k); return cam.through.lens?.parallel ? [a, 0] : [0, a]; }
    return cam.ortho ? [(2 * cam.hh) / cam.h, 0] : [0, (2 * Math.tan(cam.fov / 2)) / cam.h];
  }

  /** The ray through a canvas position: { origin, dir }. */
  ray(x, y) {
    const cam = this.camera();
    const nx = (2 * x) / cam.w - 1, ny = 1 - (2 * y) / cam.h;
    if (cam.through) return this.scene.rayAt(x, y);
    if (cam.ortho) return { origin: add(cam.eye, add(mulS(cam.right, nx * cam.hw), mulS(cam.up, ny * cam.hh))), dir: cam.forward };
    const th = Math.tan(cam.fov / 2);
    return { origin: cam.eye, dir: unit(add(cam.forward, add(mulS(cam.right, nx * th * (cam.w / cam.h)), mulS(cam.up, ny * th)))) };
  }

  /** The cuts in force, as the workspace's pick takes them. */
  cuts() {
    return { slab: this.state.box || undefined };
  }

  /** Frames a sphere (keeping the direction of view), out of a photo's camera, over `ms` milliseconds; and turns about
   *  its middle from then. */
  frame(centre, radius, ms = 450) {
    // (Before anything draws the scene: framed once it does.)
    if (!this.scene) { this.framing = [centre, radius]; return; }
    if (this.state.photo) { this.state.photo = null; this.onPhotoGone?.(); }
    this.scene.fit(centre, radius, ms);
    if (!ms) this.syncCamera();
    this.redraw();
  }

  /** The view turned to look at a point (as deep along the way it looks as it is), the way it looked; and turning
   *  about it, where nothing is under the cursor, and by its gizmo. */
  centreOn(p, ms) {
    const c = this.state.camera, depth = dot(sub(p, c.at), c.forward), back = depth > 0.01 ? depth : Math.hypot(...sub(p, c.at));
    this.go({ ...c, at: add(p, mulS(c.forward, -back)) }, ms);
    this.scene?.orbitAbout(p);
  }

  /** Looks square on at a box (from its `n` side, its `v` up), fitted in the view, seen parallel; a drag then slides it. */
  squareOn(box, ms) {
    const { c, v, n, h } = box, aspect = (this.canvas.clientWidth || 4) / (this.canvas.clientHeight || 3);
    const back = Math.max(2, 2 * Math.hypot(...h));
    this.go({ at: add(c, mulS(n, back)), forward: mulS(n, -1), up: v, ortho: 1.06 * Math.max(h[1], h[0] / aspect), flat: true, photo: null }, ms);
  }

  /** Switched between perspective and parallel, about what is under the cursor (`at`, CSS pixels of the canvas; none,
   *  its middle): it keeps its place and its size there. */
  switchProjection(at = null) {
    if (!this.scene || this.state.photo) return;
    const w = this.canvas.clientWidth || 1, h = this.canvas.clientHeight || 1;
    this.scene.switchProjection(at?.x ?? w / 2, at?.y ?? h / 2);
    this.redraw();
  }

  /** A view state (or part of one: a link's, a view kept) taken on: its camera gone to over a moment, anything else in
   *  it set at once. */
  animate(patch, ms = 450) {
    const { camera, ...rest } = patch;
    if (Object.keys(rest).length) this.set(rest, true);
    if (!camera) return this.onChange?.();
    // (Through a photo, the view's own camera is kept for after it, and the photo's is where it is.)
    if (this.state.photo) { this.state.camera = { ...camera, photo: null }; this.toScene(0); this.redraw(); return this.onChange?.(); }
    this.go(camera, ms);
  }

  /** The pointer offered to what draws the scene (`what`: "move", "down", "drag", "up", "leave"): its pins, its gizmo,
   *  then its camera. What took it ("pin", "gizmo", "camera", or ""), the cursor it says the canvas is to be drawn
   *  with there ("": none, the host's), and whether its camera moved. */
  scenePointer(what, e) {
    const r = this.scene?.pointer(what, e);
    for (const ev of r?.markers ?? []) this.markerEvent(ev, e);
    // (Whether the cursor is on a marker that says what it is: a right-click there asks that, not for a menu.)
    this.markerSays = !!r?.says;
    return { taken: r?.taken ?? "", cursor: r?.cursor ?? "", moved: !!r?.moved };
  }

  bind() {
    const c = this.canvas;
    let drag = null;
    c.addEventListener("contextmenu", (e) => e.preventDefault());
    c.addEventListener("pointerdown", (e) => {
      // (Nothing draws the scene yet, or here: nothing to press on.)
      if (!this.scene) return;
      try { c.setPointerCapture(e.pointerId); } catch (_) {}
      // The page's own tools first: what the host has under the cursor to be dragged (the view is not moved by it),
      // a window of points marked with shift, a line drawn with the line tool.
      if (e.button === 0 && !e.shiftKey && this.onGrab?.(e)) { drag = { grab: true }; return; }
      const line = this.tool === "line" && e.button === 0, select = !line && e.shiftKey && e.button === 0;
      drag = { x: e.clientX, y: e.clientY, button: e.button, line, select, moved: false };
      // Else the scene's: a pin, the gizmo, or the camera, which a drag moves.
      if (!line && !select) drag.taken = this.scenePointer("down", e).taken;
    });
    c.addEventListener("pointermove", (e) => {
      // (Nothing draws the scene yet, or here: nothing under the pointer.)
      if (!this.scene) return;
      if (!drag) {
        // (Over a pin or the gizmo, nothing of the scene under it is hovered, and the pointer is theirs.)
        const r = this.scenePointer("move", e);
        if (this.pinCursor && !r.cursor) c.style.cursor = "";
        this.pinCursor = r.cursor;
        this.onHover?.(r.taken ? null : e);
        if (r.cursor) c.style.cursor = r.cursor;
        if (r.moved) this.redraw();
        return;
      }
      if (drag.grab) { this.onGrabMove?.(e); return; }
      const dx = e.clientX - drag.x, dy = e.clientY - drag.y;
      if (Math.abs(dx) + Math.abs(dy) > 3) drag.moved = true;
      if (drag.taken != null) {
        if (this.scenePointer("drag", e).moved) this.redraw();
        return;
      }
      if (!drag.moved) return;
      // (The band chosen by, or the rule laid, from where the drag began to where it is: the scene draws them.)
      const r = c.getBoundingClientRect(), ends = [drag.x - r.left, drag.y - r.top, e.clientX - r.left, e.clientY - r.top];
      if (drag.select) this.dragged("band", ends);
      else if (drag.line) this.dragged("rule", ends);
    });
    c.addEventListener("pointerup", (e) => {
      const d = drag;
      drag = null;
      if (!d) return;
      if (d.grab) { this.onGrabEnd?.(e); return; }
      const r = c.getBoundingClientRect();
      if (d.line) {
        this.dragged("rule", null);
        if (d.moved) this.onLine?.(this.ray(d.x - r.left, d.y - r.top), this.ray(e.clientX - r.left, e.clientY - r.top));
        return;
      }
      if (d.select) {
        this.dragged("band", null);
        if (!d.moved) return;
        // The window, as the view projects it: what the workspace needs to find the points in it.
        const cam = this.camera(), nx = (x) => (2 * (x - r.left)) / cam.w - 1, ny = (y) => 1 - (2 * (y - r.top)) / cam.h;
        this.onSelect?.({ vp: Array.from(cam.vp), rect: [nx(d.x), ny(d.y), nx(e.clientX), ny(e.clientY)], eye: cam.eye, forward: cam.forward }, e);
        return;
      }
      const up = this.scenePointer("up", e);
      if (up.moved) this.redraw();
      // (A pin's, a marker's or the gizmo's: theirs alone.)
      if (d.taken === "pin" || d.taken === "marker" || d.taken === "gizmo") { if (d.taken === "gizmo") { this.syncCamera(); this.onChange?.(); } return; }
      if (d.moved) { this.syncCamera(); this.onChange?.(); }
      else if (d.button === 0) this.onPick?.(this.ray(e.clientX - r.left, e.clientY - r.top), e);
      // A right-click (not a turn): the host's menu for what is under the cursor; on a marker that says what it is,
      // that, which the scene says.
      else if (d.button === 2 && !this.markerSays) this.onContext?.(this.ray(e.clientX - r.left, e.clientY - r.top), e);
    });
    c.addEventListener("pointerleave", (e) => { this.scenePointer("leave", e); this.onHover?.(null); });
    // (The pointer taken away while something is held, by the system: what the host was dragging is let go of, with nothing done.)
    c.addEventListener("pointercancel", (e) => { const d = drag; drag = null; this.dragged("band", null); this.dragged("rule", null); if (d?.grab) this.onGrabEnd?.(e, true); else if (d?.taken != null) this.scenePointer("up", e); });
    c.addEventListener("wheel", (e) => {
      e.preventDefault();
      // (Toward the view where the wheel turns away: lines of it, as a notch is, from pixels or lines or pages.)
      const lines = -e.deltaY / (e.deltaMode === 1 ? 1 : e.deltaMode === 2 ? 0.05 : 40);
      if (this.scene?.wheel(e, lines)) {
        this.redraw();
        if (this.state.photo) this.onPhotoZoom?.();
      }
      clearTimeout(this.wheelTimer);
      this.wheelTimer = setTimeout(() => { this.syncCamera(); this.onChange?.(); }, 250);
    }, { passive: false });
  }

  // ----- drawing ------------------------------------------------------------------

  draw() {
    this.dirty = false;
    const c = this.canvas, dpr = window.devicePixelRatio || 1;
    const w = Math.round(c.clientWidth * dpr), h = Math.round(c.clientHeight * dpr);
    if (!w || !h) return;
    if (c.width !== w || c.height !== h) { c.width = w; c.height = h; }
    const s = this.state, cam = this.camera();
    // (Nothing draws the scene yet, or here (no WebGPU): the view says so, and nothing is drawn over it.)
    if (!cam) return;
    // The pins shown: where the issues are, and the view is not cut away from them (by more than a little).
    const box = s.box;
    for (const l of this.labels) {
      const q = box && sub(l.pos, box.c);
      const cut = box && [box.u, box.v, box.n].some((a, k) => Math.abs(dot(a, q)) > Math.max(box.h[k], 0.3));
      this.scene?.pin(l.id, !!s.layers.issues && !cut);
    }
    // (What is seen is drawn under it, from the same camera; through a photo's camera, its picture too. Where nothing
    // is, the canvas is the paper, or the dark, it would be drawn on.)
    const under = !!this.under?.(cam, w, h);
    c.style.background = under ? "transparent" : s.paper ? "#f7f7f4" : "#16171b";
    // What the host draws over the view to match it (a compass; what is drawn flat over it).
    this.onDraw?.(cam);
  }
}
