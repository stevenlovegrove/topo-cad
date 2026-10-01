// A WebGL2 view of a scan with a model over it: points (coloured by
// intensity, height, surface normal or offset from a plane), the model's
// members (faces and edges), marks (lines), and labels as HTML over the
// canvas. Everything is in the scan's frame (z up, metres).
//
// One class draws both kinds of view:
//   "3d"       an orbit camera (perspective or orthographic) about a target;
//   "section"  an orthographic camera looking along −n at the slab about a
//              plane (points within `half` of it), panned and zoomed in the
//              plane. Sections are views of the real points, not images.
//
// `state` is the view state: plain data, so it can be saved, linked to and
// restored. `set(patch)` changes only what the patch names.

const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const add = (a, b) => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
const mulS = (a, s) => [a[0] * s, a[1] * s, a[2] * s];
const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const unit = (a) => { const n = Math.hypot(a[0], a[1], a[2]) || 1; return [a[0] / n, a[1] / n, a[2] / n]; };
export const vec = { sub, add, scale: mulS, dot, cross, unit };

// Column-major 4×4 matrices.
function matMul(a, b) {
  const o = new Float32Array(16);
  for (let c = 0; c < 4; c++) for (let r = 0; r < 4; r++) o[c * 4 + r] = a[r] * b[c * 4] + a[4 + r] * b[c * 4 + 1] + a[8 + r] * b[c * 4 + 2] + a[12 + r] * b[c * 4 + 3];
  return o;
}
function viewMatrix(eye, s, u, f) {
  return new Float32Array([s[0], u[0], -f[0], 0, s[1], u[1], -f[1], 0, s[2], u[2], -f[2], 0, -dot(s, eye), -dot(u, eye), dot(f, eye), 1]);
}
function perspective(fovy, aspect, near, far) {
  const f = 1 / Math.tan(fovy / 2);
  return new Float32Array([f / aspect, 0, 0, 0, 0, f, 0, 0, 0, 0, (far + near) / (near - far), -1, 0, 0, (2 * far * near) / (near - far), 0]);
}
function orthographic(hw, hh, near, far) {
  return new Float32Array([1 / hw, 0, 0, 0, 0, 1 / hh, 0, 0, 0, 0, -2 / (far - near), 0, 0, 0, -(far + near) / (far - near), 1]);
}

const FOVY = (45 * Math.PI) / 180;
const D2R = Math.PI / 180;
export const COLOURS = ["intensity", "height", "normal", "depth"];

const POINT_VS = `#version 300 es
in vec3 a_pos; in float a_int; in vec3 a_nrm;
uniform mat4 u_vp; uniform float u_size; uniform float u_persp; uniform int u_mode;
uniform vec2 u_z; uniform vec4 u_plane; uniform float u_half; uniform vec3 u_toEye; uniform float u_floor;
out vec3 v_col;
vec3 hue(float h) { return clamp(abs(fract(h + vec3(0., 2. / 3., 1. / 3.)) * 6. - 3.) - 1., 0., 1.); }
void main() {
  float off = dot(u_plane.xyz, a_pos) - u_plane.w;
  if (a_pos.z < u_z.x || a_pos.z > u_z.y || (u_half > 0. && abs(off) > u_half)) { gl_Position = vec4(2., 2., 2., 1.); gl_PointSize = 0.; return; }
  gl_Position = u_vp * vec4(a_pos, 1.);
  gl_PointSize = u_persp > 0. ? clamp(u_size * 6. / max(gl_Position.w, .5), 1., u_size * 2.) : u_size;
  bool surf = dot(a_nrm, a_nrm) > .2;
  // Lit a little by how squarely the surface faces the viewer.
  float lit = surf ? .55 + .45 * abs(dot(normalize(a_nrm), u_toEye)) : .8;
  if (u_mode == 1) v_col = hue(.66 - .66 * clamp((a_pos.z - u_floor) / 3.6, 0., 1.)) * (.35 + .65 * lit);
  else if (u_mode == 2) v_col = surf ? .2 + .8 * abs(normalize(a_nrm)) : vec3(.42);
  else if (u_mode == 3) { float t = u_half > 0. ? clamp(off / u_half, -1., 1.) : 0.; v_col = hue(.33 - .33 * t) * (.55 + .45 * lit); }
  else if (u_mode == 4) v_col = vec3(1., .82, .1);
  else v_col = vec3(.12 + .88 * pow(a_int, .55)) * vec3(1., .98, .94) * lit;
}`;
const POINT_FS = `#version 300 es
precision mediump float;
in vec3 v_col; out vec4 o;
void main() { vec2 d = gl_PointCoord - .5; if (dot(d, d) > .25) discard; o = vec4(v_col, 1.); }`;
const FLAT_VS = `#version 300 es
in vec3 a_pos; in vec4 a_col;
uniform mat4 u_vp;
out vec4 v_col; out vec3 v_pos;
void main() { gl_Position = u_vp * vec4(a_pos, 1.); v_col = a_col; v_pos = a_pos; }`;
const FLAT_FS = `#version 300 es
precision mediump float;
in vec4 v_col; in vec3 v_pos; out vec4 o;
uniform vec2 u_z; uniform vec4 u_plane; uniform float u_half;
void main() {
  if (v_pos.z < u_z.x || v_pos.z > u_z.y) discard;
  if (u_half > 0. && abs(dot(u_plane.xyz, v_pos) - u_plane.w) > u_half) discard;
  o = v_col;
}`;

function program(gl, vs, fs) {
  const p = gl.createProgram();
  for (const [type, src] of [[gl.VERTEX_SHADER, vs], [gl.FRAGMENT_SHADER, fs]]) {
    const s = gl.createShader(type);
    gl.shaderSource(s, src);
    gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s));
    gl.attachShader(p, s);
  }
  gl.linkProgram(p);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(p));
  const u = {};
  for (let i = 0; i < gl.getProgramParameter(p, gl.ACTIVE_UNIFORMS); i++) {
    const name = gl.getActiveUniform(p, i).name;
    u[name] = gl.getUniformLocation(p, name);
  }
  return { p, u };
}

export function defaultState(kind) {
  return {
    kind,
    // 3d: an orbit about `target`, seen from compass bearing `az`, `el` up (degrees).
    target: [0, 0, 1.2], az: 215, el: 28, dist: 14, ortho: false, half: 6,
    // section: the plane and slab, and where in the plane the view is.
    section: null, centre: [0, 0], zoom: 4,
    colour: kind === "section" ? "depth" : "intensity",
    // Height cuts, in the model's frame (null: none).
    zMin: null, zMax: null,
    layers: { scan: true, model: true, marks: true, issues: true },
    pointSize: 2,
  };
}

export class GLView {
  constructor(host, kind) {
    this.host = host;
    this.canvas = document.createElement("canvas");
    this.canvas.style.cssText = "width:100%;height:100%;display:block;touch-action:none";
    this.overlay = document.createElement("div");
    this.overlay.style.cssText = "position:absolute;inset:0;pointer-events:none;overflow:hidden";
    host.append(this.canvas, this.overlay);
    const gl = (this.gl = this.canvas.getContext("webgl2", { antialias: true, preserveDrawingBuffer: true }));
    if (!gl) throw new Error("this browser has no WebGL2");
    this.points = program(gl, POINT_VS, POINT_FS);
    this.flat = program(gl, FLAT_VS, FLAT_FS);
    this.state = defaultState(kind);
    /** The model's floor in the scan (model z = scan z − zOffset). */
    this.zOffset = 0;
    this.floorZ = 0;
    this.clouds = { thin: null, slab: null };
    this.highlights = new Map();
    this.tris = null; this.lines = null; this.marks = null;
    this.labels = [];
    this.labelEls = new Map();
    this.onPick = null; this.onChange = null; this.onLabel = null; this.onSelect = null;
    this.band = document.createElement("div");
    this.band.style.cssText = "position:absolute;border:1px dashed #ffd21a;background:rgba(255,210,26,.12);display:none";
    this.overlay.appendChild(this.band);
    this.dirty = false;
    this.bind();
    new ResizeObserver(() => this.redraw()).observe(host);
    this.redraw();
  }

  // ----- state --------------------------------------------------------------------

  /** Sets what `patch` names (layers merge), and redraws. */
  set(patch, quiet) {
    const { layers, ...rest } = patch;
    Object.assign(this.state, structuredClone(rest));
    if (layers) Object.assign(this.state.layers, layers);
    this.redraw();
    if (!quiet) this.onChange?.();
  }
  get() {
    return structuredClone(this.state);
  }
  redraw() {
    if (this.dirty) return;
    this.dirty = true;
    requestAnimationFrame(() => this.draw());
  }

  // ----- data ---------------------------------------------------------------------

  cloudBuffers(c) {
    const gl = this.gl;
    const vao = gl.createVertexArray();
    gl.bindVertexArray(vao);
    const attr = (name, data, size, type, norm) => {
      const loc = gl.getAttribLocation(this.points.p, name);
      gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
      gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
      gl.enableVertexAttribArray(loc);
      gl.vertexAttribPointer(loc, size, type, norm, 0, 0);
    };
    attr("a_pos", c.xyz, 3, gl.FLOAT, false);
    attr("a_int", c.intensity, 1, gl.UNSIGNED_BYTE, true);
    attr("a_nrm", c.normal, 3, gl.BYTE, true);
    gl.bindVertexArray(null);
    return { vao, count: c.count };
  }
  /** The thinned scan (`which` "thin") or a section's slab ("slab"; null drops it). */
  /** A set of points to draw highlighted, by key (null drops it). */
  setHighlight(key, cloud) {
    if (cloud) this.highlights.set(key, this.cloudBuffers(cloud));
    else this.highlights.delete(key);
    this.redraw();
  }
  setCloud(which, cloud) {
    this.clouds[which] = cloud ? this.cloudBuffers(cloud) : null;
    this.redraw();
  }

  flatBuffers(pos, col) {
    const gl = this.gl;
    const vao = gl.createVertexArray();
    gl.bindVertexArray(vao);
    const a = gl.getAttribLocation(this.flat.p, "a_pos"), c = gl.getAttribLocation(this.flat.p, "a_col");
    gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
    gl.bufferData(gl.ARRAY_BUFFER, pos, gl.STATIC_DRAW);
    gl.enableVertexAttribArray(a);
    gl.vertexAttribPointer(a, 3, gl.FLOAT, false, 0, 0);
    gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
    gl.bufferData(gl.ARRAY_BUFFER, col, gl.STATIC_DRAW);
    gl.enableVertexAttribArray(c);
    gl.vertexAttribPointer(c, 4, gl.UNSIGNED_BYTE, true, 0, 0);
    gl.bindVertexArray(null);
    return { vao, count: pos.length / 3 };
  }
  /** The model: faces as triangles and edges as line segments, each vertex [x, y, z] with an RGBA colour. */
  setModel(tris, triCol, lines, lineCol) {
    this.tris = tris.length ? this.flatBuffers(tris, triCol) : null;
    this.lines = lines.length ? this.flatBuffers(lines, lineCol) : null;
    this.redraw();
  }
  /** Marks: line segments (boxes, a section's plane, picks, surfaces). */
  setMarks(pos, col) {
    this.marks = pos.length ? this.flatBuffers(pos, col) : null;
    this.redraw();
  }
  /** Labels at places: [{ id, pos, text, colour, title, on }]. */
  setLabels(labels) {
    this.labels = labels;
    for (const [id, el] of this.labelEls) if (!labels.some((l) => l.id === id)) { el.remove(); this.labelEls.delete(id); }
    for (const l of labels) {
      let el = this.labelEls.get(l.id);
      if (!el) {
        el = document.createElement("div");
        // A dot at the place, and the label set off from it on a leader, so it does not cover what it points at.
        el.className = "pin";
        el.innerHTML = `<i class="dot"></i><i class="lead"></i><span class="tag"></span>`;
        el.querySelector(".tag").ondblclick = (e) => { e.stopPropagation(); this.onLabelZoom?.(l.id); };
        el.querySelector(".tag").onclick = (e) => { e.stopPropagation(); this.onLabel?.(l.id); };
        el.querySelector(".tag").onpointerdown = (e) => e.stopPropagation();
        this.overlay.appendChild(el);
        this.labelEls.set(l.id, el);
      }
      el.querySelector(".tag").textContent = l.text;
      el.title = l.title || "";
      el.style.setProperty("--pin", l.colour || "#1f5fbf");
      el.classList.toggle("on", !!l.on);
      el.classList.toggle("dim", !!l.dim);
    }
    this.redraw();
  }

  // ----- camera -------------------------------------------------------------------

  /** The camera: view-projection matrix, basis (right, up, forward), eye. */
  camera() {
    const s = this.state, w = this.canvas.clientWidth || 1, h = this.canvas.clientHeight || 1, aspect = w / h;
    if (s.kind === "section" && s.section) {
      const { origin, n, u, v } = s.section;
      const c = add(origin, add(mulS(u, s.centre[0]), mulS(v, s.centre[1])));
      const eye = add(c, mulS(n, 200)), f = mulS(n, -1);
      return { vp: matMul(orthographic(s.zoom * aspect, s.zoom, 1, 400), viewMatrix(eye, u, v, f)), right: u, up: v, forward: f, eye, ortho: true, hh: s.zoom, hw: s.zoom * aspect, w, h };
    }
    const az = s.az * D2R, el = Math.max(-89.9, Math.min(89.9, s.el)) * D2R;
    const t = [Math.sin(az) * Math.cos(el), Math.cos(az) * Math.cos(el), Math.sin(el)];  // toward the viewer
    const f = mulS(t, -1);
    // Looking straight down: the bearing is what is "behind" the viewer, so up is away from it.
    let right = cross(f, [0, 0, 1]);
    right = Math.hypot(...right) < 1e-3 ? [-Math.cos(az), Math.sin(az), 0] : unit(right);
    const up = cross(right, f);
    if (s.ortho) {
      const eye = add(s.target, mulS(t, 200));
      return { vp: matMul(orthographic(s.half * aspect, s.half, 1, 400), viewMatrix(eye, right, up, f)), right, up, forward: f, eye, ortho: true, hh: s.half, hw: s.half * aspect, w, h };
    }
    const eye = add(s.target, mulS(t, s.dist));
    return { vp: matMul(perspective(FOVY, aspect, Math.max(0.05, s.dist / 200), s.dist * 20 + 200), viewMatrix(eye, right, up, f)), right, up, forward: f, eye, ortho: false, w, h };
  }

  /** Where a point is on the canvas (CSS pixels), or null if behind the viewer. */
  project(cam, p) {
    const m = cam.vp;
    const x = m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12], y = m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13], w = m[3] * p[0] + m[7] * p[1] + m[11] * p[2] + m[15];
    if (w <= 1e-6) return null;
    return [(x / w + 1) * 0.5 * cam.w, (1 - y / w) * 0.5 * cam.h];
  }

  /** The ray through a canvas position: { origin, dir }. */
  ray(x, y) {
    const cam = this.camera();
    const nx = (2 * x) / cam.w - 1, ny = 1 - (2 * y) / cam.h;
    if (cam.ortho) return { origin: add(cam.eye, add(mulS(cam.right, nx * cam.hw), mulS(cam.up, ny * cam.hh))), dir: cam.forward };
    const th = Math.tan(FOVY / 2);
    return { origin: cam.eye, dir: unit(add(cam.forward, add(mulS(cam.right, nx * th * (cam.w / cam.h)), mulS(cam.up, ny * th)))) };
  }

  /** The cuts in force, as the service's pick takes them. */
  cuts() {
    const s = this.state;
    const z = s.zMin == null && s.zMax == null ? undefined : [s.zMin ?? -1e9, s.zMax ?? 1e9];
    const slab = s.kind === "section" && s.section ? { origin: s.section.origin, normal: s.section.n, half: s.section.half } : undefined;
    return { z, slab };
  }

  /** Frames a sphere (keeping the direction of view). */
  frame(centre, radius) {
    const s = this.state;
    if (s.kind === "section" && s.section) {
      const q = sub(centre, s.section.origin);
      this.animate({ centre: [dot(q, s.section.u), dot(q, s.section.v)], zoom: Math.max(0.3, radius * 1.2) });
    } else {
      this.animate({ target: centre, dist: Math.max(1.5, radius / Math.sin(FOVY / 2) * 1.1), half: Math.max(0.5, radius * 1.2) });
    }
  }

  /**
   * Goes to a view smoothly, so it is seen where it went: the camera's
   * numbers in `patch` (target, az, el, dist, half, centre, zoom) are eased
   * to; anything else in it is set at once. A drag or a wheel stops it.
   */
  animate(patch, ms = 450) {
    const s = this.state, from = {}, to = {}, rest = {};
    for (const [k, v] of Object.entries(patch)) {
      if (["target", "az", "el", "dist", "half", "centre", "zoom"].includes(k) && v != null) { from[k] = structuredClone(s[k]); to[k] = v; }
      else rest[k] = v;
    }
    // A bearing turns the short way round.
    if ("az" in to) to.az = from.az + ((((to.az - from.az) % 360) + 540) % 360 - 180);
    if (Object.keys(rest).length) this.set(rest, true);
    cancelAnimationFrame(this.anim);
    if (!Object.keys(to).length) return this.onChange?.();
    const t0 = performance.now(), scaled = new Set(["dist", "half", "zoom"]);
    const step = (now) => {
      const x = Math.min(1, (now - t0) / ms), e = x < 0.5 ? 4 * x * x * x : 1 - Math.pow(-2 * x + 2, 3) / 2;
      const p = {};
      for (const k of Object.keys(to)) {
        const a = from[k], b = to[k];
        p[k] = Array.isArray(a) ? a.map((v, i) => v + (b[i] - v) * e) : scaled.has(k) ? a * Math.pow(b / a, e) : a + (b - a) * e;
      }
      this.set(p, true);
      if (x < 1) this.anim = requestAnimationFrame(step);
      else this.onChange?.();
    };
    this.anim = requestAnimationFrame(step);
  }

  bind() {
    const c = this.canvas;
    let drag = null;
    c.addEventListener("contextmenu", (e) => e.preventDefault());
    c.addEventListener("pointerdown", (e) => {
      try { c.setPointerCapture(e.pointerId); } catch (_) {}
      cancelAnimationFrame(this.anim);
      // Shift-drag marks a window of points; right- or alt-drag pans (a section pans with any drag).
      drag = { x: e.clientX, y: e.clientY, select: e.shiftKey && e.button === 0, pan: e.button === 2 || e.altKey || this.state.kind === "section", moved: false, s: this.get() };
    });
    c.addEventListener("pointermove", (e) => {
      if (!drag) return;
      const dx = e.clientX - drag.x, dy = e.clientY - drag.y;
      if (Math.abs(dx) + Math.abs(dy) > 3) drag.moved = true;
      if (!drag.moved) return;
      if (drag.select) {
        const r = c.getBoundingClientRect();
        Object.assign(this.band.style, { display: "block", left: `${Math.min(drag.x, e.clientX) - r.left}px`, top: `${Math.min(drag.y, e.clientY) - r.top}px`, width: `${Math.abs(dx)}px`, height: `${Math.abs(dy)}px` });
        return;
      }
      const s0 = drag.s, cam = this.camera();
      if (s0.kind === "section" && s0.section) {
        const k = (2 * s0.zoom) / cam.h;
        this.set({ centre: [s0.centre[0] - dx * k, s0.centre[1] + dy * k] }, true);
      } else if (drag.pan) {
        const k = s0.ortho ? (2 * s0.half) / cam.h : (2 * s0.dist * Math.tan(FOVY / 2)) / cam.h;
        this.set({ target: add(s0.target, add(mulS(cam.right, -dx * k), mulS(cam.up, dy * k))) }, true);
      } else {
        this.set({ az: s0.az - dx * 0.35, el: Math.max(-89.9, Math.min(89.9, s0.el + dy * 0.35)) }, true);
      }
    });
    c.addEventListener("pointerup", (e) => {
      const d = drag;
      drag = null;
      if (!d) return;
      const r = c.getBoundingClientRect();
      if (d.select) {
        this.band.style.display = "none";
        if (!d.moved) return;
        // The window, as the view projects it: what the service needs to find the points in it.
        const cam = this.camera(), nx = (x) => (2 * (x - r.left)) / cam.w - 1, ny = (y) => 1 - (2 * (y - r.top)) / cam.h;
        this.onSelect?.({ vp: Array.from(cam.vp), rect: [nx(d.x), ny(d.y), nx(e.clientX), ny(e.clientY)], eye: cam.eye, forward: cam.forward });
        return;
      }
      if (d.moved) this.onChange?.();
      else if (e.button === 0) this.onPick?.(this.ray(e.clientX - r.left, e.clientY - r.top), e);
      // A right-click (not a pan): the host's menu for what is under the cursor.
      else if (e.button === 2) this.onContext?.(this.ray(e.clientX - r.left, e.clientY - r.top), e);
    });
    c.addEventListener("wheel", (e) => {
      e.preventDefault();
      cancelAnimationFrame(this.anim);
      const f = Math.exp(e.deltaY * 0.0015), s = this.state;
      if (s.kind === "section" && s.section) {
        // Zoom about the cursor.
        const r = c.getBoundingClientRect(), cam = this.camera();
        const ux = ((e.clientX - r.left) / cam.w * 2 - 1) * cam.hw, vy = (1 - (e.clientY - r.top) / cam.h * 2) * cam.hh;
        this.set({ zoom: s.zoom * f, centre: [s.centre[0] + ux * (1 - f), s.centre[1] + vy * (1 - f)] }, true);
      } else {
        // Zoom toward what is under the cursor, so it stays there: the scan point
        // it is on (asked of the host, once per place the cursor rests), or until
        // that is known, the point at the depth of the view's target.
        const r = c.getBoundingClientRect(), x = e.clientX - r.left, y = e.clientY - r.top, ray = this.ray(x, y), now = performance.now();
        const z = this.zoomAt;
        if (!z || Math.hypot(z.x - x, z.y - y) > 12 || now - z.t > 1500) {
          const fresh = (this.zoomAt = { x, y, t: now, p: null });
          this.onDepth?.(ray).then((p) => { if (p) fresh.p = p; }).catch(() => {});
        } else z.t = now;
        const cam = this.camera(), depth = dot(sub(s.target, cam.eye), cam.forward);
        const p = this.zoomAt.p ?? add(ray.origin, mulS(ray.dir, cam.ortho ? depth : depth / Math.max(0.2, dot(ray.dir, cam.forward))));
        const target = add(p, mulS(sub(s.target, p), f));
        this.set(s.ortho ? { half: s.half * f, target } : { dist: Math.max(0.2, s.dist * f), target }, true);
      }
      clearTimeout(this.wheelTimer);
      this.wheelTimer = setTimeout(() => this.onChange?.(), 250);
    }, { passive: false });
  }

  // ----- drawing ------------------------------------------------------------------

  draw() {
    this.dirty = false;
    const gl = this.gl, c = this.canvas, dpr = window.devicePixelRatio || 1;
    const w = Math.round(c.clientWidth * dpr), h = Math.round(c.clientHeight * dpr);
    if (!w || !h) return;
    if (c.width !== w || c.height !== h) { c.width = w; c.height = h; }
    gl.viewport(0, 0, w, h);
    gl.clearColor(0.085, 0.09, 0.105, 1);
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
    gl.enable(gl.DEPTH_TEST);
    gl.depthFunc(gl.LEQUAL);
    const s = this.state, cam = this.camera();
    const z = [s.zMin == null ? -1e9 : s.zMin + this.zOffset, s.zMax == null ? 1e9 : s.zMax + this.zOffset];
    const sec = s.kind === "section" ? s.section : null;
    const plane = sec ? [...sec.n, dot(sec.n, sec.origin)] : [0, 0, 1, 0];
    const cuts = (u) => {
      gl.uniformMatrix4fv(u.u_vp, false, cam.vp);
      gl.uniform2f(u.u_z, z[0], z[1]);
      gl.uniform4f(u.u_plane, ...plane);
      gl.uniform1f(u.u_half, sec ? sec.half : 0);
    };
    const cloud = s.kind === "section" ? this.clouds.slab || this.clouds.thin : this.clouds.thin;
    if (s.layers.scan && cloud) {
      const u = this.points.u;
      gl.useProgram(this.points.p);
      cuts(u);
      gl.uniform1f(u.u_size, s.pointSize * dpr);
      gl.uniform1f(u.u_persp, cam.ortho ? 0 : 1);
      gl.uniform1i(u.u_mode, Math.max(0, COLOURS.indexOf(s.colour)));
      gl.uniform3f(u.u_toEye, -cam.forward[0], -cam.forward[1], -cam.forward[2]);
      gl.uniform1f(u.u_floor, this.floorZ);
      gl.bindVertexArray(cloud.vao);
      gl.drawArrays(gl.POINTS, 0, cloud.count);
    }
    // Highlighted sets of points (selections), over the others; shown whether or not the scan is.
    if (this.highlights.size) {
      const u = this.points.u;
      gl.useProgram(this.points.p);
      cuts(u);
      gl.uniform1f(u.u_persp, cam.ortho ? 0 : 1);
      gl.uniform1i(u.u_mode, 4);
      gl.uniform1f(u.u_size, (s.pointSize + 1) * dpr);
      for (const h of this.highlights.values()) { gl.bindVertexArray(h.vao); gl.drawArrays(gl.POINTS, 0, h.count); }
    }
    gl.useProgram(this.flat.p);
    cuts(this.flat.u);
    // In a section the model is drawn over the slab's points: it is what they are read against.
    if (sec) gl.disable(gl.DEPTH_TEST);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
    if (s.layers.model && this.tris) {
      // Faces are translucent: seen through to the points, not hiding each other.
      gl.depthMask(false);
      gl.bindVertexArray(this.tris.vao);
      gl.drawArrays(gl.TRIANGLES, 0, this.tris.count);
      gl.depthMask(true);
    }
    if (s.layers.model && this.lines) {
      gl.bindVertexArray(this.lines.vao);
      gl.drawArrays(gl.LINES, 0, this.lines.count);
    }
    if (s.layers.marks && this.marks) {
      gl.disable(gl.DEPTH_TEST);
      gl.uniform1f(this.flat.u.u_half, 0);
      gl.uniform2f(this.flat.u.u_z, -1e9, 1e9);
      gl.bindVertexArray(this.marks.vao);
      gl.drawArrays(gl.LINES, 0, this.marks.count);
    }
    gl.disable(gl.BLEND);
    gl.bindVertexArray(null);
    // Labels: HTML over the canvas, where their places project to.
    for (const l of this.labels) {
      const el = this.labelEls.get(l.id);
      const p = s.layers.issues ? this.project(cam, l.pos) : null;
      const cut = sec && Math.abs(dot(sec.n, sub(l.pos, sec.origin))) > Math.max(sec.half, 0.3);
      if (!p || cut || p[0] < -40 || p[1] < -20 || p[0] > cam.w + 40 || p[1] > cam.h + 20) { el.style.display = "none"; continue; }
      el.style.display = "";
      el.style.transform = `translate(${p[0].toFixed(1)}px, ${p[1].toFixed(1)}px)`;
    }
    // What the host draws over the view to match it (a compass).
    this.onDraw?.(cam);
  }

  /** The view as a PNG data URL (as drawn, without the labels). */
  png() {
    this.draw();
    return this.canvas.toDataURL("image/png");
  }
}
